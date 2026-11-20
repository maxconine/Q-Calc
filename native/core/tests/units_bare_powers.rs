//! QA backlog item 106: a bare 2 or 3 on the unit after a slash (`ft/s2`) is that unit squared or cubed.
//! `9.8 m/s^2 to ft/s2` was blank (the web's too: `s2` is no unit); `m/s2` and `ft2` were already table names.
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use qcalc_core::units::unit_powers_shown;
use serde_json::Value as Json;

fn d(t: &str) -> String {
    live(&LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    })
    .display
}

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
fn bare_power_after_a_slash_is_a_power() {
    for (t, want, shown) in [
        (
            "9.8 m/s^2 to ft/s2",
            "32.1522309711 ft/s^2",
            "32.1522309711 ft/s²",
        ),
        (
            "9.8 m/s2 to ft/s2",
            "32.1522309711 ft/s^2",
            "32.1522309711 ft/s²",
        ),
        (
            "9.8 m/s^2 in ft/s2",
            "32.1522309711 ft/s^2",
            "32.1522309711 ft/s²",
        ),
        (
            "9.8 m s^-2 to ft/s2",
            "32.1522309711 ft/s^2",
            "32.1522309711 ft/s²",
        ),
        ("1 ft/s2 to m/s2", "0.3048 m/s^2", "0.3048 m/s²"),
        ("1 m/s2 to cm/s2", "100 cm/s^2", "100 cm/s²"),
        ("1 kg m2/s2 to J", "1 J", "1 J"),
        ("1 J to kg m2/s2", "1 kg m2/s^2", "1 kg m²/s²"),
    ] {
        assert_eq!(d(t), want, "{t}");
        assert_eq!(unit_powers_shown(want), shown, "{t}");
    }
    // unchanged: table names and plain powers
    for (t, want) in [
        ("9.8 m/s^2 to ft/s^2", "32.1522309711 ft/s^2"),
        ("9.8 m/s2", "0.999321888718 g"),
        ("100 N/m2 to psi", "0.014503773773 psi"),
        ("1 kg/m3 to lb/ft3", "0.0624279605761 lb/ft3"),
        ("1 W/m2 to W/ft2", "0.09290304 W/ft2"),
        ("5 kg m2", "5 kg·m²"),
    ] {
        assert_eq!(d(t), want, "{t}");
    }
}

#[test]
fn a_variable_named_like_a_bare_power_is_kept() {
    let s = session(&["s2 = 4", "8 m/s2", "6/s2"]);
    assert_eq!(s[2], "1.5");
    // `s2` is the variable, not seconds squared
    assert_ne!(s[1], "8 m/s²");
}
