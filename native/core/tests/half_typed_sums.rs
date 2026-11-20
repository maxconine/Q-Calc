//! QA item 72: a half-typed `prod(`/`sum(` stays blank instead of the grey paren's reading (`prod(k` was `1 K`,
//! one kelvin). Also pins the item's other report: `inv([1 1; 1 1.0000000001])` answers exactly as typed (a
//! documented divergence from the web's "nearly singular", MATRIX_PLAN.md), not a regression.
use qcalc_core::app::live::{live, LiveRequest};

fn shown(text: &str) -> String {
    live(&LiveRequest {
        text: text.into(),
        phrases: true,
        typst_preview: true,
        no_squiggle: true,
        now_ms: 1.7e12,
        ..Default::default()
    })
    .display
}

#[test]
fn half_typed_product_is_blank() {
    for half in [
        "prod(k",
        "prod(k, k",
        "prod(n",
        "prod(k^2",
        "product(k",
        "sum(k",
        "Π(k",
        "2 + prod(k",
    ] {
        assert_eq!(shown(half), "", "{half}");
    }
    assert_eq!(shown("prod(k, k, 1, 5"), "120");
    assert_eq!(shown("prod(k, k, 1, 5)"), "120");
    assert_eq!(shown("sum(k, k, 1, 5"), "15");
    assert_eq!(shown("prod(2, 3"), "6");
    // other grey-paren fills still answer
    assert_eq!(shown("det([1 2; 3 4]"), shown("det([1 2; 3 4])"));
}

#[test]
fn nearly_singular_inverse_is_exact_as_typed() {
    assert_eq!(
        shown("inv([1 1; 1 1.0000000001])"),
        shown("inv([1 1; 1 1.0000000001]")
    );
    assert!(shown("inv([1 1; 1 1.0000000001])").contains("10000000001"));
}

// QA item 100 (typing fuzz): item 72's blank also caught sums whose bounds were already typed, so
// `Σ(n^2, n = 1..10` (385 since item 57's grey-paren fill) went blank until the `)` was typed
#[test]
fn a_sum_with_its_bounds_typed_answers_before_the_paren() {
    for (half, answer) in [
        ("Σ(n^2, n = 1..10", "385"),
        ("sum(n^2, n=1..5", "55"),
        ("Σ(n^2, 1, 10", "385"),
        ("Σ(n^2, n=1, 10", "385"),
        ("sqrt(Σ(n^3, n=1..4", "10"),
        ("Σ(n, n=1..3) * Π(n, n=1..3", "36"),
    ] {
        assert_eq!(shown(half), answer, "{half}");
    }
    for half in ["sum(n^2, n=1..", "sum(k, k, 1", "sum(k, k=x, 5", "Σ(n^2, 1, "] {
        assert_eq!(shown(half), "", "{half}");
    }
}
