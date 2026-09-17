import CoreServices
import Foundation

/// Apple Dictionary lookups are currently commented out of the app.
/// Restore by uncommenting DictionaryLookup usage in Overlay.swift,
/// adding this file back to macos/build.sh, and restoring wantDefinition in nativeEval.ts.
enum DictionaryLookup {
    struct Query: Equatable {
        var term: String
        var forced: Bool
    }

    struct Found: Equatable {
        var term: String
        var display: String
        var body: String
        var partOfSpeech: String?
        var pronunciation: String?

        var shortLabel: String {
            if let partOfSpeech, !partOfSpeech.isEmpty { return partOfSpeech }
            let line = body.split(whereSeparator: \.isNewline).first.map(String.init) ?? display
            if line.count <= 48 { return line }
            return String(line.prefix(45)).trimmingCharacters(in: .whitespaces) + "…"
        }
    }

    private static let prefixPatterns: [NSRegularExpression] = {
        let patterns = [
            #"^define[:\s]+(.+)$"#,
            #"^definition of\s+(.+)$"#,
            #"^meaning of\s+(.+)$"#,
            #"^what does\s+(.+?)\s+mean\??$"#,
        ]
        return patterns.map { try! NSRegularExpression(pattern: $0, options: [.caseInsensitive]) }
    }()

    private static let wordPattern = try! NSRegularExpression(
        pattern: #"^\p{L}[\p{L}'’\-]*$"#,
        options: []
    )

    private static let partsOfSpeech = [
        "combining form",
        "possessive adjective",
        "ordinal number",
        "adjective",
        "adverb",
        "pronoun",
        "preposition",
        "conjunction",
        "exclamation",
        "interjection",
        "contraction",
        "determiner",
        "abbreviation",
        "noun",
        "verb",
        "prefix",
        "suffix",
        "symbol",
        "number",
    ]

    /// Pull a dictionary term out of overlay input. `forced` is true for `define …` style queries.
    static func query(from expr: String) -> Query? {
        let src = expr.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !src.isEmpty else { return nil }
        let ns = src as NSString
        let full = NSRange(location: 0, length: ns.length)
        for re in prefixPatterns {
            if let match = re.firstMatch(in: src, options: [], range: full),
               match.range(at: 1).location != NSNotFound {
                let term = ns.substring(with: match.range(at: 1))
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if !term.isEmpty { return Query(term: term, forced: true) }
            }
        }
        if isSingleWord(src) { return Query(term: src, forced: false) }
        return nil
    }

    static func define(_ term: String) -> Found? {
        let src = term.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !src.isEmpty else { return nil }
        if let raw = copyDefinition(src) {
            return parse(raw, fallbackTerm: src)
        }
        let tokens = src.split { $0.isWhitespace || $0 == "," }
        guard tokens.count > 1, let last = tokens.last else { return nil }
        let word = String(last).trimmingCharacters(in: CharacterSet(charactersIn: "?!.,;:'\""))
        guard word.count >= 2, word.caseInsensitiveCompare(src) != .orderedSame else { return nil }
        return define(word)
    }

    static func isSingleWord(_ src: String) -> Bool {
        let ns = src as NSString
        let full = NSRange(location: 0, length: ns.length)
        guard wordPattern.firstMatch(in: src, options: [], range: full) != nil else { return false }
        let letters = src.unicodeScalars.filter { CharacterSet.letters.contains($0) }.count
        return letters >= 2
    }

    private static func copyDefinition(_ src: String) -> String? {
        let cf = src as CFString
        let range = CFRange(location: 0, length: CFStringGetLength(cf))
        guard let unmanaged = DCSCopyTextDefinition(nil, cf, range) else { return nil }
        let text = unmanaged.takeRetainedValue() as String
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    static func parse(_ raw: String, fallbackTerm: String) -> Found {
        let parts = raw.split(separator: "|", maxSplits: 2, omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        var term = fallbackTerm
        var pronunciation: String?
        var rest = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if parts.count >= 3 {
            if let headword = parts[0].split(whereSeparator: { $0.isWhitespace }).first {
                term = String(headword)
            }
            if !parts[1].isEmpty { pronunciation = parts[1] }
            rest = parts[2]
        }
        let pos = detectPartOfSpeech(in: rest)
        var body = rest
        if let pos, body.lowercased().hasPrefix(pos) {
            body = String(body.dropFirst(pos.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        body = pretty(body)
        var lines: [String] = [term]
        if let pronunciation { lines.append(pronunciation) }
        lines.append("")
        if let pos { lines.append(pos) }
        if !body.isEmpty { lines.append(body) }
        let display = lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        return Found(
            term: term,
            display: display,
            body: body,
            partOfSpeech: pos,
            pronunciation: pronunciation
        )
    }

    private static func detectPartOfSpeech(in text: String) -> String? {
        let head = String(text.prefix(160)).lowercased()
        var best: (pos: String, idx: String.Index)?
        for pos in partsOfSpeech {
            guard let range = head.range(of: pos) else { continue }
            if best == nil || range.lowerBound < best!.idx {
                best = (pos, range.lowerBound)
            }
        }
        return best?.pos
    }

    private static func pretty(_ text: String) -> String {
        var s = text
        for marker in ["ORIGIN", "PHRASES", "DERIVATIVES", "USAGE"] {
            s = s.replacingOccurrences(of: " \(marker)", with: "\n\n\(marker)")
        }
        s = s.replacingOccurrences(of: " • ", with: "\n• ")
        return s.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
