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
        // JS `===`: the same UTF-16 units (Swift's `==` would also call é and e + ◌́ equal)
        if next.value.utf16.elementsEqual(u.current.value.utf16) { return u }
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

    /// a key that wraps the selection is its own step, and undo brings the selection back with the text
    /// (QuickInput's wrapSelection: `current` takes the selection, then the run is broken)
    static func markSelection(_ u: State, start: Int, end: Int) -> State {
        var m = u
        m.current.start = start
        m.current.end = end
        return breakRun(m)
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

/// What a key press (or a click) is to a `sys` equation's or matrix cell's typing run (FieldUndo).
enum FieldKeyEdit: Equatable {
    case kind(Undo.Kind?), breakRun, none

    /// `keyCode` as kVK: typing and ⌫/⌦ join runs, ⌘⌫ / ⌘V / ⌘X are steps of
    /// their own, arrows and Home/End/Page keys end the run (a click does too, `breakRun`).
    static func of(keyCode: UInt16, command: Bool, control: Bool, characters: String) -> FieldKeyEdit {
        if keyCode == 51 || keyCode == 117 { return command || control ? .kind(nil) : .kind(.delete) }
        if command {
            let c = characters.lowercased()
            return c == "v" || c == "x" || keyCode == 9 || keyCode == 7 ? .kind(nil) : .none
        }
        if [123, 124, 125, 126, 115, 119, 116, 121].contains(keyCode) { return .breakRun }
        // ↵, Tab, esc move or close; ⌃ keys are the bar's actions
        if control || [36, 76, 48, 53].contains(keyCode) { return .none }
        guard let s = characters.unicodeScalars.first,
              !(0xF700...0xF8FF).contains(s.value), s.value >= 0x20, s.value != 0x7F else { return .none }
        return .kind(.insert)
    }
}

/// Native only (QA item 76): the `sys` equations' and matrix cells' own undo, each field its own `Undo.State`
/// with typing runs joined as on the line. SwiftUI sets those fields' text again on every key (symbols, the line
/// the grid writes), so the field editor's own undo had nothing and ⌘Z / ⇧⌘Z did nothing there. The panel tells
/// it each key (`note`) and every new set of values (`observe`); a change with no key just before it (a paste, ⌘⌫,
/// → filling parens, a system pasted in) is its own step.
final class FieldUndo<Key: Hashable> {
    /// how long a key's kind waits for the change it makes
    static var keyMs: Double { 300 }

    static var nowMs: Double { Date().timeIntervalSince1970 * 1000 }

    private var states: [Key: Undo.State] = [:]
    private var last: [Key: String] = [:]
    private var pending: (kind: Undo.Kind?, at: Double)?

    /// how many fields it last saw
    var count: Int { last.count }

    func note(_ edit: FieldKeyEdit, now: Double) {
        switch edit {
        case .kind(let k): pending = (k, now)
        case .breakRun: breakRun()
        case .none: break
        }
    }

    func breakRun() {
        pending = nil
        for k in states.keys { states[k] = Undo.breakRun(states[k]!) }
    }

    /// the fields' values now; a field seen for the first time starts its history there, a changed one records a
    /// step (the caret given for it, else at the end)
    func observe(_ values: [Key: String], caret: (Key) -> NSRange?, now: Double) {
        let kind = pending.flatMap { now - $0.at < Self.keyMs ? $0.kind : nil }
        var changed = false
        for (k, v) in values {
            guard let was = last[k] else {
                if states[k].map({ !$0.current.value.utf16.elementsEqual(v.utf16) }) ?? true { states[k] = Undo.start(v) }
                continue
            }
            guard !was.utf16.elementsEqual(v.utf16) else { continue }
            changed = true
            let s = states[k] ?? Undo.start(was)
            let n = (v as NSString).length
            let c = caret(k).flatMap { NSMaxRange($0) <= n ? $0 : nil } ?? NSRange(location: n, length: 0)
            states[k] = Undo.record(s, Undo.Step(value: v, start: c.location, end: NSMaxRange(c)), kind, now: now)
        }
        if changed { pending = nil }
        last = values
    }

    /// ⌘Z (or ⇧⌘Z) in field `key`: the value and selection to put back, nil when there's nothing
    func step(_ key: Key, redo: Bool) -> Undo.Step? {
        guard let s = states[key], let next = redo ? Undo.redo(s) : Undo.undo(s) else { return nil }
        states[key] = next
        return next.current
    }

    /// a new set of fields (a system of another size, another matrix)
    func reset() {
        states = [:]
        last = [:]
        pending = nil
    }
}
