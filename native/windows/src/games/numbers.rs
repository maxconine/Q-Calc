//! The numbers game: `24 game` or `countdown` then enter (components/NumbersPanel.tsx and NumbersPanel.css, the
//! Mac's NumbersPanel.swift). The lobby picks the mode (1 or 2) and countdown's large numbers (← →); rounds run
//! against the clock. The rules are the core's `numbers`; the faint parens are the bar's own (`parens`).

use qcalc_core::games::numbers::{
    self, Mode, NumbersStats, STATS_KEY, check_expr, countdown_points, deal24, deal_countdown, frac_text,
    load_stats, record_countdown, record24, solves24, stats_json, used_tiles,
};
use qcalc_core::parens::{autofill_parens, fill_parens, infer_parens};

use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::store::GameStore;
use super::theme::{Rgba, m};
use super::widgets::{self as w, Run};
use super::{Env, GameDef, Play};

const SECONDS_24: f64 = 60.0;
const SECONDS_COUNTDOWN: f64 = 45.0;
const BAD: Rgba = Rgba::hex(0xd9534f, 1.0);
const ACT_MENU: u32 = 100;
const ACT_GIVE_UP: u32 = 101;

pub static DEF: GameDef = GameDef {
    kind: "numbers",
    title: "24 / countdown",
    name: "24 / countdown",
    blurb: "make the number, against the clock.",
    hint: "↵ play 24 or countdown",
    // ^(?:24 ?game|game of 24|count ?down|numbers game|24 ?\/ ?countdown)$
    matches: |t| {
        matches!(
            t,
            "24game"
                | "24 game"
                | "game of 24"
                | "countdown"
                | "count down"
                | "numbers game"
                | "24/countdown"
                | "24 /countdown"
                | "24/ countdown"
                | "24 / countdown"
        )
    },
    prompt: "",
    options: &["24", "countdown"],
    together: &[],
    local: |i, env| Box::new(Numbers::new(i == 1, env)),
    online: None,
    sub: super::no_sub,
    footer: Some(footer),
    lobby_key: Some(lobby_key),
    own_head: false,
};

fn load(store: &GameStore) -> NumbersStats {
    load_stats(store.get(STATS_KEY))
}

fn save(store: &mut GameStore, s: &NumbersStats) {
    store.set(STATS_KEY, Some(&stats_json(s)));
}

fn footer(ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
    let large = load(&env.store).large;
    let t = format!(
        "24: four numbers, each once, make exactly 24 in {}s · countdown: six numbers, get near the target in {}s, dealing {} large · ← → changes",
        SECONDS_24, SECONDS_COUNTDOWN, large
    );
    w::note(ui, x + width / 2.0, y, width, &t, false)
}

fn lobby_key(k: &GameKey, env: &mut Env) -> bool {
    if k.key != "ArrowLeft" && k.key != "ArrowRight" {
        return false;
    }
    let mut s = load(&env.store);
    s.large = (s.large + if k.key == "ArrowLeft" { -1.0 } else { 1.0 }).clamp(0.0, 4.0);
    save(&mut env.store, &s);
    true
}

fn num(v: f64) -> String {
    if v == v.round() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

struct Round {
    countdown: bool,
    nums: Vec<f64>,
    target: f64,
    solution: String,
    best_value: f64,
    deadline: f64,
    /// how it ended, the value locked in, the points
    over: Option<(&'static str, Option<f64>, u32)>,
}

struct Numbers {
    stats: NumbersStats,
    round: Round,
    text: String,
    /// in chars
    caret: usize,
    note: String,
    now: f64,
}

impl Numbers {
    fn new(countdown: bool, env: &mut Env) -> Numbers {
        let mut s = load(&env.store);
        s.mode = if countdown { Mode::Countdown } else { Mode::TwentyFour };
        // the mode the lobby picked is kept as the next default
        save(&mut env.store, &s);
        let round = Self::deal(countdown, s.large, env);
        Numbers {
            stats: s,
            round,
            text: String::new(),
            caret: 0,
            note: String::new(),
            now: env.date(),
        }
    }

    fn deal(countdown: bool, large: f64, env: &mut Env) -> Round {
        let secs = if countdown { SECONDS_COUNTDOWN } else { SECONDS_24 };
        let deadline = env.date() + secs * 1000.0;
        if countdown {
            let (nums, target, best) = deal_countdown(large, &mut || env.rand());
            Round {
                countdown,
                nums,
                target,
                solution: best.expr,
                best_value: best.value,
                deadline,
                over: None,
            }
        } else {
            let (nums, solution) = deal24(&mut || env.rand());
            Round {
                countdown,
                nums,
                target: numbers::TARGET_24,
                solution,
                best_value: numbers::TARGET_24,
                deadline,
                over: None,
            }
        }
    }

    fn mode(&self) -> Mode {
        if self.round.countdown { Mode::Countdown } else { Mode::TwentyFour }
    }

    fn filled(&self) -> String {
        fill_parens(&self.text)
    }

    fn check(&self) -> numbers::Check {
        check_expr(&self.filled(), &self.round.nums, self.mode())
    }

    fn update(&mut self, env: &mut Env, f: impl FnOnce(&NumbersStats) -> NumbersStats) {
        self.stats = f(&self.stats);
        save(&mut env.store, &self.stats);
    }

    fn start(&mut self, env: &mut Env) {
        let countdown = self.round.countdown;
        self.update(env, |s| NumbersStats {
            mode: if countdown { Mode::Countdown } else { Mode::TwentyFour },
            ..*s
        });
        self.round = Self::deal(countdown, self.stats.large, env);
        self.text.clear();
        self.caret = 0;
        self.note.clear();
        self.now = env.date();
    }

    /// Ends the round once; points and streaks are kept as it ends.
    fn finish(&mut self, how: &'static str, value: Option<f64>, env: &mut Env) {
        if self.round.over.is_some() {
            return;
        }
        let points = if self.round.countdown && how != "gave up" {
            countdown_points(self.round.target, value)
        } else {
            0
        };
        if self.round.countdown {
            self.update(env, |s| record_countdown(s, points));
        } else {
            self.update(env, |s| record24(s, how == "solved"));
        }
        self.round.over = Some((how, value, points));
        self.note.clear();
    }

    fn submit(&mut self, env: &mut Env) {
        if self.round.over.is_some() {
            return;
        }
        let c = self.check();
        let blank = self.text.trim().is_empty();
        if !self.round.countdown {
            if solves24(&self.filled(), &self.round.nums) {
                self.finish("solved", Some(24.0), env);
            } else if let Some(p) = c.problem {
                self.note = p;
            } else if let Some(v) = c.value {
                self.note = format!("that’s {}, not 24", frac_text(v));
            } else {
                self.note = if blank { "type an expression".into() } else { "not finished".into() };
            }
            return;
        }
        match c.value {
            Some(v) if c.valid => self.finish("locked", Some(v.n / v.d), env),
            _ => {
                self.note = c
                    .problem
                    .unwrap_or_else(|| if blank { "type an expression".into() } else { "not finished".into() });
            }
        }
    }

    fn insert(&mut self, s: &str) {
        let mut chars: Vec<char> = self.text.chars().collect();
        let at = self.caret.min(chars.len());
        for (i, ch) in s.chars().enumerate() {
            chars.insert(at + i, ch);
        }
        self.caret = at + s.chars().count();
        self.text = chars.into_iter().collect();
        self.note.clear();
    }

    fn live(&self) -> (String, bool) {
        if !self.note.is_empty() {
            return (self.note.clone(), true);
        }
        let c = self.check();
        if let Some(v) = c.value {
            // numbers still to use isn't wrong yet, just not done
            return match c.problem {
                Some(p) => (format!("= {} · {p}", frac_text(v)), !p.starts_with("use ")),
                None => (format!("= {}", frac_text(v)), false),
            };
        }
        match c.problem {
            Some(p) => (p, true),
            None => (String::new(), false),
        }
    }

    fn result(&self) -> (String, bool, Option<String>) {
        let r = &self.round;
        let Some((how, value, points)) = r.over else {
            return (String::new(), false, None);
        };
        let text = if !r.countdown {
            match how {
                "solved" => format!("24! streak {}", self.stats.streak24),
                "time" => "time’s up".into(),
                _ => "gave up".into(),
            }
        } else {
            let what = if how == "gave up" {
                "gave up".to_string()
            } else if let Some(v) = value {
                let off = (r.target - v).abs();
                if off == 0.0 {
                    format!("exactly {}!", num(r.target))
                } else {
                    format!("{}{}, {} away", if how == "time" { "time’s up · " } else { "" }, num(v), num(off))
                }
            } else if how == "time" {
                "time’s up · nothing locked in".to_string()
            } else {
                "nothing".to_string()
            };
            format!("{what} · {points} pts")
        };
        let won = how == "solved" || points == 10;
        let solution = if r.countdown && r.best_value != r.target {
            format!("{} = {} (closest possible)", r.solution, num(r.best_value))
        } else {
            format!("{} = {}", r.solution, num(r.target))
        };
        (text, won, (!won).then_some(solution))
    }
}

impl Play for Numbers {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        self.now = env.date();
        let c = ui.c;
        let cx = x + width / 2.0;
        let r = &self.round;
        let mut yy = y;
        // the head
        let s = &self.stats;
        let sub = if r.countdown {
            format!("{} pts in {} · exact run {} · best {}", s.points, s.rounds, s.exact_streak, s.best_exact)
        } else {
            format!("streak {} · best {}", s.streak24, s.best24)
        };
        let name = if r.countdown { "countdown" } else { "24" };
        let (nw, nh) = ui.measure(name, Font::bold(m::FS_BODY));
        let hh = nh.max(22.0);
        ui.text(name, Font::bold(m::FS_BODY), x, yy + (hh - nh) / 2.0, c.ink);
        let (_, sh) = ui.measure(&sub, Font::SMALL);
        ui.text(&sub, Font::digits(m::FS_SMALL, false), x + nw + m::SP3, yy + (hh - sh) / 2.0 + 1.0, c.muted);
        let ew = w::esc_button(ui, x + width + m::SP2, yy + (hh - sh - 6.0) / 2.0, "esc leaves", Hit::Esc);
        w::esc_button(ui, x + width + m::SP2 - ew - (m::SP3 - m::SP2), yy + (hh - sh - 6.0) / 2.0, "modes", Hit::Act(ACT_MENU));
        yy += hh + m::SP3;
        // the target
        let label = if r.countdown { "target" } else { "make" };
        let tf = Font::digits(30.0, true);
        let target = num(r.target);
        let (lw, lh) = ui.measure(label, Font::SMALL);
        let (tw, th) = ui.measure(&target, tf);
        let lx = cx - (lw + m::SP2 + tw) / 2.0;
        ui.text(label, Font::SMALL, lx, yy + th * 0.78 - lh * 0.78, c.muted);
        ui.text(&target, tf, lx + lw + m::SP2, yy, c.accent);
        yy += th + m::SP2;
        // the tiles
        let taken = used_tiles(&self.text, &r.nums);
        let (tile_w, tile_f) = if r.countdown { (46.0, Font::digits(18.0, true)) } else { (52.0, Font::digits(20.0, true)) };
        let n = r.nums.len() as f32;
        let mut tx = cx - (n * tile_w + (n - 1.0) * m::SP2) / 2.0;
        for (i, v) in r.nums.iter().enumerate() {
            let rect = Rect::new(tx, yy, tile_w, 44.0);
            let was = ui.set_alpha(if taken.get(i).copied().unwrap_or(false) { 0.3 } else { 1.0 });
            ui.control_shadow(rect, m::RADIUS_S);
            ui.round(rect, m::RADIUS_S, c.control);
            let t = num(*v);
            let (vw, vh) = ui.measure(&t, tile_f);
            ui.text(&t, tile_f, rect.x + (tile_w - vw) / 2.0, rect.y + (44.0 - vh) / 2.0, if *v >= 25.0 && r.countdown { c.accent } else { c.ink });
            ui.set_alpha(was);
            if r.over.is_none() {
                ui.hit(rect, Hit::Act(i as u32));
            }
            tx += tile_w + m::SP2;
        }
        yy += 44.0 + m::SP2;
        // the clock
        let total = if r.countdown { SECONDS_COUNTDOWN } else { SECONDS_24 } * 1000.0;
        let left = (r.deadline - self.now).max(0.0);
        let bar = Rect::new(cx - 160.0, yy, 320.0, 3.0);
        ui.round(bar, 2.0, c.fill_track());
        if r.over.is_none() {
            let fw = (bar.w as f64 * left / total) as f32;
            ui.round(Rect::new(bar.x, bar.y, fw, 3.0), 1.5, if left < 10_000.0 { BAD } else { c.accent });
        }
        yy += 3.0 + m::SP2;
        // the entry
        let field = Rect::new(cx - 160.0, yy, 320.0, 34.0);
        ui.round(field, m::RADIUS_S, c.bg_tape);
        ui.ring(field, m::RADIUS_S, 1.0, if r.over.is_none() { c.ring() } else { c.line });
        let ef = Font::digits(16.0, false);
        let (_, eh) = ui.measure("0", ef);
        let ty = field.y + (34.0 - eh) / 2.0;
        let text_x = field.x + m::SP3;
        if self.text.is_empty() && r.over.is_none() {
            let hint = if r.countdown { "e.g. 100 × 7 − 25" } else { "e.g. (8 − 2) × 4" };
            ui.text(hint, ef, text_x, ty, c.muted.opacity(0.6));
        }
        let tw = ui.text(&self.text, ef, text_x, ty, c.ink);
        if r.over.is_none() {
            let ghost = ")".repeat(infer_parens(&self.text).trailing);
            if !ghost.is_empty() {
                ui.text(&ghost, ef, text_x + tw, ty, c.muted.opacity(0.55));
            }
            let before: String = self.text.chars().take(self.caret).collect();
            let (bw, _) = ui.measure(&before, ef);
            ui.fill(Rect::new(text_x + bw, ty + 1.0, 1.0, eh - 2.0), c.accent);
            let secs = format!("{}s", (left / 1000.0).ceil());
            ui.text_in(&secs, Font::digits(m::FS_SMALL, false), field.x, field.y + (34.0 - 14.0) / 2.0, field.w - m::SP3, Align::Right, c.muted);
        }
        yy += 34.0 + m::SP2;
        // the live line
        let (live, bad) = if r.over.is_some() { (String::new(), false) } else { self.live() };
        let lf = Font::digits(m::FS_BODY, false);
        let (_, llh) = ui.measure("0", lf);
        if !live.is_empty() {
            ui.text_in(&live, lf, x, yy, width, Align::Center, if bad { BAD } else { c.muted });
        }
        yy += llh;
        if r.over.is_some() {
            let (text, won, solution) = self.result();
            yy += m::SP2;
            let (_, rh) = ui.measure(&text, Font::digits(m::FS_BODY, true));
            ui.text_in(&text, Font::digits(m::FS_BODY, true), x, yy, width, Align::Center, if won { c.accent } else { c.ink });
            yy += rh + 2.0;
            if let Some(sol) = solution {
                yy += ui.text_wrap(&sol, lf, x, yy, width, Align::Center, c.muted);
            }
        }
        yy += m::SP2;
        // the note
        let over = self.round.over.is_some();
        let lead = if over {
            "↵ next · "
        } else if self.round.countdown {
            "↵ lock in · "
        } else {
            "↵ check · "
        };
        let mut v = vec![Run::plain(lead)];
        if !over {
            v.push(Run::link("? show a solution", Hit::Act(ACT_GIVE_UP)));
            v.push(Run::plain(" · "));
        }
        v.push(Run::plain("esc leaves"));
        let muted = c.muted;
        yy += w::runs(ui, cx, yy, Font::digits(m::FS_SMALL, false), muted, &v);
        yy - y
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if !k.down || k.key == "Escape" || !k.plain() {
            return false;
        }
        if k.key == "Enter" {
            if !k.repeat {
                if self.round.over.is_some() {
                    self.start(env);
                } else {
                    self.submit(env);
                }
            }
            return true;
        }
        if k.key == "?" {
            if self.round.over.is_none() {
                self.finish("gave up", None, env);
            }
            return true;
        }
        if self.round.over.is_some() {
            return k.is_char() || k.key == "Backspace" || k.key.starts_with("Arrow");
        }
        let len = self.text.chars().count();
        if k.key == "Tab" || k.key == "ArrowRight" {
            let caret16 = self.text.chars().take(self.caret).map(char::len_utf16).sum::<usize>();
            if let Some(written) = autofill_parens(&self.text, caret16, caret16) {
                self.text = written;
                self.caret = self.text.chars().count();
                return true;
            }
            // tab never leaves the field
            if k.key == "Tab" {
                return true;
            }
        }
        match k.key.as_str() {
            "ArrowLeft" => self.caret = self.caret.saturating_sub(1),
            "ArrowRight" => self.caret = (self.caret + 1).min(len),
            "Home" => self.caret = 0,
            "End" => self.caret = len,
            "Backspace" => {
                if self.caret > 0 {
                    let mut chars: Vec<char> = self.text.chars().collect();
                    chars.remove(self.caret - 1);
                    self.caret -= 1;
                    self.text = chars.into_iter().collect();
                    self.note.clear();
                }
            }
            "Delete" => {
                if self.caret < len {
                    let mut chars: Vec<char> = self.text.chars().collect();
                    chars.remove(self.caret);
                    self.text = chars.into_iter().collect();
                    self.note.clear();
                }
            }
            _ if k.is_char() => self.insert(&k.key),
            _ => return k.key.starts_with("Arrow"),
        }
        true
    }

    fn act(&mut self, id: u32, env: &mut Env) {
        match id {
            ACT_MENU => {
                self.closing(env);
                env.end = Some(None);
            }
            ACT_GIVE_UP => self.finish("gave up", None, env),
            i => {
                if let Some(v) = self.round.nums.get(i as usize).copied() {
                    // a tile goes in; after a number it comes with a +
                    let after_digit = self.text.chars().take(self.caret).last().is_some_and(|c| c.is_ascii_digit());
                    let piece = if after_digit { format!(" + {}", num(v)) } else { num(v) };
                    self.insert(&piece);
                }
            }
        }
    }

    fn ticks(&self) -> bool {
        self.round.over.is_none()
    }

    /// The clock; when it runs out, countdown takes what's typed if it's a legal answer.
    fn tick(&mut self, env: &mut Env) {
        if self.round.over.is_some() {
            return;
        }
        self.now = env.date();
        if self.now < self.round.deadline {
            return;
        }
        let c = self.check();
        let value = if self.round.countdown && c.valid {
            c.value.map(|v| v.n / v.d)
        } else {
            None
        };
        self.finish("time", value, env);
    }

    /// Leaving mid-round (modes, esc, closing) counts as giving up.
    fn closing(&mut self, env: &mut Env) {
        if self.round.over.is_none() {
            let s = load(&env.store);
            let s = if self.round.countdown { record_countdown(&s, 0) } else { record24(&s, false) };
            save(&mut env.store, &s);
            self.round.over = Some(("gave up", None, 0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_the_page_does() {
        assert_eq!(num(24.0), "24");
        assert_eq!(num(2.5), "2.5");
    }
}
