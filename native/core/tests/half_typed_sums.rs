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
