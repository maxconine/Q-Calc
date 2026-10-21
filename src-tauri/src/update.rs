// silent updates on windows: check at launch and then daily, download in the background,
// and install only once the calculator is hidden and the user has been idle for a while.
// the installer runs quietly and relaunches the app, which always starts hidden.
// the settings window shows how that's going and can ask for a check or an install now, or turn the automatic ones off
#![cfg_attr(any(not(windows), debug_assertions), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use tauri::plugin::TauriPlugin;
use tauri::Runtime;

const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
const TICK: Duration = Duration::from_secs(30);
// how often the loop looks for a check or an install asked for from settings while it waits out a tick
const NUDGE: Duration = Duration::from_millis(250);
const IDLE: Duration = Duration::from_secs(5 * 60);
pub const PLACEHOLDER_KEY: &str = "QCALC_UPDATER_PUBKEY_PLACEHOLDER";

// left in the config folder just before an update installs, and taken by the launch that follows it
pub const UPDATED_MARKER: &str = "updated";
// the installer relaunches q calc within moments; a marker older than this was never taken by that launch (the
// install failed, or didn't relaunch), and must not keep someone opening q calc by hand in the tray
const MARKER_FRESH: Duration = Duration::from_secs(10 * 60);

// whether this launch is the one an update made: takes the marker either way, so it counts at most once
pub fn take_marker(dir: &std::path::Path, now: SystemTime) -> bool {
    let marker = dir.join(UPDATED_MARKER);
    let made = std::fs::metadata(&marker).and_then(|m| m.modified());
    std::fs::remove_file(&marker).is_ok() && made.is_ok_and(|made| fresh(made, now))
}

// a clock set back counts as stale
fn fresh(made: SystemTime, now: SystemTime) -> bool {
    now.duration_since(made).is_ok_and(|age| age < MARKER_FRESH)
}

// what the settings window shows. `state` is one of: off (a build that can't update), idle (nothing checked yet),
// checking, current, downloading, ready (downloaded, waiting to install), installing, failed
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: &'static str,
    // the newer version found, while there's one
    pub latest: String,
    // unix seconds of the last check that finished; 0 before any
    pub checked_at: u64,
    pub error: String,
}

struct Control {
    status: Mutex<Status>,
    check: AtomicBool,
    install: AtomicBool,
    auto: AtomicBool,
    notify: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

fn control() -> &'static Control {
    static CONTROL: OnceLock<Control> = OnceLock::new();
    CONTROL.get_or_init(|| Control {
        status: Mutex::new(Status { state: "off", latest: String::new(), checked_at: 0, error: String::new() }),
        check: AtomicBool::new(false),
        install: AtomicBool::new(false),
        auto: AtomicBool::new(true),
        notify: OnceLock::new(),
    })
}

pub fn status() -> Status {
    control().status.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_status(change: impl FnOnce(&mut Status)) {
    let changed = {
        let mut status = control().status.lock().unwrap_or_else(|e| e.into_inner());
        let before = status.clone();
        change(&mut status);
        *status != before
    };
    if changed {
        if let Some(notify) = control().notify.get() {
            notify();
        }
    }
}

// told each time the status changes, so the settings window can show it
pub fn on_change(notify: impl Fn() + Send + Sync + 'static) {
    let _ = control().notify.set(Box::new(notify));
}

pub fn ask_check() {
    control().check.store(true, Ordering::SeqCst);
}

pub fn ask_install() {
    control().install.store(true, Ordering::SeqCst);
}

// off, nothing is checked or installed until settings asks
pub fn set_auto(on: bool) {
    control().auto.store(on, Ordering::SeqCst);
}

fn auto() -> bool {
    control().auto.load(Ordering::SeqCst)
}

fn unix(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("qcalc-update")
        .setup(|_app, _| {
            #[cfg(all(windows, not(debug_assertions)))]
            live::start(_app);
            Ok(())
        })
        .build()
}

pub fn is_placeholder(pubkey: &str) -> bool {
    pubkey.trim().is_empty() || pubkey.contains(PLACEHOLDER_KEY)
}

// a clock that jumped backwards counts as due, so a bad clock can't stall updates forever
pub fn check_due(last: Option<SystemTime>, wait: Duration, now: SystemTime) -> bool {
    match last {
        None => true,
        Some(at) => now.duration_since(at).map_or(true, |since| since >= wait),
    }
}

// milliseconds since the last input; GetTickCount wraps every 49.7 days
pub fn idle_millis(tick_now: u32, last_input: u32) -> u32 {
    tick_now.wrapping_sub(last_input)
}

#[derive(Default)]
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

    // hidden for IDLE and no keyboard or mouse input for IDLE; unknown input idle leans on hidden time alone
    pub fn ready(&self, now: Instant, input_idle: Option<Duration>) -> bool {
        let Some(since) = self.hidden_since else { return false };
        now.saturating_duration_since(since) >= IDLE && input_idle.map_or(true, |idle| idle >= IDLE)
    }
}

#[cfg(all(windows, not(debug_assertions)))]
mod live {
    use super::*;
    use tauri::{AppHandle, Manager};
    use tauri_plugin_updater::{Update, UpdaterExt};

    pub fn start<R: Runtime>(app: &AppHandle<R>) {
        let pubkey = app.config().plugins.0.get("updater").and_then(|u| u["pubkey"].as_str()).unwrap_or("");
        if is_placeholder(pubkey) {
            eprintln!("updates off: the updater public key is still the placeholder");
            return;
        }
        let app = app.clone();
        std::thread::spawn(move || run(app));
    }

    fn run<R: Runtime>(app: AppHandle<R>) {
        // registered off the setup thread: the plugin store stays locked while plugins set up
        if let Err(err) = app.plugin(tauri_plugin_updater::Builder::new().build()) {
            return eprintln!("updates off: {err}");
        }
        set_status(|s| s.state = "idle");
        wait_or_asked(FIRST_CHECK);
        let cleanup = app.clone();
        let updater = match app
            .updater_builder()
            .timeout(Duration::from_secs(10 * 60))
            .on_before_exit(move || cleanup.cleanup_before_exit())
            .build()
        {
            Ok(updater) => updater,
            Err(err) => {
                set_status(|s| (s.state, s.error) = ("off", err.to_string()));
                return eprintln!("updates off: {err}");
            }
        };

        let mut last_check: Option<SystemTime> = None;
        let mut wait = CHECK_EVERY;
        let mut ready: Option<(Update, Vec<u8>)> = None;
        let mut quiet = Quiet::default();
        loop {
            let asked = control().check.swap(false, Ordering::SeqCst);
            if asked || (auto() && check_due(last_check, wait, SystemTime::now())) {
                last_check = Some(SystemTime::now());
                wait = CHECK_EVERY;
                set_status(|s| (s.state, s.error) = ("checking", String::new()));
                let have = ready.as_ref().map(|(u, _)| u.version.clone());
                let found = tauri::async_runtime::block_on(fetch(&updater, have, |version| {
                    set_status(|s| (s.state, s.latest) = ("downloading", version.to_string()));
                }));
                let at = unix(SystemTime::now());
                match found {
                    Ok(Some(found)) => {
                        let version = found.0.version.clone();
                        ready = Some(found);
                        set_status(|s| (s.state, s.latest, s.checked_at) = ("ready", version, at));
                    }
                    Ok(None) => set_status(|s| {
                        s.state = if ready.is_some() { "ready" } else { "current" };
                        s.checked_at = at;
                    }),
                    Err(err) => {
                        eprintln!("update check failed: {err}");
                        wait = RETRY_AFTER;
                        let error = err.to_string();
                        set_status(|s| {
                            s.state = if ready.is_some() { "ready" } else { "failed" };
                            s.error = error;
                            s.checked_at = at;
                        });
                    }
                }
            }

            let visible = app.get_webview_window("main").map_or(false, |w| w.is_visible().unwrap_or(false));
            quiet.observe(visible, Instant::now());
            // asked for from settings, or the usual quiet moment when updates are automatic
            let now = control().install.swap(false, Ordering::SeqCst);
            if ready.is_some() && (now || (auto() && quiet.ready(Instant::now(), input_idle()))) {
                if let Some((update, bytes)) = ready.take() {
                    set_status(|s| s.state = "installing");
                    // exits the process once the installer starts; the installer relaunches us. the marker tells
                    // that launch it was an update, so it stays in the tray rather than opening the bar; one asked
                    // for opens the bar as any launch does, so it's plain the update went in
                    if !now {
                        if let Ok(dir) = app.path().app_config_dir() {
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = std::fs::write(dir.join(UPDATED_MARKER), "");
                        }
                    }
                    if let Err(err) = update.install(&bytes) {
                        crate::crash::log(&format!("update install failed: {err}"));
                        if let Ok(dir) = app.path().app_config_dir() {
                            let _ = std::fs::remove_file(dir.join(UPDATED_MARKER));
                        }
                        let error = err.to_string();
                        set_status(|s| (s.state, s.error) = ("failed", error));
                    }
                }
            }
            wait_or_asked(TICK);
        }
    }

    // a tick, cut short when settings asks for a check or an install
    fn wait_or_asked(tick: Duration) {
        let until = Instant::now() + tick;
        while Instant::now() < until {
            if control().check.load(Ordering::SeqCst) || control().install.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(NUDGE);
        }
    }

    // a newer version than the one already downloaded, verified against the public key
    async fn fetch(
        updater: &tauri_plugin_updater::Updater,
        have: Option<String>,
        found: impl Fn(&str),
    ) -> tauri_plugin_updater::Result<Option<(Update, Vec<u8>)>> {
        let Some(update) = updater.check().await? else { return Ok(None) };
        if have.as_deref() == Some(update.version.as_str()) {
            return Ok(None);
        }
        found(&update.version);
        let bytes = update.download(|_, _| {}, || {}).await?;
        Ok(Some((update, bytes)))
    }

    fn input_idle() -> Option<Duration> {
        use windows_sys::Win32::System::SystemInformation::GetTickCount;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        Some(Duration::from_millis(idle_millis(unsafe { GetTickCount() }, info.dwTime) as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_settings_shows_starts_off_and_says_what_changed() {
        let s = status();
        assert_eq!(serde_json::to_value(&s).unwrap()["checkedAt"], 0);
        set_status(|s| (s.state, s.latest) = ("ready", "9.9.9".to_string()));
        assert_eq!((status().state, status().latest.as_str()), ("ready", "9.9.9"));
        ask_check();
        assert!(control().check.swap(false, Ordering::SeqCst));
        assert!(!control().check.load(Ordering::SeqCst));
        set_auto(false);
        assert!(!auto());
        set_auto(true);
    }

    #[test]
    fn placeholder_key_is_caught() {
        assert!(is_placeholder(PLACEHOLDER_KEY));
        assert!(is_placeholder(""));
        assert!(is_placeholder("  "));
        assert!(!is_placeholder("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFCQ0QK"));
    }

    #[test]
    fn only_a_fresh_marker_is_an_update() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(fresh(t0, t0 + Duration::from_secs(5)));
        assert!(!fresh(t0, t0 + MARKER_FRESH));
        // a clock set back
        assert!(!fresh(t0, t0 - Duration::from_secs(5)));
    }

    #[test]
    fn the_marker_is_taken_once_and_a_stale_one_still_goes() {
        let dir = std::env::temp_dir().join(format!("qcalc-marker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(UPDATED_MARKER), "").unwrap();
        assert!(take_marker(&dir, SystemTime::now()));
        assert!(!take_marker(&dir, SystemTime::now()));
        // left behind by an install that never relaunched: this launch is someone opening q calc
        std::fs::write(dir.join(UPDATED_MARKER), "").unwrap();
        assert!(!take_marker(&dir, SystemTime::now() + Duration::from_secs(3600)));
        assert!(!dir.join(UPDATED_MARKER).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn checks_on_launch_then_daily() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(check_due(None, CHECK_EVERY, t0));
        assert!(!check_due(Some(t0), CHECK_EVERY, t0 + Duration::from_secs(60)));
        assert!(!check_due(Some(t0), CHECK_EVERY, t0 + CHECK_EVERY - Duration::from_secs(1)));
        assert!(check_due(Some(t0), CHECK_EVERY, t0 + CHECK_EVERY));
        assert!(check_due(Some(t0), RETRY_AFTER, t0 + RETRY_AFTER));
    }

    #[test]
    fn clock_going_backwards_is_due() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(check_due(Some(t0), CHECK_EVERY, t0 - Duration::from_secs(3600)));
    }

    #[test]
    fn idle_survives_tick_wrap() {
        assert_eq!(idle_millis(5_000, 2_000), 3_000);
        assert_eq!(idle_millis(1_000, u32::MAX - 999), 2_000);
        assert_eq!(idle_millis(7, 7), 0);
    }

    #[test]
    fn never_ready_while_visible() {
        let t0 = Instant::now();
        let mut q = Quiet::default();
        q.observe(true, t0);
        assert!(!q.ready(t0 + IDLE * 10, Some(IDLE * 10)));
        assert!(!q.ready(t0 + IDLE * 10, None));
    }

    #[test]
    fn ready_after_hidden_and_idle() {
        let t0 = Instant::now();
        let mut q = Quiet::default();
        q.observe(false, t0);
        assert!(!q.ready(t0 + IDLE - Duration::from_secs(1), Some(IDLE)));
        assert!(q.ready(t0 + IDLE, Some(IDLE)));
        assert!(q.ready(t0 + IDLE, None));
    }

    #[test]
    fn recent_input_holds_the_install() {
        let t0 = Instant::now();
        let mut q = Quiet::default();
        q.observe(false, t0);
        assert!(!q.ready(t0 + IDLE * 3, Some(Duration::from_secs(10))));
    }

    #[test]
    fn showing_resets_the_hidden_clock() {
        let t0 = Instant::now();
        let mut q = Quiet::default();
        q.observe(false, t0);
        q.observe(false, t0 + IDLE);
        q.observe(true, t0 + IDLE * 2);
        q.observe(false, t0 + IDLE * 2 + Duration::from_secs(1));
        assert!(!q.ready(t0 + IDLE * 3, Some(IDLE * 3)));
        assert!(q.ready(t0 + IDLE * 3 + Duration::from_secs(1), Some(IDLE * 3)));
    }
}
