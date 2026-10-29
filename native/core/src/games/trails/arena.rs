//! What the trails panel decided beside the drawing (`src/components/TrailsPanel.tsx`): the offline match against
//! the computer or a second player on the same keyboard (the clock, the count, the rounds, the score), which keys
//! steer whom, and the lines the arena shows. The online match is session.rs; both end in an ArenaView.

use super::net::{COUNT_MS, POINT_MS};
use super::session::{TrailsPhase, TrailsView};
use super::{
    Dir, Level, Player, Round, TICK_MS, WIN_ROUNDS, bot_turn, match_winner, new_round, queue_turn,
    score_round, steer, step,
};

/// who the left cycle plays: the computer at a level, or a friend on the same keyboard
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vs {
    Computer(Level),
    Friend,
}

/// the shared lobby's choices, in its order: the computer at each level, then two players
pub const VS: [Vs; 4] = [
    Vs::Computer(Level::Easy),
    Vs::Computer(Level::Normal),
    Vs::Computer(Level::Hard),
    Vs::Friend,
];

/// a key the arena takes: WASD or the arrows (a direction), enter, backspace
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaKey {
    Wasd(Dir),
    Arrow(Dir),
    Enter,
    Back,
}

/// what the arena draws and says this frame
#[derive(Clone, Debug, PartialEq)]
pub struct ArenaView {
    pub round: Round,
    /// how far the heads are into the next cell, 0–1
    pub t: f64,
    /// how long ago the round ended
    pub crash_ms: Option<f64>,
    pub score: [u32; 2],
    pub names: [String; 2],
    /// the line over the arena; big: the count
    pub banner: String,
    pub big: bool,
    pub note: String,
    /// online: the line the lobby shows once the game is over for good
    pub ended: Option<String>,
    /// backspace: back to the lobby's menu (the online side has already said bye)
    pub to_menu: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Count,
    Run,
    Point,
    Over,
}

/// the offline match: TrailsGame's frame loop and key handler
#[derive(Debug)]
pub struct LocalArena {
    vs: Vs,
    round: Round,
    score: [u32; 2],
    phase: Phase,
    phase_at: f64,
    last: f64,
    acc: f64,
    last_winner: Option<Player>,
    to_menu: bool,
    rand: Xorshift,
}

/// Math.random's stand-in for the computer's careless turns
#[derive(Clone, Copy, Debug)]
pub struct Xorshift(u64);

impl Xorshift {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    pub fn draw(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl LocalArena {
    pub fn new(vs: Vs, now: f64, seed: u64) -> Self {
        Self {
            vs,
            round: new_round(),
            score: [0, 0],
            phase: Phase::Count,
            phase_at: now,
            last: now,
            acc: 0.0,
            last_winner: None,
            to_menu: false,
            rand: Xorshift::new(seed),
        }
    }

    fn restart(&mut self, now: f64) {
        self.round = new_round();
        self.score = [0, 0];
        self.last_winner = None;
        self.phase = Phase::Count;
        self.phase_at = now;
    }

    /// a key down (not a repeat for enter); `now` is the game clock
    pub fn key(&mut self, key: ArenaKey, repeat: bool, now: f64) {
        match key {
            ArenaKey::Back => self.to_menu = true,
            ArenaKey::Enter => {
                if self.phase == Phase::Over && !repeat {
                    self.restart(now);
                }
            }
            ArenaKey::Wasd(d) | ArenaKey::Arrow(d) => {
                if !matches!(self.phase, Phase::Count | Phase::Run) {
                    return;
                }
                // vs the computer, either set steers you; two players: WASD is the left cycle, arrows the right
                let who: Player = if self.vs != Vs::Friend || matches!(key, ArenaKey::Wasd(_)) {
                    0
                } else {
                    1
                };
                self.round = queue_turn(&self.round, who, d);
            }
        }
    }

    pub fn tick(&mut self, now: f64) -> ArenaView {
        if self.phase == Phase::Count && now - self.phase_at >= COUNT_MS * 3.0 {
            self.phase = Phase::Run;
            self.last = now;
            self.acc = 0.0;
        }
        if self.phase == Phase::Run {
            let mut dt = now - self.last;
            self.last = now;
            // the window was hidden: pick up where it left off rather than catch up
            if dt > 250.0 {
                dt = 0.0;
            }
            self.acc += dt;
            while self.acc >= TICK_MS && self.phase == Phase::Run {
                self.acc -= TICK_MS;
                if let Vs::Computer(level) = self.vs {
                    let mut r = || self.rand.draw();
                    let d = bot_turn(&self.round, 1, level, &mut r);
                    self.round = steer(&self.round, 1, d);
                }
                self.round = step(&self.round);
                if let Some(res) = self.round.result {
                    self.score = score_round(self.score, res);
                    self.last_winner = res.winner;
                    self.phase = if match_winner(self.score).is_some() {
                        Phase::Over
                    } else {
                        Phase::Point
                    };
                    self.phase_at = now;
                }
            }
        }
        if self.phase == Phase::Point && now - self.phase_at >= POINT_MS {
            self.round = new_round();
            self.phase = Phase::Count;
            self.phase_at = now;
        }
        let ended = matches!(self.phase, Phase::Point | Phase::Over);
        let count = if self.phase == Phase::Count {
            3 - ((now - self.phase_at).max(0.0) / COUNT_MS).floor() as i64
        } else {
            0
        };
        let solo = self.vs != Vs::Friend;
        let names: [String; 2] = if solo {
            ["you".into(), "computer".into()]
        } else {
            ["WASD".into(), "arrows".into()]
        };
        let takes = |p: Player| {
            if solo {
                if p == 0 {
                    "you take the round".to_string()
                } else {
                    "the computer takes it".to_string()
                }
            } else {
                format!(
                    "{} take{} the round",
                    names[p],
                    if p == 0 { "s" } else { "" }
                )
            }
        };
        let mut banner = String::new();
        let mut big = false;
        match self.phase {
            Phase::Count => {
                banner = count.max(1).to_string();
                big = true;
            }
            Phase::Point => banner = self.last_winner.map_or("a draw · no point".into(), takes),
            Phase::Over => {
                if let Some(w) = match_winner(self.score) {
                    let who = if solo {
                        if w == 0 {
                            "you win".to_string()
                        } else {
                            "the computer wins".to_string()
                        }
                    } else {
                        format!("{} win{}", names[w], if w == 0 { "s" } else { "" })
                    };
                    banner = format!("{who} {}–{} · ↵ again", self.score[0], self.score[1]);
                }
            }
            Phase::Run => {}
        }
        let note = format!(
            "{} · first to {WIN_ROUNDS} · ⌫ menu · esc leaves",
            if solo {
                "WASD or arrows to turn"
            } else {
                "left: WASD · right: arrows"
            }
        );
        ArenaView {
            round: self.round.clone(),
            t: if self.phase == Phase::Run {
                (self.acc / TICK_MS).min(1.0)
            } else {
                0.0
            },
            crash_ms: if ended {
                Some(now - self.phase_at)
            } else {
                None
            },
            score: self.score,
            names,
            banner,
            big,
            note,
            ended: None,
            to_menu: std::mem::take(&mut self.to_menu),
        }
    }

    /// the panel's head line: 'one keyboard' or 'vs the computer · hard'
    pub fn sub(&self) -> String {
        match self.vs {
            Vs::Friend => "one keyboard".into(),
            Vs::Computer(l) => format!("vs the computer · {}", l.name()),
        }
    }
}

/// TrailsOnline's lines over a session's view. me: 0 host (left), 1 guest
pub fn online_view(v: TrailsView, me: Player, peer: &str) -> ArenaView {
    let names: [String; 2] = if me == 0 {
        ["you".into(), peer.into()]
    } else {
        [peer.into(), "you".into()]
    };
    let mut banner = String::new();
    let mut big = false;
    if v.phase == TrailsPhase::Wait || v.stalled {
        banner = format!("waiting for {peer}…");
    } else if v.phase == TrailsPhase::Count {
        banner = v.count.max(1).to_string();
        big = true;
    } else if v.phase == TrailsPhase::Point {
        banner = match v.last {
            None => "a draw · no point".into(),
            Some(p) if p == me => "you take the round".into(),
            Some(_) => format!("{peer} takes the round"),
        };
    } else if v.phase == TrailsPhase::Over
        && let Some(w) = v.winner
    {
        let who = if w == me {
            "you win".to_string()
        } else {
            format!("{peer} wins")
        };
        let ask = if v.again.mine {
            "waiting for a rematch…".to_string()
        } else if v.again.theirs {
            format!("{peer} wants a rematch · ↵")
        } else {
            "↵ rematch".to_string()
        };
        banner = format!("{who} {}–{} · {ask}", v.score[0], v.score[1]);
    }
    ArenaView {
        round: v.round,
        t: v.t,
        crash_ms: v.crash_ms,
        score: v.score,
        names,
        banner,
        big,
        note: format!(
            "you’re {} · WASD or arrows to turn · first to {WIN_ROUNDS} · ⌫ leaves",
            if me == 0 { "left" } else { "right" }
        ),
        ended: v.ended,
        to_menu: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_in_runs_and_scores() {
        let mut a = LocalArena::new(Vs::Computer(Level::Hard), 0.0, 7);
        let v = a.tick(0.0);
        assert_eq!((v.banner.as_str(), v.big), ("3", true));
        assert_eq!(a.tick(600.0).banner, "2");
        assert_eq!(a.tick(1200.0).banner, "1");
        let v = a.tick(1700.0);
        assert_eq!(v.banner, "");
        // you drive straight into the far wall; the computer doesn't
        let mut now = 1700.0;
        let mut v = a.tick(now);
        while v.crash_ms.is_none() {
            now += 16.0;
            v = a.tick(now);
        }
        assert_eq!(v.score, [0, 1]);
        assert_eq!(v.banner, "the computer takes it");
        assert_eq!(a.sub(), "vs the computer · hard");
    }

    #[test]
    fn friends_steer_their_own_cycles() {
        let mut a = LocalArena::new(Vs::Friend, 0.0, 1);
        a.key(ArenaKey::Arrow(0), false, 0.0);
        a.key(ArenaKey::Wasd(2), false, 0.0);
        let v = a.tick(0.0);
        assert_eq!(v.round.cycles[1].queue, vec![0]);
        assert_eq!(v.round.cycles[0].queue, vec![2]);
        assert_eq!(v.names, ["WASD".to_string(), "arrows".to_string()]);
        a.key(ArenaKey::Back, false, 0.0);
        assert!(a.tick(1.0).to_menu);
    }
}
