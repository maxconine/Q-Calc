//! The other ways Tab can show an answer (`src/lib/answerForms.ts`): the plain decimal, each side of a dual
//! answer, a fraction, a closed form, scientific notation and the same quantity in other units.

use serde::{Deserialize, Serialize};

use crate::app::js as jstr;
use crate::format::{clamp_sig_figs, format_as_fraction, format_number, format_scientific, format_value};
use crate::jsre::{self, Lazy, js};
use crate::simplify::{ExactFormOptions, exact_form};
use crate::types::{AngleMode, EvaluateOptions, Meas, Value, ValueKind};
use crate::units::unit_alternatives;

/// One form. `value` is set when the form is a different quantity of units (26.5 lb as 423 oz).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnswerView {
    pub display: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// The answer to re-express, and what the answer slot already shows (forms equal to it are skipped).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormSource {
    pub value: Value,
    pub display: String,
    #[serde(default)]
    pub exact: Option<String>,
    #[serde(default)]
    pub meas: Option<Meas>,
    pub sig_figs: f64,
    #[serde(default)]
    pub sig_fig_mode: Option<bool>,
    #[serde(default)]
    pub rationalize: Option<bool>,
}

// a closed form has to name this very number, not a neighbor of it (355/113 is not π)
const SAME_NUMBER: f64 = 1e-14;

fn same_number(text: &str, n: f64) -> bool {
    let back = crate::evaluate_line(
        text,
        &EvaluateOptions {
            angle_mode: Some(AngleMode::Rad),
            ..EvaluateOptions::default()
        },
    )
    .value;
    let Some(back) = back else { return false };
    if back.kind != ValueKind::Number
        || back.unit.as_deref().is_some_and(|u| !u.is_empty())
        || !back.n.is_finite()
    {
        return false;
    }
    (back.n - n).abs() <= SAME_NUMBER * n.abs().max(1.0)
}

fn with_unit(text: String, unit: Option<&str>) -> String {
    match unit {
        Some(u) if !u.is_empty() => format!("{text} {u}"),
        _ => text,
    }
}

fn digit_count(s: &str) -> usize {
    s.chars().filter(char::is_ascii_digit).count()
}

fn has_unc(m: Option<&Meas>) -> bool {
    m.and_then(|m| m.unc).is_some_and(|u| u != 0.0 && !u.is_nan())
}

/// The other ways Tab can show an answer, in cycle order; the answer as shown is not included.
pub fn answer_forms(src: &FormSource) -> Vec<AnswerView> {
    static EXACT_NAMES: Lazy = Lazy::new(|| js(r"[π√]|pi|sqrt", ""));
    static E_NOTATION: Lazy = Lazy::new(|| js(r"e[+-]", ""));
    static NOT_SHOWN: Lazy = Lazy::new(|| js(r"undefined|∞", ""));
    let value = &src.value;
    let sig_figs = src.sig_figs;
    if value.kind != ValueKind::Number || !value.n.is_finite() {
        return Vec::new();
    }
    // a measured answer's digits are its precision; re-expressing them would claim more or less
    let sig_of = |m: Option<&Meas>| m.is_some_and(|m| m.sig.is_some());
    if has_unc(src.meas.as_ref())
        || has_unc(value.meas.as_ref())
        || (src.sig_fig_mode == Some(true)
            && (sig_of(src.meas.as_ref()) || sig_of(value.meas.as_ref())))
    {
        return Vec::new();
    }
    let n = value.n;
    let unit = value.unit.as_deref();
    let mut out: Vec<AnswerView> = Vec::new();
    let mut add = |display: String, v: Option<Value>| {
        if !display.is_empty() {
            out.push(AnswerView { display, value: v });
        }
    };

    let dual = src
        .exact
        .as_deref()
        .is_some_and(|e| !e.is_empty() && jstr::trim(e) != jstr::trim(&src.display));
    add(with_unit(format_number(n, sig_figs), unit), None);
    // a dual answer can also be taken one side at a time
    if dual {
        add(src.exact.clone().unwrap_or_default(), None);
    }
    // 200/9 °C restates a decimal nobody reads as a fraction; so does 123456789/1000
    if let Some(fraction) = format_as_fraction(n, 10_000)
        && fraction.contains('/')
        && !unit.is_some_and(|u| u.contains('°'))
        && digit_count(&fraction) <= 8
        && same_number(&fraction, n)
    {
        add(with_unit(fraction, unit), None);
    }
    if let Some(exact) = exact_form(
        n,
        ExactFormOptions {
            rationalize: src.rationalize,
            wide: false,
        },
    ) && jsre::test(&EXACT_NAMES, &exact)
        && !jsre::test(&E_NOTATION, &exact)
        && digit_count(&exact) <= 6
        && same_number(&exact, n)
    {
        add(with_unit(exact, unit), None);
    }
    let mag = if n == 0.0 { 0.0 } else { n.abs().log10().floor() };
    if mag.abs() >= 3.0 {
        add(
            with_unit(format_scientific(n, clamp_sig_figs(sig_figs)), unit),
            None,
        );
    }
    for alt in unit_alternatives(value, 3) {
        add(format_value(&alt, sig_figs), Some(alt));
    }

    let shown = (!dual).then(|| jstr::trim(&src.display).to_string());
    let mut seen: Vec<String> = Vec::new();
    out.retain(|form| {
        let key = jstr::trim(&form.display).to_string();
        if shown.as_ref() == Some(&key) || seen.contains(&key) || jsre::test(&NOT_SHOWN, &key) {
            return false;
        }
        seen.push(key);
        true
    });
    out
}
