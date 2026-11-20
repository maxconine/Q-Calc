// Headless checks for the effects' and Konami code's rules (native/mac/scripts/test-eggs.sh). Foundation only.

import Foundation

var failures = 0

func check(_ what: String, _ ok: Bool) {
    print(ok ? "ok   \(what)" : "FAIL \(what)")
    if !ok { failures += 1 }
}

// ---- the Konami code's B and A left in the line ----

func left(_ b: String, _ a: String) -> String? {
    EggRules.konamiLeftover(before: b, after: a).map { "\($0.text)|\($0.caret)" }
}
check("ba at the end comes out", left("2+2", "2+2ba") == "2+2|3")
check("on an empty line", left("", "ba") == "|0")
check("in the middle, caret where it was", left("12", "1ba2") == "12|1")
check("typed with shift", left("x", "xBA") == "x|1")
check("a line that already ended in b", left("ab", "abba") == "ab|2")
check("after a π (UTF-16 caret)", left("π", "πba") == "π|1")
check("anything else changed: left alone", left("2+2", "2+3ba") == nil)
check("only one letter landed: left alone", left("2+2", "2+2a") == nil)
check("other letters: left alone", left("2", "2ab") == nil)

// ---- the hotkey during a barrel roll ----

check("a rolling bar is up (the hotkey hides it)", EggRules.barIsUp(visible: true, onActiveSpace: true, alpha: 0, rolling: true))
check("a bar mid-show at alpha 0 isn't", !EggRules.barIsUp(visible: true, onActiveSpace: true, alpha: 0, rolling: false))
check("a shown bar is up", EggRules.barIsUp(visible: true, onActiveSpace: true, alpha: 1, rolling: false))
check("another space's bar isn't", !EggRules.barIsUp(visible: true, onActiveSpace: false, alpha: 1, rolling: true))
check("a hidden bar isn't", !EggRules.barIsUp(visible: false, onActiveSpace: true, alpha: 1, rolling: false))

print(failures == 0 ? "all passed" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
