//! What a screen reader hears from the games' drawn boards (the Mac's Shell/GameSpeech.swift and
//! PuzzleSpeech.swift, word for word): connect 4's and ultimate's boards, pong's score, slots' line, equal's rows,
//! 24/countdown's tiles and the trails arena. Pure, so it's tested here; mod.rs hands it to UI Automation
//! (uia.rs) as each game's parts and notifications.

/// Connect 4's board, column by column from the bottom: `column 1: you, computer. column 4: you`, empty columns
/// left out. `board` is row-major, row 0 at the bottom, 7 wide; Some(true) is a disc of mine.
pub fn c4_board(board: &[Option<bool>], mine: &str, theirs: &str) -> String {
    let mut cols = Vec::new();
    for c in 0..7 {
        let mut discs = Vec::new();
        for r in 0..6 {
            let Some(Some(me)) = board.get(r * 7 + c) else {
                break;
            };
            discs.push(if *me { mine } else { theirs });
        }
        if !discs.is_empty() {
            cols.push(format!("column {}: {}", c + 1, discs.join(", ")));
        }
    }
    if cols.is_empty() {
        "empty".into()
    } else {
        cols.join(". ")
    }
}

/// The column held over the board, as a key moves it: `column 4`, `column 4, full`.
pub fn c4_column(col: usize, full: bool) -> String {
    format!("column {}{}", col + 1, if full { ", full" } else { "" })
}

/// A small board's outcome in ultimate, from the listener's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtOwner {
    Mine,
    Theirs,
    Draw,
}

/// The nine spots of a 3×3 board, read row by row.
pub const UT_SPOTS: [&str; 9] = [
    "top left",
    "top",
    "top right",
    "left",
    "centre",
    "right",
    "bottom left",
    "bottom",
    "bottom right",
];

/// Ultimate's board, small board by small board (numbered 1–9 as the digit keys pick them), empty ones left out:
/// `board 1: won by you. board 5: you centre; computer top left, bottom`. `cells` is `b * 9 + spot`, Some(true) a
/// mark of mine.
pub fn ut_board(
    cells: &[Option<bool>],
    boards: &[Option<UtOwner>],
    mine: &str,
    theirs: &str,
) -> String {
    let mut out = Vec::new();
    for b in 0..9 {
        match boards.get(b).copied().flatten() {
            Some(UtOwner::Mine) => {
                out.push(format!("board {}: won by {mine}", b + 1));
                continue;
            }
            Some(UtOwner::Theirs) => {
                out.push(format!("board {}: won by {theirs}", b + 1));
                continue;
            }
            Some(UtOwner::Draw) => {
                out.push(format!("board {}: drawn", b + 1));
                continue;
            }
            None => {}
        }
        let (mut me, mut them) = (Vec::new(), Vec::new());
        for (s, spot) in UT_SPOTS.iter().enumerate() {
            match cells.get(b * 9 + s).copied().flatten() {
                Some(true) => me.push(*spot),
                Some(false) => them.push(*spot),
                None => {}
            }
        }
        let mut parts = Vec::new();
        if !me.is_empty() {
            parts.push(format!("{mine} {}", me.join(", ")));
        }
        if !them.is_empty() {
            parts.push(format!("{theirs} {}", them.join(", ")));
        }
        if !parts.is_empty() {
            out.push(format!("board {}: {}", b + 1, parts.join("; ")));
        }
    }
    if out.is_empty() {
        "empty".into()
    } else {
        out.join(". ")
    }
}

/// Where the next move goes: `next move in board 3`, `next move anywhere`; nothing once the game is over.
pub fn ut_next(forced: Option<usize>, over: bool) -> String {
    match (over, forced) {
        (true, _) => String::new(),
        (false, Some(b)) => format!("next move in board {}", b + 1),
        (false, None) => "next move anywhere".into(),
    }
}

/// The cell the keys moved to: `board 5, top left`, then who holds it, or that it can't be played.
pub fn ut_cell(cell: usize, mark: Option<&str>, playable: bool) -> String {
    let at = format!("board {}, {}", cell / 9 + 1, UT_SPOTS[cell % 9]);
    match mark {
        Some(m) => format!("{at}, {m}"),
        None if playable => at,
        None => format!("{at}, can't play"),
    }
}

/// Pong's score as it changes: `point to you. you 3, computer 2`; at the end `you win, 11 to 7`. `scorer` is
/// Some(true) when the point was mine, None when no point was just scored; `won` is set at the end.
pub fn pong_score(
    mine: u32,
    theirs: u32,
    peer: &str,
    scorer: Option<bool>,
    won: Option<bool>,
) -> String {
    if let Some(won) = won {
        let (hi, lo) = (mine.max(theirs), mine.min(theirs));
        let who = if won {
            "you win".to_string()
        } else {
            format!("{peer} wins")
        };
        return format!("{who}, {hi} to {lo}");
    }
    let score = format!("you {mine}, {peer} {theirs}");
    match scorer {
        None => score,
        Some(me) => format!("point to {}. {score}", if me { "you" } else { peer }),
    }
}

/// Slots' line and what it paid: `π, blank, e. +5`; just the line before any result.
pub fn slots_line(symbols: &[&str], result: &str) -> String {
    let line = symbols
        .iter()
        .map(|s| if s.is_empty() { "blank" } else { s })
        .collect::<Vec<_>>()
        .join(", ");
    if result.is_empty() {
        line
    } else {
        format!("{line}. {result}")
    }
}

/// equal's marks, as the legend words them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Hit,
    Near,
    Miss,
}

impl Mark {
    pub fn word(self) -> &'static str {
        match self {
            Mark::Hit => "right spot",
            Mark::Near => "elsewhere",
            Mark::Miss => "not in it",
        }
    }
}

/// One equal character, spoken (`*` and `×` are times, `-` and `−` minus).
pub fn char_word(c: char) -> String {
    match c {
        '+' => "plus".into(),
        '-' | '−' => "minus".into(),
        '*' | '×' => "times".into(),
        '/' | '÷' => "divided by".into(),
        '=' => "equals".into(),
        c => c.to_string(),
    }
}

/// An equal row: a scored guess says each character's mark, the row being typed says what's in it.
pub fn equal_row(index: usize, text: &str, marks: Option<&[Mark]>, current: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let head = format!("guess {}", index + 1);
    if let Some(marks) = marks
        && marks.len() == chars.len()
        && !chars.is_empty()
    {
        let parts: Vec<String> = chars
            .iter()
            .zip(marks)
            .map(|(c, m)| format!("{} {}", char_word(*c), m.word()))
            .collect();
        return format!("{head}: {}", parts.join(", "));
    }
    if chars.is_empty() {
        return if current {
            format!("{head}, empty, typing here")
        } else {
            format!("{head}, empty")
        };
    }
    let typed: Vec<String> = chars.iter().map(|c| char_word(*c)).collect();
    format!("{head}, typing: {}", typed.join(" "))
}

/// A 24/countdown tile: its number, and whether the expression has used it.
pub fn numbers_tile(n: &str, used: bool) -> String {
    if used { format!("{n}, used") } else { n.into() }
}

/// The trails arena: the score, then the banner (a count-down or who took the round) when there is one.
pub fn trails_arena(names: &[&str], score: &[u32], banner: &str) -> String {
    let parts: Vec<String> = names
        .iter()
        .zip(score)
        .map(|(n, s)| format!("{n} {s}"))
        .collect();
    let parts = parts.join(", ");
    let b = banner.trim();
    if b.is_empty() {
        parts
    } else {
        format!("{parts}. {b}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the Mac's scripts/test-games.sh and test-puzzles.sh cases
    #[test]
    fn boards_read_as_the_mac_reads_them() {
        let mut b = vec![None; 42];
        assert_eq!(c4_board(&b, "you", "computer"), "empty");
        b[0] = Some(true);
        b[7] = Some(false);
        b[3] = Some(true);
        assert_eq!(
            c4_board(&b, "you", "computer"),
            "column 1: you, computer. column 4: you"
        );
        assert_eq!(c4_column(3, false), "column 4");
        assert_eq!(c4_column(6, true), "column 7, full");

        let mut cells = vec![None; 81];
        let mut boards = vec![None; 9];
        assert_eq!(ut_board(&cells, &boards, "you", "computer"), "empty");
        boards[0] = Some(UtOwner::Mine);
        cells[4 * 9 + 4] = Some(true);
        cells[4 * 9] = Some(false);
        cells[4 * 9 + 7] = Some(false);
        boards[8] = Some(UtOwner::Draw);
        assert_eq!(
            ut_board(&cells, &boards, "you", "computer"),
            "board 1: won by you. board 5: you centre; computer top left, bottom. board 9: drawn"
        );
        assert_eq!(ut_next(Some(2), false), "next move in board 3");
        assert_eq!(ut_next(None, false), "next move anywhere");
        assert_eq!(ut_next(None, true), "");
        assert_eq!(ut_cell(36, None, true), "board 5, top left");
        assert_eq!(
            ut_cell(36, Some("computer"), false),
            "board 5, top left, computer"
        );
        assert_eq!(ut_cell(0, None, false), "board 1, top left, can't play");
    }

    #[test]
    fn scores_and_lines() {
        assert_eq!(
            pong_score(3, 2, "computer", None, None),
            "you 3, computer 2"
        );
        assert_eq!(
            pong_score(3, 2, "computer", Some(true), None),
            "point to you. you 3, computer 2"
        );
        assert_eq!(
            pong_score(7, 11, "computer", Some(false), Some(false)),
            "computer wins, 11 to 7"
        );
        assert_eq!(
            pong_score(11, 7, "sam", None, Some(true)),
            "you win, 11 to 7"
        );
        assert_eq!(slots_line(&["π", "", "e"], "+5"), "π, blank, e. +5");
        assert_eq!(slots_line(&["7", "7", "7"], ""), "7, 7, 7");
        assert_eq!(
            trails_arena(&["you", "computer"], &[1, 2], "3"),
            "you 1, computer 2. 3"
        );
        assert_eq!(
            trails_arena(&["you", "computer"], &[0, 0], " "),
            "you 0, computer 0"
        );
    }

    #[test]
    fn puzzles() {
        use Mark::*;
        assert_eq!(
            equal_row(0, "1+2=3", Some(&[Hit, Near, Near, Miss, Hit]), false),
            "guess 1: 1 right spot, plus elsewhere, 2 elsewhere, equals not in it, 3 right spot"
        );
        assert_eq!(
            equal_row(2, "4*5", None, true),
            "guess 3, typing: 4 times 5"
        );
        assert_eq!(equal_row(3, "", None, true), "guess 4, empty, typing here");
        assert_eq!(equal_row(4, "", None, false), "guess 5, empty");
        assert_eq!(char_word('÷'), "divided by");
        assert_eq!(char_word('−'), "minus");
        assert_eq!(numbers_tile("8", true), "8, used");
        assert_eq!(numbers_tile("8", false), "8");
    }
}
