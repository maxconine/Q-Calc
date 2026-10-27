import AppKit
import SwiftUI

/// A native game's window: the same window the web view had (titled, transparent title bar, sized by
/// `GameWindow.frame`), holding `GameShellView` around the game's view. Keys go to the shell (`GameShellModel.handle`)
/// before AppKit sees them, as the page's window listeners did; ⌘ shortcuts (⌘W, ⌘Q) still reach the menu.
final class NativeGameWindowController: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private var shell: GameShellModel?
    private var monitor: Any?
    private var unzoomed: NSRect?
    var onClose: (() -> Void)?

    var isOpen: Bool { window != nil }

    func open(_ kind: String, on screen: NSScreen?, view: (GameShellModel) -> AnyView) {
        close()
        let shell = GameShellModel(kind: kind)
        self.shell = shell
        let root = GameShellView(shell: shell) { view(shell) }
        let host = NSHostingView(rootView: root)
        let window = NSWindow(
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
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .keyUp]) { [weak self] event in
            guard let self, let window = self.window, event.window === window, let shell = self.shell else { return event }
            let key = GameKey(event: event)
            // the app's own shortcuts
            if key.meta { return event }
            // a text field of the game's (24's input) types for itself unless the game takes the key first
            let typing = window.firstResponder is NSTextView
            if shell.handle(key) { return nil }
            return typing ? event : nil
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
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        window?.contentView = nil
        window = nil
        shell = nil
        OverlayLog.note("game closed (native)")
        onClose?()
    }
}
