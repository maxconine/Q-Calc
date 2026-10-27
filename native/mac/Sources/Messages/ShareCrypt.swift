import CryptoKit
import Foundation

/// End-to-end sealing for clips and file offers (native only; native/PRIVACY.md). Foundation and CryptoKit only, so
/// the headless tests (native/mac/scripts/test-share.sh) build it on its own.
///
/// Each app has an X25519 key pair (ShareKeys keeps the private half in the Keychain). To seal for a contact, the
/// sender draws an ephemeral key pair and derives a one-time key with HKDF-SHA256 over two agreements: the
/// ephemeral key's with the recipient's key, then the sender's own key's with the recipient's. The salt is the
/// scheme's version and the info is the core's `shareContext` (the version, `file` or `clip`, both names, the id
/// and both public keys), which is also the GCM associated data. Only the recipient's private key opens it, and
/// only the sender's could have made it; a box moved to another id, name or kind doesn't open.
enum ShareCrypt {
    static let version = "qcalc-share-v1"

    enum Failure: Error { case badKey, sealed }

    typealias PrivateKey = Curve25519.KeyAgreement.PrivateKey
    typealias PublicKey = Curve25519.KeyAgreement.PublicKey

    static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }

    static func decode(_ text: String) -> Data? {
        guard text.range(of: "^[A-Za-z0-9_-]*$", options: .regularExpression) != nil, text.count % 4 != 1 else { return nil }
        var b64 = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while b64.count % 4 != 0 { b64 += "=" }
        return Data(base64Encoded: b64)
    }

    static func publicKey(_ text: String) -> PublicKey? {
        guard text.count == 43, let raw = decode(text), raw.count == 32, raw.contains(where: { $0 != 0 }) else { return nil }
        return try? PublicKey(rawRepresentation: raw)
    }

    static func text(_ key: PublicKey) -> String { encode(key.rawRepresentation) }

    /// a share's id: 16 random bytes as hex, drawn by the sender so the seal can be bound to it
    static func newId() -> String {
        var bytes = [UInt8](repeating: 0, count: 16)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            return (0..<16).map { _ in String(format: "%02x", UInt8.random(in: 0...255)) }.joined()
        }
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    private static func secret(_ mine: PrivateKey, _ theirs: PublicKey) throws -> Data {
        let shared = try mine.sharedSecretFromKeyAgreement(with: theirs)
        let bytes = shared.withUnsafeBytes { Data($0) }
        // a low-order point agrees on zero: never a key
        guard bytes.contains(where: { $0 != 0 }) else { throw Failure.badKey }
        return bytes
    }

    private static func wrapKey(_ ephemeral: Data, _ fixed: Data, context: String) -> SymmetricKey {
        HKDF<SHA256>.deriveKey(inputKeyMaterial: SymmetricKey(data: ephemeral + fixed), salt: Data(version.utf8),
                               info: Data(context.utf8), outputByteCount: 32)
    }

    /// `plain` sealed for `recipient` by `me`: the ephemeral public key and the box (nonce, ciphertext, tag), both
    /// base64url. `context` is the core's shareContext for this share.
    static func seal(_ plain: Data, me: PrivateKey, to recipient: PublicKey, context: String) throws -> (epk: String, sealed: String) {
        let ephemeral = PrivateKey()
        let key = try wrapKey(secret(ephemeral, recipient), secret(me, recipient), context: context)
        let box = try AES.GCM.seal(plain, using: key, authenticating: Data(context.utf8))
        guard let combined = box.combined else { throw Failure.sealed }
        return (text(ephemeral.publicKey), encode(combined))
    }

    /// a box sealed for `me` by `sender`, opened; throws `sealed` when it isn't theirs, for us, for this context
    static func open(_ sealed: String, epk: String, me: PrivateKey, from sender: PublicKey, context: String) throws -> Data {
        guard let ephemeral = publicKey(epk), let combined = decode(sealed), combined.count >= 12 + 16 else { throw Failure.sealed }
        let key = try wrapKey(secret(me, ephemeral), secret(me, sender), context: context)
        guard let box = try? AES.GCM.SealedBox(combined: combined),
              let plain = try? AES.GCM.open(box, using: key, authenticating: Data(context.utf8)) else { throw Failure.sealed }
        return plain
    }
}

/// Where a received file goes: decrypted into a temporary file on the same volume as Downloads, checked, made
/// non-executable and quarantined there, then moved into place under a name nothing has. Anything that fails
/// leaves nothing behind. Nothing here opens, previews or runs it.
enum SafeSave {
    enum Failure: Error { case size, corrupt, disk }

    /// `sealed` (the encrypted download) checked and saved into `dir` as `name` (already cleaned), or as the next
    /// free numbered name. `size` is the plain size the sender sealed; the download must be exactly its encrypted
    /// length and decrypt to exactly that many bytes with that SHA-256.
    static func save(_ sealed: URL, key: SymmetricKey, sha256: String, size: UInt64, name: String, into dir: URL,
                     numbered: (String, Int) -> String, maxSize: UInt64) throws -> URL {
        let fm = FileManager.default
        // the size limits again, on this side
        guard size <= maxSize,
              let got = (try? sealed.resourceValues(forKeys: [.fileSizeKey]))?.fileSize, UInt64(got) == FileCrypt.encryptedLength(size) else {
            throw Failure.size
        }
        let scratch = (try? fm.url(for: .itemReplacementDirectory, in: .userDomainMask, appropriateFor: dir, create: true))
            ?? fm.temporaryDirectory.appendingPathComponent("QCalcSave-\(UUID().uuidString)", isDirectory: true)
        try? fm.createDirectory(at: scratch, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: scratch) }
        let partial = scratch.appendingPathComponent(UUID().uuidString, isDirectory: false)
        do {
            try FileCrypt.decrypt(sealed, to: partial, key: key, sha256: sha256, size: size)
        } catch {
            try? fm.removeItem(at: partial)
            throw (error as? FileCrypt.Failure) == .disk ? Failure.disk : Failure.corrupt
        }
        // never runnable as it lands, and marked as from elsewhere before it's anywhere it can be seen
        guard (try? fm.setAttributes([.posixPermissions: 0o644], ofItemAtPath: partial.path)) != nil, quarantine(partial) else {
            try? fm.removeItem(at: partial)
            throw Failure.disk
        }
        for n in 1...999 {
            let candidate = n == 1 ? name : numbered(name, n)
            guard !candidate.contains("/"), !candidate.hasPrefix(".") else { continue }
            let dest = dir.appendingPathComponent(candidate, isDirectory: false)
            guard dest.deletingLastPathComponent().standardizedFileURL == dir.standardizedFileURL, !fm.fileExists(atPath: dest.path) else { continue }
            // moveItem never replaces, so a race loses only a try
            if (try? fm.moveItem(at: partial, to: dest)) != nil { return dest }
        }
        try? fm.removeItem(at: partial)
        throw Failure.disk
    }

    /// marks a file as downloaded from elsewhere, so Gatekeeper asks before anything of it runs; false if it
    /// couldn't be marked
    @discardableResult
    static func quarantine(_ url: URL) -> Bool {
        let value = String(format: "0081;%08lx;Q Calc;%@", Int(Date().timeIntervalSince1970), UUID().uuidString)
        let set = value.withCString { v in url.withUnsafeFileSystemRepresentation { p in p.map { setxattr($0, "com.apple.quarantine", v, strlen(v), 0, 0) } ?? -1 } }
        return set == 0
    }

    static func quarantined(_ url: URL) -> Bool {
        url.withUnsafeFileSystemRepresentation { p in p.map { getxattr($0, "com.apple.quarantine", nil, 0, 0, 0) > 0 } ?? false }
    }
}
