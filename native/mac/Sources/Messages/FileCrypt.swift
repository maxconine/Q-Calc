import CryptoKit
import Foundation

/// How a file is encrypted for the relay (native only; relay/README.md, "Files and clips"). A fresh AES-256 key
/// per file, sent only inside the offer message. The file is cut into chunks of `chunk` plain bytes (one chunk,
/// maybe empty, at least), and each is sealed with AES-GCM on its own: the nonce is the chunk's index (4 zero
/// bytes and 8 big-endian), and the associated data is the index and whether it's the last chunk, so chunks
/// can't be reordered, dropped or cut off the end. The upload is each chunk's ciphertext and 16-byte tag in
/// order, `encryptedLength(size)` bytes in all. The SHA-256 of the plain bytes rides in the offer and is checked
/// after decrypting.
enum FileCrypt {
    static let chunk = 1024 * 1024
    static let tag = 16

    enum Failure: Error { case unreadable, corrupt, disk }

    static func encryptedLength(_ size: UInt64) -> UInt64 {
        let chunks = max(1, (size + UInt64(chunk) - 1) / UInt64(chunk))
        return size + UInt64(tag) * chunks
    }

    static func newKey() -> SymmetricKey { SymmetricKey(size: .bits256) }

    static func encode(_ key: SymmetricKey) -> String {
        key.withUnsafeBytes { Data($0) }.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }

    static func decode(_ text: String) -> SymmetricKey? {
        var b64 = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while b64.count % 4 != 0 { b64 += "=" }
        guard let data = Data(base64Encoded: b64), data.count == 32 else { return nil }
        return SymmetricKey(data: data)
    }

    private static func nonce(_ i: UInt64) -> AES.GCM.Nonce {
        var bytes = Data(count: 4)
        withUnsafeBytes(of: i.bigEndian) { bytes.append(contentsOf: $0) }
        return try! AES.GCM.Nonce(data: bytes)
    }

    private static func aad(_ i: UInt64, last: Bool) -> Data {
        var bytes = Data()
        withUnsafeBytes(of: i.bigEndian) { bytes.append(contentsOf: $0) }
        bytes.append(last ? 1 : 0)
        return bytes
    }

    static func hex(_ digest: SHA256.Digest) -> String { digest.map { String(format: "%02x", $0) }.joined() }

    /// `source` sealed into `dest`; the plain bytes' SHA-256 as hex. `progress` hears the fraction done.
    static func encrypt(_ source: URL, to dest: URL, key: SymmetricKey, progress: (Double) -> Void = { _ in }) throws -> String {
        guard let size = (try? source.resourceValues(forKeys: [.fileSizeKey]))?.fileSize, let input = try? FileHandle(forReadingFrom: source) else {
            throw Failure.unreadable
        }
        defer { try? input.close() }
        guard FileManager.default.createFile(atPath: dest.path, contents: nil), let output = try? FileHandle(forWritingTo: dest) else { throw Failure.disk }
        defer { try? output.close() }
        let total = UInt64(size)
        let chunks = max(1, (total + UInt64(chunk) - 1) / UInt64(chunk))
        var hash = SHA256()
        var read: UInt64 = 0
        for i in 0..<chunks {
            let plain = try autoreleasepool { () throws -> Data in
                let want = Int(min(UInt64(chunk), total - read))
                let data = want == 0 ? Data() : (try? input.read(upToCount: want)) ?? Data()
                guard data.count == want else { throw Failure.unreadable }
                return data
            }
            read += UInt64(plain.count)
            hash.update(data: plain)
            let box = try AES.GCM.seal(plain, using: key, nonce: nonce(i), authenticating: aad(i, last: i == chunks - 1))
            do {
                try output.write(contentsOf: box.ciphertext + box.tag)
            } catch {
                throw Failure.disk
            }
            progress(Double(i + 1) / Double(chunks))
        }
        return hex(hash.finalize())
    }

    /// `source` opened into `dest`, checked against `sha256`; throws `corrupt` when anything doesn't match.
    static func decrypt(_ source: URL, to dest: URL, key: SymmetricKey, sha256: String) throws {
        guard let size = (try? source.resourceValues(forKeys: [.fileSizeKey]))?.fileSize, let input = try? FileHandle(forReadingFrom: source) else {
            throw Failure.corrupt
        }
        defer { try? input.close() }
        guard FileManager.default.createFile(atPath: dest.path, contents: nil), let output = try? FileHandle(forWritingTo: dest) else { throw Failure.disk }
        defer { try? output.close() }
        let sealed = UInt64(chunk + tag)
        let total = UInt64(size)
        let chunks = max(1, (total + sealed - 1) / sealed)
        var hash = SHA256()
        for i in 0..<chunks {
            let plain = try autoreleasepool { () throws -> Data in
                let data = (try? input.read(upToCount: Int(sealed))) ?? Data()
                guard data.count >= tag else { throw Failure.corrupt }
                guard let box = try? AES.GCM.SealedBox(nonce: nonce(i), ciphertext: data.dropLast(tag), tag: data.suffix(tag)),
                      let plain = try? AES.GCM.open(box, using: key, authenticating: aad(i, last: i == chunks - 1)) else { throw Failure.corrupt }
                return plain
            }
            hash.update(data: plain)
            do {
                try output.write(contentsOf: plain)
            } catch {
                throw Failure.disk
            }
        }
        guard hex(hash.finalize()) == sha256 else { throw Failure.corrupt }
    }
}
