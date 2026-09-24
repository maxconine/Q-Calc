import { describe, expect, it } from 'vitest'
import { keepEndInView } from './dom'

const input = (value: string, start: number, end = start) => ({ value, selectionStart: start, selectionEnd: end, scrollWidth: 751, scrollLeft: 235 })

describe('keepEndInView', () => {
  it('scrolls a caret at the end back into view after the input narrows', () => {
    const el = input('lim x→∞ (1+1/x)^x+0+0', 21)
    keepEndInView(el)
    expect(el.scrollLeft).toBe(751)
  })

  it('leaves a caret mid-text or a selection where the user put it', () => {
    const mid = input('lim x→∞ (1+1/x)^x+0+0', 5)
    keepEndInView(mid)
    expect(mid.scrollLeft).toBe(235)
    const sel = input('lim x→∞ (1+1/x)^x+0+0', 3, 21)
    keepEndInView(sel)
    expect(sel.scrollLeft).toBe(235)
  })
})
