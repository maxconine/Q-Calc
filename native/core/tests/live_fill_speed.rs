//! QA item 107: item 57's grey-paren fill evaluates a half-typed line again with its parens closed, so a
//! half-typed time derivative (`d/dt (theta^2 dot(the`) ran simplify's rule matching every keystroke, 4-6 ms
//! (release). The matcher now reads a split of an n-ary product or sum without copying it per split, merges a
//! single child match by moving it, and charges a match's key length without printing it: about 40% less, the
//! same answers and the same simplify budget spent.
use qcalc_core::app::live::{LiveRequest, live};
use qcalc_core::evaluate_sheet;
use qcalc_core::types::EvaluateOptions;
use std::time::Instant;

fn shown(text: &str) -> String {
    live(&LiveRequest {
        text: text.into(),
        ..Default::default()
    })
    .display
}

#[test]
fn half_typed_time_derivatives_answer_as_before() {
    for (line, answer) in [
        ("d/dt (theta^2 dot(the", "2dehotθ·(tθ̇ + θ)"),
        ("d/dt (theta^2 dot(theta", "2θ θ̇² + θ²θ̈"),
        ("d/dt (e^(k theta) thetadot", "e^(kθ) (k θ̇² + θ̈)"),
        ("d²/dt² (L sin(theta", "L·(θ̈ cos(θ) - θ̇² sin(θ))"),
        ("d/dt (x sin(theta", "ẋ sin(θ) + xθ̇ cos(θ)"),
        ("d/dt sqrt(x^2 + h^2", "xẋ/√(x² + h²)"),
        ("d/dt (t^2 s", "2st + t²ṡ"),
    ] {
        assert_eq!(shown(line), answer, "{line}");
    }
}

#[test]
fn simplify_matching_still_reads_every_split() {
    let o = EvaluateOptions::default();
    let show = |l: &str| evaluate_sheet(&[l.to_string()], &o).pop().unwrap().display;
    // commutative splits (repeated factors across a long product), non-commutative ones (/), sums
    assert_eq!(show("d/dx x^2 y x z"), "3x²yz");
    assert_eq!(show("d/dx a b x a b x"), "2a² b²x");
    assert_eq!(show("d/dx (2x + 3y - x + y*2)^2"), "2(x + 5y)");
    assert_eq!(show("d/dx x/y/z*x"), "2x/(yz)");
    assert_eq!(show("d/dt (x y z w)"), "wxyż + wxzẏ + wyzẋ + xyzẇ");
    assert_eq!(show("d/dt (theta^2 thetadot)"), "2θ θ̇² + θ²θ̈");
}

#[test]
fn a_half_typed_derivative_stays_quick() {
    let line = "d/dt (theta^2 dot(the";
    shown(line);
    let t = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(shown(line));
    }
    let ms = t.elapsed().as_secs_f64() * 1e3;
    // release ~3.6 ms each (was ~6); debug builds are far slower, so this only catches a large regression
    let limit = if cfg!(debug_assertions) {
        4000.0
    } else {
        200.0
    };
    assert!(ms < limit, "20 keystrokes took {ms:.0} ms");
}
