import { C4_HINT, isConnect4Command } from './connect4'
import { CRACK_HINT, isCrackCommand } from './crack'
import { EMBER_HINT, isEmberCommand } from './ember'
import { EQUAL_HINT, isEqualCommand } from './equal'
import { isNumbersCommand, NUMBERS_HINT } from './numbers'
import { isPongCommand, PONG_HINT } from './pong'
import { isTrailsCommand, TRAILS_HINT } from './trails'

// the games the bar opens, by what's typed in it. a new game adds its kind, a row in GAMES and a panel in QuickCalc

export type GameKind = 'pong' | 'connect4' | 'ember' | 'crack' | 'equal' | 'numbers' | 'trails'

type Game = {
  kind: GameKind
  // what `games` lists it as
  name: string
  blurb: string
  // whether a typed line opens it
  is: (text: string) => boolean
  // the line under the bar while its name is typed
  hint: string
  // played only over the peer link; the rest play on this computer alone
  needsLink: boolean
}

// what `games` lists, in order
export const GAMES: readonly Game[] = [
  { kind: 'pong', name: 'pong', blurb: 'first to 7 · two players, anywhere', is: isPongCommand, hint: PONG_HINT, needsLink: true },
  { kind: 'connect4', name: 'connect 4', blurb: 'four in a row · two players, anywhere', is: isConnect4Command, hint: C4_HINT, needsLink: true },
  { kind: 'ember', name: 'ember & frost', blurb: 'co-op puzzle platformer · one keyboard or online', is: isEmberCommand, hint: EMBER_HINT, needsLink: false },
  { kind: 'crack', name: 'crack', blurb: 'crack the safe · tap when the dial hits the dot', is: isCrackCommand, hint: CRACK_HINT, needsLink: false },
  { kind: 'equal', name: 'equal', blurb: 'guess the hidden equation · daily or practice', is: isEqualCommand, hint: EQUAL_HINT, needsLink: false },
  { kind: 'numbers', name: '24 / countdown', blurb: 'make the number · two modes against the clock', is: isNumbersCommand, hint: NUMBERS_HINT, needsLink: false },
  { kind: 'trails', name: 'trails', blurb: 'leave a trail, don’t hit one · vs the computer or a friend', is: isTrailsCommand, hint: TRAILS_HINT, needsLink: false },
]

const byKind = (kind: GameKind) => GAMES.find((g) => g.kind === kind)!

export function gameCommand(text: string): GameKind | null {
  return GAMES.find((g) => g.is(text))?.kind ?? null
}

export function gameNeedsLink(kind: GameKind): boolean {
  return byKind(kind).needsLink
}

export const GAMES_HINT = 'type a game’s name, or click one'

// `games` lists them above the bar, the way `?` lists the shortcuts
export function isGamesCommand(text: string): boolean {
  const t = text.trim().toLowerCase()
  return t === 'games' || t === 'game'
}

// the hint under the bar; linked is false where there's no peer link to play over
export function gameHint(kind: GameKind, linked: boolean): string {
  const game = byKind(kind)
  if (game.needsLink && !linked) return `${game.name} needs Q Calc for Mac`
  return game.hint
}
