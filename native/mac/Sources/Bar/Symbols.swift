import Foundation

// The input's typing rules from QuickInput.tsx, ported as they are: typed words become symbols (WORD_SYMBOLS,
// AT_ONCE, LONGER_WORDS, SHORTCUT_SYMBOLS, prettyTokens), `[` pairing (bracketKey) and wrapping a selection
// (wrapSelection). WP-APP-LOGIC moves these into the core; then this calls the engine.
enum Symbols {
    private static func re(_ p: String, _ ci: Bool = false) -> NSRegularExpression {
        try! NSRegularExpression(pattern: p, options: ci ? [.caseInsensitive] : [])
    }

    // (pattern, symbol, template); ICU's look-behind must be bounded, so the lim arrow keeps its lead as a group
    private static let wordSymbols: [(NSRegularExpression, String, String?)] = [
        (re(#"(?<![\\A-Za-z])sqrt(?![A-Za-z])"#, true), "√", nil),
        (re(#"(?<![\\A-Za-z])pi(?![A-Za-z])"#, true), "π", nil),
        (re(#"(?<![\\A-Za-z])theta(?![A-Za-z0-9])"#, true), "θ", nil),
        (re(#"(?<![\\A-Za-z])infty(?![A-Za-z0-9])"#, true), "∞", nil),
        (re(#"(?<![\\A-Za-z])inf(?![A-Za-z0-9])"#, true), "∞", nil),
        (re(#"(?<![\\A-Za-z])cbrt(?![A-Za-z])"#, true), "∛", nil),
        (re(#"(\blim(?:it)?\s*_?[({]?\s*[A-Za-z]\s*)->"#), "→", "$1→"),
        (re(#"(?<![\\A-Za-z])plus[\s.-]?minus(?![A-Za-z])"#, true), "±", nil),
        (re(#"(?<![\\A-Za-z])minus[\s.-]?plus(?![A-Za-z])"#, true), "∓", nil),
        (re(#"(?<![\\A-Za-z])dot(?![A-Za-z])"#, true), "·", nil),
        (re(#"(?<![\\A-Za-z_])sum(?![A-Za-z0-9])"#), "Σ", nil),
        (re(#"(?<![\\A-Za-z_])prod(?![A-Za-z0-9])"#), "Π", nil),
        (re(#"(?<![\\A-Za-z_])int(?![A-Za-z]|_[A-Za-z])"#), "∫", nil),
    ]
    private static let shortcutSymbols: [(NSRegularExpression, String, String?)] = [
        (re(#"-[/.]\+"#), "∓", nil),
        (re(#"\+[/.]?-"#), "±", nil),
        (re(#"~"#), "±", nil),
    ]
    private static let atOnce: Set<String> = ["√", "∛", "±", "∓", "∫", "π", "Σ", "Π", "∞"]
    private static let longerWords: [(String, String, [String])] = [
        ("π", "pi", ["pint", "pica", "pico", "pipe", "pixel"]),
        ("∫", "int", ["integral", "integrate", "integer", "interest", "into"]),
        ("∞", "inf", ["infinity", "info", "infty"]),
        ("Σ", "sum", ["sums", "summary", "summation"]),
        ("Π", "prod", ["product"]),
    ]
    private static let message = re(#"^\s*message(?:\s|$)"#, true)

    private static func restoreLongerWords(_ text: String) -> String {
        var out = text
        for (sign, word, longer) in longerWords {
            let r = re(NSRegularExpression.escapedPattern(for: sign) + "_?([A-Za-z]+)")
            let ns = out as NSString
            var result = ""
            var last = 0
            for m in r.matches(in: out, range: NSRange(location: 0, length: ns.length)) {
                result += ns.substring(with: NSRange(location: last, length: m.range.location - last))
                let rest = ns.substring(with: m.range(at: 1))
                let w = (word + rest).lowercased()
                let hit = longer.contains { $0 == w || (rest.count >= 2 && ($0.hasPrefix(w) || w.hasPrefix($0))) }
                result += hit ? word + rest : ns.substring(with: m.range)
                last = m.range.location + m.range.length
            }
            result += ns.substring(from: last)
            out = result
        }
        return out
    }

    private static func replaceTokens(_ text: String, keepTrailing: Bool, keepWords: Bool) -> String {
        var out = keepWords ? text : restoreLongerWords(text)
        for (r, put, template) in (keepWords ? shortcutSymbols : wordSymbols + shortcutSymbols) {
            let ns = out as NSString
            var result = ""
            var last = 0
            for m in r.matches(in: out, range: NSRange(location: 0, length: ns.length)) {
                result += ns.substring(with: NSRange(location: last, length: m.range.location - last))
                let matched = ns.substring(with: m.range)
                let atEnd = m.range.location + m.range.length == ns.length
                // a word at the end may still grow into a longer one unless its symbol converts at once
                if keepTrailing, !atOnce.contains(put), atEnd, matched.last.map({ $0.isASCII && $0.isLetter }) == true {
                    result += matched
                } else if let template {
                    result += r.replacementString(for: m, in: out, offset: 0, template: template)
                } else {
                    result += put
                }
                last = m.range.location + m.range.length
            }
            result += ns.substring(from: last)
            out = result
        }
        return out
    }

    /// prettyTokens(text, caret): the text with symbols, and where the caret goes (UTF-16).
    static func pretty(_ text: String, caret: Int, keepWords: Bool) -> (text: String, caret: Int) {
        guard message.firstMatch(in: text, range: NSRange(location: 0, length: (text as NSString).length)) == nil else {
            return (text, caret)
        }
        let ns = text as NSString
        let c = max(0, min(caret, ns.length))
        let head = replaceTokens(ns.substring(to: c), keepTrailing: true, keepWords: keepWords)
        let tail = replaceTokens(ns.substring(from: c), keepTrailing: false, keepWords: keepWords)
        return (head + tail, (head as NSString).length)
    }

    private static func bracketsEven(_ s: String) -> Bool {
        s.filter { $0 == "[" }.count == s.filter { $0 == "]" }.count
    }

    /// bracketKey: `[` brings its `]`, a typed `]` steps over one, backspace in `[]` takes both.
    static func bracketKey(_ text: String, start: Int, end: Int, key: String) -> (text: String, caret: Int)? {
        guard start == end else { return nil }
        let ns = text as NSString
        let next: String? = start < ns.length ? ns.substring(with: NSRange(location: start, length: 1)) : nil
        switch key {
        case "[":
            if let next, next.rangeOfCharacter(from: CharacterSet(charactersIn: " \t]),;")) == nil { return nil }
            guard bracketsEven(text) else { return nil }
            return (ns.substring(to: start) + "[]" + ns.substring(from: start), start + 1)
        case "]":
            return next == "]" && bracketsEven(text) ? (text, start + 1) : nil
        case "backspace":
            guard start > 0, ns.substring(with: NSRange(location: start - 1, length: 1)) == "[", next == "]", bracketsEven(text) else { return nil }
            return (ns.substring(to: start - 1) + ns.substring(from: start + 1), start - 1)
        default:
            return nil
        }
    }

    /// wrapSelection: an opening bracket typed over a selection wraps it and keeps it selected.
    static func wrap(_ text: String, start: Int, end: Int, key: String) -> (text: String, start: Int, end: Int)? {
        let closes = ["(": ")", "[": "]", "{": "}"]
        guard let close = closes[key], end > start else { return nil }
        let ns = text as NSString
        return (ns.substring(to: start) + key + ns.substring(with: NSRange(location: start, length: end - start)) + close + ns.substring(from: end), start + 1, end + 1)
    }
}
