//! emberDraw.ts, the two characters and the select screen's thumbnails (the Mac's EmberDrawPlayers.swift). Line for
//! line with the Swift.

use std::f64::consts::PI;

use qcalc_core::games::ember::{
    Element, GameState, ONEWAY_H, PLAYER_H, PLAYER_W, POOL_SINK, Phase, Player, RUN_SPEED, Stage,
    T_CRUMBLE, T_GOO, T_LAVA, T_LIGHT, T_ONEWAY, T_THIN, T_TRAMP, T_WALL, T_WATER, TILE,
};

use super::draw::*;
use super::fx::*;
use crate::games::canvas::{Baseline, Canvas, Cap, Col, Gradient, Join, TextAlign};

impl Renderer {
    pub fn draw_player(
        &mut self,
        g: &mut Canvas,
        i: usize,
        cur: &GameState,
        t: f64,
        dt: f64,
        opts: DrawOpts,
    ) {
        let pl = cur.players[i];
        let (x, y) = (self.px[i], self.py[i]);
        let gv = if pl.grav == -1 { -1.0 } else { 1.0 };
        self.vy_rel = pl.vy * gv;
        let mut an = self.anims[i];
        animate(&mut an, &pl, self.vy_rel, t, dt);
        an.flip = if opts.reduce_motion {
            if gv < 0.0 { 1.0 } else { 0.0 }
        } else {
            approach(an.flip, if gv < 0.0 { 1.0 } else { 0.0 }, 16.0, dt)
        };
        an.aura = approach(an.aura, if gv < 0.0 { 1.0 } else { 0.0 }, 8.0, dt);
        let fire = i == 0;
        let cx = x + PLAYER_W / 2.0;
        let feet = if gv > 0.0 { y + PLAYER_H } else { y };
        an.x = cx;
        an.y = feet;
        self.anims[i] = an;
        if !pl.alive || pl.cannon >= 0 || !self.vis(x, y, PLAYER_W, PLAYER_H, 40.0) {
            return;
        }
        let p = self.pal;
        let cy = y + PLAYER_H / 2.0;
        let moving = pl.vx.abs() > 30.0;
        let bob = if pl.ground && moving {
            an.run.sin().abs() * 1.6
        } else {
            0.0
        };
        let breathe = 1.0 + (t * 3.1 + i as f64).sin() * 0.025;
        let sy = (1.0 + an.sq) * breathe;
        let sx = 1.0 - an.sq * 0.55;
        self.begin_glow(g);
        self.glow(
            g,
            cx,
            cy,
            if fire { 30.0 } else { 26.0 },
            css1(if fire { p.fire } else { p.frost }),
            if fire {
                0.55 + 0.08 * (t * 13.0).sin()
            } else {
                0.42
            },
        );
        if an.aura > 0.02 {
            self.glow(g, cx, cy, 30.0, css1(GRAV), 0.5 * an.aura);
        }
        self.end_glow(g);
        if pl.ground {
            g.fill_style(css(BLACK, if p.dark { 0.3 } else { 0.12 }));
            g.begin_path();
            g.ellipse(cx, feet, 7.0 * sx, 1.6, 0.0, 0.0, TAU, false);
            g.fill();
        }
        if an.aura > 0.02 {
            g.save();
            g.set_alpha(an.aura);
            g.stroke_style(css(if p.dark { GRAV } else { mix(GRAV, BLACK, 0.15) }, 0.9));
            g.set_line_width(1.3);
            g.set_line_dash(&[4.0, 4.0]);
            g.set_dash_offset(if opts.reduce_motion { 0.0 } else { -t * 14.0 });
            g.begin_path();
            g.ellipse(cx, cy, 13.0, 16.0, 0.0, 0.0, TAU, false);
            g.stroke();
            g.set_line_dash(&[]);
            g.set_line_cap(Cap::Round);
            g.set_line_join(Join::Round);
            g.set_line_width(1.6);
            for s in [-1.0, 1.0] {
                let ay = cy
                    + if opts.reduce_motion {
                        0.0
                    } else {
                        -((t * 14.0 + if s > 0.0 { 8.0 } else { 0.0 }) % 16.0) + 8.0
                    };
                g.set_alpha(an.aura * (1.0 - (ay - cy).abs() / 10.0));
                g.begin_path();
                g.move_to(cx + s * 13.0 - 2.6, ay + 1.6);
                g.line_to(cx + s * 13.0, ay - 1.6);
                g.line_to(cx + s * 13.0 + 2.6, ay + 1.6);
                g.stroke();
            }
            g.restore();
            g.set_alpha(1.0);
        }
        if pl.fly > 0 {
            let mut sp = pl.vx.hypot(pl.vy);
            if sp == 0.0 {
                sp = 1.0;
            }
            g.fill_style(css1(if fire { p.fire2 } else { p.frost2 }));
            for k in 1..=4 {
                let kd = f64::from(k);
                g.set_alpha(0.34 - kd * 0.07);
                g.begin_path();
                g.arc(
                    cx - pl.vx / sp * kd * 7.0,
                    cy - pl.vy / sp * kd * 7.0,
                    7.5 - kd * 1.2,
                    0.0,
                    TAU,
                    false,
                );
                g.fill();
            }
            g.set_alpha(1.0);
        }
        g.save();
        g.translate(cx, cy);
        if an.flip > 0.001 {
            g.rotate(PI * an.flip);
            if an.flip > 0.5 {
                g.scale(-1.0, 1.0);
            }
        }
        g.translate(0.0, PLAYER_H / 2.0 - bob);
        g.rotate(an.tilt);
        g.scale(sx, sy);
        let me = opts.me == Some(i);
        let happy = cur.phase == Phase::Won || cur.phase == Phase::Clear || pl.in_door;
        if fire {
            self.draw_ember(g, &an, &pl, t, me, happy);
        } else {
            self.draw_frost(g, &an, &pl, t, me, happy);
        }
        g.restore();

        if pl.air > 0 {
            let col = if p.dark {
                ARROW
            } else {
                mix(ARROW, BLACK, 0.2)
            };
            let pulse = if opts.reduce_motion {
                0.5
            } else {
                0.5 + 0.5 * (t * 9.0).sin()
            };
            self.begin_glow(g);
            self.glow(g, cx, feet + gv * 4.0, 13.0, css1(ARROW), 0.5 + 0.3 * pulse);
            self.end_glow(g);
            g.save();
            g.translate(cx, feet + gv * (5.0 + pulse * 1.5));
            g.scale(1.0, gv);
            g.set_line_cap(Cap::Round);
            g.set_line_join(Join::Round);
            chevrons(g, 1.15);
            g.stroke_style(css1(mix(ARROW, BLACK, 0.55)));
            g.set_line_width(3.0);
            g.stroke();
            g.stroke_style(css1(if p.dark {
                mix(ARROW, WHITE, 0.4)
            } else {
                WHITE
            }));
            g.set_line_width(1.5);
            g.stroke();
            g.restore();
            g.stroke_style(css(col, 0.55 + 0.3 * pulse));
            g.set_line_width(1.1);
            g.begin_path();
            g.ellipse(cx, cy, 11.5, 14.5, 0.0, 0.0, TAU, false);
            g.stroke();
        }

        let up = |d: f64| feet - gv * d;
        let face = pl.face as f64;
        let less = if opts.reduce_motion { 0.3 } else { 1.0 };
        let rate = (if pl.fly > 0 {
            70.0
        } else if moving || !pl.ground {
            26.0
        } else {
            0.0
        }) * less;
        self.anims[i].trail += rate * dt;
        self.anims[i].ambient += (if fire { 7.0 } else { 3.0 }) * less * dt;
        while self.anims[i].trail >= 1.0 {
            self.anims[i].trail -= 1.0;
            if fire {
                self.parts.spawn(
                    PK_SPARK,
                    if rnd() < 0.5 { C_FIRE2 } else { C_CORE },
                    cx - face * rand(2.0, 6.0),
                    up(rand(6.0, 16.0)),
                    -pl.vx * 0.15 + rand(-10.0, 10.0),
                    gv * rand(-40.0, -10.0),
                    rand(0.3, 0.55),
                    rand(0.6, 1.1),
                    gv * -40.0,
                    1.0,
                    true,
                    0.0,
                );
            } else {
                self.parts.spawn(
                    PK_FLAKE,
                    C_FROST2,
                    cx - face * rand(2.0, 6.0),
                    up(rand(4.0, 18.0)),
                    -pl.vx * 0.12 + rand(-8.0, 8.0),
                    gv * rand(-6.0, 12.0),
                    rand(0.5, 0.9),
                    rand(1.4, 2.2),
                    gv * 10.0,
                    1.5,
                    true,
                    3.0,
                );
            }
        }
        while self.anims[i].ambient >= 1.0 {
            self.anims[i].ambient -= 1.0;
            if fire {
                self.parts.spawn(
                    PK_SPARK,
                    C_FIRE2,
                    cx + rand(-3.0, 3.0),
                    up(22.0 + bob),
                    rand(-8.0, 8.0),
                    gv * rand(-45.0, -25.0),
                    rand(0.35, 0.6),
                    rand(0.5, 0.9),
                    gv * -20.0,
                    1.0,
                    true,
                    0.0,
                );
            } else {
                self.parts.spawn(
                    PK_STAR,
                    C_WHITE,
                    cx + rand(-7.0, 7.0),
                    up(rand(6.0, 24.0)),
                    0.0,
                    gv * -3.0,
                    0.45,
                    rand(1.8, 2.6),
                    0.0,
                    0.0,
                    true,
                    0.0,
                );
            }
            if gv < 0.0 && !opts.reduce_motion && rnd() < 0.6 {
                self.parts.spawn(
                    PK_CHEV,
                    C_GRAV,
                    cx + rand(-10.0, 10.0),
                    cy + rand(-4.0, 12.0),
                    0.0,
                    -rand(30.0, 60.0),
                    0.5,
                    2.0,
                    0.0,
                    0.0,
                    true,
                    0.0,
                );
            }
        }

        if me && cur.phase == Phase::Play && cur.tick < 150 {
            let fade = clamp((150 - cur.tick) as f64 / 30.0, 0.0, 1.0);
            let ay = y - 12.0 + (t * 6.0).sin() * 1.5;
            g.set_alpha(fade);
            g.fill_style(css1(p.accent));
            g.begin_path();
            g.move_to(cx - 4.0, ay - 4.0);
            g.line_to(cx + 4.0, ay - 4.0);
            g.line_to(cx, ay + 1.0);
            g.close_path();
            g.fill();
            g.set_font(8.0, true);
            g.set_text_align(TextAlign::Center);
            g.set_text_baseline(Baseline::Alphabetic);
            g.fill_text("you", cx, ay - 6.0);
            g.set_alpha(1.0);
        }
    }

    fn eyes(&self, g: &mut Canvas, an: &PAnim, t: f64, y: f64, ink: Col, happy: bool) {
        let lx = an.look * 1.7;
        let ly = if self.vy_rel < -60.0 {
            -0.7
        } else if self.vy_rel > 200.0 {
            0.6
        } else {
            0.0
        };
        let blink = t < an.blink_until;
        for s in [-1.0, 1.0] {
            let ex = s * 2.9 + lx;
            let ey = y + ly;
            if happy {
                g.stroke_style(ink);
                g.set_line_width(1.1);
                g.set_line_cap(Cap::Round);
                g.begin_path();
                g.arc(ex, ey + 0.8, 1.5, PI * 1.15, PI * 1.85, false);
                g.stroke();
                continue;
            }
            g.fill_style(ink);
            g.begin_path();
            g.ellipse(
                ex,
                ey,
                1.25,
                if blink { 0.3 } else { 2.0 },
                0.0,
                0.0,
                TAU,
                false,
            );
            g.fill();
            if !blink {
                g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.95));
                g.fill_rect(ex - 0.2 + an.look * 0.3, ey - 1.3, 0.75, 0.75);
            }
        }
    }

    fn draw_ember(&self, g: &mut Canvas, an: &PAnim, pl: &Player, t: f64, me: bool, happy: bool) {
        let p = self.pal;
        let fl = (t * 17.0).sin() * 0.9 + (t * 11.3 + 1.0).sin() * 0.6;
        let lean = -an.tilt * 40.0 - clamp(self.vy_rel / 400.0, -1.0, 1.0) * 0.5;
        let tip_x = lean + (t * 6.3).sin() * 0.9;
        let tip_y = -25.0 - fl - an.sq.max(0.0) * 6.0;
        let fire = css1(p.fire);
        g.fill_style(fire);
        for s in [-1.0, 1.0] {
            let h = 7.0 + (t * 13.0 + s * 2.0).sin() * 1.6;
            let bx = s * 6.0;
            g.begin_path();
            g.move_to(bx - 2.6, -9.0);
            g.quadratic_curve_to(
                bx + s * 2.5,
                -11.0 - h * 0.4,
                bx + s * 2.2 + tip_x * 0.2,
                -10.0 - h,
            );
            g.quadratic_curve_to(bx + s * 0.5, -10.0, bx + 2.6, -8.0);
            g.close_path();
            g.fill();
        }
        flame(g, 8.0, -8.0, tip_x, tip_y);
        if me {
            g.stroke_style(css(p.accent, 0.7));
            g.set_line_width(2.4);
            g.stroke();
        }
        g.fill_style(fire);
        g.fill();
        flame(
            g,
            5.8,
            -6.6,
            tip_x * 0.75,
            -19.0 - (t * 19.0 + 2.0).sin() * 1.2,
        );
        g.fill_style(css1(p.fire2));
        g.fill();
        flame(g, 3.4, -4.8, tip_x * 0.5, -12.5 - (t * 23.0).sin() * 0.8);
        g.fill_style(Col::rgba(255.0, 246.0, 214.0, 0.95));
        g.fill();
        self.eyes(g, an, t, -10.5, Col::rgb(64.0, 22.0, 6.0), happy);
        g.fill_style(Col::rgba(255.0, 90.0, 60.0, 0.45));
        g.begin_path();
        g.ellipse(-5.0 + an.look * 1.2, -7.6, 1.5, 0.9, 0.0, 0.0, TAU, false);
        g.ellipse(5.0 + an.look * 1.2, -7.6, 1.5, 0.9, 0.0, 0.0, TAU, false);
        g.fill();
        if happy || !pl.ground {
            g.stroke_style(Col::rgb(64.0, 22.0, 6.0));
            g.set_line_width(0.9);
            g.begin_path();
            if happy {
                g.arc(an.look * 1.7, -7.8, 1.6, 0.15 * PI, 0.85 * PI, false);
            } else {
                g.ellipse(an.look * 1.7, -6.8, 0.8, 1.1, 0.0, 0.0, TAU, false);
            }
            g.stroke();
        }
    }

    fn draw_frost(&self, g: &mut Canvas, an: &PAnim, pl: &Player, t: f64, me: bool, happy: bool) {
        let p = self.pal;
        let edge = css1(p.frost);
        let pale = mix(p.frost2, WHITE, 0.55);
        let shimmer = (t * 0.6) % 3.0;
        let spikes: [(f64, f64, f64, f64); 3] = [
            (-5.5, -15.0, -0.5, 7.5),
            (5.5, -15.0, 0.5, 7.5),
            (0.0, -17.0, 0.0, 10.0 + an.sq.max(0.0) * 5.0),
        ];
        for (k, &(bx, by, ang, len)) in spikes.iter().enumerate() {
            let tx = bx + ang.sin() * len - an.tilt * 8.0;
            let ty = by - ang.cos() * len;
            let w = 2.6;
            let (nx, ny) = (ang.cos() * w, ang.sin() * w);
            g.fill_style(css1(p.frost2));
            g.begin_path();
            g.move_to(bx - nx, by - ny);
            g.line_to(tx, ty);
            g.line_to(bx + nx, by + ny);
            g.close_path();
            g.fill();
            g.fill_style(css(
                if (shimmer - k as f64).abs() < 0.4 {
                    WHITE
                } else {
                    pale
                },
                0.9,
            ));
            g.begin_path();
            g.move_to(bx - nx, by - ny);
            g.line_to(tx, ty);
            g.line_to(bx, by);
            g.close_path();
            g.fill();
        }
        g.begin_path();
        g.move_to(-7.5, -4.0);
        g.line_to(-8.2, -11.5);
        g.line_to(-4.0, -18.0);
        g.line_to(4.0, -18.0);
        g.line_to(8.2, -11.5);
        g.line_to(7.5, -4.0);
        g.quadratic_curve_to(6.5, 0.0, 0.0, 0.0);
        g.quadratic_curve_to(-6.5, 0.0, -7.5, -4.0);
        g.close_path();
        g.set_line_join(Join::Round);
        if me {
            g.stroke_style(css(p.accent, 0.7));
            g.set_line_width(3.2);
            g.stroke();
        }
        let mut body = Gradient::linear(-8.0, -18.0, 8.0, 0.0);
        body.stop(0.0, css1(pale));
        body.stop(1.0, css1(p.frost2));
        g.fill_style(body);
        g.fill();
        g.stroke_style(edge);
        g.set_line_width(1.1);
        g.stroke();
        g.fill_style(Col::rgba(255.0, 255.0, 255.0, 0.6));
        g.begin_path();
        g.move_to(-4.0, -18.0);
        g.line_to(0.5, -18.0);
        g.line_to(-2.5, -13.5);
        g.line_to(-8.2, -11.5);
        g.close_path();
        g.fill();
        g.stroke_style(css(p.frost, 0.35));
        g.set_line_width(0.6);
        g.begin_path();
        g.move_to(-2.5, -13.5);
        g.line_to(2.5, -13.5);
        g.line_to(8.2, -11.5);
        g.move_to(2.5, -13.5);
        g.line_to(4.0, -18.0);
        g.stroke();
        self.eyes(g, an, t, -9.0, Col::rgb(14.0, 48.0, 84.0), happy);
        g.fill_style(Col::rgba(120.0, 170.0, 255.0, 0.45));
        g.begin_path();
        g.ellipse(-5.0 + an.look * 1.2, -6.2, 1.4, 0.8, 0.0, 0.0, TAU, false);
        g.ellipse(5.0 + an.look * 1.2, -6.2, 1.4, 0.8, 0.0, 0.0, TAU, false);
        g.fill();
        if happy || !pl.ground {
            g.stroke_style(Col::rgb(14.0, 48.0, 84.0));
            g.set_line_width(0.9);
            g.begin_path();
            if happy {
                g.arc(an.look * 1.7, -6.2, 1.6, 0.15 * PI, 0.85 * PI, false);
            } else {
                g.ellipse(an.look * 1.7, -5.2, 0.8, 1.1, 0.0, 0.0, TAU, false);
            }
            g.stroke();
        }
    }
}

fn animate(an: &mut PAnim, pl: &Player, vy_rel: f64, t: f64, dt: f64) {
    if an.was_ground && !pl.ground && vy_rel < 0.0 {
        an.sqv += 5.0;
    }
    if !an.was_ground && pl.ground {
        an.sqv -= 6.0;
    }
    an.was_ground = pl.ground;
    let mut left = dt;
    while left > 0.0 {
        let h = left.min(1.0 / 240.0);
        an.sqv += (-320.0 * an.sq - 16.0 * an.sqv) * h;
        an.sq += an.sqv * h;
        left -= h;
    }
    an.sq = clamp(an.sq, -0.32, 0.32);
    if pl.ground {
        an.run += dt * pl.vx.abs() * 0.085;
    }
    an.tilt = approach(
        an.tilt,
        clamp(pl.vx / RUN_SPEED, -1.0, 1.0) * 0.12,
        10.0,
        dt,
    );
    an.look = approach(an.look, pl.face as f64, 12.0, dt);
    if t > an.blink_at {
        an.blink_until = t + 0.11;
        an.blink_at = t + if rnd() < 0.2 { 0.25 } else { 2.0 + rnd() * 3.0 };
    }
}

fn flame(g: &mut Canvas, r: f64, cy: f64, tip_x: f64, tip_y: f64) {
    g.begin_path();
    g.move_to(r, cy);
    g.arc(0.0, cy, r, 0.0, PI, false);
    g.bezier_curve_to(
        -r,
        cy - r * 0.95,
        tip_x - r * 0.45,
        tip_y + r * 1.1,
        tip_x,
        tip_y,
    );
    g.bezier_curve_to(tip_x + r * 0.45, tip_y + r * 1.1, r, cy - r * 0.95, r, cy);
    g.close_path();
}

/// drawThumb: a small static picture of a stage for the select screen, the whole stage fitted in
pub fn draw_thumb(g: &mut Canvas, level: &Stage, stages: usize, dark: bool) {
    let p = read_palette(dark);
    let (cw, ch) = (g.width, g.height);
    let k = (cw / level.w).min(ch / level.h);
    let (ox, oy) = ((cw - level.w * k) / 2.0, (ch - level.h * k) / 2.0);
    let u = 1f64.max(level.w / 640.0).max(level.h / 360.0);
    g.reset_transform();
    g.clear_rect(0.0, 0.0, cw, ch);
    g.fill_style(css1(p.wall));
    g.fill_rect(0.0, 0.0, cw, ch);
    g.set_transform(k, 0.0, 0.0, k, ox, oy);
    let mut sky = Gradient::linear(0.0, 0.0, 0.0, level.h);
    sky.stop(0.0, css1(mix(p.bg, p.tape, 0.15)));
    sky.stop(
        1.0,
        css1(if p.dark {
            mix(p.tape, BLACK, 0.12)
        } else {
            mix(p.tape, p.wall, 0.15)
        }),
    );
    g.fill_style(sky);
    g.fill_rect(0.0, 0.0, level.w, level.h);
    paint_walls(g, level, &p, false, None);
    for i in 0..level.tiles.len() {
        let t = level.tiles[i];
        if t == 0 || t == T_WALL {
            continue;
        }
        let tx = i % level.cols;
        let ty = (i - tx) / level.cols;
        let (x, y) = (tx as f64 * TILE, ty as f64 * TILE);
        if t == T_LAVA || t == T_WATER || t == T_GOO || t == T_THIN {
            let top = if ty > 0 && level.tiles[i - level.cols] == t {
                0.0
            } else {
                POOL_SINK
            };
            g.fill_style(css1(match t {
                T_LAVA => LAVA_MID,
                T_WATER => WATER_TOP,
                T_GOO => GOO_TOP,
                _ => THIN_TOP,
            }));
            g.fill_rect(x, y + top, TILE + 0.5, TILE - top + 0.5);
        } else if t == T_TRAMP {
            g.fill_style(css1(mix(METAL, BLACK, 0.3)));
            g.fill_rect(x, y + 5.0, TILE + 0.5, TILE - 5.0);
            g.fill_style(css1(TRAMP));
            g.fill_rect(x, y, TILE + 0.5, 6.0 * u.min(2.0));
        } else if t == T_ONEWAY {
            g.fill_style(css1(WOOD));
            g.fill_rect(x, y, TILE + 0.5, ONEWAY_H * u.min(2.0));
        } else if t == T_CRUMBLE {
            g.fill_style(css1(mix(SAND, p.wall, 0.3)));
            g.fill_rect(x + 0.5, y + 0.5, TILE - 1.0, TILE - 1.0);
        } else {
            let sun = t == T_LIGHT;
            let there = sun != level.dark;
            g.fill_style(css(
                if sun { SUN } else { MOON },
                if there { 0.95 } else { 0.3 },
            ));
            g.fill_rect(x + 1.0, y + 1.0, TILE - 2.0, TILE - 2.0);
        }
    }
    let solid = |g: &mut Canvas, x: f64, y: f64, w: f64, h: f64, c: Rgb| {
        g.fill_style(css1(c));
        rr(g, x, y, w, h, 2.0);
        g.fill();
    };
    let dot = |g: &mut Canvas, x: f64, y: f64, r: f64, c: Rgb| {
        g.fill_style(css1(c));
        g.begin_path();
        g.arc(x, y, r, 0.0, TAU, false);
        g.fill();
    };
    let um = u.min(2.0);
    for r in &level.ice {
        solid(g, r.x, r.y, r.w, r.h, ICE_BOT);
    }
    for gt in &level.gates {
        solid(
            g,
            gt.r.x,
            gt.r.y,
            gt.r.w,
            gt.r.h,
            if p.dark {
                mix(METAL, BLACK, 0.2)
            } else {
                METAL
            },
        );
    }
    for l in &level.locks {
        solid(g, l.x, l.y, l.w, l.h, mix(METAL, BLACK, 0.35));
        dot(g, l.x + l.w / 2.0, l.y + l.h / 2.0, 5.0 * um, GOLD);
    }
    for m in &level.movers {
        solid(g, m.r.x, m.r.y, m.r.w, m.r.h, mix(p.wall, METAL, 0.5));
    }
    for b in &level.boxes {
        solid(g, b.x, b.y, TILE, TILE, WOOD);
    }
    for f in &level.fans {
        solid(
            g,
            f.r.x,
            f.r.y + f.r.h - TILE,
            f.r.w,
            TILE,
            mix(METAL, BLACK, 0.3),
        );
    }
    let mut pairs: Vec<&str> = Vec::new();
    for pt in &level.portals {
        let k2 = match pairs.iter().position(|q| *q == pt.pair) {
            Some(k) => k,
            None => {
                pairs.push(&pt.pair);
                pairs.len() - 1
            }
        };
        g.fill_style(css1(PORTAL_COLORS[k2 % PORTAL_COLORS.len()]));
        g.begin_path();
        g.ellipse(
            pt.r.x + pt.r.w / 2.0,
            pt.r.y + pt.r.h / 2.0,
            pt.r.w / 2.0 - 1.0,
            pt.r.h / 2.0 - 1.0,
            0.0,
            0.0,
            TAU,
            false,
        );
        g.fill();
    }
    for pl in &level.plates {
        solid(
            g,
            pl.r.x,
            if pl.ceil {
                pl.r.y
            } else {
                pl.r.y + pl.r.h - 4.0
            },
            pl.r.w,
            4.0,
            rgb_of(channel_color(&pl.ch)),
        );
    }
    for b in &level.buttons {
        solid(
            g,
            b.r.x + 3.0,
            if b.ceil { b.r.y } else { b.r.y + b.r.h - 5.0 },
            b.r.w - 6.0,
            5.0,
            rgb_of(channel_color(&b.ch)),
        );
    }
    for c in &level.cannons {
        let mut len = c.vx.hypot(c.vy);
        if len == 0.0 {
            len = 1.0;
        }
        dot(g, c.x, c.y, 8.0, mix(METAL, BLACK, 0.3));
        dot(
            g,
            c.x + c.vx / len * 8.0,
            c.y + c.vy / len * 8.0,
            5.0,
            mix(METAL, BLACK, 0.3),
        );
    }
    for s in &level.gswitches {
        dot(g, s.r.x + s.r.w / 2.0, s.r.y + s.r.h / 2.0, 6.0 * um, GRAV);
    }
    for l in &level.lamps {
        dot(g, l.r.x + l.r.w / 2.0, l.r.y + l.r.h / 2.0, 6.0 * um, SUN);
    }
    for a in &level.arrows {
        dot(g, a.x, a.y, 5.0 * um, ARROW);
    }
    for key in &level.keys {
        dot(g, key.x, key.y, 5.0 * um, GOLD);
    }
    for c in &level.checkpoints {
        g.fill_style(css1(p.star));
        g.fill_rect(c.x - 1.0, c.y - 24.0, 2.0 * um, 24.0);
        g.begin_path();
        g.move_to(c.x, c.y - 24.0);
        g.line_to(c.x + 12.0 * um, c.y - 19.0);
        g.line_to(c.x, c.y - 14.0);
        g.close_path();
        g.fill();
    }
    for (i, d) in level.doors.iter().enumerate() {
        let c = if i == 0 { p.fire } else { p.frost };
        let e = (u.min(2.5) - 1.0) * 6.0;
        arch_path(
            g,
            d.x - 2.0 - e,
            d.y - 2.0 - 2.0 * e,
            d.w + 4.0 + 2.0 * e,
            d.h + 2.0 + 2.0 * e,
        );
        g.fill_style(css1(c));
        g.fill();
        arch_path(g, d.x + 2.0, d.y + 2.0, d.w - 4.0, d.h - 2.0);
        g.fill_style(css1(mix(c, BLACK, if p.dark { 0.6 } else { 0.35 })));
        g.fill();
    }
    for gm in &level.gems {
        g.save();
        g.translate(gm.x, gm.y);
        let s = 1.4 * u.min(2.5);
        g.scale(s, s);
        gem_path(g, 1.0);
        g.fill_style(css1(if gm.el == Element::Fire {
            p.fire
        } else {
            p.frost
        }));
        g.fill();
        g.restore();
    }
    let hero = |i: usize| {
        (
            level.spawns[i].x + PLAYER_W / 2.0,
            level.spawns[i].y + PLAYER_H - 9.0,
        )
    };
    if level.dark {
        g.save();
        g.begin_path();
        if !level.darks.is_empty() {
            for r in &level.darks {
                g.rect(r.x, r.y, r.w, r.h);
            }
        } else {
            g.rect(-ox / k, -oy / k, cw / k, ch / k);
        }
        g.clip();
        g.fill_style(Col::rgba(7.0, 6.0, 14.0, 0.8));
        g.fill_rect(-ox / k, -oy / k, cw / k, ch / k);
        for i in 0..2 {
            let h = hero(i);
            let r = (if i == 0 { 110.0 } else { 60.0 }) * u.min(1.6);
            let c = if i == 0 { p.fire } else { p.frost };
            let mut gr = Gradient::radial(h.0, h.1, 0.0, h.0, h.1, r);
            gr.stop(0.0, css(c, 0.55));
            gr.stop(1.0, css(c, 0.0));
            g.fill_style(gr);
            g.fill_rect(h.0 - r, h.1 - r, 2.0 * r, 2.0 * r);
        }
        g.restore();
    }
    for i in 0..2 {
        let h = hero(i);
        dot(
            g,
            h.0,
            h.1,
            8.0 * u.min(2.5),
            if i == 0 { p.fire } else { p.frost },
        );
    }
    g.reset_transform();
    let n = stages.min(6);
    if n > 1 {
        let r = 2f64.max(ch * 0.035);
        let gap = r * 3.2;
        let bw = gap * (n - 1) as f64 + r * 4.0;
        let by = ch - r * 2.6;
        g.fill_style(if p.dark {
            Col::rgba(0.0, 0.0, 0.0, 0.6)
        } else {
            Col::rgba(255.0, 255.0, 255.0, 0.78)
        });
        rr(g, cw / 2.0 - bw / 2.0, by - r * 1.8, bw, r * 3.6, r * 1.8);
        g.fill();
        for i in 0..n {
            g.fill_style(if i == 0 {
                css1(p.ink)
            } else {
                css(p.ink, 0.32)
            });
            g.begin_path();
            g.arc(
                cw / 2.0 - gap * (n - 1) as f64 / 2.0 + i as f64 * gap,
                by,
                if i == 0 { r } else { r * 0.8 },
                0.0,
                TAU,
                false,
            );
            g.fill();
        }
    }
}
