import AppKit
import Carbon
import SwiftUI
import WebKit

extension Notification.Name {
    static let focusOverlay = Notification.Name("QCalc.focusOverlay")
}

final class OverlayPanel: NSPanel {
    var onEscape: (() -> Void)?
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

    override func resignKey() {
        super.resignKey()
        if !ignoreResignKey {
            onEscape?()
        }
    }
}

final class OverlayWebView: WKWebView {
    override var needsPanelToBecomeKey: Bool { true }
    override var acceptsFirstResponder: Bool { true }
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
    private var settingsObserver: NSObjectProtocol?

    deinit {
        if let escapeMonitor {
            NSEvent.removeMonitor(escapeMonitor)
        }
        if let dragMonitor {
            NSEvent.removeMonitor(dragMonitor)
        }
        if let clickAwayMonitor {
            NSEvent.removeMonitor(clickAwayMonitor)
        }
        if let clickAwayLocalMonitor {
            NSEvent.removeMonitor(clickAwayLocalMonitor)
        }
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
    }

    func preload() {
        if panel == nil { build() }
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
    }

    func show() {
        if panel == nil { build() }
        if panel?.contentView === fallback, let web, webReady {
            panel?.contentView = web
        }
        applySize(height: overlayMinHeight)
        position()
        panel?.ignoreResignKey = true
        panel?.orderFrontRegardless()
        panel?.makeKeyAndOrderFront(nil)
        panel?.makeFirstResponder(web)
        resetAndFocus()
        DispatchQueue.main.async { [weak self] in
            self?.resetAndFocus()
            self?.panel?.ignoreResignKey = false
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in self?.focusInput() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in self?.focusInput() }
        NotificationCenter.default.post(name: .focusOverlay, object: nil)
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
                if let height = dict["height"] as? Double {
                    applySize(height: CGFloat(height))
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
        replyHandler(soulverPayload(from: message.body), nil)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        webReady = true
        disableWebViewScrolling(webView)
        focusInput()
        for delay in [0.05, 0.12, 0.3] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                self?.focusInput()
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        if (error as NSError).code == NSURLErrorCancelled { return }
        recover(from: webView)
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        if (error as NSError).code == NSURLErrorCancelled { return }
        recover(from: webView)
    }

    private func build() {
        let panel = OverlayPanel(
            contentRect: NSRect(x: 0, y: 0, width: overlayWidth, height: overlayMinHeight),
            styleMask: [.borderless, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.onEscape = { [weak self] in self?.hide() }
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
        config.websiteDataStore = WKWebsiteDataStore.nonPersistent()
        config.userContentController.add(self, name: "qcalc")
        config.userContentController.addScriptMessageHandler(self, contentWorld: .page, name: "soulver")
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        config.setValue(true, forKey: "allowUniversalAccessFromFileURLs")
        let sigFigs = AppSettings.shared.significantFigures
        let draftSeconds = AppSettings.shared.draftSeconds
        let defaultUnits = AppSettings.shared.defaultUnitsJSON()
        let answerForm = AppSettings.shared.answerForm
        let theme = AppSettings.shared.theme
        let boot = WKUserScript(
            source: """
            window.__QCALC_NATIVE = true;
            window.__QCALC_KEYS = [];
            window.__QCALC_HELD = '';
            window.__QCALC_META = false;
            window.__QCALC_SETTINGS = { sigFigs: \(sigFigs), draftSeconds: \(draftSeconds), defaultUnits: \(defaultUnits), answerForm: "\(answerForm)", theme: "\(theme)" };
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
                try { window.webkit.messageHandlers.qcalc.postMessage({ type: 'dismiss' }); } catch (err) {}
                return;
              }
              if (e.key === 'Meta' || e.key === 'Control') window.__QCALC_META = true;
              if (e.metaKey || e.ctrlKey) window.__qcalcRememberText();
              if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && (e.key === 'c' || e.key === 'C' || e.keyCode === 67)) {
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

    private func recover(from webView: WKWebView) {
        if !triedBundle, let bundled = bundledQuickURL() {
            triedBundle = true
            webView.loadFileURL(bundled, allowingReadAccessTo: bundled.deletingLastPathComponent())
            return
        }
        if !triedDevServer {
            loadDevServer(webView)
            return
        }
        loadFallback(from: webView)
    }

    private func loadFallback(from _: WKWebView) {
        if fallback == nil {
            let root = OverlayView(onDismiss: { [weak self] in self?.hide() })
            let host = NSHostingView(rootView: root)
            host.frame = panel?.contentView?.bounds ?? .zero
            host.autoresizingMask = [.width, .height]
            fallback = host
        }
        if let fallback, panel?.contentView !== fallback {
            panel?.contentView = fallback
            applySize(height: overlayMinHeight)
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
            applySize(height: CGFloat(raw))
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

    private func copyToPasteboard(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func notifyWebWillHide() {
        web?.evaluateJavaScript("if (window.__qcalcWillHide) window.__qcalcWillHide();")
    }

    private func resetAndFocus() {
        guard let web else { return }
        panel?.makeFirstResponder(web)
        web.evaluateJavaScript("""
        (function () {
          if (window.__qcalcReset) window.__qcalcReset();
          var el = document.querySelector('.quick-plain');
          if (el) { el.focus(); if (window.__qcalcFocus) window.__qcalcFocus(); }
          if (window.__qcalcSize) window.__qcalcSize();
        })()
        """)
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

    private func applySize(height: CGFloat) {
        guard let panel else { return }
        let h = min(max(height.rounded(.up), overlayMinHeight), 420)
        var frame = panel.frame
        frame.size = NSSize(width: overlayWidth, height: h)
        if let screen = panel.screen?.visibleFrame ?? NSScreen.main?.visibleFrame {
            if frame.maxY > screen.maxY {
                frame.origin.y = screen.maxY - h
            }
            if frame.origin.y < screen.minY {
                frame.origin.y = screen.minY
            }
        }
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
            panel.performDrag(with: event)
        }
    }

    private func installEscapeMonitor() {
        guard escapeMonitor == nil else { return }
        escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, self.panel?.isVisible == true else { return event }
            if event.keyCode == UInt16(kVK_Escape) {
                self.hide()
                return nil
            }
            return event
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
                panel.performDrag(with: event)
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
        guard let panel, let screen = NSScreen.main?.visibleFrame else { return }
        let size = panel.frame.size
        let x = screen.midX - size.width / 2
        let y = screen.minY + screen.height * 0.72 - size.height
        panel.setFrameOrigin(NSPoint(x: x, y: y))
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
        let theme = AppSettings.shared.theme
        return "{ sigFigs: \(n), draftSeconds: \(d), defaultUnits: \(units), answerForm: \"\(form)\", theme: \"\(theme)\" }"
    }

    private func applyWebAppearance(_ webView: WKWebView? = nil) {
        let target = webView ?? web
        let appearance: NSAppearance?
        switch AppSettings.shared.theme {
        case "light":
            appearance = NSAppearance(named: .aqua)
        case "dark":
            appearance = NSAppearance(named: .darkAqua)
        default:
            appearance = nil
        }
        target?.appearance = appearance
        panel?.appearance = appearance
    }

    private func applyWebSettings(_ dict: [String: Any]) {
        guard let n = intValue(dict["sigFigs"]) else { return }
        AppSettings.shared.setSignificantFigures(n, notifyWeb: false)
    }

    private func soulverPayload(from body: Any) -> [String: Any] {
        let dict = dictionary(from: body)
        let id = intValue(dict["id"]) ?? 0
        let expr = dict["expr"] as? String ?? ""
        let sigFigs = intValue(dict["sigFigs"]) ?? AppSettings.shared.significantFigures
        let answer = SoulverEval.evaluate(
            expr,
            ans: doubleValue(dict["ans"]),
            variables: stringKeyedDoubles(dict["variables"]),
            sigFigs: sigFigs
        )
        let display = answer?.display ?? ""
        var payload: [String: Any] = [
            "id": id,
            "expr": expr,
            "display": display,
        ]
        if let n = answer?.number, n.isFinite {
            payload["n"] = n
        } else {
            payload["n"] = NSNull()
        }
        return payload
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
        guard let v = MathEval.evaluate(text) else { return "" }
        return MathEval.format(v)
    }

    private func copyAnswer() {
        let shown = answer(for: text)
        guard !shown.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(shown, forType: .string)
        copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
            copied = false
        }
    }

    private func submit() {
        let shown = answer(for: text)
        if !shown.isEmpty {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(shown, forType: .string)
        }
        text = ""
        copied = false
    }
}
