//! QA item 70: prefixed tesla and siemens symbols (`mT`, `µT`, `nT`, `mS`, `µS`, `kS`) read natively (bare `T` and
//! `S` stay the ton and the second the web's goldens pin), a field or flux left in base units reads in T or Wb,
//! and `ans + 1` after a moles answer stays blank as it does after any unit answer.
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

fn sheet(lines: &[&str]) -> Vec<String> {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&lines, &EvaluateOptions::default())
        .into_iter()
        .map(|r| r.display)
        .collect()
}

#[test]
fn prefixed_tesla() {
    // were 5 kg, 5 g (milli- and micro-tons)
    assert_eq!(show("5 mT"), "5 mT");
    assert_eq!(show("40 µT"), "40 µT");
    assert_eq!(show("40 uT"), "40 µT");
    assert_eq!(show("2 nT"), "2 nT");
    assert_eq!(show("5 mT + 1 mT"), "6 mT");
    assert_eq!(show("5 mT in T"), "0.005 T");
    assert_eq!(show("5 mT in gauss"), "50 gauss");
    assert_eq!(show("5 mT * 2 m^2"), "0.01 Wb");
    assert_eq!(show("50 µT * 1 cm^2"), "5e-9 Wb");
    // a field chained through ans was `0.01 N / m A`
    assert_eq!(sheet(&["5 mT", "ans * 2"])[1], "0.01 T");
    assert_eq!(sheet(&["5 mT", "ans * 2 m^2"])[1], "0.01 Wb");
    // bare T stays the ton, kT the knot (web)
    assert_eq!(show("2 T"), "2.20462262185 ton");
    assert_eq!(show("1 kT"), "1.852 km/h");
    assert_eq!(show("1 mt"), "1 kg");
}

#[test]
fn prefixed_siemens() {
    // were 2 ms, 3 ks, 2 µs
    assert_eq!(show("2 mS"), "2 mS");
    assert_eq!(show("3 kS"), "3 kS");
    assert_eq!(show("2 µS"), "2 µS");
    assert_eq!(show("2 mS + 3 mS"), "5 mS");
    assert_eq!(show("2 mS * 3 V"), "6 mA");
    assert_eq!(show("2 mS * 5 kohm"), "10");
    assert_eq!(show("3 mS in S"), "0.003 S");
    assert_eq!(show("3 mS in µS"), "3000 µS");
    assert_eq!(show("1 / (2 kohm) in mS"), "0.5 mS");
    assert_eq!(sheet(&["x = 2 mS", "x * 3 V"])[1], "6 mA");
    // S and ms stay seconds; a line that only reads as time keeps the web's ms
    assert_eq!(show("1 S"), "1 s");
    assert_eq!(show("1 S in ms"), "1000 ms");
    assert_eq!(show("10 ms"), "10 ms");
    assert_eq!(show("2 mS + 3 s"), "3002 ms");
    // volt-seconds are webers (was 0.006 N m / A)
    assert_eq!(show("2 ms * 3 V"), "0.006 Wb");
}

#[test]
fn ans_plus_a_number_after_moles() {
    // blank, as after `5 kg` (a number can't be added to an amount); with a unit it adds
    assert_eq!(sheet(&["5 kg", "ans + 1"])[1], "");
    assert_eq!(sheet(&["0.5 mol/L * 2 L", "ans + 1"]), ["1 mol", ""]);
    assert_eq!(sheet(&["x = 1 mol/L * 2 L", "x + 1"]), ["2 mol", ""]);
    assert_eq!(sheet(&["0.5 mol/L * 2 L", "ans + 1 mol"])[1], "2 mol");
    assert_eq!(sheet(&["0.5 mol/L * 2 L", "ans - 0.5 mol"])[1], "0.5 mol");
    assert_eq!(
        sheet(&["8.314 J/(mol K) * 300 K", "ans + 1 kJ/mol"])[1],
        "3.4942 kJ/mol"
    );
    assert_eq!(sheet(&["0.5 mol/L * 2 L", "ans * 2"])[1], "2 mol");
}
