// a failure while q calc starts would otherwise end the process with nothing on screen and nothing kept: the windows
// subsystem has no console for a panic, and the tray icon comes last. so each launch and anything that goes wrong
// lands in %LOCALAPPDATA%\com.maxconine.qcalc\qcalc.log, and a failure before the app is up says so in a message box.
// a copy that won't open with nothing in the log never ran at all (windows or an antivirus stopped it first)
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// past this the log starts over, so it never grows without end
const MAX_LOG_BYTES: u64 = 256 * 1024;

// set once the app is up; a panic after that is logged but doesn't interrupt with a message box
static STARTED: AtomicBool = AtomicBool::new(false);

pub fn log_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("com.maxconine.qcalc").join("qcalc.log"))
}

pub fn log(line: &str) {
    let Some(path) = log_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let full = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES);
    let file = std::fs::OpenOptions::new().create(true).append(!full).write(true).truncate(full).open(&path);
    if let Ok(mut file) = file {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(file, "{at} {line}");
    }
}

// before anything else in main, so a panic anywhere after is kept
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(&format!("panic: {info}"));
        if !STARTED.load(Ordering::SeqCst) {
            tell(&info.to_string());
        }
        previous(info);
    }));
}

pub fn started(line: &str) {
    STARTED.store(true, Ordering::SeqCst);
    log(line);
}

// the app couldn't be built; the caller exits after
pub fn failed(why: &str) {
    log(&format!("couldn't start: {why}"));
    tell(why);
}

pub fn message(why: &str) -> String {
    let log = log_path().map_or(String::new(), |p| format!("\n\nThis is also written to {}.", p.display()));
    format!("Q Calc couldn't start.\n\n{why}{log}")
}

#[cfg(windows)]
fn tell(why: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(&message(why)), wide("Q Calc"));
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR | MB_SETFOREGROUND) };
}

#[cfg(not(windows))]
fn tell(why: &str) {
    eprintln!("{}", message(why));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_says_what_went_wrong_and_where_it_was_kept() {
        let text = message("webview2 couldn't be created");
        assert!(text.starts_with("Q Calc couldn't start.\n\nwebview2 couldn't be created"));
        if let Some(path) = log_path() {
            assert!(text.contains(&path.display().to_string()));
        }
    }
}
