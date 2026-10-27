//! Self-update for the native app, on the same GitHub releases and the same signing key as the Tauri updater.
//!
//! **The feed.** Each release carries, beside the Tauri app's `latest.json`, a `latest-native.json` in the same
//! shape (`{version, pub_date, platforms: {"windows-x86_64": {url, signature}, "windows-aarch64": ...}}`), written by
//! native/windows/installer/build.ps1. `signature` is what `tauri signer sign` writes: base64 of a minisign
//! signature of the installer, made with the Tauri updater's key, its trusted comment naming `version:<v>`. The
//! app checks `https://github.com/maxconine/Q-Calc/releases/latest/download/latest-native.json`. A separate file
//! keeps Tauri installs on Tauri builds until the switch is made on purpose; switching them over is pointing
//! `latest.json`'s entries at the native installer (it upgrades a Tauri install in place, keeping settings).
//!
//! **The loop** (a background thread, started after the first frame): about 20 s after launch and then daily,
//! fetch the manifest; a newer version downloads in the background into `%TEMP%\qcalc-update`, hashed as it
//! streams and verified against `PUBKEY` (minisign, Ed25519 over BLAKE2b; legacy signatures re-read and checked
//! whole, as tauri-plugin-updater allows). The trusted comment must name the manifest's version, so an old signed
//! installer can't be passed off as a newer one. A verified installer waits ("ready") until Q Calc has been put
//! away for 5 minutes and there has been no input for 5 minutes, then runs with `/S` (silent) and `/R=hidden`;
//! the app quits, the installer replaces it and starts it again in the tray. From settings, "Install now" runs it
//! at once with `/R=show`. A failed check retries an hour later. Off (`store.auto_update` false), nothing happens
//! until settings asks.
//!
//! Only an installed copy updates (autostart.rs `is_installed`); a dev build's status is "off".
//! `QCALC_UPDATE_FEED=<https URL>` points a build at another manifest and lets it check (testing).
//!
//! The pure parts (manifest, versions, the quiet rule, base64, the signature) are unit-tested.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use qcalc_core::app::host_settings::{UpdateState, UpdateStatus};
use serde_json::Value;

use crate::app::App;
use crate::qlog;

/// The version this build reports and compares: the release's (`QCALC_VERSION` at build time), else the crate's.
pub const VERSION: &str = match option_env!("QCALC_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

pub const FEED: &str =
    "https://github.com/maxconine/Q-Calc/releases/latest/download/latest-native.json";

/// src-tauri/tauri.windows.conf.json's `plugins.updater.pubkey`: base64 of the minisign public key file.
pub const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDUxMjE3NjdFMDRBQzc5REQKUldUZGVhd0VmblloVWJrb2xaQ3grMy9PVlRGQjFzUVpRcm4vOTJhUy9nRnBqYUZNTngyYjY1bEMK";

const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// While an installer waits for a quiet moment, how often the loop looks.
const QUIET_POLL: Duration = Duration::from_secs(60);
pub const IDLE: Duration = Duration::from_secs(5 * 60);
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_INSTALLER: u64 = 200 * 1024 * 1024;

// ---- pure parts ----

/// What the manifest offers this machine.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub version: String,
    pub url: String,
    /// base64 of the minisign signature file
    pub signature: String,
}

/// This build's platform key in the manifest.
pub fn platform() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "windows-aarch64"
    } else {
        "windows-x86_64"
    }
}

/// `1.2.3`, `1.2.3-beta.1`: the numbers and whether it is a pre-release; anything else is none.
pub fn parse_version(v: &str) -> Option<([u64; 3], bool)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once(['-', '+']) {
        Some((c, rest)) => (c, v.as_bytes()[c.len()] == b'-' && !rest.is_empty()),
        None => (v, false),
    };
    let mut parts = core.split('.');
    let mut out = [0u64; 3];
    for slot in &mut out {
        let p = parts.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.len() > 9 {
            return None;
        }
        *slot = p.parse().ok()?;
    }
    parts.next().is_none().then_some((out, pre))
}

/// Whether `offered` is newer than `running` (a release beats its own pre-release).
pub fn newer(offered: &str, running: &str) -> bool {
    match (parse_version(offered), parse_version(running)) {
        (Some((a, a_pre)), Some((b, b_pre))) => a > b || (a == b && b_pre && !a_pre),
        (Some(_), None) => true,
        _ => false,
    }
}

/// The manifest's entry for `platform`, checked.
pub fn offer(manifest: &str, platform: &str) -> Result<Offer, String> {
    let v: Value = serde_json::from_str(manifest.trim_start_matches('\u{feff}'))
        .map_err(|_| "the update feed isn't JSON")?;
    let version = v["version"]
        .as_str()
        .filter(|s| parse_version(s).is_some())
        .ok_or("the update feed has no version")?;
    let p = &v["platforms"][platform];
    let url = p["url"]
        .as_str()
        .filter(|u| crate_split_url(u))
        .ok_or_else(|| format!("the update feed has no installer for {platform}"))?;
    let signature = p["signature"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("the update feed's installer isn't signed")?;
    Ok(Offer {
        version: version.trim_start_matches('v').to_string(),
        url: url.to_string(),
        signature: signature.trim().to_string(),
    })
}

fn crate_split_url(u: &str) -> bool {
    #[cfg(windows)]
    {
        crate::net::split_url(u).is_some()
    }
    #[cfg(not(windows))]
    {
        u.starts_with("https://")
    }
}

/// Standard base64 (padding optional, whitespace ignored).
pub fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        let d = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(d);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// A minisign text (key or signature file) from the base64 Tauri stores it as.
fn minisign_text(b64: &str) -> Result<String, String> {
    base64(b64)
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| "the signature isn't base64 text".into())
}

pub fn public_key(b64_file: &str) -> Result<minisign_verify::PublicKey, String> {
    minisign_verify::PublicKey::decode(&minisign_text(b64_file)?)
        .map_err(|e| format!("bad public key: {e}"))
}

pub fn signature(b64_file: &str) -> Result<minisign_verify::Signature, String> {
    minisign_verify::Signature::decode(&minisign_text(b64_file)?)
        .map_err(|e| format!("bad signature: {e}"))
}

/// The trusted comment names the version the installer was signed for (`version:<v>`, tab-separated fields).
pub fn signed_for(sig: &minisign_verify::Signature, version: &str) -> bool {
    sig.trusted_comment()
        .split('\t')
        .any(|f| f.trim().strip_prefix("version:") == Some(version))
}

/// The whole-file check: prehashed signatures, and legacy ones as tauri-plugin-updater allows.
pub fn verify_bytes(
    key: &minisign_verify::PublicKey,
    sig: &minisign_verify::Signature,
    bytes: &[u8],
) -> Result<(), String> {
    key.verify(bytes, sig, true)
        .map_err(|e| format!("the installer's signature doesn't match: {e}"))
}

/// When the check after `last` is due; a clock that jumped back counts as due.
pub fn check_due(last: Option<SystemTime>, wait: Duration, now: SystemTime) -> bool {
    match last {
        None => true,
        Some(at) => now.duration_since(at).is_ok_and(|d| d >= wait) || now < at,
    }
}

/// Milliseconds since the last input; GetTickCount wraps every 49.7 days.
pub fn idle_millis(tick_now: u32, last_input: u32) -> u32 {
    tick_now.wrapping_sub(last_input)
}

/// Put away for IDLE and no keyboard or mouse for IDLE: the moment an automatic install may run.
#[derive(Default, Debug)]
pub struct Quiet {
    hidden_since: Option<Instant>,
}

impl Quiet {
    pub fn observe(&mut self, visible: bool, now: Instant) {
        if visible {
            self.hidden_since = None;
        } else if self.hidden_since.is_none() {
            self.hidden_since = Some(now);
        }
    }

    pub fn ready(&self, now: Instant, input_idle: Option<Duration>) -> bool {
        let Some(since) = self.hidden_since else {
            return false;
        };
        now.saturating_duration_since(since) >= IDLE && input_idle.is_none_or(|i| i >= IDLE)
    }
}

// ---- the shared state ----

struct Control {
    status: Mutex<UpdateStatus>,
    wake: Condvar,
    check: AtomicBool,
    install: AtomicBool,
    auto: AtomicBool,
    started: AtomicBool,
    bar: AtomicUsize,
}

static CONTROL: Control = Control {
    status: Mutex::new(UpdateStatus {
        state: UpdateState::Off,
        latest: String::new(),
        checked_at: 0.0,
        error: String::new(),
    }),
    wake: Condvar::new(),
    check: AtomicBool::new(false),
    install: AtomicBool::new(false),
    auto: AtomicBool::new(true),
    started: AtomicBool::new(false),
    bar: AtomicUsize::new(0),
};

/// What settings shows.
pub fn status() -> UpdateStatus {
    CONTROL
        .status
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default()
}

fn set_status(change: impl FnOnce(&mut UpdateStatus)) {
    let changed = match CONTROL.status.lock() {
        Ok(mut s) => {
            let before = s.clone();
            change(&mut s);
            *s != before
        }
        Err(_) => false,
    };
    if changed {
        #[cfg(windows)]
        crate::bg::post(crate::settings_window::refresh);
    }
}

fn nudge() {
    if let Ok(_s) = CONTROL.status.lock() {
        CONTROL.wake.notify_all();
    }
}

pub fn ask_check() {
    CONTROL.check.store(true, Ordering::SeqCst);
    nudge();
}

pub fn ask_install() {
    CONTROL.install.store(true, Ordering::SeqCst);
    nudge();
}

/// Off, nothing is checked or installed until settings asks.
pub fn set_auto(on: bool) {
    CONTROL.auto.store(on, Ordering::SeqCst);
    nudge();
}

fn auto() -> bool {
    CONTROL.auto.load(Ordering::SeqCst)
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs() as f64)
}

/// Sleeps up to `d`, or until settings asks for something.
fn wait(d: Duration) {
    let until = Instant::now() + d;
    let Ok(mut guard) = CONTROL.status.lock() else {
        return;
    };
    loop {
        if CONTROL.check.load(Ordering::SeqCst) || CONTROL.install.load(Ordering::SeqCst) {
            return;
        }
        let now = Instant::now();
        if now >= until {
            return;
        }
        match CONTROL.wake.wait_timeout(guard, until - now) {
            Ok((g, _)) => guard = g,
            Err(_) => return,
        }
    }
}

// ---- the loop (Windows) ----

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use super::*;
    use std::io::Read;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;

    fn feed() -> Option<String> {
        if let Some(f) = std::env::var("QCALC_UPDATE_FEED")
            .ok()
            .filter(|f| crate::net::split_url(f).is_some())
        {
            return Some(f);
        }
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        crate::autostart::is_installed(&exe, &crate::autostart::Roots::from_env())
            .then(|| FEED.to_string())
    }

    /// After the first frame (bg.rs). A build that can't update stays "off".
    pub fn start(app: &mut App) {
        set_auto(app.store.auto_update);
        CONTROL
            .bar
            .store(app.bar.hwnd.0 as usize, Ordering::Relaxed);
        let Some(feed) = feed() else {
            qlog!("updates: off (not an installed copy)");
            return;
        };
        if CONTROL.started.swap(true, Ordering::SeqCst) {
            return;
        }
        set_status(|s| s.state = UpdateState::Idle);
        crate::bg::spawn("qcalc-update", move || run(&feed));
    }

    fn input_idle() -> Option<Duration> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: a sized local
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return None;
        }
        // SAFETY: plain query
        let now = unsafe { GetTickCount() };
        Some(Duration::from_millis(u64::from(idle_millis(now, info.dwTime))))
    }

    fn bar_visible() -> bool {
        let bar = CONTROL.bar.load(Ordering::Relaxed);
        // SAFETY: IsWindowVisible only reads the window's state, from any thread
        bar != 0 && unsafe { IsWindowVisible(HWND(bar as *mut _)) }.as_bool()
    }

    struct Ready {
        version: String,
        path: std::path::PathBuf,
    }

    fn run(feed: &str) {
        wait(FIRST_CHECK);
        let mut last: Option<SystemTime> = None;
        let mut every = CHECK_EVERY;
        let mut ready: Option<Ready> = None;
        let mut quiet = Quiet::default();
        loop {
            let asked = CONTROL.check.swap(false, Ordering::SeqCst);
            if asked || (auto() && check_due(last, every, SystemTime::now())) {
                last = Some(SystemTime::now());
                every = CHECK_EVERY;
                set_status(|s| {
                    s.state = UpdateState::Checking;
                    s.error.clear();
                });
                let have = ready.as_ref().map(|r| r.version.clone());
                let found = check(feed, have.as_deref());
                let at = unix_now();
                match found {
                    Ok(Some(r)) => {
                        let v = r.version.clone();
                        qlog!("updates: {v} downloaded and verified");
                        ready = Some(r);
                        set_status(|s| {
                            (s.state, s.latest, s.checked_at) = (UpdateState::Ready, v, at)
                        });
                    }
                    Ok(None) => set_status(|s| {
                        s.state = if ready.is_some() {
                            UpdateState::Ready
                        } else {
                            UpdateState::Current
                        };
                        s.checked_at = at;
                    }),
                    Err(e) => {
                        qlog!("updates: check failed: {e}");
                        every = RETRY_AFTER;
                        set_status(|s| {
                            s.state = if ready.is_some() {
                                UpdateState::Ready
                            } else {
                                UpdateState::Failed
                            };
                            s.error = e;
                            s.checked_at = at;
                        });
                    }
                }
            }
            quiet.observe(bar_visible(), Instant::now());
            let now_asked = CONTROL.install.swap(false, Ordering::SeqCst);
            if let Some(r) = &ready
                && (now_asked || (auto() && quiet.ready(Instant::now(), input_idle())))
            {
                set_status(|s| s.state = UpdateState::Installing);
                let (path, show) = (r.path.clone(), now_asked);
                qlog!("updates: installing {} ({})", r.version, if show { "asked" } else { "quiet moment" });
                crate::bg::post(move |app| install(app, &path, show));
                ready = None;
            }
            let next = if ready.is_some() && auto() {
                QUIET_POLL
            } else if auto() {
                last.and_then(|at| at.checked_add(every))
                    .and_then(|due| due.duration_since(SystemTime::now()).ok())
                    .unwrap_or(Duration::from_secs(60))
                    .max(Duration::from_secs(60))
            } else {
                CHECK_EVERY
            };
            wait(next);
        }
    }

    /// A newer installer than `have`, downloaded and verified; None when there's nothing newer.
    fn check(feed: &str, have: Option<&str>) -> Result<Option<Ready>, String> {
        let manifest = crate::net::get(feed, MAX_MANIFEST)?;
        let offer = offer(&String::from_utf8_lossy(&manifest), platform())?;
        if !newer(&offer.version, VERSION) || have == Some(offer.version.as_str()) {
            return Ok(None);
        }
        let key = public_key(PUBKEY)?;
        let sig = signature(&offer.signature)?;
        if !signed_for(&sig, &offer.version) {
            return Err(format!(
                "the installer isn't signed for {}",
                offer.version
            ));
        }
        set_status(|s| {
            (s.state, s.latest) = (UpdateState::Downloading, offer.version.clone())
        });
        let dir = std::env::temp_dir().join("qcalc-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("Q-Calc-{}-setup.exe", offer.version));
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut stream = key.verify_stream(&sig).ok();
        crate::net::download(&offer.url, &mut file, MAX_INSTALLER, |piece| {
            if let Some(s) = &mut stream {
                s.update(piece);
            }
        })?;
        drop(file);
        let verified = match &mut stream {
            Some(s) => s
                .finalize()
                .map_err(|e| format!("the installer's signature doesn't match: {e}")),
            // a legacy (not prehashed) signature: the whole file at once
            None => {
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .and_then(|mut f| f.read_to_end(&mut bytes))
                    .map_err(|e| e.to_string())?;
                verify_bytes(&key, &sig, &bytes)
            }
        };
        if let Err(e) = verified {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        Ok(Some(Ready {
            version: offer.version,
            path,
        }))
    }

    /// On the UI thread: start the installer (it waits for this process to go) and quit.
    fn install(app: &mut App, path: &std::path::Path, show: bool) {
        let relaunch = if show { "/R=show" } else { "/R=hidden" };
        match std::process::Command::new(path).args(["/S", relaunch]).spawn() {
            Ok(_) => {
                qlog!("updates: installer started, quitting");
                app.save_store();
                app.ops.push(crate::app::Op::Quit);
            }
            Err(e) => {
                qlog!("updates: couldn't start the installer: {e}");
                set_status(|s| {
                    s.state = UpdateState::Failed;
                    s.error = e.to_string();
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_releases() {
        assert!(newer("2.0.16", "2.0.15"));
        assert!(newer("2.1.0", "2.0.99"));
        assert!(newer("10.0.0", "9.9.9"));
        assert!(!newer("2.0.15", "2.0.15"));
        assert!(!newer("2.0.14", "2.0.15"));
        assert!(newer("2.0.15", "2.0.15-beta.2"));
        assert!(!newer("2.0.15-beta.2", "2.0.15"));
        assert!(newer("v2.0.16", "2.0.15"));
        assert!(!newer("2.0", "1.0.0"));
        assert!(!newer("2.0.x", "1.0.0"));
        assert!(newer("0.2.0", "0.1.0"));
        assert_eq!(parse_version("1.2.3+build"), Some(([1, 2, 3], false)));
    }

    #[test]
    fn the_manifest_names_this_platforms_installer() {
        let m = r#"{ "version": "2.1.0", "pub_date": "2026-10-27T00:00:00Z", "platforms": {
            "windows-x86_64": { "url": "https://github.com/maxconine/Q-Calc/releases/download/v2.1.0/Q-Calc-2.1.0-x64-native-setup.exe", "signature": "c2ln" } } }"#;
        let o = offer(m, "windows-x86_64").unwrap();
        assert_eq!(o.version, "2.1.0");
        assert!(o.url.ends_with("x64-native-setup.exe"));
        assert!(offer(m, "windows-aarch64").is_err());
        assert!(offer("<html>", "windows-x86_64").is_err());
        // a byte-order mark (PowerShell 5's UTF-8) is fine
        assert!(offer(&format!("\u{feff}{m}"), "windows-x86_64").is_ok());
        assert!(offer(&m.replace("https://", "http://"), "windows-x86_64").is_err());
        assert!(offer(&m.replace("\"c2ln\"", "\"\""), "windows-x86_64").is_err());
        assert!(offer(&m.replace("2.1.0\", \"pub", "two\", \"pub"), "windows-x86_64").is_err());
    }

    #[test]
    fn base64_decodes() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64("aGVs\nbG8=").unwrap(), b"hello");
        assert_eq!(base64("").unwrap(), b"");
        assert!(base64("a*b").is_none());
    }

    #[test]
    fn the_shipped_key_is_the_tauri_updaters() {
        let text = minisign_text(PUBKEY).unwrap();
        assert!(text.starts_with("untrusted comment: minisign public key: 5121767E04AC79DD"));
        assert!(public_key(PUBKEY).is_ok());
    }

    // minisign-verify's own test vector: the prehashed signature of b"test"
    const TEST_KEY: &str = "untrusted comment: test\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n";
    const TEST_SIG: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==\n";

    fn b64(s: &str) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in s.as_bytes().chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..4 {
                out.push(if i <= chunk.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
            }
        }
        out
    }

    #[test]
    fn signatures_verify_whole_and_streamed_and_name_their_version() {
        let key = public_key(&b64(TEST_KEY)).unwrap();
        let sig = signature(&b64(TEST_SIG)).unwrap();
        assert!(verify_bytes(&key, &sig, b"test").is_ok());
        assert!(verify_bytes(&key, &sig, b"tess").is_err());
        let mut s = key.verify_stream(&sig).unwrap();
        s.update(b"te");
        s.update(b"st");
        assert!(s.finalize().is_ok());
        // the vector's trusted comment names no version, so it isn't an installer signed for one
        assert!(!signed_for(&sig, "2.1.0"));
        // and the Tauri key isn't the one that made it
        assert!(verify_bytes(&public_key(PUBKEY).unwrap(), &sig, b"test").is_err());
    }

    #[test]
    fn checks_on_launch_then_daily_and_a_backwards_clock_is_due() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(check_due(None, CHECK_EVERY, t0));
        assert!(!check_due(Some(t0), CHECK_EVERY, t0 + Duration::from_secs(60)));
        assert!(check_due(Some(t0), CHECK_EVERY, t0 + CHECK_EVERY));
        assert!(check_due(Some(t0), CHECK_EVERY, t0 - Duration::from_secs(3600)));
    }

    #[test]
    fn installs_only_when_put_away_and_idle() {
        let t0 = Instant::now();
        let mut q = Quiet::default();
        q.observe(true, t0);
        assert!(!q.ready(t0 + IDLE * 10, Some(IDLE * 10)));
        q.observe(false, t0);
        assert!(!q.ready(t0 + IDLE - Duration::from_secs(1), Some(IDLE)));
        assert!(q.ready(t0 + IDLE, Some(IDLE)));
        assert!(!q.ready(t0 + IDLE * 3, Some(Duration::from_secs(10))));
        q.observe(true, t0 + IDLE * 4);
        q.observe(false, t0 + IDLE * 4);
        assert!(!q.ready(t0 + IDLE * 4 + Duration::from_secs(5), Some(IDLE * 9)));
        assert_eq!(idle_millis(1_000, u32::MAX - 999), 2_000);
    }
}
