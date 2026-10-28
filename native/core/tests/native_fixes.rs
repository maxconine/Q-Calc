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
fn and_is_bitwise_and() {
    // `and` reads as `&`
    for (t, want) in [("12 and 10", "8"), ("12 and 3", "0"), ("1 and 1", "1"), ("1 and 0", "0"), ("6 AND 3", "2")] {
        assert_eq!(show(t), want, "{t}");
        assert_eq!(show(t), show(&t.to_lowercase().replace(" and ", " & ")), "{t}");
    }
    assert_eq!(show("(2>1) and (3>4)"), "0");
    assert_eq!(show("(2>1) and (4>3)"), "1");
}

#[test]
fn logic_or() {
    // `or` reads as `|`: on 0 and 1 (or true/false comparisons) it's logical or, on whole numbers bitwise
    for (t, want) in [
        ("1 or 0", "1"),
        ("0 or 1", "1"),
        ("1 or 1", "1"),
        ("0 or 0", "0"),
        ("12 or 3", "15"),
        ("8 OR 1", "9"),
        ("1 or 0 or 0", "1"),
        ("0 or 0 or 0", "0"),
        ("(2>1) or (3>4)", "1"),
        ("(1>2) or (3>4)", "0"),
        ("2>1 or 3>4", "1"),
        ("1>2 or 3>4", "0"),
        ("(1 or 0) and 1", "1"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    assert_eq!(show("12 or 3"), show("12 | 3"));
}

#[test]
fn and_or_in_phrases_stay_words() {
    // the words in a phrase aren't operators: `between 1 and 2` stays blank (as on the page), not `1 & 2`
    assert_eq!(show("between 1 and 2"), "");
}

#[test]
fn and_or_are_wedge_and_vee_in_the_typst_preview() {
    use qcalc_core::app::typst_math::to_typst_math;
    // ∧ for & and `and`, ∨ for `or` (a bare & was Typst's alignment point and drew nothing)
    for (line, want) in [
        ("sin(22&3)", "sin(22 and 3)"),
        ("(22&3*21)22/23", "(22 and 3 dot 21) 22 / 23"),
        ("12 and 3", "12 and 3"),
        ("1 or 0", "1 or 0"),
        ("(2>1) or (3>4)", "(2 > 1) or (3 > 4)"),
    ] {
        assert_eq!(to_typst_math(line), want, "{line}");
    }
}

#[test]
fn spaced_and_starred_factors_differentiate_as_powers() {
    // x written out six times, spaced or starred, is x^6: same answer, same form
    let want = show("d/dx 4x^3 + 12(x^6)");
    assert_eq!(want, "12x² + 72x⁵");
    for t in ["d/dx 4x^3 + 12(x*x x x x x)", "d/dx 4x^3 + 12(x x x x x x)", "d/dx 4x^3 + 12 x x x x x x", "d/dx 4x^3 + 12x*x*x*x*x*x"] {
        assert_eq!(show(t), want, "{t}");
    }
    assert_eq!(show("d/dx 4 x^3 + 12 x x"), show("d/dx 4x^3 + 12x^2"));
    assert_eq!(show("d/dx x x^2"), "3x²");
    assert_eq!(show("d/dx x*y x"), "2xy");
    assert_eq!(show("int 0..1 x*x x dx"), show("int 0..1 x^3 dx"));
}

#[test]
fn spaced_numbers_multiply_in_calculus() {
    // `2 3 x` is 6x in a derivative as on a plain line (it had come back blank)
    assert_eq!(show("2 3"), "6");
    for t in ["d/dx 2 3 x", "d/dx 2 3*x", "d/dx 2*3 x", "d/dx 6x"] {
        assert_eq!(show(t), "6", "{t}");
    }
    assert_eq!(show("d/dx 3.5 2 x"), "7");
    assert_eq!(show("int 0..1 2 3 x dx"), show("int 0..1 6x dx"));
}

#[test]
fn exponent_form_of_an_answer() {
    use qcalc_core::format::exponent_form;
    for (shown, want) in [
        ("3,000", "3 × 10³"),
        ("3000", "3 × 10³"),
        ("15,372", "1.5372 × 10⁴"),
        ("0.00052", "5.2 × 10⁻⁴"),
        ("-42", "−4.2 × 10¹"),
        ("−42", "−4.2 × 10¹"),
        ("7", "7 × 10⁰"),
        ("3,000 m", "3 × 10³ m"),
        ("1.5e12", "1.5 × 10¹²"),
        ("2.5 × 10⁻³", "2.5 × 10⁻³"),
        ("3 000", "3 × 10³"),
    ] {
        assert_eq!(exponent_form(shown, 6).as_deref(), Some(want), "{shown}");
    }
    for shown in ["1/3", "7:55 pm", "", "abc", "2x"] {
        assert_eq!(exponent_form(shown, 6), None, "{shown}");
    }
}

#[test]
fn lcd_is_the_least_common_denominator() {
    assert_eq!(show("lcd(4,6)"), "12");
    assert_eq!(show("LCD(3, 4, 6)"), "12");
    assert_eq!(show("lcd(4,6)"), show("lcm(4,6)"));
    let names = qcalc_core::app::word_names::word_names("lcd(4,6) + gcf(12,18)", &[], &[], false);
    let said: Vec<&str> = names.iter().map(|w| w.name.as_str()).collect();
    assert!(said.contains(&"least common denominator"), "{said:?}");
    assert!(said.contains(&"greatest common factor"), "{said:?}");
}
