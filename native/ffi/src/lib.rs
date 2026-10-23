//! The engine's surface for the native UIs, through UniFFI (Swift now; C# or Kotlin later from the same crate).
//!
//! Kept coarse: one call per keystroke, records in and out, no callbacks into the UI. Every typed record has a
//! `json` twin (`evaluate_json`, `LineAnswer::json`) in the TS engine's exact shapes, so the UI can reach any
//! field the typed records don't carry yet without waiting for an FFI change.

use std::collections::HashMap;

use qcalc_core::types::{
    AngleMode, EvaluateOptions, LineKind, LineResult, UserFunction, ValueKind,
};

uniffi::setup_scaffolding!();

/// Errors the FFI can return; evaluation itself never fails (an unreadable line is a blank answer).
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    /// The JSON handed to `evaluate_json` wasn't an evaluate request.
    BadRequest { message: String },
}

impl std::error::Error for CoreError {}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::BadRequest { message } => write!(f, "bad request: {message}"),
        }
    }
}

/// A user function, `f(x) = x^2`.
#[derive(Clone, Debug, uniffi::Record)]
pub struct UserFn {
    pub params: Vec<String>,
    pub body: String,
}

/// Settings and the sheet's context, as `EvaluateOptions` in the TS.
#[derive(Clone, Debug, uniffi::Record)]
pub struct EvalOptions {
    pub angle_radians: bool,
    pub fraction_mode: bool,
    pub sig_figs: u32,
    pub sig_fig_mode: bool,
    pub rationalize: bool,
    pub space_thousands: bool,
    pub ans: Option<f64>,
    pub variables: HashMap<String, f64>,
    pub functions: HashMap<String, UserFn>,
    /// Unit-valued variables and `ans`, as text the unit reader reads back (`5 cm`).
    pub quantities: HashMap<String, String>,
    /// Display unit per dimension (`length` → `ft`).
    pub default_units: HashMap<String, String>,
}

/// The settings a new user starts with (`defaultSettings()` in `src/lib/settings.ts`).
#[uniffi::export]
pub fn default_eval_options() -> EvalOptions {
    EvalOptions {
        angle_radians: false,
        fraction_mode: false,
        sig_figs: 6,
        sig_fig_mode: false,
        rationalize: true,
        space_thousands: false,
        ans: None,
        variables: HashMap::new(),
        functions: HashMap::new(),
        quantities: HashMap::new(),
        default_units: HashMap::new(),
    }
}

impl From<EvalOptions> for EvaluateOptions {
    fn from(o: EvalOptions) -> Self {
        EvaluateOptions {
            angle_mode: Some(if o.angle_radians {
                AngleMode::Rad
            } else {
                AngleMode::Deg
            }),
            ans: o.ans,
            variables: o.variables.into_iter().collect(),
            functions: o
                .functions
                .into_iter()
                .map(|(k, f)| {
                    (
                        k,
                        UserFunction {
                            params: f.params,
                            body: f.body,
                        },
                    )
                })
                .collect(),
            fraction_mode: Some(o.fraction_mode),
            rationalize: Some(o.rationalize),
            sig_figs: Some(f64::from(o.sig_figs)),
            sig_fig_mode: Some(o.sig_fig_mode),
            quantities: o.quantities.into_iter().collect(),
            default_units: o.default_units.into_iter().collect(),
            space_thousands: Some(o.space_thousands),
            ..EvaluateOptions::default()
        }
    }
}

/// What a line was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AnswerKind {
    Empty,
    Expression,
    Assignment,
    Function,
    Solve,
}

/// One answered line.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LineAnswer {
    pub raw: String,
    pub kind: AnswerKind,
    /// What the answer slot shows; empty when the line is blank.
    pub display: String,
    /// The closed form beside the decimal (`2sqrt(3)`).
    pub exact: Option<String>,
    /// The number, when the answer is one (with `unit` if it has one).
    pub number: Option<f64>,
    pub unit: Option<String>,
    /// The variable an assignment defined.
    pub variable: Option<String>,
    /// A unit answer as text the reader reads back, for chaining `ans`.
    pub quantity: Option<String>,
    /// The whole `LineResult` as the TS engine's JSON, for fields not typed here yet.
    pub json: String,
}

fn answer(r: &LineResult) -> LineAnswer {
    let number = r
        .value
        .as_ref()
        .filter(|v| v.kind == ValueKind::Number)
        .map(|v| v.n);
    LineAnswer {
        raw: r.raw.clone(),
        kind: match r.kind {
            LineKind::Empty => AnswerKind::Empty,
            LineKind::Expression => AnswerKind::Expression,
            LineKind::Assignment => AnswerKind::Assignment,
            LineKind::Function => AnswerKind::Function,
            LineKind::Solve => AnswerKind::Solve,
        },
        display: r.display.clone(),
        exact: r.exact.clone(),
        number,
        unit: r.value.as_ref().and_then(|v| v.unit.clone()),
        variable: r.variable.clone(),
        quantity: r.quantity.clone(),
        json: serde_json::to_string(r).unwrap_or_default(),
    }
}

/// The engine's version.
#[uniffi::export]
pub fn core_version() -> String {
    qcalc_core::VERSION.to_string()
}

/// Every line of a sheet; each line sees what the lines above it defined.
#[uniffi::export]
pub fn evaluate(lines: Vec<String>, options: EvalOptions) -> Vec<LineAnswer> {
    qcalc_core::evaluate_sheet(&lines, &options.into())
        .iter()
        .map(answer)
        .collect()
}

/// `{"lines": [...], "options": {...}}` in the TS shapes in, the TS engine's `LineResult[]` JSON out.
#[uniffi::export]
pub fn evaluate_json(request: String) -> Result<String, CoreError> {
    #[derive(serde::Deserialize)]
    struct Request {
        lines: Vec<String>,
        #[serde(default)]
        options: EvaluateOptions,
    }
    let req: Request = serde_json::from_str(&request).map_err(|e| CoreError::BadRequest {
        message: e.to_string(),
    })?;
    let out = qcalc_core::evaluate_sheet(&req.lines, &req.options);
    serde_json::to_string(&out).map_err(|e| CoreError::BadRequest {
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_through_the_ffi_types() {
        let out = evaluate(vec!["x = 2".into(), "x^10".into()], default_eval_options());
        assert_eq!(out[1].display, "1024");
        assert_eq!(out[1].number, Some(1024.0));
        assert_eq!(out[0].kind, AnswerKind::Assignment);
        let json =
            evaluate_json(r#"{"lines":["0.1+0.2"],"options":{"sigFigs":16}}"#.into()).unwrap();
        assert!(json.contains(r#""display":"0.3""#), "{json}");
        assert!(evaluate_json("nope".into()).is_err());
    }
}
