import { describe, expect, it } from 'vitest'
import { sixtySevenGate } from './sixtySeven'

function run(answers: Array<number | undefined>): number {
  let armed = true
  let fired = 0
  for (const n of answers) {
    const next = sixtySevenGate(armed, n)
    armed = next.armed
    if (next.fire) fired++
  }
  return fired
}

describe('sixtySevenGate', () => {
  it('fires once when the answer settles on 67', () => {
    expect(run([6, 67])).toBe(1)
  })

  it('does not refire while the answer stays 67', () => {
    expect(run([67, 67, 67])).toBe(1)
  })

  it('re-arms only after the answer changes', () => {
    expect(run([67, undefined, 67])).toBe(2)
    expect(run([67, 670, 67])).toBe(2)
  })

  it('never fires for other answers', () => {
    expect(run([6, 670, 6.7, undefined])).toBe(0)
  })
})
