//! The user's settings (`src/lib/settings.ts`, with `normalizeTheme` from `theme.ts` and `clampDraftSeconds` from
//! `draft.ts`): defaults, merging a partial update from storage or the settings window, equality and the toggles.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::answer::{AnswerForm, HistoryInsert, normalize_history_insert};
use crate::app::history_show::{HistoryShow, normalize_history_show};
use crate::app::keybinds::{Keybinds, sanitize_keybinds};
use crate::app::needs::sanitize_default_units;
use crate::format::clamp_sig_figs;
use crate::types::{AngleMode, DefaultUnits, EvaluateOptions};

/// What a new user's answers round to; the engine's own fallback (`DEFAULT_SIG_FIGS`) stays 12 for its callers.
pub const DEFAULT_SETTINGS_SIG_FIGS: u32 = 6;

pub const DEFAULT_DRAFT_SECONDS: u32 = 60;
pub const MIN_DRAFT_SECONDS: f64 = 0.0;
pub const MAX_DRAFT_SECONDS: f64 = 3600.0;

/// How long a hidden bar keeps its input: whole seconds up to an hour, 60 when unreadable (`clampDraftSeconds`,
/// lib/draft.ts; WP-UNITS owns the rest of that module).
pub fn clamp_draft_seconds(n: f64) -> u32 {
    if !n.is_finite() || n < MIN_DRAFT_SECONDS {
        return DEFAULT_DRAFT_SECONDS;
    }
    crate::jsnum::round(n).min(MAX_DRAFT_SECONDS) as u32
}

/// The colour scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// `normalizeTheme`: anything unknown follows the system.
pub fn normalize_theme(value: &Json) -> Theme {
    match value.as_str() {
        Some("light") => Theme::Light,
        Some("dark") => Theme::Dark,
        _ => Theme::System,
    }
}

/// Every setting, in the TS `Settings` JSON shape (a field missing from JSON reads as its default).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub angle_mode: AngleMode,
    pub fraction_mode: bool,
    pub sig_fig_mode: bool,
    pub rationalize: bool,
    pub keep_words: bool,
    pub answer_form: AnswerForm,
    pub history_insert: HistoryInsert,
    pub history_show: HistoryShow,
    pub sig_figs: u32,
    pub draft_seconds: u32,
    pub default_units: DefaultUnits,
    pub theme: Theme,
    pub typst_preview: bool,
    pub typst_copy: bool,
    /// A typed space between digits groups thousands (`1 000 000`); off, it multiplies (`3 4` is 12).
    pub space_thousands: bool,
    /// Copied answers leave their unit behind (`15 m²` copies as 15).
    pub copy_unitless: bool,
    /// Anonymous usage counts.
    pub share_usage: bool,
    pub keybinds: Keybinds,
}

impl Default for Settings {
    fn default() -> Self {
        default_settings()
    }
}

/// A new user's settings.
pub fn default_settings() -> Settings {
    Settings {
        angle_mode: AngleMode::Deg,
        fraction_mode: false,
        sig_fig_mode: false,
        rationalize: true,
        keep_words: false,
        answer_form: AnswerForm::Exact,
        history_insert: HistoryInsert::Expr,
        history_show: HistoryShow::Recent,
        sig_figs: DEFAULT_SETTINGS_SIG_FIGS,
        draft_seconds: DEFAULT_DRAFT_SECONDS,
        default_units: DefaultUnits::new(),
        theme: Theme::System,
        typst_preview: true,
        typst_copy: false,
        space_thousands: false,
        copy_unitless: true,
        share_usage: true,
        keybinds: Keybinds::new(),
    }
}

impl Settings {
    /// What the engine reads of the settings; the sheet's context (ans, variables…) is added by the caller.
    pub fn eval_options(&self) -> EvaluateOptions {
        EvaluateOptions {
            angle_mode: Some(self.angle_mode),
            fraction_mode: Some(self.fraction_mode),
            rationalize: Some(self.rationalize),
            sig_figs: Some(f64::from(self.sig_figs)),
            sig_fig_mode: Some(self.sig_fig_mode),
            default_units: self.default_units.clone(),
            space_thousands: Some(self.space_thousands),
            ..EvaluateOptions::default()
        }
    }

    /// The settings as the TS JSON.
    pub fn to_json(&self) -> Json {
        serde_json::to_value(self).unwrap_or(Json::Null)
    }
}

/// JS truthiness (`Boolean(v)`).
pub fn truthy(v: &Json) -> bool {
    match v {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Json::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// A partial update (from storage, the settings window or the Mac app) over `base`; each field is sanitised, and a
/// missing or null one keeps `base`'s.
pub fn merge_settings(partial: &Json, base: &Settings) -> Settings {
    let get = |k: &str| partial.get(k).filter(|v| !v.is_null());
    let flag = |k: &str, b: bool| get(k).map_or(b, truthy);
    let num = |v: &Json| v.as_f64().unwrap_or(f64::NAN);
    Settings {
        angle_mode: match get("angleMode").and_then(Json::as_str) {
            Some("rad") => AngleMode::Rad,
            Some("deg") => AngleMode::Deg,
            _ => base.angle_mode,
        },
        fraction_mode: flag("fractionMode", base.fraction_mode),
        sig_fig_mode: flag("sigFigMode", base.sig_fig_mode),
        rationalize: flag("rationalize", base.rationalize),
        keep_words: flag("keepWords", base.keep_words),
        answer_form: match get("answerForm").and_then(Json::as_str) {
            Some("approx") => AnswerForm::Approx,
            Some("exact") => AnswerForm::Exact,
            _ => base.answer_form,
        },
        history_insert: get("historyInsert").map_or(base.history_insert, normalize_history_insert),
        history_show: get("historyShow").map_or(base.history_show, normalize_history_show),
        sig_figs: get("sigFigs").map_or(base.sig_figs, |v| clamp_sig_figs(num(v))),
        draft_seconds: get("draftSeconds").map_or(base.draft_seconds, |v| clamp_draft_seconds(num(v))),
        default_units: get("defaultUnits")
            .map_or_else(|| base.default_units.clone(), sanitize_default_units),
        theme: get("theme").map_or(base.theme, normalize_theme),
        typst_preview: flag("typstPreview", base.typst_preview),
        typst_copy: flag("typstCopy", base.typst_copy),
        space_thousands: flag("spaceThousands", base.space_thousands),
        copy_unitless: flag("copyUnitless", base.copy_unitless),
        share_usage: flag("shareUsage", base.share_usage),
        keybinds: get("keybinds").map_or_else(|| base.keybinds.clone(), sanitize_keybinds),
    }
}

/// Whether two settings are the same (`settingsEqual`).
pub fn settings_equal(a: &Settings, b: &Settings) -> bool {
    a == b
}

/// What the Mac app pushes beside the settings; never stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInfo {
    pub hotkey: String,
    pub hotkey_failed: bool,
}

/// Each field of `partial` that has the right type, over `base`.
pub fn merge_native_info(partial: &Json, base: &NativeInfo) -> NativeInfo {
    NativeInfo {
        hotkey: partial
            .get("hotkey")
            .and_then(Json::as_str)
            .map_or_else(|| base.hotkey.clone(), str::to_string),
        hotkey_failed: partial
            .get("hotkeyFailed")
            .and_then(Json::as_bool)
            .unwrap_or(base.hotkey_failed),
    }
}

/// ⌃D.
pub fn toggle_angle_mode(s: &Settings) -> Settings {
    Settings {
        angle_mode: match s.angle_mode {
            AngleMode::Deg => AngleMode::Rad,
            AngleMode::Rad => AngleMode::Deg,
        },
        ..s.clone()
    }
}

/// ⌃F.
pub fn toggle_fraction_mode(s: &Settings) -> Settings {
    Settings {
        fraction_mode: !s.fraction_mode,
        ..s.clone()
    }
}

/// ⌃S.
pub fn toggle_sig_fig_mode(s: &Settings) -> Settings {
    Settings {
        sig_fig_mode: !s.sig_fig_mode,
        ..s.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_partials() {
        let base = default_settings();
        let next = merge_settings(
            &json!({"angleMode": "grad", "sigFigs": 40, "fractionMode": 1, "draftSeconds": -5}),
            &base,
        );
        assert_eq!(next.angle_mode, AngleMode::Deg);
        assert_eq!(next.sig_figs, 16);
        assert!(next.fraction_mode);
        assert_eq!(next.draft_seconds, 60);
        assert_eq!(base.to_json()["historyShow"], json!("recent"));
    }
}
