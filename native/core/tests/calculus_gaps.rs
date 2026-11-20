//! QA backlog item 40: calculus gaps left by items 9 and 23.
use qcalc_core::app::typst_math::{preview_math, typst_preview_useful};
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::{AngleMode, EvaluateOptions};

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

fn show_rad(t: &str) -> String {
    let o = EvaluateOptions {
        angle_mode: Some(AngleMode::Rad),
        ..Default::default()
    };
    evaluate_line(t, &o).display
}

// 2026-11-20: in degrees (the default) sin x/x has zeros 180 apart, past the oscillating scan's reach, so
// ∫₀^∞ sin(x)/x was blank; the scan step now widens with the gap between zeros
#[test]
fn oscillating_tails_in_degrees() {
    assert_eq!(show("integral of sin(x)/x from 0 to inf"), "1.5707963268");
    assert_eq!(show("∫_0^∞ sin(x)/x dx"), "1.5707963268");
    // ∫ sin²(x°)/x² = (π/180)·π/2
    assert_eq!(show("integral of sin(x)^2/x^2 from 0 to ∞"), "0.0274155678");
    assert_eq!(
        show_rad("integral of sin(x)/x from 0 to inf"),
        "1.5707963268"
    );
}

// 2026-11-20: `log_2 8` without parens was blank (`log_2(8)` and `\log_2 8` read)
#[test]
fn bare_log_base() {
    for (t, want) in [
        ("log_2 8", "3"),
        ("log_2 8 + 1", "4"),
        ("2 log_2 8", "6"),
        ("log_10 1000", "3"),
        ("log_2 0.5", "-1"),
        ("x = 4; log_2 x", "2"),
        ("log_2(8)", "3"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    // a power or a name after the number is left to the parens
    assert_eq!(show("log_2 8x"), "");
}

// 2026-11-20: the CAS spelling `diff(f, x)` was blank (`diff` not a name)
#[test]
fn diff_call() {
    for (t, want) in [
        ("diff(x^3, x)", "3x²"),
        ("diff(x^3,x)", "3x²"),
        ("diff(x^3, x, 2)", "6x"),
        ("diff(x^5, x, 4)", "120x"),
        ("diff(t^2, t)", "2t"),
        ("diff(x^2 y, y)", "x²"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    assert_eq!(show("diff(1, 2)"), "");
    // mathjs's string form stays blank (web parity)
    assert_eq!(show("derivative(\"x^2\", \"x\")"), "");
}

// 2026-11-20: ODE conditions after a comma were blank (only `;` read them)
#[test]
fn ode_conditions_after_a_comma() {
    for (t, want) in [
        ("y' = y, y(0)=1", "y = e^x"),
        ("y'' = -y, y(0)=0, y'(0)=1", "y = sin(x)"),
        ("y'' = -y, y(0)=0; y'(0)=1", "y = sin(x)"),
        ("y' = 2x, y(1) = 3", "y = x^2 + 2"),
        ("dy/dx = y, y(0) = 2", "y = 2e^x"),
        // unchanged
        ("y' = y; y(0)=1", "y = e^x"),
        ("y' = y", "y = C e^x"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

// 2026-11-20: an integral with no bounds previewed as the word "int" and was hidden
#[test]
fn indefinite_integral_preview() {
    for (t, want) in [
        ("int x^2 dx", "integral x ^ 2 dif x"),
        ("∫ x^2 dx", "integral x ^ 2 dif x"),
        ("integral of x^2 dx", "integral x ^ 2 dif x"),
        ("integrate x^2", "integral x ^ 2 dif x"),
        ("∫ t^2 dt", "integral t ^ 2 dif t"),
        ("int x^2 dx from 0 to 1", "integral_(0)^(1) x ^ 2 dif x"),
        (
            "∫ sin(x)/x dx from 0 to ∞",
            "integral_(0)^(infinity) sin(x) / x dif x",
        ),
        // unchanged
        ("int_0^1 x^2 dx", "integral_(0)^(1) x ^ 2 dif x"),
        ("int 0..1 x^2 dx", "integral_(0)^(1) x ^ 2 dif x"),
    ] {
        assert_eq!(preview_math(t), want, "{t}");
        assert!(typst_preview_useful(t), "{t}");
    }
    assert!(!typst_preview_useful("integer(3.5)"));
}
