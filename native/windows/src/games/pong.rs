//! pong: `pong` then enter (components/PongPanel.tsx, the Mac's PongPanel.swift). The core's `PongSession`
//! (lib/pongSession.ts) runs the game; this feeds it keys and frames and draws what it returns: the court on whole
//! device pixels, the score, the banner.

use qcalc_core::games::Side;
use qcalc_core::games::peer::{PeerEvent, close_reason_text};
use qcalc_core::games::pong::{
    BALL_SIZE, COURT_H, COURT_W, PADDLE_H, PADDLE_INSET, PADDLE_W, Phase, Skill, WIN_SCORE,
};
use qcalc_core::games::pong_session::{PongSession, SessionView};

use super::gfx::{Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::theme::m;
use super::widgets as w;
use super::{Env, GameDef, Play};

const SKILLS: [(&str, Skill); 3] = [("easy", Skill::Easy), ("normal", Skill::Normal), ("hard", Skill::Hard)];

pub static DEF: GameDef = GameDef {
    kind: "pong",
    title: "pong",
    name: "pong",
    blurb: "first to 7.",
    hint: "↵ play pong",
    matches: |t| t == "pong",
    prompt: "play the computer",
    options: &["easy", "normal", "hard"],
    together: &[],
    local: |i, env| Box::new(Pong::new(None, Side::Host, Some(SKILLS[i.min(2)].1), env)),
    online: Some(|role, peer, env| Box::new(Pong::new(Some(peer), role, None, env))),
    sub: super::no_sub,
    footer: None,
    lobby_key: None,
    own_head: false,
};

struct Pong {
    game: PongSession,
    role: Side,
    solo: Option<Skill>,
    peer: String,
    view: SessionView,
    done: bool,
}

impl Pong {
    fn new(peer: Option<String>, role: Side, solo: Option<Skill>, env: &mut Env) -> Pong {
        let mut game = PongSession::with_defaults(role, env.now(), solo);
        let view = game.tick(env.now());
        let mut p = Pong {
            role: game.role,
            game,
            solo,
            peer: peer.unwrap_or_else(|| "computer".into()),
            view,
            done: false,
        };
        p.flush(env);
        p
    }

    fn flush(&mut self, env: &mut Env) {
        env.sent.extend(self.game.take_sent());
    }

    fn finish(&mut self, note: Option<String>, env: &mut Env) {
        if !self.done {
            self.done = true;
            env.end = Some(note.filter(|n| !n.is_empty()));
        }
    }

    /// One frame: the session moves on to now.
    fn frame(&mut self, env: &mut Env) {
        if self.done {
            return;
        }
        let v = self.game.tick(env.now());
        self.flush(env);
        if let Some(ended) = v.ended.clone() {
            self.finish(Some(ended), env);
            return;
        }
        if v.snap.is_some() || self.view.snap.is_none() {
            self.view = v;
        } else {
            self.view.started = v.started;
            self.view.again = v.again;
        }
    }

    fn banner(&self) -> String {
        let v = &self.view;
        let peer = &self.peer;
        if !v.started {
            return format!("waiting for {peer}…");
        }
        let Some(s) = v.snap else {
            return format!("first to {WIN_SCORE}");
        };
        if s.phase == Phase::Over {
            let who = if s.winner == Some(self.role) {
                "you win".to_string()
            } else {
                format!("{peer} wins")
            };
            if self.solo.is_some() {
                return format!("{who} · ↵ play again");
            }
            let tail = if v.again.mine {
                "waiting for a rematch…".to_string()
            } else if v.again.theirs {
                format!("{peer} wants a rematch · ↵")
            } else {
                "↵ rematch".to_string()
            };
            return format!("{who} · {tail}");
        }
        if s.phase == Phase::Serve && s.score[0] + s.score[1] == 0 {
            return format!("first to {WIN_SCORE}");
        }
        String::new()
    }
}

impl Play for Pong {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        self.frame(env);
        if !self.done {
            ui.animate();
        }
        let c = ui.c;
        let mut yy = y;
        let score = self.view.snap.map_or((0, 0), |s| (s.score[0], s.score[1]));
        let me = |side: Side| if side == self.role { "you".to_string() } else { self.peer.clone() };
        let col = |side: Side| if side == self.role { c.accent } else { c.muted };
        yy += w::score(ui, x, yy, width, &me(Side::Host), &me(Side::Guest), score, col(Side::Host), col(Side::Guest));
        yy += m::SP2;
        // the court: full width, or in full screen as wide as leaves room for its height
        let cw = match env.stage {
            Some(s) => width.min(((s.game_h - 160.0) * (COURT_W / COURT_H) as f32).max(0.0)),
            None => width,
        };
        let ch = cw * (COURT_H / COURT_W) as f32;
        let court = Rect::new(x + (width - cw) / 2.0, yy, cw, ch);
        ui.round(court, m::RADIUS_S, c.court());
        let k = cw / COURT_W as f32;
        ui.clip(court);
        ui.push_transform(1.0, court.x, court.y);
        // on whole device pixels, as the page's canvas drew
        let px = ui_scale(ui);
        let rect = |ux: f64, uy: f64, uw: f64, uh: f64| {
            let snap = |v: f64| ((v as f32 * k * px).round()) / px;
            let (x0, y0) = (snap(ux), snap(uy));
            Rect::new(x0, y0, snap(ux + uw) - x0, snap(uy + uh) - y0)
        };
        let mut dy = 4.0;
        while dy < COURT_H {
            ui.fill(rect(COURT_W / 2.0 - 1.0, dy, 2.0, 8.0), c.muted.opacity(0.35));
            dy += 16.0;
        }
        let (ph, pg) = self
            .view
            .snap
            .map_or((COURT_H / 2.0, COURT_H / 2.0), |s| (s.paddles[0], s.paddles[1]));
        for (side, py) in [(Side::Host, ph), (Side::Guest, pg)] {
            let px0 = if side == Side::Host {
                PADDLE_INSET
            } else {
                COURT_W - PADDLE_INSET - PADDLE_W
            };
            let color = if side == self.role { c.accent } else { c.ink };
            ui.fill(rect(px0, py - PADDLE_H / 2.0, PADDLE_W, PADDLE_H), color);
        }
        if let Some(s) = self.view.snap
            && s.phase == Phase::Play
        {
            ui.fill(rect(s.ball.x - BALL_SIZE / 2.0, s.ball.y - BALL_SIZE / 2.0, BALL_SIZE, BALL_SIZE), c.ink);
        }
        ui.pop_transform();
        ui.unclip();
        ui.ring(court, m::RADIUS_S, 1.0, c.line);
        let text = self.banner();
        if !text.is_empty() {
            w::banner(ui, court, &text);
        }
        ui.hit(court, Hit::Board(0));
        yy += ch + m::SP2;
        let skill = self
            .solo
            .and_then(|s| SKILLS.iter().find(|(_, k)| *k == s))
            .map_or(String::new(), |(n, _)| format!("{n} · "));
        yy += w::note(ui, x + width / 2.0, yy, width, &format!("↑↓ or W S to move · {skill}esc leaves"), false);
        let _ = Font::BODY;
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.plain() || k.key == "Escape" {
            return false;
        }
        let l = k.lower();
        if k.key == "ArrowUp" || l == "w" {
            self.game.key(true, k.down);
        } else if k.key == "ArrowDown" || l == "s" {
            self.game.key(false, k.down);
        } else if k.key == "Enter" {
            if k.down && !k.repeat {
                self.game.rematch();
                self.flush(env);
            }
        } else if !k.is_char() && k.key != "Backspace" {
            return false;
        }
        true
    }

    fn heard(&mut self, e: &PeerEvent, env: &mut Env) {
        match e {
            PeerEvent::Message { data } => {
                self.game.receive(data, env.now());
                self.flush(env);
            }
            PeerEvent::Closed { reason } => self.finish(Some(close_reason_text(*reason, "computer")), env),
            _ => {}
        }
    }

    fn release(&mut self) {
        self.game.release();
    }

    fn closing(&mut self, _env: &mut Env) {
        self.game.release();
        self.done = true;
    }
}

/// Device pixels per DIP, from the current transform's scale (1 when unknown).
fn ui_scale(ui: &Ui) -> f32 {
    ui.px_per_dip
}
