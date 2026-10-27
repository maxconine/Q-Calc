//! The shell around whichever game is open (`GameShell.tsx`, the Mac's GameShellModel): where the player is
//! (the lobby, a game on this computer, one over the link), pause, "esc again to leave", full screen, and which keys
//! the shell takes before the lobby and the game see them. The rules are the core's (`shell::esc_action`,
//! `shell::paused_key`, `fullscreen::is_fullscreen_chord`); the window carries out what `route` returns.

use qcalc_core::games::fullscreen::{Chord, is_fullscreen_chord};
use qcalc_core::games::shell::{EscAction, EscState, KeyLike, PausedKey, ShellMode, esc_action, paused_key};

use super::keys::GameKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellAct {
    /// taken, and nothing more (a held key repeating a toggle)
    Nothing,
    ToggleFullscreen,
    ExitFullscreen,
    Resume,
    /// esc's own handling (`onEscape`)
    Escape,
    /// Ctrl+W
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shell {
    pub mode: ShellMode,
    pub paused: bool,
    /// when the first esc of an online game came (wall ms)
    pub armed_at: Option<f64>,
    pub fullscreen: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Shell {
            mode: ShellMode::Lobby,
            paused: false,
            armed_at: None,
            fullscreen: false,
        }
    }
}

impl Shell {
    /// What esc does now; `idle`: nothing is running, so there's nothing to pause.
    pub fn esc(&self, idle: bool, now: f64) -> EscAction {
        esc_action(&EscState {
            mode: self.mode,
            fullscreen: self.fullscreen,
            paused: self.paused,
            armed_at: self.armed_at,
            idle,
            now,
        })
    }

    /// The keys the shell takes first: Ctrl+W, the paused card's keys, Ctrl+F, and esc. None passes it on.
    pub fn route(&self, k: &GameKey) -> Option<ShellAct> {
        if !k.down {
            return None;
        }
        if k.ctrl && !k.alt && k.lower() == "w" {
            return Some(ShellAct::Close);
        }
        let once = |a: ShellAct| Some(if k.repeat { ShellAct::Nothing } else { a });
        if self.paused {
            match paused_key(&KeyLike {
                key: k.key.clone(),
                meta: false,
                ctrl: k.ctrl,
                alt: k.alt,
            }) {
                PausedKey::Fullscreen => return once(ShellAct::ToggleFullscreen),
                PausedKey::Resume => return once(ShellAct::Resume),
                PausedKey::Pass => {}
            }
        }
        if is_fullscreen_chord(&Chord {
            key: k.key.clone(),
            code: k.code.clone(),
            ctrl: k.ctrl,
            meta: false,
            alt: k.alt,
            shift: k.shift,
        }) {
            return once(ShellAct::ToggleFullscreen);
        }
        if k.key == "Escape" {
            let plain = !k.ctrl && !k.alt && !k.shift;
            return Some(if self.fullscreen && plain {
                ShellAct::ExitFullscreen
            } else {
                ShellAct::Escape
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str) -> GameKey {
        GameKey::named(k)
    }

    #[test]
    fn esc_leaves_full_screen_then_pauses_a_local_game_then_leaves() {
        let mut s = Shell {
            mode: ShellMode::Local,
            fullscreen: true,
            ..Shell::default()
        };
        assert_eq!(s.route(&key("Escape")), Some(ShellAct::ExitFullscreen));
        s.fullscreen = false;
        assert_eq!(s.route(&key("Escape")), Some(ShellAct::Escape));
        assert_eq!(s.esc(false, 0.0), EscAction::Pause);
        s.paused = true;
        assert_eq!(s.esc(false, 0.0), EscAction::Leave);
        // while paused, any key carries on and f flips full screen
        assert_eq!(s.route(&key("x")), Some(ShellAct::Resume));
        assert_eq!(s.route(&key("f")), Some(ShellAct::ToggleFullscreen));
    }

    #[test]
    fn an_online_game_warns_before_leaving() {
        let mut s = Shell {
            mode: ShellMode::Online,
            ..Shell::default()
        };
        assert_eq!(s.esc(false, 1000.0), EscAction::Arm);
        s.armed_at = Some(1000.0);
        assert_eq!(s.esc(false, 2500.0), EscAction::Leave);
        assert_eq!(s.esc(false, 9000.0), EscAction::Arm);
    }

    #[test]
    fn ctrl_f_and_ctrl_w_are_the_shells() {
        let s = Shell::default();
        let mut f = key("f");
        f.code = "KeyF".into();
        f.ctrl = true;
        assert_eq!(s.route(&f), Some(ShellAct::ToggleFullscreen));
        f.repeat = true;
        assert_eq!(s.route(&f), Some(ShellAct::Nothing));
        let mut w = key("w");
        w.ctrl = true;
        assert_eq!(s.route(&w), Some(ShellAct::Close));
        assert_eq!(s.route(&key("ArrowUp")), None);
        assert_eq!(s.esc(false, 0.0), EscAction::Lobby);
    }
}
