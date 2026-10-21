// the show / hide shortcut: any chord the user records, spelled like the page's keybinds ("ctrl+alt+space")
use std::str::FromStr;

use tauri_plugin_global_shortcut::Shortcut;

pub const STANDARD: &str = "alt+space";

// what 2.0 stored: one of four preset ids
const PRESET_IDS: [(&str, &str); 4] = [
    ("alt-space", "alt+space"),
    ("ctrl-alt-space", "ctrl+alt+space"),
    ("ctrl-space", "ctrl+space"),
    ("ctrl-shift-space", "ctrl+shift+space"),
];

const MODIFIERS: [&str; 4] = ["ctrl", "alt", "shift", "cmd"];

// modifiers first, in the page's order, so the same chord is always the same string
fn normalize(chord: &str) -> Option<String> {
    let lower = chord.to_lowercase();
    let mut parts: Vec<&str> = lower.split('+').collect();
    let key = parts.pop().filter(|k| !k.is_empty() && !MODIFIERS.contains(k))?;
    if !parts.iter().all(|p| MODIFIERS.contains(p)) {
        return None;
    }
    let mut out: Vec<&str> = MODIFIERS.iter().copied().filter(|m| parts.contains(m)).collect();
    out.push(key);
    Some(out.join("+"))
}

// the plugin's parser reads the page's key names as they are; cmd is the windows key
pub fn shortcut(chord: &str) -> Option<Shortcut> {
    let text = chord.split('+').map(|p| if p == "cmd" { "super" } else { p }).collect::<Vec<_>>().join("+");
    Shortcut::from_str(&text).ok()
}

// a stored preset id or chord, as a chord that registers; anything else is the standard
pub fn named(raw: &str) -> String {
    let chord = PRESET_IDS.iter().find(|(id, _)| *id == raw).map_or(raw, |(_, c)| c);
    normalize(chord).filter(|c| shortcut(c).is_some()).unwrap_or_else(|| STANDARD.to_string())
}

// Ctrl+Alt+Space, the way windows writes keys
pub fn title(chord: &str) -> String {
    chord
        .split('+')
        .map(|p| match p {
            "ctrl" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "cmd" => "Win".to_string(),
            "space" => "Space".to_string(),
            "backspace" => "Backspace".to_string(),
            "delete" => "Delete".to_string(),
            "enter" => "Enter".to_string(),
            "tab" => "Tab".to_string(),
            "escape" => "Esc".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "home" => "Home".to_string(),
            "end" => "End".to_string(),
            "pageup" => "PgUp".to_string(),
            "pagedown" => "PgDn".to_string(),
            key => key.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[derive(Clone, Debug, PartialEq)]
pub struct HotKey {
    pub active: Option<String>,
    pub failed: bool,
}

// at launch: the picked chord, else the default, else none, and the page's hint line says so
pub fn launch(preferred: &str, mut bind: impl FnMut(&str) -> bool) -> HotKey {
    if bind(preferred) {
        return HotKey { active: Some(preferred.to_string()), failed: false };
    }
    if preferred != STANDARD && bind(STANDARD) {
        return HotKey { active: Some(STANDARD.to_string()), failed: true };
    }
    HotKey { active: None, failed: true }
}

// false when another app owns `next`; the previous one stays bound
pub fn select(next: &str, now: &HotKey, mut bind: impl FnMut(&str) -> bool, unbind: impl FnOnce()) -> (HotKey, bool) {
    if now.active.as_deref() == Some(next) {
        return (HotKey { active: Some(next.to_string()), failed: false }, true);
    }
    unbind();
    if bind(next) {
        return (HotKey { active: Some(next.to_string()), failed: false }, true);
    }
    match &now.active {
        Some(previous) if bind(previous) => (now.clone(), false),
        _ => (HotKey { active: None, failed: true }, false),
    }
}

// what the settings page names as taken: the chord just refused, else the one that failed at launch
pub fn taken(refused: Option<&str>, now: &HotKey, wanted: &str) -> String {
    match refused {
        Some(chord) => chord.to_string(),
        None if now.failed && now.active.as_deref() != Some(wanted) => wanted.to_string(),
        None => String::new(),
    }
}

// a windows virtual key as the page names it; the keys a hotkey can't be (numpad, media, ...) are none
pub fn vk_name(vk: u32) -> Option<String> {
    let name = match vk {
        0x30..=0x39 | 0x41..=0x5A => return Some(char::from(vk as u8).to_ascii_lowercase().to_string()),
        0x70..=0x87 => return Some(format!("f{}", vk - 0x6F)),
        0x08 => "backspace",
        0x09 => "tab",
        0x0D => "enter",
        0x1B => "escape",
        0x20 => "space",
        0x21 => "pageup",
        0x22 => "pagedown",
        0x23 => "end",
        0x24 => "home",
        0x25 => "left",
        0x26 => "up",
        0x27 => "right",
        0x28 => "down",
        0x2E => "delete",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => return None,
    };
    Some(name.to_string())
}

fn modifier(vk: u32) -> Option<&'static str> {
    match vk {
        0x11 | 0xA2 | 0xA3 => Some("ctrl"),
        0x12 | 0xA4 | 0xA5 => Some("alt"),
        0x10 | 0xA0 | 0xA1 => Some("shift"),
        0x5B | 0x5C => Some("cmd"),
        _ => None,
    }
}

// what the recorder's hook tells the settings page
#[derive(Clone, Debug, PartialEq)]
pub enum Heard {
    // the modifiers down so far, "ctrl+alt", so the row shows what it hears before the key
    Held(String),
    Chord(String),
    // a key no shortcut can use
    Unknown,
}

// the keyboard as the recorder's hook sees it: modifiers down, and the presses it kept from windows, so their
// releases are kept too and nothing is left half pressed
#[derive(Default)]
pub struct Keys {
    held: Vec<u32>,
    kept: Vec<u32>,
}

impl Keys {
    fn mods(&self) -> Vec<&'static str> {
        let down: Vec<&str> = self.held.iter().filter_map(|vk| modifier(*vk)).collect();
        MODIFIERS.iter().copied().filter(|m| down.contains(m)).collect()
    }

    // true to keep the press from windows and every other app. only while the settings window is in front, and
    // only what the page can't hear itself: alt and win (the window menu, the start menu) and chords with ctrl, alt
    // or win, which another app's hotkey or the shell would take first. plain keys and esc stay the page's
    pub fn key(&mut self, vk: u32, down: bool, focused: bool) -> (bool, Option<Heard>) {
        if let Some(m) = modifier(vk) {
            let before = self.mods();
            let keep = if down { focused && (m == "alt" || m == "cmd") } else { self.kept.contains(&vk) };
            if down {
                if !self.held.contains(&vk) {
                    self.held.push(vk);
                }
                if keep && !self.kept.contains(&vk) {
                    self.kept.push(vk);
                }
            } else {
                self.held.retain(|k| *k != vk);
                self.kept.retain(|k| *k != vk);
            }
            let after = self.mods();
            return (keep, (focused && after != before).then(|| Heard::Held(after.join("+"))));
        }
        if !down {
            let kept = self.kept.contains(&vk);
            self.kept.retain(|k| *k != vk);
            return (kept, None);
        }
        // a held key repeats
        if self.kept.contains(&vk) {
            return (true, None);
        }
        let mods = self.mods();
        let fkey = (0x70..=0x87).contains(&vk);
        if !focused || !(fkey || mods.iter().any(|m| *m != "shift")) {
            return (false, None);
        }
        self.kept.push(vk);
        let heard = match vk_name(vk) {
            Some(key) => Heard::Chord(mods.into_iter().chain([key.as_str()]).collect::<Vec<_>>().join("+")),
            None => Heard::Unknown,
        };
        (true, Some(heard))
    }
}

// launchers known to take a chord like q calc's, with their own default. windows has no call that names the
// process holding a hotkey, so a running launcher whose shortcut matches is only a likely owner
pub struct Launcher {
    pub id: &'static str,
    pub exes: &'static [&'static str],
    pub chord: &'static str,
}

pub const LAUNCHERS: [Launcher; 6] = [
    Launcher { id: "powertoys-run", exes: &["powertoys.exe", "powertoys.powerlauncher.exe"], chord: "alt+space" },
    Launcher { id: "command-palette", exes: &["microsoft.cmdpal.ui.exe"], chord: "alt+cmd+space" },
    Launcher { id: "flow-launcher", exes: &["flow.launcher.exe"], chord: "alt+space" },
    Launcher { id: "wox", exes: &["wox.exe"], chord: "alt+space" },
    Launcher { id: "raycast", exes: &["raycast.exe"], chord: "alt+space" },
    Launcher { id: "keypirinha", exes: &["keypirinha.exe", "keypirinha-x64.exe", "keypirinha-x86.exe"], chord: "ctrl+cmd+k" },
];

// what a launcher's own settings file says
#[derive(Clone, Debug, PartialEq)]
pub enum Setting {
    Off,
    Chord(String),
    // no file, or one this can't read: its default is a guess
    Unknown,
}

// a launcher that's running, and its shortcut when its settings name one
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub id: &'static str,
    pub chord: Option<String>,
}

pub fn launchers(exes: &[String], setting: impl Fn(&'static str) -> Setting) -> Vec<Running> {
    LAUNCHERS
        .iter()
        .filter(|l| exes.iter().any(|e| l.exes.contains(&e.to_lowercase().as_str())))
        .filter_map(|l| match setting(l.id) {
            Setting::Off => None,
            Setting::Chord(c) => Some(Running { id: l.id, chord: Some(c) }),
            Setting::Unknown => Some(Running { id: l.id, chord: None }),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Owner {
    pub id: &'static str,
    // its settings name the chord; otherwise only its default does, or it's windows' own guess
    pub sure: bool,
}

pub fn guess(chord: &str, running: &[Running]) -> Option<Owner> {
    if let Some(r) = running.iter().find(|r| r.chord.as_deref() == Some(chord)) {
        return Some(Owner { id: r.id, sure: true });
    }
    let default = |id: &str| LAUNCHERS.iter().find(|l| l.id == id).map(|l| l.chord);
    if let Some(r) = running.iter().find(|r| r.chord.is_none() && default(r.id) == Some(chord)) {
        return Some(Owner { id: r.id, sure: false });
    }
    // the shell keeps most win chords for itself
    chord.split('+').any(|p| p == "cmd").then_some(Owner { id: "windows", sure: false })
}

// powertoys run's activation shortcut, from its settings.json
pub fn powertoys_run_chord(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let key = &v["properties"]["open_powerlauncher"];
    let name = vk_name(u32::try_from(key["code"].as_u64()?).ok()?)?;
    let held = |k: &str| key[k].as_bool() == Some(true);
    let mods = [("ctrl", held("ctrl")), ("alt", held("alt")), ("shift", held("shift")), ("cmd", held("win"))];
    let parts: Vec<&str> = mods.iter().filter(|(_, on)| *on).map(|(m, _)| *m).chain([name.as_str()]).collect();
    normalize(&parts.join("+"))
}

// false only when powertoys' own settings turn powertoys run off
pub fn powertoys_run_enabled(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json).map_or(true, |v| v["enabled"]["PowerToys Run"].as_bool() != Some(false))
}

// flow launcher and wox write theirs as words: "Alt + Space"
pub fn words_chord(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let text = v["Hotkey"].as_str()?.to_lowercase();
    let parts: Vec<&str> = text
        .split('+')
        .map(|p| match p.trim() {
            "control" => "ctrl",
            "win" | "windows" | "super" => "cmd",
            "esc" => "escape",
            "back" => "backspace",
            "return" => "enter",
            other => other,
        })
        .collect();
    normalize(&parts.join("+"))
}

// what the settings page hears back when it tries a chord before keeping it
#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    pub chord: String,
    // false when no hotkey can be this chord
    pub usable: bool,
    pub free: bool,
    pub owner: Option<Owner>,
    // a chord that is free right now, offered instead
    pub suggest: Option<String>,
}

const SUGGESTIONS: [&str; 5] = ["ctrl+alt+space", "alt+q", "ctrl+alt+q", "ctrl+shift+space", "ctrl+alt+k"];

// bind registers and lets go again; the active chord is q calc's own, so it counts as free
pub fn probe(
    raw: &str,
    active: Option<&str>,
    mut bind: impl FnMut(&str) -> bool,
    owner: impl FnOnce(&str) -> Option<Owner>,
) -> Verdict {
    let chord = normalize(raw).filter(|c| shortcut(c).is_some());
    let name = chord.clone().unwrap_or_else(|| raw.to_lowercase());
    let usable = chord.is_some();
    if chord.as_deref().is_some_and(|c| active == Some(c) || bind(c)) {
        return Verdict { chord: name, usable, free: true, owner: None, suggest: None };
    }
    let suggest =
        SUGGESTIONS.iter().find(|c| **c != name && active != Some(**c) && bind(c)).map(|c| c.to_string());
    let owner = if usable { owner(&name) } else { None };
    Verdict { chord: name, usable, free: false, owner, suggest }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_preset_ids_and_bad_chords() {
        assert_eq!(named("ctrl-space"), "ctrl+space");
        assert_eq!(named("ctrl-shift-space"), "ctrl+shift+space");
        assert_eq!(named("cmd-opt-space"), STANDARD);
        assert_eq!(named(""), STANDARD);
        assert_eq!(named("ctrl+"), STANDARD);
        assert_eq!(named("SHIFT+CTRL+K"), "ctrl+shift+k");
        assert_eq!(named("alt+cmd+f5"), "alt+cmd+f5");
    }

    #[test]
    fn every_page_key_name_parses() {
        for key in [
            "a", "z", "0", "9", ",", ".", "/", "\\", ";", "'", "[", "]", "-", "=", "`", "space", "backspace", "delete",
            "enter", "tab", "escape", "up", "down", "left", "right", "home", "end", "pageup", "pagedown", "f1", "f12",
        ] {
            assert!(shortcut(&format!("ctrl+alt+{key}")).is_some(), "{key}");
        }
        assert!(shortcut("cmd+shift+k").is_some());
    }

    #[test]
    fn titles_read_like_windows() {
        assert_eq!(title("ctrl+alt+space"), "Ctrl+Alt+Space");
        assert_eq!(title("ctrl+shift+backspace"), "Ctrl+Shift+Backspace");
        assert_eq!(title("alt+cmd+k"), "Alt+Win+K");
        assert_eq!(title("ctrl+,"), "Ctrl+,");
    }

    #[test]
    fn launch_falls_back_and_says_so() {
        let on = |c: &str| HotKey { active: Some(c.to_string()), failed: false };
        assert_eq!(launch("ctrl+space", |_| true), on("ctrl+space"));
        assert_eq!(launch("ctrl+space", |c| c == STANDARD), HotKey { active: Some(STANDARD.into()), failed: true });
        assert_eq!(launch(STANDARD, |_| false), HotKey { active: None, failed: true });
        let mut tried = vec![];
        launch(STANDARD, |c| {
            tried.push(c.to_string());
            false
        });
        assert_eq!(tried, [STANDARD]);
    }

    #[test]
    fn select_keeps_the_old_key_when_the_new_one_is_taken() {
        let now = HotKey { active: Some("alt+space".into()), failed: false };
        let next = HotKey { active: Some("ctrl+k".into()), failed: false };
        assert_eq!(select("ctrl+k", &now, |_| true, || ()), (next, true));
        assert_eq!(select("ctrl+k", &now, |c| c == "alt+space", || ()), (now.clone(), false));
        assert_eq!(select("ctrl+k", &now, |_| false, || ()), (HotKey { active: None, failed: true }, false));
        let none = HotKey { active: None, failed: true };
        assert_eq!(select("ctrl+k", &none, |_| false, || ()), (none, false));
    }

    #[test]
    fn reselecting_the_active_key_clears_a_failure_without_rebinding() {
        let now = HotKey { active: Some("alt+space".into()), failed: true };
        let mut unbound = false;
        let out = select("alt+space", &now, |_| panic!("no rebind"), || unbound = true);
        assert_eq!(out, (HotKey { active: Some("alt+space".into()), failed: false }, true));
        assert!(!unbound);
    }

    #[test]
    fn taken_names_the_refused_or_the_failed_chord() {
        let fine = HotKey { active: Some("ctrl+k".into()), failed: false };
        assert_eq!(taken(None, &fine, "ctrl+k"), "");
        assert_eq!(taken(Some("alt+space"), &fine, "ctrl+k"), "alt+space");
        let fell_back = HotKey { active: Some(STANDARD.into()), failed: true };
        assert_eq!(taken(None, &fell_back, "ctrl+k"), "ctrl+k");
        assert_eq!(taken(None, &HotKey { active: None, failed: true }, STANDARD), STANDARD);
    }

    #[test]
    fn virtual_keys_read_like_the_page_and_parse() {
        assert_eq!(vk_name(0x41).as_deref(), Some("a"));
        assert_eq!(vk_name(0x39).as_deref(), Some("9"));
        assert_eq!(vk_name(0x20).as_deref(), Some("space"));
        assert_eq!(vk_name(0x7B).as_deref(), Some("f12"));
        assert_eq!(vk_name(0xBC).as_deref(), Some(","));
        assert_eq!(vk_name(0x6B), None);
        for key in (0..=0xFF).filter_map(vk_name) {
            assert!(shortcut(&format!("ctrl+alt+{key}")).is_some(), "{key}");
        }
    }

    // left alt, right alt, left win, left ctrl, left shift
    const LALT: u32 = 0xA4;
    const LWIN: u32 = 0x5B;
    const LCTRL: u32 = 0xA2;
    const LSHIFT: u32 = 0xA0;
    const SPACE: u32 = 0x20;

    #[test]
    fn the_hook_keeps_alt_space_from_the_window_menu() {
        let mut keys = Keys::default();
        assert_eq!(keys.key(LALT, true, true), (true, Some(Heard::Held("alt".into()))));
        assert_eq!(keys.key(SPACE, true, true), (true, Some(Heard::Chord("alt+space".into()))));
        // the repeat and the release stay kept, and say nothing more
        assert_eq!(keys.key(SPACE, true, true), (true, None));
        assert_eq!(keys.key(SPACE, false, true), (true, None));
        assert_eq!(keys.key(LALT, false, true), (true, Some(Heard::Held("".into()))));
    }

    #[test]
    fn the_hook_hears_win_chords_in_order_and_leaves_plain_keys() {
        let mut keys = Keys::default();
        keys.key(LSHIFT, true, true);
        keys.key(LWIN, true, true);
        assert_eq!(keys.key(LCTRL, true, true), (false, Some(Heard::Held("ctrl+shift+cmd".into()))));
        assert_eq!(keys.key(0x4B, true, true), (true, Some(Heard::Chord("ctrl+shift+cmd+k".into()))));
        let mut keys = Keys::default();
        assert_eq!(keys.key(0x1B, true, true), (false, None));
        keys.key(LSHIFT, true, true);
        assert_eq!(keys.key(0x4B, true, true), (false, None));
        assert_eq!(keys.key(0x74, true, true), (true, Some(Heard::Chord("shift+f5".into()))));
        let mut keys = Keys::default();
        keys.key(LCTRL, true, true);
        assert_eq!(keys.key(0x6B, true, true), (true, Some(Heard::Unknown)));
    }

    #[test]
    fn the_hook_leaves_other_windows_alone() {
        let mut keys = Keys::default();
        assert_eq!(keys.key(LALT, true, false), (false, None));
        assert_eq!(keys.key(SPACE, true, false), (false, None));
        assert_eq!(keys.key(SPACE, false, false), (false, None));
        // a release whose press windows saw goes to windows too
        assert_eq!(keys.key(LALT, false, true), (false, Some(Heard::Held("".into()))));
    }

    #[test]
    fn owners_are_sure_only_when_their_settings_say_so() {
        let exes = ["explorer.exe".to_string(), "PowerToys.exe".to_string(), "Flow.Launcher.exe".to_string()];
        let read = |id: &str| match id {
            "powertoys-run" => Setting::Chord("alt+space".into()),
            _ => Setting::Unknown,
        };
        let running = launchers(&exes, read);
        assert_eq!(running.len(), 2);
        assert_eq!(guess("alt+space", &running), Some(Owner { id: "powertoys-run", sure: true }));
        // powertoys run moved off alt+space: flow launcher's default is the guess
        let running = launchers(&exes, |id| if id == "powertoys-run" { Setting::Chord("ctrl+space".into()) } else { Setting::Unknown });
        assert_eq!(guess("alt+space", &running), Some(Owner { id: "flow-launcher", sure: false }));
        assert_eq!(guess("ctrl+space", &running), Some(Owner { id: "powertoys-run", sure: true }));
        let running = launchers(&exes, |_| Setting::Off);
        assert_eq!(guess("alt+space", &running), None);
        assert_eq!(guess("cmd+e", &running), Some(Owner { id: "windows", sure: false }));
        assert_eq!(guess("ctrl+alt+k", &[]), None);
    }

    #[test]
    fn launcher_settings_files() {
        let run = r#"{"properties":{"open_powerlauncher":{"win":false,"ctrl":false,"alt":true,"shift":false,"code":32,"key":"Space"}},"name":"PowerToys Run"}"#;
        assert_eq!(powertoys_run_chord(run).as_deref(), Some("alt+space"));
        let moved = r#"{"properties":{"open_powerlauncher":{"win":true,"ctrl":true,"alt":false,"shift":false,"code":82}}}"#;
        assert_eq!(powertoys_run_chord(moved).as_deref(), Some("ctrl+cmd+r"));
        assert_eq!(powertoys_run_chord("{}"), None);
        assert!(powertoys_run_enabled(r#"{"enabled":{"PowerToys Run":true}}"#));
        assert!(!powertoys_run_enabled(r#"{"enabled":{"PowerToys Run":false}}"#));
        assert!(powertoys_run_enabled("not json"));
        assert_eq!(words_chord(r#"{"Hotkey":"Alt + Space"}"#).as_deref(), Some("alt+space"));
        assert_eq!(words_chord(r#"{"Hotkey":"Win + Ctrl + K"}"#).as_deref(), Some("ctrl+cmd+k"));
        assert_eq!(words_chord(r#"{"Hotkey":""}"#), None);
    }

    #[test]
    fn a_probe_names_the_owner_and_a_free_chord() {
        let held = ["alt+space", "ctrl+alt+space"];
        let free = |c: &str| !held.contains(&c);
        let powertoys = |_: &str| Some(Owner { id: "powertoys-run", sure: true });
        let v = probe("ALT+space", None, free, powertoys);
        assert_eq!(v.chord, "alt+space");
        assert!(v.usable && !v.free);
        assert_eq!(v.owner, Some(Owner { id: "powertoys-run", sure: true }));
        assert_eq!(v.suggest.as_deref(), Some("alt+q"));
        let v = probe("ctrl+k", None, free, |_| panic!("no owner for a free chord"));
        assert_eq!(v, Verdict { chord: "ctrl+k".into(), usable: true, free: true, owner: None, suggest: None });
        // q calc's own active chord isn't taken, and isn't offered
        assert!(probe("alt+space", Some("alt+space"), |_| false, |_| None).free);
        assert_eq!(probe("ctrl+j", Some("alt+q"), |c| c == "alt+q" || c == "ctrl+alt+q", |_| None).suggest.as_deref(), Some("ctrl+alt+q"));
        let v = probe("ctrl+", None, |_| true, |_| None);
        assert!(!v.usable && !v.free);
    }
}
