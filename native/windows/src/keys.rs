//! The bar's own shortcuts (`src/lib/keybinds.ts`): the user's overrides from `settings.keybinds` over the
//! Windows defaults, matched against a key-down. Pure; model.rs asks `Keybinds::action` before the field gets the
//! key. `show` is the global hotkey (hotkey.rs), not routed here.

use std::collections::BTreeMap;

use crate::hotkey::{normalize, vk_name};

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
        match self {
            KeyAction::Settings => "settings",
            KeyAction::CopyAnswer => "copyAnswer",
            KeyAction::CopyLine => "copyLine",
            KeyAction::Angle => "angle",
            KeyAction::Fraction => "fraction",
            KeyAction::SigFigs => "sigFigs",
            KeyAction::Clear => "clear",
        }
    }

    pub fn of(id: &str) -> Option<KeyAction> {
        [
            KeyAction::Settings,
            KeyAction::CopyAnswer,
            KeyAction::CopyLine,
            KeyAction::Angle,
            KeyAction::Fraction,
            KeyAction::SigFigs,
            KeyAction::Clear,
        ]
        .into_iter()
        .find(|a| a.id() == id)
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

/// A key-down as a chord string ("ctrl+shift+c"), or none for keys a chord can't name.
pub fn chord(mods: Mods, vk: u32) -> Option<String> {
    let key = vk_name(vk)?;
    let mut parts = Vec::new();
    if mods.ctrl {
        parts.push("ctrl");
    }
    if mods.alt {
        parts.push("alt");
    }
    if mods.shift {
        parts.push("shift");
    }
    if mods.win {
        parts.push("cmd");
    }
    parts.push(&key);
    Some(parts.join("+"))
}

/// The bound chords, defaults overridden by the user's (an empty string unbinds).
#[derive(Clone, Debug, Default)]
pub struct Keybinds {
    by_chord: Vec<(String, KeyAction)>,
}

impl Keybinds {
    pub fn new(overrides: &BTreeMap<String, String>) -> Keybinds {
        let by_chord = WINDOWS_DEFAULTS
            .iter()
            .filter_map(|(id, default)| {
                let action = KeyAction::of(id)?;
                let chord = overrides.get(*id).map_or(*default, String::as_str);
                normalize(chord).map(|c| (c, action))
            })
            .collect();
        Keybinds { by_chord }
    }

    pub fn action(&self, mods: Mods, vk: u32) -> Option<KeyAction> {
        let pressed = chord(mods, vk)?;
        self.by_chord
            .iter()
            .find(|(c, _)| *c == pressed)
            .map(|(_, a)| *a)
    }

    pub fn chord_for(&self, action: KeyAction) -> Option<&str> {
        self.by_chord
            .iter()
            .find(|(_, a)| *a == action)
            .map(|(c, _)| c.as_str())
    }
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
        assert_eq!(k.action(Mods::default(), 0x44), None);
        let k = Keybinds::new(&BTreeMap::from([
            ("angle".to_string(), "ctrl+alt+r".to_string()),
            ("clear".to_string(), String::new()),
        ]));
        assert_eq!(k.action(CTRL, 0x44), None);
        assert_eq!(
            k.action(Mods { alt: true, ..CTRL }, 0x52),
            Some(KeyAction::Angle)
        );
        assert_eq!(k.action(CTRL_SHIFT, 0x08), None);
        assert_eq!(k.chord_for(KeyAction::Fraction), Some("ctrl+f"));
    }
}
