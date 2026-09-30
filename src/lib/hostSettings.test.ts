import { describe, expect, it } from 'vitest'
import { draftChoices, EMPTY_HOST, hostInfo, hotkeyNote } from './hostSettings'

describe('hostInfo', () => {
  it('reads what the shell sends', () => {
    const info = hostInfo({ hotkeyChord: 'ctrl+space', hotkeyFailed: false, autostart: true, sigFigs: 9 })
    expect(info).toEqual({ hotkeyChord: 'ctrl+space', hotkeyFailed: false, hotkeyRefused: '', autostart: true })
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
