import { describe, expect, it } from 'vitest'
import { GameClock } from './gameClock'
import { ARM_MS, escAction, pausedKey, type EscState } from './gameShell'

const base: EscState = { mode: 'local', fullscreen: false, paused: false, armedAt: null, idle: false, now: 10_000 }
const key = (k: string, mods: Partial<Record<'metaKey' | 'ctrlKey' | 'altKey', boolean>> = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, ...mods })

describe('esc around a game', () => {
  it('leaves full screen before anything else', () => {
    for (const mode of ['lobby', 'local', 'online'] as const) expect(escAction({ ...base, mode, fullscreen: true, paused: true })).toBe('unfullscreen')
  })

  it('steps back through the lobby', () => {
    expect(escAction({ ...base, mode: 'lobby' })).toBe('lobby')
  })

  it('pauses a game on this computer, then leaves', () => {
    expect(escAction(base)).toBe('pause')
    expect(escAction({ ...base, paused: true })).toBe('leave')
    // nothing running, nothing to pause
    expect(escAction({ ...base, idle: true })).toBe('leave')
  })

  it('warns in an online game, and leaves on a second esc soon after', () => {
    const online = { ...base, mode: 'online' as const }
    expect(escAction(online)).toBe('arm')
    expect(escAction({ ...online, armedAt: base.now - 500 })).toBe('leave')
    expect(escAction({ ...online, armedAt: base.now - ARM_MS - 1 })).toBe('arm')
    // an online game is never paused, even if asked
    expect(escAction({ ...online, idle: true })).toBe('arm')
  })
})

describe('keys while paused', () => {
  it('resumes on a plain key, flips full screen on f, and lets esc and shortcuts through', () => {
    expect(pausedKey(key(' '))).toBe('resume')
    expect(pausedKey(key('ArrowUp'))).toBe('resume')
    expect(pausedKey(key('f'))).toBe('fullscreen')
    expect(pausedKey(key('F'))).toBe('fullscreen')
    expect(pausedKey(key('Escape'))).toBe('pass')
    expect(pausedKey(key('Shift'))).toBe('pass')
    expect(pausedKey(key('c', { metaKey: true }))).toBe('pass')
    // ⌃F goes on to useGameFullscreen
    expect(pausedKey(key('f', { ctrlKey: true }))).toBe('pass')
  })
})

describe('the game clock', () => {
  it('stands still while paused and carries on from there', () => {
    let mono = 1000
    let wall = 1_700_000_000_000
    const clock = new GameClock(() => mono, () => wall)
    expect(clock.now()).toBe(1000)
    clock.pause()
    mono += 5000
    wall += 5000
    expect(clock.now()).toBe(1000)
    expect(clock.date()).toBe(1_700_000_000_000)
    clock.resume()
    mono += 10
    wall += 10
    expect(clock.now()).toBe(1010)
    expect(clock.date()).toBe(1_700_000_000_010)
    expect(clock.paused).toBe(false)
  })
})
