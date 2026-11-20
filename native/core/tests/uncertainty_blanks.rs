//! Uncertainty forms that were blank (item 37 of QA_BACKLOG.md).
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, context_from_history, live};

fn run(l: &str) -> String {
    let s = qcalc_core::app::settings::default_settings();
    let o = s.eval_options();
    let now = 1_795_651_200_000.0;
    let ctx = context_from_history(&[], HistoryShow::Always, now, AnswerForm::Exact, 6.0);
    let r = LiveRequest {
        text: l.into(),
        phrases: true,
        options: ctx.apply(&o),
        now_ms: now,
        ..Default::default()
    };
    live(&r).display
}

#[test]
fn were_blank() {
    for (l, want) in [
        ("(6.02 ± 0.01)e23", "6.02e+23 ± 1e+21"),
        ("(6.02 ± 0.01)E23", "6.02e+23 ± 1e+21"),
        ("10 m ± 1 m", "10 ± 1 m"),
        ("10 m ± 5 cm", "10.00 ± 0.05 m"),
        ("10 m ± 1", "10 ± 1 m"),
        ("(10 m ± 1 m) * 2", "20 ± 2 m"),
        ("(2.0 ± 0.1)^(3.0 ± 0.1)", "8.0 ± 1.8"),
        ("2^(3.0 ± 0.1)", "8.0 ± 0.6"),
    ] {
        assert_eq!(run(l), want, "{l}");
    }
}

#[test]
fn tiny_values_go_scientific() {
    assert_eq!(run("(6.02 ± 0.01)e-23"), "6.02e-23 ± 1e-25");
    assert_eq!(run("6.02e-23 ± 0.01e-23"), "6.02e-23 ± 1e-25");
}

#[test]
fn still_blank() {
    // a ± that can't be kept gives no answer rather than a bare number
    for l in ["10 m ± 1 m ± 1 m", "(-2.0 ± 0.1)^(3.0 ± 0.1)"] {
        assert!(
            !run(l).contains(|c: char| c.is_ascii_digit()) || run(l).contains('±'),
            "{l}: {}",
            run(l)
        );
    }
    assert_ne!(run("10 m ± 1 s"), "10 ± 1 m");
}
