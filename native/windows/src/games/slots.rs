//! slots: `slots` then enter (the Mac's Slots/SlotsPanel.swift; native only, no web original). A three-reel,
//! one-line machine played for free chips, kept as `qcalc-slots`. One player, no link and **no lobby**: the window
//! opens on the machine (lobby.rs `direct`), and one esc (or the head's `esc to leave`) goes back to the bar; there
//! is nothing to pause, as the reels never move on their own. The rules and the reels' motion are the core's
//! (`qcalc_core::games::slots`); this draws the reels and takes the keys.
//!
//! A spin or a press samples the core's path for each reel (`slots_path`'s `position`) at 240 a second from now to
//! where it settles: the reels' keyframes, held between samples, as the Mac hands Core Animation. Frames run only
//! while a reel moves, the chips count up or the pay line pulses; the core pays from those frames (the Mac's 30 ms
//! timer), so a standing machine has no timer and draws nothing. A resize, full screen or a theme change mid-spin
//! reads the same keyframes, so the reels roll on from where they are.

use qcalc_core::games::slots::{
    self as core, BLANK, PI, SAVE_KEY, SEVEN, STRIP, SYMBOLS, SlotsEvent, SlotsPhase, SlotsState,
};

use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::theme::{Rgba, ease, m};
use super::widgets as w;
use super::{Env, GameDef, Play, speech};

pub static DEF: GameDef = GameDef {
    kind: "slots",
    title: "slots",
    name: "slots",
    blurb: "",
    hint: "↵ play slots",
    matches: |t| {
        qcalc_core::app::games::game_command(t) == Some(qcalc_core::app::games::GameKind::Slots)
    },
    prompt: "",
    options: &[],
    together: &[],
    local: |_, env| Box::new(Slots::new(env)),
    online: None,
    sub: |_| String::new(),
    footer: None,
    lobby_key: None,
    own_head: false,
};

/// Keyframes a second (SlotsPanel's 240).
const RATE: f64 = 240.0;
const ACT_LESS: u32 = 1;
const ACT_MORE: u32 = 2;

/// Each reel's path from a spin or a press to where it settles, sampled every `step` ms from `start`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub start: f64,
    pub step: f64,
    pub path: [Vec<f64>; 3],
}

impl Plan {
    /// The core's path (`slots_path`) for every reel still moving at `now`, to its settle.
    pub fn sample(s: &SlotsState, now: f64) -> Plan {
        let step = 1000.0 / RATE;
        let path = std::array::from_fn(|i| {
            let end = core::settled_at(s, i);
            if end <= now {
                return Vec::new();
            }
            let count = (((end - now) / step).ceil() as usize + 1).max(2);
            (0..count)
                .map(|k| core::position(s, i, now + step * k as f64))
                .collect()
        });
        Plan {
            start: now,
            step,
            path,
        }
    }

    /// Reel i at `now`: its position (stops) and how blurred it is (0–1, by stops moved per 60th of a second),
    /// held at the last keyframe before `now`; past its keyframes it stands on `rest`.
    pub fn at(&self, i: usize, now: f64, rest: f64) -> (f64, f32) {
        let p = &self.path[i];
        if p.len() < 2 || now < self.start {
            return (rest, 0.0);
        }
        let k = ((now - self.start) / self.step).floor() as usize;
        if k >= p.len() {
            return (rest, 0.0);
        }
        let d = if k == 0 {
            (p[1] - p[0]).abs()
        } else {
            (p[k] - p[k - 1]).abs()
        };
        (p[k], blur(d * (1000.0 / 60.0) / self.step))
    }

    /// When the last reel settles (its last keyframe).
    pub fn end(&self) -> f64 {
        self.path
            .iter()
            .map(|p| self.start + self.step * p.len().saturating_sub(1) as f64)
            .fold(f64::NEG_INFINITY, f64::max)
    }
}

/// The sharp/smeared crossfade for `d` stops moved per 60th of a second.
pub fn blur(d: f64) -> f32 {
    ((d - 0.15) / 0.3).clamp(0.0, 1.0) as f32
}

/// The result under the machine for the last event.
pub fn result_text(e: Option<SlotsEvent>, win: u32) -> String {
    match e {
        Some(SlotsEvent::Big) => format!("jackpot +{win}"),
        Some(SlotsEvent::Win) => format!("+{win}"),
        Some(SlotsEvent::Lose) => "no win".into(),
        Some(SlotsEvent::Refill) => "+100 free chips".into(),
        _ => String::new(),
    }
}

/// How long a win's chips count up (s): quicker for small ones.
pub fn count_secs(gain: u32) -> f64 {
    (0.35 + 0.18 * (f64::from(gain) + 1.0).log2()).min(1.4)
}

/// The symbols on the line for these stops.
fn line_words(reels: [u32; 3]) -> [&'static str; 3] {
    core::line(reels).map(|s| SYMBOLS[s as usize])
}

/// Windows' "Show animations in Windows" off: each reel stands until it lands, no blur, pulse or count-up.
fn reduced_motion() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut on = windows::core::BOOL(1);
    // SAFETY: a live out BOOL
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut on as *mut windows::core::BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && !on.as_bool()
}

struct Slots {
    s: SlotsState,
    plan: Plan,
    last: Option<SlotsEvent>,
    /// game ms of the last result, for its fade in and the pay line's light
    last_at: f64,
    /// the chips shown count up from here to `s.chips` after a win, from `chips_at` (game ms)
    chips_from: u32,
    chips_at: f64,
    reduced: bool,
    pressing: bool,
    /// where the reels were drawn (window DIPs), for a reader
    reels_at: Rect,
}

impl Slots {
    fn new(env: &mut Env) -> Slots {
        let s = core::load(env.store.get(SAVE_KEY));
        Slots {
            chips_from: s.chips,
            s,
            plan: Plan::default(),
            last: None,
            last_at: f64::NEG_INFINITY,
            chips_at: f64::NEG_INFINITY,
            reduced: reduced_motion(),
            pressing: false,
            reels_at: Rect::default(),
        }
    }

    fn save(&self, env: &mut Env) {
        env.store.set(SAVE_KEY, Some(&core::save(&self.s)));
    }

    fn apply(&mut self, (next, event): core::Step, env: &mut Env) {
        let before = self.s.chips;
        self.s = next;
        let now = env.now();
        if event == Some(SlotsEvent::Stop) {
            self.plan = Plan::sample(&self.s, now);
        }
        let Some(e) = event.filter(|e| !matches!(e, SlotsEvent::Spin | SlotsEvent::Stop)) else {
            return;
        };
        self.last = Some(e);
        self.last_at = now;
        if self.s.chips > before && !self.reduced {
            self.chips_from = before;
            self.chips_at = now;
        } else {
            self.chips_from = self.s.chips;
        }
        self.save(env);
        // every result is read out, with the line, even when it's the same words as the last
        let result = result_text(Some(e), self.s.win);
        env.say.push(if e == SlotsEvent::Refill {
            result
        } else {
            speech::slots_line(&line_words(self.s.reels), &result)
        });
    }

    /// Space, ↵ or a click: spins, or while spinning stops the next reel.
    fn spin(&mut self, env: &mut Env) {
        let now = env.now();
        if self.s.phase == SlotsPhase::Spinning {
            let step = core::press(self.s, now);
            self.apply(step, env);
            return;
        }
        self.last = None;
        let step = core::spin(self.s, now, &mut || env.rand());
        let spun = step.1 == Some(SlotsEvent::Spin);
        self.apply(step, env);
        if spun {
            self.plan = Plan::sample(&self.s, now);
            self.chips_from = self.s.chips;
            // the bet is taken now; saved as what the spin will pay, so closing mid-spin loses nothing
            self.save(env);
        }
    }

    fn bet(&mut self, n: i64, env: &mut Env) {
        let next = core::set_bet(self.s, n);
        if next != self.s {
            self.s = next;
            self.save(env);
        }
    }

    fn counting(&self, now: f64) -> bool {
        self.chips_from < self.s.chips
            && now - self.chips_at < count_secs(self.s.chips - self.chips_from) * 1000.0
    }

    fn shown_chips(&self, now: f64) -> u32 {
        if !self.counting(now) {
            return self.s.chips;
        }
        let gain = self.s.chips - self.chips_from;
        let u = ((now - self.chips_at) / (count_secs(gain) * 1000.0)).max(0.0);
        self.chips_from + (f64::from(gain) * ease(u, 0.25, 0.1, 0.25, 1.0)).floor() as u32
    }

    fn won(&self) -> bool {
        self.s.phase == SlotsPhase::Idle
            && matches!(self.last, Some(SlotsEvent::Win | SlotsEvent::Big))
    }

    fn big(&self) -> bool {
        self.s.phase == SlotsPhase::Idle && self.last == Some(SlotsEvent::Big)
    }

    /// Before a result: how to play, or out of chips.
    fn hint(&self) -> &'static str {
        if self.s.phase == SlotsPhase::Spinning {
            "space stops a reel"
        } else if self.s.chips == 0 {
            "space for 100 free chips"
        } else {
            "space or click to spin"
        }
    }

    fn result(&self) -> String {
        if self.s.phase != SlotsPhase::Idle {
            return String::new();
        }
        result_text(self.last, self.s.win)
    }

    /// The reels' value for a reader: the line and the result, or `spinning`.
    fn spoken(&self) -> String {
        if self.s.phase == SlotsPhase::Spinning {
            return "spinning".into();
        }
        speech::slots_line(&line_words(self.s.reels), &self.result())
    }

    /// The machine: three rounded reel windows on a tape-coloured body, the line across them.
    fn machine(&self, ui: &mut Ui, r: Rect, now: f64) {
        let c = ui.c;
        let seven = Rgba::hex(if c.dark { 0xff6b5c } else { 0xd93a2b }, 1.0);
        let gold = Rgba::hex(if c.dark { 0xffd23f } else { 0xe9a800 }, 1.0);
        let color = |sym: u8| match sym {
            SEVEN => seven,
            PI => c.accent,
            core::E => gold,
            _ => c.ink,
        };
        ui.round(r, 14.0, c.bg_tape);
        ui.outline(r, 14.0, 1.0, c.line);
        let (pad, gap) = (10.0, 8.0);
        let reel_w = (r.w - 2.0 * pad - 2.0 * gap) / 3.0;
        let reel_h = r.h - 2.0 * pad;
        let cell = reel_h / 3.0;
        let font = Font::bold((cell * 0.58).round().max(8.0));
        let n = STRIP.len() as i64;
        let mid = r.y + pad + reel_h / 2.0;
        for i in 0..3 {
            let wr = Rect::new(
                r.x + pad + i as f32 * (reel_w + gap),
                r.y + pad,
                reel_w,
                reel_h,
            );
            ui.round(wr, 8.0, c.bg);
            let rest = f64::from(self.s.reels[i]);
            let (pos, blur) = if self.reduced {
                // the reel stands where it was, then shows where it landed
                let p = &self.plan.path[i];
                if now < self.s.land[i] && !p.is_empty() {
                    (p[0], 0.0)
                } else {
                    (rest, 0.0)
                }
            } else {
                self.plan.at(i, now, rest)
            };
            ui.clip(wr);
            let base = pos.floor() as i64;
            for j in (base - 2)..=(base + 3) {
                let sym = STRIP[j.rem_euclid(n) as usize];
                if sym == BLANK {
                    continue;
                }
                let text = SYMBOLS[sym as usize];
                let (tw, th) = ui.measure(text, font);
                // stop j sits (j − pos) cells above the line: a rising position rolls the symbols down
                let cy = mid - (j as f64 - pos) as f32 * cell;
                let x = wr.x + (reel_w - tw) / 2.0;
                let col = color(sym);
                if blur < 1.0 {
                    ui.text(text, font, x, cy - th / 2.0, col.opacity(1.0 - 0.85 * blur));
                }
                if blur > 0.0 {
                    // smeared along the reel: 9 faint copies
                    for g in 0..9 {
                        let k = g as f32 / 8.0 - 0.5;
                        let a = (2.2 / 9.0) * blur;
                        ui.text(
                            text,
                            font,
                            x,
                            cy - th / 2.0 - k * cell * 0.9,
                            col.opacity(a),
                        );
                    }
                }
            }
            // the rows above and below the line shaded, like the curve of a drum
            let bg = c.bg;
            ui.vgradient(
                wr,
                &[
                    (0.0, bg),
                    (0.3, bg.opacity(0.5)),
                    (0.36, bg.opacity(0.0)),
                    (0.64, bg.opacity(0.0)),
                    (0.7, bg.opacity(0.5)),
                    (1.0, bg),
                ],
            );
            // the whole reel dimmed while it rolls, lifted over 180 ms as it lands
            let rolling = self.s.land[i] > self.plan.start;
            if rolling {
                let since = now - self.s.land[i];
                let dim = if since < 0.0 {
                    1.0
                } else if self.reduced {
                    0.0
                } else {
                    (1.0 - since / 180.0).max(0.0) as f32
                };
                if dim > 0.0 {
                    ui.fill(wr, bg.opacity(0.45 * dim));
                }
            }
            ui.unclip();
            ui.outline(wr, 8.0, 1.0, c.line);
        }
        // the pay line: muted, lit on a win (gold for a big one, pulsing 4 times)
        let since = (now - self.last_at) / 1000.0;
        let (col, width) = if self.won() {
            let lit = if self.big() { gold } else { c.accent };
            let fade = if self.reduced {
                1.0
            } else {
                (since / 0.25).clamp(0.0, 1.0) as f32
            };
            let mut col = lit.mix(fade, c.muted.opacity(0.5));
            if self.big() && !self.reduced && since < 0.62 * 4.0 {
                let u = (since % 0.62) / 0.31;
                let k = if u < 1.0 { u } else { 2.0 - u };
                let k = ease(k, 0.42, 0.0, 0.58, 1.0);
                col = col.opacity(1.0 - 0.6 * k as f32);
            }
            (col, 1.5 + 1.5 * fade)
        } else {
            (c.muted.opacity(0.5), 1.5)
        };
        ui.line(
            (r.x + pad - 6.0, mid),
            (r.right() - pad + 6.0, mid),
            width,
            col,
            true,
        );
    }

    fn moving(&self, now: f64) -> bool {
        let since = (now - self.last_at) / 1000.0;
        self.s.phase == SlotsPhase::Spinning
            || now < self.plan.end()
            || self.counting(now)
            || (!self.reduced && since < if self.big() { 2.5 } else { 0.3 })
    }
}

impl Play for Slots {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        let now = env.now();
        if self.s.phase == SlotsPhase::Spinning {
            // the core pays once its clock passes the last stop
            let step = core::tick(self.s, now);
            self.apply(step, env);
        }
        if self.moving(now) {
            ui.animate();
        }
        let c = ui.c;
        let mut yy = y;
        // slots · chips, and the way out, on one line
        let (nw, nh) = ui.measure("slots", Font::bold(m::FS_BODY));
        let hh = nh.max(22.0);
        ui.text(
            "slots",
            Font::bold(m::FS_BODY),
            x,
            yy + (hh - nh) / 2.0,
            c.ink,
        );
        let chips = format!("{} chips", self.shown_chips(now));
        let digits = Font::digits(m::FS_BODY, false);
        let (_, ch) = ui.measure(&chips, digits);
        ui.text(
            &chips,
            digits,
            x + nw + m::SP3,
            yy + (hh - ch) / 2.0,
            c.muted,
        );
        let (_, sh) = ui.measure("esc", Font::SMALL);
        w::esc_button(
            ui,
            x + width + m::SP2,
            yy + (hh - sh - 6.0) / 2.0,
            "esc to leave",
            Hit::Esc,
        );
        yy += hh + m::SP3;
        // the machine
        let mw = match env.stage {
            Some(s) => 260f32.max(
                420f32
                    .min(s.width - 2.0 * m::GUTTER)
                    .min((s.game_h - 200.0) / 0.52),
            ),
            None => 300.0,
        }
        .min(width);
        let mx = x + (width - mw) / 2.0;
        let r = Rect::new(mx, yy, mw, (mw * 0.52).round());
        self.machine(ui, r, now);
        ui.hit(r, Hit::Board(0));
        self.reels_at = r;
        yy += r.h + m::SP3;
        // bet − N + on the left; on the right the result, or a hint before one
        let idle = self.s.phase == SlotsPhase::Idle;
        let row_h = 26.0;
        let body = Font::BODY;
        let (bw, bh) = ui.measure("bet", body);
        ui.text("bet", body, mx, yy + (row_h - bh) / 2.0, c.muted);
        let mut bx = mx + bw + m::SP2;
        for (glyph, act, on) in [
            ("−", ACT_LESS, idle && self.s.bet > core::MIN_BET),
            ("", 0, false),
            ("+", ACT_MORE, idle && self.s.bet < core::MAX_BET),
        ] {
            if act == 0 {
                let n = self.s.bet.to_string();
                let f = Font::digits(m::FS_ROW, true);
                let (tw, th) = ui.measure(&n, f);
                let cw = tw.max(22.0);
                ui.text(&n, f, bx + (cw - tw) / 2.0, yy + (row_h - th) / 2.0, c.ink);
                bx += cw + m::SP2;
                continue;
            }
            let br = Rect::new(bx, yy, 24.0, row_h);
            if on && ui.hover(br) {
                ui.round(br, m::RADIUS_S, c.fill_hover());
            }
            let f = Font::new(m::FS_ROW);
            let (tw, th) = ui.measure(glyph, f);
            ui.text(
                glyph,
                f,
                br.x + (br.w - tw) / 2.0,
                br.y + (row_h - th) / 2.0,
                if on { c.ink } else { c.muted.opacity(0.5) },
            );
            if on {
                ui.hit(br, Hit::Act(act));
            }
            bx += br.w + m::SP2;
        }
        let result = self.result();
        let right = mx + mw;
        let room = (right - bx).max(0.0);
        if result.is_empty() {
            let (_, th) = ui.measure(self.hint(), body);
            ui.text_in(
                self.hint(),
                body,
                bx,
                yy + (row_h - th) / 2.0,
                room,
                Align::Right,
                c.muted,
            );
        } else {
            let col = match self.last {
                Some(SlotsEvent::Big | SlotsEvent::Win | SlotsEvent::Refill) => c.accent,
                _ => c.muted,
            };
            let f = Font::digits(m::FS_BODY, true);
            let (_, th) = ui.measure(&result, f);
            let fade = if self.reduced {
                1.0
            } else {
                ((now - self.last_at) / 200.0).clamp(0.0, 1.0) as f32
            };
            ui.text_in(
                &result,
                f,
                bx,
                yy + (row_h - th) / 2.0,
                room,
                Align::Right,
                col.opacity(fade),
            );
        }
        yy += row_h + m::SP2;
        // what pays, small and quiet
        let pays = core::pay_table()
            .iter()
            .map(|(l, t)| format!("{l} ×{t}"))
            .collect::<Vec<_>>()
            .join("   ");
        let f = Font::digits(m::FS_CAPTION, false);
        let (_, th) = ui.measure(&pays, f);
        ui.text_in(&pays, f, x, yy, width, Align::Center, c.muted);
        yy += th;
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.plain() || k.key == "Escape" {
            return false;
        }
        if !k.down {
            return k.key == " " || k.key == "Enter";
        }
        let bet = i64::from(self.s.bet);
        match k.key.as_str() {
            " " | "Enter" => {
                if !k.repeat {
                    self.spin(env);
                }
            }
            "ArrowLeft" | "ArrowDown" | "-" => self.bet(bet - 1, env),
            "ArrowRight" | "ArrowUp" | "+" | "=" => self.bet(bet + 1, env),
            "0" => self.bet(10, env),
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                self.bet(i64::from(d.as_bytes()[0] - b'0'), env)
            }
            _ => {
                if !k.is_char() && k.key != "Backspace" {
                    return false;
                }
            }
        }
        true
    }

    fn act(&mut self, id: u32, env: &mut Env) {
        let bet = i64::from(self.s.bet);
        match id {
            ACT_LESS => self.bet(bet - 1, env),
            ACT_MORE => self.bet(bet + 1, env),
            _ => {}
        }
    }

    fn pointer(&mut self, _id: u32, _x: f32, _y: f32, press: bool, env: &mut Env) {
        if press && !self.pressing {
            self.pressing = true;
            self.spin(env);
        }
    }

    fn pointer_up(&mut self, _env: &mut Env) {
        self.pressing = false;
    }

    fn idle(&self) -> bool {
        true
    }

    fn parts(&self) -> Vec<super::Part> {
        let state = if self.s.phase == SlotsPhase::Spinning {
            "stops a reel"
        } else {
            "spins"
        };
        vec![
            super::Part::new(1, crate::uia::Role::Button, "reels", self.reels_at)
                .value(&self.spoken())
                .help(&format!("space {state}")),
            super::Part::new(
                2,
                crate::uia::Role::Text,
                &format!("{} chips, bet {}", self.s.chips, self.s.bet),
                Rect::new(
                    self.reels_at.x,
                    self.reels_at.y - 30.0,
                    self.reels_at.w,
                    22.0,
                ),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcalc_core::games::slots::{new_game, spin};

    #[test]
    fn the_bar_names_slots() {
        for t in ["slots", "slot", "slot machine", "one armed bandit"] {
            assert!((DEF.matches)(t), "{t}");
        }
        assert!(!(DEF.matches)("slots 2"));
        assert!(DEF.options.is_empty() && DEF.together.is_empty() && DEF.online.is_none());
    }

    #[test]
    fn keyframes_follow_the_cores_path_and_end_where_it_lands() {
        let (s, _) = spin(new_game(100, 1, 100), 1000.0, &mut || 0.4);
        let plan = Plan::sample(&s, 1000.0);
        for i in 0..3 {
            let p = &plan.path[i];
            assert!(p.len() > 100);
            // held between samples: the keyframe at or before now
            let t = 1000.0 + 123.4;
            let (pos, _) = plan.at(i, t, 0.0);
            let k = ((t - 1000.0) / plan.step).floor();
            assert!((pos - core::position(&s, i, 1000.0 + k * plan.step)).abs() < 1e-9);
            // rolling at speed is fully smeared; the last keyframe is the stop it lands on
            assert_eq!(plan.at(i, 1500.0, 0.0).1, 1.0);
            assert!((p[p.len() - 1] - s.target[i]).abs() < 1e-6);
            let rest = f64::from(s.reels[i]);
            assert_eq!(
                plan.at(i, core::settled_at(&s, i) + 10.0, rest),
                (rest, 0.0)
            );
        }
        assert!((plan.end() - core::settled_at(&s, 2)).abs() <= plan.step + 1e-9);
        // a machine at rest has nothing to play
        let still = Plan::sample(&new_game(100, 1, 100), 0.0);
        assert!(still.path.iter().all(Vec::is_empty));
        assert_eq!(still.at(0, 5.0, 7.0), (7.0, 0.0));
    }

    #[test]
    fn a_rebuild_mid_spin_rolls_on_from_where_the_reels_are() {
        let (s, _) = spin(new_game(100, 1, 100), 0.0, &mut || 0.7);
        let plan = Plan::sample(&s, 0.0);
        // a new plan taken mid-spin (full screen, a resize) starts where the old one is
        let again = Plan::sample(&s, 400.0);
        for i in 0..3 {
            let (a, _) = plan.at(i, 400.0, 0.0);
            let (b, _) = again.at(i, 400.0, 0.0);
            assert!((a - b).abs() < 0.2, "reel {i}: {a} vs {b}");
        }
    }

    #[test]
    fn results_and_counting() {
        assert_eq!(result_text(Some(SlotsEvent::Big), 2500), "jackpot +2500");
        assert_eq!(result_text(Some(SlotsEvent::Win), 5), "+5");
        assert_eq!(result_text(Some(SlotsEvent::Lose), 0), "no win");
        assert_eq!(result_text(Some(SlotsEvent::Refill), 0), "+100 free chips");
        assert_eq!(result_text(None, 0), "");
        assert!((count_secs(0) - 0.35).abs() < 1e-9);
        assert_eq!(count_secs(1_000_000), 1.4);
        assert_eq!(blur(0.0), 0.0);
        assert_eq!(blur(0.5), 1.0);
        assert_eq!(line_words([0, 1, 12]), ["7", "", "π"]);
    }
}
