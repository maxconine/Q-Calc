// the settings window's show / hide recorder, on the host's side. while the page records, a keyboard hook hears the
// chord before windows or another app acts on it: alt+space would open the window menu, win chords the shell's own
// things, and a launcher holding the chord would open instead of the page hearing anything. a probe then tries the
// chord before it's kept, and guesses who holds a taken one
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::hotkey::{self, Heard, Owner, Setting, Verdict};

// the page listens for these on the window
pub fn tell(app: &AppHandle, detail: &Value) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.eval(format!("window.dispatchEvent(new CustomEvent('qcalc-hotkey', {{ detail: {detail} }}))"));
    }
}

fn heard_json(heard: &Heard) -> Value {
    match heard {
        Heard::Held(mods) => json!({ "held": mods }),
        Heard::Chord(chord) => json!({ "heard": chord }),
        Heard::Unknown => json!({ "unknown": true }),
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

// on while the page records the show / hide key, off when it stops. the hook lives on the main thread, whose message
// loop it needs
#[cfg(windows)]
pub fn listen(app: &AppHandle, on: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        hook::stop();
        if on {
            hook::start(&handle);
        }
    });
}

#[cfg(not(windows))]
pub fn listen(_: &AppHandle, _: bool) {}

#[cfg(windows)]
mod hook {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Mutex;
    use std::thread;

    use serde_json::Value;
    use tauri::{AppHandle, Manager};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    use crate::hotkey::Keys;

    static HOOK: AtomicIsize = AtomicIsize::new(0);
    static WINDOW: AtomicIsize = AtomicIsize::new(0);
    // the keyboard so far, and the way out to the page
    static STATE: Mutex<Option<(Keys, Sender<Value>)>> = Mutex::new(None);

    pub fn start(app: &AppHandle) {
        let Some(hwnd) = app.get_webview_window("settings").and_then(|w| w.hwnd().ok()) else { return };
        WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
        // windows drops a hook that's slow to answer, so the page hears from one thread of its own, in order; it
        // ends when stop drops the sender
        let (send, heard) = channel::<Value>();
        let app = app.clone();
        thread::spawn(move || {
            for detail in heard {
                super::tell(&app, &detail);
            }
        });
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Keys::default(), send));
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), GetModuleHandleW(std::ptr::null()), 0) };
        HOOK.store(hook as isize, Ordering::SeqCst);
    }

    pub fn stop() {
        let hook = HOOK.swap(0, Ordering::SeqCst);
        if hook != 0 {
            unsafe { UnhookWindowsHookEx(hook as _) };
        }
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    // injected keys (another app's, or a test's) go by untouched
    unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = &*(lparam as *const KBDLLHOOKSTRUCT);
            if info.flags & LLKHF_INJECTED == 0 {
                let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
                let focused = GetForegroundWindow() as isize == WINDOW.load(Ordering::SeqCst);
                let mut keep = false;
                if let Ok(mut state) = STATE.try_lock() {
                    if let Some((keys, send)) = state.as_mut() {
                        let (kept, heard) = keys.key(info.vkCode, down, focused);
                        keep = kept;
                        if let Some(heard) = heard {
                            let _ = send.send(super::heard_json(&heard));
                        }
                    }
                }
                if keep {
                    return 1;
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }
}