// the settings window's show / hide recorder, on the host's side. while the page records, alt+space reaches it: web
// view2 keeps that chord from the page as a system key, so the host hands it on, and the window's own menu stays shut. a probe then tries the chord before it's kept,
// and guesses who holds a taken one. (a global keyboard hook heard nothing in this process, and would read as a
// keylogger to security software already wary of an unsigned app)
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::hotkey::{self, Owner, Setting, Verdict};

// the page listens for these on the window
pub fn tell(app: &AppHandle, detail: &Value) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.eval(format!("window.dispatchEvent(new CustomEvent('qcalc-hotkey', {{ detail: {detail} }}))"));
    }
}

fn verdict_json(v: &Verdict) -> Value {
    json!({
        "chord": v.chord,
        "usable": v.usable,
        "free": v.free,
        "owner": v.owner.as_ref().map(|o| o.id),
        "sure": v.owner.as_ref().is_some_and(|o| o.sure),
        "suggest": v.suggest,
    })
}

// registers and lets go straight away; on the main thread, like every other register
fn free(shortcuts: &tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>, chord: &str) -> bool {
    let Some(shortcut) = hotkey::shortcut(chord) else { return false };
    if shortcuts.register(shortcut).is_err() {
        return false;
    }
    let _ = shortcuts.unregister(shortcut);
    true
}

pub fn probe(app: &AppHandle, raw: &str) {
    let active = crate::with(app, |s| s.hotkey.active.clone());
    let shortcuts = app.global_shortcut();
    let verdict = hotkey::probe(raw, active.as_deref(), |c| free(shortcuts, c), owner);
    tell(app, &json!({ "verdict": verdict_json(&verdict) }));
}

fn owner(chord: &str) -> Option<Owner> {
    hotkey::guess(chord, &hotkey::launchers(&processes(), setting))
}

#[cfg(windows)]
fn processes() -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut out = vec![];
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            out.push(String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase());
            more = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
fn processes() -> Vec<String> {
    vec![]
}

fn read(var: &str, path: &str) -> Option<String> {
    std::fs::read_to_string(std::path::Path::new(&std::env::var_os(var)?).join(path)).ok()
}

// each launcher's own settings, where they're known to live
fn setting(id: &'static str) -> Setting {
    let chord = |c: Option<String>| c.map_or(Setting::Unknown, Setting::Chord);
    match id {
        "powertoys-run" => {
            if read("LOCALAPPDATA", r"Microsoft\PowerToys\settings.json").is_some_and(|j| !hotkey::powertoys_run_enabled(&j)) {
                return Setting::Off;
            }
            chord(read("LOCALAPPDATA", r"Microsoft\PowerToys\PowerToys Run\settings.json").and_then(|j| hotkey::powertoys_run_chord(&j)))
        }
        "flow-launcher" => chord(read("APPDATA", r"FlowLauncher\Settings\Settings.json").and_then(|j| hotkey::words_chord(&j))),
        "wox" => chord(read("APPDATA", r"Wox\Settings\Settings.json").and_then(|j| hotkey::words_chord(&j))),
        _ => Setting::Unknown,
    }
}

// on while the page records the show / hide key, off when it stops
#[cfg(windows)]
pub fn listen(app: &AppHandle, on: bool) {
    menu::set(app, on);
}

#[cfg(not(windows))]
pub fn listen(_: &AppHandle, _: bool) {}

// once, as the settings window is built: web view2 tells the host of alt+space instead of the page, and while the row
// records the host passes it on as heard
#[cfg(windows)]
pub fn attach(window: &tauri::WebviewWindow) {
    use webview2_com::AcceleratorKeyPressedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    };
    let page = window.clone();
    let _ = window.with_webview(move |webview| unsafe {
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            if !menu::recording() {
                return Ok(());
            }
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            args.KeyEventKind(&mut kind)?;
            let mut key = 0u32;
            args.VirtualKey(&mut key)?;
            // space with alt held: what windows would turn into the window menu
            if kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN && key == 0x20 {
                args.SetHandled(true)?;
                tell(page.app_handle(), &json!({ "heard": "alt+space" }));
            }
            Ok(())
        }));
        let mut token = 0;
        let _ = webview.controller().add_AcceleratorKeyPressed(&handler, &mut token);
    });
}

#[cfg(not(windows))]
pub fn attach(_: &tauri::WebviewWindow) {}

// windows opens a window's menu at alt+space (the web view hands the key on to the window when the page doesn't take
// it all the way); while the row records, the settings window lets that one command go by
#[cfg(windows)]
mod menu {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    use tauri::{AppHandle, Manager};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SC_KEYMENU, WM_SYSCOMMAND};

    static RECORDING: AtomicBool = AtomicBool::new(false);

    pub fn recording() -> bool {
        RECORDING.load(Ordering::SeqCst)
    }
    // the settings window the subclass is on; a new one (it's rebuilt each time it opens) gets it again
    static ON: AtomicIsize = AtomicIsize::new(0);

    pub fn set(app: &AppHandle, on: bool) {
        RECORDING.store(on, Ordering::SeqCst);
        if !on {
            return;
        }
        let Some(window) = app.get_webview_window("settings") else { return };
        let Ok(hwnd) = window.hwnd() else { return };
        let hwnd = hwnd.0 as isize;
        if ON.swap(hwnd, Ordering::SeqCst) == hwnd {
            return;
        }
        // on the window's own thread, as subclassing has to be
        let _ = window.run_on_main_thread(move || unsafe {
            SetWindowSubclass(hwnd as HWND, Some(proc), 1, 0);
        });
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
        if msg == WM_SYSCOMMAND && (wparam & 0xFFF0) as u32 == SC_KEYMENU && RECORDING.load(Ordering::SeqCst) {
            return 0;
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

