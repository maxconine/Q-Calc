//! QA backlog item 92: typeset drawing seen live on a re-signed copy.
use qcalc_core::app::typst_math::{native_typst_document, preview_math};

fn body(expr: &str, answer: &str) -> String {
    let doc = native_typst_document(expr, answer, "").unwrap_or_default();
    doc.lines().last().unwrap_or("").to_string()
}

// 2026-11-20: `\frac{1}{2} + \sqrt{x}` and `$frac(1, 2)$` drew (½) in brackets the calculator only needs
#[test]
fn typed_fractions_preview_without_their_brackets() {
    for (t, want) in [
        (r"\frac{1}{2} + \sqrt{x}", "(1) / (2) + sqrt(x)"),
        ("$frac(1, 2)$", "(1) / (2)"),
        (r"\frac{\frac{1}{2}}{3}", "((1) / (2)) / (3)"),
        ("$sin(frac(1, 2))$", "sin((1) / (2))"),
        // kept where they mean something: a power, a number beside it
        ("$frac(1, 2)^2$", "((1) / (2)) ^ 2"),
        ("$2 frac(1, 2)$", "2 ((1) / (2))"),
    ] {
        assert_eq!(preview_math(t), want, "{t}");
    }
}

// 2026-11-20: `\frac{d}{dx} x^3` previewed `= 3x ²`, the ² a small digit set apart
#[test]
fn raised_digits_in_the_preview_are_powers() {
    assert!(body(r"\frac{d}{dx} x^3", "3x²").ends_with("= 3 x^(2) $"));
    assert!(body("x", "2x³y²").ends_with("= 2 x^(3) y^(2) $"));
    assert!(body("x", "(x+1)²").ends_with("= (x + 1)^(2) $"));
    // a unit's ² is its text
    assert!(body("x", "5 m²").ends_with("= 5 \"m²\" $"));
}
