import AppKit
import QCalcCore
import SwiftUI

// The graph under the bar, laid out as GraphPanel.tsx draws its SVG (.graph in src/index.css): a head with the
// label, the domain and "reset", then a 652 × 200 plot (padding 12/12/28/40) on the plot colour, grid lines at
// ink 5%, axes at ink 18%, 10 pt muted ticks and a 2 pt accent curve. Drag pans, pinch or scroll zooms, and the
// engine re-samples (Engine.graphAsync, until WP-GRAPH the evaluate_json sampler in EngineService.swift).
struct GraphPanel: View {
    @ObservedObject var model: BarModel
    let state: GraphState
    @State private var dragStart: CGPoint?
    @State private var lastZoom: CGFloat = 1
    @State private var resetHover = false

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
            if state.failed {
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

    private var label: String { state.curves.first?.label ?? Pretty.radicals(state.expression) }
    private var atHome: Bool {
        guard state.xMin == -10, state.xMax == 10 else { return false }
        guard let home = state.homeY else { return true }
        return state.yMin == home.0 && state.yMax == home.1
    }

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
        let xs = ticks(g.xMin, g.xMax, count: 8), ys = ticks(g.yMin, g.yMax, count: 5)
        for x in xs {
            var l = Path(); l.move(to: CGPoint(x: px(x), y: p.top)); l.addLine(to: CGPoint(x: px(x), y: bottom))
            ctx.stroke(l, with: .color(grid), lineWidth: 1)
            ctx.draw(Text(fmt(x)).font(.system(size: 10).monospacedDigit()).foregroundColor(.qMuted), at: CGPoint(x: px(x), y: Self.svgH - 12), anchor: .center)
        }
        for y in ys {
            var l = Path(); l.move(to: CGPoint(x: p.left, y: py(y))); l.addLine(to: CGPoint(x: right, y: py(y)))
            ctx.stroke(l, with: .color(grid), lineWidth: 1)
            ctx.draw(Text(fmt(y)).font(.system(size: 10).monospacedDigit()).foregroundColor(.qMuted), at: CGPoint(x: p.left - 6, y: py(y)), anchor: .trailing)
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
