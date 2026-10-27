import AppKit
import QCalcCore
import SwiftUI

// The graph under the bar, laid out as GraphPanel.tsx draws its SVG (.graph in src/index.css): a head with the
// label, the domain and "reset", then a 652 × 200 plot (padding 12/12/28/40) on the plot colour, grid lines at
// ink 5%, axes at ink 18%, 10 pt muted ticks and a 2 pt accent curve. Drag pans, pinch or scroll zooms, and the
// engine re-samples (Engine.graphAsync, graphHome, graphTicks, graphReadoutY; GraphModel.swift).
struct GraphPanel: View {
    @ObservedObject var model: BarModel
    let state: GraphState
    @State private var dragStart: CGPoint?
    @State private var lastZoom: CGFloat = 1
    @State private var resetHover = false
    /// the pointer over the plot, in plot (viewBox) units
    @State private var hoverX: Double?

    /// y on the curve at x, as the readout shows it (the engine's graphReadoutY)
    private func curveY(at x: Double) -> Double? { model.graphReadout(x) }

    private static let svgW: CGFloat = 652, svgH: CGFloat = 200
    private static let pad = (top: CGFloat(12), right: CGFloat(12), bottom: CGFloat(28), left: CGFloat(40))
    private static let plotBgNS = NSColor(name: nil) { a in
        // color-mix(in srgb, var(--bg) 70%, var(--bg-tape))
        a.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: (0x2c * 0.7 + 0x25 * 0.3) / 255, green: (0x2c * 0.7 + 0x25 * 0.3) / 255, blue: (0x2e * 0.7 + 0x27 * 0.3) / 255, alpha: 1)
            : NSColor(srgbRed: (255 * 0.7 + 0xf5 * 0.3) / 255, green: (255 * 0.7 + 0xf5 * 0.3) / 255, blue: (255 * 0.7 + 0xf7 * 0.3) / 255, alpha: 1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                Text(label).font(.system(size: Theme.body, weight: .semibold)).foregroundStyle(Color.qInk)
                Text("[\(fmt(state.xMin)), \(fmt(state.xMax))]")
                    .font(.system(size: Theme.small).monospacedDigit())
                    .foregroundStyle(Color.qMuted)
                if let unit = state.angleUnit {
                    Text(unit).font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                }
                Spacer(minLength: 0)
                Button { model.homeGraph() } label: {
                    Text("reset")
                        .font(.system(size: Theme.small))
                        .foregroundStyle(resetHover && !atHome ? Color.qInk : Color.qMuted)
                        .padding(.vertical, 3).padding(.horizontal, 8)
                        .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(resetHover && !atHome ? Color.qFillHover : .clear))
                }
                .buttonStyle(.plain)
                .disabled(atHome)
                .opacity(atHome ? 0.4 : 1)
                .padding(.vertical, -3).padding(.trailing, -8)
                .onHover { resetHover = $0 }
                .help("Reset view")
            }
            if let error = state.error {
                // .graph-error
                Text(error)
                    .font(.system(size: Theme.body))
                    .foregroundStyle(Color.qMuted)
                    .padding(.vertical, 12)
            } else if state.failed {
                Text("can’t graph \(state.expression)")
                    .font(.system(size: Theme.body))
                    .foregroundStyle(Color.qMuted)
                    .padding(.vertical, 12)
            } else {
                plot
                details
            }
        }
        .padding(.top, 12)
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.qBgTape)
        .overlay(alignment: .top) { Rectangle().fill(Color.qLine).frame(height: 1) }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Graph of \(label)")
    }

    private var label: String { state.label.isEmpty ? Pretty.radicals(state.expression) : state.label }
    private var atHome: Bool { state.atHome }

    private func kindName(_ k: CriticalKind) -> String {
        switch k {
        case .min: return "min"
        case .max: return "max"
        default: return "critical"
        }
    }

    // .graph-detail: "Critical points", then a chip per point (kind in caption muted, the point in ink)
    @ViewBuilder private var details: some View {
        if !state.critical.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                Text("Critical points").font(.system(size: Theme.caption, weight: .semibold)).foregroundStyle(Color.qMuted)
                    .padding(.top, 12)
                HStack(spacing: 4) {
                    ForEach(Array(state.critical.enumerated()), id: \.offset) { _, c in
                        HStack(spacing: 6) {
                            Text(kindName(c.kind)).font(.system(size: Theme.caption, weight: .medium)).foregroundStyle(Color.qMuted)
                            Text("(\(fmt(c.x)), \(fmt(c.y)))").font(.system(size: Theme.small).monospacedDigit()).foregroundStyle(Color.qInk)
                        }
                        .padding(.vertical, 3).padding(.horizontal, 8)
                        .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qInk.opacity(0.06)))
                    }
                }
            }
            .padding(.top, 8)
        }
    }

    private var plot: some View {
        GeometryReader { geo in
            let scale = geo.size.width / Self.svgW
            // an AppKit view drawing through Core Graphics, not a Canvas: a Canvas renders each redraw into a new
            // GPU surface that SwiftUI keeps for a while, 130 MB of them while a graph line is typed
            Color.clear
                .background(PlotCanvas { cg in
                    cg.scaleBy(x: scale, y: scale)
                    draw(cg)
                })
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 2)
                .onChanged { g in
                    let prev = dragStart ?? g.startLocation
                    dragStart = g.location
                    let plotW = (Self.svgW - Self.pad.left - Self.pad.right) * scale
                    let plotH = (Self.svgH - Self.pad.top - Self.pad.bottom) * scale
                    model.panGraph(dx: (g.location.x - prev.x) / plotW, dy: (g.location.y - prev.y) / plotH)
                }
                .onEnded { _ in dragStart = nil })
            .gesture(MagnificationGesture()
                .onChanged { s in
                    model.zoomGraph(Double(lastZoom / s))
                    lastZoom = s
                }
                .onEnded { _ in lastZoom = 1 })
            .onTapGesture(count: 2) { model.homeGraph() }
            // the readout: a hairline, a dot on the curve and (x, y) at the pointer (GraphPanel.tsx readout)
            .pointerHover { p in hoverX = p.map { Double($0.x / scale) } }
            .accessibilityLabel(summary)
        }
        .aspectRatio(Self.svgW / Self.svgH, contentMode: .fit)
        .frame(maxHeight: 200)
    }

    private var summary: String {
        let ys = state.curves.flatMap { $0.points.compactMap(\.y) }
        guard let lo = ys.min(), let hi = ys.max() else { return "Graph of \(label), nothing to draw" }
        return "Graph of \(label) from x \(fmt(state.xMin)) to \(fmt(state.xMax)), y from \(fmt(lo)) to \(fmt(hi))"
    }

    private func fmt4(_ v: Double) -> String {
        Pretty.answer(String(format: "%.4g", abs(v) < 1e-9 ? 0 : v))
    }

    private func fmt(_ v: Double) -> String {
        let r = abs(v) < 1e-9 ? 0 : v
        return Pretty.answer(String(format: "%g", r))
    }

    /// The plot in viewBox units, as the Canvas drew it: the same fills, strokes and text, in the colours the
    /// view's appearance resolves.
    private func draw(_ cg: CGContext) {
        let g = state, p = Self.pad
        let plotW = Self.svgW - p.left - p.right, plotH = Self.svgH - p.top - p.bottom
        let bottom = p.top + plotH, right = p.left + plotW
        func px(_ x: Double) -> CGFloat { p.left + CGFloat((x - g.xMin) / (g.xMax - g.xMin)) * plotW }
        func py(_ y: Double) -> CGFloat { p.top + CGFloat((g.yMax - y) / (g.yMax - g.yMin)) * plotH }
        func line(_ a: CGPoint, _ b: CGPoint, _ color: CGColor, _ width: CGFloat) {
            cg.move(to: a)
            cg.addLine(to: b)
            cg.setStrokeColor(color)
            cg.setLineWidth(width)
            cg.strokePath()
        }
        let ink = Theme.ink.cgColor, accent = Theme.accent.cgColor, plotBg = Self.plotBgNS.cgColor
        cg.addPath(CGPath(roundedRect: CGRect(x: p.left, y: p.top, width: plotW, height: plotH), cornerWidth: 4, cornerHeight: 4, transform: nil))
        cg.setFillColor(plotBg)
        cg.fillPath()
        let grid = ink.copy(alpha: 0.05) ?? ink, axis = ink.copy(alpha: 0.18) ?? ink
        let tickFont = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        // the engine's graphTicks: x angle-aware (π/2 steps for a radian curve), y plain
        for t in g.xTicks where t.value >= g.xMin && t.value <= g.xMax {
            line(CGPoint(x: px(t.value), y: p.top), CGPoint(x: px(t.value), y: bottom), grid, 1)
            Self.text(Pretty.answer(t.label), tickFont, Theme.muted, at: CGPoint(x: px(t.value), y: Self.svgH - 12), anchor: .center)
        }
        for t in g.yTicks where t.value >= g.yMin && t.value <= g.yMax {
            line(CGPoint(x: p.left, y: py(t.value)), CGPoint(x: right, y: py(t.value)), grid, 1)
            Self.text(Pretty.answer(t.label), tickFont, Theme.muted, at: CGPoint(x: p.left - 6, y: py(t.value)), anchor: .trailing)
        }
        if py(0) >= p.top, py(0) <= bottom {
            line(CGPoint(x: p.left, y: py(0)), CGPoint(x: right, y: py(0)), axis, 1)
        }
        if px(0) >= p.left, px(0) <= right {
            line(CGPoint(x: px(0), y: p.top), CGPoint(x: px(0), y: bottom), axis, 1)
        }
        cg.saveGState()
        cg.clip(to: CGRect(x: p.left, y: p.top, width: plotW, height: plotH))
        for curve in g.curves {
            let path = CGMutablePath()
            var pen = false
            var lastY: Double?
            for pt in curve.points {
                guard let v = pt.y else { pen = false; continue }
                if let ly = lastY, abs(v - ly) > (g.yMax - g.yMin) * 4 { pen = false }
                let q = CGPoint(x: px(pt.x), y: max(-1e4, min(1e4, py(v))))
                if pen { path.addLine(to: q) } else { path.move(to: q) }
                pen = true
                lastY = v
            }
            cg.addPath(path)
            cg.setStrokeColor(accent)
            cg.setLineWidth(2)
            cg.setLineCap(.round)
            cg.setLineJoin(.round)
            cg.strokePath()
        }
        cg.restoreGState()
        if let hx = hoverX, hx >= Double(p.left), hx <= Double(right) {
            let x = g.xMin + (hx - Double(p.left)) / Double(plotW) * (g.xMax - g.xMin)
            line(CGPoint(x: hx, y: p.top), CGPoint(x: hx, y: bottom), ink.copy(alpha: 0.22) ?? ink, 1)
            if let y = curveY(at: x), py(y) >= p.top, py(y) <= bottom {
                let dot = CGRect(x: hx - 3.5, y: py(y) - 3.5, width: 7, height: 7)
                cg.setFillColor(accent)
                cg.fillEllipse(in: dot)
                cg.setStrokeColor(plotBg)
                cg.setLineWidth(1.5)
                cg.strokeEllipse(in: dot)
                let label = "(\(fmt4(x)), \(fmt4(y)))"
                let onRight = hx > Double(p.left) + Double(plotW) * 0.62
                Self.text(label, NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular), Theme.ink,
                          at: CGPoint(x: hx + (onRight ? -8 : 8), y: p.top + 13), anchor: onRight ? .trailing : .leading)
            }
        }
        // .graph-mark: roots and critical points, rings of the plot colour with an accent edge
        let marks = g.roots.map { CGPoint(x: px($0), y: py(0)) } + g.critical.map { CGPoint(x: px($0.x), y: py($0.y)) }
        cg.saveGState()
        cg.clip(to: CGRect(x: p.left, y: p.top, width: plotW, height: plotH))
        for m in marks where m.x >= p.left && m.x <= right && m.y >= p.top && m.y <= bottom {
            let ring = CGRect(x: m.x - 2.75, y: m.y - 2.75, width: 5.5, height: 5.5)
            cg.setFillColor(plotBg)
            cg.fillEllipse(in: ring)
            cg.setStrokeColor(accent)
            cg.setLineWidth(1.5)
            cg.strokeEllipse(in: ring)
        }
        cg.restoreGState()
    }

    /// GraphicsContext.draw(Text, at:, anchor:): the line's box placed so `anchor` of it sits on `point`
    /// (the context is flipped, as the view is).
    private static func text(_ s: String, _ font: NSFont, _ color: NSColor, at point: CGPoint, anchor: UnitPoint) {
        let str = NSAttributedString(string: s, attributes: [.font: font, .foregroundColor: color])
        let size = str.size()
        str.draw(at: CGPoint(x: point.x - size.width * anchor.x, y: point.y - size.height * anchor.y))
    }

    // graphTicks' rule: steps of 1, 2 or 5 × 10ⁿ, about `count` across, the origin always one of them
    private func ticks(_ lo: Double, _ hi: Double, count: Int) -> [Double] {
        let raw = (hi - lo) / Double(count)
        guard raw > 0, raw.isFinite else { return [] }
        let mag = pow(10, floor(log10(raw)))
        let f = raw / mag
        let step = (f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10) * mag
        var out: [Double] = []
        var v = (lo / step).rounded(.up) * step
        while v <= hi + step * 1e-9, out.count < 40 {
            out.append(abs(v) < step * 1e-9 ? 0 : v)
            v += step
        }
        return out
    }
}

/// The plot's drawing surface: an NSView that draws through `draw` into a bitmap of its own and hands that to its
/// layer as contents, and lets every event through to the SwiftUI gestures above it. Drawing into the layer's
/// backing store instead (or a SwiftUI Canvas) costs 110-145 MB of GPU surfaces for a few seconds each time the
/// panel opens, and ~9 MB held after; a bitmap is one ~2 MB image.
private struct PlotCanvas: NSViewRepresentable {
    let draw: (CGContext) -> Void

    func makeNSView(context: Context) -> PlotNSView { PlotNSView() }

    func updateNSView(_ view: PlotNSView, context: Context) {
        view.drawing = draw
        view.needsDisplay = true
    }

    final class PlotNSView: NSView {
        var drawing: ((CGContext) -> Void)?

        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override var isFlipped: Bool { true }
        override var wantsUpdateLayer: Bool { true }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidChangeEffectiveAppearance() {
            super.viewDidChangeEffectiveAppearance()
            needsDisplay = true
        }

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

        /// The plot at `scale` pixels a point, in the view's appearance, y down.
        func image(scale: CGFloat) -> CGImage? {
            let w = Int((bounds.width * scale).rounded(.up)), h = Int((bounds.height * scale).rounded(.up))
            guard let drawing, w > 0, h > 0, let space = CGColorSpace(name: CGColorSpace.sRGB),
                  let cg = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
            cg.translateBy(x: 0, y: CGFloat(h))
            cg.scaleBy(x: scale, y: -scale)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: true)
            effectiveAppearance.performAsCurrentDrawingAppearance { drawing(cg) }
            NSGraphicsContext.restoreGraphicsState()
            return cg.makeImage()
        }

        // cacheDisplay (DevSnapshot) and printing still draw through here: the same bitmap
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
