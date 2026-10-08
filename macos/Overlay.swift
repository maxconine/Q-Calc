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

private func copyToPasteboard(_ text: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
}

private func uptime() -> TimeInterval {
    ProcessInfo.processInfo.systemUptime
}

private func commandShiftHeld() -> Bool {
    let flags = NSEvent.modifierFlags.intersection(.deviceIndependentFlagsMask)
    return flags.contains(.command) && flags.contains(.shift)
}

final class OverlayPanel: NSPanel {
    var onEscape: (() -> Void)?
    var onResignKey: (() -> Void)?
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
        onResignKey?()
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

// space the page asks for around the bar while the 420 smoke plays; the bar keeps its size and spot inside it
private struct OverlayRoom: Equatable {
    var top: CGFloat = 0
    var side: CGFloat = 0
    var bottom: CGFloat = 0
    static let closed = OverlayRoom()
}

final class OverlayController: NSObject, WKNavigationDelegate, WKScriptMessageHandler, WKScriptMessageHandlerWithReply {
    private var panel: OverlayPanel?
    private var web: OverlayWebView?
    private var fallback: NSView?
    private var escapeMonitor: Any?
    private var dragMonitor: Any?
    // the page says when the pointer is on a button, so the edge strip doesn't steal its click
    private var overControl = false
    private var clickAwayMonitor: Any?
    private var clickAwayLocalMonitor: Any?
    private var webReady = false
    private var triedBundle = false
    private var triedDevServer = false
    private let minOverlayWidth: CGFloat = 680
    // the bar's width: dragged wider from either edge for long calculations, and kept across launches
    private static let overlayWidthKey = "qcalc.overlayWidth"
    private var overlayWidth: CGFloat = max(680, CGFloat(UserDefaults.standard.double(forKey: OverlayController.overlayWidthKey)))
    // how close to the bar's left or right edge a press resizes it rather than reaching the page
    private let resizeGrip: CGFloat = 8
    private let overlayMinHeight: CGFloat = 72
    private let overlayMaxHeight: CGFloat = 560
    // distance from the overlay top to the composer, so history grows up and graphs grow down
    private var sizeAnchorTop: CGFloat = 0
    private var sizeRoom = OverlayRoom.closed
    private var settingsObserver: NSObjectProtocol?
    private var lastPasteAt: TimeInterval = 0
    private var pendingFirstRun = false
    private var pendingShow: String?
    private var showCount = 0
    private var revealedShow = 0
    // the last show whose page answered from a painted frame
    private var paintedShow = 0
    // hotkey shows in the last few seconds; pressing again and again means the user sees nothing
    private var recentShows: [TimeInterval] = []
    // whether the user asked for the overlay and hasn't dismissed it; a panel ordered out behind our back is a failure
    private var wantVisible = false
    private var shownAt: TimeInterval = 0
    // the display picked for the current show, the one with the pointer
    private var targetScreen: NSRect?
    private var pageLoadStartedAt: TimeInterval = 0
    // why and when the current panel was made, for the diary
    private var panelMadeAt: TimeInterval = 0
    private var panelMadeFor = "launch"
    // a fresh panel waiting for a space switch, wake or display change to settle
    private var pendingRefresh: DispatchWorkItem?
    private var observingEnvironment = false
    // the page wouldn't draw for this show, so the plain bar stays until the next one
    private var keepFallback = false
    var onSettings: (() -> Void)?
    private lazy var periodic: PeriodicWindowController = {
        let controller = PeriodicWindowController()
        controller.onPick = { [weak self] in self?.insertText($0) }
        return controller
    }()
    // one window per sheet, identities and calculus, each made on its first open
    private var identitySheets: [String: IdentityWindowController] = [:]
    var isShown: Bool { panel?.isVisible == true }

    // the walkthrough from the top, from settings or the menu
    func showTutorial() {
        show()
        web?.evaluateJavaScript("if (window.__qcalcTutorial) window.__qcalcTutorial();")
    }

    // off the main thread so a slow soulver evaluation never blocks typing
    private let soulverQueue = DispatchQueue(label: "qcalc.soulver", qos: .userInitiated)
    // the page has something open (pong) that esc should close before the panel hides
    private var pageTakesEscape = false
    private lazy var peer: PeerLink = {
        let link = PeerLink()
        link.onEvent = { [weak self] in self?.pushPeerEvent($0) }
        return link
    }()

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

    func toggle(from source: String = "hotkey") {
        if let panel, OverlayHealth.hotkeyHides(snapshot(panel)) {
            hide(because: source)
            return
        }
        // ordered in but somewhere the user isn't: a show on the same panel would land there again
        if let panel, panel.isVisible, !panel.isOnActiveSpace {
            replacePanel("ordered in on another space")
        }
        let now = uptime()
        recentShows = recentShows.filter { now - $0 < 4 } + [now]
        if recentShows.count >= 3 {
            recentShows = []
            rebuildAll("shown three times in four seconds")
        }
        show(from: source)
    }

    func hide(because why: String = "asked") {
        wantVisible = false
        guard panel?.isVisible == true else { return }
        OverlayLog.note("hide \(showCount) (\(why)) after \(seconds(uptime() - shownAt))")
        notifyWebWillHide()
        panel?.ignoreResignKey = true
        panel?.orderOut(nil)
        panel?.alphaValue = 1
    }

    func show(from source: String = "menu") {
        pendingRefresh?.cancel()
        pendingRefresh = nil
        // a hidden app's windows never come forward, and with no dock icon nothing else unhides it
        if NSApp.isHidden { NSApp.unhideWithoutActivation() }
        wantVisible = true
        keepFallback = false
        shownAt = uptime()
        targetScreen = Self.screenWithPointer()?.frame
        showOnce()
        let age = panelMadeAt > 0 ? seconds(shownAt - panelMadeAt) : "new"
        OverlayLog.note(
            "show \(showCount) (\(source)): panel \(age) old (\(panelMadeFor)), "
                + "page \(webReady ? "ready" : "loading"), \(NSScreen.screens.count) display(s)")
        verify(showCount, attempt: 0)
    }

    // the one place a show is judged: isVisible only says the panel is ordered in, so ask the window server
    // where it is and the page whether it drew, fix the least that's wrong, and look again
    private func verify(_ token: Int, attempt: Int) {
        let delay = attempt == 0 ? 0.4 : 0.5
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, token == self.showCount, self.wantVisible else { return }
            guard let panel = self.panel else { return }
            let state = self.snapshot(panel)
            let problems = OverlayHealth.problems(state)
            if problems.isEmpty {
                // a loading page is see-through; it isn't counted against the show until it's stuck
                if !state.pageReady, !state.usesFallback {
                    self.verify(token, attempt: attempt)
                    return
                }
                OverlayLog.note("show \(token) seen after \(self.seconds(uptime() - self.shownAt))\(attempt > 0 ? ", recovered" : "")")
                return
            }
            let fix = OverlayHealth.recovery(for: problems, attempt: attempt)
            OverlayLog.note(
                "show \(token) not seen (\(OverlayHealth.summary(problems))), check \(attempt + 1), \(fix.name). "
                    + self.describe(panel))
            switch fix {
            case .none:
                // out of fixes; at least don't leave it see-through
                if panel.isVisible, panel.alphaValue < 1 { panel.alphaValue = 1 }
                return
            case .wait:
                self.verify(token, attempt: attempt + 1)
                return
            case .reorder:
                break
            case .freshPanel:
                self.replacePanel(OverlayHealth.summary(problems))
            case .freshPage:
                self.rebuildAll(OverlayHealth.summary(problems))
            case .fallback:
                self.keepFallback = true
                self.loadFallback()
            }
            if NSApp.isHidden { NSApp.unhideWithoutActivation() }
            self.showOnce()
            self.verify(self.showCount, attempt: attempt + 1)
        }
    }

    private func snapshot(_ panel: NSPanel) -> OverlaySnapshot {
        var s = OverlaySnapshot()
        s.ordered = panel.isVisible
        s.appHidden = NSApp.isHidden
        s.onActiveSpace = panel.isOnActiveSpace
        s.windowServerOnScreen = Self.windowServerShows(panel)
        s.occlusionVisible = panel.occlusionState.contains(.visible)
        s.alpha = Double(panel.alphaValue)
        s.frame = panel.frame
        s.targetScreen = targetScreen
        s.screens = NSScreen.screens.map(\.frame)
        let content = panel.contentView
        s.usesFallback = fallback != nil && content === fallback
        s.contentInPanel = s.usesFallback || (web != nil && content === web)
        s.pageReady = webReady
        s.pageLoadingFor = webReady ? 0 : uptime() - pageLoadStartedAt
        s.painted = paintedShow == showCount
        return s
    }

    private static func windowServerShows(_ panel: NSPanel) -> Bool {
        guard panel.windowNumber > 0,
              let rows = CGWindowListCopyWindowInfo([.optionIncludingWindow], CGWindowID(panel.windowNumber)) as? [[String: Any]],
              let row = rows.first else { return false }
        return (row[kCGWindowIsOnscreen as String] as? Bool) == true
    }

    private static func screenWithPointer() -> NSScreen? {
        let mouse = NSEvent.mouseLocation
        return NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main ?? NSScreen.screens.first
    }

    private func describe(_ panel: NSPanel) -> String {
        "ordered \(panel.isVisible), key \(panel.isKeyWindow), active space \(panel.isOnActiveSpace), "
            + "on screen \(Self.windowServerShows(panel)), occlusion \(panel.occlusionState.rawValue), "
            + "alpha \(panel.alphaValue), level \(panel.level.rawValue), frame \(NSStringFromRect(panel.frame)), "
            + "target \(targetScreen.map(NSStringFromRect) ?? "none"), app hidden \(NSApp.isHidden), active \(NSApp.isActive), "
            + "panel \(seconds(uptime() - panelMadeAt)) old (\(panelMadeFor)), "
            + "content \(panel.contentView === web ? "page" : panel.contentView === fallback ? "plain bar" : "none"), "
            + "page frame \(NSStringFromRect(web?.frame ?? .zero)), page ready \(webReady), painted \(paintedShow)/\(showCount)"
    }

    private func seconds(_ t: TimeInterval) -> String {
        String(format: "%.2fs", t)
    }

    private func showOnce() {
        if panel == nil { build() }
        if panel?.contentView === fallback, let web, webReady, !keepFallback {
            panel?.contentView = web
        }
        sizeAnchorTop = 0
        applySize(height: overlayMinHeight, anchorTop: 0, room: .closed)
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
        askPainted(token)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in self?.reveal(token) }
        DispatchQueue.main.async { [weak self] in
            self?.focusInput()
            self?.panel?.ignoreResignKey = false
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in self?.focusInput() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in self?.focusInput() }
        NotificationCenter.default.post(name: .focusOverlay, object: nil)
    }

    // the page answers from its second frame, so an answer means it really drew for this show
    private func askPainted(_ token: Int) {
        web?.evaluateJavaScript("""
        requestAnimationFrame(function () { requestAnimationFrame(function () {
          try { window.webkit.messageHandlers.qcalc.postMessage({ type: 'painted', token: \(token) }); } catch (err) {}
        }); });
        """)
    }

    // at launch the page is still loading, and a show before it can size itself would flash
    func showWhenReady(from source: String) {
        guard webReady else {
            pendingShow = source
            return
        }
        show(from: source)
    }

    func showFirstRun() {
        guard webReady else {
            pendingFirstRun = true
            return
        }
        show(from: "first run")
        web?.evaluateJavaScript("window.__QCALC_FIRST_RUN = true; if (window.__qcalcFirstRun) window.__qcalcFirstRun();")
    }

    func showTips() {
        show(from: "tips")
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
                        anchorTop: CGFloat(doubleValue(dict["anchorTop"]) ?? 0),
                        room: overlayRoom(dict["room"])
                    )
                }
            case "dismiss":
                hide(because: "page")
            case "painted":
                if let token = doubleValue(dict["token"]) { paintedShow = max(paintedShow, Int(token)) }
            case "copy":
                if let text = dict["text"] as? String {
                    copyToPasteboard(text)
                }
            case "drag":
                beginWindowDrag()
            case "overControl":
                overControl = dict["on"] as? Bool ?? false
            case "settings":
                applyWebSettings(dict)
            case "openSettings":
                onSettings?()
            case "onboarding":
                applyWebOnboarding(dict)
            case "analytics":
                AppSettings.shared.saveAnalytics(dict["stash"])
            case "eval":
                pushSoulverResult(soulverPayload(from: dict))
            case "periodic":
                periodic.show(PeriodicElement.list(from: dict["elements"]))
            case "identities":
                if let sheet = IdentitySheet(dict) { identityWindow(sheet.id).show(sheet) }
            case "peer":
                handlePeer(dict)
            case "escapeLayer":
                pageTakesEscape = dict["on"] as? Bool ?? false
            default:
                break
            }
        }
    }

    private func identityWindow(_ id: String) -> IdentityWindowController {
        if let controller = identitySheets[id] { return controller }
        let controller = IdentityWindowController(id: id, cascade: identitySheets.count)
        identitySheets[id] = controller
        return controller
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

    // a page whose process ended (long sleeps, memory pressure) is fully transparent, so reload it
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        OverlayLog.note("page process ended (overlay \(panel?.isVisible == true ? "up" : "hidden")), reloading")
        webReady = false
        pageTakesEscape = false
        peer.close()
        triedBundle = false
        triedDevServer = false
        loadQuickCalc(webView)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        if pageLoadStartedAt > 0, uptime() - pageLoadStartedAt > OverlayHealth.pageLoadLimit {
            OverlayLog.note("page loaded after \(seconds(uptime() - pageLoadStartedAt))")
        }
        webReady = true
        disableWebViewScrolling(webView)
        if pendingFirstRun {
            pendingFirstRun = false
            DispatchQueue.main.async { [weak self] in self?.showFirstRun() }
        } else if let source = pendingShow {
            pendingShow = nil
            DispatchQueue.main.async { [weak self] in self?.showWhenReady(from: source) }
        }
        // a page rebuilt while the panel is up still owes it a size and a reveal
        if panel?.isVisible == true {
            let token = showCount
            resetAndFocus { [weak self] in self?.reveal(token) }
            askPainted(token)
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

    private func makePanel() -> OverlayPanel {
        let panel = OverlayPanel(
            contentRect: NSRect(x: 0, y: 0, width: overlayWidth, height: overlayMinHeight),
            styleMask: [.borderless, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.onEscape = { [weak self] in self?.hide(because: "esc") }
        panel.onResignKey = { [weak self] in self?.lostFocus() }
        panel.onPaste = { [weak self] in self?.pasteIntoWeb() }
        panel.isFloatingPanel = true
        // above full screen apps' own floating windows, like spotlight
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.becomesKeyOnlyIfNeeded = false
        return panel
    }

    // a fresh panel around the same page, for when the old one stops coming forward (seen after a long sleep)
    private func replacePanel(_ why: String, quiet: Bool = false) {
        guard let old = panel else { return }
        if !quiet { OverlayLog.note("fresh panel (\(why))") }
        let content = old.contentView === fallback ? fallback : web
        old.ignoreResignKey = true
        old.orderOut(nil)
        old.contentView = NSView()
        let fresh = makePanel()
        fresh.contentView = content
        panel = fresh
        markPanel(why)
        sizeRoom = .closed
        applyWebAppearance()
    }

    // a fresh panel and a fresh page from the same configuration, so the handlers and stored drafts carry over
    private func rebuildAll(_ why: String) {
        guard let old = web, let oldPanel = panel else { return }
        OverlayLog.note("fresh panel and page (\(why))")
        oldPanel.ignoreResignKey = true
        oldPanel.orderOut(nil)
        oldPanel.contentView = NSView()
        old.navigationDelegate = nil
        // the new page knows nothing of a link the old one had open, so nothing should stay hosting behind it
        pageTakesEscape = false
        peer.close()
        let fresh = makeWebView(old.configuration)
        let next = makePanel()
        next.contentView = fresh
        web = fresh
        panel = next
        markPanel(why)
        webReady = false
        triedBundle = false
        triedDevServer = false
        sizeRoom = .closed
        applyWebAppearance()
        loadQuickCalc(fresh)
    }

    private func markPanel(_ why: String) {
        panelMadeAt = uptime()
        panelMadeFor = why
    }

    // a space switch, wake or display change can leave a hidden panel unable to come forward where the user is,
    // so it's swapped for a fresh one once things settle. never while the overlay is wanted: the switch can land
    // in the middle of a show (the notification arrives after the swipe ends), and swapping then hid the panel
    private func observeEnvironment() {
        guard !observingEnvironment else { return }
        observingEnvironment = true
        let workspace = NSWorkspace.shared.notificationCenter
        workspace.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.environmentChanged("wake", log: true)
        }
        workspace.addObserver(forName: NSWorkspace.activeSpaceDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.environmentChanged("space switch", log: false)
        }
        NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { [weak self] _ in
            self?.environmentChanged("display change", log: true)
        }
    }

    private func environmentChanged(_ why: String, log: Bool) {
        if wantVisible {
            // shown just before the switch finished: the panel joins every space, but the switch took its focus
            if let panel, panel.isVisible, uptime() - shownAt < 2 {
                if log || !panel.isKeyWindow {
                    OverlayLog.note("\(why) during show \(showCount), key \(panel.isKeyWindow), bringing it forward")
                }
                panel.orderFrontRegardless()
                panel.makeKeyAndOrderFront(nil)
                focusInput()
            }
            return
        }
        if log { OverlayLog.note("\(why) while hidden, fresh panel once it settles") }
        pendingRefresh?.cancel()
        let item = DispatchWorkItem { [weak self] in
            guard let self, !self.wantVisible, self.panel?.isVisible != true else { return }
            self.pendingRefresh = nil
            self.replacePanel(why, quiet: true)
        }
        pendingRefresh = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: item)
    }

    // the panel lost key: a click elsewhere or an app switch hides it, but focus taken during the show itself
    // (a space switch finishing, the launch activating something) leaves it up
    private func lostFocus() {
        guard wantVisible, panel?.isVisible == true else { return }
        let since = uptime() - shownAt
        if OverlayHealth.focusLossHides(sinceShow: since) {
            hide(because: "lost focus")
        } else {
            OverlayLog.note("show \(showCount) lost focus after \(seconds(since)), kept up")
        }
    }

    private func build() {
        let panel = makePanel()
        markPanel("launch")
        observeEnvironment()
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
        let analytics = AppSettings.shared.analyticsJSON()
        let os = ProcessInfo.processInfo.operatingSystemVersion
        let boot = WKUserScript(
            source: """
            window.__QCALC_NATIVE = true;
            document.documentElement.classList.add('quick-resizable');
            window.__QCALC_SMOKE_ROOM = true;
            window.__QCALC_KEYS = [];
            window.__QCALC_HELD = '';
            window.__QCALC_HELD_BAR = false;
            window.__QCALC_META = false;
            window.__QCALC_SETTINGS = \(settings);
            window.__QCALC_ONBOARDING = \(onboarding);
            window.__QCALC_ANALYTICS = \(analytics);
            window.__QCALC_OS = "macOS \(os.majorVersion).\(os.minorVersion)";
            document.documentElement.dataset.theme = "\(theme)";
            window.__qcalcNativeResult = window.__qcalcNativeResult || function (reply) {
              window.dispatchEvent(new CustomEvent('qcalc-soulver', { detail: reply }));
            };
            window.__qcalcBarText = function () {
              var el = document.querySelector('.quick-plain');
              if (el && el.selectionStart != null && el.selectionEnd > el.selectionStart) {
                return el.value.slice(el.selectionStart, el.selectionEnd);
              }
              return '';
            };
            window.__qcalcSelectedText = function () {
              var bar = window.__qcalcBarText();
              if (bar) return bar;
              var ae = document.activeElement;
              if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA') && ae.selectionEnd > ae.selectionStart) {
                return ae.value.slice(ae.selectionStart, ae.selectionEnd);
              }
              var sel = window.getSelection();
              return (sel && sel.toString()) || '';
            };
            window.__qcalcCopyText = function (text, fromBar) {
              if (!fromBar || typeof window.__qcalcFormatCopy !== 'function') return text;
              try {
                var next = window.__qcalcFormatCopy(text);
                if (typeof next === 'string' && next) return next;
              } catch (err) {}
              return text;
            };
            window.__qcalcRememberText = function () {
              var bar = window.__qcalcBarText();
              var live = bar || window.__qcalcSelectedText();
              if (live) {
                window.__QCALC_HELD = live;
                window.__QCALC_HELD_BAR = !!bar;
              } else if (!window.__QCALC_META) {
                window.__QCALC_HELD = '';
                window.__QCALC_HELD_BAR = false;
              }
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
                var barText = window.__qcalcBarText();
                var fromBar = !!barText || (!window.__qcalcSelectedText() && window.__QCALC_HELD_BAR);
                var text = barText || window.__qcalcSelectedText() || window.__QCALC_HELD || '';
                if (text) {
                  e.preventDefault();
                  e.stopImmediatePropagation();
                  text = window.__qcalcCopyText(text, fromBar);
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
              var barText = window.__qcalcBarText();
              var fromBar = !!barText || (!window.__qcalcSelectedText() && window.__QCALC_HELD_BAR);
              var text = barText || window.__qcalcSelectedText() || window.__QCALC_HELD || '';
              if (!text) return;
              text = window.__qcalcCopyText(text, fromBar);
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
        let web = makeWebView(config)
        panel.contentView = web
        self.web = web
        self.panel = panel
        applyWebAppearance(web)
        loadQuickCalc(web)
    }

    private func makeWebView(_ config: WKWebViewConfiguration) -> OverlayWebView {
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
        web.onPaste = { [weak self] in self?.pasteIntoWeb() }
        return web
    }

    private func loadQuickCalc(_ web: WKWebView) {
        pageLoadStartedAt = uptime()
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
            let root = OverlayView(onDismiss: { [weak self] in self?.hide(because: "esc") })
            let host = NSHostingView(rootView: root)
            host.frame = panel?.contentView?.bounds ?? .zero
            host.autoresizingMask = [.width, .height]
            fallback = host
        }
        if let fallback, panel?.contentView !== fallback {
            OverlayLog.note("showing the plain bar instead of the page")
            panel?.contentView = fallback
            applySize(height: overlayMinHeight, anchorTop: 0, room: .closed)
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
            hide(because: "page")
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

    // a periodic table click: the insert is queued behind show()'s reset, so it lands in the fresh input
    private func insertText(_ text: String) {
        guard let encoded = jsonStringLiteral(text) else { return }
        if panel?.isVisible != true { show(from: "periodic table") }
        web?.evaluateJavaScript("window.__qcalcInsert && window.__qcalcInsert(\(encoded));")
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

    private func applySize(height: CGFloat, anchorTop: CGFloat? = nil, room: OverlayRoom? = nil) {
        guard let panel else { return }
        let room = room ?? sizeRoom
        let extra = room.top + room.bottom
        let h = min(max(height.rounded(.up), overlayMinHeight + extra), overlayMaxHeight + extra)
        let lowest = max(room.top, h - room.bottom - overlayMinHeight)
        let nextAnchor = min(max(anchorTop ?? sizeAnchorTop, room.top), lowest)
        let composerTop = panel.frame.maxY - sizeAnchorTop
        // the panel without its room: the bar, with any history or graph
        var body = NSRect(
            x: panel.frame.minX + sizeRoom.side,
            y: composerTop + nextAnchor - h + room.bottom,
            width: fittedWidth(overlayWidth, on: panel.screen),
            height: h - extra
        )
        // the body stays on the visible screen; if it can't fit, the top wins. the room may run off the edges
        if let screen = panel.screen?.visibleFrame ?? NSScreen.main?.visibleFrame {
            body.origin.y = min(max(body.minY, screen.minY), screen.maxY - body.height)
            body.origin.x = min(max(body.minX, screen.minX), screen.maxX - body.width)
        }
        let frame = NSRect(
            x: body.minX - room.side,
            y: body.minY - room.bottom,
            width: body.width + 2 * room.side,
            height: h
        )
        sizeAnchorTop = nextAnchor
        sizeRoom = room
        // the shadow follows the window's alpha, so it would trace the smoke; the page shadows the bar meanwhile
        if room != .closed { panel.hasShadow = false }
        panel.setFrame(frame, display: true)
        if room == .closed, !panel.hasShadow {
            panel.hasShadow = true
            panel.invalidateShadow()
            // again once the page has drawn at the new size
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak panel] in panel?.invalidateShadow() }
        }
    }

    // never wider than the screen it's on, less a margin each side
    private func fittedWidth(_ width: CGFloat, on screen: NSScreen?) -> CGFloat {
        guard let visible = (screen ?? NSScreen.main)?.visibleFrame else { return max(width, minOverlayWidth) }
        return min(max(width, minOverlayWidth), max(minOverlayWidth, visible.width - 40))
    }

    private enum ResizeSide { case left, right }

    // runs until the mouse comes up, so the page never sees the press
    private func trackResize(from side: ResizeSide) {
        guard let panel else { return }
        var lastX = NSEvent.mouseLocation.x
        panel.trackEvents(matching: [.leftMouseDragged, .leftMouseUp], timeout: .greatestFiniteMagnitude, mode: .eventTracking) { event, stop in
            guard let event else { return }
            if event.type == .leftMouseUp {
                stop.pointee = true
                return
            }
            // screen points, so the window moving under the cursor doesn't feed back into the drag
            let x = NSEvent.mouseLocation.x
            self.resizeBar(by: side == .right ? x - lastX : lastX - x, from: side)
            lastX = x
        }
        saveBarWidth(movedLeftEdge: side == .left)
    }

    // the dragged side moves and the other stays put; the page reports its new height as it reflows
    private func resizeBar(by dx: CGFloat, from side: ResizeSide) {
        guard let panel, sizeRoom == .closed, dx != 0 else { return }
        let width = fittedWidth(panel.frame.width + dx, on: panel.screen)
        guard width != panel.frame.width else { return }
        var frame = panel.frame
        if side == .left { frame.origin.x = frame.maxX - width }
        frame.size.width = width
        overlayWidth = width
        panel.setFrame(frame, display: true)
        panel.invalidateShadow()
    }

    // dragging the left edge moves the bar, so its spot is kept too; a centred bar dragged on the right stays centred
    private func saveBarWidth(movedLeftEdge: Bool) {
        UserDefaults.standard.set(Double(overlayWidth), forKey: Self.overlayWidthKey)
        if movedLeftEdge { rememberPosition() }
    }

    // the page's room for the 420 smoke; anything missing or odd is no room
    private func overlayRoom(_ any: Any?) -> OverlayRoom {
        guard let dict = any as? [String: Any] else { return .closed }
        func px(_ key: String) -> CGFloat { CGFloat(min(max(doubleValue(dict[key]) ?? 0, 0), 240)) }
        return OverlayRoom(top: px("top"), side: px("side"), bottom: px("bottom"))
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
            // the settings key is the user's choice now, so the page reads it and posts openSettings
            return event
        }
    }

    // esc hides immediately, unless the page said it has a layer open; then the page closes that instead,
    // and a page that doesn't answer true still gets the panel hidden
    private func escape() {
        guard pageTakesEscape, let web else {
            hide(because: "esc")
            return
        }
        web.evaluateJavaScript("!!(window.__qcalcEscape && window.__qcalcEscape())") { [weak self] result, _ in
            if (result as? Bool) != true { self?.hide(because: "esc") }
        }
    }

    private func handlePeer(_ dict: [String: Any]) {
        switch dict["op"] as? String {
        case "discover":
            peer.discover()
        case "stopDiscovery":
            peer.stopDiscovery()
        case "host":
            peer.host()
        case "join":
            if let id = dict["peer"] as? String, let code = dict["code"] as? String {
                peer.join(id, code: code)
            }
        case "send":
            if let data = dict["data"] as? String { peer.send(data) }
        case "close":
            peer.close()
        default:
            break
        }
    }

    private func pushPeerEvent(_ event: [String: Any]) {
        guard JSONSerialization.isValidJSONObject(event),
              let data = try? JSONSerialization.data(withJSONObject: event, options: []),
              let json = String(data: data, encoding: .utf8)
        else { return }
        web?.evaluateJavaScript("window.__qcalcPeer && window.__qcalcPeer(\(json));")
    }

    private func installDragMonitor() {
        guard dragMonitor == nil else { return }
        dragMonitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window === panel, !self.overControl else {
                return event
            }
            let p = event.locationInWindow
            let room = self.sizeRoom
            let size = panel.frame.size
            let body = NSRect(
                x: room.side,
                y: room.bottom,
                width: size.width - 2 * room.side,
                height: size.height - room.top - room.bottom
            )
            // a click in the smoke's room goes to the page, which takes it as a click away
            guard p.x >= body.minX, p.x <= body.maxX, p.y >= body.minY, p.y <= body.maxY else { return event }
            let edge: CGFloat = 14
            let alongTop = p.y >= body.maxY - edge
            // either side edge drags the bar wider or narrower; the top strip still moves it
            let onLeft = p.x <= body.minX + self.resizeGrip
            let onRight = p.x >= body.maxX - self.resizeGrip
            if !alongTop, room == .closed, onLeft || onRight {
                self.trackResize(from: onLeft ? .left : .right)
                return nil
            }
            let topCorner = p.y >= body.maxY - 44 && (p.x <= body.minX + edge || p.x >= body.maxX - edge)
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
            self?.hide(because: "click in another app")
        }
        clickAwayLocalMonitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window !== panel,
                  !(event.window is PeriodicPanel) else {
                return event
            }
            self.hide(because: "click in another window")
            return event
        }
    }

    // on the display with the pointer, so it opens where the user is looking
    private func position() {
        guard let panel,
              let main = NSScreen.screens.first(where: { $0.frame == targetScreen }) ?? Self.screenWithPointer()
        else { return }
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
        all[key] = [Double(panel.frame.minX + sizeRoom.side - origin.x), Double(top - origin.y)]
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
        let typstPreview = AppSettings.shared.typstPreview ? "true" : "false"
        let typstCopy = AppSettings.shared.typstCopy ? "true" : "false"
        let shareUsage = AppSettings.shared.shareUsage ? "true" : "false"
        let copyUnitless = AppSettings.shared.copyUnitless ? "true" : "false"
        let theme = AppSettings.shared.theme
        let angle = AppSettings.shared.angleMode
        let fractions = AppSettings.shared.fractionMode ? "true" : "false"
        return "{ sigFigs: \(n), draftSeconds: \(d), defaultUnits: \(units), answerForm: \"\(form)\", historyInsert: \"\(insert)\", historyShow: \"\(historyShow)\", rationalize: \(rationalize), sigFigMode: \(sigFigMode), theme: \"\(theme)\", angleMode: \"\(angle)\", fractionMode: \(fractions), keepWords: \(keepWords), typstPreview: \(typstPreview), typstCopy: \(typstCopy), copyUnitless: \(copyUnitless), shareUsage: \(shareUsage), keybinds: \(AppSettings.shared.keybindsJSON()), \(hotKeyJavaScriptFields()) }"
    }

    // a chosen key can be \ or ', so the title goes through json
    private func hotKeyJavaScriptFields() -> String {
        let title = AppSettings.shared.activeHotKey?.title ?? ""
        let data = (try? JSONSerialization.data(withJSONObject: [title], options: [])) ?? Data("[\"\"]".utf8)
        let quoted = String(data: data, encoding: .utf8).map { String($0.dropFirst().dropLast()) } ?? "\"\""
        let failed = AppSettings.shared.hotKeyFailed ? "true" : "false"
        return "hotkey: \(quoted), hotkeyFailed: \(failed)"
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
        if let typstPreview = boolValue(dict["typstPreview"]) {
            AppSettings.shared.setTypstPreview(typstPreview, notifyWeb: false)
        }
        if let typstCopy = boolValue(dict["typstCopy"]) {
            AppSettings.shared.setTypstCopy(typstCopy, notifyWeb: false)
        }
        if let copyUnitless = boolValue(dict["copyUnitless"]) {
            AppSettings.shared.setCopyUnitless(copyUnitless, notifyWeb: false)
        }
        if let shareUsage = boolValue(dict["shareUsage"]) {
            AppSettings.shared.setShareUsage(shareUsage, notifyWeb: false)
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
        if let binds = dict["keybinds"] as? [String: Any] {
            AppSettings.shared.setKeybinds(binds.compactMapValues { $0 as? String }, notifyWeb: false)
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
        copyToPasteboard(UnitlessCopy.copied(shown))
        copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
            copied = false
        }
    }

    private func submit() {
        let shown = answer(for: text)
        if !shown.isEmpty { copyToPasteboard(UnitlessCopy.copied(shown)) }
        text = ""
        copied = false
    }
}

// q calc's NSLog lines never reach the unified log, so the overlay keeps a short diary of its own shows:
// every show with how old its panel was, whether it was seen, what was wrong and what fixed it, and every hide
// with its reason. at 128 KB it moves to overlay.log.1, so the pair stays under 256 KB
enum OverlayLog {
    private static let url = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Logs/Q Calc/overlay.log")
    private static let previous = url.deletingPathExtension().appendingPathExtension("log.1")
    private static let limit = 128_000
    private static let queue = DispatchQueue(label: "qcalc.overlaylog", qos: .utility)
    private static let stamp: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        return formatter
    }()

    static func note(_ line: String) {
        let text = "\(stamp.string(from: Date())) \(line)\n"
        queue.async {
            let files = FileManager.default
            try? files.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let size = try? files.attributesOfItem(atPath: url.path)[.size] as? Int, size > limit {
                try? files.removeItem(at: previous)
                try? files.moveItem(at: url, to: previous)
            }
            guard let handle = try? FileHandle(forWritingTo: url) else {
                try? Data(text.utf8).write(to: url)
                return
            }
            handle.seekToEndOfFile()
            handle.write(Data(text.utf8))
            try? handle.close()
        }
    }
}
