import Foundation

/// The input's own undo, src/lib/undo.ts line for line. It runs in Swift, not through `callJson`: it's called on
/// every keystroke from the main thread, and a call to the engine's logic thread waited there behind the
/// keystroke's own decorations, with the whole history sent over as JSON and back each time.
/// Offsets are UTF-16, as in the TS.
enum Undo {
    struct Step: Equatable {
        var value: String
        var start: Int
        var end: Int
    }

    /// `insert` and `delete` runs group like typing does on the mac; anything else is its own step
    enum Kind: Equatable { case insert, delete }

    struct State: Equatable {
        var past: [Step]
        var future: [Step]
        var current: Step
        /// the kind of the run `current` belongs to, nil once the run is broken
        var run: Kind?
        var at: Double
    }

    static let runMs: Double = 1000
    static let limit = 200

    static func start(_ value: String) -> State {
        let n = (value as NSString).length
        return State(past: [], future: [], current: Step(value: value, start: n, end: n), run: nil, at: 0)
    }

    static func editKind(_ inputType: String?) -> Kind? {
        if inputType == "insertText" || inputType == "insertCompositionText" { return .insert }
        if inputType == "deleteContentBackward" || inputType == "deleteContentForward" { return .delete }
        return nil
    }

    static func record(_ u: State, _ next: Step, _ kind: Kind?, now: Double) -> State {
        if next.value == u.current.value { return u }
        let joins = kind != nil && u.run == kind && now - u.at < runMs
        if joins {
            var j = u
            j.future = []
            j.current = next
            j.at = now
            return j
        }
        return State(past: Array((u.past + [u.current]).suffix(limit)), future: [], current: next, run: kind, at: now)
    }

    /// a click or an arrow key ends the typing run, so the next keystroke starts a new step
    static func breakRun(_ u: State) -> State {
        guard u.run != nil else { return u }
        var b = u
        b.run = nil
        return b
    }

    static func undo(_ u: State) -> State? {
        guard let prev = u.past.last else { return nil }
        return State(past: Array(u.past.dropLast()), future: [u.current] + u.future, current: prev, run: nil, at: 0)
    }

    static func redo(_ u: State) -> State? {
        guard let next = u.future.first else { return nil }
        return State(past: u.past + [u.current], future: Array(u.future.dropFirst()), current: next, run: nil, at: 0)
    }
}
