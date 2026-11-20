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

// the completion ghost and grey parens follow each key (QA item 57)
check("typing the ghost's letter eats it", GhostRules.rest("rt", from: "sq", to: "sqr") ?? "nil", "t")
check("typing its last letter ends it", GhostRules.rest("t", from: "sqr", to: "sqrt") ?? "nil", "nil")
check("another letter drops it", GhostRules.rest("rt", from: "sq", to: "sqx") ?? "nil", "nil")
check("a capital still follows it", GhostRules.rest("ilogram", from: "5 k", to: "5 kI") ?? "nil", "logram")
check("a delete drops it", GhostRules.rest("rt", from: "sq", to: "s") ?? "nil", "nil")
check("a paste of its rest", GhostRules.rest("s(", from: "co", to: "cos") ?? "nil", "(")
check("grey ) for an open (", "\(GhostRules.parenCounts("sin(cos(2"))", "(leading: 0, trailing: 2)")
check("a typed ) takes the grey one's place", "\(GhostRules.parenCounts("(2)"))", "(leading: 0, trailing: 0)")
check("grey ( in front", "\(GhostRules.parenCounts("5+3)*2)"))", "(leading: 2, trailing: 0)")
check("both ends", "\(GhostRules.parenCounts(")("))", "(leading: 1, trailing: 1)")
check("filled", GhostRules.filled("2+3)*4", leading: 1, trailing: 0), "(2+3)*4")
check("a Typst line fills inside its $", GhostRules.filled("$sqrt(2$", leading: 0, trailing: 1), "$sqrt(2)$")
check("money isn't Typst", GhostRules.filled("$10 + (2", leading: 0, trailing: 1), "$10 + (2)")

// mid-line completion and a `$…$` line's grey `)` (QA item 64)
check("a word the caret ends mid-line", "\(GhostRules.midWordEnd("2 sq(9)", caret: 4) ?? -1)", "4")
check("not inside a word", "\(GhostRules.midWordEnd("2 sqrt(9)", caret: 4) ?? -1)", "-1")
check("not at the end", "\(GhostRules.midWordEnd("2 sq", caret: 4) ?? -1)", "-1")
check("not with a selection", "\(GhostRules.midWordEnd("2 sq(9)", caret: 4, length: 2) ?? -1)", "-1")
check("before a space", "\(GhostRules.midWordEnd("sq + 2", caret: 2) ?? -1)", "2")
check("sq|(9) has its paren", GhostRules.midCompletion("rt(", in: "2 sq(9)", at: 4) ?? "nil", "rt")
check("sq| + 2 takes the paren", GhostRules.midCompletion("rt(", in: "sq + 2", at: 2) ?? "nil", "rt(")
check("only the paren left", GhostRules.midCompletion("(", in: "2 sqrt(9)", at: 6) ?? "nil", "nil")
check("a Typst line", "\(GhostRules.typstLine("$sqrt(2$"))", "true")
check("money isn't a Typst line", "\(GhostRules.typstLine("$10 + (2"))", "false")
check("one $ sign", "\(GhostRules.typstLine("$"))", "false")

// QA item 71: a completion or a history insert is its own step (kind nil), typing after it starts a new one
var t = Undo.start("")
t = Undo.record(t, step("2 s"), .insert, now: 0)
t = Undo.record(t, step("2 sq"), .insert, now: 100)
t = Undo.record(t, step("2 sqrt("), nil, now: 200)
t = Undo.record(t, step("2 sqrt(9"), .insert, now: 300)
check("completion between typing runs", t.past.map(\.value).joined(separator: ","), ",2 sq,2 sqrt(")
t = Undo.undo(t)!
check("⌘Z takes the typing after the completion", t.current.value, "2 sqrt(")
t = Undo.undo(t)!
check("⌘Z takes the completion alone", t.current.value, "2 sq")
t = Undo.redo(t)!
check("⇧⌘Z puts the completion back", t.current.value, "2 sqrt(")
// a wrap: undo gives back the line with what was wrapped selected
var wu = Undo.record(Undo.start(""), step("2x+1"), .insert, now: 0)
wu = Undo.markSelection(wu, start: 0, end: 2)
wu = Undo.record(wu, Undo.Step(value: "(2x)+1", start: 1, end: 3), nil, now: 50)
wu = Undo.record(wu, step("(2x)+12"), .insert, now: 100)
check("typing after a wrap is its own step", wu.past.map(\.value).joined(separator: ","), ",2x+1,(2x)+1")
wu = Undo.undo(Undo.undo(wu)!)!
check("undo a wrap selects it again", "\(wu.current.value) \(wu.current.start) \(wu.current.end)", "2x+1 0 2")

if failures > 0 { print("\(failures) failed"); exit(1) }
print("all passed")
