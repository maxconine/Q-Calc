// QA backlog item 1 (typing latency, closed-form upgrade): the upgraded exact form in the preview row.
use qcalc_core::app::typst_math::to_typst_math;

// 2026-11-20: the PSLQ upgrade `2 - π²/6` typeset as 2 − π·(²/6): a lone ² stayed a glyph, so the fraction took
// it as its numerator and drew the 2 over the 6
#[test]
fn raised_digit_before_a_fraction_bar_is_the_power() {
    assert_eq!(to_typst_math("2 - π²/6"), "2 - pi ^(2) / 6");
    assert_eq!(to_typst_math("π³ / 32"), "pi ^(3) / 32");
    // elsewhere ¹ ² ³ still typeset as the glyphs (the math font has them)
    assert_eq!(to_typst_math("π² + 1"), "pi ² + 1");
}
