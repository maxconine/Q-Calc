//! WP-GAMES: each game's own surface (`qcalc_core::games::*`): the pure ones as free functions over records, the
//! ones with a session as objects. Randomness is `Math.random`'s stand-in here; times are `gameNow()` ms.

use std::sync::{Arc, Mutex};

use qcalc_core::games::{connect4, connect4_bot, connect4_session, crack, pong, pong_session, ultimate, ultimate_bot, ultimate_chalk, ultimate_session};

use crate::games::{GameSide, lock, math_random};

// ---- connect 4

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum C4Skill {
    Easy,
    Normal,
    Hard,
}

impl From<C4Skill> for connect4_bot::C4Skill {
    fn from(s: C4Skill) -> Self {
        match s {
            C4Skill::Easy => Self::Easy,
            C4Skill::Normal => Self::Normal,
            C4Skill::Hard => Self::Hard,
        }
    }
}

/// `C4View` with its board: cells bottom row first (`board[row * 7 + col]`), and where a disc dropped in each
/// column would land (-1: full).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct C4View {
    pub board: Vec<Option<GameSide>>,
    pub first: GameSide,
    pub turn: GameSide,
    pub moves: u32,
    pub last: Option<u32>,
    pub winner: Option<GameSide>,
    pub line: Vec<u32>,
    pub draw: bool,
    pub over: bool,
    pub landing: Vec<i32>,
    pub started: bool,
    pub wins_host: u32,
    pub wins_guest: u32,
    pub again_mine: bool,
    pub again_theirs: bool,
    pub ended: Option<String>,
    pub rev: u64,
    pub thinking: bool,
}

impl From<connect4_session::C4View> for C4View {
    fn from(v: connect4_session::C4View) -> Self {
        let s = &v.state;
        C4View {
            board: s.board.iter().map(|c| c.map(Into::into)).collect(),
            first: s.first.into(),
            turn: s.turn.into(),
            moves: s.moves as u32,
            last: s.last.map(|l| l as u32),
            winner: s.winner.map(Into::into),
            line: s.line.iter().flatten().map(|&i| i as u32).collect(),
            draw: s.draw,
            over: connect4::is_over(s),
            landing: (0..connect4::COLS).map(|c| connect4::landing_row(s, c as f64).map_or(-1, |r| r as i32)).collect(),
            started: v.started,
            wins_host: v.wins[0],
            wins_guest: v.wins[1],
            again_mine: v.again.mine,
            again_theirs: v.again.theirs,
            ended: v.ended,
            rev: v.rev,
            thinking: v.thinking,
        }
    }
}

/// `Connect4Session`: one side of a game, or a game against the computer (`solo`). What it sends the other side
/// waits in `take_sent`.
#[derive(Debug, uniffi::Object)]
pub struct Connect4Game {
    inner: Mutex<connect4_session::Connect4Session>,
}

#[uniffi::export]
impl Connect4Game {
    #[uniffi::constructor]
    pub fn new(role: GameSide, now: f64, solo: Option<C4Skill>) -> Arc<Self> {
        let s = connect4_session::Connect4Session::with_defaults(role.into(), now, solo.map(Into::into));
        Arc::new(Connect4Game { inner: Mutex::new(s) })
    }

    pub fn play(&self, col: u32, now: f64) -> bool {
        lock(&self.inner).play(col as usize, now)
    }
    pub fn rematch(&self, now: f64) {
        lock(&self.inner).rematch(now)
    }
    pub fn receive(&self, text: String, now: f64) {
        lock(&self.inner).receive(&text, now)
    }
    pub fn tick(&self, now: f64) -> C4View {
        lock(&self.inner).tick(now).into()
    }
    pub fn view(&self) -> C4View {
        lock(&self.inner).view().into()
    }
    pub fn take_sent(&self) -> Vec<String> {
        lock(&self.inner).take_sent()
    }
}

// ---- pong

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PongSkill {
    Easy,
    Normal,
    Hard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PongPhase {
    Serve,
    Play,
    Over,
}

/// What the court draws (`SessionView` with its `Snapshot` flattened; `has_snap` false before there's one).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PongView {
    pub has_snap: bool,
    pub ball_x: f64,
    pub ball_y: f64,
    pub paddle_host: f64,
    pub paddle_guest: f64,
    pub score_host: u32,
    pub score_guest: u32,
    pub phase: PongPhase,
    pub winner: Option<GameSide>,
    pub started: bool,
    pub again_mine: bool,
    pub again_theirs: bool,
    pub ended: Option<String>,
}

/// The court's sizes (`COURT_W`, …), for drawing.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct PongGeometry {
    pub court_w: f64,
    pub court_h: f64,
    pub paddle_w: f64,
    pub paddle_h: f64,
    pub paddle_inset: f64,
    pub ball_size: f64,
    pub win_score: u32,
}

#[uniffi::export]
pub fn pong_geometry() -> PongGeometry {
    PongGeometry {
        court_w: pong::COURT_W,
        court_h: pong::COURT_H,
        paddle_w: pong::PADDLE_W,
        paddle_h: pong::PADDLE_H,
        paddle_inset: pong::PADDLE_INSET,
        ball_size: pong::BALL_SIZE,
        win_score: pong::WIN_SCORE,
    }
}

impl From<pong_session::SessionView> for PongView {
    fn from(v: pong_session::SessionView) -> Self {
        let s = v.snap;
        PongView {
            has_snap: s.is_some(),
            ball_x: s.map_or(0.0, |s| s.ball.x),
            ball_y: s.map_or(0.0, |s| s.ball.y),
            paddle_host: s.map_or(pong::COURT_H / 2.0, |s| s.paddles[0]),
            paddle_guest: s.map_or(pong::COURT_H / 2.0, |s| s.paddles[1]),
            score_host: s.map_or(0, |s| s.score[0]),
            score_guest: s.map_or(0, |s| s.score[1]),
            phase: match s.map_or(pong::Phase::Serve, |s| s.phase) {
                pong::Phase::Serve => PongPhase::Serve,
                pong::Phase::Play => PongPhase::Play,
                pong::Phase::Over => PongPhase::Over,
            },
            winner: s.and_then(|s| s.winner).map(Into::into),
            started: v.started,
            again_mine: v.again.mine,
            again_theirs: v.again.theirs,
            ended: v.ended,
        }
    }
}

/// `PongSession`: one side of a game, or one against the computer (`solo`).
#[derive(Debug, uniffi::Object)]
pub struct PongGame {
    inner: Mutex<pong_session::PongSession>,
}

#[uniffi::export]
impl PongGame {
    #[uniffi::constructor]
    pub fn new(role: GameSide, now: f64, solo: Option<PongSkill>) -> Arc<Self> {
        let solo = solo.map(|s| match s {
            PongSkill::Easy => pong::Skill::Easy,
            PongSkill::Normal => pong::Skill::Normal,
            PongSkill::Hard => pong::Skill::Hard,
        });
        Arc::new(PongGame { inner: Mutex::new(pong_session::PongSession::with_defaults(role.into(), now, solo)) })
    }

    /// The up or down key went down (`held`) or up.
    pub fn key(&self, up: bool, held: bool) {
        lock(&self.inner).key(up, held)
    }
    pub fn release(&self) {
        lock(&self.inner).release()
    }
    pub fn rematch(&self) {
        lock(&self.inner).rematch()
    }
    pub fn receive(&self, text: String, now: f64) {
        lock(&self.inner).receive(&text, now)
    }
    pub fn tick(&self, now: f64) -> PongView {
        lock(&self.inner).tick(now).into()
    }
    pub fn take_sent(&self) -> Vec<String> {
        lock(&self.inner).take_sent()
    }
}

// ---- ultimate tic-tac-toe

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum UtSkill {
    Easy,
    Normal,
    Hard,
}

impl From<UtSkill> for ultimate_bot::UtSkill {
    fn from(s: UtSkill) -> Self {
        match s {
            UtSkill::Easy => Self::Easy,
            UtSkill::Normal => Self::Normal,
            UtSkill::Hard => Self::Hard,
        }
    }
}

/// A small board once it's decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum UtMark {
    Host,
    Guest,
    Draw,
}

/// `UtView` with its board: `cells[b * 9 + spot]`, each small board's mark, and which boards the side to move may
/// play in (`open`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct UtView {
    pub cells: Vec<Option<GameSide>>,
    pub boards: Vec<Option<UtMark>>,
    pub open: Vec<bool>,
    pub first: GameSide,
    pub turn: GameSide,
    pub moves: u32,
    pub forced: Option<u32>,
    pub last: Option<u32>,
    pub winner: Option<GameSide>,
    pub line: Vec<u32>,
    pub draw: bool,
    pub over: bool,
    pub started: bool,
    pub wins_host: u32,
    pub wins_guest: u32,
    pub again_mine: bool,
    pub again_theirs: bool,
    pub ended: Option<String>,
    pub rev: u64,
    pub thinking: bool,
}

impl From<ultimate_session::UtView> for UtView {
    fn from(v: ultimate_session::UtView) -> Self {
        let s = &v.state;
        UtView {
            cells: s.cells.iter().map(|c| c.map(Into::into)).collect(),
            boards: s
                .boards
                .iter()
                .map(|b| {
                    b.map(|m| match m {
                        ultimate::BoardMark::Won(qcalc_core::games::Side::Host) => UtMark::Host,
                        ultimate::BoardMark::Won(qcalc_core::games::Side::Guest) => UtMark::Guest,
                        ultimate::BoardMark::Draw => UtMark::Draw,
                    })
                })
                .collect(),
            open: (0..ultimate::SIZE).map(|b| ultimate::board_open(s, b)).collect(),
            first: s.first.into(),
            turn: s.turn.into(),
            moves: s.moves as u32,
            forced: s.forced.map(|f| f as u32),
            last: s.last.map(|l| l as u32),
            winner: s.winner.map(Into::into),
            line: s.line.iter().flatten().map(|&i| i as u32).collect(),
            draw: s.draw,
            over: ultimate::is_over(s),
            started: v.started,
            wins_host: v.wins[0],
            wins_guest: v.wins[1],
            again_mine: v.again.mine,
            again_theirs: v.again.theirs,
            ended: v.ended,
            rev: v.rev,
            thinking: v.thinking,
        }
    }
}

/// `UltimateSession`: one side of a game, or one against the computer (`solo`).
#[derive(Debug, uniffi::Object)]
pub struct UltimateGame {
    inner: Mutex<ultimate_session::UltimateSession>,
}

#[uniffi::export]
impl UltimateGame {
    #[uniffi::constructor]
    pub fn new(role: GameSide, now: f64, solo: Option<UtSkill>) -> Arc<Self> {
        let s = ultimate_session::UltimateSession::with_defaults(role.into(), now, solo.map(Into::into));
        Arc::new(UltimateGame { inner: Mutex::new(s) })
    }

    pub fn play(&self, cell: u32, now: f64) -> bool {
        lock(&self.inner).play(cell as usize, now)
    }
    /// Whether `who` may take `cell` now (`canPlay`).
    pub fn can_play(&self, cell: u32, who: GameSide) -> bool {
        ultimate::can_play(&lock(&self.inner).view().state, cell as f64, who.into())
    }
    pub fn rematch(&self, now: f64) {
        lock(&self.inner).rematch(now)
    }
    pub fn receive(&self, text: String, now: f64) {
        lock(&self.inner).receive(&text, now)
    }
    pub fn tick(&self, now: f64) -> UtView {
        lock(&self.inner).tick(now).into()
    }
    pub fn view(&self) -> UtView {
        lock(&self.inner).view().into()
    }
    pub fn take_sent(&self) -> Vec<String> {
        lock(&self.inner).take_sent()
    }
}

/// `chalkMark(kind, x, y, size, seed)`: an x or an o as an svg path (`M… C…`).
#[uniffi::export]
pub fn ut_chalk_mark(kind: String, x: f64, y: f64, size: f64, seed: f64) -> String {
    ultimate_chalk::chalk_mark(if kind == "o" { 'o' } else { 'x' }, x, y, size, seed)
}

/// `chalkLine([ax, ay], [bx, by], seed, amp)`.
#[uniffi::export]
pub fn ut_chalk_line(ax: f64, ay: f64, bx: f64, by: f64, seed: f64, amp: f64) -> String {
    ultimate_chalk::chalk_line((ax, ay), (bx, by), seed, amp)
}

/// `chalkBox(x, y, w, h, seed)`.
#[uniffi::export]
pub fn ut_chalk_box(x: f64, y: f64, w: f64, h: f64, seed: f64) -> String {
    ultimate_chalk::chalk_box(x, y, w, h, seed)
}

// ---- crack

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CrackPhase {
    Ready,
    Play,
    Fail,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CrackEvent {
    Start,
    Hit,
    Clear,
    Miss,
}

/// `CrackState`: the needle and the dot in radians clockwise from 12 o'clock, `ahead` to go to the dot.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct CrackState {
    pub level: u32,
    pub left: u32,
    pub angle: f64,
    pub dir: f64,
    pub dot: f64,
    pub ahead: f64,
    pub phase: CrackPhase,
    pub at: f64,
    pub since: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct CrackStep {
    pub state: CrackState,
    pub event: Option<CrackEvent>,
}

impl From<crack::CrackState> for CrackState {
    fn from(s: crack::CrackState) -> Self {
        let phase = match s.phase {
            crack::CrackPhase::Ready => CrackPhase::Ready,
            crack::CrackPhase::Play => CrackPhase::Play,
            crack::CrackPhase::Fail => CrackPhase::Fail,
            crack::CrackPhase::Clear => CrackPhase::Clear,
        };
        CrackState { level: s.level, left: s.left, angle: s.angle, dir: s.dir, dot: s.dot, ahead: s.ahead, phase, at: s.at, since: s.since }
    }
}

impl From<CrackState> for crack::CrackState {
    fn from(s: CrackState) -> Self {
        let phase = match s.phase {
            CrackPhase::Ready => crack::CrackPhase::Ready,
            CrackPhase::Play => crack::CrackPhase::Play,
            CrackPhase::Fail => crack::CrackPhase::Fail,
            CrackPhase::Clear => crack::CrackPhase::Clear,
        };
        crack::CrackState { level: s.level, left: s.left, angle: s.angle, dir: s.dir, dot: s.dot, ahead: s.ahead, phase, at: s.at, since: s.since }
    }
}

fn crack_step((state, event): crack::Step) -> CrackStep {
    CrackStep {
        state: state.into(),
        event: event.map(|e| match e {
            crack::CrackEvent::Start => CrackEvent::Start,
            crack::CrackEvent::Hit => CrackEvent::Hit,
            crack::CrackEvent::Clear => CrackEvent::Clear,
            crack::CrackEvent::Miss => CrackEvent::Miss,
        }),
    }
}

/// `HIT_WINDOW`, for the dot's drawn size.
#[uniffi::export]
pub fn crack_hit_window() -> f64 {
    crack::HIT_WINDOW
}

/// `startLevel(level, now, Math.random, angle, dir)`.
#[uniffi::export]
pub fn crack_start(level: u32, now: f64, angle: f64, dir: f64) -> CrackState {
    crack::start_level(level, now, &mut math_random, angle, dir).into()
}

#[uniffi::export]
pub fn crack_tick(state: CrackState, now: f64) -> CrackStep {
    crack_step(crack::tick(state.into(), now, &mut math_random))
}

#[uniffi::export]
pub fn crack_press(state: CrackState, now: f64) -> CrackStep {
    crack_step(crack::press(state.into(), now, &mut math_random))
}

/// `parseBest(localStorage.getItem('qcalc-crack'))`.
#[uniffi::export]
pub fn crack_parse_best(raw: Option<String>) -> u32 {
    crack::parse_best(raw.as_deref())
}
