//! The linear algebra functions only the native engine has: eigen, systems, subspaces, decompositions, checks.
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::EvaluateOptions;

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

#[test]
fn eigenvalues() {
    assert_eq!(show("eig([2 0; 0 3])"), "λ = 3, 2");
    assert_eq!(show("eig([4 1; 2 3])"), "λ = 5, 2");
    assert_eq!(show("eig([1 2; 3 4])"), "λ = 5.37228, -0.372281");
    assert_eq!(
        exact("eig([1 2; 3 4])").as_deref(),
        Some("λ = (5 ± sqrt(33))/2")
    );
    assert_eq!(show("eig([0 -1; 1 0])"), "λ = ±i");
    assert_eq!(show("eig([1 -2; 2 1])"), "λ = 1 ± 2i");
    assert_eq!(show("eig([2 1; 0 2])"), "λ = 2 (×2)");
    assert_eq!(show("eigenvalues of [2 0 0; 0 3 4; 0 4 9]"), "λ = 11, 2, 1");
}

#[test]
fn eigenvectors() {
    assert_eq!(
        show("eigenvectors([2 0; 0 3])"),
        "λ = 3: [0; 1],  λ = 2: [1; 0]"
    );
    assert_eq!(
        show("eigenvectors([4 1; 2 3])"),
        "λ = 5: [1; 1],  λ = 2: [-1; 2]"
    );
}

#[test]
fn characteristic_polynomial() {
    assert_eq!(show("charpoly([1 2; 3 4])"), "λ^2 - 5*λ - 2");
    assert_eq!(
        show("characteristic polynomial of [2 0; 0 3]"),
        "λ^2 - 5*λ + 6"
    );
    assert_eq!(show("charpoly([k 1; 1 k])"), "λ^2 - 2*k*λ + (k^2 - 1)");
}

#[test]
fn diagonalize_and_jordan() {
    assert_eq!(
        show("diagonalize([4 1; 2 3])"),
        "P = [1 -1; 1 2],  D = [5 0; 0 2]"
    );
    assert!(show("diagonalize([2 1; 0 2])").starts_with("not diagonalizable"));
    assert_eq!(show("jordan([2 1; 0 2])"), "[2 1; 0 2]");
    assert_eq!(
        show("jordan([5 4 2 1; 0 1 -1 -1; -1 -1 3 0; 1 1 -1 2])"),
        "[4 1 0 0; 0 4 0 0; 0 0 2 0; 0 0 0 1]"
    );
}

#[test]
fn systems() {
    assert_eq!(show("solve([1 2; 3 4], [5; 6])"), "[-4; 4.5]");
    assert_eq!(
        exact("solve([1 2; 3 4], [5; 6])").as_deref(),
        Some("[-4; 9/2]")
    );
    assert_eq!(show("[2 1; 1 3] \\ [3; 5]"), "[0.8; 1.4]");
    assert_eq!(show("solve([1 2 | 5; 3 4 | 6])"), "[-4; 4.5]");
    assert_eq!(
        show("solve([1 1; 2 2], [1; 3])").split(':').next(),
        Some("no solution")
    );
    assert_eq!(
        show("solve([1 2 3; 2 4 6], [6; 12])"),
        "x = [6; 0; 0] + t*[-2; 1; 0] + s*[-3; 0; 1] (any t, s)"
    );
    assert_eq!(show("rref([1 2 | 5; 3 4 | 6])"), "[1 0 -4; 0 1 4.5]");
    // least squares: the best line through (0, 1), (1, 3), (2, 4)
    assert_eq!(
        exact("lsq([1 0; 1 1; 1 2], [1; 3; 4])").as_deref(),
        Some("[7/6; 3/2]")
    );
}

#[test]
fn subspaces() {
    assert_eq!(show("null([1 2 3; 2 4 6])"), "span{[-2; 1; 0], [-3; 0; 1]}");
    assert_eq!(
        show("null space of [1 0; 0 1]"),
        "{0} (only the zero vector)"
    );
    assert_eq!(show("colspace([1 2; 2 4])"), "span{[1; 2]}");
    assert_eq!(show("rowspace([1 2; 2 4])"), "span{[1 2]}");
    assert_eq!(show("nullity([1 2 3; 2 4 6])"), "2");
    assert_eq!(show("proj([1; 2], [1; 0])"), "[1; 0]");
    assert_eq!(show("proj([1 1 1], [[1, 0], [0, 1], [0, 0]])"), "[1 1 0]");
    assert_eq!(
        exact("unit([1 1])").as_deref(),
        Some("[sqrt(2)/2 sqrt(2)/2]")
    );
    assert_eq!(show("orth([1 1; 0 1])"), "[1 0; 0 1]");
    assert_eq!(show("angle([1 0], [0 1])"), "90");
    assert_eq!(show("basis([1 2 3; 2 4 7])"), "span{[1; 2], [3; 7]}");
}

#[test]
fn entries_and_builders() {
    assert_eq!(show("adj([1 2; 3 4])"), "[4 -2; -3 1]");
    assert_eq!(show("cofactor([1 2; 3 4])"), "[4 -3; -2 1]");
    assert_eq!(show("minor([1 2 3; 4 5 6; 7 8 10], 1, 1)"), "2");
    assert_eq!(show("entry([1 2; 3 4], 2, 1)"), "3");
    assert_eq!(show("row([1 2; 3 4], 2)"), "[3 4]");
    assert_eq!(show("col([1 2; 3 4], 2)"), "[2; 4]");
    assert_eq!(show("kron([1 2], [1; 1])"), "[1 2; 1 2]");
    assert_eq!(show("augment([1; 2], [3; 4])"), "[1 3; 2 4]");
    assert_eq!(show("stack([1 2], [3 4])"), "[1 2; 3 4]");
    assert_eq!(show("zeros(2, 3)"), "[0 0 0; 0 0 0]");
    assert_eq!(show("ones(2)"), "[1 1; 1 1]");
    assert_eq!(show("diag([1 2 3])"), "[1 0 0; 0 2 0; 0 0 3]");
}

#[test]
fn checks() {
    assert_eq!(show("issymmetric([1 2; 2 1])"), "yes");
    assert_eq!(show("isorthogonal([0 1; 1 0])"), "yes");
    assert_eq!(show("isinvertible([1 2; 2 4])"), "no (det = 0)");
    assert_eq!(show("isposdef([2 -1; -1 2])"), "yes");
    assert_eq!(
        show("isposdef([1 2; 2 1])"),
        "no (the leading 2×2 minor isn't positive)"
    );
    assert_eq!(
        show("isdiagonalizable([2 1; 0 2])").split(' ').next(),
        Some("no")
    );
    assert_eq!(show("islinind([1; 0], [0; 1], [1; 1])"), "no (rank 2 of 3)");
}

#[test]
fn decompositions() {
    assert_eq!(
        show("lu([4 3; 6 3])"),
        "L = [1 0; 1.5 1],  U = [4 3; 0 -1.5]"
    );
    assert_eq!(
        show("lu([0 1; 1 0])"),
        "P = [0 1; 1 0],  L = [1 0; 0 1],  U = [1 0; 0 1]"
    );
    assert_eq!(show("cholesky([4 2; 2 3])"), "[2 0; 1 1.41421]");
    assert_eq!(
        exact("cholesky([4 2; 2 3])").as_deref(),
        Some("[2 0; 1 sqrt(2)]")
    );
    assert_eq!(show("pinv([1 2; 2 4])"), "[0.04 0.08; 0.08 0.16]");
    assert_eq!(show("qr([3 1; 4 2])").split(",  ").count(), 2);
    assert_eq!(
        show("svd([3 0; 0 4])"),
        "U = [0 1; 1 0],  Σ = [4 0; 0 3],  V = [0 1; 1 0]"
    );
    assert_eq!(show("cond([1 0; 0 10])"), "10");
    assert_eq!(show("expm([0 1; 0 0])"), "[1 1; 0 1]");
    assert_eq!(show("sqrtm([4 0; 0 9])"), "[2 0; 0 3]");
}

#[test]
fn floats_and_symbols_reach_the_new_functions() {
    // an entry only the calculator reads
    assert_eq!(show("eig([pi 0; 0 1])"), "λ = 3.14159, 1");
    assert_eq!(show("det(rot(90))"), "1");
    assert_eq!(show("eig([k 0; 1 2])"), "λ = k, 2");
    assert_eq!(
        show("eig([a b; c d])"),
        "λ = (a + d ± sqrt(a^2 - 2*a*d + 4*b*c + d^2))/2"
    );
}

#[test]
fn stored_matrices_and_words() {
    let lines = [
        "A = [4 1; 2 3]",
        "b = [1; 2]",
        "eigenvalues of A",
        "A \\ b",
        "the null space of [1 1; 1 1]",
        "det(A)",
        "diagonalize A",
    ];
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let r: Vec<String> = evaluate_sheet(&lines, &EvaluateOptions::default())
        .into_iter()
        .map(|r| r.display)
        .collect();
    assert_eq!(r[2], "λ = 5, 2");
    assert_eq!(r[3], "[0.1; 0.6]");
    assert_eq!(r[4], "span{[-1; 1]}");
    assert_eq!(r[5], "10");
    assert_eq!(r[6], "P = [1 -1; 1 2],  D = [5 0; 0 2]");
}

#[test]
fn ordinary_lines_keep_their_meaning() {
    let r = evaluate_sheet(
        &["row = 3".to_string(), "row + 1".to_string()],
        &EvaluateOptions::default(),
    );
    assert_eq!(r[1].display, "4");
    assert!(!show("unit(5)").contains("span"));
}
