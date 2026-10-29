// The games capture: the level packs' own walkthroughs, recorded as each pack's describeLevel sees them, and the
// whole run of each through the TS engine, wire and camera, so the Rust port can be held to the same states.
// Only native/scripts/games-golden/capture.sh loads this; `npm test` never does.
import { mkdirSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { vi } from 'vitest'
import { cameraFor } from '../../../src/lib/emberCamera'
import { compileLevel, scoreOf, stageOf, startStage, step, traceBeams } from '../../../src/lib/emberEngine'
import { encodeEmber, fastOf, slowOf } from '../../../src/lib/emberProtocol'
import { LEVELS } from '../../../src/lib/emberLevels'
import { reachability, renderReach } from '../../../src/lib/emberSolver'
import type { GameEvent, GameState, LevelDef } from '../../../src/lib/emberTypes'

export const OUT = fileURLToPath(new URL('../../core/golden/games/', import.meta.url))
mkdirSync(`${OUT}ember`, { recursive: true })

type Walk = Array<Array<[number, number, number]>>

// every HASH_EVERY steps: the state's hash; every WIRE_EVERY: what the wire, the camera and the beams make of it
const HASH_EVERY = 30
const WIRE_EVERY = 240

function capture(def: LevelDef, walk: Walk | undefined, stateHash: (s: GameState) => string): void {
  if (!walk) return
  const level = compileLevel(def)
  const index = LEVELS.findIndex((l) => l.id === def.id)
  const hashes: Array<[number, string]> = []
  const wire: unknown[] = []
  let n = 0
  let pending: GameEvent[] = []
  const after = (s: GameState) => {
    n++
    pending.push(...s.events)
    if (n % HASH_EVERY === 0) hashes.push([n, stateHash(s)])
    if (n % WIRE_EVERY === 0) {
      const stage = stageOf(level, s)
      const k = slowOf(index, stage, s)
      const g = fastOf(stage, s)
      const msg = encodeEmber({ t: 's', a: 1, f: n, fi: 3, r: 2, k, g, e: pending, q: [n - 1, pending.slice(0, 2)] })
      pending = []
      wire.push({
        msg,
        n,
        fast: JSON.stringify(fastOf(stage, s)),
        slow: JSON.stringify(slowOf(index, stage, s)),
        cam: [cameraFor(stage, s, 16 / 9), cameraFor(stage, s, 0.8)],
        beams: traceBeams(stage, s),
        score: scoreOf(level, s),
      })
    }
  }
  // runWalkthrough, with the hook
  let s = startStage(level, 0)
  for (let i = 0; i < level.stages.length; i++) {
    for (const [ticks, a, b] of walk[i] ?? []) {
      for (let t = 0; t < ticks && s.phase === 'play' && s.stage === i; t++) {
        s = step(level, s, [a, b])
        after(s)
      }
    }
    if (i === level.stages.length - 1 || s.phase !== 'clear') break
    for (let t = 0; t < 600 && s.phase === 'clear'; t++) {
      s = step(level, s, [0, 0])
      after(s)
    }
  }
  const reach = def.stages.map((st) => {
    const r = reachability(st)
    return { fireDoor: r.fireDoor, frostDoor: r.frostDoor, fireGems: r.fireGems, frostGems: r.frostGems, melted: r.melted, frozen: r.frozen, render: renderReach(st, r) }
  })
  const fixture = { id: def.id, walk, steps: n, hashes, final: stateHash(s), finalJson: JSON.stringify(s), wire, reach }
  writeFileSync(`${OUT}ember/${def.id}.json`, JSON.stringify(fixture))
}

vi.mock('../../../src/lib/emberLevels/testkit', async (importOriginal) => {
  const kit = (await importOriginal()) as typeof import('../../../src/lib/emberLevels/testkit')
  return {
    ...kit,
    describeLevel: (def: LevelDef, walk: Walk | undefined) => capture(def, walk, kit.stateHash),
  }
})
