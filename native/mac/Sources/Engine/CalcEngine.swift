import Foundation

// The app's whole view of the maths engine. Everything else in the app talks to this protocol, so moving from the
// foundation's raw UniFFI functions (FoundationEngine.swift) to the QCalcCore package's `Engine` object is a change
// to one file: write a `QCalcCoreEngine: CalcEngine` beside it and return it from `makeEngine()`.
//
// Every call is synchronous and runs on EngineThread (a 32 MB stack, ARCHITECTURE.md §5), never on the main thread.

/// Settings and sheet context, as the engine's `EvalOptions`.
struct EngineOptions: Equatable {
    var angleRadians = false
    var fractionMode = false
    var sigFigs = 6
    var sigFigMode = false
    var rationalize = true
    var spaceThousands = false
    var ans: Double?
    var ansQuantity: String?
    var defaultUnits: [String: String] = [:]

    static func from(_ s: AppSettings) -> EngineOptions {
        var o = EngineOptions()
        o.angleRadians = s.angleMode == "rad"
        o.fractionMode = s.fractionMode
        o.sigFigs = s.significantFigures
        o.sigFigMode = s.sigFigMode
        o.rationalize = s.rationalize
        o.spaceThousands = s.spaceThousands
        o.defaultUnits = s.defaultUnits
        return o
    }
}

enum EngineKind: String, Codable {
    case empty, expression, assignment, function, solve
}

/// One answered line.
struct EngineAnswer: Equatable {
    var raw: String
    var kind: EngineKind
    /// What the answer slot shows; empty when the line is blank.
    var display: String
    /// The closed form beside the decimal (`2sqrt(3)`).
    var exact: String?
    var number: Double?
    var unit: String?
    var variable: String?
    /// A unit answer as text the reader reads back, for chaining `ans`.
    var quantity: String?
    /// The whole TS `LineResult` JSON, for fields the typed record doesn't carry yet.
    var json: String

    var isBlank: Bool { display.trimmingCharacters(in: .whitespaces).isEmpty }
    static let blank = EngineAnswer(raw: "", kind: .empty, display: "", json: "{}")
}

/// Points of one curve, sampled by the engine.
struct GraphCurve: Equatable {
    var label: String
    /// nil where the function is undefined (a gap in the line).
    var points: [(x: Double, y: Double?)]

    static func == (a: GraphCurve, b: GraphCurve) -> Bool {
        a.label == b.label && a.points.count == b.points.count
            && zip(a.points, b.points).allSatisfy { $0.x == $1.x && $0.y == $1.y }
    }
}

protocol CalcEngine: AnyObject {
    var version: String { get }
    /// Every line of a sheet; each line sees what the lines above it defined. The last one is the live line.
    func evaluate(lines: [String], options: EngineOptions) -> [EngineAnswer]
    /// The TS engine's `{lines, options}` JSON in, `LineResult[]` JSON out.
    func evaluateJSON(_ request: String) throws -> String
    /// A graph's curves over [xMin, xMax], or nil when the engine can't graph the input. GRAPH package: until it
    /// lands this samples through the JSON escape hatch.
    func graph(expression: String, xMin: Double, xMax: Double, samples: Int, context: [String], options: EngineOptions) -> [GraphCurve]?
    /// The display text the answer slot draws (minus signs, digit grouping), `prettyAnswer` in the core.
    func pretty(_ answer: String) -> String
}

func makeEngine() -> CalcEngine {
    FoundationEngine()
}
