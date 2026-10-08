import type { Cond } from '../lib/emberTypes'

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

// whether a condition holds for the channels that are on; no condition means always
export function condHolds(c: Cond | undefined, on: readonly string[]): boolean {
  if (c === undefined) return true
  const list = typeof c === 'string' ? [c] : c
  for (const n of list) {
    const neg = n.startsWith('!')
    const name = neg ? n.slice(1) : n
    if (on.includes(name) === neg) return false
  }
  return true
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

  // two passes, plain then additive, so the composite mode switches at most twice a frame
  draw(ctx: CanvasRenderingContext2D, colors: readonly string[], addOp: GlobalCompositeOperation): void {
    for (let pass = 0; pass < 2; pass++) {
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
    }
  }
}
