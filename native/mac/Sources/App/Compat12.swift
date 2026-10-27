import AppKit
import SwiftUI

// macOS 12 stand-ins for SwiftUI pointer API that arrived in 13. On 13 and later each one is exactly the 13 API;
// on 12 an AppKit equivalent. native/mac/PARITY.md lists every such fork and what differs on 12.

extension View {
    /// `.onContinuousHover`: the pointer's location in this view while it's over it, nil when it leaves. On 12 a
    /// tracking area in an AppKit view over this one (which never takes a click).
    @ViewBuilder func pointerHover(_ action: @escaping (CGPoint?) -> Void) -> some View {
        if #available(macOS 13, *) {
            onContinuousHover { phase in
                switch phase {
                case let .active(p): action(p)
                case .ended: action(nil)
                }
            }
        } else {
            overlay(HoverTracker(action: action))
        }
    }

    /// `.gesture(SpatialTapGesture().onEnded { $0.location })`. On 12 a press that ends within a few points of
    /// where it began.
    @ViewBuilder func spatialTap(_ action: @escaping (CGPoint) -> Void) -> some View {
        if #available(macOS 13, *) {
            gesture(SpatialTapGesture().onEnded { action($0.location) })
        } else {
            gesture(DragGesture(minimumDistance: 0).onEnded { g in
                if hypot(g.translation.width, g.translation.height) < 4 { action(g.location) }
            })
        }
    }
}

private struct HoverTracker: NSViewRepresentable {
    let action: (CGPoint?) -> Void

    func makeNSView(context: Context) -> HoverTrackingView {
        let v = HoverTrackingView()
        v.action = action
        return v
    }

    func updateNSView(_ view: HoverTrackingView, context: Context) {
        view.action = action
    }
}

final class HoverTrackingView: NSView {
    var action: ((CGPoint?) -> Void)?

    override var isFlipped: Bool { true }
    // clicks, drags and scrolls go to the SwiftUI view under it
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                       owner: self))
    }

    override func mouseEntered(with event: NSEvent) { report(event) }
    override func mouseMoved(with event: NSEvent) { report(event) }
    override func mouseDragged(with event: NSEvent) { report(event) }
    override func mouseExited(with event: NSEvent) { action?(nil) }

    private func report(_ event: NSEvent) {
        action?(convert(event.locationInWindow, from: nil))
    }
}
