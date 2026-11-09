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
    assert_eq!(
        after(
            &["A = [2 1; 1 3]"],
            &["det(A)", "det A", "the determinant of A"]
        ),
        ["5", "5", "5"]
    );
}

#[test]
fn characteristic_equation() {
    assert_eq!(show("charpoly([1 2; 3 4])"), "λ^2 - 5*λ - 2");
    assert_eq!(show("char poly [1 2; 3 4]"), "λ^2 - 5*λ - 2");
    assert_eq!(
        show("characteristic equation of [2 0; 0 3]"),
        "λ^2 - 5*λ + 6 = 0,  λ = 3, 2"
    );
    assert_eq!(
        exact("characteristic equation of [1 2; 3 4]").as_deref(),
        Some("λ^2 - 5*λ - 2 = 0,  λ = (5 ± sqrt(33))/2")
    );
    // det(A − λI) written out is the same polynomial
    let r = after(
        &["A = [2 1; 1 3]"],
        &["det(A - λI)", "det(A - x*eye(2))", "chareq(A)"],
    );
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
    let r = after(
        &["B = [1 2 3; 2 4 6]", "c = [6; 12]"],
        &["Bx = c", "B x = [1; 3]"],
    );
    assert_eq!(
        r[0],
        "x = [6; 0; 0] + t*[-2; 1; 0] + s*[-3; 0; 1] (any t, s)"
    );
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
    let r = after(
        &["A = [1 2 3; 4 5 6]", "v = [7 8 9]"],
        &["A[2, 1]", "A[1, :]", "A[:, 3]", "v[2]", "A[3, 1]"],
    );
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
    assert_eq!(
        exact("unit([1 1])").as_deref(),
        Some("[sqrt(2)/2 sqrt(2)/2]")
    );
    assert_eq!(
        exact("3 * unit([1 1])").as_deref(),
        Some("[3sqrt(2)/2 3sqrt(2)/2]")
    );
    // stored and read back exactly
    assert_eq!(after(&["u = unit([1 1])"], &["dot(u, u)"])[0], "1");
}

#[test]
fn messages_are_marked() {
    let r = evaluate_line("det([1 2 0; 3 4 0])", &EvaluateOptions::default());
    assert!(r.matrix.is_some_and(|m| m.message));
    assert!(
        evaluate_line("det([1 2; 3 4])", &EvaluateOptions::default())
            .matrix
            .is_some_and(|m| !m.message)
    );
}

#[test]
fn systems_on_one_line() {
    assert_eq!(show("2x + y = 5, x - y = 1"), "x = 2, y = 1");
    assert_eq!(show("2x + y = 5; x - y = 1"), "x = 2, y = 1");
    assert_eq!(show("solve 2x + y = 5, x - y = 1"), "x = 2, y = 1");
    // the current app's `solve(…)` call is left as it was
    assert_eq!(show("solve(2x + y = 5, x - y = 1)"), "");
    assert_eq!(
        show("solve x + 2y + z = 4, 2x - y + z = 1, x + y - z = 0"),
        "x = 1/3, y = 10/9, z = 13/9"
    );
    assert_eq!(show("x + y = 2, x + y = 3"), "no solution");
    // assignments and a function of two variables stay what they were
    assert_eq!(show("a = 1, b = 2"), "");
    assert_eq!(show("f(x, y) = x + y"), "f(x, y) = x + y");
}

#[test]
fn products_as_written() {
    assert_eq!(show("[1 2 3]·[4 5 6]"), "32");
    assert_eq!(show("[1 2 3] . [4 5 6]"), "32");
    assert_eq!(show("[1 2 3] × [4 5 6]"), "[-3 6 -3]");
    assert_eq!(show("2 × [1 2; 3 4]"), "[2 4; 6 8]");
    assert_eq!(show("[1 2; 3 4] · [5 6; 7 8]"), "[19 22; 43 50]");
    assert_eq!(show("[1 2; 3 4] .* [1 1; 2 2]"), "[1 2; 6 8]");
    assert_eq!(show("[1 2; 3 4] ./ [1 2; 3 4]"), "[1 1; 1 1]");
    assert_eq!(show("|[1 2; 3 4]|"), "-2");
    assert_eq!(show("|[3 4]|"), "5");
    let v = [
        "u = [1 2 3]",
        "v = [4 5 6]",
        "A = [1 2; 3 4]",
        "x = [1; 2; 3]",
    ];
    assert_eq!(
        after(&v, &["u · v", "u × v", "|A|", "x^T x", "A^T A"]),
        ["32", "[-3 6 -3]", "-2", "14", "[10 14; 14 20]"]
    );
}

#[test]
fn questions_as_a_textbook_asks() {
    assert_eq!(show("projection of [1 2] onto [3 4]"), "[1.32 1.76]");
    assert_eq!(show("angle between [1 0] and [1 1]"), "45");
    assert_eq!(show("the dot product of [1 2 3] and [4 5 6]"), "32");
    assert_eq!(show("cross product of [1 0 0] and [0 1 0]"), "[0 0 1]");
    assert_eq!(show("magnitude of [1 2 2]"), "3");
    assert_eq!(show("unit vector [3 4]"), "[0.6 0.8]");
    assert_eq!(
        show("basis for the null space of [1 2 3; 2 4 6]"),
        "span{[-2; 1; 0], [-3; 0; 1]}"
    );
    assert_eq!(show("dim null [1 2 3; 2 4 6]"), "2");
    assert_eq!(show("dimension of the column space of [1 2 3; 2 4 6]"), "1");
    assert_eq!(
        show("least squares [1 1; 1 2; 1 3] [1; 2; 2]"),
        "[0.666667; 0.5]"
    );
    assert_eq!(show("is [1 2; 2 4] invertible?"), "no (det = 0)");
    assert_eq!(show("is [2 1; 1 2] positive definite"), "yes");
    assert_eq!(
        show("are [1 0 0], [0 1 0] and [0 0 1] linearly independent"),
        "yes (rank 3 of 3)"
    );
    assert_eq!(
        after(&["A = [1 2; 2 4]"], &["is A invertible"]),
        ["no (det = 0)"]
    );
    // a sentence without a matrix is not one
    assert_eq!(show("the length of the trip"), "");
}

#[test]
fn asked_with_a_lead_in() {
    assert_eq!(show("find eigenvalues of [2 1; 1 2]"), "λ = 3, 1");
    assert_eq!(show("find the eigenvalues of [2 1; 1 2]"), "λ = 3, 1");
    assert_eq!(show("what are the eigenvalues of [2 1; 1 2]?"), "λ = 3, 1");
    assert_eq!(show("compute the determinant of [1 2; 3 4]"), "-2");
    assert_eq!(show("find det [1 2; 3 4]"), "-2");
    assert_eq!(show("what is the rank of [1 2; 2 4]"), "1");
    assert_eq!(
        after(&["A = [2 1; 1 2]"], &["find the eigenvalues of A?"]),
        ["λ = 3, 1"]
    );
    // nothing to do with a matrix: untouched
    assert_eq!(show("find 2+2"), "");
}

#[test]
fn more_textbook_questions() {
    assert_eq!(show("cofactor matrix of [1 2; 3 4]"), "[4 -3; -2 1]");
    assert_eq!(show("minor M12 of [1 2 0; 3 4 5; 6 7 8]"), "-6");
    assert_eq!(show("the (1, 2) cofactor of [1 2 0; 3 4 5; 6 7 8]"), "6");
    assert_eq!(show("rotation matrix 30°"), "[0.866025 -0.5; 0.5 0.866025]");
    assert_eq!(
        show("rotation matrix about the z axis by 90°"),
        "[0 -1 0; 1 0 0; 0 0 1]"
    );
    assert_eq!(show("[cos(30°) 1]"), "[0.866025 1]");
    assert_eq!(
        show("is [1 2 2] in the span of [1 0 0], [0 1 1]"),
        "yes (weights 1, 2)"
    );
    assert_eq!(show("is [1 2 3] in the span of [1 0 0] and [0 1 1]"), "no");
    assert_eq!(
        show("coordinates of [3 5] in basis [1 1], [1 -1]"),
        "[4; -1]"
    );
    assert_eq!(
        show("orthogonal complement of [1 2 3]"),
        "span{[-2; 1; 0], [-3; 0; 1]}"
    );
    assert_eq!(show("singular values of [3 0; 0 4]"), "σ = 4, 3");
    assert_eq!(
        show("change of basis from [1 1], [1 -1]"),
        "P = [1 1; 1 -1],  P⁻¹ = [0.5 0.5; 0.5 -0.5]"
    );
    assert_eq!(
        show("change of basis matrix from [1 1], [1 -1] to [1 0], [1 1]"),
        "P = [0 2; 1 -1]"
    );
    assert_eq!(show("outer product of [1 2] and [3 4]"), "[3 4; 6 8]");
    assert_eq!(show("[4 0; 0 9]^(1/2)"), "[2 0; 0 3]");
    assert_eq!(show("matrix square root of [4 0; 0 9]"), "[2 0; 0 3]");
    assert_eq!(
        after(&["A = [1 2 0; 3 4 5; 6 7 8]"], &["minor 1,2 of A"]),
        ["-6"]
    );
    // not a matrix question
    assert_eq!(show("rotation 30"), "");
}

/// The bar writes `√` for a typed `sqrt`, so a root in an entry stays one entry (found on Windows).
#[test]
fn roots_and_pi_in_entries() {
    assert_eq!(show("det([√3 1; 0 2])"), "3.46410161514");
    assert_eq!(exact("det([√3 1; 0 2])").as_deref(), Some("2sqrt(3)"));
    assert_eq!(show("det([√(3) 1; 0 2])"), "3.46410161514");
    assert_eq!(show("[√3 1; 0 2]"), "[1.73205 1; 0 2]");
    assert_eq!(show("[√3 -1; 1 √3]"), "[1.73205 -1; 1 1.73205]");
    assert_eq!(show("[2√3 1; 0 2]"), "[3.4641 1; 0 2]");
    assert_eq!(show("[√3 √2]"), "[1.73205 1.41421]");
    assert_eq!(show("[1 -√2]"), "[1 -1.41421]");
    assert_eq!(show("[∛8 1]"), "[2 1]");
    assert_eq!(show("[π 1]"), "[3.14159 1]");
    assert_eq!(show("[2π 1; 0 1]"), "[6.28319 1; 0 1]");
    assert_eq!(
        exact("inv([√2 0; 0 1])").as_deref(),
        Some("[sqrt(2)/2 0; 0 1]")
    );
    assert_eq!(show("[1 -2]"), "[1 -2]");
}
