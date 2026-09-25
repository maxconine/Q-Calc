#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hotkey;
mod place;
mod rates;
mod store;
mod update;

use std::path::PathBuf;
use std::sync::Mutex;
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

use hotkey::{HotKey, PRESETS};
use place::Area;
use rates::Cache;
use store::Store;

const WIDTH: f64 = 680.0;
const MIN_HEIGHT: f64 = 72.0;
const MAX_HEIGHT: f64 = 560.0;
const SETTINGS_SIZE: (f64, f64) = (520.0, 640.0);
// windows can hand focus back to the taskbar or tray just after a show; the mac overlay ignores that too
const BLUR_GRACE: Duration = Duration::from_millis(250);

const RESET: &str = "(function () {
  if (window.__qcalcReset) window.__qcalcReset();
  var el = document.querySelector('.quick-plain');
  if (el) { el.focus(); if (window.__qcalcFocus) window.__qcalcFocus(); }
  if (window.__qcalcSize) window.__qcalcSize();
})()";
const FIRST_RUN: &str = "window.__QCALC_FIRST_RUN = true; if (window.__qcalcFirstRun) window.__qcalcFirstRun();";
const SHOW_TIPS: &str = "if (window.__qcalcShowTips) window.__qcalcShowTips();";

struct State {
    store: Store,
    path: Option<PathBuf>,
    hotkey: HotKey,
    // distance from the window top to the composer, so history grows up and graphs grow down
    anchor: f64,
    shown_at: Option<Instant>,
    // set by the page's drag; moves while it's set are the user's, and get remembered
    dragging: bool,
    unsaved: bool,
    first_run: bool,
    // what the page has as __QCALC_RATES
    rates: Option<Value>,
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

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
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
            let path = dir.as_ref().map(|dir| dir.join("settings.json"));
            let rates_path = dir.map(|dir| dir.join("rates.json"));
            let cache = rates_path.as_deref().map(Cache::load).unwrap_or_default();
            let mut store = path.as_deref().map(Store::load).unwrap_or_default();
            let first_run = store.claim_first_run();
            if let (true, Some(path)) = (first_run, &path) {
                let _ = store.save(path);
            }
            let preferred = hotkey::named(&store.hotkey);
            app.manage(Host(Mutex::new(State {
                store,
                path,
                hotkey: HotKey { active: None, failed: false },
                anchor: 0.0,
                shown_at: None,
                dragging: false,
                unsaved: false,
                first_run,
                rates: cache.page_value(now()),
            })));
            // before the web view boots, so its injected settings already carry the shortcut
            let shortcuts = app.global_shortcut();
            let hotkey = hotkey::launch(preferred, |i| shortcuts.register(PRESETS[i].shortcut()).is_ok());
            with(app.handle(), |s| s.hotkey = hotkey);

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("quick.html".into()))
                .title("Q Calc")
                .inner_size(WIDTH, MIN_HEIGHT)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .visible(false)
                .initialization_script(boot_script(app.handle(), true))
                .on_page_load(|window, payload| {
                    let app = window.app_handle();
                    if payload.event() == PageLoadEvent::Finished && with(app, |s| std::mem::take(&mut s.first_run)) {
                        show_window(app);
                        let _ = window.eval(FIRST_RUN);
                    }
                })
                .build()?;
            quiet_browser_keys(&window);
            let handle = window.clone();
            window.on_window_event(move |event| match event {
                WindowEvent::Focused(false) => blurred(&handle),
                WindowEvent::Moved(position) => moved(&handle, *position),
                _ => {}
            });

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "show", "Show Q Calc", true, None::<&str>)?,
                    &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
                    &MenuItem::with_id(app, "tips", "Tips…", true, None::<&str>)?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(app, "quit", "Quit Q Calc", true, None::<&str>)?,
                ],
            )?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Q Calc")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_window(app),
                    "settings" => open_settings(app),
                    "tips" => show_tips(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if let Some(path) = rates_path {
                let app = app.handle().clone();
                thread::spawn(move || refresh_rates(&app, cache, &path));
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("q calc failed to start");
    app.run(|app, event| {
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
    match message["type"].as_str() {
        Some("size") if overlay => {
            if let Some(height) = message["height"].as_f64() {
                resize(&window, height, message["anchorTop"].as_f64().unwrap_or(0.0));
            }
        }
        Some("dismiss") if overlay => hide(&window),
        Some("copy") => {
            if let Some(text) = message["text"].as_str() {
                let _ = app.clipboard().write_text(text);
            }
        }
        Some("drag") if overlay => {
            with(app, |s| s.dragging = true);
            let _ = window.start_dragging();
        }
        Some("recenter") if overlay => recenter(&window),
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
        Some("openSettings") => open_settings(app),
        Some("hotkey") => choose_hotkey(app, message["id"].as_str().unwrap_or_default()),
        Some("autostart") => {
            let launcher = app.autolaunch();
            let _ = if message["on"].as_bool() == Some(true) { launcher.enable() } else { launcher.disable() };
            push_settings(app, None, None);
        }
        _ => {}
    }
}

// what the page reads as __QCALC_SETTINGS: its settings plus the host's own facts
fn settings_payload(state: &State, refused: Option<usize>, autostart: bool) -> Value {
    let mut payload = serde_json::to_value(&state.store.settings).unwrap_or_else(|_| json!({}));
    let picked = state.hotkey.active.unwrap_or_else(|| hotkey::named(&state.store.hotkey));
    let extra = json!({
        "hotkey": state.hotkey.active.map_or("", |i| PRESETS[i].title),
        "hotkeyFailed": state.hotkey.failed,
        "hotkeyId": PRESETS[picked].id,
        "hotkeyRefused": refused.map_or("", |i| PRESETS[i].title),
        "hotkeys": PRESETS.iter().map(|p| json!({ "id": p.id, "title": p.title })).collect::<Vec<_>>(),
        "autostart": autostart,
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
        json!({ "overlay": overlay, "settings": settings_payload(s, None, autostart), "onboarding": s.store.onboarding, "rates": rates })
    });
    include_str!("shim.js").replace("__QCALC_BOOT__", &boot.to_string())
}

fn push_settings(app: &AppHandle, except: Option<&str>, refused: Option<usize>) {
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let (payload, theme) = with(app, |s| (settings_payload(s, refused, autostart), s.store.settings.theme.clone()));
    let script = format!(
        "window.__QCALC_SETTINGS = {payload}; document.documentElement.dataset.theme = {theme}; if (window.__qcalcApplySettings) window.__qcalcApplySettings({payload});",
        theme = Value::String(theme.clone()),
    );
    for label in ["main", "settings"] {
        if except == Some(label) {
            continue;
        }
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.eval(&script);
        }
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_theme(window_theme(&theme));
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

fn window_theme(theme: &str) -> Option<Theme> {
    match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

fn choose_hotkey(app: &AppHandle, id: &str) {
    let next = hotkey::named(id);
    let now = with(app, |s| s.hotkey);
    let shortcuts = app.global_shortcut();
    let (after, taken) = hotkey::select(
        next,
        now,
        |i| shortcuts.register(PRESETS[i].shortcut()).is_ok(),
        || {
            let _ = shortcuts.unregister_all();
        },
    );
    with(app, |s| {
        s.hotkey = after;
        if taken {
            s.store.hotkey = PRESETS[next].id.to_string();
        }
        save(s);
    });
    push_settings(app, None, (!taken).then_some(next));
}

fn toggle(app: &AppHandle) {
    match app.get_webview_window("main") {
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
    let saved = with(app, |s| {
        s.anchor = 0.0;
        s.dragging = false;
        s.shown_at = Some(Instant::now());
        key.and_then(|k| s.store.positions.get(&k).copied())
    });
    match monitor {
        Some(monitor) => {
            let (_, area, scale) = describe(&monitor);
            let size = (WIDTH * scale, MIN_HEIGHT * scale);
            set_frame(&window, place::show_origin(area, scale, size.0, size.1, saved), size);
        }
        None => {
            let _ = window.set_size(LogicalSize::new(WIDTH, MIN_HEIGHT));
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.eval(RESET);
}

fn show_tips(app: &AppHandle) {
    show_window(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(SHOW_TIPS);
    }
}

fn hide(window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let _ = window.eval("if (window.__qcalcWillHide) window.__qcalcWillHide();");
    let _ = window.hide();
    with(window.app_handle(), |s| {
        s.shown_at = None;
        if s.unsaved {
            save(s);
        }
    });
}

fn blurred(window: &WebviewWindow) {
    let early = with(window.app_handle(), |s| s.shown_at.is_some_and(|t| t.elapsed() < BLUR_GRACE));
    if early {
        let _ = window.set_focus();
        return;
    }
    hide(window);
}

fn moved(window: &WebviewWindow, position: PhysicalPosition<i32>) {
    let app = window.app_handle();
    let Some(anchor) = with(app, |s| s.dragging.then_some(s.anchor)) else { return };
    let Some(monitor) = window.current_monitor().ok().flatten() else { return };
    let (key, area, scale) = describe(&monitor);
    let spot = place::remembered(area, scale, position.x as f64, position.y as f64, anchor * scale);
    with(app, |s| {
        s.store.positions.insert(key, spot);
        s.unsaved = true;
    });
}

// a double click on the top edge forgets this monitor's spot, as on the mac
fn recenter(window: &WebviewWindow) {
    let Some(monitor) = window.current_monitor().ok().flatten() else { return };
    let (key, area, scale) = describe(&monitor);
    let anchor = with(window.app_handle(), |s| {
        s.dragging = false;
        s.store.positions.remove(&key);
        save(s);
        s.anchor
    });
    let (x, y) = place::centred(area, WIDTH * scale, anchor * scale);
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn resize(window: &WebviewWindow, height: f64, anchor_top: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let h = height.ceil().clamp(MIN_HEIGHT, MAX_HEIGHT);
    let next = anchor_top.clamp(0.0, h - MIN_HEIGHT);
    let previous = with(window.app_handle(), |s| {
        s.dragging = false;
        std::mem::replace(&mut s.anchor, next)
    });
    let Ok(origin) = window.outer_position() else { return };
    let area = window.current_monitor().ok().flatten().map(|m| describe(&m).1);
    let size = (WIDTH * scale, h * scale);
    let origin = place::resized(area, origin.x as f64, origin.y as f64, size.0, size.1, previous * scale, next * scale);
    set_frame(window, origin, size);
}

fn open_settings(app: &AppHandle) {
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
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Q Calc Settings")
        .inner_size(SETTINGS_SIZE.0, SETTINGS_SIZE.1)
        .min_inner_size(420.0, 360.0)
        .maximizable(false)
        .theme(theme)
        .visible(false)
        .initialization_script(boot_script(app, false))
        .build();
    let Ok(window) = built else { return };
    quiet_browser_keys(&window);
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
