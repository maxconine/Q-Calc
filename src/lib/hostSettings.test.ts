import { describe, expect, it } from 'vitest'
import { conflictText, draftChoices, EMPTY_HOST, hostInfo, hotkeyNote, hotkeyVerdict, spotChoices } from './hostSettings'

describe('hostInfo', () => {
  it('reads what the shell sends', () => {
    const info = hostInfo({ hotkeyChord: 'ctrl+space', hotkeyFailed: false, autostart: true, sigFigs: 9, spotSeconds: 300 })
    expect(info).toEqual({
      hotkeyChord: 'ctrl+space',
      hotkeyFailed: false,
      hotkeyRefused: '',
      hotkeyTaken: '',
      autostart: true,
      spotSeconds: 300,
    })
    expect(hostInfo({ hotkeyTaken: 'alt+space' }).hotkeyTaken).toBe('alt+space')
  })

  it('keeps what a partial push leaves out, except a refusal', () => {
    const base = { ...EMPTY_HOST, hotkeyChord: 'alt+space', autostart: true, hotkeyRefused: 'Ctrl+Space', hotkeyTaken: 'ctrl+space' }
    expect(hostInfo({ theme: 'dark' }, base)).toEqual({ ...base, hotkeyRefused: '', hotkeyTaken: '' })
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

describe('hotkeyVerdict', () => {
  it('reads the shell’s answer and drops junk', () => {
    const raw = { chord: 'alt+space', usable: true, free: false, owner: 'powertoys-run', sure: true, suggest: 'alt+q' }
    expect(hotkeyVerdict(raw)).toEqual(raw)
    expect(hotkeyVerdict({ chord: 'ctrl+k', free: true, owner: null, suggest: null })).toEqual({
      chord: 'ctrl+k',
      usable: true,
      free: true,
      owner: '',
      sure: false,
      suggest: '',
    })
    expect(hotkeyVerdict({ chord: 'ctrl+k' })).toBeNull()
    expect(hotkeyVerdict('alt+space')).toBeNull()
    expect(hotkeyVerdict(undefined)).toBeNull()
  })
})

describe('conflictText', () => {
  const taken = { chord: 'alt+space', usable: true, free: false, owner: '', sure: false, suggest: '' }

  it('names powertoys run and where to change it', () => {
    expect(conflictText({ ...taken, owner: 'powertoys-run', sure: true, suggest: 'ctrl+alt+space' })).toBe(
      'Alt+Space is used by PowerToys Run. Change it in PowerToys → PowerToys Run → Activation shortcut, or pick another here. Ctrl+Alt+Space is free.',
    )
  })

  it('says probably when the owner is a guess', () => {
    expect(conflictText({ ...taken, owner: 'flow-launcher' })).toBe(
      'Alt+Space is in use, probably by Flow Launcher. Change it in Flow Launcher’s settings → Hotkey, or pick another here.',
    )
    expect(conflictText({ ...taken, chord: 'cmd+e', owner: 'windows', suggest: 'alt+q' })).toBe(
      'Win+E is in use, probably by Windows. Pick another. Alt+Q is free.',
    )
  })

  it('doesn’t guess when nothing it knows is running', () => {
    expect(conflictText({ ...taken, chord: 'ctrl+k', suggest: 'ctrl+alt+space' })).toBe(
      'Ctrl+K is in use by another app. Pick another. Ctrl+Alt+Space is free.',
    )
    expect(conflictText({ ...taken, owner: 'toString' })).toBe('Alt+Space is in use by another app. Pick another.')
    expect(conflictText({ ...taken, chord: 'ctrl+plus', usable: false })).toBe('Q Calc can’t use Ctrl++ as a shortcut. Pick another.')
  })
})

describe('hotkeyNote with a verdict', () => {
  const verdict = { chord: 'alt+space', usable: true, free: false, owner: 'powertoys-run', sure: false, suggest: 'ctrl+alt+space' }

  it('names the holder of a refused chord', () => {
    const host = { ...EMPTY_HOST, hotkeyChord: 'ctrl+k', hotkeyRefused: 'Alt+Space', hotkeyTaken: 'alt+space' }
    expect(hotkeyNote(host, verdict)).toBe(
      'Alt+Space is in use, probably by PowerToys Run. Change it in PowerToys → PowerToys Run → Activation shortcut, or pick another here. Ctrl+Alt+Space is free.',
    )
  })

  it('says which chord opens q calc after a launch fell back', () => {
    const host = { ...EMPTY_HOST, hotkeyChord: 'alt+space', hotkeyFailed: true, hotkeyTaken: 'ctrl+k' }
    expect(hotkeyNote(host, { ...verdict, chord: 'ctrl+k', owner: '', suggest: '' })).toBe(
      'Ctrl+K is in use by another app. Pick another. Alt+Space opens Q Calc for now.',
    )
  })

  it('ignores a verdict about another chord, or a free one', () => {
    const host = { ...EMPTY_HOST, hotkeyRefused: 'Ctrl+Space', hotkeyTaken: 'ctrl+space' }
    expect(hotkeyNote(host, verdict)).toBe('Ctrl+Space is in use by another app')
    expect(hotkeyNote({ ...host, hotkeyTaken: 'alt+space' }, { ...verdict, free: true })).toBe('Ctrl+Space is in use by another app')
  })
})

describe('draftChoices', () => {
  it('matches the mac list and keeps an odd value visible', () => {
    expect(draftChoices(60).map((c) => c.id)).toEqual([0, 30, 60, 120, 300])
    expect(draftChoices(45).at(-1)).toEqual({ id: 45, label: '45 seconds' })
  })
})
