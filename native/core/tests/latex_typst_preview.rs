//! QA backlog item 23: LaTeX and Typst input, and the preview row they draw.
use qcalc_core::app::typst_math::{preview_math, typst_preview_useful};
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

// 2026-11-20: LaTeX the calculator left blank
#[test]
fn latex_shorthands_evaluate() {
    for (t, want) in [
        (r"\frac12", "0.5"),
        (r"\frac34 + \frac{1}{4}", "1"),
        (r"\log_{2} 8", "3"),
        (r"\log_2 8", "3"),
        (r"\frac{d}{dx} x^3", "3x²"),
        (r"\frac{d^2}{dx^2} x^3", "6x"),
        (r"\frac{\partial}{\partial x} x^2 y", "2xy"),
        // unchanged
        (r"\log_2(8)", "3"),
        (r"\frac{1}{2} + \frac{1}{3}", "0.833333333333"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

// 2026-11-20: the preview drew Typst's own words, its $ signs and LaTeX's braces as typed text
#[test]
fn typed_typst_and_latex_preview_as_math() {
    for (t, want) in [
        ("$frac(1, 2)$", "((1) / (2))"),
        ("$2 dot.c 3$", "2 dot 3"),
        ("2 times 3", "2 dot 3"),
        ("lr(|-5|)", "abs(- 5)"),
        ("upright(e)^2", "e ^ 2"),
        ("integral_0^1 x^2 dif x", "integral_(0)^(1) x ^ 2 dif x"),
        // a sum's index is its variable, not nano or kilo set upright
        ("sum_(n=1)^10 n^2", "sum_(n = 1)^(10) n ^ 2"),
        ("product_(k=1)^5 k", "product_(k = 1)^(5) k"),
        // brackets group a subscript: x_(a+b) was drawn x₍ₐ₊ᵦ₎
        ("lim_(x -> 0) sin(x)/x", "lim_(x arrow.r 0) sin(x) / x"),
        ("x_(a+b)", "x_(a + b)"),
        ("10^{-3}", "10 ^ (- 3)"),
        (r"5\%", "5 %"),
        (
            r"\lim_{x \to 0} \frac{\sin x}{x}",
            "lim_(x arrow.r 0) ((sin(x)) / (x))",
        ),
        (r"\frac{d}{dx} x^3", "frac(dif, dif x) x ^ 3"),
        (r"\log_{2} 8", "log(8, 2)"),
        // unchanged
        ("x^2 = 4", "x ^ 2 = 4"),
        (r"\sum_{n=1}^{10} n^2", "sum_(n = 1)^(10) n ^ 2"),
        ("root(3, 27)", "root(3, 27)"),
    ] {
        assert_eq!(preview_math(t), want, "{t}");
    }
    assert!(typst_preview_useful("integral_0^1 x^2 dif x"));
    assert!(typst_preview_useful("$frac(1, 2)$"));
}
