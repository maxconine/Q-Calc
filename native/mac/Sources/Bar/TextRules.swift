import Foundation

// Small text rules the bar needs today, each a port of one TS function named beside it. They're app logic, so
// WP-APP-LOGIC moves them into the core; when QCalcCore exposes them these call the engine instead.

private func regex(_ pattern: String, _ options: NSRegularExpression.Options = []) -> NSRegularExpression {
    try! NSRegularExpression(pattern: pattern, options: options)
}

private extension NSRegularExpression {
    func matches(_ s: String) -> Bool {
        firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) != nil
    }

    func groups(_ s: String) -> [String?]? {
        guard let m = firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) else { return nil }
        return (0..<m.numberOfRanges).map { i in Range(m.range(at: i), in: s).map { String(s[$0]) } }
    }
}

enum Pretty {
    static let minus = "−"
    private static let leadNumber = regex(#"^(-?)([$€£¥]?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?(e[+-]?\d+)?(?=\s|$)"#)
    private static let mathOnly = regex(#"^[\d\s+\-*/^().,πe√i·×sqrt]+$"#)
    private static let looseMinus = regex(#"(^|[\s(])-(?=[\s\d(π√sei])"#)

    /// prettyAnswer (src/engine/format.ts): a real minus sign and grouped digits. Display only; copy and insert
    /// keep the plain answer, which re-parses.
    static func answer(_ s: String) -> String {
        if let g = leadNumber.groups(s), let whole = g[0] {
            let sign = g[1] ?? "", cur = g[2] ?? "", int = (g[3] ?? "").replacingOccurrences(of: ",", with: "")
            let frac = g[4] ?? "", exp = (g[5] ?? "").replacingOccurrences(of: "-", with: minus)
            return (sign.isEmpty ? "" : minus) + cur + group(int) + frac + exp + String(s.dropFirst(whole.count))
        }
        if s == "-∞" { return minus + "∞" }
        if mathOnly.matches(s) {
            let replaced = looseMinus.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: "$1" + minus)
            return radicals(replaced)
        }
        return s
    }

    private static func group(_ int: String) -> String {
        guard int.count >= 4 else { return int }
        var out = ""
        for (i, c) in int.reversed().enumerated() {
            if i > 0, i % 3 == 0 { out.append(",") }
            out.append(c)
        }
        return String(out.reversed())
    }

    // the web view draws sqrt(…) as a radical over its argument (Radical.tsx); in plain text a short argument
    // reads as √2, a long one keeps its brackets. The Typst/native math view replaces this (MathText.swift)
    private static let shortRoot = regex(#"sqrt\(([0-9.]+|[A-Za-zπ])\)"#)
    static func radicals(_ s: String) -> String {
        var t = shortRoot.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: "√$1")
        t = t.replacingOccurrences(of: "sqrt(", with: "√(")
        t = t.replacingOccurrences(of: "pi", with: "π")
        return t
    }

    /// formatScientific (src/engine/format.ts) for the Tab cycle.
    static func scientific(_ n: Double, sigFigs: Int) -> String {
        var s = String(format: "%.\(max(0, sigFigs - 1))e", n)
        // %e writes e+05; JS writes e+5 and trims trailing zeros of the mantissa
        if let e = s.firstIndex(of: "e") {
            var mant = String(s[..<e]), exp = String(s[s.index(after: e)...])
            if mant.contains(".") {
                while mant.hasSuffix("0") { mant.removeLast() }
                if mant.hasSuffix(".") { mant.removeLast() }
            }
            let sign = exp.hasPrefix("-") ? "-" : "+"
            exp = String(Int(exp.trimmingCharacters(in: CharacterSet(charactersIn: "+-"))) ?? 0)
            s = "\(mant)e\(sign)\(exp)"
        }
        return s
    }
}

// chain.ts: an operator typed first continues from the last answer
enum Chain {
    private static let op = regex(#"^\s*[*×·/÷^+!]"#)
    private static let convert = regex(#"^\s*(?:in|to)\s+\S"#, .caseInsensitive)
    private static let plainUnsigned = regex(#"^(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$"#)
    private static let inTo = regex(#"^\s*(?:in|to)\b"#, .caseInsensitive)

    static func chains(_ q: String, lastPlain: String?, lastHasUnit: Bool) -> Bool {
        guard let last = lastPlain, !last.isEmpty else { return false }
        if op.matches(q) { return true }
        return lastHasUnit && convert.matches(q)
    }

    static func expr(_ q: String) -> String {
        inTo.matches(q) ? "ans \(q.drop { $0 == " " })" : "ans\(q)"
    }

    /// chainedHistoryExpr: the tape keeps the answer written out
    static func historyExpr(_ q: String, plain: String) -> String {
        let rest = String(q.drop { $0 == " " })
        if inTo.matches(rest) { return "\(plain) \(rest)" }
        let loose = rest.first.map { "+-−".contains($0) } ?? false
        let base = loose || plainUnsigned.matches(plain) ? plain : "(\(plain))"
        return base + rest
    }
}

// what Enter does with a line that isn't maths: the commands QuickCalc.tsx reads before committing
enum BarCommand: Equatable {
    case periodic
    case identities(id: String, section: Int, hint: String)
    case games
    case game(kind: String, name: String)
    case message(to: String, name: String, text: String)
    case help
    case settings
    case tutorial
    case graph(String)
    case greeting

    static let periodicRE = regex(#"^periodic(\s+table)?$"#, .caseInsensitive)
    private static let identityWords = regex(#"^(?:(?:trig(?:onometric|onometry)?|log(?:arithm(?:ic)?)?|exponent|algebra(?:ic)?|math)\s+)?identit(?:y|ies)$"#, .caseInsensitive)
    private static let conversionWords = regex(#"^(?:(?:(?:unit|metric)\s+)?conversions?(?:\s+(?:table|sheet|factors))?|(?:(?:metric|si)\s+)?prefix(?:es)?(?:\s+table)?|metric(?:\s+(?:table|system))?|units?\s+(?:table|sheet))$"#, .caseInsensitive)
    private static let calculusWords = regex(#"^(?:(?:common\s+)?(?:derivatives?|integrals?)|calculus(?:\s+(?:identities|sheet|rules))?|derivative\s+rules)$"#, .caseInsensitive)
    private static let constantsWords = regex(#"^(?:(?:physical|physics|fundamental)\s+)?constants(?:\s+(?:table|sheet))?$|^codata$"#, .caseInsensitive)
    private static let partCommand = regex(#"^(?:identities|identity|idt)\s+(.+)$"#, .caseInsensitive)
    // (pattern, sheet id, section title)
    private static let parts: [(NSRegularExpression, String, String)] = [
        (regex(#"^trig(?:onometry|onometric)?$"#), "identities", "Trig"),
        (regex(#"^(?:logs?|logarithms?|exponents?)$"#), "identities", "Logs and exponents"),
        (regex(#"^algebra$"#), "identities", "Algebra"),
        (regex(#"^(?:integration|integrals?|antiderivatives?)$"#), "calculus", "Integrals"),
        (regex(#"^(?:partial(?: fractions?)?|pf)$"#), "identities", "Partial fractions"),
        (regex(#"^(?:summations?|sums?|series|sigma)$"#), "identities", "Summation"),
        (regex(#"^(?:derivatives?|differentiation)$"#), "calculus", "Derivatives"),
        (regex(#"^(?:(?:physical )?constants?|codata)$"#), "constants", ""),
        (regex(#"^(?:conversions?|units?|prefixes)$"#), "conversions", ""),
    ]

    // games.ts GAMES, with each game's own `is…Command` rule
    static let gameList: [(kind: String, name: String, blurb: String, re: NSRegularExpression)] = [
        ("pong", "pong", "first to 7 · vs the computer or anyone online", regex(#"^pong$"#, .caseInsensitive)),
        ("connect4", "connect 4", "four in a row · vs the computer or anyone online", regex(#"^connect[\s-]*(4|four)$"#, .caseInsensitive)),
        ("ultimate", "ultimate tic-tac-toe", "nine boards in one · vs the computer or anyone online", regex(#"^(ultimate([\s-]*(tic[\s-]*tac[\s-]*toe|ttt))?|uttt)$"#, .caseInsensitive)),
        ("ember", "ember & frost", "co-op puzzle platformer · one keyboard or online", regex(#"^(?:ember(?:(?: ?[&+] ?| and )frost)?|frost(?: ?[&+] ?| and )ember|fire(?:(?: ?[&+] ?| and )| )ice)$"#, .caseInsensitive)),
        ("crack", "crack", "crack the safe · tap when the dial hits the dot", regex(#"^(?:crack(?: the safe)?|safe ?crack(?:er|ing)?)$"#, .caseInsensitive)),
        ("equal", "equal", "guess the hidden equation · daily or practice", regex(#"^(?:equal|nerdle)$"#, .caseInsensitive)),
        ("numbers", "24 / countdown", "make the number · two modes against the clock", regex(#"^(?:24 ?game|game of 24|count ?down|numbers game|24 ?\/ ?countdown)$"#, .caseInsensitive)),
        ("trails", "trails", "leave a trail, don’t hit one · solo, one keyboard or online", regex(#"^(?:trails?|cycles)$"#, .caseInsensitive)),
    ]

    private static let messageRE = regex(#"^\s*message(?:\s+(\S+))?(?:\s+([\s\S]*))?$"#, .caseInsensitive)
    private static let graphRE = regex(#"^\s*(?:graph|plot)\s+(.+)$"#, .caseInsensitive)

    static func of(_ raw: String, sheets: SheetData) -> BarCommand? {
        let t = raw.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
        let lower = t.lowercased()
        if lower.isEmpty { return nil }
        if periodicRE.matches(t) { return .periodic }
        if let target = identityTarget(t, sheets: sheets) { return target }
        if lower == "games" || lower == "game" { return .games }
        if let g = gameList.first(where: { $0.re.matches(t) }) { return .game(kind: g.kind, name: g.name) }
        if let m = messageRE.groups(raw) {
            let to = m[1] ?? nil ?? ""
            return .message(to: to, name: Messages.clean(to), text: String((m[2] ?? nil ?? "").trimmingCharacters(in: .whitespacesAndNewlines).prefix(Messages.textMax)))
        }
        if lower == "help" || lower == "?" { return .help }
        if lower == "settings" || lower == "preferences" { return .settings }
        if lower == "tutorial" { return .tutorial }
        if lower == "hi" || lower == "hello" || lower == "hey" { return .greeting }
        if let g = graphRE.groups(t), let body = g[1] ?? nil { return .graph(body) }
        return nil
    }

    private static func identityTarget(_ t: String, sheets: SheetData) -> BarCommand? {
        func at(_ id: String, _ sectionTitle: String) -> BarCommand? {
            guard let sheet = sheets.sheet(id) else { return nil }
            let section = sectionTitle.isEmpty ? 0 : (sheet.sectionTitles.firstIndex { $0.caseInsensitiveCompare(sectionTitle) == .orderedSame } ?? 0)
            let hint = section > 0 ? "\(sheet.hint): \(sheet.sectionTitles[section].lowercased())" : sheet.hint
            return .identities(id: id, section: section, hint: hint)
        }
        let part = partCommand.groups(t)?[1].flatMap { $0 }?.lowercased()
        if let part, let named = parts.first(where: { $0.0.matches(part) }) { return at(named.1, named.2) }
        if t.lowercased() == "trig" { return at("identities", "Trig") }
        if t.lowercased() == "partial fractions" { return at("identities", "Partial fractions") }
        if identityWords.matches(t) || t.lowercased() == "idt" { return at("identities", "") }
        if calculusWords.matches(t) { return at("calculus", "") }
        if conversionWords.matches(t) { return at("conversions", "") }
        if constantsWords.matches(t) { return at("constants", "") }
        return nil
    }

    /// The hint under the bar while this command is typed.
    func hint(messaging: Messages) -> String {
        switch self {
        case .periodic: return "periodic table"
        case let .identities(_, _, hint): return hint
        case .games: return "↑↓ pick · ↵ play · or type a name"
        case let .game(kind, name): return kind == "crack" ? "↵ crack the safe" : kind == "numbers" ? "↵ play 24 or countdown" : "↵ play \(name)"
        case let .message(to, name, text):
            if messaging.name.isEmpty { return "set your name in settings to send messages · ↵ opens settings" }
            if messaging.link == .paused { return "notifications are off for Q Calc, so messages are paused · ↵ opens settings" }
            if to.isEmpty { return "message <name> <what to say>" }
            if name.isEmpty { return "a name is 2–20 letters, digits, - or _" }
            if text.isEmpty { return "then what to say to \(name)" }
            return "↵ sends to \(name)"
        case .help: return ""
        case .settings: return "↵ opens settings"
        case .tutorial: return "start the tutorial"
        case .graph: return ""
        case .greeting: return ""
        }
    }
}
