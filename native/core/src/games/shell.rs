//! `src/lib/gameShell.ts`: what esc and the keys do around an open game, as plain functions; the shell keeps the
//! state. esc leaves full screen first. in the lobby it steps back, and closes the game from the screen it opened
//! on. a game on this computer pauses on the first esc and leaves to the lobby on the second; a game over the link
//! can't pause the other side, so the first esc only warns and a second one soon after leaves.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellMode {
    Lobby,
    Local,
    Online,
}

/// How long "esc again to leave" waits for the second esc.
pub const ARM_MS: f64 = 2000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EscState {
    pub mode: ShellMode,
    pub fullscreen: bool,
    pub paused: bool,
    /// When the first esc of an online game came, or None.
    pub armed_at: Option<f64>,
    /// Nothing is running (a level picker inside the game), so there's nothing to pause.
    pub idle: bool,
    pub now: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscAction {
    Unfullscreen,
    Lobby,
    Pause,
    Arm,
    Leave,
}

pub fn esc_action(s: &EscState) -> EscAction {
    if s.fullscreen {
        return EscAction::Unfullscreen;
    }
    match s.mode {
        ShellMode::Lobby => EscAction::Lobby,
        ShellMode::Local => {
            if s.paused || s.idle {
                EscAction::Leave
            } else {
                EscAction::Pause
            }
        }
        ShellMode::Online => match s.armed_at {
            Some(at) if s.now - at <= ARM_MS => EscAction::Leave,
            _ => EscAction::Arm,
        },
    }
}

/// A key as the page sees it: `key`, and the modifiers.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct KeyLike {
    pub key: String,
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
}

const MODIFIERS: [&str; 6] = ["Shift", "Meta", "Control", "Alt", "CapsLock", "Fn"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PausedKey {
    Pass,
    Fullscreen,
    Resume,
}

/// A key while paused: f flips full screen, esc goes on to the esc handling, shortcuts with ⌘ or ⌃ still work
/// (⌃F among them, which the full screen handles), and any other key resumes (and goes no further, so it doesn't
/// also move a paddle).
pub fn paused_key(e: &KeyLike) -> PausedKey {
    if e.key == "Escape" || e.key == "Esc" || MODIFIERS.contains(&e.key.as_str()) {
        return PausedKey::Pass;
    }
    if e.meta || e.ctrl || e.alt {
        return PausedKey::Pass;
    }
    if e.key.to_lowercase() == "f" {
        return PausedKey::Fullscreen;
    }
    PausedKey::Resume
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: EscState = EscState {
        mode: ShellMode::Local,
        fullscreen: false,
        paused: false,
        armed_at: None,
        idle: false,
        now: 10_000.0,
    };

    fn key(k: &str) -> KeyLike {
        KeyLike {
            key: k.into(),
            ..Default::default()
        }
    }

    #[test]
    fn leaves_full_screen_before_anything_else() {
        for mode in [ShellMode::Lobby, ShellMode::Local, ShellMode::Online] {
            let s = EscState {
                mode,
                fullscreen: true,
                paused: true,
                ..BASE
            };
            assert_eq!(esc_action(&s), EscAction::Unfullscreen);
        }
    }

    #[test]
    fn steps_back_through_the_lobby() {
        assert_eq!(
            esc_action(&EscState {
                mode: ShellMode::Lobby,
                ..BASE
            }),
            EscAction::Lobby
        );
    }

    #[test]
    fn pauses_a_game_on_this_computer_then_leaves() {
        assert_eq!(esc_action(&BASE), EscAction::Pause);
        assert_eq!(
            esc_action(&EscState {
                paused: true,
                ..BASE
            }),
            EscAction::Leave
        );
        assert_eq!(
            esc_action(&EscState { idle: true, ..BASE }),
            EscAction::Leave
        );
    }

    #[test]
    fn warns_in_an_online_game_and_leaves_on_a_second_esc_soon_after() {
        let online = EscState {
            mode: ShellMode::Online,
            ..BASE
        };
        assert_eq!(esc_action(&online), EscAction::Arm);
        assert_eq!(
            esc_action(&EscState {
                armed_at: Some(BASE.now - 500.0),
                ..online
            }),
            EscAction::Leave
        );
        assert_eq!(
            esc_action(&EscState {
                armed_at: Some(BASE.now - ARM_MS - 1.0),
                ..online
            }),
            EscAction::Arm
        );
        assert_eq!(
            esc_action(&EscState {
                idle: true,
                ..online
            }),
            EscAction::Arm
        );
    }

    #[test]
    fn keys_while_paused() {
        assert_eq!(paused_key(&key(" ")), PausedKey::Resume);
        assert_eq!(paused_key(&key("ArrowUp")), PausedKey::Resume);
        assert_eq!(paused_key(&key("f")), PausedKey::Fullscreen);
        assert_eq!(paused_key(&key("F")), PausedKey::Fullscreen);
        assert_eq!(paused_key(&key("Escape")), PausedKey::Pass);
        assert_eq!(paused_key(&key("Shift")), PausedKey::Pass);
        assert_eq!(
            paused_key(&KeyLike {
                meta: true,
                ..key("c")
            }),
            PausedKey::Pass
        );
        assert_eq!(
            paused_key(&KeyLike {
                ctrl: true,
                ..key("f")
            }),
            PausedKey::Pass
        );
    }
}
