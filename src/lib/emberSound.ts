import type { GameEvent } from './emberTypes'

// ember & frost's sounds.
// STUB: the render agent replaces this; the exported shape is the contract
export type EmberSound = { play(e: GameEvent): void; setMuted(m: boolean): void; muted: boolean }

export function createEmberSound(): EmberSound {
  const s: EmberSound = {
    muted: false,
    play() {},
    setMuted(m) {
      s.muted = m
    },
  }
  return s
}
