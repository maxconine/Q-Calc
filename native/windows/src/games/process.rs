//! A game runs in a process of its own: `qcalc.exe --game <kind>`, the same exe. The bar's process never loads
//! Direct3D, the GPU driver or a game's heap, and closing the game window ends its process, so everything a game
//! used goes back to the system (in the bar's process ~28 MB of driver and heap stayed committed after a game).
//!
//! - The bar (`open`): a game of that kind already up is brought forward (its window class is per kind), else the
//!   exe starts with `--game <kind> --t0 <QueryPerformanceCounter at â†µ>`. QCALC_INSTANCE, QCALC_DATA and QCALC_LOG
//!   go with it (the environment is inherited).
//! - The game (`child_main`): its own single-instance mutex per kind (never the bar's, so it isn't handed to the
//!   bar's instance), the game window with its hardware target, a message loop, and out when the window closes.
//!   Nothing of the bar starts: no engine thread, Typst, hotkey, tray, autostart check or bar window.

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, DispatchMessageW, FindWindowW, GetMessageW, IsIconic, MSG,
    SW_RESTORE, SetForegroundWindow, ShowWindow, TranslateMessage,
};
use windows::core::HSTRING;

use crate::qlog;

/// The argument that makes the exe a game.
pub const ARG: &str = "--game";
const T0: &str = "--t0";
const PARENT: &str = "--parent";

/// The window class of a game of `kind` (per instance, so a test copy finds only its own games).
pub fn class_name(kind: &str) -> String {
    format!("QCalcGame.{kind}{}", crate::log::instance_suffix())
}

fn mutex_name(kind: &str) -> String {
    format!(
        "Local\\com.maxconine.qcalc.game.{kind}{}",
        crate::log::instance_suffix()
    )
}

/// What `--game` asks: the kind, and the bar's â†µ as a performance-counter reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameArgs {
    pub kind: String,
    pub t0: Option<i64>,
    /// the bar's process: the game closes when it ends (quit from the tray, an update's restart, a crash)
    pub parent: Option<u32>,
}

impl GameArgs {
    pub fn parse(args: &[String]) -> Option<GameArgs> {
        let at = args.iter().position(|a| a == ARG)?;
        let kind = args.get(at + 1)?.clone();
        let t0 = args
            .iter()
            .position(|a| a == T0)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok());
        let parent = args
            .iter()
            .position(|a| a == PARENT)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok());
        Some(GameArgs { kind, t0, parent })
    }
}

/// The wall clock in microseconds (GetSystemTimePreciseAsFileTime underneath, the same in every process).
pub fn qpc() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as i64)
}

/// Milliseconds from a `qpc` reading to now.
pub fn ms_since(t0: i64) -> f64 {
    (qpc() - t0) as f64 / 1000.0
}

pub fn find(kind: &str) -> Option<HWND> {
    // SAFETY: plain lookup by class
    unsafe { FindWindowW(&HSTRING::from(class_name(kind)), None) }
        .ok()
        .filter(|h| !h.is_invalid())
}

/// Brings a game window forward (restoring it when minimised).
fn bring_forward(hwnd: HWND) {
    // SAFETY: plain calls on another process's top-level window
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// The bar's side: the game of `kind` already up comes forward, else its process starts.
pub fn launch(kind: &str) {
    if let Some(hwnd) = find(kind) {
        bring_forward(hwnd);
        qlog!("games: {kind} is already open; brought forward");
        return;
    }
    let t0 = qpc();
    let Ok(exe) = std::env::current_exe() else {
        qlog!("games: couldn't find our own exe to start {kind}");
        return;
    };
    match std::process::Command::new(exe)
        .args([
            ARG,
            kind,
            T0,
            &t0.to_string(),
            PARENT,
            &std::process::id().to_string(),
        ])
        .spawn()
    {
        Ok(child) => {
            // the bar has the foreground (its â†µ); the game may take it
            // SAFETY: plain call with the child's id
            let _ = unsafe { AllowSetForegroundWindow(child.id()) };
            qlog!("games: started {kind} (pid {})", child.id());
        }
        Err(e) => qlog!("games: couldn't start {kind}: {e}"),
    }
}

/// Closes the game (as its own close button would, so it saves) once the bar's process `pid` has ended. A thread
/// that only waits on the process handle: no polling, no cost while the bar runs.
fn close_with(pid: u32, kind: String) {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    // SAFETY: a handle we close below; an id that's already gone fails to open, and then the bar is gone too
    let Ok(parent) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        qlog!("games: the bar (pid {pid}) is already gone; closing {kind}");
        if let Some(hwnd) = find(&kind) {
            // SAFETY: our own window
            let _ = unsafe {
                PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default())
            };
        }
        return;
    };
    let handle = parent.0 as usize;
    let _ = std::thread::Builder::new()
        .name("qcalc-game-parent".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let parent = windows::Win32::Foundation::HANDLE(handle as *mut _);
            // SAFETY: our open handle, closed once the wait ends
            unsafe {
                if WaitForSingleObject(parent, INFINITE) == WAIT_OBJECT_0 {
                    qlog!("games: the bar (pid {pid}) ended; closing {kind}");
                    if let Some(hwnd) = find(&kind) {
                        let _ = PostMessageW(
                            Some(hwnd),
                            WM_CLOSE,
                            Default::default(),
                            Default::default(),
                        );
                    }
                }
                let _ = CloseHandle(parent);
            }
        });
}

/// `qcalc.exe --game <kind>`: runs the game and returns the exit code; None when the command line isn't a game's.
pub fn child_main() -> Option<i32> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = GameArgs::parse(&argv)?;
    crate::log::install();
    let kind = args.kind.as_str();
    // SAFETY: the handle stays open for the process's life
    let mutex = unsafe { CreateMutexW(None, true, &HSTRING::from(mutex_name(kind))) };
    // SAFETY: plain query right after the call
    if mutex.is_ok() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // a second launch of the same game: the first one comes forward
        if let Some(hwnd) = find(kind) {
            bring_forward(hwnd);
        }
        return Some(0);
    }
    // SAFETY: plain call
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if let Err(e) = super::create(kind, args.t0) {
        qlog!("games: couldn't open {kind}: {e}");
        return Some(1);
    }
    crate::log::started();
    if let Some(parent) = args.parent {
        close_with(parent, kind.to_string());
    }
    let mut msg = MSG::default();
    // SAFETY: the standard loop on this thread; it ends with the window (WM_QUIT)
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let _ = mutex;
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_arguments_parse() {
        let a = |s: &[&str]| GameArgs::parse(&s.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(
            a(&["--game", "pong"]),
            Some(GameArgs {
                kind: "pong".into(),
                t0: None,
                parent: None
            })
        );
        assert_eq!(
            a(&["--game", "ember", "--t0", "123"]),
            Some(GameArgs {
                kind: "ember".into(),
                t0: Some(123),
                parent: None
            })
        );
        assert_eq!(
            a(&["--game", "pong", "--t0", "5", "--parent", "4242"]),
            Some(GameArgs {
                kind: "pong".into(),
                t0: Some(5),
                parent: Some(4242)
            })
        );
        assert_eq!(a(&["--autostart"]), None);
        assert_eq!(a(&["--game"]), None);
        assert!(class_name("pong").starts_with("QCalcGame.pong"));
    }
}
