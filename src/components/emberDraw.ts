import type { GameEvent, GameState, Level, PlayerIndex } from '../lib/emberTypes'

// ember & frost on a canvas: tiles, pools, mechanics, the two characters, particles and glow.
// STUB: the render agent replaces this; the exported shapes are the contract

export type DrawOpts = {
  me: PlayerIndex | null // online: outline/label this side's character; null in local play
  reduceMotion: boolean // fewer particles, no screen shake
}

export class EmberRenderer {
  constructor(canvas: HTMLCanvasElement) {
    void canvas
  }
  // match the canvas backing store to its css size and devicePixelRatio; call on mount and resize
  resize(): void {}
  // forget particles and effects (new level)
  reset(): void {}
  // re-read css colour tokens (theme change)
  refreshColors(): void {}
  // one frame. events are everything since the last frame; prev/alpha interpolate moving things
  draw(level: Level, prev: GameState | null, cur: GameState, alpha: number, events: GameEvent[], nowMs: number, opts: DrawOpts): void {
    void level
    void prev
    void cur
    void alpha
    void events
    void nowMs
    void opts
  }
}

// a small static picture of a level for the select screen
export function drawThumb(canvas: HTMLCanvasElement, level: Level): void {
  void canvas
  void level
}
