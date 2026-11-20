//! A variable named like a unit doesn't take over that unit on later sheet lines (user's call, native only).

use qcalc_core::evaluate::evaluate_sheet;
use qcalc_core::types::EvaluateOptions;

fn sheet(lines: &[&str]) -> Vec<String> {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&lines, &EvaluateOptions::default())
        .into_iter()
        .map(|l| l.display)
        .collect()
}

#[test]
fn mass_named_m_leaves_metres_alone() {
    let out = sheet(&["m = 1200 kg", "v = 20 m/s", "m v^2 / 2"]);
    assert!(!out[1].contains("kg"), "{out:?}");
    assert!(out[2].contains("J") || out[2].contains("240"), "{out:?}");
}

#[test]
fn quantity_variable_before_amount_is_the_unit() {
    let out = sheet(&["m = 1200 kg", "d = 50 m", "m * 2"]);
    assert!(!out[1].contains("kg"), "{out:?}");
    assert!(
        out[2].contains("2400") || out[2].contains("2,400"),
        "{out:?}"
    );
}

#[test]
fn plain_number_variable_still_multiplies() {
    let out = sheet(&["a = 5", "2a", "s = 3", "2 s"]);
    assert_eq!(out[1], "10", "{out:?}");
}

#[test]
fn redefined_name_keeps_the_new_value() {
    let out = sheet(&["m = 1200 kg", "m = 3 m/s", "m"]);
    assert!(out[2].contains("/s") || out[2].contains("ft"), "{out:?}");
}

#[test]
#[ignore]
fn show_examples() {
    for s in [
        &[
            "m = 1200 kg",
            "v = 20 m/s",
            "m v^2 / 2",
            "a = 9.8 m/s^2",
            "m a",
        ][..],
        &["m = 1200 kg", "d = 50 m", "m * 2", "2 m"][..],
    ] {
        eprintln!("{:?}", sheet(s));
    }
}
