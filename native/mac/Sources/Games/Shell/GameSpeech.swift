import Foundation

// What VoiceOver hears from the boards that are drawn as layers or canvases (connect 4, ultimate, pong, slots),
// and when slots' reels keep rolling across a rebuild. Pure (no QCalcCore, no AppKit) so `scripts/test-games.sh` checks it headless.

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

    /// A small board's outcome in ultimate, from the listener's side.
    enum UtOwner { case mine, theirs, draw }

    /// The nine spots of a 3×3 board, read row by row.
    static let utSpots = ["top left", "top", "top right", "left", "centre", "right", "bottom left", "bottom", "bottom right"]

    /// Ultimate's board, small board by small board (numbered 1–9 as the digit keys pick them), empty ones left
    /// out: `board 1: won by you. board 5: you centre; computer top left, bottom`. `cells` is `b * 9 + spot`,
    /// true a mark of mine.
    static func utBoard(_ cells: [Bool?], boards: [UtOwner?], mine: String, theirs: String) -> String {
        var out: [String] = []
        for b in 0..<9 {
            let owner = b < boards.count ? boards[b] : nil
            switch owner {
            case .mine?: out.append("board \(b + 1): won by \(mine)"); continue
            case .theirs?: out.append("board \(b + 1): won by \(theirs)"); continue
            case .draw?: out.append("board \(b + 1): drawn"); continue
            case nil: break
            }
            var me: [String] = [], them: [String] = []
            for s in 0..<9 {
                let i = b * 9 + s
                guard i < cells.count, let m = cells[i] else { continue }
                if m { me.append(utSpots[s]) } else { them.append(utSpots[s]) }
            }
            var parts: [String] = []
            if !me.isEmpty { parts.append("\(mine) \(me.joined(separator: ", "))") }
            if !them.isEmpty { parts.append("\(theirs) \(them.joined(separator: ", "))") }
            if !parts.isEmpty { out.append("board \(b + 1): \(parts.joined(separator: "; "))") }
        }
        return out.isEmpty ? "empty" : out.joined(separator: ". ")
    }

    /// Where the next move goes: `next move in board 3`, `next move anywhere`; nothing once the game is over.
    static func utNext(_ forced: Int?, over: Bool) -> String {
        over ? "" : forced.map { "next move in board \($0 + 1)" } ?? "next move anywhere"
    }

    /// The cell the keys moved to: `board 5, top left`, then who holds it, or that it can't be played.
    static func utCell(_ cell: Int, mark: String?, playable: Bool) -> String {
        let at = "board \(cell / 9 + 1), \(utSpots[cell % 9])"
        if let mark { return "\(at), \(mark)" }
        return playable ? at : "\(at), can't play"
    }

    /// Pong's score as it changes: `point to you. you 3, computer 2`; at the end `you win, 11 to 7`.
    /// `scorer` is true when the point was mine, nil when no point was just scored (a new game, say).
    static func pongScore(mine: Int, theirs: Int, peer: String, scorer: Bool?, won: Bool?) -> String {
        if let won {
            let hi = max(mine, theirs), lo = min(mine, theirs)
            return "\(won ? "you win" : "\(peer) wins"), \(hi) to \(lo)"
        }
        let score = "you \(mine), \(peer) \(theirs)"
        guard let scorer else { return score }
        return "point to \(scorer ? "you" : peer). \(score)"
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
