//! Trails (`src/lib/trails.ts`): two cycles on a grid, each leaving a solid wall behind it. A crash into a wall, a
//! trail or the other cycle ends your round; the last one moving takes it, and first to WIN_ROUNDS takes the
//! match. Pure and deterministic: the panel owns the clock, keys and drawing, and feeds rand in for the computer.

pub mod arena;
pub mod net;
pub mod session;

use crate::app::games::{GameKind, game_command};

pub const TRAILS_HINT: &str = "↵ play trails";

pub const COLS: i32 = 64;
pub const ROWS: i32 = 26;
pub const WIN_ROUNDS: u32 = 3;
/// one cell per tick
pub const TICK_MS: f64 = 72.0;
/// turns pressed between ticks wait their turn, this many deep
pub const QUEUE_MAX: usize = 2;

/// up, right, down, left: +2 (mod 4) is the reverse
pub type Dir = u8;
pub const DX: [i32; 4] = [0, 1, 0, -1];
pub const DY: [i32; 4] = [-1, 0, 1, 0];
/// 0 or 1
pub type Player = usize;

#[derive(Clone, Debug, PartialEq)]
pub struct Cycle {
    pub x: i32,
    pub y: i32,
    pub dir: Dir,
    pub queue: Vec<Dir>,
    pub alive: bool,
    /// cell indexes from the start to the head, for drawing the wall as one line
    pub path: Vec<i32>,
    /// where it hit, when it hit; may sit just outside the grid
    pub crash: Option<(i32, i32)>,
}

/// winner None is a draw
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundResult {
    pub winner: Option<Player>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    /// 0 empty, 1 player 0's wall, 2 player 1's
    pub grid: Vec<u8>,
    pub cycles: [Cycle; 2],
    pub tick: u32,
    pub result: Option<RoundResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Easy,
    Normal,
    Hard,
}

pub const LEVELS: [Level; 3] = [Level::Easy, Level::Normal, Level::Hard];

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Easy => "easy",
            Level::Normal => "normal",
            Level::Hard => "hard",
        }
    }
}

/// 'trails', 'trail' or 'cycles', alone
pub fn is_trails_command(text: &str) -> bool {
    game_command(text) == Some(GameKind::Trails)
}

pub const fn cell_index(x: i32, y: i32) -> i32 {
    y * COLS + x
}
const fn inside(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < COLS && y < ROWS
}
const fn opposite(d: Dir) -> Dir {
    (d + 2) % 4
}

fn start_cycle(x: i32, y: i32, dir: Dir) -> Cycle {
    Cycle {
        x,
        y,
        dir,
        queue: vec![],
        alive: true,
        path: vec![cell_index(x, y)],
        crash: None,
    }
}

/// point-symmetric starts, a row apart, so neither side has the better corner and straight on isn't a crash
pub fn new_round() -> Round {
    let a = start_cycle(8, ROWS / 2 - 1, 1);
    let b = start_cycle(COLS - 1 - 8, ROWS / 2, 3);
    let mut grid = vec![0u8; (COLS * ROWS) as usize];
    grid[a.path[0] as usize] = 1;
    grid[b.path[0] as usize] = 2;
    Round {
        grid,
        cycles: [a, b],
        tick: 0,
        result: None,
    }
}

/// a turn pressed now; it's taken on a later tick. the same way or straight back (from the last turn already
/// waiting, or the way it's going) does nothing, and a full queue drops it
pub fn queue_turn(r: &Round, p: Player, dir: Dir) -> Round {
    let c = &r.cycles[p];
    if r.result.is_some() || !c.alive || c.queue.len() >= QUEUE_MAX {
        return r.clone();
    }
    let last = c.queue.last().copied().unwrap_or(c.dir);
    if dir == last || dir == opposite(last) {
        return r.clone();
    }
    let mut out = r.clone();
    out.cycles[p].queue.push(dir);
    out
}

/// the computer's choice for this tick, in place of anything queued
pub fn steer(r: &Round, p: Player, dir: Dir) -> Round {
    let c = &r.cycles[p];
    if r.result.is_some() || !c.alive {
        return r.clone();
    }
    let mut out = r.clone();
    out.cycles[p].queue = if dir == c.dir || dir == opposite(c.dir) {
        vec![]
    } else {
        vec![dir]
    };
    out
}

/// one cell on, both at once. both into the same cell is a draw, as is both crashing on the same tick
pub fn step(r: &Round) -> Round {
    if r.result.is_some() {
        return r.clone();
    }
    let mut grid = r.grid.clone();
    struct Moved {
        dir: Dir,
        queue: Vec<Dir>,
        x: i32,
        y: i32,
        hit: bool,
    }
    let mut moved: Vec<Moved> = r
        .cycles
        .iter()
        .map(|c| {
            let dir = c.queue.first().copied().unwrap_or(c.dir);
            let queue = c.queue.iter().skip(1).copied().collect();
            let x = c.x + DX[dir as usize];
            let y = c.y + DY[dir as usize];
            let hit = !inside(x, y) || r.grid[cell_index(x, y) as usize] != 0;
            Moved {
                dir,
                queue,
                x,
                y,
                hit,
            }
        })
        .collect();
    if !moved[0].hit && !moved[1].hit && moved[0].x == moved[1].x && moved[0].y == moved[1].y {
        moved[0].hit = true;
        moved[1].hit = true;
    }
    let cycles: [Cycle; 2] = std::array::from_fn(|p| {
        let m = &moved[p];
        let c = &r.cycles[p];
        if m.hit {
            return Cycle {
                dir: m.dir,
                queue: vec![],
                alive: false,
                crash: Some((m.x, m.y)),
                ..c.clone()
            };
        }
        let at = cell_index(m.x, m.y);
        grid[at as usize] = p as u8 + 1;
        let mut path = c.path.clone();
        path.push(at);
        Cycle {
            x: m.x,
            y: m.y,
            dir: m.dir,
            queue: m.queue.clone(),
            path,
            ..c.clone()
        }
    });
    let (ah, bh) = (moved[0].hit, moved[1].hit);
    let result = match (ah, bh) {
        (true, true) => Some(RoundResult { winner: None }),
        (true, false) => Some(RoundResult { winner: Some(1) }),
        (false, true) => Some(RoundResult { winner: Some(0) }),
        _ => None,
    };
    Round {
        grid,
        cycles,
        tick: r.tick + 1,
        result,
    }
}

pub fn score_round(score: [u32; 2], result: RoundResult) -> [u32; 2] {
    let mut next = score;
    if let Some(w) = result.winner {
        next[w] += 1;
    }
    next
}

pub fn match_winner(score: [u32; 2]) -> Option<Player> {
    if score[0] >= WIN_ROUNDS {
        return Some(0);
    }
    if score[1] >= WIN_ROUNDS {
        return Some(1);
    }
    None
}

/// seeded: a rand in [0, 1), for tests and replays (mulberry32)
pub fn seeded(seed: u32) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s = s.wrapping_add(0x6d2b79f5);
        let mut t = s;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4294967296.0
    }
}

// ---- the computer

fn free(grid: &[u8], x: i32, y: i32) -> bool {
    inside(x, y) && grid[cell_index(x, y) as usize] == 0
}

/// empty cells reachable from (x, y), which is itself taken; stops counting at cap
fn room(grid: &[u8], x: i32, y: i32, cap: f64) -> f64 {
    let mut seen = vec![0u8; grid.len()];
    let mut stack = vec![cell_index(x, y)];
    seen[stack[0] as usize] = 1;
    let mut n = 0.0;
    while !stack.is_empty() && n < cap {
        let i = stack.pop().unwrap();
        let cx = i % COLS;
        let cy = (i - cx) / COLS;
        for d in 0..4 {
            let nx = cx + DX[d];
            let ny = cy + DY[d];
            if !free(grid, nx, ny) {
                continue;
            }
            let j = cell_index(nx, ny);
            if seen[j as usize] != 0 {
                continue;
            }
            seen[j as usize] = 1;
            n += 1.0;
            stack.push(j);
        }
    }
    n
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    x: i32,
    y: i32,
}

/// cells each head reaches first, mine minus theirs; ties count for no one. both heads are taken cells.
/// separated tells whether the two regions ever met
fn territory(grid: &[u8], me: Cell, them: Cell) -> (f64, bool) {
    let mut owner = vec![0i8; grid.len()]; // 0 unseen, 1 mine, 2 theirs, 3 tie
    let mut dist = vec![0i32; grid.len()];
    let mut frontier = vec![cell_index(me.x, me.y), cell_index(them.x, them.y)];
    owner[frontier[0] as usize] = 1;
    owner[frontier[1] as usize] = 2;
    let mut mine = 0i32;
    let mut theirs = 0i32;
    let mut met = false;
    let mut depth = 0;
    while !frontier.is_empty() {
        depth += 1;
        let mut next = vec![];
        for &i in &frontier {
            let who = owner[i as usize];
            if who == 3 {
                continue;
            }
            let cx = i % COLS;
            let cy = (i - cx) / COLS;
            for d in 0..4 {
                let nx = cx + DX[d];
                let ny = cy + DY[d];
                if !free(grid, nx, ny) {
                    continue;
                }
                let j = cell_index(nx, ny) as usize;
                if owner[j] == 0 {
                    owner[j] = who;
                    dist[j] = depth;
                    next.push(j as i32);
                    if who == 1 {
                        mine += 1;
                    } else {
                        theirs += 1;
                    }
                } else if owner[j] != who {
                    met = true;
                    if owner[j] != 3 && dist[j] == depth {
                        if owner[j] == 1 {
                            mine -= 1;
                        } else {
                            theirs -= 1;
                        }
                        owner[j] = 3;
                    }
                }
            }
        }
        frontier = next;
    }
    (f64::from(mine - theirs), !met)
}

/// chance of a careless (but never fatal) turn each tick, and how far it looks into a pocket
fn bot(level: Level) -> (f64, f64) {
    match level {
        Level::Easy => (0.05, 24.0),
        Level::Normal | Level::Hard => (0.0, f64::INFINITY),
    }
}

fn safe_dirs(grid: &[u8], c: &Cycle) -> Vec<Dir> {
    (0..4u8)
        .filter(|&d| d != opposite(c.dir) && free(grid, c.x + DX[d as usize], c.y + DY[d as usize]))
        .collect()
}

/// how a move to (x, y) looks from the cell's walls: hugging one wastes less room
fn open_sides(grid: &[u8], x: i32, y: i32) -> f64 {
    (0..4).filter(|&d| free(grid, x + DX[d], y + DY[d])).count() as f64
}

fn with_cell(grid: &[u8], x: i32, y: i32, v: u8) -> Vec<u8> {
    let mut g = grid.to_vec();
    g[cell_index(x, y) as usize] = v;
    g
}

const WIN: f64 = 1000.0;

fn open_cells(grid: &[u8], c: Cell) -> Vec<Cell> {
    (0..4)
        .filter(|&d| free(grid, c.x + DX[d], c.y + DY[d]))
        .map(|d| Cell {
            x: c.x + DX[d],
            y: c.y + DY[d],
        })
        .collect()
}

/// hard's lookahead, taking turns: it has moved to me (already in grid), they answer their best way, then it
/// moves again, depth more times. the score is cells it reaches first, at the end of the line
const HARD_DEPTH: u32 = 1;
fn answer(grid: &[u8], me: Cell, them: Cell, me_v: u8, them_v: u8, depth: u32) -> f64 {
    let mut worst = f64::INFINITY;
    for t in open_cells(grid, them) {
        let g = with_cell(grid, t.x, t.y, them_v);
        let v = if depth == 0 {
            territory(&g, me, t).0
        } else {
            best_move(&g, me, t, me_v, them_v, depth - 1)
        };
        worst = worst.min(v);
        if worst <= -WIN {
            break;
        }
    }
    if worst == f64::INFINITY { WIN } else { worst }
}

fn best_move(grid: &[u8], me: Cell, them: Cell, me_v: u8, them_v: u8, depth: u32) -> f64 {
    let mut best = f64::NEG_INFINITY;
    for m in open_cells(grid, me) {
        best = best.max(answer(
            &with_cell(grid, m.x, m.y, me_v),
            m,
            them,
            me_v,
            them_v,
            depth,
        ));
        if best >= WIN {
            break;
        }
    }
    if best == f64::NEG_INFINITY {
        -WIN
    } else {
        best
    }
}

/// walled in alone: the most cells it can be sure of, trying every path FILL_DEPTH moves deep and counting the
/// room left at the end of each
const FILL_DEPTH: u32 = 3;
fn fill(grid: &[u8], x: i32, y: i32, depth: u32) -> f64 {
    if depth == 0 {
        return room(grid, x, y, f64::INFINITY);
    }
    let mut best: f64 = 0.0;
    for m in open_cells(grid, Cell { x, y }) {
        best = best.max(1.0 + fill(&with_cell(grid, m.x, m.y, 3), m.x, m.y, depth - 1));
    }
    best
}

/// the way the computer (player p) goes this tick. a move straight into a wall or trail is only ever picked when
/// every move is one.
/// easy: the room each way, looked at only so far, and now and then a careless turn
/// normal: the cells it would reach before you
/// hard: the same, two moves ahead, assuming you answer each of its moves your best way, which is how it cuts you
/// off; walled off from you, it searches for the tidiest way to fill its side
pub fn bot_turn(r: &Round, p: Player, level: Level, rand: &mut dyn FnMut() -> f64) -> Dir {
    let me = &r.cycles[p];
    let them = &r.cycles[1 - p];
    let safe = safe_dirs(&r.grid, me);
    if safe.is_empty() {
        return me.dir;
    }
    if safe.len() == 1 {
        return safe[0];
    }
    let (slip, look) = bot(level);
    if slip != 0.0 && rand() < slip {
        return safe[(rand() * safe.len() as f64).floor() as usize];
    }

    let mut best = safe[0];
    let mut best_score = f64::NEG_INFINITY;
    let them_cell = Cell {
        x: them.x,
        y: them.y,
    };
    for &d in &safe {
        let x = me.x + DX[d as usize];
        let y = me.y + DY[d as usize];
        let grid = with_cell(&r.grid, x, y, p as u8 + 1);
        let mut score;
        if level == Level::Easy || !them.alive {
            score = room(&grid, x, y, look);
        } else {
            let their_moves = safe_dirs(&r.grid, them);
            // could they take the same cell next tick? that's a draw at best
            let contested = their_moves
                .iter()
                .any(|&t| them.x + DX[t as usize] == x && them.y + DY[t as usize] == y);
            if level == Level::Normal {
                let (t, separated) = territory(&grid, Cell { x, y }, them_cell);
                score = if separated {
                    t - open_sides(&grid, x, y) * 0.3
                } else {
                    t
                };
                if contested {
                    score -= 500.0;
                }
            } else {
                let (t, separated) = territory(&grid, Cell { x, y }, them_cell);
                // walled off: its room counted by how well it can fill it, less theirs
                score = if separated {
                    t - room(&grid, x, y, f64::INFINITY) + fill(&grid, x, y, FILL_DEPTH)
                        - open_sides(&grid, x, y) * 0.3
                } else {
                    answer(
                        &grid,
                        Cell { x, y },
                        them_cell,
                        p as u8 + 1,
                        2 - p as u8,
                        HARD_DEPTH,
                    )
                };
                // the lookahead takes turns, so it can't see a head-on; only a sure loss is worse
                if contested {
                    score = score.min(-WIN + 100.0);
                }
            }
        }
        // a hair's preference for straight on, so it doesn't wiggle when all else is equal
        if d == me.dir {
            score += 0.1;
        }
        if score > best_score {
            best_score = score;
            best = d;
        }
    }
    best
}

#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;
