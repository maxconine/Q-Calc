import { describe, expect, it } from 'vitest'
import { draftChoices, EMPTY_HOST, hostInfo, hotkeyNote, NO_UPDATES, spotChoices, updateLine } from './hostSettings'

describe('hostInfo', () => {
  it('reads what the shell sends', () => {
    const info = hostInfo({ hotkeyChord: 'ctrl+space', hotkeyFailed: false, autostart: true, sigFigs: 9, spotSeconds: 300 })
    expect(info).toEqual({ ...EMPTY_HOST, hotkeyChord: 'ctrl+space', autostart: true, spotSeconds: 300 })
  })

  it('keeps what a partial push leaves out, except a refusal', () => {
    const base = { ...EMPTY_HOST, hotkeyChord: 'alt+space', autostart: true, hotkeyRefused: 'Ctrl+Space' }
    expect(hostInfo({ theme: 'dark' }, base)).toEqual({ ...base, hotkeyRefused: '' })
    expect(hostInfo(undefined, base).hotkeyChord).toBe('alt+space')
  })

  it('drops junk', () => {
    const info = hostInfo({ hotkeyChord: 3, autostart: 'yes' })
    expect(info.hotkeyChord).toBe('')
    expect(info.autostart).toBe(false)
  })

  it('a moved bar goes back after 5 minutes until the shell says otherwise', () => {
    expect(hostInfo({}).spotSeconds).toBe(300)
    expect(hostInfo({ spotSeconds: 0 }).spotSeconds).toBe(0)
    expect(hostInfo({ spotSeconds: -5 }).spotSeconds).toBe(300)
    expect(hostInfo({ spotSeconds: 'soon' }, { ...EMPTY_HOST, spotSeconds: 300 }).spotSeconds).toBe(300)
  })
})

describe('software update', () => {
  it('reads the shell’s update status, keeping what a push leaves out', () => {
    const info = hostInfo({ version: '2.0.15', autoUpdate: false, update: { state: 'ready', latest: '2.0.16', checkedAt: 100, error: '' } })
    expect(info.version).toBe('2.0.15')
    expect(info.autoUpdate).toBe(false)
    expect(info.update).toEqual({ state: 'ready', latest: '2.0.16', checkedAt: 100, error: '' })
    expect(hostInfo({ update: { state: 'bogus' } }, info).update.state).toBe('ready')
    expect(hostInfo({}).update).toEqual(NO_UPDATES)
  })

  it('says what the updates are doing', () => {
    const now = 1_000_000
    const at = (state: string, extra = {}) => ({ ...NO_UPDATES, state, ...extra }) as typeof NO_UPDATES
    expect(updateLine(at('current', { checkedAt: now - 300 }), true, now)).toBe('Up to date · checked 5 minutes ago')
    expect(updateLine(at('current', { checkedAt: now - 10 }), true, now)).toBe('Up to date · checked just now')
    expect(updateLine(at('current', { checkedAt: now - 2 * 86400 }), true, now)).toBe('Up to date · checked 2 days ago')
    expect(updateLine(at('downloading', { latest: '2.0.16' }), true, now)).toBe('Downloading 2.0.16…')
    expect(updateLine(at('ready', { latest: '2.0.16' }), true, now)).toMatch(/^2\.0\.16 is ready\. It installs when/)
    expect(updateLine(at('ready', { latest: '2.0.16' }), false, now)).toBe('2.0.16 is ready to install.')
    expect(updateLine(at('failed', { error: 'offline', checkedAt: now - 60 }), true, now)).toBe('Couldn’t update: offline · checked 1 minute ago')
    expect(updateLine(at('off'), true, now)).toMatch(/can’t update itself/)
  })
})

describe('spotChoices', () => {
  it('lists the mac’s times, and keeps an odd value visible', () => {
    expect(spotChoices(300).map((c) => c.id)).toEqual([0, 60, 300, 900, 3600])
    expect(spotChoices(0)[0]).toEqual({ id: 0, label: 'Never' })
    expect(spotChoices(30).at(-1)).toEqual({ id: 30, label: '30 seconds' })
    expect(spotChoices(1800).at(-1)).toEqual({ id: 1800, label: '30 minutes' })
  })
})

describe('hotkeyNote', () => {
  it('says why the shortcut did not change, and nothing when it did', () => {
    expect(hotkeyNote({ ...EMPTY_HOST, hotkeyRefused: 'Ctrl+Space' })).toBe('Ctrl+Space is in use by another app')
    expect(hotkeyNote({ ...EMPTY_HOST, hotkeyFailed: true })).toBe('Your show / hide shortcut is in use by another app')
    expect(hotkeyNote(EMPTY_HOST)).toBe('')
  })
})

describe('draftChoices', () => {
  it('matches the mac list and keeps an odd value visible', () => {
    expect(draftChoices(60).map((c) => c.id)).toEqual([0, 30, 60, 120, 300])
    expect(draftChoices(45).at(-1)).toEqual({ id: 45, label: '45 seconds' })
  })
})
