// Headless checks for the games' spoken boards and slots' rebuild rule (native/mac/scripts/test-games.sh).
// Foundation only: no app, no core.

import Foundation

var failures = 0

func check(_ what: String, _ ok: Bool) {
    print(ok ? "ok   \(what)" : "FAIL \(what)")
    if !ok { failures += 1 }
}

// ---- connect 4's board ----

var board = [Bool?](repeating: nil, count: 42)
check("an empty board says so", GameSpeech.c4Board(board, mine: "you", theirs: "computer") == "empty")
board[0] = true; board[7] = false; board[3] = true
check("columns bottom up, empty ones left out",
      GameSpeech.c4Board(board, mine: "you", theirs: "computer") == "column 1: you, computer. column 4: you")
let full = (0..<6).map { $0 * 7 + 6 }
func evenRow(_ i: Int) -> Bool { (i / 7) % 2 == 0 }
for i in full { board[i] = evenRow(i) }
check("a full column has all six", GameSpeech.c4Board(board, mine: "a", theirs: "b").hasSuffix("column 7: a, b, a, b, a, b"))
check("the held column", GameSpeech.c4Column(3, full: false) == "column 4")
check("a full held column says so", GameSpeech.c4Column(6, full: true) == "column 7, full")

// ---- slots ----

check("the line with blanks named", GameSpeech.slotsLine(["π", "", "e"], result: "+5") == "π, blank, e. +5")
check("just the line before a result", GameSpeech.slotsLine(["7", "7", "7"], result: "") == "7, 7, 7")
// a resize mid-spin used to snap every reel to where it would land
check("a rebuild mid-spin keeps rolling", GameSpeech.slotsKeepRolling(settledAt: [900, 1500, 2100], now: 1000))
check("a reel still bouncing keeps rolling", GameSpeech.slotsKeepRolling(settledAt: [400, 600, 1001], now: 1000))
check("a rebuild at rest places the reels", !GameSpeech.slotsKeepRolling(settledAt: [400, 600, 800], now: 1000))
check("before any spin the reels stand", !GameSpeech.slotsKeepRolling(settledAt: [-.infinity, -.infinity, -.infinity], now: 0))

print(failures == 0 ? "all passed" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
