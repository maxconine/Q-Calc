//! The engine's public data model. Field names and shapes follow `src/engine/types.ts` exactly (serde camelCase),
//! so a `LineResult` serialises to the same JSON the TS engine returns and the golden fixtures compare directly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Degrees or radians, for trig in and inverse trig out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AngleMode {
    #[default]
    Deg,
    Rad,
}

/// Whether an answer is a number (with an optional unit) or text the engine wrote out (`undefined`, `x = 2, 3`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueKind {
    #[default]
    Number,
    Text,
}

/// Sig figs and ± of a measured answer. Absent `sig` and `dp` mean exact.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meas {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unc: Option<f64>,
    /// Significant digits of a ± the user typed, while the answer's ± is still that one (`0.10` → `10`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unc_digits: Option<String>,
}

/// A complex answer's parts at full precision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

/// One answer. `n` is 0 for text answers, as in the TS engine.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Value {
    pub kind: ValueKind,
    pub n: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Table id behind `unit`, for SI prefix stepping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meas: Option<Meas>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex: Option<Complex>,
}

impl Value {
    /// A plain number.
    pub fn number(n: f64) -> Self {
        Value {
            kind: ValueKind::Number,
            n,
            ..Value::default()
        }
    }

    /// Text the engine wrote out, such as `undefined` or `overflow`.
    pub fn text(text: impl Into<String>) -> Self {
        Value {
            kind: ValueKind::Text,
            n: 0.0,
            text: Some(text.into()),
            ..Value::default()
        }
    }
}

/// What a line turned out to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Empty,
    #[default]
    Expression,
    Assignment,
    Function,
    Solve,
}

/// How a solve ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SolveOutcome {
    Roots,
    None,
    Contradiction,
    NoneFound,
    All,
}

/// A solve's unknown and roots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveInfo {
    pub variable: String,
    /// Ascending; at most `MAX_SHOWN_ROOTS`, the ones nearest 0 when there are more.
    pub roots: Vec<f64>,
    pub outcome: SolveOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<bool>,
}

/// A user function, `f(x) = x^2` as params `[x]` and body `x^2`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserFunction {
    pub params: Vec<String>,
    pub body: String,
}

/// Letters a time derivative took as moving with time or held constant.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeLetters {
    pub varying: Vec<String>,
    pub constant: Vec<String>,
    pub locked: Vec<String>,
}

/// One evaluated line: the answer, how to show it, and what it defines.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineResult {
    pub raw: String,
    pub kind: LineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    pub display: String,
    /// Simplified exact form, for trig and square-root expressions (`2sqrt(3)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meas: Option<Meas>,
    /// A unit answer as text the unit parser reads back (`5 cm`); empty when it can't be read back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_params: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solve: Option<SolveInfo>,
    /// A calculus answer the closed-form search may still upgrade (opaque job, see `ARCHITECTURE.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_form: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<TimeLetters>,
}

impl LineResult {
    /// A line with nothing in it.
    pub fn empty(raw: &str) -> Self {
        LineResult {
            raw: raw.to_string(),
            kind: LineKind::Empty,
            ..LineResult::default()
        }
    }

    /// A line the engine can't answer: shown blank.
    pub fn blank(raw: &str) -> Self {
        LineResult {
            raw: raw.to_string(),
            kind: LineKind::Expression,
            ..LineResult::default()
        }
    }
}

/// Default display units per dimension (`{"length": "ft"}`).
pub type DefaultUnits = BTreeMap<String, String>;

/// Everything besides the text that decides an answer: settings plus the sheet's context.
/// Mirrors `EvaluateOptions` in `src/engine/types.ts`; every field is optional there and defaults the same here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EvaluateOptions {
    pub angle_mode: Option<AngleMode>,
    pub ans: Option<f64>,
    pub variables: BTreeMap<String, f64>,
    pub functions: BTreeMap<String, UserFunction>,
    pub fraction_mode: Option<bool>,
    /// Move square roots out of the denominator. Default true.
    pub rationalize: Option<bool>,
    pub sig_figs: Option<f64>,
    /// Round measured answers to the sig figs their inputs carry.
    pub sig_fig_mode: Option<bool>,
    pub measures: BTreeMap<String, Meas>,
    /// Unit-valued variables and `ans`, as `LineResult::quantity` text.
    pub quantities: BTreeMap<String, String>,
    pub default_units: DefaultUnits,
    /// A typed space groups thousands (`1 000 000`); off, it multiplies (`3 4` is 12).
    pub space_thousands: Option<bool>,
    /// For d/dt: letters moved to varying (true) or constant (false) by hand.
    pub time_varying: BTreeMap<String, bool>,
}

impl EvaluateOptions {
    /// The angle mode, degrees unless set.
    pub fn angle(&self) -> AngleMode {
        self.angle_mode.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_result_serialises_like_the_ts_engine() {
        let r = LineResult {
            raw: "2+3".into(),
            kind: LineKind::Expression,
            value: Some(Value::number(5.0)),
            display: "5".into(),
            ..LineResult::default()
        };
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(
            json,
            r#"{"raw":"2+3","kind":"expression","value":{"kind":"number","n":5.0},"display":"5"}"#
        );
    }

    #[test]
    fn options_read_from_ts_json() {
        let o: EvaluateOptions =
            serde_json::from_str(r#"{"angleMode":"rad","sigFigs":6,"variables":{"x":2}}"#).unwrap();
        assert_eq!(o.angle(), AngleMode::Rad);
        assert_eq!(o.sig_figs, Some(6.0));
        assert_eq!(o.variables.get("x"), Some(&2.0));
    }
}
