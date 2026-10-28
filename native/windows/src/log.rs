//! The log: `%LOCALAPPDATA%\com.maxconine.qcalc\qcalc.log`, the same file the Tauri app writes (crash.rs there), so
//! one place says what every launch did. Each line is `<unix seconds> [native +<ms since process start>] <text>`,
//! the ms measured from the process's creation time (GetProcessTimes), so a script can read warm launch straight
//! from the `ready` line. The file starts over past 256 KB. A panic is logged too, and one before the app is up
//! says so in a message box (a windows-subsystem app has no console).
//!
//! Never log per keystroke: the log is for launches, hotkey attempts, failures and the bench.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 256 * 1024;

static STARTED: AtomicBool = AtomicBool::new(false);
/// (an Instant, the ms since process creation at that Instant)
static ORIGIN: OnceLock<(Instant, f64)> = OnceLock::new();

pub fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("com.maxconine.qcalc"))
}

pub fn path() -> Option<PathBuf> {
    // QCALC_LOG lets a test or a script keep its own log
    if let Some(p) = std::env::var_os("QCALC_LOG") {
        return Some(PathBuf::from(p));
    }
    dir().map(|d| d.join("qcalc.log"))
}

/// Milliseconds since the process was created (not since main), so launch timings include the loader.
pub fn since_start_ms() -> f64 {
    let (at, base) = *ORIGIN.get_or_init(|| (Instant::now(), process_age_ms()));
    base + at.elapsed().as_secs_f64() * 1000.0
}

#[cfg(windows)]
fn process_age_ms() -> f64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let ticks = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    // SAFETY: the pseudo handle needs no closing, and the out pointers are live locals
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok.is_err() {
        return 0.0;
    }
    // SAFETY: no arguments; returns the current time
    let now = unsafe { GetSystemTimePreciseAsFileTime() };
    ticks(now).saturating_sub(ticks(created)) as f64 / 10_000.0
}

#[cfg(not(windows))]
fn process_age_ms() -> f64 {
    0.0
}

pub fn log(line: &str) {
    let ms = since_start_ms();
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let full = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(&path);
    if let Ok(mut file) = file {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let _ = writeln!(file, "{at} [native +{ms:.1}ms] {line}");
    }
    if cfg!(debug_assertions) {
        eprintln!("[qcalc +{ms:.1}ms] {line}");
    }
}

/// `log!("x {}", y)`
#[macro_export]
macro_rules! qlog {
    ($($t:tt)*) => { $crate::log::log(&format!($($t)*)) };
}

/// Before anything else in main, so a panic anywhere after is kept.
pub fn install() {
    since_start_ms();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(&format!("panic: {info}"));
        if !STARTED.load(Ordering::SeqCst) {
            tell(&format!("Q Calc couldn't start.\n\n{info}"));
        }
        previous(info);
    }));
}

/// The app is up: later panics are logged without a message box.
pub fn started() {
    STARTED.store(true, Ordering::SeqCst);
}

#[cfg(windows)]
pub fn tell(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };
    use windows::core::HSTRING;
    // SAFETY: both strings outlive the call
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from("Q Calc"),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        )
    };
}

#[cfg(not(windows))]
pub fn tell(text: &str) {
    eprintln!("{text}");
}

/// `-<name>` when QCALC_INSTANCE is set, else empty: a dev copy run with it takes its own single-instance mutex and
/// bench pipe (`qcalc-bench-<name>`), so several test copies can run side by side without handing off to each other.
pub fn instance_suffix() -> &'static str {
    static SUFFIX: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SUFFIX.get_or_init(|| match std::env::var("QCALC_INSTANCE") {
        Ok(name) if !name.is_empty() => format!("-{name}"),
        _ => String::new(),
    })
}
