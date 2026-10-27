//! `src/lib/connect4Bot.ts`: the computer for solo connect 4. it searches ahead with alpha-beta on a board of its
//! own (small ints, no copies), scoring a position it stops at by the fours each side could still make.

use std::sync::LazyLock;

use super::connect4::{C4State, CELLS, COLS, LINE, ROWS, cell_at, is_over};
use super::{Rand, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C4Skill {
    Easy,
    Normal,
    Hard,
}

impl C4Skill {
    pub const ALL: [C4Skill; 3] = [C4Skill::Easy, C4Skill::Normal, C4Skill::Hard];

    pub fn name(self) -> &'static str {
        match self {
            C4Skill::Easy => "easy",
            C4Skill::Normal => "normal",
            C4Skill::Hard => "hard",
        }
    }

    // how many moves ahead each level looks at most
    fn depth(self) -> usize {
        match self {
            C4Skill::Easy => 1,
            C4Skill::Normal => 4,
            C4Skill::Hard => 14,
        }
    }
}

/// The most a move may take: the computer's pause is only half a second.
pub const BOT_BUDGET_MS: f64 = 350.0;
// easy: how often it plays a random column instead, and how often it sees a win or a threat it should block
const EASY_WANDER: f64 = 0.35;
const EASY_SEES_WIN: f64 = 0.75;
const EASY_SEES_BLOCK: f64 = 0.55;

const WIN: f64 = 1_000_000.0;
// centre first: the best moves are usually there, so alpha-beta cuts more
const ORDER: [usize; 7] = [3, 2, 4, 1, 5, 0, 6];

// every group of four cells in a line, once
static WINDOWS: LazyLock<Vec<[usize; LINE]>> = LazyLock::new(|| {
    let mut out = vec![];
    for (dc, dr) in [(1i32, 0i32), (0, 1), (1, 1), (1, -1)] {
        for row in 0..ROWS as i32 {
            for col in 0..COLS as i32 {
                let end_c = col + dc * (LINE as i32 - 1);
                let end_r = row + dr * (LINE as i32 - 1);
                if end_c < 0 || end_c >= COLS as i32 || end_r < 0 || end_r >= ROWS as i32 {
                    continue;
                }
                out.push(std::array::from_fn(|k| cell_at((col + dc * k as i32) as usize, (row + dr * k as i32) as usize)));
            }
        }
    }
    out
});

// the windows through each cell, so a move is checked for a win without scanning the whole board
static THROUGH: LazyLock<Vec<Vec<[usize; LINE]>>> =
    LazyLock::new(|| (0..CELLS).map(|i| WINDOWS.iter().filter(|w| w.contains(&i)).copied().collect()).collect());

// a search that ran out of time; the last depth that finished stands
struct OutOfTime;

struct Search<'c> {
    // 1 is the computer, -1 its opponent, 0 empty
    board: [i8; CELLS],
    heights: [usize; COLS],
    nodes: u64,
    deadline: f64,
    clock: Option<&'c mut dyn FnMut() -> f64>,
}

impl<'c> Search<'c> {
    fn new(s: &C4State, me: Side) -> Search<'c> {
        let mut board = [0i8; CELLS];
        for (i, c) in s.board.iter().enumerate() {
            board[i] = match c {
                None => 0,
                Some(w) if *w == me => 1,
                Some(_) => -1,
            };
        }
        let mut heights = [0; COLS];
        for (col, h) in heights.iter_mut().enumerate() {
            while *h < ROWS && board[cell_at(col, *h)] != 0 {
                *h += 1;
            }
        }
        Search { board, heights, nodes: 0, deadline: f64::INFINITY, clock: None }
    }

    fn can_play(&self, col: usize) -> bool {
        self.heights[col] < ROWS
    }

    // drops for who, and says whether it made four
    fn play(&mut self, col: usize, who: i8) -> bool {
        let i = cell_at(col, self.heights[col]);
        self.heights[col] += 1;
        self.board[i] = who;
        THROUGH[i].iter().any(|w| w.iter().all(|&j| self.board[j] == who))
    }

    fn undo(&mut self, col: usize) {
        self.heights[col] -= 1;
        self.board[cell_at(col, self.heights[col])] = 0;
    }

    // the computer's view of a position nobody has won yet: fours it could still make, minus the opponent's
    fn score(&self) -> f64 {
        let mut total = 0.0;
        for w in WINDOWS.iter() {
            let (mut mine, mut theirs) = (0, 0);
            for &j in w {
                match self.board[j] {
                    1 => mine += 1,
                    -1 => theirs += 1,
                    _ => {}
                }
            }
            if mine > 0 && theirs > 0 {
                continue;
            }
            total += match (mine, theirs) {
                (3, _) => 50.0,
                (2, _) => 6.0,
                (1, _) => 1.0,
                (_, 3) => -60.0,
                (_, 2) => -6.0,
                (_, 1) => -1.0,
                _ => 0.0,
            };
        }
        for row in 0..ROWS {
            total += match self.board[cell_at(3, row)] {
                1 => 4.0,
                -1 => -4.0,
                _ => 0.0,
            };
        }
        total
    }

    fn now(&mut self) -> f64 {
        self.clock.as_mut().map_or(0.0, |c| c())
    }

    // negamax with alpha-beta: the best score `who` can force from here, looking `depth` moves ahead. a win found
    // sooner scores higher, so the computer takes the quick one and puts off a loss
    fn negamax(&mut self, depth: usize, mut alpha: f64, beta: f64, who: i8, moves: usize) -> Result<f64, OutOfTime> {
        // the clock is only read every so often; reading it is slower than a node
        self.nodes += 1;
        if self.nodes & 1023 == 0 && self.now() > self.deadline {
            return Err(OutOfTime);
        }
        if moves == CELLS {
            return Ok(0.0);
        }
        if depth == 0 {
            return Ok(f64::from(who) * self.score());
        }
        let mut best = f64::NEG_INFINITY;
        for col in ORDER {
            if !self.can_play(col) {
                continue;
            }
            let won = self.play(col, who);
            let v = if won {
                Ok(WIN + depth as f64)
            } else {
                self.negamax(depth - 1, -beta, -alpha, -who, moves + 1).map(|x| -x)
            };
            self.undo(col);
            let v = v?;
            if v > best {
                best = v;
            }
            if v > alpha {
                alpha = v;
            }
            if alpha >= beta {
                break;
            }
        }
        Ok(best)
    }
}

fn legal(s: &C4State) -> Vec<usize> {
    ORDER.into_iter().filter(|&col| s.board[cell_at(col, ROWS - 1)].is_none()).collect()
}

// a column that wins at once for who, or None
fn winning_column(search: &mut Search, who: i8) -> Option<usize> {
    for col in ORDER {
        if !search.can_play(col) {
            continue;
        }
        let won = search.play(col, who);
        search.undo(col);
        if won {
            return Some(col);
        }
    }
    None
}

// centre columns more often than edge ones
fn loose_column(cols: &[usize], rand: Rand) -> usize {
    let weights: Vec<f64> = cols.iter().map(|&c| 4.0 - (c as f64 - 3.0).abs()).collect();
    let mut r = rand() * weights.iter().sum::<f64>();
    for (k, w) in weights.iter().enumerate() {
        r -= w;
        if r < 0.0 {
            return cols[k];
        }
    }
    cols[cols.len() - 1]
}

// the best columns looking `depth` moves ahead, best first in `cols`' order; ties are all kept, so games differ
fn best_columns(search: &mut Search, cols: &[usize], depth: usize, moves: usize) -> Result<(Vec<usize>, f64), OutOfTime> {
    let mut best = f64::NEG_INFINITY;
    let mut picks = vec![];
    for &col in cols {
        let won = search.play(col, 1);
        // a column can only matter if it's at least as good as the best so far; the window stays one point open
        // below, so a column as good as the best still counts as one
        let v = if won {
            Ok(WIN + depth as f64 + 1.0)
        } else {
            search.negamax(depth - 1, f64::NEG_INFINITY, -(best - 1.0), -1, moves + 1).map(|x| -x)
        };
        search.undo(col);
        let v = v?;
        if v > best {
            best = v;
            picks = vec![col];
        } else if v == best {
            picks.push(col);
        }
    }
    Ok((picks, best))
}

/// The column the computer drops in, playing as `me`; None when the game is over or it isn't its turn.
pub fn bot_column(s: &C4State, me: Side, skill: C4Skill, rand: Rand, clock: &mut dyn FnMut() -> f64) -> Option<usize> {
    let cols = legal(s);
    if is_over(s) || s.turn != me || cols.is_empty() {
        return None;
    }
    let mut search = Search::new(s, me);

    if skill == C4Skill::Easy {
        if let Some(win) = winning_column(&mut search, 1)
            && rand() < EASY_SEES_WIN
        {
            return Some(win);
        }
        if let Some(block) = winning_column(&mut search, -1)
            && rand() < EASY_SEES_BLOCK
        {
            return Some(block);
        }
        if rand() < EASY_WANDER {
            return Some(loose_column(&cols, rand));
        }
    }

    // a four on the board right now needs no thinking
    if let Some(now) = winning_column(&mut search, 1) {
        return Some(now);
    }

    // deeper and deeper until the level's depth or the time runs out; each depth tries the last one's best first.
    // a win or loss found for certain won't change by looking further, so that ends it too
    let start = clock();
    search.deadline = start + BOT_BUDGET_MS;
    search.clock = Some(clock);
    let mut picks = cols.clone();
    let mut depth = 1;
    while depth <= skill.depth() && depth <= CELLS - s.moves {
        let mut order = picks.clone();
        order.extend(cols.iter().filter(|c| !picks.contains(c)));
        let Ok((found, best)) = best_columns(&mut search, &order, depth, s.moves) else { break };
        picks = found;
        if best.abs() >= WIN {
            break;
        }
        depth += 1;
    }
    Some(picks[(rand() * picks.len() as f64).floor() as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::connect4::{drop, new_board, tests::play};
    use crate::games::mulberry32;

    fn real_clock() -> impl FnMut() -> f64 {
        let t0 = std::time::Instant::now();
        move || t0.elapsed().as_secs_f64() * 1000.0
    }

    #[test]
    fn takes_a_win_and_blocks_a_four() {
        let s = play(&[0, 6, 1, 6, 2, 5]);
        for k in [C4Skill::Normal, C4Skill::Hard] {
            assert_eq!(bot_column(&s, Side::Host, k, &mut mulberry32(1), &mut real_clock()), Some(3));
        }
        let s = play(&[0, 6, 1, 6, 2]);
        for k in [C4Skill::Normal, C4Skill::Hard] {
            assert_eq!(bot_column(&s, Side::Guest, k, &mut mulberry32(1), &mut real_clock()), Some(3));
        }
    }

    #[test]
    fn plays_a_column_with_room_on_its_own_turn() {
        let full = play(&[0, 0, 0, 0, 0, 0]);
        for k in C4Skill::ALL {
            for i in 0..20 {
                let mut t = 0.0;
                let mut fast = || {
                    t += 40.0;
                    t
                };
                assert_ne!(bot_column(&full, Side::Host, k, &mut mulberry32(i), &mut fast), Some(0));
            }
        }
        assert_eq!(bot_column(&new_board(Side::Host), Side::Guest, C4Skill::Hard, &mut mulberry32(1), &mut real_clock()), None);
    }

    #[test]
    fn stops_on_time_and_still_finds_the_win() {
        assert!(BOT_BUDGET_MS < super::super::connect4_session::BOT_THINK_MS);
        for s in [new_board(Side::Host), play(&[3, 3, 2, 4])] {
            let t = std::time::Instant::now();
            assert!(bot_column(&s, Side::Host, C4Skill::Hard, &mut mulberry32(3), &mut real_clock()).is_some());
            assert!(t.elapsed().as_secs_f64() * 1000.0 < BOT_BUDGET_MS + 100.0);
        }
        let mut t = 0.0;
        let s = play(&[0, 6, 1, 6, 2, 5]);
        let mut slow = || {
            t += 1e6;
            t
        };
        assert_eq!(bot_column(&s, Side::Host, C4Skill::Hard, &mut mulberry32(5), &mut slow), Some(3));
    }

    fn matchup(a: C4Skill, b: C4Skill, games: usize, seed: u32) -> (usize, usize, usize) {
        let mut rand = mulberry32(seed);
        let mut out = (0, 0, 0);
        for g in 0..games {
            let mut s = new_board(if g % 2 == 0 { Side::Host } else { Side::Guest });
            let mut t = 0.0;
            while !is_over(&s) {
                let skill = if s.turn == Side::Host { a } else { b };
                let mut clock = || {
                    t += 40.0;
                    t
                };
                let col = bot_column(&s, s.turn, skill, &mut rand, &mut clock).unwrap();
                s = drop(&s, col as f64, s.turn).unwrap();
            }
            match s.winner {
                Some(Side::Host) => out.0 += 1,
                Some(Side::Guest) => out.1 += 1,
                None => out.2 += 1,
            }
        }
        out
    }

    #[test]
    fn the_levels() {
        let ne = matchup(C4Skill::Normal, C4Skill::Easy, 20, 1);
        assert!(ne.0 > ne.1 * 3, "{ne:?}");
        let hn = matchup(C4Skill::Hard, C4Skill::Normal, 8, 2);
        assert!(hn.0 > hn.1, "{hn:?}");
    }
}
