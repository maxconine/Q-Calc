import { CH_LIT, GLOW_EMBER, GLOW_FROST, GLOW_SMALL, PLAYER_H, PLAYER_W, POOL_SINK, T_LAVA, TILE } from '../lib/emberTypes'
import type { Camera, Cond, GameState, Rect, Stage } from '../lib/emberTypes'

// ember & frost's little helpers for the renderer: colour maths, channel colours, and a fixed particle pool.
// nothing here touches the DOM, so it can be tested in node

export type RGB = [number, number, number]

// '#rgb', '#rrggbb', 'rgb(…)' and 'rgba(…)'; anything else (color-mix, names) falls back
export function parseColor(s: string, fallback: RGB): RGB {
  const t = s.trim().toLowerCase()
  if (t.startsWith('#')) {
    const h = t.slice(1)
    if (/^[0-9a-f]{3,4}$/.test(h)) return [parseInt(h[0] + h[0], 16), parseInt(h[1] + h[1], 16), parseInt(h[2] + h[2], 16)]
    if (/^[0-9a-f]{6}([0-9a-f]{2})?$/.test(h)) return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)]
    return fallback
  }
  const m = /^rgba?\(([^)]*)\)$/.exec(t)
  if (m) {
    const parts = m[1].split(/[\s,/]+/).filter(Boolean).map(Number)
    if (parts.length >= 3 && parts.slice(0, 3).every((n) => Number.isFinite(n))) return [parts[0], parts[1], parts[2]]
  }
  return fallback
}

export function mixRgb(a: RGB, b: RGB, t: number): RGB {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

export function css(c: RGB, alpha = 1): string {
  const r = Math.round(c[0])
  const g = Math.round(c[1])
  const b = Math.round(c[2])
  return alpha >= 1 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${Math.max(0, alpha).toFixed(3)})`
}

// relative brightness 0..1, enough to tell a dark theme from a light one
export function luma(c: RGB): number {
  return (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255
}

// small stable string hash (fnv-1a), so the same channel name gets the same colour on every machine
export function hashStr(s: string): number {
  let h = 0x811c9dc5
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return h >>> 0
}

// a cheap deterministic 0..1 noise from an integer, for per-tile variety that doesn't flicker between frames
export function hash01(n: number): number {
  let h = Math.imul(n ^ 0x9e3779b9, 0x85ebca6b)
  h ^= h >>> 13
  h = Math.imul(h, 0xc2b2ae35)
  h ^= h >>> 16
  return (h >>> 0) / 4294967296
}

// channel colours: picked to stay clear of fire orange, frost cyan, lava, water and goo so a link never reads as an element
export const CHANNEL_COLORS = ['#9b7bff', '#ff5fa2', '#1fb39a', '#e3b21c', '#5b7cff', '#d05ad6', '#8fa63a'] as const

export function channelIndex(name: string): number {
  return hashStr(name.replace(/^!/, '')) % CHANNEL_COLORS.length
}

export function channelColor(name: string): string {
  return CHANNEL_COLORS[channelIndex(name)]
}

// the channel names a condition mentions, '!' stripped, in order
export function condChannels(c: Cond | undefined): string[] {
  if (c === undefined) return []
  const list = typeof c === 'string' ? [c] : c
  return list.map((n) => n.replace(/^!/, ''))
}

// whether a condition holds for the channels that are on; no condition means always. `lit` is the lights'
// own channel, which a state never lists in `on`
export function condHolds(c: Cond | undefined, on: readonly string[], lit = false): boolean {
  if (c === undefined) return true
  const list = typeof c === 'string' ? [c] : c
  for (const n of list) {
    const neg = n.startsWith('!')
    const name = neg ? n.slice(1) : n
    if ((name === CH_LIT ? lit : on.includes(name)) === neg) return false
  }
  return true
}

// ---------------------------------------------------------------- the camera on a canvas

// world px to canvas px: cx = wx * k + ox. the offsets are whole pixels so tiles and the baked static layer
// land on the pixel grid and stay crisp while the camera glides
export type View = { k: number; ox: number; oy: number }

export function viewFor(cam: Camera, width: number, shakeX = 0, shakeY = 0): View {
  const k = width / cam.w
  return { k, ox: Math.round((shakeX - cam.x) * k), oy: Math.round((shakeY - cam.y) * k) }
}

export function toCanvas(v: View, x: number, y: number): [number, number] {
  return [x * v.k + v.ox, y * v.k + v.oy]
}

export function toWorld(v: View, cx: number, cy: number): [number, number] {
  return [(cx - v.ox) / v.k, (cy - v.oy) / v.k]
}

// the tiles a camera can see, x1 / y1 exclusive, kept inside the stage. pad is in tiles
export function visibleTiles(cam: Camera, stage: { cols: number; rows: number }, pad = 0): { x0: number; y0: number; x1: number; y1: number } {
  const x0 = Math.max(0, Math.floor(cam.x / TILE) - pad)
  const y0 = Math.max(0, Math.floor(cam.y / TILE) - pad)
  const x1 = Math.min(stage.cols, Math.ceil((cam.x + cam.w) / TILE) + pad)
  const y1 = Math.min(stage.rows, Math.ceil((cam.y + cam.h) / TILE) + pad)
  return { x0, y0, x1: Math.max(x0, x1), y1: Math.max(y0, y1) }
}

// whether a world rect (grown by pad px) touches what the camera shows
export function inView(cam: Camera, x: number, y: number, w: number, h: number, pad = 0): boolean {
  return x + w + pad >= cam.x && x - pad <= cam.x + cam.w && y + h + pad >= cam.y && y - pad <= cam.y + cam.h
}

// the static layer is baked in square chunks at a few fixed zooms, each half a stop apart, and drawn scaled
// by at most ~19% either way: at zoom 1, 0.5 and 0.25 it is pixel for pixel
export const MIPS = [1, Math.SQRT1_2, 0.5, Math.SQRT1_2 / 2, 0.25] as const
export const CHUNK_PX = 512

// the baked zoom to draw `zoom` from. it keeps the one it has (`held`) until another is clearly nearer, so a
// camera hovering between two doesn't bake both over and over
export function mipFor(zoom: number, held = -1): number {
  let best = 0
  let bd = Infinity
  for (let i = 0; i < MIPS.length; i++) {
    const d = Math.abs(Math.log(zoom / MIPS[i]))
    if (d < bd) {
      bd = d
      best = i
    }
  }
  if (held >= 0 && held < MIPS.length && held !== best && Math.abs(Math.log(zoom / MIPS[held])) < bd + 0.06) return held
  return best
}

// the chunks (x1 / y1 exclusive, may be negative: the wall around a stage is baked too) a camera sees when
// the layer is baked at `kb` canvas px per world px
export function chunkRange(cam: Camera, kb: number, size = CHUNK_PX): { x0: number; y0: number; x1: number; y1: number } {
  const w = size / kb
  return { x0: Math.floor(cam.x / w), y0: Math.floor(cam.y / w), x1: Math.ceil((cam.x + cam.w) / w), y1: Math.ceil((cam.y + cam.h) / w) }
}

// ---------------------------------------------------------------- darkness

// a soft hole in the dark: fully clear (times a) out to HOLE_CORE of its radius, then fading to nothing at r
// p: the player whose glow this is
export type Hole = { x: number; y: number; r: number; a: number; p?: number }
export const HOLE_CORE = 0.45

const lavaTopsOf = new WeakMap<Stage, number[]>()
// lava tiles with something other than lava above them: where lava glows from
export function lavaTops(stage: Stage): number[] {
  let l = lavaTopsOf.get(stage)
  if (!l) {
    l = []
    for (let i = 0; i < stage.tiles.length; i++) if (stage.tiles[i] === T_LAVA && (i < stage.cols || stage.tiles[i - stage.cols] !== T_LAVA)) l.push(i)
    lavaTopsOf.set(stage, l)
  }
  return l
}

// where the dark is when the lights are off: the stage's dark patches, or all of it (and the wall around it)
export function darkRects(stage: Stage): Rect[] {
  return stage.darks.length ? stage.darks : [{ x: -1e5, y: -1e5, w: 2e5 + stage.w, h: 2e5 + stage.h }]
}

// everything that lights its own surroundings in the dark. the two players first (ember's glow is the big
// warm one), then the small lights: lava, lamps, doors, gems still there, portals and the checkpoint taken.
// `at` is where the players are drawn this frame, when that isn't exactly where the state has them
export function lightHoles(stage: Stage, s: GameState, at?: ReadonlyArray<{ x: number; y: number }>): Hole[] {
  const out: Hole[] = []
  for (let i = 0; i < 2; i++) {
    const p = s.players[i]
    if (!p || !p.alive) continue
    const q = at?.[i] ?? p
    out.push({ x: q.x + PLAYER_W / 2, y: q.y + PLAYER_H / 2, r: i === 0 ? GLOW_EMBER : GLOW_FROST, a: 1, p: i })
  }
  for (const i of lavaTops(stage)) {
    const tx = i % stage.cols
    out.push({ x: tx * TILE + TILE / 2, y: ((i - tx) / stage.cols) * TILE + POOL_SINK, r: GLOW_SMALL, a: 0.6 })
  }
  for (const l of stage.lamps) out.push({ x: l.x + l.w / 2, y: l.y + l.h / 2, r: GLOW_SMALL, a: 0.85 })
  for (const d of stage.doors) out.push({ x: d.x + d.w / 2, y: d.y + d.h / 2, r: GLOW_SMALL, a: 0.75 })
  for (let i = 0; i < stage.gems.length; i++) if (!s.taken[i]) out.push({ x: stage.gems[i].x, y: stage.gems[i].y, r: GLOW_SMALL * 0.7, a: 0.7 })
  for (const p of stage.portals) out.push({ x: p.x + p.w / 2, y: p.y + p.h / 2, r: GLOW_SMALL, a: 0.75 })
  const cp = stage.checkpoints[s.cp]
  if (cp) out.push({ x: cp.x, y: cp.y - TILE * 0.7, r: GLOW_SMALL, a: 0.75 })
  return out
}

// how dark a point is, 0 (lit) to 1 (black), with the lights off: the model the renderer's mask follows
export function darknessAt(holes: readonly Hole[], rects: readonly Rect[], x: number, y: number): number {
  let inside = false
  for (const r of rects) if (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) inside = true
  if (!inside) return 0
  let dark = 1
  for (const h of holes) {
    const d = Math.hypot(x - h.x, y - h.y) / h.r
    if (d >= 1) continue
    const f = d <= HOLE_CORE ? 1 : 1 - (d - HOLE_CORE) / (1 - HOLE_CORE)
    dark *= 1 - h.a * f
  }
  return dark
}

// particle kinds
export const PK_DOT = 0 // a soft round blob that shrinks
export const PK_SPARK = 1 // a tiny bright ember
export const PK_FLAKE = 2 // a six-armed snowflake
export const PK_CONFETTI = 3 // a tumbling paper rectangle
export const PK_RING = 4 // an expanding ring flash
export const PK_STAR = 5 // a four-point twinkle
export const PK_SHARD = 6 // a spinning ice shard
export const PK_STEAM = 7 // a growing soft puff
export const PK_STREAK = 8 // a vertical airflow line
export const PK_CHEV = 9 // a little chevron pointing the way it moves, up or down
export const PK_CHUNK = 10 // a tumbling square of rubble

export const MAX_PARTICLES = 400

// a fixed pool of particles kept in parallel typed arrays: spawning and updating never allocate. when it's full the
// next spawn overwrites a rolling slot, so a burst always shows even mid-firework
export class Particles {
  readonly cap: number
  n = 0
  private over = 0
  readonly x: Float32Array
  readonly y: Float32Array
  readonly vx: Float32Array
  readonly vy: Float32Array
  readonly life: Float32Array
  readonly max: Float32Array
  readonly size: Float32Array
  readonly grav: Float32Array
  readonly drag: Float32Array
  readonly rot: Float32Array
  readonly spin: Float32Array
  readonly kind: Uint8Array
  readonly col: Uint8Array
  readonly add: Uint8Array

  constructor(cap = MAX_PARTICLES) {
    this.cap = cap
    this.x = new Float32Array(cap)
    this.y = new Float32Array(cap)
    this.vx = new Float32Array(cap)
    this.vy = new Float32Array(cap)
    this.life = new Float32Array(cap)
    this.max = new Float32Array(cap)
    this.size = new Float32Array(cap)
    this.grav = new Float32Array(cap)
    this.drag = new Float32Array(cap)
    this.rot = new Float32Array(cap)
    this.spin = new Float32Array(cap)
    this.kind = new Uint8Array(cap)
    this.col = new Uint8Array(cap)
    this.add = new Uint8Array(cap)
  }

  clear(): void {
    this.n = 0
  }

  spawn(kind: number, col: number, x: number, y: number, vx: number, vy: number, life: number, size: number, grav = 0, drag = 0, add = false, spin = 0): void {
    let i = this.n
    if (i >= this.cap) {
      this.over = (this.over + 1) % this.cap
      i = this.over
    } else this.n++
    this.x[i] = x
    this.y[i] = y
    this.vx[i] = vx
    this.vy[i] = vy
    this.life[i] = life
    this.max[i] = life
    this.size[i] = size
    this.grav[i] = grav
    this.drag[i] = drag
    this.rot[i] = spin === 0 ? 0 : (x * 7.31 + y * 3.17) % 6.283
    this.spin[i] = spin
    this.kind[i] = kind
    this.col[i] = col
    this.add[i] = add ? 1 : 0
  }

  update(dt: number): void {
    for (let i = 0; i < this.n; i++) {
      const life = this.life[i] - dt
      if (life <= 0) {
        // swap the last one in and look at this slot again
        this.n--
        if (i < this.n) this.copy(this.n, i)
        i--
        continue
      }
      this.life[i] = life
      this.vy[i] += this.grav[i] * dt
      const damp = 1 / (1 + this.drag[i] * dt)
      this.vx[i] *= damp
      this.vy[i] *= damp
      this.x[i] += this.vx[i] * dt
      this.y[i] += this.vy[i] * dt
      this.rot[i] += this.spin[i] * dt
    }
  }

  private copy(from: number, to: number): void {
    this.x[to] = this.x[from]
    this.y[to] = this.y[from]
    this.vx[to] = this.vx[from]
    this.vy[to] = this.vy[from]
    this.life[to] = this.life[from]
    this.max[to] = this.max[from]
    this.size[to] = this.size[from]
    this.grav[to] = this.grav[from]
    this.drag[to] = this.drag[from]
    this.rot[to] = this.rot[from]
    this.spin[to] = this.spin[from]
    this.kind[to] = this.kind[from]
    this.col[to] = this.col[from]
    this.add[to] = this.add[from]
  }

  // two passes, plain then additive, so the composite mode switches at most twice a frame. `only` draws one
  // of them (0 plain, 1 additive): the glowing ones go on after the darkness, so sparks show in the dark
  draw(ctx: CanvasRenderingContext2D, colors: readonly string[], addOp: GlobalCompositeOperation, only = -1): void {
    for (let pass = 0; pass < 2; pass++) {
      if (only >= 0 && only !== pass) continue
      ctx.globalCompositeOperation = pass === 0 ? 'source-over' : addOp
      for (let i = 0; i < this.n; i++) {
        if (this.add[i] !== pass) continue
        this.drawOne(ctx, i, colors[this.col[i]] ?? '#fff')
      }
    }
    ctx.globalCompositeOperation = 'source-over'
    ctx.globalAlpha = 1
  }

  private drawOne(ctx: CanvasRenderingContext2D, i: number, color: string): void {
    const t = this.life[i] / this.max[i] // 1 → 0
    const x = this.x[i]
    const y = this.y[i]
    const s = this.size[i]
    const fadeIn = Math.min(1, (1 - t) * 8)
    switch (this.kind[i]) {
      case PK_DOT:
      case PK_SPARK: {
        ctx.globalAlpha = Math.min(1, t * 2) * fadeIn
        ctx.fillStyle = color
        const r = s * (0.35 + 0.65 * t)
        ctx.fillRect(x - r, y - r, r * 2, r * 2) // tiny, a square reads as round at this size and is much cheaper
        break
      }
      case PK_FLAKE: {
        ctx.globalAlpha = Math.min(1, t * 2.5) * fadeIn
        ctx.strokeStyle = color
        ctx.lineWidth = 0.7
        ctx.beginPath()
        const a = this.rot[i]
        for (let k = 0; k < 3; k++) {
          const c = Math.cos(a + (k * Math.PI) / 3) * s
          const d = Math.sin(a + (k * Math.PI) / 3) * s
          ctx.moveTo(x - c, y - d)
          ctx.lineTo(x + c, y + d)
        }
        ctx.stroke()
        break
      }
      case PK_CONFETTI: {
        ctx.globalAlpha = Math.min(1, t * 3)
        ctx.fillStyle = color
        const a = this.rot[i]
        const w = s * Math.abs(Math.cos(a * 1.7)) + 0.4 // flips as it tumbles
        ctx.save()
        ctx.translate(x, y)
        ctx.rotate(a)
        ctx.fillRect(-w / 2, -s * 0.3, w, s * 0.6)
        ctx.restore()
        break
      }
      case PK_RING: {
        ctx.globalAlpha = t * 0.9
        ctx.strokeStyle = color
        ctx.lineWidth = 0.6 + 1.6 * t
        ctx.beginPath()
        ctx.arc(x, y, 1 + s * (1 - t * t), 0, Math.PI * 2)
        ctx.stroke()
        break
      }
      case PK_STAR: {
        const k = Math.sin(Math.PI * t) // grows then shrinks
        ctx.globalAlpha = k
        ctx.fillStyle = color
        const r = s * k
        const w = r * 0.22
        ctx.beginPath()
        ctx.moveTo(x, y - r)
        ctx.lineTo(x + w, y - w)
        ctx.lineTo(x + r, y)
        ctx.lineTo(x + w, y + w)
        ctx.lineTo(x, y + r)
        ctx.lineTo(x - w, y + w)
        ctx.lineTo(x - r, y)
        ctx.lineTo(x - w, y - w)
        ctx.closePath()
        ctx.fill()
        break
      }
      case PK_SHARD: {
        ctx.globalAlpha = Math.min(1, t * 2)
        ctx.fillStyle = color
        ctx.save()
        ctx.translate(x, y)
        ctx.rotate(this.rot[i])
        ctx.beginPath()
        ctx.moveTo(0, -s)
        ctx.lineTo(s * 0.45, s * 0.6)
        ctx.lineTo(-s * 0.45, s * 0.4)
        ctx.closePath()
        ctx.fill()
        ctx.restore()
        break
      }
      case PK_STEAM: {
        ctx.globalAlpha = 0.4 * t * fadeIn
        ctx.fillStyle = color
        ctx.beginPath()
        ctx.arc(x, y, s * (1.5 - t), 0, Math.PI * 2)
        ctx.fill()
        break
      }
      case PK_STREAK: {
        ctx.globalAlpha = 0.45 * Math.min(1, t * 3) * fadeIn
        ctx.fillStyle = color
        ctx.fillRect(x - 0.4, y, 0.8, s)
        break
      }
      case PK_CHEV: {
        ctx.globalAlpha = Math.min(1, t * 2.5) * fadeIn
        ctx.strokeStyle = color
        ctx.lineWidth = 1.1
        const d = this.vy[i] > 0 ? -1 : 1
        ctx.beginPath()
        ctx.moveTo(x - s, y + s * 0.5 * d)
        ctx.lineTo(x, y - s * 0.5 * d)
        ctx.lineTo(x + s, y + s * 0.5 * d)
        ctx.stroke()
        break
      }
      case PK_CHUNK: {
        ctx.globalAlpha = Math.min(1, t * 2.5)
        ctx.fillStyle = color
        ctx.save()
        ctx.translate(x, y)
        ctx.rotate(this.rot[i])
        ctx.fillRect(-s / 2, -s / 2, s, s * 0.8)
        ctx.restore()
        break
      }
    }
  }
}
