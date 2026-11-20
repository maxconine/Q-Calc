//! Native only: converting to feet and inches (`1.8 m to ft in`), and the inches of that target coloured as a unit.
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn converts_to_feet_and_inches() {
    for (t, want) in [
        ("1.8 m to ft in", "5 ft 10.8661417323 in"),
        ("1.8 m in ft and in", "5 ft 10.8661417323 in"),
        ("2 m in feet and inches", "6 ft 6.7401574803 in"),
        ("180 cm → ft in", "5 ft 10.8661417323 in"),
        ("70 in to '\"", "5 ft 10 in"),
        ("5.5 ft in ft in", "5 ft 6 in"),
        ("2.3 m to ft-in", "7 ft 6.5511811024 in"),
        ("-1.8 m to ft in", "-5 ft 10.8661417323 in"),
        ("50 3/8\" to ft in", "4 ft 2 3/8 in"),
        ("1 mile to ft in", "5280 ft"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    // not a length, or a ±: blank as before
    assert_eq!(show("5 kg to ft in"), "");
    // the plain conversions are unchanged
    assert_eq!(show("1.8 m to in"), "70.8661417323 in");
    assert_eq!(show("1.8 m to ft"), "5.90551181102 ft");
}

#[test]
fn feet_and_inches_answer_steps_with_option_arrows() {
    let r = evaluate_line("1.8 m to ft in", &EvaluateOptions::default());
    let v = r.value.expect("a value");
    let (_, up) = qcalc_core::feet_inches::step_feet_inches(&v, &r.display, 1, 12.0).unwrap();
    assert_eq!(up, "5.90551181102 ft");
    let (_, down) = qcalc_core::feet_inches::step_feet_inches(&v, &r.display, -1, 12.0).unwrap();
    assert_eq!(down, "70.8661417323 in");
}

#[test]
fn inches_of_the_target_are_coloured() {
    let words = |t: &str| -> Vec<String> {
        let u16: Vec<u16> = t.encode_utf16().collect();
        qcalc_core::app::unit_spans::unit_spans(t, &[], &[])
            .into_iter()
            .map(|s| String::from_utf16_lossy(&u16[s.start..s.end]))
            .collect()
    };
    assert_eq!(words("1.8 m to ft in"), ["m", "ft", "in"]);
    assert_eq!(words("2 m in feet and inches"), ["m", "feet", "inches"]);
    // `in` as the conversion word still isn't a unit
    assert_eq!(words("5 m in mm"), ["m", "mm"]);
    assert_eq!(words("5ft 6in"), ["ft", "in"]);
}
