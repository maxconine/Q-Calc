//! QA item 62: `ans` and variables keep a moles answer's mol, a conductance reads in siemens, `T` is the tesla in a
//! magnetic line (beside an area, or with a magnetic unit or target), and `(5 km/h)^2` reads in km²/h².
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::EvaluateOptions;
use serde_json::Value as Json;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

fn sheet(lines: &[&str]) -> Vec<String> {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&lines, &EvaluateOptions::default())
        .into_iter()
        .map(|r| r.display)
        .collect()
}

/// Each line committed in turn, as the bar does: the live answers.
fn session(lines: &[&str]) -> Vec<String> {
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
        out.push(live(&r).display);
        tape = commit(&r, &tape, &format!("r{i}")).history;
    }
    out
}

#[test]
fn ans_after_moles_keeps_mol() {
    assert_eq!(
        sheet(&["0.5 mol/L * 2 L", "ans", "ans * 18 g/mol", "2 ans"]),
        ["1 mol", "1 mol", "18 g", "36 g"]
    );
    assert_eq!(sheet(&["0.5 mol/L * 2 L", "ans / 2 L"])[1], "0.5 mol/L");
    assert_eq!(
        sheet(&["8.314 J/(mol K) * 300 K", "ans * 2 mol"])[1],
        "4.9884 kJ"
    );
    // a variable set to moles (was blank)
    assert_eq!(
        sheet(&["x = 0.5 mol/L * 2 L", "x * 18 g/mol"]),
        ["1 mol", "18 g"]
    );
    // web paths unchanged
    assert_eq!(sheet(&["n = 2", "n * 3 g"]), ["2", "6 g"]);
    assert_eq!(show("1 mol"), "");
}

#[test]
fn ans_after_moles_in_the_bar() {
    let s = session(&["0.5 mol/L * 2 L", "ans * 18 g/mol"]);
    assert_eq!(s, ["1 mol", "18 g"]);
}

#[test]
fn conductance_in_siemens() {
    assert_eq!(show("60 Hz 100 µF"), "0.006 S");
    assert_eq!(show("2 siemens"), "2 S");
    assert_eq!(show("2 siemens * 3 V"), "6 A");
    assert_eq!(sheet(&["60 Hz 100 µF", "ans * 2 V"])[1], "12 mA");
    // the web's own reciprocal keeps 1/Ω (a golden), and S alone is still seconds
    assert_eq!(show("1 / 6.8 ohms"), "0.147058823529 1/Ω");
    assert_eq!(show("1 S"), "1 s");
}

#[test]
fn tesla_in_a_magnetic_line() {
    assert_eq!(show("2 T * 3 m^2"), "6 Wb");
    assert_eq!(show("2 T * 3 cm^2"), "0.0006 Wb");
    assert_eq!(show("2 T * 3 m^2 in mWb"), "6000 mWb");
    assert_eq!(show("5 mT * 2 m^2"), "0.01 Wb");
    assert_eq!(show("2 tesla * 3 m^2"), "6 Wb");
    assert_eq!(show("1 Wb"), "1 Wb");
    assert_eq!(show("3 mWb / 2 m^2"), "0.0015 T");
    assert_eq!(show("100 gauss in T"), "0.01 T");
    assert_eq!(show("2 T in mT"), "2000 mT");
    assert_eq!(sheet(&["2 T * 3 m^2", "ans / 2 s"])[1], "3 V");
    // elsewhere T stays the ton (web goldens)
    assert_eq!(show("2 T * 3 A * 1 m"), "6000 kg m A");
    assert_eq!(show("2 T in kg"), "2000 kg");
}

#[test]
fn power_of_a_speed_in_its_units() {
    assert_eq!(show("(5 km/h)^2"), "25 km²/h²");
    assert_eq!(show("(3 km/h)^3"), "27 km³/h³");
    assert_eq!(show("(10 cm/s)^2"), "100 cm²/s²");
    assert_eq!(show("(5 km/h)^2 * 2 kg"), "3.85802469136 J");
}
