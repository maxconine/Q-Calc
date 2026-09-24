import { describe, expect, it } from 'vitest'
import { expectBlankOr } from './audit.helpers'
import { evaluateLine, evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

const GARBAGE = [
  '',
  '   ',
  'hello',
  'abc def',
  '+',
  '-',
  '*',
  '/',
  '^',
  '!',
  '%',
  '=',
  '()',
  '(((',
  ')))',
  '2+',
  '2*',
  '*2',
  '/2',
  '2^',
  '..',
  '1..2',
  '1.2.3',
  '$',
  '#',
  '@',
  '&',
  '2 $ 3',
  'x',
  'foo',
  'foo(2)',
  'sin',
  'sin()',
  'log()',
  '√',
  'sqrt()',
  'ans',
  'NaN',
  'null',
  'undefined',
  'true',
  'false',
  '[1,2',
  '{',
  '}',
  '\\',
  '?',
  '???',
  'what is',
  'calculate',
  '1 +* 2',
  'e^',
  'sin(',
  '<script>alert(1)</script>',
  'import("fs")',
  'evaluate(1+1)',
  'createUnit("foo")',
  'derivative("x^2", "x")',
  'simplify("x+x")',
  'parse("1")',
  'format(2)',
  'unit(5, "cm")',
  '2:30',
  '12:00',
  '1:5',
  '10:30 + 1',
  'graph',
  'graph x^2',
  'f(x) =',
  '= 5',
  'x = ',
  ',',
  ',,,',
  '1,,2',
  '±',
  '± 5',
  '5 ±',
  '∓',
  '° C',
  'to',
  'cm to',
  'to cm',
]

const MODES: EvaluateOptions[] = [
  {},
  { angleMode: 'rad' },
  { fractionMode: true },
  { sigFigMode: true },
  { sigFigMode: true, fractionMode: true, angleMode: 'rad' },
]

// a garbage line must come back blank (or say so in words); a bare number is a wrong answer
function looksNumeric(display: string): boolean {
  return /\d/.test(display)
}

describe('audit: garbage input is blank', () => {
  it.each(GARBAGE)('%j', (text) => {
    for (const opts of MODES) {
      const r = evaluateLine(text, opts)
      expect(looksNumeric(r.display), `${JSON.stringify(text)} → ${r.display}`).toBe(false)
    }
  })

  it('never throws on random junk', () => {
    const alphabet = '0123456789+-*/^()!%.,=:;|[]{}<>?~±∓°µΩπ√∛ eEaxsinlogmodcmkgFCH$#@&\'"\\'
    let seed = 12345
    const rand = () => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff
      return seed / 0x7fffffff
    }
    for (let i = 0; i < 600; i++) {
      const len = 1 + Math.floor(rand() * 14)
      let s = ''
      for (let k = 0; k < len; k++) s += alphabet[Math.floor(rand() * alphabet.length)]
      for (const opts of MODES) {
        expect(() => evaluateLine(s, opts), s).not.toThrow()
      }
    }
  })

  it('never throws on a sheet of junk, and the junk does not set ans', () => {
    const out = evaluateSheet(['5', ...GARBAGE, 'ans * 2'])
    expect(out[out.length - 1]!.display).toBe('10')
  })

  it('very long input does not throw', () => {
    expect(() => evaluateLine('1+'.repeat(2000) + '1')).not.toThrow()
    expect(() => evaluateLine('('.repeat(500) + '1' + ')'.repeat(500))).not.toThrow()
    expect(() => evaluateLine('sqrt('.repeat(200) + '2')).not.toThrow()
  })
})

describe('audit: stray unit symbols', () => {
  // a lone quote or mu is not a quantity; "1 in" / "1 u" is a number the user never typed
  it('a lone " is blank', () => {
    expect(evaluateLine('"').display).toBe('')
  })
  it("a lone ' is blank", () => {
    expect(evaluateLine("'").display).toBe('')
  })
  it('a lone µ is blank', () => {
    expect(evaluateLine('µ').display).toBe('')
  })
  // µ is folded to u before matching, and u is the atomic mass unit
  it('5 µ is blank, not 5 atomic mass units', () => {
    expect(evaluateLine('5 µ').display).toBe('')
  })
  // half-typed "5 kg in lb": the live answer reads "in" as inches and multiplies
  it('5 kg in (half typed) is blank or 5 kg, not 0.127 kg m', () => {
    expectBlankOr('5 kg in', ['5 kg', '11.0231131092 lbs'])
  })
  it('5 ft in (half typed) is blank or 5 ft, not 0.0387 m²', () => {
    expectBlankOr('5 ft in', ['5 ft', '1.524 m'])
  })
  // "m in" is read as a milli-inch (inches accept SI prefixes), so 5 m becomes 5 mil = 127 μm
  it('5 m in (half typed) is blank or 5 m, not 127 μm', () => {
    expectBlankOr('5 m in', ['5 m', '16.4041994751 ft'])
  })
  it('kg in is blank, not 0.0254 kg m', () => {
    expect(evaluateLine('kg in').display).toBe('')
  })
})

describe('audit: inputs that could be misread as a different number', () => {
  it('spaced thousands are 1000000 or blank', () => {
    expectBlankOr('1 000 000', ['1000000'])
    expectBlankOr('10 000', ['10000'])
  })

  it('hex, binary and octal literals are right or blank', () => {
    expectBlankOr('0x1F', ['31'])
    expectBlankOr('0b101', ['5'])
    expectBlankOr('0o17', ['15'])
  })

  it('a trailing operator is blank, not the number before it', () => {
    for (const t of ['5+', '5*', '5/', '5^', '5-']) expect(evaluateLine(t).display, t).toBe('')
  })

  it('a leading = does not change the answer', () => {
    expectBlankOr('=2+2', ['4'])
  })
})
