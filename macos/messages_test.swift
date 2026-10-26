import Foundation

/// Standalone test for the messages the bar hasn't shown yet. Not part of the app target.
///   swiftc macos/MessageInbox.swift macos/messages_test.swift -o /tmp/qcalc-messages-test && /tmp/qcalc-messages-test
@main
enum MessagesTest {
    static func main() {
        queue()
        asks()
        clean()
        recent()
        print("messages tests passed")
    }

    static func check(_ ok: Bool, _ what: String) {
        if !ok {
            fputs("failed: \(what)\n", stderr)
            exit(1)
        }
    }

    static func queue() {
        var inbox = MessageInbox()
        let a = inbox.add(from: "max", text: "hi", ask: false)
        let b = inbox.add(from: "sam", text: "yo", ask: false)
        check(inbox.items.count == 2 && a?.id != b?.id, "each message is kept with its own id")
        inbox.seen([a!.id])
        check(inbox.items == [b!], "a message the bar showed goes")
        for i in 0..<30 { inbox.add(from: "max", text: "\(i)", ask: false) }
        check(inbox.items.count == MessageInbox.cap, "the oldest make way past the cap")
        check(inbox.items.last?.text == "29", "the newest is kept")
        let rows = try? JSONSerialization.jsonObject(with: Data(inbox.json.utf8)) as? [[String: Any]]
        check(rows?.count == MessageInbox.cap && rows?.last?["from"] as? String == "max", "the page gets them as json")
        inbox.clear()
        check(inbox.items.isEmpty && inbox.json == "[]", "turned off, nothing waits")
    }

    static func asks() {
        var inbox = MessageInbox()
        check(inbox.add(from: "new", text: "first", ask: true) != nil, "someone new asks")
        check(inbox.add(from: "new", text: "second", ask: true) == nil, "a name asking isn't asked about twice")
        check(inbox.items.first?.text == "", "an ask doesn't carry what they said")
        inbox.add(from: "max", text: "hi", ask: false)
        inbox.answered("new", blocked: false)
        check(inbox.items.map(\.from) == ["max"], "answered, the ask goes")
        inbox.answered("max", blocked: true)
        check(inbox.items.isEmpty, "blocked, their messages go too")
    }

    static func clean() {
        check(MessageInbox.cleanText("hi\nmax: no", max: 500) == "hi max: no", "a new line can't start a fake sender's line")
        check(MessageInbox.cleanText("a\u{202E}b\u{0085}c", max: 500) == "a b c", "direction overrides and c1 controls become spaces")
        check(MessageInbox.cleanText(String(repeating: "x", count: 600), max: 500).count == 500, "long text is cut")
    }

    static func recent() {
        let lower: (String) -> String = { $0.count >= 2 ? $0.lowercased() : "" }
        check(MessageInbox.recent(["Zoe", "bob", "zoe", "x", "amy"], max: 2, clean: lower) == ["zoe", "bob"],
              "who was sent to: each name once, the latest first, at most a few")
    }
}
