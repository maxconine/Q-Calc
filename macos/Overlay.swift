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

// ⌘ and a letter by the layout's letter; a layout without latin letters falls back to the key where it sits on qwerty
private func isCommandKey(_ event: NSEvent, _ letter: String, _ code: Int) -> Bool {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.contains(.command), !flags.contains(.option), !flags.contains(.control), !flags.contains(.shift) else {
        return false
    }
    guard let chars = event.charactersIgnoringModifiers?.lowercased(), !chars.isEmpty else { return false }
    if chars == letter { return true }
    return !chars.unicodeScalars.allSatisfy({ $0.isASCII }) && event.keyCode == UInt16(code)
}

func isCommandWCloseKey(_ event: NSEvent) -> Bool {
    isCommandKey(event, "w", kVK_ANSI_W)
}

private func isPlainEnterKey(_ event: NSEvent) -> Bool {
    let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
    return flags.isEmpty && !event.isARepeat
        && (event.keyCode == UInt16(kVK_Return) || event.keyCode == UInt16(kVK_ANSI_KeypadEnter))
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
    // the close button, while a game has the panel as its window
    var onClose: (() -> Void)?
    var ignoreResignKey = false
    // a game's window (⌃F): an ordinary one, main while it's in front
    var gameWindow = false

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { gameWindow }

    override func performClose(_ sender: Any?) {
        guard gameWindow, let onClose else { return super.performClose(sender) }
        onClose()
    }

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

final class OverlayController: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
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
    // the usual width, and how narrow a drag on a side edge may make it
    private let defaultOverlayWidth: CGFloat = 680
    private let minOverlayWidth: CGFloat = 340
    // the bar's width: dragged wider from either edge for long calculations, and kept across launches
    private static let overlayWidthKey = "qcalc.overlayWidth"
    private var overlayWidth: CGFloat = {
        let saved = CGFloat(UserDefaults.standard.double(forKey: OverlayController.overlayWidthKey))
        return saved > 0 ? max(340, saved) : 680
    }()
    // set by a drag the user started; while it's set, wherever the window moves is where they put it
    private var userMoving = false
    private var moveObserver: NSObjectProtocol?
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

    // the page has something open (pong) that esc should close before the panel hides
    private var pageTakesEscape = false
    // a game in a window of its own (⌃F in the page): the bar's frame from before, to go back to. nil when not
    private var gameRestoreFrame: NSRect?
    private static let barStyle: NSWindow.StyleMask = [.borderless, .fullSizeContentView, .nonactivatingPanel]
    // a titled window whose page runs up under a clear title bar, the traffic lights over the page's top strip.
    // still non-activating (the flag can't be reliably taken off a panel once made), so the app is activated by hand
    private static let gameStyle: NSWindow.StyleMask = [
        .titled, .closable, .miniaturizable, .resizable, .fullSizeContentView, .nonactivatingPanel,
    ]
    // the title bar's height, which the page keeps clear
    private static let titlebarHeight: CGFloat = {
        let content = NSRect(x: 0, y: 0, width: 400, height: 400)
        return NSWindow.frameRect(forContentRect: content, styleMask: [.titled]).height - content.height
    }()
    // what was in front before a game's window activated q calc, handed the front back as the bar hides
    private var appBeforeGame: NSRunningApplication?
    // a game is open: clicking away or losing focus leaves the panel up
    private var gameOpen = false
    private static let nicknameKey = "qcalc.nickname"
    private var messagingObserver: NSObjectProtocol?
    private var inboxObserver: NSObjectProtocol?

    deinit {
        for monitor in [escapeMonitor, dragMonitor, clickAwayMonitor, clickAwayLocalMonitor].compactMap({ $0 }) {
            NSEvent.removeMonitor(monitor)
        }
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
        if let messagingObserver {
            NotificationCenter.default.removeObserver(messagingObserver)
        }
        if let inboxObserver {
            NotificationCenter.default.removeObserver(inboxObserver)
        }
    }

    func preload() {
        if panel == nil { build() }
        // the ecb's rates for currency conversions, handed to the page as they arrive
        ECBRates.shared.onChange = { [weak self] json in
            self?.web?.evaluateJavaScript("window.__QCALC_RATES = \(json);")
        }
        ECBRates.shared.start()
    }

    func toggle(from source: String = "hotkey") {
        // a game's window minimised, or behind another app's, comes forward rather than going
        if let panel, gameRestoreFrame != nil, panel.isMiniaturized || (panel.isVisible && panel.isOnActiveSpace && !panel.isKeyWindow) {
            if panel.isMiniaturized { panel.deminiaturize(nil) } else { focusGameWindow() }
            return
        }
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
        tellMessagesBar()
        userMoving = false
        // the clock for moving the bar back runs from the last time it was in use
        if panel?.isVisible == true { UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: Self.lastHiddenKey) }
        // before anything else, so the next show never finds the bar the size of the screen
        setGameWindow(false, fade: false)
        guard panel?.isVisible == true else { return }
        OverlayLog.note("hide \(showCount) (\(why)) after \(seconds(uptime() - shownAt))")
        notifyWebWillHide()
        panel?.ignoreResignKey = true
        panel?.orderOut(nil)
        panel?.alphaValue = 1
        // not while settings is open, which is a window of q calc's own the user is in
        if let app = appBeforeGame {
            appBeforeGame = nil
            if NSApp.isActive, NSApp.activationPolicy() == .accessory { app.activate(options: []) }
        }
    }

    func show(from source: String = "menu") {
        pendingRefresh?.cancel()
        pendingRefresh = nil
        // a hidden app's windows never come forward, and with no dock icon nothing else unhides it
        if NSApp.isHidden { NSApp.unhideWithoutActivation() }
        wantVisible = true
        tellMessagesBar()
        keepFallback = false
        shownAt = uptime()
        targetScreen = Self.screenWithPointer()?.frame
        resetPlaceIfIdle()
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
        setGameWindow(false, fade: false)
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
            case "periodic":
                periodic.show(PeriodicElement.list(from: dict["elements"]), bar: barSpot())
            case "identities":
                if let sheet = IdentitySheet(dict) {
                    identityWindow(sheet.id).show(sheet, section: (dict["section"] as? NSNumber)?.intValue ?? 0, bar: barSpot())
                }
            case "escapeLayer":
                pageTakesEscape = dict["on"] as? Bool ?? false
            case "gameFullscreen":
                let natural = doubleValue(dict["w"]).flatMap { w in doubleValue(dict["h"]).map { CGSize(width: w, height: $0) } }
                setGameWindow(dict["on"] as? Bool ?? false, fromPage: true, natural: natural)
            case "game":
                gameOpen = dict["open"] as? Bool ?? false
                if !gameOpen { setGameWindow(false, fade: false) }
                tellMessagesBar()
            case "gameSave":
                if let key = dict["key"] as? String {
                    AppSettings.shared.saveGame(key, dict["value"] as? String)
                }
            case "nickname":
                // the page's web storage is wiped every launch, so the name the room lists show is kept here
                if let name = dict["name"] as? String, !name.isEmpty, name.count <= 64 {
                    UserDefaults.standard.set(name, forKey: Self.nicknameKey)
                }
            case "messaging":
                Messages.shared.setLists(accepted: dict["accepted"] as? [String] ?? [], blocked: dict["blocked"] as? [String] ?? [])
            case "messagesSeen":
                Messages.shared.seen((dict["ids"] as? [String] ?? []).prefix(64).map { $0 })
            case "messageAnswer":
                let from = Messages.clean(dict["from"] as? String ?? "")
                guard !from.isEmpty else { break }
                if dict["allow"] as? Bool == true { Messages.shared.allow(from) } else { Messages.shared.block(from) }
            default:
                break
            }
        }
    }

    // the bar's frame and its screen's visible frame, for a reference sheet to open clear of it
    private func barSpot() -> (frame: NSRect, visible: NSRect)? {
        guard let panel, panel.isVisible, let screen = panel.screen ?? NSScreen.main else { return nil }
        return (panel.frame, screen.visibleFrame)
    }

    private func identityWindow(_ id: String) -> IdentityWindowController {
        if let controller = identitySheets[id] { return controller }
        let controller = IdentityWindowController(id: id, cascade: identitySheets.count)
        identitySheets[id] = controller
        return controller
    }

    // a page whose process ended (long sleeps, memory pressure) is fully transparent, so reload it
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        OverlayLog.note("page process ended (overlay \(panel?.isVisible == true ? "up" : "hidden")), reloading")
        webReady = false
        pageTakesEscape = false
        gameOpen = false
        setGameWindow(false, fade: false)
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
            styleMask: Self.barStyle,
            backing: .buffered,
            defer: false
        )
        // a minimised game window is closed when it's swapped out, so its dock tile goes; ordered out otherwise
        panel.isReleasedWhenClosed = false
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.title = "Q Calc"
        panel.onEscape = { [weak self] in self?.hide(because: "esc") }
        panel.onClose = { [weak self] in self?.setGameWindow(false) }
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
        setGameWindow(false, fade: false)
        if !quiet { OverlayLog.note("fresh panel (\(why))") }
        let content = old.contentView === fallback ? fallback : web
        let minimised = old.isMiniaturized
        old.ignoreResignKey = true
        old.orderOut(nil)
        old.contentView = NSView()
        if minimised { old.close() }
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
        setGameWindow(false, fade: false)
        oldPanel.ignoreResignKey = true
        oldPanel.orderOut(nil)
        oldPanel.contentView = NSView()
        old.navigationDelegate = nil
        pageTakesEscape = false
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
        // a game's window back from the dock takes the keys again
        NotificationCenter.default.addObserver(forName: NSWindow.didDeminiaturizeNotification, object: nil, queue: .main) { [weak self] note in
            guard let self, let panel = self.panel, note.object as? NSWindow === panel else { return }
            self.focusGameWindow()
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
        guard wantVisible, panel?.isVisible == true, !gameOpen else { return }
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
        // wiped every launch, so state that must survive (settings, onboarding, game saves) is kept in AppSettings
        config.websiteDataStore = WKWebsiteDataStore.nonPersistent()
        config.userContentController.add(self, name: "qcalc")
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        config.setValue(true, forKey: "allowUniversalAccessFromFileURLs")
        // writing tools (and macos 26's "write with siri" prompt) would offer itself on every focused field; the
        // property is only in the headers for a 15+ target, so it's set by key
        if #available(macOS 15.0, *) {
            config.setValue(NSWritingToolsBehavior.none.rawValue, forKey: "writingToolsBehavior")
        }
        let settings = settingsJavaScriptObject()
        let theme = AppSettings.shared.theme
        let onboarding = AppSettings.shared.onboardingJSON()
        let analytics = AppSettings.shared.analyticsJSON()
        let gameSaves = AppSettings.shared.gameSavesJSON()
        let os = ProcessInfo.processInfo.operatingSystemVersion
        let nickname = UserDefaults.standard.string(forKey: Self.nicknameKey).flatMap { jsonStringLiteral($0) } ?? "null"
        let boot = WKUserScript(
            source: """
            window.__QCALC_NATIVE = true;
            document.documentElement.classList.add('quick-resizable');
            document.documentElement.style.setProperty('--bar-width', '\(Int(overlayWidth))px');
            window.__QCALC_SMOKE_ROOM = true;
            window.__QCALC_KEYS = [];
            window.__QCALC_HELD = '';
            window.__QCALC_HELD_BAR = false;
            window.__QCALC_META = false;
            window.__QCALC_SETTINGS = \(settings);
            window.__QCALC_ONBOARDING = \(onboarding);
            window.__QCALC_ANALYTICS = \(analytics);
            window.__QCALC_NICKNAME = \(nickname);
            window.__QCALC_MESSAGING = \(Messages.shared.webJSON);
            window.__QCALC_GAME_SAVES = \(gameSaves);
            window.__QCALC_RATES = \(ECBRates.shared.pageJSON);
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
            setGameWindow(false, fade: false)
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
          if (window.__qcalcInbox) window.__qcalcInbox(\(Messages.shared.inboxJSON));
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
        // a game has the whole display; the page is asked for its size again when that ends
        guard let panel, gameRestoreFrame == nil else { return }
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
    // the page centres a bar this wide in the window (the window is wider while the smoke's room is open)
    private func tellPageBarWidth() {
        web?.evaluateJavaScript("document.documentElement.style.setProperty('--bar-width', '\(Int(overlayWidth))px')")
    }

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
        var width = fittedWidth(panel.frame.width + dx, on: panel.screen)
        // the dragged edge stops at the screen's edge; the other side never moves to make room
        if let visible = panel.screen?.visibleFrame {
            let room = side == .right ? visible.maxX - panel.frame.minX : panel.frame.maxX - visible.minX
            width = max(min(width, room), min(panel.frame.width, minOverlayWidth))
        }
        guard width != panel.frame.width else { return }
        var frame = panel.frame
        if side == .left { frame.origin.x = frame.maxX - width }
        frame.size.width = width
        overlayWidth = width
        tellPageBarWidth()
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
        guard let panel, gameRestoreFrame == nil, let event = NSApp.currentEvent else { return }
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
            // the periodic table never takes the keys, so while it has a formula ↵ in the bar puts that in instead of
            // submitting the line; with none built ↵ is the bar's as usual
            if event.window === self.panel, isPlainEnterKey(event), self.periodic.hasFormula,
               (self.panel?.firstResponder as? NSTextInputClient)?.hasMarkedText() != true {
                self.periodic.putFormulaIn()
                return nil
            }
            // ⌘W closes the bar like any window, whatever layer the page has open; settings keeps its own ⌘W.
            // a reference sheet (identities, calculus, the periodic table) never takes the keys while the bar is
            // up, so while one is open ⌘W closes the front one of those first
            // a game's window closes back to the bar, as its red button does, and ⌘M minimises it
            if event.window === self.panel, isCommandWCloseKey(event) {
                if let sheet = PeriodicPanel.front() {
                    sheet.close()
                } else if self.gameRestoreFrame != nil {
                    self.setGameWindow(false)
                } else {
                    self.hide(because: "⌘W")
                }
                return nil
            }
            if event.window === self.panel, self.gameRestoreFrame != nil, isCommandKey(event, "m", kVK_ANSI_M) {
                self.panel?.miniaturize(nil)
                return nil
            }
            // the settings key is the user's choice now, so the page reads it and posts openSettings
            return event
        }
    }

    // esc hides immediately, unless the page said it has a layer open; then the page closes that instead,
    // and a page that doesn't answer true still gets the panel hidden
    private func escape() {
        // out of a game's window first, back to the bar; the next esc is the game's or the bar's as usual
        if gameRestoreFrame != nil {
            setGameWindow(false)
            return
        }
        guard pageTakesEscape, let web else {
            hide(because: "esc")
            return
        }
        web.evaluateJavaScript("!!(window.__qcalcEscape && window.__qcalcEscape())") { [weak self] result, _ in
            if (result as? Bool) != true { self?.hide(because: "esc") }
        }
    }

    // a game gets a window of its own, large and centred on the bar's display, and gives it back. the same panel
    // turns into an ordinary titled window (close, minimise, zoom, resizable, the system's corners and shadow) with
    // the page running up under a clear title bar, and back into the bar after. the change happens in one step
    // behind a short fade (the page needs a frame or two to lay out at the new size, and a window animating its
    // frame would let a size report or a hide land halfway). `fromPage` is the page asking (⌃F), with its normal
    // size for the window's shape; anything else is the shell leaving on its own (esc, the close button, the
    // panel hiding), which the page is told so its state follows
    private func setGameWindow(_ on: Bool, fromPage: Bool = false, fade: Bool = true, natural: CGSize? = nil) {
        guard let panel else {
            gameRestoreFrame = nil
            return
        }
        if on {
            guard gameRestoreFrame == nil else { return }
            guard panel.isVisible, wantVisible, let web, panel.contentView === web,
                  let screen = panel.screen ?? Self.screenWithPointer() else {
                // not now (hidden, or the plain bar is up): the page mustn't think it has a window
                tellPageGameFullscreen(false)
                return
            }
            gameRestoreFrame = panel.frame
            let visible = screen.visibleFrame
            let frame = GameWindow.frame(visible: visible, natural: natural, titlebar: Self.titlebarHeight)
            OverlayLog.note("game window on \(NSStringFromRect(frame))")
            // before the frame changes, so the page's first layout at the new size already keeps the strip clear
            web.evaluateJavaScript("document.documentElement.style.setProperty('--game-fs-inset', '\(Int(Self.titlebarHeight))px')")
            moveForGame(panel, to: frame, fade: fade) {
                self.dressForGame(panel, true)
                panel.minSize = GameWindow.minSize(visible: visible, natural: natural, titlebar: Self.titlebarHeight)
            }
            focusGameWindow()
            return
        }
        guard let frame = gameRestoreFrame else { return }
        gameRestoreFrame = nil
        OverlayLog.note("game window off")
        web?.evaluateJavaScript("document.documentElement.style.removeProperty('--game-fs-inset')")
        if panel.isMiniaturized {
            // a minimised window can't be put back in place; a fresh panel takes the page where the bar was
            web?.layer?.cornerRadius = 12
            replacePanel("game window minimised", quiet: true)
            self.panel?.setFrame(frame, display: false)
        } else {
            moveForGame(panel, to: frame, fade: fade && panel.isVisible) { self.dressForGame(panel, false) }
            if sizeRoom == .closed { panel.invalidateShadow() }
            if panel.isVisible { panel.makeKeyAndOrderFront(nil) }
        }
        if !fromPage { tellPageGameFullscreen(false) }
        // size reports were ignored meanwhile, so ask for the current one
        web?.evaluateJavaScript("if (window.__qcalcSize) window.__qcalcSize();")
    }

    // the panel as a game's window, or as the bar
    private func dressForGame(_ panel: OverlayPanel, _ on: Bool) {
        panel.gameWindow = on
        panel.styleMask = on ? Self.gameStyle : Self.barStyle
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        // before the level, which isFloatingPanel sets too
        panel.isFloatingPanel = !on
        panel.level = on ? .normal : .statusBar
        // one space, and the green button zooms rather than going full screen
        panel.collectionBehavior = on ? [.fullScreenNone] : [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.isMovableByWindowBackground = !on
        // the system rounds a titled window's corners itself; a solid back, so they never show the desktop
        panel.backgroundColor = on ? .windowBackgroundColor : .clear
        panel.hasShadow = on || sizeRoom == .closed
        if !on { panel.minSize = NSSize(width: 1, height: 1) }
        web?.layer?.cornerRadius = on ? 0 : 12
    }

    // keys to the game, with the app in front, so the traffic lights are lit and the window takes the keys
    private func focusGameWindow() {
        guard let panel, gameRestoreFrame != nil else { return }
        activateForGame()
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(web)
    }

    // the bar never activates the app; a game's window does, like any window, and the app that was in front
    // gets it back when the bar goes
    private func activateForGame() {
        guard !NSApp.isActive else { return }
        if appBeforeGame == nil { appBeforeGame = NSWorkspace.shared.frontmostApplication }
        NSApp.activate(ignoringOtherApps: true)
    }

    private func moveForGame(_ panel: OverlayPanel, to frame: NSRect, fade: Bool, change: () -> Void = {}) {
        guard fade, panel.isVisible, panel.alphaValue == 1, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else {
            change()
            panel.setFrame(frame, display: true)
            return
        }
        panel.alphaValue = 0
        change()
        panel.setFrame(frame, display: true)
        // a hide in the meantime sets the alpha back itself, and a show starts its own reveal
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self, weak panel] in
            guard let self, let panel, panel === self.panel, panel.isVisible, self.wantVisible,
                  self.revealedShow == self.showCount else { return }
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.16
                context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                panel.animator().alphaValue = 1
            }
        }
    }

    private func tellPageGameFullscreen(_ on: Bool) {
        web?.evaluateJavaScript("if (window.__qcalcGameFullscreen) window.__qcalcGameFullscreen(\(on));")
    }

    private func installDragMonitor() {
        guard dragMonitor == nil else { return }
        dragMonitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window === panel else { return event }
            // a game's window moves and resizes by its own frame; its title bar strip is page underneath, so a
            // press there is taken here
            if self.gameRestoreFrame != nil { return self.gameWindowPress(panel, event) }
            guard !self.overControl else { return event }
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
            // a thin strip along the bottom moves it too, below the input's text
            let alongBottom = p.y <= body.minY + 5
            // either side edge drags the bar wider or narrower; the top strip still moves it
            let onLeft = p.x <= body.minX + self.resizeGrip
            let onRight = p.x >= body.maxX - self.resizeGrip
            if !alongTop, room == .closed, onLeft || onRight {
                self.trackResize(from: onLeft ? .left : .right)
                return nil
            }
            let topCorner = p.y >= body.maxY - 44 && (p.x <= body.minX + edge || p.x >= body.maxX - edge)
            if alongTop || topCorner || alongBottom {
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

    // a press in a game's window: the app comes forward (the panel doesn't do that by itself), and the title bar
    // strip drags the window, or zooms or minimises it on a double click as the system setting says
    private func gameWindowPress(_ panel: OverlayPanel, _ event: NSEvent) -> NSEvent? {
        activateForGame()
        let p = event.locationInWindow
        // the window's own edges, a few points in, still resize it
        let edge: CGFloat = 5
        guard let frameView = panel.contentView?.superview, p.y >= frameView.bounds.height - Self.titlebarHeight,
              p.y < frameView.bounds.height - edge, p.x > edge, p.x < frameView.bounds.width - edge,
              !(frameView.hitTest(p) is NSButton) else { return event }
        if event.clickCount == 2 {
            switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
            case "Minimize": panel.miniaturize(nil)
            case "None": break
            default: panel.zoom(nil)
            }
        } else {
            panel.performDrag(with: event)
        }
        return nil
    }

    private func installClickAwayMonitors() {
        guard clickAwayMonitor == nil else { return }
        clickAwayMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            guard self?.panel?.isVisible == true, self?.gameOpen == false else { return }
            self?.hide(because: "click in another app")
        }
        clickAwayLocalMonitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window !== panel,
                  !(event.window is PeriodicPanel), !self.gameOpen else {
                return event
            }
            self.hide(because: "click in another window")
            return event
        }
    }

    // on the display with the pointer, so it opens where the user is looking
    private func position() {
        userMoving = false
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
    private static let lastHiddenKey = "qcalc.lastHidden"

    // gone unopened longer than the setting: every remembered spot and the dragged width go, so this show opens in the
    // usual place at the usual width
    private func resetPlaceIfIdle() {
        let minutes = AppSettings.shared.placeResetMinutes
        let last = UserDefaults.standard.double(forKey: Self.lastHiddenKey)
        guard minutes > 0, last > 0, Date().timeIntervalSince1970 - last >= Double(minutes) * 60 else { return }
        let moved = UserDefaults.standard.object(forKey: Self.positionsKey) != nil || overlayWidth != defaultOverlayWidth
        guard moved else { return }
        UserDefaults.standard.removeObject(forKey: Self.positionsKey)
        UserDefaults.standard.removeObject(forKey: Self.overlayWidthKey)
        overlayWidth = defaultOverlayWidth
        tellPageBarWidth()
        if let panel, !panel.isVisible, sizeRoom == .closed {
            var frame = panel.frame
            frame.size.width = defaultOverlayWidth
            panel.setFrame(frame, display: false)
        }
        OverlayLog.note("back to the usual spot and width after \(minutes) min unopened")
    }

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
        observeMoves()
        userMoving = true
        panel.performDrag(with: event)
        if panel.frame.origin != before { rememberPosition() }
    }

    // performDrag can hand the drag to the window server and return before the window has moved, so the spot is kept
    // as the window moves, for as long as the bar stays up after the user grabbed it
    private func observeMoves() {
        guard moveObserver == nil else { return }
        moveObserver = NotificationCenter.default.addObserver(forName: NSWindow.didMoveNotification, object: nil, queue: .main) { [weak self] note in
            guard let self, self.userMoving, let panel = self.panel, note.object as? NSWindow === panel, panel.isVisible else { return }
            self.rememberPosition()
        }
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

    // messages go to the bar while it's up and no game has it, and to notifications otherwise
    private func tellMessagesBar() {
        let open = wantVisible && !gameOpen
        let opened = open && !Messages.shared.barOpen
        Messages.shared.barOpen = open
        // a game closing hands the bar what came meanwhile; a show does it as the page resets
        if opened, panel?.isVisible == true { pushInbox() }
    }

    private func pushInbox() {
        guard Messages.shared.barOpen, panel?.isVisible == true else { return }
        web?.evaluateJavaScript("if (window.__qcalcInbox) window.__qcalcInbox(\(Messages.shared.inboxJSON));")
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
        // a new name or list: the page sends with the name and completes the accepted ones
        messagingObserver = NotificationCenter.default.addObserver(
            forName: .qcalcMessagingChanged,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            let payload = Messages.shared.webJSON
            self?.web?.evaluateJavaScript(
                "window.__QCALC_MESSAGING = \(payload); if (window.__qcalcMessaging) window.__qcalcMessaging(\(payload));"
            )
        }
        inboxObserver = NotificationCenter.default.addObserver(
            forName: .qcalcInboxChanged,
            object: nil,
            queue: .main
        ) { [weak self] _ in self?.pushInbox() }
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
        let spaceThousands = AppSettings.shared.spaceThousands ? "true" : "false"
        let theme = AppSettings.shared.theme
        let angle = AppSettings.shared.angleMode
        let fractions = AppSettings.shared.fractionMode ? "true" : "false"
        return "{ sigFigs: \(n), draftSeconds: \(d), defaultUnits: \(units), answerForm: \"\(form)\", historyInsert: \"\(insert)\", historyShow: \"\(historyShow)\", rationalize: \(rationalize), sigFigMode: \(sigFigMode), theme: \"\(theme)\", angleMode: \"\(angle)\", fractionMode: \(fractions), keepWords: \(keepWords), typstPreview: \(typstPreview), typstCopy: \(typstCopy), copyUnitless: \(copyUnitless), spaceThousands: \(spaceThousands), shareUsage: \(shareUsage), keybinds: \(AppSettings.shared.keybindsJSON()), \(hotKeyJavaScriptFields()) }"
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
        if let spaceThousands = boolValue(dict["spaceThousands"]) {
            AppSettings.shared.setSpaceThousands(spaceThousands, notifyWeb: false)
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
                .noWritingTools()
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

extension View {
    // no writing tools, nor the "write with siri" prompt macos 26 puts beside a focused field
    @ViewBuilder func noWritingTools() -> some View {
        if #available(macOS 15.4, *) {
            writingToolsBehavior(.disabled).writingToolsAffordanceVisibility(.hidden)
        } else if #available(macOS 15.0, *) {
            writingToolsBehavior(.disabled)
        } else {
            self
        }
    }
}
