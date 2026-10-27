//! The engine's share of a keystroke, without the app (native/windows/PERFORMANCE.md): every prefix of every input
//! through what the bar's live job calls (`Engine::chains`, `Engine::live`), with a returning user's history as
//! the context, and the core's `evaluate_sheet` alone for comparison.
//!   cargo run --release -p qcalc-win --example live_bench -- <data dir with history.json> [inputs.json]
use std::time::Instant;

use qcalc_ffi::{AnswerForm, Engine, EngineConfig, HistoryShow, HistoryView, LiveRequest};

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    v[rank.clamp(1, v.len()) - 1]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = args.first().cloned();
    let inputs: Vec<String> = match args.get(1) {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap(),
        None => [
            "2+2",
            "sin(90)",
            "72 f to c",
            "1/3 + 1/6",
            "sqrt(2)*pi",
            "5 km in miles",
            "3x+2=11",
            "(1+2)*(3+4)/5^2",
            "20% of 80",
            "e^(i pi)+1",
        ]
        .map(String::from)
        .to_vec(),
    };
    let engine = Engine::new(EngineConfig {
        is_windows: true,
        locale: "en_US".into(),
        data_dir: data,
        time_zone: None,
    });
    let rows = engine.load_history();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let view = HistoryView {
        show: HistoryShow::Recent,
        answer_form: AnswerForm::Exact,
        sig_figs: 10,
        now_ms: now,
    };
    let t = Instant::now();
    let context = engine.context_from_history(rows.clone(), view);
    eprintln!(
        "rows {} context {:.3} ms",
        rows.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    let options = qcalc_ffi::default_eval_options();
    let core_options = qcalc_core::EvaluateOptions::default();
    let (mut chains, mut live, mut sheet, mut ctx) = (vec![], vec![], vec![], vec![]);
    for pass in 0..4 {
        for input in &inputs {
            let chars: Vec<char> = input.chars().collect();
            for k in 1..=chars.len() {
                let text: String = chars[..k].iter().collect();
                let t = Instant::now();
                let c = std::hint::black_box(context.clone());
                let a = t.elapsed().as_secs_f64() * 1e6;
                let t = Instant::now();
                let chained = engine.chains(text.clone(), c.clone());
                let b = t.elapsed().as_secs_f64() * 1e6;
                let t = Instant::now();
                std::hint::black_box(engine.live(LiveRequest {
                    text: text.clone(),
                    options: options.clone(),
                    context: c,
                    chained,
                    now_ms: now,
                    phrases: true,
                }));
                let l = t.elapsed().as_secs_f64() * 1e6;
                let t = Instant::now();
                std::hint::black_box(qcalc_core::evaluate_sheet(&[text], &core_options));
                let s = t.elapsed().as_secs_f64() * 1e6;
                if pass > 0 {
                    ctx.push(a);
                    chains.push(b);
                    live.push(l);
                    sheet.push(s);
                }
            }
        }
    }
    if std::env::var_os("LIVE_BENCH_EACH").is_some() {
        // each prefix's median over the timed passes
        let per = live.len() / 3;
        for i in 0..per.min(120) {
            let mut v = vec![live[i], live[i + per], live[i + 2 * per]];
            print!("{:.0} ", pct(&mut v, 50.0));
        }
        println!();
    }
    for (name, v) in [
        ("context clone", &mut ctx),
        ("chains", &mut chains),
        ("live", &mut live),
        ("evaluate_sheet", &mut sheet),
    ] {
        let n = v.len();
        println!(
            "{name:>15}: n {n} median {:.1} us p90 {:.1} p99 {:.1}",
            pct(v, 50.0),
            pct(v, 90.0),
            pct(v, 99.0)
        );
    }
}
