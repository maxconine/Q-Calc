import { describe, expect, it } from 'vitest'
import { decodeC4 } from './connect4'
import { decodeEmber } from './emberProtocol'
import { helloGame, otherHello } from './hello'
import { decodePong } from './pong'
import { decodeTrails } from './trailsNet'
import { decodeUt } from './ultimate'

describe('hellos', () => {
  it('reads tagged hellos and the older untagged ones', () => {
    expect(helloGame({ t: 'hi', v: 1, g: 'pong' })).toBe('pong')
    expect(helloGame({ t: 'hi', v: 1 })).toBe('pong')
    expect(helloGame({ t: 'hi', v: 1, u: 2 })).toBe('ember')
    expect(helloGame({ t: 'hi', v: 1, u: 2, g: 'ember' })).toBe('ember')
    // ember 2's untagged hi carries its wire version instead of u
    expect(helloGame({ t: 'hi', v: 2, w: 1 })).toBe('ember')
    expect(helloGame({ t: 'c4', v: 1 })).toBe('connect4')
    expect(helloGame({ t: 'ut', v: 1 })).toBe('ultimate')
    expect(helloGame({ t: 'tr', v: 1 })).toBe('trails')
    expect(helloGame({ t: 'tr', v: 1, g: 'trails' })).toBe('trails')
    expect(helloGame({ t: 'p', y: 3 })).toBeNull()
    expect(otherHello({ t: 'hi', v: 1 }, 'pong')).toBeNull()
    expect(otherHello({ t: 'ut', v: 1 }, 'pong')).toBe('ultimate')
  })

  it('every game knows every other game’s hello', () => {
    const hellos = {
      pong: '{"t":"hi","v":1,"g":"pong"}',
      connect4: '{"t":"c4","v":1,"g":"connect4"}',
      ultimate: '{"t":"ut","v":1,"g":"ultimate"}',
      ember: '{"t":"hi","v":2,"w":1,"g":"ember"}',
      trails: '{"t":"tr","v":1,"g":"trails"}',
    }
    const decoders = { pong: decodePong, connect4: decodeC4, ultimate: decodeUt, ember: decodeEmber, trails: decodeTrails }
    for (const [mine, decode] of Object.entries(decoders)) {
      for (const [theirs, text] of Object.entries(hellos)) {
        const m = decode(text)
        if (mine === theirs) expect(m?.t, `${mine} reading its own`).not.toBe('other')
        else expect(m?.t, `${mine} reading ${theirs}`).toBe('other')
      }
    }
  })
})
