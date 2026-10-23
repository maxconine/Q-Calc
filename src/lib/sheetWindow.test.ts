import { describe, expect, it } from 'vitest'
import { closesSheet, fitScale, sheetFromSearch } from './sheetWindow'

const keys = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }

describe('sheet windows', () => {
  it('read which sheet from the query the shell opened them on', () => {
    expect(sheetFromSearch('?kind=periodic')).toEqual({ kind: 'periodic' })
    expect(sheetFromSearch('?kind=identities&id=calculus')).toEqual({ kind: 'identities', id: 'calculus', section: 0 })
    expect(sheetFromSearch('?kind=identities&id=identities&section=4')).toEqual({ kind: 'identities', id: 'identities', section: 4 })
    expect(sheetFromSearch('?kind=identities&id=identities&section=x')).toEqual({ kind: 'identities', id: 'identities', section: 0 })
    expect(sheetFromSearch('?kind=identities')).toBeNull()
    expect(sheetFromSearch('')).toBeNull()
  })

  it('close on esc and ctrl+w only', () => {
    expect(closesSheet({ ...keys, key: 'Escape' })).toBe(true)
    expect(closesSheet({ ...keys, key: 'W', ctrlKey: true })).toBe(true)
    expect(closesSheet({ ...keys, key: 'w', ctrlKey: true, shiftKey: true })).toBe(false)
    expect(closesSheet({ ...keys, key: 'w' })).toBe(false)
  })

  it('scale the table to the window, keeping its shape', () => {
    expect(fitScale({ width: 800, height: 500 }, { width: 800, height: 500 })).toBe(1)
    expect(fitScale({ width: 800, height: 500 }, { width: 1600, height: 600 })).toBe(1.2)
    expect(fitScale({ width: 800, height: 500 }, { width: 100, height: 100 })).toBe(0.4)
    expect(fitScale({ width: 0, height: 0 }, { width: 100, height: 100 })).toBe(1)
  })
})
