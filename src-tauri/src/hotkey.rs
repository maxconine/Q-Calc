// presets only, each is space plus modifiers, like the mac's list
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

pub struct Preset {
    pub id: &'static str,
    pub title: &'static str,
    modifiers: fn() -> Modifiers,
}

impl Preset {
    pub fn shortcut(&self) -> Shortcut {
        Shortcut::new(Some((self.modifiers)()), Code::Space)
    }
}

pub const PRESETS: [Preset; 4] = [
    Preset { id: "alt-space", title: "Alt+Space", modifiers: || Modifiers::ALT },
    Preset { id: "ctrl-alt-space", title: "Ctrl+Alt+Space", modifiers: || Modifiers::CONTROL | Modifiers::ALT },
    Preset { id: "ctrl-space", title: "Ctrl+Space", modifiers: || Modifiers::CONTROL },
    Preset { id: "ctrl-shift-space", title: "Ctrl+Shift+Space", modifiers: || Modifiers::CONTROL | Modifiers::SHIFT },
];
pub const STANDARD: usize = 0;

pub fn named(id: &str) -> usize {
    PRESETS.iter().position(|p| p.id == id).unwrap_or(STANDARD)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotKey {
    pub active: Option<usize>,
    pub failed: bool,
}

// at launch: the picked preset, else the default, else none, and the page's hint line says so
pub fn launch(preferred: usize, mut bind: impl FnMut(usize) -> bool) -> HotKey {
    if bind(preferred) {
        return HotKey { active: Some(preferred), failed: false };
    }
    if preferred != STANDARD && bind(STANDARD) {
        return HotKey { active: Some(STANDARD), failed: true };
    }
    HotKey { active: None, failed: true }
}

// false when another app owns `next`; the previous one stays bound
pub fn select(next: usize, now: HotKey, mut bind: impl FnMut(usize) -> bool, unbind: impl FnOnce()) -> (HotKey, bool) {
    if now.active == Some(next) {
        return (HotKey { active: Some(next), failed: false }, true);
    }
    unbind();
    if bind(next) {
        return (HotKey { active: Some(next), failed: false }, true);
    }
    match now.active {
        Some(previous) if bind(previous) => (now, false),
        _ => (HotKey { active: None, failed: true }, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_fall_back_to_the_standard() {
        assert_eq!(named("ctrl-space"), 2);
        assert_eq!(named("cmd-opt-space"), STANDARD);
        assert_eq!(named(""), STANDARD);
    }

    #[test]
    fn launch_falls_back_and_says_so() {
        assert_eq!(launch(2, |_| true), HotKey { active: Some(2), failed: false });
        assert_eq!(launch(2, |i| i == STANDARD), HotKey { active: Some(STANDARD), failed: true });
        assert_eq!(launch(STANDARD, |_| false), HotKey { active: None, failed: true });
        let mut tried = vec![];
        launch(STANDARD, |i| {
            tried.push(i);
            false
        });
        assert_eq!(tried, [STANDARD]);
    }

    #[test]
    fn select_keeps_the_old_key_when_the_new_one_is_taken() {
        let now = HotKey { active: Some(0), failed: false };
        assert_eq!(select(1, now, |_| true, || ()), (HotKey { active: Some(1), failed: false }, true));
        assert_eq!(select(1, now, |i| i == 0, || ()), (now, false));
        assert_eq!(select(1, now, |_| false, || ()), (HotKey { active: None, failed: true }, false));
        let none = HotKey { active: None, failed: true };
        assert_eq!(select(3, none, |_| true, || ()), (HotKey { active: Some(3), failed: false }, true));
        assert_eq!(select(3, none, |_| false, || ()), (none, false));
    }

    #[test]
    fn reselecting_the_active_key_clears_a_failure_without_rebinding() {
        let now = HotKey { active: Some(0), failed: true };
        let mut unbound = false;
        let out = select(0, now, |_| panic!("no rebind"), || unbound = true);
        assert_eq!(out, (HotKey { active: Some(0), failed: false }, true));
        assert!(!unbound);
    }

    #[test]
    fn titles_match_their_shortcuts() {
        for p in &PRESETS {
            let text = p.shortcut().into_string().to_lowercase().replace("control", "ctrl");
            let want = p.title.to_lowercase();
            for part in want.split('+') {
                assert!(text.contains(part), "{} vs {}", p.title, text);
            }
        }
    }
}
