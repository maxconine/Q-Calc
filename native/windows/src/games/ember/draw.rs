//! emberDraw.ts on canvas.rs (the Mac's EmberDraw.swift): tiles, pools, mechanics, the two characters, particles,
//! glow and the dark. It draws ONE stage; world units throughout, one transform maps the camera's rect onto the
//! canvas, and the backdrop and walls come from chunks baked once per zoom. Line for line with the Swift; things.rs
//! and players.rs hold the rest of the renderer.

use std::collections::HashMap;
use std::f64::consts::PI;

use qcalc_core::games::ember::camera::{base_view, camera_at, ease_camera};
use qcalc_core::games::ember::engine::trace_beams;
use qcalc_core::games::ember::{
    Camera, DeathCause, Element, EventKind, GLOW_SMALL, GameEvent, GameState, PLAYER_H, PLAYER_W, Phase, STAGE_TICKS,
    Slant, Stage, T_CRUMBLE, T_DARK, T_LIGHT, T_ONEWAY, T_TRAMP, T_WALL, TILE,
};

use super::fx::*;
use crate::games::canvas::{Canvas, Cap, Col, Composite, Gradient, Offscreen, Res};

pub const TAU: f64 = PI * 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawOpts {
    pub me: Option<usize>,
    pub reduce_motion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Rgb,
    pub tape: Rgb,
    pub ink: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub fire: Rgb,
    pub fire2: Rgb,
    pub frost: Rgb,
    pub frost2: Rgb,
    pub star: Rgb,
    pub wall: Rgb,
    pub wall_hi: Rgb,
    pub wall_lo: Rgb,
}

pub const WHITE: Rgb = (255.0, 255.0, 255.0);
pub const BLACK: Rgb = (0.0, 0.0, 0.0);

/// readPalette: the theme's tokens (index.css, EmberPanel.css), light or dark
pub fn read_palette(dark: bool) -> Palette {
    let bg = if dark { (44.0, 44.0, 46.0) } else { (255.0, 255.0, 255.0) };
    let tape = if dark { (37.0, 37.0, 39.0) } else { (245.0, 245.0, 247.0) };
    let ink = if dark { (245.0, 245.0, 247.0) } else { (29.0, 29.0, 31.0) };
    let muted = if dark { (152.0, 152.0, 157.0) } else { (134.0, 134.0, 139.0) };
    let accent = if dark { (99.0, 211.0, 146.0) } else { (29.0, 122.0, 76.0) };
    let fire = if dark { (255.0, 138.0, 71.0) } else { (232.0, 100.0, 28.0) };
    let fire2 = if dark { (255.0, 193.0, 94.0) } else { (245.0, 165.0, 36.0) };
    let frost = if dark { (95.0, 205.0, 242.0) } else { (31.0, 147.0, 198.0) };
    let frost2 = if dark { (165.0, 232.0, 251.0) } else { (92.0, 200.0, 230.0) };
    let star = if dark { (255.0, 196.0, 77.0) } else { (240.0, 168.0, 26.0) };
    let is_dark = luma(bg) < 0.45;
    let stone = if is_dark { (92.0, 88.0, 108.0) } else { (178.0, 172.0, 190.0) };
    let wall = mix(mix(tape, ink, if is_dark { 0.2 } else { 0.3 }), stone, 0.4);
    let wall_hi = if is_dark { mix(wall, ink, 0.24) } else { mix(wall, WHITE, 0.6) };
    let wall_lo = if is_dark { mix(wall, BLACK, 0.38) } else { mix(wall, ink, 0.2) };
    Palette { dark: is_dark, bg, tape, ink, muted, accent, fire, fire2, frost, frost2, star, wall, wall_hi, wall_lo }
}

pub const LAVA_TOP: Rgb = (255.0, 196.0, 84.0);
pub const LAVA_MID: Rgb = (255.0, 106.0, 31.0);
pub const LAVA_BOT: Rgb = (190.0, 38.0, 26.0);
pub const WATER_TOP: Rgb = (104.0, 190.0, 250.0);
pub const WATER_BOT: Rgb = (28.0, 92.0, 190.0);
pub const GOO_TOP: Rgb = (170.0, 236.0, 80.0);
pub const GOO_BOT: Rgb = (58.0, 140.0, 34.0);
pub const THIN_TOP: Rgb = (176.0, 224.0, 248.0);
pub const THIN_BOT: Rgb = (110.0, 178.0, 226.0);
pub const ICE_TOP: Rgb = (236.0, 250.0, 255.0);
pub const ICE_BOT: Rgb = (168.0, 220.0, 244.0);
pub const WOOD: Rgb = (178.0, 118.0, 62.0);
pub const WOOD_DARK: Rgb = (116.0, 70.0, 34.0);
pub const METAL: Rgb = (128.0, 134.0, 150.0);
pub const TRAMP: Rgb = (238.0, 78.0, 104.0);
pub const SAND: Rgb = (204.0, 160.0, 104.0);
pub const SUN: Rgb = (255.0, 214.0, 120.0);
pub const MOON: Rgb = (128.0, 116.0, 226.0);
pub const GRAV: Rgb = (190.0, 132.0, 255.0);
pub const ARROW: Rgb = (72.0, 214.0, 150.0);
pub const GOLD: Rgb = (246.0, 196.0, 62.0);
pub const GOLD_DARK: Rgb = (150.0, 104.0, 22.0);
pub const NIGHT: Col = Col::rgb(7.0, 6.0, 14.0);
pub const PORTAL_COLORS: [Rgb; 4] = [(168.0, 112.0, 255.0), (255.0, 122.0, 214.0), (64.0, 222.0, 196.0), (255.0, 212.0, 92.0)];

pub const C_FIRE: usize = 0;
pub const C_FIRE2: usize = 1;
pub const C_FROST: usize = 2;
pub const C_FROST2: usize = 3;
pub const C_WHITE: usize = 4;
pub const C_STEAM: usize = 5;
pub const C_GOO: usize = 6;
pub const C_LAVA: usize = 7;
pub const C_WATER: usize = 8;
pub const C_DUST: usize = 9;
pub const C_STAR: usize = 10;
pub const C_CORE: usize = 11;
pub const C_ICE: usize = 12;
pub const C_PORTAL: usize = 13;
pub const C_CH: usize = C_PORTAL + PORTAL_COLORS.len();
pub const C_TRAMP: usize = C_CH + CHANNEL_HEX.len();
pub const C_SAND: usize = C_TRAMP + 1;
pub const C_SUN: usize = C_TRAMP + 2;
pub const C_GRAV: usize = C_TRAMP + 3;
pub const C_ARROW: usize = C_TRAMP + 4;
pub const C_GOLD: usize = C_TRAMP + 5;
pub const C_SMOKE: usize = C_TRAMP + 6;

pub fn color_table(p: &Palette) -> Vec<Col> {
    let mut t = vec![Col::WHITE; C_SMOKE + 1];
    t[C_FIRE] = css1(p.fire);
    t[C_FIRE2] = css1(p.fire2);
    t[C_FROST] = css1(p.frost);
    t[C_FROST2] = css1(p.frost2);
    t[C_WHITE] = if p.dark { Col::rgb(255.0, 255.0, 255.0) } else { css1(mix(p.frost2, WHITE, 0.3)) };
    t[C_STEAM] = if p.dark { Col::rgb(220.0, 228.0, 236.0) } else { Col::rgb(170.0, 180.0, 192.0) };
    t[C_GOO] = css1(GOO_TOP);
    t[C_LAVA] = css1(LAVA_MID);
    t[C_WATER] = css1(WATER_TOP);
    t[C_DUST] = css1(if p.dark { mix(p.wall, p.ink, 0.35) } else { mix(p.wall, p.ink, 0.15) });
    t[C_STAR] = css1(p.star);
    t[C_CORE] = if p.dark { Col::rgb(255.0, 246.0, 214.0) } else { css1(mix(p.star, LAVA_MID, 0.25)) };
    t[C_ICE] = css1(if p.dark { ICE_TOP } else { mix(ICE_BOT, p.frost, 0.35) });
    for (i, c) in PORTAL_COLORS.iter().enumerate() {
        t[C_PORTAL + i] = css1(*c);
    }
    for (i, c) in channel_colors().iter().enumerate() {
        t[C_CH + i] = *c;
    }
    t[C_TRAMP] = css1(TRAMP);
    t[C_SAND] = css1(if p.dark { SAND } else { mix(SAND, BLACK, 0.2) });
    t[C_SUN] = css1(if p.dark { SUN } else { mix(SUN, LAVA_MID, 0.35) });
    t[C_GRAV] = css1(if p.dark { GRAV } else { mix(GRAV, BLACK, 0.12) });
    t[C_ARROW] = css1(if p.dark { ARROW } else { mix(ARROW, BLACK, 0.18) });
    t[C_GOLD] = css1(GOLD);
    t[C_SMOKE] = if p.dark { Col::rgb(120.0, 120.0, 132.0) } else { Col::rgb(96.0, 96.0, 108.0) };
    t
}

// ---- shared shape helpers

/// a rect with its own radius per corner (tl, tr, br, bl)
#[allow(clippy::too_many_arguments)]
pub fn round_rect_path(g: &mut Canvas, x: f64, y: f64, w: f64, h: f64, tl: f64, tr: f64, br: f64, bl: f64) {
    g.begin_path();
    g.move_to(x + tl, y);
    g.line_to(x + w - tr, y);
    if tr != 0.0 {
        g.arc_to(x + w, y, x + w, y + tr, tr);
    } else {
        g.line_to(x + w, y);
    }
    g.line_to(x + w, y + h - br);
    if br != 0.0 {
        g.arc_to(x + w, y + h, x + w - br, y + h, br);
    } else {
        g.line_to(x + w, y + h);
    }
    g.line_to(x + bl, y + h);
    if bl != 0.0 {
        g.arc_to(x, y + h, x, y + h - bl, bl);
    } else {
        g.line_to(x, y + h);
    }
    g.line_to(x, y + tl);
    if tl != 0.0 {
        g.arc_to(x, y, x + tl, y, tl);
    } else {
        g.line_to(x, y);
    }
    g.close_path();
}

pub fn rr(g: &mut Canvas, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let k = r.min(w / 2.0).min(h / 2.0);
    round_rect_path(g, x, y, w, h, k, k, k, k);
}

pub fn flame_glyph(g: &mut Canvas, x: f64, y: f64, h: f64) {
    let w = h * 0.42;
    let b = y + h * 0.5;
    g.begin_path();
    g.move_to(x, b);
    g.bezier_curve_to(x - w * 1.1, b, x - w * 1.15, y - h * 0.02, x - w * 0.6, y - h * 0.24);
    g.quadratic_curve_to(x - w * 0.5, y - h * 0.02, x - w * 0.18, y - h * 0.06);
    g.bezier_curve_to(x - w * 0.35, y - h * 0.3, x - w * 0.05, y - h * 0.42, x + w * 0.2, y - h * 0.5);
    g.bezier_curve_to(x + w * 0.3, y - h * 0.25, x + w * 1.15, y - h * 0.12, x + w * 0.98, y + h * 0.2);
    g.bezier_curve_to(x + w * 0.88, b, x + w * 0.4, b, x, b);
    g.close_path();
}

pub fn flake_glyph(g: &mut Canvas, x: f64, y: f64, r: f64) {
    g.begin_path();
    for k in 0..6 {
        let a = f64::from(k) * PI / 3.0 - PI / 2.0;
        let (s, c) = a.sin_cos();
        g.move_to(x, y);
        g.line_to(x + c * r, y + s * r);
        let (bx, by, b) = (x + c * r * 0.6, y + s * r * 0.6, r * 0.3);
        g.move_to(bx, by);
        g.line_to(bx + (a + 0.8).cos() * b, by + (a + 0.8).sin() * b);
        g.move_to(bx, by);
        g.line_to(bx + (a - 0.8).cos() * b, by + (a - 0.8).sin() * b);
    }
}

pub fn gem_path(g: &mut Canvas, s: f64) {
    g.begin_path();
    g.move_to(-5.0 * s, -1.5 * s);
    g.line_to(-2.8 * s, -4.5 * s);
    g.line_to(2.8 * s, -4.5 * s);
    g.line_to(5.0 * s, -1.5 * s);
    g.line_to(0.0, 5.5 * s);
    g.close_path();
}

pub fn arch_path(g: &mut Canvas, x: f64, y: f64, w: f64, h: f64) {
    let r = w / 2.0;
    g.begin_path();
    g.move_to(x, y + h);
    g.line_to(x, y + r);
    g.arc(x + r, y + r, r, PI, 0.0, false);
    g.line_to(x + w, y + h);
    g.close_path();
}

pub fn is_wall(level: &Stage, tx: i64, ty: i64) -> bool {
    if tx < 0 || ty < 0 || tx >= level.cols as i64 || ty >= level.rows as i64 {
        return true;
    }
    level.tiles[ty as usize * level.cols + tx as usize] == T_WALL
}

pub fn closed_tile(level: &Stage, tx: i64, ty: i64) -> bool {
    if tx < 0 || ty < 0 || tx >= level.cols as i64 || ty >= level.rows as i64 {
        return true;
    }
    let t = level.tiles[ty as usize * level.cols + tx as usize];
    t != 0 && t != T_ONEWAY && t != T_CRUMBLE && t != T_LIGHT && t != T_DARK
}

pub fn key_path(g: &mut Canvas, s: f64) {
    g.begin_path();
    g.arc(-3.4 * s, 0.0, 2.5 * s, 0.0, TAU, false);
    g.move_to(-0.9 * s, 0.0);
    g.line_to(6.0 * s, 0.0);
    g.move_to(3.4 * s, 0.0);
    g.line_to(3.4 * s, 2.4 * s);
    g.move_to(5.6 * s, 0.0);
    g.line_to(5.6 * s, 2.0 * s);
}

pub fn chevrons(g: &mut Canvas, s: f64) {
    g.begin_path();
    g.move_to(-3.6 * s, 0.2 * s);
    g.line_to(0.0, -3.4 * s);
    g.line_to(3.6 * s, 0.2 * s);
    g.move_to(-3.6 * s, 3.8 * s);
    g.line_to(0.0, 0.2 * s);
    g.line_to(3.6 * s, 3.8 * s);
}

// ---- the static layer: backdrop + walls

pub fn paint_backdrop(g: &mut Canvas, level: &Stage, p: &Palette, range: Option<TileRange>) {
    let (w, h) = (level.w, level.h);
    let mut sky = Gradient::linear(0.0, 0.0, 0.0, h);
    sky.stop(0.0, css1(mix(p.bg, p.tape, if p.dark { 0.1 } else { 0.15 })));
    sky.stop(1.0, css1(if p.dark { mix(p.tape, BLACK, 0.12) } else { mix(p.tape, p.wall, 0.12) }));
    g.fill_style(sky);
    g.fill_rect(0.0, 0.0, w, h);
    let mut warm = Gradient::radial(w * 0.12, h * 1.05, 0.0, w * 0.12, h * 1.05, h * 0.95);
    warm.stop(0.0, css(p.fire, if p.dark { 0.1 } else { 0.07 }));
    warm.stop(1.0, css(p.fire, 0.0));
    g.fill_style(warm);
    g.fill_rect(0.0, 0.0, w, h);
    let mut cool = Gradient::radial(w * 0.88, h * 1.05, 0.0, w * 0.88, h * 1.05, h * 0.95);
    cool.stop(0.0, css(p.frost, if p.dark { 0.1 } else { 0.07 }));
    cool.stop(1.0, css(p.frost, 0.0));
    g.fill_style(cool);
    g.fill_rect(0.0, 0.0, w, h);
    let (bw, bh) = (TILE * 2.0, TILE);
    let r0 = range.map_or(0, |r| r.y0.max(0));
    let r1 = range.map_or(level.rows as i64, |r| r.y1.min(level.rows as i64));
    let bx0 = range.map_or(0.0, |r| (((r.x0 as f64 * TILE / bw).floor() as i64 - 1).max(0)) as f64 * bw);
    let bx1 = range.map_or(w, |r| w.min(r.x1 as f64 * TILE));
    for row in r0..r1.max(r0) {
        let off = if row % 2 != 0 { bw / 2.0 } else { 0.0 };
        let mut bx = bx0 - off;
        while bx < bx1 {
            let n = hash01((row * 97) as f64 + crate::games::canvas::js_round(bx) * 13.0 + 5.0);
            g.fill_style(css(p.ink, (if p.dark { 0.018 } else { 0.012 }) + n * if p.dark { 0.02 } else { 0.016 }));
            rr(g, bx + 1.0, row as f64 * bh + 1.0, bw - 2.0, bh - 2.0, 3.0);
            g.fill();
            bx += bw;
        }
    }
    let mut vg = Gradient::radial(w / 2.0, h / 2.0, h * 0.35, w / 2.0, h / 2.0, w * 0.65);
    vg.stop(0.0, Col::rgba(0.0, 0.0, 0.0, 0.0));
    vg.stop(1.0, if p.dark { Col::rgba(0.0, 0.0, 0.0, 0.28) } else { Col::rgba(40.0, 30.0, 60.0, 0.07) });
    g.fill_style(vg);
    g.fill_rect(0.0, 0.0, w, h);
}

pub fn paint_walls(g: &mut Canvas, level: &Stage, p: &Palette, detail: bool, range: Option<TileRange>) {
    let r_ = 5.0;
    let tx0 = range.map_or(0, |r| r.x0);
    let ty0 = range.map_or(0, |r| r.y0);
    let tx1 = range.map_or(level.cols as i64, |r| r.x1);
    let ty1 = range.map_or(level.rows as i64, |r| r.y1);
    if tx0 >= tx1 || ty0 >= ty1 {
        return;
    }
    g.fill_style(if p.dark { Col::rgba(0.0, 0.0, 0.0, 0.32) } else { Col::rgba(40.0, 30.0, 70.0, 0.10) });
    for ty in ty0..ty1 {
        for tx in tx0..tx1 {
            if !is_wall(level, tx, ty) || (closed_tile(level, tx, ty + 1) && closed_tile(level, tx + 1, ty)) {
                continue;
            }
            let (u, d) = (!closed_tile(level, tx, ty - 1), !closed_tile(level, tx, ty + 1));
            let (l, r) = (!closed_tile(level, tx - 1, ty), !closed_tile(level, tx + 1, ty));
            let k = |on: bool| if on { r_ } else { 0.0 };
            round_rect_path(g, tx as f64 * TILE + 1.0, ty as f64 * TILE + 2.5, TILE, TILE, k(u && l), k(u && r), k(d && r), k(d && l));
            g.fill();
        }
    }
    let (base, hi, lo) = (css1(p.wall), css1(p.wall_hi), css1(p.wall_lo));
    let mortar = css(p.wall_lo, if p.dark { 0.35 } else { 0.22 });
    let speck = css(if p.dark { p.wall_hi } else { p.wall_lo }, 0.25);
    for ty in ty0..ty1 {
        for tx in tx0..tx1 {
            if !is_wall(level, tx, ty) {
                continue;
            }
            let (x, y) = (tx as f64 * TILE, ty as f64 * TILE);
            let (u, d) = (!closed_tile(level, tx, ty - 1), !closed_tile(level, tx, ty + 1));
            let (l, r) = (!closed_tile(level, tx - 1, ty), !closed_tile(level, tx + 1, ty));
            let k = |on: bool| if on { r_ } else { 0.0 };
            g.save();
            round_rect_path(g, x, y, TILE, TILE, k(u && l), k(u && r), k(d && r), k(d && l));
            g.fill_style(base);
            g.fill();
            let rounded = (u && l) || (u && r) || (d && r) || (d && l);
            if rounded {
                g.clip();
            }
            if detail {
                g.fill_style(mortar);
                g.fill_rect(x, y + TILE / 2.0 - 0.5, TILE, 1.0);
                let j = if ty & 1 != 0 { 0.0 } else { TILE / 2.0 };
                g.fill_rect(x + j - 0.5, y, 1.0, TILE / 2.0);
                g.fill_rect(x + (j + TILE / 2.0) % TILE - 0.5, y + TILE / 2.0, 1.0, TILE / 2.0);
                g.fill_style(speck);
                for k in 0..3 {
                    let n = hash01((ty * level.cols as i64 * 7 + tx * 7 + k) as f64);
                    let m = hash01((ty * level.cols as i64 * 11 + tx * 5 + k + 999) as f64);
                    g.fill_rect(x + 2.0 + n * (TILE - 4.0), y + 2.0 + m * (TILE - 4.0), 1.0, 1.0);
                }
            }
            if u {
                g.fill_style(hi);
                g.fill_rect(x, y, TILE, 3.0);
                g.fill_style(css(p.wall_lo, 0.25));
                g.fill_rect(x, y + 3.0, TILE, 1.0);
            }
            if d {
                g.fill_style(lo);
                g.fill_rect(x, y + TILE - 2.5, TILE, 2.5);
            }
            if l {
                g.fill_style(css(if p.dark { p.wall_hi } else { p.wall_lo }, 0.35));
                g.fill_rect(x, y, 1.5, TILE);
            }
            if r {
                g.fill_style(css(p.wall_lo, 0.55));
                g.fill_rect(x + TILE - 1.5, y, 1.5, TILE);
            }
            g.restore();
        }
    }
}

// ---- per-player animation state

#[derive(Clone, Copy, Debug)]
pub struct PAnim {
    pub sq: f64,
    pub sqv: f64,
    pub run: f64,
    pub tilt: f64,
    pub look: f64,
    pub blink_at: f64,
    pub blink_until: f64,
    pub was_ground: bool,
    pub trail: f64,
    pub ambient: f64,
    pub x: f64,
    pub y: f64,
    pub flip: f64,
    pub aura: f64,
}

impl PAnim {
    pub fn new() -> PAnim {
        PAnim {
            sq: 0.0,
            sqv: 0.0,
            run: 0.0,
            tilt: 0.0,
            look: 1.0,
            blink_at: 1.0 + rnd() * 2.0,
            blink_until: 0.0,
            was_ground: true,
            trail: 0.0,
            ambient: 0.0,
            x: 0.0,
            y: 0.0,
            flip: 0.0,
            aura: 0.0,
        }
    }
}

/// one frame's input: the stage, the states, and what happened
pub struct DrawInput<'a> {
    pub level: &'a Stage,
    pub prev: Option<&'a GameState>,
    pub cur: &'a GameState,
    pub alpha: f64,
    pub events: &'a [GameEvent],
    pub now_ms: f64,
    pub opts: DrawOpts,
}

pub struct Renderer {
    pub pal: Palette,
    pub colors: Vec<Col>,
    pub parts: Particles,
    chunks: HashMap<String, Offscreen>,
    chunk_order: Vec<String>,
    stat_key: String,
    mip: i64,
    cam: Option<Camera>,
    pub view: View,
    pub vr: Camera,
    pub far: bool,
    pub pos: [(f64, f64); 2],
    pub beams: Vec<(f64, f64, f64, f64)>,
    mask: Option<Offscreen>,
    hole: Option<Offscreen>,
    pub dark_a: f64,
    pub glow_boost: f64,
    pub flash: f64,
    pub lamp_max: f64,
    iris_at: f64,
    iris_dur: f64,
    pub crumble_index: HashMap<usize, usize>,
    pub tramp_at: HashMap<usize, f64>,
    pub mend_at: HashMap<usize, f64>,
    pub cp_at: Vec<f64>,
    pub cp_up: Vec<f64>,
    pub lock_gone: Vec<f64>,
    pub cannon_kick: Vec<f64>,
    pub gsw_spin: Vec<f64>,
    pub lamp_on: Vec<f64>,
    pub key_holder: usize,
    pub key_trail: Vec<(f64, f64)>,
    pub vy_rel: f64,
    glow_cache: HashMap<(i64, i64, i64), Offscreen>,
    level_key: Option<String>,
    level_index: i64,
    pub lava_tops_list: Vec<usize>,
    pub thin_index: HashMap<usize, usize>,
    pub portal_col: Vec<usize>,
    last_ms: f64,
    pub shake: f64,
    win_at: f64,
    pub anims: [PAnim; 2],
    pub lever_ang: Vec<f64>,
    pub mirror_ang: Vec<f64>,
    pub fan_ang: Vec<f64>,
    pub fan_spin: Vec<f64>,
    pub ice_last: Vec<f64>,
    pub door_open: [f64; 2],
    pub text_w: HashMap<String, f64>,
    pub px: [f64; 2],
    pub py: [f64; 2],
    /// the canvas's target generation the cached offscreens were made for
    generation: u32,
}

impl Renderer {
    pub fn new(dark: bool) -> Renderer {
        let pal = read_palette(dark);
        Renderer {
            colors: color_table(&pal),
            pal,
            parts: Particles::new(MAX_PARTICLES),
            chunks: HashMap::new(),
            chunk_order: Vec::new(),
            stat_key: String::new(),
            mip: -1,
            cam: None,
            view: View { k: 1.0, ox: 0.0, oy: 0.0 },
            vr: Camera { x: 0.0, y: 0.0, w: 640.0, h: 360.0, zoom: 1.0 },
            far: false,
            pos: [(0.0, 0.0); 2],
            beams: Vec::new(),
            mask: None,
            hole: None,
            dark_a: 0.0,
            glow_boost: 0.0,
            flash: 0.0,
            lamp_max: 0.0,
            iris_at: -9.0,
            iris_dur: 0.5,
            crumble_index: HashMap::new(),
            tramp_at: HashMap::new(),
            mend_at: HashMap::new(),
            cp_at: Vec::new(),
            cp_up: Vec::new(),
            lock_gone: Vec::new(),
            cannon_kick: Vec::new(),
            gsw_spin: Vec::new(),
            lamp_on: Vec::new(),
            key_holder: 0,
            key_trail: Vec::new(),
            vy_rel: 0.0,
            glow_cache: HashMap::new(),
            level_key: None,
            level_index: -1,
            lava_tops_list: Vec::new(),
            thin_index: HashMap::new(),
            portal_col: Vec::new(),
            last_ms: 0.0,
            shake: 0.0,
            win_at: -1.0,
            anims: [PAnim::new(), PAnim::new()],
            lever_ang: Vec::new(),
            mirror_ang: Vec::new(),
            fan_ang: Vec::new(),
            fan_spin: Vec::new(),
            ice_last: Vec::new(),
            door_open: [0.0, 0.0],
            text_w: HashMap::new(),
            px: [0.0; 2],
            py: [0.0; 2],
            generation: u32::MAX,
        }
    }

    /// forget particles and effects (new level)
    pub fn reset(&mut self) {
        self.parts.clear();
        self.shake = 0.0;
        self.win_at = -1.0;
        self.level_key = None;
        self.last_ms = 0.0;
        self.cam = None;
        self.iris_at = -9.0;
        self.flash = 0.0;
        self.glow_boost = 0.0;
    }

    /// the theme changed
    pub fn refresh_colors(&mut self, dark: bool) {
        if dark == self.pal.dark {
            return;
        }
        self.pal = read_palette(dark);
        self.colors = color_table(&self.pal);
        self.glow_cache.clear();
        self.stat_key.clear();
        self.hole = None;
    }

    fn set_level(&mut self, level: &Stage, key: String) {
        self.level_key = Some(key);
        self.level_index = level.index as i64;
        self.stat_key.clear();
        self.chunks.clear();
        self.chunk_order.clear();
        self.cam = None;
        self.lava_tops_list = lava_tops(level);
        self.thin_index = level.thin.iter().enumerate().map(|(j, &ti)| (ti, j)).collect();
        self.crumble_index = level.crumble.iter().enumerate().map(|(j, &ti)| (ti, j)).collect();
        self.tramp_at.clear();
        self.mend_at.clear();
        self.cp_at = vec![-9.0; level.checkpoints.len()];
        self.cp_up = vec![0.0; level.checkpoints.len()];
        self.lock_gone = vec![0.0; level.locks.len()];
        self.cannon_kick = vec![0.0; level.cannons.len()];
        self.gsw_spin = vec![0.0; level.gswitches.len()];
        self.lamp_on = vec![0.0; level.lamps.len()];
        self.lamp_max = level.lamps.iter().fold(0.0, |m, l| m.max(l.ticks as f64));
        self.key_trail.clear();
        self.key_holder = 0;
        self.flash = 0.0;
        self.glow_boost = 0.0;
        let mut pairs: Vec<&str> = Vec::new();
        self.portal_col = level
            .portals
            .iter()
            .map(|p| {
                let k = match pairs.iter().position(|q| *q == p.pair) {
                    Some(k) => k,
                    None => {
                        pairs.push(&p.pair);
                        pairs.len() - 1
                    }
                };
                k % PORTAL_COLORS.len()
            })
            .collect();
        self.lever_ang = level.levers.iter().map(|l| if l.on { 1.0 } else { -1.0 }).collect();
        self.mirror_ang = level.mirrors.iter().map(|m| if m.slant == Slant::Slash { -PI / 4.0 } else { PI / 4.0 }).collect();
        self.fan_ang = vec![0.0; level.fans.len()];
        self.fan_spin = vec![0.0; level.fans.len()];
        self.ice_last = vec![0.0; level.ice.len()];
        self.door_open = [0.0, 0.0];
        self.anims = [PAnim::new(), PAnim::new()];
        self.parts.clear();
    }

    // ---- glow sprites

    fn glow_sprite(&mut self, g: &Canvas, res: &Res, color: Col) -> Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap> {
        let id = (color.r as i64, color.g as i64, color.b as i64);
        if let std::collections::hash_map::Entry::Vacant(slot) = self.glow_cache.entry(id) {
            let mut s = g.offscreen(64.0, 64.0)?;
            let c = rgb_of(color);
            s.draw(res, |s| {
                let mut grad = Gradient::radial(32.0, 32.0, 0.0, 32.0, 32.0, 32.0);
                grad.stop(0.0, css(c, 0.9));
                grad.stop(0.25, css(c, 0.45));
                grad.stop(0.6, css(c, 0.12));
                grad.stop(1.0, css(c, 0.0));
                s.fill_style(grad);
                s.fill_rect(0.0, 0.0, 64.0, 64.0);
            });
            slot.insert(s);
        }
        self.glow_cache.get_mut(&id)?.image()
    }

    pub fn glow(&mut self, g: &mut Canvas, x: f64, y: f64, r: f64, color: Col, a: f64) {
        if a <= 0.003 {
            return;
        }
        let res = g.res();
        let img = self.glow_sprite(g, res, color);
        g.set_alpha((a * if self.pal.dark { 1.0 } else { 0.5 }).min(1.0));
        g.draw_image(img.as_ref(), x - r, y - r, r * 2.0, r * 2.0);
    }

    pub fn begin_glow(&self, g: &mut Canvas) {
        g.set_composite(if self.pal.dark { Composite::Lighter } else { Composite::SourceOver });
    }

    pub fn end_glow(&self, g: &mut Canvas) {
        g.set_composite(Composite::SourceOver);
        g.set_alpha(1.0);
    }

    pub fn vis(&self, x: f64, y: f64, w: f64, h: f64, pad: f64) -> bool {
        in_view(&self.vr, x, y, w, h, pad)
    }

    // ---- static layer

    fn bake_chunk(&self, g: &Canvas, res: &Res, level: &Stage, kb: f64, ix: i64, iy: i64) -> Option<Offscreen> {
        let px = CHUNK_PX as f64;
        let mut c = g.offscreen(px, px)?;
        let p = self.pal;
        c.draw(res, |c| {
            c.fill_style(css1(p.wall));
            c.fill_rect(0.0, 0.0, px, px);
            c.set_transform(kb, 0.0, 0.0, kb, -(ix as f64) * px, -(iy as f64) * px);
            let w = px / kb;
            let range = TileRange {
                x0: (ix as f64 * w / TILE).floor() as i64 - 1,
                y0: (iy as f64 * w / TILE).floor() as i64 - 1,
                x1: ((ix + 1) as f64 * w / TILE).ceil() as i64 + 1,
                y1: ((iy + 1) as f64 * w / TILE).ceil() as i64 + 1,
            };
            if range.x1 > 0 && range.y1 > 0 && range.x0 < level.cols as i64 && range.y0 < level.rows as i64 {
                c.save();
                c.begin_path();
                c.rect(0.0, 0.0, level.w, level.h);
                c.clip();
                paint_backdrop(c, level, &p, Some(range));
                c.restore();
            }
            paint_walls(c, level, &p, kb >= 1.2, Some(range));
        });
        Some(c)
    }

    fn draw_static(&mut self, g: &mut Canvas, level: &Stage, w: f64, h: f64) {
        let key = format!("{}|{w}x{h}|{}", if self.pal.dark { "dark" } else { "light" }, g.generation);
        if key != self.stat_key {
            self.chunks.clear();
            self.chunk_order.clear();
            self.stat_key = key;
            self.mip = -1;
        }
        let v = self.view;
        let bv = base_view(w / h);
        let k1 = w / bv.0;
        self.mip = mip_for(v.k / k1, self.mip) as i64;
        let kb = k1 * MIPS[self.mip as usize];
        let s = v.k / kb;
        let r = chunk_range(&self.vr, kb);
        let cap = (2.4 * w * h / (CHUNK_PX * CHUNK_PX) as f64).ceil() as usize + 8;
        let pp = CHUNK_PX as f64;
        let res = g.res();
        for iy in r.y0..r.y1.max(r.y0) {
            for ix in r.x0..r.x1.max(r.x0) {
                let id = format!("{}|{ix}|{iy}", self.mip);
                if self.chunks.contains_key(&id) {
                    self.chunk_order.retain(|x| *x != id);
                } else if let Some(c) = self.bake_chunk(g, res, level, kb, ix, iy) {
                    self.chunks.insert(id.clone(), c);
                }
                self.chunk_order.push(id.clone());
                let img = self.chunks.get_mut(&id).and_then(Offscreen::image);
                let x0 = crate::games::canvas::js_round(ix as f64 * pp * s) + v.ox;
                let y0 = crate::games::canvas::js_round(iy as f64 * pp * s) + v.oy;
                let x1 = crate::games::canvas::js_round((ix + 1) as f64 * pp * s) + v.ox;
                let y1 = crate::games::canvas::js_round((iy + 1) as f64 * pp * s) + v.oy;
                g.draw_image(img.as_ref(), x0, y0, x1 - x0, y1 - y0);
            }
        }
        while self.chunks.len() > cap && !self.chunk_order.is_empty() {
            let first = self.chunk_order.remove(0);
            self.chunks.remove(&first);
        }
    }

    // ---- particles helpers

    #[allow(clippy::too_many_arguments)]
    pub fn burst(&mut self, kind: u8, col: usize, x: f64, y: f64, n: i64, speed: f64, life: f64, size: f64, grav: f64, drag: f64, add: bool, spin: f64, up: f64) {
        for _ in 0..n.max(0) {
            let a = rnd() * TAU;
            let v = speed * (0.35 + rnd() * 0.65);
            let sp = if spin != 0.0 { rand(-spin, spin) } else { 0.0 };
            self.parts.spawn(kind, col, x, y, a.cos() * v, a.sin() * v - up, life * (0.6 + rnd() * 0.6), size * (0.7 + rnd() * 0.6), grav, drag, add, sp);
        }
    }

    /// burst with the TS's defaults after `size`: no gravity, drag 2, plain
    #[allow(clippy::too_many_arguments)]
    pub fn burst_add(&mut self, kind: u8, col: usize, x: f64, y: f64, n: i64, speed: f64, life: f64, size: f64, grav: f64, drag: f64) {
        self.burst(kind, col, x, y, n, speed, life, size, grav, drag, true, 0.0, 0.0);
    }

    pub fn ring(&mut self, col: usize, x: f64, y: f64, life: f64, size: f64) {
        self.burst(PK_RING, col, x, y, 1, 0.0, life, size, 0.0, 0.0, true, 0.0, 0.0);
    }

    pub fn ch_col(&self, name: Option<&str>) -> usize {
        name.map_or(C_WHITE, |n| C_CH + channel_index(n))
    }

    fn on_event(&mut self, level: &Stage, cur: &GameState, e: &GameEvent, opts: DrawOpts, t: f64) {
        let less = if opts.reduce_motion { 0.4 } else { 1.0 };
        let n = |k: f64| -> i64 { 1.max(crate::games::canvas::js_round(k * less) as i64) };
        let (pw, ph) = (PLAYER_W, PLAYER_H);
        let pi = e.p.unwrap_or(0);
        let pl = cur.players[pi.min(1)];
        let gv = if pl.grav == -1 { -1.0 } else { 1.0 };
        let fx = pl.x + pw / 2.0;
        let fy = if gv > 0.0 { pl.y + ph } else { pl.y };
        let i = e.i.unwrap_or(0).max(0) as usize;
        let (ex, ey) = (e.px(), e.py());
        let fire_c = |p: usize| if p == 0 { C_FIRE } else { C_FROST };
        let fire2_c = |p: usize| if p == 0 { C_FIRE2 } else { C_FROST2 };
        match e.k {
            EventKind::Jump => {
                for _ in 0..n(5.0) {
                    self.parts.spawn(PK_DOT, C_DUST, fx + rand(-5.0, 5.0), fy - gv, rand(-30.0, 30.0), gv * rand(-25.0, -5.0), rand(0.25, 0.4), rand(1.2, 2.0), gv * 40.0, 4.0, false, 0.0);
                }
                if pi == 0 {
                    self.burst(PK_SPARK, C_FIRE2, fx, fy - gv * 4.0, n(4.0), 50.0, 0.35, 0.9, -60.0, 3.0, true, 0.0, 0.0);
                } else {
                    self.burst(PK_FLAKE, C_FROST2, fx, fy - gv * 4.0, n(3.0), 40.0, 0.5, 2.0, 20.0, 3.0, true, 4.0, 0.0);
                }
            }
            EventKind::Land => {
                for k in 0..n(7.0) {
                    let s = if k % 2 != 0 { 1.0 } else { -1.0 };
                    self.parts.spawn(PK_DOT, C_DUST, fx + s * rand(2.0, 6.0), fy - gv, s * rand(25.0, 60.0), gv * rand(-18.0, -4.0), rand(0.25, 0.45), rand(1.2, 2.2), gv * 60.0, 5.0, false, 0.0);
                }
            }
            EventKind::Airjump => {
                self.ring(C_ARROW, fx, fy, 0.4, 14.0);
                for _ in 0..n(5.0) {
                    self.parts.spawn(PK_CHEV, C_ARROW, fx + rand(-7.0, 7.0), fy + gv * rand(0.0, 4.0), rand(-12.0, 12.0), gv * rand(50.0, 110.0), rand(0.3, 0.5), rand(2.0, 3.0), 0.0, 2.5, true, 0.0);
                }
                self.burst_add(PK_STAR, C_WHITE, fx, fy, n(4.0), 60.0, 0.4, 2.6, 0.0, 3.0);
            }
            EventKind::Bounce => {
                let ti = tramp_near(level, ex, ey);
                if ti >= 0 {
                    self.tramp_at.insert(ti as usize, t);
                }
                let cx = pl.x + pw / 2.0;
                let dir = if pl.vy > 0.0 { 1.0 } else { -1.0 };
                let by = if dir < 0.0 { pl.y + ph } else { pl.y };
                let strong = clamp((e.v.unwrap_or(0.0) - 420.0) / 280.0, 0.2, 1.0);
                self.ring(C_TRAMP, cx, by, 0.35, 10.0 + strong * 10.0);
                for k in 0..n(4.0 + strong * 8.0) {
                    self.parts.spawn(
                        PK_STREAK,
                        if k % 3 != 0 { C_WHITE } else { C_TRAMP },
                        cx + rand(-6.0, 6.0),
                        by + dir * rand(0.0, 26.0) - if dir < 0.0 { 12.0 } else { 0.0 },
                        0.0,
                        dir * rand(60.0, 160.0) * strong,
                        rand(0.25, 0.45),
                        rand(8.0, 16.0),
                        0.0,
                        1.5,
                        true,
                        0.0,
                    );
                }
                for _ in 0..n(6.0) {
                    self.parts.spawn(PK_DOT, C_TRAMP, cx + rand(-8.0, 8.0), by, rand(-70.0, 70.0), dir * rand(20.0, 90.0), rand(0.25, 0.45), rand(1.0, 1.8), -dir * 200.0, 2.0, false, 0.0);
                }
            }
            EventKind::Arrow => {
                let a = level.arrows.get(i).map_or((ex, ey), |a| (a.x, a.y));
                self.ring(C_ARROW, a.0, a.1, 0.45, 18.0);
                self.burst_add(PK_STAR, C_ARROW, a.0, a.1, n(8.0), 100.0, 0.5, 3.2, 0.0, 3.0);
                for k in 0..n(6.0) {
                    self.parts.spawn(PK_CHEV, if k % 2 != 0 { C_WHITE } else { C_ARROW }, a.0 + rand(-8.0, 8.0), a.1 + rand(-4.0, 6.0), rand(-20.0, 20.0), -rand(60.0, 130.0), rand(0.35, 0.6), rand(2.0, 3.2), 0.0, 2.0, true, 0.0);
                }
            }
            EventKind::Arrowback => {
                let a = level.arrows.get(i).map_or((ex, ey), |a| (a.x, a.y));
                self.ring(C_ARROW, a.0, a.1, 0.35, 11.0);
                self.burst_add(PK_STAR, C_WHITE, a.0, a.1, n(3.0), 30.0, 0.4, 2.4, 0.0, 3.0);
            }
            EventKind::Key => {
                self.key_holder = pi;
                let a = level.keys.get(i).map_or((ex, ey), |a| (a.x, a.y));
                self.ring(C_GOLD, a.0, a.1, 0.45, 16.0);
                self.burst_add(PK_STAR, C_GOLD, a.0, a.1, n(9.0), 100.0, 0.6, 3.2, 0.0, 3.0);
            }
            EventKind::Unlock => {
                let r = level.locks.get(i);
                let (cx, cy) = r.map_or((ex, ey), |r| (r.x + r.w / 2.0, r.y + r.h / 2.0));
                let (hw, hh) = r.map_or((TILE / 2.0, TILE / 2.0), |r| (r.w / 2.0, r.h / 2.0));
                let area = hw * hh * 4.0 / (TILE * TILE);
                for k in 0..n((6.0 + area * 4.0).min(26.0)) {
                    self.parts.spawn(PK_CHUNK, if k % 3 != 0 { C_SMOKE } else { C_GOLD }, cx + rand(-hw, hw), cy + rand(-hh, hh), rand(-90.0, 90.0), rand(-150.0, -20.0), rand(0.5, 0.9), rand(2.0, 3.6), 520.0, 1.0, false, 10.0);
                }
                self.ring(C_GOLD, cx, cy, 0.5, 14.0 + hw.max(hh));
                self.burst_add(PK_STAR, C_GOLD, cx, cy, n(8.0), 90.0, 0.6, 3.2, 0.0, 3.0);
            }
            EventKind::Load => {
                let c = level.cannons.get(i).map_or((ex, ey), |c| (c.x, c.y));
                self.ring(fire_c(pi), c.0, c.1, 0.3, 13.0);
                self.burst(PK_DOT, C_SMOKE, c.0, c.1, n(5.0), 40.0, 0.35, 1.6, 0.0, 4.0, false, 0.0, 0.0);
            }
            EventKind::Fire => {
                let c = level.cannons.get(i);
                if let Some(k) = self.cannon_kick.get_mut(i) {
                    *k = 1.0;
                }
                if !opts.reduce_motion {
                    self.shake = self.shake.max(0.55);
                }
                let mut len = c.map_or(1.0, |c| c.vx.hypot(c.vy));
                if len == 0.0 {
                    len = 1.0;
                }
                let ux = c.map_or(0.0, |c| c.vx / len);
                let uy = c.map_or(-1.0, |c| c.vy / len);
                let mx = c.map_or(ex, |c| c.x) + ux * 13.0;
                let my = c.map_or(ey, |c| c.y) + uy * 13.0;
                for k in 0..n(14.0) {
                    let (sp, side) = (rand(120.0, 320.0), rand(-0.5, 0.5));
                    self.parts.spawn(
                        PK_SPARK,
                        if k % 2 != 0 { C_CORE } else { fire2_c(pi) },
                        mx,
                        my,
                        (ux - uy * side) * sp,
                        (uy + ux * side) * sp,
                        rand(0.25, 0.5),
                        rand(0.8, 1.4),
                        0.0,
                        3.0,
                        true,
                        0.0,
                    );
                }
                for _ in 0..n(9.0) {
                    self.parts.spawn(PK_STEAM, C_SMOKE, mx + rand(-3.0, 3.0), my + rand(-3.0, 3.0), ux * rand(20.0, 80.0) + rand(-20.0, 20.0), uy * rand(20.0, 80.0) + rand(-20.0, 20.0), rand(0.5, 0.9), rand(3.0, 6.0), -14.0, 2.0, false, 0.0);
                }
                self.ring(C_WHITE, mx, my, 0.3, 16.0);
            }
            EventKind::Flip => {
                let (cx, cy) = (pl.x + pw / 2.0, pl.y + ph / 2.0);
                self.ring(C_GRAV, cx, cy, 0.5, 22.0);
                self.ring(C_WHITE, cx, cy, 0.3, 12.0);
                let g = e.grav.unwrap_or(1) as f64;
                for k in 0..n(12.0) {
                    self.parts.spawn(PK_CHEV, if k % 3 != 0 { C_GRAV } else { C_WHITE }, cx + rand(-11.0, 11.0), cy + rand(-12.0, 12.0), 0.0, g * rand(70.0, 170.0), rand(0.4, 0.7), rand(2.2, 3.4), 0.0, 1.5, true, 0.0);
                }
            }
            EventKind::Gswitch => {
                if let Some(s) = self.gsw_spin.get_mut(i) {
                    *s = 1.0;
                }
                self.ring(C_GRAV, ex, ey, 0.4, 15.0);
                self.burst_add(PK_SPARK, C_GRAV, ex, ey, n(8.0), 90.0, 0.4, 1.0, 0.0, 2.0);
            }
            EventKind::Checkpoint => {
                let c = level.checkpoints.get(i);
                if let Some(a) = self.cp_at.get_mut(i) {
                    *a = t;
                }
                let (cx, cy) = c.map_or((ex, ey), |c| (c.x, c.y - 20.0));
                self.ring(C_STAR, cx, cy, 0.6, 26.0);
                self.burst_add(PK_STAR, C_STAR, cx, cy, n(10.0), 110.0, 0.7, 3.4, 0.0, 2.5);
                let cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR];
                for k in 0..n(16.0) {
                    self.parts.spawn(PK_CONFETTI, cols[k as usize % cols.len()], cx + rand(-4.0, 4.0), cy, rand(-80.0, 80.0), rand(-220.0, -110.0), rand(0.9, 1.4), rand(2.4, 3.6), 380.0, 1.6, false, 9.0);
                }
            }
            EventKind::Light => {
                if e.on == Some(true) {
                    if !opts.reduce_motion {
                        self.flash = 1.0;
                    }
                    self.glow_boost = 0.0;
                    self.ring(C_SUN, ex, ey, 0.5, 70.0);
                    self.ring(C_WHITE, ex, ey, 0.35, 34.0);
                    self.burst_add(PK_STAR, C_SUN, ex, ey, n(8.0), 120.0, 0.5, 3.0, 0.0, 3.0);
                } else {
                    self.glow_boost = if opts.reduce_motion { 0.0 } else { 1.0 };
                    self.flash = 0.0;
                    self.ring(C_GRAV, ex, ey, 0.4, 26.0);
                    self.burst_add(PK_SPARK, C_SUN, ex, ey, n(6.0), 60.0, 0.5, 0.9, 160.0, 2.0);
                }
            }
            EventKind::Shake => {
                for _ in 0..n(4.0) {
                    self.parts.spawn(PK_DOT, C_SAND, ex + rand(-9.0, 9.0), ey + rand(-2.0, 9.0), rand(-14.0, 14.0), rand(5.0, 30.0), rand(0.3, 0.5), rand(1.0, 1.7), 260.0, 1.0, false, 0.0);
                }
            }
            EventKind::Break => {
                for _ in 0..n(7.0) {
                    self.parts.spawn(PK_CHUNK, C_SAND, ex + rand(-8.0, 8.0), ey + rand(-8.0, 8.0), rand(-50.0, 50.0), rand(-60.0, 30.0), rand(0.55, 0.95), rand(2.6, 5.0), 620.0, 0.6, false, 9.0);
                }
                for _ in 0..n(5.0) {
                    self.parts.spawn(PK_STEAM, C_DUST, ex + rand(-8.0, 8.0), ey + rand(-4.0, 6.0), rand(-20.0, 20.0), rand(-16.0, 6.0), rand(0.4, 0.7), rand(2.5, 4.5), 0.0, 2.0, false, 0.0);
                }
            }
            EventKind::Mend => {
                if let Some(tile) = e.tile {
                    self.mend_at.insert(tile.max(0) as usize, t);
                }
                self.burst_add(PK_STAR, C_SAND, ex, ey, n(3.0), 30.0, 0.4, 2.6, 0.0, 3.0);
            }
            EventKind::Clear => {
                for d in level.doors {
                    self.burst_add(PK_STAR, C_STAR, d.x + d.w / 2.0, d.y + d.h / 2.0, n(8.0), 110.0, 0.7, 3.2, 0.0, 2.0);
                    self.ring(C_STAR, d.x + d.w / 2.0, d.y + d.h / 2.0, 0.6, 28.0);
                }
            }
            EventKind::Respawn => {
                self.iris_at = t;
                self.iris_dur = 0.38;
                self.key_trail.clear();
                for k in 0..2 {
                    let q = cur.players[k];
                    let (cx, cy) = (q.x + pw / 2.0, q.y + ph / 2.0);
                    self.ring(fire_c(k), cx, cy, 0.5, 20.0);
                    self.burst_add(PK_STAR, fire2_c(k), cx, cy, n(6.0), 70.0, 0.5, 2.8, 0.0, 3.0);
                }
                if let Some(cp) = e.cp
                    && cp >= 0
                    && let Some(a) = self.cp_at.get_mut(cp as usize)
                {
                    *a = t;
                }
            }
            EventKind::Die => {
                let (cx, cy) = (pl.x + pw / 2.0, pl.y + ph / 2.0);
                if !opts.reduce_motion {
                    self.shake = 1.0;
                }
                match e.cause {
                    Some(DeathCause::Water) => {
                        for _ in 0..n(16.0) {
                            self.parts.spawn(PK_STEAM, C_STEAM, cx + rand(-6.0, 6.0), cy + rand(-4.0, 6.0), rand(-20.0, 20.0), rand(-60.0, -25.0), rand(0.6, 1.1), rand(3.0, 6.0), -10.0, 1.5, false, 0.0);
                        }
                        self.burst_add(PK_SPARK, C_FIRE2, cx, cy, n(12.0), 90.0, 0.5, 1.0, 120.0, 2.0);
                        self.burst(PK_RING, C_STEAM, cx, cy + 4.0, 1, 0.0, 0.5, 14.0, 0.0, 2.0, false, 0.0, 0.0);
                    }
                    Some(DeathCause::Lava) => {
                        for k in 0..n(18.0) {
                            self.parts.spawn(PK_DOT, if k % 3 != 0 { C_FROST } else { C_FROST2 }, cx + rand(-6.0, 6.0), cy + rand(-8.0, 4.0), rand(-70.0, 70.0), rand(-140.0, -40.0), rand(0.5, 0.9), rand(1.2, 2.2), 520.0, 1.0, false, 0.0);
                        }
                        self.burst(PK_SHARD, C_ICE, cx, cy, n(6.0), 90.0, 0.6, 3.0, 300.0, 1.0, false, 12.0, 50.0);
                        for _ in 0..n(8.0) {
                            self.parts.spawn(PK_STEAM, C_STEAM, cx + rand(-5.0, 5.0), cy, rand(-15.0, 15.0), rand(-50.0, -20.0), rand(0.5, 0.9), rand(3.0, 5.0), -10.0, 1.5, false, 0.0);
                        }
                    }
                    Some(DeathCause::Crush) => {
                        let (c1, c2) = (fire_c(pi), fire2_c(pi));
                        for k in 0..n(22.0) {
                            let s = if k % 2 != 0 { 1.0 } else { -1.0 };
                            self.parts.spawn(
                                if k % 3 != 0 { PK_DOT } else { PK_CHUNK },
                                if k % 2 != 0 { c1 } else { c2 },
                                cx + s * rand(0.0, 5.0),
                                cy + rand(-3.0, 3.0),
                                s * rand(80.0, 260.0),
                                rand(-30.0, 30.0),
                                rand(0.35, 0.7),
                                rand(1.4, 2.8),
                                300.0,
                                2.5,
                                false,
                                10.0,
                            );
                        }
                        for _ in 0..n(8.0) {
                            self.parts.spawn(PK_STEAM, C_DUST, cx + rand(-10.0, 10.0), cy + rand(-10.0, 10.0), rand(-30.0, 30.0), rand(-20.0, 10.0), rand(0.4, 0.7), rand(3.0, 5.0), 0.0, 2.0, false, 0.0);
                        }
                        self.ring(C_WHITE, cx, cy, 0.3, 18.0);
                        self.burst_add(PK_STAR, C_WHITE, cx, cy, n(4.0), 60.0, 0.35, 3.0, 0.0, 3.0);
                    }
                    _ => {
                        for k in 0..n(20.0) {
                            self.parts.spawn(PK_DOT, if k % 4 != 0 { C_GOO } else { fire_c(pi) }, cx + rand(-5.0, 5.0), cy + 4.0, rand(-80.0, 80.0), rand(-170.0, -60.0), rand(0.5, 0.9), rand(1.3, 2.5), 560.0, 0.8, false, 0.0);
                        }
                        self.burst(PK_RING, C_GOO, cx, cy + 6.0, 1, 0.0, 0.45, 16.0, 0.0, 2.0, false, 0.0, 0.0);
                    }
                }
            }
            EventKind::Door => {
                if let Some(d) = level.doors.get(pi).copied() {
                    self.ring(fire_c(pi), d.x + d.w / 2.0, d.y + d.h / 2.0, 0.6, 22.0);
                    self.burst_add(PK_STAR, fire2_c(pi), d.x + d.w / 2.0, d.y + d.h / 2.0, n(6.0), 50.0, 0.6, 3.0, 0.0, 2.0);
                }
            }
            EventKind::Portal => {
                let col = self.portal_near(level, ex, ey);
                self.ring(col, ex, ey, 0.45, 16.0);
                self.ring(col, pl.x + pw / 2.0, pl.y + ph / 2.0, 0.5, 18.0);
                self.burst_add(PK_STAR, col, pl.x + pw / 2.0, pl.y + ph / 2.0, n(8.0), 70.0, 0.5, 2.5, 0.0, 3.0);
            }
            EventKind::Push => {
                if rnd() < 0.5 {
                    self.parts.spawn(PK_DOT, C_DUST, ex + rand(-4.0, 4.0), ey, rand(-10.0, 10.0), rand(-15.0, -5.0), 0.35, 1.4, 30.0, 4.0, false, 0.0);
                }
            }
            EventKind::Gem => {
                let fire = e.el == Some(Element::Fire);
                let (c, c2) = if fire { (C_FIRE, C_FIRE2) } else { (C_FROST, C_FROST2) };
                self.ring(c2, ex, ey, 0.45, 16.0);
                self.burst_add(PK_STAR, c2, ex, ey, n(10.0), 110.0, 0.6, 3.5, 0.0, 3.0);
                self.burst_add(PK_SPARK, c, ex, ey, n(14.0), 140.0, 0.5, 1.0, 160.0, 2.0);
            }
            EventKind::Lever | EventKind::Mirror => {
                let col = if e.k == EventKind::Lever { self.ch_col(level.levers.get(i).map(|l| l.ch.as_str())) } else { C_WHITE };
                self.ring(col, ex, ey, 0.35, 10.0);
                self.burst_add(PK_SPARK, col, ex, ey, n(5.0), 60.0, 0.3, 0.9, 100.0, 2.0);
            }
            EventKind::Plate | EventKind::Button | EventKind::Sensor => {
                if e.on == Some(true) {
                    let ch = match e.k {
                        EventKind::Plate => level.plates.get(i).map(|p| p.ch.as_str()),
                        EventKind::Button => level.buttons.get(i).map(|b| b.ch.as_str()),
                        _ => level.sensors.get(i).map(|s| s.ch.as_str()),
                    };
                    let col = self.ch_col(ch);
                    self.ring(col, ex, ey, 0.4, 12.0);
                }
            }
            EventKind::Melt => {
                let r = level.ice.get(i);
                let (cx, cy) = r.map_or((ex, ey), |r| (r.x + r.w / 2.0, r.y + r.h / 2.0));
                let (hw, hh) = r.map_or((TILE / 2.0, TILE / 2.0), |r| (r.w / 2.0, r.h / 2.0));
                let area = hw * hh * 4.0 / (TILE * TILE);
                self.burst(PK_SHARD, C_ICE, cx, cy, n((8.0 + area * 4.0).min(30.0)), 130.0, 0.8, 3.5, 380.0, 1.0, false, 14.0, 60.0);
                for _ in 0..n((6.0 + area * 2.0).min(16.0)) {
                    self.parts.spawn(PK_STEAM, C_STEAM, cx + rand(-hw, hw), cy + rand(-hh, hh), rand(-10.0, 10.0), rand(-45.0, -20.0), rand(0.6, 1.0), rand(3.0, 6.0), -8.0, 1.5, false, 0.0);
                }
            }
            EventKind::Freeze => {
                self.burst(PK_FLAKE, C_FROST2, ex, ey, n(5.0), 40.0, 0.6, 2.2, 0.0, 3.0, true, 3.0, 0.0);
                self.burst_add(PK_STAR, C_WHITE, ex, ey - 4.0, n(2.0), 20.0, 0.5, 2.5, 0.0, 3.0);
            }
            EventKind::Thaw => {
                for _ in 0..n(4.0) {
                    self.parts.spawn(PK_DOT, C_WATER, ex + rand(-8.0, 8.0), ey - 4.0, rand(-10.0, 10.0), rand(-50.0, -20.0), 0.4, 1.2, 300.0, 1.0, false, 0.0);
                }
            }
            EventKind::Win => {
                self.win_at = t;
                self.confetti(n(80.0));
                for d in level.doors {
                    self.burst_add(PK_STAR, C_STAR, d.x + d.w / 2.0, d.y + d.h / 2.0, n(10.0), 120.0, 0.8, 3.5, 0.0, 2.0);
                }
            }
            _ => {}
        }
    }

    fn portal_near(&self, level: &Stage, x: f64, y: f64) -> usize {
        let mut best = 0;
        let mut bd = f64::INFINITY;
        for (i, p) in level.portals.iter().enumerate() {
            let d = (p.r.x + p.r.w / 2.0 - x).abs() + (p.r.y + p.r.h / 2.0 - y).abs();
            if d < bd {
                bd = d;
                best = i;
            }
        }
        C_PORTAL + self.portal_col.get(best).copied().unwrap_or(0)
    }

    pub fn confetti(&mut self, n: i64) {
        let cols = [C_FIRE, C_FIRE2, C_FROST, C_FROST2, C_STAR];
        let v = self.vr;
        let u = v.w / 640.0;
        for i in 0..n.max(0) {
            let from_left = i % 2 == 0;
            let x = if from_left { v.x + rand(-10.0, 60.0) * u } else { v.x + v.w - rand(-10.0, 60.0) * u };
            self.parts.spawn(
                PK_CONFETTI,
                cols[i as usize % cols.len()],
                x,
                v.y + v.h - rand(-10.0, 110.0) * u,
                (if from_left { 1.0 } else { -1.0 }) * rand(60.0, 200.0) * u,
                rand(-420.0, -260.0) * u,
                rand(1.6, 2.6),
                rand(3.0, 5.0) * u,
                320.0 * u,
                1.4,
                false,
                9.0,
            );
        }
    }

    // ---- the frame

    /// one frame on a canvas of W × H pixels. events are everything since the last frame
    pub fn draw(&mut self, g: &mut Canvas, input: &DrawInput) {
        let level = input.level;
        let cur = input.cur;
        let (w, h) = (g.width, g.height);
        if w < 2.0 || h < 2.0 {
            return;
        }
        if g.generation != self.generation {
            // offscreens belong to the target they were made on
            self.generation = g.generation;
            self.chunks.clear();
            self.chunk_order.clear();
            self.glow_cache.clear();
            self.hole = None;
            self.mask = None;
        }
        let now_ms = input.now_ms;
        let opts = input.opts;
        let dt = if self.last_ms != 0.0 { clamp((now_ms - self.last_ms) / 1000.0, 0.0, 0.05) } else { 1.0 / 60.0 };
        self.last_ms = now_ms;
        let t = now_ms / 1000.0;
        let key = format!(
            "{}|{}x{}|{}|{}",
            level.index,
            level.cols,
            level.rows,
            level.tiles.len(),
            level.spawns[0].x
        );
        let fresh = self.level_key.as_deref() != Some(key.as_str());
        if fresh {
            if self.level_key.is_some() || level.index > 0 {
                self.iris_at = t;
                self.iris_dur = 0.5;
            }
            self.set_level(level, key);
            self.dark_a = if cur.lit { 0.0 } else { 1.0 };
        }
        let a = clamp(input.alpha, 0.0, 1.0);
        let p0 = input.prev;

        let mut snap = fresh;
        for e in input.events {
            if e.k == EventKind::Respawn || e.k == EventKind::Stage {
                snap = true;
            }
            self.on_event(level, cur, e, opts, t);
        }
        if snap {
            self.cam = None;
        }

        for i in 0..2 {
            let c = cur.players[i];
            let p = p0.map(|p| p.players[i]);
            let is_far = p.is_none_or(|p| (p.x - c.x).abs() > 40.0 || (p.y - c.y).abs() > 40.0);
            self.px[i] = match p {
                Some(p) if !is_far => p.x + (c.x - p.x) * a,
                _ => c.x,
            };
            self.py[i] = match p {
                Some(p) if !is_far => p.y + (c.y - p.y) * a,
                _ => c.y,
            };
            self.pos[i] = (self.px[i], self.py[i]);
        }

        let target = camera_at(level.w, level.h, (self.px[0], self.py[0]), (self.px[1], self.py[1]), w / h);
        let eased = ease_camera(self.cam.as_ref(), &target, dt * 60.0);
        self.cam = Some(eased);

        let (mut sx, mut sy) = (0.0, 0.0);
        if self.shake > 0.0 && !opts.reduce_motion {
            let amp = self.shake * self.shake * 3.0 / eased.zoom.max(0.4);
            sx = rand(-amp, amp);
            sy = rand(-amp, amp);
        }
        self.shake = (self.shake - dt * 3.0).max(0.0);

        let v = view_for(&eased, w, sx, sy);
        self.view = v;
        self.vr = Camera { x: -v.ox / v.k, y: -v.oy / v.k, w: w / v.k, h: h / v.k, zoom: eased.zoom };
        self.far = eased.zoom < 0.55;
        self.beams = trace_beams(level, cur).iter().map(|b| (b.x1, b.y1, b.x2, b.y2)).collect();

        let target_dark = if cur.lit { 0.0 } else { 1.0 };
        self.dark_a = approach(self.dark_a, target_dark, if cur.lit { 16.0 } else { 9.0 }, dt);
        if (self.dark_a - target_dark).abs() < 0.004 {
            self.dark_a = target_dark;
        }
        self.glow_boost = (self.glow_boost - dt * 3.2).max(0.0);
        self.flash = (self.flash - dt * 4.5).max(0.0);

        g.reset_transform();
        g.set_alpha(1.0);
        g.set_composite(Composite::SourceOver);
        g.fill_style(css1(self.pal.wall));
        g.fill_rect(0.0, 0.0, w, h);
        self.draw_static(g, level, w, h);
        g.set_transform(v.k, 0.0, 0.0, v.k, v.ox, v.oy);

        self.draw_fan_columns(g, level, cur, dt, opts);
        self.draw_mover_rails(g, level);
        self.draw_tiles(g, level, cur, t, opts);
        self.draw_signs(g, level, t);
        self.draw_checkpoints(g, level, cur, t, dt);
        self.draw_doors(g, level, cur, t, dt);
        self.draw_portals(g, level, t, opts);
        self.draw_pools(g, level, cur, t, dt, opts);
        self.draw_plates(g, level, cur, t);
        self.draw_buttons(g, level, cur, t);
        self.draw_levers(g, level, cur, dt);
        self.draw_lamps(g, level, cur, t, dt);
        self.draw_gswitches(g, level, t, dt);
        self.draw_sensors(g, level, cur, t);
        self.draw_emitters(g, level, cur, t);
        self.draw_mirrors(g, level, cur, dt);
        self.draw_gates(g, level, cur);
        self.draw_locks(g, level, cur, t, dt);
        self.draw_movers(g, level, p0, cur, a);
        self.draw_ice(g, level, cur, t);
        self.draw_boxes(g, p0, cur, a);
        self.draw_gems(g, level, cur, t, opts);
        self.draw_keys(g, level, cur, t, opts);
        self.draw_arrows(g, level, cur, t, opts);
        self.draw_beams(g, t, opts);
        self.draw_cannons(g, level, cur, t, dt, opts);
        for i in 0..2 {
            self.draw_player(g, i, cur, t, dt, opts);
        }
        self.draw_carried(g, cur, t, dt);

        self.parts.update(dt);
        self.parts.draw(g, &self.colors, Composite::SourceOver, 0);
        self.draw_darkness(g, level, cur, t, w, h, opts);
        g.set_transform(v.k, 0.0, 0.0, v.k, v.ox, v.oy);
        self.parts.draw(g, &self.colors, if self.pal.dark { Composite::Lighter } else { Composite::SourceOver }, 1);

        if cur.phase == Phase::Won
            && self.win_at >= 0.0
            && t - self.win_at < 1.6
            && rnd() < (if opts.reduce_motion { 9.0 } else { 30.0 }) * dt
        {
            self.confetti(2);
        }
        if cur.phase == Phase::Won && self.win_at < 0.0 {
            self.win_at = t;
        }

        g.reset_transform();
        g.set_alpha(1.0);
        g.set_composite(Composite::SourceOver);
        if cur.phase == Phase::Dead {
            let d = clamp((cur.phase_t as f64 - 18.0) / 30.0, 0.0, 1.0);
            if d > 0.0 {
                g.fill_style(if self.pal.dark {
                    Col::rgba(0.0, 0.0, 0.0, (0.3 * d * 1000.0).round() / 1000.0)
                } else {
                    Col::rgba(30.0, 24.0, 48.0, (0.16 * d * 1000.0).round() / 1000.0)
                });
                g.fill_rect(0.0, 0.0, w, h);
            }
        }
        if self.flash > 0.01 {
            g.fill_style(Col::rgba(255.0, 244.0, 214.0, (0.3 * self.flash * self.flash * 1000.0).round() / 1000.0));
            g.fill_rect(0.0, 0.0, w, h);
        }
        self.draw_lamp_clock(g, cur, t, w, h);
        self.draw_wipe(g, level, cur, a, t, w, h, opts);
        g.reset_transform();
        g.set_alpha(1.0);
    }

    // ---- the dark

    fn hole_sprite(&mut self, g: &Canvas, res: &Res) -> Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap> {
        if self.hole.is_none() {
            let mut s = g.offscreen(128.0, 128.0)?;
            s.draw(res, |s| {
                let mut gr = Gradient::radial(64.0, 64.0, 0.0, 64.0, 64.0, 64.0);
                gr.stop(0.0, Col::rgba(0.0, 0.0, 0.0, 1.0));
                gr.stop(HOLE_CORE, Col::rgba(0.0, 0.0, 0.0, 1.0));
                gr.stop(HOLE_CORE + (1.0 - HOLE_CORE) * 0.5, Col::rgba(0.0, 0.0, 0.0, 0.42));
                gr.stop(1.0, Col::rgba(0.0, 0.0, 0.0, 0.0));
                s.fill_style(gr);
                s.fill_rect(0.0, 0.0, 128.0, 128.0);
            });
            self.hole = Some(s);
        }
        self.hole.as_mut()?.image()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_darkness(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64, w: f64, h: f64, opts: DrawOpts) {
        let mut lvl = self.dark_a;
        if cur.lit_t > 0 && cur.lit_t < 90 && !opts.reduce_motion {
            let near = 1.0 - cur.lit_t as f64 / 90.0;
            let blink = if (t * (14.0 + near * 22.0)).sin() > 0.35 { 0.1 + 0.3 * near } else { 0.0 };
            lvl = if cur.lit { lvl.max(blink) } else { lvl.min(1.0 - blink) };
        }
        if lvl < 0.004 {
            return;
        }
        let q = 4.0;
        let (mw, mh) = ((w / q).ceil(), (h / q).ceil());
        if self.mask.as_ref().is_none_or(|m| m.w != mw || m.h != mh) {
            self.mask = g.offscreen(mw, mh);
        }
        let res = g.res();
        let sprite = self.hole_sprite(g, res);
        let v = self.view;
        let (k, ox, oy) = (v.k / q, v.ox / q, v.oy / q);
        let boost = 1.0 + 2.2 * self.glow_boost * self.glow_boost;
        let holes = light_holes(level, cur, &self.lava_tops_list, &self.pos);
        let vr = self.vr;
        let visible = |x: f64, y: f64, w: f64, h: f64| in_view(&vr, x, y, w, h, 0.0);
        let beams = self.beams.clone();
        let Some(m) = self.mask.as_mut() else { return };
        m.draw(res, |m| {
            m.reset_transform();
            m.set_composite(Composite::SourceOver);
            m.set_alpha(1.0);
            m.clear_rect(0.0, 0.0, mw, mh);
            m.fill_style(NIGHT);
            if !level.darks.is_empty() {
                for r in &level.darks {
                    if visible(r.x, r.y, r.w, r.h) {
                        m.fill_rect(r.x * k + ox, r.y * k + oy, r.w * k, r.h * k);
                    }
                }
            } else {
                m.fill_rect(0.0, 0.0, mw, mh);
            }
            m.set_composite(Composite::DestinationOut);
            for hl in &holes {
                let mut r = hl.r;
                if let Some(hp) = hl.p {
                    r *= boost * if hp == 0 && !opts.reduce_motion { 1.0 + 0.025 * (t * 9.0).sin() + 0.012 * (t * 23.7).sin() } else { 1.0 };
                }
                if !visible(hl.x - r, hl.y - r, 2.0 * r, 2.0 * r) {
                    continue;
                }
                m.set_alpha(hl.a);
                m.draw_image(sprite.as_ref(), (hl.x - r) * k + ox, (hl.y - r) * k + oy, 2.0 * r * k, 2.0 * r * k);
            }
            if !beams.is_empty() {
                m.set_transform(k, 0.0, 0.0, k, ox, oy);
                m.stroke_style(Col::BLACK);
                m.set_line_cap(Cap::Round);
                for pass in 0..2 {
                    m.set_alpha(if pass != 0 { 0.6 } else { 0.3 });
                    m.set_line_width(if pass != 0 { GLOW_SMALL * 0.6 } else { GLOW_SMALL * 1.5 });
                    m.begin_path();
                    for b in &beams {
                        m.move_to(b.0, b.1);
                        m.line_to(b.2, b.3);
                    }
                    m.stroke();
                }
            }
        });
        let img = m.image();
        g.reset_transform();
        g.set_composite(Composite::SourceOver);
        g.set_alpha(lvl);
        g.set_smoothing(true);
        g.draw_image(img.as_ref(), 0.0, 0.0, mw * q, mh * q);
        g.set_alpha(1.0);
        g.set_transform(v.k, 0.0, 0.0, v.k, v.ox, v.oy);
        self.begin_glow(g);
        for hl in &holes {
            let Some(hp) = hl.p else { break };
            let r = hl.r * boost;
            if !self.vis(hl.x - r, hl.y - r, 2.0 * r, 2.0 * r, 0.0) {
                continue;
            }
            let c = css1(if hp == 0 { self.pal.fire } else { self.pal.frost });
            self.glow(g, hl.x, hl.y, r * 1.05, c, (if hp == 0 { 0.3 } else { 0.26 }) * lvl);
        }
        self.end_glow(g);
    }

    fn draw_lamp_clock(&self, g: &mut Canvas, cur: &GameState, t: f64, w: f64, h: f64) {
        if cur.lit_t <= 0 || self.lamp_max <= 0.0 {
            return;
        }
        let u = 1f64.max((w / 640.0).min(h / 360.0));
        let frac = clamp(cur.lit_t as f64 / self.lamp_max, 0.0, 1.0);
        let (bw, x, y) = (84.0 * u, w / 2.0 - 42.0 * u, 9.0 * u);
        let col = if cur.lit { SUN } else { MOON };
        let low = frac < 0.25 && (t * 20.0).sin() > 0.0;
        g.reset_transform();
        g.fill_style(Col::rgba(0.0, 0.0, 0.0, 0.45));
        rr(g, x - 9.0 * u, y - 4.0 * u, bw + 13.0 * u, 8.0 * u, 4.0 * u);
        g.fill();
        g.fill_style(css(col, 0.28));
        rr(g, x, y - 1.5 * u, bw, 3.0 * u, 1.5 * u);
        g.fill();
        g.fill_style(css1(if low { WHITE } else { col }));
        rr(g, x, y - 1.5 * u, (3.0 * u).max(bw * frac), 3.0 * u, 1.5 * u);
        g.fill();
        g.begin_path();
        g.arc(x - 4.5 * u, y, 2.4 * u, 0.0, TAU, false);
        g.fill();
        if !cur.lit {
            g.fill_style(Col::rgba(0.0, 0.0, 0.0, 0.75));
            g.begin_path();
            g.arc(x - 3.5 * u, y - 0.8 * u, 2.0 * u, 0.0, TAU, false);
            g.fill();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_wipe(&self, g: &mut Canvas, level: &Stage, cur: &GameState, a: f64, t: f64, w: f64, h: f64, opts: DrawOpts) {
        let (mut f, mut wx, mut wy) = (0.0, 0.0, 0.0);
        if cur.phase == Phase::Clear {
            f = clamp((cur.phase_t as f64 + a - 26.0) / (STAGE_TICKS as f64 - 32.0), 0.0, 1.0);
            wx = (level.doors[0].x + level.doors[1].x + TILE) / 2.0;
            wy = (level.doors[0].y + level.doors[1].y) / 2.0 + TILE;
        } else if t - self.iris_at < self.iris_dur && t >= self.iris_at {
            f = 1.0 - (t - self.iris_at) / self.iris_dur;
            wx = (self.px[0] + self.px[1] + PLAYER_W) / 2.0;
            wy = (self.py[0] + self.py[1] + PLAYER_H) / 2.0;
        }
        if f <= 0.001 {
            return;
        }
        let v = self.view;
        let (cx, cy) = (wx * v.k + v.ox, wy * v.k + v.oy);
        let dark = self.pal.dark || self.dark_a > 0.5;
        let col = if dark { (8.0, 7.0, 14.0) } else { mix(self.pal.tape, self.pal.wall, 0.35) };
        g.reset_transform();
        if opts.reduce_motion {
            g.fill_style(css(col, f));
            g.fill_rect(0.0, 0.0, w, h);
            return;
        }
        let reach = cx.max(w - cx).hypot(cy.max(h - cy));
        let e = f * f * (3.0 - 2.0 * f);
        let r = reach * (1.0 - e);
        g.fill_style(css1(col));
        g.begin_path();
        g.rect(0.0, 0.0, w, h);
        if r > 0.5 {
            g.arc(cx, cy, r, 0.0, TAU, true);
        }
        g.fill_even_odd();
        if r > 0.5 {
            g.set_line_width((v.k * 1.4).max(2.0));
            g.stroke_style(css(self.pal.fire, 0.8));
            g.begin_path();
            g.arc(cx, cy, r, PI * 0.5, PI * 1.5, false);
            g.stroke();
            g.stroke_style(css(self.pal.frost, 0.8));
            g.begin_path();
            g.arc(cx, cy, r, PI * 1.5, PI * 2.5, false);
            g.stroke();
        }
    }
}

fn tramp_near(level: &Stage, x: f64, y: f64) -> i64 {
    let (cx, cy) = ((x / TILE).floor() as i64, (y / TILE).floor() as i64);
    let mut best = -1;
    let mut bd = f64::INFINITY;
    for ty in (cy - 2)..=(cy + 2) {
        for tx in (cx - 1)..=(cx + 1) {
            if tx < 0 || ty < 0 || tx >= level.cols as i64 || ty >= level.rows as i64 || level.tiles[ty as usize * level.cols + tx as usize] != T_TRAMP {
                continue;
            }
            let d = ((tx as f64 + 0.5) * TILE - x).abs() + ((ty as f64 + 0.5) * TILE - y).abs();
            if d < bd {
                bd = d;
                best = ty * level.cols as i64 + tx;
            }
        }
    }
    best
}

/// the static backdrop's colour for a stage's thumbnail
pub fn at<T: Copy>(v: &[T], i: usize) -> Option<T> {
    v.get(i).copied()
}
