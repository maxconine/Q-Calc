import { describe, expect, it } from 'vitest'
import { defaultSettings, mergeSettings, settingsEqual, type Settings } from './settings'

// the shape macos/Overlay.swift settingsJavaScriptObject pushes, hotkey fields included
const nativePush = {
  sigFigs: 6,
  draftSeconds: 30,
  defaultUnits: { length: 'ft' },
  answerForm: 'approx',
  historyInsert: 'answer',
  historyShow: 'always',
  rationalize: false,
  sigFigMode: true,
  theme: 'dark',
  angleMode: 'rad',
  fractionMode: true,
  keepWords: true,
  hotkey: '⌃⌥Space',
  hotkeyFailed: false,
} as const

describe('settings sync with the mac settings window', () => {
  it('takes every field the mac app pushes', () => {
    const next = mergeSettings(nativePush as unknown as Partial<Settings>, defaultSettings())
    const { hotkey: _hotkey, hotkeyFailed: _failed, ...fields } = nativePush
    expect(next).toEqual(fields)
  })

  it('the full object the web posts back merges to itself, so the echo is a no-op', () => {
    const s = mergeSettings(nativePush as unknown as Partial<Settings>, defaultSettings())
    const echoed = mergeSettings({ ...s }, defaultSettings())
    expect(settingsEqual(echoed, s)).toBe(true)
  })

  it('a ⌃D style toggle from the web differs only in the toggled field', () => {
    const s = defaultSettings()
    const toggled = mergeSettings({ angleMode: 'rad' }, s)
    expect(toggled).toEqual({ ...s, angleMode: 'rad' })
  })
})
