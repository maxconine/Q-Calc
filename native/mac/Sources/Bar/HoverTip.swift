import AppKit
import SwiftUI

/// The bar's hover label (native only), in place of AppKit's tooltips (`.help`), which come late and linger: it
/// shows after a short rest (`HoverTip.delay`) under what's hovered, in the word tip's ink pill (WordTipWindow),
/// and goes the moment the pointer leaves. The text is also the accessibility hint, as `.help`'s was.
struct HoverTip: ViewModifier {
    static let delay: TimeInterval = 0.6
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
            if Self.owner === self { Self.window.hide(); Self.owner = nil }
            return
        }
        timer = Timer.scheduledTimer(withTimeInterval: HoverTip.delay, repeats: false) { [weak self] _ in self?.show(text) }
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
