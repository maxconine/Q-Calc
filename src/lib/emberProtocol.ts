import { LEVELS } from './emberLevels'
import {
  COLS,
  ROWS,
  type DeathCause,
  type Element,
  type GameEvent,
  type GameState,
  type Level,
  type Phase,
  type Player,
  type Slant,
} from './emberTypes'

// ember & frost over the peer link, as json text. the host is authoritative: it streams state at STATE_HZ,
// the guest sends its keys when they change. every message may cross a relay that charges per request,
// so state goes as short keys and rounded numbers, and anything empty is left out

// nothing from the other side for this long and the game is over
export const SILENCE_MS = 3000
export const STATE_HZ = 30

// what the other side did, for the small note line
export type EmberNote = 'play' | 'retry' | 'next' | 'menu'

// a guest key change: the guest frame it starts at, and the held bits
export type KeyChange = [frame: number, bits: number]

export type EmberMsg =
  // u: levels unlocked on the sender's side; ok: the sender already heard the other side's hi
  | { t: 'hi'; v: number; u: number; ok?: boolean }
  | { t: 'bye' }
  // host: the select screen. l is the shared highlight
  | { t: 'sel'; l: number; u: number }
  // host: authoritative state of attempt a of level l after frame f (steps since the attempt began).
  // fi: ember's held keys; sl: the least slack of the guest keys heard since the last state, and the
  // guest frame that message carried; e: events since the last state message
  | { t: 's'; l: number; a: number; f: number; fi: number; sl?: [slack: number, frame: number]; g: GameState; e: GameEvent[] }
  // guest: its keys for attempt a, sent at guest frame f. i: the last few changes, so a dropped message heals
  | { t: 'k'; a: number; f: number; i: KeyChange[] }
  // guest asks, the host decides
  | { t: 'pick'; l: number }
  | { t: 'go'; l: number }
  | { t: 'retry' }
  | { t: 'next' }
  | { t: 'menu' }
  // host: something it did itself
  | { t: 'note'; w: EmberNote }

// ---- limits for text from another machine

const MAX_TEXT = 32_000
const MAX_FRAME = 1e9
const MAX_COUNT = 1e9
const MAX_THINGS = 512
const MAX_EVENTS = 128
const MAX_CHANNELS = 128
const MAX_CHANGES = 8
const POS = 10_000
const VEL = 10_000

const PHASES: Phase[] = ['play', 'dead', 'won']
const NOTES: EmberNote[] = ['play', 'retry', 'next', 'menu']
const CAUSES: DeathCause[] = ['lava', 'water', 'goo']
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
] as const
type Kind = (typeof KINDS)[number]

const r1 = (n: number) => Math.round(n * 10) / 10
const r2 = (n: number) => Math.round(n * 100) / 100
const r4 = (n: number) => Math.round(n * 1e4) / 1e4
const bit = (b: boolean) => (b ? 1 : 0)

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)
const int = (v: unknown, lo: number, hi: number): v is number => Number.isInteger(v) && (v as number) >= lo && (v as number) <= hi
const within = (v: unknown, lim: number): v is number => finite(v) && Math.abs(v) <= lim
const isBit = (v: unknown): v is 0 | 1 => v === 0 || v === 1
const levelIndex = (v: unknown): v is number => int(v, 0, LEVELS.length - 1)
const clampUnlocked = (v: unknown) => (finite(v) ? Math.min(Math.max(1, LEVELS.length), Math.max(1, Math.floor(v))) : 1)

// ---- state

// x y vx vy ground face alive inDoor coyote buffer prevIn portalCd gems
const PLAYER_LEN = 13

function packPlayer(p: Player): number[] {
  return [
    r2(p.x),
    r2(p.y),
    r1(p.vx),
    r1(p.vy),
    bit(p.ground),
    p.face,
    bit(p.alive),
    bit(p.inDoor),
    p.coyote,
    p.buffer,
    p.prevIn,
    p.portalCd,
    p.gems,
  ]
}

function unpackPlayer(v: unknown): Player | null {
  if (!Array.isArray(v) || v.length !== PLAYER_LEN) return null
  const [x, y, vx, vy, ground, face, alive, inDoor, coyote, buffer, prevIn, portalCd, gems] = v as unknown[]
  if (!within(x, POS) || !within(y, POS) || !within(vx, VEL) || !within(vy, VEL)) return null
  if (!isBit(ground) || !isBit(alive) || !isBit(inDoor) || (face !== 1 && face !== -1)) return null
  if (!int(coyote, 0, 1e4) || !int(buffer, 0, 1e4) || !int(prevIn, 0, 15) || !int(portalCd, 0, 1e4) || !int(gems, 0, MAX_THINGS)) return null
  return { x, y, vx, vy, ground: ground === 1, face, alive: alive === 1, inDoor: inDoor === 1, coyote, buffer, prevIn, portalCd, gems }
}

// booleans and slants go as strings of 0s and 1s
const flags = (bs: boolean[]) => bs.map((b) => (b ? '1' : '0')).join('')
const unflags = (v: unknown): boolean[] | null => (typeof v === 'string' && /^[01]*$/.test(v) && v.length <= MAX_THINGS ? [...v].map((c) => c === '1') : null)

// empty arrays are left out entirely; most levels have no movers, boxes, fans...
function packState(s: GameState): Record<string, unknown> {
  const o: Record<string, unknown> = { T: s.tick, ph: PHASES.indexOf(s.phase), pt: s.phaseT, d: s.deaths, p: s.players.map(packPlayer) }
  if (s.taken.length) o.gm = flags(s.taken)
  if (s.levers.length) o.lv = flags(s.levers)
  if (s.buttons.length) o.bt = s.buttons
  if (s.gates.length) o.gt = s.gates.map(r4)
  if (s.movers.length) o.mv = s.movers.map((m) => [r2(m.x), r2(m.y), r4(m.t), m.dir])
  if (s.boxes.length) o.bx = s.boxes.map((b) => [r2(b.x), r2(b.y), r1(b.vx), r1(b.vy)])
  if (s.ice.length) o.ic = s.ice.map(r4)
  if (s.thin.length) o.th = s.thin
  if (s.mirrors.length) o.mi = s.mirrors.map((m) => (m === '/' ? '0' : '1')).join('')
  if (s.on.length) o.on = s.on
  return o
}

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
const count = (v: unknown) => (int(v, 0, MAX_COUNT) ? v : null)

function unpackState(v: unknown): GameState | null {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return null
  const o = v as Record<string, unknown>
  if (!int(o.T, 0, MAX_COUNT) || !int(o.pt, 0, MAX_COUNT) || !int(o.d, 0, MAX_COUNT) || !int(o.ph, 0, PHASES.length - 1)) return null
  if (!Array.isArray(o.p) || o.p.length !== 2) return null
  const a = unpackPlayer(o.p[0])
  const b = unpackPlayer(o.p[1])
  if (!a || !b) return null
  const taken = o.gm === undefined ? [] : unflags(o.gm)
  const levers = o.lv === undefined ? [] : unflags(o.lv)
  const buttons = list(o.bt, count)
  const gates = list(o.gt, frac)
  const movers = list(o.mv, (m) => {
    if (!Array.isArray(m) || m.length !== 4) return null
    const [x, y, t, dir] = m as unknown[]
    return within(x, POS) && within(y, POS) && frac(t) !== null && (dir === 1 || dir === -1) ? { x, y, t: t as number, dir: dir as 1 | -1 } : null
  })
  const boxes = list(o.bx, (m) => {
    if (!Array.isArray(m) || m.length !== 4) return null
    const [x, y, vx, vy] = m as unknown[]
    return within(x, POS) && within(y, POS) && within(vx, VEL) && within(vy, VEL) ? { x, y, vx, vy } : null
  })
  const ice = list(o.ic, frac)
  const thin = list(o.th, count, ROWS * COLS)
  const mirrors: Slant[] | null =
    o.mi === undefined ? [] : typeof o.mi === 'string' && /^[01]*$/.test(o.mi) && o.mi.length <= MAX_THINGS ? [...o.mi].map((c) => (c === '0' ? '/' : '\\')) : null
  const on = list(o.on, (c) => (typeof c === 'string' && c.length <= 64 ? c : null), MAX_CHANNELS)
  if (!taken || !levers || !buttons || !gates || !movers || !boxes || !ice || !thin || !mirrors || !on) return null
  return {
    tick: o.T,
    phase: PHASES[o.ph]!,
    phaseT: o.pt,
    deaths: o.d,
    players: [a, b],
    taken,
    levers,
    buttons,
    gates,
    movers,
    boxes,
    ice,
    thin,
    mirrors,
    on,
    events: [],
  }
}

// a decoded state still has to match the level it's for: the renderer indexes level arrays with it
export function fitsLevel(level: Level, s: GameState): boolean {
  return (
    s.taken.length === level.gems.length &&
    s.levers.length === level.levers.length &&
    s.buttons.length === level.buttons.length &&
    s.gates.length === level.gates.length &&
    s.movers.length === level.movers.length &&
    s.boxes.length === level.boxes.length &&
    s.ice.length === level.ice.length &&
    s.thin.length === level.thin.length &&
    s.mirrors.length === level.mirrors.length
  )
}

// ---- events

function packEvent(e: GameEvent): number[] {
  const k = KINDS.indexOf(e.k)
  switch (e.k) {
    case 'win':
      return [k]
    case 'gem':
      return [k, e.p, r1(e.x), r1(e.y), ELS.indexOf(e.el)]
    case 'lever':
    case 'mirror':
    case 'melt':
      return [k, e.i, r1(e.x), r1(e.y)]
    case 'plate':
    case 'button':
    case 'sensor':
      return [k, e.i, bit(e.on), r1(e.x), r1(e.y)]
    case 'freeze':
    case 'thaw':
      return [k, e.tile, r1(e.x), r1(e.y)]
    default:
      return [k, e.p, r1(e.x), r1(e.y), ...(e.cause ? [CAUSES.indexOf(e.cause)] : [])]
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
    case 'gem': {
      const el = ELS[n[4]!]
      return n.length === 5 && isBit(n[1]) && xy(2) && el ? { k, p: n[1], x: n[2]!, y: n[3]!, el } : null
    }
    case 'lever':
    case 'mirror':
    case 'melt':
      return n.length === 4 && idx(1) && xy(2) ? { k, i: n[1]!, x: n[2]!, y: n[3]! } : null
    case 'plate':
    case 'button':
    case 'sensor':
      return n.length === 5 && idx(1) && isBit(n[2]) && xy(3) ? { k, i: n[1]!, on: n[2] === 1, x: n[3]!, y: n[4]! } : null
    case 'freeze':
    case 'thaw':
      return n.length === 4 && int(n[1], 0, ROWS * COLS - 1) && xy(2) ? { k, tile: n[1]!, x: n[2]!, y: n[3]! } : null
    default: {
      if ((n.length !== 4 && n.length !== 5) || !isBit(n[1]) || !xy(2)) return null
      const cause = n.length === 5 ? CAUSES[n[4]!] : undefined
      if (n.length === 5 && !cause) return null
      return { k, p: n[1], x: n[2]!, y: n[3]!, ...(cause ? { cause } : {}) }
    }
  }
}

// an event's index points at something in this level, so the panel can look it up without checking
export function eventFits(level: Level, e: GameEvent): boolean {
  switch (e.k) {
    case 'lever':
      return e.i < level.levers.length
    case 'mirror':
      return e.i < level.mirrors.length
    case 'melt':
      return e.i < level.ice.length
    case 'plate':
      return e.i < level.plates.length
    case 'button':
      return e.i < level.buttons.length
    case 'sensor':
      return e.i < level.sensors.length
    default:
      return true
  }
}

// ---- messages

export function encodeEmber(m: EmberMsg): string {
  if (m.t === 's') {
    const { l, a, f, fi, sl, g, e } = m
    return JSON.stringify({
      t: 's',
      l,
      a,
      f,
      fi,
      ...(sl ? { sl } : {}),
      g: packState(g),
      ...(e.length ? { e: e.slice(-MAX_EVENTS).map(packEvent) } : {}),
    })
  }
  return JSON.stringify(m)
}

// anything malformed is null, never a throw: this is text from another machine
export function decodeEmber(raw: string): EmberMsg | null {
  if (typeof raw !== 'string' || raw.length > MAX_TEXT) return null
  let m: unknown
  try {
    m = JSON.parse(raw)
  } catch {
    return null
  }
  if (!m || typeof m !== 'object' || Array.isArray(m)) return null
  const o = m as Record<string, unknown>
  switch (o.t) {
    case 'hi':
      // u is read leniently: a hi from another version must still decode, to say so
      return finite(o.v) ? { t: 'hi', v: o.v, u: clampUnlocked(o.u), ...(o.ok === true ? { ok: true } : {}) } : null
    case 'bye':
    case 'retry':
    case 'next':
    case 'menu':
      return { t: o.t }
    case 'pick':
    case 'go':
      return levelIndex(o.l) ? { t: o.t, l: o.l } : null
    case 'sel':
      return levelIndex(o.l) ? { t: 'sel', l: o.l, u: clampUnlocked(o.u) } : null
    case 'note':
      return NOTES.includes(o.w as EmberNote) ? { t: 'note', w: o.w as EmberNote } : null
    case 'k': {
      if (!int(o.a, 0, MAX_COUNT) || !int(o.f, 0, MAX_FRAME)) return null
      const i = list(o.i, (c) => (Array.isArray(c) && c.length === 2 && int(c[0], 0, MAX_FRAME) && int(c[1], 0, 15) ? ([c[0], c[1]] as KeyChange) : null), MAX_CHANGES)
      if (!i) return null
      return { t: 'k', a: o.a, f: o.f, i: i.sort((p, q) => p[0] - q[0]) }
    }
    case 's': {
      if (!levelIndex(o.l) || !int(o.a, 0, MAX_COUNT) || !int(o.f, 0, MAX_FRAME) || !int(o.fi, 0, 15)) return null
      let sl: [number, number] | undefined
      if (o.sl !== undefined) {
        if (!Array.isArray(o.sl) || o.sl.length !== 2 || !int(o.sl[0], -MAX_FRAME, MAX_FRAME) || !int(o.sl[1], 0, MAX_FRAME)) return null
        sl = [o.sl[0], o.sl[1]]
      }
      const g = unpackState(o.g)
      const e = list(o.e, unpackEvent, MAX_EVENTS)
      if (!g || !e) return null
      return { t: 's', l: o.l, a: o.a, f: o.f, fi: o.fi, ...(sl ? { sl } : {}), g, e }
    }
    default:
      return null
  }
}
