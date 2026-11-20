//! QA item 45: engineering and physics homework lines (statics, circuits, thermo, dynamics) that were blank or wrong.
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn a_power_before_a_divided_amount_is_not_a_fraction() {
    // `s^2/50 m` read the exponent as 2/50 (a whole-number fraction before a unit)
    assert_eq!(show("(20 m/s)^2 / 50 m"), "8 m/s²");
    assert_eq!(show("400 m^2/s^2 / 50 m"), "8 m/s²");
    assert_eq!(show("(12 V)^2 / 8 ohm"), "18 W");
    assert_eq!(show("144 V^2 / 8 Ω"), "18 W");
    // the fraction before a unit still reads as one amount
    assert_eq!(show("3/8 in + 1/8 in"), "1/2 in");
}

#[test]
fn juxtaposed_factors_multiply() {
    assert_eq!(show("1/(2 pi 60 Hz)"), "0.00265258238486 s");
    // spaced, or after a power, a bracket multiplies a unit
    assert_eq!(show("(40 mA)^2 (100 Ω)"), "160 mW");
    // `5 ft 10 kg` stays blank (web parity)
    assert_eq!(show("5 ft 10 kg"), "");
}

#[test]
fn rotational_energy_and_three_part_units() {
    assert_eq!(show("1/2 * 0.5 kg m^2 * (10 rad/s)^2"), "25 J");
    assert_eq!(show("3 J/(kg K)"), "3 J/(kg·K)");
    assert_eq!(show("10 N s/m"), "10 N·s/m");
    // web parity kept: torque times an angle, J/K, hp·hr
    assert_eq!(show("1 kg * m^2 * 10 rad / s^2"), "10 N m rad");
    assert_eq!(show("1.38e-23 J/K"), "1.38e-23 N m / K");
}

#[test]
fn where_definitions_and_unit_names_defined_before() {
    assert_eq!(show("x^2 where x = 3"), "9");
    assert_eq!(show("R*C where R = 10 kΩ, C = 47 µF"), "470 ms");
    assert_eq!(
        show("m v^2/r where m = 1200 kg, v = 20 m/s, r = 50 m"),
        "9.6 kN"
    );
    assert_eq!(show("m*v^2/r; m = 1200 kg; v = 20 m/s; r = 50 m"), "9.6 kN");
    assert_eq!(
        show("c/(2 sqrt(k m)) where c = 10 N s/m, k = 200 N/m, m = 2 kg"),
        "0.25"
    );
    // a definition keeps the units typed (it read 19.685 ft/s, and mm became in⁴)
    assert_eq!(show("x*v; x = 2; v = 3 m/s"), "6 m/s");
    assert!(show("b*h^3/12 where b = 50 mm, h = 100 mm").ends_with("mm^4"));
}
