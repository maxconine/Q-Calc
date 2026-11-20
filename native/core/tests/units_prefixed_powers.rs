//! QA backlog item 101: a prefix symbol on a squared or cubed metre prefixes the metre. `1 Mm2 to m2` was
//! 0.000001 m² (read as the table's mm², ignoring case), `Gm2` was 10⁹ m² and `km3` 1000 m³ (the prefix scaled
//! the whole square, as the web does).
use qcalc_core::app::live::{LiveRequest, live};

fn d(t: &str) -> String {
    live(&LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    })
    .display
}

#[test]
fn prefixed_squares_and_cubes_of_metres() {
    for (t, want) in [
        ("1 Mm2 to m2", "1e+12 m2"),
        ("1 Mm^2 to m^2", "1e+12 m^2"),
        ("2 Mm² to m²", "2e+12 m^2"),
        ("1 Mm2", "1 Mm²"),
        ("1 Mm2 in km2", "1000000 km2"),
        ("1 Gm2 to m2", "1e+18 m2"),
        ("1 Tm2 to m2", "1e+24 m2"),
        ("1 µm2 to m2", "1e-12 m2"),
        ("1 um2 to m2", "1e-12 m2"),
        ("1 nm2 to m2", "1e-18 m2"),
        ("1 m2 to Mm2", "1e-12 Mm2"),
        ("1 km3 to m3", "1000000000 m3"),
        ("1 km^3 to L", "1e+12 L"),
        ("1 Mm3 to m3", "1e+18 m3"),
        ("1 Gm^3 to km^3", "1e+18 km^3"),
        ("1 µm3 to m3", "1e-18 m3"),
        ("1 Mm2 + 1 km2", "1.000001 Mm²"),
        // the table's own squares and the lowercase prefixes keep their reading
        ("1 mm3 to m3", "1e-9 m³"),
        ("1 mm2 to m2", "0.000001 m²"),
        ("1 m2 to mm2", "1000000 mm²"),
        ("3 km2 to m2", "3000000 m²"),
        ("1 hm2 to ha", "1 ha"),
        ("1 dm3 to L", "1 L"),
        ("1 cm3 to L", "0.001 L"),
        ("5 mPa to Pa", "0.005 Pa"),
        ("1 MPa to Pa", "1000000 Pa"),
        ("10 kN / 20 mm^2", "500 MPa"),
        ("1 Mm to m", "1000000 m"),
    ] {
        assert_eq!(d(t), want, "{t}");
    }
}
