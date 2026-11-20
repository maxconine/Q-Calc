// Headless checks for what VoiceOver reads in equal, 24/countdown and trails (native/mac/scripts/test-puzzles.sh).
// Foundation only: no app, no core.

import Foundation

var failures = 0

func check(_ what: String, _ ok: Bool, _ got: String = "") {
    print(ok ? "ok   \(what)" : "FAIL \(what)\(got.isEmpty ? "" : ": \(got)")")
    if !ok { failures += 1 }
}

// ---- equal: a scored row said each tile's colour as nothing (only the glyph was read) ----

let row = PuzzleSpeech.equalRow(0, text: "12+3=15", marks: [.hit, .near, .miss, .hit, .hit, .near, .miss], current: false)
check("a scored row names every mark", row == "guess 1: 1 right spot, 2 elsewhere, plus not in it, 3 right spot, equals right spot, 1 elsewhere, 5 not in it", row)
check("operators are words", PuzzleSpeech.char("*") == "times" && PuzzleSpeech.char("÷") == "divided by" && PuzzleSpeech.char("-") == "minus")
check("the row being typed", PuzzleSpeech.equalRow(2, text: "4*5", marks: nil, current: true) == "guess 3, typing: 4 times 5")
check("the empty row being typed", PuzzleSpeech.equalRow(1, text: "", marks: nil, current: true) == "guess 2, empty, typing here")
check("a row still to come", PuzzleSpeech.equalRow(5, text: "", marks: nil, current: false) == "guess 6, empty")
check("marks that don't match the text fall back to the characters",
      PuzzleSpeech.equalRow(0, text: "12", marks: [.hit], current: false) == "guess 1, typing: 1 2")

// ---- 24/countdown: a used tile looked faded and read the same ----

check("a used tile says so", PuzzleSpeech.numbersTile("8", used: true) == "8, used")
check("a free tile is its number", PuzzleSpeech.numbersTile("100", used: false) == "100")

// ---- trails: the arena read "arena" and nothing else ----

check("score only", PuzzleSpeech.trailsArena(names: ["you", "computer"], score: [1, 2], banner: "") == "you 1, computer 2")
check("score and banner", PuzzleSpeech.trailsArena(names: ["you", "computer"], score: [0, 0], banner: "3") == "you 0, computer 0. 3")

print(failures == 0 ? "all passed" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
