import { describe, expect, it } from 'vitest'
import { fmtPar, fmtTime, isEmberCommand, levelDifficulty, levelLength, levelScreens, MAX_DIFFICULTY } from './ember'
import { LEVELS } from './emberLevels'

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

describe('the panel’s small sums', () => {
  it('writes clocks and target times', () => {
    expect(fmtTime(0)).toBe('0:00.0')
    expect(fmtTime(7.44)).toBe('0:07.4')
    expect(fmtTime(59.96)).toBe('1:00.0')
    expect(fmtTime(130)).toBe('2:10.0')
    expect(fmtTime(-3)).toBe('0:00.0')
    expect(fmtTime(Number.NaN)).toBe('0:00.0')
    expect(fmtPar(45)).toBe('0:45')
    expect(fmtPar(130)).toBe('2:10')
    expect(fmtPar(180)).toBe('3:00')
  })

  it('marks difficulty by place in the list, 1 to 5, never going down', () => {
    expect(levelDifficulty(0, 14)).toBe(1)
    expect(levelDifficulty(13, 14)).toBe(MAX_DIFFICULTY)
    expect(levelDifficulty(0, 1)).toBe(1)
    expect(levelDifficulty(40, 14)).toBe(MAX_DIFFICULTY)
    const marks = Array.from({ length: 14 }, (_, i) => levelDifficulty(i, 14))
    for (let i = 1; i < marks.length; i++) expect(marks[i]!).toBeGreaterThanOrEqual(marks[i - 1]!)
    expect(new Set(marks).size).toBe(MAX_DIFFICULTY)
  })

  it('measures a level in stages and screens', () => {
    const stage = (cols: number, rows: number) => ({ map: Array.from({ length: rows }, () => '#'.repeat(cols)) })
    expect(levelScreens({ stages: [stage(32, 18)] })).toBe(1)
    expect(levelScreens({ stages: [stage(64, 36), stage(96, 18)] })).toBe(7)
    expect(levelScreens({ stages: [stage(48, 18)] })).toBe(1.5)
    expect(levelLength({ stages: [stage(32, 18)] })).toBe('1 stage · 1 screen')
    expect(levelLength({ stages: [stage(64, 18), stage(64, 36), stage(32, 72)] })).toBe('3 stages · 10 screens')
    for (const def of LEVELS) expect(levelScreens(def), def.id).toBeGreaterThanOrEqual(1)
  })
})
