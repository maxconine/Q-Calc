import { describe, expect, it } from 'vitest'
import {
  backingSize,
  fullscreenHint,
  fullscreenLabel,
  fullscreenLayout,
  isFullscreenChord,
  isPlainEscape,
  letterbox,
  macLike,
  naturalSize,
} from './gameFullscreen'

const key = (over: Partial<Parameters<typeof isFullscreenChord>[0]>) => ({
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  key: 'f',
  code: 'KeyF',
  ...over,
})

describe('the fullscreen chord', () => {
  it('is ctrl+F in either letter case', () => {
    expect(isFullscreenChord(key({ ctrlKey: true }))).toBe(true)
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'F' }))).toBe(true)
  })

  it('is not F alone, ⌘F, or ctrl with another modifier', () => {
    expect(isFullscreenChord(key({}))).toBe(false)
    expect(isFullscreenChord(key({ metaKey: true }))).toBe(false)
    expect(isFullscreenChord(key({ ctrlKey: true, metaKey: true }))).toBe(false)
    expect(isFullscreenChord(key({ ctrlKey: true, shiftKey: true, key: 'F' }))).toBe(false)
    expect(isFullscreenChord(key({ ctrlKey: true, altKey: true }))).toBe(false)
  })

  it('follows the layout’s letter, not the qwerty position', () => {
    // dvorak: the key labelled F sits where qwerty has Y, and qwerty's F key types U
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'f', code: 'KeyY' }))).toBe(true)
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'u', code: 'KeyF' }))).toBe(false)
  })

  it('falls back to the qwerty key on a layout without latin letters', () => {
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'а', code: 'KeyF' }))).toBe(true)
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'ф', code: 'KeyA' }))).toBe(false)
  })

  it('takes the control character ctrl+F types', () => {
    expect(isFullscreenChord(key({ ctrlKey: true, key: '\u0006' }))).toBe(true)
  })

  it('ignores other keys with ctrl', () => {
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'd', code: 'KeyD' }))).toBe(false)
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'Enter', code: 'Enter' }))).toBe(false)
    expect(isFullscreenChord(key({ ctrlKey: true, key: 'F5', code: 'F5' }))).toBe(false)
  })
})

describe('escape', () => {
  it('is escape with nothing held', () => {
    expect(isPlainEscape(key({ key: 'Escape' }))).toBe(true)
    expect(isPlainEscape(key({ key: 'Esc' }))).toBe(true)
    expect(isPlainEscape(key({ key: 'Escape', metaKey: true }))).toBe(false)
    expect(isPlainEscape(key({ key: 'f' }))).toBe(false)
  })
})

describe('the hint', () => {
  it('uses the glyph on a mac and the word elsewhere', () => {
    expect(fullscreenHint(true)).toBe('⌃F fullscreen')
    expect(fullscreenHint(false)).toBe('Ctrl+F fullscreen')
  })

  it('says how to leave while fullscreen', () => {
    expect(fullscreenHint(true, true)).toBe('⌃F or esc leaves fullscreen')
    expect(fullscreenHint(false, true)).toBe('Ctrl+F or esc leaves fullscreen')
  })

  it('in the apps, says it opens a big window and goes back to the bar', () => {
    expect(fullscreenHint(true, false, true)).toBe('⌃F big window')
    expect(fullscreenHint(false, true, true)).toBe('Ctrl+F or esc back to the bar')
    expect(fullscreenLabel(false, true)).toBe('big window')
    expect(fullscreenLabel(true, true)).toBe('back to the bar')
    expect(fullscreenLabel(false)).toBe('full screen')
    expect(fullscreenLabel(true)).toBe('leave full screen')
  })

  it('knows a mac from the platform, and the windows shell from its own word', () => {
    expect(macLike(false, 'MacIntel')).toBe(true)
    expect(macLike(false, 'Win32')).toBe(false)
    expect(macLike(false, 'Linux x86_64')).toBe(false)
    expect(macLike(true, 'MacIntel')).toBe(false)
    expect(macLike(false, '')).toBe(false)
  })
})

describe('letterbox', () => {
  const game = { w: 640, h: 360 }

  it('fills a screen of the same shape', () => {
    expect(letterbox({ w: 1920, h: 1080 }, game)).toEqual({ x: 0, y: 0, w: 1920, h: 1080, scale: 3 })
  })

  it('bars above and below on a taller screen', () => {
    // a 16:10 macbook
    expect(letterbox({ w: 1440, h: 900 }, game)).toEqual({ x: 0, y: 45, w: 1440, h: 810, scale: 2.25 })
  })

  it('bars left and right on a wider screen', () => {
    expect(letterbox({ w: 3440, h: 1440 }, game)).toEqual({ x: 440, y: 0, w: 2560, h: 1440, scale: 4 })
  })

  it('keeps the shape and stays inside on awkward sizes', () => {
    for (const c of [
      { w: 1512, h: 982 },
      { w: 1366, h: 768 },
      { w: 1001, h: 563 },
      { w: 333, h: 777 },
      { w: 700.5, h: 393.7 },
    ]) {
      const box = letterbox(c, game)
      expect(box.w).toBeLessThanOrEqual(c.w)
      expect(box.h).toBeLessThanOrEqual(c.h)
      expect(Number.isInteger(box.w) && Number.isInteger(box.h) && Number.isInteger(box.x) && Number.isInteger(box.y)).toBe(true)
      expect(Math.abs(box.w / box.h - 16 / 9)).toBeLessThan(0.01)
      // touches one pair of edges
      expect(c.w - box.w < 2 || c.h - box.h < 2).toBe(true)
      // centred to the pixel (a fractional container leaves up to one more)
      expect(Math.abs(c.w - box.w - 2 * box.x)).toBeLessThan(2)
      expect(Math.abs(c.h - box.h - 2 * box.y)).toBeLessThan(2)
    }
  })

  it('snaps to a whole scale when asked', () => {
    expect(letterbox({ w: 1440, h: 900 }, game, { integer: true })).toEqual({ x: 80, y: 90, w: 1280, h: 720, scale: 2 })
    // no room for a whole one: shrinks to fit rather than overflow
    expect(letterbox({ w: 480, h: 400 }, game, { integer: true })).toEqual({ x: 0, y: 65, w: 480, h: 270, scale: 0.75 })
  })

  it('is empty for an empty container or game', () => {
    const none = { x: 0, y: 0, w: 0, h: 0, scale: 0 }
    expect(letterbox({ w: 0, h: 900 }, game)).toEqual(none)
    expect(letterbox({ w: 100, h: -1 }, game)).toEqual(none)
    expect(letterbox({ w: 100, h: 100 }, { w: 0, h: 360 })).toEqual(none)
    expect(letterbox({ w: NaN, h: 100 }, game)).toEqual(none)
  })
})

describe('the fullscreen layout', () => {
  const base = { w: 640, h: 360 }

  it('gives the picture everything when the panel has no chrome', () => {
    const l = fullscreenLayout({ w: 1920, h: 1080 }, { base })
    expect(l.stage).toEqual({ x: 0, y: 0, w: 1920, h: 1080, scale: 3 })
    expect(l.ui).toBe(2)
  })

  it('leaves the hud its room, scaled with the screen', () => {
    const l = fullscreenLayout({ w: 1440, h: 900 }, { base, chrome: 90, pad: 12 })
    // the whole small panel is 664 × 474, which fits 1.89 times
    expect(l.ui).toBe(1.89)
    expect(l.stage.h + 90 * l.ui + 2 * 12 * l.ui).toBeLessThanOrEqual(900)
    expect(l.stage.w + 2 * 12 * l.ui).toBeLessThanOrEqual(1440)
    expect(Math.abs(l.stage.w / l.stage.h - 16 / 9)).toBeLessThan(0.01)
    // centred across, and inside the screen
    expect(Math.abs(1440 - l.stage.w - 2 * l.stage.x)).toBeLessThanOrEqual(2)
    expect(l.stage.y).toBeGreaterThan(0)
    expect(l.stage.y + l.stage.h).toBeLessThanOrEqual(900)
  })

  it('never shrinks the hud below its normal size, nor past the cap', () => {
    expect(fullscreenLayout({ w: 500, h: 300 }, { base, chrome: 90 }).ui).toBe(1)
    expect(fullscreenLayout({ w: 5120, h: 2880 }, { base, chrome: 90 }).ui).toBe(2)
    expect(fullscreenLayout({ w: 5120, h: 2880 }, { base, chrome: 90, maxUi: 3 }).ui).toBe(3)
  })

  it('still fits a picture in a window smaller than the panel', () => {
    const l = fullscreenLayout({ w: 500, h: 300 }, { base, chrome: 90 })
    expect(l.stage.h).toBeLessThanOrEqual(210)
    expect(l.stage.w).toBeGreaterThan(0)
  })

  it('has no picture on no screen', () => {
    expect(fullscreenLayout({ w: 0, h: 0 }, { base }).stage.w).toBe(0)
  })
})

describe('the natural size', () => {
  it('is the picture with the chrome and a margin each side, the shape the app window takes', () => {
    expect(naturalSize({ base: { w: 640, h: 360 }, chrome: 96, pad: 12 })).toEqual({ w: 664, h: 480 })
    expect(naturalSize({ base: { w: 640, h: 360 } })).toEqual({ w: 640, h: 360 })
  })
})

describe('the backing store', () => {
  it('is the css size in device pixels', () => {
    expect(backingSize({ w: 1440, h: 810 }, 2)).toEqual({ w: 2880, h: 1620 })
    expect(backingSize({ w: 640, h: 360 }, 1)).toEqual({ w: 640, h: 360 })
    expect(backingSize({ w: 853, h: 480 }, 1.5)).toEqual({ w: 1280, h: 720 })
  })

  it('caps the ratio and never goes below one', () => {
    expect(backingSize({ w: 100, h: 50 }, 4)).toEqual({ w: 300, h: 150 })
    expect(backingSize({ w: 100, h: 50 }, 4, 2)).toEqual({ w: 200, h: 100 })
    expect(backingSize({ w: 100, h: 50 }, 0.5)).toEqual({ w: 100, h: 50 })
    expect(backingSize({ w: 100, h: 50 }, NaN)).toEqual({ w: 100, h: 50 })
  })
})
