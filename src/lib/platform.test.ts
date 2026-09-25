import { afterEach, describe, expect, it, vi } from 'vitest'
import { cheatSheet, HINTS, HOTKEY_FAILED_HINT, pickHint } from './onboarding'
import { openNativePeriodicTable } from './periodic'
import { commandHeld, hostCheats, hostKeys, isClearHistoryKey, isWindowsHost } from './platform'

const MAC_GLYPHS = /[⌘⌃⌥⇧]/
const key = (k: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey', boolean>> = {}) => ({
  key: k,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
})

function stubHost(platform?: string) {
  const posted: unknown[] = []
  vi.stubGlobal('window', {
    __QCALC_PLATFORM: platform,
    webkit: { messageHandlers: { qcalc: { postMessage: (m: unknown) => posted.push(m) } } },
  })
  return posted
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('host detection', () => {
  it('is windows only when the shim says so', () => {
    expect(isWindowsHost()).toBe(false)
    stubHost('windows')
    expect(isWindowsHost()).toBe(true)
    stubHost()
    expect(isWindowsHost()).toBe(false)
  })
})

describe('shortcut keys', () => {
  it('reads ⌘ as ctrl on windows only', () => {
    expect(commandHeld(key('c', { metaKey: true }), false)).toBe(true)
    expect(commandHeld(key('c', { ctrlKey: true }), false)).toBe(false)
    expect(commandHeld(key('c', { ctrlKey: true }), true)).toBe(true)
    expect(commandHeld(key('c', { metaKey: true }), true)).toBe(false)
  })

  it('never clears history on the key that copies', () => {
    expect(isClearHistoryKey(key('c', { ctrlKey: true }), false)).toBe(true)
    expect(isClearHistoryKey(key('c', { ctrlKey: true }), true)).toBe(false)
    expect(isClearHistoryKey(key('Backspace', { ctrlKey: true, shiftKey: true }), true)).toBe(true)
    expect(isClearHistoryKey(key('Backspace', { ctrlKey: true }), true)).toBe(false)
    expect(isClearHistoryKey(key('C', { ctrlKey: true, shiftKey: true }), false)).toBe(false)
    expect(isClearHistoryKey(key('c', { ctrlKey: true, altKey: true }), false)).toBe(false)
  })
})

describe('windows wording', () => {
  it('leaves the mac and the web page alone', () => {
    expect(hostKeys('⌘C copies the answer', false)).toBe('⌘C copies the answer')
    expect(hostCheats(cheatSheet('⌃⌥Space', true), false)).toEqual(cheatSheet('⌃⌥Space', true))
  })

  it('spells every hint without mac glyphs', () => {
    const facts = { expr: 'sin(1/2)', answer: '0.5', unit: true, variable: 'x', native: true, opens: 3 }
    for (const rule of HINTS) {
      for (const angleMode of ['deg', 'rad'] as const) {
        const text = typeof rule.text === 'string' ? rule.text : rule.text({ ...facts, angleMode })
        expect(hostKeys(text, true)).not.toMatch(MAC_GLYPHS)
      }
    }
    expect(hostKeys(pickHint(0, { expr: '2+2', answer: '4' }, 'pause')!.text, true)).toBe('Ctrl+C copies the answer')
    expect(hostKeys('degrees · ⌃D for radians', true)).toBe('degrees · Ctrl+D for radians')
    expect(hostKeys('⌥↑ ⌥↓ step the unit prefix', true)).toBe('Alt+↑ Alt+↓ step the unit prefix')
    expect(hostKeys('⌘, opens settings', true)).toBe('Ctrl+, opens settings')
  })

  it('points a hotkey failure at the tray', () => {
    expect(hostKeys(HOTKEY_FAILED_HINT, true)).toBe('shortcut in use by another app · change it from the tray')
  })

  it('rewrites the cheat sheet, with clearing off ctrl+c', () => {
    const [keys] = hostCheats(cheatSheet('Alt+Space', true), true)
    const byLabel = new Map(keys!.map(([k, label]) => [label, k]))
    expect(keys!.flat().join(' ')).not.toMatch(MAC_GLYPHS)
    expect(byLabel.get('show / hide')).toBe('Alt+Space')
    expect(byLabel.get('copy answer / line')).toBe('Ctrl+C Ctrl+Shift+C')
    expect(byLabel.get('clear history')).toBe('Ctrl+Shift+⌫')
    expect(byLabel.get('settings')).toBe('Ctrl+,')
    expect(byLabel.get('degrees / radians')).toBe('Ctrl+D')
    expect(new Set(keys!.map(([k]) => k)).size).toBe(keys!.length)
  })
})

describe('periodic table on windows', () => {
  it('opens the page’s own table instead of posting to the host', () => {
    const posted = stubHost('windows')
    expect(openNativePeriodicTable()).toBe(false)
    expect(posted).toEqual([])
  })

  it('still goes to the mac window', () => {
    const posted = stubHost()
    expect(openNativePeriodicTable()).toBe(true)
    expect(posted).toHaveLength(1)
    expect((posted[0] as { type: string }).type).toBe('periodic')
  })
})
