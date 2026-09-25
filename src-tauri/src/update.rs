// silent updates on windows: check at launch and then daily, download in the background,
// and install only once the calculator is hidden and the user has been idle for a while.
// the installer runs quietly and relaunches the app, which always starts hidden.
#![cfg_attr(any(not(windows), debug_assertions), allow(dead_code))]

use std::time::{Duration, Instant, SystemTime};

use tauri::plugin::TauriPlugin;
use tauri::Runtime;

const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
const TICK: Duration = Duration::from_secs(30);
const IDLE: Duration = Duration::from_secs(5 * 60);
pub const PLACEHOLDER_KEY: &str = "QCALC_UPDATER_PUBKEY_PLACEHOLDER";

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
        std::thread::sleep(FIRST_CHECK);
        let cleanup = app.clone();
        let updater = match app
            .updater_builder()
            .timeout(Duration::from_secs(10 * 60))
            .on_before_exit(move || cleanup.cleanup_before_exit())
            .build()
        {
            Ok(updater) => updater,
            Err(err) => return eprintln!("updates off: {err}"),
        };

        let mut last_check: Option<SystemTime> = None;
        let mut wait = CHECK_EVERY;
        let mut ready: Option<(Update, Vec<u8>)> = None;
        let mut quiet = Quiet::default();
        loop {
            if check_due(last_check, wait, SystemTime::now()) {
                last_check = Some(SystemTime::now());
                wait = CHECK_EVERY;
                match tauri::async_runtime::block_on(fetch(&updater, ready.as_ref().map(|(u, _)| u.version.clone()))) {
                    Ok(Some(found)) => ready = Some(found),
                    Ok(None) => {}
                    Err(err) => {
                        eprintln!("update check failed: {err}");
                        wait = RETRY_AFTER;
                    }
                }
            }

            let visible = app.get_webview_window("main").map_or(false, |w| w.is_visible().unwrap_or(false));
            quiet.observe(visible, Instant::now());
            if ready.is_some() && quiet.ready(Instant::now(), input_idle()) {
                if let Some((update, bytes)) = ready.take() {
                    // exits the process once the installer starts; the installer relaunches us
                    if let Err(err) = update.install(&bytes) {
                        eprintln!("update install failed: {err}");
                    }
                }
            }
            std::thread::sleep(TICK);
        }
    }

    // a newer version than the one already downloaded, verified against the public key
    async fn fetch(
        updater: &tauri_plugin_updater::Updater,
        have: Option<String>,
    ) -> tauri_plugin_updater::Result<Option<(Update, Vec<u8>)>> {
        let Some(update) = updater.check().await? else { return Ok(None) };
        if have.as_deref() == Some(update.version.as_str()) {
            return Ok(None);
        }
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
    fn placeholder_key_is_caught() {
        assert!(is_placeholder(PLACEHOLDER_KEY));
        assert!(is_placeholder(""));
        assert!(is_placeholder("  "));
        assert!(!is_placeholder("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFCQ0QK"));
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
