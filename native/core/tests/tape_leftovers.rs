//! QA item 98: tape answer leftovers from item 95.
use qcalc_core::app::answer::{AnswerForm, row_answers, unitless_answer};
use qcalc_core::app::live::{LiveRequest, commit};
use qcalc_core::app::touches::answer_copy;

fn req(text: &str) -> LiveRequest {
    LiveRequest {
        text: text.into(),
        phrases: true,
        options: qcalc_core::app::settings::default_settings().eval_options(),
        ..Default::default()
    }
}

#[test]
fn an_exact_answer_with_a_unit_copies_without_it() {
    // `√(2) m` kept its unit with "Copy answers without units" on (the pattern wanted a digit last)
    assert_eq!(unitless_answer("√(2) m"), "√(2)");
    assert_eq!(unitless_answer("2sqrt(2) m/s"), "2sqrt(2)");
    assert_eq!(unitless_answer("π/4 rad"), "π/4");
    assert_eq!(unitless_answer("x = 3π m, y = √(3)/2 m"), "x = 3π, y = √(3)/2");
    assert_eq!(answer_copy("√(2) m", true), "√(2)");
    assert_eq!(answer_copy("√(2) m", false), "√(2) m");
    // what the page already stripped, and what it keeps, are unchanged
    assert_eq!(unitless_answer("√2 m"), "√2");
    assert_eq!(unitless_answer("15 m²"), "15");
    assert_eq!(unitless_answer("5 ft 3 in"), "5 ft 3 in");
    assert_eq!(unitless_answer("$10.00"), "$10.00");

    let row = commit(&req("sqrt(8) m"), &[], "a").row.unwrap();
    let exact = &row_answers(&row, AnswerForm::Exact, 6.0, true).answers[0];
    assert_eq!(exact.insert, "2sqrt(2) m");
    assert_eq!(exact.bare, "2sqrt(2)", "⌥-click / ⌃↵ on the exact half leaves the unit off");
    assert_eq!(exact.copy, "2sqrt(2)");
    let kept = &row_answers(&row, AnswerForm::Exact, 6.0, false).answers[0];
    assert_eq!(kept.copy, "2sqrt(2) m");
}

#[test]
fn a_roots_row_has_each_root_to_insert() {
    // the tape draws each root its own button (the page's RootPicks); each inserts its own root
    let row = commit(&req("x^2 = 2"), &[], "a").row.unwrap();
    let a = row_answers(&row, AnswerForm::Exact, 6.0, false);
    let ins: Vec<&str> = a.answers.iter().map(|x| x.insert.as_str()).collect();
    assert_eq!(ins, ["-1.4142135623731", "1.4142135623731"]);
    let shown: Vec<&str> = a.answers.iter().map(|x| x.copy.as_str()).collect();
    assert_eq!(shown, ["-1.41421", "1.41421"]);
}
