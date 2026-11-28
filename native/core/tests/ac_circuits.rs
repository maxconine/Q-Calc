//! AC circuits second pass (QA item 81): a phasor's parts of a real quantity (`abs(-5 V)` was blank), power factor
//! (`pf(Z)`, `power factor of Z`), reactive power in var and apparent power in volt-amperes.
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn homework_lines_already_right() {
    for (t, want) in [
        ("(3+4j) kΩ", "(3 + j4) kΩ"),
        (
            "10∠30° V / (5 - 2j) Ω in polar",
            "1.85695338177∠51.8014094864° A",
        ),
        ("1/(j 2 pi 60 Hz 10 mF)", "-j265.258238486 mΩ"),
        ("j 2 pi 60 Hz 10 mH", "j3.76991118431 Ω"),
        ("120∠0 V / (3+4j) Ω", "(14.4 - j19.2) A"),
        (
            "(3+4j) Ω || (5-2j) Ω",
            "(3.11764705882 + j0.970588235294) Ω",
        ),
        ("10 V / (3+4j) kΩ", "(1.2 - j1.6) mA"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

#[test]
fn parts_of_a_real_quantity() {
    for (t, want) in [
        ("abs(-5 V)", "5 V"),
        ("abs(2 V - 7 V)", "5 V"),
        ("conj(2 A)", "2 A"),
        ("1/2 * 10 V * conj(2 A)", "10 W"),
        ("re(-5 V)", "-5 V"),
        ("angle(-5)", "180 deg"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

#[test]
fn power_factor() {
    for (t, want) in [
        ("pf(3+4j)", "0.6"),
        ("pf((8 - 6j) Ω)", "0.8"),
        ("power factor of (3+4j) Ω", "0.6"),
        ("pf of 10∠30° V * conj(2∠-15° A)", "0.707106781187"),
        ("pf(30 kW + j40 kVAR)", "0.6"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    // still a variable when you name one pf
    assert_eq!(show("pf = cos(30°)"), "0.866025403784");
}

#[test]
fn complex_power_in_volt_amperes() {
    for (t, want) in [
        // var was undefined: mathjs gives it no real size
        ("30 kW + j40 kVAR", "(30 + j40) kVA"),
        ("120∠0 V * conj(10∠-30 A)", "(1.03923048454 + j0.6) kVA"),
        ("120∠0 V * conj(10∠-30 A) in polar", "1.2∠30° kVA"),
        ("(3+4j) Ω * (2 A)^2", "(12 + j16) VA"),
        // the real power stays in watts
        ("re(120∠0 V * conj(10∠-30 A))", "1.03923048454 kW"),
        ("40 kVA", "40 kVA"),
        ("40 VA", "40 VA"),
        ("1500 VA in kVA", "1.5 kVA"),
        ("50 kVA to kW", "50 kW"),
        ("5 MW", "5 MW"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

/// QA item 88: the leftovers of item 81.
#[test]
fn rectangular_impedance_with_a_trailing_unit() {
    for (t, want) in [
        ("100 + j377 Ω", "(100 + j377) Ω"),
        ("100 - j377 Ω", "(100 - j377) Ω"),
        ("100 + 377j Ω", "(100 + j377) Ω"),
        ("100 + j377 kΩ", "(100 + j377) kΩ"),
        ("100 + j377 Ω in kΩ", "(0.1 + j0.377) kΩ"),
        ("100 + j377 V in polar", "390.037177715∠75.1442832297° V"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    // a plain sum of reals with a unit is still a mismatch (native: it says so, and ↵ offers 100 Ω), and sqrt of
    // ohms has no unit to show (as sqrt(4 Ω))
    assert_eq!(show("100 + 377 Ω"), "improper unit conversion");
    assert_eq!(show("sqrt((3+4j) Ω)"), "");
}

#[test]
fn apparent_power_keeps_volt_amperes() {
    for (t, want) in [
        ("5 MVA", "5 MVA"),
        ("2 MVA + 1 MVA", "3 MVA"),
        ("5000 kVA", "5 MVA"),
        ("5e6 VA", "5 MVA"),
        ("0.002 kVA", "2 VA"),
        ("5 MVA in kVA", "5000 kVA"),
        ("5 MVA to kW", "5000 kW"),
        ("5 MW", "5 MW"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}
