//! trails: `trails` then enter (components/TrailsPanel.tsx, the Mac's TrailsPanel.swift). The shared lobby picks the
//! computer, two players on one keyboard, or a room online; the core runs the match (`LocalArena`, or the lockstep
//! `TrailsSession` over the link) and the arena below draws it as the page's canvas did, through canvas.rs.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use qcalc_core::games::Side;
use qcalc_core::games::peer::{PeerEvent, close_reason_text};
use qcalc_core::games::peer_online::OnlinePeer;
use qcalc_core::games::trails::arena::{ArenaKey, ArenaView, LocalArena, VS, online_view};
use qcalc_core::games::trails::session::{Role, TrailsSession};
use qcalc_core::games::trails::{COLS, Cycle, is_trails_command};

use super::canvas::{Canvas, Cap, Col, Gradient, Join, Res, js_round};
use super::gfx::{Font, Rect, Ui};
use super::keys::GameKey;
use super::theme::{Rgba, m};
use super::widgets as w;
use super::{Env, GameDef, Play};

const TCOLS: f64 = 64.0;
const TROWS: f64 = 26.0;
const BURST_MS: f64 = 900.0;
const HOT_CELLS: usize = 14;
const LEVELS: [&str; 3] = ["easy", "normal", "hard"];

pub static DEF: GameDef = GameDef {
    kind: "trails",
    title: "trails",
    name: "trails",
    blurb: "leave a trail, don’t hit one. the last one moving takes the round.",
    hint: "↵ play trails",
    matches: is_trails_command,
    prompt: "play the computer",
    options: &["easy", "normal", "hard"],
    together: &["two players (same keyboard)"],
    local: |i, env| Box::new(Trails::local(i, env)),
    online: Some(|role, peer, env| Box::new(Trails::online(role, peer, env))),
    sub: |_| "first to 3 rounds".into(),
    footer: Some(|ui, x, y, width, _env| {
        w::note(
            ui,
            x + width / 2.0,
            y,
            width,
            "vs the computer or online, steer with WASD or the arrows · two players: WASD and arrows",
            false,
        )
    }),
    lobby_key: None,
    own_head: false,
};

enum Game {
    Local(Box<LocalArena>),
    Online {
        session: Box<TrailsSession>,
        out: Arc<Mutex<Vec<String>>>,
        peer: String,
        me: usize,
    },
}

struct Trails {
    game: Game,
    /// the option picked (local), for the head's line
    vs: Option<usize>,
    origin: Instant,
    view: Option<ArenaView>,
    last_frame: f64,
    done: bool,
    res: Option<Res>,
}

impl Trails {
    fn local(option: usize, env: &mut Env) -> Trails {
        let vs = VS[option.min(VS.len() - 1)];
        let seed = (env.rand() * 9e15) as u64 | 1;
        Trails::make(
            Game::Local(Box::new(LocalArena::new(vs, env.now(), seed))),
            Some(option),
        )
    }

    fn online(role: Side, peer: String, _env: &mut Env) -> Trails {
        let out = Arc::new(Mutex::new(Vec::new()));
        let o = out.clone();
        let host = role == Side::Host;
        let mut t = Trails::make(Game::Local(Box::new(LocalArena::new(VS[0], 0.0, 1))), None);
        let session = TrailsSession::new(
            if host { Role::Host } else { Role::Guest },
            Box::new(move |s| {
                if let Ok(mut v) = o.lock() {
                    v.push(s);
                }
            }),
            t.mono(),
        );
        t.game = Game::Online {
            session: Box::new(session),
            out,
            peer,
            me: if host { 0 } else { 1 },
        };
        t
    }

    fn make(game: Game, vs: Option<usize>) -> Trails {
        Trails {
            game,
            vs,
            origin: Instant::now(),
            view: None,
            last_frame: 0.0,
            done: false,
            res: None,
        }
    }

    fn mono(&self) -> f64 {
        self.origin.elapsed().as_secs_f64() * 1000.0
    }

    /// online games run on the monotonic clock, the others on the game clock (which pauses)
    fn clock(&self, env: &Env) -> f64 {
        match self.game {
            Game::Local(_) => env.now(),
            Game::Online { .. } => self.mono(),
        }
    }

    fn flush(&mut self, env: &mut Env) {
        if let Game::Online { out, .. } = &self.game
            && let Ok(mut v) = out.lock()
        {
            env.sent.append(&mut v);
        }
    }

    fn finish(&mut self, note: Option<String>, env: &mut Env) {
        if !self.done {
            self.done = true;
            self.flush(env);
            env.end = Some(note.filter(|n| !n.is_empty()));
        }
    }

    /// One frame of the match.
    fn pump(&mut self, env: &mut Env) {
        if self.done {
            return;
        }
        let now = self.clock(env);
        self.last_frame = self.mono();
        let v = match &mut self.game {
            Game::Local(a) => a.tick(now),
            Game::Online {
                session, peer, me, ..
            } => {
                let mut v = online_view(session.tick(now), *me, peer);
                if v.ended.as_deref() == Some("") {
                    v.to_menu = true;
                }
                v
            }
        };
        self.flush(env);
        if let Some(ended) = v.ended.clone()
            && !v.to_menu
        {
            self.finish(Some(ended), env);
            return;
        }
        if v.to_menu {
            self.finish(None, env);
            return;
        }
        self.view = Some(v);
    }
}

impl Play for Trails {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        self.pump(env);
        if !self.done {
            ui.animate();
        }
        let c = ui.c;
        let mut yy = y;
        if let Some(vs) = self.vs {
            let sub = if vs >= 3 {
                "one keyboard".to_string()
            } else {
                format!("vs the computer · {}", LEVELS[vs])
            };
            yy += w::head(ui, x, yy, width, "trails", &sub, env.shell.fullscreen) + m::SP2;
        }
        let Some(v) = self.view.clone() else {
            return yy - y;
        };
        let rival = rival(c.dark);
        let rival_c = Rgba {
            r: (rival.0 / 255.0) as f32,
            g: (rival.1 / 255.0) as f32,
            b: (rival.2 / 255.0) as f32,
            a: 1.0,
        };
        yy += w::score(
            ui,
            x,
            yy,
            width,
            &v.names[0],
            &v.names[1],
            (v.score[0], v.score[1]),
            c.accent,
            rival_c,
        );
        yy += m::SP2;
        let cw = match env.stage {
            Some(s) => width.min(((s.game_h - 140.0) * 640.0 / 260.0).max(0.0)),
            None => width,
        };
        let ch = cw * 260.0 / 640.0;
        let area = Rect::new(x + (width - cw) / 2.0, yy, cw, ch);
        ui.round(area, m::RADIUS_S, c.court());
        if self.res.is_none() {
            self.res = Some(Res::new(ui));
        }
        if let Some(res) = &self.res {
            ui.clip(area);
            {
                let mut g = Canvas::on(ui, res, area);
                trails_draw(&mut g, &v, c.dark, rgb(c.ink), rgb(c.accent), self.mono());
                g.finish();
            }
            ui.unclip();
            ui.sync_transform();
        }
        if !v.banner.is_empty() {
            let f = if v.big {
                Font::digits(m::FS_DISPLAY, true)
            } else {
                Font::BODY
            };
            let (tw, th) = ui.measure(&v.banner, f);
            let bw = (tw + 2.0 * m::SP3)
                .max(if v.big { 44.0 } else { 0.0 })
                .min(area.w - 2.0 * m::SP3);
            let r = Rect::new(
                area.x + (area.w - bw) / 2.0,
                area.y + (area.h - th - 12.0) / 2.0,
                bw,
                th + 12.0,
            );
            ui.card_shadow(r, m::RADIUS_S);
            ui.round(r, m::RADIUS_S, c.bg.opacity(0.92));
            ui.text_center(&v.banner, f, r.x + r.w / 2.0, r.y + 6.0, c.ink);
        }
        yy += ch + m::SP2;
        yy += w::note(ui, x + width / 2.0, yy, width, &v.note, false);
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.down || !k.plain() || k.key == "Escape" {
            return false;
        }
        let bar =
            k.is_char() || k.key == "Enter" || k.key.starts_with("Arrow") || k.key == "Backspace";
        if !bar {
            return false;
        }
        let key = if k.is_char() {
            k.lower()
        } else {
            k.key.clone()
        };
        let wasd = |s: &str| {
            ["w", "d", "s", "a"]
                .iter()
                .position(|x| *x == s)
                .map(|i| i as u8)
        };
        let arrows = |s: &str| {
            ["ArrowUp", "ArrowRight", "ArrowDown", "ArrowLeft"]
                .iter()
                .position(|x| *x == s)
                .map(|i| i as u8)
        };
        let ak = if key == "Backspace" {
            Some(ArenaKey::Back)
        } else if key == "Enter" {
            Some(ArenaKey::Enter)
        } else if let Some(d) = wasd(&key) {
            Some(ArenaKey::Wasd(d))
        } else {
            arrows(&key).map(ArenaKey::Arrow)
        };
        if let Some(ak) = ak {
            let now = self.clock(env);
            match &mut self.game {
                Game::Local(a) => a.key(ak, k.repeat, now),
                Game::Online { session, .. } => match ak {
                    ArenaKey::Back => session.leave(),
                    ArenaKey::Enter => {
                        if !k.repeat {
                            session.rematch(now);
                        }
                    }
                    ArenaKey::Wasd(d) | ArenaKey::Arrow(d) => {
                        if !k.repeat {
                            session.turn(d);
                        }
                    }
                },
            }
            self.flush(env);
        }
        true
    }

    fn heard(&mut self, e: &PeerEvent, env: &mut Env) {
        match e {
            PeerEvent::Message { data } => {
                let now = self.mono();
                if let Game::Online { session, .. } = &mut self.game {
                    session.receive(data, now);
                }
                self.flush(env);
            }
            PeerEvent::Closed { reason } => {
                self.finish(Some(close_reason_text(*reason, OnlinePeer::PEER_WORD)), env)
            }
            _ => {}
        }
    }

    /// online: a hidden window gets no frames, so a slow tick keeps the link warm (the page's interval)
    fn ticks(&self) -> bool {
        matches!(self.game, Game::Online { .. }) && !self.done
    }

    fn tick(&mut self, env: &mut Env) {
        if self.mono() - self.last_frame > 200.0 {
            self.pump(env);
        }
    }

    fn closing(&mut self, env: &mut Env) {
        if !self.done {
            self.done = true;
            if let Game::Online { session, .. } = &mut self.game {
                session.leave();
            }
            self.flush(env);
        }
    }
}

// ---- the arena on a canvas (TrailsPanel.tsx draw)

type Rgb = (f64, f64, f64);

fn rgb(c: Rgba) -> Rgb {
    (
        f64::from(c.r) * 255.0,
        f64::from(c.g) * 255.0,
        f64::from(c.b) * 255.0,
    )
}

fn rival(dark: bool) -> Rgb {
    if dark {
        (255.0, 154.0, 92.0)
    } else {
        (212.0, 82.0, 15.0)
    }
}

fn trgba(c: Rgb, a: f64) -> Col {
    Col::rgba(c.0, c.1, c.2, a)
}

fn hot(c: Rgb, k: f64) -> Rgb {
    (
        js_round(c.0 + (255.0 - c.0) * k),
        js_round(c.1 + (255.0 - c.1) * k),
        js_round(c.2 + (255.0 - c.2) * k),
    )
}

const DX: [f64; 4] = [0.0, 1.0, 0.0, -1.0];
const DY: [f64; 4] = [-1.0, 0.0, 1.0, 0.0];

fn trails_draw(ctx: &mut Canvas, r: &ArenaView, dark: bool, ink: Rgb, mine: Rgb, now: f64) {
    let (wd, ht) = (ctx.width, ctx.height);
    let dpr = wd / (TCOLS * 10.0);
    let c = wd / TCOLS;
    // (the court under the canvas is the window's: nothing to clear)
    let mut floor = Gradient::radial(wd / 2.0, ht / 2.0, ht * 0.1, wd / 2.0, ht / 2.0, wd * 0.62);
    floor.stop(0.0, trgba(ink, if dark { 0.06 } else { 0.025 }));
    floor.stop(1.0, trgba(ink, if dark { 0.015 } else { 0.06 }));
    ctx.fill_style(floor);
    ctx.begin_path();
    ctx.round_rect(0.0, 0.0, wd, ht, 6.0 * dpr);
    ctx.fill();

    for x in 1..TCOLS as i32 {
        ctx.fill_style(trgba(
            ink,
            if x % 4 != 0 {
                if dark { 0.035 } else { 0.04 }
            } else if dark {
                0.08
            } else {
                0.09
            },
        ));
        ctx.fill_rect(js_round(f64::from(x) * c), 0.0, dpr, ht);
    }
    for y in 1..TROWS as i32 {
        ctx.fill_style(trgba(
            ink,
            if y % 4 != 0 {
                if dark { 0.035 } else { 0.04 }
            } else if dark {
                0.08
            } else {
                0.09
            },
        ));
        ctx.fill_rect(0.0, js_round(f64::from(y) * c), wd, dpr);
    }

    let rival = rival(dark);
    let color_of = |p: usize| if p == 0 { mine } else { rival };

    let mut rim = Gradient::linear(0.0, 0.0, wd, 0.0);
    rim.stop(0.0, trgba(mine, if dark { 0.55 } else { 0.45 }));
    rim.stop(0.5, trgba(ink, if dark { 0.18 } else { 0.15 }));
    rim.stop(1.0, trgba(rival, if dark { 0.55 } else { 0.45 }));
    ctx.stroke_style(rim);
    ctx.set_line_width(1.5 * dpr);
    ctx.begin_path();
    ctx.round_rect(dpr, dpr, wd - 2.0 * dpr, ht - 2.0 * dpr, 6.0 * dpr);
    ctx.stroke();

    let over = r.round.result.is_some();
    let centre = |i: i32| {
        (
            (f64::from(i % COLS) + 0.5) * c,
            (f64::from(i / COLS) + 0.5) * c,
        )
    };
    let head_at = |cy: &Cycle| {
        let glide = if cy.alive && !over { r.t } else { 0.0 };
        let next = cy.queue.first().copied();
        let dir = match next {
            Some(n) if (n + 2) % 4 != cy.dir => n,
            _ => cy.dir,
        } as usize;
        (
            (f64::from(cy.x) + 0.5 + DX[dir] * glide) * c,
            (f64::from(cy.y) + 0.5 + DY[dir] * glide) * c,
        )
    };
    let wall = |ctx: &mut Canvas,
                cy: &Cycle,
                from: usize,
                width: f64,
                style: Col,
                blur: f64,
                glow: Col| {
        let mut pts: Vec<(f64, f64)> = cy.path.iter().skip(from).map(|&i| centre(i)).collect();
        if pts.is_empty() {
            return;
        }
        pts.push(head_at(cy));
        ctx.stroke_style(style);
        ctx.set_line_width(width);
        ctx.set_line_cap(Cap::Round);
        ctx.set_line_join(Join::Round);
        ctx.set_shadow(blur, glow);
        ctx.begin_path();
        for (n, p) in pts.iter().enumerate() {
            if n > 0 {
                ctx.line_to(p.0, p.1);
            } else {
                ctx.move_to(p.0, p.1);
            }
        }
        ctx.stroke();
        ctx.set_shadow(0.0, Col::CLEAR);
    };

    for (p, cy) in r.round.cycles.iter().enumerate() {
        let col = color_of(p);
        let dim = if cy.alive { 1.0 } else { 0.55 };
        wall(
            ctx,
            cy,
            0,
            c * 1.2,
            trgba(col, (if dark { 0.16 } else { 0.12 }) * dim),
            0.0,
            Col::CLEAR,
        );
        wall(
            ctx,
            cy,
            0,
            c * 0.6,
            trgba(col, 0.9 * dim),
            if dark { 10.0 * dpr } else { 4.0 * dpr },
            trgba(col, if dark { 0.8 } else { 0.45 }),
        );
        wall(
            ctx,
            cy,
            0,
            dpr.max(c * 0.18),
            trgba(hot(col, if dark { 0.65 } else { 0.45 }), 0.9 * dim),
            0.0,
            Col::CLEAR,
        );
        if cy.alive {
            wall(
                ctx,
                cy,
                cy.path.len().saturating_sub(HOT_CELLS),
                c * 0.62,
                trgba(hot(col, 0.3), 0.55),
                0.0,
                Col::CLEAR,
            );
        }
    }
    for (p, cy) in r.round.cycles.iter().enumerate() {
        if cy.alive {
            trails_head(ctx, cy, color_of(p), head_at(cy), c, dpr, now, dark);
        }
    }
    for (p, cy) in r.round.cycles.iter().enumerate() {
        if !cy.alive {
            trails_burst(
                ctx,
                cy,
                color_of(p),
                c,
                dpr,
                r.crash_ms.unwrap_or(BURST_MS),
                dark,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn trails_head(
    ctx: &mut Canvas,
    cy: &Cycle,
    col: Rgb,
    at: (f64, f64),
    c: f64,
    dpr: f64,
    now: f64,
    dark: bool,
) {
    let (x, y) = at;
    let pulse = 0.5 + 0.5 * (now / 160.0).sin();
    let mut halo = Gradient::radial(x, y, 0.0, x, y, c * (2.3 + 0.45 * pulse));
    halo.stop(0.0, trgba(col, if dark { 0.55 } else { 0.35 }));
    halo.stop(1.0, trgba(col, 0.0));
    ctx.fill_style(halo);
    ctx.begin_path();
    ctx.arc(
        x,
        y,
        c * (2.3 + 0.45 * pulse),
        0.0,
        std::f64::consts::PI * 2.0,
        false,
    );
    ctx.fill();
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(f64::from(cy.dir) * std::f64::consts::PI / 2.0);
    ctx.set_shadow((if dark { 14.0 } else { 6.0 }) * dpr, trgba(col, 0.9));
    ctx.fill_style(trgba(col, 1.0));
    ctx.begin_path();
    ctx.round_rect(-c * 0.55, -c * 0.8, c * 1.1, c * 1.45, c * 0.5);
    ctx.fill();
    ctx.set_shadow(0.0, Col::CLEAR);
    ctx.fill_style(trgba(hot(col, 0.85), 0.95));
    ctx.begin_path();
    ctx.ellipse(
        0.0,
        -c * 0.42,
        c * 0.28,
        c * 0.22,
        0.0,
        0.0,
        std::f64::consts::PI * 2.0,
        false,
    );
    ctx.fill();
    ctx.restore();
}

fn trails_burst(ctx: &mut Canvas, cy: &Cycle, col: Rgb, c: f64, dpr: f64, ms: f64, dark: bool) {
    let d = cy.dir as usize;
    let bx = (f64::from(cy.x) + 0.5 + DX[d] * 0.5) * c;
    let by = (f64::from(cy.y) + 0.5 + DY[d] * 0.5) * c;
    let k = (ms / BURST_MS).min(1.0);
    let fade = 1.0 - k;
    let eased = 1.0 - (1.0 - k).powi(3);
    let tau = std::f64::consts::PI * 2.0;
    if fade > 0.0 {
        let mut flash = Gradient::radial(bx, by, 0.0, bx, by, c * 3.0);
        flash.stop(0.0, trgba(hot(col, 0.7), 0.9 * fade));
        flash.stop(1.0, trgba(col, 0.0));
        ctx.fill_style(flash);
        ctx.begin_path();
        ctx.arc(bx, by, c * 3.0, 0.0, tau, false);
        ctx.fill();
        ctx.stroke_style(trgba(col, 0.8 * fade));
        ctx.set_line_width(dpr.max(c * 0.18 * fade));
        ctx.begin_path();
        ctx.arc(bx, by, c * (0.6 + 4.5 * eased), 0.0, tau, false);
        ctx.stroke();
    }
    ctx.set_line_cap(Cap::Round);
    for n in 0..14 {
        let seed = ((f64::from(cy.x) * 31.0 + f64::from(cy.y) * 17.0 + f64::from(n) * 7.3)
            * 12.9898)
            .sin()
            * 43758.5453;
        let rnd = seed - seed.floor();
        let a = f64::from(n) / 14.0 * tau + rnd * 0.6;
        let reach = c * (1.6 + rnd * 2.6) * eased;
        let len = c * (0.5 + rnd * 0.6) * fade;
        let (sx, sy) = (bx + a.cos() * reach, by + a.sin() * reach);
        ctx.stroke_style(trgba(
            if n % 3 != 0 { col } else { hot(col, 0.7) },
            fade.max(0.0),
        ));
        ctx.set_line_width(dpr.max(c * 0.12));
        ctx.begin_path();
        ctx.move_to(sx, sy);
        ctx.line_to(sx - a.cos() * len, sy - a.sin() * len);
        ctx.stroke();
    }
    ctx.fill_style(trgba(col, if dark { 0.35 } else { 0.3 }));
    ctx.begin_path();
    ctx.arc(bx, by, c * 0.35, 0.0, tau, false);
    ctx.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_names_trails() {
        assert!((DEF.matches)("trails"));
        assert!((DEF.matches)("cycles"));
        assert!(!(DEF.matches)("trail mix"));
        assert_eq!(hot((0.0, 100.0, 255.0), 0.5), (128.0, 178.0, 255.0));
    }
}
