import CryptoKit
import Foundation

// arithmetic mod 2^255 - 19, just enough for the elligator map. values stay under 2^256 between steps
// and are only reduced fully for comparisons and bytes
struct Field25519: Equatable {
    private(set) var limbs: [UInt64]

    static let p: [UInt64] = [0xFFFF_FFFF_FFFF_FFED, .max, .max, 0x7FFF_FFFF_FFFF_FFFF]
    static let pMinus2: [UInt64] = [0xFFFF_FFFF_FFFF_FFEB, .max, .max, 0x7FFF_FFFF_FFFF_FFFF]
    static let halfPMinus1: [UInt64] = [0xFFFF_FFFF_FFFF_FFF6, .max, .max, 0x3FFF_FFFF_FFFF_FFFF]
    static let zero = Field25519(0)
    static let one = Field25519(1)

    init(_ n: UInt64) {
        limbs = [n, 0, 0, 0]
    }

    init(limbs: [UInt64]) {
        self.limbs = limbs
    }

    // little endian with the top bit dropped, as rfc 7748 reads a u-coordinate
    init(bytes: [UInt8]) {
        var l = [UInt64](repeating: 0, count: 4)
        for i in 0..<min(32, bytes.count) {
            l[i / 8] |= UInt64(bytes[i]) << (8 * UInt64(i % 8))
        }
        l[3] &= 0x7FFF_FFFF_FFFF_FFFF
        limbs = l
    }

    var canonical: Field25519 {
        var x = limbs
        while !Self.less(x, Self.p) { x = Self.subtract(x, Self.p) }
        return Field25519(limbs: x)
    }

    var bytes: [UInt8] {
        let c = canonical.limbs
        return (0..<32).map { UInt8(truncatingIfNeeded: c[$0 / 8] >> (8 * UInt64($0 % 8))) }
    }

    static func == (a: Field25519, b: Field25519) -> Bool {
        a.canonical.limbs == b.canonical.limbs
    }

    private static func less(_ a: [UInt64], _ b: [UInt64]) -> Bool {
        for i in (0..<4).reversed() where a[i] != b[i] {
            return a[i] < b[i]
        }
        return false
    }

    // a - b for a >= b
    private static func subtract(_ a: [UInt64], _ b: [UInt64]) -> [UInt64] {
        var out = [UInt64](repeating: 0, count: 4)
        var borrow: UInt64 = 0
        for i in 0..<4 {
            let (d1, o1) = a[i].subtractingReportingOverflow(b[i])
            let (d2, o2) = d1.subtractingReportingOverflow(borrow)
            out[i] = d2
            borrow = o1 || o2 ? 1 : 0
        }
        return out
    }

    // 2^256 is 38 mod p, so whatever spills past the top limb comes back in at the bottom
    private static func fold(_ out: inout [UInt64], _ spill: UInt64) {
        var c = spill &* 38
        while c != 0 {
            var carry = c
            for i in 0..<4 where carry != 0 {
                let (s, o) = out[i].addingReportingOverflow(carry)
                out[i] = s
                carry = o ? 1 : 0
            }
            c = carry &* 38
        }
    }

    static func + (a: Field25519, b: Field25519) -> Field25519 {
        var out = [UInt64](repeating: 0, count: 4)
        var carry: UInt64 = 0
        for i in 0..<4 {
            let (s1, o1) = a.limbs[i].addingReportingOverflow(b.limbs[i])
            let (s2, o2) = s1.addingReportingOverflow(carry)
            out[i] = s2
            carry = (o1 ? 1 : 0) + (o2 ? 1 : 0)
        }
        fold(&out, carry)
        return Field25519(limbs: out)
    }

    static prefix func - (a: Field25519) -> Field25519 {
        Field25519(limbs: subtract(p, a.canonical.limbs)).canonical
    }

    static func - (a: Field25519, b: Field25519) -> Field25519 {
        a + -b
    }

    static func * (a: Field25519, b: Field25519) -> Field25519 {
        var wide = [UInt64](repeating: 0, count: 8)
        for i in 0..<4 {
            var carry: UInt64 = 0
            for j in 0..<4 {
                let (hi, lo) = a.limbs[i].multipliedFullWidth(by: b.limbs[j])
                let (s1, o1) = wide[i + j].addingReportingOverflow(lo)
                let (s2, o2) = s1.addingReportingOverflow(carry)
                wide[i + j] = s2
                carry = hi &+ (o1 ? 1 : 0) &+ (o2 ? 1 : 0)
            }
            wide[i + 4] = carry
        }
        var out = [UInt64](repeating: 0, count: 4)
        var carry: UInt64 = 0
        for i in 0..<4 {
            let (hi, lo) = wide[i + 4].multipliedFullWidth(by: 38)
            let (s1, o1) = wide[i].addingReportingOverflow(lo)
            let (s2, o2) = s1.addingReportingOverflow(carry)
            out[i] = s2
            carry = hi &+ (o1 ? 1 : 0) &+ (o2 ? 1 : 0)
        }
        fold(&out, carry)
        return Field25519(limbs: out)
    }

    func power(_ e: [UInt64]) -> Field25519 {
        var result = Field25519.one
        for i in (0..<4).reversed() {
            for bit in (0..<64).reversed() {
                result = result * result
                if (e[i] >> UInt64(bit)) & 1 == 1 { result = result * self }
            }
        }
        return result
    }

    // zero has no inverse and comes back as zero
    var inverse: Field25519 { power(Self.pMinus2) }

    var isSquare: Bool {
        let l = power(Self.halfPMinus1)
        return l == .zero || l == .one
    }
}

// rfc 9380's elligator 2 for curve25519, u-coordinate only since x25519 needs nothing else
enum Elligator25519 {
    static let a = Field25519(486_662)
    static let z = Field25519(2)

    static func curveRight(_ u: Field25519) -> Field25519 {
        u * (u * u + a * u + .one)
    }

    static func map(_ r: Field25519) -> Field25519 {
        var t = z * r * r
        if t == -Field25519.one { t = .zero }
        let x1 = -a * (Field25519.one + t).inverse
        return curveRight(x1).isSquare ? x1.canonical : (-x1 - a).canonical
    }
}

struct PeerKeys {
    let send: SymmetricKey
    let receive: SymmetricKey
}

enum HandshakeStep {
    case send(Data)
    case paired(PeerKeys, reply: Data?)
    case wrongCode
    case invalid
}

// pairing is a cpace-style pake over x25519: both sides derive a generator from the code and a fresh session id,
// swap one share each, and confirm the key. a wrong code can't finish, and a transcript gives nothing to guess
// the code against offline, so each try at the 4 digits costs a round trip the host can count.
//   host  → guest  01 "QCP1" sid
//   guest → host   02 guest share
//   host  → guest  03 host share, host confirmation
//   guest → host   04 guest confirmation
final class PeerHandshake {
    enum Role { case host, guest }

    static let magic = Data("QCP1".utf8)
    static let hello: UInt8 = 0x01
    static let guestShare: UInt8 = 0x02
    static let hostShare: UInt8 = 0x03
    static let guestConfirm: UInt8 = 0x04
    static let busy: UInt8 = 0x05

    let role: Role
    private let code: String
    private let secret = Curve25519.KeyAgreement.PrivateKey()
    private var sid = Data()
    private var guestY = Data()
    private var hostY = Data()
    private var keys: (confirmHost: SymmetricKey, confirmGuest: SymmetricKey, toGuest: SymmetricKey, toHost: SymmetricKey)?
    // host: a share came in, so this connection spent one try at the code
    private(set) var guessed = false

    init(role: Role, code: String) {
        self.role = role
        self.code = code
    }

    static func isCode(_ s: String) -> Bool {
        s.count == 4 && s.allSatisfy { $0.isASCII && $0.isNumber }
    }

    static func newCode() -> String {
        String(format: "%04d", Int.random(in: 0...9999))
    }

    static func lengthPrefixed(_ parts: [Data]) -> Data {
        var out = Data()
        for part in parts {
            var n = UInt32(part.count).bigEndian
            out.append(Data(bytes: &n, count: 4))
            out.append(part)
        }
        return out
    }

    static func generator(code: String, sid: Data) -> Data {
        let digest = Array(SHA512.hash(data: lengthPrefixed([Data("CPace255".utf8), Data("qcalc peer code \(code)".utf8), sid])))
        return Data(Elligator25519.map(Field25519(bytes: Array(digest.prefix(32)))).bytes)
    }

    // x25519 against any u-coordinate; cryptokit throws on a low-order point
    private func multiply(_ point: Data) -> Data? {
        guard point.count == 32,
              let key = try? Curve25519.KeyAgreement.PublicKey(rawRepresentation: point),
              let shared = try? secret.sharedSecretFromKeyAgreement(with: key)
        else { return nil }
        return shared.withUnsafeBytes { Data($0) }
    }

    private func derive(_ k: Data) {
        let isk = SHA512.hash(data: Self.lengthPrefixed([Data("CPace255_ISK".utf8), sid, k, guestY, hostY]))
        let ikm = SymmetricKey(data: Data(isk))
        func key(_ label: String) -> SymmetricKey {
            HKDF<SHA256>.deriveKey(inputKeyMaterial: ikm, salt: sid, info: Data(label.utf8), outputByteCount: 32)
        }
        keys = (key("qcalc confirm host"), key("qcalc confirm guest"), key("qcalc to guest"), key("qcalc to host"))
    }

    private func confirmation(_ key: SymmetricKey, _ label: String) -> Data {
        Data(HMAC<SHA256>.authenticationCode(for: Data(label.utf8) + guestY + hostY, using: key))
    }

    // host only: the first frame on a new connection
    func start() -> Data {
        var bytes = [UInt8](repeating: 0, count: 16)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        sid = Data(bytes)
        return Data([Self.hello]) + Self.magic + sid
    }

    func receive(_ frame: Data) -> HandshakeStep {
        let frame = Data(frame)
        guard let kind = frame.first else { return .invalid }
        let body = frame.dropFirst()
        switch (role, kind) {
        case (.guest, Self.hello):
            guard sid.isEmpty, body.count == 20, body.prefix(4) == Self.magic, Self.isCode(code) else { return .invalid }
            sid = Data(body.dropFirst(4))
            guard let share = multiply(Self.generator(code: code, sid: sid)) else { return .invalid }
            guestY = share
            return .send(Data([Self.guestShare]) + share)
        case (.host, Self.guestShare):
            guard !sid.isEmpty, guestY.isEmpty, body.count == 32 else { return .invalid }
            guessed = true
            guestY = Data(body)
            guard let share = multiply(Self.generator(code: code, sid: sid)), let k = multiply(guestY) else { return .invalid }
            hostY = share
            derive(k)
            guard let keys else { return .invalid }
            return .send(Data([Self.hostShare]) + share + confirmation(keys.confirmHost, "host"))
        case (.guest, Self.hostShare):
            guard !guestY.isEmpty, hostY.isEmpty, body.count == 64 else { return .invalid }
            hostY = Data(body.prefix(32))
            guard let k = multiply(hostY) else { return .invalid }
            derive(k)
            guard let keys else { return .invalid }
            let mac = Data(body.suffix(32))
            guard HMAC<SHA256>.isValidAuthenticationCode(mac, authenticating: Data("host".utf8) + guestY + hostY, using: keys.confirmHost) else {
                return .wrongCode
            }
            return .paired(PeerKeys(send: keys.toHost, receive: keys.toGuest), reply: Data([Self.guestConfirm]) + confirmation(keys.confirmGuest, "guest"))
        case (.host, Self.guestConfirm):
            guard let keys, body.count == 32 else { return .invalid }
            guard HMAC<SHA256>.isValidAuthenticationCode(Data(body), authenticating: Data("guest".utf8) + guestY + hostY, using: keys.confirmGuest) else {
                return .wrongCode
            }
            return .paired(PeerKeys(send: keys.toGuest, receive: keys.toHost), reply: nil)
        default:
            return .invalid
        }
    }
}

// after pairing every frame is chacha20-poly1305 under a per-direction key, with a counter for the nonce
final class PeerCipher {
    static let sealed: UInt8 = 0x10
    private let keys: PeerKeys
    private var sent: UInt64 = 0
    private var received: UInt64 = 0

    init(_ keys: PeerKeys) {
        self.keys = keys
    }

    private static func nonce(_ n: UInt64) -> ChaChaPoly.Nonce? {
        var be = n.bigEndian
        return try? ChaChaPoly.Nonce(data: Data(count: 4) + Data(bytes: &be, count: 8))
    }

    func seal(_ plain: Data) -> Data? {
        guard let nonce = Self.nonce(sent), let box = try? ChaChaPoly.seal(plain, using: keys.send, nonce: nonce) else { return nil }
        sent += 1
        return Data([Self.sealed]) + box.ciphertext + box.tag
    }

    func open(_ frame: Data) -> Data? {
        let frame = Data(frame)
        guard frame.first == Self.sealed, frame.count >= 17, let nonce = Self.nonce(received),
              let box = try? ChaChaPoly.SealedBox(nonce: nonce, ciphertext: frame.dropFirst().dropLast(16), tag: frame.suffix(16)),
              let plain = try? ChaChaPoly.open(box, using: keys.receive)
        else { return nil }
        received += 1
        return plain
    }
}
