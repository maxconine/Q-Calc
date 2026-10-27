import Foundation
import QCalcCore

/// The graph panel's state, as GraphPanel.tsx keeps it: an x window (opening on `graphHome`, moved by dragging and
/// zooming), and the engine's GraphResult for it, whose y scale, ticks, roots and critical points the panel draws.
struct GraphState: Equatable {
    var expression: String
    var xMin: Double = -10
    var xMax: Double = 10
    var yMin: Double = -10
    var yMax: Double = 10
    var homeLo: Double?
    var homeHi: Double?
    var label = ""
    var curves: [GraphCurve] = []
    var xTicks: [GraphTick] = []
    var yTicks: [GraphTick] = []
    var angleUnit: String?
    var error: String?
    var failed = false
    var roots: [Double] = []
    var critical: [CriticalPoint] = []
    var contours: [ContourLevel] = []

    var input: String { "graph \(expression)" }
    var atHome: Bool { homeLo == nil || (xMin == homeLo && xMax == homeHi) }
}

/// Points of one curve.
struct GraphCurve: Equatable {
    var label: String
    var points: [GraphPoint]
}

extension BarModel {
    /// PLOT_ASPECT: the plot's height over its width in the 652 × 200 view (padding 12/12/28/40)
    static let plotAspect = (200.0 - 12 - 28) / (652.0 - 40 - 12)

    func showGraph(_ body: String) {
        if graph?.expression == body { return sampleGraph() }
        graph = GraphState(expression: body)
        sampleGraph(home: true)
    }

    /// a drag: the x window moves with the pointer (the y scale follows the curve)
    func panGraph(dx: Double, dy: Double) {
        guard var g = graph else { return }
        let w = g.xMax - g.xMin
        g.xMin -= dx * w
        g.xMax -= dx * w
        graph = g
        sampleGraph()
    }

    /// zoomAt: the x window about its centre (or `at`, a fraction across), kept within 1e-6 … 1e6 wide
    func zoomGraph(_ factor: Double, at: Double = 0.5) {
        guard var g = graph, factor.isFinite, factor > 0 else { return }
        let w = (g.xMax - g.xMin) * factor
        guard w >= 1e-6, w <= 1e6 else { return }
        let pivot = g.xMin + (g.xMax - g.xMin) * at
        g.xMin = pivot - w * at
        g.xMax = g.xMin + w
        graph = g
        sampleGraph()
    }

    func homeGraph() {
        guard var g = graph, let lo = g.homeLo, let hi = g.homeHi else { return }
        g.xMin = lo
        g.xMax = hi
        graph = g
        sampleGraph()
    }

    /// y on the curve under the pointer, as the readout shows it (graphReadoutY)
    func graphReadout(_ x: Double) -> Double? {
        guard let g = graph else { return nil }
        let options = GraphOptions(domainLo: g.xMin, domainHi: g.xMax, sampleCount: nil, findRoots: false, aspect: Self.plotAspect)
        return core.graphReadoutY(input: g.input, x: x, options: options, settings: graphSettings)
    }

    private var graphSettings: EvalOptions {
        let context = core.contextFromHistory(rows: rows, view: HistoryView.from(settings))
        return mergedOptions(options: options, context: context)
    }

    /// buildGraph on the engine's long-work pool (graphAsync, cancelled by the next pan, zoom or keystroke)
    func sampleGraph(home: Bool = false) {
        guard let g = graph else { return }
        let input = g.input, expr = g.expression
        let settings = graphSettings, engine = core
        graphTask?.cancel()
        graphTask = Task.detached(priority: .userInitiated) { [weak self] in
            var lo = g.xMin, hi = g.xMax
            if home {
                let w = engine.graphHome(input: input, options: GraphOptions(aspect: Self.plotAspect), settings: settings)
                if w.hi > w.lo { (lo, hi) = (w.lo, w.hi) }
            }
            let options = GraphOptions(domainLo: lo, domainHi: hi, sampleCount: nil, findRoots: true, aspect: Self.plotAspect)
            let result = try? await engine.graphAsync(input: input, options: options, settings: settings)
            if Task.isCancelled { return }
            var unit: AngleUnit?
            var xTicks: [GraphTick] = [], yTicks: [GraphTick] = []
            if let result {
                unit = result.angleUnit
                xTicks = engine.graphTicks(lo: lo, hi: hi, unit: unit, target: 6)
                yTicks = engine.graphTicks(lo: result.yMin, hi: result.yMax, unit: nil, target: 5)
            }
            await MainActor.run {
                guard let self, var now = self.graph, now.expression == expr else { return }
                if home { (now.homeLo, now.homeHi) = (lo, hi) }
                now.xMin = lo
                now.xMax = hi
                guard let result else {
                    now.failed = true
                    now.curves = []
                    self.graph = now
                    return
                }
                now.failed = false
                now.error = result.error
                now.label = result.label
                now.curves = [GraphCurve(label: result.label, points: result.points)]
                if result.yMax > result.yMin { (now.yMin, now.yMax) = (result.yMin, result.yMax) }
                now.roots = result.roots
                now.critical = result.criticalPoints
                now.contours = result.contours
                now.angleUnit = unit.map { $0 == .rad ? "rad" : "deg" }
                now.xTicks = xTicks
                now.yTicks = yTicks
                self.graph = now
            }
        }
    }
}
