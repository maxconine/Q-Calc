//! QA item 59: a power of one typed unit no name fits reads in that unit, not in base units (native only; the
//! web's base-unit answers its goldens pin, like `kg m / s` and `rad / s`, are unchanged).
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn powers_of_a_typed_unit() {
    assert_eq!(show("(12 V)^2"), "144 V²");
    assert_eq!(show("12 V * 12 V"), "144 V²");
    assert_eq!(show("(4 Ω)^2"), "16 Ω²");
    assert_eq!(show("(2 A)^2"), "4 A²");
    assert_eq!(show("(3 kV)^2"), "9 kV²");
    assert_eq!(show("(3 V)^3"), "27 V³");
    assert_eq!(show("(3 m/s)^2"), "9 m²/s²");
    assert_eq!(show("(2 ft/s)^2"), "4 ft²/s²");
}

#[test]
fn named_units_and_web_answers_unchanged() {
    assert_eq!(show("(2 A)^2 * 3 Ω"), "12 W");
    assert_eq!(show("(3 m/s)^2 * 2 kg"), "18 J");
    assert_eq!(show("1 kg * 10 m/s"), "10 kg m / s");
    assert_eq!(show("1.38e-23 J/K"), "1.38e-23 N m / K");
    assert_eq!(show("2 deg / 1 s"), "0.0349065850399 rad / s");
}

#[test]
fn the_answer_reads_back() {
    let o = EvaluateOptions::default();
    let lines = |a: &[&str]| -> Vec<String> {
        evaluate_sheet(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &o)
            .into_iter()
            .map(|r| r.display)
            .collect()
    };
    assert_eq!(lines(&["(12 V)^2", "ans / 4 V"]), ["144 V²", "36 V"]);
    assert_eq!(lines(&["(4 Ω)^2", "sqrt(ans)"]), ["16 Ω²", "4 Ω"]);
    assert_eq!(lines(&["(3 m/s)^2", "ans * 2 kg"]), ["9 m²/s²", "18 J"]);
}
