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

    /// The store kept at `path` (tests).
    pub fn at(path: PathBuf) -> GameStore {
        let mut s = GameStore::from_json(&std::fs::read_to_string(&path).unwrap_or_default());
        s.path = Some(path);
        s
    }

    /// A change, made to the file as it is now: under the games' lock, read fresh (another game's process may have
    /// written since this one loaded it), changed, written, and kept. Two games saving at once both keep theirs.
    fn update(&mut self, change: impl FnOnce(&mut GameStore)) {
        let Some(p) = self.path.clone() else {
            change(self);
            return;
        };
        let _lock = Lock::take();
        let mut fresh = GameStore::from_json(&std::fs::read_to_string(&p).unwrap_or_default());
        change(&mut fresh);
        if let Err(e) = crate::store::write_atomic(&p, &fresh.to_json()) {
            crate::qlog!("games: couldn't save {}: {e}", p.display());
        }
        self.saves = fresh.saves;
        self.nickname = fresh.nickname;
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
            Some(v) if v.encode_utf16().count() > saves::GAME_SAVE_MAX => {}
            Some(v) => {
                if self.get(key) != Some(v) {
                    self.update(|s| {
                        s.saves.insert(key.into(), v.into());
                    });
                }
            }
            None => {
                if self.saves.contains_key(key) {
                    self.update(|s| {
                        s.saves.remove(key);
                    });
                }
            }
        }
    }

    /// The name the room lists show: the kept one, or a fresh random one kept from now on.
    pub fn nickname(&mut self, rand: &mut dyn FnMut() -> f64) -> String {
        if self.nickname.is_empty() {
            let made = nickname::random_nickname(rand);
            // another game may have made one meanwhile: that one stays
            self.update(|s| {
                if s.nickname.is_empty() {
                    s.nickname = made;
                }
            });
        }
        self.nickname.clone()
    }

    /// A rename: cleaned, and an empty one keeps the old name.
    pub fn rename(&mut self, raw: &str, rand: &mut dyn FnMut() -> f64) -> String {
        let cleaned = nickname::clean_nickname(raw);
        if cleaned.is_empty() {
            return self.nickname(rand);
        }
        self.update(|s| s.nickname = cleaned);
        self.nickname.clone()
    }
}

/// The games' file lock: a named mutex every Q Calc process (the bar, each game) takes around a write.
struct Lock(Option<windows::Win32::Foundation::HANDLE>);

impl Lock {
    fn take() -> Lock {
        use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
        let name = format!(
            "Local\\com.maxconine.qcalc.games-store{}",
            crate::log::instance_suffix()
        );
        // SAFETY: a named mutex, waited for at most 2 s (a stuck holder never blocks a save for long)
        unsafe {
            match CreateMutexW(None, false, &windows::core::HSTRING::from(name)) {
                Ok(h) => {
                    let _ = WaitForSingleObject(h, 2000);
                    Lock(Some(h))
                }
                Err(_) => Lock(None),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            // SAFETY: our handle, released and closed once
            unsafe {
                let _ = windows::Win32::System::Threading::ReleaseMutex(h);
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
        }
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
    fn two_games_saving_at_once_both_keep_theirs() {
        let dir = std::env::temp_dir().join(format!("qcalc-games-store-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("games.json");
        let _ = std::fs::remove_file(&path);
        // both loaded before either wrote, as two game processes would be
        let mut a = GameStore::at(path.clone());
        let mut b = GameStore::at(path.clone());
        a.set("qcalc-crack", Some("3"));
        b.set("qcalc-ember", Some("{}"));
        let mut r = qcalc_core::games::Random::new(3);
        let name = a.nickname(&mut || r.next_f64());
        // b had no name; the one a made stays
        assert_eq!(b.nickname(&mut || r.next_f64()), name);
        let back = GameStore::at(path.clone());
        assert_eq!(back.get("qcalc-crack"), Some("3"));
        assert_eq!(back.get("qcalc-ember"), Some("{}"));
        assert_eq!(back.nickname, name);
        assert_eq!(b.get("qcalc-crack"), Some("3"));
        let _ = std::fs::remove_dir_all(&dir);
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
