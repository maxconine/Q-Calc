#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

mod update;

const WIDTH: f64 = 680.0;
const MIN_HEIGHT: f64 = 72.0;
const MAX_HEIGHT: f64 = 560.0;
const HOTKEYS: [&str; 2] = ["Alt+Space", "Ctrl+Alt+Space"];

const RESET: &str = "(function () {
  if (window.__qcalcReset) window.__qcalcReset();
  var el = document.querySelector('.quick-plain');
  if (el) { el.focus(); if (window.__qcalcFocus) window.__qcalcFocus(); }
  if (window.__qcalcSize) window.__qcalcSize();
})()";

// distance from the window top to the composer, so history grows up and graphs grow down
struct AnchorTop(Mutex<f64>);

fn main() {
    tauri::Builder::default()
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
        .manage(AnchorTop(Mutex::new(0.0)))
        .invoke_handler(tauri::generate_handler![host])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let hotkey = HOTKEYS.iter().find(|k| app.global_shortcut().register(**k).is_ok());
            let settings = json!({ "hotkey": hotkey.copied().unwrap_or(""), "hotkeyFailed": hotkey.is_none() });
            let shim = include_str!("shim.js").replace("__QCALC_HOTKEY__", &settings.to_string());

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("quick.html".into()))
                .title("Q Calc")
                .inner_size(WIDTH, MIN_HEIGHT)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .visible(false)
                .initialization_script(shim)
                .build()?;
            let handle = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Focused(false) = event {
                    hide(&handle);
                }
            });

            let show = MenuItem::with_id(app, "show", "Show Q Calc", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Q Calc")
                .menu(&Menu::with_items(app, &[&show, &quit])?)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_window(app),
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("q calc failed to start");
}

#[tauri::command]
fn host(window: WebviewWindow, message: Value) {
    match message["type"].as_str() {
        Some("size") => {
            if let Some(height) = message["height"].as_f64() {
                resize(&window, height, message["anchorTop"].as_f64().unwrap_or(0.0));
            }
        }
        Some("dismiss") => hide(&window),
        Some("copy") => {
            if let Some(text) = message["text"].as_str() {
                let _ = window.app_handle().clipboard().write_text(text);
            }
        }
        Some("drag") => {
            let _ = window.start_dragging();
        }
        _ => {}
    }
}

fn toggle(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(window) if window.is_visible().unwrap_or(false) => hide(&window),
        _ => show_window(app),
    }
}

fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    *app.state::<AnchorTop>().0.lock().unwrap() = 0.0;
    place(&window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.eval(RESET);
}

fn hide(window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let _ = window.eval("if (window.__qcalcWillHide) window.__qcalcWillHide();");
    let _ = window.hide();
}

// logical work area of the monitor under the pointer: (left, top, width, height)
fn work_area(window: &WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    let cursor = window.cursor_position().ok();
    let monitor = cursor
        .and_then(|p| window.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    Some((
        area.position.x as f64 / scale,
        area.position.y as f64 / scale,
        area.size.width as f64 / scale,
        area.size.height as f64 / scale,
    ))
}

fn place(window: &WebviewWindow) {
    let Some((left, top, width, height)) = work_area(window) else { return };
    let _ = window.set_size(LogicalSize::new(WIDTH, MIN_HEIGHT));
    let _ = window.set_position(LogicalPosition::new(left + (width - WIDTH) / 2.0, top + height * 0.28));
}

fn resize(window: &WebviewWindow, height: f64, anchor_top: f64) {
    let state = window.state::<AnchorTop>();
    let mut anchor = state.0.lock().unwrap();
    let h = height.ceil().clamp(MIN_HEIGHT, MAX_HEIGHT);
    let next = anchor_top.clamp(0.0, h - MIN_HEIGHT);
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(origin) = window.outer_position() else { return };
    let origin = origin.to_logical::<f64>(scale);
    let mut x = origin.x;
    let mut y = origin.y + *anchor - next;
    // the whole window stays on screen; if it can't fit, the top wins
    if let Some((left, top, width, area_height)) = work_area(window) {
        y = y.min(top + area_height - h).max(top);
        x = x.min(left + width - WIDTH).max(left);
    }
    *anchor = next;
    let _ = window.set_size(LogicalSize::new(WIDTH, h));
    let _ = window.set_position(LogicalPosition::new(x, y));
}
