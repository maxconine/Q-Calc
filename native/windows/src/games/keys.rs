//! Keys as the web page saw them (`KeyboardEvent`, the Mac's GameKeys.swift): `key` is the DOM's name
//! (`ArrowUp`, `Enter`, `Backspace`, `Tab`, `Escape`, `" "`, or the character typed, shift applied) and `code` the
//! qwerty position (`KeyW`, `ArrowLeft`, `Space`, `Digit1`), so the games' key handling reads as the TS.
//!
//! Win32 sends a key down (WM_KEYDOWN, with the virtual key and the scan code) and then, for a key that types,
//! WM_CHAR with the character. Named keys go out on the key down; a typing key waits for its character (so the
//! layout and shift are applied), unless Ctrl or Alt is held, when the letter comes from the virtual key as the
//! page saw it (⌃F is `f`, not U+0006). Key ups name the character the key last typed (`Tracker`).

use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameKey {
    pub key: String,
    pub code: String,
    pub down: bool,
    pub repeat: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl GameKey {
    /// A key down with this name (tests, and clicks that act as keys).
    pub fn named(key: &str) -> GameKey {
        GameKey {
            key: key.into(),
            down: true,
            ..GameKey::default()
        }
    }

    /// `plainKey`: no Ctrl or Alt (Windows has no ⌘ that reaches a window).
    pub fn plain(&self) -> bool {
        !self.ctrl && !self.alt
    }

    /// A one-character key (`e.key.length === 1`, in UTF-16 units like the page).
    pub fn is_char(&self) -> bool {
        self.key.encode_utf16().count() == 1
    }

    pub fn is(&self, name: &str) -> bool {
        self.key == name
    }

    pub fn lower(&self) -> String {
        self.key.to_lowercase()
    }
}

/// The DOM name of a key that doesn't type (or that the page named even though it types: Enter, Tab, Backspace).
pub fn named_key(vk: u32) -> Option<&'static str> {
    Some(match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Escape",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x2E => "Delete",
        0x70 => "F1",
        0x71 => "F2",
        0x72 => "F3",
        0x73 => "F4",
        0x74 => "F5",
        0x75 => "F6",
        0x76 => "F7",
        0x77 => "F8",
        0x78 => "F9",
        0x79 => "F10",
        0x7A => "F11",
        0x7B => "F12",
        0x10 => "Shift",
        0x11 => "Control",
        0x12 => "Alt",
        0x14 => "CapsLock",
        _ => return None,
    })
}

/// `KeyboardEvent.code` from the key's scan code (set 1, the physical position) and the extended bit.
pub fn code_of(scan: u32, extended: bool, vk: u32) -> String {
    if let Some(n) = named_key(vk) {
        return if n == "Enter" && extended {
            "NumpadEnter".into()
        } else {
            n.into()
        };
    }
    const ROW1: &str = "QWERTYUIOP";
    const ROW2: &str = "ASDFGHJKL";
    const ROW3: &str = "ZXCVBNM";
    let letter = |row: &str, i: u32| format!("Key{}", &row[i as usize..=i as usize]);
    match scan {
        0x02..=0x0A => format!("Digit{}", scan - 1),
        0x0B => "Digit0".into(),
        0x0C => "Minus".into(),
        0x0D => "Equal".into(),
        0x10..=0x19 => letter(ROW1, scan - 0x10),
        0x1A => "BracketLeft".into(),
        0x1B => "BracketRight".into(),
        0x1E..=0x26 => letter(ROW2, scan - 0x1E),
        0x27 => "Semicolon".into(),
        0x28 => "Quote".into(),
        0x29 => "Backquote".into(),
        0x2B => "Backslash".into(),
        0x2C..=0x32 => letter(ROW3, scan - 0x2C),
        0x33 => "Comma".into(),
        0x34 => "Period".into(),
        0x35 => "Slash".into(),
        0x39 => "Space".into(),
        _ => String::new(),
    }
}

/// The page's key for a typing key pressed with Ctrl or Alt: the unshifted letter or digit.
pub fn chord_key(vk: u32, shift: bool) -> Option<String> {
    let c = char::from_u32(vk)?;
    if c.is_ascii_uppercase() {
        return Some(if shift {
            c.to_string()
        } else {
            c.to_ascii_lowercase().to_string()
        });
    }
    if c.is_ascii_digit() {
        return Some(c.to_string());
    }
    match vk {
        0x20 => Some(" ".into()),
        0xBC => Some(",".into()),
        0xBE => Some(".".into()),
        0xBD => Some("-".into()),
        0xBB => Some("=".into()),
        _ => None,
    }
}

/// Puts WM_KEYDOWN / WM_CHAR / WM_KEYUP together into `GameKey`s.
#[derive(Debug, Default)]
pub struct Tracker {
    /// the key down waiting for its WM_CHAR: (vk, code, repeat)
    pending: Option<(u32, String, bool)>,
    /// what each held key typed, for its key up
    typed: HashMap<u32, String>,
}

/// The modifiers at the time of a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Tracker {
    /// WM_KEYDOWN / WM_SYSKEYDOWN. A key that types returns None and comes back as `char`.
    pub fn down(
        &mut self,
        vk: u32,
        scan: u32,
        extended: bool,
        repeat: bool,
        mods: Mods,
    ) -> Option<GameKey> {
        let code = code_of(scan, extended, vk);
        let make = |key: String| GameKey {
            key,
            code: code.clone(),
            down: true,
            repeat,
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
        };
        if let Some(n) = named_key(vk) {
            self.pending = None;
            return Some(make(n.into()));
        }
        if mods.ctrl || mods.alt {
            self.pending = None;
            let key = chord_key(vk, mods.shift)?;
            self.typed.insert(vk, key.clone());
            return Some(make(key));
        }
        self.pending = Some((vk, code, repeat));
        None
    }

    /// WM_CHAR: the character the pending key typed (control characters are dropped).
    pub fn char(&mut self, unit: u16, mods: Mods) -> Option<GameKey> {
        let c = char::from_u32(u32::from(unit))?;
        // Enter, Tab and Backspace were sent as named keys on their key down; ctrl chords type control codes
        if c.is_control() {
            return None;
        }
        let (vk, code, repeat) = self.pending.take().unwrap_or((0, String::new(), false));
        let key = c.to_string();
        if vk != 0 {
            self.typed.insert(vk, key.clone());
        }
        Some(GameKey {
            key,
            code,
            down: true,
            repeat,
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
        })
    }

    /// WM_KEYUP / WM_SYSKEYUP.
    pub fn up(&mut self, vk: u32, scan: u32, extended: bool, mods: Mods) -> Option<GameKey> {
        let key = match named_key(vk) {
            Some(n) => n.to_string(),
            None => self.typed.remove(&vk)?,
        };
        Some(GameKey {
            key,
            code: code_of(scan, extended, vk),
            down: false,
            repeat: false,
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
        })
    }

    /// The window lost the keyboard: nothing is held any more.
    pub fn reset(&mut self) {
        self.pending = None;
        self.typed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typing_key_waits_for_its_character_and_its_key_up_names_it() {
        let mut t = Tracker::default();
        // W: vk 0x57, scan 0x11
        assert_eq!(t.down(0x57, 0x11, false, false, Mods::default()), None);
        let k = t.char(u16::from(b'w'), Mods::default()).unwrap();
        assert_eq!(
            (k.key.as_str(), k.code.as_str(), k.down),
            ("w", "KeyW", true)
        );
        let up = t.up(0x57, 0x11, false, Mods::default()).unwrap();
        assert_eq!((up.key.as_str(), up.down), ("w", false));
        assert!(t.up(0x57, 0x11, false, Mods::default()).is_none());
    }

    #[test]
    fn named_keys_and_chords_go_out_on_the_key_down() {
        let mut t = Tracker::default();
        let k = t.down(0x26, 0x48, true, false, Mods::default()).unwrap();
        assert_eq!((k.key.as_str(), k.code.as_str()), ("ArrowUp", "ArrowUp"));
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        let f = t.down(0x46, 0x21, false, false, ctrl).unwrap();
        assert_eq!(
            (f.key.as_str(), f.code.as_str(), f.ctrl),
            ("f", "KeyF", true)
        );
        // the U+0006 that follows is dropped
        assert!(t.char(6, ctrl).is_none());
        let space = t.down(0x20, 0x39, false, false, Mods::default());
        assert!(space.is_none());
        assert_eq!(t.char(32, Mods::default()).unwrap().code, "Space");
        assert_eq!(code_of(0x02, false, 0x31), "Digit1");
        assert_eq!(code_of(0x0B, false, 0x30), "Digit0");
        assert!(GameKey::named("x").is_char());
        assert!(!GameKey::named("Enter").is_char());
    }
}
