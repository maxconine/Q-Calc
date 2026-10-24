import AppKit
import SwiftUI

// The graph under the bar, drawn natively from points the engine sampled. Placeholder for GraphPanel.tsx until the
// GRAPH package's engine.graph lands (GraphResult: curves, implicit plots, roots and critical points, graphTicks,
// graphHome); today FoundationEngine.graph samples y = f(x) through the evaluate_json escape hatch.
struct GraphPanel: View {
    @ObservedObject var model: BarModel
    let state: GraphState
    @State private var dragStart: CGPoint?
    @State private var lastZoom: CGFloat = 1

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Canvas { ctx, size in draw(&ctx, size) }
                .frame(height: 240)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 2)
                    .onChanged { g in
                        let prev = dragStart ?? g.startLocation
                        dragStart = g.location
                        model.panGraph(dx: (g.location.x - prev.x) / 640, dy: (g.location.y - prev.y) / 240)
                    }
                    .onEnded { _ in dragStart = nil })
                .gesture(MagnificationGesture()
                    .onChanged { s in
                        model.zoomGraph(Double(lastZoom / s))
                        lastZoom = s
                    }
                    .onEnded { _ in lastZoom = 1 })
                .onTapGesture(count: 2) { model.homeGraph() }
                .accessibilityElement()
                .accessibilityLabel(accessibilitySummary)
            HStack {
                Text(state.failed ? "can’t graph \(state.expression) yet" : "y = \(Pretty.radicals(state.expression))")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                Spacer()
                Button { model.zoomGraph(0.5) } label: { Image(systemName: "plus.magnifyingglass") }
                    .help("Zoom in").accessibilityLabel("Zoom in")
                Button { model.zoomGraph(2) } label: { Image(systemName: "minus.magnifyingglass") }
                    .help("Zoom out").accessibilityLabel("Zoom out")
                Button { model.homeGraph() } label: { Image(systemName: "house") }
                    .help("Back to the usual view").accessibilityLabel("Home view")
            }
            .buttonStyle(.borderless)
            .font(.system(size: 12))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
    }

    private var accessibilitySummary: String {
        let ys = state.curves.flatMap { $0.points.compactMap(\.y) }
        guard let lo = ys.min(), let hi = ys.max() else { return "Graph of \(state.expression), nothing to draw" }
        return "Graph of \(state.expression) from x \(fmt(state.xMin)) to \(fmt(state.xMax)), y from \(fmt(lo)) to \(fmt(hi))"
    }

    private func fmt(_ v: Double) -> String { Pretty.answer(String(format: "%g", v)) }

    private func draw(_ ctx: inout GraphicsContext, _ size: CGSize) {
        let g = state
        func px(_ x: Double) -> CGFloat { CGFloat((x - g.xMin) / (g.xMax - g.xMin)) * size.width }
        func py(_ y: Double) -> CGFloat { size.height - CGFloat((y - g.yMin) / (g.yMax - g.yMin)) * size.height }
        let grid = Color.primary.opacity(0.08), axis = Color.primary.opacity(0.35)
        // ticks every 1, 2 or 5 × 10^n, about eight across
        let step = niceStep((g.xMax - g.xMin) / 8)
        var x = (g.xMin / step).rounded(.up) * step
        while x <= g.xMax {
            var p = Path(); p.move(to: CGPoint(x: px(x), y: 0)); p.addLine(to: CGPoint(x: px(x), y: size.height))
            ctx.stroke(p, with: .color(grid), lineWidth: 1)
            if abs(x) > step / 2 {
                ctx.draw(Text(fmt(x)).font(.system(size: 9)).foregroundColor(.secondary), at: CGPoint(x: px(x), y: min(max(py(0) + 8, 8), size.height - 8)))
            }
            x += step
        }
        let ystep = niceStep((g.yMax - g.yMin) / 6)
        var y = (g.yMin / ystep).rounded(.up) * ystep
        while y <= g.yMax {
            var p = Path(); p.move(to: CGPoint(x: 0, y: py(y))); p.addLine(to: CGPoint(x: size.width, y: py(y)))
            ctx.stroke(p, with: .color(grid), lineWidth: 1)
            if abs(y) > ystep / 2 {
                ctx.draw(Text(fmt(y)).font(.system(size: 9)).foregroundColor(.secondary), at: CGPoint(x: min(max(px(0) + 14, 14), size.width - 14), y: py(y)))
            }
            y += ystep
        }
        var axes = Path()
        axes.move(to: CGPoint(x: 0, y: py(0))); axes.addLine(to: CGPoint(x: size.width, y: py(0)))
        axes.move(to: CGPoint(x: px(0), y: 0)); axes.addLine(to: CGPoint(x: px(0), y: size.height))
        ctx.stroke(axes, with: .color(axis), lineWidth: 1)
        for curve in g.curves {
            var path = Path()
            var pen = false
            var lastY: Double?
            for pt in curve.points {
                guard let v = pt.y else { pen = false; continue }
                // a jump across most of the view is an asymptote, not a line to draw
                if let ly = lastY, abs(v - ly) > (g.yMax - g.yMin) * 4 { pen = false }
                let p = CGPoint(x: px(pt.x), y: py(v))
                if pen { path.addLine(to: p) } else { path.move(to: p) }
                pen = true
                lastY = v
            }
            ctx.stroke(path, with: .color(.accentColor), style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
        }
    }

    private func niceStep(_ raw: Double) -> Double {
        guard raw > 0, raw.isFinite else { return 1 }
        let mag = pow(10, floor(log10(raw)))
        let f = raw / mag
        return (f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10) * mag
    }
}
