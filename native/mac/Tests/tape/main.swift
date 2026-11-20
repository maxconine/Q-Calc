// Headless checks for the tape's rules (native/mac/scripts/test-tape.sh): which row stays picked after a delete or
// ⌘Z, where the tape scrolls, and what VoiceOver hears on a row. Foundation only: no app, no window.

import Foundation

var failures = 0

func check<T: Equatable>(_ what: String, _ got: T, _ want: T) {
    let ok = got == want
    print(ok ? "ok   \(what)" : "FAIL \(what)\n     got  \(got)\n     want \(want)")
    if !ok { failures += 1 }
}

// ⌘⌫ / ⌫ on a picked row
check("delete picked mid-tape keeps the slot", TapeRules.selectionAfterDelete(deleted: 2, selected: 2, count: 4), Optional(2))
check("delete picked last row picks new last", TapeRules.selectionAfterDelete(deleted: 4, selected: 4, count: 4), Optional(3))
check("delete the only row picks none", TapeRules.selectionAfterDelete(deleted: 0, selected: 0, count: 0), Optional<Int>.none)
// a row's Delete menu
check("menu delete with nothing picked picks none (⌫ on the line stays text)",
      TapeRules.selectionAfterDelete(deleted: 1, selected: nil, count: 3), Optional<Int>.none)
check("menu delete above the picked row keeps it", TapeRules.selectionAfterDelete(deleted: 0, selected: 3, count: 4), Optional(2))
check("menu delete below the picked row keeps it", TapeRules.selectionAfterDelete(deleted: 3, selected: 1, count: 3), Optional(1))
// ⌘Z
check("undo while walking the tape picks the row", TapeRules.selectsRestored(tapeOpen: true, selected: nil), true)
check("undo of a menu delete picks nothing", TapeRules.selectsRestored(tapeOpen: false, selected: nil), false)
// scrolling
check("rows change with a row picked: stay on it", TapeRules.scrollTarget(selected: 2, count: 10), Optional(2))
check("rows change with none picked: newest", TapeRules.scrollTarget(selected: nil, count: 10), Optional(9))
check("stale pick past the end: newest", TapeRules.scrollTarget(selected: 12, count: 10), Optional(9))
check("empty tape: nowhere", TapeRules.scrollTarget(selected: nil, count: 0), Optional<Int>.none)
// VoiceOver
check("answer row", TapeRules.spoken(expr: "2^10", display: "1024", note: false), "2^10 equals 1024")
check("note row has no equals", TapeRules.spoken(expr: "call Sam", display: "", note: true), "Note call Sam")
check("blank answer has no equals", TapeRules.spoken(expr: "f(x) = x", display: "", note: false), "f(x) = x")
// QA 97: ∫/Σ limits drawn beside the sign are said, not read as typed
check("integral limits said", TapeRules.spoken(expr: "∫_0^1 x^2 dx", display: "1/3", note: false), "integral from 0 to 1 x^2 dx equals 1/3")
check("sum limits said", TapeRules.spokenLimits("Σ_(n=1)^10 n^2"), "sum from n = 1 to 10 n^2")
check("product, bracketed upper", TapeRules.spokenLimits("2∏_(k=1)^(n+1)k"), "2 product from k = 1 to n+1 k")
check("lower limit only", TapeRules.spokenLimits("∫_C f"), "integral from C f")
check("plain ∫ kept", TapeRules.spokenLimits("∫ x dx"), "∫ x dx")
check("note kept as typed", TapeRules.spoken(expr: "∫_0^1", display: "", note: true), "Note ∫_0^1")
// VoiceOver: the live answer once typing pauses
check("live answer read", TapeRules.liveSpoken(answer: "1024", text: "2^10", last: ""), Optional("equals 1024"))
check("same answer not read twice", TapeRules.liveSpoken(answer: "1024", text: "2^10 ", last: "1024"), Optional<String>.none)
check("blank answer not read", TapeRules.liveSpoken(answer: " ", text: "2^", last: "1024"), Optional<String>.none)
check("empty line not read", TapeRules.liveSpoken(answer: "1024", text: "", last: ""), Optional<String>.none)
check("answer that repeats the line not read", TapeRules.liveSpoken(answer: "5", text: "5", last: ""), Optional<String>.none)

// an answer put in from the tape among other text (answerAmong)
check("empty line: the answer alone", TapeRules.answerAlone(line: "  ", selection: ""), true)
check("text on the line: among it", TapeRules.answerAlone(line: "5", selection: ""), false)
check("the whole line selected goes: alone", TapeRules.answerAlone(line: "5 + 2", selection: "5 + 2"), true)
check("part selected: among the rest", TapeRules.answerAlone(line: "5 + 2", selection: "2"), false)

// QA 98: ⌫ right after a ⇧-insert edits the line; after ↑ ↓ ← → it deletes the picked row
check("⌫ after ⇧-insert: the line", TapeRules.deleteEditsLine(kept: (row: 3, pick: 1), selected: 3, pick: 1), true)
check("⌫ after ← moved on: the row", TapeRules.deleteEditsLine(kept: (row: 3, pick: 1), selected: 3, pick: 0), false)
check("⌫ on another row: the row", TapeRules.deleteEditsLine(kept: (row: 3, pick: 1), selected: 2, pick: 1), false)
check("⌫ with no ⇧-insert: the row", TapeRules.deleteEditsLine(kept: nil, selected: 3, pick: 1), false)
// QA 98: each root of a solved row its own button
check("±1.41421: each root", TapeRules.rootLabels(display: "±1.41421", each: ["-1.41421", "1.41421"]), ["-1.41421", "1.41421"])
check("listed roots as the row reads", TapeRules.rootLabels(display: "-3, 3", each: ["-3", "3"]), ["-3", "3"])
check("more roots: the … isn't one", TapeRules.rootLabels(display: "1, 2, 3, …", each: ["1", "2", "3"]), ["1", "2", "3"])

print(failures == 0 ? "all tape checks pass" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
