import AppKit
import Carbon
import SwiftUI
import WebKit

extension Notification.Name {
    static let focusOverlay = Notification.Name("QCalc.focusOverlay")
}

private func isCommandVPasteKey(_ event: NSEvent) -> Bool {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.contains(.command), !flags.contains(.option), !flags.contains(.control) else {
        return false
    }
    if event.keyCode == UInt16(kVK_ANSI_V) { return true }
    return event.charactersIgnoringModifiers?.lowercased() == "v"
}

private func isCommandCommaKey(_ event: NSEvent) -> Bool {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    return flags.intersection([.command, .option, .control, .shift]) == .command
        && event.charactersIgnoringModifiers == ","
}

private func copyToPasteboard(_ text: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
}

private func commandShiftHeld() -> Bool {
    let flags = NSEvent.modifierFlags.intersection(.deviceIndependentFlagsMask)
    return flags.contains(.command) && flags.contains(.shift)
}

final class OverlayPanel: NSPanel {
    var onEscape: (() -> Void)?
    var onPaste: (() -> Void)?
    var ignoreResignKey = false

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onEscape?()
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == UInt16(kVK_Escape) {
            onEscape?()
            return
        }
        super.keyDown(with: event)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if isCommandVPasteKey(event) {
            onPaste?()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func resignKey() {
        super.resignKey()
        if ignoreResignKey || commandShiftHeld() { return }
        onEscape?()
    }
}

final class OverlayWebView: WKWebView {
    var onPaste: (() -> Void)?

    override var needsPanelToBecomeKey: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    @objc func paste(_ sender: Any?) {
        onPaste?()
    }

    @objc func pasteAsPlainText(_ sender: Any?) {
        onPaste?()
    }

    @objc func pasteAndMatchStyle(_ sender: Any?) {
        onPaste?()
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if isCommandVPasteKey(event) {
            onPaste?()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func doCommand(by selector: Selector) {
        switch NSStringFromSelector(selector) {
        case "paste:", "pasteAsPlainText:", "pasteAndMatchStyle:":
            onPaste?()
        default:
            super.doCommand(by: selector)
        }
    }
}

final class OverlayController: NSObject, WKNavigationDelegate, WKScriptMessageHandler, WKScriptMessageHandlerWithReply {
    private var panel: OverlayPanel?
    private var web: OverlayWebView?
    private var fallback: NSView?
    private var escapeMonitor: Any?
    private var dragMonitor: Any?
    private var clickAwayMonitor: Any?
    private var clickAwayLocalMonitor: Any?
    private var webReady = false
    private var triedBundle = false
    private var triedDevServer = false
    private let overlayWidth: CGFloat = 680
    private let overlayMinHeight: CGFloat = 72
    private let overlayMaxHeight: CGFloat = 560
    // distance from the overlay top to the composer, so history grows up and graphs grow down
    private var sizeAnchorTop: CGFloat = 0
    private var settingsObserver: NSObjectProtocol?
    private var lastPasteAt: TimeInterval = 0
    private var pendingFirstRun = false
    private var showCount = 0
    private var revealedShow = 0
    var onSettings: (() -> Void)?
    // off the main thread so a slow soulver evaluation never blocks typing
    private let soulverQueue = DispatchQueue(label: "qcalc.soulver", qos: .userInitiated)

    deinit {
        for monitor in [escapeMonitor, dragMonitor, clickAwayMonitor, clickAwayLocalMonitor].compactMap({ $0 }) {
            NSEvent.removeMonitor(monitor)
        }
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
    }

    func preload() {
        if panel == nil { build() }
        SoulverEval.warm()
    }

    func toggle() {
        if panel?.isVisible == true {
            hide()
            return
        }
        show()
    }

    func hide() {
        guard panel?.isVisible == true else { return }
        notifyWebWillHide()
        panel?.ignoreResignKey = true
        panel?.orderOut(nil)
        panel?.alphaValue = 1
    }

    func show() {
        if panel == nil { build() }
        if panel?.contentView === fallback, let web, webReady {
            panel?.contentView = web
        }
        sizeAnchorTop = 0
        applySize(height: overlayMinHeight, anchorTop: 0)
        position()
        panel?.ignoreResignKey = true
        showCount += 1
        let token = showCount
        // clear until the page has sized itself, so a history tape open on show doesn't pop in
        panel?.alphaValue = 0
        panel?.orderFrontRegardless()
        panel?.makeKeyAndOrderFront(nil)
        panel?.makeFirstResponder(web)
        resetAndFocus { [weak self] in self?.reveal(token) }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in self?.reveal(token) }
        DispatchQueue.main.async { [weak self] in
            self?.focusInput()
            self?.panel?.ignoreResignKey = false
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in self?.focusInput() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in self?.focusInput() }
        NotificationCenter.default.post(name: .focusOverlay, object: nil)
    }

    func showFirstRun() {
        guard webReady else {
            pendingFirstRun = true
            return
        }
        show()
        web?.evaluateJavaScript("window.__QCALC_FIRST_RUN = true; if (window.__qcalcFirstRun) window.__qcalcFirstRun();")
    }

    func showTips() {
        show()
        web?.evaluateJavaScript("if (window.__qcalcShowTips) window.__qcalcShowTips();")
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        if message.name == "soulver" {
            pushSoulverResult(soulverPayload(from: message.body))
            return
        }
        guard message.name == "qcalc" else { return }
        if let body = message.body as? String {
            handleMessage(body)
            return
        }
        if let dict = message.body as? [String: Any] {
            switch dict["type"] as? String {
            case "size":
                if let height = doubleValue(dict["height"]) {
                    applySize(
                        height: CGFloat(height),
                        anchorTop: CGFloat(doubleValue(dict["anchorTop"]) ?? 0)
                    )
                }
            case "dismiss":
                hide()
            case "copy":
                if let text = dict["text"] as? String {
                    copyToPasteboard(text)
                }
            case "drag":
                beginWindowDrag()
            case "settings":
                applyWebSettings(dict)
            case "onboarding":
                applyWebOnboarding(dict)
            case "eval":
                pushSoulverResult(soulverPayload(from: dict))
            default:
                break
            }
        }
    }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage,
        replyHandler: @escaping (Any?, String?) -> Void
    ) {
        guard message.name == "soulver" else {
            replyHandler(nil, "unknown handler")
            return
        }
        let body = message.body
        soulverQueue.async { [self] in
            let payload = soulverPayload(from: body)
            DispatchQueue.main.async { replyHandler(payload, nil) }
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        webReady = true
        disableWebViewScrolling(webView)
        if pendingFirstRun {
            pendingFirstRun = false
            DispatchQueue.main.async { [weak self] in self?.showFirstRun() }
        }
        focusInput()
        for delay in [0.05, 0.12, 0.3] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                self?.focusInput()
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        recover(from: webView, after: error)
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        recover(from: webView, after: error)
    }

    private func build() {
        let panel = OverlayPanel(
            contentRect: NSRect(x: 0, y: 0, width: overlayWidth, height: overlayMinHeight),
            styleMask: [.borderless, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.onEscape = { [weak self] in self?.hide() }
        panel.onPaste = { [weak self] in self?.pasteIntoWeb() }
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.becomesKeyOnlyIfNeeded = false
        installEscapeMonitor()
        installDragMonitor()
        installClickAwayMonitors()
        observeSettings()

        let config = WKWebViewConfiguration()
        // wiped every launch, so state that must survive (settings, onboarding) is kept in AppSettings
        config.websiteDataStore = WKWebsiteDataStore.nonPersistent()
        config.userContentController.add(self, name: "qcalc")
        config.userContentController.addScriptMessageHandler(self, contentWorld: .page, name: "soulver")
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        config.setValue(true, forKey: "allowUniversalAccessFromFileURLs")
        let settings = settingsJavaScriptObject()
        let theme = AppSettings.shared.theme
        let onboarding = AppSettings.shared.onboardingJSON()
        let boot = WKUserScript(
            source: """
            window.__QCALC_NATIVE = true;
            window.__QCALC_KEYS = [];
            window.__QCALC_HELD = '';
            window.__QCALC_META = false;
            window.__QCALC_SETTINGS = \(settings);
            window.__QCALC_ONBOARDING = \(onboarding);
            document.documentElement.dataset.theme = "\(theme)";
            window.__qcalcNativeResult = window.__qcalcNativeResult || function (reply) {
              window.dispatchEvent(new CustomEvent('qcalc-soulver', { detail: reply }));
            };
            window.__qcalcSelectedText = function () {
              var el = document.querySelector('.quick-plain');
              if (el && el.selectionStart != null && el.selectionEnd > el.selectionStart) {
                return el.value.slice(el.selectionStart, el.selectionEnd);
              }
              var ae = document.activeElement;
              if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA') && ae.selectionEnd > ae.selectionStart) {
                return ae.value.slice(ae.selectionStart, ae.selectionEnd);
              }
              var sel = window.getSelection();
              return (sel && sel.toString()) || '';
            };
            window.__qcalcRememberText = function () {
              var live = window.__qcalcSelectedText();
              if (live) window.__QCALC_HELD = live;
              else if (!window.__QCALC_META) window.__QCALC_HELD = '';
            };
            document.documentElement.classList.add('quick-native');
            document.documentElement.style.overflow = 'hidden';
            window.addEventListener('keydown', function (e) {
              if (e.key === 'Escape' || e.keyCode === 27) {
                e.preventDefault();
                e.stopPropagation();
                if (window.__qcalcEscape && window.__qcalcEscape()) return;
                try { window.webkit.messageHandlers.qcalc.postMessage({ type: 'dismiss' }); } catch (err) {}
                return;
              }
              if (e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey && (e.key === 'c' || e.key === 'C' || e.keyCode === 67)) {
                e.preventDefault();
              }
              if (e.key === 'Meta' || e.key === 'Control') window.__QCALC_META = true;
              if (e.metaKey || e.ctrlKey) window.__qcalcRememberText();
              if ((e.metaKey || e.ctrlKey) && !e.altKey && (e.key === 'v' || e.key === 'V' || e.keyCode === 86)) {
                e.preventDefault();
                e.stopImmediatePropagation();
                return;
              }
              if (e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 'c' || e.key === 'C' || e.keyCode === 67)) {
                var text = window.__qcalcSelectedText() || window.__QCALC_HELD || '';
                if (text) {
                  e.preventDefault();
                  e.stopImmediatePropagation();
                  try { window.webkit.messageHandlers.qcalc.postMessage({ type: 'copy', text: text }); } catch (err) {}
                  return;
                }
              }
              var field = document.querySelector('.quick-plain');
              if (field && document.activeElement !== field && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
                window.__QCALC_KEYS.push(e.key);
              }
            }, true);
            window.addEventListener('keyup', function (e) {
              if (e.key === 'Meta' || e.key === 'Control') window.__QCALC_META = false;
              window.__qcalcRememberText();
            }, true);
            document.addEventListener('select', window.__qcalcRememberText, true);
            document.addEventListener('mouseup', window.__qcalcRememberText, true);
            window.addEventListener('copy', function (e) {
              var text = window.__qcalcSelectedText() || window.__QCALC_HELD || '';
              if (!text) return;
              e.preventDefault();
              e.stopImmediatePropagation();
              if (e.clipboardData) e.clipboardData.setData('text/plain', text);
              try { window.webkit.messageHandlers.qcalc.postMessage({ type: 'copy', text: text }); } catch (err) {}
            }, true);
            window.addEventListener('wheel', function (e) {
              var t = e.target;
              if (t && t.closest && t.closest('.tape')) return;
              e.preventDefault();
            }, { passive: false, capture: true });
            """,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        )
        config.userContentController.addUserScript(boot)
        let web = OverlayWebView(frame: NSRect(x: 0, y: 0, width: overlayWidth, height: overlayMinHeight), configuration: config)
        web.navigationDelegate = self
        web.autoresizingMask = [.width, .height]
        web.setValue(false, forKey: "drawsBackground")
        if #available(macOS 12.0, *) {
            web.underPageBackgroundColor = .clear
        }
        web.wantsLayer = true
        web.layer?.isOpaque = false
        web.layer?.backgroundColor = NSColor.clear.cgColor
        web.layer?.cornerRadius = 12
        web.layer?.masksToBounds = true
        #if DEBUG
        if #available(macOS 13.3, *) { web.isInspectable = true }
        #endif
        panel.contentView = web
        self.web = web
        self.panel = panel
        web.onPaste = { [weak self] in self?.pasteIntoWeb() }
        applyWebAppearance(web)
        loadQuickCalc(web)
    }

    private func loadQuickCalc(_ web: WKWebView) {
        if let bundled = bundledQuickURL() {
            triedBundle = true
            web.loadFileURL(bundled, allowingReadAccessTo: bundled.deletingLastPathComponent())
            return
        }
        loadDevServer(web)
    }

    private func loadDevServer(_ web: WKWebView) {
        triedDevServer = true
        let url = URL(string: "http://127.0.0.1:5173/quick.html")!
        web.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 2))
    }

    private func recover(from webView: WKWebView, after error: Error) {
        if (error as NSError).code == NSURLErrorCancelled { return }
        if !triedBundle, let bundled = bundledQuickURL() {
            triedBundle = true
            webView.loadFileURL(bundled, allowingReadAccessTo: bundled.deletingLastPathComponent())
            return
        }
        if !triedDevServer {
            loadDevServer(webView)
            return
        }
        loadFallback()
    }

    private func loadFallback() {
        if fallback == nil {
            let root = OverlayView(onDismiss: { [weak self] in self?.hide() })
            let host = NSHostingView(rootView: root)
            host.frame = panel?.contentView?.bounds ?? .zero
            host.autoresizingMask = [.width, .height]
            fallback = host
        }
        if let fallback, panel?.contentView !== fallback {
            panel?.contentView = fallback
            applySize(height: overlayMinHeight, anchorTop: 0)
        }
    }

    private func bundledQuickURL() -> URL? {
        guard let dir = Bundle.main.url(forResource: "web", withExtension: nil) else { return nil }
        let quick = dir.appendingPathComponent("quick.html")
        if FileManager.default.fileExists(atPath: quick.path) { return quick }
        let index = dir.appendingPathComponent("index.html")
        return FileManager.default.fileExists(atPath: index.path) ? index : nil
    }

    private func handleMessage(_ body: String) {
        if body == "dismiss" {
            hide()
            return
        }
        if body == "drag" {
            beginWindowDrag()
            return
        }
        if body.hasPrefix("copy:") {
            copyToPasteboard(String(body.dropFirst("copy:".count)))
            return
        }
        if body.hasPrefix("height:") {
            let raw = Double(body.dropFirst("height:".count)) ?? Double(overlayMinHeight)
            applySize(height: CGFloat(raw), anchorTop: sizeAnchorTop)
            return
        }
        if body.hasPrefix("sigFigs:") {
            let n = Int(body.dropFirst("sigFigs:".count)) ?? AppSettings.defaultSigFigs
            AppSettings.shared.setSignificantFigures(n, notifyWeb: false)
            return
        }
        if body.hasPrefix("eval:") {
            let json = String(body.dropFirst("eval:".count))
            pushSoulverResult(soulverPayload(from: json))
            return
        }
        if body.hasPrefix("{"), let data = body.data(using: .utf8),
           let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           dict["type"] as? String == "eval" || dict["expr"] != nil {
            pushSoulverResult(soulverPayload(from: dict))
        }
    }

    private func pasteIntoWeb() {
        let now = ProcessInfo.processInfo.systemUptime
        guard now - lastPasteAt > 0.05 else { return }
        lastPasteAt = now

        panel?.ignoreResignKey = true
        let text = NSPasteboard.general.string(forType: .string) ?? ""
        if !text.isEmpty, let encoded = jsonStringLiteral(text) {
            web?.evaluateJavaScript("window.__qcalcPaste && window.__qcalcPaste(\(encoded));")
        }
        panel?.makeKeyAndOrderFront(nil)
        panel?.makeFirstResponder(web)
        focusInput()
        DispatchQueue.main.async { [weak self] in
            guard let self, self.panel?.isVisible == true else { return }
            self.panel?.makeKeyAndOrderFront(nil)
            self.panel?.makeFirstResponder(self.web)
            self.focusInput()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            self?.panel?.ignoreResignKey = false
        }
    }

    private func jsonStringLiteral(_ string: String) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: [string], options: []),
              let json = String(data: data, encoding: .utf8),
              json.count >= 2
        else { return nil }
        return String(json.dropFirst().dropLast())
    }

    private func notifyWebWillHide() {
        web?.evaluateJavaScript("if (window.__qcalcWillHide) window.__qcalcWillHide();")
    }

    private func reveal(_ token: Int) {
        guard token == showCount, token != revealedShow, let panel, panel.isVisible else { return }
        revealedShow = token
        guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else {
            panel.alphaValue = 1
            return
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.11
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            panel.animator().alphaValue = 1
        }
    }

    // the page posts its size while this script runs, so `done` comes after the panel has its height
    private func resetAndFocus(_ done: @escaping () -> Void) {
        guard let web else {
            done()
            return
        }
        panel?.makeFirstResponder(web)
        web.evaluateJavaScript("""
        (function () {
          if (window.__qcalcReset) window.__qcalcReset();
          var el = document.querySelector('.quick-plain');
          if (el) { el.focus(); if (window.__qcalcFocus) window.__qcalcFocus(); }
          if (window.__qcalcSize) window.__qcalcSize();
        })()
        """) { _, _ in done() }
    }

    private func focusInput() {
        guard let web else { return }
        panel?.makeFirstResponder(web)
        web.evaluateJavaScript("""
        (function () {
          var el = document.querySelector('.quick-plain');
          if (el) { el.focus(); if (window.__qcalcFocus) window.__qcalcFocus(); }
        })()
        """)
    }

    private func applySize(height: CGFloat, anchorTop: CGFloat? = nil) {
        guard let panel else { return }
        let h = min(max(height.rounded(.up), overlayMinHeight), overlayMaxHeight)
        let nextAnchor = min(max(anchorTop ?? sizeAnchorTop, 0), max(0, h - overlayMinHeight))
        var frame = panel.frame
        let composerTop = frame.maxY - sizeAnchorTop
        frame.size = NSSize(width: overlayWidth, height: h)
        frame.origin.y = composerTop + nextAnchor - h
        if let screen = panel.screen?.visibleFrame ?? NSScreen.main?.visibleFrame {
            if frame.maxY > screen.maxY {
                frame.origin.y = screen.maxY - h
            }
        }
        sizeAnchorTop = nextAnchor
        panel.setFrame(frame, display: true)
    }

    private func disableWebViewScrolling(_ web: WKWebView) {
        func walk(_ view: NSView) {
            if let sv = view as? NSScrollView {
                sv.hasVerticalScroller = false
                sv.hasHorizontalScroller = false
                sv.verticalScrollElasticity = .none
                sv.horizontalScrollElasticity = .none
            }
            for child in view.subviews { walk(child) }
        }
        walk(web)
    }

    private func beginWindowDrag() {
        guard let panel, let event = NSApp.currentEvent else { return }
        if event.type == .leftMouseDown || event.type == .leftMouseDragged {
            dragAndRemember(panel, event)
        }
    }

    private func installEscapeMonitor() {
        guard escapeMonitor == nil else { return }
        escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, self.panel?.isVisible == true else { return event }
            if event.keyCode == UInt16(kVK_Escape) {
                self.escape()
                return nil
            }
            if isCommandVPasteKey(event) {
                self.pasteIntoWeb()
                return nil
            }
            if isCommandCommaKey(event), event.window === self.panel {
                self.onSettings?()
                return nil
            }
            return event
        }
    }

    // the page clears the tape or the input first; only an esc with nothing left hides
    private func escape() {
        guard let web, webReady, panel?.contentView === web else {
            hide()
            return
        }
        web.evaluateJavaScript("window.__qcalcEscape ? window.__qcalcEscape() : false") { [weak self] result, _ in
            if (result as? Bool) != true { self?.hide() }
        }
    }

    private func installDragMonitor() {
        guard dragMonitor == nil else { return }
        dragMonitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window === panel else {
                return event
            }
            let p = event.locationInWindow
            let size = panel.frame.size
            let edge: CGFloat = 14
            let alongTop = p.y >= size.height - edge
            let topCorner = p.y >= size.height - 44 && (p.x <= edge || p.x >= size.width - edge)
            if alongTop || topCorner {
                if event.clickCount == 2 {
                    self.forgetPosition()
                    return nil
                }
                self.dragAndRemember(panel, event)
                return nil
            }
            return event
        }
    }

    private func installClickAwayMonitors() {
        guard clickAwayMonitor == nil else { return }
        clickAwayMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            guard self?.panel?.isVisible == true else { return }
            self?.hide()
        }
        clickAwayLocalMonitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window !== panel else {
                return event
            }
            self.hide()
            return event
        }
    }

    private func position() {
        guard let panel, let main = NSScreen.main else { return }
        let screen = main.visibleFrame
        let size = panel.frame.size
        if let saved = savedComposerTop(on: main) {
            // clamped so a resolution change can't leave it off screen
            let x = min(max(saved.x, screen.minX), screen.maxX - size.width)
            let top = min(max(saved.y, screen.minY + size.height), screen.maxY)
            panel.setFrameOrigin(NSPoint(x: x, y: top - size.height))
            return
        }
        let x = screen.midX - size.width / 2
        let y = screen.minY + screen.height * 0.72 - size.height
        panel.setFrameOrigin(NSPoint(x: x, y: y))
    }

    private static let positionsKey = "overlayPositions"

    private func screenKey(_ screen: NSScreen) -> String? {
        (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.stringValue
    }

    // stored per display, relative to its visible frame, as the composer's top-left corner
    private func savedComposerTop(on screen: NSScreen) -> NSPoint? {
        guard let key = screenKey(screen),
              let all = UserDefaults.standard.dictionary(forKey: Self.positionsKey),
              let pair = all[key] as? [Double], pair.count == 2 else { return nil }
        let origin = screen.visibleFrame.origin
        return NSPoint(x: origin.x + pair[0], y: origin.y + pair[1])
    }

    // performDrag runs until mouse up; a click that didn't move it saves nothing
    private func dragAndRemember(_ panel: OverlayPanel, _ event: NSEvent) {
        let before = panel.frame.origin
        panel.performDrag(with: event)
        if panel.frame.origin != before { rememberPosition() }
    }

    private func rememberPosition() {
        guard let panel, let screen = panel.screen, let key = screenKey(screen) else { return }
        let origin = screen.visibleFrame.origin
        let top = panel.frame.maxY - sizeAnchorTop
        var all = UserDefaults.standard.dictionary(forKey: Self.positionsKey) ?? [:]
        all[key] = [Double(panel.frame.minX - origin.x), Double(top - origin.y)]
        UserDefaults.standard.set(all, forKey: Self.positionsKey)
    }

    private func forgetPosition() {
        guard let panel, let screen = panel.screen, let key = screenKey(screen) else { return }
        var all = UserDefaults.standard.dictionary(forKey: Self.positionsKey) ?? [:]
        all.removeValue(forKey: key)
        UserDefaults.standard.set(all, forKey: Self.positionsKey)
        let height = panel.frame.height
        let anchor = sizeAnchorTop
        let visible = screen.visibleFrame
        let top = visible.minY + visible.height * 0.72
        panel.setFrameOrigin(NSPoint(x: visible.midX - panel.frame.width / 2, y: top + anchor - height))
    }

    private func observeSettings() {
        guard settingsObserver == nil else { return }
        settingsObserver = NotificationCenter.default.addObserver(
            forName: .qcalcSettingsChanged,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.applyWebAppearance()
            self?.pushSettingsToWeb()
        }
    }

    private func pushSettingsToWeb() {
        let payload = settingsJavaScriptObject()
        web?.evaluateJavaScript(
            "window.__QCALC_SETTINGS = \(payload); document.documentElement.dataset.theme = \"\(AppSettings.shared.theme)\"; if (window.__qcalcApplySettings) window.__qcalcApplySettings(\(payload));"
        )
    }

    private func settingsJavaScriptObject() -> String {
        let n = AppSettings.shared.significantFigures
        let d = AppSettings.shared.draftSeconds
        let units = AppSettings.shared.defaultUnitsJSON()
        let form = AppSettings.shared.answerForm
        let insert = AppSettings.shared.historyInsert
        let historyShow = AppSettings.shared.historyShow
        let rationalize = AppSettings.shared.rationalize ? "true" : "false"
        let sigFigMode = AppSettings.shared.sigFigMode ? "true" : "false"
        let keepWords = AppSettings.shared.keepWords ? "true" : "false"
        let theme = AppSettings.shared.theme
        let angle = AppSettings.shared.angleMode
        let fractions = AppSettings.shared.fractionMode ? "true" : "false"
        return "{ sigFigs: \(n), draftSeconds: \(d), defaultUnits: \(units), answerForm: \"\(form)\", historyInsert: \"\(insert)\", historyShow: \"\(historyShow)\", rationalize: \(rationalize), sigFigMode: \(sigFigMode), theme: \"\(theme)\", angleMode: \"\(angle)\", fractionMode: \(fractions), keepWords: \(keepWords), \(hotKeyJavaScriptFields()) }"
    }

    // titles are fixed preset strings, so they need no escaping
    private func hotKeyJavaScriptFields() -> String {
        let title = AppSettings.shared.activeHotKey?.title ?? ""
        let failed = AppSettings.shared.hotKeyFailed ? "true" : "false"
        return "hotkey: \"\(title)\", hotkeyFailed: \(failed)"
    }

    private func applyWebAppearance(_ webView: WKWebView? = nil) {
        let appearance = AppSettings.shared.nsAppearance
        (webView ?? web)?.appearance = appearance
        panel?.appearance = appearance
    }

    private func applyWebSettings(_ dict: [String: Any]) {
        if let n = intValue(dict["sigFigs"]) {
            AppSettings.shared.setSignificantFigures(n, notifyWeb: false)
        }
        if let rationalize = boolValue(dict["rationalize"]) {
            AppSettings.shared.setRationalize(rationalize, notifyWeb: false)
        }
        if let sigFigMode = boolValue(dict["sigFigMode"]) {
            AppSettings.shared.setSigFigMode(sigFigMode, notifyWeb: false)
        }
        if let keepWords = boolValue(dict["keepWords"]) {
            AppSettings.shared.setKeepWords(keepWords, notifyWeb: false)
        }
        if let fractions = boolValue(dict["fractionMode"]) {
            AppSettings.shared.setFractionMode(fractions, notifyWeb: false)
        }
        if let angle = dict["angleMode"] as? String {
            AppSettings.shared.setAngleMode(angle, notifyWeb: false)
        }
        if let form = dict["answerForm"] as? String {
            AppSettings.shared.setAnswerForm(form, notifyWeb: false)
        }
        if let insert = dict["historyInsert"] as? String {
            AppSettings.shared.setHistoryInsert(insert, notifyWeb: false)
        }
        if let historyShow = dict["historyShow"] as? String {
            AppSettings.shared.setHistoryShow(historyShow, notifyWeb: false)
        }
        if let d = intValue(dict["draftSeconds"]) {
            AppSettings.shared.setDraftSeconds(d, notifyWeb: false)
        }
        if let units = dict["defaultUnits"] as? [String: Any] {
            AppSettings.shared.replaceDefaultUnits(units.compactMapValues { $0 as? String }, notifyWeb: false)
        }
        // theme changes the panel's own appearance, so it goes through the full notify
        if let theme = dict["theme"] as? String {
            AppSettings.shared.setTheme(theme, notifyWeb: true)
        }
    }

    private func applyWebOnboarding(_ dict: [String: Any]) {
        var incoming: [String: Int] = [:]
        for key in ["opens", "commits", "hints"] {
            if let n = intValue(dict[key]) { incoming[key] = n }
        }
        if boolValue(dict["done"]) == true { incoming["done"] = 1 }
        AppSettings.shared.mergeOnboarding(incoming)
    }

    private func soulverPayload(from body: Any) -> [String: Any] {
        let dict = dictionary(from: body)
        let id = intValue(dict["id"]) ?? 0
        let expr = dict["expr"] as? String ?? ""
        let sigFigs = intValue(dict["sigFigs"]) ?? AppSettings.shared.significantFigures
        if let answer = SoulverEval.evaluate(
            expr,
            ans: doubleValue(dict["ans"]),
            variables: stringKeyedDoubles(dict["variables"]),
            sigFigs: sigFigs
        ) {
            var payload: [String: Any] = [
                "id": id,
                "expr": expr,
                "display": answer.display,
            ]
            if let n = answer.number, n.isFinite {
                payload["n"] = n
            } else {
                payload["n"] = NSNull()
            }
            return payload
        }
        return ["id": id, "expr": expr, "display": "", "n": NSNull()]
    }

    private func pushSoulverResult(_ payload: [String: Any]) {
        guard JSONSerialization.isValidJSONObject(payload),
              let data = try? JSONSerialization.data(withJSONObject: payload, options: []),
              let json = String(data: data, encoding: .utf8)
        else { return }
        web?.evaluateJavaScript("window.__qcalcNativeResult && window.__qcalcNativeResult(\(json));")
    }

    private func dictionary(from body: Any) -> [String: Any] {
        if let dict = body as? [String: Any] { return dict }
        if let s = body as? String {
            let json = s.hasPrefix("eval:") ? String(s.dropFirst("eval:".count)) : s
            if let data = json.data(using: .utf8),
               let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                return obj
            }
            if !s.isEmpty, s != "eval:" { return ["expr": s] }
        }
        return [:]
    }

    private func boolValue(_ any: Any?) -> Bool? {
        if let b = any as? Bool { return b }
        if let n = any as? NSNumber { return n.boolValue }
        if let s = any as? String {
            if s.caseInsensitiveCompare("true") == .orderedSame || s == "1" { return true }
            if s.caseInsensitiveCompare("false") == .orderedSame || s == "0" { return false }
        }
        return nil
    }

    private func intValue(_ any: Any?) -> Int? {
        if let i = any as? Int { return i }
        if let d = any as? Double { return intFromDouble(d) }
        if let n = any as? NSNumber { return intFromDouble(n.doubleValue) }
        return nil
    }

    private func intFromDouble(_ d: Double) -> Int? {
        guard d.isFinite, d >= Double(Int.min), d <= Double(Int.max) else { return nil }
        return Int(d)
    }

    private func doubleValue(_ any: Any?) -> Double? {
        if any == nil || any is NSNull { return nil }
        if let d = any as? Double { return d.isFinite ? d : nil }
        if let i = any as? Int { return Double(i) }
        if let n = any as? NSNumber {
            let d = n.doubleValue
            return d.isFinite ? d : nil
        }
        return nil
    }

    private func stringKeyedDoubles(_ any: Any?) -> [String: Double] {
        guard let dict = any as? [String: Any] else { return [:] }
        var out: [String: Double] = [:]
        for (key, value) in dict {
            if let n = doubleValue(value) { out[key] = n }
        }
        return out
    }
}

struct OverlayView: View {
    var onDismiss: () -> Void
    @State private var text = ""
    @State private var copied = false
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            TextField("", text: $text, prompt: Text(""))
                .textFieldStyle(.plain)
                .font(.system(size: 22, weight: .regular, design: .default))
                .focused($focused)
                .onSubmit { submit() }
            Text(copied ? "copied" : answer(for: text))
                .font(.system(size: copied ? 13 : 22, weight: .regular, design: .default).monospacedDigit())
                .foregroundStyle(copied ? Color.secondary : Color(red: 0.11, green: 0.48, blue: 0.30))
                .lineLimit(1)
                .frame(minWidth: 72, alignment: .trailing)
                .onTapGesture { copyAnswer() }
        }
        .padding(.horizontal, 18)
        .frame(maxWidth: .infinity, minHeight: 72, maxHeight: 72)
        .background(Color(nsColor: .windowBackgroundColor).opacity(0.9))
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onAppear { focused = true }
        .onReceive(NotificationCenter.default.publisher(for: .focusOverlay)) { _ in
            focused = true
        }
        .onExitCommand { onDismiss() }
    }

    private func answer(for text: String) -> String {
        if let soulver = SoulverEval.evaluate(text) { return soulver.display }
        if let v = MathEval.evaluate(text) { return MathEval.format(v) }
        return ""
    }

    private func copyAnswer() {
        let shown = answer(for: text)
        guard !shown.isEmpty else { return }
        copyToPasteboard(shown)
        copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
            copied = false
        }
    }

    private func submit() {
        let shown = answer(for: text)
        if !shown.isEmpty { copyToPasteboard(shown) }
        text = ""
        copied = false
    }
}
