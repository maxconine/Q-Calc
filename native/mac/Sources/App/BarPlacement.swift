import CoreGraphics

/// Where the bar's window goes on a screen, kept apart from AppKit so `scripts/test-placement.sh` checks it headless:
/// the width fitted to the screen it opens on (a width dragged out on a big display no longer spills off a laptop's),
/// and a frame pulled back onto a screen after displays change (one unplugged, the Dock grown, a resolution switch).
enum BarPlacement {
    static let margin: CGFloat = 40

    /// The bar's width on a screen with this visible frame: never under `minimum`, never wider than the screen
    /// less the margin (still `minimum` on a screen too narrow for that)
    static func fittedWidth(_ width: CGFloat, minimum: CGFloat, visible: CGRect?) -> CGFloat {
        guard let visible else { return max(width, minimum) }
        return min(max(width, minimum), max(minimum, visible.width - margin))
    }

    /// The frame pulled inside `visible`: no wider or taller than fits, moved in from any edge it hangs over. A frame
    /// already inside is unchanged.
    static func keptOnScreen(_ frame: CGRect, minimumWidth: CGFloat, visible: CGRect) -> CGRect {
        var out = frame
        out.size.width = min(frame.width, fittedWidth(frame.width, minimum: minimumWidth, visible: visible))
        out.size.height = min(frame.height, visible.height)
        out.origin.x = min(max(out.minX, visible.minX), visible.maxX - out.width)
        out.origin.y = min(max(out.minY, visible.minY), visible.maxY - out.height)
        return out
    }

    /// Whether a frame needs pulling back: not wholly inside any screen's visible frame
    static func isOffScreen(_ frame: CGRect, visibleFrames: [CGRect]) -> Bool {
        !visibleFrames.contains { $0.contains(frame) }
    }

    /// The screen a frame is mostly on (by overlap), else nil when it touches none
    static func mostlyOn(_ frame: CGRect, screens: [CGRect]) -> Int? {
        var best: (index: Int, area: CGFloat)?
        for (i, screen) in screens.enumerated() {
            let overlap = screen.intersection(frame)
            guard !overlap.isNull, overlap.width > 0, overlap.height > 0 else { continue }
            let area = overlap.width * overlap.height
            if best == nil || area > best!.area { best = (i, area) }
        }
        return best?.index
    }
}
