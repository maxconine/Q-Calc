import { describe, it } from 'vitest'
import { compileLevel } from '../emberEngine'
import type { LevelDef } from '../emberTypes'
import { SUMMIT } from './summit'
import { describeLevel, LEFT, record, StageRun, timingSummary, type LevelChecks, type Walkthrough } from './testkit'

// every level of the pack with a walkthrough that finishes it in the real engine without a death: one function
// per stage, written with the testkit's recorder (each line drives the engine, and the bare inputs it took are
// what describeLevel replays). EMBER_TRACE=1 prints each level's timings
const E = 'ember'
const F = 'frost'

// the last one into their door: the stage ends the tick they arrive, mid-stride
const last = (r: StageRun, p: 'ember' | 'frost', bits: number) => r.oneUntil(p, bits, (pl) => pl.inDoor, {}, 'to walk into the door')

// walks the ceiling to the middle of a two-wide hole (tileX is its left column) and rises through it
const rise = (r: StageRun, p: 'ember' | 'frost', tileX: number) => r.walkTo(p, tileX + 0.5).land(p)

const RUNS: Record<string, { stages: Array<(r: StageRun) => void>; checks: LevelChecks }> = {
  'head-over-heels': {
    // skipReach: the solver's reachability check doesn't follow gravity switches
    checks: { minSecs: 70, skipReach: true },
    stages: [
      (r) => {
        // hall 1: up at x7, the ceiling over the wall and the goo, down at x22, under the hanging wall
        for (const p of [E, F] as const) r.walkToStop(p, 7).flip(p).walkToStop(p, 22).flip(p)
        r.walkToStop(E, 33).jump(E).walkToStop(E, 38)
        r.walkToStop(F, 30).jump(F).walkToStop(F, 37).checkpoint(0)
        // up at x44, over the wall, and up through the hole
        for (const p of [E, F] as const) rise(r.walkToStop(p, 44).flip(p), p, 60)
        r.checkpoint(1)

        // hall 2: ember down and through the lava to the plate; frost through the ceiling gate to the ceiling plate
        r.walkToStop(E, 55).flip(E).walkToStop(E, 41.5).waitGate(0)
        r.walkToStop(F, 38.5).waitGate(1)
        r.walkToStop(E, 34)
        r.walkToStop(F, 33).flip(F)
        r.walkToStop(E, 28).walkToStop(F, 27).checkpoint(2)
        // the other way round: ember up, frost through the water to the plate, ember to the ceiling plate
        r.walkToStop(E, 26).flip(E)
        r.walkToStop(F, 12.5).waitGate(2)
        r.walkToStop(E, 10.5).waitGate(3)
        rise(r.walkToStop(F, 6).flip(F), F, 2)
        rise(r, E, 2).checkpoint(3)

        // hall 3: down onto the island for the gems, up again, down at x24
        r.walkToStop(E, 15).flip(E).walkToStop(E, 16).walkToStop(E, 15).flip(E).walkToStop(E, 24).flip(E)
        r.walkToStop(F, 15).flip(F).walkToStop(F, 14).walkToStop(F, 15).flip(F).walkToStop(F, 24).flip(F)
        r.walkToStop(E, 26).checkpoint(4)
        // over the block; frost back under it, turns over in the water tunnel and comes out onto the ceiling; ember onto the lift
        for (const p of [E, F] as const) r.walkToStop(p, 29).jump(p, 1, { toX: 30 }).jump(p, 1, { toX: 31 }).jump(p, 1, { toX: 32.5 }).walkToStop(p, 38.5)
        r.walkToStop(F, 33).flip(F).walkTo(F, 37.5).land(F)
        r.jump(E, 1, { toX: 41 }).settle(E)
        r.walkToStop(F, 43.5).waitMover(0)
        r.walkToStop(E, 59).flip(E)
        rise(r, E, 60)
        rise(r, F, 60).checkpoint(5)

        // hall 4: each down their own switch, into their own pool; frost hops the lava
        r.walkToStop(E, 47).flip(E).walkToStop(E, 39)
        r.walkToStop(F, 50).flip(F).walkToStop(F, 49).jump(F, -1, { toX: 44 }).walkToStop(F, 38).checkpoint(6)
        // the dash: frost holds the ceiling gate from the floor button; ember up, over the ceiling button
        // and through, and frost under as ember passes
        r.walkToStop(E, 37).flip(E)
        r.walkToStop(F, 32).waitGate(4)
        r.walkTo(E, 34).walkTo(E, 26, { other: LEFT }).walkToStop(F, 24)
        r.walkToStop(E, 22).flip(E)
        r.intoDoor(E, -1)
        last(r.walkTo(F, 9), F, LEFT)
      },
    ],
  },
}

function walkFor(def: LevelDef): { walk: Walkthrough | undefined; error: unknown } {
  const run = RUNS[def.id]
  if (!run) return { walk: undefined, error: new Error(`${def.id} has no walkthrough yet`) }
  try {
    const walk = record(def, run.stages)
    if (process.env.EMBER_TRACE) console.log(`${def.id}\n${timingSummary(compileLevel(def), walk)}`)
    return { walk, error: null }
  } catch (error) {
    return { walk: undefined, error }
  }
}

describe('summit', () => {
  for (const def of SUMMIT) {
    const { walk, error } = walkFor(def)
    if (error) it(`${def.id}: the walkthrough records`, () => { throw error })
    describeLevel(def, walk, RUNS[def.id]?.checks)
  }
})
