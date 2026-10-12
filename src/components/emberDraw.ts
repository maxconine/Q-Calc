import { baseView, cameraFor, easeCamera } from '../lib/emberCamera'
import { gateRect, tileAt, traceBeams } from '../lib/emberEngine'
import {
  ARROW_RESPAWN_TICKS,
  CANNON_FLY_TICKS,
  CANNON_FUSE_TICKS,
  CRUMBLE_GONE_TICKS,
  CRUMBLE_TICKS,
  GLOW_SMALL,
  GRAVITY,
  ONEWAY_H,
  PLAYER_H,
  PLAYER_W,
  POOL_SINK,
  RUN_SPEED,
  STAGE_TICKS,
  T_CRUMBLE,
  T_DARK,
  T_GOO,
  T_LAVA,
  T_LIGHT,
  T_ONEWAY,
  T_THIN,
  T_TRAMP,
  T_WALL,
  T_WATER,
  TILE,
} from '../lib/emberTypes'
import type { Beam, Camera, GameEvent, GameState, Player, PlayerIndex, Rect, Stage, Who } from '../lib/emberTypes'
import {
  CHANNEL_COLORS,
  channelIndex,
  CHUNK_PX,
  chunkRange,
  condChannels,
  condHolds,
  css,
  hash01,
  HOLE_CORE,
  inView,
  lavaTops,
  lightHoles,
  luma,
  MAX_PARTICLES,
  mipFor,
  MIPS,
  mixRgb,
  parseColor,
  Particles,
  PK_CHEV,
  PK_CHUNK,
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
import { viewFor, visibleTiles } from './emberFx'
import type { RGB, View } from './emberFx'

// ember & frost on a canvas: tiles, pools, mechanics, the two characters, particles, glow and the dark.
// it draws ONE stage (the panel passes compiled.stages[state.stage]); a `level` in here is a Stage.
// world units throughout: the camera (cameraFor, eased with easeCamera) picks the part of the stage on screen
// and one transform maps it onto a canvas of any size and shape. everything a frame draws is culled to that
// rect, and the backdrop and walls come from chunks baked once per zoom, so a 128x72 stage costs no more per
// frame than one screen does

export type DrawOpts = {
  me: PlayerIndex | null // online: outline/label this side's character; null in local play
  reduceMotion: boolean // fewer particles, no screen shake, fades instead of wipes
}

// drawThumb's extras
export type ThumbOpts = {
  stages?: number // how many stages the level has: more than one shows as pips under the picture
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
// version 2's things keep their own colours in both themes too
const TRAMP: RGB = [238, 78, 104]
const SAND: RGB = [204, 160, 104]
const SUN: RGB = [255, 214, 120]
const MOON: RGB = [128, 116, 226]
const GRAV: RGB = [190, 132, 255]
const ARROW: RGB = [72, 214, 150]
const GOLD: RGB = [246, 196, 62]
const GOLD_DARK: RGB = [150, 104, 22]
const NIGHT = 'rgb(7, 6, 14)'
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
const C_TRAMP = C_CH + CHANNEL_COLORS.length
const C_SAND = C_TRAMP + 1
const C_SUN = C_TRAMP + 2
const C_GRAV = C_TRAMP + 3
const C_ARROW = C_TRAMP + 4
const C_GOLD = C_TRAMP + 5
const C_SMOKE = C_TRAMP + 6

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
  t[C_TRAMP] = css(TRAMP)
  t[C_SAND] = css(p.dark ? SAND : mixRgb(SAND, BLACK, 0.2))
  t[C_SUN] = css(p.dark ? SUN : mixRgb(SUN, LAVA_MID, 0.35))
  t[C_GRAV] = css(p.dark ? GRAV : mixRgb(GRAV, BLACK, 0.12))
  t[C_ARROW] = css(p.dark ? ARROW : mixRgb(ARROW, BLACK, 0.18))
  t[C_GOLD] = css(GOLD)
  t[C_SMOKE] = p.dark ? 'rgb(120, 120, 132)' : 'rgb(96, 96, 108)'
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

function isWall(level: Stage, tx: number, ty: number): boolean {
  if (tx < 0 || ty < 0 || tx >= level.cols || ty >= level.rows) return true
  return level.tiles[ty * level.cols + tx] === T_WALL
}

// a wall's neighbour that hides its edge: more wall, a trampoline, or a pool sunk into it (so basins have square
// rims). blocks that come and go, and one-way slabs, leave the edge showing
function closed(level: Stage, tx: number, ty: number): boolean {
  if (tx < 0 || ty < 0 || tx >= level.cols || ty >= level.rows) return true
  const t = level.tiles[ty * level.cols + tx]
  return t !== 0 && t !== T_ONEWAY && t !== T_CRUMBLE && t !== T_LIGHT && t !== T_DARK
}

type TileRange = { x0: number; y0: number; x1: number; y1: number }

// a key, pointing right: a ring, a shaft and two teeth. centred on (0, 0), about 12 * s long
function keyPath(g: CanvasRenderingContext2D, s: number): void {
  g.beginPath()
  g.arc(-3.4 * s, 0, 2.5 * s, 0, TAU)
  g.moveTo(-0.9 * s, 0)
  g.lineTo(6 * s, 0)
  g.moveTo(3.4 * s, 0)
  g.lineTo(3.4 * s, 2.4 * s)
  g.moveTo(5.6 * s, 0)
  g.lineTo(5.6 * s, 2 * s)
}

// two stacked chevrons pointing up, centred on (0, 0)
function chevrons(g: CanvasRenderingContext2D, s: number): void {
  g.beginPath()
  g.moveTo(-3.6 * s, 0.2 * s)
  g.lineTo(0, -3.4 * s)
  g.lineTo(3.6 * s, 0.2 * s)
  g.moveTo(-3.6 * s, 3.8 * s)
  g.lineTo(0, 0.2 * s)
  g.lineTo(3.6 * s, 3.8 * s)
}

// ---------------------------------------------------------------- the static layer: backdrop + walls

function paintBackdrop(g: CanvasRenderingContext2D, level: Stage, p: Palette, range?: TileRange): void {
  const W = level.w
  const H = level.h
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
  const r0 = range ? Math.max(0, range.y0) : 0
  const r1 = range ? Math.min(level.rows, range.y1) : level.rows
  const bx0 = range ? Math.max(0, Math.floor((range.x0 * TILE) / bw) - 1) * bw : 0
  const bx1 = range ? Math.min(W, range.x1 * TILE) : W
  for (let row = r0; row < r1; row++) {
    const off = row % 2 ? bw / 2 : 0
    for (let bx = bx0 - off; bx < bx1; bx += bw) {
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
function paintWalls(g: CanvasRenderingContext2D, level: Stage, p: Palette, detail: boolean, range?: TileRange): void {
  const R = 5
  // a range may reach past the stage: everything out there is wall
  const tx0 = range ? range.x0 : 0
  const ty0 = range ? range.y0 : 0
  const tx1 = range ? range.x1 : level.cols
  const ty1 = range ? range.y1 : level.rows
  // a soft drop shadow first, so it sits under every block
  g.fillStyle = p.dark ? 'rgba(0, 0, 0, 0.32)' : 'rgba(40, 30, 70, 0.10)'
  for (let ty = ty0; ty < ty1; ty++)
    for (let tx = tx0; tx < tx1; tx++) {
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
  for (let ty = ty0; ty < ty1; ty++)
    for (let tx = tx0; tx < tx1; tx++) {
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
        const j = ty & 1 ? 0 : TILE / 2
        g.fillRect(x + j - 0.5, y, 1, TILE / 2)
        g.fillRect(x + ((j + TILE / 2) % TILE) - 0.5, y + TILE / 2, 1, TILE / 2)
        g.fillStyle = speck
        for (let k = 0; k < 3; k++) {
          const n = hash01(ty * level.cols * 7 + tx * 7 + k)
          const m = hash01(ty * level.cols * 11 + tx * 5 + k + 999)
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
  flip: number // 0 upright .. 1 upside down, turning over when gravity flips
  aura: number
}

function newAnim(): PAnim {
  return { sq: 0, sqv: 0, run: 0, tilt: 0, look: 1, blinkAt: 1 + Math.random() * 2, blinkUntil: 0, wasGround: true, trail: 0, ambient: 0, x: 0, y: 0, flip: 0, aura: 0 }
}

// ---------------------------------------------------------------- the renderer

export class EmberRenderer {
  private canvas: HTMLCanvasElement
  private ctx: CanvasRenderingContext2D | null
  private pal: Palette
  private colors: string[]
  private parts = new Particles(MAX_PARTICLES)
  // the static layer: backdrop and walls baked in chunks, per zoom, a few at a time as the camera moves
  private chunks = new Map<string, HTMLCanvasElement>()
  private spare: HTMLCanvasElement[] = []
  private statKey = ''
  private mip = -1
  private cam: Camera | null = null
  private view: View = { k: 1, ox: 0, oy: 0 }
  // what the canvas shows this frame, in world px (the camera, shaken and rounded): everything is culled to it
  private vr: Camera = { x: 0, y: 0, w: 640, h: 360, zoom: 1 }
  private far = false // zoomed far out: liquids and small details are drawn plainly
  private camState = { players: [{ x: 0, y: 0 }, { x: 0, y: 0 }] }
  private pos = [{ x: 0, y: 0 }, { x: 0, y: 0 }]
  private beams: Beam[] = []
  // the dark
  private mask: HTMLCanvasElement | null = null
  private hole: HTMLCanvasElement | null = null
  private darkA = 0 // 0 lights on .. 1 off, eased
  private glowBoost = 0 // the glows start wide and close in as the lights go out
  private flash = 0
  private lampMax = 0
  // wipes: the iris opens on a new stage and after a respawn
  private irisAt = -9
  private irisDur = 0.5
  // per-thing animation
  private crumbleIndex = new Map<number, number>()
  private trampAt = new Map<number, number>()
  private mendAt = new Map<number, number>()
  private cpAt: number[] = []
  private cpUp: number[] = []
  private lockGone: number[] = []
  private cannonKick: number[] = []
  private gswSpin: number[] = []
  private lampOn: number[] = []
  private keyHolder: PlayerIndex = 0
  private keyTrail: Array<{ x: number; y: number }> = []
  private vyRel = 0
  private glowCache = new Map<string, HTMLCanvasElement>()
  private poolGrad: Array<CanvasGradient | null> = []
  private level: Stage | null = null
  // per-level lists built once so a frame doesn't have to search the map
  private lavaTops: number[] = [] // lava tiles with open sky, for sparks
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

  // match the canvas backing store to its css box, whatever size and shape that is, at devicePixelRatio;
  // call on mount and resize
  resize(): void {
    const r = this.canvas.getBoundingClientRect()
    if (r.width < 1 || r.height < 1) return
    const dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1))
    const w = Math.max(2, Math.round(r.width * dpr))
    const h = Math.max(2, Math.round(r.height * dpr))
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w
      this.canvas.height = h
      this.statKey = ''
      // a new shape is a new camera
      this.cam = null
    }
  }

  // forget particles and effects (new level)
  reset(): void {
    this.parts.clear()
    this.shake = 0
    this.winAt = -1
    this.level = null
    this.lastMs = 0
    this.cam = null
    this.irisAt = -9
    this.flash = 0
    this.glowBoost = 0
  }

  // re-read css colour tokens (theme change)
  refreshColors(): void {
    this.pal = readPalette(this.canvas)
    this.colors = colorTable(this.pal)
    this.glowCache.clear()
    this.poolGrad = []
    this.statKey = ''
    this.hole = null
  }

  private setLevel(level: Stage): void {
    this.level = level
    this.statKey = ''
    this.cam = null
    this.lavaTops = lavaTops(level)
    this.thinIndex.clear()
    level.thin.forEach((ti, j) => this.thinIndex.set(ti, j))
    this.crumbleIndex.clear()
    level.crumble.forEach((ti, j) => this.crumbleIndex.set(ti, j))
    this.trampAt.clear()
    this.mendAt.clear()
    this.cpAt = level.checkpoints.map(() => -9)
    this.cpUp = level.checkpoints.map(() => 0)
    this.lockGone = level.locks.map(() => 0)
    this.cannonKick = level.cannons.map(() => 0)
    this.gswSpin = level.gswitches.map(() => 0)
    this.lampOn = level.lamps.map(() => 0)
    this.lampMax = level.lamps.reduce((m, l) => Math.max(m, l.ticks), 0)
    this.keyTrail = []
    this.keyHolder = 0
    this.flash = 0
    this.glowBoost = 0
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

  // whether a world rect is on screen (pad in px, for glows and shadows that spill out of it)
  private vis(x: number, y: number, w: number, h: number, pad = 24): boolean {
    return inView(this.vr, x, y, w, h, pad)
  }

  private bakeChunk(level: Stage, kb: number, ix: number, iy: number): HTMLCanvasElement {
    const c = this.spare.pop() ?? document.createElement('canvas')
    if (c.width !== CHUNK_PX) c.width = CHUNK_PX
    if (c.height !== CHUNK_PX) c.height = CHUNK_PX
    const g = c.getContext('2d')
    if (!g) return c
    const p = this.pal
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalAlpha = 1
    g.fillStyle = css(p.wall)
    g.fillRect(0, 0, CHUNK_PX, CHUNK_PX)
    g.setTransform(kb, 0, 0, kb, -ix * CHUNK_PX, -iy * CHUNK_PX)
    const w = CHUNK_PX / kb
    // a tile more all round: a block's shadow falls into the chunk next door
    const range = { x0: Math.floor((ix * w) / TILE) - 1, y0: Math.floor((iy * w) / TILE) - 1, x1: Math.ceil(((ix + 1) * w) / TILE) + 1, y1: Math.ceil(((iy + 1) * w) / TILE) + 1 }
    if (range.x1 > 0 && range.y1 > 0 && range.x0 < level.cols && range.y0 < level.rows) {
      g.save()
      g.beginPath()
      g.rect(0, 0, level.w, level.h)
      g.clip()
      paintBackdrop(g, level, p, range)
      g.restore()
    }
    paintWalls(g, level, p, kb >= 1.2, range)
    return c
  }

  // the backdrop and walls under the camera, from the chunk cache. at rest on a baked zoom the chunks land
  // pixel for pixel; while the camera zooms they are drawn a little scaled
  private drawStatic(g: CanvasRenderingContext2D, level: Stage, W: number, H: number): void {
    const key = `${this.pal.key}|${W}x${H}`
    if (key !== this.statKey) {
      for (const c of this.chunks.values()) if (this.spare.length < 8) this.spare.push(c)
      this.chunks.clear()
      this.statKey = key
      this.mip = -1
    }
    const v = this.view
    const k1 = W / baseView(W / H).w
    this.mip = mipFor(v.k / k1, this.mip)
    const kb = k1 * MIPS[this.mip]
    const s = v.k / kb
    const r = chunkRange(this.vr, kb)
    const cap = Math.ceil((2.4 * W * H) / (CHUNK_PX * CHUNK_PX)) + 8
    for (let iy = r.y0; iy < r.y1; iy++)
      for (let ix = r.x0; ix < r.x1; ix++) {
        const id = `${this.mip}|${ix}|${iy}`
        let c = this.chunks.get(id)
        if (c) this.chunks.delete(id)
        else c = this.bakeChunk(level, kb, ix, iy)
        // most recently used last
        this.chunks.set(id, c)
        const x0 = Math.round(ix * CHUNK_PX * s) + v.ox
        const y0 = Math.round(iy * CHUNK_PX * s) + v.oy
        const x1 = Math.round((ix + 1) * CHUNK_PX * s) + v.ox
        const y1 = Math.round((iy + 1) * CHUNK_PX * s) + v.oy
        g.drawImage(c, x0, y0, x1 - x0, y1 - y0)
      }
    if (this.chunks.size > cap) {
      for (const [id, c] of this.chunks) {
        if (this.chunks.size <= cap) break
        this.chunks.delete(id)
        if (this.spare.length < 8) this.spare.push(c)
      }
    }
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

  private onEvent(level: Stage, cur: GameState, e: GameEvent, opts: DrawOpts, t: number): void {
    const less = opts.reduceMotion ? 0.4 : 1
    const n = (k: number) => Math.max(1, Math.round(k * less))
    switch (e.k) {
      case 'jump': {
        // gv: which way this player falls. the dust comes off whatever they stood on
        const pl = cur.players[e.p]
        const gv = pl.grav === -1 ? -1 : 1
        const fx = pl.x + PLAYER_W / 2
        const fy = gv > 0 ? pl.y + PLAYER_H : pl.y
        for (let i = 0; i < n(5); i++) this.parts.spawn(PK_DOT, C_DUST, fx + rand(-5, 5), fy - gv, rand(-30, 30), gv * rand(-25, -5), rand(0.25, 0.4), rand(1.2, 2), gv * 40, 4)
        if (e.p === 0) this.burst(PK_SPARK, C_FIRE2, fx, fy - gv * 4, n(4), 50, 0.35, 0.9, -60, 3, true)
        else this.burst(PK_FLAKE, C_FROST2, fx, fy - gv * 4, n(3), 40, 0.5, 2, 20, 3, true, 4)
        break
      }
      case 'land': {
        const pl = cur.players[e.p]
        const gv = pl.grav === -1 ? -1 : 1
        const fx = pl.x + PLAYER_W / 2
        const fy = gv > 0 ? pl.y + PLAYER_H : pl.y
        for (let i = 0; i < n(7); i++) {
          const s = i % 2 ? 1 : -1
          this.parts.spawn(PK_DOT, C_DUST, fx + s * rand(2, 6), fy - gv, s * rand(25, 60), gv * rand(-18, -4), rand(0.25, 0.45), rand(1.2, 2.2), gv * 60, 5)
        }
        break
      }
      case 'airjump': {
        // the spare jump goes off under the feet: a ring and a puff of chevrons kicked away
        const pl = cur.players[e.p]
        const gv = pl.grav === -1 ? -1 : 1
        const fx = pl.x + PLAYER_W / 2
        const fy = gv > 0 ? pl.y + PLAYER_H : pl.y
        this.burst(PK_RING, C_ARROW, fx, fy, 1, 0, 0.4, 14, 0, 0, true)
        for (let i = 0; i < n(5); i++) this.parts.spawn(PK_CHEV, C_ARROW, fx + rand(-7, 7), fy + gv * rand(0, 4), rand(-12, 12), gv * rand(50, 110), rand(0.3, 0.5), rand(2, 3), 0, 2.5, true)
        this.burst(PK_STAR, C_WHITE, fx, fy, n(4), 60, 0.4, 2.6, 0, 3, true)
        break
      }
      case 'bounce': {
        const ti = this.trampNear(level, e.x, e.y)
        if (ti >= 0) this.trampAt.set(ti, t)
        const pl = cur.players[e.p]
        const cx = pl.x + PLAYER_W / 2
        const dir = pl.vy > 0 ? 1 : -1 // the way they left
        const fy = dir < 0 ? pl.y + PLAYER_H : pl.y
        const strong = clamp((e.v - 420) / 280, 0.2, 1)
        this.burst(PK_RING, C_TRAMP, cx, fy, 1, 0, 0.35, 10 + strong * 10, 0, 0, true)
        // a launch streak: lines left hanging where they took off
        for (let i = 0; i < n(4 + strong * 8); i++) this.parts.spawn(PK_STREAK, i % 3 ? C_WHITE : C_TRAMP, cx + rand(-6, 6), fy + dir * rand(0, 26) - (dir < 0 ? 12 : 0), 0, dir * rand(60, 160) * strong, rand(0.25, 0.45), rand(8, 16), 0, 1.5, true)
        for (let i = 0; i < n(6); i++) this.parts.spawn(PK_DOT, C_TRAMP, cx + rand(-8, 8), fy, rand(-70, 70), dir * rand(20, 90), rand(0.25, 0.45), rand(1, 1.8), -dir * 200, 2)
        break
      }
      case 'arrow': {
        const a = level.arrows[e.i] ?? e
        this.burst(PK_RING, C_ARROW, a.x, a.y, 1, 0, 0.45, 18, 0, 0, true)
        this.burst(PK_STAR, C_ARROW, a.x, a.y, n(8), 100, 0.5, 3.2, 0, 3, true)
        for (let i = 0; i < n(6); i++) this.parts.spawn(PK_CHEV, i % 2 ? C_WHITE : C_ARROW, a.x + rand(-8, 8), a.y + rand(-4, 6), rand(-20, 20), -rand(60, 130), rand(0.35, 0.6), rand(2, 3.2), 0, 2, true)
        break
      }
      case 'arrowback': {
        const a = level.arrows[e.i] ?? e
        this.burst(PK_RING, C_ARROW, a.x, a.y, 1, 0, 0.35, 11, 0, 0, true)
        this.burst(PK_STAR, C_WHITE, a.x, a.y, n(3), 30, 0.4, 2.4, 0, 3, true)
        break
      }
      case 'key': {
        this.keyHolder = e.p
        const a = level.keys[e.i] ?? e
        this.burst(PK_RING, C_GOLD, a.x, a.y, 1, 0, 0.45, 16, 0, 0, true)
        this.burst(PK_STAR, C_GOLD, a.x, a.y, n(9), 100, 0.6, 3.2, 0, 3, true)
        break
      }
      case 'unlock': {
        const r = level.locks[e.i]
        const cx = r ? r.x + r.w / 2 : e.x
        const cy = r ? r.y + r.h / 2 : e.y
        const hw = r ? r.w / 2 : TILE / 2
        const hh = r ? r.h / 2 : TILE / 2
        const area = (hw * hh * 4) / (TILE * TILE)
        for (let i = 0; i < n(Math.min(26, 6 + area * 4)); i++) this.parts.spawn(PK_CHUNK, i % 3 ? C_SMOKE : C_GOLD, cx + rand(-hw, hw), cy + rand(-hh, hh), rand(-90, 90), rand(-150, -20), rand(0.5, 0.9), rand(2, 3.6), 520, 1, false, 10)
        this.burst(PK_RING, C_GOLD, cx, cy, 1, 0, 0.5, 14 + Math.max(hw, hh), 0, 0, true)
        this.burst(PK_STAR, C_GOLD, cx, cy, n(8), 90, 0.6, 3.2, 0, 3, true)
        break
      }
      case 'load': {
        const c = level.cannons[e.i] ?? e
        this.burst(PK_RING, e.p === 0 ? C_FIRE : C_FROST, c.x, c.y, 1, 0, 0.3, 13, 0, 0, true)
        this.burst(PK_DOT, C_SMOKE, c.x, c.y, n(5), 40, 0.35, 1.6, 0, 4)
        break
      }
      case 'fire': {
        const c = level.cannons[e.i]
        if (this.cannonKick[e.i] !== undefined) this.cannonKick[e.i] = 1
        if (!opts.reduceMotion) this.shake = Math.max(this.shake, 0.55)
        const len = c ? Math.hypot(c.vx, c.vy) || 1 : 1
        const ux = c ? c.vx / len : 0
        const uy = c ? c.vy / len : -1
        const mx = (c ? c.x : e.x) + ux * 13
        const my = (c ? c.y : e.y) + uy * 13
        for (let i = 0; i < n(14); i++) {
          const sp = rand(120, 320)
          const side = rand(-0.5, 0.5)
          this.parts.spawn(PK_SPARK, i % 2 ? C_CORE : e.p === 0 ? C_FIRE2 : C_FROST2, mx, my, (ux - uy * side) * sp, (uy + ux * side) * sp, rand(0.25, 0.5), rand(0.8, 1.4), 0, 3, true)
        }
        for (let i = 0; i < n(9); i++) this.parts.spawn(PK_STEAM, C_SMOKE, mx + rand(-3, 3), my + rand(-3, 3), ux * rand(20, 80) + rand(-20, 20), uy * rand(20, 80) + rand(-20, 20), rand(0.5, 0.9), rand(3, 6), -14, 2)
        this.burst(PK_RING, C_WHITE, mx, my, 1, 0, 0.3, 16, 0, 0, true)
        break
      }
      case 'flip': {
        // chevrons pour the way gravity now pulls
        const pl = cur.players[e.p]
        const cx = pl.x + PLAYER_W / 2
        const cy = pl.y + PLAYER_H / 2
        this.burst(PK_RING, C_GRAV, cx, cy, 1, 0, 0.5, 22, 0, 0, true)
        this.burst(PK_RING, C_WHITE, cx, cy, 1, 0, 0.3, 12, 0, 0, true)
        for (let i = 0; i < n(12); i++) this.parts.spawn(PK_CHEV, i % 3 ? C_GRAV : C_WHITE, cx + rand(-11, 11), cy + rand(-12, 12), 0, e.grav * rand(70, 170), rand(0.4, 0.7), rand(2.2, 3.4), 0, 1.5, true)
        break
      }
      case 'gswitch': {
        if (this.gswSpin[e.i] !== undefined) this.gswSpin[e.i] = 1
        this.burst(PK_RING, C_GRAV, e.x, e.y, 1, 0, 0.4, 15, 0, 0, true)
        this.burst(PK_SPARK, C_GRAV, e.x, e.y, n(8), 90, 0.4, 1, 0, 2, true)
        break
      }
      case 'checkpoint': {
        const c = level.checkpoints[e.i]
        if (this.cpAt[e.i] !== undefined) this.cpAt[e.i] = t
        const fx = c ? c.x : e.x
        const fy = c ? c.y - 20 : e.y
        this.burst(PK_RING, C_STAR, fx, fy, 1, 0, 0.6, 26, 0, 0, true)
        this.burst(PK_STAR, C_STAR, fx, fy, n(10), 110, 0.7, 3.4, 0, 2.5, true)
        const cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR]
        for (let i = 0; i < n(16); i++) this.parts.spawn(PK_CONFETTI, cols[i % cols.length], fx + rand(-4, 4), fy, rand(-80, 80), rand(-220, -110), rand(0.9, 1.4), rand(2.4, 3.6), 380, 1.6, false, 9)
        break
      }
      case 'light': {
        if (e.on) {
          // the lights snap on: a flash, and a ring racing out from the lamp
          if (!opts.reduceMotion) this.flash = 1
          this.glowBoost = 0
          this.burst(PK_RING, C_SUN, e.x, e.y, 1, 0, 0.5, 70, 0, 0, true)
          this.burst(PK_RING, C_WHITE, e.x, e.y, 1, 0, 0.35, 34, 0, 0, true)
          this.burst(PK_STAR, C_SUN, e.x, e.y, n(8), 120, 0.5, 3, 0, 3, true)
        } else {
          // and off: the dark rushes in on each player's glow
          this.glowBoost = opts.reduceMotion ? 0 : 1
          this.flash = 0
          this.burst(PK_RING, C_GRAV, e.x, e.y, 1, 0, 0.4, 26, 0, 0, true)
          this.burst(PK_SPARK, C_SUN, e.x, e.y, n(6), 60, 0.5, 0.9, 160, 2, true)
        }
        break
      }
      case 'shake': {
        for (let i = 0; i < n(4); i++) this.parts.spawn(PK_DOT, C_SAND, e.x + rand(-9, 9), e.y + rand(-2, 9), rand(-14, 14), rand(5, 30), rand(0.3, 0.5), rand(1, 1.7), 260, 1)
        break
      }
      case 'break': {
        for (let i = 0; i < n(7); i++) this.parts.spawn(PK_CHUNK, C_SAND, e.x + rand(-8, 8), e.y + rand(-8, 8), rand(-50, 50), rand(-60, 30), rand(0.55, 0.95), rand(2.6, 5), 620, 0.6, false, 9)
        for (let i = 0; i < n(5); i++) this.parts.spawn(PK_STEAM, C_DUST, e.x + rand(-8, 8), e.y + rand(-4, 6), rand(-20, 20), rand(-16, 6), rand(0.4, 0.7), rand(2.5, 4.5), 0, 2)
        break
      }
      case 'mend': {
        this.mendAt.set(e.tile, t)
        this.burst(PK_STAR, C_SAND, e.x, e.y, n(3), 30, 0.4, 2.6, 0, 3, true)
        break
      }
      case 'clear': {
        for (const d of level.doors) {
          this.burst(PK_STAR, C_STAR, d.x + d.w / 2, d.y + d.h / 2, n(8), 110, 0.7, 3.2, 0, 2, true)
          this.burst(PK_RING, C_STAR, d.x + d.w / 2, d.y + d.h / 2, 1, 0, 0.6, 28, 0, 0, true)
        }
        break
      }
      case 'respawn': {
        // back at the flag: the view opens on them and each pops in with a ring
        this.irisAt = t
        this.irisDur = 0.38
        this.keyTrail = []
        for (let i = 0; i < 2; i++) {
          const pl = cur.players[i]
          const cx = pl.x + PLAYER_W / 2
          const cy = pl.y + PLAYER_H / 2
          this.burst(PK_RING, i === 0 ? C_FIRE : C_FROST, cx, cy, 1, 0, 0.5, 20, 0, 0, true)
          this.burst(PK_STAR, i === 0 ? C_FIRE2 : C_FROST2, cx, cy, n(6), 70, 0.5, 2.8, 0, 3, true)
        }
        if (this.cpAt[e.cp] !== undefined) this.cpAt[e.cp] = t
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
        } else if (e.cause === 'crush') {
          // caught in a block: flattened, and what's left squirts out sideways
          const c1 = e.p === 0 ? C_FIRE : C_FROST
          const c2 = e.p === 0 ? C_FIRE2 : C_FROST2
          for (let i = 0; i < n(22); i++) {
            const s = i % 2 ? 1 : -1
            this.parts.spawn(i % 3 ? PK_DOT : PK_CHUNK, i % 2 ? c1 : c2, cx + s * rand(0, 5), cy + rand(-3, 3), s * rand(80, 260), rand(-30, 30), rand(0.35, 0.7), rand(1.4, 2.8), 300, 2.5, false, 10)
          }
          for (let i = 0; i < n(8); i++) this.parts.spawn(PK_STEAM, C_DUST, cx + rand(-10, 10), cy + rand(-10, 10), rand(-30, 30), rand(-20, 10), rand(0.4, 0.7), rand(3, 5), 0, 2)
          this.burst(PK_RING, C_WHITE, cx, cy, 1, 0, 0.3, 18, 0, 0, true)
          this.burst(PK_STAR, C_WHITE, cx, cy, n(4), 60, 0.35, 3, 0, 3, true)
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

  private portalNear(level: Stage, x: number, y: number): number {
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

  // the trampoline tile a bounce came off: the nearest one to the event, a tile or two away at most
  private trampNear(level: Stage, x: number, y: number): number {
    const cx = Math.floor(x / TILE)
    const cy = Math.floor(y / TILE)
    let best = -1
    let bd = Infinity
    for (let ty = cy - 2; ty <= cy + 2; ty++)
      for (let tx = cx - 1; tx <= cx + 1; tx++) {
        if (tx < 0 || ty < 0 || tx >= level.cols || ty >= level.rows || level.tiles[ty * level.cols + tx] !== T_TRAMP) continue
        const d = Math.abs((tx + 0.5) * TILE - x) + Math.abs((ty + 0.5) * TILE - y)
        if (d < bd) {
          bd = d
          best = ty * level.cols + tx
        }
      }
    return best
  }

  // from the bottom corners of whatever is on screen, sized to it
  private confetti(n: number): void {
    const cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR]
    const v = this.vr
    const u = v.w / 640
    for (let i = 0; i < n; i++) {
      const fromLeft = i % 2 === 0
      const x = fromLeft ? v.x + rand(-10, 60) * u : v.x + v.w - rand(-10, 60) * u
      this.parts.spawn(PK_CONFETTI, cols[i % cols.length], x, v.y + v.h - rand(-10, 110) * u, (fromLeft ? 1 : -1) * rand(60, 200) * u, rand(-420, -260) * u, rand(1.6, 2.6), rand(3, 5) * u, 320 * u, 1.4, false, 9)
    }
  }

  // ------------------------------------------------------------ the frame

  // one frame. events are everything since the last frame; prev/alpha interpolate moving things
  draw(level: Stage, prev: GameState | null, cur: GameState, alpha: number, events: GameEvent[], nowMs: number, opts: DrawOpts): void {
    const g = this.ctx
    if (!g) return
    const W = this.canvas.width
    const H = this.canvas.height
    if (W < 2 || H < 2) return
    const dt = this.lastMs ? clamp((nowMs - this.lastMs) / 1000, 0, 0.05) : 1 / 60
    this.lastMs = nowMs
    const t = nowMs / 1000
    const fresh = this.level !== level
    if (fresh) {
      // a stage after the first opens with the iris; the first one of a level just begins
      if (this.level || level.index > 0) {
        this.irisAt = t
        this.irisDur = 0.5
      }
      this.setLevel(level)
      this.darkA = cur.lit ? 0 : 1
    }
    const a = clamp(alpha, 0, 1)
    const p0 = prev && prev.players ? prev : null

    let snap = fresh
    for (const e of events) {
      if (e.k === 'respawn' || e.k === 'stage') snap = true
      this.onEvent(level, cur, e, opts, t)
    }
    if (snap) this.cam = null

    // where everything is this frame
    for (let i = 0; i < 2; i++) {
      const c = cur.players[i]
      const p = p0 ? p0.players[i] : null
      const far = !p || Math.abs(p.x - c.x) > 40 || Math.abs(p.y - c.y) > 40
      this.px[i] = far ? c.x : p.x + (c.x - p.x) * a
      this.py[i] = far ? c.y : p.y + (c.y - p.y) * a
      this.pos[i].x = this.camState.players[i].x = this.px[i]
      this.pos[i].y = this.camState.players[i].y = this.py[i]
    }

    // the camera: where it wants to be for the players as drawn, closed on at the sim's pace
    const target = cameraFor(level, this.camState as unknown as GameState, W / H)
    const cam = (this.cam = easeCamera(this.cam, target, dt * 60))

    let sx = 0
    let sy = 0
    if (this.shake > 0 && !opts.reduceMotion) {
      const amp = (this.shake * this.shake * 3) / Math.max(0.4, cam.zoom)
      sx = rand(-amp, amp)
      sy = rand(-amp, amp)
    }
    this.shake = Math.max(0, this.shake - dt * 3)

    const v = (this.view = viewFor(cam, W, sx, sy))
    const vr = this.vr
    vr.x = -v.ox / v.k
    vr.y = -v.oy / v.k
    vr.w = W / v.k
    vr.h = H / v.k
    vr.zoom = cam.zoom
    this.far = cam.zoom < 0.55
    this.beams = level.emitters.length ? traceBeams(level, cur) : []

    // the lights: ease the dark in and out; a timed lamp about to run out makes them stutter
    this.darkA = approach(this.darkA, cur.lit ? 0 : 1, cur.lit ? 16 : 9, dt)
    if (Math.abs(this.darkA - (cur.lit ? 0 : 1)) < 0.004) this.darkA = cur.lit ? 0 : 1
    this.glowBoost = Math.max(0, this.glowBoost - dt * 3.2)
    this.flash = Math.max(0, this.flash - dt * 4.5)

    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalAlpha = 1
    g.globalCompositeOperation = 'source-over'
    g.fillStyle = css(this.pal.wall)
    g.fillRect(0, 0, W, H)
    this.drawStatic(g, level, W, H)
    g.setTransform(v.k, 0, 0, v.k, v.ox, v.oy)

    this.drawFanColumns(g, level, cur, dt, opts)
    this.drawMoverRails(g, level)
    this.drawTiles(g, level, cur, t, opts)
    this.drawSigns(g, level, t)
    this.drawCheckpoints(g, level, cur, t, dt)
    this.drawDoors(g, level, cur, t, dt)
    this.drawPortals(g, level, t, opts)
    this.drawPools(g, level, cur, t, dt, opts)
    this.drawPlates(g, level, cur, t)
    this.drawButtons(g, level, cur, t)
    this.drawLevers(g, level, cur, dt)
    this.drawLamps(g, level, cur, t, dt)
    this.drawGswitches(g, level, t, dt)
    this.drawSensors(g, level, cur, t)
    this.drawEmitters(g, level, cur, t)
    this.drawMirrors(g, level, cur, dt)
    this.drawGates(g, level, cur)
    this.drawLocks(g, level, cur, t, dt)
    this.drawMovers(g, level, p0, cur, a)
    this.drawIce(g, level, cur, t)
    this.drawBoxes(g, p0, cur, a)
    this.drawGems(g, level, cur, t, opts)
    this.drawKeys(g, level, cur, t, opts)
    this.drawArrows(g, level, cur, t, opts)
    this.drawBeams(g, t, opts)
    this.drawCannons(g, level, cur, t, dt, opts)
    for (let i = 0; i < 2; i++) this.drawPlayer(g, i as PlayerIndex, cur, t, dt, opts)
    this.drawCarried(g, cur, t, dt)

    // plain particles sit in the world and the dark covers them; the glowing ones are drawn over it
    this.parts.update(dt)
    this.parts.draw(g, this.colors, 'source-over', 0)
    this.drawDarkness(g, level, cur, t, W, H, opts)
    g.setTransform(v.k, 0, 0, v.k, v.ox, v.oy)
    this.parts.draw(g, this.colors, this.pal.dark ? 'lighter' : 'source-over', 1)

    // celebration keeps fizzing for a moment
    if (cur.phase === 'won' && this.winAt >= 0 && t - this.winAt < 1.6 && Math.random() < (opts.reduceMotion ? 0.15 : 0.5)) this.confetti(2)
    if (cur.phase === 'won' && this.winAt < 0) this.winAt = t

    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalAlpha = 1
    g.globalCompositeOperation = 'source-over'
    // a dead attempt: dim the room a little after the splash has had its moment
    if (cur.phase === 'dead') {
      const d = clamp((cur.phaseT - 18) / 30, 0, 1)
      if (d > 0) {
        g.fillStyle = this.pal.dark ? `rgba(0, 0, 0, ${(0.3 * d).toFixed(3)})` : `rgba(30, 24, 48, ${(0.16 * d).toFixed(3)})`
        g.fillRect(0, 0, W, H)
      }
    }
    if (this.flash > 0.01) {
      g.fillStyle = `rgba(255, 244, 214, ${(0.3 * this.flash * this.flash).toFixed(3)})`
      g.fillRect(0, 0, W, H)
    }
    this.drawLampClock(g, cur, t, W, H)
    this.drawWipe(g, level, cur, a, t, W, H, opts)
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalAlpha = 1
  }

  // ------------------------------------------------------------ the dark

  // the soft-edged hole a light cuts: solid out to HOLE_CORE of its radius, then fading away
  private holeSprite(): HTMLCanvasElement {
    if (this.hole) return this.hole
    const s = document.createElement('canvas')
    s.width = s.height = 128
    const g = s.getContext('2d')
    if (g) {
      const gr = g.createRadialGradient(64, 64, 0, 64, 64, 64)
      gr.addColorStop(0, 'rgba(0, 0, 0, 1)')
      gr.addColorStop(HOLE_CORE, 'rgba(0, 0, 0, 1)')
      gr.addColorStop(HOLE_CORE + (1 - HOLE_CORE) * 0.5, 'rgba(0, 0, 0, 0.42)')
      gr.addColorStop(1, 'rgba(0, 0, 0, 0)')
      g.fillStyle = gr
      g.fillRect(0, 0, 128, 128)
    }
    this.hole = s
    return s
  }

  // lights off: black over the dark patches (or everything), with a hole for each player's glow and for
  // whatever lights itself. built small on its own canvas (the dark is soft, a quarter of the pixels is
  // plenty) and laid over the frame
  private drawDarkness(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, W: number, H: number, opts: DrawOpts): void {
    let lvl = this.darkA
    // a timed lamp's last second and a half: the lights stutter towards what's coming
    if (cur.litT > 0 && cur.litT < 90 && !opts.reduceMotion) {
      const near = 1 - cur.litT / 90
      const blink = Math.sin(t * (14 + near * 22)) > 0.35 ? 0.1 + 0.3 * near : 0
      lvl = cur.lit ? Math.max(lvl, blink) : Math.min(lvl, 1 - blink)
    }
    if (lvl < 0.004) return
    const q = 4
    const mw = Math.ceil(W / q)
    const mh = Math.ceil(H / q)
    if (!this.mask) this.mask = document.createElement('canvas')
    const mc = this.mask
    if (mc.width !== mw || mc.height !== mh) {
      mc.width = mw
      mc.height = mh
    }
    const m = mc.getContext('2d')
    if (!m) return
    const v = this.view
    const k = v.k / q
    const ox = v.ox / q
    const oy = v.oy / q
    m.setTransform(1, 0, 0, 1, 0, 0)
    m.globalCompositeOperation = 'source-over'
    m.globalAlpha = 1
    m.clearRect(0, 0, mw, mh)
    m.fillStyle = NIGHT
    if (level.darks.length) {
      for (const r of level.darks) if (this.vis(r.x, r.y, r.w, r.h, 0)) m.fillRect(r.x * k + ox, r.y * k + oy, r.w * k, r.h * k)
    } else m.fillRect(0, 0, mw, mh)
    m.globalCompositeOperation = 'destination-out'
    const sprite = this.holeSprite()
    const boost = 1 + 2.2 * this.glowBoost * this.glowBoost
    const holes = lightHoles(level, cur, this.pos)
    for (const h of holes) {
      let r = h.r
      if (h.p !== undefined) r *= boost * (h.p === 0 && !opts.reduceMotion ? 1 + 0.025 * Math.sin(t * 9) + 0.012 * Math.sin(t * 23.7) : 1)
      if (!this.vis(h.x - r, h.y - r, 2 * r, 2 * r, 0)) continue
      m.globalAlpha = h.a
      m.drawImage(sprite, (h.x - r) * k + ox, (h.y - r) * k + oy, 2 * r * k, 2 * r * k)
    }
    if (this.beams.length) {
      m.setTransform(k, 0, 0, k, ox, oy)
      m.strokeStyle = '#000'
      m.lineCap = 'round'
      for (let pass = 0; pass < 2; pass++) {
        m.globalAlpha = pass ? 0.6 : 0.3
        m.lineWidth = pass ? GLOW_SMALL * 0.6 : GLOW_SMALL * 1.5
        m.beginPath()
        for (const b of this.beams) {
          m.moveTo(b.x1, b.y1)
          m.lineTo(b.x2, b.y2)
        }
        m.stroke()
      }
    }
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalCompositeOperation = 'source-over'
    g.globalAlpha = lvl
    g.imageSmoothingEnabled = true
    g.drawImage(mc, 0, 0, mw, mh, 0, 0, mw * q, mh * q)
    g.globalAlpha = 1
    // each glow has its colour: ember's wide and warm, frost's close and cool
    g.setTransform(v.k, 0, 0, v.k, v.ox, v.oy)
    this.beginGlow(g)
    for (const h of holes) {
      if (h.p === undefined) break
      const r = h.r * boost
      if (!this.vis(h.x - r, h.y - r, 2 * r, 2 * r, 0)) continue
      this.glow(g, h.x, h.y, r * 1.05, css(h.p === 0 ? this.pal.fire : this.pal.frost), (h.p === 0 ? 0.3 : 0.26) * lvl)
    }
    this.endGlow(g)
  }

  // a timed lamp counting down, pinned to the top of the screen so the one running in the dark can see it too
  private drawLampClock(g: CanvasRenderingContext2D, cur: GameState, t: number, W: number, H: number): void {
    if (!(cur.litT > 0) || this.lampMax <= 0) return
    const u = Math.max(1, Math.min(W / 640, H / 360))
    const frac = clamp(cur.litT / this.lampMax, 0, 1)
    const w = 84 * u
    const x = W / 2 - w / 2
    const y = 9 * u
    const col = cur.lit ? SUN : MOON
    const low = frac < 0.25 && Math.sin(t * 20) > 0
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.fillStyle = 'rgba(0, 0, 0, 0.45)'
    rr(g, x - 9 * u, y - 4 * u, w + 13 * u, 8 * u, 4 * u)
    g.fill()
    g.fillStyle = css(col, 0.28)
    rr(g, x, y - 1.5 * u, w, 3 * u, 1.5 * u)
    g.fill()
    g.fillStyle = css(low ? WHITE : col)
    rr(g, x, y - 1.5 * u, Math.max(3 * u, w * frac), 3 * u, 1.5 * u)
    g.fill()
    // a little bulb (lights on for now) or moon (off for now) says which way it will go
    g.beginPath()
    g.arc(x - 4.5 * u, y, 2.4 * u, 0, TAU)
    g.fill()
    if (!cur.lit) {
      g.fillStyle = 'rgba(0, 0, 0, 0.75)'
      g.beginPath()
      g.arc(x - 3.5 * u, y - 0.8 * u, 2 * u, 0, TAU)
      g.fill()
    }
  }

  // the wipes: an iris closing on the doors when a stage is done, and opening on the players when the next
  // begins or they come back at a checkpoint. with reduced motion, a plain fade
  private drawWipe(g: CanvasRenderingContext2D, level: Stage, cur: GameState, a: number, t: number, W: number, H: number, opts: DrawOpts): void {
    let f = 0 // 0 open .. 1 shut
    let wx = 0
    let wy = 0
    if (cur.phase === 'clear') {
      f = clamp((cur.phaseT + a - 26) / (STAGE_TICKS - 32), 0, 1)
      wx = (level.doors[0].x + level.doors[1].x + TILE) / 2
      wy = (level.doors[0].y + level.doors[1].y) / 2 + TILE
    } else if (t - this.irisAt < this.irisDur && t >= this.irisAt) {
      f = 1 - (t - this.irisAt) / this.irisDur
      wx = (this.px[0] + this.px[1] + PLAYER_W) / 2
      wy = (this.py[0] + this.py[1] + PLAYER_H) / 2
    }
    if (f <= 0.001) return
    const v = this.view
    const cx = wx * v.k + v.ox
    const cy = wy * v.k + v.oy
    const dark = this.pal.dark || this.darkA > 0.5
    const col: RGB = dark ? [8, 7, 14] : mixRgb(this.pal.tape, this.pal.wall, 0.35)
    g.setTransform(1, 0, 0, 1, 0, 0)
    if (opts.reduceMotion) {
      g.fillStyle = css(col, f)
      g.fillRect(0, 0, W, H)
      return
    }
    const reach = Math.hypot(Math.max(cx, W - cx), Math.max(cy, H - cy))
    const e = f * f * (3 - 2 * f)
    const r = reach * (1 - e)
    g.fillStyle = css(col)
    g.beginPath()
    g.rect(0, 0, W, H)
    if (r > 0.5) g.arc(cx, cy, r, 0, TAU, true)
    g.fill('evenodd')
    if (r > 0.5) {
      // a rim in the two colours, so the closing eye belongs to the game
      g.lineWidth = Math.max(2, v.k * 1.4)
      g.strokeStyle = css(this.pal.fire, 0.8)
      g.beginPath()
      g.arc(cx, cy, r, Math.PI * 0.5, Math.PI * 1.5)
      g.stroke()
      g.strokeStyle = css(this.pal.frost, 0.8)
      g.beginPath()
      g.arc(cx, cy, r, Math.PI * 1.5, Math.PI * 2.5)
      g.stroke()
    }
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

  private drawPools(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, dt: number, opts: DrawOpts): void {
    const tiles = level.tiles
    const vt = visibleTiles(this.vr, level, 1)
    const far = this.far
    for (let ty = vt.y0; ty < vt.y1; ty++)
    for (let tx = vt.x0; tx < vt.x1; tx++) {
      const i = ty * level.cols + tx
      const kind = tiles[i]
      if (kind !== T_LAVA && kind !== T_WATER && kind !== T_GOO && kind !== T_THIN) continue
      const x = tx * TILE
      const y = ty * TILE
      if (kind === T_THIN && tileAt(level, cur, tx, ty) === T_WALL) {
        this.drawFrozen(g, level, cur, i, x, y, t)
        continue
      }
      const open = ty === 0 || tiles[i - level.cols] !== kind
      if (far) {
        // from far off a pool is its colour and a bright line: no waves, no bubbles
        g.fillStyle = css(kind === T_LAVA ? LAVA_MID : kind === T_WATER ? WATER_BOT : kind === T_GOO ? GOO_BOT : THIN_BOT, kind === T_THIN ? 0.9 : 1)
        g.fillRect(x, y + (open ? POOL_SINK : 0), TILE + 0.5, TILE - (open ? POOL_SINK : 0) + 0.5)
        if (open) {
          g.fillStyle = css(kind === T_LAVA ? LAVA_TOP : kind === T_WATER ? WATER_TOP : kind === T_GOO ? GOO_TOP : THIN_TOP)
          g.fillRect(x, y + POOL_SINK, TILE + 0.5, 2.5)
        }
        continue
      }
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
    let seen = 0
    for (const i of this.lavaTops) {
      const tx = i % level.cols
      const ty = (i - tx) / level.cols
      if (tx < vt.x0 || tx >= vt.x1 || ty < vt.y0 || ty >= vt.y1) continue
      seen++
      // far off, every other tile carries the glow for two
      if (far && tx & 1) continue
      const fl = 0.85 + 0.15 * Math.sin(t * 3 + tx)
      this.glow(g, tx * TILE + TILE / 2, ty * TILE + POOL_SINK + 2, far ? 36 : 24, css(LAVA_MID), 0.33 * fl)
    }
    this.endGlow(g)
    // sparks keep the rate one screen's worth of lava always had, however much of it there is
    if (seen && Math.random() < Math.min(seen, 40) * dt * (opts.reduceMotion ? 0.2 : 0.7)) {
      const i = this.lavaTops[Math.floor(Math.random() * this.lavaTops.length)]
      const tx = i % level.cols
      const ty = (i - tx) / level.cols
      if (tx >= vt.x0 && tx < vt.x1 && ty >= vt.y0 && ty < vt.y1)
        this.parts.spawn(PK_SPARK, Math.random() < 0.5 ? C_CORE : C_FIRE2, tx * TILE + rand(2, 18), ty * TILE + POOL_SINK, rand(-12, 12), rand(-70, -30), rand(0.5, 0.9), rand(0.7, 1.1), 60, 0.5, true)
    }
  }

  // frost's frozen water: solid pale ice that flickers when it's about to thaw
  private drawFrozen(g: CanvasRenderingContext2D, level: Stage, cur: GameState, i: number, x: number, y: number, t: number): void {
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

  private drawDoors(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, dt: number): void {
    const p = this.pal
    for (let i = 0; i < 2; i++) {
      const d = level.doors[i]
      const fire = i === 0
      const c = fire ? p.fire : p.frost
      const c2 = fire ? p.fire2 : p.frost2
      const inDoor = cur.players[i].inDoor
      this.doorOpen[i] = approach(this.doorOpen[i], inDoor || cur.phase === 'won' || cur.phase === 'clear' ? 1 : 0, 6, dt)
      if (!this.vis(d.x, d.y, d.w, d.h, 44)) continue
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

  private drawSigns(g: CanvasRenderingContext2D, level: Stage, t: number): void {
    if (!level.signs.length) return
    const p = this.pal
    g.font = '9px system-ui, -apple-system, "Helvetica Neue", sans-serif'
    g.textAlign = 'center'
    g.textBaseline = 'middle'
    for (let i = 0; i < level.signs.length; i++) {
      const s = level.signs[i]
      if (!this.vis(s.x - 100, s.y - 10, 200, 20)) continue
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

  private drawPortals(g: CanvasRenderingContext2D, level: Stage, t: number, opts: DrawOpts): void {
    for (let i = 0; i < level.portals.length; i++) {
      const pt = level.portals[i]
      if (!this.vis(pt.x, pt.y, pt.w, pt.h, 34)) continue
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

  private drawPlates(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number): void {
    for (let i = 0; i < level.plates.length; i++) {
      const pl = level.plates[i]
      if (!this.vis(pl.x, pl.y, pl.w, pl.h)) continue
      const on = cur.plates?.[i] ?? cur.on.includes(pl.ch)
      const col = CHANNEL_COLORS[channelIndex(pl.ch)]
      const floor = pl.y + pl.h
      // on a ceiling: the same plate, hanging upside down from the top of its row
      if (pl.ceil) {
        g.save()
        g.translate(0, 2 * pl.y + pl.h)
        g.scale(1, -1)
      }
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
      if (pl.ceil) g.restore()
    }
  }

  private drawButtons(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number): void {
    for (let i = 0; i < level.buttons.length; i++) {
      const b = level.buttons[i]
      if (!this.vis(b.x, b.y, b.w, b.h)) continue
      const left = cur.buttons[i] ?? 0
      const frac = b.ticks > 0 ? clamp(left / b.ticks, 0, 1) : 0
      const on = left > 0
      const col = CHANNEL_COLORS[channelIndex(b.ch)]
      const cx = b.x + b.w / 2
      const floor = b.y + b.h
      if (b.ceil) {
        g.save()
        g.translate(0, 2 * b.y + b.h)
        g.scale(1, -1)
      }
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
      if (b.ceil) g.restore()
    }
  }

  private drawLevers(g: CanvasRenderingContext2D, level: Stage, cur: GameState, dt: number): void {
    for (let i = 0; i < level.levers.length; i++) {
      const lv = level.levers[i]
      const on = cur.levers[i] ?? lv.on
      this.leverAng[i] = approach(this.leverAng[i] ?? 0, on ? 1 : -1, 14, dt)
      if (!this.vis(lv.x, lv.y, lv.w, lv.h)) continue
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

  private drawSensors(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number): void {
    for (let i = 0; i < level.sensors.length; i++) {
      const s = level.sensors[i]
      if (!this.vis(s.x, s.y, s.w, s.h)) continue
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

  private drawEmitters(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number): void {
    for (const em of level.emitters) {
      if (!this.vis(em.x - 10, em.y - 10, 20, 20)) continue
      const on = condHolds(em.when, cur.on, cur.lit)
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

  private drawMirrors(g: CanvasRenderingContext2D, level: Stage, cur: GameState, dt: number): void {
    for (let i = 0; i < level.mirrors.length; i++) {
      const m = level.mirrors[i]
      const slant = cur.mirrors[i] ?? m.slant
      this.mirrorAng[i] = approach(this.mirrorAng[i] ?? 0, slant === '/' ? -Math.PI / 4 : Math.PI / 4, 16, dt)
      if (!this.vis(m.x - 12, m.y - 12, 24, 24)) continue
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

  private drawGates(g: CanvasRenderingContext2D, level: Stage, cur: GameState): void {
    const p = this.pal
    for (let i = 0; i < level.gates.length; i++) {
      const home = level.gates[i]
      if (!this.vis(home.x, home.y, home.w, home.h, 4)) continue
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

  private drawMoverRails(g: CanvasRenderingContext2D, level: Stage): void {
    g.lineCap = 'round'
    for (const m of level.movers) {
      if (!this.vis(Math.min(m.x, m.to.x), Math.min(m.y, m.to.y), Math.abs(m.to.x - m.x) + m.w, Math.abs(m.to.y - m.y) + m.h, 4)) continue
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

  private drawMovers(g: CanvasRenderingContext2D, level: Stage, prev: GameState | null, cur: GameState, a: number): void {
    const p = this.pal
    for (let i = 0; i < level.movers.length; i++) {
      const m = level.movers[i]
      const c = cur.movers[i]
      if (!c) continue
      const q = prev?.movers?.[i]
      const far = !q || Math.abs(q.x - c.x) > 40 || Math.abs(q.y - c.y) > 40
      const x = far ? c.x : q.x + (c.x - q.x) * a
      const y = far ? c.y : q.y + (c.y - q.y) * a
      if (!this.vis(x, y, m.w, m.h, 4)) continue
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
      if (!this.vis(x, y, s, s, 4)) continue
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

  private drawIce(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number): void {
    for (let i = 0; i < level.ice.length; i++) {
      const r = level.ice[i]
      const p = cur.ice[i] ?? 0
      const melting = p > (this.iceLast[i] ?? 0)
      this.iceLast[i] = p
      if (p >= 1 || !this.vis(r.x, r.y, r.w, r.h, 4)) continue
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

  // ------------------------------------------------------------ version 2's tiles

  // trampolines, one-way slabs, crumbling blocks and the blocks that follow the lights: one walk over the
  // tiles on screen
  private drawTiles(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, opts: DrawOpts): void {
    const tiles = level.tiles
    const vt = visibleTiles(this.vr, level, 1)
    const p = this.pal
    // how much of a light-only block is there: follows the dark as it eases in and out
    const lit = 1 - this.darkA
    for (let ty = vt.y0; ty < vt.y1; ty++)
      for (let tx = vt.x0; tx < vt.x1; tx++) {
        const i = ty * level.cols + tx
        const kind = tiles[i]
        if (kind < T_TRAMP) continue
        const x = tx * TILE
        const y = ty * TILE
        if (kind === T_TRAMP) this.drawTramp(g, level, i, tx, ty, t)
        else if (kind === T_ONEWAY) {
          const l = tx > 0 && tiles[i - 1] !== 0 && tiles[i - 1] !== T_CRUMBLE
          const r = tx < level.cols - 1 && tiles[i + 1] !== 0 && tiles[i + 1] !== T_CRUMBLE
          const wood = p.dark ? mixRgb(WOOD, p.wall, 0.25) : mixRgb(WOOD, p.wallHi, 0.2)
          g.fillStyle = css(BLACK, p.dark ? 0.28 : 0.1)
          g.fillRect(x + 1, y + 2, TILE, ONEWAY_H)
          g.fillStyle = css(wood)
          roundRectPath(g, x, y, TILE + (r ? 0.4 : 0), ONEWAY_H, l ? 0 : 2.5, r ? 0 : 2.5, r ? 0 : 2.5, l ? 0 : 2.5)
          g.fill()
          g.fillStyle = css(mixRgb(wood, WHITE, p.dark ? 0.35 : 0.5))
          g.fillRect(x + (l ? 0 : 1.5), y, TILE - (l ? 0 : 1.5) - (r ? 0 : 1.5), 1.4)
          g.fillStyle = css(WOOD_DARK, 0.7)
          g.fillRect(x + (l ? 0 : 1.5), y + ONEWAY_H - 1.2, TILE - (l ? 0 : 1.5) - (r ? 0 : 1.5), 1.2)
          if (!this.far) {
            // a faint chevron underneath: you can come up through it
            g.strokeStyle = css(p.ink, 0.2)
            g.lineWidth = 1
            g.beginPath()
            g.moveTo(x + 7, y + 12.5)
            g.lineTo(x + 10, y + 9.5)
            g.lineTo(x + 13, y + 12.5)
            g.stroke()
          }
        } else if (kind === T_CRUMBLE) this.drawCrumble(g, cur, i, x, y, t, opts)
        else {
          // 'O' is there with the lights on, 'D' with them off; the other time each is a dashed ghost
          const sun = kind === T_LIGHT
          const a = sun ? lit : 1 - lit
          const col = sun ? SUN : MOON
          const cx = x + TILE / 2
          const cy = y + TILE / 2
          if (a < 0.97) {
            g.globalAlpha = 1 - a
            g.fillStyle = css(col, p.dark ? 0.07 : 0.1)
            rr(g, x + 1.5, y + 1.5, TILE - 3, TILE - 3, 3)
            g.fill()
            g.strokeStyle = css(p.dark ? col : mixRgb(col, BLACK, 0.3), 0.75)
            g.lineWidth = 1
            g.setLineDash([3, 2.5])
            g.stroke()
            g.setLineDash([])
            if (!this.far) this.lightGlyph(g, sun, cx, cy, css(p.dark ? col : mixRgb(col, BLACK, 0.3), 0.6))
            g.globalAlpha = 1
          }
          if (a > 0.03) {
            const k = 0.8 + 0.2 * a
            g.save()
            g.translate(cx, cy)
            g.scale(k, k)
            g.globalAlpha = a
            g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.1)
            rr(g, -TILE / 2 + 1, -TILE / 2 + 2, TILE, TILE, 3)
            g.fill()
            const body = sun ? mixRgb(SUN, WHITE, 0.35) : p.dark ? mixRgb(MOON, BLACK, 0.45) : mixRgb(MOON, BLACK, 0.3)
            g.fillStyle = css(body)
            rr(g, -TILE / 2, -TILE / 2, TILE, TILE, 3)
            g.fill()
            g.fillStyle = css(WHITE, sun ? 0.7 : 0.22)
            g.fillRect(-TILE / 2 + 2, -TILE / 2, TILE - 4, 2.2)
            g.fillStyle = css(sun ? mixRgb(SUN, LAVA_MID, 0.5) : BLACK, sun ? 0.55 : 0.35)
            g.fillRect(-TILE / 2 + 2, TILE / 2 - 2.2, TILE - 4, 2.2)
            g.strokeStyle = css(sun ? mixRgb(SUN, LAVA_MID, 0.55) : mixRgb(MOON, WHITE, 0.3), 0.9)
            g.lineWidth = 1
            rr(g, -TILE / 2 + 0.5, -TILE / 2 + 0.5, TILE - 1, TILE - 1, 3)
            g.stroke()
            if (!this.far) this.lightGlyph(g, sun, 0, 0, sun ? css(mixRgb(SUN, LAVA_BOT, 0.55)) : css(mixRgb(MOON, WHITE, 0.6)))
            g.restore()
            g.globalAlpha = 1
          }
        }
      }
  }

  // a little sun or a crescent moon, the mark of a block that follows the lights
  private lightGlyph(g: CanvasRenderingContext2D, sun: boolean, cx: number, cy: number, color: string): void {
    g.fillStyle = color
    g.strokeStyle = color
    if (sun) {
      g.beginPath()
      g.arc(cx, cy, 2.4, 0, TAU)
      g.fill()
      g.lineWidth = 1
      g.lineCap = 'round'
      g.beginPath()
      for (let k = 0; k < 8; k++) {
        const an = (k * TAU) / 8
        g.moveTo(cx + Math.cos(an) * 4, cy + Math.sin(an) * 4)
        g.lineTo(cx + Math.cos(an) * 5.6, cy + Math.sin(an) * 5.6)
      }
      g.stroke()
    } else {
      g.beginPath()
      g.arc(cx, cy, 4.4, Math.PI * 0.35, Math.PI * 1.65)
      g.arc(cx + 2.6, cy, 3.6, Math.PI * 1.42, Math.PI * 0.58, true)
      g.closePath()
      g.fill()
    }
  }

  private drawTramp(g: CanvasRenderingContext2D, level: Stage, i: number, tx: number, ty: number, t: number): void {
    const p = this.pal
    const x = tx * TILE
    const y = ty * TILE
    // a bounce presses the pad in and lets it ring
    const at = this.trampAt.get(i)
    let d = 0
    if (at !== undefined) {
      const e = t - at
      if (e > 0.6) this.trampAt.delete(i)
      else d = 4.5 * Math.exp(-e * 7) * Math.cos(e * 34)
    }
    const l = tx > 0 && level.tiles[i - 1] === T_TRAMP
    const r = tx < level.cols - 1 && level.tiles[i + 1] === T_TRAMP
    const base = p.dark ? mixRgb(METAL, BLACK, 0.45) : mixRgb(METAL, BLACK, 0.2)
    g.fillStyle = css(base)
    roundRectPath(g, x, y + 5, TILE + (r ? 0.4 : 0), TILE - 5, 0, 0, r ? 0 : 3, l ? 0 : 3)
    g.fill()
    // the spring showing through a slot
    g.fillStyle = css(BLACK, 0.4)
    g.fillRect(x + 4, y + 7, TILE - 8, 9)
    g.strokeStyle = css(p.dark ? mixRgb(METAL, WHITE, 0.4) : mixRgb(METAL, WHITE, 0.6))
    g.lineWidth = 1.2
    g.beginPath()
    const top = y + 6 + Math.max(-1, d)
    for (let k = 0; k <= 4; k++) {
      const yy = top + ((y + 16 - top) * k) / 4
      const xx = x + (k % 2 ? 14.5 : 5.5)
      if (k === 0) g.moveTo(xx, yy)
      else g.lineTo(xx, yy)
    }
    g.stroke()
    const pad = (py: number, flip: boolean) => {
      const h = 5
      g.fillStyle = css(TRAMP)
      roundRectPath(g, x - (l ? 0 : 0.5), py, TILE + (l ? 0 : 0.5) + (r ? 0.4 : 0.5), h, l || flip ? 0 : 2.5, r || flip ? 0 : 2.5, r || !flip ? 0 : 2.5, l || !flip ? 0 : 2.5)
      g.fill()
      g.fillStyle = css(WHITE, 0.85)
      g.fillRect(x + 2, flip ? py + h - 1.6 : py, 5, 1.6)
      g.fillRect(x + 13, flip ? py + h - 1.6 : py, 5, 1.6)
      g.fillStyle = css(mixRgb(TRAMP, BLACK, 0.4), 0.8)
      g.fillRect(x, flip ? py : py + h - 1.2, TILE, 1.2)
    }
    pad(y + Math.max(-1.5, d), false)
    // open underneath: it bounces a flipped player off its underside too
    if (ty < level.rows - 1 && level.tiles[i + level.cols] === 0) pad(y + TILE - 5 - Math.max(-1.5, d) * 0.6, true)
  }

  private drawCrumble(g: CanvasRenderingContext2D, cur: GameState, i: number, x: number, y: number, t: number, opts: DrawOpts): void {
    const p = this.pal
    const j = this.crumbleIndex.get(i)
    const c = j === undefined ? 0 : (cur.crumble[j] ?? 0)
    const sand = p.dark ? mixRgb(SAND, p.wall, 0.45) : mixRgb(SAND, p.wall, 0.3)
    if (c > CRUMBLE_TICKS) {
      // gone: a dotted outline that fills back in as it gets ready to return
      const back = clamp((c - CRUMBLE_TICKS) / CRUMBLE_GONE_TICKS, 0, 1)
      g.strokeStyle = css(sand, 0.6)
      g.lineWidth = 1
      g.setLineDash([1.5, 3])
      rr(g, x + 1.5, y + 1.5, TILE - 3, TILE - 3, 3)
      g.stroke()
      g.setLineDash([])
      const s = (TILE - 6) * back
      g.fillStyle = css(sand, 0.1 + 0.16 * back)
      rr(g, x + TILE / 2 - s / 2, y + TILE / 2 - s / 2, s, s, 2)
      g.fill()
      return
    }
    const shake = c > 0 ? c / CRUMBLE_TICKS : 0
    const jx = shake && !opts.reduceMotion ? Math.sin(t * 70 + i) * (0.4 + shake * 1.1) : 0
    const jy = shake && !opts.reduceMotion ? Math.cos(t * 57 + i * 3) * shake * 0.7 : 0
    const at = this.mendAt.get(i)
    let k = 1
    if (at !== undefined) {
      const e = (t - at) / 0.22
      if (e >= 1) this.mendAt.delete(i)
      else k = 0.55 + 0.45 * e * (2 - e)
    }
    g.save()
    g.translate(x + TILE / 2 + jx, y + TILE / 2 + jy)
    g.scale(k, k)
    g.translate(-TILE / 2, -TILE / 2)
    g.globalAlpha = k < 1 ? k : 1
    g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.1)
    rr(g, 1, 2, TILE, TILE, 3)
    g.fill()
    g.fillStyle = css(sand)
    rr(g, 0, 0, TILE, TILE, 3)
    g.fill()
    g.fillStyle = css(mixRgb(sand, WHITE, p.dark ? 0.3 : 0.5))
    g.fillRect(2, 0, TILE - 4, 2.2)
    g.fillStyle = css(mixRgb(sand, BLACK, 0.35), 0.7)
    g.fillRect(2, TILE - 2.2, TILE - 4, 2.2)
    // always a little cracked, so it reads as the block not to trust; the cracks run as it goes
    const h = hash01(i)
    g.strokeStyle = css(mixRgb(sand, BLACK, 0.55), 0.85)
    g.lineWidth = 0.9 + shake * 0.7
    g.lineJoin = 'round'
    g.beginPath()
    g.moveTo(4 + h * 5, 2)
    g.lineTo(8 + h * 3, 7)
    g.lineTo(6 + h * 4, 11)
    g.moveTo(15 - h * 3, TILE - 2)
    g.lineTo(13 - h * 2, 13)
    if (shake > 0.01) {
      g.lineTo(10 + h * 2, 10)
      g.lineTo(6 + h * 4, 11)
      g.moveTo(8 + h * 3, 7)
      g.lineTo(14, 5 + h * 2)
      if (shake > 0.5) {
        g.lineTo(TILE - 2, 7)
        g.moveTo(6 + h * 4, 11)
        g.lineTo(3, 15)
        g.lineTo(5, TILE - 2)
      }
    }
    g.stroke()
    g.restore()
    g.globalAlpha = 1
    if (shake > 0 && Math.random() < (opts.reduceMotion ? 0.06 : 0.3)) this.parts.spawn(PK_DOT, C_SAND, x + rand(2, TILE - 2), y + TILE - 1, rand(-8, 8), rand(10, 40), 0.4, rand(0.9, 1.5), 300, 0.5)
  }

  // ------------------------------------------------------------ version 2's things

  private drawCheckpoints(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, dt: number): void {
    const p = this.pal
    for (let i = 0; i < level.checkpoints.length; i++) {
      const c = level.checkpoints[i]
      const taken = i === cur.cp
      this.cpUp[i] = approach(this.cpUp[i] ?? 0, taken ? 1 : 0, 9, dt)
      if (!this.vis(c.x - 14, c.y - 30, 30, 32)) continue
      const up = this.cpUp[i]
      const top = c.y - 27
      // the flourish: a moment of overshoot when it's taken (or come back to)
      const e = t - (this.cpAt[i] ?? -9)
      const pop = e >= 0 && e < 0.7 ? Math.exp(-e * 6) * Math.sin(e * 20) : 0
      if (up > 0.02) {
        this.beginGlow(g)
        this.glow(g, c.x + 5, top + 6, 20 + pop * 6, css(p.star), (0.5 + 0.08 * Math.sin(t * 3)) * up)
        this.endGlow(g)
      }
      // foot and pole
      g.fillStyle = css(p.dark ? mixRgb(METAL, BLACK, 0.3) : METAL)
      g.beginPath()
      g.ellipse(c.x, c.y, 5, 2.2, 0, Math.PI, 0)
      g.fill()
      g.fillRect(c.x - 0.9, top, 1.8, 27)
      g.fillStyle = css(up > 0.5 ? p.star : p.muted)
      g.beginPath()
      g.arc(c.x, top - 0.5, 1.9, 0, TAU)
      g.fill()
      // the pennant: grey and hanging until both have stood here, then up in both colours, waving
      const len = 5 + 9 * up + pop * 4
      const hy = top + 2 + (1 - up) * 3
      const hgt = 9
      const wave = (u: number) => Math.sin(t * 5.5 - u * 3.2 + i) * 1.5 * u * up
      const droop = (u: number) => (1 - up) * u * 7
      for (let half = 0; half < 2; half++) {
        const y0 = hy + (half * hgt) / 2
        const y1 = y0 + hgt / 2
        g.fillStyle = up > 0.5 ? css(half ? p.frost : p.fire) : css(p.muted, 0.75)
        g.beginPath()
        g.moveTo(c.x + 0.9, y0)
        for (let u = 0.25; u <= 1.001; u += 0.25) g.lineTo(c.x + 0.9 + len * u, y0 + (hgt / 2 - half * (hgt / 2)) * u * (half ? 0 : 1) + wave(u) + droop(u) + (half ? 0 : 0))
        for (let u = 1; u >= -0.001; u -= 0.25) g.lineTo(c.x + 0.9 + len * u, y1 - (half ? (hgt / 2) * u : 0) + wave(u) + droop(u))
        g.closePath()
        g.fill()
      }
      if (!taken && cur.phase === 'play') {
        // two pips at the foot: each lights while its player stands in the zone. it takes both
        for (let k = 0; k < 2; k++) {
          const pl = cur.players[k]
          const z = c.zone
          const inside = pl.alive && pl.x < z.x + z.w && pl.x + PLAYER_W > z.x && pl.y < z.y + z.h && pl.y + PLAYER_H > z.y
          g.fillStyle = inside ? css(k === 0 ? p.fire : p.frost) : css(p.ink, 0.22)
          g.beginPath()
          g.arc(c.x + (k ? 4.2 : -4.2), c.y - 4.5, inside ? 1.9 : 1.4, 0, TAU)
          g.fill()
        }
      }
    }
  }

  private drawLamps(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, dt: number): void {
    const p = this.pal
    for (let i = 0; i < level.lamps.length; i++) {
      const l = level.lamps[i]
      this.lampOn[i] = approach(this.lampOn[i] ?? 0, cur.lit ? 1 : 0, 18, dt)
      if (!this.vis(l.x, l.y, l.w, l.h, 30)) continue
      const on = this.lampOn[i]
      const cx = l.x + l.w / 2
      const by = l.y + l.h
      const hy = l.y + 6.5
      // a short post, a cage, a bulb, and a pull cord to say "press down here"
      const metal = p.dark ? mixRgb(METAL, BLACK, 0.3) : METAL
      this.beginGlow(g)
      this.glow(g, cx, hy, 13 + on * 13, css(SUN), 0.22 + on * 0.5 + 0.04 * Math.sin(t * 5 + i))
      this.endGlow(g)
      g.fillStyle = css(metal)
      g.fillRect(cx - 1, hy + 4, 2, by - hy - 4)
      g.beginPath()
      g.ellipse(cx, by, 5, 2.2, 0, Math.PI, 0)
      g.fill()
      rr(g, cx - 4, hy + 3.4, 8, 2.2, 1)
      g.fill()
      g.fillStyle = css(mixRgb(mixRgb(SUN, BLACK, 0.72), mixRgb(SUN, WHITE, 0.55), on))
      g.beginPath()
      g.arc(cx, hy, 4.6, 0, TAU)
      g.fill()
      // the filament always smoulders a little, so a lamp can be found in the dark
      g.fillStyle = css(mixRgb(LAVA_MID, WHITE, on), 0.9)
      g.beginPath()
      g.arc(cx, hy + 0.4, 1.4 + on * 0.8, 0, TAU)
      g.fill()
      g.strokeStyle = css(metal)
      g.lineWidth = 0.9
      g.beginPath()
      g.arc(cx, hy, 4.9, 0, TAU)
      g.moveTo(cx, hy - 4.9)
      g.lineTo(cx, hy + 4.9)
      g.stroke()
      g.strokeStyle = css(p.ink, 0.4)
      g.lineWidth = 0.7
      g.beginPath()
      g.moveTo(cx + 5.2, hy + 3)
      g.lineTo(cx + 5.2, hy + 8 + on)
      g.stroke()
      g.fillStyle = css(SUN)
      g.beginPath()
      g.arc(cx + 5.2, hy + 8.6 + on, 1.1, 0, TAU)
      g.fill()
      if (l.ticks > 0) {
        // a timed lamp wears a ring: full when pressed, emptying until the lights fall back
        const frac = cur.litT > 0 ? clamp(cur.litT / l.ticks, 0, 1) : 0
        g.lineCap = 'round'
        g.strokeStyle = css(p.ink, 0.18)
        g.lineWidth = 1.5
        g.setLineDash([1.2, 2.4])
        g.beginPath()
        g.arc(cx, hy, 7.6, 0, TAU)
        g.stroke()
        g.setLineDash([])
        if (frac > 0) {
          g.strokeStyle = css(frac < 0.25 && Math.sin(t * 20) > 0 ? WHITE : SUN)
          g.lineWidth = 1.9
          g.beginPath()
          g.arc(cx, hy, 7.6, -Math.PI / 2, -Math.PI / 2 + TAU * frac)
          g.stroke()
        }
      }
    }
  }

  private drawGswitches(g: CanvasRenderingContext2D, level: Stage, t: number, dt: number): void {
    const p = this.pal
    for (let i = 0; i < level.gswitches.length; i++) {
      const s = level.gswitches[i]
      this.gswSpin[i] = Math.max(0, (this.gswSpin[i] ?? 0) - dt * 2.4)
      if (!this.vis(s.x, s.y, s.w, s.h, 20)) continue
      const who: Who = s.who
      const spin = this.gswSpin[i]
      const cx = s.x + s.w / 2
      const cy = s.y + 8.5 + Math.sin(t * 2.2 + i) * 0.7
      const col = p.dark ? GRAV : mixRgb(GRAV, BLACK, 0.15)
      this.beginGlow(g)
      this.glow(g, cx, cy, 15 + spin * 8, css(GRAV), 0.4 + spin * 0.4)
      this.endGlow(g)
      g.fillStyle = css(p.dark ? mixRgb(GRAV, BLACK, 0.74) : mixRgb(GRAV, WHITE, 0.72))
      g.beginPath()
      g.arc(cx, cy, 7.2, 0, TAU)
      g.fill()
      // the rim says whose gravity: ember's, frost's, half and half for both, plain for whoever presses,
      // dashed for the one who doesn't
      g.lineWidth = 1.5
      if (who === 'both') {
        g.strokeStyle = css(p.fire)
        g.beginPath()
        g.arc(cx, cy, 7.2, Math.PI * 0.5, Math.PI * 1.5)
        g.stroke()
        g.strokeStyle = css(p.frost)
        g.beginPath()
        g.arc(cx, cy, 7.2, Math.PI * 1.5, Math.PI * 2.5)
        g.stroke()
      } else {
        g.strokeStyle = css(who === 'fire' ? p.fire : who === 'frost' ? p.frost : col)
        if (who === 'other') {
          g.setLineDash([2.6, 2.2])
          g.lineDashOffset = -t * 6
        }
        g.beginPath()
        g.arc(cx, cy, 7.2, 0, TAU)
        g.stroke()
        g.setLineDash([])
      }
      // two arrows, one up and one down, that swap over when it's pressed
      g.save()
      g.translate(cx, cy)
      g.rotate(spin * spin * Math.PI)
      g.strokeStyle = css(col)
      g.lineWidth = 1.4
      g.lineCap = 'round'
      g.lineJoin = 'round'
      g.beginPath()
      g.moveTo(-2.4, 3.6)
      g.lineTo(-2.4, -3.4)
      g.moveTo(-4.4, -1.4)
      g.lineTo(-2.4, -3.6)
      g.lineTo(-0.4, -1.4)
      g.moveTo(2.4, -3.6)
      g.lineTo(2.4, 3.4)
      g.moveTo(0.4, 1.4)
      g.lineTo(2.4, 3.6)
      g.lineTo(4.4, 1.4)
      g.stroke()
      g.restore()
      // and spelled out underneath: a flame, a flake, both, or who relative to the presser
      const ly = s.y + s.h - 2.6
      if (who === 'fire' || who === 'both') {
        g.fillStyle = css(p.fire)
        flameGlyph(g, cx - (who === 'both' ? 3.4 : 0), ly - 0.4, 5.6)
        g.fill()
      }
      if (who === 'frost' || who === 'both') {
        g.strokeStyle = css(p.frost)
        g.lineWidth = 0.8
        flakeGlyph(g, cx + (who === 'both' ? 3.4 : 0), ly - 0.4, 2.5)
        g.stroke()
      }
      if (who === 'self' || who === 'other') {
        // a dot is the presser; 'other' points away from it to a ring
        g.fillStyle = css(p.ink, 0.75)
        g.strokeStyle = css(p.ink, 0.75)
        g.lineWidth = 0.8
        g.beginPath()
        g.arc(cx - (who === 'other' ? 4.4 : 0), ly, 1.5, 0, TAU)
        g.fill()
        if (who === 'other') {
          g.beginPath()
          g.moveTo(cx - 1.6, ly)
          g.lineTo(cx + 1.6, ly)
          g.moveTo(cx + 0.4, ly - 1.2)
          g.lineTo(cx + 1.6, ly)
          g.lineTo(cx + 0.4, ly + 1.2)
          g.stroke()
          g.beginPath()
          g.arc(cx + 4.4, ly, 1.5, 0, TAU)
          g.stroke()
        }
      }
    }
  }

  private drawLocks(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, dt: number): void {
    const p = this.pal
    for (let i = 0; i < level.locks.length; i++) {
      const r = level.locks[i]
      this.lockGone[i] = approach(this.lockGone[i] ?? 0, cur.locks[i] ? 1 : 0, 11, dt)
      const gone = this.lockGone[i]
      if (gone > 0.98 || !this.vis(r.x, r.y, r.w, r.h, 12)) continue
      const cx = r.x + r.w / 2
      const cy = r.y + r.h / 2
      const body = p.dark ? mixRgb(METAL, BLACK, 0.5) : mixRgb(METAL, BLACK, 0.28)
      g.save()
      g.translate(cx, cy)
      g.scale(1 + gone * 0.18, 1 + gone * 0.18)
      g.translate(-cx, -cy)
      g.globalAlpha = 1 - gone
      g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.1)
      rr(g, r.x + 1, r.y + 2, r.w, r.h, 3)
      g.fill()
      g.fillStyle = css(body)
      rr(g, r.x, r.y, r.w, r.h, 3)
      g.fill()
      // gold bands and a gold frame: this one wants a key
      g.fillStyle = css(GOLD, 0.9)
      for (let yy = r.y + 3; yy < r.y + r.h - 3; yy += TILE) g.fillRect(r.x + 1, yy, r.w - 2, 1.6)
      g.strokeStyle = css(GOLD)
      g.lineWidth = 1.4
      rr(g, r.x + 0.7, r.y + 0.7, r.w - 1.4, r.h - 1.4, 3)
      g.stroke()
      // the keyhole wakes up when the team has a key to spend
      const ready = cur.keys > 0 && !cur.locks[i]
      if (ready) {
        this.beginGlow(g)
        this.glow(g, cx, cy, 14, css(GOLD), 0.5 + 0.25 * Math.sin(t * 6))
        this.endGlow(g)
        g.globalAlpha = 1 - gone
      }
      g.fillStyle = css(GOLD)
      g.beginPath()
      g.arc(cx, cy, 5.6, 0, TAU)
      g.fill()
      g.fillStyle = css(ready ? mixRgb(GOLD_DARK, BLACK, 0.5) : mixRgb(body, BLACK, 0.5))
      g.beginPath()
      g.arc(cx, cy - 1.2, 1.9, 0, TAU)
      g.fill()
      g.beginPath()
      g.moveTo(cx - 1, cy - 0.6)
      g.lineTo(cx + 1, cy - 0.6)
      g.lineTo(cx + 1.7, cy + 3.6)
      g.lineTo(cx - 1.7, cy + 3.6)
      g.closePath()
      g.fill()
      g.restore()
      g.globalAlpha = 1
    }
  }

  private keyAt(g: CanvasRenderingContext2D, x: number, y: number, s: number, ang: number): void {
    g.save()
    g.translate(x, y)
    g.rotate(ang)
    g.lineCap = 'round'
    g.lineJoin = 'round'
    keyPath(g, s)
    g.strokeStyle = css(GOLD_DARK)
    g.lineWidth = 3.2 * s
    g.stroke()
    g.strokeStyle = css(GOLD)
    g.lineWidth = 1.7 * s
    g.stroke()
    g.fillStyle = css(WHITE, 0.8)
    g.fillRect(-4.6 * s, -2 * s, 1.1 * s, 1.1 * s)
    g.restore()
  }

  private drawKeys(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, opts: DrawOpts): void {
    for (let i = 0; i < level.keys.length; i++) {
      if (cur.keyTaken[i]) continue
      const k = level.keys[i]
      if (!this.vis(k.x - 10, k.y - 10, 20, 20)) continue
      const y = k.y + Math.sin(t * 2.4 + i * 2.1) * 1.8
      this.beginGlow(g)
      this.glow(g, k.x, y, 17, css(GOLD), 0.5 + 0.12 * Math.sin(t * 4 + i))
      this.endGlow(g)
      this.keyAt(g, k.x - 1, y, 1.1, -0.5 + Math.sin(t * 1.7 + i) * 0.12)
      if (Math.random() < (opts.reduceMotion ? 0.01 : 0.035)) this.parts.spawn(PK_STAR, C_GOLD, k.x + rand(-7, 7), y + rand(-6, 6), 0, -4, 0.5, rand(2, 3), 0, 0, true)
    }
  }

  // the keys the team carries trail after whoever picked the last one up, each a little behind the one before
  private drawCarried(g: CanvasRenderingContext2D, cur: GameState, t: number, dt: number): void {
    const n = Math.min(6, Math.max(0, cur.keys | 0))
    if (this.keyTrail.length > n) this.keyTrail.length = n
    if (!n) return
    const h: PlayerIndex = cur.players[this.keyHolder].alive ? this.keyHolder : this.keyHolder === 0 ? 1 : 0
    const pl = cur.players[h]
    const gv = pl.grav === -1 ? -1 : 1
    let tx = this.px[h] + PLAYER_W / 2 - pl.face * 13
    let ty = this.py[h] + PLAYER_H / 2 - gv * 7
    for (let i = 0; i < n; i++) {
      let k = this.keyTrail[i]
      if (!k) k = this.keyTrail[i] = { x: tx, y: ty }
      k.x = approach(k.x, tx, 11 - i * 1.2, dt)
      k.y = approach(k.y, ty, 11 - i * 1.2, dt)
      // too far behind (a portal, a cannon): catch up at once
      if (Math.abs(k.x - tx) + Math.abs(k.y - ty) > 160) {
        k.x = tx
        k.y = ty
      }
      const y = k.y + Math.sin(t * 4 + i * 1.3) * 1.4
      if (this.vis(k.x - 8, y - 8, 16, 16)) {
        this.beginGlow(g)
        this.glow(g, k.x, y, 11, css(GOLD), 0.45)
        this.endGlow(g)
        this.keyAt(g, k.x, y, 0.8, -0.5 + Math.sin(t * 3 + i) * 0.2)
      }
      tx = k.x - pl.face * 9
      ty = k.y
    }
  }

  private drawArrows(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, opts: DrawOpts): void {
    const p = this.pal
    const col = p.dark ? ARROW : mixRgb(ARROW, BLACK, 0.2)
    for (let i = 0; i < level.arrows.length; i++) {
      const a = level.arrows[i]
      if (!this.vis(a.x - 10, a.y - 12, 20, 24)) continue
      const left = cur.arrows[i] ?? 0
      g.lineCap = 'round'
      g.lineJoin = 'round'
      if (left > 0) {
        // taken: a ghost of it, and a ring that closes as it comes back
        const back = 1 - clamp(left / ARROW_RESPAWN_TICKS, 0, 1)
        g.save()
        g.translate(a.x, a.y)
        g.globalAlpha = 0.3 + 0.25 * back
        g.strokeStyle = css(col)
        g.lineWidth = 1.1
        g.setLineDash([2, 2])
        chevrons(g, 1.25)
        g.stroke()
        g.setLineDash([])
        g.globalAlpha = 0.75
        g.lineWidth = 1.4
        g.beginPath()
        g.arc(0, 0, 8.6, -Math.PI / 2, -Math.PI / 2 + TAU * back)
        g.stroke()
        g.restore()
        g.globalAlpha = 1
        continue
      }
      const bob = opts.reduceMotion ? 0 : Math.sin(t * 3 + i * 1.9) * 2.2
      const y = a.y + bob
      this.beginGlow(g)
      this.glow(g, a.x, y, 18, css(ARROW), 0.5 + 0.12 * Math.sin(t * 5 + i))
      this.endGlow(g)
      g.save()
      g.translate(a.x, y)
      g.fillStyle = css(p.dark ? mixRgb(ARROW, BLACK, 0.72) : mixRgb(ARROW, WHITE, 0.78), 0.85)
      g.beginPath()
      g.arc(0, 0, 8, 0, TAU)
      g.fill()
      g.strokeStyle = css(col, 0.9)
      g.lineWidth = 1
      g.stroke()
      chevrons(g, 1.25)
      g.strokeStyle = css(mixRgb(ARROW, BLACK, 0.5))
      g.lineWidth = 3.2
      g.stroke()
      g.strokeStyle = css(p.dark ? mixRgb(ARROW, WHITE, 0.45) : WHITE)
      g.lineWidth = 1.7
      g.stroke()
      g.restore()
      if (Math.random() < (opts.reduceMotion ? 0.01 : 0.05)) this.parts.spawn(PK_CHEV, C_ARROW, a.x + rand(-6, 6), y + 8, 0, -rand(20, 40), 0.5, 1.8, 0, 0, true)
    }
  }

  private drawCannons(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, dt: number, opts: DrawOpts): void {
    const p = this.pal
    for (let i = 0; i < level.cannons.length; i++) {
      const c = level.cannons[i]
      this.cannonKick[i] = Math.max(0, (this.cannonKick[i] ?? 0) - dt * 4)
      if (!this.vis(c.x - 12, c.y - 12, 24, 24, 220)) continue
      const kick = this.cannonKick[i]
      const ticks = cur.cannons[i] ?? 0
      const who = cur.players[0].cannon === i ? 0 : cur.players[1].cannon === i ? 1 : -1
      const loaded = ticks > 0 || who >= 0
      const ang = Math.atan2(c.vy, c.vx)
      const chs = condChannels(c.fire)
      const ready = c.fire !== undefined && condHolds(c.fire, cur.on, cur.lit)
      const chCol = chs[0] ? channelColorOf(chs[0]) : css(p.muted)
      // where the shot will go: dots along the arc as far as the first wall
      const gv = (who === 0 || who === 1) && cur.players[who].grav === -1 ? -1 : 1
      g.fillStyle = css(who === 0 ? p.fire : who === 1 ? p.frost : p.ink, loaded ? 0.7 : p.dark ? 0.26 : 0.22)
      const flyS = CANNON_FLY_TICKS / 60
      for (let k = 1; k <= 26; k++) {
        const tt = k * 0.055 + (opts.reduceMotion || !loaded ? 0 : ((t * 0.5) % 0.055))
        if (tt > flyS) break
        const x = c.x + c.vx * tt
        const y = c.y + c.vy * tt + 0.5 * GRAVITY * gv * tt * tt
        if (tileAt(level, cur, Math.floor(x / TILE), Math.floor(y / TILE)) === T_WALL) break
        if (k < 3) continue
        g.beginPath()
        g.arc(x, y, loaded ? 1.5 : 1.1, 0, TAU)
        g.fill()
      }
      const metal = p.dark ? mixRgb(METAL, BLACK, 0.42) : mixRgb(METAL, BLACK, 0.22)
      const swell = loaded && !opts.reduceMotion ? 1 + 0.05 * Math.sin(t * 22) : 1
      g.save()
      g.translate(c.x, c.y)
      g.rotate(ang)
      g.translate(-kick * kick * 5, 0)
      g.scale(1, swell)
      // the barrel, with a band in its channel's colour and a bright muzzle ring
      g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.12)
      rr(g, -8, -5.5, 21, 14, 5)
      g.fill()
      g.fillStyle = css(metal)
      rr(g, -9, -7, 21, 14, 5)
      g.fill()
      g.fillStyle = css(mixRgb(metal, WHITE, p.dark ? 0.22 : 0.4))
      g.fillRect(-5, -7, 14, 2)
      g.fillStyle = chCol
      g.fillRect(-2.5, -7, 3, 14)
      g.fillStyle = css(mixRgb(metal, WHITE, 0.35))
      rr(g, 9.5, -8, 3.6, 16, 1.6)
      g.fill()
      g.fillStyle = css(BLACK, 0.7)
      g.beginPath()
      g.ellipse(13, 0, 1.6, 5.4, 0, 0, TAU)
      g.fill()
      if (who >= 0) {
        // whoever's inside peeks out of the muzzle
        g.fillStyle = css(who === 0 ? p.fire : p.frost2)
        g.beginPath()
        g.arc(12.2, 0, 4.6, -Math.PI / 2, Math.PI / 2)
        g.fill()
        g.fillStyle = who === 0 ? 'rgb(64, 22, 6)' : 'rgb(14, 48, 84)'
        g.fillRect(14.4, -2.4, 1.3, 1.6)
        g.fillRect(14.4, 0.9, 1.3, 1.6)
      }
      g.restore()
      // the hub it turns on
      g.fillStyle = css(mixRgb(metal, BLACK, 0.3))
      g.beginPath()
      g.arc(c.x, c.y, 4.6, 0, TAU)
      g.fill()
      g.fillStyle = loaded && (c.fire === undefined || ready || Math.sin(t * 10) > 0) ? chCol : css(mixRgb(metal, WHITE, 0.3))
      g.beginPath()
      g.arc(c.x, c.y, 1.9, 0, TAU)
      g.fill()
      if (loaded) {
        this.beginGlow(g)
        this.glow(g, c.x, c.y, 18, css(who === 0 ? p.fire : p.frost), 0.45)
        this.endGlow(g)
        g.lineCap = 'round'
        if (c.fire === undefined) {
          // its own fuse: a ring burning round to the bang
          const frac = clamp(ticks / CANNON_FUSE_TICKS, 0, 1)
          g.strokeStyle = css(p.ink, 0.2)
          g.lineWidth = 1.6
          g.beginPath()
          g.arc(c.x, c.y, 12.5, 0, TAU)
          g.stroke()
          g.strokeStyle = css(LAVA_TOP)
          g.lineWidth = 2
          g.beginPath()
          g.arc(c.x, c.y, 12.5, -Math.PI / 2, -Math.PI / 2 + TAU * frac)
          g.stroke()
          const fa = -Math.PI / 2 + TAU * frac
          if (Math.random() < (opts.reduceMotion ? 0.1 : 0.6)) this.parts.spawn(PK_SPARK, Math.random() < 0.5 ? C_CORE : C_FIRE2, c.x + Math.cos(fa) * 12.5, c.y + Math.sin(fa) * 12.5, rand(-30, 30), rand(-40, 10), rand(0.2, 0.4), rand(0.7, 1.2), 120, 1, true)
        } else {
          // waiting on a partner: a ring in the channel's colour, pulsing
          g.strokeStyle = chCol
          g.globalAlpha = 0.45 + 0.4 * Math.abs(Math.sin(t * 5))
          g.lineWidth = 1.6
          g.setLineDash([3, 3])
          g.lineDashOffset = -t * 10
          g.beginPath()
          g.arc(c.x, c.y, 12.5, 0, TAU)
          g.stroke()
          g.setLineDash([])
          g.globalAlpha = 1
        }
      }
    }
  }

  // ------------------------------------------------------------ fans

  private drawFanColumns(g: CanvasRenderingContext2D, level: Stage, cur: GameState, dt: number, opts: DrawOpts): void {
    const p = this.pal
    for (let i = 0; i < level.fans.length; i++) {
      const f = level.fans[i]
      const on = condHolds(f.when, cur.on, cur.lit)
      this.fanSpin[i] = approach(this.fanSpin[i] ?? 0, on ? 1 : 0, on ? 3 : 1.5, dt)
      this.fanAng[i] = (this.fanAng[i] ?? 0) + this.fanSpin[i] * dt * 22
      const spin = this.fanSpin[i]
      if (!this.vis(f.x, f.y, f.w, f.h, 4)) continue
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

  private drawGems(g: CanvasRenderingContext2D, level: Stage, cur: GameState, t: number, opts: DrawOpts): void {
    const p = this.pal
    for (let i = 0; i < level.gems.length; i++) {
      if (cur.taken[i]) continue
      const gm = level.gems[i]
      if (!this.vis(gm.x - 8, gm.y - 8, 16, 16)) continue
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

  private drawBeams(g: CanvasRenderingContext2D, t: number, opts: DrawOpts): void {
    const beams = this.beams
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
    // gv: which way this player falls. everything about them hangs off it: the body, the shadow, the trail
    const gv = pl.grav === -1 ? -1 : 1
    this.vyRel = pl.vy * gv
    this.animate(an, pl, t, dt)
    an.flip = opts.reduceMotion ? (gv < 0 ? 1 : 0) : approach(an.flip, gv < 0 ? 1 : 0, 16, dt)
    an.aura = approach(an.aura, gv < 0 ? 1 : 0, 8, dt)
    const fire = i === 0
    const cx = x + PLAYER_W / 2
    const feet = gv > 0 ? y + PLAYER_H : y // the end they stand on
    an.x = cx
    an.y = feet
    // dead, or riding in a cannon (the cannon draws them peeking out)
    if (!pl.alive || pl.cannon >= 0) return
    if (!this.vis(x, y, PLAYER_W, PLAYER_H, 40)) return
    const p = this.pal
    const cy = y + PLAYER_H / 2
    const moving = Math.abs(pl.vx) > 30
    const bob = pl.ground && moving ? Math.abs(Math.sin(an.run)) * 1.6 : 0
    const breathe = 1 + Math.sin(t * 3.1 + i) * 0.025
    const sy = (1 + an.sq) * breathe
    const sx = 1 - an.sq * 0.55
    // glow
    this.beginGlow(g)
    this.glow(g, cx, cy, fire ? 30 : 26, css(fire ? p.fire : p.frost), fire ? 0.55 + 0.08 * Math.sin(t * 13) : 0.42)
    if (an.aura > 0.02) this.glow(g, cx, cy, 30, css(GRAV), 0.5 * an.aura)
    this.endGlow(g)
    // contact shadow, on whatever they stand on
    if (pl.ground) {
      g.fillStyle = css(BLACK, p.dark ? 0.3 : 0.12)
      g.beginPath()
      g.ellipse(cx, feet, 7 * sx, 1.6, 0, 0, TAU)
      g.fill()
    }
    if (an.aura > 0.02) {
      // flipped: a turning violet ring with arrowheads pointing up, so nobody has to guess whose world is upside down
      g.save()
      g.globalAlpha = an.aura
      g.strokeStyle = css(p.dark ? GRAV : mixRgb(GRAV, BLACK, 0.15), 0.9)
      g.lineWidth = 1.3
      g.setLineDash([4, 4])
      g.lineDashOffset = opts.reduceMotion ? 0 : -t * 14
      g.beginPath()
      g.ellipse(cx, cy, 13, 16, 0, 0, TAU)
      g.stroke()
      g.setLineDash([])
      g.lineCap = 'round'
      g.lineJoin = 'round'
      g.lineWidth = 1.6
      for (let s = -1; s <= 1; s += 2) {
        const ay = cy + (opts.reduceMotion ? 0 : -((t * 14 + (s > 0 ? 8 : 0)) % 16) + 8)
        g.globalAlpha = an.aura * (1 - Math.abs(ay - cy) / 10)
        g.beginPath()
        g.moveTo(cx + s * 13 - 2.6, ay + 1.6)
        g.lineTo(cx + s * 13, ay - 1.6)
        g.lineTo(cx + s * 13 + 2.6, ay + 1.6)
        g.stroke()
      }
      g.restore()
      g.globalAlpha = 1
    }
    // shot from a cannon: a comet tail back along the way they came
    if (pl.fly > 0) {
      const sp = Math.hypot(pl.vx, pl.vy) || 1
      g.fillStyle = css(fire ? p.fire2 : p.frost2)
      for (let k = 1; k <= 4; k++) {
        g.globalAlpha = 0.34 - k * 0.07
        g.beginPath()
        g.arc(cx - (pl.vx / sp) * k * 7, cy - (pl.vy / sp) * k * 7, 7.5 - k * 1.2, 0, TAU)
        g.fill()
      }
      g.globalAlpha = 1
    }
    g.save()
    // turn about the middle of the body: half a turn puts the feet on the ceiling. past halfway the picture
    // is mirrored back, so a flipped player still looks the way they walk
    g.translate(cx, cy)
    if (an.flip > 0.001) {
      g.rotate(Math.PI * an.flip)
      if (an.flip > 0.5) g.scale(-1, 1)
    }
    g.translate(0, PLAYER_H / 2 - bob)
    g.rotate(an.tilt)
    g.scale(sx, sy)
    const me = opts.me === i
    const happy = cur.phase === 'won' || cur.phase === 'clear' || pl.inDoor
    if (fire) this.drawEmber(g, an, pl, t, me, happy)
    else this.drawFrost(g, an, pl, t, me, happy)
    g.restore()

    // a spare jump in hand: two bright chevrons riding under the feet and a ring, until it's spent or lost
    if (pl.air > 0) {
      const col = p.dark ? ARROW : mixRgb(ARROW, BLACK, 0.2)
      const pulse = opts.reduceMotion ? 0.5 : 0.5 + 0.5 * Math.sin(t * 9)
      this.beginGlow(g)
      this.glow(g, cx, feet + gv * 4, 13, css(ARROW), 0.5 + 0.3 * pulse)
      this.endGlow(g)
      g.save()
      g.translate(cx, feet + gv * (5 + pulse * 1.5))
      g.scale(1, gv)
      g.lineCap = 'round'
      g.lineJoin = 'round'
      chevrons(g, 1.15)
      g.strokeStyle = css(mixRgb(ARROW, BLACK, 0.55))
      g.lineWidth = 3
      g.stroke()
      g.strokeStyle = css(p.dark ? mixRgb(ARROW, WHITE, 0.4) : WHITE)
      g.lineWidth = 1.5
      g.stroke()
      g.restore()
      g.strokeStyle = css(col, 0.55 + 0.3 * pulse)
      g.lineWidth = 1.1
      g.beginPath()
      g.ellipse(cx, cy, 11.5, 14.5, 0, 0, TAU)
      g.stroke()
    }

    // trails: sparks off the flame, snowflakes off the crystal, thicker in flight. flipped, they fall up
    const up = (d: number) => feet - gv * d
    const rate = (pl.fly > 0 ? 70 : moving || !pl.ground ? 26 : 0) * (opts.reduceMotion ? 0.3 : 1)
    an.trail += rate * dt
    an.ambient += (fire ? 7 : 3) * (opts.reduceMotion ? 0.3 : 1) * dt
    while (an.trail >= 1) {
      an.trail -= 1
      if (fire) this.parts.spawn(PK_SPARK, Math.random() < 0.5 ? C_FIRE2 : C_CORE, cx - pl.face * rand(2, 6), up(rand(6, 16)), -pl.vx * 0.15 + rand(-10, 10), gv * rand(-40, -10), rand(0.3, 0.55), rand(0.6, 1.1), gv * -40, 1, true)
      else this.parts.spawn(PK_FLAKE, C_FROST2, cx - pl.face * rand(2, 6), up(rand(4, 18)), -pl.vx * 0.12 + rand(-8, 8), gv * rand(-6, 12), rand(0.5, 0.9), rand(1.4, 2.2), gv * 10, 1.5, true, 3)
    }
    while (an.ambient >= 1) {
      an.ambient -= 1
      if (fire) this.parts.spawn(PK_SPARK, C_FIRE2, cx + rand(-3, 3), up(22 + bob), rand(-8, 8), gv * rand(-45, -25), rand(0.35, 0.6), rand(0.5, 0.9), gv * -20, 1, true)
      else this.parts.spawn(PK_STAR, C_WHITE, cx + rand(-7, 7), up(rand(6, 24)), 0, gv * -3, 0.45, rand(1.8, 2.6), 0, 0, true)
      if (gv < 0 && !opts.reduceMotion && Math.random() < 0.6) this.parts.spawn(PK_CHEV, C_GRAV, cx + rand(-10, 10), cy + rand(-4, 12), 0, -rand(30, 60), 0.5, 2, 0, 0, true)
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
    if (an.wasGround && !pl.ground && this.vyRel < 0) an.sqv += 5
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
  private eyes(g: CanvasRenderingContext2D, an: PAnim, t: number, y: number, ink: string, happy: boolean): void {
    const lx = an.look * 1.7
    const ly = this.vyRel < -60 ? -0.7 : this.vyRel > 200 ? 0.6 : 0
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
    const lean = -an.tilt * 40 - clamp(this.vyRel / 400, -1, 1) * 0.5
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
    this.eyes(g, an, t, -10.5, 'rgb(64, 22, 6)', happy)
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
    this.eyes(g, an, t, -9, 'rgb(14, 48, 84)', happy)
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

// a small static picture of a stage for the select screen: the whole stage fitted inside the canvas, whatever
// shape either is, with wall filling the bars. a dark stage is drawn dark around its two starts, and a level
// of several stages (opts.stages) gets a row of pips
export function drawThumb(canvas: HTMLCanvasElement, level: Stage, opts: ThumbOpts = {}): void {
  const g = canvas.getContext('2d')
  if (!g) return
  const p = readPalette(canvas)
  const cw = canvas.width
  const ch = canvas.height
  const k = Math.min(cw / level.w, ch / level.h)
  const ox = (cw - level.w * k) / 2
  const oy = (ch - level.h * k) / 2
  // world px per canvas px: marks that must stay visible on a big stage are sized with it
  const u = Math.max(1, level.w / 640, level.h / 360)
  g.setTransform(1, 0, 0, 1, 0, 0)
  g.clearRect(0, 0, cw, ch)
  g.fillStyle = css(p.wall)
  g.fillRect(0, 0, cw, ch)
  g.setTransform(k, 0, 0, k, ox, oy)
  const sky = g.createLinearGradient(0, 0, 0, level.h)
  sky.addColorStop(0, css(mixRgb(p.bg, p.tape, 0.15)))
  sky.addColorStop(1, css(p.dark ? mixRgb(p.tape, BLACK, 0.12) : mixRgb(p.tape, p.wall, 0.15)))
  g.fillStyle = sky
  g.fillRect(0, 0, level.w, level.h)
  // walls without texture: at this size the shapes are what matter
  paintWalls(g, level, p, false)
  for (let i = 0; i < level.tiles.length; i++) {
    const t = level.tiles[i]
    if (t === 0 || t === T_WALL) continue
    const tx = i % level.cols
    const ty = (i - tx) / level.cols
    const x = tx * TILE
    const y = ty * TILE
    if (t === T_LAVA || t === T_WATER || t === T_GOO || t === T_THIN) {
      const top = ty > 0 && level.tiles[i - level.cols] === t ? 0 : POOL_SINK
      g.fillStyle = css(t === T_LAVA ? LAVA_MID : t === T_WATER ? WATER_TOP : t === T_GOO ? GOO_TOP : THIN_TOP)
      g.fillRect(x, y + top, TILE + 0.5, TILE - top + 0.5)
    } else if (t === T_TRAMP) {
      g.fillStyle = css(mixRgb(METAL, BLACK, 0.3))
      g.fillRect(x, y + 5, TILE + 0.5, TILE - 5)
      g.fillStyle = css(TRAMP)
      g.fillRect(x, y, TILE + 0.5, 6 * Math.min(u, 2))
    } else if (t === T_ONEWAY) {
      g.fillStyle = css(WOOD)
      g.fillRect(x, y, TILE + 0.5, ONEWAY_H * Math.min(u, 2))
    } else if (t === T_CRUMBLE) {
      g.fillStyle = css(mixRgb(SAND, p.wall, 0.3))
      g.fillRect(x + 0.5, y + 0.5, TILE - 1, TILE - 1)
    } else {
      // the blocks that follow the lights: solid the way the stage starts, an outline otherwise
      const sun = t === T_LIGHT
      const there = sun !== level.dark
      g.fillStyle = css(sun ? SUN : MOON, there ? 0.95 : 0.3)
      g.fillRect(x + 1, y + 1, TILE - 2, TILE - 2)
    }
  }
  const solid = (r: Rect, c: RGB) => {
    g.fillStyle = css(c)
    rr(g, r.x, r.y, r.w, r.h, 2)
    g.fill()
  }
  const dot = (x: number, y: number, r: number, c: RGB) => {
    g.fillStyle = css(c)
    g.beginPath()
    g.arc(x, y, r, 0, TAU)
    g.fill()
  }
  for (const r of level.ice) solid(r, ICE_BOT)
  for (const gt of level.gates) solid(gt, p.dark ? mixRgb(METAL, BLACK, 0.2) : METAL)
  for (const l of level.locks) {
    solid(l, mixRgb(METAL, BLACK, 0.35))
    dot(l.x + l.w / 2, l.y + l.h / 2, 5 * Math.min(u, 2), GOLD)
  }
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
  const strip = (r: Rect & { ceil: boolean }, inset: number, h: number): Rect => ({ x: r.x + inset, y: r.ceil ? r.y : r.y + r.h - h, w: r.w - 2 * inset, h })
  for (const pl of level.plates) solid(strip(pl, 0, 4), parseColor(channelColorOf(pl.ch), WHITE))
  for (const b of level.buttons) solid(strip(b, 3, 5), parseColor(channelColorOf(b.ch), WHITE))
  for (const c of level.cannons) {
    const len = Math.hypot(c.vx, c.vy) || 1
    dot(c.x, c.y, 8, mixRgb(METAL, BLACK, 0.3))
    dot(c.x + (c.vx / len) * 8, c.y + (c.vy / len) * 8, 5, mixRgb(METAL, BLACK, 0.3))
  }
  for (const s of level.gswitches) dot(s.x + s.w / 2, s.y + s.h / 2, 6 * Math.min(u, 2), GRAV)
  for (const l of level.lamps) dot(l.x + l.w / 2, l.y + l.h / 2, 6 * Math.min(u, 2), SUN)
  for (const a of level.arrows) dot(a.x, a.y, 5 * Math.min(u, 2), ARROW)
  for (const key of level.keys) dot(key.x, key.y, 5 * Math.min(u, 2), GOLD)
  for (const c of level.checkpoints) {
    g.fillStyle = css(p.star)
    g.fillRect(c.x - 1, c.y - 24, 2 * Math.min(u, 2), 24)
    g.beginPath()
    g.moveTo(c.x, c.y - 24)
    g.lineTo(c.x + 12 * Math.min(u, 2), c.y - 19)
    g.lineTo(c.x, c.y - 14)
    g.closePath()
    g.fill()
  }
  level.doors.forEach((d, i) => {
    const c = i === 0 ? p.fire : p.frost
    const e = (Math.min(u, 2.5) - 1) * 6
    archPath(g, d.x - 2 - e, d.y - 2 - 2 * e, d.w + 4 + 2 * e, d.h + 2 + 2 * e)
    g.fillStyle = css(c)
    g.fill()
    archPath(g, d.x + 2, d.y + 2, d.w - 4, d.h - 2)
    g.fillStyle = css(mixRgb(c, BLACK, p.dark ? 0.6 : 0.35))
    g.fill()
  })
  for (const gm of level.gems) {
    g.save()
    g.translate(gm.x, gm.y)
    g.scale(1.4 * Math.min(u, 2.5), 1.4 * Math.min(u, 2.5))
    gemPath(g, 1)
    g.fillStyle = css(gm.el === 'fire' ? p.fire : p.frost)
    g.fill()
    g.restore()
  }
  const hero = (i: number) => ({ x: level.spawns[i].x + PLAYER_W / 2, y: level.spawns[i].y + PLAYER_H - 9 })
  if (level.dark) {
    // lights off: night over it all (or over its dark patches), and the two glows where they start
    g.save()
    g.beginPath()
    if (level.darks.length) for (const r of level.darks) g.rect(r.x, r.y, r.w, r.h)
    else g.rect(-ox / k, -oy / k, cw / k, ch / k)
    g.clip()
    g.fillStyle = 'rgba(7, 6, 14, 0.8)'
    g.fillRect(-ox / k, -oy / k, cw / k, ch / k)
    for (let i = 0; i < 2; i++) {
      const h = hero(i)
      const r = (i === 0 ? 110 : 60) * Math.min(u, 1.6)
      const gr = g.createRadialGradient(h.x, h.y, 0, h.x, h.y, r)
      gr.addColorStop(0, css(i === 0 ? p.fire : p.frost, 0.55))
      gr.addColorStop(1, css(i === 0 ? p.fire : p.frost, 0))
      g.fillStyle = gr
      g.fillRect(h.x - r, h.y - r, 2 * r, 2 * r)
    }
    g.restore()
  }
  // the two heroes as dots where they start
  for (let i = 0; i < 2; i++) dot(hero(i).x, hero(i).y, 8 * Math.min(u, 2.5), i === 0 ? p.fire : p.frost)
  g.setTransform(1, 0, 0, 1, 0, 0)
  // more stages than this one: a pip each along the bottom, the first one lit
  const n = Math.min(6, Math.floor(opts.stages ?? 1))
  if (n > 1) {
    const r = Math.max(2, ch * 0.035)
    const gap = r * 3.2
    const bw = gap * (n - 1) + r * 4
    const by = ch - r * 2.6
    g.fillStyle = p.dark ? 'rgba(0, 0, 0, 0.6)' : 'rgba(255, 255, 255, 0.78)'
    rr(g, cw / 2 - bw / 2, by - r * 1.8, bw, r * 3.6, r * 1.8)
    g.fill()
    for (let i = 0; i < n; i++) {
      g.fillStyle = i === 0 ? css(p.ink) : css(p.ink, 0.32)
      g.beginPath()
      g.arc(cw / 2 - (gap * (n - 1)) / 2 + i * gap, by, i === 0 ? r : r * 0.8, 0, TAU)
      g.fill()
    }
  }
}
