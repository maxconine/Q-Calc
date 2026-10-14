import { describe, expect, it } from 'vitest'
import { evaluateLine, isImproperUnitConversion } from '../engine'
import { missingUnit } from './missingUnit'

const ok = (text: string) => {
  const shown = evaluateLine(text).display
  return Boolean(shown) && !isImproperUnitConversion(shown)
}

describe('missingUnit', () => {
  it.each([
    ['10 mA + 5 / 1 kohm', '10 mA + 5 V / 1 kohm'],
    ['5/1 kohm + 10 mA', '5 V/1 kohm + 10 mA'],
    ['10 mA + 5 / 2 kohm', '10 mA + 5 V / 2 kohm'],
    ['10 V - 3 / 1.5 A', '10 V - 3 W / 1.5 A'],
    [
      '(10 "mA" + 5 / 1 "kohm") / (1 / 1 "kohm" + 1 / (2 "kohm" + 4 "kohm")))',
      '(10 "mA" + 5 "V" / 1 "kohm") / (1 / 1 "kohm" + 1 / (2 "kohm" + 4 "kohm")))',
    ],
  ])('%s is offered as %s', (text, filled) => {
    expect(ok(text)).toBe(false)
    const fix = missingUnit(text, ok)
    expect(fix?.filled).toBe(filled)
    expect(text.slice(0, fix!.at) + fix!.text + text.slice(fix!.at)).toBe(filled)
  })

  it('says which number and which unit', () => {
    expect(missingUnit('10 mA + 5 / 1 kohm', ok)?.hint).toBe('5 has no unit · ↵ again makes it 5 V')
  })

  it.each(['2 kg + 3 m', '10 m + 2 * 5 s', '10 mA + 5 kohm', '10 m to kg', '10 mA + 5 V / 1 kohm^2'])('%s has no unit to offer', (text) => {
    expect(missingUnit(text, ok)).toBeNull()
  })

  it('the engine itself still refuses the sum', () => {
    expect(evaluateLine('10 mA + 5 / 1 kohm').display).toBe('improper unit conversion')
  })
})
