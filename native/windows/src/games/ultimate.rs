//! ultimate tic-tac-toe: `ultimate` then enter (components/UltimatePanel.tsx and the .ut-* rules; the Mac's
//! UltimatePanel.swift). Connect 4's panel and status line on a chalkboard: hand-drawn lines (the wobble is in the
//! paths, from the core's chalk port) in neon chalk, you green and them orange. Only the newest mark and the boards
//! you can play in animate, once.

use std::collections::HashMap;
use std::time::Instant;

use qcalc_core::games::Side;
use qcalc_core::games::peer::{PeerEvent, close_reason_text};
use qcalc_core::games::ultimate::{BoardMark, can_play, is_over};
use qcalc_core::games::ultimate_bot::UtSkill;
use qcalc_core::games::ultimate_chalk::{chalk_box, chalk_line, chalk_mark};
use qcalc_core::games::ultimate_session::{UltimateSession, UtView};

use super::connect4::status_over;
use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::svg::{self, Poly};
use super::theme::{Rgba, ease_out, m};
use super::widgets as w;
use super::{Env, GameDef, Play};

const SKILLS: [(&str, UtSkill); 3] = [
    ("easy", UtSkill::Easy),
    ("normal", UtSkill::Normal),
    ("hard", UtSkill::Hard),
];

pub static DEF: GameDef = GameDef {
    kind: "ultimate",
    title: "ultimate tic-tac-toe",
    name: "ultimate",
    blurb: "win three small boards in a row.",
    hint: "↵ play ultimate tic-tac-toe",
    matches: |t| {
        if t == "uttt" || t == "ultimate" {
            return true;
        }
        let Some(rest) = t.strip_prefix("ultimate") else {
            return false;
        };
        let sep = |s: &str| s.trim_start_matches([' ', '-']).to_string();
        let rest = sep(rest);
        if rest == "ttt" {
            return true;
        }
        rest.strip_prefix("tic")
            .map(sep)
            .and_then(|r| r.strip_prefix("tac").map(sep))
            .is_some_and(|r| r == "toe")
    },
    prompt: "play the computer",
    options: &["easy", "normal", "hard"],
    together: &[],
    local: |i, env| {
        Box::new(Ultimate::new(
            None,
            Side::Host,
            Some(SKILLS[i.min(2)].1),
            env,
        ))
    },
    online: Some(|role, peer, env| Box::new(Ultimate::new(Some(peer), role, None, env))),
    sub: super::no_sub,
    footer: None,
    lobby_key: None,
    own_head: false,
};

// geometry in svg units: nine 3×3 boards on a slate, the big # between them
const CELL: f64 = 30.0;
const BOARD: f64 = 90.0;
const GAP: f64 = 14.0;
const PAD: f64 = 10.0;
const GW: f64 = 3.0 * BOARD + 2.0 * GAP;
const SPAN: f64 = GW + 2.0 * PAD;
const CENTRE: usize = 4;

fn bx(b: usize) -> f64 {
    (b % 3) as f64 * (BOARD + GAP)
}
fn by(b: usize) -> f64 {
    (b / 3) as f64 * (BOARD + GAP)
}
fn cx(c: usize) -> f64 {
    bx(c / 9) + (c % 9 % 3) as f64 * CELL
}
fn cy(c: usize) -> f64 {
    by(c / 9) + (c % 9 / 3) as f64 * CELL
}
// the 9×9 grid the arrows move over, and back to a cell
fn grid_row(c: usize) -> usize {
    (c / 9) / 3 * 3 + (c % 9) / 3
}
fn grid_col(c: usize) -> usize {
    (c / 9) % 3 * 3 + (c % 9) % 3
}
fn at_grid(row: usize, col: usize) -> usize {
    (row / 3 * 3 + col / 3) * 9 + (row % 3) * 3 + col % 3
}

/// The host plays x and the guest o, whoever goes first.
fn mark_of(who: Side) -> char {
    if who == Side::Host { 'x' } else { 'o' }
}

// the chalk colours: the slate is dark in both themes, so they're fixed; the names above follow the theme
const CHALK: Rgba = Rgba::hex(0xefece2, 1.0);
const MINE: Rgba = Rgba::hex(0x6ee69e, 1.0);
const THEIRS: Rgba = Rgba::hex(0xff9a5c, 1.0);

fn theirs_name(dark: bool) -> Rgba {
    if dark {
        Rgba::hex(0xff9a5c, 1.0)
    } else {
        Rgba::hex(0xd4520f, 1.0)
    }
}

/// The cell under a point on the board (svg units from the slate's corner), or None in a gap or off it.
fn cell_at(x: f64, y: f64) -> Option<usize> {
    let x = x - PAD;
    let y = y - PAD;
    let col = (x / (BOARD + GAP)).floor();
    let row = (y / (BOARD + GAP)).floor();
    if !(0.0..=2.0).contains(&col) || !(0.0..=2.0).contains(&row) {
        return None;
    }
    let ix = x - col * (BOARD + GAP);
    let iy = y - row * (BOARD + GAP);
    if ix >= BOARD || iy >= BOARD || ix < 0.0 || iy < 0.0 {
        return None;
    }
    Some(
        ((row * 3.0 + col) as usize) * 9
            + (iy / CELL).floor() as usize * 3
            + (ix / CELL).floor() as usize,
    )
}

struct Ultimate {
    game: UltimateSession,
    view: UtView,
    role: Side,
    solo: Option<UtSkill>,
    peer: String,
    done: bool,
    cursor: usize,
    /// free to play anywhere: the board 1–9 picked, so the next digit picks a cell in it
    picked: Option<usize>,
    aiming: bool,
    moved: usize,
    board: Rect,
    t0: Instant,
    // the newest mark and when, and when your turn came (real seconds), for the draw-ins
    last: (Option<usize>, f64),
    live: (bool, f64),
    lines: Vec<(Poly, bool)>,
    paths: HashMap<(u8, usize), Poly>,
}

impl Ultimate {
    fn new(peer: Option<String>, role: Side, solo: Option<UtSkill>, env: &mut Env) -> Ultimate {
        let game = UltimateSession::with_defaults(role, env.now(), solo);
        let view = game.view();
        // the slate's lines never change, so their paths are made once
        let mut lines = Vec::new();
        for k in [1.0, 2.0] {
            let p = k * (BOARD + GAP) - GAP / 2.0;
            lines.push((
                svg::parse(&chalk_line((p, -4.0), (p, GW + 4.0), 100.0 + k, 1.6)),
                true,
            ));
            lines.push((
                svg::parse(&chalk_line((-4.0, p), (GW + 4.0, p), 200.0 + k, 1.6)),
                true,
            ));
        }
        for b in 0..9 {
            for k in [1.0, 2.0] {
                let s = (b * 10) as f64 + k;
                lines.push((
                    svg::parse(&chalk_line(
                        (bx(b) + k * CELL, by(b) + 5.0),
                        (bx(b) + k * CELL, by(b) + BOARD - 5.0),
                        s,
                        1.1,
                    )),
                    false,
                ));
                lines.push((
                    svg::parse(&chalk_line(
                        (bx(b) + 5.0, by(b) + k * CELL),
                        (bx(b) + BOARD - 5.0, by(b) + k * CELL),
                        s + 5.0,
                        1.1,
                    )),
                    false,
                ));
            }
        }
        let mut u = Ultimate {
            role: game.role,
            moved: view.state.moves,
            view,
            game,
            solo,
            peer: peer.unwrap_or_else(|| "computer".into()),
            done: false,
            cursor: CENTRE * 9 + CENTRE,
            picked: None,
            aiming: false,
            board: Rect::default(),
            t0: Instant::now(),
            last: (None, -10.0),
            live: (false, -10.0),
            lines,
            paths: HashMap::new(),
        };
        u.sync(env);
        u
    }

    fn real(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn sync(&mut self, env: &mut Env) {
        env.sent.extend(self.game.take_sent());
        let v = self.game.view();
        if let Some(ended) = v.ended.clone() {
            if !self.done {
                self.done = true;
                env.end = Some(Some(ended).filter(|n| !n.is_empty()));
            }
            return;
        }
        if v.rev == self.view.rev {
            return;
        }
        // each move: the board pick is spent, and the cursor jumps to the board the next move is sent to
        if v.state.moves != self.moved {
            self.moved = v.state.moves;
            self.picked = None;
            if let Some(f) = v.state.forced {
                self.cursor = f * 9 + self.cursor % 9;
            }
        }
        self.view = v;
    }

    fn my_turn(&self) -> bool {
        let v = &self.view;
        v.started && !is_over(&v.state) && v.state.turn == self.role
    }

    fn play(&mut self, cell: usize, env: &mut Env) {
        let _ = self.game.play(cell, env.now());
        self.sync(env);
    }

    fn status(&self) -> String {
        let v = &self.view;
        let s = &v.state;
        let peer = &self.peer;
        if !v.started {
            return format!("waiting for {peer}…");
        }
        if is_over(s) {
            return status_over(
                s.winner,
                self.role,
                peer,
                self.solo.is_some(),
                v.again.mine,
                v.again.theirs,
            );
        }
        if v.thinking {
            return if s.moves == 0 {
                "the computer goes first…".into()
            } else {
                "the computer is thinking…".into()
            };
        }
        let mine = self.my_turn();
        if s.moves == 0 {
            return if mine {
                format!("you go first · {}", mark_of(self.role))
            } else {
                format!("{peer} goes first")
            };
        }
        if mine {
            if s.forced.is_none() {
                "your turn · play anywhere".into()
            } else {
                "your turn".into()
            }
        } else {
            format!("{peer}’s turn")
        }
    }

    fn path(&mut self, kind: u8, i: usize, make: impl FnOnce() -> String) -> Poly {
        self.paths
            .entry((kind, i))
            .or_insert_with(|| svg::parse(&make()))
            .clone()
    }

    fn draw(&mut self, ui: &mut Ui, r: Rect) {
        let now = self.real();
        let live = self.my_turn();
        if self.view.state.last != self.last.0 {
            self.last = (self.view.state.last, now);
        }
        if live != self.live.0 {
            self.live = (live, if live { now } else { self.live.1 });
        }
        if now - self.last.1 < 1.2 || now - self.live.1 < 0.5 {
            ui.animate();
        }
        // the slate: dark in both themes, a soft light top left and a shadow bottom right, and the inset shading
        ui.clip(r);
        ui.round(r, 14.0, Rgba::hex(0x1f2a25, 1.0));
        ui.glow(
            r.x + r.w * 0.85,
            r.y + r.h * 0.95,
            r.w * 0.9,
            r.h * 0.7,
            Rgba::hex(0, 0.28),
            0.6,
        );
        ui.glow(
            r.x + r.w * 0.3,
            r.y + r.h * 0.15,
            r.w * 1.2,
            r.h * 0.9,
            Rgba::hex(0xffffff, 0.06),
            0.6,
        );
        for (i, a) in [(1.0, 0.12), (3.0, 0.07), (6.0, 0.04)] {
            ui.ring(
                Rect::new(r.x, r.y + 2.0, r.w, r.h),
                14.0,
                i,
                Rgba::hex(0, a),
            );
        }
        ui.unclip();
        ui.ring(r, 14.0, 1.0, Rgba::hex(0xffffff, 0.06));
        let k = r.w / SPAN as f32;
        ui.push_transform(k, r.x + PAD as f32 * k, r.y + PAD as f32 * k);
        let v = self.view.clone();
        let s = &v.state;
        // the dust and the grid
        for i in 0..70 {
            let f = f64::from(i);
            let h = (f * 12.9898).sin() * 43758.5453;
            let kk = (f * 78.233).sin() * 12345.678;
            let frac = |v: f64| v - v.floor();
            let x = frac(h) * SPAN - PAD;
            let y = frac(kk) * SPAN - PAD;
            let rad = 0.4 + (((h * 7.0) % 1.0 + 1.0) % 1.0) * 0.9;
            ui.circle(x as f32, y as f32, rad as f32, CHALK.opacity(0.07));
        }
        for (p, big) in &self.lines {
            ui.poly(
                p,
                if *big { 3.2 } else { 1.4 },
                CHALK.opacity(if *big { 0.85 } else { 0.3 }),
            );
        }
        let zone = s.forced.or(self.picked);
        // the boards you may play in, chalked round as your turn comes
        if live {
            let t = ease_out(((now - self.live.1) / 0.38).min(1.0)) as f32;
            for b in 0..9 {
                if !qcalc_core::games::ultimate::board_open(s, b) {
                    continue;
                }
                let p = self.path(0, b, || {
                    chalk_box(
                        bx(b) - 4.0,
                        by(b) - 4.0,
                        BOARD + 8.0,
                        BOARD + 8.0,
                        (300 + b) as f64,
                    )
                });
                let on = zone == Some(b);
                ui.poly(
                    &p.trim(t),
                    if on { 2.4 } else { 1.5 },
                    MINE.opacity(if on { 1.0 } else { 0.45 }),
                );
            }
        }
        let last = s.last;
        // where the last move sent you, as a dotted chalk arc
        if live
            && let (Some(l), Some(f)) = (last, s.forced)
            && l / 9 != f
        {
            let (x1, y1) = (cx(l) + CELL / 2.0, cy(l) + CELL / 2.0);
            let (x2, y2) = (bx(f) + BOARD / 2.0, by(f) + BOARD / 2.0);
            let bend = 0.18;
            let ctrl = (
                (x1 + x2) / 2.0 - (y2 - y1) * bend,
                (y1 + y2) / 2.0 + (x2 - x1) * bend,
            );
            let o = 0.6 * ease_out(((now - self.last.1 - 0.15) / 0.3).clamp(0.0, 1.0));
            ui.dashed_curve(
                (x1 as f32, y1 as f32),
                (ctrl.0 as f32, ctrl.1 as f32),
                (x2 as f32, y2 as f32),
                1.6,
                0.5,
                6.0,
                MINE.opacity(o as f32),
            );
        }
        if self.my_turn() && self.aiming {
            let c = self.cursor;
            let ok = can_play(s, c as f64, self.role);
            let rr = Rect::new(
                (cx(c) + 3.0) as f32,
                (cy(c) + 3.0) as f32,
                (CELL - 6.0) as f32,
                (CELL - 6.0) as f32,
            );
            ui.round(
                rr,
                7.0,
                if ok {
                    MINE.opacity(0.2)
                } else {
                    CHALK.opacity(0.06)
                },
            );
        }
        // the pieces
        let role = self.role;
        let tone = |who: Side| if who == role { MINE } else { THEIRS };
        let since = now - self.last.1;
        let ink = |ui: &mut Ui,
                   p: &Poly,
                   color: Rgba,
                   glow: f32,
                   glow_op: f32,
                   core: f32,
                   dim: bool,
                   fresh: Option<(f64, f64)>| {
            let mut t = 1.0;
            if let Some((dur, delay)) = fresh {
                t = ease_out(((since - delay) / dur).clamp(0.0, 1.0));
            }
            if t <= 0.0 {
                return;
            }
            let p = p.trim(t as f32);
            let was = ui.set_alpha(if dim { 0.22 } else { 1.0 });
            ui.poly(&p, glow, color.opacity(glow_op));
            ui.poly(&p, core, color);
            ui.set_alpha(was);
        };
        for b in 0..9 {
            if let Some(BoardMark::Won(_)) = s.boards[b] {
                ui.ellipse(
                    (bx(b) + BOARD / 2.0) as f32,
                    (by(b) + BOARD / 2.0) as f32,
                    (BOARD / 2.0) as f32,
                    (BOARD / 2.3) as f32,
                    CHALK.opacity(0.045),
                );
            }
        }
        for i in 0..81 {
            let Some(who) = s.cells[i] else { continue };
            let kind = mark_of(who);
            let p = self.path(if kind == 'x' { 1 } else { 2 }, i, || {
                chalk_mark(kind, cx(i), cy(i), CELL, i as f64)
            });
            let fresh = (Some(i) == last).then_some((0.32, 0.0));
            ink(
                ui,
                &p,
                tone(who),
                7.0,
                0.2,
                2.7,
                s.boards[i / 9].is_some(),
                fresh,
            );
        }
        let won: Vec<usize> = s.line.clone().unwrap_or_default();
        for b in 0..9 {
            let Some(BoardMark::Won(who)) = s.boards[b] else {
                continue;
            };
            let kind = mark_of(who);
            let p = self.path(if kind == 'x' { 3 } else { 4 }, b, || {
                chalk_mark(kind, bx(b), by(b), BOARD, (50 + b) as f64)
            });
            // a decided board holding the last move was decided by it
            let fresh = last.is_some_and(|l| l / 9 == b).then_some((0.56, 0.12));
            ink(
                ui,
                &p,
                tone(who),
                16.0,
                0.16,
                6.5,
                !won.is_empty() && !won.contains(&b),
                fresh,
            );
        }
        if won.len() == 3
            && let Some(winner) = s.winner
        {
            let (a, z) = (won[0], won[2]);
            let (ax, ay) = (bx(a) + BOARD / 2.0, by(a) + BOARD / 2.0);
            let (zx, zy) = (bx(z) + BOARD / 2.0, by(z) + BOARD / 2.0);
            let ext = 0.18;
            let p = svg::parse(&chalk_line(
                (ax - (zx - ax) * ext, ay - (zy - ay) * ext),
                (zx + (zx - ax) * ext, zy + (zy - ay) * ext),
                999.0,
                2.5,
            ));
            ink(
                ui,
                &p,
                tone(winner),
                14.0,
                0.3,
                4.0,
                false,
                Some((0.5, 0.65)),
            );
        }
        ui.pop_transform();
    }
}

impl Play for Ultimate {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, _env: &mut Env) -> f32 {
        let c = ui.c;
        let mut yy = y;
        let v = &self.view;
        let who = |side: Side| {
            if side == self.role {
                "you".to_string()
            } else {
                self.peer.clone()
            }
        };
        let tone = |side: Side| {
            if side == self.role {
                c.accent
            } else {
                theirs_name(c.dark)
            }
        };
        yy += w::score(
            ui,
            x,
            yy,
            width,
            &format!("{} · x", who(Side::Host)),
            &format!("o · {}", who(Side::Guest)),
            (v.wins[0], v.wins[1]),
            tone(Side::Host),
            tone(Side::Guest),
        );
        yy += m::SP2;
        let status = self.status();
        let lit = self.my_turn() || v.state.winner == Some(self.role);
        let was = if v.started {
            ui.set_alpha(1.0)
        } else {
            let a = w::wait_alpha(ui);
            ui.set_alpha(a)
        };
        let f = Font::chalk(12.0);
        let (_, sh) = ui.measure(&status, f);
        ui.text_in(
            &status,
            f,
            x,
            yy,
            width,
            Align::Center,
            if lit { c.accent } else { c.ink },
        );
        ui.set_alpha(was);
        yy += sh + m::SP2;
        let size = 340.0;
        let r = Rect::new(x + (width - size) / 2.0, yy, size, size);
        self.board = r;
        self.draw(ui, r);
        ui.hit(r, Hit::Board(0));
        yy += size + m::SP2;
        let zone = self.view.state.forced.or(self.picked);
        let skill = self
            .solo
            .and_then(|s| SKILLS.iter().find(|(_, k)| *k == s))
            .map_or(String::new(), |(n, _)| format!("{n} · "));
        let lead = if self.my_turn() && zone.is_none() {
            "1–9 pick a board"
        } else {
            "1–9 or arrows pick a cell"
        };
        yy += w::note(
            ui,
            x + width / 2.0,
            yy,
            width,
            &format!("{lead} · ↵ play · {skill}esc leaves"),
            false,
        );
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.down || !k.plain() || k.key == "Escape" {
            return false;
        }
        let state = self.game.view().state;
        let n = k
            .key
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=9).contains(n))
            .map(|n| n - 1);
        let step = match k.key.as_str() {
            "ArrowUp" => Some((8, 0)),
            "ArrowDown" => Some((1, 0)),
            "ArrowLeft" => Some((0, 8)),
            "ArrowRight" => Some((0, 1)),
            _ => None,
        };
        if let Some((dr, dc)) = step {
            self.cursor = at_grid(
                (grid_row(self.cursor) + dr) % 9,
                (grid_col(self.cursor) + dc) % 9,
            );
            self.aiming = true;
        } else if let Some(n) = n {
            if let Some(zone) = state.forced.or(self.picked) {
                self.cursor = zone * 9 + n;
            } else if qcalc_core::games::ultimate::board_open(&state, n) {
                self.picked = Some(n);
                self.cursor = n * 9 + CENTRE;
            }
            self.aiming = true;
        } else if k.key == "Backspace" {
            self.picked = None;
        } else if k.key == "Enter" || k.key == " " {
            if !k.repeat {
                if is_over(&state) {
                    self.game.rematch(env.now());
                    self.sync(env);
                } else if !self.aiming {
                    // a hidden cursor shows first, so ↵ never plays a cell you can't see
                    self.aiming = true;
                } else {
                    self.play(self.cursor, env);
                }
            }
        } else if !k.is_char() {
            return false;
        }
        true
    }

    fn pointer(&mut self, _id: u32, x: f32, y: f32, press: bool, env: &mut Env) {
        let k = f64::from(self.board.w) / SPAN;
        let Some(cell) = cell_at(
            f64::from(x - self.board.x) / k,
            f64::from(y - self.board.y) / k,
        ) else {
            return;
        };
        if press {
            self.play(cell, env);
        } else {
            self.cursor = cell;
            self.aiming = true;
        }
    }

    fn pointer_left(&mut self) {
        self.aiming = false;
    }

    fn ticks(&self) -> bool {
        !self.done
    }

    fn tick(&mut self, env: &mut Env) {
        let _ = self.game.tick(env.now());
        self.sync(env);
    }

    fn heard(&mut self, e: &PeerEvent, env: &mut Env) {
        match e {
            PeerEvent::Message { data } => {
                self.game.receive(data, env.now());
                self.sync(env);
            }
            PeerEvent::Closed { reason } if !self.done => {
                self.done = true;
                env.end = Some(Some(close_reason_text(*reason, "computer")));
            }
            _ => {}
        }
    }

    fn closing(&mut self, _env: &mut Env) {
        self.done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_round_trips_and_clicks_find_cells() {
        for c in 0..81 {
            assert_eq!(at_grid(grid_row(c), grid_col(c)), c);
        }
        // the top-left cell, a gap, the centre board's centre
        assert_eq!(cell_at(PAD + 1.0, PAD + 1.0), Some(0));
        assert_eq!(cell_at(PAD + BOARD + 2.0, PAD + 1.0), None);
        let mid = PAD + BOARD + GAP + BOARD / 2.0;
        assert_eq!(cell_at(mid, mid), Some(4 * 9 + 4));
        assert!(
            !svg::parse(&chalk_mark('x', 0.0, 0.0, CELL, 1.0))
                .parts
                .is_empty()
        );
    }
}
