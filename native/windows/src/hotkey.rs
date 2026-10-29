//! The global show / hide shortcut.
//!
//! Chords are spelled like the page's keybinds (`"ctrl+alt+space"`, modifiers in the order ctrl, alt, shift, cmd;
//! cmd is the Windows key), read from `Store::hotkey` (old preset ids too), exactly as src-tauri/src/hotkey.rs did.
//!
//! **Why this is not a single RegisterHotKey at launch.** After Windows restarts, the Tauri app often came up
//! without its shortcut: at sign-in PowerToys Run or another launcher may hold Alt+Space for a moment, and after an
//! update relaunch the old process may still hold it, and it tried once and never again. Here registration
//! retries on a schedule (`retry_delay`: every second for 30 s, then every 10 s for 3 more minutes), and starts
//! over on resume from sleep, on session unlock and on Explorer restarting (app.rs routes those to
//! `Hotkey::kick`). Every attempt is logged. Only once the schedule runs out does the app say who likely holds the
//! chord (`guess`, from the running launchers and their settings files), as a tray notice and the bar's hint.
//!
//! The pure parts (chord parsing, the schedule, the owner guess) are unit-tested on any OS; the Win32 part is the
//! `Hotkey` at the bottom.

pub const STANDARD: &str = "alt+space";

/// Ids the host window's WM_HOTKEY and WM_TIMER use.
pub const HOTKEY_ID: i32 = 1;
pub const TIMER_ID: usize = 0x4b31;

/// What 2.0 stored: one of four preset ids.
const PRESET_IDS: [(&str, &str); 4] = [
    ("alt-space", "alt+space"),
    ("ctrl-alt-space", "ctrl+alt+space"),
    ("ctrl-space", "ctrl+space"),
    ("ctrl-shift-space", "ctrl+shift+space"),
];

pub const MODIFIERS: [&str; 4] = ["ctrl", "alt", "shift", "cmd"];

/// Modifiers first, in the page's order, so the same chord is always the same string.
pub fn normalize(chord: &str) -> Option<String> {
    let lower = chord.to_lowercase();
    let mut parts: Vec<&str> = lower.split('+').collect();
    // "ctrl++" is ctrl and the plus key
    if lower.ends_with("++") {
        parts.pop();
        parts.pop();
        parts.push("=");
    }
    let key = parts
        .pop()
        .filter(|k| !k.is_empty() && !MODIFIERS.contains(k))?;
    if !parts.iter().all(|p| MODIFIERS.contains(p)) {
        return None;
    }
    let mut out: Vec<&str> = MODIFIERS
        .iter()
        .copied()
        .filter(|m| parts.contains(m))
        .collect();
    out.push(key);
    Some(out.join("+"))
}

/// A virtual key as the page names it; the keys a hotkey can't be (numpad, media...) are none.
pub fn vk_name(vk: u32) -> Option<String> {
    let name = match vk {
        0x30..=0x39 | 0x41..=0x5A => {
            return Some(char::from(vk as u8).to_ascii_lowercase().to_string());
        }
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

/// The virtual key a page key name stands for.
pub fn vk_of(name: &str) -> Option<u32> {
    (0..=0xFFu32).find(|&vk| vk_name(vk).as_deref() == Some(name))
}

/// MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4, MOD_WIN 8 (MOD_NOREPEAT is added when registering).
pub fn win32_chord(chord: &str) -> Option<(u32, u32)> {
    let chord = normalize(chord)?;
    let mut parts: Vec<&str> = chord.split('+').collect();
    let vk = vk_of(parts.pop()?)?;
    let mods = parts.iter().fold(0, |m, p| {
        m | match *p {
            "alt" => 1,
            "ctrl" => 2,
            "shift" => 4,
            _ => 8,
        }
    });
    Some((mods, vk))
}

/// A stored preset id or chord, as a chord that registers; anything else is the standard.
pub fn named(raw: &str) -> String {
    let chord = PRESET_IDS
        .iter()
        .find(|(id, _)| *id == raw)
        .map_or(raw, |(_, c)| c);
    normalize(chord)
        .filter(|c| win32_chord(c).is_some())
        .unwrap_or_else(|| STANDARD.to_string())
}

/// Ctrl+Alt+Space, the way Windows writes keys.
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

// ---- the retry schedule ----

/// Fast retries: one a second for the first 30 s (a launcher grabbing the chord at sign-in, the old process of an
/// update relaunch letting go).
pub const FAST_TRIES: u32 = 30;
pub const FAST_MS: u32 = 1_000;
/// Then one every 10 s for 3 more minutes (a slow sign-in, a launcher that restarts itself).
pub const SLOW_TRIES: u32 = 18;
pub const SLOW_MS: u32 = 10_000;

/// How long to wait after the `failed`-th failed attempt (1-based) before the next one; none once the schedule
/// has run out and the app should say the chord is held.
pub fn retry_delay(failed: u32) -> Option<u32> {
    match failed {
        0 => Some(0),
        n if n <= FAST_TRIES => Some(FAST_MS),
        n if n <= FAST_TRIES + SLOW_TRIES => Some(SLOW_MS),
        _ => None,
    }
}

// ---- who holds a chord ----

/// Launchers known to take a chord like Q Calc's, with their own default. Windows has no call that names the
/// process holding a hotkey, so a running launcher whose shortcut matches is only a likely owner.
pub struct Launcher {
    pub id: &'static str,
    pub name: &'static str,
    pub exes: &'static [&'static str],
    pub chord: &'static str,
}

pub const LAUNCHERS: [Launcher; 6] = [
    Launcher {
        id: "powertoys-run",
        name: "PowerToys Run",
        exes: &["powertoys.exe", "powertoys.powerlauncher.exe"],
        chord: "alt+space",
    },
    Launcher {
        id: "command-palette",
        name: "Command Palette",
        exes: &["microsoft.cmdpal.ui.exe"],
        chord: "alt+cmd+space",
    },
    Launcher {
        id: "flow-launcher",
        name: "Flow Launcher",
        exes: &["flow.launcher.exe"],
        chord: "alt+space",
    },
    Launcher {
        id: "wox",
        name: "Wox",
        exes: &["wox.exe"],
        chord: "alt+space",
    },
    Launcher {
        id: "raycast",
        name: "Raycast",
        exes: &["raycast.exe"],
        chord: "alt+space",
    },
    Launcher {
        id: "keypirinha",
        name: "Keypirinha",
        exes: &["keypirinha.exe", "keypirinha-x64.exe", "keypirinha-x86.exe"],
        chord: "ctrl+cmd+k",
    },
];

/// What a launcher's own settings file says.
#[derive(Clone, Debug, PartialEq)]
pub enum Setting {
    Off,
    Chord(String),
    /// no file, or one this can't read: its default is a guess
    Unknown,
}

/// A launcher that's running, and its shortcut when its settings name one.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub id: &'static str,
    pub chord: Option<String>,
}

pub fn launchers(exes: &[String], setting: impl Fn(&'static str) -> Setting) -> Vec<Running> {
    LAUNCHERS
        .iter()
        .filter(|l| {
            exes.iter()
                .any(|e| l.exes.contains(&e.to_lowercase().as_str()))
        })
        .filter_map(|l| match setting(l.id) {
            Setting::Off => None,
            Setting::Chord(c) => Some(Running {
                id: l.id,
                chord: Some(c),
            }),
            Setting::Unknown => Some(Running {
                id: l.id,
                chord: None,
            }),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Owner {
    pub id: &'static str,
    /// its settings name the chord; otherwise only its default does, or it's Windows' own guess
    pub sure: bool,
}

impl Owner {
    pub fn name(&self) -> &'static str {
        LAUNCHERS
            .iter()
            .find(|l| l.id == self.id)
            .map_or("Windows", |l| l.name)
    }
}

pub fn guess(chord: &str, running: &[Running]) -> Option<Owner> {
    if let Some(r) = running.iter().find(|r| r.chord.as_deref() == Some(chord)) {
        return Some(Owner {
            id: r.id,
            sure: true,
        });
    }
    let default = |id: &str| LAUNCHERS.iter().find(|l| l.id == id).map(|l| l.chord);
    if let Some(r) = running
        .iter()
        .find(|r| r.chord.is_none() && default(r.id) == Some(chord))
    {
        return Some(Owner {
            id: r.id,
            sure: false,
        });
    }
    // the shell keeps most win chords for itself
    chord.split('+').any(|p| p == "cmd").then_some(Owner {
        id: "windows",
        sure: false,
    })
}

/// PowerToys Run's activation shortcut, from its settings.json.
pub fn powertoys_run_chord(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let key = &v["properties"]["open_powerlauncher"];
    let name = vk_name(u32::try_from(key["code"].as_u64()?).ok()?)?;
    let held = |k: &str| key[k].as_bool() == Some(true);
    let mods = [
        ("ctrl", held("ctrl")),
        ("alt", held("alt")),
        ("shift", held("shift")),
        ("cmd", held("win")),
    ];
    let parts: Vec<&str> = mods
        .iter()
        .filter(|(_, on)| *on)
        .map(|(m, _)| *m)
        .chain([name.as_str()])
        .collect();
    normalize(&parts.join("+"))
}

/// False only when PowerToys' own settings turn PowerToys Run off.
pub fn powertoys_run_enabled(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json).map_or(true, |v| {
        v["enabled"]["PowerToys Run"].as_bool() != Some(false)
    })
}

/// Flow Launcher and Wox write theirs as words: "Alt + Space".
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

/// The notice once the schedule has run out.
pub fn held_notice(chord: &str, owner: Option<&Owner>) -> String {
    let keys = title(chord);
    match owner {
        Some(o) if o.sure => format!(
            "{keys} is held by {}. Q Calc is in the tray; pick another shortcut in Settings or change {}'s.",
            o.name(),
            o.name()
        ),
        Some(o) => format!(
            "{keys} is taken, probably by {}. Q Calc is in the tray; pick another shortcut in Settings.",
            o.name()
        ),
        None => format!(
            "{keys} is taken by another app. Q Calc is in the tray; pick another shortcut in Settings."
        ),
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::qlog;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
    };
    use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

    /// The registered shortcut and its retry state. Lives on the host window's thread (app.rs).
    #[derive(Debug)]
    pub struct Hotkey {
        hwnd: HWND,
        /// the chord the user picked (named)
        pub wanted: String,
        /// what is registered now
        pub active: Option<String>,
        failed: u32,
        /// the schedule ran out; the notice was given
        pub gave_up: bool,
    }

    impl Hotkey {
        pub fn new(hwnd: HWND, wanted: &str) -> Hotkey {
            Hotkey {
                hwnd,
                wanted: named(wanted),
                active: None,
                failed: 0,
                gave_up: false,
            }
        }

        fn register(&self, chord: &str) -> windows::core::Result<()> {
            let (mods, vk) = win32_chord(chord).ok_or_else(windows::core::Error::empty)?;
            // SAFETY: hwnd is the host window, alive for the app's life
            unsafe {
                RegisterHotKey(
                    Some(self.hwnd),
                    HOTKEY_ID,
                    HOT_KEY_MODIFIERS(mods) | MOD_NOREPEAT,
                    vk,
                )
            }
        }

        /// One attempt now; true when the shortcut is held by Q Calc afterwards.
        pub fn attempt(&mut self, why: &str) -> bool {
            if self.active.as_deref() == Some(self.wanted.as_str()) {
                return true;
            }
            match self.register(&self.wanted) {
                Ok(()) => {
                    qlog!(
                        "hotkey {} registered ({why}, attempt {})",
                        self.wanted,
                        self.failed + 1
                    );
                    self.active = Some(self.wanted.clone());
                    self.failed = 0;
                    self.gave_up = false;
                    self.stop_timer();
                    true
                }
                Err(e) => {
                    self.failed += 1;
                    qlog!(
                        "hotkey {} not registered ({why}, attempt {}): {}",
                        self.wanted,
                        self.failed,
                        e.message()
                    );
                    false
                }
            }
        }

        /// Starts the schedule over (launch, resume, unlock, Explorer restart, a new chord): one attempt now, the
        /// rest on WM_TIMER (`on_timer`).
        pub fn kick(&mut self, why: &str) {
            if self.active.as_deref() == Some(self.wanted.as_str()) {
                qlog!("hotkey {} still registered ({why})", self.wanted);
                return;
            }
            self.failed = 0;
            self.gave_up = false;
            if !self.attempt(why) {
                self.schedule();
            }
        }

        /// WM_TIMER for TIMER_ID: the next attempt. Some(notice) when the schedule just ran out.
        pub fn on_timer(&mut self) -> Option<String> {
            self.stop_timer();
            if self.attempt("retry") {
                return None;
            }
            if self.schedule() {
                return None;
            }
            self.gave_up = true;
            let owner = guess(&self.wanted, &running_launchers());
            qlog!(
                "hotkey {} gave up after {} attempts; likely owner {:?}",
                self.wanted,
                self.failed,
                owner.as_ref().map(|o| o.id)
            );
            // a chord the user picked that can't be had: the standard one, if it is free, so the bar opens at all
            if self.wanted != STANDARD && self.register(STANDARD).is_ok() {
                qlog!("hotkey fell back to {STANDARD}");
                self.active = Some(STANDARD.to_string());
            }
            Some(held_notice(&self.wanted, owner.as_ref()))
        }

        /// Arms the timer for the next attempt; false when the schedule is over.
        fn schedule(&mut self) -> bool {
            let Some(ms) = retry_delay(self.failed) else {
                return false;
            };
            // SAFETY: hwnd is the host window; no callback, so WM_TIMER arrives in its wndproc
            unsafe { SetTimer(Some(self.hwnd), TIMER_ID, ms.max(1), None) };
            true
        }

        fn stop_timer(&self) {
            // SAFETY: killing a timer that isn't set is harmless
            let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_ID) };
        }

        /// A new chord from settings: keep the old one until the new one registers.
        pub fn select(&mut self, chord: &str) -> bool {
            let next = named(chord);
            if self.active.as_deref() == Some(next.as_str()) {
                self.wanted = next;
                return true;
            }
            self.unregister();
            self.wanted = next;
            self.kick("new chord");
            self.active.is_some()
        }

        pub fn unregister(&mut self) {
            if self.active.take().is_some() {
                // SAFETY: our own registration on our own window
                let _ = unsafe { UnregisterHotKey(Some(self.hwnd), HOTKEY_ID) };
            }
        }
    }

    /// Lowercased exe names of the running processes.
    pub fn running_exes() -> Vec<String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };
        let mut out = Vec::new();
        // SAFETY: the snapshot handle is closed below; the entry is a sized local
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap, &mut e).is_ok();
            while more {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                out.push(String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase());
                more = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    fn read(var: &str, rel: &str) -> Option<String> {
        let dir = std::env::var_os(var)?;
        std::fs::read_to_string(std::path::PathBuf::from(dir).join(rel)).ok()
    }

    /// The running launchers and what their settings files say.
    pub fn running_launchers() -> Vec<Running> {
        let chord = |c: Option<String>| c.map_or(Setting::Unknown, Setting::Chord);
        launchers(&running_exes(), |id| match id {
            "powertoys-run" => {
                if read("LOCALAPPDATA", r"Microsoft\PowerToys\settings.json")
                    .is_some_and(|j| !powertoys_run_enabled(&j))
                {
                    return Setting::Off;
                }
                chord(
                    read(
                        "LOCALAPPDATA",
                        r"Microsoft\PowerToys\PowerToys Run\settings.json",
                    )
                    .and_then(|j| powertoys_run_chord(&j)),
                )
            }
            "flow-launcher" => chord(
                read("APPDATA", r"FlowLauncher\Settings\Settings.json")
                    .and_then(|j| words_chord(&j)),
            ),
            "wox" => {
                chord(read("APPDATA", r"Wox\Settings\Settings.json").and_then(|j| words_chord(&j)))
            }
            _ => Setting::Unknown,
        })
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
    fn chords_become_win32_modifiers_and_keys() {
        assert_eq!(win32_chord("alt+space"), Some((1, 0x20)));
        assert_eq!(win32_chord("ctrl+alt+space"), Some((3, 0x20)));
        assert_eq!(win32_chord("shift+ctrl+k"), Some((6, 0x4B)));
        assert_eq!(win32_chord("cmd+f12"), Some((8, 0x7B)));
        assert_eq!(win32_chord("ctrl+,"), Some((2, 0xBC)));
        assert_eq!(win32_chord("ctrl+numpad5"), None);
        for key in (0..=0xFF).filter_map(vk_name) {
            assert!(win32_chord(&format!("ctrl+alt+{key}")).is_some(), "{key}");
        }
    }

    #[test]
    fn titles_read_like_windows() {
        assert_eq!(title("ctrl+alt+space"), "Ctrl+Alt+Space");
        assert_eq!(title("alt+cmd+k"), "Alt+Win+K");
    }

    #[test]
    fn the_schedule_is_fast_then_slow_then_over() {
        assert_eq!(retry_delay(0), Some(0));
        assert_eq!(retry_delay(1), Some(1_000));
        assert_eq!(retry_delay(30), Some(1_000));
        assert_eq!(retry_delay(31), Some(10_000));
        assert_eq!(retry_delay(48), Some(10_000));
        assert_eq!(retry_delay(49), None);
        // the fast phase covers 30 s, the whole schedule about three and a half minutes
        let total: u32 = (1..).map_while(retry_delay).sum();
        assert_eq!(total, 30 * 1_000 + 18 * 10_000);
        let fast: u32 = (1..=FAST_TRIES).map_while(retry_delay).sum();
        assert_eq!(fast, 30_000);
    }

    #[test]
    fn owners_are_sure_only_when_their_settings_say_so() {
        let exes = [
            "explorer.exe".to_string(),
            "PowerToys.exe".to_string(),
            "Flow.Launcher.exe".to_string(),
        ];
        let running = launchers(&exes, |id| match id {
            "powertoys-run" => Setting::Chord("alt+space".into()),
            _ => Setting::Unknown,
        });
        assert_eq!(running.len(), 2);
        let owner = guess("alt+space", &running).unwrap();
        assert_eq!(
            owner,
            Owner {
                id: "powertoys-run",
                sure: true
            }
        );
        assert!(
            held_notice("alt+space", Some(&owner))
                .starts_with("Alt+Space is held by PowerToys Run.")
        );
        let running = launchers(&exes, |id| {
            if id == "powertoys-run" {
                Setting::Chord("ctrl+space".into())
            } else {
                Setting::Unknown
            }
        });
        assert_eq!(
            guess("alt+space", &running),
            Some(Owner {
                id: "flow-launcher",
                sure: false
            })
        );
        assert_eq!(guess("ctrl+alt+k", &[]), None);
        assert!(held_notice("ctrl+alt+k", None).contains("another app"));
    }

    #[test]
    fn launcher_settings_files() {
        let run = r#"{"properties":{"open_powerlauncher":{"win":false,"ctrl":false,"alt":true,"shift":false,"code":32}}}"#;
        assert_eq!(powertoys_run_chord(run).as_deref(), Some("alt+space"));
        assert!(!powertoys_run_enabled(
            r#"{"enabled":{"PowerToys Run":false}}"#
        ));
        assert_eq!(
            words_chord(r#"{"Hotkey":"Win + Ctrl + K"}"#).as_deref(),
            Some("ctrl+cmd+k")
        );
    }
}
