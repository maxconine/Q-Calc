//! What the games keep between launches: the page's local-storage saves (`gameSaves`: key → the string the page
//! wrote, only the keys in `GAME_SAVE_KEYS`, each at most `GAME_SAVE_MAX` UTF-16 units, as the Mac's GameSaves
//! and the Tauri page's mirror), and the nickname the room lists show (`nickname.ts`: random once, then kept).
//!
//! They live in `games.json` beside settings.json (`{ "gameSaves": {…}, "nickname": "…" }`) rather than in it, so
//! the bar's settings writer (which writes only the fields it knows) never drops them. Read when a game window
//! opens, written on every change; nothing is held while no game is open.

use std::collections::BTreeMap;
use std::path::PathBuf;

use qcalc_core::games::{nickname, saves};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameStore {
    pub saves: BTreeMap<String, String>,
    pub nickname: String,
    path: Option<PathBuf>,
}

impl GameStore {
    pub fn path() -> Option<PathBuf> {
        crate::store::config_dir().map(|d| d.join("games.json"))
    }

    pub fn load() -> GameStore {
        let path = Self::path();
        let text = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let mut s = GameStore::from_json(&text);
        s.path = path;
        s
    }

    pub fn from_json(text: &str) -> GameStore {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let mut out = GameStore::default();
        if let Some(all) = v["gameSaves"].as_object() {
            for (k, val) in all {
                if let Some(s) = val.as_str()
                    && saves::is_game_save_key(k)
                    && s.encode_utf16().count() <= saves::GAME_SAVE_MAX
                {
                    out.saves.insert(k.clone(), s.to_string());
                }
            }
        }
        out.nickname = nickname::clean_nickname(v["nickname"].as_str().unwrap_or_default());
        out
    }

    pub fn to_json(&self) -> String {
        let saves: serde_json::Map<String, Value> = self
            .saves
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        serde_json::to_string_pretty(&json!({ "gameSaves": saves, "nickname": self.nickname }))
            .unwrap_or_default()
    }

    fn write(&self) {
        if let Some(p) = &self.path
            && let Err(e) = crate::store::write_atomic(p, &self.to_json())
        {
            crate::qlog!("games: couldn't save {}: {e}", p.display());
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.saves.get(key).map(String::as_str)
    }

    /// None removes it. A key the games don't keep, or a value over the size line, isn't kept (as the page's mirror).
    pub fn set(&mut self, key: &str, value: Option<&str>) {
        if !saves::is_game_save_key(key) {
            return;
        }
        match value {
            Some(v) if v.encode_utf16().count() > saves::GAME_SAVE_MAX => return,
            Some(v) => {
                if self.get(key) == Some(v) {
                    return;
                }
                self.saves.insert(key.into(), v.into());
            }
            None => {
                if self.saves.remove(key).is_none() {
                    return;
                }
            }
        }
        self.write();
    }

    /// The name the room lists show: the kept one, or a fresh random one kept from now on.
    pub fn nickname(&mut self, rand: &mut dyn FnMut() -> f64) -> String {
        if self.nickname.is_empty() {
            self.nickname = nickname::random_nickname(rand);
            self.write();
        }
        self.nickname.clone()
    }

    /// A rename: cleaned, and an empty one keeps the old name.
    pub fn rename(&mut self, raw: &str, rand: &mut dyn FnMut() -> f64) -> String {
        let cleaned = nickname::clean_nickname(raw);
        if cleaned.is_empty() {
            return self.nickname(rand);
        }
        self.nickname = cleaned;
        self.write();
        self.nickname.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_games_keys_and_reads_back_what_it_wrote() {
        let mut s = GameStore::from_json(
            r#"{"gameSaves":{"qcalc-crack":"7","nope":"x","qcalc-equal":5},"nickname":"  teal otter "}"#,
        );
        assert_eq!(s.get("qcalc-crack"), Some("7"));
        assert_eq!(s.get("nope"), None);
        assert_eq!(s.get("qcalc-equal"), None);
        assert_eq!(s.nickname, "teal otter");
        s.set("qcalc-c4-ghost", Some("off"));
        s.set("whatever", Some("x"));
        let big = "x".repeat(saves::GAME_SAVE_MAX + 1);
        s.set("qcalc-numbers", Some(&big));
        let back = GameStore::from_json(&s.to_json());
        assert_eq!(back.get("qcalc-c4-ghost"), Some("off"));
        assert_eq!(back.get("whatever"), None);
        assert_eq!(back.get("qcalc-numbers"), None);
        assert_eq!(back.nickname, "teal otter");
    }

    #[test]
    fn a_missing_nickname_is_made_once() {
        let mut s = GameStore::from_json("");
        let mut r = qcalc_core::games::Random::new(7);
        let mut rand = || r.next_f64();
        let a = s.nickname(&mut rand);
        assert!(!a.is_empty());
        assert_eq!(s.nickname(&mut rand), a);
        assert_eq!(s.rename("   ", &mut rand), a);
        assert_eq!(s.rename("Ada", &mut rand), "Ada");
    }
}
