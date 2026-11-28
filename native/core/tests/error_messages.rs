//! Native only (22 Nov 2026): lines that went blank, or answered wrongly, now say what's wrong.

use qcalc_core::app::blank_reason::{EnterFacts, Names, blank_reason, enter_hint};
use qcalc_core::app::missing_unit::missing_unit;
use qcalc_core::{EvaluateOptions, evaluate_line};

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

/// The bar's answer, with the phrase layer (money, percents) on.
fn bar(t: &str) -> String {
    qcalc_core::app::live::live(&qcalc_core::app::live::LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    })
    .display
}

fn ok(t: &str) -> bool {
    let d = show(t);
    !d.trim().is_empty() && !d.contains("improper")
}

/// What ↵ says on a blank line.
fn hint(t: &str) -> String {
    let span = blank_reason(t, &|p| !show(p).trim().is_empty(), &Names::default());
    enter_hint(&EnterFacts {
        expr: t.into(),
        span,
        improper: Some(show(t).contains("improper")),
        ..Default::default()
    })
}

#[test]
fn a_bare_number_beside_a_quantity_is_offered_its_unit() {
    for (t, filled) in [
        ("6.3489 in + (-5.8221)", "6.3489 in + (-5.8221 in)"),
        ("161.262mm - 5.8221", "161.262mm - 5.8221 mm"),
        ("5 m + 3", "5 m + 3 m"),
    ] {
        assert_eq!(show(t), "improper unit conversion", "{t}");
        assert_eq!(
            missing_unit(t, &ok).map(|f| f.filled),
            Some(filled.into()),
            "{t}"
        );
    }
    assert_eq!(bar("10 m + 5%"), "10.5 m");
    assert_eq!(bar("1.5k + 500"), "2000");
    assert_eq!(show("2 + prod(k"), "");
    assert_eq!(show("2 m + 3 m"), "5 m");
}

#[test]
fn money_and_measures_dont_add() {
    assert_eq!(show("$5 + 3 m"), "improper unit conversion");
    assert_eq!(show("$5 * 3 kg"), "improper unit conversion");
    assert_eq!(bar("$5 + 3"), "$8.00");
    assert_eq!(bar("$4/gal * 12 gal"), "$48.00");
}

#[test]
fn a_quantity_over_zero_is_undefined() {
    assert_eq!(show("5 m / 0"), "undefined");
    assert_eq!(show("3 m / 0 m"), "undefined");
    assert_eq!(show("1/0"), "undefined");
}

#[test]
fn empty_calls_have_no_answer_and_say_why() {
    for t in ["max()", "sin()", "sqrt()", "mean()"] {
        assert_eq!(show(t), "", "{t}");
    }
    assert_eq!(
        hint("max()"),
        "max() needs something inside, like max(2, 5)"
    );
    assert_eq!(hint("sin()"), "sin() needs something inside, like sin(2)");
}

#[test]
fn limits_and_integrals_say_why() {
    assert_eq!(
        show("lim x->0 1/x"),
        "no limit: −∞ from the left (x→0⁻), ∞ from the right"
    );
    assert_eq!(show("lim x->0 1/x^2"), "∞");
    assert_eq!(
        show("∫ 1/x dx from 0 to 1"),
        "no answer: the function goes to ∞ at x = 0"
    );
    assert_eq!(show("∫ x^2 dx from 0 to 1"), "0.333333333333");
}

#[test]
fn lists_of_different_lengths_say_so() {
    assert_eq!(
        show("[1,2]+[1,2,3]"),
        "can't add lists of 2 and 3: lengths must match"
    );
    assert_eq!(
        show("[1,2]^2"),
        "a list can't take ^ · .^ raises each entry"
    );
    assert_eq!(show("[1,2]+[3,4]"), "[4, 6]");
}

#[test]
fn enter_hints_for_misused_lines() {
    assert_eq!(hint("2..3"), "a number there has two decimal points");
    assert_eq!(hint("sin(5 m)"), "sin takes a plain number, not m");
    assert_eq!(hint("e^(2 m)"), "a power can’t have a unit (m)");
    assert_eq!(show("sin(30 deg)"), "0.5");
}

#[test]
fn rounding_and_comparing_quantities() {
    assert_eq!(show("round(5.5 m)"), "6 m");
    assert_eq!(show("round(5.4567 m, 2)"), "5.46 m");
    assert_eq!(show("floor(5.5 kg)"), "5 kg");
    assert_eq!(show("min(1 ft, 2 m)"), "1 ft");
    assert_eq!(show("max(1 ft, 2 m)"), "2 m");
    assert_eq!(show("sum(1 m, 2 m)"), "3 m");
    assert_eq!(show("mean(1 m, 3 m)"), "2 m");
    assert_eq!(show("min(1 ft, 2 kg)"), "improper unit conversion");
    assert_eq!(
        missing_unit("min(1 ft, 2)", &ok).map(|f| f.filled),
        Some("min(1 ft, 2 ft)".into())
    );
}

#[test]
fn fuel_economy() {
    assert_eq!(show("40 mpg"), "40 mpg");
    assert_eq!(show("40 mpg to L/100km"), "5.88036458333 L/100km");
    assert_eq!(show("8 L/100km to km/L"), "12.5 km/L");
    assert_eq!(show("300 mi / 25 mpg"), "12 gal");
}

#[test]
fn words_for_maths() {
    assert_eq!(show("3:30 + 1:45"), "");
    assert_eq!(bar("3:30 + 1:45"), "5:15");
    assert_eq!(bar("2:30:15 - 0:45:30"), "1:44:45");
    assert_eq!(show("log base 2 of 8"), "3");
    assert_eq!(show("sum(1 to 10)"), "55");
    assert_eq!(show("mean 1 2 3"), "2");
    assert_eq!(show("average of 1, 2, 3"), "2");
    assert_eq!(show("1/2 of 10"), "5");
    assert_eq!(show("fib(10) * 2"), "110");
    assert_eq!(show("mode(1, 2, 2, 3)"), "2");
    assert_eq!(show("simplify (x^2-1)/(x-1)"), "x + 1");
    assert_eq!(show("expand (x+1)^2"), "x² + 2x + 1");
    assert_eq!(show("factor x^2 + 5x + 6"), "(x + 2)(x + 3)");
    assert_eq!(show("factor 12"), "2² × 3");
    assert_eq!(show("is 91 prime"), "91 isn’t prime: 7 × 13");
    assert_eq!(bar("5k"), "5000");
    assert_eq!(bar("$40 * 15%"), "$6.00");
    assert_eq!(show("pi r^2"), "");
}

#[test]
fn more_mistakes_are_named() {
    assert_eq!(hint("5 x 3"), "x is a letter here · use * or × to multiply");
    assert_eq!(hint("x = 5 = 6"), "a line can have only one =");
    assert_eq!(
        hint("e^ipi"),
        "write i pi or i*pi, with a space or *, for i times π"
    );
    assert_eq!(
        hint("1.000,5 + 1"),
        "a comma is a thousands mark here · write 1000.5 for a decimal"
    );
    assert_eq!(
        hint("300 K + 5"),
        "5 has no unit · write 5 K for a change in temperature"
    );
}
