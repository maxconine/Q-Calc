import AppKit
import CoreVideo
import QuartzCore
import SwiftUI

// The web canvas for the Swift games: a Core Animation layer drawn through Canvas2D every display frame. The
// layer draws asynchronously (Core Animation records the Core Graphics calls and replays them off the main
// thread), so a full-screen stage stays at 60 fps. `frame` runs first each display frame (tick the game), then
// `draw` paints, in canvas pixels, y down.

final class CanvasLayer: CALayer {
    var painter: ((Canvas2D) -> Void)?

    override init() {
        super.init()
        drawsAsynchronously = true
        needsDisplayOnBoundsChange = true
        isOpaque = false
    }

    override init(layer: Any) {
        super.init(layer: layer)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override func draw(in ctx: CGContext) {
        let s = contentsScale
        let w = Int((bounds.width * s).rounded()), h = Int((bounds.height * s).rounded())
        guard w >= 2, h >= 2, let painter else { return }
        ctx.saveGState()
        // canvas pixels, y down
        ctx.translateBy(x: 0, y: bounds.height)
        ctx.scaleBy(x: 1 / s, y: -1 / s)
        painter(Canvas2D(context: ctx, width: w, height: h))
        ctx.restoreGState()
    }
}

final class CanvasHostView: NSView {
    let canvas = CanvasLayer()
    var frameHook: (() -> Void)?
    private var link: CVDisplayLink?
    private var pending = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        layer = canvas
        wantsLayer = true
        canvas.contentsScale = NSScreen.main?.backingScaleFactor ?? 2
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    var dark: Bool { effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        canvas.contentsScale = window?.backingScaleFactor ?? canvas.contentsScale
        canvas.setNeedsDisplay()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window != nil { start() } else { stop() }
        viewDidChangeBackingProperties()
    }

    private func start() {
        guard link == nil else { return }
        var l: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&l)
        guard let l else { return }
        CVDisplayLinkSetOutputHandler(l) { [weak self] _, _, _, _, _ in
            guard let self, !self.pending else { return kCVReturnSuccess }
            self.pending = true
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.pending = false
                self.tick()
            }
            return kCVReturnSuccess
        }
        CVDisplayLinkStart(l)
        link = l
    }

    func stop() {
        if let link { CVDisplayLinkStop(link) }
        link = nil
    }

    deinit { stop() }

    private func tick() {
        frameHook?()
        canvas.setNeedsDisplay()
        canvas.displayIfNeeded()
    }
}

/// a canvas in a SwiftUI layout
struct GameCanvas: NSViewRepresentable {
    let frame: () -> Void
    let draw: (Canvas2D, Bool) -> Void

    func makeNSView(context: Context) -> CanvasHostView {
        let v = CanvasHostView(frame: .zero)
        update(v)
        return v
    }

    func updateNSView(_ v: CanvasHostView, context: Context) { update(v) }

    private func update(_ v: CanvasHostView) {
        v.frameHook = frame
        let draw = self.draw
        v.canvas.painter = { [weak v] g in draw(g, v?.dark ?? false) }
    }

    static func dismantleNSView(_ v: CanvasHostView, coordinator: ()) { v.stop() }
}

/// an offscreen picture, for thumbnails
func renderPicture(width: Int, height: Int, _ paint: (Canvas2D) -> Void) -> CGImage? {
    guard let c = Canvas2D(width: width, height: height) else { return nil }
    paint(c)
    return c.image()
}
