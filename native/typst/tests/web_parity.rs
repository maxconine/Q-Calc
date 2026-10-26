//! The native preview against the web one. `corpus.jsonl` holds 1262 preview documents (`typstDocument` over
//! the golden inputs, with the answers the bar typesets beside them), each compiled by typst.ts, the web
//! preview's own WASM Typst, with whether it compiled, its first error and the SVG's size. Regenerate it with
//! `native/typst/scripts/gen-corpus.ts` when `typstMath.ts` or the golden fixtures change.

use qcalc_typst::{Error, Renderer};

struct Case {
    expr: String,
    doc: String,
    web_ok: bool,
    web_size: (f64, f64),
    web_error: String,
}

fn corpus() -> Vec<Case> {
    let text = include_str!("corpus.jsonl");
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            let web = &v["web"];
            Case {
                expr: v["expr"].as_str().unwrap().to_string(),
                doc: v["doc"].as_str().unwrap().to_string(),
                web_ok: web["ok"].as_bool().unwrap(),
                web_size: (
                    web["width"].as_f64().unwrap_or(0.0),
                    web["height"].as_f64().unwrap_or(0.0),
                ),
                web_error: web["error"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect()
}

/// typst.ts writes the page size rounded up to whole points (`width="93.000"` for 92.4 pt).
fn web_rounding(pt: f32) -> f64 {
    (pt as f64).ceil()
}

#[test]
fn compiles_what_the_web_compiles_at_the_same_size() {
    let renderer = Renderer::new();
    let cases = corpus();
    assert!(cases.len() > 1000);
    let mut mismatched = Vec::new();
    let mut compiled = 0;
    for case in &cases {
        match (renderer.compile(&case.doc), case.web_ok) {
            (Ok((w, h)), true) => {
                compiled += 1;
                let got = (web_rounding(w), web_rounding(h));
                if got != case.web_size {
                    mismatched.push(format!(
                        "{:?}: native {w}×{h} pt, web {:?}",
                        case.expr, case.web_size
                    ));
                }
            }
            // the same first error the web's diagnostics give
            (Err(Error::Compile { message }), false) => {
                let first = message.lines().next().unwrap_or("");
                if first != case.web_error {
                    mismatched.push(format!(
                        "{:?}: native error {first:?}, web {:?}",
                        case.expr, case.web_error
                    ));
                }
            }
            (native, web) => mismatched.push(format!(
                "{:?}: native {native:?}, web compiled: {web}",
                case.expr
            )),
        }
    }
    println!(
        "{} sources, {compiled} compiled, {} differ from the web",
        cases.len(),
        mismatched.len()
    );
    for m in mismatched.iter().take(20) {
        println!("  {m}");
    }
    assert!(mismatched.is_empty());
}
