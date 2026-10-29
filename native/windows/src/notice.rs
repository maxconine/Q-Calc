//! The note in the corner when a message, a request or a file comes while the bar is away (it replaced the tray
//! balloon). Raw text on the bar's own colours: the sender in the strong ink, then their words (at most three
//! lines), "sent you notes.pdf (4.2 MB)", or "wants to message you". Opaque, no fade, rounded and shadowed by DWM
//! as the bar is, in the corner of the work area of the monitor the bar was last on (so a taskbar on any edge is
//! kept clear), sized for that monitor's DPI.
//!
//! - **It never takes the keys.** A topmost tool window with WS_EX_NOACTIVATE, shown with SW_SHOWNOACTIVATE and
//!   answering WM_MOUSEACTIVATE with MA_NOACTIVATE: whatever is in front keeps the focus and the caret, even while
//!   it's clicked. No taskbar button, not in Alt+Tab.
//! - **Several** stack newest on top, at most three; a newer one from the same name takes the place of theirs.
//!   Each goes 6 s after it came, none while the pointer rests on them (3 s more once it leaves). A click opens
//!   the bar on that person's thread (a request's card for someone new); × (shown on hover) or a right-click lets
//!   one go. The bar showing for any reason lets them all go: what they said is in the bar now.
//! - **Quiet.** None while Windows' Do Not Disturb is on (`quiet`), unless Settings › Messages › Notifications says
//!   "Even in Do Not Disturb"; never over a full-screen app, a game or a presentation, or while the pc is locked.
//!   Nothing is lost: the message waits in the inbox for the bar either way (messaging.rs keeps it before asking).
//! - **Cost.** Nothing until a notice shows: the window, its render target and its bold font are made then and
//!   all go with the last notice. The quiet check reads two words of state when a notice comes, never otherwise.
//!
//! Do Not Disturb has no documented API on Windows 11. `SHQueryUserNotificationState` reports full screen,
//! presentations and Windows 10's quiet hours (QUNS_QUIET_TIME), but not reliably Windows 11's toggle, so the
//! active Focus Assist / Do Not Disturb profile is read too: the WNF state `WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED`
//! through ntdll's `NtQueryWnfStateData` (0 off, 1 priority only, 2 alarms only), as other apps that honour it do,
//! and Windows' own "Notifications" switch (`NOC_GLOBAL_SETTING_TOASTS_ENABLED` 0) counts as quiet as well.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_WRAP, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAK, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, UpdateWindow, ValidateRect};
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_TOPMOST, IDC_ARROW, LoadCursorW,
    MA_NOACTIVATE, RegisterClassExW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SetWindowPos, ShowWindow,
    WINDOW_EX_STYLE, WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE,
    WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SIZE, WNDCLASSEXW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, w};

use crate::app::App;
use crate::render::{Canvas, Gfx, Rect, Surface, Theme, measure, size};
use crate::{later, qlog};

const CLASS: windows::core::PCWSTR = w!("QCalcNotice");
/// DIPs
const WIDTH: f32 = 340.0;
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 12.0;
const HEAD: f32 = 18.0;
const GAP: f32 = 3.0;
const MAX_LINES: u32 = 3;
/// the corner's distance from the work area's edges
const MARGIN: f32 = 12.0;
const CLOSE: f32 = 20.0;
const MAX: usize = 3;
const LIFE: Duration = Duration::from_secs(6);
const AFTER_LEAVE: Duration = Duration::from_secs(3);

// ---- what a notice says ----

/// What came.
#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// a message's words
    Message(String),
    /// someone new's first message
    Ask,
    /// a clip, a file or a note about one (share.rs's card text)
    Share(String),
    /// someone new shared something
    AskShare,
}

/// The two lines of a notice: muted words beside the name, and the body under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Words {
    pub beside: String,
    pub body: String,
}

/// What a notice from someone says, with their words or without (settings' "Show message text" off).
pub fn words(what: &What, text_shown: bool) -> Words {
    let w = |beside: &str, body: &str| Words {
        beside: beside.into(),
        body: body.into(),
    };
    match what {
        What::Ask => w("wants to message you", ""),
        What::AskShare => w("wants to share with you", ""),
        What::Message(_) | What::Share(_) if !text_shown => w("sent you a message", ""),
        What::Message(t) | What::Share(t) => w("", t),
    }
}

// ---- quiet: Do Not Disturb, full screen ----

/// Why a notice isn't shown now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quiet {
    /// Windows' Do Not Disturb or Focus (quiet hours on Windows 10)
    DoNotDisturb,
    /// Windows' own Notifications switch is off
    NotificationsOff,
    /// a full-screen app or game, or a Store app full screen
    FullScreen,
    Presentation,
    /// locked, or the screen saver
    Away,
}

impl Quiet {
    /// Whether "Even in Do Not Disturb" lets a notice through anyway (never over full screen or a presentation).
    pub fn overridable(self) -> bool {
        matches!(self, Quiet::DoNotDisturb | Quiet::NotificationsOff)
    }

    pub fn name(self) -> &'static str {
        match self {
            Quiet::DoNotDisturb => "do not disturb",
            Quiet::NotificationsOff => "notifications off in windows",
            Quiet::FullScreen => "full screen",
            Quiet::Presentation => "presentation",
            Quiet::Away => "away",
        }
    }
}

/// The quiet reason from what Windows says: `quns` is SHQueryUserNotificationState's answer, `profile` the active
/// quiet-hours profile (WNF; 0 off, 1 priority only, 2 alarms only), `toasts_off` Windows' Notifications switch.
/// What the person is doing (full screen, presenting, away) comes first, as no setting lets a notice over it.
pub fn quiet_of(quns: Option<i32>, profile: Option<u32>, toasts_off: bool) -> Option<Quiet> {
    // QUNS_NOT_PRESENT 1, QUNS_BUSY 2, QUNS_RUNNING_D3D_FULL_SCREEN 3, QUNS_PRESENTATION_MODE 4,
    // QUNS_ACCEPTS_NOTIFICATIONS 5, QUNS_QUIET_TIME 6, QUNS_APP 7
    match quns {
        Some(1) => return Some(Quiet::Away),
        Some(2 | 3 | 7) => return Some(Quiet::FullScreen),
        Some(4) => return Some(Quiet::Presentation),
        Some(6) => return Some(Quiet::DoNotDisturb),
        _ => {}
    }
    if profile.is_some_and(|p| p != 0) {
        return Some(Quiet::DoNotDisturb);
    }
    toasts_off.then_some(Quiet::NotificationsOff)
}

/// The active profile's WNF state: its first four bytes, little-endian, when the read gave at least that many.
pub fn profile_of(status: i32, data: &[u8], size: u32) -> Option<u32> {
    if status != 0 || size < 4 || data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// The bench pipe's stand-in for Windows' state (share-e2e.ps1): 0 real, 1 Do Not Disturb on, 2 nothing quiet.
static FAKE: AtomicU8 = AtomicU8::new(0);

pub fn fake_quiet(on: Option<bool>) {
    FAKE.store(
        match on {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        Ordering::Relaxed,
    );
}

/// WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED
const WNF_QUIET_PROFILE: u64 = 0x0D83_063E_A3BF_1C75;

/// The active quiet-hours (Do Not Disturb) profile, from ntdll's NtQueryWnfStateData (looked up, not linked:
/// it is undocumented, and an ntdll without it just means "unknown").
fn quiet_profile() -> Option<u32> {
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    type Query = unsafe extern "system" fn(
        *const u64,
        *const core::ffi::c_void,
        *const core::ffi::c_void,
        *mut u32,
        *mut core::ffi::c_void,
        *mut u32,
    ) -> i32;
    // SAFETY: ntdll is in every process; the name is a static C string
    let f = unsafe {
        let ntdll = GetModuleHandleW(w!("ntdll.dll")).ok()?;
        GetProcAddress(ntdll, windows::core::s!("NtQueryWnfStateData"))?
    };
    // SAFETY: the documented-by-use signature (state name, type, scope, stamp, buffer, size)
    let query: Query = unsafe { std::mem::transmute(f) };
    let name = WNF_QUIET_PROFILE;
    let mut stamp = 0u32;
    let mut buf = [0u8; 16];
    let mut size = buf.len() as u32;
    // SAFETY: live locals; `size` says how much of `buf` may be written
    let status = unsafe {
        query(
            &name,
            std::ptr::null(),
            std::ptr::null(),
            &mut stamp,
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    profile_of(status, &buf, size)
}

fn toasts_off() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let mut on: u32 = 1;
    let mut size = 4u32;
    // SAFETY: live out pointers of the right size
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings"),
            w!("NOC_GLOBAL_SETTING_TOASTS_ENABLED"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut on as *mut u32).cast()),
            Some(&mut size),
        )
    };
    status.is_ok() && on == 0
}

/// Whether Windows wants quiet now, and why (the bench pipe's fake first).
pub fn quiet() -> Option<Quiet> {
    match FAKE.load(Ordering::Relaxed) {
        1 => return Some(Quiet::DoNotDisturb),
        2 => return None,
        _ => {}
    }
    // SAFETY: plain query
    let quns = unsafe { windows::Win32::UI::Shell::SHQueryUserNotificationState() }
        .ok()
        .map(|s| s.0);
    quiet_of(quns, quiet_profile(), toasts_off())
}

/// What Windows says now, raw, for the pipe (how the detection was checked on a real machine).
fn quiet_raw() -> Value {
    // SAFETY: plain query
    let quns = unsafe { windows::Win32::UI::Shell::SHQueryUserNotificationState() }
        .ok()
        .map(|s| s.0);
    json!({ "quns": quns, "profile": quiet_profile(), "toastsOff": toasts_off() })
}

// ---- the notices ----

struct Note {
    from: String,
    what: What,
    words: Words,
    until: Instant,
}

/// The window while notices show.
struct Win {
    hwnd: HWND,
    surface: Surface,
    /// physical px on screen
    rect: RECT,
    bold: Option<IDWriteTextFormat>,
    tracking: bool,
}

#[derive(Default)]
struct State {
    notes: Vec<Note>,
    win: Option<Win>,
    /// the row under the pointer, and whether on its ×
    hover: Option<(usize, bool)>,
    pressed: Option<usize>,
    /// suppressed since launch, and the last reason (the pipe's)
    quieted: u32,
    last_quiet: Option<&'static str>,
}

thread_local! {
    static N: RefCell<State> = RefCell::new(State::default());
}

static CLASS_DONE: AtomicU32 = AtomicU32::new(0);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    N.with(|s| s.try_borrow_mut().ok().map(|mut s| f(&mut s)))
}

/// Something came from `from` while the bar is away (messaging.rs): a notice, unless settings or Windows say not.
pub fn post(app: &App, from: &str, what: What) {
    let set = &app.store.settings;
    if !set.notifications {
        return;
    }
    if let Some(q) = quiet()
        && !(q.overridable() && set.notifications_in_dnd)
    {
        qlog!("notice: quiet ({}), {from}'s waits for the bar", q.name());
        with(|s| {
            s.quieted += 1;
            s.last_quiet = Some(q.name());
        });
        return;
    }
    let words = words(&what, set.notification_text);
    let until = Instant::now() + LIFE;
    with(|s| {
        // a newer one from the same name takes the place of theirs (a request stays a request)
        s.notes
            .retain(|n| n.from != from || matches!(n.what, What::Ask | What::AskShare));
        if !matches!(what, What::Ask | What::AskShare)
            || !s.notes.iter().any(|n| n.from == from && n.what == what)
        {
            s.notes.insert(
                0,
                Note {
                    from: from.to_string(),
                    what,
                    words,
                    until,
                },
            );
        }
        s.notes.truncate(MAX);
        s.hover = None;
    });
    later::free("notice", 0, sync);
}

/// The bar is showing: what the notices said is in it now.
pub fn clear() {
    let had = with(|s| {
        let had = !s.notes.is_empty();
        s.notes.clear();
        had
    });
    if had == Some(true) {
        later::free("notice", 0, sync);
    }
}

/// The window made, moved, sized and painted for the notices as they are, or gone with the last; outside every
/// borrow (window calls send messages back).
fn sync() {
    later::cancel("notice-expire");
    let empty = with(|s| s.notes.is_empty()).unwrap_or(true);
    if empty {
        let win = with(|s| {
            s.hover = None;
            s.pressed = None;
            s.win.take()
        })
        .flatten();
        if let Some(w) = win {
            // SAFETY: our window; its render target and font go with `w`
            let _ = unsafe { DestroyWindow(w.hwnd) };
            drop(w);
            qlog!("notice: gone");
        }
        return;
    }
    // where and how big, for the bar's monitor and the notices' text
    let Some(Some((x, y, wpx, hpx, dpi, dark))) = crate::app::with_app(|a| {
        let f = a.bar.frame();
        let monitor = crate::window::monitor_at(POINT {
            x: (f.left + f.right) / 2,
            y: (f.top + f.bottom) / 2,
        });
        let scale = monitor.scale() as f32;
        N.with(|s| {
            let mut s = s.try_borrow_mut().ok()?;
            let bold = bold_format(&a.gfx, &mut s);
            let h = rows(&a.gfx, &s.notes, bold.as_ref())
                .last()
                .map_or(0.0, |r| r.top + r.height);
            let (wpx, hpx) = ((WIDTH * scale).round() as i32, (h * scale).round() as i32);
            let m = (MARGIN * scale).round() as i32;
            let area = monitor.area;
            let x = (area.x + area.width) as i32 - m - wpx;
            let y = (area.y + area.height) as i32 - m - hpx;
            Some((x, y, wpx, hpx, monitor.dpi, a.theme.dark))
        })
    }) else {
        return;
    };
    let had = with(|s| s.win.as_ref().map(|w| w.hwnd)).flatten();
    let hwnd = match had {
        Some(h) => h,
        None => match create(x, y, wpx, hpx, dpi) {
            Some(h) => h,
            None => return,
        },
    };
    set_dark(hwnd, dark);
    with(|s| {
        if let Some(w) = &mut s.win {
            w.surface.set_dpi(dpi);
            w.rect = RECT {
                left: x,
                top: y,
                right: x + wpx,
                bottom: y + hpx,
            };
        }
    });
    // SAFETY: our window; WM_SIZE comes back with nothing borrowed
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, wpx, hpx, SWP_NOACTIVATE);
    }
    if had.is_none() {
        // cloaked while it is shown and painted, so no blank frame flashes (as the bar shows)
        cloak(hwnd, true);
        // SAFETY: our window
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(Some(hwnd), None, false);
            let _ = UpdateWindow(hwnd);
        }
        cloak(hwnd, false);
        qlog!("notice: shown");
    } else {
        // SAFETY: our window
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
            let _ = UpdateWindow(hwnd);
        }
    }
    arm();
}

/// The next expiry, unless the pointer rests on them.
fn arm() {
    let next = with(|s| {
        if s.win.as_ref().is_some_and(|w| w.tracking) {
            return None;
        }
        s.notes.iter().map(|n| n.until).min()
    })
    .flatten();
    if let Some(at) = next {
        let ms = at.saturating_duration_since(Instant::now()).as_millis() as u32;
        later::free("notice-expire", ms.max(1), expire);
    }
}

fn expire() {
    let now = Instant::now();
    with(|s| {
        if s.win.as_ref().is_some_and(|w| w.tracking) {
            return;
        }
        s.notes.retain(|n| n.until > now);
        s.hover = None;
    });
    sync();
}

fn create(x: i32, y: i32, w: i32, h: i32, dpi: u32) -> Option<HWND> {
    // SAFETY: a class with our wndproc, and a window of it with static strings
    let hwnd = unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
        if CLASS_DONE.swap(1, Ordering::Relaxed) == 0 {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassExW(&class);
        }
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0),
            CLASS,
            w!("Q Calc notice"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?
    };
    // SAFETY: attributes on our own window, with pointers to live locals (as window.rs does for the bar)
    unsafe {
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&round as *const windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of_val(&round) as u32,
        );
        // a 1 px frame inset gives a borderless window DWM's shadow
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 0,
            cyBottomHeight: 1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
    let mut surface = Surface::new(hwnd, dpi);
    surface.resize(w.max(1) as u32, h.max(1) as u32);
    with(|s| {
        s.win = Some(Win {
            hwnd,
            surface,
            rect: RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            bold: None,
            tracking: false,
        });
    });
    Some(hwnd)
}

fn set_dark(hwnd: HWND, dark: bool) {
    let v = BOOL::from(dark);
    // SAFETY: as in create
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

fn cloak(hwnd: HWND, on: bool) {
    let v = BOOL::from(on);
    // SAFETY: as in create
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK,
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

/// The sender's semibold face, made with the window and gone with it.
fn bold_format(gfx: &Gfx, s: &mut State) -> Option<IDWriteTextFormat> {
    let win = s.win.as_mut();
    if let Some(f) = win.as_ref().and_then(|w| w.bold.clone()) {
        return Some(f);
    }
    let f = crate::render::format(
        &gfx.dw,
        &gfx.fonts.text_family,
        size::BODY,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
    )
    .ok();
    if let Some(w) = win {
        w.bold = f.clone();
    }
    f
}

// ---- layout and paint ----

struct Row {
    top: f32,
    height: f32,
    name: Option<IDWriteTextLayout>,
    beside: Option<IDWriteTextLayout>,
    body: Option<(IDWriteTextLayout, f32)>,
}

fn row_rect(r: &Row) -> Rect {
    Rect::new(0.0, r.top, WIDTH, r.height)
}

fn close_rect(r: &Row) -> Rect {
    Rect::new(
        WIDTH - PAD_X - CLOSE + 6.0,
        r.top + PAD_Y + (HEAD - CLOSE) / 2.0,
        CLOSE,
        CLOSE,
    )
}

/// The body wrapped in the inner width, cut with an ellipsis past MAX_LINES; its height.
fn body_layout(gfx: &Gfx, text: &str, w: f32) -> Option<(IDWriteTextLayout, f32)> {
    let l = gfx.layout_in(
        text,
        &gfx.fonts.body,
        w,
        1000.0,
        DWRITE_TEXT_ALIGNMENT_LEADING,
    )?;
    let mut m = DWRITE_TEXT_METRICS::default();
    // SAFETY: setters and a query on a live layout; out pointer is a live local
    unsafe {
        let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
        let _ = l.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
        l.GetMetrics(&mut m).ok()?;
    }
    let lines = m.lineCount.max(1);
    let line = m.height / lines as f32;
    let h = line * lines.min(MAX_LINES) as f32;
    // SAFETY: a live layout; trimming then cuts at the last whole line with an ellipsis
    unsafe {
        let _ = l.SetMaxHeight(h + 0.5);
    }
    Some((l, h))
}

fn rows(gfx: &Gfx, notes: &[Note], bold: Option<&IDWriteTextFormat>) -> Vec<Row> {
    let inner = WIDTH - 2.0 * PAD_X;
    let mut y = 0.0;
    let mut out = Vec::new();
    for (i, n) in notes.iter().enumerate() {
        if i > 0 {
            y += 1.0;
        }
        let name = bold.and_then(|f| {
            gfx.layout_in(
                &n.from,
                f,
                inner - CLOSE,
                HEAD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )
        });
        let name_w = name
            .as_ref()
            .map_or(0.0, |l| measure(l).0.min(inner - CLOSE));
        let beside = (!n.words.beside.is_empty())
            .then(|| {
                gfx.layout_in(
                    &n.words.beside,
                    &gfx.fonts.body,
                    (inner - CLOSE - name_w - 6.0).max(1.0),
                    HEAD,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                )
            })
            .flatten();
        let body = (!n.words.body.is_empty())
            .then(|| body_layout(gfx, &n.words.body, inner))
            .flatten();
        let height = PAD_Y + HEAD + body.as_ref().map_or(0.0, |b| GAP + b.1) + PAD_Y;
        out.push(Row {
            top: y,
            height,
            name,
            beside,
            body,
        });
        y += height;
    }
    out
}

fn paint(c: &Canvas, rows: &[Row], hover: Option<(usize, bool)>) {
    let t: &Theme = c.theme;
    c.clear(t.bg);
    for (i, r) in rows.iter().enumerate() {
        let rr = row_rect(r);
        if i > 0 {
            c.fill(Rect::new(0.0, r.top - 1.0, WIDTH, 1.0), t.line);
        }
        let hovered = hover.filter(|h| h.0 == i);
        if hovered.is_some() {
            c.fill(rr, t.fill_hover);
        }
        let head_y = r.top + PAD_Y;
        let mut x = PAD_X;
        if let Some(l) = &r.name {
            c.text(l, x, head_y, t.ink);
            x += measure(l).0.min(WIDTH - 2.0 * PAD_X - CLOSE) + 6.0;
        }
        if let Some(l) = &r.beside {
            c.text(l, x, head_y, t.muted);
        }
        if let Some((l, h)) = &r.body {
            let by = head_y + HEAD + GAP;
            c.push_clip(Rect::new(PAD_X, by, WIDTH - 2.0 * PAD_X, *h + 1.0));
            c.text(l, PAD_X, by, t.answer);
            c.pop_clip();
        }
        if let Some((_, on_close)) = hovered {
            let cr = close_rect(r);
            if on_close {
                c.fill_round(cr, size::RADIUS_S, t.fill_selected);
            }
            if let Some(l) = c.gfx.layout_in(
                "×",
                &c.gfx.fonts.body,
                cr.w,
                cr.h,
                windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_CENTER,
            ) {
                c.text(&l, cr.x, cr.y, if on_close { t.answer } else { t.muted });
            }
        }
    }
}

/// WM_PAINT: one frame, with the app's fonts and theme.
fn on_paint() {
    crate::app::with_app(|a| {
        N.with(|s| {
            let Ok(mut s) = s.try_borrow_mut() else {
                return;
            };
            let bold = bold_format(&a.gfx, &mut s);
            let rows = rows(&a.gfx, &s.notes, bold.as_ref());
            let hover = s.hover;
            if let Some(w) = &mut s.win
                && let Err(e) = w.surface.draw(&a.gfx, &a.theme, |c| paint(c, &rows, hover))
            {
                qlog!("notice: paint failed ({e})");
            }
        })
    });
}

/// The row (and whether its ×) at a client point in physical px.
fn at(lp: LPARAM) -> Option<(usize, bool)> {
    let (px, py) = (
        f32::from((lp.0 & 0xffff) as u16 as i16),
        f32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    );
    crate::app::with_app(|a| {
        N.with(|s| {
            let mut s = s.try_borrow_mut().ok()?;
            let scale = s.win.as_ref()?.surface.scale();
            let (x, y) = (px / scale, py / scale);
            let bold = bold_format(&a.gfx, &mut s);
            rows(&a.gfx, &s.notes, bold.as_ref())
                .iter()
                .enumerate()
                .find(|(_, r)| row_rect(r).contains(x, y))
                .map(|(i, r)| (i, close_rect(r).inset(-4.0, -4.0).contains(x, y)))
        })
    })
    .flatten()
}

/// Lets one go.
fn dismiss(i: usize) {
    with(|s| {
        if i < s.notes.len() {
            s.notes.remove(i);
        }
        s.hover = None;
    });
    later::free("notice", 0, sync);
}

/// Opens the bar on their thread (a request's card for someone new); every notice goes, as the bar has them now.
fn open(i: usize) {
    let Some(Some((from, ask))) = with(|s| {
        s.notes
            .get(i)
            .map(|n| (n.from.clone(), matches!(n.what, What::Ask | What::AskShare)))
    }) else {
        return;
    };
    qlog!("notice: opened {from}'s");
    with(|s| s.notes.clear());
    later::free("notice", 0, sync);
    crate::app::poke(move |app| {
        if !ask {
            crate::threads::open(app, &from);
        }
        app.op(crate::app::Op::Show);
    });
}

unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // a click never activates it: whatever is in front keeps the keys
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_PAINT => {
            on_paint();
            // SAFETY: our window
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            let (w, h) = ((lp.0 & 0xffff) as u32, ((lp.0 >> 16) & 0xffff) as u32);
            with(|s| {
                if let Some(win) = &mut s.win {
                    win.surface.resize(w, h);
                }
            });
            LRESULT(0)
        }
        // it is placed for its monitor by `sync`
        WM_DPICHANGED => LRESULT(0),
        WM_MOUSEMOVE => {
            let now = at(lp);
            let start = with(|s| {
                let changed = s.hover != now;
                s.hover = now;
                let start = s
                    .win
                    .as_mut()
                    .is_some_and(|w| !std::mem::replace(&mut w.tracking, true));
                (changed, start)
            });
            if let Some((changed, start)) = start {
                if start {
                    later::cancel("notice-expire");
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: a live struct for our window
                    let _ = unsafe { TrackMouseEvent(&mut t) };
                }
                if changed {
                    // SAFETY: our window
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let floor = Instant::now() + AFTER_LEAVE;
            with(|s| {
                s.hover = None;
                s.pressed = None;
                if let Some(w) = &mut s.win {
                    w.tracking = false;
                }
                for n in &mut s.notes {
                    n.until = n.until.max(floor);
                }
            });
            // SAFETY: our window
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            arm();
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let i = at(lp).map(|h| h.0);
            with(|s| s.pressed = i);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let now = at(lp);
            let pressed = with(|s| s.pressed.take()).flatten();
            match now {
                Some((i, true)) if pressed == Some(i) => dismiss(i),
                Some((i, false)) if pressed == Some(i) => open(i),
                _ => {}
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let pressed = with(|s| s.pressed.take()).flatten();
            if let Some((i, _)) = at(lp)
                && pressed == Some(i)
            {
                dismiss(i);
            }
            LRESULT(0)
        }
        // SAFETY: the default for our own window
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

// ---- settings › messages › notifications ----

/// The Notifications rows on the Messages page (messaging.rs adds them).
pub fn settings_items(app: &App) -> Vec<crate::settings_window::Item> {
    use crate::settings_window::{Control, Item, row};
    let s = &app.store.settings;
    let mut out = vec![
        Item::Heading("Notifications".into()),
        row(
            "notifications",
            "Show notifications",
            "A note in the corner when a message or file comes while the bar is closed. It never takes the keys.",
            Control::Toggle(s.notifications),
        ),
    ];
    if s.notifications {
        out.push(row(
            "notificationText",
            "Show message text",
            "Off, a note only says who sent something.",
            Control::Toggle(s.notification_text),
        ));
        out.push(row(
            "notificationsInDnd",
            "Even in Do Not Disturb",
            "Windows' Do Not Disturb hides them unless this is on. Full-screen apps and presentations always do.",
            Control::Toggle(s.notifications_in_dnd),
        ));
    }
    out
}

/// A Notifications row changed: true when it was one of these.
pub fn settings_changed(app: &mut App, id: &str, value: &Value) -> bool {
    let Some(on) = value.as_bool() else {
        return false;
    };
    let s = &mut app.store.settings;
    match id {
        "notifications" => s.notifications = on,
        "notificationText" => s.notification_text = on,
        "notificationsInDnd" => s.notifications_in_dnd = on,
        _ => return false,
    }
    if !app.store.settings.notifications {
        clear();
    }
    app.save_store();
    true
}

// ---- the bench pipe (share_debug.rs) ----

/// What shows, where, and what Windows says about quiet.
pub fn debug_state() -> Value {
    let quiet_now = quiet().map(Quiet::name);
    with(|s| {
        json!({
            "shown": s.win.is_some(),
            "hwnd": s.win.as_ref().map(|w| w.hwnd.0 as usize),
            "rect": s.win.as_ref().map(|w| [w.rect.left, w.rect.top, w.rect.right, w.rect.bottom]),
            "notes": s.notes.iter().map(|n| json!({
                "from": n.from, "beside": n.words.beside, "body": n.words.body,
            })).collect::<Vec<_>>(),
            "hover": s.hover.map(|h| h.0),
            "quiet": quiet_now,
            "quieted": s.quieted,
            "lastQuiet": s.last_quiet,
            "fake": FAKE.load(Ordering::Relaxed),
            "windows": quiet_raw(),
        })
    })
    .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_notice_says() {
        let m = What::Message("see you at 5".into());
        assert_eq!(
            words(&m, true),
            Words {
                beside: String::new(),
                body: "see you at 5".into()
            }
        );
        // without the text: only who
        assert_eq!(words(&m, false).body, "");
        assert_eq!(words(&m, false).beside, "sent you a message");
        let f = What::Share("sent you notes.pdf (4.2 MB)".into());
        assert_eq!(words(&f, true).body, "sent you notes.pdf (4.2 MB)");
        assert_eq!(words(&f, false).body, "");
        // a request says the same either way
        for shown in [true, false] {
            assert_eq!(words(&What::Ask, shown).beside, "wants to message you");
            assert_eq!(
                words(&What::AskShare, shown).beside,
                "wants to share with you"
            );
        }
    }

    #[test]
    fn quiet_from_what_windows_says() {
        // accepts notifications, no profile: shown
        assert_eq!(quiet_of(Some(5), Some(0), false), None);
        assert_eq!(quiet_of(None, None, false), None);
        // full screen, a game, a store app full screen, a presentation, away
        assert_eq!(quiet_of(Some(2), Some(0), false), Some(Quiet::FullScreen));
        assert_eq!(quiet_of(Some(3), None, false), Some(Quiet::FullScreen));
        assert_eq!(quiet_of(Some(7), None, false), Some(Quiet::FullScreen));
        assert_eq!(quiet_of(Some(4), None, false), Some(Quiet::Presentation));
        assert_eq!(quiet_of(Some(1), None, false), Some(Quiet::Away));
        // Windows 10's quiet hours, and Windows 11's Do Not Disturb read from the active profile
        assert_eq!(quiet_of(Some(6), None, false), Some(Quiet::DoNotDisturb));
        assert_eq!(quiet_of(Some(5), Some(1), false), Some(Quiet::DoNotDisturb));
        assert_eq!(quiet_of(Some(5), Some(2), false), Some(Quiet::DoNotDisturb));
        // Windows' Notifications switch off
        assert_eq!(
            quiet_of(Some(5), Some(0), true),
            Some(Quiet::NotificationsOff)
        );
        // full screen wins over Do Not Disturb, and no setting lets a notice over it
        assert_eq!(quiet_of(Some(3), Some(1), true), Some(Quiet::FullScreen));
        assert!(!Quiet::FullScreen.overridable() && !Quiet::Presentation.overridable());
        assert!(!Quiet::Away.overridable());
        assert!(Quiet::DoNotDisturb.overridable() && Quiet::NotificationsOff.overridable());
    }

    #[test]
    fn the_profile_is_the_states_first_word() {
        assert_eq!(profile_of(0, &[0, 0, 0, 0], 4), Some(0));
        assert_eq!(profile_of(0, &[1, 0, 0, 0, 9, 9], 4), Some(1));
        assert_eq!(profile_of(0, &[2, 0, 0, 0], 8), Some(2));
        // a failed read, or one too short, is unknown (not quiet)
        assert_eq!(profile_of(-1_073_741_772, &[1, 0, 0, 0], 4), None);
        assert_eq!(profile_of(0, &[1, 0], 2), None);
        assert_eq!(profile_of(0, &[0, 0, 0, 0], 0), None);
    }
}
