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
// VoiceOver: the live answer once typing pauses
check("live answer read", TapeRules.liveSpoken(answer: "1024", text: "2^10", last: ""), Optional("equals 1024"))
check("same answer not read twice", TapeRules.liveSpoken(answer: "1024", text: "2^10 ", last: "1024"), Optional<String>.none)
check("blank answer not read", TapeRules.liveSpoken(answer: " ", text: "2^", last: "1024"), Optional<String>.none)
check("empty line not read", TapeRules.liveSpoken(answer: "1024", text: "", last: ""), Optional<String>.none)
check("answer that repeats the line not read", TapeRules.liveSpoken(answer: "5", text: "5", last: ""), Optional<String>.none)

print(failures == 0 ? "all tape checks pass" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
