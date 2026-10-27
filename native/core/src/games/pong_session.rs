//! `src/lib/pongSession.ts`: one side of a game, without the page. The court feeds it keys, messages and frame
//! times, and draws what `tick` returns; what it sends the other side waits in `take_sent`.

use super::hello::{LinkedGame, other_game_note};
use super::pong::*;
use super::{Random, Side, js_hypot};

// hi goes out this often until the other side's hi comes in
const HI_MS: f64 = 400.0;
// a paddle goes out when its way changes, at most this often, and at least every KEEP_MS
const PADDLE_MS: f64 = 1000.0 / 30.0;
// frame times land a hair either side of a frame; without this a change could wait a whole extra frame
const PADDLE_SLACK_MS: f64 = 1.0;
const KEEP_MS: f64 = 250.0;
// the other side is still on an older leg than the newest one sent from here, this long after: it went missing
const RESEND_MS: f64 = 1000.0;
// the guest's offset moves toward a better guess at a tenth of real time; further off than this and it jumps
const SLEW: f64 = 0.1;
const SLEW_MAX_MS: f64 = 250.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Again {
    pub mine: bool,
    pub theirs: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionView {
    /// What to draw.
    pub snap: Option<Snapshot>,
    pub started: bool,
    pub again: Again,
    /// Set once the game is over for good: the line the lobby shows.
    pub ended: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct Ease {
    x: f64,
    y: f64,
    at: f64,
}

pub struct PongSession {
    // solo: the whole game, stepped here. online it only holds the serve's way for the next game
    game: PongState,
    carry: f64,
    held_up: bool,
    held_down: bool,
    mine_y: f64,
    // solo: the computer's paddle
    their_y: f64,
    greeted: bool,
    // host: the guest said hi; guest: the first leg came
    started: bool,
    heard: f64,
    last_tick: f64,
    last_hi: f64,
    again: Again,
    ended: Option<String>,
    leg: Option<Leg>,
    leg_mine: bool,
    missed: bool,
    guess: Option<Leg>,
    guessed: bool,
    ease: Ease,
    far: FarPaddle,
    clock: HostClock,
    offset: f64,
    echo: Option<(f64, f64)>,
    sent_paddle: (f64, f64, f64),
    resent_at: f64,
    pub role: Side,
    /// A game against the computer: no link, the computer plays the guest's paddle.
    pub solo: Option<Skill>,
    bot: BotMind,
    sent: Vec<String>,
    rand: Box<dyn FnMut() -> f64 + Send>,
}

impl std::fmt::Debug for PongSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PongSession").field("role", &self.role).field("leg", &self.leg).finish_non_exhaustive()
    }
}

impl PongSession {
    pub fn new(role: Side, now: f64, rand: Box<dyn FnMut() -> f64 + Send>, solo: Option<Skill>) -> PongSession {
        let mut rand = rand;
        let game = new_game(if rand() < 0.5 { 1.0 } else { -1.0 });
        let mut s = PongSession {
            game,
            carry: 0.0,
            held_up: false,
            held_down: false,
            mine_y: COURT_H / 2.0,
            their_y: COURT_H / 2.0,
            greeted: false,
            started: false,
            heard: now,
            last_tick: now,
            last_hi: now,
            again: Again::default(),
            ended: None,
            leg: None,
            leg_mine: false,
            missed: false,
            guess: None,
            guessed: false,
            ease: Ease { x: 0.0, y: 0.0, at: 0.0 },
            far: FarPaddle::default(),
            clock: HostClock::default(),
            offset: 0.0,
            echo: None,
            sent_paddle: (COURT_H / 2.0, 0.0, f64::NEG_INFINITY),
            resent_at: f64::NEG_INFINITY,
            role: if solo.is_some() { Side::Host } else { role },
            solo,
            bot: BotMind::default(),
            sent: vec![],
            rand,
        };
        if solo.is_some() {
            s.greeted = true;
            s.started = true;
        } else {
            s.hi(now);
        }
        s
    }

    /// A session with `Math.random`.
    pub fn with_defaults(role: Side, now: f64, solo: Option<Skill>) -> PongSession {
        let mut r = Random::from_entropy();
        PongSession::new(role, now, Box::new(move || r.next_f64()), solo)
    }

    /// What's waiting to go to the other side, oldest first.
    pub fn take_sent(&mut self) -> Vec<String> {
        std::mem::take(&mut self.sent)
    }

    fn hi(&mut self, now: f64) {
        self.post(PongMsg::Hi { v: PONG_VERSION, g: true, ok: self.greeted, stamp: Stamp::default() }, now);
    }

    fn post(&mut self, mut m: PongMsg, now: f64) {
        let stamp = Stamp { c: Some(now), e: self.echo.map(|e| e.0), h: self.echo.map(|e| now - e.1) };
        match &mut m {
            PongMsg::Hi { stamp: s, .. } | PongMsg::P { stamp: s, .. } | PongMsg::L { stamp: s, .. } | PongMsg::Again { stamp: s } => *s = stamp,
            PongMsg::Bye => {}
        }
        self.sent.push(encode_pong(&m));
    }

    fn end(&mut self, note: String) {
        if self.ended.is_none() {
            self.ended = Some(note);
        }
    }

    // the shared clock: the host's own, the guest's guess at it
    fn shared(&self, now: f64) -> f64 {
        if self.role == Side::Host { now } else { now + self.offset }
    }

    // guest: moves the offset in use toward the clock's best guess, by at most SLEW of ms
    fn follow(&mut self, ms: f64) {
        let gap = self.clock.offset() - self.offset;
        self.offset += if self.leg.is_none() || gap.abs() > SLEW_MAX_MS { gap } else { jmin(ms * SLEW, jmax(-ms * SLEW, gap)) };
    }

    fn over(&self) -> bool {
        if self.solo.is_some() { self.game.phase == Phase::Over } else { self.leg.is_some_and(|l| l.kind == LegKind::Over) }
    }

    fn restart(&mut self, now: f64) {
        let dir = if (self.rand)() < 0.5 { 1.0 } else { -1.0 };
        self.game = new_game(dir);
        self.carry = 0.0;
        self.bot = BotMind::default();
        self.again = Again::default();
        if self.solo.is_none() {
            let game = self.leg.map_or(0, |l| l.game) + 1;
            let leg = self.serve_leg(game, 0, self.shared(now), self.game.serve_dir, [0, 0]);
            self.produce(leg, now);
        }
    }

    fn serve_leg(&mut self, game: u64, n: u64, from: f64, dir: f64, score: [u32; 2]) -> Leg {
        Leg { game, n, at: from + SERVE_PAUSE_S * 1000.0, kind: LegKind::Serve, ball: serve(dir, &mut *self.rand), score }
    }

    /// A paddle key went down or up.
    pub fn key(&mut self, up: bool, down: bool) {
        if up {
            self.held_up = down;
        } else {
            self.held_down = down;
        }
    }

    pub fn release(&mut self) {
        self.held_up = false;
        self.held_down = false;
    }

    /// Enter: asks for a rematch once the game is over; the host restarts when both have asked.
    pub fn rematch(&mut self) {
        if self.ended.is_some() || !self.over() || self.again.mine {
            return;
        }
        // the computer always wants another
        if self.solo.is_some() {
            self.restart(self.last_tick);
            return;
        }
        self.again.mine = true;
        self.post(PongMsg::Again { stamp: Stamp::default() }, self.last_tick);
        if self.role == Side::Host && self.again.theirs {
            self.restart(self.last_tick);
        }
    }

    pub fn receive(&mut self, text: &str, now: f64) {
        if self.ended.is_some() {
            return;
        }
        let Some(m) = decode_pong(text) else { return };
        self.heard = now;
        let stamp = match &m {
            PongIn::Msg(PongMsg::Hi { stamp, .. } | PongMsg::P { stamp, .. } | PongMsg::L { stamp, .. } | PongMsg::Again { stamp }) => Some(*stamp),
            _ => None,
        };
        if let Some(stamp) = stamp {
            if self.role == Side::Host {
                if let Some(c) = stamp.c {
                    self.echo = Some((c, now));
                }
            } else {
                self.clock.heard(&stamp, now);
                self.follow(0.0);
            }
        }
        match m {
            PongIn::Msg(PongMsg::Hi { v, ok, .. }) => {
                if v != PONG_VERSION {
                    self.post(PongMsg::Bye, now);
                    self.end(if v < PONG_VERSION {
                        "your friend needs to update Q Calc to play".into()
                    } else {
                        "update Q Calc to play your friend".into()
                    });
                    return;
                }
                self.greeted = true;
                if !ok {
                    self.hi(now);
                }
                if self.role == Side::Host && !self.started {
                    self.started = true;
                    // the serve waits until the guest's court is up
                    let leg = self.serve_leg(1, 0, now, self.game.serve_dir, [0, 0]);
                    self.produce(leg, now);
                }
            }
            PongIn::Msg(PongMsg::P { at, y, d, game, n, .. }) => {
                let t = self.shared(now);
                self.far.report(y, d, at, t, now);
                // still behind on a leg sent from here, well after it should have landed: send it again
                if let Some(leg) = self.leg
                    && self.leg_mine
                    && newer(&leg, Some(&Leg { game, n, ..leg }))
                    && at - leg.at > RESEND_MS
                    && t - self.resent_at > RESEND_MS
                {
                    self.resent_at = t;
                    self.post(PongMsg::L { leg, stamp: Stamp::default() }, now);
                }
            }
            PongIn::Msg(PongMsg::L { leg, .. }) => self.take(leg, now),
            PongIn::Msg(PongMsg::Again { .. }) => {
                // only an ask made at the end counts; one from mid game would start the next game early
                if !self.over() {
                    return;
                }
                self.again.theirs = true;
                if self.role == Side::Host && self.again.mine {
                    self.restart(now);
                }
            }
            PongIn::Other(game) => {
                self.post(PongMsg::Bye, now);
                self.end(other_game_note(&game, LinkedGame::Pong));
            }
            PongIn::Msg(PongMsg::Bye) => self.end("your friend left".into()),
        }
    }

    // a leg judged on this side: kept as the wire will carry it, so both sides fly the same ball
    fn produce(&mut self, leg: Leg, now: f64) {
        let w = wire_leg(&leg);
        self.set_leg(w);
        self.leg_mine = true;
        self.post(PongMsg::L { leg: w, stamp: Stamp::default() }, now);
    }

    fn set_leg(&mut self, leg: Leg) {
        self.leg = Some(leg);
        self.missed = false;
        self.guess = None;
        self.guessed = false;
    }

    // a leg judged on the other side
    fn take(&mut self, m: Leg, now: f64) {
        if !newer(&m, self.leg.as_ref()) {
            return;
        }
        let t = self.shared(now);
        let before = self.shown(t, now);
        let mut leg = m;
        // after a stall, take it up from LATE_MS in rather than from wherever it has got to
        if leg.kind != LegKind::Over && towards(&leg.ball) == self.role && t - leg.at > LATE_MS {
            leg.at = t - LATE_MS;
        }
        let fresh = self.leg.is_none_or(|l| l.game != leg.game);
        self.set_leg(leg);
        self.leg_mine = false;
        self.started = true;
        // a new game clears the rematch asks
        if fresh {
            self.again = Again::default();
        }
        self.ease = Ease { x: 0.0, y: 0.0, at: now };
        let after = self.shown(t, now);
        if fresh || before.1 != Phase::Play || after.1 != Phase::Play {
            return;
        }
        let x = before.0.x - after.0.x;
        let y = before.0.y - after.0.y;
        if js_hypot(x, y) <= SNAP_PX {
            self.ease = Ease { x, y, at: now };
        }
    }

    // the ball as drawn at shared time t, our time now
    fn shown(&self, t: f64, now: f64) -> (Ball, Phase) {
        let Some(leg) = self.leg else { return (centre_ball(), Phase::Serve) };
        if leg.kind == LegKind::Over {
            return (centre_ball(), Phase::Over);
        }
        if leg.kind == LegKind::Serve && t < leg.at {
            return (centre_ball(), Phase::Serve);
        }
        let ball = if let Some(g) = self.guess {
            // a guessed return stops at this side's face: only a real leg is judged here
            let face = if self.role == Side::Host { HOST_FACE_X } else { GUEST_FACE_X };
            let stop = g.at + seconds_to(&g.ball, face) * 1000.0;
            fly(g.ball, (jmin(t, stop) - g.at) / 1000.0)
        } else {
            fly(leg.ball, jmax(0.0, t - leg.at) / 1000.0)
        };
        let k = fade(now - self.ease.at);
        let half = BALL_SIZE / 2.0;
        (Ball { x: ball.x + self.ease.x * k, y: jmin(COURT_H - half, jmax(half, ball.y + self.ease.y * k)), ..ball }, Phase::Play)
    }

    // the ball heading here: hit or miss, against this paddle as it is now
    fn judge(&mut self, t: f64, now: f64) {
        let Some(leg) = self.leg else { return };
        if leg.kind == LegKind::Over || towards(&leg.ball) != self.role || t < leg.at {
            return;
        }
        let host = self.role == Side::Host;
        let face = if host { HOST_FACE_X } else { GUEST_FACE_X };
        let dir = if host { 1.0 } else { -1.0 };
        if !self.missed {
            let cross = leg.at + seconds_to(&leg.ball, face) * 1000.0;
            if t < cross {
                return;
            }
            let b = fly(leg.ball, (cross - leg.at) / 1000.0);
            if overlaps_paddle(b.y, self.mine_y) {
                let ball = bounce_off(Ball { x: face, ..b }, self.mine_y, dir);
                self.produce(Leg { n: leg.n + 1, at: cross, kind: LegKind::Play, ball, ..leg }, now);
                return;
            }
            self.missed = true;
        }
        let out = leg.at + seconds_to(&leg.ball, if host { -BALL_SIZE / 2.0 } else { COURT_W + BALL_SIZE / 2.0 }) * 1000.0;
        if t < out {
            return;
        }
        let score = if host { [leg.score[0], leg.score[1] + 1] } else { [leg.score[0] + 1, leg.score[1]] };
        if winner_of(score).is_some() {
            self.produce(Leg { n: leg.n + 1, at: out, kind: LegKind::Over, ball: centre_ball(), score, ..leg }, now);
        } else {
            // served back toward this side, which lost the point
            let next = self.serve_leg(leg.game, leg.n + 1, out, if host { -1.0 } else { 1.0 }, score);
            self.produce(next, now);
        }
    }

    // the ball heading away: once it reaches the far face, guess the far side's bounce from its paddle as drawn here
    fn guess_far(&mut self, t: f64, now: f64) {
        let Some(leg) = self.leg else { return };
        if leg.kind == LegKind::Over || self.guessed || towards(&leg.ball) == self.role || t < leg.at {
            return;
        }
        let host = self.role == Side::Host;
        let face = if host { GUEST_FACE_X } else { HOST_FACE_X };
        let cross = leg.at + seconds_to(&leg.ball, face) * 1000.0;
        if t < cross {
            return;
        }
        self.guessed = true;
        let b = fly(leg.ball, (cross - leg.at) / 1000.0);
        let far_y = self.far.shown(t, now);
        if overlaps_paddle(b.y, far_y) {
            self.guess = Some(Leg { at: cross, ball: bounce_off(Ball { x: face, ..b }, far_y, if host { -1.0 } else { 1.0 }), ..leg });
        }
    }

    pub fn tick(&mut self, now: f64) -> SessionView {
        let dt = jmin(0.1, jmax(0.0, now - self.last_tick) / 1000.0);
        self.last_tick = now;
        if self.ended.is_some() {
            return self.view(None);
        }
        if self.solo.is_none() && now - self.heard > SILENCE_MS {
            self.end("lost the connection".into());
            return self.view(None);
        }
        if !self.greeted && now - self.last_hi >= HI_MS {
            self.last_hi = now;
            self.hi(now);
        }
        let d = (if self.held_down { 1.0 } else { 0.0 }) - (if self.held_up { 1.0 } else { 0.0 });
        self.mine_y = move_paddle(self.mine_y, d, dt);
        if let Some(skill) = self.solo {
            let (y, mind) = bot_step(&self.game, self.their_y, self.bot, skill, dt, &mut *self.rand);
            self.their_y = y;
            self.bot = mind;
            self.game = PongState { paddles: [self.mine_y, self.their_y], ..self.game };
            let (g, carry) = advance(self.game, self.carry, dt, &mut *self.rand);
            self.game = g;
            self.carry = carry;
            return self.view(Some(snapshot_of(&self.game, now)));
        }
        if self.role == Side::Guest {
            self.follow(dt * 1000.0);
        }
        let t = self.shared(now);
        self.judge(t, now);
        self.guess_far(t, now);
        if self.greeted {
            self.send_paddle(d, t, now);
        }
        let (ball, phase) = self.shown(t, now);
        let theirs = self.far.shown(t, now);
        let score = self.leg.map_or([0, 0], |l| l.score);
        let winner = if phase == Phase::Over { winner_of(score) } else { None };
        let paddles = if self.role == Side::Host { [self.mine_y, theirs] } else { [theirs, self.mine_y] };
        self.view(Some(Snapshot { at: t, ball, paddles, score, phase, winner }))
    }

    // this paddle, when its way changes or it has strayed from where the other side would carry it, and as a keepalive
    fn send_paddle(&mut self, d: f64, t: f64, now: f64) {
        let (sy, sd, sat) = self.sent_paddle;
        let since = now - sat;
        let carried = clamp_paddle(sy + (sd * PADDLE_SPEED * since) / 1000.0);
        let changed = d != sd || (carried - self.mine_y).abs() > 1.0;
        if !((changed && since >= PADDLE_MS - PADDLE_SLACK_MS) || since >= KEEP_MS) {
            return;
        }
        self.sent_paddle = (self.mine_y, d, now);
        let (game, n) = self.leg.map_or((0, 0), |l| (l.game, l.n));
        self.post(PongMsg::P { at: t, y: self.mine_y, d, game, n, stamp: Stamp::default() }, now);
    }

    fn view(&self, snap: Option<Snapshot>) -> SessionView {
        SessionView { snap, started: self.started, again: self.again, ended: self.ended.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::mulberry32;

    // the TS test's seeded() is mulberry32 written another way
    fn seeded(seed: u32) -> impl FnMut() -> f64 + Send {
        mulberry32(seed)
    }

    #[derive(Default, Clone)]
    struct LinkOpts {
        lag: f64,
        jitter: f64,
        stall: Option<(f64, f64)>,
        skew: f64,
        fps: f64,
        guest_version: Option<f64>,
        drop_first: usize,
        drop_leg_to_guest: Option<usize>,
        drop_first_hi_to_host: bool,
        seed: u32,
    }

    struct Pair {
        host: PongSession,
        guest: PongSession,
        now: f64,
        o: LinkOpts,
        net: Box<dyn FnMut() -> f64>,
        dropped: usize,
        legs_to_guest: usize,
        queue: Vec<(f64, Side, String)>,
        sent: Vec<(f64, Side, String)>,
        last: [f64; 2],
        hv: SessionView,
        gv: SessionView,
    }

    // one fixed sequence both sessions draw from, as the TS test's single closure: serves go out at an angle
    fn rand_seq(r: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Box<dyn FnMut() -> f64 + Send> {
        let r = r.clone();
        Box::new(move || [0.9, 0.1, 0.8, 0.2][r.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 4])
    }

    impl Pair {
        fn new(o: LinkOpts) -> Pair {
            let now = 1000.0;
            let r = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut host = PongSession::new(Side::Host, now, rand_seq(&r), None);
            let mut guest = PongSession::new(Side::Guest, now + o.skew, rand_seq(&r), None);
            let hv = host.tick(now);
            let gv = guest.tick(now + o.skew);
            let net = Box::new(seeded(if o.seed == 0 { 1 } else { o.seed }));
            let mut p = Pair { host, guest, now, o, net, dropped: 0, legs_to_guest: 0, queue: vec![], sent: vec![], last: [0.0, 0.0], hv, gv };
            p.route();
            p
        }
        fn push(&mut self, to: Side, text: String) {
            self.sent.push((self.now, to, text.clone()));
            if self.dropped < self.o.drop_first {
                self.dropped += 1;
                return;
            }
            if to == Side::Guest && text.starts_with(r#"{"t":"l""#) {
                let i = self.legs_to_guest;
                self.legs_to_guest += 1;
                if self.o.drop_leg_to_guest == Some(i) {
                    return;
                }
            }
            if to == Side::Host && self.o.drop_first_hi_to_host && text.contains("\"hi\"") {
                self.o.drop_first_hi_to_host = false;
                return;
            }
            let k = if to == Side::Host { 0 } else { 1 };
            let mut at = (self.now + self.o.lag + (self.net)() * self.o.jitter).max(self.last[k]);
            if let Some((from, ms)) = self.o.stall
                && at >= from
                && at < from + ms
            {
                at = from + ms;
            }
            self.last[k] = at;
            self.queue.push((at, to, text));
        }
        fn route(&mut self) {
            for t in self.host.take_sent() {
                self.push(Side::Guest, t);
            }
            for mut t in self.guest.take_sent() {
                if let Some(v) = self.o.guest_version {
                    t = t.replace(&format!("\"v\":{}", PONG_VERSION), &format!("\"v\":{v}"));
                }
                self.push(Side::Host, t);
            }
        }
        fn run(&mut self, ms: f64, mut each: impl FnMut(&mut Pair), cut: bool) {
            let end = self.now + ms;
            let fps = if self.o.fps == 0.0 { 60.0 } else { self.o.fps };
            while self.now < end {
                self.now += 1000.0 / fps;
                let now = self.now;
                if !cut {
                    let (due, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queue).into_iter().partition(|m| m.0 <= now);
                    self.queue = rest;
                    for (_, to, text) in due {
                        if to == Side::Host { self.host.receive(&text, now) } else { self.guest.receive(&text, now + self.o.skew) }
                        self.route();
                    }
                } else {
                    self.queue.clear();
                }
                self.hv = self.host.tick(now);
                self.gv = self.guest.tick(now + self.o.skew);
                self.route();
                if cut {
                    self.queue.clear();
                }
                each(self);
                self.route();
            }
        }
    }

    fn mine(s: &Snapshot, side: Side) -> f64 {
        s.paddles[if side == Side::Host { 0 } else { 1 }]
    }
    fn face_of(side: Side) -> f64 {
        if side == Side::Host { HOST_FACE_X } else { GUEST_FACE_X }
    }
    fn coming(s: &Snapshot, side: Side) -> bool {
        s.phase == Phase::Play && if side == Side::Host { s.ball.vx < 0.0 } else { s.ball.vx > 0.0 }
    }

    // a player who watches only its own screen
    struct Player {
        side: Side,
        lazy: f64,
        spare: f64,
        was: bool,
        aim: f64,
        idle: bool,
    }

    impl Player {
        fn new(side: Side, lazy: f64, spare: f64) -> Player {
            Player { side, lazy, spare, was: false, aim: 0.0, idle: false }
        }
        fn play(&mut self, v: &SessionView, s: &mut PongSession, rand: &mut dyn FnMut() -> f64) {
            let Some(snap) = v.snap else { return };
            let now = coming(&snap, self.side);
            if now && !self.was {
                self.aim = (rand() * 2.0 - 1.0) * PADDLE_H * 0.35;
                self.idle = rand() < self.lazy;
            }
            self.was = now;
            let mut gap = 0.0;
            if now && !self.idle {
                let want = landing_y(&snap.ball, face_of(self.side)) - self.aim;
                let due = seconds_to(&snap.ball, face_of(self.side)) * 1000.0;
                if due <= (want - mine(&snap, self.side)).abs() / PADDLE_SPEED * 1000.0 + self.spare {
                    gap = want - mine(&snap, self.side);
                }
            }
            s.key(true, gap < -4.0);
            s.key(false, gap > 4.0);
        }
    }

    #[derive(Default)]
    struct Referee {
        hits: u32,
        misses: u32,
        wrong: Vec<String>,
        scores: Vec<String>,
        prev: Option<Snapshot>,
    }

    impl Referee {
        fn watch(&mut self, side: Side, snap: Option<Snapshot>) {
            let Some(snap) = snap else { return };
            let reach = PADDLE_H / 2.0 + BALL_SIZE / 2.0;
            if let Some(prev) = self.prev
                && coming(&prev, side)
            {
                let face = face_of(side);
                let hit = snap.phase == Phase::Play && !coming(&snap, side) && snap.score == prev.score;
                let past = coming(&snap, side) && if side == Side::Guest { snap.ball.x > face + 0.01 } else { snap.ball.x < face - 0.01 };
                if hit || (past && if side == Side::Guest { prev.ball.x <= face } else { prev.ball.x >= face }) {
                    let y = fly(prev.ball, seconds_to(&prev.ball, face)).y;
                    let off = (y - mine(&snap, side)).abs();
                    if hit { self.hits += 1 } else { self.misses += 1 }
                    if off < reach - 1.5 && !hit {
                        self.wrong.push(format!("in place ({off:.1} off) but missed"));
                    }
                    if off > reach + 1.5 && hit {
                        self.wrong.push(format!("out of place ({off:.1} off) but hit"));
                    }
                }
            }
            if let Some(was) = self.prev.map(|p| p.score)
                && snap.score != was
            {
                let mut up = [snap.score[0] as i64 - was[0] as i64, snap.score[1] as i64 - was[1] as i64];
                up.sort();
                if up != [0, 1] && snap.score != [0, 0] {
                    self.scores.push(format!("{was:?} → {:?}", snap.score));
                }
            }
            self.prev = Some(snap);
        }
    }

    fn play_match(o: LinkOpts, ms: f64) -> (Pair, Referee, Referee, f64) {
        let seed = if o.seed == 0 { 1 } else { o.seed };
        let mut g = Pair::new(o);
        let mut rand = seeded(seed * 7);
        let mut hp = Player::new(Side::Host, 0.25, 40.0);
        let mut gp = Player::new(Side::Guest, 0.25, 40.0);
        let (mut hr, mut gr) = (Referee::default(), Referee::default());
        let mut over_at = -1.0;
        g.run(
            ms,
            |p| {
                let (hv, gv) = (p.hv.clone(), p.gv.clone());
                hp.play(&hv, &mut p.host, &mut rand);
                gp.play(&gv, &mut p.guest, &mut rand);
                hr.watch(Side::Host, hv.snap);
                gr.watch(Side::Guest, gv.snap);
                if over_at < 0.0 && hv.snap.is_some_and(|s| s.phase == Phase::Over) && gv.snap.is_some_and(|s| s.phase == Phase::Over) {
                    over_at = p.now;
                }
            },
            false,
        );
        (g, hr, gr, over_at)
    }

    #[test]
    fn waits_for_both_courts_then_serves() {
        let mut g = Pair::new(LinkOpts { lag: 5.0, drop_first: 2, ..Default::default() });
        assert!(!g.hv.started);
        g.run(1000.0, |_| {}, false);
        assert!(g.hv.started && g.gv.started);
        g.run(1000.0, |_| {}, false);
        assert_eq!(g.hv.snap.map(|s| s.phase), Some(Phase::Play));
    }

    #[test]
    fn both_sides_fly_the_same_ball() {
        let mut g = Pair::new(LinkOpts { lag: 60.0, skew: 12_345.6, ..Default::default() });
        g.run(1500.0, |_| {}, false);
        let (h, gs) = (g.hv.snap.unwrap(), g.gv.snap.unwrap());
        assert_eq!(gs.phase, Phase::Play);
        assert!((h.ball.x - gs.ball.x).abs() < 2.0);
        g.guest.key(true, true);
        g.host.key(false, true);
        g.run(1000.0, |_| {}, false);
        let (h, gs) = (g.hv.snap.unwrap(), g.gv.snap.unwrap());
        assert_eq!(gs.paddles[1], PADDLE_H / 2.0);
        assert!((h.paddles[1] - PADDLE_H / 2.0).abs() < 0.01);
        assert_eq!(h.paddles[0], COURT_H - PADDLE_H / 2.0);
        assert!((gs.paddles[0] - (COURT_H - PADDLE_H / 2.0)).abs() < 0.01);
    }

    #[test]
    fn what_each_side_sees_counts_and_the_scores_agree() {
        let cases = [
            LinkOpts { lag: 0.0, ..Default::default() },
            LinkOpts { lag: 60.0, skew: -4321.0, ..Default::default() },
            LinkOpts { lag: 150.0, skew: 98_765.0, ..Default::default() },
            LinkOpts { lag: 60.0, jitter: 40.0, skew: 777.0, seed: 2, ..Default::default() },
            LinkOpts { lag: 150.0, jitter: 30.0, stall: Some((9000.0, 300.0)), seed: 3, ..Default::default() },
            LinkOpts { lag: 20.0, jitter: 120.0, seed: 4, ..Default::default() },
        ];
        for (i, o) in cases.into_iter().enumerate() {
            let (g, host, guest, over_at) = play_match(o, 150_000.0);
            assert!(over_at > 0.0, "case {i}: {:?} {:?} {} {} {}", g.hv.snap, g.gv.snap, host.hits, guest.hits, g.sent.len());
            for r in [&host, &guest] {
                assert!(r.wrong.is_empty(), "case {i}: {:?}", r.wrong);
                assert!(r.scores.is_empty(), "case {i}: {:?}", r.scores);
                assert!(r.hits > 5, "case {i}");
            }
            let (h, gs) = (g.hv.snap.unwrap(), g.gv.snap.unwrap());
            assert_eq!(host.misses + guest.misses, h.score[0] + h.score[1], "case {i}");
            assert_eq!(h.score, gs.score);
            assert_eq!(h.score.iter().max().copied(), Some(WIN_SCORE));
            assert_eq!(gs.winner, h.winner);
        }
    }

    #[test]
    fn a_guest_who_gets_there_keeps_the_rally_going() {
        let mut g = Pair::new(LinkOpts { lag: 150.0, skew: 50.0, ..Default::default() });
        let mut rand = seeded(5);
        let mut hp = Player::new(Side::Host, 0.0, 25.0);
        let mut gp = Player::new(Side::Guest, 0.0, 25.0);
        g.run(
            60_000.0,
            |p| {
                let (hv, gv) = (p.hv.clone(), p.gv.clone());
                hp.play(&hv, &mut p.host, &mut rand);
                gp.play(&gv, &mut p.guest, &mut rand);
            },
            false,
        );
        assert_eq!(g.hv.snap.unwrap().score, [0, 0]);
        assert_eq!(g.gv.snap.unwrap().score, [0, 0]);
    }

    #[test]
    fn sends_fewer_messages_and_resends_a_lost_leg() {
        let (g, _, _, over_at) = play_match(LinkOpts { lag: 60.0, ..Default::default() }, 150_000.0);
        let n = g.sent.iter().filter(|m| m.0 > 3000.0 && m.0 < over_at).count() as f64;
        assert!(n / ((over_at - 3000.0) / 1000.0) < 30.0);
        let (g, _, _, _) = play_match(LinkOpts { lag: 40.0, drop_leg_to_guest: Some(1), ..Default::default() }, 150_000.0);
        assert!(g.legs_to_guest > 2);
        assert_eq!(g.hv.snap.unwrap().phase, Phase::Over);
        assert_eq!(g.gv.snap.unwrap().score, g.hv.snap.unwrap().score);
    }

    #[test]
    fn plays_to_the_win_and_rematches() {
        let mut g = Pair::new(LinkOpts { lag: 80.0, jitter: 20.0, ..Default::default() });
        g.host.key(true, true);
        g.guest.key(false, true);
        g.run(60_000.0, |_| {}, false);
        let (h, gs) = (g.hv.snap.unwrap(), g.gv.snap.unwrap());
        assert_eq!((h.phase, gs.phase), (Phase::Over, Phase::Over));
        assert_eq!(h.score.iter().max().copied(), Some(WIN_SCORE));
        assert_eq!((gs.score, gs.winner), (h.score, h.winner));
        g.guest.rematch();
        g.route();
        g.run(300.0, |_| {}, false);
        assert_eq!(g.hv.again, Again { mine: false, theirs: true });
        assert_eq!(g.hv.snap.unwrap().phase, Phase::Over);
        g.host.rematch();
        g.route();
        g.run(400.0, |_| {}, false);
        assert_eq!((g.hv.snap.unwrap().score, g.gv.snap.unwrap().score), ([0, 0], [0, 0]));
        assert_eq!((g.gv.again, g.hv.again), (Again::default(), Again::default()));
        g.run(60_000.0, |_| {}, false);
        assert_eq!(g.hv.snap.unwrap().phase, Phase::Over);
        assert_eq!(g.gv.snap.unwrap().score, g.hv.snap.unwrap().score);
    }

    #[test]
    fn rematch_asks_only_count_at_the_end() {
        let mut g = Pair::new(LinkOpts { lag: 5.0, ..Default::default() });
        g.run(1500.0, |_| {}, false);
        g.guest.rematch();
        g.route();
        g.run(200.0, |_| {}, false);
        assert!(!g.hv.again.theirs);
        let mut g = Pair::new(LinkOpts { lag: 5.0, ..Default::default() });
        g.run(1500.0, |_| {}, false);
        g.host.receive(&encode_pong(&PongMsg::Again { stamp: Stamp::default() }), 2500.0);
        g.host.key(true, true);
        g.guest.key(false, true);
        g.run(60_000.0, |_| {}, false);
        assert_eq!(g.hv.snap.unwrap().phase, Phase::Over);
        assert!(!g.hv.again.theirs);
        g.host.rematch();
        g.route();
        g.run(200.0, |_| {}, false);
        assert_eq!(g.hv.snap.unwrap().phase, Phase::Over);
    }

    #[test]
    fn the_link_ending() {
        let mut g = Pair::new(LinkOpts { lag: 5.0, ..Default::default() });
        g.run(1500.0, |_| {}, false);
        g.run(SILENCE_MS + 500.0, |_| {}, true);
        assert_eq!(g.hv.ended.as_deref(), Some("lost the connection"));
        assert_eq!(g.gv.ended.as_deref(), Some("lost the connection"));
        let mut g = Pair::new(LinkOpts { lag: 5.0, ..Default::default() });
        g.run(1500.0, |_| {}, false);
        g.host.receive(&encode_pong(&PongMsg::Bye), 3000.0);
        g.run(100.0, |_| {}, false);
        assert_eq!(g.hv.ended.as_deref(), Some("your friend left"));
        let mut old = Pair::new(LinkOpts { lag: 5.0, guest_version: Some(PONG_VERSION - 1.0), ..Default::default() });
        old.run(500.0, |_| {}, false);
        assert_eq!(old.hv.ended.as_deref(), Some("your friend needs to update Q Calc to play"));
        assert_eq!(old.gv.ended.as_deref(), Some("your friend left"));
        let mut later = Pair::new(LinkOpts { lag: 5.0, guest_version: Some(PONG_VERSION + 1.0), ..Default::default() });
        later.run(500.0, |_| {}, false);
        assert_eq!(later.hv.ended.as_deref(), Some("update Q Calc to play your friend"));
        let mut s = PongSession::new(Side::Guest, 0.0, Box::new(|| 0.5), None);
        s.receive(r#"{"t":"s","at":1,"b":[1,2,3,4],"p":[0,0],"s":[0,0],"ph":1}"#, 10.0);
        s.receive(r#"{"t":"p","y":40}"#, 10.0);
        assert!(!s.tick(20.0).started);
        s.receive(r#"{"t":"hi","v":1,"g":"pong"}"#, 30.0);
        assert_eq!(s.tick(40.0).ended.as_deref(), Some("your friend needs to update Q Calc to play"));
        assert_eq!(decode_pong(s.take_sent().last().unwrap()), Some(PongIn::Msg(PongMsg::Bye)));
    }

    #[test]
    fn hellos() {
        let mut g = Pair::new(LinkOpts { lag: 5.0, drop_first_hi_to_host: true, ..Default::default() });
        g.run(2000.0, |_| {}, false);
        assert!(g.hv.started);
        assert_eq!(g.hv.snap.unwrap().phase, Phase::Play);
        let mut g = Pair::new(LinkOpts { lag: 5.0, drop_first: 1, ..Default::default() });
        g.run(3000.0, |_| {}, false);
        assert!(!g.sent.iter().any(|m| m.0 > 2500.0 && m.2.contains("\"hi\"")));
    }

    #[test]
    fn the_message_rate_and_the_paddle_message() {
        for fps in [60.0, 120.0, 144.0] {
            let mut g = Pair::new(LinkOpts { lag: 5.0, fps, ..Default::default() });
            g.guest.key(false, true);
            g.run(3000.0, |_| {}, false);
            let n = g.sent.iter().filter(|m| m.0 > 2000.0 && m.0 <= 3000.0).count();
            assert!((8..=12).contains(&n), "{fps} fps: {n}");
        }
        let mut s = PongSession::new(Side::Guest, 0.0, Box::new(|| 0.5), None);
        s.receive(&encode_pong(&PongMsg::Hi { v: PONG_VERSION, g: false, ok: true, stamp: Stamp::default() }), 0.0);
        s.key(false, true);
        let mut t = 16.0;
        while t < 2000.0 {
            s.tick(t);
            t += 16.0;
        }
        let Some(PongIn::Msg(PongMsg::P { y, d, .. })) = decode_pong(s.take_sent().last().unwrap()) else { panic!() };
        assert_eq!((y, d), (COURT_H - PADDLE_H / 2.0, 1.0));
    }

    #[test]
    fn solo() {
        let mut s = PongSession::new(Side::Host, 0.0, Box::new(seeded(3)), Some(Skill::Easy));
        let mut now = 0.0;
        let mut v = s.tick(now);
        assert!(v.started);
        let mut i = 0;
        while i < 60 * 60 * 10 && v.snap.map(|s| s.phase) != Some(Phase::Over) {
            now += 1000.0 / 60.0;
            v = s.tick(now);
            i += 1;
        }
        assert_eq!(v.ended, None);
        assert_eq!(v.snap.unwrap().phase, Phase::Over);
        s.rematch();
        now += 1000.0 / 60.0;
        v = s.tick(now);
        assert_eq!((v.snap.unwrap().phase, v.snap.unwrap().score), (Phase::Serve, [0, 0]));
        assert!(s.take_sent().is_empty());
    }

    // the TS test's digest of every frame of seeded games, taken before online play changed: the port must
    // play exactly the same
    #[test]
    fn solo_frame_for_frame() {
        for (skill, digest) in [(Skill::Easy, 827_484_971i32), (Skill::Normal, 1_301_697_141), (Skill::Hard, -1_500_791_610)] {
            let mut game = PongSession::new(Side::Host, 0.0, Box::new(seeded(11)), Some(skill));
            let mut now = 0.0;
            let mut h: i32 = 0;
            for i in 0..60 * 90 {
                if i % 37 == 0 {
                    game.key(true, i % 74 == 0);
                }
                if i % 53 == 0 {
                    game.key(false, i % 106 != 0);
                }
                now += 1000.0 / 60.0;
                let s = game.tick(now).snap.unwrap();
                for n in [s.ball.x, s.ball.y, s.paddles[0], s.paddles[1], f64::from(s.score[0]), f64::from(s.score[1]), s.phase.name().len() as f64] {
                    h = h.wrapping_mul(31).wrapping_add(crate::games::js_round(n * 1000.0) as i64 as i32);
                }
                if s.phase == Phase::Over {
                    game.rematch();
                }
            }
            assert_eq!(h, digest, "{skill:?}");
        }
    }
}
