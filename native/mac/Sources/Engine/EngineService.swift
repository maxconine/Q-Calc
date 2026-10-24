import Foundation
import QCalcCore

// The app's one handle on the QCalcCore engine (native/QCalcCore/API.md). The engine is thread-safe and hops each
// call to its own 32 MB evaluation thread, but sync calls block their caller, so the bar only ever calls it
// through `EngineThread` (lanes, generations, newest-wins) and never from the main thread.

extension EvalOptions {
    /// The settings as the engine takes them, from the same UserDefaults keys the web app used.
    static func from(_ s: AppSettings) -> EvalOptions {
        var o = defaultEvalOptions()
        o.angleRadians = s.angleMode == "rad"
        o.fractionMode = s.fractionMode
        o.sigFigs = UInt32(AppSettings.clampSigFigs(s.significantFigures))
        o.sigFigMode = s.sigFigMode
        o.rationalize = s.rationalize
        o.spaceThousands = s.spaceThousands
        o.defaultUnits = s.defaultUnits
        return o
    }
}

extension HistoryView {
    static func from(_ s: AppSettings, now: Date = Date()) -> HistoryView {
        var view = HistoryView.standard(sigFigs: UInt32(AppSettings.clampSigFigs(s.significantFigures)), now: now)
        view.show = s.historyShow == "always" ? .always : s.historyShow == "arrow" ? .arrow : .recent
        view.answerForm = s.answerForm == "approx" ? .approx : .exact
        return view
    }
}

extension LiveAnswer {
    var isBlank: Bool { display.trimmingCharacters(in: .whitespaces).isEmpty }

    static let blank = LiveAnswer(display: "", pretty: "", exact: nil, dualLabel: "", number: nil, unit: nil,
                                  kind: .empty, blank: nil, completion: nil, solve: nil, closedFormJob: nil,
                                  timeLetters: nil, json: "{}")
}

extension HistoryRow: Identifiable {
    var hasDual: Bool {
        guard let exact = exact?.trimmingCharacters(in: .whitespaces), !exact.isEmpty else { return false }
        return exact != display.trimmingCharacters(in: .whitespaces)
    }
}

/// Points of one curve.
struct GraphCurve: Equatable {
    var label: String
    var points: [GraphPoint]
}

enum EngineFactory {
    static func make() -> Engine {
        let dir = AppInfo.supportDirectory
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return Engine(config: EngineConfig(isWindows: false, locale: Locale.current.identifier, dataDir: dir.path))
    }
}

/// Graph sampling until WP-GRAPH registers `buildGraph`: `Engine.graph` is bridged and answers nil until then,
/// so this samples y = f(x) through the `evaluateJson` escape hatch: one sheet of `x = …` then the curve.
enum GraphFallback {
    static func sample(_ engine: Engine, expression: String, xMin: Double, xMax: Double, samples: Int,
                       context: EvalContext, settings: EvalOptions) -> [GraphCurve]? {
        let body = expression.trimmingCharacters(in: .whitespaces)
        guard !body.isEmpty, samples > 1, xMax > xMin else { return nil }
        let rhs: String = {
            if let eq = body.firstIndex(of: "="), body.lowercased().hasPrefix("y") || body.contains("(x)") {
                return String(body[body.index(after: eq)...]).trimmingCharacters(in: .whitespaces)
            }
            return body
        }()
        var lines: [String] = []
        var xs: [Double] = []
        for i in 0..<samples {
            let x = xMin + (xMax - xMin) * Double(i) / Double(samples - 1)
            xs.append(x)
            lines.append("x = \(x)")
            lines.append(rhs)
        }
        var functions: [String: Any] = [:]
        for (name, fn) in context.functions { functions[name] = ["params": fn.params, "body": fn.body] }
        var variables: [String: Any] = [:]
        for (name, v) in context.variables where name != "x" { variables[name] = v }
        var opts: [String: Any] = [
            "angleMode": settings.angleRadians ? "rad" : "deg", "sigFigs": 16, "rationalize": false,
            "functions": functions, "variables": variables,
        ]
        if let ans = context.ans { opts["ans"] = ans }
        guard let data = try? JSONSerialization.data(withJSONObject: ["lines": lines, "options": opts]),
              let request = String(data: data, encoding: .utf8),
              let reply = try? engine.evaluateJson(request: request),
              let rows = try? JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [[String: Any]],
              rows.count == samples * 2
        else { return nil }
        var points: [GraphPoint] = []
        var any = false
        for (i, x) in xs.enumerated() {
            let value = rows[i * 2 + 1]["value"] as? [String: Any]
            var y: Double?
            if value?["kind"] as? String == "number", value?["unit"] == nil,
               let n = (value?["n"] as? NSNumber)?.doubleValue, n.isFinite {
                y = n
                any = true
            }
            points.append(GraphPoint(x: x, y: y))
        }
        return any ? [GraphCurve(label: rhs, points: points)] : nil
    }
}
