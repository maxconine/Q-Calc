import { describe, expect, it } from 'vitest'
import { manifest, signedVersion } from './latest-json.mjs'

const sig = (comment) =>
  Buffer.from(
    `untrusted comment: signature from tauri secret key\nRUQfake\ntrusted comment: ${comment}\nglobalfake\n`,
  ).toString('base64')

const x64 = { platform: 'windows-x86_64', file: 'Q-Calc-2.1.0-x64-setup.exe', sig: sig('timestamp:1\tfile:x\tversion:2.1.0') }
const arm = { platform: 'windows-aarch64', file: 'Q-Calc-2.1.0-arm64-setup.exe', sig: sig('timestamp:1\tfile:y\tversion:2.1.0') }
const base = 'https://github.com/o/r/releases/download/v2.1.0/'
const now = new Date('2026-09-24T12:00:00.123Z')

describe('signedVersion', () => {
  it('reads the version from the trusted comment', () => {
    expect(signedVersion(x64.sig)).toBe('2.1.0')
    expect(signedVersion(`${x64.sig}\n`)).toBe('2.1.0')
  })

  it('is null for a signature without one', () => {
    expect(signedVersion(sig('timestamp:1\tfile:x'))).toBeNull()
    expect(signedVersion('')).toBeNull()
  })
})

describe('manifest', () => {
  it('lists each installer with its signature and download url', () => {
    expect(manifest('2.1.0', base, [x64, arm], now)).toEqual({
      version: '2.1.0',
      pub_date: '2026-09-24T12:00:00Z',
      platforms: {
        'windows-x86_64': { signature: x64.sig, url: `${base}Q-Calc-2.1.0-x64-setup.exe` },
        'windows-aarch64': { signature: arm.sig, url: `${base}Q-Calc-2.1.0-arm64-setup.exe` },
      },
    })
  })

  it('ships x64 alone when the arm build is missing', () => {
    expect(Object.keys(manifest('2.1.0', base, [x64], now).platforms)).toEqual(['windows-x86_64'])
  })

  it('refuses a release without x64', () => {
    expect(() => manifest('2.1.0', base, [arm], now)).toThrow(/x64/)
  })

  it('refuses a signature made for another version', () => {
    const stale = { ...x64, sig: sig('timestamp:1\tfile:x\tversion:2.0.9') }
    expect(() => manifest('2.1.0', base, [stale], now)).toThrow(/2\.0\.9/)
    const bare = { ...x64, sig: sig('timestamp:1\tfile:x') }
    expect(() => manifest('2.1.0', base, [bare], now)).toThrow(/no version/)
  })

  it('refuses bad versions and platforms', () => {
    expect(() => manifest('v2.1.0', base, [x64], now)).toThrow(/version/)
    expect(() => manifest('2.1.0', base, [x64, { ...arm, platform: 'darwin-aarch64' }], now)).toThrow(/platform/)
  })

  it('escapes file names in urls', () => {
    const spaced = { ...x64, file: 'Q Calc setup.exe' }
    expect(manifest('2.1.0', base.slice(0, -1), [spaced], now).platforms['windows-x86_64'].url).toBe(`${base}Q%20Calc%20setup.exe`)
  })
})
