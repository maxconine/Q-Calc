import { describe, expect, it } from 'vitest'
import { createEmberSound } from '../lib/emberSound'
import { CHANNEL_COLORS, channelColor, condChannels, condHolds, hash01, parseColor, Particles, PK_DOT } from './emberFx'

describe('emberFx', () => {
  it('parses the colour forms the theme uses', () => {
    expect(parseColor('#e8641c', [0, 0, 0])).toEqual([232, 100, 28])
    expect(parseColor('#fff', [0, 0, 0])).toEqual([255, 255, 255])
    expect(parseColor(' rgba(0, 0, 0, 0.08) ', [9, 9, 9])).toEqual([0, 0, 0])
    expect(parseColor('color-mix(in srgb, red, blue)', [1, 2, 3])).toEqual([1, 2, 3])
    expect(parseColor('', [1, 2, 3])).toEqual([1, 2, 3])
  })

  it('gives a channel and its negation the same stable colour', () => {
    expect(channelColor('door')).toBe(channelColor('!door'))
    expect(CHANNEL_COLORS).toContain(channelColor('anything'))
  })

  it('reads conditions', () => {
    expect(condChannels(['a', '!b'])).toEqual(['a', 'b'])
    expect(condHolds(undefined, [])).toBe(true)
    expect(condHolds(['a', '!b'], ['a'])).toBe(true)
    expect(condHolds(['a', '!b'], ['a', 'b'])).toBe(false)
  })

  it('hash01 stays in 0..1 and is deterministic', () => {
    for (let i = 0; i < 200; i++) {
      const h = hash01(i)
      expect(h).toBeGreaterThanOrEqual(0)
      expect(h).toBeLessThan(1)
      expect(hash01(i)).toBe(h)
    }
  })

  it('caps particles and lets them die', () => {
    const p = new Particles(8)
    for (let i = 0; i < 20; i++) p.spawn(PK_DOT, 0, i, 0, 0, 0, 0.5, 1)
    expect(p.n).toBe(8)
    p.update(0.3)
    expect(p.n).toBe(8)
    p.update(0.3)
    expect(p.n).toBe(0)
  })
})

describe('emberSound', () => {
  it('stays silent and never throws without webaudio', () => {
    const s = createEmberSound()
    expect(() => {
      s.play({ k: 'jump', p: 0, x: 0, y: 0 })
      s.play({ k: 'gem', p: 1, x: 0, y: 0, el: 'frost' })
      s.play({ k: 'win' })
    }).not.toThrow()
    s.setMuted(true)
    expect(s.muted).toBe(true)
  })
})
