import { describe, expect, it } from 'vitest'
import { EMBER_HINT } from './ember'
import { GAMES, gameCommand, gameHint, isGamesCommand } from './games'

describe('isGamesCommand', () => {
  it('takes games and game, in any case and spacing', () => {
    for (const t of ['games', 'Games', ' GAMES ', 'game']) expect(isGamesCommand(t)).toBe(true)
    for (const t of ['gam', 'games 2', 'pong', '', 'gamess']) expect(isGamesCommand(t)).toBe(false)
  })

  it('lists every game by a name that opens it', () => {
    expect(GAMES.length).toBeGreaterThan(1)
    for (const g of GAMES) expect(gameCommand(g.name)).toBe(g.kind)
  })
})

describe('gameHint', () => {
  it('ember & frost plays on one keyboard, so it never asks for the mac link', () => {
    expect(gameHint('ember', false)).toBe(EMBER_HINT)
    expect(gameHint('pong', false)).toMatch(/needs Q Calc for Mac/)
  })
})
