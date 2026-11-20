//! QA item 22: what the Mac's copies put on the clipboard. ⌘⇧C copied `text = shown` as typed (a chain as `+ 2 =
//! 7`, the grey parens left off, a dual answer's exact side dropped, a solve as `x^2 = 9 = -3, 3`) and, with a tape
//! row picked, the live line instead of the row; a reaction's answer copied with its subscripts, and "improper unit
//! conversion" copied as if it were an answer.
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::touches::{answer_copy, live_line_copy, row_line_copy};
use qcalc_core::golden::{Outcome, dispatch};
use serde_json::json;

#[test]
fn live_line_copies_as_the_tape_reads() {
    let f = AnswerForm::Exact;
    assert_eq!(
        live_line_copy("2+3", false, None, "5", None, None, f),
        "2+3 = 5"
    );
    // the grey parens it was read with
    assert_eq!(
        live_line_copy("(1+2", false, None, "3", None, None, f),
        "(1+2) = 3"
    );
    // a chain: the answer it continues written out
    assert_eq!(
        live_line_copy("+ 2", true, Some("5"), "7", None, None, f),
        "5 + 2 = 7"
    );
    assert_eq!(
        live_line_copy("* 2", true, Some("-3"), "-6", None, None, f),
        "(-3) * 2 = -6"
    );
    // a typed ans as its number
    assert_eq!(
        live_line_copy("ans^2", false, Some("4"), "16", None, None, f),
        "4^2 = 16"
    );
    // the exact side and the decimal
    assert_eq!(
        live_line_copy("sqrt(8)", false, None, "2.82843", Some("2√2"), None, f),
        "sqrt(8) = 2√2 ≈ 2.82843"
    );
    // a solve names its variable once
    assert_eq!(
        live_line_copy(
            "x^2 = 9",
            false,
            None,
            "-3, 3",
            None,
            Some(("x", "roots")),
            f
        ),
        "x^2 = 9, x = -3, 3"
    );
    // nothing to copy
    assert_eq!(live_line_copy("2+", false, None, "", None, None, f), "");
    assert_eq!(
        live_line_copy("graph x^2", false, None, "x^2", None, None, f),
        ""
    );
    assert_eq!(
        live_line_copy(
            "5 m + 2 s",
            false,
            None,
            "improper unit conversion",
            None,
            None,
            f
        ),
        ""
    );
}

#[test]
fn reactions_copy_in_ascii() {
    assert_eq!(answer_copy("2H₂ + O₂ → 2H₂O", false), "2H2 + O2 -> 2H2O");
    assert_eq!(answer_copy("15 m²", true), "15");
    assert_eq!(answer_copy("15 m²", false), "15 m²");
    assert_eq!(answer_copy("improper unit conversion", true), "");
    assert_eq!(
        live_line_copy(
            "H2 + O2 -> H2O",
            false,
            None,
            "2H₂ + O₂ → 2H₂O",
            None,
            None,
            AnswerForm::Exact
        ),
        "2H2 + O2 -> 2H2O"
    );
}

#[test]
fn a_picked_row_copies_its_own_line() {
    let f = AnswerForm::Exact;
    assert_eq!(
        row_line_copy(
            &json!({"kind": "calc", "expr": "2^10", "display": "1,024"}),
            f
        ),
        "2^10 = 1,024"
    );
    assert_eq!(
        row_line_copy(
            &json!({"expr": "sqrt(8)", "display": "2.82843", "exact": "2√2"}),
            f
        ),
        "sqrt(8) = 2√2 ≈ 2.82843"
    );
    assert_eq!(
        row_line_copy(
            &json!({"expr": "x^2 = 9", "display": "-3, 3", "solve": {"variable": "x", "outcome": "roots", "roots": [-3, 3]}}),
            f
        ),
        "x^2 = 9, x = -3, 3"
    );
    assert_eq!(
        row_line_copy(
            &json!({"kind": "definition", "expr": "x = 3", "display": "3"}),
            f
        ),
        "x = 3"
    );
    assert_eq!(
        row_line_copy(
            &json!({"kind": "note", "expr": "call Sam", "display": ""}),
            f
        ),
        "call Sam"
    );
}

#[test]
fn the_mac_reaches_them_by_name() {
    let call = |name: &str, args: serde_json::Value| match dispatch(
        "lib/touches",
        name,
        args.as_array().unwrap(),
    ) {
        Outcome::Answer(v) => v,
        _ => panic!("{name} not dispatched"),
    };
    assert_eq!(call("answerCopy", json!(["12 kg", true])), json!("12"));
    assert_eq!(
        call(
            "rowLineCopy",
            json!([{"expr": "1+1", "display": "2"}, "exact"])
        ),
        json!("1+1 = 2")
    );
    assert_eq!(
        call(
            "liveLineCopy",
            json!(["+ 2", true, "5", {"display": "7", "exact": null}, "exact"])
        ),
        json!("5 + 2 = 7")
    );
}
