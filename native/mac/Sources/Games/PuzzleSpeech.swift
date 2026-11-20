import Foundation

/// What VoiceOver reads for the one-player puzzles and trails (equal's rows and keys, 24/countdown's tiles,
/// the trails arena). Foundation only, so `scripts/test-puzzles.sh` checks it without the app or the core.
enum PuzzleSpeech {
    /// equal's marks, as the legend words them
    enum Mark: String {
        case hit = "right spot", near = "elsewhere", miss = "not in it"
    }

    /// one equal character, spoken (`*` and `×` are times, `-` and `−` minus)
    static func char(_ c: Character) -> String {
        switch c {
        case "+": return "plus"
        case "-", "−": return "minus"
        case "*", "×": return "times"
        case "/", "÷": return "divided by"
        case "=": return "equals"
        default: return String(c)
        }
    }

    /// An equal row: a scored guess says each character's mark, the row being typed says what's in it.
    static func equalRow(_ index: Int, text: String, marks: [Mark]?, current: Bool) -> String {
        let chars = Array(text)
        let head = "guess \(index + 1)"
        if let marks, marks.count == chars.count, !chars.isEmpty {
            return "\(head): " + zip(chars, marks).map { "\(char($0)) \($1.rawValue)" }.joined(separator: ", ")
        }
        if chars.isEmpty { return current ? "\(head), empty, typing here" : "\(head), empty" }
        return "\(head), typing: " + chars.map(char).joined(separator: " ")
    }

    /// A 24/countdown tile: its number, and whether the expression has used it.
    static func numbersTile(_ n: String, used: Bool) -> String { used ? "\(n), used" : n }

    /// The trails arena: the score, then the banner (a count-down or who took the round) when there is one.
    static func trailsArena(names: [String], score: [UInt32], banner: String) -> String {
        let parts = zip(names, score).map { "\($0) \($1)" }.joined(separator: ", ")
        let b = banner.trimmingCharacters(in: .whitespaces)
        return b.isEmpty ? parts : "\(parts). \(b)"
    }
}
