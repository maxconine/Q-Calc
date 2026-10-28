//! The app: entry, single instance, the windows' message routing, the message loop, and `App`, the one owner of
//! all state (on the UI thread, in a thread-local).
//!
//! **Threads.** The UI thread runs the Win32 loop and owns `App`. The engine thread and the Typst thread
//! (engine.rs) do the maths and post `WM_ENGINE` back; the bench pipe has its own thread (bench.rs). Nothing
//! else touches `App`.
//!
//! **Routing.** Two windows: the hidden *host* (class `QCalcHost`, top-level so it hears broadcasts) takes the
//! hotkey, the tray, power / session / Explorer-restart notices, a second launch's WM_COPYDATA, engine results and
//! the bench; the *bar* (window.rs) takes paint, keys, IME and mouse. Both wndprocs borrow `App` for the message
//! (`with_app`); a message that arrives while it is borrowed (sent back synchronously by a Win32 call) gets
//! DefWindowProc. So handlers never call Win32 functions that send messages back (SetWindowPos, ShowWindow,
//! SetForegroundWindow, TrackPopupMenu, UpdateWindow): they push an `Op`, and `run_ops` runs them after the borrow
//! ends.
//!
//! **Repaint only on change.** After each message `App::after` checks the model's `changed` flag: if set it lays
//! the bar out again (a new height becomes `Op::Reframe`) and paints at once (`UpdateWindow`). No timer runs while
//! idle; the timers that exist are one-shot (hotkey retries, the trim after a long hide, a bench step's timeout).

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Instant;

use qcalc_ffi::{EngineConfig, HistoryRow};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ScreenToClient, UpdateWindow, ValidateRect};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::Ime::{GCS_COMPSTR, GCS_RESULTSTR};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    FindWindowW, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    HWND_TOPMOST, IDC_ARROW, IDC_IBEAM, KillTimer, LoadCursorW, MSG, PostMessageW, PostQuitMessage,
    RegisterClassExW, RegisterWindowMessageW, SMTO_ABORTIFHUNG, SW_HIDE, SWP_NOACTIVATE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SendMessageTimeoutW, SetCursor, SetForegroundWindow, SetTimer,
    SetWindowPos, ShowWindow, TranslateMessage, WA_INACTIVE, WM_ACTIVATE, WM_APP, WM_CHAR,
    WM_CLOSE, WM_COPYDATA, WM_DESTROY, WM_DPICHANGED, WM_ENDSESSION, WM_ERASEBKGND,
    WM_EXITSIZEMOVE, WM_HOTKEY, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_NCHITTEST, WM_PAINT,
    WM_POWERBROADCAST, WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WM_SIZING, WM_SYSCHAR,
    WM_SYSKEYDOWN, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::bench::{self, Run, Step, Wait};
use crate::engine::{EngineThread, Rows};
use crate::hotkey::{self, Hotkey};
use crate::input::ime;
use crate::keys::Mods;
use crate::model::{BarModel, Outcome, now_ms};
use crate::render::{Gfx, Theme};
use crate::store::{self, Store};
use crate::tray::{self, Tray};
use crate::window::{self, Bar};
use crate::{autostart, clip, history, log, qlog, settings_window};

pub const WM_ENGINE: u32 = WM_APP + 1;
pub const WM_TRAY: u32 = WM_APP + 2;
pub const WM_PIPE: u32 = WM_APP + 3;
pub const WM_BENCH_NEXT: u32 = WM_APP + 4;
pub const WM_WARM: u32 = WM_APP + 5;

const TIMER_TRIM: usize = 0x5452;
const TIMER_BENCH: usize = 0x4243;
/// After this long hidden, the working set is trimmed (memory target: < 25 MB with the bar hidden).
const TRIM_AFTER_MS: u32 = 30_000;
const BENCH_STEP_MS: u32 = 10_000;
/// WM_COPYDATA's tag for a second launch's arguments.
const COPYDATA_ARGS: usize = 0x5143_4c43;

const HOST_CLASS: PCWSTR = w!("QCalcHost");
const MUTEX: &str = "Local\\com.maxconine.qcalc.native";

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app unless it is missing or already borrowed (a re-entrant message).
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        slot.as_mut().map(f)
    })
}

/// Window work that sends messages back, run after the app's borrow ends.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// place, show and focus the bar
    Show,
    Hide,
    /// the bar's height changed: move / size it with the composer kept still
    Reframe,
    /// the tray menu (modal)
    TrayMenu,
    Quit,
    /// a feature's own window work (settings_window.rs), run outside the borrow
    Run(crate::bg::Later),
}

/// What the command line asks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    pub autostart: bool,
    pub bench: Option<String>,
    pub bench_pipe: bool,
}

impl Args {
    pub fn parse(args: &[String]) -> Args {
        let mut out = Args {
            bench_pipe: std::env::var_os("QCALC_BENCH").is_some(),
            ..Args::default()
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                autostart::ARG => out.autostart = true,
                "--bench" => out.bench = it.next().cloned(),
                "--bench-pipe" => out.bench_pipe = true,
                _ => {}
            }
        }
        out
    }
}

/// All of the app's state.
pub struct App {
    pub host: HWND,
    pub bar: Bar,
    pub gfx: Gfx,
    pub theme: Theme,
    pub model: BarModel,
    pub engine: EngineThread,
    pub store: Store,
    store_path: Option<PathBuf>,
    pub hotkey: Hotkey,
    pub tray: Tray,
    /// the bench run in progress
    pub run: Option<Run>,
    /// a notice for the next showing (the hotkey couldn't be had)
    pub notice: Option<String>,
    pub ops: Vec<Op>,
    needs_paint: bool,
    ready_logged: bool,
    taskbar_created: u32,
    hidden_at: Option<Instant>,
}

pub fn run() -> i32 {
    log::install();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = Args::parse(&argv);
    qlog!("launch native {} {:?}", env!("CARGO_PKG_VERSION"), argv);

    // one Q Calc: a second launch hands its arguments to the first (which shows the bar) and leaves
    // SAFETY: the handle stays open for the process's life (that is the point)
    let mutex = unsafe {
        CreateMutexW(
            None,
            true,
            &HSTRING::from(format!("{MUTEX}{}", crate::log::instance_suffix())),
        )
    };
    // SAFETY: plain query right after the call
    if mutex.is_ok() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        forward(&argv);
        return 0;
    }
    // the bar's device, made meanwhile on a thread of its own (warp.rs)
    crate::warp::prefetch();
    // the manifest already says per-monitor v2; this covers a build without it
    // SAFETY: plain call
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let store_path = store::settings_path();
    let mut st = store_path.as_deref().map(Store::load).unwrap_or_default();
    st.hotkey = hotkey::named(&st.hotkey);
    let first_run = st.claim_first_run();
    if first_run && let Some(p) = &store_path {
        let _ = st.save(p);
    }
    autostart::apply(first_run, None);

    let gfx = match Gfx::new() {
        Ok(g) => g,
        Err(e) => {
            log::tell(&format!(
                "Q Calc couldn't start: Direct2D or DirectWrite is unavailable ({e})."
            ));
            return 1;
        }
    };
    let host = match create_host() {
        Ok(h) => h,
        Err(e) => {
            log::tell(&format!("Q Calc couldn't start: {e}"));
            return 1;
        }
    };
    let mut bar = match Bar::create(Some(bar_proc)) {
        Ok(b) => b,
        Err(e) => {
            log::tell(&format!("Q Calc couldn't start: {e}"));
            return 1;
        }
    };
    if st.width > 0.0 {
        bar.width = crate::place::fitted_width(st.width, f64::MAX) as f32;
    }
    let theme = Theme::for_setting(&st.settings.theme);
    bar.set_dark(theme.dark);

    let host_id = host.0 as usize;
    let engine = EngineThread::start(
        EngineConfig {
            is_windows: true,
            locale: "en_US".into(),
            data_dir: store::config_dir().map(|d| d.display().to_string()),
            time_zone: None,
        },
        move || {
            // SAFETY: posting to our host window from a worker thread is what PostMessage is for
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(host_id as *mut _)),
                    WM_ENGINE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        },
    );
    // the first evaluation is the slow one: now if the bar shows now, else with its first showing (engine.rs)
    if !args.autostart || first_run {
        engine.warm_up();
    }
    crate::migrate::history(&engine);
    history::load(&engine);

    // SAFETY: plain registration on our host window
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    // SAFETY: as above; WTS notices go to the host's wndproc
    let _ = unsafe { WTSRegisterSessionNotification(host, 0) };
    let mut tray = Tray::new(host, WM_TRAY);
    if !tray.add() {
        qlog!("tray: icon not added (Explorer not up yet?); retried on TaskbarCreated");
    }
    let mut hk = Hotkey::new(host, &st.hotkey);
    hk.kick("launch");

    let mut model = BarModel::new(st.settings.clone());
    model.touch();
    let app = App {
        host,
        bar,
        gfx,
        theme,
        model,
        engine,
        store: st,
        store_path,
        hotkey: hk,
        tray,
        run: None,
        notice: None,
        ops: Vec::new(),
        needs_paint: false,
        ready_logged: false,
        taskbar_created,
        hidden_at: None,
    };
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    if args.bench_pipe {
        bench::pipe::serve(host, WM_PIPE);
        qlog!("bench pipe listening");
    }
    with_app(|app| {
        if !args.autostart || first_run {
            app.ops.push(Op::Show);
        }
        if let Some(spec) = &args.bench {
            app.start_bench_file(spec);
        }
    });
    // SAFETY: posting to our own window
    let _ = unsafe { PostMessageW(Some(host), WM_WARM, WPARAM(0), LPARAM(0)) };
    run_ops();
    log::started();

    let mut msg = MSG::default();
    // SAFETY: the standard loop
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        // SAFETY: as above
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let app = APP.with(|cell| cell.borrow_mut().take());
    if let Some(app) = app {
        app.engine.shutdown();
        drop(app);
    }
    qlog!("quit");
    0
}

/// A second launch: hand the arguments to the running app and let it take the foreground.
fn forward(argv: &[String]) {
    // SAFETY: plain lookups and a synchronous WM_COPYDATA with a live buffer
    unsafe {
        // by title too, which carries QCALC_INSTANCE: a test copy hands off to its own first copy, not another's
        let Ok(host) = FindWindowW(HOST_CLASS, &host_title()) else {
            qlog!("second launch: no running host window found");
            return;
        };
        let mut pid = 0u32;
        GetWindowThreadProcessId(host, Some(&mut pid));
        let _ = AllowSetForegroundWindow(pid);
        let text: Vec<u16> = argv.join("\n").encode_utf16().collect();
        let data = COPYDATASTRUCT {
            dwData: COPYDATA_ARGS,
            cbData: (text.len() * 2) as u32,
            lpData: text.as_ptr() as *mut _,
        };
        let mut result = 0usize;
        SendMessageTimeoutW(
            host,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&data as *const _ as isize),
            SMTO_ABORTIFHUNG,
            3000,
            Some(&mut result),
        );
    }
    qlog!("second launch: handed {argv:?} to the running app");
}

/// "Q Calc host", plus `-<QCALC_INSTANCE>` for a test copy.
fn host_title() -> HSTRING {
    HSTRING::from(format!("Q Calc host{}", crate::log::instance_suffix()))
}

fn create_host() -> windows::core::Result<HWND> {
    // SAFETY: a class with our wndproc and a hidden window
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(host_proc),
            hInstance: instance.into(),
            lpszClassName: HOST_CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            HOST_CLASS,
            &host_title(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

fn mods() -> Mods {
    // SAFETY: plain queries of the keyboard state for this message
    let down = |vk: i32| unsafe { GetKeyState(vk) } < 0;
    Mods {
        ctrl: down(i32::from(VK_CONTROL.0)),
        alt: down(i32::from(VK_MENU.0)),
        shift: down(i32::from(VK_SHIFT.0)),
        win: down(i32::from(VK_LWIN.0)) || down(i32::from(VK_RWIN.0)),
    }
}

fn lo(v: isize) -> i32 {
    i32::from((v & 0xffff) as u16 as i16)
}

fn hi(v: isize) -> i32 {
    i32::from(((v >> 16) & 0xffff) as u16 as i16)
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let out = with_app(|app| {
        let r = app.on_host(msg, wp, lp);
        app.after();
        r
    });
    run_ops();
    match out {
        Some(Some(r)) => r,
        // SAFETY: the default for our own window
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let out = with_app(|app| {
        let r = app.on_bar(msg, wp, lp);
        app.after();
        r
    });
    run_ops();
    match out {
        Some(Some(r)) => r,
        // a paint while the app is busy (or before it exists) must still validate, or it repeats forever
        _ if msg == WM_PAINT => {
            // SAFETY: our own window
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            LRESULT(0)
        }
        // SAFETY: the default for our own window
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Runs the queued window ops, outside any borrow.
pub(crate) fn run_ops() {
    loop {
        let Some((ops, paint, bar)) = with_app(|a| {
            (
                std::mem::take(&mut a.ops),
                std::mem::take(&mut a.needs_paint),
                a.bar.hwnd,
            )
        }) else {
            return;
        };
        if ops.is_empty() && !paint {
            return;
        }
        for op in ops {
            run_op(op);
        }
        if paint {
            // SAFETY: our bar; UpdateWindow sends WM_PAINT now, outside the borrow
            unsafe {
                let _ = InvalidateRect(Some(bar), None, false);
                let _ = UpdateWindow(bar);
            }
        }
    }
}

/// Runs `f` on the app from outside its two wndprocs (a sheet's window, a timer, the relay's window), then lays
/// out, repaints and runs the ops as after a message. Never call it while the app is borrowed.
pub fn poke(f: impl FnOnce(&mut App)) {
    with_app(|a| {
        f(a);
        a.after();
    });
    run_ops();
}

fn run_op(op: Op) {
    match op {
        Op::Show => {
            let _show_ms = crate::perf::ShowTimer(Instant::now());
            // a hidden bar's device, made while the showing lays out (warp.rs)
            crate::warp::prefetch();
            let Some(Some((hwnd, x, y, w, h))) = with_app(|a| a.prepare_show()) else {
                return;
            };
            with_app(|a| a.bar.cloak(true));
            // SAFETY: our bar; these send WM_SIZE / WM_ACTIVATE / WM_PAINT back, which borrow the app afresh
            unsafe {
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = UpdateWindow(hwnd);
            }
            with_app(|a| a.bar.cloak(false));
        }
        Op::Hide => {
            // not visible from here on, so the WM_ACTIVATE the hide sends back doesn't hide it again
            let Some(Some(hwnd)) =
                with_app(|a| std::mem::take(&mut a.bar.visible).then_some(a.bar.hwnd))
            else {
                return;
            };
            // SAFETY: our bar
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            with_app(|a| a.after_hide());
        }
        Op::Reframe => {
            let Some(Some((hwnd, x, y, w, h))) = with_app(|a| {
                a.bar.visible.then(|| {
                    let (x, y, w, h) = a.bar.reframe(a.bar.layout.height, a.bar.layout.anchor);
                    (a.bar.hwnd, x, y, w, h)
                })
            }) else {
                return;
            };
            // SAFETY: our bar
            let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
        }
        Op::TrayMenu => {
            let Some((host, state)) = with_app(|a| (a.host, tray::MenuState::of(a))) else {
                return;
            };
            let cmd = Tray::menu(host, &state);
            with_app(|a| tray::picked(a, cmd));
        }
        Op::Run(f) => (f.0)(),
        Op::Quit => {
            let Some((host, bar)) = with_app(|a| {
                a.save_store();
                a.hotkey.unregister();
                a.tray.remove();
                (a.host, a.bar.hwnd)
            }) else {
                return;
            };
            // SAFETY: our windows; the host's WM_DESTROY ends the loop
            unsafe {
                let _ = DestroyWindow(bar);
                let _ = DestroyWindow(host);
            }
        }
    }
}

impl App {
    /// Queues a window op (run after the app's borrow ends), for modules outside app.rs.
    pub fn op(&mut self, op: Op) {
        self.ops.push(op);
    }

    // ---- host messages ----

    fn on_host(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_HOTKEY if wp.0 as i32 == hotkey::HOTKEY_ID => self.toggle(),
            WM_TIMER => match wp.0 {
                hotkey::TIMER_ID => {
                    if let Some(notice) = self.hotkey.on_timer() {
                        self.tray.notify("Q Calc's shortcut is taken", &notice);
                        self.notice = Some(notice);
                    }
                }
                TIMER_TRIM => self.trim(),
                TIMER_BENCH => self.bench_timeout(),
                _ => return None,
            },
            WM_POWERBROADCAST => {
                // PBT_APMRESUMESUSPEND 7, PBT_APMRESUMEAUTOMATIC 0x12
                if wp.0 == 7 || wp.0 == 0x12 {
                    self.hotkey.kick("resume");
                    crate::messaging::woke();
                }
                return Some(LRESULT(1));
            }
            WM_WTSSESSION_CHANGE => {
                // WTS_SESSION_LOGON 5, WTS_SESSION_UNLOCK 8
                if wp.0 == 5 || wp.0 == 8 {
                    self.hotkey.kick("session unlock");
                }
            }
            WM_TRAY => {
                // NOTIFYICON_VERSION_4: the event is lParam's low word
                match (lp.0 & 0xffff) as u32 {
                    // WM_LBUTTONUP, NIN_SELECT, NIN_KEYSELECT
                    0x0202 | 0x0400 | 0x0401 => self.ops.push(Op::Show),
                    // WM_CONTEXTMENU, WM_RBUTTONUP
                    0x007B | 0x0205 => self.ops.push(Op::TrayMenu),
                    _ => {}
                }
            }
            WM_COPYDATA => {
                // SAFETY: the sender's struct lives for this SendMessage
                let data = unsafe { &*(lp.0 as *const COPYDATASTRUCT) };
                if data.dwData == COPYDATA_ARGS {
                    // SAFETY: cbData bytes of UTF-16 at lpData, as forward() sent them
                    let units = unsafe {
                        std::slice::from_raw_parts(
                            data.lpData as *const u16,
                            data.cbData as usize / 2,
                        )
                    };
                    let text = String::from_utf16_lossy(units);
                    let argv: Vec<String> = text
                        .split('\n')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect();
                    let args = Args::parse(&argv);
                    qlog!("second launch said {argv:?}");
                    if let Some(spec) = &args.bench {
                        self.start_bench_file(spec);
                    } else if !args.autostart {
                        self.ops.push(Op::Show);
                    }
                    return Some(LRESULT(1));
                }
                return None;
            }
            WM_ENGINE => {
                crate::perf::mark(crate::perf::Stage::Engine);
                for done in self.engine.take_results() {
                    done(self);
                }
                crate::perf::mark(crate::perf::Stage::Applied);
                crate::answer::poll(self);
            }
            WM_PIPE => {
                // SAFETY: the pipe thread boxed it and gave it up by posting (bench::pipe)
                let req = unsafe { Box::from_raw(lp.0 as *mut bench::pipe::Request) };
                self.on_pipe(*req);
            }
            WM_BENCH_NEXT => self.bench_next(),
            WM_WARM => self.warm(),
            WM_SETTINGCHANGE => {
                let what = if lp.0 == 0 {
                    String::new()
                } else {
                    // SAFETY: WM_SETTINGCHANGE's lParam is a NUL-terminated string when set
                    unsafe { PCWSTR(lp.0 as *const u16).to_string() }.unwrap_or_default()
                };
                if what == "ImmersiveColorSet" || wp.0 == crate::contrast::SPI_SETHIGHCONTRAST {
                    self.theme = Theme::for_setting(&self.store.settings.theme);
                    self.bar.set_dark(self.theme.dark);
                    self.tray.refresh_icon();
                    crate::sheets::retheme(self.theme);
                    self.model.touch();
                    settings_window::refresh(self);
                }
            }
            WM_ENDSESSION => self.save_store(),
            WM_DESTROY => {
                // SAFETY: ends the loop
                unsafe { PostQuitMessage(0) };
            }
            m if crate::bg::is_ours(m) => crate::bg::run(self),
            m if m == self.taskbar_created && m != 0 => {
                qlog!("Explorer restarted: tray icon and hotkey again");
                self.tray.add();
                self.hotkey.kick("explorer restarted");
            }
            _ => return None,
        }
        Some(LRESULT(0))
    }

    // ---- bar messages ----

    fn on_bar(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        let hwnd = self.bar.hwnd;
        match msg {
            WM_PAINT => {
                self.paint();
                // SAFETY: our bar
                let _ = unsafe { ValidateRect(Some(hwnd), None) };
            }
            WM_ERASEBKGND => return Some(LRESULT(1)),
            WM_SIZE => {
                self.bar
                    .surface
                    .resize(lo(lp.0) as u32 & 0xffff, hi(lp.0) as u32 & 0xffff);
            }
            WM_DPICHANGED => {
                self.bar.surface.set_dpi((wp.0 & 0xffff) as u32);
                crate::drag::dpi_changed(self, lp);
                self.model.touch();
                self.ops.push(Op::Reframe);
            }
            WM_ACTIVATE => {
                if (wp.0 & 0xffff) as u32 == WA_INACTIVE {
                    self.bar.focused = false;
                    let to = HWND(lp.0 as *mut _);
                    let ours = !to.is_invalid() && {
                        let mut pid = 0u32;
                        // SAFETY: plain query
                        unsafe { GetWindowThreadProcessId(to, Some(&mut pid)) };
                        pid == std::process::id()
                    };
                    // a click away (another window took the foreground) puts the bar away
                    // except on a messaging line, so files can be dragged in from Explorer (share.rs)
                    if self.bar.visible
                        && !ours
                        && self.run.is_none()
                        && !crate::share::holds_open(&self.model)
                    {
                        self.ops.push(Op::Hide);
                    }
                } else {
                    self.bar.focused = true;
                }
                ime::forget();
                self.model.touch();
            }
            WM_CLOSE => self.ops.push(Op::Hide),
            // files dropped on the bar go to whoever the line names (share.rs)
            windows::Win32::UI::WindowsAndMessaging::WM_DROPFILES => {
                crate::share::dropped(self, windows::Win32::UI::Shell::HDROP(wp.0 as *mut _));
            }
            // the input's one-shot timers: the caret's blink, the examples, the squiggle (line.rs)
            WM_TIMER if crate::line::timer(self, wp.0) => {}
            // dragged by the background, widened from the edges, remembered per monitor (drag.rs)
            WM_NCHITTEST => return Some(LRESULT(crate::drag::nc_hit_test(self, lp) as isize)),
            WM_SIZING => {
                crate::drag::sizing(self, wp, lp);
                return Some(LRESULT(1));
            }
            WM_MOVE => crate::drag::moved(self),
            WM_EXITSIZEMOVE => crate::drag::remember(self),
            // HTCAPTION: a double click on the background puts the bar back (drag.rs)
            windows::Win32::UI::WindowsAndMessaging::WM_NCLBUTTONDBLCLK if wp.0 == 2 => {
                crate::drag::put_back(self)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wp.0 as u32;
                if crate::effects::key_down(self, vk, lp.0)
                    || crate::commands::key_down(self, vk, mods())
                {
                    return Some(LRESULT(0));
                }
                let host = self.bar.hwnd;
                let outcome = self
                    .model
                    .key_down(vk, mods(), &self.engine, || clip::get(host));
                match outcome {
                    Outcome::Ignored => return None,
                    Outcome::Handled => {}
                    Outcome::Hide => self.ops.push(Op::Hide),
                    Outcome::OpenSettings => settings_window::open(self),
                    Outcome::ClearHistory => self.set_history(Vec::new()),
                    Outcome::CloseSheet => {
                        if !crate::keys::close_front_sheet(self) {
                            self.ops.push(Op::Hide);
                        }
                    }
                }
            }
            WM_CHAR => {
                crate::perf::mark(crate::perf::Stage::Char);
                if let Some(run) = &mut self.run {
                    run.pending_chars = run.pending_chars.saturating_sub(1);
                    run.keyed = true;
                }
                if !crate::commands::char16(self, wp.0 as u16) {
                    self.model.char16(wp.0 as u16, &self.engine);
                }
                crate::perf::mark(crate::perf::Stage::Submitted);
            }
            // Alt+letter isn't the bar's: no beep
            WM_SYSCHAR => {}
            // the composition is drawn inline (input.rs), not in the IME's own window
            WM_IME_SETCONTEXT => {
                // SAFETY: the default for our own window, without ISC_SHOWUICOMPOSITIONWINDOW
                return Some(unsafe {
                    DefWindowProcW(hwnd, msg, wp, LPARAM(lp.0 & !0x8000_0000isize))
                });
            }
            WM_IME_STARTCOMPOSITION => {
                ime::forget();
                self.place_ime();
                return Some(LRESULT(0));
            }
            WM_IME_ENDCOMPOSITION => {
                self.bar.field_view.composition = None;
                self.model.touch();
                return Some(LRESULT(0));
            }
            WM_IME_COMPOSITION => {
                if lp.0 as u32 & GCS_RESULTSTR.0 != 0 {
                    self.bar.field_view.composition = None;
                    if let Some(text) = ime::result(hwnd) {
                        self.model.insert_text(&text, &self.engine);
                    }
                }
                if lp.0 as u32 & GCS_COMPSTR.0 != 0 {
                    let (text, cursor) = ime::composition(hwnd);
                    self.bar.field_view.composition =
                        (!text.is_empty()).then_some(crate::input::Composition { text, cursor });
                    self.model.touch();
                }
                self.place_ime();
                return Some(LRESULT(0));
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                let (x, y) = self.dips(lp);
                self.click(x, y, msg == WM_LBUTTONDBLCLK);
            }
            WM_MOUSEMOVE => {
                let (x, y) = self.dips(lp);
                crate::commands::pointer(self, Some((x, y)));
                if crate::systems::mouse_move(self, x, y) || crate::graph::mouse_move(self, x, y) {
                    return Some(LRESULT(0));
                }
                if self.bar.selecting {
                    let (x, y) = self.dips(lp);
                    let pos = self.bar.field_view.hit(x, y);
                    self.model.field.set_caret_utf16(pos, true);
                    self.model.touch();
                }
            }
            WM_LBUTTONUP => {
                let (x, y) = self.dips(lp);
                if crate::systems::mouse_up(self) || crate::graph::mouse_up(self, x, y) {
                    return Some(LRESULT(0));
                }
                if self.bar.selecting {
                    self.bar.selecting = false;
                    // SAFETY: we hold the capture
                    let _ = unsafe { ReleaseCapture() };
                }
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                if !crate::graph::wheel(self, msg == WM_MOUSEHWHEEL, wp, lp) {
                    if msg != WM_MOUSEWHEEL {
                        return None;
                    }
                    history::wheel(self, wp, lp);
                }
            }
            windows::Win32::UI::Controls::WM_MOUSELEAVE => {
                crate::commands::pointer(self, None);
                crate::graph::leave(self);
            }
            WM_SETCURSOR => {
                if (lp.0 & 0xffff) as u32 != 1 {
                    return None; // not HTCLIENT
                }
                let mut pt = POINT::default();
                // SAFETY: live out point; our window
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    let _ = ScreenToClient(hwnd, &mut pt);
                }
                let s = self.bar.scale();
                let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
                let panel = crate::graph::cursor(self, x, y)
                    .or_else(|| crate::systems::cursor(self, x, y))
                    .or_else(|| crate::hover::cursor(self, x, y));
                let over = self.bar.layout.field.contains(x, y);
                // SAFETY: system cursors
                unsafe {
                    if let Ok(c) = LoadCursorW(
                        None,
                        panel.unwrap_or(if over { IDC_IBEAM } else { IDC_ARROW }),
                    ) {
                        SetCursor(Some(c));
                    }
                }
                return Some(LRESULT(1));
            }
            _ => return None,
        }
        Some(LRESULT(0))
    }

    fn dips(&self, lp: LPARAM) -> (f32, f32) {
        let s = self.bar.scale();
        (lo(lp.0) as f32 / s, hi(lp.0) as f32 / s)
    }

    fn click(&mut self, x: f32, y: f32, double: bool) {
        if crate::line::click_chip(&mut self.model, x, y, &self.engine)
            || crate::commands::click(self, x, y)
            || crate::systems::click(self, x, y, double, mods().shift)
            || crate::graph::click(self, x, y, double)
        {
            return;
        }
        let l = &self.bar.layout;
        if l.field.contains(x, y)
            || (y >= l.composer.y && y < l.composer.bottom() && x < l.answer.x)
        {
            let pos = self.bar.field_view.hit(x, y);
            if double {
                let i = self.model.field.index_of_utf16(pos);
                self.model.field.select_word_at(i);
            } else {
                let extend = mods().shift;
                self.model.field.set_caret_utf16(pos, extend);
                self.bar.selecting = true;
                // SAFETY: our bar
                unsafe { SetCapture(self.bar.hwnd) };
            }
            self.model.touch();
            return;
        }
        if l.answer.contains(x, y) {
            if let Some(copy) = crate::answer::hit(&l.live, x, y) {
                crate::answer::copy(&mut self.model, &copy);
            }
            return;
        }
        history::click(self, x, y);
    }

    fn place_ime(&self) {
        let s = self.bar.scale();
        let r = self.bar.layout.field;
        ime::place_if_moved(
            self.bar.hwnd,
            (self.bar.field_view.caret_at * s) as i32,
            ((r.y + r.h - 6.0) * s) as i32,
        );
    }

    // ---- model plumbing ----

    /// The line changed outside the field's own keys (a commit's new input, a row insert, the bench).
    pub fn text_changed(&mut self) {
        self.model.text_changed(&self.engine);
    }

    /// A new tape after a commit or an edit: kept, shown, and saved off the UI thread.
    pub fn set_history(&mut self, rows: Vec<HistoryRow>) {
        self.model.rows = Rows::new(rows.clone());
        self.model.touch();
        history::save(&self.engine, rows);
    }

    /// The tape loaded at launch.
    pub fn set_history_loaded(&mut self, rows: Vec<HistoryRow>) {
        self.model.rows = Rows::new(rows);
        self.model.touch();
        if !self.model.field.text().trim().is_empty() {
            self.model.evaluate_live(&self.engine);
        }
    }

    /// After every message: lay out and repaint if the model changed; save settings; write the clipboard.
    pub(crate) fn after(&mut self) {
        if self.model.take_changed() {
            crate::effects::after(self);
            let layout = Bar::layout(&self.gfx, &self.model, self.bar.width, now_ms());
            let resized = (layout.height - self.bar.layout.height).abs() > 0.1
                || (layout.anchor - self.bar.layout.anchor).abs() > 0.1;
            self.bar.layout = layout;
            crate::perf::mark(crate::perf::Stage::Laidout);
            if self.bar.visible {
                if resized {
                    self.ops.push(Op::Reframe);
                }
                self.needs_paint = true;
            }
        }
        if std::mem::take(&mut self.model.settings_dirty) {
            self.store.settings = self.model.settings.clone();
            self.save_store();
            settings_window::refresh(self);
        }
        crate::line::after(self);
        if let Some(text) = self.model.to_clipboard.take()
            && !clip::set(self.bar.hwnd, &text)
        {
            qlog!("clipboard: couldn't write");
        }
    }

    fn paint(&mut self) {
        crate::perf::paint_start();
        if let Err(e) = self.bar.paint(&self.gfx, &self.theme, &self.model) {
            qlog!("paint failed: {e}");
        }
        crate::perf::painted();
        if self.bar.visible && !self.ready_logged {
            self.ready_logged = true;
            qlog!(
                "ready (shown, first frame) at +{:.1} ms since process start",
                log::since_start_ms()
            );
        }
        if self.bar.focused {
            self.place_ime();
        }
        if let Some(run) = &mut self.run
            && run.painted(&self.model)
        {
            // SAFETY: our host's timer and window
            unsafe {
                let _ = KillTimer(Some(self.host), TIMER_BENCH);
                let _ = PostMessageW(Some(self.host), WM_BENCH_NEXT, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn save_store(&mut self) {
        if let Some(p) = &self.store_path
            && let Err(e) = self.store.save(p)
        {
            qlog!("settings: save failed: {e}");
        }
    }

    // ---- show / hide ----

    fn toggle(&mut self) {
        // SAFETY: plain query
        let front = unsafe { GetForegroundWindow() };
        // a game in the bar's place is the bar while it's up: the hotkey closes or fronts it (games/process.rs)
        if !self.bar.visible && crate::games::hotkey() {
            return;
        }
        if self.bar.visible && front == self.bar.hwnd {
            self.ops.push(Op::Hide);
        } else {
            self.ops.push(Op::Show);
        }
    }

    /// Where the bar opens; the model readied for a showing. None when it is already up and in front.
    fn prepare_show(&mut self) -> Option<(HWND, i32, i32, i32, i32)> {
        let monitor = window::cursor_monitor();
        let now_s = (now_ms() / 1000) as u64;
        if !self.bar.visible
            && self.store.moved_or_widened()
            && crate::place::spot_expired(now_s, self.store.used_at, self.store.spot_seconds)
        {
            self.store.positions.clear();
            self.store.width = 0.0;
        }
        self.bar.width = if self.store.width > 0.0 {
            crate::place::fitted_width(self.store.width, monitor.area.width / monitor.scale())
                as f32
        } else {
            crate::place::MIN_WIDTH as f32
        };
        if !self.bar.visible {
            self.engine.warm_up();
            // the line outlives a hide only for draftSeconds (draft.json); the open counts for the examples
            crate::line::will_show(&mut self.model, &mut self.store.onboarding, &self.engine);
            self.model.hint = self.notice.clone();
            self.model.tape.hotkey = self.store.hotkey.clone();
            self.model.field.select_all();
            crate::analytics::track(self, "open");
            // after line.rs counted the open: the tour reads the count, and leaves the examples to line.rs
            crate::tour::will_show(self);
            crate::messaging::will_show(self);
        }
        self.bar.surface.set_dpi(monitor.dpi);
        let layout = Bar::layout(&self.gfx, &self.model, self.bar.width, now_ms());
        let saved = self.store.positions.get(&monitor.key).copied();
        let (x, y, w, h) = self.bar.placement(
            &monitor,
            saved,
            self.bar.width,
            layout.height,
            layout.anchor,
        );
        self.bar.composer_at = (
            x,
            y + (layout.anchor * monitor.scale() as f32).round() as i32,
        );
        self.bar.layout = layout;
        self.bar.monitor = Some(monitor);
        self.bar.visible = true;
        self.hidden_at = None;
        // SAFETY: our host's timer
        let _ = unsafe { KillTimer(Some(self.host), TIMER_TRIM) };
        self.model.touch();
        Some((self.bar.hwnd, x, y, w, h))
    }

    fn after_hide(&mut self) {
        crate::line::will_hide(&mut self.model, &self.engine);
        self.bar.visible = false;
        self.bar.focused = false;
        self.bar.selecting = false;
        self.model.selected = None;
        self.model.tape_open = false;
        self.model.hint = None;
        crate::commands::will_hide(&mut self.model);
        self.model.touch();
        self.hidden_at = Some(Instant::now());
        if self.store.moved_or_widened() {
            self.store.used_at = (now_ms() / 1000) as u64;
            self.save_store();
        }
        // SAFETY: our host's timer (one-shot: trim() kills it)
        unsafe { SetTimer(Some(self.host), TIMER_TRIM, TRIM_AFTER_MS, None) };
        // what only a showing needs goes now (the layouts, the Typst thread): ~3.5 MB back, remade as needed
        crate::answer::trim_hidden(self);
        // the swap chain and the WARP device too (~2.5 MB, and D3D10Warp unloads): a showing makes them in ~4 ms
        self.bar.surface.release();
        self.gfx.drop_flip_device();
        // and the engine's compiled regexes (engine.rs), compiled again with the next showing's warm-up
        self.engine.release();
    }

    /// The bar has been hidden a while: give memory back (the working set; a hardware render target too).
    fn trim(&mut self) {
        // SAFETY: our host's timer
        let _ = unsafe { KillTimer(Some(self.host), TIMER_TRIM) };
        if self.bar.visible {
            return;
        }
        // a GPU target holds the device's memory; the software one is a few hundred KB and quicker kept
        if crate::render::hardware() {
            self.bar.surface.release();
        }
        crate::answer::trim_hidden(self);
        let (ws, private) = bench::memory();
        // SAFETY: our own process
        let _ = unsafe {
            windows::Win32::System::ProcessStatus::K32EmptyWorkingSet(GetCurrentProcess())
        };
        let (ws2, _) = bench::memory();
        qlog!(
            "trimmed while hidden: working set {:.1} → {:.1} MB, private {:.1} MB",
            ws as f64 / 1e6,
            ws2 as f64 / 1e6,
            private as f64 / 1e6
        );
    }

    /// Right after launch, off the first frame's path: the bar's render target and DirectWrite's font cache, so
    /// the first showing (from the tray, at sign-in) is as quick as the next.
    fn warm(&mut self) {
        let t = Instant::now();
        if !self.bar.visible {
            let layout = Bar::layout(&self.gfx, &self.model, self.bar.width, now_ms());
            self.bar.layout = layout;
            let _ = self.bar.paint(&self.gfx, &self.theme, &self.model);
        }
        for s in ["0123456789+-×÷=.", "Calculate", "sin(90) = 1"] {
            let _ = self.gfx.layout(s, &self.gfx.fonts.display, 0.0, 36.0);
            let _ = self.gfx.layout(s, &self.gfx.fonts.body, 0.0, 36.0);
            let _ = self.gfx.layout(s, &self.gfx.fonts.row, 0.0, 36.0);
        }
        qlog!("bar warmed in {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
        crate::bg::start(self);
        crate::messaging::start();
        if !self.ready_logged && !self.bar.visible {
            self.ready_logged = true;
            qlog!(
                "ready (in the tray) at +{:.1} ms since process start",
                log::since_start_ms()
            );
        }
    }

    // ---- bench ----

    fn start_bench_file(&mut self, path: &str) {
        match std::fs::read_to_string(path).map(|t| serde_json::from_str::<bench::Spec>(&t)) {
            Ok(Ok(spec)) => self.start_bench(spec, None),
            other => qlog!("bench: bad spec {path}: {other:?}"),
        }
    }

    fn start_bench(&mut self, spec: bench::Spec, reply: Option<std::sync::mpsc::Sender<String>>) {
        qlog!("bench: {} inputs", spec.inputs.len());
        if !self.bar.visible {
            self.ops.push(Op::Show);
        }
        self.model.field.reset("");
        self.model.text_changed(&self.engine);
        self.run = Some(Run::new(spec, reply));
        // SAFETY: our host
        let _ = unsafe { PostMessageW(Some(self.host), WM_BENCH_NEXT, WPARAM(0), LPARAM(0)) };
    }

    fn bench_next(&mut self) {
        let host = self.host;
        let bar = self.bar.hwnd;
        let Some(run) = &mut self.run else { return };
        if run.k >= run.steps.len() {
            self.finish_bench();
            return;
        }
        let step = run.steps[run.k].clone();
        run.k += 1;
        let arm = || {
            // SAFETY: our host's timer
            unsafe { SetTimer(Some(host), TIMER_BENCH, BENCH_STEP_MS, None) };
        };
        match step {
            Step::Key {
                input,
                units,
                expect,
            } => {
                run.current_input = input;
                run.wait = Wait::Answer(expect, self.model.field.text().to_string());
                run.keyed = false;
                crate::perf::reset();
                run.t0 = Instant::now();
                run.pending_chars = units.len();
                for u in units {
                    // SAFETY: posting typed characters to our bar, as the keyboard would
                    let _ = unsafe {
                        PostMessageW(Some(bar), WM_CHAR, WPARAM(usize::from(u)), LPARAM(1))
                    };
                }
                arm();
            }
            Step::Enter => {
                run.wait = Wait::Commit(self.model.field.text().to_string());
                // SAFETY: as above
                let _ = unsafe {
                    PostMessageW(
                        Some(bar),
                        WM_KEYDOWN,
                        WPARAM(usize::from(VK_RETURN.0)),
                        LPARAM(1),
                    )
                };
                arm();
            }
            Step::Clear => {
                run.wait = Wait::None;
                self.model.field.reset("");
                self.model.text_changed(&self.engine);
                // SAFETY: our host
                let _ = unsafe { PostMessageW(Some(host), WM_BENCH_NEXT, WPARAM(0), LPARAM(0)) };
            }
        }
    }

    fn bench_timeout(&mut self) {
        // SAFETY: our host's timer
        let _ = unsafe { KillTimer(Some(self.host), TIMER_BENCH) };
        if let Some(run) = &mut self.run {
            run.timeouts += 1;
            qlog!("bench: step {} timed out waiting for {:?}", run.k, run.wait);
            run.wait = Wait::None;
            self.bench_next();
        }
    }

    fn finish_bench(&mut self) {
        let Some(run) = self.run.take() else { return };
        let result = run.result();
        qlog!(
            "bench: n {} median {:.3} ms p90 {:.3} ms p99 {:.3} ms timeouts {}",
            result["n"],
            result["median"].as_f64().unwrap_or(0.0),
            result["p90"].as_f64().unwrap_or(0.0),
            result["p99"].as_f64().unwrap_or(0.0),
            run.timeouts
        );
        let text = result.to_string();
        if let Some(out) = &run.spec.out
            && let Err(e) = std::fs::write(out, &text)
        {
            qlog!("bench: couldn't write {out}: {e}");
        }
        if let Some(reply) = run.reply {
            let _ = reply.send(text);
        }
        if run.spec.quit {
            self.ops.push(Op::Quit);
        }
    }

    fn on_pipe(&mut self, req: bench::pipe::Request) {
        let spec: bench::Spec = match serde_json::from_str(&req.text) {
            Ok(s) => s,
            Err(e) => {
                let _ = req
                    .reply
                    .send(serde_json::json!({ "error": e.to_string() }).to_string());
                return;
            }
        };
        let Some(cmd) = spec.cmd.clone() else {
            self.start_bench(spec, Some(req.reply));
            return;
        };
        let reply = |v: serde_json::Value| {
            let _ = req.reply.send(v.to_string());
        };
        match cmd.as_str() {
            "show" => {
                self.ops.push(Op::Show);
                reply(serde_json::json!({ "ok": true }));
            }
            "hide" => {
                self.ops.push(Op::Hide);
                reply(serde_json::json!({ "ok": true }));
            }
            "type" => {
                for u in spec.text.unwrap_or_default().encode_utf16() {
                    // SAFETY: posting typed characters to our bar
                    let _ = unsafe {
                        PostMessageW(
                            Some(self.bar.hwnd),
                            WM_CHAR,
                            WPARAM(usize::from(u)),
                            LPARAM(1),
                        )
                    };
                }
                reply(serde_json::json!({ "ok": true }));
            }
            "key" => {
                let vk = spec.vk.unwrap_or(0) as usize;
                // SAFETY: as above
                let _ =
                    unsafe { PostMessageW(Some(self.bar.hwnd), WM_KEYDOWN, WPARAM(vk), LPARAM(1)) };
                reply(serde_json::json!({ "ok": true }));
            }
            "state" => {
                let rows: Vec<serde_json::Value> = self
                    .model
                    .rows
                    .rows
                    .iter()
                    .map(|r| serde_json::json!({ "expr": r.expr, "display": r.display }))
                    .collect();
                let r: RECT = self.bar.frame();
                reply(serde_json::json!({
                    "text": self.model.field.text(), "answer": self.model.answer_text(), "liveFor": self.model.live_for,
                    "hint": self.model.hint, "rows": rows, "visible": self.bar.visible, "focused": self.bar.focused,
                    "frame": [r.left, r.top, r.right, r.bottom], "hotkey": self.hotkey.active,
                    "graph": crate::graph::debug(&self.model), "systems": crate::systems::debug(&self.model),
                    "slot": self.model.command.as_ref().map(|c| c.message.clone()), "sheets": crate::sheets::open_keys(),
                    "game": crate::games::is_open(), "settingsOpen": settings_window::is_open(),
                    "cheats": crate::cheats::height(&self.model) > 0.0, "gamesList": crate::commands::games_shown(&self.model),
                    "tour": self.model.tour.line.as_ref().map(|l| l.text.clone()),
                    "caret": self.model.field.to_utf16(self.model.field.caret()),
                    "selectionAnswer": self.model.line.selection_answer,
                    "shown": crate::answer::shown_answer(&self.model),
                }));
            }
            "mem" => {
                let (ws, private) = bench::memory();
                reply(
                    serde_json::json!({ "workingSet": ws, "private": private, "visible": self.bar.visible }),
                );
            }
            "trim" => {
                self.trim();
                let (ws, private) = bench::memory();
                reply(serde_json::json!({ "workingSet": ws, "private": private }));
            }
            "settings" => {
                settings_window::open_named(self, spec.text.as_deref().unwrap_or(""));
                reply(serde_json::json!({ "ok": true }));
            }
            "quit" => {
                reply(serde_json::json!({ "ok": true }));
                self.ops.push(Op::Quit);
            }
            other => {
                reply(bench::extra(self, other).unwrap_or_else(
                    || serde_json::json!({ "error": format!("unknown cmd {other}") }),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        let a = Args::parse(&["--autostart".into()]);
        assert!(a.autostart);
        let b = Args::parse(&["--bench".into(), "x.json".into(), "--bench-pipe".into()]);
        assert_eq!(b.bench.as_deref(), Some("x.json"));
        assert!(b.bench_pipe);
    }
}
