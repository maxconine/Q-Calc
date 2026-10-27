//! `src/lib/gameClock.ts`: the clock games run on, `performance.now` and `Date.now` with the paused stretches
//! taken out. pausing a game stops this clock, so whatever reads it (frame loops, countdowns, the computer's
//! thinking time) stands still without each game knowing about pauses. The TS reads its two clocks itself; here
//! the caller hands them in (`mono`: monotonic ms, `wall`: ms since the epoch).

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameClock {
    stopped_at: Option<f64>,
    lost: f64,
}

impl GameClock {
    pub fn new() -> GameClock {
        GameClock::default()
    }

    pub fn paused(&self) -> bool {
        self.stopped_at.is_some()
    }

    /// `performance.now` on this clock.
    pub fn now(&self, mono: f64) -> f64 {
        self.stopped_at.unwrap_or(mono) - self.lost
    }

    /// `Date.now` on this clock, for games that count down in wall time.
    pub fn date(&self, mono: f64, wall: f64) -> f64 {
        wall - self.stopped_at.map_or(0.0, |at| mono - at) - self.lost
    }

    pub fn pause(&mut self, mono: f64) {
        if self.stopped_at.is_none() {
            self.stopped_at = Some(mono);
        }
    }

    pub fn resume(&mut self, mono: f64) {
        let Some(at) = self.stopped_at else { return };
        self.lost += mono - at;
        self.stopped_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stands_still_while_paused_and_carries_on_from_there() {
        let mut mono = 1000.0;
        let mut wall = 1_700_000_000_000.0;
        let mut clock = GameClock::new();
        assert_eq!(clock.now(mono), 1000.0);
        clock.pause(mono);
        mono += 5000.0;
        wall += 5000.0;
        assert_eq!(clock.now(mono), 1000.0);
        assert_eq!(clock.date(mono, wall), 1_700_000_000_000.0);
        clock.resume(mono);
        mono += 10.0;
        wall += 10.0;
        assert_eq!(clock.now(mono), 1010.0);
        assert_eq!(clock.date(mono, wall), 1_700_000_000_010.0);
        assert!(!clock.paused());
    }
}
