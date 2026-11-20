//! QA item 53: every Settings › General and Units choice changes the answer it says it does, read from the stored
//! settings keys the apps write (`merge_settings`), through `eval_options` and the live answer.
use qcalc_core::app::answer::{AnswerForm, row_answers};
use qcalc_core::app::history_show::HistoryShow;
use qcalc_core::app::live::{LiveRequest, commit, context_from_history, live};
use qcalc_core::app::settings::{default_settings, merge_settings};
use qcalc_core::app::touches::answer_copy;
use qcalc_core::units::UNIT_SETTING_GROUPS;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

const NOW: f64 = 1_790_287_960_000.0;

/// The live answer (and its exact side) for `text` with the stored settings `stored` over the defaults.
fn answer(text: &str, stored: Json) -> (String, Option<String>) {
    let s = merge_settings(&stored, &default_settings());
    let a = live(&LiveRequest {
        text: text.into(),
        options: s.eval_options(),
        phrases: true,
        now_ms: NOW,
        ..LiveRequest::default()
    });
    (a.display, a.exact)
}

fn shown(text: &str, stored: Json) -> String {
    answer(text, stored).0
}

#[test]
fn angle_mode() {
    let (deg, rad) = (json!({"angleMode": "deg"}), json!({"angleMode": "rad"}));
    for (t, d, r) in [
        ("sin(30)", "0.5", "-0.988032"),
        ("tan(45)", "1", "1.61978"),
        ("sin(pi/2)", "0.0274121", "1"),
        ("asin(0.5)", "30 deg", "0.523599 rad"),
        ("acos(0)", "90 deg", "1.5708 rad"),
        ("atan2(1,1)", "45 deg", "0.785398 rad"),
        ("angle(1+i)", "45 deg", "0.785398 rad"),
        ("solve cos(x) = 0", "90, 270", "1.5708, 4.71239"),
        ("d/dx sin(x)", "π cos(x)/180", "cos(x)"),
        ("rect(5, 0.5)", "4.99981 + 0.0436327i", "4.38791 + 2.39713i"),
        ("5∠30", "4.33013 + j2.5", "0.771257 - j4.94016"),
    ] {
        assert_eq!(shown(t, deg.clone()), d, "deg: {t}");
        assert_eq!(shown(t, rad.clone()), r, "rad: {t}");
    }
    // a typed degree sign wins over the mode
    assert_eq!(shown("sin(30°)", rad.clone()), "0.5");
    assert_eq!(answer("asin(1)", rad).1.as_deref(), Some("pi/2 rad"));
}

#[test]
fn fractions() {
    let (off, on) = (
        json!({"fractionMode": false}),
        json!({"fractionMode": true}),
    );
    for (t, a, b) in [
        ("1/3+1/4", "0.583333", "7/12"),
        ("0.75", "0.75", "3/4"),
        ("2/3 m", "0.666667 m", "2/3 m"),
        ("[1/3 2/3]", "[0.333333 0.666667]", "[1/3 2/3]"),
        ("sin(30)", "0.5", "1/2"),
    ] {
        assert_eq!(shown(t, off.clone()), a, "off: {t}");
        assert_eq!(shown(t, on.clone()), b, "on: {t}");
    }
}

#[test]
fn rationalize() {
    let (on, off) = (json!({"rationalize": true}), json!({"rationalize": false}));
    for (t, a, b) in [
        ("5/sqrt(41)", "5sqrt(41)/41", "5/sqrt(41)"),
        ("sqrt(8)/sqrt(3)", "2sqrt(6)/3", "2sqrt(2)/sqrt(3)"),
    ] {
        assert_eq!(answer(t, on.clone()).1.as_deref(), Some(a), "on: {t}");
        assert_eq!(answer(t, off.clone()).1.as_deref(), Some(b), "off: {t}");
        // the decimal is the same either way
        assert_eq!(shown(t, on.clone()), shown(t, off.clone()));
    }
}

#[test]
fn digits_shown() {
    let at = |n: u32, t: &str| shown(t, json!({ "sigFigs": n }));
    for (t, two, three, six, sixteen) in [
        ("1/3", "0.33", "0.333", "0.333333", "0.3333333333333333"),
        (
            "2/3 m",
            "0.67 m",
            "0.667 m",
            "0.666667 m",
            "0.6666666666666666 m",
        ),
        (
            "e^(i 90)",
            "-0.45 + 0.89i",
            "-0.448 + 0.894i",
            "-0.448074 + 0.893997i",
            "",
        ),
        ("100 km/h", "62 mph", "62.1 mph", "62.1371 mph", ""),
        ("x^2=2", "±1.4", "±1.41", "±1.41421", ""),
    ] {
        assert_eq!(at(2, t), two, "2: {t}");
        assert_eq!(at(3, t), three, "3: {t}");
        assert_eq!(at(6, t), six, "6: {t}");
        if !sixteen.is_empty() {
            assert_eq!(at(16, t), sixteen, "16: {t}");
        }
    }
    // out of range from an older build: clamped, not ignored
    assert_eq!(at(1, "1/3"), "0.33");
    assert_eq!(at(40, "1/3"), at(16, "1/3"));
}

#[test]
fn propagate_from_input() {
    let (off, on) = (json!({"sigFigMode": false}), json!({"sigFigMode": true}));
    for (t, a, b) in [
        ("2.0*3.00", "6", "6.0"),
        ("1.0/3", "0.333333", "0.33"),
        ("12.3 m * 2.0", "24.6 m", "25 m"),
        ("2.00 m + 1.0 m", "3 m", "3.0 m"),
        ("9.8 m/s^2", "0.999322 g", "1.0 g"),
    ] {
        assert_eq!(shown(t, off.clone()), a, "off: {t}");
        assert_eq!(shown(t, on.clone()), b, "on: {t}");
    }
}

#[test]
fn spaces_group_thousands() {
    let (on, off) = (
        json!({"spaceThousands": true}),
        json!({"spaceThousands": false}),
    );
    for (t, a, b) in [("1 000 + 1", "1001", "1"), ("1 000 000", "1000000", "0")] {
        assert_eq!(shown(t, on.clone()), a, "on: {t}");
        assert_eq!(shown(t, off.clone()), b, "off: {t}");
    }
    // not a thousands group: multiplies either way
    assert_eq!(shown("3 4", on), "12");
    assert_eq!(shown("3 4", off), "12");
    assert!(!default_settings().space_thousands);
}

/// Each unit offered for a dimension turns a quantity of that dimension into that unit, and no two choices give
/// the same answer.
#[test]
fn every_default_unit_choice_converts() {
    let sample = |dim: &str| match dim {
        "length" => "2 m + 3 ft",
        "area" => "3 ft * 4 ft",
        "volume" => "2 l",
        "speed" => "100 m / 9 s",
        "acceleration" => "10 m/s / 2 s",
        "angle" => "2 rad",
        "mass" => "3 lb + 2 kg",
        "force" => "2 kg * 9.8 m/s^2",
        "pressure" => "50 psi",
        "energy" => "10 N * 3 m",
        "power" => "100 J / 2 s",
        "temperature" => "300 K",
        "time" => "90 min",
        "frequency" => "5 Hz",
        other => panic!("no sample for {other}: add one"),
    };
    let mut items = 0;
    for item in UNIT_SETTING_GROUPS.iter().flat_map(|g| g.items.iter()) {
        items += 1;
        let t = sample(item.dim);
        let mut seen = BTreeSet::new();
        for u in item.units {
            let got = shown(t, json!({ "defaultUnits": { item.dim: u.id } }));
            assert!(!got.is_empty(), "{} = {}: {t} blank", item.dim, u.id);
            assert!(
                seen.insert(got.clone()),
                "{} = {}: {t} gave {got} again",
                item.dim,
                u.id
            );
        }
    }
    assert_eq!(items, 14);
    // a few by name, and `to …` still wins over the default
    let ft = json!({"defaultUnits": {"length": "ft"}});
    assert_eq!(shown("2 m + 3 ft", ft.clone()), "9.56168 ft");
    assert_eq!(shown("2 m to cm", ft.clone()), "200 cm");
    assert_eq!(
        shown("2 m + 3 ft", json!({"defaultUnits": {"length": "mm"}})),
        "2914.4 mm"
    );
    assert_eq!(
        shown("300 K", json!({"defaultUnits": {"temperature": "f"}})),
        "80.33 °F"
    );
    assert_eq!(
        shown("5 Hz", json!({"defaultUnits": {"frequency": "rpm"}})),
        "300 rpm"
    );
    // a unit that isn't that dimension's is dropped, as Automatic
    assert_eq!(
        shown("2 m + 3 ft", json!({"defaultUnits": {"length": "kg"}})),
        shown("2 m + 3 ft", json!({}))
    );
}

/// Each line committed in turn at its time, as the bar does, with the history setting and answer form given.
fn session(lines: &[(&str, f64)], show: HistoryShow, form: AnswerForm) -> Vec<String> {
    let o = default_settings().eval_options();
    let mut tape: Vec<Json> = Vec::new();
    let mut out = Vec::new();
    for (i, (l, at)) in lines.iter().enumerate() {
        let ctx = context_from_history(&tape, show, *at, form, 6.0);
        let r = LiveRequest {
            text: (*l).into(),
            phrases: true,
            now_ms: *at,
            options: ctx.apply(&o),
            chain_answer: ctx.chain_answer.clone(),
            last_expr: ctx.last_expr.clone(),
            answer_form: form,
            ..Default::default()
        };
        out.push(live(&r).shown);
        tape = commit(&r, &tape, &format!("r{i}")).history;
    }
    out
}

#[test]
fn show_history_scopes_variables() {
    let min = 60_000.0;
    let late = [("x = 5", NOW), ("x*2", NOW + 10.0 * min)];
    let soon = [("x = 5", NOW), ("x*2", NOW + min)];
    let f = [("f(x) = x^2", NOW), ("f(3)", NOW + 10.0 * min)];
    for (show, late_x, late_f) in [
        (HistoryShow::Always, "10", "9"),
        // recent and ↑-only: a row older than 5 minutes is out of sight, so its names don't count
        (HistoryShow::Recent, "", ""),
        (HistoryShow::Arrow, "", ""),
    ] {
        assert_eq!(
            session(&late, show, AnswerForm::Exact)[1],
            late_x,
            "{show:?}"
        );
        assert_eq!(session(&f, show, AnswerForm::Exact)[1], late_f, "{show:?}");
        assert_eq!(session(&soon, show, AnswerForm::Exact)[1], "10", "{show:?}");
    }
}

#[test]
fn answer_form() {
    let exact = session(&[("sqrt(8)", NOW)], HistoryShow::Recent, AnswerForm::Exact);
    let approx = session(&[("sqrt(8)", NOW)], HistoryShow::Recent, AnswerForm::Approx);
    assert_eq!(
        (exact[0].as_str(), approx[0].as_str()),
        ("2sqrt(2)", "2.82843")
    );
    // a chain uses the full value either way
    for form in [AnswerForm::Exact, AnswerForm::Approx] {
        let s = session(
            &[("sqrt(2)", NOW), ("^2", NOW + 1.0)],
            HistoryShow::Recent,
            form,
        );
        assert_eq!(s[1], "2", "{form:?}");
    }
    // the stored key reads back
    let s = merge_settings(&json!({"answerForm": "approx"}), &default_settings());
    assert_eq!(s.answer_form, AnswerForm::Approx);
    // a history row: ↑ lands on the side the setting reads
    let row = json!({"id": "r", "expr": "sqrt(2)", "display": "1.41421", "exact": "sqrt(2)", "n": 2f64.sqrt()});
    let e = row_answers(&row, AnswerForm::Exact, 6.0, true);
    let a = row_answers(&row, AnswerForm::Approx, 6.0, true);
    assert_ne!(e.start, a.start);
}

#[test]
fn copy_answers_without_units() {
    assert_eq!(answer_copy("15 m²", true), "15");
    assert_eq!(answer_copy("15 m²", false), "15 m²");
    assert_eq!(answer_copy("62.1371 mph", true), "62.1371");
    let s = merge_settings(&json!({"copyUnitless": false}), &default_settings());
    assert!(!s.copy_unitless);
    assert!(default_settings().copy_unitless);
}
