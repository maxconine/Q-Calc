# QCalcCore: the engine API the Mac UI calls

**Status: frozen (WP-FFI-API).** The Swift signatures below won't change. Later packages change what a call
returns, not how you call it. New fields can still be added to records, always with a default value, so code
that builds a record with the memberwise initializer keeps compiling. Treat every enum as non-exhaustive
(`@unknown default`), because a case may be added.

The Rust source is in `native/ffi/src/` (`engine.rs`, `records.rs`, `cancel.rs`, `history.rs`, `rates.rs`,
`bridge.rs`). UniFFI generates the Swift. This file is the reference, and the generated
`Sources/QCalcCore/qcalc_ffi.swift` is the authority if the two ever disagree.

## Using the package

```swift
// Package.swift of the app
.package(path: "../QCalcCore")              // native/QCalcCore
.target(name: "QCalcMac", dependencies: ["QCalcCore"])
```

Build the binary first with `native/scripts/build-mac.sh --no-test` (the full script also runs the Swift test on
both architectures and `swift test` on this package). It builds the Rust library for arm64
and x86_64, lipos them, regenerates `Sources/QCalcCore/qcalc_ffi.swift`, and writes
`native/QCalcCore/qcalc_ffiFFI.xcframework` (gitignored), which `Package.swift` uses as a `binaryTarget`. The
package's floor is macOS 12.0, the native app's own.

```swift
import QCalcCore

let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("Q Calc", isDirectory: true)
let engine = Engine(config: EngineConfig(isWindows: false, locale: "en_US", dataDir: support.path))
try? engine.reloadRates()                       // reads <dataDir>/rates.json, which ECBRates.swift writes

var history = engine.loadHistory()              // <dataDir>/history.json
var settings = defaultEvalOptions()             // build from UserDefaults qcalc.*
let view = HistoryView(show: .recent, answerForm: .exact, sigFigs: settings.sigFigs,
                       nowMs: Int64(Date().timeIntervalSince1970 * 1000))
let context = engine.contextFromHistory(rows: history, view: view)

// every keystroke, on one serial background queue (never the main thread), tagged with a generation
let text = field.stringValue
let request = LiveRequest(text: text, options: settings, context: context,
                          chained: engine.chains(text: text, context: context), nowMs: view.nowMs)
let live = engine.live(request: request)
DispatchQueue.main.async { answerLabel.stringValue = live.pretty }

// a calculus answer the closed-form search may upgrade (π²/6): cancel when the user types again
if let job = live.closedFormJob {
    upgrade?.cancel()
    upgrade = Task.detached { if let exact = try? await engine.closedForm(jobJson: job) { /* show it */ } }
}

// Enter
let done = engine.commit(request: request, history: history)
if done.saved { history = done.history; _ = try? engine.saveHistory(rows: history) }
else if let hint = done.hint { hintLabel.stringValue = hint }
```

`closedForm(jobJson:)` with no token is one of the conveniences in `QCalcCore+Async.swift`, alongside
`liveAsync(request:)`, `evaluateAsync(lines:options:)`, `graphAsync(input:options:settings:)`,
`Span.nsRange`/`TextSpan.nsRange` and `HistoryView.standard(sigFigs:now:)`. Each async convenience makes its
own `CancelToken` and cancels it when the calling Swift `Task` is cancelled, and then the call throws
`CoreError.Cancelled`.

## Threading

- `Engine` is thread-safe (`Sendable`). Every call hops to the engine's own evaluation thread, which has a
  **32 MB stack**, so deep inputs can't overflow a 512 KB dispatch stack. The deepest input the parser reads,
  250 nested parentheses, overflows a 512 KB thread when it is evaluated there directly, and the Swift test
  sends it from one. Sync calls block the caller until the answer is ready, usually well under a millisecond per keystroke.
  Call them off the main thread anyway.
- `…Async` calls run on a separate pool of two long-work threads, also with 32 MB stacks, so a long job never
  delays a keystroke. They take a `CancelToken`. A cancelled job throws `CoreError.Cancelled` at its next
  `work::spend` checkpoint, which every evaluation passes many times, or before it starts. Dropping the Swift
  `Task` alone doesn't stop the Rust work. Call `token.cancel()`, or use the conveniences, which do it for you.
- The same input always gives the same answer, because budgets count work and never time. Only the phrase layer
  reads a clock, and it reads `nowMs` from the request.
- A panic inside the engine never crashes the app. Sync calls return their blank value (an empty answer, `nil`,
  or `[]`), and calls that throw raise `CoreError.Internal`.

## Text offsets

Every `start`/`end` (`Span`, `TextSpan`) is a **UTF-16 offset**, as in the TS engine. That makes them
`NSRange`-compatible: `NSRange(location: Int(s.start), length: Int(s.end - s.start))`.

## Errors

```swift
enum CoreError: Error {
    case BadRequest(message: String)   // JSON that isn't the shape asked for; an unknown callJson function
    case Cancelled                     // the CancelToken was cancelled
    case Io(message: String)           // reading or writing <dataDir>/history.json or rates.json
    case Unported(function: String)    // callJson on a TS function nothing answers yet
    case Internal(message: String)     // the engine panicked; it is still usable
}
```

## Free functions (foundation, kept)

| Swift | Notes |
|---|---|
| `coreVersion() -> String` | the engine version |
| `defaultEvalOptions() -> EvalOptions` | a new user's settings (`defaultSettings()` in `src/lib/settings.ts`) |
| `evaluate(lines: [String], options: EvalOptions) -> [LineAnswer]` | runs on the caller's thread; prefer `Engine.evaluate` |
| `evaluateJson(request: String) throws -> String` | `{"lines":[…],"options":{…}}` in the TS shapes, `LineResult[]` JSON out |
| `mergedOptions(options: EvalOptions, context: EvalContext) -> EvalOptions` | settings plus history context in one `EvalOptions`; the context wins |

## `CancelToken`

```swift
final class CancelToken {
    init()
    func cancel()
    func isCancelled() -> Bool
}
```

## `NameCheck`

The relay's word on the name after `message` (`src/lib/nameCheck.ts`). The shell waits `nameCheckDelayMs()` after
typing rests, asks `POST /lookup` when `start` says to, and hands back `finish`. Times are ms since the epoch.

```swift
func nameCheckDelayMs() -> UInt32
final class NameCheck {
    init()
    func cached(name: String, nowMs: Double) -> Bool?           // nil: nothing fresh, so say nothing
    func start(name: String, nowMs: Double) -> Bool             // ask now (no fresh answer, none out)
    func finish(name: String, found: Bool?, nowMs: Double)      // nil (offline…) keeps nothing
    func noteSend(name: String, status: String, nowMs: Double)  // delivered/offline: found; unknown: not
}
```

## `Engine`

```swift
final class Engine {
    init(config: EngineConfig)

    // ---- the bar, per keystroke ----
    func live(request: LiveRequest) -> LiveAnswer
    func liveAsync(request: LiveRequest, cancel: CancelToken) async throws -> LiveAnswer
    /// Whether `text` continues from the last answer (an operator typed first, `in ft` after a unit answer).
    func chains(text: String, context: EvalContext) -> Bool
    /// Enter: the history row it saves and the new history, or why nothing was saved.
    func commit(request: LiveRequest, history: [HistoryRow]) -> CommitResult

    // ---- sheets ----
    func evaluate(lines: [String], options: EvalOptions) -> [LineAnswer]
    func evaluateAsync(lines: [String], options: EvalOptions, cancel: CancelToken) async throws -> [LineAnswer]
    func evaluateJson(request: String) throws -> String

    // ---- pieces of the bar ----
    func answerForms(source: FormSource) -> [AnswerView]                  // the Tab cycle
    func completion(text: String, names: CompletionNames) -> String?      // the ghost text after the caret
    func blankReason(text: String, options: EvalOptions, names: CompletionNames) -> Span?   // the squiggle
    func spans(text: String, names: CompletionNames) -> [TextSpan]        // units, known words, radicals, bounds
    func graph(input: String, options: GraphOptions, settings: EvalOptions) -> GraphResult?
    func graphAsync(input: String, options: GraphOptions, settings: EvalOptions,
                    cancel: CancelToken) async throws -> GraphResult?
    /// y at any x, for the readout between samples (`evaluateGraphY`).
    func graphY(expression: String, x: Double, options: GraphOptions, settings: EvalOptions) -> Double?
    /// z at (x, y) for a contour plot's readout.
    func graphZ(expression: String, x: Double, y: Double, options: GraphOptions, settings: EvalOptions) -> Double?
    /// The readout under the pointer as GraphPanel shows it (`result.y(x)` / `result.z(x, y)`): cut to the
    /// graph's `{…}` bounds, with the graph's own angle mode. Cheap (compiles, doesn't sample).
    func graphReadoutY(input: String, x: Double, options: GraphOptions, settings: EvalOptions) -> Double?
    func graphReadoutZ(input: String, x: Double, y: Double, options: GraphOptions, settings: EvalOptions) -> Double?
    /// The x window a graph opens on (`graphHome`); pass the panel's aspect.
    func graphHome(input: String, options: GraphOptions, settings: EvalOptions) -> GraphWindow
    /// Axis ticks (`graphTicks`): x with `result.angleUnit` and target 6, y with nil and 5.
    func graphTicks(lo: Double, hi: Double, unit: AngleUnit?, target: UInt32) -> [GraphTick]
    func typstSource(expr: String, answer: String, solvedFor: String) -> String?   // typstDocument
    /// The closed-form search (PSLQ) for `LiveAnswer.closedFormJob`; nil when none is found.
    func closedForm(jobJson: String, cancel: CancelToken) async throws -> String?

    // ---- currency rates ----
    /// The rates cache JSON (`{checked, fetched, date, rates}`, what ECBRates.swift writes) or `{base, rates}`.
    func setRates(ratesJson: String) throws
    /// Reads <dataDir>/rates.json; false when there is no file or no usable rate in it.
    func reloadRates() throws -> Bool
    func ratesStatus(nowMs: Int64) -> RatesStatus

    // ---- history ----
    func loadHistory() -> [HistoryRow]                         // <dataDir>/history.json, migrated and slimmed
    func saveHistory(rows: [HistoryRow]) throws -> [HistoryRow] // writes what persistableHistory keeps; returns it
    func persistableHistory(rows: [HistoryRow]) -> [HistoryRow]
    func normalizeHistory(rowsJson: String) -> String          // TS HistoryRow[] JSON (legacy rows too) → slimmed JSON
    func historyFromJson(rowsJson: String) -> [HistoryRow]     // the same, typed
    func historyToJson(rows: [HistoryRow]) -> String
    func contextFromHistory(rows: [HistoryRow], view: HistoryView) -> EvalContext

    // ---- reference ----
    func identitySheet(command: String) -> IdentitySheet?      // `identities`, `idt calculus`, `constants`…
    func elements() -> [Element]                               // the periodic table
    func hint(facts: HintFacts) -> Hint?                       // onboarding (`pickHint`)

    // ---- escape hatch ----
    /// Any ported TS function by module and name, with its arguments as a JSON array in the TS shapes
    /// (`callJson(module: "lib/completion", function: "completionFor", argsJson: #"["sq", {}]"#)`).
    func callJson(module: String, function: String, argsJson: String) throws -> String
}
```

## Records

Every field marked `= …` has that default in the Swift initializer. Field types are the Swift types that UniFFI
generates.

```swift
struct EngineConfig { var isWindows: Bool = false; var locale: String = "en_US"; var dataDir: String? = nil }

struct EvalOptions {                       // settings, plus optional context (see mergedOptions)
    var angleRadians: Bool = false
    var fractionMode: Bool = false
    var sigFigs: UInt32 = 6
    var sigFigMode: Bool = false
    var rationalize: Bool = true
    var spaceThousands: Bool = false
    var ans: Double? = nil
    var variables: [String: Double] = [:]
    var functions: [String: UserFn] = [:]
    var quantities: [String: String] = [:]   // unit-valued variables and ans, as text (`5 cm`)
    var defaultUnits: [String: String] = [:] // dimension → unit (`length` → `ft`)
    var measures: [String: MeasInfo] = [:]   // sig figs and ± of variables and `ans`
    var timeVarying: [String: Bool] = [:]    // d/dt letters moved by hand
}
struct UserFn { var params: [String]; var body: String }
struct MeasInfo { var sig: Int32? = nil; var dp: Int32? = nil; var unc: Double? = nil; var uncDigits: String? = nil }

struct EvalContext {                       // what the history defines, from contextFromHistory
    var ans: Double? = nil
    var variables: [String: Double] = [:]
    var functions: [String: UserFn] = [:]
    var measures: [String: MeasInfo] = [:]
    var quantities: [String: String] = [:]
    var chainAnswer: ChainAnswer? = nil      // the last answer as a chain continues from it
    var lastExpr: String? = nil              // the last row's expression (`isolate x` alone works on it)
}
struct ChainAnswer { var plain: String; var unit: Bool }

struct LiveRequest {
    var text: String
    var options: EvalOptions
    var context: EvalContext
    var chained: Bool = false                // from engine.chains(text:context:)
    var nowMs: Int64 = 0                     // the clock, for time phrases and the rates' freshness
    var phrases: Bool = true                 // answer natural language (`20% of 80`, `$20 in euros`)
}

struct LiveAnswer {
    var display: String                      // what is copied and saved; "" when blank
    var pretty: String                       // what the bar shows: grouping, a real minus sign
    var exact: String?                       // the closed form beside the decimal (`2sqrt(2)`)
    var dualLabel: String                    // `π/4 ≈ 0.785398`, or the display
    var number: Double?
    var unit: String?
    var kind: AnswerKind
    var blank: Span?                         // where a blank answer stops reading
    var completion: String?                  // the rest of the word being typed (no contacts: call completion() for `message …`)
    var solve: SolveInfo?
    var closedFormJob: String?               // pass to closedForm(jobJson:)
    var timeLetters: TimeLetters?
    var quantity: String? = nil              // a unit answer as text the reader reads back
    var variable: String? = nil              // what an assignment defined
    var meas: MeasInfo? = nil
    var valueJson: String? = nil             // the TS `Value`, for FormSource
    var expr: String = ""                    // what was evaluated (`ans*2` when chained)
    var phrase: Bool = false                 // the phrase layer answered
    var improper: Bool = false               // `improper unit conversion`
    var json: String                         // the whole TS LineResult
}

struct CommitResult {
    var saved: Bool
    var row: HistoryRow?                     // the row saved (or refreshed, when it repeats the last one)
    var history: [HistoryRow]                // the history to keep and save
    var input: String                        // what the bar holds now ("" after a save, the text when nothing saved)
    var hint: String?                        // when nothing was saved: why (`enterHint`)
    var json: String                         // the row as TS HistoryRow JSON
}

struct LineAnswer {                          // one line of a sheet (foundation)
    var raw: String; var kind: AnswerKind; var display: String; var exact: String?
    var number: Double?; var unit: String?; var variable: String?; var quantity: String?
    var json: String                         // the TS LineResult
}
enum AnswerKind { case empty, expression, assignment, function, solve }

struct SolveInfo { var variable: String; var roots: [Double]; var outcome: SolveOutcome; var more: Bool = false }
enum SolveOutcome { case roots, none, contradiction, noneFound, all }
struct TimeLetters { var varying: [String]; var constant: [String]; var locked: [String] }

struct HistoryRow {                          // the TS HistoryRow
    var id: String
    var expr: String
    var display: String
    var exact: String? = nil
    var n: Double? = nil
    var meas: MeasInfo? = nil
    var quantity: String? = nil
    var kind: HistoryKind? = nil
    var equations: [String]? = nil
    var fnName: String? = nil
    var fnParams: [String]? = nil
    var fnBody: String? = nil
    var solve: SolveInfo? = nil
    var at: Int64? = nil                     // commit time, ms since 1970
}
enum HistoryKind { case definition, function, system }
struct HistoryView {
    var show: HistoryShow                    // required: UniFFI can't default an enum field (settings: .recent)
    var answerForm: AnswerForm               // required (settings default: .exact)
    var sigFigs: UInt32 = 6
    var nowMs: Int64 = 0
}
enum HistoryShow { case recent, always, arrow }
enum AnswerForm { case exact, approx }

struct Span { var start: UInt32; var end: UInt32 }          // UTF-16
struct TextSpan { var start: UInt32; var end: UInt32; var kind: SpanKind; var name: String? = nil; var json: String = "" }
enum SpanKind { case unit, knownWord, radical, bound }
struct CompletionNames { var variables: [String] = []; var functions: [String] = []; var ans: Bool = false; var contacts: [String] = [] }

struct FormSource {                          // answerForms input: the answer as shown
    var valueJson: String                    // LiveAnswer.valueJson
    var display: String
    var exact: String? = nil
    var meas: MeasInfo? = nil
    var sigFigs: UInt32 = 6
    var sigFigMode: Bool = false
    var rationalize: Bool = true
}
struct AnswerView { var display: String; var valueJson: String? = nil }

struct GraphOptions { var domainLo: Double? = nil; var domainHi: Double? = nil; var sampleCount: UInt32? = nil
                      var findRoots: Bool = true; var aspect: Double? = nil }
struct GraphTick { var value: Double; var label: String }   // `−0.5`, `π/2`, `90°`
struct GraphWindow { var lo: Double; var hi: Double }
struct GraphResult {
    var label: String                        // `x^3`, `f(x)`, `y = x^3`
    var expression: String                   // in x (and y for a contour plot)
    var functionDef: FunctionDef?            // `graph f(x) = …`: register it
    var points: [GraphPoint]
    var domainLo: Double; var domainHi: Double
    var yMin: Double; var yMax: Double
    var criticalPoints: [CriticalPoint]
    var roots: [Double]
    var angleUnit: AngleUnit?                // nil when the curve doesn't depend on the angle mode
    var contours: [ContourLevel]
    var contour: Bool                        // a function of x and y
    var contourLevels: Bool                  // labelled level curves with a z readout (not an `f = g` curve)
    var error: String?
    var json: String                         // the TS GraphResult (without its functions)
}
struct FunctionDef { var name: String; var params: [String]; var body: String }
struct GraphPoint { var x: Double; var y: Double? }
struct CriticalPoint { var x: Double; var y: Double; var kind: CriticalKind }
enum CriticalKind { case min, max, critical }
enum AngleUnit { case deg, rad }
struct ContourLevel { var level: Double; var label: String; var segments: [Double]; var atX: Double?; var atY: Double? }

struct RatesStatus { var date: String?; var currencies: UInt32; var fresh: Bool; var fetchedMs: Int64? }

struct IdentitySheet { var id: String; var title: String; var hint: String; var sections: [IdentitySection] }
struct IdentitySection { var title: String; var groups: [IdentityGroup]; var wide: Bool }
struct IdentityGroup { var title: String; var items: [Identity] }
struct Identity { var show: String; var copy: String; var note: String?; var lead: [String] }

struct Element { var n: UInt32; var symbol: String; var name: String; var mass: String
                 var period: UInt32; var group: UInt32?; var category: ElementCategory }
enum ElementCategory { case alkali, alkaline, transition, post, metalloid, nonmetal, halogen, noble, lanthanide, actinide }

struct HintFacts {
    var seen: UInt32                         // the onboarding bit mask already shown
    var on: HintTrigger
    var expr: String
    var answer: String? = nil; var variable: String? = nil
    var unit: Bool = false; var chained: Bool = false; var angleRadians: Bool = false
    var fractionMode: Bool = false; var native: Bool = false; var opens: UInt32 = 0
}
enum HintTrigger { case pause, commit, open }
struct Hint { var bit: UInt32; var text: String }
```

## What answers today, and who wires the rest

The calls are frozen now. Calls marked *bridged* go through the JSON hatch (`callJson`, which is
`qcalc_core::golden::dispatch`) to the TS function named in the table. They return their blank value (`nil`,
`[]`, or a blank answer) until the package that ports that function registers it, and then they start answering
with no change on the Swift side.

| Call | Today | Wired by |
|---|---|---|
| `evaluate`, `evaluateAsync`, `evaluateJson`, `live` (maths) | foundation pipeline; readers still in `stubs.rs` stay blank | every engine WP |
| `live` per-keystroke flow (chain, isolate-previous, phrase merge) | interim flow in `ffi/src/engine.rs`, following `QuickCalc.tsx` | WP-APP-LOGIC `app/live.rs` replaces it |
| `live` phrases | bridged `engine/phrases` `answerPhrase` + `lib/nativeEval` `looksLikeNaturalLanguage` | WP-PHRASES |
| `chains`, `commit`, history, `contextFromHistory` | ported here (`history.rs`), replayed against `lib.history`, `lib.historyShow` | WP-APP-LOGIC may move them into core |
| `setRates`, `reloadRates`, `ratesStatus` | ported here (cache shape of `rates.rs`) | WP-PHRASES consumes them |
| `answerForms` | bridged `lib/answerForms` `answerForms` | WP-APP-LOGIC |
| `completion` | bridged `lib/completion` `completionFor` | WP-APP-LOGIC |
| `blankReason`, `commit.hint` | bridged `lib/blankReason` `blankReason`, `enterHint` | WP-APP-LOGIC |
| `spans` | bridged `lib/unitSpans` `unitSpans`, `lib/knownWords` `knownWordSpans`, `lib/radical` `radicalSpans`, `lib/bounds` `boundsIn` | WP-UNITS, WP-APP-LOGIC, WP-NOTATION |
| `graph`, `graphAsync`, `graphY`, `graphZ` | bridged `engine/graph` `buildGraph`, `evaluateGraphY`, `evaluateGraphZ` | WP-GRAPH |
| `typstSource` | bridged `lib/typstMath` `typstDocument` | WP-NOTATION |
| `closedForm` | bridged `engine/closedForm` `identifyJob` | WP-CALCULUS |
| `identitySheet` | bridged `lib/identities` `identitySheetFor` | WP-APP-LOGIC |
| `elements` | bridged `lib/elements` `ELEMENTS` (the table as JSON, no arguments) | WP-CHEM |
| `hint` | bridged `lib/onboarding` `pickHint` | WP-APP-LOGIC |

`evaluateGraphZ` and `ELEMENTS` aren't functions the TS exports. WP-GRAPH and WP-CHEM register those two
dispatch names for this API: `evaluateGraphZ(expression, x, y, options)` is `compileGraphZ(expression, options)(x, y)`,
and `ELEMENTS()` returns the `ELEMENTS` array.

## Files

- `<dataDir>/history.json`: `{"version": 1, "rows": [HistoryRow…]}` in the TS JSON shape. A bare array (an export
  of the web app's `localStorage`) also loads. It is written atomically (a temp file, then a rename).
- `<dataDir>/rates.json`: written by `macos/ECBRates.swift` (later by `qcalc-net`), read only by the engine:
  `{"checked": s, "fetched": s, "date": "YYYY-MM-DD", "rates": {"USD": 1.1367, …}}`, with times in seconds since
  1970. Rates over 7 days old (by `nowMs`) aren't used.
