import { gateRect, tileAt, traceBeams } from '../lib/emberEngine'
import { COLS, PLAYER_H, PLAYER_W, POOL_SINK, ROWS, RUN_SPEED, T_GOO, T_LAVA, T_THIN, T_WALL, T_WATER, TILE, WORLD_W } from '../lib/emberTypes'
import type { GameEvent, GameState, Level, Player, PlayerIndex, Rect } from '../lib/emberTypes'
import {
  CHANNEL_COLORS,
  channelIndex,
  condChannels,
  condHolds,
  css,
  hash01,
  luma,
  MAX_PARTICLES,
  mixRgb,
  parseColor,
  Particles,
  PK_CONFETTI,
  PK_DOT,
  PK_FLAKE,
  PK_RING,
  PK_SHARD,
  PK_SPARK,
  PK_STAR,
  PK_STEAM,
  PK_STREAK,
} from './emberFx'
import type { RGB } from './emberFx'

// ember & frost on a canvas: tiles, pools, mechanics, the two characters, particles and glow.
// world units throughout (640x360); one transform scales them to the backing store

export type DrawOpts = {
  me: PlayerIndex | null // online: outline/label this side's character; null in local play
  reduceMotion: boolean // fewer particles, no screen shake
}

const TAU = Math.PI * 2
const clamp = (v: number, a: number, b: number) => (v < a ? a : v > b ? b : v)
// frame-rate independent ease toward a target
const approach = (v: number, target: number, rate: number, dt: number) => v + (target - v) * (1 - Math.exp(-rate * dt))
const rand = (a: number, b: number) => a + Math.random() * (b - a)

// ---------------------------------------------------------------- colours

export type Palette = {
  dark: boolean
  bg: RGB
  tape: RGB
  ink: RGB
  muted: RGB
  accent: RGB
  fire: RGB
  fire2: RGB
  frost: RGB
  frost2: RGB
  star: RGB
  wall: RGB
  wallHi: RGB
  wallLo: RGB
  key: string
}

const WHITE: RGB = [255, 255, 255]
const BLACK: RGB = [0, 0, 0]

// the theme tokens live on :root and the panel's .ember; the canvas inherits both
export function readPalette(el: HTMLCanvasElement | null): Palette {
  let cs: CSSStyleDeclaration | null = null
  try {
    cs = el && typeof getComputedStyle === 'function' ? getComputedStyle(el) : null
  } catch {
    cs = null
  }
  const v = (name: string, fb: RGB): RGB => (cs ? parseColor(cs.getPropertyValue(name) || '', fb) : fb)
  const bg = v('--bg', [255, 255, 255])
  const dark = luma(bg) < 0.45
  const tape = v('--bg-tape', dark ? [37, 37, 39] : [245, 245, 247])
  const ink = v('--ink', dark ? [245, 245, 247] : [29, 29, 31])
  const muted = v('--muted', dark ? [152, 152, 157] : [134, 134, 139])
  const accent = v('--accent', dark ? [99, 211, 146] : [29, 122, 76])
  const fire = v('--ember-fire', dark ? [255, 138, 71] : [232, 100, 28])
  const fire2 = v('--ember-fire-2', dark ? [255, 193, 94] : [245, 165, 36])
  const frost = v('--ember-frost', dark ? [95, 205, 242] : [31, 147, 198])
  const frost2 = v('--ember-frost-2', dark ? [165, 232, 251] : [92, 200, 230])
  const star = v('--ember-star', dark ? [255, 196, 77] : [240, 168, 26])
  // a cool slate stone that sits between the theme's paper and ink
  const stone: RGB = dark ? [92, 88, 108] : [178, 172, 190]
  const wall = mixRgb(mixRgb(tape, ink, dark ? 0.2 : 0.3), stone, 0.4)
  const wallHi = dark ? mixRgb(wall, ink, 0.24) : mixRgb(wall, WHITE, 0.6)
  const wallLo = dark ? mixRgb(wall, BLACK, 0.38) : mixRgb(wall, ink, 0.2)
  const key = [bg, tape, ink, accent, fire, frost].map((c) => c.join(',')).join('|')
  return { dark, bg, tape, ink, muted, accent, fire, fire2, frost, frost2, star, wall, wallHi, wallLo, key }
}

// liquids keep their own colours in both themes: they're the rules of the game
const LAVA_TOP: RGB = [255, 196, 84]
const LAVA_MID: RGB = [255, 106, 31]
const LAVA_BOT: RGB = [190, 38, 26]
const WATER_TOP: RGB = [104, 190, 250]
const WATER_BOT: RGB = [28, 92, 190]
const GOO_TOP: RGB = [170, 236, 80]
const GOO_BOT: RGB = [58, 140, 34]
const THIN_TOP: RGB = [176, 224, 248]
const THIN_BOT: RGB = [110, 178, 226]
const ICE_TOP: RGB = [236, 250, 255]
const ICE_BOT: RGB = [168, 220, 244]
const WOOD: RGB = [178, 118, 62]
const WOOD_DARK: RGB = [116, 70, 34]
const METAL: RGB = [128, 134, 150]
const PORTAL_COLORS: RGB[] = [
  [168, 112, 255],
  [255, 122, 214],
  [64, 222, 196],
  [255, 212, 92],
]

// particle colour slots (indices into the renderer's colour table)
const C_FIRE = 0
const C_FIRE2 = 1
const C_FROST = 2
const C_FROST2 = 3
const C_WHITE = 4
const C_STEAM = 5
const C_GOO = 6
const C_LAVA = 7
const C_WATER = 8
const C_DUST = 9
const C_STAR = 10
const C_CORE = 11
const C_ICE = 12
const C_PORTAL = 13 // + PORTAL_COLORS.length
const C_CH = C_PORTAL + PORTAL_COLORS.length // + CHANNEL_COLORS.length

function colorTable(p: Palette): string[] {
  const t: string[] = []
  t[C_FIRE] = css(p.fire)
  t[C_FIRE2] = css(p.fire2)
  t[C_FROST] = css(p.frost)
  t[C_FROST2] = css(p.frost2)
  t[C_WHITE] = p.dark ? '#ffffff' : css(mixRgb(p.frost2, WHITE, 0.3))
  t[C_STEAM] = p.dark ? 'rgb(220, 228, 236)' : 'rgb(170, 180, 192)'
  t[C_GOO] = css(GOO_TOP)
  t[C_LAVA] = css(LAVA_MID)
  t[C_WATER] = css(WATER_TOP)
  t[C_DUST] = css(p.dark ? mixRgb(p.wall, p.ink, 0.35) : mixRgb(p.wall, p.ink, 0.15))
  t[C_STAR] = css(p.star)
  t[C_CORE] = p.dark ? 'rgb(255, 246, 214)' : css(mixRgb(p.star, LAVA_MID, 0.25))
  t[C_ICE] = css(p.dark ? ICE_TOP : mixRgb(ICE_BOT, p.frost, 0.35))
  PORTAL_COLORS.forEach((c, i) => (t[C_PORTAL + i] = css(c)))
  CHANNEL_COLORS.forEach((c, i) => (t[C_CH + i] = c))
  return t
}

// ---------------------------------------------------------------- shared shape helpers

// a rect with its own radius per corner (tl, tr, br, bl)
function roundRectPath(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, tl: number, tr: number, br: number, bl: number): void {
  g.beginPath()
  g.moveTo(x + tl, y)
  g.lineTo(x + w - tr, y)
  if (tr) g.arcTo(x + w, y, x + w, y + tr, tr)
  else g.lineTo(x + w, y)
  g.lineTo(x + w, y + h - br)
  if (br) g.arcTo(x + w, y + h, x + w - br, y + h, br)
  else g.lineTo(x + w, y + h)
  g.lineTo(x + bl, y + h)
  if (bl) g.arcTo(x, y + h, x, y + h - bl, bl)
  else g.lineTo(x, y + h)
  g.lineTo(x, y + tl)
  if (tl) g.arcTo(x, y, x + tl, y, tl)
  else g.lineTo(x, y)
  g.closePath()
}

function rr(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number): void {
  const k = Math.min(r, w / 2, h / 2)
  roundRectPath(g, x, y, w, h, k, k, k, k)
}

// a little flame glyph, centred on (x, y), h tall
function flameGlyph(g: CanvasRenderingContext2D, x: number, y: number, h: number): void {
  const w = h * 0.42
  const b = y + h * 0.5
  g.beginPath()
  g.moveTo(x, b)
  g.bezierCurveTo(x - w * 1.1, b, x - w * 1.15, y - h * 0.02, x - w * 0.6, y - h * 0.24)
  // a small side lick, then the main tongue leaning right
  g.quadraticCurveTo(x - w * 0.5, y - h * 0.02, x - w * 0.18, y - h * 0.06)
  g.bezierCurveTo(x - w * 0.35, y - h * 0.3, x - w * 0.05, y - h * 0.42, x + w * 0.2, y - h * 0.5)
  g.bezierCurveTo(x + w * 0.3, y - h * 0.25, x + w * 1.15, y - h * 0.12, x + w * 0.98, y + h * 0.2)
  g.bezierCurveTo(x + w * 0.88, b, x + w * 0.4, b, x, b)
  g.closePath()
}

// a six-armed snowflake stroke, centred on (x, y), radius r
function flakeGlyph(g: CanvasRenderingContext2D, x: number, y: number, r: number): void {
  g.beginPath()
  for (let k = 0; k < 6; k++) {
    const a = (k * Math.PI) / 3 - Math.PI / 2
    const c = Math.cos(a)
    const s = Math.sin(a)
    g.moveTo(x, y)
    g.lineTo(x + c * r, y + s * r)
    // little barbs two thirds out
    const bx = x + c * r * 0.6
    const by = y + s * r * 0.6
    const b = r * 0.3
    g.moveTo(bx, by)
    g.lineTo(bx + Math.cos(a + 0.8) * b, by + Math.sin(a + 0.8) * b)
    g.moveTo(bx, by)
    g.lineTo(bx + Math.cos(a - 0.8) * b, by + Math.sin(a - 0.8) * b)
  }
}

// a faceted gem, centred on (0, 0) in the current transform
function gemPath(g: CanvasRenderingContext2D, s: number): void {
  g.beginPath()
  g.moveTo(-5 * s, -1.5 * s)
  g.lineTo(-2.8 * s, -4.5 * s)
  g.lineTo(2.8 * s, -4.5 * s)
  g.lineTo(5 * s, -1.5 * s)
  g.lineTo(0, 5.5 * s)
  g.closePath()
}

// the door's arch: square bottom, round top
function archPath(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number): void {
  const r = w / 2
  g.beginPath()
  g.moveTo(x, y + h)
  g.lineTo(x, y + r)
  g.arc(x + r, y + r, r, Math.PI, 0)
  g.lineTo(x + w, y + h)
  g.closePath()
}

function isWall(level: Level, tx: number, ty: number): boolean {
  if (tx < 0 || ty < 0 || tx >= COLS || ty >= ROWS) return true
  return level.tiles[ty * COLS + tx] === T_WALL
}

// a wall's neighbour that hides its edge: more wall, or a pool sunk into it (so basins have square rims)
function closed(level: Level, tx: number, ty: number): boolean {
  if (tx < 0 || ty < 0 || tx >= COLS || ty >= ROWS) return true
  return level.tiles[ty * COLS + tx] !== 0
}

// ---------------------------------------------------------------- the static layer: backdrop + walls

function paintBackdrop(g: CanvasRenderingContext2D, p: Palette): void {
  const W = WORLD_W
  const H = ROWS * TILE
  const sky = g.createLinearGradient(0, 0, 0, H)
  sky.addColorStop(0, css(mixRgb(p.bg, p.tape, p.dark ? 0.1 : 0.15)))
  sky.addColorStop(1, css(p.dark ? mixRgb(p.tape, BLACK, 0.12) : mixRgb(p.tape, p.wall, 0.12)))
  g.fillStyle = sky
  g.fillRect(0, 0, W, H)
  // a warm hearth glow low on the left, a cool one on the right: the two halves of the game
  const warm = g.createRadialGradient(W * 0.12, H * 1.05, 0, W * 0.12, H * 1.05, H * 0.95)
  warm.addColorStop(0, css(p.fire, p.dark ? 0.1 : 0.07))
  warm.addColorStop(1, css(p.fire, 0))
  g.fillStyle = warm
  g.fillRect(0, 0, W, H)
  const cool = g.createRadialGradient(W * 0.88, H * 1.05, 0, W * 0.88, H * 1.05, H * 0.95)
  cool.addColorStop(0, css(p.frost, p.dark ? 0.1 : 0.07))
  cool.addColorStop(1, css(p.frost, 0))
  g.fillStyle = cool
  g.fillRect(0, 0, W, H)
  // faint big bricks in the back wall, each a touch different
  const bw = TILE * 2
  const bh = TILE
  for (let row = 0; row < ROWS; row++) {
    const off = row % 2 ? bw / 2 : 0
    for (let bx = -off; bx < W; bx += bw) {
      const n = hash01(row * 97 + Math.round(bx) * 13 + 5)
      g.fillStyle = css(p.ink, (p.dark ? 0.018 : 0.012) + n * (p.dark ? 0.02 : 0.016))
      rr(g, bx + 1, row * bh + 1, bw - 2, bh - 2, 3)
      g.fill()
    }
  }
  // soft vignette keeps the eye in the middle
  const vg = g.createRadialGradient(W / 2, H / 2, H * 0.35, W / 2, H / 2, W * 0.65)
  vg.addColorStop(0, 'rgba(0, 0, 0, 0)')
  vg.addColorStop(1, p.dark ? 'rgba(0, 0, 0, 0.28)' : 'rgba(40, 30, 60, 0.07)')
  g.fillStyle = vg
  g.fillRect(0, 0, W, H)
}

// walls as one soft mass: rounded only where two open sides meet, lit on top, shaded underneath
function paintWalls(g: CanvasRenderingContext2D, level: Level, p: Palette, detail: boolean): void {
  const R = 5
  // a soft drop shadow first, so it sits under every block
  g.fillStyle = p.dark ? 'rgba(0, 0, 0, 0.32)' : 'rgba(40, 30, 70, 0.10)'
  for (let ty = 0; ty < ROWS; ty++)
    for (let tx = 0; tx < COLS; tx++) {
      if (!isWall(level, tx, ty)) continue
      if (closed(level, tx, ty + 1) && closed(level, tx + 1, ty)) continue
      const u = !closed(level, tx, ty - 1)
      const d = !closed(level, tx, ty + 1)
      const l = !closed(level, tx - 1, ty)
      const r = !closed(level, tx + 1, ty)
      roundRectPath(g, tx * TILE + 1, ty * TILE + 2.5, TILE, TILE, u && l ? R : 0, u && r ? R : 0, d && r ? R : 0, d && l ? R : 0)
      g.fill()
    }
  const base = css(p.wall)
  const hi = css(p.wallHi)
  const lo = css(p.wallLo)
  const mortar = css(p.wallLo, p.dark ? 0.35 : 0.22)
  const speck = css(p.dark ? p.wallHi : p.wallLo, 0.25)
  for (let ty = 0; ty < ROWS; ty++)
    for (let tx = 0; tx < COLS; tx++) {
      if (!isWall(level, tx, ty)) continue
      const x = tx * TILE
      const y = ty * TILE
      const u = !closed(level, tx, ty - 1)
      const d = !closed(level, tx, ty + 1)
      const l = !closed(level, tx - 1, ty)
      const r = !closed(level, tx + 1, ty)
      const rtl = u && l ? R : 0
      const rtr = u && r ? R : 0
      const rbr = d && r ? R : 0
      const rbl = d && l ? R : 0
      g.save()
      roundRectPath(g, x, y, TILE, TILE, rtl, rtr, rbr, rbl)
      g.fillStyle = base
      g.fill()
      g.clip()
      if (detail) {
        // staggered stone courses: a seam halfway down, joints offset row to row
        g.fillStyle = mortar
        g.fillRect(x, y + TILE / 2 - 0.5, TILE, 1)
        const j = ty % 2 ? 0 : TILE / 2
        g.fillRect(x + j - 0.5, y, 1, TILE / 2)
        g.fillRect(x + ((j + TILE / 2) % TILE) - 0.5, y + TILE / 2, 1, TILE / 2)
        g.fillStyle = speck
        for (let k = 0; k < 3; k++) {
          const n = hash01(ty * COLS * 7 + tx * 7 + k)
          const m = hash01(ty * COLS * 11 + tx * 5 + k + 999)
          g.fillRect(x + 2 + n * (TILE - 4), y + 2 + m * (TILE - 4), 1, 1)
        }
      }
      if (u) {
        // the walkable lip: bright, with a slightly darker underside so it reads as a ledge
        g.fillStyle = hi
        g.fillRect(x, y, TILE, 3)
        g.fillStyle = css(p.wallLo, 0.25)
        g.fillRect(x, y + 3, TILE, 1)
      }
      if (d) {
        g.fillStyle = lo
        g.fillRect(x, y + TILE - 2.5, TILE, 2.5)
      }
      if (l) {
        g.fillStyle = css(p.dark ? p.wallHi : p.wallLo, 0.35)
        g.fillRect(x, y, 1.5, TILE)
      }
      if (r) {
        g.fillStyle = css(p.wallLo, 0.55)
        g.fillRect(x + TILE - 1.5, y, 1.5, TILE)
      }
      g.restore()
    }
}

// ---------------------------------------------------------------- per-player animation state

type PAnim = {
  sq: number // squash (-) / stretch (+)
  sqv: number
  run: number // run cycle phase
  tilt: number
  look: number // eyes: -1 left .. 1 right
  blinkAt: number
  blinkUntil: number
  wasGround: boolean
  trail: number // particle emission accumulator
  ambient: number
  x: number // last drawn spot, for effects
  y: number
}

function newAnim(): PAnim {
  return { sq: 0, sqv: 0, run: 0, tilt: 0, look: 1, blinkAt: 1 + Math.random() * 2, blinkUntil: 0, wasGround: true, trail: 0, ambient: 0, x: 0, y: 0 }
}

// ---------------------------------------------------------------- the renderer

export class EmberRenderer {
  private canvas: HTMLCanvasElement
  private ctx: CanvasRenderingContext2D | null
  private pal: Palette
  private colors: string[]
  private parts = new Particles(MAX_PARTICLES)
  private stat: HTMLCanvasElement | null = null
  private statKey = ''
  private glowCache = new Map<string, HTMLCanvasElement>()
  private poolGrad: Array<CanvasGradient | null> = []
  private level: Level | null = null
  // per-level lists built once so a frame doesn't have to search the map
  private pools: number[] = [] // tile indices of every liquid tile
  private lavaTops: number[] = [] // lava tiles with open sky, for glow and sparks
  private thinIndex = new Map<number, number>()
  private portalCol: number[] = []
  private lastMs = 0
  private shake = 0
  private winAt = -1
  private anims: [PAnim, PAnim] = [newAnim(), newAnim()]
  private leverAng: number[] = []
  private mirrorAng: number[] = []
  private fanAng: number[] = []
  private fanSpin: number[] = []
  private iceLast: number[] = []
  private doorOpen: [number, number] = [0, 0]
  private textW = new Map<string, number>()
  private px: [number, number] = [0, 0]
  private py: [number, number] = [0, 0]

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas
    this.ctx = canvas.getContext('2d')
    this.pal = readPalette(canvas)
    this.colors = colorTable(this.pal)
  }

  // match the canvas backing store to its css size and devicePixelRatio; call on mount and resize
  resize(): void {
    const r = this.canvas.getBoundingClientRect()
    if (r.width < 1) return
    const dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1))
    const w = Math.round(r.width * dpr)
    const h = Math.round((w * 360) / 640)
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w
      this.canvas.height = h
      this.statKey = ''
    }
  }

  // forget particles and effects (new level)
  reset(): void {
    this.parts.clear()
    this.shake = 0
    this.winAt = -1
    this.level = null
    this.lastMs = 0
  }

  // re-read css colour tokens (theme change)
  refreshColors(): void {
    this.pal = readPalette(this.canvas)
    this.colors = colorTable(this.pal)
    this.glowCache.clear()
    this.poolGrad = []
    this.statKey = ''
  }

  private setLevel(level: Level): void {
    this.level = level
    this.statKey = ''
    this.pools = []
    this.lavaTops = []
    for (let i = 0; i < level.tiles.length; i++) {
      const t = level.tiles[i]
      if (t === T_LAVA || t === T_WATER || t === T_GOO || t === T_THIN) this.pools.push(i)
      if (t === T_LAVA && (i < COLS || level.tiles[i - COLS] !== T_LAVA)) this.lavaTops.push(i)
    }
    this.thinIndex.clear()
    level.thin.forEach((ti, j) => this.thinIndex.set(ti, j))
    const pairs: string[] = []
    this.portalCol = level.portals.map((p) => {
      let k = pairs.indexOf(p.pair)
      if (k < 0) k = pairs.push(p.pair) - 1
      return k % PORTAL_COLORS.length
    })
    this.leverAng = level.levers.map((l) => (l.on ? 1 : -1))
    this.mirrorAng = level.mirrors.map((m) => (m.slant === '/' ? -Math.PI / 4 : Math.PI / 4))
    this.fanAng = level.fans.map(() => 0)
    this.fanSpin = level.fans.map(() => 0)
    this.iceLast = level.ice.map(() => 0)
    this.doorOpen = [0, 0]
    this.anims = [newAnim(), newAnim()]
    this.parts.clear()
  }

  // ------------------------------------------------------------ glow sprites

  // a soft radial blob per colour, drawn scaled: far cheaper than a gradient every frame
  private glowSprite(color: string): HTMLCanvasElement {
    let s = this.glowCache.get(color)
    if (s) return s
    s = document.createElement('canvas')
    s.width = s.height = 64
    const g = s.getContext('2d')!
    const c = parseColor(color, WHITE)
    const grad = g.createRadialGradient(32, 32, 0, 32, 32, 32)
    grad.addColorStop(0, css(c, 0.9))
    grad.addColorStop(0.25, css(c, 0.45))
    grad.addColorStop(0.6, css(c, 0.12))
    grad.addColorStop(1, css(c, 0))
    g.fillStyle = grad
    g.fillRect(0, 0, 64, 64)
    this.glowCache.set(color, s)
    return s
  }

  // call between beginGlow/endGlow
  private glow(g: CanvasRenderingContext2D, x: number, y: number, r: number, color: string, a: number): void {
    if (a <= 0.003) return
    g.globalAlpha = Math.min(1, a * (this.pal.dark ? 1 : 0.5))
    g.drawImage(this.glowSprite(color), x - r, y - r, r * 2, r * 2)
  }

  private beginGlow(g: CanvasRenderingContext2D): void {
    g.globalCompositeOperation = this.pal.dark ? 'lighter' : 'source-over'
  }

  private endGlow(g: CanvasRenderingContext2D): void {
    g.globalCompositeOperation = 'source-over'
    g.globalAlpha = 1
  }

  // ------------------------------------------------------------ static layer

  private ensureStatic(level: Level): void {
    const W = this.canvas.width
    const H = this.canvas.height
    const key = `${level.def.id}|${this.pal.key}|${W}x${H}`
    if (this.stat && key === this.statKey) return
    if (!this.stat) this.stat = document.createElement('canvas')
    const c = this.stat
    c.width = W
    c.height = H
    const g = c.getContext('2d')
    if (!g) return
    const k = W / WORLD_W
    g.setTransform(k, 0, 0, k, 0, 0)
    paintBackdrop(g, this.pal)
    paintWalls(g, level, this.pal, true)
    this.statKey = key
  }

  // ------------------------------------------------------------ particles helpers

  private burst(kind: number, col: number, x: number, y: number, n: number, speed: number, life: number, size: number, grav = 0, drag = 2, add = false, spin = 0, up = 0): void {
    for (let i = 0; i < n; i++) {
      const a = Math.random() * TAU
      const v = speed * (0.35 + Math.random() * 0.65)
      this.parts.spawn(kind, col, x, y, Math.cos(a) * v, Math.sin(a) * v - up, life * (0.6 + Math.random() * 0.6), size * (0.7 + Math.random() * 0.6), grav, drag, add, spin ? rand(-spin, spin) : 0)
    }
  }

  private chCol(name: string | undefined): number {
    return name ? C_CH + channelIndex(name) : C_WHITE
  }

  private onEvent(level: Level, cur: GameState, e: GameEvent, opts: DrawOpts, t: number): void {
    const less = opts.reduceMotion ? 0.4 : 1
    const n = (k: number) => Math.max(1, Math.round(k * less))
    switch (e.k) {
      case 'jump': {
        const pl = cur.players[e.p]
        const fx = pl.x + PLAYER_W / 2
        const fy = pl.y + PLAYER_H
        for (let i = 0; i < n(5); i++) this.parts.spawn(PK_DOT, C_DUST, fx + rand(-5, 5), fy - 1, rand(-30, 30), rand(-25, -5), rand(0.25, 0.4), rand(1.2, 2), 40, 4)
        if (e.p === 0) this.burst(PK_SPARK, C_FIRE2, fx, fy - 4, n(4), 50, 0.35, 0.9, -60, 3, true)
        else this.burst(PK_FLAKE, C_FROST2, fx, fy - 4, n(3), 40, 0.5, 2, 20, 3, true, 4)
        break
      }
      case 'land': {
        const pl = cur.players[e.p]
        const fx = pl.x + PLAYER_W / 2
        const fy = pl.y + PLAYER_H
        for (let i = 0; i < n(7); i++) {
          const s = i % 2 ? 1 : -1
          this.parts.spawn(PK_DOT, C_DUST, fx + s * rand(2, 6), fy - 1, s * rand(25, 60), rand(-18, -4), rand(0.25, 0.45), rand(1.2, 2.2), 60, 5)
        }
        break
      }
      case 'die': {
        const pl = cur.players[e.p]
        const cx = pl.x + PLAYER_W / 2
        const cy = pl.y + PLAYER_H / 2
        if (!opts.reduceMotion) this.shake = 1
        if (e.cause === 'water') {
          // the flame hisses out: steam and a last few sparks
          for (let i = 0; i < n(16); i++) this.parts.spawn(PK_STEAM, C_STEAM, cx + rand(-6, 6), cy + rand(-4, 6), rand(-20, 20), rand(-60, -25), rand(0.6, 1.1), rand(3, 6), -10, 1.5)
          this.burst(PK_SPARK, C_FIRE2, cx, cy, n(12), 90, 0.5, 1, 120, 2, true)
          this.burst(PK_RING, C_STEAM, cx, cy + 4, 1, 0, 0.5, 14)
        } else if (e.cause === 'lava') {
          // the crystal melts into droplets
          for (let i = 0; i < n(18); i++) this.parts.spawn(PK_DOT, i % 3 ? C_FROST : C_FROST2, cx + rand(-6, 6), cy + rand(-8, 4), rand(-70, 70), rand(-140, -40), rand(0.5, 0.9), rand(1.2, 2.2), 520, 1)
          this.burst(PK_SHARD, C_ICE, cx, cy, n(6), 90, 0.6, 3, 300, 1, false, 12, 50)
          for (let i = 0; i < n(8); i++) this.parts.spawn(PK_STEAM, C_STEAM, cx + rand(-5, 5), cy, rand(-15, 15), rand(-50, -20), rand(0.5, 0.9), rand(3, 5), -10, 1.5)
        } else {
          // goo: a sticky green splash
          for (let i = 0; i < n(20); i++) this.parts.spawn(PK_DOT, i % 4 ? C_GOO : e.p === 0 ? C_FIRE : C_FROST, cx + rand(-5, 5), cy + 4, rand(-80, 80), rand(-170, -60), rand(0.5, 0.9), rand(1.3, 2.5), 560, 0.8)
          this.burst(PK_RING, C_GOO, cx, cy + 6, 1, 0, 0.45, 16)
        }
        break
      }
      case 'door': {
        const d = level.doors[e.p]
        this.burst(PK_RING, e.p === 0 ? C_FIRE : C_FROST, d.x + d.w / 2, d.y + d.h / 2, 1, 0, 0.6, 22, 0, 0, true)
        this.burst(PK_STAR, e.p === 0 ? C_FIRE2 : C_FROST2, d.x + d.w / 2, d.y + d.h / 2, n(6), 50, 0.6, 3, 0, 2, true)
        break
      }
      case 'portal': {
        const pl = cur.players[e.p]
        const col = this.portalNear(level, e.x, e.y)
        this.burst(PK_RING, col, e.x, e.y, 1, 0, 0.45, 16, 0, 0, true)
        this.burst(PK_RING, col, pl.x + PLAYER_W / 2, pl.y + PLAYER_H / 2, 1, 0, 0.5, 18, 0, 0, true)
        this.burst(PK_STAR, col, pl.x + PLAYER_W / 2, pl.y + PLAYER_H / 2, n(8), 70, 0.5, 2.5, 0, 3, true)
        break
      }
      case 'push': {
        if (Math.random() < 0.5) this.parts.spawn(PK_DOT, C_DUST, e.x + rand(-4, 4), e.y, rand(-10, 10), rand(-15, -5), 0.35, 1.4, 30, 4)
        break
      }
      case 'gem': {
        const c = e.el === 'fire' ? C_FIRE : C_FROST
        const c2 = e.el === 'fire' ? C_FIRE2 : C_FROST2
        this.burst(PK_RING, c2, e.x, e.y, 1, 0, 0.45, 16, 0, 0, true)
        this.burst(PK_STAR, c2, e.x, e.y, n(10), 110, 0.6, 3.5, 0, 3, true)
        this.burst(PK_SPARK, c, e.x, e.y, n(14), 140, 0.5, 1, 160, 2, true)
        break
      }
      case 'lever':
      case 'mirror': {
        const col = e.k === 'lever' ? this.chCol(level.levers[e.i]?.ch) : C_WHITE
        this.burst(PK_RING, col, e.x, e.y, 1, 0, 0.35, 10, 0, 0, true)
        this.burst(PK_SPARK, col, e.x, e.y, n(5), 60, 0.3, 0.9, 100, 2, true)
        break
      }
      case 'plate':
      case 'button':
      case 'sensor': {
        if (!e.on) break
        const list = e.k === 'plate' ? level.plates : e.k === 'button' ? level.buttons : level.sensors
        const col = this.chCol(list[e.i]?.ch)
        this.burst(PK_RING, col, e.x, e.y, 1, 0, 0.4, 12, 0, 0, true)
        break
      }
      case 'melt': {
        const r = level.ice[e.i]
        const cx = r ? r.x + r.w / 2 : e.x
        const cy = r ? r.y + r.h / 2 : e.y
        const hw = r ? r.w / 2 : TILE / 2
        const hh = r ? r.h / 2 : TILE / 2
        const area = (hw * hh * 4) / (TILE * TILE)
        this.burst(PK_SHARD, C_ICE, cx, cy, n(Math.min(30, 8 + area * 4)), 130, 0.8, 3.5, 380, 1, false, 14, 60)
        for (let i = 0; i < n(Math.min(16, 6 + area * 2)); i++) this.parts.spawn(PK_STEAM, C_STEAM, cx + rand(-hw, hw), cy + rand(-hh, hh), rand(-10, 10), rand(-45, -20), rand(0.6, 1), rand(3, 6), -8, 1.5)
        break
      }
      case 'freeze': {
        this.burst(PK_FLAKE, C_FROST2, e.x, e.y, n(5), 40, 0.6, 2.2, 0, 3, true, 3)
        this.burst(PK_STAR, C_WHITE, e.x, e.y - 4, n(2), 20, 0.5, 2.5, 0, 3, true)
        break
      }
      case 'thaw': {
        for (let i = 0; i < n(4); i++) this.parts.spawn(PK_DOT, C_WATER, e.x + rand(-8, 8), e.y - 4, rand(-10, 10), rand(-50, -20), 0.4, 1.2, 300, 1)
        break
      }
      case 'win': {
        this.winAt = t
        this.confetti(n(80))
        for (const d of level.doors) this.burst(PK_STAR, C_STAR, d.x + d.w / 2, d.y + d.h / 2, n(10), 120, 0.8, 3.5, 0, 2, true)
        break
      }
    }
  }

  private portalNear(level: Level, x: number, y: number): number {
    let best = 0
    let bd = Infinity
    level.portals.forEach((p, i) => {
      const d = Math.abs(p.x + p.w / 2 - x) + Math.abs(p.y + p.h / 2 - y)
      if (d < bd) {
        bd = d
        best = i
      }
    })
    return C_PORTAL + (this.portalCol[best] ?? 0)
  }

  private confetti(n: number): void {
    const cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR]
    for (let i = 0; i < n; i++) {
      const fromLeft = i % 2 === 0
      this.parts.spawn(PK_CONFETTI, cols[i % cols.length], fromLeft ? rand(-10, 60) : rand(580, 650), rand(250, 370), (fromLeft ? 1 : -1) * rand(60, 200), rand(-420, -260), rand(1.6, 2.6), rand(3, 5), 320, 1.4, false, 9)
    }
  }

  // ------------------------------------------------------------ the frame

  // one frame. events are everything since the last frame; prev/alpha interpolate moving things
  draw(level: Level, prev: GameState | null, cur: GameState, alpha: number, events: GameEvent[], nowMs: number, opts: DrawOpts): void {
    const g = this.ctx
    if (!g) return
    const W = this.canvas.width
    const H = this.canvas.height
    if (W < 2 || H < 2) return
    if (this.level !== level) this.setLevel(level)
    const dt = this.lastMs ? clamp((nowMs - this.lastMs) / 1000, 0, 0.05) : 1 / 60
    this.lastMs = nowMs
    const t = nowMs / 1000
    const a = clamp(alpha, 0, 1)
    const p0 = prev && prev.players ? prev : null

    for (const e of events) this.onEvent(level, cur, e, opts, t)

    // where everything is this frame
    for (let i = 0; i < 2; i++) {
      const c = cur.players[i]
      const p = p0 ? p0.players[i] : null
      const far = !p || Math.abs(p.x - c.x) > 40 || Math.abs(p.y - c.y) > 40
      this.px[i] = far ? c.x : p.x + (c.x - p.x) * a
      this.py[i] = far ? c.y : p.y + (c.y - p.y) * a
    }

    this.ensureStatic(level)

    let sx = 0
    let sy = 0
    if (this.shake > 0 && !opts.reduceMotion) {
      const amp = this.shake * this.shake * 3
      sx = rand(-amp, amp)
      sy = rand(-amp, amp)
    }
    this.shake = Math.max(0, this.shake - dt * 3)

    const k = W / WORLD_W
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalAlpha = 1
    g.globalCompositeOperation = 'source-over'
    g.fillStyle = css(this.pal.tape)
    g.fillRect(0, 0, W, H)
    if (this.stat) g.drawImage(this.stat, Math.round(sx * k), Math.round(sy * k))
    g.setTransform(k, 0, 0, k, sx * k, sy * k)

    this.drawFanColumns(g, level, cur, dt, opts)
    this.drawMoverRails(g, level)
    this.drawSigns(g, level, t)
    this.drawDoors(g, level, cur, t, dt)
    this.drawPortals(g, level, t, opts)
    this.drawPools(g, level, cur, t, dt, opts)
    this.drawPlates(g, level, cur, t)
    this.drawButtons(g, level, cur, t)
    this.drawLevers(g, level, cur, dt)
    this.drawSensors(g, level, cur, t)
    this.drawEmitters(g, level, cur, t)
    this.drawMirrors(g, level, cur, dt)
    this.drawGates(g, level, cur)
    this.drawMovers(g, level, p0, cur, a)
    this.drawIce(g, level, cur, t)
    this.drawBoxes(g, p0, cur, a)
    this.drawGems(g, level, cur, t, opts)
    this.drawBeams(g, level, cur, t, opts)
    for (let i = 0; i < 2; i++) this.drawPlayer(g, i as PlayerIndex, cur, t, dt, opts)

    this.parts.update(dt)
    this.parts.draw(g, this.colors, this.pal.dark ? 'lighter' : 'source-over')

    // celebration keeps fizzing for a moment
    if (cur.phase === 'won' && this.winAt >= 0 && t - this.winAt < 1.6 && Math.random() < (opts.reduceMotion ? 0.15 : 0.5)) this.confetti(2)
    if (cur.phase === 'won' && this.winAt < 0) this.winAt = t

    // a dead attempt: dim the room a little after the splash has had its moment
    if (cur.phase === 'dead') {
      const d = clamp((cur.phaseT - 18) / 30, 0, 1)
      if (d > 0) {
        g.setTransform(1, 0, 0, 1, 0, 0)
        g.fillStyle = this.pal.dark ? `rgba(0, 0, 0, ${(0.3 * d).toFixed(3)})` : `rgba(30, 24, 48, ${(0.16 * d).toFixed(3)})`
        g.fillRect(0, 0, W, H)
      }
    }
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalAlpha = 1
  }

  // ------------------------------------------------------------ pools

  private poolGradient(g: CanvasRenderingContext2D, kind: number): CanvasGradient {
    let gr = this.poolGrad[kind]
    if (gr) return gr
    gr = g.createLinearGradient(0, 0, 0, TILE)
    const [top, bot] = kind === T_LAVA ? [LAVA_TOP, LAVA_BOT] : kind === T_WATER ? [WATER_TOP, WATER_BOT] : kind === T_GOO ? [GOO_TOP, GOO_BOT] : kind === T_WALL ? [ICE_TOP, ICE_BOT] : [THIN_TOP, THIN_BOT]
    const water = kind === T_WATER || kind === T_THIN
    gr.addColorStop(0, css(top, water ? 0.88 : 1))
    if (kind === T_LAVA) gr.addColorStop(0.35, css(LAVA_MID))
    gr.addColorStop(1, css(bot, water ? 0.95 : 1))
    this.poolGrad[kind] = gr
    return gr
  }

  private wave(kind: number, wx: number, t: number): number {
    if (kind === T_LAVA) return Math.sin(wx * 0.22 + t * 2.1) * 0.9 + Math.sin(wx * 0.53 - t * 3.3) * 0.5
    if (kind === T_GOO) return Math.sin(wx * 0.17 + t * 1.3) * 0.7 + Math.sin(wx * 0.41 - t * 1.9) * 0.35
    return Math.sin(wx * 0.19 + t * 2.6) * 0.8 + Math.sin(wx * 0.47 - t * 3.7) * 0.35
  }

  private drawPools(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number, dt: number, opts: DrawOpts): void {
    const tiles = level.tiles
    for (const i of this.pools) {
      const tx = i % COLS
      const ty = (i - tx) / COLS
      const x = tx * TILE
      const y = ty * TILE
      const kind = tiles[i]
      if (kind === T_THIN && tileAt(level, cur, tx, ty) === T_WALL) {
        this.drawFrozen(g, level, cur, i, x, y, t)
        continue
      }
      const open = ty === 0 || tiles[i - COLS] !== kind
      g.save()
      g.translate(x, y)
      g.beginPath()
      if (open) {
        g.moveTo(0, POOL_SINK + this.wave(kind, x, t))
        for (let lx = 2; lx <= TILE; lx += 2) g.lineTo(lx, POOL_SINK + this.wave(kind, x + lx, t))
      } else {
        g.moveTo(0, 0)
        g.lineTo(TILE, 0)
      }
      g.lineTo(TILE, TILE)
      g.lineTo(0, TILE)
      g.closePath()
      g.fillStyle = this.poolGradient(g, kind)
      g.fill()
      // bubbles rising through lava and goo; glints drifting on water
      if (kind === T_LAVA || kind === T_GOO) {
        for (let b = 0; b < 2; b++) {
          const seed = hash01(i * 7 + b)
          const rate = kind === T_LAVA ? 0.55 : 0.35
          const ph = (t * rate + seed) % 1
          const bx = 3 + hash01(i * 13 + b + Math.floor(t * rate + seed) * 31) * (TILE - 6)
          const by = TILE - 2 - ph * (TILE - POOL_SINK - 2) * (open ? 1.05 : 1.2)
          const r = 0.8 + ph * 1.6
          if (by < (open ? POOL_SINK + 1 : 0)) continue
          g.fillStyle = kind === T_LAVA ? 'rgba(255, 236, 160, 0.8)' : 'rgba(214, 255, 150, 0.75)'
          g.beginPath()
          g.arc(bx, by, r, 0, TAU)
          g.fill()
          if (kind === T_GOO) {
            g.strokeStyle = 'rgba(40, 110, 20, 0.5)'
            g.lineWidth = 0.6
            g.stroke()
          }
        }
      } else if (open) {
        for (let b = 0; b < 2; b++) {
          const ph = (t * 0.3 + hash01(i * 5 + b)) % 1
          const gx = (ph * TILE * 1.4 + b * 9) % TILE
          g.fillStyle = `rgba(255, 255, 255, ${(0.5 * Math.sin(ph * Math.PI)).toFixed(3)})`
          g.fillRect(gx, POOL_SINK + 3 + b * 4, 3, 0.8)
        }
      }
      if (open) {
        // the surface line catches the light
        g.beginPath()
        g.moveTo(0, POOL_SINK + this.wave(kind, x, t))
        for (let lx = 2; lx <= TILE; lx += 2) g.lineTo(lx, POOL_SINK + this.wave(kind, x + lx, t))
        g.strokeStyle = kind === T_LAVA ? 'rgba(255, 244, 190, 0.95)' : kind === T_GOO ? 'rgba(220, 255, 160, 0.9)' : 'rgba(235, 248, 255, 0.9)'
        g.lineWidth = 1.2
        g.stroke()
      }
      g.restore()
    }
    // lava glows and spits the odd spark
    this.beginGlow(g)
    for (const i of this.lavaTops) {
      const tx = i % COLS
      const ty = (i - tx) / COLS
      const fl = 0.85 + 0.15 * Math.sin(t * 3 + tx)
      this.glow(g, tx * TILE + TILE / 2, ty * TILE + POOL_SINK + 2, 24, css(LAVA_MID), 0.33 * fl)
    }
    this.endGlow(g)
    if (this.lavaTops.length && Math.random() < this.lavaTops.length * dt * (opts.reduceMotion ? 0.2 : 0.7)) {
      const i = this.lavaTops[Math.floor(Math.random() * this.lavaTops.length)]
      const tx = i % COLS
      const ty = (i - tx) / COLS
      this.parts.spawn(PK_SPARK, Math.random() < 0.5 ? C_CORE : C_FIRE2, tx * TILE + rand(2, 18), ty * TILE + POOL_SINK, rand(-12, 12), rand(-70, -30), rand(0.5, 0.9), rand(0.7, 1.1), 60, 0.5, true)
    }
  }

  // frost's frozen water: solid pale ice that flickers when it's about to thaw
  private drawFrozen(g: CanvasRenderingContext2D, level: Level, cur: GameState, i: number, x: number, y: number, t: number): void {
    const j = this.thinIndex.get(i)
    const left = j === undefined ? 999 : (cur.thin[j] ?? 999)
    const warn = left < 40 ? 0.55 + 0.45 * Math.abs(Math.sin(t * 9)) : 1
    g.save()
    g.translate(x, y)
    g.globalAlpha = warn
    g.fillStyle = this.poolGradient(g, T_WALL)
    const lOpen = this.level?.tiles[i - 1] !== T_THIN && !isWall(level, x / TILE - 1, y / TILE)
    const rOpen = this.level?.tiles[i + 1] !== T_THIN && !isWall(level, x / TILE + 1, y / TILE)
    roundRectPath(g, 0, 0, TILE, TILE, lOpen ? 3 : 0, rOpen ? 3 : 0, 0, 0)
    g.fill()
    g.fillStyle = 'rgba(255, 255, 255, 0.85)'
    g.fillRect(0, 0, TILE, 1.6)
    g.strokeStyle = 'rgba(255, 255, 255, 0.7)'
    g.lineWidth = 0.8
    g.beginPath()
    const h = hash01(i)
    g.moveTo(3 + h * 6, 4)
    g.lineTo(7 + h * 4, 9)
    g.lineTo(5 + h * 8, 15)
    g.moveTo(12 + h * 4, 3)
    g.lineTo(16, 7)
    g.stroke()
    g.strokeStyle = css(this.pal.frost, 0.35)
    g.strokeRect(0.4, 0.4, TILE - 0.8, TILE - 0.8)
    g.restore()
    g.globalAlpha = 1
    if (Math.random() < 0.012) this.parts.spawn(PK_STAR, C_WHITE, x + rand(3, 17), y + rand(2, 12), 0, 0, 0.6, 2.5, 0, 0, true)
  }

  // ------------------------------------------------------------ doors, signs, portals

  private drawDoors(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number, dt: number): void {
    const p = this.pal
    for (let i = 0; i < 2; i++) {
      const d = level.doors[i]
      const fire = i === 0
      const c = fire ? p.fire : p.frost
      const c2 = fire ? p.fire2 : p.frost2
      const inDoor = cur.players[i].inDoor
      this.doorOpen[i] = approach(this.doorOpen[i], inDoor || cur.phase === 'won' ? 1 : 0, 6, dt)
      const o = this.doorOpen[i]
      const cx = d.x + d.w / 2
      // halo
      this.beginGlow(g)
      this.glow(g, cx, d.y + d.h * 0.55, 26 + o * 14, css(c), 0.22 + o * 0.45 + 0.05 * Math.sin(t * 2 + i))
      this.endGlow(g)
      // frame: an arch of little stones
      const frame = p.dark ? mixRgb(p.wall, c, 0.25) : mixRgb(p.wall, c, 0.2)
      archPath(g, d.x - 2.5, d.y - 3, d.w + 5, d.h + 3)
      g.fillStyle = css(frame)
      g.fill()
      g.strokeStyle = css(mixRgb(frame, BLACK, 0.3), 0.6)
      g.lineWidth = 0.6
      g.beginPath()
      for (let k = 1; k < 6; k++) {
        const ang = Math.PI + (k * Math.PI) / 6
        const ax = cx + Math.cos(ang) * (d.w / 2)
        const ay = d.y + d.w / 2 + Math.sin(ang) * (d.w / 2)
        g.moveTo(ax, ay)
        g.lineTo(cx + Math.cos(ang) * (d.w / 2 + 2.5), d.y + d.w / 2 + Math.sin(ang) * (d.w / 2 + 3))
      }
      for (let yy = d.y + d.w / 2 + 6; yy < d.y + d.h - 2; yy += 6) {
        g.moveTo(d.x - 2.5, yy)
        g.lineTo(d.x, yy)
        g.moveTo(d.x + d.w, yy)
        g.lineTo(d.x + d.w + 2.5, yy)
      }
      g.stroke()
      // opening: dark when shut, bright when its player is home
      archPath(g, d.x, d.y, d.w, d.h)
      const inner = g.createLinearGradient(0, d.y, 0, d.y + d.h)
      const shut = p.dark ? mixRgb(c, BLACK, 0.72) : mixRgb(c, p.ink, 0.55)
      inner.addColorStop(0, css(mixRgb(shut, c2, o * 0.9)))
      inner.addColorStop(1, css(mixRgb(mixRgb(shut, BLACK, 0.2), c, o * 0.8)))
      g.fillStyle = inner
      g.fill()
      g.strokeStyle = css(c, 0.45 + o * 0.4)
      g.lineWidth = 0.9
      archPath(g, d.x + 0.5, d.y + 0.5, d.w - 1, d.h - 0.5)
      g.stroke()
      if (o > 0.02) {
        // light spills out of an open door onto the floor
        g.fillStyle = css(c2, 0.35 * o)
        g.beginPath()
        g.ellipse(cx, d.y + d.h, d.w * 0.9, 2.2, 0, 0, TAU)
        g.fill()
      }
      // keystone and step
      g.fillStyle = css(c)
      g.beginPath()
      g.moveTo(cx - 2.5, d.y - 3)
      g.lineTo(cx + 2.5, d.y - 3)
      g.lineTo(cx + 1.6, d.y + 1)
      g.lineTo(cx - 1.6, d.y + 1)
      g.closePath()
      g.fill()
      g.fillStyle = css(c, 0.8)
      g.fillRect(d.x - 3, d.y + d.h - 1.5, d.w + 6, 1.5)
      // the element glyph floats in the doorway
      const gy = d.y + d.h * 0.42 + Math.sin(t * 2.2 + i) * 0.8
      g.globalAlpha = 0.75 + o * 0.25
      if (fire) {
        g.fillStyle = css(o > 0.5 ? WHITE : c2)
        flameGlyph(g, cx, gy, 11)
        g.fill()
        g.fillStyle = css(o > 0.5 ? c2 : c)
        flameGlyph(g, cx + 0.3, gy + 2, 5)
        g.fill()
      } else {
        g.strokeStyle = css(o > 0.5 ? WHITE : c2)
        g.lineWidth = 1.1
        g.lineCap = 'round'
        flakeGlyph(g, cx, gy, 4.6)
        g.stroke()
      }
      g.globalAlpha = 1
    }
  }

  private drawSigns(g: CanvasRenderingContext2D, level: Level, t: number): void {
    if (!level.signs.length) return
    const p = this.pal
    g.font = '9px system-ui, -apple-system, "Helvetica Neue", sans-serif'
    g.textAlign = 'center'
    g.textBaseline = 'middle'
    for (let i = 0; i < level.signs.length; i++) {
      const s = level.signs[i]
      let w = this.textW.get(s.text)
      if (w === undefined) {
        w = g.measureText(s.text).width
        this.textW.set(s.text, w)
      }
      // level.signs are tile-centre px
      const cx = s.x
      const cy = s.y + Math.sin(t * 1.6 + i) * 0.8
      const bw = w + 10
      rr(g, cx - bw / 2, cy - 7, bw, 14, 7)
      g.fillStyle = css(p.bg, p.dark ? 0.82 : 0.88)
      g.fill()
      g.strokeStyle = css(p.ink, 0.12)
      g.lineWidth = 0.8
      g.stroke()
      g.fillStyle = css(p.ink, 0.78)
      g.fillText(s.text, cx, cy + 0.5)
    }
  }

  private drawPortals(g: CanvasRenderingContext2D, level: Level, t: number, opts: DrawOpts): void {
    for (let i = 0; i < level.portals.length; i++) {
      const pt = level.portals[i]
      const col = PORTAL_COLORS[this.portalCol[i] ?? 0]
      const cx = pt.x + pt.w / 2
      const cy = pt.y + pt.h / 2
      const rx = pt.w / 2 - 1
      const ry = pt.h / 2 - 1
      this.beginGlow(g)
      this.glow(g, cx, cy, 30, css(col), 0.45 + 0.1 * Math.sin(t * 3 + i))
      this.endGlow(g)
      // the deep middle
      const gr = g.createRadialGradient(cx, cy, 0, cx, cy, ry)
      gr.addColorStop(0, css(mixRgb(col, BLACK, 0.75)))
      gr.addColorStop(0.7, css(mixRgb(col, BLACK, 0.35)))
      gr.addColorStop(1, css(col))
      g.fillStyle = gr
      g.beginPath()
      g.ellipse(cx, cy, rx, ry, 0, 0, TAU)
      g.fill()
      // swirling dashed rings, alternating directions
      g.lineWidth = 1.2
      for (let r = 0; r < 3; r++) {
        const s = 1 - r * 0.27
        g.strokeStyle = css(mixRgb(col, WHITE, 0.25 + r * 0.25), 0.9 - r * 0.2)
        g.setLineDash([5 * s, 4 * s])
        g.lineDashOffset = (r % 2 ? -1 : 1) * t * (18 + r * 8)
        g.beginPath()
        g.ellipse(cx, cy, rx * s, ry * s, 0, 0, TAU)
        g.stroke()
      }
      g.setLineDash([])
      if (Math.random() < (opts.reduceMotion ? 0.03 : 0.12)) {
        const ang = Math.random() * TAU
        this.parts.spawn(PK_DOT, C_PORTAL + (this.portalCol[i] ?? 0), cx + Math.cos(ang) * rx * 1.3, cy + Math.sin(ang) * ry * 1.1, -Math.cos(ang) * 14, -Math.sin(ang) * 26, 0.6, 1, 0, 0.5, true)
      }
    }
  }

  // ------------------------------------------------------------ switches

  private drawPlates(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number): void {
    for (let i = 0; i < level.plates.length; i++) {
      const pl = level.plates[i]
      const on = cur.plates?.[i] ?? cur.on.includes(pl.ch)
      const col = CHANNEL_COLORS[channelIndex(pl.ch)]
      const floor = pl.y + pl.h
      const h = on ? 1 : 3
      const metal = this.pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL
      if (on) {
        this.beginGlow(g)
        this.glow(g, pl.x + pl.w / 2, floor - 2, pl.w * 0.6 + 8, col, 0.55 + 0.08 * Math.sin(t * 6))
        this.endGlow(g)
      }
      // the cap rises out of a low metal housing; pressed, it sinks flush and lights up
      g.fillStyle = col
      rr(g, pl.x + 2, floor - 2.5 - h, pl.w - 4, h + 1, 1)
      g.fill()
      g.fillStyle = on ? 'rgba(255, 255, 255, 0.6)' : 'rgba(255, 255, 255, 0.3)'
      g.fillRect(pl.x + 3, floor - 2.5 - h, pl.w - 6, 0.8)
      g.fillStyle = css(metal)
      roundRectPath(g, pl.x, floor - 2.5, pl.w, 2.5, 1.2, 1.2, 0, 0)
      g.fill()
      g.fillStyle = on ? col : css(mixRgb(metal, BLACK, 0.3))
      g.fillRect(pl.x + 1.5, floor - 1.4, pl.w - 3, 0.7)
    }
  }

  private drawButtons(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number): void {
    for (let i = 0; i < level.buttons.length; i++) {
      const b = level.buttons[i]
      const left = cur.buttons[i] ?? 0
      const frac = b.ticks > 0 ? clamp(left / b.ticks, 0, 1) : 0
      const on = left > 0
      const col = CHANNEL_COLORS[channelIndex(b.ch)]
      const cx = b.x + b.w / 2
      const floor = b.y + b.h
      g.fillStyle = css(METAL)
      rr(g, b.x + 1, floor - 2.5, b.w - 2, 2.5, 1)
      g.fill()
      const dome = on ? 2 : 4.2
      g.fillStyle = col
      g.beginPath()
      g.ellipse(cx, floor - 2.5, Math.min(b.w / 2 - 3, 7), dome, 0, Math.PI, 0)
      g.fill()
      g.fillStyle = 'rgba(255, 255, 255, 0.45)'
      g.beginPath()
      g.ellipse(cx - 2, floor - 2.5 - dome * 0.55, 2, dome * 0.25, 0, 0, TAU)
      g.fill()
      if (on) {
        // the countdown: a ring that empties as the time runs out
        const ay = floor - 13
        this.beginGlow(g)
        this.glow(g, cx, floor - 3, 14, col, 0.4)
        this.endGlow(g)
        g.lineCap = 'round'
        g.strokeStyle = css(this.pal.ink, 0.15)
        g.lineWidth = 1.6
        g.beginPath()
        g.arc(cx, ay, 4, 0, TAU)
        g.stroke()
        g.strokeStyle = col
        g.lineWidth = frac < 0.25 && Math.sin(t * 20) > 0 ? 2.4 : 1.8
        g.beginPath()
        g.arc(cx, ay, 4, -Math.PI / 2, -Math.PI / 2 + TAU * frac)
        g.stroke()
      }
    }
  }

  private drawLevers(g: CanvasRenderingContext2D, level: Level, cur: GameState, dt: number): void {
    for (let i = 0; i < level.levers.length; i++) {
      const lv = level.levers[i]
      const on = cur.levers[i] ?? lv.on
      this.leverAng[i] = approach(this.leverAng[i] ?? 0, on ? 1 : -1, 14, dt)
      const ang = this.leverAng[i] * 0.65
      const col = CHANNEL_COLORS[channelIndex(lv.ch)]
      const cx = lv.x + lv.w / 2
      const by = lv.y + lv.h - 1
      // handle
      const hx = cx + Math.sin(ang) * 12
      const hy = by - 3 - Math.cos(ang) * 12
      g.lineCap = 'round'
      g.strokeStyle = css(this.pal.dark ? mixRgb(METAL, WHITE, 0.2) : mixRgb(METAL, BLACK, 0.15))
      g.lineWidth = 2
      g.beginPath()
      g.moveTo(cx, by - 3)
      g.lineTo(hx, hy)
      g.stroke()
      g.fillStyle = col
      g.beginPath()
      g.arc(hx, hy, 2.6, 0, TAU)
      g.fill()
      g.fillStyle = 'rgba(255, 255, 255, 0.55)'
      g.beginPath()
      g.arc(hx - 0.8, hy - 0.8, 0.9, 0, TAU)
      g.fill()
      // base: a half-dome with a light that shows the channel is on
      g.fillStyle = css(this.pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL)
      g.beginPath()
      g.ellipse(cx, by, 6.5, 5, 0, Math.PI, 0)
      g.fill()
      g.fillStyle = on ? col : css(this.pal.wallLo)
      g.beginPath()
      g.arc(cx, by - 2, 1.4, 0, TAU)
      g.fill()
      if (on) {
        this.beginGlow(g)
        this.glow(g, cx, by - 2, 8, col, 0.6)
        this.endGlow(g)
      }
    }
  }

  private drawSensors(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number): void {
    for (let i = 0; i < level.sensors.length; i++) {
      const s = level.sensors[i]
      const lit = cur.sensors?.[i] ?? cur.on.includes(s.ch)
      const col = CHANNEL_COLORS[channelIndex(s.ch)]
      const cx = s.x + s.w / 2
      const cy = s.y + s.h / 2
      g.fillStyle = css(this.pal.dark ? mixRgb(METAL, BLACK, 0.35) : METAL)
      rr(g, s.x + 2, s.y + 2, s.w - 4, s.h - 4, 4)
      g.fill()
      g.strokeStyle = col
      g.lineWidth = 1.2
      g.stroke()
      // the lens: a little eye that opens when light reaches it
      g.fillStyle = lit ? css(mixRgb(parseColor(col, WHITE), WHITE, 0.5)) : css(BLACK, 0.55)
      g.beginPath()
      g.arc(cx, cy, 4, 0, TAU)
      g.fill()
      g.fillStyle = col
      g.beginPath()
      g.arc(cx, cy, lit ? 2.4 : 1.4, 0, TAU)
      g.fill()
      if (lit) {
        this.beginGlow(g)
        this.glow(g, cx, cy, 16, col, 0.7 + 0.15 * Math.sin(t * 8))
        this.endGlow(g)
      }
    }
  }

  private drawEmitters(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number): void {
    for (const em of level.emitters) {
      const on = condHolds(em.when, cur.on)
      const ch = condChannels(em.when)[0]
      const rot = em.dir === 'right' ? 0 : em.dir === 'down' ? Math.PI / 2 : em.dir === 'left' ? Math.PI : -Math.PI / 2
      g.save()
      g.translate(em.x, em.y)
      g.rotate(rot)
      g.fillStyle = css(this.pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL)
      rr(g, -8, -7, 13, 14, 3)
      g.fill()
      if (ch) {
        g.fillStyle = channelColorOf(ch)
        g.fillRect(-8, -7, 2.5, 14)
      }
      // the nozzle
      g.fillStyle = css(this.pal.dark ? mixRgb(METAL, WHITE, 0.15) : mixRgb(METAL, BLACK, 0.2))
      g.fillRect(5, -4, 3.5, 8)
      g.fillStyle = on ? this.colors[C_CORE] : css(BLACK, 0.5)
      g.beginPath()
      g.arc(8.5, 0, 2.6, -Math.PI / 2, Math.PI / 2)
      g.fill()
      g.restore()
      if (on) {
        const dx = em.dir === 'right' ? 8 : em.dir === 'left' ? -8 : 0
        const dy = em.dir === 'down' ? 8 : em.dir === 'up' ? -8 : 0
        this.beginGlow(g)
        this.glow(g, em.x + dx, em.y + dy, 12, css(this.pal.star), 0.7 + 0.1 * Math.sin(t * 14))
        this.endGlow(g)
      }
    }
  }

  private drawMirrors(g: CanvasRenderingContext2D, level: Level, cur: GameState, dt: number): void {
    for (let i = 0; i < level.mirrors.length; i++) {
      const m = level.mirrors[i]
      const slant = cur.mirrors[i] ?? m.slant
      this.mirrorAng[i] = approach(this.mirrorAng[i] ?? 0, slant === '/' ? -Math.PI / 4 : Math.PI / 4, 16, dt)
      // the pivot post
      g.fillStyle = css(this.pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL)
      g.fillRect(m.x - 1, m.y, 2, TILE / 2)
      g.beginPath()
      g.arc(m.x, m.y, 2.4, 0, TAU)
      g.fill()
      g.save()
      g.translate(m.x, m.y)
      g.rotate(this.mirrorAng[i])
      // the silvered plank: a dark back and a bright face
      g.fillStyle = css(this.pal.dark ? mixRgb(METAL, BLACK, 0.45) : mixRgb(METAL, BLACK, 0.25))
      rr(g, -12, -2.2, 24, 4.4, 2)
      g.fill()
      const sheen = g.createLinearGradient(-12, 0, 12, 0)
      sheen.addColorStop(0, 'rgba(210, 230, 255, 0.95)')
      sheen.addColorStop(0.5, 'rgba(255, 255, 255, 1)')
      sheen.addColorStop(1, 'rgba(180, 205, 240, 0.95)')
      g.fillStyle = sheen
      rr(g, -11, -2.2, 22, 2.2, 1.1)
      g.fill()
      g.restore()
    }
  }

  // ------------------------------------------------------------ solid moving things

  private drawGates(g: CanvasRenderingContext2D, level: Level, cur: GameState): void {
    const p = this.pal
    for (let i = 0; i < level.gates.length; i++) {
      const home = level.gates[i]
      const r = gateRect(level, cur, i)
      const chs = condChannels(home.open)
      // it slides into the wall it's set in, so clip it to where it lives when shut
      g.save()
      g.beginPath()
      g.rect(home.x, home.y, home.w, home.h)
      g.clip()
      const bar = p.dark ? mixRgb(METAL, BLACK, 0.15) : METAL
      g.fillStyle = css(mixRgb(bar, BLACK, 0.35))
      rr(g, r.x, r.y, r.w, r.h, 2)
      g.fill()
      const vertical = r.h >= r.w
      g.fillStyle = css(bar)
      if (vertical) {
        for (let bx = r.x + 2; bx < r.x + r.w - 1; bx += 5) g.fillRect(bx, r.y + 1, 2.5, r.h - 2)
        g.fillStyle = 'rgba(255, 255, 255, 0.28)'
        for (let bx = r.x + 2; bx < r.x + r.w - 1; bx += 5) g.fillRect(bx, r.y + 1, 0.8, r.h - 2)
      } else {
        for (let by = r.y + 2; by < r.y + r.h - 1; by += 5) g.fillRect(r.x + 1, by, r.w - 2, 2.5)
        g.fillStyle = 'rgba(255, 255, 255, 0.28)'
        for (let by = r.y + 2; by < r.y + r.h - 1; by += 5) g.fillRect(r.x + 1, by, r.w - 2, 0.8)
      }
      // coloured stripes: one per channel it listens to, near its leading edge
      const n = Math.max(1, chs.length)
      for (let c = 0; c < n; c++) {
        g.fillStyle = chs[c] ? channelColorOf(chs[c]) : css(p.muted)
        if (vertical) g.fillRect(r.x, r.y + r.h - 7 - c * 4, r.w, 3)
        else g.fillRect(r.x + r.w - 7 - c * 4, r.y, 3, r.h)
      }
      g.strokeStyle = css(BLACK, p.dark ? 0.45 : 0.25)
      g.lineWidth = 0.8
      rr(g, r.x + 0.4, r.y + 0.4, r.w - 0.8, r.h - 0.8, 2)
      g.stroke()
      g.restore()
    }
  }

  private drawMoverRails(g: CanvasRenderingContext2D, level: Level): void {
    g.lineCap = 'round'
    for (const m of level.movers) {
      const ch = condChannels(m.when)[0]
      g.strokeStyle = ch ? channelColorOf(ch) : css(this.pal.muted)
      g.globalAlpha = 0.45
      g.lineWidth = 1.5
      g.setLineDash([0.1, 5])
      g.beginPath()
      g.moveTo(m.x + m.w / 2, m.y + m.h / 2)
      g.lineTo(m.to.x + m.w / 2, m.to.y + m.h / 2)
      g.stroke()
      g.setLineDash([])
      // end stops
      g.fillStyle = g.strokeStyle
      g.beginPath()
      g.arc(m.x + m.w / 2, m.y + m.h / 2, 1.6, 0, TAU)
      g.arc(m.to.x + m.w / 2, m.to.y + m.h / 2, 1.6, 0, TAU)
      g.fill()
      g.globalAlpha = 1
    }
  }

  private drawMovers(g: CanvasRenderingContext2D, level: Level, prev: GameState | null, cur: GameState, a: number): void {
    const p = this.pal
    for (let i = 0; i < level.movers.length; i++) {
      const m = level.movers[i]
      const c = cur.movers[i]
      if (!c) continue
      const q = prev?.movers?.[i]
      const far = !q || Math.abs(q.x - c.x) > 40 || Math.abs(q.y - c.y) > 40
      const x = far ? c.x : q.x + (c.x - q.x) * a
      const y = far ? c.y : q.y + (c.y - q.y) * a
      const ch = condChannels(m.when)[0]
      const body = p.dark ? mixRgb(p.wall, METAL, 0.5) : mixRgb(p.wall, METAL, 0.45)
      g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.1)
      rr(g, x + 1, y + 2, m.w, m.h, 3)
      g.fill()
      g.fillStyle = css(body)
      rr(g, x, y, m.w, m.h, 3)
      g.fill()
      g.fillStyle = css(p.dark ? mixRgb(body, WHITE, 0.2) : mixRgb(body, WHITE, 0.5))
      g.fillRect(x + 1, y, m.w - 2, 2)
      g.fillStyle = ch ? channelColorOf(ch) : css(p.muted)
      g.fillRect(x + 3, y + 3, m.w - 6, 2)
      // rivets
      g.fillStyle = css(mixRgb(body, BLACK, 0.35))
      for (let rx = x + 4; rx < x + m.w - 2; rx += 10) {
        g.beginPath()
        g.arc(rx, y + m.h - 4, 0.9, 0, TAU)
        g.fill()
      }
    }
  }

  private drawBoxes(g: CanvasRenderingContext2D, prev: GameState | null, cur: GameState, a: number): void {
    const p = this.pal
    const wood = p.dark ? mixRgb(WOOD, BLACK, 0.1) : WOOD
    for (let i = 0; i < cur.boxes.length; i++) {
      const c = cur.boxes[i]
      const q = prev?.boxes?.[i]
      const far = !q || Math.abs(q.x - c.x) > 40 || Math.abs(q.y - c.y) > 40
      const x = far ? c.x : q.x + (c.x - q.x) * a
      const y = far ? c.y : q.y + (c.y - q.y) * a
      const s = TILE
      g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.1)
      rr(g, x + 1, y + 2, s, s, 2)
      g.fill()
      g.fillStyle = css(wood)
      rr(g, x, y, s, s, 2)
      g.fill()
      // planks and a cross brace
      g.strokeStyle = css(WOOD_DARK, 0.7)
      g.lineWidth = 0.8
      g.beginPath()
      g.moveTo(x + 2, y + s / 3)
      g.lineTo(x + s - 2, y + s / 3)
      g.moveTo(x + 2, y + (2 * s) / 3)
      g.lineTo(x + s - 2, y + (2 * s) / 3)
      g.stroke()
      g.strokeStyle = css(mixRgb(wood, WHITE, 0.15))
      g.lineWidth = 2
      g.beginPath()
      g.moveTo(x + 3, y + s - 3)
      g.lineTo(x + s - 3, y + 3)
      g.stroke()
      g.strokeStyle = css(WOOD_DARK)
      g.lineWidth = 1.4
      rr(g, x + 0.7, y + 0.7, s - 1.4, s - 1.4, 2)
      g.stroke()
      g.fillStyle = 'rgba(255, 255, 255, 0.25)'
      g.fillRect(x + 1.5, y + 1.2, s - 3, 1)
    }
  }

  private drawIce(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number): void {
    for (let i = 0; i < level.ice.length; i++) {
      const r = level.ice[i]
      const p = cur.ice[i] ?? 0
      const melting = p > (this.iceLast[i] ?? 0)
      this.iceLast[i] = p
      if (p >= 1) continue
      const jx = melting ? Math.sin(t * 60 + i) * 0.6 : 0
      const x = r.x + jx
      g.save()
      g.globalAlpha = 0.92 - p * 0.35
      const gr = g.createLinearGradient(x, r.y, x + r.w, r.y + r.h)
      gr.addColorStop(0, css(ICE_TOP, 0.92))
      gr.addColorStop(1, css(mixRgb(ICE_BOT, this.pal.frost, 0.3), 0.85))
      g.fillStyle = gr
      rr(g, x, r.y, r.w, r.h, 2.5)
      g.fill()
      // block seams so a big chunk reads as stacked ice bricks
      g.strokeStyle = 'rgba(255, 255, 255, 0.55)'
      g.lineWidth = 0.7
      g.beginPath()
      for (let ty = r.y + TILE; ty < r.y + r.h; ty += TILE) {
        g.moveTo(x + 1, ty)
        g.lineTo(x + r.w - 1, ty)
      }
      for (let tx = x + TILE; tx < x + r.w; tx += TILE) {
        g.moveTo(tx, r.y + 1)
        g.lineTo(tx, r.y + r.h - 1)
      }
      g.stroke()
      // glints
      g.strokeStyle = 'rgba(255, 255, 255, 0.85)'
      g.lineWidth = 1.2
      g.lineCap = 'round'
      g.beginPath()
      g.moveTo(x + 3, r.y + 7)
      g.lineTo(x + 7, r.y + 3)
      g.moveTo(x + 3, r.y + 11)
      g.lineTo(x + 5, r.y + 9)
      g.stroke()
      // cracks spread as it melts
      const cracks = Math.ceil(p * 7)
      if (cracks > 0) {
        g.strokeStyle = css(this.pal.frost, 0.85)
        g.lineWidth = 0.8
        g.beginPath()
        for (let c = 0; c < cracks; c++) {
          let cx = x + hash01(i * 41 + c) * r.w
          let cy = r.y + hash01(i * 43 + c + 7) * r.h
          g.moveTo(cx, cy)
          for (let s = 0; s < 3; s++) {
            cx += (hash01(i * 47 + c * 5 + s) - 0.5) * 10
            cy += (hash01(i * 53 + c * 5 + s) - 0.5) * 10
            g.lineTo(clamp(cx, x + 1, x + r.w - 1), clamp(cy, r.y + 1, r.y + r.h - 1))
          }
        }
        g.stroke()
      }
      g.strokeStyle = css(this.pal.frost, 0.6)
      g.lineWidth = 1
      rr(g, x + 0.5, r.y + 0.5, r.w - 1, r.h - 1, 2.5)
      g.stroke()
      g.restore()
      if (melting && Math.random() < 0.3) this.parts.spawn(PK_DOT, C_WATER, x + rand(1, r.w - 1), r.y + r.h - 1, 0, rand(10, 30), 0.4, 1, 300, 0)
    }
  }

  // ------------------------------------------------------------ fans

  private drawFanColumns(g: CanvasRenderingContext2D, level: Level, cur: GameState, dt: number, opts: DrawOpts): void {
    const p = this.pal
    for (let i = 0; i < level.fans.length; i++) {
      const f = level.fans[i]
      const on = condHolds(f.when, cur.on)
      this.fanSpin[i] = approach(this.fanSpin[i] ?? 0, on ? 1 : 0, on ? 3 : 1.5, dt)
      this.fanAng[i] = (this.fanAng[i] ?? 0) + this.fanSpin[i] * dt * 22
      const spin = this.fanSpin[i]
      const ch = condChannels(f.when)[0]
      const fy = f.y + f.h - TILE
      // the column of moving air: a faint rising tint and streaks
      if (spin > 0.05) {
        const gr = g.createLinearGradient(0, fy, 0, f.y)
        gr.addColorStop(0, css(p.dark ? WHITE : p.frost, 0.09 * spin))
        gr.addColorStop(1, css(p.dark ? WHITE : p.frost, 0))
        g.fillStyle = gr
        g.fillRect(f.x + 2, f.y, f.w - 4, f.h - TILE)
        const rate = (f.w / TILE) * (opts.reduceMotion ? 3 : 10) * spin
        if (Math.random() < rate * dt) this.parts.spawn(PK_STREAK, C_WHITE, f.x + rand(3, f.w - 3), fy, 0, -rand(160, 260), (f.h - TILE) / 220, rand(5, 10), 0, 0, true)
      }
      // the housing in the floor with a rotor behind the grill
      g.fillStyle = css(p.dark ? mixRgb(METAL, BLACK, 0.45) : mixRgb(METAL, BLACK, 0.1))
      rr(g, f.x, fy, f.w, TILE, 3)
      g.fill()
      for (let tx = f.x; tx < f.x + f.w; tx += TILE) {
        const cx = tx + TILE / 2
        const cy = fy + TILE / 2 + 1
        g.fillStyle = css(BLACK, 0.4)
        g.beginPath()
        g.arc(cx, cy, 7.5, 0, TAU)
        g.fill()
        g.fillStyle = css(p.dark ? mixRgb(METAL, WHITE, 0.25) : mixRgb(METAL, WHITE, 0.35))
        for (let b = 0; b < 3; b++) {
          const ang = this.fanAng[i] + (b * TAU) / 3
          g.beginPath()
          g.ellipse(cx + Math.cos(ang) * 3.6, cy + Math.sin(ang) * 3.6, 3.6, 1.6, ang, 0, TAU)
          g.fill()
        }
        g.fillStyle = ch ? channelColorOf(ch) : css(p.muted)
        g.beginPath()
        g.arc(cx, cy, 1.6, 0, TAU)
        g.fill()
      }
      // the grill slats on top
      g.fillStyle = css(p.dark ? mixRgb(METAL, WHITE, 0.1) : METAL)
      g.fillRect(f.x + 1, fy, f.w - 2, 2.2)
      if (ch) {
        g.fillStyle = channelColorOf(ch)
        g.fillRect(f.x + 1, fy + TILE - 2.5, f.w - 2, 1.6)
      }
    }
  }

  // ------------------------------------------------------------ gems, beams

  private drawGems(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number, opts: DrawOpts): void {
    const p = this.pal
    for (let i = 0; i < level.gems.length; i++) {
      if (cur.taken[i]) continue
      const gm = level.gems[i]
      const fire = gm.el === 'fire'
      const c = fire ? p.fire : p.frost
      const c2 = fire ? p.fire2 : p.frost2
      const y = gm.y + Math.sin(t * 2.6 + i * 1.7) * 1.6
      this.beginGlow(g)
      this.glow(g, gm.x, y, 16, css(c), 0.5 + 0.12 * Math.sin(t * 4 + i))
      this.endGlow(g)
      g.save()
      g.translate(gm.x, y)
      // a slow turn: squeeze the width so the facets seem to rotate
      const turn = 0.75 + 0.25 * Math.cos(t * 1.8 + i)
      g.scale(turn, 1)
      gemPath(g, 1)
      g.fillStyle = css(c)
      g.fill()
      // facets: a bright crown, a shaded right pavilion
      g.fillStyle = css(c2)
      g.beginPath()
      g.moveTo(-5, -1.5)
      g.lineTo(-2.8, -4.5)
      g.lineTo(2.8, -4.5)
      g.lineTo(5, -1.5)
      g.closePath()
      g.fill()
      g.fillStyle = css(mixRgb(c, BLACK, 0.25))
      g.beginPath()
      g.moveTo(1.5, -1.5)
      g.lineTo(5, -1.5)
      g.lineTo(0, 5.5)
      g.closePath()
      g.fill()
      g.fillStyle = 'rgba(255, 255, 255, 0.85)'
      g.beginPath()
      g.moveTo(-2.6, -3.9)
      g.lineTo(-0.6, -3.9)
      g.lineTo(-2.4, -1.9)
      g.closePath()
      g.fill()
      g.strokeStyle = css(mixRgb(c, BLACK, 0.4), 0.7)
      g.lineWidth = 0.6
      gemPath(g, 1)
      g.stroke()
      g.restore()
      if (Math.random() < (opts.reduceMotion ? 0.01 : 0.03)) this.parts.spawn(PK_STAR, fire ? C_FIRE2 : C_WHITE, gm.x + rand(-6, 6), y + rand(-6, 5), 0, -4, 0.5, rand(2, 3.2), 0, 0, true)
    }
  }

  private drawBeams(g: CanvasRenderingContext2D, level: Level, cur: GameState, t: number, opts: DrawOpts): void {
    const beams = traceBeams(level, cur)
    if (!beams.length) return
    const p = this.pal
    const star = css(p.star)
    const fl = 0.9 + 0.1 * Math.sin(t * 31)
    g.lineCap = 'round'
    this.beginGlow(g)
    for (const b of beams) {
      g.strokeStyle = star
      g.globalAlpha = (p.dark ? 0.18 : 0.14) * fl
      g.lineWidth = 8
      g.beginPath()
      g.moveTo(b.x1, b.y1)
      g.lineTo(b.x2, b.y2)
      g.stroke()
      g.globalAlpha = (p.dark ? 0.45 : 0.35) * fl
      g.lineWidth = 3.4
      g.stroke()
    }
    this.endGlow(g)
    g.strokeStyle = this.colors[C_CORE]
    g.lineWidth = 1.3
    g.beginPath()
    for (const b of beams) {
      g.moveTo(b.x1, b.y1)
      g.lineTo(b.x2, b.y2)
    }
    g.stroke()
    // a bright splash where each beam lands
    this.beginGlow(g)
    for (const b of beams) this.glow(g, b.x2, b.y2, 9, star, 0.7 * fl)
    this.endGlow(g)
    if (Math.random() < (opts.reduceMotion ? 0.05 : 0.25)) {
      const b = beams[Math.floor(Math.random() * beams.length)]
      this.parts.spawn(PK_STAR, C_STAR, b.x2, b.y2, rand(-20, 20), rand(-20, 20), 0.35, 2.4, 0, 2, true)
    }
  }

  // ------------------------------------------------------------ players

  private drawPlayer(g: CanvasRenderingContext2D, i: PlayerIndex, cur: GameState, t: number, dt: number, opts: DrawOpts): void {
    const pl = cur.players[i]
    const an = this.anims[i]
    const x = this.px[i]
    const y = this.py[i]
    this.animate(an, pl, t, dt)
    if (!pl.alive) return
    const fire = i === 0
    const cx = x + PLAYER_W / 2
    const feet = y + PLAYER_H
    an.x = cx
    an.y = feet
    const p = this.pal
    const moving = Math.abs(pl.vx) > 30
    const bob = pl.ground && moving ? Math.abs(Math.sin(an.run)) * 1.6 : 0
    const breathe = 1 + Math.sin(t * 3.1 + i) * 0.025
    const sy = (1 + an.sq) * breathe
    const sx = 1 - an.sq * 0.55
    // glow
    this.beginGlow(g)
    this.glow(g, cx, feet - 11, fire ? 30 : 26, css(fire ? p.fire : p.frost), fire ? 0.55 + 0.08 * Math.sin(t * 13) : 0.42)
    this.endGlow(g)
    // contact shadow
    if (pl.ground) {
      g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.12)
      g.beginPath()
      g.ellipse(cx, feet, 7 * sx, 1.6, 0, 0, TAU)
      g.fill()
    }
    g.save()
    g.translate(cx, feet - bob)
    g.rotate(an.tilt)
    g.scale(sx, sy)
    const me = opts.me === i
    const happy = cur.phase === 'won' || pl.inDoor
    if (fire) this.drawEmber(g, an, pl, t, me, happy)
    else this.drawFrost(g, an, pl, t, me, happy)
    g.restore()

    // trails: sparks off the flame, snowflakes off the crystal
    const rate = (moving || !pl.ground ? 26 : 0) * (opts.reduceMotion ? 0.3 : 1)
    an.trail += rate * dt
    an.ambient += (fire ? 7 : 3) * (opts.reduceMotion ? 0.3 : 1) * dt
    while (an.trail >= 1) {
      an.trail -= 1
      if (fire) this.parts.spawn(PK_SPARK, Math.random() < 0.5 ? C_FIRE2 : C_CORE, cx - pl.face * rand(2, 6), feet - rand(6, 16), -pl.vx * 0.15 + rand(-10, 10), rand(-40, -10), rand(0.3, 0.55), rand(0.6, 1.1), -40, 1, true)
      else this.parts.spawn(PK_FLAKE, C_FROST2, cx - pl.face * rand(2, 6), feet - rand(4, 18), -pl.vx * 0.12 + rand(-8, 8), rand(-6, 12), rand(0.5, 0.9), rand(1.4, 2.2), 10, 1.5, true, 3)
    }
    while (an.ambient >= 1) {
      an.ambient -= 1
      if (fire) this.parts.spawn(PK_SPARK, C_FIRE2, cx + rand(-3, 3), feet - 22 - bob, rand(-8, 8), rand(-45, -25), rand(0.35, 0.6), rand(0.5, 0.9), -20, 1, true)
      else this.parts.spawn(PK_STAR, C_WHITE, cx + rand(-7, 7), feet - rand(6, 24), 0, -3, 0.45, rand(1.8, 2.6), 0, 0, true)
    }

    // online: point at "you" for the first couple of seconds
    if (me && cur.phase === 'play' && cur.tick < 150) {
      const fade = clamp((150 - cur.tick) / 30, 0, 1)
      const ay = y - 12 + Math.sin(t * 6) * 1.5
      g.globalAlpha = fade
      g.fillStyle = css(p.accent)
      g.beginPath()
      g.moveTo(cx - 4, ay - 4)
      g.lineTo(cx + 4, ay - 4)
      g.lineTo(cx, ay + 1)
      g.closePath()
      g.fill()
      g.font = '600 8px system-ui, -apple-system, sans-serif'
      g.textAlign = 'center'
      g.textBaseline = 'alphabetic'
      g.fillText('you', cx, ay - 6)
      g.globalAlpha = 1
    }
  }

  private animate(an: PAnim, pl: Player, t: number, dt: number): void {
    // squash and stretch: kicked by take-off and landing, settled by a damped spring
    if (an.wasGround && !pl.ground && pl.vy < 0) an.sqv += 5
    if (!an.wasGround && pl.ground) an.sqv -= 6
    an.wasGround = pl.ground
    let left = dt
    while (left > 0) {
      const h = Math.min(left, 1 / 240)
      an.sqv += (-320 * an.sq - 16 * an.sqv) * h
      an.sq += an.sqv * h
      left -= h
    }
    an.sq = clamp(an.sq, -0.32, 0.32)
    if (pl.ground) an.run += dt * Math.abs(pl.vx) * 0.085
    an.tilt = approach(an.tilt, clamp(pl.vx / RUN_SPEED, -1, 1) * 0.12, 10, dt)
    an.look = approach(an.look, pl.face, 12, dt)
    if (t > an.blinkAt) {
      an.blinkUntil = t + 0.11
      an.blinkAt = t + (Math.random() < 0.2 ? 0.25 : 2 + Math.random() * 3)
    }
  }

  // eyes: shared by both, so they read as a pair
  private eyes(g: CanvasRenderingContext2D, an: PAnim, pl: Player, t: number, y: number, ink: string, happy: boolean): void {
    const lx = an.look * 1.7
    const ly = pl.vy < -60 ? -0.7 : pl.vy > 200 ? 0.6 : 0
    const blink = t < an.blinkUntil
    for (let s = -1; s <= 1; s += 2) {
      const ex = s * 2.9 + lx
      const ey = y + ly
      if (happy) {
        g.strokeStyle = ink
        g.lineWidth = 1.1
        g.lineCap = 'round'
        g.beginPath()
        g.arc(ex, ey + 0.8, 1.5, Math.PI * 1.15, Math.PI * 1.85)
        g.stroke()
        continue
      }
      g.fillStyle = ink
      g.beginPath()
      g.ellipse(ex, ey, 1.25, blink ? 0.3 : 2, 0, 0, TAU)
      g.fill()
      if (!blink) {
        g.fillStyle = 'rgba(255, 255, 255, 0.95)'
        g.fillRect(ex - 0.2 + an.look * 0.3, ey - 1.3, 0.75, 0.75)
      }
    }
  }

  private drawEmber(g: CanvasRenderingContext2D, an: PAnim, pl: Player, t: number, me: boolean, happy: boolean): void {
    const p = this.pal
    const fl = Math.sin(t * 17) * 0.9 + Math.sin(t * 11.3 + 1) * 0.6
    const lean = -an.tilt * 40 - clamp(pl.vy / 400, -1, 1) * 0.5
    const tipX = lean + Math.sin(t * 6.3) * 0.9
    const tipY = -25 - fl - Math.max(0, an.sq) * 6
    const fire = css(p.fire)
    // side tufts lick up behind the body
    g.fillStyle = fire
    for (let s = -1; s <= 1; s += 2) {
      const h = 7 + Math.sin(t * 13 + s * 2) * 1.6
      const bx = s * 6
      g.beginPath()
      g.moveTo(bx - 2.6, -9)
      g.quadraticCurveTo(bx + s * 2.5, -11 - h * 0.4, bx + s * 2.2 + tipX * 0.2, -10 - h)
      g.quadraticCurveTo(bx + s * 0.5, -10, bx + 2.6, -8)
      g.closePath()
      g.fill()
    }
    // outer flame
    this.flame(g, 8, -8, tipX, tipY)
    if (me) {
      g.strokeStyle = css(p.accent, 0.7)
      g.lineWidth = 2.4
      g.stroke()
    }
    g.fillStyle = fire
    g.fill()
    // inner and core, a little out of step so it flickers
    this.flame(g, 5.8, -6.6, tipX * 0.75, -19 - Math.sin(t * 19 + 2) * 1.2)
    g.fillStyle = css(p.fire2)
    g.fill()
    this.flame(g, 3.4, -4.8, tipX * 0.5, -12.5 - Math.sin(t * 23) * 0.8)
    g.fillStyle = 'rgba(255, 246, 214, 0.95)'
    g.fill()
    // face
    this.eyes(g, an, pl, t, -10.5, 'rgb(64, 22, 6)', happy)
    // cheeks
    g.fillStyle = 'rgba(255, 90, 60, 0.45)'
    g.beginPath()
    g.ellipse(-5 + an.look * 1.2, -7.6, 1.5, 0.9, 0, 0, TAU)
    g.ellipse(5 + an.look * 1.2, -7.6, 1.5, 0.9, 0, 0, TAU)
    g.fill()
    if (happy || !pl.ground) {
      g.strokeStyle = 'rgb(64, 22, 6)'
      g.lineWidth = 0.9
      g.beginPath()
      if (happy) g.arc(an.look * 1.7, -7.8, 1.6, 0.15 * Math.PI, 0.85 * Math.PI)
      else g.ellipse(an.look * 1.7, -6.8, 0.8, 1.1, 0, 0, TAU)
      g.stroke()
    }
  }

  // a teardrop flame: a round bottom of radius r centred at (0, cy) pulling up into a tip
  private flame(g: CanvasRenderingContext2D, r: number, cy: number, tipX: number, tipY: number): void {
    g.beginPath()
    g.moveTo(r, cy)
    g.arc(0, cy, r, 0, Math.PI)
    g.bezierCurveTo(-r, cy - r * 0.95, tipX - r * 0.45, tipY + r * 1.1, tipX, tipY)
    g.bezierCurveTo(tipX + r * 0.45, tipY + r * 1.1, r, cy - r * 0.95, r, cy)
    g.closePath()
  }

  private drawFrost(g: CanvasRenderingContext2D, an: PAnim, pl: Player, t: number, me: boolean, happy: boolean): void {
    const p = this.pal
    const edge = css(p.frost)
    const pale = mixRgb(p.frost2, WHITE, 0.55)
    const shimmer = (t * 0.6) % 3
    // crystal spikes behind the body
    const spikes: Array<[number, number, number, number]> = [
      [-5.5, -15, -0.5, 7.5],
      [5.5, -15, 0.5, 7.5],
      [0, -17, 0, 10 + Math.max(0, an.sq) * 5],
    ]
    for (let k = 0; k < spikes.length; k++) {
      const [bx, by, ang, len] = spikes[k]
      const tx = bx + Math.sin(ang) * len - an.tilt * 8
      const ty = by - Math.cos(ang) * len
      const w = 2.6
      const nx = Math.cos(ang) * w
      const ny = Math.sin(ang) * w
      g.fillStyle = css(p.frost2)
      g.beginPath()
      g.moveTo(bx - nx, by - ny)
      g.lineTo(tx, ty)
      g.lineTo(bx + nx, by + ny)
      g.closePath()
      g.fill()
      // the lit half of each spike
      g.fillStyle = css(Math.abs(shimmer - k) < 0.4 ? WHITE : pale, 0.9)
      g.beginPath()
      g.moveTo(bx - nx, by - ny)
      g.lineTo(tx, ty)
      g.lineTo(bx, by)
      g.closePath()
      g.fill()
    }
    // body: a rounded hexagonal crystal
    g.beginPath()
    g.moveTo(-7.5, -4)
    g.lineTo(-8.2, -11.5)
    g.lineTo(-4, -18)
    g.lineTo(4, -18)
    g.lineTo(8.2, -11.5)
    g.lineTo(7.5, -4)
    g.quadraticCurveTo(6.5, 0, 0, 0)
    g.quadraticCurveTo(-6.5, 0, -7.5, -4)
    g.closePath()
    g.lineJoin = 'round'
    if (me) {
      g.strokeStyle = css(p.accent, 0.7)
      g.lineWidth = 3.2
      g.stroke()
    }
    const body = g.createLinearGradient(-8, -18, 8, 0)
    body.addColorStop(0, css(pale))
    body.addColorStop(1, css(p.frost2))
    g.fillStyle = body
    g.fill()
    g.strokeStyle = edge
    g.lineWidth = 1.1
    g.stroke()
    // facets
    g.fillStyle = 'rgba(255, 255, 255, 0.6)'
    g.beginPath()
    g.moveTo(-4, -18)
    g.lineTo(0.5, -18)
    g.lineTo(-2.5, -13.5)
    g.lineTo(-8.2, -11.5)
    g.closePath()
    g.fill()
    g.strokeStyle = css(p.frost, 0.35)
    g.lineWidth = 0.6
    g.beginPath()
    g.moveTo(-2.5, -13.5)
    g.lineTo(2.5, -13.5)
    g.lineTo(8.2, -11.5)
    g.moveTo(2.5, -13.5)
    g.lineTo(4, -18)
    g.stroke()
    // face
    this.eyes(g, an, pl, t, -9, 'rgb(14, 48, 84)', happy)
    g.fillStyle = 'rgba(120, 170, 255, 0.45)'
    g.beginPath()
    g.ellipse(-5 + an.look * 1.2, -6.2, 1.4, 0.8, 0, 0, TAU)
    g.ellipse(5 + an.look * 1.2, -6.2, 1.4, 0.8, 0, 0, TAU)
    g.fill()
    if (happy || !pl.ground) {
      g.strokeStyle = 'rgb(14, 48, 84)'
      g.lineWidth = 0.9
      g.beginPath()
      if (happy) g.arc(an.look * 1.7, -6.2, 1.6, 0.15 * Math.PI, 0.85 * Math.PI)
      else g.ellipse(an.look * 1.7, -5.2, 0.8, 1.1, 0, 0, TAU)
      g.stroke()
    }
  }
}

function channelColorOf(name: string): string {
  return CHANNEL_COLORS[channelIndex(name)]
}

// ---------------------------------------------------------------- the select screen's thumbnails

// a small static picture of a level for the select screen
export function drawThumb(canvas: HTMLCanvasElement, level: Level): void {
  const g = canvas.getContext('2d')
  if (!g) return
  const p = readPalette(canvas)
  const k = canvas.width / WORLD_W
  g.setTransform(1, 0, 0, 1, 0, 0)
  g.clearRect(0, 0, canvas.width, canvas.height)
  g.setTransform(k, 0, 0, canvas.height / (ROWS * TILE), 0, 0)
  const sky = g.createLinearGradient(0, 0, 0, ROWS * TILE)
  sky.addColorStop(0, css(mixRgb(p.bg, p.tape, 0.15)))
  sky.addColorStop(1, css(p.dark ? mixRgb(p.tape, BLACK, 0.12) : mixRgb(p.tape, p.wall, 0.15)))
  g.fillStyle = sky
  g.fillRect(0, 0, WORLD_W, ROWS * TILE)
  // walls without texture: at this size the shapes are what matter
  paintWalls(g, level, p, false)
  for (let i = 0; i < level.tiles.length; i++) {
    const t = level.tiles[i]
    if (t !== T_LAVA && t !== T_WATER && t !== T_GOO && t !== T_THIN) continue
    const tx = i % COLS
    const ty = (i - tx) / COLS
    const top = ty > 0 && level.tiles[i - COLS] === t ? 0 : POOL_SINK
    g.fillStyle = css(t === T_LAVA ? LAVA_MID : t === T_WATER ? WATER_TOP : t === T_GOO ? GOO_TOP : THIN_TOP)
    g.fillRect(tx * TILE, ty * TILE + top, TILE, TILE - top)
  }
  const solid = (r: Rect, c: RGB) => {
    g.fillStyle = css(c)
    rr(g, r.x, r.y, r.w, r.h, 2)
    g.fill()
  }
  for (const r of level.ice) solid(r, ICE_BOT)
  for (const gt of level.gates) solid(gt, p.dark ? mixRgb(METAL, BLACK, 0.2) : METAL)
  for (const m of level.movers) solid(m, mixRgb(p.wall, METAL, 0.5))
  for (const b of level.boxes) solid({ x: b.x, y: b.y, w: TILE, h: TILE }, WOOD)
  for (const f of level.fans) solid({ x: f.x, y: f.y + f.h - TILE, w: f.w, h: TILE }, mixRgb(METAL, BLACK, 0.3))
  const pairs: string[] = []
  for (const pt of level.portals) {
    let k2 = pairs.indexOf(pt.pair)
    if (k2 < 0) k2 = pairs.push(pt.pair) - 1
    g.fillStyle = css(PORTAL_COLORS[k2 % PORTAL_COLORS.length])
    g.beginPath()
    g.ellipse(pt.x + pt.w / 2, pt.y + pt.h / 2, pt.w / 2 - 1, pt.h / 2 - 1, 0, 0, TAU)
    g.fill()
  }
  for (const pl of level.plates) solid({ x: pl.x, y: pl.y + pl.h - 4, w: pl.w, h: 4 }, parseColor(channelColorOf(pl.ch), WHITE))
  for (const b of level.buttons) solid({ x: b.x + 3, y: b.y + b.h - 5, w: b.w - 6, h: 5 }, parseColor(channelColorOf(b.ch), WHITE))
  level.doors.forEach((d, i) => {
    const c = i === 0 ? p.fire : p.frost
    archPath(g, d.x - 2, d.y - 2, d.w + 4, d.h + 2)
    g.fillStyle = css(c)
    g.fill()
    archPath(g, d.x + 2, d.y + 2, d.w - 4, d.h - 2)
    g.fillStyle = css(mixRgb(c, BLACK, p.dark ? 0.6 : 0.35))
    g.fill()
  })
  for (const gm of level.gems) {
    g.save()
    g.translate(gm.x, gm.y)
    g.scale(1.4, 1.4)
    gemPath(g, 1)
    g.fillStyle = css(gm.el === 'fire' ? p.fire : p.frost)
    g.fill()
    g.restore()
  }
  // the two heroes as dots where they start
  level.spawns.forEach((s, i) => {
    g.fillStyle = css(i === 0 ? p.fire : p.frost)
    g.beginPath()
    g.arc(s.x + PLAYER_W / 2, s.y + PLAYER_H - 9, 8, 0, TAU)
    g.fill()
  })
  g.setTransform(1, 0, 0, 1, 0, 0)
}
