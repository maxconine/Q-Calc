import { describe, expect, it } from 'vitest'
import { compileLevel, startStage, step, validateLevel } from '../emberEngine'
import type { LevelDef } from '../emberTypes'
import { BASICS } from './basics'
import { describeLevel, JUMP, LEFT, record, RIGHT, StageRun, timingSummary, type LevelChecks, type Walkthrough } from './testkit'

// every level of the pack with a walkthrough that finishes it in the real engine without a death: one script
// per stage. the new levels' walkthroughs are written with the testkit's recorder: each line drives the engine
// (walk to a column, jump, wait for a gate) and the bare inputs it took are what describeLevel replays.
// EMBER_TRACE=1 prints each level's timings
const one = (script: Walkthrough[number]): Walkthrough => [script]

// the tutorial's script is version 1's, written by hand: [ticks, fire bits, frost bits] with 1 left, 2 right, 4 jump, 8 down
const SCRIPTS: Record<string, Walkthrough[number]> = {
  'first-light': [
    // ember: over the step, through the lava, over the goo, up the step and shelf, down to the red door
    [8, 2, 0], [20, 6, 0], [20, 2, 0], [40, 2, 0], [2, 2, 0], [25, 6, 0], [10, 2, 0], [5, 0, 0], [12, 6, 0], [10, 0, 0],
    [25, 6, 0], [10, 2, 0], [30, 2, 0], [14, 2, 0], [30, 0, 0],
    // frost: onto the step, the shelf over the lava, over the goo and the steps, to the blue door
    [3, 0, 2], [18, 0, 6], [8, 0, 0], [25, 0, 6], [30, 0, 2], [10, 0, 2], [25, 0, 6], [10, 0, 2], [5, 0, 0], [12, 0, 6],
    [10, 0, 0], [40, 0, 2], [30, 0, 2],
  ],
}

const E = 'ember'
const F = 'frost'

// the last one into their door: the stage ends the tick they arrive, mid-stride
const last = (r: StageRun, p: 'ember' | 'frost', bits: number) => r.oneUntil(p, bits, (pl) => pl.inDoor, {}, 'to walk into the door')

// in the air: steers towards a tile column until the player lands. jump stays held while rising (letting go
// would cut a bounce short) and is let go on the way down (held through a landing, it hops again)
function glide(r: StageRun, p: 'ember' | 'frost', tileX: number, other = 0): StageRun {
  const i = p === 'ember' ? 0 : 1
  const target = tileX * 20 + 10
  for (let t = 0; !r.state.players[i].ground && !r.over; t++) {
    if (t > 600) r.fail(`${p} never landed`)
    const cx = r.state.players[i].x + 7
    const up = r.state.players[i].vy < 0 ? JUMP : 0
    r.one(p, (cx < target - 2 ? RIGHT : cx > target + 2 ? LEFT : 0) | up, 1, { other })
  }
  return r
}

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
  'thin-ice': {
    checks: { minSecs: 60, skipReach: true },
    stages: [
      (r) => {
        const cx = (p: 0 | 1) => (r.state.players[p].x + 7) / 20
        // the bottom. ember melts frost out; frost takes the gem in his cell
        r.oneUntil(E, LEFT, (_, s) => s.ice[0]! >= 1, {}, 'the ice to melt')
        r.walkTo(F, 1.3).walkTo(F, 7.6)
        // the thin water in the floor, frost a tile and a half ahead; then frost back for the gem above it
        r.until(() => cx(0) > 17.5, RIGHT, RIGHT)
        // the lift: ember holds the plate, frost rides up
        r.walkTo(E, 17.6).jump(E, 1, { toX: 22 }).walkToStop(E, 23.5)
        r.walkTo(F, 17.6).jump(F, 1, { toX: 19.5 }).settle(F).waitMover(0)
        // frost over the thin water and the crate, the gem, then the crate all the way to the hole
        r.walkTo(F, 10.9).jump(F, -1, { toX: 7.6 }).walkTo(F, 7)
        r.oneUntil(F, RIGHT, (_, s) => s.boxes[0]!.y > 501, { max: 1200 }, 'the crate to fall').waitGate(0)
        // down the hole after it, onto the lift's plate; ember up and along
        r.walkTo(F, 27.6).land(F).jump(F, -1, { toX: 25.5 })
        r.walkToStop(E, 21.6).waitMover(0, 0).jump(E, -1, { toX: 19.5 }).settle(E)
        r.walkToStop(F, 23.5).waitMover(0)
        r.walkTo(E, 26.2).jump(E, 1, { toX: 29.5 }).walkToStop(E, 35.5)
        // frost up the steps into the corridor
        r.walkTo(F, 26.3).jump(F, 1, { toX: 29.6 }).expect(() => r.at(F).y === 31, 'frost on the first step')
        r.jump(F, 1, { toX: 31.6 }).jump(F, 1, { toX: 34 }).walkToStop(F, 36).expect(() => r.at(F).y === 28, 'frost in the corridor')
        // the white crossing
        r.until(() => cx(0) > 45.8, RIGHT, RIGHT)
        r.walkTo(E, 51.6).land(E).walkToStop(E, 51.9).walkToStop(F, 51.3).checkpoint(1).mark('the shaft')
        // the shaft. frost over the crumbling ledge to the first thin one; ember after it has mended
        r.jump(F, 1, { toX: 53.6 }).jump(F, 1, { toX: 57 }).expect(() => r.at(F).y === 24, 'frost on the second ledge')
        r.waitMend()
        r.jump(E, 1, { toX: 53.6 }).jump(E, 1, { toX: 56.3 }).expect(() => r.at(E).y === 24, 'ember on the second ledge')
        r.jump(F, 1, { toX: 60 }).jump(E, 1, { toX: 59.4 }).expect(() => r.at(E).y === 22, 'ember on the third ledge')
        // ember first over the second crumbling ledge
        r.jump(E, 1, { toX: 61 }).jump(E, -1, { toX: 58 }).expect(() => r.at(E).y === 18, 'ember on the rock')
        r.waitMend()
        r.jump(F, 1, { toX: 61 }).jump(F, -1, { toX: 57.6 }).expect(() => r.at(F).y === 18, 'frost on the rock')
        r.jump(E, -1, { toX: 54.5 }).walkToStop(E, 52.3).jump(F, -1, { toX: 54.5 }).walkToStop(F, 52.8).checkpoint(0).mark('the top')
        // the top. frost over the crate and the ice; then ember melts it and drops into the lava with the crate
        r.walkTo(F, 47).jump(F, -1, { toX: 43 }).walkToStop(F, 40.5)
        r.oneUntil(E, LEFT, (_, s) => s.ice[1]! >= 1, {}, 'the bridge to melt').land(E).waitChannel('shelf')
        r.jump(E, -1, { toX: 40.6 }).expect(() => r.at(E).y === 16, 'ember out of the lava')
        // ember up the rungs onto the deck
        r.walkToStop(E, 40.6)
        for (let i = 0; i < 3; i++) r.jump(E, 0)
        r.expect(() => r.at(E).y === 10, 'ember at the top of the rungs').waitGate(2).walkToStop(E, 30.6)
        // frost through his gate and up through the one-way into the corridor
        r.waitGate(1).walkToStop(F, 34.6).jump(F, 0).expect(() => r.at(F).y === 13, 'frost in the top corridor')
        // the gem under the crumbling floor: frost lets it drop him, wades over, and comes back round
        r.walkToStop(F, 28.5).until((s) => s.players[1].y > 300, 0, 0, 200, 'the floor to give way').land(F)
        r.walkTo(F, 20).walkToStop(F, 34.6).jump(F, 0).expect(() => r.at(F).y === 13, 'frost back in the top corridor')
        r.waitMend().walkToStop(F, 30.3)
        // the run: frost can't stop on the crumbling floor, ember stays just behind and hops the block at x22
        r.until(() => cx(0) < 23.6, LEFT, LEFT)
        r.jump(E, -1, { other: LEFT })
        r.until(() => cx(0) < 12, LEFT, LEFT)
        r.intoDoor(E, -1)
        last(r, F, LEFT)
      },
    ],
  },
  'updraft-tower': {
    checks: { minSecs: 70, skipReach: true },
    stages: [
      (r) => {
        const cx = (p: 0 | 1) => (r.state.players[p].x + 7) / 20
        const feet = (p: 0 | 1) => (r.state.players[p].y + 22) / 20
        // the bottom. ember on the plate; frost floats out over the lava to where the fans overlap, and she steps off
        r.walkToStop(E, 5.6)
        r.walkTo(F, 13.5)
        r.walkTo(E, 7.5)
        r.oneUntil(F, RIGHT, () => cx(1) > 20.4, { max: 300 }, 'frost to float across').land(F)
        r.walkTo(E, 21).land(E)
        // frost over the lava puddle and onto the lift, then ember onto its plate
        r.walkTo(F, 22).jump(F, 1, { toX: 27 }).jump(F, 1, { toX: 29.5 }).settle(F)
        r.walkToStop(E, 24).waitMover(0)
        r.walkToStop(F, 27)
        // ember onto the lift once it's back down; frost's lever brings it up and leaves it there
        r.walkToStop(E, 27.5).waitMover(0, 0).jump(E, 1, { toX: 29.5 }).settle(E)
        r.press(F).waitMover(0)
        // the shuttle: both on at its right stop, off at its left
        r.walkToStop(E, 21.5).walkToStop(F, 21.5)
        r.waitMover(1, (m) => m.t < 0.03)
        r.jump(E, -1, { toX: 17.2 }).jump(F, -1, { toX: 17 })
        r.waitMover(1, (m) => m.t > 0.9)
        r.jump(F, -1, { toX: 6 }).jump(E, -1, { toX: 6.8 }).expect(() => r.at(F).y === 43 && r.at(E).y === 43, 'both on the left ledge')
        // the corner fan, one at a time
        r.walkToStop(E, 5.5)
        r.walkTo(F, 1.6).oneUntil(F, 0, () => feet(1) < 34.9, {}, 'frost to float up').walkTo(F, 4.5).land(F).settle(F)
        r.walkTo(E, 1.6).oneUntil(E, 0, () => feet(0) < 34.9, {}, 'ember to float up').walkTo(E, 3.5).land(E)
        r.walkToStop(E, 8).walkToStop(F, 7.4).checkpoint(1).mark('the middle')
        // the middle. the first trampoline: up for the gem, then over onto the ledge. frost comes back down to his plate
        r.walkTo(E, 9.8).bounce(E, 670).one(E, JUMP, 1).bounce(E, 670)
        glide(r, E, 14.5).expect(() => r.at(E).y === 28, 'ember on the ledge')
        r.walkTo(F, 10.9).bounce(F, 670).one(F, JUMP, 1).bounce(F, 670)
        glide(r, F, 12.4).walkTo(F, 12.3).land(F).walkToStop(F, 15).waitMover(2)
        // ember off the second trampoline onto the platform; frost lets go and it carries her over
        r.walkTo(E, 17).bounce(E, 655)
        glide(r, E, 21.6).expect(() => r.at(E).y === 21, 'ember on the platform')
        r.walkTo(F, 12.5).waitMover(2, 0).walkToStop(E, 29).waitMover(2)
        // frost the same way while ember holds the far plate, then both up the fan
        r.walkTo(F, 11).bounce(F, 640)
        glide(r, F, 14.5).expect(() => r.at(F).y === 28, 'frost on the ledge')
        r.walkTo(F, 17).bounce(F, 655)
        glide(r, F, 21.6).expect(() => r.at(F).y === 21, 'frost on the platform')
        r.walkTo(E, 30.2).oneUntil(E, 0, () => feet(0) < 16.6, {}, 'ember to float up').walkTo(E, 27.8).land(E).expect(() => r.at(E).y === 16, 'ember on the top floor')
        r.waitMover(2, 0).walkTo(F, 30.2).oneUntil(F, 0, () => feet(1) < 16.6, {}, 'frost to float up').walkTo(F, 27.8).land(F).expect(() => r.at(F).y === 16, 'frost on the top floor')
        r.walkToStop(E, 27.4).walkToStop(F, 26.8).checkpoint(0).mark('the top')
        // the top. the goo: jump, take the arrow, jump again. frost, then ember once it's back
        r.walkTo(F, 24.4).jump(F, -1, { land: false }).takeArrow(F, -1).airJump(F, -1, { toX: 15.5 })
        r.waitArrow(2).walkTo(E, 24.4).jump(E, -1, { land: false }).takeArrow(E, -1).airJump(E, -1, { toX: 15.5 })
        // frost into the water, on the fan's plate; ember over the water, up the fan, the arrow, and left onto the ledge
        r.walkToStop(F, 12)
        r.walkTo(E, 14.3).jump(E, -1, { toX: 9.6 }).walkTo(E, 7.4).takeArrow(E, 0).airJump(E, -1, { toX: 5.4 })
        r.expect(() => r.at(E).y === 9, 'ember on the high ledge').walkToStop(E, 3.5)
        // frost the same way while ember holds the plate in the lava
        r.walkTo(F, 7.4).waitArrow(1).takeArrow(F, 0).airJump(F, -1, { toX: 5.6 }).expect(() => r.at(F).y === 9, 'frost on the high ledge')
        // the last arrow, across to the doors
        r.walkTo(F, 6.2).jump(F, 1, { land: false }).takeArrow(F, 1).airJump(F, 1, { toX: 14 }).expect(() => r.at(F).y === 9, 'frost on the top ledge')
        r.walkTo(E, 6.2).waitArrow(0).jump(E, 1, { land: false }).takeArrow(E, 1).airJump(E, 1, { toX: 14 }).expect(() => r.at(E).y === 9, 'ember on the top ledge')
        r.walkTo(E, 17).walkTo(F, 13).intoDoor(E, -1)
        last(r, F, RIGHT)
      },
    ],
  },
  'lock-and-light': {
    checks: { minSecs: 90, skipReach: true },
    stages: [
      (r) => {
        // room A. ember into the lava for the key: her shadow opens the gate to the water lane
        r.walkToStop(E, 14).waitGate(6)
        // frost up the rungs, through the water, and the crate off the ledge into the upper beam
        r.walkToStop(F, 10.6).jump(F, 0).jump(F, 0).jump(F, 1, { toX: 12.6 })
        r.oneUntil(F, RIGHT, (_, s) => s.boxes[0]!.y > 201, {}, 'the crate to fall').settle(F).waitGate(0)
        r.walkTo(F, 19.6).land(F)
        r.walkTo(E, 19)
        r.expect((s) => s.keys === 1 || !!process.env.EMBER_HACK, 'the key taken')
        r.walkToStop(E, 23.4).walkToStop(F, 22.7).checkpoint(0).mark('room B')
        // room B. both up the rungs; frost through the water to the lock, and back down out of the beam
        r.walkToStop(E, 31.4).walkToStop(F, 31.7)
        while (r.at(E).y > 10) r.jump(E, 0)
        while (r.at(F).y > 10) r.jump(F, 0)
        r.oneUntil(F, LEFT, (_, s) => s.locks[0]!, {}, 'the lock to open')
        r.walkTo(F, 33.6).land(F)
        // ember into the lava, the mirror, and a step clear of the beam
        r.jump(E, 1, { toX: 35.6 }).walkToStop(E, 37).press(E).walkToStop(E, 38.4).waitGate(1)
        r.walkTo(E, 39.6).land(E).walkToStop(E, 44.6).walkToStop(F, 43.8).checkpoint(1).mark('room C')
        // room C. ember in the lava breaks the beam; frost over the lava, through his gate and up the rungs to the lane
        r.walkToStop(E, 47).waitGate(2)
        r.walkTo(F, 45.2).jump(F, 1, { toX: 50 }).walkToStop(F, 60.6)
        while (r.at(F).y > 3) r.jump(F, 0)
        // the crate through the first gate, which ember's shadow holds open
        r.waitGate(3).walkTo(F, 50.5).mark('the crate past the first gate')
        // ember back to room B, up the rungs and into the beam there: the second gate
        r.walkToStop(E, 31.4)
        while (r.at(E).y > 10) r.jump(E, 0)
        r.jump(E, 1, { toX: 35.6 }).waitGate(4)
        r.walkTo(F, 38.5).mark('the crate past the second gate')
        // ember back to room A and into the lava there: the third gate
        r.walkTo(E, 33.5).land(E).walkToStop(E, 14.5).waitGate(7)
        r.oneUntil(F, LEFT, (_, s) => s.boxes[1]!.y > 61, { max: 900 }, 'the crate to drop into the pit').waitGate(5)
        r.walkTo(F, 12.4).land(F).jump(F, -1, { toX: 10.5 }).intoDoor(F, -1)
        last(r, E, RIGHT)
      },
      (r) => {
        // room D. ember up the rungs into the lava, the top mirror, and off the ledge's far end
        r.walkToStop(E, 6.6).jump(E, 0).jump(E, 0).jump(E, 1, { toX: 8.6 })
        r.walkToStop(E, 10.8).press(E).walkTo(E, 12.6).land(E).walkTo(E, 14.5)
        // frost into the water, the bottom mirror, and out
        r.walkToStop(F, 11).press(F).walkTo(F, 13.5).waitGate(0)
        r.walkToStop(E, 23.5).walkToStop(F, 22.8).checkpoint(1).mark('room E')
        // room E. ember onto the lift, frost into the beam: up she goes
        r.walkTo(E, 39.2).jump(E, 1, { toX: 40.5 }).settle(E)
        r.walkToStop(F, 37).waitMover(0)
        r.walkToStop(E, 38.8)
        // frost to the lift as it comes back down; ember into the beam on the deck
        r.walkToStop(F, 38.6).waitMover(0, 0).jump(F, 1, { toX: 40.5 }).settle(F)
        r.jump(E, -1, { toX: 35.5 }).waitMover(0)
        r.walkToStop(F, 38.8)
        // the key in the lava, frost's gem, and on over the lift's gap
        r.walkTo(E, 27.2).walkTo(E, 36).jump(E, 1, { toX: 38.8 })
        r.jump(F, -1, { toX: 35.5 }).walkTo(F, 33).walkTo(F, 36).jump(F, 1, { toX: 38.8 })
        r.expect((s) => s.keys === 1 || !!process.env.EMBER_HACK, 'the key taken')
        r.walkTo(E, 39.4).jump(E, 1, { toX: 43 }).walkTo(F, 39.4).jump(F, 1, { toX: 42.6 })
        r.walkToStop(E, 44.5).walkToStop(F, 43.8).checkpoint(0).mark('room F')
        // room F. onto the walkway: ember melts the ice and opens the lock, then back and down
        r.walkTo(E, 47.4).land(E)
        r.oneUntil(E, RIGHT, (_, s) => s.ice[0]! >= 1, {}, 'the ice to melt')
        r.oneUntil(E, RIGHT, (_, s) => s.locks[0]!, {}, 'the lock to open')
        r.walkTo(E, 44.3).land(E)
        // frost through the water to the mirror, and out of the light the same way
        r.walkTo(F, 47.4).land(F).walkToStop(F, 56).press(F)
        r.walkTo(F, 44.3).land(F).waitGate(1).waitMover(1, 0)
        // frost in onto the little lift; ember back up the rungs into the beam, which sends him up
        r.walkTo(F, 60.5).jump(F, 1, { toX: 62 }).settle(F)
        r.walkToStop(E, 46.6).jump(E, 0).jump(E, 0).expect(() => r.at(E).y === 11, 'ember back on the walkway').waitMover(1)
        r.intoDoor(F, -1)
        // and out of it again for her own gate
        r.walkTo(E, 44.3).land(E).waitGate(1)
        last(r, E, RIGHT)
      },
    ],
  },
}

// EMBER_TRACE: the gems a walkthrough leaves behind, per stage, as tiles
function missed(def: LevelDef, walk: Walkthrough): string {
  const level = compileLevel(def)
  let out = ''
  let s = startStage(level, 0)
  for (let i = 0; i < level.stages.length; i++) {
    let last = s
    for (const [ticks, a, b] of walk[i]!) for (let t = 0; t < ticks && s.phase === 'play' && s.stage === i; t++) s = step(level, (last = s), [a, b])
    const end = s.stage === i ? s : last
    const left = level.stages[i]!.gems.filter((_, g) => !end.taken[g]).map((g) => `${g.el} ${Math.floor(g.x / 20)},${Math.floor(g.y / 20)}`)
    if (left.length) out += `\nstage ${i + 1} leaves ${left.join('; ')}`
    for (let t = 0; t < 600 && s.phase === 'clear'; t++) s = step(level, s, [0, 0])
  }
  return out
}

function walkFor(def: LevelDef): { walk: Walkthrough | undefined; error: unknown } {
  const run = RUNS[def.id]
  if (!run) return { walk: SCRIPTS[def.id] && one(SCRIPTS[def.id]!), error: null }
  try {
    const walk = record(def, run.stages)
    if (process.env.EMBER_TRACE) console.log(`${def.id}\n${timingSummary(compileLevel(def), walk)}${missed(def, walk)}`)
    return { walk, error: null }
  } catch (error) {
    return { walk: undefined, error }
  }
}

// TODO(ember-2:levels-a): levels whose walkthrough stops where a mechanic the engine hasn't landed yet begins
const WAITING: Record<string, string> = { 'updraft-tower': 'arrows (engine step 2)', 'lock-and-light': 'keys and locks (engine step 4)' }

// EMBER_STAGE='level-id:2' runs one stage's recording on its own, from that stage's start, and prints how far it got
if (process.env.EMBER_STAGE) {
  const [id, n] = process.env.EMBER_STAGE.split(':')
  const level = compileLevel(BASICS.find((d) => d.id === id)!)
  const run = new StageRun(level, Number(n) - 1)
  // EMBER_HACK: every lock already open, to try a stage before the engine has keys
  if (process.env.EMBER_HACK) run.state = { ...run.state, keys: 1, locks: run.state.locks.map(() => true) }
  try {
    RUNS[id!]!.stages[Number(n) - 1]!(run)
    console.log(`${id} stage ${n}: ${(run.tick / 60).toFixed(1)} s, ${run.state.phase}\n${run.timings()}`)
  } catch (e) {
    console.log(String(e).split('\n').slice(0, 26).join('\n'))
  }
}

describe('basics', () => {
  for (const def of BASICS) {
    const { walk, error } = walkFor(def)
    if (WAITING[def.id] && !walk) {
      if (process.env.EMBER_TRACE) console.log(String(error).split('\n').slice(0, 24).join('\n'))
      it(`${def.id} validates`, () => expect(validateLevel(def)).toEqual([]))
      it.todo(`${def.id}: the walkthrough waits on ${WAITING[def.id]}`)
      continue
    }
    if (error) it(`${def.id}: the walkthrough records`, () => { throw error })
    describeLevel(def, walk, RUNS[def.id]?.checks)
  }
})
