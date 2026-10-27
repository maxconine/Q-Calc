import AppKit
import QCalcCore

/// A link to one other Q Calc (`PeerTransport` in `src/lib/peer.ts`). Games see only its events (`GamePeerEvent`,
/// from the core) and send text; the lobby hosts, joins and watches rooms. Events arrive on the main thread.
protocol PeerTransport: AnyObject {
    /// What the lobby says it is.
    var label: String { get }
    /// What the other end is called in the lobby's copy.
    var peerWord: String { get }
    var codeLength: Int { get }
    /// Lists the rooms open for this game, as `.rooms` events, until unwatch.
    func watch(_ game: String)
    func unwatch()
    func host(game: String, name: String, locked: Bool)
    /// code is "" for an open room; name is what the host will see.
    func join(roomId: String, code: String, name: String)
    func send(_ data: String)
    /// Mid-game, the window hid: let go of the link but keep the seat.
    func stepAway()
    /// The window showed again: take the seat back.
    func comeBack()
    /// Leaves quietly: the other side hears `bye`, this side hears nothing.
    func close()
    /// Returns the unsubscribe.
    func subscribe(_ fn: @escaping (GamePeerEvent) -> Void) -> () -> Void
}

/// The relay (`OnlinePeer` in `src/lib/peerOnline.ts`). The core's `RelayPeer` decides everything; this runs its
/// websockets (URLSessionWebSocketTask) and its timers, and passes the events on.
final class RelayTransport: NSObject, PeerTransport, URLSessionWebSocketDelegate {
    static let shared = RelayTransport()

    let label = "online"
    let peerWord = "computer"
    let codeLength = Int(relayCodeLength())

    private let core: RelayPeer
    private var session: URLSession!
    private var sockets: [UInt64: URLSessionWebSocketTask] = [:]
    private var ids: [Int: UInt64] = [:]
    private var listeners: [(id: Int, fn: (GamePeerEvent) -> Void)] = []
    private var nextListener = 0
    private var timer: Timer?

    /// The relay this build talks to: QCALC_RELAY_URL for a local one (`wrangler dev`, loopback only), as
    /// Messages does, else the real one.
    static var base: String {
        if let env = ProcessInfo.processInfo.environment["QCALC_RELAY_URL"], let url = URL(string: env),
           let host = url.host, ["localhost", "127.0.0.1", "::1"].contains(host) {
            return env.replacingOccurrences(of: "http", with: "ws", options: .anchored)
        }
        return relayDefaultUrl()
    }

    override init() {
        core = RelayPeer(base: RelayTransport.base)
        super.init()
        session = URLSession(configuration: .ephemeral, delegate: self, delegateQueue: .main)
        // quitting loses the game itself, so leave and the friend hears at once
        NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
            self?.core.pageHide()
            self?.run()
        }
        // back sooner than the next try: the computer woke
        NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.retryNow()
        }
    }

    private var now: Double { Date().timeIntervalSince1970 * 1000 }

    func watch(_ game: String) { core.watch(game: game, now: now); run() }
    func unwatch() { core.unwatch(); run() }
    func host(game: String, name: String, locked: Bool) { core.host(game: game, name: name, locked: locked, now: now); run() }
    func join(roomId: String, code: String, name: String) { core.join(roomId: roomId, code: code, name: name, now: now); run() }
    func send(_ data: String) { core.send(data: data); run() }
    func stepAway() { core.stepAway(now: now); run() }
    func comeBack() { core.comeBack(now: now); run() }
    func close() { core.close(); run() }
    /// The computer woke or came back online: try for a held seat now.
    func retryNow() { core.retryNow(now: now); run() }

    func subscribe(_ fn: @escaping (GamePeerEvent) -> Void) -> () -> Void {
        nextListener += 1
        let id = nextListener
        listeners.append((id, fn))
        return { [weak self] in self?.listeners.removeAll { $0.id == id } }
    }

    // what the core asked for, then what it said, then when to wake it
    private func run() {
        for cmd in core.takeCommands() {
            switch cmd {
            case let .open(id, url):
                guard let u = URL(string: url) else { core.closed(id: id, now: now); continue }
                let task = session.webSocketTask(with: u)
                sockets[id] = task
                ids[task.taskIdentifier] = id
                task.resume()
                listen(task, id)
            case let .send(id, text):
                sockets[id]?.send(.string(text)) { _ in }
            case let .close(id, code):
                if let task = sockets.removeValue(forKey: id) {
                    ids.removeValue(forKey: task.taskIdentifier)
                    task.cancel(with: URLSessionWebSocketTask.CloseCode(rawValue: Int(code)) ?? .normalClosure, reason: nil)
                }
            }
        }
        let events = core.takeEvents()
        for e in events {
            for l in listeners { l.fn(e) }
        }
        // a listener may have called back in
        if !events.isEmpty { return run() }
        schedule()
    }

    private func schedule() {
        timer?.invalidate()
        timer = nil
        guard let at = core.nextDeadline() else { return }
        let wait = max(0, (at - now) / 1000)
        let t = Timer(timeInterval: wait + 0.001, repeats: false) { [weak self] _ in
            guard let self else { return }
            self.core.tick(now: self.now)
            self.run()
        }
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    private func listen(_ task: URLSessionWebSocketTask, _ id: UInt64) {
        task.receive { [weak self] result in
            DispatchQueue.main.async {
                guard let self, self.sockets[id] === task else { return }
                switch result {
                case let .success(.string(text)):
                    self.core.message(id: id, text: text, now: self.now)
                    self.run()
                    self.listen(task, id)
                case .success:
                    self.listen(task, id)
                case .failure:
                    self.gone(id)
                }
            }
        }
    }

    private func gone(_ id: UInt64) {
        guard let task = sockets.removeValue(forKey: id) else { return }
        ids.removeValue(forKey: task.taskIdentifier)
        core.closed(id: id, now: now)
        run()
    }

    func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didOpenWithProtocol protocol: String?) {
        guard let id = ids[webSocketTask.taskIdentifier] else { return }
        core.opened(id: id, now: now)
        run()
    }

    func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didCloseWith closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        if let id = ids[webSocketTask.taskIdentifier] { gone(id) }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let id = ids[task.taskIdentifier] { gone(id) }
    }
}
