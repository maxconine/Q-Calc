//! QA item 34: definitions written first on a line (`x = 3; x^2`, `g(x)=x^2; g(4)`), a function that calls itself
//! refused, and `5 m` answering in feet with default settings (the web's own default, kept).
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use serde_json::Value as Json;

/// Each line committed in turn: the live answer, the saved row (if any) and ↵'s hint.
fn session(lines: &[&str]) -> Vec<(String, Option<Json>, Option<String>)> {
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
        out.push((shown, c.row.clone(), c.hint.clone()));
        tape = c.history;
    }
    out
}

fn answers(lines: &[&str]) -> Vec<String> {
    session(lines).into_iter().map(|s| s.0).collect()
}

#[test]
fn definitions_before_the_expression_apply_to_the_line() {
    // all three were blank (`g(x)=x^2; g(4)` was saved as a function whose body was `x^2; g(4)`)
    assert_eq!(answers(&["x = 3; x^2"]), ["9"]);
    assert_eq!(answers(&["x = 3, y = 4; x*y"]), ["12"]);
    assert_eq!(answers(&["x = 3; y = 2x; x*y"]), ["18"]);
    let s = session(&["g(x)=x^2; g(4)"]);
    assert_eq!(s[0].0, "16");
    assert_eq!(s[0].1.as_ref().unwrap()["kind"], Json::Null);
    assert_eq!(answers(&["f(x) = x^2; f(3)"]), ["9"]);
}

#[test]
fn definitions_first_are_not_kept() {
    assert_eq!(answers(&["x = 3; x^2", "x"]), ["9", ""]);
    assert_eq!(answers(&["g(x)=x^2; g(4)", "g(2)"]), ["16", ""]);
    // the expression itself still sets its name, and definitions after still work
    assert_eq!(answers(&["x = y^2; y = 3", "x"]), ["9", "9"]);
    assert_eq!(answers(&["x^2; x = 3"]), ["9"]);
    assert_eq!(answers(&["f(x) = x^2", "f(3)"]), ["f(x) = x^2", "9"]);
}

#[test]
fn a_function_that_calls_itself_is_refused() {
    // it was saved, and every f(…) after it answered `undefined`
    for line in ["f(x) = f(x) + 1", "h(a, b) = 2 h(a,b)"] {
        let s = session(&[line, "f(2)"]);
        assert!(s[0].1.is_none(), "saved {:?}", s[0].1);
        assert!(
            s[0].2.as_deref().unwrap_or("").contains("itself"),
            "{:?}",
            s[0].2
        );
        assert_eq!(s[1].0, "");
    }
    // recursion that moves on is kept (web parity), as is a call of another function or a longer name
    let s = session(&["f(x) = x <= 1 ? 1 : x * f(x-1)", "f(5)"]);
    assert_eq!(s[0].1.as_ref().unwrap()["kind"], "function");
    assert_eq!(answers(&["g(x) = 2x", "f(x) = g(x) + 1", "f(2)"])[2], "5");
    assert_eq!(
        answers(&["f(x) = ff(x) + x", "f(1)"])[0],
        "f(x) = ff(x) + x"
    );
}

#[test]
fn a_bare_length_answers_in_the_other_system() {
    // by design (web `defaultTo`): a unit with nothing to convert to shows its usual counterpart
    assert_eq!(answers(&["5 m", "3 ft"]), ["16.4042 ft", "0.9144 m"]);
}
