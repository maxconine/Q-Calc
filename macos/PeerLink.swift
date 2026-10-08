import Foundation
import Network
import SystemConfiguration

// a small same-wi-fi link between two q calcs: bonjour to find a host, its 4-digit code to pair (PeerHandshake),
// then sealed text frames both ways. no server, nothing leaves the network. all of it runs on the main queue,
// and events go out through onEvent as plain dictionaries for the page:
//   peers {peers: [{id, name}]} · denied · hosting {code, renewed?} · pairing · open {role, peer} · message {data} · closed {reason}
final class PeerLink {
    static let serviceType = "_qcalc._tcp"
    static let maxFrame = 64 * 1024
    // the sealed marker, the kind byte and the poly1305 tag around a frame's body
    static let sealOverhead = 1 + 1 + 16
    // wrong codes a host takes before it shows a new one
    static let triesPerCode = 3
    // wrong codes a host takes in all before it stops hosting, so the code can't be worn down by guessing
    static let triesBeforeStop = 30
    static let handshakeSeconds = 10.0
    // how long a guest that says nothing holds the host's one slot
    static let quietSeconds = 3.0
    // kDNSServiceErr_PolicyDenied: local network access is off for the app
    private static let policyDenied: Int32 = -65570

    private static let name: UInt8 = 0x6E // n
    private static let data: UInt8 = 0x64 // d
    private static let bye: UInt8 = 0x62 // b

    var onEvent: (([String: Any]) -> Void)?

    private let nameOverride: String?
    // read on first use; the computer name is what bonjour and the other bar show
    private lazy var localName = String((nameOverride ?? Self.computerName()).prefix(63))
    // off for the loopback test: no bonjour, and the host listens on 127.0.0.1 only
    private let advertise: Bool
    private var listener: NWListener?
    private var browser: NWBrowser?
    private var connection: NWConnection?
    private var handshake: PeerHandshake?
    private var cipher: PeerCipher?
    private var role: PeerHandshake.Role?
    private var code = ""
    private var failures = 0
    private var wrongTotal = 0
    // the guest in the slot has sent its first frame
    private var heardGuest = false
    private var opened = false
    private var found: [String: NWEndpoint] = [:]
    private var ownService: String?
    private var timeout: DispatchWorkItem?

    init(name: String? = nil, advertise: Bool = true) {
        nameOverride = name
        self.advertise = advertise
    }

    static func computerName() -> String {
        (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? "a Mac"
    }

    var localPort: NWEndpoint.Port? { listener?.port }

    private static func parameters() -> NWParameters {
        let tcp = NWProtocolTCP.Options()
        tcp.noDelay = true
        tcp.enableKeepalive = true
        tcp.keepaliveIdle = 3
        tcp.keepaliveInterval = 1
        tcp.keepaliveCount = 3
        let params = NWParameters(tls: nil, tcp: tcp)
        // same wi-fi only, not awdl
        params.includePeerToPeer = false
        return params
    }

    private static func denied(_ error: NWError) -> Bool {
        if case .dns(let code) = error { return code == policyDenied }
        return false
    }

    private func emit(_ event: [String: Any]) {
        onEvent?(event)
    }

    // MARK: discovery

    func discover() {
        browser?.cancel()
        let b = NWBrowser(for: .bonjour(type: Self.serviceType, domain: nil), using: Self.parameters())
        b.browseResultsChangedHandler = { [weak self, weak b] results, _ in
            guard let self, let b, b === self.browser else { return }
            self.publish(results)
        }
        b.stateUpdateHandler = { [weak self, weak b] state in
            guard let self, let b, b === self.browser else { return }
            switch state {
            case .waiting(let error), .failed(let error):
                if Self.denied(error) { self.emit(["type": "denied"]) }
            default:
                break
            }
        }
        browser = b
        b.start(queue: .main)
    }

    func stopDiscovery() {
        browser?.cancel()
        browser = nil
    }

    private func publish(_ results: Set<NWBrowser.Result>) {
        var next: [String: NWEndpoint] = [:]
        for result in results {
            guard case .service(let name, _, _, _) = result.endpoint, name != ownService else { continue }
            next[name] = result.endpoint
        }
        found = next
        let peers = next.keys.sorted { $0.localizedStandardCompare($1) == .orderedAscending }.map { ["id": $0, "name": $0] }
        emit(["type": "peers", "peers": peers])
    }

    // MARK: hosting

    func host() {
        reset()
        role = .host
        code = PeerHandshake.newCode()
        failures = 0
        wrongTotal = 0
        let params = Self.parameters()
        if !advertise {
            params.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: .any)
        }
        guard let l = try? NWListener(using: params) else {
            fail("network")
            return
        }
        if advertise {
            l.service = NWListener.Service(name: localName, type: Self.serviceType, txtRecord: NWTXTRecord(["v": "1"]))
        }
        l.serviceRegistrationUpdateHandler = { [weak self] change in
            if case .add(let endpoint) = change, case .service(let name, _, _, _) = endpoint {
                self?.ownService = name
            }
        }
        l.newConnectionHandler = { [weak self, weak l] c in
            guard let self, let l, l === self.listener else {
                c.cancel()
                return
            }
            self.accept(c)
        }
        var announced = false
        l.stateUpdateHandler = { [weak self, weak l] state in
            guard let self, let l, l === self.listener else { return }
            switch state {
            case .ready:
                guard !announced else { return }
                announced = true
                self.emit(["type": "hosting", "code": self.code])
            case .waiting(let error):
                if Self.denied(error) { self.fail("denied") }
            case .failed(let error):
                self.fail(Self.denied(error) ? "denied" : "network")
            default:
                break
            }
        }
        listener = l
        l.start(queue: .main)
    }

    // one guest at a time; anyone else hears busy
    private func accept(_ c: NWConnection) {
        guard role == .host, connection == nil, cipher == nil else {
            c.start(queue: .main)
            c.send(content: Self.framed(Data([PeerHandshake.busy])), completion: .contentProcessed { _ in c.cancel() })
            return
        }
        let hs = PeerHandshake(role: .host, code: code)
        handshake = hs
        connection = c
        c.stateUpdateHandler = { [weak self] state in
            guard let self, c === self.connection else { return }
            switch state {
            case .ready:
                self.write(hs.start())
                self.readFrame(c)
            case .failed, .cancelled:
                self.dropped()
            default:
                break
            }
        }
        heardGuest = false
        armTimeout(seconds: Self.quietSeconds)
        c.start(queue: .main)
    }

    // a try that didn't pair: drop that guest, keep hosting, and after a few wrong codes show a fresh one
    private func endAttempt(guessed: Bool) {
        disarmTimeout()
        let c = connection
        connection = nil
        handshake = nil
        c?.cancel()
        guard guessed else { return }
        failures += 1
        wrongTotal += 1
        if wrongTotal >= Self.triesBeforeStop {
            fail("too-many-tries")
            return
        }
        guard failures >= Self.triesPerCode else { return }
        failures = 0
        code = PeerHandshake.newCode()
        emit(["type": "hosting", "code": code, "renewed": true])
    }

    // MARK: joining

    func join(_ id: String, code: String) {
        guard let endpoint = found[id] else {
            reset()
            emit(["type": "closed", "reason": "unreachable"])
            return
        }
        connect(to: endpoint, code: code)
    }

    func connect(to endpoint: NWEndpoint, code: String) {
        let keepBrowser = browser
        browser = nil
        reset()
        browser = keepBrowser
        guard PeerHandshake.isCode(code) else {
            emit(["type": "closed", "reason": "wrong-code"])
            return
        }
        role = .guest
        handshake = PeerHandshake(role: .guest, code: code)
        let c = NWConnection(to: endpoint, using: Self.parameters())
        connection = c
        c.stateUpdateHandler = { [weak self] state in
            guard let self, c === self.connection else { return }
            switch state {
            case .ready:
                self.readFrame(c)
            case .waiting, .failed, .cancelled:
                self.dropped()
            default:
                break
            }
        }
        armTimeout()
        emit(["type": "pairing"])
        c.start(queue: .main)
    }

    // MARK: after pairing

    // a frame the other side would refuse (and drop the link over) is not sent
    func send(_ text: String) {
        let body = Data(text.utf8)
        guard opened, body.count + Self.sealOverhead <= Self.maxFrame else { return }
        seal(Self.data, body)
    }

    // the page's own leave: tell the other side, then let go of everything quietly
    func close() {
        if let c = connection, let cipher, let frame = cipher.seal(Data([Self.bye])) {
            c.send(content: Self.framed(frame), completion: .contentProcessed { _ in c.cancel() })
            connection = nil
        }
        reset()
    }

    private func seal(_ kind: UInt8, _ body: Data) {
        guard let cipher, let frame = cipher.seal(Data([kind]) + body) else { return }
        write(frame)
    }

    // the timeout stays armed until the other side's name opens the link
    private func paired(_ keys: PeerKeys) {
        handshake = nil
        cipher = PeerCipher(keys)
        // paired, so stop advertising; the open connection stays
        listener?.cancel()
        listener = nil
        seal(Self.name, Data(localName.utf8))
    }

    // MARK: frames

    private static func framed(_ body: Data) -> Data {
        var n = UInt32(body.count).bigEndian
        return Data(bytes: &n, count: 4) + body
    }

    private func write(_ body: Data) {
        connection?.send(content: Self.framed(body), completion: .contentProcessed { _ in })
    }

    private func readFrame(_ c: NWConnection) {
        c.receive(minimumIncompleteLength: 4, maximumLength: 4) { [weak self] head, _, _, error in
            guard let self, c === self.connection else { return }
            guard error == nil, let head, head.count == 4 else {
                self.dropped()
                return
            }
            let n = head.reduce(0) { $0 << 8 | Int($1) }
            guard n <= Self.maxFrame else {
                self.dropped()
                return
            }
            guard n > 0 else {
                self.readFrame(c)
                return
            }
            c.receive(minimumIncompleteLength: n, maximumLength: n) { [weak self] body, _, _, error in
                guard let self, c === self.connection else { return }
                guard error == nil, let body, body.count == n else {
                    self.dropped()
                    return
                }
                self.handle(Data(body))
                if c === self.connection { self.readFrame(c) }
            }
        }
    }

    private func handle(_ frame: Data) {
        if let cipher {
            guard let plain = cipher.open(frame), let kind = plain.first else {
                fail("lost")
                return
            }
            let body = plain.dropFirst()
            switch kind {
            case Self.name:
                guard !opened else { return }
                opened = true
                disarmTimeout()
                let peer = String(decoding: body.prefix(63), as: UTF8.self)
                emit(["type": "open", "role": role == .host ? "host" : "guest", "peer": peer])
            case Self.data:
                guard opened else { return }
                emit(["type": "message", "data": String(decoding: body, as: UTF8.self)])
            case Self.bye:
                fail("bye")
            default:
                break
            }
            return
        }
        guard let hs = handshake else { return }
        if role == .host, !heardGuest {
            heardGuest = true
            armTimeout()
        }
        if role == .guest, frame.first == PeerHandshake.busy {
            fail("busy")
            return
        }
        switch hs.receive(frame) {
        case .send(let reply):
            write(reply)
        case .paired(let keys, let reply):
            if let reply { write(reply) }
            paired(keys)
        case .wrongCode:
            if role == .host { endAttempt(guessed: true) } else { fail("wrong-code") }
        case .invalid:
            if role == .host { endAttempt(guessed: hs.guessed) } else { fail("unreachable") }
        }
    }

    // the connection went away: mid-pairing a host keeps hosting, anything else is over
    private func dropped() {
        if role == .host, let hs = handshake {
            endAttempt(guessed: hs.guessed)
            return
        }
        fail(cipher == nil ? "unreachable" : "lost")
    }

    private func armTimeout(seconds: Double = PeerLink.handshakeSeconds) {
        disarmTimeout()
        let item = DispatchWorkItem { [weak self] in
            guard let self, self.connection != nil, !self.opened else { return }
            if self.role == .host, let hs = self.handshake {
                self.endAttempt(guessed: hs.guessed)
            } else {
                self.fail(self.cipher == nil ? "unreachable" : "lost")
            }
        }
        timeout = item
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: item)
    }

    private func disarmTimeout() {
        timeout?.cancel()
        timeout = nil
    }

    private func fail(_ reason: String) {
        reset()
        emit(["type": "closed", "reason": reason])
    }

    private func reset() {
        disarmTimeout()
        connection?.cancel()
        connection = nil
        listener?.cancel()
        listener = nil
        browser?.cancel()
        browser = nil
        handshake = nil
        cipher = nil
        role = nil
        opened = false
        ownService = nil
    }
}
