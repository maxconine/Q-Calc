//! Native-only chemistry (QA item 13): grams and moles of a formula, more units for `chem R`, and the hint for a
//! reaction that can't balance.

use qcalc_core::app::blank_reason::{EnterFacts, Names, blank_reason, enter_hint};
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    // the app's defaults (6 sig figs)
    let o: EvaluateOptions = qcalc_core::app::settings::default_settings().eval_options();
    evaluate_line(t, &o).display
}

#[test]
fn grams_and_moles_of_a_formula() {
    assert_eq!(show("10 g H2O in mol"), "0.555093 mol");
    assert_eq!(show("10 g of H2O to mol"), "0.555093 mol");
    assert_eq!(show("2 mol NaCl in g"), "116.88 g");
    assert_eq!(show("500 mg CaCO3 to mmol"), "4.9957 mmol");
    assert_eq!(show("0.5 mol of CO2 to kg"), "0.0220045 kg");
    assert_eq!(show("1 kg H2O in moles"), "55.5093 mol");
    // no formula, or not a formula: not this reading
    assert_eq!(show("10 g in mol"), "");
    assert_eq!(show("10 g x in mol"), "");
}

#[test]
fn gas_constant_in_kilojoules_and_kpa() {
    // these were blank
    assert_eq!(show("chem R kJ"), "0.00831446 kJ·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R kcal"), "0.0019872 kcal·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R L kPa"), "8.31446 L·kPa·mol⁻¹·K⁻¹");
    // the web's words are unchanged
    assert_eq!(show("chem R cal"), "1.9872 cal·mol⁻¹·K⁻¹");
}

#[test]
fn a_reaction_that_wont_balance_says_so() {
    let never = |_: &str| false;
    // was a squiggle under the O and "O has no value yet · O = 5 sets it"
    assert_eq!(
        blank_reason("H2 + O2 -> CO2", &never, &Names::default()),
        None
    );
    let hint = |t: &str| {
        enter_hint(&EnterFacts {
            expr: t.into(),
            ..EnterFacts::default()
        })
    };
    assert_eq!(
        hint("H2 + O2 -> CO2"),
        "that reaction can’t balance · each element needs to be on both sides"
    );
    // a line that isn't chemistry keeps its hint
    assert_eq!(
        hint("100 C -> F"),
        "no answer for that · ? shows what you can type"
    );
}
