import { describe, expect, it } from 'vitest'
import { breakRun, editKind, recordEdit, redo, undo, undoStart, UNDO_RUN_MS, type EditKind, type Undo } from './undo'

const at = (value: string) => ({ value, start: value.length, end: value.length })

function typeAll(u: Undo, steps: Array<[string, EditKind, number]>): Undo {
  return steps.reduce((acc, [v, kind, t]) => recordEdit(acc, at(v), kind, t), u)
}

describe('input undo', () => {
  it('groups a typing run into one step', () => {
    const u = typeAll(undoStart(''), [
      ['1', 'insert', 100],
      ['12', 'insert', 200],
      ['12+', 'insert', 300],
    ])
    const back = undo(u)!
    expect(back.current.value).toBe('')
    expect(undo(back)).toBeNull()
  })

  it('a symbol swap mid run stays in the run', () => {
    // `sqr` then `t` turns into √
    const u = typeAll(undoStart(''), [
      ['s', 'insert', 10],
      ['sq', 'insert', 20],
      ['sqr', 'insert', 30],
      ['√', 'insert', 40],
      ['√2', 'insert', 50],
    ])
    expect(undo(u)!.current.value).toBe('')
  })

  it('a pause, a switch to deleting, or a caret move starts a new step', () => {
    let u = typeAll(undoStart(''), [
      ['1', 'insert', 0],
      ['12', 'insert', 100],
      ['123', 'insert', 100 + UNDO_RUN_MS],
    ])
    expect(undo(u)!.current.value).toBe('12')

    u = typeAll(undoStart(''), [
      ['12', 'insert', 0],
      ['1', 'delete', 50],
    ])
    expect(undo(u)!.current.value).toBe('12')

    u = recordEdit(breakRun(recordEdit(undoStart(''), at('12'), 'insert', 0)), at('123'), 'insert', 10)
    expect(undo(u)!.current.value).toBe('12')
  })

  it('pastes, inserts and clears are single steps', () => {
    const u = typeAll(undoStart(''), [
      ['5*', 'insert', 0],
      ['5*3.14', null, 10],
      ['', null, 20],
    ])
    const once = undo(u)!
    expect(once.current.value).toBe('5*3.14')
    expect(undo(once)!.current.value).toBe('5*')
  })

  it('redo walks forward and a new edit drops it', () => {
    const u = typeAll(undoStart(''), [
      ['2', 'insert', 0],
      ['', null, 10],
    ])
    const back = undo(u)!
    expect(redo(back)!.current.value).toBe('')
    const edited = recordEdit(back, at('2+'), 'insert', 20)
    expect(redo(edited)).toBeNull()
    expect(redo(undoStart('x'))).toBeNull()
  })

  it('keeps the caret each step had', () => {
    const u = recordEdit(recordEdit(undoStart(''), { value: '1+2', start: 1, end: 1 }, null, 0), at(''), null, 5)
    expect(undo(u)!.current).toEqual({ value: '1+2', start: 1, end: 1 })
  })

  it('ignores a change that leaves the text alone', () => {
    const u = undoStart('7')
    expect(recordEdit(u, at('7'), 'insert', 0)).toBe(u)
  })

  it('reads the edit kind off the input event', () => {
    expect(editKind('insertText')).toBe('insert')
    expect(editKind('deleteContentBackward')).toBe('delete')
    expect(editKind('insertFromPaste')).toBeNull()
    expect(editKind('deleteHardLineBackward')).toBeNull()
    expect(editKind(undefined)).toBeNull()
  })
})
