// Headless checks for end-to-end sharing (native/mac/scripts/test-share.sh): ShareCrypt's seal and open, FileCrypt,
// and SafeSave's checked, quarantined, never-overwriting save. Foundation and CryptoKit only: no app, no window,
// no Keychain (the keys here live in memory).

import CryptoKit
import Foundation

var failures = 0

func check(_ what: String, _ ok: Bool) {
    print(ok ? "ok   \(what)" : "FAIL \(what)")
    if !ok { failures += 1 }
}

func throwsError(_ body: () throws -> Void) -> Bool {
    do { try body(); return false } catch { return true }
}

// the core's shareContext, as messaging.rs builds it (its test pins the same layout)
func context(_ kind: String, _ from: String, _ to: String, _ id: String, _ fromKey: ShareCrypt.PublicKey, _ toKey: ShareCrypt.PublicKey) -> String {
    "qcalc-share-v1\n\(kind)\n\(from)\n\(to)\n\(id)\n\(ShareCrypt.text(fromKey))\n\(ShareCrypt.text(toKey))"
}

// ---- sealing ----

let ana = ShareCrypt.PrivateKey()
let max2 = ShareCrypt.PrivateKey()
let eve = ShareCrypt.PrivateKey()
let id = ShareCrypt.newId()
check("ids are 32 hex characters", id.range(of: "^[0-9a-f]{32}$", options: .regularExpression) != nil && id != ShareCrypt.newId())
let ctx = context("file", "ana", "max2", id, ana.publicKey, max2.publicKey)
let facts = Data(#"{"v":1,"name":"notes.pdf"}"#.utf8)

do {
    let (epk, sealed) = try ShareCrypt.seal(facts, me: ana, to: max2.publicKey, context: ctx)
    check("the relay sees no plain facts", !sealed.contains("notes") && ShareCrypt.decode(sealed).map { !String(decoding: $0, as: UTF8.self).contains("notes") } == true)
    check("public keys are 43 characters of base64url", epk.count == 43 && ShareCrypt.publicKey(epk) != nil)
    let opened = try ShareCrypt.open(sealed, epk: epk, me: max2, from: ana.publicKey, context: ctx)
    check("the recipient opens it", opened == facts)
    check("nobody else can", throwsError { _ = try ShareCrypt.open(sealed, epk: epk, me: eve, from: ana.publicKey, context: ctx) })
    check("it doesn't open as from someone else", throwsError { _ = try ShareCrypt.open(sealed, epk: epk, me: max2, from: eve.publicKey, context: ctx) })
    let otherId = context("file", "ana", "max2", ShareCrypt.newId(), ana.publicKey, max2.publicKey)
    check("it doesn't open under another id", throwsError { _ = try ShareCrypt.open(sealed, epk: epk, me: max2, from: ana.publicKey, context: otherId) })
    let asClip = context("clip", "ana", "max2", id, ana.publicKey, max2.publicKey)
    check("a file's box doesn't open as a clip", throwsError { _ = try ShareCrypt.open(sealed, epk: epk, me: max2, from: ana.publicKey, context: asClip) })
    let otherNames = context("file", "eve", "max2", id, ana.publicKey, max2.publicKey)
    check("it doesn't open under other names", throwsError { _ = try ShareCrypt.open(sealed, epk: epk, me: max2, from: ana.publicKey, context: otherNames) })
    var raw = ShareCrypt.decode(sealed)!
    raw[raw.count - 20] ^= 1
    check("a changed byte doesn't open", throwsError { _ = try ShareCrypt.open(ShareCrypt.encode(raw), epk: epk, me: max2, from: ana.publicKey, context: ctx) })
    let otherEpk = ShareCrypt.text(ShareCrypt.PrivateKey().publicKey)
    check("another ephemeral key doesn't open", throwsError { _ = try ShareCrypt.open(sealed, epk: otherEpk, me: max2, from: ana.publicKey, context: ctx) })
    check("an all-zero key isn't one", ShareCrypt.publicKey(String(repeating: "A", count: 43)) == nil)
    check("a short box doesn't open", throwsError { _ = try ShareCrypt.open("AAAA", epk: epk, me: max2, from: ana.publicKey, context: ctx) })
    let (_, again) = try ShareCrypt.seal(facts, me: ana, to: max2.publicKey, context: ctx)
    check("sealing twice gives different boxes", again != sealed)
} catch {
    check("sealing works: \(error)", false)
}

// ---- files: encrypt, then save into a folder, checked and quarantined ----

let fm = FileManager.default
let root = fm.temporaryDirectory.appendingPathComponent("qcalc-share-test-\(UUID().uuidString)", isDirectory: true)
let downloads = root.appendingPathComponent("Downloads", isDirectory: true)
try? fm.createDirectory(at: downloads, withIntermediateDirectories: true)
defer { try? fm.removeItem(at: root) }

let source = root.appendingPathComponent("source.bin")
var plain = Data(count: FileCrypt.chunk * 2 + 1234)
for i in plain.indices { plain[i] = UInt8(truncatingIfNeeded: i &* 31) }
try? plain.write(to: source)
let sealedFile = root.appendingPathComponent("sealed.bin")
let key = FileCrypt.newKey()
let numbered: (String, Int) -> String = { name, n in
    let ext = (name as NSString).pathExtension
    let stem = (name as NSString).deletingPathExtension
    return ext.isEmpty ? "\(name) \(n)" : "\(stem) \(n).\(ext)"
}
let listing = { (try? fm.contentsOfDirectory(atPath: downloads.path).sorted()) ?? [] }

do {
    let sha = try FileCrypt.encrypt(source, to: sealedFile, key: key)
    let size = UInt64(plain.count)
    let sealedSize = (try? sealedFile.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
    check("the upload is exactly the encrypted length", UInt64(sealedSize) == FileCrypt.encryptedLength(size))
    let saved = try SafeSave.save(sealedFile, key: key, sha256: sha, size: size, name: "notes.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30)
    check("it lands under its name", saved.lastPathComponent == "notes.pdf" && (try? Data(contentsOf: saved)) == plain)
    check("it's quarantined", SafeSave.quarantined(saved))
    let perms = (try? fm.attributesOfItem(atPath: saved.path)[.posixPermissions] as? NSNumber)?.intValue ?? -1
    check("it can't be run as it lands", perms == 0o644)
    let second = try SafeSave.save(sealedFile, key: key, sha256: sha, size: size, name: "notes.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30)
    check("a name that's taken is never replaced", second.lastPathComponent == "notes 2.pdf")
    let before = listing()

    // a changed byte, the wrong hash, the wrong size, a cut-short download, a size over the limit: nothing lands
    var bad = (try? Data(contentsOf: sealedFile)) ?? Data()
    bad[100] ^= 1
    let corrupt = root.appendingPathComponent("corrupt.bin")
    try? bad.write(to: corrupt)
    check("a changed byte isn't saved", throwsError { _ = try SafeSave.save(corrupt, key: key, sha256: sha, size: size, name: "x.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30) })
    check("the wrong hash isn't saved", throwsError { _ = try SafeSave.save(sealedFile, key: key, sha256: String(repeating: "0", count: 64), size: size, name: "x.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30) })
    check("another size isn't saved", throwsError { _ = try SafeSave.save(sealedFile, key: key, sha256: sha, size: size - 1, name: "x.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30) })
    let short = root.appendingPathComponent("short.bin")
    try? Data(((try? Data(contentsOf: sealedFile)) ?? Data()).prefix(FileCrypt.chunk + 16)).write(to: short)
    check("a cut-short download isn't saved", throwsError { _ = try SafeSave.save(short, key: key, sha256: sha, size: size, name: "x.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30) })
    check("a file over the limit isn't saved", throwsError { _ = try SafeSave.save(sealedFile, key: key, sha256: sha, size: size, name: "x.pdf", into: downloads, numbered: numbered, maxSize: size - 1) })
    check("the wrong key isn't saved", throwsError { _ = try SafeSave.save(sealedFile, key: FileCrypt.newKey(), sha256: sha, size: size, name: "x.pdf", into: downloads, numbered: numbered, maxSize: 1 << 30) })
    check("nothing partial is left behind", listing() == before)

    // an empty file is one sealed chunk
    let empty = root.appendingPathComponent("empty")
    try? Data().write(to: empty)
    let emptySealed = root.appendingPathComponent("empty.sealed")
    let emptySha = try FileCrypt.encrypt(empty, to: emptySealed, key: key)
    let e = try SafeSave.save(emptySealed, key: key, sha256: emptySha, size: 0, name: "empty.txt", into: downloads, numbered: numbered, maxSize: 1 << 30)
    check("an empty file saves empty", (try? Data(contentsOf: e))?.isEmpty == true)
} catch {
    check("saving works: \(error)", false)
}

print(failures == 0 ? "all share checks passed" : "\(failures) share check(s) failed")
exit(failures == 0 ? 0 : 1)
