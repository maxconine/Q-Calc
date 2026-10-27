import AppKit
import CryptoKit
import Foundation
import UniformTypeIdentifiers
import UserNotifications

// Clips and files between names (native only; relay/README.md, "Files and clips", and native/PRIVACY.md). Both are
// end-to-end encrypted: a clip, and a file's facts (its name, type, size, SHA-256 and its own AES key), are sealed
// for the recipient alone (ShareCrypt, with the keys ShareKeys keeps and pins), so the relay sees only who sent
// to whom, an id, the encrypted length and when. A clip rides the inbox like a message. A file is offered first:
// the recipient's app opens the facts and shows its name and size, and only when they press Save (or their
// settings take it on their own) does the relay let the sender upload it, which this app does in the background,
// encrypted (FileCrypt), in the relay's parts, while it keeps running. Then the recipient's app downloads it,
// decrypts it beside Downloads, checks its size and hash, and moves it into Downloads under a name nothing else
// has, quarantined and not executable (SafeSave). It's never opened, previewed or run. Someone not accepted yet
// is a request: their file shows, but Save waits until they're allowed. Everything about a file on its way (its
// key, its grant, its progress) is in memory only.
final class MessageFiles: NSObject, ObservableObject {
    static let shared = MessageFiles()
    static let clipMax = 64 * 1024
    static let fileMax: UInt64 = 1024 * 1024 * 1024
    static let autoAcceptKey = "qcalc.messaging.autoAcceptBytes"
    private static let partTries = 3

    /// A file on its way, either way.
    struct Transfer: Equatable {
        let id: String
        let mine: Bool
        let who: String
        let name: String
        let size: UInt64
        /// preparing, offered, uploading, sent (mine); request, received, accepting, waiting, downloading,
        /// decrypting, saved (theirs); declined, cancelled, withdrawn, refused, lost, failed (either)
        var state: String
        var percent: Int?
        /// why it failed, in the bar's words
        var error: String?
        var saved: URL?
    }

    /// What the core says, through BarModel's logic (set when the bar is made)
    var logic: AppLogic?

    @Published private(set) var transfers: [String: Transfer] = [:]
    /// a file dropped on the bar with no name on the line yet (ShareCommands.swift)
    var pendingDrop: [URL] = []

    /// what a sender's app holds for an upload: the sealed file and its length
    private struct Outgoing {
        let sealed: URL
        let bytes: UInt64
        var tasks: [URLSessionTask] = []
        var cancelled = false
    }

    /// what a recipient's app holds for a download: the opened facts, with the file's key
    private struct Incoming {
        let key: String
        let sha256: String
        let size: UInt64
        let bytes: UInt64
        let expires: Double
        var auto = false
        var task: URLSessionTask?
    }

    private var outgoing: [String: Outgoing] = [:]
    private var incoming: [String: Incoming] = [:]
    /// share ids already taken in, so one replayed by the relay is ignored
    private var seenIds = Set<String>()
    /// what came from someone whose key changed, opened once it's trusted (ShareKeys.trust)
    private var heldForKey: [(from: String, event: [String: Any])] = []
    /// shares from names not accepted yet, to show once they are: (from, clip or file id)
    private var held: [(from: String, clip: String?, file: ThreadFile?)] = []
    /// cards for the bar that it hasn't taken yet
    private var queued: [BannerCard] = []
    /// each transfer's progress watchers, let go when it's done
    private var observers: [String: [NSKeyValueObservation]] = [:]

    private let work = DispatchQueue(label: "com.maxconine.qcalc.message-files", qos: .userInitiated)
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 60
        config.timeoutIntervalForResource = 60 * 60 * 6
        config.waitsForConnectivity = false
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.urlCache = nil
        let queue = OperationQueue()
        queue.maxConcurrentOperationCount = 1
        return URLSession(configuration: config, delegate: nil, delegateQueue: queue)
    }()

    private static let temp: URL = {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("QCalcFiles", isDirectory: true)
        try? FileManager.default.removeItem(at: dir)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }()

    // MARK: - words, from the core

    private func core(_ fn: String, _ args: [Any]) -> String? { logic?.string("lib/messaging", fn, args) }

    func sizeText(_ bytes: UInt64) -> String {
        core("fileSizeText", [bytes]) ?? ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
    }

    func stateText(_ t: Transfer) -> String {
        if let error = t.error { return error }
        return core("fileStateText", [t.state, t.who, t.percent as Any? ?? NSNull()]) ?? t.state
    }

    func errorText(_ status: String, who: String) -> String {
        core("fileErrorText", [status, who]) ?? "couldn’t send that file"
    }

    func shareStatus(_ status: String, to: String, what: String, name: String = "") -> String {
        core("shareStatusText", [status, to, what, name]) ?? Messages.sendStatusText(status, to: to)
    }

    /// a file name kept to its last part, with nothing in it that means anything to a path or hides what it is
    /// (the core's cleanFilename; this is only its stand-in before the core is up)
    static func safeName(_ raw: String) -> String {
        if let clean = shared.logic?.string("lib/messaging", "cleanFilename", [raw]) { return clean }
        let last = raw.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
        let cleaned = last.replacingOccurrences(of: "[\\x{0000}-\\x{001f}\\x{007f}-\\x{009f}\\x{200b}-\\x{200f}\\x{2028}-\\x{202e}\\x{2066}-\\x{2069}\\x{feff}:]+", with: "_", options: .regularExpression)
            .replacingOccurrences(of: "^[\\s.]+", with: "", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
        return cleaned.isEmpty ? "file" : String(cleaned.prefix(120))
    }

    private func cleanName(_ raw: String) -> String { core("cleanFilename", [raw]) ?? Self.safeName(raw) }

    private var autoLimit: UInt64 { UInt64(max(0, UserDefaults.standard.integer(forKey: Self.autoAcceptKey))) }

    // MARK: - the relay

    private func request(_ path: String, method: String, headers: [String: String] = [:]) -> URLRequest? {
        let m = Messages.shared
        guard let url = URL(string: "\(Messages.relayBase)\(path)"), !m.name.isEmpty, !m.relayKey.isEmpty else { return nil }
        var r = URLRequest(url: url, timeoutInterval: 60)
        r.httpMethod = method
        r.setValue(m.name, forHTTPHeaderField: "X-QCalc-Name")
        r.setValue(m.relayKey, forHTTPHeaderField: "X-QCalc-Key")
        for (k, v) in headers { r.setValue(v, forHTTPHeaderField: k) }
        return r
    }

    /// the relay's status for a request, or `offline` when it couldn't be reached
    private static func status(_ data: Data?, _ response: URLResponse?, _ error: Error?) -> (String, [String: Any]) {
        if let error = error as? URLError {
            let off: Set<URLError.Code> = [.notConnectedToInternet, .networkConnectionLost, .cannotFindHost, .cannotConnectToHost, .dnsLookupFailed, .timedOut]
            // not the relay's `offline`, which is about the recipient
            return (off.contains(error.code) ? "no-network" : "network", [:])
        }
        let o = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] } ?? [:]
        if let s = o["status"] as? String { return (s, o) }
        switch (response as? HTTPURLResponse)?.statusCode {
        case 413: return ("too-big", o)
        case 410: return ("expired", o)
        case 404: return ("gone", o)
        default: return ("network", o)
        }
    }

    private func call(_ path: String, method: String = "POST", headers: [String: String] = [:], body: Data? = nil,
                      done: @escaping (String, [String: Any]) -> Void) {
        guard var r = request(path, method: method, headers: headers) else { return done("not-yours", [:]) }
        r.httpBody = body
        session.dataTask(with: r) { data, response, error in
            let (s, o) = Self.status(data, response, error)
            DispatchQueue.main.async { done(s, o) }
        }.resume()
    }

    /// a json /send, as Messages does for text
    private func send(_ body: [String: Any], done: @escaping (String, [String: Any]) -> Void) {
        guard let url = URL(string: "\(Messages.relayBase)/send"), let data = try? JSONSerialization.data(withJSONObject: body) else {
            return done("bad", [:])
        }
        var r = URLRequest(url: url, timeoutInterval: 20)
        r.httpMethod = "POST"
        r.setValue("text/plain", forHTTPHeaderField: "Content-Type")
        r.httpBody = data
        session.dataTask(with: r) { data, response, error in
            let (s, o) = Self.status(data, response, error)
            DispatchQueue.main.async { done(s, o) }
        }.resume()
    }

    private func update(_ id: String, _ change: (inout Transfer) -> Void) {
        guard var t = transfers[id] else { return }
        change(&t)
        if transfers[id] != t { transfers[id] = t }
    }

    // MARK: - sending a clip

    /// the clipboard to `to`: text as a clip (a text file past 64 KB), a copied file or an image as a file
    func sendClipboard(to: String, done: @escaping (String) -> Void) {
        let pb = NSPasteboard.general
        if let urls = pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL], let url = urls.first {
            return sendFile(url, to: to, done: done)
        }
        if let text = pb.string(forType: .string), !text.isEmpty {
            if text.utf8.count <= Self.clipMax { return sendClip(text, to: to, done: done) }
            let url = Self.temp.appendingPathComponent("clipboard.txt")
            guard (try? Data(text.utf8).write(to: url)) != nil else { return done(errorText("disk", who: to)) }
            return sendFile(url, to: to, done: done)
        }
        if let image = NSImage(pasteboard: pb), let tiff = image.tiffRepresentation, let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) {
            let url = Self.temp.appendingPathComponent("clipboard.png")
            guard (try? png.write(to: url)) != nil else { return done(errorText("disk", who: to)) }
            return sendFile(url, to: to, done: done)
        }
        done("there’s nothing copied to share")
    }

    func sendClip(_ clip: String, to raw: String, done: @escaping (String) -> Void) {
        let to = Messages.clean(raw)
        let m = Messages.shared
        guard !m.name.isEmpty else { return done(shareStatus("not-yours", to: to, what: "clip")) }
        PrivacyNotice.shared.firstShare()
        recipientKey(to, what: "clip", done: done) { [weak self] theirs, theirText in
            guard let self else { return }
            let id = ShareCrypt.newId()
            guard let sealed = self.seal(Data(clip.utf8), kind: "clip", to: to, id: id, theirs: theirs, theirText: theirText) else {
                return done(self.shareStatus("bad", to: to, what: "clip"))
            }
            let body: [String: Any] = ["from": m.name, "key": m.relayKey, "to": to, "kind": "clip",
                                       "clip": ["id": id, "epk": sealed.epk, "sealed": sealed.sealed]]
            self.send(body) { status, _ in
                if status == "delivered" {
                    m.noteSent(to)
                    MessageHistory.shared.addShare(with: to, mine: true, summary: self.clipSummary(clip), clip: clip)
                }
                done(self.shareStatus(status, to: to, what: "clip"))
            }
        }
    }

    // MARK: - sealing

    /// `to`'s key, checked against their pin, before anything's sealed for them; `done` hears why not, if not
    private func recipientKey(_ to: String, what: String, done: @escaping (String) -> Void,
                              then: @escaping (ShareCrypt.PublicKey, String) -> Void) {
        ShareKeys.shared.keyFor(sending: to) { [weak self] found in
            guard let self else { return }
            switch found {
            case let .key(theirs, text): then(theirs, text)
            case .changed: done(self.shareStatus("key-changed", to: to, what: what))
            case .none: done(self.shareStatus("no-key", to: to, what: what))
            case .unknown: done(self.shareStatus("unknown", to: to, what: what))
            case let .offline(status): done(self.shareStatus(status == "not-yours" ? status : "network", to: to, what: what))
            }
        }
    }

    /// the core's context for a share between two names, which the seal is bound to
    private func context(_ kind: String, from: String, to: String, id: String, fromKey: String, toKey: String) -> String? {
        core("shareContext", [kind, from, to, id, fromKey, toKey])
    }

    private func seal(_ plain: Data, kind: String, to: String, id: String, theirs: ShareCrypt.PublicKey,
                      theirText: String) -> (epk: String, sealed: String)? {
        let keys = ShareKeys.shared
        guard let me = keys.privateKey,
              let ctx = context(kind, from: Messages.shared.name, to: to, id: id, fromKey: keys.publicText, toKey: theirText) else { return nil }
        return try? ShareCrypt.seal(plain, me: me, to: theirs, context: ctx)
    }

    private func clipSummary(_ clip: String) -> String {
        let first = core("clipPreview", [clip]) ?? clip
        return first.isEmpty ? "clipboard" : first
    }

    // MARK: - sending a file

    /// encrypts the file, then offers it; `done` hears what the bar says. The upload waits for the recipient
    func sendFile(_ url: URL, to raw: String, done: @escaping (String) -> Void) {
        let to = Messages.clean(raw)
        let m = Messages.shared
        guard !m.name.isEmpty else { return done(shareStatus("not-yours", to: to, what: "file")) }
        let values = try? url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey, .typeIdentifierKey])
        guard values?.isRegularFile == true, let size = values?.fileSize.map(UInt64.init) else { return done(errorText("unreadable", who: to)) }
        guard size <= Self.fileMax else { return done(shareStatus("too-big", to: to, what: "file")) }
        let name = cleanName(url.lastPathComponent)
        let type = values?.typeIdentifier.flatMap { UTType($0)?.preferredMIMEType } ?? "application/octet-stream"
        PrivacyNotice.shared.firstShare()
        recipientKey(to, what: "file", done: done) { [weak self] theirs, theirText in
            self?.encryptAndOffer(url, name: name, type: type, size: size, to: to, theirs: theirs, theirText: theirText, done: done)
        }
    }

    private func encryptAndOffer(_ url: URL, name: String, type: String, size: UInt64, to: String,
                                 theirs: ShareCrypt.PublicKey, theirText: String, done: @escaping (String) -> Void) {
        let m = Messages.shared
        let key = FileCrypt.newKey()
        let sealed = Self.temp.appendingPathComponent(UUID().uuidString)
        done("encrypting \(name)…")
        work.async { [weak self] in
            guard let self else { return }
            let sha: String
            do {
                sha = try FileCrypt.encrypt(url, to: sealed, key: key)
            } catch {
                try? FileManager.default.removeItem(at: sealed)
                DispatchQueue.main.async { done(self.errorText(error as? FileCrypt.Failure == .disk ? "disk" : "unreadable", who: to)) }
                return
            }
            let bytes = FileCrypt.encryptedLength(size)
            DispatchQueue.main.async {
                // the name, type, size, hash and key, sealed for `to` alone under an id drawn here
                let id = ShareCrypt.newId()
                guard let facts = self.core("sealFileFacts", [name, type, size, sha, FileCrypt.encode(key)]),
                      let box = self.seal(Data(facts.utf8), kind: "file", to: to, id: id, theirs: theirs, theirText: theirText) else {
                    try? FileManager.default.removeItem(at: sealed)
                    return done(self.shareStatus("bad", to: to, what: "file", name: name))
                }
                let file: [String: Any] = ["id": id, "epk": box.epk, "sealed": box.sealed, "bytes": bytes]
                self.send(["from": m.name, "key": m.relayKey, "to": to, "kind": "file", "file": file]) { status, o in
                    guard status == "delivered", o["id"] as? String == id else {
                        try? FileManager.default.removeItem(at: sealed)
                        return done(self.shareStatus(status, to: to, what: "file", name: name))
                    }
                    m.noteSent(to)
                    self.outgoing[id] = Outgoing(sealed: sealed, bytes: bytes)
                    self.transfers[id] = Transfer(id: id, mine: true, who: to, name: name, size: size, state: "offered")
                    MessageHistory.shared.addShare(with: to, mine: true, summary: name, file: ThreadFile(id: id, name: name, size: size))
                    done(self.shareStatus(status, to: to, what: "file", name: name))
                }
            }
        }
    }

    /// the recipient said yes: upload it with the grant, part by part
    private func upload(_ id: String, grant: String, partBytes: UInt64) {
        guard let out = outgoing[id], let t = transfers[id], t.mine else {
            // a file this app doesn't have any more (it was quit and opened again): the relay lets it go
            call("/file/\(id)", method: "DELETE") { _, _ in }
            return
        }
        update(id) { $0.state = "uploading"; $0.percent = 0; $0.error = nil }
        call("/file/\(id)/start", headers: ["X-QCalc-Grant": grant]) { [weak self] status, o in
            guard let self else { return }
            guard status == "ok" else { return self.failed(id, status) }
            let parts = (o["parts"] as? NSNumber)?.intValue ?? 1
            let done = Set((o["done"] as? [NSNumber] ?? []).map(\.intValue))
            let size = (o["partBytes"] as? NSNumber)?.uint64Value ?? partBytes
            self.part(id, n: 1, of: parts, skip: done, partBytes: size, total: out.bytes, tries: 0)
        }
    }

    private func part(_ id: String, n: Int, of parts: Int, skip: Set<Int>, partBytes: UInt64, total: UInt64, tries: Int) {
        guard let out = outgoing[id], !out.cancelled else { return }
        if n > parts { return finish(id) }
        if skip.contains(n) { return part(id, n: n + 1, of: parts, skip: skip, partBytes: partBytes, total: total, tries: 0) }
        let offset = UInt64(n - 1) * partBytes
        let length = Int(min(partBytes, total - offset))
        work.async { [weak self] in
            guard let self else { return }
            let data: Data? = {
                guard let h = try? FileHandle(forReadingFrom: out.sealed) else { return nil }
                defer { try? h.close() }
                guard (try? h.seek(toOffset: offset)) != nil else { return nil }
                return length == 0 ? Data() : try? h.read(upToCount: length)
            }()
            DispatchQueue.main.async {
                guard let data, data.count == length, var r = self.request("/file/\(id)/part/\(n)", method: "PUT") else {
                    return self.failed(id, "unreadable")
                }
                r.setValue(String(length), forHTTPHeaderField: "Content-Length")
                r.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
                let task = self.session.uploadTask(with: r, from: data) { body, response, error in
                    let (status, _) = Self.status(body, response, error)
                    DispatchQueue.main.async {
                        guard self.outgoing[id]?.cancelled == false else { return }
                        if status == "ok" {
                            return self.part(id, n: n + 1, of: parts, skip: skip, partBytes: partBytes, total: total, tries: 0)
                        }
                        // a part cut short goes again, a few times, a little later each time
                        if Self.retryable.contains(status), tries + 1 < Self.partTries {
                            DispatchQueue.main.asyncAfter(deadline: .now() + Double(2 << tries)) {
                                self.part(id, n: n, of: parts, skip: skip, partBytes: partBytes, total: total, tries: tries + 1)
                            }
                            return
                        }
                        self.failed(id, status)
                    }
                }
                let base = Double(offset) / Double(max(total, 1))
                let share = Double(length) / Double(max(total, 1))
                self.observers[id, default: []].append(task.progress.observe(\.fractionCompleted) { p, _ in
                    let pct = Int(((base + share * p.fractionCompleted) * 100).rounded(.down))
                    DispatchQueue.main.async { self.update(id) { if $0.state == "uploading" { $0.percent = pct } } }
                })
                self.outgoing[id]?.tasks.append(task)
                task.resume()
            }
        }
    }

    private func finish(_ id: String) {
        call("/file/\(id)/finish") { [weak self] status, _ in
            guard let self else { return }
            guard status == "ok" else { return self.failed(id, status) }
            self.update(id) { $0.state = "sent"; $0.percent = nil }
            self.dropOutgoing(id)
        }
    }

    private func dropOutgoing(_ id: String) {
        if let out = outgoing.removeValue(forKey: id) {
            out.tasks.forEach { $0.cancel() }
            try? FileManager.default.removeItem(at: out.sealed)
        }
        unwatch(id)
    }

    private func unwatch(_ id: String) {
        observers.removeValue(forKey: id)?.forEach { $0.invalidate() }
    }

    /// what can go again: a part or a request cut short, or the network gone a while
    private static let retryable: Set<String> = ["failed", "no-network", "network"]

    private func failed(_ id: String, _ status: String) {
        guard let t = transfers[id] else { return }
        update(id) { $0.state = "failed"; $0.percent = nil; $0.error = self.errorText(status, who: t.who) }
        // an upload that can pick up where it stopped keeps its sealed file for Retry
        if t.mine, !Self.retryable.contains(status) { dropOutgoing(id) }
    }

    /// whether Retry can pick a failed upload of yours up again
    func canRetry(_ id: String) -> Bool {
        transfers[id]?.mine == true && transfers[id]?.state == "failed" && outgoing[id] != nil
    }

    /// Retry: the relay says which parts it has (no grant needed once started), and the rest go
    func retry(_ id: String) {
        guard canRetry(id) else { return }
        upload(id, grant: "", partBytes: 50 * 1024 * 1024)
    }

    // MARK: - what arrives

    /// a `message` with a kind: a clip or an offer, from someone not blocked
    func heard(_ event: [String: Any]) {
        let m = Messages.shared
        let from = Messages.clean(event["from"] as? String ?? "")
        guard !from.isEmpty, !m.blocked.contains(from) else {
            // a blocked name's file is turned down unseen
            if let offer = event["file"] as? [String: Any], let id = offer["id"] as? String, !from.isEmpty { call("/file/\(id)", method: "DELETE") { _, _ in } }
            return
        }
        // only sealed shares are taken (a plain clip or offer from an older copy is ignored), each id once
        let kind = event["kind"] as? String ?? ""
        let fn = kind == "clip" ? "cleanSealedClip" : kind == "file" ? "cleanSealedOffer" : ""
        guard !fn.isEmpty, let share = logic?.call("lib/messaging", fn, [event[kind] ?? NSNull()]) as? [String: Any],
              let id = share["id"] as? String, let epk = share["epk"] as? String, let sealed = share["sealed"] as? String,
              !seenIds.contains(id) else { return }
        openShare(kind: kind, from: from, id: id, epk: epk, sealed: sealed, event: event) { [weak self] plain in
            guard let self, self.seenIds.insert(id).inserted else { return }
            if kind == "clip" {
                self.received(clip: plain, from: from)
            } else {
                self.received(facts: plain, id: id, share: share, from: from)
            }
        }
    }

    /// a sealed share from `from`, opened with their pinned key (pinned now, the first time). One that doesn't
    /// open is checked against the relay: if their key has changed it waits for Trust New Key in their thread;
    /// otherwise it's dropped, and a file's offer is turned down so the relay lets it go
    private func openShare(kind: String, from: String, id: String, epk: String, sealed: String, event: [String: Any],
                           then: @escaping (Data) -> Void) {
        let keys = ShareKeys.shared
        let turnDown = { [weak self] in if kind == "file" { self?.call("/file/\(id)", method: "DELETE") { _, _ in } } }
        let holdForKey = { [weak self] in
            guard let self else { return }
            if !self.heldForKey.contains(where: { ($0.event[kind] as? [String: Any])?["id"] as? String == id }) {
                self.heldForKey.append((from, event))
            }
            NotificationCenter.default.post(name: .qcalcInboxChanged, object: nil)
        }
        keys.keyFor(receiving: from) { [weak self] found in
            guard let self else { return }
            switch found {
            case let .key(theirs, theirText):
                if let me = keys.privateKey,
                   let ctx = self.context(kind, from: from, to: Messages.shared.name, id: id, fromKey: theirText, toKey: keys.publicText),
                   let plain = try? ShareCrypt.open(sealed, epk: epk, me: me, from: theirs, context: ctx) {
                    return then(plain)
                }
                keys.recheck(from) { changed in if changed { holdForKey() } else { turnDown() } }
            case .changed:
                holdForKey()
            case .none, .unknown:
                turnDown()
            case .offline:
                // their key couldn't be fetched the first time: tried again a little later, a few times
                let tries = (event["retries"] as? Int) ?? 0
                guard tries < 3 else { return }
                var again = event
                again["retries"] = tries + 1
                DispatchQueue.main.asyncAfter(deadline: .now() + Double(10 << tries)) { [weak self] in self?.heard(again) }
            }
        }
    }

    /// shares held while `who`'s key had changed, opened now it's trusted
    func retryHeld(from who: String) {
        let theirs = heldForKey.filter { $0.from == who }
        heldForKey.removeAll { $0.from == who }
        theirs.forEach { heard($0.event) }
    }

    private func received(clip plain: Data, from: String) {
        guard let text = String(data: plain, encoding: .utf8),
              let clip = logic?.call("lib/messaging", "cleanClip", [text]) as? String else { return }
        if Messages.shared.accepted.contains(from) { show(from: from, clip: clip, file: nil) } else { ask(from: from, clip: clip, file: nil) }
    }

    /// a file's facts, checked again here: the size against the limit and the length the relay holds
    private func received(facts plain: Data, id: String, share: [String: Any], from: String) {
        guard let bytes = (share["bytes"] as? NSNumber)?.uint64Value, let text = String(data: plain, encoding: .utf8),
              let o = logic?.call("lib/messaging", "openFileFacts", [text, bytes]) as? [String: Any],
              let name = o["name"] as? String, let size = (o["size"] as? NSNumber)?.uint64Value,
              let key = o["key"] as? String, let sha = o["sha256"] as? String,
              size <= Self.fileMax, bytes == FileCrypt.encryptedLength(size), FileCrypt.decode(key) != nil else {
            call("/file/\(id)", method: "DELETE") { _, _ in }
            return
        }
        let accepted = Messages.shared.accepted.contains(from)
        let file = ThreadFile(id: id, name: name, size: size)
        incoming[id] = Incoming(key: key, sha256: sha, size: size, bytes: bytes, expires: (share["expires"] as? NSNumber)?.doubleValue ?? 0)
        transfers[id] = Transfer(id: id, mine: false, who: from, name: name, size: size, state: accepted ? "received" : "request")
        if accepted {
            show(from: from, clip: nil, file: file)
            autoAccept(id, from: from, size: size)
        } else {
            ask(from: from, clip: nil, file: file)
        }
    }

    private func autoAccept(_ id: String, from: String, size: UInt64) {
        let messaging = (try? JSONSerialization.jsonObject(with: Data(Messages.shared.webJSON.utf8))) ?? [:]
        guard logic?.bool("lib/messaging", "autoAccept", [messaging, from, size, autoLimit]) ?? false else { return }
        incoming[id]?.auto = true
        save(id)
    }

    /// an accepted name's clip or file: into the thread, a card for the bar, a notification while it's away
    private func show(from: String, clip: String?, file: ThreadFile?) {
        let summary = clip.map(clipSummary) ?? file?.name ?? ""
        MessageHistory.shared.addShare(with: from, mine: false, summary: summary, clip: clip, file: file)
        queued.append(BannerCard(id: "f\(UUID().uuidString)", from: from, text: summary, ask: false, clip: clip, file: file))
        NotificationCenter.default.post(name: .qcalcInboxChanged, object: nil)
        guard !Messages.shared.barOpen else { return }
        let content = UNMutableNotificationContent()
        content.title = from
        content.body = file.map { "sent you \($0.name) (\(sizeText($0.size)))" } ?? "shared their clipboard"
        content.sound = .default
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "share-\(UUID().uuidString)", content: content, trigger: nil))
    }

    /// someone new: their request asks as a message's would, and their share waits until they're allowed
    private func ask(from: String, clip: String?, file: ThreadFile?) {
        held.append((from, clip, file))
        Messages.shared.askShare(from: from)
    }

    /// files waiting on a request from `who`, for its card
    func heldFiles(from who: String) -> [ThreadFile] { held.filter { $0.from == who }.compactMap(\.file) }
    func heldClips(from who: String) -> Int { held.filter { $0.from == who && $0.clip != nil }.count }

    /// allowed, their shares show; blocked, their files are turned down
    func answered(_ who: String, allowed: Bool) {
        let theirs = held.filter { $0.from == who }
        held.removeAll { $0.from == who }
        for h in theirs {
            if allowed {
                if let id = h.file?.id { update(id) { $0.state = "received" } }
                show(from: who, clip: h.clip, file: h.file)
            } else if let id = h.file?.id {
                decline(id)
            }
        }
    }

    /// the bar takes what's come in
    func takeCards() -> [BannerCard] {
        defer { queued = [] }
        return queued
    }

    // MARK: - saving

    /// Save: ask the relay for it (the sender uploads it), then download it once it's ready
    func save(_ id: String) {
        // only an accepted name's, and only one that isn't on its way or saved already
        guard let t = transfers[id], !t.mine, Messages.shared.accepted.contains(t.who), t.state == "received" || t.state == "failed" else { return }
        PrivacyNotice.shared.firstShare()
        guard let inc = incoming[id] else { return update(id) { $0.state = "lost"; $0.error = nil } }
        if inc.expires > 0, inc.expires < Date().timeIntervalSince1970 * 1000 {
            return update(id) { $0.state = "failed"; $0.error = self.errorText("expired", who: t.who) }
        }
        switch t.state {
        case "received", "failed":
            update(id) { $0.state = "accepting"; $0.error = nil }
            call("/file/\(id)/accept") { [weak self] status, _ in
                guard let self else { return }
                switch status {
                case "ok": self.update(id) { if $0.state == "accepting" { $0.state = "waiting" } }
                // already accepted: it may be ready
                case "busy": self.download(id)
                default:
                    self.update(id) { $0.state = "failed"; $0.error = self.errorText(status, who: t.who) }
                }
            }
        default:
            break
        }
    }

    private func download(_ id: String) {
        guard let t = transfers[id], let inc = incoming[id], var r = request("/file/\(id)", method: "GET") else { return }
        r.timeoutInterval = 120
        update(id) { $0.state = "downloading"; $0.percent = 0; $0.error = nil }
        let task = session.downloadTask(with: r) { [weak self] location, response, error in
            guard let self else { return }
            let code = (response as? HTTPURLResponse)?.statusCode ?? 0
            // the file at `location` goes when this returns, so it's moved now
            var kept: URL?
            if let location, code == 200 {
                let dest = Self.temp.appendingPathComponent(UUID().uuidString)
                if (try? FileManager.default.moveItem(at: location, to: dest)) != nil { kept = dest }
            }
            guard let sealed = kept else {
                let body = location.flatMap { try? Data(contentsOf: $0) }
                let (status, _) = Self.status(body, response, error)
                DispatchQueue.main.async {
                    // not uploaded yet: `ready` will come
                    if status == "not-ready" { return self.update(id) { $0.state = "waiting"; $0.percent = nil } }
                    self.update(id) { $0.state = "failed"; $0.percent = nil; $0.error = self.errorText(status, who: t.who) }
                }
                return
            }
            DispatchQueue.main.async {
                self.unwatch(id)
                self.update(id) { $0.state = "decrypting"; $0.percent = nil }
            }
            self.work.async { self.open(id, sealed: sealed, inc: inc, name: t.name, who: t.who) }
        }
        let most = Int64(inc.bytes)
        observers[id, default: []].append(task.progress.observe(\.fractionCompleted) { [weak self, weak task] p, _ in
            // never more than the length that was offered, whatever the relay sends
            if p.completedUnitCount > most { task?.cancel() }
            let pct = Int((p.fractionCompleted * 100).rounded(.down))
            DispatchQueue.main.async { self?.update(id) { if $0.state == "downloading" { $0.percent = pct } } }
        })
        incoming[id]?.task = task
        task.resume()
    }

    /// on the work queue: decrypted beside Downloads, checked (its length, its size and its hash), made not
    /// executable and quarantined, then moved into Downloads under a name nothing has (SafeSave). Nothing partial
    /// is ever left, and it's never opened or shown: Show in Finder is the user's to press
    private func open(_ id: String, sealed: URL, inc: Incoming, name: String, who: String) {
        defer { try? FileManager.default.removeItem(at: sealed) }
        let result: Result<URL, SafeSave.Failure>
        do {
            guard let key = FileCrypt.decode(inc.key),
                  let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first else { throw SafeSave.Failure.disk }
            let numbered: (String, Int) -> String = { [weak self] safe, n in
                Self.safeName(self?.logic?.string("lib/messaging", "numberedName", [safe, n]) ?? "\(n) \(safe)")
            }
            result = .success(try SafeSave.save(sealed, key: key, sha256: inc.sha256, size: inc.size, name: Self.safeName(name),
                                                into: downloads, numbered: numbered, maxSize: Self.fileMax))
        } catch {
            result = .failure(error as? SafeSave.Failure ?? .disk)
        }
        DispatchQueue.main.async {
            switch result {
            case let .success(url):
                self.update(id) { $0.state = "saved"; $0.saved = url }
                self.incoming.removeValue(forKey: id)
            case let .failure(why):
                self.update(id) { $0.state = "failed"; $0.error = self.errorText(why == .disk ? "disk" : "corrupt", who: who) }
            }
        }
    }

    func reveal(_ id: String) {
        if let url = transfers[id]?.saved { NSWorkspace.shared.activateFileViewerSelecting([url]) }
    }

    // MARK: - taking back and turning down

    /// the sender takes it back
    func cancel(_ id: String) {
        guard let t = transfers[id], t.mine else { return }
        outgoing[id]?.cancelled = true
        dropOutgoing(id)
        update(id) { $0.state = "withdrawn"; $0.percent = nil; $0.error = nil }
        call("/file/\(id)", method: "DELETE") { _, _ in }
    }

    /// the recipient turns it down
    func decline(_ id: String) {
        incoming[id]?.task?.cancel()
        incoming.removeValue(forKey: id)
        update(id) { $0.state = "refused"; $0.percent = nil }
        call("/file/\(id)", method: "DELETE") { _, _ in }
    }

    /// a file event from the inbox: accepted (the sender's), ready, cancelled (the recipient's), declined
    func event(_ e: [String: Any]) {
        guard let id = e["id"] as? String, id.range(of: "^[0-9a-f]{32}$", options: .regularExpression) != nil else { return }
        switch e["event"] as? String {
        case "accepted":
            guard let grant = e["grant"] as? String else { return }
            upload(id, grant: grant, partBytes: (e["partBytes"] as? NSNumber)?.uint64Value ?? 50 * 1024 * 1024)
        case "ready":
            if transfers[id]?.state == "waiting" || transfers[id]?.state == "accepting" { download(id) }
        case "cancelled":
            incoming[id]?.task?.cancel()
            incoming.removeValue(forKey: id)
            held.removeAll { $0.file?.id == id }
            update(id) { if $0.state != "saved" { $0.state = "cancelled"; $0.percent = nil; $0.error = nil } }
        case "declined":
            dropOutgoing(id)
            update(id) { $0.state = "declined"; $0.percent = nil; $0.error = nil }
        default:
            break
        }
    }

    func copy(_ clip: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(clip, forType: .string)
    }

    /// a file in the thread or a card: its transfer, or a stand-in once the app's forgotten it
    func transfer(for file: ThreadFile, mine: Bool, who: String) -> Transfer {
        transfers[file.id] ?? Transfer(id: file.id, mine: mine, who: who, name: file.name, size: file.size, state: "lost")
    }
}
