import { describe, it } from 'vitest'
import { compileLevel } from '../emberEngine'
import type { LevelDef } from '../emberTypes'
import { BASICS } from './basics'
import { describeLevel, LEFT, record, timingSummary, type LevelChecks, type StageRun, type Walkthrough } from './testkit'

// every level of the pack with a walkthrough that finishes it in the real engine without a death: one script
// per stage. the new levels' walkthroughs are written with the testkit's recorder: each line drives the engine
// (walk to a column, jump, wait for a gate) and the bare inputs it took are what describeLevel replays.
// EMBER_TRACE=1 prints each level's timings
const one = (script: Walkthrough[number]): Walkthrough => [script]

// TODO(ember-2:levels-a): version 1's hand-made scripts, [ticks, fire bits, frost bits] with 1 left, 2 right,
// 4 jump, 8 down, for the old levels still in the pack
const SCRIPTS: Record<string, Walkthrough[number]> = {
  'first-light': [
    // ember: over the step, through the lava, over the goo, up the step and shelf, down to the red door
    [8, 2, 0], [20, 6, 0], [20, 2, 0], [40, 2, 0], [2, 2, 0], [25, 6, 0], [10, 2, 0], [5, 0, 0], [12, 6, 0], [10, 0, 0],
    [25, 6, 0], [10, 2, 0], [30, 2, 0], [14, 2, 0], [30, 0, 0],
    // frost: onto the step, the shelf over the lava, over the goo and the steps, to the blue door
    [3, 0, 2], [18, 0, 6], [8, 0, 0], [25, 0, 6], [30, 0, 2], [10, 0, 2], [25, 0, 6], [10, 0, 2], [5, 0, 0], [12, 0, 6],
    [10, 0, 0], [40, 0, 2], [30, 0, 2],
  ],
  'two-lanes': [
    // ember: over the goo, up the stairs, the lava lane, down to the red door
    [6, 2, 0], [20, 6, 0], [10, 2, 0], [20, 6, 0], [10, 2, 0], [20, 6, 0], [10, 2, 0], [20, 6, 0], [60, 2, 0], [40, 2, 0],
    [10, 2, 0], [30, 0, 0],
    // frost: over the goo, up two stairs, the ledge, the lip, the water lane, down to the blue door
    [2, 0, 2], [20, 0, 6], [10, 0, 2], [20, 0, 6], [10, 0, 2], [20, 0, 6], [6, 0, 2], [10, 0, 0], [4, 0, 4], [14, 0, 5],
    [15, 0, 0], [22, 0, 6], [15, 0, 2], [60, 0, 2], [30, 0, 2], [20, 0, 2], [7, 0, 2], [20, 0, 0],
  ],
  'hold-the-door': [
    // ember onto the low plate; frost through the low gate, over the water onto the high plate
    [26, 2, 0], [20, 0, 0], [60, 0, 2], [60, 0, 2], [30, 0, 2], [12, 0, 2], [20, 0, 0],
    // ember up the ledge and shelf, through the high gate and the lava to the red door
    [20, 1, 0], [18, 5, 0], [10, 0, 0], [20, 6, 0], [10, 2, 0], [60, 2, 0], [60, 2, 0], [20, 2, 0], [44, 2, 0], [20, 0, 0],
    // frost off the plate to the blue door
    [33, 0, 2], [20, 0, 0],
  ],
  // the rest are terser: each was found by stepping the real engine and checking positions
  'lockstep': [
    [28, 2, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [30, 0, 0], [80, 0, 2], [10, 0, 0], [12, 0, 2], [10, 0, 0],
    [2, 0, 8], [2, 0, 0], [30, 0, 0], [66, 2, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [30, 0, 0], [30, 0, 2],
    [20, 0, 6], [12, 0, 2], [10, 0, 0], [2, 0, 8], [2, 0, 0], [30, 0, 0], [30, 2, 0], [20, 6, 0], [12, 2, 0],
    [10, 0, 0], [2, 8, 0], [2, 0, 0], [30, 0, 0], [32, 0, 2], [10, 0, 0], [2, 0, 8], [2, 0, 0], [30, 0, 0],
    [57, 2, 0], [20, 0, 0], [25, 0, 2], [20, 0, 0],
  ],
  'the-lifts': [
    [40, 0, 1], [15, 0, 5], [20, 0, 1], [10, 0, 0], [124, 2, 0], [10, 0, 0], [220, 0, 0], [60, 0, 2], [10, 0, 0],
    [22, 0, 2], [185, 0, 0], [16, 0, 2], [100, 0, 0], [30, 0, 2], [14, 2, 2], [12, 6, 0], [20, 2, 0], [10, 0, 0],
    [2, 0, 8], [2, 0, 0], [250, 0, 0], [20, 5, 0], [10, 1, 0], [20, 0, 0], [26, 1, 2], [17, 1, 0], [20, 0, 0],
  ],
  'crates': [
    [16, 2, 0], [20, 0, 0], [6, 1, 0], [2, 0, 2], [16, 0, 6], [50, 0, 2], [10, 0, 2], [18, 0, 6], [10, 0, 2],
    [5, 0, 0], [18, 0, 6], [8, 0, 0], [18, 0, 6], [8, 0, 0], [18, 0, 6], [8, 0, 0], [18, 0, 6], [8, 0, 0],
    [18, 0, 6], [8, 0, 0], [20, 0, 0], [18, 0, 6], [8, 0, 0], [20, 0, 0], [22, 0, 5], [10, 0, 1], [10, 0, 0],
    [80, 0, 1], [40, 0, 0], [20, 0, 5], [60, 0, 1], [21, 0, 1], [40, 0, 0], [20, 6, 0], [60, 2, 0], [18, 6, 0],
    [8, 0, 0], [18, 6, 0], [8, 0, 0], [18, 6, 0], [8, 0, 0], [18, 6, 0], [8, 0, 0], [18, 6, 0], [8, 0, 0],
    [20, 0, 0], [18, 6, 0], [8, 0, 0], [20, 2, 0], [20, 0, 0], [60, 0, 2], [20, 0, 6], [60, 0, 2], [30, 0, 0],
    [10, 0, 2], [20, 0, 0], [18, 0, 6], [8, 0, 0], [50, 0, 2], [30, 0, 0],
  ],
  'thaw': [
    [98, 2, 0], [60, 1, 2], [30, 1, 2], [40, 0, 2], [15, 0, 0], [18, 5, 0], [10, 0, 0], [20, 6, 0], [10, 2, 0],
    [60, 2, 0], [60, 2, 0], [30, 2, 0], [20, 0, 0], [64, 2, 0], [20, 0, 0], [70, 0, 2], [20, 0, 0],
  ],
  'white-crossing': [
    [60, 2, 2], [20, 0, 0], [24, 0, 2], [22, 0, 6], [10, 0, 0], [12, 0, 2], [3, 0, 2], [10, 0, 0], [24, 2, 0],
    [22, 6, 0], [10, 0, 0], [20, 6, 0], [10, 0, 0], [70, 2, 2], [12, 2, 0], [20, 0, 0], [22, 0, 2], [20, 0, 0],
    [20, 2, 0], [20, 0, 0],
  ],
  'updraft': [
    [116, 2, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [20, 0, 0], [30, 0, 2], [24, 0, 6], [8, 0, 2], [60, 0, 0],
    [40, 0, 2], [10, 0, 0], [36, 0, 2], [10, 0, 0], [2, 0, 8], [2, 0, 0], [66, 2, 0], [70, 0, 0], [30, 2, 0],
    [20, 0, 0], [10, 0, 2], [20, 0, 0],
  ],
  'relay': [
    [30, 0, 2], [10, 0, 0], [14, 2, 0], [30, 2, 0], [22, 6, 0], [60, 2, 0], [20, 2, 0], [10, 0, 0], [20, 2, 0],
    [10, 0, 0], [22, 0, 2], [50, 0, 2], [20, 0, 6], [80, 0, 2], [20, 0, 0], [30, 0, 2], [85, 0, 2], [10, 0, 0],
    [50, 2, 0], [20, 0, 0], [103, 1, 0], [20, 0, 0],
  ],
  'break-the-beam': [
    [22, 1, 0], [20, 0, 0], [124, 0, 2], [20, 0, 0], [2, 0, 8], [2, 0, 0], [10, 0, 0], [12, 0, 1], [20, 0, 0],
    [18, 5, 0], [10, 0, 0], [20, 6, 0], [10, 2, 0], [60, 2, 0], [60, 2, 0], [60, 2, 0], [20, 0, 0],
  ],
  'two-mirrors': [
    [4, 1, 0], [20, 5, 0], [10, 0, 0], [20, 5, 0], [10, 0, 0], [20, 5, 0], [10, 0, 0], [12, 1, 0], [22, 5, 0],
    [10, 1, 0], [20, 1, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [20, 5, 0], [30, 1, 0], [12, 1, 0], [30, 0, 0],
    [4, 0, 1], [22, 0, 5], [20, 0, 1], [50, 0, 1], [30, 0, 1], [10, 0, 0], [20, 0, 5], [20, 0, 0], [4, 0, 1],
    [10, 0, 0], [2, 0, 8], [2, 0, 0], [20, 0, 0], [40, 1, 0], [78, 0, 1], [20, 0, 0],
  ],
  'cold-storage': [
    [70, 1, 0], [10, 0, 0], [22, 2, 0], [18, 6, 0], [20, 0, 0], [36, 0, 2], [24, 0, 6], [20, 0, 2], [10, 0, 0],
    [220, 0, 0], [91, 2, 0], [40, 0, 0], [20, 6, 0], [20, 2, 0], [10, 0, 0], [30, 0, 2], [18, 0, 6], [8, 0, 0],
    [18, 0, 6], [8, 0, 0], [14, 0, 6], [10, 0, 0], [58, 2, 2], [19, 2, 0], [20, 0, 0],
  ],
  'lantern-works': [
    [22, 0, 1], [10, 0, 0], [2, 0, 8], [2, 0, 0], [20, 2, 0], [20, 6, 0], [24, 2, 0], [2, 2, 0], [40, 0, 0],
    [12, 0, 0], [2, 8, 0], [2, 0, 0], [27, 0, 0], [20, 1, 0], [20, 0, 0], [4, 0, 1], [22, 0, 5], [10, 0, 1],
    [10, 0, 0], [16, 0, 1], [30, 0, 0], [32, 0, 2], [20, 0, 0], [70, 1, 0], [10, 0, 0], [20, 2, 0], [10, 0, 0],
    [2, 8, 0], [2, 0, 0], [10, 0, 0], [60, 0, 2], [10, 0, 0], [70, 2, 0], [30, 0, 0], [44, 2, 1], [68, 0, 1],
    [20, 0, 0],
  ],
  'last-light': [
    [21, 1, 0], [20, 0, 0], [2, 8, 0], [2, 0, 0], [16, 2, 0], [20, 0, 0], [106, 0, 2], [15, 0, 0], [86, 2, 0],
    [40, 0, 0], [38, 0, 2], [50, 0, 0], [24, 0, 2], [10, 0, 0], [22, 6, 0], [10, 0, 0], [44, 2, 2], [30, 2, 0],
    [20, 0, 0],
  ],
}


const E = 'ember'
const F = 'frost'

// the last one into their door: the stage ends the tick they arrive, mid-stride
const last = (r: StageRun, p: 'ember' | 'frost', bits: number) => r.oneUntil(p, bits, (pl) => pl.inDoor, {}, 'to walk into the door')

// the recorded walkthroughs: per level, one function per stage
const RUNS: Record<string, { stages: Array<(r: StageRun) => void>; checks: LevelChecks }> = {
  'dead-weight': {
    checks: { minSecs: 45, skipReach: true },
    stages: [
      (r) => {
        // act 1. ember holds the plate, frost goes through and parks the crate on the second plate
        r.walkToStop(E, 4.5).waitGate(0)
        r.walkTo(F, 11).settle(F)
        r.walkTo(E, 9)
        // the crate under the deck; frost up it and through the deck, and along for the gem
        r.walkTo(F, 16.3).settle(F)
        r.jump(F, 1, { toX: 17.5 }).jump(F, 0).expect(() => r.at(F).y === 11, `${F} on row 11`)
        r.jump(F, 1, { toX: 19.5 })
        // ember over the crate, the gem under the deck, then the plate
        r.walkTo(E, 14.5).jump(E, 1, { toX: 17.5 }).walkToStop(E, 23).jump(E, 0).walkToStop(E, 21.5).waitGate(1)
        r.walkToStop(F, 33.5).waitGate(2)
        r.walkTo(E, 37.5)
        r.walkTo(F, 35).land(F).walkTo(F, 38.5).bothLand().checkpoint(0).mark('act 2')
        // act 2. the goo, the pit (frost drops in for the gem), the block, the deck
        for (const p of [E, F] as const) {
          r.walkTo(p, 39.2).jump(p, 1, { toX: 42.4 })
          r.walkTo(p, 43.5).land(p).jump(p, 1, { toX: 46 })
          // a full jump from the floor goes up through the deck by itself; the block at x49-50 is the easy way
          r.walkTo(p, 47.5).jump(p, 1, { toX: 49.6 })
          if (r.at(p).y > 12) r.jump(p, 0)
          r.expect(() => r.at(p).y === 12, `${p} on row 12`)
        }
        // ember up the crate for the gem on the top deck, back down off its end, and over the crate
        r.jump(E, -1, { toX: 47.5 }).jump(E, 0).expect(() => r.at(E).y === 8, `${E} on row 8`)
        r.oneUntil(E, LEFT, (pl) => !pl.ground, {}, 'to step off the top deck').land(E).expect(() => r.at(E).y === 12, `${E} on row 12`)
        r.jump(E, 1, { toX: 49.5 })
        // frost up top; ember sends the crate into the pit
        r.jump(F, -1, { toX: 47.5 }).jump(F, 0).expect(() => r.at(F).y === 8, `${F} on row 8`)
        r.oneUntil(E, LEFT, (_, s) => s.boxes[1]!.y > 241, {}, 'the crate to fall').waitChannel('pit').waitGate(3)
        r.walkToStop(F, 59).waitGate(4)
        r.walkToStop(E, 62)
        // the rungs
        for (let i = 0; i < 3; i++) r.jump(E, 0)
        r.jump(E, -1, { toX: 61 }).expect(() => r.at(E).y === 5, `${E} on row 5`)
        r.walkToStop(F, 60)
        while (r.at(F).y > 2) r.jump(F, 0)
        r.checkpoint(1).mark('act 3')
        // act 3. the first airlock: ember's lever lets frost in, frost's lets ember in, and each lets the other out
        r.walkToStop(E, 41).press(E).waitGate(5)
        r.walkToStop(F, 35).press(F).waitGate(7)
        r.press(E).walkToStop(E, 36)
        r.press(F).waitGate(6).waitGate(8)
        // the second, the other way round
        r.walkToStop(F, 26).press(F).waitGate(9)
        r.walkToStop(E, 20).press(E).waitGate(11)
        r.press(F).walkToStop(F, 21)
        r.press(E).waitGate(10).waitGate(12)
        r.intoDoor(E, -1)
        last(r, F, LEFT)
      },
    ],
  },
}

function walkFor(def: LevelDef): { walk: Walkthrough | undefined; error: unknown } {
  const run = RUNS[def.id]
  if (!run) return { walk: SCRIPTS[def.id] && one(SCRIPTS[def.id]!), error: null }
  try {
    const walk = record(def, run.stages)
    if (process.env.EMBER_TRACE) console.log(`${def.id}\n${timingSummary(compileLevel(def), walk)}`)
    return { walk, error: null }
  } catch (error) {
    return { walk: undefined, error }
  }
}

describe('basics', () => {
  for (const def of BASICS) {
    const { walk, error } = walkFor(def)
    if (error) it(`${def.id}: the walkthrough records`, () => { throw error })
    describeLevel(def, walk, RUNS[def.id]?.checks)
  }
})
