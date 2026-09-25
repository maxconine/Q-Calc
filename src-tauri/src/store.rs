// settings, onboarding and window spots live here, not in the web view's storage, so a data wipe keeps them
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

const MIN_SIG_FIGS: f64 = 2.0;
const MAX_SIG_FIGS: f64 = 16.0;
const DEFAULT_SIG_FIGS: i64 = 12;
const MAX_DRAFT_SECONDS: f64 = 3600.0;
const DEFAULT_DRAFT_SECONDS: i64 = 60;

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
}

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
            typst_preview: false,
            typst_copy: false,
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
        if let Some(s) = v["angleMode"].as_str() {
            if s == "deg" || s == "rad" {
                self.angle_mode = s.into();
            }
        }
        if let Some(s) = v["answerForm"].as_str() {
            if s == "exact" || s == "approx" {
                self.answer_form = s.into();
            }
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
            self.draft_seconds = if n < 0.0 { DEFAULT_DRAFT_SECONDS } else { n.round().min(MAX_DRAFT_SECONDS) as i64 };
        }
        if let Some(units) = v["defaultUnits"].as_object() {
            self.default_units = units
                .iter()
                .filter_map(|(dim, id)| id.as_str().filter(|id| !id.is_empty()).map(|id| (dim.clone(), id.to_string())))
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
    v.as_f64().filter(|n| n.is_finite() && *n > 0.0).map_or(0, |n| n.floor() as u64)
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

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub settings: Settings,
    // the preset the user picked; what actually got registered is kept by the host
    pub hotkey: String,
    pub onboarding: Onboarding,
    pub first_run_done: bool,
    // per monitor: the composer's top-left from the work area's top-left, in logical pixels
    pub positions: BTreeMap<String, [f64; 2]>,
}

impl Store {
    // field by field, so one bad value can't reset the rest
    pub fn from_json(text: &str) -> Store {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let mut store = Store::default();
        store.settings.merge(&v["settings"]);
        store.onboarding.merge(&v["onboarding"]);
        store.hotkey = v["hotkey"].as_str().unwrap_or_default().to_string();
        store.first_run_done = v["firstRunDone"].as_bool().unwrap_or(false);
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
        fs::read_to_string(path).map(|text| Store::from_json(&text)).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_the_page() {
        let v = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            v,
            json!({
                "angleMode": "deg", "fractionMode": false, "sigFigMode": false, "rationalize": true,
                "keepWords": false, "answerForm": "exact", "historyInsert": "expr", "historyShow": "recent",
                "sigFigs": 12, "draftSeconds": 60, "defaultUnits": {}, "theme": "system",
                "typstPreview": false, "typstCopy": false
            })
        );
    }

    #[test]
    fn merge_takes_valid_fields_and_keeps_the_rest() {
        let mut s = Settings::default();
        assert!(s.merge(&json!({ "angleMode": "rad", "fractionMode": true, "theme": "dark", "defaultUnits": { "length": "ft", "mass": "" } })));
        assert_eq!(s.angle_mode, "rad");
        assert!(s.fraction_mode);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.default_units, BTreeMap::from([("length".to_string(), "ft".to_string())]));
        assert!(s.rationalize);
        assert!(!s.merge(&json!({ "angleMode": "grad", "fractionMode": "yes" })));
        assert_eq!(s.angle_mode, "rad");
    }

    #[test]
    fn merge_normalizes_like_the_page() {
        let mut s = Settings::default();
        s.merge(&json!({ "theme": "sepia", "historyShow": "never", "historyInsert": "row", "answerForm": "rough" }));
        assert_eq!((s.theme.as_str(), s.history_show.as_str(), s.history_insert.as_str()), ("system", "recent", "expr"));
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
        let mut o = Onboarding { opens: 4, commits: 2, hints: 16, done: false };
        assert!(o.merge(&json!({ "opens": 3, "commits": 5, "hints": 32, "done": false })));
        assert_eq!(o, Onboarding { opens: 4, commits: 5, hints: 48, done: false });
        assert!(o.merge(&json!({ "done": 1 })));
        assert!(o.done);
        assert!(!o.merge(&json!({ "opens": -2, "hints": "lots", "done": false })));
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("qcalc-store-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut store = Store::default();
        store.settings.merge(&json!({ "sigFigs": 9, "theme": "light", "defaultUnits": { "length": "m" } }));
        store.onboarding.merge(&json!({ "opens": 3, "hints": 512 }));
        store.hotkey = "ctrl-space".into();
        store.positions.insert("\\\\.\\DISPLAY2".into(), [120.5, 300.0]);
        assert!(store.claim_first_run());
        assert!(!store.claim_first_run());
        store.save(&path).unwrap();
        assert_eq!(Store::load(&path), store);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_keeps_what_it_can() {
        let store = Store::from_json(r#"{ "settings": { "sigFigs": "many", "theme": "dark" }, "positions": { "a": [1, 2], "b": "x" }, "firstRunDone": 1 }"#);
        assert_eq!(store.settings.sig_figs, 12);
        assert_eq!(store.settings.theme, "dark");
        assert_eq!(store.positions.len(), 1);
        assert!(!store.first_run_done);
        assert_eq!(Store::from_json("not json"), Store::default());
        assert_eq!(Store::load(Path::new("/nonexistent/qcalc/settings.json")), Store::default());
    }
}
