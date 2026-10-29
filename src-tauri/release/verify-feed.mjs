// checks an updater manifest (latest.json or latest-native.json) the way the apps will read it, before it is
// uploaded: which entry tauri-plugin-updater 2.x picks on each windows arch (windows-<arch>-nsis first, then
// windows-<arch>), that the announced version is newer than an installed one (semver, as the plugin compares),
// that pub_date parses, and that each picked installer's signature verifies against the public key the apps
// trust (minisign: Ed25519 over the BLAKE2b-512 of the file, or over the file for a legacy signature; the global
// signature over the trusted comment, which must name exactly the announced version).
//
// usage: node verify-feed.mjs <feed.json> --dir <folder with the installers> [--pubkey <base64> | --pubkey-from
//          <tauri conf json>] [--current <installed version>] [--native] [--arch x86_64,aarch64]
//   --native: every picked installer must be a *-native-setup.exe (the v3 switch, windows.yml NATIVE_WINDOWS)
//   --arch: the arches that must have an entry (default x86_64; aarch64 is checked whenever it's there)
// exits non-zero with a sentence on the first problem. reads only local files.

import { createHash, createPublicKey, verify as edVerify } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { signedVersion } from './latest-json.mjs'

// tauri-plugin-updater's get_urls: [`{os}-{arch}-{installer}`, `{os}-{arch}`] in that order, the first with both
// a url and a signature
export function pick(feed, arch, { os = 'windows', installer = 'nsis' } = {}) {
  const platforms = feed.platforms ?? {}
  const targets = [`${os}-${arch}-${installer}`, `${os}-${arch}`]
  for (const key of targets) {
    const entry = platforms[key]
    if (entry && typeof entry.url === 'string' && typeof entry.signature === 'string') return { key, ...entry }
  }
  throw new Error(`no entry for ${targets.join(' or ')}`)
}

// semver 2 precedence (build metadata ignored), as the plugin's `release.version > self.current_version`
export function parseSemver(text) {
  const m = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(String(text).trim())
  if (!m) throw new Error(`not a semver version: ${text}`)
  return { core: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] ? m[4].split('.') : [] }
}

export function compareSemver(a, b) {
  const x = parseSemver(a)
  const y = parseSemver(b)
  for (let i = 0; i < 3; i++) if (x.core[i] !== y.core[i]) return x.core[i] < y.core[i] ? -1 : 1
  if (!x.pre.length || !y.pre.length) return x.pre.length === y.pre.length ? 0 : x.pre.length ? -1 : 1
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const p = x.pre[i]
    const q = y.pre[i]
    if (p === undefined) return -1
    if (q === undefined) return 1
    if (p === q) continue
    const pn = /^\d+$/.test(p)
    const qn = /^\d+$/.test(q)
    if (pn && qn) return Number(p) < Number(q) ? -1 : 1
    if (pn !== qn) return pn ? -1 : 1
    return p < q ? -1 : 1
  }
  return 0
}

const lines = (b64) =>
  Buffer.from(String(b64).trim(), 'base64')
    .toString('utf8')
    .split('\n')
    .map((l) => l.replace(/\r$/, ''))

// base64 of a minisign public key file -> its key id and Ed25519 key
export function parsePublicKey(b64) {
  const raw = Buffer.from(lines(b64)[1] ?? '', 'base64')
  if (raw.length !== 42 || raw.subarray(0, 2).toString() !== 'Ed') throw new Error('not a minisign public key')
  return { keyId: raw.subarray(2, 10), key: raw.subarray(10) }
}

// base64 of a minisign signature file (what `tauri signer sign` writes as <file>.sig)
export function parseSignature(b64) {
  const [untrusted, sigLine, trusted, globalLine] = lines(b64)
  if (!untrusted?.startsWith('untrusted comment:') || !trusted?.startsWith('trusted comment: ')) {
    throw new Error('not a minisign signature (its comment lines are missing)')
  }
  const raw = Buffer.from(sigLine ?? '', 'base64')
  const algorithm = raw.subarray(0, 2).toString()
  if (raw.length !== 74 || !['ED', 'Ed'].includes(algorithm)) throw new Error('not a minisign signature')
  const global = Buffer.from(globalLine ?? '', 'base64')
  if (global.length !== 64) throw new Error('the signature has no global signature')
  return {
    prehashed: algorithm === 'ED',
    keyId: raw.subarray(2, 10),
    sig: raw.subarray(10),
    trustedComment: trusted.slice('trusted comment: '.length),
    global,
  }
}

const SPKI_ED25519 = Buffer.from('302a300506032b6570032100', 'hex')

export function verifySignature(bytes, sigB64, pubB64, announced) {
  const pub = parsePublicKey(pubB64)
  const sig = parseSignature(sigB64)
  if (!pub.keyId.equals(sig.keyId)) throw new Error('signed with another key than the one the apps trust')
  const key = createPublicKey({ key: Buffer.concat([SPKI_ED25519, pub.key]), format: 'der', type: 'spki' })
  const message = sig.prehashed ? createHash('blake2b512').update(bytes).digest() : bytes
  if (!edVerify(null, message, key, sig.sig)) throw new Error("the signature doesn't match the file")
  if (!edVerify(null, Buffer.concat([sig.sig, Buffer.from(sig.trustedComment)]), key, sig.global)) {
    throw new Error("the trusted comment's signature doesn't verify")
  }
  const signed = signedVersion(sigB64)
  if (signed === null) throw new Error('the signature names no version (requireSignedVersion refuses it)')
  if (compareSemver(signed, announced) !== 0) throw new Error(`signed for ${signed}, announced as ${announced}`)
}

// every check above, for each arch; returns what each arch would install
export function checkFeed(feed, { dir, pubkey, current, native = false, arches = ['x86_64'] }) {
  parseSemver(feed.version)
  if (feed.pub_date !== undefined && Number.isNaN(Date.parse(feed.pub_date))) throw new Error('pub_date is not a date')
  if (feed.pub_date !== undefined && !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)$/.test(feed.pub_date)) {
    throw new Error(`pub_date ${feed.pub_date} is not RFC 3339`)
  }
  if (current !== undefined && compareSemver(feed.version, current) <= 0) {
    throw new Error(`${feed.version} is not newer than ${current}: installs would not update`)
  }
  const stale = Object.keys(feed.platforms ?? {}).filter((k) => /^windows-.+-.+$/.test(k))
  if (stale.length) throw new Error(`installer-specific keys would win over the plain ones: ${stale.join(', ')}`)
  const all = [...new Set([...arches, ...(feed.platforms?.['windows-aarch64'] ? ['aarch64'] : [])])]
  return all.map((arch) => {
    const entry = pick(feed, arch)
    const file = decodeURIComponent(basename(new URL(entry.url).pathname))
    if (native && !/-native-setup\.exe$/.test(file)) throw new Error(`${entry.key} points at ${file}, not a native installer`)
    if (!new RegExp(`-${arch === 'aarch64' ? 'arm64' : 'x64'}-`).test(file)) throw new Error(`${entry.key} points at ${file}, another arch`)
    const path = join(dir, file)
    if (!existsSync(path)) throw new Error(`${entry.key}: ${file} is not in ${dir}`)
    verifySignature(readFileSync(path), entry.signature, pubkey, feed.version)
    return { arch, key: entry.key, file }
  })
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [feedPath, ...rest] = process.argv.slice(2)
  const opt = {}
  for (let i = 0; i < rest.length; i++) {
    const flag = rest[i]
    if (flag === '--native') opt.native = true
    else opt[flag.replace(/^--/, '')] = rest[++i]
  }
  try {
    const feed = JSON.parse(readFileSync(feedPath, 'utf8').replace(/^\uFEFF/, ''))
    let pubkey = opt.pubkey
    if (opt['pubkey-from']) pubkey = JSON.parse(readFileSync(opt['pubkey-from'], 'utf8')).plugins.updater.pubkey
    if (!pubkey) throw new Error('--pubkey or --pubkey-from is required')
    const picked = checkFeed(feed, {
      dir: opt.dir ?? '.',
      pubkey,
      current: opt.current,
      native: Boolean(opt.native),
      arches: (opt.arch ?? 'x86_64').split(','),
    })
    for (const p of picked) console.log(`${basename(feedPath)} ${feed.version}: windows ${p.arch} -> ${p.key} -> ${p.file} (signature ok)`)
  } catch (err) {
    console.error(`${feedPath}: ${err.message}`)
    process.exit(1)
  }
}
