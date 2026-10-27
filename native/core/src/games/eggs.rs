//! The 420 smoke, 67 arms and 69 fold that play over the bar when the answer settles on that number
//! (`src/lib/fourTwenty.ts`, `sixtySeven.ts`, `sixtyNine.ts`, `smokeRoom.ts`, and the hooks' timings).

/// Which effect an answer plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Egg {
    /// 420: smoke rolls off the bar
    Smoke,
    /// 67: two hands weigh it up
    Arms,
    /// 69: the digits fold
    Fold,
}

impl Egg {
    pub const ALL: [Egg; 3] = [Egg::Smoke, Egg::Arms, Egg::Fold];

    /// the number that sets it off
    pub fn number(self) -> f64 {
        match self {
            Egg::Smoke => 420.0,
            Egg::Arms => 67.0,
            Egg::Fold => 69.0,
        }
    }

    /// the name the web bundle used for it (games.html#smoke, #arms, #fold)
    pub fn kind(self) -> &'static str {
        match self {
            Egg::Smoke => "smoke",
            Egg::Arms => "arms",
            Egg::Fold => "fold",
        }
    }

    /// how long the effect stays up, ms: each hook's timer, a little longer than its css animation
    pub fn play_ms(self) -> f64 {
        match self {
            Egg::Smoke => SMOKE_MS,
            Egg::Arms => SHAKE_MS,
            Egg::Fold => FOLD_MS,
        }
    }
}

/// long enough that typing `4200` passes through 420 without firing (FOUR_TWENTY_SETTLE_MS; the same for 67 and 69)
pub const SETTLE_MS: f64 = 200.0;
/// useFourTwentySmoke: a little longer than the 2.5s css animation
pub const SMOKE_MS: f64 = 2700.0;
/// useSixtySevenArms
pub const SHAKE_MS: f64 = 2700.0;
/// useSixtyNineFold
pub const FOLD_MS: f64 = 2400.0;

/// fourTwentyGate / sixtySevenGate / sixtyNineGate: fires once per `target`, and re-arms only after the settled
/// answer is something else. Returns (armed, fire).
pub fn gate(target: f64, armed: bool, n: Option<f64>) -> (bool, bool) {
    if n == Some(target) {
        return (false, armed);
    }
    (true, false)
}

/// The three gates side by side, as the bar runs them: each settled answer goes through all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EggGates {
    armed: [bool; 3],
}

impl Default for EggGates {
    fn default() -> Self {
        Self { armed: [true; 3] }
    }
}

impl EggGates {
    /// the settled answer (None when it isn't a plain number); the effect to play, if any
    pub fn settle(&mut self, n: Option<f64>) -> Option<Egg> {
        let mut out = None;
        for (i, egg) in Egg::ALL.iter().enumerate() {
            let (armed, fire) = gate(egg.number(), self.armed[i], n);
            self.armed[i] = armed;
            if fire {
                out = Some(*egg);
            }
        }
        out
    }
}

/// Room the host adds around the bar while the 420 smoke plays, in css px; the bar keeps its size and spot
/// (smokeRoom.ts `SMOKE_ROOM`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeRoom {
    pub top: f64,
    pub side: f64,
    pub bottom: f64,
}

pub const SMOKE_ROOM: SmokeRoom = SmokeRoom {
    top: 160.0,
    side: 100.0,
    bottom: 64.0,
};

/// wideSmokeRoom: only a host that says it can grow the window sideways gets the wide room
pub fn wide_smoke_room(native: bool, smoke_room: bool) -> bool {
    native && smoke_room
}

/// Where along the bar a puff sits: a fraction of its top, or its left or right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PuffAt {
    Left,
    Right,
    Frac(f64),
}

/// One puff of the 420 smoke (FourTwentySmoke.tsx `PUFFS`): x, y offset it (y below the bar's top), w, h its
/// size; delay in seconds and drift, rise, sway in px, as the css variables carry them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub at: PuffAt,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub delay: f64,
    pub drift: f64,
    pub rise: f64,
    pub sway: f64,
}

#[allow(clippy::too_many_arguments)]
const fn puff(
    at: PuffAt,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    delay: f64,
    drift: f64,
    rise: f64,
    sway: f64,
) -> Puff {
    Puff {
        at,
        x,
        y,
        w,
        h,
        delay,
        drift,
        rise,
        sway,
    }
}

use PuffAt::{Frac, Left, Right};

#[rustfmt::skip]
pub const PUFFS: [Puff; 22] = [
    // off the top, overlapping into one cloud that fans out as it rises
    puff(Frac(0.08), 0.0, 10.0, 150.0, 100.0, 0.04, -58.0, -70.0, -30.0),
    puff(Frac(0.2), 0.0, -4.0, 190.0, 110.0, 0.0, -44.0, -92.0, -16.0),
    puff(Frac(0.33), 0.0, 8.0, 170.0, 104.0, 0.1, -30.0, -108.0, 8.0),
    puff(Frac(0.47), 0.0, -10.0, 220.0, 120.0, 0.02, -8.0, -96.0, -6.0),
    puff(Frac(0.6), 0.0, 6.0, 180.0, 110.0, 0.07, 18.0, -112.0, 10.0),
    puff(Frac(0.72), 0.0, -6.0, 200.0, 112.0, 0.12, 36.0, -94.0, -8.0),
    puff(Frac(0.86), 0.0, 10.0, 160.0, 100.0, 0.03, 52.0, -80.0, 22.0),
    puff(Frac(0.15), 0.0, -20.0, 110.0, 110.0, 0.44, -48.0, -74.0, -20.0),
    puff(Frac(0.4), 0.0, -30.0, 130.0, 130.0, 0.3, -20.0, -84.0, -10.0),
    puff(Frac(0.52), 0.0, -48.0, 240.0, 110.0, 0.2, 4.0, -60.0, 6.0),
    puff(Frac(0.62), 0.0, -26.0, 120.0, 124.0, 0.36, 24.0, -88.0, 8.0),
    puff(Frac(0.83), 0.0, -22.0, 110.0, 110.0, 0.48, 46.0, -76.0, 20.0),
    // the corners, out and up
    puff(Left, 16.0, 4.0, 140.0, 100.0, 0.05, -74.0, -76.0, -44.0),
    puff(Right, -16.0, 4.0, 140.0, 100.0, 0.08, 74.0, -76.0, 44.0),
    // beside the bar, rolling out and curling up
    puff(Left, -4.0, 28.0, 110.0, 96.0, 0.0, -70.0, -56.0, -46.0),
    puff(Left, 6.0, 54.0, 96.0, 104.0, 0.12, -62.0, -80.0, -40.0),
    puff(Left, -10.0, 40.0, 120.0, 100.0, 0.3, -78.0, -44.0, -54.0),
    puff(Left, 10.0, 70.0, 80.0, 80.0, 0.42, -52.0, -64.0, -36.0),
    puff(Right, 4.0, 30.0, 112.0, 94.0, 0.04, 72.0, -58.0, 48.0),
    puff(Right, -6.0, 56.0, 94.0, 106.0, 0.16, 60.0, -82.0, 40.0),
    puff(Right, 10.0, 42.0, 118.0, 98.0, 0.26, 80.0, -46.0, 56.0),
    puff(Right, -10.0, 68.0, 82.0, 82.0, 0.46, 54.0, -62.0, 34.0),
];

impl Puff {
    /// puffLeft: the puff's left edge in a layer `width` wide whose bar starts `bar_x` in from each side
    pub fn left(&self, width: f64, bar_x: f64) -> f64 {
        match self.at {
            PuffAt::Left => bar_x + self.x,
            PuffAt::Right => width - bar_x + self.x,
            PuffAt::Frac(f) => bar_x + (width - 2.0 * bar_x) * f + self.x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // fourTwenty.test.ts, sixtyNine.test.ts, sixtySeven.test.ts: the same cases for each number
    fn run(target: f64, answers: &[Option<f64>]) -> usize {
        let mut armed = true;
        let mut fired = 0;
        for &n in answers {
            let (a, f) = gate(target, armed, n);
            armed = a;
            if f {
                fired += 1;
            }
        }
        fired
    }

    #[test]
    fn gates_fire_once_and_rearm_after_a_change() {
        for (t, near, far) in [
            (420.0, 42.0, 4200.0),
            (69.0, 6.9, 690.0),
            (67.0, 6.7, 670.0),
        ] {
            assert_eq!(run(t, &[Some(near), Some(t)]), 1);
            assert_eq!(run(t, &[Some(t), Some(t), Some(t)]), 1);
            assert_eq!(run(t, &[Some(t), None, Some(t)]), 2);
            assert_eq!(run(t, &[Some(t), Some(far), Some(t)]), 2);
            assert_eq!(run(t, &[Some(near), Some(far), Some(near), None]), 0);
        }
    }

    #[test]
    fn the_three_gates_run_side_by_side() {
        let mut g = EggGates::default();
        assert_eq!(g.settle(Some(420.0)), Some(Egg::Smoke));
        assert_eq!(g.settle(Some(420.0)), None);
        assert_eq!(g.settle(Some(69.0)), Some(Egg::Fold));
        assert_eq!(g.settle(Some(67.0)), Some(Egg::Arms));
        assert_eq!(g.settle(Some(420.0)), Some(Egg::Smoke));
        assert_eq!(g.settle(None), None);
    }

    // smokeRoom.test.ts
    #[test]
    fn wide_room_needs_a_native_host_that_says_so() {
        assert!(wide_smoke_room(true, true));
        assert!(!wide_smoke_room(true, false));
        assert!(!wide_smoke_room(false, true));
        assert!(!wide_smoke_room(false, false));
        assert_eq!(
            SMOKE_ROOM,
            SmokeRoom {
                top: 160.0,
                side: 100.0,
                bottom: 64.0
            }
        );
    }

    #[test]
    fn puffs_sit_along_the_bar() {
        assert_eq!(PUFFS[12].left(600.0, 100.0), 116.0);
        assert_eq!(PUFFS[13].left(600.0, 100.0), 484.0);
        assert!((PUFFS[0].left(600.0, 100.0) - 132.0).abs() < 1e-9);
    }
}
