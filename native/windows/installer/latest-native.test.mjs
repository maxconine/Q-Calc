import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { nativeManifest } from './latest-native.mjs'

const sig = (comment) =>
  Buffer.from(`untrusted comment: x\nRUQfake\ntrusted comment: ${comment}\nglobalfake\n`).toString('base64')
const x64 = { platform: 'windows-x86_64', file: 'Q-Calc-2.1.0-x64-native-setup.exe', sig: sig('timestamp:1\tversion:2.1.0') }
const base = 'https://github.com/maxconine/Q-Calc/releases/download/v2.1.0'

describe('latest-native.json', () => {
  it("has update.rs's shape", () => {
    const m = nativeManifest('2.1.0', base, [x64], new Date('2026-10-27T12:00:00.5Z'))
    expect(m).toEqual({
      version: '2.1.0',
      pub_date: '2026-10-27T12:00:00Z',
      platforms: { 'windows-x86_64': { signature: x64.sig, url: `${base}/Q-Calc-2.1.0-x64-native-setup.exe` } },
    })
  })

  it('matches the fixture update.rs is tested on', () => {
    const fixture = JSON.parse(readFileSync(new URL('../tests/fixtures/latest-native.json', import.meta.url), 'utf8'))
    expect(Object.keys(fixture)).toEqual(['version', 'pub_date', 'platforms'])
    expect(Object.keys(fixture.platforms['windows-x86_64'])).toEqual(['signature', 'url'])
  })

  it('takes only native installers signed for this version', () => {
    expect(() => nativeManifest('2.1.0', base, [{ ...x64, file: 'Q-Calc-2.1.0-x64-setup.exe' }])).toThrow(/native/)
    expect(() => nativeManifest('2.1.0', base, [{ ...x64, sig: sig('version:2.0.9') }])).toThrow(/2.0.9/)
  })

  // the release builds with QCALC_VERSION = package.json's; a plain cargo build reports the crate's, so they agree
  it("the native app's crate version is package.json's", () => {
    const pkg = JSON.parse(readFileSync(new URL('../../../package.json', import.meta.url), 'utf8'))
    const cargo = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8')
    expect(/^version = "([^"]+)"/m.exec(cargo)?.[1]).toBe(pkg.version)
  })
})
