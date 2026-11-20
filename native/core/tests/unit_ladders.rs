//! ⌥↑/⌥↓ ladders native adds to the web's SI prefix step (QA item 96): squares and cubes of metres step one
//! length prefix (×10⁶ / ×10⁹) and clock time steps s ↔ min ↔ hr ↔ day.

use qcalc_core::types::Value;
use qcalc_core::units::{
    can_step_prefix, in_ladder_unit, step_ladder, step_prefix, try_convert_opt,
};

fn q(text: &str) -> Value {
    try_convert_opt(text, &Default::default(), &Default::default(), false, None).expect(text)
}

fn shown(v: Option<Value>) -> String {
    v.map_or("None".into(), |v| {
        format!(
            "{} {} [{}]",
            qcalc_core::jsnum::to_string(v.n),
            v.unit.unwrap_or_default(),
            v.unit_id.unwrap_or_default()
        )
    })
}

fn walk(text: &str, dirs: &[i32]) -> Vec<String> {
    let mut v = q(text);
    dirs.iter()
        .map(|&d| {
            let next = step_ladder(&v, d);
            let s = shown(next.clone());
            if let Some(n) = next {
                v = n;
            }
            s
        })
        .collect()
}

#[test]
fn squared_metres_step_by_a_million() {
    assert_eq!(
        walk("15 m^2 to m^2", &[-1, -1, 1, 1, 1]),
        [
            "15000000 mm² [mm2]",
            "15000000000000 μm² [micro_m^2]",
            "15000000 mm² [mm2]",
            "15 m² [m2]",
            "0.000015 km² [km2]",
        ]
    );
    assert_eq!(
        shown(step_ladder(&q("15 cm^2 to cm^2"), 1)),
        "0.0015 m² [m2]"
    );
    assert_eq!(
        shown(step_ladder(&q("15 cm^2 to cm^2"), -1)),
        "1500 mm² [mm2]"
    );
    assert_eq!(shown(step_ladder(&q("2 ha to ha"), 1)), "0.02 km² [km2]");
    assert_eq!(shown(step_ladder(&q("2 ha to ha"), -1)), "20000 m² [m2]");
    // imperial areas and barns have no ladder
    assert!(step_ladder(&q("3 ft^2 to ft^2"), 1).is_none());
    assert!(!can_step_prefix(Some(&q("2 acres to acres"))));
    assert!(can_step_prefix(Some(&q("15 m^2 to m^2"))));
}

#[test]
fn cubed_metres_step_by_a_billion() {
    assert_eq!(
        walk("2 m^3 to m^3", &[1, -1, -1, -1]),
        [
            "2e-9 km³ [kilo_m^3]",
            "2 m³ [m3]",
            "2000000000 mm³ [mm3]",
            "2000000000000000000 μm³ [micro_m^3]",
        ]
    );
    assert_eq!(
        shown(step_ladder(&q("5 cm^3 to cm^3"), 1)),
        "0.000005 m³ [m3]"
    );
    // litres keep the web's prefix ladder
    assert_eq!(shown(step_ladder(&q("2 L to L"), -1)), "2000 mL [ml]");
}

#[test]
fn time_steps_seconds_minutes_hours_days() {
    assert_eq!(
        walk("1.5 hr to hr", &[-1, -1, -1, 1, 1, 1, 1]),
        [
            "90 min [min]",
            "5400 s [s]",
            "5400000 ms [milli_s]",
            "5400 s [s]",
            "90 min [min]",
            "1.5 hr [hr]",
            "0.0625 d [day]",
        ]
    );
    assert!(step_ladder(&q("2 days to days"), 1).is_none());
    assert_eq!(shown(step_ladder(&q("2 days to days"), -1)), "48 hr [hr]");
    assert!(can_step_prefix(Some(&q("1 hr to hr"))));
    // the web's own step is unchanged (goldens): no hour ladder, a second steps up to ks
    assert!(step_prefix(&q("1 hr to hr"), 1).is_none());
}

#[test]
fn a_chosen_ladder_unit_is_kept_while_typing() {
    assert_eq!(
        shown(in_ladder_unit(&q("2 hr to hr"), "min")),
        "120 min [min]"
    );
    assert_eq!(
        shown(in_ladder_unit(&q("2 hr to hr"), "milli_s")),
        "7200000 ms [milli_s]"
    );
    assert_eq!(
        shown(in_ladder_unit(&q("3 m^2 to m^2"), "mm2")),
        "3000000 mm² [mm2]"
    );
    assert_eq!(
        shown(in_ladder_unit(&q("3 m^3 to m^3"), "kilo_m^3")),
        "3e-9 km³ [kilo_m^3]"
    );
    assert!(in_ladder_unit(&q("3 m^2 to m^2"), "kilo_m^3").is_none());
    assert!(in_ladder_unit(&q("3 m^2 to m^2"), "min").is_none());
}
