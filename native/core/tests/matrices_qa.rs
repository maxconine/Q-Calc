//! Matrix fixes from the QA pass (native only): `A(2, 1)` indexes, a parse failure elsewhere isn't "rows differ",
//! `A^-1` shows the inverse's working, a singular or inconsistent system still shows its steps and says why.
use qcalc_core::evaluate::evaluate_sheet;
use qcalc_core::types::{EvaluateOptions, LineResult};

fn run(lines: &[&str]) -> Vec<LineResult> {
    let all: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&all, &EvaluateOptions::default())
}

fn last(lines: &[&str]) -> LineResult {
    run(lines).pop().expect("a line")
}

fn ops(r: &LineResult) -> Vec<String> {
    r.matrix
        .as_ref()
        .map(|m| m.steps.iter().map(|s| s.op.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn parenthesised_indices() {
    let a = "A = [1 2; 3 4]";
    assert_eq!(last(&[a, "A(2,1)"]).display, "3");
    assert_eq!(last(&[a, "A(2, :)"]).display, "[3 4]");
    assert_eq!(last(&[a, "A(:, 1)"]).display, "[1; 3]");
    assert_eq!(last(&["[1 2; 3 4](1, 2)"]).display, "2");
    // one number in parentheses still multiplies
    assert_eq!(last(&[a, "A(2)"]).display, "[2 4; 6 8]");
    assert_eq!(last(&[a, "A[2, 1]"]).display, "3");
}

#[test]
fn ragged_only_when_a_matrix_is() {
    assert_eq!(last(&["[1 2; 3]"]).display, "rows differ in length");
    assert_eq!(last(&["det([1 2; 3])"]).display, "rows differ in length");
    assert_eq!(last(&["[[1,2],[3]]"]).display, "rows differ in length");
    // every matrix here is fine: it was "rows differ in length"
    assert_ne!(
        last(&["A = [1 2; 3 4]", "A == A"]).display,
        "rows differ in length"
    );
}

#[test]
fn power_minus_one_shows_the_inverse_working() {
    let inv = ops(&last(&["inv([1 2; 3 4])"]));
    assert_eq!(
        inv.last().map(String::as_str),
        Some("the right side is the inverse")
    );
    for line in ["[1 2; 3 4]^-1", "[1 2; 3 4]^(-1)", "[1 2; 3 4]⁻¹"] {
        assert_eq!(ops(&last(&[line])), inv, "{line}");
    }
    assert_eq!(ops(&last(&["A = [1 2; 3 4]", "A^-1"])), inv);
    // any other power has none
    assert!(ops(&last(&["[1 2; 3 4]^-2"])).is_empty());
}

#[test]
fn steps_say_why_there_is_no_answer() {
    let r = last(&["inv([1 2; 2 4])"]);
    assert_eq!(r.display, "no inverse (determinant is 0)");
    assert_eq!(
        ops(&r),
        [
            "Start",
            "R₂ → R₂ − 2R₁",
            "no inverse: the left side can't become I"
        ]
    );
    let r = last(&["[1 2; 2 4] \\ [1; 3]"]);
    assert!(r.display.starts_with("no solution"));
    assert_eq!(
        ops(&r).last().map(String::as_str),
        Some("R₂ reads 0 = 1: no solution")
    );
    let r = last(&["[1 2; 2 4] \\ [1; 2]"]);
    assert_eq!(
        ops(&r).last().map(String::as_str),
        Some("a free variable: infinitely many solutions")
    );
    // a unique solution ends on its last row operation, as before
    assert_eq!(
        ops(&last(&["[1 2; 3 4] \\ [5; 6]"]))
            .last()
            .map(String::as_str),
        Some("R₁ → R₁ − 2R₂")
    );
    // a size error still has none
    assert!(ops(&last(&["det([1 2; 3 4; 5 6])"])).is_empty());
}
