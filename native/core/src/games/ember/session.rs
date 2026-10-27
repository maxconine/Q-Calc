//! Drives ember & frost (`src/lib/emberSession.ts`): which screen, which level, the fixed-step clock, and (online)
//! the link. The panel feeds it keys and frame times and draws what tick() returns.
//!
//! Online, the host (ember) is authoritative: it steps the engine with its own keys and the guest's, and streams
//! state at STATE_HZ. The guest (frost) runs the same engine ahead of the host's clock, so frost answers its keys
//! at once: on each host state it rewinds to it and replays its own recorded keys up to its current frame. The
//! guest never changes phase by itself: a death, a respawn, a cleared stage and a win all wait for the host.

use super::engine::{init_state, respawn, stage_of, step};
use super::protocol::{
    EmberMsg, EmberNote, KEY_EVERY, KeyChange, SILENCE_MS, SLOW_REPEAT, STATE_HZ, Slow, StateMsg,
    WIRE_VERSION, decode_ember_n, encode_ember, event_fits, fast_of, join_state, slow_json,
    slow_of,
};
use super::types::*;
use std::sync::Arc;

use super::{Catalog, shipped};
use crate::games::hello::{LinkedGame, other_game_note};

/// a frame after a long gap only catches up this much, so it never spirals
const MAX_GAP_S: f64 = 0.25;
/// frame times land a hair either side of a step
const STEP_EPS: f64 = 1e-6;
const HI_MS: f64 = 400.0;
const KEEP_MS: f64 = 250.0;
const SEL_MS: f64 = 300.0;
const STATE_MS: f64 = 1000.0 / STATE_HZ;
const NOTE_MS: f64 = 3000.0;
const PICK_MS: f64 = 1000.0;
const MAX_AHEAD: i64 = 40;
const START_LEAD: i64 = 6;
const MIN_SLACK: i64 = 1;
const AIM_SLACK: i64 = 3;
const LOOSE_SLACK: i64 = 6;
const SLACK_WINDOW: usize = 8;
const SNAP_PX: f64 = 40.0;
const SMOOTH: f64 = 0.8;
const ASK_MS: f64 = 100.0;
const NAMES: [&str; 2] = ["ember", "frost"];

/// the guest plays these for its own character as it predicts them, and skips the host's copies
fn own_kind(k: EventKind) -> bool {
    use EventKind as K;
    matches!(
        k,
        K::Jump | K::Land | K::Gem | K::Portal | K::Push | K::Airjump | K::Bounce
    )
}

fn note_text(w: EmberNote) -> &'static str {
    match w {
        EmberNote::Play => "started the level",
        EmberNote::Retry => "went back to the checkpoint",
        EmberNote::Restart => "restarted the level",
        EmberNote::Next => "went on to the next level",
        EmberNote::Menu => "went back to the levels",
    }
}

fn clamp_level(i: f64, len: usize) -> usize {
    let n = len.saturating_sub(1) as f64;
    let i = if i.is_finite() { i.floor() } else { 0.0 };
    n.min(i.max(0.0)) as usize
}

/// two states the drawing mustn't blend between: another stage, or a respawn
fn jumped(a: &GameState, b: &GameState) -> bool {
    a.stage != b.stage || b.events.iter().any(|e| e.k == EventKind::Respawn)
}

/// whole fixed steps out of real time; what's left over is how far into the next step this frame sits
#[derive(Clone, Debug)]
struct Clock {
    acc: f64,
    last: f64,
}

impl Clock {
    fn new(now: f64) -> Self {
        Self {
            acc: 0.0,
            last: now,
        }
    }
    fn steps(&mut self, now: f64) -> u32 {
        self.acc += MAX_GAP_S.min((now - self.last).max(0.0) / 1000.0);
        self.last = now;
        let mut n = 0;
        while self.acc >= STEP_S - STEP_EPS {
            self.acc -= STEP_S;
            n += 1;
        }
        self.acc = self.acc.max(0.0);
        n
    }
    fn alpha(&self) -> f64 {
        (self.acc / STEP_S).min(1.0)
    }
    fn reset(&mut self) {
        self.acc = 0.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Wait,
    Select,
    Play,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Local,
    Online,
}

/// what the panel draws this frame
#[derive(Clone, Debug)]
pub struct EmberView {
    pub mode: Mode,
    pub screen: Screen,
    /// index into LEVELS: the one being played, or the one highlighted on select
    pub level: usize,
    /// whether compiled_at(level) is the level on screen (while playing)
    pub playing: bool,
    pub state: Option<GameState>,
    /// the state one step before `state`, for interpolation; the same as state across a jump
    pub prev: Option<GameState>,
    pub alpha: f64,
    /// everything that happened since the last tick() call
    pub events: Vec<GameEvent>,
    /// online: the character this side plays; local: None (both)
    pub me: Option<PlayerIndex>,
    pub ended: Option<String>,
    pub note: Option<String>,
}

/// the session's face to the panel (EmberGame in the TS)
pub trait EmberGame: Send {
    fn tick(&mut self, now: f64) -> EmberView;
    /// held keys. local: per player; online: this side's character
    fn keys(&mut self, player: PlayerIndex, bits: i64);
    fn release(&mut self);
    fn pick(&mut self, index: f64);
    fn play(&mut self);
    fn retry(&mut self);
    fn restart(&mut self);
    fn next(&mut self);
    fn menu(&mut self);
    fn receive(&mut self, text: &str, now: f64);
    fn leave(&mut self);
    /// messages to send, oldest first (online only)
    fn take_outbox(&mut self) -> Vec<String> {
        vec![]
    }
}

// ---- local: both characters on one keyboard

#[derive(Debug)]
pub struct LocalGame {
    cat: Arc<Catalog>,
    screen: Screen,
    level: usize,
    /// cat.get(level) is being played
    compiled: bool,
    state: Option<GameState>,
    prev: Option<GameState>,
    held: [i64; 2],
    tap: [i64; 2],
    clock: Clock,
    out: Vec<GameEvent>,
}

impl LocalGame {
    pub fn new(cat: Arc<Catalog>, now: f64, start: f64) -> Self {
        let level = clamp_level(start, cat.len());
        Self {
            cat,
            screen: Screen::Select,
            level,
            compiled: false,
            state: None,
            prev: None,
            held: [0, 0],
            tap: [0, 0],
            clock: Clock::new(now),
            out: vec![],
        }
    }

    fn start(&mut self, i: usize, deaths: i64) {
        let cat = self.cat.clone();
        let Some(level) = cat.get(i) else { return };
        self.level = i;
        self.compiled = true;
        let s = init_state(level, deaths);
        self.prev = Some(s.clone());
        self.state = Some(s);
        self.out = vec![];
        self.screen = Screen::Play;
        self.tap = [0, 0];
        self.clock.reset();
    }
}

impl EmberGame for LocalGame {
    fn tick(&mut self, now: f64) -> EmberView {
        let n = self.clock.steps(now);
        let mut events = std::mem::take(&mut self.out);
        let cat = self.cat.clone();
        if self.screen == Screen::Play
            && self.compiled
            && let (Some(level), Some(_)) = (cat.get(self.level), &self.state)
        {
            for _ in 0..n {
                let before = self.state.take().unwrap();
                let next = step(
                    level,
                    &before,
                    [self.held[0] | self.tap[0], self.held[1] | self.tap[1]],
                );
                self.tap = [0, 0];
                self.prev = Some(if jumped(&before, &next) {
                    next.clone()
                } else {
                    before
                });
                events.extend(next.events.iter().cloned());
                self.state = Some(next);
            }
        }
        let playing = self.screen == Screen::Play;
        EmberView {
            mode: Mode::Local,
            screen: self.screen,
            level: self.level,
            playing: playing && self.compiled,
            state: if playing { self.state.clone() } else { None },
            prev: if playing { self.prev.clone() } else { None },
            alpha: self.clock.alpha(),
            events,
            me: None,
            ended: None,
            note: None,
        }
    }

    fn keys(&mut self, player: PlayerIndex, bits: i64) {
        self.tap[player] |= bits & 15 & !self.held[player];
        self.held[player] = bits & 15;
    }

    fn release(&mut self) {
        self.held = [0, 0];
        self.tap = [0, 0];
    }

    fn pick(&mut self, index: f64) {
        if self.screen == Screen::Select {
            self.level = clamp_level(index, self.cat.len());
        }
    }

    fn play(&mut self) {
        if self.screen == Screen::Select {
            self.start(self.level, 0);
        }
    }

    /// back to the last checkpoint; after a win there's nothing to go back to, so the level begins again
    fn retry(&mut self) {
        if self.screen != Screen::Play || !self.compiled {
            return;
        }
        let cat = self.cat.clone();
        let (Some(s), Some(level)) = (&self.state, cat.get(self.level)) else {
            return;
        };
        match s.phase {
            Phase::Won => self.restart(),
            Phase::Clear => {}
            _ => {
                let n = respawn(level, s);
                self.out.extend(n.events.iter().cloned());
                self.prev = Some(n.clone());
                self.state = Some(n);
            }
        }
    }

    fn restart(&mut self) {
        if self.screen == Screen::Play && self.state.is_some() {
            self.start(self.level, 0);
        }
    }

    fn next(&mut self) {
        if self.screen != Screen::Play || self.state.as_ref().map(|s| s.phase) != Some(Phase::Won) {
            return;
        }
        if self.level + 1 < self.cat.len() {
            self.start(self.level + 1, 0);
        } else {
            self.menu();
        }
    }

    fn menu(&mut self) {
        self.screen = Screen::Select;
        self.state = None;
        self.prev = None;
        self.compiled = false;
        self.out = vec![];
    }

    fn receive(&mut self, _text: &str, _now: f64) {}

    fn leave(&mut self) {
        self.menu();
    }
}

// ---- online: the parts both sides share (handshake, silence, notes)

#[derive(Debug)]
struct Online {
    cat: Arc<Catalog>,
    screen: Screen,
    level: usize,
    /// cat.get(level) is being played
    compiled: bool,
    held: i64,
    tap: i64,
    greeted: bool,
    now: f64,
    clock: Clock,
    out: Vec<GameEvent>,
    heard: f64,
    last_hi: f64,
    ended: Option<String>,
    note: Option<String>,
    note_at: f64,
    outbox: Vec<String>,
}

impl Online {
    fn new(cat: Arc<Catalog>, now: f64, start: f64) -> Self {
        let level = clamp_level(start, cat.len());
        let mut o = Self {
            cat,
            screen: Screen::Wait,
            level,
            compiled: false,
            held: 0,
            tap: 0,
            greeted: false,
            now,
            clock: Clock::new(now),
            out: vec![],
            heard: now,
            last_hi: now,
            ended: None,
            note: None,
            note_at: 0.0,
            outbox: vec![],
        };
        o.hi();
        o
    }

    fn post(&mut self, m: &EmberMsg) {
        self.outbox.push(encode_ember(m));
    }

    fn hi(&mut self) {
        let ok = self.greeted;
        self.post(&EmberMsg::Hi {
            v: EMBER_VERSION,
            w: WIRE_VERSION,
            g: true,
            ok,
        });
    }

    fn end(&mut self, line: &str) {
        if self.ended.is_none() {
            self.ended = Some(line.into());
        }
    }

    fn say(&mut self, who: PlayerIndex, what: &str) {
        self.note = Some(format!("{} {what}", NAMES[who]));
        self.note_at = self.now;
    }

    fn say_note(&mut self, who: PlayerIndex, w: EmberNote) {
        self.say(who, note_text(w));
    }

    /// the shared part of receive: true when the message is for the side's own handler
    fn receive(&mut self, text: &str, now: f64) -> Option<EmberMsg> {
        if self.ended.is_some() {
            return None;
        }
        let m = decode_ember_n(text, self.cat.len())?;
        self.heard = now;
        self.now = self.now.max(now);
        match m {
            EmberMsg::Hi { v, w, ok, .. } => {
                if v != EMBER_VERSION || w != WIRE_VERSION {
                    self.post(&EmberMsg::Bye);
                    self.end("update both Q Calcs to play each other");
                    return None;
                }
                let first = !self.greeted;
                self.greeted = true;
                if first && self.screen == Screen::Wait {
                    self.screen = Screen::Select;
                }
                if !ok {
                    self.hi();
                }
                None
            }
            EmberMsg::Other { game } => {
                self.post(&EmberMsg::Bye);
                self.end(&other_game_note(&game, LinkedGame::Ember));
                None
            }
            EmberMsg::Bye => {
                self.end("your friend left");
                None
            }
            // nothing but hi counts until the versions have matched
            m if self.greeted => Some(m),
            _ => None,
        }
    }

    /// the shared start of tick: the steps due, and whether to advance
    fn begin_tick(&mut self, now: f64) -> (u32, bool) {
        self.now = now;
        let n = self.clock.steps(now);
        let mut go = false;
        if self.ended.is_none() {
            if now - self.heard > SILENCE_MS {
                self.end("lost the connection");
            } else {
                if !self.greeted && now - self.last_hi >= HI_MS {
                    self.last_hi = now;
                    self.hi();
                }
                go = true;
            }
        }
        (n, go)
    }

    fn view(
        &mut self,
        me: PlayerIndex,
        drawn: Option<(Option<GameState>, Option<GameState>)>,
    ) -> EmberView {
        if self.note.is_some() && self.now - self.note_at > NOTE_MS {
            self.note = None;
        }
        let events = std::mem::take(&mut self.out);
        let playing = self.screen == Screen::Play;
        let (state, prev) = if playing {
            drawn.unwrap_or((None, None))
        } else {
            (None, None)
        };
        EmberView {
            mode: Mode::Online,
            screen: self.screen,
            level: self.level,
            playing: playing && self.compiled,
            state,
            prev,
            alpha: self.clock.alpha(),
            events,
            me: Some(me),
            ended: self.ended.clone(),
            note: self.note.clone(),
        }
    }

    fn keys(&mut self, bits: i64) {
        self.tap |= bits & 15 & !self.held;
        self.held = bits & 15;
    }

    fn release(&mut self) {
        self.held = 0;
        self.tap = 0;
    }

    fn leave(&mut self) {
        if self.ended.is_some() {
            return;
        }
        self.post(&EmberMsg::Bye);
        self.end("you left");
    }
}

// ---- the host: ember, and the one true game

#[derive(Debug)]
pub struct HostGame {
    o: Online,
    state: Option<GameState>,
    prev: Option<GameState>,
    frame: i64,
    attempt: i64,
    guest_bits: i64,
    queue: Vec<KeyChange>,
    queued_to: i64,
    slack: Option<(i64, i64)>,
    pending: Vec<GameEvent>,
    next_state: f64,
    next_sel: f64,
    slow_text: String,
    rev: i64,
    slow_left: u32,
    since_key: u32,
    last: Option<(i64, Vec<GameEvent>)>,
}

impl HostGame {
    pub fn new(cat: Arc<Catalog>, now: f64, start: f64) -> Self {
        Self {
            o: Online::new(cat, now, start),
            state: None,
            prev: None,
            frame: 0,
            attempt: 0,
            guest_bits: 0,
            queue: vec![],
            queued_to: -1,
            slack: None,
            pending: vec![],
            next_state: f64::NEG_INFINITY,
            next_sel: f64::NEG_INFINITY,
            slow_text: String::new(),
            rev: 0,
            slow_left: 0,
            since_key: 0,
            last: None,
        }
    }

    fn start(&mut self, i: usize, deaths: i64) -> bool {
        let cat = self.o.cat.clone();
        let Some(level) = cat.get(i) else {
            return false;
        };
        self.o.level = i;
        self.o.compiled = true;
        let s = init_state(level, deaths);
        self.prev = Some(s.clone());
        self.state = Some(s);
        self.o.screen = Screen::Play;
        self.frame = 0;
        self.attempt += 1;
        self.guest_bits = 0;
        self.o.tap = 0;
        self.queue = vec![];
        self.queued_to = -1;
        self.slack = None;
        self.pending = vec![];
        self.last = None;
        // a new attempt always begins with its slow part
        self.slow_text = String::new();
        self.next_state = f64::NEG_INFINITY;
        self.o.clock.reset();
        true
    }

    fn again(&mut self) -> bool {
        if self.o.screen != Screen::Play || self.state.is_none() {
            return false;
        }
        self.start(self.o.level, 0)
    }

    fn back(&mut self) -> bool {
        let cat = self.o.cat.clone();
        let level = if self.o.compiled {
            cat.get(self.o.level)
        } else {
            None
        };
        let (Some(s), Some(level)) = (&self.state, level) else {
            return false;
        };
        if self.o.screen != Screen::Play || s.phase == Phase::Won || s.phase == Phase::Clear {
            return false;
        }
        let n = respawn(level, s);
        self.o.out.extend(n.events.iter().cloned());
        self.pending.extend(n.events.iter().cloned());
        self.prev = Some(n.clone());
        self.state = Some(n);
        self.next_state = f64::NEG_INFINITY;
        true
    }

    fn forward(&mut self) -> bool {
        if self.o.screen != Screen::Play || self.state.as_ref().map(|s| s.phase) != Some(Phase::Won)
        {
            return false;
        }
        if self.o.level + 1 < self.o.cat.len() {
            return self.start(self.o.level + 1, 0);
        }
        self.go_to_select()
    }

    fn go_to_select(&mut self) -> bool {
        if self.o.screen != Screen::Play {
            return false;
        }
        self.o.screen = Screen::Select;
        self.state = None;
        self.prev = None;
        self.o.compiled = false;
        self.next_sel = f64::NEG_INFINITY;
        true
    }

    fn won(&self) -> bool {
        self.state.as_ref().map(|s| s.phase) == Some(Phase::Won)
    }

    fn handle(&mut self, m: EmberMsg) {
        match m {
            EmberMsg::K { a, f, i, r } => {
                if self.o.screen != Screen::Play {
                    return;
                }
                // the guest holds another slow part than the current one, or none: send it again
                if r != Some(self.rev as f64) {
                    self.slow_left = self.slow_left.max(1);
                }
                if a as i64 != self.attempt {
                    return;
                }
                for c in i {
                    if c.0 <= self.queued_to {
                        continue;
                    }
                    self.queue.push(c);
                    self.queued_to = c.0;
                }
                // how many frames before the host needs them the keys turned up
                let f = f as i64;
                let slack = f - self.frame;
                if self.slack.is_none_or(|s| slack < s.0) {
                    self.slack = Some((slack, f));
                }
            }
            EmberMsg::Pick { l } => {
                if self.o.screen == Screen::Select {
                    self.pick(l);
                }
            }
            EmberMsg::Go { l } => {
                if self.o.screen != Screen::Select {
                    return;
                }
                if self.start(clamp_level(l, self.o.cat.len()), 0) {
                    self.o.say_note(1, EmberNote::Play);
                }
            }
            EmberMsg::Retry => {
                let won = self.won();
                if if won { self.again() } else { self.back() } {
                    let w = if self.won() {
                        EmberNote::Restart
                    } else {
                        EmberNote::Retry
                    };
                    // the TS reads the phase after the restart, which is never 'won': a restart from a win says 'retry'
                    self.o.say_note(1, w);
                }
            }
            EmberMsg::Restart => {
                if self.again() {
                    self.o.say_note(1, EmberNote::Restart);
                }
            }
            EmberMsg::Next => {
                if self.forward() {
                    self.o.say_note(1, EmberNote::Next);
                }
            }
            EmberMsg::Menu if self.go_to_select() => self.o.say_note(1, EmberNote::Menu),
            _ => {}
        }
    }

    fn advance(&mut self, steps: u32, now: f64) {
        let cat = self.o.cat.clone();
        let level = if self.o.compiled {
            cat.get(self.o.level)
        } else {
            None
        };
        if self.o.screen == Screen::Play
            && let (Some(level), Some(_)) = (level, &self.state)
        {
            for _ in 0..steps {
                // one change a step: keys that turn up late play out in order
                if let Some(&(f, bits)) = self.queue.first()
                    && f <= self.frame
                {
                    self.guest_bits = bits;
                    self.queue.remove(0);
                }
                let before = self.state.take().unwrap();
                let next = step(level, &before, [self.o.held | self.o.tap, self.guest_bits]);
                self.o.tap = 0;
                self.prev = Some(if jumped(&before, &next) {
                    next.clone()
                } else {
                    before
                });
                self.frame += 1;
                self.o.out.extend(next.events.iter().cloned());
                // the guest makes its own win from the phase
                self.pending.extend(
                    next.events
                        .iter()
                        .filter(|e| e.k != EventKind::Win)
                        .cloned(),
                );
                self.state = Some(next);
            }
            if now >= self.next_state {
                // a frame that came very late doesn't make the next few go early
                self.next_state = self.next_state.max(now - STATE_MS / 2.0) + STATE_MS;
                let s = self.state.as_ref().unwrap();
                let stage = stage_of(level, s.stage);
                let k = slow_of(self.o.level, stage, s);
                let text = slow_json(&k);
                if text != self.slow_text {
                    self.slow_text = text;
                    self.rev += 1;
                    self.slow_left = SLOW_REPEAT;
                }
                let key = self.slow_left > 0 || self.since_key >= KEY_EVERY - 1;
                if key {
                    self.slow_left = self.slow_left.saturating_sub(1);
                    self.since_key = 0;
                } else {
                    self.since_key += 1;
                }
                let g = fast_of(stage, s);
                let pending = std::mem::take(&mut self.pending);
                let msg = StateMsg {
                    a: self.attempt as f64,
                    f: self.frame as f64,
                    fi: self.o.held as f64,
                    sl: self.slack.map(|(a, b)| (a as f64, b as f64)),
                    r: self.rev as f64,
                    k: if key { Some(k) } else { None },
                    g,
                    e: pending.clone(),
                    q: self
                        .last
                        .as_ref()
                        .filter(|l| !l.1.is_empty())
                        .map(|(f, e)| (*f as f64, e.clone())),
                };
                self.o.post(&EmberMsg::State(Box::new(msg)));
                self.last = Some((self.frame, pending));
                self.slack = None;
            }
        } else if self.o.screen == Screen::Select && now >= self.next_sel {
            self.next_sel = now + SEL_MS;
            let l = self.o.level as f64;
            self.o.post(&EmberMsg::Sel { l });
        }
    }
}

impl EmberGame for HostGame {
    fn tick(&mut self, now: f64) -> EmberView {
        let (n, go) = self.o.begin_tick(now);
        if go {
            self.advance(n, now);
        }
        let drawn = Some((self.state.clone(), self.prev.clone()));
        self.o.view(0, drawn)
    }
    fn keys(&mut self, _player: PlayerIndex, bits: i64) {
        self.o.keys(bits);
    }
    fn release(&mut self) {
        self.o.release();
    }
    fn pick(&mut self, index: f64) {
        if self.o.screen != Screen::Select {
            return;
        }
        self.o.level = clamp_level(index, self.o.cat.len());
        self.next_sel = f64::NEG_INFINITY;
    }
    fn play(&mut self) {
        if self.o.screen != Screen::Select || !self.start(self.o.level, 0) {
            return;
        }
        self.o.post(&EmberMsg::Note { w: EmberNote::Play });
    }
    fn retry(&mut self) {
        if self.won() {
            return self.restart();
        }
        if !self.back() {
            return;
        }
        self.o.post(&EmberMsg::Note {
            w: EmberNote::Retry,
        });
    }
    fn restart(&mut self) {
        if !self.again() {
            return;
        }
        self.o.post(&EmberMsg::Note {
            w: EmberNote::Restart,
        });
    }
    fn next(&mut self) {
        if !self.forward() {
            return;
        }
        self.o.post(&EmberMsg::Note { w: EmberNote::Next });
    }
    fn menu(&mut self) {
        if !self.go_to_select() {
            return;
        }
        self.o.post(&EmberMsg::Note { w: EmberNote::Menu });
    }
    fn receive(&mut self, text: &str, now: f64) {
        if let Some(m) = self.o.receive(text, now) {
            self.handle(m);
        }
    }
    fn leave(&mut self) {
        self.o.leave();
    }
    fn take_outbox(&mut self) -> Vec<String> {
        std::mem::take(&mut self.o.outbox)
    }
}

// ---- the guest: frost, predicted

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Offset {
    x: f64,
    y: f64,
}

#[derive(Debug)]
pub struct GuestGame {
    o: Online,
    attempt: i64,
    auth: Option<GameState>,
    auth_f: i64,
    fire_bits: i64,
    dirty: bool,
    cur: Option<GameState>,
    prev: Option<GameState>,
    frame: i64,
    holding: bool,
    log: Vec<KeyChange>,
    recent: Vec<KeyChange>,
    logged: i64,
    unsent: bool,
    last_sent: f64,
    lead: i64,
    adjusted_at: i64,
    slacks: Vec<i64>,
    skip: u32,
    offsets: [Offset; 2],
    win_seen: bool,
    picked: Option<(usize, f64)>,
    slow: Option<Slow>,
    slow_rev: i64,
    parked: Option<StateMsg>,
    asked: f64,
    ev_f: i64,
}

impl GuestGame {
    pub fn new(cat: Arc<Catalog>, now: f64, start: f64) -> Self {
        Self {
            o: Online::new(cat, now, start),
            attempt: -1,
            auth: None,
            auth_f: 0,
            fire_bits: 0,
            dirty: false,
            cur: None,
            prev: None,
            frame: 0,
            holding: false,
            log: vec![],
            recent: vec![],
            logged: -1,
            unsent: false,
            last_sent: f64::NEG_INFINITY,
            lead: START_LEAD,
            adjusted_at: 0,
            slacks: vec![],
            skip: 0,
            offsets: [Offset::default(); 2],
            win_seen: false,
            picked: None,
            slow: None,
            slow_rev: -1,
            parked: None,
            asked: f64::NEG_INFINITY,
            ev_f: -1,
        }
    }

    fn handle(&mut self, m: EmberMsg, now: f64) {
        match m {
            EmberMsg::Sel { l } => {
                if self.o.screen != Screen::Select {
                    self.o.screen = Screen::Select;
                    self.auth = None;
                    self.cur = None;
                    self.prev = None;
                    self.o.compiled = false;
                    self.parked = None;
                }
                // an older select still in flight shouldn't undo this side's own move
                let l = l as usize;
                if let Some((pl, until)) = self.picked
                    && now < until
                    && l != pl
                {
                    return;
                }
                self.picked = None;
                self.o.level = l;
            }
            EmberMsg::State(m) => self.on_state(*m),
            EmberMsg::Note { w } => self.o.say_note(0, w),
            _ => {}
        }
    }

    fn on_state(&mut self, m: StateMsg) {
        let a = m.a as i64;
        let f = m.f as i64;
        if a < self.attempt {
            return;
        }
        let fresh = a > self.attempt;
        if !fresh && (self.o.screen != Screen::Play || f <= self.auth_f) {
            return;
        }
        let slow = match &m.k {
            Some(k) => Some(k.clone()),
            None if !fresh && m.r as i64 == self.slow_rev => self.slow.clone(),
            None => None,
        };
        let Some(slow) = slow else {
            // its slow part was lost or is still on its way: keep the newest such state and ask
            let newer = self
                .parked
                .as_ref()
                .is_none_or(|p| m.a > p.a || (m.a == p.a && m.f > p.f));
            if newer {
                self.parked = Some(m);
            }
            if self.o.now - self.asked >= ASK_MS {
                self.asked = self.o.now;
                self.unsent = true;
            }
            return;
        };
        let cat = self.o.cat.clone();
        let Some(level) = cat.get(slow.l as usize) else {
            return;
        };
        let Some(g) = join_state(level, &slow, &m.g) else {
            return;
        };
        self.slow = Some(slow.clone());
        self.slow_rev = m.r as i64;
        let before = if fresh { None } else { self.auth.clone() };
        if fresh {
            self.attempt = a;
            self.o.level = slow.l as usize;
            self.o.compiled = true;
            self.o.screen = Screen::Play;
            self.frame = f + self.lead;
            self.cur = None;
            self.prev = None;
            self.log = vec![];
            self.recent = vec![];
            self.logged = -1;
            self.o.tap = 0;
            self.adjusted_at = self.frame;
            self.slacks = vec![];
            self.skip = 0;
            self.win_seen = false;
            self.ev_f = -1;
            self.offsets = [Offset::default(); 2];
        }
        let stage = stage_of(level, g.stage);
        let mut heard: Vec<GameEvent> =
            m.q.as_ref()
                .filter(|q| q.0 as i64 > self.ev_f)
                .map(|q| q.1.clone())
                .unwrap_or_default()
                .into_iter()
                .chain(m.e.iter().cloned())
                .filter(|e| e.k != EventKind::Win && event_fits(level, stage, e))
                .collect();
        self.ev_f = f;
        // two lost messages running could take a stage change or a respawn with them: the camera needs both
        if let Some(b) = &before {
            if b.stage != g.stage && !heard.iter().any(|e| e.k == EventKind::Stage) {
                heard.push(GameEvent {
                    stage: Some(g.stage as i64),
                    ..GameEvent::bare(EventKind::Stage)
                });
            } else if b.deaths != g.deaths && !heard.iter().any(|e| e.k == EventKind::Respawn) {
                heard.push(GameEvent {
                    cp: Some(g.cp),
                    ..GameEvent::bare(EventKind::Respawn)
                });
            }
        }
        for e in heard {
            if !(e.p == Some(1) && own_kind(e.k)) {
                self.o.out.push(e);
            }
        }
        if g.phase == Phase::Won && !self.win_seen {
            self.win_seen = true;
            self.o.out.push(GameEvent::bare(EventKind::Win));
        }
        self.auth = Some(g);
        self.auth_f = f;
        self.fire_bits = m.fi as i64;
        self.dirty = true;
        // the host fell back behind this side's clock: jump ahead of it again
        if self.frame < f {
            self.frame = f + self.lead;
            self.adjusted_at = self.frame;
        }
        self.steer(m.sl.map(|(a, b)| (a as i64, b as i64)));
        self.lead = MAX_AHEAD.min((self.frame - f).max(2));
        // a newer state was waiting for this slow part
        if let Some(p) = self.parked.take() {
            if p.a < m.a || (p.a == m.a && p.f <= m.f) {
                // done with
            } else if p.a == m.a && p.r == m.r {
                self.on_state(p);
            } else {
                self.parked = Some(p);
            }
        }
    }

    /// keys turning up late: run further ahead at once. keys turning up early for a while: ease back a frame
    fn steer(&mut self, sl: Option<(i64, i64)>) {
        let Some((slack, at)) = sl else { return };
        if at < self.adjusted_at {
            return;
        }
        if slack < MIN_SLACK {
            self.frame = (self.auth_f + MAX_AHEAD).min(self.frame + AIM_SLACK - slack);
            self.adjusted_at = self.frame;
            self.slacks = vec![];
            return;
        }
        self.slacks.push(slack);
        if self.slacks.len() < SLACK_WINDOW {
            return;
        }
        if self.slacks.iter().copied().min().unwrap_or(0) >= LOOSE_SLACK {
            self.skip += 1;
            self.adjusted_at = self.frame + 1;
            self.slacks = vec![];
        } else {
            self.slacks.remove(0);
        }
    }

    fn bits_at(&self, f: i64) -> i64 {
        self.log.iter().rev().find(|c| c.0 <= f).map_or(0, |c| c.1)
    }

    /// one predicted step; a phase change holds instead
    fn predict(&self, level: &Level, s: &GameState, bits: i64) -> Option<GameState> {
        let n = step(level, s, [self.fire_bits, bits]);
        if n.phase == s.phase { Some(n) } else { None }
    }

    /// rewind to the host's state and replay this side's keys up to the current frame
    fn resim(&mut self, level: &Level, auth: GameState) {
        let before = self.cur.take();
        let mut s = auth.clone();
        let mut prev = auth;
        self.holding = false;
        // the log only needs to reach back to the newest host state
        while self.log.len() > 1 && self.log[1].0 <= self.auth_f {
            self.log.remove(0);
        }
        let mut f = self.auth_f;
        while f < self.frame {
            match self.predict(level, &s, self.bits_at(f)) {
                None => {
                    self.holding = true;
                    prev = s.clone();
                    break;
                }
                Some(n) => {
                    prev = std::mem::replace(&mut s, n);
                }
            }
            f += 1;
        }
        self.prev = Some(if prev.stage == s.stage {
            prev
        } else {
            s.clone()
        });
        // the whole world jumped: another phase, another stage, or back at a checkpoint
        let all = before
            .as_ref()
            .is_none_or(|b| b.phase != s.phase || b.stage != s.stage || b.deaths != s.deaths);
        for p in 0..2 {
            let o = self.offsets[p];
            let a = before.as_ref().map(|b| b.players[p]);
            let b = s.players[p];
            let x = a.map_or(0.0, |a| a.x + o.x - b.x);
            let y = a.map_or(0.0, |a| a.y + o.y - b.y);
            let far = all
                || a.is_none_or(|a| {
                    a.grav != b.grav
                        || a.cannon != b.cannon
                        || (a.fly > 0) != (b.fly > 0)
                        || (a.portal_cd > 0) != (b.portal_cd > 0)
                        || a.alive != b.alive
                        || x.hypot(y) > SNAP_PX
                });
            self.offsets[p] = if far {
                Offset::default()
            } else {
                Offset { x, y }
            };
        }
        self.cur = Some(s);
    }

    fn advance(&mut self, steps: u32, now: f64) {
        let cat = self.o.cat.clone();
        let level = if self.o.compiled {
            cat.get(self.o.level)
        } else {
            None
        };
        if self.o.screen == Screen::Play
            && let (Some(level), Some(_)) = (level, &self.auth)
        {
            if self.dirty {
                self.dirty = false;
                let auth = self.auth.clone().unwrap();
                self.resim(level, auth);
            }
            let skip = self.skip.min(steps);
            self.skip -= skip;
            for _ in skip..steps {
                // far ahead of anything the host has said: wait for it rather than guess on
                if self.frame - self.auth_f >= MAX_AHEAD {
                    break;
                }
                let bits = self.o.held | self.o.tap;
                self.o.tap = 0;
                if bits != self.logged {
                    let c = (self.frame, bits);
                    self.log.push(c);
                    self.recent.push(c);
                    if self.recent.len() > 4 {
                        self.recent.remove(0);
                    }
                    self.logged = bits;
                    self.unsent = true;
                }
                let s = self.cur.take().unwrap();
                let n = if self.holding {
                    None
                } else {
                    self.predict(level, &s, bits)
                };
                match n {
                    Some(n) => {
                        for e in &n.events {
                            if e.p == Some(1) && own_kind(e.k) {
                                self.o.out.push(e.clone());
                            }
                        }
                        self.prev = Some(s);
                        self.cur = Some(n);
                    }
                    None => {
                        self.holding = true;
                        self.prev = Some(s.clone());
                        self.cur = Some(s);
                    }
                }
                self.frame += 1;
                for o in &mut self.offsets {
                    o.x = if o.x.abs() < 0.05 { 0.0 } else { o.x * SMOOTH };
                    o.y = if o.y.abs() < 0.05 { 0.0 } else { o.y * SMOOTH };
                }
            }
        }
        if self.o.greeted && (self.unsent || now - self.last_sent >= KEEP_MS) {
            self.unsent = false;
            self.last_sent = now;
            let m = EmberMsg::K {
                a: self.attempt.max(0) as f64,
                f: self.frame as f64,
                i: if self.o.screen == Screen::Play {
                    self.recent.clone()
                } else {
                    vec![]
                },
                r: if self.slow_rev >= 0 {
                    Some(self.slow_rev as f64)
                } else {
                    None
                },
            };
            self.o.post(&m);
        }
    }

    fn drawn(&self) -> (Option<GameState>, Option<GameState>) {
        let [a, b] = self.offsets;
        let shift = |s: &Option<GameState>| -> Option<GameState> {
            let s = s.as_ref()?;
            if a.x == 0.0 && a.y == 0.0 && b.x == 0.0 && b.y == 0.0 {
                return Some(s.clone());
            }
            let mut out = s.clone();
            out.players[0].x += a.x;
            out.players[0].y += a.y;
            out.players[1].x += b.x;
            out.players[1].y += b.y;
            Some(out)
        };
        (shift(&self.cur), shift(&self.prev))
    }
}

impl EmberGame for GuestGame {
    fn tick(&mut self, now: f64) -> EmberView {
        let (n, go) = self.o.begin_tick(now);
        if go {
            self.advance(n, now);
        }
        let drawn = if self.o.screen == Screen::Play {
            Some(self.drawn())
        } else {
            None
        };
        self.o.view(1, drawn)
    }
    fn keys(&mut self, _player: PlayerIndex, bits: i64) {
        self.o.keys(bits);
    }
    fn release(&mut self) {
        self.o.release();
    }
    fn pick(&mut self, index: f64) {
        if self.o.screen != Screen::Select {
            return;
        }
        self.o.level = clamp_level(index, self.o.cat.len());
        self.picked = Some((self.o.level, self.o.now + PICK_MS));
        let l = self.o.level as f64;
        self.o.post(&EmberMsg::Pick { l });
    }
    fn play(&mut self) {
        if self.o.screen == Screen::Select {
            let l = self.o.level as f64;
            self.o.post(&EmberMsg::Go { l });
        }
    }
    fn retry(&mut self) {
        if self.o.screen == Screen::Play {
            self.o.post(&EmberMsg::Retry);
        }
    }
    fn restart(&mut self) {
        if self.o.screen == Screen::Play {
            self.o.post(&EmberMsg::Restart);
        }
    }
    fn next(&mut self) {
        if self.o.screen == Screen::Play && self.auth.as_ref().map(|s| s.phase) == Some(Phase::Won)
        {
            self.o.post(&EmberMsg::Next);
        }
    }
    fn menu(&mut self) {
        if self.o.screen == Screen::Play {
            self.o.post(&EmberMsg::Menu);
        }
    }
    fn receive(&mut self, text: &str, now: f64) {
        if let Some(m) = self.o.receive(text, now) {
            self.handle(m, now);
        }
    }
    fn leave(&mut self) {
        self.o.leave();
    }
    fn take_outbox(&mut self) -> Vec<String> {
        std::mem::take(&mut self.o.outbox)
    }
}

/// start: the level highlighted on the select screen to begin with
pub fn create_local_game(now: f64, start: f64) -> Box<dyn EmberGame> {
    Box::new(LocalGame::new(shipped(), now, start))
}

/// host plays fire (0), guest plays frost (1). the host's highlight is the shared one
pub fn create_online_game(host: bool, now: f64, start: f64) -> Box<dyn EmberGame> {
    if host {
        Box::new(HostGame::new(shipped(), now, start))
    } else {
        Box::new(GuestGame::new(shipped(), now, start))
    }
}

#[cfg(test)]
mod tests;
