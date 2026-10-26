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
//! - `name_check.rs`: `NameCheck`, the relay's word on the name after `message`
//! - `bridge.rs`: typed calls over `qcalc_core::golden::dispatch`, for code other packages are porting

mod bridge;
mod cancel;
pub mod chain;
mod engine;
pub mod history;
mod jsstr;
mod name_check;
mod rates;
pub mod records;
#[cfg(feature = "typst")]
mod typst_preview; // WP-TYPST-RENDER
mod worker;

pub use cancel::CancelToken;
pub use engine::Engine;
pub use name_check::NameCheck;
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

// ---- WP-UNITS: the Settings unit choices (replaces `UnitCatalog` in `macos/UnitSettings.swift`) ----

/// One unit a default-unit setting can pick.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UnitChoiceInfo {
    pub id: String,
    pub label: String,
}

/// One setting: the default unit for one dimension (`dim` is the stored key, `length`).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UnitSettingItemInfo {
    pub dim: String,
    pub label: String,
    pub units: Vec<UnitChoiceInfo>,
}

/// A Settings section of unit choices.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UnitSettingGroupInfo {
    pub id: String,
    pub title: String,
    pub items: Vec<UnitSettingItemInfo>,
}

/// `UNIT_SETTING_GROUPS`: the default-unit choices Settings offers, in order. "Automatic" (no entry) is the UI's.
#[uniffi::export]
pub fn unit_setting_groups() -> Vec<UnitSettingGroupInfo> {
    qcalc_core::units::UNIT_SETTING_GROUPS
        .iter()
        .map(|g| UnitSettingGroupInfo {
            id: g.id.into(),
            title: g.title.into(),
            items: g
                .items
                .iter()
                .map(|i| UnitSettingItemInfo {
                    dim: i.dim.into(),
                    label: i.label.into(),
                    units: i
                        .units
                        .iter()
                        .map(|u| UnitChoiceInfo {
                            id: u.id.into(),
                            label: u.label.into(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

/// `sanitizeDefaultUnits`: only Settings dimensions, each with a unit of that dimension.
#[uniffi::export]
pub fn sanitize_default_units(
    units: std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    qcalc_core::units::sanitize_default_units(&units.into_iter().collect())
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_settings_come_from_the_core() {
        let groups = unit_setting_groups();
        assert_eq!(groups[0].items[0].units[0].id, "in");
        let kept = sanitize_default_units(std::collections::HashMap::from([
            ("length".to_string(), "ft".to_string()),
            ("mass".to_string(), "ft".to_string()),
        ]));
        assert_eq!(kept.len(), 1);
    }

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
