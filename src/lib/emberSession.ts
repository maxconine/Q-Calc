import type { GameEvent, GameState, Level, PlayerIndex } from './emberTypes'

// drives ember & frost: which screen, which level, the fixed-step clock, and (online) the link.
// the panel feeds it keys and frame times and draws what tick() returns. no DOM here
// STUB: the session agent replaces this; the exported shapes are the contract

export type EmberScreen = 'wait' | 'select' | 'play'

export type EmberView = {
  mode: 'local' | 'online'
  screen: EmberScreen
  level: number // index into LEVELS: the one being played, or the one highlighted on select
  compiled: Level | null // compiled LEVELS[level] while playing
  state: GameState | null // latest state to draw (online guest: predicted/interpolated)
  prev: GameState | null // the state one step before `state`, for interpolation
  alpha: number // 0..1, how far between prev and state this frame sits
  events: GameEvent[] // everything that happened since the last tick() call
  me: PlayerIndex | null // online: the character this side plays; local: null (both)
  unlocked: number // levels 0..unlocked-1 can be picked
  // online: the line the lobby shows once the link is over
  ended: string | null
  // online: something the other side asked for, shown small (e.g. 'frost wants the next level')
  note: string | null
}

export interface EmberGame {
  tick(now: number): EmberView
  // held keys. local: per player; online: the player argument is ignored and this side's character is used
  keys(player: PlayerIndex, bits: number): void
  // drop all held keys (window blur)
  release(): void
  // select screen: move the highlight / start the highlighted level
  pick(index: number): void
  play(): void
  // in play: restart the attempt; after a win: the next level; back to select
  retry(): void
  next(): void
  menu(): void
  // online only: a message from the link
  receive(text: string, now: number): void
  // tell the other side we're leaving (online), stop
  leave(): void
}

export function createLocalGame(now: number, unlocked: number): EmberGame {
  void now
  void unlocked
  throw new Error('todo')
}

// role host plays fire (0), guest plays frost (1). unlocked is this side's own; online uses the max of both
export function createOnlineGame(role: 'host' | 'guest', send: (text: string) => void, now: number, unlocked: number): EmberGame {
  void role
  void send
  void now
  void unlocked
  throw new Error('todo')
}
