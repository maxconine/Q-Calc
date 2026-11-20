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

    /// Whether an answer put in from the tape stands alone on the line (the page's `!rest.trim()`): nothing else is
    /// there, or all of it is selected and goes. Otherwise it goes in as `answerAmong` wraps it (`5(-2)`, not `5-2`).
    static func answerAlone(line: String, selection: String) -> Bool {
        let rest = line.trimmingCharacters(in: .whitespaces)
        return rest.isEmpty || (!selection.isEmpty && rest == selection.trimmingCharacters(in: .whitespaces))
    }

    /// native only (QA item 98): a plain ⌫ or ⌦ right after ⇧-click or ⇧↵ put an answer in (the row stays picked so
    /// more can follow) edits the line, not deletes that row; once ↑ ↓ ← → moved on, ⌫ deletes the picked row again.
    static func deleteEditsLine(kept: (row: Int, pick: Int)?, selected: Int?, pick: Int) -> Bool {
        guard let kept, let selected else { return false }
        return kept.row == selected && kept.pick == pick
    }

    /// The roots a solved tape row shows, each its own button (the page's RootPicks): as the row lists them
    /// (`-3, 3`), else each at the row's figures (`±1.41421` is `-1.41421` and `1.41421`).
    static func rootLabels(display: String, each: [String]) -> [String] {
        let shown = display.components(separatedBy: ", ").filter { $0 != "…" }
        return shown.count == each.count ? shown : each
    }

    /// What VoiceOver hears as ↑ ↓ land on a row, as the row reads: a note has no answer, so no "equals".
    static func spoken(expr: String, display: String, note: Bool) -> String {
        let expr = note ? expr : spokenLimits(expr)
        return note || display.isEmpty ? (note ? "Note \(expr)" : expr) : "\(expr) equals \(display)"
    }

    /// native only (QA item 97): a row's `∫_0^1` and `Σ_(n=1)^10`, drawn with the limits small beside the sign (QA
    /// item 92), are said "integral from 0 to 1" and "sum from n = 1 to 10", not read out as typed. The limits are
    /// read as `RadicalText` draws them: a bracketed run, or what runs up to a space, `^`, `_` or `(`.
    static func spokenLimits(_ text: String) -> String {
        let chars = Array(text)
        func limit(_ from: Int) -> (String, Int)? {
            guard from < chars.count else { return nil }
            if chars[from] == "(" {
                var depth = 0, j = from
                while j < chars.count {
                    if chars[j] == "(" { depth += 1 }
                    if chars[j] == ")" { depth -= 1; if depth == 0 { return (String(chars[(from + 1)..<j]), j + 1) } }
                    j += 1
                }
                return nil
            }
            var j = from
            while j < chars.count, !chars[j].isWhitespace, chars[j] != "^", chars[j] != "_", chars[j] != "(" { j += 1 }
            return j > from ? (String(chars[from..<j]), j) : nil
        }
        func said(_ bound: String) -> String {
            bound.replacingOccurrences(of: "=", with: " = ").split(separator: " ").joined(separator: " ")
        }
        var out = "", i = 0
        while i < chars.count {
            if "∫∑Σ∏Π".contains(chars[i]), i + 1 < chars.count, chars[i + 1] == "_", let (lower, afterLower) = limit(i + 2) {
                let name = chars[i] == "∫" ? "integral" : "∑Σ".contains(chars[i]) ? "sum" : "product"
                var words = "\(name) from \(said(lower))", end = afterLower
                if afterLower < chars.count, chars[afterLower] == "^", let (upper, afterUpper) = limit(afterLower + 1) {
                    words += " to \(said(upper))"
                    end = afterUpper
                }
                if let last = out.last, !last.isWhitespace { out += " " }
                out += words
                if end < chars.count, !chars[end].isWhitespace { out += " " }
                i = end
                continue
            }
            out.append(chars[i])
            i += 1
        }
        return out
    }

    /// What VoiceOver hears once typing pauses (native only; the page's live answer is aria-live): "equals" and the
    /// answer shown. Nothing for an empty line, a blank answer, an answer that only repeats the line (`5`), or the
    /// same answer read last time (a space typed, a unit retyped).
    static func liveSpoken(answer: String, text: String, last: String) -> String? {
        let a = answer.trimmingCharacters(in: .whitespaces), t = text.trimmingCharacters(in: .whitespaces)
        guard !a.isEmpty, !t.isEmpty, a != t, a != last else { return nil }
        return "equals \(a)"
    }
}
