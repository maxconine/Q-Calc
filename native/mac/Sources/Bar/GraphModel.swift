import AppKit
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
    /// native: the y step that makes the grid's cells square (nil when y isn't fitted, or is set or log)
    var squareYStep: Double?
    var yTicks: [GraphTick] = []
    var angleUnit: String?
    var error: String?
    var failed = false
    var roots: [Double] = []
    var critical: [CriticalPoint] = []
    var contours: [ContourLevel] = []
    /// What was graphed: a curve in x, an implicit curve (`y^2 = 4x`) or a surface (`z = x^2 - y^2`).
    var kind: GraphKind = .curve
    /// Native only: the y a plot of x and y centres on (an implicit curve or a contour plot pans in y too).
    var yCenter: Double?
    var homeYCenter: Double?
    /// A surface drawn in 3D (the default) rather than as level curves; nil until the engine says it's a surface.
    var view3D: Bool?
    /// The 3D view's turn about the vertical and its tilt, in radians.
    var yaw = GraphState.homeYaw
    var pitch = GraphState.homePitch
    var surface: SurfaceGrid?

    // native only: what the settings panel sets for this graph, and the equations graphed with it

    /// More equations drawn over the first, each in its own colour: those after a `;` in the bar, then those added
    /// in the panel.
    var layers: [GraphLayer] = []
    /// The 2D plot's y window when set (in the settings, by "Zoom Square" or by panning or zooming one that is);
    /// nil fits y to the curve, as the page does.
    var fixedY: ClosedRange<Double>?
    /// The 3D cube's z when set; nil is a cube, z as wide as x about 0 (as Desmos 3D has it).
    var fixedZ: ClosedRange<Double>?
    var xLabel = ""
    var yLabel = ""
    /// Grid steps set in the settings; nil chooses them.
    var xStep: Double?
    var yStep: Double?
    var xLog = false
    var yLog = false

    static let homeYaw = -0.62
    static let homePitch = 0.52

    var input: String { "graph \(expression)" }
    var shows3D: Bool { kind == .surface && view3D == true }
    var atHome: Bool {
        (homeLo == nil || (xMin == homeLo && xMax == homeHi)) && yCenter == homeYCenter && fixedY == nil && fixedZ == nil
            && (!shows3D || (yaw == Self.homeYaw && pitch == Self.homePitch))
    }

    /// The 3D cube's z: `fixedZ`, or as wide as x about 0. A surface under a tenth that tall (`sin(x)cos(y)` in
    /// degrees, over ±360) would be a flat sheet in that, so its own heights are fitted instead.
    var zWindow: ClosedRange<Double> {
        if let z = fixedZ { return z }
        let half = (xMax - xMin) / 2
        if let s = surface, s.zHi > s.zLo, s.zHi - s.zLo < half * 0.2 {
            let lo = min(s.zLo, 0), hi = max(s.zHi, 0), pad = (hi - lo) * 0.15
            return (lo - pad)...(hi + pad)
        }
        return -half...half
    }

    /// The x axis is drawn on a log scale (a window above 0).
    var logX: Bool { xLog && xMin > 0 && xMax > xMin }
    var logY: Bool { yLog && yMin > 0 && yMax > yMin }
}

enum GraphKind: Equatable {
    case curve, implicit, surface
}

/// Points of one curve.
struct GraphCurve: Equatable {
    var label: String
    var points: [GraphPoint]
}

/// Native only: one more equation on the graph, sampled over the same window as the first.
struct GraphLayer: Equatable, Identifiable {
    let id: Int
    var expression: String
    /// After a `;` in the bar (edited there), rather than added in the panel.
    var fromBar: Bool
    var points: [GraphPoint] = []
    var contours: [ContourLevel] = []
    var kind: GraphKind = .curve
    var surface: SurfaceGrid?
    var critical: [CriticalPoint] = []
    var roots: [Double] = []
    var error: String?

    var input: String { "graph \(expression)" }
    var isBlank: Bool { expression.trimmingCharacters(in: .whitespaces).isEmpty }
}

/// Native only: the graph settings' display choices (Desmos's graph settings), kept between launches.
struct GraphPrefs: Codable, Equatable {
    var largeText = false
    var reverseContrast = false
    var grid = true
    var polar = false
    var arrows = false
    var axisNumbers = true
    var minorGrid = true
    var showXAxis = true
    var showYAxis = true
    var lockViewport = false
    /// Degrees or radians for the graph's trig; nil follows the calculator's setting.
    var degrees: Bool?

    private static let key = "graphPrefs"

    static func load() -> GraphPrefs {
        guard let data = UserDefaults.standard.data(forKey: key),
              let prefs = try? JSONDecoder().decode(GraphPrefs.self, from: data) else { return GraphPrefs() }
        return prefs
    }

    func save() {
        if let data = try? JSONEncoder().encode(self) { UserDefaults.standard.set(data, forKey: Self.key) }
    }
}

extension BarModel {
    /// PLOT_ASPECT: the plot's height over its width in the 652 × 200 view (padding 12/12/28/40)
    static let plotAspect = (200.0 - 12 - 28) / (652.0 - 40 - 12)

    /// the least 1, 2 or 5 × 10ⁿ at or above v
    static func stepAtLeast(_ v: Double) -> Double {
        guard v > 0, v.isFinite else { return 1 }
        let p = pow(10, floor(log10(v)))
        for m in [1.0, 2, 5, 10] where m * p >= v * (1 - 1e-12) { return m * p }
        return 10 * p
    }

    /// `graph a; b; c`: the first equation and the rest, split at the `;`s outside brackets.
    static func splitGraphs(_ body: String) -> [String] {
        var parts: [String] = [], depth = 0, cur = ""
        for ch in body {
            if "([{".contains(ch) { depth += 1 } else if ")]}".contains(ch) { depth = max(0, depth - 1) }
            if ch == ";", depth == 0 {
                parts.append(cur)
                cur = ""
            } else {
                cur.append(ch)
            }
        }
        parts.append(cur)
        let trimmed = parts.map { $0.trimmingCharacters(in: .whitespaces) }
        let first = trimmed.first ?? body
        return [first] + trimmed.dropFirst().filter { !$0.isEmpty }
    }

    func showGraph(_ body: String) {
        let parts = Self.splitGraphs(body)
        let primary = parts[0], barExtras = Array(parts.dropFirst())
        guard var g = graph else {
            var fresh = GraphState(expression: primary)
            fresh.layers = barExtras.enumerated().map { GraphLayer(id: $0.offset + 1, expression: $0.element, fromBar: true) }
            graph = fresh
            sampleGraph(home: true)
            return
        }
        let oldBar = g.layers.filter(\.fromBar).map(\.expression)
        if g.expression == primary, oldBar == barExtras { return sampleGraph() }
        let samePrimary = g.expression == primary
        var nextID = (g.layers.map(\.id).max() ?? 0) + 1
        var bar: [GraphLayer] = []
        for (i, e) in barExtras.enumerated() {
            // a layer typed on keeps its id (and its result until the new one comes)
            let old = g.layers.filter(\.fromBar)
            if i < old.count {
                var l = old[i]
                l.expression = e
                bar.append(l)
            } else {
                bar.append(GraphLayer(id: nextID, expression: e, fromBar: true))
                nextID += 1
            }
        }
        let added = g.layers.filter { !$0.fromBar }
        if !samePrimary {
            // a new first equation: a new window, but the panel's equations and axis settings stay
            var fresh = GraphState(expression: primary)
            (fresh.xLabel, fresh.yLabel, fresh.xStep, fresh.yStep, fresh.xLog, fresh.yLog) =
                (g.xLabel, g.yLabel, g.xStep, g.yStep, g.xLog, g.yLog)
            g = fresh
        }
        g.layers = bar + added
        graph = g
        sampleGraph(home: !samePrimary)
    }

    // MARK: equations added in the panel

    func addGraphLayer() {
        guard var g = graph else { return }
        let id = (g.layers.map(\.id).max() ?? 0) + 1
        g.layers.append(GraphLayer(id: id, expression: "", fromBar: false))
        graph = g
    }

    func setGraphLayer(_ id: Int, expression: String) {
        guard var g = graph, let i = g.layers.firstIndex(where: { $0.id == id }), g.layers[i].expression != expression else { return }
        g.layers[i].expression = expression
        if g.layers[i].isBlank {
            g.layers[i] = GraphLayer(id: id, expression: expression, fromBar: g.layers[i].fromBar)
        }
        graph = g
        sampleGraph()
    }

    func removeGraphLayer(_ id: Int) {
        guard var g = graph else { return }
        g.layers.removeAll { $0.id == id }
        graph = g
        sampleGraph()
    }

    // MARK: the settings panel's window and axes

    func setGraphX(_ lo: Double, _ hi: Double) {
        guard var g = graph, lo.isFinite, hi.isFinite, hi > lo, !(g.xLog && lo <= 0) else { return }
        (g.xMin, g.xMax) = (lo, hi)
        graph = g
        sampleGraph()
    }

    func setGraphY(_ lo: Double, _ hi: Double) {
        guard var g = graph, lo.isFinite, hi.isFinite, hi > lo, !(g.yLog && lo <= 0) else { return }
        g.fixedY = lo...hi
        graph = g
        sampleGraph()
    }

    func setGraphZ(_ lo: Double, _ hi: Double) {
        guard var g = graph, lo.isFinite, hi.isFinite, hi > lo else { return }
        g.fixedZ = lo...hi
        graph = g
    }

    /// Zoom Square: y as many units a point as x, about the middle of the y shown
    func zoomGraphSquare() {
        guard var g = graph, !g.logX, !g.logY else { return }
        if g.shows3D {
            g.fixedZ = nil
            graph = g
            return
        }
        let mid = (g.yMin + g.yMax) / 2, half = (g.xMax - g.xMin) * Self.plotAspect / 2
        g.fixedY = (mid - half)...(mid + half)
        graph = g
        sampleGraph()
    }

    func setGraphAxis(xLabel: String? = nil, yLabel: String? = nil, xStep: Double?? = nil, yStep: Double?? = nil) {
        guard var g = graph else { return }
        if let xLabel { g.xLabel = xLabel }
        if let yLabel { g.yLabel = yLabel }
        if let xStep { g.xStep = xStep.flatMap { $0 > 0 && $0.isFinite ? $0 : nil } }
        if let yStep { g.yStep = yStep.flatMap { $0 > 0 && $0.isFinite ? $0 : nil } }
        graph = g
    }

    /// A log x or y axis; a window reaching 0 or below moves above it (three decades under its top).
    func setGraphLog(x: Bool? = nil, y: Bool? = nil) {
        guard var g = graph else { return }
        if let x {
            g.xLog = x
            if x, g.xMin <= 0 { (g.xMin, g.xMax) = g.xMax > 0 ? (g.xMax / 1000, g.xMax) : (0.1, 100) }
        }
        if let y {
            g.yLog = y
            if y {
                let top = max(g.fixedY?.upperBound ?? g.yMax, 0)
                if (g.fixedY?.lowerBound ?? g.yMin) <= 0 { g.fixedY = top > 0 ? (top / 1000)...top : 0.1...100 }
            }
        }
        graph = g
        sampleGraph()
    }

    // MARK: pan, zoom, orbit

    /// `lo…hi` moved by `by` of its width (a log axis by that much of its decades)
    /// A fitted y window held as it is, so a drag or zoom moves it rather than fitting it again
    private static func holdY(_ g: inout GraphState) {
        if g.fixedY == nil, g.yMax > g.yMin { g.fixedY = g.yMin...g.yMax }
    }

    private static func shifted(_ lo: Double, _ hi: Double, by: Double, log: Bool) -> (Double, Double) {
        if log, lo > 0, hi > lo {
            let a = log10(lo), b = log10(hi), d = by * (b - a)
            return (pow(10, a + d), pow(10, b + d))
        }
        let d = by * (hi - lo)
        return (lo + d, hi + d)
    }

    /// `lo…hi` scaled by `factor` about `at` of the way across
    private static func scaled(_ lo: Double, _ hi: Double, by factor: Double, at: Double, log: Bool) -> (Double, Double) {
        if log, lo > 0, hi > lo {
            let a = log10(lo), b = log10(hi), w = (b - a) * factor, pivot = a + (b - a) * at
            return (pow(10, pivot - w * at), pow(10, pivot - w * at + w))
        }
        let w = (hi - lo) * factor, pivot = lo + (hi - lo) * at
        return (pivot - w * at, pivot - w * at + w)
    }

    /// a drag: the x window moves with the pointer (the y scale follows a curve; a plot of x and y, which keeps
    /// one scale for both, moves in y too). In 3D a drag turns the surface instead.
    func panGraph(dx: Double, dy: Double, move3D: Bool = false) {
        guard var g = graph else { return }
        if g.shows3D, !move3D {
            g.yaw += dx * .pi
            g.pitch = min(1.45, max(0.05, g.pitch - dy * 1.6))
            graph = g
            return
        }
        if graphPrefs.lockViewport { return }
        if g.shows3D {
            // the window slides under the pointer along the surface's floor: the drag back through the turn and tilt
            let c = cos(g.yaw), s = sin(g.yaw)
            let bx = -dx * 2, by = dy * 2 / max(0.3, sin(g.pitch))
            let du = bx * c - by * s, dv = -bx * s - by * c
            let w = g.xMax - g.xMin
            g.xMin += du * w / 2
            g.xMax += du * w / 2
            g.yCenter = (g.yCenter ?? (g.yMin + g.yMax) / 2) + dv * (g.yMax - g.yMin) / 2
            graph = g
            sampleGraph()
            return
        }
        (g.xMin, g.xMax) = Self.shifted(g.xMin, g.xMax, by: -dx, log: g.logX)
        // native, as Desmos: the view moves as one piece under the pointer. A fitted y window is held from the
        // first drag on (refitting it each move made the plot jump up and down), and shown at once, not after
        // the resample
        if g.kind == .curve { Self.holdY(&g) }
        if let fy = g.fixedY {
            let (lo, hi) = Self.shifted(fy.lowerBound, fy.upperBound, by: dy, log: g.logY)
            g.fixedY = lo...hi
            (g.yMin, g.yMax) = (lo, hi)
        } else if g.kind != .curve {
            g.yCenter = (g.yCenter ?? (g.yMin + g.yMax) / 2) + dy * (g.yMax - g.yMin)
        }
        graph = g
        sampleGraph()
    }

    /// A scroll over the 3D view, as Onshape takes one: two fingers up/down zoom (with shift, a two-finger drag
    /// pans), a mouse wheel zooms about the middle. Pinching zooms too. False (the scroll goes on) outside 3D.
    /// Two fingers (or a wheel) over the plot, 2D or 3D: up/down zooms (about `at`, the pointer's fraction across a
    /// 2D plot), shift pans. True when it was the graph's.
    func scrollGraph(_ event: NSEvent, size: CGSize, at: Double = 0.5) -> Bool {
        guard let g = graph, size.width > 0, size.height > 0 else { return false }
        if event.hasPreciseScrollingDeltas {
            // momentum after the fingers lift would keep it going; only the gesture itself counts
            guard event.momentumPhase.isEmpty else { return true }
            if event.modifierFlags.contains(.shift) {
                // shift + two-finger drag pans, following the fingers
                panGraph(dx: event.scrollingDeltaX / size.width, dy: event.scrollingDeltaY / size.height, move3D: g.shows3D)
            } else {
                // two fingers up/down zoom (as Onshape): fingers up zooms in, whatever the natural-scrolling setting
                let up = (event.isDirectionInvertedFromDevice ? -event.scrollingDeltaY : event.scrollingDeltaY) / size.height
                if up != 0 { zoomGraph(exp(-up * 2.5), at: at) }
            }
        } else {
            let lines = event.scrollingDeltaY
            guard lines != 0 else { return true }
            zoomGraph(exp(-lines * 0.08), at: at)
        }
        return true
    }

    /// a surface between its 3D view and its level curves
    func setGraph3D(_ on: Bool) {
        guard var g = graph, g.kind == .surface, g.view3D != on else { return }
        g.view3D = on
        graph = g
        sampleGraph()
    }

    /// zoomAt: the x window about its centre (or `at`, a fraction across), kept within 1e-6 … 1e6 wide; a y or z
    /// window that's set scales with it
    func zoomGraph(_ factor: Double, at: Double = 0.5) {
        guard var g = graph, factor.isFinite, factor > 0, !graphPrefs.lockViewport else { return }
        let (lo, hi) = Self.scaled(g.xMin, g.xMax, by: factor, at: at, log: g.logX)
        guard hi - lo >= 1e-6, hi - lo <= 1e6 || g.logX else { return }
        (g.xMin, g.xMax) = (lo, hi)
        if !g.shows3D, g.kind == .curve { Self.holdY(&g) }
        if let fy = g.fixedY {
            let (a, b) = Self.scaled(fy.lowerBound, fy.upperBound, by: factor, at: 0.5, log: g.logY)
            g.fixedY = a...b
            if !g.shows3D { (g.yMin, g.yMax) = (a, b) }
        }
        if let fz = g.fixedZ {
            let (a, b) = Self.scaled(fz.lowerBound, fz.upperBound, by: factor, at: 0.5, log: false)
            g.fixedZ = a...b
        }
        graph = g
        sampleGraph()
    }

    func homeGraph() {
        guard var g = graph, let lo = g.homeLo, let hi = g.homeHi else { return }
        g.xMin = lo
        g.xMax = hi
        g.yCenter = g.homeYCenter
        g.fixedY = nil
        g.fixedZ = nil
        g.yaw = GraphState.homeYaw
        g.pitch = GraphState.homePitch
        if g.xLog, lo <= 0 { (g.xMin, g.xMax) = hi > 0 ? (hi / 1000, hi) : (0.1, 100) }
        graph = g
        sampleGraph()
    }

    // MARK: readouts

    /// y on the curve under the pointer, as the readout shows it (graphReadoutY); `input` for another equation's
    func graphReadout(_ x: Double, input: String? = nil) -> Double? {
        guard let g = graph else { return nil }
        let options = GraphOptions(domainLo: g.xMin, domainHi: g.xMax, sampleCount: nil, findRoots: false, aspect: Self.plotAspect)
        return core.graphReadoutY(input: input ?? g.input, x: x, options: options, settings: graphSettings)
    }

    /// a surface's z at the pointer, for the contour view's readout (graphReadoutZ)
    func graphReadoutZ(_ x: Double, _ y: Double, input: String? = nil) -> Double? {
        guard let g = graph else { return nil }
        let options = GraphOptions(domainLo: g.xMin, domainHi: g.xMax, sampleCount: nil, findRoots: false, aspect: Self.plotAspect)
        return core.graphReadoutZ(input: input ?? g.input, x: x, y: y, options: options, settings: graphSettings)
    }

    private var graphSettings: EvalOptions {
        let context = core.contextFromHistory(rows: rows, view: HistoryView.from(settings))
        var merged = mergedOptions(options: options, context: context)
        if let deg = graphPrefs.degrees { merged.angleRadians = !deg }
        return merged
    }

    // MARK: sampling

    /// buildGraph on the engine's long-work pool (graphAsync, cancelled by the next pan, zoom or keystroke), the
    /// first equation and then each of the others over the same window
    func sampleGraph(home: Bool = false) {
        guard let g = graph else { return }
        let input = g.input, expr = g.expression
        let layers = g.layers.filter { !$0.isBlank }.map { ($0.id, $0.input) }
        let settings = graphSettings, engine = core
        graphTask?.cancel()
        graphTask = Task.detached(priority: .userInitiated) { [weak self] in
            var lo = g.xMin, hi = g.xMax
            var yCenter = g.yCenter
            if home {
                let w = engine.graphHome(input: input, options: GraphOptions(aspect: Self.plotAspect), settings: settings)
                if w.hi > w.lo { (lo, hi) = (w.lo, w.hi) }
                yCenter = w.yCenter
                // native, as Desmos: the window opens wide enough for every equation (`x² {3 < x < ∞}; x {x < 1}`)
                if !g.shows3D {
                    for (_, layerInput) in layers {
                        let lw = engine.graphHome(input: layerInput, options: GraphOptions(aspect: Self.plotAspect), settings: settings)
                        guard lw.hi > lw.lo, lw.lo < lo || lw.hi > hi else { continue }
                        (lo, hi) = (min(lo, lw.lo), max(hi, lw.hi))
                        yCenter = nil
                    }
                }
                if g.xLog, lo <= 0 { (lo, hi) = hi > 0 ? (hi / 1000, hi) : (0.1, 100) }
            }
            let in3D = g.shows3D
            /// the options for a y window: a plot of x and y samples exactly it. A curve gets 4001 samples, not the
            /// page's 401, which alias a zoomed-out sin x into a slow false wave; odd, so one lands on the window's
            /// middle (with an even count x² has two samples level across its turn, and the turn finder skips it)
            func options(y: ClosedRange<Double>?) -> GraphOptions {
                var aspect = in3D ? 1 : Self.plotAspect, center = yCenter
                if !in3D, let y, hi > lo {
                    aspect = (y.upperBound - y.lowerBound) / (hi - lo)
                    center = (y.lowerBound + y.upperBound) / 2
                }
                return GraphOptions(domainLo: lo, domainHi: hi, sampleCount: in3D ? nil : 4001, findRoots: true, aspect: aspect,
                                    yCenter: center, surface: in3D, xLog: g.xLog && lo > 0)
            }
            // the 3D view samples a square of x and y
            let first = options(y: in3D ? nil : g.fixedY)
            let result = try? await engine.graphAsync(input: input, options: first, settings: settings)
            if Task.isCancelled { return }
            var others: [Int: GraphResult] = [:]
            for (id, layerInput) in layers {
                if let r = try? await engine.graphAsync(input: layerInput, options: first, settings: settings) { others[id] = r }
                if Task.isCancelled { return }
            }
            // y: as set; or fitted to every curve when the first is one (a plot of x and y keeps its own)
            var yLo = result?.yMin ?? -10, yHi = result?.yMax ?? 10
            // native: about a dozen x ticks (Desmos's density), and with them square grid cells
            let xTicks = result == nil ? [] : engine.graphTicks(lo: lo, hi: hi, unit: result?.angleUnit, target: in3D ? 6 : 12)
            var squareStep: Double?
            if let fy = g.fixedY, !in3D {
                (yLo, yHi) = (fy.lowerBound, fy.upperBound)
            } else if let result, !result.contour, !in3D {
                let curves = [result.points] + others.values.filter { !$0.contour && $0.error == nil }.map(\.points)
                if g.yLog {
                    let ys = curves.flatMap { $0.compactMap(\.y) }.filter { $0 > 0 && $0.isFinite }
                    if let a = ys.min(), let b = ys.max() { (yLo, yHi) = a < b ? (a / 1.5, b * 1.5) : (a / 10, a * 10) } else { (yLo, yHi) = (0.1, 100) }
                } else {
                    for r in others.values where !r.contour && r.error == nil && r.yMax > r.yMin {
                        yLo = min(yLo, r.yMin)
                        yHi = max(yHi, r.yMax)
                    }
                    // native, as Desmos: square grid cells. y's step is the least 1, 2 or 5 × 10ⁿ whose rows, as
                    // tall as x's columns are wide, hold every curve; the window is centred on them
                    if !g.logX, g.yStep == nil, xTicks.count >= 2, hi > lo, yHi > yLo {
                        let sx = xTicks[1].value - xTicks[0].value
                        let rows = Self.plotAspect * (hi - lo) / sx
                        if sx > 0, rows.isFinite, rows > 0 {
                            let sy = Self.stepAtLeast((yHi - yLo) / rows)
                            let mid = (yLo + yHi) / 2
                            (yLo, yHi) = (mid - rows * sy / 2, mid + rows * sy / 2)
                            squareStep = sy
                        }
                    }
                }
                // the others' plots of x and y again, over this y
                for (id, r) in others where r.contour {
                    if let again = try? await engine.graphAsync(input: "graph " + (g.layers.first { $0.id == id }?.expression ?? ""),
                                                                options: options(y: yLo...yHi), settings: settings) {
                        others[id] = again
                    }
                    if Task.isCancelled { return }
                }
            }
            var unit: AngleUnit?
            var yTicks: [GraphTick] = []
            if let result {
                unit = result.angleUnit
                yTicks = engine.graphTicks(lo: yLo, hi: yHi, unit: nil, target: 5)
            }
            // a held window keeps its cells square while x's ticks move to 1, 2, 5 steps: y's step at the same scale,
            // when that is a round one
            if squareStep == nil, !in3D, let fy = g.fixedY, !g.logX, !g.logY, g.yStep == nil, xTicks.count >= 2, hi > lo {
                let sx = xTicks[1].value - xTicks[0].value
                let sy = sx * (fy.upperBound - fy.lowerBound) / (Self.plotAspect * (hi - lo))
                let p = pow(10, floor(log10(sy))), m = sy / p
                if sy > 0, sy.isFinite, [1.0, 2, 2.5, 5, 10].contains(where: { abs(m - $0) < 1e-6 * $0 }) { squareStep = sy }
            }
            let finalY = (yLo, yHi), sampled = others, square = squareStep
            await MainActor.run {
                guard let self, var now = self.graph, now.expression == expr else { return }
                // the window is the panel's: a result that comes in after the view has moved on (a drag) is drawn
                // in the current window, not put back to the one it was sampled for, which made the plot jump back
                if home {
                    (now.homeLo, now.homeHi, now.homeYCenter) = (lo, hi, yCenter)
                    now.xMin = lo
                    now.xMax = hi
                    now.yCenter = yCenter
                }
                for i in now.layers.indices {
                    let id = now.layers[i].id
                    guard layers.contains(where: { $0.0 == id }) else {
                        if now.layers[i].isBlank { now.layers[i] = GraphLayer(id: id, expression: now.layers[i].expression, fromBar: now.layers[i].fromBar) }
                        continue
                    }
                    guard let r = sampled[id] else {
                        now.layers[i].error = "can’t graph this"
                        now.layers[i].points = []
                        now.layers[i].contours = []
                        now.layers[i].surface = nil
                        continue
                    }
                    now.layers[i].error = r.error
                    now.layers[i].points = r.points
                    now.layers[i].contours = r.contours
                    now.layers[i].kind = !r.contour ? .curve : r.contourLevels ? .surface : .implicit
                    now.layers[i].surface = r.surface
                    now.layers[i].critical = r.criticalPoints
                    now.layers[i].roots = r.roots
                }
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
                if let fy = now.fixedY, !now.shows3D {
                    (now.yMin, now.yMax) = (fy.lowerBound, fy.upperBound)
                } else if finalY.1 > finalY.0 {
                    (now.yMin, now.yMax) = finalY
                }
                now.roots = result.roots
                now.critical = result.criticalPoints
                now.contours = result.contours
                now.kind = !result.contour ? .curve : result.contourLevels ? .surface : .implicit
                now.surface = result.surface
                now.angleUnit = unit.map { $0 == .rad ? "rad" : "deg" }
                now.xTicks = xTicks
                now.yTicks = yTicks
                now.squareYStep = square
                // a surface opens in 3D: sample it again as a square, with its grid
                let first3D = now.kind == .surface && now.view3D == nil
                if first3D { now.view3D = true }
                self.graph = now
                if first3D { self.sampleGraph() }
            }
        }
    }
}

