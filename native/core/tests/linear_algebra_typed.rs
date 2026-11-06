//! Linear algebra the way a textbook writes it: determinants, the characteristic equation, `Ax = b`, `A[i, j]`,
//! `det(A − λI)`. Native only.
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

fn exact(t: &str) -> Option<String> {
    evaluate_line(t, &EvaluateOptions::default()).exact
}

/// Each line after the stored ones, shown.
fn after(stored: &[&str], lines: &[&str]) -> Vec<String> {
    let all: Vec<String> = stored.iter().chain(lines).map(|s| s.to_string()).collect();
    evaluate_sheet(&all, &EvaluateOptions::default())
        .into_iter()
        .skip(stored.len())
        .map(|r| r.display)
        .collect()
}

#[test]
fn determinants() {
    assert_eq!(show("det([1 2; 3 4])"), "-2");
    assert_eq!(show("determinant of [2 1 3; 0 4 5; 1 0 6]"), "41");
    assert_eq!(show("det([a b; c d])"), "a*d - b*c");
    assert_eq!(after(&["A = [2 1; 1 3]"], &["det(A)", "det A", "the determinant of A"]), ["5", "5", "5"]);
}

#[test]
fn characteristic_equation() {
    assert_eq!(show("charpoly([1 2; 3 4])"), "λ^2 - 5*λ - 2");
    assert_eq!(show("char poly [1 2; 3 4]"), "λ^2 - 5*λ - 2");
    assert_eq!(show("characteristic equation of [2 0; 0 3]"), "λ^2 - 5*λ + 6 = 0,  λ = 3, 2");
    assert_eq!(
        exact("characteristic equation of [1 2; 3 4]").as_deref(),
        Some("λ^2 - 5*λ - 2 = 0,  λ = (5 ± sqrt(33))/2")
    );
    // det(A − λI) written out is the same polynomial
    let r = after(&["A = [2 1; 1 3]"], &["det(A - λI)", "det(A - x*eye(2))", "chareq(A)"]);
    assert_eq!(r[0], "λ^2 - 5*λ + 5");
    assert_eq!(r[1], "x^2 - 5*x + 5");
    assert!(r[2].starts_with("λ^2 - 5*λ + 5 = 0,  λ = "));
}

#[test]
fn ax_equals_b() {
    let stored = ["A = [2 1; 1 3]", "b = [3; 5]"];
    let lines = [
        "A \\ b",
        "solve(A, b)",
        "Ax = b",
        "A x = b",
        "A*x = b",
        "solve Ax = b",
        "Ax = b for x",
        "[2 1; 1 3] x = [3; 5]",
    ];
    let r = after(&stored, &lines);
    assert_eq!(r[0], "[0.8; 1.4]");
    assert_eq!(r[1], "[0.8; 1.4]");
    for shown in &r[2..] {
        assert_eq!(shown, "x = [0.8; 1.4]");
    }
    assert_eq!(after(&stored, &["A y = b"])[0], "y = [0.8; 1.4]");
    // a family, and no solution
    let r = after(&["B = [1 2 3; 2 4 6]", "c = [6; 12]"], &["Bx = c", "B x = [1; 3]"]);
    assert_eq!(r[0], "x = [6; 0; 0] + t*[-2; 1; 0] + s*[-3; 0; 1] (any t, s)");
    assert!(r[1].starts_with("no solution"));
    // ordinary equations are untouched
    assert_eq!(show("2x = 6"), "3");
    assert_eq!(after(&["x = 4"], &["x + 1"])[0], "5");
}

#[test]
fn identity_and_unknowns() {
    let r = after(&["A = [2 1; 1 3]"], &["A - 2I", "A + I", "k*[1 2]"]);
    assert_eq!(r, ["[0 1; 1 1]", "[3 1; 1 4]", "[k 2*k]"]);
}

#[test]
fn indexing() {
    let r = after(&["A = [1 2 3; 4 5 6]", "v = [7 8 9]"], &["A[2, 1]", "A[1, :]", "A[:, 3]", "v[2]", "A[3, 1]"]);
    assert_eq!(r[0], "4");
    assert_eq!(r[1], "[1 2 3]");
    assert_eq!(r[2], "[3; 6]");
    assert_eq!(r[3], "8");
    assert!(r[4].contains("from 1 to 2"));
    assert_eq!(show("[1 2; 3 4][2, 2]"), "4");
    assert_eq!(show("inv([2 0; 0 4])[2, 2]"), "0.25");
    // a matrix after a matrix still multiplies
    assert_eq!(show("[1 2] [3; 4]"), "11");
}

#[test]
fn surds_as_the_calculator_writes_them() {
    assert_eq!(exact("unit([1 1])").as_deref(), Some("[sqrt(2)/2 sqrt(2)/2]"));
    assert_eq!(exact("3 * unit([1 1])").as_deref(), Some("[3sqrt(2)/2 3sqrt(2)/2]"));
    // stored and read back exactly
    assert_eq!(after(&["u = unit([1 1])"], &["dot(u, u)"])[0], "1");
}

#[test]
fn messages_are_marked() {
    let r = evaluate_line("det([1 2 0; 3 4 0])", &EvaluateOptions::default());
    assert!(r.matrix.is_some_and(|m| m.message));
    assert!(evaluate_line("det([1 2; 3 4])", &EvaluateOptions::default()).matrix.is_some_and(|m| !m.message));
}
