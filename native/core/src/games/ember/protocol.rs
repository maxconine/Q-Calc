//! Ember & frost over the peer link, as json text (`src/lib/emberProtocol.ts`). The host is authoritative: it
//! streams state at STATE_HZ, the guest sends its keys when they change. A state message carries the game in two
//! parts: `g`, the fast part, in every message, and `k`, the slow part, only when it changed (twice running), on
//! a key frame every KEY_EVERY messages, and when the guest says it's missing. The text is the web build's, key
//! for key, so a Mac and a browser play each other.

use std::fmt::Write;

use serde_json::{Map, Value};

use super::LEVELS;
use super::engine::mover_pos;
use super::types::*;
use crate::games::hello::{LinkedGame, other_hello};
use crate::jsnum;

pub const WIRE_VERSION: f64 = 1.0;
pub const SILENCE_MS: f64 = 3000.0;
pub const STATE_HZ: f64 = 30.0;
pub const KEY_EVERY: u32 = 60;
pub const SLOW_REPEAT: u32 = 2;

/// what the other side did, for the small note line
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmberNote {
    Play,
    Retry,
    Restart,
    Next,
    Menu,
}

const NOTES: [EmberNote; 5] = [
    EmberNote::Play,
    EmberNote::Retry,
    EmberNote::Restart,
    EmberNote::Next,
    EmberNote::Menu,
];

impl EmberNote {
    pub fn name(self) -> &'static str {
        match self {
            EmberNote::Play => "play",
            EmberNote::Retry => "retry",
            EmberNote::Restart => "restart",
            EmberNote::Next => "next",
            EmberNote::Menu => "menu",
        }
    }
}

/// a guest key change: the guest frame it starts at, and the held bits
pub type KeyChange = (i64, i64);

/// the fast part, as it travels
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fast {
    pub t: f64,
    pub pt: f64,
    pub ph: Option<f64>,
    pub p: [Vec<f64>; 2],
    pub bt: Option<Vec<f64>>,
    pub gt: Option<Vec<f64>>,
    pub mv: Option<Vec<Vec<f64>>>,
    pub bx: Option<Vec<Vec<f64>>>,
    pub ic: Option<Vec<f64>>,
    pub th: Option<Vec<f64>>,
    pub ar: Option<Vec<f64>>,
    pub cr: Option<Vec<f64>>,
    pub cn: Option<Vec<f64>>,
    pub lt: Option<f64>,
}

/// the slow part, as it travels
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slow {
    pub l: f64,
    pub st: Option<f64>,
    pub d: Option<f64>,
    pub bk: Option<[f64; 2]>,
    pub gm: Option<String>,
    pub lv: Option<String>,
    pub mi: Option<String>,
    pub dk: bool,
    pub ky: Option<f64>,
    pub kt: Option<String>,
    pub lk: Option<String>,
    pub cp: Option<f64>,
    pub on: Option<String>,
    pub ox: Option<Vec<String>>,
    pub pg: Option<[f64; 2]>,
    pub gv: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateMsg {
    /// the attempt
    pub a: f64,
    /// the host frame this state is after
    pub f: f64,
    /// ember's held keys
    pub fi: f64,
    /// the least slack of the guest keys heard since the last state, and the guest frame that message carried
    pub sl: Option<(f64, f64)>,
    /// which slow part this state goes with
    pub r: f64,
    pub k: Option<Slow>,
    pub g: Fast,
    /// events since the last state message
    pub e: Vec<GameEvent>,
    /// the message before this one: its f and its e
    pub q: Option<(f64, Vec<GameEvent>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmberMsg {
    /// ok: the sender already heard the other side's hi. g: tagged 'ember' (sent, never decoded)
    Hi {
        v: f64,
        w: f64,
        g: bool,
        ok: bool,
    },
    Bye,
    Sel {
        l: f64,
    },
    State(Box<StateMsg>),
    /// guest: its keys for attempt a, sent at guest frame f; i: the last few changes; r: the slow part it holds
    K {
        a: f64,
        f: f64,
        i: Vec<KeyChange>,
        r: Option<f64>,
    },
    Pick {
        l: f64,
    },
    Go {
        l: f64,
    },
    Retry,
    Restart,
    Next,
    Menu,
    Note {
        w: EmberNote,
    },
    /// another game's hello (only ever decoded)
    Other {
        game: String,
    },
}

// ---- limits for text from another machine

pub const MAX_TEXT: usize = 16_000;
const MAX_FRAME: f64 = 1e9;
const MAX_COUNT: f64 = 1e9;
const MAX_TICKS: f64 = 1e5;
const MAX_THINGS: f64 = 512.0;
const MAX_SPARSE: usize = 1024;
const MAX_EVENTS: usize = 64;
const MAX_CHANNELS: usize = 128;
const MAX_CHANGES: usize = 8;
const MAX_TILE: f64 = 128.0 * 72.0 - 1.0;
const POS: f64 = 5000.0;
const VEL: f64 = 5000.0;

const PHASES: [Phase; 4] = [Phase::Play, Phase::Dead, Phase::Won, Phase::Clear];
const ELS: [Element; 2] = [Element::Fire, Element::Frost];

fn r1(n: f64) -> f64 {
    jsnum::round(n * 10.0) / 10.0
}
fn r2(n: f64) -> f64 {
    jsnum::round(n * 100.0) / 100.0
}
fn r4(n: f64) -> f64 {
    jsnum::round(n * 1e4) / 1e4
}

fn finite(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| n.is_finite())
}
fn int(v: Option<&Value>, lo: f64, hi: f64) -> Option<f64> {
    finite(v).filter(|n| n.fract() == 0.0 && *n >= lo && *n <= hi)
}
fn within(v: Option<&Value>, lim: f64) -> Option<f64> {
    finite(v).filter(|n| n.abs() <= lim)
}
fn is_bit(v: f64) -> bool {
    v == 0.0 || v == 1.0
}
fn level_index(v: Option<&Value>, levels: usize) -> Option<f64> {
    int(v, 0.0, levels as f64 - 1.0)
}
fn only(o: &Map<String, Value>, keys: &[&str]) -> bool {
    o.keys().all(|k| keys.contains(&k.as_str()))
}

// ---- packing

/// trailing zeros are left off; the stage says how long the list really is
fn cut(mut a: Vec<f64>, keep: usize) -> Vec<f64> {
    while a.len() > keep && a.last() == Some(&0.0) {
        a.pop();
    }
    a
}

/// a list that is mostly zero, as index, value, index, value...
fn sparse(a: &[i64]) -> Vec<f64> {
    let mut out = vec![];
    for (i, v) in a.iter().enumerate() {
        if *v != 0 {
            out.push(i as f64);
            out.push(*v as f64);
        }
    }
    out
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// booleans as hex, four to a digit, lowest bit first, trailing zeros left off
fn mask(bs: &[bool]) -> String {
    let mut out = String::new();
    for i in (0..bs.len()).step_by(4) {
        let b = |j: usize| bs.get(i + j).copied().unwrap_or(false);
        out.push(
            HEX[usize::from(b(0))
                | usize::from(b(1)) << 1
                | usize::from(b(2)) << 2
                | usize::from(b(3)) << 3] as char,
        );
    }
    out.trim_end_matches('0').to_string()
}

/// every channel a stage can turn on, sorted: GameState.on goes as bits over this list
pub fn channels_of(stage: &Stage) -> Vec<String> {
    let mut all: Vec<String> = stage
        .plates
        .iter()
        .map(|t| t.ch.clone())
        .chain(stage.levers.iter().map(|t| t.ch.clone()))
        .chain(stage.buttons.iter().map(|t| t.ch.clone()))
        .chain(stage.sensors.iter().map(|t| t.ch.clone()))
        .collect();
    all.sort();
    all.dedup();
    all
}

fn pack_player(p: &Player) -> Vec<f64> {
    let flags = i64::from(p.ground)
        | i64::from(p.face == 1) << 1
        | i64::from(p.alive) << 2
        | i64::from(p.in_door) << 3;
    let v = [
        p.prev_in,
        p.coyote,
        p.buffer,
        p.portal_cd,
        p.air,
        p.cannon + 1,
        p.fly,
        p.cannon_cd,
    ];
    let mut out = vec![r2(p.x), r2(p.y), r1(p.vx), r1(p.vy), flags as f64];
    out.extend(v.iter().map(|n| *n as f64));
    cut(out, 5)
}

fn some(a: Vec<f64>) -> Option<Vec<f64>> {
    if a.is_empty() { None } else { Some(a) }
}

/// what changes most ticks
pub fn fast_of(stage: &Stage, s: &GameState) -> Fast {
    let mut g = Fast {
        t: s.tick as f64,
        pt: s.phase_t as f64,
        p: [pack_player(&s.players[0]), pack_player(&s.players[1])],
        ..Default::default()
    };
    if s.phase != Phase::Play {
        g.ph = PHASES.iter().position(|p| *p == s.phase).map(|i| i as f64);
    }
    g.bt = some(cut(s.buttons.iter().map(|n| *n as f64).collect(), 0));
    g.gt = some(cut(s.gates.iter().map(|n| r4(*n)).collect(), 0));
    if !s.movers.is_empty() {
        g.mv = Some(
            s.movers
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let t = r4(m.t);
                    let there = stage.movers.get(i).is_some_and(|def| {
                        let at = mover_pos(def, t);
                        (at.x - m.x).abs() < 0.005 && (at.y - m.y).abs() < 0.005
                    });
                    if there {
                        if m.dir == 1 {
                            vec![t]
                        } else {
                            vec![t, m.dir as f64]
                        }
                    } else {
                        vec![t, m.dir as f64, r2(m.x), r2(m.y)]
                    }
                })
                .collect(),
        );
    }
    if !s.boxes.is_empty() {
        g.bx = Some(
            s.boxes
                .iter()
                .map(|b| cut(vec![r2(b.x), r2(b.y), r1(b.vx), r1(b.vy)], 2))
                .collect(),
        );
    }
    g.ic = some(cut(s.ice.iter().map(|n| r4(*n)).collect(), 0));
    g.th = some(sparse(&s.thin));
    g.ar = some(sparse(&s.arrows));
    g.cr = some(sparse(&s.crumble));
    g.cn = some(sparse(&s.cannons));
    if s.lit_t != 0 {
        g.lt = Some(s.lit_t as f64);
    }
    g
}

fn nonempty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// what changes now and then. two states with the same slow part give the same JSON, key for key
pub fn slow_of(l: usize, stage: &Stage, s: &GameState) -> Slow {
    let mut k = Slow {
        l: l as f64,
        ..Default::default()
    };
    if s.stage != 0 {
        k.st = Some(s.stage as f64);
    }
    if s.deaths != 0 {
        k.d = Some(s.deaths as f64);
    }
    if s.banked[0] != 0 || s.banked[1] != 0 {
        k.bk = Some([s.banked[0] as f64, s.banked[1] as f64]);
    }
    k.gm = nonempty(mask(&s.taken));
    k.lv = nonempty(mask(&s.levers));
    k.mi = nonempty(mask(
        &s.mirrors
            .iter()
            .map(|m| *m == Slant::Back)
            .collect::<Vec<_>>(),
    ));
    k.dk = !s.lit;
    if s.keys != 0 {
        k.ky = Some(s.keys as f64);
    }
    k.kt = nonempty(mask(&s.key_taken));
    k.lk = nonempty(mask(&s.locks));
    if s.cp >= 0 {
        k.cp = Some(s.cp as f64);
    }
    let names = channels_of(stage);
    k.on = nonempty(mask(
        &names.iter().map(|c| s.on.contains(c)).collect::<Vec<_>>(),
    ));
    let ox: Vec<String> =
        s.on.iter()
            .filter(|c| !names.contains(c))
            .cloned()
            .collect();
    if !ox.is_empty() {
        k.ox = Some(ox);
    }
    let [a, b] = &s.players;
    if a.gems != 0 || b.gems != 0 {
        k.pg = Some([a.gems as f64, b.gems as f64]);
    }
    let gv = i64::from(a.grav == -1) | i64::from(b.grav == -1) << 1;
    if gv != 0 {
        k.gv = Some(gv as f64);
    }
    k
}

// ---- writing, in JSON.stringify's key order

fn n(out: &mut String, x: f64) {
    out.push_str(&jsnum::to_string(if x == 0.0 { 0.0 } else { x }));
}

fn ns(out: &mut String, v: &[f64]) {
    out.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        n(out, *x);
    }
    out.push(']');
}

fn q(out: &mut String, s: &str) {
    out.push_str(&serde_json::to_string(s).unwrap_or_default());
}

fn key_nums(out: &mut String, key: &str, v: &Option<Vec<f64>>) {
    if let Some(v) = v {
        let _ = write!(out, ",\"{key}\":");
        ns(out, v);
    }
}

fn key_lists(out: &mut String, key: &str, v: &Option<Vec<Vec<f64>>>) {
    if let Some(v) = v {
        let _ = write!(out, ",\"{key}\":[");
        for (i, x) in v.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            ns(out, x);
        }
        out.push(']');
    }
}

pub fn fast_json(g: &Fast) -> String {
    let mut out = String::from("{\"T\":");
    n(&mut out, g.t);
    out.push_str(",\"pt\":");
    n(&mut out, g.pt);
    out.push_str(",\"p\":[");
    ns(&mut out, &g.p[0]);
    out.push(',');
    ns(&mut out, &g.p[1]);
    out.push(']');
    if let Some(ph) = g.ph {
        out.push_str(",\"ph\":");
        n(&mut out, ph);
    }
    key_nums(&mut out, "bt", &g.bt);
    key_nums(&mut out, "gt", &g.gt);
    key_lists(&mut out, "mv", &g.mv);
    key_lists(&mut out, "bx", &g.bx);
    key_nums(&mut out, "ic", &g.ic);
    key_nums(&mut out, "th", &g.th);
    key_nums(&mut out, "ar", &g.ar);
    key_nums(&mut out, "cr", &g.cr);
    key_nums(&mut out, "cn", &g.cn);
    if let Some(lt) = g.lt {
        out.push_str(",\"lt\":");
        n(&mut out, lt);
    }
    out.push('}');
    out
}

pub fn slow_json(k: &Slow) -> String {
    let mut out = String::from("{\"l\":");
    n(&mut out, k.l);
    let num = |out: &mut String, key: &str, v: Option<f64>| {
        if let Some(v) = v {
            let _ = write!(out, ",\"{key}\":");
            n(out, v);
        }
    };
    let text = |out: &mut String, key: &str, v: &Option<String>| {
        if let Some(v) = v {
            let _ = write!(out, ",\"{key}\":");
            q(out, v);
        }
    };
    let pair = |out: &mut String, key: &str, v: Option<[f64; 2]>| {
        if let Some(v) = v {
            let _ = write!(out, ",\"{key}\":");
            ns(out, &v);
        }
    };
    num(&mut out, "st", k.st);
    num(&mut out, "d", k.d);
    pair(&mut out, "bk", k.bk);
    text(&mut out, "gm", &k.gm);
    text(&mut out, "lv", &k.lv);
    text(&mut out, "mi", &k.mi);
    if k.dk {
        out.push_str(",\"dk\":1");
    }
    num(&mut out, "ky", k.ky);
    text(&mut out, "kt", &k.kt);
    text(&mut out, "lk", &k.lk);
    num(&mut out, "cp", k.cp);
    text(&mut out, "on", &k.on);
    if let Some(ox) = &k.ox {
        out.push_str(",\"ox\":[");
        for (i, c) in ox.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            q(&mut out, c);
        }
        out.push(']');
    }
    pair(&mut out, "pg", k.pg);
    num(&mut out, "gv", k.gv);
    out.push('}');
    out
}

// ---- checking what arrived: shape and range here, sizes in join_state

fn list<T>(
    v: Option<&Value>,
    max: usize,
    mut each: impl FnMut(&Value) -> Option<T>,
) -> Option<Vec<T>> {
    let Some(v) = v else { return Some(vec![]) };
    let a = v.as_array()?;
    if a.len() > max {
        return None;
    }
    a.iter().map(&mut each).collect()
}

fn frac(v: &Value) -> Option<f64> {
    finite(Some(v)).filter(|n| (0.0..=1.0).contains(n))
}

fn ticks(v: &Value) -> Option<f64> {
    int(Some(v), 0.0, MAX_TICKS)
}

/// x y vx vy flags prevIn coyote buffer portalCd air cannon+1 fly cannonCd
const PLAYER_MAX: [f64; 13] = [
    POS, POS, VEL, VEL, 15.0, 15.0, MAX_TICKS, MAX_TICKS, MAX_TICKS, 1.0, MAX_THINGS, MAX_TICKS,
    MAX_TICKS,
];

fn check_player(v: Option<&Value>) -> Option<Vec<f64>> {
    let a = v?.as_array()?;
    if a.len() < 5 || a.len() > PLAYER_MAX.len() {
        return None;
    }
    a.iter()
        .enumerate()
        .map(|(i, x)| {
            if i < 4 {
                within(Some(x), PLAYER_MAX[i])
            } else {
                int(Some(x), 0.0, PLAYER_MAX[i])
            }
        })
        .collect()
}

/// index, value pairs: indices rising, values whole and above zero
fn check_sparse(v: Option<&Value>) -> Option<Vec<f64>> {
    let Some(v) = v else { return Some(vec![]) };
    let a = v.as_array()?;
    if a.len() % 2 != 0 || a.len() > MAX_SPARSE * 2 {
        return None;
    }
    let mut last = -1.0;
    let mut out = vec![];
    for pair in a.chunks(2) {
        let i = int(pair.first(), last + 1.0, MAX_TILE)?;
        let n = int(pair.get(1), 1.0, MAX_TICKS)?;
        last = i;
        out.push(i);
        out.push(n);
    }
    Some(out)
}

const FAST_KEYS: [&str; 14] = [
    "T", "pt", "ph", "p", "bt", "gt", "mv", "bx", "ic", "th", "ar", "cr", "cn", "lt",
];

fn opt(v: Vec<f64>) -> Option<Vec<f64>> {
    some(v)
}

fn check_fast(o: Option<&Value>) -> Option<Fast> {
    let o = o?.as_object()?;
    if !only(o, &FAST_KEYS) {
        return None;
    }
    let t = int(o.get("T"), 0.0, MAX_COUNT)?;
    let pt = int(o.get("pt"), 0.0, MAX_COUNT)?;
    let ph = match o.get("ph") {
        None => None,
        v => Some(int(v, 1.0, PHASES.len() as f64 - 1.0)?),
    };
    let lt = match o.get("lt") {
        None => None,
        v => Some(int(v, 1.0, MAX_TICKS)?),
    };
    let p = o.get("p")?.as_array()?;
    if p.len() != 2 {
        return None;
    }
    let a = check_player(p.first());
    let b = check_player(p.get(1));
    let bt = list(o.get("bt"), MAX_THINGS as usize, ticks);
    let gt = list(o.get("gt"), MAX_THINGS as usize, frac);
    let ic = list(o.get("ic"), MAX_THINGS as usize, frac);
    let mv = list(o.get("mv"), MAX_THINGS as usize, |m| {
        let m = m.as_array()?;
        if m.len() != 1 && m.len() != 2 && m.len() != 4 {
            return None;
        }
        let t = frac(&m[0])?;
        let mut out = vec![t];
        if m.len() > 1 {
            let d = m[1].as_f64()?;
            if d != 1.0 && d != -1.0 {
                return None;
            }
            out.push(d);
        }
        if m.len() == 4 {
            out.push(within(m.get(2), POS)?);
            out.push(within(m.get(3), POS)?);
        }
        Some(out)
    });
    let bx = list(o.get("bx"), MAX_THINGS as usize, |m| {
        let m = m.as_array()?;
        if m.len() < 2 || m.len() > 4 {
            return None;
        }
        m.iter()
            .enumerate()
            .map(|(i, x)| within(Some(x), if i < 2 { POS } else { VEL }))
            .collect()
    });
    let th = check_sparse(o.get("th"));
    let ar = check_sparse(o.get("ar"));
    let cr = check_sparse(o.get("cr"));
    let cn = check_sparse(o.get("cn"));
    let (a, b, bt, gt, ic, mv, bx, th, ar, cr, cn) =
        (a?, b?, bt?, gt?, ic?, mv?, bx?, th?, ar?, cr?, cn?);
    Some(Fast {
        t,
        pt,
        ph,
        p: [a, b],
        bt: opt(bt),
        gt: opt(gt),
        mv: if mv.is_empty() { None } else { Some(mv) },
        bx: if bx.is_empty() { None } else { Some(bx) },
        ic: opt(ic),
        th: opt(th),
        ar: opt(ar),
        cr: opt(cr),
        cn: opt(cn),
        lt,
    })
}

const SLOW_KEYS: [&str; 16] = [
    "l", "st", "d", "bk", "gm", "lv", "mi", "dk", "ky", "kt", "lk", "cp", "on", "ox", "pg", "gv",
];

fn is_mask(v: Option<&Value>) -> Option<Option<String>> {
    match v {
        None => Some(None),
        Some(Value::String(s))
            if !s.is_empty()
                && s.len() <= (MAX_THINGS / 4.0) as usize
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) =>
        {
            Some(Some(s.clone()))
        }
        _ => None,
    }
}

fn pair_of(v: Option<&Value>, max: f64) -> Option<Option<[f64; 2]>> {
    let Some(v) = v else { return Some(None) };
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    Some(Some([int(a.first(), 0.0, max)?, int(a.get(1), 0.0, max)?]))
}

fn check_slow(o: &Value, levels: usize) -> Option<Slow> {
    let o = o.as_object()?;
    if !only(o, &SLOW_KEYS) {
        return None;
    }
    let l = level_index(o.get("l"), levels)?;
    let num = |key: &str, lo: f64, hi: f64| -> Option<Option<f64>> {
        match o.get(key) {
            None => Some(None),
            v => Some(Some(int(v, lo, hi)?)),
        }
    };
    let st = num("st", 1.0, 63.0)?;
    let d = num("d", 1.0, MAX_COUNT)?;
    let ky = num("ky", 1.0, MAX_THINGS)?;
    let cp = num("cp", 0.0, MAX_THINGS)?;
    let gv = num("gv", 1.0, 3.0)?;
    let dk = match o.get("dk") {
        None => false,
        Some(v) if v.as_f64() == Some(1.0) => true,
        _ => return None,
    };
    let gm = is_mask(o.get("gm"))?;
    let lv = is_mask(o.get("lv"))?;
    let mi = is_mask(o.get("mi"))?;
    let kt = is_mask(o.get("kt"))?;
    let lk = is_mask(o.get("lk"))?;
    let on = is_mask(o.get("on"))?;
    let bk = pair_of(o.get("bk"), MAX_COUNT)?;
    let pg = pair_of(o.get("pg"), MAX_THINGS)?;
    let ox = list(o.get("ox"), MAX_CHANNELS, |c| {
        c.as_str()
            .filter(|s| crate::app::js::len16(s) <= 64)
            .map(String::from)
    })?;
    Some(Slow {
        l,
        st,
        d,
        bk,
        gm,
        lv,
        mi,
        dk,
        ky,
        kt,
        lk,
        cp,
        on,
        ox: if ox.is_empty() { None } else { Some(ox) },
        pg,
        gv,
    })
}

// ---- a state out of its two parts, against the stage it's for

fn unmask(v: &Option<String>, len: usize) -> Option<Vec<bool>> {
    let mut out = vec![false; len];
    let Some(v) = v else { return Some(out) };
    for (i, c) in v.bytes().enumerate() {
        let n = HEX.iter().position(|h| *h == c).map_or(-1i32, |p| p as i32);
        for j in 0..4 {
            if (n >> j) & 1 == 0 {
                continue;
            }
            if i * 4 + j >= len {
                return None;
            }
            out[i * 4 + j] = true;
        }
    }
    Some(out)
}

fn fill(v: &Option<Vec<f64>>, len: usize) -> Option<Vec<f64>> {
    let Some(v) = v else {
        return Some(vec![0.0; len]);
    };
    if v.len() > len {
        return None;
    }
    let mut out = v.clone();
    out.resize(len, 0.0);
    Some(out)
}

fn unsparse(v: &Option<Vec<f64>>, len: usize) -> Option<Vec<i64>> {
    let mut out = vec![0; len];
    let Some(v) = v else { return Some(out) };
    for pair in v.chunks(2) {
        let i = pair[0] as usize;
        if i >= len {
            return None;
        }
        out[i] = pair.get(1).copied().unwrap_or(0.0) as i64;
    }
    Some(out)
}

fn join_player(a: &[f64], grav: i64, gems: i64, cannons: usize) -> Option<Player> {
    let at = |i: usize| a.get(i).copied().unwrap_or(0.0);
    let flags = at(4) as i64;
    if at(10) > cannons as f64 {
        return None;
    }
    Some(Player {
        x: at(0),
        y: at(1),
        vx: at(2),
        vy: at(3),
        ground: flags & 1 != 0,
        face: if flags & 2 != 0 { 1 } else { -1 },
        alive: flags & 4 != 0,
        in_door: flags & 8 != 0,
        coyote: at(6) as i64,
        buffer: at(7) as i64,
        prev_in: at(5) as i64,
        portal_cd: at(8) as i64,
        gems,
        grav,
        air: at(9) as i64,
        cannon: at(10) as i64 - 1,
        fly: at(11) as i64,
        cannon_cd: at(12) as i64,
    })
}

pub fn join_state(level: &Level, k: &Slow, g: &Fast) -> Option<GameState> {
    let stage = level.stages.get(k.st.unwrap_or(0.0) as usize)?;
    let gv = k.gv.unwrap_or(0.0) as i64;
    let pg = k.pg.unwrap_or([0.0, 0.0]);
    let a = join_player(
        &g.p[0],
        if gv & 1 != 0 { -1 } else { 1 },
        pg[0] as i64,
        stage.cannons.len(),
    );
    let b = join_player(
        &g.p[1],
        if gv & 2 != 0 { -1 } else { 1 },
        pg[1] as i64,
        stage.cannons.len(),
    );
    let taken = unmask(&k.gm, stage.gems.len());
    let levers = unmask(&k.lv, stage.levers.len());
    let mirrors = unmask(&k.mi, stage.mirrors.len());
    let key_taken = unmask(&k.kt, stage.keys.len());
    let locks = unmask(&k.lk, stage.locks.len());
    let names = channels_of(stage);
    let on = unmask(&k.on, names.len());
    let buttons = fill(&g.bt, stage.buttons.len());
    let gates = fill(&g.gt, stage.gates.len());
    let ice = fill(&g.ic, stage.ice.len());
    let thin = unsparse(&g.th, stage.thin.len());
    let arrows = unsparse(&g.ar, stage.arrows.len());
    let crumble = unsparse(&g.cr, stage.crumble.len());
    let cannons = unsparse(&g.cn, stage.cannons.len());
    let (a, b, taken, levers, mirrors, key_taken, locks, on) =
        (a?, b?, taken?, levers?, mirrors?, key_taken?, locks?, on?);
    let (buttons, gates, ice, thin, arrows, crumble, cannons) =
        (buttons?, gates?, ice?, thin?, arrows?, crumble?, cannons?);
    if g.mv.as_ref().map_or(0, Vec::len) != stage.movers.len()
        || g.bx.as_ref().map_or(0, Vec::len) != stage.boxes.len()
    {
        return None;
    }
    if k.cp.unwrap_or(-1.0) >= stage.checkpoints.len() as f64 {
        return None;
    }
    let mut on_list: Vec<String> = names
        .iter()
        .zip(&on)
        .filter(|(_, b)| **b)
        .map(|(c, _)| c.clone())
        .collect();
    on_list.extend(k.ox.clone().unwrap_or_default());
    on_list.sort();
    Some(GameState {
        tick: g.t as i64,
        phase: PHASES[g.ph.unwrap_or(0.0) as usize],
        phase_t: g.pt as i64,
        deaths: k.d.unwrap_or(0.0) as i64,
        stage: stage.index,
        banked: k.bk.map_or([0, 0], |b| [b[0] as i64, b[1] as i64]),
        players: [a, b],
        taken,
        levers,
        buttons: buttons.iter().map(|n| *n as i64).collect(),
        gates,
        movers: g
            .mv
            .clone()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let t = m[0];
                let dir = if m.get(1) == Some(&-1.0) { -1 } else { 1 };
                if m.len() == 4 {
                    MoverState {
                        x: m[2],
                        y: m[3],
                        t,
                        dir,
                    }
                } else {
                    let p = mover_pos(&stage.movers[i], t);
                    MoverState {
                        x: p.x,
                        y: p.y,
                        t,
                        dir,
                    }
                }
            })
            .collect(),
        boxes: g
            .bx
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|m| BoxState {
                x: m[0],
                y: m[1],
                vx: m.get(2).copied().unwrap_or(0.0),
                vy: m.get(3).copied().unwrap_or(0.0),
            })
            .collect(),
        ice,
        thin,
        mirrors: mirrors
            .iter()
            .map(|m| if *m { Slant::Back } else { Slant::Slash })
            .collect(),
        lit: !k.dk,
        lit_t: g.lt.unwrap_or(0.0) as i64,
        arrows,
        crumble,
        keys: k.ky.unwrap_or(0.0) as i64,
        key_taken,
        locks,
        cannons,
        cp: k.cp.map_or(-1, |c| c as i64),
        saved: None,
        on: on_list,
        events: vec![],
        plates: None,
        sensors: None,
        is_snapshot: false,
    })
}

// ---- events

fn px(x: Option<f64>) -> f64 {
    jsnum::round(x.unwrap_or(0.0))
}

fn pack_event(e: &GameEvent) -> Vec<f64> {
    use EventKind as K;
    let k = e.k.index() as f64;
    let p = e.p.unwrap_or(0) as f64;
    let i = e.i.unwrap_or(0) as f64;
    let bit = |b: Option<bool>| if b == Some(true) { 1.0 } else { 0.0 };
    match e.k {
        K::Win => vec![k],
        K::Clear | K::Stage => vec![k, e.stage.unwrap_or(0) as f64],
        K::Respawn => vec![k, e.cp.unwrap_or(-1) as f64],
        K::Gem => vec![
            k,
            p,
            px(e.x),
            px(e.y),
            ELS.iter().position(|x| Some(*x) == e.el).unwrap_or(0) as f64,
        ],
        K::Bounce => vec![k, p, px(e.x), px(e.y), jsnum::round(e.v.unwrap_or(0.0))],
        K::Flip => vec![k, p, px(e.x), px(e.y), e.grav.unwrap_or(1) as f64],
        K::Arrow | K::Key | K::Load | K::Fire => vec![k, p, i, px(e.x), px(e.y)],
        K::Light => vec![k, bit(e.on), px(e.x), px(e.y)],
        K::Lever | K::Mirror | K::Melt | K::Gswitch | K::Arrowback | K::Unlock | K::Checkpoint => {
            vec![k, i, px(e.x), px(e.y)]
        }
        K::Plate | K::Button | K::Sensor => vec![k, i, bit(e.on), px(e.x), px(e.y)],
        K::Freeze | K::Thaw | K::Shake | K::Break | K::Mend => {
            vec![k, e.tile.unwrap_or(0) as f64, px(e.x), px(e.y)]
        }
        _ => {
            let mut v = vec![k, p, px(e.x), px(e.y)];
            if let Some(c) = e.cause {
                v.push(CAUSES.iter().position(|x| *x == c).unwrap_or(0) as f64);
            }
            v
        }
    }
}

fn unpack_event(v: &Value) -> Option<GameEvent> {
    use EventKind as K;
    let a = v.as_array()?;
    if a.is_empty() || a.len() > 5 {
        return None;
    }
    let n: Vec<f64> = a.iter().map(|x| finite(Some(x))).collect::<Option<_>>()?;
    let len = n.len();
    let get = |i: usize| n.get(i).copied();
    let isint =
        |i: usize, lo: f64, hi: f64| get(i).is_some_and(|x| x.fract() == 0.0 && x >= lo && x <= hi);
    let k = *KINDS
        .get(n[0] as usize)
        .filter(|_| isint(0, 0.0, KINDS.len() as f64 - 1.0))?;
    let xy = |i: usize| {
        get(i).is_some_and(|x| x.abs() <= POS) && get(i + 1).is_some_and(|y| y.abs() <= POS)
    };
    let idx = |i: usize| isint(i, 0.0, MAX_THINGS);
    let bit1 = get(1).is_some_and(is_bit);
    let ev = GameEvent::bare(k);
    match k {
        K::Win => (len == 1).then_some(ev),
        K::Clear | K::Stage => (len == 2 && isint(1, 0.0, 63.0)).then(|| GameEvent {
            stage: Some(n[1] as i64),
            ..ev
        }),
        K::Respawn => (len == 2 && isint(1, -1.0, MAX_THINGS)).then(|| GameEvent {
            cp: Some(n[1] as i64),
            ..ev
        }),
        K::Bounce => (len == 5 && bit1 && xy(2) && n[4].abs() <= VEL).then(|| GameEvent {
            v: Some(n[4]),
            ..GameEvent::at_player(k, n[1] as usize, n[2], n[3])
        }),
        K::Flip => {
            (len == 5 && bit1 && xy(2) && (n[4] == 1.0 || n[4] == -1.0)).then(|| GameEvent {
                grav: Some(n[4] as i64),
                ..GameEvent::at_player(k, n[1] as usize, n[2], n[3])
            })
        }
        K::Arrow | K::Key | K::Load | K::Fire => (len == 5 && bit1 && idx(2) && xy(3))
            .then(|| GameEvent::took(k, n[1] as usize, n[2] as usize, n[3], n[4])),
        K::Light => (len == 4 && bit1 && xy(2)).then(|| GameEvent {
            on: Some(n[1] == 1.0),
            x: Some(n[2]),
            y: Some(n[3]),
            ..ev
        }),
        K::Gem => {
            let el = get(4).and_then(|e| {
                if e == 0.0 {
                    Some(Element::Fire)
                } else if e == 1.0 {
                    Some(Element::Frost)
                } else {
                    None
                }
            });
            (len == 5 && bit1 && xy(2) && el.is_some()).then(|| GameEvent {
                el,
                ..GameEvent::at_player(k, n[1] as usize, n[2], n[3])
            })
        }
        K::Lever | K::Mirror | K::Melt | K::Gswitch | K::Arrowback | K::Unlock | K::Checkpoint => {
            (len == 4 && idx(1) && xy(2)).then(|| GameEvent::at_thing(k, n[1] as usize, n[2], n[3]))
        }
        K::Plate | K::Button | K::Sensor => {
            (len == 5 && idx(1) && get(2).is_some_and(is_bit) && xy(3))
                .then(|| GameEvent::switched(k, n[1] as usize, n[2] == 1.0, n[3], n[4]))
        }
        K::Freeze | K::Thaw | K::Shake | K::Break | K::Mend => {
            (len == 4 && isint(1, 0.0, MAX_TILE) && xy(2))
                .then(|| GameEvent::at_tile(k, n[1] as usize, n[2], n[3]))
        }
        _ => {
            if (len != 4 && len != 5) || !bit1 || !xy(2) {
                return None;
            }
            let cause = if len == 5 {
                Some(
                    *CAUSES
                        .get(n[4] as usize)
                        .filter(|_| n[4].fract() == 0.0 && n[4] >= 0.0)?,
                )
            } else {
                None
            };
            Some(GameEvent {
                cause,
                ..GameEvent::at_player(k, n[1] as usize, n[2], n[3])
            })
        }
    }
}

/// an event's index points at something in this stage, so the panel can look it up without checking
pub fn event_fits(level: &Level, stage: &Stage, e: &GameEvent) -> bool {
    use EventKind as K;
    let i = e.i.unwrap_or(0).max(0) as usize;
    match e.k {
        K::Lever => i < stage.levers.len(),
        K::Mirror => i < stage.mirrors.len(),
        K::Melt => i < stage.ice.len(),
        K::Plate => i < stage.plates.len(),
        K::Button => i < stage.buttons.len(),
        K::Sensor => i < stage.sensors.len(),
        K::Gswitch => i < stage.gswitches.len(),
        K::Arrow | K::Arrowback => i < stage.arrows.len(),
        K::Key => i < stage.keys.len(),
        K::Unlock => i < stage.locks.len(),
        K::Load | K::Fire => i < stage.cannons.len(),
        K::Checkpoint => i < stage.checkpoints.len(),
        K::Respawn => e.cp.unwrap_or(-1) < stage.checkpoints.len() as i64,
        K::Freeze | K::Thaw | K::Shake | K::Break | K::Mend => {
            (e.tile.unwrap_or(0) as usize) < stage.tiles.len()
        }
        K::Clear | K::Stage => (e.stage.unwrap_or(0) as usize) < level.stages.len(),
        _ => true,
    }
}

// ---- messages

fn events_json(out: &mut String, es: &[GameEvent]) {
    let from = es.len().saturating_sub(MAX_EVENTS);
    out.push('[');
    for (j, e) in es[from..].iter().enumerate() {
        if j > 0 {
            out.push(',');
        }
        ns(out, &pack_event(e));
    }
    out.push(']');
}

pub fn encode_ember(m: &EmberMsg) -> String {
    let mut out = String::new();
    match m {
        EmberMsg::Hi { v, w, g, ok } => {
            out.push_str("{\"t\":\"hi\",\"v\":");
            n(&mut out, *v);
            out.push_str(",\"w\":");
            n(&mut out, *w);
            if *g {
                out.push_str(",\"g\":\"ember\"");
            }
            if *ok {
                out.push_str(",\"ok\":true");
            }
            out.push('}');
        }
        EmberMsg::Bye => out.push_str("{\"t\":\"bye\"}"),
        EmberMsg::Retry => out.push_str("{\"t\":\"retry\"}"),
        EmberMsg::Restart => out.push_str("{\"t\":\"restart\"}"),
        EmberMsg::Next => out.push_str("{\"t\":\"next\"}"),
        EmberMsg::Menu => out.push_str("{\"t\":\"menu\"}"),
        EmberMsg::Sel { l } | EmberMsg::Pick { l } | EmberMsg::Go { l } => {
            let t = match m {
                EmberMsg::Sel { .. } => "sel",
                EmberMsg::Pick { .. } => "pick",
                _ => "go",
            };
            let _ = write!(out, "{{\"t\":\"{t}\",\"l\":");
            n(&mut out, *l);
            out.push('}');
        }
        EmberMsg::Note { w } => {
            let _ = write!(out, "{{\"t\":\"note\",\"w\":\"{}\"}}", w.name());
        }
        EmberMsg::Other { game } => {
            out.push_str("{\"t\":\"other\",\"game\":");
            q(&mut out, game);
            out.push('}');
        }
        EmberMsg::K { a, f, i, r } => {
            out.push_str("{\"t\":\"k\",\"a\":");
            n(&mut out, *a);
            out.push_str(",\"f\":");
            n(&mut out, *f);
            out.push_str(",\"i\":[");
            for (j, (fr, bits)) in i.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                let _ = write!(out, "[{fr},{bits}]");
            }
            out.push(']');
            if let Some(r) = r {
                out.push_str(",\"r\":");
                n(&mut out, *r);
            }
            out.push('}');
        }
        EmberMsg::State(s) => {
            out.push_str("{\"t\":\"s\",\"a\":");
            n(&mut out, s.a);
            out.push_str(",\"f\":");
            n(&mut out, s.f);
            out.push_str(",\"fi\":");
            n(&mut out, s.fi);
            if let Some((sl, fr)) = s.sl {
                out.push_str(",\"sl\":");
                ns(&mut out, &[sl, fr]);
            }
            out.push_str(",\"r\":");
            n(&mut out, s.r);
            if let Some(k) = &s.k {
                out.push_str(",\"k\":");
                out.push_str(&slow_json(k));
            }
            out.push_str(",\"g\":");
            out.push_str(&fast_json(&s.g));
            if !s.e.is_empty() {
                out.push_str(",\"e\":");
                events_json(&mut out, &s.e);
            }
            if let Some((qf, qe)) = &s.q
                && !qe.is_empty()
            {
                out.push_str(",\"q\":[");
                n(&mut out, *qf);
                out.push(',');
                events_json(&mut out, qe);
                out.push(']');
            }
            out.push('}');
        }
    }
    out
}

const STATE_KEYS: [&str; 10] = ["t", "a", "f", "fi", "sl", "r", "k", "g", "e", "q"];

/// anything malformed is None, never a panic: this is text from another machine
pub fn decode_ember(raw: &str) -> Option<EmberMsg> {
    decode_ember_n(raw, LEVELS.len())
}

/// decode_ember against a level list of this length
pub fn decode_ember_n(raw: &str, levels: usize) -> Option<EmberMsg> {
    if crate::app::js::len16(raw) > MAX_TEXT {
        return None;
    }
    let m: Value = serde_json::from_str(raw).ok()?;
    let o = m.as_object()?;
    // a hello without a version is nobody's
    if finite(o.get("v")).is_some()
        && let Some(game) = other_hello(o, LinkedGame::Ember)
    {
        return Some(EmberMsg::Other { game: game.into() });
    }
    match o.get("t")?.as_str()? {
        "hi" => {
            let v = finite(o.get("v"))?;
            Some(EmberMsg::Hi {
                v,
                w: finite(o.get("w")).unwrap_or(0.0),
                g: false,
                ok: o.get("ok") == Some(&Value::Bool(true)),
            })
        }
        "bye" => Some(EmberMsg::Bye),
        "retry" => Some(EmberMsg::Retry),
        "restart" => Some(EmberMsg::Restart),
        "next" => Some(EmberMsg::Next),
        "menu" => Some(EmberMsg::Menu),
        "pick" => Some(EmberMsg::Pick {
            l: level_index(o.get("l"), levels)?,
        }),
        "go" => Some(EmberMsg::Go {
            l: level_index(o.get("l"), levels)?,
        }),
        "sel" => Some(EmberMsg::Sel {
            l: level_index(o.get("l"), levels)?,
        }),
        "note" => {
            let w = o.get("w")?.as_str()?;
            NOTES
                .iter()
                .find(|n| n.name() == w)
                .map(|w| EmberMsg::Note { w: *w })
        }
        "k" => {
            let a = int(o.get("a"), 0.0, MAX_COUNT)?;
            let f = int(o.get("f"), 0.0, MAX_FRAME)?;
            let r = match o.get("r") {
                None => None,
                v => Some(int(v, 0.0, MAX_COUNT)?),
            };
            let mut i = list(o.get("i"), MAX_CHANGES, |c| {
                let c = c.as_array()?;
                if c.len() != 2 {
                    return None;
                }
                Some((
                    int(c.first(), 0.0, MAX_FRAME)? as i64,
                    int(c.get(1), 0.0, 15.0)? as i64,
                ))
            })?;
            i.sort_by_key(|c| c.0);
            Some(EmberMsg::K { a, f, i, r })
        }
        "s" => {
            if !only(o, &STATE_KEYS) {
                return None;
            }
            let a = int(o.get("a"), 0.0, MAX_COUNT)?;
            let f = int(o.get("f"), 0.0, MAX_FRAME)?;
            let fi = int(o.get("fi"), 0.0, 15.0)?;
            let r = int(o.get("r"), 0.0, MAX_COUNT)?;
            let sl = match o.get("sl") {
                None => None,
                Some(v) => {
                    let s = v.as_array()?;
                    if s.len() != 2 {
                        return None;
                    }
                    Some((
                        int(s.first(), -MAX_FRAME, MAX_FRAME)?,
                        int(s.get(1), 0.0, MAX_FRAME)?,
                    ))
                }
            };
            let k = match o.get("k") {
                None => None,
                Some(v) => Some(check_slow(v, levels)?),
            };
            let g = check_fast(o.get("g"))?;
            let e = list(o.get("e"), MAX_EVENTS, unpack_event)?;
            let q = match o.get("q") {
                None => None,
                Some(v) => {
                    let q = v.as_array()?;
                    if q.len() != 2 {
                        return None;
                    }
                    let qf = int(q.first(), 0.0, f)?;
                    Some((qf, list(q.get(1), MAX_EVENTS, unpack_event)?))
                }
            };
            Some(EmberMsg::State(Box::new(StateMsg {
                a,
                f,
                fi,
                sl,
                r,
                k,
                g,
                e,
                q,
            })))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
