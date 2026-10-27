//! emberDraw.ts, the stage's things (the Mac's EmberDrawThings.swift): pools, doors, signs, portals, switches, gates,
//! movers, boxes, ice, version 2's tiles and things, fans, gems and beams. Line for line with the Swift.

use std::f64::consts::PI;

use qcalc_core::games::ember::engine::{gate_rect, tile_at};
use qcalc_core::games::ember::{
    ARROW_RESPAWN_TICKS, CANNON_FLY_TICKS, CANNON_FUSE_TICKS, CRUMBLE_GONE_TICKS, CRUMBLE_TICKS,
    Dir, Element, GRAVITY, GameState, ONEWAY_H, PLAYER_H, PLAYER_W, POOL_SINK, Phase, Slant, Stage,
    T_CRUMBLE, T_GOO, T_LAVA, T_LIGHT, T_ONEWAY, T_THIN, T_TRAMP, T_WALL, T_WATER, TILE, Who,
};

use super::draw::*;
use super::fx::*;
use crate::games::canvas::{Baseline, Canvas, Cap, Col, Gradient, Join, TextAlign};

const T: f64 = TILE;

impl Renderer {
    // ---- pools

    pub fn pool_gradient(&self, kind: u8) -> Gradient {
        let mut gr = Gradient::linear(0.0, 0.0, 0.0, TILE);
        let (top, bot) = match kind {
            T_LAVA => (LAVA_TOP, LAVA_BOT),
            T_WATER => (WATER_TOP, WATER_BOT),
            T_GOO => (GOO_TOP, GOO_BOT),
            T_WALL => (ICE_TOP, ICE_BOT),
            _ => (THIN_TOP, THIN_BOT),
        };
        let water = kind == T_WATER || kind == T_THIN;
        gr.stop(0.0, css(top, if water { 0.88 } else { 1.0 }));
        if kind == T_LAVA {
            gr.stop(0.35, css1(LAVA_MID));
        }
        gr.stop(1.0, css(bot, if water { 0.95 } else { 1.0 }));
        gr
    }

    fn wave(kind: u8, wx: f64, t: f64) -> f64 {
        if kind == T_LAVA {
            return (wx * 0.22 + t * 2.1).sin() * 0.9 + (wx * 0.53 - t * 3.3).sin() * 0.5;
        }
        if kind == T_GOO {
            return (wx * 0.17 + t * 1.3).sin() * 0.7 + (wx * 0.41 - t * 1.9).sin() * 0.35;
        }
        (wx * 0.19 + t * 2.6).sin() * 0.8 + (wx * 0.47 - t * 3.7).sin() * 0.35
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_pools(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        dt: f64,
        opts: DrawOpts,
    ) {
        let tiles = &level.tiles;
        let vt = visible_tiles(&self.vr, level.cols, level.rows, 1);
        let sink = POOL_SINK;
        for ty in vt.y0..vt.y1.max(vt.y0) {
            for tx in vt.x0..vt.x1.max(vt.x0) {
                let i = ty as usize * level.cols + tx as usize;
                let kind = tiles[i];
                if kind != T_LAVA && kind != T_WATER && kind != T_GOO && kind != T_THIN {
                    continue;
                }
                let (x, y) = (tx as f64 * T, ty as f64 * T);
                if kind == T_THIN && tile_at(level, cur, tx, ty) == T_WALL {
                    self.draw_frozen(g, level, cur, i, x, y, t);
                    continue;
                }
                let open = ty == 0 || tiles[i - level.cols] != kind;
                if self.far {
                    let mid = match kind {
                        T_LAVA => LAVA_MID,
                        T_WATER => WATER_BOT,
                        T_GOO => GOO_BOT,
                        _ => THIN_BOT,
                    };
                    g.fill_style(css(mid, if kind == T_THIN { 0.9 } else { 1.0 }));
                    let s = if open { sink } else { 0.0 };
                    g.fill_rect(x, y + s, T + 0.5, T - s + 0.5);
                    if open {
                        let top = match kind {
                            T_LAVA => LAVA_TOP,
                            T_WATER => WATER_TOP,
                            T_GOO => GOO_TOP,
                            _ => THIN_TOP,
                        };
                        g.fill_style(css1(top));
                        g.fill_rect(x, y + sink, T + 0.5, 2.5);
                    }
                    continue;
                }
                g.save();
                g.translate(x, y);
                g.begin_path();
                if open {
                    g.move_to(0.0, sink + Self::wave(kind, x, t));
                    let mut lx = 2.0;
                    while lx <= T {
                        g.line_to(lx, sink + Self::wave(kind, x + lx, t));
                        lx += 2.0;
                    }
                } else {
                    g.move_to(0.0, 0.0);
                    g.line_to(T, 0.0);
                }
                g.line_to(T, T);
                g.line_to(0.0, T);
                g.close_path();
                g.fill_style(self.pool_gradient(kind));
                g.fill();
                if kind == T_LAVA || kind == T_GOO {
                    for b in 0..2 {
                        let seed = hash01((i * 7 + b) as f64);
                        let rate = if kind == T_LAVA { 0.55 } else { 0.35 };
                        let ph = (t * rate + seed) % 1.0;
                        let bx = 3.0
                            + hash01((i * 13 + b) as f64 + (t * rate + seed).floor() * 31.0)
                                * (T - 6.0);
                        let by = T - 2.0 - ph * (T - sink - 2.0) * if open { 1.05 } else { 1.2 };
                        let r = 0.8 + ph * 1.6;
                        if by < if open { sink + 1.0 } else { 0.0 } {
                            continue;
                        }
                        g.fill_style(if kind == T_LAVA {
                            Col::rgba(255.0, 236.0, 160.0, 0.8)
                        } else {
                            Col::rgba(214.0, 255.0, 150.0, 0.75)
                        });
                        g.begin_path();
                        g.arc(bx, by, r, 0.0, TAU, false);
                        g.fill();
                        if kind == T_GOO {
                            g.stroke_style(Col::rgba(40.0, 110.0, 20.0, 0.5));
                            g.set_line_width(0.6);
                            g.stroke();
                        }
                    }
                } else if open {
                    for b in 0..2 {
                        let ph = (t * 0.3 + hash01((i * 5 + b) as f64)) % 1.0;
                        let gx = (ph * T * 1.4 + b as f64 * 9.0) % T;
                        g.fill_style(Col::rgba(
                            255.0,
                            255.0,
                            255.0,
                            ((0.5 * (ph * PI).sin()) * 1000.0).round() / 1000.0,
                        ));
                        g.fill_rect(gx, sink + 3.0 + b as f64 * 4.0, 3.0, 0.8);
                    }
                }
                if open {
                    g.begin_path();
                    g.move_to(0.0, sink + Self::wave(kind, x, t));
                    let mut lx = 2.0;
                    while lx <= T {
                        g.line_to(lx, sink + Self::wave(kind, x + lx, t));
                        lx += 2.0;
                    }
                    g.stroke_style(match kind {
                        T_LAVA => Col::rgba(255.0, 244.0, 190.0, 0.95),
                        T_GOO => Col::rgba(220.0, 255.0, 160.0, 0.9),
                        _ => Col::rgba(235.0, 248.0, 255.0, 0.9),
                    });
                    g.set_line_width(1.2);
                    g.stroke();
                }
                g.restore();
            }
        }
        self.begin_glow(g);
        let mut seen = 0usize;
        let tops = self.lava_tops_list.clone();
        for &i in &tops {
            let tx = (i % level.cols) as i64;
            let ty = ((i - tx as usize) / level.cols) as i64;
            if tx < vt.x0 || tx >= vt.x1 || ty < vt.y0 || ty >= vt.y1 {
                continue;
            }
            seen += 1;
            if self.far && tx & 1 != 0 {
                continue;
            }
            let fl = 0.85 + 0.15 * (t * 3.0 + tx as f64).sin();
            self.glow(
                g,
                tx as f64 * T + T / 2.0,
                ty as f64 * T + POOL_SINK + 2.0,
                if self.far { 36.0 } else { 24.0 },
                css1(LAVA_MID),
                0.33 * fl,
            );
        }
        self.end_glow(g);
        if seen > 0 && rnd() < seen.min(40) as f64 * dt * if opts.reduce_motion { 0.2 } else { 0.7 }
        {
            let i = tops[(rnd() * tops.len() as f64).floor() as usize % tops.len()];
            let tx = (i % level.cols) as i64;
            let ty = ((i - tx as usize) / level.cols) as i64;
            if tx >= vt.x0 && tx < vt.x1 && ty >= vt.y0 && ty < vt.y1 {
                self.parts.spawn(
                    PK_SPARK,
                    if rnd() < 0.5 { C_CORE } else { C_FIRE2 },
                    tx as f64 * T + rand(2.0, 18.0),
                    ty as f64 * T + POOL_SINK,
                    rand(-12.0, 12.0),
                    rand(-70.0, -30.0),
                    rand(0.5, 0.9),
                    rand(0.7, 1.1),
                    60.0,
                    0.5,
                    true,
                    0.0,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_frozen(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        i: usize,
        x: f64,
        y: f64,
        t: f64,
    ) {
        let left = self
            .thin_index
            .get(&i)
            .and_then(|&j| cur.thin.get(j).copied())
            .unwrap_or(999);
        let warn = if left < 40 {
            0.55 + 0.45 * (t * 9.0).sin().abs()
        } else {
            1.0
        };
        g.save();
        g.translate(x, y);
        g.set_alpha(warn);
        g.fill_style(self.pool_gradient(T_WALL));
        let (tx, ty) = ((x / T) as i64, (y / T) as i64);
        let l_open =
            (i == 0 || level.tiles.get(i - 1) != Some(&T_THIN)) && !is_wall(level, tx - 1, ty);
        let r_open = level.tiles.get(i + 1) != Some(&T_THIN) && !is_wall(level, tx + 1, ty);
        round_rect_path(
            g,
            0.0,
            0.0,
            T,
            T,
            if l_open { 3.0 } else { 0.0 },
            if r_open { 3.0 } else { 0.0 },
            0.0,
            0.0,
        );
        g.fill();
        g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.85));
        g.fill_rect(0.0, 0.0, T, 1.6);
        g.stroke_style(Col::rgba(255.0, 255.0, 255.0, 0.7));
        g.set_line_width(0.8);
        g.begin_path();
        let h = hash01(i as f64);
        g.move_to(3.0 + h * 6.0, 4.0);
        g.line_to(7.0 + h * 4.0, 9.0);
        g.line_to(5.0 + h * 8.0, 15.0);
        g.move_to(12.0 + h * 4.0, 3.0);
        g.line_to(16.0, 7.0);
        g.stroke();
        g.stroke_style(css(self.pal.frost, 0.35));
        g.stroke_rect(0.4, 0.4, T - 0.8, T - 0.8);
        g.restore();
        g.set_alpha(1.0);
        if rnd() < 0.012 {
            self.parts.spawn(
                PK_STAR,
                C_WHITE,
                x + rand(3.0, 17.0),
                y + rand(2.0, 12.0),
                0.0,
                0.0,
                0.6,
                2.5,
                0.0,
                0.0,
                true,
                0.0,
            );
        }
    }

    // ---- doors, signs, portals

    pub fn draw_doors(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64, dt: f64) {
        let p = self.pal;
        for i in 0..2 {
            let d = level.doors[i];
            let fire = i == 0;
            let c = if fire { p.fire } else { p.frost };
            let c2 = if fire { p.fire2 } else { p.frost2 };
            let in_door = cur.players[i].in_door;
            let open_now = in_door || cur.phase == Phase::Won || cur.phase == Phase::Clear;
            self.door_open[i] =
                approach(self.door_open[i], if open_now { 1.0 } else { 0.0 }, 6.0, dt);
            if !self.vis(d.x, d.y, d.w, d.h, 44.0) {
                continue;
            }
            let o = self.door_open[i];
            let cx = d.x + d.w / 2.0;
            self.begin_glow(g);
            self.glow(
                g,
                cx,
                d.y + d.h * 0.55,
                26.0 + o * 14.0,
                css1(c),
                0.22 + o * 0.45 + 0.05 * (t * 2.0 + i as f64).sin(),
            );
            self.end_glow(g);
            let frame = if p.dark {
                mix(p.wall, c, 0.25)
            } else {
                mix(p.wall, c, 0.2)
            };
            arch_path(g, d.x - 2.5, d.y - 3.0, d.w + 5.0, d.h + 3.0);
            g.fill_style(css1(frame));
            g.fill();
            g.stroke_style(css(mix(frame, BLACK, 0.3), 0.6));
            g.set_line_width(0.6);
            g.begin_path();
            for k in 1..6 {
                let ang = PI + f64::from(k) * PI / 6.0;
                let ax = cx + ang.cos() * (d.w / 2.0);
                let ay = d.y + d.w / 2.0 + ang.sin() * (d.w / 2.0);
                g.move_to(ax, ay);
                g.line_to(
                    cx + ang.cos() * (d.w / 2.0 + 2.5),
                    d.y + d.w / 2.0 + ang.sin() * (d.w / 2.0 + 3.0),
                );
            }
            let mut yy = d.y + d.w / 2.0 + 6.0;
            while yy < d.y + d.h - 2.0 {
                g.move_to(d.x - 2.5, yy);
                g.line_to(d.x, yy);
                g.move_to(d.x + d.w, yy);
                g.line_to(d.x + d.w + 2.5, yy);
                yy += 6.0;
            }
            g.stroke();
            arch_path(g, d.x, d.y, d.w, d.h);
            let mut inner = Gradient::linear(0.0, d.y, 0.0, d.y + d.h);
            let shut = if p.dark {
                mix(c, BLACK, 0.72)
            } else {
                mix(c, p.ink, 0.55)
            };
            inner.stop(0.0, css1(mix(shut, c2, o * 0.9)));
            inner.stop(1.0, css1(mix(mix(shut, BLACK, 0.2), c, o * 0.8)));
            g.fill_style(inner);
            g.fill();
            g.stroke_style(css(c, 0.45 + o * 0.4));
            g.set_line_width(0.9);
            arch_path(g, d.x + 0.5, d.y + 0.5, d.w - 1.0, d.h - 0.5);
            g.stroke();
            if o > 0.02 {
                g.fill_style(css(c2, 0.35 * o));
                g.begin_path();
                g.ellipse(cx, d.y + d.h, d.w * 0.9, 2.2, 0.0, 0.0, TAU, false);
                g.fill();
            }
            g.fill_style(css1(c));
            g.begin_path();
            g.move_to(cx - 2.5, d.y - 3.0);
            g.line_to(cx + 2.5, d.y - 3.0);
            g.line_to(cx + 1.6, d.y + 1.0);
            g.line_to(cx - 1.6, d.y + 1.0);
            g.close_path();
            g.fill();
            g.fill_style(css(c, 0.8));
            g.fill_rect(d.x - 3.0, d.y + d.h - 1.5, d.w + 6.0, 1.5);
            let gy = d.y + d.h * 0.42 + (t * 2.2 + i as f64).sin() * 0.8;
            g.set_alpha(0.75 + o * 0.25);
            if fire {
                g.fill_style(css1(if o > 0.5 { WHITE } else { c2 }));
                flame_glyph(g, cx, gy, 11.0);
                g.fill();
                g.fill_style(css1(if o > 0.5 { c2 } else { c }));
                flame_glyph(g, cx + 0.3, gy + 2.0, 5.0);
                g.fill();
            } else {
                g.stroke_style(css1(if o > 0.5 { WHITE } else { c2 }));
                g.set_line_width(1.1);
                g.set_line_cap(Cap::Round);
                flake_glyph(g, cx, gy, 4.6);
                g.stroke();
            }
            g.set_alpha(1.0);
        }
    }

    pub fn draw_signs(&mut self, g: &mut Canvas, level: &Stage, t: f64) {
        if level.signs.is_empty() {
            return;
        }
        let p = self.pal;
        g.set_font(9.0, false);
        g.set_text_align(TextAlign::Center);
        g.set_text_baseline(Baseline::Middle);
        for (i, s) in level.signs.iter().enumerate() {
            if !self.vis(s.x - 100.0, s.y - 10.0, 200.0, 20.0, 24.0) {
                continue;
            }
            let w = match self.text_w.get(&s.text) {
                Some(w) => *w,
                None => {
                    let w = g.measure_text(&s.text);
                    self.text_w.insert(s.text.clone(), w);
                    w
                }
            };
            let cx = s.x;
            let cy = s.y + (t * 1.6 + i as f64).sin() * 0.8;
            let bw = w + 10.0;
            rr(g, cx - bw / 2.0, cy - 7.0, bw, 14.0, 7.0);
            g.fill_style(css(p.bg, if p.dark { 0.82 } else { 0.88 }));
            g.fill();
            g.stroke_style(css(p.ink, 0.12));
            g.set_line_width(0.8);
            g.stroke();
            g.fill_style(css(p.ink, 0.78));
            g.fill_text(&s.text, cx, cy + 0.5);
        }
    }

    pub fn draw_portals(&mut self, g: &mut Canvas, level: &Stage, t: f64, opts: DrawOpts) {
        for (i, pt) in level.portals.iter().enumerate() {
            let r = pt.r;
            if !self.vis(r.x, r.y, r.w, r.h, 34.0) {
                continue;
            }
            let pc = self.portal_col.get(i).copied().unwrap_or(0);
            let col = PORTAL_COLORS[pc];
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            let (rx, ry) = (r.w / 2.0 - 1.0, r.h / 2.0 - 1.0);
            self.begin_glow(g);
            self.glow(
                g,
                cx,
                cy,
                30.0,
                css1(col),
                0.45 + 0.1 * (t * 3.0 + i as f64).sin(),
            );
            self.end_glow(g);
            let mut gr = Gradient::radial(cx, cy, 0.0, cx, cy, ry);
            gr.stop(0.0, css1(mix(col, BLACK, 0.75)));
            gr.stop(0.7, css1(mix(col, BLACK, 0.35)));
            gr.stop(1.0, css1(col));
            g.fill_style(gr);
            g.begin_path();
            g.ellipse(cx, cy, rx, ry, 0.0, 0.0, TAU, false);
            g.fill();
            g.set_line_width(1.2);
            for k in 0..3 {
                let s = 1.0 - f64::from(k) * 0.27;
                g.stroke_style(css(
                    mix(col, WHITE, 0.25 + f64::from(k) * 0.25),
                    0.9 - f64::from(k) * 0.2,
                ));
                g.set_line_dash(&[5.0 * s, 4.0 * s]);
                g.set_dash_offset(
                    (if k % 2 != 0 { -1.0 } else { 1.0 }) * t * (18.0 + f64::from(k) * 8.0),
                );
                g.begin_path();
                g.ellipse(cx, cy, rx * s, ry * s, 0.0, 0.0, TAU, false);
                g.stroke();
            }
            g.set_line_dash(&[]);
            if rnd() < if opts.reduce_motion { 0.03 } else { 0.12 } {
                let ang = rnd() * TAU;
                self.parts.spawn(
                    PK_DOT,
                    C_PORTAL + pc,
                    cx + ang.cos() * rx * 1.3,
                    cy + ang.sin() * ry * 1.1,
                    -ang.cos() * 14.0,
                    -ang.sin() * 26.0,
                    0.6,
                    1.0,
                    0.0,
                    0.5,
                    true,
                    0.0,
                );
            }
        }
    }

    // ---- switches

    pub fn draw_plates(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64) {
        for (i, pl) in level.plates.iter().enumerate() {
            let r = pl.r;
            if !self.vis(r.x, r.y, r.w, r.h, 24.0) {
                continue;
            }
            let on = cur
                .plates
                .as_ref()
                .and_then(|v| v.get(i).copied())
                .unwrap_or_else(|| cur.on.contains(&pl.ch));
            let col = channel_color(&pl.ch);
            let floor = r.y + r.h;
            if pl.ceil {
                g.save();
                g.translate(0.0, 2.0 * r.y + r.h);
                g.scale(1.0, -1.0);
            }
            let h = if on { 1.0 } else { 3.0 };
            let metal = if self.pal.dark {
                mix(METAL, BLACK, 0.3)
            } else {
                METAL
            };
            if on {
                self.begin_glow(g);
                self.glow(
                    g,
                    r.x + r.w / 2.0,
                    floor - 2.0,
                    r.w * 0.6 + 8.0,
                    col,
                    0.55 + 0.08 * (t * 6.0).sin(),
                );
                self.end_glow(g);
            }
            g.fill_style(col);
            rr(g, r.x + 2.0, floor - 2.5 - h, r.w - 4.0, h + 1.0, 1.0);
            g.fill();
            g.fill_style(if on {
                Col::rgba(255.0, 255.0, 255.0, 0.6)
            } else {
                Col::rgba(255.0, 255.0, 255.0, 0.3)
            });
            g.fill_rect(r.x + 3.0, floor - 2.5 - h, r.w - 6.0, 0.8);
            g.fill_style(css1(metal));
            round_rect_path(g, r.x, floor - 2.5, r.w, 2.5, 1.2, 1.2, 0.0, 0.0);
            g.fill();
            g.fill_style(if on {
                col
            } else {
                css1(mix(metal, BLACK, 0.3))
            });
            g.fill_rect(r.x + 1.5, floor - 1.4, r.w - 3.0, 0.7);
            if pl.ceil {
                g.restore();
            }
        }
    }

    pub fn draw_buttons(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64) {
        for (i, b) in level.buttons.iter().enumerate() {
            let r = b.r;
            if !self.vis(r.x, r.y, r.w, r.h, 24.0) {
                continue;
            }
            let left = cur.buttons.get(i).copied().unwrap_or(0) as f64;
            let frac = if b.ticks > 0 {
                clamp(left / b.ticks as f64, 0.0, 1.0)
            } else {
                0.0
            };
            let on = left > 0.0;
            let col = channel_color(&b.ch);
            let cx = r.x + r.w / 2.0;
            let floor = r.y + r.h;
            if b.ceil {
                g.save();
                g.translate(0.0, 2.0 * r.y + r.h);
                g.scale(1.0, -1.0);
            }
            g.fill_style(css1(METAL));
            rr(g, r.x + 1.0, floor - 2.5, r.w - 2.0, 2.5, 1.0);
            g.fill();
            let dome = if on { 2.0 } else { 4.2 };
            g.fill_style(col);
            g.begin_path();
            g.ellipse(
                cx,
                floor - 2.5,
                (r.w / 2.0 - 3.0).min(7.0),
                dome,
                0.0,
                PI,
                0.0,
                false,
            );
            g.fill();
            g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.45));
            g.begin_path();
            g.ellipse(
                cx - 2.0,
                floor - 2.5 - dome * 0.55,
                2.0,
                dome * 0.25,
                0.0,
                0.0,
                TAU,
                false,
            );
            g.fill();
            if on {
                let ay = floor - 13.0;
                self.begin_glow(g);
                self.glow(g, cx, floor - 3.0, 14.0, col, 0.4);
                self.end_glow(g);
                g.set_line_cap(Cap::Round);
                g.stroke_style(css(self.pal.ink, 0.15));
                g.set_line_width(1.6);
                g.begin_path();
                g.arc(cx, ay, 4.0, 0.0, TAU, false);
                g.stroke();
                g.stroke_style(col);
                g.set_line_width(if frac < 0.25 && (t * 20.0).sin() > 0.0 {
                    2.4
                } else {
                    1.8
                });
                g.begin_path();
                g.arc(cx, ay, 4.0, -PI / 2.0, -PI / 2.0 + TAU * frac, false);
                g.stroke();
            }
            if b.ceil {
                g.restore();
            }
        }
    }

    pub fn draw_levers(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, dt: f64) {
        for (i, lv) in level.levers.iter().enumerate() {
            let on = cur.levers.get(i).copied().unwrap_or(lv.on);
            if let Some(a) = self.lever_ang.get_mut(i) {
                *a = approach(*a, if on { 1.0 } else { -1.0 }, 14.0, dt);
            }
            let r = lv.r;
            if !self.vis(r.x, r.y, r.w, r.h, 24.0) {
                continue;
            }
            let ang = self.lever_ang.get(i).copied().unwrap_or(0.0) * 0.65;
            let col = channel_color(&lv.ch);
            let cx = r.x + r.w / 2.0;
            let by = r.y + r.h - 1.0;
            let (hx, hy) = (cx + ang.sin() * 12.0, by - 3.0 - ang.cos() * 12.0);
            g.set_line_cap(Cap::Round);
            g.stroke_style(css1(if self.pal.dark {
                mix(METAL, WHITE, 0.2)
            } else {
                mix(METAL, BLACK, 0.15)
            }));
            g.set_line_width(2.0);
            g.begin_path();
            g.move_to(cx, by - 3.0);
            g.line_to(hx, hy);
            g.stroke();
            g.fill_style(col);
            g.begin_path();
            g.arc(hx, hy, 2.6, 0.0, TAU, false);
            g.fill();
            g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.55));
            g.begin_path();
            g.arc(hx - 0.8, hy - 0.8, 0.9, 0.0, TAU, false);
            g.fill();
            g.fill_style(css1(if self.pal.dark {
                mix(METAL, BLACK, 0.3)
            } else {
                METAL
            }));
            g.begin_path();
            g.ellipse(cx, by, 6.5, 5.0, 0.0, PI, 0.0, false);
            g.fill();
            g.fill_style(if on { col } else { css1(self.pal.wall_lo) });
            g.begin_path();
            g.arc(cx, by - 2.0, 1.4, 0.0, TAU, false);
            g.fill();
            if on {
                self.begin_glow(g);
                self.glow(g, cx, by - 2.0, 8.0, col, 0.6);
                self.end_glow(g);
            }
        }
    }

    pub fn draw_sensors(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64) {
        for (i, s) in level.sensors.iter().enumerate() {
            let r = s.r;
            if !self.vis(r.x, r.y, r.w, r.h, 24.0) {
                continue;
            }
            let lit = cur
                .sensors
                .as_ref()
                .and_then(|v| v.get(i).copied())
                .unwrap_or_else(|| cur.on.contains(&s.ch));
            let col = channel_color(&s.ch);
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            g.fill_style(css1(if self.pal.dark {
                mix(METAL, BLACK, 0.35)
            } else {
                METAL
            }));
            rr(g, r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, 4.0);
            g.fill();
            g.stroke_style(col);
            g.set_line_width(1.2);
            g.stroke();
            g.fill_style(if lit {
                css1(mix(rgb_of(col), WHITE, 0.5))
            } else {
                css(BLACK, 0.55)
            });
            g.begin_path();
            g.arc(cx, cy, 4.0, 0.0, TAU, false);
            g.fill();
            g.fill_style(col);
            g.begin_path();
            g.arc(cx, cy, if lit { 2.4 } else { 1.4 }, 0.0, TAU, false);
            g.fill();
            if lit {
                self.begin_glow(g);
                self.glow(g, cx, cy, 16.0, col, 0.7 + 0.15 * (t * 8.0).sin());
                self.end_glow(g);
            }
        }
    }

    pub fn draw_emitters(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64) {
        for em in &level.emitters {
            if !self.vis(em.x - 10.0, em.y - 10.0, 20.0, 20.0, 24.0) {
                continue;
            }
            let on = cond_holds(em.when.as_ref(), &cur.on, cur.lit);
            let ch = cond_channels(em.when.as_ref()).first().cloned();
            let rot = match em.dir {
                Dir::Right => 0.0,
                Dir::Down => PI / 2.0,
                Dir::Left => PI,
                Dir::Up => -PI / 2.0,
            };
            g.save();
            g.translate(em.x, em.y);
            g.rotate(rot);
            g.fill_style(css1(if self.pal.dark {
                mix(METAL, BLACK, 0.3)
            } else {
                METAL
            }));
            rr(g, -8.0, -7.0, 13.0, 14.0, 3.0);
            g.fill();
            if let Some(ch) = &ch {
                g.fill_style(channel_color(ch));
                g.fill_rect(-8.0, -7.0, 2.5, 14.0);
            }
            g.fill_style(css1(if self.pal.dark {
                mix(METAL, WHITE, 0.15)
            } else {
                mix(METAL, BLACK, 0.2)
            }));
            g.fill_rect(5.0, -4.0, 3.5, 8.0);
            g.fill_style(if on {
                self.colors[C_CORE]
            } else {
                css(BLACK, 0.5)
            });
            g.begin_path();
            g.arc(8.5, 0.0, 2.6, -PI / 2.0, PI / 2.0, false);
            g.fill();
            g.restore();
            if on {
                let dx = match em.dir {
                    Dir::Right => 8.0,
                    Dir::Left => -8.0,
                    _ => 0.0,
                };
                let dy = match em.dir {
                    Dir::Down => 8.0,
                    Dir::Up => -8.0,
                    _ => 0.0,
                };
                self.begin_glow(g);
                self.glow(
                    g,
                    em.x + dx,
                    em.y + dy,
                    12.0,
                    css1(self.pal.star),
                    0.7 + 0.1 * (t * 14.0).sin(),
                );
                self.end_glow(g);
            }
        }
    }

    pub fn draw_mirrors(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, dt: f64) {
        for (i, m) in level.mirrors.iter().enumerate() {
            let slant = cur.mirrors.get(i).copied().unwrap_or(m.slant);
            if let Some(a) = self.mirror_ang.get_mut(i) {
                *a = approach(
                    *a,
                    if slant == Slant::Slash {
                        -PI / 4.0
                    } else {
                        PI / 4.0
                    },
                    16.0,
                    dt,
                );
            }
            if !self.vis(m.x - 12.0, m.y - 12.0, 24.0, 24.0, 24.0) {
                continue;
            }
            g.fill_style(css1(if self.pal.dark {
                mix(METAL, BLACK, 0.3)
            } else {
                METAL
            }));
            g.fill_rect(m.x - 1.0, m.y, 2.0, T / 2.0);
            g.begin_path();
            g.arc(m.x, m.y, 2.4, 0.0, TAU, false);
            g.fill();
            g.save();
            g.translate(m.x, m.y);
            g.rotate(self.mirror_ang.get(i).copied().unwrap_or(0.0));
            g.fill_style(css1(if self.pal.dark {
                mix(METAL, BLACK, 0.45)
            } else {
                mix(METAL, BLACK, 0.25)
            }));
            rr(g, -12.0, -2.2, 24.0, 4.4, 2.0);
            g.fill();
            let mut sheen = Gradient::linear(-12.0, 0.0, 12.0, 0.0);
            sheen.stop(0.0, Col::rgba(210.0, 230.0, 255.0, 0.95));
            sheen.stop(0.5, Col::rgba(255.0, 255.0, 255.0, 1.0));
            sheen.stop(1.0, Col::rgba(180.0, 205.0, 240.0, 0.95));
            g.fill_style(sheen);
            rr(g, -11.0, -2.2, 22.0, 2.2, 1.1);
            g.fill();
            g.restore();
        }
    }

    // ---- solid moving things

    pub fn draw_gates(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState) {
        let p = self.pal;
        for (i, home_def) in level.gates.iter().enumerate() {
            let home = home_def.r;
            if !self.vis(home.x, home.y, home.w, home.h, 4.0) {
                continue;
            }
            let r = gate_rect(level, cur, i);
            let chs = cond_channels(Some(&home_def.open));
            g.save();
            g.begin_path();
            g.rect(home.x, home.y, home.w, home.h);
            g.clip();
            let bar = if p.dark {
                mix(METAL, BLACK, 0.15)
            } else {
                METAL
            };
            g.fill_style(css1(mix(bar, BLACK, 0.35)));
            rr(g, r.x, r.y, r.w, r.h, 2.0);
            g.fill();
            let vertical = r.h >= r.w;
            g.fill_style(css1(bar));
            if vertical {
                let mut bx = r.x + 2.0;
                while bx < r.x + r.w - 1.0 {
                    g.fill_rect(bx, r.y + 1.0, 2.5, r.h - 2.0);
                    bx += 5.0;
                }
                g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.28));
                bx = r.x + 2.0;
                while bx < r.x + r.w - 1.0 {
                    g.fill_rect(bx, r.y + 1.0, 0.8, r.h - 2.0);
                    bx += 5.0;
                }
            } else {
                let mut by = r.y + 2.0;
                while by < r.y + r.h - 1.0 {
                    g.fill_rect(r.x + 1.0, by, r.w - 2.0, 2.5);
                    by += 5.0;
                }
                g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.28));
                by = r.y + 2.0;
                while by < r.y + r.h - 1.0 {
                    g.fill_rect(r.x + 1.0, by, r.w - 2.0, 0.8);
                    by += 5.0;
                }
            }
            let n = chs.len().max(1);
            for c in 0..n {
                g.fill_style(if c < chs.len() {
                    channel_color(&chs[c])
                } else {
                    css1(p.muted)
                });
                if vertical {
                    g.fill_rect(r.x, r.y + r.h - 7.0 - c as f64 * 4.0, r.w, 3.0);
                } else {
                    g.fill_rect(r.x + r.w - 7.0 - c as f64 * 4.0, r.y, 3.0, r.h);
                }
            }
            g.stroke_style(css(BLACK, if p.dark { 0.45 } else { 0.25 }));
            g.set_line_width(0.8);
            rr(g, r.x + 0.4, r.y + 0.4, r.w - 0.8, r.h - 0.8, 2.0);
            g.stroke();
            g.restore();
        }
    }

    pub fn draw_mover_rails(&mut self, g: &mut Canvas, level: &Stage) {
        g.set_line_cap(Cap::Round);
        for m in &level.movers {
            let r = m.r;
            if !self.vis(
                r.x.min(m.to.x),
                r.y.min(m.to.y),
                (m.to.x - r.x).abs() + r.w,
                (m.to.y - r.y).abs() + r.h,
                4.0,
            ) {
                continue;
            }
            let ch = cond_channels(m.when.as_ref()).first().cloned();
            let col = ch.as_deref().map_or(css1(self.pal.muted), channel_color);
            g.stroke_style(col);
            g.set_alpha(0.45);
            g.set_line_width(1.5);
            g.set_line_dash(&[0.1, 5.0]);
            g.begin_path();
            g.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
            g.line_to(m.to.x + r.w / 2.0, m.to.y + r.h / 2.0);
            g.stroke();
            g.set_line_dash(&[]);
            g.fill_style(col);
            g.begin_path();
            g.arc(r.x + r.w / 2.0, r.y + r.h / 2.0, 1.6, 0.0, TAU, false);
            g.arc(m.to.x + r.w / 2.0, m.to.y + r.h / 2.0, 1.6, 0.0, TAU, false);
            g.fill();
            g.set_alpha(1.0);
        }
    }

    pub fn draw_movers(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        prev: Option<&GameState>,
        cur: &GameState,
        a: f64,
    ) {
        let p = self.pal;
        for (i, m) in level.movers.iter().enumerate() {
            let Some(c) = cur.movers.get(i) else { continue };
            let q = prev.and_then(|s| s.movers.get(i));
            let (x, y) = match q {
                Some(q) if (q.x - c.x).abs() <= 40.0 && (q.y - c.y).abs() <= 40.0 => {
                    (q.x + (c.x - q.x) * a, q.y + (c.y - q.y) * a)
                }
                _ => (c.x, c.y),
            };
            let (mw, mh) = (m.r.w, m.r.h);
            if !self.vis(x, y, mw, mh, 4.0) {
                continue;
            }
            let ch = cond_channels(m.when.as_ref()).first().cloned();
            let body = if p.dark {
                mix(p.wall, METAL, 0.5)
            } else {
                mix(p.wall, METAL, 0.45)
            };
            g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.1 }));
            rr(g, x + 1.0, y + 2.0, mw, mh, 3.0);
            g.fill();
            g.fill_style(css1(body));
            rr(g, x, y, mw, mh, 3.0);
            g.fill();
            g.fill_style(css1(if p.dark {
                mix(body, WHITE, 0.2)
            } else {
                mix(body, WHITE, 0.5)
            }));
            g.fill_rect(x + 1.0, y, mw - 2.0, 2.0);
            g.fill_style(ch.as_deref().map_or(css1(p.muted), channel_color));
            g.fill_rect(x + 3.0, y + 3.0, mw - 6.0, 2.0);
            g.fill_style(css1(mix(body, BLACK, 0.35)));
            let mut rx = x + 4.0;
            while rx < x + mw - 2.0 {
                g.begin_path();
                g.arc(rx, y + mh - 4.0, 0.9, 0.0, TAU, false);
                g.fill();
                rx += 10.0;
            }
        }
    }

    pub fn draw_boxes(
        &mut self,
        g: &mut Canvas,
        prev: Option<&GameState>,
        cur: &GameState,
        a: f64,
    ) {
        let p = self.pal;
        let wood = if p.dark { mix(WOOD, BLACK, 0.1) } else { WOOD };
        for (i, c) in cur.boxes.iter().enumerate() {
            let q = prev.and_then(|s| s.boxes.get(i));
            let (x, y) = match q {
                Some(q) if (q.x - c.x).abs() <= 40.0 && (q.y - c.y).abs() <= 40.0 => {
                    (q.x + (c.x - q.x) * a, q.y + (c.y - q.y) * a)
                }
                _ => (c.x, c.y),
            };
            let s = T;
            if !self.vis(x, y, s, s, 4.0) {
                continue;
            }
            g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.1 }));
            rr(g, x + 1.0, y + 2.0, s, s, 2.0);
            g.fill();
            g.fill_style(css1(wood));
            rr(g, x, y, s, s, 2.0);
            g.fill();
            g.stroke_style(css(WOOD_DARK, 0.7));
            g.set_line_width(0.8);
            g.begin_path();
            g.move_to(x + 2.0, y + s / 3.0);
            g.line_to(x + s - 2.0, y + s / 3.0);
            g.move_to(x + 2.0, y + 2.0 * s / 3.0);
            g.line_to(x + s - 2.0, y + 2.0 * s / 3.0);
            g.stroke();
            g.stroke_style(css1(mix(wood, WHITE, 0.15)));
            g.set_line_width(2.0);
            g.begin_path();
            g.move_to(x + 3.0, y + s - 3.0);
            g.line_to(x + s - 3.0, y + 3.0);
            g.stroke();
            g.stroke_style(css1(WOOD_DARK));
            g.set_line_width(1.4);
            rr(g, x + 0.7, y + 0.7, s - 1.4, s - 1.4, 2.0);
            g.stroke();
            g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.25));
            g.fill_rect(x + 1.5, y + 1.2, s - 3.0, 1.0);
        }
    }

    pub fn draw_ice(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64) {
        for (i, r) in level.ice.iter().enumerate() {
            let p = cur.ice.get(i).copied().unwrap_or(0.0);
            let melting = p > self.ice_last.get(i).copied().unwrap_or(0.0);
            if let Some(l) = self.ice_last.get_mut(i) {
                *l = p;
            }
            if p >= 1.0 || !self.vis(r.x, r.y, r.w, r.h, 4.0) {
                continue;
            }
            let jx = if melting {
                (t * 60.0 + i as f64).sin() * 0.6
            } else {
                0.0
            };
            let x = r.x + jx;
            g.save();
            g.set_alpha(0.92 - p * 0.35);
            let mut gr = Gradient::linear(x, r.y, x + r.w, r.y + r.h);
            gr.stop(0.0, css(ICE_TOP, 0.92));
            gr.stop(1.0, css(mix(ICE_BOT, self.pal.frost, 0.3), 0.85));
            g.fill_style(gr);
            rr(g, x, r.y, r.w, r.h, 2.5);
            g.fill();
            g.stroke_style(Col::rgba(255.0, 255.0, 255.0, 0.55));
            g.set_line_width(0.7);
            g.begin_path();
            let mut ty = r.y + T;
            while ty < r.y + r.h {
                g.move_to(x + 1.0, ty);
                g.line_to(x + r.w - 1.0, ty);
                ty += T;
            }
            let mut tx = x + T;
            while tx < x + r.w {
                g.move_to(tx, r.y + 1.0);
                g.line_to(tx, r.y + r.h - 1.0);
                tx += T;
            }
            g.stroke();
            g.stroke_style(Col::rgba(255.0, 255.0, 255.0, 0.85));
            g.set_line_width(1.2);
            g.set_line_cap(Cap::Round);
            g.begin_path();
            g.move_to(x + 3.0, r.y + 7.0);
            g.line_to(x + 7.0, r.y + 3.0);
            g.move_to(x + 3.0, r.y + 11.0);
            g.line_to(x + 5.0, r.y + 9.0);
            g.stroke();
            let cracks = (p * 7.0).ceil() as usize;
            if cracks > 0 {
                g.stroke_style(css(self.pal.frost, 0.85));
                g.set_line_width(0.8);
                g.begin_path();
                for c in 0..cracks {
                    let mut cx = x + hash01((i * 41 + c) as f64) * r.w;
                    let mut cy = r.y + hash01((i * 43 + c + 7) as f64) * r.h;
                    g.move_to(cx, cy);
                    for s in 0..3 {
                        cx += (hash01((i * 47 + c * 5 + s) as f64) - 0.5) * 10.0;
                        cy += (hash01((i * 53 + c * 5 + s) as f64) - 0.5) * 10.0;
                        g.line_to(
                            clamp(cx, x + 1.0, x + r.w - 1.0),
                            clamp(cy, r.y + 1.0, r.y + r.h - 1.0),
                        );
                    }
                }
                g.stroke();
            }
            g.stroke_style(css(self.pal.frost, 0.6));
            g.set_line_width(1.0);
            rr(g, x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0, 2.5);
            g.stroke();
            g.restore();
            if melting && rnd() < 0.3 {
                self.parts.spawn(
                    PK_DOT,
                    C_WATER,
                    x + rand(1.0, r.w - 1.0),
                    r.y + r.h - 1.0,
                    0.0,
                    rand(10.0, 30.0),
                    0.4,
                    1.0,
                    300.0,
                    0.0,
                    false,
                    0.0,
                );
            }
        }
    }

    // ---- version 2's tiles

    pub fn draw_tiles(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        opts: DrawOpts,
    ) {
        let tiles = &level.tiles;
        let vt = visible_tiles(&self.vr, level.cols, level.rows, 1);
        let p = self.pal;
        let lit = 1.0 - self.dark_a;
        for ty in vt.y0..vt.y1.max(vt.y0) {
            for tx in vt.x0..vt.x1.max(vt.x0) {
                let i = ty as usize * level.cols + tx as usize;
                let kind = tiles[i];
                if kind < T_TRAMP {
                    continue;
                }
                let (x, y) = (tx as f64 * T, ty as f64 * T);
                if kind == T_TRAMP {
                    self.draw_tramp(g, level, i, tx, ty, t);
                } else if kind == T_ONEWAY {
                    let l = tx > 0 && tiles[i - 1] != 0 && tiles[i - 1] != T_CRUMBLE;
                    let r = tx < level.cols as i64 - 1
                        && tiles[i + 1] != 0
                        && tiles[i + 1] != T_CRUMBLE;
                    let h = ONEWAY_H;
                    let wood = if p.dark {
                        mix(WOOD, p.wall, 0.25)
                    } else {
                        mix(WOOD, p.wall_hi, 0.2)
                    };
                    g.fill_style(css(BLACK, if p.dark { 0.28 } else { 0.1 }));
                    g.fill_rect(x + 1.0, y + 2.0, T, h);
                    g.fill_style(css1(wood));
                    let k = |on: bool| if on { 0.0 } else { 2.5 };
                    round_rect_path(
                        g,
                        x,
                        y,
                        T + if r { 0.4 } else { 0.0 },
                        h,
                        k(l),
                        k(r),
                        k(r),
                        k(l),
                    );
                    g.fill();
                    let inset_l = if l { 0.0 } else { 1.5 };
                    let inset_r = if r { 0.0 } else { 1.5 };
                    g.fill_style(css1(mix(wood, WHITE, if p.dark { 0.35 } else { 0.5 })));
                    g.fill_rect(x + inset_l, y, T - inset_l - inset_r, 1.4);
                    g.fill_style(css(WOOD_DARK, 0.7));
                    g.fill_rect(x + inset_l, y + h - 1.2, T - inset_l - inset_r, 1.2);
                    if !self.far {
                        g.stroke_style(css(p.ink, 0.2));
                        g.set_line_width(1.0);
                        g.begin_path();
                        g.move_to(x + 7.0, y + 12.5);
                        g.line_to(x + 10.0, y + 9.5);
                        g.line_to(x + 13.0, y + 12.5);
                        g.stroke();
                    }
                } else if kind == T_CRUMBLE {
                    self.draw_crumble(g, cur, i, x, y, t, opts);
                } else {
                    let sun = kind == T_LIGHT;
                    let a = if sun { lit } else { 1.0 - lit };
                    let col = if sun { SUN } else { MOON };
                    let (cx, cy) = (x + T / 2.0, y + T / 2.0);
                    if a < 0.97 {
                        g.set_alpha(1.0 - a);
                        g.fill_style(css(col, if p.dark { 0.07 } else { 0.1 }));
                        rr(g, x + 1.5, y + 1.5, T - 3.0, T - 3.0, 3.0);
                        g.fill();
                        g.stroke_style(css(if p.dark { col } else { mix(col, BLACK, 0.3) }, 0.75));
                        g.set_line_width(1.0);
                        g.set_line_dash(&[3.0, 2.5]);
                        g.stroke();
                        g.set_line_dash(&[]);
                        if !self.far {
                            light_glyph(
                                g,
                                sun,
                                cx,
                                cy,
                                css(if p.dark { col } else { mix(col, BLACK, 0.3) }, 0.6),
                            );
                        }
                        g.set_alpha(1.0);
                    }
                    if a > 0.03 {
                        let k = 0.8 + 0.2 * a;
                        g.save();
                        g.translate(cx, cy);
                        g.scale(k, k);
                        g.set_alpha(a);
                        g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.1 }));
                        rr(g, -T / 2.0 + 1.0, -T / 2.0 + 2.0, T, T, 3.0);
                        g.fill();
                        let body = if sun {
                            mix(SUN, WHITE, 0.35)
                        } else if p.dark {
                            mix(MOON, BLACK, 0.45)
                        } else {
                            mix(MOON, BLACK, 0.3)
                        };
                        g.fill_style(css1(body));
                        rr(g, -T / 2.0, -T / 2.0, T, T, 3.0);
                        g.fill();
                        g.fill_style(css(WHITE, if sun { 0.7 } else { 0.22 }));
                        g.fill_rect(-T / 2.0 + 2.0, -T / 2.0, T - 4.0, 2.2);
                        g.fill_style(css(
                            if sun { mix(SUN, LAVA_MID, 0.5) } else { BLACK },
                            if sun { 0.55 } else { 0.35 },
                        ));
                        g.fill_rect(-T / 2.0 + 2.0, T / 2.0 - 2.2, T - 4.0, 2.2);
                        g.stroke_style(css(
                            if sun {
                                mix(SUN, LAVA_MID, 0.55)
                            } else {
                                mix(MOON, WHITE, 0.3)
                            },
                            0.9,
                        ));
                        g.set_line_width(1.0);
                        rr(g, -T / 2.0 + 0.5, -T / 2.0 + 0.5, T - 1.0, T - 1.0, 3.0);
                        g.stroke();
                        if !self.far {
                            light_glyph(
                                g,
                                sun,
                                0.0,
                                0.0,
                                if sun {
                                    css1(mix(SUN, LAVA_BOT, 0.55))
                                } else {
                                    css1(mix(MOON, WHITE, 0.6))
                                },
                            );
                        }
                        g.restore();
                        g.set_alpha(1.0);
                    }
                }
            }
        }
    }

    fn draw_tramp(&mut self, g: &mut Canvas, level: &Stage, i: usize, tx: i64, ty: i64, t: f64) {
        let p = self.pal;
        let (x, y) = (tx as f64 * T, ty as f64 * T);
        let mut d = 0.0;
        if let Some(&at) = self.tramp_at.get(&i) {
            let e = t - at;
            if e > 0.6 {
                self.tramp_at.remove(&i);
            } else {
                d = 4.5 * (-e * 7.0).exp() * (e * 34.0).cos();
            }
        }
        let l = tx > 0 && level.tiles[i - 1] == T_TRAMP;
        let r = tx < level.cols as i64 - 1 && level.tiles[i + 1] == T_TRAMP;
        let base = if p.dark {
            mix(METAL, BLACK, 0.45)
        } else {
            mix(METAL, BLACK, 0.2)
        };
        g.fill_style(css1(base));
        round_rect_path(
            g,
            x,
            y + 5.0,
            T + if r { 0.4 } else { 0.0 },
            T - 5.0,
            0.0,
            0.0,
            if r { 0.0 } else { 3.0 },
            if l { 0.0 } else { 3.0 },
        );
        g.fill();
        g.fill_style(css(BLACK, 0.4));
        g.fill_rect(x + 4.0, y + 7.0, T - 8.0, 9.0);
        g.stroke_style(css1(if p.dark {
            mix(METAL, WHITE, 0.4)
        } else {
            mix(METAL, WHITE, 0.6)
        }));
        g.set_line_width(1.2);
        g.begin_path();
        let top = y + 6.0 + d.max(-1.0);
        for k in 0..=4 {
            let yy = top + (y + 16.0 - top) * f64::from(k) / 4.0;
            let xx = x + if k % 2 != 0 { 14.5 } else { 5.5 };
            if k == 0 {
                g.move_to(xx, yy);
            } else {
                g.line_to(xx, yy);
            }
        }
        g.stroke();
        let pad = |g: &mut Canvas, py: f64, flip: bool| {
            let h = 5.0;
            g.fill_style(css1(TRAMP));
            let k = |off: bool| if off { 0.0 } else { 2.5 };
            round_rect_path(
                g,
                x - if l { 0.0 } else { 0.5 },
                py,
                T + if l { 0.0 } else { 0.5 } + if r { 0.4 } else { 0.5 },
                h,
                k(l || flip),
                k(r || flip),
                k(r || !flip),
                k(l || !flip),
            );
            g.fill();
            g.fill_style(css(WHITE, 0.85));
            g.fill_rect(x + 2.0, if flip { py + h - 1.6 } else { py }, 5.0, 1.6);
            g.fill_rect(x + 13.0, if flip { py + h - 1.6 } else { py }, 5.0, 1.6);
            g.fill_style(css(mix(TRAMP, BLACK, 0.4), 0.8));
            g.fill_rect(x, if flip { py } else { py + h - 1.2 }, T, 1.2);
        };
        pad(g, y + d.max(-1.5), false);
        if ty < level.rows as i64 - 1 && level.tiles[i + level.cols] == 0 {
            pad(g, y + T - 5.0 - d.max(-1.5) * 0.6, true);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_crumble(
        &mut self,
        g: &mut Canvas,
        cur: &GameState,
        i: usize,
        x: f64,
        y: f64,
        t: f64,
        opts: DrawOpts,
    ) {
        let p = self.pal;
        let c = self
            .crumble_index
            .get(&i)
            .and_then(|&j| cur.crumble.get(j).copied())
            .unwrap_or(0) as f64;
        let sand = if p.dark {
            mix(SAND, p.wall, 0.45)
        } else {
            mix(SAND, p.wall, 0.3)
        };
        if c > CRUMBLE_TICKS as f64 {
            let back = clamp(
                (c - CRUMBLE_TICKS as f64) / CRUMBLE_GONE_TICKS as f64,
                0.0,
                1.0,
            );
            g.stroke_style(css(sand, 0.6));
            g.set_line_width(1.0);
            g.set_line_dash(&[1.5, 3.0]);
            rr(g, x + 1.5, y + 1.5, T - 3.0, T - 3.0, 3.0);
            g.stroke();
            g.set_line_dash(&[]);
            let s = (T - 6.0) * back;
            g.fill_style(css(sand, 0.1 + 0.16 * back));
            rr(g, x + T / 2.0 - s / 2.0, y + T / 2.0 - s / 2.0, s, s, 2.0);
            g.fill();
            return;
        }
        let shake = if c > 0.0 {
            c / CRUMBLE_TICKS as f64
        } else {
            0.0
        };
        let jx = if shake != 0.0 && !opts.reduce_motion {
            (t * 70.0 + i as f64).sin() * (0.4 + shake * 1.1)
        } else {
            0.0
        };
        let jy = if shake != 0.0 && !opts.reduce_motion {
            (t * 57.0 + (i * 3) as f64).cos() * shake * 0.7
        } else {
            0.0
        };
        let mut k = 1.0;
        if let Some(&at) = self.mend_at.get(&i) {
            let e = (t - at) / 0.22;
            if e >= 1.0 {
                self.mend_at.remove(&i);
            } else {
                k = 0.55 + 0.45 * e * (2.0 - e);
            }
        }
        g.save();
        g.translate(x + T / 2.0 + jx, y + T / 2.0 + jy);
        g.scale(k, k);
        g.translate(-T / 2.0, -T / 2.0);
        g.set_alpha(if k < 1.0 { k } else { 1.0 });
        g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.1 }));
        rr(g, 1.0, 2.0, T, T, 3.0);
        g.fill();
        g.fill_style(css1(sand));
        rr(g, 0.0, 0.0, T, T, 3.0);
        g.fill();
        g.fill_style(css1(mix(sand, WHITE, if p.dark { 0.3 } else { 0.5 })));
        g.fill_rect(2.0, 0.0, T - 4.0, 2.2);
        g.fill_style(css(mix(sand, BLACK, 0.35), 0.7));
        g.fill_rect(2.0, T - 2.2, T - 4.0, 2.2);
        let h = hash01(i as f64);
        g.stroke_style(css(mix(sand, BLACK, 0.55), 0.85));
        g.set_line_width(0.9 + shake * 0.7);
        g.set_line_join(Join::Round);
        g.begin_path();
        g.move_to(4.0 + h * 5.0, 2.0);
        g.line_to(8.0 + h * 3.0, 7.0);
        g.line_to(6.0 + h * 4.0, 11.0);
        g.move_to(15.0 - h * 3.0, T - 2.0);
        g.line_to(13.0 - h * 2.0, 13.0);
        if shake > 0.01 {
            g.line_to(10.0 + h * 2.0, 10.0);
            g.line_to(6.0 + h * 4.0, 11.0);
            g.move_to(8.0 + h * 3.0, 7.0);
            g.line_to(14.0, 5.0 + h * 2.0);
            if shake > 0.5 {
                g.line_to(T - 2.0, 7.0);
                g.move_to(6.0 + h * 4.0, 11.0);
                g.line_to(3.0, 15.0);
                g.line_to(5.0, T - 2.0);
            }
        }
        g.stroke();
        g.restore();
        g.set_alpha(1.0);
        if shake > 0.0 && rnd() < if opts.reduce_motion { 0.06 } else { 0.3 } {
            self.parts.spawn(
                PK_DOT,
                C_SAND,
                x + rand(2.0, T - 2.0),
                y + T - 1.0,
                rand(-8.0, 8.0),
                rand(10.0, 40.0),
                0.4,
                rand(0.9, 1.5),
                300.0,
                0.5,
                false,
                0.0,
            );
        }
    }

    // ---- version 2's things

    pub fn draw_checkpoints(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        dt: f64,
    ) {
        let p = self.pal;
        for (i, c) in level.checkpoints.iter().enumerate() {
            let taken = i as i64 == cur.cp;
            if let Some(u) = self.cp_up.get_mut(i) {
                *u = approach(*u, if taken { 1.0 } else { 0.0 }, 9.0, dt);
            }
            if !self.vis(c.x - 14.0, c.y - 30.0, 30.0, 32.0, 24.0) {
                continue;
            }
            let up = self.cp_up.get(i).copied().unwrap_or(0.0);
            let top = c.y - 27.0;
            let e = t - self.cp_at.get(i).copied().unwrap_or(-9.0);
            let pop = if (0.0..0.7).contains(&e) {
                (-e * 6.0).exp() * (e * 20.0).sin()
            } else {
                0.0
            };
            if up > 0.02 {
                self.begin_glow(g);
                self.glow(
                    g,
                    c.x + 5.0,
                    top + 6.0,
                    20.0 + pop * 6.0,
                    css1(p.star),
                    (0.5 + 0.08 * (t * 3.0).sin()) * up,
                );
                self.end_glow(g);
            }
            g.fill_style(css1(if p.dark {
                mix(METAL, BLACK, 0.3)
            } else {
                METAL
            }));
            g.begin_path();
            g.ellipse(c.x, c.y, 5.0, 2.2, 0.0, PI, 0.0, false);
            g.fill();
            g.fill_rect(c.x - 0.9, top, 1.8, 27.0);
            g.fill_style(css1(if up > 0.5 { p.star } else { p.muted }));
            g.begin_path();
            g.arc(c.x, top - 0.5, 1.9, 0.0, TAU, false);
            g.fill();
            let len = 5.0 + 9.0 * up + pop * 4.0;
            let hy = top + 2.0 + (1.0 - up) * 3.0;
            let hgt = 9.0;
            let wave = |u: f64| (t * 5.5 - u * 3.2 + i as f64).sin() * 1.5 * u * up;
            let droop = |u: f64| (1.0 - up) * u * 7.0;
            for half in 0..2 {
                let hf = f64::from(half);
                let y0 = hy + hf * hgt / 2.0;
                let y1 = y0 + hgt / 2.0;
                g.fill_style(if up > 0.5 {
                    css1(if half != 0 { p.frost } else { p.fire })
                } else {
                    css(p.muted, 0.75)
                });
                g.begin_path();
                g.move_to(c.x + 0.9, y0);
                let mut u = 0.25;
                while u <= 1.001 {
                    g.line_to(
                        c.x + 0.9 + len * u,
                        y0 + (hgt / 2.0 - hf * (hgt / 2.0)) * u * if half != 0 { 0.0 } else { 1.0 }
                            + wave(u)
                            + droop(u),
                    );
                    u += 0.25;
                }
                u = 1.0;
                while u >= -0.001 {
                    g.line_to(
                        c.x + 0.9 + len * u,
                        y1 - if half != 0 { (hgt / 2.0) * u } else { 0.0 } + wave(u) + droop(u),
                    );
                    u -= 0.25;
                }
                g.close_path();
                g.fill();
            }
            if !taken && cur.phase == Phase::Play {
                for k in 0..2 {
                    let pl = cur.players[k];
                    let z = c.zone;
                    let inside = pl.alive
                        && pl.x < z.x + z.w
                        && pl.x + PLAYER_W > z.x
                        && pl.y < z.y + z.h
                        && pl.y + PLAYER_H > z.y;
                    g.fill_style(if inside {
                        css1(if k == 0 { p.fire } else { p.frost })
                    } else {
                        css(p.ink, 0.22)
                    });
                    g.begin_path();
                    g.arc(
                        c.x + if k != 0 { 4.2 } else { -4.2 },
                        c.y - 4.5,
                        if inside { 1.9 } else { 1.4 },
                        0.0,
                        TAU,
                        false,
                    );
                    g.fill();
                }
            }
        }
    }

    pub fn draw_lamps(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64, dt: f64) {
        let p = self.pal;
        for (i, l) in level.lamps.iter().enumerate() {
            if let Some(o) = self.lamp_on.get_mut(i) {
                *o = approach(*o, if cur.lit { 1.0 } else { 0.0 }, 18.0, dt);
            }
            let r = l.r;
            if !self.vis(r.x, r.y, r.w, r.h, 30.0) {
                continue;
            }
            let on = self.lamp_on.get(i).copied().unwrap_or(0.0);
            let cx = r.x + r.w / 2.0;
            let by = r.y + r.h;
            let hy = r.y + 6.5;
            let metal = if p.dark {
                mix(METAL, BLACK, 0.3)
            } else {
                METAL
            };
            self.begin_glow(g);
            self.glow(
                g,
                cx,
                hy,
                13.0 + on * 13.0,
                css1(SUN),
                0.22 + on * 0.5 + 0.04 * (t * 5.0 + i as f64).sin(),
            );
            self.end_glow(g);
            g.fill_style(css1(metal));
            g.fill_rect(cx - 1.0, hy + 4.0, 2.0, by - hy - 4.0);
            g.begin_path();
            g.ellipse(cx, by, 5.0, 2.2, 0.0, PI, 0.0, false);
            g.fill();
            rr(g, cx - 4.0, hy + 3.4, 8.0, 2.2, 1.0);
            g.fill();
            g.fill_style(css1(mix(mix(SUN, BLACK, 0.72), mix(SUN, WHITE, 0.55), on)));
            g.begin_path();
            g.arc(cx, hy, 4.6, 0.0, TAU, false);
            g.fill();
            g.fill_style(css(mix(LAVA_MID, WHITE, on), 0.9));
            g.begin_path();
            g.arc(cx, hy + 0.4, 1.4 + on * 0.8, 0.0, TAU, false);
            g.fill();
            g.stroke_style(css1(metal));
            g.set_line_width(0.9);
            g.begin_path();
            g.arc(cx, hy, 4.9, 0.0, TAU, false);
            g.move_to(cx, hy - 4.9);
            g.line_to(cx, hy + 4.9);
            g.stroke();
            g.stroke_style(css(p.ink, 0.4));
            g.set_line_width(0.7);
            g.begin_path();
            g.move_to(cx + 5.2, hy + 3.0);
            g.line_to(cx + 5.2, hy + 8.0 + on);
            g.stroke();
            g.fill_style(css1(SUN));
            g.begin_path();
            g.arc(cx + 5.2, hy + 8.6 + on, 1.1, 0.0, TAU, false);
            g.fill();
            if l.ticks > 0 {
                let frac = if cur.lit_t > 0 {
                    clamp(cur.lit_t as f64 / l.ticks as f64, 0.0, 1.0)
                } else {
                    0.0
                };
                g.set_line_cap(Cap::Round);
                g.stroke_style(css(p.ink, 0.18));
                g.set_line_width(1.5);
                g.set_line_dash(&[1.2, 2.4]);
                g.begin_path();
                g.arc(cx, hy, 7.6, 0.0, TAU, false);
                g.stroke();
                g.set_line_dash(&[]);
                if frac > 0.0 {
                    g.stroke_style(css1(if frac < 0.25 && (t * 20.0).sin() > 0.0 {
                        WHITE
                    } else {
                        SUN
                    }));
                    g.set_line_width(1.9);
                    g.begin_path();
                    g.arc(cx, hy, 7.6, -PI / 2.0, -PI / 2.0 + TAU * frac, false);
                    g.stroke();
                }
            }
        }
    }

    pub fn draw_gswitches(&mut self, g: &mut Canvas, level: &Stage, t: f64, dt: f64) {
        let p = self.pal;
        for (i, s) in level.gswitches.iter().enumerate() {
            if let Some(sp) = self.gsw_spin.get_mut(i) {
                *sp = (*sp - dt * 2.4).max(0.0);
            }
            let r = s.r;
            if !self.vis(r.x, r.y, r.w, r.h, 20.0) {
                continue;
            }
            let who = s.who;
            let spin = self.gsw_spin.get(i).copied().unwrap_or(0.0);
            let cx = r.x + r.w / 2.0;
            let cy = r.y + 8.5 + (t * 2.2 + i as f64).sin() * 0.7;
            let col = if p.dark { GRAV } else { mix(GRAV, BLACK, 0.15) };
            self.begin_glow(g);
            self.glow(g, cx, cy, 15.0 + spin * 8.0, css1(GRAV), 0.4 + spin * 0.4);
            self.end_glow(g);
            g.fill_style(css1(if p.dark {
                mix(GRAV, BLACK, 0.74)
            } else {
                mix(GRAV, WHITE, 0.72)
            }));
            g.begin_path();
            g.arc(cx, cy, 7.2, 0.0, TAU, false);
            g.fill();
            g.set_line_width(1.5);
            if who == Who::Both {
                g.stroke_style(css1(p.fire));
                g.begin_path();
                g.arc(cx, cy, 7.2, PI * 0.5, PI * 1.5, false);
                g.stroke();
                g.stroke_style(css1(p.frost));
                g.begin_path();
                g.arc(cx, cy, 7.2, PI * 1.5, PI * 2.5, false);
                g.stroke();
            } else {
                g.stroke_style(css1(match who {
                    Who::Fire => p.fire,
                    Who::Frost => p.frost,
                    _ => col,
                }));
                if who == Who::Other {
                    g.set_line_dash(&[2.6, 2.2]);
                    g.set_dash_offset(-t * 6.0);
                }
                g.begin_path();
                g.arc(cx, cy, 7.2, 0.0, TAU, false);
                g.stroke();
                g.set_line_dash(&[]);
            }
            g.save();
            g.translate(cx, cy);
            g.rotate(spin * spin * PI);
            g.stroke_style(css1(col));
            g.set_line_width(1.4);
            g.set_line_cap(Cap::Round);
            g.set_line_join(Join::Round);
            g.begin_path();
            g.move_to(-2.4, 3.6);
            g.line_to(-2.4, -3.4);
            g.move_to(-4.4, -1.4);
            g.line_to(-2.4, -3.6);
            g.line_to(-0.4, -1.4);
            g.move_to(2.4, -3.6);
            g.line_to(2.4, 3.4);
            g.move_to(0.4, 1.4);
            g.line_to(2.4, 3.6);
            g.line_to(4.4, 1.4);
            g.stroke();
            g.restore();
            let ly = r.y + r.h - 2.6;
            if who == Who::Fire || who == Who::Both {
                g.fill_style(css1(p.fire));
                flame_glyph(
                    g,
                    cx - if who == Who::Both { 3.4 } else { 0.0 },
                    ly - 0.4,
                    5.6,
                );
                g.fill();
            }
            if who == Who::Frost || who == Who::Both {
                g.stroke_style(css1(p.frost));
                g.set_line_width(0.8);
                flake_glyph(
                    g,
                    cx + if who == Who::Both { 3.4 } else { 0.0 },
                    ly - 0.4,
                    2.5,
                );
                g.stroke();
            }
            if who == Who::Myself || who == Who::Other {
                g.fill_style(css(p.ink, 0.75));
                g.stroke_style(css(p.ink, 0.75));
                g.set_line_width(0.8);
                g.begin_path();
                g.arc(
                    cx - if who == Who::Other { 4.4 } else { 0.0 },
                    ly,
                    1.5,
                    0.0,
                    TAU,
                    false,
                );
                g.fill();
                if who == Who::Other {
                    g.begin_path();
                    g.move_to(cx - 1.6, ly);
                    g.line_to(cx + 1.6, ly);
                    g.move_to(cx + 0.4, ly - 1.2);
                    g.line_to(cx + 1.6, ly);
                    g.line_to(cx + 0.4, ly + 1.2);
                    g.stroke();
                    g.begin_path();
                    g.arc(cx + 4.4, ly, 1.5, 0.0, TAU, false);
                    g.stroke();
                }
            }
        }
    }

    pub fn draw_locks(&mut self, g: &mut Canvas, level: &Stage, cur: &GameState, t: f64, dt: f64) {
        let p = self.pal;
        for (i, r) in level.locks.iter().enumerate() {
            let open = cur.locks.get(i).copied().unwrap_or(false);
            if let Some(gn) = self.lock_gone.get_mut(i) {
                *gn = approach(*gn, if open { 1.0 } else { 0.0 }, 11.0, dt);
            }
            let gone = self.lock_gone.get(i).copied().unwrap_or(0.0);
            if gone > 0.98 || !self.vis(r.x, r.y, r.w, r.h, 12.0) {
                continue;
            }
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            let body = if p.dark {
                mix(METAL, BLACK, 0.5)
            } else {
                mix(METAL, BLACK, 0.28)
            };
            g.save();
            g.translate(cx, cy);
            g.scale(1.0 + gone * 0.18, 1.0 + gone * 0.18);
            g.translate(-cx, -cy);
            g.set_alpha(1.0 - gone);
            g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.1 }));
            rr(g, r.x + 1.0, r.y + 2.0, r.w, r.h, 3.0);
            g.fill();
            g.fill_style(css1(body));
            rr(g, r.x, r.y, r.w, r.h, 3.0);
            g.fill();
            g.fill_style(css(GOLD, 0.9));
            let mut yy = r.y + 3.0;
            while yy < r.y + r.h - 3.0 {
                g.fill_rect(r.x + 1.0, yy, r.w - 2.0, 1.6);
                yy += T;
            }
            g.stroke_style(css1(GOLD));
            g.set_line_width(1.4);
            rr(g, r.x + 0.7, r.y + 0.7, r.w - 1.4, r.h - 1.4, 3.0);
            g.stroke();
            let ready = cur.keys > 0 && !open;
            if ready {
                self.begin_glow(g);
                self.glow(g, cx, cy, 14.0, css1(GOLD), 0.5 + 0.25 * (t * 6.0).sin());
                self.end_glow(g);
                g.set_alpha(1.0 - gone);
            }
            g.fill_style(css1(GOLD));
            g.begin_path();
            g.arc(cx, cy, 5.6, 0.0, TAU, false);
            g.fill();
            g.fill_style(css1(if ready {
                mix(GOLD_DARK, BLACK, 0.5)
            } else {
                mix(body, BLACK, 0.5)
            }));
            g.begin_path();
            g.arc(cx, cy - 1.2, 1.9, 0.0, TAU, false);
            g.fill();
            g.begin_path();
            g.move_to(cx - 1.0, cy - 0.6);
            g.line_to(cx + 1.0, cy - 0.6);
            g.line_to(cx + 1.7, cy + 3.6);
            g.line_to(cx - 1.7, cy + 3.6);
            g.close_path();
            g.fill();
            g.restore();
            g.set_alpha(1.0);
        }
    }

    pub fn key_at(&self, g: &mut Canvas, x: f64, y: f64, s: f64, ang: f64) {
        g.save();
        g.translate(x, y);
        g.rotate(ang);
        g.set_line_cap(Cap::Round);
        g.set_line_join(Join::Round);
        key_path(g, s);
        g.stroke_style(css1(GOLD_DARK));
        g.set_line_width(3.2 * s);
        g.stroke();
        g.stroke_style(css1(GOLD));
        g.set_line_width(1.7 * s);
        g.stroke();
        g.fill_style(css(WHITE, 0.8));
        g.fill_rect(-4.6 * s, -2.0 * s, 1.1 * s, 1.1 * s);
        g.restore();
    }

    pub fn draw_keys(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        opts: DrawOpts,
    ) {
        for (i, k) in level.keys.iter().enumerate() {
            if cur.key_taken.get(i) == Some(&true)
                || !self.vis(k.x - 10.0, k.y - 10.0, 20.0, 20.0, 24.0)
            {
                continue;
            }
            let fi = i as f64;
            let y = k.y + (t * 2.4 + fi * 2.1).sin() * 1.8;
            self.begin_glow(g);
            self.glow(
                g,
                k.x,
                y,
                17.0,
                css1(GOLD),
                0.5 + 0.12 * (t * 4.0 + fi).sin(),
            );
            self.end_glow(g);
            self.key_at(g, k.x - 1.0, y, 1.1, -0.5 + (t * 1.7 + fi).sin() * 0.12);
            if rnd() < if opts.reduce_motion { 0.01 } else { 0.035 } {
                self.parts.spawn(
                    PK_STAR,
                    C_GOLD,
                    k.x + rand(-7.0, 7.0),
                    y + rand(-6.0, 6.0),
                    0.0,
                    -4.0,
                    0.5,
                    rand(2.0, 3.0),
                    0.0,
                    0.0,
                    true,
                    0.0,
                );
            }
        }
    }

    pub fn draw_carried(&mut self, g: &mut Canvas, cur: &GameState, t: f64, dt: f64) {
        let n = cur.keys.clamp(0, 6) as usize;
        self.key_trail.truncate(n);
        if n == 0 {
            return;
        }
        let h = if cur.players[self.key_holder].alive {
            self.key_holder
        } else if self.key_holder == 0 {
            1
        } else {
            0
        };
        let pl = cur.players[h];
        let gv = if pl.grav == -1 { -1.0 } else { 1.0 };
        let face = pl.face as f64;
        let mut tx = self.px[h] + PLAYER_W / 2.0 - face * 13.0;
        let mut ty = self.py[h] + PLAYER_H / 2.0 - gv * 7.0;
        for i in 0..n {
            if i >= self.key_trail.len() {
                self.key_trail.push((tx, ty));
            }
            let mut k = self.key_trail[i];
            k.0 = approach(k.0, tx, 11.0 - i as f64 * 1.2, dt);
            k.1 = approach(k.1, ty, 11.0 - i as f64 * 1.2, dt);
            if (k.0 - tx).abs() + (k.1 - ty).abs() > 160.0 {
                k = (tx, ty);
            }
            self.key_trail[i] = k;
            let y = k.1 + (t * 4.0 + i as f64 * 1.3).sin() * 1.4;
            if self.vis(k.0 - 8.0, y - 8.0, 16.0, 16.0, 24.0) {
                self.begin_glow(g);
                self.glow(g, k.0, y, 11.0, css1(GOLD), 0.45);
                self.end_glow(g);
                self.key_at(g, k.0, y, 0.8, -0.5 + (t * 3.0 + i as f64).sin() * 0.2);
            }
            tx = k.0 - face * 9.0;
            ty = k.1;
        }
    }

    pub fn draw_arrows(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        opts: DrawOpts,
    ) {
        let p = self.pal;
        let col = if p.dark {
            ARROW
        } else {
            mix(ARROW, BLACK, 0.2)
        };
        for (i, a) in level.arrows.iter().enumerate() {
            if !self.vis(a.x - 10.0, a.y - 12.0, 20.0, 24.0, 24.0) {
                continue;
            }
            let left = cur.arrows.get(i).copied().unwrap_or(0) as f64;
            g.set_line_cap(Cap::Round);
            g.set_line_join(Join::Round);
            if left > 0.0 {
                let back = 1.0 - clamp(left / ARROW_RESPAWN_TICKS as f64, 0.0, 1.0);
                g.save();
                g.translate(a.x, a.y);
                g.set_alpha(0.3 + 0.25 * back);
                g.stroke_style(css1(col));
                g.set_line_width(1.1);
                g.set_line_dash(&[2.0, 2.0]);
                chevrons(g, 1.25);
                g.stroke();
                g.set_line_dash(&[]);
                g.set_alpha(0.75);
                g.set_line_width(1.4);
                g.begin_path();
                g.arc(0.0, 0.0, 8.6, -PI / 2.0, -PI / 2.0 + TAU * back, false);
                g.stroke();
                g.restore();
                g.set_alpha(1.0);
                continue;
            }
            let bob = if opts.reduce_motion {
                0.0
            } else {
                (t * 3.0 + i as f64 * 1.9).sin() * 2.2
            };
            let y = a.y + bob;
            self.begin_glow(g);
            self.glow(
                g,
                a.x,
                y,
                18.0,
                css1(ARROW),
                0.5 + 0.12 * (t * 5.0 + i as f64).sin(),
            );
            self.end_glow(g);
            g.save();
            g.translate(a.x, y);
            g.fill_style(css(
                if p.dark {
                    mix(ARROW, BLACK, 0.72)
                } else {
                    mix(ARROW, WHITE, 0.78)
                },
                0.85,
            ));
            g.begin_path();
            g.arc(0.0, 0.0, 8.0, 0.0, TAU, false);
            g.fill();
            g.stroke_style(css(col, 0.9));
            g.set_line_width(1.0);
            g.stroke();
            chevrons(g, 1.25);
            g.stroke_style(css1(mix(ARROW, BLACK, 0.5)));
            g.set_line_width(3.2);
            g.stroke();
            g.stroke_style(css1(if p.dark {
                mix(ARROW, WHITE, 0.45)
            } else {
                WHITE
            }));
            g.set_line_width(1.7);
            g.stroke();
            g.restore();
            if rnd() < if opts.reduce_motion { 0.01 } else { 0.05 } {
                self.parts.spawn(
                    PK_CHEV,
                    C_ARROW,
                    a.x + rand(-6.0, 6.0),
                    y + 8.0,
                    0.0,
                    -rand(20.0, 40.0),
                    0.5,
                    1.8,
                    0.0,
                    0.0,
                    true,
                    0.0,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_cannons(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        dt: f64,
        opts: DrawOpts,
    ) {
        let p = self.pal;
        for (i, c) in level.cannons.iter().enumerate() {
            if let Some(k) = self.cannon_kick.get_mut(i) {
                *k = (*k - dt * 4.0).max(0.0);
            }
            if !self.vis(c.x - 12.0, c.y - 12.0, 24.0, 24.0, 220.0) {
                continue;
            }
            let kick = self.cannon_kick.get(i).copied().unwrap_or(0.0);
            let ticks = cur.cannons.get(i).copied().unwrap_or(0) as f64;
            let who: i64 = if cur.players[0].cannon == i as i64 {
                0
            } else if cur.players[1].cannon == i as i64 {
                1
            } else {
                -1
            };
            let loaded = ticks > 0.0 || who >= 0;
            let ang = c.vy.atan2(c.vx);
            let chs = cond_channels(c.fire.as_ref());
            let ready = c.fire.is_some() && cond_holds(c.fire.as_ref(), &cur.on, cur.lit);
            let ch_col = chs.first().map_or(css1(p.muted), |s| channel_color(s));
            let gv = if who >= 0 && cur.players[who as usize].grav == -1 {
                -1.0
            } else {
                1.0
            };
            g.fill_style(css(
                match who {
                    0 => p.fire,
                    1 => p.frost,
                    _ => p.ink,
                },
                if loaded {
                    0.7
                } else if p.dark {
                    0.26
                } else {
                    0.22
                },
            ));
            let fly_s = CANNON_FLY_TICKS as f64 / 60.0;
            for k in 1..=26 {
                let tt = f64::from(k) * 0.055
                    + if opts.reduce_motion || !loaded {
                        0.0
                    } else {
                        (t * 0.5) % 0.055
                    };
                if tt > fly_s {
                    break;
                }
                let x = c.x + c.vx * tt;
                let y = c.y + c.vy * tt + 0.5 * GRAVITY * gv * tt * tt;
                if tile_at(level, cur, (x / T).floor() as i64, (y / T).floor() as i64) == T_WALL {
                    break;
                }
                if k < 3 {
                    continue;
                }
                g.begin_path();
                g.arc(x, y, if loaded { 1.5 } else { 1.1 }, 0.0, TAU, false);
                g.fill();
            }
            let metal = if p.dark {
                mix(METAL, BLACK, 0.42)
            } else {
                mix(METAL, BLACK, 0.22)
            };
            let swell = if loaded && !opts.reduce_motion {
                1.0 + 0.05 * (t * 22.0).sin()
            } else {
                1.0
            };
            g.save();
            g.translate(c.x, c.y);
            g.rotate(ang);
            g.translate(-kick * kick * 5.0, 0.0);
            g.scale(1.0, swell);
            g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.12 }));
            rr(g, -8.0, -5.5, 21.0, 14.0, 5.0);
            g.fill();
            g.fill_style(css1(metal));
            rr(g, -9.0, -7.0, 21.0, 14.0, 5.0);
            g.fill();
            g.fill_style(css1(mix(metal, WHITE, if p.dark { 0.22 } else { 0.4 })));
            g.fill_rect(-5.0, -7.0, 14.0, 2.0);
            g.fill_style(ch_col);
            g.fill_rect(-2.5, -7.0, 3.0, 14.0);
            g.fill_style(css1(mix(metal, WHITE, 0.35)));
            rr(g, 9.5, -8.0, 3.6, 16.0, 1.6);
            g.fill();
            g.fill_style(css(BLACK, 0.7));
            g.begin_path();
            g.ellipse(13.0, 0.0, 1.6, 5.4, 0.0, 0.0, TAU, false);
            g.fill();
            if who >= 0 {
                g.fill_style(css1(if who == 0 { p.fire } else { p.frost2 }));
                g.begin_path();
                g.arc(12.2, 0.0, 4.6, -PI / 2.0, PI / 2.0, false);
                g.fill();
                g.fill_style(if who == 0 {
                    Col::rgb(64.0, 22.0, 6.0)
                } else {
                    Col::rgb(14.0, 48.0, 84.0)
                });
                g.fill_rect(14.4, -2.4, 1.3, 1.6);
                g.fill_rect(14.4, 0.9, 1.3, 1.6);
            }
            g.restore();
            g.fill_style(css1(mix(metal, BLACK, 0.3)));
            g.begin_path();
            g.arc(c.x, c.y, 4.6, 0.0, TAU, false);
            g.fill();
            g.fill_style(
                if loaded && (c.fire.is_none() || ready || (t * 10.0).sin() > 0.0) {
                    ch_col
                } else {
                    css1(mix(metal, WHITE, 0.3))
                },
            );
            g.begin_path();
            g.arc(c.x, c.y, 1.9, 0.0, TAU, false);
            g.fill();
            if loaded {
                self.begin_glow(g);
                self.glow(
                    g,
                    c.x,
                    c.y,
                    18.0,
                    css1(if who == 0 { p.fire } else { p.frost }),
                    0.45,
                );
                self.end_glow(g);
                g.set_line_cap(Cap::Round);
                if c.fire.is_none() {
                    let frac = clamp(ticks / CANNON_FUSE_TICKS as f64, 0.0, 1.0);
                    g.stroke_style(css(p.ink, 0.2));
                    g.set_line_width(1.6);
                    g.begin_path();
                    g.arc(c.x, c.y, 12.5, 0.0, TAU, false);
                    g.stroke();
                    g.stroke_style(css1(LAVA_TOP));
                    g.set_line_width(2.0);
                    g.begin_path();
                    g.arc(c.x, c.y, 12.5, -PI / 2.0, -PI / 2.0 + TAU * frac, false);
                    g.stroke();
                    let fa = -PI / 2.0 + TAU * frac;
                    if rnd() < if opts.reduce_motion { 0.1 } else { 0.6 } {
                        self.parts.spawn(
                            PK_SPARK,
                            if rnd() < 0.5 { C_CORE } else { C_FIRE2 },
                            c.x + fa.cos() * 12.5,
                            c.y + fa.sin() * 12.5,
                            rand(-30.0, 30.0),
                            rand(-40.0, 10.0),
                            rand(0.2, 0.4),
                            rand(0.7, 1.2),
                            120.0,
                            1.0,
                            true,
                            0.0,
                        );
                    }
                } else {
                    g.stroke_style(ch_col);
                    g.set_alpha(0.45 + 0.4 * (t * 5.0).sin().abs());
                    g.set_line_width(1.6);
                    g.set_line_dash(&[3.0, 3.0]);
                    g.set_dash_offset(-t * 10.0);
                    g.begin_path();
                    g.arc(c.x, c.y, 12.5, 0.0, TAU, false);
                    g.stroke();
                    g.set_line_dash(&[]);
                    g.set_alpha(1.0);
                }
            }
        }
    }

    // ---- fans

    pub fn draw_fan_columns(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        dt: f64,
        opts: DrawOpts,
    ) {
        let p = self.pal;
        for (i, f) in level.fans.iter().enumerate() {
            let on = cond_holds(f.when.as_ref(), &cur.on, cur.lit);
            if i >= self.fan_spin.len() {
                continue;
            }
            self.fan_spin[i] = approach(
                self.fan_spin[i],
                if on { 1.0 } else { 0.0 },
                if on { 3.0 } else { 1.5 },
                dt,
            );
            self.fan_ang[i] += self.fan_spin[i] * dt * 22.0;
            let spin = self.fan_spin[i];
            let r = f.r;
            if !self.vis(r.x, r.y, r.w, r.h, 4.0) {
                continue;
            }
            let ch = cond_channels(f.when.as_ref()).first().cloned();
            let fy = r.y + r.h - T;
            if spin > 0.05 {
                let mut gr = Gradient::linear(0.0, fy, 0.0, r.y);
                gr.stop(0.0, css(if p.dark { WHITE } else { p.frost }, 0.09 * spin));
                gr.stop(1.0, css(if p.dark { WHITE } else { p.frost }, 0.0));
                g.fill_style(gr);
                g.fill_rect(r.x + 2.0, r.y, r.w - 4.0, r.h - T);
                let rate = (r.w / T) * if opts.reduce_motion { 3.0 } else { 10.0 } * spin;
                if rnd() < rate * dt {
                    self.parts.spawn(
                        PK_STREAK,
                        C_WHITE,
                        r.x + rand(3.0, r.w - 3.0),
                        fy,
                        0.0,
                        -rand(160.0, 260.0),
                        (r.h - T) / 220.0,
                        rand(5.0, 10.0),
                        0.0,
                        0.0,
                        true,
                        0.0,
                    );
                }
            }
            g.fill_style(css1(if p.dark {
                mix(METAL, BLACK, 0.45)
            } else {
                mix(METAL, BLACK, 0.1)
            }));
            rr(g, r.x, fy, r.w, T, 3.0);
            g.fill();
            let mut tx = r.x;
            while tx < r.x + r.w {
                let (cx, cy) = (tx + T / 2.0, fy + T / 2.0 + 1.0);
                g.fill_style(css(BLACK, 0.4));
                g.begin_path();
                g.arc(cx, cy, 7.5, 0.0, TAU, false);
                g.fill();
                g.fill_style(css1(if p.dark {
                    mix(METAL, WHITE, 0.25)
                } else {
                    mix(METAL, WHITE, 0.35)
                }));
                for b in 0..3 {
                    let ang = self.fan_ang[i] + f64::from(b) * TAU / 3.0;
                    g.begin_path();
                    g.ellipse(
                        cx + ang.cos() * 3.6,
                        cy + ang.sin() * 3.6,
                        3.6,
                        1.6,
                        ang,
                        0.0,
                        TAU,
                        false,
                    );
                    g.fill();
                }
                g.fill_style(ch.as_deref().map_or(css1(p.muted), channel_color));
                g.begin_path();
                g.arc(cx, cy, 1.6, 0.0, TAU, false);
                g.fill();
                tx += T;
            }
            g.fill_style(css1(if p.dark {
                mix(METAL, WHITE, 0.1)
            } else {
                METAL
            }));
            g.fill_rect(r.x + 1.0, fy, r.w - 2.0, 2.2);
            if let Some(ch) = &ch {
                g.fill_style(channel_color(ch));
                g.fill_rect(r.x + 1.0, fy + T - 2.5, r.w - 2.0, 1.6);
            }
        }
    }

    // ---- gems, beams

    pub fn draw_gems(
        &mut self,
        g: &mut Canvas,
        level: &Stage,
        cur: &GameState,
        t: f64,
        opts: DrawOpts,
    ) {
        let p = self.pal;
        for (i, gm) in level.gems.iter().enumerate() {
            if cur.taken.get(i) == Some(&true)
                || !self.vis(gm.x - 8.0, gm.y - 8.0, 16.0, 16.0, 24.0)
            {
                continue;
            }
            let fire = gm.el == Element::Fire;
            let (c, c2) = if fire {
                (p.fire, p.fire2)
            } else {
                (p.frost, p.frost2)
            };
            let fi = i as f64;
            let y = gm.y + (t * 2.6 + fi * 1.7).sin() * 1.6;
            self.begin_glow(g);
            self.glow(g, gm.x, y, 16.0, css1(c), 0.5 + 0.12 * (t * 4.0 + fi).sin());
            self.end_glow(g);
            g.save();
            g.translate(gm.x, y);
            let turn = 0.75 + 0.25 * (t * 1.8 + fi).cos();
            g.scale(turn, 1.0);
            gem_path(g, 1.0);
            g.fill_style(css1(c));
            g.fill();
            g.fill_style(css1(c2));
            g.begin_path();
            g.move_to(-5.0, -1.5);
            g.line_to(-2.8, -4.5);
            g.line_to(2.8, -4.5);
            g.line_to(5.0, -1.5);
            g.close_path();
            g.fill();
            g.fill_style(css1(mix(c, BLACK, 0.25)));
            g.begin_path();
            g.move_to(1.5, -1.5);
            g.line_to(5.0, -1.5);
            g.line_to(0.0, 5.5);
            g.close_path();
            g.fill();
            g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.85));
            g.begin_path();
            g.move_to(-2.6, -3.9);
            g.line_to(-0.6, -3.9);
            g.line_to(-2.4, -1.9);
            g.close_path();
            g.fill();
            g.stroke_style(css(mix(c, BLACK, 0.4), 0.7));
            g.set_line_width(0.6);
            gem_path(g, 1.0);
            g.stroke();
            g.restore();
            if rnd() < if opts.reduce_motion { 0.01 } else { 0.03 } {
                self.parts.spawn(
                    PK_STAR,
                    if fire { C_FIRE2 } else { C_WHITE },
                    gm.x + rand(-6.0, 6.0),
                    y + rand(-6.0, 5.0),
                    0.0,
                    -4.0,
                    0.5,
                    rand(2.0, 3.2),
                    0.0,
                    0.0,
                    true,
                    0.0,
                );
            }
        }
    }

    pub fn draw_beams(&mut self, g: &mut Canvas, t: f64, opts: DrawOpts) {
        if self.beams.is_empty() {
            return;
        }
        let star = css1(self.pal.star);
        let fl = 0.9 + 0.1 * (t * 31.0).sin();
        let dark = self.pal.dark;
        g.set_line_cap(Cap::Round);
        self.begin_glow(g);
        let beams = self.beams.clone();
        for b in &beams {
            g.stroke_style(star);
            g.set_alpha((if dark { 0.18 } else { 0.14 }) * fl);
            g.set_line_width(8.0);
            g.begin_path();
            g.move_to(b.0, b.1);
            g.line_to(b.2, b.3);
            g.stroke();
            g.set_alpha((if dark { 0.45 } else { 0.35 }) * fl);
            g.set_line_width(3.4);
            g.stroke();
        }
        self.end_glow(g);
        g.stroke_style(self.colors[C_CORE]);
        g.set_line_width(1.3);
        g.begin_path();
        for b in &beams {
            g.move_to(b.0, b.1);
            g.line_to(b.2, b.3);
        }
        g.stroke();
        self.begin_glow(g);
        for b in &beams {
            self.glow(g, b.2, b.3, 9.0, star, 0.7 * fl);
        }
        self.end_glow(g);
        if rnd() < if opts.reduce_motion { 0.05 } else { 0.25 } {
            let b = beams[(rnd() * beams.len() as f64).floor() as usize % beams.len()];
            self.parts.spawn(
                PK_STAR,
                C_STAR,
                b.2,
                b.3,
                rand(-20.0, 20.0),
                rand(-20.0, 20.0),
                0.35,
                2.4,
                0.0,
                2.0,
                true,
                0.0,
            );
        }
    }
}

pub fn light_glyph(g: &mut Canvas, sun: bool, cx: f64, cy: f64, color: Col) {
    g.fill_style(color);
    g.stroke_style(color);
    if sun {
        g.begin_path();
        g.arc(cx, cy, 2.4, 0.0, TAU, false);
        g.fill();
        g.set_line_width(1.0);
        g.set_line_cap(Cap::Round);
        g.begin_path();
        for k in 0..8 {
            let an = f64::from(k) * TAU / 8.0;
            g.move_to(cx + an.cos() * 4.0, cy + an.sin() * 4.0);
            g.line_to(cx + an.cos() * 5.6, cy + an.sin() * 5.6);
        }
        g.stroke();
    } else {
        g.begin_path();
        g.arc(cx, cy, 4.4, PI * 0.35, PI * 1.65, false);
        g.arc(cx + 2.6, cy, 3.6, PI * 1.42, PI * 0.58, true);
        g.close_path();
        g.fill();
    }
}
