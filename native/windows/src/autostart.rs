//! Start at sign-in: the `Q Calc` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, the same value
//! the Tauri app's autostart plugin wrote, so turning it off there stays off here.
//!
//! **The bug this fixes.** The Tauri app rewrote the entry with whatever exe was running, so running a dev build
//! re-pointed sign-in at `target\release\qcalc.exe`, and after the next restart the hotkey seemed dead. Here:
//! - only an *installed* build writes the entry (`is_installed`: under `%LOCALAPPDATA%\Programs`, the NSIS per-user
//!   folder `%LOCALAPPDATA%\Q Calc`, or `%ProgramFiles%`; never anything with a `target` folder in its path);
//! - it always points at the running installed exe, quoted, with `--autostart`;
//! - an existing entry that points at a dev build or a stale path is repaired by the next installed launch;
//! - a dev build never writes or removes it, and only logs what it found.
//!
//! The decision (`decide`) is pure and unit-tested; `apply` does the registry part.

pub const VALUE: &str = "Q Calc";
pub const ARG: &str = "--autostart";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Where an exe counts as installed.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    /// %LOCALAPPDATA%
    pub local: Option<String>,
    /// %ProgramFiles%, %ProgramFiles(x86)%
    pub program_files: Vec<String>,
}

impl Roots {
    pub fn from_env() -> Roots {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Roots {
            local: var("LOCALAPPDATA"),
            program_files: ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                .iter()
                .filter_map(|k| var(k))
                .collect(),
        }
    }
}

fn lower(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn under(path: &str, root: &str) -> bool {
    let (path, root) = (lower(path), lower(root));
    !root.is_empty() && path.starts_with(&format!("{root}\\"))
}

/// A path any part of which is a cargo `target` folder (or the worktrees the agents build in).
pub fn is_dev_path(path: &str) -> bool {
    lower(path)
        .split('\\')
        .any(|part| part == "target" || part == ".claude")
}

/// The installer's places, never a build folder.
pub fn is_installed(exe: &str, roots: &Roots) -> bool {
    if is_dev_path(exe) {
        return false;
    }
    let local = roots.local.as_deref().unwrap_or("");
    under(exe, &format!("{local}\\Programs"))
        || under(exe, &format!("{local}\\Q Calc"))
        || roots.program_files.iter().any(|pf| under(exe, pf))
}

/// The Run value for an exe: quoted (a path with spaces), then `--autostart`.
pub fn command(exe: &str) -> String {
    format!("\"{exe}\" {ARG}")
}

/// The exe a Run value starts, quoted or not.
pub fn exe_of(value: &str) -> String {
    let v = value.trim();
    if let Some(rest) = v.strip_prefix('"') {
        return rest.split('"').next().unwrap_or("").to_string();
    }
    match v.to_lowercase().find(".exe") {
        Some(i) => v[..i + 4].to_string(),
        None => v.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// write this value
    Write(String),
    /// delete the value
    Remove,
    /// touch nothing
    Leave,
}

/// What to do with the Run entry at launch.
/// - `existing`: the value now, if any
/// - `exe`: this process's exe
/// - `first_run`: the first launch of this install (`Store::claim_first_run`), which turns sign-in on
/// - `wanted`: Some(on/off) when settings just asked for a change; None at launch
pub fn decide(
    existing: Option<&str>,
    exe: &str,
    roots: &Roots,
    first_run: bool,
    wanted: Option<bool>,
) -> Action {
    if !is_installed(exe, roots) {
        return Action::Leave;
    }
    let ours = command(exe);
    match wanted {
        Some(false) if existing.is_some() => return Action::Remove,
        Some(false) => return Action::Leave,
        Some(true) if existing != Some(ours.as_str()) => return Action::Write(ours),
        Some(true) => return Action::Leave,
        None => {}
    }
    match existing {
        // on, and already right
        Some(v) if v == ours => Action::Leave,
        // on, but pointing at a dev build, an old install, or written without --autostart: repaired
        Some(_) => Action::Write(ours),
        // off: only a first launch turns it on; after that, off is the user's choice
        None if first_run => Action::Write(ours),
        None => Action::Leave,
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::qlog;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey,
        RegDeleteKeyValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    };
    use windows::core::{HSTRING, PCWSTR};

    /// The Run value now.
    pub fn current() -> Option<String> {
        let (key, value) = (HSTRING::from(RUN_KEY), HSTRING::from(VALUE));
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: buf and size describe a live buffer
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    pub fn enabled() -> bool {
        current().is_some()
    }

    fn write(value: &str) -> bool {
        let mut key = HKEY::default();
        let path = HSTRING::from(RUN_KEY);
        // SAFETY: the key is closed below; the data is a live UTF-16 buffer with its terminator
        unsafe {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &path,
                None,
                KEY_READ | KEY_WRITE,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return false;
            }
            let wide: Vec<u16> = value.encode_utf16().chain([0]).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
            let ok = RegSetValueExW(key, &HSTRING::from(VALUE), None, REG_SZ, Some(bytes))
                == ERROR_SUCCESS;
            let _ = RegCloseKey(key);
            ok
        }
    }

    fn remove() -> bool {
        // SAFETY: plain strings in
        unsafe {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(RUN_KEY),
                PCWSTR(HSTRING::from(VALUE).as_ptr()),
            ) == ERROR_SUCCESS
        }
    }

    /// At launch (`wanted` None) or from settings: applies `decide` and logs what it did.
    pub fn apply(first_run: bool, wanted: Option<bool>) {
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let exe = exe.trim_start_matches(r"\\?\").to_string();
        let existing = current();
        let roots = Roots::from_env();
        let action = decide(existing.as_deref(), &exe, &roots, first_run, wanted);
        match &action {
            Action::Write(v) => qlog!("autostart: wrote {v} (was {existing:?}): {}", write(v)),
            Action::Remove => qlog!("autostart: removed (was {existing:?}): {}", remove()),
            Action::Leave if !is_installed(&exe, &roots) => {
                let stale = existing.as_deref().is_some_and(|v| is_dev_path(&exe_of(v)));
                qlog!(
                    "autostart: dev build, left alone (entry {existing:?}{})",
                    if stale {
                        ", points at a dev build: the installed app repairs it at its next launch"
                    } else {
                        ""
                    }
                );
            }
            Action::Leave => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Roots {
        Roots {
            local: Some(r"C:\Users\max\AppData\Local".into()),
            program_files: vec![r"C:\Program Files".into(), r"C:\Program Files (x86)".into()],
        }
    }

    const NSIS: &str = r"C:\Users\max\AppData\Local\Q Calc\qcalc.exe";
    const DEV: &str = r"C:\Users\max\Documents\GitHub\Q-Calc\native\target\release\qcalc.exe";

    #[test]
    fn only_the_installers_places_count_as_installed() {
        let r = roots();
        assert!(is_installed(NSIS, &r));
        assert!(is_installed(
            r"C:\Users\max\AppData\Local\Programs\Q Calc\qcalc.exe",
            &r
        ));
        assert!(is_installed(r"C:\Program Files\Q Calc\qcalc.exe", &r));
        assert!(is_installed(&NSIS.to_lowercase(), &r));
        assert!(!is_installed(DEV, &r));
        assert!(!is_installed(
            r"C:\Users\max\Documents\GitHub\Q-Calc\src-tauri\target\release\qcalc.exe",
            &r
        ));
        // a target folder inside an installed-looking place is still a build
        assert!(!is_installed(
            r"C:\Users\max\AppData\Local\Programs\src\target\debug\qcalc.exe",
            &r
        ));
        assert!(!is_installed(r"C:\Users\max\Downloads\qcalc.exe", &r));
        // a folder that only starts like the install folder isn't it
        assert!(!is_installed(
            r"C:\Users\max\AppData\Local\Q Calculator\qcalc.exe",
            &r
        ));
        assert!(!is_installed(NSIS, &Roots::default()));
    }

    #[test]
    fn a_dev_build_never_touches_the_entry() {
        let r = roots();
        for existing in [None, Some("x"), Some(DEV)] {
            for wanted in [None, Some(true), Some(false)] {
                assert_eq!(decide(existing, DEV, &r, true, wanted), Action::Leave);
            }
        }
    }

    #[test]
    fn the_installed_build_points_the_entry_at_itself() {
        let r = roots();
        let ours = command(NSIS);
        assert_eq!(ours, format!("\"{NSIS}\" --autostart"));
        // the first launch turns it on
        assert_eq!(
            decide(None, NSIS, &r, true, None),
            Action::Write(ours.clone())
        );
        // turned off later stays off
        assert_eq!(decide(None, NSIS, &r, false, None), Action::Leave);
        // right already: nothing to write
        assert_eq!(decide(Some(&ours), NSIS, &r, false, None), Action::Leave);
        // the bug: the entry a dev build left, or the Tauri plugin's unquoted one, is repaired
        let tauri_dev =
            r"C:\Users\max\Documents\GitHub\Q-Calc\src-tauri\target\release\qcalc.exe --autostart";
        assert_eq!(
            decide(Some(tauri_dev), NSIS, &r, false, None),
            Action::Write(ours.clone())
        );
        assert_eq!(
            decide(Some(&format!("{NSIS} --autostart")), NSIS, &r, false, None),
            Action::Write(ours.clone())
        );
        // settings
        assert_eq!(
            decide(Some(&ours), NSIS, &r, false, Some(false)),
            Action::Remove
        );
        assert_eq!(decide(None, NSIS, &r, false, Some(false)), Action::Leave);
        assert_eq!(
            decide(None, NSIS, &r, false, Some(true)),
            Action::Write(ours.clone())
        );
        assert_eq!(
            decide(Some(&ours), NSIS, &r, false, Some(true)),
            Action::Leave
        );
    }

    #[test]
    fn the_exe_a_value_starts() {
        assert_eq!(exe_of(&command(NSIS)), NSIS);
        assert_eq!(exe_of(r"C:\a b\qcalc.exe --autostart"), r"C:\a b\qcalc.exe");
        assert!(is_dev_path(&exe_of(&format!("{DEV} --autostart"))));
    }
}
