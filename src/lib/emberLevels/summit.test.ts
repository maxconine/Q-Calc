import { describe, expect, it } from 'vitest'
import { compileLevel, validateLevel } from '../emberEngine'
import type { LevelDef } from '../emberTypes'
import { SUMMIT } from './summit'
import { describeLevel, record, StageRun, timingSummary, type LevelChecks, type Walkthrough } from './testkit'

// every level of the pack with a walkthrough that finishes it in the real engine without a death: one function
// per stage, written with the testkit's recorder (each line drives the engine, and the bare inputs it took are
// what describeLevel replays). EMBER_TRACE=1 prints each level's timings
const E = 'ember'
const F = 'frost'

const RUNS: Record<string, { stages: Array<(r: StageRun) => void>; checks: LevelChecks }> = {}

// TODO(ember-2:levels-c): levels whose walkthrough waits on a mechanic the engine hasn't landed yet
const WAITING: Record<string, string> = {
  'head-over-heels': 'gravity (engine step 5)',
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

// the structure checks describeLevel makes, for a level whose walkthrough can't run yet
function describeShape(def: LevelDef): void {
  it(`${def.id} validates, is walled, and has its spawns, doors and gems`, () => {
    expect(validateLevel(def)).toEqual([])
    expect(() => compileLevel(def)).not.toThrow()
    const n = (ch: string) => def.stages.reduce((sum, st) => sum + st.map.join('').split(ch).length - 1, 0)
    expect(n('r')).toBeGreaterThanOrEqual(2)
    expect(n('b')).toBeGreaterThanOrEqual(2)
    for (const st of def.stages) {
      const cols = st.map[0]!.length
      expect(st.map[0]).toBe('#'.repeat(cols))
      expect(st.map[st.map.length - 1]).toBe('#'.repeat(cols))
      for (const row of st.map) expect(row[0]! + row[cols - 1]!, row).toBe('##')
    }
  })
}

describe('summit', () => {
  for (const def of SUMMIT) {
    const { walk, error } = walkFor(def)
    if (WAITING[def.id] && !walk) {
      if (process.env.EMBER_TRACE) console.log(String(error).split('\n').slice(0, 24).join('\n'))
      describeShape(def)
      it.todo(`${def.id}: the walkthrough waits on ${WAITING[def.id]}`)
      continue
    }
    if (error) it(`${def.id}: the walkthrough records`, () => { throw error })
    describeLevel(def, walk, RUNS[def.id]?.checks)
  }
})

void E
void F
