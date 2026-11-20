// Headless checks for the message cards' rules (native/mac/scripts/test-cards.sh): which cards stay past three,
// what VoiceOver reads for a card and a file, and the Copy flash. Foundation only: no app, no relay.

import Foundation

var failures = 0

func check(_ what: String, _ ok: Bool) {
    print(ok ? "ok   \(what)" : "FAIL \(what)")
    if !ok { failures += 1 }
}

struct Card: Equatable {
    let id: String
    var held = false
}

func ids(_ cards: [Card]) -> [String] { cards.map(\.id) }

// ---- three at most ----

let plain = [Card(id: "d"), Card(id: "c"), Card(id: "b"), Card(id: "a")]
check("three or fewer stay as they are", ids(CardRules.kept(Array(plain.prefix(3))) { $0.held }) == ["d", "c", "b"])
check("past three the oldest goes", ids(CardRules.kept(plain) { $0.held }) == ["d", "c", "b"])
let answering = [Card(id: "e"), Card(id: "d"), Card(id: "c"), Card(id: "b"), Card(id: "a", held: true)]
check("a card being answered stays when three new ones come", ids(CardRules.kept(answering) { $0.held }) == ["e", "d", "a"])
check("the order is kept", ids(CardRules.kept([Card(id: "c"), Card(id: "b", held: true), Card(id: "a"), Card(id: "z")]) { $0.held }) == ["c", "b", "a"])
let allHeld = (0..<4).map { Card(id: "\($0)", held: true) }
check("past three answered cards, none is dropped", CardRules.kept(allHeld) { $0.held }.count == 4)

// ---- VoiceOver ----

check("a request", CardRules.label(from: "max", ask: true, clip: false, file: nil, text: "") == "max wants to message you")
check("a clip reads as announced", CardRules.label(from: "max", ask: false, clip: true, file: nil, text: "x = 3…") == "max shared their clipboard")
check("a file reads as announced", CardRules.label(from: "max", ask: false, clip: false, file: "a.pdf", text: "a.pdf") == "max sent a.pdf")
check("a file with no name", CardRules.label(from: "max", ask: false, clip: false, file: "", text: "") == "max sent a file")
check("words", CardRules.label(from: "max", ask: false, clip: false, file: nil, text: "hi") == "Message from max: hi")
check("a file with nothing to say ends at its size",
      CardRules.fileLabel(mine: false, who: "max", name: "a.pdf", size: "2 KB", note: "") == "max’s file a.pdf, 2 KB")
check("a file's state follows",
      CardRules.fileLabel(mine: true, who: "max", name: "a.pdf", size: "2 KB", note: "sent") == "Your file a.pdf, 2 KB. sent")

// ---- a file that never went goes back on the line ----

let one = CardRules.putBack("a.pdf", to: "max", line: "", waiting: [String]())
check("an unsent drop waits on an empty line again", one?.line == "send file to max" && one?.waiting == ["a.pdf"])
let two = CardRules.putBack("b.pdf", to: "max", line: "send file to max", waiting: ["a.pdf"])
check("a second unsent file joins the first", two?.waiting == ["a.pdf", "b.pdf"])
check("the same file isn't put back twice", CardRules.putBack("a.pdf", to: "max", line: "send file to max", waiting: ["a.pdf"])?.waiting == ["a.pdf"])
check("a line typed on since is left alone", CardRules.putBack("a.pdf", to: "max", line: "2+3", waiting: [String]()) == nil)
check("a line for someone else is left alone", CardRules.putBack("a.pdf", to: "max", line: "send file to sam", waiting: ["c.pdf"]) == nil)

// ---- Copied ----

var flash = FlashTicket()
let first = flash.start()
let second = flash.start()
check("a second click's flash isn't cut short by the first's timer", !flash.current(first) && flash.current(second))

if failures > 0 {
    print("\(failures) failed")
    exit(1)
}
print("all card checks passed")
