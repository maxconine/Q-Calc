// The Mac's sealing, written again with Node's crypto (OpenSSL) straight from native/mac/Sources/Messages/
// ShareCrypt.swift and FileCrypt.swift, so share-e2e.ps1 can check the Windows app against it both ways:
//
//   node share-vectors.mjs make <dir>   a clip, a file's offer and a 2.5 MiB file (3 chunks) sealed as a Mac seals
//                                       them, for the app to open (`share {"op":"vector-open"}`)  → <dir>/mac.json
//   node share-vectors.mjs open <dir>   what the app sealed for the same recipient (`vector-seal`, <dir>/win.json)
//                                       opened as a Mac opens it                                    → prints JSON
//
// The construction (native/PRIVACY.md): X25519; the one-time key is HKDF-SHA256(ephemeral·recipient ‖
// sender·recipient, salt "qcalc-share-v1", info = the share context), AES-256-GCM with the context as associated
// data, the box nonce ‖ ciphertext ‖ tag in base64url. Files: 1 MiB chunks, nonce 0000 ‖ index (8, big-endian),
// associated data index ‖ last, each chunk's ciphertext ‖ tag.
import crypto from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

const VERSION = 'qcalc-share-v1'
const CHUNK = 1024 * 1024
const TAG = 16

const b64u = (b) => Buffer.from(b).toString('base64url')
const unb64u = (s) => Buffer.from(s, 'base64url')

function privateKey(raw) {
  return crypto.createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b656e04220420', 'hex'), raw]), format: 'der', type: 'pkcs8' })
}
function publicKey(raw) {
  return crypto.createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b656e032100', 'hex'), raw]), format: 'der', type: 'spki' })
}
const publicRaw = (priv) => crypto.createPublicKey(priv).export({ format: 'der', type: 'spki' }).subarray(-32)
function agree(priv, pubRaw) {
  const s = crypto.diffieHellman({ privateKey: priv, publicKey: publicKey(pubRaw) })
  if (s.every((b) => b === 0)) throw new Error('bad key')
  return s
}
const wrapKey = (eph, fixed, ctx) => Buffer.from(crypto.hkdfSync('sha256', Buffer.concat([eph, fixed]), Buffer.from(VERSION), Buffer.from(ctx), 32))

// the core's share_context
const context = (kind, from, to, id, fromKey, toKey) => [VERSION, kind, from, to, id, fromKey, toKey].join('\n')

function seal(plain, me, toRaw, ctx) {
  const eph = privateKey(crypto.randomBytes(32))
  const key = wrapKey(agree(eph, toRaw), agree(me, toRaw), ctx)
  const nonce = crypto.randomBytes(12)
  const c = crypto.createCipheriv('aes-256-gcm', key, nonce)
  c.setAAD(Buffer.from(ctx))
  const ct = Buffer.concat([c.update(plain), c.final()])
  return { epk: b64u(publicRaw(eph)), sealed: b64u(Buffer.concat([nonce, ct, c.getAuthTag()])) }
}

function open(sealed, epk, me, fromRaw, ctx) {
  const box = unb64u(sealed)
  const key = wrapKey(agree(me, unb64u(epk)), agree(me, fromRaw), ctx)
  const d = crypto.createDecipheriv('aes-256-gcm', key, box.subarray(0, 12))
  d.setAAD(Buffer.from(ctx))
  d.setAuthTag(box.subarray(box.length - TAG))
  return Buffer.concat([d.update(box.subarray(12, box.length - TAG)), d.final()])
}

const nonceOf = (i) => Buffer.concat([Buffer.alloc(4), (() => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(i)); return b })()])
const aadOf = (i, last) => Buffer.concat([(() => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(i)); return b })(), Buffer.from([last ? 1 : 0])])

function encryptFile(plain, key) {
  const chunks = Math.max(1, Math.ceil(plain.length / CHUNK))
  const out = []
  for (let i = 0; i < chunks; i++) {
    const c = crypto.createCipheriv('aes-256-gcm', key, nonceOf(i))
    c.setAAD(aadOf(i, i === chunks - 1))
    out.push(c.update(plain.subarray(i * CHUNK, (i + 1) * CHUNK)), c.final(), c.getAuthTag())
  }
  return Buffer.concat(out)
}

function decryptFile(sealed, key) {
  const chunks = Math.max(1, Math.ceil(sealed.length / (CHUNK + TAG)))
  const out = []
  for (let i = 0; i < chunks; i++) {
    const part = sealed.subarray(i * (CHUNK + TAG), (i + 1) * (CHUNK + TAG))
    const d = crypto.createDecipheriv('aes-256-gcm', key, nonceOf(i))
    d.setAAD(aadOf(i, i === chunks - 1))
    d.setAuthTag(part.subarray(part.length - TAG))
    out.push(d.update(part.subarray(0, part.length - TAG)), d.final())
  }
  return Buffer.concat(out)
}

const encryptedLength = (size) => size + TAG * Math.max(1, Math.ceil(size / CHUNK))
const sha = (b) => crypto.createHash('sha256').update(b).digest('hex')
const hexId = () => crypto.randomBytes(16).toString('hex')

// the recipient's key is fixed, so `open` can use it for what the app seals
const RECIP = Buffer.from(Array.from({ length: 32 }, (_, i) => (0x40 + i * 3) & 0xff))
const FROM = 'mac-user'
const TO = 'win-user'

const [mode, dir] = process.argv.slice(2)
if (!dir) {
  console.error('usage: node share-vectors.mjs make|open <dir>')
  process.exit(2)
}
fs.mkdirSync(dir, { recursive: true })
const recip = privateKey(RECIP)
const recipPub = b64u(publicRaw(recip))

if (mode === 'make') {
  const sender = privateKey(crypto.randomBytes(32))
  const senderPub = b64u(publicRaw(sender))
  const clipText = 'hello from a mac · 2+2=4 ✓'
  const clipId = hexId()
  const clip = { id: clipId, ...seal(Buffer.from(clipText), sender, unb64u(recipPub), context('clip', FROM, TO, clipId, senderPub, recipPub)) }
  // 2.5 MiB: three chunks, the last one short
  const plain = Buffer.alloc(CHUNK * 2 + CHUNK / 2)
  for (let i = 0; i < plain.length; i++) plain[i] = (i * 31 + 7) % 251
  const fileKey = crypto.randomBytes(32)
  const sealedFile = path.join(dir, 'mac-file.sealed')
  fs.writeFileSync(sealedFile, encryptFile(plain, fileKey))
  // the core's seal_file_facts, as the Mac calls it
  const facts = JSON.stringify({ v: 1, name: 'mac report.bin', type: 'application/octet-stream', size: plain.length, sha256: sha(plain), key: b64u(fileKey) })
  const offerId = hexId()
  const offer = { id: offerId, bytes: encryptedLength(plain.length), ...seal(Buffer.from(facts), sender, unb64u(recipPub), context('file', FROM, TO, offerId, senderPub, recipPub)) }
  const out = { secret: RECIP.toString('hex'), recipPub, senderPub, from: FROM, to: TO, clip, offer, sealedFile, dir: path.join(dir, 'saved'), clipText, plainSha: sha(plain), plainSize: plain.length }
  fs.mkdirSync(out.dir, { recursive: true })
  fs.writeFileSync(path.join(dir, 'mac.json'), JSON.stringify(out, null, 1))
  // and a plain file for the app to seal in the other direction
  fs.writeFileSync(path.join(dir, 'win-plain.bin'), plain)
  console.log(JSON.stringify({ ok: true, recipPub }))
} else if (mode === 'open') {
  const w = JSON.parse(fs.readFileSync(path.join(dir, 'win.json'), 'utf8'))
  const from = unb64u(w.senderPub)
  const result = {}
  try {
    result.clip = open(w.clip.sealed, w.clip.epk, recip, from, context('clip', w.from, w.to, w.clip.id, w.senderPub, recipPub)).toString()
  } catch (e) {
    result.clipError = String(e)
  }
  try {
    const facts = JSON.parse(open(w.offer.sealed, w.offer.epk, recip, from, context('file', w.from, w.to, w.offer.id, w.senderPub, recipPub)).toString())
    result.facts = { name: facts.name, size: facts.size, sha256: facts.sha256 }
    const sealed = fs.readFileSync(w.sealedFile)
    result.lengthOk = sealed.length === encryptedLength(facts.size) && w.offer.bytes === sealed.length
    const plain = decryptFile(sealed, unb64u(facts.key))
    result.sha256 = sha(plain)
    result.matches = result.sha256 === facts.sha256 && sha(fs.readFileSync(w.plainFile)) === facts.sha256
  } catch (e) {
    result.fileError = String(e)
  }
  console.log(JSON.stringify(result))
} else {
  console.error('make or open')
  process.exit(2)
}
