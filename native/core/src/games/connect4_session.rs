//! `src/lib/connect4Session.ts`: one side of a game, without the page. The board feeds it columns, messages and
//! times; what it sends the other side waits in `take_sent`.

use super::connect4::{
    C4_VERSION, C4In, C4Msg, C4State, can_drop, decode_c4, drop, encode_c4, first_for, is_over,
    new_board,
};
use super::connect4_bot::{C4Skill, bot_column};
use super::hello::{LinkedGame, other_game_note};
use super::{Random, Side};

// hi goes out this often until the other side's hi comes in, since its board may not have been listening yet
const HI_MS: f64 = 400.0;
/// A turn can take a while, so both sides say they're still there this often.
pub const KEEP_MS: f64 = 1000.0;
/// Nothing from the other side for this long and the game is over.
pub const C4_SILENCE_MS: f64 = 5000.0;
/// Solo: how long the computer seems to think before it drops.
pub const BOT_THINK_MS: f64 = 500.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Again {
    pub mine: bool,
    pub theirs: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct C4View {
    pub state: C4State,
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

/// A clock for the computer's search (`performance.now`).
pub type Clock = Box<dyn FnMut() -> f64 + Send>;

pub struct Connect4Session {
    game: u64,
    state: C4State,
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
    pub solo: Option<C4Skill>,
    // solo: when the computer drops next, once it's its turn
    bot_at: Option<f64>,
    sent: Vec<String>,
    rand: Box<dyn FnMut() -> f64 + Send>,
    clock: Clock,
}

impl std::fmt::Debug for Connect4Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connect4Session")
            .field("role", &self.role)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Connect4Session {
    pub fn new(
        role: Side,
        now: f64,
        solo: Option<C4Skill>,
        rand: Box<dyn FnMut() -> f64 + Send>,
        clock: Clock,
    ) -> Connect4Session {
        let mut s = Connect4Session {
            game: 0,
            state: new_board(first_for(0)),
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
    pub fn with_defaults(role: Side, now: f64, solo: Option<C4Skill>) -> Connect4Session {
        let mut r = Random::from_entropy();
        let t0 = std::time::Instant::now();
        Connect4Session::new(
            role,
            now,
            solo,
            Box::new(move || r.next_f64()),
            Box::new(move || t0.elapsed().as_secs_f64() * 1000.0),
        )
    }

    /// What's waiting to go to the other side, oldest first.
    pub fn take_sent(&mut self) -> Vec<String> {
        std::mem::take(&mut self.sent)
    }

    // solo: if it's the computer's turn, it moves after a moment
    fn bot_turn(&mut self, now: f64) {
        self.bot_at =
            if self.solo.is_some() && !is_over(&self.state) && self.state.turn != self.role {
                Some(now + BOT_THINK_MS)
            } else {
                None
            };
    }

    fn hi(&mut self) {
        self.post(&C4Msg::Hi {
            v: C4_VERSION,
            g: true,
            ok: self.greeted,
        });
    }

    fn post(&mut self, m: &C4Msg) {
        self.sent.push(encode_c4(m));
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

    fn apply(&mut self, col: usize, who: Side) -> bool {
        let Some(next) = drop(&self.state, col as f64, who) else {
            return false;
        };
        if let Some(w) = next.winner {
            self.wins[if w == Side::Host { 0 } else { 1 }] += 1;
        }
        self.state = next;
        self.changed();
        true
    }

    fn restart(&mut self, now: f64) {
        self.game += 1;
        self.state = new_board(first_for(self.game));
        self.again = Again::default();
        self.bot_turn(now);
        self.changed();
    }

    /// My turn, a free column, the other side listening: drops and sends it.
    pub fn play(&mut self, col: usize, now: f64) -> bool {
        if self.ended.is_some() || !self.greeted || !can_drop(&self.state, col as f64, self.role) {
            return false;
        }
        let n = self.state.moves;
        self.apply(col, self.role);
        if self.solo.is_some() {
            self.bot_turn(now);
            return true;
        }
        self.post(&C4Msg::Move {
            g: self.game,
            n,
            c: col,
        });
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
        self.post(&C4Msg::Again { g: self.game });
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
        let Some(m) = decode_c4(text) else { return };
        self.heard = now;
        match m {
            C4In::Msg(C4Msg::Hi { v, ok, .. }) => {
                if v != C4_VERSION {
                    self.post(&C4Msg::Bye);
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
            C4In::Other(game) => {
                self.post(&C4Msg::Bye);
                self.end(other_game_note(&game, LinkedGame::Connect4));
            }
            C4In::Msg(C4Msg::Move { g, n, c }) => {
                // only the next move of this game, from the side whose turn it is; anything else is stale or made up
                if !self.greeted || g != self.game || n != self.state.moves {
                    return;
                }
                self.apply(c, self.role.other());
            }
            C4In::Msg(C4Msg::Again { g }) => {
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
            C4In::Msg(C4Msg::Keep) => {}
            C4In::Msg(C4Msg::Bye) => self.end("your friend left".into()),
        }
    }

    /// Resends hi until it's answered, keeps the link warm, and notices when the other side has gone quiet.
    pub fn tick(&mut self, now: f64) -> C4View {
        if self.ended.is_some() {
            return self.view();
        }
        if let Some(skill) = self.solo {
            if self.bot_at.is_some_and(|at| now >= at) {
                self.bot_at = None;
                let me = self.role.other();
                let col = bot_column(&self.state, me, skill, &mut *self.rand, &mut *self.clock);
                if let Some(col) = col {
                    self.apply(col, me);
                }
            }
            return self.view();
        }
        if now - self.heard > C4_SILENCE_MS {
            self.end("lost the connection".into());
            return self.view();
        }
        if !self.greeted && now - self.last_hi >= HI_MS {
            self.last_hi = now;
            self.last_sent = now;
            self.hi();
        } else if now - self.last_sent >= KEEP_MS {
            self.last_sent = now;
            self.post(&C4Msg::Keep);
        }
        self.view()
    }

    pub fn view(&self) -> C4View {
        C4View {
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
    use crate::games::connect4::{cell_at, tests::DRAW};
    use crate::games::mulberry32;

    fn session(role: Side, now: f64, solo: Option<C4Skill>, seed: u32) -> Connect4Session {
        let r = mulberry32(seed);
        let mut t = 0.0;
        Connect4Session::new(
            role,
            now,
            solo,
            Box::new(r),
            Box::new(move || {
                t += 40.0;
                t
            }),
        )
    }

    // two sessions joined by a queue that delivers after `lag` ms
    struct Pair {
        host: Connect4Session,
        guest: Connect4Session,
        now: f64,
        lag: f64,
        queue: Vec<(f64, Side, String)>,
        hv: C4View,
        gv: C4View,
        guest_version: Option<f64>,
        drop_first_hi_to_host: bool,
    }

    impl Pair {
        fn new(lag: f64) -> Pair {
            let now = 1000.0;
            let mut host = session(Side::Host, now, None, 1);
            let mut guest = session(Side::Guest, now, None, 2);
            let hv = host.tick(now);
            let gv = guest.tick(now);
            let mut p = Pair {
                host,
                guest,
                now,
                lag,
                queue: vec![],
                hv,
                gv,
                guest_version: None,
                drop_first_hi_to_host: false,
            };
            p.route();
            p
        }
        fn route(&mut self) {
            for text in self.host.take_sent() {
                self.queue.push((self.now + self.lag, Side::Guest, text));
            }
            for mut text in self.guest.take_sent() {
                if let Some(v) = self.guest_version {
                    text = text.replace("\"v\":1", &format!("\"v\":{v}"));
                }
                if self.drop_first_hi_to_host && text.contains("\"c4\"") {
                    self.drop_first_hi_to_host = false;
                    continue;
                }
                self.queue.push((self.now + self.lag, Side::Host, text));
            }
        }
        fn run(&mut self, ms: f64, cut: bool) {
            let end = self.now + ms;
            while self.now < end {
                self.now += 50.0;
                if cut {
                    self.queue.clear();
                }
                let now = self.now;
                let (due, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queue)
                    .into_iter()
                    .partition(|m| m.0 <= now);
                self.queue = rest;
                for (_, to, text) in due {
                    if to == Side::Host {
                        self.host.receive(&text, now)
                    } else {
                        self.guest.receive(&text, now)
                    }
                }
                self.hv = self.host.tick(now);
                self.gv = self.guest.tick(now);
                self.route();
                if cut {
                    self.queue.clear();
                }
            }
        }
        fn moves(&mut self, cols: &[usize]) {
            for &c in cols {
                let turn = self.host.view().state.turn;
                let ok = if turn == Side::Host {
                    self.host.play(c, self.now)
                } else {
                    self.guest.play(c, self.now)
                };
                assert!(ok, "column {c}");
                self.route();
                self.run(self.lag + 100.0, false);
            }
        }
    }

    #[test]
    fn waits_for_both_hellos_even_when_one_is_lost() {
        let mut g = Pair::new(500.0);
        assert!(!g.host.play(3, g.now));
        g.run(1200.0, false);
        assert!(g.hv.started && g.gv.started);
        let mut g = Pair::new(5.0);
        g.drop_first_hi_to_host = true;
        g.run(1000.0, false);
        assert!(g.hv.started && g.gv.started);
    }

    #[test]
    fn moves_show_on_both_boards() {
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        assert!(!g.guest.play(0, g.now));
        assert!(g.host.play(3, g.now));
        assert!(!g.host.play(3, g.now));
        g.route();
        g.run(100.0, false);
        assert_eq!(g.gv.state.board[cell_at(3, 0)], Some(Side::Host));
        assert_eq!(g.gv.state.turn, Side::Guest);
        assert!(g.guest.play(3, g.now));
        g.route();
        g.run(100.0, false);
        assert_eq!(g.hv.state.board, g.gv.state.board);
        assert_eq!(g.hv.state.turn, Side::Host);
    }

    #[test]
    fn rejects_bad_moves_from_the_peer() {
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        let before = g.host.view();
        let now = g.now;
        for m in [
            C4Msg::Move { g: 0, n: 0, c: 2 },
            C4Msg::Move { g: 1, n: 0, c: 2 },
            C4Msg::Move { g: 0, n: 3, c: 2 },
        ] {
            g.host.receive(&encode_c4(&m), now);
        }
        g.host.receive(r#"{"t":"c4m","g":0,"n":0,"c":9}"#, now);
        assert_eq!(g.host.view().state, before.state);
        assert_eq!(g.host.view().rev, before.rev);
        g.moves(&[0, 0, 0, 0, 0, 0]);
        assert_eq!(g.host.view().state.turn, Side::Host);
        assert!(!g.host.play(0, g.now));
        g.host.play(1, g.now);
        g.host
            .receive(&encode_c4(&C4Msg::Move { g: 0, n: 7, c: 0 }), g.now);
        assert_eq!(g.host.view().state.moves, 7);
        assert_eq!(g.host.view().state.turn, Side::Guest);
        g.host
            .receive(&encode_c4(&C4Msg::Move { g: 0, n: 1, c: 4 }), g.now);
        assert_eq!(g.host.view().state.moves, 7);
    }

    #[test]
    fn wins_count_and_rematches_swap_who_starts() {
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        g.moves(&[0, 0, 1, 1, 2, 2, 3]);
        assert_eq!(g.hv.state.winner, Some(Side::Host));
        assert_eq!(g.gv.state.winner, Some(Side::Host));
        assert_eq!(g.gv.state.line, g.hv.state.line);
        assert_eq!((g.hv.wins, g.gv.wins), ([1, 0], [1, 0]));
        g.guest.rematch(g.now);
        g.route();
        g.run(100.0, false);
        assert_eq!(
            g.hv.again,
            Again {
                mine: false,
                theirs: true
            }
        );
        g.host.rematch(g.now);
        g.route();
        g.run(100.0, false);
        for v in [&g.hv, &g.gv] {
            assert_eq!(
                (v.state.moves, v.state.turn, v.again, v.wins),
                (0, Side::Guest, Again::default(), [1, 0])
            );
        }
        assert!(!g.host.play(3, g.now));
        assert!(g.guest.play(3, g.now));
        g.route();
        g.run(100.0, false);
        assert_eq!(g.hv.state.board[cell_at(3, 0)], Some(Side::Guest));

        let mut g = Pair::new(200.0);
        g.run(800.0, false);
        g.moves(&[0, 0, 1, 1, 2, 2, 3]);
        g.run(500.0, false);
        g.host.rematch(g.now);
        g.guest.rematch(g.now);
        g.route();
        g.run(500.0, false);
        assert_eq!((g.hv.state.moves, g.gv.state.moves), (0, 0));
        assert_eq!(
            (g.hv.state.turn, g.gv.state.turn),
            (Side::Guest, Side::Guest)
        );
    }

    #[test]
    fn draws_and_early_asks() {
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        g.moves(&DRAW);
        assert!(g.hv.state.draw && g.gv.state.draw);
        assert_eq!(g.hv.wins, [0, 0]);
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        g.guest.rematch(g.now);
        g.host.receive(&encode_c4(&C4Msg::Again { g: 0 }), g.now);
        g.route();
        g.run(100.0, false);
        assert!(!g.hv.again.theirs);
        g.moves(&[0, 0, 1, 1, 2, 2, 3]);
        assert_eq!(g.hv.again, Again::default());
        g.host.receive(&encode_c4(&C4Msg::Again { g: 4 }), g.now);
        assert!(!g.host.view().again.theirs);
    }

    #[test]
    fn the_link() {
        let mut g = Pair::new(5.0);
        g.run(C4_SILENCE_MS * 3.0, false);
        assert_eq!((g.hv.ended.clone(), g.gv.ended.clone()), (None, None));
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        g.run(C4_SILENCE_MS + 500.0, true);
        assert_eq!(g.hv.ended.as_deref(), Some("lost the connection"));
        assert_eq!(g.gv.ended.as_deref(), Some("lost the connection"));
        let mut g = Pair::new(5.0);
        g.run(200.0, false);
        g.host.receive(&encode_c4(&C4Msg::Bye), g.now);
        assert_eq!(g.host.view().ended.as_deref(), Some("your friend left"));
        assert!(!g.host.play(0, g.now));
        let mut g = Pair::new(5.0);
        g.guest_version = Some(C4_VERSION + 1.0);
        g.run(500.0, false);
        assert_eq!(
            g.hv.ended.as_deref(),
            Some("update both Q Calcs to play each other")
        );
        assert_eq!(g.gv.ended.as_deref(), Some("your friend left"));
        let mut s = session(Side::Guest, 0.0, None, 1);
        s.take_sent();
        s.receive(r#"{"t":"hi","v":1}"#, 10.0);
        assert!(s.view().ended.unwrap().contains("pong"));
        assert_eq!(s.take_sent().last().cloned(), Some(encode_c4(&C4Msg::Bye)));
    }

    #[test]
    fn solo() {
        let mut s = session(Side::Host, 0.0, Some(C4Skill::Easy), 4);
        assert!(s.view().started);
        assert!(s.play(3, 10.0));
        assert!(s.view().thinking);
        assert_eq!(s.tick(10.0 + BOT_THINK_MS - 1.0).state.moves, 1);
        assert_eq!(s.tick(10.0 + BOT_THINK_MS).state.moves, 2);
        assert!(!s.view().thinking);
        let mut now = 1000.0;
        while !is_over(&s.view().state) {
            let st = s.view().state;
            if st.turn == Side::Host {
                let c = (0..7).find(|&c| st.board[35 + c].is_none()).unwrap();
                s.play(c, now);
            }
            now += BOT_THINK_MS;
            s.tick(now);
        }
        s.rematch(now);
        let next = s.view();
        assert_eq!(
            (next.state.moves, next.state.first, next.thinking),
            (0, Side::Guest, true)
        );
        assert_eq!(s.tick(now + BOT_THINK_MS).state.moves, 1);
        assert!(s.take_sent().is_empty());
    }
}
