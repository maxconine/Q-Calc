// the sign-in entry on windows: HKCU\...\Run "Q Calc", which starts q calc in the tray when someone signs in. only an
// installed copy may be what it starts: a build under target\ that wrote itself there left sign-in starting a stale
// dev build, or nothing once that build was cleaned. and the installer deletes the entry whenever it uninstalls the old
// version first (a manual upgrade does), so a launch puts back an entry the user still wants
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

// what windows passes when it starts q calc at sign in; that launch stays in the tray
pub const AUTOSTART_ARG: &str = "--autostart";
// the value's name: the product name, which the installer's uninstall also deletes
#[cfg(windows)]
const NAME: &str = "Q Calc";
const EXE: &str = "qcalc.exe";

// quoted, since the install folder ("...\Q Calc") has a space in it
pub fn command(exe: &Path) -> String {
    format!("\"{}\" {AUTOSTART_ARG}", exe.display())
}

// a folder as the installer's registry writes it, maybe quoted
pub fn folder(raw: &str) -> Option<PathBuf> {
    let clean = raw.trim().trim_matches('"').trim();
    (!clean.is_empty()).then(|| PathBuf::from(clean))
}

fn same(a: &Path, b: &Path) -> bool {
    let key = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    key(a) == key(b)
}

// a copy cargo built: anything under a target folder
pub fn built(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"))
}

// this exe is the one in an install folder
pub fn installed(exe: &Path, installs: &[PathBuf]) -> bool {
    !built(exe) && exe.parent().is_some_and(|dir| installs.iter().any(|i| same(i, dir)))
}

// the exe sign in should start: this one when it's installed, else an installed copy that's there, else none
pub fn target(exe: &Path, installs: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    if installed(exe, installs) {
        return Some(exe.to_path_buf());
    }
    installs.iter().filter(|dir| !built(dir)).map(|dir| dir.join(EXE)).find(|exe| exists(exe))
}

// what a launch writes, if anything: the entry for the target while the user wants one and it says something else
// (another exe, one that's gone, an unquoted path, or nothing after an uninstall)
pub fn repair(want: bool, current: Option<&str>, target: Option<&Path>) -> Option<String> {
    let next = command(target.filter(|_| want)?);
    (current != Some(next.as_str())).then_some(next)
}

#[cfg(windows)]
mod live {
    use super::*;
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_BINARY, REG_SZ, RRF_RT_REG_SZ,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    // the installer's own key, whose default value is the folder it installed to
    const INSTALLER: &str = r"Software\maxconine\Q Calc";
    // task manager's "enabled"
    const ENABLED: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read(root: HKEY, key: &str, value: &str) -> Option<String> {
        let (key, value) = (wide(key), wide(value));
        let mut buf = vec![0u16; 2048];
        let mut size = (buf.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(root, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
        };
        if status != 0 {
            return None;
        }
        let len = (size as usize / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]).trim_end_matches('\0').to_string())
    }

    fn set(key: &str, value: &str, kind: u32, bytes: &[u8]) -> bool {
        let (key, value) = (wide(key), wide(value));
        let status = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), kind, bytes.as_ptr().cast(), bytes.len() as u32)
        };
        status == 0
    }

    fn write(command: &str) -> bool {
        let bytes: Vec<u8> = wide(command).iter().flat_map(|u| u.to_le_bytes()).collect();
        set(RUN, NAME, REG_SZ, &bytes)
    }

    // where the installer put q calc: its registry key, then its per-user default
    fn installs() -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
            .into_iter()
            .filter_map(|root| read(root, INSTALLER, "").as_deref().and_then(folder))
            .collect();
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            out.push(local.join("Q Calc"));
            out.push(local.join("Programs").join("Q Calc"));
        }
        out
    }

    fn sign_in_target() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        target(&exe, &installs(), |p| p.is_file())
    }

    // at launch: the entry put right, while the user wants it. task manager's own on / off is theirs and stays
    pub fn sync(want: bool) {
        let current = read(HKEY_CURRENT_USER, RUN, NAME);
        if let Some(next) = repair(want, current.as_deref(), sign_in_target().as_deref()) {
            if write(&next) {
                crate::crash::log(&format!("sign-in entry was {current:?}, now {next}"));
            }
        }
    }

    // turned on in settings: the entry, and task manager's switch on too. false when there's no installed copy
    pub fn enable() -> bool {
        let Some(exe) = sign_in_target() else { return false };
        write(&command(&exe)) && set(APPROVED, NAME, REG_BINARY, &ENABLED)
    }
}

#[cfg(windows)]
pub use live::{enable, sync};

#[cfg(test)]
mod tests {
    use super::*;

    fn installs() -> Vec<PathBuf> {
        vec![PathBuf::from(r"C:\Users\me\AppData\Local\Q Calc"), PathBuf::from(r"D:\Apps\Q Calc")]
    }

    #[test]
    fn the_command_is_quoted_and_quiet() {
        assert_eq!(
            command(Path::new(r"C:\Users\me\AppData\Local\Q Calc\qcalc.exe")),
            r#""C:\Users\me\AppData\Local\Q Calc\qcalc.exe" --autostart"#
        );
    }

    #[test]
    fn folders_from_the_registry() {
        assert_eq!(folder(r#""C:\Users\me\AppData\Local\Q Calc""#), Some(PathBuf::from(r"C:\Users\me\AppData\Local\Q Calc")));
        assert_eq!(folder("  "), None);
        assert_eq!(folder(r#""""#), None);
    }

    #[test]
    fn only_a_copy_in_an_install_folder_is_installed() {
        let installs = installs();
        assert!(installed(Path::new(r"C:\Users\me\AppData\Local\Q Calc\qcalc.exe"), &installs));
        assert!(installed(Path::new(r"c:\users\ME\appdata\local\q calc\QCALC.EXE"), &installs));
        assert!(installed(Path::new(r"D:\Apps\Q Calc\qcalc.exe"), &installs));
        assert!(!installed(Path::new(r"C:\code\Q-Calc\src-tauri\target\release\qcalc.exe"), &installs));
        assert!(!installed(Path::new(r"C:\Users\me\Downloads\qcalc.exe"), &installs));
        // a build dir the registry somehow names still isn't an install
        let odd = vec![PathBuf::from(r"C:\code\Q-Calc\src-tauri\target\release")];
        assert!(!installed(Path::new(r"C:\code\Q-Calc\src-tauri\target\release\qcalc.exe"), &odd));
    }

    #[test]
    fn a_dev_build_points_sign_in_at_the_installed_copy_or_nowhere() {
        let installs = installs();
        let dev = Path::new(r"C:\code\Q-Calc\src-tauri\target\release\qcalc.exe");
        let there = |p: &Path| p == Path::new(r"D:\Apps\Q Calc\qcalc.exe");
        assert_eq!(target(dev, &installs, there), Some(PathBuf::from(r"D:\Apps\Q Calc\qcalc.exe")));
        assert_eq!(target(dev, &installs, |_| false), None);
        let mine = Path::new(r"C:\Users\me\AppData\Local\Q Calc\qcalc.exe");
        assert_eq!(target(mine, &installs, |_| false), Some(mine.to_path_buf()));
    }

    #[test]
    fn a_launch_repairs_only_a_wrong_entry_the_user_wants() {
        let exe = Path::new(r"C:\Users\me\AppData\Local\Q Calc\qcalc.exe");
        let right = command(exe);
        // a dev build's path, an unquoted one, or none after the uninstaller deleted it
        for current in [Some(r"C:\code\Q-Calc\src-tauri\target\release\qcalc.exe --autostart"), Some(r"C:\Users\me\AppData\Local\Q Calc\qcalc.exe --autostart"), None] {
            assert_eq!(repair(true, current, Some(exe)), Some(right.clone()));
        }
        assert_eq!(repair(true, Some(&right), Some(exe)), None);
        // turned off in settings, or no installed copy to point at
        assert_eq!(repair(false, None, Some(exe)), None);
        assert_eq!(repair(true, Some("anything"), None), None);
    }
}
