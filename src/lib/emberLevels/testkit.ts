import { describe, expect, it } from 'vitest'
import { compileLevel, startStage, step, validateLevel } from '../emberEngine'
import { reachability, renderReach } from '../emberSolver'
import { CH_LIT, MAX_COLS, MAX_ROWS, MIN_COLS, MIN_ROWS, STEP_HZ, type Cond, type GameState, type Level, type LevelDef, type StageDef } from '../emberTypes'

// what every shipped level has to pass, for the pack test files next to this one. import only from tests

// input frames [ticks, fire bits, frost bits] with 1 left, 2 right, 4 jump, 8 down
export type Script = Array<[ticks: number, fire: number, frost: number]>
// one script per stage, in order. a script only has to get both players into their doors: the runner waits out
// the 'clear' pause and starts the next script on the next stage's first tick
export type Walkthrough = Script[]

export type LevelChecks = {
  // the walkthrough must take at least this long (the clock at the win), so a level can't quietly be short
  minSecs?: number
  // TODO(ember-2:levels): only while the solver can't see a mechanic the level leans on. no shipped level sets this
  skipReach?: boolean
}

const conds = (c: Cond | undefined): string[] => (c === undefined ? [] : Array.isArray(c) ? c : [c]).map((s) => s.replace(/^!/, ''))

function channels(def: StageDef): { used: Set<string>; driven: Set<string> } {
  const used = new Set<string>()
  const driven = new Set<string>()
  for (const t of def.things) {
    if (t.k === 'plate' || t.k === 'lever' || t.k === 'button' || t.k === 'sensor') driven.add(t.ch)
    if (t.k === 'gate') conds(t.open).forEach((c) => used.add(c))
    if (t.k === 'mover' || t.k === 'fan' || t.k === 'emitter') conds(t.when).forEach((c) => used.add(c))
    if (t.k === 'cannon') conds(t.fire).forEach((c) => used.add(c))
  }
  used.delete(CH_LIT)
  return { used, driven }
}

const count = (def: StageDef, ch: string) => def.map.reduce((n, row) => n + [...row].filter((c) => c === ch).length, 0)

// steps a script through the real engine, stopping early when the phase or the stage changes
export function play(level: Level, script: Script, s: GameState): GameState {
  const stage = s.stage
  for (const [ticks, fire, frost] of script) for (let t = 0; t < ticks && s.phase === 'play' && s.stage === stage; t++) s = step(level, s, [fire, frost])
  return s
}

// for WRITING a walkthrough: runs one stage's script from that stage's start (or from `from`) and returns a line
// per segment: the tick, each player's position in tiles (x of the left edge, y of the feet), g on the ground,
// ^ flipped, x dead, and the phase. console.log it from a scratch test until the script gets where it should
export function traceScript(level: Level, stage: number, script: Script, from: GameState = startStage(level, stage)): string {
  let s = from
  const who = (i: 0 | 1) => {
    const p = s.players[i]
    const feet = p.grav > 0 ? p.y + 22 : p.y
    return `${(p.x / 20).toFixed(2)},${(feet / 20).toFixed(2)}${p.ground ? 'g' : ' '}${p.grav < 0 ? '^' : ''}${p.alive ? '' : 'x'}`
  }
  const lines: string[] = []
  script.forEach((seg, n) => {
    s = play(level, [seg], s)
    lines.push(`${String(n).padStart(3)} [${seg.join(', ')}]  t${s.tick}  ember ${who(0)}  frost ${who(1)}  ${s.phase}${s.cp >= 0 ? ` cp${s.cp}` : ''}`)
  })
  return lines.join('\n')
}

// runs a whole walkthrough from a fresh level. returns the last state: phase 'won' when it works
export function runWalkthrough(level: Level, walk: Walkthrough): GameState {
  let s = startStage(level, 0)
  for (let i = 0; i < level.stages.length; i++) {
    s = play(level, walk[i] ?? [], s)
    if (i === level.stages.length - 1 || s.phase !== 'clear') break
    for (let t = 0; t < 600 && s.phase === 'clear'; t++) s = step(level, s, [0, 0])
  }
  return s
}

// registers the standard tests for one level. every level in a pack goes through this, with its walkthrough
export function describeLevel(def: LevelDef, walk: Walkthrough | undefined, checks: LevelChecks = {}): void {
  describe(def.id, () => {
    it('has a kebab-case id, a plain name and a par', () => {
      expect(def.id).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
      expect(def.name).toMatch(/^[a-z0-9 '-]+$/)
      expect(def.par).toBeGreaterThan(0)
      expect(def.stages.length).toBeGreaterThanOrEqual(1)
    })

    it('validates and compiles', () => {
      expect(validateLevel(def)).toEqual([])
      expect(() => compileLevel(def)).not.toThrow()
    })

    it('has at least two gems of each colour', () => {
      const n = (ch: string) => def.stages.reduce((sum, st) => sum + count(st, ch), 0)
      expect(n('r')).toBeGreaterThanOrEqual(2)
      expect(n('b')).toBeGreaterThanOrEqual(2)
    })

    def.stages.forEach((st, i) => {
      const where = def.stages.length > 1 ? `stage ${i + 1}: ` : ''

      it(`${where}is a walled map of an allowed size with one of each spawn and door`, () => {
        const rows = st.map.length
        const cols = st.map[0]!.length
        expect(rows).toBeGreaterThanOrEqual(MIN_ROWS)
        expect(rows).toBeLessThanOrEqual(MAX_ROWS)
        expect(cols).toBeGreaterThanOrEqual(MIN_COLS)
        expect(cols).toBeLessThanOrEqual(MAX_COLS)
        for (const row of st.map) expect(row.length, row).toBe(cols)
        expect(st.map[0]).toBe('#'.repeat(cols))
        expect(st.map[rows - 1]).toBe('#'.repeat(cols))
        for (const row of st.map) expect(row[0]! + row[cols - 1]!, row).toBe('##')
        for (const ch of ['1', '2', 'R', 'B']) expect(count(st, ch), ch).toBe(1)
      })

      it(`${where}drives every channel it uses, and uses every channel it drives`, () => {
        const { used, driven } = channels(st)
        for (const c of used) expect(driven.has(c), `nothing drives '${c}'`).toBe(true)
        for (const c of driven) expect(used.has(c), `nothing listens to '${c}'`).toBe(true)
      })

      it.skipIf(checks.skipReach)(`${where}lets each player reach their door and every gem of their element`, () => {
        const r = reachability(st)
        const why = `\n${renderReach(st, r)}\nmissed ${JSON.stringify(r.missedGems)}`
        expect(r.fireDoor, why).toBe(true)
        expect(r.frostDoor, why).toBe(true)
        expect(r.missedGems, why).toEqual([])
      })

      it(`${where}nobody dies standing still`, () => {
        const level = compileLevel(def)
        const s = play(level, [[120, 0, 0]], startStage(level, i))
        expect(s.phase).toBe('play')
        expect(s.players.map((p) => p.alive)).toEqual([true, true])
        expect(s.deaths).toBe(0)
      })
    })

    it('the walkthrough wins without a death', () => {
      expect(walk, `${def.id} has no walkthrough`).toBeDefined()
      expect(walk!.length).toBe(def.stages.length)
      const s = runWalkthrough(compileLevel(def), walk!)
      expect(s.deaths).toBe(0)
      expect(`${s.phase} on stage ${s.stage + 1}`).toBe(`won on stage ${def.stages.length}`)
      if (checks.minSecs) expect(s.tick / STEP_HZ).toBeGreaterThanOrEqual(checks.minSecs)
    })
  })
}
