//! Matrix answers in labeled parts, the row operations behind them, and storing parts by name. Native only.
use qcalc_core::app::history::history_quantities;
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::{EvaluateOptions, LineKind, LineResult, MatrixParts};
use serde_json::json;

fn line(t: &str) -> LineResult {
    evaluate_line(t, &EvaluateOptions::default())
}

fn parts(t: &str) -> MatrixParts {
    line(t).matrix.unwrap_or_default()
}

fn sheet(lines: &[&str]) -> Vec<LineResult> {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&lines, &EvaluateOptions::default())
}

fn ops(t: &str) -> Vec<String> {
    parts(t).steps.into_iter().map(|s| s.op).collect()
}

#[test]
fn labeled_parts() {
    let m = parts("diagonalize([4 1; 2 3])");
    let labels: Vec<&str> = m.parts.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["P", "D"]);
    assert_eq!(m.parts[0].display, "[1 -1; 1 2]");
    assert_eq!(m.parts[0].literal.as_deref(), Some("[[1, -1], [1, 2]]"));
    assert_eq!(m.parts[1].typst, r#"D = mat(delim: "[", 5, 0; 0, 2)"#);
    assert_eq!(
        m.typst(false),
        r#"P = mat(delim: "[", 1, - 1; 1, 2) quad D = mat(delim: "[", 5, 0; 0, 2)"#
    );

    let m = parts("eig([1 2; 3 4])");
    assert_eq!(m.parts[0].label, "λ");
    assert_eq!(m.parts[0].exact.as_deref(), Some("(5 ± sqrt(33))/2"));
    assert!(m.parts[0].exact_typst.as_deref().is_some_and(|t| t.contains("sqrt(33)")));

    // Σ is a name, not a sum
    assert!(parts("svd([3 0; 0 4])").parts[1].typst.starts_with("Sigma = "));
    // words stay words; a general solution keeps its note apart
    assert_eq!(parts("isinvertible([1 2; 2 4])").parts[0].typst, r#""no (det = 0)""#);
    let family = parts("solve([1 2 3; 2 4 6], [6; 12])");
    assert_eq!(family.parts[0].label, "x");
    assert_eq!(family.parts[0].note.as_deref(), Some("(any t, s)"));
    assert!(family.parts[0].typst.ends_with(r#"quad "(any t, s)""#));
    // a plain answer is one unlabeled part; a message has none
    let one = parts("[1 2; 3 4] * 2");
    assert_eq!(one.parts.len(), 1);
    assert_eq!(one.parts[0].label, "");
    assert!(line("inv([1 2; 2 4])").matrix.is_none());
}

#[test]
fn row_operation_steps() {
    assert_eq!(
        ops("rref([1 2 | 5; 3 4 | 6])"),
        ["Start", "R₂ → R₂ − 3R₁", "R₂ → −(1/2)R₂", "R₁ → R₁ − 2R₂"]
    );
    let s = parts("rref([1 2 | 5; 3 4 | 6])").steps;
    assert_eq!(s[3].display, "[1 0 | -4; 0 1 | 9/2]");
    assert!(s[0].typst.contains("augment: #2"));

    let det = ops("det([0 2 1; 1 1 1; 2 1 3])");
    assert_eq!(det[1], "R₁ ↔ R₂");
    assert_eq!(det.last().map(String::as_str), Some("det = −(1 · 2 · 3/2) = -3 (one swap flips the sign)"));

    let inv = ops("inv([2 1; 1 1])");
    assert_eq!(inv.last().map(String::as_str), Some("the right side is the inverse"));
    assert_eq!(parts("inv([2 1; 1 1])").steps[0].display, "[2 1 | 1 0; 1 1 | 0 1]");

    assert_eq!(ops("[2 1; 1 3] \\ [3; 5]").len(), 5);
    assert_eq!(ops("rank([1 2; 2 4])").last().map(String::as_str), Some("rank = 1 (rows with a pivot)"));
    // symbols are carried through exactly
    assert_eq!(ops("det([k 2; 3 k])")[1], "R₂ → R₂ − (3/k)R₁");
    // nothing to do, nothing to show
    assert!(ops("rref([1 0; 0 1])").is_empty());
    assert!(ops("eig([1 2; 3 4])").is_empty());
}

#[test]
fn storing_parts_by_name() {
    let r = sheet(&[
        "A = [4 3; 6 3]",
        "L, U = lu(A)",
        "L * U",
        "P, L, U = lu([0 1; 1 0])",
        "P",
        "v1, v2 = eigenvectors([4 1; 2 3])",
        "v2",
        "Q, R = eig([1 2; 3 4])",
        "B, C = [1 2; 3 4]",
        "X, Y = diagonalize([4 1; 2 3])",
        "ans",
    ]);
    assert_eq!(r[1].display, "L = [1 0; 1.5 1],  U = [4 3; 0 -1.5]");
    assert_eq!(r[1].kind, LineKind::Assignment);
    assert_eq!(r[2].display, "[4 3; 6 3]");
    assert_eq!(r[4].display, "[0 1; 1 0]");
    assert_eq!(r[6].display, "[-1; 2]");
    assert_eq!(r[7].display, "this answer has 1 part (λ); give 1 name");
    assert_eq!(r[8].display, "this answer has 1 part; give 1 name");
    // ans keeps the first part
    assert_eq!(r[10].display, "[1 -1; 1 2]");
}

#[test]
fn fewer_names_match_by_label() {
    // `L, U` of a `P, L, U`
    let r = sheet(&["L, U = lu([0 1; 1 0])", "U"]);
    assert_eq!(r[1].display, "[1 0; 0 1]");
}

#[test]
fn history_reads_parts_back() {
    let rows = vec![
        json!({"id": "1", "expr": "A = [4 3; 6 3]", "display": "[4 3; 6 3]", "quantity": "[[4, 3], [6, 3]]"}),
        json!({"id": "2", "expr": "L, U = lu(A)", "display": "L = [1 0; 1.5 1],  U = [4 3; 0 -1.5]"}),
        json!({"id": "3", "expr": "M = [k 2; 3 k]", "display": ""}),
        json!({"id": "4", "expr": "N = 2 * [k 1; 1 k]", "display": "[2*k 2; 2 2*k]"}),
    ];
    let q = history_quantities(&rows);
    assert_eq!(q.get("L").and_then(|v| v.as_str()), Some("[[1, 0], [1.5, 1]]"));
    assert_eq!(q.get("U").and_then(|v| v.as_str()), Some("[[4, 3], [0, -1.5]]"));
    assert_eq!(q.get("M").and_then(|v| v.as_str()), Some("[[k, 2], [3, k]]"));
    let options = EvaluateOptions {
        quantities: q
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect(),
        ..EvaluateOptions::default()
    };
    assert_eq!(evaluate_line("det(N)", &options).display, "4*k^2 - 4");
    assert_eq!(evaluate_line("L * U", &options).display, "[4 3; 6 3]");
}
