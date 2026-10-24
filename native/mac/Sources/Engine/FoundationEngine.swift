import Foundation
import QCalcFFI

// The engine through the foundation's UniFFI functions (native/ffi: core_version, evaluate, evaluate_json), built
// into the QCalcFFI module by build.sh. The only file that imports it: when QCalcCore lands, a QCalcCoreEngine
// replaces this one (see CalcEngine.swift).
final class FoundationEngine: CalcEngine {
    var version: String { QCalcFFI.coreVersion() }

    func evaluate(lines: [String], options: EngineOptions) -> [EngineAnswer] {
        QCalcFFI.evaluate(lines: lines, options: ffiOptions(options)).map { a in
            EngineAnswer(
                raw: a.raw,
                kind: kind(a.kind),
                display: a.display,
                exact: a.exact,
                number: a.number,
                unit: a.unit,
                variable: a.variable,
                quantity: a.quantity,
                json: a.json
            )
        }
    }

    func evaluateJSON(_ request: String) throws -> String {
        try QCalcFFI.evaluateJson(request: request)
    }

    // the JSON escape hatch: one sheet of `x = …` then the curve, per sample, read back from LineResult.value.n.
    // GRAPH (engine.graph) replaces this with real sampling, asymptotes and implicit curves
    func graph(expression: String, xMin: Double, xMax: Double, samples: Int, context: [String], options: EngineOptions) -> [GraphCurve]? {
        let body = expression.trimmingCharacters(in: .whitespaces)
        guard !body.isEmpty, samples > 1, xMax > xMin else { return nil }
        // y = f(x) and f(x) = … graph their right-hand side
        let rhs: String = {
            if let eq = body.firstIndex(of: "="), body.lowercased().hasPrefix("y") || body.contains("(x)") {
                return String(body[body.index(after: eq)...]).trimmingCharacters(in: .whitespaces)
            }
            return body
        }()
        var lines = context
        var xs: [Double] = []
        for i in 0..<samples {
            let x = xMin + (xMax - xMin) * Double(i) / Double(samples - 1)
            xs.append(x)
            lines.append("x = \(x)")
            lines.append(rhs)
        }
        var opts: [String: Any] = [
            "angleMode": options.angleRadians ? "rad" : "deg",
            "sigFigs": 16,
            "rationalize": false,
        ]
        if let ans = options.ans { opts["ans"] = ans }
        guard let data = try? JSONSerialization.data(withJSONObject: ["lines": lines, "options": opts]),
              let request = String(data: data, encoding: .utf8),
              let reply = try? evaluateJSON(request),
              let rows = try? JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [[String: Any]]
        else { return nil }
        let answers = Array(rows.suffix(samples * 2))
        var points: [(x: Double, y: Double?)] = []
        var any = false
        for (i, x) in xs.enumerated() {
            let row = answers.count == samples * 2 ? answers[i * 2 + 1] : [:]
            let value = row["value"] as? [String: Any]
            var y: Double?
            if value?["kind"] as? String == "number", value?["unit"] == nil, let n = (value?["n"] as? NSNumber)?.doubleValue, n.isFinite {
                y = n
                any = true
            }
            points.append((x, y))
        }
        return any ? [GraphCurve(label: rhs, points: points)] : nil
    }

    func pretty(_ answer: String) -> String {
        Pretty.answer(answer)
    }

    private func kind(_ k: AnswerKind) -> EngineKind {
        switch k {
        case .empty: return .empty
        case .expression: return .expression
        case .assignment: return .assignment
        case .function: return .function
        case .solve: return .solve
        }
    }

    private func ffiOptions(_ o: EngineOptions) -> EvalOptions {
        var out = QCalcFFI.defaultEvalOptions()
        out.angleRadians = o.angleRadians
        out.fractionMode = o.fractionMode
        out.sigFigs = UInt32(max(2, min(16, o.sigFigs)))
        out.sigFigMode = o.sigFigMode
        out.rationalize = o.rationalize
        out.spaceThousands = o.spaceThousands
        out.ans = o.ans
        if let q = o.ansQuantity { out.quantities["ans"] = q }
        out.defaultUnits = o.defaultUnits
        return out
    }
}
