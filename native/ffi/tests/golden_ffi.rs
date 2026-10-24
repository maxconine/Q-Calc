//! Replays the golden fixtures of the `src/lib` modules ported into this crate (`history.rs`, `chain.rs`) against
//! the TS answers, with the core's comparison rules. Prints a table; fails below the floors in
//! `ffi/golden-floors.json` (`GOLDEN_UPDATE_BASELINE=1` raises them), the same ratchet as the core's runner.
//! `GOLDEN_FAILS=20` prints failures.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use qcalc_core::golden::{compare, decode};
use qcalc_ffi::chain;
use qcalc_ffi::history as h;
use qcalc_ffi::records::{ChainAnswer, HistoryShow};
use serde_json::{Map, Value as Json, json};

fn rows(v: &Json) -> Vec<Json> {
    v.as_array().cloned().unwrap_or_default()
}

fn obj(m: Map<String, Json>) -> Json {
    Json::Object(m)
}

fn opt_str(s: Option<String>) -> Json {
    s.map_or(Json::Null, Json::String)
}

/// The Rust answer for one recorded call, or None when this crate doesn't port that function.
fn replay(name: &str, args: &[Json]) -> Option<Json> {
    let a = |i: usize| args.get(i).cloned().unwrap_or(Json::Null);
    let s = |i: usize| a(i).as_str().unwrap_or("").to_string();
    let n = |i: usize| a(i).as_f64();
    let sig = |i: usize| n(i).unwrap_or(12.0);
    Some(match name {
        // ---- lib/history ----
        "ellipsize" => json!(h::ellipsize(&s(0), n(1).unwrap_or(0.0) as i64)),
        "isMatrixQuantity" => json!(h::is_matrix_quantity(a(0).as_str())),
        "isComplexQuantity" => json!(h::is_complex_quantity(a(0).as_str())),
        "isLiteralQuantity" => json!(h::is_literal_quantity(a(0).as_str())),
        "slimHistoryRow" => h::slim_history_row(&a(0)).unwrap_or(Json::Null),
        "normalizeHistoryRow" => match h::normalize_history_row(&a(0), &s(1)) {
            Ok(r) => r.unwrap_or(Json::Null),
            Err(h::Throws) => return Some(json!({ "$throws": true })),
        },
        "persistableHistory" => Json::Array(h::persistable_history(&rows(&a(0)))),
        "historyVariables" => obj(h::history_variables(&rows(&a(0)))
            .into_iter()
            .map(|(k, v)| (k, json!(v)))
            .collect()),
        "historyMeasures" => obj(h::history_measures(&rows(&a(0)))),
        "historyQuantities" => obj(h::history_quantities(&rows(&a(0)))),
        "historyFunctions" => obj(h::history_functions(&rows(&a(0)))),
        "lastHistoryNumber" => {
            h::last_history_number(&rows(&a(0))).map_or(Json::Null, |n| json!(n))
        }
        // ---- lib/historyShow ----
        "normalizeHistoryShow" => json!(match h::normalize_history_show(&a(0)) {
            HistoryShow::Recent => "recent",
            HistoryShow::Always => "always",
            HistoryShow::Arrow => "arrow",
        }),
        "recentStart" => json!(h::recent_start(&rows(&a(0)), n(1).unwrap_or(0.0))),
        "scopeStart" => json!(h::scope_start(
            &rows(&a(0)),
            h::normalize_history_show(&a(1)),
            n(2).unwrap_or(0.0)
        )),
        "nextRecentExpiry" => h::next_recent_expiry(&rows(&a(0)), n(1).unwrap_or(0.0))
            .map_or(Json::Null, |n| json!(n)),
        // ---- lib/chain ----
        "chainsFromAnswer" => {
            let last = a(1).as_object().map(|o| ChainAnswer {
                plain: o
                    .get("plain")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                unit: o.get("unit").and_then(Json::as_bool).unwrap_or(false),
            });
            json!(chain::chains_from_answer(&s(0), last.as_ref()))
        }
        "chainedExpr" => json!(chain::chained_expr(&s(0))),
        "chainedHistoryExpr" => json!(chain::chained_history_expr(&s(0), &s(1))),
        "ansWrittenOut" => json!(chain::ans_written_out(&s(0), &s(1))),
        // ---- lib/answer (the parts chaining needs) ----
        "answerAmong" => json!(chain::answer_among(&s(0), a(1).as_bool().unwrap_or(false))),
        "insertableAnswer" => json!(chain::insertable_answer(&s(0), n(1), sig(2))),
        "insertableHistoryAnswer" => json!(chain::insertable_history_answer(
            &a(0),
            s(1) == "exact",
            sig(2)
        )),
        "solveRoots" => chain::solve_roots(&a(0), sig(1)).map_or(Json::Null, |r| json!(r)),
        "solveInsert" => opt_str(chain::solve_insert(&a(0), sig(1))),
        _ => return None,
    })
}

#[derive(Default)]
struct Tally {
    pass: usize,
    fail: usize,
    unported: usize,
    failures: Vec<String>,
}

#[test]
fn ported_lib_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/golden");
    let show: usize = std::env::var("GOLDEN_FAILS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut table: BTreeMap<String, Tally> = BTreeMap::new();
    for area in ["lib.history", "lib.historyShow", "lib.chain", "lib.answer"] {
        let text = fs::read_to_string(dir.join(format!("{area}.jsonl"))).unwrap_or_default();
        let t = table.entry(area.to_string()).or_default();
        for line in text.lines().filter(|l| !l.is_empty()) {
            let Ok(rec) = serde_json::from_str::<Json>(line) else {
                continue;
            };
            if rec.get("nondet").and_then(Json::as_bool) == Some(true) {
                continue;
            }
            let name = rec["fn"].as_str().unwrap_or("");
            let args: Vec<Json> = rec["args"]
                .as_array()
                .map(|a| a.iter().map(decode).collect())
                .unwrap_or_default();
            let Some(got) = replay(name, &args) else {
                t.unported += 1;
                continue;
            };
            let verdict = match (rec.get("throws"), got.get("$throws")) {
                (Some(_), Some(_)) => Ok(()),
                (Some(e), None) => Err(format!("want throw {e}, got {got}")),
                (None, _) => compare(rec.get("out").unwrap_or(&Json::Null), &got, "out"),
            };
            match verdict {
                Ok(()) => t.pass += 1,
                Err(e) => {
                    t.fail += 1;
                    if t.failures.len() < show {
                        t.failures
                            .push(format!("{name} {}: {e}", Json::Array(args)));
                    }
                }
            }
        }
    }
    println!("| area | pass | fail | not ported here |\n|---|---:|---:|---:|");
    for (area, t) in &table {
        println!("| {area} | {} | {} | {} |", t.pass, t.fail, t.unported);
        for f in &t.failures {
            println!("    {f}");
        }
    }
    let floors_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden-floors.json");
    let floors: BTreeMap<String, usize> = fs::read_to_string(&floors_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if std::env::var("GOLDEN_UPDATE_BASELINE").is_ok_and(|v| v == "1") {
        let raised: BTreeMap<&String, usize> = table
            .iter()
            .map(|(a, t)| (a, t.pass.max(floors.get(a).copied().unwrap_or(0))))
            .collect();
        fs::write(
            &floors_path,
            serde_json::to_string_pretty(&raised).unwrap() + "\n",
        )
        .unwrap();
    }
    for (area, floor) in &floors {
        let pass = table.get(area).map_or(0, |t| t.pass);
        assert!(
            pass >= *floor,
            "{area}: {pass} passes, below its floor of {floor}"
        );
    }
}
