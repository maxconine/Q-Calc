import AppKit
import Carbon
import Combine
import SwiftUI

extension Notification.Name {
    static let focusOverlay = Notification.Name("QCalc.focusOverlay")
}

private func commandShiftHeld() -> Bool {
    let flags = NSEvent.modifierFlags.intersection(.deviceIndependentFlagsMask)
    return flags.contains(.command) && flags.contains(.shift)
}

/// The bar's window: a borderless, non-activating panel at status bar level on every space, as in Overlay.swift.
/// PeriodicWindow.swift looks for this class by name, so a sheet never takes the keys while the bar is up.
final class OverlayPanel: NSPanel {
    var onEscape: (() -> Void)?
    var onResignKey: (() -> Void)?
    var ignoreResignKey = false
    override var canBecomeKey: Bool { true }

    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) { onEscape?() }

    override func resignKey() {
        super.resignKey()
        if ignoreResignKey || commandShiftHeld() { return }
        onResignKey?()
    }
}

/// The panel's content: clear and rounded as the web view was; the SwiftUI bar sits on it.
private final class BarBackground: NSView {
    override var mouseDownCanMoveWindow: Bool { true }
}

/// Everything about the bar's window, kept from Overlay.swift: showing on the screen with the pointer, the
/// remembered spot per screen and width, edge resizing, dragging, click-away and focus-loss hiding, health checks,
/// and Esc / ⌘W. The page is gone: the content is an NSHostingView of BarView, and every message the page posted
/// is a direct call on BarModel.
final class OverlayController: NSObject {
    private var panel: OverlayPanel?
    private var host: NSHostingView<BarView>?
    let model: BarModel
    private var escapeMonitor: Any?
    private var dragMonitor: Any?
    private var clickAwayMonitor: Any?
    private var clickAwayLocalMonitor: Any?
    private var bag: Set<AnyCancellable> = []

    private let defaultOverlayWidth: CGFloat = 680
    private let minOverlayWidth: CGFloat = 340
    private static let overlayWidthKey = "qcalc.overlayWidth"
    private var overlayWidth: CGFloat = {
        let saved = CGFloat(UserDefaults.standard.double(forKey: OverlayController.overlayWidthKey))
        return saved > 0 ? max(340, saved) : 680
    }()
    private let resizeGrip: CGFloat = 8
    private let overlayMinHeight: CGFloat = 64
    private let overlayMaxHeight: CGFloat = 640
    private var sizeAnchorTop: CGFloat = 0
    private var lastSize = BarSize()
    private var userMoving = false
    private var moveObserver: NSObjectProtocol?
    private var wantVisible = false
    private var shownAt: TimeInterval = 0
    private var showCount = 0
    private var targetScreen: NSRect?
    private var recentShows: [TimeInterval] = []
    private var firstLayout: ((Int) -> Void)?
    var onSettings: (() -> Void)?
    var onMessagesSettings: (() -> Void)?
    var gameOpen = false
    var isShown: Bool { panel?.isVisible == true }

    private lazy var periodic: PeriodicWindowController = {
        let controller = PeriodicWindowController()
        controller.onPick = { [weak self] in self?.insertText($0) }
        return controller
    }()
    private var identitySheets: [String: IdentityWindowController] = [:]
    private lazy var games = GameWindowController()
    private lazy var effects = EggOverlay()

    init(model: BarModel) {
        self.model = model
        super.init()
        model.onHide = { [weak self] why in self?.hide(because: why) }
        model.onShow = { [weak self] why in self?.show(from: why) }
        model.onSettings = { [weak self] in
            track("settings.open")
            self?.onSettings?()
        }
        model.onMessagesSettings = { [weak self] in
            track("settings.open")
            self?.onMessagesSettings?()
        }
        model.onPeriodic = { [weak self] in
            guard let self else { return }
            self.periodic.show(SheetData.shared.elements, bar: self.barSpot())
        }
        model.onIdentities = { [weak self] id, section in
            guard let self, let sheet = SheetData.shared.identitySheet(id) else { return }
            self.identityWindow(id).show(sheet, section: section, bar: self.barSpot())
        }
        model.onGame = { [weak self] kind in self?.play(kind) }
        model.onEffect = { [weak self] kind in
            guard let self, let panel = self.panel, panel.isVisible else { return }
            let m = self.model
            self.effects.play(kind, over: panel, answer: (m.answerFrame, m.pretty(m.shownAnswer)))
        }
        model.onEffectInterrupt = { [weak self] in self?.effects.interrupt() }
        effects.onVeil = { [weak self] veiled in self?.model.answerVeiled = veiled }
        model.$crt.dropFirst().removeDuplicates().sink { [weak self] _ in
            DispatchQueue.main.async { self?.applyAppearance() }
        }.store(in: &bag)
        model.onTutorial = { [weak self] in self?.showTutorial() }
        games.onClose = { [weak self] in
            self?.gameOpen = false
            self?.tellMessagesBar()
        }
    }

    deinit {
        for monitor in [escapeMonitor, dragMonitor, clickAwayMonitor, clickAwayLocalMonitor].compactMap({ $0 }) {
            NSEvent.removeMonitor(monitor)
        }
    }

    // MARK: - building

    func preload() {
        if panel == nil { build() }
        ECBRates.shared.start()
    }

    private func build() {
        let panel = makePanel()
        installEscapeMonitor()
        installDragMonitor()
        installClickAwayMonitors()
        observe()
        self.panel = panel
        applyAppearance()
    }

    private func makePanel() -> OverlayPanel {
        let panel = OverlayPanel(
            contentRect: NSRect(x: 0, y: 0, width: overlayWidth, height: overlayMinHeight),
            styleMask: [.borderless, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isReleasedWhenClosed = false
        panel.title = "Q Calc"
        panel.onEscape = { [weak self] in self?.escape() }
        panel.onResignKey = { [weak self] in self?.lostFocus() }
        panel.isFloatingPanel = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.becomesKeyOnlyIfNeeded = false
        panel.setAccessibilityLabel("Q Calc")

        // the web view's layer: clear, 12 pt corners (makeWebView); the bar paints its own solid --bg card
        let background = BarBackground()
        background.wantsLayer = true
        background.layer?.backgroundColor = NSColor.clear.cgColor
        background.layer?.cornerRadius = Theme.radius
        background.layer?.masksToBounds = true
        background.autoresizingMask = [.width, .height]

        let host = NSHostingView(rootView: BarView(model: model) { [weak self] size in self?.barResized(size) })
        host.autoresizingMask = [.width, .height]
        if #available(macOS 13.0, *) { host.sizingOptions = [] }
        background.addSubview(host)
        host.frame = background.bounds
        panel.contentView = background
        self.host = host
        return panel
    }

    // a fresh panel around the same bar, for when the old one stops coming forward (seen after long sleeps)
    private func replacePanel(_ why: String) {
        guard let old = panel else { return }
        OverlayLog.note("fresh panel (\(why))")
        old.ignoreResignKey = true
        old.orderOut(nil)
        old.contentView = NSView()
        panel = makePanel()
        applyAppearance()
    }

    private func observe() {
        NotificationCenter.default.publisher(for: .qcalcSettingsChanged)
            .merge(with: AppSettings.shared.objectWillChange.map { _ in Notification(name: .qcalcSettingsChanged) })
            .debounce(for: .milliseconds(1), scheduler: RunLoop.main)
            .sink { [weak self] _ in self?.applyAppearance() }
            .store(in: &bag)
        let workspace = NSWorkspace.shared.notificationCenter
        workspace.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.environmentChanged("wake")
        }
        // Increase Contrast / Reduce Transparency switched: the bar redraws with Theme's other colours
        workspace.addObserver(forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.model.objectWillChange.send()
        }
        NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main) { [weak self] _ in
            self?.environmentChanged("display change")
        }
    }

    private func environmentChanged(_ why: String) {
        guard !wantVisible else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
            guard let self, !self.wantVisible, self.panel?.isVisible != true else { return }
            self.replacePanel(why)
        }
    }

    /// dev builds (`drive.sh contrast:on`): the bar drawn as with Increase Contrast on, to picture it
    var devContrast = false {
        didSet {
            Theme.devContrast = devContrast
            applyAppearance()
            model.objectWillChange.send()
        }
    }

    private func applyAppearance() {
        // the Konami CRT reads as phosphor on black: it always draws on the dark bar
        panel?.appearance = model.crt ? NSAppearance(named: .darkAqua) : AppSettings.shared.nsAppearance
        if devContrast, let panel {
            let dark = panel.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            panel.appearance = NSAppearance(named: dark ? .accessibilityHighContrastDarkAqua : .accessibilityHighContrastAqua)
        }
        if let background = panel?.contentView { CRTLook.apply(model.crt, to: background) }
    }

    // MARK: - showing and hiding

    func toggle(from source: String = "hotkey") {
        if let panel, panel.isVisible, panel.isOnActiveSpace, panel.alphaValue > 0 {
            hide(because: source)
            return
        }
        if let panel, panel.isVisible, !panel.isOnActiveSpace { replacePanel("ordered in on another space") }
        let now = uptime()
        recentShows = recentShows.filter { now - $0 < 4 } + [now]
        if recentShows.count >= 3 {
            recentShows = []
            replacePanel("shown three times in four seconds")
        }
        show(from: source)
    }

    func show(from source: String = "menu") {
        if panel == nil { build() }
        if NSApp.isHidden { NSApp.unhideWithoutActivation() }
        wantVisible = true
        tellMessagesBar()
        shownAt = uptime()
        targetScreen = Self.screenWithPointer()?.frame
        resetPlaceIfIdle()
        model.willShow()
        showOnce()
        OverlayLog.note("show \(showCount) (\(source))")
        verify(showCount, attempt: 0)
    }

    private func showOnce() {
        guard let panel else { return }
        sizeAnchorTop = 0
        applySize(lastSize.height > 0 ? lastSize : BarSize(height: overlayMinHeight, anchorTop: 0))
        position()
        panel.ignoreResignKey = true
        showCount += 1
        panel.alphaValue = 0
        panel.orderFrontRegardless()
        panel.makeKeyAndOrderFront(nil)
        model.focusField?()
        // one layout pass so the tape is in place, then fade in (the page waited for its size message)
        let token = showCount
        DispatchQueue.main.async { [weak self] in
            self?.model.focusField?()
            self?.panel?.ignoreResignKey = false
            self?.reveal(token)
        }
        NotificationCenter.default.post(name: .focusOverlay, object: nil)
    }

    private func reveal(_ token: Int) {
        guard token == showCount, let panel, panel.isVisible else { return }
        if NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            panel.alphaValue = 1
            return
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.1
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            panel.animator().alphaValue = 1
        }
    }

    func hide(because why: String = "asked") {
        wantVisible = false
        tellMessagesBar()
        userMoving = false
        if panel?.isVisible == true { UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: Self.lastHiddenKey) }
        guard panel?.isVisible == true else { return }
        OverlayLog.note("hide \(showCount) (\(why)) after \(String(format: "%.2fs", uptime() - shownAt))")
        model.willHide()
        effects.stop()
        panel?.ignoreResignKey = true
        panel?.orderOut(nil)
        panel?.alphaValue = 1
    }

    // OverlayHealth judges the show from what the window server says; the "page" fields are always fine now
    private func verify(_ token: Int, attempt: Int) {
        DispatchQueue.main.asyncAfter(deadline: .now() + (attempt == 0 ? 0.4 : 0.5)) { [weak self] in
            guard let self, token == self.showCount, self.wantVisible, let panel = self.panel else { return }
            var s = OverlaySnapshot()
            s.ordered = panel.isVisible
            s.appHidden = NSApp.isHidden
            s.onActiveSpace = panel.isOnActiveSpace
            s.windowServerOnScreen = Self.windowServerShows(panel)
            s.occlusionVisible = panel.occlusionState.contains(.visible)
            s.alpha = Double(panel.alphaValue)
            s.frame = panel.frame
            s.targetScreen = self.targetScreen
            s.screens = NSScreen.screens.map(\.frame)
            let problems = OverlayHealth.problems(s)
            guard !problems.isEmpty else { return }
            let fix = OverlayHealth.recovery(for: problems, attempt: attempt)
            OverlayLog.note("show \(token) not seen (\(OverlayHealth.summary(problems))), \(fix.name)")
            switch fix {
            case .none:
                if panel.isVisible, panel.alphaValue < 1 { panel.alphaValue = 1 }
                return
            case .wait:
                self.verify(token, attempt: attempt + 1)
                return
            case .reorder:
                break
            case .freshPanel, .freshPage, .fallback:
                self.replacePanel(OverlayHealth.summary(problems))
            }
            self.showOnce()
            self.verify(self.showCount, attempt: attempt + 1)
        }
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

    private func lostFocus() {
        guard wantVisible, panel?.isVisible == true, !gameOpen else { return }
        let since = uptime() - shownAt
        if OverlayHealth.focusLossHides(sinceShow: since) {
            hide(because: "lost focus")
        }
    }

    // the first run leads the examples with `Press <shortcut> anytime` (exampleList's firstRunHotkey)
    func showFirstRun() {
        model.firstRunHotkey = AppSettings.shared.activeHotKey?.title
        show(from: "first run")
    }

    func showTips() {
        show(from: "tips")
        model.text = "help"
    }

    func showTutorial() {
        show(from: "tutorial")
        model.startTutorial()
    }

    // MARK: - size and place

    private func barResized(_ size: BarSize) {
        guard size.height > 0 else { return }
        lastSize = size
        applySize(size)
    }

    // the composer stays where it is: the tape grows up, panels grow down (Overlay.swift applySize, no smoke room)
    private func applySize(_ size: BarSize) {
        guard let panel else { return }
        let h = min(max(size.height.rounded(.up), overlayMinHeight), overlayMaxHeight)
        let nextAnchor = min(max(size.anchorTop, 0), h - overlayMinHeight)
        let composerTop = panel.frame.maxY - sizeAnchorTop
        var frame = NSRect(
            x: panel.frame.minX,
            y: composerTop + nextAnchor - h,
            width: fittedWidth(overlayWidth, on: panel.screen),
            height: h
        )
        if let screen = panel.screen?.visibleFrame ?? NSScreen.main?.visibleFrame {
            frame.origin.y = min(max(frame.minY, screen.minY), screen.maxY - frame.height)
            frame.origin.x = min(max(frame.minX, screen.minX), screen.maxX - frame.width)
        }
        sizeAnchorTop = nextAnchor
        guard frame != panel.frame else { return }
        panel.setFrame(frame, display: true)
        panel.invalidateShadow()
    }

    private func fittedWidth(_ width: CGFloat, on screen: NSScreen?) -> CGFloat {
        guard let visible = (screen ?? NSScreen.main)?.visibleFrame else { return max(width, minOverlayWidth) }
        return min(max(width, minOverlayWidth), max(minOverlayWidth, visible.width - 40))
    }

    private static let positionsKey = "overlayPositions"
    private static let lastHiddenKey = "qcalc.lastHidden"

    private func position() {
        userMoving = false
        guard let panel,
              let main = NSScreen.screens.first(where: { $0.frame == targetScreen }) ?? Self.screenWithPointer()
        else { return }
        let screen = main.visibleFrame
        let size = panel.frame.size
        if let saved = savedComposerTop(on: main) {
            let x = min(max(saved.x, screen.minX), screen.maxX - size.width)
            let top = min(max(saved.y, screen.minY + size.height), screen.maxY)
            panel.setFrameOrigin(NSPoint(x: x, y: top + sizeAnchorTop - size.height))
            return
        }
        let x = screen.midX - size.width / 2
        let y = screen.minY + screen.height * 0.72 + sizeAnchorTop - size.height
        panel.setFrameOrigin(NSPoint(x: x, y: y))
    }

    private func resetPlaceIfIdle() {
        let minutes = AppSettings.shared.placeResetMinutes
        let last = UserDefaults.standard.double(forKey: Self.lastHiddenKey)
        guard minutes > 0, last > 0, Date().timeIntervalSince1970 - last >= Double(minutes) * 60 else { return }
        guard UserDefaults.standard.object(forKey: Self.positionsKey) != nil || overlayWidth != defaultOverlayWidth else { return }
        UserDefaults.standard.removeObject(forKey: Self.positionsKey)
        UserDefaults.standard.removeObject(forKey: Self.overlayWidthKey)
        overlayWidth = defaultOverlayWidth
        if let panel, !panel.isVisible {
            var frame = panel.frame
            frame.size.width = defaultOverlayWidth
            panel.setFrame(frame, display: false)
        }
        OverlayLog.note("back to the usual spot and width after \(minutes) min unopened")
    }

    private func screenKey(_ screen: NSScreen) -> String? {
        (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.stringValue
    }

    private func savedComposerTop(on screen: NSScreen) -> NSPoint? {
        guard let key = screenKey(screen),
              let all = UserDefaults.standard.dictionary(forKey: Self.positionsKey),
              let pair = all[key] as? [Double], pair.count == 2 else { return nil }
        let origin = screen.visibleFrame.origin
        return NSPoint(x: origin.x + pair[0], y: origin.y + pair[1])
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
        let visible = screen.visibleFrame
        let top = visible.minY + visible.height * 0.72
        panel.setFrameOrigin(NSPoint(x: visible.midX - panel.frame.width / 2, y: top + sizeAnchorTop - panel.frame.height))
    }

    private func dragAndRemember(_ panel: OverlayPanel, _ event: NSEvent) {
        let before = panel.frame.origin
        if moveObserver == nil {
            moveObserver = NotificationCenter.default.addObserver(forName: NSWindow.didMoveNotification, object: nil, queue: .main) { [weak self] note in
                guard let self, self.userMoving, let panel = self.panel, note.object as? NSWindow === panel, panel.isVisible else { return }
                self.rememberPosition()
            }
        }
        userMoving = true
        panel.performDrag(with: event)
        if panel.frame.origin != before { rememberPosition() }
    }

    private enum ResizeSide { case left, right }

    private func trackResize(from side: ResizeSide) {
        guard let panel else { return }
        var lastX = NSEvent.mouseLocation.x
        panel.trackEvents(matching: [.leftMouseDragged, .leftMouseUp], timeout: .greatestFiniteMagnitude, mode: .eventTracking) { event, stop in
            guard let event else { return }
            if event.type == .leftMouseUp {
                stop.pointee = true
                return
            }
            let x = NSEvent.mouseLocation.x
            self.resizeBar(by: side == .right ? x - lastX : lastX - x, from: side)
            lastX = x
        }
        UserDefaults.standard.set(Double(overlayWidth), forKey: Self.overlayWidthKey)
        if side == .left { rememberPosition() }
    }

    private func resizeBar(by dx: CGFloat, from side: ResizeSide) {
        guard let panel, dx != 0 else { return }
        var width = fittedWidth(panel.frame.width + dx, on: panel.screen)
        if let visible = panel.screen?.visibleFrame {
            let room = side == .right ? visible.maxX - panel.frame.minX : panel.frame.maxX - visible.minX
            width = max(min(width, room), min(panel.frame.width, minOverlayWidth))
        }
        guard width != panel.frame.width else { return }
        var frame = panel.frame
        if side == .left { frame.origin.x = frame.maxX - width }
        frame.size.width = width
        overlayWidth = width
        panel.setFrame(frame, display: true)
        panel.invalidateShadow()
    }

    // MARK: - mouse and keys

    // the edges resize, the top and bottom strips (and the corners) move it; double-click there puts it back
    private func installDragMonitor() {
        guard dragMonitor == nil else { return }
        dragMonitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window === panel else { return event }
            let p = event.locationInWindow
            let size = panel.frame.size
            let alongTop = p.y >= size.height - 10
            let alongBottom = p.y <= 5
            let onLeft = p.x <= self.resizeGrip
            let onRight = p.x >= size.width - self.resizeGrip
            if !alongTop, onLeft || onRight {
                self.trackResize(from: onLeft ? .left : .right)
                return nil
            }
            if alongTop || alongBottom {
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
            guard self?.panel?.isVisible == true, self?.gameOpen == false else { return }
            self?.hide(because: "click in another app")
        }
        clickAwayLocalMonitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window !== panel,
                  !(event.window is PeriodicPanel), !self.gameOpen else { return event }
            self.hide(because: "click in another window")
            return event
        }
    }

    private func installEscapeMonitor() {
        guard escapeMonitor == nil else { return }
        escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, let panel = self.panel, panel.isVisible, event.window === panel else { return event }
            // the Konami code watches every key here, once each, before anything takes it: the arrows never reach
            // the window, since AppKit offers them to the field as key equivalents first and the bar uses them up
            self.model.konamiKey(event)
            let composing = (panel.firstResponder as? NSTextInputClient)?.hasMarkedText() == true
            if event.keyCode == UInt16(kVK_Escape), !composing {
                self.escape()
                return nil
            }
            // the periodic table never takes the keys, so while it has a formula ↵ puts that in rather than
            // committing the line; with none built ↵ is the bar's as usual
            if isPlainEnterKey(event), self.periodic.hasFormula, !composing {
                self.periodic.putFormulaIn()
                return nil
            }
            // ⌘V with files copied sends them, in a thread or on a line that names someone; otherwise it pastes
            if event.modifierFlags.intersection([.command, .option, .control, .shift]) == [.command],
               event.keyCode == UInt16(kVK_ANSI_V), !composing, self.model.pasteFiles() {
                return nil
            }
            // ⌘W closes the front reference sheet first (sheets never take the keys while the bar is up), else
            // hides the bar
            if isCommandWCloseKey(event) {
                if let sheet = PeriodicPanel.front() {
                    sheet.close()
                } else if self.model.closeMessages() {
                    // a message thread or the conversations list closes before the bar does
                } else {
                    self.hide(because: "⌘W")
                }
                return nil
            }
            return event
        }
    }

    private func escape() {
        if !model.escape() { hide(because: "esc") }
    }

    // MARK: - sheets, games, messages

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

    private func insertText(_ text: String) {
        if panel?.isVisible != true { show(from: "periodic table") }
        model.insert(text)
    }

    private func play(_ kind: String) {
        let screen = panel?.screen ?? Self.screenWithPointer()
        hide(because: "game")
        gameOpen = true
        tellMessagesBar()
        games.open(kind, on: screen)
    }

    /// dev builds: closes an open game window (the bench's "closegame")
    func closeGame() {
        games.close()
    }

    // messages go to the bar while it's up and no game has it, and to notifications otherwise
    private func tellMessagesBar() {
        let open = wantVisible && !gameOpen
        let opened = open && !Messages.shared.barOpen
        Messages.shared.barOpen = open
        if opened, panel?.isVisible == true { model.pullInbox() }
    }
}
