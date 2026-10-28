#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crash;
mod e2e;
mod frame;
mod hotkey;
mod icon;
mod place;
mod sheet;
mod rates;
mod recorder;
mod startup;
mod store;
mod update;

use std::path::PathBuf;
use std::sync::{mpsc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::PageLoadEvent;
use tauri::{
    AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize, RunEvent, Theme, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use hotkey::HotKey;
use place::{Area, Room};
use rates::Cache;
use store::Store;

const MIN_HEIGHT: f64 = 72.0;
const MAX_HEIGHT: f64 = 560.0;
const SETTINGS_SIZE: (f64, f64) = (520.0, 640.0);
// windows can hand focus back to the taskbar or tray just after a show; the mac overlay ignores that too
const BLUR_GRACE: Duration = Duration::from_millis(250);
// how long dwm can take to switch a title bar between light and dark
const CAPTION_SETTLE: Duration = Duration::from_millis(200);

const RESET: &str = "(function () {
  if (window.__qcalcReset) window.__qcalcReset();
  var el = document.querySelector('.quick-plain');
  if (el) { el.focus(); if (window.__qcalcFocus) window.__qcalcFocus(); }
  if (window.__qcalcSize) window.__qcalcSize();
})()";
const FIRST_RUN: &str = "window.__QCALC_FIRST_RUN = true; if (window.__qcalcFirstRun) window.__qcalcFirstRun();";
const SHOW_TIPS: &str = "if (window.__qcalcShowTips) window.__qcalcShowTips();";
const SHOW_TUTORIAL: &str = "if (window.__qcalcTutorial) window.__qcalcTutorial();";

struct State {
    store: Store,
    path: Option<PathBuf>,
    hotkey: HotKey,
    // distance from the window top to the composer, so history grows up and graphs grow down
    anchor: f64,
    // the 420 smoke's room around the bar, logical
    room: Room,
    shown_at: Option<Instant>,
    // set by the page's drag; moves while it's set are the user's, and get remembered
    dragging: bool,
    unsaved: bool,
    first_run: bool,
    // whether the bar shows once the page has loaded: every launch but the one at sign in, so opening q calc from
    // start or a shortcut opens it rather than only putting an icon in the tray
    show_on_load: bool,
    // what the page has as __QCALC_RATES
    rates: Option<Value>,
    // a game is open: losing focus leaves the window up
    game_open: bool,
    // a game has the window as its own (ctrl+F): the bar's frame from before, to go back to. the page's sizes
    // wait until it's done
    game_frame: Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>,
    // the reference sheets' windows that are open, the front one last
    sheets: Vec<String>,
    // the settings page is recording a new show / hide key, and every chord is let go meanwhile
    hotkey_paused: bool,
}

struct Host(Mutex<State>);

// window calls happen outside this lock, so their events can't find it held
fn with<T>(app: &AppHandle, f: impl FnOnce(&mut State) -> T) -> T {
    let host = app.state::<Host>();
    let mut state = host.0.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut state)
}

fn save(state: &mut State) {
    if let Some(path) = &state.path {
        let _ = state.store.save(path);
    }
    state.unsaved = false;
}

use startup::AUTOSTART_ARG;

fn main() {
    crash::install();
    e2e::catch_panics();
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![AUTOSTART_ARG])))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(update::plugin())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![host])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_config_dir().ok();
            // the launch an update's installer makes is as quiet as the one at sign in
            let updated = dir.as_deref().is_some_and(|dir| update::take_marker(dir, SystemTime::now()));
            let path = dir.as_ref().map(|dir| dir.join("settings.json"));
            let rates_path = dir.map(|dir| dir.join("rates.json"));
            let cache = rates_path.as_deref().map(Cache::load).unwrap_or_default();
            let mut store = path.as_deref().map(Store::load).unwrap_or_default();
            let first_run = store.claim_first_run();
            if let (true, Some(path)) = (first_run, &path) {
                let _ = store.save(path);
            }
            // a tray app that isn't running after a restart just looks like a dead hotkey, so windows opens it at
            // sign in from the start; settings turns that off. each launch puts the entry right: at the installed
            // copy (never a build under target\), quoted, quiet, and back after an uninstall deleted it. the mac
            // leaves this to login items
            #[cfg(windows)]
            if !cfg!(debug_assertions) {
                startup::sync(store.sign_in);
            }
            let autostarted = updated || std::env::args().any(|arg| arg == AUTOSTART_ARG);
            update::set_auto(store.auto_update);
            let notify = app.handle().clone();
            update::on_change(move || push_settings(&notify, None, None));
            let preferred = hotkey::named(&store.hotkey);
            store.hotkey = preferred.clone();
            app.manage(Host(Mutex::new(State {
                store,
                path,
                hotkey: HotKey { active: None, failed: false },
                anchor: 0.0,
                room: Room::CLOSED,
                shown_at: None,
                dragging: false,
                unsaved: false,
                first_run,
                show_on_load: first_run || !autostarted,
                rates: cache.page_value(now()),
                game_open: false,
                game_frame: None,
                sheets: Vec::new(),
                hotkey_paused: false,
            })));
            // before the web view boots, so its injected settings already carry the shortcut
            let shortcuts = app.global_shortcut();
            let hotkey = hotkey::launch(&preferred, |c| register(shortcuts, c));
            let missed = hotkey.active.as_deref() != Some(preferred.as_str());
            if missed {
                crash::log(&format!("shortcut {preferred} taken at launch, bound {:?}; trying again", hotkey.active));
            }
            with(app.handle(), |s| s.hotkey = hotkey);
            keep_hotkey(app.handle(), missed);

            let theme = with(app.handle(), |s| window_theme(&s.store.settings.theme));
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("quick.html".into()))
                .title("Q Calc")
                .inner_size(place::MIN_WIDTH, MIN_HEIGHT)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .visible(false)
                .theme(theme)
                .scroll_bar_style(frame::SCROLL_BARS)
                .initialization_script(boot_script(app.handle(), true))
                .on_page_load(|window, payload| {
                    let app = window.app_handle();
                    if payload.event() == PageLoadEvent::Finished && e2e::enabled() {
                        let _ = window.eval(e2e::PROBE);
                    }
                    if payload.event() == PageLoadEvent::Finished && with(app, |s| std::mem::take(&mut s.show_on_load)) {
                        show_window(app);
                        if with(app, |s| std::mem::take(&mut s.first_run)) {
                            let _ = window.eval(FIRST_RUN);
                        }
                    }
                })
                .build()?;
            for line in frame::dress(&window) {
                e2e::note(&line);
            }
            if e2e::enabled() {
                frame::webview_background(&window, |line| e2e::note(&line));
            }
            quiet_browser_keys(&window);
            watch_wake(&window);
            let handle = window.clone();
            window.on_window_event(move |event| match event {
                WindowEvent::Focused(false) => blurred(&handle),
                // a game's window closes back to the bar
                WindowEvent::CloseRequested { api, .. } if with(handle.app_handle(), |s| s.game_frame.is_some()) => {
                    api.prevent_close();
                    leave_game_window(&handle);
                }
                WindowEvent::Moved(position) => moved(&handle, *position),
                _ => {}
            });

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "show", "Show Q Calc", true, None::<&str>)?,
                    &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
                    &MenuItem::with_id(app, "tips", "Tips…", true, None::<&str>)?,
                    &MenuItem::with_id(app, "tutorial", "Tutorial", true, None::<&str>)?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(app, "quit", "Quit Q Calc", true, None::<&str>)?,
                ],
            )?;
            let mut tray = TrayIconBuilder::with_id(icon::TRAY)
                .tooltip("Q Calc")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_window(app),
                    "settings" => open_settings_later(app),
                    "tips" => show_tips(app),
                    "tutorial" => show_tutorial(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_window(tray.app_handle());
                    }
                });
            if let Some(image) = icon::tray(app.handle()) {
                tray = tray.icon(image);
            }
            tray.build(app)?;
            icon::watch_taskbar(app.handle());
            if let Some(path) = rates_path {
                let app = app.handle().clone();
                thread::spawn(move || refresh_rates(&app, cache, &path));
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match built {
        Ok(app) => app,
        Err(err) => {
            crash::failed(&err.to_string());
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        if let RunEvent::Ready = event {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let exe = std::env::current_exe().map_or(String::new(), |p| p.display().to_string());
            crash::started(&format!("started {} on {} {args:?} from {exe}", app.package_info().version, os_name()));
        }
        if let RunEvent::Exit = event {
            with(app, |s| {
                if s.unsaved {
                    save(s);
                }
            });
        }
    });
}

#[tauri::command]
fn host(window: WebviewWindow, message: Value) {
    let app = window.app_handle();
    let overlay = window.label() == "main";
    if e2e::enabled() && message["type"] != "size" && message["type"] != "e2e" {
        e2e::note(&format!("host got {} from {}", message["type"], window.label()));
    }
    match message["type"].as_str() {
        Some("size") if overlay => {
            if let Some(height) = message["height"].as_f64() {
                let anchor = message["anchorTop"].as_f64().unwrap_or(0.0);
                let px = |key: &str| message["room"][key].as_f64().unwrap_or(0.0).clamp(0.0, 240.0);
                let room = Room { top: px("top"), side: px("side"), bottom: px("bottom") };
                if e2e::enabled() {
                    e2e::note(&format!("size {height} anchor {anchor} room {room:?}"));
                }
                resize(&window, height, anchor, room);
            }
        }
        // ctrl+w closes the front reference sheet before the bar, as cmd-w does on the mac
        Some("dismiss") if overlay => {
            if message["key"] != "ctrlW" || !close_front_sheet(app) {
                hide(&window);
            }
        }
        Some("periodic") if overlay => open_sheet_later(app, &sheet::PERIODIC, 0),
        Some("identities") if overlay => {
            if let Some(found) = message["id"].as_str().and_then(sheet::identities) {
                open_sheet_later(app, found, message["section"].as_u64().unwrap_or(0));
            }
        }
        // a periodic table tile: what it holds goes in the bar, which opens first if it's put away
        Some("insert") if sheet::is_sheet(window.label()) => {
            if let Some(text) = message["text"].as_str() {
                insert_in_bar(app, text);
            }
        }
        Some("closeSheet") if sheet::is_sheet(window.label()) => {
            let _ = window.close();
        }
        Some("game") if overlay => {
            let open = message["open"].as_bool() == Some(true);
            with(app, |s| s.game_open = open);
            if !open {
                set_game_window(&window, false, None);
            }
        }
        // the page's ⌃F (useGameFullscreen), with its normal size for the window's shape
        Some("gameFullscreen") if overlay => {
            let natural = message["w"].as_f64().zip(message["h"].as_f64());
            set_game_window(&window, message["on"].as_bool() == Some(true), natural);
        }
        Some("copy") => {
            if let Some(text) = message["text"].as_str() {
                let _ = app.clipboard().write_text(text);
            }
        }
        Some("drag") if overlay => {
            with(app, |s| s.dragging = true);
            let _ = window.start_dragging();
            refocus_after_press(&window);
        }
        Some("recenter") if overlay => recenter(&window),
        // a side edge dragged by `dx` logical pixels since the last of these; the page sends them while the button is down
        Some("widen") if overlay => {
            if let Some(dx) = message["dx"].as_f64().filter(|dx| dx.is_finite()) {
                widen(&window, dx, message["side"] == "left");
            }
        }
        Some("widenEnd") if overlay => widened(&window, message["side"] == "left"),
        Some("spotSeconds") => {
            with(app, |s| {
                s.store.spot_seconds = store::spot_seconds(&message["seconds"]);
                save(s);
            });
            push_settings(app, None, None);
        }
        Some("settings") => {
            if with(app, |s| s.store.settings.merge(&message).then(|| save(s)).is_some()) {
                push_settings(app, Some(window.label()), None);
            }
        }
        Some("onboarding") => with(app, |s| {
            if s.store.onboarding.merge(&message) {
                save(s);
            }
        }),
        Some("analytics") if overlay => with(app, |s| {
            let next = if s.store.settings.share_usage { message["stash"].clone() } else { Value::Null };
            if s.store.analytics != next {
                s.store.analytics = next;
                save(s);
            }
        }),
        Some("openSettings") => open_settings_later(app),
        Some("e2e") if e2e::enabled() => {
            for line in message["lines"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                e2e::note(line);
            }
        }
        Some("hotkey") => choose_hotkey(app, message["chord"].as_str().unwrap_or_default()),
        Some("updateCheck") => update::ask_check(),
        Some("updateInstall") => update::ask_install(),
        Some("autoUpdate") => {
            let on = message["on"].as_bool() == Some(true);
            update::set_auto(on);
            with(app, |s| {
                s.store.auto_update = on;
                save(s);
            });
            push_settings(app, None, None);
        }
        // while the settings page records a new show / hide key, the old one mustn't swallow the press
        Some("hotkeyPause") => pause_hotkey(app, message["on"].as_bool() == Some(true)),
        // a recorded chord, tried before it's kept
        Some("hotkeyProbe") => recorder::probe(app, message["chord"].as_str().unwrap_or_default()),
        Some("tutorial") => show_tutorial(app),
        Some("autostart") => {
            let on = message["on"].as_bool() == Some(true);
            let launcher = app.autolaunch();
            if !on {
                let _ = launcher.disable();
            } else if cfg!(windows) {
                // the installed copy, quoted; a dev build with no installed copy writes nothing
                #[cfg(windows)]
                startup::enable();
            } else {
                let _ = launcher.enable();
            }
            with(app, |s| {
                s.store.sign_in = on;
                save(s);
            });
            push_settings(app, None, None);
        }
        _ => {}
    }
}

// what the page reads as __QCALC_SETTINGS: its settings plus the host's own facts
fn settings_payload(state: &State, refused: Option<&str>, autostart: bool) -> Value {
    let mut payload = serde_json::to_value(&state.store.settings).unwrap_or_else(|_| json!({}));
    let picked = state.hotkey.active.clone().unwrap_or_else(|| hotkey::named(&state.store.hotkey));
    let extra = json!({
        "hotkey": state.hotkey.active.as_deref().map_or(String::new(), hotkey::title),
        "hotkeyFailed": state.hotkey.failed,
        "hotkeyChord": picked,
        "hotkeyRefused": refused.map_or(String::new(), hotkey::title),
        "hotkeyTaken": hotkey::taken(refused, &state.hotkey, &state.store.hotkey),
        "autostart": autostart,
        "spotSeconds": state.store.spot_seconds,
        "version": env!("CARGO_PKG_VERSION"),
        "autoUpdate": state.store.auto_update,
        "update": update::status(),
    });
    if let (Some(to), Some(from)) = (payload.as_object_mut(), extra.as_object()) {
        to.extend(from.clone());
    }
    payload
}

fn boot_script(app: &AppHandle, overlay: bool) -> String {
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let boot = with(app, |s| {
        let rates = if overlay { s.rates.clone() } else { None };
        json!({
            "overlay": overlay,
            "settings": settings_payload(s, None, autostart),
            "onboarding": s.store.onboarding,
            "rates": rates,
            "analytics": s.store.analytics,
            "os": os_name(),
            "width": s.store.width.max(place::MIN_WIDTH),
        })
    });
    let shim = include_str!("shim.js").replace("__QCALC_BOOT__", &boot.to_string());
    if overlay && e2e::enabled() {
        e2e::guarded(&(shim + e2e::PAGE))
    } else {
        shim
    }
}

fn push_settings(app: &AppHandle, except: Option<&str>, refused: Option<&str>) {
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let (payload, theme) = with(app, |s| (settings_payload(s, refused, autostart), s.store.settings.theme.clone()));
    let script = format!(
        "window.__QCALC_SETTINGS = {payload}; document.documentElement.dataset.theme = {theme}; if (window.__qcalcApplySettings) window.__qcalcApplySettings({payload});",
        theme = Value::String(theme.clone()),
    );
    // every window: the overlay, settings and any reference sheet
    for (label, window) in app.webview_windows() {
        if except != Some(label.as_str()) {
            let _ = window.eval(&script);
        }
    }
    // the frame follows too: the border, the shadow and the acrylic's tint
    for window in app.webview_windows().values() {
        let _ = window.set_theme(window_theme(&theme));
        icon::dress(window);
    }
    // queued behind the theme change, so the title bar repaints in the new one. dwm takes the dark mode attribute
    // on its own time, and a repaint that beats it paints the old theme, so a second one follows once it has
    if let Some(window) = app.get_webview_window("settings") {
        let w = window.clone();
        let _ = window.run_on_main_thread(move || frame::repaint_caption(&w));
        thread::spawn(move || {
            thread::sleep(CAPTION_SETTLE);
            let w = window.clone();
            let _ = window.run_on_main_thread(move || frame::repaint_caption(&w));
        });
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

// asks the ecb at most once a day for as long as the app runs; a failed ask keeps the cached rates until they go stale
fn refresh_rates(app: &AppHandle, mut cache: Cache, path: &std::path::Path) {
    loop {
        let at = now();
        if cache.due(at) {
            cache.checked = at;
            if let Some(fresh) = rates::fetch() {
                cache.rates = Some(fresh);
                cache.fetched = at;
            }
            cache.save(path);
        }
        let next = cache.page_value(now());
        if with(app, |s| std::mem::replace(&mut s.rates, next.clone()) != next) {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval(&match next {
                    Some(rates) => format!("window.__QCALC_RATES = {rates};"),
                    None => "delete window.__QCALC_RATES;".to_string(),
                });
            }
        }
        thread::sleep(Duration::from_secs(60 * 60));
    }
}

// for the usage counts, like the mac's "macOS 14.5": "Windows 11 26200". RtlGetVersion, because the documented
// calls answer with windows 8 unless the app's manifest names each later version
#[cfg(windows)]
fn os_name() -> String {
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
    }
    let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
    if unsafe { RtlGetVersion(&mut info) } != 0 {
        return "Windows".to_string();
    }
    // windows 11 kept 10.0 as its version; its builds start at 22000
    let name = if info.dwMajorVersion >= 10 && info.dwBuildNumber >= 22000 { 11 } else { info.dwMajorVersion };
    format!("Windows {name} {}", info.dwBuildNumber)
}

#[cfg(not(windows))]
fn os_name() -> String {
    String::new()
}

fn window_theme(theme: &str) -> Option<Theme> {
    match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

fn register(shortcuts: &tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>, chord: &str) -> bool {
    hotkey::shortcut(chord).is_some_and(|s| shortcuts.register(s).is_ok())
}

fn pause_hotkey(app: &AppHandle, paused: bool) {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    with(app, |s| s.hotkey_paused = paused);
    recorder::listen(app, paused);
    if !paused {
        if let Some(active) = with(app, |s| s.hotkey.active.clone()) {
            register(shortcuts, &active);
        }
    }
}

// set when the pc wakes or the session unlocks, so a chord another app held is tried again
static WOKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

fn wake_hotkey() {
    *WOKE.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
    WOKE.1.notify_all();
}

// true when woken, false once `wait` is up; with no wait, only a wake ends it
fn wait_for_wake(wait: Option<Duration>) -> bool {
    let mut woke = WOKE.0.lock().unwrap_or_else(|e| e.into_inner());
    let until = wait.map(|w| Instant::now() + w);
    while !*woke {
        match until {
            Some(until) => {
                let left = until.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return false;
                }
                woke = WOKE.1.wait_timeout(woke, left).unwrap_or_else(|e| e.into_inner()).0;
            }
            None => woke = WOKE.1.wait(woke).unwrap_or_else(|e| e.into_inner()),
        }
    }
    *woke = false;
    true
}

// a show / hide chord that didn't bind at launch is tried again on hotkey::retry_delay's schedule, and again whenever
// the pc wakes or the session unlocks; the settings page hears as soon as it binds
fn keep_hotkey(app: &AppHandle, missed: bool) {
    let app = app.clone();
    thread::spawn(move || {
        let mut attempt = missed.then_some(0u32);
        loop {
            let woke = wait_for_wake(attempt.and_then(hotkey::retry_delay));
            if !woke && attempt.is_none() {
                continue;
            }
            // on the main thread, as the commands that change the chord are, so the two can't cross
            let (tx, rx) = mpsc::channel();
            let main = app.clone();
            if app.run_on_main_thread(move || {
                let _ = tx.send(retry_hotkey(&main));
            })
            .is_err() {
                return;
            }
            let Ok(done) = rx.recv() else { return };
            attempt = match attempt {
                _ if done => None,
                Some(n) if !woke => Some(n + 1),
                _ => Some(0),
            };
            if attempt.is_some_and(|n| hotkey::retry_delay(n).is_none()) {
                crash::log("shortcut still taken after 5 minutes; trying again when the pc wakes or unlocks");
                attempt = None;
            }
        }
    });
}

// true once the picked chord is bound
fn retry_hotkey(app: &AppHandle) -> bool {
    let (preferred, now, paused) = with(app, |s| (hotkey::named(&s.store.hotkey), s.hotkey.clone(), s.hotkey_paused));
    if paused {
        return false;
    }
    let shortcuts = app.global_shortcut();
    let unbind = |c: &str| {
        if let Some(s) = hotkey::shortcut(c) {
            let _ = shortcuts.unregister(s);
        }
    };
    let Some(next) = hotkey::retry(&preferred, &now, |c| register(shortcuts, c), unbind) else { return true };
    let done = next.active.as_deref() == Some(preferred.as_str());
    if next != now {
        crash::log(&format!("shortcut now {:?} (wanted {preferred})", next.active));
        with(app, |s| s.hotkey = next);
        // clears the settings page's "taken" line
        push_settings(app, None, None);
    }
    done
}

// the main window hears windows' power and session news; a wake or an unlock retries a chord that isn't bound
#[cfg(windows)]
fn watch_wake(window: &WebviewWindow) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    const WM_POWERBROADCAST: u32 = 0x0218;
    const PBT_APMRESUMESUSPEND: usize = 0x7;
    const PBT_APMRESUMEAUTOMATIC: usize = 0x12;
    const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
    // console connect, remote connect, logon, unlock
    const SESSION_BACK: [usize; 4] = [0x1, 0x3, 0x5, 0x8];

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
        let resumed = msg == WM_POWERBROADCAST && (wparam == PBT_APMRESUMESUSPEND || wparam == PBT_APMRESUMEAUTOMATIC);
        if resumed || (msg == WM_WTSSESSION_CHANGE && SESSION_BACK.contains(&wparam)) {
            wake_hotkey();
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as HWND;
    // setup runs on the window's own thread, as subclassing has to
    unsafe {
        SetWindowSubclass(hwnd, Some(proc), 2, 0);
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    }
}

#[cfg(not(windows))]
fn watch_wake(_: &WebviewWindow) {}

fn choose_hotkey(app: &AppHandle, raw: &str) {
    // a pause from the recorder ends with the pick
    pause_hotkey(app, false);
    let next = hotkey::named(raw);
    let now = with(app, |s| s.hotkey.clone());
    let shortcuts = app.global_shortcut();
    let (after, taken) = hotkey::select(
        &next,
        &now,
        |c| register(shortcuts, c),
        || {
            let _ = shortcuts.unregister_all();
        },
    );
    with(app, |s| {
        s.hotkey = after;
        if taken {
            s.store.hotkey = next.clone();
        }
        save(s);
    });
    push_settings(app, None, (!taken).then_some(next.as_str()));
}

fn toggle(app: &AppHandle) {
    match app.get_webview_window("main") {
        // a game's window minimised, or behind another, comes forward rather than going
        Some(window)
            if with(app, |s| s.game_frame.is_some())
                && (window.is_minimized().unwrap_or(false) || !window.is_focused().unwrap_or(true)) =>
        {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        Some(window) if window.is_visible().unwrap_or(false) => hide(&window),
        _ => show_window(app),
    }
}

// the monitor under the pointer, as a store key, its work area and its scale
fn monitor_at_cursor(window: &WebviewWindow) -> Option<Monitor> {
    window
        .cursor_position()
        .ok()
        .and_then(|p| window.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
}

fn describe(monitor: &Monitor) -> (String, Area, f64) {
    let area = monitor.work_area();
    let key = monitor.name().cloned().unwrap_or_else(|| format!("{},{}", monitor.position().x, monitor.position().y));
    let area = Area {
        x: area.position.x as f64,
        y: area.position.y as f64,
        width: area.size.width as f64,
        height: area.size.height as f64,
    };
    (key, area, monitor.scale_factor())
}

fn set_frame(window: &WebviewWindow, (x, y): (f64, f64), (width, height): (f64, f64)) {
    let position = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    // moved first so a dpi change on the new monitor is settled before the size lands
    let _ = window.set_position(position);
    let _ = window.set_size(PhysicalSize::new(width.round() as u32, height.round() as u32));
    let _ = window.set_position(position);
}

fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let monitor = monitor_at_cursor(&window);
    let key = monitor.as_ref().map(|m| describe(m).0);
    // a moved or widened bar keeps its spot and width only so long after it was last put away, as on the mac; one
    // in use doesn't run out
    let (at, hidden) = (now(), !window.is_visible().unwrap_or(false));
    let (saved, was) = with(app, |s| {
        e2e::note(&format!(
            "show on {key:?}: hidden {hidden}, spots {:?}, used {}s ago, keep {}s",
            s.store.positions,
            at.saturating_sub(s.store.used_at),
            s.store.spot_seconds
        ));
        if hidden && s.store.moved_or_widened() && place::spot_expired(at, s.store.used_at, s.store.spot_seconds) {
            s.store.positions.clear();
            s.store.width = 0.0;
            s.unsaved = true;
        }
        s.anchor = 0.0;
        s.dragging = false;
        s.shown_at = Some(Instant::now());
        (key.and_then(|k| s.store.positions.get(&k).copied()), std::mem::replace(&mut s.room, Room::CLOSED))
    });
    if was != Room::CLOSED {
        frame::dress(&window);
    }
    match monitor {
        Some(monitor) => {
            let (_, area, scale) = describe(&monitor);
            let width = bar_width(app, Some(area), scale);
            // a narrower monitor than the one it was dragged on gets a narrower bar
            tell_width(&window, width);
            let size = (width * scale, MIN_HEIGHT * scale);
            let origin = place::show_origin(area, scale, size.0, size.1, saved);
            e2e::note(&format!("show at {origin:?} from {saved:?}"));
            set_frame(&window, origin, size);
        }
        None => {
            let _ = window.set_size(LogicalSize::new(bar_width(app, None, 1.0), MIN_HEIGHT));
        }
    }
    let _ = window.show();
    // above any reference sheet left open
    frame::raise_quietly(&window);
    let _ = window.set_focus();
    let _ = window.eval(RESET);
    if e2e::enabled() {
        let _ = window.eval("if (window.__qcalcE2eReport) window.__qcalcE2eReport();");
    }
}

fn show_tips(app: &AppHandle) {
    show_window(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(SHOW_TIPS);
    }
}

// the walkthrough from the top, from the tray or the settings window
fn show_tutorial(app: &AppHandle) {
    show_window(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(SHOW_TUTORIAL);
    }
}

// a game's own window (ctrl+F): the bar becomes an ordinary window, with windows' title bar, frame and caption
// buttons, large and centred on its monitor, and goes back to the bar's frame after (the page reports its size
// again). says whether it changed
fn set_game_window(window: &WebviewWindow, on: bool, natural: Option<(f64, f64)>) -> bool {
    let app = window.app_handle();
    if !on {
        let Some((position, size)) = with(app, |s| s.game_frame.take()) else { return false };
        let _ = window.unminimize();
        let _ = window.unmaximize();
        let _ = window.set_min_size(None::<LogicalSize<f64>>);
        let _ = window.set_decorations(false);
        let _ = window.set_resizable(false);
        let _ = window.set_maximizable(false);
        let _ = window.set_minimizable(false);
        let _ = window.set_skip_taskbar(true);
        let _ = window.set_always_on_top(true);
        let _ = window.set_shadow(false);
        let _ = window.set_position(position);
        let _ = window.set_size(size);
        frame::dress(window);
        let _ = window.eval("if (window.__qcalcSize) window.__qcalcSize();");
        return true;
    }
    if with(app, |s| s.game_frame.is_some()) {
        return false;
    }
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else { return false };
    with(app, |s| s.game_frame = Some((position, size)));
    let monitor = window.current_monitor().ok().flatten().map(|m| describe(&m));
    // dwm's flyout edge is the bar's; windows draws a real frame's
    frame::bare(window);
    let _ = window.set_always_on_top(false);
    let _ = window.set_skip_taskbar(false);
    let _ = window.set_decorations(true);
    let _ = window.set_shadow(true);
    let _ = window.set_resizable(true);
    let _ = window.set_maximizable(true);
    let _ = window.set_minimizable(true);
    if let Some((_, area, scale)) = monitor {
        let (w, h) = place::game_window(area.width / scale, area.height / scale, natural);
        let (min_w, min_h) = place::game_window_min(area.width / scale, area.height / scale, natural);
        let _ = window.set_min_size(Some(LogicalSize::new(min_w, min_h)));
        let _ = window.set_size(LogicalSize::new(w, h));
    }
    let _ = window.center();
    let _ = window.set_focus();
    true
}

// the shell left the game's window by itself (its close button, the bar hiding), so the page's state follows
fn leave_game_window(window: &WebviewWindow) {
    if set_game_window(window, false, None) {
        let _ = window.eval("if (window.__qcalcGameFullscreen) window.__qcalcGameFullscreen(false);");
    }
}

fn hide(window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    leave_game_window(window);
    let _ = window.eval("if (window.__qcalcWillHide) window.__qcalcWillHide();");
    let _ = window.hide();
    let at = now();
    with(window.app_handle(), |s| {
        e2e::note(&format!("hide: spots {:?}", s.store.positions));
        s.shown_at = None;
        // what a moved or widened bar's time runs from; with nothing kept there's nothing to time
        if s.store.moved_or_widened() {
            s.store.used_at = at;
            s.unsaved = true;
        }
        if s.unsaved {
            save(s);
        }
    });
}

// off the event loop's thread, as settings is: a window built on it never finishes
fn open_sheet_later(app: &AppHandle, which: &'static sheet::Sheet, section: u64) {
    let app = app.clone();
    thread::spawn(move || open_sheet(&app, which, section));
}

// the bar's frame, its monitor's work area and that monitor's scale, while it's up, for a sheet to open clear of it
fn bar_spot(app: &AppHandle) -> Option<(Area, Area, f64)> {
    let main = app.get_webview_window("main").filter(|w| w.is_visible().unwrap_or(false))?;
    let (position, size) = (main.outer_position().ok()?, main.outer_size().ok()?);
    let (_, area, scale) = describe(&main.current_monitor().ok().flatten()?);
    let bar = Area { x: position.x as f64, y: position.y as f64, width: size.width as f64, height: size.height as f64 };
    Some((bar, area, scale))
}

// a sheet that would cover the bar, or is off its monitor, moves beside it; one put somewhere clear stays there
fn keep_clear_of_bar(app: &AppHandle, window: &WebviewWindow, which: &sheet::Sheet) {
    let Some((bar, area, scale)) = bar_spot(app) else { return };
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else { return };
    let now = Area { x: position.x as f64, y: position.y as f64, width: size.width as f64, height: size.height as f64 };
    if place::needs_moving(now, bar, area) {
        let r = place::beside(area, bar, now.width, now.height, place::SHEET_MARGIN * scale, which.give(scale));
        set_frame(window, (r.x, r.y), (r.width, r.height));
    }
}

// a reference sheet in its own window, as on the mac: always on top, never taking the keys (so the bar keeps its caret
// and stays up while it's clicked), and a second open brings the same one forward. it stays when the bar is put away
fn open_sheet(app: &AppHandle, which: &sheet::Sheet, section: u64) {
    let label = which.label();
    with(app, |s| {
        s.sheets.retain(|l| l != &label);
        s.sheets.push(label.clone());
    });
    if let Some(window) = app.get_webview_window(&label) {
        // asked for again, maybe at another part: the page scrolls there
        let _ = window.eval(format!("window.__qcalcSection && window.__qcalcSection({section});"));
        keep_clear_of_bar(app, &window, which);
        let _ = window.show();
        frame::raise_quietly(&window);
        keep_bar_on_top(app);
        return;
    }
    let theme = with(app, |s| window_theme(&s.store.settings.theme));
    let shown = app.get_webview_window("main").and_then(|w| w.theme().ok()).unwrap_or(Theme::Light);
    let earlier = with(app, |s| s.sheets.len().saturating_sub(1)) as f64;
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(which.url(section).into()))
        .title(which.title)
        .inner_size(which.size.0, which.size.1)
        .min_inner_size(which.min.0, which.min.1)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .maximizable(false)
        .minimizable(false)
        .theme(theme)
        .background_color(frame::sheet_background(shown))
        .scroll_bar_style(frame::SCROLL_BARS)
        .visible(false)
        .initialization_script(boot_script(app, false))
        .build();
    let window = match built {
        Ok(window) => window,
        Err(err) => {
            crash::log(&format!("{label} window failed: {err}"));
            with(app, |s| s.sheets.retain(|l| l != &label));
            return;
        }
    };
    quiet_browser_keys(&window);
    let handle = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Destroyed => {
            let label = handle.label().to_string();
            with(handle.app_handle(), |s| s.sheets.retain(|l| l != &label));
        }
        // in case windows hands it the keys anyway: they go back to the bar while it's up
        WindowEvent::Focused(true) => {
            if let Some(main) = handle.app_handle().get_webview_window("main") {
                if main.is_visible().unwrap_or(false) {
                    let _ = main.set_focus();
                }
            }
        }
        _ => {}
    });
    // beside the bar, clear of it, while it's up; else centred on the screen the cursor is on, each further sheet a
    // little down and right of the one before
    if let Some((bar, area, scale)) = bar_spot(app) {
        let (w, h) = (which.size.0 * scale, which.size.1 * scale);
        let r = place::beside(area, bar, w, h, place::SHEET_MARGIN * scale, which.give(scale));
        set_frame(&window, (r.x, r.y), (r.width, r.height));
    } else if let Some(monitor) = monitor_at_cursor(&window) {
        let (_, area, scale) = describe(&monitor);
        let (w, h) = (which.size.0 * scale, which.size.1 * scale);
        let step = sheet::CASCADE * scale * earlier;
        let origin = (area.x + (area.width - w) / 2.0 + step, area.y + (area.height - h) / 2.0 + step);
        set_frame(&window, place::fit(area, origin.0, origin.1, w, h), (w, h));
    }
    icon::dress(&window);
    let _ = window.show();
    keep_bar_on_top(app);
}

// the bar sits above the sheets, as the mac's panel sits a level above its floating sheets
fn keep_bar_on_top(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main").filter(|w| w.is_visible().unwrap_or(false)) {
        frame::raise_quietly(&main);
    }
}

// the front one that's still up; false when there's none, so ctrl+w goes on to the bar
fn close_front_sheet(app: &AppHandle) -> bool {
    let open = with(app, |s| s.sheets.clone());
    let front = open.iter().rev().find_map(|label| app.get_webview_window(label).filter(|w| w.is_visible().unwrap_or(false)));
    match front {
        Some(window) => {
            let _ = window.close();
            true
        }
        None => false,
    }
}

// behind show_window's reset, so it lands in the fresh input
fn insert_in_bar(app: &AppHandle, text: &str) {
    let Some(main) = app.get_webview_window("main") else { return };
    if !main.is_visible().unwrap_or(false) {
        show_window(app);
    }
    let _ = main.eval(format!("window.__qcalcInsert && window.__qcalcInsert({});", Value::String(text.to_string())));
    let _ = main.set_focus();
}

fn blurred(window: &WebviewWindow) {
    // clicking away from an open game leaves it up
    if with(window.app_handle(), |s| s.game_open) {
        return;
    }
    let early = with(window.app_handle(), |s| s.shown_at.is_some_and(|t| t.elapsed() < BLUR_GRACE));
    if early {
        let _ = window.set_focus();
        return;
    }
    // moving the bar hands the keyboard from the web view to the window's own frame, which arrives here as a blur
    // though q calc is still the window in front. only another window taking the foreground is a click away
    if frame::in_front(window) {
        return;
    }
    // a click on a reference sheet can still hand it the foreground (the web view inside asks for focus); that's
    // q calc's own window, so the bar stays up and takes the keys back, as the mac's never-key panels leave it
    let app = window.app_handle();
    let sheets = with(app, |s| s.sheets.clone());
    if sheets.iter().filter_map(|label| app.get_webview_window(label)).any(|sheet| frame::in_front(&sheet)) {
        let _ = window.set_focus();
        return;
    }
    hide(window);
}

// the move leaves the keyboard with the window's frame; once the button is up it goes back to the page, so typing
// carries on
fn refocus_after_press(window: &WebviewWindow) {
    let window = window.clone();
    thread::spawn(move || {
        // the move loop takes a moment to begin
        thread::sleep(Duration::from_millis(60));
        while frame::primary_button_down() {
            thread::sleep(Duration::from_millis(30));
        }
        // where the button came up is the spot kept: a size report from the page can end `dragging` mid-move, and
        // the moves after it weren't remembered
        if window.is_visible().unwrap_or(false) {
            if let Ok(position) = window.outer_position() {
                remember(&window, position);
            }
        }
        with(window.app_handle(), |s| s.dragging = false);
        if window.is_visible().unwrap_or(false) && frame::in_front(&window) {
            let _ = window.set_focus();
        }
    });
}

fn moved(window: &WebviewWindow, position: PhysicalPosition<i32>) {
    if with(window.app_handle(), |s| s.dragging) {
        remember(window, position);
    }
}

// keeps where the bar is as this monitor's spot
fn remember(window: &WebviewWindow, position: PhysicalPosition<i32>) {
    let app = window.app_handle();
    let (anchor, side) = with(app, |s| (s.anchor, s.room.side));
    let Some(monitor) = window.current_monitor().ok().flatten() else { return };
    let (key, area, scale) = describe(&monitor);
    let x = position.x as f64 + side * scale;
    let spot = place::remembered(area, scale, x, position.y as f64, anchor * scale);
    e2e::note(&format!("remember {key} at {},{} as {spot:?}", position.x, position.y));
    with(app, |s| {
        s.store.positions.insert(key, spot);
        s.unsaved = true;
    });
}

// a double click on the top edge forgets this monitor's spot, as on the mac
fn recenter(window: &WebviewWindow) {
    let Some(monitor) = window.current_monitor().ok().flatten() else { return };
    let (key, area, scale) = describe(&monitor);
    let (anchor, side) = with(window.app_handle(), |s| {
        s.dragging = false;
        s.store.positions.remove(&key);
        save(s);
        (s.anchor, s.room.side)
    });
    let width = bar_width(window.app_handle(), Some(area), scale);
    let (x, y) = place::centred(area, (width + 2.0 * side) * scale, anchor * scale);
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

// the bar's width in logical pixels: what it was dragged to, as far as this monitor allows
fn bar_width(app: &AppHandle, area: Option<Area>, scale: f64) -> f64 {
    let stored = with(app, |s| s.store.width);
    match area {
        Some(area) => place::fitted_width(stored, area.width / scale),
        None => stored.max(place::MIN_WIDTH),
    }
}

// one step of a side edge's drag: that edge moves and the other stays put, as on the mac. the page reflows to the
// new width and reports its height as usual
fn widen(window: &WebviewWindow, dx: f64, left: bool) {
    let app = window.app_handle();
    // the smoke's room has the window wider than the bar, and it's gone in a moment
    if with(app, |s| s.room != Room::CLOSED) {
        return;
    }
    let Some(monitor) = window.current_monitor().ok().flatten() else { return };
    let (_, area, scale) = describe(&monitor);
    let (Ok(origin), Ok(size)) = (window.outer_position(), window.inner_size()) else { return };
    let width = bar_width(app, Some(area), scale);
    let (next, shift) = place::widened(width, dx, left, area.width / scale);
    if next == width {
        return;
    }
    with(app, |s| {
        s.dragging = false;
        s.store.width = next;
        s.unsaved = true;
    });
    set_frame(window, (origin.x as f64 + shift * scale, origin.y as f64), (next * scale, size.height as f64));
}

// the page pads the bar to this width inside a window the 420 smoke has grown, so it has to know it
fn tell_width(window: &WebviewWindow, width: f64) {
    let _ = window.eval(format!("document.documentElement.style.setProperty('--bar-width', '{width}px')"));
}

// the drag is over: the width is kept, and a moved left edge is a moved bar, so its spot is kept too. a centred bar
// widened from the right stays centred
fn widened(window: &WebviewWindow, left: bool) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let area = window.current_monitor().ok().flatten().map(|m| describe(&m).1);
    tell_width(window, bar_width(window.app_handle(), area, scale));
    if left {
        if let Ok(position) = window.outer_position() {
            remember(window, position);
        }
    }
    with(window.app_handle(), |s| {
        if s.unsaved {
            save(s);
        }
    });
}

fn resize(window: &WebviewWindow, height: f64, anchor_top: f64, room: Room) {
    if with(window.app_handle(), |s| s.game_frame.is_some()) {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let extra = room.top + room.bottom;
    let h = height.ceil().clamp(MIN_HEIGHT + extra, MAX_HEIGHT + extra);
    let next = anchor_top.clamp(room.top, h - room.bottom - MIN_HEIGHT);
    let (previous, was) = with(window.app_handle(), |s| {
        s.dragging = false;
        (std::mem::replace(&mut s.anchor, next), std::mem::replace(&mut s.room, room))
    });
    let Ok(origin) = window.outer_position() else { return };
    let area = window.current_monitor().ok().flatten().map(|m| describe(&m).1);
    let size = ((bar_width(window.app_handle(), area, scale) + 2.0 * room.side) * scale, h * scale);
    let (x, y) = (origin.x as f64, origin.y as f64);
    let (anchor, next) = (previous * scale, next * scale);
    let origin = place::resized(area, x, y, size.0, size.1, anchor, next, was.scaled(scale), room.scaled(scale));
    // dwm's edge would draw a box round the smoke, so it goes while the room is open
    if was == Room::CLOSED && room != Room::CLOSED {
        frame::bare(window);
    }
    set_frame(window, origin, size);
    if was != Room::CLOSED && room == Room::CLOSED {
        frame::dress(window);
    }
}

// on windows, building a web view window on the event loop's own thread (a command, a menu click, even
// run_on_main_thread) never returns: the build waits for the loop it is blocking. from another thread it asks the
// loop to build it and waits there instead
fn open_settings_later(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || open_settings(&app));
}

fn open_settings(app: &AppHandle) {
    e2e::note("open_settings");
    if let Some(main) = app.get_webview_window("main") {
        hide(&main);
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let theme = with(app, |s| window_theme(&s.store.settings.theme));
    // the overlay already wears the chosen theme, or the system's
    let shown = app.get_webview_window("main").and_then(|w| w.theme().ok()).unwrap_or(Theme::Light);
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Q Calc Settings")
        .inner_size(SETTINGS_SIZE.0, SETTINGS_SIZE.1)
        .min_inner_size(420.0, 360.0)
        .maximizable(false)
        .theme(theme)
        .background_color(frame::settings_background(shown))
        .scroll_bar_style(frame::SCROLL_BARS)
        .visible(false)
        .initialization_script(boot_script(app, false));
    e2e::note("building the settings window");
    let built = built.build();
    e2e::note(&match &built {
        Ok(_) => "settings window built".to_string(),
        Err(e) => format!("settings window failed: {e}"),
    });
    let Ok(window) = built else { return };
    quiet_browser_keys(&window);
    recorder::attach(&window);
    icon::dress(&window);
    if let Some(monitor) = monitor_at_cursor(&window) {
        let (_, area, scale) = describe(&monitor);
        let (w, h) = (SETTINGS_SIZE.0 * scale, SETTINGS_SIZE.1 * scale);
        let origin = (area.x + (area.width - w) / 2.0, area.y + (area.height - h) / 2.0);
        set_frame(&window, place::fit(area, origin.0, origin.1, w, h), (w, h));
    }
    let _ = window.show();
    let _ = window.set_focus();
}

// reload, print and find would break the page, so webview2's browser shortcuts are off
#[cfg(windows)]
fn quiet_browser_keys(window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;
    let _ = window.with_webview(|webview| unsafe {
        let settings = webview.controller().CoreWebView2().and_then(|core| core.Settings());
        if let Ok(settings) = settings.and_then(|s| s.cast::<ICoreWebView2Settings3>()) {
            let _ = settings.SetAreBrowserAcceleratorKeysEnabled(false);
        }
    });
}

#[cfg(not(windows))]
fn quiet_browser_keys(_: &WebviewWindow) {}
