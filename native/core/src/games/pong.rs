//! `src/lib/pong.ts`: two-player pong over the peer link, and solo against the computer. solo runs `step`; online,
//! each side judges the ball at its own paddle and sends the next leg of its flight (see `pong_session`).
//! JS numbers throughout: `Math.cos`/`sin`/`exp` through fdlibm (`libm`), `Math.hypot` as V8 has it, and `Math.min`/
//! `max` letting NaN through, so a solo game plays frame for frame as the page's did.

use serde_json::Value as Json;

use super::hello::{LinkedGame, other_hello};
use super::{Out, Rand, Side, finite, js_hypot, js_round, parse, stringify};

/// 2: each side judges its own paddle. 1 had the host judge both.
pub const PONG_VERSION: f64 = 2.0;
pub const COURT_W: f64 = 640.0;
pub const COURT_H: f64 = 260.0;
pub const PADDLE_W: f64 = 8.0;
pub const PADDLE_H: f64 = 52.0;
pub const PADDLE_INSET: f64 = 18.0;
/// px per second
pub const PADDLE_SPEED: f64 = 420.0;
pub const BALL_SIZE: f64 = 8.0;
pub const SERVE_SPEED: f64 = 300.0;
pub const MAX_SPEED: f64 = 720.0;
pub const SPEED_UP: f64 = 1.06;
/// Steepest return off a paddle's edge.
pub const MAX_BOUNCE: f64 = (55.0 * std::f64::consts::PI) / 180.0;
pub const MAX_SERVE_ANGLE: f64 = (25.0 * std::f64::consts::PI) / 180.0;
pub const SERVE_PAUSE_S: f64 = 0.9;
pub const WIN_SCORE: u32 = 7;
/// Fixed simulation step; at max speed the ball moves 6px a step, under a paddle's width.
pub const STEP_S: f64 = 1.0 / 120.0;
/// Nothing from the other side for this long and the game is over.
pub const SILENCE_MS: f64 = 3000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Serve,
    Play,
    Over,
}

impl Phase {
    /// `phase.length` (the frame digest of the TS test counts it).
    pub fn name(self) -> &'static str {
        match self {
            Phase::Serve => "serve",
            Phase::Play => "play",
            Phase::Over => "over",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Ball {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

/// Host on the left, guest on the right; paddle ys are centres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PongState {
    pub ball: Ball,
    pub paddles: [f64; 2],
    pub score: [u32; 2],
    pub phase: Phase,
    /// Seconds until the serve, while phase is serve.
    pub serve_in: f64,
    /// +1 serves toward the guest, -1 toward the host.
    pub serve_dir: f64,
    pub winner: Option<Side>,
}

// Math.min / Math.max: NaN wins
pub(crate) fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
pub(crate) fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}
fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    jmin(hi, jmax(lo, v))
}
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

pub fn clamp_paddle(y: f64) -> f64 {
    clamp(y, PADDLE_H / 2.0, COURT_H - PADDLE_H / 2.0)
}

/// dir is -1 up, 0 still, 1 down.
pub fn move_paddle(y: f64, dir: f64, dt: f64) -> f64 {
    clamp_paddle(y + sign(dir) * PADDLE_SPEED * dt)
}

pub fn centre_ball() -> Ball {
    Ball {
        x: COURT_W / 2.0,
        y: COURT_H / 2.0,
        vx: 0.0,
        vy: 0.0,
    }
}

pub fn new_game(serve_dir: f64) -> PongState {
    PongState {
        ball: centre_ball(),
        paddles: [COURT_H / 2.0, COURT_H / 2.0],
        score: [0, 0],
        phase: Phase::Serve,
        serve_in: SERVE_PAUSE_S,
        serve_dir,
        winner: None,
    }
}

/// rand in [0, 1) picks the serve angle.
pub fn serve(dir: f64, rand: Rand) -> Ball {
    let angle = (rand() * 2.0 - 1.0) * MAX_SERVE_ANGLE;
    Ball {
        x: COURT_W / 2.0,
        y: COURT_H / 2.0,
        vx: libm::cos(angle) * SERVE_SPEED * dir,
        vy: libm::sin(angle) * SERVE_SPEED,
    }
}

/// The return angle follows where the ball met the paddle: centre sends it flat, the edges steep.
pub fn bounce_off(ball: Ball, paddle_y: f64, dir: f64) -> Ball {
    let reach = PADDLE_H / 2.0 + BALL_SIZE / 2.0;
    let offset = clamp((ball.y - paddle_y) / reach, -1.0, 1.0);
    let speed = jmin(MAX_SPEED, js_hypot(ball.vx, ball.vy) * SPEED_UP);
    let angle = offset * MAX_BOUNCE;
    Ball {
        vx: libm::cos(angle) * speed * dir,
        vy: libm::sin(angle) * speed,
        ..ball
    }
}

pub fn overlaps_paddle(y: f64, paddle_y: f64) -> bool {
    (y - paddle_y).abs() <= PADDLE_H / 2.0 + BALL_SIZE / 2.0
}

/// One fixed step of the ball: walls, paddles, points. paddles are moved by the caller first.
pub fn step(s: PongState, dt: f64, rand: Rand) -> PongState {
    if s.phase == Phase::Over {
        return s;
    }
    if s.phase == Phase::Serve {
        let serve_in = s.serve_in - dt;
        if serve_in > 0.0 {
            return PongState { serve_in, ..s };
        }
        return PongState {
            phase: Phase::Play,
            serve_in: 0.0,
            ball: serve(s.serve_dir, rand),
            ..s
        };
    }
    let half = BALL_SIZE / 2.0;
    let Ball {
        mut x,
        mut y,
        vx,
        mut vy,
    } = s.ball;
    let px = x;
    x += vx * dt;
    y += vy * dt;
    if y < half {
        y = 2.0 * half - y;
        vy = vy.abs();
    } else if y > COURT_H - half {
        y = 2.0 * (COURT_H - half) - y;
        vy = -vy.abs();
    }
    let mut ball = Ball { x, y, vx, vy };
    let left_face = PADDLE_INSET + PADDLE_W;
    let right_face = COURT_W - PADDLE_INSET - PADDLE_W;
    // only the face that faces the court, and only as the ball crosses it
    if vx < 0.0
        && px - half >= left_face
        && x - half < left_face
        && overlaps_paddle(y, s.paddles[0])
    {
        ball = bounce_off(
            Ball {
                x: left_face + half,
                ..ball
            },
            s.paddles[0],
            1.0,
        );
    } else if vx > 0.0
        && px + half <= right_face
        && x + half > right_face
        && overlaps_paddle(y, s.paddles[1])
    {
        ball = bounce_off(
            Ball {
                x: right_face - half,
                ..ball
            },
            s.paddles[1],
            -1.0,
        );
    }
    if ball.x + half < 0.0 {
        return point(s, Side::Guest);
    }
    if ball.x - half > COURT_W {
        return point(s, Side::Host);
    }
    PongState { ball, ..s }
}

fn point(s: PongState, scorer: Side) -> PongState {
    let score = if scorer == Side::Host {
        [s.score[0] + 1, s.score[1]]
    } else {
        [s.score[0], s.score[1] + 1]
    };
    let won = score[0] >= WIN_SCORE || score[1] >= WIN_SCORE;
    PongState {
        score,
        ball: centre_ball(),
        phase: if won { Phase::Over } else { Phase::Serve },
        serve_in: if won { 0.0 } else { SERVE_PAUSE_S },
        // toward whoever just lost the point
        serve_dir: if scorer == Side::Host { 1.0 } else { -1.0 },
        winner: if won { Some(scorer) } else { None },
        ..s
    }
}

/// Steps whole STEP_S slices out of real time; the rest carries to the next frame.
pub fn advance(s: PongState, carry: f64, dt: f64, rand: Rand) -> (PongState, f64) {
    let mut left = carry + jmin(dt, 0.25);
    let mut state = s;
    while left >= STEP_S {
        state = step(state, STEP_S, rand);
        left -= STEP_S;
    }
    (state, left)
}

/// What a court draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    /// Shared clock, ms.
    pub at: f64,
    pub ball: Ball,
    pub paddles: [f64; 2],
    pub score: [u32; 2],
    pub phase: Phase,
    pub winner: Option<Side>,
}

pub fn snapshot_of(s: &PongState, at: f64) -> Snapshot {
    Snapshot {
        at,
        ball: s.ball,
        paddles: s.paddles,
        score: s.score,
        phase: s.phase,
        winner: s.winner,
    }
}

// ---- online play. each side judges its own paddle: while the ball heads toward a side, that side alone decides
// the hit or the miss, against its own paddle where it is now, and sends the next leg of the ball's flight. the
// other side flies the ball on from that leg by a shared clock, the host's, and guesses the far bounce from the
// far paddle as it draws it until the real leg comes in. the score only changes in a leg, and only the side the
// ball was heading to sends one, so the two can't both count a point

/// Where the ball's centre meets each paddle's face.
pub const HOST_FACE_X: f64 = PADDLE_INSET + PADDLE_W + BALL_SIZE / 2.0;
pub const GUEST_FACE_X: f64 = COURT_W - PADDLE_INSET - PADDLE_W - BALL_SIZE / 2.0;
/// A correction fades rather than jumps, to a third in this long.
pub const EASE_MS: f64 = 50.0;
/// One bigger than this jumps instead.
pub const SNAP_PX: f64 = 96.0;
/// A leg that reaches the side it's heading to later than this, after a stall, is taken up from this far in.
pub const LATE_MS: f64 = 200.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegKind {
    Play,
    Serve,
    Over,
}

const KINDS: [LegKind; 3] = [LegKind::Play, LegKind::Serve, LegKind::Over];

/// A stretch of the ball's flight between two judgements. game counts games, n the legs within one. at is shared
/// time in ms: when the leg starts, or for a serve when the ball leaves the centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub game: u64,
    pub n: u64,
    pub at: f64,
    pub kind: LegKind,
    pub ball: Ball,
    pub score: [u32; 2],
}

pub fn towards(b: &Ball) -> Side {
    if b.vx > 0.0 { Side::Guest } else { Side::Host }
}

pub fn fade(ms: f64) -> f64 {
    libm::exp(-jmax(0.0, ms) / EASE_MS)
}

pub fn newer(a: &Leg, b: Option<&Leg>) -> bool {
    b.is_none_or(|b| a.game > b.game || (a.game == b.game && a.n > b.n))
}

pub fn winner_of(score: [u32; 2]) -> Option<Side> {
    if score[0] >= WIN_SCORE {
        Some(Side::Host)
    } else if score[1] >= WIN_SCORE {
        Some(Side::Guest)
    } else {
        None
    }
}

/// The ball s seconds on, off the walls but through the paddles, which are judged apart.
pub fn fly(b: Ball, s: f64) -> Ball {
    let half = BALL_SIZE / 2.0;
    let span = COURT_H - 2.0 * half;
    let m = (((b.y - half + b.vy * s) % (2.0 * span)) + 2.0 * span) % (2.0 * span);
    let down = m <= span;
    Ball {
        x: b.x + b.vx * s,
        y: half + if down { m } else { 2.0 * span - m },
        vx: b.vx,
        vy: if down { b.vy } else { -b.vy },
    }
}

/// Seconds until the ball's centre reaches x; 0 if it's already there or past.
pub fn seconds_to(b: &Ball, x: f64) -> f64 {
    if b.vx == 0.0 || b.vx.is_nan() {
        return f64::INFINITY;
    }
    jmax(0.0, (x - b.x) / b.vx)
}

// ---- wire messages, as json text over the peer link

/// Every message carries the sender's own clock as c. the host's also echo the guest's latest c as e, with how
/// long it held it as h: that's the guest's round trip, and from it the host's clock.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Stamp {
    pub c: Option<f64>,
    pub e: Option<f64>,
    pub h: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PongMsg {
    /// ok: the sender has already heard the other side's hi. g: sent as `pong`.
    Hi {
        v: f64,
        g: bool,
        ok: bool,
        stamp: Stamp,
    },
    /// A paddle at shared time at and the way it's going; game and n: the newest leg the sender has.
    P {
        at: f64,
        y: f64,
        d: f64,
        game: u64,
        n: u64,
        stamp: Stamp,
    },
    L {
        leg: Leg,
        stamp: Stamp,
    },
    Again {
        stamp: Stamp,
    },
    Bye,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PongIn {
    Msg(PongMsg),
    Other(String),
}

fn round1(n: f64) -> f64 {
    js_round(n * 10.0) / 10.0
}

fn stamp_out(s: &Stamp, f: &mut Vec<(&'static str, Out)>) {
    for (k, v) in [("c", s.c), ("e", s.e), ("h", s.h)] {
        if let Some(v) = v {
            f.push((k, Out::N(round1(v))));
        }
    }
}

/// A leg as it goes over the wire; the side that sends one keeps this copy too, so both fly the very same ball.
pub fn wire_leg(l: &Leg) -> Leg {
    Leg {
        at: round1(l.at),
        ball: Ball {
            x: round1(l.ball.x),
            y: round1(l.ball.y),
            vx: round1(l.ball.vx),
            vy: round1(l.ball.vy),
        },
        ..*l
    }
}

pub fn encode_pong(msg: &PongMsg) -> String {
    let t = |s: &str| ("t", Out::S(s.into()));
    let mut f: Vec<(&str, Out)> = vec![];
    match msg {
        PongMsg::P {
            at,
            y,
            d,
            game,
            n,
            stamp,
        } => {
            f.extend([
                t("p"),
                ("at", Out::N(round1(*at))),
                ("y", Out::N(round1(*y))),
                ("d", Out::N(*d)),
                ("r", Out::N(*game as f64)),
                ("n", Out::N(*n as f64)),
            ]);
            stamp_out(stamp, &mut f);
        }
        PongMsg::L { leg, stamp } => {
            let l = wire_leg(leg);
            let k = KINDS.iter().position(|k| *k == l.kind).unwrap_or(0) as f64;
            let b = Out::A(vec![
                Out::N(l.ball.x),
                Out::N(l.ball.y),
                Out::N(l.ball.vx),
                Out::N(l.ball.vy),
            ]);
            let s = Out::A(vec![
                Out::N(f64::from(l.score[0])),
                Out::N(f64::from(l.score[1])),
            ]);
            f.extend([
                t("l"),
                ("r", Out::N(l.game as f64)),
                ("n", Out::N(l.n as f64)),
                ("at", Out::N(l.at)),
                ("k", Out::N(k)),
                ("b", b),
                ("s", s),
            ]);
            stamp_out(stamp, &mut f);
        }
        PongMsg::Hi { v, g, ok, stamp } => {
            f.extend([t("hi"), ("v", Out::N(*v))]);
            if *g {
                f.push(("g", Out::S("pong".into())));
            }
            if *ok {
                f.push(("ok", Out::B(true)));
            }
            stamp_out(stamp, &mut f);
        }
        PongMsg::Again { stamp } => {
            f.push(t("again"));
            stamp_out(stamp, &mut f);
        }
        PongMsg::Bye => f.push(t("bye")),
    }
    stringify(&f)
}

// Number.isInteger(v) && v >= 0 && v < 1e9
fn pcount(v: Option<&Json>) -> Option<u64> {
    finite(v)
        .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n < 1e9)
        .map(|n| n as u64)
}

fn numbers(v: Option<&Json>, n: usize) -> Option<Vec<f64>> {
    let a = v?.as_array()?;
    if a.len() != n {
        return None;
    }
    a.iter()
        .map(|x| x.as_f64().filter(|f| f.is_finite()))
        .collect()
}

fn read_stamp(o: &serde_json::Map<String, Json>) -> Stamp {
    Stamp {
        c: finite(o.get("c")),
        e: finite(o.get("e")),
        h: finite(o.get("h")),
    }
}

/// Anything malformed is None: this is text from another machine. Another game's hello comes back as Other.
pub fn decode_pong(raw: &str) -> Option<PongIn> {
    let m = parse(raw)?;
    let o = m.as_object()?;
    if let Some(other) = other_hello(o, LinkedGame::Pong) {
        return Some(PongIn::Other(other.into()));
    }
    let msg = match o.get("t").and_then(Json::as_str)? {
        "hi" => PongMsg::Hi {
            v: finite(o.get("v"))?,
            g: false,
            ok: o.get("ok") == Some(&Json::Bool(true)),
            stamp: read_stamp(o),
        },
        "p" => {
            let at = finite(o.get("at"))?;
            let y = finite(o.get("y"))?;
            let d = finite(o.get("d")).filter(|d| *d == -1.0 || *d == 0.0 || *d == 1.0)?;
            let game = pcount(o.get("r"))?;
            let n = pcount(o.get("n"))?;
            PongMsg::P {
                at,
                y: clamp_paddle(y),
                d,
                game,
                n,
                stamp: read_stamp(o),
            }
        }
        "l" => {
            let game = pcount(o.get("r"))?;
            let n = pcount(o.get("n"))?;
            let at = finite(o.get("at"))?;
            let b = numbers(o.get("b"), 4)?;
            let s = numbers(o.get("s"), 2)?;
            let k = finite(o.get("k"))?;
            let kind = if k.fract() == 0.0 && (0.0..3.0).contains(&k) {
                KINDS[k as usize]
            } else {
                return None;
            };
            // a ball in flight has to be going somewhere, or nobody would ever judge it
            if kind != LegKind::Over && b[2].abs() < 1.0 {
                return None;
            }
            let half = BALL_SIZE / 2.0;
            let ball = Ball {
                x: clamp(b[0], -BALL_SIZE, COURT_W + BALL_SIZE),
                y: clamp(b[1], half, COURT_H - half),
                vx: clamp(b[2], -MAX_SPEED, MAX_SPEED),
                vy: clamp(b[3], -MAX_SPEED, MAX_SPEED),
            };
            let score = [
                clamp(s[0].floor(), 0.0, 99.0) as u32,
                clamp(s[1].floor(), 0.0, 99.0) as u32,
            ];
            PongMsg::L {
                leg: Leg {
                    game,
                    n,
                    at,
                    kind,
                    ball,
                    score,
                },
                stamp: read_stamp(o),
            }
        }
        "again" => PongMsg::Again {
            stamp: read_stamp(o),
        },
        "bye" => PongMsg::Bye,
        _ => return None,
    };
    Some(PongIn::Msg(msg))
}

// ---- the clock and the far paddle

/// The guest's guess at the host's clock. a round trip pins the offset to within half its lopsidedness, so the
/// quickest recent one wins; before the first, the host's stamps as they land, as if they took no time.
#[derive(Clone, Debug, PartialEq)]
pub struct HostClock {
    trips: Vec<(f64, f64)>,
    rough: f64,
}

impl Default for HostClock {
    fn default() -> Self {
        HostClock {
            trips: vec![],
            rough: f64::NEG_INFINITY,
        }
    }
}

impl HostClock {
    pub fn heard(&mut self, s: &Stamp, now: f64) {
        let Some(c) = s.c else { return };
        self.rough = jmax(self.rough, c - now);
        let (Some(e), Some(h)) = (s.e, s.h) else {
            return;
        };
        let rtt = now - e - h;
        if !(0.0..10_000.0).contains(&rtt) {
            return;
        }
        self.trips.push((rtt, c + rtt / 2.0 - now));
        if self.trips.len() > 40 {
            self.trips.remove(0);
        }
    }

    /// The host's clock minus ours.
    pub fn offset(&self) -> f64 {
        let mut best: Option<(f64, f64)> = None;
        for t in &self.trips {
            if best.is_none_or(|b| t.0 < b.0) {
                best = Some(*t);
            }
        }
        if let Some(b) = best {
            return b.1;
        }
        if self.rough.is_finite() {
            self.rough
        } else {
            0.0
        }
    }
}

/// The other side's paddle, carried on at paddle speed from its last report; a report that moves it eases in.
#[derive(Clone, Debug, PartialEq)]
pub struct FarPaddle {
    y: f64,
    d: f64,
    at: f64,
    seen: f64,
    off: f64,
    off_at: f64,
}

impl Default for FarPaddle {
    fn default() -> Self {
        FarPaddle {
            y: COURT_H / 2.0,
            d: 0.0,
            at: 0.0,
            seen: f64::NEG_INFINITY,
            off: 0.0,
            off_at: 0.0,
        }
    }
}

impl FarPaddle {
    /// Where it is at shared time t, by the reports.
    pub fn where_at(&self, t: f64) -> f64 {
        clamp_paddle(self.y + (self.d * PADDLE_SPEED * clamp(t - self.at, 0.0, 1000.0)) / 1000.0)
    }

    /// Where it's drawn at shared time t, our time now.
    pub fn shown(&self, t: f64, now: f64) -> f64 {
        clamp_paddle(self.where_at(t) + self.off * fade(now - self.off_at))
    }

    pub fn report(&mut self, y: f64, d: f64, at: f64, t: f64, now: f64) {
        if at < self.seen {
            return;
        }
        let was = self.shown(t, now);
        self.y = y;
        self.d = d;
        self.at = at;
        self.seen = at;
        let off = was - self.where_at(t);
        self.off = if off.abs() > SNAP_PX { 0.0 } else { off };
        self.off_at = now;
    }
}

// ---- the computer, for solo games: it plays the guest's paddle on the right

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    Easy,
    Normal,
    Hard,
}

impl Skill {
    pub const ALL: [Skill; 3] = [Skill::Easy, Skill::Normal, Skill::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Skill::Easy => "easy",
            Skill::Normal => "normal",
            Skill::Hard => "hard",
        }
    }

    // reach: the share of the court where it starts following an incoming ball; speed: of a player's paddle; miss:
    // how far off its aim can land, in paddle heights, for a ball at serve speed
    fn bot(self) -> (f64, f64, f64) {
        match self {
            Skill::Easy => (0.45, 0.55, 0.7),
            Skill::Normal => (0.7, 0.8, 0.45),
            Skill::Hard => (1.0, 1.0, 0.3),
        }
    }
}

/// What the computer keeps between frames: where on its paddle it means to meet the ball, picked once per rally.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BotMind {
    pub aim: f64,
    pub coming: bool,
}

/// Where the ball will be, top to bottom, when it reaches x, folding its path off the walls.
pub fn landing_y(ball: &Ball, x: f64) -> f64 {
    if ball.vx == 0.0 {
        return ball.y;
    }
    let t = (x - ball.x) / ball.vx;
    if t <= 0.0 {
        return ball.y;
    }
    let half = BALL_SIZE / 2.0;
    let span = COURT_H - 2.0 * half;
    let raw = ball.y - half + ball.vy * t;
    let m = ((raw % (2.0 * span)) + 2.0 * span) % (2.0 * span);
    half + if m <= span { m } else { 2.0 * span - m }
}

/// One frame of the computer's paddle. it heads for where the ball will land once the ball is near enough, and
/// drifts back to the middle while the ball goes the other way.
pub fn bot_step(
    s: &PongState,
    y: f64,
    mind: BotMind,
    skill: Skill,
    dt: f64,
    rand: Rand,
) -> (f64, BotMind) {
    let (reach, speed, miss) = skill.bot();
    let coming = s.phase == Phase::Play && s.ball.vx > 0.0;
    // a new rally toward it: decide how well this one goes
    let pace = js_hypot(s.ball.vx, s.ball.vy) / SERVE_SPEED;
    let next = if coming && !mind.coming {
        BotMind {
            aim: (rand() * 2.0 - 1.0) * miss * pace * PADDLE_H,
            coming,
        }
    } else {
        BotMind { coming, ..mind }
    };
    let face = COURT_W - PADDLE_INSET - PADDLE_W;
    let seen = coming && s.ball.x >= face - reach * COURT_W;
    let target = if seen {
        landing_y(&s.ball, face - BALL_SIZE / 2.0) + next.aim
    } else {
        COURT_H / 2.0
    };
    let gap = target - y;
    // a dead zone, so it doesn't shiver once it's there
    if gap.abs() < 2.0 {
        return (y, next);
    }
    let most = PADDLE_SPEED * speed * dt;
    (clamp_paddle(y + clamp(gap, -most, most)), next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::mulberry32;

    fn half() -> impl FnMut() -> f64 {
        || 0.5
    }
    fn playing(f: impl FnOnce(&mut PongState)) -> PongState {
        let mut s = PongState {
            phase: Phase::Play,
            serve_in: 0.0,
            ..new_game(1.0)
        };
        f(&mut s);
        s
    }
    fn ball(x: f64, y: f64, vx: f64, vy: f64) -> Ball {
        Ball { x, y, vx, vy }
    }
    fn step_for(s: PongState, seconds: f64) -> PongState {
        let mut state = s;
        let mut t = 0.0;
        while t < seconds {
            state = step(state, STEP_S, &mut half());
            t += STEP_S;
        }
        state
    }
    fn close(a: f64, b: f64, digits: i32) -> bool {
        (a - b).abs() < 10f64.powi(-digits) / 2.0
    }

    #[test]
    fn paddles_and_serves() {
        assert_eq!(clamp_paddle(-50.0), PADDLE_H / 2.0);
        assert_eq!(clamp_paddle(COURT_H + 50.0), COURT_H - PADDLE_H / 2.0);
        assert!(close(
            move_paddle(100.0, 1.0, 0.1),
            100.0 + PADDLE_SPEED * 0.1,
            2
        ));
        assert!(close(
            move_paddle(100.0, -3.0, 0.1),
            100.0 - PADDLE_SPEED * 0.1,
            2
        ));
        assert_eq!(move_paddle(100.0, 0.0, 0.1), 100.0);
        let s = step(new_game(-1.0), SERVE_PAUSE_S / 2.0, &mut half());
        assert_eq!(s.phase, Phase::Serve);
        let s = step(s, SERVE_PAUSE_S, &mut half());
        assert_eq!(s.phase, Phase::Play);
        assert!(close(s.ball.vx, -SERVE_SPEED, 2) && close(s.ball.vy, 0.0, 2));
        let s = step(new_game(1.0), SERVE_PAUSE_S + 0.01, &mut || 0.999);
        assert!(close(js_hypot(s.ball.vx, s.ball.vy), SERVE_SPEED, 2));
        assert!(
            s.ball.vy > 0.0
                && crate::scientific::functions::jsmath::atan2(s.ball.vy, s.ball.vx).abs() < 0.45
        );
    }

    #[test]
    fn steps() {
        let next = step(
            playing(|s| s.ball = ball(300.0, BALL_SIZE / 2.0 + 1.0, 0.0, -300.0)),
            STEP_S,
            &mut half(),
        );
        assert!(next.ball.vy > 0.0 && next.ball.y >= BALL_SIZE / 2.0);
        let low = step(
            playing(|s| s.ball = ball(300.0, COURT_H - BALL_SIZE / 2.0 - 1.0, 0.0, 300.0)),
            STEP_S,
            &mut half(),
        );
        assert!(low.ball.vy < 0.0);
        let face = PADDLE_INSET + PADDLE_W;
        let s = playing(|s| {
            s.ball = ball(face + BALL_SIZE / 2.0 + 1.0, 130.0, -400.0, 0.0);
            s.paddles = [130.0, 130.0];
        });
        let next = step(s, STEP_S, &mut half());
        assert!(
            close(next.ball.vx, 400.0 * 1.06, 2)
                && close(next.ball.vy, 0.0, 2)
                && close(next.ball.x, face + BALL_SIZE / 2.0, 2)
        );
        let rface = COURT_W - PADDLE_INSET - PADDLE_W;
        let next = step(
            playing(|s| {
                s.ball = ball(rface - BALL_SIZE / 2.0 - 1.0, 150.0, 400.0, 0.0);
                s.paddles = [130.0, 130.0];
            }),
            STEP_S,
            &mut half(),
        );
        assert!(next.ball.vx < 0.0 && next.ball.vy > 0.0);
        let s = step_for(
            playing(|s| {
                s.ball = ball(60.0, 30.0, -500.0, 0.0);
                s.paddles = [220.0, 130.0];
            }),
            0.5,
        );
        assert_eq!(
            (s.score, s.phase, s.serve_dir),
            ([0, 1], Phase::Serve, -1.0)
        );
        assert_eq!((s.ball.x, s.ball.y), (COURT_W / 2.0, COURT_H / 2.0));
        let s = playing(|s| {
            s.ball = ball(PADDLE_INSET - 2.0, 130.0, 300.0, 0.0);
            s.paddles = [130.0, 130.0];
        });
        assert_eq!(step(s, STEP_S, &mut half()).ball.vx, 300.0);
        let mut x = face + BALL_SIZE / 2.0;
        while x < face + BALL_SIZE / 2.0 + 7.0 {
            let s = playing(|s| {
                s.ball = ball(x, 130.0, -MAX_SPEED, 0.0);
                s.paddles = [130.0, 130.0];
            });
            assert!(step_for(s, 0.05).ball.vx > 0.0);
            x += 0.37;
        }
        let s = step_for(
            playing(|s| {
                s.score = [WIN_SCORE - 1, 3];
                s.ball = ball(COURT_W - 20.0, 30.0, 600.0, 0.0);
                s.paddles = [130.0, 220.0];
            }),
            0.2,
        );
        assert_eq!((s.phase, s.winner), (Phase::Over, Some(Side::Host)));
        assert_eq!(step_for(s, 1.0), s);
        let (_, carry) = advance(new_game(1.0), 0.0, STEP_S * 2.5, &mut half());
        assert!(close(carry, STEP_S * 0.5, 6));
        let (long, _) = advance(
            playing(|s| s.ball = ball(320.0, 130.0, 100.0, 0.0)),
            0.0,
            10.0,
            &mut half(),
        );
        assert!(close(long.ball.x, 320.0 + 100.0 * 0.25, 0));
        let b = bounce_off(ball(0.0, 200.0, -MAX_SPEED, 0.0), 100.0, 1.0);
        assert!(
            close(
                crate::scientific::functions::jsmath::atan2(b.vy, b.vx),
                MAX_BOUNCE,
                2
            ) && close(js_hypot(b.vx, b.vy), MAX_SPEED, 2)
        );
    }

    #[test]
    fn flight_and_legs() {
        let b0 = ball(320.0, 40.0, 400.0, -650.0);
        let mut s = playing(|s| {
            s.ball = b0;
            s.paddles = [130.0, 130.0];
        });
        for _ in 0..60 {
            s = step(s, STEP_S, &mut half());
        }
        let b = fly(b0, 60.0 * STEP_S);
        assert!(close(b.x, s.ball.x, 6) && close(b.y, s.ball.y, 6) && b.vy == s.ball.vy);
        assert!(close(
            seconds_to(&ball(100.0, 0.0, 200.0, 0.0), GUEST_FACE_X),
            (GUEST_FACE_X - 100.0) / 200.0,
            6
        ));
        assert!(close(
            seconds_to(&ball(100.0, 0.0, -200.0, 0.0), HOST_FACE_X),
            (100.0 - HOST_FACE_X) / 200.0,
            6
        ));
        assert_eq!(seconds_to(&ball(10.0, 0.0, -200.0, 0.0), HOST_FACE_X), 0.0);
        let leg = Leg {
            game: 2,
            n: 5,
            at: 1234.56,
            kind: LegKind::Play,
            ball: ball(100.123, 50.46, -300.04, 12.5),
            score: [3, 6],
        };
        let msg = decode_pong(&encode_pong(&PongMsg::L {
            leg,
            stamp: Stamp {
                c: Some(99.99),
                ..Default::default()
            },
        }));
        assert_eq!(
            msg,
            Some(PongIn::Msg(PongMsg::L {
                leg: wire_leg(&leg),
                stamp: Stamp {
                    c: Some(100.0),
                    ..Default::default()
                }
            }))
        );
        let w = wire_leg(&leg);
        assert_eq!((w.at, w.ball), (1234.6, ball(100.1, 50.5, -300.0, 12.5)));
        assert!(newer(&Leg { n: 6, ..leg }, Some(&leg)));
        assert!(newer(
            &Leg {
                game: 3,
                n: 0,
                ..leg
            },
            Some(&leg)
        ));
        assert!(!newer(&leg, Some(&leg)));
        assert!(!newer(&Leg { n: 4, ..leg }, Some(&leg)));
        assert!(newer(&leg, None));
        assert_eq!(winner_of([WIN_SCORE, 2]), Some(Side::Host));
        assert_eq!(winner_of([2, WIN_SCORE]), Some(Side::Guest));
        assert_eq!(winner_of([6, 6]), None);
    }

    #[test]
    fn the_wire() {
        let st = |c, e, h| Stamp { c, e, h };
        for m in [
            PongMsg::Hi {
                v: 2.0,
                g: false,
                ok: false,
                stamp: Stamp::default(),
            },
            PongMsg::Hi {
                v: 2.0,
                g: false,
                ok: true,
                stamp: st(Some(5.0), Some(1.5), Some(2.0)),
            },
            PongMsg::Again {
                stamp: st(Some(7.0), None, None),
            },
            PongMsg::Bye,
            PongMsg::P {
                at: 10.5,
                y: 120.0,
                d: -1.0,
                game: 1,
                n: 4,
                stamp: Stamp::default(),
            },
        ] {
            assert_eq!(decode_pong(&encode_pong(&m)), Some(PongIn::Msg(m.clone())));
        }
        assert_eq!(
            encode_pong(&PongMsg::Hi {
                v: 2.0,
                g: true,
                ok: true,
                stamp: st(Some(1000.04), Some(5.0), Some(2.25))
            }),
            r#"{"t":"hi","v":2,"g":"pong","ok":true,"c":1000,"e":5,"h":2.3}"#
        );
        let Some(PongIn::Msg(PongMsg::P { y, .. })) =
            decode_pong(r#"{"t":"p","at":1,"y":99999,"d":0,"r":0,"n":0}"#)
        else {
            panic!()
        };
        assert_eq!(y, COURT_H - PADDLE_H / 2.0);
        let Some(PongIn::Msg(PongMsg::L { leg, .. })) = decode_pong(
            r#"{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[9999,-5,1e300,-1e300],"s":[-3,400.7]}"#,
        ) else {
            panic!()
        };
        assert_eq!(
            (leg.ball, leg.score),
            (
                ball(COURT_W + BALL_SIZE, BALL_SIZE / 2.0, MAX_SPEED, -MAX_SPEED),
                [0, 99]
            )
        );
        assert_eq!(
            decode_pong(r#"{"t":"hi","v":1,"ok":"yes","c":"x"}"#),
            Some(PongIn::Msg(PongMsg::Hi {
                v: 1.0,
                g: false,
                ok: false,
                stamp: Stamp::default()
            }))
        );
        for raw in [
            "",
            "nope",
            "null",
            "[]",
            r#"{"t":"x"}"#,
            r#"{"t":"p"}"#,
            r#"{"t":"p","y":3}"#,
            r#"{"t":"hi"}"#,
            r#"{"t":"s","at":1,"b":[1,2,3,4],"p":[0,0],"s":[0,0],"ph":1}"#,
            r#"{"t":"p","at":1,"y":3,"d":2,"r":0,"n":0}"#,
            r#"{"t":"p","at":1,"y":3,"d":0,"r":-1,"n":0}"#,
            r#"{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[1,2,3],"s":[0,0]}"#,
            r#"{"t":"l","r":1,"n":1,"at":1,"k":7,"b":[1,2,3,4],"s":[0,0]}"#,
            r#"{"t":"l","r":1,"n":1.5,"at":1,"k":0,"b":[1,2,3,4],"s":[0,0]}"#,
            r#"{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[1,2,3,null],"s":[0,0]}"#,
            r#"{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[1,2,0,4],"s":[0,0]}"#,
        ] {
            assert_eq!(decode_pong(raw), None, "{raw}");
        }
        assert_eq!(
            decode_pong(r#"{"t":"c4","v":1}"#),
            Some(PongIn::Other("connect 4".into()))
        );
    }

    #[test]
    fn host_clock_and_far_paddle() {
        let st = |c, e, h| Stamp { c, e, h };
        let mut clock = HostClock::default();
        assert_eq!(clock.offset(), 0.0);
        clock.heard(&st(Some(5000.0 + 960.0), None, None), 1000.0);
        assert_eq!(clock.offset(), 4960.0);
        clock.heard(&st(Some(5000.0 + 1960.0), Some(1900.0), Some(20.0)), 2000.0);
        assert!(close(clock.offset(), 5000.0, 2));
        clock.heard(&st(Some(5000.0 + 2900.0), Some(2700.0), Some(10.0)), 3000.0);
        assert!(close(clock.offset(), 5000.0, 2));
        clock.heard(&st(Some(5000.0 + 3975.0), Some(3940.0), Some(10.0)), 4000.0);
        assert!(close(clock.offset(), 5000.0, 2));
        let mut p = FarPaddle::default();
        p.report(100.0, 1.0, 1000.0, 1000.0, -1000.0);
        assert!(close(p.where_at(1100.0), 100.0 + PADDLE_SPEED * 0.1, 2));
        assert_eq!(p.where_at(10_000.0), COURT_H - PADDLE_H / 2.0);
        p.report(120.0, 0.0, 1050.0, 1100.0, 100.0);
        assert!(close(p.shown(1100.0, 100.0), 100.0 + PADDLE_SPEED * 0.1, 2));
        assert!(close(p.shown(1100.0, 100.0 + 6.0 * EASE_MS), 120.0, 0));
        p.report(30.0, 0.0, 1020.0, 1100.0, 400.0);
        assert_eq!(p.where_at(1200.0), 120.0);
    }

    const FACE: f64 = COURT_W - PADDLE_INSET - PADDLE_W - BALL_SIZE / 2.0;

    #[test]
    fn the_computer() {
        assert!(close(
            landing_y(&ball(100.0, 100.0, 100.0, 10.0), 200.0),
            110.0,
            2
        ));
        let h = BALL_SIZE / 2.0;
        assert!(close(
            landing_y(&ball(0.0, COURT_H - h - 50.0, 100.0, 150.0), 100.0),
            COURT_H - h - 100.0,
            2
        ));
        assert!(close(
            landing_y(&ball(0.0, h + 50.0, 100.0, -150.0), 100.0),
            h + 100.0,
            2
        ));
        let mut r = mulberry32(7);
        for _ in 0..500 {
            let b = ball(
                0.0,
                r() * COURT_H,
                50.0 + r() * 600.0,
                (r() * 2.0 - 1.0) * 900.0,
            );
            let y = landing_y(&b, COURT_W);
            assert!(y >= h - 1e-9 && y <= COURT_H - h + 1e-9);
        }
        let play = |b| PongState {
            phase: Phase::Play,
            ball: b,
            ..new_game(1.0)
        };
        let s = play(ball(300.0, 200.0, -300.0, 0.0));
        let (mut y, mut mind) = (30.0, BotMind::default());
        for _ in 0..200 {
            (y, mind) = bot_step(&s, y, mind, Skill::Hard, STEP_S, &mut half());
        }
        assert!(close(y, COURT_H / 2.0, 0));
        let s = play(ball(FACE - 20.0, 40.0, 300.0, 0.0));
        let before = COURT_H / 2.0;
        let (mut y, mut mind) = bot_step(
            &s,
            before,
            BotMind::default(),
            Skill::Hard,
            0.01,
            &mut half(),
        );
        assert!(close(before - y, 4.2, 2));
        for _ in 0..200 {
            (y, mind) = bot_step(&s, y, mind, Skill::Hard, STEP_S, &mut half());
        }
        assert!((y - clamp_paddle(40.0)).abs() < 2.5);
        let s = play(ball(60.0, 40.0, 300.0, 0.0));
        assert_eq!(
            bot_step(
                &s,
                COURT_H / 2.0,
                BotMind::default(),
                Skill::Easy,
                0.01,
                &mut half()
            )
            .0,
            COURT_H / 2.0
        );
    }

    fn points_against(skill: Skill, games: usize, seed: u32) -> (u32, u32) {
        let mut rand = mulberry32(seed);
        let (mut won, mut lost) = (0, 0);
        for _ in 0..games {
            let mut s = new_game(if rand() < 0.5 { 1.0 } else { -1.0 });
            let (mut bot, mut mind) = (COURT_H / 2.0, BotMind::default());
            let mut i = 0;
            while i < 120 * 60 * 10 && s.phase != Phase::Over {
                (bot, mind) = bot_step(&s, bot, mind, skill, STEP_S, &mut rand);
                s = step(
                    PongState {
                        paddles: [clamp_paddle(s.ball.y), bot],
                        ..s
                    },
                    STEP_S,
                    &mut rand,
                );
                i += 1;
            }
            won += s.score[0];
            lost += s.score[1];
        }
        (won, lost)
    }

    fn rally_length(skill: Skill, rallies: usize, seed: u32) -> f64 {
        let mut rand = mulberry32(seed);
        let mut returns = 0.0;
        for _ in 0..rallies {
            let mut s = PongState {
                serve_in: 0.0,
                ..new_game(1.0)
            };
            let (mut bot, mut mind) = (COURT_H / 2.0, BotMind::default());
            let mut was = false;
            for _ in 0..120 * 120 {
                (bot, mind) = bot_step(&s, bot, mind, skill, STEP_S, &mut rand);
                let next = step(
                    PongState {
                        paddles: [clamp_paddle(s.ball.y), bot],
                        ..s
                    },
                    STEP_S,
                    &mut rand,
                );
                if next.score[0] != s.score[0] {
                    break;
                }
                let coming = next.phase == Phase::Play && next.ball.vx > 0.0;
                if was && !coming && next.phase == Phase::Play {
                    returns += 1.0;
                }
                was = coming;
                s = next;
            }
        }
        returns / rallies as f64
    }

    #[test]
    fn the_computer_by_level() {
        for k in Skill::ALL {
            assert_eq!(points_against(k, 12, 42).0, 12 * WIN_SCORE);
        }
        let [easy, normal, hard] = Skill::ALL.map(|k| rally_length(k, 300, 9));
        assert!(
            easy < normal && normal < hard && easy > 1.0,
            "{easy} {normal} {hard}"
        );
    }
}
