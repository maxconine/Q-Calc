import Foundation

/// A sheet's search (IdentityWindow.swift's filter), pure so it is tested alone: native/mac/scripts/test-sheet-search.sh
/// runs native/mac/Tests/sheetsearch/queries.tsv through it.
///
/// Each row is indexed once: its texts folded (case, accents, widths; − as -) and cut into words, each word also
/// squeezed to letters and digits (`x^{2}` is `x2`, `τ_{max}` is `τmax`) and, with Greek in it, in Latin letters
/// (`tmax`) and spelled out (`tau`, `taumax`), ∇ ∂ Σ ∫ √ by name too, all kept as ids into one vocabulary. Synonyms
/// (`ke` and `kinetic energy`) are added to a row's words where it has one of them. A query word is matched against
/// the vocabulary once (cached while it is typed on), as
///   exact 10 · same stem 9 (`forces` is force, `rotating` rotation) · the start of a word 6 (typing, `deriv`) ·
///   a stem's start 5 · inside a word 3 (`magnet`) · one letter off 2 · two off in a long word 1 (`intergral`)
/// (a synonym's a point less) times where the row has it: its name (note, tags) 4, its formula and explanation's
/// title 3, its sheet's, section's or group's title 2, its explanation's letters and related formulas 1. A row's
/// score is the sum over the query's words, plus a bonus when the words come together in its own text (`moment of
/// inertia`), when the query is a clause of its note or title (`average` is "mean, average") or ends or starts one
/// (`flux` is "heat flux", `ideal gas` "ideal gas law"), and when it is what the row's formula defines (`Pr`, `tau max`,
/// `tanh` for tanh(z) = …, `del f` for ∇f, ζ typed as itself). A row matches when it has every
/// word but the little ones (`of`, `the`); when none does, every word but one ("closest matches"). A formula typed with
/// spaces (`P = V I`) is first looked for whole in a row's formula; when some row has it, only those rows match.
final class SheetSearch {
    private struct Row {
        // token ids in each of `weights`' places, a word in the first place it is
        var fields: [[Int32]]
        // and the synonyms of its words, in the same place, a point under the word itself
        var synonyms: [[Int32]]
        // its formula's letters and digits (and in Latin letters), for a formula typed whole
        var formula: [[UInt8]]
        // the words of its first two places joined by single spaces (after one), for a phrase's bonus
        var ownWords: [UInt8]
        // its note's and explanation title's clauses (cut at , ; : · and brackets), each its words joined by single
        // spaces, for a query that is a whole clause (`gradient` is ∇f's, `average` the mean's "mean, average"), its
        // last words (`flux` is "heat flux", `modulus` "bulk modulus") or its first (`centripetal` is "centripetal force")
        var clauses: [String]
        // what its formula defines, the letters left of its `=` (`pr` for Pr = μ c_p/k), for a symbol typed alone or
        // spelled in parts
        var defines: [String]
        // its note's length in words (its explanation title's without one): of two rows with a clause a query is,
        // the shorter is more about it (`average` is "mean, average" before "laminar flat plate, average")
        var noteLength: Int
        // everything folded, for a query word that is only symbols (`∫`, `±`)
        var hay: String
    }

    /// what a word counts for in each place: a row's name (note, tags, the line it opens), its formula and
    /// explanation's title, its sheet's, section's and group's titles, its explanation's letters and related formulas
    private static let weights = [4, 3, 2, 1]

    struct Result {
        // each row's score, 0 where it doesn't match
        var scores: [Int]
        // no row had every word: these have all but one
        var closest: Bool
    }

    private var rows: [Row] = []
    private var vocab: [String] = []
    private var bytes: [[UInt8]] = []
    private var ids: [String: Int32] = [:]
    private var stems: [String: [Int32]] = [:]
    private var textCache: [String: [Int32]] = [:]
    private var wordCache: [String: [UInt8]] = [:]

    /// a row's texts in each of `weights`' places, its formula alone, and its names (note, explanation title) for clauses
    private func index(_ places: [[String]], formula: [String], names: [String]) {
        let folded = places.map { $0.map(self.folded) }
        let fields = places.map { Set($0.flatMap(tokens)) }
        var synonyms = places.map { _ in Set<Int32>() }
        // synonyms where the row has one
        for (n, texts) in folded.enumerated() {
            let words = Self.words(texts.joined(separator: " "))
            let have = Set(words)
            let padded = " " + words.joined(separator: " ") + " "
            for group in Self.synonymWords where group.contains(where: { phrase in
                phrase.allSatisfy(have.contains) && (phrase.count == 1 || padded.contains(" " + phrase.joined(separator: " ") + " "))
            }) {
                synonyms[n].formUnion(group.joined().map(id))
            }
        }
        var seen = Set<Int32>()
        let unique = fields.map { field -> [Int32] in
            let out = field.subtracting(seen).sorted()
            seen.formUnion(field)
            return out
        }
        let similar = synonyms.enumerated().map { $0.element.subtracting(fields[$0.offset]).subtracting(seen).sorted() }
        let squeezed = formula.map { Self.compact(Self.fold($0)) }
        let formulaKeys = squeezed + squeezed.map(latin).filter { !squeezed.contains($0) }
        let ownWords = Array((" " + Self.words(folded.prefix(2).joined().joined(separator: " ")).joined(separator: " ")).utf8)
        var clauses: [String] = []
        for name in names {
            // an aside in brackets isn't a clause of its own ("Mean (hydrostatic) pressure" is "mean pressure")
            let f = self.folded(name)
            let bare = f.contains("(") ? f.replacingOccurrences(of: "\\s*\\([^()]*\\)", with: "", options: .regularExpression) : f
            for clause in bare.split(whereSeparator: { ",;:·—–".contains($0) }) {
                let w = Self.words(String(clause)).joined(separator: " ")
                if !w.isEmpty, !clauses.contains(w) { clauses.append(w) }
            }
        }
        var defines: [String] = []
        for f in formula {
            guard let eq = f.firstIndex(of: "="), eq != f.startIndex else { continue }
            var side = Self.fold(String(f[..<eq])).trimmingCharacters(in: .whitespaces)
            // a function defined by its argument is its name (`tanh(z) = …` is tanh)
            if side.hasSuffix(")"), let open = side.firstIndex(of: "("), open != side.startIndex, side[..<open].allSatisfy(\.isLetter) {
                side = String(side[..<open])
            }
            let left = Self.compact(side)
            var keys: [String] = []
            // a lone letter (F, β) is on the left of too many rows to say much, unless it's Greek typed as itself (ζ;
            // `beta` is still a word)
            if left.count == 1, left.unicodeScalars.allSatisfy({ Self.greekNames[$0] != nil }) { keys.append(left) }
            if (2...6).contains(left.count) { keys += [left, Self.compact(latin(left)), Self.spelled(left)] }
            // ∇f is `del f` and `nabla f` too (only a symbol before letters: not ∇ × F)
            for (symbol, names) in Self.symbolNames where side.unicodeScalars.contains(symbol) {
                let rest = side.replacingOccurrences(of: String(Character(symbol)), with: "")
                guard !rest.isEmpty, rest.allSatisfy(\.isLetter) else { continue }
                keys += names.map { $0 + rest }
            }
            for key in keys where !key.isEmpty && !defines.contains(key) { defines.append(key) }
        }
        rows.append(Row(fields: unique, synonyms: similar, formula: formulaKeys.map { Array($0.utf8) }, ownWords: ownWords, clauses: clauses,
                        defines: defines, noteLength: names.lazy.map { Self.words(self.folded($0)).count }.first { $0 > 0 } ?? 99, hay: folded.joined().joined(separator: " ")))
        wordCache = [:]
    }

    /// a sheet row's texts, as IdentityWindow.swift and the test both have them
    struct RowText {
        var lead: [String] = []
        var show = ""
        var copy = ""
        var note = ""
        var leadCopy: [String] = []
        var parts: [String] = []
        var open = ""
        // words it is found by but doesn't show (the generator's `tags`)
        var tags = ""
        var explainTitle = ""
        // the explanation's letters (symbol, meaning, unit) and related formulas (formula, when)
        var vars: [[String]] = []
        var related: [[String]] = []
    }

    /// a row under its sheet's, section's and group's titles
    func add(_ r: RowText, titles: [String]) {
        var more = r.related.map { $0.joined(separator: " ") }
        for v in r.vars {
            more.append(v.dropFirst().joined(separator: " "))
            if v.count > 2, let name = Self.unitNames[v[2].trimmingCharacters(in: .whitespaces)] { more.append(name) }
        }
        let formula = r.lead + [r.show, r.copy] + r.leadCopy + r.parts
        index([[r.note, r.tags, r.open], formula + [r.explainTitle], titles, more], formula: formula, names: [r.note, r.explainTitle])
    }

    // the SI units the explanations give, by name and by what they measure, so a word finds the symbol (`ohm` finds
    // rows in Ω, `newton` and `force` rows in N)
    private static let unitNames: [String: String] = [
        "Ω": "ohm ohms resistance resistor", "S": "siemens conductance", "V": "volt volts voltage potential emf",
        "A": "amp amps ampere current", "W": "watt watts power", "J": "joule joules energy work heat",
        "N": "newton newtons force", "N·m": "newton meter torque moment", "Pa": "pascal pressure stress",
        "C": "coulomb charge", "F": "farad capacitance capacitor", "H": "henry inductance inductor",
        "T": "tesla magnetic field flux density", "Wb": "weber magnetic flux", "Hz": "hertz frequency",
        "rad/s": "angular frequency velocity", "kg": "kilogram mass", "m": "meter metre length distance",
        "s": "second seconds time", "K": "kelvin temperature", "°C": "celsius temperature", "mol": "mole moles amount",
        "m/s": "velocity speed", "m/s^{2}": "acceleration", "kg·m/s": "momentum impulse", "m^{2}": "area",
        "m^{3}": "volume", "kg/m^{3}": "density", "W/(m·K)": "thermal conductivity", "N/m": "spring constant stiffness",
        "kg·m^{2}": "moment of inertia", "m^{4}": "second moment of area", "rad": "radian angle", "lm": "lumen", "cd": "candela",
        "V/m": "electric field", "A/m": "magnetic field strength", "Pa·s": "viscosity", "m^{3}/s": "flow rate",
    ]

    /// what a row on the sheet that's open adds to its score, so a tie (`modulus` on complex numbers) goes its way
    static let openSheetBonus = 10

    // the little words a query may leave out of a row
    private static let little: Set<String> = ["of", "the", "a", "an", "for", "in", "and", "to", "on", "with", "from", "by", "is", "at"]

    func search(_ query: String) -> Result {
        let words = Self.fold(query).split(whereSeparator: \.isWhitespace).map(String.init)
        var result = Result(scores: Array(repeating: 0, count: rows.count), closest: false)
        guard !words.isEmpty else { return result }
        // a formula typed with spaces (`P = V I`): its words alone match nearly every row (`=`, `v` and `i` are everywhere)
        // (only with a symbol in it: `z transform` and `a cos b sin` are words, found by name)
        let formulaLike = words.count > 1 && query.contains { !($0.isLetter || $0.isNumber || $0.isWhitespace || $0 == "'" || $0 == "\u{2019}") }
        let phrase = formulaLike ? Self.compact(words.joined()) : ""
        if phrase.count > 1 {
            let p = Array(phrase.utf8)
            for (n, row) in rows.enumerated() where row.formula.contains(where: { Self.contains($0, p) }) {
                result.scores[n] = 100 + (row.formula.contains(p) ? 20 : 0)
            }
            if result.scores.contains(where: { $0 > 0 }) { return result }
        }
        let needed = words.filter { !Self.little.contains($0) }
        let need = needed.isEmpty ? words.count : needed.count
        let wordTiers = words.map(tiers)
        let asked = Self.words(words.joined(separator: " ")).joined(separator: " ")
        let askedLength = asked.split(separator: " ").count
        // a symbol typed alone, or spelled in parts (`tau max`, `omega n`)
        let symbol = words.count <= 2 ? Self.compact(words.joined()) : ""
        let together = words.count > 1 ? Array((" " + Self.words(words.joined(separator: " ")).joined(separator: " ")).utf8) : []
        // each row's count of the query's words it has (the little ones counted only when there's nothing else)
        var wordsHad = Array(repeating: 0, count: rows.count)
        for (n, row) in rows.enumerated() {
            var score = 0, have = 0, inOwn = 0
            for (w, word) in words.enumerated() {
                let t = wordTiers[w]
                var best = 0
                if t.isEmpty {
                    // only symbols: anywhere in the row
                    if row.hay.contains(word) { best = 3 * Self.points[6] }
                } else {
                    var own = false
                    for (f, field) in row.fields.enumerated() {
                        let tier = Self.best(t, field)
                        let similar = Self.best(t, row.synonyms[f])
                        if tier > 0 || similar > 0, f < 2 { own = true }
                        best = max(best, Self.weights[f] * Self.points[tier], Self.weights[f] * (Self.points[similar] - 1))
                    }
                    if own { inOwn += 1 }
                }
                if best > 0 {
                    score += best
                    if needed.isEmpty || !Self.little.contains(word) { have += 1 }
                }
            }
            if score == 0 { continue }
            if inOwn == words.count { score += 6 }
            if !together.isEmpty, Self.contains(row.ownWords, together) { score += 15 }
            if !asked.isEmpty {
                if row.clauses.contains(asked) {
                    score += 12 + max(0, 3 - (row.noteLength - askedLength))
                } else if row.clauses.contains(where: { $0.hasSuffix(" " + asked) || $0.hasPrefix(asked + " ") }) {
                    score += 5
                }
            }
            if !symbol.isEmpty, row.defines.contains(symbol) { score += 25 }
            wordsHad[n] = have
            result.scores[n] = score
        }
        if wordsHad.contains(where: { $0 >= need }) {
            for n in rows.indices where wordsHad[n] < need { result.scores[n] = 0 }
        } else if need > 1, wordsHad.contains(where: { $0 >= need - 1 }) {
            result.closest = true
            for n in rows.indices where wordsHad[n] < need - 1 { result.scores[n] = 0 }
        } else {
            for n in rows.indices { result.scores[n] = 0 }
        }
        return result
    }

    /// the rows a search matched, best first: by score (and `bonus`, the open sheet's), then in the order they were
    /// added (a sheet's own order: its usual form first). A shorter note isn't put first: that lost more than it won
    func ranked(_ result: Result, bonus: (Int) -> Int = { _ in 0 }) -> [Int] {
        var keyed: [(n: Int, score: Int)] = []
        for n in result.scores.indices where result.scores[n] > 0 { keyed.append((n, result.scores[n] + bonus(n))) }
        keyed.sort { a, b in a.score != b.score ? a.score > b.score : a.n < b.n }
        return keyed.map(\.n)
    }

    // a tier's points: none, two letters off, one letter off, inside a word, a stem's start, a word's start, the
    // same stem, exact
    private static let points = [0, 1, 2, 3, 5, 6, 9, 10]

    private static func best(_ tiers: [UInt8], _ ids: [Int32]) -> Int {
        var best: UInt8 = 0
        for id in ids {
            let t = tiers[Int(id)]
            if t > best { best = t; if best == 7 { break } }
        }
        return Int(best)
    }

    /// each vocabulary word's tier for one query word (empty for a word that is only symbols)
    private func tiers(_ word: String) -> [UInt8] {
        if let cached = wordCache[word] { return cached }
        let key = Self.compact(word)
        guard !key.isEmpty else { return [] }
        var out = [UInt8](repeating: 0, count: vocab.count)
        let k = Array(key.utf8)
        let stem = Self.stem(key)
        let s = Array(stem.utf8)
        // a word's start, inside one, a stem's start
        for (n, v) in bytes.enumerated() {
            if v.count >= k.count, k.count >= 2, v.starts(with: k) {
                out[n] = 5
            } else if s.count >= 4, v.count > s.count, v.starts(with: s) {
                out[n] = 4
            } else if k.count >= 3, v.count > k.count, Self.contains(v, k) {
                out[n] = 3
            }
        }
        // a longer query word whose start is a word's stem (`integrals`, `ohms`)
        if k.count >= 4 {
            for (n, v) in bytes.enumerated() where out[n] < 4 && v.count >= 3 && v.count < k.count && k.count - v.count <= 3 && k.starts(with: v) {
                let vs = Array(Self.stem(vocab[n]).utf8)
                if vs.count >= 3 && k.starts(with: vs) && Self.endings.contains(String(decoding: k[vs.count...], as: UTF8.self)) { out[n] = 4 }
            }
        }
        // one letter off, or two in a long word, for words that are letters (not `x2`); one off over two
        // (`intergral` is integral before internal, `momentem` momentum before moment)
        if k.count >= 4, key.allSatisfy(\.isLetter) {
            let most = k.count >= 8 ? 2 : 1
            for (n, v) in bytes.enumerated() where out[n] == 0 && abs(v.count - k.count) <= most && v.first == k.first {
                let d = Self.distance(k, v, most: most)
                if d <= most { out[n] = d == 1 ? 2 : 1 }
            }
        }
        if stem.count >= 3, let same = stems[stem] { for id in same where out[Int(id)] < 6 { out[Int(id)] = 6 } }
        if let exact = ids[key] { out[Int(exact)] = 7 }
        // a long session types thousands of words, each a vocabulary's worth of bytes
        if wordCache.count >= 256 { wordCache = [:] }
        wordCache[word] = out
        return out
    }

    // MARK: text

    private func id(_ token: String) -> Int32 {
        if let id = ids[token] { return id }
        let id = Int32(vocab.count)
        vocab.append(token)
        bytes.append(Array(token.utf8))
        ids[token] = id
        let stem = Self.stem(token)
        if stem.count >= 3 { stems[stem, default: []].append(id) }
        return id
    }

    /// a text's words, each also squeezed to its letters and digits and in Latin letters where it has Greek
    private func tokens(_ text: String) -> [Int32] {
        if let cached = textCache[text] { return cached }
        let folded = self.folded(text)
        var found = Set<String>(Self.words(folded))
        for chunk in folded.split(whereSeparator: \.isWhitespace) {
            let c = Self.compact(String(chunk))
            if !c.isEmpty { found.insert(c) }
        }
        // a size written 2 × 2 is also 2x2
        if folded.contains("×") {
            for m in Self.matrixSize.matches(in: folded, range: NSRange(folded.startIndex..., in: folded)) {
                if let a = Range(m.range(at: 1), in: folded), let b = Range(m.range(at: 2), in: folded) { found.insert("\(folded[a])x\(folded[b])") }
            }
        }
        for t in found where t.unicodeScalars.contains(where: { $0.value > 0x7f }) {
            let l = Self.compact(latin(t))
            if !l.isEmpty { found.insert(l) }
        }
        // and with Greek spelled out, as it's typed on a keyboard: τ_max is `tau` `max` and `taumax`, ω_n `omegan`
        for t in found where t.unicodeScalars.contains(where: { $0.value >= 0xB5 && Self.greekNames[$0] != nil }) { found.insert(Self.spelled(t)) }
        // and the symbols with names of their own
        for c in text.unicodeScalars where c.value > 0x7f { if let names = Self.symbolNames[c] { found.formUnion(names) } }
        let out = found.map(id)
        textCache[text] = out
        return out
    }

    /// Greek letters by name: `ωn` is `omegan`
    private static func spelled(_ text: String) -> String {
        var out = ""
        for c in text.unicodeScalars { if let name = greekNames[c] { out += name } else { out.unicodeScalars.append(c) } }
        return out
    }

    private static let greekNames: [Unicode.Scalar: String] = {
        var out: [Unicode.Scalar: String] = [:]
        let names = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda", "mu", "nu", "xi",
                     "omicron", "pi", "rho", "sigma", "sigma", "tau", "upsilon", "phi", "chi", "psi", "omega"]
        // α (U+03B1) to ω (U+03C9), ς the final sigma among them
        for (n, name) in names.enumerated() { out[Unicode.Scalar(0x3B1 + UInt32(n))!] = name }
        for (c, name) in [("\u{3D1}", "theta"), ("\u{3D5}", "phi"), ("\u{3F5}", "epsilon"), ("\u{B5}", "mu"), ("\u{3D6}", "pi")] { out[Unicode.Scalar(c)!] = name }
        return out
    }()

    private static let symbolNames: [Unicode.Scalar: [String]] = [
        "∇": ["nabla", "del"], "∂": ["partial"], "Σ": ["sum"], "∑": ["sum"], "∏": ["product"], "∫": ["integral"], "√": ["sqrt"],
    ]

    private static let matrixSize = try! NSRegularExpression(pattern: "(\\d+) ?× ?(\\d+)")

    private static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive], locale: nil)
            .replacingOccurrences(of: "\u{2212}", with: "-")
    }

    // a sheet repeats its titles and letters on row after row: each folded once
    private var foldCache: [String: String] = [:]

    private func folded(_ text: String) -> String {
        if let cached = foldCache[text] { return cached }
        let out = Self.fold(text)
        foldCache[text] = out
        return out
    }

    /// letters and digits alone, so `tmax` finds τ_max's `t_max` and `x2` finds x^{2}
    private static func compact(_ text: String) -> String {
        String(String.UnicodeScalarView(text.unicodeScalars.filter { CharacterSet.alphanumerics.contains($0) }))
    }

    /// the words of folded text: runs of letters and digits
    /// (an apostrophe inside a word is dropped: `ohm's` is `ohms`)
    private static func words(_ folded: String) -> [String] {
        var out: [String] = []
        var word = String.UnicodeScalarView()
        for c in folded.unicodeScalars {
            if CharacterSet.alphanumerics.contains(c) {
                word.append(c)
            } else if (c == "'" || c == "\u{2019}") && !word.isEmpty {
                continue
            } else if !word.isEmpty {
                out.append(String(word))
                word = String.UnicodeScalarView()
            }
        }
        if !word.isEmpty { out.append(String(word)) }
        return out
    }

    private var latinCache: [String: String] = [:]

    /// greek in latin letters (ICU's Greek-Latin), folded again: τ_max is t_max, ω is o, Δ is d
    private func latin(_ text: String) -> String {
        if let cached = latinCache[text] { return cached }
        let out = Self.fold(text.applyingTransform(.toLatin, reverse: false) ?? text)
        latinCache[text] = out
        return out
    }

    /// a light English stem: `forces` and `force` are `forc`, `rotating` and `rotation` `rotat`, `integrals` `integral`
    private static func stem(_ word: String) -> String {
        guard word.count > 3, word.allSatisfy(\.isLetter) else { return word }
        var w = word
        if w.hasSuffix("'s") { w.removeLast(2) }
        for (end, with) in suffixes where w.hasSuffix(end) && w.count - end.count >= 3 {
            w.removeLast(end.count)
            w += with
            break
        }
        if w.hasSuffix("e"), w.count > 3 { w.removeLast() }
        return w
    }

    // what a longer query word may end with past a word's stem (`integrals`, `ohms`, `rotational`; not `momentem`)
    private static let endings: Set<String> = [
        "s", "es", "e", "ed", "er", "ers", "ing", "ings", "ly", "al", "ally", "ic", "ical", "ion", "ions", "ation", "ations", "ational",
        "ment", "ments", "ness", "ity", "ive", "ian", "y", "ies", "ial", "ous",
    ]

    private static let suffixes: [(String, String)] = [
        ("izations", "iz"), ("ization", "iz"), ("ational", "at"), ("ations", "at"), ("ation", "at"), ("ically", "ic"),
        ("ings", ""), ("ing", ""), ("ions", ""), ("ion", ""), ("ies", "y"), ("ied", "y"), ("ness", ""), ("ments", ""),
        ("ment", ""), ("ally", "al"), ("ers", ""), ("ed", ""), ("es", ""), ("ly", ""), ("s", ""),
    ]

    private static func contains(_ hay: [UInt8], _ needle: [UInt8]) -> Bool {
        guard needle.count <= hay.count, let first = needle.first else { return needle.isEmpty }
        var i = 0
        let last = hay.count - needle.count
        while i <= last {
            if hay[i] == first {
                var j = 1
                while j < needle.count && hay[i + j] == needle[j] { j += 1 }
                if j == needle.count { return true }
            }
            i += 1
        }
        return false
    }

    /// edits (with a swap of two letters as one) between two words, or more than `most` once it's sure to be
    private static func distance(_ a: [UInt8], _ b: [UInt8], most: Int) -> Int {
        let n = a.count, m = b.count
        var before = [Int](repeating: 0, count: m + 1)
        var previous = Array(0...m)
        var current = [Int](repeating: 0, count: m + 1)
        for i in 1...n {
            current[0] = i
            var low = i
            for j in 1...m {
                let cost = a[i - 1] == b[j - 1] ? 0 : 1
                var d = min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + cost)
                if i > 1, j > 1, a[i - 1] == b[j - 2], a[i - 2] == b[j - 1] { d = min(d, before[j - 2] + 1) }
                current[j] = d
                low = min(low, d)
            }
            if low > most { return most + 1 }
            (before, previous, current) = (previous, current, before)
        }
        return previous[m]
    }

    // MARK: synonyms

    private static let synonymWords: [[[String]]] = synonyms.map { $0.map { $0.split(separator: " ").map(String.init) } }

    /// words that mean the same, any one finding a row with another (each a phrase of folded words)
    private static let synonyms: [[String]] = [
        ["ke", "kinetic energy"], ["pe", "potential energy"], ["moi", "moment of inertia", "mass moment of inertia"],
        ["parallel axis", "shift theorem", "steiner"], ["ohm", "ohms law"], ["sohcahtoa", "soh cah toa"],
        ["derivative", "differentiate", "differentiation"], ["integral", "integrate", "integration", "antiderivative"],
        ["std", "stdev", "standard deviation", "sd"], ["var", "variance"], ["avg", "average", "mean"],
        ["kvl", "kirchhoffs voltage law"], ["kcl", "kirchhoffs current law"],
        ["emf", "electromotive force"], ["eom", "equations of motion", "equation of motion"], ["fbd", "free body diagram"],
        ["shm", "simple harmonic"], ["ode", "differential equation"], ["pde", "partial differential equation"],
        ["sho", "harmonic oscillator"], ["mass centre", "center of mass", "centre of mass", "mass center", "com"],
        ["torque", "moment of a force"], ["fft", "dft", "discrete fourier transform"], ["npv", "net present value"],
        ["log", "logarithm"], ["ln", "natural log"], ["sqrt", "square root"], ["temp", "temperature"],
        ["vel", "velocity"], ["accel", "acceleration"], ["freq", "frequency"],
        ["cos law", "law of cosines"], ["sin law", "law of sines"], ["hyp", "hypotenuse"],
        ["lhopital", "l hopital", "l hospital"], ["eigen", "eigenvalue", "eigenvector"], ["det", "determinant"],
        ["pmf", "probability mass"], ["pdf", "probability density"], ["cdf", "cumulative distribution"],
        ["rms", "root mean square"], ["sse", "sum of squares"], ["mse", "mean squared error"],
    ]
}
