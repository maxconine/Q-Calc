import AppKit
import Foundation

// emberDraw.ts on Canvas2D: tiles, pools, mechanics, the two characters, particles, glow and the dark. It draws
// ONE stage; world units throughout, one transform maps the camera's rect onto the canvas, and the backdrop and
// walls come from chunks baked once per zoom. Line for line with the TS; EmberDrawThings.swift and
// EmberDrawPlayers.swift hold the rest of the class.

let TAU = Double.pi * 2

@inline(__always) func clamp(_ v: Double, _ a: Double, _ b: Double) -> Double { v < a ? a : v > b ? b : v }
/// frame-rate independent ease toward a target
@inline(__always) func approach(_ v: Double, _ target: Double, _ rate: Double, _ dt: Double) -> Double { v + (target - v) * (1 - exp(-rate * dt)) }
@inline(__always) func rnd() -> Double { Double.random(in: 0..<1) }
@inline(__always) func rand(_ a: Double, _ b: Double) -> Double { a + rnd() * (b - a) }

struct DrawOpts {
    var me: Int?
    var reduceMotion: Bool
}

struct Palette {
    var dark: Bool
    var bg: RGB, tape: RGB, ink: RGB, muted: RGB, accent: RGB
    var fire: RGB, fire2: RGB, frost: RGB, frost2: RGB, star: RGB
    var wall: RGB, wallHi: RGB, wallLo: RGB
    var key: String
}

let WHITE: RGB = (255, 255, 255)
let BLACK: RGB = (0, 0, 0)

/// readPalette: the theme's tokens (index.css, EmberPanel.css), light or dark
func readPalette(dark: Bool) -> Palette {
    let bg: RGB = dark ? (44, 44, 46) : (255, 255, 255)
    let tape: RGB = dark ? (37, 37, 39) : (245, 245, 247)
    let ink: RGB = dark ? (245, 245, 247) : (29, 29, 31)
    let muted: RGB = dark ? (152, 152, 157) : (134, 134, 139)
    let accent: RGB = dark ? (99, 211, 146) : (29, 122, 76)
    let fire: RGB = dark ? (255, 138, 71) : (232, 100, 28)
    let fire2: RGB = dark ? (255, 193, 94) : (245, 165, 36)
    let frost: RGB = dark ? (95, 205, 242) : (31, 147, 198)
    let frost2: RGB = dark ? (165, 232, 251) : (92, 200, 230)
    let star: RGB = dark ? (255, 196, 77) : (240, 168, 26)
    let isDark = luma(bg) < 0.45
    let stone: RGB = isDark ? (92, 88, 108) : (178, 172, 190)
    let wall = mixRgb(mixRgb(tape, ink, isDark ? 0.2 : 0.3), stone, 0.4)
    let wallHi = isDark ? mixRgb(wall, ink, 0.24) : mixRgb(wall, WHITE, 0.6)
    let wallLo = isDark ? mixRgb(wall, BLACK, 0.38) : mixRgb(wall, ink, 0.2)
    return Palette(dark: isDark, bg: bg, tape: tape, ink: ink, muted: muted, accent: accent, fire: fire, fire2: fire2, frost: frost,
                   frost2: frost2, star: star, wall: wall, wallHi: wallHi, wallLo: wallLo, key: isDark ? "dark" : "light")
}

let LAVA_TOP: RGB = (255, 196, 84)
let LAVA_MID: RGB = (255, 106, 31)
let LAVA_BOT: RGB = (190, 38, 26)
let WATER_TOP: RGB = (104, 190, 250)
let WATER_BOT: RGB = (28, 92, 190)
let GOO_TOP: RGB = (170, 236, 80)
let GOO_BOT: RGB = (58, 140, 34)
let THIN_TOP: RGB = (176, 224, 248)
let THIN_BOT: RGB = (110, 178, 226)
let ICE_TOP: RGB = (236, 250, 255)
let ICE_BOT: RGB = (168, 220, 244)
let WOOD: RGB = (178, 118, 62)
let WOOD_DARK: RGB = (116, 70, 34)
let METAL: RGB = (128, 134, 150)
let TRAMP: RGB = (238, 78, 104)
let SAND: RGB = (204, 160, 104)
let SUN: RGB = (255, 214, 120)
let MOON: RGB = (128, 116, 226)
let GRAV: RGB = (190, 132, 255)
let ARROW: RGB = (72, 214, 150)
let GOLD: RGB = (246, 196, 62)
let GOLD_DARK: RGB = (150, 104, 22)
let NIGHT = RGBA(7, 6, 14)
let PORTAL_COLORS: [RGB] = [(168, 112, 255), (255, 122, 214), (64, 222, 196), (255, 212, 92)]

let C_FIRE = 0, C_FIRE2 = 1, C_FROST = 2, C_FROST2 = 3, C_WHITE = 4, C_STEAM = 5, C_GOO = 6, C_LAVA = 7, C_WATER = 8
let C_DUST = 9, C_STAR = 10, C_CORE = 11, C_ICE = 12, C_PORTAL = 13
let C_CH = C_PORTAL + PORTAL_COLORS.count
let C_TRAMP = C_CH + CHANNEL_COLORS.count
let C_SAND = C_TRAMP + 1, C_SUN = C_TRAMP + 2, C_GRAV = C_TRAMP + 3, C_ARROW = C_TRAMP + 4, C_GOLD = C_TRAMP + 5, C_SMOKE = C_TRAMP + 6

func colorTable(_ p: Palette) -> [RGBA] {
    var t = [RGBA](repeating: .white, count: C_SMOKE + 1)
    t[C_FIRE] = css(p.fire)
    t[C_FIRE2] = css(p.fire2)
    t[C_FROST] = css(p.frost)
    t[C_FROST2] = css(p.frost2)
    t[C_WHITE] = p.dark ? RGBA(255, 255, 255) : css(mixRgb(p.frost2, WHITE, 0.3))
    t[C_STEAM] = p.dark ? RGBA(220, 228, 236) : RGBA(170, 180, 192)
    t[C_GOO] = css(GOO_TOP)
    t[C_LAVA] = css(LAVA_MID)
    t[C_WATER] = css(WATER_TOP)
    t[C_DUST] = css(p.dark ? mixRgb(p.wall, p.ink, 0.35) : mixRgb(p.wall, p.ink, 0.15))
    t[C_STAR] = css(p.star)
    t[C_CORE] = p.dark ? RGBA(255, 246, 214) : css(mixRgb(p.star, LAVA_MID, 0.25))
    t[C_ICE] = css(p.dark ? ICE_TOP : mixRgb(ICE_BOT, p.frost, 0.35))
    for (i, c) in PORTAL_COLORS.enumerated() { t[C_PORTAL + i] = css(c) }
    for (i, c) in CHANNEL_COLORS.enumerated() { t[C_CH + i] = c }
    t[C_TRAMP] = css(TRAMP)
    t[C_SAND] = css(p.dark ? SAND : mixRgb(SAND, BLACK, 0.2))
    t[C_SUN] = css(p.dark ? SUN : mixRgb(SUN, LAVA_MID, 0.35))
    t[C_GRAV] = css(p.dark ? GRAV : mixRgb(GRAV, BLACK, 0.12))
    t[C_ARROW] = css(p.dark ? ARROW : mixRgb(ARROW, BLACK, 0.18))
    t[C_GOLD] = css(GOLD)
    t[C_SMOKE] = p.dark ? RGBA(120, 120, 132) : RGBA(96, 96, 108)
    return t
}

// ---- shared shape helpers

/// a rect with its own radius per corner (tl, tr, br, bl)
func roundRectPath(_ g: Canvas2D, _ x: Double, _ y: Double, _ w: Double, _ h: Double, _ tl: Double, _ tr: Double, _ br: Double, _ bl: Double) {
    g.beginPath()
    g.moveTo(x + tl, y)
    g.lineTo(x + w - tr, y)
    if tr != 0 { g.arcTo(x + w, y, x + w, y + tr, tr) } else { g.lineTo(x + w, y) }
    g.lineTo(x + w, y + h - br)
    if br != 0 { g.arcTo(x + w, y + h, x + w - br, y + h, br) } else { g.lineTo(x + w, y + h) }
    g.lineTo(x + bl, y + h)
    if bl != 0 { g.arcTo(x, y + h, x, y + h - bl, bl) } else { g.lineTo(x, y + h) }
    g.lineTo(x, y + tl)
    if tl != 0 { g.arcTo(x, y, x + tl, y, tl) } else { g.lineTo(x, y) }
    g.closePath()
}

func rr(_ g: Canvas2D, _ x: Double, _ y: Double, _ w: Double, _ h: Double, _ r: Double) {
    let k = min(r, w / 2, h / 2)
    roundRectPath(g, x, y, w, h, k, k, k, k)
}

func flameGlyph(_ g: Canvas2D, _ x: Double, _ y: Double, _ h: Double) {
    let w = h * 0.42
    let b = y + h * 0.5
    g.beginPath()
    g.moveTo(x, b)
    g.bezierCurveTo(x - w * 1.1, b, x - w * 1.15, y - h * 0.02, x - w * 0.6, y - h * 0.24)
    g.quadraticCurveTo(x - w * 0.5, y - h * 0.02, x - w * 0.18, y - h * 0.06)
    g.bezierCurveTo(x - w * 0.35, y - h * 0.3, x - w * 0.05, y - h * 0.42, x + w * 0.2, y - h * 0.5)
    g.bezierCurveTo(x + w * 0.3, y - h * 0.25, x + w * 1.15, y - h * 0.12, x + w * 0.98, y + h * 0.2)
    g.bezierCurveTo(x + w * 0.88, b, x + w * 0.4, b, x, b)
    g.closePath()
}

func flakeGlyph(_ g: Canvas2D, _ x: Double, _ y: Double, _ r: Double) {
    g.beginPath()
    for k in 0..<6 {
        let a = Double(k) * .pi / 3 - .pi / 2
        let c = cos(a), s = sin(a)
        g.moveTo(x, y)
        g.lineTo(x + c * r, y + s * r)
        let bx = x + c * r * 0.6, by = y + s * r * 0.6, b = r * 0.3
        g.moveTo(bx, by)
        g.lineTo(bx + cos(a + 0.8) * b, by + sin(a + 0.8) * b)
        g.moveTo(bx, by)
        g.lineTo(bx + cos(a - 0.8) * b, by + sin(a - 0.8) * b)
    }
}

func gemPath(_ g: Canvas2D, _ s: Double) {
    g.beginPath()
    g.moveTo(-5 * s, -1.5 * s)
    g.lineTo(-2.8 * s, -4.5 * s)
    g.lineTo(2.8 * s, -4.5 * s)
    g.lineTo(5 * s, -1.5 * s)
    g.lineTo(0, 5.5 * s)
    g.closePath()
}

func archPath(_ g: Canvas2D, _ x: Double, _ y: Double, _ w: Double, _ h: Double) {
    let r = w / 2
    g.beginPath()
    g.moveTo(x, y + h)
    g.lineTo(x, y + r)
    g.arc(x + r, y + r, r, .pi, 0)
    g.lineTo(x + w, y + h)
    g.closePath()
}

func isWall(_ level: EStage, _ tx: Int, _ ty: Int) -> Bool {
    if tx < 0 || ty < 0 || tx >= level.cols || ty >= level.rows { return true }
    return level.tiles[ty * level.cols + tx] == Ember.T_WALL
}

func closedTile(_ level: EStage, _ tx: Int, _ ty: Int) -> Bool {
    if tx < 0 || ty < 0 || tx >= level.cols || ty >= level.rows { return true }
    let t = level.tiles[ty * level.cols + tx]
    return t != 0 && t != Ember.T_ONEWAY && t != Ember.T_CRUMBLE && t != Ember.T_LIGHT && t != Ember.T_DARK
}

func keyPath(_ g: Canvas2D, _ s: Double) {
    g.beginPath()
    g.arc(-3.4 * s, 0, 2.5 * s, 0, TAU)
    g.moveTo(-0.9 * s, 0)
    g.lineTo(6 * s, 0)
    g.moveTo(3.4 * s, 0)
    g.lineTo(3.4 * s, 2.4 * s)
    g.moveTo(5.6 * s, 0)
    g.lineTo(5.6 * s, 2 * s)
}

func chevrons(_ g: Canvas2D, _ s: Double) {
    g.beginPath()
    g.moveTo(-3.6 * s, 0.2 * s)
    g.lineTo(0, -3.4 * s)
    g.lineTo(3.6 * s, 0.2 * s)
    g.moveTo(-3.6 * s, 3.8 * s)
    g.lineTo(0, 0.2 * s)
    g.lineTo(3.6 * s, 3.8 * s)
}

// ---- the static layer: backdrop + walls

func paintBackdrop(_ g: Canvas2D, _ level: EStage, _ p: Palette, _ range: TileRange?) {
    let W = level.w, H = level.h, TILE = Ember.TILE
    let sky = CanvasGradient(.linear(CGPoint(x: 0, y: 0), CGPoint(x: 0, y: H)))
    sky.addColorStop(0, css(mixRgb(p.bg, p.tape, p.dark ? 0.1 : 0.15)))
    sky.addColorStop(1, css(p.dark ? mixRgb(p.tape, BLACK, 0.12) : mixRgb(p.tape, p.wall, 0.12)))
    g.fill(sky)
    g.fillRect(0, 0, W, H)
    let warm = CanvasGradient(.radial(CGPoint(x: W * 0.12, y: H * 1.05), 0, CGPoint(x: W * 0.12, y: H * 1.05), H * 0.95))
    warm.addColorStop(0, css(p.fire, p.dark ? 0.1 : 0.07))
    warm.addColorStop(1, css(p.fire, 0))
    g.fill(warm)
    g.fillRect(0, 0, W, H)
    let cool = CanvasGradient(.radial(CGPoint(x: W * 0.88, y: H * 1.05), 0, CGPoint(x: W * 0.88, y: H * 1.05), H * 0.95))
    cool.addColorStop(0, css(p.frost, p.dark ? 0.1 : 0.07))
    cool.addColorStop(1, css(p.frost, 0))
    g.fill(cool)
    g.fillRect(0, 0, W, H)
    let bw = TILE * 2, bh = TILE
    let r0 = range.map { max(0, $0.y0) } ?? 0
    let r1 = range.map { min(level.rows, $0.y1) } ?? level.rows
    let bx0 = range.map { Double(max(0, Int(floor(Double($0.x0) * TILE / bw)) - 1)) * bw } ?? 0
    let bx1 = range.map { min(W, Double($0.x1) * TILE) } ?? W
    if r0 < r1 {
        for row in r0..<r1 {
            let off = row % 2 != 0 ? bw / 2 : 0
            var bx = bx0 - off
            while bx < bx1 {
                let n = hash01(Double(row * 97) + jsRound(bx) * 13 + 5)
                g.fill(css(p.ink, (p.dark ? 0.018 : 0.012) + n * (p.dark ? 0.02 : 0.016)))
                rr(g, bx + 1, Double(row) * bh + 1, bw - 2, bh - 2, 3)
                g.fill()
                bx += bw
            }
        }
    }
    let vg = CanvasGradient(.radial(CGPoint(x: W / 2, y: H / 2), H * 0.35, CGPoint(x: W / 2, y: H / 2), W * 0.65))
    vg.addColorStop(0, RGBA(0, 0, 0, 0))
    vg.addColorStop(1, p.dark ? RGBA(0, 0, 0, 0.28) : RGBA(40, 30, 60, 0.07))
    g.fill(vg)
    g.fillRect(0, 0, W, H)
}

func paintWalls(_ g: Canvas2D, _ level: EStage, _ p: Palette, _ detail: Bool, _ range: TileRange?) {
    let R = 5.0, TILE = Ember.TILE
    let tx0 = range?.x0 ?? 0, ty0 = range?.y0 ?? 0
    let tx1 = range?.x1 ?? level.cols, ty1 = range?.y1 ?? level.rows
    guard tx0 < tx1, ty0 < ty1 else { return }
    g.fill(p.dark ? RGBA(0, 0, 0, 0.32) : RGBA(40, 30, 70, 0.10))
    for ty in ty0..<ty1 {
        for tx in tx0..<tx1 {
            if !isWall(level, tx, ty) { continue }
            if closedTile(level, tx, ty + 1) && closedTile(level, tx + 1, ty) { continue }
            let u = !closedTile(level, tx, ty - 1), d = !closedTile(level, tx, ty + 1)
            let l = !closedTile(level, tx - 1, ty), r = !closedTile(level, tx + 1, ty)
            roundRectPath(g, Double(tx) * TILE + 1, Double(ty) * TILE + 2.5, TILE, TILE, u && l ? R : 0, u && r ? R : 0, d && r ? R : 0, d && l ? R : 0)
            g.fill()
        }
    }
    let base = css(p.wall), hi = css(p.wallHi), lo = css(p.wallLo)
    let mortar = css(p.wallLo, p.dark ? 0.35 : 0.22)
    let speck = css(p.dark ? p.wallHi : p.wallLo, 0.25)
    for ty in ty0..<ty1 {
        for tx in tx0..<tx1 {
            if !isWall(level, tx, ty) { continue }
            let x = Double(tx) * TILE, y = Double(ty) * TILE
            let u = !closedTile(level, tx, ty - 1), d = !closedTile(level, tx, ty + 1)
            let l = !closedTile(level, tx - 1, ty), r = !closedTile(level, tx + 1, ty)
            g.save()
            roundRectPath(g, x, y, TILE, TILE, u && l ? R : 0, u && r ? R : 0, d && r ? R : 0, d && l ? R : 0)
            g.fill(base)
            g.fill()
            g.clip()
            if detail {
                g.fill(mortar)
                g.fillRect(x, y + TILE / 2 - 0.5, TILE, 1)
                let j = ty & 1 != 0 ? 0 : TILE / 2
                g.fillRect(x + j - 0.5, y, 1, TILE / 2)
                g.fillRect(x + (j + TILE / 2).truncatingRemainder(dividingBy: TILE) - 0.5, y + TILE / 2, 1, TILE / 2)
                g.fill(speck)
                for k in 0..<3 {
                    let n = hash01(Double(ty * level.cols * 7 + tx * 7 + k))
                    let m = hash01(Double(ty * level.cols * 11 + tx * 5 + k + 999))
                    g.fillRect(x + 2 + n * (TILE - 4), y + 2 + m * (TILE - 4), 1, 1)
                }
            }
            if u {
                g.fill(hi)
                g.fillRect(x, y, TILE, 3)
                g.fill(css(p.wallLo, 0.25))
                g.fillRect(x, y + 3, TILE, 1)
            }
            if d {
                g.fill(lo)
                g.fillRect(x, y + TILE - 2.5, TILE, 2.5)
            }
            if l {
                g.fill(css(p.dark ? p.wallHi : p.wallLo, 0.35))
                g.fillRect(x, y, 1.5, TILE)
            }
            if r {
                g.fill(css(p.wallLo, 0.55))
                g.fillRect(x + TILE - 1.5, y, 1.5, TILE)
            }
            g.restore()
        }
    }
}

// ---- per-player animation state

struct PAnim {
    var sq = 0.0, sqv = 0.0, run = 0.0, tilt = 0.0, look = 1.0
    var blinkAt = 1 + rnd() * 2, blinkUntil = 0.0
    var wasGround = true, trail = 0.0, ambient = 0.0, x = 0.0, y = 0.0, flip = 0.0, aura = 0.0
}

/// one frame's input: the stage, the states, the world as it behaves now, and what happened
struct EmberDrawInput {
    var level: EStage
    var prev: EState?
    var cur: EState
    var world: EmberWorldNow
    var alpha: Double
    var events: [EEvent]
    var nowMs: Double
    var opts: DrawOpts
}

final class EmberRenderer {
    var pal: Palette
    var colors: [RGBA]
    let parts = Particles(MAX_PARTICLES)
    var chunks: [String: Canvas2D] = [:]
    var chunkOrder: [String] = []
    var statKey = ""
    var mip = -1
    var cam: Cam?
    var view = View(k: 1, ox: 0, oy: 0)
    var vr = Cam(x: 0, y: 0, w: 640, h: 360, zoom: 1)
    var far = false
    var pos: [EPoint] = [EPoint(x: 0, y: 0), EPoint(x: 0, y: 0)]
    var beams: [(Double, Double, Double, Double)] = []
    var world = EmberWorldNow(tiles: [], beams: [], gates: [])
    var mask: Canvas2D?
    var hole: CGImage?
    var darkA = 0.0
    var glowBoost = 0.0
    var flash = 0.0
    var lampMax = 0.0
    var irisAt = -9.0
    var irisDur = 0.5
    var crumbleIndex: [Int: Int] = [:]
    var trampAt: [Int: Double] = [:]
    var mendAt: [Int: Double] = [:]
    var cpAt: [Double] = []
    var cpUp: [Double] = []
    var lockGone: [Double] = []
    var cannonKick: [Double] = []
    var gswSpin: [Double] = []
    var lampOn: [Double] = []
    var keyHolder = 0
    var keyTrail: [EPoint] = []
    var vyRel = 0.0
    var glowCache: [String: CGImage] = [:]
    var poolGrad: [Int: CanvasGradient] = [:]
    var levelKey: String?
    var levelIndex = -1
    var lavaTopsList: [Int] = []
    var thinIndex: [Int: Int] = [:]
    var portalCol: [Int] = []
    var lastMs = 0.0
    var shake = 0.0
    var winAt = -1.0
    var anims = [PAnim(), PAnim()]
    var leverAng: [Double] = []
    var mirrorAng: [Double] = []
    var fanAng: [Double] = []
    var fanSpin: [Double] = []
    var iceLast: [Double] = []
    var doorOpen: [Double] = [0, 0]
    var textW: [String: Double] = [:]
    var px: [Double] = [0, 0]
    var py: [Double] = [0, 0]
    var W = 2.0
    var H = 2.0

    init(dark: Bool) {
        pal = readPalette(dark: dark)
        colors = colorTable(pal)
    }

    /// a new shape is a new camera
    func resized() {
        statKey = ""
        cam = nil
    }

    /// forget particles and effects (new level)
    func reset() {
        parts.clear()
        shake = 0
        winAt = -1
        levelKey = nil
        lastMs = 0
        cam = nil
        irisAt = -9
        flash = 0
        glowBoost = 0
    }

    /// the theme changed
    func refreshColors(dark: Bool) {
        guard dark != pal.dark else { return }
        pal = readPalette(dark: dark)
        colors = colorTable(pal)
        glowCache.removeAll()
        poolGrad.removeAll()
        statKey = ""
        hole = nil
    }

    private func setLevel(_ level: EStage, key: String) {
        levelKey = key
        levelIndex = level.index
        statKey = ""
        chunks.removeAll()
        chunkOrder.removeAll()
        cam = nil
        lavaTopsList = lavaTops(level)
        thinIndex = [:]
        for (j, ti) in level.thin.enumerated() { thinIndex[ti] = j }
        crumbleIndex = [:]
        for (j, ti) in level.crumble.enumerated() { crumbleIndex[ti] = j }
        trampAt = [:]
        mendAt = [:]
        cpAt = level.checkpoints.map { _ in -9 }
        cpUp = level.checkpoints.map { _ in 0 }
        lockGone = level.locks.map { _ in 0 }
        cannonKick = level.cannons.map { _ in 0 }
        gswSpin = level.gswitches.map { _ in 0 }
        lampOn = level.lamps.map { _ in 0 }
        lampMax = level.lamps.reduce(0) { max($0, $1.ticks) }
        keyTrail = []
        keyHolder = 0
        flash = 0
        glowBoost = 0
        var pairs: [String] = []
        portalCol = level.portals.map { p in
            var k = pairs.firstIndex(of: p.pair) ?? -1
            if k < 0 {
                pairs.append(p.pair)
                k = pairs.count - 1
            }
            return k % PORTAL_COLORS.count
        }
        leverAng = level.levers.map { $0.on ? 1 : -1 }
        mirrorAng = level.mirrors.map { $0.slant == "/" ? -Double.pi / 4 : Double.pi / 4 }
        fanAng = level.fans.map { _ in 0 }
        fanSpin = level.fans.map { _ in 0 }
        iceLast = level.ice.map { _ in 0 }
        doorOpen = [0, 0]
        anims = [PAnim(), PAnim()]
        parts.clear()
    }

    // ---- glow sprites

    private func glowSprite(_ color: RGBA) -> CGImage? {
        let id = "\(color.r),\(color.g),\(color.b)"
        if let s = glowCache[id] { return s }
        guard let s = Canvas2D(width: 64, height: 64) else { return nil }
        let c = rgbOf(color)
        let grad = CanvasGradient(.radial(CGPoint(x: 32, y: 32), 0, CGPoint(x: 32, y: 32), 32))
        grad.addColorStop(0, css(c, 0.9))
        grad.addColorStop(0.25, css(c, 0.45))
        grad.addColorStop(0.6, css(c, 0.12))
        grad.addColorStop(1, css(c, 0))
        s.fill(grad)
        s.fillRect(0, 0, 64, 64)
        let img = s.image()
        glowCache[id] = img
        return img
    }

    func glow(_ g: Canvas2D, _ x: Double, _ y: Double, _ r: Double, _ color: RGBA, _ a: Double) {
        if a <= 0.003 { return }
        g.globalAlpha = min(1, a * (pal.dark ? 1 : 0.5))
        g.drawImage(glowSprite(color), x - r, y - r, r * 2, r * 2)
    }

    func beginGlow(_ g: Canvas2D) { g.globalCompositeOperation = pal.dark ? .lighter : .sourceOver }

    func endGlow(_ g: Canvas2D) {
        g.globalCompositeOperation = .sourceOver
        g.globalAlpha = 1
    }

    func vis(_ x: Double, _ y: Double, _ w: Double, _ h: Double, _ pad: Double = 24) -> Bool { inView(vr, x, y, w, h, pad) }

    // ---- static layer

    private func bakeChunk(_ level: EStage, _ kb: Double, _ ix: Int, _ iy: Int) -> Canvas2D? {
        guard let c = Canvas2D(width: CHUNK_PX, height: CHUNK_PX) else { return nil }
        let p = pal
        let px = Double(CHUNK_PX)
        c.fill(css(p.wall))
        c.fillRect(0, 0, px, px)
        c.setTransform(kb, 0, 0, kb, -Double(ix) * px, -Double(iy) * px)
        let w = px / kb
        let T = Ember.TILE
        let range = TileRange(x0: Int(floor(Double(ix) * w / T)) - 1, y0: Int(floor(Double(iy) * w / T)) - 1,
                              x1: Int(ceil(Double(ix + 1) * w / T)) + 1, y1: Int(ceil(Double(iy + 1) * w / T)) + 1)
        if range.x1 > 0 && range.y1 > 0 && range.x0 < level.cols && range.y0 < level.rows {
            c.save()
            c.beginPath()
            c.rect(0, 0, level.w, level.h)
            c.clip()
            paintBackdrop(c, level, p, range)
            c.restore()
        }
        paintWalls(c, level, p, kb >= 1.2, range)
        return c
    }

    private func drawStatic(_ g: Canvas2D, _ level: EStage, _ W: Double, _ H: Double) {
        let key = "\(pal.key)|\(W)x\(H)"
        if key != statKey {
            chunks.removeAll()
            chunkOrder.removeAll()
            statKey = key
            mip = -1
        }
        let v = view
        let bv = emberBaseView(W / H)
        let k1 = W / bv.0
        mip = mipFor(v.k / k1, mip)
        let kb = k1 * MIPS[mip]
        let s = v.k / kb
        let r = chunkRange(vr, kb)
        let cap = Int(ceil(2.4 * W * H / Double(CHUNK_PX * CHUNK_PX))) + 8
        let P = Double(CHUNK_PX)
        if r.y0 < r.y1 && r.x0 < r.x1 {
            for iy in r.y0..<r.y1 {
                for ix in r.x0..<r.x1 {
                    let id = "\(mip)|\(ix)|\(iy)"
                    var c = chunks[id]
                    if c != nil {
                        chunkOrder.removeAll { $0 == id }
                    } else {
                        c = bakeChunk(level, kb, ix, iy)
                        chunks[id] = c
                    }
                    chunkOrder.append(id)
                    let x0 = jsRound(Double(ix) * P * s) + v.ox
                    let y0 = jsRound(Double(iy) * P * s) + v.oy
                    let x1 = jsRound(Double(ix + 1) * P * s) + v.ox
                    let y1 = jsRound(Double(iy + 1) * P * s) + v.oy
                    g.drawImage(c, x0, y0, x1 - x0, y1 - y0)
                }
            }
        }
        while chunks.count > cap, let first = chunkOrder.first {
            chunkOrder.removeFirst()
            chunks[first] = nil
        }
    }

    // ---- particles helpers

    func burst(_ kind: Int, _ col: Int, _ x: Double, _ y: Double, _ n: Int, _ speed: Double, _ life: Double, _ size: Double,
               _ grav: Double = 0, _ drag: Double = 2, _ add: Bool = false, _ spin: Double = 0, _ up: Double = 0) {
        for _ in 0..<max(0, n) {
            let a = rnd() * TAU
            let v = speed * (0.35 + rnd() * 0.65)
            parts.spawn(kind, col, x, y, cos(a) * v, sin(a) * v - up, life * (0.6 + rnd() * 0.6), size * (0.7 + rnd() * 0.6), grav, drag, add, spin != 0 ? rand(-spin, spin) : 0)
        }
    }

    func chCol(_ name: String?) -> Int { name.map { C_CH + channelIndex($0) } ?? C_WHITE }

    // swiftlint:disable:next cyclomatic_complexity function_body_length
    private func onEvent(_ level: EStage, _ cur: EState, _ e: EEvent, _ opts: DrawOpts, _ t: Double) {
        let less = opts.reduceMotion ? 0.4 : 1
        let n = { (k: Double) -> Int in max(1, Int(jsRound(k * less))) }
        let PW = Ember.PLAYER_W, PH = Ember.PLAYER_H, TILE = Ember.TILE
        let pi = e.p ?? 0
        let pl = cur.players[min(1, max(0, pi))]
        let gv: Double = pl.grav == -1 ? -1 : 1
        let fx = pl.x + PW / 2
        let fy = gv > 0 ? pl.y + PH : pl.y
        let i = e.i ?? 0
        switch e.k {
        case "jump":
            for _ in 0..<n(5) { parts.spawn(PK_DOT, C_DUST, fx + rand(-5, 5), fy - gv, rand(-30, 30), gv * rand(-25, -5), rand(0.25, 0.4), rand(1.2, 2), gv * 40, 4) }
            if pi == 0 { burst(PK_SPARK, C_FIRE2, fx, fy - gv * 4, n(4), 50, 0.35, 0.9, -60, 3, true) } else { burst(PK_FLAKE, C_FROST2, fx, fy - gv * 4, n(3), 40, 0.5, 2, 20, 3, true, 4) }
        case "land":
            for k in 0..<n(7) {
                let s: Double = k % 2 != 0 ? 1 : -1
                parts.spawn(PK_DOT, C_DUST, fx + s * rand(2, 6), fy - gv, s * rand(25, 60), gv * rand(-18, -4), rand(0.25, 0.45), rand(1.2, 2.2), gv * 60, 5)
            }
        case "airjump":
            burst(PK_RING, C_ARROW, fx, fy, 1, 0, 0.4, 14, 0, 0, true)
            for _ in 0..<n(5) { parts.spawn(PK_CHEV, C_ARROW, fx + rand(-7, 7), fy + gv * rand(0, 4), rand(-12, 12), gv * rand(50, 110), rand(0.3, 0.5), rand(2, 3), 0, 2.5, true) }
            burst(PK_STAR, C_WHITE, fx, fy, n(4), 60, 0.4, 2.6, 0, 3, true)
        case "bounce":
            let ti = trampNear(level, e.ex, e.ey)
            if ti >= 0 { trampAt[ti] = t }
            let cx = pl.x + PW / 2
            let dir: Double = pl.vy > 0 ? 1 : -1
            let by = dir < 0 ? pl.y + PH : pl.y
            let strong = clamp(((e.v ?? 0) - 420) / 280, 0.2, 1)
            burst(PK_RING, C_TRAMP, cx, by, 1, 0, 0.35, 10 + strong * 10, 0, 0, true)
            for k in 0..<n(4 + strong * 8) { parts.spawn(PK_STREAK, k % 3 != 0 ? C_WHITE : C_TRAMP, cx + rand(-6, 6), by + dir * rand(0, 26) - (dir < 0 ? 12 : 0), 0, dir * rand(60, 160) * strong, rand(0.25, 0.45), rand(8, 16), 0, 1.5, true) }
            for _ in 0..<n(6) { parts.spawn(PK_DOT, C_TRAMP, cx + rand(-8, 8), by, rand(-70, 70), dir * rand(20, 90), rand(0.25, 0.45), rand(1, 1.8), -dir * 200, 2) }
        case "arrow":
            let a = level.arrows.indices.contains(i) ? level.arrows[i] : EPoint(x: e.ex, y: e.ey)
            burst(PK_RING, C_ARROW, a.x, a.y, 1, 0, 0.45, 18, 0, 0, true)
            burst(PK_STAR, C_ARROW, a.x, a.y, n(8), 100, 0.5, 3.2, 0, 3, true)
            for k in 0..<n(6) { parts.spawn(PK_CHEV, k % 2 != 0 ? C_WHITE : C_ARROW, a.x + rand(-8, 8), a.y + rand(-4, 6), rand(-20, 20), -rand(60, 130), rand(0.35, 0.6), rand(2, 3.2), 0, 2, true) }
        case "arrowback":
            let a = level.arrows.indices.contains(i) ? level.arrows[i] : EPoint(x: e.ex, y: e.ey)
            burst(PK_RING, C_ARROW, a.x, a.y, 1, 0, 0.35, 11, 0, 0, true)
            burst(PK_STAR, C_WHITE, a.x, a.y, n(3), 30, 0.4, 2.4, 0, 3, true)
        case "key":
            keyHolder = pi
            let a = level.keys.indices.contains(i) ? level.keys[i] : EPoint(x: e.ex, y: e.ey)
            burst(PK_RING, C_GOLD, a.x, a.y, 1, 0, 0.45, 16, 0, 0, true)
            burst(PK_STAR, C_GOLD, a.x, a.y, n(9), 100, 0.6, 3.2, 0, 3, true)
        case "unlock":
            let r = level.locks.indices.contains(i) ? level.locks[i] : nil
            let cx = r.map { $0.x + $0.w / 2 } ?? e.ex, cy = r.map { $0.y + $0.h / 2 } ?? e.ey
            let hw = r.map { $0.w / 2 } ?? TILE / 2, hh = r.map { $0.h / 2 } ?? TILE / 2
            let area = hw * hh * 4 / (TILE * TILE)
            for k in 0..<n(min(26, 6 + area * 4)) { parts.spawn(PK_CHUNK, k % 3 != 0 ? C_SMOKE : C_GOLD, cx + rand(-hw, hw), cy + rand(-hh, hh), rand(-90, 90), rand(-150, -20), rand(0.5, 0.9), rand(2, 3.6), 520, 1, false, 10) }
            burst(PK_RING, C_GOLD, cx, cy, 1, 0, 0.5, 14 + max(hw, hh), 0, 0, true)
            burst(PK_STAR, C_GOLD, cx, cy, n(8), 90, 0.6, 3.2, 0, 3, true)
        case "load":
            let c = level.cannons.indices.contains(i) ? EPoint(x: level.cannons[i].x, y: level.cannons[i].y) : EPoint(x: e.ex, y: e.ey)
            burst(PK_RING, pi == 0 ? C_FIRE : C_FROST, c.x, c.y, 1, 0, 0.3, 13, 0, 0, true)
            burst(PK_DOT, C_SMOKE, c.x, c.y, n(5), 40, 0.35, 1.6, 0, 4)
        case "fire":
            let c = level.cannons.indices.contains(i) ? level.cannons[i] : nil
            if cannonKick.indices.contains(i) { cannonKick[i] = 1 }
            if !opts.reduceMotion { shake = max(shake, 0.55) }
            var len = c.map { hypot($0.vx, $0.vy) } ?? 1
            if len == 0 { len = 1 }
            let ux = c.map { $0.vx / len } ?? 0, uy = c.map { $0.vy / len } ?? -1
            let mx = (c?.x ?? e.ex) + ux * 13, my = (c?.y ?? e.ey) + uy * 13
            for k in 0..<n(14) {
                let sp = rand(120, 320), side = rand(-0.5, 0.5)
                parts.spawn(PK_SPARK, k % 2 != 0 ? C_CORE : pi == 0 ? C_FIRE2 : C_FROST2, mx, my, (ux - uy * side) * sp, (uy + ux * side) * sp, rand(0.25, 0.5), rand(0.8, 1.4), 0, 3, true)
            }
            for _ in 0..<n(9) { parts.spawn(PK_STEAM, C_SMOKE, mx + rand(-3, 3), my + rand(-3, 3), ux * rand(20, 80) + rand(-20, 20), uy * rand(20, 80) + rand(-20, 20), rand(0.5, 0.9), rand(3, 6), -14, 2) }
            burst(PK_RING, C_WHITE, mx, my, 1, 0, 0.3, 16, 0, 0, true)
        case "flip":
            let cx = pl.x + PW / 2, cy = pl.y + PH / 2
            burst(PK_RING, C_GRAV, cx, cy, 1, 0, 0.5, 22, 0, 0, true)
            burst(PK_RING, C_WHITE, cx, cy, 1, 0, 0.3, 12, 0, 0, true)
            for k in 0..<n(12) { parts.spawn(PK_CHEV, k % 3 != 0 ? C_GRAV : C_WHITE, cx + rand(-11, 11), cy + rand(-12, 12), 0, (e.grav ?? 1) * rand(70, 170), rand(0.4, 0.7), rand(2.2, 3.4), 0, 1.5, true) }
        case "gswitch":
            if gswSpin.indices.contains(i) { gswSpin[i] = 1 }
            burst(PK_RING, C_GRAV, e.ex, e.ey, 1, 0, 0.4, 15, 0, 0, true)
            burst(PK_SPARK, C_GRAV, e.ex, e.ey, n(8), 90, 0.4, 1, 0, 2, true)
        case "checkpoint":
            let c = level.checkpoints.indices.contains(i) ? level.checkpoints[i] : nil
            if cpAt.indices.contains(i) { cpAt[i] = t }
            let cx = c?.x ?? e.ex, cy = c.map { $0.y - 20 } ?? e.ey
            burst(PK_RING, C_STAR, cx, cy, 1, 0, 0.6, 26, 0, 0, true)
            burst(PK_STAR, C_STAR, cx, cy, n(10), 110, 0.7, 3.4, 0, 2.5, true)
            let cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR]
            for k in 0..<n(16) { parts.spawn(PK_CONFETTI, cols[k % cols.count], cx + rand(-4, 4), cy, rand(-80, 80), rand(-220, -110), rand(0.9, 1.4), rand(2.4, 3.6), 380, 1.6, false, 9) }
        case "light":
            if e.on == true {
                if !opts.reduceMotion { flash = 1 }
                glowBoost = 0
                burst(PK_RING, C_SUN, e.ex, e.ey, 1, 0, 0.5, 70, 0, 0, true)
                burst(PK_RING, C_WHITE, e.ex, e.ey, 1, 0, 0.35, 34, 0, 0, true)
                burst(PK_STAR, C_SUN, e.ex, e.ey, n(8), 120, 0.5, 3, 0, 3, true)
            } else {
                glowBoost = opts.reduceMotion ? 0 : 1
                flash = 0
                burst(PK_RING, C_GRAV, e.ex, e.ey, 1, 0, 0.4, 26, 0, 0, true)
                burst(PK_SPARK, C_SUN, e.ex, e.ey, n(6), 60, 0.5, 0.9, 160, 2, true)
            }
        case "shake":
            for _ in 0..<n(4) { parts.spawn(PK_DOT, C_SAND, e.ex + rand(-9, 9), e.ey + rand(-2, 9), rand(-14, 14), rand(5, 30), rand(0.3, 0.5), rand(1, 1.7), 260, 1) }
        case "break":
            for _ in 0..<n(7) { parts.spawn(PK_CHUNK, C_SAND, e.ex + rand(-8, 8), e.ey + rand(-8, 8), rand(-50, 50), rand(-60, 30), rand(0.55, 0.95), rand(2.6, 5), 620, 0.6, false, 9) }
            for _ in 0..<n(5) { parts.spawn(PK_STEAM, C_DUST, e.ex + rand(-8, 8), e.ey + rand(-4, 6), rand(-20, 20), rand(-16, 6), rand(0.4, 0.7), rand(2.5, 4.5), 0, 2) }
        case "mend":
            if let tile = e.tile { mendAt[tile] = t }
            burst(PK_STAR, C_SAND, e.ex, e.ey, n(3), 30, 0.4, 2.6, 0, 3, true)
        case "clear":
            for d in level.doors {
                burst(PK_STAR, C_STAR, d.x + d.w / 2, d.y + d.h / 2, n(8), 110, 0.7, 3.2, 0, 2, true)
                burst(PK_RING, C_STAR, d.x + d.w / 2, d.y + d.h / 2, 1, 0, 0.6, 28, 0, 0, true)
            }
        case "respawn":
            irisAt = t
            irisDur = 0.38
            keyTrail = []
            for k in 0..<2 {
                let q = cur.players[k]
                let cx = q.x + PW / 2, cy = q.y + PH / 2
                burst(PK_RING, k == 0 ? C_FIRE : C_FROST, cx, cy, 1, 0, 0.5, 20, 0, 0, true)
                burst(PK_STAR, k == 0 ? C_FIRE2 : C_FROST2, cx, cy, n(6), 70, 0.5, 2.8, 0, 3, true)
            }
            if let cp = e.cp, cpAt.indices.contains(cp) { cpAt[cp] = t }
        case "die":
            let cx = pl.x + PW / 2, cy = pl.y + PH / 2
            if !opts.reduceMotion { shake = 1 }
            switch e.cause {
            case "water":
                for _ in 0..<n(16) { parts.spawn(PK_STEAM, C_STEAM, cx + rand(-6, 6), cy + rand(-4, 6), rand(-20, 20), rand(-60, -25), rand(0.6, 1.1), rand(3, 6), -10, 1.5) }
                burst(PK_SPARK, C_FIRE2, cx, cy, n(12), 90, 0.5, 1, 120, 2, true)
                burst(PK_RING, C_STEAM, cx, cy + 4, 1, 0, 0.5, 14)
            case "lava":
                for k in 0..<n(18) { parts.spawn(PK_DOT, k % 3 != 0 ? C_FROST : C_FROST2, cx + rand(-6, 6), cy + rand(-8, 4), rand(-70, 70), rand(-140, -40), rand(0.5, 0.9), rand(1.2, 2.2), 520, 1) }
                burst(PK_SHARD, C_ICE, cx, cy, n(6), 90, 0.6, 3, 300, 1, false, 12, 50)
                for _ in 0..<n(8) { parts.spawn(PK_STEAM, C_STEAM, cx + rand(-5, 5), cy, rand(-15, 15), rand(-50, -20), rand(0.5, 0.9), rand(3, 5), -10, 1.5) }
            case "crush":
                let c1 = pi == 0 ? C_FIRE : C_FROST, c2 = pi == 0 ? C_FIRE2 : C_FROST2
                for k in 0..<n(22) {
                    let s: Double = k % 2 != 0 ? 1 : -1
                    parts.spawn(k % 3 != 0 ? PK_DOT : PK_CHUNK, k % 2 != 0 ? c1 : c2, cx + s * rand(0, 5), cy + rand(-3, 3), s * rand(80, 260), rand(-30, 30), rand(0.35, 0.7), rand(1.4, 2.8), 300, 2.5, false, 10)
                }
                for _ in 0..<n(8) { parts.spawn(PK_STEAM, C_DUST, cx + rand(-10, 10), cy + rand(-10, 10), rand(-30, 30), rand(-20, 10), rand(0.4, 0.7), rand(3, 5), 0, 2) }
                burst(PK_RING, C_WHITE, cx, cy, 1, 0, 0.3, 18, 0, 0, true)
                burst(PK_STAR, C_WHITE, cx, cy, n(4), 60, 0.35, 3, 0, 3, true)
            default:
                for k in 0..<n(20) { parts.spawn(PK_DOT, k % 4 != 0 ? C_GOO : pi == 0 ? C_FIRE : C_FROST, cx + rand(-5, 5), cy + 4, rand(-80, 80), rand(-170, -60), rand(0.5, 0.9), rand(1.3, 2.5), 560, 0.8) }
                burst(PK_RING, C_GOO, cx, cy + 6, 1, 0, 0.45, 16)
            }
        case "door":
            if level.doors.indices.contains(pi) {
                let d = level.doors[pi]
                burst(PK_RING, pi == 0 ? C_FIRE : C_FROST, d.x + d.w / 2, d.y + d.h / 2, 1, 0, 0.6, 22, 0, 0, true)
                burst(PK_STAR, pi == 0 ? C_FIRE2 : C_FROST2, d.x + d.w / 2, d.y + d.h / 2, n(6), 50, 0.6, 3, 0, 2, true)
            }
        case "portal":
            let col = portalNear(level, e.ex, e.ey)
            burst(PK_RING, col, e.ex, e.ey, 1, 0, 0.45, 16, 0, 0, true)
            burst(PK_RING, col, pl.x + PW / 2, pl.y + PH / 2, 1, 0, 0.5, 18, 0, 0, true)
            burst(PK_STAR, col, pl.x + PW / 2, pl.y + PH / 2, n(8), 70, 0.5, 2.5, 0, 3, true)
        case "push":
            if rnd() < 0.5 { parts.spawn(PK_DOT, C_DUST, e.ex + rand(-4, 4), e.ey, rand(-10, 10), rand(-15, -5), 0.35, 1.4, 30, 4) }
        case "gem":
            let c = e.el == "fire" ? C_FIRE : C_FROST, c2 = e.el == "fire" ? C_FIRE2 : C_FROST2
            burst(PK_RING, c2, e.ex, e.ey, 1, 0, 0.45, 16, 0, 0, true)
            burst(PK_STAR, c2, e.ex, e.ey, n(10), 110, 0.6, 3.5, 0, 3, true)
            burst(PK_SPARK, c, e.ex, e.ey, n(14), 140, 0.5, 1, 160, 2, true)
        case "lever", "mirror":
            let col = e.k == "lever" ? chCol(level.levers.indices.contains(i) ? level.levers[i].ch : nil) : C_WHITE
            burst(PK_RING, col, e.ex, e.ey, 1, 0, 0.35, 10, 0, 0, true)
            burst(PK_SPARK, col, e.ex, e.ey, n(5), 60, 0.3, 0.9, 100, 2, true)
        case "plate", "button", "sensor":
            if e.on != true { break }
            let ch: String? = e.k == "plate" ? (level.plates.indices.contains(i) ? level.plates[i].ch : nil)
                : e.k == "button" ? (level.buttons.indices.contains(i) ? level.buttons[i].ch : nil)
                : (level.sensors.indices.contains(i) ? level.sensors[i].ch : nil)
            burst(PK_RING, chCol(ch), e.ex, e.ey, 1, 0, 0.4, 12, 0, 0, true)
        case "melt":
            let r = level.ice.indices.contains(i) ? level.ice[i] : nil
            let cx = r.map { $0.x + $0.w / 2 } ?? e.ex, cy = r.map { $0.y + $0.h / 2 } ?? e.ey
            let hw = r.map { $0.w / 2 } ?? TILE / 2, hh = r.map { $0.h / 2 } ?? TILE / 2
            let area = hw * hh * 4 / (TILE * TILE)
            burst(PK_SHARD, C_ICE, cx, cy, n(min(30, 8 + area * 4)), 130, 0.8, 3.5, 380, 1, false, 14, 60)
            for _ in 0..<n(min(16, 6 + area * 2)) { parts.spawn(PK_STEAM, C_STEAM, cx + rand(-hw, hw), cy + rand(-hh, hh), rand(-10, 10), rand(-45, -20), rand(0.6, 1), rand(3, 6), -8, 1.5) }
        case "freeze":
            burst(PK_FLAKE, C_FROST2, e.ex, e.ey, n(5), 40, 0.6, 2.2, 0, 3, true, 3)
            burst(PK_STAR, C_WHITE, e.ex, e.ey - 4, n(2), 20, 0.5, 2.5, 0, 3, true)
        case "thaw":
            for _ in 0..<n(4) { parts.spawn(PK_DOT, C_WATER, e.ex + rand(-8, 8), e.ey - 4, rand(-10, 10), rand(-50, -20), 0.4, 1.2, 300, 1) }
        case "win":
            winAt = t
            confetti(n(80))
            for d in level.doors { burst(PK_STAR, C_STAR, d.x + d.w / 2, d.y + d.h / 2, n(10), 120, 0.8, 3.5, 0, 2, true) }
        default:
            break
        }
    }

    private func portalNear(_ level: EStage, _ x: Double, _ y: Double) -> Int {
        var best = 0, bd = Double.infinity
        for (i, p) in level.portals.enumerated() {
            let d = abs(p.x + p.w / 2 - x) + abs(p.y + p.h / 2 - y)
            if d < bd {
                bd = d
                best = i
            }
        }
        return C_PORTAL + (portalCol.indices.contains(best) ? portalCol[best] : 0)
    }

    private func trampNear(_ level: EStage, _ x: Double, _ y: Double) -> Int {
        let TILE = Ember.TILE
        let cx = Int(floor(x / TILE)), cy = Int(floor(y / TILE))
        var best = -1, bd = Double.infinity
        for ty in (cy - 2)...(cy + 2) {
            for tx in (cx - 1)...(cx + 1) {
                if tx < 0 || ty < 0 || tx >= level.cols || ty >= level.rows || level.tiles[ty * level.cols + tx] != Ember.T_TRAMP { continue }
                let d = abs((Double(tx) + 0.5) * TILE - x) + abs((Double(ty) + 0.5) * TILE - y)
                if d < bd {
                    bd = d
                    best = ty * level.cols + tx
                }
            }
        }
        return best
    }

    func confetti(_ n: Int) {
        let cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR]
        let v = vr
        let u = v.w / 640
        for i in 0..<n {
            let fromLeft = i % 2 == 0
            let x = fromLeft ? v.x + rand(-10, 60) * u : v.x + v.w - rand(-10, 60) * u
            parts.spawn(PK_CONFETTI, cols[i % cols.count], x, v.y + v.h - rand(-10, 110) * u, (fromLeft ? 1 : -1) * rand(60, 200) * u, rand(-420, -260) * u, rand(1.6, 2.6), rand(3, 5) * u, 320 * u, 1.4, false, 9)
        }
    }

    // ---- the frame

    /// one frame on a canvas of W × H pixels. events are everything since the last frame
    func draw(_ g: Canvas2D, _ input: EmberDrawInput) {
        let level = input.level
        let cur = input.cur
        world = input.world
        let W = Double(g.width), H = Double(g.height)
        self.W = W
        self.H = H
        if W < 2 || H < 2 { return }
        let nowMs = input.nowMs
        let opts = input.opts
        let dt = lastMs != 0 ? clamp((nowMs - lastMs) / 1000, 0, 0.05) : 1.0 / 60
        lastMs = nowMs
        let t = nowMs / 1000
        let key = "\(level.index)|\(level.cols)x\(level.rows)|\(level.tiles.count)|\(level.spawns.first?.x ?? 0)"
        let fresh = levelKey != key
        if fresh {
            if levelKey != nil || level.index > 0 {
                irisAt = t
                irisDur = 0.5
            }
            setLevel(level, key: key)
            darkA = cur.lit ? 0 : 1
        }
        let a = clamp(input.alpha, 0, 1)
        let p0 = input.prev

        var snap = fresh
        for e in input.events {
            if e.k == "respawn" || e.k == "stage" { snap = true }
            onEvent(level, cur, e, opts, t)
        }
        if snap { cam = nil }

        for i in 0..<2 {
            let c = cur.players[i]
            let p = p0?.players[i]
            let isFar = p == nil || abs(p!.x - c.x) > 40 || abs(p!.y - c.y) > 40
            px[i] = isFar ? c.x : p!.x + (c.x - p!.x) * a
            py[i] = isFar ? c.y : p!.y + (c.y - p!.y) * a
            pos[i] = EPoint(x: px[i], y: py[i])
        }

        let target = emberCameraFor(level.w, level.h, px[0], py[0], px[1], py[1], W / H)
        let eased = emberEaseCamera(cam, target, dt * 60)
        cam = eased

        var sx = 0.0, sy = 0.0
        if shake > 0 && !opts.reduceMotion {
            let amp = shake * shake * 3 / max(0.4, eased.zoom)
            sx = rand(-amp, amp)
            sy = rand(-amp, amp)
        }
        shake = max(0, shake - dt * 3)

        let v = viewFor(eased, W, sx, sy)
        view = v
        vr = Cam(x: -v.ox / v.k, y: -v.oy / v.k, w: W / v.k, h: H / v.k, zoom: eased.zoom)
        far = eased.zoom < 0.55
        beams = stride(from: 0, to: world.beams.count - 3, by: 4).map { (world.beams[$0], world.beams[$0 + 1], world.beams[$0 + 2], world.beams[$0 + 3]) }

        darkA = approach(darkA, cur.lit ? 0 : 1, cur.lit ? 16 : 9, dt)
        if abs(darkA - (cur.lit ? 0 : 1)) < 0.004 { darkA = cur.lit ? 0 : 1 }
        glowBoost = max(0, glowBoost - dt * 3.2)
        flash = max(0, flash - dt * 4.5)

        g.resetTransform()
        g.globalAlpha = 1
        g.globalCompositeOperation = .sourceOver
        g.fill(css(pal.wall))
        g.fillRect(0, 0, W, H)
        drawStatic(g, level, W, H)
        g.setTransform(v.k, 0, 0, v.k, v.ox, v.oy)

        drawFanColumns(g, level, cur, dt, opts)
        drawMoverRails(g, level)
        drawTiles(g, level, cur, t, opts)
        drawSigns(g, level, t)
        drawCheckpoints(g, level, cur, t, dt)
        drawDoors(g, level, cur, t, dt)
        drawPortals(g, level, t, opts)
        drawPools(g, level, cur, t, dt, opts)
        drawPlates(g, level, cur, t)
        drawButtons(g, level, cur, t)
        drawLevers(g, level, cur, dt)
        drawLamps(g, level, cur, t, dt)
        drawGswitches(g, level, t, dt)
        drawSensors(g, level, cur, t)
        drawEmitters(g, level, cur, t)
        drawMirrors(g, level, cur, dt)
        drawGates(g, level, cur)
        drawLocks(g, level, cur, t, dt)
        drawMovers(g, level, p0, cur, a)
        drawIce(g, level, cur, t)
        drawBoxes(g, p0, cur, a)
        drawGems(g, level, cur, t, opts)
        drawKeys(g, level, cur, t, opts)
        drawArrows(g, level, cur, t, opts)
        drawBeams(g, t, opts)
        drawCannons(g, level, cur, t, dt, opts)
        for i in 0..<2 { drawPlayer(g, i, cur, t, dt, opts) }
        drawCarried(g, cur, t, dt)

        parts.update(dt)
        parts.draw(g, colors, .sourceOver, 0)
        drawDarkness(g, level, cur, t, W, H, opts)
        g.setTransform(v.k, 0, 0, v.k, v.ox, v.oy)
        parts.draw(g, colors, pal.dark ? .lighter : .sourceOver, 1)

        if cur.phase == "won" && winAt >= 0 && t - winAt < 1.6 && rnd() < (opts.reduceMotion ? 9 : 30) * dt { confetti(2) }
        if cur.phase == "won" && winAt < 0 { winAt = t }

        g.resetTransform()
        g.globalAlpha = 1
        g.globalCompositeOperation = .sourceOver
        if cur.phase == "dead" {
            let d = clamp((cur.phaseT - 18) / 30, 0, 1)
            if d > 0 {
                g.fill(pal.dark ? RGBA(0, 0, 0, (0.3 * d * 1000).rounded() / 1000) : RGBA(30, 24, 48, (0.16 * d * 1000).rounded() / 1000))
                g.fillRect(0, 0, W, H)
            }
        }
        if flash > 0.01 {
            g.fill(RGBA(255, 244, 214, (0.3 * flash * flash * 1000).rounded() / 1000))
            g.fillRect(0, 0, W, H)
        }
        drawLampClock(g, cur, t, W, H)
        drawWipe(g, level, cur, a, t, W, H, opts)
        g.resetTransform()
        g.globalAlpha = 1
    }

    // ---- the dark

    private func holeSprite() -> CGImage? {
        if let hole { return hole }
        guard let s = Canvas2D(width: 128, height: 128) else { return nil }
        let gr = CanvasGradient(.radial(CGPoint(x: 64, y: 64), 0, CGPoint(x: 64, y: 64), 64))
        gr.addColorStop(0, RGBA(0, 0, 0, 1))
        gr.addColorStop(HOLE_CORE, RGBA(0, 0, 0, 1))
        gr.addColorStop(HOLE_CORE + (1 - HOLE_CORE) * 0.5, RGBA(0, 0, 0, 0.42))
        gr.addColorStop(1, RGBA(0, 0, 0, 0))
        s.fill(gr)
        s.fillRect(0, 0, 128, 128)
        hole = s.image()
        return hole
    }

    private func drawDarkness(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ W: Double, _ H: Double, _ opts: DrawOpts) {
        var lvl = darkA
        if cur.litT > 0 && cur.litT < 90 && !opts.reduceMotion {
            let near = 1 - cur.litT / 90
            let blink = sin(t * (14 + near * 22)) > 0.35 ? 0.1 + 0.3 * near : 0
            lvl = cur.lit ? max(lvl, blink) : min(lvl, 1 - blink)
        }
        if lvl < 0.004 { return }
        let q = 4.0
        let mw = Int(ceil(W / q)), mh = Int(ceil(H / q))
        if mask == nil || mask!.width != mw || mask!.height != mh { mask = Canvas2D(width: mw, height: mh) }
        guard let m = mask else { return }
        let v = view
        let k = v.k / q, ox = v.ox / q, oy = v.oy / q
        m.resetTransform()
        m.globalCompositeOperation = .sourceOver
        m.globalAlpha = 1
        m.clearRect(0, 0, Double(mw), Double(mh))
        m.fill(NIGHT)
        if !level.darks.isEmpty {
            for r in level.darks where vis(r.x, r.y, r.w, r.h, 0) { m.fillRect(r.x * k + ox, r.y * k + oy, r.w * k, r.h * k) }
        } else {
            m.fillRect(0, 0, Double(mw), Double(mh))
        }
        m.globalCompositeOperation = .destinationOut
        let sprite = holeSprite()
        let boost = 1 + 2.2 * glowBoost * glowBoost
        let holes = lightHoles(level, cur, lavaTopsList, pos)
        for h in holes {
            var r = h.r
            if let hp = h.p { r *= boost * (hp == 0 && !opts.reduceMotion ? 1 + 0.025 * sin(t * 9) + 0.012 * sin(t * 23.7) : 1) }
            if !vis(h.x - r, h.y - r, 2 * r, 2 * r, 0) { continue }
            m.globalAlpha = h.a
            m.drawImage(sprite, (h.x - r) * k + ox, (h.y - r) * k + oy, 2 * r * k, 2 * r * k)
        }
        if !beams.isEmpty {
            m.setTransform(k, 0, 0, k, ox, oy)
            m.stroke(.black)
            m.lineCap = .round
            for pass in 0..<2 {
                m.globalAlpha = pass != 0 ? 0.6 : 0.3
                m.lineWidth = pass != 0 ? Ember.GLOW_SMALL * 0.6 : Ember.GLOW_SMALL * 1.5
                m.beginPath()
                for b in beams {
                    m.moveTo(b.0, b.1)
                    m.lineTo(b.2, b.3)
                }
                m.stroke()
            }
        }
        g.resetTransform()
        g.globalCompositeOperation = .sourceOver
        g.globalAlpha = lvl
        g.imageSmoothingEnabled = true
        g.drawImage(m.image(), 0, 0, Double(mw) * q, Double(mh) * q)
        g.globalAlpha = 1
        g.setTransform(v.k, 0, 0, v.k, v.ox, v.oy)
        beginGlow(g)
        for h in holes {
            guard let hp = h.p else { break }
            let r = h.r * boost
            if !vis(h.x - r, h.y - r, 2 * r, 2 * r, 0) { continue }
            glow(g, h.x, h.y, r * 1.05, css(hp == 0 ? pal.fire : pal.frost), (hp == 0 ? 0.3 : 0.26) * lvl)
        }
        endGlow(g)
    }

    private func drawLampClock(_ g: Canvas2D, _ cur: EState, _ t: Double, _ W: Double, _ H: Double) {
        if !(cur.litT > 0) || lampMax <= 0 { return }
        let u = max(1, min(W / 640, H / 360))
        let frac = clamp(cur.litT / lampMax, 0, 1)
        let w = 84 * u, x = W / 2 - w / 2, y = 9 * u
        let col = cur.lit ? SUN : MOON
        let low = frac < 0.25 && sin(t * 20) > 0
        g.resetTransform()
        g.fill(RGBA(0, 0, 0, 0.45))
        rr(g, x - 9 * u, y - 4 * u, w + 13 * u, 8 * u, 4 * u)
        g.fill()
        g.fill(css(col, 0.28))
        rr(g, x, y - 1.5 * u, w, 3 * u, 1.5 * u)
        g.fill()
        g.fill(css(low ? WHITE : col))
        rr(g, x, y - 1.5 * u, max(3 * u, w * frac), 3 * u, 1.5 * u)
        g.fill()
        g.beginPath()
        g.arc(x - 4.5 * u, y, 2.4 * u, 0, TAU)
        g.fill()
        if !cur.lit {
            g.fill(RGBA(0, 0, 0, 0.75))
            g.beginPath()
            g.arc(x - 3.5 * u, y - 0.8 * u, 2 * u, 0, TAU)
            g.fill()
        }
    }

    private func drawWipe(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ a: Double, _ t: Double, _ W: Double, _ H: Double, _ opts: DrawOpts) {
        var f = 0.0, wx = 0.0, wy = 0.0
        if cur.phase == "clear" {
            f = clamp((cur.phaseT + a - 26) / (Ember.STAGE_TICKS - 32), 0, 1)
            wx = (level.doors[0].x + level.doors[1].x + Ember.TILE) / 2
            wy = (level.doors[0].y + level.doors[1].y) / 2 + Ember.TILE
        } else if t - irisAt < irisDur && t >= irisAt {
            f = 1 - (t - irisAt) / irisDur
            wx = (px[0] + px[1] + Ember.PLAYER_W) / 2
            wy = (py[0] + py[1] + Ember.PLAYER_H) / 2
        }
        if f <= 0.001 { return }
        let v = view
        let cx = wx * v.k + v.ox, cy = wy * v.k + v.oy
        let dark = pal.dark || darkA > 0.5
        let col: RGB = dark ? (8, 7, 14) : mixRgb(pal.tape, pal.wall, 0.35)
        g.resetTransform()
        if opts.reduceMotion {
            g.fill(css(col, f))
            g.fillRect(0, 0, W, H)
            return
        }
        let reach = hypot(max(cx, W - cx), max(cy, H - cy))
        let e = f * f * (3 - 2 * f)
        let r = reach * (1 - e)
        g.fill(css(col))
        g.beginPath()
        g.rect(0, 0, W, H)
        if r > 0.5 { g.arc(cx, cy, r, 0, TAU, true) }
        g.fillEvenOdd()
        if r > 0.5 {
            g.lineWidth = max(2, v.k * 1.4)
            g.stroke(css(pal.fire, 0.8))
            g.beginPath()
            g.arc(cx, cy, r, .pi * 0.5, .pi * 1.5)
            g.stroke()
            g.stroke(css(pal.frost, 0.8))
            g.beginPath()
            g.arc(cx, cy, r, .pi * 1.5, .pi * 2.5)
            g.stroke()
        }
    }

    // helpers the drawing calls

    func at<T>(_ a: [T], _ i: Int) -> T? { a.indices.contains(i) ? a[i] : nil }
}

/// the camera from Rust (emberCamera.ts)
func emberCameraFor(_ w: Double, _ h: Double, _ ax: Double, _ ay: Double, _ bx: Double, _ by: Double, _ aspect: Double) -> Cam {
    Cam(emberCameraFor(stageW: w, stageH: h, ax: ax, ay: ay, bx: bx, by: by, aspect: aspect)) ?? Cam(x: 0, y: 0, w: 640, h: 360, zoom: 1)
}

func emberEaseCamera(_ from: Cam?, _ to: Cam, _ ticks: Double) -> Cam {
    Cam(emberEaseCamera(from: from?.list ?? [], to: to.list, ticks: ticks)) ?? to
}

func emberBaseView(_ aspect: Double) -> (Double, Double) {
    let v = emberBaseView(aspect: aspect)
    return v.count >= 2 ? (v[0], v[1]) : (640, 360)
}
