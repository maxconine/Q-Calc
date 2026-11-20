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
        if trailing > 0, leading == 0, text.hasSuffix("$"), text.filter({ $0 == "$" }).count == 2,
           text.trimmingCharacters(in: .whitespaces).hasPrefix("$"), text.trimmingCharacters(in: .whitespaces).count > 2 {
            return String(text.dropLast()) + String(repeating: ")", count: trailing) + "$"
        }
        return String(repeating: "(", count: leading) + text + String(repeating: ")", count: trailing)
    }
}
