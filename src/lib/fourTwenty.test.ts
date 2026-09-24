import { describe, expect, it } from 'vitest'
import { fourTwentyGate } from './fourTwenty'

function run(answers: Array<number | undefined>): number {
  let armed = true
  let fired = 0
  for (const n of answers) {
    const next = fourTwentyGate(armed, n)
    armed = next.armed
    if (next.fire) fired++
  }
  return fired
}

describe('fourTwentyGate', () => {
  it('fires once when the answer settles on 420', () => {
    expect(run([42, 420])).toBe(1)
  })

  it('does not refire while the answer stays 420', () => {
    expect(run([420, 420, 420])).toBe(1)
  })

  it('re-arms only after the answer changes', () => {
    expect(run([420, undefined, 420])).toBe(2)
    expect(run([420, 4200, 420])).toBe(2)
  })

  it('never fires for other answers', () => {
    expect(run([42, 4200, 42.0, undefined])).toBe(0)
  })
})
