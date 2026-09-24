import { describe, expect, it } from 'vitest'
import { clampSigFigs, DEFAULT_SIG_FIGS } from '../engine/format'
import { defaultUnitsEqual, sanitizeDefaultUnits } from '../engine/units'
import { defaultSettings, mergeNativeInfo, mergeSettings, toggleAngleMode, toggleFractionMode, toggleSigFigMode } from './settings'

describe('audit2: clampSigFigs boundaries', () => {
  it.each([
    [1, 2],
    [0, 2],
    [-5, 2],
    [2, 2],
    [16, 16],
    [17, 16],
    [100, 16],
    [2.4, 2],
    [2.6, 3],
  ])('clampSigFigs(%s) = %s', (input, want) => {
    expect(clampSigFigs(input)).toBe(want)
  })

  it('a non-finite request falls back to the default', () => {
    expect(clampSigFigs(NaN)).toBe(DEFAULT_SIG_FIGS)
    expect(clampSigFigs(Infinity)).toBe(DEFAULT_SIG_FIGS)
  })
})

describe('audit2: sanitizeDefaultUnits rejects anything that does not fit', () => {
  it('drops a unit id that belongs to the wrong dimension', () => {
    expect(sanitizeDefaultUnits({ length: 'kg' })).toEqual({})
  })
  it('drops an unknown dimension key entirely', () => {
    expect(sanitizeDefaultUnits({ notADimension: 'ft' })).toEqual({})
  })
  it('drops an unknown unit id for a real dimension', () => {
    expect(sanitizeDefaultUnits({ length: 'furlongs-per-fortnight' })).toEqual({})
  })
  it('keeps a valid mapping and drops the invalid ones alongside it', () => {
    expect(sanitizeDefaultUnits({ length: 'km', mass: 'not-a-unit' })).toEqual({ length: 'km' })
  })
  it.each([null, undefined, 'a string', 42, [1, 2, 3]])('non-object input %s becomes {}', (raw) => {
    expect(sanitizeDefaultUnits(raw)).toEqual({})
  })
})

describe('audit2: defaultUnitsEqual compares the union of both sides', () => {
  it('is true for two empty objects and for identical objects', () => {
    expect(defaultUnitsEqual({}, {})).toBe(true)
    expect(defaultUnitsEqual({ length: 'ft' }, { length: 'ft' })).toBe(true)
  })
  it('is false when one side has an extra key the other lacks', () => {
    expect(defaultUnitsEqual({ length: 'ft' }, {})).toBe(false)
    expect(defaultUnitsEqual({}, { length: 'ft' })).toBe(false)
  })
  it('is false when the same key maps to different units', () => {
    expect(defaultUnitsEqual({ length: 'ft' }, { length: 'm' })).toBe(false)
  })
})

describe('audit2: mergeNativeInfo keeps the base unless the pushed value has the right type', () => {
  const base = { hotkey: '⌃⌥Space', hotkeyFailed: false }
  it('accepts a matching partial', () => {
    expect(mergeNativeInfo({ hotkey: '⌘K', hotkeyFailed: true }, base)).toEqual({ hotkey: '⌘K', hotkeyFailed: true })
  })
  it('ignores wrongly-typed fields and keeps the base', () => {
    expect(mergeNativeInfo({ hotkey: 5 as unknown as string }, base)).toEqual(base)
    expect(mergeNativeInfo({ hotkeyFailed: 'yes' as unknown as boolean }, base)).toEqual(base)
  })
  it('an undefined partial keeps the base untouched', () => {
    expect(mergeNativeInfo(undefined, base)).toEqual(base)
  })
})

describe('audit2: the three toggle helpers only flip their own field', () => {
  it('toggleAngleMode flips deg/rad and back', () => {
    const s = defaultSettings()
    expect(toggleAngleMode(s).angleMode).toBe('rad')
    expect(toggleAngleMode(toggleAngleMode(s)).angleMode).toBe('deg')
  })
  it('toggleFractionMode and toggleSigFigMode do not touch each other or angleMode', () => {
    const s = defaultSettings()
    const t = toggleFractionMode(toggleSigFigMode(s))
    expect(t.fractionMode).toBe(true)
    expect(t.sigFigMode).toBe(true)
    expect(t.angleMode).toBe(s.angleMode)
  })
})

describe('audit2: mergeSettings ignores unrecognized values instead of adopting them', () => {
  it('a garbage angleMode or answerForm falls back to the base', () => {
    const s = defaultSettings()
    expect(mergeSettings({ angleMode: 'grad' as never }, s).angleMode).toBe(s.angleMode)
    expect(mergeSettings({ answerForm: 'both' as never }, s).answerForm).toBe(s.answerForm)
  })
  it('sigFigs is clamped through mergeSettings, not just stored raw', () => {
    const s = defaultSettings()
    expect(mergeSettings({ sigFigs: 999 }, s).sigFigs).toBe(16)
    expect(mergeSettings({ sigFigs: 0 }, s).sigFigs).toBe(2)
  })
})
