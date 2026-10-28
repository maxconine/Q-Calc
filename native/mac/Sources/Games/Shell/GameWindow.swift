import AppKit
import Combine
import SwiftUI

/// Where the bar was when a game opened: its left edge, width and the composer's top, on its screen. The game
/// takes the bar's place there, as the page's game took the panel (the composer hidden, the panel growing down
/// from where the composer's top was).
struct GameBarSpot {
    var x: CGFloat
    var width: CGFloat
    var top: CGFloat
    var visible: NSRect
}

/// A native game's window. Normally it is the bar: a borderless card at the bar's spot and width, as tall as the
/// game, at the bar's level (the page drew the game in the bar's own panel). Full screen (⌃F, f, the ⛶ buttons)
/// turns it into the big window the web-view app made of the panel (`setGameWindow` in Overlay.swift): titled,
/// centred, resizable; esc, ⌃F, ⌘W or its red button bring it back to the bar. `GameShellView` around the game's
/// view draws either. Keys go to the shell (`GameShellModel.handle`) before AppKit sees them (`GameNSWindow`), as
/// the page's window listeners did; other ⌘ shortcuts still reach the menu.
final class NativeGameWindowController: NSObject, NSWindowDelegate {
    private var window: GameNSWindow?
    private var container: NSView?
    private var shell: GameShellModel?
    private var spot: GameBarSpot?
    private var cardHeight: CGFloat = 0
    private var themeWatch: AnyCancellable?
    /// hidden with the bar (the hotkey) while the game must stay up, to come back on the next show
    private(set) var held = false
    /// The game closed: true when it goes back to the bar (esc from the lobby), false when the bar goes too.
    var onClose: ((Bool) -> Void)?

    var isOpen: Bool { window != nil }
    var isFullscreen: Bool { shell?.fullscreen ?? false }

    private static let barStyle: NSWindow.StyleMask = [.borderless, .fullSizeContentView]
    private static let bigStyle: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]

    func open(_ kind: String, at spot: GameBarSpot, view: (GameShellModel) -> AnyView) {
        close(backToBar: false, quiet: true)
        let shell = GameShellModel(kind: kind)
        self.shell = shell
        self.spot = spot
        cardHeight = 0
        held = false
        let root = GameShellView(shell: shell) { view(shell) }
        let host = NSHostingView(rootView: root)
        if #available(macOS 13.0, *) { host.sizingOptions = [] }
        host.autoresizingMask = [.width, .height]
        // clear and rounded as the bar's own content is; the window's shadow follows the card
        let container = NSView()
        container.wantsLayer = true
        container.layer?.cornerRadius = Theme.radius
        container.layer?.masksToBounds = true
        container.addSubview(host)
        let window = GameNSWindow(
            contentRect: NSRect(x: spot.x, y: spot.top - 300, width: spot.width, height: 300),
            styleMask: Self.barStyle,
            backing: .buffered,
            defer: false
        )
        window.title = BarCommand.gameList.first { $0.kind == kind }?.name ?? "Q Calc"
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.contentView = container
        host.frame = container.bounds
        window.appearance = AppSettings.shared.nsAppearance
        // the theme follows settings while the game is up, as the bar's does
        themeWatch = NotificationCenter.default.publisher(for: .qcalcSettingsChanged)
            .merge(with: AppSettings.shared.objectWillChange.map { _ in Notification(name: .qcalcSettingsChanged) })
            .debounce(for: .milliseconds(1), scheduler: RunLoop.main)
            .sink { [weak self] _ in self?.window?.appearance = AppSettings.shared.nsAppearance }
        self.window = window
        self.container = container
        dress(big: false)
        shell.onClose = { [weak self] in DispatchQueue.main.async { self?.close(backToBar: true) } }
        shell.onFullscreen = { [weak self] on in self?.setBig(on) }
        shell.onCardHeight = { [weak self] h in self?.cardResized(h) }
        window.onPerformClose = { [weak self] in self?.closeOrLeaveBig() }
        window.keys = { [weak self, weak shell] event in
            guard let self, let shell, let window = self.window else { return false }
            // ⌘W: out of the big window back to the bar, as its red button; from the bar it goes with the bar
            if event.type == .keyDown, isCommandWCloseKey(event) {
                self.closeOrLeaveBig()
                return true
            }
            let key = GameKey(event: event)
            // the app's own shortcuts
            if key.meta { return false }
            if shell.handle(key) { return true }
            // a text field of the game's (24's input) types for itself; nothing else beeps
            return !(window.firstResponder is NSTextView)
        }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        OverlayLog.note("game \(kind) opened (native) in the bar's place at \(Int(spot.x)),\(Int(spot.top)) w \(Int(spot.width))")
    }

    // the card's height, measured by GameShellView: the window is as tall as the game, hanging from the composer's top
    private func cardResized(_ h: CGFloat) {
        guard h > 0, abs(h.rounded(.up) - cardHeight) > 0.5 else { return }
        cardHeight = h.rounded(.up)
        guard let window, !isFullscreen else { return }
        window.setFrame(barFrame(), display: true)
        window.invalidateShadow()
    }

    private func barFrame() -> NSRect {
        guard let spot else { return window?.frame ?? .zero }
        let visible = spot.visible
        let h = min(max(cardHeight, 64), visible.height)
        var frame = NSRect(x: spot.x, y: spot.top - h, width: spot.width, height: h)
        frame.origin.y = min(max(frame.minY, visible.minY), visible.maxY - h)
        frame.origin.x = min(max(frame.minX, visible.minX), max(visible.minX, visible.maxX - frame.width))
        return frame
    }

    // the window as the bar's card, or as a big ordinary window
    private func dress(big: Bool) {
        guard let window else { return }
        window.styleMask = big ? Self.bigStyle : Self.barStyle
        if big {
            // the name stays for the Window menu; the page draws its own heading, as the web panel did
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
        }
        window.level = big ? .normal : .statusBar
        window.collectionBehavior = big ? [.fullScreenNone] : [.canJoinAllSpaces, .fullScreenAuxiliary]
        window.isOpaque = false
        window.backgroundColor = big ? .windowBackgroundColor : .clear
        window.hasShadow = true
        window.isMovableByWindowBackground = false
        container?.layer?.cornerRadius = big ? 0 : Theme.radius
    }

    private func setBig(_ on: Bool) {
        guard let window else { return }
        if on {
            let visible = window.screen?.visibleFrame ?? spot?.visible ?? NSScreen.main?.visibleFrame ?? .zero
            // the card's size is the shape the big window keeps (the page's natural size)
            let natural = window.frame.size
            let frame = GameWindow.frame(visible: visible, natural: natural, titlebar: GameWindow.titlebarHeight)
            OverlayLog.note("game window on \(NSStringFromRect(frame))")
            dress(big: true)
            window.minSize = GameWindow.minSize(visible: visible, natural: natural, titlebar: GameWindow.titlebarHeight)
            window.setFrame(frame, display: true)
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
        } else {
            OverlayLog.note("game window off")
            if window.isMiniaturized { window.deminiaturize(nil) }
            window.minSize = NSSize(width: 1, height: 1)
            dress(big: false)
            window.setFrame(barFrame(), display: true)
            window.makeKeyAndOrderFront(nil)
        }
        window.invalidateShadow()
    }

    private func closeOrLeaveBig() {
        if isFullscreen { shell?.exitFullscreen() } else { close(backToBar: false) }
    }

    /// The hotkey while the game is up: a big window minimised or behind comes forward; otherwise it goes as the
    /// bar would, back to the bar's size first, and stays held when someone's waiting or playing online.
    func hotkey() {
        guard let window, let shell else { return }
        if held { reshow(); return }
        if window.isMiniaturized { window.deminiaturize(nil); return }
        if !window.isKeyWindow || !window.isOnActiveSpace {
            reshow()
            return
        }
        if shell.fullscreen { shell.exitFullscreen() }
        if shell.holds {
            shell.windowAway()
            held = true
            window.orderOut(nil)
            OverlayLog.note("game hidden with the bar (held)")
        } else {
            close(backToBar: false)
        }
    }

    /// Back in front: a held game, or a show while the game is up.
    func reshow() {
        guard let window else { return }
        held = false
        if window.isMiniaturized { window.deminiaturize(nil) }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func close(backToBar: Bool = false, quiet: Bool = false) {
        guard let window else { return }
        window.delegate = nil
        window.close()
        teardown(backToBar: backToBar, quiet: quiet)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        // the big window's red button goes back to the bar, as the web-view app's did
        if isFullscreen {
            shell?.exitFullscreen()
            return false
        }
        return true
    }

    func windowWillClose(_ notification: Notification) {
        teardown(backToBar: false, quiet: false)
    }

    func windowDidResignKey(_ notification: Notification) {
        shell?.windowAway()
    }

    func windowDidMiniaturize(_ notification: Notification) {
        shell?.windowAway()
    }

    private func teardown(backToBar: Bool, quiet: Bool) {
        window?.contentView = nil
        window = nil
        themeWatch = nil
        container = nil
        shell = nil
        spot = nil
        held = false
        OverlayLog.note("game closed (native)")
        if !quiet { onClose?(backToBar) }
    }
}

/// Hands every key to the shell before AppKit does, as the page's window listeners had them first (a window's
/// sendEvent sees real and synthesised events alike). `keys` returns true when the key was taken.
final class GameNSWindow: NSWindow {
    var keys: ((NSEvent) -> Bool)?
    var onPerformClose: (() -> Void)?

    // borderless while it's the bar's card, and it still takes the keys
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    // the red button and ⌘W: back to the bar from the big window, closed from the bar
    override func performClose(_ sender: Any?) {
        if let onPerformClose { onPerformClose() } else { super.performClose(sender) }
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown || event.type == .keyUp, keys?(event) == true { return }
        super.sendEvent(event)
    }
}

extension GameWindow {
    // a titled window's title bar strip, which the page keeps clear
    static let titlebarHeight: CGFloat = {
        let content = NSRect(x: 0, y: 0, width: 400, height: 400)
        return NSWindow.frameRect(forContentRect: content, styleMask: [.titled]).height - content.height
    }()
}
