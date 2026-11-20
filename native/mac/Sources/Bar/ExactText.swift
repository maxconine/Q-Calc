import Foundation

/// Native only (QA item 105): an exact form the engine writes with `pi` (a sum's `pi^2/6`, `1 - pi/4`) drawn as
/// maths in the answer slot and the tape, `π²/6`, `1 − π/4`. Display only: copy, insert and the goldens keep the
/// engine's text. Pure, so `scripts/test-input.sh` checks it.
enum ExactText {
    private static func regex(_ pattern: String) -> NSRegularExpression { try! NSRegularExpression(pattern: pattern) }
    private static let minus = "−"
    // what Pretty.answer reads as maths (its `mathOnly`), once `pi` is π
    private static let mathOnly = regex(#"^[\d\s+\-*/^().,πe√i·×±∓sqrt]+$"#)
    private static let looseMinus = regex(#"(^|[\s(])-(?=[\s\d(π√sei])"#)
    private static let binaryMinus = regex(#"(?<=\S) - (?=\S)"#)
    private static let shortRoot = regex(#"sqrt\(([0-9.]+|[A-Za-zπ])\)"#)
    private static let digitPower = regex(#"\^(\d+)"#)
    private static let timesSymbol = regex(#"(?<=[\dπ])\s*\*\s*(?=[π√(]|sqrt)"#)
    private static let superscript: [Character: Character] = [
        "0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴", "5": "⁵", "6": "⁶", "7": "⁷", "8": "⁸", "9": "⁹",
    ]

    /// The exact form as maths: real minus signs, roots drawn, a power of digits raised (`π^2` → `π²`), a number or
    /// π times π or a root run together (`3*pi` → `3π`), any other `*` a `·`. Nil when it has no `pi` or isn't
    /// maths alone (words, units), so Pretty.answer goes on as before.
    static func answer(_ s: String) -> String? {
        guard s.contains("pi") else { return nil }
        let pi = s.replacingOccurrences(of: "pi", with: "π")
        guard mathOnly.matches(in: pi) else { return nil }
        func sub(_ re: NSRegularExpression, _ t: String, _ with: String) -> String {
            re.stringByReplacingMatches(in: t, range: NSRange(t.startIndex..., in: t), withTemplate: with)
        }
        var t = sub(looseMinus, pi, "$1" + minus)
        t = sub(binaryMinus, t, " " + minus + " ")
        t = sub(timesSymbol, t, "")
        t = sub(shortRoot, t, "√$1").replacingOccurrences(of: "sqrt(", with: "√(")
        t = t.replacingOccurrences(of: "*", with: "·")
        let ns = t as NSString
        var out = t
        for m in digitPower.matches(in: t, range: NSRange(location: 0, length: ns.length)).reversed() {
            let raised = String(ns.substring(with: m.range(at: 1)).compactMap { superscript[$0] })
            out = (out as NSString).replacingCharacters(in: m.range, with: raised)
        }
        return out
    }
}

private extension NSRegularExpression {
    func matches(in s: String) -> Bool { firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) != nil }
}
