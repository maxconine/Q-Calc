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
}
