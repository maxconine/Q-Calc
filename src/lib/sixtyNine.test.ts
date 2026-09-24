import { describe, expect, it } from 'vitest'
import { sixtyNineGate } from './sixtyNine'

function run(answers: Array<number | undefined>): number {
  let armed = true
  let fired = 0
  for (const n of answers) {
    const next = sixtyNineGate(armed, n)
    armed = next.armed
    if (next.fire) fired++
  }
  return fired
}

describe('sixtyNineGate', () => {
  it('fires once when the answer settles on 69', () => {
    expect(run([6, 69])).toBe(1)
  })

  it('does not refire while the answer stays 69', () => {
    expect(run([69, 69, 69])).toBe(1)
  })

  it('re-arms only after the answer changes', () => {
    expect(run([69, undefined, 69])).toBe(2)
    expect(run([69, 690, 69])).toBe(2)
  })

  it('never fires for other answers', () => {
    expect(run([6, 690, 6.9, undefined])).toBe(0)
  })
})
