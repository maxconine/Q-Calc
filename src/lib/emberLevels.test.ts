import { describe, expect, it } from 'vitest'
import { LEVELS } from './emberLevels'
import { reachability } from './emberSolver'
import { compileLevel } from './emberEngine'
import { checkDeterminism, diagnose, formatWalkthrough, record, runWalkthrough, StageRun, timingSummary, traceStage, viewState } from './emberLevels/testkit'
import type { StageDef, ThingDef } from './emberTypes'

// the set as a whole, and the solver. each level's own checks and walkthrough live in its pack's test file
// (emberLevels/<pack>.test.ts), through emberLevels/testkit.ts
describe('ember & frost levels', () => {
  it('has levels, the tutorial first', () => {
    // seven for now: the five basics, lights out and head over heels
    expect(LEVELS.length).toBe(7)
    expect(LEVELS[0]!.id).toBe('first-light')
  })

  it('has unique kebab-case ids and unique names', () => {
    const ids = LEVELS.map((l) => l.id)
    const names = LEVELS.map((l) => l.name)
    expect(new Set(ids).size).toBe(ids.length)
    expect(new Set(names).size).toBe(names.length)
    for (const id of ids) expect(id, id).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
  })
})

// a small map for solver tests: rows padded into a closed box
function tiny(rows: string[], things: ThingDef[] = []): StageDef {
  const w = Math.max(...rows.map((r) => r.length)) + 2
  const map = ['#'.repeat(w), ...rows.map((r) => '#' + r.padEnd(w - 2, '.') + '#'), '#'.repeat(w)]
  return { map, things }
}

describe('ember solver', () => {
  it('climbs a 3-tile wall but not a 4-tile one', () => {
    const three = tiny(['.....', '.....', '..#..', '..#..', '12#RB', '#####'])
    const r3 = reachability(three)
    expect(r3.fireDoor && r3.frostDoor).toBe(true)
    const four = tiny(['.....', '.....', '..#..', '..#..', '..#..', '12#RB', '#####'])
    const r4 = reachability(four)
    expect(r4.fireDoor || r4.frostDoor).toBe(false)
  })

  it('jumps a 3-tile gap but not a 5-tile one', () => {
    const gap = (n: number) => tiny(['.'.repeat(n + 4), '.'.repeat(n + 4), '.'.repeat(n + 4), '12' + '.'.repeat(n) + 'RB', '##' + 'G'.repeat(n) + '##'])
    const r3 = reachability(gap(3))
    expect(r3.fireDoor && r3.frostDoor).toBe(true)
    const r5 = reachability(gap(5))
    expect(r5.fireDoor || r5.frostDoor).toBe(false)
  })

  it('lets fire wade lava and frost wade water, but not the other way', () => {
    // a 2-tile ceiling over a wide pool: no jumping it
    const lava = tiny(['##########', '..........', '12......RB', '##LLLLLL##'])
    const rl = reachability(lava)
    expect(rl.fireDoor).toBe(true)
    expect(rl.frostDoor).toBe(false)
    const water = tiny(['##########', '..........', '12......RB', '##WWWWWW##'])
    const rw = reachability(water)
    expect(rw.fireDoor).toBe(false)
    expect(rw.frostDoor).toBe(true)
  })

  it('melts ice only where fire can reach it', () => {
    // frost is shut in behind ice; fire drops down a hole and melts it from the other side
    const lock = tiny(['.........', '1.R......', '######.##', '.........', '2...B....'], [{ k: 'ice', x: 4, y: 4, w: 1, h: 2 }])
    const r = reachability(lock)
    expect(r.melted).toBe(1)
    expect(r.frostDoor).toBe(true)
    const walled = tiny(['.........', '1.R......', '#########', '.........', '2...B....'], [{ k: 'ice', x: 4, y: 4, w: 1, h: 2 }])
    const rw = reachability(walled)
    expect(rw.melted).toBe(0)
    expect(rw.frostDoor).toBe(false)
  })

  it('freezes thin water only where frost can stand near it', () => {
    const near = tiny(['##########', '..........', '12......RB', '##wwwwww##'])
    const rn = reachability(near)
    expect(rn.frozen).toBe(6)
    expect(rn.fireDoor).toBe(true)
    // frost is boxed in far away, so the water stays liquid and fire can't get over it
    const far = tiny(['.#.........', '2#.........', '##.........', '...........', '.1.......RB', '###wwwwww##'])
    const rf = reachability(far)
    expect(rf.frozen).toBe(0)
    expect(rf.fireDoor).toBe(false)
  })
})

// the walkthrough tools in emberLevels/testkit.ts, on the tutorial
describe('ember testkit', () => {
  const level = compileLevel(LEVELS[0]!)

  it('records a script by driving the engine, and the bare script replays to the same state', () => {
    const run = new StageRun(level)
    run.walkTo('ember', run.at('ember').x + 2).jump('ember', 1).settle('ember').mark('hopped')
    expect(run.player('ember').ground).toBe(true)
    expect(run.script.length).toBeGreaterThan(1)
    const again = runWalkthrough(level, [run.script])
    expect(again.players[0].x).toBe(run.state.players[0].x)
    expect(again.tick).toBe(run.tick)
    expect(formatWalkthrough([run.script])).toContain('[')
    expect(run.timings()).toContain('hopped')
  })

  it('says where and why a run stopped', () => {
    const run = new StageRun(level)
    expect(() => run.walkTo('ember', 0)).toThrow(/ember is stuck walking left[\s\S]*tick \d+[\s\S]*ember tile/)
    expect(() => run.press('frost')).toThrow(/nothing answered/)
    const d = diagnose(level, [[[30, 0, 0]]])
    expect(d.won).toBe(false)
    expect(d.problem).toMatch(/script ran out[\s\S]*NOT in the door/)
    expect(viewState(level, d.state)).toMatch(/E/)
    expect(traceStage(level, 0, [[10, 2, 0]])).toMatch(/ember \d+,\d+/)
    expect(timingSummary(level, [[[30, 0, 0]]])).toMatch(/NOT finished/)
    expect(() => record(level, [(r) => r.wait(5)])).toThrow(/hasn't/)
  })

  it('runs the same script to the same hash twice', () => {
    const r = checkDeterminism(level, [[[40, 2, 1], [30, 6, 5], [60, 0, 0]]])
    expect(r.same).toBe(true)
    expect(r.hash).toMatch(/^[0-9a-f]{8}$/)
  })
})
