import {
  AIR_ACCEL,
  BOX_SIZE,
  COLS,
  COYOTE_TICKS,
  DEATH_TICKS,
  FAN_ACCEL,
  FAN_MAX_UP,
  FREEZE_RADIUS,
  FREEZE_TICKS,
  GATE_SPEED,
  GRAVITY,
  IN_DOWN,
  IN_LEFT,
  IN_RIGHT,
  IN_UP,
  JUMP_BUFFER_TICKS,
  JUMP_V,
  MAX_FALL,
  MELT_TICKS,
  MOVER_SPEED,
  PLAYER_H,
  PLAYER_W,
  POOL_SINK,
  PORTAL_COOLDOWN_TICKS,
  PUSH_SPEED,
  ROWS,
  RUN_ACCEL,
  RUN_SPEED,
  STEP_HZ,
  STEP_S,
  T_EMPTY,
  T_GOO,
  T_LAVA,
  T_THIN,
  T_WALL,
  T_WATER,
  TILE,
  WORLD_H,
  WORLD_W,
  type Beam,
  type Cond,
  type DeathCause,
  type Dir,
  type GameEvent,
  type GameState,
  type Inputs,
  type Level,
  type LevelDef,
  type Player,
  type PlayerIndex,
  type Rect,
  type Score,
  type Slant,
  type Tile,
} from './emberTypes'

// ember & frost's simulation: pure and deterministic. no DOM, no Math.random, no Date, no iteration over
// Sets or object keys whose order could differ. online play reruns it on both computers, so the same
// (level, state, inputs) must give a bit-identical state everywhere.
//
// one play tick, in order:
//   1. clocks: tick, phaseT, portal cooldowns
//   2. interact: a down-key press flips a lever the player overlaps, else turns a mirror it overlaps
//   3. channels: plates (alive players and boxes overlapping them, as of the end of the last tick), levers,
//      buttons (refreshed while pressed, then counting down), then sensors lit by beams. emitters' `when` is
//      read with this tick's plates/levers/buttons plus last tick's sensors, so a sensor feeding an emitter lags a tick
//   4. gates slide and movers travel, carrying what stands on them; anything they would crush makes them wait
//   5. thin water freezes around frost (or counts down to a thaw); ice melts under fire's touch
//   6. boxes: gravity and fans
//   7. players: run, jump, gravity and fans, then move x (pushing boxes) and y, axis by axis
//   8. pools kill, gems, portals, doors, and the win

const EPS = 0.01 // overlaps thinner than this don't count, so rounded network states don't wedge things
const RIDE = 0.5 // a body whose feet are within this of a solid's top rides it
const PLATE_H = 4
const GEM_BOX = 12
const LAND_VY = 150 // a land event needs a real fall
const JUMP_CUT = 0.5 // releasing jump while rising keeps this much of vy
const MAX_SEGMENTS = 32

const TILE_CHARS: Record<string, Tile> = {
  '#': T_WALL,
  '.': T_EMPTY,
  ' ': T_EMPTY,
  L: T_LAVA,
  W: T_WATER,
  G: T_GOO,
  w: T_THIN,
  '1': T_EMPTY,
  '2': T_EMPTY,
  r: T_EMPTY,
  b: T_EMPTY,
  R: T_EMPTY,
  B: T_EMPTY,
}

const DIRS: Record<Dir, [number, number]> = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] }

const overlaps = (a: Rect, b: Rect) => a.x < b.x + b.w - EPS && b.x < a.x + a.w - EPS && a.y < b.y + b.h - EPS && b.y < a.y + a.h - EPS
const inRect = (r: Rect, x: number, y: number) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
const tileRect = (tx: number, ty: number): Rect => ({ x: tx * TILE, y: ty * TILE, w: TILE, h: TILE })
const playerRect = (p: Player): Rect => ({ x: p.x, y: p.y, w: PLAYER_W, h: PLAYER_H })
const boxRect = (b: { x: number; y: number }): Rect => ({ x: b.x, y: b.y, w: BOX_SIZE, h: BOX_SIZE })

// ---- compiling

// every map problem, as lines. compileLevel throws on the first of these
function mapProblems(def: LevelDef): string[] {
  const out: string[] = []
  if (!Array.isArray(def.map) || def.map.length !== ROWS) out.push(`map has ${def.map?.length ?? 0} rows, needs ${ROWS}`)
  const counts: Record<string, number> = { '1': 0, '2': 0, R: 0, B: 0 }
  ;(def.map ?? []).forEach((row, ty) => {
    if (row.length !== COLS) out.push(`row ${ty} has ${row.length} chars, needs ${COLS}`)
    for (let tx = 0; tx < row.length; tx++) {
      const c = row[tx]!
      if (!(c in TILE_CHARS)) out.push(`unknown char '${c}' at (${tx}, ${ty})`)
      if (c in counts) counts[c]!++
      if ((c === 'R' || c === 'B') && ty === 0) out.push(`door '${c}' at (${tx}, ${ty}) has no room for its top tile`)
    }
  })
  const names: Record<string, string> = { '1': 'fire spawn', '2': 'frost spawn', R: 'fire door', B: 'frost door' }
  for (const c of ['1', '2', 'R', 'B']) {
    if (counts[c] === 0) out.push(`missing ${names[c]} '${c}'`)
    else if (counts[c]! > 1) out.push(`${counts[c]} ${names[c]}s '${c}', needs exactly 1`)
  }
  return out
}

// tile rects each thing covers, for bounds checks
function thingBounds(t: LevelDef['things'][number]): Rect[] {
  switch (t.k) {
    case 'plate':
      return [{ x: t.x, y: t.y, w: t.w ?? 2, h: 1 }]
    case 'button':
      return [{ x: t.x, y: t.y, w: t.w ?? 1, h: 1 }]
    case 'gate':
    case 'ice':
      return [{ x: t.x, y: t.y, w: t.w, h: t.h }]
    case 'mover':
      return [
        { x: t.x, y: t.y, w: t.w, h: t.h },
        { x: t.to.x, y: t.to.y, w: t.w, h: t.h },
      ]
    case 'fan':
      return [{ x: t.x, y: t.y - t.h + 1, w: t.w, h: t.h }]
    case 'portal':
      return [{ x: t.x, y: t.y - 1, w: 1, h: 2 }]
    default:
      return [{ x: t.x, y: t.y, w: 1, h: 1 }]
  }
}

function condChannels(c: Cond | undefined): string[] {
  if (c === undefined) return []
  return (Array.isArray(c) ? c : [c]).map((n) => (n.startsWith('!') ? n.slice(1) : n))
}

function thingProblems(def: LevelDef): string[] {
  const out: string[] = []
  const driven = new Set<string>()
  const pairs: Record<string, number> = {}
  def.things.forEach((t, i) => {
    for (const b of thingBounds(t)) {
      const ok = [b.x, b.y, b.w, b.h].every(Number.isFinite) && b.w > 0 && b.h > 0 && b.x >= 0 && b.y >= 0 && b.x + b.w <= COLS && b.y + b.h <= ROWS
      if (!ok) out.push(`thing ${i} (${t.k}) at (${t.x}, ${t.y}) is out of bounds`)
    }
    if (t.k === 'plate' || t.k === 'lever' || t.k === 'button' || t.k === 'sensor') driven.add(t.ch)
    if (t.k === 'portal') pairs[t.pair] = (pairs[t.pair] ?? 0) + 1
    if (t.k === 'button' && !(t.secs > 0)) out.push(`thing ${i} (button) needs secs > 0`)
  })
  for (const pair of Object.keys(pairs).sort()) if (pairs[pair] !== 2) out.push(`portal pair '${pair}' has ${pairs[pair]} portals, needs exactly 2`)
  def.things.forEach((t, i) => {
    const c = t.k === 'gate' ? t.open : t.k === 'mover' || t.k === 'fan' || t.k === 'emitter' ? t.when : undefined
    for (const ch of condChannels(c)) if (!driven.has(ch)) out.push(`thing ${i} (${t.k}) listens to channel '${ch}' that no plate, lever, button or sensor drives`)
  })
  return out
}

// everything wrong with a level definition, as readable lines; [] when it compiles cleanly
export function validateLevel(def: LevelDef): string[] {
  const m = mapProblems(def)
  return [...m, ...(Array.isArray(def.things) ? thingProblems(def) : ['things is not a list'])]
}

// turns a LevelDef into px geometry. throws on a malformed map (wrong size, missing spawn or door, unknown char),
// a portal without exactly one partner, or a thing out of bounds. channels nobody drives don't throw (validateLevel reports them)
export function compileLevel(def: LevelDef): Level {
  const problems = [...mapProblems(def), ...thingProblems(def).filter((p) => !p.includes('listens to channel'))]
  if (problems.length) throw new Error(`level '${def.id}': ${problems.join('; ')}`)
  const tiles: Tile[] = []
  const spawns: Array<{ x: number; y: number }> = []
  const doors: Rect[] = []
  const gems: Level['gems'] = []
  const thin: number[] = []
  def.map.forEach((row, ty) => {
    for (let tx = 0; tx < COLS; tx++) {
      const c = row[tx]!
      const t = TILE_CHARS[c]!
      tiles.push(t)
      if (t === T_THIN) thin.push(ty * COLS + tx)
      if (c === '1' || c === '2') spawns[Number(c) - 1] = { x: tx * TILE + (TILE - PLAYER_W) / 2, y: (ty + 1) * TILE - PLAYER_H }
      if (c === 'R' || c === 'B') doors[c === 'R' ? 0 : 1] = { x: tx * TILE, y: (ty - 1) * TILE, w: TILE, h: 2 * TILE }
      if (c === 'r' || c === 'b') gems.push({ x: tx * TILE + TILE / 2, y: ty * TILE + TILE / 2, el: c === 'r' ? 'fire' : 'frost' })
    }
  })
  const L: Level = {
    def,
    tiles,
    spawns: spawns as Level['spawns'],
    doors: doors as Level['doors'],
    gems,
    plates: [],
    levers: [],
    buttons: [],
    gates: [],
    movers: [],
    boxes: [],
    fans: [],
    ice: [],
    emitters: [],
    mirrors: [],
    sensors: [],
    portals: [],
    signs: [],
    thin,
  }
  const portalPairs: string[] = []
  const centre = (tx: number, ty: number) => ({ x: tx * TILE + TILE / 2, y: ty * TILE + TILE / 2 })
  for (const t of def.things) {
    switch (t.k) {
      case 'plate':
        L.plates.push({ x: t.x * TILE, y: (t.y + 1) * TILE - PLATE_H, w: (t.w ?? 2) * TILE, h: PLATE_H, ch: t.ch })
        break
      case 'button':
        L.buttons.push({ x: t.x * TILE, y: (t.y + 1) * TILE - PLATE_H, w: (t.w ?? 1) * TILE, h: PLATE_H, ch: t.ch, ticks: Math.max(1, Math.round(t.secs * STEP_HZ)) })
        break
      case 'lever':
        L.levers.push({ ...tileRect(t.x, t.y), ch: t.ch, on: !!t.on })
        break
      case 'gate':
        L.gates.push({ x: t.x * TILE, y: t.y * TILE, w: t.w * TILE, h: t.h * TILE, open: t.open, dir: t.dir ?? 'up' })
        break
      case 'mover': {
        const m: Level['movers'][number] = {
          x: t.x * TILE,
          y: t.y * TILE,
          w: t.w * TILE,
          h: t.h * TILE,
          to: { x: t.to.x * TILE, y: t.to.y * TILE },
          speed: (t.speed ?? MOVER_SPEED) * TILE,
        }
        if (t.when !== undefined) m.when = t.when
        L.movers.push(m)
        break
      }
      case 'box':
        L.boxes.push({ x: t.x * TILE, y: t.y * TILE })
        break
      case 'fan': {
        const f: Level['fans'][number] = { x: t.x * TILE, y: (t.y - t.h + 1) * TILE, w: t.w * TILE, h: t.h * TILE }
        if (t.when !== undefined) f.when = t.when
        L.fans.push(f)
        break
      }
      case 'ice':
        L.ice.push({ x: t.x * TILE, y: t.y * TILE, w: t.w * TILE, h: t.h * TILE })
        break
      case 'emitter': {
        const e: Level['emitters'][number] = { ...centre(t.x, t.y), dir: t.dir }
        if (t.when !== undefined) e.when = t.when
        L.emitters.push(e)
        break
      }
      case 'mirror':
        L.mirrors.push({ ...centre(t.x, t.y), slant: t.slant })
        break
      case 'sensor':
        L.sensors.push({ ...tileRect(t.x, t.y), ch: t.ch })
        break
      case 'portal':
        L.portals.push({ x: t.x * TILE, y: (t.y - 1) * TILE, w: TILE, h: 2 * TILE, pair: t.pair, other: -1 })
        portalPairs.push(t.pair)
        break
      case 'sign':
        L.signs.push({ ...centre(t.x, t.y), text: t.text })
        break
    }
  }
  L.portals.forEach((p, i) => (p.other = portalPairs.findIndex((q, j) => j !== i && q === p.pair)))
  return L
}

// ---- lookups cached per compiled level (pure functions of the level, so caching can't change results)

type Lookup = { mirrorAt: Map<number, number>; sensorAt: Map<number, number>; thinAt: Map<number, number> }
const lookups = new WeakMap<Level, Lookup>()
function lookup(level: Level): Lookup {
  let l = lookups.get(level)
  if (!l) {
    const at = (x: number, y: number) => Math.floor(y / TILE) * COLS + Math.floor(x / TILE)
    l = { mirrorAt: new Map(), sensorAt: new Map(), thinAt: new Map() }
    level.mirrors.forEach((m, i) => l!.mirrorAt.set(at(m.x, m.y), i))
    level.sensors.forEach((m, i) => l!.sensorAt.set(at(m.x, m.y), i))
    level.thin.forEach((t, i) => l!.thinAt.set(t, i))
    lookups.set(level, l)
  }
  return l
}

// ---- state

function evalCond(c: Cond | undefined, on: Set<string>): boolean {
  if (c === undefined) return true
  const one = (n: string) => (n.startsWith('!') ? !on.has(n.slice(1)) : on.has(n))
  return Array.isArray(c) ? c.every(one) : one(c)
}

function newPlayer(level: Level, i: PlayerIndex): Player {
  const sp = level.spawns[i]
  return { x: sp.x, y: sp.y, vx: 0, vy: 0, ground: true, face: 1, alive: true, inDoor: false, coyote: 0, buffer: 0, prevIn: 0, portalCd: 0, gems: 0 }
}

const moverPos = (m: Level['movers'][number], t: number) => ({ x: m.x + (m.to.x - m.x) * t, y: m.y + (m.to.y - m.y) * t })

// a fresh attempt at a level. deaths carries the death count across restarts.
// gates and when-driven movers start where their conditions put them, so a '!plate' gate starts open
export function initState(level: Level, deaths = 0): GameState {
  const s: GameState = {
    tick: 0,
    phase: 'play',
    phaseT: 0,
    deaths,
    players: [newPlayer(level, 0), newPlayer(level, 1)],
    taken: level.gems.map(() => false),
    levers: level.levers.map((l) => l.on),
    buttons: level.buttons.map(() => 0),
    gates: level.gates.map(() => 0),
    movers: level.movers.map((m) => ({ x: m.x, y: m.y, t: 0, dir: 1 as const })),
    boxes: level.boxes.map((b) => ({ x: b.x, y: b.y, vx: 0, vy: 0 })),
    ice: level.ice.map(() => 0),
    thin: level.thin.map(() => 0),
    mirrors: level.mirrors.map((m) => m.slant),
    on: [],
    events: [],
    plates: level.plates.map(() => false),
    sensors: level.sensors.map(() => false),
  }
  const on = channels(level, s, null)
  level.gates.forEach((g, i) => (s.gates[i] = evalCond(g.open, on) ? 1 : 0))
  level.movers.forEach((m, i) => {
    if (m.when === undefined || !evalCond(m.when, on)) return
    s.movers[i] = { ...moverPos(m, 1), t: 1, dir: 1 }
  })
  // sensors may see differently once gates are open
  s.on = [...channels(level, s, null)].sort()
  s.events = []
  return s
}

function clone(s: GameState): GameState {
  return {
    tick: s.tick,
    phase: s.phase,
    phaseT: s.phaseT,
    deaths: s.deaths,
    players: [{ ...s.players[0] }, { ...s.players[1] }],
    taken: s.taken.slice(),
    levers: s.levers.slice(),
    buttons: s.buttons.slice(),
    gates: s.gates.slice(),
    movers: s.movers.map((m) => ({ ...m })),
    boxes: s.boxes.map((b) => ({ ...b })),
    ice: s.ice.slice(),
    thin: s.thin.slice(),
    mirrors: s.mirrors.slice(),
    on: s.on.slice(),
    events: [],
    ...(s.plates ? { plates: s.plates.slice() } : {}),
    ...(s.sensors ? { sensors: s.sensors.slice() } : {}),
  }
}

// ---- geometry

// the tile at (tx, ty) as it currently behaves: outside the map is T_WALL; a frozen thin tile reads as T_WALL
export function tileAt(level: Level, s: GameState, tx: number, ty: number): Tile {
  if (tx < 0 || ty < 0 || tx >= COLS || ty >= ROWS) return T_WALL
  const t = level.tiles[ty * COLS + tx]!
  if (t === T_THIN) {
    const i = lookup(level).thinAt.get(ty * COLS + tx)
    if (i !== undefined && (s.thin[i] ?? 0) > 0) return T_WALL
  }
  return t
}

// current gate rect (after sliding) for drawing and collision
export function gateRect(level: Level, s: GameState, i: number): { x: number; y: number; w: number; h: number } {
  const g = level.gates[i]!
  const f = s.gates[i] ?? 0
  const [dx, dy] = DIRS[g.dir]
  return { x: g.x + dx * f * g.w, y: g.y + dy * f * g.h, w: g.w, h: g.h }
}

const moverRect = (level: Level, s: GameState, i: number): Rect => ({ x: s.movers[i]!.x, y: s.movers[i]!.y, w: level.movers[i]!.w, h: level.movers[i]!.h })

// body ids: players 0 and 1, boxes 2 + i. solid ids: gates 1000 + i, movers 2000 + i
const BOX0 = 2
const GATE0 = 1000
const MOVER0 = 2000

function bodyRect(s: GameState, id: number): Rect {
  return id < BOX0 ? playerRect(s.players[id as PlayerIndex]) : boxRect(s.boxes[id - BOX0]!)
}

type Hit = Rect & { id: number }
const hits: Hit[] = []

// fills `hits` with every solid overlapping r. `self` is the body asking (skipped). players are solid only to boxes
// (players never collide with each other). `skip` lists more ids to ignore (a gate or mover moving its riders, the riders)
function collide(level: Level, s: GameState, r: Rect, self: number, skip?: number[]): number {
  hits.length = 0
  const x0 = Math.max(0, Math.floor((r.x + EPS) / TILE))
  const x1 = Math.min(COLS - 1, Math.floor((r.x + r.w - EPS) / TILE))
  const y0 = Math.max(0, Math.floor((r.y + EPS) / TILE))
  const y1 = Math.min(ROWS - 1, Math.floor((r.y + r.h - EPS) / TILE))
  // the world's edge is a wall
  if (r.x < -EPS) hits.push({ x: -TILE, y: r.y, w: TILE, h: r.h, id: -1 })
  if (r.x + r.w > WORLD_W + EPS) hits.push({ x: WORLD_W, y: r.y, w: TILE, h: r.h, id: -1 })
  if (r.y < -EPS) hits.push({ x: r.x, y: -TILE, w: r.w, h: TILE, id: -1 })
  if (r.y + r.h > WORLD_H + EPS) hits.push({ x: r.x, y: WORLD_H, w: r.w, h: TILE, id: -1 })
  for (let ty = y0; ty <= y1; ty++) {
    for (let tx = x0; tx <= x1; tx++) {
      const t = tileAt(level, s, tx, ty)
      if (t === T_EMPTY) continue
      const sink = t === T_WALL ? 0 : POOL_SINK
      const h = { x: tx * TILE, y: ty * TILE + sink, w: TILE, h: TILE - sink, id: -1 }
      if (overlaps(r, h)) hits.push(h)
    }
  }
  for (let i = 0; i < level.ice.length; i++) if (s.ice[i]! < 1 && overlaps(r, level.ice[i]!)) hits.push({ ...level.ice[i]!, id: -1 })
  for (let i = 0; i < level.gates.length; i++) {
    if (skip?.includes(GATE0 + i)) continue
    const g = gateRect(level, s, i)
    if (overlaps(r, g)) hits.push({ ...g, id: GATE0 + i })
  }
  for (let i = 0; i < level.movers.length; i++) {
    if (skip?.includes(MOVER0 + i)) continue
    const m = moverRect(level, s, i)
    if (overlaps(r, m)) hits.push({ ...m, id: MOVER0 + i })
  }
  for (let i = 0; i < s.boxes.length; i++) {
    const id = BOX0 + i
    if (id === self || skip?.includes(id)) continue
    const b = boxRect(s.boxes[i]!)
    if (overlaps(r, b)) hits.push({ ...b, id })
  }
  if (self >= BOX0) {
    for (let p = 0; p < 2; p++) {
      const pl = s.players[p as PlayerIndex]
      if (!pl.alive || skip?.includes(p)) continue
      const pr = playerRect(pl)
      if (overlaps(r, pr)) hits.push({ ...pr, id: p })
    }
  }
  return hits.length
}

const standsOn = (b: Rect, top: Rect) => Math.abs(b.y + b.h - top.y) <= RIDE && b.x < top.x + top.w - EPS && top.x < b.x + b.w - EPS

// moves a gate or mover (solid id `self`) from rect a to b, carrying everything standing on it (and on those).
// returns false, moving nothing, when the move would push into a body that isn't riding or lift a rider into something
function shove(level: Level, s: GameState, self: number, a: Rect, b: Rect, apply: () => void): boolean {
  const dx = b.x - a.x
  const dy = b.y - a.y
  if (dx === 0 && dy === 0) {
    apply()
    return true
  }
  const bodies: number[] = []
  for (let p = 0; p < 2; p++) if (s.players[p as PlayerIndex].alive) bodies.push(p)
  for (let i = 0; i < s.boxes.length; i++) bodies.push(BOX0 + i)
  // riders: standing on a, then on riders, until nothing new joins
  const riders: number[] = []
  const tops: Rect[] = [a]
  for (let grew = true; grew; ) {
    grew = false
    for (const id of bodies) {
      if (riders.includes(id)) continue
      const r = bodyRect(s, id)
      if (id < BOX0 && s.players[id as PlayerIndex].vy < 0) continue
      if (tops.some((t) => standsOn(r, t))) {
        riders.push(id)
        tops.push(r)
        grew = true
      }
    }
  }
  for (const id of bodies) if (!riders.includes(id) && overlaps(bodyRect(s, id), b)) return false
  // where each rider ends up: the full shift, or just the vertical part if something stops it sideways
  const skip = [self, ...riders]
  const moves: Array<[number, number, number]> = []
  for (const id of riders) {
    const r = bodyRect(s, id)
    let mx = dx
    if (collide(level, s, { ...r, x: r.x + dx, y: r.y + dy }, id, skip)) {
      mx = 0
      if (dy !== 0 && collide(level, s, { ...r, y: r.y + dy }, id, skip)) return false
      if (dy === 0) continue
    }
    // players standing on a box rider: boxes are solid to them, but the box moves too
    moves.push([id, mx, dy])
  }
  apply()
  for (const [id, mx, my] of moves) {
    const o = id < BOX0 ? s.players[id as PlayerIndex] : s.boxes[id - BOX0]!
    o.x += mx
    o.y += my
  }
  return true
}

// ---- channels and light

const BLOCK_NONE = -1

// light segments from every running emitter; also reports which sensors are lit
function trace(level: Level, s: GameState, on: Set<string>, lit: boolean[] | null): Beam[] {
  const beams: Beam[] = []
  if (!level.emitters.length) return beams
  const { mirrorAt, sensorAt } = lookup(level)
  const blockers: Rect[] = []
  level.gates.forEach((_, i) => s.gates[i]! < 1 && blockers.push(gateRect(level, s, i)))
  level.ice.forEach((r, i) => s.ice[i]! < 1 && blockers.push(r))
  s.boxes.forEach((b) => blockers.push(boxRect(b)))
  level.movers.forEach((_, i) => blockers.push(moverRect(level, s, i)))
  s.players.forEach((p) => p.alive && blockers.push(playerRect(p)))
  for (const e of level.emitters) {
    if (!evalCond(e.when, on)) continue
    let x = e.x
    let y = e.y
    let dir = e.dir
    for (let seg = 0; seg < MAX_SEGMENTS; seg++) {
      const [dx, dy] = DIRS[dir]
      // nearest blocker rect the ray enters
      let dObs = Infinity
      for (const r of blockers) {
        if (dy === 0 ? !(y > r.y && y < r.y + r.h) : !(x > r.x && x < r.x + r.w)) continue
        let d: number
        if (dx > 0) d = r.x + r.w > x ? r.x - x : Infinity
        else if (dx < 0) d = r.x < x ? x - (r.x + r.w) : Infinity
        else if (dy > 0) d = r.y + r.h > y ? r.y - y : Infinity
        else d = r.y < y ? y - (r.y + r.h) : Infinity
        dObs = Math.min(dObs, Math.max(0, d))
      }
      // walk tile centres until a wall, a mirror or a sensor
      let dEnd = Infinity
      let turn: Slant | null = null
      let mirror = BLOCK_NONE
      const tx0 = Math.floor(x / TILE)
      const ty0 = Math.floor(y / TILE)
      for (let k = 1; k <= COLS + ROWS; k++) {
        const tx = tx0 + dx * k
        const ty = ty0 + dy * k
        if (tx < 0 || ty < 0 || tx >= COLS || ty >= ROWS || level.tiles[ty * COLS + tx] === T_WALL) {
          dEnd = (k - 0.5) * TILE
          break
        }
        const idx = ty * COLS + tx
        const si = sensorAt.get(idx)
        if (si !== undefined) {
          dEnd = k * TILE
          if (dObs >= dEnd && lit) lit[si] = true
          break
        }
        const mi = mirrorAt.get(idx)
        if (mi !== undefined) {
          dEnd = k * TILE
          turn = s.mirrors[mi]!
          mirror = mi
          break
        }
      }
      const d = Math.min(dObs, dEnd)
      beams.push({ x1: x, y1: y, x2: x + dx * d, y2: y + dy * d })
      if (dObs < dEnd || turn === null || mirror === BLOCK_NONE) break
      x += dx * d
      y += dy * d
      dir = reflect(dir, turn)
    }
  }
  return beams
}

function reflect(d: Dir, m: Slant): Dir {
  if (m === '/') return d === 'right' ? 'up' : d === 'up' ? 'right' : d === 'left' ? 'down' : 'left'
  return d === 'right' ? 'down' : d === 'down' ? 'right' : d === 'left' ? 'up' : 'left'
}

// light beam segments for the current state, emitter to wherever each stops (for drawing)
export function traceBeams(level: Level, s: GameState): Beam[] {
  return trace(level, s, new Set(s.on), null)
}

const rectMid = (r: Rect) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 })

// which channels are on. with `ev` (a stepping state) it also advances buttons, records plates/sensors and pushes change events
function channels(level: Level, s: GameState, ev: GameEvent[] | null): Set<string> {
  const on = new Set<string>()
  const bodies: Rect[] = []
  for (const p of s.players) if (p.alive) bodies.push(playerRect(p))
  for (const b of s.boxes) bodies.push(boxRect(b))
  const pressed = (r: Rect) => bodies.some((b) => overlaps(b, r))
  const plates = level.plates.map((p) => pressed(p))
  level.plates.forEach((p, i) => {
    if (plates[i]) on.add(p.ch)
    if (ev && s.plates && s.plates[i] !== plates[i]) ev.push({ k: 'plate', i, on: plates[i]!, ...rectMid(p) })
  })
  s.plates = plates
  level.levers.forEach((l, i) => s.levers[i] && on.add(l.ch))
  level.buttons.forEach((b, i) => {
    const was = s.buttons[i]! > 0
    if (ev) s.buttons[i] = pressed(b) ? b.ticks : Math.max(0, s.buttons[i]! - 1)
    const now = s.buttons[i]! > 0
    if (now) on.add(b.ch)
    if (ev && was !== now) ev.push({ k: 'button', i, on: now, ...rectMid(b) })
  })
  if (level.sensors.length) {
    // emitters see this tick's plates, levers and buttons plus last tick's sensors
    const seen = new Set(on)
    // a state off the network has no sensors list; its `on` still says which sensor channels were lit
    if (s.sensors) level.sensors.forEach((m, i) => s.sensors![i] && seen.add(m.ch))
    else level.sensors.forEach((m) => s.on.includes(m.ch) && seen.add(m.ch))
    const lit = level.sensors.map(() => false)
    trace(level, s, seen, lit)
    level.sensors.forEach((m, i) => {
      if (lit[i]) on.add(m.ch)
      if (ev && s.sensors && s.sensors[i] !== lit[i]) ev.push({ k: 'sensor', i, on: lit[i]!, ...rectMid(m) })
    })
    s.sensors = lit
  } else s.sensors = []
  return on
}

// ---- the tick

const centreOf = (p: Player) => ({ x: p.x + PLAYER_W / 2, y: p.y + PLAYER_H / 2 })

// fan lift on a body whose centre x is in a running fan's column and whose feet are inside it
function fanned(level: Level, on: Set<string>, r: Rect): boolean {
  const cx = r.x + r.w / 2
  const bottom = r.y + r.h
  for (const f of level.fans) if (cx >= f.x && cx < f.x + f.w && bottom > f.y && r.y < f.y + f.h && evalCond(f.when, on)) return true
  return false
}

function liftFall(vy: number, fan: boolean): number {
  vy += GRAVITY * STEP_S
  if (fan && vy > -FAN_MAX_UP) vy = Math.max(-FAN_MAX_UP, vy - FAN_ACCEL * STEP_S)
  return Math.min(MAX_FALL, vy)
}

// the pool under a body's feet, if any: the tile containing a point just above the feet
function poolUnder(level: Level, s: GameState, r: Rect): Tile {
  const y = Math.floor((r.y + r.h - 1) / TILE)
  for (const x of [r.x + 2, r.x + r.w / 2, r.x + r.w - 2]) {
    const t = tileAt(level, s, Math.floor(x / TILE), y)
    if (t === T_LAVA || t === T_WATER || t === T_GOO || t === T_THIN) return t
  }
  return T_EMPTY
}

function killer(t: Tile, el: PlayerIndex): DeathCause | null {
  if (t === T_GOO) return 'goo'
  if (t === T_LAVA && el === 1) return 'lava'
  if ((t === T_WATER || t === T_THIN) && el === 0) return 'water'
  return null
}

// moves body `id` along y by dy; returns true when it hit something (and snaps flush to it)
function moveY(level: Level, s: GameState, id: number, o: { x: number; y: number }, w: number, h: number, dy: number): boolean {
  if (dy === 0) return false
  const r = { x: o.x, y: o.y + dy, w, h }
  if (!collide(level, s, r, id)) {
    o.y += dy
    return false
  }
  if (dy > 0) o.y = Math.max(o.y, Math.min(...hits.map((t) => t.y)) - h)
  else o.y = Math.min(o.y, Math.max(...hits.map((t) => t.y + t.h)))
  return true
}

// moves body `id` along x by dx, stepping up pool lips; returns true when it hit something
function moveX(level: Level, s: GameState, id: number, o: { x: number; y: number }, w: number, h: number, dx: number, ground: boolean): boolean {
  if (dx === 0) return false
  const r = { x: o.x + dx, y: o.y, w, h }
  if (!collide(level, s, r, id)) {
    o.x += dx
    return false
  }
  // climbing out of a pool: everything in the way tops out within POOL_SINK of our feet
  if (ground) {
    const top = Math.min(...hits.map((t) => t.y))
    const up = o.y + h - top
    if (up > 0 && up <= POOL_SINK + EPS && !collide(level, s, { x: o.x + dx, y: top - h, w, h }, id)) {
      o.x += dx
      o.y = top - h
      return false
    }
    collide(level, s, r, id)
  }
  if (dx > 0) o.x = Math.max(o.x, Math.min(...hits.map((t) => t.x)) - w)
  else o.x = Math.min(o.x, Math.max(...hits.map((t) => t.x + t.w)))
  return true
}

// pushes a box sideways by up to dx (it stops flush against whatever's in the way); true when it moved
function pushBox(level: Level, s: GameState, i: number, dx: number): boolean {
  const b = s.boxes[i]!
  const x0 = b.x
  moveX(level, s, BOX0 + i, b, BOX_SIZE, BOX_SIZE, dx, false)
  b.vx = (b.x - x0) / STEP_S
  return b.x !== x0
}

function stepPlayer(level: Level, s: GameState, pi: PlayerIndex, bits: number, on: Set<string>, prevBoxVx: number[]): void {
  const p = s.players[pi]
  const ev = s.events
  const pressed = bits & ~p.prevIn
  const dirX = (bits & IN_RIGHT ? 1 : 0) - (bits & IN_LEFT ? 1 : 0)
  if (dirX) p.face = dirX > 0 ? 1 : -1

  // run
  const target = dirX * RUN_SPEED
  const acc = (p.ground ? RUN_ACCEL : AIR_ACCEL) * STEP_S
  p.vx = p.vx < target ? Math.min(target, p.vx + acc) : Math.max(target, p.vx - acc)

  // jump: buffered presses and coyote time
  if (pressed & IN_UP) p.buffer = JUMP_BUFFER_TICKS
  if (p.ground) p.coyote = COYOTE_TICKS
  if (p.buffer > 0 && p.coyote > 0) {
    p.vy = -JUMP_V
    p.buffer = 0
    p.coyote = 0
    p.ground = false
    ev.push({ k: 'jump', p: pi, ...centreOf(p) })
  } else {
    if (p.buffer > 0) p.buffer--
    if (!p.ground && p.coyote > 0) p.coyote--
  }
  // a short hop: letting go while rising
  if (p.prevIn & IN_UP && !(bits & IN_UP) && p.vy < 0) p.vy *= JUMP_CUT

  // y moves by the average of this tick's start and end velocity: a full jump peaks at JUMP_V²/2g = 63px
  const v0 = p.vy
  p.vy = liftFall(p.vy, fanned(level, on, playerRect(p)))

  // x, pushing a box that's in the way
  const dx = p.vx * STEP_S
  if (dx !== 0) {
    const r = { x: p.x + dx, y: p.y, w: PLAYER_W, h: PLAYER_H }
    let done = false
    if (collide(level, s, r, pi) === 1 && hits[0]!.id >= BOX0 && hits[0]!.id < GATE0) {
      const bi = hits[0]!.id - BOX0
      const b = s.boxes[bi]!
      // only a box beside us, not one we're under or on
      if (b.y < p.y + PLAYER_H - POOL_SINK - EPS && b.y + BOX_SIZE > p.y + EPS) {
        const amt = Math.sign(dx) * Math.min(Math.abs(dx), PUSH_SPEED * STEP_S)
        // the gap between us and the box first, then push the rest
        const gap = dx > 0 ? b.x - (p.x + PLAYER_W) : p.x - (b.x + BOX_SIZE)
        const pushBy = amt - Math.sign(dx) * Math.max(0, gap)
        if (Math.sign(pushBy) === Math.sign(dx) && pushBox(level, s, bi, pushBy)) {
          p.x = dx > 0 ? b.x - PLAYER_W : b.x + BOX_SIZE
          p.vx = Math.sign(dx) * Math.min(Math.abs(p.vx), PUSH_SPEED)
          if (!prevBoxVx[bi]) ev.push({ k: 'push', p: pi, ...centreOf(p) })
          done = true
        }
      }
    }
    // pressing into a wall keeps the run speed, so a jump up beside a ledge pops over its corner
    if (!done && moveX(level, s, pi, p, PLAYER_W, PLAYER_H, dx, p.ground) && Math.sign(dx) !== dirX) p.vx = 0
  }

  // y
  const wasGround = p.ground
  const fallVy = p.vy
  const hit = moveY(level, s, pi, p, PLAYER_W, PLAYER_H, ((v0 + p.vy) / 2) * STEP_S)
  p.ground = hit && fallVy >= 0
  if (hit) p.vy = 0
  if (p.ground && !wasGround) {
    if (fallVy > LAND_VY) ev.push({ k: 'land', p: pi, ...centreOf(p) })
    // holding jump through a landing hops again
    if (bits & IN_UP) p.buffer = Math.max(p.buffer, 1)
  }
  p.prevIn = bits
}

// advances exactly one STEP_S tick. pure: never mutates `s`, returns a new state whose `events` are this tick's only.
// in phase 'dead' it counts DEATH_TICKS then returns initState(level, deaths + 1); in 'won' it only counts phaseT
export function step(level: Level, s0: GameState, input: Inputs): GameState {
  const s = clone(s0)
  if (s.phase === 'dead') {
    s.phaseT++
    return s.phaseT >= DEATH_TICKS ? initState(level, s.deaths + 1) : s
  }
  if (s.phase === 'won') {
    s.phaseT++
    return s
  }
  const ev = s.events
  s.tick++
  s.phaseT++
  const bits = [input[0] | 0, input[1] | 0]

  // 2. interact
  for (const pi of [0, 1] as const) {
    const p = s.players[pi]
    if (!p.alive || !(bits[pi]! & IN_DOWN) || p.prevIn & IN_DOWN) continue
    const r = playerRect(p)
    const li = level.levers.findIndex((l) => overlaps(r, l))
    if (li >= 0) {
      s.levers[li] = !s.levers[li]
      ev.push({ k: 'lever', i: li, ...rectMid(level.levers[li]!) })
      continue
    }
    const mi = level.mirrors.findIndex((m) => overlaps(r, { x: m.x - TILE / 2, y: m.y - TILE / 2, w: TILE, h: TILE }))
    if (mi >= 0) {
      s.mirrors[mi] = s.mirrors[mi] === '/' ? '\\' : '/'
      ev.push({ k: 'mirror', i: mi, x: level.mirrors[mi]!.x, y: level.mirrors[mi]!.y })
    }
  }

  // 3. channels
  const on = channels(level, s, ev)
  s.on = [...on].sort()

  // 4. gates and movers
  level.gates.forEach((g, i) => {
    const f = s.gates[i]!
    const want = evalCond(g.open, on) ? 1 : 0
    if (f === want) return
    const nf = want > f ? Math.min(1, f + GATE_SPEED * STEP_S) : Math.max(0, f - GATE_SPEED * STEP_S)
    const a = gateRect(level, s, i)
    s.gates[i] = nf
    const b = gateRect(level, s, i)
    s.gates[i] = f
    shove(level, s, GATE0 + i, a, b, () => (s.gates[i] = nf))
  })
  level.movers.forEach((m, i) => {
    const st = s.movers[i]!
    const len = Math.hypot(m.to.x - m.x, m.to.y - m.y)
    if (len === 0) return
    const dt = (m.speed * STEP_S) / len
    let nt: number
    let nd = st.dir
    if (m.when !== undefined) {
      const want = evalCond(m.when, on) ? 1 : 0
      if (st.t === want) return
      nd = want > st.t ? 1 : -1
      nt = nd > 0 ? Math.min(1, st.t + dt) : Math.max(0, st.t - dt)
    } else {
      nt = st.t + st.dir * dt
      if (nt >= 1) [nt, nd] = [1, -1]
      else if (nt <= 0) [nt, nd] = [0, 1]
    }
    const to = moverPos(m, nt)
    const a = moverRect(level, s, i)
    shove(level, s, MOVER0 + i, a, { ...a, x: to.x, y: to.y }, () => {
      st.x = to.x
      st.y = to.y
      st.t = nt
      st.dir = nd
    })
  })

  // 5. thin water and ice
  const frost = s.players[1]
  const fc = centreOf(frost)
  level.thin.forEach((ti, i) => {
    const tx = ti % COLS
    const ty = (ti - tx) / COLS
    const cx = tx * TILE + TILE / 2
    const cy = ty * TILE + TILE / 2
    const near = frost.alive && Math.hypot(fc.x - cx, fc.y - cy) <= FREEZE_RADIUS
    const was = s.thin[i]! > 0
    if (near) s.thin[i] = FREEZE_TICKS
    else if (was) s.thin[i]!--
    const now = s.thin[i]! > 0
    if (now && !was) {
      ev.push({ k: 'freeze', tile: ti, x: cx, y: cy })
      // whatever sat sunk in the water rises onto the new ice
      const top = ty * TILE
      const lift = (o: { x: number; y: number }, w: number, h: number) => {
        if (o.x < tx * TILE + TILE - EPS && tx * TILE < o.x + w - EPS && o.y + h > top + EPS && o.y + h <= top + POOL_SINK + EPS) o.y = top - h
      }
      for (const p of s.players) lift(p, PLAYER_W, PLAYER_H)
      for (const b of s.boxes) lift(b, BOX_SIZE, BOX_SIZE)
    }
    if (was && !now) ev.push({ k: 'thaw', tile: ti, x: cx, y: cy })
  })
  const fire = s.players[0]
  if (fire.alive) {
    const fr = { x: fire.x - 1, y: fire.y - 1, w: PLAYER_W + 2, h: PLAYER_H + 2 }
    level.ice.forEach((r, i) => {
      if (s.ice[i]! >= 1 || !overlaps(fr, r)) return
      s.ice[i] = s.ice[i]! + 1 / MELT_TICKS >= 1 - 1e-6 ? 1 : s.ice[i]! + 1 / MELT_TICKS
      if (s.ice[i] === 1) ev.push({ k: 'melt', i, ...rectMid(r) })
    })
  }

  // 6. boxes
  const prevBoxVx = s.boxes.map((b) => b.vx)
  s.boxes.forEach((b, i) => {
    b.vx = 0
    const v0 = b.vy
    b.vy = liftFall(b.vy, fanned(level, on, boxRect(b)))
    if (moveY(level, s, BOX0 + i, b, BOX_SIZE, BOX_SIZE, ((v0 + b.vy) / 2) * STEP_S)) b.vy = 0
  })

  // 7. players
  for (const pi of [0, 1] as const) if (s.players[pi].alive) stepPlayer(level, s, pi, bits[pi]!, on, prevBoxVx)

  // 8. pools, gems, portals, doors
  for (const pi of [0, 1] as const) {
    const p = s.players[pi]
    if (!p.alive) continue
    const r = playerRect(p)
    const cause = killer(poolUnder(level, s, r), pi)
    if (cause) {
      p.alive = false
      p.inDoor = false
      ev.push({ k: 'die', p: pi, ...centreOf(p), cause })
      continue
    }
    const el = pi === 0 ? 'fire' : 'frost'
    level.gems.forEach((g, gi) => {
      if (s.taken[gi] || g.el !== el) return
      if (!overlaps(r, { x: g.x - GEM_BOX / 2, y: g.y - GEM_BOX / 2, w: GEM_BOX, h: GEM_BOX })) return
      s.taken[gi] = true
      p.gems++
      ev.push({ k: 'gem', p: pi, x: g.x, y: g.y, el })
    })
    const c = centreOf(p)
    const inPortal = level.portals.findIndex((q) => inRect(q, c.x, c.y))
    if (inPortal >= 0 && p.portalCd === 0) {
      const o = level.portals[level.portals[inPortal]!.other]!
      p.x = o.x + (TILE - PLAYER_W) / 2
      p.y = o.y + o.h - PLAYER_H
      p.portalCd = PORTAL_COOLDOWN_TICKS
      ev.push({ k: 'portal', p: pi, ...centreOf(p) })
    } else if (inPortal < 0 && p.portalCd > 0) p.portalCd-- // the cooldown holds while still standing in a portal
    const d = level.doors[pi]
    const cx = p.x + PLAYER_W / 2
    const was = p.inDoor
    p.inDoor = p.ground && cx >= d.x && cx <= d.x + d.w && Math.abs(p.y + PLAYER_H - (d.y + d.h)) < 1
    if (p.inDoor && !was) ev.push({ k: 'door', p: pi, x: d.x + d.w / 2, y: d.y + d.h / 2 })
  }
  if (!s.players[0].alive || !s.players[1].alive) {
    s.phase = 'dead'
    s.phaseT = 0
  } else if (s.players[0].inDoor && s.players[1].inDoor) {
    s.phase = 'won'
    s.phaseT = 0
    ev.push({ k: 'win' })
  }
  return s
}

// stars: 1 for finishing, +1 for every gem, +1 for secs <= par
export function scoreOf(level: Level, s: GameState): Score {
  const secs = s.tick / STEP_HZ
  const gems: [number, number] = [0, 0]
  const total: [number, number] = [0, 0]
  level.gems.forEach((g, i) => {
    const k = g.el === 'fire' ? 0 : 1
    total[k]++
    if (s.taken[i]) gems[k]++
  })
  const won = s.phase === 'won'
  const all = gems[0] === total[0] && gems[1] === total[1]
  const stars = won ? 1 + (all ? 1 : 0) + (secs <= level.def.par ? 1 : 0) : 0
  return { secs, gems, total, stars }
}
