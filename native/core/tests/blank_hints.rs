//! QA item 24: blank lines that stop where their next part goes say so, and a derivative of a derivative is one
//! derivative (it was read as plain division: `d/dx d/dx x^3` was `-4/x⁵`).
use qcalc_core::app::live::{LiveRequest, commit, live};
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn hint(t: &str) -> String {
    let req = LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    };
    commit(&req, &[], "r").hint.unwrap_or_default()
}

fn squiggle(t: &str) -> bool {
    let req = LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    };
    live(&req).squiggle.is_some()
}

#[test]
fn unfinished_lines_say_what_goes_next() {
    for (t, want) in [
        ("d/dx", "what to differentiate"),
        ("the derivative of", "what to differentiate"),
        ("∫", "what to integrate"),
        ("integral of", "what to integrate"),
        ("lim", "where x goes"),
        ("lim x", "where x goes"),
        ("solve", "an equation goes next"),
        ("5 m to", "a unit goes next"),
        ("12 ft into", "a unit goes next"),
        ("f(x) =", "after the ="),
        ("[1, 2", "a [ has no ]"),
        ("sin", "sin needs something"),
    ] {
        assert!(hint(t).contains(want), "{t}: {}", hint(t));
        assert!(!squiggle(t), "{t} is unfinished, not wrong");
    }
    // unchanged
    assert_eq!(hint("2+*3"), "check the extra *");
    assert_eq!(hint("2+2=="), "unfinished: something goes after the =");
    assert!(hint("3 + foo").contains("“foo”"));
    assert!(squiggle("3 + foo"));
}

#[test]
fn a_derivative_of_a_derivative_is_one_derivative() {
    let show = |t: &str| evaluate_line(t, &EvaluateOptions::default()).display;
    assert_eq!(show("d/dx d/dx x^3"), "6x");
    assert_eq!(show("d/dx (d/dx x^3)"), "6x");
    assert_eq!(show("derivative of derivative of x^3"), "6x");
    assert_eq!(show("derivative of d/dx x^3"), "6x");
    assert_eq!(show("d/dx d/dx d/dx x^5"), "60x²");
    assert_eq!(show("d²/dx² d/dx x^4"), "24x");
    assert_eq!(show("d/dx d/dx x^3 at 2"), "12");
    assert_eq!(show("d/dx d/dy x^2 y^3"), "6x y²");
}
