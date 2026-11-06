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
    @State private var settingsShown = false
    /// the 3D scene from the last drawing of the plot, for the pointer's pass
    @State private var sceneCache = SceneCache()
    /// the pointer over the plot, in plot (viewBox) units
    @State private var hover: CGPoint?
    /// the critical point whose chip is hovered or focused, ringed on the plot (native only: the page's chips
    /// only tint)
    @State private var spot: PlotSpot?

    /// y on the curve at x, as the readout shows it (the engine's graphReadoutY)
    private func curveY(at x: Double) -> Double? { model.graphReadout(x) }

    private static let svgW: CGFloat = 652, svgH: CGFloat = 200
    /// native only: the 3D view is taller, so a surface has room to tilt
    private static let svgH3D: CGFloat = 360
    private var plotH: CGFloat { state.shows3D ? Self.svgH3D : Self.svgH }
    /// how near (viewBox units) the pointer must come to a critical point's mark to pick it out
    private static let snap: CGFloat = 9
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
                Text(label).font(.system(size: Theme.body, weight: .semibold, design: .monospaced)).foregroundStyle(Color.qInk)
                Text("x ∈ [\(fmt(state.xMin)), \(fmt(state.xMax))]")
                    .font(.system(size: Theme.small, design: .monospaced))
                    .foregroundStyle(Color.qMuted)
                if let unit = state.angleUnit {
                    Text(unit).font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                }
                Spacer(minLength: 0)
                if state.kind == .surface, state.error == nil {
                    // native only: a surface in 3D or as level curves
                    HStack(spacing: 2) {
                        ViewToggle(title: "3D", on: state.shows3D, tip: "Two-finger press and drag to orbit, two-finger scroll to zoom, shift + two-finger drag to pan") { model.setGraph3D(true) }
                        ViewToggle(title: "contour", on: !state.shows3D, tip: "Level curves, seen from above") { model.setGraph3D(false) }
                    }
                }
                // native only: another equation, and the graph settings
                HeadIcon(symbol: "plus", tip: "Add an equation") { model.addGraphLayer() }
                HeadIcon(symbol: "wrench.adjustable", tip: "Graph settings", on: settingsShown) { settingsShown.toggle() }
                    .popover(isPresented: $settingsShown, arrowEdge: .bottom) { GraphSettingsView(model: model) }
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
                .hoverTip("Reset view")
            }
            if !state.layers.isEmpty {
                GraphEquations(model: model, state: state, label: label)
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
        case .xIntercept: return "x-intercept"
        case .yIntercept: return "y-intercept"
        case .saddle: return "saddle"
        case .verticalTangent: return "vertical tangent"
        case .point: return "point"
        default: return "critical"
        }
    }

    /// the critical point the pointer is on, on the plot (native only)
    private var hoveredMark: Int? {
        guard let h = hover else { return nil }
        var best: (Int, CGFloat)?
        for (i, m) in markPoints().enumerated() {
            guard let m else { continue }
            let d = hypot(m.x - h.x, m.y - h.y)
            if d <= Self.snap, best.map({ d < $0.1 }) ?? true { best = (i, d) }
        }
        return best?.0
    }

    /// Native only: every equation's critical points, the bar's first, each with its equation's number (0 is the
    /// bar's), which colours its mark and chip. On the 3D view only surfaces' points count.
    private var allCritical: [(layer: Int, c: CriticalPoint)] {
        var out = state.critical.map { (layer: 0, c: $0) }
        for (i, l) in state.layers.enumerated() where l.error == nil && !l.isBlank && (!state.shows3D || l.surface != nil) {
            out += l.critical.map { (layer: i + 1, c: $0) }
        }
        return out
    }

    /// the heading over the chips: what the list holds for this kind of graph
    private var detailTitle: String {
        switch state.kind {
        case .surface: return "Critical points"
        case .implicit: return "Key points"
        case .curve: return state.critical.contains { $0.kind == .point } ? "Points" : "Critical points"
        }
    }

    private func coords(_ c: CriticalPoint) -> String {
        if let z = c.z { return "(\(coord(c.x)), \(coord(c.y)), \(coord(z)))" }
        return "(\(coord(c.x)), \(coord(c.y)))"
    }

    // .graph-detail: "Critical points", then a chip per point (kind in caption muted, the point in ink)
    @ViewBuilder private var details: some View {
        if allCritical.isEmpty {
            // .graph-detail-empty: the page says there are none rather than leaving the label out
            VStack(alignment: .leading, spacing: 6) {
                Text(detailTitle.uppercased()).font(.system(size: Theme.caption, weight: .medium, design: .monospaced)).tracking(0.6).foregroundStyle(Color.qMuted)
                    .padding(.top, 12)
                Text("None in this domain").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
            }
            .padding(.top, 8)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                Text(detailTitle.uppercased()).font(.system(size: Theme.caption, weight: .medium, design: .monospaced)).tracking(0.6).foregroundStyle(Color.qMuted)
                    .padding(.top, 12)
                // .graph-detail-list: flex-wrap, so a narrow bar puts the chips on more rows (an HStack squeezed
                // each into a column a character wide)
                let onPlot = hoveredMark
                WrapRow(spacing: 4, leading: true) {
                    ForEach(Array(allCritical.enumerated()), id: \.offset) { i, entry in
                        let c = entry.c
                        let here = PlotSpot(x: c.x, y: c.y)
                        // .graph-detail-item: a button, "Copy x" (selectX → copyValue), the point at 8 figures
                        // (formatCoord), as the page has it; they were static and grouped to 6
                        // a y-intercept's x is always 0, so its chip copies y; a surface's copies its height
                        let copies = c.z != nil ? "z" : c.kind == .yIntercept ? "y" : "x"
                        let copied = c.z ?? (c.kind == .yIntercept ? c.y : c.x)
                        CriticalChip(kind: kindName(c.kind), coords: coords(c), copies: copies, pointed: onPlot == i,
                                     color: state.layers.isEmpty ? nil : Self.layerSwiftColor(entry.layer), action: {
                            model.copyAnswer(model.logic.string("engine/format", "formatNumber", [copied, 8]) ?? fmt(copied))
                        }, highlight: { on in
                            if on { spot = here } else if spot == here { spot = nil }
                        })
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
            // the plot is drawn once per change of graph, view or settings; the pointer's reading over it in a
            // second, light pass, so moving the pointer never draws the plot again
            let baseKey = PlotKey(state: state, prefs: model.graphPrefs, hover: nil, spot: nil, scale: scale)
            Color.clear
                .background(PlotCanvas(key: baseKey, draw: { cg in
                    cg.scaleBy(x: scale, y: scale)
                    if state.shows3D { draw3D(cg, .base, key: baseKey) } else { draw(cg, .base) }
                }))
                .overlay(PlotCanvas(key: PlotKey(state: state, prefs: model.graphPrefs, hover: hover, spot: spot, scale: scale), draw: { cg in
                    cg.scaleBy(x: scale, y: scale)
                    if state.shows3D { draw3D(cg, .overlay, key: baseKey) } else { draw(cg, .overlay) }
                }))
            // the 3D view takes the pointer and trackpad in AppKit, ahead of SwiftUI and of the panel's
            // drag-by-background, so a drag or a two-finger drag orbits and never moves the window
            .overlay {
                if state.shows3D {
                    PlotInput(
                        drag: { [model] d, pan in
                            let plotW = (Self.svgW - Self.pad.left - Self.pad.right) * scale
                            let plotH = (Self.svgH3D - Self.pad.top - Self.pad.bottom) * scale
                            // a pan follows the pointer; an orbit turns against it side to side and with it up/down
                            let k: CGFloat = pan ? 1 : -1
                            model.panGraph(dx: k * d.width / plotW, dy: d.height / plotH, move3D: pan)
                        },
                        scroll: { [model] event, size in model.scrollGraph(event, size: size) },
                        magnify: { [model] m in model.zoomGraph(1 / (1 + m)) },
                        hover: { p in hover = p.map { CGPoint(x: $0.x / scale, y: $0.y / scale) } },
                        home: { [model] in model.homeGraph() })
                }
            }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 2)
                .onChanged { g in
                    let prev = dragStart ?? g.startLocation
                    dragStart = g.location
                    let plotW = (Self.svgW - Self.pad.left - Self.pad.right) * scale
                    let plotH = (plotH - Self.pad.top - Self.pad.bottom) * scale
                    // in 3D a drag orbits; shift- or option-drag pans
                    let flags = NSEvent.modifierFlags
                    model.panGraph(dx: (g.location.x - prev.x) / plotW, dy: (g.location.y - prev.y) / plotH,
                                   move3D: flags.contains(.shift) || flags.contains(.option))
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
            .pointerHover { p in hover = p.map { CGPoint(x: $0.x / scale, y: $0.y / scale) } }
            .accessibilityLabel(summary)
        }
        .aspectRatio(Self.svgW / plotH, contentMode: .fit)
        .frame(maxHeight: plotH)
    }

    private var summary: String {
        if state.kind == .surface {
            return "Surface \(label) over x \(fmt(state.xMin)) to \(fmt(state.xMax)), \(state.critical.count) critical points"
        }
        if state.kind == .implicit {
            return "Curve \(label) from x \(fmt(state.xMin)) to \(fmt(state.xMax)), y \(fmt(state.yMin)) to \(fmt(state.yMax))"
        }
        let ys = state.curves.flatMap { $0.points.compactMap(\.y) }
        guard let lo = ys.min(), let hi = ys.max() else { return "Graph of \(label), nothing to draw" }
        return "Graph of \(label) from x \(fmt(state.xMin)) to \(fmt(state.xMax)), y from \(fmt(lo)) to \(fmt(hi))"
    }

    /// a tick as the page draws it (`{t.label}`: formatNumber, no grouping, a real minus); Pretty.answer had
    /// grouped `50000` but not `−50000`, so one axis read `50,000` beside `−50000`
    private static func tickText(_ label: String) -> String {
        label.hasPrefix("-") ? Pretty.minus + label.dropFirst() : label
    }

    /// formatCoord: formatNumber at 8 figures with a real minus (the page's `withMinus`)
    private func coord(_ v: Double) -> String {
        Self.tickText(model.logic.string("engine/format", "formatNumber", [abs(v) < 1e-12 ? 0 : v, 8]) ?? fmt(v))
    }

    private func fmt4(_ v: Double) -> String {
        Pretty.answer(String(format: "%.4g", abs(v) < 1e-9 ? 0 : v))
    }

    private func fmt(_ v: Double) -> String {
        let r = abs(v) < 1e-9 ? 0 : v
        return Pretty.answer(String(format: "%g", r))
    }

    /// Native only: each equation's colour, the first in the accent (as the page draws its one curve), the rest
    /// from a palette that reads on both backgrounds
    static func layerRGB(_ i: Int) -> (Double, Double, Double)? {
        guard i > 0 else { return nil }
        let palette: [(Double, Double, Double)] = [
            (0.86, 0.30, 0.27), (0.25, 0.52, 0.86), (0.62, 0.42, 0.86), (0.93, 0.58, 0.16), (0.20, 0.70, 0.72), (0.60, 0.60, 0.62),
        ]
        return palette[(i - 1) % palette.count]
    }

    static func layerColor(_ i: Int) -> CGColor {
        guard let c = layerRGB(i) else { return Theme.accent.cgColor }
        return CGColor(srgbRed: c.0, green: c.1, blue: c.2, alpha: 1)
    }

    static func layerSwiftColor(_ i: Int) -> Color {
        guard let c = layerRGB(i) else { return Color.qAccent }
        return Color(.sRGB, red: c.0, green: c.1, blue: c.2)
    }

    /// The plot's monospaced type (native: the plot reads as a technical drawing)
    private static func mono(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        NSFont.monospacedSystemFont(ofSize: size, weight: weight)
    }

    /// Which pass of the plot: everything that stays put (drawn once per change of graph, view or settings), or
    /// what follows the pointer (drawn over it, so moving the pointer never redraws the plot)
    enum PlotPart { case base, overlay }

    /// One axis's grid lines: the labelled values, and the minor ones between
    private struct AxisGrid {
        var major: [(Double, String)] = []
        var minor: [Double] = []
        var step: Double? { major.count >= 2 ? major[1].0 - major[0].0 : nil }
    }

    /// The step set in the settings, else decades on a log axis, else the engine's ticks; minor lines fifths of a
    /// step (quarters of a 2), or 2…9 of each decade
    private func axisGrid(_ lo: Double, _ hi: Double, log: Bool, step: Double?, engine: [GraphTick]) -> AxisGrid {
        var out = AxisGrid()
        guard hi > lo, lo.isFinite, hi.isFinite else { return out }
        func subdivide(from base: Double, step: Double) {
            let mant = step / pow(10, floor(log10(step)))
            let sub = abs(mant - 2) < 0.01 ? 4 : 5
            let ms = step / Double(sub)
            guard ms > 0, (hi - lo) / ms <= 1000 else { return }
            var m = ((lo - base) / ms).rounded(.up)
            while base + m * ms <= hi {
                if Int(m.rounded()) % sub != 0 { out.minor.append(base + m * ms) }
                m += 1
            }
        }
        if let step, step > 0, (hi - lo) / step <= 200 {
            var k = (lo / step).rounded(.up)
            while k * step <= hi + step * 1e-9 {
                let v = abs(k * step) < step * 1e-9 ? 0 : k * step
                out.major.append((v, Self.tickText(fmt(v))))
                k += 1
            }
            subdivide(from: 0, step: step)
        } else if log, lo > 0 {
            let a = Int(floor(log10(lo))), b = Int(ceil(log10(hi)))
            let every = max(1, (b - a) / 8)
            for k in a...b {
                let v = pow(10, Double(k))
                if v >= lo * (1 - 1e-9), v <= hi * (1 + 1e-9), (k - a) % every == 0 { out.major.append((v, Self.tickText(fmt(v)))) }
                if b - a <= 12 {
                    for m in 2...9 where Double(m) * v >= lo && Double(m) * v <= hi { out.minor.append(Double(m) * v) }
                }
            }
        } else {
            out.major = engine.filter { $0.value >= lo && $0.value <= hi }.map { ($0.value, Self.tickText($0.label)) }
            if let step = out.step, step > 0 { subdivide(from: out.major[0].0, step: step) }
        }
        return out
    }

    /// The 2D plot's mapping, graph units to viewBox units and back, linear or log on each axis
    private struct Map2D {
        let left: CGFloat, top: CGFloat, w: CGFloat, h: CGFloat
        let logX: Bool, logY: Bool
        let x0: Double, x1: Double, y0: Double, y1: Double

        init(_ g: GraphState, pad: (top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat), size: CGSize) {
            (left, top) = (pad.left, pad.top)
            w = size.width - pad.left - pad.right
            h = size.height - pad.top - pad.bottom
            (logX, logY) = (g.logX, g.logY)
            x0 = logX ? log10(g.xMin) : g.xMin
            x1 = logX ? log10(g.xMax) : g.xMax
            y0 = logY ? log10(g.yMin) : g.yMin
            y1 = logY ? log10(g.yMax) : g.yMax
        }

        func tx(_ x: Double) -> Double { logX ? (x > 0 ? log10(x) : -.infinity) : x }
        func ty(_ y: Double) -> Double { logY ? (y > 0 ? log10(y) : -.infinity) : y }
        func px(_ x: Double) -> CGFloat { left + CGFloat((tx(x) - x0) / (x1 - x0)) * w }
        func py(_ y: Double) -> CGFloat { top + CGFloat((y1 - ty(y)) / (y1 - y0)) * h }
        func dataX(_ sx: CGFloat) -> Double {
            let t = x0 + Double((sx - left) / w) * (x1 - x0)
            return logX ? pow(10, t) : t
        }
        func dataY(_ sy: CGFloat) -> Double {
            let t = y1 - Double((sy - top) / h) * (y1 - y0)
            return logY ? pow(10, t) : t
        }
        var rect: CGRect { CGRect(x: left, y: top, width: w, height: h) }
        func inside(_ p: CGPoint) -> Bool { p.x >= left && p.x <= left + w && p.y >= top && p.y <= top + h }
    }

    private var map2D: Map2D { Map2D(state, pad: Self.pad, size: CGSize(width: Self.svgW, height: Self.svgH)) }

    private static func stroke(_ cg: CGContext, _ a: CGPoint, _ b: CGPoint, _ color: CGColor, _ width: CGFloat, dash: [CGFloat]? = nil) {
        cg.saveGState()
        if let dash { cg.setLineDash(phase: 0, lengths: dash) }
        cg.move(to: a)
        cg.addLine(to: b)
        cg.setStrokeColor(color)
        cg.setLineWidth(width)
        cg.strokePath()
        cg.restoreGState()
    }

    /// The 2D plot, drawn as a technical plot: a square frame, hairline major and minor grids, tick marks on the
    /// frame with monospaced numbers, solid axes, each equation in its colour, and its points as small squares.
    /// The overlay pass draws the pointer's crosshair and reading and the picked-out point.
    private func draw(_ cg: CGContext, _ part: PlotPart) {
        let g = state, p = Self.pad, prefs = model.graphPrefs, m = map2D
        let plotW = m.w, plotH = m.h
        let bottom = p.top + plotH, right = p.left + plotW
        func px(_ x: Double) -> CGFloat { m.px(x) }
        func py(_ y: Double) -> CGFloat { m.py(y) }
        func line(_ a: CGPoint, _ b: CGPoint, _ color: CGColor, _ width: CGFloat) { Self.stroke(cg, a, b, color, width) }
        let ink = Theme.ink.cgColor, accent = Theme.accent.cgColor, plotBg = Self.plotBgNS.cgColor
        let big = prefs.largeText
        if part == .overlay {
            drawOverlay2D(cg, m: m, ink: ink, accent: accent, plotBg: plotBg)
            return
        }
        if prefs.reverseContrast {
            cg.setFillColor(Theme.bgTape.cgColor)
            cg.fill(CGRect(x: 0, y: 0, width: Self.svgW, height: Self.svgH))
        }
        cg.setFillColor(plotBg)
        cg.fill(m.rect)
        let gridC = ink.copy(alpha: 0.11) ?? ink, minorC = ink.copy(alpha: 0.045) ?? ink
        let axisC = ink.copy(alpha: 0.62) ?? ink, frameC = ink.copy(alpha: 0.3) ?? ink
        let tickFont = Self.mono(big ? 12 : 9.5)
        let gx = axisGrid(g.xMin, g.xMax, log: m.logX, step: g.xStep, engine: g.xTicks)
        let gy = axisGrid(g.yMin, g.yMax, log: m.logY, step: g.yStep, engine: g.yTicks)
        cg.saveGState()
        cg.clip(to: m.rect)
        if prefs.grid {
            if prefs.polar, !m.logX, !m.logY {
                // circles about the origin a step apart (minor ones a fifth), and spokes every 15°
                let step = gx.step ?? (g.xMax - g.xMin) / 6
                let corners = [(g.xMin, g.yMin), (g.xMin, g.yMax), (g.xMax, g.yMin), (g.xMax, g.yMax)]
                let rMax = corners.map { ($0.0 * $0.0 + $0.1 * $0.1).squareRoot() }.max() ?? 0
                func circles(_ every: Double, skip: Int, _ color: CGColor) {
                    guard every > 0, rMax / every <= 1000 else { return }
                    cg.setStrokeColor(color)
                    cg.setLineWidth(0.5)
                    var k = 1.0
                    while k * every <= rMax {
                        if skip == 0 || Int(k) % skip != 0 {
                            let r = k * every
                            cg.strokeEllipse(in: CGRect(x: px(-r), y: py(r), width: px(r) - px(-r), height: py(-r) - py(r)))
                        }
                        k += 1
                    }
                }
                if prefs.minorGrid { circles(step / 5, skip: 5, minorC) }
                circles(step, skip: 0, gridC)
                for d in stride(from: 0, to: 360, by: 15) {
                    let a = Double(d) * .pi / 180
                    line(CGPoint(x: px(0), y: py(0)), CGPoint(x: px(rMax * cos(a)), y: py(rMax * sin(a))), d % 45 == 0 ? gridC : minorC, 0.5)
                }
            } else {
                if prefs.minorGrid {
                    for v in gx.minor { line(CGPoint(x: px(v), y: p.top), CGPoint(x: px(v), y: bottom), minorC, 0.5) }
                    for v in gy.minor { line(CGPoint(x: p.left, y: py(v)), CGPoint(x: right, y: py(v)), minorC, 0.5) }
                }
                for (v, _) in gx.major { line(CGPoint(x: px(v), y: p.top), CGPoint(x: px(v), y: bottom), gridC, 0.5) }
                for (v, _) in gy.major { line(CGPoint(x: p.left, y: py(v)), CGPoint(x: right, y: py(v)), gridC, 0.5) }
            }
        }
        cg.restoreGState()
        // the frame, with tick marks out from it at the labelled values
        cg.setStrokeColor(frameC)
        cg.setLineWidth(0.75)
        cg.stroke(m.rect)
        if prefs.showXAxis {
            for (v, _) in gx.major { line(CGPoint(x: px(v), y: bottom), CGPoint(x: px(v), y: bottom + 4), frameC, 0.75) }
            if prefs.minorGrid { for v in gx.minor { line(CGPoint(x: px(v), y: bottom), CGPoint(x: px(v), y: bottom + 2), frameC, 0.5) } }
        }
        if prefs.showYAxis {
            for (v, _) in gy.major { line(CGPoint(x: p.left - 4, y: py(v)), CGPoint(x: p.left, y: py(v)), frameC, 0.75) }
            if prefs.minorGrid { for v in gy.minor { line(CGPoint(x: p.left - 2, y: py(v)), CGPoint(x: p.left, y: py(v)), frameC, 0.5) } }
        }
        if prefs.axisNumbers {
            if prefs.showXAxis {
                for (v, label) in gx.major {
                    Self.text(label, tickFont, Theme.muted, at: CGPoint(x: px(v), y: Self.svgH - 11), anchor: .center)
                }
            }
            if prefs.showYAxis {
                for (v, label) in gy.major {
                    Self.text(label, tickFont, Theme.muted, at: CGPoint(x: p.left - 7, y: py(v)), anchor: .trailing)
                }
            }
        }
        // the axes (x = 0 and y = 0, none on a log axis), with arrowheads at their ends when asked
        let xAxisY: CGFloat? = !m.logY && py(0) >= p.top && py(0) <= bottom ? py(0) : nil
        let yAxisX: CGFloat? = !m.logX && px(0) >= p.left && px(0) <= right ? px(0) : nil
        func arrow(at tip: CGPoint, dx: CGFloat, dy: CGFloat) {
            let size: CGFloat = 6, half: CGFloat = 3
            let path = CGMutablePath()
            path.move(to: tip)
            path.addLine(to: CGPoint(x: tip.x - dx * size - dy * half, y: tip.y - dy * size + dx * half))
            path.addLine(to: CGPoint(x: tip.x - dx * size + dy * half, y: tip.y - dy * size - dx * half))
            path.closeSubpath()
            cg.addPath(path)
            cg.setFillColor(axisC)
            cg.fillPath()
        }
        if prefs.showXAxis, let y = xAxisY {
            line(CGPoint(x: p.left, y: y), CGPoint(x: right, y: y), axisC, 1)
            if prefs.arrows {
                arrow(at: CGPoint(x: right, y: y), dx: 1, dy: 0)
                arrow(at: CGPoint(x: p.left, y: y), dx: -1, dy: 0)
            }
        }
        if prefs.showYAxis, let x = yAxisX {
            line(CGPoint(x: x, y: p.top), CGPoint(x: x, y: bottom), axisC, 1)
            if prefs.arrows {
                arrow(at: CGPoint(x: x, y: p.top), dx: 0, dy: -1)
                arrow(at: CGPoint(x: x, y: bottom), dx: 0, dy: 1)
            }
        }
        let axisLabelFont = Self.mono(big ? 13 : 11, .medium)
        if prefs.showXAxis, !g.xLabel.isEmpty {
            Self.text(g.xLabel, axisLabelFont, Theme.ink, at: CGPoint(x: right - (prefs.arrows ? 10 : 4), y: (xAxisY ?? bottom) - 4), anchor: .bottomTrailing)
        }
        if prefs.showYAxis, !g.yLabel.isEmpty {
            Self.text(g.yLabel, axisLabelFont, Theme.ink, at: CGPoint(x: (yAxisX ?? p.left) + 6, y: p.top + (prefs.arrows ? 8 : 3)), anchor: .topLeading)
        }
        cg.saveGState()
        cg.clip(to: m.rect)
        func curve(_ points: [GraphPoint], _ color: CGColor) {
            let path = CGMutablePath()
            var pen = false
            var lastY: Double?
            let span = m.y1 - m.y0
            for pt in points {
                guard let v = pt.y, !(m.logY && v <= 0), !(m.logX && pt.x <= 0) else { pen = false; continue }
                if let ly = lastY, abs(m.ty(v) - ly) > span * 4 { pen = false }
                let q = CGPoint(x: px(pt.x), y: max(-1e4, min(1e4, py(v))))
                if pen { path.addLine(to: q) } else { path.move(to: q) }
                pen = true
                lastY = m.ty(v)
            }
            cg.addPath(path)
            cg.setStrokeColor(color)
            cg.setLineWidth(1.6)
            cg.setLineCap(.round)
            cg.setLineJoin(.round)
            cg.strokePath()
        }
        // an implicit curve as one line, a surface's level curves fainter for the low levels, each labelled
        func contours(_ levels: [ContourLevel], implicit: Bool, _ color: CGColor) {
            for (i, c) in levels.enumerated() {
                let s = c.segments
                guard s.count >= 4 else { continue }
                let path = CGMutablePath()
                var k = 0
                while k + 3 < s.count {
                    path.move(to: CGPoint(x: px(s[k]), y: py(s[k + 1])))
                    path.addLine(to: CGPoint(x: px(s[k + 2]), y: py(s[k + 3])))
                    k += 4
                }
                let opacity = levels.count > 1 ? 0.35 + 0.65 * CGFloat(i) / CGFloat(levels.count - 1) : 1
                cg.addPath(path)
                cg.setStrokeColor(color.copy(alpha: opacity) ?? color)
                cg.setLineWidth(implicit ? 1.6 : 1)
                cg.setLineCap(.round)
                cg.strokePath()
            }
        }
        for (i, layer) in g.layers.enumerated() where layer.error == nil {
            let color = Self.layerColor(i + 1)
            if layer.kind == .curve { curve(layer.points, color) } else { contours(layer.contours, implicit: layer.kind == .implicit, color) }
        }
        for c in g.curves { curve(c.points, accent) }
        let levels = g.contours
        contours(levels, implicit: g.kind == .implicit, accent)
        if g.kind == .surface {
            let labelFont = Self.mono(big ? 10.5 : 8.5)
            for c in levels {
                guard let ax = c.atX, let ay = c.atY else { continue }
                let at = CGPoint(x: px(ax), y: py(ay))
                let str = NSAttributedString(string: Self.tickText(c.label), attributes: [.font: labelFont])
                let box = str.size()
                cg.setFillColor(plotBg)
                cg.fill(CGRect(x: at.x - box.width / 2 - 2, y: at.y - box.height / 2, width: box.width + 4, height: box.height))
                Self.text(Self.tickText(c.label), labelFont, Theme.muted, at: at, anchor: .center)
            }
        }
        cg.restoreGState()
        // roots and critical points: small hollow squares in their equation's colour
        cg.saveGState()
        cg.clip(to: m.rect.insetBy(dx: -4, dy: -4))
        for (i, layer) in g.layers.enumerated() where layer.error == nil && layer.kind != .surface {
            let color = Self.layerColor(i + 1)
            let pts = m.logY ? [] : layer.roots.map { CGPoint(x: px($0), y: py(0)) }
            for q in pts where m.inside(q) { Self.ring(cg, at: q, plotBg: plotBg, accent: color) }
        }
        let rootMarks = m.logY ? [] : g.roots.map { CGPoint(x: px($0), y: py(0)) }
        for q in rootMarks where m.inside(q) { Self.ring(cg, at: q, plotBg: plotBg, accent: accent) }
        for (q, e) in zip(markPoints(), allCritical) {
            if let q, m.inside(q) { Self.ring(cg, at: q, plotBg: plotBg, accent: Self.layerColor(e.layer)) }
        }
        cg.restoreGState()
    }

    /// The 2D plot's pointer pass: the crosshair and reading (each curve's value at the pointer's x, or the point
    /// under it on a plot of x and y), and the picked-out critical point with guides to the frame.
    private func drawOverlay2D(_ cg: CGContext, m: Map2D, ink: CGColor, accent: CGColor, plotBg: CGColor) {
        let g = state, p = Self.pad, prefs = model.graphPrefs
        let bottom = p.top + m.h, right = p.left + m.w
        let hair = ink.copy(alpha: 0.35) ?? ink
        let snapped = hoveredMark
        if snapped == nil, let h = hover, m.inside(h) {
            let x = m.dataX(h.x)
            Self.stroke(cg, CGPoint(x: h.x, y: p.top), CGPoint(x: h.x, y: bottom), hair, 0.5, dash: [2, 2])
            if g.kind == .curve {
                // a mark on each curve at the pointer's x; the reading is the one nearest the pointer
                var dots: [(Double, CGColor, Int)] = []
                if let y = curveY(at: x) { dots.append((y, accent, 0)) }
                for (i, layer) in g.layers.enumerated() where layer.kind == .curve && layer.error == nil && !layer.isBlank {
                    if let y = model.graphReadout(x, input: layer.input) { dots.append((y, Self.layerColor(i + 1), i + 1)) }
                }
                let shown = dots.filter { m.py($0.0) >= p.top && m.py($0.0) <= bottom && !(m.logY && $0.0 <= 0) }
                if let near = shown.min(by: { abs(m.py($0.0) - h.y) < abs(m.py($1.0) - h.y) }) {
                    Self.stroke(cg, CGPoint(x: p.left, y: m.py(near.0)), CGPoint(x: right, y: m.py(near.0)), hair, 0.5, dash: [2, 2])
                    Self.readoutBox(cg, ["x  \(fmt4(x))", "y  \(fmt4(near.0))"], near: CGPoint(x: h.x, y: m.py(near.0)), in: m.rect,
                                    plotBg: plotBg, edge: near.1, big: prefs.largeText)
                }
                for (y, color, _) in shown { Self.dot(cg, at: CGPoint(x: h.x, y: m.py(y)), color: color, plotBg: plotBg) }
            } else {
                // a plot of x and y reads out the point under the pointer (and a surface's height there)
                let y = m.dataY(h.y)
                Self.stroke(cg, CGPoint(x: p.left, y: h.y), CGPoint(x: right, y: h.y), hair, 0.5, dash: [2, 2])
                var lines = ["x  \(fmt4(x))", "y  \(fmt4(y))"]
                if g.kind == .surface {
                    lines.append("z  " + (model.graphReadoutZ(x, y).map { Self.tickText(fmt4($0)) } ?? "—"))
                }
                Self.readoutBox(cg, lines, near: h, in: m.rect, plotBg: plotBg, edge: ink.copy(alpha: 0.3) ?? ink, big: prefs.largeText)
            }
        }
        // the chip under the pointer or keyboard, or the mark under the pointer: dashed guides to the left and
        // bottom edges (where the ticks read), the mark filled with a square round it
        var lit: CriticalPoint?
        let all = allCritical
        if let i = snapped { lit = all[i].c } else if let s = spot { lit = all.first { $0.c.x == s.x && $0.c.y == s.y }?.c }
        if let c0 = lit {
            let c = CGPoint(x: m.px(c0.x), y: m.py(c0.y))
            if m.inside(c) {
                let guide = accent.copy(alpha: 0.6) ?? accent
                Self.stroke(cg, CGPoint(x: c.x, y: c.y + 6), CGPoint(x: c.x, y: bottom), guide, 0.75, dash: [3, 2])
                Self.stroke(cg, CGPoint(x: c.x - 6, y: c.y), CGPoint(x: p.left, y: c.y), guide, 0.75, dash: [3, 2])
                Self.halo(cg, at: c, plotBg: plotBg, accent: accent)
                if snapped != nil {
                    Self.markLabel(cg, "\(kindName(c0.kind)) \(coords(c0))", at: c, in: m.rect, plotBg: plotBg)
                }
            }
        }
    }

    /// Where each critical point's mark sits, in viewBox units (nil when it's off the plot): on the 2D plot, or
    /// projected into the 3D view's cube.
    private func markPoints() -> [CGPoint?] {
        let g = state
        if g.shows3D {
            guard let v = Surface3D(state: g, size: CGSize(width: Self.svgW, height: Self.svgH3D)) else {
                return allCritical.map { _ in nil }
            }
            return allCritical.map { v.project($0.c.x, $0.c.y, $0.c.z ?? .nan).0 }
        }
        let m = map2D
        return allCritical.map { e in
            let c = e.c
            let q = CGPoint(x: m.px(c.x), y: m.py(c.y))
            return m.inside(q) ? q : nil
        }
    }

    /// A mark: a small hollow square in the curve's colour.
    private static func ring(_ cg: CGContext, at m: CGPoint, plotBg: CGColor, accent: CGColor) {
        let r = CGRect(x: m.x - 3, y: m.y - 3, width: 6, height: 6)
        cg.setFillColor(plotBg)
        cg.fill(r)
        cg.setStrokeColor(accent)
        cg.setLineWidth(1.25)
        cg.stroke(r)
    }

    /// The picked-out mark: filled, with a square round it.
    private static func halo(_ cg: CGContext, at c: CGPoint, plotBg: CGColor, accent: CGColor) {
        let outer = CGRect(x: c.x - 7, y: c.y - 7, width: 14, height: 14)
        cg.setStrokeColor(accent)
        cg.setLineWidth(1)
        cg.stroke(outer)
        let inner = CGRect(x: c.x - 3.5, y: c.y - 3.5, width: 7, height: 7)
        cg.setFillColor(accent)
        cg.fill(inner)
    }

    /// The reading's point on a curve: a small filled square.
    private static func dot(_ cg: CGContext, at c: CGPoint, color: CGColor, plotBg: CGColor) {
        let r = CGRect(x: c.x - 3, y: c.y - 3, width: 6, height: 6)
        cg.setFillColor(color)
        cg.fill(r)
        cg.setStrokeColor(plotBg)
        cg.setLineWidth(1)
        cg.stroke(r)
    }

    /// The pointed-at point's name and place in a box beside it, kept inside the plot.
    private static func markLabel(_ cg: CGContext, _ s: String, at c: CGPoint, in box: CGRect, plotBg: CGColor) {
        readoutBox(cg, [s], near: c, in: box, plotBg: plotBg, edge: Theme.ink.cgColor.copy(alpha: 0.3) ?? Theme.ink.cgColor, big: false)
    }

    /// Lines of monospaced text in a square box with a hairline edge, beside `near` (flipped to stay in `box`)
    private static func readoutBox(_ cg: CGContext, _ lines: [String], near c: CGPoint, in box: CGRect, plotBg: CGColor, edge: CGColor, big: Bool) {
        let font = mono(big ? 12.5 : 10.5)
        let strs = lines.map { NSAttributedString(string: $0, attributes: [.font: font, .foregroundColor: Theme.ink]) }
        let lineH = (strs.first?.size().height ?? 12)
        let w = strs.map { $0.size().width }.max() ?? 0, h = lineH * CGFloat(strs.count)
        var x = c.x + 12, y = c.y - h - 10
        if x + w + 10 > box.maxX { x = c.x - 12 - w }
        if y < box.minY + 4 { y = c.y + 10 }
        let frame = CGRect(x: x - 5, y: y - 3, width: w + 10, height: h + 6)
        cg.setFillColor(plotBg.copy(alpha: 0.96) ?? plotBg)
        cg.fill(frame)
        cg.setStrokeColor(edge)
        cg.setLineWidth(0.75)
        cg.stroke(frame)
        for (i, s) in strs.enumerated() { s.draw(at: CGPoint(x: x, y: y + CGFloat(i) * lineH)) }
    }

    /// A piece of the 3D scene, drawn back to front: a patch of a surface (its fill and its mesh line), or a short
    /// length of a grid line or axis
    fileprivate enum Piece {
        case patch([CGPoint], CGColor, CGColor)
        case segment(CGPoint, CGPoint, CGColor, CGFloat)
    }

    /// A drawn patch of a surface, for finding the point under the pointer: its corners on the view and in the
    /// cube, and which equation it's of
    fileprivate struct Patch {
        let depth: Double
        let screen: [CGPoint]
        let cube: [(Double, Double, Double)]
        let layer: Int
    }

    /// The 3D scene's patches from the last base pass, for the pointer pass to find what's under the pointer
    /// without building them again
    final class SceneCache {
        fileprivate var key: PlotKey?
        fileprivate var patches: [Patch] = []
    }

    /// Where the 3D view puts its plane and axes: the plane at z = 0 (the cube's floor when 0 is out of it), the
    /// axes through the origin (along the walls when it's outside)
    private struct Frame3D {
        let v: Surface3D
        let w0: Double, clampU: Double, clampV: Double
        init(_ v: Surface3D, _ g: GraphState) {
            self.v = v
            w0 = g.zWindow.contains(0) ? v.w(0) : -1
            clampU = min(max(v.u(0), -1), 1)
            clampV = min(max(v.v(0), -1), 1)
        }
    }

    /// The 3D view: a cube (x and y the window, z the same width about 0 unless set) that cuts each surface off
    /// at its walls, the z = 0 plane's grid, the axes through the origin, and the surfaces shaded in their
    /// equations' colours with their mesh drawn, all back to front; the critical points marked. The overlay pass
    /// reads out the point under the pointer and the picked-out critical point.
    private func draw3D(_ cg: CGContext, _ part: PlotPart, key: PlotKey) {
        let g = state, prefs = model.graphPrefs
        let size = CGSize(width: Self.svgW, height: Self.svgH3D)
        let plotBg = Self.plotBgNS.cgColor, accent = Theme.accent.cgColor, ink = Theme.ink.cgColor
        let frame = CGRect(origin: .zero, size: size).insetBy(dx: 0.5, dy: 2)
        guard let v = Surface3D(state: g, size: size) else {
            if part == .base {
                cg.setFillColor(plotBg)
                cg.fill(frame)
            }
            return
        }
        let f = Frame3D(v, g)
        if part == .overlay {
            let patches = sceneCache.key == key ? sceneCache.patches : scene3D(f).1
            drawOverlay3D(cg, f, patches: patches, frame: frame, ink: ink, accent: accent, plotBg: plotBg)
            return
        }
        if prefs.reverseContrast {
            cg.setFillColor(Theme.bgTape.cgColor)
            cg.fill(CGRect(origin: .zero, size: size))
        }
        cg.setFillColor(plotBg)
        cg.fill(frame)
        cg.setStrokeColor(ink.copy(alpha: 0.3) ?? ink)
        cg.setLineWidth(0.75)
        cg.stroke(frame)
        let big = prefs.largeText
        let font = Self.mono(big ? 11.5 : 9.5)
        func seg(_ a: (Double, Double, Double), _ b: (Double, Double, Double), _ color: CGColor, _ w: CGFloat = 0.75) {
            Self.stroke(cg, v.unit(a.0, a.1, a.2).0, v.unit(b.0, b.1, b.2).0, color, w)
        }
        // the cube: the edges at its nearest corner go over everything, the rest under
        var corners: [(Double, Double, Double)] = []
        for a in [-1.0, 1] { for b in [-1.0, 1] { for c in [-1.0, 1] { corners.append((a, b, c)) } } }
        let nearest: (Double, Double, Double) = corners.min { v.unit($0.0, $0.1, $0.2).1 < v.unit($1.0, $1.1, $1.2).1 } ?? (1.0, 1.0, 1.0)
        var frontEdges: [((Double, Double, Double), (Double, Double, Double))] = []
        let edgeC = ink.copy(alpha: 0.32) ?? ink
        for a in corners {
            for b in corners where (a.0 < b.0 && a.1 == b.1 && a.2 == b.2) || (a.0 == b.0 && a.1 < b.1 && a.2 == b.2) || (a.0 == b.0 && a.1 == b.1 && a.2 < b.2) {
                if a == nearest || b == nearest { frontEdges.append((a, b)) } else { seg(a, b, edgeC) }
            }
        }
        let (pieces, patches) = scene3D(f)
        sceneCache.key = key
        sceneCache.patches = patches
        cg.setLineCap(.round)
        cg.setLineJoin(.round)
        for (_, piece) in pieces {
            switch piece {
            case let .patch(pts, fill, mesh):
                // fill and mesh line in one call (the mesh also closes the seams between patches)
                cg.addLines(between: pts)
                cg.closePath()
                cg.setFillColor(fill)
                cg.setStrokeColor(mesh)
                cg.setLineWidth(0.5)
                cg.drawPath(using: .fillStroke)
            case let .segment(a, b, color, w):
                cg.move(to: a)
                cg.addLine(to: b)
                cg.setStrokeColor(color)
                cg.setLineWidth(w)
                cg.strokePath()
            }
        }
        for (a, b) in frontEdges { seg(a, b, edgeC) }
        // the axes' arrows and names, and their numbers
        let axisC = ink.copy(alpha: 0.7) ?? ink
        func label(_ s: String, _ p: (Double, Double, Double), dx: CGFloat = 0, dy: CGFloat = 0, name: Bool = false) {
            let q = v.unit(p.0, p.1, p.2).0
            Self.text(s, name ? Self.mono(big ? 13 : 11, .medium) : font, name ? Theme.ink : Theme.muted,
                      at: CGPoint(x: q.x + dx, y: q.y + dy), anchor: .center)
        }
        func arrowhead(from a: (Double, Double, Double), to b: (Double, Double, Double)) {
            let tip = v.unit(b.0, b.1, b.2).0, back = v.unit(a.0, a.1, a.2).0
            let len = max(hypot(tip.x - back.x, tip.y - back.y), 0.001)
            let dx = (tip.x - back.x) / len, dy = (tip.y - back.y) / len
            let path = CGMutablePath()
            path.move(to: tip)
            path.addLine(to: CGPoint(x: tip.x - dx * 6 - dy * 2.5, y: tip.y - dy * 6 + dx * 2.5))
            path.addLine(to: CGPoint(x: tip.x - dx * 6 + dy * 2.5, y: tip.y - dy * 6 - dx * 2.5))
            path.closeSubpath()
            cg.addPath(path)
            cg.setFillColor(axisC)
            cg.fillPath()
        }
        let (w0, cu, cv) = (f.w0, f.clampU, f.clampV)
        if prefs.showXAxis {
            arrowhead(from: (0.9, cv, w0), to: (1.05, cv, w0))
            label(g.xLabel.isEmpty ? "x" : g.xLabel, (1.14, cv, w0), name: true)
        }
        if prefs.showYAxis {
            arrowhead(from: (cu, 0.9, w0), to: (cu, 1.05, w0))
            label(g.yLabel.isEmpty ? "y" : g.yLabel, (cu, 1.14, w0), name: true)
        }
        arrowhead(from: (cu, cv, 0.9), to: (cu, cv, 1.05))
        label("z", (cu, cv, 1.13), name: true)
        if prefs.axisNumbers {
            // at most about four numbers an axis, so they stay clear of each other at the origin
            func thin(_ major: [(Double, String)]) -> [(Double, String)] {
                guard major.count > 5 else { return major }
                let step = major[1].0 - major[0].0, every = Int((Double(major.count) / 4).rounded(.up))
                return major.filter { Int(($0.0 / step).rounded()) % every == 0 }
            }
            let (gx, gy) = grids3D(v)
            if prefs.showXAxis {
                for (t, s) in thin(gx.major) where t != 0 && abs(v.u(t)) < 0.97 { label(s, (v.u(t), cv, w0), dy: 8) }
            }
            if prefs.showYAxis {
                for (t, s) in thin(gy.major) where t != 0 && abs(v.v(t)) < 0.97 { label(s, (cu, v.v(t), w0), dy: 8) }
            }
            let zw = g.zWindow
            for t in ticks(zw.lowerBound, zw.upperBound, count: 4) where t != 0 && abs(v.w(t)) < 0.97 {
                label(Self.tickText(fmt4(t)), (cu, cv, v.w(t)), dx: -18)
            }
        }
        for (m, e) in zip(markPoints(), allCritical) { if let m { Self.ring(cg, at: m, plotBg: plotBg, accent: Self.layerColor(e.layer)) } }
    }

    /// the 3D view's x and y grid values (on the z = 0 plane); y takes x's step, the two sharing a scale
    private func grids3D(_ v: Surface3D) -> (AxisGrid, AxisGrid) {
        let gx = axisGrid(v.xLo, v.xHi, log: false, step: state.xStep, engine: state.xTicks)
        let gy = axisGrid(v.yLo, v.yHi, log: false, step: state.yStep ?? gx.step, engine: [])
        return (gx, gy)
    }

    /// The 3D scene's depth-sorted pieces (the plane's grid and the axes in short lengths, the surfaces' patches
    /// cut off at the cube's top and bottom, lit by their normals) and the patches for finding the pointer
    private func scene3D(_ f: Frame3D) -> ([(Double, Piece)], [Patch]) {
        let g = state, prefs = model.graphPrefs, v = f.v, w0 = f.w0
        let ink = Theme.ink.cgColor, accent = Theme.accent.cgColor
        var pieces: [(Double, Piece)] = []
        var patches: [Patch] = []
        pieces.reserveCapacity(2600)
        func addLine(_ a: (Double, Double, Double), _ b: (Double, Double, Double), _ color: CGColor, _ w: CGFloat, parts: Int) {
            for k in 0..<parts {
                let t0 = Double(k) / Double(parts), t1 = Double(k + 1) / Double(parts)
                let (q0, d0) = v.unit(a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0, a.2 + (b.2 - a.2) * t0)
                let (q1, d1) = v.unit(a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1, a.2 + (b.2 - a.2) * t1)
                // a hair in front of a surface at the same place
                pieces.append(((d0 + d1) / 2 - 0.004, .segment(q0, q1, color, w)))
            }
        }
        if prefs.grid {
            let (gx, gy) = grids3D(v)
            let major = ink.copy(alpha: 0.16) ?? ink, minor = ink.copy(alpha: 0.06) ?? ink
            if prefs.minorGrid {
                for t in gx.minor { addLine((v.u(t), -1, w0), (v.u(t), 1, w0), minor, 0.5, parts: 6) }
                for t in gy.minor { addLine((-1, v.v(t), w0), (1, v.v(t), w0), minor, 0.5, parts: 6) }
            }
            for (t, _) in gx.major { addLine((v.u(t), -1, w0), (v.u(t), 1, w0), major, 0.5, parts: 8) }
            for (t, _) in gy.major { addLine((-1, v.v(t), w0), (1, v.v(t), w0), major, 0.5, parts: 8) }
        }
        let axisC = ink.copy(alpha: 0.7) ?? ink
        if prefs.showXAxis { addLine((-1, f.clampV, w0), (1, f.clampV, w0), axisC, 1, parts: 16) }
        if prefs.showYAxis { addLine((f.clampU, -1, w0), (f.clampU, 1, w0), axisC, 1, parts: 16) }
        addLine((f.clampU, f.clampV, -1), (f.clampU, f.clampV, 1), axisC, 1, parts: 16)
        let light = Surface3D.normalize((-0.35, -0.45, 0.82))
        var grids: [(SurfaceGrid, Int)] = []
        if let s = g.surface { grids.append((s, 0)) }
        for (i, layer) in g.layers.enumerated() where layer.error == nil { if let s = layer.surface { grids.append((s, i + 1)) } }
        for (grid, layer) in grids {
            let base = Self.layerRGB(layer) ?? Self.rgb(accent)
            // the fill and mesh colours by shade, made once (64 steps can't be told apart), not twice a patch
            let steps = 64
            let fills = (0...steps).map { k -> CGColor in
                let t = 0.42 + 0.58 * Double(k) / Double(steps)
                return CGColor(srgbRed: base.0 * t, green: base.1 * t, blue: base.2 * t, alpha: 1)
            }
            let meshes = (0...steps).map { k -> CGColor in
                let t = (0.42 + 0.58 * Double(k) / Double(steps)) * 0.62
                return CGColor(srgbRed: base.0 * t, green: base.1 * t, blue: base.2 * t, alpha: 1)
            }
            let nx = Int(grid.nx), ny = Int(grid.ny)
            // the grid's corners in the cube once, not four times
            var cube = [(Double, Double, Double)?](repeating: nil, count: (nx + 1) * (ny + 1))
            for j in 0...ny {
                for i in 0...nx {
                    let z = grid.z[j * (nx + 1) + i]
                    guard z.isFinite else { continue }
                    cube[j * (nx + 1) + i] = (v.u(grid.xLo + Double(i) / Double(nx) * (grid.xHi - grid.xLo)),
                                              v.v(grid.yLo + Double(j) / Double(ny) * (grid.yHi - grid.yLo)), v.w(z))
                }
            }
            for j in 0..<ny {
                for i in 0..<nx {
                    let k = j * (nx + 1) + i
                    guard let a = cube[k], let b = cube[k + 1], let c = cube[k + nx + 2], let d = cube[k + nx + 1] else { continue }
                    // wholly above or below the cube: nothing to draw
                    if a.2 > 1, b.2 > 1, c.2 > 1, d.2 > 1 { continue }
                    if a.2 < -1, b.2 < -1, c.2 < -1, d.2 < -1 { continue }
                    let poly = Surface3D.clip([a, b, c, d])
                    guard poly.count >= 3 else { continue }
                    let n = Surface3D.normalize(Surface3D.cross((c.0 - a.0, c.1 - a.1, c.2 - a.2), (d.0 - b.0, d.1 - b.1, d.2 - b.2)))
                    let level = Int((abs(n.0 * light.0 + n.1 * light.1 + n.2 * light.2) * Double(steps)).rounded())
                    let projected = poly.map { v.unit($0.0, $0.1, $0.2) }
                    let depth = projected.reduce(0) { $0 + $1.1 } / Double(projected.count)
                    let screen = projected.map(\.0)
                    pieces.append((depth, .patch(screen, fills[min(level, steps)], meshes[min(level, steps)])))
                    patches.append(Patch(depth: depth, screen: screen, cube: poly, layer: layer))
                }
            }
        }
        pieces.sort { $0.0 > $1.0 }
        return (pieces, patches)
    }

    /// The 3D view's pointer pass: the picked-out critical point with a drop to the plane and its label, or the
    /// point of the surface under the pointer with guides to the plane and axes and its (x, y, z)
    private func drawOverlay3D(_ cg: CGContext, _ f: Frame3D, patches: [Patch], frame: CGRect, ink: CGColor, accent: CGColor, plotBg: CGColor) {
        let g = state, v = f.v, prefs = model.graphPrefs
        let marks = markPoints()
        let snapped = hoveredMark
        var litIndex = snapped
        let all = allCritical
        if litIndex == nil, let s = spot { litIndex = all.firstIndex { $0.c.x == s.x && $0.c.y == s.y } }
        if let i = litIndex, let m = marks[i] {
            let c0 = all[i].c
            Self.stroke(cg, m, v.unit(v.u(c0.x), v.v(c0.y), f.w0).0, accent.copy(alpha: 0.7) ?? accent, 0.75, dash: [3, 2])
            Self.halo(cg, at: m, plotBg: plotBg, accent: accent)
            if snapped != nil {
                Self.markLabel(cg, "\(kindName(c0.kind)) \(coords(c0))", at: m, in: frame, plotBg: plotBg)
            }
        }
        var onSurface = false
        if let h = hover, snapped == nil, let hit = Self.patchHit(at: h, patches) {
            let x = v.xLo + (hit.0 + 1) / 2 * (v.xHi - v.xLo), y = v.yLo + (hit.1 + 1) / 2 * (v.yHi - v.yLo)
            let layerInput = hit.layer == 0 ? nil : g.layers[hit.layer - 1].input
            let zGrid = v.zLo + (hit.2 + 1) / 2 * (v.zHi - v.zLo)
            let z = model.graphReadoutZ(x, y, input: layerInput) ?? zGrid
            if z.isFinite, let q = v.project(x, y, z).0 {
                onSurface = true
                let guide = ink.copy(alpha: 0.6) ?? ink
                // down (or up) to the plane, then across it to the axes, where the numbers are
                let foot = v.unit(v.u(x), v.v(y), f.w0).0
                Self.stroke(cg, q, foot, guide, 0.75, dash: [2, 2])
                Self.stroke(cg, v.unit(v.u(x), f.clampV, f.w0).0, foot, guide, 0.75, dash: [2, 2])
                Self.stroke(cg, foot, v.unit(f.clampU, v.v(y), f.w0).0, guide, 0.75, dash: [2, 2])
                Self.dot(cg, at: q, color: hit.layer == 0 ? ink : Self.layerColor(hit.layer), plotBg: plotBg)
                Self.readoutBox(cg, ["x  \(Self.tickText(fmt4(x)))", "y  \(Self.tickText(fmt4(y)))", "z  \(Self.tickText(fmt4(z)))"],
                                near: q, in: frame, plotBg: plotBg, edge: Self.layerColor(hit.layer), big: prefs.largeText)
            }
        }
        if hover != nil, snapped == nil, !onSurface {
            Self.text("drag/2-finger press: orbit · scroll: zoom · ⇧: pan · dbl-click: reset", Self.mono(9),
                      Theme.muted, at: CGPoint(x: frame.maxX - 8, y: frame.minY + 12), anchor: .trailing)
        }
    }

    /// a colour's sRGB components
    private static func rgb(_ c: CGColor) -> (Double, Double, Double) {
        let s = c.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil)?.components ?? [0.4, 0.7, 0.5]
        return s.count >= 3 ? (Double(s[0]), Double(s[1]), Double(s[2])) : (Double(s[0]), Double(s[0]), Double(s[0]))
    }

    /// The nearest drawn patch under `p` and the place in the cube there (u, v, w), and its equation
    private static func patchHit(at p: CGPoint, _ patches: [Patch]) -> (Double, Double, Double, layer: Int)? {
        func bary(_ a: CGPoint, _ b: CGPoint, _ c: CGPoint) -> (Double, Double, Double)? {
            let d = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y)
            guard abs(d) > 1e-9 else { return nil }
            let l1 = ((b.y - c.y) * (p.x - c.x) + (c.x - b.x) * (p.y - c.y)) / d
            let l2 = ((c.y - a.y) * (p.x - c.x) + (a.x - c.x) * (p.y - c.y)) / d
            let l3 = 1 - l1 - l2
            return l1 >= -1e-6 && l2 >= -1e-6 && l3 >= -1e-6 ? (Double(l1), Double(l2), Double(l3)) : nil
        }
        var best: (Double, (Double, Double, Double), Int)?
        for patch in patches {
            if let b = best, patch.depth >= b.0 { continue }
            for k in 1..<(patch.screen.count - 1) {
                guard let (l1, l2, l3) = bary(patch.screen[0], patch.screen[k], patch.screen[k + 1]) else { continue }
                let a = patch.cube[0], b = patch.cube[k], c = patch.cube[k + 1]
                best = (patch.depth, (l1 * a.0 + l2 * b.0 + l3 * c.0, l1 * a.1 + l2 * b.1 + l3 * c.1, l1 * a.2 + l2 * b.2 + l3 * c.2), patch.layer)
                break
            }
        }
        return best.map { ($0.1.0, $0.1.1, $0.1.2, layer: $0.2) }
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

/// .graph-detail-item: fill-track, accent tint on hover; the kind in caption muted, the point in ink
private struct CriticalChip: View {
    let kind: String
    let coords: String
    /// which coordinate a click copies
    let copies: String
    /// its mark is under the pointer on the plot
    var pointed = false
    /// its equation's colour, when there are several equations: a small square before the kind
    var color: Color? = nil
    let action: () -> Void
    /// hovered or keyboard-focused: the panel rings this point on the plot
    let highlight: (Bool) -> Void
    @State private var hover = false
    @FocusState private var focused: Bool

    private var lit: Bool { hover || focused || pointed }

    var body: some View {
        Button(action: action) {
            // native: a plain monospaced cell with a hairline edge (the page's is a filled pill)
            HStack(spacing: 6) {
                if let color { Rectangle().fill(color).frame(width: 6, height: 6) }
                Text(kind).font(.system(size: Theme.caption, design: .monospaced)).foregroundStyle(lit ? Color.qAccent : Color.qMuted)
                Text(coords).font(.system(size: Theme.small, design: .monospaced)).foregroundStyle(lit ? Color.qAccent : Color.qInk)
            }
            .padding(.vertical, 3).padding(.horizontal, 7)
            .background(Rectangle().fill(lit ? Color.qAccent.opacity(0.1) : Color.clear))
            .overlay(Rectangle().strokeBorder(lit ? Color.qAccent.opacity(0.6) : Color.qInk.opacity(0.16), lineWidth: 0.75))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focused($focused)
        .onHover { hover = $0 }
        .onChange(of: lit) { highlight($0) }
        .onDisappear { if lit { highlight(false) } }
        .hoverTip("Copy \(copies)")
        .accessibilityLabel("\(kind) \(coords)")
        .accessibilityHint("Copies \(copies)")
    }
}

/// The 3D view's input, native only: a clear AppKit view over the plot that takes the mouse and trackpad itself.
/// As Onshape: a two-finger press-drag (a right-drag) orbits, two fingers up/down zoom and shift + two-finger
/// drag pans (`scrollGraph`); a plain drag orbits too, shift/option-drag or a middle-drag pans, a wheel or pinch zooms, a double-click resets, and the pointer is reported for the marks' labels. It never lets a
/// mouse-down start a drag of the bar's window (the panel moves by its background).
private struct PlotInput: NSViewRepresentable {
    /// the pointer moved this far while pressed; true to pan rather than orbit
    let drag: (CGSize, Bool) -> Void
    let scroll: (NSEvent, CGSize) -> Bool
    let magnify: (CGFloat) -> Void
    let hover: (CGPoint?) -> Void
    let home: () -> Void

    func makeNSView(context: Context) -> InputView { InputView() }

    func updateNSView(_ view: InputView, context: Context) { view.input = self }

    final class InputView: NSView {
        var input: PlotInput?
        private var last: NSPoint?

        override var isFlipped: Bool { true }
        override var mouseDownCanMoveWindow: Bool { false }
        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
        override func hitTest(_ point: NSPoint) -> NSView? {
            frame.contains(point) ? self : nil
        }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            trackingAreas.forEach(removeTrackingArea)
            addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                           owner: self))
        }

        private func local(_ event: NSEvent) -> NSPoint { convert(event.locationInWindow, from: nil) }

        override func mouseMoved(with event: NSEvent) { input?.hover(local(event)) }
        override func mouseEntered(with event: NSEvent) { input?.hover(local(event)) }
        override func mouseExited(with event: NSEvent) { input?.hover(nil) }

        override func mouseDown(with event: NSEvent) {
            if event.clickCount == 2 {
                input?.home()
                return
            }
            last = local(event)
        }

        private func dragged(_ event: NSEvent, pan: Bool) {
            let p = local(event)
            if let l = last { input?.drag(CGSize(width: p.x - l.x, height: p.y - l.y), pan) }
            last = p
            input?.hover(p)
        }

        private func panKey(_ event: NSEvent) -> Bool { !event.modifierFlags.isDisjoint(with: [.shift, .option]) }

        override func mouseDragged(with event: NSEvent) { dragged(event, pan: panKey(event)) }
        override func mouseUp(with event: NSEvent) { last = nil }

        // a trackpad's two-finger press is a right-click: its drag orbits, and no menu opens
        override func rightMouseDown(with event: NSEvent) { last = local(event) }
        override func rightMouseDragged(with event: NSEvent) { dragged(event, pan: panKey(event)) }
        override func rightMouseUp(with event: NSEvent) { last = nil }
        override func menu(for event: NSEvent) -> NSMenu? { nil }

        override func otherMouseDown(with event: NSEvent) { last = local(event) }
        override func otherMouseDragged(with event: NSEvent) { dragged(event, pan: true) }
        override func otherMouseUp(with event: NSEvent) { last = nil }

        override func scrollWheel(with event: NSEvent) {
            if input?.scroll(event, bounds.size) != true { super.scrollWheel(with: event) }
        }

        override func magnify(with event: NSEvent) { input?.magnify(event.magnification) }
    }
}

/// Native only: an icon button in the head (add an equation, the settings), drawn as "reset" is.
private struct HeadIcon: View {
    let symbol: String
    let tip: String
    var on = false
    let action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(on || hover ? Color.qInk : Color.qMuted)
                .frame(width: 22, height: 20)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(on ? Color.qInk.opacity(0.08) : hover ? Color.qFillHover : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.vertical, -3)
        .onHover { hover = $0 }
        .hoverTip(tip)
        .accessibilityLabel(tip)
    }
}

/// The head's 3D / contour switch for a surface: as "reset" is drawn, the chosen one in ink on the hover fill.
private struct ViewToggle: View {
    let title: String
    let on: Bool
    let tip: String
    let action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: Theme.small, weight: on ? .semibold : .regular))
                .foregroundStyle(on || hover ? Color.qInk : Color.qMuted)
                .padding(.vertical, 3).padding(.horizontal, 8)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(on ? Color.qInk.opacity(0.08) : hover ? Color.qFillHover : .clear))
        }
        .buttonStyle(.plain)
        .padding(.vertical, -3)
        .onHover { hover = $0 }
        .hoverTip(tip)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// The 3D view's projection, as Desmos 3D's: the window's x and y and the cube's z each onto −1…1 (a cube), turned
/// by `yaw` about the vertical and tilted by `pitch`, with a little perspective. y runs toward the viewer at yaw 0,
/// so the axes are right-handed.
struct Surface3D {
    let xLo: Double, xHi: Double, yLo: Double, yHi: Double, zLo: Double, zHi: Double
    let yaw: Double, pitch: Double
    let cx: CGFloat, cy: CGFloat, s: CGFloat

    init?(state: GraphState, size: CGSize) {
        let x = state.surface.map { ($0.xLo, $0.xHi) } ?? (state.xMin, state.xMax)
        let yc = state.yCenter ?? 0, half = (state.xMax - state.xMin) / 2
        let y = state.surface.map { ($0.yLo, $0.yHi) } ?? (yc - half, yc + half)
        let z = state.zWindow
        guard x.1 > x.0, y.1 > y.0, z.upperBound > z.lowerBound else { return nil }
        (xLo, xHi, yLo, yHi, zLo, zHi) = (x.0, x.1, y.0, y.1, z.lowerBound, z.upperBound)
        (yaw, pitch) = (state.yaw, state.pitch)
        let w = size.width - 24, h = size.height - 24
        // room for the cube and its axis names at any turn and tilt
        s = min(w / 3.4, h / 3.9)
        cx = 12 + w / 2
        cy = 12 + h / 2
    }

    func u(_ x: Double) -> Double { (x - xLo) / (xHi - xLo) * 2 - 1 }
    func v(_ y: Double) -> Double { (y - yLo) / (yHi - yLo) * 2 - 1 }
    func w(_ z: Double) -> Double { (z - zLo) / (zHi - zLo) * 2 - 1 }

    /// A point of the unit cube on the view, and how far back it is (larger is farther).
    func unit(_ u: Double, _ v: Double, _ w: Double) -> (CGPoint, Double) {
        let x = u * cos(yaw) - v * sin(yaw)
        let y = -(u * sin(yaw) + v * cos(yaw))
        let up = w * cos(pitch) + y * sin(pitch)
        let depth = y * cos(pitch) - w * sin(pitch)
        let k = 1 / (1 + 0.12 * depth)
        return (CGPoint(x: cx + CGFloat(x * k) * s, y: cy - CGFloat(up * k) * s), depth)
    }

    /// A point of the graph on the view; nil outside the cube.
    func project(_ x: Double, _ y: Double, _ z: Double) -> (CGPoint?, Double) {
        guard x.isFinite, y.isFinite, z.isFinite else { return (nil, .nan) }
        let uu = u(x), vv = v(y), ww = w(z)
        guard abs(uu) <= 1.0001, abs(vv) <= 1.0001, abs(ww) <= 1.0001 else { return (nil, .nan) }
        let r = unit(uu, vv, ww)
        return (r.0, r.1)
    }

    /// A patch of a surface (in cube units) cut off at the cube's top and bottom (Sutherland–Hodgman, twice)
    static func clip(_ poly: [(Double, Double, Double)]) -> [(Double, Double, Double)] {
        func cut(_ pts: [(Double, Double, Double)], keep: (Double) -> Bool, at level: Double) -> [(Double, Double, Double)] {
            guard !pts.isEmpty else { return [] }
            var out: [(Double, Double, Double)] = []
            for i in pts.indices {
                let a = pts[i], b = pts[(i + 1) % pts.count]
                let ina = keep(a.2), inb = keep(b.2)
                if ina { out.append(a) }
                if ina != inb {
                    let t = (level - a.2) / (b.2 - a.2)
                    out.append((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, level))
                }
            }
            return out
        }
        if poly.allSatisfy({ abs($0.2) <= 1 }) { return poly }
        return cut(cut(poly, keep: { $0 <= 1 }, at: 1), keep: { $0 >= -1 }, at: -1)
    }

    static func cross(_ a: (Double, Double, Double), _ b: (Double, Double, Double)) -> (Double, Double, Double) {
        (a.1 * b.2 - a.2 * b.1, a.2 * b.0 - a.0 * b.2, a.0 * b.1 - a.1 * b.0)
    }

    static func normalize(_ a: (Double, Double, Double)) -> (Double, Double, Double) {
        let n = (a.0 * a.0 + a.1 * a.1 + a.2 * a.2).squareRoot()
        return n > 0 ? (a.0 / n, a.1 / n, a.2 / n) : (0, 0, 1)
    }
}

/// a critical point, in graph units, picked out on the plot
private struct PlotSpot: Equatable {
    let x: Double
    let y: Double
}

private struct PlotKey: Equatable {
    let state: GraphState
    let prefs: GraphPrefs
    let hover: CGPoint?
    let spot: PlotSpot?
    let scale: CGFloat
}

/// The plot's drawing surface: an NSView that draws through `draw` into a bitmap of its own and hands that to its
/// layer as contents, and lets every event through to the SwiftUI gestures above it. Drawing into the layer's
/// backing store instead (or a SwiftUI Canvas) costs 110-145 MB of GPU surfaces for a few seconds each time the
/// panel opens, and ~9 MB held after; a bitmap is one ~2 MB image.
private struct PlotCanvas: NSViewRepresentable {
    /// everything the drawing reads besides the appearance: the panel redraws on any change to the bar's model,
    /// and drawing the plot costs ~10 ms, so it's drawn again only when this changes
    let key: PlotKey
    let draw: (CGContext) -> Void

    func makeNSView(context: Context) -> PlotNSView { PlotNSView() }

    func updateNSView(_ view: PlotNSView, context: Context) {
        view.drawing = draw
        guard view.key != key else { return }
        view.key = key
        view.needsDisplay = true
    }

    final class PlotNSView: NSView {
        var drawing: ((CGContext) -> Void)?
        var key: PlotKey?

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
            // the screen's own pixel format and colour space (BGRA, premultiplied), so Core Animation shows the
            // image as it is instead of keeping a converted copy beside it
            guard let drawing, w > 0, h > 0,
                  let space = window?.colorSpace?.cgColorSpace ?? CGColorSpace(name: CGColorSpace.sRGB),
                  let cg = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
            else { return nil }
            cg.translateBy(x: 0, y: CGFloat(h))
            cg.scaleBy(x: scale, y: -scale)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: true)
            // reverse contrast (the graph settings) draws in the other appearance
            var look = effectiveAppearance
            if key?.prefs.reverseContrast == true {
                let dark = look.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                look = NSAppearance(named: dark ? .aqua : .darkAqua) ?? look
            }
            look.performAsCurrentDrawingAppearance { drawing(cg) }
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
