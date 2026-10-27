// trailsSession.test.ts: two sessions joined by a link that delivers each message after its own delay (so they
// can overtake each other), can lose them, and can hold one back to deliver late. stepped at 120 fps, the two
// sides taking turns

use std::sync::{Arc, Mutex};

use super::net::{MAX_FRAMES, TRAILS_VERSION, TrailsMsg, decode_trails, encode_trails};
use super::session::{Again, Role, STALL_SHOW_MS, TRAILS_SILENCE_MS, TrailsPhase, TrailsSession, TrailsView};
use super::{Dir, TICK_MS, WIN_ROUNDS, seeded};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum To {
    Host,
    Guest,
}

type Fault = Box<dyn FnMut(To, &TrailsMsg, f64) -> Option<f64>>;

struct Wire {
    at: f64,
    to: To,
    text: String,
}

struct Pair {
    host: TrailsSession,
    guest: TrailsSession,
    host_out: Arc<Mutex<Vec<String>>>,
    guest_out: Arc<Mutex<Vec<String>>>,
    lag: f64,
    jitter: f64,
    rand: Box<dyn FnMut() -> f64>,
    // None: deliver; Some(f64::INFINITY): lose; Some(ms): that many ms late
    fault: Option<Fault>,
    guest_version: Option<u32>,
    now: f64,
    queue: Vec<Wire>,
    sent: Vec<(To, String)>,
    hv: TrailsView,
    gv: TrailsView,
    stalls: (u32, u32),
    cut: bool,
    script: Vec<(f64, To, Dir)>,
}

fn outbox() -> (Arc<Mutex<Vec<String>>>, Box<dyn FnMut(String) + Send>) {
    let out = Arc::new(Mutex::new(vec![]));
    let o = out.clone();
    (out, Box::new(move |t| o.lock().unwrap().push(t)))
}

impl Pair {
    fn new(lag: f64, jitter: f64, seed: u32, fault: Option<Fault>, guest_version: Option<u32>) -> Self {
        let now = 1000.0;
        let (host_out, hs) = outbox();
        let (guest_out, gs) = outbox();
        let mut host = TrailsSession::new(Role::Host, hs, now);
        let mut guest = TrailsSession::new(Role::Guest, gs, now);
        let hv = host.tick(now);
        let gv = guest.tick(now);
        let mut p = Self {
            host,
            guest,
            host_out,
            guest_out,
            lag,
            jitter,
            rand: Box::new(seeded(seed)),
            fault,
            guest_version,
            now,
            queue: vec![],
            sent: vec![],
            hv,
            gv,
            stalls: (0, 0),
            cut: false,
            script: vec![],
        };
        p.route();
        p
    }

    fn simple() -> Self {
        Self::new(40.0, 0.0, 1, None, None)
    }

    // everything the two have sent since the last look goes on the wire
    fn route(&mut self) {
        let from_host: Vec<String> = std::mem::take(&mut *self.host_out.lock().unwrap());
        let from_guest: Vec<String> = std::mem::take(&mut *self.guest_out.lock().unwrap());
        for (to, text) in from_host.into_iter().map(|t| (To::Guest, t)).chain(from_guest.into_iter().map(|t| (To::Host, t))) {
            let text = match (to, self.guest_version) {
                (To::Host, Some(v)) => text.replace(&format!("\"v\":{TRAILS_VERSION}"), &format!("\"v\":{v}")),
                _ => text,
            };
            let m = decode_trails(&text).unwrap();
            self.sent.push((to, text.clone()));
            let now = self.now;
            let late = match self.fault.as_mut() {
                Some(f) => f(to, &m, now),
                None => None,
            };
            if late == Some(f64::INFINITY) {
                continue;
            }
            let at = now + self.lag + (self.rand)() * self.jitter + late.unwrap_or(0.0);
            self.queue.push(Wire { at, to, text });
        }
    }

    fn deliver(&mut self, to: To) {
        let mut due: Vec<usize> = (0..self.queue.len()).filter(|&i| self.queue[i].to == to && self.queue[i].at <= self.now).collect();
        due.sort_by(|&a, &b| self.queue[a].at.total_cmp(&self.queue[b].at));
        let texts: Vec<String> = due.iter().map(|&i| self.queue[i].text.clone()).collect();
        let mut keep = vec![];
        for (i, w) in std::mem::take(&mut self.queue).into_iter().enumerate() {
            if !due.contains(&i) {
                keep.push(w);
            }
        }
        self.queue = keep;
        for t in texts {
            if !self.cut {
                let now = self.now;
                match to {
                    To::Host => self.host.receive(&t, now),
                    To::Guest => self.guest.receive(&t, now),
                }
                self.route();
            }
        }
    }

    fn run(&mut self, ms: f64, until: &mut dyn FnMut(&Pair) -> bool) {
        let end = self.now + ms;
        while self.now < end && !until(self) {
            self.now += 1000.0 / 120.0;
            let now = self.now;
            let (due, rest): (Vec<_>, Vec<_>) = self.script.drain(..).partition(|s| s.0 <= now);
            self.script = rest;
            for (_, who, dir) in due {
                match who {
                    To::Host => self.host.turn(dir),
                    To::Guest => self.guest.turn(dir),
                }
            }
            let side = if ((now - 1000.0) / (1000.0 / 120.0)).round() as i64 % 2 != 0 { To::Host } else { To::Guest };
            self.deliver(side);
            match side {
                To::Host => self.hv = self.host.tick(now),
                To::Guest => self.gv = self.guest.tick(now),
            }
            self.route();
            if self.hv.stalled {
                self.stalls.0 += 1;
            }
            if self.gv.stalled {
                self.stalls.1 += 1;
            }
        }
    }

    fn run_for(&mut self, ms: f64) {
        self.run(ms, &mut |_| false);
    }

    fn to_guest(&mut self, m: &TrailsMsg) {
        let now = self.now;
        self.guest.receive(&encode_trails(m), now);
        self.route();
    }
}

fn running(p: &Pair) -> bool {
    p.hv.phase == TrailsPhase::Run && p.gv.phase == TrailsPhase::Run
}

fn over(p: &Pair) -> bool {
    p.hv.phase == TrailsPhase::Over && p.gv.phase == TrailsPhase::Over
}

// both sides agree on everything that matters
fn expect_same(g: &mut Pair) {
    g.run(400.0, &mut |p| p.host.state().r == p.guest.state().r);
    let h = g.host.state();
    let s = g.guest.state();
    assert_eq!(s.r, h.r);
    assert_eq!(s.score, h.score);
    for p in 0..2 {
        let a = &g.hv.round.cycles[p].path;
        let b = &g.gv.round.cycles[p].path;
        let n = a.len().min(b.len());
        assert_eq!(b[..n], a[..n]);
        assert!(a.len().abs_diff(b.len()) <= 8);
    }
}

// a player who turns at random now and then, so rounds end in different ways
fn wander(g: &mut Pair, seed: u32, for_ms: f64) {
    let mut rand = seeded(seed);
    let mut at = g.now;
    while at < g.now + for_ms {
        let who = if rand() < 0.5 { To::Host } else { To::Guest };
        let dir = (rand() * 4.0).floor() as Dir;
        g.script.push((at, who, dir));
        at += 90.0 + rand() * 500.0;
    }
}

const KEEPALIVE: [&str; 3] = ["\"trp\"", "\"trq\"", "\"trk\""];

#[test]
fn waits_measures_then_counts_in_together() {
    let mut g = Pair::new(60.0, 0.0, 1, None, None);
    assert_eq!(g.hv.phase, TrailsPhase::Wait);
    g.run(3000.0, &mut |p| p.gv.phase == TrailsPhase::Count);
    assert_eq!(g.gv.phase, TrailsPhase::Count);
    assert_eq!(g.hv.phase, TrailsPhase::Count);
    assert_eq!(g.host.state().delay, 2);
    assert_eq!(g.guest.state().delay, 2);
    g.run(3000.0, &mut running);
    assert_eq!(g.hv.phase, TrailsPhase::Run);
}

#[test]
fn picks_the_input_delay_from_the_ping() {
    for (lag, delay) in [(20.0, 1), (60.0, 2), (120.0, 3)] {
        let mut g = Pair::new(lag, 0.0, 1, None, None);
        g.run(4000.0, &mut running);
        assert_eq!(g.host.state().delay, delay);
    }
}

#[test]
fn resends_hello_until_heard() {
    let mut lost = 0;
    let fault: Fault = Box::new(move |_, m, _| {
        if matches!(m, TrailsMsg::Hi { .. }) && lost < 2 {
            lost += 1;
            return Some(f64::INFINITY);
        }
        None
    });
    let mut g = Pair::new(40.0, 0.0, 1, Some(fault), None);
    g.run(4000.0, &mut running);
    assert_eq!(g.gv.phase, TrailsPhase::Run);
}

#[test]
fn refuses_a_different_version() {
    let mut g = Pair::new(40.0, 0.0, 1, None, Some(TRAILS_VERSION + 1));
    g.run_for(500.0);
    assert_eq!(g.hv.ended.as_deref(), Some("update both Q Calcs to play each other"));
    assert!(g.sent.iter().any(|(to, t)| *to == To::Guest && t == "{\"t\":\"bye\"}"));
}

#[test]
fn tells_when_the_other_side_opened_another_game() {
    for (text, game) in [(r#"{"t":"hi","v":1}"#, "pong"), (r#"{"t":"c4","v":1}"#, "connect 4"), (r#"{"t":"hi","v":1,"u":3}"#, "ember")] {
        let (out, send) = outbox();
        let mut s = TrailsSession::new(Role::Host, send, 0.0);
        s.receive(text, 10.0);
        assert_eq!(s.tick(20.0).ended, Some(format!("your friend opened {game} · both type trails")));
        assert_eq!(out.lock().unwrap().last().unwrap(), "{\"t\":\"bye\"}");
    }
}

#[test]
fn every_message_is_a_tr_type() {
    let mut g = Pair::simple();
    g.run_for(6000.0);
    assert!(g.sent.len() > 50);
    for (_, t) in &g.sent {
        assert!(t.starts_with("{\"t\":\"tr"), "{t}");
    }
}

#[test]
fn stays_in_step_across_latency_and_jitter() {
    for (lag, jitter) in [(20.0, 10.0), (60.0, 40.0), (120.0, 60.0)] {
        let mut g = Pair::new(lag, jitter, lag as u32, None, None);
        g.run(4000.0, &mut running);
        wander(&mut g, lag as u32, 20_000.0);
        for _ in 0..40 {
            g.run_for(500.0);
            expect_same(&mut g);
        }
        assert_eq!(g.host.state().epoch, 1);
        assert!(g.host.state().r > 0);
    }
}

#[test]
fn runs_smoothly_on_a_steady_link() {
    let mut g = Pair::new(80.0, 15.0, 1, None, None);
    g.run(4000.0, &mut running);
    g.run_for(3000.0);
    assert_eq!(g.stalls, (0, 0));
    assert!(f64::from(g.host.state().tick) > (3000.0 / TICK_MS) * 0.9);
}

#[test]
fn a_turn_lands_on_the_same_tick_on_both_sides() {
    let mut g = Pair::new(60.0, 0.0, 1, None, None);
    g.run(4000.0, &mut running);
    g.run_for(400.0);
    let pressed_at = g.host.state().tick as usize;
    g.host.turn(0);
    g.guest.turn(2);
    g.run_for(1000.0);
    for p in 0..2 {
        let a: Vec<i32> = g.hv.round.cycles[p].path.iter().take(200).copied().collect();
        let b: Vec<i32> = g.gv.round.cycles[p].path.iter().take(200).copied().collect();
        assert_eq!(a, b);
    }
    let path = &g.hv.round.cycles[0].path;
    let corner = (0..path.len()).position(|i| i > 0 && path.get(i + 1) == Some(&(path[i] - 64))).unwrap();
    assert_eq!(corner, pressed_at + g.host.state().delay as usize);
    assert_eq!(g.gv.round.cycles[0].path[corner + 1], path[corner + 1]);
}

#[test]
fn turns_pressed_during_the_count_are_taken_as_the_round_starts() {
    let mut g = Pair::simple();
    g.run(4000.0, &mut |p| p.gv.phase == TrailsPhase::Count && p.hv.phase == TrailsPhase::Count);
    g.host.turn(2);
    g.host.turn(3);
    g.host.turn(0);
    g.guest.turn(0);
    g.run(4000.0, &mut |p| p.host.state().tick >= 6 && p.guest.state().tick >= 6);
    for v in [&g.hv, &g.gv] {
        assert_eq!(v.round.cycles[0].path[..3], [12 * 64 + 8, 13 * 64 + 8, 13 * 64 + 7]);
        assert_eq!(v.round.cycles[1].path[..2], [13 * 64 + 55, 12 * 64 + 55]);
    }
}

#[test]
fn a_lost_frame_heals_with_the_next_message() {
    let lost = Arc::new(Mutex::new(0));
    let l = lost.clone();
    let fault: Fault = Box::new(move |_, m, _| {
        if matches!(m, TrailsMsg::Frames { .. }) {
            let mut n = l.lock().unwrap();
            *n += 1;
            if (*n - 1) % 5 == 0 {
                return Some(f64::INFINITY);
            }
        }
        None
    });
    let mut g = Pair::new(40.0, 0.0, 1, Some(fault), None);
    g.run(4000.0, &mut running);
    wander(&mut g, 9, 8000.0);
    g.run_for(8000.0);
    assert!(*lost.lock().unwrap() > 40);
    expect_same(&mut g);
    assert_eq!(g.host.state().epoch, 1);
}

#[test]
fn never_sticks_whatever_a_bad_link_loses() {
    for seed in 1..=4u32 {
        let bad = Arc::new(Mutex::new(true));
        let b = bad.clone();
        let mut loss = seeded(seed * 77);
        let fault: Fault = Box::new(move |_, m, _| {
            let text = encode_trails(m);
            if *b.lock().unwrap() && loss() < 0.15 && !KEEPALIVE.iter().any(|k| text.contains(k)) {
                return Some(f64::INFINITY);
            }
            None
        });
        let mut g = Pair::new(20.0 + f64::from(seed % 12) * 12.0, 60.0, seed, Some(fault), None);
        wander(&mut g, seed, 45_000.0);
        g.run_for(45_000.0);
        *bad.lock().unwrap() = false;
        g.run_for(3000.0);
        assert_eq!(g.hv.ended, None, "seed {seed}");
        expect_same(&mut g);
        if g.hv.phase == TrailsPhase::Over {
            continue;
        }
        let was = (g.host.state().r, g.host.state().tick);
        g.run_for(2500.0);
        assert_ne!((g.host.state().r, g.host.state().tick), was, "seed {seed}");
        expect_same(&mut g);
    }
}

#[test]
fn a_stall_shows_and_ends_when_the_late_frame_arrives() {
    let mut from: Option<f64> = None;
    let fault: Fault = Box::new(move |to, m, now| {
        let TrailsMsg::Frames { f, i, .. } = m else { return None };
        if to != To::Host || (*f as usize + i.len()) < 20 {
            return None;
        }
        let start = *from.get_or_insert(now);
        if now - start < 900.0 { Some(start + 900.0 - now) } else { None }
    });
    let mut g = Pair::new(30.0, 0.0, 1, Some(fault), None);
    g.run(4000.0, &mut running);
    g.run(8000.0, &mut |p| p.hv.stalled);
    let stuck_at = g.host.state().tick;
    assert!(g.hv.stalled);
    assert_eq!(g.hv.phase, TrailsPhase::Run);
    g.run_for(300.0);
    assert_eq!(g.host.state().tick, stuck_at);
    g.run(3000.0, &mut |p| !p.hv.stalled);
    g.run_for(500.0);
    assert!(!g.hv.stalled);
    assert!(g.host.state().tick > stuck_at + 3);
    expect_same(&mut g);
    assert_eq!(g.host.state().epoch, 1);
    const { assert!(STALL_SHOW_MS > TICK_MS * 2.0) };
}

#[test]
fn a_stale_or_far_future_frame_is_not_taken() {
    let mut g = Pair::simple();
    g.run(4000.0, &mut running);
    g.run_for(500.0);
    let s = g.guest.state();
    let (r, tick) = (s.r as u32, s.tick);
    g.to_guest(&TrailsMsg::Frames { e: 1, r: r + 1, f: tick + 4, i: vec![Some(0)], a: -1, h: None });
    g.to_guest(&TrailsMsg::Frames { e: 7, r, f: tick + 4, i: vec![Some(0)], a: -1, h: None });
    g.to_guest(&TrailsMsg::Frames { e: 1, r, f: tick + MAX_FRAMES as u32 + 5, i: vec![Some(0)], a: -1, h: None });
    g.run_for(1500.0);
    expect_same(&mut g);
    assert_eq!(g.host.state().epoch, 1);
}

#[test]
fn puts_a_desync_right_from_the_host() {
    let mut g = Pair::new(50.0, 20.0, 1, None, None);
    g.run(4000.0, &mut running);
    g.run_for(300.0);
    let s = g.guest.state();
    g.to_guest(&TrailsMsg::Frames { e: 1, r: s.r as u32, f: s.tick + 6, i: vec![Some(0)], a: -1, h: None });
    g.run_for(700.0);
    assert_ne!(g.gv.round.cycles[0].dir, g.hv.round.cycles[0].dir);
    g.run(3000.0, &mut |p| p.host.state().epoch > 1);
    assert_eq!(g.host.state().epoch, 2);
    g.run_for(1000.0);
    assert_eq!(g.guest.state().epoch, 2);
    assert_eq!(g.gv.round.cycles[0].dir, 1);
    expect_same(&mut g);
    wander(&mut g, 11, 60_000.0);
    g.run(60_000.0, &mut over);
    assert_eq!(g.guest.state().score, g.host.state().score);
    assert_eq!(g.host.state().epoch, 2);
}

#[test]
fn a_snapshot_that_never_arrives_is_sent_again() {
    let lost = Arc::new(Mutex::new(0));
    let l = lost.clone();
    let fault: Fault = Box::new(move |_, m, _| {
        if matches!(m, TrailsMsg::Snap(_)) {
            let mut n = l.lock().unwrap();
            *n += 1;
            if *n <= 1 {
                return Some(f64::INFINITY);
            }
        }
        None
    });
    let mut g = Pair::new(40.0, 0.0, 1, Some(fault), None);
    g.run(6000.0, &mut running);
    assert!(*lost.lock().unwrap() > 1);
    assert_eq!(g.gv.phase, TrailsPhase::Run);
    expect_same(&mut g);
}

#[test]
fn plays_a_whole_match_then_a_rematch() {
    let mut g = Pair::new(70.0, 50.0, 4, None, None);
    g.run(4000.0, &mut running);
    wander(&mut g, 21, 120_000.0);
    g.run(120_000.0, &mut over);
    assert!(over(&g));
    assert_eq!(g.gv.score, g.hv.score);
    assert_eq!(*g.hv.score.iter().max().unwrap(), WIN_ROUNDS);
    assert_eq!(g.gv.winner, g.hv.winner);
    assert_eq!(g.host.state().epoch, 1);
    g.script.clear();
    let now = g.now;
    g.guest.rematch(now);
    g.guest.turn(0);
    g.route();
    g.run_for(500.0);
    assert_eq!(g.hv.again, Again { mine: false, theirs: true });
    assert_eq!(g.gv.again, Again { mine: true, theirs: false });
    assert_eq!(g.hv.phase, TrailsPhase::Over);
    let now = g.now;
    g.host.rematch(now);
    g.route();
    g.run_for(500.0);
    for v in [&g.hv, &g.gv] {
        assert_eq!(v.phase, TrailsPhase::Count);
        assert_eq!(v.score, [0, 0]);
        assert_eq!(v.again, Again::default());
    }
    g.run(4000.0, &mut running);
    g.run_for(1000.0);
    expect_same(&mut g);
}

#[test]
fn keeps_the_link_warm_and_ends_after_silence() {
    let mut g = Pair::simple();
    g.run(4000.0, &mut running);
    wander(&mut g, 8, 120_000.0);
    g.run(120_000.0, &mut over);
    g.run_for(TRAILS_SILENCE_MS * 3.0);
    assert_eq!(g.hv.ended, None);
    assert_eq!(g.gv.ended, None);
    g.cut = true;
    g.run_for(TRAILS_SILENCE_MS + 200.0);
    assert_eq!(g.hv.ended.as_deref(), Some("lost the connection"));
    assert_eq!(g.gv.ended.as_deref(), Some("lost the connection"));
}

#[test]
fn times_out_and_says_bye() {
    let mut s = TrailsSession::new(Role::Guest, Box::new(|_| {}), 0.0);
    assert_eq!(s.tick(TRAILS_SILENCE_MS - 1.0).ended, None);
    assert_eq!(s.tick(TRAILS_SILENCE_MS + 1.0).ended.as_deref(), Some("lost the connection"));
    let mut g = Pair::simple();
    g.run(4000.0, &mut running);
    g.guest.leave();
    g.route();
    g.run_for(300.0);
    assert_eq!(g.hv.ended.as_deref(), Some("your friend left"));
    let now = g.now;
    assert_eq!(g.guest.tick(now).ended.as_deref(), Some(""));
}
