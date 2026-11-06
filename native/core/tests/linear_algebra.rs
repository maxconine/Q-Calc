//! Linear algebra the native engine works out exactly (native only; the TS reads matrices in floats).
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::EvaluateOptions;

/// The shown answer and its exact form.
fn both(t: &str) -> (String, Option<String>) {
    let r = evaluate_line(t, &EvaluateOptions::default());
    (r.display, r.exact)
}

fn show(t: &str) -> String {
    both(t).0
}

fn exact(t: &str) -> Option<String> {
    both(t).1
}

fn sheet(lines: &[&str]) -> Vec<String> {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&lines, &EvaluateOptions::default())
        .into_iter()
        .map(|r| r.display)
        .collect()
}

#[test]
fn float_answers_are_unchanged() {
    assert_eq!(show("det([1 2; 3 4])"), "-2");
    assert_eq!(show("[1 2; 3 4] * [5; 6]"), "[17; 39]");
    assert_eq!(both("inv([1 2; 3 4])"), ("[-2 1; 1.5 -0.5]".into(), Some("[-2 1; 3/2 -1/2]".into())));
    assert_eq!(show("inv([1 2; 2 4])"), "no inverse (determinant is 0)");
    assert_eq!(show("rank([1 2; 2 4])"), "1");
    assert_eq!(show("[1 2] + [1 2 3]"), "can't add 1×2 and 1×3: sizes must match");
}

#[test]
fn fractions_past_the_float_reader() {
    // 1/1234 is too fine for the float reader's fraction search
    assert_eq!(exact("det([1/1234 0; 0 1])").as_deref(), Some("1/1234"));
    assert_eq!(
        exact("inv([7 0; 0 1001])").as_deref(),
        Some("[1/7 0; 0 1/1001]")
    );
}

#[test]
fn ill_conditioned_but_exact() {
    // the 5×5 Hilbert matrix: floats call it nearly singular, its exact inverse is whole numbers
    let h = "inv([1 1/2 1/3 1/4 1/5; 1/2 1/3 1/4 1/5 1/6; 1/3 1/4 1/5 1/6 1/7; 1/4 1/5 1/6 1/7 1/8; 1/5 1/6 1/7 1/8 1/9])";
    let d = show(h);
    assert!(d.starts_with("[25 -300 1050 -1400 630;"), "{d}");
    assert_eq!(show("det([1 1/2; 1/2 1/3])"), "0.0833333333333");
    assert_eq!(exact("det([1 1/2; 1/2 1/3])").as_deref(), Some("1/12"));
}

#[test]
fn square_roots_stay_exact() {
    assert_eq!(
        both("det([sqrt(2) 1; 1 sqrt(2)])"),
        ("1".into(), None)
    );
    // written as the calculator writes a surd (`2sqrt(2)`, `sqrt(2)/2`)
    assert_eq!(exact("det([sqrt(3) 1; 0 2])").as_deref(), Some("2sqrt(3)"));
    assert_eq!(
        exact("inv([sqrt(2) 0; 0 1])").as_deref(),
        Some("[sqrt(2)/2 0; 0 1]")
    );
    assert_eq!(exact("norm([1 2 3])").as_deref(), Some("sqrt(14)"));
    assert_eq!(show("norm([3 4])"), "5");
    assert_eq!(show("det([sqrt(3) 1; 0 2])"), "3.46410161514");
    assert_eq!(show("norm([1 2 3])"), "3.74165738677");
}

#[test]
fn symbols_inside_a_matrix() {
    assert_eq!(show("det([k 2; 3 k])"), "k^2 - 6");
    assert_eq!(show("det([a b; c d])"), "a*d - b*c");
    assert_eq!(show("trace([a b; c d])"), "a + d");
    assert_eq!(show("[k 1; 0 k] * [1; 2]"), "[k+2; 2*k]");
    assert_eq!(show("[k 1; 0 k]^2"), "[k^2 2*k; 0 k^2]");
    assert_eq!(show("[x 0; 0 y]^T"), "[x 0; 0 y]");
    // a constant answer is a number again
    assert_eq!(show("det([k k; 1 1])"), "0");
    assert_eq!(show("inv([k k; 1 1])"), "no inverse (determinant is 0)");
}

#[test]
fn symbolic_inverse() {
    assert_eq!(show("inv([k 0; 0 2])"), "[1/k 0; 0 1/2]");
    let d = show("inv([a b; c d])");
    assert!(d.contains("d") && d.contains("a*d") && d.starts_with('['), "{d}");
}

#[test]
fn a_bare_unknown_is_still_blank() {
    // `det(A)` with no A stored is still being typed
    assert_eq!(show("det(A)"), "");
    assert_eq!(show("inv(B)"), "");
}

#[test]
fn stored_symbolic_matrices() {
    assert_eq!(
        sheet(&["A = [k 2; 3 k]", "det(A)", "A * [1; 1]"]),
        vec!["", "k^2 - 6", "[k+2; k+3]"]
    );
    assert_eq!(
        sheet(&["k = 4", "det([k 2; 3 k])"]),
        vec!["4", "10"]
    );
}

#[test]
fn stored_matrices_stay_exact() {
    // the float literal is 1/1009 rounded; the stored one is exact, so A times its inverse is exactly I
    assert_eq!(
        sheet(&["A = [1009 0; 0 1]", "B = inv(A)", "A * B"]),
        vec!["[1009 0; 0 1]", "[0.00099108 0; 0 1]", "[1 0; 0 1]"]
    );
    let r = evaluate_sheet(
        &["B = inv([1009 0; 0 1])".to_string(), "B".to_string()],
        &EvaluateOptions::default(),
    );
    assert_eq!(r[0].exact.as_deref(), Some("[1/1009 0; 0 1]"));
}

#[test]
fn nearly_singular_as_typed_is_still_invertible() {
    // the floats can't trust this inverse; exact arithmetic on the entries as typed can
    assert_eq!(
        show("inv([1 1; 1 1.0000000001])"),
        "[10000000001 -10000000000; -10000000000 10000000000]"
    );
}
