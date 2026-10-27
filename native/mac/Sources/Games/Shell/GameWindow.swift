import AppKit
import SwiftUI

/// A native game's window: the same window the web view had (titled, transparent title bar, sized by
/// `GameWindow.frame`), holding `GameShellView` around the game's view. Keys go to the shell (`GameShellModel.handle`)
/// before AppKit sees them (`GameNSWindow`), as the page's window listeners did; ⌘ shortcuts (⌘W, ⌘Q) still reach the menu.
final class NativeGameWindowController: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private var shell: GameShellModel?
    private var unzoomed: NSRect?
    var onClose: (() -> Void)?

    var isOpen: Bool { window != nil }

    func open(_ kind: String, on screen: NSScreen?, view: (GameShellModel) -> AnyView) {
        close()
        let shell = GameShellModel(kind: kind)
        self.shell = shell
        let root = GameShellView(shell: shell) { view(shell) }
        let host = NSHostingView(rootView: root)
        let window = GameNSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = BarCommand.gameList.first { $0.kind == kind }?.name ?? "Q Calc"
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.contentView = host
        window.backgroundColor = .windowBackgroundColor
        window.appearance = AppSettings.shared.nsAppearance
        window.minSize = NSSize(width: 480, height: 360)
        if let visible = (screen ?? NSScreen.main)?.visibleFrame {
            window.setFrame(GameWindow.frame(visible: visible, natural: CGSize(width: 900, height: 620), titlebar: 28), display: false)
        } else {
            window.center()
        }
        shell.onClose = { [weak self] in DispatchQueue.main.async { self?.close() } }
        shell.onFullscreen = { [weak self] on in self?.zoom(on) }
        self.window = window
        window.keys = { [weak shell] event in
            guard let shell else { return false }
            let key = GameKey(event: event)
            // the app's own shortcuts
            if key.meta { return false }
            if shell.handle(key) { return true }
            // a text field of the game's (24's input) types for itself; nothing else beeps
            return !(window.firstResponder is NSTextView)
        }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        OverlayLog.note("game \(kind) opened (native)")
    }

    private func zoom(_ on: Bool) {
        guard let window else { return }
        if on != window.isZoomed { window.zoom(nil) }
    }

    func close() {
        guard let window else { return }
        window.delegate = nil
        window.close()
        teardown()
    }

    func windowWillClose(_ notification: Notification) {
        teardown()
    }

    func windowDidResignKey(_ notification: Notification) {
        shell?.windowAway()
    }

    func windowDidMiniaturize(_ notification: Notification) {
        shell?.windowAway()
    }

    private func teardown() {
        window?.contentView = nil
        window = nil
        shell = nil
        OverlayLog.note("game closed (native)")
        onClose?()
    }
}

/// Hands every key to the shell before AppKit does, as the page's window listeners had them first (a window's
/// sendEvent sees real and synthesised events alike). `keys` returns true when the key was taken.
final class GameNSWindow: NSWindow {
    var keys: ((NSEvent) -> Bool)?

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown || event.type == .keyUp, keys?(event) == true { return }
        super.sendEvent(event)
    }
}
