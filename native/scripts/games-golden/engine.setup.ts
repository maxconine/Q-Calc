// The engine tests' own calls: emberEngine.test.ts (and the session and protocol tests) run against an engine that
// records every step, respawn, startStage and initState call, its level, its input state and the hash of what came
// back, so native/core/tests/games_golden.rs can replay them all. A state the recorder made itself goes as a
// reference to that call; a state a test built or changed by hand goes whole.
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { afterAll, expect, vi } from 'vitest'
import type { GameState, Inputs, Level, LevelDef } from '../../../src/lib/emberTypes'

const OUT = fileURLToPath(new URL('../../core/golden/games/ember-calls/', import.meta.url))
mkdirSync(OUT, { recursive: true })

function hash(s: unknown): string {
  const text = JSON.stringify(s)
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16).padStart(8, '0')
}

let file = 'calls'
const defs: string[] = []
const defIds = new Map<string, number>()
type Rec = { id: number; kind: string; lvl: number; args: unknown[]; h: string }
const recs: Rec[] = []
// what each recorded call returned: the object, its id and its hash when it came back
const made = new WeakMap<object, { id: number; h: string }>()
let calls = 0

function levelId(level: Level): number {
  const text = JSON.stringify(level.def)
  let id = defIds.get(text)
  if (id === undefined) {
    id = defs.length
    defs.push(text)
    defIds.set(text, id)
  }
  return id
}

function stateRef(s: GameState): unknown {
  const m = made.get(s)
  if (m && m.h === hash(s)) return m.id
  return JSON.parse(JSON.stringify(s))
}

function record(kind: string, level: Level, args: unknown[], out: GameState): void {
  const id = calls++
  const h = hash(out)
  made.set(out, { id, h })
  recs.push({ id, kind, lvl: levelId(level), args, h })
}

vi.mock('../../../src/lib/emberEngine', async (importOriginal) => {
  const e = (await importOriginal()) as typeof import('../../../src/lib/emberEngine')
  return {
    ...e,
    step: (level: Level, s: GameState, input: Inputs) => {
      const ref = stateRef(s)
      const out = e.step(level, s, input)
      record('step', level, [ref, [input[0] | 0, input[1] | 0]], out)
      return out
    },
    respawn: (level: Level, s: GameState) => {
      const ref = stateRef(s)
      const out = e.respawn(level, s)
      record('respawn', level, [ref], out)
      return out
    },
    initState: (level: Level, deaths?: number) => {
      const out = e.initState(level, deaths)
      record('init', level, [deaths ?? 0], out)
      return out
    },
    startStage: (level: Level, index: number, from?: { tick: number; deaths: number; banked: [number, number] }) => {
      const out = e.startStage(level, index, from)
      record('stage', level, [index, from ?? null], out)
      return out
    },
  }
})

afterAll(() => {
  file = (expect.getState().testPath ?? 'calls').replace(/^.*\//, '').replace(/\.test\.ts$/, '')
  writeFileSync(`${OUT}${file}.defs.json`, JSON.stringify(defs.map((d) => JSON.parse(d) as LevelDef)))
  // steps that each carry on from the call before with the same keys go as one run: its length, the keys, and
  // the hash every 64 steps and at its end
  const lines: string[] = []
  let run: { first: number; lvl: number; input: string; n: number; hs: Array<[number, string]>; last: string } | null = null
  const flush = () => {
    if (!run) return
    if (run.hs.length === 0 || run.hs[run.hs.length - 1]![0] !== run.n - 1) run.hs.push([run.n - 1, run.last])
    lines.push(JSON.stringify(['r', run.first, run.lvl, run.n, JSON.parse(run.input), run.hs]))
    run = null
  }
  for (const r of recs) {
    const input = r.kind === 'step' ? JSON.stringify(r.args[1]) : ''
    if (r.kind === 'step' && r.args[0] === r.id - 1 && run && run.lvl === r.lvl && run.input === input && run.first + run.n === r.id) {
      run.n++
      run.last = r.h
      if ((run.n - 1) % 64 === 0) run.hs.push([run.n - 1, r.h])
      continue
    }
    flush()
    if (r.kind === 'step' && r.args[0] === r.id - 1) {
      run = { first: r.id, lvl: r.lvl, input, n: 1, hs: [], last: r.h }
      continue
    }
    lines.push(JSON.stringify(['c', r.id, r.kind, r.lvl, ...r.args, r.h]))
  }
  flush()
  writeFileSync(`${OUT}${file}.jsonl`, lines.join('\n') + '\n')
  appendFileSync(`${OUT}index.txt`, `${file} ${recs.length}\n`)
})
