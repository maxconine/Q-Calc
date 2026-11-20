import Foundation

/// The message cards' rules, kept free of the app so `scripts/test-cards.sh` can check them headless.
enum CardRules {
    /// The most cards under the bar at once (messageBanner.ts keeps three).
    static let max = 3

    /// The cards kept when more than `max` are up, newest first as given. A card being answered (its reply line has
    /// the keys, words typed in it, a reply on its way) is never the one dropped: three new messages
    /// arriving while a reply was typed had taken the card, the draft and the keyboard focus with it. The others go
    /// oldest first; past `max` answered cards, all of them stay.
    static func kept<T>(_ cards: [T], max: Int = CardRules.max, held: (T) -> Bool) -> [T] {
        guard cards.count > max else { return cards }
        let heldFlags = cards.map(held)
        var room = Swift.max(0, max - heldFlags.filter { $0 }.count)
        var out: [T] = []
        for (card, h) in zip(cards, heldFlags) {
            if h {
                out.append(card)
            } else if room > 0 {
                out.append(card)
                room -= 1
            }
        }
        return out
    }

    /// What VoiceOver reads for a card: the words it was announced with (`max shared their clipboard`,
    /// `max sent a.pdf`), not `Message from max:` and a clip's or file's summary.
    static func label(from: String, ask: Bool, clip: Bool, file: String?, text: String) -> String {
        if ask { return "\(from) wants to message you" }
        if clip { return "\(from) shared their clipboard" }
        if let file { return file.isEmpty ? "\(from) sent a file" : "\(from) sent \(file)" }
        return "Message from \(from): \(text)"
    }

    /// A file's VoiceOver label in a card or a thread: whose, its name and size, then where it's got to when that's
    /// said (it ended `2 KB. ` with nothing after).
    static func fileLabel(mine: Bool, who: String, name: String, size: String, note: String) -> String {
        let head = "\(mine ? "Your" : "\(who)’s") file \(name), \(size)"
        return note.isEmpty ? head : "\(head). \(note)"
    }

    /// A dropped file whose send never started (no relay, no key for them) goes back on the line as
    /// `send file to <name>` with any others put back already, so ↵ sends them again; nil when the line holds
    /// something else by then (typed on since), which is left alone.
    static func putBack<T: Equatable>(_ file: T, to name: String, line: String, waiting: [T]) -> (line: String, waiting: [T])? {
        putBack([file], to: name, line: line, waiting: waiting)
    }

    /// The same for a drop sent from a `message <name> words` line (`typed`, QA item 80): when its words weren't
    /// sent (`wordsUnsent`, any status but delivered, as a plain `message` line keeps them) the whole line comes
    /// back, with the files that didn't go waiting on it, so ↵ sends both again; words that went leave
    /// `send file to <name>` for the files alone. `files` may be empty (only the words failed). Either failure can
    /// come first: a `send file to <name>` line already put back grows into the words line, and a words line
    /// already back takes the files.
    static func putBack<T: Equatable>(_ files: [T], to name: String, typed: String = "", wordsUnsent: Bool = false,
                                      line: String, waiting: [T]) -> (line: String, waiting: [T])? {
        let fileLine = "send file to \(name)"
        let back = wordsUnsent && !typed.isEmpty ? typed : fileLine
        var merged = waiting
        for f in files where !merged.contains(f) { merged.append(f) }
        if line.trimmingCharacters(in: .whitespaces).isEmpty { return (back, files) }
        if line == fileLine { return (back, merged) }
        if !typed.isEmpty, line == typed { return (typed, merged) }
        return nil
    }
}

/// The `Copied` flash on a Copy button: each click restarts its 1.2 s, where the first click's timer had cut a
/// second click's flash short.
struct FlashTicket {
    private(set) var count = 0
    mutating func start() -> Int {
        count += 1
        return count
    }
    func current(_ ticket: Int) -> Bool { ticket == count }
}
