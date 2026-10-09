import { C4_HINT, isConnect4Command } from './connect4'
import { CRACK_HINT, isCrackCommand } from './crack'
import { EMBER_HINT, isEmberCommand } from './ember'
import { isPongCommand, PONG_HINT } from './pong'

// the two-player games the bar opens, by what's typed in it

export type GameKind = 'pong' | 'connect4' | 'ember' | 'crack'

export function gameCommand(text: string): GameKind | null {
  if (isPongCommand(text)) return 'pong'
  if (isConnect4Command(text)) return 'connect4'
  if (isEmberCommand(text)) return 'ember'
  if (isCrackCommand(text)) return 'crack'
  return null
}

// what `games` lists, in order; a new game adds a row here
export const GAMES: ReadonlyArray<{ kind: GameKind; name: string; blurb: string }> = [
  { kind: 'pong', name: 'pong', blurb: 'first to 7 · two players, anywhere' },
  { kind: 'connect4', name: 'connect 4', blurb: 'four in a row · two players, anywhere' },
  { kind: 'ember', name: 'ember & frost', blurb: 'co-op puzzle platformer · one keyboard or online' },
  { kind: 'crack', name: 'crack', blurb: 'crack the safe · tap when the dial hits the dot' },
]

export const GAMES_HINT = 'type a game’s name, or click one'

// `games` lists them above the bar, the way `?` lists the shortcuts
export function isGamesCommand(text: string): boolean {
  const t = text.trim().toLowerCase()
  return t === 'games' || t === 'game'
}

// the hint under the bar; linked is false where there's no peer link to play over
export function gameHint(kind: GameKind, linked: boolean): string {
  // ember & frost plays on one keyboard, so it needs no link
  if (kind === 'ember') return EMBER_HINT
  // crack is one player
  if (kind === 'crack') return CRACK_HINT
  const name = kind === 'pong' ? 'pong' : 'connect 4'
  if (!linked) return `${name} needs Q Calc for Mac`
  return kind === 'pong' ? PONG_HINT : C4_HINT
}
