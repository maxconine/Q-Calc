import { describe, expect, it } from 'vitest'
import {
  actionForEvent,
  chordFromEvent,
  chordLabel,
  chordProblem,
  keybindFor,
  keybindsEqual,
  sanitizeKeybinds,
  setKeybind,
} from './keybinds'

type Mods = Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey', boolean>>
const press = (key: string, mods: Mods = {}, code?: string) => ({
  key,
  code,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
})

describe('chords from key presses', () => {
  it('spells modifiers in one order, with the physical key', () => {
    expect(chordFromEvent(press('C', { shiftKey: true, metaKey: true }, 'KeyC'))).toBe('shift+cmd+c')
    // ⌥D types ∂; the chord is still d
    expect(chordFromEvent(press('∂', { altKey: true }, 'KeyD'))).toBe('alt+d')
    expect(chordFromEvent(press(',', { metaKey: true }, 'Comma'))).toBe('cmd+,')
    expect(chordFromEvent(press(' ', { ctrlKey: true, altKey: true }, 'Space'))).toBe('ctrl+alt+space')
    expect(chordFromEvent(press('Backspace', { ctrlKey: true, shiftKey: true }))).toBe('ctrl+shift+backspace')
  })

  it('reads letters from the layout, so non-qwerty shortcuts stay where their letters are', () => {
    // dvorak: the qwerty c key types j, and the qwerty i key types c
    expect(chordFromEvent(press('j', { ctrlKey: true }, 'KeyC'))).toBe('ctrl+j')
    expect(chordFromEvent(press('c', { ctrlKey: true }, 'KeyI'))).toBe('ctrl+c')
    expect(actionForEvent(press('c', { ctrlKey: true }, 'KeyI'), {}, false)).toBe('clear')
    expect(actionForEvent(press('j', { ctrlKey: true }, 'KeyC'), {}, false)).toBeNull()
    // azerty: the qwerty q key types a
    expect(chordFromEvent(press('a', { metaKey: true }, 'KeyQ'))).toBe('cmd+a')
    // a non-latin layout types no latin letter, so the physical key stands in
    expect(chordFromEvent(press('с', { ctrlKey: true }, 'KeyC'))).toBe('ctrl+c')
    // ⌥ on dvorak still types a glyph, so it stays physical
    expect(chordFromEvent(press('ç', { altKey: true }, 'KeyC'))).toBe('alt+c')
  })

  it('waits while only modifiers are down', () => {
    expect(chordFromEvent(press('Meta', { metaKey: true }, 'MetaLeft'))).toBeNull()
    expect(chordFromEvent(press('Shift', { shiftKey: true }))).toBeNull()
  })
})

describe('defaults', () => {
  it('never clears history on the key that copies', () => {
    expect(actionForEvent(press('c', { ctrlKey: true }), {}, false)).toBe('clear')
    expect(actionForEvent(press('c', { ctrlKey: true }), {}, true)).toBe('copyAnswer')
    expect(actionForEvent(press('Backspace', { ctrlKey: true, shiftKey: true }), {}, true)).toBe('clear')
    expect(actionForEvent(press('Backspace', { ctrlKey: true }), {}, true)).toBeNull()
    expect(actionForEvent(press('c', { ctrlKey: true, altKey: true }), {}, false)).toBeNull()
    expect(actionForEvent(press('C', { metaKey: true, shiftKey: true }), {}, false)).toBe('copyLine')
  })

  it('keeps the old toggles', () => {
    for (const windows of [false, true]) {
      expect(actionForEvent(press('d', { ctrlKey: true }), {}, windows)).toBe('angle')
      expect(actionForEvent(press('f', { ctrlKey: true }), {}, windows)).toBe('fraction')
      expect(actionForEvent(press('s', { ctrlKey: true }), {}, windows)).toBe('sigFigs')
    }
  })
})

describe('choosing keys', () => {
  it('uses the chosen key and drops the old one', () => {
    const binds = setKeybind({}, 'angle', 'ctrl+alt+r', false)
    expect(actionForEvent(press('r', { ctrlKey: true, altKey: true }, 'KeyR'), binds, false)).toBe('angle')
    expect(actionForEvent(press('d', { ctrlKey: true }, 'KeyD'), binds, false)).toBeNull()
  })

  it('stores a default as no override, and none as empty', () => {
    expect(setKeybind({ angle: 'ctrl+r' }, 'angle', 'ctrl+d', false)).toEqual({})
    const none = setKeybind({}, 'clear', '', false)
    expect(none).toEqual({ clear: '' })
    expect(keybindFor(none, 'clear', false)).toBe('')
    expect(actionForEvent(press('c', { ctrlKey: true }), none, false)).toBeNull()
  })

  it('refuses typing keys, editing keys and keys already taken', () => {
    expect(chordProblem('shift+d', 'angle', {}, false)).toMatch(/typing/)
    expect(chordProblem('cmd+v', 'angle', {}, false)).toMatch(/paste/)
    expect(chordProblem('ctrl+z', 'angle', {}, true)).toMatch(/undo/)
    expect(chordProblem('ctrl+f', 'angle', {}, false)).toMatch(/fractions/)
    expect(chordProblem('ctrl+alt+space', 'angle', {}, false)).toMatch(/show/)
    expect(chordProblem('ctrl+d', 'angle', {}, false)).toBeNull()
    expect(chordProblem('f5', 'angle', {}, false)).toBeNull()
  })

  it('writes keys the way each host does', () => {
    expect(chordLabel('ctrl+alt+shift+cmd+k', false)).toBe('⌃⌥⇧⌘K')
    expect(chordLabel('ctrl+shift+backspace', true)).toBe('Ctrl+Shift+Backspace')
    expect(chordLabel('cmd+,', false)).toBe('⌘,')
    expect(chordLabel('', false)).toBe('')
  })
})

describe('stored keybinds', () => {
  it('keeps only known actions and real chords', () => {
    expect(sanitizeKeybinds({ angle: 'CMD+CTRL+R', clear: '', nope: 'ctrl+x', fraction: 'ctrl+', show: 'ctrl+space' })).toEqual({
      angle: 'ctrl+cmd+r',
      clear: '',
    })
    expect(sanitizeKeybinds('ctrl+d')).toEqual({})
    expect(keybindsEqual({ angle: 'ctrl+r' }, { angle: 'ctrl+r' })).toBe(true)
    expect(keybindsEqual({ angle: 'ctrl+r' }, {})).toBe(false)
  })
})
