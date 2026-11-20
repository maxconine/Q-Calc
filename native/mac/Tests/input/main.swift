// Headless checks for the bar's typing rules (native/mac/scripts/test-input.sh): words to symbols
// (Symbols.pretty), `[` pairing and wrapping (Symbols.bracketKey / wrap) and the undo runs (Undo).
// Foundation only: no app, no window.

import Foundation

var failures = 0

func check(_ what: String, _ got: String, _ want: String) {
    let ok = got == want
    print(ok ? "ok   \(what)" : "FAIL \(what)\n     got  \(got)\n     want \(want)")
    if !ok { failures += 1 }
}

/// prettyTokens with the caret at the end, the way typing reaches it; `|` marks the caret in the result
func typed(_ s: String, keep: Bool = false) -> String {
    let p = Symbols.pretty(s, caret: (s as NSString).length, keepWords: keep)
    let ns = p.text as NSString
    return ns.substring(to: p.caret) + "|" + ns.substring(from: p.caret)
}

// words to symbols
check("sqrt at once", typed("sqrt"), "√|")
check("pi at once", typed("2pi"), "2π|")
check("theta waits at the end", typed("theta"), "theta|")
check("theta after a space", typed("theta "), "θ |")
check("pint stays a unit", typed("3 pint"), "3 pint|")
check("integer stays a word", typed("integer"), "integer|")
check("no symbol inside a word", typed("epsilon"), "epsilon|")
check("+- is ±", typed("3+-"), "3±|")
check("keep words leaves sqrt", typed("sqrt", keep: true), "sqrt|")
check("keep words still takes +-", typed("1+-", keep: true), "1±|")
check("message lines stay", typed("message max pi"), "message max pi|")
check("lim arrow", typed("lim x->"), "lim x→|")
let mid = Symbols.pretty("sqrt9", caret: 4, keepWords: false)
check("caret kept after a swap", "\(mid.text) \(mid.caret)", "√9 1")

// brackets
func key(_ s: String, _ at: Int, _ k: String) -> String {
    guard let r = Symbols.bracketKey(s, start: at, end: at, key: k) else { return "nil" }
    let ns = r.text as NSString
    return ns.substring(to: r.caret) + "|" + ns.substring(from: r.caret)
}
check("[ pairs at the end", key("v=", 2, "["), "v=[|]")
check("[ before a word doesn't pair", key("x", 0, "["), "nil")
check("] steps over", key("[1 2]", 4, "]"), "[1 2]|")
check("] with odd brackets types", key("[[1]", 3, "]"), "nil")
check("⌫ in [] takes both", key("a[]", 2, "backspace"), "a|")
let w = Symbols.wrap("2x+1", start: 0, end: 2, key: "(")
check("( wraps a selection", w.map { "\($0.text) \($0.start) \($0.end)" } ?? "nil", "(2x)+1 1 3")

// undo runs
func step(_ v: String) -> Undo.Step { Undo.Step(value: v, start: (v as NSString).length, end: (v as NSString).length) }
var u = Undo.start("")
u = Undo.record(u, step("1"), .insert, now: 0)
u = Undo.record(u, step("12"), .insert, now: 100)
check("typing joins a run", u.past.map(\.value).joined(separator: ","), "")
// a paste is its own step (editKind nil): the bar had sent it as typing, so one ⌘Z took it and the typing before
u = Undo.record(u, step("12+34"), Undo.editKind("insertFromPaste"), now: 200)
check("paste is its own step", u.past.map(\.value).joined(separator: ","), ",12")
u = Undo.record(u, step("12+345"), .insert, now: 300)
check("typing after a paste starts a run", u.past.map(\.value).joined(separator: ","), ",12,12+34")
u = Undo.undo(u)!
check("undo back to the paste", u.current.value, "12+34")
u = Undo.undo(u)!
check("undo the paste alone", u.current.value, "12")
u = Undo.redo(u)!
check("redo the paste", u.current.value, "12+34")
var c = Undo.record(Undo.start("ab"), step("a"), Undo.editKind("deleteByCut"), now: 0)
c = Undo.record(c, step("ax"), .insert, now: 10)
check("a cut is its own step", c.past.map(\.value).joined(separator: ","), "ab,a")
var r = Undo.record(Undo.start(""), step("1"), .insert, now: 0)
r = Undo.record(r, step("12"), .insert, now: 2000)
check("a pause ends the run", r.past.map(\.value).joined(separator: ","), ",1")
r = Undo.breakRun(r)
r = Undo.record(r, step("123"), .insert, now: 2001)
check("a click ends the run", r.past.map(\.value).joined(separator: ","), ",1,12")

if failures > 0 { print("\(failures) failed"); exit(1) }
print("all passed")
