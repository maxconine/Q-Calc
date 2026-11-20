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

// ---- ultimate's board ----

var ut = [Bool?](repeating: nil, count: 81)
var utWon = [GameSpeech.UtOwner?](repeating: nil, count: 9)
check("an empty ultimate board says so", GameSpeech.utBoard(ut, boards: utWon, mine: "you", theirs: "computer") == "empty")
ut[4 * 9 + 4] = true; ut[4 * 9 + 0] = false; ut[4 * 9 + 7] = false
utWon[0] = .mine; utWon[8] = .draw
check("boards 1–9, won and drawn named, spots read",
      GameSpeech.utBoard(ut, boards: utWon, mine: "you", theirs: "computer")
          == "board 1: won by you. board 5: you centre; computer top left, bottom. board 9: drawn")
check("forced board spoken", GameSpeech.utNext(2, over: false) == "next move in board 3")
check("free move spoken", GameSpeech.utNext(nil, over: false) == "next move anywhere")
check("nothing next once over", GameSpeech.utNext(nil, over: true) == "")
check("the cursor's cell", GameSpeech.utCell(4 * 9 + 0, mark: nil, playable: true) == "board 5, top left")
check("a held cell names its mark", GameSpeech.utCell(80, mark: "computer", playable: false) == "board 9, bottom right, computer")
check("a closed empty cell says so", GameSpeech.utCell(9, mark: nil, playable: false) == "board 2, top left, can't play")

// ---- pong's score ----

check("a point names the scorer", GameSpeech.pongScore(mine: 3, theirs: 2, peer: "computer", scorer: true, won: nil)
          == "point to you. you 3, computer 2")
check("their point", GameSpeech.pongScore(mine: 0, theirs: 1, peer: "sam", scorer: false, won: nil) == "point to sam. you 0, sam 1")
check("a win gives the final score", GameSpeech.pongScore(mine: 7, theirs: 11, peer: "computer", scorer: false, won: false)
          == "computer wins, 11 to 7")
check("a reset reads the score alone", GameSpeech.pongScore(mine: 0, theirs: 0, peer: "computer", scorer: nil, won: nil)
          == "you 0, computer 0")

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
