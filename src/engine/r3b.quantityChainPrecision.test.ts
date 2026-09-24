import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

describe('a quantity chained through a variable keeps full precision', () => {
  it('cubing a stored quantity should stay exact', () => {
    const rows = evaluateSheet(['y = 2 kg', 'y^3'])
    expect(rows[1]!.display).toBe('8 kg^3')
  })
})
