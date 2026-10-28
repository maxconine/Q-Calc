//! A game runs in a process of its own: `qcalc.exe --game <kind>`, the same exe. The bar's process never loads
//! Direct3D, the GPU driver or a game's heap, and closing the game window ends its process, so everything a game
//! used goes back to the system (in the bar's process ~28 MB of driver and heap stayed committed after a game).
//!
//! - The bar (`open`): a game of that kind already up is brought forward (its window class is per kind), else the
//!   exe starts with `--game <kind> --t0 <QueryPerformanceCounter at â†µ>`, and `--at <x>,<width>,<top>`: the bar's
//!   place, which the game takes (GameWindow.swift's `GameBarSpot`). QCALC_INSTANCE, QCALC_DATA and QCALC_LOG
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
const AT: &str = "--at";

/// Where the bar was as a game opened, in physical pixels: its left edge, width and the composer's top. The game
/// takes the bar's place there (the Mac's `GameBarSpot`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BarSpot {
    pub x: i32,
    pub width: i32,
    pub top: i32,
}

impl BarSpot {
    fn arg(self) -> String {
        format!("{},{},{}", self.x, self.width, self.top)
    }

    fn parse(s: &str) -> Option<BarSpot> {
        let mut it = s.split(',').map(|p| p.trim().parse::<i32>().ok());
        let (x, width, top) = (it.next()??, it.next()??, it.next()??);
        (width > 0 && it.next().is_none()).then_some(BarSpot { x, width, top })
    }
}

/// A monitor's work area, physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The card's frame (x, y, w, h) for a game `h` px tall at `spot`: hanging from the composer's top, moved up only
/// as far as the work area needs, never wider or taller than it (GameWindow.swift `barFrame`).
pub fn card_frame(spot: BarSpot, h: i32, area: Area) -> (i32, i32, i32, i32) {
    let w = spot.width.min(area.w).max(1);
    let h = h.min(area.h).max(1);
    let y = spot.top.min(area.y + area.h - h).max(area.y);
    let x = spot.x.min(area.x + area.w - w).max(area.x);
    (x, y, w, h)
}

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
    /// the bar's place, which the game takes
    pub at: Option<BarSpot>,
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
        let at = args
            .iter()
            .position(|a| a == AT)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| BarSpot::parse(v));
        Some(GameArgs {
            kind,
            t0,
            parent,
            at,
        })
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

/// The bar's hotkey while a game is up: the one in front closes (as the bar would hide), one behind comes forward.
/// False when none of `kinds` is up.
pub fn hotkey<'a>(kinds: impl Iterator<Item = &'a str>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, PostMessageW, WM_CLOSE};
    let Some(hwnd) = kinds.filter_map(find).next() else {
        return false;
    };
    // SAFETY: plain query, and a close posted to the game's window
    unsafe {
        if GetForegroundWindow() == hwnd && !IsIconic(hwnd).as_bool() {
            qlog!("games: the hotkey closes the game in front");
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default());
        } else {
            qlog!("games: the hotkey brings the game forward");
            bring_forward(hwnd);
        }
    }
    true
}

/// The game's side, closing by esc from its lobby: the bar shows again where it was (a second launch's hand-off
/// with no arguments, app.rs `forward`, which shows the bar).
pub fn bar_back() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_COPYDATA,
    };
    let title = HSTRING::from(format!("Q Calc host{}", crate::log::instance_suffix()));
    // SAFETY: plain lookups and a synchronous WM_COPYDATA with a live (empty) buffer
    unsafe {
        let Ok(host) = FindWindowW(windows::core::w!("QCalcHost"), &title) else {
            qlog!("games: no bar to go back to");
            return;
        };
        let mut pid = 0u32;
        GetWindowThreadProcessId(host, Some(&mut pid));
        let _ = AllowSetForegroundWindow(pid);
        let empty = [0u16; 1];
        let data = COPYDATASTRUCT {
            // app.rs COPYDATA_ARGS
            dwData: 0x5143_4c43,
            cbData: 0,
            lpData: empty.as_ptr() as *mut _,
        };
        let mut result = 0usize;
        SendMessageTimeoutW(
            host,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&data as *const _ as isize),
            SMTO_ABORTIFHUNG,
            2000,
            Some(&mut result),
        );
    }
    qlog!("games: closed back to the bar");
}

/// The bar's side: the game of `kind` already up comes forward, else its process starts in the bar's place.
pub fn launch(kind: &str, spot: Option<BarSpot>) {
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
    let mut cmd = std::process::Command::new(exe);
    cmd.args([
        ARG,
        kind,
        T0,
        &t0.to_string(),
        PARENT,
        &std::process::id().to_string(),
    ]);
    if let Some(spot) = spot {
        cmd.args([AT, &spot.arg()]);
    }
    match cmd.spawn() {
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
    if let Err(e) = super::create(kind, args.t0, args.at) {
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
                parent: None,
                at: None,
            })
        );
        assert_eq!(
            a(&["--game", "ember", "--t0", "123"]),
            Some(GameArgs {
                kind: "ember".into(),
                t0: Some(123),
                parent: None,
                at: None,
            })
        );
        assert_eq!(
            a(&["--game", "pong", "--t0", "5", "--parent", "4242"]),
            Some(GameArgs {
                kind: "pong".into(),
                t0: Some(5),
                parent: Some(4242),
                at: None,
            })
        );
        assert_eq!(
            a(&["--game", "pong", "--at", "-1900,1020,300"]).and_then(|g| g.at),
            Some(BarSpot {
                x: -1900,
                width: 1020,
                top: 300
            })
        );
        assert_eq!(
            a(&["--game", "pong", "--at", "1,0,3"]).and_then(|g| g.at),
            None
        );
        assert_eq!(
            a(&["--game", "pong", "--at", "1,2"]).and_then(|g| g.at),
            None
        );
        assert_eq!(a(&["--autostart"]), None);
        assert_eq!(a(&["--game"]), None);
        assert!(class_name("pong").starts_with("QCalcGame.pong"));
    }

    #[test]
    fn the_card_hangs_from_the_composer_and_stays_on_screen() {
        let area = Area {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let spot = BarSpot {
            x: 450,
            width: 1020,
            top: 300,
        };
        // the bar's left and width, its top at the composer's
        assert_eq!(card_frame(spot, 500, area), (450, 300, 1020, 500));
        // too tall to hang there: moved up just enough
        assert_eq!(card_frame(spot, 900, area), (450, 140, 1020, 900));
        // never taller or wider than the screen, never off its left
        assert_eq!(
            card_frame(
                BarSpot {
                    x: -50,
                    width: 3000,
                    top: 10
                },
                2000,
                area
            ),
            (0, 0, 1920, 1040)
        );
    }
}
