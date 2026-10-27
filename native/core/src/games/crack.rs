//! `src/lib/crack.ts`: a needle sweeps a safe dial; press as it crosses the dot. level n needs n hits in a row.
//! pure and deterministic: time comes in as ms, randomness as a () -> [0, 1) function. (`isCrackCommand` is in
//! `app/games.rs` with the other bar commands.)

use std::f64::consts::PI;

use super::Rand;

const TAU: f64 = PI * 2.0;

/// How far either side of the dot's centre still counts, in radians (~9.5°, about the dot's drawn size).
pub const HIT_WINDOW: f64 = 0.165;
/// A new dot never lands closer than this to the needle, either side (~50°).
pub const MIN_GAP: f64 = 0.87;
/// …nor further ahead than this, so the wait for it stays short (270°).
pub const MAX_AHEAD: f64 = TAU * 0.75;
/// Radians per second at level 1, the step per level, and the ceiling.
pub const BASE_SPEED: f64 = 2.3;
pub const SPEED_STEP: f64 = 0.1;
pub const MAX_SPEED: f64 = 4.4;
/// The shake after a miss, and the click-open after a cleared level.
pub const FAIL_MS: f64 = 650.0;
pub const CLEAR_MS: f64 = 950.0;
// a frame never moves the needle by more than this much time, so a stalled frame doesn't skip the dot
const MAX_STEP_MS: f64 = 50.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrackPhase {
    Ready,
    Play,
    Fail,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrackState {
    pub level: u32,
    /// Hits still needed this level.
    pub left: u32,
    /// The needle, radians clockwise from 12 o'clock, in [0, 2π).
    pub angle: f64,
    /// 1 = clockwise, -1 the other way.
    pub dir: f64,
    /// The target, radians like angle.
    pub dot: f64,
    /// How far the needle still has to travel to the dot's centre; negative once past it.
    pub ahead: f64,
    pub phase: CrackPhase,
    /// ms of the last update.
    pub at: f64,
    /// ms the phase began.
    pub since: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrackEvent {
    Start,
    Hit,
    Clear,
    Miss,
}

pub type Step = (CrackState, Option<CrackEvent>);

pub fn speed_for(level: u32) -> f64 {
    MAX_SPEED.min(BASE_SPEED + SPEED_STEP * (level.max(1) - 1) as f64)
}

fn wrap(a: f64) -> f64 {
    ((a % TAU) + TAU) % TAU
}

// a fresh dot somewhere ahead of the needle in its direction, never within MIN_GAP of it
fn place_dot(s: CrackState, rand: Rand) -> CrackState {
    let ahead = MIN_GAP + rand() * (MAX_AHEAD - MIN_GAP);
    CrackState { ahead, dot: wrap(s.angle + s.dir * ahead), ..s }
}

/// A level waiting for its first press; the needle keeps where it was.
pub fn start_level(level: u32, now: f64, rand: Rand, angle: f64, dir: f64) -> CrackState {
    let lv = level.max(1);
    place_dot(
        CrackState {
            level: lv,
            left: lv,
            angle: wrap(angle),
            dir,
            dot: 0.0,
            ahead: 0.0,
            phase: CrackPhase::Ready,
            at: now,
            since: now,
        },
        rand,
    )
}

pub fn over_dot(s: &CrackState) -> bool {
    s.ahead.abs() <= HIT_WINDOW
}

/// Moves the needle to now, fails the level if it has gone right past the dot, and ends the shake or the
/// click-open when their time is up.
pub fn tick(s: CrackState, now: f64, rand: Rand) -> Step {
    if s.phase == CrackPhase::Fail && now - s.since >= FAIL_MS {
        return (start_level(s.level, now, rand, s.angle, s.dir), None);
    }
    if s.phase == CrackPhase::Clear && now - s.since >= CLEAR_MS {
        return (start_level(s.level + 1, now, rand, s.angle, s.dir), None);
    }
    if s.phase != CrackPhase::Play {
        return (CrackState { at: now, ..s }, None);
    }
    let dt = MAX_STEP_MS.min((now - s.at).max(0.0)) / 1000.0;
    let mv = speed_for(s.level) * dt;
    let next = CrackState { angle: wrap(s.angle + s.dir * mv), ahead: s.ahead - mv, at: now, ..s };
    if next.ahead < -HIT_WINDOW {
        return (CrackState { phase: CrackPhase::Fail, since: now, ..next }, Some(CrackEvent::Miss));
    }
    (next, None)
}

/// Space / ↵ / a click. in ready it starts the needle; in play it's a hit over the dot and a miss anywhere else.
pub fn press(s: CrackState, now: f64, rand: Rand) -> Step {
    if s.phase == CrackPhase::Ready {
        return (CrackState { phase: CrackPhase::Play, at: now, since: now, ..s }, Some(CrackEvent::Start));
    }
    if s.phase != CrackPhase::Play {
        return (s, None);
    }
    let (cur, event) = tick(s, now, rand);
    if event == Some(CrackEvent::Miss) {
        return (cur, event);
    }
    if !over_dot(&cur) {
        return (CrackState { phase: CrackPhase::Fail, since: now, ..cur }, Some(CrackEvent::Miss));
    }
    let left = cur.left - 1;
    if left == 0 {
        return (CrackState { left, ahead: 0.0, phase: CrackPhase::Clear, since: now, ..cur }, Some(CrackEvent::Clear));
    }
    let flipped = CrackState { left, dir: -cur.dir, ..cur };
    (place_dot(flipped, rand), Some(CrackEvent::Hit))
}

/// The furthest level reached, kept across runs as `qcalc-crack`.
pub const BEST_KEY: &str = "qcalc-crack";

pub fn parse_best(raw: Option<&str>) -> u32 {
    let n = raw.and_then(crate::app::js::number).unwrap_or(f64::NAN);
    if n.fract() == 0.0 && (1.0..=9999.0).contains(&n) { n as u32 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(v: f64) -> impl FnMut() -> f64 {
        move || v
    }

    fn gap(a: f64, b: f64) -> f64 {
        let d = (a - b).abs() % TAU;
        d.min(TAU - d)
    }

    fn run_to(s: CrackState, ahead: f64) -> CrackState {
        let ms = (s.ahead - ahead) / speed_for(s.level) * 1000.0;
        let steps = (ms / 10.0).ceil() as i32;
        let mut cur = s;
        for i in 1..=steps {
            let (next, event) = tick(cur, s.at + ms * i as f64 / steps as f64, &mut fixed(0.5));
            cur = next;
            if event == Some(CrackEvent::Miss) {
                break;
            }
        }
        cur
    }

    fn playing(level: u32) -> CrackState {
        press(start_level(level, 0.0, &mut fixed(0.5), 0.0, 1.0), 0.0, &mut fixed(0.5)).0
    }

    #[test]
    fn start_level_waits_and_places_the_dot() {
        let s = start_level(4, 100.0, &mut fixed(0.3), 0.0, 1.0);
        assert_eq!(s.left, 4);
        assert_eq!(s.phase, CrackPhase::Ready);
        assert_eq!(tick(s, 5000.0, &mut fixed(0.3)).0.angle, s.angle);
        for r in [0.0, 0.001, 0.25, 0.5, 0.999, 0.9999999] {
            for dir in [1.0, -1.0] {
                let s = start_level(1, 0.0, &mut fixed(r), 1.2, dir);
                assert!(s.ahead >= MIN_GAP && s.ahead <= MAX_AHEAD);
                assert!(gap(s.dot, s.angle) >= MIN_GAP - 1e-9);
            }
        }
        assert!((start_level(1, 0.0, &mut fixed(0.0), 0.0, 1.0).dot - MIN_GAP).abs() < 1e-9);
        assert!((start_level(1, 0.0, &mut fixed(0.0), 0.0, -1.0).dot - (TAU - MIN_GAP)).abs() < 1e-9);
    }

    #[test]
    fn the_hit_window() {
        for ahead in [HIT_WINDOW * 0.95, 0.0, -HIT_WINDOW * 0.95] {
            let s = run_to(playing(3), ahead);
            assert!(over_dot(&s));
            let (r, e) = press(s, s.at, &mut fixed(0.5));
            assert_eq!(e, Some(CrackEvent::Hit));
            assert_eq!(r.left, 2);
        }
        let s = run_to(playing(3), HIT_WINDOW * 1.1);
        let (r, e) = press(s, s.at, &mut fixed(0.5));
        assert_eq!(e, Some(CrackEvent::Miss));
        assert_eq!(r.phase, CrackPhase::Fail);
        assert_eq!(r.left, 3);
        assert_eq!(press(playing(2), 10.0, &mut fixed(0.5)).1, Some(CrackEvent::Miss));
    }

    #[test]
    fn a_hit_reverses_the_needle() {
        let s = run_to(playing(3), 0.0);
        assert_eq!(s.dir, 1.0);
        let (r, _) = press(s, s.at, &mut fixed(0.2));
        assert_eq!(r.dir, -1.0);
        assert!(r.ahead >= MIN_GAP);
        assert!(gap(r.dot, r.angle) >= MIN_GAP - 1e-9);
        let later = tick(r, r.at + 40.0, &mut fixed(0.2)).0;
        let moved = later.angle - r.angle;
        assert_eq!((if moved.abs() > PI { -moved } else { moved }).signum(), -1.0);
    }

    #[test]
    fn passing_the_dot() {
        let before = run_to(playing(2), -HIT_WINDOW * 0.9);
        assert_eq!(before.phase, CrackPhase::Play);
        let (r, e) = tick(before, before.at + 60.0, &mut fixed(0.5));
        assert_eq!(e, Some(CrackEvent::Miss));
        assert_eq!(r.phase, CrackPhase::Fail);
        let s = run_to(playing(1), HIT_WINDOW * 1.5);
        let (r, e) = tick(s, s.at + 2000.0, &mut fixed(0.5));
        assert_eq!(e, None);
        assert!(r.ahead > -HIT_WINDOW);
        let failed = press(playing(3), 10.0, &mut fixed(0.5)).0;
        assert_eq!(press(failed, 20.0, &mut fixed(0.5)).1, None);
        assert_eq!(tick(failed, 10.0 + FAIL_MS - 1.0, &mut fixed(0.5)).0.phase, CrackPhase::Fail);
        let back = tick(failed, 10.0 + FAIL_MS, &mut fixed(0.5)).0;
        assert_eq!((back.phase, back.level, back.left), (CrackPhase::Ready, 3, 3));
    }

    #[test]
    fn levels() {
        let mut s = playing(3);
        for i in 0..3 {
            s = run_to(s, 0.0);
            let (r, e) = press(s, s.at, &mut fixed(0.5));
            assert_eq!(e, Some(if i < 2 { CrackEvent::Hit } else { CrackEvent::Clear }));
            s = r;
        }
        assert_eq!((s.phase, s.left), (CrackPhase::Clear, 0));
        assert_eq!(tick(s, s.since + CLEAR_MS - 1.0, &mut fixed(0.5)).0.phase, CrackPhase::Clear);
        let next = tick(s, s.since + CLEAR_MS, &mut fixed(0.5)).0;
        assert_eq!((next.phase, next.level, next.left), (CrackPhase::Ready, 4, 4));
        assert_eq!(speed_for(1), BASE_SPEED);
        for n in 1..60 {
            assert!(speed_for(n + 1) >= speed_for(n));
        }
        assert!(speed_for(2) - speed_for(1) < 0.2);
        assert_eq!(speed_for(1000), MAX_SPEED);
        assert_eq!(parse_best(Some("7")), 7);
        for raw in [None, Some(""), Some("x"), Some("0"), Some("-2"), Some("2.5")] {
            assert_eq!(parse_best(raw), 1);
        }
    }
}
