import Foundation

// What VoiceOver hears from the boards that are drawn as layers (connect 4, slots), and when slots' reels keep
// rolling across a rebuild. Pure (no QCalcCore, no AppKit) so `scripts/test-games.sh` checks it headless.

enum GameSpeech {
    /// Connect 4's board, column by column from the bottom: `column 1: you, computer. column 4: you`, empty
    /// columns left out. `board` is the core's row-major cells, row 0 at the bottom; true is a disc of mine.
    static func c4Board(_ board: [Bool?], mine: String, theirs: String) -> String {
        var cols: [String] = []
        for c in 0..<7 {
            var discs: [String] = []
            for r in 0..<6 {
                let i = r * 7 + c
                guard i < board.count, let me = board[i] else { break }
                discs.append(me ? mine : theirs)
            }
            if !discs.isEmpty { cols.append("column \(c + 1): \(discs.joined(separator: ", "))") }
        }
        return cols.isEmpty ? "empty" : cols.joined(separator: ". ")
    }

    /// The column held over the board, as a key moves it: `column 4`, `column 4, full`.
    static func c4Column(_ col: Int, full: Bool) -> String {
        "column \(col + 1)\(full ? ", full" : "")"
    }

    /// Slots' line and what it paid: `π, blank, e. +5`; just the line before any result.
    static func slotsLine(_ symbols: [String], result: String) -> String {
        let line = symbols.map { $0.isEmpty ? "blank" : $0 }.joined(separator: ", ")
        return result.isEmpty ? line : "\(line). \(result)"
    }

    /// A rebuild (resize, full screen, theme) while any reel still moves or bounces keeps it rolling from where it
    /// is, rather than snapping it to where it will land.
    static func slotsKeepRolling(settledAt: [Double], now: Double) -> Bool {
        settledAt.contains { $0 > now }
    }
}
