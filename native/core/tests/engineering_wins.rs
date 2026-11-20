//! QA item 56: small engineering wins, native only, tried after the web paths.
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn deg_and_rad_convert_angles() {
    assert_eq!(show("deg(pi)"), "180");
    assert_eq!(show("deg(pi/2)"), "90");
    assert_eq!(show("rad(180)"), "3.14159265359");
    assert_eq!(show("rad(90)"), "1.57079632679");
    // the bare units are unchanged
    assert_eq!(show("deg"), "1 deg");
    assert_eq!(show("rad"), "1 rad");
}

#[test]
fn mol_beside_other_units() {
    assert_eq!(show("8.314 J/(mol K) * 300 K"), "2.4942 kJ/mol");
    assert_eq!(show("8.314 J/(mol K)"), "8.314 J/(mol·K)");
    assert_eq!(show("8.314 J/(mol*K) * 300 K / 101325 Pa"), "24.6158401184 L/mol");
    assert_eq!(show("1 mol / 22.4 L"), "0.0446428571429 mol/L");
    assert_eq!(show("0.5 mol/L * 2 L"), "1 mol");
    assert_eq!(show("2 kmol * 18 g/mol"), "36000 g");
    // mol alone stays blank, as on the web (its goldens), and chemistry lines keep their own reader
    assert_eq!(show("1 mol"), "");
    assert_eq!(show("1 mol + 1 kg"), "");
    assert_eq!(show("10 g in mol"), "");
    assert_eq!(show("2 mol NaCl in g"), "116.88 g");
    assert_eq!(show("10 g H2O in mol"), "0.555092978074 mol");
}

#[test]
fn parallel_resistors() {
    assert_eq!(show("10 Ω || 20 Ω"), "6.66666666667 Ω");
    assert_eq!(show("10 ohm || 10 ohm || 10 ohm"), "3.33333333333 Ω");
    assert_eq!(show("10 || 20"), "6.66666666667");
    assert_eq!(show("5 Ω + (10 Ω || 10 Ω)"), "10 Ω");
    assert_eq!(show("10 ||"), "");
}

#[test]
fn circuit_quantities_side_by_side_multiply() {
    assert_eq!(show("60 Hz 100 µF"), "0.006 1/Ω");
    assert_eq!(show("1/(2 pi 60 Hz 100 µF)"), "26.5258238486 Ω");
    assert_eq!(show("10 kΩ 1 µF"), "10 ms");
    assert_eq!(show("12 V 2 A"), "24 W");
    // other units side by side are left as they were (web parity)
    assert_eq!(show("5 ft 10 kg"), "");
    assert_eq!(show("1 h 30 min"), "1.5 hr");
}

