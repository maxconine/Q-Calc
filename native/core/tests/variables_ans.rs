//! QA item 7: `ans` written beside a number or a name, and functions defined for one line after `;`.
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use serde_json::Value as Json;

/// Each line committed in turn, as the bar does: the answer slot and the tape's expression.
fn session(lines: &[&str]) -> Vec<(String, String)> {
    let o = qcalc_core::app::settings::default_settings().eval_options();
    let mut tape: Vec<Json> = Vec::new();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let ctx = context_from_history(&tape, HistoryShow::Always, 0.0, AnswerForm::Exact, 6.0);
        let r = LiveRequest {
            text: (*l).into(),
            phrases: true,
            options: ctx.apply(&o),
            chain_answer: ctx.chain_answer.clone(),
            last_expr: ctx.last_expr.clone(),
            ..Default::default()
        };
        let shown = live(&r).display;
        let c = commit(&r, &tape, &format!("r{i}"));
        let expr = c
            .row
            .as_ref()
            .and_then(|r| r["expr"].as_str())
            .unwrap_or("")
            .to_string();
        out.push((shown, expr));
        tape = c.history;
    }
    out
}

#[test]
fn ans_glued_to_a_number_multiplies() {
    let s = session(&["5", "2ans", "3 ans", "-4", "3ans", "x2ans = 1"]);
    // `2ans` was blank; the tape keeps the bracket so `3 ans` doesn't read as the number 310
    assert_eq!(s[1], ("10".into(), "2 (5)".into()));
    assert_eq!(s[2], ("30".into(), "3 (10)".into()));
    assert_eq!(s[4], ("-12".into(), "3 (-4)".into()));
    // a name that runs on into `ans` is its own name
    assert_eq!(s[5].0, "1");
}

#[test]
fn ans_after_a_constant_or_letter_multiplies() {
    let s = session(&["5", "pi ans", "x = 2", "x ans", "sin ans"]);
    // `pi ans` and `x ans` were blank (`ans` after a name read as a call)
    assert_eq!(s[1], ("15.708".into(), "pi*5".into()));
    assert_eq!(s[3], ("4".into(), "x*2".into()));
    assert_eq!(s[4].1, "sin (4)");
}

#[test]
fn a_function_defined_after_a_semicolon_is_for_that_line() {
    let s = session(&[
        "f(2); f(x) = x^3",
        "g(2, 3) + 1; g(a, b) = a*b",
        "f(2) + y; f(x) = x^2, y = 3",
        "x + f(1); f(t) = 2t; x = 4",
        "f(2)",
    ]);
    assert_eq!(s[0].0, "8");
    assert_eq!(s[1].0, "7");
    assert_eq!(s[2].0, "7");
    assert_eq!(s[3].0, "6");
    // nothing was kept: f is still unknown on the next line
    assert_eq!(s[4].0, "");
}
