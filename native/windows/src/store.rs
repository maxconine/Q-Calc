//! Settings and the files the app keeps, read and written in the Tauri app's exact shapes so an existing user's
//! settings, shortcut, onboarding and bar spots carry over: `%APPDATA%\com.maxconine.qcalc\settings.json`
//! (`Store`, ported from src-tauri/src/store.rs with its tests), `rates.json` beside it, and the engine's
//! `history.json` (written by `Engine::save_history`, data_dir = that folder). The draft (`draft.json`,
//! `{expr, savedAt}`) is new to the native app.
//!
//! The Tauri app kept history in the web view's localStorage (WebView2's LevelDB under
//! `%LOCALAPPDATA%\com.maxconine.qcalc\EBWebView`); reading it over is a later job (see README "gaps").
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

const MIN_SIG_FIGS: f64 = 2.0;
const MAX_SIG_FIGS: f64 = 16.0;
const DEFAULT_SIG_FIGS: i64 = 6;
// the file keeps every setting, the defaults too, so a changed default needs the file brought up to date once: 2 is
// 6 significant figures (was 12) and the typst preview on (was off). a 12 or an off preview in an older file is
// taken for the old default, since the file can't say whether someone chose it
const DEFAULTS: u64 = 2;
const MAX_DRAFT_SECONDS: f64 = 3600.0;
const DEFAULT_DRAFT_SECONDS: i64 = 60;
// how long a moved or widened bar keeps its spot and width after it was last used; 0 keeps them for good. the
// mac's 5 minutes
pub const DEFAULT_SPOT_SECONDS: i64 = 300;
const MAX_SPOT_SECONDS: f64 = 86_400.0;

// what the settings window sends for "move the bar back after"; anything odd is the default
pub fn spot_seconds(v: &Value) -> i64 {
    match v.as_f64() {
        Some(n) if n.is_finite() && n >= 0.0 => n.round().min(MAX_SPOT_SECONDS) as i64,
        _ => DEFAULT_SPOT_SECONDS,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub angle_mode: String,
    pub fraction_mode: bool,
    pub sig_fig_mode: bool,
    pub rationalize: bool,
    pub keep_words: bool,
    pub answer_form: String,
    pub history_insert: String,
    pub history_show: String,
    pub sig_figs: i64,
    pub draft_seconds: i64,
    pub default_units: BTreeMap<String, String>,
    pub theme: String,
    pub typst_preview: bool,
    pub typst_copy: bool,
    // a typed space between digits groups thousands; off, it multiplies
    pub space_thousands: bool,
    // copied answers leave their unit behind
    pub copy_unitless: bool,
    pub share_usage: bool,
    // the page's own shortcuts, overrides only; "" is an action left without a key. the page checks the chords
    pub keybinds: BTreeMap<String, String>,
    // native only: Ctrl+E's exponent form, on until pressed again (exponent.rs)
    pub exponent_mode: bool,
}

// the core's actions, then the native app's own (exponent.rs NATIVE_ACTIONS)
const KEY_ACTIONS: [&str; 9] = [
    "settings",
    "copyAnswer",
    "copyLine",
    "angle",
    "fraction",
    "sigFigs",
    "clear",
    "exponent",
    "forms",
];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            angle_mode: "deg".into(),
            fraction_mode: false,
            sig_fig_mode: false,
            rationalize: true,
            keep_words: false,
            answer_form: "exact".into(),
            history_insert: "expr".into(),
            history_show: "recent".into(),
            sig_figs: DEFAULT_SIG_FIGS,
            draft_seconds: DEFAULT_DRAFT_SECONDS,
            default_units: BTreeMap::new(),
            theme: "system".into(),
            typst_preview: true,
            typst_copy: false,
            space_thousands: false,
            copy_unitless: true,
            share_usage: true,
            keybinds: BTreeMap::new(),
            exponent_mode: false,
        }
    }
}

fn pick(value: &Value, allowed: &[&str], fallback: &str) -> String {
    let s = value.as_str().unwrap_or_default();
    if allowed.contains(&s) { s } else { fallback }.to_string()
}

impl Settings {
    // the page's mergeSettings rules: a missing field keeps the old value; true when anything changed
    pub fn merge(&mut self, v: &Value) -> bool {
        let before = self.clone();
        let flag = |key: &str, slot: &mut bool| {
            if let Some(b) = v[key].as_bool() {
                *slot = b;
            }
        };
        flag("fractionMode", &mut self.fraction_mode);
        flag("sigFigMode", &mut self.sig_fig_mode);
        flag("rationalize", &mut self.rationalize);
        flag("keepWords", &mut self.keep_words);
        flag("typstPreview", &mut self.typst_preview);
        flag("typstCopy", &mut self.typst_copy);
        flag("spaceThousands", &mut self.space_thousands);
        flag("copyUnitless", &mut self.copy_unitless);
        flag("shareUsage", &mut self.share_usage);
        flag("exponentMode", &mut self.exponent_mode);
        if let Some(s) = v["angleMode"].as_str()
            && (s == "deg" || s == "rad")
        {
            self.angle_mode = s.into();
        }
        if let Some(s) = v["answerForm"].as_str()
            && (s == "exact" || s == "approx")
        {
            self.answer_form = s.into();
        }
        if !v["historyInsert"].is_null() {
            self.history_insert = pick(&v["historyInsert"], &["answer"], "expr");
        }
        if !v["historyShow"].is_null() {
            self.history_show = pick(&v["historyShow"], &["always", "arrow"], "recent");
        }
        if !v["theme"].is_null() {
            self.theme = pick(&v["theme"], &["light", "dark"], "system");
        }
        if let Some(n) = v["sigFigs"].as_f64() {
            self.sig_figs = n.round().clamp(MIN_SIG_FIGS, MAX_SIG_FIGS) as i64;
        }
        if let Some(n) = v["draftSeconds"].as_f64() {
            self.draft_seconds = if n < 0.0 {
                DEFAULT_DRAFT_SECONDS
            } else {
                n.round().min(MAX_DRAFT_SECONDS) as i64
            };
        }
        if let Some(units) = v["defaultUnits"].as_object() {
            self.default_units = units
                .iter()
                .filter_map(|(dim, id)| {
                    id.as_str()
                        .filter(|id| !id.is_empty())
                        .map(|id| (dim.clone(), id.to_string()))
                })
                .collect();
        }
        if let Some(binds) = v["keybinds"].as_object() {
            self.keybinds = binds
                .iter()
                .filter(|(action, _)| KEY_ACTIONS.contains(&action.as_str()))
                .filter_map(|(action, chord)| {
                    chord.as_str().map(|c| (action.clone(), c.to_lowercase()))
                })
                .collect();
        }
        *self != before
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Onboarding {
    pub opens: u64,
    pub commits: u64,
    pub hints: u64,
    pub done: bool,
}

fn count(v: &Value) -> u64 {
    v.as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .map_or(0, |n| n.floor() as u64)
}

impl Onboarding {
    // progress only moves forward, whichever side reports it
    pub fn merge(&mut self, v: &Value) -> bool {
        let next = Onboarding {
            opens: self.opens.max(count(&v["opens"])),
            commits: self.commits.max(count(&v["commits"])),
            hints: self.hints | count(&v["hints"]),
            done: self.done || v["done"] == Value::Bool(true) || v["done"].as_f64() == Some(1.0),
        };
        let changed = next != *self;
        *self = next;
        changed
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub settings: Settings,
    // the show / hide chord the user picked; what actually got registered is kept by the host
    pub hotkey: String,
    pub onboarding: Onboarding,
    pub first_run_done: bool,
    // per monitor: the composer's top-left from the work area's top-left, in logical pixels
    pub positions: BTreeMap<String, [f64; 2]>,
    // the spots above and the width below go once the bar has been out of use this long; 0 never
    pub spot_seconds: i64,
    // when the bar was last put away, unix seconds
    pub used_at: u64,
    // the bar's width in logical pixels, dragged wider from either side edge; 0 is the standard width
    pub width: f64,
    // the page's usage counts (lib/analytics), kept as it sent them; null when sharing is off
    pub analytics: Value,
    // which defaults the settings above were last brought up to (DEFAULTS)
    pub defaults: u64,
    // updates check, download and install by themselves; off, only when settings asks
    pub auto_update: bool,
    // the Tauri app's own record of start-at-sign-in (src-tauri store.rs signIn), kept as it was so going back
    // to the Tauri app doesn't lose it; this app reads the Run entry itself (autostart.rs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<bool>,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            settings: Settings::default(),
            hotkey: String::new(),
            onboarding: Onboarding::default(),
            first_run_done: false,
            positions: BTreeMap::new(),
            spot_seconds: DEFAULT_SPOT_SECONDS,
            used_at: 0,
            width: 0.0,
            analytics: Value::Null,
            defaults: DEFAULTS,
            auto_update: true,
            sign_in: None,
        }
    }
}

impl Store {
    // something the "move the bar back after" setting would put back
    pub fn moved_or_widened(&self) -> bool {
        !self.positions.is_empty() || self.width > crate::place::MIN_WIDTH
    }

    // field by field, so one bad value can't reset the rest
    pub fn from_json(text: &str) -> Store {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let mut store = Store::default();
        store.settings.merge(&v["settings"]);
        store.onboarding.merge(&v["onboarding"]);
        store.hotkey = v["hotkey"].as_str().unwrap_or_default().to_string();
        store.first_run_done = v["firstRunDone"].as_bool().unwrap_or(false);
        store.analytics = v["analytics"].clone();
        store.spot_seconds = spot_seconds(&v["spotSeconds"]);
        store.used_at = v["usedAt"].as_u64().unwrap_or(0);
        store.width = v["width"]
            .as_f64()
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(0.0);
        store.auto_update = v["autoUpdate"].as_bool().unwrap_or(true);
        store.sign_in = v["signIn"].as_bool();
        if v["settings"].is_object() && v["defaults"].as_u64().unwrap_or(1) < 2 {
            if store.settings.sig_figs == 12 {
                store.settings.sig_figs = DEFAULT_SIG_FIGS;
            }
            store.settings.typst_preview = true;
        }
        if let Some(all) = v["positions"].as_object() {
            for (key, pair) in all {
                if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                    store.positions.insert(key.clone(), [x, y]);
                }
            }
        }
        store
    }

    pub fn load(path: &Path) -> Store {
        fs::read_to_string(path)
            .map(|text| Store::from_json(&text))
            .unwrap_or_default()
    }

    // written beside and renamed over, so a crash mid-write leaves the old file
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }

    // true exactly once per install
    pub fn claim_first_run(&mut self) -> bool {
        !std::mem::replace(&mut self.first_run_done, true)
    }
}

/// `%APPDATA%\com.maxconine.qcalc`: the Tauri app's config dir (its `app_config_dir`), so the files carry over.
/// `QCALC_DATA` points it elsewhere for tests and the bench.
pub fn config_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("QCALC_DATA") {
        return Some(std::path::PathBuf::from(dir));
    }
    std::env::var_os("APPDATA").map(|d| std::path::PathBuf::from(d).join("com.maxconine.qcalc"))
}

pub fn settings_path() -> Option<std::path::PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

/// The unfinished line kept across a hide (`draftSeconds`), as the web app's `draft.ts` keeps it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub expr: String,
    /// unix ms
    pub saved_at: i64,
}

impl Draft {
    pub fn path() -> Option<std::path::PathBuf> {
        config_dir().map(|d| d.join("draft.json"))
    }

    pub fn load() -> Option<Draft> {
        let text = fs::read_to_string(Self::path()?).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The draft if it is younger than `seconds` (0 never keeps one).
    pub fn fresh(&self, now_ms: i64, seconds: i64) -> bool {
        seconds > 0
            && !self.expr.trim().is_empty()
            && (0..=seconds * 1000).contains(&(now_ms - self.saved_at))
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| io::Error::other("no config dir"))?;
        write_atomic(
            &path,
            &serde_json::to_string(self).map_err(io::Error::other)?,
        )
    }

    pub fn clear() {
        if let Some(p) = Self::path() {
            let _ = fs::remove_file(p);
        }
    }
}

/// Written beside and renamed over, so a crash mid-write leaves the old file.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_older_file_takes_the_new_defaults_once() {
        // written before 6 figures and the preview were the defaults: its 12 and its off are the old defaults
        let old = Store::from_json(
            r#"{ "settings": { "sigFigs": 12, "typstPreview": false, "theme": "dark" } }"#,
        );
        assert_eq!(
            (
                old.settings.sig_figs,
                old.settings.typst_preview,
                old.defaults
            ),
            (6, true, DEFAULTS)
        );
        assert_eq!(old.settings.theme, "dark");
        // a value someone picked that was never a default stays
        assert_eq!(
            Store::from_json(r#"{ "settings": { "sigFigs": 9 } }"#)
                .settings
                .sig_figs,
            9
        );
        // once brought up to date, a 12 or an off preview is a choice
        let chosen = Store::from_json(
            r#"{ "defaults": 2, "settings": { "sigFigs": 12, "typstPreview": false } }"#,
        );
        assert_eq!(
            (chosen.settings.sig_figs, chosen.settings.typst_preview),
            (12, false)
        );
    }

    #[test]
    fn defaults_match_the_page() {
        let v = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            v,
            json!({
                "angleMode": "deg", "fractionMode": false, "sigFigMode": false, "rationalize": true,
                "keepWords": false, "answerForm": "exact", "historyInsert": "expr", "historyShow": "recent",
                "sigFigs": 6, "draftSeconds": 60, "defaultUnits": {}, "theme": "system",
                "typstPreview": true, "typstCopy": false, "spaceThousands": false, "copyUnitless": true, "shareUsage": true, "keybinds": {},
                "exponentMode": false
            })
        );
    }

    #[test]
    fn keybinds_keep_known_actions_and_empty_keys() {
        let mut s = Settings::default();
        assert!(s.merge(&json!({ "keybinds": { "angle": "Ctrl+Alt+R", "clear": "", "show": "ctrl+k", "nope": "ctrl+x", "fraction": 3 } })));
        assert_eq!(
            s.keybinds,
            BTreeMap::from([
                ("angle".to_string(), "ctrl+alt+r".to_string()),
                ("clear".to_string(), String::new())
            ])
        );
        assert!(!s.merge(&json!({ "theme": "system" })));
        assert!(s.merge(&json!({ "keybinds": {} })));
        assert!(s.keybinds.is_empty());
    }

    #[test]
    fn merge_takes_valid_fields_and_keeps_the_rest() {
        let mut s = Settings::default();
        assert!(s.merge(&json!({ "angleMode": "rad", "fractionMode": true, "theme": "dark", "defaultUnits": { "length": "ft", "mass": "" } })));
        assert_eq!(s.angle_mode, "rad");
        assert!(s.fraction_mode);
        assert_eq!(s.theme, "dark");
        assert_eq!(
            s.default_units,
            BTreeMap::from([("length".to_string(), "ft".to_string())])
        );
        assert!(s.rationalize);
        assert!(!s.merge(&json!({ "angleMode": "grad", "fractionMode": "yes" })));
        assert_eq!(s.angle_mode, "rad");
    }

    #[test]
    fn merge_normalizes_like_the_page() {
        let mut s = Settings::default();
        s.merge(&json!({ "theme": "sepia", "historyShow": "never", "historyInsert": "row", "answerForm": "rough" }));
        assert_eq!(
            (
                s.theme.as_str(),
                s.history_show.as_str(),
                s.history_insert.as_str()
            ),
            ("system", "recent", "expr")
        );
        assert_eq!(s.answer_form, "exact");
        s.merge(&json!({ "sigFigs": 40, "draftSeconds": 99999 }));
        assert_eq!((s.sig_figs, s.draft_seconds), (16, 3600));
        s.merge(&json!({ "sigFigs": 1, "draftSeconds": -5 }));
        assert_eq!((s.sig_figs, s.draft_seconds), (2, 60));
        s.merge(&json!({ "sigFigs": 7.6, "draftSeconds": 0 }));
        assert_eq!((s.sig_figs, s.draft_seconds), (8, 0));
    }

    #[test]
    fn onboarding_only_moves_forward() {
        let mut o = Onboarding {
            opens: 4,
            commits: 2,
            hints: 16,
            done: false,
        };
        assert!(o.merge(&json!({ "opens": 3, "commits": 5, "hints": 32, "done": false })));
        assert_eq!(
            o,
            Onboarding {
                opens: 4,
                commits: 5,
                hints: 48,
                done: false
            }
        );
        assert!(o.merge(&json!({ "done": 1 })));
        assert!(o.done);
        assert!(!o.merge(&json!({ "opens": -2, "hints": "lots", "done": false })));
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("qcalc-store-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut store = Store::default();
        store
            .settings
            .merge(&json!({ "sigFigs": 9, "theme": "light", "defaultUnits": { "length": "m" } }));
        store.onboarding.merge(&json!({ "opens": 3, "hints": 512 }));
        store.hotkey = "ctrl+shift+k".into();
        store
            .settings
            .merge(&json!({ "keybinds": { "angle": "ctrl+alt+r" } }));
        store
            .positions
            .insert("\\\\.\\DISPLAY2".into(), [120.5, 300.0]);
        (store.spot_seconds, store.used_at, store.width) = (300, 1_790_000_000, 912.0);
        assert!(store.claim_first_run());
        assert!(!store.claim_first_run());
        store.save(&path).unwrap();
        assert_eq!(Store::load(&path), store);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_keeps_what_it_can() {
        let store = Store::from_json(
            r#"{ "settings": { "sigFigs": "many", "theme": "dark" }, "positions": { "a": [1, 2], "b": "x" }, "firstRunDone": 1 }"#,
        );
        assert_eq!(store.settings.sig_figs, DEFAULT_SIG_FIGS);
        assert_eq!(store.settings.theme, "dark");
        assert_eq!(store.positions.len(), 1);
        assert!(!store.first_run_done);
        assert_eq!(Store::from_json("not json"), Store::default());
    }

    #[test]
    fn a_moved_bar_lasts_five_minutes_unless_the_file_says_otherwise() {
        let fresh = Store::from_json("{}");
        assert_eq!(
            (fresh.spot_seconds, fresh.used_at, fresh.width),
            (DEFAULT_SPOT_SECONDS, 0, 0.0)
        );
        // 0 is never, which a missing or broken value mustn't turn into
        assert_eq!(Store::from_json(r#"{ "spotSeconds": 0 }"#).spot_seconds, 0);
        assert_eq!(
            Store::from_json(r#"{ "spotSeconds": -3, "width": -200, "usedAt": "then" }"#),
            Store::default()
        );
        assert_eq!(spot_seconds(&json!(1e9)), 86_400);
        assert_eq!(spot_seconds(&json!("5")), DEFAULT_SPOT_SECONDS);
        assert_eq!(
            Store::load(Path::new("/nonexistent/qcalc/settings.json")),
            Store::default()
        );
    }

    #[test]
    fn a_draft_lasts_as_long_as_the_setting() {
        let d = Draft {
            expr: "2+".into(),
            saved_at: 1_000_000,
        };
        assert!(d.fresh(1_030_000, 60));
        assert!(!d.fresh(1_061_000, 60));
        assert!(!d.fresh(1_000_001, 0));
        assert!(
            !Draft {
                expr: "  ".into(),
                saved_at: 1_000_000
            }
            .fresh(1_000_001, 60)
        );
    }

    #[test]
    fn a_wider_bar_counts_as_moved() {
        let mut store = Store::default();
        assert!(!store.moved_or_widened());
        store.width = crate::place::MIN_WIDTH;
        assert!(!store.moved_or_widened());
        store.width = 900.0;
        assert!(store.moved_or_widened());
        store.width = 0.0;
        store.positions.insert("a".into(), [1.0, 2.0]);
        assert!(store.moved_or_widened());
    }
}
