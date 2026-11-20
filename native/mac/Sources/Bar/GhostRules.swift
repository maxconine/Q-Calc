import Foundation

/// The bar's faint helpers as the text changes (QA item 57), pure so `scripts/test-input.sh` checks them: the
/// completion ghost and the grey inferred parens follow each keystroke at once, as QuickInput.tsx draws them on
/// every render, instead of waiting for the engine's answer (the ghost blinked off on every key and the grey `)`
/// stayed one keystroke behind).
enum GhostRules {
    /// what's left of the completion ghost after an edit: its rest when the new text only typed its next letters
    /// (`sq` + `rt`, then `sqr` + `t`), nil otherwise (the engine's answer brings the next one)
    static func rest(_ ghost: String?, from old: String, to new: String) -> String? {
        guard let ghost, !ghost.isEmpty, new.count > old.count, new.hasPrefix(old) else { return nil }
        let typed = new.dropFirst(old.count)
        guard ghost.count > typed.count, ghost.lowercased().hasPrefix(typed.lowercased()) else { return nil }
        return String(ghost.dropFirst(typed.count))
    }

    /// inferParens (core parens.rs), cheap enough for the main thread: how many `(` go in front and `)` behind
    static func parenCounts(_ text: String) -> (leading: Int, trailing: Int) {
        var depth = 0, low = 0
        for c in text.unicodeScalars {
            if c == "(" { depth += 1 } else if c == ")" { depth -= 1; low = min(low, depth) }
        }
        return (-low, depth - low)
    }

    /// the line with its grey parens typed in (Tab/→): a Typst line between `$` signs takes its `)` inside the
    /// closing `$`, where it reads (core `fill_typed`; `$sqrt(2$)` was blank)
    static func filled(_ text: String, leading: Int, trailing: Int) -> String {
        if trailing > 0, leading == 0, typstLine(text) {
            return String(text.dropLast()) + String(repeating: ")", count: trailing) + "$"
        }
        return String(repeating: "(", count: leading) + text + String(repeating: ")", count: trailing)
    }

    /// a Typst line between two `$` signs (`$sqrt(2$`, not `$10 + (2`): its grey `)` is drawn and typed inside the
    /// closing `$` (QA item 64; they were drawn after it, where Tab didn't put them)
    static func typstLine(_ text: String) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        return text.hasSuffix("$") && text.filter({ $0 == "$" }).count == 2 && trimmed.hasPrefix("$") && trimmed.count > 2
    }

    /// where a word typed mid-line can complete (QuickInput's `completionAt`): a collapsed caret inside the line
    /// (UTF-16 offset) with no letter, digit or `_` after it, so `2 sq|(9)` completes as `2 sq|` does. Nil at the
    /// end (the ghost after the text takes that) or inside a word.
    static func midWordEnd(_ text: String, caret: Int, length selection: Int = 0) -> Int? {
        let ns = text as NSString
        guard selection == 0, caret > 0, caret < ns.length else { return nil }
        guard let next = Unicode.Scalar(ns.character(at: caret)) else { return caret }
        let word = (next.isASCII && (CharacterSet.alphanumerics.contains(next) || next == "_"))
        return word ? nil : caret
    }

    /// the completion at a mid-line word end: `sq|(9)` already has its paren, so `rt(` ghosts `rt`; nil when
    /// nothing is left
    static func midCompletion(_ completed: String?, in text: String, at: Int) -> String? {
        guard var rest = completed, !rest.isEmpty else { return nil }
        let ns = text as NSString
        if at < ns.length, ns.character(at: at) == 0x28, rest.hasSuffix("(") { rest.removeLast() }
        return rest.isEmpty ? nil : rest
    }
}

/// the completion ghost drawn inside the line, at a word the caret ends there (QA item 64), for the line it was
/// worked out for
struct MidGhost: Equatable {
    var at: Int
    var text: String
    var line: String
}
