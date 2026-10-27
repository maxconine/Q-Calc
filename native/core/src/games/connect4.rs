//! `src/lib/connect4.ts`: two-player connect 4 over the peer link. turn based, so neither side is in charge: each
//! sends its own moves and both apply every move to the same board. a move the board doesn't allow is dropped.

use serde_json::Value as Json;

use super::hello::{LinkedGame, other_hello};
use super::{Out, Side, count, finite, parse, stringify};

pub const C4_VERSION: f64 = 1.0;
pub const COLS: usize = 7;
pub const ROWS: usize = 6;
pub const CELLS: usize = COLS * ROWS;
pub const LINE: usize = 4;

pub type Cell = Option<Side>;

/// Cells run left to right, bottom row first: cell (col, row) is `board[row * COLS + col]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C4State {
    pub board: Vec<Cell>,
    /// Who dropped first this game; the host in game 0, then alternating.
    pub first: Side,
    pub turn: Side,
    pub moves: usize,
    /// The cell the last move filled.
    pub last: Option<usize>,
    /// Set once the game is over: the winner and their four (or more) in a row; a full board is a draw.
    pub winner: Option<Side>,
    pub line: Option<Vec<usize>>,
    pub draw: bool,
}

/// Game numbers count from 0; the host goes first in even ones.
pub fn first_for(game: u64) -> Side {
    if game % 2 == 0 { Side::Host } else { Side::Guest }
}

pub fn new_board(first: Side) -> C4State {
    C4State { board: vec![None; CELLS], first, turn: first, moves: 0, last: None, winner: None, line: None, draw: false }
}

pub fn cell_at(col: usize, row: usize) -> usize {
    row * COLS + col
}

pub fn is_over(s: &C4State) -> bool {
    s.winner.is_some() || s.draw
}

/// `validColumn`: a whole number on the board.
pub fn valid_column(col: f64) -> bool {
    col.fract() == 0.0 && (0.0..COLS as f64).contains(&col)
}

/// The row a disc dropped in col lands on, or None when the column is full (or isn't one).
pub fn landing_row(s: &C4State, col: f64) -> Option<usize> {
    if !valid_column(col) {
        return None;
    }
    let col = col as usize;
    (0..ROWS).find(|&row| s.board[cell_at(col, row)].is_none())
}

pub fn can_drop(s: &C4State, col: f64, who: Side) -> bool {
    !is_over(s) && s.turn == who && landing_row(s, col).is_some()
}

const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

/// Every cell in a row of LINE or more through the cell at (col, row), all of its colour; empty when there isn't one.
pub fn line_through(board: &[Cell], col: usize, row: usize) -> Vec<usize> {
    let Some(who) = board[cell_at(col, row)] else { return vec![] };
    let mut out = std::collections::BTreeSet::new();
    for (dc, dr) in DIRS {
        let mut run = vec![cell_at(col, row)];
        for sign in [1, -1] {
            let (mut c, mut r) = (col as i32 + dc * sign, row as i32 + dr * sign);
            while c >= 0 && c < COLS as i32 && r >= 0 && r < ROWS as i32 {
                if board[cell_at(c as usize, r as usize)] != Some(who) {
                    break;
                }
                run.push(cell_at(c as usize, r as usize));
                c += dc * sign;
                r += dr * sign;
            }
        }
        if run.len() >= LINE {
            out.extend(run);
        }
    }
    out.into_iter().collect()
}

/// Who drops in col; None when it isn't theirs to make (wrong turn, full or unknown column, game over).
pub fn drop(s: &C4State, col: f64, who: Side) -> Option<C4State> {
    if !can_drop(s, col, who) {
        return None;
    }
    let row = landing_row(s, col)?;
    let col = col as usize;
    let mut board = s.board.clone();
    let last = cell_at(col, row);
    board[last] = Some(who);
    let moves = s.moves + 1;
    let line = line_through(&board, col, row);
    let base = C4State { board, moves, last: Some(last), ..s.clone() };
    if !line.is_empty() {
        return Some(C4State { winner: Some(who), line: Some(line), ..base });
    }
    if moves == CELLS {
        return Some(C4State { draw: true, ..base });
    }
    Some(C4State { turn: who.other(), ..base })
}

/// The wire. every type starts c4 so a pong court on the other end ignores it; bye is the one both games share.
#[derive(Clone, Debug, PartialEq)]
pub enum C4Msg {
    /// ok: the sender has already heard the other side's hi, so it needs no answer. g: sent as `connect4`.
    Hi { v: f64, g: bool, ok: bool },
    /// Move n (counting from 0) of game g drops in column c.
    Move { g: u64, n: usize, c: usize },
    /// Asks for a rematch once game g is over.
    Again { g: u64 },
    /// Keeps the link from looking dead while someone thinks.
    Keep,
    Bye,
}

/// What can come in: a c4 message, or another game's hello from a friend who opened that instead.
#[derive(Clone, Debug, PartialEq)]
pub enum C4In {
    Msg(C4Msg),
    Other(String),
}

pub fn encode_c4(msg: &C4Msg) -> String {
    let t = |s: &str| ("t", Out::S(s.into()));
    match msg {
        C4Msg::Hi { v, g, ok } => {
            let mut f = vec![t("c4"), ("v", Out::N(*v))];
            if *g {
                f.push(("g", Out::S("connect4".into())));
            }
            if *ok {
                f.push(("ok", Out::B(true)));
            }
            stringify(&f)
        }
        C4Msg::Move { g, n, c } => stringify(&[t("c4m"), ("g", Out::N(*g as f64)), ("n", Out::N(*n as f64)), ("c", Out::N(*c as f64))]),
        C4Msg::Again { g } => stringify(&[t("c4a"), ("g", Out::N(*g as f64))]),
        C4Msg::Keep => stringify(&[t("c4k")]),
        C4Msg::Bye => stringify(&[t("bye")]),
    }
}

/// Anything malformed is None, never a panic: this is text from another machine.
pub fn decode_c4(raw: &str) -> Option<C4In> {
    if raw.encode_utf16().count() > 256 {
        return None;
    }
    let m = parse(raw)?;
    let o = m.as_object()?;
    if let Some(other) = other_hello(o, LinkedGame::Connect4) {
        return Some(C4In::Other(other.into()));
    }
    let msg = match o.get("t").and_then(Json::as_str)? {
        "c4" => C4Msg::Hi { v: finite(o.get("v"))?, g: false, ok: o.get("ok") == Some(&Json::Bool(true)) },
        "c4m" => {
            let g = count(o.get("g"), 1e6)?;
            let n = count(o.get("n"), 1e6).filter(|n| *n < CELLS as f64)?;
            let c = finite(o.get("c")).filter(|c| valid_column(*c))?;
            C4Msg::Move { g: g as u64, n: n as usize, c: c as usize }
        }
        "c4a" => C4Msg::Again { g: count(o.get("g"), 1e6)? as u64 },
        "c4k" => C4Msg::Keep,
        "bye" => C4Msg::Bye,
        _ => return None,
    };
    Some(C4In::Msg(msg))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // plays columns in turn from the host, failing loudly on an illegal one
    pub fn play(cols: &[usize]) -> C4State {
        let mut s = new_board(Side::Host);
        for &c in cols {
            s = drop(&s, c as f64, s.turn).unwrap_or_else(|| panic!("column {c} refused after {} moves", s.moves));
        }
        s
    }

    pub const DRAW: [usize; 42] = [
        0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 2, 3, 2, 3, 2, 3, 3, 2, 3, 2, 3, 2, 4, 5, 4, 5, 4, 5, 5, 4, 5, 4, 5, 4, 6,
        6, 6, 6, 6, 6,
    ];

    #[test]
    fn the_board() {
        let s = new_board(Side::Guest);
        assert_eq!(s.board.len(), COLS * ROWS);
        assert!(s.board.iter().all(Option::is_none));
        assert_eq!(s.turn, Side::Guest);
        assert!(!is_over(&s));
        assert_eq!([0, 1, 2, 3].map(first_for), [Side::Host, Side::Guest, Side::Host, Side::Guest]);
        let s = drop(&new_board(Side::Host), 3.0, Side::Host).unwrap();
        assert_eq!(s.board[cell_at(3, 0)], Some(Side::Host));
        assert_eq!(s.turn, Side::Guest);
        assert_eq!(s.last, Some(cell_at(3, 0)));
        let s = drop(&s, 3.0, Side::Guest).unwrap();
        assert_eq!(s.board[cell_at(3, 1)], Some(Side::Guest));
        assert_eq!(landing_row(&s, 3.0), Some(2));
        assert_eq!(s.moves, 2);
        let s = new_board(Side::Host);
        assert_eq!(drop(&s, 0.0, Side::Guest), None);
        for c in [-1.0, COLS as f64, 2.5, f64::NAN] {
            assert_eq!(drop(&s, c, Side::Host), None);
        }
        let full = play(&[0, 0, 0, 0, 0, 0]);
        assert_eq!(landing_row(&full, 0.0), None);
        assert!(!can_drop(&full, 0.0, full.turn));
    }

    #[test]
    fn wins_and_draws() {
        let s = play(&[0, 0, 1, 1, 2, 2, 3]);
        assert_eq!(s.winner, Some(Side::Host));
        assert_eq!(s.line, Some([0, 1, 2, 3].map(|c| cell_at(c, 0)).to_vec()));
        let s = play(&[0, 1, 2, 1, 2, 1, 2, 1]);
        assert_eq!(s.winner, Some(Side::Guest));
        assert_eq!(s.line, Some([0, 1, 2, 3].map(|r| cell_at(1, r)).to_vec()));
        let s = play(&[0, 1, 1, 2, 2, 3, 2, 3, 3, 6, 3]);
        assert_eq!(s.line, Some(vec![cell_at(0, 0), cell_at(1, 1), cell_at(2, 2), cell_at(3, 3)]));
        let s = play(&[3, 2, 2, 1, 1, 0, 1, 0, 0, 6, 0]);
        let mut want = vec![cell_at(3, 0), cell_at(2, 1), cell_at(1, 2), cell_at(0, 3)];
        want.sort();
        assert_eq!(s.line, Some(want));
        assert_eq!(play(&[0, 0, 1, 1, 3, 3, 4, 4, 2]).line.map(|l| l.len()), Some(5));
        assert_eq!(play(&[0, 0, 1, 1, 2]).winner, None);
        let won = play(&[0, 0, 1, 1, 2, 2, 3]);
        assert_eq!(drop(&won, 5.0, Side::Guest), None);
        assert_eq!(drop(&won, 5.0, Side::Host), None);
        assert!(line_through(&new_board(Side::Host).board, 0, 0).is_empty());
        let d = play(&DRAW);
        assert_eq!((d.moves, d.winner, d.draw), (CELLS, None, true));
        for c in 0..COLS {
            assert_eq!(landing_row(&d, c as f64), None);
        }
    }

    #[test]
    fn the_wire() {
        let msgs = [
            C4Msg::Hi { v: C4_VERSION, g: false, ok: false },
            C4Msg::Hi { v: C4_VERSION, g: false, ok: true },
            C4Msg::Move { g: 2, n: 5, c: 6 },
            C4Msg::Again { g: 3 },
            C4Msg::Keep,
            C4Msg::Bye,
        ];
        for m in msgs {
            assert_eq!(decode_c4(&encode_c4(&m)), Some(C4In::Msg(m.clone())));
        }
        assert!(encode_c4(&C4Msg::Move { g: 0, n: 10, c: 3 }).len() < 40);
        assert_eq!(encode_c4(&C4Msg::Hi { v: 1.0, g: true, ok: true }), r#"{"t":"c4","v":1,"g":"connect4","ok":true}"#);
        assert_eq!(decode_c4(r#"{"t":"hi","v":1}"#), Some(C4In::Other("pong".into())));
        assert_eq!(decode_c4(r#"{"t":"hi","v":1,"u":3}"#), Some(C4In::Other("ember".into())));
        assert_eq!(decode_c4(r#"{"t":"ut","v":1,"g":"ultimate"}"#), Some(C4In::Other("ultimate".into())));
        let pad = format!(r#"{{"t":"c4k","pad":"{}"}}"#, "x".repeat(300));
        let junk = [
            "", "nope", "{", "null", "42", "\"c4m\"", "[]", r#"[{"t":"c4k"}]"#, "{}", r#"{"t":"who"}"#, r#"{"t":"c4"}"#,
            r#"{"t":"c4","v":"1"}"#, r#"{"t":"c4m"}"#, r#"{"t":"c4m","g":0,"n":0}"#, r#"{"t":"c4m","g":0,"n":0,"c":7}"#,
            r#"{"t":"c4m","g":0,"n":0,"c":-1}"#, r#"{"t":"c4m","g":0,"n":0,"c":1.5}"#, r#"{"t":"c4m","g":0,"n":0,"c":"3"}"#,
            r#"{"t":"c4m","g":0,"n":0,"c":null}"#, r#"{"t":"c4m","g":-1,"n":0,"c":3}"#, r#"{"t":"c4m","g":0,"n":42,"c":3}"#,
            r#"{"t":"c4m","g":0,"n":1e9,"c":3}"#, r#"{"t":"c4m","g":0.5,"n":0,"c":3}"#, r#"{"t":"c4a"}"#, r#"{"t":"c4a","g":"0"}"#,
            pad.as_str(),
        ];
        for j in junk {
            assert_eq!(decode_c4(j), None, "{j}");
        }
    }
}
