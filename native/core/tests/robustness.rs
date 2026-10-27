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
