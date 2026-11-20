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

// `sys` equations and matrix cells: each field its own undo, typing runs joined (QA item 76)
check("a letter is typing", "\(FieldKeyEdit.of(keyCode: 0, command: false, control: false, characters: "a") == .kind(.insert))", "true")
check("⌫ deletes", "\(FieldKeyEdit.of(keyCode: 51, command: false, control: false, characters: "\u{7F}") == .kind(.delete))", "true")
check("⌘⌫ is its own step", "\(FieldKeyEdit.of(keyCode: 51, command: true, control: false, characters: "\u{7F}") == .kind(nil))", "true")
check("⌘V is its own step", "\(FieldKeyEdit.of(keyCode: 9, command: true, control: false, characters: "v") == .kind(nil))", "true")
check("← ends the run", "\(FieldKeyEdit.of(keyCode: 123, command: false, control: false, characters: "\u{F702}") == .breakRun)", "true")
check("↵ is no edit", "\(FieldKeyEdit.of(keyCode: 36, command: false, control: false, characters: "\r") == FieldKeyEdit.none)", "true")
let fu = FieldUndo<Int>()
var vals = [0: "", 1: "y = 2"]
fu.observe(vals, caret: { _ in nil }, now: 0)
for (i, ch) in ["x", "x+", "x+y", "x+y=", "x+y=3"].enumerated() {
    fu.note(.kind(.insert), now: Double(1000 + i * 100))
    vals[0] = ch
    fu.observe(vals, caret: { _ in nil }, now: Double(1000 + i * 100 + 5))
}
fu.note(.kind(nil), now: 2000) // ⌘V
vals[1] = "y = 2x"
fu.observe(vals, caret: { _ in nil }, now: 2005)
fu.note(.kind(.insert), now: 2100)
vals[1] = "y = 2x+1"
fu.observe(vals, caret: { _ in nil }, now: 2105)
check("⌘Z in an equation takes its typing run", fu.step(0, redo: false)?.value ?? "nil", "")
check("nothing more to undo there", fu.step(0, redo: false)?.value ?? "nil", "nil")
check("⇧⌘Z puts the run back", fu.step(0, redo: true)?.value ?? "nil", "x+y=3")
check("the other field's typing after a paste", fu.step(1, redo: false)?.value ?? "nil", "y = 2x")
check("then the paste alone", fu.step(1, redo: false)?.value ?? "nil", "y = 2")
// a change with no key just before it (→ filling parens, ⌘⌫) is its own step, a key long ago doesn't count
let fp = FieldUndo<Int>()
fp.observe([0: "sin x"], caret: { _ in nil }, now: 0)
fp.note(.kind(.insert), now: 100)
fp.observe([0: "sin x2"], caret: { _ in nil }, now: 105)
fp.observe([0: "sin(x2)"], caret: { _ in nil }, now: 200)
fp.note(.kind(.insert), now: 300)
fp.observe([0: "sin(x2)+"], caret: { _ in nil }, now: 305)
check("parens filled are a step of their own", fp.step(0, redo: false)?.value ?? "nil", "sin(x2)")
check("then the typing before", fp.step(0, redo: false)?.value ?? "nil", "sin x2")
// a run broken by a click or an arrow; undo puts the caret back where it was
struct GridCellT: Hashable { let r: Int; let c: Int }
let fc = FieldUndo<GridCellT>()
let a = GridCellT(r: 0, c: 0)
fc.observe([a: ""], caret: { _ in nil }, now: 0)
fc.note(.kind(.insert), now: 10); fc.observe([a: "12"], caret: { _ in NSRange(location: 2, length: 0) }, now: 12)
fc.note(.breakRun, now: 20)
fc.note(.kind(.insert), now: 30); fc.observe([a: "312"], caret: { _ in NSRange(location: 1, length: 0) }, now: 32)
let back = fc.step(a, redo: false)
check("a click ends the cell's run, caret kept", "\(back?.value ?? "nil") \(back?.start ?? -1)", "12 2")
// a cell taken away and brought back (grid resized) starts afresh
fc.observe([:], caret: { _ in nil }, now: 40)
fc.observe([a: ""], caret: { _ in nil }, now: 50)
check("a cell brought back has no old history", fc.step(a, redo: false)?.value ?? "nil", "nil")

// QA item 105: no squiggle under a word while its completion ghost shows; exact forms with `pi` drawn as maths
let sq = NSRange(location: 0, length: 3)
check("ghost after `sqr` hides its squiggle", GhostRules.squiggle(sq, ghost: "t(", text: "sqr", caret: NSRange(location: 3, length: 0)) == nil ? "hidden" : "kept", "hidden")
check("no ghost keeps the squiggle", GhostRules.squiggle(sq, ghost: "", text: "sqr", caret: NSRange(location: 3, length: 0)) == sq ? "kept" : "hidden", "kept")
check("caret off the end keeps the squiggle", GhostRules.squiggle(sq, ghost: "t(", text: "sqr", caret: NSRange(location: 1, length: 0)) == sq ? "kept" : "hidden", "kept")
check("a squiggle before the ghosted word stays", GhostRules.squiggle(sq, ghost: "t(", text: "qqq + sqr", caret: NSRange(location: 9, length: 0)) == sq ? "kept" : "hidden", "kept")
check("pi^2/6", ExactText.answer("pi^2/6") ?? "nil", "π²/6")
check("pi^4/90", ExactText.answer("pi^4/90") ?? "nil", "π⁴/90")
check("1 - pi/4", ExactText.answer("1 - pi/4") ?? "nil", "1 − π/4")
check("-pi/2", ExactText.answer("-pi/2") ?? "nil", "−π/2")
check("3*pi/2", ExactText.answer("3*pi/2") ?? "nil", "3π/2")
check("pi*sqrt(2)", ExactText.answer("pi*sqrt(2)") ?? "nil", "π√2")
check("sqrt(3)*pi/6", ExactText.answer("sqrt(3)*pi/6") ?? "nil", "√3·π/6")
check("plain numbers left to Pretty", ExactText.answer("-1234.5") ?? "nil", "nil")
check("words with pi left to Pretty", ExactText.answer("3 pint") ?? "nil", "nil")
check("PSLQ π² left to Pretty", ExactText.answer("2 - π²/6") ?? "nil", "nil")

if failures > 0 { print("\(failures) failed"); exit(1) }
print("all passed")
