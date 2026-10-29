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
//! A feed that isn't there (HTTP 404: no release carries one yet) is no news: "up to date", again in an hour.
//!
//! Only an installed copy updates (autostart.rs `is_installed`); a dev build's status is "off".
//!
//! **Testing** (`tests/e2e/update-e2e.ps1`): only a build with the cargo feature `update-e2e` reads
//! `QCALC_UPDATE_FEED` (an https URL, or loopback http), `QCALC_UPDATE_PUBKEY` (the key it trusts instead of
//! `PUBKEY`) and `QCALC_UPDATE_QUIET_SECS` (the first check, the quiet wait and the retry, in seconds). A release
//! build ignores all three, so it only ever trusts GitHub's feed and the real key (unit-tested below).
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

// ---- the test build's overrides ----

/// The loop's waits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub first: Duration,
    pub idle: Duration,
    pub poll: Duration,
    pub retry: Duration,
}

pub const TIMING: Timing = Timing {
    first: FIRST_CHECK,
    idle: IDLE,
    poll: QUIET_POLL,
    retry: RETRY_AFTER,
};

/// What the environment may change, read through these so a release build provably can't be pointed elsewhere.
pub mod overrides {
    use super::*;

    /// The feed `QCALC_UPDATE_FEED` names, in an `update-e2e` build only.
    pub fn feed(var: Option<&str>) -> Option<String> {
        if !cfg!(feature = "update-e2e") {
            return None;
        }
        var.map(str::trim)
            .filter(|f| super::crate_split_url(f))
            .map(String::from)
    }

    /// The key the installer must be signed with: `PUBKEY`, or `QCALC_UPDATE_PUBKEY` in an `update-e2e` build.
    pub fn key(var: Option<&str>) -> String {
        match var.map(str::trim).filter(|k| !k.is_empty()) {
            Some(k) if cfg!(feature = "update-e2e") => k.to_string(),
            _ => PUBKEY.to_string(),
        }
    }

    /// `QCALC_UPDATE_QUIET_SECS` (1-3600) for every wait, in an `update-e2e` build only.
    pub fn timing(var: Option<&str>) -> Timing {
        match var.and_then(|v| v.trim().parse::<u64>().ok()) {
            Some(s) if cfg!(feature = "update-e2e") && (1..=3600).contains(&s) => {
                let d = Duration::from_secs(s);
                Timing {
                    first: d,
                    idle: d,
                    poll: Duration::from_secs(1),
                    retry: d,
                }
            }
            _ => TIMING,
        }
    }

    fn var(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    pub fn env_feed() -> Option<String> {
        feed(var("QCALC_UPDATE_FEED").as_deref())
    }

    pub fn env_key() -> String {
        key(var("QCALC_UPDATE_PUBKEY").as_deref())
    }

    pub fn env_timing() -> Timing {
        timing(var("QCALC_UPDATE_QUIET_SECS").as_deref())
    }
}

/// An error as settings shows it after "Couldn't update: ".
pub fn friendly(e: &str) -> String {
    let net = [
        "open:",
        "connect:",
        "request:",
        "send:",
        "response:",
        "status:",
        "read:",
    ];
    if net.iter().any(|p| e.starts_with(p)) {
        return "no connection to the update server".into();
    }
    if let Some(code) = e.strip_prefix("HTTP ") {
        return format!("the update server answered {code}");
    }
    if e.starts_with("the installer's signature doesn't match") {
        return "the download didn't match its signature, so it was thrown away".into();
    }
    e.to_string()
}

/// The Updates page's button: its label and whether it can be pressed; none when this copy can't update.
pub fn button(state: UpdateState) -> Option<(&'static str, bool)> {
    match state {
        UpdateState::Off => None,
        UpdateState::Ready => Some(("Install now", true)),
        UpdateState::Checking | UpdateState::Downloading | UpdateState::Installing => {
            Some(("Check now", false))
        }
        UpdateState::Idle | UpdateState::Current | UpdateState::Failed => Some(("Check now", true)),
    }
}

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
        crate::net::parse_url(u).is_some()
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
#[derive(Debug)]
pub struct Quiet {
    hidden_since: Option<Instant>,
    idle: Duration,
}

impl Default for Quiet {
    fn default() -> Quiet {
        Quiet::new(IDLE)
    }
}

impl Quiet {
    pub fn new(idle: Duration) -> Quiet {
        Quiet {
            hidden_since: None,
            idle,
        }
    }

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
        now.saturating_duration_since(since) >= self.idle
            && input_idle.is_none_or(|i| i >= self.idle)
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
    CONTROL.status.lock().map(|s| s.clone()).unwrap_or_default()
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
        if let Some(f) = overrides::env_feed() {
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
        qlog!("updates: on, feed {feed}");
        crate::bg::spawn("qcalc-update", move || run(&feed));
    }

    /// The bench's `update` (update-e2e.ps1): the state and what the Updates page shows for it.
    pub fn debug(app: &App) -> Value {
        let st = status();
        let auto = app.store.auto_update;
        let line = qcalc_core::app::host_settings::update_line(&st, auto, unix_now());
        let (label, enabled) = button(st.state).unwrap_or(("", false));
        serde_json::json!({
            "version": VERSION, "state": st.state, "latest": st.latest, "error": st.error,
            "checkedAt": st.checked_at, "auto": auto, "line": line,
            "button": label, "enabled": enabled,
        })
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
        Some(Duration::from_millis(u64::from(idle_millis(
            now,
            info.dwTime,
        ))))
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

    /// What a check found.
    enum Found {
        Newer(Ready),
        /// nothing newer (or the one already downloaded)
        Current,
        /// no feed at all (HTTP 404: no release carries one yet)
        NoFeed,
    }

    fn run(feed: &str) {
        let t = overrides::env_timing();
        let key = overrides::env_key();
        wait(t.first);
        let mut last: Option<SystemTime> = None;
        let mut every = CHECK_EVERY;
        let mut ready: Option<Ready> = None;
        let mut quiet = Quiet::new(t.idle);
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
                let found = check(feed, &key, have.as_deref());
                let at = unix_now();
                let current = |s: &mut UpdateStatus| {
                    s.state = if ready.is_some() {
                        UpdateState::Ready
                    } else {
                        UpdateState::Current
                    };
                    s.checked_at = at;
                };
                match found {
                    Ok(Found::Newer(r)) => {
                        let v = r.version.clone();
                        qlog!("updates: {v} downloaded and verified");
                        ready = Some(r);
                        set_status(|s| {
                            (s.state, s.latest, s.checked_at) = (UpdateState::Ready, v, at)
                        });
                    }
                    Ok(Found::Current) => set_status(current),
                    Ok(Found::NoFeed) => {
                        qlog!("updates: no feed yet (HTTP 404), again in {:?}", t.retry);
                        every = t.retry;
                        set_status(current);
                    }
                    Err(e) => {
                        qlog!("updates: check failed: {e}");
                        every = t.retry;
                        set_status(|s| {
                            s.state = if ready.is_some() {
                                UpdateState::Ready
                            } else {
                                UpdateState::Failed
                            };
                            s.error = friendly(&e);
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
                qlog!(
                    "updates: installing {} ({})",
                    r.version,
                    if show { "asked" } else { "quiet moment" }
                );
                crate::bg::post(move |app| install(app, &path, show));
                ready = None;
            }
            let next = if ready.is_some() && auto() {
                t.poll
            } else if auto() {
                last.and_then(|at| at.checked_add(every))
                    .and_then(|due| due.duration_since(SystemTime::now()).ok())
                    .unwrap_or(Duration::from_secs(60))
                    .max(t.poll.min(Duration::from_secs(60)))
            } else {
                CHECK_EVERY
            };
            wait(next);
        }
    }

    /// A newer installer than the running version and `have`, downloaded and verified against `key`.
    fn check(feed: &str, key: &str, have: Option<&str>) -> Result<Found, String> {
        let manifest = match crate::net::get(feed, MAX_MANIFEST) {
            Err(e) if e == "HTTP 404" => return Ok(Found::NoFeed),
            r => r?,
        };
        let offer = offer(&String::from_utf8_lossy(&manifest), platform())?;
        if !newer(&offer.version, VERSION) {
            if offer.version != VERSION {
                qlog!(
                    "updates: the feed offers {}, older than {VERSION}: left alone",
                    offer.version
                );
            }
            return Ok(Found::Current);
        }
        if have == Some(offer.version.as_str()) {
            return Ok(Found::Current);
        }
        let key = public_key(key)?;
        let sig = signature(&offer.signature)?;
        if !signed_for(&sig, &offer.version) {
            return Err(format!("the installer isn't signed for {}", offer.version));
        }
        set_status(|s| (s.state, s.latest) = (UpdateState::Downloading, offer.version.clone()));
        let dir = std::env::temp_dir().join("qcalc-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("Q-Calc-{}-setup.exe", offer.version));
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut stream = key.verify_stream(&sig).ok();
        let got = crate::net::download(&offer.url, &mut file, MAX_INSTALLER, |piece| {
            if let Some(s) = &mut stream {
                s.update(piece);
            }
        });
        drop(file);
        if let Err(e) = got {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
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
        Ok(Found::Newer(Ready {
            version: offer.version,
            path,
        }))
    }

    /// On the UI thread: start the installer (it waits for this process to go) and quit.
    fn install(app: &mut App, path: &std::path::Path, show: bool) {
        let relaunch = if show { "/R=show" } else { "/R=hidden" };
        match std::process::Command::new(path)
            .args(["/S", relaunch])
            .spawn()
        {
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
        assert!(
            offer(
                &m.replace("2.1.0\", \"pub", "two\", \"pub"),
                "windows-x86_64"
            )
            .is_err()
        );
    }

    /// A shipping build (no `update-e2e` feature) can't be pointed at another feed or key, or made to install
    /// sooner, whatever the environment says; the test build takes them, and plain http only to this machine.
    #[test]
    fn a_release_build_trusts_only_githubs_feed_and_the_real_key() {
        let elsewhere = Some("https://example.com/latest-native.json");
        let lo = Some("http://127.0.0.1:8123/latest-native.json");
        let test_build = cfg!(feature = "update-e2e");
        assert_eq!(
            overrides::feed(elsewhere).is_some(),
            test_build,
            "QCALC_UPDATE_FEED"
        );
        assert_eq!(overrides::feed(lo).is_some(), test_build && cfg!(windows));
        assert_eq!(overrides::key(Some("other")) == PUBKEY, !test_build);
        assert_eq!(overrides::timing(Some("3")) == TIMING, !test_build);
        // in either build: never plain http to another machine, nothing set is nothing changed
        assert_eq!(overrides::feed(Some("http://example.com:80/x")), None);
        assert_eq!(overrides::feed(None), None);
        assert_eq!(overrides::key(None), PUBKEY);
        assert_eq!(overrides::key(Some("  ")), PUBKEY);
        assert_eq!(overrides::timing(Some("0")), TIMING);
        assert_eq!(overrides::timing(Some("soon")), TIMING);
        // the feed's installer link too
        let m = r#"{ "version": "9.0.0", "platforms": { "windows-x86_64": { "url": "http://127.0.0.1:8123/Q-Calc-9.0.0-x64-native-setup.exe", "signature": "c2ln" } } }"#;
        assert_eq!(
            offer(m, "windows-x86_64").is_ok(),
            test_build && cfg!(windows)
        );
    }

    #[test]
    fn errors_read_as_sentences_and_the_button_follows_the_state() {
        assert_eq!(
            friendly("send: The server name or address could not be resolved"),
            "no connection to the update server"
        );
        assert_eq!(friendly("HTTP 500"), "the update server answered 500");
        assert_eq!(
            friendly("the installer's signature doesn't match: Signature verification failed"),
            "the download didn't match its signature, so it was thrown away"
        );
        assert_eq!(
            friendly("the installer isn't signed for 2.1.0"),
            "the installer isn't signed for 2.1.0"
        );
        assert_eq!(button(UpdateState::Off), None);
        assert_eq!(button(UpdateState::Ready), Some(("Install now", true)));
        assert_eq!(button(UpdateState::Checking), Some(("Check now", false)));
        assert_eq!(button(UpdateState::Downloading), Some(("Check now", false)));
        assert_eq!(button(UpdateState::Failed), Some(("Check now", true)));
        assert_eq!(button(UpdateState::Current), Some(("Check now", true)));
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
    const TEST_KEY: &str =
        "untrusted comment: test\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n";
    const TEST_SIG: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==\n";

    fn b64(s: &str) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in s.as_bytes().chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..4 {
                out.push(if i <= chunk.len() {
                    A[(n >> (18 - 6 * i) & 63) as usize] as char
                } else {
                    '='
                });
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

    /// The feed exactly as the release writes it (native/windows/installer/latest-native.mjs), with both platforms.
    #[test]
    fn the_release_feed_parses_for_both_platforms() {
        let feed = include_str!("../tests/fixtures/latest-native.json");
        for (platform, file) in [
            ("windows-x86_64", "Q-Calc-2.1.0-x64-native-setup.exe"),
            ("windows-aarch64", "Q-Calc-2.1.0-arm64-native-setup.exe"),
        ] {
            let o = offer(feed, platform).unwrap();
            assert_eq!(o.version, "2.1.0");
            assert_eq!(
                o.url,
                format!("https://github.com/maxconine/Q-Calc/releases/download/v2.1.0/{file}")
            );
            assert!(signed_for(&signature(&o.signature).unwrap(), "2.1.0"));
        }
        assert!(newer(&offer(feed, platform()).unwrap().version, "2.0.15"));
    }

    /// A feed built from a real local installer by native/windows/installer/feed-check.ps1: a throwaway minisign
    /// key signs the installer (`tauri signer sign --app-version`), latest-native.mjs writes the feed, and this
    /// reads it as the app would and verifies the installer against that key, whole and streamed.
    /// `QCALC_FEED_CHECK=<dir with latest-native.json and the installer>`, `QCALC_FEED_PUBKEY=<base64 key>`.
    #[test]
    #[ignore = "run by installer/feed-check.ps1"]
    fn feed_from_a_local_installer() {
        let dir =
            std::path::PathBuf::from(std::env::var("QCALC_FEED_CHECK").expect("QCALC_FEED_CHECK"));
        let pubkey = std::env::var("QCALC_FEED_PUBKEY").expect("QCALC_FEED_PUBKEY");
        let feed = std::fs::read_to_string(dir.join("latest-native.json")).unwrap();
        let o = offer(&feed, platform()).unwrap();
        let file = o.url.rsplit('/').next().unwrap().replace("%20", " ");
        let bytes = std::fs::read(dir.join(&file)).unwrap();
        let key = public_key(pubkey.trim()).unwrap();
        let sig = signature(&o.signature).unwrap();
        assert!(signed_for(&sig, &o.version), "not signed for {}", o.version);
        verify_bytes(&key, &sig, &bytes).unwrap();
        let mut s = key.verify_stream(&sig).unwrap();
        for piece in bytes.chunks(64 * 1024) {
            s.update(piece);
        }
        s.finalize().unwrap();
        // and the real key didn't make it: the shipped app would refuse this one
        assert!(verify_bytes(&public_key(PUBKEY).unwrap(), &sig, &bytes).is_err());
        eprintln!("feed ok: {} {} ({} bytes)", o.version, o.url, bytes.len());
    }

    #[test]
    fn checks_on_launch_then_daily_and_a_backwards_clock_is_due() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(check_due(None, CHECK_EVERY, t0));
        assert!(!check_due(
            Some(t0),
            CHECK_EVERY,
            t0 + Duration::from_secs(60)
        ));
        assert!(check_due(Some(t0), CHECK_EVERY, t0 + CHECK_EVERY));
        assert!(check_due(
            Some(t0),
            CHECK_EVERY,
            t0 - Duration::from_secs(3600)
        ));
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
