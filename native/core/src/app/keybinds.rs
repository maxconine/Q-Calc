//! The bar's own shortcuts, chosen by the user (`src/lib/keybinds.ts`): chords, their defaults per platform, their
//! labels, why one can't be used, and sanitising what was stored. A chord is `ctrl+alt+shift+cmd+key` in that order,
//! key names from the physical key. Key capture itself stays in Swift; it hands over a [`KeyEventLike`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::jsre::{self, Lazy, js};

/// An action a chord can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyAction {
    /// The host's global shortcut; listed here, registered by the host.
    Show,
    Settings,
    CopyAnswer,
    CopyLine,
    Angle,
    Fraction,
    SigFigs,
    Clear,
}

/// The actions in the settings list's order, with their labels.
pub const KEY_ACTIONS: [(KeyAction, &str); 8] = [
    (KeyAction::Show, "Show / hide Q Calc"),
    (KeyAction::Settings, "Open settings"),
    (KeyAction::CopyAnswer, "Copy answer"),
    (KeyAction::CopyLine, "Copy whole line"),
    (KeyAction::Angle, "Degrees / radians"),
    (KeyAction::Fraction, "Fractions"),
    (KeyAction::SigFigs, "Sig figs from input"),
    (KeyAction::Clear, "Clear history"),
];

impl KeyAction {
    /// The TS id (`copyAnswer`).
    pub fn id(self) -> &'static str {
        match self {
            KeyAction::Show => "show",
            KeyAction::Settings => "settings",
            KeyAction::CopyAnswer => "copyAnswer",
            KeyAction::CopyLine => "copyLine",
            KeyAction::Angle => "angle",
            KeyAction::Fraction => "fraction",
            KeyAction::SigFigs => "sigFigs",
            KeyAction::Clear => "clear",
        }
    }

    pub fn from_id(id: &str) -> Option<KeyAction> {
        KEY_ACTIONS.iter().map(|a| a.0).find(|a| a.id() == id)
    }

    pub fn label(self) -> &'static str {
        KEY_ACTIONS.iter().find(|a| a.0 == self).map_or("", |a| a.1)
    }
}

/// Only the overrides are stored; "" means the action has no key.
pub type Keybinds = BTreeMap<KeyAction, String>;

/// The default chord of an action.
pub fn default_keybind(action: KeyAction, windows: bool) -> &'static str {
    match (action, windows) {
        (KeyAction::Show, false) => "ctrl+alt+space",
        (KeyAction::Show, true) => "alt+space",
        (KeyAction::Settings, false) => "cmd+,",
        (KeyAction::Settings, true) => "ctrl+,",
        (KeyAction::CopyAnswer, false) => "cmd+c",
        (KeyAction::CopyAnswer, true) => "ctrl+c",
        (KeyAction::CopyLine, false) => "shift+cmd+c",
        (KeyAction::CopyLine, true) => "ctrl+shift+c",
        (KeyAction::Angle, _) => "ctrl+d",
        (KeyAction::Fraction, _) => "ctrl+f",
        (KeyAction::SigFigs, _) => "ctrl+s",
        // windows keeps ctrl+c for copy, so clearing moves to ctrl+shift+backspace
        (KeyAction::Clear, false) => "ctrl+c",
        (KeyAction::Clear, true) => "ctrl+shift+backspace",
    }
}

/// The chord an action has: the user's own, or the default.
pub fn keybind_for(binds: &Keybinds, action: KeyAction, windows: bool) -> String {
    binds
        .get(&action)
        .cloned()
        .unwrap_or_else(|| default_keybind(action, windows).to_string())
}

const MODIFIERS: [&str; 4] = ["ctrl", "alt", "shift", "cmd"];

fn code_key(code: &str) -> Option<&'static str> {
    Some(match code {
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        "Space" => "space",
        "Backspace" => "backspace",
        "Delete" => "delete",
        "Enter" | "NumpadEnter" => "enter",
        "Tab" => "tab",
        "Escape" => "escape",
        "ArrowUp" => "up",
        "ArrowDown" => "down",
        "ArrowLeft" => "left",
        "ArrowRight" => "right",
        "Home" => "home",
        "End" => "end",
        "PageUp" => "pageup",
        "PageDown" => "pagedown",
        _ => return None,
    })
}

fn key_name(key: &str) -> Option<&'static str> {
    Some(match key {
        " " => "space",
        "+" => "plus",
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "esc" => "escape",
        "del" => "delete",
        "return" => "enter",
        _ => return None,
    })
}

const MODIFIER_KEYS: [&str; 10] = [
    "shift", "meta", "control", "alt", "altgraph", "capslock", "fn", "os", "hyper", "super",
];

/// A keydown, as the web's `KeyboardEvent` describes it (`key` is what the layout types, `code` the physical key).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyEventLike {
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub alt_key: bool,
    pub shift_key: bool,
    pub key: String,
    pub code: Option<String>,
}

/// A letter is the one the layout types, so ⌃C on dvorak or azerty is c wherever c sits. With ⌥ held the mac types
/// ∂ for d, and a non-latin layout types no latin letter, so those fall back to the physical key.
pub fn event_key_name(e: &KeyEventLike) -> Option<String> {
    static LETTER_CODE: Lazy = Lazy::new(|| js(r"^Key[A-Z]$", ""));
    static LATIN: Lazy = Lazy::new(|| js(r"^[a-z]$", "i"));
    static DIGIT_CODE: Lazy = Lazy::new(|| js(r"^Digit[0-9]$", ""));
    static F_CODE: Lazy = Lazy::new(|| js(r"^F[0-9]{1,2}$", ""));
    let code = e.code.as_deref().unwrap_or("");
    if jsre::test(&LETTER_CODE, code) {
        return Some(if !e.alt_key && jsre::test(&LATIN, &e.key) {
            e.key.to_lowercase()
        } else {
            code[3..].to_lowercase()
        });
    }
    if jsre::test(&DIGIT_CODE, code) {
        return Some(code[5..].to_string());
    }
    if jsre::test(&F_CODE, code) {
        return Some(code.to_lowercase());
    }
    if let Some(k) = code_key(code) {
        return Some(k.into());
    }
    let key = e.key.to_lowercase();
    if key.is_empty() || key == "unidentified" || MODIFIER_KEYS.contains(&key.as_str()) {
        return None;
    }
    Some(key_name(&key).map_or(key.clone(), str::to_string))
}

/// The chord a keydown makes; None while only modifiers are down.
pub fn chord_from_event(e: &KeyEventLike) -> Option<String> {
    let key = event_key_name(e)?;
    let held = [e.ctrl_key, e.alt_key, e.shift_key, e.meta_key];
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .zip(held)
        .filter(|(_, h)| *h)
        .map(|(m, _)| *m)
        .collect();
    parts.push(&key);
    Some(parts.join("+"))
}

fn split_chord(chord: &str) -> (Vec<&str>, &str) {
    let mut parts: Vec<&str> = chord.split('+').collect();
    // "ctrl++" can't happen: + is spelled plus
    let key = parts.pop().unwrap_or("");
    (parts, key)
}

/// Whether text is a chord: a key, after known modifiers only.
pub fn is_chord(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let (mods, key) = split_chord(text);
    !key.is_empty() && !MODIFIERS.contains(&key) && mods.iter().all(|m| MODIFIERS.contains(m))
}

/// The stored order, whatever order it came in.
pub fn normalize_chord(text: &str) -> String {
    let lower = text.to_lowercase();
    let (mods, key) = split_chord(&lower);
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .copied()
        .filter(|m| mods.contains(m))
        .collect();
    parts.push(key);
    parts.join("+")
}

/// The action a keydown does on the page; never `show`, which the host owns.
pub fn action_for_event(e: &KeyEventLike, binds: &Keybinds, windows: bool) -> Option<KeyAction> {
    let chord = chord_from_event(e)?;
    KEY_ACTIONS
        .iter()
        .map(|a| a.0)
        .find(|&id| id != KeyAction::Show && keybind_for(binds, id, windows) == chord)
}

fn mac_glyph(m: &str) -> &'static str {
    match m {
        "ctrl" => "⌃",
        "alt" => "⌥",
        "shift" => "⇧",
        _ => "⌘",
    }
}

fn windows_name(m: &str) -> &'static str {
    match m {
        "ctrl" => "Ctrl",
        "alt" => "Alt",
        "shift" => "Shift",
        _ => "Win",
    }
}

fn key_label(key: &str, windows: bool) -> Option<&'static str> {
    Some(match (key, windows) {
        ("space", _) => "Space",
        ("backspace", false) => "⌫",
        ("backspace", true) => "Backspace",
        ("delete", false) => "⌦",
        ("delete", true) => "Delete",
        ("enter", false) => "↵",
        ("enter", true) => "Enter",
        ("tab", false) => "⇥",
        ("tab", true) => "Tab",
        ("escape", false) => "esc",
        ("escape", true) => "Esc",
        ("up", _) => "↑",
        ("down", _) => "↓",
        ("left", _) => "←",
        ("right", _) => "→",
        ("home", false) => "↖",
        ("home", true) => "Home",
        ("end", false) => "↘",
        ("end", true) => "End",
        ("pageup", false) => "⇞",
        ("pageup", true) => "PgUp",
        ("pagedown", false) => "⇟",
        ("pagedown", true) => "PgDn",
        ("plus", _) => "+",
        _ => return None,
    })
}

/// ⌃⇧C on the mac, Ctrl+Shift+C on windows.
pub fn chord_label(chord: &str, windows: bool) -> String {
    if chord.is_empty() {
        return String::new();
    }
    let (mods, key) = split_chord(chord);
    let key_text = key_label(key, windows).map_or_else(|| key.to_uppercase(), str::to_string);
    // ⌃⌥⇧⌘ is also the mac's own order
    let held: Vec<&str> = MODIFIERS
        .iter()
        .copied()
        .filter(|m| mods.contains(m))
        .collect();
    if windows {
        let mut parts: Vec<String> = held.iter().map(|m| windows_name(m).to_string()).collect();
        parts.push(key_text);
        return parts.join("+");
    }
    format!(
        "{}{key_text}",
        held.iter().map(|m| mac_glyph(m)).collect::<String>()
    )
}

// the text box's own keys, which a binding would take away
fn reserved(chord: &str, windows: bool) -> Option<&'static str> {
    let (mods, key) = split_chord(chord);
    let set: std::collections::BTreeSet<&str> = mods.iter().copied().collect();
    let cmd = if windows { "ctrl" } else { "cmd" };
    let only = |want: &[&str]| set.len() == want.len() && want.iter().all(|m| set.contains(m));
    if only(&[cmd]) {
        match key {
            "v" => return Some("paste"),
            "x" => return Some("cut"),
            "a" => return Some("select all"),
            "z" => return Some("undo"),
            _ => {}
        }
    }
    if only(&[cmd, "shift"]) && key == "z" {
        return Some("redo");
    }
    if set.is_empty()
        && [
            "escape",
            "enter",
            "tab",
            "up",
            "down",
            "left",
            "right",
            "backspace",
            "delete",
            "home",
            "end",
        ]
        .contains(&key)
    {
        return Some("editing");
    }
    None
}

// chords windows keeps for itself: some never reach an app at all (ctrl+alt+delete, win+l), the rest do something of
// windows' own first, so no app can register them
const WINDOWS_KEEPS: [(&str, &str); 31] = [
    ("ctrl+alt+delete", "the security screen"),
    ("ctrl+shift+escape", "Task Manager"),
    ("ctrl+escape", "the Start menu"),
    ("alt+tab", "switching windows"),
    ("alt+shift+tab", "switching windows"),
    ("alt+escape", "switching windows"),
    ("alt+f4", "closing windows"),
    ("cmd+l", "locking the PC"),
    ("cmd+d", "showing the desktop"),
    ("cmd+e", "File Explorer"),
    ("cmd+r", "Run"),
    ("cmd+i", "Settings"),
    ("cmd+a", "quick settings"),
    ("cmd+s", "search"),
    ("cmd+x", "the Start button’s menu"),
    ("cmd+v", "clipboard history"),
    ("cmd+.", "emoji"),
    ("cmd+;", "emoji"),
    ("cmd+space", "switching keyboard layouts"),
    ("shift+cmd+space", "switching keyboard layouts"),
    ("cmd+tab", "Task view"),
    ("cmd+m", "minimizing windows"),
    ("cmd+n", "notifications"),
    ("cmd+h", "voice typing"),
    ("cmd+k", "casting"),
    ("cmd+p", "display modes"),
    ("cmd+u", "accessibility"),
    ("cmd+g", "Game Bar"),
    ("cmd+w", "widgets"),
    ("cmd+,", "peeking at the desktop"),
    ("shift+cmd+s", "screenshots"),
];

/// What windows does with a chord, or None when an app could have it.
pub fn windows_keeps(chord: &str) -> Option<&'static str> {
    static TASKBAR: Lazy = Lazy::new(|| js(r"^(ctrl\+|alt\+|shift\+)*cmd\+[0-9]$", ""));
    static SNAP: Lazy = Lazy::new(|| js(r"^(shift\+)?cmd\+(up|down|left|right)$", ""));
    let c = normalize_chord(chord);
    if let Some((_, what)) = WINDOWS_KEEPS.iter().find(|(k, _)| *k == c) {
        return Some(what);
    }
    // win+1..9 open the taskbar's apps, win+arrows snap windows
    if jsre::test(&TASKBAR, &c) {
        return Some("opening taskbar apps");
    }
    if jsre::test(&SNAP, &c) {
        return Some("snapping windows");
    }
    None
}

/// Windows hands a key another app has as a global shortcut to that app, so the recorder hears only the modifiers
/// go down and up again. What it says then; "" when no modifier was held.
pub fn missed_key_text(mods: &str) -> String {
    let held = held_label(mods, true);
    let label = held.strip_suffix("+…").unwrap_or(&held);
    if label.is_empty() {
        return String::new();
    }
    let them = if label.contains('+') { "them" } else { "it" };
    format!(
        "Only {label} came through. If you pressed a key with {them}, another app has that shortcut (PowerToys Run uses Alt+Space): change it there, or pick another here."
    )
}

/// The modifiers held so far, while the rest of a chord is on its way: "Ctrl+Alt+…", "⌃⌥…".
pub fn held_label(mods: &str, windows: bool) -> String {
    let held: Vec<&str> = mods.split('+').collect();
    let down: Vec<&str> = MODIFIERS
        .iter()
        .copied()
        .filter(|m| held.contains(m))
        .collect();
    if down.is_empty() {
        return String::new();
    }
    if windows {
        format!(
            "{}+…",
            down.iter()
                .map(|m| windows_name(m))
                .collect::<Vec<_>>()
                .join("+")
        )
    } else {
        format!("{}…", down.iter().map(|m| mac_glyph(m)).collect::<String>())
    }
}

/// Why a chord can't be used, or None when it can. Typing keys need ctrl, alt or ⌘ so they don't eat the input.
pub fn chord_problem(
    chord: &str,
    action: KeyAction,
    binds: &Keybinds,
    windows: bool,
) -> Option<String> {
    static FKEY: Lazy = Lazy::new(|| js(r"^f[0-9]{1,2}$", ""));
    if !is_chord(chord) {
        return Some("Press a key".into());
    }
    let (mods, key) = split_chord(chord);
    let fkey = jsre::test(&FKEY, key);
    if !fkey && !mods.contains(&"ctrl") && !mods.contains(&"alt") && !mods.contains(&"cmd") {
        return Some(format!(
            "Add {} so typing still works",
            if windows {
                "Ctrl or Alt"
            } else {
                "⌘, ⌃ or ⌥"
            }
        ));
    }
    if windows && let Some(own) = windows_keeps(chord) {
        return Some(format!(
            "{} is kept by Windows for {own}",
            chord_label(chord, windows)
        ));
    }
    if let Some(kept) = reserved(chord, windows) {
        return Some(format!(
            "{} is kept for {kept}",
            chord_label(chord, windows)
        ));
    }
    for (other, label) in KEY_ACTIONS {
        if other != action && keybind_for(binds, other, windows) == chord {
            return Some(format!(
                "{} already does {}",
                chord_label(chord, windows),
                label.to_lowercase()
            ));
        }
    }
    None
}

/// A default choice is stored as no override, so a later default change still reaches it.
pub fn set_keybind(binds: &Keybinds, action: KeyAction, chord: &str, windows: bool) -> Keybinds {
    let mut next = binds.clone();
    if chord == default_keybind(action, windows) {
        next.remove(&action);
    } else {
        next.insert(action, chord.to_string());
    }
    next
}

/// Stored keybinds read back: known actions but `show`, each a chord (normalised) or "".
pub fn sanitize_keybinds(raw: &Json) -> Keybinds {
    let mut out = Keybinds::new();
    let Some(o) = raw.as_object() else {
        return out;
    };
    for (id, value) in o {
        let Some(action) = KeyAction::from_id(id).filter(|a| *a != KeyAction::Show) else {
            continue;
        };
        let Some(value) = value.as_str() else {
            continue;
        };
        let chord = value.to_lowercase();
        if chord.is_empty() {
            out.insert(action, String::new());
        } else if is_chord(&chord) {
            out.insert(action, normalize_chord(&chord));
        }
    }
    out
}

/// Keybinds as the TS JSON (`{"angle": "ctrl+r"}`).
pub fn keybinds_json(binds: &Keybinds) -> Json {
    Json::Object(
        binds
            .iter()
            .map(|(k, v)| (k.id().to_string(), Json::String(v.clone())))
            .collect(),
    )
}

/// Keybinds read from typed JSON (no sanitising): known action ids with string chords.
pub fn keybinds_from_json(raw: &Json) -> Keybinds {
    raw.as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| Some((KeyAction::from_id(k)?, v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether two sets of overrides are the same.
pub fn keybinds_equal(a: &Keybinds, b: &Keybinds) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords() {
        let e = KeyEventLike {
            ctrl_key: true,
            alt_key: true,
            key: " ".into(),
            code: Some("Space".into()),
            ..KeyEventLike::default()
        };
        assert_eq!(chord_from_event(&e).as_deref(), Some("ctrl+alt+space"));
        assert_eq!(chord_label("shift+cmd+c", false), "⇧⌘C");
        assert_eq!(chord_label("alt+q", true), "Alt+Q");
        assert_eq!(held_label("alt+ctrl", true), "Ctrl+Alt+…");
        assert_eq!(windows_keeps("cmd+3"), Some("opening taskbar apps"));
    }
}
