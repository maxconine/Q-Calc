import { describe, expect, it } from 'vitest'
import { GAMES, gameCommand, isGamesCommand } from './games'

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
