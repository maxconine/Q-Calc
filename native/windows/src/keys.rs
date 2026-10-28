//! The bar's own shortcuts (`src/lib/keybinds.ts`, the core's `app::keybinds`): the user's overrides from
//! `settings.keybinds` over the Windows defaults, matched against a key-down by the core's `action_for_event`.
//! Pure; model.rs asks `Keybinds::action` before the field gets the key. `show` is the global hotkey (hotkey.rs),
//! not routed here.
//!
//! Also here: the cheat sheet's rows (`?` / `help`, the core's `cheat_sheet` reworded for Windows as `hostCheats`
//! does), drawn by cheats.rs, and the hooks other packages fill in (`close_front_sheet`).

use std::collections::BTreeMap;

use qcalc_core::app::keybinds as kb;

use crate::hotkey::vk_name;

/// Windows keeps Ctrl+C for copy (the answer when nothing is selected), so clearing is Ctrl+Shift+Backspace.
pub const WINDOWS_DEFAULTS: [(&str, &str); 7] = [
    ("settings", "ctrl+,"),
    ("copyAnswer", "ctrl+c"),
    ("copyLine", "ctrl+shift+c"),
    ("angle", "ctrl+d"),
    ("fraction", "ctrl+f"),
    ("sigFigs", "ctrl+s"),
    ("clear", "ctrl+shift+backspace"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Settings,
    CopyAnswer,
    CopyLine,
    Angle,
    Fraction,
    SigFigs,
    Clear,
}

impl KeyAction {
    pub fn id(self) -> &'static str {
        self.core().id()
    }

    pub fn of(id: &str) -> Option<KeyAction> {
        kb::KeyAction::from_id(id).and_then(KeyAction::from_core)
    }

    fn core(self) -> kb::KeyAction {
        match self {
            KeyAction::Settings => kb::KeyAction::Settings,
            KeyAction::CopyAnswer => kb::KeyAction::CopyAnswer,
            KeyAction::CopyLine => kb::KeyAction::CopyLine,
            KeyAction::Angle => kb::KeyAction::Angle,
            KeyAction::Fraction => kb::KeyAction::Fraction,
            KeyAction::SigFigs => kb::KeyAction::SigFigs,
            KeyAction::Clear => kb::KeyAction::Clear,
        }
    }

    fn from_core(a: kb::KeyAction) -> Option<KeyAction> {
        Some(match a {
            kb::KeyAction::Show => return None,
            kb::KeyAction::Settings => KeyAction::Settings,
            kb::KeyAction::CopyAnswer => KeyAction::CopyAnswer,
            kb::KeyAction::CopyLine => KeyAction::CopyLine,
            kb::KeyAction::Angle => KeyAction::Angle,
            kb::KeyAction::Fraction => KeyAction::Fraction,
            kb::KeyAction::SigFigs => KeyAction::SigFigs,
            kb::KeyAction::Clear => KeyAction::Clear,
        })
    }
}

/// Modifier state of a key-down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// The web's `KeyboardEvent.code` for a virtual key (the physical key the core names chords by).
fn code_of(vk: u32) -> Option<String> {
    Some(
        match vk {
            0x41..=0x5A => return Some(format!("Key{}", char::from(vk as u8))),
            0x30..=0x39 => return Some(format!("Digit{}", char::from(vk as u8))),
            0x70..=0x87 => return Some(format!("F{}", vk - 0x6F)),
            0x08 => "Backspace",
            0x09 => "Tab",
            0x0D => "Enter",
            0x1B => "Escape",
            0x20 => "Space",
            0x21 => "PageUp",
            0x22 => "PageDown",
            0x23 => "End",
            0x24 => "Home",
            0x25 => "ArrowLeft",
            0x26 => "ArrowUp",
            0x27 => "ArrowRight",
            0x28 => "ArrowDown",
            0x2E => "Delete",
            0xBA => "Semicolon",
            0xBB => "Equal",
            0xBC => "Comma",
            0xBD => "Minus",
            0xBE => "Period",
            0xBF => "Slash",
            0xC0 => "Backquote",
            0xDB => "BracketLeft",
            0xDC => "Backslash",
            0xDD => "BracketRight",
            0xDE => "Quote",
            _ => return None,
        }
        .to_string(),
    )
}

/// A key-down as the core's `KeyEventLike` (Win counts as the mac's ⌘, as the chords spell it `cmd`).
pub fn event(mods: Mods, vk: u32) -> Option<kb::KeyEventLike> {
    let code = code_of(vk)?;
    Some(kb::KeyEventLike {
        ctrl_key: mods.ctrl,
        meta_key: mods.win,
        alt_key: mods.alt,
        shift_key: mods.shift,
        key: vk_name(vk).unwrap_or_default(),
        code: Some(code),
    })
}

/// A key-down as a chord string ("ctrl+shift+c"), or none for keys a chord can't name.
pub fn chord(mods: Mods, vk: u32) -> Option<String> {
    kb::chord_from_event(&event(mods, vk)?)
}

/// The bound chords: the core's sanitised overrides (an empty string unbinds) over the Windows defaults.
#[derive(Clone, Debug, Default)]
pub struct Keybinds {
    binds: kb::Keybinds,
}

impl Keybinds {
    pub fn new(overrides: &BTreeMap<String, String>) -> Keybinds {
        let raw = serde_json::Value::Object(
            overrides
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        );
        Keybinds {
            binds: kb::sanitize_keybinds(&raw),
        }
    }

    pub fn action(&self, mods: Mods, vk: u32) -> Option<KeyAction> {
        kb::action_for_event(&event(mods, vk)?, &self.binds, true).and_then(KeyAction::from_core)
    }

    /// The chord an action has now ("" when unbound).
    pub fn chord_for(&self, action: KeyAction) -> Option<String> {
        let c = kb::keybind_for(&self.binds, action.core(), true);
        (!c.is_empty()).then_some(c)
    }

    /// The chord as Windows writes it ("Ctrl+Shift+C"), "" when unbound.
    pub fn label(&self, action: KeyAction) -> String {
        kb::chord_label(&kb::keybind_for(&self.binds, action.core(), true), true)
    }

    /// The `?` sheet: [keys, things to type], every key as the user has it, worded for Windows (`hostCheats`).
    pub fn cheat_sheet(&self, hotkey_label: &str) -> [Vec<(String, String)>; 2] {
        let labels: BTreeMap<kb::KeyAction, String> = kb::KEY_ACTIONS
            .iter()
            .map(|(a, _)| {
                (
                    *a,
                    kb::chord_label(&kb::keybind_for(&self.binds, *a, true), true),
                )
            })
            .collect();
        let [keys, typed] =
            qcalc_core::app::onboarding::cheat_sheet(Some(hotkey_label), true, Some(&labels));
        let host = |rows: Vec<(String, String)>| {
            rows.into_iter()
                .map(|(k, l)| (host_keys(&k), l))
                .collect::<Vec<_>>()
        };
        [host(keys), host(typed)]
    }
}

/// `hostKeys` (src/lib/platform.ts): mac glyphs spelled the Windows way. `⌃C` (the mac's clear) is the Windows
/// clear, Ctrl+Shift+⌫; a run of ⌘⌃⌥⇧ becomes `Ctrl+Alt+Shift+`.
pub fn host_keys(text: &str) -> String {
    // ⌃C(?![A-Za-z]) → Ctrl+Shift+⌫
    let mut cleared = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("⌃C") {
        let after = &rest[i + "⌃C".len()..];
        cleared.push_str(&rest[..i]);
        if after.starts_with(|c: char| c.is_ascii_alphabetic()) {
            cleared.push_str("⌃C");
        } else {
            cleared.push_str("Ctrl+Shift+⌫");
        }
        rest = after;
    }
    cleared.push_str(rest);
    let text = cleared;
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.is_empty() {
            return;
        }
        for (glyphs, name) in [("⌘⌃", "Ctrl+"), ("⌥", "Alt+"), ("⇧", "Shift+")] {
            if glyphs.chars().any(|g| run.contains(g)) {
                out.push_str(name);
            }
        }
        run.clear();
    };
    for c in text.chars() {
        if "⌘⌃⌥⇧".contains(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Ctrl+W: closes the sheet in front of the bar (periodic table, identities) and says true; false when none is
/// open, and then the bar hides (sheets.rs `close_front`).
#[cfg(windows)]
pub fn close_front_sheet(app: &mut crate::app::App) -> bool {
    let _ = app;
    crate::sheets::close_front()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        win: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: true,
        win: false,
    };

    #[test]
    fn defaults_and_overrides() {
        let k = Keybinds::new(&BTreeMap::new());
        assert_eq!(k.action(CTRL, 0x44), Some(KeyAction::Angle));
        assert_eq!(k.action(CTRL, 0xBC), Some(KeyAction::Settings));
        assert_eq!(k.action(CTRL_SHIFT, 0x08), Some(KeyAction::Clear));
        assert_eq!(k.action(CTRL, 0x43), Some(KeyAction::CopyAnswer));
        assert_eq!(k.action(CTRL_SHIFT, 0x43), Some(KeyAction::CopyLine));
        assert_eq!(k.action(Mods::default(), 0x44), None);
        let k = Keybinds::new(&BTreeMap::from([
            ("angle".to_string(), "ctrl+alt+r".to_string()),
            ("clear".to_string(), String::new()),
            ("fraction".to_string(), "SHIFT+CTRL+F".to_string()),
        ]));
        assert_eq!(k.action(CTRL, 0x44), None);
        assert_eq!(
            k.action(Mods { alt: true, ..CTRL }, 0x52),
            Some(KeyAction::Angle)
        );
        assert_eq!(k.action(CTRL_SHIFT, 0x08), None);
        assert_eq!(k.action(CTRL_SHIFT, 0x46), Some(KeyAction::Fraction));
        assert_eq!(
            k.chord_for(KeyAction::Fraction).as_deref(),
            Some("ctrl+shift+f")
        );
        assert_eq!(k.chord_for(KeyAction::Clear), None);
        assert_eq!(k.label(KeyAction::Angle), "Ctrl+Alt+R");
        assert_eq!(chord(CTRL_SHIFT, 0x43).as_deref(), Some("ctrl+shift+c"));
    }

    #[test]
    fn windows_wording() {
        assert_eq!(host_keys("⌥↑ ⌥↓"), "Alt+↑ Alt+↓");
        assert_eq!(host_keys("⌘,"), "Ctrl+,");
        assert_eq!(host_keys("⌘C ⌘⇧C"), "Ctrl+C Ctrl+Shift+C");
        assert_eq!(host_keys("⌃C"), "Ctrl+Shift+⌫");
        assert_eq!(host_keys("Ctrl+D"), "Ctrl+D");
    }

    #[test]
    fn cheat_sheet_follows_the_keybinds() {
        let k = Keybinds::new(&BTreeMap::from([
            ("copyLine".to_string(), String::new()),
            ("sigFigs".to_string(), String::new()),
        ]));
        let [keys, typed] = k.cheat_sheet("Alt+Space");
        let by: BTreeMap<&str, &str> = keys.iter().map(|(a, b)| (b.as_str(), a.as_str())).collect();
        assert_eq!(by["show / hide"], "Alt+Space");
        assert_eq!(by["copy answer / line"], "Ctrl+C");
        assert_eq!(by["clear history"], "Ctrl+Shift+Backspace");
        assert_eq!(by["settings"], "Ctrl+,");
        assert_eq!(by["unit prefix"], "Alt+↑ Alt+↓");
        assert!(!by.contains_key("sig figs from input"));
        let glyphs = |s: &str| s.chars().any(|c| "⌘⌃⌥⇧".contains(c));
        assert!(!keys.iter().any(|(k, _)| glyphs(k)));
        assert!(typed.iter().any(|(k, _)| k == "?"));
    }
}
