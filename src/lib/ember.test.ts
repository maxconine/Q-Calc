import { describe, expect, it } from 'vitest'
import { isEmberCommand } from './ember'

describe('isEmberCommand', () => {
  it('opens on the names', () => {
    const yes = [
      'ember',
      'Ember',
      ' ember ',
      'ember & frost',
      'ember&frost',
      'ember and frost',
      'ember  +  frost',
      'frost & ember',
      'fire ice',
      'fire & ice',
      'fire&ice',
      'fire and ice',
      'FIRE AND ICE',
    ]
    for (const t of yes) expect(isEmberCommand(t), t).toBe(true)
  })

  it('leaves calculator input alone', () => {
    const no = ['', 'e', 'em', 'fire', 'ice', 'frost', '2+2', 'ember2', 'ember 2', 'ember*frost', 'fireice', 'embers', 'ember &', 'fire or ice', 'ember frost', 'emberandfrost', '2 ember']
    for (const t of no) expect(isEmberCommand(t), t).toBe(false)
  })
})
