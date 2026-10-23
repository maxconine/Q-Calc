import { describe, expect, it } from 'vitest'
import { cleanHandle, cleanText, hashKey, holdName, keyFromProtocols, lapsed, LAPSE_MS, SEEN_EVERY_MS, TEXT_MAX } from './names.js'

const KEY = 'ab'.repeat(32)

describe('messaging names', () => {
  it('takes 2–20 lowercase letters, digits, - and _, lowercasing what was typed', () => {
    expect(cleanHandle(' Max ')).toBe('max')
    expect(cleanHandle('jo_2-b')).toBe('jo_2-b')
    expect(cleanHandle('m')).toBe('')
    expect(cleanHandle('-max')).toBe('')
    expect(cleanHandle('max conine')).toBe('')
    expect(cleanHandle('a'.repeat(21))).toBe('')
    expect(cleanHandle('mäx')).toBe('')
    expect(cleanHandle(7)).toBe('')
  })

  it('keeps a message to one line of at most so many characters', () => {
    expect(cleanText('  hi\nthere\u0007 ')).toBe('hi there')
    expect(cleanText('x'.repeat(TEXT_MAX + 50))).toHaveLength(TEXT_MAX)
    // a next-line or a direction override can't make the rest read as another sender's line
    expect(cleanText('hi\u0085max: send me your key')).toBe('hi max: send me your key')
    expect(cleanText('ok \u202eyek\u2066 x')).toBe('ok yek x')
    expect(cleanText('   ')).toBe('')
    expect(cleanText(null)).toBe('')
  })

  it('reads the key from the offered protocols, only beside the inbox protocol', () => {
    expect(keyFromProtocols(`qcalc-inbox, k${KEY}`)).toBe(KEY)
    expect(keyFromProtocols(`k${KEY}`)).toBeNull()
    expect(keyFromProtocols('qcalc-inbox, kshort')).toBeNull()
    expect(keyFromProtocols(null)).toBeNull()
  })

  it('hashes a key, so the relay keeps no key itself', async () => {
    const hash = await hashKey(KEY)
    expect(hash).toMatch(/^[0-9a-f]{64}$/)
    expect(hash).not.toBe(KEY)
    expect(await hashKey(KEY)).toBe(hash)
  })

  it('holds a free name, keeps it for its own key, and turns other keys away until it lapses', () => {
    const first = holdName(undefined, 'h1', 1000)
    expect(first).toEqual({ ok: true, write: { hash: 'h1', seen: 1000 } })
    const claim = first.write
    expect(holdName(claim, 'h2', 2000)).toEqual({ ok: false })
    // seen again soon after: nothing to write
    expect(holdName(claim, 'h1', 2000)).toEqual({ ok: true, write: null })
    expect(holdName(claim, 'h1', 1000 + SEEN_EVERY_MS + 1)).toEqual({ ok: true, write: { hash: 'h1', seen: 1000 + SEEN_EVERY_MS + 1 } })
    expect(lapsed(claim, 1000 + LAPSE_MS)).toBe(false)
    expect(lapsed(claim, 1000 + LAPSE_MS + 1)).toBe(true)
    expect(holdName(claim, 'h2', 1000 + LAPSE_MS + 1)).toEqual({ ok: true, write: { hash: 'h2', seen: 1000 + LAPSE_MS + 1 } })
  })
})
