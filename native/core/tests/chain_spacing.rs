//! QA item 78: the walkthrough's `* 2` chain wrote the tape row `84* 2`; a spaced operator is now spaced on both
//! sides (`84 * 2`), a glued one stays glued (`84*2`). The page's `chainedHistoryExpr` (goldens) is unchanged.
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::chain::{chained_history_expr, chained_tape_expr};
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use serde_json::Value as Json;

/// Each line committed in turn, as the bar does: the answer and the tape's expression.
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
fn walkthrough_chain_is_evenly_spaced() {
    let s = session(&["12 * 7", "* 2", "/4", "+ 1", "^ 2"]);
    assert_eq!(s[1], ("168".into(), "84 * 2".into()));
    assert_eq!(s[2], ("42".into(), "168/4".into()));
    assert_eq!(s[3], ("43".into(), "42 + 1".into()));
    assert_eq!(s[4], ("1849".into(), "43 ^ 2".into()));
}

#[test]
fn tape_expr_spacing() {
    assert_eq!(chained_tape_expr("* 2", "84"), "84 * 2");
    assert_eq!(chained_tape_expr(" * 2", "84"), "84 * 2");
    assert_eq!(chained_tape_expr("*2", "84"), "84*2");
    assert_eq!(chained_tape_expr("* 2", "-3"), "(-3) * 2");
    assert_eq!(chained_tape_expr("× 2", "26.5 lb"), "(26.5 lb) × 2");
    assert_eq!(chained_tape_expr("- 1", "5"), "5 - 1");
    assert_eq!(chained_tape_expr(" in kg", "26.5 lb"), "26.5 lb in kg");
    assert_eq!(chained_tape_expr("!", "5"), "5!");
    // the page's own form, for the goldens
    assert_eq!(chained_history_expr("+ 1", "sqrt(3)/2"), "sqrt(3)/2+ 1");
}
