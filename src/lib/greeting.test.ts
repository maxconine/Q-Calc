import { describe, expect, it } from 'vitest'
import { isGreeting } from './greeting'

describe('isGreeting', () => {
  it.each(['hello', 'Hello', 'HELLO!', ' hello ', 'hello there!', 'howdy'])('says hi back to %s', (text) => {
    expect(isGreeting(text)).toBe(true)
  })

  it.each(['', 'h', 'hel', 'hi', 'hey', 'hello world', 'hello = 5', 'height', 'hex(255)'])('leaves %s to the calculator', (text) => {
    expect(isGreeting(text)).toBe(false)
  })
})
