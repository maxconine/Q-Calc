//! QA backlog item 104: a typed target unit is echoed as typed by the expression path (`1 Gm2 to m2` →
//! `1e+18 m2`, `2 ft * 3 ft to m2` → `… m2`, web parity and the goldens keep it); the bar and tape now show the
//! powers as superscripts (`m²`), as the table's own symbols are. Copy and insert keep the typed text.
use qcalc_core::app::live::{LiveRequest, live};
use qcalc_core::units::unit_powers_shown;

fn d(t: &str) -> String {
    live(&LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    })
    .display
}

#[test]
fn typed_target_powers_shown_as_superscripts() {
    for (t, copied, shown) in [
        ("1 Gm2 to m2", "1e+18 m2", "1e+18 m²"),
        ("1 Mm2 to m2", "1e+12 m2", "1e+12 m²"),
        ("2 ft * 3 ft to m2", "0.55741824 m2", "0.55741824 m²"),
        ("2 ft * 3 ft to m^2", "0.55741824 m^2", "0.55741824 m²"),
        ("1 Gm^3 to km^3", "1e+18 km^3", "1e+18 km³"),
        ("1 m2 to Mm2", "1e-12 Mm2", "1e-12 Mm²"),
        (
            "100 J to kg * m^2/s^2",
            "100 kg * m^2/s^2",
            "100 kg * m²/s²",
        ),
        ("1 Hz to s^-1", "1 s^-1", "1 s⁻¹"),
        // already the table's symbols
        ("1 m2 to ft2", "10.7639104167 ft²", "10.7639104167 ft²"),
        (
            "10 g H2O in mol",
            "0.555092978074 mol",
            "0.555092978074 mol",
        ),
    ] {
        assert_eq!(d(t), copied, "{t}");
        assert_eq!(unit_powers_shown(copied), shown, "{t}");
    }
}

#[test]
fn only_units_in_a_unit_tail_change() {
    for s in [
        "2 mol N2",
        "56 g H2O",
        "3 x^2",
        "2 x^2 + 1",
        "10 m2 = 5",
        "x = 3 m^2",
        "3 L2",
        "1234",
        "f(x) = x^2",
        "−e^2",
    ] {
        assert_eq!(unit_powers_shown(s), s, "{s}");
    }
    for (s, want) in [
        ("1,000 ft3", "1,000 ft³"),
        ("−5 km2", "−5 km²"),
        ("3.0 ± 0.1 m^2", "3.0 ± 0.1 m²"),
        ("6.67e-11 m^3 / kg s^2", "6.67e-11 m³ / kg s²"),
        ("1 meter^3", "1 meter³"),
        ("5 in2", "5 in²"),
    ] {
        assert_eq!(unit_powers_shown(s), want, "{s}");
    }
}
