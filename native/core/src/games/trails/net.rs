//! Trails over the peer link, as json text (`src/lib/trailsNet.ts`). Both sides run the same round and only swap
//! turns: one small frame message a tick, each naming the tick its turns land on. Every type starts `tr`, so pong,
//! connect 4 and ember & frost ignore them; the hello is tagged with its game, and every game's is recognised
//! (hello.rs) so a mismatched pair is told.

use serde_json::{Map, Value};

use super::{COLS, Cycle, Dir, ROWS, Round, RoundResult, WIN_ROUNDS};
use crate::games::hello::{LinkedGame, other_hello};
use crate::jsnum;

pub const TRAILS_VERSION: u32 = 1;
/// how the start of a round is timed, here so the panel and the online game count the same
pub const COUNT_MS: f64 = 550.0;
pub const POINT_MS: f64 = 1300.0;
/// turns land this many ticks after they're pressed, at most; the host picks from the link's ping
pub const MAX_DELAY: u32 = 4;
/// a round can't outlast the grid: each tick fills a cell per cycle
pub const MAX_TICK: u32 = (COLS * ROWS) as u32;
/// frames one message may carry
pub const MAX_FRAMES: usize = 48;

const MAX_TEXT: usize = 12_000;
const MAX_COUNT: f64 = 1e6;
const MAX_WAIT_MS: f64 = 10_000.0;
const U32: f64 = 4294967295.0;

/// a player's turn for one tick; None is straight on
pub type Frame = Option<Dir>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetPhase {
    Count,
    Run,
    Point,
    Over,
}

const PHASES: [NetPhase; 4] = [NetPhase::Count, NetPhase::Run, NetPhase::Point, NetPhase::Over];

/// the host's whole game, enough for the guest to carry on from it
#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    /// epoch: goes up with every snapshot
    pub e: u32,
    /// the round's number since the link opened
    pub r: u32,
    /// the input delay, in ticks
    pub d: u32,
    pub score: [u32; 2],
    pub phase: NetPhase,
    /// ms left of count or point, from when this arrives
    pub wait: f64,
    pub round: Round,
    /// each side's turns from round.tick on, as far as the host holds them: host then guest
    pub frames: [Vec<Frame>; 2],
    pub hash: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrailsMsg {
    /// ok: the sender has already heard the other side's hello, so it needs no answer. g: tagged 'trails'
    Hi { v: f64, g: bool, ok: bool },
    /// the host's ping, and keepalive: c is its clock, sent straight back in trq
    Ping { c: f64 },
    Pong { c: f64 },
    /// the guest's keepalive
    Keep { e: u32 },
    /// turns for ticks f, f+1…; a: the last tick of yours I hold with none missing before it; h: the guest's
    /// running hash after that many ticks, for the host to check
    Frames { e: u32, r: u32, f: u32, i: Vec<Frame>, a: i64, h: Option<(u32, u32)> },
    Snap(Box<Snap>),
    /// a rematch asked for
    Again { e: u32 },
    Bye,
    /// what arrives may also be another game's hello (only ever decoded)
    Other { game: String },
}

// ---- the running hash: every position since the round began, so two sides that agree on it agree on the grid

fn mix(h: u32, n: i64) -> u32 {
    (h ^ ((n as i32 as u32) & 0xffff)).wrapping_mul(16777619)
}

pub fn hash_start(r: u32, score: [u32; 2]) -> u32 {
    mix(mix(mix(0x811c9dc5, i64::from(r)), i64::from(score[0])), i64::from(score[1]))
}

pub fn hash_step(h: u32, round: &Round) -> u32 {
    let mut out = mix(h, i64::from(round.tick));
    for c in &round.cycles {
        out = mix(mix(mix(mix(out, i64::from(c.x)), i64::from(c.y)), i64::from(c.dir)), if c.alive { 1 } else { 2 });
    }
    out
}

// ---- a round as text: each wall as the cells where it starts, turns and ends

fn corners(path: &[i32]) -> Vec<i32> {
    let mut out = vec![];
    for (i, &cell) in path.iter().enumerate() {
        let before = if i > 0 { path.get(i - 1) } else { None };
        let after = path.get(i + 1);
        match (before, after) {
            (Some(&b), Some(&a)) if cell - b == a - cell => {}
            _ => out.push(cell),
        }
    }
    out
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// `count(v, max)`: a whole number in 0..=max
fn count(v: Option<&Value>, max: f64) -> Option<f64> {
    num(v).filter(|&n| n.fract() == 0.0 && (0.0..=max).contains(&n))
}

fn is_dir(v: Option<&Value>) -> Option<Dir> {
    count(v, 3.0).map(|n| n as Dir)
}

/// the cells from corner to corner, written into grid as v; None if they don't make a wall
fn wall(grid: &mut [u8], corners_in: Option<&Value>, v: u8) -> Option<Vec<i32>> {
    let list = corners_in?.as_array()?;
    if list.is_empty() || list.len() > MAX_TICK as usize {
        return None;
    }
    let mut path = vec![];
    let mut put = |grid: &mut [u8], cell: i32| -> bool {
        if grid[cell as usize] != 0 {
            return false;
        }
        grid[cell as usize] = v;
        path.push(cell);
        true
    };
    let mut at: Option<i32> = None;
    for c in list {
        let c = count(Some(c), f64::from(COLS * ROWS - 1))? as i32;
        match at {
            None => {
                if !put(grid, c) {
                    return None;
                }
            }
            Some(a) => {
                let dx = (c % COLS) - (a % COLS);
                let dy = c / COLS - a / COLS;
                if (dx == 0) == (dy == 0) {
                    return None;
                }
                let by = if dx != 0 { dx.signum() } else { dy.signum() * COLS };
                let mut cell = a + by;
                loop {
                    if !put(grid, cell) {
                        return None;
                    }
                    if cell == c {
                        break;
                    }
                    cell += by;
                }
            }
        }
        at = Some(c);
    }
    Some(path)
}

fn pack_cycle(c: &Cycle) -> String {
    let cs: Vec<String> = corners(&c.path).iter().map(|n| n.to_string()).collect();
    let mut out = format!("[{},[{}]", c.dir, cs.join(","));
    if !c.alive {
        let (x, y) = c.crash.unwrap_or((c.x, c.y));
        out.push_str(&format!(",[{x},{y}]"));
    }
    out.push(']');
    out
}

fn unpack_cycle(grid: &mut [u8], raw: &Value, v: u8) -> Option<Cycle> {
    let raw = raw.as_array()?;
    if raw.len() < 2 || raw.len() > 3 {
        return None;
    }
    let dir = is_dir(raw.first())?;
    let path = wall(grid, raw.get(1), v)?;
    let head = *path.last().unwrap();
    let cycle = Cycle { x: head % COLS, y: head / COLS, dir, queue: vec![], alive: true, path, crash: None };
    if raw.len() == 2 {
        return Some(cycle);
    }
    let hit = raw[2].as_array()?;
    if hit.len() != 2 {
        return None;
    }
    let (x, y) = (hit[0].as_f64()?, hit[1].as_f64()?);
    // a crash may sit one cell outside the grid
    if x.fract() != 0.0 || y.fract() != 0.0 {
        return None;
    }
    if x < -1.0 || x > f64::from(COLS) || y < -1.0 || y > f64::from(ROWS) {
        return None;
    }
    Some(Cycle { alive: false, crash: Some((x as i32, y as i32)), ..cycle })
}

fn unpack_round(tick: Option<&Value>, raw: Option<&Value>) -> Option<Round> {
    let tick = count(tick, f64::from(MAX_TICK))? as u32;
    let raw = raw?.as_array()?;
    if raw.len() != 2 {
        return None;
    }
    let mut grid = vec![0u8; (COLS * ROWS) as usize];
    let a = unpack_cycle(&mut grid, &raw[0], 1)?;
    let b = unpack_cycle(&mut grid, &raw[1], 2)?;
    // a cycle that's still going has moved every tick; one that crashed stopped a tick short
    let want = |c: &Cycle| tick as usize + usize::from(c.alive);
    if a.path.len() != want(&a) || b.path.len() != want(&b) {
        return None;
    }
    let result = match (a.alive, b.alive) {
        (false, false) => Some(RoundResult { winner: None }),
        (false, true) => Some(RoundResult { winner: Some(1) }),
        (true, false) => Some(RoundResult { winner: Some(0) }),
        _ => None,
    };
    Some(Round { grid, cycles: [a, b], tick, result })
}

fn pack_frames(frames: &[Frame]) -> String {
    frames.iter().map(|f| f.map_or('.', |d| char::from(b'0' + d))).collect()
}

fn unpack_frames(raw: Option<&Value>) -> Option<Vec<Frame>> {
    let s = raw?.as_str()?;
    if s.chars().count() > MAX_FRAMES || !s.chars().all(|c| c == '.' || ('0'..='3').contains(&c)) {
        return None;
    }
    Some(s.chars().map(|c| if c == '.' { None } else { Some(c as u8 - b'0') }).collect())
}

fn n(x: f64) -> String {
    jsnum::to_string(x)
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// encodeTrails: JSON.stringify's own key order, so the text is the web build's
pub fn encode_trails(m: &TrailsMsg) -> String {
    match m {
        TrailsMsg::Hi { v, g, ok } => {
            let mut s = format!("{{\"t\":\"tr\",\"v\":{}", n(*v));
            if *g {
                s.push_str(",\"g\":\"trails\"");
            }
            if *ok {
                s.push_str(",\"ok\":true");
            }
            s + "}"
        }
        TrailsMsg::Ping { c } => format!("{{\"t\":\"trp\",\"c\":{}}}", n(*c)),
        TrailsMsg::Pong { c } => format!("{{\"t\":\"trq\",\"c\":{}}}", n(*c)),
        TrailsMsg::Keep { e } => format!("{{\"t\":\"trk\",\"e\":{e}}}"),
        TrailsMsg::Again { e } => format!("{{\"t\":\"tra\",\"e\":{e}}}"),
        TrailsMsg::Bye => "{\"t\":\"bye\"}".into(),
        TrailsMsg::Other { game } => format!("{{\"t\":\"other\",\"game\":{}}}", quote(game)),
        TrailsMsg::Frames { e, r, f, i, a, h } => {
            let mut s = String::from("{\"t\":\"trf\"");
            if *e != 0 {
                s.push_str(&format!(",\"e\":{e}"));
            }
            s.push_str(&format!(",\"r\":{r},\"f\":{f},\"i\":{},\"a\":{a}", quote(&pack_frames(i))));
            if let Some((t, hash)) = h {
                s.push_str(&format!(",\"h\":[{t},{hash}]"));
            }
            s + "}"
        }
        TrailsMsg::Snap(m) => {
            let ph = PHASES.iter().position(|p| *p == m.phase).unwrap_or(0);
            format!(
                "{{\"t\":\"trs\",\"e\":{},\"r\":{},\"d\":{},\"s\":[{},{}],\"ph\":{ph},\"w\":{},\"k\":{},\"c\":[{},{}],\"q\":[{},{}],\"h\":{}}}",
                m.e,
                m.r,
                m.d,
                m.score[0],
                m.score[1],
                n(jsnum::round(m.wait).max(0.0)),
                m.round.tick,
                pack_cycle(&m.round.cycles[0]),
                pack_cycle(&m.round.cycles[1]),
                quote(&pack_frames(&m.frames[0])),
                quote(&pack_frames(&m.frames[1])),
                m.hash
            )
        }
    }
}

/// decodeTrails: anything malformed is None, never a panic: this is text from another machine
pub fn decode_trails(raw: &str) -> Option<TrailsMsg> {
    if crate::app::js::len16(raw) > MAX_TEXT {
        return None;
    }
    let parsed: Value = serde_json::from_str(raw).ok()?;
    let o: &Map<String, Value> = parsed.as_object()?;
    if let Some(game) = other_hello(o, LinkedGame::Trails) {
        return Some(TrailsMsg::Other { game: game.into() });
    }
    let get = |k: &str| o.get(k);
    match o.get("t")?.as_str()? {
        "tr" => {
            let v = count(get("v"), MAX_COUNT)?;
            Some(TrailsMsg::Hi { v, g: false, ok: get("ok") == Some(&Value::Bool(true)) })
        }
        "trp" => Some(TrailsMsg::Ping { c: count(get("c"), 9007199254740991.0)? }),
        "trq" => Some(TrailsMsg::Pong { c: count(get("c"), 9007199254740991.0)? }),
        "trk" => Some(TrailsMsg::Keep { e: count(get("e"), MAX_COUNT)? as u32 }),
        "tra" => Some(TrailsMsg::Again { e: count(get("e"), MAX_COUNT)? as u32 }),
        "bye" => Some(TrailsMsg::Bye),
        "trf" => {
            let e = match get("e") {
                None => 0.0,
                some => count(some, MAX_COUNT)?,
            };
            let i = unpack_frames(get("i"))?;
            let r = count(get("r"), MAX_COUNT)?;
            let f = count(get("f"), f64::from(MAX_TICK))?;
            if f + i.len() as f64 > f64::from(MAX_TICK + MAX_DELAY) {
                return None;
            }
            let a = num(get("a"))?;
            if a.fract() != 0.0 || a < -1.0 || a > f64::from(MAX_TICK + MAX_DELAY) {
                return None;
            }
            let h = match get("h") {
                None => None,
                Some(h) => {
                    let h = h.as_array()?;
                    if h.len() != 2 {
                        return None;
                    }
                    Some((count(h.first(), f64::from(MAX_TICK))? as u32, count(h.get(1), U32)? as u32))
                }
            };
            Some(TrailsMsg::Frames { e: e as u32, r: r as u32, f: f as u32, i, a: a as i64, h })
        }
        "trs" => {
            let phase = PHASES[count(get("ph"), 3.0)? as usize];
            let round = unpack_round(get("k"), get("c"))?;
            let e = count(get("e"), MAX_COUNT)?;
            let r = count(get("r"), MAX_COUNT)?;
            let d = count(get("d"), f64::from(MAX_DELAY))?;
            if e < 1.0 || d < 1.0 {
                return None;
            }
            let w = count(get("w"), MAX_WAIT_MS)?;
            let hash = count(get("h"), U32)?;
            let s = get("s")?.as_array()?;
            if s.len() != 2 {
                return None;
            }
            let s0 = count(s.first(), f64::from(WIN_ROUNDS))?;
            let s1 = count(s.get(1), f64::from(WIN_ROUNDS))?;
            let q = get("q")?.as_array()?;
            if q.len() != 2 {
                return None;
            }
            let mine = unpack_frames(q.first())?;
            let theirs = unpack_frames(q.get(1))?;
            // a round that's over goes with the score it made, and one still going with neither
            if round.result.is_some() != matches!(phase, NetPhase::Point | NetPhase::Over) {
                return None;
            }
            if phase == NetPhase::Count && round.tick != 0 {
                return None;
            }
            Some(TrailsMsg::Snap(Box::new(Snap {
                e: e as u32,
                r: r as u32,
                d: d as u32,
                score: [s0 as u32, s1 as u32],
                phase,
                wait: w,
                round,
                frames: [mine, theirs],
                hash: hash as u32,
            })))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{new_round, queue_turn, step};
    use super::*;

    #[test]
    fn a_snapshot_round_trips() {
        let mut r = new_round();
        for t in 0..9 {
            if t == 3 {
                r = queue_turn(&r, 0, 0);
            }
            r = step(&r);
        }
        let snap = Snap { e: 2, r: 0, d: 2, score: [1, 0], phase: NetPhase::Run, wait: 0.0, round: r.clone(), frames: [vec![None, Some(2)], vec![]], hash: 7 };
        let text = encode_trails(&TrailsMsg::Snap(Box::new(snap.clone())));
        let back = decode_trails(&text).unwrap();
        let TrailsMsg::Snap(b) = back else { panic!("{text}") };
        assert_eq!(b.round.grid, r.grid);
        assert_eq!(b.round.cycles[0].path, r.cycles[0].path);
        assert_eq!(b.frames, snap.frames);
    }

    #[test]
    fn frames_encode_like_the_web() {
        let m = TrailsMsg::Frames { e: 0, r: 3, f: 10, i: vec![None, Some(1), None], a: -1, h: Some((16, 99)) };
        assert_eq!(encode_trails(&m), r#"{"t":"trf","r":3,"f":10,"i":".1.","a":-1,"h":[16,99]}"#);
        assert_eq!(decode_trails(&encode_trails(&m)), Some(m));
        assert_eq!(encode_trails(&TrailsMsg::Hi { v: 1.0, g: true, ok: true }), r#"{"t":"tr","v":1,"g":"trails","ok":true}"#);
        assert_eq!(decode_trails(r#"{"t":"c4","v":1}"#), Some(TrailsMsg::Other { game: "connect 4".into() }));
        assert_eq!(decode_trails("not json"), None);
        assert_eq!(decode_trails(r#"{"t":"trf","r":0,"f":0,"i":"4","a":0}"#), None);
    }
}
