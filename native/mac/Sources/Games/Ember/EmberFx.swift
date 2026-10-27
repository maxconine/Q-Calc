import CoreGraphics
import Foundation

// emberFx.ts: the renderer's little helpers. colour maths, channel colours, the camera on a canvas, the dark's
// holes, and a fixed particle pool.

typealias RGB = (Double, Double, Double)

func mixRgb(_ a: RGB, _ b: RGB, _ t: Double) -> RGB {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// css(c, alpha): the colour rounded to whole channels, alpha to three places, as the TS's string said
func css(_ c: RGB, _ alpha: Double = 1) -> RGBA {
    let r = jsRound(c.0), g = jsRound(c.1), b = jsRound(c.2)
    if alpha >= 1 { return RGBA(r, g, b) }
    return RGBA(r, g, b, (max(0, alpha) * 1000).rounded() / 1000)
}

func rgbOf(_ c: RGBA) -> RGB { (c.r, c.g, c.b) }

/// Math.round: half up, toward +infinity
@inline(__always) func jsRound(_ x: Double) -> Double { (x + 0.5).rounded(.down) }

func luma(_ c: RGB) -> Double { (0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2) / 255 }

/// small stable string hash (fnv-1a over UTF-16), so a channel gets the same colour on every machine
func hashStr(_ s: String) -> UInt32 {
    var h: UInt32 = 0x811c9dc5
    for u in s.utf16 {
        h ^= UInt32(u)
        h = h &* 0x01000193
    }
    return h
}

/// a cheap deterministic 0..1 noise from an integer
func hash01(_ n: Double) -> Double {
    let i = UInt32(truncatingIfNeeded: Int64(n.rounded(.towardZero)))
    var h = (i ^ 0x9e3779b9) &* 0x85ebca6b
    h ^= h >> 13
    h = h &* 0xc2b2ae35
    h ^= h >> 16
    return Double(h) / 4294967296
}

let CHANNEL_COLORS: [RGBA] = ["#9b7bff", "#ff5fa2", "#1fb39a", "#e3b21c", "#5b7cff", "#d05ad6", "#8fa63a"].map { parseCSSColor($0, .white) }

func channelIndex(_ name: String) -> Int {
    Int(hashStr(name.hasPrefix("!") ? String(name.dropFirst()) : name) % UInt32(CHANNEL_COLORS.count))
}

func channelColor(_ name: String) -> RGBA { CHANNEL_COLORS[channelIndex(name)] }

func condChannels(_ c: [String]?) -> [String] {
    (c ?? []).map { $0.hasPrefix("!") ? String($0.dropFirst()) : $0 }
}

/// whether a condition holds for the channels that are on; no condition means always
func condHolds(_ c: [String]?, _ on: [String], _ lit: Bool = false) -> Bool {
    guard let c else { return true }
    for n in c {
        let neg = n.hasPrefix("!")
        let name = neg ? String(n.dropFirst()) : n
        if (name == Ember.CH_LIT ? lit : on.contains(name)) == neg { return false }
    }
    return true
}

struct Cam {
    var x: Double
    var y: Double
    var w: Double
    var h: Double
    var zoom: Double

    init(x: Double, y: Double, w: Double, h: Double, zoom: Double) {
        self.x = x
        self.y = y
        self.w = w
        self.h = h
        self.zoom = zoom
    }

    init?(_ v: [Double]) {
        guard v.count >= 5 else { return nil }
        self.init(x: v[0], y: v[1], w: v[2], h: v[3], zoom: v[4])
    }

    var list: [Double] { [x, y, w, h, zoom] }
}

/// world px to canvas px: cx = wx * k + ox, the offsets whole pixels
struct View {
    var k: Double
    var ox: Double
    var oy: Double
}

func viewFor(_ cam: Cam, _ width: Double, _ shakeX: Double = 0, _ shakeY: Double = 0) -> View {
    let k = width / cam.w
    return View(k: k, ox: jsRound((shakeX - cam.x) * k), oy: jsRound((shakeY - cam.y) * k))
}

struct TileRange {
    var x0: Int
    var y0: Int
    var x1: Int
    var y1: Int
}

func visibleTiles(_ cam: Cam, _ cols: Int, _ rows: Int, _ pad: Int = 0) -> TileRange {
    let x0 = max(0, Int(floor(cam.x / Ember.TILE)) - pad)
    let y0 = max(0, Int(floor(cam.y / Ember.TILE)) - pad)
    let x1 = min(cols, Int(ceil((cam.x + cam.w) / Ember.TILE)) + pad)
    let y1 = min(rows, Int(ceil((cam.y + cam.h) / Ember.TILE)) + pad)
    return TileRange(x0: x0, y0: y0, x1: max(x0, x1), y1: max(y0, y1))
}

func inView(_ cam: Cam, _ x: Double, _ y: Double, _ w: Double, _ h: Double, _ pad: Double = 0) -> Bool {
    x + w + pad >= cam.x && x - pad <= cam.x + cam.w && y + h + pad >= cam.y && y - pad <= cam.y + cam.h
}

let MIPS: [Double] = [1, 0.5.squareRoot(), 0.5, 0.5.squareRoot() / 2, 0.25]
let CHUNK_PX = 512

func mipFor(_ zoom: Double, _ held: Int = -1) -> Int {
    var best = 0
    var bd = Double.infinity
    for i in 0..<MIPS.count {
        let d = abs(log(zoom / MIPS[i]))
        if d < bd {
            bd = d
            best = i
        }
    }
    if held >= 0 && held < MIPS.count && held != best && abs(log(zoom / MIPS[held])) < bd + 0.06 { return held }
    return best
}

func chunkRange(_ cam: Cam, _ kb: Double, _ size: Int = CHUNK_PX) -> TileRange {
    let w = Double(size) / kb
    return TileRange(x0: Int(floor(cam.x / w)), y0: Int(floor(cam.y / w)), x1: Int(ceil((cam.x + cam.w) / w)), y1: Int(ceil((cam.y + cam.h) / w)))
}

// ---- darkness

struct Hole {
    var x: Double
    var y: Double
    var r: Double
    var a: Double
    var p: Int?
}

let HOLE_CORE = 0.45

func lavaTops(_ stage: EStage) -> [Int] {
    var l: [Int] = []
    for i in 0..<stage.tiles.count where stage.tiles[i] == Ember.T_LAVA && (i < stage.cols || stage.tiles[i - stage.cols] != Ember.T_LAVA) {
        l.append(i)
    }
    return l
}

func lightHoles(_ stage: EStage, _ s: EState, _ lava: [Int], _ at: [EPoint]?) -> [Hole] {
    var out: [Hole] = []
    for i in 0..<2 {
        let p = s.players[i]
        if !p.alive { continue }
        let q = at?[i] ?? EPoint(x: p.x, y: p.y)
        out.append(Hole(x: q.x + Ember.PLAYER_W / 2, y: q.y + Ember.PLAYER_H / 2, r: i == 0 ? Ember.GLOW_EMBER : Ember.GLOW_FROST, a: 1, p: i))
    }
    for i in lava {
        let tx = i % stage.cols
        out.append(Hole(x: Double(tx) * Ember.TILE + Ember.TILE / 2, y: Double((i - tx) / stage.cols) * Ember.TILE + Ember.POOL_SINK, r: Ember.GLOW_SMALL, a: 0.6))
    }
    for l in stage.lamps { out.append(Hole(x: l.x + l.w / 2, y: l.y + l.h / 2, r: Ember.GLOW_SMALL, a: 0.85)) }
    for d in stage.doors { out.append(Hole(x: d.x + d.w / 2, y: d.y + d.h / 2, r: Ember.GLOW_SMALL, a: 0.75)) }
    for (i, g) in stage.gems.enumerated() where !(s.taken.indices.contains(i) && s.taken[i]) {
        out.append(Hole(x: g.x, y: g.y, r: Ember.GLOW_SMALL * 0.7, a: 0.7))
    }
    for p in stage.portals { out.append(Hole(x: p.x + p.w / 2, y: p.y + p.h / 2, r: Ember.GLOW_SMALL, a: 0.75)) }
    if stage.checkpoints.indices.contains(s.cp) {
        let cp = stage.checkpoints[s.cp]
        out.append(Hole(x: cp.x, y: cp.y - Ember.TILE * 0.7, r: Ember.GLOW_SMALL, a: 0.75))
    }
    return out
}

// ---- particles

let PK_DOT = 0
let PK_SPARK = 1
let PK_FLAKE = 2
let PK_CONFETTI = 3
let PK_RING = 4
let PK_STAR = 5
let PK_SHARD = 6
let PK_STEAM = 7
let PK_STREAK = 8
let PK_CHEV = 9
let PK_CHUNK = 10

let MAX_PARTICLES = 400

/// a fixed pool of particles in parallel arrays (Float, as the TS's Float32Arrays)
final class Particles {
    let cap: Int
    var n = 0
    private var over = 0
    var x: [Float], y: [Float], vx: [Float], vy: [Float], life: [Float], max: [Float], size: [Float]
    var grav: [Float], drag: [Float], rot: [Float], spin: [Float]
    var kind: [UInt8], col: [UInt8], add: [UInt8]

    init(_ cap: Int = MAX_PARTICLES) {
        self.cap = cap
        let z = [Float](repeating: 0, count: cap)
        let b = [UInt8](repeating: 0, count: cap)
        x = z; y = z; vx = z; vy = z; life = z; max = z; size = z; grav = z; drag = z; rot = z; spin = z
        kind = b; col = b; add = b
    }

    func clear() { n = 0 }

    func spawn(_ kind: Int, _ col: Int, _ x: Double, _ y: Double, _ vx: Double, _ vy: Double, _ life: Double, _ size: Double,
               _ grav: Double = 0, _ drag: Double = 0, _ add: Bool = false, _ spin: Double = 0) {
        var i = n
        if i >= cap {
            over = (over + 1) % cap
            i = over
        } else {
            n += 1
        }
        self.x[i] = Float(x)
        self.y[i] = Float(y)
        self.vx[i] = Float(vx)
        self.vy[i] = Float(vy)
        self.life[i] = Float(life)
        self.max[i] = Float(life)
        self.size[i] = Float(size)
        self.grav[i] = Float(grav)
        self.drag[i] = Float(drag)
        self.rot[i] = spin == 0 ? 0 : Float((x * 7.31 + y * 3.17).truncatingRemainder(dividingBy: 6.283))
        self.spin[i] = Float(spin)
        self.kind[i] = UInt8(kind)
        self.col[i] = UInt8(col)
        self.add[i] = add ? 1 : 0
    }

    func update(_ dt: Double) {
        let dt = Float(dt)
        var i = 0
        while i < n {
            let l = life[i] - dt
            if l <= 0 {
                n -= 1
                if i < n { copy(n, i) }
                continue
            }
            life[i] = l
            vy[i] += grav[i] * dt
            let damp = 1 / (1 + drag[i] * dt)
            vx[i] *= damp
            vy[i] *= damp
            x[i] += vx[i] * dt
            y[i] += vy[i] * dt
            rot[i] += spin[i] * dt
            i += 1
        }
    }

    private func copy(_ from: Int, _ to: Int) {
        x[to] = x[from]; y[to] = y[from]; vx[to] = vx[from]; vy[to] = vy[from]
        life[to] = life[from]; max[to] = max[from]; size[to] = size[from]; grav[to] = grav[from]
        drag[to] = drag[from]; rot[to] = rot[from]; spin[to] = spin[from]
        kind[to] = kind[from]; col[to] = col[from]; add[to] = add[from]
    }

    /// two passes, plain then additive; `only` draws one of them
    func draw(_ ctx: Canvas2D, _ colors: [RGBA], _ addOp: Composite, _ only: Int = -1) {
        for pass in 0..<2 {
            if only >= 0 && only != pass { continue }
            ctx.globalCompositeOperation = pass == 0 ? .sourceOver : addOp
            for i in 0..<n where Int(add[i]) == pass {
                let c = Int(col[i])
                drawOne(ctx, i, c < colors.count ? colors[c] : .white)
            }
        }
        ctx.globalCompositeOperation = .sourceOver
        ctx.globalAlpha = 1
    }

    private func drawOne(_ ctx: Canvas2D, _ i: Int, _ color: RGBA) {
        let t = Double(life[i] / max[i])
        let x = Double(self.x[i])
        let y = Double(self.y[i])
        let s = Double(size[i])
        let fadeIn = Swift.min(1, (1 - t) * 8)
        switch Int(kind[i]) {
        case PK_DOT, PK_SPARK:
            ctx.globalAlpha = Swift.min(1, t * 2) * fadeIn
            ctx.fill(color)
            let r = s * (0.35 + 0.65 * t)
            ctx.fillRect(x - r, y - r, r * 2, r * 2)
        case PK_FLAKE:
            ctx.globalAlpha = Swift.min(1, t * 2.5) * fadeIn
            ctx.stroke(color)
            ctx.lineWidth = 0.7
            ctx.beginPath()
            let a = Double(rot[i])
            for k in 0..<3 {
                let c = cos(a + Double(k) * .pi / 3) * s
                let d = sin(a + Double(k) * .pi / 3) * s
                ctx.moveTo(x - c, y - d)
                ctx.lineTo(x + c, y + d)
            }
            ctx.stroke()
        case PK_CONFETTI:
            ctx.globalAlpha = Swift.min(1, t * 3)
            ctx.fill(color)
            let a = Double(rot[i])
            let w = s * abs(cos(a * 1.7)) + 0.4
            ctx.save()
            ctx.translate(x, y)
            ctx.rotate(a)
            ctx.fillRect(-w / 2, -s * 0.3, w, s * 0.6)
            ctx.restore()
        case PK_RING:
            ctx.globalAlpha = t * 0.9
            ctx.stroke(color)
            ctx.lineWidth = 0.6 + 1.6 * t
            ctx.beginPath()
            ctx.arc(x, y, 1 + s * (1 - t * t), 0, .pi * 2)
            ctx.stroke()
        case PK_STAR:
            let k = sin(.pi * t)
            ctx.globalAlpha = k
            ctx.fill(color)
            let r = s * k
            let w = r * 0.22
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
        case PK_SHARD:
            ctx.globalAlpha = Swift.min(1, t * 2)
            ctx.fill(color)
            ctx.save()
            ctx.translate(x, y)
            ctx.rotate(Double(rot[i]))
            ctx.beginPath()
            ctx.moveTo(0, -s)
            ctx.lineTo(s * 0.45, s * 0.6)
            ctx.lineTo(-s * 0.45, s * 0.4)
            ctx.closePath()
            ctx.fill()
            ctx.restore()
        case PK_STEAM:
            ctx.globalAlpha = 0.4 * t * fadeIn
            ctx.fill(color)
            ctx.beginPath()
            ctx.arc(x, y, s * (1.5 - t), 0, .pi * 2)
            ctx.fill()
        case PK_STREAK:
            ctx.globalAlpha = 0.45 * Swift.min(1, t * 3) * fadeIn
            ctx.fill(color)
            ctx.fillRect(x - 0.4, y, 0.8, s)
        case PK_CHEV:
            ctx.globalAlpha = Swift.min(1, t * 2.5) * fadeIn
            ctx.stroke(color)
            ctx.lineWidth = 1.1
            let d: Double = vy[i] > 0 ? -1 : 1
            ctx.beginPath()
            ctx.moveTo(x - s, y + s * 0.5 * d)
            ctx.lineTo(x, y - s * 0.5 * d)
            ctx.lineTo(x + s, y + s * 0.5 * d)
            ctx.stroke()
        case PK_CHUNK:
            ctx.globalAlpha = Swift.min(1, t * 2.5)
            ctx.fill(color)
            ctx.save()
            ctx.translate(x, y)
            ctx.rotate(Double(rot[i]))
            ctx.fillRect(-s / 2, -s / 2, s, s * 0.8)
            ctx.restore()
        default:
            break
        }
    }
}
