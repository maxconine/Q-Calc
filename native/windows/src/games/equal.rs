//! equal: `equal` then enter (components/EqualPanel.tsx and EqualPanel.css, the Mac's EqualPanel.swift). Guess
//! the hidden 8-character equation in six. Daily is one puzzle a day for everyone, saved with the streak; practice
//! deals a fresh one each time. The rules are the core's `equal`.

use std::time::Instant;

use qcalc_core::games::equal::{
    self, DailyGame, EqualSave, LEN, Mark, SAVE_KEY, TRIES, current_streak, daily_answer, daily_number,
    guess_problem, key_marks, load_save, record_daily, save_json, score, share_text,
};

use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::theme::{Rgba, ease, keyframes, m};
use super::widgets as w;
use super::{Env, GameDef, Play};

pub static DEF: GameDef = GameDef {
    kind: "equal",
    title: "equal",
    name: "equal",
    blurb: "guess the hidden equation in six tries.",
    hint: "↵ play equal",
    matches: |t| t == "equal" || t == "nerdle",
    prompt: "",
    options: &["daily", "practice"],
    together: &[],
    local: |i, env| Box::new(Equal::new(i == 1, env)),
    online: None,
    sub: super::no_sub,
    footer: None,
    lobby_key: None,
    own_head: false,
};

const KEY_ROWS: [&str; 2] = ["1234567890", "+-*/="];
const ACT_BACK: u32 = 8;
const ACT_ENTER: u32 = 13;
const ACT_COPY: u32 = 100;
const ACT_PRACTICE: u32 = 101;
const ACT_NEXT: u32 = 102;
const ACT_DAILY_TAB: u32 = 200;
const ACT_PRACTICE_TAB: u32 = 201;

fn glyph(c: char) -> char {
    match c {
        '*' => '×',
        '/' => '÷',
        '-' => '−',
        c => c,
    }
}

/// Today in local time, as `dateKey` writes it (yyyy-mm-dd).
pub fn today_key() -> String {
    // SAFETY: a plain query
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    equal::date_key(i64::from(t.wYear), i64::from(t.wMonth), i64::from(t.wDay))
}

struct Equal {
    practice_mode: bool,
    save: EqualSave,
    today: String,
    practice: (String, Vec<String>),
    input: String,
    note: String,
    // the current row shakes on a refused guess; the row just entered flips over (real seconds)
    shake_at: f64,
    fresh: Option<usize>,
    fresh_at: f64,
    copied: bool,
    t0: Instant,
}

impl Equal {
    fn new(practice: bool, env: &mut Env) -> Equal {
        Equal {
            practice_mode: practice,
            save: load_save(env.store.get(SAVE_KEY)),
            today: today_key(),
            practice: (equal::generate(&mut || env.rand()), Vec::new()),
            input: String::new(),
            note: String::new(),
            shake_at: -10.0,
            fresh: None,
            fresh_at: -10.0,
            copied: false,
            t0: Instant::now(),
        }
    }

    fn real(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn daily(&self) -> bool {
        !self.practice_mode
    }

    fn answer(&self) -> String {
        if self.daily() {
            daily_answer(&self.today)
        } else {
            self.practice.0.clone()
        }
    }

    fn guesses(&self) -> Vec<String> {
        if self.daily() {
            match &self.save.daily {
                Some(d) if d.date == self.today => d.guesses.clone(),
                _ => Vec::new(),
            }
        } else {
            self.practice.1.clone()
        }
    }

    fn won(&self) -> bool {
        self.guesses().contains(&self.answer())
    }

    fn over(&self) -> bool {
        self.won() || self.guesses().len() >= TRIES
    }

    fn title(&self) -> String {
        if self.daily() {
            format!("equal #{}", daily_number(&self.today))
        } else {
            "equal practice".into()
        }
    }

    fn switch_to(&mut self, practice: bool) {
        self.practice_mode = practice;
        self.input.clear();
        self.note.clear();
        self.fresh = None;
        self.copied = false;
        // past midnight, daily moves on to the new day's puzzle
        if !practice {
            self.today = today_key();
        }
    }

    fn next_practice(&mut self, env: &mut Env) {
        self.practice = (equal::generate(&mut || env.rand()), Vec::new());
        self.input.clear();
        self.note.clear();
        self.fresh = None;
        self.copied = false;
    }

    fn submit(&mut self, env: &mut Env) {
        if self.over() {
            return;
        }
        if let Some(problem) = guess_problem(&self.input) {
            self.note = problem.to_string();
            self.shake_at = self.real();
            return;
        }
        let mut next = self.guesses();
        next.push(std::mem::take(&mut self.input));
        self.fresh = Some(next.len() - 1);
        self.fresh_at = self.real();
        self.note.clear();
        if !self.daily() {
            self.practice.1 = next;
            return;
        }
        let answer = self.answer();
        let done = next.contains(&answer) || next.len() >= TRIES;
        let stats = if done {
            record_daily(&self.save.stats, &self.today, &next, &answer)
        } else {
            self.save.stats.clone()
        };
        let saved = EqualSave {
            daily: Some(DailyGame {
                date: self.today.clone(),
                guesses: next,
            }),
            stats,
        };
        env.store.set(SAVE_KEY, Some(&save_json(&saved)));
        self.save = saved;
    }

    fn press(&mut self, k: &str, env: &mut Env) {
        if k == "Enter" {
            if self.over() && !self.daily() {
                self.next_practice(env);
            } else {
                self.submit(env);
            }
        } else if self.over() {
            if self.daily() {
                self.note = "today’s is done · P for practice".into();
            }
        } else if k == "Backspace" {
            self.input.pop();
            self.note.clear();
        } else if self.input.chars().count() < LEN {
            self.input.push_str(k);
        }
    }

    fn copy(&mut self, env: &mut Env) {
        env.clipboard = Some(share_text(&self.guesses(), &self.answer(), &self.title()));
        self.copied = true;
    }

    fn colors(dark: bool) -> (Rgba, Rgba, Rgba) {
        if dark {
            (Rgba::hex(0x3f9e64, 1.0), Rgba::hex(0xc7a33a, 1.0), Rgba::hex(0x545458, 1.0))
        } else {
            (Rgba::hex(0x2e8b57, 1.0), Rgba::hex(0xc9a227, 1.0), Rgba::hex(0x8e8e93, 1.0))
        }
    }

    fn fill(mark: Mark, dark: bool) -> Rgba {
        let (hit, near, miss) = Self::colors(dark);
        match mark {
            Mark::Hit => hit,
            Mark::Near => near,
            Mark::Miss => miss,
        }
    }

    fn head(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32) -> f32 {
        let c = ui.c;
        let streak = current_streak(&self.save.stats, &self.today);
        let (nw, nh) = ui.measure("equal", Font::bold(m::FS_BODY));
        let h = nh.max(22.0);
        ui.text("equal", Font::bold(m::FS_BODY), x, y + (h - nh) / 2.0, c.ink);
        let sub = if self.daily() {
            format!("#{} · streak {streak}", daily_number(&self.today))
        } else {
            "practice".into()
        };
        let (_, sh) = ui.measure(&sub, Font::SMALL);
        ui.text(&sub, Font::SMALL, x + nw + m::SP3, y + (h - sh) / 2.0 + 1.0, c.muted);
        let esc_w = w::esc_button(ui, x + width + m::SP2, y + (h - sh - 6.0) / 2.0, "esc", Hit::Esc);
        // the daily / practice switch
        let labels = [("daily", false, ACT_DAILY_TAB), ("practice", true, ACT_PRACTICE_TAB)];
        let sizes: Vec<(f32, f32)> = labels.iter().map(|(t, _, _)| ui.measure(t, Font::CAPTION)).collect();
        let seg_w: f32 = sizes.iter().map(|s| s.0 + 2.0 * m::SP2).sum::<f32>() + 4.0;
        let seg_h = sizes[0].1 + 4.0 + 4.0;
        let right = x + width + m::SP2 - esc_w - (m::SP3 - m::SP2);
        let seg = Rect::new(right - seg_w, y + (h - seg_h) / 2.0, seg_w, seg_h);
        ui.round(seg, m::RADIUS_S, c.fill_track());
        let mut sx = seg.x + 2.0;
        for ((t, p, act), (tw, th)) in labels.iter().zip(&sizes) {
            let on = self.practice_mode == *p;
            let r = Rect::new(sx, seg.y + 2.0, tw + 2.0 * m::SP2, th + 4.0);
            if on {
                ui.control_shadow(r, 4.0);
                ui.round(r, 4.0, c.control);
            }
            ui.text(t, Font::CAPTION, r.x + m::SP2, r.y + 2.0, if on { c.ink } else { c.muted });
            ui.hit(r, Hit::Act(*act));
            sx += r.w;
        }
        h
    }

    fn grid(&mut self, ui: &mut Ui, cx: f32, y: f32, dark: bool) -> f32 {
        let c = ui.c;
        let now = self.real();
        if now - self.shake_at.max(self.fresh_at) < 1.2 {
            ui.animate();
        }
        let guesses = self.guesses();
        let over = self.over();
        let answer = self.answer();
        let (tw, th, gap) = (32.0, 34.0, 5.0);
        let row_w = LEN as f32 * tw + (LEN as f32 - 1.0) * gap;
        let f = Font::digits(m::FS_ROW, true);
        let mut yy = y;
        for r in 0..TRIES {
            let done = r < guesses.len();
            let current = r == guesses.len() && !over;
            let text: Vec<char> = if done {
                guesses[r].chars().collect()
            } else if current {
                self.input.chars().collect()
            } else {
                Vec::new()
            };
            let marks = done.then(|| score(&guesses[r], &answer));
            // equal-shake: 0.36 s through ±6, ±4, 3 px
            let dx = if current {
                keyframes((now - self.shake_at) / 0.36, &[(0.0, 0.0), (0.2, -6.0), (0.4, 6.0), (0.6, -4.0), (0.8, 3.0), (1.0, 0.0)], |k| {
                    ease(k, 0.25, 0.1, 0.25, 1.0)
                }) as f32
            } else {
                0.0
            };
            let mut x = cx - row_w / 2.0 + dx;
            for i in 0..LEN {
                let ch = text.get(i).copied();
                let mark = marks.as_ref().and_then(|ms| ms.get(i).copied());
                // equal-flip: rotateX 90° → 0, 0.42 s, held at 90° until its delay
                let flip = if Some(r) == self.fresh && mark.is_some() {
                    ((now - self.fresh_at - i as f64 * 0.07) / 0.42).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let squash = (std::f64::consts::FRAC_PI_2 * (1.0 - ease(flip, 0.25, 0.1, 0.25, 1.0))).cos() as f32;
                let hh = th * squash;
                let tile = Rect::new(x, yy + (th - hh) / 2.0, tw, hh);
                match mark {
                    Some(mk) => ui.round(tile, m::RADIUS_S, Self::fill(mk, dark)),
                    None => {
                        let (ring, wd) = if ch.is_some() {
                            (c.ink.mix(0.4, Rgba::CLEAR), 1.5)
                        } else {
                            (c.ink.mix(0.14, Rgba::CLEAR), 1.0)
                        };
                        ui.ring(tile, m::RADIUS_S, wd, ring);
                    }
                }
                if let Some(ch) = ch
                    && hh > 4.0
                {
                    let s = glyph(ch).to_string();
                    let (gw, gh) = ui.measure(&s, f);
                    ui.push_transform(1.0, 0.0, 0.0);
                    ui.text(&s, f, x + (tw - gw) / 2.0, yy + (th - gh) / 2.0, if mark.is_some() { Rgba::WHITE } else { c.ink });
                    ui.pop_transform();
                }
                x += tw + gap;
            }
            yy += th + if r + 1 < TRIES { gap } else { 0.0 };
        }
        yy - y
    }

    fn keypad(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, dark: bool) -> f32 {
        let c = ui.c;
        let cx = x + width / 2.0;
        let guesses = self.guesses();
        let marks = key_marks(&guesses, &self.answer());
        let left = TRIES - guesses.len();
        let mut yy = y;
        let line = if self.note.is_empty() {
            format!("{left} {} left", if left == 1 { "guess" } else { "guesses" })
        } else {
            self.note.clone()
        };
        let (_, lh) = ui.measure(&line, Font::SMALL);
        ui.text_center(&line, Font::SMALL, cx, yy, c.muted);
        yy += lh.max(16.0) + m::SP2;
        let f = Font::bold(m::FS_BODY);
        for (ri, row) in KEY_ROWS.iter().enumerate() {
            let mut keys: Vec<(String, u32, Option<Mark>, bool)> = row
                .chars()
                .map(|k| (glyph(k).to_string(), k as u32, marks.get(&k).copied(), false))
                .collect();
            if ri == 1 {
                keys.push(("⌫".into(), ACT_BACK, None, true));
                keys.push(("↵".into(), ACT_ENTER, None, true));
            }
            let widths: Vec<f32> = keys
                .iter()
                .map(|(l, _, _, wide)| (ui.measure(l, f).0 + 2.0 * m::SP1).max(if *wide { 46.0 } else { 30.0 }))
                .collect();
            let total: f32 = widths.iter().sum::<f32>() + 5.0 * (keys.len() as f32 - 1.0);
            let mut kx = cx - total / 2.0;
            for ((label, act, mark, _), kw) in keys.iter().zip(&widths) {
                let r = Rect::new(kx, yy, *kw, 30.0);
                let hover = ui.hover(r);
                let was = ui.set_alpha(if *mark == Some(Mark::Miss) { 0.7 } else { 1.0 });
                match mark {
                    Some(mk) => ui.round(r, m::RADIUS_S, Self::fill(*mk, dark)),
                    None => {
                        ui.control_shadow(r, m::RADIUS_S);
                        ui.round(r, m::RADIUS_S, if hover { c.control.mix(0.92, c.ink) } else { c.control });
                    }
                }
                let (lw, lh) = ui.measure(label, f);
                ui.text(label, f, r.x + (r.w - lw) / 2.0, r.y + (r.h - lh) / 2.0, if mark.is_some() { Rgba::WHITE } else { c.ink });
                ui.set_alpha(was);
                ui.hit(r, Hit::Act(*act));
                kx += kw + 5.0;
            }
            yy += 30.0 + 5.0;
        }
        yy += m::SP2 - 5.0;
        yy += w::note(ui, cx, yy, width, "green: right spot · yellow: elsewhere · grey: not in it · D daily · P practice · esc leaves", false);
        yy - y
    }

    fn summary(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32) -> f32 {
        let c = ui.c;
        let cx = x + width / 2.0;
        let guesses = self.guesses();
        let stats = &self.save.stats;
        let streak = current_streak(stats, &self.today);
        let answer: String = self.answer().chars().map(glyph).collect();
        let pct = if stats.played > 0 {
            (f64::from(stats.wins) / f64::from(stats.played) * 100.0).round() as u32
        } else {
            0
        };
        let mut yy = y;
        let lead = if self.won() {
            format!("got it in {}", guesses.len())
        } else {
            "out of guesses".to_string()
        };
        let tone = if self.won() { c.accent } else { c.ink };
        let first = format!("{lead} · the answer was ");
        let runs = [
            w::Run { text: &first, hit: None, ink: false, bold: false },
            w::Run { text: &answer, hit: None, ink: true, bold: true },
        ];
        yy += w::runs(ui, cx, yy, Font::digits(m::FS_BODY, false), tone, &runs) + m::SP2;
        if self.daily() {
            let t = format!(
                "streak {streak} · best {} · played {} · {pct}% won · next one tomorrow",
                stats.best, stats.played
            );
            yy += w::note(ui, cx, yy, width, &t, false) + m::SP2;
        }
        let copy = if self.copied { "copied" } else { "copy result" };
        let items = if self.daily() {
            vec![
                w::ButtonSpec { title: copy, kbd: Some("C".into()), on: false, hit: Hit::Act(ACT_COPY) },
                w::ButtonSpec { title: "practice", kbd: Some("P".into()), on: false, hit: Hit::Act(ACT_PRACTICE) },
            ]
        } else {
            vec![
                w::ButtonSpec { title: copy, kbd: Some("C".into()), on: false, hit: Hit::Act(ACT_COPY) },
                w::ButtonSpec { title: "next puzzle", kbd: Some("↵".into()), on: false, hit: Hit::Act(ACT_NEXT) },
            ]
        };
        yy += w::button_row(ui, cx, yy, width, &items, false);
        if !self.note.is_empty() {
            yy += m::SP2 + w::note(ui, cx, yy + m::SP2, width, &self.note.clone(), false);
        }
        yy + m::SP1 - y
    }
}

impl Play for Equal {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, _env: &mut Env) -> f32 {
        // left open past midnight, daily moves on by itself once the board is empty or done
        if self.daily() && (self.guesses().is_empty() || self.over()) {
            self.today = today_key();
        }
        let dark = ui.c.dark;
        let mut yy = y;
        yy += self.head(ui, x, yy, width);
        yy += m::SP3;
        yy += self.grid(ui, x + width / 2.0, yy, dark);
        yy += m::SP2;
        yy += if self.over() {
            self.summary(ui, x, yy, width)
        } else {
            self.keypad(ui, x, yy, width, dark)
        };
        let _ = Align::Left;
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.down || !k.plain() || k.key == "Escape" {
            return false;
        }
        let key = match k.key.as_str() {
            "x" | "X" | "×" => "*",
            "÷" => "/",
            other => other,
        };
        let typing = key.chars().count() == 1 && "0123456789+-*/=".contains(key);
        if typing || key == "Backspace" || key == "Enter" {
            if !(key == "Enter" && k.repeat) {
                self.press(key, env);
            }
        } else if key == "d" || key == "D" {
            self.switch_to(false);
        } else if key == "p" || key == "P" {
            self.switch_to(true);
        } else if (key == "c" || key == "C") && self.over() {
            // C copies and N deals again only once a game is over
            self.copy(env);
        } else if (key == "n" || key == "N") && self.over() && !self.daily() {
            self.next_practice(env);
        } else if !k.is_char() && !key.starts_with("Arrow") {
            return false;
        }
        true
    }

    fn act(&mut self, id: u32, env: &mut Env) {
        match id {
            ACT_BACK => self.press("Backspace", env),
            ACT_ENTER => self.press("Enter", env),
            ACT_COPY => self.copy(env),
            ACT_PRACTICE | ACT_PRACTICE_TAB => self.switch_to(true),
            ACT_DAILY_TAB => self.switch_to(false),
            ACT_NEXT => self.next_practice(env),
            c => {
                if let Some(ch) = char::from_u32(c) {
                    self.press(&ch.to_string(), env);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_is_a_date_key() {
        let t = today_key();
        assert_eq!(t.len(), 10);
        assert_eq!(&t[4..5], "-");
        assert_eq!(glyph('*'), '×');
    }
}
