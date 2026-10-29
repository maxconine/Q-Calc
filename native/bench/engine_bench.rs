//! The Rust core alone, on the inputs engine_bench.ts times in the TS engine (built as qcalc-core's
//! `engine_bench` example; engine.sh runs both). Prints one JSON object like the TS side.
//!   keystrokes: evaluate_sheet([prefix]) for every prefix of every input; pass 0 warms up, the rest are timed
//!   heavy:      each case once cold (its first call) and then `passes * 3` times warm
use qcalc_core::{EvaluateOptions, evaluate_sheet, graph};
use serde_json::{Value, json};
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: engine_bench <bench.json>");
    let bench: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let inputs: Vec<String> = serde_json::from_value(bench["inputs"].clone()).unwrap();
    let passes = bench["passes"].as_u64().unwrap_or(3) as usize;
    let options = EvaluateOptions::default();

    let t = Instant::now();
    evaluate_sheet(&["1+1".to_string()], &options);
    let first_call = ms(t);

    let mut keys = Vec::new();
    let mut key_total = 0.0;
    let mut first_pass = 0.0;
    for pass in 0..=passes {
        for input in &inputs {
            let chars: Vec<char> = input.chars().collect();
            for k in 1..=chars.len() {
                let text: String = chars[..k].iter().collect();
                let t = Instant::now();
                std::hint::black_box(evaluate_sheet(&[text], &options));
                let took = ms(t);
                if pass == 0 {
                    first_pass += took;
                }
                if pass > 0 {
                    keys.push(took);
                    key_total += took;
                }
            }
        }
    }

    // the closed-form search (PSLQ) the bar runs after a definite integral's numeric answer, through the same
    // entry point the golden tests call
    let closed_form = |h: &Value| match qcalc_core::golden::dispatch(
        "engine/closedForm",
        "identifyJob",
        &[h["job"].clone()],
    ) {
        qcalc_core::golden::Outcome::Answer(v) => v.as_str().unwrap_or("none").to_string(),
        _ => "none".to_string(),
    };
    let run = |h: &Value| {
        if h["kind"] == "closedForm" {
            std::hint::black_box(closed_form(h));
        } else if h["kind"] == "graph" {
            std::hint::black_box(graph::build_graph(
                h["input"].as_str().unwrap(),
                &graph::GraphOptions::default(),
            ));
        } else {
            let lines: Vec<String> = serde_json::from_value(h["lines"].clone()).unwrap();
            std::hint::black_box(evaluate_sheet(&lines, &options));
        }
    };
    // what each case answered (the last line's display, or the graph's point count)
    let answer = |h: &Value| -> String {
        if h["kind"] == "closedForm" {
            closed_form(h)
        } else if h["kind"] == "graph" {
            let g = graph::build_graph(
                h["input"].as_str().unwrap(),
                &graph::GraphOptions::default(),
            );
            format!("{} points", g.map_or(0, |g| g.points.len()))
        } else {
            let lines: Vec<String> = serde_json::from_value(h["lines"].clone()).unwrap();
            evaluate_sheet(&lines, &options)
                .last()
                .map(|l| l.display.clone())
                .unwrap_or_default()
        }
    };
    let heavy: Vec<Value> = bench["heavy"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            let t = Instant::now();
            run(h);
            let cold = ms(t);
            let warm: Vec<f64> = (0..passes * 3)
                .map(|_| {
                    let t = Instant::now();
                    run(h);
                    ms(t)
                })
                .collect();
            json!({ "name": h["name"], "cold": cold, "warm": warm, "answer": answer(h) })
        })
        .collect();

    println!(
        "{}",
        json!({ "firstCall": first_call, "firstPass": first_pass, "keys": keys, "keyTotal": key_total / passes as f64, "heavy": heavy })
    );
}
