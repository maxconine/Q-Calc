import { describe, expect, it } from 'vitest'
import { compileLevel } from '../emberEngine'
import { STEP_HZ, type LevelDef } from '../emberTypes'
import { DEPTHS } from './depths'
import { describeLevel, record, runWalkthrough, StageRun, type LevelChecks, type Walkthrough } from './testkit'

// every level of the pack with a walkthrough that finishes it in the real engine without a death, one recording
// per stage: each line drives the engine (walk to a column, jump, press a lamp) and the bare inputs it took are
// what describeLevel replays. EMBER_TRACE=1 prints each level's time and where a recording stopped

const E = 'ember'
const F = 'frost'

// the down key at a lamp, and what the lights must be afterwards
const lamp = (r: StageRun, p: 'ember' | 'frost', on: boolean) => r.press(p).expect((s) => s.lit === on, `the lights ${on ? 'on' : 'off'} after ${p}'s press`)

const RUNS: Record<string, { stages: Array<(r: StageRun) => void>; checks: LevelChecks }> = {
  'lights-out': {
    // skipReach: the reachability check is left off for this level (bridges, steps and a lift that follow the lights)
    checks: { minSecs: 50, skipReach: true },
    stages: [
      (r) => {
        // the bottom floor. lights out, and both over the bridge of dark
        lamp(r.walkToStop(E, 7), E, false)
        r.walkToStop(F, 12).jump(F, 0).walkToStop(F, 18).walkToStop(E, 17)
        // the bridge of light and the gate for the dark: frost first, then he works the far lamp for ember
        lamp(r, F, true)
        r.walkToStop(F, 26).walkToStop(E, 18)
        lamp(r, E, false).waitGate(0)
        lamp(r.walkToStop(F, 30), F, true).waitGate(0, false)
        r.walkToStop(E, 26)
        lamp(r, F, false).waitGate(0)
        r.walkToStop(E, 31).mark('through the gate')
        // ember wades the lava and lights frost's bridge; frost wades the water and darkens ember's
        r.walkToStop(E, 37).jump(E, 1, { toX: 38.5 })
        lamp(r.walkToStop(E, 39), E, true)
        r.walkToStop(F, 40).walkToStop(F, 45).jump(F, 1, { toX: 46.5 })
        lamp(r.walkToStop(F, 47), F, false)
        r.walkToStop(E, 51).walkToStop(F, 51).checkpoint(0)
        // the lift goes up in the dark and comes down in the light. ember at the lamp sends frost up to his gem and back
        lamp(r.walkToStop(E, 54), E, true).waitMover(0, 0)
        r.walkToStop(F, 56)
        lamp(r, E, false).waitMover(0, 1).walkToStop(F, 57).expect((s) => s.players[1].gems === 2, "frost's gem at the top of the lift")
        lamp(r, E, true).waitMover(0, 0)
        r.walkToStop(F, 55)
        // in the light: over the lift and up the steps of light to ember's gem, and back onto the lift
        r.walkToStop(E, 60).jump(E, 1, { toX: 61 }).jump(E, -1, { toX: 59 }).expect((s) => s.players[0].gems === 1, "ember's gem on the steps")
        r.walkToStop(E, 56).expectAt(E, 56, 14)
        // frost at the lamp: the dark takes ember to the top floor, the light brings the lift back for his climb
        lamp(r.walkToStop(F, 54), F, false).waitMover(0, 1)
        r.walkToStop(E, 55).expectAt(E, 55, 4)
        lamp(r, F, true).waitMover(0, 0)
        r.walkToStop(F, 60).jump(F, 1, { toX: 61 }).jump(F, -1, { toX: 58.6 }).jump(F, -1, { toX: 55 }).expectAt(F, 55, 9).mark('both in their lanes')
        // the lanes
        lamp(r.walkToStop(F, 39), F, false)
        r.walkToStop(E, 39).walkToStop(F, 31)
        lamp(r, E, true)
        r.walkToStop(F, 15).walkToStop(E, 23)
        lamp(r, F, false)
        // the trapdoors: ember drops through hers in the dark onto frost's, and his lamp opens that one under her
        r.walkToStop(F, 5).walkTo(E, 2, { slack: 6 }).land(E).settle(E).expectAt(E, 2, 9)
        lamp(r, F, true).land(E)
        r.walkTo(F, 2, { slack: 6 }).land(F).intoDoor(F, 1)
        r.oneUntil(E, 1, (p) => p.inDoor, {}, 'to walk into the door')
      },
    ],
  },
}

function walkFor(def: LevelDef): { walk?: Walkthrough; error?: unknown } {
  try {
    return { walk: record(def, RUNS[def.id]?.stages ?? []) }
  } catch (error) {
    if (process.env.EMBER_TRACE) console.log(String(error).split('\n').slice(0, 40).join('\n'))
    return { error }
  }
}

describe('depths', () => {
  for (const def of DEPTHS) {
    const { walk, error } = walkFor(def)
    if (error) {
      it(`${def.id}: the walkthrough records`, () => {
        throw error
      })
    }
    describeLevel(def, walk, RUNS[def.id]?.checks)
    if (walk && process.env.EMBER_TRACE) console.log(`${def.id}: ${(runWalkthrough(compileLevel(def), walk).tick / STEP_HZ).toFixed(1)} s`)
  }
  it('has one level for now', () => expect(DEPTHS.map((d) => d.id)).toEqual(['lights-out']))
})
