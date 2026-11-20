//! A polynomial equation with no real root answers with its complex roots (native only).
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn both(t: &str) -> (String, Option<String>) {
    let r = evaluate_line(t, &EvaluateOptions::default());
    (r.display, r.exact)
}

#[test]
fn a_square_equal_to_a_negative_is_imaginary() {
    for t in ["x^(2)=-1", "x^2=-1", "x^2+1=0", "x²=-1"] {
        assert_eq!(both(t), ("±i".into(), None), "{t}");
    }
    assert_eq!(both("x^2 = -4"), ("±2i".into(), None));
    assert_eq!(
        both("x^2=-2"),
        ("±1.41421356237i".into(), Some("±sqrt(2)i".into()))
    );
}

#[test]
fn a_quadratic_gives_its_conjugate_pair() {
    assert_eq!(both("x^2+2x+5=0"), ("-1 ± 2i".into(), None));
    assert_eq!(
        both("x^2+x+1=0"),
        (
            "-0.5 ± 0.866025403784i".into(),
            Some("(-1 ± sqrt(3)i)/2".into())
        )
    );
}

#[test]
fn a_higher_degree_lists_each_pair() {
    assert_eq!(
        both("x^4=-1").0,
        "-0.707107 ± 0.707107i, 0.707107 ± 0.707107i"
    );
}

#[test]
fn real_roots_are_unchanged() {
    assert_eq!(both("x^2=4").0, "±2");
    assert_eq!(both("x^3=1").0, "1");
}

#[test]
fn complex_roots_are_an_answer_not_a_message() {
    use qcalc_core::types::SolveOutcome;
    let r = evaluate_line("x^2=-1", &EvaluateOptions::default());
    assert_eq!(r.solve.map(|s| s.outcome), Some(SolveOutcome::Complex));
    let r = evaluate_line("x^2=x", &EvaluateOptions::default());
    assert_eq!(r.solve.map(|s| s.outcome), Some(SolveOutcome::Roots));
}

#[test]
fn the_preview_typesets_a_complex_answer() {
    use qcalc_core::app::typst_math::native_typst_document;
    for (line, answer) in [("solve x^2 = -1", "±i"), ("x^2+x+1=0", "(-1 ± sqrt(3)i)/2")] {
        let doc = native_typst_document(line, answer, "x");
        assert!(doc.is_some(), "{line}");
        eprintln!("{line} => {doc:?}");
    }
}
