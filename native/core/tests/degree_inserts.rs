//! An angle answer shows `deg` but a click inserts only its number (the user's call, native only).

use qcalc_core::app::answer::{AnswerForm, row_answers, without_degrees};
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::{AngleMode, EvaluateOptions};
use serde_json::json;

#[test]
fn angle_answers_still_show_deg() {
    let deg = EvaluateOptions {
        angle_mode: Some(AngleMode::Deg),
        ..Default::default()
    };
    assert_eq!(evaluate_line("asin(0.5)", &deg).display, "30 deg");
}

#[test]
fn a_click_drops_deg() {
    assert_eq!(without_degrees("30 deg"), "30");
    assert_eq!(without_degrees("-57.4914 deg"), "-57.4914");
    assert_eq!(without_degrees("1874.27 N m / deg"), "1874.27 N m / deg");
    assert_eq!(without_degrees("32 °F"), "32 °F");
    assert_eq!(without_degrees("5 m"), "5 m");
}

#[test]
fn a_tape_row_inserts_the_number_but_copies_the_unit() {
    let row = json!({"kind": "expression", "expr": "asin(0.5)", "display": "30 deg",
        "value": {"kind": "number", "n": 30, "unit": "deg"}, "quantity": "30 deg"});
    let got = row_answers(&row, AnswerForm::Approx, 6.0, false);
    let a = &got.answers[0];
    assert_eq!(a.insert, "30");
    assert_eq!(a.copy, "30 deg");
}
