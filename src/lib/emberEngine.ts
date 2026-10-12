import {
  AIR_ACCEL,
  AIR_JUMP_V,
  ARROW_BOX,
  ARROW_RESPAWN_TICKS,
  BOX_SIZE,
  CANNON_POWER,
  CH_LIT,
  CHECKPOINT_ZONE_H,
  CHECKPOINT_ZONE_W,
  COYOTE_TICKS,
  CRUMBLE_GONE_TICKS,
  CRUMBLE_TICKS,
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
  LIGHT_EJECT_TILES,
  MAX_COLS,
  MAX_FALL,
  MAX_ROWS,
  MAX_STAGES,
  MELT_TICKS,
  MIN_COLS,
  MIN_ROWS,
  MOVER_SPEED,
  ONEWAY_H,
  PLAYER_H,
  PLAYER_W,
  POOL_SINK,
  PORTAL_COOLDOWN_TICKS,
  PUSH_SPEED,
  RUN_ACCEL,
  RUN_SPEED,
  STAGE_TICKS,
  STEP_HZ,
  STEP_S,
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
  TILE,
  TRAMP_ADD,
  TRAMP_GAIN,
  TRAMP_MAX_V,
  TRAMP_MIN_V,
  type Aim,
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
  type Snapshot,
  type Stage,
  type StageDef,
  type Tile,
} from './emberTypes'

// ember & frost's simulation: pure and deterministic. no DOM, no Math.random, no Date, no iteration over
// Sets or object keys whose order could differ. online play reruns it on both computers, so the same
// (level, state, inputs) must give a bit-identical state everywhere.
//
// a level is a list of stages; the state says which one is being played (s.stage) and every per-thing array in
// it indexes that stage. one play tick, in order:
//   1. clocks: tick, phaseT, crumbling blocks (breaking, mending once nothing is inside), a timed lamp
//      running out, arrows coming back
//   2. interact: a down-key press flips the first of these the player overlaps: a lever, a lamp, a mirror.
//      the moment the lights change, a body inside a light-only or dark-only block that is now solid is pushed
//      out against its gravity, or (a player) crushed when there's no room within LIGHT_EJECT_TILES
//   3. channels: plates (alive players and boxes overlapping them, as of the end of the last tick), levers,
//      buttons (refreshed while pressed, then counting down), then sensors lit by beams. emitters' `when` is
//      read with this tick's plates/levers/buttons plus last tick's sensors, so a sensor feeding an emitter lags a tick
//   4. gates slide and movers travel, carrying what stands on them; anything they would crush makes them wait
//   5. thin water freezes around frost (or counts down to a thaw); ice melts under fire's touch
//   6. boxes: gravity and fans
//   7. players: run, jump, gravity and fans, then move x (pushing boxes) and y, axis by axis; landing on a
//      trampoline bounces, standing on a crumbling block starts it; one-way platforms only stop a fall
//   8. pools kill, gems, keys (taken on touch, carried by the team), locks (a closed one within a pixel of a
//      player spends a key and is gone for good), arrows (taken in the air; spent by a jump press in mid-air
//      in 7), portals, doors
//   9. checkpoints, then the phase: a death ('dead'), a finished stage ('clear') or the win ('won')
// outside 'play': 'dead' counts DEATH_TICKS then respawns at the last checkpoint; 'clear' counts STAGE_TICKS
// then begins the next stage; 'won' only counts.
//
// ember 2 status: one-way platforms, trampolines, crumbling blocks, double-jump arrows, lights, keys and locks work, and the player's step already
// mirrors by Player.grav. the rest is carried in the state but does nothing yet: search TODO(ember-2:engine)

const EPS = 0.01 // overlaps thinner than this don't count, so rounded network states don't wedge things
const RIDE = 0.5 // a body whose feet are within this of a solid's top rides it
const PLATE_H = 4
const GEM_BOX = 12
const LAND_VY = 150 // a land event needs a real fall
const JUMP_CUT = 0.5 // releasing jump while rising keeps this much of vy
const MAX_SEGMENTS = 32
const ONEWAY_SLOP = 0.05 // feet this far into a one-way still count as on it (states off the network are rounded)

const TILE_CHARS: Record<string, Tile> = {
  '#': T_WALL,
  '.': T_EMPTY,
  ' ': T_EMPTY,
  L: T_LAVA,
  W: T_WATER,
  G: T_GOO,
  w: T_THIN,
  T: T_TRAMP,
  '=': T_ONEWAY,
  '~': T_CRUMBLE,
  O: T_LIGHT,
  D: T_DARK,
  '1': T_EMPTY,
  '2': T_EMPTY,
  r: T_EMPTY,
  b: T_EMPTY,
  R: T_EMPTY,
  B: T_EMPTY,
  '^': T_EMPTY,
  k: T_EMPTY,
  C: T_EMPTY,
}

const DIRS: Record<Dir, [number, number]> = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] }
const D = Math.SQRT1_2
const AIMS: Record<Aim, [number, number]> = { ...DIRS, upleft: [-D, -D], upright: [D, -D], downleft: [-D, D], downright: [D, D] }

const overlaps = (a: Rect, b: Rect) => a.x < b.x + b.w - EPS && b.x < a.x + a.w - EPS && a.y < b.y + b.h - EPS && b.y < a.y + a.h - EPS
const inRect = (r: Rect, x: number, y: number) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
const tileRect = (tx: number, ty: number): Rect => ({ x: tx * TILE, y: ty * TILE, w: TILE, h: TILE })
const playerRect = (p: Player): Rect => ({ x: p.x, y: p.y, w: PLAYER_W, h: PLAYER_H })
const boxRect = (b: { x: number; y: number }): Rect => ({ x: b.x, y: b.y, w: BOX_SIZE, h: BOX_SIZE })

// ---- compiling

const mapCols = (def: StageDef) => def.map?.[0]?.length ?? 0
const mapRows = (def: StageDef) => def.map?.length ?? 0

// every map problem of one stage, as lines. compileLevel throws on the first of these
function mapProblems(def: StageDef): string[] {
  const out: string[] = []
  if (!Array.isArray(def.map)) return ['map is not a list of rows']
  const cols = mapCols(def)
  const rows = mapRows(def)
  if (rows < MIN_ROWS || rows > MAX_ROWS) out.push(`map has ${rows} rows, needs ${MIN_ROWS} to ${MAX_ROWS}`)
  if (cols < MIN_COLS || cols > MAX_COLS) out.push(`map is ${cols} wide, needs ${MIN_COLS} to ${MAX_COLS}`)
  const counts: Record<string, number> = { '1': 0, '2': 0, R: 0, B: 0 }
  def.map.forEach((row, ty) => {
    if (row.length !== cols) out.push(`row ${ty} has ${row.length} chars, needs ${cols} like row 0`)
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

type Thing = StageDef['things'][number]

// a checkpoint's zone in tiles: its own, or the default around the flag
const zoneOf = (t: Extract<Thing, { k: 'checkpoint' }> | { x: number; y: number; zone?: undefined }): Rect =>
  t.zone ?? { x: t.x - (CHECKPOINT_ZONE_W - 1) / 2, y: t.y - CHECKPOINT_ZONE_H + 1, w: CHECKPOINT_ZONE_W, h: CHECKPOINT_ZONE_H }

// tile rects each thing covers, for bounds checks
function thingBounds(t: Thing): Rect[] {
  switch (t.k) {
    case 'plate':
      return [{ x: t.x, y: t.y, w: t.w ?? 2, h: 1 }]
    case 'button':
      return [{ x: t.x, y: t.y, w: t.w ?? 1, h: 1 }]
    case 'gate':
    case 'ice':
    case 'lock':
    case 'dark':
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
    case 'checkpoint':
      return [{ x: t.x, y: t.y, w: 1, h: 1 }, ...(t.zone ? [t.zone] : [])]
    default:
      return [{ x: t.x, y: t.y, w: 1, h: 1 }]
  }
}

function condChannels(c: Cond | undefined): string[] {
  if (c === undefined) return []
  return (Array.isArray(c) ? c : [c]).map((n) => (n.startsWith('!') ? n.slice(1) : n))
}

function thingProblems(def: StageDef): string[] {
  const out: string[] = []
  const cols = mapCols(def)
  const rows = mapRows(def)
  // the lights drive their own channel
  const driven = new Set<string>([CH_LIT])
  const pairs: Record<string, number> = {}
  def.things.forEach((t, i) => {
    for (const b of thingBounds(t)) {
      const ok = [b.x, b.y, b.w, b.h].every(Number.isFinite) && b.w > 0 && b.h > 0 && b.x >= 0 && b.y >= 0 && b.x + b.w <= cols && b.y + b.h <= rows
      if (!ok) out.push(`thing ${i} (${t.k}) at (${t.x}, ${t.y}) is out of bounds`)
    }
    if (t.k === 'plate' || t.k === 'lever' || t.k === 'button' || t.k === 'sensor') {
      if (t.ch === CH_LIT) out.push(`thing ${i} (${t.k}) can't drive '${CH_LIT}': that channel is the lights`)
      driven.add(t.ch)
    }
    if (t.k === 'portal') pairs[t.pair] = (pairs[t.pair] ?? 0) + 1
    if (t.k === 'button' && !(t.secs > 0)) out.push(`thing ${i} (button) needs secs > 0`)
    if (t.k === 'lamp' && t.secs !== undefined && !(t.secs > 0)) out.push(`thing ${i} (lamp) needs secs > 0, or none`)
    if (t.k === 'cannon' && !(t.aim in AIMS)) out.push(`thing ${i} (cannon) has no aim '${String(t.aim)}'`)
    if (t.k === 'cannon' && t.power !== undefined && !(t.power > 0 && t.power <= 60)) out.push(`thing ${i} (cannon) needs power in 1..60 tiles/s`)
  })
  for (const pair of Object.keys(pairs).sort()) if (pairs[pair] !== 2) out.push(`portal pair '${pair}' has ${pairs[pair]} portals, needs exactly 2`)
  def.things.forEach((t, i) => {
    const c = t.k === 'gate' ? t.open : t.k === 'mover' || t.k === 'fan' || t.k === 'emitter' ? t.when : t.k === 'cannon' ? t.fire : undefined
    for (const ch of condChannels(c)) if (!driven.has(ch)) out.push(`thing ${i} (${t.k}) listens to channel '${ch}' that no plate, lever, button or sensor drives`)
  })
  // a key with nowhere to go, or a lock nothing opens, is a mistake
  const keys = def.map.reduce((n, row) => n + [...row].filter((c) => c === 'k').length, 0)
  const locks = def.things.filter((t) => t.k === 'lock').length
  if (keys !== locks) out.push(`${keys} keys for ${locks} locks: every lock needs its own key`)
  return out
}

function stageProblems(def: StageDef): string[] {
  if (!def || typeof def !== 'object') return ['stage is not an object']
  const m = mapProblems(def)
  return [...m, ...(Array.isArray(def.things) ? thingProblems(def) : ['things is not a list'])]
}

// everything wrong with a level definition, as readable lines; [] when it compiles cleanly.
// lines about a stage of a multi-stage level start 'stage N: ' (N from 1)
export function validateLevel(def: LevelDef): string[] {
  if (!Array.isArray(def.stages) || def.stages.length < 1 || def.stages.length > MAX_STAGES) return [`a level needs 1 to ${MAX_STAGES} stages`]
  const many = def.stages.length > 1
  return def.stages.flatMap((st, i) => stageProblems(st).map((p) => (many ? `stage ${i + 1}: ${p}` : p)))
}

// turns a LevelDef into px geometry. throws on a malformed map (wrong size, missing spawn or door,
// unknown char), a portal without exactly one partner, or a thing out of bounds. channels nobody drives don't
// throw (validateLevel reports them)
export function compileLevel(def: LevelDef): Level {
  const problems = validateLevel(def).filter((p) => !p.includes('listens to channel'))
  if (problems.length) throw new Error(`level '${def.id}': ${problems.join('; ')}`)
  const stages = def.stages.map(compileStage)
  const gemTotals: [number, number] = [0, 0]
  for (const st of stages) for (const g of st.gems) gemTotals[g.el === 'fire' ? 0 : 1]++
  return { def, stages, gemTotals }
}

function compileStage(def: StageDef, index: number): Stage {
  const cols = mapCols(def)
  const rows = mapRows(def)
  const tiles: Tile[] = []
  const spawns: Array<{ x: number; y: number }> = []
  const doors: Rect[] = []
  const centre = (tx: number, ty: number) => ({ x: tx * TILE + TILE / 2, y: ty * TILE + TILE / 2 })
  const px = (r: Rect): Rect => ({ x: r.x * TILE, y: r.y * TILE, w: r.w * TILE, h: r.h * TILE })
  const flag = (t: { x: number; y: number; zone?: Rect }) => ({ x: t.x * TILE + TILE / 2, y: (t.y + 1) * TILE, zone: px(zoneOf(t as Extract<Thing, { k: 'checkpoint' }>)) })
  const L: Stage = {
    index,
    def,
    cols,
    rows,
    w: cols * TILE,
    h: rows * TILE,
    tiles,
    dark: !!def.dark,
    spawns: spawns as Stage['spawns'],
    doors: doors as Stage['doors'],
    gems: [],
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
    thin: [],
    crumble: [],
    checkpoints: [],
    lamps: [],
    gswitches: [],
    arrows: [],
    keys: [],
    locks: [],
    cannons: [],
    darks: [],
  }
  def.map.forEach((row, ty) => {
    for (let tx = 0; tx < cols; tx++) {
      const c = row[tx]!
      const t = TILE_CHARS[c]!
      tiles.push(t)
      if (t === T_THIN) L.thin.push(ty * cols + tx)
      if (t === T_CRUMBLE) L.crumble.push(ty * cols + tx)
      if (c === '1' || c === '2') spawns[Number(c) - 1] = { x: tx * TILE + (TILE - PLAYER_W) / 2, y: (ty + 1) * TILE - PLAYER_H }
      if (c === 'R' || c === 'B') doors[c === 'R' ? 0 : 1] = { x: tx * TILE, y: (ty - 1) * TILE, w: TILE, h: 2 * TILE }
      if (c === 'r' || c === 'b') L.gems.push({ ...centre(tx, ty), el: c === 'r' ? 'fire' : 'frost' })
      if (c === '^') L.arrows.push(centre(tx, ty))
      if (c === 'k') L.keys.push(centre(tx, ty))
      if (c === 'C') L.checkpoints.push(flag({ x: tx, y: ty }))
    }
  })
  const portalPairs: string[] = []
  // a plate's or button's strip: the bottom of its row, or the top when it's on the ceiling
  const strip = (t: { x: number; y: number; ceil?: boolean }, w: number): Rect => ({ x: t.x * TILE, y: t.ceil ? t.y * TILE : (t.y + 1) * TILE - PLATE_H, w: w * TILE, h: PLATE_H })
  for (const t of def.things) {
    switch (t.k) {
      case 'plate':
        L.plates.push({ ...strip(t, t.w ?? 2), ch: t.ch, ceil: !!t.ceil })
        break
      case 'button':
        L.buttons.push({ ...strip(t, t.w ?? 1), ch: t.ch, ticks: Math.max(1, Math.round(t.secs * STEP_HZ)), ceil: !!t.ceil })
        break
      case 'lever':
        L.levers.push({ ...tileRect(t.x, t.y), ch: t.ch, on: !!t.on })
        break
      case 'gate':
        L.gates.push({ ...px(t), open: t.open, dir: t.dir ?? 'up' })
        break
      case 'mover': {
        const m: Stage['movers'][number] = { ...px(t), to: { x: t.to.x * TILE, y: t.to.y * TILE }, speed: (t.speed ?? MOVER_SPEED) * TILE }
        if (t.when !== undefined) m.when = t.when
        L.movers.push(m)
        break
      }
      case 'box':
        L.boxes.push({ x: t.x * TILE, y: t.y * TILE })
        break
      case 'fan': {
        const f: Stage['fans'][number] = { x: t.x * TILE, y: (t.y - t.h + 1) * TILE, w: t.w * TILE, h: t.h * TILE }
        if (t.when !== undefined) f.when = t.when
        L.fans.push(f)
        break
      }
      case 'ice':
        L.ice.push(px(t))
        break
      case 'emitter': {
        const e: Stage['emitters'][number] = { ...centre(t.x, t.y), dir: t.dir }
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
      case 'checkpoint':
        L.checkpoints.push(flag(t))
        break
      case 'lamp':
        L.lamps.push({ ...tileRect(t.x, t.y), ticks: t.secs === undefined ? 0 : Math.max(1, Math.round(t.secs * STEP_HZ)) })
        break
      case 'gswitch':
        L.gswitches.push({ ...tileRect(t.x, t.y), who: t.who ?? 'self' })
        break
      case 'lock':
        L.locks.push(px(t))
        break
      case 'cannon': {
        const v = (t.power ?? CANNON_POWER) * TILE
        const c: Stage['cannons'][number] = { ...centre(t.x, t.y), aim: t.aim, vx: AIMS[t.aim][0] * v, vy: AIMS[t.aim][1] * v }
        if (t.fire !== undefined) c.fire = t.fire
        L.cannons.push(c)
        break
      }
      case 'dark':
        L.darks.push(px(t))
        break
    }
  }
  L.portals.forEach((p, i) => (p.other = portalPairs.findIndex((q, j) => j !== i && q === p.pair)))
  return L
}

// ---- lookups cached per compiled stage (pure functions of the stage, so caching can't change results)

type Lookup = { mirrorAt: Map<number, number>; sensorAt: Map<number, number>; thinAt: Map<number, number>; crumbleAt: Map<number, number>; oneWays: boolean }
const lookups = new WeakMap<Stage, Lookup>()
function lookup(stage: Stage): Lookup {
  let l = lookups.get(stage)
  if (!l) {
    const at = (x: number, y: number) => Math.floor(y / TILE) * stage.cols + Math.floor(x / TILE)
    l = { mirrorAt: new Map(), sensorAt: new Map(), thinAt: new Map(), crumbleAt: new Map(), oneWays: stage.tiles.includes(T_ONEWAY) }
    stage.mirrors.forEach((m, i) => l!.mirrorAt.set(at(m.x, m.y), i))
    stage.sensors.forEach((m, i) => l!.sensorAt.set(at(m.x, m.y), i))
    stage.thin.forEach((t, i) => l!.thinAt.set(t, i))
    stage.crumble.forEach((t, i) => l!.crumbleAt.set(t, i))
    lookups.set(stage, l)
  }
  return l
}

// ---- state

// what goes in GameState.on: every channel that's on but the lights' own, which `lit` already says
const listOn = (on: Set<string>) => [...on].filter((c) => c !== CH_LIT).sort()

function evalCond(c: Cond | undefined, on: Set<string>): boolean {
  if (c === undefined) return true
  const one = (n: string) => (n.startsWith('!') ? !on.has(n.slice(1)) : on.has(n))
  return Array.isArray(c) ? c.every(one) : one(c)
}

function newPlayer(stage: Stage, i: PlayerIndex): Player {
  const sp = stage.spawns[i]
  return {
    x: sp.x,
    y: sp.y,
    vx: 0,
    vy: 0,
    ground: true,
    face: 1,
    alive: true,
    inDoor: false,
    coyote: 0,
    buffer: 0,
    prevIn: 0,
    portalCd: 0,
    gems: 0,
    grav: 1,
    air: 0,
    cannon: -1,
    fly: 0,
    cannonCd: 0,
  }
}

const moverPos = (m: Stage['movers'][number], t: number) => ({ x: m.x + (m.to.x - m.x) * t, y: m.y + (m.to.y - m.y) * t })

// the stage the state is in
export function stageOf(level: Level, s: { stage: number }): Stage {
  return level.stages[Math.min(level.stages.length - 1, Math.max(0, s.stage | 0))]!
}

// the beginning of stage `index`. `from` carries the clock, the deaths and the gems banked so far (a fresh level
// when absent). gravity, keys and lights never carry over from one stage to the next.
// gates and when-driven movers start where their conditions put them, so a '!plate' gate starts open
export function startStage(level: Level, index: number, from?: { tick: number; deaths: number; banked: [number, number] }): GameState {
  const stage = level.stages[index]!
  const s: GameState = {
    tick: from?.tick ?? 0,
    phase: 'play',
    phaseT: 0,
    deaths: from?.deaths ?? 0,
    stage: index,
    banked: from ? [from.banked[0], from.banked[1]] : [0, 0],
    players: [newPlayer(stage, 0), newPlayer(stage, 1)],
    taken: stage.gems.map(() => false),
    levers: stage.levers.map((l) => l.on),
    buttons: stage.buttons.map(() => 0),
    gates: stage.gates.map(() => 0),
    movers: stage.movers.map((m) => ({ x: m.x, y: m.y, t: 0, dir: 1 as const })),
    boxes: stage.boxes.map((b) => ({ x: b.x, y: b.y, vx: 0, vy: 0 })),
    ice: stage.ice.map(() => 0),
    thin: stage.thin.map(() => 0),
    mirrors: stage.mirrors.map((m) => m.slant),
    lit: !stage.dark,
    litT: 0,
    arrows: stage.arrows.map(() => 0),
    crumble: stage.crumble.map(() => 0),
    keys: 0,
    keyTaken: stage.keys.map(() => false),
    locks: stage.locks.map(() => false),
    cannons: stage.cannons.map(() => 0),
    cp: -1,
    saved: null,
    on: [],
    events: [],
    plates: stage.plates.map(() => false),
    sensors: stage.sensors.map(() => false),
  }
  const on = channels(stage, s, null)
  stage.gates.forEach((g, i) => (s.gates[i] = evalCond(g.open, on) ? 1 : 0))
  stage.movers.forEach((m, i) => {
    if (m.when === undefined || !evalCond(m.when, on)) return
    s.movers[i] = { ...moverPos(m, 1), t: 1, dir: 1 }
  })
  // sensors may see differently once gates are open
  s.on = listOn(channels(stage, s, null))
  s.events = []
  return s
}

// a fresh go at a level, from its first stage with the clock at zero
export function initState(level: Level, deaths = 0): GameState {
  return startStage(level, 0, { tick: 0, deaths, banked: [0, 0] })
}

// a copy to step on. `saved` is shared, never copied: nothing mutates a snapshot
function clone(s: GameState | Snapshot): GameState {
  return {
    tick: s.tick,
    phase: s.phase,
    phaseT: s.phaseT,
    deaths: s.deaths,
    stage: s.stage,
    banked: [s.banked[0], s.banked[1]],
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
    lit: s.lit,
    litT: s.litT,
    arrows: s.arrows.slice(),
    crumble: s.crumble.slice(),
    keys: s.keys,
    keyTaken: s.keyTaken.slice(),
    locks: s.locks.slice(),
    cannons: s.cannons.slice(),
    cp: s.cp,
    saved: 'saved' in s ? (s.saved ?? null) : null,
    on: s.on.slice(),
    events: [],
    ...(s.plates ? { plates: s.plates.slice() } : {}),
    ...(s.sensors ? { sensors: s.sensors.slice() } : {}),
  }
}

// both players back at the last checkpoint taken in this stage (or the stage's start), the world as it was
// then. the clock keeps the time of death and the death counts. what a death does after DEATH_TICKS, and what
// the retry key does at once
export function respawn(level: Level, s: GameState): GameState {
  const at = { tick: s.tick, deaths: s.deaths + 1, banked: s.banked }
  let n: GameState
  if (s.saved) {
    n = clone(s.saved)
    n.saved = s.saved
    n.tick = at.tick
    n.deaths = at.deaths
    n.phase = 'play'
    n.phaseT = 0
    // a key still held from before the death is not a fresh press
    for (const p of n.players) p.prevIn = IN_LEFT | IN_RIGHT | IN_UP | IN_DOWN
  } else n = startStage(level, s.stage, at)
  n.events = [{ k: 'respawn', cp: n.cp }]
  return n
}

// ---- geometry

// the tile at (tx, ty) as it currently behaves: outside the map is T_WALL; a frozen thin tile reads as T_WALL;
// a light-only or dark-only block reads as T_WALL when it's there and T_EMPTY when it isn't; a crumbling block
// that has fallen away reads as T_EMPTY. trampolines, one-ways and whole crumbling blocks read as themselves
export function tileAt(stage: Stage, s: GameState, tx: number, ty: number): Tile {
  if (tx < 0 || ty < 0 || tx >= stage.cols || ty >= stage.rows) return T_WALL
  const t = stage.tiles[ty * stage.cols + tx]!
  if (t === T_THIN) {
    const i = lookup(stage).thinAt.get(ty * stage.cols + tx)
    if (i !== undefined && (s.thin[i] ?? 0) > 0) return T_WALL
  }
  if (t === T_LIGHT) return s.lit ? T_WALL : T_EMPTY
  if (t === T_DARK) return s.lit ? T_EMPTY : T_WALL
  if (t === T_CRUMBLE) {
    const i = lookup(stage).crumbleAt.get(ty * stage.cols + tx)
    if (i !== undefined && (s.crumble[i] ?? 0) > CRUMBLE_TICKS) return T_EMPTY
  }
  return t
}

// current gate rect (after sliding) for drawing and collision
export function gateRect(stage: Stage, s: GameState, i: number): { x: number; y: number; w: number; h: number } {
  const g = stage.gates[i]!
  const f = s.gates[i] ?? 0
  const [dx, dy] = DIRS[g.dir]
  return { x: g.x + dx * f * g.w, y: g.y + dy * f * g.h, w: g.w, h: g.h }
}

const moverRect = (stage: Stage, s: GameState, i: number): Rect => ({ x: s.movers[i]!.x, y: s.movers[i]!.y, w: stage.movers[i]!.w, h: stage.movers[i]!.h })

// body ids: players 0 and 1, boxes 2 + i. solid ids: gates 1000 + i, movers 2000 + i
const BOX0 = 2
const GATE0 = 1000
const MOVER0 = 2000

function bodyRect(s: GameState, id: number): Rect {
  return id < BOX0 ? playerRect(s.players[id as PlayerIndex]) : boxRect(s.boxes[id - BOX0]!)
}

// t: what a tile hit is made of, as the map has it (T_WALL for everything that isn't a tile). ti: its tile index, else -1
type Hit = Rect & { id: number; t: Tile; ti: number }
const hits: Hit[] = []
// after a moveY that hit something: the hits the body came to rest against
let support: Hit[] = []
const solid = (r: Rect, id: number): Hit => ({ x: r.x, y: r.y, w: r.w, h: r.h, id, t: T_WALL, ti: -1 })

// fills `hits` with every solid overlapping r. `self` is the body asking (skipped). players are solid only to boxes
// (players never collide with each other). `skip` lists more ids to ignore (a gate or mover moving its riders, the riders)
function collide(stage: Stage, s: GameState, r: Rect, self: number, skip?: number[]): number {
  hits.length = 0
  const x0 = Math.max(0, Math.floor((r.x + EPS) / TILE))
  const x1 = Math.min(stage.cols - 1, Math.floor((r.x + r.w - EPS) / TILE))
  const y0 = Math.max(0, Math.floor((r.y + EPS) / TILE))
  const y1 = Math.min(stage.rows - 1, Math.floor((r.y + r.h - EPS) / TILE))
  // the world's edge is a wall
  if (r.x < -EPS) hits.push(solid({ x: -TILE, y: r.y, w: TILE, h: r.h }, -1))
  if (r.x + r.w > stage.w + EPS) hits.push(solid({ x: stage.w, y: r.y, w: TILE, h: r.h }, -1))
  if (r.y < -EPS) hits.push(solid({ x: r.x, y: -TILE, w: r.w, h: TILE }, -1))
  if (r.y + r.h > stage.h + EPS) hits.push(solid({ x: r.x, y: stage.h, w: r.w, h: TILE }, -1))
  for (let ty = y0; ty <= y1; ty++) {
    for (let tx = x0; tx <= x1; tx++) {
      const t = tileAt(stage, s, tx, ty)
      // one-way platforms are never solid here: moveY alone lands a body on them
      if (t === T_EMPTY || t === T_ONEWAY) continue
      const sink = t === T_WALL || t === T_TRAMP || t === T_CRUMBLE ? 0 : POOL_SINK
      const ti = ty * stage.cols + tx
      const h: Hit = { x: tx * TILE, y: ty * TILE + sink, w: TILE, h: TILE - sink, id: -1, t: stage.tiles[ti]!, ti }
      if (overlaps(r, h)) hits.push(h)
    }
  }
  for (let i = 0; i < stage.ice.length; i++) if (s.ice[i]! < 1 && overlaps(r, stage.ice[i]!)) hits.push(solid(stage.ice[i]!, -1))
  for (let i = 0; i < stage.locks.length; i++) if (!s.locks[i] && overlaps(r, stage.locks[i]!)) hits.push(solid(stage.locks[i]!, -1))
  for (let i = 0; i < stage.gates.length; i++) {
    if (skip?.includes(GATE0 + i)) continue
    const g = gateRect(stage, s, i)
    if (overlaps(r, g)) hits.push(solid(g, GATE0 + i))
  }
  for (let i = 0; i < stage.movers.length; i++) {
    if (skip?.includes(MOVER0 + i)) continue
    const m = moverRect(stage, s, i)
    if (overlaps(r, m)) hits.push(solid(m, MOVER0 + i))
  }
  for (let i = 0; i < s.boxes.length; i++) {
    const id = BOX0 + i
    if (id === self || skip?.includes(id)) continue
    const b = boxRect(s.boxes[i]!)
    if (overlaps(r, b)) hits.push(solid(b, id))
  }
  if (self >= BOX0) {
    for (let p = 0; p < 2; p++) {
      const pl = s.players[p as PlayerIndex]
      if (!pl.alive || skip?.includes(p)) continue
      const pr = playerRect(pl)
      if (overlaps(r, pr)) hits.push(solid(pr, p))
    }
  }
  return hits.length
}

// a body with gravity g rests on `on`: its feet on the top, or flipped, on the underside
const restsOn = (b: Rect, g: number, on: Rect) =>
  Math.abs(g > 0 ? b.y + b.h - on.y : b.y - (on.y + on.h)) <= RIDE && b.x < on.x + on.w - EPS && on.x < b.x + b.w - EPS

// moves a gate or mover (solid id `self`) from rect a to b, carrying everything standing on it (and on those),
// and every flipped player standing on its underside.
// returns false, moving nothing, when the move would push into a body that isn't riding or lift a rider into something
function shove(stage: Stage, s: GameState, self: number, a: Rect, b: Rect, apply: () => void): boolean {
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
      const pl = id < BOX0 ? s.players[id as PlayerIndex] : null
      // a player jumping away isn't riding, and one in a cannon is held where it is
      if (pl && (pl.vy * pl.grav < 0 || pl.cannon >= 0)) continue
      const g = pl ? pl.grav : 1
      if (tops.some((t) => restsOn(r, g, t))) {
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
    if (collide(stage, s, { ...r, x: r.x + dx, y: r.y + dy }, id, skip)) {
      mx = 0
      if (dy !== 0 && collide(stage, s, { ...r, y: r.y + dy }, id, skip)) return false
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
function trace(stage: Stage, s: GameState, on: Set<string>, lit: boolean[] | null): Beam[] {
  const beams: Beam[] = []
  if (!stage.emitters.length) return beams
  const { mirrorAt, sensorAt } = lookup(stage)
  const blockers: Rect[] = []
  stage.gates.forEach((_, i) => s.gates[i]! < 1 && blockers.push(gateRect(stage, s, i)))
  stage.ice.forEach((r, i) => s.ice[i]! < 1 && blockers.push(r))
  stage.locks.forEach((r, i) => !s.locks[i] && blockers.push(r))
  s.boxes.forEach((b) => blockers.push(boxRect(b)))
  stage.movers.forEach((_, i) => blockers.push(moverRect(stage, s, i)))
  s.players.forEach((p) => p.alive && blockers.push(playerRect(p)))
  for (const e of stage.emitters) {
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
      for (let k = 1; k <= stage.cols + stage.rows; k++) {
        const tx = tx0 + dx * k
        const ty = ty0 + dy * k
        const t = tx < 0 || ty < 0 || tx >= stage.cols || ty >= stage.rows ? T_WALL : tileAt(stage, s, tx, ty)
        if (t === T_WALL || t === T_TRAMP || t === T_CRUMBLE) {
          dEnd = (k - 0.5) * TILE
          break
        }
        const idx = ty * stage.cols + tx
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
export function traceBeams(stage: Stage, s: GameState): Beam[] {
  return trace(stage, s, new Set(s.lit ? [...s.on, CH_LIT] : s.on), null)
}

const rectMid = (r: Rect) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 })

// which channels are on. with `ev` (a stepping state) it also advances buttons, records plates/sensors and pushes change events
function channels(stage: Stage, s: GameState, ev: GameEvent[] | null): Set<string> {
  const on = new Set<string>()
  if (s.lit) on.add(CH_LIT)
  const bodies: Rect[] = []
  for (const p of s.players) if (p.alive) bodies.push(playerRect(p))
  for (const b of s.boxes) bodies.push(boxRect(b))
  const pressed = (r: Rect) => bodies.some((b) => overlaps(b, r))
  const plates = stage.plates.map((p) => pressed(p))
  stage.plates.forEach((p, i) => {
    if (plates[i]) on.add(p.ch)
    if (ev && s.plates && s.plates[i] !== plates[i]) ev.push({ k: 'plate', i, on: plates[i]!, ...rectMid(p) })
  })
  s.plates = plates
  stage.levers.forEach((l, i) => s.levers[i] && on.add(l.ch))
  stage.buttons.forEach((b, i) => {
    const was = s.buttons[i]! > 0
    if (ev) s.buttons[i] = pressed(b) ? b.ticks : Math.max(0, s.buttons[i]! - 1)
    const now = s.buttons[i]! > 0
    if (now) on.add(b.ch)
    if (ev && was !== now) ev.push({ k: 'button', i, on: now, ...rectMid(b) })
  })
  if (stage.sensors.length) {
    // emitters see this tick's plates, levers and buttons plus last tick's sensors
    const seen = new Set(on)
    // a state off the network has no sensors list; its `on` still says which sensor channels were lit
    if (s.sensors) stage.sensors.forEach((m, i) => s.sensors![i] && seen.add(m.ch))
    else stage.sensors.forEach((m) => s.on.includes(m.ch) && seen.add(m.ch))
    const lit = stage.sensors.map(() => false)
    trace(stage, s, seen, lit)
    stage.sensors.forEach((m, i) => {
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
function fanned(stage: Stage, on: Set<string>, r: Rect): boolean {
  const cx = r.x + r.w / 2
  const bottom = r.y + r.h
  for (const f of stage.fans) if (cx >= f.x && cx < f.x + f.w && bottom > f.y && r.y < f.y + f.h && evalCond(f.when, on)) return true
  return false
}

// one tick of gravity (g: 1 down, -1 up) and of a fan, which always blows up
function liftFall(vy: number, fan: boolean, g: number): number {
  vy += g * GRAVITY * STEP_S
  if (fan && vy > -FAN_MAX_UP) vy = Math.max(-FAN_MAX_UP, vy - FAN_ACCEL * STEP_S)
  return g > 0 ? Math.min(MAX_FALL, vy) : Math.max(-MAX_FALL, vy)
}

// the pool under a body's feet, if any: the tile containing a point just above the feet
function poolUnder(stage: Stage, s: GameState, r: Rect): Tile {
  const y = Math.floor((r.y + r.h - 1) / TILE)
  for (const x of [r.x + 2, r.x + r.w / 2, r.x + r.w - 2]) {
    const t = tileAt(stage, s, Math.floor(x / TILE), y)
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

// adds to `hits` the one-way slabs a body with gravity g lands on when it moves from o to r: only those its
// feet were beyond (above for an upright body, below for a flipped one) before the move
function oneWayHits(stage: Stage, o: { x: number; y: number }, r: Rect, g: number): number {
  if (!lookup(stage).oneWays) return 0
  let n = 0
  const x0 = Math.max(0, Math.floor((r.x + EPS) / TILE))
  const x1 = Math.min(stage.cols - 1, Math.floor((r.x + r.w - EPS) / TILE))
  const y0 = Math.max(0, Math.floor((r.y + EPS) / TILE))
  const y1 = Math.min(stage.rows - 1, Math.floor((r.y + r.h - EPS) / TILE))
  for (let ty = y0; ty <= y1; ty++) {
    for (let tx = x0; tx <= x1; tx++) {
      const ti = ty * stage.cols + tx
      if (stage.tiles[ti] !== T_ONEWAY) continue
      const top = ty * TILE
      if (g > 0 ? o.y + r.h > top + ONEWAY_SLOP : o.y < top + ONEWAY_H - ONEWAY_SLOP) continue
      const slab: Hit = { x: tx * TILE, y: top, w: TILE, h: ONEWAY_H, id: -1, t: T_ONEWAY, ti }
      if (overlaps(r, slab)) {
        hits.push(slab)
        n++
      }
    }
  }
  return n
}

// moves body `id` along y by dy; returns true when it hit something (and snaps flush to it), leaving what it
// came to rest against in `support`. g is the body's gravity: one-way platforms only stop a move along it
function moveY(stage: Stage, s: GameState, id: number, o: { x: number; y: number }, w: number, h: number, dy: number, g: number): boolean {
  if (dy === 0) return false
  const r = { x: o.x, y: o.y + dy, w, h }
  let n = collide(stage, s, r, id)
  if (dy * g > 0) n += oneWayHits(stage, o, r, g)
  if (!n) {
    o.y += dy
    return false
  }
  if (dy > 0) {
    let top = Infinity
    for (const t of hits) top = Math.min(top, t.y)
    o.y = Math.max(o.y, top - h)
    support = hits.filter((t) => t.y <= top + EPS)
  } else {
    let bottom = -Infinity
    for (const t of hits) bottom = Math.max(bottom, t.y + t.h)
    o.y = Math.min(o.y, bottom)
    support = hits.filter((t) => t.y + t.h >= bottom - EPS)
  }
  return true
}

// moves body `id` along x by dx, stepping up pool lips; returns true when it hit something
// (`ground`: the body stands upright on something, so it may step up a lip)
function moveX(stage: Stage, s: GameState, id: number, o: { x: number; y: number }, w: number, h: number, dx: number, ground: boolean): boolean {
  if (dx === 0) return false
  const r = { x: o.x + dx, y: o.y, w, h }
  if (!collide(stage, s, r, id)) {
    o.x += dx
    return false
  }
  // climbing out of a pool: everything in the way tops out within POOL_SINK of our feet
  if (ground) {
    const top = Math.min(...hits.map((t) => t.y))
    const up = o.y + h - top
    if (up > 0 && up <= POOL_SINK + EPS && !collide(stage, s, { x: o.x + dx, y: top - h, w, h }, id)) {
      o.x += dx
      o.y = top - h
      return false
    }
    collide(stage, s, r, id)
  }
  if (dx > 0) o.x = Math.max(o.x, Math.min(...hits.map((t) => t.x)) - w)
  else o.x = Math.min(o.x, Math.max(...hits.map((t) => t.x + t.w)))
  return true
}

// pushes a box sideways by up to dx (it stops flush against whatever's in the way); true when it moved
function pushBox(stage: Stage, s: GameState, i: number, dx: number): boolean {
  const b = s.boxes[i]!
  const x0 = b.x
  moveX(stage, s, BOX0 + i, b, BOX_SIZE, BOX_SIZE, dx, false)
  b.vx = (b.x - x0) / STEP_S
  return b.x !== x0
}

function stepPlayer(stage: Stage, s: GameState, pi: PlayerIndex, bits: number, on: Set<string>, prevBoxVx: number[]): void {
  const p = s.players[pi]
  const ev = s.events
  // everything up and down below is mirrored by the player's own gravity
  const g = p.grav
  // a cannon's shot can't be steered
  const flying = p.fly > 0
  const pressed = bits & ~p.prevIn
  const dirX = flying ? 0 : (bits & IN_RIGHT ? 1 : 0) - (bits & IN_LEFT ? 1 : 0)
  if (dirX) p.face = dirX > 0 ? 1 : -1

  // run
  if (!flying) {
    const target = dirX * RUN_SPEED
    const acc = (p.ground ? RUN_ACCEL : AIR_ACCEL) * STEP_S
    p.vx = p.vx < target ? Math.min(target, p.vx + acc) : Math.max(target, p.vx - acc)
  }

  // jump: buffered presses and coyote time; in mid-air, an arrow's jump if one is held
  if (pressed & IN_UP) p.buffer = JUMP_BUFFER_TICKS
  if (p.ground) p.coyote = COYOTE_TICKS
  if (p.buffer > 0 && p.coyote > 0) {
    p.vy = -g * JUMP_V
    p.buffer = 0
    p.coyote = 0
    p.ground = false
    ev.push({ k: 'jump', p: pi, ...centreOf(p) })
  } else if (p.buffer > 0 && p.air > 0) {
    p.vy = -g * AIR_JUMP_V
    p.buffer = 0
    p.air = 0
    p.fly = 0 // and it takes back control of a cannon shot
    ev.push({ k: 'airjump', p: pi, ...centreOf(p) })
  } else {
    if (p.buffer > 0) p.buffer--
    if (!p.ground && p.coyote > 0) p.coyote--
  }
  // a short hop: letting go while rising (off the ground, an arrow or a trampoline, but not out of a cannon)
  if (!flying && p.prevIn & IN_UP && !(bits & IN_UP) && p.vy * g < 0) p.vy *= JUMP_CUT

  // y moves by the average of this tick's start and end velocity: a full jump peaks at JUMP_V²/2g = 63px
  const v0 = p.vy
  p.vy = liftFall(p.vy, fanned(stage, on, playerRect(p)), g)

  // x, pushing a box that's in the way
  const dx = p.vx * STEP_S
  let hitX = false
  if (dx !== 0) {
    const r = { x: p.x + dx, y: p.y, w: PLAYER_W, h: PLAYER_H }
    let done = false
    if (collide(stage, s, r, pi) === 1 && hits[0]!.id >= BOX0 && hits[0]!.id < GATE0) {
      const bi = hits[0]!.id - BOX0
      const b = s.boxes[bi]!
      // only a box beside us, not one we're under or on
      if (b.y < p.y + PLAYER_H - POOL_SINK - EPS && b.y + BOX_SIZE > p.y + EPS) {
        const amt = Math.sign(dx) * Math.min(Math.abs(dx), PUSH_SPEED * STEP_S)
        // the gap between us and the box first, then push the rest
        const gap = dx > 0 ? b.x - (p.x + PLAYER_W) : p.x - (b.x + BOX_SIZE)
        const pushBy = amt - Math.sign(dx) * Math.max(0, gap)
        if (Math.sign(pushBy) === Math.sign(dx) && pushBox(stage, s, bi, pushBy)) {
          p.x = dx > 0 ? b.x - PLAYER_W : b.x + BOX_SIZE
          p.vx = Math.sign(dx) * Math.min(Math.abs(p.vx), PUSH_SPEED)
          if (!prevBoxVx[bi]) ev.push({ k: 'push', p: pi, ...centreOf(p) })
          done = hitX = true
        }
      }
    }
    // pressing into a wall keeps the run speed, so a jump up beside a ledge pops over its corner
    if (!done && moveX(stage, s, pi, p, PLAYER_W, PLAYER_H, dx, p.ground && g > 0)) {
      hitX = true
      if (Math.sign(dx) !== dirX) p.vx = 0
    }
  }

  // y
  const wasGround = p.ground
  const fall = p.vy * g // speed along gravity
  const hit = moveY(stage, s, pi, p, PLAYER_W, PLAYER_H, ((v0 + p.vy) / 2) * STEP_S, g)
  p.ground = hit && fall >= 0
  if (hit) p.vy = 0
  if (p.ground) {
    // a trampoline bounces whoever comes down with their middle over it, or with nothing else underfoot
    const cx = p.x + PLAYER_W / 2
    let tramp = false
    let other = false
    let mid = false
    for (const h of support) {
      const under = cx >= h.x && cx < h.x + h.w
      if (h.t === T_TRAMP) {
        tramp = true
        if (under) mid = true
      } else if (under) other = true
    }
    if (tramp && (mid || !other)) {
      const v = Math.min(TRAMP_MAX_V, Math.max(TRAMP_MIN_V, fall * TRAMP_GAIN + TRAMP_ADD))
      p.vy = -g * v
      p.ground = false
      p.coyote = 0
      p.buffer = 0
      ev.push({ k: 'bounce', p: pi, ...centreOf(p), v })
    } else {
      // landing loses an arrow's jump
      p.air = 0
      // crumbling blocks underfoot begin to go
      for (const h of support) {
        if (h.t !== T_CRUMBLE) continue
        const i = lookup(stage).crumbleAt.get(h.ti)
        if (i === undefined || s.crumble[i] !== 0) continue
        s.crumble[i] = 1
        ev.push({ k: 'shake', tile: h.ti, x: h.x + TILE / 2, y: h.y + TILE / 2 })
      }
      if (!wasGround) {
        if (fall > LAND_VY) ev.push({ k: 'land', p: pi, ...centreOf(p) })
        // holding jump through a landing hops again
        if (bits & IN_UP) p.buffer = Math.max(p.buffer, 1)
      }
    }
  }
  // a shot ends on anything it hits
  if (p.fly > 0) p.fly = hit || hitX ? 0 : p.fly - 1
  p.prevIn = bits
}

// where a body caught inside a light-only or dark-only block that has just turned solid ends up: pushed out
// against its gravity g to the nearest tile line with room, at most LIGHT_EJECT_TILES away. the y it should
// have (its own when it isn't caught at all), or null when there is nowhere to go
function ejectY(stage: Stage, s: GameState, r: Rect, id: number, g: number): number | null {
  collide(stage, s, r, id)
  if (!hits.some((h) => h.t === T_LIGHT || h.t === T_DARK)) return r.y
  const far = LIGHT_EJECT_TILES * TILE + EPS
  if (g > 0) {
    const bottom = r.y + r.h
    for (let ty = Math.floor((bottom - EPS) / TILE); bottom - ty * TILE <= far; ty--) {
      if (!collide(stage, s, { x: r.x, y: ty * TILE - r.h, w: r.w, h: r.h }, id)) return ty * TILE - r.h
    }
  } else {
    for (let ty = Math.ceil((r.y + EPS) / TILE); ty * TILE - r.y <= far; ty++) {
      if (!collide(stage, s, { x: r.x, y: ty * TILE, w: r.w, h: r.h }, id)) return ty * TILE
    }
  }
  return null
}

// turns the lights on or off. (x, y) is where the light event happens. players caught in a block that
// appears are pushed out or crushed; a box with nowhere to go stays stuck in it until the lights change back
function setLit(stage: Stage, s: GameState, lit: boolean, x: number, y: number): void {
  if (s.lit === lit) return
  s.lit = lit
  s.events.push({ k: 'light', on: lit, x, y })
  for (const pi of [0, 1] as const) {
    const p = s.players[pi]
    // a player in a cannon is safe from everything
    if (!p.alive || p.cannon >= 0) continue
    const y1 = ejectY(stage, s, playerRect(p), pi, p.grav)
    if (y1 === null) {
      p.alive = false
      p.inDoor = false
      s.events.push({ k: 'die', p: pi, ...centreOf(p), cause: 'crush' })
    } else if (y1 !== p.y) {
      p.y = y1
      if (p.vy * p.grav > 0) p.vy = 0
    }
  }
  s.boxes.forEach((b, i) => {
    const y1 = ejectY(stage, s, boxRect(b), BOX0 + i, 1)
    if (y1 === null || y1 === b.y) return
    b.y = y1
    b.vy = 0
  })
}

// advances exactly one STEP_S tick. pure: never mutates `s`, returns a new state whose `events` are this tick's only.
// in phase 'dead' it counts DEATH_TICKS then returns respawn(level, s); in 'clear' it counts STAGE_TICKS then
// returns the start of the next stage; in 'won' it only counts phaseT
export function step(level: Level, s0: GameState, input: Inputs): GameState {
  const s = clone(s0)
  const stage = stageOf(level, s)
  if (s.phase === 'dead') {
    s.phaseT++
    return s.phaseT >= DEATH_TICKS ? respawn(level, s) : s
  }
  if (s.phase === 'clear') {
    s.phaseT++
    if (s.phaseT < STAGE_TICKS) return s
    const n = startStage(level, s.stage + 1, { tick: s.tick, deaths: s.deaths, banked: bank(stage, s) })
    n.events = [{ k: 'stage', stage: n.stage }]
    return n
  }
  if (s.phase === 'won') {
    s.phaseT++
    return s
  }
  const ev = s.events
  s.tick++
  s.phaseT++
  const bits = [input[0] | 0, input[1] | 0]

  // 1. clocks
  for (let i = 0; i < stage.crumble.length; i++) {
    const c = s.crumble[i]!
    if (c === 0) continue
    const ti = stage.crumble[i]!
    const tx = ti % stage.cols
    const ty = (ti - tx) / stage.cols
    const at = { tile: ti, x: tx * TILE + TILE / 2, y: ty * TILE + TILE / 2 }
    if (c < CRUMBLE_TICKS + CRUMBLE_GONE_TICKS) {
      s.crumble[i] = c + 1
      if (c === CRUMBLE_TICKS) ev.push({ k: 'break', ...at })
      continue
    }
    // time to grow back, once nothing is in the way
    const r = tileRect(tx, ty)
    if (s.players.some((p) => p.alive && overlaps(playerRect(p), r)) || s.boxes.some((b) => overlaps(boxRect(b), r))) continue
    s.crumble[i] = 0
    ev.push({ k: 'mend', ...at })
  }
  if (s.litT > 0 && --s.litT === 0) setLit(stage, s, !stage.dark, stage.w / 2, stage.h / 2)
  for (let i = 0; i < stage.arrows.length; i++) {
    if (!(s.arrows[i]! > 0)) continue
    s.arrows[i]!--
    if (s.arrows[i] === 0) ev.push({ k: 'arrowback', i, x: stage.arrows[i]!.x, y: stage.arrows[i]!.y })
  }

  // 2. interact
  for (const pi of [0, 1] as const) {
    const p = s.players[pi]
    if (!p.alive || !(bits[pi]! & IN_DOWN) || p.prevIn & IN_DOWN) continue
    const r = playerRect(p)
    const li = stage.levers.findIndex((l) => overlaps(r, l))
    if (li >= 0) {
      s.levers[li] = !s.levers[li]
      ev.push({ k: 'lever', i: li, ...rectMid(stage.levers[li]!) })
      continue
    }
    const la = stage.lamps.findIndex((l) => overlaps(r, l))
    if (la >= 0) {
      const lamp = stage.lamps[la]!
      const at = rectMid(lamp)
      // a timed lamp: the other state from the one the stage began in, for its ticks (a press while it runs
      // starts the time again). a plain one: the other state from now, for good
      if (lamp.ticks > 0) {
        setLit(stage, s, stage.dark, at.x, at.y)
        s.litT = lamp.ticks
      } else {
        setLit(stage, s, !s.lit, at.x, at.y)
        s.litT = 0
      }
      continue
    }
    const mi = stage.mirrors.findIndex((m) => overlaps(r, { x: m.x - TILE / 2, y: m.y - TILE / 2, w: TILE, h: TILE }))
    if (mi >= 0) {
      s.mirrors[mi] = s.mirrors[mi] === '/' ? '\\' : '/'
      ev.push({ k: 'mirror', i: mi, x: stage.mirrors[mi]!.x, y: stage.mirrors[mi]!.y })
    }
  }

  // 3. channels
  const on = channels(stage, s, ev)
  s.on = listOn(on)

  // 4. gates and movers
  stage.gates.forEach((g, i) => {
    const f = s.gates[i]!
    const want = evalCond(g.open, on) ? 1 : 0
    if (f === want) return
    const nf = want > f ? Math.min(1, f + GATE_SPEED * STEP_S) : Math.max(0, f - GATE_SPEED * STEP_S)
    const a = gateRect(stage, s, i)
    s.gates[i] = nf
    const b = gateRect(stage, s, i)
    s.gates[i] = f
    shove(stage, s, GATE0 + i, a, b, () => (s.gates[i] = nf))
  })
  stage.movers.forEach((m, i) => {
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
    const a = moverRect(stage, s, i)
    shove(stage, s, MOVER0 + i, a, { ...a, x: to.x, y: to.y }, () => {
      st.x = to.x
      st.y = to.y
      st.t = nt
      st.dir = nd
    })
  })

  // 5. thin water and ice
  const frost = s.players[1]
  const fc = centreOf(frost)
  stage.thin.forEach((ti, i) => {
    const tx = ti % stage.cols
    const ty = (ti - tx) / stage.cols
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
    stage.ice.forEach((r, i) => {
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
    b.vy = liftFall(b.vy, fanned(stage, on, boxRect(b)), 1)
    if (moveY(stage, s, BOX0 + i, b, BOX_SIZE, BOX_SIZE, ((v0 + b.vy) / 2) * STEP_S, 1)) b.vy = 0
  })

  // 7. players
  for (const pi of [0, 1] as const) if (s.players[pi].alive) stepPlayer(stage, s, pi, bits[pi]!, on, prevBoxVx)

  // 8. pools, gems, portals, doors
  for (const pi of [0, 1] as const) {
    const p = s.players[pi]
    if (!p.alive) continue
    const r = playerRect(p)
    const cause = killer(poolUnder(stage, s, r), pi)
    if (cause) {
      p.alive = false
      p.inDoor = false
      ev.push({ k: 'die', p: pi, ...centreOf(p), cause })
      continue
    }
    const el = pi === 0 ? 'fire' : 'frost'
    stage.gems.forEach((g, gi) => {
      if (s.taken[gi] || g.el !== el) return
      if (!overlaps(r, { x: g.x - GEM_BOX / 2, y: g.y - GEM_BOX / 2, w: GEM_BOX, h: GEM_BOX })) return
      s.taken[gi] = true
      p.gems++
      ev.push({ k: 'gem', p: pi, x: g.x, y: g.y, el })
    })
    // keys go to the team; a closed lock within a pixel of a player takes one and opens for good
    stage.keys.forEach((k, i) => {
      if (s.keyTaken[i] || !overlaps(r, { x: k.x - GEM_BOX / 2, y: k.y - GEM_BOX / 2, w: GEM_BOX, h: GEM_BOX })) return
      s.keyTaken[i] = true
      s.keys++
      ev.push({ k: 'key', p: pi, i, x: k.x, y: k.y })
    })
    if (s.keys > 0) {
      const reach = { x: r.x - 1, y: r.y - 1, w: r.w + 2, h: r.h + 2 }
      stage.locks.forEach((l, i) => {
        if (s.locks[i] || s.keys <= 0 || !overlaps(reach, l)) return
        s.locks[i] = true
        s.keys--
        ev.push({ k: 'unlock', i, ...rectMid(l) })
      })
    }
    // an arrow: only in the air, and only one at a time
    if (!p.ground && p.air === 0) {
      for (let i = 0; i < stage.arrows.length; i++) {
        const a = stage.arrows[i]!
        if (s.arrows[i] !== 0 || !overlaps(r, { x: a.x - ARROW_BOX / 2, y: a.y - ARROW_BOX / 2, w: ARROW_BOX, h: ARROW_BOX })) continue
        p.air = 1
        s.arrows[i] = ARROW_RESPAWN_TICKS
        ev.push({ k: 'arrow', p: pi, i, x: a.x, y: a.y })
        break
      }
    }
    const c = centreOf(p)
    const inPortal = stage.portals.findIndex((q) => inRect(q, c.x, c.y))
    if (inPortal >= 0 && p.portalCd === 0) {
      const o = stage.portals[stage.portals[inPortal]!.other]!
      p.x = o.x + (TILE - PLAYER_W) / 2
      p.y = o.y + o.h - PLAYER_H
      p.portalCd = PORTAL_COOLDOWN_TICKS
      ev.push({ k: 'portal', p: pi, ...centreOf(p) })
    } else if (inPortal < 0 && p.portalCd > 0) p.portalCd-- // the cooldown holds while still standing in a portal
    const d = stage.doors[pi]
    const cx = p.x + PLAYER_W / 2
    const was = p.inDoor
    p.inDoor = p.ground && cx >= d.x && cx <= d.x + d.w && Math.abs(p.y + PLAYER_H - (d.y + d.h)) < 1
    if (p.inDoor && !was) ev.push({ k: 'door', p: pi, x: d.x + d.w / 2, y: d.y + d.h / 2 })
  }

  // 9. checkpoints, then the phase
  // TODO(ember-2:engine): gravity switches and cannons all go in above; this tick order is the engine agent's to extend (and to document in the header)
  const [fire0, frost0] = s.players
  if (fire0.alive && frost0.alive) {
    const safe = (p: Player) => p.ground && p.cannon < 0
    stage.checkpoints.forEach((c, i) => {
      if (i === s.cp || !safe(fire0) || !safe(frost0)) return
      if (!overlaps(playerRect(fire0), c.zone) || !overlaps(playerRect(frost0), c.zone)) return
      s.cp = i
      // the world as it stands at the end of this tick, flag already lit. s is done changing below unless the
      // stage ends this very tick, and then the snapshot is never used
      const { saved: _old, events: _ev, ...snap } = clone(s)
      s.saved = snap
      ev.push({ k: 'checkpoint', i, x: c.x, y: c.y })
    })
  }
  if (!s.players[0].alive || !s.players[1].alive) {
    s.phase = 'dead'
    s.phaseT = 0
  } else if (s.players[0].inDoor && s.players[1].inDoor) {
    s.phaseT = 0
    if (s.stage + 1 < level.stages.length) {
      s.phase = 'clear'
      ev.push({ k: 'clear', stage: s.stage })
    } else {
      s.phase = 'won'
      ev.push({ k: 'win' })
    }
  }
  return s
}

// the banked gems plus this stage's
function bank(stage: Stage, s: GameState): [number, number] {
  const out: [number, number] = [s.banked[0], s.banked[1]]
  stage.gems.forEach((g, i) => s.taken[i] && out[g.el === 'fire' ? 0 : 1]++)
  return out
}

// gems over every stage so far against the level's total. stars: 1 for finishing, +1 for every gem, +1 for secs <= par
export function scoreOf(level: Level, s: GameState): Score {
  const secs = s.tick / STEP_HZ
  const gems = bank(stageOf(level, s), s)
  const total: [number, number] = [level.gemTotals[0], level.gemTotals[1]]
  const won = s.phase === 'won'
  const all = gems[0] === total[0] && gems[1] === total[1]
  const stars = won ? 1 + (all ? 1 : 0) + (secs <= level.def.par ? 1 : 0) : 0
  return { secs, gems, total, stars }
}
