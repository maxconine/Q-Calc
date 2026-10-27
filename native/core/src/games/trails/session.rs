//! One side of an online trails match, without the page (`src/lib/trailsSession.ts`): the arena feeds it keys,
//! messages and frame times, and draws what tick() returns.
//!
//! Lockstep: both sides run the same deterministic round and swap only their turns. A turn pressed now is stamped
//! with a tick `delay` ticks ahead and sent at once, so it has usually arrived by the time either side gets there;
//! every tick gets a frame, turn or not, so neither side ever waits on silence. A tick is only stepped once both
//! frames for it are in. If one is late the round holds still ("waiting for friend…") and picks up when it lands.
//!
//! Each message repeats the frames the other side hasn't acknowledged, so a lost or late one heals with the next.
//! The guest sends a running hash of the round now and then; if the host's differs it sends its whole game (a
//! snapshot) and the guest carries on from that. The same snapshot starts a match and a rematch.

use super::net::{
    COUNT_MS, Frame, MAX_DELAY, MAX_FRAMES, NetPhase, POINT_MS, Snap, TRAILS_VERSION, TrailsMsg, decode_trails, encode_trails,
    hash_start, hash_step,
};
use super::{Dir, Player, QUEUE_MAX, Round, TICK_MS, match_winner, new_round, score_round, steer, step};

/// hello goes out this often until the other side's comes in
const HI_MS: f64 = 400.0;
/// with no frames going each side says it's still there this often
pub const KEEP_MS: f64 = 1000.0;
/// nothing from the other side for this long and the game is over
pub const TRAILS_SILENCE_MS: f64 = 5000.0;
/// a stall shorter than this isn't worth a banner
pub const STALL_SHOW_MS: f64 = 300.0;
/// held up this long, say so again
const RESEND_MS: f64 = 250.0;
/// the host measures the link before the first round: this many pings, this far apart, for at most this long
const PINGS: usize = 3;
const PING_MS: f64 = 150.0;
const PING_WAIT_MS: f64 = 900.0;
/// what a frame needs on top of the trip itself
const MARGIN_MS: f64 = 30.0;
/// one-way, when nothing has been measured
const DEFAULT_LAG_MS: f64 = 60.0;
/// the guest's hash goes out every this many ticks
pub const HASH_EVERY: u32 = 16;
/// the guest still talking in an old epoch this long after a snapshot never got it
const RESYNC_MS: f64 = 700.0;

const COUNT_TOTAL_MS: f64 = COUNT_MS * 3.0;
const fn opposite(d: Dir) -> Dir {
    (d + 2) % 4
}
const fn turns(heading: Dir, to: Dir) -> bool {
    to != heading && to != opposite(heading)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrailsPhase {
    Wait,
    Count,
    Run,
    Point,
    Over,
}

impl From<NetPhase> for TrailsPhase {
    fn from(p: NetPhase) -> Self {
        match p {
            NetPhase::Count => TrailsPhase::Count,
            NetPhase::Run => TrailsPhase::Run,
            NetPhase::Point => TrailsPhase::Point,
            NetPhase::Over => TrailsPhase::Over,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Again {
    pub mine: bool,
    pub theirs: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrailsView {
    /// what to draw; each cycle's queue holds the turn it's about to take, so heads glide the right way
    pub round: Round,
    pub score: [u32; 2],
    pub phase: TrailsPhase,
    pub count: u32,
    /// who took the last round; None is a draw
    pub last: Option<Player>,
    pub winner: Option<Player>,
    /// how far the heads are into the next cell, 0–1
    pub t: f64,
    /// how long ago the round ended
    pub crash_ms: Option<f64>,
    /// the other side's turns are late and the round is holding still
    pub stalled: bool,
    pub again: Again,
    /// set once the game is over for good: the line the lobby shows
    pub ended: Option<String>,
}

/// for tests: where the game stands
#[derive(Clone, Debug, PartialEq)]
pub struct TrailsState {
    pub r: i64,
    pub tick: u32,
    pub hash: u32,
    pub score: [u32; 2],
    pub epoch: u32,
    pub delay: u32,
    pub phase: TrailsPhase,
}

struct Prev {
    r: i64,
    hashes: Vec<Option<u32>>,
    mine: Vec<Frame>,
    sealed: i64,
}

fn put<T: Clone + Default>(v: &mut Vec<T>, i: usize, x: T) {
    if v.len() <= i {
        v.resize(i + 1, T::default());
    }
    v[i] = x;
}

fn slice<T: Clone + Default>(v: &[T], from: i64, to: i64) -> Vec<T> {
    let from = from.max(0) as usize;
    let to = to.max(0) as usize;
    (from..to).map(|i| v.get(i).cloned().unwrap_or_default()).collect()
}

pub struct TrailsSession {
    pub role: Role,
    /// the host is the left cycle
    pub me: Player,
    send: Box<dyn FnMut(String) + Send>,

    greeted: bool,
    greeted_at: f64,
    heard: f64,
    last_hi: f64,
    last_sent: f64,
    last_tick: f64,
    ended: Option<String>,

    rtts: Vec<f64>,
    last_ping: f64,
    snap_at: f64,
    prev: Option<Prev>,
    check: Option<(u32, u32)>,

    epoch: u32,
    r: i64,
    delay: u32,
    score: [u32; 2],
    round: Round,
    phase: TrailsPhase,
    phase_at: f64,
    run_at: f64,
    last: Option<Player>,
    acc: f64,
    again: Again,

    // turns by tick: mine as sent (a hole is no turn), theirs as heard (None: not heard)
    mine: Vec<Frame>,
    theirs: Vec<Option<Frame>>,
    sealed: i64,
    their_next: i64,
    acked: i64,
    queue: Vec<Dir>,
    heading: Dir,
    hashes: Vec<Option<u32>>,
    hash_due: Option<(u32, u32)>,
    dirty: bool,
    stall_at: Option<f64>,
}

impl std::fmt::Debug for TrailsSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrailsSession").field("role", &self.role).field("state", &self.state()).finish()
    }
}

impl TrailsSession {
    pub fn new(role: Role, send: Box<dyn FnMut(String) + Send>, now: f64) -> Self {
        let mut s = Self {
            role,
            me: if role == Role::Host { 0 } else { 1 },
            send,
            greeted: false,
            greeted_at: 0.0,
            heard: now,
            last_hi: now,
            last_sent: now,
            last_tick: now,
            ended: None,
            rtts: vec![],
            last_ping: f64::NEG_INFINITY,
            snap_at: f64::NEG_INFINITY,
            prev: None,
            check: None,
            epoch: 0,
            r: -1,
            delay: 2,
            score: [0, 0],
            round: new_round(),
            phase: TrailsPhase::Wait,
            phase_at: 0.0,
            run_at: 0.0,
            last: None,
            acc: 0.0,
            again: Again::default(),
            mine: vec![],
            theirs: vec![],
            sealed: -1,
            their_next: 0,
            acked: -1,
            queue: vec![],
            heading: 1,
            hashes: vec![],
            hash_due: None,
            dirty: false,
            stall_at: None,
        };
        s.hi();
        s
    }

    fn post(&mut self, m: TrailsMsg, now: Option<f64>) {
        if let Some(now) = now {
            self.last_sent = now;
        }
        (self.send)(encode_trails(&m));
    }

    fn hi(&mut self) {
        let ok = self.greeted;
        self.post(TrailsMsg::Hi { v: f64::from(TRAILS_VERSION), g: true, ok }, None);
    }

    fn end(&mut self, note: &str) {
        if self.ended.is_none() {
            self.ended = Some(note.into());
        }
    }

    fn hash_at(&self, tick: u32) -> Option<u32> {
        self.hashes.get(tick as usize).copied().flatten()
    }

    // ---- the round

    fn reset(&mut self, r: i64, round: Round, hash: u32) {
        if self.r >= 0 {
            self.prev = Some(Prev {
                r: self.r,
                hashes: std::mem::take(&mut self.hashes),
                mine: std::mem::take(&mut self.mine),
                sealed: self.sealed,
            });
        }
        self.r = r;
        let tick = round.tick;
        self.heading = round.cycles[self.me].dir;
        self.round = round;
        self.mine = vec![];
        self.theirs = vec![];
        self.sealed = i64::from(tick) - 1;
        self.their_next = i64::from(tick);
        self.acked = i64::from(tick) - 1;
        self.queue = vec![];
        self.hashes = vec![];
        put(&mut self.hashes, tick as usize, Some(hash));
        self.hash_due = if self.role == Role::Guest { Some((tick, hash)) } else { None };
        self.check = None;
        self.stall_at = None;
        self.acc = 0.0;
    }

    fn next_round(&mut self, run_at: f64) {
        let h = hash_start((self.r + 1) as u32, self.score);
        self.reset(self.r + 1, new_round(), h);
        self.phase = TrailsPhase::Count;
        self.run_at = run_at;
    }

    /// fixes my frames up to tick `to`: each takes the oldest turn waiting, or none
    fn seal_to(&mut self, to: i64) {
        while self.sealed < to {
            let turn = if self.queue.is_empty() { None } else { Some(self.queue.remove(0)) };
            self.sealed += 1;
            put(&mut self.mine, self.sealed as usize, turn);
            if let Some(t) = turn {
                self.heading = t;
            }
            self.dirty = true;
        }
    }

    /// a frame as heard: Some(None) is straight on, None not heard (or, for mine, not sealed)
    fn frame(&self, p: Player, tick: u32) -> Option<Frame> {
        if p == self.me {
            if i64::from(tick) > self.sealed { None } else { self.mine.get(tick as usize).copied() }
        } else {
            self.theirs.get(tick as usize).copied().flatten()
        }
    }

    fn step_once(&mut self, now: f64) {
        let mut round = self.round.clone();
        for p in 0..2 {
            if let Some(Some(turn)) = self.frame(p, round.tick) {
                round = steer(&round, p, turn);
            }
        }
        round = step(&round);
        let hash = hash_step(self.hash_at(round.tick - 1).unwrap_or(0), &round);
        put(&mut self.hashes, round.tick as usize, Some(hash));
        let tick = round.tick;
        let result = round.result;
        self.round = round;
        if let Some(res) = result {
            self.score = score_round(self.score, res);
            self.last = res.winner;
            self.phase = if match_winner(self.score).is_some() { TrailsPhase::Over } else { TrailsPhase::Point };
            self.phase_at = now;
            // the guest says how it ended, so a different ending is caught before the next round
            if self.role == Role::Guest {
                self.hash_due = Some((tick, hash));
                self.dirty = true;
            }
        } else {
            self.seal_to(i64::from(tick) + i64::from(self.delay) - 1);
            if self.role == Role::Guest && tick % HASH_EVERY == 0 {
                self.hash_due = Some((tick, hash));
            }
        }
        if let Some((ct, theirs)) = self.check
            && (ct == tick || result.is_some())
        {
            self.check = None;
            if ct != tick || theirs != hash {
                self.snapshot(now);
            }
        }
    }

    // ---- the host's side of keeping the two in step

    fn lag(&self) -> f64 {
        if self.rtts.is_empty() {
            return DEFAULT_LAG_MS;
        }
        let mut sorted = self.rtts.clone();
        sorted.sort_by(f64::total_cmp);
        sorted[(sorted.len() - 1) / 2] / 2.0
    }

    fn ping(&mut self, now: f64) {
        self.last_ping = now;
        self.post(TrailsMsg::Ping { c: crate::jsnum::round(now).max(0.0) }, Some(now));
    }

    /// a new match: the delay is set from the ping, and the guest starts from the snapshot
    fn start(&mut self, now: f64) {
        self.delay = (((self.lag() + MARGIN_MS) / TICK_MS).ceil().max(1.0) as u32).min(MAX_DELAY);
        self.score = [0, 0];
        self.last = None;
        self.again = Again::default();
        self.next_round(now + COUNT_TOTAL_MS);
        self.snapshot(now);
    }

    /// the whole game as the host has it. whatever the guest sent before it now counts for nothing
    fn snapshot(&mut self, now: f64) {
        self.epoch += 1;
        self.snap_at = now;
        self.check = None;
        // only frames with none missing before them go; the guest makes the rest again
        self.theirs.resize(self.their_next.max(0) as usize, None);
        let lag = self.lag();
        let tick = self.round.tick;
        let running = matches!(self.phase, TrailsPhase::Run | TrailsPhase::Count);
        let mut wait = 0.0;
        if self.phase == TrailsPhase::Count {
            wait = self.run_at - now - lag;
        } else if self.phase == TrailsPhase::Point {
            wait = self.phase_at + POINT_MS - now - lag;
        }
        let t = i64::from(tick);
        let mine: Vec<Frame> = if running { slice(&self.mine, t, self.sealed + 1).into_iter().take(MAX_FRAMES).collect() } else { vec![] };
        let theirs: Vec<Frame> = if running {
            slice(&self.theirs, t, self.their_next).into_iter().map(Option::flatten).take(MAX_FRAMES).collect()
        } else {
            vec![]
        };
        self.their_next = t + theirs.len() as i64;
        self.theirs.resize(self.their_next.max(0) as usize, None);
        let snap = Snap {
            e: self.epoch,
            r: self.r.max(0) as u32,
            d: self.delay,
            score: self.score,
            phase: match self.phase {
                TrailsPhase::Wait | TrailsPhase::Count => NetPhase::Count,
                TrailsPhase::Run => NetPhase::Run,
                TrailsPhase::Point => NetPhase::Point,
                TrailsPhase::Over => NetPhase::Over,
            },
            wait: wait.max(0.0),
            round: self.round.clone(),
            frames: [mine, theirs],
            hash: self.hash_at(tick).unwrap_or(0),
        };
        self.post(TrailsMsg::Snap(Box::new(snap)), Some(now));
    }

    /// the guest's hash for a tick, against mine
    fn verify(&mut self, r: i64, (tick, hash): (u32, u32), now: f64) {
        let same;
        if r == self.r {
            if tick > self.round.tick && self.round.result.is_none() {
                self.check = Some((tick, hash));
                return;
            }
            same = self.hash_at(tick) == Some(hash);
        } else if let Some(p) = self.prev.as_ref().filter(|p| p.r == r) {
            same = p.hashes.get(tick as usize).copied().flatten() == Some(hash);
        } else if r == self.r + 1 && self.phase == TrailsPhase::Point {
            // the guest is a moment ahead, into the next round
            return;
        } else {
            same = false;
        }
        if !same {
            self.snapshot(now);
        }
    }

    /// the guest is still in an old epoch: once the snapshot has had time to land, it didn't
    fn stale(&mut self, e: u32, now: f64) {
        if self.phase != TrailsPhase::Wait && e < self.epoch && now - self.snap_at > RESYNC_MS + self.lag() * 3.0 {
            self.snapshot(now);
        }
    }

    // ---- the guest's side

    fn adopt(&mut self, m: Snap, now: f64) {
        let tick = m.round.tick;
        self.epoch = m.e;
        self.delay = m.d;
        self.score = m.score;
        let winner = m.round.result.and_then(|x| x.winner);
        self.reset(i64::from(m.r), m.round, m.hash);
        let [theirs, mine] = m.frames;
        for (j, f) in theirs.iter().enumerate() {
            put(&mut self.theirs, tick as usize + j, Some(*f));
        }
        self.their_next = i64::from(tick) + theirs.len() as i64;
        for (j, f) in mine.iter().enumerate() {
            put(&mut self.mine, tick as usize + j, *f);
            if let Some(f) = f
                && turns(self.heading, *f)
            {
                self.heading = *f;
            }
        }
        self.sealed = i64::from(tick) + mine.len() as i64 - 1;
        self.acked = self.sealed;
        self.phase = m.phase.into();
        self.last = winner;
        match m.phase {
            NetPhase::Count => self.run_at = now + m.wait,
            NetPhase::Point => self.phase_at = now + m.wait - POINT_MS,
            NetPhase::Over => self.phase_at = now,
            NetPhase::Run => self.seal_to(i64::from(tick) + i64::from(self.delay) - 1),
        }
        if m.phase != NetPhase::Over {
            self.again = Again::default();
        }
        self.last_tick = now;
        self.dirty = true;
    }

    // ---- what the arena calls

    /// a turn pressed: it takes the next free tick. the same way or straight back does nothing, as offline
    pub fn turn(&mut self, dir: Dir) {
        if self.ended.is_some() || !matches!(self.phase, TrailsPhase::Count | TrailsPhase::Run) || self.queue.len() >= QUEUE_MAX {
            return;
        }
        if turns(self.queue.last().copied().unwrap_or(self.heading), dir) {
            self.queue.push(dir);
        }
    }

    /// enter: asks for a rematch once the match is over; the host starts it when both have asked
    pub fn rematch(&mut self, now: f64) {
        if self.ended.is_some() || self.phase != TrailsPhase::Over || self.again.mine {
            return;
        }
        if self.role == Role::Host && self.again.theirs {
            self.start(now);
            return;
        }
        self.again.mine = true;
        let e = self.epoch;
        self.post(TrailsMsg::Again { e }, Some(now));
    }

    /// back to the menu: the other side is told rather than left to time out
    pub fn leave(&mut self) {
        if self.ended.is_some() {
            return;
        }
        self.post(TrailsMsg::Bye, None);
        self.end("");
    }

    pub fn receive(&mut self, text: &str, now: f64) {
        if self.ended.is_some() {
            return;
        }
        let Some(m) = decode_trails(text) else { return };
        self.heard = now;
        match m {
            TrailsMsg::Hi { v, ok, .. } => {
                if v != f64::from(TRAILS_VERSION) {
                    self.post(TrailsMsg::Bye, None);
                    self.end("update both Q Calcs to play each other");
                    return;
                }
                if !self.greeted {
                    self.greeted = true;
                    self.greeted_at = now;
                }
                if !ok {
                    self.hi();
                }
            }
            TrailsMsg::Other { game } => {
                self.post(TrailsMsg::Bye, None);
                self.end(&format!("your friend opened {game} · both type trails"));
            }
            TrailsMsg::Ping { c } => self.post(TrailsMsg::Pong { c }, Some(now)),
            TrailsMsg::Pong { c } => {
                let rtt = now - c;
                if self.role != Role::Host || !(0.0..=TRAILS_SILENCE_MS).contains(&rtt) {
                    return;
                }
                self.rtts.push(rtt);
                if self.rtts.len() > 5 {
                    self.rtts.remove(0);
                }
            }
            TrailsMsg::Keep { e } => {
                if self.role == Role::Host {
                    self.stale(e, now);
                }
            }
            TrailsMsg::Frames { e, r, f, i, a, h } => {
                if e != self.epoch || self.phase == TrailsPhase::Wait {
                    if self.role == Role::Host {
                        self.stale(e, now);
                    }
                    return;
                }
                let r = i64::from(r);
                if r == self.r {
                    self.acked = self.acked.max(a.min(self.sealed));
                    // first heard is kept: a frame never changes once sent. nothing from far beyond where the round is
                    let limit = self.round.tick as usize + MAX_FRAMES;
                    for (j, fr) in i.iter().enumerate() {
                        let tick = f as usize + j;
                        if tick >= self.round.tick as usize && tick < limit && self.theirs.get(tick).copied().flatten().is_none() {
                            put(&mut self.theirs, tick, Some(*fr));
                        }
                    }
                    while self.theirs.get(self.their_next as usize).copied().flatten().is_some() {
                        self.their_next += 1;
                    }
                    // my round is over and theirs is stuck short of it: the last frames go again
                    if self.round.result.is_some() && a < self.sealed {
                        self.dirty = true;
                    }
                } else if let Some(p) = self.prev.as_ref().filter(|p| p.r == r && a < p.sealed) {
                    let from = (a + 1).max(p.sealed - MAX_FRAMES as i64 + 1).max(0);
                    let frames = slice(&p.mine, from, p.sealed + 1);
                    let e = self.epoch;
                    self.post(TrailsMsg::Frames { e, r: r as u32, f: from as u32, i: frames, a: -1, h: None }, Some(now));
                }
                if let Some(h) = h
                    && self.role == Role::Host
                {
                    self.verify(r, h, now);
                }
            }
            TrailsMsg::Snap(m) => {
                if self.role == Role::Guest && self.greeted && m.e > self.epoch {
                    self.adopt(*m, now);
                }
            }
            TrailsMsg::Again { e } => {
                // only an ask made at the end of this match counts
                if e != self.epoch || self.phase != TrailsPhase::Over {
                    return;
                }
                if self.role == Role::Host && self.again.mine {
                    self.start(now);
                } else {
                    self.again.theirs = true;
                }
            }
            TrailsMsg::Bye => self.end("your friend left"),
        }
    }

    pub fn tick(&mut self, now: f64) -> TrailsView {
        // a hidden window's clock crawls: carry on from where it is rather than race to catch up
        let dt = (TICK_MS * 3.0).min((now - self.last_tick).max(0.0));
        self.last_tick = now;
        if self.ended.is_some() {
            return self.view(now);
        }
        if now - self.heard > TRAILS_SILENCE_MS {
            self.end("lost the connection");
            return self.view(now);
        }
        if !self.greeted {
            if now - self.last_hi >= HI_MS {
                self.last_hi = now;
                self.hi();
            }
            return self.view(now);
        }
        if self.phase == TrailsPhase::Wait && self.role == Role::Host {
            if self.rtts.len() >= PINGS || (!self.rtts.is_empty() && now - self.greeted_at >= PING_WAIT_MS) {
                self.start(now);
            } else if now - self.last_ping >= PING_MS {
                self.ping(now);
            }
        }
        if self.phase == TrailsPhase::Point && now - self.phase_at >= POINT_MS {
            self.next_round(self.phase_at + POINT_MS + COUNT_TOTAL_MS);
        }
        if self.phase == TrailsPhase::Count {
            // the first frames are fixed before the count ends, each as far ahead of its tick as any later one
            let d = i64::from(self.delay);
            let ahead = ((now - self.run_at) / TICK_MS).floor() as i64 + d - 1;
            self.seal_to((d - 1).min(ahead));
            if now >= self.run_at {
                self.phase = TrailsPhase::Run;
                self.acc = TICK_MS.min(now - self.run_at);
            }
        } else if self.phase == TrailsPhase::Run {
            self.acc += dt;
            while self.acc >= TICK_MS && self.phase == TrailsPhase::Run {
                if self.theirs.get(self.round.tick as usize).copied().flatten().is_none() {
                    // held at the brink, so it steps the moment the frame lands
                    self.acc = TICK_MS;
                    self.stall_at.get_or_insert(now);
                    break;
                }
                self.stall_at = None;
                self.acc -= TICK_MS;
                self.step_once(now);
            }
        }
        if self.phase != TrailsPhase::Run {
            self.stall_at = None;
        } else if self.stall_at.is_some() && now - self.last_sent >= RESEND_MS {
            self.dirty = true;
            // where the guest is stuck, in case it's stuck because the two disagree
            if self.role == Role::Guest {
                let t = self.round.tick;
                self.hash_due = Some((t, self.hash_at(t).unwrap_or(0)));
            }
        }
        if self.dirty {
            self.send_frames(now);
        } else if now - self.last_sent >= KEEP_MS {
            if self.role == Role::Host {
                self.ping(now);
            } else {
                let e = self.epoch;
                self.post(TrailsMsg::Keep { e }, Some(now));
            }
        }
        self.view(now)
    }

    /// my frames they haven't acknowledged, what I hold of theirs, and a hash if one is due
    fn send_frames(&mut self, now: f64) {
        self.dirty = false;
        if self.phase == TrailsPhase::Wait {
            return;
        }
        let from = (self.acked + 1).max(self.sealed - MAX_FRAMES as i64 + 1).max(0);
        let h = self.hash_due.take();
        let i = slice(&self.mine, from, self.sealed + 1);
        let m = TrailsMsg::Frames { e: self.epoch, r: self.r.max(0) as u32, f: from as u32, i, a: self.their_next - 1, h };
        self.post(m, Some(now));
    }

    fn view(&self, now: f64) -> TrailsView {
        let mut round = self.round.clone();
        if matches!(self.phase, TrailsPhase::Run | TrailsPhase::Count) {
            for p in 0..2 {
                if let Some(Some(turn)) = self.frame(p, round.tick) {
                    round = steer(&round, p, turn);
                }
            }
        }
        let over = matches!(self.phase, TrailsPhase::Point | TrailsPhase::Over);
        TrailsView {
            round,
            score: self.score,
            phase: self.phase,
            count: if self.phase == TrailsPhase::Count { ((self.run_at - now) / COUNT_MS).ceil().clamp(1.0, 3.0) as u32 } else { 0 },
            last: self.last,
            winner: match_winner(self.score),
            t: if self.phase == TrailsPhase::Run { (self.acc / TICK_MS).min(1.0) } else { 0.0 },
            crash_ms: if over { Some(now - self.phase_at) } else { None },
            stalled: self.stall_at.is_some_and(|at| now - at >= STALL_SHOW_MS),
            again: self.again,
            ended: self.ended.clone(),
        }
    }

    pub fn state(&self) -> TrailsState {
        TrailsState {
            r: self.r,
            tick: self.round.tick,
            hash: self.hash_at(self.round.tick).unwrap_or(0),
            score: self.score,
            epoch: self.epoch,
            delay: self.delay,
            phase: self.phase,
        }
    }
}
