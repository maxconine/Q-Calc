import { C4_HINT, isConnect4Command } from './connect4'
import { isPongCommand, PONG_HINT } from './pong'

// the two-player games the bar opens, by what's typed in it

export type GameKind = 'pong' | 'connect4'

export function gameCommand(text: string): GameKind | null {
  if (isPongCommand(text)) return 'pong'
  if (isConnect4Command(text)) return 'connect4'
  return null
}

// the hint under the bar; linked is false where there's no peer link to play over
export function gameHint(kind: GameKind, linked: boolean): string {
  const name = kind === 'pong' ? 'pong' : 'connect 4'
  if (!linked) return `${name} needs Q Calc for Mac`
  return kind === 'pong' ? PONG_HINT : C4_HINT
}
