//! Latency and memory of the newest core features: graph intersections (as the graph view asks for them, the
//! home window's 10× look-out and then the window itself), the matrix questions asked in words, and integrals
//! whose dx touches the integrand.
//!   cargo run --release -p qcalc-core --example new_features_bench [case number]   (QC_RUNS=n for more runs)
use std::time::Instant;

use qcalc_core::evaluate::evaluate_line;
use qcalc_core::graph::GraphOptions;
use qcalc_core::graph_native::native_intersections;
use qcalc_core::types::EvaluateOptions;

/// Median ms of `runs` warm calls, after one cold. With a case number as the argument, only that case runs (so the
/// process's peak RSS is that case's: `/usr/bin/time -l`).
fn measure(name: &str, runs: usize, mut f: impl FnMut()) {
    f();
    let mut ts = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        f();
        ts.push(t.elapsed().as_secs_f64() * 1e3);
    }
    ts.sort_by(f64::total_cmp);
    println!("{name:<60} {:>8.3} ms", ts[runs / 2]);
}

fn main() {
    let only: Option<usize> = std::env::args().nth(1).and_then(|a| a.parse().ok());
    let mut case = 0usize;
    let mut pick = || {
        case += 1;
        only.is_none_or(|o| o == case)
    };
    let o = GraphOptions::default();
    let g = |v: &[&str]| v.iter().map(|e| format!("graph {e}")).collect::<Vec<_>>();
    for (name, inputs, lo, hi) in [
        ("x^2; 12x, home look-out", g(&["x^2", "y=12x"]), -50.0, 50.0),
        ("x^2; 12x, window", g(&["x^2", "y=12x"]), -10.0, 10.0),
        ("tan x; 0", g(&["tan(x)", "0"]), -10.0, 10.0),
        (
            "4 curves (sin, cos, 0.1, x/5)",
            g(&["sin(x)", "cos(x)", "0.1", "x/5"]),
            -10.0,
            10.0,
        ),
        (
            "6 curves",
            g(&["sin(x)", "cos(x)", "x^2-3", "x/5", "e^(x/4)", "{0<x<5} 2"]),
            -10.0,
            10.0,
        ),
    ] {
        if !pick() {
            continue;
        }
        let runs = std::env::var("QC_RUNS")
            .ok()
            .and_then(|r| r.parse().ok())
            .unwrap_or(15);
        measure(&format!("intersections: {name}"), runs, || {
            std::hint::black_box(native_intersections(&inputs, &o, lo, hi));
        });
    }
    let e = EvaluateOptions::default();
    for line in [
        "cofactor matrix of [1 2; 3 4]",
        "minor M12 of [1 2 0; 3 4 5; 6 7 8]",
        "rotation matrix about the z axis by 90°",
        "is [1 2 2] in the span of [1 0 0], [0 1 1]",
        "orthogonal complement of [1 2 3]",
        "singular values of [3 0; 0 4]",
        "change of basis matrix from [1 1], [1 -1] to [1 0], [1 1]",
        "[4 0; 0 9]^(1/2)",
        "∫_12^3 x^2dx",
        "√3/6",
    ] {
        if !pick() {
            continue;
        }
        let runs = std::env::var("QC_RUNS")
            .ok()
            .and_then(|r| r.parse().ok())
            .unwrap_or(31);
        measure(line, runs, || {
            std::hint::black_box(evaluate_line(line, &e));
        });
    }
}
