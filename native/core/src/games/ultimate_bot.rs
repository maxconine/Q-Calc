//! `src/lib/ultimateBot.ts`: the computer for solo ultimate tic-tac-toe. the tree is too wide for alpha-beta to see
//! far, so it plays monte carlo tree search: random games from each candidate, more of them down the lines that keep
//! winning.

use super::ultimate::{BoardMark, LINES, SIZE, UtState, is_over, legal_cells};
use super::{Rand, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtSkill {
    Easy,
    Normal,
    Hard,
}

impl UtSkill {
    pub const ALL: [UtSkill; 3] = [UtSkill::Easy, UtSkill::Normal, UtSkill::Hard];

    pub fn name(self) -> &'static str {
        match self {
            UtSkill::Easy => "easy",
            UtSkill::Normal => "normal",
            UtSkill::Hard => "hard",
        }
    }

    // the most each level may think, in ms and in playouts
    fn budget(self) -> (f64, f64) {
        match self {
            UtSkill::Easy => (15.0, 120.0),
            UtSkill::Normal => (120.0, 6000.0),
            UtSkill::Hard => (350.0, f64::INFINITY),
        }
    }
}

// easy: how often it plays a random cell instead, and how often it sees a big-board win it should take
const EASY_WANDER: f64 = 0.3;
const EASY_SEES_WIN: f64 = 0.7;
const EXPLORE: f64 = std::f64::consts::SQRT_2;

// a position of its own, as small ints: 1 and 2 are the players, 0 empty; a board's mark is 3 for a draw
#[derive(Clone)]
struct Pos {
    cells: [i8; SIZE * SIZE],
    boards: [i8; SIZE],
    turn: i8,
    forced: i32,
    // 0 while the game goes on, else 1, 2 or 3 for a draw
    result: i8,
}

fn three_of(marks: &[i8], base: usize, who: i8) -> bool {
    LINES.iter().any(|[a, b, c]| marks[base + a] == who && marks[base + b] == who && marks[base + c] == who)
}

impl Pos {
    fn from(s: &UtState, me: Side) -> Pos {
        let side = |v: Side| if v == me { 1 } else { 2 };
        let mut cells = [0i8; SIZE * SIZE];
        for (i, c) in s.cells.iter().enumerate() {
            cells[i] = c.map_or(0, side);
        }
        let mut boards = [0i8; SIZE];
        for (i, b) in s.boards.iter().enumerate() {
            boards[i] = match b {
                None => 0,
                Some(BoardMark::Draw) => 3,
                Some(BoardMark::Won(w)) => side(*w),
            };
        }
        let result = s.winner.map_or(if s.draw { 3 } else { 0 }, side);
        Pos { cells, boards, turn: if s.turn == me { 1 } else { 2 }, forced: s.forced.map_or(-1, |f| f as i32), result }
    }

    fn moves(&self, out: &mut Vec<usize>) {
        out.clear();
        if self.result != 0 {
            return;
        }
        for b in 0..SIZE {
            if self.boards[b] != 0 || (self.forced >= 0 && self.forced != b as i32) {
                continue;
            }
            for k in b * SIZE..b * SIZE + SIZE {
                if self.cells[k] == 0 {
                    out.push(k);
                }
            }
        }
    }

    fn play(&mut self, cell: usize) {
        let who = self.turn;
        self.cells[cell] = who;
        let b = cell / SIZE;
        let base = b * SIZE;
        if three_of(&self.cells, base, who) {
            self.boards[b] = who;
            if three_of(&self.boards, 0, who) {
                self.result = who;
                return;
            }
        } else if self.cells[base..base + SIZE].iter().all(|&c| c != 0) {
            self.boards[b] = 3;
        }
        if self.boards.iter().all(|&m| m != 0) {
            self.result = 3;
            return;
        }
        let next = cell % SIZE;
        self.forced = if self.boards[next] != 0 { -1 } else { next as i32 };
        self.turn = 3 - who;
    }
}

struct Node {
    mv: usize,
    parent: Option<usize>,
    // who made move: the one this node's wins count for
    by: i8,
    kids: Vec<usize>,
    untried: Vec<usize>,
    wins: f64,
    visits: f64,
}

fn node(mv: usize, parent: Option<usize>, by: i8, pos: &Pos) -> Node {
    let mut untried = vec![];
    pos.moves(&mut untried);
    Node { mv, parent, by, kids: vec![], untried, wins: 0.0, visits: 0.0 }
}

fn pick(rand: Rand, n: usize) -> usize {
    (rand() * n as f64).floor() as usize
}

// plays one random game on from pos (which it changes) and says how it ended
fn playout(pos: &mut Pos, rand: Rand, buf: &mut Vec<usize>) -> i8 {
    while pos.result == 0 {
        pos.moves(buf);
        let m = buf[pick(rand, buf.len())];
        pos.play(m);
    }
    pos.result
}

// a cell that wins the whole game now, for whoever is to move, or None
fn winning_cell(pos: &Pos) -> Option<usize> {
    let mut moves = vec![];
    pos.moves(&mut moves);
    moves.into_iter().find(|&m| {
        let mut p = pos.clone();
        p.play(m);
        p.result == pos.turn
    })
}

/// The cell the computer takes, playing me; None when it isn't its turn or there's nothing to take.
pub fn bot_cell(s: &UtState, me: Side, skill: UtSkill, rand: Rand, clock: &mut dyn FnMut() -> f64) -> Option<usize> {
    let legal = legal_cells(s);
    if is_over(s) || s.turn != me || legal.is_empty() {
        return None;
    }
    if legal.len() == 1 {
        return Some(legal[0]);
    }
    let root = Pos::from(s, me);

    if let Some(win) = winning_cell(&root)
        && (skill != UtSkill::Easy || rand() < EASY_SEES_WIN)
    {
        return Some(win);
    }
    if skill == UtSkill::Easy && rand() < EASY_WANDER {
        return Some(legal[pick(rand, legal.len())]);
    }

    let (ms, playouts) = skill.budget();
    let deadline = clock() + ms;
    let mut tree = vec![node(usize::MAX, None, 2, &root)];
    let mut buf = vec![];
    let mut n = 0.0;
    while n < playouts && (n % 32.0 != 0.0 || clock() < deadline) {
        // down the tree by ucb1 until a node with a move left to try
        let mut pos = root.clone();
        let mut at = 0;
        while tree[at].untried.is_empty() && !tree[at].kids.is_empty() {
            let log_n = tree[at].visits.ln();
            let mut best = tree[at].kids[0];
            let mut best_score = f64::NEG_INFINITY;
            for &k in &tree[at].kids {
                let kid = &tree[k];
                let score = kid.wins / kid.visits + EXPLORE * (log_n / kid.visits).sqrt();
                if score > best_score {
                    best_score = score;
                    best = k;
                }
            }
            at = best;
            pos.play(tree[at].mv);
        }
        // one new node, then a random game from it
        if !tree[at].untried.is_empty() {
            let i = pick(rand, tree[at].untried.len());
            let mv = tree[at].untried.swap_remove(i);
            let by = pos.turn;
            pos.play(mv);
            let kid = node(mv, Some(at), by, &pos);
            tree.push(kid);
            let id = tree.len() - 1;
            tree[at].kids.push(id);
            at = id;
        }
        let result = playout(&mut pos, rand, &mut buf);
        let mut up = Some(at);
        while let Some(u) = up {
            let nd = &mut tree[u];
            nd.visits += 1.0;
            if result == nd.by {
                nd.wins += 1.0;
            } else if result == 3 {
                nd.wins += 0.5;
            }
            up = nd.parent;
        }
        n += 1.0;
    }
    // the most visited move is the one the search trusts most
    let mut pick_kid: Option<usize> = None;
    for &k in &tree[0].kids {
        if pick_kid.is_none_or(|p| tree[k].visits > tree[p].visits) {
            pick_kid = Some(k);
        }
    }
    Some(pick_kid.map_or(legal[0], |k| tree[k].mv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::ultimate::{cell_in, new_game, play};

    // the test's own seeded random (xorshift32)
    fn seeded(seed: u32) -> impl FnMut() -> f64 {
        let mut x = if seed == 0 { 1 } else { seed };
        move || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            f64::from(x) / 4_294_967_296.0
        }
    }

    fn fast() -> impl FnMut() -> f64 {
        let mut t = 0.0;
        move || {
            t += 20.0;
            t
        }
    }

    #[test]
    fn takes_the_winning_cell() {
        let g = Some(BoardMark::Won(Side::Guest));
        let h = Some(BoardMark::Won(Side::Host));
        let mut s = new_game(Side::Host);
        s.boards = vec![g, g, None, None, h, None, h, None, None];
        s.cells[cell_in(2, 4)] = Some(Side::Guest);
        s.cells[cell_in(2, 6)] = Some(Side::Guest);
        s.turn = Side::Guest;
        s.forced = Some(2);
        for skill in [UtSkill::Normal, UtSkill::Hard] {
            assert_eq!(bot_cell(&s, Side::Guest, skill, &mut seeded(1), &mut fast()), Some(cell_in(2, 2)));
        }
    }

    #[test]
    fn moves_on_its_turn_to_legal_cells() {
        let s = play(&new_game(Side::Host), cell_in(4, 4) as f64, Side::Host).unwrap();
        assert_eq!(bot_cell(&s, Side::Host, UtSkill::Hard, &mut seeded(2), &mut fast()), None);
        let c = bot_cell(&s, Side::Guest, UtSkill::Normal, &mut seeded(3), &mut fast()).unwrap();
        assert!(legal_cells(&s).contains(&c));
        let t0 = std::time::Instant::now();
        bot_cell(&new_game(Side::Host), Side::Host, UtSkill::Normal, &mut seeded(4), &mut || 0.0);
        assert!(t0.elapsed().as_secs_f64() < 2.0);
        let s = new_game(Side::Guest);
        assert!(legal_cells(&s).contains(&bot_cell(&s, Side::Guest, UtSkill::Easy, &mut seeded(7), &mut fast()).unwrap()));
    }

    #[test]
    fn beats_random_play() {
        let mut rand = seeded(5);
        let mut wins = 0;
        for g in 0..6 {
            let mut s = new_game(if g % 2 == 1 { Side::Guest } else { Side::Host });
            let mut clock = fast();
            while !is_over(&s) {
                let cell = if s.turn == Side::Host {
                    bot_cell(&s, Side::Host, UtSkill::Normal, &mut rand, &mut clock).unwrap()
                } else {
                    let legal = legal_cells(&s);
                    legal[(rand() * legal.len() as f64).floor() as usize]
                };
                s = play(&s, cell as f64, s.turn).unwrap();
            }
            if s.winner == Some(Side::Host) {
                wins += 1;
            }
        }
        assert!(wins >= 5, "{wins}");
    }
}
