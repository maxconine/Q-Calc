//! Bugs the sheet critics found in the bar (20 Nov 2026): next to a unit, a bare `e` was read as the electron charge
//! (`20 V * e^(-1)` blank, `100 uF * 9 V * (1 - e^(-2)) to mC` "improper unit conversion"), and `e^(jπ) + 1` showed
//! its round-off imaginary part as `j0`.
use qcalc_core::{EvaluateOptions, evaluate_line};

fn shown(line: &'static str) -> String {
    std::thread::Builder::new()
        .stack_size(32 << 20)
        .spawn(move || evaluate_line(line, &EvaluateOptions::default()).display)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn a_bare_e_beside_units_is_eulers_number() {
    for (line, want) in [
        ("20 kg * (1 - 1/e)", "12.6424111766 kg"),
        ("20 kg * (1 - e^-1)", "12.6424111766 kg"),
        ("2 kg * e", "5.43656365692 kg"),
        ("10 V - 20 V * e^(-1)", "2.64241117657 V"),
        ("e^(-1) * 20 V", "7.35758882343 V"),
        ("100 uF * 9 V * (1 - e^(-2)) to mC", "0.778198245087 mC"),
        ("10 C / e", "3.67879441171 C"),
    ] {
        assert_eq!(shown(line), want, "{line}");
    }
}

#[test]
fn round_off_imaginary_parts_are_not_shown() {
    assert_eq!(shown("e^(jπ) + 1"), "0");
    assert_eq!(shown("e^(iπ) + 1"), "0");
    assert_eq!(shown("e^(jπ/2)"), "j");
    assert_eq!(shown("3 + 4j"), "3 + j4");
}
