import { createElement } from 'react'
import { renderToString } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { KeybindSettings } from './KeybindSettings'

function render(show: Parameters<typeof KeybindSettings>[0]['show']): string {
  return renderToString(createElement(KeybindSettings, { value: {}, onChange: () => {}, windows: true, settingsWindow: true, show }))
}

describe('the show / hide row', () => {
  const taken = 'Alt+Space is used by PowerToys Run. Change it in PowerToys → PowerToys Run → Activation shortcut, or pick another here.'

  it('offers a free chord under a taken one’s note', () => {
    const html = render({ chord: 'alt+space', note: taken, onPick: () => {}, suggest: 'ctrl+alt+space' })
    expect(html).toContain('PowerToys → PowerToys Run → Activation shortcut')
    expect(html).toMatch(/keybind-suggest[^>]*>Use <!-- -->Ctrl\+Alt\+Space/)
  })

  it('offers nothing when the shortcut is fine', () => {
    expect(render({ chord: 'alt+space', note: '', onPick: () => {}, suggest: 'ctrl+alt+space' })).not.toContain('keybind-suggest')
  })
})
