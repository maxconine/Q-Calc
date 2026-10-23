//! Golden parity: replays every fixture in `core/golden/*.jsonl` and reports the pass rate per area.
//!
//! `cargo test -p qcalc-core --test golden -- --nocapture` prints the table and writes `golden/STATUS.md`.
//! Environment:
//! - `GOLDEN_AREA=eval.arithmetic` (comma-separated, prefix match) limits the run;
//! - `GOLDEN_FAILS=20` prints that many failures per area (default 0);
//! - the test fails only when an area drops below its floor in `golden/baseline.json`, so ports ratchet up
//!   (`GOLDEN_UPDATE_BASELINE=1` raises the floors to the current pass counts).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use qcalc_core::golden::{Outcome, compare, dispatch, module_of};
use serde_json::Value as Json;

#[derive(Default)]
struct Tally {
    total: usize,
    pass: usize,
    fail: usize,
    unported: usize,
    skipped: usize,
    failures: Vec<String>,
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("golden")
}

fn wanted(area: &str) -> bool {
    match std::env::var("GOLDEN_AREA") {
        Ok(filter) if !filter.is_empty() => filter.split(',').any(|f| area.starts_with(f.trim())),
        _ => true,
    }
}

fn replay(area: &str, path: &Path, show: usize) -> Tally {
    let module = module_of(area);
    let mut t = Tally::default();
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines().filter(|l| !l.is_empty()) {
        t.total += 1;
        let Ok(rec) = serde_json::from_str::<Json>(line) else {
            t.skipped += 1;
            continue;
        };
        if rec.get("nondet").and_then(Json::as_bool) == Some(true) {
            t.skipped += 1;
            continue;
        }
        let name = rec["fn"].as_str().unwrap_or("");
        let args = rec["args"].as_array().cloned().unwrap_or_default();
        let outcome = std::panic::catch_unwind(|| dispatch(&module, name, &args))
            .unwrap_or_else(|_| Outcome::Threw("panicked".into()));
        let verdict = match (outcome, rec.get("throws")) {
            (Outcome::Unported, _) => {
                t.unported += 1;
                continue;
            }
            (Outcome::Unreplayable, _) => {
                t.skipped += 1;
                continue;
            }
            (Outcome::Threw(_), Some(_)) => Ok(()),
            (Outcome::Threw(e), None) => Err(format!("threw {e}")),
            (Outcome::Answer(a), None) => compare(rec.get("out").unwrap_or(&Json::Null), &a, "out"),
            (Outcome::Answer(a), Some(e)) => Err(format!("want throw {e}, got {a}")),
        };
        match verdict {
            Ok(()) => t.pass += 1,
            Err(why) => {
                t.fail += 1;
                if t.failures.len() < show {
                    t.failures.push(format!("{name}({}) — {why}", serde_json::to_string(&args).unwrap_or_default()));
                }
            }
        }
    }
    t
}

fn pct(a: usize, b: usize) -> String {
    if b == 0 { "—".into() } else { format!("{:.1}%", 100.0 * a as f64 / b as f64) }
}

#[test]
fn golden_parity() {
    // deep fixtures (hundreds of nested brackets) need the stack an evaluation thread gets in the app
    let run = std::thread::Builder::new().stack_size(256 << 20).spawn(parity).expect("spawn");
    if let Err(e) = run.join() {
        std::panic::resume_unwind(e);
    }
}

fn parity() {
    let dir = golden_dir();
    let show: usize = std::env::var("GOLDEN_FAILS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect())
        .unwrap_or_default();
    files.sort();
    let mut results: BTreeMap<String, Tally> = BTreeMap::new();
    for path in &files {
        let area = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if wanted(&area) {
            results.insert(area.clone(), replay(&area, path, show));
        }
    }

    let mut table = String::from("| area | fixtures | pass | fail | unported | skipped | pass rate (of replayed) |\n|---|---:|---:|---:|---:|---:|---:|\n");
    let mut sum = Tally::default();
    for (area, t) in &results {
        let replayed = t.pass + t.fail;
        let _ = writeln!(table, "| {area} | {} | {} | {} | {} | {} | {} |", t.total, t.pass, t.fail, t.unported, t.skipped, pct(t.pass, replayed));
        sum.total += t.total;
        sum.pass += t.pass;
        sum.fail += t.fail;
        sum.unported += t.unported;
        sum.skipped += t.skipped;
    }
    let _ = writeln!(
        table,
        "| **all** | {} | {} | {} | {} | {} | {} (of all: {}) |",
        sum.total,
        sum.pass,
        sum.fail,
        sum.unported,
        sum.skipped,
        pct(sum.pass, sum.pass + sum.fail),
        pct(sum.pass, sum.total)
    );
    println!("\n{table}");
    for (area, t) in &results {
        for f in &t.failures {
            println!("FAIL [{area}] {f}");
        }
    }
    if std::env::var("GOLDEN_AREA").map_or(true, |a| a.is_empty()) {
        let _ = fs::write(dir.join("STATUS.md"), format!("# Golden parity\n\n{table}"));
    }

    // the ratchet: no area may fall below its recorded floor
    let baseline_path = dir.join("baseline.json");
    let baseline: BTreeMap<String, usize> =
        fs::read_to_string(&baseline_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if std::env::var("GOLDEN_UPDATE_BASELINE").is_ok_and(|v| v == "1") {
        let mut next = baseline.clone();
        for (area, t) in &results {
            let floor = next.entry(area.clone()).or_insert(0);
            *floor = (*floor).max(t.pass);
        }
        next.retain(|_, v| *v > 0);
        let _ = fs::write(&baseline_path, format!("{}\n", serde_json::to_string_pretty(&next).unwrap_or_default()));
    }
    let regressions: Vec<String> = baseline
        .iter()
        .filter_map(|(area, floor)| {
            let t = results.get(area)?;
            (t.pass < *floor).then(|| format!("{area}: {} passing, floor {floor}", t.pass))
        })
        .collect();
    assert!(regressions.is_empty(), "golden regressions:\n{}", regressions.join("\n"));
}
