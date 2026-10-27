import CryptoKit
import Foundation
import Security

/// This Mac's end-to-end key and each contact's pinned one (native only; native/PRIVACY.md).
///
/// The X25519 private key is made the first time it's needed and kept in the login Keychain as this device only
/// (never synced, never exported, never sent). Its public half is published with the name on the relay
/// (`POST /pubkey`, with the name's own key) and read back by others through `/lookup`. A contact's key is
/// pinned the first time it's used; if the relay later hands back a different one, nothing more goes to or comes
/// from them until it's trusted in their thread, where both sides can compare the safety code first.
final class ShareKeys: ObservableObject {
    static let shared = ShareKeys()
    private static let pinsKey = "qcalc.messaging.pins"
    /// Q Calc's own item, `com.maxconine.qcalc.share-key`, as before; any other build (Q Calc Dev, a copy under
    /// another id) keeps its own beside it. One shared item meant a dev build beside Q Calc read the other app's
    /// key, which macOS answers with a Keychain password prompt while the bar waits, and both used one identity.
    private static let service: String = {
        let id = Bundle.main.bundleIdentifier ?? "com.maxconine.qcalc"
        return id == "com.maxconine.qcalc" ? "com.maxconine.qcalc.share-key" : "\(id).share-key"
    }()
    private static let account = "x25519"

    /// contact → their pinned public key (base64url)
    @Published private(set) var pins: [String: String]
    /// contacts whose key isn't the pinned one: who → the key the relay has now, until it's trusted
    @Published private(set) var changed: [String: String] = [:]
    private var cached: ShareCrypt.PrivateKey?
    private var published = ""

    private init() {
        pins = (UserDefaults.standard.dictionary(forKey: Self.pinsKey) as? [String: String]) ?? [:]
    }

    private var logic: AppLogic? { MessageFiles.shared.logic }

    // MARK: - this Mac's key

    /// made once and kept in the Keychain; nil only if the Keychain can't hold it
    var privateKey: ShareCrypt.PrivateKey? {
        if let cached { return cached }
        if let raw = Self.read(), let key = try? ShareCrypt.PrivateKey(rawRepresentation: raw) {
            cached = key
            return key
        }
        let key = ShareCrypt.PrivateKey()
        guard Self.write(key.rawRepresentation) else { return nil }
        cached = key
        return key
    }

    var publicText: String { privateKey.map { ShareCrypt.text($0.publicKey) } ?? "" }

    private static func query() -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account,
         kSecAttrSynchronizable as String: kCFBooleanFalse as Any]
    }

    /// Dev and tester builds keep the key in a file only this user can read, in the build's own support folder:
    /// they're signed ad hoc, so each build is a new app to the Keychain, which asked for the login password every
    /// time a new one read the key. A release, signed for real, keeps it in the Keychain.
    private static var keyFile: URL? {
        AppInfo.isDev ? AppInfo.supportDirectory.appendingPathComponent("share-key", isDirectory: false) : nil
    }

    private static func read() -> Data? {
        if let file = keyFile {
            if let data = try? Data(contentsOf: file), data.count == 32 { return data }
            // an earlier dev build's Keychain key, moved over only if it opens without asking (else a new key)
            guard let old = readKeychain(prompt: false), writeFile(old, to: file) else { return nil }
            return old
        }
        return readKeychain(prompt: true)
    }

    private static func writeFile(_ raw: Data, to file: URL) -> Bool {
        let fm = FileManager.default
        try? fm.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard fm.createFile(atPath: file.path, contents: raw, attributes: [.posixPermissions: 0o600]) else { return false }
        return (try? Data(contentsOf: file)) == raw
    }

    private static func readKeychain(prompt: Bool) -> Data? {
        var q = query()
        if !prompt { q[kSecUseAuthenticationUI as String] = kSecUseAuthenticationUIFail }
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data, data.count == 32 else { return nil }
        return data
    }

    private static func write(_ raw: Data) -> Bool {
        if let file = keyFile { return writeFile(raw, to: file) }
        var q = query()
        q[kSecValueData as String] = raw
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        q[kSecAttrLabel as String] = "Q Calc sharing key"
        let status = SecItemAdd(q as CFDictionary, nil)
        return status == errSecSuccess || (status == errSecDuplicateItem && readKeychain(prompt: true) == raw)
    }

    // MARK: - the relay

    private func post(_ path: String, _ body: [String: String], done: @escaping (String, [String: Any]) -> Void) {
        guard let url = URL(string: "\(Messages.relayBase)\(path)"), let data = try? JSONSerialization.data(withJSONObject: body) else {
            return done("bad", [:])
        }
        var r = URLRequest(url: url, timeoutInterval: 15)
        r.httpMethod = "POST"
        r.setValue("text/plain", forHTTPHeaderField: "Content-Type")
        r.httpBody = data
        URLSession.shared.dataTask(with: r) { data, _, _ in
            let o = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] } ?? [:]
            DispatchQueue.main.async { done(o["status"] as? String ?? "network", o) }
        }.resume()
    }

    /// publishes this Mac's public key with the name (after a claim, and once a launch)
    func publish() {
        let m = Messages.shared
        // no name, no key: the Keychain isn't touched (or the key made) for someone who doesn't use messages
        guard !m.name.isEmpty, !m.relayKey.isEmpty else { return }
        let pub = publicText
        guard !pub.isEmpty, published != "\(m.name):\(pub)" else { return }
        post("/pubkey", ["name": m.name, "key": m.relayKey, "pub": pub]) { [weak self] status, _ in
            if status == "ok" { self?.published = "\(m.name):\(pub)" }
        }
    }

    /// publishes again even if this launch already did: something sealed for this Mac didn't open, so the relay
    /// may hold a key it no longer has
    func republish() {
        published = ""
        publish()
    }

    /// What's known of a contact's key.
    enum Found {
        case key(ShareCrypt.PublicKey, String)
        /// not the pinned one: nothing goes until it's trusted
        case changed
        /// they hold the name but their app has published no key (an older copy)
        case none
        case unknown
        /// the relay couldn't say
        case offline(String)
    }

    private func lookup(_ who: String, done: @escaping (String, String) -> Void) {
        let m = Messages.shared
        guard !m.name.isEmpty, !m.relayKey.isEmpty else { return done("not-yours", "") }
        post("/lookup", ["from": m.name, "key": m.relayKey, "name": who]) { status, o in
            done(status, o["pub"] as? String ?? "")
        }
    }

    private func state(_ who: String, _ pub: String) -> String {
        logic?.string("lib/messaging", "pinState", [pins, who, pub]) ?? "bad"
    }

    private func pin(_ who: String, _ pub: String) {
        pins[who] = pub
        UserDefaults.standard.set(pins, forKey: Self.pinsKey)
    }

    /// before sending: the relay's key for `who`, checked against the pin (and pinned the first time)
    func keyFor(sending who: String, done: @escaping (Found) -> Void) {
        publish()
        lookup(who) { [weak self] status, pub in
            guard let self else { return }
            switch status {
            case "found": done(self.found(who, pub))
            case "unknown": done(.unknown)
            default: done(.offline(status))
            }
        }
    }

    private func found(_ who: String, _ pub: String) -> Found {
        // no key published (an older copy of Q Calc): nothing can be sealed for them
        guard !pub.isEmpty else { return .none }
        switch state(who, pub) {
        case "new":
            pin(who, pub)
        case "same":
            break
        case "changed":
            changed[who] = pub
            return .changed
        default:
            return .none
        }
        changed.removeValue(forKey: who)
        guard let key = ShareCrypt.publicKey(pub) else { return .none }
        return .key(key, pub)
    }

    /// on receiving: the pinned key at once, or the relay's, pinned, the first time
    func keyFor(receiving who: String, done: @escaping (Found) -> Void) {
        if changed[who] != nil { return done(.changed) }
        if let pinned = pins[who], let key = ShareCrypt.publicKey(pinned) { return done(.key(key, pinned)) }
        keyFor(sending: who, done: done)
    }

    /// something from `who` didn't open with their pinned key: true when the relay now has another
    func recheck(_ who: String, done: @escaping (Bool) -> Void) {
        lookup(who) { [weak self] status, pub in
            guard let self, status == "found", !pub.isEmpty, let pinned = self.pins[who], pinned != pub else { return done(false) }
            self.changed[who] = pub
            done(true)
        }
    }

    /// their thread's Trust New Key: the new key is pinned, and what was held from them is opened
    func trust(_ who: String) {
        guard let pub = changed.removeValue(forKey: who) else { return }
        pin(who, pub)
        MessageFiles.shared.retryHeld(from: who)
    }

    /// the code both people compare out loud, from both keys; "" until their key is known
    func safetyCode(with who: String) -> String {
        guard let theirs = changed[who] ?? pins[who] else { return "" }
        return logic?.string("lib/messaging", "safetyCode", [publicText, theirs]) ?? ""
    }

    /// their key, fetched and pinned for the thread's info when nothing's pinned yet
    func learn(_ who: String) {
        guard pins[who] == nil, changed[who] == nil else { return }
        keyFor(sending: who) { _ in self.objectWillChange.send() }
    }
}

/// The bar's one-time line on what's encrypted (MessageFiles shows it the first time a file or clip is sent or
/// saved); dismissed, it's remembered and never shows again. Settings › Messages › Privacy says it for good.
final class PrivacyNotice: ObservableObject {
    static let shared = PrivacyNotice()
    static let dismissedKey = "qcalc.messaging.privacyNoticeDismissed"
    @Published private(set) var showing = false

    func firstShare() {
        guard !UserDefaults.standard.bool(forKey: Self.dismissedKey) else { return }
        showing = true
    }

    func dismiss() {
        showing = false
        UserDefaults.standard.set(true, forKey: Self.dismissedKey)
    }
}
