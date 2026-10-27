//! A state as the TS's `JSON.stringify` writes it: the same keys in the same order and JS number text, so
//! `state_hash` (testkit.ts `stateHash`) gives the web build's hash for the same state. The golden walkthroughs
//! hold the port to those hashes.

use std::fmt::Write;

use super::types::*;
use crate::jsnum;

fn num(out: &mut String, x: f64) {
    // JSON.stringify writes -0 as 0 and non-finite numbers as null
    if !x.is_finite() {
        out.push_str("null");
    } else {
        out.push_str(&jsnum::to_string(if x == 0.0 { 0.0 } else { x }));
    }
}

fn int(out: &mut String, x: i64) {
    let _ = write!(out, "{x}");
}

fn string(out: &mut String, s: &str) {
    out.push_str(&serde_json::to_string(s).unwrap_or_default());
}

fn list<T>(out: &mut String, v: &[T], mut each: impl FnMut(&mut String, &T)) {
    out.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        each(out, x);
    }
    out.push(']');
}

fn bools(out: &mut String, v: &[bool]) {
    list(out, v, |o, b| o.push_str(if *b { "true" } else { "false" }));
}

fn ints(out: &mut String, v: &[i64]) {
    list(out, v, |o, n| int(o, *n));
}

fn nums(out: &mut String, v: &[f64]) {
    list(out, v, |o, n| num(o, *n));
}

fn player(out: &mut String, p: &Player) {
    out.push_str("{\"x\":");
    num(out, p.x);
    out.push_str(",\"y\":");
    num(out, p.y);
    out.push_str(",\"vx\":");
    num(out, p.vx);
    out.push_str(",\"vy\":");
    num(out, p.vy);
    let _ = write!(
        out,
        ",\"ground\":{},\"face\":{},\"alive\":{},\"inDoor\":{},\"coyote\":{},\"buffer\":{},\"prevIn\":{},\"portalCd\":{},\"gems\":{},\"grav\":{},\"air\":{},\"cannon\":{},\"fly\":{},\"cannonCd\":{}}}",
        p.ground,
        p.face,
        p.alive,
        p.in_door,
        p.coyote,
        p.buffer,
        p.prev_in,
        p.portal_cd,
        p.gems,
        p.grav,
        p.air,
        p.cannon,
        p.fly,
        p.cannon_cd
    );
}

pub fn event_json(out: &mut String, e: &GameEvent) {
    out.push_str("{\"k\":");
    string(out, e.k.name());
    if let Some(p) = e.p {
        let _ = write!(out, ",\"p\":{p}");
    }
    if let Some(t) = e.tile {
        let _ = write!(out, ",\"tile\":{t}");
    }
    if let Some(i) = e.i {
        let _ = write!(out, ",\"i\":{i}");
    }
    if let Some(on) = e.on {
        let _ = write!(out, ",\"on\":{on}");
    }
    if let Some(x) = e.x {
        out.push_str(",\"x\":");
        num(out, x);
    }
    if let Some(y) = e.y {
        out.push_str(",\"y\":");
        num(out, y);
    }
    if let Some(c) = e.cause {
        out.push_str(",\"cause\":");
        string(out, c.name());
    }
    if let Some(v) = e.v {
        out.push_str(",\"v\":");
        num(out, v);
    }
    if let Some(g) = e.grav {
        let _ = write!(out, ",\"grav\":{g}");
    }
    if let Some(el) = e.el {
        out.push_str(",\"el\":");
        string(out, el.name());
    }
    if let Some(st) = e.stage {
        let _ = write!(out, ",\"stage\":{st}");
    }
    if let Some(cp) = e.cp {
        let _ = write!(out, ",\"cp\":{cp}");
    }
    out.push('}');
}

pub fn state_json(out: &mut String, s: &GameState) {
    state_json_with(out, s, true);
}

/// with_saved false writes the checkpoint snapshot as null, for drawing
pub fn state_json_with(out: &mut String, s: &GameState, with_saved: bool) {
    let _ = write!(
        out,
        "{{\"tick\":{},\"phase\":\"{}\",\"phaseT\":{},\"deaths\":{},\"stage\":{},\"banked\":",
        s.tick,
        s.phase.name(),
        s.phase_t,
        s.deaths,
        s.stage
    );
    ints(out, &s.banked);
    out.push_str(",\"players\":");
    list(out, &s.players, player);
    out.push_str(",\"taken\":");
    bools(out, &s.taken);
    out.push_str(",\"levers\":");
    bools(out, &s.levers);
    out.push_str(",\"buttons\":");
    ints(out, &s.buttons);
    out.push_str(",\"gates\":");
    nums(out, &s.gates);
    out.push_str(",\"movers\":");
    list(out, &s.movers, |o, m| {
        o.push_str("{\"x\":");
        num(o, m.x);
        o.push_str(",\"y\":");
        num(o, m.y);
        o.push_str(",\"t\":");
        num(o, m.t);
        let _ = write!(o, ",\"dir\":{}}}", m.dir);
    });
    out.push_str(",\"boxes\":");
    list(out, &s.boxes, |o, b| {
        o.push_str("{\"x\":");
        num(o, b.x);
        o.push_str(",\"y\":");
        num(o, b.y);
        o.push_str(",\"vx\":");
        num(o, b.vx);
        o.push_str(",\"vy\":");
        num(o, b.vy);
        o.push('}');
    });
    out.push_str(",\"ice\":");
    nums(out, &s.ice);
    out.push_str(",\"thin\":");
    ints(out, &s.thin);
    out.push_str(",\"mirrors\":");
    list(out, &s.mirrors, |o, m| string(o, m.text()));
    let _ = write!(out, ",\"lit\":{},\"litT\":{},\"arrows\":", s.lit, s.lit_t);
    ints(out, &s.arrows);
    out.push_str(",\"crumble\":");
    ints(out, &s.crumble);
    let _ = write!(out, ",\"keys\":{},\"keyTaken\":", s.keys);
    bools(out, &s.key_taken);
    out.push_str(",\"locks\":");
    bools(out, &s.locks);
    out.push_str(",\"cannons\":");
    ints(out, &s.cannons);
    let _ = write!(out, ",\"cp\":{}", s.cp);
    if !s.is_snapshot {
        out.push_str(",\"saved\":");
        match &s.saved {
            Some(snap) if with_saved => state_json(out, snap),
            _ => out.push_str("null"),
        }
    }
    out.push_str(",\"on\":");
    list(out, &s.on, |o, c| string(o, c));
    if !s.is_snapshot {
        out.push_str(",\"events\":");
        list(out, &s.events, event_json);
    }
    if let Some(p) = &s.plates {
        out.push_str(",\"plates\":");
        bools(out, p);
    }
    if let Some(p) = &s.sensors {
        out.push_str(",\"sensors\":");
        bools(out, p);
    }
    out.push('}');
}

pub fn to_json(s: &GameState) -> String {
    let mut out = String::with_capacity(2048);
    state_json(&mut out, s);
    out
}

/// a state for drawing: its JSON without the checkpoint snapshot
pub fn view_json(s: &GameState) -> String {
    let mut out = String::with_capacity(2048);
    state_json_with(&mut out, s, false);
    out
}

/// testkit.ts stateHash: FNV-1a over the JSON's UTF-16 code units, as 8 hex digits
pub fn state_hash(s: &GameState) -> String {
    let text = to_json(s);
    let mut h: u32 = 0x811c9dc5;
    for u in text.encode_utf16() {
        h ^= u32::from(u);
        h = h.wrapping_mul(0x01000193);
    }
    format!("{h:08x}")
}
