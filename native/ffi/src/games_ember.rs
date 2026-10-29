//! The games and the bar's easter eggs for the Mac's native panels (`qcalc_core::games`): Ember & Frost, Trails and
//! the 420/67/69 gates. The panels own the clock, the keys, the drawing and the sound; each frame they tick a
//! match and draw what comes back. Ember's stage and state cross as the TS's own JSON (the renderer is a line for
//! line port of emberDraw.ts and reads the same fields), everything else as records.

use std::sync::{Arc, Mutex};

use qcalc_core::games::eggs::{self, Egg, EggGates, KonamiTracker, LineEgg, LineEggGates, PuffAt};
use qcalc_core::games::ember::camera::{base_view, camera_at, ease_camera};
use qcalc_core::games::ember::engine::{gate_rect, stage_of, tile_at, trace_beams};
use qcalc_core::games::ember::json::{event_json, view_json};
use qcalc_core::games::ember::panel::{BannerKind, PanelTracker, grid_step, win_lines};
use qcalc_core::games::ember::progress::{first_unfinished, parse_progress, progress_json};
use qcalc_core::games::ember::session::{EmberGame, GuestGame, HostGame, LocalGame, Screen};
use qcalc_core::games::ember::sound::{Filter, SoundGate, Voice, Wave, voices};
use qcalc_core::games::ember::{
    self, Camera, Catalog, fmt_par, fmt_time, level_difficulty, level_length, shipped,
};
use qcalc_core::games::trails::arena::{ArenaKey, ArenaView, LocalArena, VS, online_view};
use qcalc_core::games::trails::session::{Role, TrailsSession};

// ---- the 420 / 67 / 69 effects

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EggKind {
    /// 420: smoke rolls off the bar
    Smoke,
    /// 67: two hands weigh it up
    Arms,
    /// 69: the digits fold
    Fold,
}

impl From<Egg> for EggKind {
    fn from(e: Egg) -> Self {
        match e {
            Egg::Smoke => EggKind::Smoke,
            Egg::Arms => EggKind::Arms,
            Egg::Fold => EggKind::Fold,
        }
    }
}

/// One puff of the 420 smoke. `at`: where along the bar's top, 0–1, or -1 its left edge, -2 its right.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SmokePuff {
    pub at: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// seconds
    pub delay: f64,
    pub drift: f64,
    pub rise: f64,
    pub sway: f64,
}

/// how long an answer must rest on 420, 67 or 69 before its effect plays, ms
#[uniffi::export]
pub fn egg_settle_ms() -> f64 {
    eggs::SETTLE_MS
}

/// how long an effect stays up, ms
#[uniffi::export]
pub fn egg_play_ms(kind: EggKind) -> f64 {
    match kind {
        EggKind::Smoke => Egg::Smoke.play_ms(),
        EggKind::Arms => Egg::Arms.play_ms(),
        EggKind::Fold => Egg::Fold.play_ms(),
    }
}

#[uniffi::export]
pub fn smoke_puffs() -> Vec<SmokePuff> {
    eggs::PUFFS
        .iter()
        .map(|p| SmokePuff {
            at: match p.at {
                PuffAt::Left => -1.0,
                PuffAt::Right => -2.0,
                PuffAt::Frac(f) => f,
            },
            x: p.x,
            y: p.y,
            w: p.w,
            h: p.h,
            delay: p.delay,
            drift: p.drift,
            rise: p.rise,
            sway: p.sway,
        })
        .collect()
}

/// The three gates: fires once per settled 420, 67 or 69, and again only after the answer was something else.
#[derive(Debug, Default, uniffi::Object)]
pub struct EggGate {
    inner: Mutex<EggGates>,
}

#[uniffi::export]
impl EggGate {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// the answer as it settled (nil when it isn't a plain number): the effect to play, if any
    pub fn settle(&self, n: Option<f64>) -> Option<EggKind> {
        self.inner.lock().ok()?.settle(n).map(EggKind::from)
    }
}

// ---- the native-only line eggs and the Konami code

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LineEggKind {
    /// `x/0`: a black hole swallows the answer
    Void,
    /// `e^(iπ)+1`: confetti and a note
    Euler,
    /// `pi` on March 14: more digits
    PiDigits,
    /// `do a barrel roll`
    BarrelRoll,
}

impl From<LineEgg> for LineEggKind {
    fn from(e: LineEgg) -> Self {
        match e {
            LineEgg::Void => LineEggKind::Void,
            LineEgg::Euler => LineEggKind::Euler,
            LineEgg::PiDigits => LineEggKind::PiDigits,
            LineEgg::BarrelRoll => LineEggKind::BarrelRoll,
        }
    }
}

/// how long a line egg stays up, ms
#[uniffi::export]
pub fn line_egg_play_ms(kind: LineEggKind) -> f64 {
    match kind {
        LineEggKind::Void => LineEgg::Void.play_ms(),
        LineEggKind::Euler => LineEgg::Euler.play_ms(),
        LineEggKind::PiDigits => LineEgg::PiDigits.play_ms(),
        LineEggKind::BarrelRoll => LineEgg::BarrelRoll.play_ms(),
    }
}

/// the whole line is `do a barrel roll` (the bar keeps the squiggle off it)
#[uniffi::export]
pub fn is_barrel_roll(text: String) -> bool {
    eggs::is_barrel_roll(&text)
}

/// the whole line is the Konami code typed out, or `konami`
#[uniffi::export]
pub fn is_konami_line(text: String) -> bool {
    eggs::is_konami_line(&text)
}

/// March 14
#[uniffi::export]
pub fn is_pi_day(month: u32, day: u32) -> bool {
    eggs::is_pi_day(month, day)
}

/// π to 50 places
#[uniffi::export]
pub fn pi_digits() -> String {
    eggs::PI_DIGITS.to_string()
}

/// The line eggs' gates: each fires once per settled line that sets it off, and again only after one that didn't.
#[derive(Debug, Default, uniffi::Object)]
pub struct LineEggGate {
    inner: Mutex<LineEggGates>,
}

#[uniffi::export]
impl LineEggGate {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// the line as it settled, the answer the bar shows and its number: the effect to play, if any
    pub fn settle(
        &self,
        text: String,
        display: String,
        number: Option<f64>,
        pi_day: bool,
    ) -> Option<LineEggKind> {
        self.inner
            .lock()
            .ok()?
            .settle(&text, &display, number, pi_day)
            .map(LineEggKind::from)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum KonamiKey {
    Up,
    Down,
    Left,
    Right,
    B,
    A,
    Other,
}

/// Watches the bar's keys for ↑↑↓↓←→←→BA.
#[derive(Debug, Default, uniffi::Object)]
pub struct KonamiGate {
    inner: Mutex<KonamiTracker>,
}

#[uniffi::export]
impl KonamiGate {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// how many keys of the code the last keys spell
    pub fn progress(&self) -> u32 {
        self.inner.lock().map(|t| t.progress() as u32).unwrap_or(0)
    }

    /// one key down; true when it completes the code
    pub fn push(&self, key: KonamiKey) -> bool {
        use eggs::KonamiKey as K;
        let key = match key {
            KonamiKey::Up => K::Up,
            KonamiKey::Down => K::Down,
            KonamiKey::Left => K::Left,
            KonamiKey::Right => K::Right,
            KonamiKey::B => K::B,
            KonamiKey::A => K::A,
            KonamiKey::Other => K::Other,
        };
        self.inner.lock().is_ok_and(|mut k| k.push(key))
    }
}

// ---- trails

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TrailsCycleView {
    pub x: i32,
    pub y: i32,
    pub dir: u8,
    /// the turn it's about to take
    pub next: Option<u8>,
    pub alive: bool,
    /// cell indexes from the start to the head
    pub path: Vec<i32>,
    pub crash_x: Option<i32>,
    pub crash_y: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TrailsFrame {
    pub cycles: Vec<TrailsCycleView>,
    /// the round has a result
    pub over: bool,
    /// how far the heads are into the next cell, 0–1
    pub t: f64,
    pub crash_ms: Option<f64>,
    pub score: Vec<u32>,
    pub names: Vec<String>,
    pub banner: String,
    pub big: bool,
    pub note: String,
    /// online: the line the lobby shows once the game is over for good ("" for none)
    pub ended: Option<String>,
    /// back to the lobby's menu
    pub to_menu: bool,
}

fn trails_frame(v: ArenaView) -> TrailsFrame {
    TrailsFrame {
        cycles: v
            .round
            .cycles
            .iter()
            .map(|c| TrailsCycleView {
                x: c.x,
                y: c.y,
                dir: c.dir,
                next: c.queue.first().copied(),
                alive: c.alive,
                path: c.path.clone(),
                crash_x: c.crash.map(|p| p.0),
                crash_y: c.crash.map(|p| p.1),
            })
            .collect(),
        over: v.round.result.is_some(),
        t: v.t,
        crash_ms: v.crash_ms,
        score: v.score.to_vec(),
        names: v.names.to_vec(),
        banner: v.banner,
        big: v.big,
        note: v.note,
        ended: v.ended,
        to_menu: v.to_menu,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TrailsKey {
    /// WASD: 0 up, 1 right, 2 down, 3 left
    Wasd {
        dir: u8,
    },
    Arrow {
        dir: u8,
    },
    Enter,
    Back,
}

#[allow(clippy::large_enum_variant)]
enum TrailsInner {
    Local(LocalArena),
    Online {
        session: Box<TrailsSession>,
        out: Arc<Mutex<Vec<String>>>,
        peer: String,
        me: usize,
    },
}

/// A trails match: against the computer or a friend on one keyboard, or online.
#[derive(uniffi::Object)]
pub struct TrailsMatch {
    inner: Mutex<TrailsInner>,
}

impl std::fmt::Debug for TrailsMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TrailsMatch")
    }
}

#[uniffi::export]
impl TrailsMatch {
    /// `vs`: the lobby's choice, 0 easy, 1 normal, 2 hard, 3 two players on one keyboard
    #[uniffi::constructor]
    pub fn local(vs: u32, now: f64, seed: u64) -> Arc<Self> {
        let vs = VS[(vs as usize).min(VS.len() - 1)];
        Arc::new(Self {
            inner: Mutex::new(TrailsInner::Local(LocalArena::new(vs, now, seed))),
        })
    }

    /// the host is the left cycle; `peer` is the friend's name for the lines
    #[uniffi::constructor]
    pub fn online(host: bool, peer: String, now: f64) -> Arc<Self> {
        let out = Arc::new(Mutex::new(vec![]));
        let o = out.clone();
        let session = TrailsSession::new(
            if host { Role::Host } else { Role::Guest },
            Box::new(move |t| {
                if let Ok(mut v) = o.lock() {
                    v.push(t);
                }
            }),
            now,
        );
        Arc::new(Self {
            inner: Mutex::new(TrailsInner::Online {
                session: Box::new(session),
                out,
                peer,
                me: if host { 0 } else { 1 },
            }),
        })
    }

    pub fn key(&self, key: TrailsKey, repeat: bool, now: f64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let k = match key {
            TrailsKey::Wasd { dir } => ArenaKey::Wasd(dir % 4),
            TrailsKey::Arrow { dir } => ArenaKey::Arrow(dir % 4),
            TrailsKey::Enter => ArenaKey::Enter,
            TrailsKey::Back => ArenaKey::Back,
        };
        match &mut *inner {
            TrailsInner::Local(a) => a.key(k, repeat, now),
            TrailsInner::Online { session, .. } => match k {
                ArenaKey::Back => session.leave(),
                ArenaKey::Enter => {
                    if !repeat {
                        session.rematch(now);
                    }
                }
                ArenaKey::Wasd(d) | ArenaKey::Arrow(d) => {
                    if !repeat {
                        session.turn(d);
                    }
                }
            },
        }
    }

    pub fn tick(&self, now: f64) -> Option<TrailsFrame> {
        let mut inner = self.inner.lock().ok()?;
        Some(match &mut *inner {
            TrailsInner::Local(a) => trails_frame(a.tick(now)),
            TrailsInner::Online {
                session, peer, me, ..
            } => {
                let v = session.tick(now);
                let mut f = trails_frame(online_view(v, *me, peer));
                // backspace's leave: the lobby goes back to its menu with nothing to say
                if f.ended.as_deref() == Some("") {
                    f.to_menu = true;
                }
                f
            }
        })
    }

    /// a message from the link
    pub fn receive(&self, text: String, now: f64) {
        if let Ok(mut inner) = self.inner.lock()
            && let TrailsInner::Online { session, .. } = &mut *inner
        {
            session.receive(&text, now);
        }
    }

    /// messages to send, oldest first
    pub fn take_outbox(&self) -> Vec<String> {
        let Ok(inner) = self.inner.lock() else {
            return vec![];
        };
        match &*inner {
            TrailsInner::Local(_) => vec![],
            TrailsInner::Online { out, .. } => out
                .lock()
                .map(|mut v| std::mem::take(&mut *v))
                .unwrap_or_default(),
        }
    }

    /// tell the other side we're going
    pub fn leave(&self) {
        if let Ok(mut inner) = self.inner.lock()
            && let TrailsInner::Online { session, .. } = &mut *inner
        {
            session.leave();
        }
    }

    /// the head line under the name: 'one keyboard', 'vs the computer · hard'
    pub fn sub(&self) -> String {
        let Ok(inner) = self.inner.lock() else {
            return String::new();
        };
        match &*inner {
            TrailsInner::Local(a) => a.sub(),
            TrailsInner::Online { .. } => String::new(),
        }
    }
}

// ---- ember & frost

/// a level on the select screen
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EmberLevelCard {
    pub id: String,
    pub name: String,
    pub stages: u32,
    pub difficulty: u32,
    /// '3 stages · 8 screens'
    pub length: String,
    pub par: f64,
    pub par_text: String,
    pub gem_total: u32,
    pub best: Option<EmberBest>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EmberBest {
    pub stars: u32,
    pub secs: Option<f64>,
    pub secs_text: Option<String>,
    pub gems: u32,
    pub deaths: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EmberScoreView {
    pub secs: f64,
    pub gems: Vec<i64>,
    pub total: Vec<i64>,
    pub stars: i64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EmberHud {
    /// play, dead, clear, won; nil off the play screen
    pub phase: Option<String>,
    pub deaths: i64,
    pub stage: u32,
    pub stages: u32,
    pub cp: i64,
    pub cps: u32,
    pub gems: Vec<i64>,
    pub total: Vec<i64>,
    pub keys: i64,
    /// the clock has passed the target time
    pub over: bool,
    pub banner_id: u64,
    /// hint, flag, back, ouch
    pub banner_kind: Option<String>,
    pub banner_title: Option<String>,
    pub banner_text: String,
    pub show_banner: bool,
    pub entry: u64,
    pub score: Option<EmberScoreView>,
    /// the clock, 0:07.4, and the target, 0:45
    pub clock: String,
    pub par_text: String,
    pub level_name: String,
    /// the win card: its line, what this finish beat, and the keys line
    pub win_line: String,
    pub win_new: Option<String>,
    pub win_foot: String,
    /// "stage 2 clear", "stage 3 of 4 is next"
    pub clear_title: String,
    pub clear_note: String,
}

/// one note or hiss of a sound, starting `at` seconds in (see emberSound.ts)
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EmberVoice {
    pub at: f64,
    /// a tone, else filtered noise
    pub tone: bool,
    pub freq: f64,
    pub dur: f64,
    pub vol: f64,
    /// tones: 0 sine, 1 square, 2 triangle, 3 sawtooth
    pub wave: u8,
    pub glide_to: Option<f64>,
    pub q: f64,
    pub sweep_to: Option<f64>,
    /// noise: 0 bandpass, 1 lowpass, 2 highpass
    pub filter: u8,
    /// where in the 0.6 s noise loop it starts
    pub offset: f64,
}

fn voice(v: &Voice) -> EmberVoice {
    match *v {
        Voice::Tone {
            at,
            freq,
            dur,
            vol,
            wave,
            glide_to,
        } => EmberVoice {
            at,
            tone: true,
            freq,
            dur,
            vol,
            wave: match wave {
                Wave::Sine => 0,
                Wave::Square => 1,
                Wave::Triangle => 2,
                Wave::Sawtooth => 3,
            },
            glide_to,
            q: 1.0,
            sweep_to: None,
            filter: 0,
            offset: 0.0,
        },
        Voice::Hiss {
            at,
            dur,
            vol,
            freq,
            q,
            sweep_to,
            filter,
            offset,
        } => EmberVoice {
            at,
            tone: false,
            freq,
            dur,
            vol,
            wave: 0,
            glide_to: None,
            q,
            sweep_to,
            filter: match filter {
                Filter::Bandpass => 0,
                Filter::Lowpass => 1,
                Filter::Highpass => 2,
            },
            offset,
        },
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EmberFrame {
    /// wait, select, play
    pub screen: String,
    pub level: u32,
    /// online: the character this side plays
    pub me: Option<u8>,
    /// online: the line the lobby shows once the link is over ("" for none)
    pub ended: Option<String>,
    pub note: Option<String>,
    pub alpha: f64,
    pub playing: bool,
    /// the state to draw and the one a step before, as the TS's JSON (the checkpoint snapshot left out)
    pub state_json: Option<String>,
    pub prev_json: Option<String>,
    /// everything that happened since the last tick, as the TS's JSON list
    pub events_json: String,
    /// every tile of the stage as it behaves now (frozen water, lights, crumbled blocks), row-major
    pub tiles: Vec<u8>,
    /// the light beams, x1 y1 x2 y2 each
    pub beams: Vec<f64>,
    /// every gate where it has slid to, x y w h each
    pub gates: Vec<f64>,
    pub hud: EmberHud,
    /// the renderer forgets its particles and camera: a level began, or began again
    pub reset_renderer: bool,
    /// the progress file changed (a finish, the mute switch): save `progress_json`
    pub progress_changed: bool,
    pub muted: bool,
    /// this frame's sounds, one list per event (empty when muted)
    pub sounds: Vec<EmberVoice>,
}

struct EmberInner {
    game: Box<dyn EmberGame>,
    cat: Arc<Catalog>,
    tracker: PanelTracker,
    gate: SoundGate,
    rand: u64,
    mute_changed: bool,
}

/// An ember & frost game, local (both on one keyboard) or online (the host is ember), with the panel's hud and
/// the progress file.
#[derive(uniffi::Object)]
pub struct EmberMatch {
    inner: Mutex<EmberInner>,
}

impl std::fmt::Debug for EmberMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EmberMatch")
    }
}

fn screen_name(s: Screen) -> &'static str {
    match s {
        Screen::Wait => "wait",
        Screen::Select => "select",
        Screen::Play => "play",
    }
}

impl EmberMatch {
    fn make(online: Option<bool>, now: f64, progress: Option<String>) -> Arc<Self> {
        let cat = shipped();
        let progress = parse_progress(progress.as_deref());
        let ids: Vec<&str> = cat.defs.iter().map(|d| d.id.as_str()).collect();
        let start = first_unfinished(&progress, &ids) as f64;
        let game: Box<dyn EmberGame> = match online {
            None => Box::new(LocalGame::new(cat.clone(), now, start)),
            Some(true) => Box::new(HostGame::new(cat.clone(), now, start)),
            Some(false) => Box::new(GuestGame::new(cat.clone(), now, start)),
        };
        let tracker = PanelTracker::new(online.is_some(), progress);
        let seed = (now.to_bits() ^ 0x9e37_79b9_7f4a_7c15) | 1;
        Arc::new(Self {
            inner: Mutex::new(EmberInner {
                game,
                cat,
                tracker,
                gate: SoundGate::default(),
                rand: seed,
                mute_changed: false,
            }),
        })
    }
}

fn xorshift(s: &mut u64) -> f64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

#[uniffi::export]
impl EmberMatch {
    /// both characters on one keyboard. `progress`: the saved progress file, if any
    #[uniffi::constructor]
    pub fn local(now: f64, progress: Option<String>) -> Arc<Self> {
        Self::make(None, now, progress)
    }

    /// host plays ember (fire), guest frost
    #[uniffi::constructor]
    pub fn online(host: bool, now: f64, progress: Option<String>) -> Arc<Self> {
        Self::make(Some(host), now, progress)
    }

    pub fn tick(&self, now: f64) -> Option<EmberFrame> {
        let mut guard = self.inner.lock().ok()?;
        let inner = &mut *guard;
        let v = inner.game.tick(now);
        let hud = inner.tracker.frame(&inner.cat, &v);
        let progress_changed =
            inner.tracker.progress_changed || std::mem::take(&mut inner.mute_changed);
        let muted = inner.tracker.progress.muted;
        let level = if v.playing {
            inner.cat.get(v.level)
        } else {
            None
        };
        let def = inner.cat.defs.get(v.level);
        let mut tiles = vec![];
        let mut beams = vec![];
        let mut gates = vec![];
        if let (Some(level), Some(s)) = (level, v.state.as_ref()) {
            let stage = stage_of(level, s.stage);
            tiles.reserve(stage.cols * stage.rows);
            for ty in 0..stage.rows as i64 {
                for tx in 0..stage.cols as i64 {
                    tiles.push(tile_at(stage, s, tx, ty));
                }
            }
            for b in trace_beams(stage, s) {
                beams.extend([b.x1, b.y1, b.x2, b.y2]);
            }
            for i in 0..stage.gates.len() {
                let r = gate_rect(stage, s, i);
                gates.extend([r.x, r.y, r.w, r.h]);
            }
        }
        let mut events_json = String::from("[");
        for (i, e) in v.events.iter().enumerate() {
            if i > 0 {
                events_json.push(',');
            }
            event_json(&mut events_json, e);
        }
        events_json.push(']');
        let mut sounds = vec![];
        if !muted {
            for e in &v.events {
                if inner.gate.pass(e, now) {
                    let mut r = || xorshift(&mut inner.rand);
                    sounds.extend(voices(e, &mut r).iter().map(voice));
                }
            }
        }
        let (win_line, win_new, win_foot) = match (&hud.score, def) {
            (Some(sc), Some(d)) => win_lines(
                sc,
                d.par,
                hud.deaths,
                hud.fresh.as_ref(),
                v.level + 1 >= inner.cat.len(),
            ),
            _ => (String::new(), None, String::new()),
        };
        let clear_note = if hud.stage + 2 >= hud.stages {
            "the last stage is next".to_string()
        } else {
            format!("stage {} of {} is next", hud.stage + 2, hud.stages)
        };
        let banner = hud.banner.clone();
        Some(EmberFrame {
            screen: screen_name(v.screen).into(),
            level: v.level as u32,
            me: v.me.map(|m| m as u8),
            ended: v.ended.clone(),
            note: v.note.clone(),
            alpha: v.alpha,
            playing: v.playing && v.state.is_some(),
            state_json: v.state.as_ref().map(view_json),
            prev_json: v.prev.as_ref().map(view_json),
            events_json,
            tiles,
            beams,
            gates,
            hud: EmberHud {
                phase: hud.phase.map(|p| p.name().to_string()),
                deaths: hud.deaths,
                stage: hud.stage as u32,
                stages: hud.stages as u32,
                cp: hud.cp,
                cps: hud.cps as u32,
                gems: hud.gems.to_vec(),
                total: hud.total.to_vec(),
                keys: hud.keys,
                over: hud.over,
                banner_id: banner.as_ref().map_or(0, |b| b.id),
                banner_kind: banner.as_ref().map(|b| {
                    match b.kind {
                        BannerKind::Hint => "hint",
                        BannerKind::Flag => "flag",
                        BannerKind::Back => "back",
                        BannerKind::Ouch => "ouch",
                    }
                    .to_string()
                }),
                banner_title: banner.as_ref().and_then(|b| b.title.clone()),
                banner_text: banner.map(|b| b.text).unwrap_or_default(),
                show_banner: hud.show_banner,
                entry: hud.entry,
                score: hud.score.map(|s| EmberScoreView {
                    secs: s.secs,
                    gems: s.gems.to_vec(),
                    total: s.total.to_vec(),
                    stars: s.stars,
                }),
                clock: hud.clock,
                par_text: def.map_or(String::new(), |d| fmt_par(d.par)),
                level_name: def.map_or(String::new(), |d| d.name.clone()),
                win_line,
                win_new,
                win_foot,
                clear_title: format!("stage {} clear", hud.stage + 1),
                clear_note,
            },
            reset_renderer: inner.tracker.reset_renderer,
            progress_changed,
            muted,
            sounds,
        })
    }

    /// held keys: local per player (0 ember, 1 frost), online this side's character
    pub fn keys(&self, player: u8, bits: i64) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.keys(usize::from(player.min(1)), bits);
        }
    }

    pub fn release(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.release();
        }
    }

    pub fn pick(&self, index: i64) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.pick(index as f64);
        }
    }

    pub fn play(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.play();
        }
    }

    pub fn retry(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.retry();
        }
    }

    pub fn restart(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.restart();
        }
    }

    pub fn next(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.next();
        }
    }

    pub fn menu(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.menu();
        }
    }

    pub fn receive(&self, text: String, now: f64) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.receive(&text, now);
        }
    }

    pub fn leave(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.game.leave();
        }
    }

    pub fn take_outbox(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|mut i| i.game.take_outbox())
            .unwrap_or_default()
    }

    /// M: the mute switch, kept in the progress file
    pub fn toggle_mute(&self) {
        if let Ok(mut i) = self.inner.lock() {
            i.tracker.toggle_mute();
            i.mute_changed = true;
        }
    }

    /// the progress file as saveProgress writes it, under the save key `ember_save_key()`
    pub fn progress_json(&self) -> String {
        self.inner
            .lock()
            .map(|i| progress_json(&i.tracker.progress))
            .unwrap_or_default()
    }

    /// every level for the select screen, with the best saved for it
    pub fn levels(&self) -> Vec<EmberLevelCard> {
        let Ok(i) = self.inner.lock() else {
            return vec![];
        };
        let n = i.cat.len();
        i.cat
            .defs
            .iter()
            .enumerate()
            .map(|(k, d)| {
                let totals = i.cat.get(k).map_or([0, 0], |l| l.gem_totals);
                EmberLevelCard {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    stages: d.stages.len() as u32,
                    difficulty: level_difficulty(k, n),
                    length: level_length(d),
                    par: d.par,
                    par_text: fmt_par(d.par),
                    gem_total: (totals[0] + totals[1]) as u32,
                    best: i
                        .tracker
                        .progress
                        .levels
                        .get(&d.id)
                        .filter(|b| b.completed)
                        .map(|b| EmberBest {
                            stars: b.stars as u32,
                            secs: b.secs,
                            secs_text: b.secs.map(fmt_time),
                            gems: b.gems as u32,
                            deaths: b.deaths,
                        }),
                }
            })
            .collect()
    }

    /// a compiled stage as the TS's JSON (the renderer's `Stage`), nil when there's no such stage
    pub fn stage_json(&self, level: u32, stage: u32) -> Option<String> {
        let i = self.inner.lock().ok()?;
        let st = i.cat.get(level as usize)?.stages.get(stage as usize)?;
        serde_json::to_string(st).ok()
    }
}

/// the save key ember's progress lives under (the web build's localStorage key)
#[uniffi::export]
pub fn ember_save_key() -> String {
    ember::progress::SAVE_KEY.into()
}

fn cam(v: &[f64]) -> Option<Camera> {
    (v.len() >= 5).then(|| Camera {
        x: v[0],
        y: v[1],
        w: v[2],
        h: v[3],
        zoom: v[4],
    })
}

/// emberCamera.ts cameraFor, for the players as drawn: x y w h zoom
#[uniffi::export]
pub fn ember_camera_for(
    stage_w: f64,
    stage_h: f64,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    aspect: f64,
) -> Vec<f64> {
    let c = camera_at(stage_w, stage_h, (ax, ay), (bx, by), aspect);
    vec![c.x, c.y, c.w, c.h, c.zoom]
}

/// emberCamera.ts easeCamera: from (empty for none) toward `to` over `ticks` steps
#[uniffi::export]
pub fn ember_ease_camera(from: Vec<f64>, to: Vec<f64>, ticks: f64) -> Vec<f64> {
    let Some(to) = cam(&to) else { return vec![] };
    let c = ease_camera(cam(&from).as_ref(), &to, ticks);
    vec![c.x, c.y, c.w, c.h, c.zoom]
}

/// emberCamera.ts baseView: w h
#[uniffi::export]
pub fn ember_base_view(aspect: f64) -> Vec<f64> {
    let (w, h) = base_view(aspect);
    vec![w, h]
}

/// the select grid's arrows
#[uniffi::export]
pub fn ember_grid_step(from: i64, dx: i64, dy: i64, count: i64, cols: i64) -> i64 {
    grid_step(from, dx, dy, count, cols)
}

/// the running clock and best times: 0:07.4
#[uniffi::export]
pub fn ember_fmt_time(secs: f64) -> String {
    fmt_time(secs)
}
