import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { prettyTokens } from '../components/QuickInput'
import { afterTyping, boundKey, boundsIn, isHidden, snapOffset, wordToSign } from './bounds'

// the limits' text, the way the input draws them
function limits(text: string): [string, string | null][] {
  return boundsIn(text).map((b) => [
    text.slice(b.lower.start, b.lower.end),
    b.upper ? text.slice(b.upper.start, b.upper.end) : null,
  ])
}

const shown = (line: string) => evaluateSheet([line], { angleMode: 'rad' })[0]?.display ?? ''

// keys the way QuickInput sends them: characters through prettyTokens and afterTyping, others through boundKey
function typeKeys(keys: (string | { key: string })[]): { text: string; caret: number } {
  let text = ''
  let caret = 0
  for (const k of keys) {
    if (typeof k !== 'string') {
      const edit = boundKey(text, caret, k.key) ?? (k.key === 'Tab' || k.key === 'ArrowRight' ? wordToSign(text, caret) : null)
      if (edit) ({ text, caret } = edit)
      else if (k.key === 'Backspace' && caret > 0) {
        text = text.slice(0, caret - 1) + text.slice(caret)
        caret--
      } else if (k.key === ' ') {
        text = text.slice(0, caret) + ' ' + text.slice(caret)
        caret++
      }
      continue
    }
    for (const ch of k) {
      const raw = text.slice(0, caret) + ch + text.slice(caret)
      const at = caret + 1
      const before = prettyTokens(raw.slice(0, at), at)
      text = prettyTokens(raw, at)
      caret = Math.min(before.length, text.length)
      const slot = afterTyping(text, caret)
      if (slot) ({ text, caret } = slot)
    }
  }
  return { text, caret }
}

const right = { key: 'ArrowRight' }
const tab = { key: 'Tab' }

describe('boundsIn', () => {
  it.each([
    ['∫_0^1 x^2', [['0', '1']]],
    ['∫0..1 x^2', [['0', '1']]],
    ['∫_{0}^{1} x^2', [['0', '1']]],
    ['∫_{-1}^{2^3} x', [['-1', '2^3']]],
    ['∫_0^π/2 sin(x)', [['0', 'π/2']]],
    ['∫_0^sin(1) x', [['0', 'sin(1)']]],
    ['∫_0^2(x+1)', [['0', '2']]],
    ['∫0..∞ e^-x', [['0', '∞']]],
    ['∫', [['', null]]],
    ['∫_', [['', null]]],
    ['∫_0', [['0', null]]],
    ['∫_0^', [['0', '']]],
    ['∫0..', [['0', '']]],
    ['Σ_n=1^10 n^2', [['n=1', '10']]],
    ['Σ_{n=1}^{10} n^2', [['n=1', '10']]],
    ['Π_n=1^5 n', [['n=1', '5']]],
    ['Σ_n=1^(2+3) n', [['n=1', '2+3']]],
  ])('%s', (text, want) => {
    expect(limits(text)).toEqual(want)
  })

  it.each(['∫(x^2, 0, 1)', '∫ x^2 from 0 to 1', '∫x^2 dx', 'Σ n^2, n = 1..10', 'Σ(1, 2, 3)', '2 + 3', 'integral of x'])(
    'leaves %s as text',
    (text) => {
      expect(boundsIn(text)).toEqual([])
    },
  )

  it('reads the limits where the engine does', () => {
    // the drawn upper limit is exactly what's integrated to
    for (const [text, upper] of [
      ['∫_0^2(x+1)', '∫_0^2 (x+1)'],
      ['∫_0^sin(1) x', '∫_0^{sin(1)} x'],
      ['∫0..π/2 cos(x)', '∫_0^{π/2} cos(x)'],
    ] as const) {
      expect(shown(text)).toBe(shown(upper))
      expect(shown(text)).not.toBe('')
    }
  })
})

describe('hidden offsets', () => {
  it('hides the scaffolding between the limits', () => {
    const text = '∫_0^1 x'
    const b = boundsIn(text)
    expect([...Array(text.length + 1).keys()].filter((o) => isHidden(b, o))).toEqual([1])
    expect(snapOffset(b, 1, 1, text.length)).toBe(2)
    expect(snapOffset(b, 1, -1, text.length)).toBe(0)
  })

  it('hides braces and dots', () => {
    const braced = '∫_{0}^{1} x'
    expect([...Array(braced.length + 1).keys()].filter((o) => isHidden(boundsIn(braced), o))).toEqual([1, 2, 5, 6])
    const dots = '∫0..1 x'
    expect([...Array(dots.length + 1).keys()].filter((o) => isHidden(boundsIn(dots), o))).toEqual([3])
  })
})

describe('typing limits', () => {
  it('int, then the lower limit, then the upper, then the body', () => {
    const r = typeKeys(['int0', right, '1', right, 'x^2'])
    expect(r.text).toBe('∫_0^1 x^2')
    expect(shown(r.text)).toBe('0.333333333333')
  })

  it('tab moves on the same way', () => {
    expect(typeKeys(['int0', tab, '1', tab, 'x^2']).text).toBe('∫_0^1 x^2')
  })

  it('typed dots and ^ move to the upper limit too', () => {
    expect(typeKeys(['int0..1 x^2']).text).toBe('∫_0^1 x^2')
    expect(typeKeys(['int0^1 x^2']).text).toBe('∫_0^1 x^2')
    expect(typeKeys(['int-1..2 x']).text).toBe('∫_-1^2 x')
    expect(shown(typeKeys(['int0.5..1 x']).text)).toBe(shown('∫0.5..1 x'))
  })

  it('a space ends the upper limit', () => {
    expect(typeKeys(['int0', right, 'π/2 cos(x)']).text).toBe('∫_0^π/2 cos(x)')
    expect(shown('∫_0^π/2 cos(x)')).toBe('1')
  })

  it('the limits start right after the word, no key needed', () => {
    expect(typeKeys(['int'])).toEqual({ text: '∫', caret: 1 })
    expect(typeKeys(['int', '0', right, '2', right, 'x']).text).toBe('∫_0^2 x')
  })

  it('sum and prod the same way', () => {
    const r = typeKeys(['sum', 'n=1', tab, '10', tab, 'n^2'])
    expect(r.text).toBe('Σ_n=1^10 n^2')
    expect(shown(r.text)).toBe('385')
    expect(typeKeys(['sum_n=1', right, '4', right, 'n']).text).toBe('Σ_n=1^4 n')
    expect(shown(typeKeys(['prod', 'n=1', right, '5', right, 'n']).text)).toBe('120')
  })

  it('other forms still type as before', () => {
    expect(typeKeys(['int(x^2, 0, 1)']).text).toBe('∫(x^2, 0, 1)')
    expect(typeKeys(['int x^2 from 0 to 1']).text).toBe('∫ x^2 from 0 to 1')
    expect(typeKeys(['sum n^2, n = 1..10']).text).toBe('Σ n^2, n = 1..10')
    expect(typeKeys(['integral of x^2 from 0 to 1']).text).toBe('integral of x^2 from 0 to 1')
  })

  it('→ inside a limit just moves the caret', () => {
    expect(boundKey('∫_10^1 x', 3, 'ArrowRight')).toBeNull()
    expect(boundKey('∫_0^1 x', 3, 'ArrowRight')).toEqual({ text: '∫_0^1 x', caret: 4 })
    expect(boundKey('∫_0^1 x', 5, 'ArrowRight')).toEqual({ text: '∫_0^1 x', caret: 6 })
  })

  it('an open paren in a limit is left for → to close', () => {
    expect(boundKey('∫_0^sin(1', 9, 'ArrowRight')).toBeNull()
  })

  it('skipping an empty upper limit keeps it empty', () => {
    const r = typeKeys(['int0', right, right, 'x'])
    expect(r.text).toBe('∫_0^{} x')
    expect(limits(r.text)).toEqual([['0', '']])
    expect(shown(r.text)).toBe('')
  })

  it('a space in an empty limit does nothing', () => {
    expect(typeKeys(['int0', right, { key: ' ' }, '1']).text).toBe('∫_0^1')
  })
})

describe('backspace through the limits', () => {
  it('empties everything one step at a time', () => {
    let r = typeKeys(['int0', right, '1', right, 'x'])
    const seen: string[] = []
    while (r.text && seen.length < 12) {
      r = boundKey(r.text, r.caret, 'Backspace') ?? { text: r.text.slice(0, r.caret - 1) + r.text.slice(r.caret), caret: r.caret - 1 }
      seen.push(`${r.text}|${r.caret}`)
    }
    expect(seen).toEqual(['∫_0^1 |6', '∫_0^1|5', '∫_0^|4', '∫_0|3', '∫_|2', '∫|1', '|0'])
  })

  it('steps back into the upper limit from the body', () => {
    expect(boundKey('∫_0^1 x^2', 6, 'Backspace')).toEqual({ text: '∫_0^1 x^2', caret: 5 })
  })

  it('steps back into the lower limit from a filled upper', () => {
    expect(boundKey('∫_0^1 x', 4, 'Backspace')).toEqual({ text: '∫_0^1 x', caret: 3 })
  })

  it('leaves the limits from the start of the lower one', () => {
    expect(boundKey('∫_0^1 x', 2, 'Backspace')).toEqual({ text: '∫_0^1 x', caret: 0 })
  })

  it('never deletes a hidden brace', () => {
    expect(boundKey('∫_{0}^{1} x', 9, 'Backspace')).toEqual({ text: '∫_{0}^{1} x', caret: 8 })
  })

  it('leaves text without limits alone', () => {
    expect(boundKey('2 + 3', 5, 'Backspace')).toBeNull()
    expect(boundKey('∫', 1, 'Backspace')).toBeNull()
  })
})

describe('keeping words', () => {
  it('int stays text, so no limits', () => {
    let text = ''
    for (const ch of 'int0') {
      text = prettyTokens(text + ch, text.length + 1, true)
      text = afterTyping(text, text.length)?.text ?? text
    }
    expect(text).toBe('int0')
    expect(boundsIn(text)).toEqual([])
  })

  it('a typed ∫ still gets them', () => {
    expect(afterTyping('∫0', 2)).toEqual({ text: '∫_0', caret: 3 })
  })
})

describe('↑, ↓ and enter between the limits', () => {
  it('↑ or enter in the lower limit goes to the upper', () => {
    expect(boundKey('∫_0', 3, 'ArrowUp')).toEqual({ text: '∫_0^', caret: 4 })
    expect(boundKey('∫_0', 3, 'Enter')).toEqual({ text: '∫_0^', caret: 4 })
    expect(boundKey('∫_0^1 x', 3, 'ArrowUp')).toEqual({ text: '∫_0^1 x', caret: 4 })
    expect(boundKey('∫_10^1 x', 2, 'ArrowUp')).toEqual({ text: '∫_10^1 x', caret: 5 })
  })

  it('↓ in the upper limit comes back to the lower', () => {
    expect(boundKey('∫_0^1 x', 5, 'ArrowDown')).toEqual({ text: '∫_0^1 x', caret: 3 })
  })

  it('leaves ↑, ↓ and enter alone outside the limits', () => {
    expect(boundKey('∫_0^1 x', 7, 'ArrowUp')).toBeNull()
    expect(boundKey('∫_0^1 x', 7, 'Enter')).toBeNull()
    expect(boundKey('∫_0^1 x', 3, 'ArrowDown')).toBeNull()
  })
})
