import { describe, expect, it, vi } from 'vitest'
import { FOUND_MS, MISSING_MS, NameCheck } from './nameCheck'

describe('checking a name', () => {
  it('asks once, keeps what the relay said, and asks again once it’s stale', async () => {
    let now = 0
    const ask = vi.fn(async (name: string) => name === 'max')
    const check = new NameCheck(ask, () => now)
    expect(check.cached('max')).toBeNull()
    expect(await check.check('max')).toBe(true)
    expect(await check.check('max')).toBe(true)
    expect(await check.check('zed')).toBe(false)
    expect(ask).toHaveBeenCalledTimes(2)
    now = MISSING_MS + 1
    expect(check.cached('zed')).toBeNull()
    expect(check.cached('max')).toBe(true)
    now = FOUND_MS + 1
    expect(check.cached('max')).toBeNull()
  })

  it('keeps nothing when the relay couldn’t say, so offline never reads as not found', async () => {
    const ask = vi.fn(async () => null)
    const check = new NameCheck(ask)
    expect(await check.check('max')).toBeNull()
    expect(check.cached('max')).toBeNull()
    const failing = new NameCheck(async () => {
      throw new Error('offline')
    })
    expect(await failing.check('max')).toBeNull()
    expect(failing.cached('max')).toBeNull()
  })

  it('shares one ask among checks of the same name made while it’s out', async () => {
    let answer: (v: boolean) => void = () => {}
    const ask = vi.fn(() => new Promise<boolean>((resolve) => (answer = resolve)))
    const check = new NameCheck(ask)
    const a = check.check('max')
    const b = check.check('max')
    answer(true)
    expect(await Promise.all([a, b])).toEqual([true, true])
    expect(ask).toHaveBeenCalledTimes(1)
  })

  it('takes what a send found out', () => {
    const check = new NameCheck(async () => null)
    check.note('zed', false)
    expect(check.cached('zed')).toBe(false)
    check.note('zed', true)
    expect(check.cached('zed')).toBe(true)
  })
})
