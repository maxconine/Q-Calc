import Foundation

/// Text from elsewhere (a paste, a highlighted selection) as one plain line, the way Spotlight takes it: every run
/// of spaces, tabs, indents and line breaks becomes a single space.
enum PlainText {
    /// one line, every whitespace run (no-break and zero-width spaces too) a single space, trimmed
    static func flatten(_ s: String) -> String {
        s.replacingOccurrences(of: #"[\s\x{00A0}\x{200B}\x{2028}\x{2029}\x{FEFF}]+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
    }

    /// a sentence or paragraph rather than math: five or more words, mostly letters
    static func isProse(_ s: String) -> Bool {
        let words = s.split(whereSeparator: \.isWhitespace)
        guard words.count >= 5 else { return false }
        let wordy = words.filter { w in w.filter(\.isLetter).count >= 2 }.count
        let marks = s.filter { !$0.isWhitespace }
        let letters = marks.filter(\.isLetter).count
        return wordy * 2 >= words.count && letters * 10 >= marks.count * 6
    }
}
