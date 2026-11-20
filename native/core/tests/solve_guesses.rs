//! Solve and isolate (QA item 8): a guess without `for x`, `solve(f, x, guess)`, and a named solve
//! whose letters were read as units (`E = 1/2 m v^2 for v` said "units don't match").
use qcalc_core::evaluate::evaluate_line;

fn display(t: &str) -> String {
    let o = qcalc_core::app::settings::default_settings().eval_options();
    evaluate_line(t, &o).display
}

#[test]
fn a_guess_without_for_x() {
    for (t, want) in [
        ("x^2 = 9 near 5", "3"),
        ("x^2 = 9 near -5", "-3"),
        ("sin x = 0 near 170", "180"),
        ("e^x = x + 2 near -2", "-1.84141"),
        ("x^2 = 9 for x near -5", "-3"),
    ] {
        assert_eq!(display(t), want, "{t}");
    }
}

#[test]
fn solve_call_with_a_third_argument_guess() {
    for (t, want) in [
        ("solve(x^2 - 9, x, 4)", "3"),
        ("solve(x^2 = 9, x, -4)", "-3"),
        ("solve(x^2 - 9, x = -4)", "-3"),
        ("solve(x^2 - 2, x)", "±1.41421"),
    ] {
        assert_eq!(display(t), want, "{t}");
    }
}

#[test]
fn named_letters_beat_a_unit_mismatch() {
    for (t, want) in [
        ("E = 1/2 m v^2 for v", "v = ±sqrt(2E/m)"),
        ("E = 0.5 m v^2 for v", "v = ±sqrt(E/(0.5 m))"),
        ("E = 1/2 k x^2 for x", "x = ±sqrt(2E/k)"),
        ("E = m v^2 / 2 for v", "v = ±sqrt(2E/m)"),
    ] {
        assert_eq!(display(t), want, "{t}");
    }
    // a real unit solve keeps its answer
    assert_eq!(display("x * 3 m = 12 m^2"), "4 m");
    assert_eq!(display("10 km = x * 2 hr"), "1.38889 m/s");
}
