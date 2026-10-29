import { createHash, generateKeyPairSync, sign } from 'node:crypto'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, describe, expect, it } from 'vitest'
import { compose } from './latest-json.mjs'
import { checkFeed, compareSemver, pick, verifySignature } from './verify-feed.mjs'

// a minisign key and signatures in the shape `tauri signer` writes, made here with node's Ed25519
const keyId = Buffer.from('0123456789abcdef', 'hex')
const { publicKey, privateKey } = generateKeyPairSync('ed25519')
const rawPub = publicKey.export({ format: 'der', type: 'spki' }).subarray(-32)
const b64 = (text) => Buffer.from(text).toString('base64')
const pubkey = b64(`untrusted comment: minisign public key: X\n${Buffer.concat([Buffer.from('Ed'), keyId, rawPub]).toString('base64')}\n`)

function minisign(bytes, version, { prehashed = true, id = keyId, comment } = {}) {
  const message = prehashed ? createHash('blake2b512').update(bytes).digest() : bytes
  const sig = sign(null, message, privateKey)
  const trusted = comment ?? `timestamp:1\tfile:setup.exe\tversion:${version}`
  const global = sign(null, Buffer.concat([sig, Buffer.from(trusted)]), privateKey)
  const line = Buffer.concat([Buffer.from(prehashed ? 'ED' : 'Ed'), id, sig]).toString('base64')
  return b64(`untrusted comment: signature from tauri secret key\n${line}\ntrusted comment: ${trusted}\n${global.toString('base64')}\n`)
}

const dir = mkdtempSync(join(tmpdir(), 'qcalc-verify-feed-'))
afterAll(() => rmSync(dir, { recursive: true, force: true }))
const installer = (name, text) => {
  writeFileSync(join(dir, name), text)
  return { file: name, sig: minisign(Buffer.from(text), '3.0.0') }
}
const x64 = { platform: 'windows-x86_64', ...installer('Q-Calc-3.0.0-x64-native-setup.exe', 'MZ native x64') }
const arm = { platform: 'windows-aarch64', ...installer('Q-Calc-3.0.0-arm64-native-setup.exe', 'MZ native arm64') }
const base = 'https://github.com/maxconine/Q-Calc/releases/download/v3.0.0'
const darwin = { signature: 'mac-sig', url: 'https://example.invalid/Q-Calc.app.tar.gz' }
// the 2.x manifest the Tauri app reads today, with an -nsis key and a mac entry as a worst case
const old = {
  version: '2.0.15',
  platforms: {
    'windows-x86_64-nsis': { signature: 's', url: `${base}/Q-Calc-2.0.15-x64-setup.exe` },
    'windows-x86_64': { signature: 's', url: `${base}/Q-Calc-2.0.15-x64-setup.exe` },
    'darwin-aarch64': darwin,
  },
}

describe('the version compare', () => {
  it('orders as semver, as the plugin does', () => {
    expect(compareSemver('2.0.15', '3.0.0')).toBe(-1)
    expect(compareSemver('3.0.0', '2.0.15')).toBe(1)
    expect(compareSemver('2.0.9', '2.0.15')).toBe(-1)
    expect(compareSemver('v3.0.0', '3.0.0')).toBe(0)
    expect(compareSemver('3.0.0-beta.1', '3.0.0')).toBe(-1)
    expect(compareSemver('3.0.0-beta.2', '3.0.0-beta.10')).toBe(-1)
  })
})

describe('the entry the Tauri updater picks', () => {
  it('prefers windows-<arch>-nsis over windows-<arch>', () => {
    expect(pick(old, 'x86_64').key).toBe('windows-x86_64-nsis')
    expect(() => pick(old, 'aarch64')).toThrow(/windows-aarch64-nsis or windows-aarch64/)
  })

  it("picks the native installers in a composed v3 latest.json, and keeps the mac entry", () => {
    const feed = compose('3.0.0', base, [x64, arm], old)
    expect(Object.keys(feed.platforms).sort()).toEqual(['darwin-aarch64', 'windows-aarch64', 'windows-x86_64'])
    expect(feed.platforms['darwin-aarch64']).toEqual(darwin)
    expect(pick(feed, 'x86_64').url).toBe(`${base}/Q-Calc-3.0.0-x64-native-setup.exe`)
    expect(pick(feed, 'aarch64').url).toBe(`${base}/Q-Calc-3.0.0-arm64-native-setup.exe`)
    expect(checkFeed(feed, { dir, pubkey, current: '2.0.15', native: true, arches: ['x86_64', 'aarch64'] })).toEqual([
      { arch: 'x86_64', key: 'windows-x86_64', file: x64.file },
      { arch: 'aarch64', key: 'windows-aarch64', file: arm.file },
    ])
  })
})

describe('checkFeed refuses', () => {
  const feed = () => compose('3.0.0', base, [x64, arm])

  it('a version an install already has', () => {
    expect(() => checkFeed(feed(), { dir, pubkey, current: '3.0.0' })).toThrow(/not newer/)
  })

  it('an -nsis key left in', () => {
    const f = feed()
    f.platforms['windows-x86_64-nsis'] = f.platforms['windows-x86_64']
    expect(() => checkFeed(f, { dir, pubkey })).toThrow(/installer-specific/)
  })

  it('a Tauri installer when the switch says native', () => {
    writeFileSync(join(dir, 'Q-Calc-3.0.0-x64-setup.exe'), 'MZ tauri')
    const f = compose('3.0.0', base, [{ platform: 'windows-x86_64', file: 'Q-Calc-3.0.0-x64-setup.exe', sig: minisign(Buffer.from('MZ tauri'), '3.0.0') }])
    expect(checkFeed(f, { dir, pubkey })[0].file).toBe('Q-Calc-3.0.0-x64-setup.exe')
    expect(() => checkFeed(f, { dir, pubkey, native: true })).toThrow(/not a native installer/)
  })

  it('a missing arch, or one pointing at the other arch', () => {
    const f = compose('3.0.0', base, [x64])
    expect(() => checkFeed(f, { dir, pubkey, arches: ['x86_64', 'aarch64'] })).toThrow(/windows-aarch64/)
    f.platforms['windows-aarch64'] = f.platforms['windows-x86_64']
    expect(() => checkFeed(f, { dir, pubkey })).toThrow(/another arch/)
  })

  it('a bad pub_date', () => {
    expect(() => checkFeed({ ...feed(), pub_date: 'yesterday' }, { dir, pubkey })).toThrow(/pub_date/)
  })
})

describe('the signature check', () => {
  const bytes = Buffer.from('MZ native x64')

  it('takes prehashed and legacy minisign signatures', () => {
    expect(() => verifySignature(bytes, minisign(bytes, '3.0.0'), pubkey, '3.0.0')).not.toThrow()
    expect(() => verifySignature(bytes, minisign(bytes, '3.0.0', { prehashed: false }), pubkey, '3.0.0')).not.toThrow()
  })

  it('refuses other bytes, another key, another or no version', () => {
    expect(() => verifySignature(Buffer.from('MZ other'), minisign(bytes, '3.0.0'), pubkey, '3.0.0')).toThrow(/match the file/)
    expect(() => verifySignature(bytes, minisign(bytes, '3.0.0', { id: Buffer.alloc(8) }), pubkey, '3.0.0')).toThrow(/another key/)
    expect(() => verifySignature(bytes, minisign(bytes, '2.0.15'), pubkey, '3.0.0')).toThrow(/signed for 2.0.15/)
    expect(() => verifySignature(bytes, minisign(bytes, '', { comment: 'timestamp:1' }), pubkey, '3.0.0')).toThrow(/no version/)
  })

  it('refuses a trusted comment changed after signing', () => {
    const good = Buffer.from(minisign(bytes, '3.0.0'), 'base64').toString('utf8')
    const forged = b64(good.replace('version:3.0.0', 'version:3.0.1'))
    expect(() => verifySignature(bytes, forged, pubkey, '3.0.1')).toThrow(/trusted comment/)
  })
})

describe('the shipped keys', () => {
  it('the Tauri app and the native app trust the same updater key', async () => {
    const { readFileSync } = await import('node:fs')
    const conf = JSON.parse(readFileSync(new URL('../tauri.windows.conf.json', import.meta.url), 'utf8'))
    const rs = readFileSync(new URL('../../native/windows/src/update.rs', import.meta.url), 'utf8')
    const native = /pub const PUBKEY: &str = "([^"]+)"/.exec(rs)?.[1]
    expect(native).toBe(conf.plugins.updater.pubkey)
  })
})
