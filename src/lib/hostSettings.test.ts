import { describe, expect, it } from 'vitest'
import { draftChoices, EMPTY_HOST, hostInfo, hotkeyNote } from './hostSettings'

const HOTKEYS = [
  { id: 'alt-space', title: 'Alt+Space' },
  { id: 'ctrl-space', title: 'Ctrl+Space' },
]

describe('hostInfo', () => {
  it('reads what the shell sends', () => {
    const info = hostInfo({ hotkeyId: 'ctrl-space', hotkeyFailed: false, hotkeys: HOTKEYS, autostart: true, sigFigs: 9 })
    expect(info).toEqual({ hotkeyId: 'ctrl-space', hotkeyFailed: false, hotkeyRefused: '', hotkeys: HOTKEYS, autostart: true })
  })

  it('keeps what a partial push leaves out, except a refusal', () => {
    const base = { ...EMPTY_HOST, hotkeyId: 'alt-space', hotkeys: HOTKEYS, autostart: true, hotkeyRefused: 'Ctrl+Space' }
    expect(hostInfo({ theme: 'dark' }, base)).toEqual({ ...base, hotkeyRefused: '' })
    expect(hostInfo(undefined, base).hotkeys).toBe(HOTKEYS)
  })

  it('drops junk', () => {
    const info = hostInfo({ hotkeyId: 3, autostart: 'yes', hotkeys: [{ id: 'a' }, null, { id: 'b', title: 'B' }] })
    expect(info.hotkeyId).toBe('')
    expect(info.autostart).toBe(false)
    expect(info.hotkeys).toEqual([{ id: 'b', title: 'B' }])
  })
})

describe('hotkeyNote', () => {
  it('says why the shortcut did not change', () => {
    expect(hotkeyNote({ ...EMPTY_HOST, hotkeyRefused: 'Ctrl+Space' })).toBe('Ctrl+Space is in use by another app')
    expect(hotkeyNote({ ...EMPTY_HOST, hotkeyFailed: true })).toBe('Your shortcut is in use by another app')
    expect(hotkeyNote(EMPTY_HOST)).not.toMatch(/in use/)
  })
})

describe('draftChoices', () => {
  it('matches the mac list and keeps an odd value visible', () => {
    expect(draftChoices(60).map((c) => c.id)).toEqual([0, 30, 60, 120, 300])
    expect(draftChoices(45).at(-1)).toEqual({ id: 45, label: '45 seconds' })
  })
})
