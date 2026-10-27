//! emberFx.ts (the Mac's EmberFx.swift): the renderer's little helpers. Colour maths, channel colours, the camera on
//! a canvas, the dark's holes, and a fixed particle pool.

use std::cell::Cell;

use qcalc_core::games::ember::{
    Camera, Cond, GameState, GLOW_EMBER, GLOW_FROST, GLOW_SMALL, PLAYER_H, PLAYER_W, POOL_SINK, Stage, T_LAVA, TILE,
};

use crate::games::canvas::{Canvas, Col, Composite, hex, js_round};

pub type Rgb = (f64, f64, f64);

pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// css(c, alpha): the colour rounded to whole channels, alpha to three places, as the TS's string said
pub fn css(c: Rgb, alpha: f64) -> Col {
    let (r, g, b) = (js_round(c.0), js_round(c.1), js_round(c.2));
    if alpha >= 1.0 {
        return Col::rgb(r, g, b);
    }
    Col::rgba(r, g, b, (alpha.max(0.0) * 1000.0).round() / 1000.0)
}

pub fn css1(c: Rgb) -> Col {
    css(c, 1.0)
}

pub fn rgb_of(c: Col) -> Rgb {
    (c.r, c.g, c.b)
}

pub fn luma(c: Rgb) -> f64 {
    (0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2) / 255.0
}

/// small stable string hash (fnv-1a over UTF-16), so a channel gets the same colour on every machine
pub fn hash_str(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for u in s.encode_utf16() {
        h ^= u32::from(u);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// a cheap deterministic 0..1 noise from an integer
pub fn hash01(n: f64) -> f64 {
    let i = (n.trunc() as i64) as u32;
    let mut h = (i ^ 0x9e37_79b9).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    f64::from(h) / 4_294_967_296.0
}

pub const CHANNEL_HEX: [&str; 7] = ["#9b7bff", "#ff5fa2", "#1fb39a", "#e3b21c", "#5b7cff", "#d05ad6", "#8fa63a"];

pub fn channel_colors() -> [Col; 7] {
    CHANNEL_HEX.map(hex)
}

pub fn channel_index(name: &str) -> usize {
    (hash_str(name.strip_prefix('!').unwrap_or(name)) % CHANNEL_HEX.len() as u32) as usize
}

pub fn channel_color(name: &str) -> Col {
    hex(CHANNEL_HEX[channel_index(name)])
}

pub fn cond_channels(c: Option<&Cond>) -> Vec<String> {
    c.map_or(vec![], |c| c.0.iter().map(|n| n.strip_prefix('!').unwrap_or(n).to_string()).collect())
}

/// whether a condition holds for the channels that are on; no condition means always
pub fn cond_holds(c: Option<&Cond>, on: &[String], lit: bool) -> bool {
    let Some(c) = c else { return true };
    for n in &c.0 {
        let neg = n.starts_with('!');
        let name = n.strip_prefix('!').unwrap_or(n);
        let is_on = if name == qcalc_core::games::ember::CH_LIT { lit } else { on.iter().any(|o| o == name) };
        if is_on == neg {
            return false;
        }
    }
    true
}

/// world px to canvas px: cx = wx * k + ox, the offsets whole pixels
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub k: f64,
    pub ox: f64,
    pub oy: f64,
}

pub fn view_for(cam: &Camera, width: f64, shake_x: f64, shake_y: f64) -> View {
    let k = width / cam.w;
    View { k, ox: js_round((shake_x - cam.x) * k), oy: js_round((shake_y - cam.y) * k) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRange {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

pub fn visible_tiles(cam: &Camera, cols: usize, rows: usize, pad: i64) -> TileRange {
    let x0 = 0.max((cam.x / TILE).floor() as i64 - pad);
    let y0 = 0.max((cam.y / TILE).floor() as i64 - pad);
    let x1 = (cols as i64).min(((cam.x + cam.w) / TILE).ceil() as i64 + pad);
    let y1 = (rows as i64).min(((cam.y + cam.h) / TILE).ceil() as i64 + pad);
    TileRange { x0, y0, x1: x0.max(x1), y1: y0.max(y1) }
}

pub fn in_view(cam: &Camera, x: f64, y: f64, w: f64, h: f64, pad: f64) -> bool {
    x + w + pad >= cam.x && x - pad <= cam.x + cam.w && y + h + pad >= cam.y && y - pad <= cam.y + cam.h
}

pub const MIPS: [f64; 5] = [1.0, std::f64::consts::FRAC_1_SQRT_2, 0.5, std::f64::consts::FRAC_1_SQRT_2 / 2.0, 0.25];
pub const CHUNK_PX: i64 = 512;

pub fn mip_for(zoom: f64, held: i64) -> usize {
    let mut best = 0;
    let mut bd = f64::INFINITY;
    for (i, m) in MIPS.iter().enumerate() {
        let d = (zoom / m).ln().abs();
        if d < bd {
            bd = d;
            best = i;
        }
    }
    if held >= 0 && (held as usize) < MIPS.len() && held as usize != best && (zoom / MIPS[held as usize]).ln().abs() < bd + 0.06 {
        return held as usize;
    }
    best
}

pub fn chunk_range(cam: &Camera, kb: f64) -> TileRange {
    let w = CHUNK_PX as f64 / kb;
    TileRange {
        x0: (cam.x / w).floor() as i64,
        y0: (cam.y / w).floor() as i64,
        x1: ((cam.x + cam.w) / w).ceil() as i64,
        y1: ((cam.y + cam.h) / w).ceil() as i64,
    }
}

// ---- darkness

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub a: f64,
    pub p: Option<usize>,
}

pub const HOLE_CORE: f64 = 0.45;

pub fn lava_tops(stage: &Stage) -> Vec<usize> {
    (0..stage.tiles.len())
        .filter(|&i| stage.tiles[i] == T_LAVA && (i < stage.cols || stage.tiles[i - stage.cols] != T_LAVA))
        .collect()
}

pub fn light_holes(stage: &Stage, s: &GameState, lava: &[usize], at: &[(f64, f64); 2]) -> Vec<Hole> {
    let mut out = Vec::new();
    for (i, p) in s.players.iter().enumerate() {
        if !p.alive {
            continue;
        }
        let q = at[i];
        out.push(Hole {
            x: q.0 + PLAYER_W / 2.0,
            y: q.1 + PLAYER_H / 2.0,
            r: if i == 0 { GLOW_EMBER } else { GLOW_FROST },
            a: 1.0,
            p: Some(i),
        });
    }
    for &i in lava {
        let tx = i % stage.cols;
        out.push(Hole {
            x: tx as f64 * TILE + TILE / 2.0,
            y: ((i - tx) / stage.cols) as f64 * TILE + POOL_SINK,
            r: GLOW_SMALL,
            a: 0.6,
            p: None,
        });
    }
    for l in &stage.lamps {
        out.push(Hole { x: l.r.x + l.r.w / 2.0, y: l.r.y + l.r.h / 2.0, r: GLOW_SMALL, a: 0.85, p: None });
    }
    for d in &stage.doors {
        out.push(Hole { x: d.x + d.w / 2.0, y: d.y + d.h / 2.0, r: GLOW_SMALL, a: 0.75, p: None });
    }
    for (i, g) in stage.gems.iter().enumerate() {
        if !s.taken.get(i).copied().unwrap_or(false) {
            out.push(Hole { x: g.x, y: g.y, r: GLOW_SMALL * 0.7, a: 0.7, p: None });
        }
    }
    for p in &stage.portals {
        out.push(Hole { x: p.r.x + p.r.w / 2.0, y: p.r.y + p.r.h / 2.0, r: GLOW_SMALL, a: 0.75, p: None });
    }
    if s.cp >= 0
        && let Some(cp) = stage.checkpoints.get(s.cp as usize)
    {
        out.push(Hole { x: cp.x, y: cp.y - TILE * 0.7, r: GLOW_SMALL, a: 0.75, p: None });
    }
    out
}

// ---- randomness (Math.random's stand-in)

thread_local! {
    static SEED: Cell<u64> = Cell::new(0x2545_f491_4f6c_dd1d ^ (std::process::id() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
}

pub fn rnd() -> f64 {
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    })
}

pub fn rand(a: f64, b: f64) -> f64 {
    a + rnd() * (b - a)
}

pub fn clamp(v: f64, a: f64, b: f64) -> f64 {
    if v < a {
        a
    } else if v > b {
        b
    } else {
        v
    }
}

/// frame-rate independent ease toward a target
pub fn approach(v: f64, target: f64, rate: f64, dt: f64) -> f64 {
    v + (target - v) * (1.0 - (-rate * dt).exp())
}

// ---- particles

pub const PK_DOT: u8 = 0;
pub const PK_SPARK: u8 = 1;
pub const PK_FLAKE: u8 = 2;
pub const PK_CONFETTI: u8 = 3;
pub const PK_RING: u8 = 4;
pub const PK_STAR: u8 = 5;
pub const PK_SHARD: u8 = 6;
pub const PK_STEAM: u8 = 7;
pub const PK_STREAK: u8 = 8;
pub const PK_CHEV: u8 = 9;
pub const PK_CHUNK: u8 = 10;

pub const MAX_PARTICLES: usize = 400;

/// a fixed pool of particles in parallel arrays (f32, as the TS's Float32Arrays)
pub struct Particles {
    cap: usize,
    pub n: usize,
    over: usize,
    x: Vec<f32>,
    y: Vec<f32>,
    vx: Vec<f32>,
    vy: Vec<f32>,
    life: Vec<f32>,
    max: Vec<f32>,
    size: Vec<f32>,
    grav: Vec<f32>,
    drag: Vec<f32>,
    rot: Vec<f32>,
    spin: Vec<f32>,
    kind: Vec<u8>,
    col: Vec<u8>,
    add: Vec<u8>,
}

impl Particles {
    pub fn new(cap: usize) -> Particles {
        let z = vec![0f32; cap];
        let b = vec![0u8; cap];
        Particles {
            cap,
            n: 0,
            over: 0,
            x: z.clone(),
            y: z.clone(),
            vx: z.clone(),
            vy: z.clone(),
            life: z.clone(),
            max: z.clone(),
            size: z.clone(),
            grav: z.clone(),
            drag: z.clone(),
            rot: z.clone(),
            spin: z,
            kind: b.clone(),
            col: b.clone(),
            add: b,
        }
    }

    pub fn clear(&mut self) {
        self.n = 0;
    }

    // 6.283 is the TS's own number, not TAU
    #[allow(clippy::too_many_arguments, clippy::approx_constant)]
    pub fn spawn(&mut self, kind: u8, col: usize, x: f64, y: f64, vx: f64, vy: f64, life: f64, size: f64, grav: f64, drag: f64, add: bool, spin: f64) {
        let mut i = self.n;
        if i >= self.cap {
            self.over = (self.over + 1) % self.cap;
            i = self.over;
        } else {
            self.n += 1;
        }
        self.x[i] = x as f32;
        self.y[i] = y as f32;
        self.vx[i] = vx as f32;
        self.vy[i] = vy as f32;
        self.life[i] = life as f32;
        self.max[i] = life as f32;
        self.size[i] = size as f32;
        self.grav[i] = grav as f32;
        self.drag[i] = drag as f32;
        self.rot[i] = if spin == 0.0 { 0.0 } else { ((x * 7.31 + y * 3.17) % 6.283) as f32 };
        self.spin[i] = spin as f32;
        self.kind[i] = kind;
        self.col[i] = col as u8;
        self.add[i] = u8::from(add);
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt as f32;
        let mut i = 0;
        while i < self.n {
            let l = self.life[i] - dt;
            if l <= 0.0 {
                self.n -= 1;
                if i < self.n {
                    self.copy(self.n, i);
                }
                continue;
            }
            self.life[i] = l;
            self.vy[i] += self.grav[i] * dt;
            let damp = 1.0 / (1.0 + self.drag[i] * dt);
            self.vx[i] *= damp;
            self.vy[i] *= damp;
            self.x[i] += self.vx[i] * dt;
            self.y[i] += self.vy[i] * dt;
            self.rot[i] += self.spin[i] * dt;
            i += 1;
        }
    }

    fn copy(&mut self, from: usize, to: usize) {
        self.x[to] = self.x[from];
        self.y[to] = self.y[from];
        self.vx[to] = self.vx[from];
        self.vy[to] = self.vy[from];
        self.life[to] = self.life[from];
        self.max[to] = self.max[from];
        self.size[to] = self.size[from];
        self.grav[to] = self.grav[from];
        self.drag[to] = self.drag[from];
        self.rot[to] = self.rot[from];
        self.spin[to] = self.spin[from];
        self.kind[to] = self.kind[from];
        self.col[to] = self.col[from];
        self.add[to] = self.add[from];
    }

    /// two passes, plain then additive; `only` draws one of them
    pub fn draw(&self, ctx: &mut Canvas, colors: &[Col], add_op: Composite, only: i32) {
        for pass in 0..2u8 {
            if only >= 0 && only != i32::from(pass) {
                continue;
            }
            ctx.set_composite(if pass == 0 { Composite::SourceOver } else { add_op });
            for i in 0..self.n {
                if self.add[i] == pass {
                    let c = usize::from(self.col[i]);
                    self.draw_one(ctx, i, colors.get(c).copied().unwrap_or(Col::WHITE));
                }
            }
        }
        ctx.set_composite(Composite::SourceOver);
        ctx.set_alpha(1.0);
    }

    fn draw_one(&self, ctx: &mut Canvas, i: usize, color: Col) {
        let t = f64::from(self.life[i] / self.max[i]);
        let x = f64::from(self.x[i]);
        let y = f64::from(self.y[i]);
        let s = f64::from(self.size[i]);
        let fade_in = ((1.0 - t) * 8.0).min(1.0);
        let pi = std::f64::consts::PI;
        match self.kind[i] {
            PK_DOT | PK_SPARK => {
                ctx.set_alpha((t * 2.0).min(1.0) * fade_in);
                ctx.fill_style(color);
                let r = s * (0.35 + 0.65 * t);
                ctx.fill_rect(x - r, y - r, r * 2.0, r * 2.0);
            }
            PK_FLAKE => {
                ctx.set_alpha((t * 2.5).min(1.0) * fade_in);
                ctx.stroke_style(color);
                ctx.set_line_width(0.7);
                ctx.begin_path();
                let a = f64::from(self.rot[i]);
                for k in 0..3 {
                    let c = (a + f64::from(k) * pi / 3.0).cos() * s;
                    let d = (a + f64::from(k) * pi / 3.0).sin() * s;
                    ctx.move_to(x - c, y - d);
                    ctx.line_to(x + c, y + d);
                }
                ctx.stroke();
            }
            PK_CONFETTI => {
                ctx.set_alpha((t * 3.0).min(1.0));
                ctx.fill_style(color);
                let a = f64::from(self.rot[i]);
                let w = s * (a * 1.7).cos().abs() + 0.4;
                ctx.save();
                ctx.translate(x, y);
                ctx.rotate(a);
                ctx.fill_rect(-w / 2.0, -s * 0.3, w, s * 0.6);
                ctx.restore();
            }
            PK_RING => {
                ctx.set_alpha(t * 0.9);
                ctx.stroke_style(color);
                ctx.set_line_width(0.6 + 1.6 * t);
                ctx.begin_path();
                ctx.arc(x, y, 1.0 + s * (1.0 - t * t), 0.0, pi * 2.0, false);
                ctx.stroke();
            }
            PK_STAR => {
                let k = (pi * t).sin();
                ctx.set_alpha(k);
                ctx.fill_style(color);
                let r = s * k;
                let w = r * 0.22;
                ctx.begin_path();
                ctx.move_to(x, y - r);
                ctx.line_to(x + w, y - w);
                ctx.line_to(x + r, y);
                ctx.line_to(x + w, y + w);
                ctx.line_to(x, y + r);
                ctx.line_to(x - w, y + w);
                ctx.line_to(x - r, y);
                ctx.line_to(x - w, y - w);
                ctx.close_path();
                ctx.fill();
            }
            PK_SHARD => {
                ctx.set_alpha((t * 2.0).min(1.0));
                ctx.fill_style(color);
                ctx.save();
                ctx.translate(x, y);
                ctx.rotate(f64::from(self.rot[i]));
                ctx.begin_path();
                ctx.move_to(0.0, -s);
                ctx.line_to(s * 0.45, s * 0.6);
                ctx.line_to(-s * 0.45, s * 0.4);
                ctx.close_path();
                ctx.fill();
                ctx.restore();
            }
            PK_STEAM => {
                ctx.set_alpha(0.4 * t * fade_in);
                ctx.fill_style(color);
                ctx.begin_path();
                ctx.arc(x, y, s * (1.5 - t), 0.0, pi * 2.0, false);
                ctx.fill();
            }
            PK_STREAK => {
                ctx.set_alpha(0.45 * (t * 3.0).min(1.0) * fade_in);
                ctx.fill_style(color);
                ctx.fill_rect(x - 0.4, y, 0.8, s);
            }
            PK_CHEV => {
                ctx.set_alpha((t * 2.5).min(1.0) * fade_in);
                ctx.stroke_style(color);
                ctx.set_line_width(1.1);
                let d = if self.vy[i] > 0.0 { -1.0 } else { 1.0 };
                ctx.begin_path();
                ctx.move_to(x - s, y + s * 0.5 * d);
                ctx.line_to(x, y - s * 0.5 * d);
                ctx.line_to(x + s, y + s * 0.5 * d);
                ctx.stroke();
            }
            PK_CHUNK => {
                ctx.set_alpha((t * 2.5).min(1.0));
                ctx.fill_style(color);
                ctx.save();
                ctx.translate(x, y);
                ctx.rotate(f64::from(self.rot[i]));
                ctx.fill_rect(-s / 2.0, -s / 2.0, s, s * 0.8);
                ctx.restore();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helpers_agree_with_the_page() {
        // fnv-1a over UTF-16
        assert_eq!(hash_str(""), 0x811c_9dc5);
        assert_eq!(channel_index("!a"), channel_index("a"));
        assert!((0.0..1.0).contains(&hash01(12345.0)));
        assert_eq!(css((10.4, 20.5, 30.6), 0.12345), Col::rgba(10.0, 21.0, 31.0, 0.123));
        assert_eq!(mip_for(1.0, -1), 0);
        assert_eq!(mip_for(0.26, -1), 4);
        let mut p = Particles::new(3);
        for _ in 0..5 {
            p.spawn(PK_DOT, 0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, false, 0.0);
        }
        assert_eq!(p.n, 3);
        p.update(2.0);
        assert_eq!(p.n, 0);
    }
}
