import { createElement } from 'react'
import { renderToString } from 'react-dom/server'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { SettingsPage } from './SettingsPage'

afterEach(() => {
  vi.unstubAllGlobals()
})

function render(settings: Record<string, unknown>): string {
  vi.stubGlobal('window', { __QCALC_PLATFORM: 'windows', __QCALC_SETTINGS: settings })
  return renderToString(createElement(SettingsPage))
}

describe('settings window', () => {
  const html = render({
    sigFigs: 9,
    draftSeconds: 120,
    angleMode: 'rad',
    hotkeyId: 'ctrl-space',
    hotkeyFailed: true,
    hotkeys: [
      { id: 'alt-space', title: 'Alt+Space' },
      { id: 'ctrl-space', title: 'Ctrl+Space' },
    ],
  })

  it('has every row the mac settings window has, plus login', () => {
    for (const title of [
      'Keyboard shortcut',
      'Open at login',
      'Appearance',
      'Show history',
      'History',
      'Keep unfinished input',
      'Angles',
      'Answer form',
      'Fractions',
      'Rationalize denominators',
      'Keep typed words as text',
      'Typst preview',
      'Copy Typst compatible',
      'Digits shown',
      'Propagate from input',
      'Default units',
    ]) {
      expect(html).toContain(`aria-label="${title}"`)
    }
  })

  it('shows the injected values', () => {
    expect(html).toMatch(/<option value="ctrl-space" selected="">Ctrl\+Space<\/option>/)
    expect(html).toMatch(/<option value="9" selected="">9<\/option>/)
    expect(html).toMatch(/<option value="120" selected="">2 minutes<\/option>/)
    expect(html).toMatch(/aria-pressed="true"[^>]*>Radians/)
    expect(html).toContain('Your shortcut is in use by another app')
  })

  it('speaks windows keys', () => {
    expect(html).toContain('Switch with Ctrl+D')
    expect(html).toContain('Ctrl+F')
    expect(html).not.toMatch(/[⌘⌃⌥]/)
  })
})
