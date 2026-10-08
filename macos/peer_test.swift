import CryptoKit
import Foundation
import Network

/// Standalone test for the peer link: field maths, pairing, and two links over loopback. Not part of the app target.
///   swiftc macos/PeerPairing.swift macos/PeerLink.swift macos/peer_test.swift -o /tmp/qcalc-peer-test && /tmp/qcalc-peer-test
@main
enum PeerTest {
    static func main() {
        field()
        elligator()
        pairing()
        cipher()
        loopback()
        print("peer link tests passed")
    }

    static func check(_ ok: Bool, _ what: String) {
        if !ok {
            fputs("failed: \(what)\n", stderr)
            exit(1)
        }
    }

    static func randomField() -> Field25519 {
        Field25519(bytes: (0..<32).map { _ in UInt8.random(in: 0...255) })
    }

    static func field() {
        check(Field25519(7) * Field25519(6) == Field25519(42), "7 * 6")
        check(Field25519(3) - Field25519(5) + Field25519(2) == .zero, "3 - 5 + 2")
        // p - 1 is -1
        let minusOne = Field25519(limbs: [0xFFFF_FFFF_FFFF_FFEC, .max, .max, 0x7FFF_FFFF_FFFF_FFFF])
        check(minusOne == -Field25519.one, "p - 1 is -1")
        check(minusOne * minusOne == .one, "(-1)^2")
        // 2^255 is 19
        check(Field25519(limbs: [0, 0, 0, 0x8000_0000_0000_0000]) == Field25519(19), "2^255 = 19")
        check(Field25519(limbs: [.max, .max, .max, .max]) == Field25519(37), "2^256 - 1 = 37")
        for _ in 0..<50 {
            let x = randomField()
            let y = randomField()
            if x == .zero { continue }
            check(x * x.inverse == .one, "x * 1/x")
            check((x + y) - y == x, "x + y - y")
            check(x * (y + .one) == x * y + x, "distributive")
            check(Field25519(bytes: x.bytes) == x, "bytes round trip")
        }
        check(Field25519.zero.inverse == .zero, "1/0 is 0")
        // p is 1 mod 4, so -1 is a square; 2 is not, which is why elligator uses it
        check(minusOne.isSquare, "-1 square")
        check(!Field25519(2).isSquare, "2 not square")
        check(Field25519(4).isSquare, "4 square")
        check((Field25519(9) * Field25519(2)).isSquare == false, "18 not square")
    }

    static func elligator() {
        for _ in 0..<50 {
            let u = Elligator25519.map(randomField())
            check(Elligator25519.curveRight(u).isSquare, "elligator lands on the curve")
        }
        check(Elligator25519.curveRight(Elligator25519.map(.zero)).isSquare, "elligator of 0")
        let sid = Data(repeating: 7, count: 16)
        check(PeerHandshake.generator(code: "1234", sid: sid) == PeerHandshake.generator(code: "1234", sid: sid), "generator is stable")
        check(PeerHandshake.generator(code: "1234", sid: sid) != PeerHandshake.generator(code: "1235", sid: sid), "generator follows the code")
    }

    // runs the four frames between two handshakes; nil keys mean a side refused
    static func run(host: String, guest: String) -> (host: PeerKeys?, guest: PeerKeys?, hostGuessed: Bool) {
        let h = PeerHandshake(role: .host, code: host)
        let g = PeerHandshake(role: .guest, code: guest)
        guard case .send(let share) = g.receive(h.start()) else { return (nil, nil, false) }
        guard case .send(let reply) = h.receive(share) else { return (nil, nil, h.guessed) }
        guard case .paired(let gk, let confirm?) = g.receive(reply) else { return (nil, nil, h.guessed) }
        guard case .paired(let hk, nil) = h.receive(confirm) else { return (nil, gk, h.guessed) }
        return (hk, gk, h.guessed)
    }

    static func same(_ a: SymmetricKey, _ b: SymmetricKey) -> Bool {
        a.withUnsafeBytes { Data($0) } == b.withUnsafeBytes { Data($0) }
    }

    static func pairing() {
        let good = run(host: "0420", guest: "0420")
        guard let hk = good.host, let gk = good.guest else {
            check(false, "same code pairs")
            return
        }
        check(same(hk.send, gk.receive) && same(gk.send, hk.receive), "keys agree")
        check(!same(hk.send, hk.receive), "each direction has its own key")
        let bad = run(host: "0420", guest: "0421")
        check(bad.host == nil && bad.guest == nil, "wrong code does not pair")
        check(bad.hostGuessed, "a wrong code counts as a try")
        // guest side refuses the host's confirmation, not only the host
        let h = PeerHandshake(role: .host, code: "1111")
        let g = PeerHandshake(role: .guest, code: "2222")
        guard case .send(let share) = g.receive(h.start()), case .send(let reply) = h.receive(share) else {
            check(false, "handshake frames")
            return
        }
        guard case .wrongCode = g.receive(reply) else {
            check(false, "guest sees a wrong code")
            return
        }
        // out of order or malformed frames are refused
        let fresh = PeerHandshake(role: .host, code: "1234")
        guard case .invalid = fresh.receive(Data([PeerHandshake.guestShare]) + Data(count: 32)) else {
            check(false, "share before hello")
            return
        }
        let g2 = PeerHandshake(role: .guest, code: "1234")
        guard case .invalid = g2.receive(Data([PeerHandshake.hello]) + Data("QCP2".utf8) + Data(count: 16)) else {
            check(false, "bad magic")
            return
        }
        guard case .invalid = PeerHandshake(role: .guest, code: "12a4").receive(PeerHandshake(role: .host, code: "12a4").start()) else {
            check(false, "code must be 4 digits")
            return
        }
        check(PeerHandshake.isCode("0007") && !PeerHandshake.isCode("123") && !PeerHandshake.isCode("١٢٣٤"), "isCode")
        for _ in 0..<20 { check(PeerHandshake.isCode(PeerHandshake.newCode()), "newCode is 4 digits") }
    }

    static func cipher() {
        let r = run(host: "5555", guest: "5555")
        guard let a = r.host, let b = r.guest else { return check(false, "pair for cipher") }
        let host = PeerCipher(a)
        let guest = PeerCipher(b)
        for text in ["hello", "", String(repeating: "x", count: 5000)] {
            guard let sealed = host.seal(Data(text.utf8)), let plain = guest.open(sealed) else { return check(false, "seal/open") }
            check(String(decoding: plain, as: UTF8.self) == text, "round trip")
        }
        guard var tampered = guest.seal(Data("paddle".utf8)) else { return check(false, "seal") }
        tampered[3] ^= 1
        check(host.open(tampered) == nil, "tampered frame refused")
        // a replayed frame fails because the counter moved on
        let host2 = PeerCipher(a)
        let guest2 = PeerCipher(b)
        guard let first = guest2.seal(Data("first".utf8)) else { return check(false, "seal") }
        check(host2.open(first) != nil, "first opens")
        check(host2.open(first) == nil, "replay refused")
    }

    static func loopback() {
        var hostEvents: [[String: Any]] = []
        var guestEvents: [[String: Any]] = []
        let host = PeerLink(name: "host mac", advertise: false)
        let guest = PeerLink(name: "guest mac", advertise: false)
        host.onEvent = { hostEvents.append($0) }
        guest.onEvent = { guestEvents.append($0) }

        func wait(_ what: String, seconds: Double = 5, until done: () -> Bool) {
            let end = Date().addingTimeInterval(seconds)
            while !done() {
                if Date() > end { check(false, "timed out waiting for \(what); host \(hostEvents) guest \(guestEvents)") }
                RunLoop.main.run(until: Date().addingTimeInterval(0.01))
            }
        }
        func last(_ events: [[String: Any]], _ type: String) -> [String: Any]? {
            events.last { $0["type"] as? String == type }
        }

        host.host()
        wait("hosting") { last(hostEvents, "hosting") != nil }
        guard let port = host.localPort, var code = last(hostEvents, "hosting")?["code"] as? String else {
            return check(false, "host port and code")
        }
        let endpoint = NWEndpoint.hostPort(host: "127.0.0.1", port: port)
        let wrong = code == "0000" ? "0001" : "0000"

        // something on the network that isn't q calc: a frame that says it's 4gb, then plain junk.
        // each is dropped, costs no try at the code, and the host keeps hosting
        for junk in [Data([0xFF, 0xFF, 0xFF, 0xFF]), Data([0, 0, 0, 3, 0x02, 0x00, 0x00])] {
            let raw = NWConnection(to: endpoint, using: .tcp)
            var rawDone = false
            raw.stateUpdateHandler = { state in
                switch state {
                case .ready: raw.send(content: junk, completion: .contentProcessed { _ in })
                case .failed, .cancelled: rawDone = true
                default: break
                }
            }
            raw.start(queue: .main)
            func drain() {
                raw.receive(minimumIncompleteLength: 1, maximumLength: 4096) { _, _, isComplete, error in
                    if isComplete || error != nil { rawDone = true } else { drain() }
                }
            }
            drain()
            wait("junk dropped") { rawDone }
            raw.cancel()
        }
        check(last(hostEvents, "closed") == nil, "junk doesn't stop the host")
        check(last(hostEvents, "hosting")?["code"] as? String == code, "junk doesn't spend a try at the code")

        // three wrong codes: each is refused, and the third makes the host show a new code
        for i in 1...PeerLink.triesPerCode {
            guestEvents.removeAll()
            guest.connect(to: endpoint, code: wrong)
            wait("wrong code \(i)") { last(guestEvents, "closed") != nil }
            check(last(guestEvents, "closed")?["reason"] as? String == "wrong-code", "wrong code \(i) is refused")
        }
        wait("renewed code") { last(hostEvents, "hosting")?["renewed"] as? Bool == true }
        guard let renewed = last(hostEvents, "hosting")?["code"] as? String else { return check(false, "renewed code") }
        code = renewed

        guestEvents.removeAll()
        guest.connect(to: endpoint, code: code)
        wait("open") { last(hostEvents, "open") != nil && last(guestEvents, "open") != nil }
        check(last(hostEvents, "open")?["peer"] as? String == "guest mac", "host sees the guest's name")
        check(last(guestEvents, "open")?["role"] as? String == "guest", "guest role")
        check(last(hostEvents, "open")?["role"] as? String == "host", "host role")

        // a third mac can't cut in once paired
        var otherEvents: [[String: Any]] = []
        let other = PeerLink(name: "other", advertise: false)
        other.onEvent = { otherEvents.append($0) }
        other.connect(to: endpoint, code: code)
        wait("other refused") { last(otherEvents, "closed") != nil }

        guest.send("{\"t\":\"p\",\"y\":140}")
        host.send("héllo ⌘")
        wait("messages") { last(hostEvents, "message") != nil && last(guestEvents, "message") != nil }
        check(last(hostEvents, "message")?["data"] as? String == "{\"t\":\"p\",\"y\":140}", "guest to host")
        check(last(guestEvents, "message")?["data"] as? String == "héllo ⌘", "host to guest")

        // too big for the other side to take: not sent, and the link carries on
        hostEvents.removeAll()
        guest.send(String(repeating: "x", count: PeerLink.maxFrame))
        guest.send("after")
        wait("message after the big one") { last(hostEvents, "message") != nil }
        check(last(hostEvents, "message")?["data"] as? String == "after", "an oversized send is dropped, not the link")
        check(last(hostEvents, "closed") == nil, "the link survives an oversized send")
        let biggest = String(repeating: "y", count: PeerLink.maxFrame - PeerLink.sealOverhead)
        guest.send(biggest)
        wait("largest frame") { (last(hostEvents, "message")?["data"] as? String)?.count == biggest.count }

        guest.close()
        wait("bye") { last(hostEvents, "closed") != nil }
        check(last(hostEvents, "closed")?["reason"] as? String == "bye", "host hears bye")
        check(last(guestEvents, "closed") == nil, "the side that leaves gets no closed event")

        // a fresh host: one that connects and says nothing gives the slot back after a few seconds
        hostEvents.removeAll()
        host.host()
        wait("hosting again") { last(hostEvents, "hosting") != nil }
        guard let port2 = host.localPort, let code2 = last(hostEvents, "hosting")?["code"] as? String else {
            return check(false, "second host port and code")
        }
        let endpoint2 = NWEndpoint.hostPort(host: "127.0.0.1", port: port2)
        let silent = NWConnection(to: endpoint2, using: .tcp)
        var silentGone = false
        // reads what the host sends (its half of the handshake) and never answers
        func drainSilent() {
            silent.receive(minimumIncompleteLength: 1, maximumLength: 4096) { _, _, isComplete, error in
                if isComplete || error != nil { silentGone = true } else { drainSilent() }
            }
        }
        silent.stateUpdateHandler = { state in
            if case .ready = state { drainSilent() }
        }
        silent.start(queue: .main)
        let quietStart = Date()
        wait("silent guest dropped", seconds: PeerLink.quietSeconds + 3) { silentGone }
        check(Date().timeIntervalSince(quietStart) < PeerLink.handshakeSeconds, "a silent guest is dropped before the full handshake time")
        silent.cancel()
        check(last(hostEvents, "closed") == nil, "a silent guest doesn't stop the host")

        // wrong codes wear out: after enough of them in all the host stops hosting
        let wrong2 = code2 == "0000" ? "0001" : "0000"
        for i in 1...PeerLink.triesBeforeStop {
            guestEvents.removeAll()
            let codeNow = last(hostEvents, "hosting")?["code"] as? String ?? code2
            guest.connect(to: endpoint2, code: codeNow == wrong2 ? (wrong2 == "0000" ? "0002" : "0000") : wrong2)
            wait("wrong code \(i) of \(PeerLink.triesBeforeStop)") { last(guestEvents, "closed") != nil }
        }
        wait("host stops") { last(hostEvents, "closed") != nil }
        check(last(hostEvents, "closed")?["reason"] as? String == "too-many-tries", "the host stops after too many wrong codes")
        host.close()
    }
}
