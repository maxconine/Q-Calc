//! The bar's live path alone: `app::live` for every prefix of every input, as a keystroke calls it (no squiggle,
//! typeset preview and phrases on). engine_bench times `evaluate_sheet`; this adds what the app layer does around it.
//!   cargo run --release -p qcalc-core --example live_bench [inputs.json]   (default bench/inputs.json)
//! Prints JSON: `first` (ms, the first call), `keys` (ms per keystroke, 3 passes after a warm-up) and `perInput`
//! (ms to type each input once, the median pass), to compare two builds input by input.
use std::time::Instant;

use qcalc_core::app::live::{LiveRequest, live};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../bench/inputs.json").into());
    let inputs: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("inputs")).expect("a JSON list");
    let req = |t: &str| LiveRequest {
        text: t.into(),
        phrases: true,
        typst_preview: true,
        no_squiggle: true,
        now_ms: 1.7e12,
        ..Default::default()
    };
    let t0 = Instant::now();
    live(&req("1+1"));
    let first = t0.elapsed().as_secs_f64() * 1e3;
    let mut per_input = vec![Vec::new(); inputs.len()];
    let mut keys = Vec::new();
    for pass in 0..4 {
        for (i, s) in inputs.iter().enumerate() {
            let mut total = 0.0;
            let ends = s.char_indices().skip(1).map(|(b, _)| b).chain([s.len()]);
            for end in ends {
                let t = Instant::now();
                std::hint::black_box(live(&req(&s[..end])));
                let ms = t.elapsed().as_secs_f64() * 1e3;
                total += ms;
                if pass > 0 {
                    keys.push(ms);
                }
            }
            if pass > 0 {
                per_input[i].push(total);
            }
        }
    }
    let per: Vec<f64> = per_input
        .into_iter()
        .map(|mut v| {
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({"first": first, "keys": keys, "perInput": per})
    );
}
