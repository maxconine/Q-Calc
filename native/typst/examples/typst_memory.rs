//! Memory over a long session: compiles the corpus pass after pass and prints the resident size.
//! `cargo run -p qcalc-typst --release --example typst_memory -- <passes> <mode>`; mode `compile`, `render`,
//! `same` (one source over and over) or `fresh` (a new renderer each pass).

use qcalc_typst::{RenderOptions, Renderer};

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let passes: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    let mode = args.get(2).map(String::as_str).unwrap_or("compile");
    let docs: Vec<String> = include_str!("../tests/corpus.jsonl")
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["doc"].as_str().unwrap().to_string()
        })
        .collect();
    let mut renderer = Renderer::new();
    for pass in 1..=passes {
        if mode == "fresh" {
            renderer = Renderer::new();
        }
        for (i, doc) in docs.iter().enumerate() {
            match mode {
                "render" => {
                    let _ = renderer.render(doc, &RenderOptions::default());
                }
                "same" => {
                    // two alternating sources, so the document cache never answers
                    let _ = renderer.compile(&docs[i % 2]);
                    renderer.clear_caches();
                }
                _ => {
                    let _ = renderer.compile(doc);
                }
            }
        }
        if pass % 5 == 0 || pass == 1 {
            println!("{mode} pass {pass}: {:.1} MB", rss_mb());
        }
    }
}
