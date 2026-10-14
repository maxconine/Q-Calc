import { otherHello } from './hello'
import { LEVELS } from './emberLevels'
import {
  type DeathCause,
  type Element,
  type GameEvent,
  type GameState,
  type Level,
  type Phase,
  type Player,
  type Stage,
} from './emberTypes'

// ember & frost over the peer link, as json text. the host is authoritative: it streams state at STATE_HZ,
// the guest sends its keys when they change. every message may cross a relay that charges per request and
// adds 20 to 120 ms each way, so there is one state message per 1/30 s and nothing else per frame, and it is small.
//
// a state message carries the game in two parts:
//   g, the FAST part, in every message: the clock, the phase, both players, boxes, movers, gates, and whichever
//      timers are running (buttons, ice, frozen water, arrows, crumbling blocks, cannons, the timed lamp).
//      whatever is at rest is left out: trailing zeros are cut, timers go as [index, value] pairs
//   k, the SLOW part, only when it changed (twice running, so one lost message costs nothing), on a key frame
//      every KEY_EVERY messages, and when the guest says it's missing: the level and stage, deaths, gems taken
//      and banked, levers, mirrors, lights, keys, locks, the checkpoint, the channels that are on, and each
//      player's gravity and gem count. bit sets go as hex.
// r numbers the slow part: a message whose r the guest doesn't hold can't be read, so the guest keeps it aside
// and says which r it has in its next key message; the host answers with the slow part again.
// e is what happened since the last message and q the batch before that, so one lost message loses no event.
// GameState.saved never crosses: only the host respawns. plates and sensors don't either: the engine refills them.
// what arrives is checked twice: decodeEmber for shape and range, joinState against the stage it claims to be in

// both sides must speak the same wire. goes in the hello beside EMBER_VERSION; bump when a message changes
export const WIRE_VERSION = 1

// nothing from the other side for this long and the game is over
export const SILENCE_MS = 3000
export const STATE_HZ = 30
// the slow part goes out at least this often, in state messages (2 s)
export const KEY_EVERY = 60
// ...and in this many messages running after it changes
export const SLOW_REPEAT = 2

// what the other side did, for the small note line
export type EmberNote = 'play' | 'retry' | 'restart' | 'next' | 'menu'

// a guest key change: the guest frame it starts at, and the held bits
export type KeyChange = [frame: number, bits: number]

// the fast part, as it travels
export type Fast = {
  T: number // tick
  pt: number // phaseT
  ph?: number // phase, when not 'play'
  // x y vx vy flags prevIn coyote buffer portalCd air cannon+1 fly cannonCd; flags: 1 ground, 2 facing right, 4 alive, 8 in door
  p: [number[], number[]]
  bt?: number[] // buttons
  gt?: number[] // gates
  mv?: number[][] // per mover: t, dir, and x, y when it isn't where t puts it
  bx?: number[][] // per box: x, y, vx, vy
  ic?: number[] // ice
  th?: number[] // frozen thin water: index, ticks, index, ticks...
  ar?: number[] // arrows away
  cr?: number[] // crumbling blocks
  cn?: number[] // loaded cannons
  lt?: number // litT
}

// the slow part, as it travels
export type Slow = {
  l: number // level
  st?: number // stage
  d?: number // deaths
  bk?: [number, number] // banked
  gm?: string // gems taken
  lv?: string // levers
  mi?: string // mirrors turned to '\'
  dk?: 1 // the lights are off
  ky?: number // keys carried
  kt?: string // keys taken
  lk?: string // locks opened
  cp?: number // the checkpoint taken
  on?: string // channels on, as bits over channelsOf(stage)
  ox?: string[] // channels on that the stage doesn't name (none, today)
  pg?: [number, number] // each player's gems
  gv?: number // 1: ember is flipped, 2: frost is
}

export type StateMsg = {
  t: 's'
  a: number // the attempt
  f: number // the host frame this state is after (steps since the attempt began)
  fi: number // ember's held keys
  sl?: [slack: number, frame: number] // the least slack of the guest keys heard since the last state, and the guest frame that message carried
  r: number // which slow part this state goes with
  k?: Slow
  g: Fast
  e: GameEvent[] // events since the last state message
  q?: [frame: number, events: GameEvent[]] // the message before this one: its f and its e
}

export type EmberMsg =
  // ok: the sender already heard the other side's hi
  | { t: 'hi'; v: number; w: number; g?: 'ember'; ok?: boolean }
  | { t: 'bye' }
  // host: the select screen. l is the shared highlight
  | { t: 'sel'; l: number }
  // host: the authoritative state
  | StateMsg
  // guest: its keys for attempt a, sent at guest frame f. i: the last few changes, so a dropped message heals.
  // r: the slow part it holds (absent: none)
  | { t: 'k'; a: number; f: number; i: KeyChange[]; r?: number }
  // guest asks, the host decides
  | { t: 'pick'; l: number }
  | { t: 'go'; l: number }
  | { t: 'retry' }
  | { t: 'restart' }
  | { t: 'next' }
  | { t: 'menu' }
  // host: something it did itself
  | { t: 'note'; w: EmberNote }

// ---- limits for text from another machine

// a busy frame on the biggest stage is about 1 kB
export const MAX_TEXT = 16_000
const MAX_FRAME = 1e9
const MAX_COUNT = 1e9
const MAX_TICKS = 1e5
const MAX_THINGS = 512
const MAX_SPARSE = 1024
const MAX_EVENTS = 64
const MAX_CHANNELS = 128
const MAX_CHANGES = 8
const MAX_TILE = 128 * 72 - 1
const POS = 5000
const VEL = 5000

const PHASES: Phase[] = ['play', 'dead', 'won', 'clear']
const NOTES: EmberNote[] = ['play', 'retry', 'restart', 'next', 'menu']
const CAUSES: DeathCause[] = ['lava', 'water', 'goo', 'crush']
const ELS: Element[] = ['fire', 'frost']
const KINDS = [
  'jump',
  'land',
  'die',
  'door',
  'portal',
  'push',
  'gem',
  'lever',
  'mirror',
  'plate',
  'button',
  'sensor',
  'melt',
  'freeze',
  'thaw',
  'win',
  'airjump',
  'bounce',
  'arrow',
  'key',
  'load',
  'fire',
  'flip',
  'gswitch',
  'arrowback',
  'unlock',
  'checkpoint',
  'light',
  'shake',
  'break',
  'mend',
  'clear',
  'stage',
  'respawn',
] as const
type Kind = (typeof KINDS)[number]

const r1 = (n: number) => Math.round(n * 10) / 10
const r2 = (n: number) => Math.round(n * 100) / 100
const r4 = (n: number) => Math.round(n * 1e4) / 1e4

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)
const int = (v: unknown, lo: number, hi: number): v is number => Number.isInteger(v) && (v as number) >= lo && (v as number) <= hi
const within = (v: unknown, lim: number): v is number => finite(v) && Math.abs(v) <= lim
const isBit = (v: unknown): v is 0 | 1 => v === 0 || v === 1
const levelIndex = (v: unknown): v is number => int(v, 0, LEVELS.length - 1)
const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v)
const only = (o: Record<string, unknown>, keys: ReadonlySet<string>) => Object.keys(o).every((k) => keys.has(k))

// ---- packing

// trailing zeros are left off; the stage says how long the list really is
function cut(a: number[], keep = 0): number[] {
  let n = a.length
  while (n > keep && !a[n - 1]) n--
  return n === a.length ? a : a.slice(0, n)
}

// a list that is mostly zero, as index, value, index, value...
function sparse(a: number[]): number[] {
  const out: number[] = []
  a.forEach((v, i) => v && out.push(i, v))
  return out
}

const HEX = '0123456789abcdef'
// booleans as hex, four to a digit, lowest bit first, trailing zeros left off
function mask(bs: boolean[]): string {
  let out = ''
  for (let i = 0; i < bs.length; i += 4) out += HEX[(bs[i] ? 1 : 0) | (bs[i + 1] ? 2 : 0) | (bs[i + 2] ? 4 : 0) | (bs[i + 3] ? 8 : 0)]
  return out.replace(/0+$/, '')
}

const moverAt = (m: Stage['movers'][number], t: number) => ({ x: m.x + (m.to.x - m.x) * t, y: m.y + (m.to.y - m.y) * t })

const channelCache = new WeakMap<Stage, string[]>()
// every channel a stage can turn on, sorted: GameState.on goes as bits over this list
export function channelsOf(stage: Stage): string[] {
  let list = channelCache.get(stage)
  if (!list) {
    const all = [...stage.plates, ...stage.levers, ...stage.buttons, ...stage.sensors].map((t) => t.ch)
    list = [...new Set(all)].sort()
    channelCache.set(stage, list)
  }
  return list
}

function packPlayer(p: Player): number[] {
  const flags = (p.ground ? 1 : 0) | (p.face === 1 ? 2 : 0) | (p.alive ? 4 : 0) | (p.inDoor ? 8 : 0)
  return cut([r2(p.x), r2(p.y), r1(p.vx), r1(p.vy), flags, p.prevIn, p.coyote, p.buffer, p.portalCd, p.air, p.cannon + 1, p.fly, p.cannonCd], 5)
}

// what changes most ticks
export function fastOf(stage: Stage, s: GameState): Fast {
  const g: Fast = { T: s.tick, pt: s.phaseT, p: [packPlayer(s.players[0]), packPlayer(s.players[1])] }
  if (s.phase !== 'play') g.ph = PHASES.indexOf(s.phase)
  const some = (a: number[]) => (a.length ? a : undefined)
  const bt = some(cut(s.buttons))
  const gt = some(cut(s.gates.map(r4)))
  const ic = some(cut(s.ice.map(r4)))
  const th = some(sparse(s.thin))
  const ar = some(sparse(s.arrows))
  const cr = some(sparse(s.crumble))
  const cn = some(sparse(s.cannons))
  if (bt) g.bt = bt
  if (gt) g.gt = gt
  if (s.movers.length) {
    g.mv = s.movers.map((m, i) => {
      const t = r4(m.t)
      const def = stage.movers[i]
      const at = def ? moverAt(def, t) : null
      const there = at && Math.abs(at.x - m.x) < 0.005 && Math.abs(at.y - m.y) < 0.005
      return there ? (m.dir === 1 ? [t] : [t, m.dir]) : [t, m.dir, r2(m.x), r2(m.y)]
    })
  }
  if (s.boxes.length) g.bx = s.boxes.map((b) => cut([r2(b.x), r2(b.y), r1(b.vx), r1(b.vy)], 2))
  if (ic) g.ic = ic
  if (th) g.th = th
  if (ar) g.ar = ar
  if (cr) g.cr = cr
  if (cn) g.cn = cn
  if (s.litT) g.lt = s.litT
  return g
}

// what changes now and then. two states with the same slow part give the same JSON, key for key
export function slowOf(l: number, stage: Stage, s: GameState): Slow {
  const k: Slow = { l }
  if (s.stage) k.st = s.stage
  if (s.deaths) k.d = s.deaths
  if (s.banked[0] || s.banked[1]) k.bk = [s.banked[0], s.banked[1]]
  const gm = mask(s.taken)
  const lv = mask(s.levers)
  const mi = mask(s.mirrors.map((m) => m === '\\'))
  const kt = mask(s.keyTaken)
  const lk = mask(s.locks)
  if (gm) k.gm = gm
  if (lv) k.lv = lv
  if (mi) k.mi = mi
  if (!s.lit) k.dk = 1
  if (s.keys) k.ky = s.keys
  if (kt) k.kt = kt
  if (lk) k.lk = lk
  if (s.cp >= 0) k.cp = s.cp
  const names = channelsOf(stage)
  const on = mask(names.map((c) => s.on.includes(c)))
  const ox = s.on.filter((c) => !names.includes(c))
  if (on) k.on = on
  if (ox.length) k.ox = ox
  const [a, b] = s.players
  if (a.gems || b.gems) k.pg = [a.gems, b.gems]
  const gv = (a.grav === -1 ? 1 : 0) | (b.grav === -1 ? 2 : 0)
  if (gv) k.gv = gv
  return k
}

// ---- checking what arrived: shape and range here, sizes in joinState

function list<T>(v: unknown, each: (x: unknown) => T | null, max = MAX_THINGS): T[] | null {
  if (v === undefined) return []
  if (!Array.isArray(v) || v.length > max) return null
  const out: T[] = []
  for (const x of v) {
    const t = each(x)
    if (t === null) return null
    out.push(t)
  }
  return out
}

const frac = (v: unknown) => (finite(v) && v >= 0 && v <= 1 ? v : null)
const ticks = (v: unknown) => (int(v, 0, MAX_TICKS) ? v : null)

// x y vx vy flags prevIn coyote buffer portalCd air cannon+1 fly cannonCd
const PLAYER_MAX = [POS, POS, VEL, VEL, 15, 15, MAX_TICKS, MAX_TICKS, MAX_TICKS, 1, MAX_THINGS, MAX_TICKS, MAX_TICKS]

function checkPlayer(v: unknown): number[] | null {
  if (!Array.isArray(v) || v.length < 5 || v.length > PLAYER_MAX.length) return null
  for (let i = 0; i < v.length; i++) if (i < 4 ? !within(v[i], PLAYER_MAX[i]!) : !int(v[i], 0, PLAYER_MAX[i]!)) return null
  return v as number[]
}

// index, value pairs: indices rising, values whole and above zero
function checkSparse(v: unknown): number[] | null {
  if (v === undefined) return []
  if (!Array.isArray(v) || v.length % 2 || v.length > MAX_SPARSE * 2) return null
  let last = -1
  for (let i = 0; i < v.length; i += 2) {
    if (!int(v[i], last + 1, MAX_TILE) || !int(v[i + 1], 1, MAX_TICKS)) return null
    last = v[i]
  }
  return v as number[]
}

const FAST_KEYS = new Set(['T', 'pt', 'ph', 'p', 'bt', 'gt', 'mv', 'bx', 'ic', 'th', 'ar', 'cr', 'cn', 'lt'])

function checkFast(o: unknown): Fast | null {
  if (!isObj(o) || !only(o, FAST_KEYS)) return null
  if (!int(o.T, 0, MAX_COUNT) || !int(o.pt, 0, MAX_COUNT)) return null
  if (o.ph !== undefined && !int(o.ph, 1, PHASES.length - 1)) return null
  if (o.lt !== undefined && !int(o.lt, 1, MAX_TICKS)) return null
  if (!Array.isArray(o.p) || o.p.length !== 2) return null
  const a = checkPlayer(o.p[0])
  const b = checkPlayer(o.p[1])
  const bt = list(o.bt, ticks)
  const gt = list(o.gt, frac)
  const ic = list(o.ic, frac)
  const mv = list(o.mv, (m) => {
    if (!Array.isArray(m) || (m.length !== 1 && m.length !== 2 && m.length !== 4)) return null
    if (frac(m[0]) === null || (m.length > 1 && m[1] !== 1 && m[1] !== -1)) return null
    if (m.length === 4 && (!within(m[2], POS) || !within(m[3], POS))) return null
    return m as number[]
  })
  const bx = list(o.bx, (m) => {
    if (!Array.isArray(m) || m.length < 2 || m.length > 4) return null
    return m.every((n, i) => within(n, i < 2 ? POS : VEL)) ? (m as number[]) : null
  })
  const th = checkSparse(o.th)
  const ar = checkSparse(o.ar)
  const cr = checkSparse(o.cr)
  const cn = checkSparse(o.cn)
  if (!a || !b || !bt || !gt || !ic || !mv || !bx || !th || !ar || !cr || !cn) return null
  const g: Fast = { T: o.T, pt: o.pt, p: [a, b] }
  if (o.ph !== undefined) g.ph = o.ph
  if (bt.length) g.bt = bt
  if (gt.length) g.gt = gt
  if (mv.length) g.mv = mv
  if (bx.length) g.bx = bx
  if (ic.length) g.ic = ic
  if (th.length) g.th = th
  if (ar.length) g.ar = ar
  if (cr.length) g.cr = cr
  if (cn.length) g.cn = cn
  if (o.lt !== undefined) g.lt = o.lt
  return g
}

const SLOW_KEYS = new Set(['l', 'st', 'd', 'bk', 'gm', 'lv', 'mi', 'dk', 'ky', 'kt', 'lk', 'cp', 'on', 'ox', 'pg', 'gv'])
const isMask = (v: unknown): v is string | undefined => v === undefined || (typeof v === 'string' && v.length <= MAX_THINGS / 4 && /^[0-9a-f]+$/.test(v))
const pairOf = (v: unknown, max: number): v is [number, number] | undefined =>
  v === undefined || (Array.isArray(v) && v.length === 2 && int(v[0], 0, max) && int(v[1], 0, max))

function checkSlow(o: unknown): Slow | null {
  if (!isObj(o) || !only(o, SLOW_KEYS) || !levelIndex(o.l)) return null
  const num = (v: unknown, lo: number, hi: number): v is number | undefined => v === undefined || int(v, lo, hi)
  if (!num(o.st, 1, 63) || !num(o.d, 1, MAX_COUNT) || !num(o.ky, 1, MAX_THINGS) || !num(o.cp, 0, MAX_THINGS) || !num(o.gv, 1, 3)) return null
  if (o.dk !== undefined && o.dk !== 1) return null
  if (!isMask(o.gm) || !isMask(o.lv) || !isMask(o.mi) || !isMask(o.kt) || !isMask(o.lk) || !isMask(o.on)) return null
  if (!pairOf(o.bk, MAX_COUNT) || !pairOf(o.pg, MAX_THINGS)) return null
  const ox = list(o.ox, (c) => (typeof c === 'string' && c.length <= 64 ? c : null), MAX_CHANNELS)
  if (!ox) return null
  // rebuilt in slowOf's own order, so the same slow part always reads the same
  const k: Slow = { l: o.l }
  if (o.st !== undefined) k.st = o.st
  if (o.d !== undefined) k.d = o.d
  if (o.bk) k.bk = [o.bk[0], o.bk[1]]
  if (o.gm !== undefined) k.gm = o.gm
  if (o.lv !== undefined) k.lv = o.lv
  if (o.mi !== undefined) k.mi = o.mi
  if (o.dk) k.dk = 1
  if (o.ky !== undefined) k.ky = o.ky
  if (o.kt !== undefined) k.kt = o.kt
  if (o.lk !== undefined) k.lk = o.lk
  if (o.cp !== undefined) k.cp = o.cp
  if (o.on !== undefined) k.on = o.on
  if (ox.length) k.ox = ox
  if (o.pg) k.pg = [o.pg[0], o.pg[1]]
  if (o.gv !== undefined) k.gv = o.gv
  return k
}

// ---- a state out of its two parts, against the stage it's for. null when anything doesn't fit:
// the engine and the renderer index the stage's arrays with what this returns

function unmask(v: string | undefined, len: number): boolean[] | null {
  const out: boolean[] = Array.from({ length: len }, () => false)
  if (!v) return out
  for (let i = 0; i < v.length; i++) {
    const n = HEX.indexOf(v[i]!)
    for (let j = 0; j < 4; j++) {
      if (!((n >> j) & 1)) continue
      if (i * 4 + j >= len) return null
      out[i * 4 + j] = true
    }
  }
  return out
}

function fill(v: number[] | undefined, len: number): number[] | null {
  if (!v) return Array.from({ length: len }, () => 0)
  if (v.length > len) return null
  return [...v, ...Array.from({ length: len - v.length }, () => 0)]
}

function unsparse(v: number[] | undefined, len: number): number[] | null {
  const out = Array.from({ length: len }, () => 0)
  if (!v) return out
  for (let i = 0; i < v.length; i += 2) {
    if (v[i]! >= len) return null
    out[v[i]!] = v[i + 1]!
  }
  return out
}

function joinPlayer(a: number[], grav: 1 | -1, gems: number, cannons: number): Player | null {
  const at = (i: number) => a[i] ?? 0
  const flags = at(4)
  if (at(10) > cannons) return null
  return {
    x: at(0),
    y: at(1),
    vx: at(2),
    vy: at(3),
    ground: !!(flags & 1),
    face: flags & 2 ? 1 : -1,
    alive: !!(flags & 4),
    inDoor: !!(flags & 8),
    coyote: at(6),
    buffer: at(7),
    prevIn: at(5),
    portalCd: at(8),
    gems,
    grav,
    air: at(9),
    cannon: at(10) - 1,
    fly: at(11),
    cannonCd: at(12),
  }
}

export function joinState(level: Level, k: Slow, g: Fast): GameState | null {
  const stage = level.stages[k.st ?? 0]
  if (!stage) return null
  const gv = k.gv ?? 0
  const a = joinPlayer(g.p[0], gv & 1 ? -1 : 1, k.pg?.[0] ?? 0, stage.cannons.length)
  const b = joinPlayer(g.p[1], gv & 2 ? -1 : 1, k.pg?.[1] ?? 0, stage.cannons.length)
  const taken = unmask(k.gm, stage.gems.length)
  const levers = unmask(k.lv, stage.levers.length)
  const mirrors = unmask(k.mi, stage.mirrors.length)
  const keyTaken = unmask(k.kt, stage.keys.length)
  const locks = unmask(k.lk, stage.locks.length)
  const names = channelsOf(stage)
  const on = unmask(k.on, names.length)
  const buttons = fill(g.bt, stage.buttons.length)
  const gates = fill(g.gt, stage.gates.length)
  const ice = fill(g.ic, stage.ice.length)
  const thin = unsparse(g.th, stage.thin.length)
  const arrows = unsparse(g.ar, stage.arrows.length)
  const crumble = unsparse(g.cr, stage.crumble.length)
  const cannons = unsparse(g.cn, stage.cannons.length)
  if (!a || !b || !taken || !levers || !mirrors || !keyTaken || !locks || !on) return null
  if (!buttons || !gates || !ice || !thin || !arrows || !crumble || !cannons) return null
  if ((g.mv?.length ?? 0) !== stage.movers.length || (g.bx?.length ?? 0) !== stage.boxes.length) return null
  if ((k.cp ?? -1) >= stage.checkpoints.length) return null
  return {
    tick: g.T,
    phase: PHASES[g.ph ?? 0]!,
    phaseT: g.pt,
    deaths: k.d ?? 0,
    stage: stage.index,
    banked: k.bk ? [k.bk[0], k.bk[1]] : [0, 0],
    players: [a, b],
    taken,
    levers,
    buttons,
    gates,
    movers: (g.mv ?? []).map((m, i) => {
      const t = m[0]!
      const dir = m[1] === -1 ? -1 : 1
      return m.length === 4 ? { x: m[2]!, y: m[3]!, t, dir } : { ...moverAt(stage.movers[i]!, t), t, dir }
    }),
    boxes: (g.bx ?? []).map((m) => ({ x: m[0]!, y: m[1]!, vx: m[2] ?? 0, vy: m[3] ?? 0 })),
    ice,
    thin,
    mirrors: mirrors.map((m) => (m ? '\\' : '/')),
    lit: !k.dk,
    litT: g.lt ?? 0,
    arrows,
    crumble,
    keys: k.ky ?? 0,
    keyTaken,
    locks,
    cannons,
    cp: k.cp ?? -1,
    on: [...names.filter((_, i) => on[i]), ...(k.ox ?? [])].sort(),
    events: [],
  }
}

// ---- events

const bit = (b: boolean) => (b ? 1 : 0)
const px = Math.round

function packEvent(e: GameEvent): number[] {
  const k = KINDS.indexOf(e.k)
  switch (e.k) {
    case 'win':
      return [k]
    case 'clear':
    case 'stage':
      return [k, e.stage]
    case 'respawn':
      return [k, e.cp]
    case 'gem':
      return [k, e.p, px(e.x), px(e.y), ELS.indexOf(e.el)]
    case 'bounce':
      return [k, e.p, px(e.x), px(e.y), Math.round(e.v)]
    case 'flip':
      return [k, e.p, px(e.x), px(e.y), e.grav]
    case 'arrow':
    case 'key':
    case 'load':
    case 'fire':
      return [k, e.p, e.i, px(e.x), px(e.y)]
    case 'light':
      return [k, bit(e.on), px(e.x), px(e.y)]
    case 'lever':
    case 'mirror':
    case 'melt':
    case 'gswitch':
    case 'arrowback':
    case 'unlock':
    case 'checkpoint':
      return [k, e.i, px(e.x), px(e.y)]
    case 'plate':
    case 'button':
    case 'sensor':
      return [k, e.i, bit(e.on), px(e.x), px(e.y)]
    case 'freeze':
    case 'thaw':
    case 'shake':
    case 'break':
    case 'mend':
      return [k, e.tile, px(e.x), px(e.y)]
    default:
      return [k, e.p, px(e.x), px(e.y), ...(e.cause ? [CAUSES.indexOf(e.cause)] : [])]
  }
}

function unpackEvent(v: unknown): GameEvent | null {
  if (!Array.isArray(v) || !v.length || v.length > 5 || !v.every(finite)) return null
  const n = v as number[]
  if (!int(n[0], 0, KINDS.length - 1)) return null
  const k: Kind = KINDS[n[0]]!
  const xy = (i: number) => within(n[i], POS) && within(n[i + 1], POS)
  const idx = (i: number) => int(n[i], 0, MAX_THINGS)
  switch (k) {
    case 'win':
      return n.length === 1 ? { k } : null
    case 'clear':
    case 'stage':
      return n.length === 2 && int(n[1], 0, 63) ? { k, stage: n[1]! } : null
    case 'respawn':
      return n.length === 2 && int(n[1], -1, MAX_THINGS) ? { k, cp: n[1]! } : null
    case 'bounce':
      return n.length === 5 && isBit(n[1]) && xy(2) && within(n[4], VEL) ? { k, p: n[1], x: n[2]!, y: n[3]!, v: n[4]! } : null
    case 'flip':
      return n.length === 5 && isBit(n[1]) && xy(2) && (n[4] === 1 || n[4] === -1) ? { k, p: n[1], x: n[2]!, y: n[3]!, grav: n[4] } : null
    case 'arrow':
    case 'key':
    case 'load':
    case 'fire':
      return n.length === 5 && isBit(n[1]) && idx(2) && xy(3) ? { k, p: n[1], i: n[2]!, x: n[3]!, y: n[4]! } : null
    case 'light':
      return n.length === 4 && isBit(n[1]) && xy(2) ? { k, on: n[1] === 1, x: n[2]!, y: n[3]! } : null
    case 'gem': {
      const el = ELS[n[4]!]
      return n.length === 5 && isBit(n[1]) && xy(2) && el ? { k, p: n[1], x: n[2]!, y: n[3]!, el } : null
    }
    case 'lever':
    case 'mirror':
    case 'melt':
    case 'gswitch':
    case 'arrowback':
    case 'unlock':
    case 'checkpoint':
      return n.length === 4 && idx(1) && xy(2) ? { k, i: n[1]!, x: n[2]!, y: n[3]! } : null
    case 'plate':
    case 'button':
    case 'sensor':
      return n.length === 5 && idx(1) && isBit(n[2]) && xy(3) ? { k, i: n[1]!, on: n[2] === 1, x: n[3]!, y: n[4]! } : null
    case 'freeze':
    case 'thaw':
    case 'shake':
    case 'break':
    case 'mend':
      return n.length === 4 && int(n[1], 0, MAX_TILE) && xy(2) ? { k, tile: n[1]!, x: n[2]!, y: n[3]! } : null
    default: {
      if ((n.length !== 4 && n.length !== 5) || !isBit(n[1]) || !xy(2)) return null
      const cause = n.length === 5 ? CAUSES[n[4]!] : undefined
      if (n.length === 5 && !cause) return null
      return { k, p: n[1], x: n[2]!, y: n[3]!, ...(cause ? { cause } : {}) }
    }
  }
}

// an event's index points at something in this stage, so the panel can look it up without checking
export function eventFits(level: Level, stage: Stage, e: GameEvent): boolean {
  switch (e.k) {
    case 'lever':
      return e.i < stage.levers.length
    case 'mirror':
      return e.i < stage.mirrors.length
    case 'melt':
      return e.i < stage.ice.length
    case 'plate':
      return e.i < stage.plates.length
    case 'button':
      return e.i < stage.buttons.length
    case 'sensor':
      return e.i < stage.sensors.length
    case 'gswitch':
      return e.i < stage.gswitches.length
    case 'arrow':
    case 'arrowback':
      return e.i < stage.arrows.length
    case 'key':
      return e.i < stage.keys.length
    case 'unlock':
      return e.i < stage.locks.length
    case 'load':
    case 'fire':
      return e.i < stage.cannons.length
    case 'checkpoint':
      return e.i < stage.checkpoints.length
    case 'respawn':
      return e.cp < stage.checkpoints.length
    case 'freeze':
    case 'thaw':
    case 'shake':
    case 'break':
    case 'mend':
      return e.tile < stage.tiles.length
    case 'clear':
    case 'stage':
      return e.stage < level.stages.length
    default:
      return true
  }
}

// ---- messages

const STATE_KEYS = new Set(['t', 'a', 'f', 'fi', 'sl', 'r', 'k', 'g', 'e', 'q'])

export function encodeEmber(m: EmberMsg): string {
  if (m.t !== 's') return JSON.stringify(m)
  const { a, f, fi, sl, r, k, g, e, q } = m
  const pack = (es: GameEvent[]) => es.slice(-MAX_EVENTS).map(packEvent)
  return JSON.stringify({
    t: 's',
    a,
    f,
    fi,
    ...(sl ? { sl } : {}),
    r,
    ...(k ? { k } : {}),
    g,
    ...(e.length ? { e: pack(e) } : {}),
    ...(q?.[1].length ? { q: [q[0], pack(q[1])] } : {}),
  })
}

// what can come in: an ember message, or another game's hello from a friend who opened that instead
export type EmberIn = EmberMsg | { t: 'other'; game: string }

// anything malformed is null, never a throw: this is text from another machine
export function decodeEmber(raw: string): EmberIn | null {
  if (typeof raw !== 'string' || raw.length > MAX_TEXT) return null
  let m: unknown
  try {
    m = JSON.parse(raw)
  } catch {
    return null
  }
  if (!isObj(m)) return null
  const o = m
  // a hello without a version is nobody's
  const other = finite(o.v) ? otherHello(o, 'ember') : null
  if (other) return { t: 'other', game: other }
  switch (o.t) {
    case 'hi':
      // read leniently: a hi from another version must still decode, to say so
      return finite(o.v) ? { t: 'hi', v: o.v, w: finite(o.w) ? o.w : 0, ...(o.ok === true ? { ok: true } : {}) } : null
    case 'bye':
    case 'retry':
    case 'restart':
    case 'next':
    case 'menu':
      return { t: o.t }
    case 'pick':
    case 'go':
      return levelIndex(o.l) ? { t: o.t, l: o.l } : null
    case 'sel':
      return levelIndex(o.l) ? { t: 'sel', l: o.l } : null
    case 'note':
      return NOTES.includes(o.w as EmberNote) ? { t: 'note', w: o.w as EmberNote } : null
    case 'k': {
      if (!int(o.a, 0, MAX_COUNT) || !int(o.f, 0, MAX_FRAME)) return null
      if (o.r !== undefined && !int(o.r, 0, MAX_COUNT)) return null
      const i = list(o.i, (c) => (Array.isArray(c) && c.length === 2 && int(c[0], 0, MAX_FRAME) && int(c[1], 0, 15) ? ([c[0], c[1]] as KeyChange) : null), MAX_CHANGES)
      if (!i) return null
      return { t: 'k', a: o.a, f: o.f, i: i.sort((p, q) => p[0] - q[0]), ...(o.r !== undefined ? { r: o.r } : {}) }
    }
    case 's': {
      if (!only(o, STATE_KEYS)) return null
      if (!int(o.a, 0, MAX_COUNT) || !int(o.f, 0, MAX_FRAME) || !int(o.fi, 0, 15) || !int(o.r, 0, MAX_COUNT)) return null
      let sl: [number, number] | undefined
      if (o.sl !== undefined) {
        if (!Array.isArray(o.sl) || o.sl.length !== 2 || !int(o.sl[0], -MAX_FRAME, MAX_FRAME) || !int(o.sl[1], 0, MAX_FRAME)) return null
        sl = [o.sl[0], o.sl[1]]
      }
      const k = o.k === undefined ? undefined : checkSlow(o.k)
      const g = checkFast(o.g)
      const e = list(o.e, unpackEvent, MAX_EVENTS)
      if (k === null || !g || !e) return null
      let q: [number, GameEvent[]] | undefined
      if (o.q !== undefined) {
        if (!Array.isArray(o.q) || o.q.length !== 2 || !int(o.q[0], 0, o.f)) return null
        const qe = list(o.q[1], unpackEvent, MAX_EVENTS)
        if (!qe) return null
        q = [o.q[0], qe]
      }
      return { t: 's', a: o.a, f: o.f, fi: o.fi, ...(sl ? { sl } : {}), r: o.r, ...(k ? { k } : {}), g, e, ...(q ? { q } : {}) }
    }
    default:
      return null
  }
}
