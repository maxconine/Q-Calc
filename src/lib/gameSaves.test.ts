import { describe, expect, it } from 'vitest'
import { GAME_SAVE_MAX, hydrateGameSaves, isGameSaveKey, mirrorGameSaves, savesToHydrate, type GameSaveMessage } from './gameSaves'

// a stand-in for Storage with its methods on the prototype, as the real one has them
function storageClass() {
  class Mem {
    map = new Map<string, string>()
    reject = false
    getItem(key: string): string | null {
      return this.map.get(key) ?? null
    }
    setItem(key: string, value: string): void {
      if (this.reject) throw new Error('quota')
      this.map.set(key, value)
    }
    removeItem(key: string): void {
      this.map.delete(key)
    }
  }
  return Mem
}

describe('isGameSaveKey', () => {
  it('covers the game keys and not the rest of the app', () => {
    for (const k of ['qcalc-equal', 'qcalc-numbers', 'qcalc-crack', 'qcalc-crack-muted', 'qcalc-ember', 'qcalc-host-locked']) {
      expect(isGameSaveKey(k)).toBe(true)
    }
    for (const k of ['qcalc-history', 'qcalc-settings', 'qcalc-draft', 'qcalc-onboarding', 'qcalc-nickname', 'qcalc-crack-x', 7]) {
      expect(isGameSaveKey(k)).toBe(false)
    }
  })
})

describe('savesToHydrate', () => {
  it('keeps known keys with string values that fit', () => {
    const big = 'x'.repeat(GAME_SAVE_MAX + 1)
    const got = savesToHydrate(
      { 'qcalc-equal': '{"a":1}', 'qcalc-crack': 5, 'qcalc-history': '[]', 'qcalc-ember': big, 'qcalc-crack-muted': '1' },
      () => false,
    )
    expect(got).toEqual([
      ['qcalc-equal', '{"a":1}'],
      ['qcalc-crack-muted', '1'],
    ])
  })

  it('ignores junk', () => {
    for (const bad of [null, undefined, 'qcalc-equal', ['qcalc-equal', 'x'], 3]) expect(savesToHydrate(bad, () => false)).toEqual([])
  })

  it('never overwrites what the page already has', () => {
    expect(savesToHydrate({ 'qcalc-equal': 'old', 'qcalc-crack': '3' }, (k) => k === 'qcalc-equal')).toEqual([['qcalc-crack', '3']])
  })
})

describe('hydrateGameSaves', () => {
  it('puts saves back without clobbering newer ones', () => {
    const Mem = storageClass()
    const s = new Mem()
    s.setItem('qcalc-crack', '9')
    hydrateGameSaves({ 'qcalc-crack': '4', 'qcalc-numbers': '{}', 'qcalc-settings': '{}' }, s)
    expect(s.getItem('qcalc-crack')).toBe('9')
    expect(s.getItem('qcalc-numbers')).toBe('{}')
    expect(s.getItem('qcalc-settings')).toBeNull()
  })

  it('survives a store that rejects writes', () => {
    const Mem = storageClass()
    const s = new Mem()
    s.reject = true
    expect(() => hydrateGameSaves({ 'qcalc-crack': '4' }, s)).not.toThrow()
  })
})

describe('mirrorGameSaves', () => {
  function setup() {
    const Mem = storageClass()
    const target = new Mem()
    const other = new Mem()
    const sent: GameSaveMessage[] = []
    mirrorGameSaves(Mem.prototype, target, (m) => sent.push(m))
    return { target, other, sent }
  }

  it('sends writes and removals of game keys, and still stores them', () => {
    const { target, sent } = setup()
    target.setItem('qcalc-equal', '{"s":1}')
    target.setItem('qcalc-crack-muted', '1')
    target.removeItem('qcalc-crack-muted')
    expect(target.getItem('qcalc-equal')).toBe('{"s":1}')
    expect(target.getItem('qcalc-crack-muted')).toBeNull()
    expect(sent).toEqual([
      { type: 'gameSave', key: 'qcalc-equal', value: '{"s":1}' },
      { type: 'gameSave', key: 'qcalc-crack-muted', value: '1' },
      { type: 'gameSave', key: 'qcalc-crack-muted', value: null },
    ])
  })

  it('leaves other keys and other storages alone', () => {
    const { target, other, sent } = setup()
    target.setItem('qcalc-history', '[]')
    other.setItem('qcalc-equal', '{}')
    expect(other.getItem('qcalc-equal')).toBe('{}')
    expect(sent).toEqual([])
  })

  it('sends even when the web store rejects the write, but not values over the cap', () => {
    const { target, sent } = setup()
    target.reject = true
    expect(() => target.setItem('qcalc-crack', '7')).toThrow()
    target.reject = false
    target.setItem('qcalc-ember', 'x'.repeat(GAME_SAVE_MAX + 1))
    expect(sent).toEqual([{ type: 'gameSave', key: 'qcalc-crack', value: '7' }])
  })

  it('keeps working if posting fails', () => {
    const Mem = storageClass()
    const target = new Mem()
    mirrorGameSaves(Mem.prototype, target, () => {
      throw new Error('gone')
    })
    target.setItem('qcalc-crack', '2')
    expect(target.getItem('qcalc-crack')).toBe('2')
  })
})
