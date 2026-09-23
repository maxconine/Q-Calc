import Foundation

enum MathEval {
    // adds the grey inferred parens so `5+3)*2` evaluates as `(5+3)*2`
    static func fillParens(_ expr: String) -> String {
        var depth = 0
        var minDepth = 0
        for ch in expr {
            if ch == "(" {
                depth += 1
            } else if ch == ")" {
                depth -= 1
                if depth < minDepth { minDepth = depth }
            }
        }
        let leading = -minDepth
        let trailing = depth + leading
        if leading == 0 && trailing == 0 { return expr }
        return String(repeating: "(", count: leading) + expr + String(repeating: ")", count: trailing)
    }

    static func evaluate(_ raw: String) -> Double? {
        var src = fillParens(raw.trimmingCharacters(in: .whitespacesAndNewlines))
        src = src.replacingOccurrences(of: "π", with: "pi")
        src = src.replacingOccurrences(of: "\\bpi\\b", with: "(\(Double.pi))", options: [.regularExpression, .caseInsensitive])
        guard src.contains(where: \.isNumber) else { return nil }
        src = src
            .replacingOccurrences(of: "×", with: "*")
            .replacingOccurrences(of: "÷", with: "/")
            .replacingOccurrences(of: "−", with: "-")
            .replacingOccurrences(of: ",", with: "")
            .replacingOccurrences(of: "**", with: "^")
        var p = Parser(src)
        guard let v = p.parseExpression() else { return nil }
        p.skip()
        return p.i >= p.s.count ? v : nil
    }

    static func format(_ n: Double) -> String {
        if n == 0 { return "0" }
        if abs(n) > 0 && abs(n) < 1e-6 { return String(format: "%.4e", n) }
        if abs(n) >= 1e12 { return String(format: "%.4e", n) }
        let f = NumberFormatter()
        f.numberStyle = .decimal
        f.maximumFractionDigits = 10
        f.minimumFractionDigits = 0
        f.usesGroupingSeparator = true
        return f.string(from: NSNumber(value: n)) ?? String(n)
    }
}

private struct Parser {
    let s: [Character]
    var i = 0

    init(_ text: String) {
        s = Array(text)
    }

    mutating func skip() {
        while i < s.count && s[i].isWhitespace { i += 1 }
    }

    mutating func parseExpression() -> Double? {
        guard var left = parseTerm() else { return nil }
        while true {
            skip()
            guard i < s.count else { return left }
            let op = s[i]
            if op != "+" && op != "-" { return left }
            i += 1
            guard let right = parseTerm() else { return nil }
            left = op == "+" ? left + right : left - right
        }
    }

    mutating func parseTerm() -> Double? {
        guard var left = parsePower() else { return nil }
        while true {
            skip()
            guard i < s.count else { return left }
            let op = s[i]
            if op != "*" && op != "/" { return left }
            i += 1
            guard let right = parsePower() else { return nil }
            if op == "/" {
                if right == 0 { return nil }
                left /= right
            } else {
                left *= right
            }
        }
    }

    mutating func parsePower() -> Double? {
        guard let base = parseUnary() else { return nil }
        skip()
        if i < s.count && s[i] == "^" {
            i += 1
            guard let exp = parseUnary() else { return nil }
            return pow(base, exp)
        }
        return base
    }

    mutating func parseUnary() -> Double? {
        skip()
        if i < s.count && s[i] == "+" {
            i += 1
            return parseUnary()
        }
        if i < s.count && s[i] == "-" {
            i += 1
            guard let v = parseUnary() else { return nil }
            return -v
        }
        return parsePrimary()
    }

    mutating func parsePrimary() -> Double? {
        skip()
        if i < s.count && s[i] == "(" {
            i += 1
            guard let v = parseExpression() else { return nil }
            skip()
            if i < s.count && s[i] == ")" { i += 1 }
            return v
        }
        return parseNumber()
    }

    mutating func parseNumber() -> Double? {
        skip()
        let start = i
        var sawDigit = false
        while i < s.count && s[i].isNumber {
            sawDigit = true
            i += 1
        }
        if i < s.count && s[i] == "." {
            i += 1
            while i < s.count && s[i].isNumber {
                sawDigit = true
                i += 1
            }
        }
        if i < s.count && (s[i] == "e" || s[i] == "E") {
            let ePos = i
            i += 1
            if i < s.count && (s[i] == "+" || s[i] == "-") { i += 1 }
            let expStart = i
            while i < s.count && s[i].isNumber { i += 1 }
            if i == expStart {
                i = ePos
            }
        }
        guard sawDigit else { return nil }
        return Double(String(s[start..<i]))
    }
}
