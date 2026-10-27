//! `src/lib/equal.ts`: guess the hidden 8-character equation in six tries. the pure part: checking a guess,
//! colouring it the wordle way, making puzzles, the daily pick and the saved stats.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use super::{Rand, mulberry32};
use crate::jsre::{self, Lazy, js};

pub const LEN: usize = 8;
pub const TRIES: usize = 6;
pub const KEYS: &str = "0123456789+-*/=";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Miss,
    Near,
    Hit,
}

fn is_num(s: &str) -> bool {
    static NUM: Lazy = Lazy::new(|| js(r"^(?:0|[1-9]\d*)$", ""));
    jsre::test(&NUM, s)
}

fn is_op(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/')
}

// expr.split(/([+\-*/])/): the numbers and the operators between them
fn split_ops(expr: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    for c in expr.chars() {
        if is_op(c) {
            parts.push(c.to_string());
            parts.push(String::new());
        } else if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts
}

/// The left side, evaluated with * and / before + and -; None when it isn't a whole-number sum.
pub fn evaluate(expr: &str) -> Option<f64> {
    let parts = split_ops(expr);
    if parts.len() < 3 {
        return None;
    }
    if parts.iter().step_by(2).any(|p| !is_num(p)) {
        return None;
    }
    let n = |s: &str| s.parse::<f64>().unwrap_or(f64::NAN);
    let mut total = 0.0;
    let mut sign = 1.0;
    let mut term = n(&parts[0]);
    let mut i = 1;
    while i < parts.len() {
        let op = parts[i].as_str();
        let v = n(&parts[i + 1]);
        match op {
            "*" => term *= v,
            "/" => {
                if v == 0.0 || term % v != 0.0 {
                    return None;
                }
                term /= v;
            }
            _ => {
                total += sign * term;
                sign = if op == "+" { 1.0 } else { -1.0 };
                term = v;
            }
        }
        i += 2;
    }
    Some(total + sign * term)
}

/// Why a guess can't be played, or None when it can.
pub fn guess_problem(guess: &str) -> Option<&'static str> {
    let len = guess.encode_utf16().count();
    if len < LEN {
        return Some("not enough characters");
    }
    if len > LEN || guess.chars().any(|c| !KEYS.contains(c)) {
        return Some("only digits, + - × ÷ and =");
    }
    let sides: Vec<&str> = guess.split('=').collect();
    if sides.len() != 2 {
        return Some("it needs exactly one =");
    }
    let (left, right) = (sides[0], sides[1]);
    if !is_num(right) {
        return Some("the right side is just a number");
    }
    if !left.chars().any(is_op) {
        return Some("the left side needs a sum");
    }
    let Some(value) = evaluate(left) else {
        static ZERO: Lazy = Lazy::new(|| js(r"(?:^|[+\-*/])0\d", ""));
        static SHAPE: Lazy = Lazy::new(|| js(r"^[+\-*/]|[+\-*/]$|[+\-*/]{2}", ""));
        if jsre::test(&ZERO, left) {
            return Some("no leading zeros");
        }
        if jsre::test(&SHAPE, left) {
            return Some("that isn’t a sum");
        }
        return Some("division has to come out exact");
    };
    if value != right.parse::<f64>().unwrap_or(f64::NAN) {
        return Some("that doesn’t add up");
    }
    None
}

pub fn is_valid_equation(s: &str) -> bool {
    guess_problem(s).is_none()
}

/// Green for the right place, yellow for elsewhere, grey otherwise. a character only turns yellow as many times
/// as the answer has spare copies of it after the greens.
pub fn score(guess: &str, answer: &str) -> Vec<Mark> {
    let g: Vec<u16> = guess.encode_utf16().collect();
    let a: Vec<u16> = answer.encode_utf16().collect();
    let mut marks = vec![Mark::Miss; g.len()];
    let mut spare: BTreeMap<Option<u16>, usize> = BTreeMap::new();
    for i in 0..g.len() {
        if Some(&g[i]) == a.get(i) {
            marks[i] = Mark::Hit;
        } else {
            *spare.entry(a.get(i).copied()).or_insert(0) += 1;
        }
    }
    for i in 0..g.len() {
        if marks[i] == Mark::Hit {
            continue;
        }
        let left = spare.get(&Some(g[i])).copied().unwrap_or(0);
        if left > 0 {
            marks[i] = Mark::Near;
            spare.insert(Some(g[i]), left - 1);
        }
    }
    marks
}

/// What the keypad knows about each character: its best mark so far.
pub fn key_marks(guesses: &[String], answer: &str) -> BTreeMap<char, Mark> {
    let mut out = BTreeMap::new();
    for g in guesses {
        for (c, m) in g.chars().zip(score(g, answer)) {
            let e = out.entry(c).or_insert(m);
            if m > *e {
                *e = m;
            }
        }
    }
    out
}

fn pick<T: Copy>(rand: Rand, xs: &[T]) -> T {
    xs[(rand() * xs.len() as f64).floor() as usize]
}

// a random number of that many digits, never starting with 0
fn number(rand: Rand, digits: usize) -> String {
    let mut s = (1 + (rand() * 9.0).floor() as u32).to_string();
    while s.len() < digits {
        s += &((rand() * 10.0).floor() as u32).to_string();
    }
    s
}

const OPS: [char; 4] = ['+', '-', '*', '/'];

/// One puzzle: build a left side by chance and keep it when its answer fills the rest of the 8 exactly. no zero
/// operands, no answer of 0, and nothing as dull as ×1 or ÷1.
pub fn generate(rand: Rand) -> String {
    for _ in 0..100_000 {
        let right_len = pick(rand, &[1, 2, 2, 2, 3, 3]);
        let left_len = LEN - 1 - right_len;
        let ops = if left_len >= 5 {
            pick(rand, &[1, 1, 2, 2])
        } else {
            pick(rand, &[1])
        };
        let mut digits = left_len as i32 - ops as i32;
        if digits < ops as i32 + 1 {
            continue;
        }
        // split the digits over ops+1 numbers of 1 to 3 digits each
        let mut lens = vec![1; ops + 1];
        digits -= ops as i32 + 1;
        while digits > 0 {
            let i = (rand() * lens.len() as f64).floor() as usize;
            if lens[i] < 3 {
                lens[i] += 1;
                digits -= 1;
            }
        }
        let nums: Vec<String> = lens.iter().map(|&d| number(rand, d)).collect();
        let signs: Vec<char> = (0..ops).map(|_| pick(rand, &OPS)).collect();
        if signs
            .iter()
            .enumerate()
            .any(|(i, op)| (*op == '*' || *op == '/') && (nums[i + 1] == "1" || nums[i] == "1"))
        {
            continue;
        }
        let mut left = nums[0].clone();
        for (i, op) in signs.iter().enumerate() {
            left.push(*op);
            left += &nums[i + 1];
        }
        let Some(value) = evaluate(&left) else {
            continue;
        };
        if value <= 0.0 {
            continue;
        }
        let eq = format!("{left}={}", crate::jsnum::to_string(value));
        if eq.len() == LEN && is_valid_equation(&eq) {
            return eq;
        }
    }
    "12+35=47".into()
}

// days since 1970-01-01 for a civil date, and back (Howard Hinnant's algorithm), for the TS's Date arithmetic
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

fn parse_key(key: &str) -> Option<(i64, i64, i64)> {
    let mut it = key.split('-').map(|p| p.parse::<i64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// A date as yyyy-mm-dd.
pub fn date_key(y: i64, m: i64, d: i64) -> String {
    format!("{y}-{m:02}-{d:02}")
}

pub fn day_before(key: &str) -> String {
    let Some((y, m, d)) = parse_key(key) else {
        return key.to_string();
    };
    let (y, m, d) = civil_from_days(days_from_civil(y, m, d) - 1);
    date_key(y, m, d)
}

/// Puzzle numbers count from the first day, 2026-01-01.
pub fn daily_number(key: &str) -> i64 {
    let Some((y, m, d)) = parse_key(key) else {
        return 0;
    };
    days_from_civil(y, m, d) - days_from_civil(2026, 1, 1) + 1
}

fn hash(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for c in s.encode_utf16() {
        h = (h ^ u32::from(c)).wrapping_mul(16_777_619);
    }
    h
}

/// Everyone gets the same equation on the same local date.
pub fn daily_answer(key: &str) -> String {
    generate(&mut mulberry32(hash(&format!("equal:{key}"))))
}

// ---- saved: today's game, the streak and how the wins went

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyGame {
    pub date: String,
    pub guesses: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EqualStats {
    pub played: u32,
    pub wins: u32,
    pub streak: u32,
    pub best: u32,
    /// The last daily won, for the streak.
    pub last_win: Option<String>,
    /// Wins by how many guesses they took.
    pub dist: Vec<u32>,
}

impl Default for EqualStats {
    fn default() -> Self {
        EqualStats {
            played: 0,
            wins: 0,
            streak: 0,
            best: 0,
            last_win: None,
            dist: vec![0; TRIES],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EqualSave {
    pub daily: Option<DailyGame>,
    pub stats: EqualStats,
}

pub fn is_won(guesses: &[String], answer: &str) -> bool {
    guesses.iter().any(|g| g == answer)
}

pub fn is_done(guesses: &[String], answer: &str) -> bool {
    is_won(guesses, answer) || guesses.len() >= TRIES
}

/// A finished daily, into the stats. a win the day after a win keeps the streak going.
pub fn record_daily(
    stats: &EqualStats,
    date: &str,
    guesses: &[String],
    answer: &str,
) -> EqualStats {
    let won = is_won(guesses, answer);
    let mut dist = stats.dist.clone();
    if won && let Some(d) = dist.get_mut(guesses.len() - 1) {
        *d += 1;
    }
    let streak = if won {
        if stats.last_win.as_deref() == Some(day_before(date).as_str()) {
            stats.streak + 1
        } else {
            1
        }
    } else {
        0
    };
    EqualStats {
        played: stats.played + 1,
        wins: stats.wins + u32::from(won),
        streak,
        best: stats.best.max(streak),
        last_win: if won {
            Some(date.to_string())
        } else {
            stats.last_win.clone()
        },
        dist,
    }
}

/// The streak as it stands today: it lapses once a day goes by without a win.
pub fn current_streak(stats: &EqualStats, today: &str) -> u32 {
    let lw = stats.last_win.as_deref();
    if lw == Some(today) || lw == Some(day_before(today).as_str()) {
        stats.streak
    } else {
        0
    }
}

/// The save under `qcalc-equal`.
pub const SAVE_KEY: &str = "qcalc-equal";

/// `loadSave()` from the stored text.
pub fn load_save(raw: Option<&str>) -> EqualSave {
    let fallback = EqualSave::default();
    let Some(raw) = raw.and_then(|r| serde_json::from_str::<Json>(r).ok()) else {
        return fallback;
    };
    let n = |v: Option<&Json>, d: u32| v.and_then(Json::as_f64).map_or(d, |f| f.max(0.0) as u32);
    let s = raw.get("stats");
    let g = |k: &str| s.and_then(|s| s.get(k));
    let mut stats = EqualStats {
        played: n(g("played"), 0),
        wins: n(g("wins"), 0),
        streak: n(g("streak"), 0),
        best: n(g("best"), 0),
        last_win: g("lastWin").and_then(Json::as_str).map(String::from),
        dist: vec![0; TRIES],
    };
    if let Some(d) = g("dist").and_then(Json::as_array)
        && d.len() == TRIES
    {
        stats.dist = d.iter().map(|x| n(Some(x), 0)).collect();
    }
    let daily = match raw.get("daily") {
        Some(d) => match (
            d.get("date").and_then(Json::as_str),
            d.get("guesses").and_then(Json::as_array),
        ) {
            (Some(date), Some(gs)) => {
                // a guess that isn't text made the TS throw, and load nothing
                if gs.iter().any(|g| !g.is_string()) {
                    return fallback;
                }
                let guesses = gs
                    .iter()
                    .filter_map(Json::as_str)
                    .filter(|g| is_valid_equation(g))
                    .take(TRIES)
                    .map(String::from)
                    .collect();
                Some(DailyGame {
                    date: date.into(),
                    guesses,
                })
            }
            _ => None,
        },
        None => None,
    };
    EqualSave { daily, stats }
}

/// `JSON.stringify(save)`.
pub fn save_json(save: &EqualSave) -> String {
    let daily = save.daily.as_ref().map_or(
        Json::Null,
        |d| json!({ "date": d.date, "guesses": d.guesses }),
    );
    let s = &save.stats;
    // serde_json keeps keys sorted; the page reads them by name, so the order doesn't matter
    json!({
        "daily": daily,
        "stats": { "played": s.played, "wins": s.wins, "streak": s.streak, "best": s.best, "lastWin": s.last_win, "dist": s.dist },
    })
    .to_string()
}

// ---- sharing

pub fn share_text(guesses: &[String], answer: &str, title: &str) -> String {
    let tries = if is_won(guesses, answer) {
        guesses.len().to_string()
    } else {
        "X".into()
    };
    let mut lines = vec![format!("{title} {tries}/{TRIES}"), String::new()];
    for g in guesses {
        lines.push(
            score(g, answer)
                .iter()
                .map(|m| match m {
                    Mark::Hit => "🟩",
                    Mark::Near => "🟨",
                    Mark::Miss => "⬛",
                })
                .collect(),
        );
    }
    lines.join("\n")
}

/// How a character shows on a tile or key.
pub fn glyph(c: char) -> char {
    match c {
        '*' => '×',
        '/' => '÷',
        '-' => '−',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(g: &str, a: &str) -> String {
        score(g, a)
            .iter()
            .map(|m| match m {
                Mark::Hit => 'G',
                Mark::Near => 'Y',
                Mark::Miss => '.',
            })
            .collect()
    }

    #[test]
    fn evaluates_and_checks() {
        assert_eq!(evaluate("9*8-3"), Some(69.0));
        assert_eq!(evaluate("2+3*4"), Some(14.0));
        assert_eq!(evaluate("20-12/4"), Some(17.0));
        assert_eq!(evaluate("1-9+9"), Some(1.0));
        assert_eq!(evaluate("8/4*3"), Some(6.0));
        for e in ["7/2", "5/0", "7/2*2"] {
            assert_eq!(evaluate(e), None);
        }
        for g in [
            "12+35=47", "9*8-3=69", "100/4=25", "2+3*4=14", "36/4-2=7", "13-9*1=4", "1-9+10=2",
            "12-3*4=0", "9-12/4=6", "8/2*3=12",
        ] {
            assert_eq!(guess_problem(g), None, "{g}");
        }
        for (g, p) in [
            ("99-99=00", "the right side is just a number"),
            ("2+3*4=20", "that doesn’t add up"),
            ("1+1=2=2+", "it needs exactly one ="),
            ("12+35+47", "it needs exactly one ="),
            ("01+46=47", "no leading zeros"),
            ("40+07=47", "no leading zeros"),
            ("12+35=07", "the right side is just a number"),
            ("7/2*4=14", "division has to come out exact"),
            ("5/0+10=6", "division has to come out exact"),
            ("12+35=48", "that doesn’t add up"),
            ("+12+3=15", "that isn’t a sum"),
            ("12++3=15", "that isn’t a sum"),
            ("1+2=-1+4", "the right side is just a number"),
            ("12345=12", "the left side needs a sum"),
            ("12+3=15", "not enough characters"),
            ("12+3a=15", "only digits, + - × ÷ and ="),
        ] {
            assert_eq!(guess_problem(g), Some(p), "{g}");
        }
    }

    #[test]
    fn scores() {
        assert_eq!(s("12+35=47", "12+35=47"), "GGGGGGGG");
        assert_eq!(s("21+35=56", "12+35=47"), "YYGGGG..");
        assert_eq!(s("22+22=44", "12+34=46"), ".GG..GGY");
        assert_eq!(s("11+11=22", "12+10=22"), "G.GG.GGG");
        assert_eq!(s("14+33=47", "12+35=47"), "G.GG.GGG");
        assert_eq!(s("9*9-9=72", "9*8-3=69"), "GGYG.G..");
        assert_eq!(s("21+12=33", "11+11=22"), "YGGGYG..");
        let k = key_marks(&["21+35=56".into(), "12+35=47".into()], "12+35=47");
        assert_eq!(
            (k[&'1'], k[&'6'], k[&'+']),
            (Mark::Hit, Mark::Miss, Mark::Hit)
        );
    }

    #[test]
    fn generates() {
        let mut rand = mulberry32(42);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..2000 {
            let eq = generate(&mut rand);
            assert_eq!(eq.len(), LEN);
            assert_eq!(guess_problem(&eq), None, "{eq}");
            assert_ne!(eq.split('=').nth(1), Some("0"));
            seen.insert(eq);
        }
        assert!(seen.len() > 1500);
        let mut rand = mulberry32(7);
        let all: String = (0..500).map(|_| generate(&mut rand)).collect();
        for op in OPS {
            assert!(all.contains(op));
        }
    }

    #[test]
    fn the_daily() {
        // from the TS (node, lib/equal.ts)
        assert_eq!(daily_answer("2026-10-09"), "9/3+8=11");
        assert_eq!(daily_answer("2027-01-01"), "6*6+4=40");
        let mut r = mulberry32(42);
        let five: Vec<String> = (0..5).map(|_| generate(&mut r)).collect();
        assert_eq!(
            five.join(" "),
            "8*29=232 4*63=252 21+73=94 6*88=528 6*8+4=52"
        );
        let days = [
            "2026-10-09",
            "2026-10-10",
            "2026-10-11",
            "2026-10-12",
            "2027-01-01",
        ];
        let set: std::collections::HashSet<_> = days.iter().map(|d| daily_answer(d)).collect();
        assert_eq!(set.len(), days.len());
        assert_eq!(daily_number("2026-01-01"), 1);
        assert_eq!(daily_number("2026-10-09"), 282);
        assert_eq!(day_before("2026-03-01"), "2026-02-28");
        assert_eq!(day_before("2027-01-01"), "2026-12-31");
    }

    #[test]
    fn stats_saves_and_sharing() {
        let a = "12+35=47";
        let g = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let st = record_daily(
            &EqualStats::default(),
            "2026-10-07",
            &g(&["21+35=56", a]),
            a,
        );
        assert_eq!(
            (
                st.played,
                st.wins,
                st.streak,
                st.best,
                st.last_win.as_deref(),
                st.dist[1]
            ),
            (1, 1, 1, 1, Some("2026-10-07"), 1)
        );
        let st = record_daily(&st, "2026-10-08", &g(&[a]), a);
        assert_eq!(st.streak, 2);
        assert_eq!(current_streak(&st, "2026-10-09"), 2);
        assert_eq!(current_streak(&st, "2026-10-10"), 0);
        let lost = record_daily(&st, "2026-10-09", &g(&["21+35=56"; 6]), a);
        assert_eq!(
            (lost.played, lost.wins, lost.streak, lost.best),
            (3, 2, 0, 2)
        );
        assert_eq!(record_daily(&st, "2026-10-11", &g(&[a]), a).streak, 1);
        let save = EqualSave {
            daily: Some(DailyGame {
                date: "2026-10-08".into(),
                guesses: g(&[a, "nope"]),
            }),
            stats: st,
        };
        let back = load_save(Some(&save_json(&save)));
        assert_eq!(back.daily.unwrap().guesses, g(&[a]));
        assert_eq!(back.stats, save.stats);
        assert_eq!(load_save(Some("junk")), EqualSave::default());
        assert_eq!(
            share_text(&g(&["21+35=56", a]), a, "equal #282"),
            "equal #282 2/6\n\n🟨🟨🟩🟩🟩🟩⬛⬛\n🟩🟩🟩🟩🟩🟩🟩🟩"
        );
        assert_eq!(
            share_text(&g(&["21+35=56"; 6]), a, "equal")
                .split('\n')
                .next(),
            Some("equal X/6")
        );
    }
}
