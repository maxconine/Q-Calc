import { C4_HINT, isConnect4Command } from './connect4'
import { CRACK_HINT, isCrackCommand } from './crack'
import { EMBER_HINT, isEmberCommand } from './ember'
import { EQUAL_HINT, isEqualCommand } from './equal'
import { isNumbersCommand, NUMBERS_HINT } from './numbers'
import { isPongCommand, PONG_HINT } from './pong'
import { isTrailsCommand, TRAILS_HINT } from './trails'
import { isUltimateCommand, UT_HINT } from './ultimate'

// the games the bar opens, by what's typed in it. a new game adds its kind, a row in GAMES and a panel in QuickCalc

export type GameKind = 'pong' | 'connect4' | 'ember' | 'crack' | 'equal' | 'numbers' | 'trails' | 'ultimate'

type Game = {
  kind: GameKind
  // what `games` lists it as
  name: string
  blurb: string
  // whether a typed line opens it
  is: (text: string) => boolean
  // the line under the bar while its name is typed
  hint: string
}

// what `games` lists, in order
export const GAMES: readonly Game[] = [
  { kind: 'pong', name: 'pong', blurb: 'first to 7 · vs the computer or anyone online', is: isPongCommand, hint: PONG_HINT },
  { kind: 'connect4', name: 'connect 4', blurb: 'four in a row · vs the computer or anyone online', is: isConnect4Command, hint: C4_HINT },
  { kind: 'ultimate', name: 'ultimate tic-tac-toe', blurb: 'nine boards in one · vs the computer or anyone online', is: isUltimateCommand, hint: UT_HINT },
  { kind: 'ember', name: 'ember & frost', blurb: 'co-op puzzle platformer · one keyboard or online', is: isEmberCommand, hint: EMBER_HINT },
  { kind: 'crack', name: 'crack', blurb: 'crack the safe · tap when the dial hits the dot', is: isCrackCommand, hint: CRACK_HINT },
  { kind: 'equal', name: 'equal', blurb: 'guess the hidden equation · daily or practice', is: isEqualCommand, hint: EQUAL_HINT },
  { kind: 'numbers', name: '24 / countdown', blurb: 'make the number · two modes against the clock', is: isNumbersCommand, hint: NUMBERS_HINT },
  { kind: 'trails', name: 'trails', blurb: 'leave a trail, don’t hit one · solo, one keyboard or online', is: isTrailsCommand, hint: TRAILS_HINT },
]

const byKind = (kind: GameKind) => GAMES.find((g) => g.kind === kind)!

export function gameCommand(text: string): GameKind | null {
  return GAMES.find((g) => g.is(text))?.kind ?? null
}

export const GAMES_HINT = '↑↓ pick · ↵ play · or type a name'

// `games` lists them above the bar, the way `?` lists the shortcuts
export function isGamesCommand(text: string): boolean {
  const t = text.trim().toLowerCase()
  return t === 'games' || t === 'game'
}

// the hint under the bar. every game plays on this computer; the ones with online play list rooms too
export function gameHint(kind: GameKind): string {
  return byKind(kind).hint
}
