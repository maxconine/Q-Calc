//! crack: `crack` then enter (components/CrackPanel.tsx and CrackPanel.css, the Mac's CrackPanel.swift). One
//! player, no link: the shared lobby, then the dial; esc pauses, then leaves. The rules are the core's `crack`.

use std::time::Instant;

use qcalc_core::games::crack::{
    BEST_KEY, CrackEvent, CrackPhase, CrackState, HIT_WINDOW, Step, parse_best, press, start_level,
    tick,
};

use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::store::GameStore;
use super::theme::{Rgba, ease, ease_out, keyframes, m};
use super::widgets as w;
use super::{Env, GameDef, Play};

pub static DEF: GameDef = GameDef {
    kind: "crack",
    title: "crack",
    name: "crack",
    blurb: "tap as the needle crosses the dot. each level spins faster.",
    hint: "↵ crack the safe",
    matches: |t| {
        if t == "crack" || t == "crack the safe" {
            return true;
        }
        let Some(rest) = t.strip_prefix("safe") else {
            return false;
        };
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        matches!(rest, "crack" | "cracker" | "cracking")
    },
    prompt: "",
    options: &["play"],
    together: &[],
    local: |_, env| Box::new(Crack::new(env)),
    online: None,
    sub: |s| format!("best level {}", best(s)),
    footer: None,
    lobby_key: None,
    own_head: false,
};

fn best(s: &GameStore) -> u32 {
    parse_best(s.get(BEST_KEY))
}

struct Crack {
    s: CrackState,
    best: u32,
    /// the last popped dot and when (real seconds), so each pop plays its animation
    pop: Option<(f64, f64)>,
    /// when the phase last changed, in real time, for the shake, the click and the colour fades
    phase_at: f64,
    last_phase: CrackPhase,
    pressing: bool,
    t0: Instant,
    dial: Rect,
}

impl Crack {
    fn new(env: &mut Env) -> Crack {
        let best = best(&env.store);
        let now = env.now();
        let s = start_level(best.max(1), now, &mut || env.rand(), 0.0, 1.0);
        Crack {
            last_phase: s.phase,
            s,
            best,
            pop: None,
            phase_at: 0.0,
            pressing: false,
            t0: Instant::now(),
            dial: Rect::default(),
        }
    }

    fn real(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn apply(&mut self, (next, event): Step, env: &mut Env) {
        let prev = self.s;
        self.s = next;
        if matches!(event, Some(CrackEvent::Hit | CrackEvent::Clear)) {
            self.pop = Some((prev.dot, self.real()));
        }
        if self.s.level > prev.level && self.s.level > best(&env.store) {
            env.store.set(BEST_KEY, Some(&self.s.level.to_string()));
            self.best = self.best.max(self.s.level);
        }
        if self.s.phase != self.last_phase {
            self.last_phase = self.s.phase;
            self.phase_at = self.real();
        }
    }

    fn frame(&mut self, env: &mut Env) {
        if self.s.phase == CrackPhase::Ready {
            return;
        }
        let now = env.now();
        let step = tick(self.s, now, &mut || env.rand());
        self.apply(step, env);
    }

    fn hit(&mut self, env: &mut Env) {
        let now = env.now();
        let step = press(self.s, now, &mut || env.rand());
        self.apply(step, env);
    }

    fn restart(&mut self, env: &mut Env) {
        self.pop = None;
        let now = env.now();
        let s = start_level(1, now, &mut || env.rand(), 0.0, 1.0);
        self.apply((s, None), env);
    }

    fn note(&self) -> String {
        let restart = if self.s.level > 1 {
            " · 1 back to level 1"
        } else {
            ""
        };
        if self.s.phase == CrackPhase::Ready {
            format!("space, ↵ or a click starts the needle{restart} · esc leaves")
        } else {
            format!("press as the needle crosses the dot{restart} · esc leaves")
        }
    }

    /// The dial in its 200-unit box (the svg of CrackPanel.tsx), with its CSS animations.
    fn dial(&self, ui: &mut Ui, r: Rect) {
        let c = ui.c;
        let s = self.s;
        let now = self.real();
        let since = now - self.phase_at;
        let k = r.w / 200.0;
        const C: f32 = 100.0;
        const TRACK: f32 = 76.0;
        const TICK: f32 = 58.0;
        let dot_r = (76.0 * HIT_WINDOW * 0.85).round() as f32;
        let dot = if c.dark {
            Rgba::hex(0xffd23f, 1.0)
        } else {
            Rgba::hex(0xe9a800, 1.0)
        };
        let miss = if c.dark {
            Rgba::hex(0xff6b5c, 1.0)
        } else {
            Rgba::hex(0xd93a2b, 1.0)
        };
        // a miss shakes the whole dial (crack-shake: 0.4 s, ease-in-out)
        let shake = if s.phase == CrackPhase::Fail {
            keyframes(
                since / 0.4,
                &[
                    (0.0, 0.0),
                    (0.15, -7.0),
                    (0.35, 6.0),
                    (0.55, -4.0),
                    (0.75, 2.0),
                    (1.0, 0.0),
                ],
                |t| ease(t, 0.42, 0.0, 0.58, 1.0),
            ) as f32
        } else {
            0.0
        };
        ui.push_transform(k, r.x + shake * k, r.y);
        let p = |a: f64, rad: f32| (C + rad * a.sin() as f32, C - rad * a.cos() as f32);
        ui.circle(C, C, TRACK + 18.0, c.court());
        ui.circle_stroke(C, C, TRACK + 18.0, 1.0, c.line);
        // the track's colour fades over 0.2 s into the phase's
        let base = c.ink.mix(0.11, Rgba::CLEAR);
        let lit = match s.phase {
            CrackPhase::Fail => miss.mix(0.45, Rgba::CLEAR),
            CrackPhase::Clear => c.accent.mix(0.55, Rgba::CLEAR),
            _ => base,
        };
        let fade = (since / 0.2).min(1.0) as f32;
        ui.circle_stroke(C, C, TRACK, 26.0, lit.mix(fade, base));
        // the tick ring; a clear turns it a click, like a tumbler giving
        let turned = s.phase == CrackPhase::Clear;
        if turned {
            let turn = 27.0 * ease((since / 0.5).min(1.0), 0.3, 1.6, 0.5, 1.0);
            ui.push_rotation(turn as f32, C, C);
        }
        for i in 0..40 {
            let a = f64::from(i) / 40.0 * std::f64::consts::TAU;
            let major = i % 5 == 0;
            ui.line(
                p(a, TICK),
                p(a, TICK - if major { 7.0 } else { 3.5 }),
                if major { 1.6 } else { 1.0 },
                c.muted.opacity(if major { 0.9 } else { 0.55 }),
                true,
            );
        }
        if turned {
            ui.pop_transform();
        }
        if s.phase != CrackPhase::Clear {
            let (dx, dy) = p(s.dot, TRACK);
            ui.circle(dx, dy, dot_r, dot);
        }
        if let Some((angle, at)) = self.pop {
            let t = (now - at) / 0.35;
            if t < 1.0 {
                let e = ease_out(t) as f32;
                let rr = dot_r * (1.0 + 1.2 * e);
                let (dx, dy) = p(angle, TRACK);
                ui.circle_stroke(dx, dy, rr, 3.0 * (1.0 + 1.2 * e), dot.opacity(1.0 - e));
            }
        }
        let needle = match s.phase {
            CrackPhase::Fail => miss,
            CrackPhase::Clear => c.accent,
            _ => c.ink,
        };
        ui.line(
            p(s.angle, TRACK - 13.0),
            p(s.angle, TRACK + 13.0),
            5.0,
            needle,
            true,
        );
        let big = if s.phase == CrackPhase::Clear {
            "open".to_string()
        } else {
            s.level.to_string()
        };
        let small = match s.phase {
            CrackPhase::Clear => format!("level {} cracked", s.level),
            CrackPhase::Fail => "missed".to_string(),
            _ => format!("{} to go", s.left),
        };
        let bf = Font::digits(30.0, true);
        let (_, bh) = ui.measure(&big, bf);
        ui.text_in(
            &big,
            bf,
            0.0,
            C + 4.0 - bh * 0.8,
            200.0,
            Align::Center,
            if turned { c.accent } else { c.ink },
        );
        let sf = Font::new(11.0);
        let (_, sh) = ui.measure(&small, sf);
        ui.text_in(
            &small,
            sf,
            0.0,
            C + 24.0 - sh * 0.8,
            200.0,
            Align::Center,
            if s.phase == CrackPhase::Fail {
                miss
            } else {
                c.muted
            },
        );
        ui.pop_transform();
    }

    fn moving(&self) -> bool {
        let now = self.real();
        self.s.phase != CrackPhase::Ready
            || self.pop.is_some_and(|(_, at)| now - at < 0.4)
            || now - self.phase_at < 0.6
    }
}

impl Play for Crack {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        self.frame(env);
        if self.moving() {
            ui.animate();
        }
        let c = ui.c;
        let mut yy = y;
        // the head: crack, the best, esc
        let (nw, nh) = ui.measure("crack", Font::bold(m::FS_BODY));
        let hh = nh.max(22.0);
        ui.text(
            "crack",
            Font::bold(m::FS_BODY),
            x,
            yy + (hh - nh) / 2.0,
            c.ink,
        );
        let sub = format!("best level {}", self.best);
        let (_, sh) = ui.measure(&sub, Font::SMALL);
        ui.text(
            &sub,
            Font::SMALL,
            x + nw + m::SP3,
            yy + (hh - sh) / 2.0 + 1.0,
            c.muted,
        );
        w::esc_button(
            ui,
            x + width + m::SP2,
            yy + (hh - sh - 6.0) / 2.0,
            "esc",
            Hit::Esc,
        );
        yy += hh + m::SP2;
        let size = match env.stage {
            Some(s) => (s.width - 2.0 * m::GUTTER).min(s.game_h - 160.0).max(200.0),
            None => 200.0,
        };
        let r = Rect::new(x + (width - size) / 2.0, yy, size, size);
        self.dial = r;
        self.dial(ui, r);
        ui.hit(r, Hit::Board(0));
        yy += size + m::SP1;
        yy += w::note(ui, x + width / 2.0, yy, width, &self.note(), false);
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.plain() || k.key == "Escape" {
            return false;
        }
        if !k.down {
            return k.key == " " || k.key == "Enter";
        }
        if k.key == " " || k.key == "Enter" {
            if !k.repeat {
                self.hit(env);
            }
        } else if k.key == "1" && !k.repeat {
            // back to the first level; the best stays
            self.restart(env);
        } else if !k.is_char() && k.key != "Backspace" {
            return false;
        }
        true
    }

    fn pointer(&mut self, _id: u32, _x: f32, _y: f32, press: bool, env: &mut Env) {
        if press && !self.pressing {
            self.pressing = true;
            self.hit(env);
        }
    }

    fn pointer_up(&mut self, _env: &mut Env) {
        self.pressing = false;
    }

    fn idle(&self) -> bool {
        false
    }
}
