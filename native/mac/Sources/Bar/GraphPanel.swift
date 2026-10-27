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
    private static let plotBg = Color(nsColor: NSColor(name: nil) { a in
        // color-mix(in srgb, var(--bg) 70%, var(--bg-tape))
        a.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: (0x2c * 0.7 + 0x25 * 0.3) / 255, green: (0x2c * 0.7 + 0x25 * 0.3) / 255, blue: (0x2e * 0.7 + 0x27 * 0.3) / 255, alpha: 1)
            : NSColor(srgbRed: (255 * 0.7 + 0xf5 * 0.3) / 255, green: (255 * 0.7 + 0xf5 * 0.3) / 255, blue: (255 * 0.7 + 0xf7 * 0.3) / 255, alpha: 1)
    })

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
        if state.critical.isEmpty {
            // .graph-detail-empty: the page says there are none rather than leaving the label out
            VStack(alignment: .leading, spacing: 6) {
                Text("Critical points").font(.system(size: Theme.caption, weight: .semibold)).foregroundStyle(Color.qMuted)
                    .padding(.top, 12)
                Text("None in this domain").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
            }
            .padding(.top, 8)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                Text("Critical points").font(.system(size: Theme.caption, weight: .semibold)).foregroundStyle(Color.qMuted)
                    .padding(.top, 12)
                // .graph-detail-list: flex-wrap, so a narrow bar puts the chips on more rows (an HStack squeezed
                // each into a column a character wide)
                WrapRow(spacing: 4, leading: true) {
                    ForEach(Array(state.critical.enumerated()), id: \.offset) { _, c in
                        // .graph-detail-item: a button, "Copy x" (selectX → copyValue), the point at 8 figures
                        // (formatCoord), as the page has it; they were static and grouped to 6
                        CriticalChip(kind: kindName(c.kind), coords: "(\(coord(c.x)), \(coord(c.y)))") {
                            model.copyAnswer(model.logic.string("engine/format", "formatNumber", [c.x, 8]) ?? fmt(c.x))
                        }
                    }
                }
            }
            .padding(.top, 8)
        }
    }

    private var plot: some View {
        GeometryReader { geo in
            let scale = geo.size.width / Self.svgW
            Canvas { ctx, size in
                ctx.scaleBy(x: scale, y: scale)
                draw(&ctx)
            }
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

    private func draw(_ ctx: inout GraphicsContext) {
        let g = state, p = Self.pad
        let plotW = Self.svgW - p.left - p.right, plotH = Self.svgH - p.top - p.bottom
        let bottom = p.top + plotH, right = p.left + plotW
        func px(_ x: Double) -> CGFloat { p.left + CGFloat((x - g.xMin) / (g.xMax - g.xMin)) * plotW }
        func py(_ y: Double) -> CGFloat { p.top + CGFloat((g.yMax - y) / (g.yMax - g.yMin)) * plotH }
        ctx.fill(Path(roundedRect: CGRect(x: p.left, y: p.top, width: plotW, height: plotH), cornerRadius: 4), with: .color(Self.plotBg))
        let grid = Color.qInk.opacity(0.05), axis = Color.qInk.opacity(0.18)
        // the engine's graphTicks: x angle-aware (π/2 steps for a radian curve), y plain
        for t in g.xTicks where t.value >= g.xMin && t.value <= g.xMax {
            var l = Path(); l.move(to: CGPoint(x: px(t.value), y: p.top)); l.addLine(to: CGPoint(x: px(t.value), y: bottom))
            ctx.stroke(l, with: .color(grid), lineWidth: 1)
            ctx.draw(Text(Self.tickText(t.label)).font(.system(size: 10).monospacedDigit()).foregroundColor(.qMuted),
                     at: CGPoint(x: px(t.value), y: Self.svgH - 12), anchor: .center)
        }
        for t in g.yTicks where t.value >= g.yMin && t.value <= g.yMax {
            var l = Path(); l.move(to: CGPoint(x: p.left, y: py(t.value))); l.addLine(to: CGPoint(x: right, y: py(t.value)))
            ctx.stroke(l, with: .color(grid), lineWidth: 1)
            ctx.draw(Text(Self.tickText(t.label)).font(.system(size: 10).monospacedDigit()).foregroundColor(.qMuted),
                     at: CGPoint(x: p.left - 6, y: py(t.value)), anchor: .trailing)
        }
        if py(0) >= p.top, py(0) <= bottom {
            var a = Path(); a.move(to: CGPoint(x: p.left, y: py(0))); a.addLine(to: CGPoint(x: right, y: py(0)))
            ctx.stroke(a, with: .color(axis), lineWidth: 1)
        }
        if px(0) >= p.left, px(0) <= right {
            var a = Path(); a.move(to: CGPoint(x: px(0), y: p.top)); a.addLine(to: CGPoint(x: px(0), y: bottom))
            ctx.stroke(a, with: .color(axis), lineWidth: 1)
        }
        var clip = ctx
        clip.clip(to: Path(CGRect(x: p.left, y: p.top, width: plotW, height: plotH)))
        for curve in g.curves {
            var path = Path()
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
            clip.stroke(path, with: .color(.qAccent), style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
        }
        if let hx = hoverX, hx >= Double(p.left), hx <= Double(right) {
            let x = g.xMin + (hx - Double(p.left)) / Double(plotW) * (g.xMax - g.xMin)
            var line = Path()
            line.move(to: CGPoint(x: hx, y: p.top)); line.addLine(to: CGPoint(x: hx, y: bottom))
            ctx.stroke(line, with: .color(Color.qInk.opacity(0.22)), lineWidth: 1)
            if let y = curveY(at: x), py(y) >= p.top, py(y) <= bottom {
                let dot = Path(ellipseIn: CGRect(x: hx - 3.5, y: py(y) - 3.5, width: 7, height: 7))
                ctx.fill(dot, with: .color(.qAccent))
                ctx.stroke(dot, with: .color(Self.plotBg), lineWidth: 1.5)
                let label = "(\(fmt4(x)), \(fmt4(y)))"
                let onRight = hx > Double(p.left) + Double(plotW) * 0.62
                ctx.draw(Text(label).font(.system(size: 11).monospacedDigit()).foregroundColor(.qInk),
                         at: CGPoint(x: hx + (onRight ? -8 : 8), y: p.top + 13), anchor: onRight ? .trailing : .leading)
            }
        }
        // .graph-mark: roots and critical points, rings of the plot colour with an accent edge
        let marks = g.roots.map { CGPoint(x: px($0), y: py(0)) } + g.critical.map { CGPoint(x: px($0.x), y: py($0.y)) }
        for m in marks where m.x >= p.left && m.x <= right && m.y >= p.top && m.y <= bottom {
            let ring = Path(ellipseIn: CGRect(x: m.x - 2.75, y: m.y - 2.75, width: 5.5, height: 5.5))
            clip.fill(ring, with: .color(Self.plotBg))
            clip.stroke(ring, with: .color(.qAccent), lineWidth: 1.5)
        }
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
    let action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Text(kind).font(.system(size: Theme.caption, weight: .medium)).foregroundStyle(hover ? Color.qAccent : Color.qMuted)
                Text(coords).font(.system(size: Theme.small).monospacedDigit()).foregroundStyle(hover ? Color.qAccent : Color.qInk)
            }
            .padding(.vertical, 3).padding(.horizontal, 8)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS)
                .fill(hover ? Color.qAccent.opacity(0.12) : Color.qInk.opacity(0.06)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .help("Copy x")
        .accessibilityLabel("\(kind) \(coords)")
        .accessibilityHint("Copies x")
    }
}
