import Foundation

/// Which sheet row an explanation's Related formula is (pure, so it is tested alone: macos/sheet_match_test.swift).
/// Cells are written as in `show` (`x^{2}`, `y_{n}`); a row's runs give it back with `markup`. Tiers, best first:
/// 0. the same characters, leaving out spaces, brackets, ^ _ and the way minus, times and ½ are written, either way round
/// 1. a row that chains equalities with the formula's sides in order (`P = VI = I^{2}R` is `P = V I = I^{2} R = V^{2}/R`)
/// 2. the same, loosely: a sum's or integral's limits left out (`Σ x[n] e^{−jωn}` is `Σ_{n=−∞}^{∞} x[n] e^{−jωn}`), `…`
///    standing for anything (`ln(1 + x) = x − x^{2}/2 + …`), a formula that goes on past the row (`BW = ω_{0}/Q = 1/(RC)`
///    is `BW = ω_{0}/Q`), and a transform pair across a row's cells (`1 ↔ 2πδ(ω)` is the row `1 ‖ 2πδ(ω)`)
/// 3. a right side that is a whole cell of a table row (`I = (1/2) m r^{2}` is the cell `(1/2) m r^{2}`), or an
///    expression alone that is a row's left side
/// 4. the same right side under another short name (`U = (1/2) C V^{2}` is `E = (1/2) C V^{2}`)
enum SheetMatch {
    static func key(_ text: String) -> String {
        var out = String.UnicodeScalarView()
        for c in text.lowercased().unicodeScalars {
            switch c {
            case " ", "\t", "{", "}", "(", ")", "^", "_", "*", "·", "×", "⋅": continue
            case "\u{2212}": out.append("-")
            case "\u{2215}": out.append("/")
            case "½": out.append(contentsOf: "1/2".unicodeScalars)
            case "⅓": out.append(contentsOf: "1/3".unicodeScalars)
            case "⅔": out.append(contentsOf: "2/3".unicodeScalars)
            case "¼": out.append(contentsOf: "1/4".unicodeScalars)
            case "¾": out.append(contentsOf: "3/4".unicodeScalars)
            default: out.append(c)
            }
        }
        return String(out)
    }

    /// a row's runs written back as `show` writes them, so its sides and limits can be told apart
    static func markup(_ runs: [(text: String, shift: String)]) -> String {
        runs.map { run in
            switch run.shift {
            case "sup": return "^{\(run.text)}"
            case "sub": return "_{\(run.text)}"
            case "supsub": return "^{_{\(run.text)}}"
            default: return run.text
            }
        }.joined()
    }

    static func same(_ formula: String, cell: String) -> Bool {
        Formula(formula)?.tier([cell]) != nil
    }

    static func tier(_ formula: String, row cells: [String]) -> Int? {
        Formula(formula)?.tier(cells)
    }

    /// a Related formula, read once and held against each row
    struct Formula {
        let text: String
        let want: String
        let flipped: String?
        let keyed: [String]
        let looseSides: [String]
        let halves: [String]

        init?(_ formula: String) {
            text = formula
            want = SheetMatch.key(formula)
            guard !want.isEmpty else { return nil }
            let sides = split(formula, "=")
            flipped = sides.count == 2 ? SheetMatch.key(sides[1] + "=" + sides[0]) : nil
            keyed = sides.map(SheetMatch.key)
            looseSides = sides.map(loose)
            halves = formula.contains("↔") ? split(formula, "↔").map(loose) : []
        }

        /// how well this is one of the row's cells (lead cells then the formula), lower is better; nil when it isn't
        func tier(_ cells: [String]) -> Int? {
            let keys = cells.map(SheetMatch.key)
            if keys.contains(where: { $0 == want || $0 == flipped }) { return 0 }
            guard !keyed.contains(where: \.isEmpty) else { return nil }
            let rows = cells.map { split($0, "=") }
            if keyed.count >= 2, rows.contains(where: { $0.count > 1 && chained(keyed, $0.map(SheetMatch.key), loose: false) }) { return 1 }
            let looseRows = rows.map { $0.map(loose) }
            if looseRows.contains(where: { chained(looseSides, $0, loose: true) }) { return 2 }
            if pair(halves, looseRows) { return 2 }
            // a table row's cell with no `=` (a moment of inertia, a beam's deflection, a Laplace pair's side)
            if keyed.count >= 2, looseRows.contains(where: { row in
                row.count == 1 && row[0].count >= 4 && row[0].contains(where: \.isLetter) && alike(looseSides[looseSides.count - 1], row[0])
            }) { return 3 }
            // an expression alone that is the left side of a row (`(a⃗ × b⃗) · (c⃗ × d⃗)`)
            if keyed.count == 1, want.unicodeScalars.count >= 5, want.contains(where: \.isLetter),
               looseRows.contains(where: { $0.count >= 2 && $0[0] == looseSides[0] }) { return 3 }
            // the same right side under another short name
            if looseSides.count == 2, looseSides[0].count <= 4, looseSides[1].count >= 6, looseRows.contains(where: { row in
                row.count == 2 && row[0].count <= 4 && alike(looseSides[1], row[1])
            }) { return 4 }
            return nil
        }
    }

    // `f(t) ↔ F(s)` as two cells of the row, in order
    private static func pair(_ halves: [String], _ cells: [[String]]) -> Bool {
        guard halves.count == 2, !halves.contains(where: \.isEmpty), cells.count >= 2 else { return false }
        let row = cells.map { $0.joined(separator: "=") }
        guard let at = row.firstIndex(where: { alike(halves[0], $0) }) else { return false }
        return row[(at + 1)...].contains { alike(halves[1], $0) }
    }

    // the formula's sides against a row's: the same sides (either way round for two), the formula's in order among a
    // longer row's, or, loosely, a shorter row's (two or more sides, the same left side) in order among the formula's
    private static func chained(_ part: [String], _ whole: [String], loose: Bool) -> Bool {
        let eq: (String, String) -> Bool = loose ? alike : { $0 == $1 }
        if part.count == whole.count {
            if zip(part, whole).allSatisfy(eq) { return true }
            return part.count == 2 && eq(part[0], whole[1]) && eq(part[1], whole[0])
        }
        if part.count >= 2, whole.count > part.count {
            return inOrder(part, whole, eq) || inOrder(part.reversed(), whole, eq)
        }
        if loose, whole.count >= 2, part.count > whole.count, eq(part[0], whole[0]) {
            return inOrder(whole, part, eq)
        }
        return false
    }

    // every one of `part`, in this order, among `whole`
    private static func inOrder(_ part: [String], _ whole: [String], _ eq: (String, String) -> Bool) -> Bool {
        var rest = whole[...]
        for side in part {
            guard let at = rest.firstIndex(where: { eq(side, $0) }) else { return false }
            rest = rest[(at + 1)...]
        }
        return true
    }

    // a key with a big operator's limits left out (`Σ_{k=0}^{n}`, `∫_{0}^{t}`); `…` is kept for `alike`
    static func loose(_ text: String) -> String {
        var out = ""
        var chars = Array(text)[...]
        while let c = chars.first {
            chars = chars.dropFirst()
            out.append(c)
            guard "Σ∑∏∫∮".contains(c) else { continue }
            while chars.count >= 2, "_^".contains(chars.first!), chars.dropFirst().first == "{" {
                var depth = 0
                chars = chars.dropFirst()
                while let d = chars.first {
                    chars = chars.dropFirst()
                    if d == "{" { depth += 1 }
                    if d == "}" { depth -= 1; if depth == 0 { break } }
                }
            }
        }
        return key(out)
    }

    // two loose keys the same, a `…` in the first standing for anything (at least three characters written around it)
    static func alike(_ a: String, _ b: String) -> Bool {
        if a == b { return true }
        guard a.contains("…") else { return false }
        let pieces = a.components(separatedBy: "…")
        guard pieces.joined().count >= 3, b.hasPrefix(pieces[0]), b.hasSuffix(pieces.last!),
              b.count >= pieces[0].count + pieces.last!.count else { return false }
        var rest = b.dropFirst(pieces[0].count).dropLast(pieces.last!.count)
        for piece in pieces.dropFirst().dropLast() where !piece.isEmpty {
            guard let found = rest.range(of: piece) else { return false }
            rest = rest[found.upperBound...]
        }
        return true
    }

    // split at `separator` outside braces (`Σ_{n=0}` holds no side)
    static func split(_ text: String, _ separator: Character) -> [String] {
        var parts = [""]
        var depth = 0
        for c in text {
            if c == "{" { depth += 1 } else if c == "}" { depth = max(0, depth - 1) }
            if c == separator && depth == 0 { parts.append("") } else { parts[parts.count - 1].append(c) }
        }
        return parts
    }
}
