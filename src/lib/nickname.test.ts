import { describe, expect, it } from 'vitest'
import { cleanNickname, NICKNAME_MAX, randomNickname } from './nickname'

describe('nicknames', () => {
  it('picks a colour and an animal', () => {
    expect(randomNickname(() => 0)).toBe('teal otter')
    expect(randomNickname(() => 0.9999)).toMatch(/^[a-z]+ [a-z]+$/)
  })

  it('cleans what is typed or heard', () => {
    expect(cleanNickname('  Teal   Otter  ')).toBe('Teal Otter')
    expect(cleanNickname('<b>x</b>')).toBe('bxb')
    expect(cleanNickname('a'.repeat(40))).toHaveLength(NICKNAME_MAX)
    expect(cleanNickname('\u0000\n')).toBe('')
    expect(cleanNickname(42)).toBe('')
    expect(cleanNickname('zoë')).toBe('zoë')
  })
})
