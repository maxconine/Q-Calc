//! `src/lib/numbers.ts`: the numbers game, 24 (four numbers, make exactly 24) and countdown (six numbers, get near
//! a target). commands live in `app/games.rs`; here the puzzles, a strict little parser, the solvers and the
//! scoring.

use std::collections::HashSet;

use serde_json::{Value as Json, json};

use super::Rand;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    TwentyFour,
    Countdown,
}

pub const TARGET_24: f64 = 24.0;
pub const SECONDS_24: u32 = 60;
pub const SECONDS_COUNTDOWN: u32 = 45;
pub const LARGE: [f64; 4] = [25.0, 50.0, 75.0, 100.0];
pub const DEFAULT_LARGE: u32 = 2;

// ---- exact fractions, so 8 ÷ 3 × 3 is 8 and not 7.999…

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frac {
    pub n: f64,
    pub d: f64,
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0.0 && !b.is_nan() {
        (a, b) = (b, a % b);
    }
    if a == 0.0 || a.is_nan() { 1.0 } else { a }
}

pub fn frac(n: f64, d: f64) -> Frac {
    let (n, d) = if d < 0.0 { (-n, -d) } else { (n, d) };
    let g = gcd(n, d);
    Frac { n: n / g + 0.0, d: d / g }
}

fn add(a: Frac, b: Frac) -> Frac {
    frac(a.n * b.d + b.n * a.d, a.d * b.d)
}
fn sub(a: Frac, b: Frac) -> Frac {
    frac(a.n * b.d - b.n * a.d, a.d * b.d)
}
fn mul(a: Frac, b: Frac) -> Frac {
    frac(a.n * b.n, a.d * b.d)
}
fn div(a: Frac, b: Frac) -> Frac {
    frac(a.n * b.d, a.d * b.n)
}

pub fn is_whole(f: Frac) -> bool {
    f.d == 1.0
}

fn num(n: f64) -> String {
    crate::jsnum::to_string(n)
}

pub fn frac_text(f: Frac) -> String {
    let neg = if f.n < 0.0 { "−" } else { "" };
    if f.d == 1.0 { format!("{neg}{}", num(f.n.abs())) } else { format!("{neg}{}/{}", num(f.n.abs()), num(f.d)) }
}

// ---- expressions as the solvers write them: fewest brackets, the game's own symbols

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn sym(self) -> char {
        match self {
            Op::Add => '+',
            Op::Sub => '−',
            Op::Mul => '×',
            Op::Div => '÷',
        }
    }
}

#[derive(Clone, Debug)]
struct Built {
    e: String,
    op: Option<Op>,
}

fn join(a: &Built, op: Op, b: &Built) -> Built {
    let loose = |x: &Built| matches!(x.op, Some(Op::Add | Op::Sub));
    let wrap = |x: &Built, need: bool| if need { format!("({})", x.e) } else { x.e.clone() };
    let mut left = a.e.clone();
    let mut right = b.e.clone();
    match op {
        Op::Sub => right = wrap(b, loose(b)),
        Op::Mul => {
            left = wrap(a, loose(a));
            right = wrap(b, loose(b));
        }
        Op::Div => {
            left = wrap(a, loose(a));
            right = wrap(b, b.op.is_some());
        }
        Op::Add => {}
    }
    Built { e: format!("{left} {} {right}", op.sym()), op: Some(op) }
}

// ---- 24

/// One way to make target from all of nums, or None when there is none.
pub fn solve24(nums: &[f64], target: f64) -> Option<String> {
    let goal = frac(target, 1.0);
    fn rec(items: Vec<(Frac, Built)>, goal: Frac) -> Option<String> {
        if items.len() == 1 {
            return (items[0].0.n == goal.n && items[0].0.d == goal.d).then(|| items[0].1.e.clone());
        }
        for i in 0..items.len() {
            for j in i + 1..items.len() {
                let (a, b) = (&items[i], &items[j]);
                let rest: Vec<(Frac, Built)> = items.iter().enumerate().filter(|(k, _)| *k != i && *k != j).map(|(_, x)| x.clone()).collect();
                let mut next = vec![
                    (add(a.0, b.0), join(&a.1, Op::Add, &b.1)),
                    (mul(a.0, b.0), join(&a.1, Op::Mul, &b.1)),
                    (sub(a.0, b.0), join(&a.1, Op::Sub, &b.1)),
                    (sub(b.0, a.0), join(&b.1, Op::Sub, &a.1)),
                ];
                if b.0.n != 0.0 {
                    next.push((div(a.0, b.0), join(&a.1, Op::Div, &b.1)));
                }
                if a.0.n != 0.0 {
                    next.push((div(b.0, a.0), join(&b.1, Op::Div, &a.1)));
                }
                for c in next {
                    let mut more = rest.clone();
                    more.push(c);
                    if let Some(found) = rec(more, goal) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    rec(nums.iter().map(|&n| (frac(n, 1.0), Built { e: num(n), op: None })).collect(), goal)
}

fn pick(rand: Rand, n: usize) -> usize {
    ((n - 1) as f64).min((rand() * n as f64).floor()) as usize
}

/// Four numbers from 1–13 that make 24, and one way.
pub fn deal24(rand: Rand) -> (Vec<f64>, String) {
    loop {
        let nums: Vec<f64> = (0..4).map(|_| 1.0 + pick(rand, 13) as f64).collect();
        if let Some(solution) = solve24(&nums, TARGET_24) {
            return (nums, solution);
        }
    }
}

// ---- countdown: whole numbers, each at most once, every step a positive whole number

#[derive(Clone, Debug, PartialEq)]
pub struct Best {
    pub value: f64,
    pub expr: String,
}

/// The closest anyone can get to target (exact when it's reachable), with one way to get there.
pub fn solve_countdown(nums: &[f64], target: f64) -> Best {
    let first = nums.first().copied().unwrap_or(0.0);
    let mut best = Best { value: first, expr: num(first) };
    for &n in nums {
        if (n - target).abs() < (best.value - target).abs() {
            best = Best { value: n, expr: num(n) };
        }
    }
    let mut seen = HashSet::new();
    fn rec(items: Vec<(f64, Built)>, target: f64, best: &mut Best, seen: &mut HashSet<String>) {
        if best.value == target || items.len() < 2 {
            return;
        }
        let mut vals: Vec<f64> = items.iter().map(|x| x.0).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let key = vals.iter().map(|v| num(*v)).collect::<Vec<_>>().join(",");
        if !seen.insert(key) {
            return;
        }
        for i in 0..items.len() {
            for j in i + 1..items.len() {
                let (mut a, mut b) = (&items[i], &items[j]);
                if a.0 < b.0 {
                    (a, b) = (b, a);
                }
                let rest: Vec<(f64, Built)> = items.iter().enumerate().filter(|(k, _)| *k != i && *k != j).map(|(_, x)| x.clone()).collect();
                let mut next = vec![(a.0 + b.0, join(&a.1, Op::Add, &b.1))];
                if b.0 != 1.0 {
                    next.push((a.0 * b.0, join(&a.1, Op::Mul, &b.1)));
                }
                if a.0 > b.0 && a.0 - b.0 != b.0 {
                    next.push((a.0 - b.0, join(&a.1, Op::Sub, &b.1)));
                }
                if b.0 != 1.0 && a.0 % b.0 == 0.0 && a.0 / b.0 != b.0 {
                    next.push((a.0 / b.0, join(&a.1, Op::Div, &b.1)));
                }
                for c in next {
                    if (c.0 - target).abs() < (best.value - target).abs() {
                        *best = Best { value: c.0, expr: c.1.e.clone() };
                    }
                    if best.value == target {
                        return;
                    }
                    let mut more = rest.clone();
                    more.push(c);
                    rec(more, target, best, seen);
                    if best.value == target {
                        return;
                    }
                }
            }
        }
    }
    rec(nums.iter().map(|&n| (n, Built { e: num(n), op: None })).collect(), target, &mut best, &mut seen);
    best
}

/// Six numbers (large from 25 50 75 100, small from two of each 1–10, as on the show) and a target 101–999 that
/// they can make exactly.
pub fn deal_countdown(large: f64, rand: Rand) -> (Vec<f64>, f64, Best) {
    let n_large = crate::jsnum::round(large).clamp(0.0, 4.0) as usize;
    let mut tries = 0;
    loop {
        let mut bigs = LARGE.to_vec();
        let mut smalls: Vec<f64> = (0..20).map(|i| 1.0 + (i % 10) as f64).collect();
        let mut nums = vec![];
        for _ in 0..n_large {
            let i = pick(rand, bigs.len());
            nums.push(bigs.remove(i));
        }
        while nums.len() < 6 {
            let i = pick(rand, smalls.len());
            nums.push(smalls.remove(i));
        }
        let target = 101.0 + pick(rand, 899) as f64;
        let best = solve_countdown(&nums, target);
        // nearly every draw is exact; after a few misses, settle for the target being one of the numbers' best
        if best.value == target {
            return (nums, target, best);
        }
        if tries >= 30 {
            let t = if (101.0..=999.0).contains(&best.value) { best.value } else { target };
            return (nums, t, best);
        }
        tries += 1;
    }
}

/// The show's points: 10 exact, 7 within 5, 5 within 10.
pub fn countdown_points(target: f64, value: Option<f64>) -> u32 {
    let Some(value) = value else { return 0 };
    let off = (target - value).abs();
    if off == 0.0 {
        10
    } else if off <= 5.0 {
        7
    } else if off <= 10.0 {
        5
    } else {
        0
    }
}

// ---- the player's expression: digits, + − × ÷ (or - * / x), brackets. no unary minus, no eval

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Num(f64),
    Op(Op, Box<Node>, Box<Node>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    Empty,
    Partial,
    Error(String),
    Ok(Node, Vec<f64>),
}

fn op_of(c: char) -> Option<Op> {
    Some(match c {
        '+' => Op::Add,
        '-' | '−' | '–' => Op::Sub,
        '*' | '×' | 'x' | 'X' | '·' => Op::Mul,
        '/' | '÷' => Op::Div,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Op(Op),
    Open,
    Close,
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
    ran_out: bool,
    used: Vec<f64>,
}

impl Parser {
    fn factor(&mut self) -> Result<Node, String> {
        let Some(tok) = self.toks.get(self.at).copied() else {
            self.ran_out = true;
            return Err("unfinished".into());
        };
        match tok {
            Tok::Num(v) => {
                self.at += 1;
                self.used.push(v);
                Ok(Node::Num(v))
            }
            Tok::Open => {
                self.at += 1;
                let inner = self.expr()?;
                match self.toks.get(self.at) {
                    None => {
                        self.ran_out = true;
                        Err("unfinished".into())
                    }
                    Some(Tok::Close) => {
                        self.at += 1;
                        Ok(inner)
                    }
                    Some(_) => Err("a bracket is missing".into()),
                }
            }
            Tok::Op(Op::Sub) => Err("no negative numbers".into()),
            Tok::Close => Err("a ) with nothing before it".into()),
            Tok::Op(op) => Err(format!("{} needs a number before it", op.sym())),
        }
    }

    fn level(&mut self, ops: &[Op], next: fn(&mut Parser) -> Result<Node, String>) -> Result<Node, String> {
        let mut node = next(self)?;
        loop {
            match self.toks.get(self.at) {
                Some(Tok::Op(op)) if ops.contains(op) => {
                    let op = *op;
                    self.at += 1;
                    let b = next(self)?;
                    node = Node::Op(op, Box::new(node), Box::new(b));
                }
                _ => return Ok(node),
            }
        }
    }

    fn term(&mut self) -> Result<Node, String> {
        self.level(&[Op::Mul, Op::Div], Parser::factor)
    }

    fn expr(&mut self) -> Result<Node, String> {
        self.level(&[Op::Add, Op::Sub], Parser::term)
    }
}

pub fn parse_expr(text: &str) -> Parsed {
    let chars: Vec<char> = text.chars().collect();
    let mut toks = vec![];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let s: String = chars[i..j].iter().collect();
            toks.push(Tok::Num(s.parse().unwrap_or(f64::NAN)));
            i = j;
        } else if c == '(' {
            toks.push(Tok::Open);
            i += 1;
        } else if c == ')' {
            toks.push(Tok::Close);
            i += 1;
        } else if let Some(op) = op_of(c) {
            toks.push(Tok::Op(op));
            i += 1;
        } else {
            return Parsed::Error(format!("only + − × ÷ and brackets, not {c}"));
        }
    }
    if toks.is_empty() {
        return Parsed::Empty;
    }
    let mut p = Parser { toks, at: 0, ran_out: false, used: vec![] };
    match p.expr() {
        Ok(node) => {
            if let Some(tok) = p.toks.get(p.at) {
                return Parsed::Error(if *tok == Tok::Close { "a ( is missing".into() } else { "an operator is missing".into() });
            }
            Parsed::Ok(node, p.used)
        }
        Err(e) => {
            if p.ran_out {
                Parsed::Partial
            } else {
                Parsed::Error(e)
            }
        }
    }
}

// value of a parsed tree. strict: countdown's rules, every step a positive whole number
fn eval(node: &Node, strict: bool) -> Result<Frac, String> {
    let Node::Op(op, a, b) = node else {
        let Node::Num(v) = node else { unreachable!() };
        return Ok(frac(*v, 1.0));
    };
    let a = eval(a, strict)?;
    let b = eval(b, strict)?;
    if *op == Op::Div && b.n == 0.0 {
        return Err("can’t divide by 0".into());
    }
    let v = match op {
        Op::Add => add(a, b),
        Op::Sub => sub(a, b),
        Op::Mul => mul(a, b),
        Op::Div => div(a, b),
    };
    if strict && !is_whole(v) {
        return Err("÷ must come out whole".into());
    }
    if strict && v.n <= 0.0 {
        return Err("every step must stay above 0".into());
    }
    Ok(v)
}

/// Whether used fits nums: all of them exactly once (24), or each at most once (countdown).
pub fn numbers_problem(used: &[f64], nums: &[f64], all: bool) -> Option<String> {
    let mut left = nums.to_vec();
    for &n in used {
        match left.iter().position(|&x| x == n) {
            Some(at) => {
                left.remove(at);
            }
            None => {
                return Some(if nums.contains(&n) { format!("{} is used too often", num(n)) } else { format!("{} isn’t one of the numbers", num(n)) });
            }
        }
    }
    if all && !left.is_empty() {
        return Some(format!("use {} too", left.iter().map(|n| num(*n)).collect::<Vec<_>>().join(", ")));
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    /// The value so far, when it parses.
    pub value: Option<Frac>,
    /// What's wrong, if anything; partial input isn't wrong yet.
    pub problem: Option<String>,
    /// A finished, legal answer.
    pub valid: bool,
}

pub fn check_expr(text: &str, nums: &[f64], mode: Mode) -> Check {
    match parse_expr(text) {
        Parsed::Empty | Parsed::Partial => Check { value: None, problem: None, valid: false },
        Parsed::Error(m) => Check { value: None, problem: Some(m), valid: false },
        Parsed::Ok(node, used) => match eval(&node, mode == Mode::Countdown) {
            Err(m) => Check { value: None, problem: Some(m), valid: false },
            Ok(v) => {
                let problem = numbers_problem(&used, nums, mode == Mode::TwentyFour);
                Check { value: Some(v), valid: problem.is_none(), problem }
            }
        },
    }
}

pub fn solves24(text: &str, nums: &[f64]) -> bool {
    let c = check_expr(text, nums, Mode::TwentyFour);
    c.valid && c.value.is_some_and(|v| v.n == TARGET_24 && v.d == 1.0)
}

/// Which tiles the typed numbers take, left to right (`usedTiles` in NumbersPanel.tsx).
pub fn used_tiles(text: &str, nums: &[f64]) -> Vec<bool> {
    let mut taken = vec![false; nums.len()];
    let mut run = String::new();
    let mut runs = vec![];
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            run.push(c);
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    for m in runs {
        if let Some(at) = (0..nums.len()).find(|&i| !taken[i] && num(nums[i]) == m) {
            taken[at] = true;
        }
    }
    taken
}

// ---- kept between games

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumbersStats {
    pub mode: Mode,
    pub large: f64,
    pub streak24: f64,
    pub best24: f64,
    pub solved24: f64,
    pub rounds: f64,
    pub points: f64,
    pub exact_streak: f64,
    pub best_exact: f64,
}

pub const STATS_KEY: &str = "qcalc-numbers";

pub const FRESH_STATS: NumbersStats = NumbersStats {
    mode: Mode::TwentyFour,
    large: DEFAULT_LARGE as f64,
    streak24: 0.0,
    best24: 0.0,
    solved24: 0.0,
    rounds: 0.0,
    points: 0.0,
    exact_streak: 0.0,
    best_exact: 0.0,
};

/// `loadStats()` from the stored text.
pub fn load_stats(raw: Option<&str>) -> NumbersStats {
    let saved = raw.and_then(|r| serde_json::from_str::<Json>(r).ok()).unwrap_or(Json::Null);
    let n = |k: &str, d: f64| saved.get(k).and_then(Json::as_f64).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(d);
    let f = FRESH_STATS;
    NumbersStats {
        mode: if saved.get("mode").and_then(Json::as_str) == Some("countdown") { Mode::Countdown } else { Mode::TwentyFour },
        large: n("large", f.large),
        streak24: n("streak24", f.streak24),
        best24: n("best24", f.best24),
        solved24: n("solved24", f.solved24),
        rounds: n("rounds", f.rounds),
        points: n("points", f.points),
        exact_streak: n("exactStreak", f.exact_streak),
        best_exact: n("bestExact", f.best_exact),
    }
}

/// `JSON.stringify(stats)`.
pub fn stats_json(s: &NumbersStats) -> String {
    json!({
        "mode": if s.mode == Mode::Countdown { "countdown" } else { "24" },
        "large": s.large, "streak24": s.streak24, "best24": s.best24, "solved24": s.solved24,
        "rounds": s.rounds, "points": s.points, "exactStreak": s.exact_streak, "bestExact": s.best_exact,
    })
    .to_string()
}

/// A 24 round ends: solved keeps the streak going, anything else (time, giving up) ends it.
pub fn record24(s: &NumbersStats, solved: bool) -> NumbersStats {
    let streak24 = if solved { s.streak24 + 1.0 } else { 0.0 };
    NumbersStats { streak24, best24: s.best24.max(streak24), solved24: s.solved24 + if solved { 1.0 } else { 0.0 }, ..*s }
}

pub fn record_countdown(s: &NumbersStats, points: u32) -> NumbersStats {
    let exact_streak = if points == 10 { s.exact_streak + 1.0 } else { 0.0 };
    NumbersStats { rounds: s.rounds + 1.0, points: s.points + f64::from(points), exact_streak, best_exact: s.best_exact.max(exact_streak), ..*s }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the TS test's linear congruential generator
    fn seeded(seed: u32) -> impl FnMut() -> f64 {
        let mut s = seed;
        move || {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f64::from(s) / 4_294_967_296.0
        }
    }

    fn f(n: f64, d: f64) -> Option<Frac> {
        Some(Frac { n, d })
    }

    #[test]
    fn solves_24() {
        for nums in [[4.0, 7.0, 8.0, 8.0], [1.0, 5.0, 5.0, 5.0], [3.0, 3.0, 8.0, 8.0], [6.0, 6.0, 6.0, 6.0], [1.0, 2.0, 3.0, 4.0], [13.0, 13.0, 13.0, 12.0]] {
            let way = solve24(&nums, TARGET_24).unwrap();
            assert!(solves24(&way, &nums), "{way}");
        }
        for nums in [[1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 2.0], [13.0, 13.0, 13.0, 13.0], [1.0, 1.0, 5.0, 9.0]] {
            assert_eq!(solve24(&nums, TARGET_24), None);
        }
        let mut rng = seeded(7);
        for _ in 0..50 {
            let (nums, solution) = deal24(&mut rng);
            assert_eq!(nums.len(), 4);
            assert!(nums.iter().all(|n| (1.0..=13.0).contains(n)));
            assert!(solves24(&solution, &nums));
        }
        // from the TS (node, lib/numbers.ts)
        assert_eq!(solve24(&[4.0, 7.0, 8.0, 8.0], TARGET_24).as_deref(), Some("8 × (4 + 7 − 8)"));
        assert_eq!(solve_countdown(&[25.0, 50.0, 75.0, 100.0, 3.0, 6.0], 952.0).expr, "(75 × 3 × (100 + 6) − 50) ÷ 25");
        let mut r = seeded(7);
        assert_eq!(deal24(&mut r), (vec![4.0, 12.0, 8.0, 13.0], "8 × (4 + 12 − 13)".to_string()));
        let (nums, target, best) = deal_countdown(2.0, &mut r);
        assert_eq!((nums, target, best.expr.as_str()), (vec![25.0, 100.0, 8.0, 2.0, 3.0, 10.0], 975.0, "(100 − 25) × (10 + 3)"));
    }

    #[test]
    fn reads_the_players_expression() {
        assert!(solves24("(7 - 8/8) * 4", &[4.0, 7.0, 8.0, 8.0]));
        assert!(solves24("(7 − 8 ÷ 8) × 4", &[4.0, 7.0, 8.0, 8.0]));
        assert!(solves24("4x(7-8/8)", &[8.0, 8.0, 7.0, 4.0]));
        assert!(solves24("8 / (3 - 8/3)", &[3.0, 3.0, 8.0, 8.0]));
        let n = [4.0, 7.0, 8.0, 8.0];
        assert!(check_expr("6*4", &n, Mode::TwentyFour).problem.unwrap().contains("isn’t one"));
        assert!(check_expr("8+8+8", &n, Mode::TwentyFour).problem.unwrap().contains("too often"));
        assert!(check_expr("8*8", &n, Mode::TwentyFour).problem.unwrap().contains("use 4, 7 too"));
        assert!(check_expr("84-7-8", &n, Mode::TwentyFour).problem.unwrap().contains("84"));
        assert!(!solves24("8*3", &n));
        for t in ["2^3", "4!", "alert(1)", "2**3", "Math.max(1)", "3 % 2", "1e3"] {
            assert!(check_expr(t, &[1.0, 2.0, 3.0, 4.0], Mode::TwentyFour).problem.is_some(), "{t}");
        }
        assert!(matches!(parse_expr("-4 + 28"), Parsed::Error(_)));
        assert!(matches!(parse_expr("4 4"), Parsed::Error(_)));
        assert!(matches!(parse_expr("4 + )"), Parsed::Error(_)));
        assert!(check_expr("4/(2-2)", &[4.0, 2.0, 2.0, 1.0], Mode::TwentyFour).problem.unwrap().contains("divide by 0"));
        for t in ["", "4 +", "(4 + 7", "(4 + 7) *", "(("] {
            let c = check_expr(t, &n, Mode::TwentyFour);
            assert_eq!((c.problem, c.valid), (None, false), "{t}");
        }
        assert_eq!(check_expr("4 + 7", &n, Mode::TwentyFour).value, f(11.0, 1.0));
        let nums = [100.0, 50.0, 3.0, 6.0, 7.0, 1.0];
        assert!(check_expr("100 * 7 - 50", &nums, Mode::Countdown).valid);
        assert!(check_expr("100 + 100", &nums, Mode::Countdown).problem.unwrap().contains("too often"));
        assert!(check_expr("7 / 3", &nums, Mode::Countdown).problem.unwrap().contains("whole"));
        assert!(check_expr("(7 / 3) * 6", &nums, Mode::Countdown).problem.unwrap().contains("whole"));
        assert!(check_expr("3 - 7 + 100", &nums, Mode::Countdown).problem.unwrap().contains("above 0"));
        assert!(check_expr("7 - 7", &[7.0, 7.0], Mode::Countdown).problem.unwrap().contains("above 0"));
        assert_eq!(check_expr("(7 / 3) * 6", &[7.0, 3.0, 6.0, 1.0], Mode::TwentyFour).value, f(14.0, 1.0));
        assert_eq!(used_tiles("8 + 8 * 12", &[8.0, 12.0, 8.0, 3.0]), vec![true, true, true, false]);
    }

    #[test]
    fn countdown() {
        let nums = [25.0, 50.0, 75.0, 100.0, 3.0, 6.0];
        let best = solve_countdown(&nums, 952.0);
        assert_eq!(best.value, 952.0);
        assert_eq!(check_expr(&best.expr, &nums, Mode::Countdown).value, f(952.0, 1.0));
        assert_eq!(solve_countdown(&[1.0; 6], 500.0).value, 9.0);
        let mut rng = seeded(3);
        for large in 0..=4 {
            let (nums, target, best) = deal_countdown(f64::from(large), &mut rng);
            assert_eq!(nums.len(), 6);
            assert_eq!(nums.iter().filter(|n| LARGE.contains(n)).count(), large as usize);
            assert!((101.0..=999.0).contains(&target));
            assert_eq!(best.value, target);
            assert_eq!(check_expr(&best.expr, &nums, Mode::Countdown).value.map(|v| v.n), Some(target));
        }
        assert_eq!(countdown_points(500.0, Some(500.0)), 10);
        assert_eq!(countdown_points(500.0, Some(495.0)), 7);
        assert_eq!(countdown_points(500.0, Some(505.0)), 7);
        assert_eq!(countdown_points(500.0, Some(510.0)), 5);
        assert_eq!(countdown_points(500.0, Some(511.0)), 0);
        assert_eq!(countdown_points(500.0, None), 0);
    }

    #[test]
    fn stats() {
        let s = record24(&record24(&FRESH_STATS, true), true);
        assert_eq!(s.streak24, 2.0);
        let s = record24(&s, false);
        assert_eq!((s.streak24, s.best24, s.solved24), (0.0, 2.0, 2.0));
        let c = record_countdown(&record_countdown(&FRESH_STATS, 10), 7);
        assert_eq!((c.rounds, c.points, c.exact_streak, c.best_exact), (2.0, 17.0, 0.0, 1.0));
        assert_eq!(record_countdown(&c, 10).exact_streak, 1.0);
        assert_eq!(load_stats(Some(&stats_json(&c))), c);
        assert_eq!(load_stats(Some("{\"mode\":\"countdown\",\"large\":-1}")).large, 2.0);
    }
}
