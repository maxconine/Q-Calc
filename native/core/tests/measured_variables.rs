//! A variable set to a measured value keeps its ± and sig figs on later lines (item 12 of QA_BACKLOG.md):
//! history JSON stores `sig`/`dp` as `2.0`, which `Meas` failed to read back, so `x = 3.0 ± 0.1` then `x^2` was `9`.
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use qcalc_core::types::Meas;

fn run(lines: &[&str], sig_fig_mode: bool) -> Vec<String> {
    let mut s = qcalc_core::app::settings::default_settings();
    s.sig_fig_mode = sig_fig_mode;
    let o = s.eval_options();
    let now = 1_795_651_200_000.0;
    let mut tape: Vec<serde_json::Value> = Vec::new();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let ctx = context_from_history(&tape, HistoryShow::Always, now, AnswerForm::Exact, 6.0);
        let r = LiveRequest {
            text: (*l).into(),
            phrases: true,
            options: ctx.apply(&o),
            chain_answer: ctx.chain_answer.clone(),
            last_expr: ctx.last_expr.clone(),
            now_ms: now,
            ..Default::default()
        };
        out.push(live(&r).display);
        tape = commit(&r, &tape, &format!("r{i}")).history;
    }
    out
}

#[test]
fn meas_reads_counts_written_as_floats() {
    let m: Meas = serde_json::from_str(r#"{"sig":2.0,"dp":1.0,"unc":0.1,"uncDigits":"1"}"#).unwrap();
    assert_eq!((m.sig, m.dp, m.unc), (Some(2), Some(1), Some(0.1)));
    let m: Meas = serde_json::from_str(r#"{"sig":3,"dp":2}"#).unwrap();
    assert_eq!((m.sig, m.dp), (Some(3), Some(2)));
}

#[test]
fn uncertain_variable_keeps_its_plus_minus() {
    let got = run(&["x = 3.0 ± 0.1", "x^2", "2x"], false);
    assert_eq!(got, ["3.0 ± 0.1", "9.0 ± 0.6", "6.0 ± 0.2"]);
}

#[test]
fn sig_fig_variable_keeps_its_figures() {
    let got = run(&["x = 2.50", "x * 2", "y = 3.0 ± 0.1", "x*y"], true);
    assert_eq!(got, ["2.50", "5.00", "3.0 ± 0.1", "7.5 ± 0.3"]);
}
