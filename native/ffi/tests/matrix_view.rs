//! A live matrix answer carries its labeled parts, its steps and ready preview documents.
use qcalc_ffi::{Engine, EngineConfig, EvalContext, LiveAnswer, LiveRequest, default_eval_options};

fn live(text: &str) -> LiveAnswer {
    let e = Engine::new(EngineConfig {
        is_windows: false,
        locale: "en_US".into(),
        data_dir: None,
        time_zone: None,
    });
    e.live(LiveRequest {
        text: text.into(),
        options: default_eval_options(),
        context: EvalContext::default(),
        chained: false,
        now_ms: 0,
        phrases: false,
    })
}

#[test]
fn parts_and_documents() {
    let a = live("diagonalize([4 1; 2 3])");
    let m = a.matrix.expect("a matrix view");
    assert_eq!(m.parts.len(), 2);
    assert_eq!(m.parts[0].label, "P");
    assert!(m.document.contains(r#"$ P = mat(delim: "[", 1, - 1; 1, 2) quad D = "#));
    assert!(m.exact_document.is_none());
    assert!(m.steps.is_empty());

    let e = live("eig([1 2; 3 4])").matrix.expect("a matrix view");
    assert!(e.exact_document.is_some_and(|d| d.contains("sqrt(33)")));
}

#[test]
fn steps_have_documents() {
    let m = live("rref([1 2 | 5; 3 4 | 6])").matrix.expect("a matrix view");
    assert_eq!(m.steps.len(), 4);
    assert!(m.steps[1].document.contains("R_2 arrow.l R_2 - 3 R_1 quad mat("));
    assert!(m.steps[1].document.starts_with("#import \"/macros.typ\""));
}

/// Every document the view hands out compiles.
#[cfg(feature = "typst")]
#[test]
fn documents_compile() {
    let typst = qcalc_ffi::TypstRenderer::new();
    for line in [
        "diagonalize([4 1; 2 3])",
        "eig([1 2; 3 4])",
        "eig([1 -2; 2 1])",
        "eig([2 1; 0 2])",
        "eigenvectors([4 1; 2 3])",
        "charpoly([k 1; 1 k])",
        "svd([3 0; 0 4])",
        "lu([0 1; 1 0])",
        "solve([1 2 3; 2 4 6], [6; 12])",
        "null([1 2 3; 2 4 6])",
        "isposdef([1 2; 2 1])",
        "unit([1 1])",
        "rref([1 2 | 5; 3 4 | 6])",
        "det([0 2 1; 1 1 1; 2 1 3])",
        "det([k 2; 3 k])",
        "inv([2 1; 1 1])",
        "rank([1 2; 2 4])",
        "rref([pi 1; 1 2])",
        "[2 1; 1 3] \\ [3; 5]",
    ] {
        let m = live(line).matrix.expect(line);
        let docs = std::iter::once(m.document.clone())
            .chain(m.exact_document.clone())
            .chain(m.steps.iter().map(|s| s.document.clone()));
        for doc in docs {
            if let Err(e) = typst.page_size(doc.clone()) {
                panic!("{line}: {e}\n{doc}");
            }
        }
    }
}

/// The live path (phrases on, as the bars run it) answers the textbook forms as a sheet does.
#[test]
fn live_textbook_forms() {
    for (line, want) in [
        ("characteristic equation of [1 2; 3 4]", "λ^2 - 5*λ - 2 = 0"),
        ("characteristic polynomial of [1 2; 3 4]", "λ^2 - 5*λ - 2"),
        ("eigenvalues of [2 0; 0 3]", "λ = 3, 2"),
        ("the null space of [1 1; 1 1]", "span{[-1; 1]}"),
        ("det(matrix 2x2)", ""),
        ("[2 1; 1 3] x = [3; 5]", "x = [0.8; 1.4]"),
        ("det([1 2 0; 3 4 0])", "determinant needs a square matrix (this is 2×3)"),
    ] {
        let e = Engine::new(EngineConfig {
            is_windows: false,
            locale: "en_US".into(),
            data_dir: None,
            time_zone: None,
        });
        let a = e.live(LiveRequest {
            text: line.into(),
            options: default_eval_options(),
            context: EvalContext::default(),
            chained: false,
            now_ms: 0,
            phrases: true,
        });
        assert!(a.display.starts_with(want), "{line}: {:?}", a.display);
    }
}

#[test]
fn no_view_for_plain_lines() {
    assert!(live("2 + 3").matrix.is_none());
    assert!(live("[k 2; 3 k]").matrix.is_none());
}
