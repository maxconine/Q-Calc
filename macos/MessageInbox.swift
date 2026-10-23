import Foundation

// what's come in that the bar hasn't shown yet, in memory only. the bar is handed the lot each time it opens
// (or as each comes while it's up) and says which it showed; those go. a notification is only the alert, so a
// message the system never showed (notifications off, focus, a missed banner) still reaches the bar
struct MessageInbox {
    struct Item: Equatable {
        let id: String
        let from: String
        let text: String
        let ask: Bool
    }

    static let cap = 20
    private(set) var items: [Item] = []
    private var next = 1

    // a name already asking isn't queued twice; the oldest make way past the cap
    @discardableResult
    mutating func add(from: String, text: String, ask: Bool) -> Item? {
        if ask, items.contains(where: { $0.ask && $0.from == from }) { return nil }
        let item = Item(id: "n\(next)", from: from, text: ask ? "" : text, ask: ask)
        next += 1
        items.append(item)
        if items.count > Self.cap { items.removeFirst(items.count - Self.cap) }
        return item
    }

    mutating func seen(_ ids: [String]) {
        let gone = Set(ids)
        items.removeAll { gone.contains($0.id) }
    }

    // answered, a name's ask goes; blocked, everything of theirs
    mutating func answered(_ from: String, blocked: Bool) {
        items.removeAll { $0.from == from && ($0.ask || blocked) }
    }

    mutating func clear() {
        items = []
    }

    var json: String {
        let rows: [[String: Any]] = items.map { ["id": $0.id, "from": $0.from, "text": $0.text, "ask": $0.ask] }
        let data = (try? JSONSerialization.data(withJSONObject: rows, options: [])) ?? Data("[]".utf8)
        return String(data: data, encoding: .utf8) ?? "[]"
    }

    // as the relay and the page do: controls and direction overrides become spaces, so no text can pass for
    // another sender's line
    static func cleanText(_ raw: String, max: Int) -> String {
        let text = raw.replacingOccurrences(
            of: "[\\x{0000}-\\x{001f}\\x{007f}-\\x{009f}\\x{200e}\\x{200f}\\x{2028}-\\x{202e}\\x{2066}-\\x{2069}]+",
            with: " ",
            options: .regularExpression
        )
        return String(text.prefix(max)).trimmingCharacters(in: .whitespaces)
    }
}
