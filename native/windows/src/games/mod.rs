//! The native games: pong, crack, connect 4, ultimate tic-tac-toe, equal and 24 / countdown, each in a game
//! window of its own drawn with Direct2D, over the core's rules, computers and online sessions
//! (`qcalc_core::games`). The Mac's spec is native/mac/Sources/Games (Shell/, the panels, GameWindowController).
//! README.md in this folder has the map, the window's behaviour and the hooks for the next two games.
//!
//! - `open(kind)` opens a game (the bar's `pong` + ↵, the games list); `command(text)` and `hint(text)` are the
//!   bar's side of it (model.rs).
//! - Everything here lives on the UI thread in this module's own thread-local, apart from the app's: a game window
//!   never borrows `App`, so it can open from inside an app handler and runs while the bar is hidden.
//! - Nothing is made until a game opens (the Direct2D factory and hardware target, fonts, saves, the relay link),
//!   and closing the window drops all of it.

mod connect4;
mod crack;
mod equal;
mod gfx;
mod keys;
mod lobby;
mod numbers;
mod pong;
mod relay;
mod shell;
mod store;
mod svg;
mod theme;
mod ultimate;
mod widgets;

use std::cell::RefCell;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use qcalc_core::games::clock::GameClock;
use qcalc_core::games::fullscreen::{FullscreenLayoutOptions, Size, fullscreen_layout};
use qcalc_core::games::peer::PeerEvent;
use qcalc_core::games::shell::{EscAction, ShellMode};
use qcalc_core::games::{Random, Side};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmFlush, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL,
    VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_STYLE, GetClientRect, GetWindowLongPtrW, GetWindowRect,
    ICON_BIG, ICON_SMALL, IDC_ARROW, IsIconic, IsWindowVisible, KillTimer, LoadCursorW, MINMAXINFO,
    RegisterClassExW, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    WA_INACTIVE, WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETICON, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_EX_APPWINDOW,
    WS_MINIMIZEBOX, WS_OVERLAPPED, WS_OVERLAPPEDWINDOW, WS_SYSMENU, WS_VISIBLE,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use self::gfx::{Gfx, Hit, Rect, Ui};
use self::keys::{GameKey, Mods, Tracker};
use self::lobby::Lobby;
use self::relay::Relay;
use self::shell::{Shell, ShellAct};
use self::store::GameStore;
use self::theme::{Colors, m};
use crate::qlog;

// ---- what a game is

/// A game's full-screen area: its height (`--game-h`) and width, for boards that grow with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    pub game_h: f32,
    pub width: f32,
}

/// A game that's on (`LocalGames.play` or `LobbyConfig.online`'s view, with its model). It draws itself under
/// the lobby's head, takes keys after the shell, and asks the shell for things through `Env`.
pub trait Play {
    /// Draws at (x, y), `w` wide; returns the height it took. Something that moves calls `ui.animate()`.
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, w: f32, env: &mut Env) -> f32;
    /// A key the shell didn't take; true keeps it from the rest.
    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool;
    /// A click on one of its `Hit::Act` regions.
    fn act(&mut self, _id: u32, _env: &mut Env) {}
    /// The pointer over (press false) or pressed on (press true) one of its `Hit::Board` regions, in window DIPs.
    fn pointer(&mut self, _id: u32, _x: f32, _y: f32, _press: bool, _env: &mut Env) {}
    fn pointer_up(&mut self, _env: &mut Env) {}
    fn pointer_left(&mut self) {}
    /// Every 200 ms while `ticks` says so (a computer's move, the link's silences, a countdown).
    fn tick(&mut self, _env: &mut Env) {}
    fn ticks(&self) -> bool {
        false
    }
    /// The link said something (a message from the other side, or the close).
    fn heard(&mut self, _e: &PeerEvent, _env: &mut Env) {}
    /// Nothing is running, so esc leaves without a pause.
    fn idle(&self) -> bool {
        false
    }
    /// Esc's leave, when the game has somewhere nearer to go than the lobby; false goes to the lobby.
    fn leave(&mut self, _env: &mut Env) -> bool {
        false
    }
    /// The window lost the keyboard: let go of held keys.
    fn release(&mut self) {}
    /// It's going (back to the lobby, or the window closed).
    fn closing(&mut self, _env: &mut Env) {}
}

/// Starts a game over the link: its side, the other side's name.
pub type OnlinePlay = fn(Side, String, &mut Env) -> Box<dyn Play>;
/// Draws under the lobby's options at (x, y), w wide; returns its height.
pub type Footer = fn(&mut Ui, f32, f32, f32, &mut Env) -> f32;

/// Everything `<GameLobby …>` took as props, and the bar's word for the game. One per game in `GAMES`.
pub struct GameDef {
    /// The kind the room lists go by (`pong`, `connect4`…).
    pub kind: &'static str,
    /// What the bar and the window call it (`connect 4`, `ultimate tic-tac-toe`).
    pub title: &'static str,
    /// The lobby's heading.
    pub name: &'static str,
    /// The line under the heading.
    pub blurb: &'static str,
    /// The bar's hint while the command is typed.
    pub hint: &'static str,
    /// Whether a line (lower case, spaces collapsed) is this game's command.
    pub matches: fn(&str) -> bool,
    /// The line over the solo options, and the options; then two on one keyboard.
    pub prompt: &'static str,
    pub options: &'static [&'static str],
    pub together: &'static [&'static str],
    /// The game on this computer: `option` counts through options, then together.
    pub local: fn(usize, &mut Env) -> Box<dyn Play>,
    /// The game over the link; None: no online play.
    pub online: Option<OnlinePlay>,
    /// The head's line when there's no link.
    pub sub: fn(&GameStore) -> String,
    /// More under the solo options (24's large numbers).
    pub footer: Option<Footer>,
    /// The game's own lobby keys, before the lobby's.
    pub lobby_key: Option<fn(&GameKey, &mut Env) -> bool>,
    /// The game over the link draws its own head.
    pub own_head: bool,
}

/// The games, in the order the games list shows them. Ember & frost and trails slot in here (README.md).
pub static GAMES: [&GameDef; 6] = [
    &pong::DEF,
    &connect4::DEF,
    &ultimate::DEF,
    &crack::DEF,
    &equal::DEF,
    &numbers::DEF,
];

fn no_sub(_: &GameStore) -> String {
    String::new()
}

/// A line as the commands read it: trimmed, spaces collapsed, lower case.
pub fn normalize(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The game a bar line names (`pong`, `connect 4`, `24 game`…).
pub fn command(text: &str) -> Option<&'static str> {
    let t = normalize(text);
    if t.is_empty() {
        return None;
    }
    GAMES.iter().find(|g| (g.matches)(&t)).map(|g| g.kind)
}

/// What the bar says under a game's command (`↵ play pong`).
pub fn hint(text: &str) -> Option<String> {
    let kind = command(text)?;
    GAMES.iter().find(|g| g.kind == kind).map(|g| g.hint.to_string())
}

/// The games for a list (kind, title).
pub fn list() -> Vec<(&'static str, &'static str)> {
    GAMES.iter().map(|g| (g.kind, g.title)).collect()
}

// ---- what a game reaches

/// The shell's state and services, as a game sees them.
pub struct Env {
    pub shell: Shell,
    clock: GameClock,
    origin: Instant,
    pub store: GameStore,
    pub rand: Random,
    relay: Option<Relay>,
    link: bool,
    hwnd: HWND,
    /// the game's messages for the other side
    pub sent: Vec<String>,
    /// the game is over: back to the lobby with this line
    pub end: Option<Option<String>>,
    /// the game's esc button
    pub escape: bool,
    pub toggle_fullscreen: bool,
    pub clipboard: Option<String>,
    /// full screen's area, while it's on
    pub stage: Option<Stage>,
}

impl Env {
    fn mono(&self) -> f64 {
        self.origin.elapsed().as_secs_f64() * 1000.0
    }

    /// `gameNow()`: ms that stand still while paused.
    pub fn now(&self) -> f64 {
        self.clock.now(self.mono())
    }

    /// `gameDateNow()`.
    pub fn date(&self) -> f64 {
        self.clock.date(self.mono(), self.wall())
    }

    /// `Date.now()`.
    pub fn wall(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }

    pub fn rand(&mut self) -> f64 {
        self.rand.next_f64()
    }

    pub fn nickname(&mut self) -> String {
        let Env { store, rand, .. } = self;
        store.nickname(&mut || rand.next_f64())
    }

    /// The relay, made the first time the lobby needs it (only for a game with online play).
    pub fn relay_mut(&mut self) -> Option<&mut Relay> {
        if self.link && self.relay.is_none() {
            self.relay = Some(Relay::new(self.hwnd));
        }
        self.relay.as_mut()
    }

    pub fn set_mode(&mut self, next: ShellMode) {
        if next != ShellMode::Local {
            self.resume();
        }
        self.shell.armed_at = None;
        self.shell.mode = next;
    }

    pub fn pause(&mut self) {
        if !self.shell.paused {
            let mono = self.mono();
            self.clock.pause(mono);
            self.shell.paused = true;
        }
    }

    pub fn resume(&mut self) {
        if self.shell.paused {
            let mono = self.mono();
            self.clock.resume(mono);
            self.shell.paused = false;
        }
    }

    /// The link dropped mid-game: the game clock stands still until it's back.
    pub fn clock_pause(&mut self) {
        let mono = self.mono();
        self.clock.pause(mono);
    }

    pub fn clock_resume(&mut self) {
        if !self.shell.paused {
            let mono = self.mono();
            self.clock.resume(mono);
        }
    }
}

// ---- the window

const CLASS: PCWSTR = w!("QCalcGame");
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_TICK: usize = 1;
const TIMER_ARM: usize = 2;
const TIMER_RELAY: usize = 3;
const TIMER_FRAME: usize = 4;
const NORMAL_STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0);

struct Win {
    hwnd: HWND,
    gfx: Gfx,
    colors: Colors,
    theme: String,
    lobby: Lobby,
    env: Env,
    keys: Tracker,
    mouse: Option<(f32, f32)>,
    hits: Vec<(Rect, Hit)>,
    hovered: Option<(Rect, Hit)>,
    /// the board under the pointer, for its leave
    on_board: bool,
    tracking: bool,
    /// the card's height at the last frame (DIPs)
    content_h: f32,
    t0: Instant,
    last_frame: Instant,
    ticking: bool,
    /// the last frame drew something that moves
    animating: bool,
    /// the window's top-left while not full screen
    normal_at: Option<(i32, i32)>,
    closed: bool,
}

/// Window work that sends messages back, run once the borrow ends.
enum Op {
    Pos(HWND, i32, i32, i32, i32, bool),
    Style(HWND, WINDOW_STYLE),
    Destroy(HWND),
    Drop,
}

thread_local! {
    static WIN: RefCell<Option<Win>> = const { RefCell::new(None) };
    static OPS: RefCell<Vec<Op>> = const { RefCell::new(Vec::new()) };
    static PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn push(op: Op) {
    OPS.with(|o| o.borrow_mut().push(op));
}

/// Opens a game window for `kind` (closing one that's open), on the next turn of the message loop, so it can be
/// called from anywhere on the UI thread, inside an app handler or not.
pub fn open(kind: &str) {
    if !GAMES.iter().any(|g| g.kind == kind) {
        qlog!("games: no game called {kind}");
        return;
    }
    PENDING.with(|p| *p.borrow_mut() = Some(kind.to_string()));
    // a thread timer with a callback: DispatchMessage runs it, whatever window the loop is serving
    // SAFETY: plain timer on this thread
    unsafe { SetTimer(None, 0, 0, Some(open_now)) };
}

/// Whether a game window is open.
pub fn is_open() -> bool {
    WIN.with(|w| w.try_borrow().map(|w| w.is_some()).unwrap_or(true))
}

unsafe extern "system" fn open_now(_: HWND, _: u32, id: usize, _: u32) {
    // SAFETY: our own one-shot thread timer
    let _ = unsafe { KillTimer(None, id) };
    let Some(kind) = PENDING.with(|p| p.borrow_mut().take()) else {
        return;
    };
    let old = WIN.with(|w| w.try_borrow().ok().and_then(|w| w.as_ref().map(|w| w.hwnd)));
    if let Some(old) = old {
        // SAFETY: our window; its WM_DESTROY drops it
        let _ = unsafe { DestroyWindow(old) };
        run_ops();
    }
    if let Err(e) = create(&kind) {
        qlog!("games: couldn't open {kind}: {e}");
    }
}

fn theme_setting() -> String {
    crate::app::with_app(|a| a.store.settings.theme.clone()).unwrap_or_else(|| {
        crate::store::settings_path()
            .map(|p| crate::store::Store::load(&p).settings.theme)
            .unwrap_or_default()
    })
}

fn create(kind: &str) -> windows::core::Result<()> {
    let Some(def) = GAMES.iter().copied().find(|g| g.kind == kind) else {
        return Ok(());
    };
    let t = Instant::now();
    let monitor = crate::window::cursor_monitor();
    // SAFETY: registering our class (again is harmless) and making a hidden window with our wndproc
    let hwnd = unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WS_EX_APPWINDOW,
            CLASS,
            &HSTRING::from(def.title),
            NORMAL_STYLE,
            monitor.area.x as i32,
            monitor.area.y as i32,
            400,
            300,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };
    // SAFETY: a plain query on our window
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let setting = theme_setting();
    let dark = crate::render::Theme::for_setting(&setting).dark;
    set_dark(hwnd, dark);
    set_icons(hwnd, dpi, dark);
    let mut gfx = Gfx::new(hwnd, dpi)?;
    let scale = dpi as f32 / 96.0;
    gfx.resize((m::PANEL_W * scale) as u32, (300.0 * scale) as u32);
    let mut r = Random::from_entropy();
    let env = Env {
        shell: Shell::default(),
        clock: GameClock::new(),
        origin: Instant::now(),
        store: GameStore::load(),
        rand: Random::new((r.next_f64() * 9e15) as u64),
        relay: None,
        link: def.online.is_some(),
        hwnd,
        sent: Vec::new(),
        end: None,
        escape: false,
        toggle_fullscreen: false,
        clipboard: None,
        stage: None,
    };
    let mut win = Win {
        hwnd,
        gfx,
        colors: Colors::new(dark),
        theme: setting,
        lobby: Lobby::new(def),
        env,
        keys: Tracker::default(),
        mouse: None,
        hits: Vec::new(),
        hovered: None,
        on_board: false,
        tracking: false,
        content_h: 300.0,
        t0: Instant::now(),
        last_frame: Instant::now(),
        ticking: false,
        animating: false,
        normal_at: None,
        closed: false,
    };
    win.lobby.attach(&mut win.env);
    // one frame while hidden, so the window opens at the card's height
    win.paint();
    let (cw, ch) = (m::PANEL_W, win.content_h);
    let (w, h) = outer(hwnd, NORMAL_STYLE, cw, ch, dpi);
    let s = monitor.scale();
    let x = (monitor.area.x + (monitor.area.width - f64::from(w)) / 2.0) as i32;
    let y = (monitor.area.y + (monitor.area.height * 0.18).min(160.0 * s)) as i32;
    win.normal_at = Some((x, y));
    win.settle(true);
    WIN.with(|c| *c.borrow_mut() = Some(win));
    // SAFETY: our window, shown and given the keyboard
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    run_ops();
    qlog!("games: {kind} opened in {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    Ok(())
}

fn set_dark(hwnd: HWND, dark: bool) {
    let v = BOOL::from(dark);
    // SAFETY: an attribute on our window, pointing at a live local
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

fn set_icons(hwnd: HWND, dpi: u32, dark: bool) {
    for (which, px) in [(ICON_SMALL, 16 * dpi / 96), (ICON_BIG, 32 * dpi / 96)] {
        if let Some(icon) = crate::tray::icon(px, dark) {
            // SAFETY: our window; the icon stays for its life (a few KB, freed with the process)
            unsafe { SendMessageW(hwnd, WM_SETICON, Some(WPARAM(which as usize)), Some(LPARAM(icon.0 as isize))) };
        }
    }
}

/// The window's outer size for a client of (w, h) DIPs.
fn outer(hwnd: HWND, style: WINDOW_STYLE, w: f32, h: f32, dpi: u32) -> (i32, i32) {
    let s = dpi as f32 / 96.0;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: (w * s).round() as i32,
        bottom: (h * s).round() as i32,
    };
    // SAFETY: a live out rect
    let _ = unsafe { AdjustWindowRectExForDpi(&mut r, style, false, WS_EX_APPWINDOW, dpi) };
    let _ = hwnd;
    (r.right - r.left, r.bottom - r.top)
}

fn mods() -> Mods {
    // SAFETY: plain queries of the keyboard state for this message
    let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    Mods {
        ctrl: down(VK_CONTROL.0),
        alt: down(VK_MENU.0),
        shift: down(VK_SHIFT.0),
    }
}

fn lo(v: isize) -> i32 {
    i32::from((v & 0xffff) as u16 as i16)
}

fn hi(v: isize) -> i32 {
    i32::from(((v >> 16) & 0xffff) as u16 as i16)
}

unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let out = WIN.with(|c| {
        let mut slot = c.try_borrow_mut().ok()?;
        let win = slot.as_mut().filter(|w| w.hwnd == hwnd)?;
        let r = win.on(msg, wp, lp);
        if !win.closed {
            // a frame repaints only when something moves (next_frame), never because it was painted
            win.settle(msg != WM_PAINT);
        }
        Some(r)
    });
    run_ops();
    match out {
        Some(Some(r)) => r,
        _ if msg == WM_PAINT => {
            // SAFETY: our window; a paint must validate or it repeats
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            LRESULT(0)
        }
        // SAFETY: the default for our window
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn run_ops() {
    loop {
        let ops = OPS.with(|o| std::mem::take(&mut *o.borrow_mut()));
        if ops.is_empty() {
            return;
        }
        for op in ops {
            match op {
                Op::Pos(hwnd, x, y, w, h, frame) => {
                    let flags = SWP_NOZORDER | SWP_NOACTIVATE | if frame { SWP_FRAMECHANGED } else { Default::default() };
                    // SAFETY: our window
                    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, flags) };
                }
                Op::Style(hwnd, style) => {
                    // SAFETY: our window; it stays visible (WS_VISIBLE is part of the style word)
                    unsafe {
                        let visible = GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_VISIBLE.0 as isize;
                        SetWindowLongPtrW(hwnd, GWL_STYLE, style.0 as isize | visible)
                    };
                }
                Op::Destroy(hwnd) => {
                    // SAFETY: our window; WM_DESTROY comes back and marks it for dropping
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                Op::Drop => {
                    let gone = WIN.with(|c| c.try_borrow_mut().ok().and_then(|mut w| w.take()));
                    if let Some(w) = gone {
                        let kind = w.lobby.def.kind;
                        drop(w);
                        // the bar's own trim ran while the game was up; with the bar still away, trim now so
                        // the working set goes back to where it was before the game
                        let hidden = crate::app::with_app(|a| !a.bar.visible).unwrap_or(false);
                        // what the game freed goes back to the system, not just to the heap
                        // SAFETY: the process heap, compacted
                        let _ = unsafe {
                            windows::Win32::System::Memory::HeapCompact(
                                windows::Win32::System::Memory::GetProcessHeap().unwrap_or_default(),
                                windows::Win32::System::Memory::HEAP_FLAGS(0),
                            )
                        };
                        if hidden {
                            // SAFETY: our own process
                            let _ = unsafe {
                                windows::Win32::System::ProcessStatus::K32EmptyWorkingSet(
                                    windows::Win32::System::Threading::GetCurrentProcess(),
                                )
                            };
                        }
                        let (ws, private) = crate::bench::memory();
                        qlog!(
                            "games: {kind} closed (window, render target and link released); working set {:.1} MB, private {:.1} MB",
                            ws as f64 / 1e6,
                            private as f64 / 1e6
                        );
                    }
                }
            }
        }
    }
}

impl Win {
    fn on(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_PAINT => {
                self.paint();
                // SAFETY: our window
                let _ = unsafe { ValidateRect(Some(self.hwnd), None) };
                self.next_frame();
            }
            WM_ERASEBKGND => return Some(LRESULT(1)),
            WM_SIZE => {
                self.gfx.resize(lo(lp.0) as u32 & 0xffff, hi(lp.0) as u32 & 0xffff);
            }
            WM_DPICHANGED => {
                self.gfx.set_dpi((wp.0 & 0xffff) as u32);
                // SAFETY: lParam is the suggested rect for this message
                let r = unsafe { *(lp.0 as *const RECT) };
                push(Op::Pos(self.hwnd, r.left, r.top, r.right - r.left, r.bottom - r.top, false));
                if self.normal_at.is_some() && !self.env.shell.fullscreen {
                    self.normal_at = Some((r.left, r.top));
                }
            }
            WM_ACTIVATE => {
                if (wp.0 & 0xffff) as u32 == WA_INACTIVE {
                    // a game on this computer pauses when the window loses focus
                    self.keys.reset();
                    if let Some(p) = &mut self.lobby.play {
                        p.release();
                    }
                    if self.env.shell.mode == ShellMode::Local && !self.lobby.idle() {
                        self.env.pause();
                    }
                }
            }
            WM_GETMINMAXINFO => {
                if self.env.shell.fullscreen {
                    let (w, h) = self.fullscreen_min();
                    // SAFETY: lParam is the MINMAXINFO for this message
                    let info = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
                    info.ptMinTrackSize = POINT { x: w, y: h };
                }
                return Some(LRESULT(0));
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wp.0 as u32;
                // Alt+F4 and Alt+Space are the window's
                if msg == WM_SYSKEYDOWN && (vk == 0x73 || vk == 0x20) {
                    return None;
                }
                let scan = ((lp.0 >> 16) & 0xff) as u32;
                let ext = (lp.0 >> 24) & 1 == 1;
                let repeat = (lp.0 >> 30) & 1 == 1;
                if let Some(k) = self.keys.down(vk, scan, ext, repeat, mods())
                    && !self.dispatch(&k)
                    && msg == WM_SYSKEYDOWN
                {
                    return None;
                }
            }
            WM_CHAR => {
                if let Some(k) = self.keys.char(wp.0 as u16, mods()) {
                    self.dispatch(&k);
                }
            }
            WM_SYSCHAR => {
                if wp.0 == 0x20 {
                    return None;
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                let vk = wp.0 as u32;
                let scan = ((lp.0 >> 16) & 0xff) as u32;
                let ext = (lp.0 >> 24) & 1 == 1;
                if let Some(k) = self.keys.up(vk, scan, ext, mods()) {
                    self.dispatch(&k);
                }
                if msg == WM_SYSKEYUP {
                    return None;
                }
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                let p = self.dips(lp);
                self.mouse = Some(p);
                self.click(p.0, p.1);
            }
            WM_LBUTTONUP => {
                // SAFETY: plain call
                let _ = unsafe { ReleaseCapture() };
                if let Some(play) = &mut self.lobby.play {
                    play.pointer_up(&mut self.env);
                }
            }
            WM_MOUSEMOVE => {
                let p = self.dips(lp);
                self.mouse = Some(p);
                if !self.tracking {
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: a live struct for our window
                    self.tracking = unsafe { TrackMouseEvent(&mut t) }.is_ok();
                }
                self.moved(p.0, p.1);
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                self.mouse = None;
                if std::mem::take(&mut self.on_board)
                    && let Some(play) = &mut self.lobby.play
                {
                    play.pointer_left();
                }
                self.hovered = None;
            }
            WM_TIMER => match wp.0 {
                TIMER_TICK => {
                    if let Some(p) = &mut self.lobby.play {
                        p.tick(&mut self.env);
                    }
                }
                TIMER_ARM => {
                    // SAFETY: our window's timer
                    let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_ARM) };
                    self.env.shell.armed_at = None;
                }
                TIMER_RELAY => {
                    // SAFETY: as above
                    let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_RELAY) };
                    let now = self.env.wall();
                    if let Some(r) = &mut self.env.relay {
                        r.peer.tick(now);
                    }
                }
                TIMER_FRAME => {
                    // SAFETY: as above
                    let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_FRAME) };
                }
                _ => return None,
            },
            relay::WM_SOCK => {
                // SAFETY: a WM_SOCK lParam, taken once here
                let e = unsafe { relay::take(lp) };
                let now = self.env.wall();
                if let Some(r) = &mut self.env.relay {
                    r.heard(wp.0 as u64, e, now);
                }
            }
            WM_SETTINGCHANGE => {
                let what = if lp.0 == 0 {
                    String::new()
                } else {
                    // SAFETY: WM_SETTINGCHANGE's lParam is a NUL-terminated string when set
                    unsafe { PCWSTR(lp.0 as *const u16).to_string() }.unwrap_or_default()
                };
                if what == "ImmersiveColorSet" {
                    let dark = crate::render::Theme::for_setting(&self.theme).dark;
                    self.colors = Colors::new(dark);
                    set_dark(self.hwnd, dark);
                    set_icons(self.hwnd, self.dpi(), dark);
                }
                return None;
            }
            WM_CLOSE => push(Op::Destroy(self.hwnd)),
            WM_DESTROY => self.teardown(),
            _ => return None,
        }
        Some(LRESULT(0))
    }

    fn dips(&self, lp: LPARAM) -> (f32, f32) {
        let s = self.gfx.scale();
        (lo(lp.0) as f32 / s, hi(lp.0) as f32 / s)
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<(Rect, Hit)> {
        self.hits.iter().rev().find(|(r, _)| r.contains(x, y)).cloned()
    }

    fn click(&mut self, x: f32, y: f32) {
        let Some((_, hit)) = self.hit_at(x, y) else {
            return;
        };
        let env = &mut self.env;
        match hit {
            Hit::Key(k) => self.lobby.click_key(&k, env),
            Hit::Esc => self.escape(),
            Hit::Fullscreen => env.toggle_fullscreen = true,
            Hit::Resume => env.resume(),
            Hit::Join(i) => self.lobby.join(i, env),
            Hit::Local(i) => self.lobby.local(i, env),
            Hit::Act(id) => {
                if let Some(p) = &mut self.lobby.play {
                    p.act(id, env);
                }
            }
            Hit::Board(id) => {
                // a press on a board is the game's: never a drag of anything
                // SAFETY: our window
                unsafe { SetCapture(self.hwnd) };
                if let Some(p) = &mut self.lobby.play {
                    p.pointer(id, x, y, true, env);
                }
            }
        }
    }

    fn moved(&mut self, x: f32, y: f32) {
        let now = self.hit_at(x, y);
        let board = match &now {
            Some((_, Hit::Board(id))) => Some(*id),
            _ => None,
        };
        if let Some(id) = board {
            self.on_board = true;
            if let Some(p) = &mut self.lobby.play {
                p.pointer(id, x, y, false, &mut self.env);
            }
        } else if std::mem::take(&mut self.on_board)
            && let Some(p) = &mut self.lobby.play
        {
            p.pointer_left();
        }
        self.hovered = now;
    }

    /// A key, through the shell first, then the lobby and the game.
    fn dispatch(&mut self, k: &GameKey) -> bool {
        if let Some(act) = self.env.shell.route(k) {
            match act {
                ShellAct::Nothing => {}
                ShellAct::ToggleFullscreen => self.env.toggle_fullscreen = true,
                ShellAct::ExitFullscreen => self.set_fullscreen(false),
                ShellAct::Resume => self.env.resume(),
                ShellAct::Escape => self.escape(),
                ShellAct::Close => push(Op::Destroy(self.hwnd)),
            }
            return true;
        }
        self.lobby.key(k, &mut self.env)
    }

    /// Esc (`onEscape`): leave full screen, step back through the lobby (closing from its first screen), pause,
    /// warn, or leave.
    fn escape(&mut self) {
        let now = self.env.wall();
        match self.env.shell.esc(self.lobby.idle(), now) {
            EscAction::Unfullscreen => self.set_fullscreen(false),
            EscAction::Lobby => {
                if !self.lobby.back(&mut self.env) {
                    push(Op::Destroy(self.hwnd));
                }
            }
            EscAction::Pause => self.env.pause(),
            EscAction::Arm => {
                self.env.shell.armed_at = Some(now);
                // SAFETY: our window's one-shot timer
                unsafe { SetTimer(Some(self.hwnd), TIMER_ARM, qcalc_core::games::shell::ARM_MS as u32, None) };
            }
            EscAction::Leave => {
                self.env.resume();
                self.env.shell.armed_at = None;
                self.lobby.leave(&mut self.env);
            }
        }
    }

    fn monitor(&self) -> crate::window::Monitor {
        let mut r = RECT::default();
        // SAFETY: a live out rect
        let _ = unsafe { GetWindowRect(self.hwnd, &mut r) };
        crate::window::monitor_at(POINT {
            x: (r.left + r.right) / 2,
            y: (r.top + r.bottom) / 2,
        })
    }

    fn dpi(&self) -> u32 {
        self.gfx.dpi as u32
    }

    fn fullscreen_min(&self) -> (i32, i32) {
        let mon = self.monitor();
        let s = mon.scale();
        let (w, h) = crate::place::game_window_min(mon.area.width / s, mon.area.height / s, Some((f64::from(m::PANEL_W), 392.0)));
        outer(self.hwnd, WS_OVERLAPPEDWINDOW, w as f32, h as f32, self.dpi())
    }

    /// Full screen: a big centred, decorated, resizable window at 80% of the screen; off, the card's own size.
    fn set_fullscreen(&mut self, on: bool) {
        if on == self.env.shell.fullscreen {
            return;
        }
        self.env.shell.fullscreen = on;
        let dpi = self.dpi();
        if on {
            let mut r = RECT::default();
            // SAFETY: a live out rect
            let _ = unsafe { GetWindowRect(self.hwnd, &mut r) };
            self.normal_at = Some((r.left, r.top));
            let mon = self.monitor();
            let s = mon.scale();
            let (cw, ch) = crate::place::game_window(mon.area.width / s, mon.area.height / s, Some((f64::from(m::PANEL_W), 392.0)));
            let (w, h) = outer(self.hwnd, WS_OVERLAPPEDWINDOW, cw as f32, ch as f32, dpi);
            let x = (mon.area.x + (mon.area.width - f64::from(w)) / 2.0) as i32;
            let y = (mon.area.y + (mon.area.height - f64::from(h)) / 2.0) as i32;
            push(Op::Style(self.hwnd, WS_OVERLAPPEDWINDOW));
            push(Op::Pos(self.hwnd, x, y, w, h, true));
        } else {
            let (x, y) = self.normal_at.unwrap_or((100, 100));
            let (w, h) = outer(self.hwnd, NORMAL_STYLE, m::PANEL_W, self.content_h, dpi);
            push(Op::Style(self.hwnd, NORMAL_STYLE));
            push(Op::Pos(self.hwnd, x, y, w, h, true));
        }
        // SAFETY: our window's title stays the game's
        let _ = unsafe { SetWindowTextW(self.hwnd, &HSTRING::from(self.lobby.def.title)) };
    }

    /// After every message: the game's requests, the link's news, the timers, the clipboard, and a repaint.
    fn settle(&mut self, repaint: bool) {
        for _ in 0..32 {
            let sent = std::mem::take(&mut self.env.sent);
            if !sent.is_empty()
                && let Some(r) = &mut self.env.relay
            {
                for s in &sent {
                    r.peer.send(s);
                }
            }
            if let Some(note) = self.env.end.take() {
                self.lobby.ended(note, &mut self.env);
            }
            if std::mem::take(&mut self.env.escape) {
                self.escape();
            }
            if std::mem::take(&mut self.env.toggle_fullscreen) {
                let on = !self.env.shell.fullscreen;
                self.set_fullscreen(on);
            }
            let events = self.env.relay.as_mut().map(Relay::pump).unwrap_or_default();
            for e in &events {
                self.lobby.heard(e, &mut self.env);
            }
            if events.is_empty() && self.env.sent.is_empty() && self.env.end.is_none() && !self.env.escape {
                break;
            }
        }
        let hwnd = self.hwnd;
        // the link's next deadline
        let wall = self.env.wall();
        match self.env.relay.as_ref().and_then(|r| r.peer.next_deadline()) {
            Some(at) => {
                let wait = (at - wall).clamp(0.0, 600_000.0) as u32 + 1;
                // SAFETY: our window's timer
                unsafe { SetTimer(Some(hwnd), TIMER_RELAY, wait, None) };
            }
            None => {
                // SAFETY: as above
                let _ = unsafe { KillTimer(Some(hwnd), TIMER_RELAY) };
            }
        }
        // the game's 200 ms tick, only while it wants one
        let ticks = self.lobby.play.as_ref().is_some_and(|p| p.ticks());
        if ticks != self.ticking {
            self.ticking = ticks;
            // SAFETY: as above
            unsafe {
                if ticks {
                    SetTimer(Some(hwnd), TIMER_TICK, 200, None);
                } else {
                    let _ = KillTimer(Some(hwnd), TIMER_TICK);
                }
            }
        }
        if let Some(text) = self.env.clipboard.take() {
            crate::clip::set(hwnd, &text);
        }
        if repaint {
            // SAFETY: our window; WM_PAINT comes when the queue is empty
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        }
    }

    /// One frame: the card (or full screen's stage), the lobby or the game, and the shell's layers.
    fn paint(&mut self) {
        // the target follows the client area (a WM_SIZE sent while the window was being restyled can go by)
        let mut cr = RECT::default();
        // SAFETY: a live out rect for our window
        if unsafe { GetClientRect(self.hwnd, &mut cr) }.is_ok() {
            let (w, h) = (cr.right.max(1) as u32, cr.bottom.max(1) as u32);
            if (w, h) != self.gfx.px && unsafe { IsWindowVisible(self.hwnd).as_bool() } {
                self.gfx.resize(w, h);
            }
        }
        let t = self.t0.elapsed().as_secs_f64();
        let fs = self.env.shell.fullscreen;
        let content_h = self.content_h;
        let colors = self.colors;
        let mouse = self.mouse;
        let Win { gfx, lobby, env, .. } = self;
        let mut new_h = content_h;
        let frame = gfx.draw(colors, mouse, t, |ui| {
            let c = ui.c;
            ui.clear(c.bg);
            let (area_x, area_w, top) = if fs {
                let l = fullscreen_layout(Size { w: f64::from(ui.w), h: f64::from(ui.h) }, &FullscreenLayoutOptions::new(Size { w: 640.0, h: 360.0 }));
                let pad = 16.0 * l.ui as f32;
                let game_h = (ui.h - 2.0 * pad).max(0.0);
                let w = (ui.w - 2.0 * pad).min(game_h * 1.8).max(0.0);
                env.stage = Some(Stage { game_h, width: w });
                ((ui.w - w) / 2.0, w, ((ui.h - content_h) / 2.0).max(pad))
            } else {
                env.stage = None;
                (0.0, ui.w, 0.0)
            };
            let x = area_x + m::GUTTER;
            let width = area_w - 2.0 * m::GUTTER;
            let h = lobby.paint(ui, x, top + m::SP3, width, env);
            new_h = h + 2.0 * m::SP3;
            let area = Rect::new(area_x, top, area_w, new_h);
            // a game on this computer gets the full-screen button in the corner
            if env.shell.mode == ShellMode::Local {
                widgets::fullscreen_button(ui, area.right() - 6.0 - 24.0, area.bottom() - 6.0 - 22.0, fs, true);
            }
            if env.shell.paused {
                paused_card(ui, if fs { Rect::new(0.0, 0.0, ui.w, ui.h) } else { area }, fs);
            }
            if env.shell.armed_at.is_some() {
                widgets::toast(ui, area.x + area.w / 2.0, area.y + m::SP3, "esc again to leave");
            }
        });
        self.last_frame = Instant::now();
        match frame {
            Ok(f) => {
                self.hits = f.hits;
                self.animating = f.next;
            }
            Err(e) => {
                qlog!("games: paint failed: {e}");
                self.animating = false;
            }
        }
        if (new_h - self.content_h).abs() > 0.5 {
            self.content_h = new_h;
            if !fs && self.normal_at.is_some() {
                // the card's height moved: the window follows, top-left still
                let mut r = RECT::default();
                // SAFETY: a live out rect
                let _ = unsafe { GetWindowRect(self.hwnd, &mut r) };
                let mon = self.monitor();
                let max_h = (mon.area.height / mon.scale()) as f32 - 40.0;
                let (w, h) = outer(self.hwnd, NORMAL_STYLE, m::PANEL_W, new_h.min(max_h), self.dpi());
                push(Op::Pos(self.hwnd, r.left, r.top, w, h, false));
            } else {
                self.animating = true;
            }
        }
    }

    /// Something moves: the next frame once this one is on screen (DWM's pace), only while the window shows and
    /// isn't paused. No frames, no timer: an idle game window costs nothing.
    fn next_frame(&mut self) {
        if !std::mem::take(&mut self.animating) || self.env.shell.paused {
            return;
        }
        // SAFETY: plain queries on our window
        let shown = unsafe { IsWindowVisible(self.hwnd).as_bool() && !IsIconic(self.hwnd).as_bool() };
        if !shown {
            return;
        }
        let start = Instant::now();
        // SAFETY: waits for the next composition
        let _ = unsafe { DwmFlush() };
        // DWM had nothing to compose (an occluded window): don't spin
        if start.elapsed().as_millis() < 2 && self.last_frame.elapsed().as_millis() < 8 {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        // SAFETY: our window
        let _ = unsafe { InvalidateRect(Some(self.hwnd), None, false) };
    }

    fn teardown(&mut self) {
        self.closed = true;
        if let Some(mut p) = self.lobby.play.take() {
            p.closing(&mut self.env);
        }
        if let Some(r) = &mut self.env.relay {
            r.shut();
        }
        // SAFETY: our window's timers
        unsafe {
            for t in [TIMER_TICK, TIMER_ARM, TIMER_RELAY, TIMER_FRAME] {
                let _ = KillTimer(Some(self.hwnd), t);
            }
        }
        push(Op::Drop);
    }
}

/// The paused overlay: any key carries on, esc leaves to the lobby.
fn paused_card(ui: &mut Ui, area: Rect, fs: bool) {
    let c = ui.c;
    ui.fill(area, c.bg.mix(0.7, theme::Rgba::CLEAR));
    ui.hit(area, Hit::Resume);
    let title = gfx::Font::bold(m::FS_DISPLAY);
    let (tw, th) = ui.measure("paused", title);
    let sub = "any key carries on · esc leaves to the lobby";
    let (sw, sh) = ui.measure(sub, gfx::Font::SMALL);
    let label = qcalc_core::games::fullscreen::fullscreen_label(fs, true);
    let (b1, bh) = widgets::button_size(ui, "resume", Some("↵"), false);
    let (b2, _) = widgets::button_size(ui, label, Some("F"), false);
    let bw = b1 + m::SP3 + b2;
    let w = tw.max(sw).max(bw) + 2.0 * m::SP5;
    let h = m::SP4 + th + m::SP2 + sh + m::SP2 + m::SP2 + bh + m::SP4;
    let r = Rect::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h);
    ui.card_shadow(r, m::RADIUS);
    ui.round(r, m::RADIUS, c.bg);
    let cx = r.x + w / 2.0;
    let mut y = r.y + m::SP4;
    ui.text_center("paused", title, cx, y, c.ink);
    y += th + m::SP2;
    ui.text_center(sub, gfx::Font::SMALL, cx, y, c.muted);
    y += sh + 2.0 * m::SP2;
    let x0 = cx - bw / 2.0;
    widgets::button(ui, Rect::new(x0, y, b1, bh), "resume", Some("↵"), false, false, Hit::Resume);
    widgets::button(ui, Rect::new(x0 + b1 + m::SP3, y, b2, bh), label, Some("F"), false, false, Hit::Fullscreen);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_names_each_game_as_the_web_app_does() {
        for (line, kind) in [
            ("pong", "pong"),
            ("  Pong ", "pong"),
            ("connect 4", "connect4"),
            ("connect-four", "connect4"),
            ("Connect4", "connect4"),
            ("ultimate", "ultimate"),
            ("ultimate tic-tac-toe", "ultimate"),
            ("ultimate ttt", "ultimate"),
            ("uttt", "ultimate"),
            ("crack", "crack"),
            ("crack the safe", "crack"),
            ("safecracker", "crack"),
            ("safe cracking", "crack"),
            ("equal", "equal"),
            ("nerdle", "equal"),
            ("24 game", "numbers"),
            ("24game", "numbers"),
            ("game of 24", "numbers"),
            ("countdown", "numbers"),
            ("count down", "numbers"),
            ("numbers game", "numbers"),
            ("24 / countdown", "numbers"),
            ("24/countdown", "numbers"),
        ] {
            assert_eq!(command(line), Some(kind), "{line}");
        }
        for line in ["", "pong 2", "connect", "24", "crack 2", "ultimate tac", "games", "2+2"] {
            assert_eq!(command(line), None, "{line}");
        }
        assert_eq!(hint("crack").as_deref(), Some("↵ crack the safe"));
        assert_eq!(hint("countdown").as_deref(), Some("↵ play 24 or countdown"));
        assert_eq!(hint("connect 4").as_deref(), Some("↵ play connect 4"));
        assert_eq!(list().len(), 6);
    }
}
