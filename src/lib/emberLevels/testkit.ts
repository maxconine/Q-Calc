import { describe, expect, it } from 'vitest'
import { compileLevel, gateRect, stageOf, startStage, step, tileAt, validateLevel } from '../emberEngine'
import { reachability, renderReach } from '../emberSolver'
import {
  CH_LIT,
  CRUMBLE_TICKS,
  MAX_COLS,
  MAX_ROWS,
  MIN_COLS,
  MIN_ROWS,
  PLAYER_H,
  PLAYER_W,
  STEP_HZ,
  TILE,
  T_CRUMBLE,
  T_DARK,
  T_EMPTY,
  T_GOO,
  T_LAVA,
  T_LIGHT,
  T_ONEWAY,
  T_THIN,
  T_TRAMP,
  T_WALL,
  T_WATER,
  type Cond,
  type GameEvent,
  type GameState,
  type Level,
  type LevelDef,
  type Player,
  type StageDef,
} from '../emberTypes'

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

// =====================================================================================================
// tools for writing and debugging long walkthroughs. everything below is an addition: nothing above changed.
//
//   const walk = record(DEF, [
//     (r) => {
//       r.walkTo('ember', 12).jump('ember', 1).walkTo('ember', 20) // run, hop a gap, run on
//       r.press('ember') // the lever / lamp / gravity switch / mirror ember stands at
//       r.waitGate(0).walkTo('frost', 30, { other: 0 }).checkpoint()
//       r.intoDoor('ember', 1).intoDoor('frost', 1)
//     },
//   ])
//   describeLevel(DEF, walk, { minSecs: 45 })
//
// a helper that can't do what it was asked (a wall in the way, a death, a wait that never ends) throws with
// the tick, where both players are, what they stand on and a picture of the map around them.
// console.log(timingSummary(level, walk)) says how long each stage and each checkpoint stretch takes
// =====================================================================================================

export const LEFT = 1
export const RIGHT = 2
export const JUMP = 4
export const DOWN = 8

// a player by index or by name
export type Whom = 0 | 1 | 'ember' | 'frost' | 'fire'
const idx = (p: Whom): 0 | 1 => (p === 0 || p === 'ember' || p === 'fire' ? 0 : 1)
const NAMES = ['ember', 'frost'] as const

type Row = Script[number]

// ---- plain script rows, for scripts written by hand
export const ember = (ticks: number, bits: number): Row => [ticks, bits, 0]
export const frost = (ticks: number, bits: number): Row => [ticks, 0, bits]
export const both = (ticks: number, fire: number, frostBits: number = fire): Row => [ticks, fire, frostBits]
export const idle = (ticks: number): Row => [ticks, 0, 0]
// a short press and release (the down key by default): a lever, lamp, gravity switch or mirror
export const tap = (p: Whom, bits: number = DOWN): Script => (idx(p) === 0 ? [[2, bits, 0], [2, 0, 0]] : [[2, 0, bits], [2, 0, 0]])
// rows and scripts joined into one script
export function script(...parts: Array<Row | Script>): Script {
  const out: Script = []
  for (const part of parts) {
    if (part.length === 3 && typeof part[0] === 'number') out.push(part as Row)
    else out.push(...(part as Script))
  }
  return out
}
export const times = (n: number, ...parts: Array<Row | Script>): Script => Array.from({ length: n }, () => script(...parts)).flat()
export const scriptTicks = (s: Script): number => s.reduce((n, r) => n + r[0], 0)

// ---- reading a state

// the tile a player's feet are in (the head's tile is one further against their gravity)
export function feetTile(p: Player): { x: number; y: number } {
  const x = Math.floor((p.x + PLAYER_W / 2) / TILE)
  const y = p.grav > 0 ? Math.floor((p.y + PLAYER_H - 0.5) / TILE) : Math.floor((p.y + 0.5) / TILE)
  return { x, y }
}

const TILE_NAMES: Record<number, string> = {
  [T_EMPTY]: 'air',
  [T_WALL]: 'wall',
  [T_LAVA]: 'lava',
  [T_WATER]: 'water',
  [T_GOO]: 'goo',
  [T_THIN]: 'thin water',
  [T_TRAMP]: 'trampoline',
  [T_ONEWAY]: 'one-way',
  [T_CRUMBLE]: 'crumbling',
  [T_LIGHT]: 'light block',
  [T_DARK]: 'dark block',
}

const hitsRect = (p: Player, r: { x: number; y: number; w: number; h: number }, grow = 1) =>
  p.x < r.x + r.w + grow && r.x < p.x + PLAYER_W + grow && p.y < r.y + r.h + grow && r.y < p.y + PLAYER_H + grow

// what a player rests on (or would, one px further along their gravity): a tile's name, or the thing
export function under(level: Level, s: GameState, who: Whom): string {
  const stage = stageOf(level, s)
  const p = s.players[idx(who)]
  const probe = { ...p, y: p.y + p.grav * 2 }
  for (let i = 0; i < stage.gates.length; i++) if (hitsRect(probe, gateRect(stage, s, i), 0)) return `gate ${i}`
  for (let i = 0; i < stage.movers.length; i++) if (hitsRect(probe, { ...stage.movers[i]!, x: s.movers[i]!.x, y: s.movers[i]!.y }, 0)) return `mover ${i}`
  for (let i = 0; i < s.boxes.length; i++) if (hitsRect(probe, { x: s.boxes[i]!.x, y: s.boxes[i]!.y, w: TILE, h: TILE }, 0)) return `box ${i}`
  for (let i = 0; i < stage.ice.length; i++) if (s.ice[i]! < 1 && hitsRect(probe, stage.ice[i]!, 0)) return `ice ${i}`
  for (let i = 0; i < stage.locks.length; i++) if (!s.locks[i] && hitsRect(probe, stage.locks[i]!, 0)) return `lock ${i}`
  const f = feetTile(p)
  const ty = f.y + p.grav
  let best = T_EMPTY as number
  for (const tx of [f.x, Math.floor(p.x / TILE), Math.floor((p.x + PLAYER_W - 0.01) / TILE)]) {
    const t = tileAt(stage, s, tx, ty)
    if (t !== T_EMPTY) {
      best = t
      break
    }
  }
  // a one-way a flipped player hangs under is the tile their feet are in
  if (best === T_EMPTY && p.grav < 0 && tileAt(stage, s, f.x, f.y) === T_ONEWAY) best = T_ONEWAY
  return TILE_NAMES[best] ?? `tile ${best}`
}

// one line about a player: where (tiles), how fast, and what state they're in
export function describePlayer(level: Level, s: GameState, who: Whom): string {
  const i = idx(who)
  const p = s.players[i]
  const f = feetTile(p)
  const flags = [
    p.alive ? '' : 'DEAD',
    p.ground ? 'grounded' : 'in the air',
    p.grav < 0 ? 'flipped' : '',
    p.air ? 'holds an arrow' : '',
    p.cannon >= 0 ? `in cannon ${p.cannon}` : '',
    p.fly > 0 ? `flying ${p.fly}` : '',
    p.inDoor ? 'in the door' : '',
  ].filter(Boolean)
  return `${NAMES[i]} tile ${f.x},${f.y} (px ${p.x.toFixed(1)},${p.y.toFixed(1)}) v ${p.vx.toFixed(0)},${p.vy.toFixed(0)}  ${flags.join(', ')}  on ${under(level, s, i)}`
}

// events worth a word in a trace (jumps, landings and pushes are left out)
export function describeEvent(e: GameEvent): string | null {
  switch (e.k) {
    case 'jump':
    case 'land':
    case 'push':
    case 'door':
    case 'portal':
    case 'airjump':
      return e.k === 'jump' || e.k === 'land' || e.k === 'push' ? null : `${NAMES[e.p]} ${e.k}`
    case 'die':
      return `${NAMES[e.p]} DIES (${e.cause ?? '?'}) at tile ${Math.floor(e.x / TILE)},${Math.floor(e.y / TILE)}`
    case 'bounce':
      return `${NAMES[e.p]} bounce ${e.v.toFixed(0)}`
    case 'gem':
      return `${NAMES[e.p]} gem`
    case 'arrow':
    case 'key':
    case 'load':
    case 'fire':
      return `${NAMES[e.p]} ${e.k} ${e.i}`
    case 'flip':
      return `${NAMES[e.p]} flips ${e.grav < 0 ? 'up' : 'down'}`
    case 'lever':
    case 'mirror':
    case 'gswitch':
    case 'unlock':
    case 'checkpoint':
      return `${e.k} ${e.i}`
    case 'plate':
    case 'button':
    case 'sensor':
      return `${e.k} ${e.i} ${e.on ? 'on' : 'off'}`
    case 'light':
      return `lights ${e.on ? 'on' : 'off'}`
    case 'melt':
      return `ice ${e.i} melts`
    case 'break':
    case 'mend':
      return `crumble ${e.k}`
    case 'clear':
      return `stage ${e.stage + 1} clear`
    case 'respawn':
      return `respawn at ${e.cp < 0 ? 'the start' : `checkpoint ${e.cp}`}`
    case 'win':
      return 'WIN'
    default:
      return null
  }
}

export type ViewOpts = {
  // tiles kept around each player (default 9 across, 5 up and down)
  padX?: number
  padY?: number
  // draw the whole stage instead
  whole?: boolean
}

// an ASCII picture of the stage AS IT IS NOW around the players, with tile coordinates on the edges.
// E ember, F frost (lower case when dead); # wall, L W G pools, w thin water (i frozen), T = ~ as in the map
// (, a crumbling block that's shaking, a space once it's gone), O D light / dark blocks that are solid now
// (o d when they aren't); H gate, M mover, X box, I ice, K lock, k key, r b gems, ^ arrow, R B doors, | lever,
// _ plate or button, * lamp, g gravity switch, Q cannon, P portal, C checkpoint (c once taken)
export function viewState(level: Level, s: GameState, opts: ViewOpts = {}): string {
  const stage = stageOf(level, s)
  const { cols, rows } = stage
  const grid: string[][] = []
  for (let y = 0; y < rows; y++) {
    const row: string[] = []
    for (let x = 0; x < cols; x++) {
      const raw = stage.tiles[y * cols + x]!
      const t = tileAt(stage, s, x, y)
      let ch = '.'
      if (raw === T_LIGHT) ch = t === T_WALL ? 'O' : 'o'
      else if (raw === T_DARK) ch = t === T_WALL ? 'D' : 'd'
      else if (raw === T_THIN) ch = t === T_WALL ? 'i' : 'w'
      else if (raw === T_CRUMBLE) {
        const c = s.crumble[stage.crumble.indexOf(y * cols + x)] ?? 0
        ch = c === 0 ? '~' : c <= CRUMBLE_TICKS ? ',' : ' '
      } else ch = ({ [T_WALL]: '#', [T_LAVA]: 'L', [T_WATER]: 'W', [T_GOO]: 'G', [T_TRAMP]: 'T', [T_ONEWAY]: '=' } as Record<number, string>)[t] ?? '.'
      row.push(ch)
    }
    grid.push(row)
  }
  const put = (px: number, py: number, ch: string, over = true) => {
    const x = Math.floor(px / TILE)
    const y = Math.floor(py / TILE)
    if (x < 0 || y < 0 || x >= cols || y >= rows) return
    if (over || grid[y]![x] === '.') grid[y]![x] = ch
  }
  const fill = (r: { x: number; y: number; w: number; h: number }, ch: string, over = true) => {
    for (let y = r.y + 1; y < r.y + r.h; y += TILE) for (let x = r.x + 1; x < r.x + r.w; x += TILE) put(x, y, ch, over)
    put(r.x + r.w - 1, r.y + r.h - 1, ch, over)
  }
  stage.portals.forEach((r) => fill(r, 'P', false))
  stage.doors.forEach((r, i) => fill(r, i === 0 ? 'R' : 'B', false))
  stage.plates.forEach((r) => fill(r, '_', false))
  stage.buttons.forEach((r) => fill(r, '_', false))
  stage.levers.forEach((r) => fill(r, '|', false))
  stage.lamps.forEach((r) => fill(r, '*', false))
  stage.gswitches.forEach((r) => fill(r, 'g', false))
  stage.cannons.forEach((c) => put(c.x, c.y, 'Q'))
  stage.checkpoints.forEach((c, i) => put(c.x, c.y - 1, i === s.cp ? 'c' : 'C', false))
  stage.gems.forEach((g, i) => !s.taken[i] && put(g.x, g.y, g.el === 'fire' ? 'r' : 'b', false))
  stage.keys.forEach((k, i) => !s.keyTaken[i] && put(k.x, k.y, 'k', false))
  stage.arrows.forEach((a, i) => !s.arrows[i] && put(a.x, a.y, '^', false))
  stage.ice.forEach((r, i) => s.ice[i]! < 1 && fill(r, 'I'))
  stage.locks.forEach((r, i) => !s.locks[i] && fill(r, 'K'))
  stage.gates.forEach((_, i) => fill(gateRect(stage, s, i), 'H'))
  stage.movers.forEach((m, i) => fill({ x: s.movers[i]!.x, y: s.movers[i]!.y, w: m.w, h: m.h }, 'M'))
  s.boxes.forEach((b) => put(b.x + TILE / 2, b.y + TILE / 2, 'X'))
  s.players.forEach((p, i) => {
    const ch = (i === 0 ? 'E' : 'F')[p.alive ? 'toUpperCase' : 'toLowerCase']()
    put(p.x + PLAYER_W / 2, p.y + 1, ch)
    put(p.x + PLAYER_W / 2, p.y + PLAYER_H - 1, ch)
  })
  // the window: one box around each player, merged when they overlap
  const padX = opts.padX ?? 9
  const padY = opts.padY ?? 5
  let boxes = s.players.map((p) => {
    const f = feetTile(p)
    return { x0: Math.max(0, f.x - padX), x1: Math.min(cols - 1, f.x + padX), y0: Math.max(0, f.y - padY - 1), y1: Math.min(rows - 1, f.y + padY) }
  })
  if (opts.whole) boxes = [{ x0: 0, x1: cols - 1, y0: 0, y1: rows - 1 }]
  else if (boxes[0]!.x0 <= boxes[1]!.x1 + 1 && boxes[1]!.x0 <= boxes[0]!.x1 + 1 && boxes[0]!.y0 <= boxes[1]!.y1 + 1 && boxes[1]!.y0 <= boxes[0]!.y1 + 1)
    boxes = [{ x0: Math.min(boxes[0]!.x0, boxes[1]!.x0), x1: Math.max(boxes[0]!.x1, boxes[1]!.x1), y0: Math.min(boxes[0]!.y0, boxes[1]!.y0), y1: Math.max(boxes[0]!.y1, boxes[1]!.y1) }]
  return boxes
    .map((b, n) => {
      const head = boxes.length > 1 ? `around ${NAMES[n]}:\n` : ''
      let tens = '    '
      let ones = '    '
      for (let x = b.x0; x <= b.x1; x++) {
        tens += x % 10 === 0 ? String(Math.floor(x / 10) % 10) : ' '
        ones += String(x % 10)
      }
      const lines = [tens, ones]
      for (let y = b.y0; y <= b.y1; y++) lines.push(`${String(y).padStart(3)} ${grid[y]!.slice(b.x0, b.x1 + 1).join('')}`)
      return head + lines.join('\n')
    })
    .join('\n')
}

// everything worth knowing about a state, for a failure message
export function explain(level: Level, s: GameState, why = ''): string {
  const stage = stageOf(level, s)
  const bits = [
    `tick ${s.tick} (${(s.tick / STEP_HZ).toFixed(2)} s)  stage ${s.stage + 1}/${level.stages.length}  phase ${s.phase}  deaths ${s.deaths}  cp ${s.cp}`,
    `lights ${s.lit ? 'on' : 'off'}${s.litT ? ` (${s.litT} ticks left)` : ''}  keys ${s.keys}  on: ${s.on.join(' ') || '-'}  stage ${stage.cols}x${stage.rows}`,
    describePlayer(level, s, 0),
    describePlayer(level, s, 1),
    viewState(level, s),
  ]
  return `${why ? why + '\n' : ''}${bits.join('\n')}`
}

// ---- determinism

// a short hash of everything in a state (the checkpoint snapshot included)
export function stateHash(s: GameState): string {
  const text = JSON.stringify(s)
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16).padStart(8, '0')
}

// the walkthrough run twice must end in the very same state; the second run also goes through JSON every
// `every` ticks, as a state does over the network, which must change nothing
export function checkDeterminism(level: Level, walk: Walkthrough, every = 97): { same: boolean; hash: string; other: string; tick: number } {
  const a = runWalkthrough(level, walk)
  let n = 0
  const b = runWith(level, walk, (s) => (++n % every === 0 ? (JSON.parse(JSON.stringify(s)) as GameState) : s))
  const hash = stateHash(a)
  const other = stateHash(b)
  return { same: hash === other, hash, other, tick: a.tick }
}

// runWalkthrough with a hook after every step (it may return a different state to carry on from)
function runWith(level: Level, walk: Walkthrough, after: (s: GameState, stage: number) => GameState | void): GameState {
  let s = startStage(level, 0)
  for (let i = 0; i < level.stages.length; i++) {
    for (const [ticks, a, b] of walk[i] ?? []) {
      for (let t = 0; t < ticks && s.phase === 'play' && s.stage === i; t++) {
        s = step(level, s, [a, b])
        s = after(s, i) ?? s
      }
    }
    if (i === level.stages.length - 1 || s.phase !== 'clear') break
    for (let t = 0; t < 600 && s.phase === 'clear'; t++) s = step(level, s, [0, 0])
  }
  return s
}

// ---- timing and diagnosis

export type StageTiming = {
  stage: number
  name: string
  ticks: number // of the clock, spent in this stage
  secs: number
  finished: boolean
  // every checkpoint taken, with the clock when it was
  checkpoints: Array<{ i: number; tick: number; secs: number }>
  // the stretches a death would send the players back over: start → flag → flag → door, in seconds
  stretches: number[]
  gems: [fire: number, frost: number]
  deaths: Array<{ tick: number; what: string }>
}

export type Diagnosis = {
  state: GameState
  won: boolean
  secs: number
  stages: StageTiming[]
  // empty when the walkthrough wins with no death; else what went wrong and where, with a picture
  problem: string
}

// runs a walkthrough and says how long each stage took, where the checkpoints fell, and, when it doesn't
// win cleanly, where and why it stopped
export function diagnose(level: Level, walk: Walkthrough): Diagnosis {
  const stages: StageTiming[] = []
  let cur: StageTiming | null = null
  let mark = 0
  let firstDeath: GameState | null = null
  const begin = (i: number, tick: number) => {
    cur = { stage: i, name: level.stages[i]!.def.name ?? '', ticks: 0, secs: 0, finished: false, checkpoints: [], stretches: [], gems: [0, 0], deaths: [] }
    stages.push(cur)
    mark = tick
    return cur
  }
  let start = 0
  const state = runWith(level, walk, (s, i) => {
    const c = cur && cur.stage === i ? cur : ((start = s.tick - 1), begin(i, s.tick - 1))
    for (const e of s.events) {
      if (e.k === 'checkpoint') {
        c.checkpoints.push({ i: e.i, tick: s.tick, secs: s.tick / STEP_HZ })
        c.stretches.push((s.tick - mark) / STEP_HZ)
        mark = s.tick
      } else if (e.k === 'die') {
        c.deaths.push({ tick: s.tick, what: describeEvent(e)! })
        firstDeath ??= s
      } else if (e.k === 'gem') c.gems[e.p]++
      else if (e.k === 'clear' || e.k === 'win') {
        c.finished = true
        c.stretches.push((s.tick - mark) / STEP_HZ)
      }
    }
    c.ticks = s.tick - start
    c.secs = c.ticks / STEP_HZ
  })
  const won = state.phase === 'won'
  let problem = ''
  const dead = firstDeath as GameState | null
  if (dead) problem = explain(level, dead, `the walkthrough dies on stage ${dead.stage + 1}: ${dead.events.map(describeEvent).filter(Boolean).join('; ')}`)
  else if (!won) {
    const used = scriptTicks(walk[state.stage] ?? [])
    const doors = state.players.map((p, i) => `${NAMES[i]} ${p.inDoor ? 'is in the door' : 'is NOT in the door'}`).join(', ')
    problem = explain(level, state, walk[state.stage] ? `stage ${state.stage + 1}'s script ran out (${used} ticks) before the stage ended: ${doors}` : `there is no script for stage ${state.stage + 1}`)
  }
  return { state, won: won && state.deaths === 0, secs: state.tick / STEP_HZ, stages, problem }
}

// the plan's rule: par is the walkthrough's time × 1.6, rounded up to 5 s
export const parFor = (secs: number): number => Math.ceil((secs * 1.6) / 5 - 1e-9) * 5

// a few lines per stage: its time, its checkpoints and the longest stretch between them, then the total and the par it asks for
export function timingSummary(level: Level, walk: Walkthrough, d: Diagnosis = diagnose(level, walk)): string {
  const lines = d.stages.map((t) => {
    const cps = t.checkpoints.map((c) => `cp${c.i} @ ${c.secs.toFixed(1)}s`).join(', ') || 'no checkpoint'
    const longest = t.stretches.length ? Math.max(...t.stretches) : t.secs
    return `stage ${t.stage + 1}${t.name ? ` "${t.name}"` : ''}: ${t.secs.toFixed(1)} s${t.finished ? '' : ' (NOT finished)'}  ${cps}  stretches ${t.stretches.map((x) => x.toFixed(0)).join(' / ') || '-'} s (longest ${longest.toFixed(0)})  gems ${t.gems[0]}r ${t.gems[1]}b${t.deaths.length ? `  deaths ${t.deaths.length}` : ''}`
  })
  lines.push(`${level.def.id}: ${d.secs.toFixed(1)} s in all, ${d.won ? 'won' : `NOT won (${d.state.phase})`}; par is ${level.def.par}, the rule asks ${parFor(d.secs)}`)
  return lines.join('\n')
}

// traceScript with a word on what happened in each segment (deaths and their cause, checkpoints, keys, flips,
// lights…), what each player stands on, and a picture of where it ended
export function traceStage(level: Level, stage: number, sc: Script, from: GameState = startStage(level, stage)): string {
  let s = from
  const lines: string[] = []
  sc.forEach((seg, n) => {
    const seen: string[] = []
    for (let t = 0; t < seg[0] && s.phase === 'play' && s.stage === stage; t++) {
      s = step(level, s, [seg[1], seg[2]])
      for (const e of s.events) {
        const w = describeEvent(e)
        if (w) seen.push(w)
      }
    }
    const pos = (i: 0 | 1) => {
      const p = s.players[i]
      const f = feetTile(p)
      return `${NAMES[i]} ${f.x},${f.y}${p.ground ? 'g' : ' '}${p.grav < 0 ? '^' : ''}${p.alive ? '' : 'x'} on ${under(level, s, i)}`
    }
    lines.push(`${String(n).padStart(3)} [${seg.join(', ')}]  t${s.tick}  ${pos(0)}  |  ${pos(1)}  ${s.phase}${seen.length ? `  << ${seen.join('; ')}` : ''}`)
  })
  lines.push(explain(level, s, s.phase === 'play' ? 'the script ended here, still playing:' : `ended in phase '${s.phase}':`))
  return lines.join('\n')
}

// ---- the recorder: drives the real engine and writes the script as it goes

export type MoveOpts = {
  // what the other player holds meanwhile (default nothing)
  other?: number
  // give up after this many ticks (default 600)
  max?: number
}

export class StageRun {
  readonly level: Level
  readonly stage: number
  state: GameState
  // the script so far: what record() returns for this stage
  script: Script = []
  // labels dropped with mark(), with the clock at that moment
  marks: Array<{ label: string; tick: number }> = []

  constructor(level: Level, stage = 0, from: GameState = startStage(level, stage)) {
    this.level = level
    this.stage = stage
    this.state = from
  }

  // ---- reading
  get tick(): number {
    return this.state.tick
  }
  // true once the stage is over (both in their doors) or someone died
  get over(): boolean {
    return this.state.phase !== 'play' || this.state.stage !== this.stage
  }
  player(p: Whom): Player {
    return this.state.players[idx(p)]
  }
  // tile coordinates of a player's feet
  at(p: Whom): { x: number; y: number } {
    return feetTile(this.player(p))
  }
  view(opts?: ViewOpts): string {
    return viewState(this.level, this.state, opts)
  }
  explain(why = ''): string {
    return explain(this.level, this.state, why)
  }
  fail(why: string): never {
    throw new Error(`${this.level.def.id} stage ${this.stage + 1}, script row ${this.script.length}: ${explain(this.level, this.state, why)}`)
  }

  // ---- raw input
  // holds these bits for so many ticks. every other helper ends up here
  hold(ticks: number, fire: number, frostBits: number): this {
    for (let t = 0; t < ticks && !this.over; t++) {
      this.state = step(this.level, this.state, [fire, frostBits])
      const last = this.script[this.script.length - 1]
      if (last && last[1] === fire && last[2] === frostBits) last[0]++
      else this.script.push([1, fire, frostBits])
      const died = this.state.events.find((e) => e.k === 'die')
      if (died) this.fail(describeEvent(died)!)
    }
    return this
  }
  wait(ticks: number): this {
    return this.hold(ticks, 0, 0)
  }
  // a hand-written piece of script, played as is
  play(sc: Script): this {
    for (const [ticks, a, b] of sc) this.hold(ticks, a, b)
    return this
  }
  private bits(p: Whom, mine: number, other = 0): [number, number] {
    return idx(p) === 0 ? [mine, other] : [other, mine]
  }
  // one player holds `bits`, the other `o.other`
  one(p: Whom, bits: number, ticks: number, o: MoveOpts = {}): this {
    const [a, b] = this.bits(p, bits, o.other)
    return this.hold(ticks, a, b)
  }

  // holds until `done(state)` is true (checked before each tick). fails, with a picture, after `max` ticks
  until(done: (s: GameState) => boolean, fire = 0, frostBits = 0, max = 600, what = 'the condition'): this {
    for (let t = 0; !done(this.state); t++) {
      if (this.over) this.fail(`the stage ended (${this.state.phase}) while waiting for ${what}`)
      if (t >= max) this.fail(`gave up after ${max} ticks waiting for ${what}`)
      this.hold(1, fire, frostBits)
    }
    return this
  }
  // the same for one player's keys
  oneUntil(p: Whom, bits: number, done: (pl: Player, s: GameState) => boolean, o: MoveOpts = {}, what = 'the condition'): this {
    const [a, b] = this.bits(p, bits, o.other)
    return this.until((s) => done(s.players[idx(p)], s), a, b, o.max ?? 600, `${NAMES[idx(p)]}: ${what}`)
  }

  // ---- moving
  // runs until the player's middle is over tile column `tileX` (within `slack` px of its centre; default 3),
  // then lets go. fails when something stops them for 20 ticks. works in the air too
  walkTo(p: Whom, tileX: number, o: MoveOpts & { slack?: number; bits?: number } = {}): this {
    const i = idx(p)
    const target = tileX * TILE + TILE / 2
    const cx = () => this.state.players[i].x + PLAYER_W / 2
    const dir = target > cx() ? 1 : -1
    const slack = o.slack ?? 3
    let still = 0
    for (let t = 0; dir * (target - cx()) > slack; t++) {
      if (this.over) this.fail(`the stage ended (${this.state.phase}) while ${NAMES[i]} walked to column ${tileX}`)
      if (t >= (o.max ?? 900)) this.fail(`${NAMES[i]} didn't get to column ${tileX} in ${o.max ?? 900} ticks`)
      const before = cx()
      this.one(p, (dir > 0 ? RIGHT : LEFT) | (o.bits ?? 0), 1, o)
      still = Math.abs(cx() - before) < 0.01 ? still + 1 : 0
      if (still >= 20) this.fail(`${NAMES[i]} is stuck walking ${dir > 0 ? 'right' : 'left'} to column ${tileX}`)
    }
    return this
  }
  // walks to the column and then waits to come to rest on the ground
  walkToStop(p: Whom, tileX: number, o: MoveOpts & { slack?: number } = {}): this {
    return this.walkTo(p, tileX, o).settle(p, o)
  }
  // holds `bits` (default nothing) until the player is on the ground
  land(p: Whom, bits = 0, o: MoveOpts = {}): this {
    // never land with jump held: the engine would hop again
    return this.oneUntil(p, bits & ~JUMP, (pl) => pl.ground, o, 'to land')
  }
  // waits until the player stands still on the ground
  settle(p: Whom, o: MoveOpts = {}): this {
    return this.oneUntil(p, 0, (pl) => pl.ground && Math.abs(pl.vx) < 1, o, 'to come to rest')
  }
  // waits for BOTH to stand on the ground
  bothLand(max = 600): this {
    return this.until((s) => s.players[0].ground && s.players[1].ground, 0, 0, max, 'both to land')
  }
  // a jump from the ground: presses jump (with a direction: -1 left, 0 straight up, 1 right), holds it while
  // rising (or for `hop` ticks, for a short hop), then keeps the direction until the player lands.
  // `toX`: stop steering once over that tile column. `land: false`: return at the top instead (to take an
  // arrow, to be caught by a fan, to chain into airJump)
  jump(p: Whom, dir: -1 | 0 | 1 = 0, o: MoveOpts & { hop?: number; toX?: number; land?: boolean } = {}): this {
    const i = idx(p)
    const side = dir > 0 ? RIGHT : dir < 0 ? LEFT : 0
    if (!this.player(p).ground) this.oneUntil(p, 0, (pl) => pl.ground, { ...o, max: 120 }, 'to be on the ground before jumping')
    // a fresh press: jump must have been up the tick before
    if (this.player(p).prevIn & JUMP) this.one(p, side, 1, o)
    this.one(p, side | JUMP, 1, o)
    if (this.over) return this
    if (this.player(p).ground) this.fail(`${NAMES[i]} pressed jump and didn't leave the ground`)
    const rising = (pl: Player) => pl.vy * pl.grav < 0
    for (let t = 1; rising(this.player(p)) && !this.over && (o.hop === undefined || t < o.hop); t++) this.one(p, this.steer(p, side, o.toX) | JUMP, 1, o)
    if (o.land === false) return this
    for (let t = 0; !this.player(p).ground && !this.over; t++) {
      if (t > (o.max ?? 600)) this.fail(`${NAMES[i]} jumped and never landed`)
      this.one(p, this.steer(p, side, o.toX), 1, o)
    }
    return this
  }
  private steer(p: Whom, side: number, toX?: number): number {
    if (toX === undefined || !side) return side
    const cx = this.player(p).x + PLAYER_W / 2
    const target = toX * TILE + TILE / 2
    return (side === RIGHT ? cx < target - 2 : cx > target + 2) ? side : 0
  }
  // in the air: steer (-1, 0, 1) until the player holds an arrow's jump
  takeArrow(p: Whom, dir: -1 | 0 | 1 = 0, o: MoveOpts & { bits?: number } = {}): this {
    const side = (dir > 0 ? RIGHT : dir < 0 ? LEFT : 0) | (o.bits ?? 0)
    return this.oneUntil(p, side, (pl) => pl.air > 0, { max: 120, ...o }, 'to take an arrow')
  }
  // spends the arrow's jump: a fresh press in mid-air, held while rising, then steering until landing
  airJump(p: Whom, dir: -1 | 0 | 1 = 0, o: MoveOpts & { toX?: number; land?: boolean } = {}): this {
    const i = idx(p)
    const side = dir > 0 ? RIGHT : dir < 0 ? LEFT : 0
    if (!this.player(p).air) this.fail(`${NAMES[i]} holds no arrow to jump with`)
    if (this.player(p).prevIn & JUMP) this.one(p, side, 1, o)
    this.one(p, side | JUMP, 1, o)
    if (!this.state.events.some((e) => e.k === 'airjump' && e.p === i)) this.fail(`${NAMES[i]} pressed jump in the air and no air jump happened`)
    for (let t = 0; this.player(p).vy * this.player(p).grav < 0 && !this.over && t < 120; t++) this.one(p, this.steer(p, side, o.toX) | JUMP, 1, o)
    if (o.land === false) return this
    return this.oneUntil(p, side, (pl) => pl.ground, o, 'to land after the air jump')
  }
  // on a trampoline: keeps bouncing (jump held, so the bounces grow) until a bounce leaves at `v` px/s or more,
  // then returns just after that bounce, still rising
  bounce(p: Whom, v = 0, o: MoveOpts & { dir?: -1 | 0 | 1 } = {}): this {
    const i = idx(p)
    const side = o.dir === 1 ? RIGHT : o.dir === -1 ? LEFT : 0
    const [a, b] = this.bits(p, side | JUMP, o.other)
    return this.until((s) => s.events.some((e) => e.k === 'bounce' && e.p === i && e.v >= v), a, b, o.max ?? 900, `${NAMES[i]}: a bounce of ${v} or more`)
  }
  // walks (-1 / 1) until the player is in their door
  intoDoor(p: Whom, dir: -1 | 1, o: MoveOpts = {}): this {
    const i = idx(p)
    const door = stageOf(this.level, this.state).doors[i]
    const col = Math.floor((door.x + door.w / 2) / TILE)
    this.walkTo(p, col, { ...o, slack: 2 })
    if (this.over) return this
    return this.oneUntil(p, 0, (pl) => pl.inDoor, { max: 90, ...o }, `to be in the door at column ${col} (walking ${dir > 0 ? 'right' : 'left'})`)
  }

  // ---- switches
  // taps the down key and checks that something answered: a lever, a mirror, a lamp or a gravity switch
  press(p: Whom, o: MoveOpts = {}): this {
    const i = idx(p)
    if (this.player(p).prevIn & DOWN) this.one(p, 0, 1, o)
    this.one(p, DOWN, 1, o)
    const hit = this.state.events.some((e) => e.k === 'lever' || e.k === 'mirror' || e.k === 'light' || e.k === 'gswitch')
    if (!hit) this.fail(`${NAMES[i]} pressed down and nothing answered (no lever, lamp, gravity switch or mirror in touch)`)
    return this.one(p, 0, 1, o)
  }
  // press(), then wait for whoever was flipped to land on their new floor
  flip(p: Whom, o: MoveOpts = {}): this {
    this.press(p, o)
    return this.until((s) => s.players[0].ground && s.players[1].ground, ...this.bits(p, 0, o.other), o.max ?? 600, 'both to land after the flip')
  }

  // ---- waiting for the world
  // a gate fully open (or, with false, fully shut)
  waitGate(i: number, open = true, fire = 0, frostBits = 0, max = 600): this {
    return this.until((s) => (open ? s.gates[i]! >= 1 : s.gates[i]! <= 0), fire, frostBits, max, `gate ${i} to ${open ? 'open' : 'shut'}`)
  }
  // a mover at the end of its path (1), back at its start (0), or wherever `at` says
  waitMover(i: number, at: 0 | 1 | ((m: { x: number; y: number; t: number; dir: 1 | -1 }) => boolean) = 1, fire = 0, frostBits = 0, max = 900): this {
    const ok = typeof at === 'function' ? at : (m: { t: number }) => (at === 1 ? m.t >= 1 : m.t <= 0)
    return this.until((s) => ok(s.movers[i]!), fire, frostBits, max, `mover ${i}`)
  }
  // a channel on (or off)
  waitChannel(ch: string, on = true, fire = 0, frostBits = 0, max = 600): this {
    return this.until((s) => s.on.includes(ch) === on, fire, frostBits, max, `channel '${ch}' ${on ? 'on' : 'off'}`)
  }
  waitLights(on: boolean, fire = 0, frostBits = 0, max = 900): this {
    return this.until((s) => s.lit === on, fire, frostBits, max, `the lights to go ${on ? 'on' : 'off'}`)
  }
  // an arrow back in its place
  waitArrow(i: number, fire = 0, frostBits = 0, max = 600): this {
    return this.until((s) => !s.arrows[i], fire, frostBits, max, `arrow ${i} to come back`)
  }
  // every crumbling block whole again
  waitMend(fire = 0, frostBits = 0, max = 900): this {
    return this.until((s) => s.crumble.every((c) => c === 0), fire, frostBits, max, 'the crumbling blocks to mend')
  }
  // an event of this kind (this tick or a later one)
  waitEvent(k: GameEvent['k'], fire = 0, frostBits = 0, max = 600): this {
    if (this.state.events.some((e) => e.k === k)) return this
    return this.until((s) => s.events.some((e) => e.k === k), fire, frostBits, max, `a '${k}' event`)
  }

  // ---- checking as you go: a wrong turn fails where it happens, not 2000 ticks later
  // the player's feet are in this tile (y optional)
  expectAt(p: Whom, tileX: number, tileY?: number): this {
    const f = this.at(p)
    if (f.x !== tileX || (tileY !== undefined && f.y !== tileY)) this.fail(`${NAMES[idx(p)]} should be at tile ${tileX},${tileY ?? '*'} and is at ${f.x},${f.y}`)
    return this
  }
  // the last checkpoint taken is flag i (default: any flag has been taken)
  checkpoint(i?: number): this {
    if (i === undefined ? this.state.cp < 0 : this.state.cp !== i) this.fail(`checkpoint ${i ?? ''} should have been taken (cp is ${this.state.cp}): both must stand in its zone, on the ground`)
    return this.mark(`checkpoint ${this.state.cp}`)
  }
  expect(ok: (s: GameState) => boolean, what: string): this {
    if (!ok(this.state)) this.fail(`expected ${what}`)
    return this
  }
  // a named moment, for timings()
  mark(label: string): this {
    this.marks.push({ label, tick: this.state.tick })
    return this
  }
  // "label: 12.3 s (+4.1)" per mark
  timings(): string {
    let prev = this.marks[0]?.tick ?? 0
    return this.marks.map((m) => {
      const line = `${m.label}: ${(m.tick / STEP_HZ).toFixed(1)} s (+${((m.tick - prev) / STEP_HZ).toFixed(1)})`
      prev = m.tick
      return line
    }).join('\n')
  }
}

// writes a whole walkthrough by driving the engine: one function per stage, each handed a StageRun that begins
// where the last stage ended. every stage function must leave both players in their doors. what comes back is
// a plain Walkthrough for describeLevel, so the level is still proven by replaying bare inputs
export function record(def: LevelDef | Level, stages: Array<(run: StageRun) => void>): Walkthrough {
  const level = 'def' in def ? def : compileLevel(def)
  const walk: Walkthrough = []
  let s = startStage(level, 0)
  for (let i = 0; i < stages.length; i++) {
    const run = new StageRun(level, i, s)
    stages[i]!(run)
    walk.push(run.script)
    s = run.state
    const last = i === level.stages.length - 1
    if (s.phase === 'play') run.fail(`stage ${i + 1}'s script ended and the stage hasn't: ${s.players.map((p, n) => `${NAMES[n]} ${p.inDoor ? 'is in the door' : 'is NOT in the door'}`).join(', ')}`)
    if (last) break
    for (let t = 0; t < 600 && s.phase === 'clear'; t++) s = step(level, s, [0, 0])
  }
  return walk
}

// a walkthrough as source text, to paste into a test file once record() has found it
export function formatWalkthrough(walk: Walkthrough, perLine = 9): string {
  return walk
    .map((sc) => {
      const rows = sc.map((r) => `[${r.join(', ')}]`)
      const lines: string[] = []
      for (let i = 0; i < rows.length; i += perLine) lines.push('    ' + rows.slice(i, i + perLine).join(', ') + ',')
      return `  [\n${lines.join('\n')}\n  ],`
    })
    .join('\n')
}
