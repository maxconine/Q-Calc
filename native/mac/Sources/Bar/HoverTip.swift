import AppKit
import SwiftUI

/// The bar's hover label (native only), in place of AppKit's tooltips (`.help`), which come late and linger: it
/// shows after a short rest (`HoverTip.delay`) under what's hovered, in the word tip's ink pill (WordTipWindow),
/// and goes the moment the pointer leaves. The text is also the accessibility hint, as `.help`'s was.
struct HoverTip: ViewModifier {
    static let delay: TimeInterval = 0.6
    /// once a label has shown, the next shows at once for this long after the last was on screen
    static let warm: TimeInterval = 5
    private static var lastOnScreen: TimeInterval = -.infinity

    /// how long a label waits before it shows: none while one showed in the last 5 s
    static var wait: TimeInterval { ProcessInfo.processInfo.systemUptime - lastOnScreen < warm ? 0 : delay }
    /// a label (any kind: these, the edge tools', the word names) is on screen, or just went
    static func onScreen() { lastOnScreen = ProcessInfo.processInfo.systemUptime }

    let text: String
    @State private var anchor = HoverTipAnchor()

    func body(content: Content) -> some View {
        content
            .background(HoverTipSpot(anchor: anchor))
            .onHover { anchor.hover($0, text) }
            .onDisappear { anchor.hover(false, text) }
            .accessibilityHint(text)
    }
}

extension View {
    func hoverTip(_ text: String) -> some View { modifier(HoverTip(text: text)) }
}

final class HoverTipAnchor {
    weak var view: NSView?
    private var timer: Timer?
    private static let window = WordTipWindow()
    private static weak var owner: HoverTipAnchor?

    func hover(_ on: Bool, _ text: String) {
        timer?.invalidate()
        timer = nil
        guard on, !text.isEmpty else {
            if Self.owner === self {
                Self.window.hide()
                Self.owner = nil
                HoverTip.onScreen()
            }
            return
        }
        let wait = HoverTip.wait
        if wait == 0 { return show(text) }
        timer = Timer.scheduledTimer(withTimeInterval: wait, repeats: false) { [weak self] _ in self?.show(text) }
    }

    private func show(_ text: String) {
        guard let view, let window = view.window, window.isVisible else { return }
        let rect = window.convertToScreen(view.convert(view.bounds, to: nil))
        let visible = window.screen?.visibleFrame ?? rect
        let height = ceil(Theme.caption * 1.3) + 4
        // under what's hovered, 4 pt off; above it when there's no room below
        var y = rect.minY - 4 - height
        if y < visible.minY { y = rect.maxY + 4 }
        Self.owner = self
        HoverTip.onScreen()
        Self.window.show(text, at: NSPoint(x: max(visible.minX, rect.minX), y: y), over: window)
        // kept on screen at the right edge too
        let frame = Self.window.frame
        if frame.maxX > visible.maxX { Self.window.setFrameOrigin(NSPoint(x: visible.maxX - frame.width, y: frame.minY)) }
    }
}

/// Where the hovered view is, for the label's place
private struct HoverTipSpot: NSViewRepresentable {
    let anchor: HoverTipAnchor

    func makeNSView(context: Context) -> NSView {
        let v = PassThrough()
        anchor.view = v
        return v
    }

    func updateNSView(_ v: NSView, context: Context) { anchor.view = v }

    private final class PassThrough: NSView {
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
    }
}
