//! `src/lib/ultimateSession.ts`: one side of a game, without the page. The board feeds it cells, messages and
//! times; what it sends the other side waits in `take_sent`.

use super::ultimate::{UT_VERSION, UtIn, UtMsg, UtState, can_play, decode_ut, encode_ut, first_for, is_over, new_game, play};
use super::ultimate_bot::{UtSkill, bot_cell};
pub use super::connect4_session::{Again, Clock};
use super::hello::{LinkedGame, other_game_note};
use super::{Random, Side};

// hi goes out this often until the other side's hi comes in, since its board may not have been listening yet
const HI_MS: f64 = 400.0;
/// A turn can take a while, so both sides say they're still there this often.
pub const KEEP_MS: f64 = 1000.0;
/// Nothing from the other side for this long and the game is over.
pub const UT_SILENCE_MS: f64 = 5000.0;
/// Solo: how long the computer seems to think before it moves.
pub const BOT_THINK_MS: f64 = 500.0;

#[derive(Clone, Debug, PartialEq)]
pub struct UtView {
    pub state: UtState,
    /// The other side's hi came in, so moves can go.
    pub started: bool,
    /// Games won so far, host then guest.
    pub wins: [u32; 2],
    pub again: Again,
    /// Set once the game is over for good: the line the lobby shows.
    pub ended: Option<String>,
    /// Bumps on every change, so the board knows when to redraw.
    pub rev: u64,
    /// Solo: the computer is about to move.
    pub thinking: bool,
}

pub struct UltimateSession {
    game: u64,
    state: UtState,
    wins: [u32; 2],
    greeted: bool,
    heard: f64,
    last_hi: f64,
    last_sent: f64,
    again: Again,
    ended: Option<String>,
    rev: u64,
    pub role: Side,
    /// A game against the computer: no link, the computer plays the guest.
    pub solo: Option<UtSkill>,
    // solo: when the computer moves next, once it's its turn
    bot_at: Option<f64>,
    sent: Vec<String>,
    rand: Box<dyn FnMut() -> f64 + Send>,
    clock: Clock,
}

impl std::fmt::Debug for UltimateSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UltimateSession").field("role", &self.role).field("state", &self.state).finish_non_exhaustive()
    }
}

impl UltimateSession {
    pub fn new(role: Side, now: f64, solo: Option<UtSkill>, rand: Box<dyn FnMut() -> f64 + Send>, clock: Clock) -> UltimateSession {
        let mut s = UltimateSession {
            game: 0,
            state: new_game(first_for(0)),
            wins: [0, 0],
            greeted: false,
            heard: now,
            last_hi: now,
            last_sent: now,
            again: Again::default(),
            ended: None,
            rev: 0,
            role: if solo.is_some() { Side::Host } else { role },
            solo,
            bot_at: None,
            sent: vec![],
            rand,
            clock,
        };
        if solo.is_some() {
            s.greeted = true;
            s.bot_turn(now);
        } else {
            s.hi();
        }
        s
    }

    /// A session with `Math.random` and a monotonic clock.
    pub fn with_defaults(role: Side, now: f64, solo: Option<UtSkill>) -> UltimateSession {
        let mut r = Random::from_entropy();
        let t0 = std::time::Instant::now();
        UltimateSession::new(role, now, solo, Box::new(move || r.next_f64()), Box::new(move || t0.elapsed().as_secs_f64() * 1000.0))
    }

    /// What's waiting to go to the other side, oldest first.
    pub fn take_sent(&mut self) -> Vec<String> {
        std::mem::take(&mut self.sent)
    }

    // solo: if it's the computer's turn, it moves after a moment
    fn bot_turn(&mut self, now: f64) {
        self.bot_at = if self.solo.is_some() && !is_over(&self.state) && self.state.turn != self.role { Some(now + BOT_THINK_MS) } else { None };
    }

    fn hi(&mut self) {
        self.post(&UtMsg::Hi { v: UT_VERSION, g: true, ok: self.greeted });
    }

    fn post(&mut self, m: &UtMsg) {
        self.sent.push(encode_ut(m));
    }

    fn changed(&mut self) {
        self.rev += 1;
    }

    fn end(&mut self, note: String) {
        if self.ended.is_some() {
            return;
        }
        self.ended = Some(note);
        self.changed();
    }

    fn apply(&mut self, cell: usize, who: Side) -> bool {
        let Some(next) = play(&self.state, cell as f64, who) else { return false };
        if let Some(w) = next.winner {
            self.wins[if w == Side::Host { 0 } else { 1 }] += 1;
        }
        self.state = next;
        self.changed();
        true
    }

    fn restart(&mut self, now: f64) {
        self.game += 1;
        self.state = new_game(first_for(self.game));
        self.again = Again::default();
        self.bot_turn(now);
        self.changed();
    }

    /// My turn, an open cell, the other side listening: takes it and sends it.
    pub fn play(&mut self, cell: usize, now: f64) -> bool {
        if self.ended.is_some() || !self.greeted || !can_play(&self.state, cell as f64, self.role) {
            return false;
        }
        let n = self.state.moves;
        self.apply(cell, self.role);
        if self.solo.is_some() {
            self.bot_turn(now);
            return true;
        }
        self.post(&UtMsg::Move { g: self.game, n, c: cell });
        self.last_sent = now;
        true
    }

    /// Enter: asks for a rematch once the game is over; both sides restart when both have asked.
    pub fn rematch(&mut self, now: f64) {
        if self.ended.is_some() || !is_over(&self.state) || self.again.mine {
            return;
        }
        // the computer always wants another
        if self.solo.is_some() {
            self.restart(now);
            return;
        }
        self.again.mine = true;
        self.post(&UtMsg::Again { g: self.game });
        self.last_sent = now;
        if self.again.theirs {
            self.restart(0.0);
        } else {
            self.changed();
        }
    }

    pub fn receive(&mut self, text: &str, now: f64) {
        if self.ended.is_some() {
            return;
        }
        let Some(m) = decode_ut(text) else { return };
        self.heard = now;
        match m {
            UtIn::Msg(UtMsg::Hi { v, ok, .. }) => {
                if v != UT_VERSION {
                    self.post(&UtMsg::Bye);
                    self.end("update both Q Calcs to play each other".into());
                    return;
                }
                if !self.greeted {
                    self.greeted = true;
                    self.changed();
                }
                if !ok {
                    self.hi();
                }
            }
            UtIn::Other(game) => {
                self.post(&UtMsg::Bye);
                self.end(other_game_note(&game, LinkedGame::Ultimate));
            }
            UtIn::Msg(UtMsg::Move { g, n, c }) => {
                // only the next move of this game, from the side whose turn it is; anything else is stale or made up
                if !self.greeted || g != self.game || n != self.state.moves {
                    return;
                }
                self.apply(c, self.role.other());
            }
            UtIn::Msg(UtMsg::Again { g }) => {
                // only an ask for this game, made at its end, counts
                if g != self.game || !is_over(&self.state) || self.again.theirs {
                    return;
                }
                self.again.theirs = true;
                if self.again.mine {
                    self.restart(0.0);
                } else {
                    self.changed();
                }
            }
            UtIn::Msg(UtMsg::Keep) => {}
            UtIn::Msg(UtMsg::Bye) => self.end("your friend left".into()),
        }
    }

    /// Resends hi until it's answered, keeps the link warm, and notices when the other side has gone quiet.
    pub fn tick(&mut self, now: f64) -> UtView {
        if self.ended.is_some() {
            return self.view();
        }
        if let Some(skill) = self.solo {
            if self.bot_at.is_some_and(|at| now >= at) {
                self.bot_at = None;
                let me = self.role.other();
                let cell = bot_cell(&self.state, me, skill, &mut *self.rand, &mut *self.clock);
                if let Some(cell) = cell {
                    self.apply(cell, me);
                }
            }
            return self.view();
        }
        if now - self.heard > UT_SILENCE_MS {
            self.end("lost the connection".into());
            return self.view();
        }
        if !self.greeted && now - self.last_hi >= HI_MS {
            self.last_hi = now;
            self.last_sent = now;
            self.hi();
        } else if now - self.last_sent >= KEEP_MS {
            self.last_sent = now;
            self.post(&UtMsg::Keep);
        }
        self.view()
    }

    pub fn view(&self) -> UtView {
        UtView {
            state: self.state.clone(),
            started: self.greeted,
            thinking: self.bot_at.is_some(),
            wins: self.wins,
            again: self.again,
            ended: self.ended.clone(),
            rev: self.rev,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::ultimate::cell_in;

    fn session(role: Side, solo: Option<UtSkill>) -> UltimateSession {
        let mut t = 0.0;
        let mut r = crate::games::mulberry32(9);
        UltimateSession::new(role, 0.0, solo, Box::new(move || r()), Box::new(move || {
            t += 20.0;
            t
        }))
    }

    fn flush(host: &mut UltimateSession, guest: &mut UltimateSession, now: f64) {
        loop {
            let to_guest = host.take_sent();
            let to_host = guest.take_sent();
            if to_guest.is_empty() && to_host.is_empty() {
                break;
            }
            for t in to_guest {
                guest.receive(&t, now);
            }
            for t in to_host {
                host.receive(&t, now);
            }
        }
    }

    #[test]
    fn hellos_then_moves_reach_the_other_board() {
        let (mut host, mut guest) = (session(Side::Host, None), session(Side::Guest, None));
        assert!(!host.view().started);
        flush(&mut host, &mut guest, 1.0);
        assert!(host.view().started && guest.view().started);
        assert!(!guest.play(cell_in(0, 0), 2.0));
        assert!(host.play(cell_in(4, 2), 2.0));
        flush(&mut host, &mut guest, 3.0);
        assert_eq!(guest.view().state.cells[cell_in(4, 2)], Some(Side::Host));
        assert_eq!(guest.view().state.forced, Some(2));
        assert!(guest.play(cell_in(2, 4), 4.0));
        flush(&mut host, &mut guest, 5.0);
        assert_eq!((host.view().state.moves, host.view().state.forced), (2, Some(4)));
    }

    #[test]
    fn ignores_stale_or_made_up_moves() {
        let (mut host, mut guest) = (session(Side::Host, None), session(Side::Guest, None));
        flush(&mut host, &mut guest, 1.0);
        for m in [UtMsg::Move { g: 0, n: 0, c: 5 }, UtMsg::Move { g: 0, n: 3, c: 5 }, UtMsg::Move { g: 1, n: 0, c: 5 }] {
            host.receive(&encode_ut(&m), 2.0);
        }
        assert_eq!(host.view().state.moves, 0);
    }

    #[test]
    fn ends_on_another_game_or_silence() {
        let mut a = session(Side::Host, None);
        a.receive(r#"{"t":"c4","v":1}"#, 1.0);
        assert!(a.view().ended.unwrap().contains("connect 4"));
        let mut b = session(Side::Host, None);
        assert_eq!(b.tick(UT_SILENCE_MS + 1.0).ended.as_deref(), Some("lost the connection"));
    }

    #[test]
    fn solo() {
        let mut s = session(Side::Guest, Some(UtSkill::Easy));
        assert_eq!(s.role, Side::Host);
        assert!(s.play(cell_in(4, 4), 0.0));
        assert_eq!(s.tick(BOT_THINK_MS - 1.0).state.moves, 1);
        let v = s.tick(BOT_THINK_MS);
        assert_eq!(v.state.moves, 2);
        assert_eq!(v.state.cells[36..45].iter().filter(|c| c.is_some()).count(), 2);
        assert!(!v.thinking);
    }
}
