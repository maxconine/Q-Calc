import Foundation

/// The tape's selection and scrolling rules, kept free of the app so `scripts/test-tape.sh` can check them headless.
enum TapeRules {
    /// The row picked after row `deleted` goes from a tape now `count` rows long. With the deleted row picked (⌘⌫, ⌫),
    /// the row that took its place (or the new last row); with another row picked, that same row; with none picked (a
    /// row's Delete menu while the line is in use), none, so a plain ⌫ on the line doesn't then delete another row.
    static func selectionAfterDelete(deleted: Int, selected: Int?, count: Int) -> Int? {
        guard let s = selected, count > 0 else { return nil }
        if s == deleted { return min(deleted, count - 1) }
        return min(s > deleted ? s - 1 : s, count - 1)
    }

    /// Whether ⌘Z picks the row it brings back: only when the tape is being walked, for the reason above.
    static func selectsRestored(tapeOpen: Bool, selected: Int?) -> Bool { tapeOpen || selected != nil }

    /// Where the tape scrolls when its rows change: the picked row (a delete or ⌘Z mid-tape stays put), else the newest.
    static func scrollTarget(selected: Int?, count: Int) -> Int? {
        guard count > 0 else { return nil }
        if let s = selected, s >= 0, s < count { return s }
        return count - 1
    }

    /// What VoiceOver hears as ↑ ↓ land on a row, as the row reads: a note has no answer, so no "equals".
    static func spoken(expr: String, display: String, note: Bool) -> String {
        note || display.isEmpty ? (note ? "Note \(expr)" : expr) : "\(expr) equals \(display)"
    }
}
