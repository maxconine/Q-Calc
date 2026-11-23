//! QA item 109: the FUZZ_TONIGHT fuzz re-run on new seeds.
use qcalc_core::eval::builtins::as_ratio;
use qcalc_core::eval::ops::real_pow;
use qcalc_core::graph::{GraphOptions, build_graph};
use qcalc_core::{EvaluateOptions, evaluate_line};

fn on_engine_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

/// `x^1e-10 = 1 m for x` isolates to `Q^1e10`, and rationalize's canonical form asked for a coefficient list of
/// 10^10 entries: 50 s and 4 GB before the line went blank (`units don't match` now at once).
#[test]
fn a_tiny_power_with_a_unit_is_quick() {
    on_engine_stack(|| {
        for (line, want) in [
            ("x^1e-10 = 1 m for x", "units don't match"),
            ("solve x^1e-10 = 1 m for x", "units don't match"),
            ("x^1e-8 = 1 m for x", "units don't match"),
            ("x^0.5 = Q for x", "x = Q^2"),
            ("isolate x in x^(1/3) = Q", "x = Q^3"),
        ] {
            let t = std::time::Instant::now();
            let got = evaluate_line(line, &EvaluateOptions::default()).display;
            let took = t.elapsed().as_millis();
            assert!(took < 2500 * ci_slack(), "{line}: {took} ms");
            assert_eq!(got, want, "{line}");
        }
        let r =
            |t: &str| qcalc_core::symbolic::rationalize(&qcalc_core::symbolic::parse(t).unwrap());
        assert!(r("Q^10000000000").is_err());
        let p = |t: &str| qcalc_core::symbolic::to_string(&r(t).unwrap(), &Default::default());
        assert_eq!(p("x^5 + 2x"), "x ^ 5 + 2 * x");
    });
}

/// `graph x^pi = y` on a tall plot took 2 s: a negative x to the power π asked `as_ratio(π)` its 10,000-step
/// search at each of 232,000 grid points. The last answer is kept, and the answers are the same.
#[test]
fn an_irrational_power_graphs_quickly() {
    assert_eq!(as_ratio(0.5), Some((1.0, 2.0)));
    assert_eq!(as_ratio(std::f64::consts::PI), None);
    assert_eq!(as_ratio(std::f64::consts::PI), None);
    assert_eq!(as_ratio(-2.0 / 3.0), Some((-2.0, 3.0)));
    assert_eq!(real_pow(-8.0, 1.0 / 3.0), -2.0);
    assert!(real_pow(-8.0, std::f64::consts::PI).is_nan());
    assert!((real_pow(-8.0, 2.0 / 3.0) - 4.0).abs() < 1e-12);
    for text in ["graph x^pi = y", "graph x^pi where x = -1e300"] {
        let o = GraphOptions {
            aspect: Some(16.0),
            ..Default::default()
        };
        let t = std::time::Instant::now();
        assert!(build_graph(text, &o).is_some(), "{text}");
        let took = t.elapsed().as_millis();
        assert!(took < 1500 * ci_slack(), "{text}: {took} ms");
    }
}

// shared CI runners are several times slower than a dev Mac; time limits get 5x there (answers are still checked)
fn ci_slack() -> u128 {
    if std::env::var_os("CI").is_some() {
        5
    } else {
        1
    }
}
