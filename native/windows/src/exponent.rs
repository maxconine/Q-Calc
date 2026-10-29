//! Ctrl+E's exponent-form mode and Ctrl+Shift+E's other forms (BarModel.swift `toggleExponent`, `exponentShown`,
//! `cycleForms`; macos/Keybinds.swift's native actions). Native only: the page's Tab cycled the forms.
//!
//! - **The mode** (`settings.exponentMode`, so it is remembered across restarts): every answer that starts with a
//!   plain number shows in exponent form, `3,000 m` → `3 × 10³ m`, and a prefixed unit gives its prefix to the power
//!   (`5 km` → `5 × 10³ m`), by the core's `exponent_form`. Turning it on, the hint says how to turn it off. Lit as
//!   `×10ⁿ` beside deg · a/b · sf (edge.rs).
//! - **The keys**: `exponent` (Ctrl+E) and `forms` (Ctrl+Shift+E) are the native app's own actions, which the
//!   core's keybind list (shared with the page) doesn't have. Their overrides sit in `settings.keybinds` beside the
//!   core's, and Settings › Keybinds lists them under the core's rows (`NATIVE_ACTIONS`, `native_problem`).
//!
//! Cost: nothing while the mode is off; while it's on, one regex per answer shown, memoised by the display.

use std::collections::BTreeMap;

use qcalc_core::app::keybinds as kb;

/// The native app's own actions: id, settings label, Windows default (macos/Keybinds.swift `native`).
pub const NATIVE_ACTIONS: [(&str, &str, &str); 2] = [
    ("exponent", "Exponent form (3 × 10³)", "ctrl+e"),
    ("forms", "Other answer forms", "ctrl+shift+e"),
];

/// Which native action an id is.
pub fn native_action(id: &str) -> Option<&'static str> {
    NATIVE_ACTIONS.iter().find(|a| a.0 == id).map(|a| a.0)
}

pub fn native_label(id: &str) -> &'static str {
    NATIVE_ACTIONS
        .iter()
        .find(|a| a.0 == id)
        .map_or("", |a| a.1)
}

pub fn native_default(id: &str) -> &'static str {
    NATIVE_ACTIONS
        .iter()
        .find(|a| a.0 == id)
        .map_or("", |a| a.2)
}

/// A native action's chord from the stored overrides: the override (normalised; "" unbinds), else its default.
pub fn native_chord(overrides: &BTreeMap<String, String>, id: &str) -> String {
    let default = native_default(id);
    match overrides.get(id).map(|c| c.to_lowercase()) {
        Some(c) if c.is_empty() => String::new(),
        Some(c) if kb::is_chord(&c) => kb::normalize_chord(&c),
        _ => default.to_string(),
    }
}

/// The action a chord does among the native ones.
pub fn native_for(overrides: &BTreeMap<String, String>, chord: &str) -> Option<&'static str> {
    NATIVE_ACTIONS
        .iter()
        .find(|a| native_chord(overrides, a.0) == chord)
        .map(|a| a.0)
}

/// Why a chord can't be a native action's (the core's rules: a modifier, not Windows' own, no clash with a core
/// action or the show / hide chord; and no clash with the other native action), or None.
pub fn native_problem(
    chord: &str,
    id: &str,
    overrides: &BTreeMap<String, String>,
    core: &kb::Keybinds,
) -> Option<String> {
    // the core checks clashes with every action but the one asked for: ask as `show`, and check show's own here
    let mut others = core.clone();
    let show = others.remove(&kb::KeyAction::Show);
    if let Some(why) = kb::chord_problem(chord, kb::KeyAction::Show, &others, true) {
        return Some(why);
    }
    if show.as_deref() == Some(chord) {
        return Some(format!(
            "{} already does {}",
            kb::chord_label(chord, true),
            kb::KeyAction::Show.label().to_lowercase()
        ));
    }
    native_clash(chord, Some(id), overrides)
}

/// A native action (but `except`) that already has the chord: "Ctrl+E already does exponent form (3 × 10³)".
pub fn native_clash(
    chord: &str,
    except: Option<&str>,
    overrides: &BTreeMap<String, String>,
) -> Option<String> {
    NATIVE_ACTIONS
        .iter()
        .filter(|a| Some(a.0) != except)
        .find(|a| native_chord(overrides, a.0) == chord)
        .map(|a| {
            format!(
                "{} already does {}",
                kb::chord_label(chord, true),
                a.1.to_lowercase()
            )
        })
}

/// The overrides with a native action's chord set; its default is stored as no override, as the core does.
pub fn set_native(
    overrides: &BTreeMap<String, String>,
    id: &str,
    chord: &str,
) -> BTreeMap<String, String> {
    let mut next = overrides.clone();
    if chord == native_default(id) {
        next.remove(id);
    } else {
        next.insert(id.to_string(), chord.to_string());
    }
    next
}

/// The overrides as the core keeps them, with the native ones carried over (settings' `pick_key` would drop
/// them: the core's JSON only knows its own actions).
pub fn with_native(
    core_json: serde_json::Value,
    overrides: &BTreeMap<String, String>,
) -> serde_json::Value {
    let mut out = core_json;
    if let Some(o) = out.as_object_mut() {
        for (id, _, _) in NATIVE_ACTIONS {
            if let Some(c) = overrides.get(id) {
                o.insert(id.to_string(), serde_json::Value::String(c.clone()));
            }
        }
    }
    out
}

/// The hint as the mode goes on ("exponent form on · Ctrl+E turns it off"); off says nothing.
pub fn hint(on: bool, key_title: &str) -> Option<String> {
    on.then(|| {
        format!(
            "exponent form on · {} turns it off",
            if key_title.is_empty() {
                "the same key"
            } else {
                key_title
            }
        )
    })
}

/// The edge tool's tip, with its key.
pub fn tip(on: bool, overrides: &BTreeMap<String, String>) -> String {
    let text = format!(
        "Answers in exponent form: {}",
        if on { "on" } else { "off" }
    );
    let chord = native_chord(overrides, "exponent");
    if chord.is_empty() {
        text
    } else {
        format!("{text} · {}", crate::hotkey::title(&chord))
    }
}

/// `display` in exponent form (`exponent_form`), or as it is when it doesn't start with a plain number.
pub fn form(display: &str, sig_figs: u32) -> String {
    if display.is_empty() {
        return String::new();
    }
    match qcalc_core::format::exponent_form(display, sig_figs) {
        Some(f) if !f.is_empty() => f,
        _ => display.to_string(),
    }
}

#[cfg(windows)]
pub use bar::*;

#[cfg(windows)]
mod bar {
    use std::cell::RefCell;

    use crate::engine::EngineThread;
    use crate::keys::Mods;
    use crate::model::BarModel;

    thread_local! {
        /// the last display shown in exponent form, and its form (exponentMemo)
        static MEMO: RefCell<Option<(String, u32, String)>> = const { RefCell::new(None) };
    }

    /// `display` as the slot shows it: in exponent form while the mode is on (exponentShown).
    pub fn shown(m: &BarModel, display: &str) -> String {
        if !m.settings.exponent_mode || display.is_empty() {
            return display.to_string();
        }
        let figs = m.settings.sig_figs.clamp(2, 16) as u32;
        MEMO.with(|memo| {
            let mut memo = memo.borrow_mut();
            if let Some((d, f, out)) = memo.as_ref()
                && d == display
                && *f == figs
            {
                return out.clone();
            }
            let out = super::form(display, figs);
            *memo = Some((display.to_string(), figs, out.clone()));
            out
        })
    }

    /// Ctrl+E (Settings › Keybinds): the mode on or off, remembered; turning it on says how to turn it off.
    pub fn toggle(m: &mut BarModel, engine: &EngineThread) {
        m.settings.exponent_mode = !m.settings.exponent_mode;
        m.settings_dirty = true;
        let chord = super::native_chord(&m.settings.keybinds, "exponent");
        let title = if chord.is_empty() {
            String::new()
        } else {
            crate::hotkey::title(&chord)
        };
        m.hint = super::hint(m.settings.exponent_mode, &title);
        crate::a11y::announce(if m.settings.exponent_mode {
            "exponent form on"
        } else {
            "exponent form off"
        });
        crate::typst_row::refresh(m, engine);
        m.touch();
    }

    /// The native actions' keys, after the core's (model.rs `key_down`): true when the key was one.
    pub fn key_down(m: &mut BarModel, vk: u32, mods: Mods, engine: &EngineThread) -> bool {
        if !(mods.ctrl || mods.alt || mods.win) {
            return false;
        }
        let Some(chord) = crate::keys::chord(mods, vk) else {
            return false;
        };
        match super::native_for(&m.settings.keybinds, &chord) {
            Some("exponent") => toggle(m, engine),
            Some(_) => crate::answer::cycle_forms(m, engine, false),
            None => return false,
        }
        m.touch();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponent_and_forms_are_native_keys() {
        let none = BTreeMap::new();
        assert_eq!(native_for(&none, "ctrl+e"), Some("exponent"));
        assert_eq!(native_for(&none, "ctrl+shift+e"), Some("forms"));
        assert_eq!(native_for(&none, "ctrl+d"), None);
        let o = BTreeMap::from([
            ("exponent".to_string(), "CTRL+ALT+X".to_string()),
            ("forms".to_string(), String::new()),
        ]);
        assert_eq!(native_for(&o, "ctrl+e"), None);
        assert_eq!(native_for(&o, "ctrl+shift+e"), None);
        assert_eq!(native_for(&o, "ctrl+alt+x"), Some("exponent"));
        assert_eq!(native_chord(&o, "forms"), "");
        // a clash with a core action, the show chord or the other native one is refused
        let mut core = kb::Keybinds::new();
        assert!(native_problem("ctrl+d", "exponent", &none, &core).is_some());
        assert!(native_problem("ctrl+shift+e", "exponent", &none, &core).is_some());
        assert!(native_problem("e", "exponent", &none, &core).is_some());
        assert_eq!(native_problem("ctrl+alt+x", "exponent", &none, &core), None);
        assert_eq!(native_problem("ctrl+e", "exponent", &none, &core), None);
        core.insert(kb::KeyAction::Show, "ctrl+alt+x".into());
        assert!(native_problem("ctrl+alt+x", "exponent", &none, &core).is_some());
        assert!(native_clash("ctrl+e", None, &none).is_some());
        assert!(set_native(&none, "exponent", "ctrl+e").is_empty());
        assert_eq!(set_native(&none, "forms", "")["forms"], "");
        let json = with_native(serde_json::json!({"angle": "ctrl+r"}), &o);
        assert_eq!(json["forms"], "");
        assert_eq!(json["exponent"], "CTRL+ALT+X");
        assert_eq!(json["angle"], "ctrl+r");
    }

    #[test]
    fn the_hint_says_how_to_turn_it_off() {
        assert_eq!(
            hint(true, "Ctrl+E").as_deref(),
            Some("exponent form on · Ctrl+E turns it off")
        );
        assert_eq!(hint(false, "Ctrl+E"), None);
        assert_eq!(
            tip(true, &BTreeMap::new()),
            "Answers in exponent form: on · Ctrl+E"
        );
    }

    #[test]
    fn answers_in_exponent_form() {
        let n = |s: &str| s.replace(['\u{2009}', '\u{202F}'], " ");
        assert_eq!(n(&form("3,000", 6)), "3 × 10³");
        assert_eq!(n(&form("5 km", 6)), "5 × 10³ m");
        assert_eq!(n(&form("1 GB", 6)), "1 × 10⁹ B");
        assert_eq!(form("1/2", 6), "1/2");
        assert_eq!(form("", 6), "");
    }
}
