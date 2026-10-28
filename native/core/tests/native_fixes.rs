//! Answers the native engine gives where the TS one doesn't (left blank or "undefined" there).
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn euler_with_a_typed_pi() {
    for t in ["e^(iπ)+1", "e^(i π)+1", "exp(i pi)+1", "exp(iπ)+1"] {
        assert_eq!(show(t), "0", "{t}");
    }
    assert_eq!(show("e^(2iπ)"), "1");
    assert_eq!(show("2iπ"), "6.28318530718i");
    assert_eq!(show("τi"), "6.28318530718i");
    assert_eq!(show("e π"), show("e*pi"));
}

#[test]
fn exp_stays_real_for_reals() {
    assert_eq!(show("exp(2)"), "7.38905609893");
}

#[test]
fn ampersand_is_drawn_in_the_typst_preview() {
    // a bare & is Typst's alignment point and draws nothing; escaped, it shows
    for line in ["sin(22&3)", "(22&3*21)22/23"] {
        let typst = qcalc_core::app::typst_math::to_typst_math(line);
        assert!(typst.contains("\\&"), "{line} → {typst}");
        assert!(!typst.replace("\\&", "").contains('&'), "{line} → {typst}");
    }
}
