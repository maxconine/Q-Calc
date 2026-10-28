import AppKit

/// .quick-unit-tip: a word's full name over the line, the panel's colour on ink, 11 pt medium, 2×7 padding, the
/// small radius. A window of its own (a child of the bar's), so it can sit above the line's box without being
/// clipped; it never takes the pointer.
final class WordTipWindow: NSPanel {
    private let label = WordTipView()

    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 10, height: 10), styleMask: [.borderless, .nonactivatingPanel],
                   backing: .buffered, defer: true)
        isOpaque = false
        backgroundColor = .clear
        // a shadow sets the solid pill off the bar under it
        hasShadow = true
        alphaValue = 1
        ignoresMouseEvents = true
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        animationBehavior = .none
        contentView = label
        setAccessibilityElement(false)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    func show(_ text: String, at bottomLeft: NSPoint, over parent: NSWindow) {
        appearance = parent.effectiveAppearance
        label.text = text
        setFrame(NSRect(origin: bottomLeft, size: label.intrinsicContentSize), display: true)
        label.needsDisplay = true
        if self.parent !== parent {
            self.parent?.removeChildWindow(self)
            parent.addChildWindow(self, ordered: .above)
        }
        orderFront(nil)
    }

    func hide() {
        parent?.removeChildWindow(self)
        orderOut(nil)
    }
}

final class WordTipView: NSView {
    var text = "" { didSet { invalidateIntrinsicContentSize() } }
    private static let font = NSFont.systemFont(ofSize: Theme.caption, weight: .medium)
    private var attributes: [NSAttributedString.Key: Any] { [.font: Self.font, .foregroundColor: Theme.bg] }

    override var intrinsicContentSize: NSSize {
        let s = (text as NSString).size(withAttributes: attributes)
        // 7 pt either side, 2 pt above and below a 1.3 line
        return NSSize(width: ceil(s.width) + 14, height: ceil(Theme.caption * 1.3) + 4)
    }

    override func draw(_ dirtyRect: NSRect) {
        // fully opaque, whatever the appearance
        (Theme.ink.usingColorSpace(.sRGB)?.withAlphaComponent(1) ?? Theme.ink).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: Theme.radiusS, yRadius: Theme.radiusS).fill()
        let s = (text as NSString).size(withAttributes: attributes)
        (text as NSString).draw(at: NSPoint(x: 7, y: (bounds.height - s.height) / 2), withAttributes: attributes)
    }
}
