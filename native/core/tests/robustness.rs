//! Inputs the fuzz harness (`bench/fuzz.rs`) found crashing, hanging or eating memory, each kept to its minimal
//! form. They run on a 32 MB thread, the stack the app's engine thread has (`ffi/src/worker.rs`).
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

/// Runs `f` on an engine-sized stack.
fn on_engine_stack(f: impl FnOnce() + Send + 'static) {
    on_stack(32, f);
}

fn on_stack(mb: usize, f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(mb * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn nest(open: &str, inner: &str, close: &str, n: usize) -> String {
    format!("{}{inner}{}", open.repeat(n), close.repeat(n))
}

#[test]
fn deep_brackets_dont_overflow_the_stack() {
    on_engine_stack(|| {
        // the unit reader's parser recursed once per bracket: 20,000 overflowed the engine's stack in a release
        // build, and a quarter of that does in a debug one
        assert_eq!(show(&"(".repeat(6_000)), "");
        assert_eq!(show(&nest("(", "1", ")", 6_000)), "");
        // and so did the chemical formula reader (100,000 in a release build)
        assert_eq!(show(&nest("(", "H2O", ")", 8_000)), "");
        // as deep as the TS engine reads them, both still read
        assert_eq!(show(&nest("(", "2 m", ")", 600)), "2 m");
        assert_eq!(show(&nest("(", "H2O", ")", 3_000)), "18.015 g/mol");
        // and at the deepest they read, a debug build's stack still holds
        show(&nest("(", "2 m", ")", 1_000));
        show(&nest("(", "H2O", ")", 4_999));
        assert_eq!(show("molar mass ((H2O)2)3"), show("molar mass H12O6"));
    });
}

#[test]
fn deep_calls_that_never_evaluate_stay_quick() {
    on_engine_stack(|| {
        // each call that failed was worked out again from inside: 1000 deep took 13 s
        assert_eq!(show(&nest("abs(", "-1", ")", 1000)), "");
        assert_eq!(show(&nest("sin(", "1", ")", 1000)), "");
        assert_eq!(show(&nest("abs(", "-1", ")", 30)), "1");
        assert_eq!(show("sqrt(sqrt(16 m^4))"), "2 m");
    });
}

#[test]
fn derivatives_that_explode_give_up() {
    on_engine_stack(|| {
        // d/dx of a power tower: seconds per extra level and gigabytes past fifteen
        let tower = |n: usize| format!("d/dx {}", vec!["x"; n].join("^"));
        assert_eq!(show(&tower(40)), "");
        assert_eq!(show(&format!("{}x^x", "d/dx ".repeat(30))), "");
        assert!(show(&tower(4)).starts_with("x^(x^(x^x) + x^x - 1)"));
    });
}

fn live_display(text: &str) -> String {
    use qcalc_core::app::live::{LiveRequest, live};
    let req = LiveRequest {
        text: text.to_string(),
        phrases: true,
        ..Default::default()
    };
    live(&req).display
}

#[test]
fn long_blank_lines_stay_quick_live() {
    on_engine_stack(|| {
        // the squiggle under a stray operator tried the line once per operator: 6 s for `e^/^/…` 1,000 long
        assert_eq!(live_display(&format!("e{}", "^/".repeat(500))), "");
        assert_eq!(
            live_display(&format!("solve for x: y {}", "= ".repeat(500))),
            ""
        );
        // isolate simplified a product of 500 letters for 11 s
        assert_eq!(
            live_display(&format!("isolate x in x^5 + x = {}", "y ".repeat(500))),
            ""
        );
        assert_eq!(live_display("2++3"), "5");
    });
}

#[test]
fn graph_options_from_an_odd_panel_stay_bounded() {
    use qcalc_core::graph::{GraphOptions, build_graph, sample_graph};
    on_engine_stack(|| {
        // a contour's grid has a row per column times the aspect: 1e9 (or a panel of no width) asked for 116 TB
        for aspect in [1e9, f64::INFINITY, 0.0, -1.0, f64::NAN] {
            let o = GraphOptions {
                aspect: Some(aspect),
                ..Default::default()
            };
            assert!(build_graph("graph x^2 + y^2 = 1", &o).is_some(), "{aspect}");
        }
        // an endless sample count was an endless loop; a NaN one still samples nothing, as in the TS
        let count = |n: f64| GraphOptions {
            sample_count: Some(n),
            ..Default::default()
        };
        assert_eq!(sample_graph("x", &count(f64::INFINITY)).len(), 100_000);
        assert_eq!(sample_graph("x", &count(f64::NAN)).len(), 0);
        assert_eq!(sample_graph("x", &count(161.0)).len(), 161);
    });
}

#[test]
fn choosing_from_more_than_doubles_count() {
    on_engine_stack(|| {
        // counting up to n by 1 never ends past 2^53 (mathjs hangs the same way)
        assert_eq!(show("nCr(999999999999999999999, 4)"), "4.16666666667e+82");
        assert_eq!(show("nCr(1e16, 2)"), "5e+31");
        // halving n − k + 1 … n never got down to one factor: a stack overflow, the app gone
        assert_eq!(show("nPr(1e17, 100)"), "overflow");
        assert_eq!(show("nPr(1e20, 3)"), "1e+60");
        assert_eq!(show("nCr(50,25)"), "1.26410606438e+14");
        assert_eq!(show("nPr(10, 3)"), "720");
    });
}
