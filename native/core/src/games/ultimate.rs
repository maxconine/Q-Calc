//! `src/lib/ultimate.ts`: ultimate tic-tac-toe over the peer link. nine small boards in a big one. where you play in
//! a small board sends the other side to that board of the big one; sent to a board that's already won or full,
//! they play anywhere. three small boards in a row wins; a drawn small board counts for nobody. turn based like
//! connect 4: each side sends its own moves and both apply every move to the same board.

use serde_json::Value as Json;

use super::hello::{LinkedGame, other_hello};
use super::{Out, Side, count, finite, parse, stringify};

pub const UT_VERSION: f64 = 1.0;
/// Boards and the cells in each run left to right, top row first. cell c of board b is `cells[b * 9 + c]`.
pub const SIZE: usize = 9;
pub const CELLS: usize = SIZE * SIZE;

pub type Cell = Option<Side>;

/// A small board once it's decided: who took it, or a draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardMark {
    Won(Side),
    Draw,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UtState {
    pub cells: Vec<Cell>,
    pub boards: Vec<Option<BoardMark>>,
    /// Who moved first this game; the host in game 0, then alternating.
    pub first: Side,
    pub turn: Side,
    pub moves: usize,
    /// The board the side to move has to play in; None when it can play in any open board.
    pub forced: Option<usize>,
    /// The cell the last move took.
    pub last: Option<usize>,
    /// Set once the game is over: the winner and their three boards in a row; no move left is a draw.
    pub winner: Option<Side>,
    pub line: Option<Vec<usize>>,
    pub draw: bool,
}

/// Every three in a row on a 3×3 grid.
pub const LINES: [[usize; 3]; 8] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [0, 3, 6],
    [1, 4, 7],
    [2, 5, 8],
    [0, 4, 8],
    [2, 4, 6],
];

/// Game numbers count from 0; the host goes first in even ones.
pub fn first_for(game: u64) -> Side {
    if game.is_multiple_of(2) {
        Side::Host
    } else {
        Side::Guest
    }
}

pub fn new_game(first: Side) -> UtState {
    UtState {
        cells: vec![None; CELLS],
        boards: vec![None; SIZE],
        first,
        turn: first,
        moves: 0,
        forced: None,
        last: None,
        winner: None,
        line: None,
        draw: false,
    }
}

pub fn board_of(cell: usize) -> usize {
    cell / SIZE
}
pub fn spot_of(cell: usize) -> usize {
    cell % SIZE
}
pub fn cell_in(board: usize, spot: usize) -> usize {
    board * SIZE + spot
}

pub fn is_over(s: &UtState) -> bool {
    s.winner.is_some() || s.draw
}

/// `validCell`: a whole number on the board.
pub fn valid_cell(c: f64) -> bool {
    c.fract() == 0.0 && (0.0..CELLS as f64).contains(&c)
}

/// Whether the side to move may play in board b.
pub fn board_open(s: &UtState, b: usize) -> bool {
    s.boards[b].is_none() && s.forced.is_none_or(|f| f == b)
}

pub fn can_play(s: &UtState, cell: f64, who: Side) -> bool {
    !is_over(s)
        && s.turn == who
        && valid_cell(cell)
        && s.cells[cell as usize].is_none()
        && board_open(s, board_of(cell as usize))
}

/// Every cell the side to move may take.
pub fn legal_cells(s: &UtState) -> Vec<usize> {
    if is_over(s) {
        return vec![];
    }
    let mut out = vec![];
    for b in 0..SIZE {
        if !board_open(s, b) {
            continue;
        }
        for k in 0..SIZE {
            if s.cells[cell_in(b, k)].is_none() {
                out.push(cell_in(b, k));
            }
        }
    }
    out
}

// the three in a row of who's marks through spot, or None
fn line_of<T: PartialEq>(marks: &[T], who: &T, spot: usize) -> Option<Vec<usize>> {
    LINES
        .iter()
        .find(|l| l.contains(&spot) && l.iter().all(|&k| marks[k] == *who))
        .map(|l| l.to_vec())
}

/// Who takes cell; None when it isn't theirs to take (wrong turn, taken or closed cell, game over).
pub fn play(s: &UtState, cell: f64, who: Side) -> Option<UtState> {
    if !can_play(s, cell, who) {
        return None;
    }
    let cell = cell as usize;
    let mut cells = s.cells.clone();
    cells[cell] = Some(who);
    let b = board_of(cell);
    let mut boards = s.boards.clone();
    let small = &cells[b * SIZE..b * SIZE + SIZE];
    if line_of(small, &Some(who), spot_of(cell)).is_some() {
        boards[b] = Some(BoardMark::Won(who));
    } else if small.iter().all(Option::is_some) {
        boards[b] = Some(BoardMark::Draw);
    }
    let moves = s.moves + 1;
    let won_board = boards[b] == Some(BoardMark::Won(who));
    let base = UtState {
        cells,
        boards: boards.clone(),
        moves,
        last: Some(cell),
        ..s.clone()
    };
    if won_board && let Some(line) = line_of(&boards, &Some(BoardMark::Won(who)), b) {
        return Some(UtState {
            forced: None,
            winner: Some(who),
            line: Some(line),
            ..base
        });
    }
    if boards.iter().all(Option::is_some) {
        return Some(UtState {
            forced: None,
            draw: true,
            ..base
        });
    }
    let next = spot_of(cell);
    Some(UtState {
        turn: who.other(),
        forced: if boards[next].is_none() {
            Some(next)
        } else {
            None
        },
        ..base
    })
}

/// The wire. every type starts ut so pong and connect 4 on the other end ignore it; bye is the one all share.
#[derive(Clone, Debug, PartialEq)]
pub enum UtMsg {
    /// ok: the sender has already heard the other side's hi. g: sent as `ultimate`.
    Hi {
        v: f64,
        g: bool,
        ok: bool,
    },
    /// Move n (counting from 0) of game g takes cell c.
    Move {
        g: u64,
        n: usize,
        c: usize,
    },
    /// Asks for a rematch once game g is over.
    Again {
        g: u64,
    },
    /// Keeps the link from looking dead while someone thinks.
    Keep,
    Bye,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UtIn {
    Msg(UtMsg),
    Other(String),
}

pub fn encode_ut(msg: &UtMsg) -> String {
    let t = |s: &str| ("t", Out::S(s.into()));
    match msg {
        UtMsg::Hi { v, g, ok } => {
            let mut f = vec![t("ut"), ("v", Out::N(*v))];
            if *g {
                f.push(("g", Out::S("ultimate".into())));
            }
            if *ok {
                f.push(("ok", Out::B(true)));
            }
            stringify(&f)
        }
        UtMsg::Move { g, n, c } => stringify(&[
            t("utm"),
            ("g", Out::N(*g as f64)),
            ("n", Out::N(*n as f64)),
            ("c", Out::N(*c as f64)),
        ]),
        UtMsg::Again { g } => stringify(&[t("uta"), ("g", Out::N(*g as f64))]),
        UtMsg::Keep => stringify(&[t("utk")]),
        UtMsg::Bye => stringify(&[t("bye")]),
    }
}

/// Anything malformed is None: this is text from another machine.
pub fn decode_ut(raw: &str) -> Option<UtIn> {
    if raw.encode_utf16().count() > 256 {
        return None;
    }
    let m = parse(raw)?;
    let o = m.as_object()?;
    if let Some(other) = other_hello(o, LinkedGame::Ultimate) {
        return Some(UtIn::Other(other.into()));
    }
    let msg = match o.get("t").and_then(Json::as_str)? {
        "ut" => UtMsg::Hi {
            v: finite(o.get("v"))?,
            g: false,
            ok: o.get("ok") == Some(&Json::Bool(true)),
        },
        "utm" => {
            let g = count(o.get("g"), 1e6)?;
            let n = count(o.get("n"), 1e6).filter(|n| *n < CELLS as f64)?;
            let c = finite(o.get("c")).filter(|c| valid_cell(*c))?;
            UtMsg::Move {
                g: g as u64,
                n: n as usize,
                c: c as usize,
            }
        }
        "uta" => UtMsg::Again {
            g: count(o.get("g"), 1e6)? as u64,
        },
        "utk" => UtMsg::Keep,
        "bye" => UtMsg::Bye,
        _ => return None,
    };
    Some(UtIn::Msg(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(moves: &[(usize, usize)]) -> UtState {
        let mut s = new_game(Side::Host);
        for &(b, k) in moves {
            s = play(&s, cell_in(b, k) as f64, s.turn)
                .unwrap_or_else(|| panic!("move {b},{k} refused"));
        }
        s
    }

    fn f(c: usize) -> f64 {
        c as f64
    }

    #[test]
    fn rules() {
        assert_eq!(legal_cells(&new_game(Side::Host)).len(), CELLS);
        let a = run(&[(4, 2)]);
        assert_eq!((a.turn, a.forced), (Side::Guest, Some(2)));
        assert!(!can_play(&a, f(cell_in(5, 0)), Side::Guest));
        assert!(can_play(&a, f(cell_in(2, 0)), Side::Guest));
        assert!(!can_play(&a, f(cell_in(2, 0)), Side::Host));
        assert_eq!(legal_cells(&a).len(), 9);
        let a = run(&[(0, 0)]);
        assert_eq!(play(&a, f(cell_in(0, 0)), Side::Guest), None);
        assert_eq!(play(&a, f(cell_in(0, 1)), Side::Host), None);
        assert_eq!(play(&a, CELLS as f64, Side::Guest), None);
        assert_eq!(play(&a, -1.0, Side::Guest), None);
        let s = run(&[(0, 0), (0, 3), (3, 0), (0, 4), (4, 0), (0, 5)]);
        assert_eq!(s.boards[0], Some(BoardMark::Won(Side::Guest)));
        assert_eq!(s.forced, Some(5));
        let t = play(&s, f(cell_in(5, 0)), Side::Host).unwrap();
        assert_eq!(t.forced, None);
        assert!(!can_play(&t, f(cell_in(0, 6)), Side::Guest));
        assert!(can_play(&t, f(cell_in(8, 8)), Side::Guest));
        assert_eq!(
            [0, 1, 2, 3].map(first_for),
            [Side::Host, Side::Guest, Side::Host, Side::Guest]
        );
    }

    #[test]
    fn wins_and_draws() {
        let h = Some(BoardMark::Won(Side::Host));
        let g = Some(BoardMark::Won(Side::Guest));
        let d = Some(BoardMark::Draw);
        let mut s = new_game(Side::Host);
        s.boards = vec![h, h, None, d, None, None, None, None, None];
        s.cells[cell_in(2, 0)] = Some(Side::Host);
        s.cells[cell_in(2, 1)] = Some(Side::Host);
        s.forced = Some(2);
        let won = play(&s, f(cell_in(2, 2)), Side::Host).unwrap();
        assert_eq!(
            (won.winner, won.line.clone()),
            (Some(Side::Host), Some(vec![0, 1, 2]))
        );
        assert!(is_over(&won) && legal_cells(&won).is_empty());
        let drawn = UtState {
            boards: vec![h, d, None, None, None, None, None, None, None],
            ..s.clone()
        };
        assert_eq!(
            play(&drawn, f(cell_in(2, 2)), Side::Host).unwrap().winner,
            None
        );

        let mut s = new_game(Side::Host);
        s.boards = vec![h, g, h, h, g, g, g, h, None];
        let marks = [
            Side::Host,
            Side::Guest,
            Side::Host,
            Side::Host,
            Side::Guest,
            Side::Guest,
            Side::Guest,
            Side::Host,
        ];
        for (i, m) in marks.iter().enumerate() {
            s.cells[72 + i] = Some(*m);
        }
        s.forced = Some(8);
        let end = play(&s, f(cell_in(8, 8)), Side::Host).unwrap();
        assert_eq!((end.boards[8], end.draw, end.winner), (d, true, None));
    }

    #[test]
    fn the_wire() {
        for m in [
            UtMsg::Hi {
                v: 1.0,
                g: false,
                ok: false,
            },
            UtMsg::Hi {
                v: 1.0,
                g: false,
                ok: true,
            },
            UtMsg::Move { g: 2, n: 5, c: 80 },
            UtMsg::Again { g: 0 },
            UtMsg::Keep,
            UtMsg::Bye,
        ] {
            assert_eq!(decode_ut(&encode_ut(&m)), Some(UtIn::Msg(m.clone())));
        }
        assert_eq!(
            decode_ut(r#"{"t":"hi","v":3}"#),
            Some(UtIn::Other("pong".into()))
        );
        assert_eq!(
            decode_ut(r#"{"t":"c4","v":1}"#),
            Some(UtIn::Other("connect 4".into()))
        );
        let pad = format!(r#"{{"t":"utk","x":"{}"}}"#, "a".repeat(300));
        for raw in [
            "",
            "nope",
            "[]",
            "null",
            r#"{"t":"utm","g":0,"n":0,"c":81}"#,
            r#"{"t":"utm","g":0,"n":0,"c":1.5}"#,
            r#"{"t":"utm","g":-1,"n":0,"c":0}"#,
            r#"{"t":"ut"}"#,
            pad.as_str(),
        ] {
            assert_eq!(decode_ut(raw), None, "{raw}");
        }
    }
}
