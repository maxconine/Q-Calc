import AppKit
import SwiftUI

/// A canvas drawn on the CPU through Core Graphics into one bitmap that's handed to its layer, for a picture
/// redrawn every display frame. SwiftUI's Canvas renders each redraw into a GPU surface it keeps for a while, so a
/// dial animating at 60 fps briefly held ~30 window-sized surfaces (~125 MB) after each change; a bitmap in the
/// screen's own format and colour space is one image Core Animation shows as it is (as GraphPanel's plot).
/// `draw` gets a context in points, y down, with AppKit's current context set to it for text.
struct CGCanvas: NSViewRepresentable {
    let draw: (CGContext, CGSize) -> Void

    func makeNSView(context: Context) -> CGCanvasView { CGCanvasView() }

    func updateNSView(_ view: CGCanvasView, context: Context) {
        view.drawing = draw
        view.needsDisplay = true
    }

    final class CGCanvasView: NSView {
        var drawing: ((CGContext, CGSize) -> Void)?

        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override var isFlipped: Bool { true }
        override var wantsUpdateLayer: Bool { true }
        // the gestures on the SwiftUI side get every event
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidChangeBackingProperties() {
            super.viewDidChangeBackingProperties()
            needsDisplay = true
        }

        override func updateLayer() {
            guard let layer else { return }
            let scale = window?.backingScaleFactor ?? 2
            layer.contents = image(scale: scale)
            layer.contentsScale = scale
        }

        func image(scale: CGFloat) -> CGImage? {
            let w = Int((bounds.width * scale).rounded(.up)), h = Int((bounds.height * scale).rounded(.up))
            guard let drawing, w > 0, h > 0,
                  let space = window?.colorSpace?.cgColorSpace ?? CGColorSpace(name: CGColorSpace.sRGB),
                  let cg = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
            else { return nil }
            cg.translateBy(x: 0, y: CGFloat(h))
            cg.scaleBy(x: scale, y: -scale)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: true)
            drawing(cg, bounds.size)
            NSGraphicsContext.restoreGraphicsState()
            return cg.makeImage()
        }

        // cacheDisplay (DevSnapshot) draws through here: the same bitmap
        override func draw(_ dirtyRect: NSRect) {
            guard let cg = NSGraphicsContext.current?.cgContext,
                  let img = image(scale: window?.backingScaleFactor ?? 2) else { return }
            cg.saveGState()
            cg.translateBy(x: 0, y: bounds.height)
            cg.scaleBy(x: 1, y: -1)
            cg.draw(img, in: bounds)
            cg.restoreGState()
        }
    }
}

extension CGContext {
    /// SwiftUI's `GraphicsContext.draw(Text, at:, anchor:)` for one line: the line's box placed so `anchor` of it
    /// sits on `point` (the context is y down).
    func drawText(_ s: String, font: NSFont, color: CGColor, at point: CGPoint, anchor: UnitPoint) {
        let str = NSAttributedString(string: s, attributes: [.font: font, .foregroundColor: NSColor(cgColor: color) ?? .black])
        let size = str.size()
        str.draw(at: CGPoint(x: point.x - size.width * anchor.x, y: point.y - size.height * anchor.y))
    }
}
