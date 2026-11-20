// Headless checks for the walkthrough's and the cheat sheet's rules (native/mac/scripts/test-tour.sh). Foundation
// only: no app, no window.

import Foundation

var failures = 0

func check<T: Equatable>(_ what: String, _ got: T, _ want: T) {
    let ok = got == want
    print(ok ? "ok   \(what)" : "FAIL \(what)\n     got  \(got)\n     want \(want)")
    if !ok { failures += 1 }
}

let done = "that’s the basics · ? lists everything"

// the shortcut taught
check("registered shortcut is taught", TourRules.hotkeyTitle(active: "⌃⌥Space", chosen: "⌘⇧Space", failed: false), "⌃⌥Space")
check("not registered yet: the chosen one", TourRules.hotkeyTitle(active: nil, chosen: "⌘⇧Space", failed: false), "⌘⇧Space")
check("no shortcut works: none taught", TourRules.hotkeyTitle(active: nil, chosen: "⌘⇧Space", failed: true), "")

// graph lines count toward the graph extra
check("graph sin(x) is a graph", TourRules.isGraphLine("graph sin(x)"), true)
check("plot x^2 is a graph", TourRules.isGraphLine("  Plot x^2"), true)
check("graphite = 3 is not", TourRules.isGraphLine("graphite = 3"), false)

// the hint after a save or a copy
check("finishing the basics says so", TourRules.afterHint(basicsLeftBefore: true, basicsLeftAfter: false, doneHint: done, next: nil), Optional(done))
check("basics still left: no hint", TourRules.afterHint(basicsLeftBefore: true, basicsLeftAfter: true, doneHint: done, next: nil), Optional<String>.none)
check("past the basics: the picked hint", TourRules.afterHint(basicsLeftBefore: false, basicsLeftAfter: false, doneHint: done, next: "↑ brings it back"),
      Optional("↑ brings it back"))
check("copy also marks the ⌘C hint", TourRules.copyHintBit, 16)

// the cheat sheet's key rows
let rows = [("↵", "save"), ("", "degrees / radians"), ("esc", "hide")]
check("shortcut leads", TourRules.keyRows(hotkey: "⌃⌥Space", rows: rows).map { $0.0 + "|" + $0.1 },
      ["⌃⌥Space|show / hide", "↵|save", "esc|hide"])
check("no shortcut: no show / hide row", TourRules.keyRows(hotkey: "", rows: rows).map(\.1), ["save", "hide"])

if failures > 0 {
    print("\(failures) failed")
    exit(1)
}
print("all passed")
