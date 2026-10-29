import AppKit
import CoreGraphics
import CoreText

// The web canvas's 2D context over Core Graphics, so the games' drawing code (emberDraw.ts, emberFx.ts, the trails
// arena, the 420/67/69 effects) ports line for line and paints the same pixels: WebKit's own canvas is Core
// Graphics underneath. The same rules as the web: the current path is kept in device space as it's built (each
// point through the transform of the moment), fills and strokes don't consume it, lineWidth, dashes and
// gradients follow the transform at the time of the fill or stroke, and shadows ignore it.
//
// Coordinates are canvas pixels, y down, from the top-left. A Canvas2D draws into any CGContext: its own bitmap
// (an offscreen canvas, drawImage-able), or the layer context Core Animation hands GameCanvasLayer each frame.

struct Col: Equatable, Hashable {
    /// 0–255 like css, alpha 0–1
    var r: Double, g: Double, b: Double, a: Double

    init(_ r: Double, _ g: Double, _ b: Double, _ a: Double = 1) {
        self.r = r
        self.g = g
        self.b = b
        self.a = a
    }

    static let clear = Col(0, 0, 0, 0)
    static let black = Col(0, 0, 0)
    static let white = Col(255, 255, 255)

    func alpha(_ a: Double) -> Col { Col(r, g, b, a) }

    var cg: CGColor {
        CGColor(colorSpace: Canvas2D.space, components: [CGFloat(r / 255), CGFloat(g / 255), CGFloat(b / 255), CGFloat(min(1, max(0, a)))])
            ?? CGColor(gray: 0, alpha: 1)
    }
}

final class CanvasGradient {
    enum Kind {
        case linear(CGPoint, CGPoint)
        case radial(CGPoint, CGFloat, CGPoint, CGFloat)
    }

    let kind: Kind
    private(set) var stops: [(CGFloat, Col)] = []
    private var made: CGGradient?

    init(_ kind: Kind) { self.kind = kind }

    func addColorStop(_ offset: Double, _ color: Col) {
        stops.append((CGFloat(min(1, max(0, offset))), color))
        made = nil
    }

    var cg: CGGradient? {
        if let made { return made }
        guard !stops.isEmpty else { return nil }
        // stops keep their order; ties are as added (the spec's rule)
        let sorted = stops.enumerated().sorted { $0.element.0 != $1.element.0 ? $0.element.0 < $1.element.0 : $0.offset < $1.offset }.map(\.element)
        var colors = sorted.map(\.1.cg)
        var locations = sorted.map(\.0)
        if colors.count == 1 {
            colors.append(colors[0])
            locations = [0, 1]
        }
        made = CGGradient(colorsSpace: Canvas2D.space, colors: colors as CFArray, locations: locations)
        return made
    }
}

enum Paint {
    case color(Col)
    case gradient(CanvasGradient)
}

enum Composite {
    case sourceOver, lighter, destinationOut, destinationIn, copy

    var blend: CGBlendMode {
        switch self {
        case .sourceOver: return .normal
        case .lighter: return .plusLighter
        case .destinationOut: return .destinationOut
        case .destinationIn: return .destinationIn
        case .copy: return .copy
        }
    }
}

enum TextAlign { case left, center, right }
enum TextBaseline { case alphabetic, middle, top, bottom }

final class Canvas2D {
    static let space = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()

    struct State {
        var fill: Paint = .color(.black)
        var stroke: Paint = .color(.black)
        var lineWidth: CGFloat = 1
        var lineCap: CGLineCap = .butt
        var lineJoin: CGLineJoin = .miter
        var alpha: CGFloat = 1
        var composite: Composite = .sourceOver
        var dash: [CGFloat] = []
        var dashOffset: CGFloat = 0
        var font: NSFont = .systemFont(ofSize: 10)
        var align: TextAlign = .left
        var baseline: TextBaseline = .alphabetic
        var smoothing = true
        var shadowBlur: CGFloat = 0
        var shadowColor: Col = .clear
        var transform: CGAffineTransform = .identity
    }

    let cg: CGContext
    let width: Int
    let height: Int
    private var st = State()
    private var stack: [State] = []
    private var path = CGMutablePath()
    private var bitmap = false

    /// draw into a context whose current transform already maps canvas pixels (y down) to its own space
    init(context: CGContext, width: Int, height: Int) {
        cg = context
        self.width = width
        self.height = height
        cg.setShouldAntialias(true)
        cg.interpolationQuality = .high
    }

    /// an offscreen canvas of its own, transparent
    convenience init?(width: Int, height: Int) {
        let w = max(1, width), h = max(1, height)
        guard let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: Canvas2D.space,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        ctx.translateBy(x: 0, y: CGFloat(h))
        ctx.scaleBy(x: 1, y: -1)
        self.init(context: ctx, width: w, height: h)
        bitmap = true
    }

    /// what an offscreen canvas holds now, for drawImage
    func image() -> CGImage? { bitmap ? cg.makeImage() : nil }

    // ---- state

    var fillStyle: Paint {
        get { st.fill }
        set { st.fill = newValue }
    }
    var strokeStyle: Paint {
        get { st.stroke }
        set { st.stroke = newValue }
    }
    func fill(_ c: Col) { st.fill = .color(c) }
    func fill(_ g: CanvasGradient) { st.fill = .gradient(g) }
    func stroke(_ c: Col) { st.stroke = .color(c) }
    func stroke(_ g: CanvasGradient) { st.stroke = .gradient(g) }

    var lineWidth: Double {
        get { Double(st.lineWidth) }
        set { if newValue > 0 && newValue.isFinite { st.lineWidth = CGFloat(newValue) } }
    }
    var lineCap: CGLineCap {
        get { st.lineCap }
        set { st.lineCap = newValue }
    }
    var lineJoin: CGLineJoin {
        get { st.lineJoin }
        set { st.lineJoin = newValue }
    }
    var globalAlpha: Double {
        get { Double(st.alpha) }
        set { if newValue >= 0 && newValue <= 1 { st.alpha = CGFloat(newValue) } else if newValue > 1 { st.alpha = 1 } }
    }
    var globalCompositeOperation: Composite {
        get { st.composite }
        set { st.composite = newValue }
    }
    var font: NSFont {
        get { st.font }
        set { st.font = newValue }
    }
    var textAlign: TextAlign {
        get { st.align }
        set { st.align = newValue }
    }
    var textBaseline: TextBaseline {
        get { st.baseline }
        set { st.baseline = newValue }
    }
    var imageSmoothingEnabled: Bool {
        get { st.smoothing }
        set { st.smoothing = newValue }
    }
    var shadowBlur: Double {
        get { Double(st.shadowBlur) }
        set { st.shadowBlur = CGFloat(max(0, newValue)) }
    }
    var shadowColor: Col {
        get { st.shadowColor }
        set { st.shadowColor = newValue }
    }
    var lineDashOffset: Double {
        get { Double(st.dashOffset) }
        set { st.dashOffset = CGFloat(newValue) }
    }
    func setLineDash(_ d: [Double]) {
        guard d.allSatisfy({ $0 >= 0 && $0.isFinite }) else { return }
        st.dash = (d.count % 2 == 1 ? d + d : d).map { CGFloat($0) }
    }

    func save() {
        stack.append(st)
        cg.saveGState()
    }

    func restore() {
        guard let s = stack.popLast() else { return }
        st = s
        cg.restoreGState()
    }

    // ---- the transform

    func setTransform(_ a: Double, _ b: Double, _ c: Double, _ d: Double, _ e: Double, _ f: Double) {
        st.transform = CGAffineTransform(a: a, b: b, c: c, d: d, tx: e, ty: f)
    }
    func resetTransform() { st.transform = .identity }
    func transform(_ a: Double, _ b: Double, _ c: Double, _ d: Double, _ e: Double, _ f: Double) {
        st.transform = CGAffineTransform(a: a, b: b, c: c, d: d, tx: e, ty: f).concatenating(st.transform)
    }
    func translate(_ x: Double, _ y: Double) { st.transform = CGAffineTransform(translationX: x, y: y).concatenating(st.transform) }
    func scale(_ x: Double, _ y: Double) { st.transform = CGAffineTransform(scaleX: x, y: y).concatenating(st.transform) }
    func rotate(_ a: Double) { st.transform = CGAffineTransform(rotationAngle: a).concatenating(st.transform) }

    // ---- the path, in device space

    func beginPath() { path = CGMutablePath() }

    func moveTo(_ x: Double, _ y: Double) {
        guard x.isFinite, y.isFinite else { return }
        path.move(to: CGPoint(x: x, y: y), transform: st.transform)
    }

    func lineTo(_ x: Double, _ y: Double) {
        guard x.isFinite, y.isFinite else { return }
        if path.isEmpty { path.move(to: CGPoint(x: x, y: y), transform: st.transform) } else { path.addLine(to: CGPoint(x: x, y: y), transform: st.transform) }
    }

    func closePath() {
        if !path.isEmpty { path.closeSubpath() }
    }

    func quadraticCurveTo(_ cx: Double, _ cy: Double, _ x: Double, _ y: Double) {
        if path.isEmpty { path.move(to: CGPoint(x: cx, y: cy), transform: st.transform) }
        path.addQuadCurve(to: CGPoint(x: x, y: y), control: CGPoint(x: cx, y: cy), transform: st.transform)
    }

    func bezierCurveTo(_ c1x: Double, _ c1y: Double, _ c2x: Double, _ c2y: Double, _ x: Double, _ y: Double) {
        if path.isEmpty { path.move(to: CGPoint(x: c1x, y: c1y), transform: st.transform) }
        path.addCurve(to: CGPoint(x: x, y: y), control1: CGPoint(x: c1x, y: c1y), control2: CGPoint(x: c2x, y: c2y), transform: st.transform)
    }

    func arcTo(_ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double, _ r: Double) {
        guard r >= 0 else { return }
        if path.isEmpty { path.move(to: CGPoint(x: x1, y: y1), transform: st.transform) }
        path.addArc(tangent1End: CGPoint(x: x1, y: y1), tangent2End: CGPoint(x: x2, y: y2), radius: r, transform: st.transform)
    }

    /// canvas arc: angles in radians, y down; anticlockwise as on the web
    func arc(_ x: Double, _ y: Double, _ r: Double, _ a0: Double, _ a1: Double, _ anticlockwise: Bool = false) {
        guard r >= 0, x.isFinite, y.isFinite, r.isFinite else { return }
        ellipseArc(st.transform, x, y, r, r, 0, a0, a1, anticlockwise)
    }

    func ellipse(_ x: Double, _ y: Double, _ rx: Double, _ ry: Double, _ rotation: Double, _ a0: Double, _ a1: Double, _ anticlockwise: Bool = false) {
        guard rx >= 0, ry >= 0 else { return }
        ellipseArc(st.transform, x, y, rx, ry, rotation, a0, a1, anticlockwise)
    }

    private func ellipseArc(_ t: CGAffineTransform, _ x: Double, _ y: Double, _ rx: Double, _ ry: Double, _ rot: Double, _ a0: Double, _ a1: Double, _ ccw: Bool) {
        // the spec's sweep: a full turn or more draws the whole ellipse; otherwise the angle runs the short way
        // round in the asked direction
        let tau = Double.pi * 2
        var end = a1
        if !ccw && a1 - a0 >= tau { end = a0 + tau } else if ccw && a0 - a1 >= tau { end = a0 - tau } else {
            var sweep = (a1 - a0).truncatingRemainder(dividingBy: tau)
            if !ccw && sweep < 0 { sweep += tau }
            if ccw && sweep > 0 { sweep -= tau }
            end = a0 + sweep
        }
        let m = CGAffineTransform(scaleX: rx == 0 ? 1e-9 : rx, y: ry == 0 ? 1e-9 : ry)
            .concatenating(CGAffineTransform(rotationAngle: rot))
            .concatenating(CGAffineTransform(translationX: x, y: y))
            .concatenating(t)
        let start = CGPoint(x: cos(a0), y: sin(a0)).applying(m)
        if path.isEmpty { path.move(to: start) } else { path.addLine(to: start) }
        // CG's arc math runs in the unit circle's own space, where increasing angle is "not clockwise"
        path.addArc(center: .zero, radius: 1, startAngle: a0, endAngle: end, clockwise: ccw, transform: m)
    }

    func rect(_ x: Double, _ y: Double, _ w: Double, _ h: Double) {
        guard x.isFinite, y.isFinite, w.isFinite, h.isFinite else { return }
        path.move(to: CGPoint(x: x, y: y), transform: st.transform)
        path.addLine(to: CGPoint(x: x + w, y: y), transform: st.transform)
        path.addLine(to: CGPoint(x: x + w, y: y + h), transform: st.transform)
        path.addLine(to: CGPoint(x: x, y: y + h), transform: st.transform)
        path.closeSubpath()
    }

    /// roundRect with one radius for every corner, scaled down as the spec says when they don't fit
    func roundRect(_ x: Double, _ y: Double, _ w: Double, _ h: Double, _ radius: Double) {
        var r = max(0, radius)
        let k = min(1, abs(w) / (2 * max(r, 1e-12)), abs(h) / (2 * max(r, 1e-12)))
        if r > 0 { r *= k }
        if r == 0 {
            rect(x, y, w, h)
            return
        }
        moveTo(x + r, y)
        lineTo(x + w - r, y)
        arcTo(x + w, y, x + w, y + r, r)
        lineTo(x + w, y + h - r)
        arcTo(x + w, y + h, x + w - r, y + h, r)
        lineTo(x + r, y + h)
        arcTo(x, y + h, x, y + h - r, r)
        lineTo(x, y + r)
        arcTo(x, y, x + r, y, r)
        closePath()
        moveTo(x, y)
    }

    // ---- painting

    private func begin() {
        cg.saveGState()
        cg.setAlpha(st.alpha)
        cg.setBlendMode(st.composite.blend)
        if st.shadowBlur > 0, st.shadowColor.a > 0 {
            cg.setShadow(offset: .zero, blur: st.shadowBlur, color: st.shadowColor.cg)
        }
    }

    private func end() { cg.restoreGState() }

    private func paintGradient(_ g: CanvasGradient) {
        guard let grad = g.cg else { return }
        cg.concatenate(st.transform)
        let opts: CGGradientDrawingOptions = [.drawsBeforeStartLocation, .drawsAfterEndLocation]
        switch g.kind {
        case let .linear(p0, p1):
            cg.drawLinearGradient(grad, start: p0, end: p1, options: opts)
        case let .radial(c0, r0, c1, r1):
            cg.drawRadialGradient(grad, startCenter: c0, startRadius: r0, endCenter: c1, endRadius: r1, options: opts)
        }
    }

    private func fillDevice(_ p: CGPath, _ paint: Paint, rule: CGPathFillRule = .winding) {
        guard !p.isEmpty else { return }
        begin()
        switch paint {
        case let .color(c):
            cg.setFillColor(c.cg)
            cg.addPath(p)
            cg.fillPath(using: rule)
        case let .gradient(g):
            if st.shadowBlur > 0, st.shadowColor.a > 0 {
                // the shadow of the shape, then the gradient inside it
                cg.setFillColor(Col.black.cg)
                cg.addPath(p)
                cg.fillPath(using: rule)
                cg.setShadow(offset: .zero, blur: 0, color: nil)
            }
            cg.addPath(p)
            cg.clip(using: rule)
            paintGradient(g)
        }
        end()
    }

    func fill(_ rule: CGPathFillRule = .winding) { fillDevice(path, st.fill, rule: rule) }
    func fillEvenOdd() { fill(.evenOdd) }

    func stroke() {
        guard !path.isEmpty else { return }
        let inv = st.transform.inverted()
        begin()
        cg.concatenate(st.transform)
        var back = inv
        if let user = path.copy(using: &back) { cg.addPath(user) }
        cg.setLineWidth(st.lineWidth)
        cg.setLineCap(st.lineCap)
        cg.setLineJoin(st.lineJoin)
        cg.setMiterLimit(10)
        if !st.dash.isEmpty { cg.setLineDash(phase: st.dashOffset, lengths: st.dash) }
        switch st.stroke {
        case let .color(c):
            cg.setStrokeColor(c.cg)
            cg.strokePath()
        case let .gradient(g):
            cg.replacePathWithStrokedPath()
            cg.clip()
            // paintGradient concatenates the transform again; undo the one above first
            cg.concatenate(inv)
            paintGradient(g)
        }
        end()
    }

    func clip(_ rule: CGPathFillRule = .winding) {
        cg.addPath(path)
        if path.isEmpty { cg.clip(to: .zero) } else { cg.clip(using: rule) }
    }

    private func rectPath(_ x: Double, _ y: Double, _ w: Double, _ h: Double) -> CGPath {
        let p = CGMutablePath()
        p.addRect(CGRect(x: x, y: y, width: w, height: h), transform: st.transform)
        return p
    }

    func fillRect(_ x: Double, _ y: Double, _ w: Double, _ h: Double) {
        guard x.isFinite, y.isFinite, w.isFinite, h.isFinite, w != 0, h != 0 else { return }
        fillDevice(rectPath(x, y, w, h), st.fill)
    }

    func strokeRect(_ x: Double, _ y: Double, _ w: Double, _ h: Double) {
        let saved = path
        beginPath()
        rect(x, y, w, h)
        stroke()
        path = saved
    }

    func clearRect(_ x: Double, _ y: Double, _ w: Double, _ h: Double) {
        cg.saveGState()
        cg.setBlendMode(.clear)
        cg.addPath(rectPath(x, y, w, h))
        cg.fillPath()
        cg.restoreGState()
    }

    /// drawImage(img, dx, dy, dw, dh), and with a source rect (sx, sy, sw, sh) in the image's own pixels
    func drawImage(_ img: CGImage?, _ dx: Double, _ dy: Double, _ dw: Double, _ dh: Double, source: CGRect? = nil) {
        guard var img, dw != 0, dh != 0 else { return }
        if let source, let cropped = img.cropping(to: source) { img = cropped }
        begin()
        cg.interpolationQuality = st.smoothing ? .high : .none
        cg.concatenate(st.transform)
        // the image's rows run top down in a y-down space
        cg.translateBy(x: dx, y: dy + dh)
        cg.scaleBy(x: 1, y: -1)
        cg.draw(img, in: CGRect(x: 0, y: 0, width: dw, height: dh))
        end()
    }

    func drawImage(_ c: Canvas2D?, _ dx: Double, _ dy: Double, _ dw: Double, _ dh: Double) {
        drawImage(c?.image(), dx, dy, dw, dh)
    }

    // ---- text

    private func line(_ text: String) -> CTLine {
        let attrs: [NSAttributedString.Key: Any] = [.font: st.font, .foregroundColor: NSColor.black]
        return CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attrs))
    }

    func measureText(_ text: String) -> Double {
        Double(CTLineGetTypographicBounds(line(text), nil, nil, nil))
    }

    func fillText(_ text: String, _ x: Double, _ y: Double) {
        guard case let .color(c) = st.fill else { return }
        let attrs: [NSAttributedString.Key: Any] = [.font: st.font, .foregroundColor: NSColor(cgColor: c.cg) ?? .black]
        let l = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attrs))
        var ascent: CGFloat = 0, descent: CGFloat = 0
        let w = CTLineGetTypographicBounds(l, &ascent, &descent, nil)
        var ox = x
        switch st.align {
        case .left: break
        case .center: ox -= w / 2
        case .right: ox -= w
        }
        var oy = y
        switch st.baseline {
        case .alphabetic: break
        case .middle: oy += Double(ascent - descent) / 2
        case .top: oy += Double(ascent)
        case .bottom: oy -= Double(descent)
        }
        begin()
        cg.concatenate(st.transform)
        cg.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        cg.textPosition = CGPoint(x: ox, y: oy)
        CTLineDraw(l, cg)
        end()
    }
}

/// css's rgb()/rgba()/#hex, for the few colours read from settings or written as text
func parseCSSColor(_ s: String, _ fallback: Col) -> Col {
    let t = s.trimmingCharacters(in: .whitespaces).lowercased()
    if t.hasPrefix("#") {
        var hex = String(t.dropFirst())
        if hex.count == 3 { hex = hex.map { "\($0)\($0)" }.joined() }
        guard hex.count == 6, let n = Int(hex, radix: 16) else { return fallback }
        return Col(Double((n >> 16) & 255), Double((n >> 8) & 255), Double(n & 255))
    }
    if t.hasPrefix("rgb") {
        let inner = t.drop { $0 != "(" }.dropFirst().prefix { $0 != ")" }
        let parts = inner.split { $0 == "," || $0 == " " || $0 == "/" }.compactMap { Double($0) }
        guard parts.count >= 3 else { return fallback }
        return Col(parts[0], parts[1], parts[2], parts.count > 3 ? parts[3] : 1)
    }
    return fallback
}
