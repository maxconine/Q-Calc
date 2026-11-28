//! slots: `slots` then enter. A three-reel, one-line slot machine played for free chips (no money): bet 1–10,
//! spin, three of a kind on the line pays. Native only, so there is no `src/lib` original. Pure and
//! deterministic: time comes in as ms, randomness as a () -> [0, 1) function. (`slots` the command is in
//! `app/games.rs` with the other bar commands.)
//!
//! The reels' motion is here too, so a press can stop one where it is: each winds up, rolls at SPEED, and lands
//! on a stop at full speed, running OVERSHOOT past it and springing back (the bounce of a real reel). Unstopped,
//! reel i lands on a stop picked at random at the spin, the first time it comes round after BASE_MS[i]; a press
//! stops the leftmost reel still rolling on the next stop to reach the line. `position` is where a reel is at any
//! time, which the panels draw.

use serde::{Deserialize, Serialize};

use super::Rand;

/// The symbols, by id. 6 is a blank, which the strip no longer has (every stop is an icon); kept as an id.
pub const SYMBOLS: [&str; 7] = ["7", "π", "∞", "∑", "√", "e", ""];
pub const SEVEN: u8 = 0;
pub const PI: u8 = 1;
pub const INF: u8 = 2;
pub const SUM: u8 = 3;
pub const ROOT: u8 = 4;
pub const E: u8 = 5;
pub const BLANK: u8 = 6;

/// One reel's stops, top to bottom; all three reels share it. 7 ×1, π ×3, ∞ ×5, ∑ ×7, √ ×8, e ×8, and no
/// blanks, so every stop on the line is an icon (the user's call); spread so no two of a symbol touch.
pub const STRIP: [u8; 32] = [
    SEVEN, SUM, E, INF, ROOT, PI, SUM, E, ROOT, INF, SUM, E, PI, ROOT, INF, E,
    SUM, ROOT, E, INF, SUM, ROOT, E, SUM, ROOT, E, ROOT, SUM, E, INF, PI, ROOT,
];

/// What three of a kind pays, as times the bet (7, π, ∞, ∑, √, e). With no blanks three of a kind comes up far
/// more often, so these are lower than they were (but 7 7 7); the machine pays back about 97% (`return_to_player`).
pub const THREE: [u32; 6] = [500, 100, 50, 25, 12, 6];
/// Two e's anywhere on the line (the bet back), and one (nothing now: one e is on 4 lines in 10).
pub const TWO_E: u32 = 1;
pub const ONE_E: u32 = 0;
/// A win of this many times the bet or more is a big one (the panel makes more of it).
pub const BIG: u32 = 50;

pub const START_CHIPS: u32 = 100;
pub const MIN_BET: u32 = 1;
pub const MAX_BET: u32 = 10;
/// Stops a second a rolling reel passes (per ms), and the wind-up from rest to it.
pub const SPEED: f64 = 0.03;
pub const WINDUP_MS: f64 = 220.0;
/// How far past its stop a landing reel runs, in stops, and the spring back.
pub const OVERSHOOT: f64 = 0.25;
pub const DECEL_MS: f64 = 2.0 * OVERSHOOT / SPEED;
pub const SETTLE_MS: f64 = 300.0;
/// Unstopped, each reel lands the first time its stop comes round after these (ms after the spin), and never
/// sooner than MIN_GAP_MS after the reel to its left.
pub const BASE_MS: [f64; 3] = [700.0, 950.0, 1200.0];
pub const MIN_GAP_MS: f64 = 180.0;
/// The line pays this long after the last reel lands.
pub const PAY_MS: f64 = 150.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotsPhase {
    Idle,
    Spinning,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotsState {
    pub chips: u32,
    pub bet: u32,
    /// The most chips ever held.
    pub best: u32,
    /// Each reel's stop on the line, an index into STRIP: the last spin's while idle, where each will land while
    /// spinning.
    pub reels: [u32; 3],
    pub phase: SlotsPhase,
    /// ms the spin began.
    pub since: f64,
    /// What the last finished spin paid (0 for none).
    pub win: u32,
    pub spins: u32,
    /// Each reel's stop when the spin began.
    pub from: [f64; 3],
    /// Where each lands, counted on from `from` (so whole turns included; `reels` is this mod 32).
    pub target: [f64; 3],
    /// When each reaches its target (ms); -inf before any spin.
    pub land: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotsEvent {
    Spin,
    /// A press stopped a reel.
    Stop,
    Win,
    Big,
    Lose,
    Refill,
}

pub type Step = (SlotsState, Option<SlotsEvent>);

/// The symbols on the line for these stops.
pub fn line(reels: [u32; 3]) -> [u8; 3] {
    reels.map(|r| STRIP[r as usize % STRIP.len()])
}

/// Times the bet a line pays.
pub fn multiplier(line: [u8; 3]) -> u32 {
    if line[0] == line[1] && line[1] == line[2] && line[0] != BLANK {
        return THREE[line[0] as usize];
    }
    match line.iter().filter(|&&s| s == E).count() {
        2 => TWO_E,
        1 => ONE_E,
        _ => 0,
    }
}

pub fn new_game(chips: u32, bet: u32, best: u32) -> SlotsState {
    let reels = [0, 12, 25];
    SlotsState {
        chips,
        bet: bet.clamp(MIN_BET, MAX_BET),
        best: best.max(chips),
        reels,
        phase: SlotsPhase::Idle,
        since: 0.0,
        win: 0,
        spins: 0,
        from: reels.map(f64::from),
        target: reels.map(f64::from),
        land: [f64::NEG_INFINITY; 3],
    }
}

/// The bet, kept in 1–10; only between spins.
pub fn set_bet(s: SlotsState, bet: i64) -> SlotsState {
    if s.phase != SlotsPhase::Idle {
        return s;
    }
    SlotsState {
        bet: bet.clamp(MIN_BET as i64, MAX_BET as i64) as u32,
        ..s
    }
}

/// Stops rolled t ms into a spin, before landing: a wind-up from rest, then SPEED.
pub fn rolled(t: f64) -> f64 {
    if t <= 0.0 {
        0.0
    } else if t < WINDUP_MS {
        0.5 * SPEED / WINDUP_MS * t * t
    } else {
        SPEED * WINDUP_MS / 2.0 + SPEED * (t - WINDUP_MS)
    }
}

/// The inverse of `rolled`: ms into the spin to have rolled d stops.
pub fn time_to_roll(d: f64) -> f64 {
    let windup = SPEED * WINDUP_MS / 2.0;
    if d <= 0.0 {
        0.0
    } else if d <= windup {
        (2.0 * d * WINDUP_MS / SPEED).sqrt()
    } else {
        WINDUP_MS + (d - windup) / SPEED
    }
}

/// Where reel i is at `now`, in stops counted on from where the spin began (mod 32 is the stop on the line):
/// rolling, then past its stop by up to OVERSHOOT and springing back.
pub fn position(s: &SlotsState, i: usize, now: f64) -> f64 {
    let dt = now - s.land[i];
    if dt < 0.0 {
        return s.from[i] + rolled(now - s.since);
    }
    if dt < DECEL_MS {
        return s.target[i] + SPEED * dt - 0.5 * SPEED / DECEL_MS * dt * dt;
    }
    let k = (dt - DECEL_MS) / SETTLE_MS;
    if k < 1.0 {
        return s.target[i] + OVERSHOOT * (1.0 - k).powi(3);
    }
    s.target[i]
}

/// When reel i is still at last (its bounce done).
pub fn settled_at(s: &SlotsState, i: usize) -> f64 {
    s.land[i] + DECEL_MS + SETTLE_MS
}

/// How many reels have landed by now.
pub fn stopped(s: &SlotsState, now: f64) -> u32 {
    s.land.iter().filter(|&&t| t <= now).count() as u32
}

fn n() -> f64 {
    STRIP.len() as f64
}

fn wrap(p: f64) -> u32 {
    (p.rem_euclid(n()) as u32).min(STRIP.len() as u32 - 1)
}

/// Space / ↵ / the lever. Takes the bet (or what's left, when less) and picks where each reel lands unless
/// stopped; with no chips at all it hands out a fresh START_CHIPS instead.
pub fn spin(s: SlotsState, now: f64, rand: Rand) -> Step {
    if s.phase != SlotsPhase::Idle {
        return (s, None);
    }
    if s.chips == 0 {
        return (
            SlotsState {
                chips: START_CHIPS,
                win: 0,
                ..s
            },
            Some(SlotsEvent::Refill),
        );
    }
    let stake = s.bet.min(s.chips);
    let from = s.reels.map(f64::from);
    let mut target = [0.0; 3];
    let mut land = [0.0; 3];
    let mut after = f64::NEG_INFINITY;
    for i in 0..3 {
        let pick = ((rand() * n()) as u32).min(STRIP.len() as u32 - 1) as f64;
        let base = BASE_MS[i].max(after + MIN_GAP_MS);
        // the first stop to reach the line from then on, then on round to the picked one
        let first = (from[i] + rolled(base)).ceil();
        target[i] = first + (pick - first).rem_euclid(n());
        let t = time_to_roll(target[i] - from[i]);
        land[i] = now + t;
        after = t;
    }
    (
        SlotsState {
            chips: s.chips - stake,
            bet: stake,
            reels: target.map(wrap),
            phase: SlotsPhase::Spinning,
            since: now,
            win: 0,
            spins: s.spins + 1,
            from,
            target,
            land,
            ..s
        },
        Some(SlotsEvent::Spin),
    )
}

/// Space while spinning: the leftmost reel still rolling lands on the next stop to reach the line. Not during
/// the wind-up.
pub fn press(s: SlotsState, now: f64) -> Step {
    if s.phase != SlotsPhase::Spinning || now - s.since < WINDUP_MS {
        return (s, None);
    }
    let Some(i) = (0..3).find(|&i| s.land[i] > now) else {
        return (s, None);
    };
    let mut next = s;
    let target = (s.from[i] + rolled(now - s.since)).ceil();
    next.target[i] = target;
    next.land[i] = s.since + time_to_roll(target - s.from[i]);
    next.reels[i] = wrap(target);
    (next, Some(SlotsEvent::Stop))
}

/// Ends the spin a moment after the last reel lands, paying the line.
pub fn tick(s: SlotsState, now: f64) -> Step {
    let last = s.land.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if s.phase != SlotsPhase::Spinning || now < last + PAY_MS {
        return (s, None);
    }
    let times = multiplier(line(s.reels));
    let win = s.bet * times;
    let chips = s.chips + win;
    let event = if times >= BIG {
        SlotsEvent::Big
    } else if win > 0 {
        SlotsEvent::Win
    } else {
        SlotsEvent::Lose
    };
    (
        SlotsState {
            chips,
            best: s.best.max(chips),
            phase: SlotsPhase::Idle,
            win,
            ..s
        },
        Some(event),
    )
}

/// Chips, bet and best, kept across launches as `qcalc-slots`.
pub const SAVE_KEY: &str = "qcalc-slots";

#[derive(Serialize, Deserialize)]
struct Save {
    chips: u32,
    bet: u32,
    best: u32,
}

/// The saved game, or a fresh one (START_CHIPS, bet 1) for nothing or anything unreadable. A spin left running
/// when the game closed was already paid for; it's lost, like a coin in a machine you walked away from.
pub fn load(raw: Option<&str>) -> SlotsState {
    match raw.and_then(|r| serde_json::from_str::<Save>(r).ok()) {
        Some(v) if v.chips <= 100_000_000 => new_game(v.chips, v.bet, v.best.min(100_000_000)),
        _ => new_game(START_CHIPS, MIN_BET, START_CHIPS),
    }
}

/// What to save; a spin still running saves what it will pay, so closing mid-spin loses nothing.
pub fn save(s: &SlotsState) -> String {
    let done = if s.phase == SlotsPhase::Spinning {
        tick(*s, f64::INFINITY).0
    } else {
        *s
    };
    serde_json::to_string(&Save {
        chips: done.chips,
        bet: done.bet,
        best: done.best,
    })
    .unwrap_or_default()
}

/// The pay table, best first, as (the line drawn, times the bet).
pub fn pay_table() -> Vec<(String, u32)> {
    let mut rows: Vec<(String, u32)> = THREE
        .iter()
        .enumerate()
        .map(|(i, &x)| ([SYMBOLS[i]; 3].join(" "), x))
        .collect();
    rows.push(("e e".into(), TWO_E));
    if ONE_E > 0 {
        rows.push(("e".into(), ONE_E));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(v: f64) -> impl FnMut() -> f64 {
        move || v
    }

    fn count(sym: u8) -> usize {
        STRIP.iter().filter(|&&s| s == sym).count()
    }

    #[test]
    fn the_strip() {
        assert_eq!(
            [SEVEN, PI, INF, SUM, ROOT, E, BLANK].map(count),
            [1, 3, 5, 7, 8, 8, 0]
        );
        for i in 0..STRIP.len() {
            let (a, b) = (STRIP[i], STRIP[(i + 1) % STRIP.len()]);
            assert!(a != b, "two {a} touch at {i}");
        }
        // every stop is an icon: the line, and the stops above and below it, never show an empty slot
        assert!(STRIP.iter().all(|&s| !SYMBOLS[s as usize].is_empty()));
    }

    #[test]
    fn pays() {
        assert_eq!(multiplier([SEVEN; 3]), 500);
        assert_eq!(multiplier([E; 3]), 6);
        assert_eq!(multiplier([BLANK; 3]), 0);
        assert_eq!(multiplier([E, BLANK, E]), TWO_E);
        assert_eq!(multiplier([ROOT, ROOT, E]), 0);
        assert_eq!(multiplier([E, E, E]), 6);
        assert_eq!(multiplier([PI, PI, PI]), 100);
        assert_eq!(multiplier([SEVEN, SEVEN, PI]), 0);
    }

    // over every one of the 32³ lines: the machine pays back a little under what it takes
    #[test]
    fn return_to_player() {
        let n = STRIP.len();
        let mut paid = 0u64;
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    paid += multiplier(line([a as u32, b as u32, c as u32])) as u64;
                }
            }
        }
        let rtp = paid as f64 / (n * n * n) as f64;
        assert!((0.9..0.99).contains(&rtp), "rtp {rtp}");
        // and about one spin in five wins something
        let hits = (0..n * n * n)
            .filter(|i| multiplier(line([(i / (n * n)) as u32, (i / n % n) as u32, (i % n) as u32])) > 0)
            .count() as f64
            / (n * n * n) as f64;
        assert!((0.15..0.25).contains(&hits), "hits {hits}");
    }

    #[test]
    fn rolling() {
        assert_eq!(rolled(0.0), 0.0);
        for d in [0.5, 3.3, 10.0, 40.0] {
            assert!((rolled(time_to_roll(d)) - d).abs() < 1e-9);
        }
        // no jump in speed at the end of the wind-up
        let v = |t: f64| (rolled(t + 0.01) - rolled(t)) / 0.01;
        assert!((v(WINDUP_MS - 0.02) - SPEED).abs() < 1e-3);
    }

    #[test]
    fn a_spin_lands_where_it_picked() {
        for r in [0.0, 0.3, 0.77, 0.9999] {
            let (s, e) = spin(new_game(100, 5, 100), 1000.0, &mut fixed(r));
            assert_eq!(e, Some(SlotsEvent::Spin));
            assert_eq!((s.chips, s.phase), (95, SlotsPhase::Spinning));
            let pick = ((r * 32.0) as u32).min(31);
            assert_eq!(s.reels, [pick; 3]);
            for i in 0..3 {
                assert!(s.land[i] - s.since >= BASE_MS[i]);
                assert!(
                    s.land[i] - s.since
                        < BASE_MS[i].max(if i > 0 {
                            s.land[i - 1] - s.since + MIN_GAP_MS
                        } else {
                            0.0
                        }) + 32.0 / SPEED
                            + 1.0
                );
                if i > 0 {
                    assert!(s.land[i] - s.land[i - 1] >= MIN_GAP_MS);
                }
                // smooth: where it lands, at speed, then a quarter stop past, then back
                let p = |t: f64| position(&s, i, t);
                assert!((p(s.land[i]) - s.target[i]).abs() < 1e-9);
                assert!((p(s.land[i] - 0.001) - s.target[i]).abs() < 1e-3);
                assert!((p(s.land[i] + DECEL_MS) - s.target[i] - OVERSHOOT).abs() < 1e-9);
                assert!((p(settled_at(&s, i)) - s.target[i]).abs() < 1e-9);
                assert_eq!(wrap(p(1e9)), pick);
            }
            assert_eq!(spin(s, 1001.0, &mut fixed(0.0)).1, None);
            assert_eq!(set_bet(s, 9).bet, 5);
        }
    }

    #[test]
    fn paying() {
        let (s, _) = spin(new_game(100, 5, 100), 1000.0, &mut fixed(0.0));
        let last = s.land[2];
        assert_eq!(stopped(&s, s.land[0]), 1);
        assert_eq!(tick(s, last + PAY_MS - 1.0).1, None);
        let (s, e) = tick(s, last + PAY_MS);
        // 7 7 7 at 5 chips
        assert_eq!(e, Some(SlotsEvent::Big));
        assert_eq!(
            (s.chips, s.win, s.best, s.phase),
            (95 + 2500, 2500, 2595, SlotsPhase::Idle)
        );
        // 7 π ∑: no blank to lose on now, so a losing line is three different icons with fewer than two e's
        let mut picks = [0.0, 5.0, 1.0].map(|k: f64| (k + 0.5) / 32.0).into_iter();
        let (s, _) = spin(s, 0.0, &mut move || picks.next().unwrap());
        assert_eq!(line(s.reels), [SEVEN, PI, SUM]);
        let (s, e) = tick(s, 1e6);
        assert_eq!((e, s.win), (Some(SlotsEvent::Lose), 0));
    }

    #[test]
    fn pressing_stops_each_reel_where_it_is() {
        let (s, _) = spin(new_game(100, 1, 100), 0.0, &mut fixed(0.5));
        // not in the wind-up
        assert_eq!(press(s, WINDUP_MS - 1.0).1, None);
        let at = 400.0;
        let (a, e) = press(s, at);
        assert_eq!(e, Some(SlotsEvent::Stop));
        let here = position(&s, 0, at);
        assert_eq!(a.target[0], here.ceil());
        assert_eq!(a.reels[0], wrap(here.ceil()));
        assert!(a.land[0] >= at && a.land[0] - at <= 1.0 / SPEED + 1e-9);
        // the others roll on as they were
        assert_eq!(
            (a.land[1], a.land[2], a.reels[1]),
            (s.land[1], s.land[2], s.reels[1])
        );
        // the path doesn't jump
        assert!((position(&a, 0, at) - here).abs() < 1e-9);
        // the next press takes the next reel, then the last; then nothing's left to stop
        let (b, _) = press(a, 500.0);
        assert!(b.land[1] <= 500.0 + 1.0 / SPEED + 1e-9);
        let (c, _) = press(b, 600.0);
        assert!(c.land[2] <= 600.0 + 1.0 / SPEED + 1e-9);
        assert_eq!(press(c, 700.0).1, None);
        let (done, e) = tick(c, c.land[2] + PAY_MS);
        assert!(e.is_some() && done.phase == SlotsPhase::Idle);
        assert_eq!(press(done, 1e6).1, None);
    }

    #[test]
    fn running_out() {
        let s = new_game(3, 10, 100);
        // 7 π ∑, a losing line
        let mut picks = [0.0, 5.0, 1.0].map(|k: f64| (k + 0.5) / 32.0).into_iter();
        let (s, _) = spin(s, 0.0, &mut move || picks.next().unwrap());
        assert_eq!((s.chips, s.bet), (0, 3));
        let (s, _) = tick(s, 1e6);
        assert_eq!(s.chips, 0);
        let (s, e) = spin(s, 2e6, &mut fixed(0.5));
        assert_eq!(
            (e, s.chips, s.phase),
            (Some(SlotsEvent::Refill), START_CHIPS, SlotsPhase::Idle)
        );
        assert_eq!(set_bet(s, 0).bet, 1);
        assert_eq!(set_bet(s, 99).bet, 10);
    }

    #[test]
    fn saves() {
        assert_eq!(load(None).chips, START_CHIPS);
        assert_eq!(load(Some("junk")).bet, 1);
        let s = load(Some(r#"{"chips":42,"bet":7,"best":300}"#));
        assert_eq!((s.chips, s.bet, s.best), (42, 7, 300));
        assert_eq!(load(Some(&save(&s))), s);
        // mid-spin saves what the spin pays
        let (mid, _) = spin(new_game(10, 1, 10), 0.0, &mut fixed(0.0));
        assert_eq!(load(Some(&save(&mid))).chips, 9 + 500);
        assert_eq!(pay_table()[0], ("7 7 7".into(), 500));
    }
}
