//! The games against the TS, step for step: native/core/golden/games/ is written by
//! native/scripts/games-golden/capture.sh from the TS engine. Every shipped ember level's walkthrough is replayed
//! here and must reach the same state hash every 30 steps (the whole state, checkpoint snapshot included), make
//! the same wire text, camera, beams and score; trails bots must make the same moves.

use std::path::PathBuf;

use qcalc_core::games::ember::camera::camera_for;
use qcalc_core::games::ember::engine::{
    compile_level, score_of, stage_of, start_stage, step, trace_beams, validate_level,
};
use qcalc_core::games::ember::json::{state_hash, to_json};
use qcalc_core::games::ember::protocol::{
    EmberMsg, StateMsg, encode_ember, fast_json, fast_of, slow_json, slow_of,
};
use qcalc_core::games::ember::{self, GameEvent, GameState, LEVELS, Level};
use qcalc_core::games::trails::{self, Level as Bot};
use serde_json::Value;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/games")
}

fn read(name: &str) -> Value {
    let text = std::fs::read_to_string(dir().join(name))
        .unwrap_or_else(|e| panic!("{name}: {e} (run native/scripts/games-golden/capture.sh)"));
    serde_json::from_str(&text).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn every_shipped_level_validates_and_compiles() {
    assert!(!LEVELS.is_empty());
    for def in LEVELS.iter() {
        assert_eq!(validate_level(def), Vec::<String>::new(), "{}", def.id);
        assert!(compile_level(def).is_ok());
    }
}

fn check_level(id: &str) {
    let fx = read(&format!("ember/{id}.json"));
    let index = LEVELS.iter().position(|l| l.id == id).unwrap();
    let def = &LEVELS[index];
    let level: Level = compile_level(def).unwrap();
    let walk: Vec<Vec<[i64; 3]>> = fx["walk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|st| {
            st.as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    [
                        r[0].as_i64().unwrap(),
                        r[1].as_i64().unwrap(),
                        r[2].as_i64().unwrap(),
                    ]
                })
                .collect()
        })
        .collect();
    let hashes: Vec<(u64, String)> = fx["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h[0].as_u64().unwrap(), h[1].as_str().unwrap().to_string()))
        .collect();
    let wire = fx["wire"].as_array().unwrap();
    let mut n: u64 = 0;
    let mut pending: Vec<GameEvent> = vec![];
    let mut hi = 0;
    let mut wi = 0;
    let mut after = |s: &GameState, n: u64| {
        pending.extend(s.events.iter().cloned());
        if hi < hashes.len() && hashes[hi].0 == n {
            assert_eq!(
                state_hash(s),
                hashes[hi].1,
                "{id}: the state after step {n} differs\n{}",
                to_json(s)
            );
            hi += 1;
        }
        if wi < wire.len() && wire[wi]["n"].as_u64() == Some(n) {
            let w = &wire[wi];
            let stage = stage_of(&level, s.stage);
            let k = slow_of(index, stage, s);
            let g = fast_of(stage, s);
            assert_eq!(
                fast_json(&g),
                w["fast"].as_str().unwrap(),
                "{id} step {n}: fast"
            );
            assert_eq!(
                slow_json(&k),
                w["slow"].as_str().unwrap(),
                "{id} step {n}: slow"
            );
            let q = pending.iter().take(2).cloned().collect();
            let msg = EmberMsg::State(Box::new(StateMsg {
                a: 1.0,
                f: n as f64,
                fi: 3.0,
                sl: None,
                r: 2.0,
                k: Some(k),
                g,
                e: std::mem::take(&mut pending),
                q: Some(((n - 1) as f64, q)),
            }));
            assert_eq!(
                encode_ember(&msg),
                w["msg"].as_str().unwrap(),
                "{id} step {n}: message"
            );
            for (c, aspect) in w["cam"].as_array().unwrap().iter().zip([16.0 / 9.0, 0.8]) {
                let cam = camera_for(stage, s, aspect);
                assert_eq!(
                    [cam.x, cam.y, cam.w, cam.h, cam.zoom],
                    [
                        f(&c["x"]),
                        f(&c["y"]),
                        f(&c["w"]),
                        f(&c["h"]),
                        f(&c["zoom"])
                    ],
                    "{id} step {n}: camera"
                );
            }
            let beams = trace_beams(stage, s);
            let want = w["beams"].as_array().unwrap();
            assert_eq!(beams.len(), want.len(), "{id} step {n}: beams");
            for (b, x) in beams.iter().zip(want) {
                assert_eq!(
                    [b.x1, b.y1, b.x2, b.y2],
                    [f(&x["x1"]), f(&x["y1"]), f(&x["x2"]), f(&x["y2"])]
                );
            }
            let sc = score_of(&level, s);
            let ws = &w["score"];
            assert_eq!(sc.secs, f(&ws["secs"]));
            assert_eq!(
                [sc.gems[0], sc.gems[1], sc.stars],
                [
                    ws["gems"][0].as_i64().unwrap(),
                    ws["gems"][1].as_i64().unwrap(),
                    ws["stars"].as_i64().unwrap()
                ]
            );
            wi += 1;
        }
    };
    // runWalkthrough, with the hook
    let mut s = start_stage(&level, 0, None);
    for i in 0..level.stages.len() {
        for row in walk.get(i).map_or(&[][..], Vec::as_slice) {
            let mut t = 0;
            while t < row[0] && s.phase == ember::Phase::Play && s.stage == i {
                s = step(&level, &s, [row[1], row[2]]);
                n += 1;
                after(&s, n);
                t += 1;
            }
        }
        if i == level.stages.len() - 1 || s.phase != ember::Phase::Clear {
            break;
        }
        let mut t = 0;
        while t < 600 && s.phase == ember::Phase::Clear {
            s = step(&level, &s, [0, 0]);
            n += 1;
            after(&s, n);
            t += 1;
        }
    }
    assert_eq!(n, fx["steps"].as_u64().unwrap(), "{id}: steps");
    assert_eq!(hi, hashes.len());
    assert_eq!(wi, wire.len());
    assert_eq!(
        to_json(&s),
        fx["finalJson"].as_str().unwrap(),
        "{id}: the final state"
    );
    assert_eq!(state_hash(&s), fx["final"].as_str().unwrap());
    assert_eq!(s.phase, ember::Phase::Won, "{id}: the walkthrough wins");
    assert_eq!(s.deaths, 0);
    // the solver's reach over each stage
    use qcalc_core::games::ember::solver::{ReachOpts, reachability, render_reach};
    for (st, want) in def.stages.iter().zip(fx["reach"].as_array().unwrap()) {
        let r = reachability(st, ReachOpts::default());
        assert_eq!(
            render_reach(st, &r),
            want["render"].as_str().unwrap(),
            "{id}: reach"
        );
        assert_eq!(
            (r.fire_door, r.frost_door),
            (
                want["fireDoor"].as_bool().unwrap(),
                want["frostDoor"].as_bool().unwrap()
            )
        );
        assert_eq!(
            [r.fire_gems, r.frost_gems, r.melted, r.frozen].map(|n| n as u64),
            ["fireGems", "frostGems", "melted", "frozen"].map(|k| want[k].as_u64().unwrap())
        );
    }
}

#[test]
fn every_walkthrough_matches_the_ts_step_for_step() {
    for def in LEVELS.iter() {
        check_level(&def.id);
    }
}

/// emberEngine.test.ts, emberProtocol.test.ts and emberCamera.test.ts: every engine call they made, replayed
fn replay_calls(name: &str) -> usize {
    use qcalc_core::games::ember::engine::{Carry, init_state, respawn};
    use qcalc_core::games::ember::json::state_from_json;
    use std::collections::{HashMap, HashSet};
    let defs: Vec<qcalc_core::games::ember::LevelDef> =
        serde_json::from_value(read(&format!("ember-calls/{name}.defs.json"))).unwrap();
    let levels: Vec<Option<Level>> = defs.iter().map(|d| compile_level(d).ok()).collect();
    let text = std::fs::read_to_string(dir().join(format!("ember-calls/{name}.jsonl"))).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // the states some later call starts from by reference
    let mut wanted: HashSet<u64> = HashSet::new();
    for l in &lines {
        let a = l.as_array().unwrap();
        if a[0] == "r" {
            wanted.insert(a[1].as_u64().unwrap() - 1);
        } else if (a[2] == "step" || a[2] == "respawn") && a[4].is_u64() {
            wanted.insert(a[4].as_u64().unwrap());
        }
    }
    let mut kept: HashMap<u64, GameState> = HashMap::new();
    let mut checked = 0;
    let input = |v: &Value| [v[0].as_i64().unwrap(), v[1].as_i64().unwrap()];
    for l in &lines {
        let a = l.as_array().unwrap();
        if a[0] == "r" {
            let first = a[1].as_u64().unwrap();
            let level = levels[a[2].as_u64().unwrap() as usize].as_ref().unwrap();
            let n = a[3].as_u64().unwrap();
            let keys = input(&a[4]);
            let hs: HashMap<u64, &str> = a[5]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| (h[0].as_u64().unwrap(), h[1].as_str().unwrap()))
                .collect();
            let mut s = kept
                .get(&(first - 1))
                .unwrap_or_else(|| panic!("{name}: no state {}", first - 1))
                .clone();
            for k in 0..n {
                s = step(level, &s, keys);
                if let Some(h) = hs.get(&k) {
                    assert_eq!(
                        state_hash(&s),
                        *h,
                        "{name}: call {} (a run from {first})\n{}",
                        first + k,
                        to_json(&s)
                    );
                    checked += 1;
                }
                if wanted.contains(&(first + k)) {
                    kept.insert(first + k, s.clone());
                }
            }
            continue;
        }
        let id = a[1].as_u64().unwrap();
        let Some(level) = levels[a[3].as_u64().unwrap() as usize].as_ref() else {
            continue;
        };
        let from = |v: &Value| -> GameState {
            if let Some(r) = v.as_u64() {
                kept.get(&r)
                    .unwrap_or_else(|| panic!("{name}: no state {r}"))
                    .clone()
            } else {
                state_from_json(v).unwrap()
            }
        };
        let (out, want) = match a[2].as_str().unwrap() {
            "step" => (step(level, &from(&a[4]), input(&a[5])), &a[6]),
            "respawn" => (respawn(level, &from(&a[4])), &a[5]),
            "init" => (init_state(level, a[4].as_i64().unwrap()), &a[5]),
            _ => {
                let carry = a[5].as_object().map(|o| Carry {
                    tick: o["tick"].as_i64().unwrap(),
                    deaths: o["deaths"].as_i64().unwrap(),
                    banked: [
                        o["banked"][0].as_i64().unwrap(),
                        o["banked"][1].as_i64().unwrap(),
                    ],
                });
                (
                    start_stage(level, a[4].as_u64().unwrap() as usize, carry),
                    &a[6],
                )
            }
        };
        assert_eq!(
            state_hash(&out),
            want.as_str().unwrap(),
            "{name}: call {id} ({})\n{}",
            a[2],
            to_json(&out)
        );
        checked += 1;
        if wanted.contains(&id) {
            kept.insert(id, out);
        }
    }
    checked
}

#[test]
fn the_engine_tests_calls_replay() {
    for name in ["emberEngine", "emberProtocol", "emberCamera"] {
        let n = replay_calls(name);
        eprintln!("{name}: {n} calls checked");
        assert!(n > 0, "{name}");
    }
}

#[test]
fn the_small_helpers_match() {
    let fx = read("ember-helpers.json");
    for t in fx["times"].as_array().unwrap() {
        let secs = t[0].as_f64().unwrap_or(f64::NAN);
        assert_eq!(ember::fmt_time(secs), t[1].as_str().unwrap(), "{secs}");
        assert_eq!(ember::fmt_par(secs), t[2].as_str().unwrap(), "{secs}");
    }
    for (i, d) in fx["difficulty"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            u64::from(ember::level_difficulty(i, 12)),
            d.as_u64().unwrap()
        );
    }
    for (def, l) in LEVELS.iter().zip(fx["lengths"].as_array().unwrap()) {
        assert_eq!(ember::level_length(def), l.as_str().unwrap());
    }
    let progress = fx["progress"].as_array().unwrap();
    let texts = [
        r#"{"levels":{"first-light":{"completed":true,"stars":5,"secs":12.5,"gems":4},"old":{"completed":true,"stars":1}},"muted":true}"#,
        r#"{"v":2,"levels":{"a":{"completed":true,"stars":2,"secs":-1,"gems":3.7,"deaths":2.2},"b":{"completed":false,"stars":3}},"muted":1}"#,
        "nonsense",
        r#"{"v":9,"levels":[]}"#,
    ];
    use qcalc_core::games::ember::progress::{parse_progress, progress_json, record_result};
    for (t, want) in texts.iter().zip(progress) {
        assert_eq!(
            progress_json(&parse_progress(Some(t))),
            want.as_str().unwrap()
        );
    }
    let rec = record_result(
        &parse_progress(Some(texts[1])),
        "a",
        &ember::Score {
            secs: 30.5,
            gems: [2, 2],
            total: [3, 3],
            stars: 2,
        },
        Some(1.0),
    );
    assert_eq!(progress_json(&rec), fx["recorded"].as_str().unwrap());
}

#[test]
#[allow(clippy::needless_range_loop)]
fn trails_bots_make_the_same_moves() {
    let fx = read("trails-duels.json");
    let mut r = trails::seeded(99);
    for d in fx["draws"].as_array().unwrap() {
        assert_eq!(r(), d.as_f64().unwrap());
    }
    let level = |s: &str| match s {
        "easy" => Bot::Easy,
        "normal" => Bot::Normal,
        _ => Bot::Hard,
    };
    for duel in fx["duels"].as_array().unwrap() {
        let (a, b) = (
            level(duel["a"].as_str().unwrap()),
            level(duel["b"].as_str().unwrap()),
        );
        let mut rand = trails::seeded(duel["seed"].as_u64().unwrap() as u32);
        for (i, game) in duel["games"].as_array().unwrap().iter().enumerate() {
            let levels = if i % 2 == 1 { [b, a] } else { [a, b] };
            let mut r = trails::new_round();
            let mut h = trails::net::hash_start(i as u32, [0, 0]);
            let mut moves = String::new();
            let opening = (rand() * 12.0).floor() as u32;
            while r.result.is_none() && r.tick < (trails::COLS * trails::ROWS) as u32 {
                for p in 0..2 {
                    if r.tick >= opening {
                        let d = trails::bot_turn(&r, p, levels[p], &mut rand);
                        moves.push(char::from(b'0' + d));
                        r = trails::steer(&r, p, d);
                    } else if rand() < 0.15 {
                        let d = (rand() * 4.0).floor() as u8;
                        r = trails::steer(&r, p, d);
                    }
                }
                r = trails::step(&r);
                h = trails::net::hash_step(h, &r);
            }
            assert_eq!(moves, game["moves"].as_str().unwrap(), "duel {i}");
            assert_eq!(u64::from(r.tick), game["ticks"].as_u64().unwrap());
            assert_eq!(
                r.result.and_then(|x| x.winner).map(|w| w as u64),
                game["winner"].as_u64()
            );
            assert_eq!(u64::from(h), game["hash"].as_u64().unwrap());
            let snap = trails::net::Snap {
                e: 1,
                r: i as u32,
                d: 2,
                score: [0, 1],
                phase: trails::net::NetPhase::Point,
                wait: 12.6,
                round: r.clone(),
                frames: [vec![], vec![]],
                hash: h,
            };
            assert_eq!(
                trails::net::encode_trails(&trails::net::TrailsMsg::Snap(Box::new(snap))),
                game["snap"].as_str().unwrap()
            );
        }
    }
}
