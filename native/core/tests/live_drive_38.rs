//! QA item 38 (live drive): bugs seen driving the bar.

use qcalc_core::app::typst_math::native_typst_document;

/// A half-typed `$sqrt(2$` previewed as `$√2$ = √2`: the grey `)` went after the closing `$`, so the line no
/// longer read as Typst and its dollar signs were drawn.
#[test]
fn a_half_typed_typst_line_previews_without_its_dollar_signs() {
    let half = native_typst_document("$sqrt(2$", "sqrt(2)", "").unwrap();
    let whole = native_typst_document("$sqrt(2)$", "sqrt(2)", "").unwrap();
    assert_eq!(half, whole);
    assert!(!half.contains("\\$"), "{half}");
}
