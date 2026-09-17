import Foundation

/// Standalone smoke test for DictionaryLookup. Not part of the app target.
/// Apple Dictionary integration is currently commented out of the app; this file is unchanged.
///   swiftc macos/DictionaryLookup.swift macos/dictionary_test.swift -o /tmp/qcalc-dict-test && /tmp/qcalc-dict-test
@main
enum DictionaryTest {
    static func main() {
        expectQuery("ingenious", term: "ingenious", forced: false)
        expectQuery("serendipity", term: "serendipity", forced: false)
        expectQuery("  well-being ", term: "well-being", forced: false)
        expectQuery("it's", term: "it's", forced: false)
        expectQuery("define serendipity", term: "serendipity", forced: true)
        expectQuery("Define:  apple", term: "apple", forced: true)
        expectQuery("definition of New York", term: "New York", forced: true)
        expectQuery("meaning of well-being", term: "well-being", forced: true)
        expectQuery("what does serendipity mean", term: "serendipity", forced: true)
        expectQuery("What does pi mean?", term: "pi", forced: true)
        expectNilQuery("")
        expectNilQuery("2+2")
        expectNilQuery("sin(90)")
        expectNilQuery("Big Apple")
        expectNilQuery("a")
        expectNilQuery("definite integral")

        guard let clever = DictionaryLookup.define("ingenious") else {
            fputs("missing definition for ingenious\n", stderr)
            exit(1)
        }
        if clever.term.lowercased() != "ingenious" {
            fputs("term \(clever.term)\n", stderr)
            exit(1)
        }
        if clever.partOfSpeech != "adjective" {
            fputs("pos \(clever.partOfSpeech ?? "nil")\n", stderr)
            exit(1)
        }

        guard let found = DictionaryLookup.define("serendipity") else {
            fputs("missing definition for serendipity\n", stderr)
            exit(1)
        }
        if found.term.lowercased() != "serendipity" {
            fputs("term \(found.term)\n", stderr)
            exit(1)
        }
        if found.partOfSpeech != "noun" {
            fputs("pos \(found.partOfSpeech ?? "nil")\n", stderr)
            exit(1)
        }
        if !found.body.localizedCaseInsensitiveContains("occurrence") {
            fputs("body missing occurrence: \(found.body)\n", stderr)
            exit(1)
        }
        guard let phrase = DictionaryLookup.define("Big Apple") else {
            fputs("missing definition for Big Apple\n", stderr)
            exit(1)
        }
        if !phrase.body.localizedCaseInsensitiveContains("new york") {
            fputs("Big Apple body: \(phrase.body)\n", stderr)
            exit(1)
        }
        print("swift dictionary lookup passed")
    }

    static func expectQuery(_ expr: String, term: String, forced: Bool) {
        guard let q = DictionaryLookup.query(from: expr) else {
            fputs("nil query for \(expr)\n", stderr)
            exit(1)
        }
        if q.term != term || q.forced != forced {
            fputs("\(expr) => \(q.term) forced=\(q.forced), expected \(term) forced=\(forced)\n", stderr)
            exit(1)
        }
    }

    static func expectNilQuery(_ expr: String) {
        if let q = DictionaryLookup.query(from: expr) {
            fputs("expected nil query for \(expr), got \(q.term)\n", stderr)
            exit(1)
        }
    }
}
