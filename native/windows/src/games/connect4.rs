//! connect 4: `connect 4` then enter (components/Connect4Panel.tsx, the .c4-* rules; the Mac's
//! Connect4Panel.swift). The session and the computer are the core's (`Connect4Session`, `connect4_bot`).

use std::time::Instant;

use qcalc_core::games::Side;
use qcalc_core::games::connect4::{COLS, is_over, landing_row};
use qcalc_core::games::connect4_bot::C4Skill;
use qcalc_core::games::connect4_session::{C4View, Connect4Session};
use qcalc_core::games::peer::{PeerEvent, close_reason_text};

use super::gfx::{Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::theme::{Rgba, ease, ease_out, m};
use super::widgets as w;
use super::{Env, GameDef, Play};

const SKILLS: [(&str, C4Skill); 3] = [
    ("easy", C4Skill::Easy),
    ("normal", C4Skill::Normal),
    ("hard", C4Skill::Hard),
];
/// whether the landing preview shows at all; G flips it, and it's kept for next time
const GHOST_KEY: &str = "qcalc-c4-ghost";

pub static DEF: GameDef = GameDef {
    kind: "connect4",
    title: "connect 4",
    name: "connect 4",
    blurb: "four in a row wins.",
    hint: "↵ play connect 4",
    matches: |t| {
        t.strip_prefix("connect")
            .map(|r| r.trim_start_matches([' ', '-']))
            .is_some_and(|r| r == "4" || r == "four")
    },
    prompt: "play the computer",
    options: &["easy", "normal", "hard"],
    together: &[],
    local: |i, env| {
        Box::new(Connect4::new(
            None,
            Side::Host,
            Some(SKILLS[i.min(2)].1),
            env,
        ))
    },
    online: Some(|role, peer, env| Box::new(Connect4::new(Some(peer), role, None, env))),
    sub: super::no_sub,
    footer: None,
    lobby_key: None,
    own_head: false,
};

// the board in svg units: a 40-unit lane on top for the disc you're holding, then the 7×6 grid
const CELL: f32 = 40.0;
const R: f32 = 15.0;
const LANE: f32 = 40.0;
const BW: f32 = 7.0 * CELL;
const BH: f32 = LANE + 6.0 * CELL;

fn cx(col: usize) -> f32 {
    col as f32 * CELL + CELL / 2.0
}
fn cy(row: usize) -> f32 {
    LANE + (5 - row) as f32 * CELL + CELL / 2.0
}

pub(super) fn status_over(
    won: Option<Side>,
    role: Side,
    peer: &str,
    solo: bool,
    mine: bool,
    theirs: bool,
) -> String {
    let who = match won {
        Some(s) if s == role => "you win".to_string(),
        Some(_) => format!("{peer} wins"),
        None => "draw".to_string(),
    };
    if solo {
        return format!("{who} · ↵ play again");
    }
    let tail = if mine {
        "waiting for a rematch…".to_string()
    } else if theirs {
        format!("{peer} wants a rematch · ↵")
    } else {
        "↵ rematch".to_string()
    };
    format!("{who} · {tail}")
}

struct Connect4 {
    game: Connect4Session,
    view: C4View,
    role: Side,
    solo: Option<C4Skill>,
    peer: String,
    done: bool,
    col: usize,
    ghost: bool,
    /// the mouse is over the board, or a column was just picked with the keys; the preview only shows then
    aiming: bool,
    board: Rect,
    t0: Instant,
    // the newest disc's fall, and the held disc's slide (real seconds)
    fell: (Option<usize>, f64),
    held: (Option<usize>, usize, f64),
}

impl Connect4 {
    fn new(peer: Option<String>, role: Side, solo: Option<C4Skill>, env: &mut Env) -> Connect4 {
        let game = Connect4Session::with_defaults(role, env.now(), solo);
        let view = game.view();
        let mut g = Connect4 {
            role: game.role,
            view,
            game,
            solo,
            peer: peer.unwrap_or_else(|| "computer".into()),
            done: false,
            col: 3,
            ghost: env.store.get(GHOST_KEY) != Some("off"),
            aiming: false,
            board: Rect::default(),
            t0: Instant::now(),
            fell: (None, -10.0),
            held: (None, 3, -10.0),
        };
        g.sync(env);
        g
    }

    fn real(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn sync(&mut self, env: &mut Env) {
        env.sent.extend(self.game.take_sent());
        let v = self.game.view();
        if let Some(ended) = v.ended.clone() {
            self.finish(Some(ended), env);
            return;
        }
        if v.rev != self.view.rev {
            self.view = v;
        }
    }

    fn finish(&mut self, note: Option<String>, env: &mut Env) {
        if !self.done {
            self.done = true;
            env.end = Some(note.filter(|n| !n.is_empty()));
        }
    }

    fn play(&mut self, col: usize, env: &mut Env) {
        let _ = self.game.play(col, env.now());
        self.sync(env);
    }

    fn rematch(&mut self, env: &mut Env) {
        self.game.rematch(env.now());
        self.sync(env);
    }

    fn my_turn(&self) -> bool {
        let v = &self.view;
        v.started && !is_over(&v.state) && v.state.turn == self.role
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
                "you go first".into()
            } else {
                format!("{peer} goes first")
            };
        }
        if mine {
            "your turn".into()
        } else {
            format!("{peer}’s turn")
        }
    }

    fn column(&self, x: f32) -> usize {
        (((x - self.board.x) / self.board.w * 7.0).floor().max(0.0) as usize).min(6)
    }

    fn draw_board(&mut self, ui: &mut Ui, r: Rect) {
        let c = ui.c;
        let now = self.real();
        let v = &self.view;
        let s = &v.state;
        // note when the newest disc dropped and where the held one is going
        if s.last != self.fell.0 {
            if s.last.is_some() {
                self.fell.1 = now;
            }
            self.fell.0 = s.last;
        }
        let cursor = self.my_turn().then_some(self.col);
        if cursor != self.held.0 {
            match (cursor, self.held.0) {
                (Some(to), Some(from)) => self.held = (Some(to), from, now),
                (Some(to), None) => self.held = (Some(to), to, -10.0),
                (None, _) => self.held.0 = None,
            }
        }
        if now - self.fell.1 < 0.3 || now - self.held.2 < 0.12 {
            ui.animate();
        }
        let k = r.w / BW;
        ui.push_transform(k, r.x, r.y);
        let frame = Rect::new(0.0, LANE, BW, 6.0 * CELL);
        ui.round(frame, 10.0, c.court());
        ui.outline(frame, 10.0, 1.0, c.line);
        for row in 0..6 {
            for col in 0..COLS {
                ui.circle(cx(col), cy(row), R, c.bg);
                ui.circle_stroke(cx(col), cy(row), R, 1.0, c.line.mix(0.6, Rgba::CLEAR));
            }
        }
        let landing = |col: usize| landing_row(s, col as f64);
        if self.ghost
            && self.aiming
            && let Some(col) = cursor
            && let Some(row) = landing(col)
        {
            ui.circle(cx(col), cy(row), R, c.accent.mix(0.22, Rgba::CLEAR));
        }
        let line: Vec<usize> = s.line.clone().unwrap_or_default();
        let over = is_over(s);
        for (i, cell) in s.board.iter().enumerate() {
            let Some(who) = cell else { continue };
            let (col, row) = (i % COLS, i / COLS);
            let mut y = cy(row);
            if Some(i) == s.last {
                // c4-fall: from the lane down to its hole, 260 ms, cubic-bezier(0.5, 0, 0.75, 0.3)
                let t = ((now - self.fell.1) / 0.26).min(1.0);
                y -= (cy(row) - LANE / 2.0) * (1.0 - ease(t, 0.5, 0.0, 0.75, 0.3) as f32);
            }
            let dim = over && !line.is_empty() && !line.contains(&i);
            let fill = if *who == self.role { c.accent } else { c.ink };
            ui.circle(cx(col), y, R, fill.opacity(if dim { 0.3 } else { 1.0 }));
            if line.contains(&i) {
                ui.circle_stroke(cx(col), y, R - 1.5, 3.0, c.bg);
            }
        }
        if let Some(to) = cursor {
            let full = landing(to).is_none();
            // the held disc slides between columns (transition: cx 90ms ease-out)
            let t = ((now - self.held.2) / 0.09).min(1.0);
            let x = cx(self.held.1) + (cx(to) - cx(self.held.1)) * ease_out(t) as f32;
            ui.circle(
                x,
                LANE / 2.0,
                R,
                c.accent.opacity(if full { 0.25 } else { 0.9 }),
            );
        }
        ui.pop_transform();
    }
}

impl Play for Connect4 {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        let c = ui.c;
        let mut yy = y;
        let v = &self.view;
        let name = |side: Side| {
            if side == self.role {
                "you".to_string()
            } else {
                self.peer.clone()
            }
        };
        let col = |side: Side| if side == self.role { c.accent } else { c.muted };
        yy += w::score(
            ui,
            x,
            yy,
            width,
            &name(Side::Host),
            &name(Side::Guest),
            (v.wins[0], v.wins[1]),
            col(Side::Host),
            col(Side::Guest),
        );
        yy += m::SP2;
        let status = self.status();
        let lit = self.my_turn() || (v.state.winner == Some(self.role));
        let was = if v.started {
            ui.set_alpha(1.0)
        } else {
            let a = w::wait_alpha(ui);
            ui.set_alpha(a)
        };
        let (_, sh) = ui.measure(&status, Font::BODY);
        ui.text_in(
            &status,
            Font::BODY,
            x,
            yy,
            width,
            super::gfx::Align::Center,
            if lit { c.accent } else { c.ink },
        );
        ui.set_alpha(was);
        yy += sh + m::SP2;
        // .c4-board: 300 wide (aspect 1); full screen grows it to min(100%, --game-h − 200)
        let size = match env.stage {
            Some(s) => (s.width - 2.0 * m::GUTTER).min(s.game_h - 200.0).max(300.0),
            None => 300.0,
        };
        let r = Rect::new(x + (width - size) / 2.0, yy, size, size * BH / BW);
        self.board = r;
        self.draw_board(ui, r);
        ui.hit(r, Hit::Board(0));
        yy += r.h + m::SP2;
        let skill = self
            .solo
            .and_then(|s| SKILLS.iter().find(|(_, k)| *k == s))
            .map_or(String::new(), |(n, _)| format!("{n} · "));
        let note = format!(
            "← → or 1–7 to pick · ↵ drop · G {} the preview · {skill}esc leaves",
            if self.ghost { "hides" } else { "shows" }
        );
        yy += w::note(ui, x + width / 2.0, yy, width, &note, false);
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.down || !k.plain() || k.key == "Escape" {
            return false;
        }
        let n = k.key.parse::<usize>().ok().filter(|n| (1..=7).contains(n));
        if k.key == "ArrowLeft" || k.key == "ArrowRight" || n.is_some() {
            self.aiming = true;
        }
        if k.key == "ArrowLeft" {
            self.col = (self.col + 6) % 7;
        } else if k.key == "ArrowRight" {
            self.col = (self.col + 1) % 7;
        } else if let Some(n) = n {
            self.col = n - 1;
        } else if k.lower() == "g" {
            if !k.repeat {
                self.ghost = !self.ghost;
                env.store
                    .set(GHOST_KEY, Some(if self.ghost { "on" } else { "off" }));
            }
        } else if k.key == "Enter" || k.key == " " {
            if !k.repeat {
                if is_over(&self.game.view().state) {
                    self.rematch(env);
                } else {
                    self.play(self.col, env);
                }
            }
        } else if !k.is_char() && k.key != "Backspace" && !k.key.starts_with("Arrow") {
            return false;
        }
        true
    }

    fn pointer(&mut self, _id: u32, x: f32, _y: f32, press: bool, env: &mut Env) {
        let col = self.column(x);
        if press {
            self.play(col, env);
        } else {
            self.col = col;
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
            PeerEvent::Closed { reason } => {
                self.finish(Some(close_reason_text(*reason, "computer")), env)
            }
            _ => {}
        }
    }

    fn closing(&mut self, _env: &mut Env) {
        self.done = true;
    }
}
