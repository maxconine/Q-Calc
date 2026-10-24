//! The engine's surface for the native UIs, through UniFFI (Swift now; C# or Kotlin later from the same crate).
//! The frozen API is `native/QCalcCore/API.md`.
//!
//! Kept coarse: about one call per keystroke, records in and out, no callbacks into the UI. Every typed record
//! has a JSON twin in the TS engine's exact shapes (`evaluate_json`, `LineAnswer::json`, `LiveAnswer::json`,
//! `Engine::call_json`), so the UI can reach any field the typed records don't carry yet without an FFI change.
//!
//! - `engine.rs`: the `Engine` object and the per-keystroke flow
//! - `records.rs`: records, enums, `CoreError`
//! - `cancel.rs`: `CancelToken`
//! - `worker.rs`: the 32 MB evaluation thread, the long-work pool, async futures
//! - `history.rs`, `chain.rs`: the tape and chaining (ported from `src/lib`), the history file
//! - `rates.rs`: the ECB rates cache
//! - `bridge.rs`: typed calls over `qcalc_core::golden::dispatch`, for code other packages are porting

mod bridge;
mod cancel;
mod chain;
mod engine;
pub mod history;
mod jsstr;
mod rates;
mod records;
mod worker;

pub use cancel::CancelToken;
pub use engine::Engine;
pub use records::*;

use qcalc_core::types::EvaluateOptions;

uniffi::setup_scaffolding!();

/// The settings a new user starts with (`defaultSettings()` in `src/lib/settings.ts`).
#[uniffi::export]
pub fn default_eval_options() -> EvalOptions {
    EvalOptions::default()
}

/// The engine's version.
#[uniffi::export]
pub fn core_version() -> String {
    qcalc_core::VERSION.to_string()
}

/// Every line of a sheet, on the caller's thread; each line sees what the lines above it defined.
/// Prefer `Engine::evaluate`, which runs on the engine's 32 MB thread.
#[uniffi::export]
pub fn evaluate(lines: Vec<String>, options: EvalOptions) -> Vec<LineAnswer> {
    qcalc_core::evaluate_sheet(&lines, &options.into())
        .iter()
        .map(records::line_answer)
        .collect()
}

/// `{"lines": [...], "options": {...}}` in the TS shapes in, the TS engine's `LineResult[]` JSON out.
#[uniffi::export]
pub fn evaluate_json(request: String) -> Result<String, CoreError> {
    evaluate_json_text(&request)
}

pub(crate) fn evaluate_json_text(request: &str) -> Result<String, CoreError> {
    #[derive(serde::Deserialize)]
    struct Request {
        lines: Vec<String>,
        #[serde(default)]
        options: EvaluateOptions,
    }
    let req: Request = serde_json::from_str(request).map_err(CoreError::bad)?;
    let out = qcalc_core::evaluate_sheet(&req.lines, &req.options);
    serde_json::to_string(&out).map_err(CoreError::bad)
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
