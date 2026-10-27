//! The reference sheets, each in its own window beside the bar: the periodic table (`periodic`) and the identity
//! sheets (`identities`, `idt <part>`, `trig`, `constants`, `conversions`, `metric prefixes`...). Spec:
//! macos/PeriodicWindow.swift, macos/IdentityWindow.swift, macos/SheetPlace.swift (place.rs `beside` /
//! `needs_moving`), native/mac/Sources/Sheets/SheetData.swift; the data is the core's (`elements::periodic_tiles`,
//! `formula_of_picks`, `identities::identity_target_for`, `identity_runs`).
//!
//! **Windows.** As the Tauri app's sheets: a small-captioned, resizable, always-on-top tool window (no taskbar
//! button) that never takes the keys (`WS_EX_NOACTIVATE`, `MA_NOACTIVATE`), so the bar keeps its caret and stays up
//! while a sheet is clicked. A sheet opens clear of the bar on the bar's monitor (below it, else right, left or
//! above), the table scaling down and an identity sheet getting shorter if it has to; with the bar away it opens
//! centred on the pointer's monitor, each further sheet a little down and right. A second open brings the same
//! window forward (at the section asked for). Ctrl+W closes the front sheet before the bar (`close_front`, for
//! keys.rs).
//!
//! **Periodic table.** A click adds an element to a formula built from clicks (H, H, O is H₂O, with its molar mass
//! and each element's share); a right click on a tile, or a click on a part, takes one away, and Clear starts over.
//! A click on the formula (or ↵ in the bar while one is built, `put_formula_in`) puts its mass in the bar and
//! starts over. Laid out at one size and scaled to the window, which keeps its shape.
//!
//! **Identity sheets.** Sections down the page, each one's groups in two columns (a wide section's at full width),
//! rows with raised and lowered runs; a row click copies it ("✓ copied" for 1.2 s). Scrolls with the wheel or the
//! thumb; only the height can change.
//!
//! **Cost.** Nothing until a sheet opens: the window class, its own D2D/DWrite factories and fonts, and the text
//! layouts are made then (layouts once per window, never per frame) and dropped when the last sheet closes.

use std::cell::RefCell;
use std::collections::HashMap;

use qcalc_core::app::elements::{self, ElementCategory, PeriodicTile};
use qcalc_core::app::identities::{self, Shift};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_METRICS, DWRITE_TEXT_RANGE,
    DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_DBLCLKS, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
    GetClientRect, GetWindowRect, HWND_TOPMOST, IDC_ARROW, IDC_HAND, IsWindow,
    LoadCursorW, MINMAXINFO, PostMessageW, RegisterClassExW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetCursor, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SIZE, WM_SIZING, WNDCLASSEXW, WS_CAPTION, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use crate::app::App;
use crate::later;
use crate::place::{self, Area, Give};
use crate::qlog;
use crate::render::{Canvas, Color, Gfx, Rect, Surface, Theme, format, rgb, with_alpha};

const CLASS: PCWSTR = w!("QCalcSheet");
const WM_MOUSELEAVE: u32 = 0x02A3;
/// how far down and right each further sheet opens from the one before (DIPs), with the bar away
const CASCADE: f64 = 28.0;

// ---- the periodic table's design (PeriodicWindow.swift), DIPs ----

const TILE_W: f32 = 44.0;
const TILE_H: f32 = 48.0;
const GAP: f32 = 3.0;
const INSET: f32 = 16.0;
/// the Mac leaves 30 for its transparent title bar; here the window has a caption of its own
const TOP: f32 = INSET;
/// between the table and the lanthanide and actinide rows
const F_GAP: f32 = 10.0;

fn cell(row: i64, col: i64) -> Rect {
    let x = INSET + (col - 1) as f32 * (TILE_W + GAP);
    let y = if row <= 7 {
        TOP + (row - 1) as f32 * (TILE_H + GAP)
    } else {
        TOP + 7.0 * (TILE_H + GAP) + F_GAP + (row - 9) as f32 * (TILE_H + GAP)
    };
    Rect::new(x, y, TILE_W, TILE_H)
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(x, y, a.right().max(b.right()) - x, a.bottom().max(b.bottom()) - y)
}

/// The table's size at scale 1.
pub fn periodic_size() -> (f32, f32) {
    let last = cell(10, 18);
    (last.right() + INSET, last.bottom() + INSET)
}

/// One symbol a click, in order: H, H, O is H₂O. The masses add in thousandths, as the core's formula does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picks(pub Vec<String>);

impl Picks {
    pub fn pick(&mut self, symbol: &str) {
        self.0.push(symbol.to_string());
    }

    /// Takes the last one of `symbol` back out; false when there was none.
    pub fn drop_one(&mut self, symbol: &str) -> bool {
        match self.0.iter().rposition(|s| s == symbol) {
            Some(i) => {
                self.0.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn count(&self, symbol: &str) -> usize {
        self.0.iter().filter(|s| *s == symbol).count()
    }

    pub fn formula(&self) -> Option<elements::Formula> {
        elements::formula_of_picks(&self.0)
    }
}

// ---- identity sheet design (IdentityWindow.swift), DIPs ----

const ID_INSET: f32 = 16.0;
const ID_TOP: f32 = 30.0;
const ID_COLUMN: f32 = 344.0;
const ID_ROW: f32 = 22.0;
const ID_GROUP_TITLE: f32 = 20.0;
const ID_SECTION_TITLE: f32 = 26.0;
const ID_MAX_HEIGHT: f32 = 620.0;
const ID_HINT: &str = "click to copy";

pub fn identity_width() -> f32 {
    ID_INSET * 3.0 + ID_COLUMN * 2.0
}

/// Where the sections, group titles and rows of a sheet go: each group in whichever column is shorter so far, a
/// wide section's groups one under another at the full width. Pure, so it's tested.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdLayout {
    /// each section's top and its title
    pub sections: Vec<(f32, String)>,
    /// group titles: (x, y, w, title)
    pub groups: Vec<(f32, f32, f32, String)>,
    /// rows: (rect, section, group, item)
    pub rows: Vec<(Rect, usize, usize, usize)>,
    pub height: f32,
}

pub fn identity_layout(sheet: &identities::IdentitySheet) -> IdLayout {
    let width = identity_width();
    let mut out = IdLayout::default();
    let mut y = 0.0f32;
    for (si, section) in sheet.sections.iter().enumerate() {
        out.sections.push((y, section.title.clone()));
        let wide = section.wide == Some(true);
        let mut columns = if wide {
            vec![y + ID_SECTION_TITLE]
        } else {
            vec![y + ID_SECTION_TITLE, y + ID_SECTION_TITLE]
        };
        let span = if wide {
            width - ID_INSET * 2.0
        } else {
            ID_COLUMN
        };
        for (gi, group) in section.groups.iter().enumerate() {
            let c = usize::from(columns.len() > 1 && columns[1] < columns[0]);
            let x = ID_INSET + c as f32 * (ID_COLUMN + ID_INSET);
            let mut gy = columns[c] + 6.0;
            out.groups
                .push((x + 6.0, gy + 2.0, span - 6.0, group.title.clone()));
            gy += ID_GROUP_TITLE;
            for ii in 0..group.items.len() {
                out.rows
                    .push((Rect::new(x, gy, span, ID_ROW), si, gi, ii));
                gy += ID_ROW;
            }
            columns[c] = gy;
        }
        y = columns.iter().copied().fold(y, f32::max) + 14.0;
    }
    out.height = y + ID_INSET - 14.0;
    out
}

/// A row of runs as plain text with its lead cells (for the tooltip-free row's label and the tests).
pub fn row_label(item: &identities::Identity) -> String {
    identities::identity_label(&item.show, item.lead.as_deref().unwrap_or(&[]))
}

// ---- state ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Tile(usize),
    Formula,
    Part(usize),
    Clear,
    Row(usize),
    Thumb,
}

struct Fonts {
    /// 8.5 tile number and mass, 8.5 semibold badge, 17 medium symbol
    small: IDWriteTextFormat,
    badge: IDWriteTextFormat,
    symbol: IDWriteTextFormat,
    /// 15 medium detail name, 11 detail line / hints / parts / notes, 11 medium clear and group titles
    name: IDWriteTextFormat,
    caption: IDWriteTextFormat,
    caption_medium: IDWriteTextFormat,
    /// 20 medium formula
    formula: IDWriteTextFormat,
    /// 13 math, 9.5 raised, 8 deeper, 12 semibold section title, 13 semibold heading
    math: IDWriteTextFormat,
    raised: IDWriteTextFormat,
    deeper: IDWriteTextFormat,
    section: IDWriteTextFormat,
    heading: IDWriteTextFormat,
}

impl Fonts {
    fn new(gfx: &Gfx) -> windows::core::Result<Fonts> {
        let f = |size: f32, weight: DWRITE_FONT_WEIGHT| format(&gfx.dw, &gfx.fonts.text_family, size, weight);
        Ok(Fonts {
            small: f(8.5, DWRITE_FONT_WEIGHT_REGULAR)?,
            badge: f(8.5, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            symbol: f(17.0, DWRITE_FONT_WEIGHT_MEDIUM)?,
            name: f(15.0, DWRITE_FONT_WEIGHT_MEDIUM)?,
            caption: f(11.0, DWRITE_FONT_WEIGHT_REGULAR)?,
            caption_medium: f(11.0, DWRITE_FONT_WEIGHT_MEDIUM)?,
            formula: f(20.0, DWRITE_FONT_WEIGHT_MEDIUM)?,
            math: f(13.0, DWRITE_FONT_WEIGHT_REGULAR)?,
            raised: f(9.5, DWRITE_FONT_WEIGHT_REGULAR)?,
            deeper: f(8.0, DWRITE_FONT_WEIGHT_REGULAR)?,
            section: f(12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            heading: f(13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
        })
    }
}

/// A one-line layout, top-left anchored, with its size and baseline.
#[derive(Clone)]
struct Text {
    layout: IDWriteTextLayout,
    w: f32,
    h: f32,
    baseline: f32,
}

fn text(gfx: &Gfx, s: &str, format: &IDWriteTextFormat) -> Option<Text> {
    let wide: Vec<u16> = s.encode_utf16().collect();
    // SAFETY: wide outlives the call; the format is a live COM object
    let layout = unsafe { gfx.dw.CreateTextLayout(&wide, format, 100_000.0, 1_000.0) }.ok()?;
    // SAFETY: setters and queries on a live layout with live out locals
    unsafe {
        let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    }
    Some(measured(layout))
}

fn measured(layout: IDWriteTextLayout) -> Text {
    let (w, h) = crate::render::measure(&layout);
    let mut lines = [DWRITE_LINE_METRICS::default()];
    let mut n = 0u32;
    // SAFETY: a live layout, a one-element out slice and count
    let _ = unsafe { layout.GetLineMetrics(Some(&mut lines), &mut n) };
    Text {
        layout,
        w,
        h,
        baseline: lines[0].baseline,
    }
}

impl Text {
    fn draw(&self, c: &Canvas, x: f32, y: f32, color: Color) {
        c.text(&self.layout, x, y, color);
    }
}

/// The periodic table's window state (its layouts, made on the first paint).
#[derive(Default)]
struct Periodic {
    tiles: Vec<PeriodicTile>,
    /// per tile: number, symbol, mass
    labels: Vec<(Text, Text, Text)>,
    /// the formula card: (hit, rect, text, resting fill)
    formula: Vec<(Hit, Rect, Text, f32)>,
    formula_hint: Option<Text>,
    detail: Option<(usize, Text, Text)>,
    /// the hovered tile, for the detail line
    hover_tile: Option<usize>,
    /// the window's scale from the design size
    scale: f32,
}

struct IdRow {
    rect: Rect,
    copy: String,
    /// lead cells then the runs: (text, dx, dy) from the row's left and top
    runs: Vec<(Text, f32, f32)>,
    note: Option<Text>,
}

struct Identity {
    id: String,
    title: String,
    heading: Option<Text>,
    status: Option<Text>,
    copied_status: Option<Text>,
    sections: Vec<(f32, Text)>,
    groups: Vec<(f32, f32, Text)>,
    rows: Vec<IdRow>,
    tick: Option<Text>,
    height: f32,
    scroll: f32,
    copied: Option<usize>,
    /// the thumb drag's start: (pointer y, scroll)
    drag: Option<(f32, f32)>,
}

enum Body {
    Periodic(Periodic),
    Identity(Box<Identity>),
}

struct Sheet {
    hwnd: HWND,
    key: String,
    surface: Surface,
    dpi: u32,
    body: Body,
    hover: Option<Hit>,
    pressed: Option<Hit>,
    tracking: bool,
}

#[derive(Default)]
struct Sheets {
    gfx: Option<(Gfx, Fonts)>,
    theme: Option<Theme>,
    open: Vec<Sheet>,
    /// sheet keys, front last
    order: Vec<String>,
    /// where each sheet was when it closed (physical px), to open there again
    frames: HashMap<String, RECT>,
    /// the table's formula outlives its window, as the Mac's panel keeps it
    picks: Picks,
    registered: bool,
}

thread_local! {
    static SHEETS: RefCell<Sheets> = RefCell::new(Sheets::default());
}

fn with<R>(f: impl FnOnce(&mut Sheets) -> R) -> Option<R> {
    SHEETS.with(|s| s.try_borrow_mut().ok().map(|mut s| f(&mut s)))
}

// ---- opening ----

/// Where the bar is, for a sheet to open clear of it: its frame and its monitor's work area (physical px), its
/// monitor's dpi; None while it's away.
#[derive(Clone, Copy, Debug)]
struct Spot {
    bar: Option<(Area, Area, u32)>,
    theme: Theme,
}

fn spot(app: &App) -> Spot {
    let bar = if app.bar.visible {
        app.bar.monitor.as_ref().map(|m| {
            let r = app.bar.frame();
            (
                Area {
                    x: f64::from(r.left),
                    y: f64::from(r.top),
                    width: f64::from(r.right - r.left),
                    height: f64::from(r.bottom - r.top),
                },
                m.area,
                m.dpi,
            )
        })
    } else {
        None
    };
    Spot {
        bar,
        theme: app.theme,
    }
}

/// `periodic`: opens the table (or brings it forward).
pub fn open_periodic(app: &mut App) {
    let at = spot(app);
    later::free("sheet-open", 0, move || open_now("periodic".into(), 0, at));
}

/// An identity sheet (`identities`, `calculus`, `conversions`, `constants`) at a section.
pub fn open_identities(app: &mut App, id: &str, section: usize) {
    if !identities::sheets().iter().any(|s| s.id == id) {
        return;
    }
    let at = spot(app);
    let id = id.to_string();
    later::free("sheet-open", 0, move || open_now(id, section, at));
}

const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0);
const EX_STYLE: WINDOW_EX_STYLE =
    WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0);

/// The window frame (physical px) for a client of `w` × `h` DIPs at `dpi`.
fn frame_for(w: f32, h: f32, dpi: u32) -> (f64, f64) {
    let s = dpi as f32 / 96.0;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: (w * s).round() as i32,
        bottom: (h * s).round() as i32,
    };
    // SAFETY: a live rect
    let _ = unsafe { AdjustWindowRectExForDpi(&mut r, STYLE, false, EX_STYLE, dpi) };
    (f64::from(r.right - r.left), f64::from(r.bottom - r.top))
}

fn open_now(key: String, section: usize, at: Spot) {
    let existing = with(|s| {
        s.theme = Some(at.theme);
        s.order.retain(|k| *k != key);
        s.order.push(key.clone());
        s.open.iter().find(|w| w.key == key).map(|w| w.hwnd)
    })
    .flatten();
    if let Some(hwnd) = existing {
        // asked for again, maybe at another part: there, in front, clear of the bar
        with(|s| {
            if let Some(w) = s.open.iter_mut().find(|w| w.key == key)
                && let Body::Identity(id) = &mut w.body
            {
                scroll_to(id, section, client_h(hwnd, w.dpi));
            }
        });
        keep_clear(hwnd, &key, at);
        // SAFETY: our window; raised among the topmost ones without taking the keys
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        return;
    }
    if let Err(e) = create(&key, section, at) {
        qlog!("sheet {key}: couldn't open: {e}");
        with(|s| s.order.retain(|k| *k != key));
    }
}

fn register() -> windows::core::Result<()> {
    if with(|s| std::mem::replace(&mut s.registered, true)) == Some(true) {
        return Ok(());
    }
    // SAFETY: a class with our static wndproc
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(sheet_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
    }
    Ok(())
}

fn ensure_gfx() -> windows::core::Result<()> {
    if with(|s| s.gfx.is_some()) == Some(true) {
        return Ok(());
    }
    let gfx = Gfx::new()?;
    let fonts = Fonts::new(&gfx)?;
    with(|s| s.gfx = Some((gfx, fonts)));
    Ok(())
}

/// The design size (DIPs) a sheet opens at, its least, and how it gives beside the bar.
fn natural(key: &str) -> Option<((f32, f32), Give, String)> {
    if key == "periodic" {
        let (w, h) = periodic_size();
        return Some(((w, h), Give::Scale(0.6), "Periodic Table".into()));
    }
    let sheet = identities::sheets().iter().find(|s| s.id == key)?;
    let full = identity_layout(sheet).height + ID_TOP;
    let least = full.min(220.0);
    Some((
        (identity_width(), full.min(ID_MAX_HEIGHT)),
        Give::Height(f64::from(least)),
        sheet.title.clone(),
    ))
}

fn create(key: &str, section: usize, at: Spot) -> windows::core::Result<()> {
    let Some(((w, h), give, title)) = natural(key) else {
        return Ok(());
    };
    register()?;
    ensure_gfx()?;
    // where: beside the bar while it's up; the spot it closed at; else centred on the pointer's monitor
    let earlier = with(|s| s.open.len()).unwrap_or(0) as f64;
    let remembered = with(|s| s.frames.get(key).copied()).flatten();
    let (x, y, fw, fh) = match (at.bar, remembered) {
        (_, Some(r)) => (
            f64::from(r.left),
            f64::from(r.top),
            f64::from(r.right - r.left),
            f64::from(r.bottom - r.top),
        ),
        (Some((bar, area, dpi)), None) => {
            let (fw, fh) = frame_for(w, h, dpi);
            let give = scaled_give(give, dpi);
            let r = place::beside(area, bar, fw, fh, place::SHEET_MARGIN * f64::from(dpi) / 96.0, give);
            (r.x, r.y, r.width, r.height)
        }
        (None, None) => {
            let m = crate::window::cursor_monitor();
            let (fw, fh) = frame_for(w, h, m.dpi);
            let (fw, fh) = (fw.min(m.area.width), fh.min(m.area.height));
            let step = CASCADE * m.scale() * earlier;
            let (x, y) = place::fit(
                m.area,
                m.area.x + (m.area.width - fw) / 2.0 + step,
                m.area.y + (m.area.height - fh) / 2.0 + step,
                fw,
                fh,
            );
            (x, y, fw, fh)
        }
    };
    let key_box = Box::new(key.to_string());
    // SAFETY: our class; the key's box is handed to WM_CREATE through lpCreateParams and taken back there
    let hwnd = unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        CreateWindowExW(
            EX_STYLE,
            CLASS,
            &HSTRING::from(title.as_str()),
            STYLE,
            x as i32,
            y as i32,
            fw as i32,
            fh as i32,
            None,
            None,
            Some(instance.into()),
            Some(Box::into_raw(key_box) as *const _),
        )?
    };
    let dark = at.theme.dark;
    set_dark(hwnd, dark);
    // a remembered spot can be under the bar now
    keep_clear(hwnd, key, at);
    with(|s| {
        if let Some(sheet) = s.open.iter_mut().find(|w| w.hwnd == hwnd)
            && let Body::Identity(id) = &mut sheet.body
        {
            scroll_to(id, section, client_h(hwnd, sheet.dpi));
        }
    });
    // SAFETY: our window, shown without taking the keys
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    qlog!("sheet {key}: opened");
    Ok(())
}

fn scaled_give(give: Give, dpi: u32) -> Give {
    match give {
        Give::Height(least) => Give::Height(least * f64::from(dpi) / 96.0),
        g => g,
    }
}

/// A sheet that would cover the bar, or sits off its monitor, moves beside it; one put somewhere clear stays.
fn keep_clear(hwnd: HWND, key: &str, at: Spot) {
    let Some((bar, area, dpi)) = at.bar else {
        return;
    };
    let mut r = RECT::default();
    // SAFETY: our window, a live out rect
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    let now = Area {
        x: f64::from(r.left),
        y: f64::from(r.top),
        width: f64::from(r.right - r.left),
        height: f64::from(r.bottom - r.top),
    };
    if !place::needs_moving(now, bar, area) {
        return;
    }
    let Some((_, give, _)) = natural(key) else {
        return;
    };
    let p = place::beside(
        area,
        bar,
        now.width,
        now.height,
        place::SHEET_MARGIN * f64::from(dpi) / 96.0,
        scaled_give(give, dpi),
    );
    // SAFETY: our window
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            p.x as i32,
            p.y as i32,
            p.width as i32,
            p.height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

fn set_dark(hwnd: HWND, dark: bool) {
    let v = BOOL::from(dark);
    // SAFETY: an attribute on our own window with a pointer to a live local
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

/// The app's theme changed: the open sheets follow.
pub fn retheme(theme: Theme) {
    let open: Vec<HWND> = with(|s| {
        s.theme = Some(theme);
        s.open.iter().map(|w| w.hwnd).collect()
    })
    .unwrap_or_default();
    for hwnd in open {
        set_dark(hwnd, theme.dark);
        // SAFETY: our window
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
}

/// Ctrl+W: closes the front sheet; false when none is open, so the key goes on to the bar. Safe to call while the
/// app is borrowed (the close is posted).
pub fn close_front() -> bool {
    let front = with(|s| {
        let key = s.order.iter().rev().find(|k| s.open.iter().any(|w| &w.key == *k))?.clone();
        s.open.iter().find(|w| w.key == key).map(|w| w.hwnd)
    })
    .flatten();
    match front {
        Some(hwnd) => {
            // SAFETY: our window; WM_CLOSE comes back through the loop
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            true
        }
        None => false,
    }
}

/// Whether any sheet is open (for the bench and tests).
pub fn open_keys() -> Vec<String> {
    with(|s| s.open.iter().map(|w| w.key.clone()).collect()).unwrap_or_default()
}

/// The table is up with a formula built: ↵ in the bar is the table's.
pub fn has_formula() -> bool {
    with(|s| !s.picks.0.is_empty() && s.open.iter().any(|w| w.key == "periodic")).unwrap_or(false)
}

/// ↵ in the bar (or a click on the formula): the formula's molar mass goes in the bar and the table starts over.
/// From the bar's key handler, inside the app's borrow.
pub fn put_formula_in(app: &mut App) -> bool {
    let Some(mass) = take_formula() else {
        return false;
    };
    insert_in_bar(app, &mass);
    true
}

fn take_formula() -> Option<String> {
    let (mass, hwnd) = with(|s| {
        let f = s.picks.formula()?;
        s.picks.0.clear();
        let hwnd = s.open.iter().find(|w| w.key == "periodic").map(|w| w.hwnd);
        Some((f.mass, hwnd))
    })
    .flatten()?;
    if let Some(hwnd) = hwnd {
        with(|s| {
            if let Some(Sheet {
                body: Body::Periodic(p),
                ..
            }) = s.open.iter_mut().find(|w| w.hwnd == hwnd)
            {
                p.formula.clear();
            }
        });
        // SAFETY: our window
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
    Some(mass)
}

fn insert_in_bar(app: &mut App, text: &str) {
    if !app.bar.visible {
        app.op(crate::app::Op::Show);
    }
    app.model.insert_text(text, &app.engine);
}

// ---- the window ----

fn client_h(hwnd: HWND, dpi: u32) -> f32 {
    let mut r = RECT::default();
    // SAFETY: our window
    let _ = unsafe { GetClientRect(hwnd, &mut r) };
    (r.bottom - r.top) as f32 * 96.0 / dpi as f32
}

fn build(key: &str, hwnd: HWND, dpi: u32) -> Option<Sheet> {
    let body = if key == "periodic" {
        Body::Periodic(Periodic {
            tiles: elements::periodic_tiles(),
            scale: 1.0,
            ..Periodic::default()
        })
    } else {
        let sheet = identities::sheets().iter().find(|s| s.id == key)?;
        Body::Identity(Box::new(build_identity(sheet)?))
    };
    Some(Sheet {
        hwnd,
        key: key.to_string(),
        surface: Surface::new(hwnd, dpi),
        dpi,
        body,
        hover: None,
        pressed: None,
        tracking: false,
    })
}

fn build_identity(sheet: &identities::IdentitySheet) -> Option<Identity> {
    with(|s| {
        let (gfx, fonts) = s.gfx.as_ref()?;
        let l = identity_layout(sheet);
        let lead_widths = identities::LEAD_WIDTHS.map(|w| w as f32);
        let rows = l
            .rows
            .iter()
            .map(|&(rect, si, gi, ii)| {
                let item = &sheet.sections[si].groups[gi].items[ii];
                let main = text(gfx, "Ag", &fonts.math);
                let (main_h, main_base) = main.map_or((17.0, 13.0), |t| (t.h, t.baseline));
                let base_y = (ID_ROW - main_h) / 2.0 + main_base;
                let mut runs = Vec::new();
                let mut x = 6.0;
                let mut put = |show: &str, x: &mut f32| {
                    for r in identities::identity_runs(show) {
                        let (format, offset) = match r.shift {
                            Some(Shift::Sup) => (&fonts.raised, 5.0),
                            Some(Shift::Sub) => (&fonts.raised, -3.0),
                            Some(Shift::Supsub) => (&fonts.deeper, 2.0),
                            None => (&fonts.math, 0.0),
                        };
                        if let Some(t) = text(gfx, &r.text, format) {
                            let dy = base_y - t.baseline - offset;
                            let w = t.w;
                            runs.push((t, *x, dy));
                            *x += w;
                        }
                    }
                };
                for (i, cell) in item.lead.as_deref().unwrap_or(&[]).iter().enumerate() {
                    let mut cx = x;
                    put(cell, &mut cx);
                    x += lead_widths[i.min(lead_widths.len() - 1)];
                }
                put(&item.show, &mut x);
                let note = item
                    .note
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .and_then(|n| text(gfx, n, &fonts.caption));
                IdRow {
                    rect,
                    copy: item.copy.clone(),
                    runs,
                    note,
                }
            })
            .collect();
        Some(Identity {
            id: sheet.id.clone(),
            title: sheet.title.clone(),
            heading: text(gfx, &sheet.title, &fonts.heading),
            status: text(gfx, ID_HINT, &fonts.caption),
            copied_status: text(gfx, "✓ copied", &fonts.caption),
            sections: l
                .sections
                .iter()
                .filter_map(|(y, t)| Some((*y, text(gfx, t, &fonts.section)?)))
                .collect(),
            groups: l
                .groups
                .iter()
                .filter_map(|(x, y, _, t)| Some((*x, *y, text(gfx, t, &fonts.caption_medium)?)))
                .collect(),
            rows,
            tick: text(gfx, "✓", &fonts.caption),
            height: l.height,
            scroll: 0.0,
            copied: None,
            drag: None,
        })
    })
    .flatten()
}

fn scroll_to(id: &mut Identity, section: usize, client_h: f32) {
    let sheet = identities::sheets().iter().find(|s| s.id == id.id);
    let Some(sheet) = sheet else { return };
    let tops = identity_layout(sheet).sections;
    if let Some((top, _)) = tops.get(section) {
        let most = (id.height - (client_h - ID_TOP)).max(0.0);
        id.scroll = top.min(most);
    }
}

unsafe extern "system" fn sheet_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: lParam is our CREATESTRUCTW; its params are the key's box from create()
            let key = unsafe {
                let cs = &*(lp.0 as *const CREATESTRUCTW);
                *Box::from_raw(cs.lpCreateParams as *mut String)
            };
            // SAFETY: plain query on our window
            let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
            if let Some(sheet) = build(&key, hwnd, dpi) {
                with(|s| s.open.push(sheet));
            }
            // SAFETY: a marker that this window is a sheet
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 1) };
            return LRESULT(0);
        }
        WM_MOUSEACTIVATE => return LRESULT(3), // MA_NOACTIVATE
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            // SAFETY: our window
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            return LRESULT(0);
        }
        WM_SIZE => {
            let (w, h) = ((lp.0 & 0xffff) as u32, ((lp.0 >> 16) & 0xffff) as u32);
            on_sheet(hwnd, |s, _| {
                s.surface.resize(w, h);
                fit_scale(s, w);
            });
            // SAFETY: our window
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            let dpi = (wp.0 & 0xffff) as u32;
            on_sheet(hwnd, |s, _| {
                s.dpi = dpi;
                s.surface.set_dpi(dpi);
            });
            // SAFETY: lParam is the suggested frame
            let r = unsafe { *(lp.0 as *const RECT) };
            // SAFETY: our window
            let _ = unsafe {
                SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE)
            };
            return LRESULT(0);
        }
        WM_GETMINMAXINFO => {
            if let Some(info) = limits(hwnd) {
                // SAFETY: lParam is a live MINMAXINFO
                let mm = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
                mm.ptMinTrackSize = info.0;
                mm.ptMaxTrackSize = info.1;
                return LRESULT(0);
            }
        }
        WM_SIZING => {
            // the table keeps its shape: the side dragged leads, the other follows
            // SAFETY: lParam is the live drag rect
            let r = unsafe { &mut *(lp.0 as *mut RECT) };
            keep_shape(hwnd, wp.0 as u32, r);
            return LRESULT(1);
        }
        WM_MOUSEMOVE => {
            let (x, y) = point(lp);
            mouse_move(hwnd, x, y);
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            on_sheet(hwnd, |s, _| {
                s.tracking = false;
                s.hover = None;
                if let Body::Periodic(p) = &mut s.body {
                    p.hover_tile = None;
                    p.detail = None;
                }
            });
            // SAFETY: our window
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            return LRESULT(0);
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN => {
            let (x, y) = point(lp);
            let right = msg == WM_RBUTTONDOWN;
            let hit = on_sheet(hwnd, |s, _| {
                let hit = hit_test(s, x, y);
                s.pressed = hit;
                let dy = y as f32 * 96.0 / s.dpi as f32;
                if let (Some(Hit::Thumb), Body::Identity(id)) = (hit, &mut s.body) {
                    id.drag = Some((dy, id.scroll));
                }
                hit
            })
            .flatten();
            raise(hwnd);
            if hit.is_some() || right {
                // SAFETY: our window takes the mouse until the button comes up
                unsafe { SetCapture(hwnd) };
            }
            // SAFETY: our window
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            return LRESULT(0);
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let (x, y) = point(lp);
            // SAFETY: we may hold the capture
            let _ = unsafe { ReleaseCapture() };
            let right = msg == WM_RBUTTONUP;
            let act = on_sheet(hwnd, |s, _| {
                let pressed = s.pressed.take();
                if let Body::Identity(id) = &mut s.body {
                    id.drag = None;
                }
                let now = hit_test(s, x, y);
                (pressed.is_some() && pressed == now).then_some(now).flatten()
            })
            .flatten();
            if let Some(hit) = act {
                act_on(hwnd, hit, right);
            }
            // SAFETY: our window
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let delta = f32::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
            on_sheet(hwnd, |s, _| {
                let h = client_h(hwnd, s.dpi);
                if let Body::Identity(id) = &mut s.body {
                    let most = (id.height - (h - ID_TOP)).max(0.0);
                    id.scroll = (id.scroll - delta / 120.0 * 3.0 * ID_ROW).clamp(0.0, most);
                }
            });
            // SAFETY: our window
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            if (lp.0 & 0xffff) as u32 == 1 {
                let over = on_sheet(hwnd, |s, _| s.hover.is_some()).unwrap_or(false);
                // SAFETY: system cursors
                unsafe {
                    if let Ok(c) = LoadCursorW(None, if over { IDC_HAND } else { IDC_ARROW }) {
                        SetCursor(Some(c));
                    }
                }
                return LRESULT(1);
            }
        }
        WM_CLOSE => {
            let mut r = RECT::default();
            // SAFETY: our window
            let _ = unsafe { GetWindowRect(hwnd, &mut r) };
            with(|s| {
                if let Some(w) = s.open.iter().find(|w| w.hwnd == hwnd) {
                    let key = w.key.clone();
                    s.frames.insert(key, r);
                }
            });
            // SAFETY: our window
            let _ = unsafe { DestroyWindow(hwnd) };
            return LRESULT(0);
        }
        WM_DESTROY => {
            with(|s| {
                if let Some(i) = s.open.iter().position(|w| w.hwnd == hwnd) {
                    let w = s.open.remove(i);
                    s.order.retain(|k| *k != w.key);
                    qlog!("sheet {}: closed", w.key);
                }
                // the last one gone: its factories and fonts go too
                if s.open.is_empty() {
                    s.gfx = None;
                }
            });
            return LRESULT(0);
        }
        _ => {}
    }
    // SAFETY: the default for our own window
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn point(lp: LPARAM) -> (i32, i32) {
    (
        i32::from((lp.0 & 0xffff) as u16 as i16),
        i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    )
}

fn on_sheet<R>(hwnd: HWND, f: impl FnOnce(&mut Sheet, Option<&(Gfx, Fonts)>) -> R) -> Option<R> {
    with(|s| {
        let gfx = s.gfx.as_ref();
        let sheet = s.open.iter_mut().find(|w| w.hwnd == hwnd)?;
        Some(f(sheet, gfx))
    })
    .flatten()
}

/// The table's scale for a client `w` px wide (its DIPs then match the design size).
fn fit_scale(s: &mut Sheet, w_px: u32) {
    if let Body::Periodic(p) = &mut s.body {
        let (dw, _) = periodic_size();
        let want = (w_px as f32 / (s.dpi as f32 / 96.0) / dw).max(0.1);
        // the render target's dpi is a whole number: the scale clicks are read at is the one it draws at
        let dpi = (s.dpi as f32 * want).round().max(1.0) as u32;
        p.scale = dpi as f32 / s.dpi as f32;
        s.surface.set_dpi(dpi);
    }
}

/// The least and most a sheet's frame may be, physical px.
fn limits(hwnd: HWND) -> Option<(POINT, POINT)> {
    let (key, dpi) = with(|s| s.open.iter().find(|w| w.hwnd == hwnd).map(|w| (w.key.clone(), w.dpi))).flatten()?;
    let ((w, h), _, _) = natural(&key)?;
    let (least, most) = if key == "periodic" {
        ((w * 0.6, h * 0.6), (w * 2.5, h * 2.5))
    } else {
        let sheet = identities::sheets().iter().find(|s| s.id == key)?;
        let full = identity_layout(sheet).height + ID_TOP;
        ((w, full.min(220.0)), (w, full))
    };
    let (a, b) = (frame_for(least.0, least.1, dpi), frame_for(most.0, most.1, dpi));
    Some((
        POINT {
            x: a.0 as i32,
            y: a.1 as i32,
        },
        POINT {
            x: b.0 as i32,
            y: b.1 as i32,
        },
    ))
}

fn keep_shape(hwnd: HWND, edge: u32, r: &mut RECT) {
    let Some(dpi) = with(|s| {
        s.open
            .iter()
            .find(|w| w.hwnd == hwnd && w.key == "periodic")
            .map(|w| w.dpi)
    })
    .flatten() else {
        return;
    };
    let (dw, dh) = periodic_size();
    let (fw, fh) = frame_for(dw, dh, dpi);
    let (cw, ch) = frame_for(0.0, 0.0, dpi);
    let ratio = (fh - ch) / (fw - cw);
    let w = f64::from(r.right - r.left) - cw;
    let h = f64::from(r.bottom - r.top) - ch;
    // WMSZ_LEFT 1, RIGHT 2, TOP 3, BOTTOM 6: a top or bottom drag leads with the height
    if edge == 3 || edge == 6 {
        let nw = (h / ratio + cw).round() as i32;
        r.right = r.left + nw;
    } else {
        let nh = (w * ratio + ch).round() as i32;
        // WMSZ_TOPLEFT 4, TOPRIGHT 5: the top moves
        if edge == 4 || edge == 5 {
            r.top = r.bottom - nh;
        } else {
            r.bottom = r.top + nh;
        }
    }
}

fn raise(hwnd: HWND) {
    with(|s| {
        if let Some(w) = s.open.iter().find(|w| w.hwnd == hwnd) {
            let key = w.key.clone();
            s.order.retain(|k| *k != key);
            s.order.push(key);
        }
    });
    // SAFETY: our window, among the topmost ones, keys left where they are
    let _ = unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
}

/// Design coordinates (DIPs at scale 1) of a client point in px.
fn design(s: &Sheet, x: i32, y: i32) -> (f32, f32) {
    let k = s.dpi as f32 / 96.0
        * match &s.body {
            Body::Periodic(p) => p.scale,
            Body::Identity(_) => 1.0,
        };
    (x as f32 / k, y as f32 / k)
}

fn hit_test(s: &Sheet, x: i32, y: i32) -> Option<Hit> {
    let (x, y) = design(s, x, y);
    match &s.body {
        Body::Periodic(p) => {
            if let Some(i) = p
                .tiles
                .iter()
                .position(|t| cell(t.row, t.col).contains(x, y))
            {
                return Some(Hit::Tile(i));
            }
            p.formula
                .iter()
                .find(|(_, r, _, _)| r.contains(x, y))
                .map(|(h, ..)| *h)
        }
        Body::Identity(id) => {
            let h = client_h(s.hwnd, s.dpi);
            if let Some(thumb) = thumb_rect(id, h)
                && thumb.inset(-4.0, 0.0).contains(x, y)
            {
                return Some(Hit::Thumb);
            }
            if y < ID_TOP {
                return None;
            }
            let cy = y - ID_TOP + id.scroll;
            id.rows
                .iter()
                .position(|r| r.rect.contains(x, cy))
                .map(Hit::Row)
        }
    }
}

fn mouse_move(hwnd: HWND, x: i32, y: i32) {
    let changed = on_sheet(hwnd, |s, _| {
        if !s.tracking {
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: a live struct for our window
            let _ = unsafe { TrackMouseEvent(&mut t) };
            s.tracking = true;
        }
        // dragging the thumb scrolls
        let dpi = s.dpi;
        if let Body::Identity(id) = &mut s.body
            && let Some((y0, scroll0)) = id.drag
        {
            let view = client_h(hwnd, dpi) - ID_TOP;
            let most = (id.height - view).max(0.0);
            let dy = y as f32 * 96.0 / dpi as f32;
            let k = if view > 0.0 { id.height / view } else { 1.0 };
            id.scroll = (scroll0 + (dy - y0) * k).clamp(0.0, most);
            return true;
        }
        let hit = hit_test(s, x, y);
        let mut changed = hit != s.hover;
        s.hover = hit;
        if let Body::Periodic(p) = &mut s.body {
            // leaving a tile for the card or the background keeps the last name until another tile is hovered
            let tile = match hit {
                Some(Hit::Tile(i)) => Some(i),
                _ => None,
            };
            if tile.is_some() && tile != p.hover_tile {
                p.hover_tile = tile;
                p.detail = None;
                changed = true;
            }
            if tile.is_none() && p.hover_tile.is_some() && !matches!(hit, Some(Hit::Tile(_))) {
                p.hover_tile = None;
                p.detail = None;
                changed = true;
            }
        }
        changed
    })
    .unwrap_or(false);
    if changed {
        // SAFETY: our window
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
}

/// A click (or right click) that went down and up on the same thing.
fn act_on(hwnd: HWND, hit: Hit, right: bool) {
    match hit {
        Hit::Tile(i) => {
            with(|s| {
                let symbol = s.open.iter().find(|w| w.hwnd == hwnd).and_then(|w| match &w.body {
                    Body::Periodic(p) => p.tiles.get(i).map(|t| t.symbol.clone()),
                    Body::Identity(_) => None,
                });
                if let Some(symbol) = symbol {
                    if right {
                        s.picks.drop_one(&symbol);
                    } else {
                        s.picks.pick(&symbol);
                    }
                    formula_changed(s, hwnd);
                }
            });
        }
        Hit::Part(k) => {
            with(|s| {
                let order = order_of(&s.picks);
                if let Some(symbol) = order.get(k) {
                    let symbol = symbol.clone();
                    s.picks.drop_one(&symbol);
                }
                formula_changed(s, hwnd);
            });
        }
        Hit::Clear => {
            with(|s| {
                s.picks.0.clear();
                formula_changed(s, hwnd);
            });
        }
        Hit::Formula if !right => {
            if let Some(mass) = take_formula() {
                // two steps: the bar shows (if it's away), then the mass goes in the line it shows
                crate::app::poke(|app| {
                    if !app.bar.visible {
                        app.op(crate::app::Op::Show);
                    }
                });
                crate::app::poke(|app| app.model.insert_text(&mass, &app.engine));
            }
        }
        Hit::Row(i) if !right => copy_row(hwnd, i),
        _ => {}
    }
}

fn order_of(picks: &Picks) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    for p in &picks.0 {
        if !order.contains(p) {
            order.push(p.clone());
        }
    }
    order
}

fn formula_changed(s: &mut Sheets, hwnd: HWND) {
    if let Some(Sheet {
        body: Body::Periodic(p),
        ..
    }) = s.open.iter_mut().find(|w| w.hwnd == hwnd)
    {
        p.formula.clear();
        p.formula_hint = None;
    }
}

fn copy_row(hwnd: HWND, i: usize) {
    let copy = on_sheet(hwnd, |s, _| match &mut s.body {
        Body::Identity(id) => {
            id.copied = Some(i);
            id.rows.get(i).map(|r| r.copy.clone())
        }
        Body::Periodic(_) => None,
    })
    .flatten();
    let Some(copy) = copy else { return };
    if !crate::clip::set(hwnd, &copy) {
        qlog!("sheet: couldn't copy a row");
    }
    let target = hwnd.0 as usize;
    later::free("sheet-copied", 1200, move || {
        let hwnd = HWND(target as *mut _);
        // SAFETY: a plain check that the window is still ours to touch
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return;
        }
        on_sheet(hwnd, |s, _| {
            if let Body::Identity(id) = &mut s.body {
                id.copied = None;
            }
        });
        // SAFETY: our window
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    });
}

// ---- painting ----

fn tint(category: ElementCategory, dark: bool) -> Color {
    // the system colours the Mac's tiles use (systemOrange, Blue, Teal, Green, Purple, Pink, Gray)
    let hex = match category {
        ElementCategory::Alkali | ElementCategory::Alkaline => {
            if dark {
                0xFF9F0A
            } else {
                0xFF9500
            }
        }
        ElementCategory::Transition => {
            if dark {
                0x0A84FF
            } else {
                0x007AFF
            }
        }
        ElementCategory::Post | ElementCategory::Metalloid => {
            if dark {
                0x40C8E0
            } else {
                0x30B0C7
            }
        }
        ElementCategory::Nonmetal | ElementCategory::Halogen => {
            if dark {
                0x30D158
            } else {
                0x34C759
            }
        }
        ElementCategory::Noble => {
            if dark {
                0xBF5AF2
            } else {
                0xAF52DE
            }
        }
        ElementCategory::Lanthanide | ElementCategory::Actinide => {
            if dark {
                0xFF375F
            } else {
                0xFF2D55
            }
        }
    };
    rgb(hex, 1.0)
}

fn paint(hwnd: HWND) {
    with(|s| {
        let theme = s.theme.unwrap_or_else(|| Theme::for_setting("system"));
        let picks = s.picks.clone();
        let Some((gfx, fonts)) = s.gfx.as_ref() else {
            return;
        };
        let Some(sheet) = s.open.iter_mut().find(|w| w.hwnd == hwnd) else {
            return;
        };
        let hover = sheet.hover;
        let pressed = sheet.pressed;
        let dpi = sheet.dpi;
        let Sheet { surface, body, .. } = sheet;
        if let Body::Periodic(p) = body {
            prepare_periodic(p, gfx, fonts, &picks);
        }
        let ch = client_h(hwnd, dpi);
        let result = surface.draw(gfx, &theme, |c| {
            c.clear(theme.bg);
            match body {
                Body::Periodic(p) => paint_periodic(c, fonts, p, &picks, hover, pressed),
                Body::Identity(id) => paint_identity(c, id, ch, hover, pressed),
            }
        });
        if let Err(e) = result {
            qlog!("sheet paint failed: {e}");
        }
    });
}

/// The table's text layouts, once; the formula card's whenever the picks changed.
fn prepare_periodic(p: &mut Periodic, gfx: &Gfx, fonts: &Fonts, picks: &Picks) {
    if p.labels.is_empty() {
        p.labels = p
            .tiles
            .iter()
            .filter_map(|t| {
                Some((
                    text(gfx, &t.n.to_string(), &fonts.small)?,
                    text(gfx, &t.symbol, &fonts.symbol)?,
                    text(gfx, &t.mass, &fonts.small)?,
                ))
            })
            .collect();
    }
    if p.detail.is_none()
        && let Some(i) = p.hover_tile
        && let Some(t) = p.tiles.get(i)
        && let (Some(a), Some(b)) = (
            text(gfx, &t.name, &fonts.name),
            text(gfx, &format!("{} · {}", t.symbol, t.n), &fonts.caption),
        )
    {
        p.detail = Some((i, a, b));
    }
    if !p.formula.is_empty() || p.formula_hint.is_some() {
        return;
    }
    let frame = union(cell(2, 3), cell(3, 12));
    let Some(f) = picks.formula() else {
        p.formula_hint = text(
            gfx,
            "click elements to build a formula · right click takes one away",
            &fonts.caption,
        );
        return;
    };
    // the formula and its molar mass: 20 medium, then the mass at 13 regular
    let main = format!("{}  {} g/mol", f.formula, f.mass);
    let wide: Vec<u16> = main.encode_utf16().collect();
    let split = f.formula.encode_utf16().count() as u32;
    // SAFETY: wide outlives the call; setters on the live layout
    let layout = unsafe {
        gfx.dw
            .CreateTextLayout(&wide, &fonts.formula, 100_000.0, 1_000.0)
            .ok()
            .inspect(|l| {
                let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let range = DWRITE_TEXT_RANGE {
                    startPosition: split,
                    length: wide.len() as u32 - split,
                };
                let _ = l.SetFontSize(13.0, range);
                let _ = l.SetFontWeight(DWRITE_FONT_WEIGHT_REGULAR, range);
            })
    };
    let Some(main) = layout.map(measured) else {
        return;
    };
    let fw = main.w.ceil() + 12.0;
    let fh = main.h.ceil() + 4.0;
    let formula_rect = Rect::new(frame.x + frame.w / 2.0 - fw / 2.0, frame.y + 18.0, fw, fh);
    let mut items = vec![(Hit::Formula, formula_rect, main, 0.0)];
    let parts: Vec<(Hit, Text)> = f
        .parts
        .iter()
        .enumerate()
        .filter_map(|(k, part)| {
            Some((
                Hit::Part(k),
                text(gfx, &format!("{} {}", part.text, part.mass), &fonts.caption)?,
            ))
        })
        .collect();
    let clear = text(gfx, "Clear", &fonts.caption_medium);
    let clear_gap = 8.0;
    let width = parts.iter().map(|(_, t)| t.w.ceil() + 12.0).sum::<f32>()
        + clear.as_ref().map_or(0.0, |t| clear_gap + t.w.ceil() + 12.0);
    let mut x = frame.x + frame.w / 2.0 - width / 2.0;
    let y = formula_rect.bottom() + 4.0;
    for (hit, t) in parts {
        let r = Rect::new(x, y, t.w.ceil() + 12.0, t.h.ceil() + 4.0);
        x += r.w;
        items.push((hit, r, t, 0.0));
    }
    if let Some(t) = clear {
        let r = Rect::new(x + clear_gap, y, t.w.ceil() + 12.0, t.h.ceil() + 4.0);
        items.push((Hit::Clear, r, t, 0.06));
    }
    p.formula = items;
}

fn paint_periodic(
    c: &Canvas,
    fonts: &Fonts,
    p: &Periodic,
    picks: &Picks,
    hover: Option<Hit>,
    pressed: Option<Hit>,
) {
    let t = c.theme;
    for (i, tile) in p.tiles.iter().enumerate() {
        let r = cell(tile.row, tile.col);
        let count = picks.count(&tile.symbol);
        let tone = tint(tile.category, t.dark);
        let alpha = if pressed == Some(Hit::Tile(i)) {
            0.4
        } else if count > 0 {
            0.32
        } else if hover == Some(Hit::Tile(i)) {
            0.2
        } else {
            0.1
        };
        if count > 0 {
            // the ring: the tint at 70%, then the tile inside it
            c.fill_round(r, 6.0, with_alpha(tone, 0.7));
            let inner = r.inset(1.5, 1.5);
            c.fill_round(inner, 4.5, t.bg);
            c.fill_round(inner, 4.5, with_alpha(tone, alpha));
        } else {
            c.fill_round(r, 6.0, with_alpha(tone, alpha));
        }
        let Some((number, symbol, mass)) = p.labels.get(i) else {
            continue;
        };
        number.draw(c, r.x + 4.0, r.y + 3.0, t.muted);
        if count > 1
            && let Some(badge) = text(c.gfx, &format!("×{count}"), &fonts.badge)
        {
            badge.draw(c, r.right() - badge.w - 4.0, r.y + 3.0, t.ink);
        }
        symbol.draw(c, r.x + (r.w - symbol.w) / 2.0, r.y + 11.0, t.ink);
        mass.draw(c, r.x + (r.w - mass.w) / 2.0, r.bottom() - mass.h - 4.0, t.muted);
    }
    // the hovered element's name in the first row's gap
    let detail = union(cell(1, 3), cell(1, 12));
    if let Some((_, name, sub)) = &p.detail {
        name.draw(c, detail.x + (detail.w - name.w) / 2.0, detail.y + 6.0, t.ink);
        sub.draw(c, detail.x + (detail.w - sub.w) / 2.0, detail.y + 28.0, t.muted);
    }
    let frame = union(cell(2, 3), cell(3, 12));
    if let Some(hint) = &p.formula_hint {
        hint.draw(
            c,
            frame.x + (frame.w - hint.w) / 2.0,
            frame.y + frame.h / 2.0 - hint.h / 2.0,
            t.muted,
        );
    }
    for (hit, r, text, resting) in &p.formula {
        let alpha = resting
            + if pressed == Some(*hit) {
                0.08
            } else if hover == Some(*hit) {
                0.05
            } else {
                0.0
            };
        if alpha > 0.0 {
            c.fill_round(*r, 5.0, with_alpha(t.ink, alpha));
        }
        let ink = match hit {
            Hit::Part(_) => t.muted,
            _ => t.ink,
        };
        text.draw(c, r.x + 6.0, r.y + 2.0, ink);
    }
}

/// The scrollbar's thumb, while the content overflows (DIPs, window coordinates).
fn thumb_rect(id: &Identity, client_h: f32) -> Option<Rect> {
    let view = client_h - ID_TOP;
    if id.height <= view + 0.5 || view <= 0.0 {
        return None;
    }
    let h = (view * view / id.height).max(24.0);
    let most = id.height - view;
    let y = ID_TOP + (view - h) * (id.scroll / most).clamp(0.0, 1.0);
    Some(Rect::new(identity_width() - 9.0, y + 2.0, 6.0, h - 4.0))
}

fn paint_identity(c: &Canvas, id: &mut Identity, client_h: f32, hover: Option<Hit>, pressed: Option<Hit>) {
    let t = c.theme;
    let width = identity_width();
    let view = client_h - ID_TOP;
    let most = (id.height - view).max(0.0);
    id.scroll = id.scroll.clamp(0.0, most);
    // the header: the title right of the middle's left, the status left of its right
    if let Some(h) = &id.heading {
        h.draw(c, width / 2.0 - 4.0 - h.w, 7.0, t.ink);
    }
    let status = if id.copied.is_some() {
        &id.copied_status
    } else {
        &id.status
    };
    if let Some(s) = status {
        s.draw(c, width / 2.0 + 4.0, 9.0, t.muted);
    }
    c.push_clip(Rect::new(0.0, ID_TOP, width, view.max(0.0)));
    let dy = ID_TOP - id.scroll;
    let visible = |y: f32, h: f32| y + dy + h >= ID_TOP && y + dy <= client_h;
    for (y, title) in &id.sections {
        if visible(*y, ID_SECTION_TITLE) {
            title.draw(c, ID_INSET, y + dy + 4.0, t.ink);
            c.fill(Rect::new(ID_INSET, y + dy + ID_SECTION_TITLE - 3.0, width - ID_INSET * 2.0, 1.0), t.line);
        }
    }
    for (x, y, title) in &id.groups {
        if visible(*y, 16.0) {
            title.draw(c, *x, y + dy, t.muted);
        }
    }
    for (i, row) in id.rows.iter().enumerate() {
        if !visible(row.rect.y, row.rect.h) {
            continue;
        }
        let r = Rect::new(row.rect.x, row.rect.y + dy, row.rect.w, row.rect.h);
        let fill = if pressed == Some(Hit::Row(i)) {
            0.08
        } else if hover == Some(Hit::Row(i)) {
            0.05
        } else {
            0.0
        };
        if fill > 0.0 {
            c.fill_round(r, 6.0, with_alpha(t.ink, fill));
        }
        let copied = id.copied == Some(i);
        let ink = if copied { t.muted } else { t.ink };
        for (text, x, y) in &row.runs {
            text.draw(c, r.x + x, r.y + y, ink);
        }
        let side = if copied { id.tick.as_ref() } else { row.note.as_ref() };
        if let Some(n) = side {
            n.draw(c, r.right() - 6.0 - n.w, r.y + (r.h - n.h) / 2.0, t.muted);
        }
    }
    c.pop_clip();
    if let Some(th) = thumb_rect(id, client_h) {
        let a = if hover == Some(Hit::Thumb) || id.drag.is_some() {
            0.45
        } else {
            0.25
        };
        c.fill_round(th, 3.0, with_alpha(t.ink, a));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_laid_out_as_the_mac_lays_it_out() {
        let (w, h) = periodic_size();
        assert_eq!(w, 16.0 + 17.0 * 47.0 + 44.0 + 16.0);
        assert_eq!(h, 16.0 + 7.0 * 51.0 + 10.0 + 51.0 + 48.0 + 16.0);
        // lanthanides start in column 3 of row 9, under the gap
        let la = cell(9, 3);
        assert!(la.y > cell(7, 3).bottom() + F_GAP - 0.1);
    }

    #[test]
    fn picks_build_a_formula_and_give_back_one_at_a_time() {
        let mut p = Picks::default();
        for s in ["H", "H", "O"] {
            p.pick(s);
        }
        let f = p.formula().unwrap();
        assert_eq!(f.formula, "H₂O");
        assert_eq!(f.mass, "18.015");
        assert_eq!(p.count("H"), 2);
        assert!(p.drop_one("H"));
        assert!(!p.drop_one("C"));
        assert_eq!(p.formula().unwrap().formula, "HO");
        assert_eq!(order_of(&p), vec!["H".to_string(), "O".to_string()]);
        p.0.clear();
        assert!(p.formula().is_none());
    }

    #[test]
    fn identity_sheets_lay_out_in_two_columns() {
        for sheet in identities::sheets() {
            let l = identity_layout(sheet);
            assert_eq!(l.sections.len(), sheet.sections.len());
            let items: usize = sheet.sections.iter().flat_map(|s| &s.groups).map(|g| g.items.len()).sum();
            assert_eq!(l.rows.len(), items, "{}", sheet.id);
            assert!(l.height > 0.0);
            for (r, ..) in &l.rows {
                assert!(r.right() <= identity_width() - ID_INSET + 0.01, "{} {r:?}", sheet.id);
                assert_eq!(r.h, ID_ROW);
            }
            // sections go down the page
            assert!(l.sections.windows(2).all(|w| w[1].0 > w[0].0));
        }
        let c = identities::sheets().iter().find(|s| s.id == "constants").unwrap();
        assert!(row_label(&c.sections[0].groups[0].items[0]).len() > 1);
    }
}
