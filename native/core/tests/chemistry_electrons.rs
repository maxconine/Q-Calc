//! Native-only chemistry (QA item 60): electrons in half-reactions and the kJ/kcal/kPa words `chem R` completes.

use qcalc_core::app::completion::{CompletionNames, completion_for};
use qcalc_core::chem::chem_copy_text;
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    let o: EvaluateOptions = qcalc_core::app::settings::default_settings().eval_options();
    evaluate_line(t, &o).display
}

#[test]
fn electrons_balance_by_charge() {
    assert_eq!(show("Fe^3+ + e^- -> Fe^2+"), "Fe³⁺ + e⁻ → Fe²⁺");
    assert_eq!(show("Fe³⁺ + e⁻ → Fe²⁺"), "Fe³⁺ + e⁻ → Fe²⁺");
    assert_eq!(show("Cu^2+ + e- -> Cu"), "Cu²⁺ + 2 e⁻ → Cu");
    assert_eq!(show("Zn -> Zn^2+ + e^-"), "Zn → Zn²⁺ + 2 e⁻");
    assert_eq!(
        show("MnO4^- + H^+ + e^- -> Mn^2+ + H2O"),
        "MnO₄⁻ + 8 H⁺ + 5 e⁻ → Mn²⁺ + 4 H₂O"
    );
    assert_eq!(
        show("Cr2O7^2- + H^+ + e^{-} -> Cr^3+ + H2O"),
        "Cr₂O₇²⁻ + 14 H⁺ + 6 e⁻ → 2 Cr³⁺ + 7 H₂O"
    );
    // a shown half-reaction copies as text that reads back
    assert_eq!(chem_copy_text("Cu²⁺ + 2 e⁻ → Cu"), "Cu^2+ + 2 e^- -> Cu");
}

#[test]
fn electrons_dont_take_over_math() {
    // `e` with no charge, or `e-x`, is still Euler's number
    assert_eq!(show("e - 1"), show("2.718281828459045 - 1"));
    assert_ne!(show("Fe + e -> Fe"), "Fe + e⁻ → Fe");
    // the same electron twice is a repeated species, which never balances
    assert_eq!(show("Fe^2+ -> Fe^3+ + 2 e^- + e^-"), "");
}

#[test]
fn chem_r_completes_the_native_units() {
    let names = CompletionNames::default();
    assert_eq!(completion_for("chem R kc", &names), "al");
    assert_eq!(completion_for("chem R kp", &names), "a");
    assert_eq!(completion_for("chem R kJ", &names), "");
    assert_eq!(completion_for("chem R at", &names), "m");
}
