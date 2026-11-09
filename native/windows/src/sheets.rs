//! The reference sheets, each in a window beside the bar: the periodic table (`periodic`) and the formula sheets
//! (`formulas`, `identities`, `idt <part>`, `trig`, `beams`, `Ohm's law`...). Spec: macos/PeriodicWindow.swift,
//! macos/IdentityWindow.swift, macos/SheetPlace.swift (place.rs `beside` / `needs_moving`),
//! native/mac/Sources/Sheets/SheetData.swift, App/OverlayController.swift (`mainSheetWindow`); the data is the
//! core's (`elements::periodic_tiles`, `formula_of_picks`, `identities::native_identity_sheet_by_id`).
//!
//! **Windows.** As the Tauri app's sheets: a small-captioned, resizable, always-on-top tool window (no taskbar
//! button) that doesn't take the keys when clicked (`WS_EX_NOACTIVATE`, `MA_NOACTIVATE`), so the bar keeps its caret
//! and stays up while a sheet is clicked. A sheet opens clear of the bar on the bar's monitor (below it, else right,
//! left or above), the table scaling down and a formula sheet getting shorter if it has to; with the bar away it
//! opens centred on the pointer's monitor. Ctrl+W closes the front sheet before the bar (`close_front`, for keys.rs).
//!
//! **Periodic table.** A click adds an element to a formula built from clicks (H, H, O is H₂O, with its molar mass
//! and each element's share); a right click on a tile, or a click on a part, takes one away, and Clear starts over.
//! A click on the formula (or ↵ in the bar while one is built, `put_formula_in`) puts its mass in the bar and
//! starts over. Laid out at one size and scaled to the window, which keeps its shape.
//!
//! **Formula sheets: one window, a page at a time.** Every sheet the bar opens goes in one window (`MAIN`),
//! replacing its page: the title, heading and height follow the page and the window keeps its top edge. A
//! `formulas` row opens its sheet in the window it was clicked in; its right-click menu's Open in New Window gives
//! the sheet a window of its own (cascaded, with its own pages). Back and forward: the header's chevrons (once there
//! are two pages), Alt+← / Alt+→, Ctrl+[ / Ctrl+] (the bar passes them on, `bar_key`) and the mouse's back and
//! forward buttons; each page remembers where it was scrolled to. A closed window starts its pages afresh.
//!
//! **A page.** Sections down the page, each one's groups in two columns (a wide section's at full width). Maths is
//! typeset (sheets_typst.rs, compiled on the Typst thread before the page shows) and words are in the app's font;
//! rows are their tallest cell plus 10. On a typeset row the pointer boxes just what a click copies: a cell that
//! copies itself, a conversion's number or unit, a figure's label, else the formula; the copy happens on the press
//! and the box stays, darker, for 0.25 s. The filter under the header (sheet_page.rs `lay_out`) keeps the rows whose
//! words match; it takes the keys once it's clicked, and Esc clears it, then closes the sheet. The pure parts (the
//! filter, layout, targets, pages) are sheet_page.rs.
//!
//! **Cost.** Nothing until a sheet opens: the window class, its own D2D/DWrite factories and fonts, the compiled
//! cells and the text layouts are made then (layouts once per page, never per frame) and dropped when the last sheet
//! closes. Nothing here runs on a keystroke in the bar.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::compat::dpi_for_window as GetDpiForWindow;
use crate::sheet_page::{self as page, Bx, Cell, Docs, Filter, Laid, Pages, Sizes};
use qcalc_core::app::elements::{self, ElementCategory, PeriodicTile};
use qcalc_core::app::identities::{self, IdentitySheet, Shift};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_METRICS, DWRITE_TEXT_RANGE,
    DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CS_DBLCLKS, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DestroyWindow, GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowRect,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, IDC_IBEAM, IsWindow, LoadCursorW, MF_STRING, MINMAXINFO,
    PostMessageW, RegisterClassExW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetCursor, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
    ShowWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CHAR, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NULL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR,
    WM_SETFOCUS, WM_SIZE, WM_SIZING, WM_SYSCHAR, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
    WNDCLASSEXW, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPED,
    WS_SYSMENU, WS_THICKFRAME,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use crate::app::App;
use crate::later;
use crate::place::{self, Area, Give};
use crate::qlog;
use crate::render::{Canvas, Color, Gfx, Rect, Surface, Theme, format, rgb, with_alpha};
use crate::sheets_typst;

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
    Rect::new(
        x,
        y,
        a.right().max(b.right()) - x,
        a.bottom().max(b.bottom()) - y,
    )
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

// ---- the formula sheets' windows ----

/// The formula sheets' one window; Open in New Window adds `sheets-2`, `sheets-3`...
const MAIN: &str = "sheets";
const HINT_COPY: &str = "click to copy";
const HINT_OPEN: &str = "click to open";
/// how long a copy stays boxed (and the header says so)
const COPIED_MS: u32 = 250;

// ---- state ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Tile(usize),
    Formula,
    Part(usize),
    Clear,
    /// a formula sheet's row, by its place in the page's layout
    Row(usize),
    Thumb,
    Back,
    Forward,
    /// the filter field, and its clear button
    Field,
    ClearFilter,
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
    /// 13 math (and a typeset sheet's words, and the filter), 9.5 raised, 8 deeper, 12 semibold section title, 13
    /// semibold heading, 12 `no matches`
    math: IDWriteTextFormat,
    raised: IDWriteTextFormat,
    deeper: IDWriteTextFormat,
    section: IDWriteTextFormat,
    heading: IDWriteTextFormat,
    quiet: IDWriteTextFormat,
}

impl Fonts {
    fn new(gfx: &Gfx) -> windows::core::Result<Fonts> {
        let f = |size: f32, weight: DWRITE_FONT_WEIGHT| {
            format(&gfx.dw, &gfx.fonts.text_family, size, weight)
        };
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
            quiet: f(12.0, DWRITE_FONT_WEIGHT_REGULAR)?,
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

/// One row of a formula sheet, ready to draw: what it typesets and the sizes, its words cells' and note's layouts,
/// and for a row that isn't typeset, its raised and lowered runs.
struct Prepared {
    docs: Docs,
    sizes: Sizes,
    /// per cell, the layout of a words cell
    words: Vec<Option<Text>>,
    /// a note in words (11 pt), when the row has no maths note
    note: Option<Text>,
    /// a plain row: lead cells then the runs, (text, dx, dy) from the row's left and top
    runs: Vec<Run>,
    /// a plain row's lead cells that copy themselves: (right edge, copy)
    cells: Vec<(f32, String)>,
    widths: Vec<f32>,
    /// a table's heading row: muted, not clicked
    heading: bool,
    height: f32,
}

/// The sheet a window shows now, laid out for its filter.
struct Page {
    id: String,
    sheet: IdentitySheet,
    items: HashMap<(usize, usize, usize), Prepared>,
    laid: Laid,
    titles: HashMap<usize, Text>,
    groups: HashMap<(usize, usize), Text>,
    heading: Option<Text>,
    figure: bool,
}

impl Page {
    fn item(&self, si: usize, gi: usize, ii: usize) -> &identities::Identity {
        &self.sheet.sections[si].groups[gi].items[ii]
    }

    /// The content's height and the window's (DIPs), for a monitor `work_h` high.
    fn heights(&self, work_h: f32) -> (f32, f32) {
        let full = self.laid.height + page::TOP;
        let tallest = if self.figure {
            page::MAX_HEIGHT.max(work_h - 8.0)
        } else {
            page::MAX_HEIGHT
        };
        (full, full.min(tallest))
    }
}

/// The words a formula sheet's window draws whatever its page.
struct Chrome {
    placeholder: Option<Text>,
    copy: Option<Text>,
    open: Option<Text>,
    copied: Option<Text>,
    none: Option<Text>,
    tick: Option<Text>,
}

/// A formula sheet's window: its pages, the one up, the filter.
struct Book {
    pages: Pages,
    page: Page,
    chrome: Chrome,
    filter: Filter,
    /// the filter's text as laid out: (text, layout)
    filter_text: Option<(String, Text)>,
    /// a surrogate's first half, typed into the filter
    high: Option<u16>,
    focused: bool,
    scroll: f32,
    /// the row just copied and the box round what it copied
    copied: Option<(usize, Option<Bx>)>,
    /// the box round what a click on the hovered row would copy
    boxed: Option<(usize, Bx)>,
    /// the thumb drag's start: (pointer y, scroll)
    drag: Option<(f32, f32)>,
}

impl Book {
    /// Another page, from its top, with the filter cleared.
    fn set_page(&mut self, p: Page) {
        self.page = p;
        self.filter.clear();
        self.filter_text = None;
        self.copied = None;
        self.boxed = None;
        self.drag = None;
        self.scroll = 0.0;
    }

    /// The filter changed: the rows again, from the top.
    fn refilter(&mut self) {
        let items = &self.page.items;
        self.page.laid = page::lay_out(&self.page.sheet, &self.filter.text, |s, g, i| {
            items.get(&(s, g, i)).map_or(page::ROW, |p| p.height)
        });
        self.copied = None;
        self.boxed = None;
        self.scroll = 0.0;
    }
}

enum Body {
    Periodic(Periodic),
    Identity(Box<Book>),
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
    /// window keys, front last
    order: Vec<String>,
    /// where each window was when it closed (physical px), to open there again
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
    later::free("sheet-open", 0, move || open_periodic_now(at));
}

/// Where a formula sheet goes: the sheets window, the window a row was clicked in, or a new one beside that.
#[derive(Clone, Copy, Debug)]
enum Dest {
    Main,
    In(usize),
    New(usize),
}

/// A formula sheet (`formulas`, `identities`, a part, a named formula) in the sheets window, at a section. Its
/// cells are typeset first, on the Typst thread, the first time it opens.
pub fn open_identities(app: &mut App, id: &str, section: usize) {
    open_page(app, Dest::Main, id, section);
}

fn open_page(app: &mut App, into: Dest, id: &str, section: usize) {
    let Some(sheet) = page::page_sheet(id) else {
        return;
    };
    let at = spot(app);
    let id = id.to_string();
    let docs = sheets_typst::missing(page::sheet_docs(&sheet));
    sheets_typst::compile(app, docs, move || show_page(into, id, section, at));
}

/// A row's line (`beams`) from a sheet window: opened in that window, or (`new`) in one of its own.
fn open_line(from: HWND, line: String, new: bool) {
    let h = from.0 as usize;
    crate::app::poke(move |app| match identities::native_target_for(&line) {
        Some(t) => {
            let into = if new { Dest::New(h) } else { Dest::In(h) };
            open_page(app, into, &t.sheet.id, t.section);
        }
        None => open_periodic(app),
    });
}

fn alive(h: usize) -> Option<HWND> {
    let hwnd = HWND(h as *mut _);
    with(|s| s.open.iter().any(|w| w.hwnd == hwnd))
        .unwrap_or(false)
        .then_some(hwnd)
}

fn window_of(key: &str) -> Option<HWND> {
    with(|s| s.open.iter().find(|w| w.key == key).map(|w| w.hwnd)).flatten()
}

fn show_page(into: Dest, id: String, section: usize, at: Spot) {
    with(|s| s.theme = Some(at.theme));
    let existing = match into {
        Dest::Main => window_of(MAIN),
        Dest::In(h) => alive(h).or_else(|| window_of(MAIN)),
        Dest::New(_) => None,
    };
    if let Some(hwnd) = existing {
        turn(hwnd, Turn::Show(id, section));
        raise(hwnd);
        // the bar's sheet keeps clear of the bar; a row's own window stays where it is
        if matches!(into, Dest::Main) {
            keep_clear(hwnd, at);
        }
        return;
    }
    let (key, from) = match into {
        Dest::New(h) => (next_key(), alive(h)),
        _ => (MAIN.to_string(), None),
    };
    if let Err(e) = create_book(&key, &id, section, at, from) {
        qlog!("sheet {id}: couldn't open: {e}");
        with(|s| s.order.retain(|k| *k != key));
    }
}

fn next_key() -> String {
    let taken: Vec<String> =
        with(|s| s.open.iter().map(|w| w.key.clone()).collect()).unwrap_or_default();
    (2..)
        .map(|n| format!("{MAIN}-{n}"))
        .find(|k| !taken.contains(k))
        .unwrap_or_default()
}

enum Turn {
    /// another sheet, at a section (the same one again only scrolls there)
    Show(String, usize),
    /// back (-1) or forward (1)
    Go(i32),
}

/// A formula sheet window's page changes: the window takes the new page's title and height, keeping its top edge.
/// False when there was nowhere to go.
fn turn(hwnd: HWND, t: Turn) -> bool {
    let out = with(|s| {
        let Sheets { gfx, open, .. } = s;
        let (gfx, fonts) = gfx.as_ref()?;
        let w = open.iter_mut().find(|w| w.hwnd == hwnd)?;
        let ch = client_h(hwnd, w.dpi);
        let Body::Identity(b) = &mut w.body else {
            return None;
        };
        match t {
            Turn::Show(id, section) => {
                if b.pages.show(&id, b.scroll) {
                    b.set_page(build_page(gfx, fonts, &id)?);
                    scroll_to(b, section, ch);
                    return Some(Some(b.page.sheet.title.clone()));
                }
                scroll_to(b, section, ch);
                Some(None)
            }
            Turn::Go(step) => {
                let scrolled = b.pages.go(step, b.scroll)?;
                let id = b.pages.current().to_string();
                b.set_page(build_page(gfx, fonts, &id)?);
                b.scroll = scrolled;
                Some(Some(b.page.sheet.title.clone()))
            }
        }
    })
    .flatten();
    let Some(retitled) = out else {
        return false;
    };
    if let Some(title) = retitled {
        // SAFETY: our window; its WM_SETTEXT goes to DefWindowProc
        let _ = unsafe { SetWindowTextW(hwnd, &HSTRING::from(title.as_str())) };
        fit_page(hwnd);
    }
    invalidate(hwnd);
    true
}

/// Back or forward in a sheet window, from a key, a button or the dev pipe (posted: never inside a borrow).
fn go_later(hwnd: HWND, step: i32) {
    let h = hwnd.0 as usize;
    later::free("sheet-go", 0, move || {
        if let Some(hwnd) = alive(h) {
            turn(hwnd, Turn::Go(step));
        }
    });
}

/// The monitor a window is on.
fn monitor_of(hwnd: HWND) -> crate::window::Monitor {
    let mut r = RECT::default();
    // SAFETY: a live out rect
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    crate::window::monitor_at(POINT {
        x: (r.left + r.right) / 2,
        y: r.top + 8,
    })
}

/// The window takes its page's height, keeping its top edge, and moves up if it would run off the screen.
fn fit_page(hwnd: HWND) {
    let m = monitor_of(hwnd);
    let Some((dpi, h)) = with(|s| {
        let w = s.open.iter().find(|w| w.hwnd == hwnd)?;
        let Body::Identity(b) = &w.body else {
            return None;
        };
        let work_h = m.area.height as f32 * 96.0 / w.dpi as f32;
        Some((w.dpi, b.page.heights(work_h).1))
    })
    .flatten() else {
        return;
    };
    let mut r = RECT::default();
    // SAFETY: our window, a live out rect
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    let (fw, fh) = frame_for(page::width(), h, dpi);
    let bottom = m.area.y + m.area.height;
    let mut top = f64::from(r.top);
    if top + fh > bottom {
        top = (bottom - fh).max(m.area.y);
    }
    // SAFETY: our window
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            r.left,
            top as i32,
            fw as i32,
            fh as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

const STYLE: WINDOW_STYLE =
    WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0);
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
    let _ = crate::compat::adjust_window_rect_for_dpi(&mut r, STYLE, false, EX_STYLE, dpi);
    (f64::from(r.right - r.left), f64::from(r.bottom - r.top))
}

fn open_periodic_now(at: Spot) {
    let key = "periodic".to_string();
    let existing = with(|s| {
        s.theme = Some(at.theme);
        s.open.iter().find(|w| w.key == key).map(|w| w.hwnd)
    })
    .flatten();
    if let Some(hwnd) = existing {
        raise(hwnd);
        keep_clear(hwnd, at);
        invalidate(hwnd);
        return;
    }
    if let Err(e) = create_periodic(at) {
        qlog!("sheet {key}: couldn't open: {e}");
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

/// What WM_CREATE makes the window from: its key and, for a formula sheet, its first page.
struct Made {
    key: String,
    page: Option<Page>,
}

fn make_window(
    made: Made,
    title: &str,
    (x, y, fw, fh): (f64, f64, f64, f64),
) -> windows::core::Result<HWND> {
    let key = made.key.clone();
    with(|s| {
        s.order.retain(|k| *k != key);
        s.order.push(key.clone());
    });
    qlog!("sheet {key}: frame {x:.0},{y:.0} {fw:.0}x{fh:.0}");
    let boxed = Box::new(made);
    // SAFETY: our class; the box is handed to WM_CREATE through lpCreateParams and taken back there
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        CreateWindowExW(
            EX_STYLE,
            CLASS,
            &HSTRING::from(title),
            STYLE,
            x as i32,
            y as i32,
            fw as i32,
            fh as i32,
            None,
            None,
            Some(instance.into()),
            Some(Box::into_raw(boxed) as *const _),
        )
    }
}

fn show_window(hwnd: HWND, dark: bool) {
    set_dark(hwnd, dark);
    // SAFETY: our window, shown without taking the keys
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

/// Centred on a monitor, `step` further down and right (physical px).
fn centred(m: &crate::window::Monitor, fw: f64, fh: f64, step: f64) -> (f64, f64, f64, f64) {
    let (fw, fh) = (fw.min(m.area.width), fh.min(m.area.height));
    let (x, y) = place::fit(
        m.area,
        m.area.x + (m.area.width - fw) / 2.0 + step,
        m.area.y + (m.area.height - fh) / 2.0 + step,
        fw,
        fh,
    );
    (x, y, fw, fh)
}

fn create_periodic(at: Spot) -> windows::core::Result<()> {
    let (w, h) = periodic_size();
    register()?;
    ensure_gfx()?;
    let key = "periodic";
    let earlier = with(|s| s.open.len()).unwrap_or(0) as f64;
    let remembered = with(|s| s.frames.get(key).copied()).flatten();
    let frame = match (at.bar, remembered) {
        (_, Some(r)) => (
            f64::from(r.left),
            f64::from(r.top),
            f64::from(r.right - r.left),
            f64::from(r.bottom - r.top),
        ),
        (Some((bar, area, dpi)), None) => {
            let (fw, fh) = frame_for(w, h, dpi);
            let r = place::beside(
                area,
                bar,
                fw,
                fh,
                place::SHEET_MARGIN * f64::from(dpi) / 96.0,
                Give::Scale(0.6),
            );
            (r.x, r.y, r.width, r.height)
        }
        (None, None) => {
            let m = crate::window::cursor_monitor();
            let (fw, fh) = frame_for(w, h, m.dpi);
            centred(&m, fw, fh, CASCADE * m.scale() * earlier)
        }
    };
    let made = Made {
        key: key.into(),
        page: None,
    };
    let hwnd = make_window(made, "Periodic Table", frame)?;
    // a remembered spot can be under the bar now
    keep_clear(hwnd, at);
    show_window(hwnd, at.theme.dark);
    qlog!("sheet {key}: opened");
    Ok(())
}

fn create_book(
    key: &str,
    id: &str,
    section: usize,
    at: Spot,
    from: Option<HWND>,
) -> windows::core::Result<()> {
    register()?;
    ensure_gfx()?;
    let Some(p) = with(|s| {
        let (g, f) = s.gfx.as_ref()?;
        build_page(g, f, id)
    })
    .flatten() else {
        return Ok(());
    };
    let figure = p.figure;
    let title = p.sheet.title.clone();
    let m = match (from, at.bar) {
        (Some(src), _) => monitor_of(src),
        (None, Some((bar, ..))) => crate::window::monitor_at(POINT {
            x: (bar.x + bar.width / 2.0) as i32,
            y: (bar.y + bar.height / 2.0) as i32,
        }),
        (None, None) => crate::window::cursor_monitor(),
    };
    let (_, h) = p.heights(m.area.height as f32 * 96.0 / m.dpi as f32);
    let (fw, fh) = frame_for(page::width(), h, m.dpi);
    let remembered = with(|s| s.frames.get(key).copied()).flatten();
    let frame = if let Some(src) = from {
        // a little down and right of the window it came from
        let mut r = RECT::default();
        // SAFETY: a live out rect
        let _ = unsafe { GetWindowRect(src, &mut r) };
        let step = CASCADE * m.scale();
        let (x, y) = place::fit(
            m.area,
            f64::from(r.left) + step,
            f64::from(r.top) + step,
            fw,
            fh.min(m.area.height),
        );
        (x, y, fw, fh.min(m.area.height))
    } else if let Some(r) = remembered {
        let (x, y) = place::fit(
            m.area,
            f64::from(r.left),
            f64::from(r.top),
            fw,
            fh.min(m.area.height),
        );
        (x, y, fw, fh.min(m.area.height))
    } else if let (Some((bar, area, dpi)), false) = (at.bar, figure) {
        let least = (p.laid.height + page::TOP).min(220.0);
        let r = place::beside(
            area,
            bar,
            fw,
            fh,
            place::SHEET_MARGIN * f64::from(dpi) / 96.0,
            Give::Height(f64::from(least) * f64::from(dpi) / 96.0),
        );
        (r.x, r.y, r.width, r.height)
    } else {
        // a picture opens at its full size, even over the bar
        centred(&m, fw, fh, 0.0)
    };
    let made = Made {
        key: key.into(),
        page: Some(p),
    };
    let hwnd = make_window(made, &title, frame)?;
    if !figure && from.is_none() {
        keep_clear(hwnd, at);
    }
    with(|s| {
        if let Some(sheet) = s.open.iter_mut().find(|w| w.hwnd == hwnd)
            && let Body::Identity(b) = &mut sheet.body
        {
            scroll_to(b, section, client_h(hwnd, sheet.dpi));
        }
    });
    show_window(hwnd, at.theme.dark);
    qlog!("sheet {key}: opened {id}");
    Ok(())
}

/// A sheet that would cover the bar, or sits off its monitor, moves beside it; one put somewhere clear stays. A
/// picture opens at its full size, even over the bar.
fn keep_clear(hwnd: HWND, at: Spot) {
    let Some((bar, area, dpi)) = at.bar else {
        return;
    };
    let Some(give) = with(|s| {
        let w = s.open.iter().find(|w| w.hwnd == hwnd)?;
        match &w.body {
            Body::Periodic(_) => Some(Give::Scale(0.6)),
            Body::Identity(b) if b.page.figure => None,
            Body::Identity(b) => {
                let least = (b.page.laid.height + page::TOP).min(220.0);
                Some(Give::Height(f64::from(least) * f64::from(dpi) / 96.0))
            }
        }
    })
    .flatten() else {
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
    let p = place::beside(
        area,
        bar,
        now.width,
        now.height,
        place::SHEET_MARGIN * f64::from(dpi) / 96.0,
        give,
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
            crate::compat::dark_attr(),
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

fn invalidate(hwnd: HWND) {
    // SAFETY: our window
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
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
        invalidate(hwnd);
    }
}

/// The front sheet window, and whether it's a formula sheet's.
fn front() -> Option<(HWND, bool)> {
    with(|s| {
        let key = s
            .order
            .iter()
            .rev()
            .find(|k| s.open.iter().any(|w| &w.key == *k))?;
        let w = s.open.iter().find(|w| &w.key == key)?;
        Some((w.hwnd, matches!(w.body, Body::Identity(_))))
    })
    .flatten()
}

/// Ctrl+W: closes the front sheet; false when none is open, so the key goes on to the bar. Safe to call while the
/// app is borrowed (the close is posted).
pub fn close_front() -> bool {
    match front() {
        Some((hwnd, _)) => {
            // SAFETY: our window; WM_CLOSE comes back through the loop
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            true
        }
        None => false,
    }
}

/// Ctrl+[ and Ctrl+] in the bar step back and forward through the front sheet window's pages, when it can go;
/// false leaves the key to the bar. Safe inside the app's borrow (the step is posted).
pub fn bar_key(vk: u32, mods: crate::keys::Mods) -> bool {
    if !mods.ctrl || mods.alt || mods.shift || mods.win {
        return false;
    }
    let step = match vk {
        0xDB => -1,
        0xDD => 1,
        _ => return false,
    };
    let Some((hwnd, true)) = front() else {
        return false;
    };
    let can = with(|s| {
        s.open
            .iter()
            .find(|w| w.hwnd == hwnd)
            .is_some_and(|w| match &w.body {
                Body::Identity(b) if step < 0 => b.pages.can_back(),
                Body::Identity(b) => b.pages.can_forward(),
                Body::Periodic(_) => false,
            })
    })
    .unwrap_or(false);
    if can {
        go_later(hwnd, step);
    }
    can
}

/// The sheets open (for the bench and tests): the table, and each formula window's page.
pub fn open_keys() -> Vec<String> {
    with(|s| {
        s.open
            .iter()
            .map(|w| match &w.body {
                Body::Identity(b) => b.pages.current().to_string(),
                Body::Periodic(_) => w.key.clone(),
            })
            .collect()
    })
    .unwrap_or_default()
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

// ---- a formula sheet's page ----

fn client_h(hwnd: HWND, dpi: u32) -> f32 {
    let mut r = RECT::default();
    // SAFETY: our window
    let _ = unsafe { GetClientRect(hwnd, &mut r) };
    (r.bottom - r.top) as f32 * 96.0 / dpi as f32
}

/// A run of a plain row: its text and where it goes from the row's left and top.
type Run = (Text, f32, f32);

/// A plain row's runs (the web's own look, for a row with no Typst): lead cells in their columns, then the row.
fn plain_runs(
    gfx: &Gfx,
    fonts: &Fonts,
    item: &identities::Identity,
    widths: &[f32],
) -> (Vec<Run>, Vec<(f32, String)>) {
    let main = text(gfx, "Ag", &fonts.math);
    let (main_h, main_base) = main.map_or((17.0, 13.0), |t| (t.h, t.baseline));
    let base_y = (page::ROW - main_h) / 2.0 + main_base;
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
    let mut cells = Vec::new();
    for (i, cell) in item.lead.as_deref().unwrap_or(&[]).iter().enumerate() {
        let mut cx = x;
        put(cell, &mut cx);
        x += widths.get(i).or(widths.last()).copied().unwrap_or_default();
        if let Some(copy) = item.lead_copy.as_ref().and_then(|c| c.get(i))
            && !copy.is_empty()
        {
            cells.push((x, copy.clone()));
        }
    }
    put(&item.show, &mut x);
    (runs, cells)
}

fn prepare(gfx: &Gfx, fonts: &Fonts, item: &identities::Identity, widths: &[f32]) -> Prepared {
    let docs = page::docs(item);
    let mut sizes = Sizes::default();
    let mut words = Vec::new();
    for cell in &docs.cells {
        match cell {
            Cell::Words(w) => {
                let t = text(gfx, w, &fonts.math);
                sizes
                    .cells
                    .push(t.as_ref().map_or((0.0, 0.0), |t| (t.w, t.h)));
                words.push(t);
            }
            Cell::Doc(d) => {
                sizes.cells.push(sheets_typst::size(d).unwrap_or_default());
                words.push(None);
            }
        }
    }
    sizes.parts = docs
        .parts
        .iter()
        .map(|(d, _)| sheets_typst::size(d).unwrap_or_default())
        .collect();
    sizes.spots = docs
        .spots
        .iter()
        .map(|s| sheets_typst::size(&s.2).unwrap_or_default())
        .collect();
    let maths_note = docs
        .note
        .as_deref()
        .is_some_and(|d| sheets_typst::size(d).is_some());
    let note = (!maths_note)
        .then(|| item.note.as_deref().filter(|n| !n.is_empty()))
        .flatten()
        .and_then(|n| text(gfx, n, &fonts.caption));
    let (runs, cells) = if docs.typeset() {
        (Vec::new(), Vec::new())
    } else {
        plain_runs(gfx, fonts, item, widths)
    };
    let height = if docs.typeset() {
        page::row_height(&sizes)
    } else {
        page::ROW
    };
    Prepared {
        heading: item.copy.is_empty() && !docs.figure,
        docs,
        sizes,
        words,
        note,
        runs,
        cells,
        widths: widths.to_vec(),
        height,
    }
}

/// A sheet's page, from its compiled cells (sheets_typst.rs): its rows' layouts and where they go.
fn build_page(gfx: &Gfx, fonts: &Fonts, id: &str) -> Option<Page> {
    let sheet = page::page_sheet(id)?;
    let mut items = HashMap::new();
    let mut titles = HashMap::new();
    let mut groups = HashMap::new();
    for (si, section) in sheet.sections.iter().enumerate() {
        let widths = page::lead_widths(section);
        if let Some(t) = text(gfx, &section.title, &fonts.section) {
            titles.insert(si, t);
        }
        for (gi, group) in section.groups.iter().enumerate() {
            if let Some(t) = text(gfx, &group.title, &fonts.caption_medium) {
                groups.insert((si, gi), t);
            }
            for (ii, item) in group.items.iter().enumerate() {
                items.insert((si, gi, ii), prepare(gfx, fonts, item, &widths));
            }
        }
    }
    let laid = page::lay_out(&sheet, "", |s, g, i| {
        items.get(&(s, g, i)).map_or(page::ROW, |p| p.height)
    });
    Some(Page {
        id: id.to_string(),
        heading: text(gfx, &sheet.title, &fonts.heading),
        figure: page::has_figure(&sheet),
        sheet,
        items,
        laid,
        titles,
        groups,
    })
}

fn chrome(gfx: &Gfx, fonts: &Fonts) -> Chrome {
    Chrome {
        placeholder: text(gfx, "Filter", &fonts.math),
        copy: text(gfx, HINT_COPY, &fonts.caption),
        open: text(gfx, HINT_OPEN, &fonts.caption),
        copied: text(gfx, "✓ copied", &fonts.caption),
        none: text(gfx, "no matches", &fonts.quiet),
        tick: text(gfx, "✓", &fonts.caption),
    }
}

fn scroll_to(b: &mut Book, section: usize, client_h: f32) {
    // a page that shows only its picture has the one section
    let section = if b.page.figure { 0 } else { section };
    if let Some(&(_, top)) = b.page.laid.tops.iter().find(|(s, _)| *s == section) {
        let most = (b.page.laid.height - (client_h - page::TOP)).max(0.0);
        b.scroll = top.min(most);
    }
}

// ---- the window ----

fn build(made: Made, hwnd: HWND, dpi: u32) -> Option<Sheet> {
    let body = match made.page {
        None => Body::Periodic(Periodic {
            tiles: elements::periodic_tiles(),
            scale: 1.0,
            ..Periodic::default()
        }),
        Some(p) => {
            let chrome = with(|s| s.gfx.as_ref().map(|(g, f)| chrome(g, f))).flatten()?;
            Body::Identity(Box::new(Book {
                pages: Pages::new(&p.id),
                page: p,
                chrome,
                filter: Filter::default(),
                filter_text: None,
                high: None,
                focused: false,
                scroll: 0.0,
                copied: None,
                boxed: None,
                drag: None,
            }))
        }
    };
    Some(Sheet {
        hwnd,
        key: made.key,
        surface: Surface::new_flip(hwnd, dpi),
        dpi,
        body,
        hover: None,
        pressed: None,
        tracking: false,
    })
}

fn mods() -> crate::keys::Mods {
    // SAFETY: plain key state queries
    let down = |vk: i32| unsafe { GetKeyState(vk) } < 0;
    crate::keys::Mods {
        ctrl: down(0x11),
        alt: down(0x12),
        shift: down(0x10),
        win: down(0x5B) || down(0x5C),
    }
}

unsafe extern "system" fn sheet_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: lParam is our CREATESTRUCTW; its params are the box from make_window()
            let made = unsafe {
                let cs = &*(lp.0 as *const CREATESTRUCTW);
                *Box::from_raw(cs.lpCreateParams as *mut Made)
            };
            let dpi = GetDpiForWindow(hwnd).max(96);
            if let Some(sheet) = build(made, hwnd, dpi) {
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
            invalidate(hwnd);
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
                SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
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
                match &mut s.body {
                    Body::Periodic(p) => {
                        p.hover_tile = None;
                        p.detail = None;
                    }
                    Body::Identity(b) => b.boxed = None,
                }
            });
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN => {
            let (x, y) = point(lp);
            let right = msg == WM_RBUTTONDOWN;
            let hit = on_sheet(hwnd, |s, _| {
                let hit = hit_test(s, x, y);
                s.pressed = hit;
                let dy = y as f32 * 96.0 / s.dpi as f32;
                if let (Some(Hit::Thumb), Body::Identity(b)) = (hit, &mut s.body) {
                    b.drag = Some((dy, b.scroll));
                }
                hit
            })
            .flatten();
            raise(hwnd);
            match hit {
                // the filter takes the keys once it's clicked
                Some(Hit::Field) if !right => {
                    // SAFETY: our window, brought forward by the user's click
                    unsafe {
                        let _ = SetForegroundWindow(hwnd);
                        let _ = SetFocus(Some(hwnd));
                    }
                    let at = on_sheet(hwnd, |s, _| design(s, x, y).0).unwrap_or(0.0);
                    field_click(hwnd, at);
                }
                // a typeset row copies the moment it's pressed, and shows it then
                Some(Hit::Row(i)) if !right => {
                    let (dx, dy) = on_sheet(hwnd, |s, _| design(s, x, y)).unwrap_or_default();
                    if press_row(hwnd, i, dx, dy) {
                        on_sheet(hwnd, |s, _| s.pressed = None);
                    }
                }
                _ => {}
            }
            if hit.is_some() || right {
                // SAFETY: our window takes the mouse until the button comes up
                unsafe { SetCapture(hwnd) };
            }
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let (x, y) = point(lp);
            // SAFETY: we may hold the capture
            let _ = unsafe { ReleaseCapture() };
            let right = msg == WM_RBUTTONUP;
            let act = on_sheet(hwnd, |s, _| {
                let pressed = s.pressed.take();
                if let Body::Identity(b) = &mut s.body {
                    b.drag = None;
                }
                let now = hit_test(s, x, y);
                (pressed.is_some() && pressed == now)
                    .then_some(now)
                    .flatten()
            })
            .flatten();
            if let Some(hit) = act {
                let click_x = on_sheet(hwnd, |s, _| design(s, x, y).0).unwrap_or(0.0);
                act_on(hwnd, hit, right, click_x);
            }
            invalidate(hwnd);
            return LRESULT(0);
        }
        // the mouse's back and forward buttons
        WM_XBUTTONDOWN => return LRESULT(1),
        WM_XBUTTONUP => {
            let step = if (wp.0 >> 16) & 0xffff == 1 { -1 } else { 1 };
            go_later(hwnd, step);
            return LRESULT(1);
        }
        WM_MOUSEWHEEL => {
            let delta = f32::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
            scroll_by(hwnd, -delta / 120.0 * 3.0 * page::ROW);
            return LRESULT(0);
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            on_sheet(hwnd, |s, _| {
                if let Body::Identity(b) = &mut s.body {
                    b.focused = msg == WM_SETFOCUS;
                }
            });
            invalidate(hwnd);
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if key_down(hwnd, wp.0 as u32, mods()) {
                return LRESULT(0);
            }
        }
        WM_CHAR => {
            typed(hwnd, wp.0 as u16);
            return LRESULT(0);
        }
        // Alt+← and Alt+→ aren't letters: no beep
        WM_SYSCHAR if matches!(wp.0 as u32, 0x25 | 0x27) => return LRESULT(0),
        WM_SETCURSOR => {
            if (lp.0 & 0xffff) as u32 == 1 {
                let (over, field) = on_sheet(hwnd, |s, _| {
                    let over = match (s.hover, &s.body) {
                        (Some(Hit::Row(i)), Body::Identity(b)) => clickable(b, i),
                        (Some(Hit::Back), Body::Identity(b)) => b.pages.can_back(),
                        (Some(Hit::Forward), Body::Identity(b)) => b.pages.can_forward(),
                        (Some(Hit::Field), _) => false,
                        (h, _) => h.is_some(),
                    };
                    (over, s.hover == Some(Hit::Field))
                })
                .unwrap_or((false, false));
                let cursor = if field {
                    IDC_IBEAM
                } else if over {
                    IDC_HAND
                } else {
                    IDC_ARROW
                };
                // SAFETY: system cursors
                unsafe {
                    if let Ok(c) = LoadCursorW(None, cursor) {
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
            let last = with(|s| {
                if let Some(i) = s.open.iter().position(|w| w.hwnd == hwnd) {
                    let w = s.open.remove(i);
                    s.order.retain(|k| *k != w.key);
                    qlog!("sheet {}: closed", w.key);
                }
                // the last one gone: its factories and fonts go too
                if s.open.is_empty() {
                    s.gfx = None;
                }
                s.open.is_empty()
            });
            if last == Some(true) {
                sheets_typst::forget();
            }
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

fn on_book<R>(hwnd: HWND, f: impl FnOnce(&mut Book, f32) -> R) -> Option<R> {
    on_sheet(hwnd, |s, _| {
        let ch = client_h(hwnd, s.dpi);
        match &mut s.body {
            Body::Identity(b) => Some(f(b, ch)),
            Body::Periodic(_) => None,
        }
    })
    .flatten()
}

fn scroll_by(hwnd: HWND, dy: f32) {
    on_book(hwnd, |b, ch| {
        let most = (b.page.laid.height - (ch - page::TOP)).max(0.0);
        b.scroll = (b.scroll + dy).clamp(0.0, most);
        b.boxed = None;
    });
    invalidate(hwnd);
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

/// The least and most a sheet's frame may be, physical px: the table keeps its shape, a formula sheet only
/// changes height.
fn limits(hwnd: HWND) -> Option<(POINT, POINT)> {
    let (least, most, dpi) = with(|s| {
        let w = s.open.iter().find(|w| w.hwnd == hwnd)?;
        Some(match &w.body {
            Body::Periodic(_) => {
                let (w0, h0) = periodic_size();
                ((w0 * 0.6, h0 * 0.6), (w0 * 2.5, h0 * 2.5), w.dpi)
            }
            Body::Identity(b) => {
                let full = b.page.laid.height + page::TOP;
                let wide = page::width();
                ((wide, full.min(220.0)), (wide, full), w.dpi)
            }
        })
    })
    .flatten()?;
    let (a, b) = (
        frame_for(least.0, least.1, dpi),
        frame_for(most.0, most.1, dpi),
    );
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
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
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

// the header's back and forward buttons, and the filter field (DIPs)
const BACK: Rect = Rect::new(10.0, 5.0, 20.0, 20.0);
const FORWARD: Rect = Rect::new(32.0, 5.0, 20.0, 20.0);

fn field_rect() -> Rect {
    Rect::new(
        page::INSET,
        page::FILTER_Y,
        page::width() - page::INSET * 2.0,
        page::FILTER_H,
    )
}

fn clear_rect() -> Rect {
    let f = field_rect();
    Rect::new(f.right() - 22.0, f.y, 22.0, f.h)
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
        Body::Identity(b) => {
            let h = client_h(s.hwnd, s.dpi);
            if let Some(thumb) = thumb_rect(b, h)
                && thumb.inset(-4.0, 0.0).contains(x, y)
            {
                return Some(Hit::Thumb);
            }
            if y < page::TOP {
                if b.pages.list.len() > 1 {
                    if BACK.contains(x, y) {
                        return Some(Hit::Back);
                    }
                    if FORWARD.contains(x, y) {
                        return Some(Hit::Forward);
                    }
                }
                if !b.filter.text.is_empty() && clear_rect().contains(x, y) {
                    return Some(Hit::ClearFilter);
                }
                return field_rect().contains(x, y).then_some(Hit::Field);
            }
            let cy = y - page::TOP + b.scroll;
            b.page
                .laid
                .rows
                .iter()
                .position(|r| r.0.contains(x, cy))
                .map(Hit::Row)
        }
    }
}

/// A row a click does something on (a table's heading isn't).
fn clickable(b: &Book, i: usize) -> bool {
    let Some(&(_, si, gi, ii)) = b.page.laid.rows.get(i) else {
        return false;
    };
    let Some(p) = b.page.items.get(&(si, gi, ii)) else {
        return false;
    };
    if p.docs.typeset() {
        return b.boxed.is_some_and(|(r, _)| r == i);
    }
    !p.heading
}

/// What a point on a row (design DIPs, window coordinates) boxes and copies, on a typeset row.
fn row_target(b: &Book, i: usize, x: f32, y: f32) -> Option<(Bx, String)> {
    let &(rect, si, gi, ii) = b.page.laid.rows.get(i)?;
    let p = b.page.items.get(&(si, gi, ii))?;
    if !p.docs.typeset() {
        return None;
    }
    let item = b.page.item(si, gi, ii);
    let cy = y - page::TOP + b.scroll;
    page::target(
        item,
        &p.docs,
        &p.sizes,
        &p.widths,
        (rect.w, rect.h),
        (x - rect.x, cy - rect.y),
    )
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
        if let Body::Identity(b) = &mut s.body
            && let Some((y0, scroll0)) = b.drag
        {
            let view = client_h(hwnd, dpi) - page::TOP;
            let most = (b.page.laid.height - view).max(0.0);
            let dy = y as f32 * 96.0 / dpi as f32;
            let k = if view > 0.0 {
                b.page.laid.height / view
            } else {
                1.0
            };
            b.scroll = (scroll0 + (dy - y0) * k).clamp(0.0, most);
            return true;
        }
        let hit = hit_test(s, x, y);
        let mut changed = hit != s.hover;
        s.hover = hit;
        let (dx, dy) = design(s, x, y);
        match &mut s.body {
            Body::Periodic(p) => {
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
            Body::Identity(b) => {
                // a typeset row boxes just the text a click there would copy
                let boxed = match hit {
                    Some(Hit::Row(i)) => row_target(b, i, dx, dy).map(|(bx, _)| (i, bx)),
                    _ => None,
                };
                if boxed != b.boxed {
                    b.boxed = boxed;
                    changed = true;
                }
            }
        }
        changed
    })
    .unwrap_or(false);
    if changed {
        invalidate(hwnd);
    }
}

/// A press on a row: a typeset row (one that doesn't open a sheet) copies now. True when it did.
fn press_row(hwnd: HWND, i: usize, x: f32, y: f32) -> bool {
    let copy = on_book(hwnd, |b, _| {
        let &(_, si, gi, ii) = b.page.laid.rows.get(i)?;
        let p = b.page.items.get(&(si, gi, ii))?;
        if !p.docs.typeset() || b.page.item(si, gi, ii).open.is_some() {
            return None;
        }
        let (bx, copy) = row_target(b, i, x, y)?;
        b.copied = Some((i, Some(bx)));
        b.boxed = Some((i, bx));
        Some(copy)
    })
    .flatten();
    match copy {
        Some(copy) => {
            copied(hwnd, &copy);
            true
        }
        None => false,
    }
}

/// `copy` goes on the clipboard; the row and the header say so for a moment.
fn copied(hwnd: HWND, copy: &str) {
    if !crate::clip::set(hwnd, copy) {
        qlog!("sheet: couldn't copy a row");
    }
    invalidate(hwnd);
    let target = hwnd.0 as usize;
    later::free("sheet-copied", COPIED_MS, move || {
        let hwnd = HWND(target as *mut _);
        // SAFETY: a plain check that the window is still ours to touch
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return;
        }
        on_book(hwnd, |b, _| b.copied = None);
        invalidate(hwnd);
    });
}

/// A click (or right click) that went down and up on the same thing.
fn act_on(hwnd: HWND, hit: Hit, right: bool, x: f32) {
    match hit {
        Hit::Tile(i) => {
            with(|s| {
                let symbol = s
                    .open
                    .iter()
                    .find(|w| w.hwnd == hwnd)
                    .and_then(|w| match &w.body {
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
        Hit::Row(i) if right => row_menu(hwnd, i),
        Hit::Row(i) => click_row(hwnd, i, x),
        Hit::Back if !right => go_later(hwnd, -1),
        Hit::Forward if !right => go_later(hwnd, 1),
        Hit::ClearFilter if !right => set_filter(hwnd, None),
        _ => {}
    }
}

/// A row's click, `x` in the sheet's own units: opens what the row names (in this window), else copies the cell
/// under it (a plain row; a typeset one copied on the press).
fn click_row(hwnd: HWND, i: usize, x: f32) {
    let act = on_book(hwnd, |b, _| {
        let &(rect, si, gi, ii) = b.page.laid.rows.get(i)?;
        let item = b.page.item(si, gi, ii);
        if let Some(line) = &item.open {
            return Some(Err(line.clone()));
        }
        let p = b.page.items.get(&(si, gi, ii))?;
        if p.docs.typeset() || p.heading {
            return None;
        }
        b.copied = Some((i, None));
        let at = x - rect.x;
        Some(Ok(p
            .cells
            .iter()
            .find(|(right, _)| at < *right)
            .map_or_else(|| item.copy.clone(), |(_, copy)| copy.clone())))
    })
    .flatten();
    match act {
        Some(Err(line)) => open_line(hwnd, line, false),
        Some(Ok(copy)) if !copy.is_empty() => copied(hwnd, &copy),
        _ => {}
    }
}

/// A right click on a row that opens a sheet: its menu's Open in New Window.
fn row_menu(hwnd: HWND, i: usize) {
    let Some(line) = on_book(hwnd, |b, _| {
        let &(_, si, gi, ii) = b.page.laid.rows.get(i)?;
        b.page.item(si, gi, ii).open.clone()
    })
    .flatten() else {
        return;
    };
    let dark = with(|s| s.theme.map(|t| t.dark)).flatten().unwrap_or(false);
    crate::settings_window::dark_menus(dark);
    let mut pt = POINT::default();
    // SAFETY: a menu of our own, shown and destroyed here; the window comes forward so the menu goes when the user
    // clicks away (TrackPopupMenu's remarks)
    let picked = unsafe {
        let _ = GetCursorPos(&mut pt);
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("Open in New Window"));
        let _ = SetForegroundWindow(hwnd);
        let picked = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        picked.0
    };
    if picked == 1 {
        open_line(hwnd, line, true);
    }
}

/// A click in the filter field puts the caret nearest it.
fn field_click(hwnd: HWND, x: f32) {
    on_book(hwnd, |b, _| {
        b.filter.all = false;
        let Some((_, t)) = &b.filter_text else {
            b.filter.end();
            return;
        };
        let at = crate::render::hit(&t.layout, x - field_rect().x - 26.0, 4.0) as usize;
        // UTF-16 to a byte offset
        let mut units = 0;
        b.filter.caret = b
            .filter
            .text
            .char_indices()
            .find(|(_, c)| {
                let here = units >= at;
                units += c.len_utf16();
                here
            })
            .map_or(b.filter.text.len(), |(i, _)| i);
    });
    invalidate(hwnd);
}

/// The filter, as if typed (the dev pipe's `sheetfilter:`), or cleared.
fn set_filter(hwnd: HWND, text: Option<&str>) {
    on_book(hwnd, |b, _| {
        match text {
            Some(t) => b.filter.set(t),
            None => b.filter.clear(),
        }
        b.filter_text = None;
        b.refilter();
    });
    invalidate(hwnd);
}

/// A key while the sheet has the keys: the filter's, Esc, back and forward, Ctrl+W. True when taken.
fn key_down(hwnd: HWND, vk: u32, m: crate::keys::Mods) -> bool {
    const BACK_KEY: u32 = 0x08;
    const ESC: u32 = 0x1B;
    const PRIOR: u32 = 0x21;
    const NEXT: u32 = 0x22;
    const END: u32 = 0x23;
    const HOME: u32 = 0x24;
    const LEFT: u32 = 0x25;
    const UP: u32 = 0x26;
    const RIGHT: u32 = 0x27;
    const DOWN: u32 = 0x28;
    const DELETE: u32 = 0x2E;
    let ctrl = m.ctrl && !m.alt;
    let plain = !m.ctrl && !m.alt && !m.win;
    // back and forward: Alt+← / Alt+→, Ctrl+[ / Ctrl+]
    let step = match vk {
        LEFT if m.alt && !m.ctrl => Some(-1),
        RIGHT if m.alt && !m.ctrl => Some(1),
        0xDB if ctrl => Some(-1),
        0xDD if ctrl => Some(1),
        _ => None,
    };
    if let Some(step) = step {
        go_later(hwnd, step);
        return true;
    }
    if ctrl && vk == 0x57 {
        // SAFETY: our window; WM_CLOSE comes back through the loop
        let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        return true;
    }
    if m.alt {
        return false;
    }
    let paste = (ctrl && vk == 0x56)
        .then(|| crate::clip::get(hwnd))
        .flatten();
    let page_h = on_sheet(hwnd, |s, _| client_h(hwnd, s.dpi) - page::TOP).unwrap_or(0.0);
    let edit = on_book(hwnd, |b, _| {
        let before = b.filter.text.clone();
        match vk {
            ESC if plain => {
                if b.filter.text.is_empty() {
                    return Some(true);
                }
                b.filter.clear();
            }
            BACK_KEY => b.filter.back(ctrl),
            DELETE => b.filter.delete(),
            LEFT => b.filter.left(),
            RIGHT => b.filter.right(),
            HOME if !ctrl => b.filter.home(),
            END if !ctrl => b.filter.end(),
            0x41 if ctrl => b.filter.select_all(),
            0x56 if ctrl => {
                if let Some(p) = &paste {
                    b.filter.insert(p.lines().next().unwrap_or(""));
                }
            }
            UP | DOWN | PRIOR | NEXT | HOME | END => {
                let most = (b.page.laid.height - page_h).max(0.0);
                b.scroll = match vk {
                    UP => b.scroll - 3.0 * page::ROW,
                    DOWN => b.scroll + 3.0 * page::ROW,
                    PRIOR => b.scroll - page_h,
                    NEXT => b.scroll + page_h,
                    HOME => 0.0,
                    _ => most,
                }
                .clamp(0.0, most);
                b.boxed = None;
            }
            _ => return None,
        }
        if b.filter.text != before {
            b.filter_text = None;
            b.refilter();
        }
        Some(false)
    })
    .flatten();
    match edit {
        // Esc on an empty filter closes the sheet
        Some(true) => {
            // SAFETY: our window; WM_CLOSE comes back through the loop
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            true
        }
        Some(false) => {
            invalidate(hwnd);
            true
        }
        None => false,
    }
}

/// Text typed while the sheet has the keys goes in the filter, which filters live.
fn typed(hwnd: HWND, unit: u16) {
    on_book(hwnd, |b, _| {
        let c = match unit {
            0xD800..=0xDBFF => {
                b.high = Some(unit);
                return;
            }
            0xDC00..=0xDFFF => {
                let Some(high) = b.high.take() else { return };
                char::decode_utf16([high, unit]).next().and_then(Result::ok)
            }
            u if u < 0x20 || u == 0x7F => None,
            u => char::from_u32(u32::from(u)),
        };
        if let Some(c) = c {
            b.filter.insert(&c.to_string());
            b.filter_text = None;
            b.refilter();
        }
    });
    invalidate(hwnd);
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
        match body {
            Body::Periodic(p) => prepare_periodic(p, gfx, fonts, &picks),
            Body::Identity(b) => prepare_filter(b, gfx, fonts),
        }
        let ch = client_h(hwnd, dpi);
        let result = surface.draw(gfx, &theme, |c| {
            c.clear(theme.bg);
            match body {
                Body::Periodic(p) => paint_periodic(c, fonts, p, &picks, hover, pressed),
                Body::Identity(b) => paint_book(c, b, ch, hover, pressed),
            }
        });
        if let Err(e) = result {
            qlog!("sheet paint failed: {e}");
        }
    });
}

/// The filter's text, laid out once per change.
fn prepare_filter(b: &mut Book, gfx: &Gfx, fonts: &Fonts) {
    if b.filter.text.is_empty() {
        b.filter_text = None;
        return;
    }
    if b.filter_text
        .as_ref()
        .is_none_or(|(t, _)| *t != b.filter.text)
    {
        b.filter_text = text(gfx, &b.filter.text, &fonts.math).map(|t| (b.filter.text.clone(), t));
    }
}

/// A rounded box's outline.
fn stroke_round(c: &Canvas, r: Rect, radius: f32, color: Color) {
    let rr = D2D1_ROUNDED_RECT {
        rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
            left: r.x + 0.5,
            top: r.y + 0.5,
            right: r.right() - 0.5,
            bottom: r.bottom() - 0.5,
        },
        radiusX: radius,
        radiusY: radius,
    };
    // SAFETY: inside the frame's BeginDraw/EndDraw
    unsafe { c.rt().DrawRoundedRectangle(&rr, c.brush(color), 1.0, None) };
}

fn rect_of(b: Bx) -> Rect {
    Rect::new(b.x, b.y, b.w, b.h)
}

/// The scrollbar's thumb, while the content overflows (DIPs, window coordinates).
fn thumb_rect(b: &Book, client_h: f32) -> Option<Rect> {
    let view = client_h - page::TOP;
    let height = b.page.laid.height;
    if height <= view + 0.5 || view <= 0.0 {
        return None;
    }
    let h = (view * view / height).max(24.0);
    let most = height - view;
    let y = page::TOP + (view - h) * (b.scroll / most).clamp(0.0, 1.0);
    Some(Rect::new(page::width() - 9.0, y + 2.0, 6.0, h - 4.0))
}

/// A chevron, pointing left (`-1`) or right, in the middle of `r`.
fn chevron(c: &Canvas, r: Rect, dir: f32, color: Color) {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    c.line(cx - 2.5 * dir, cy - 5.0, cx + 2.5 * dir, cy, 1.6, color);
    c.line(cx + 2.5 * dir, cy, cx - 2.5 * dir, cy + 5.0, 1.6, color);
}

fn paint_header(c: &Canvas, b: &Book, hover: Option<Hit>) {
    let t = c.theme;
    let width = page::width();
    // the title right of the middle's left, the status left of its right
    if let Some(h) = &b.page.heading {
        h.draw(c, width / 2.0 - 4.0 - h.w, 7.0, t.ink);
    }
    let status = if b.copied.is_some() {
        &b.chrome.copied
    } else if b.page.laid.opens {
        &b.chrome.open
    } else {
        &b.chrome.copy
    };
    if let Some(s) = status {
        s.draw(c, width / 2.0 + 4.0, 9.0, t.muted);
    }
    // back and forward, once there's somewhere to have gone
    if b.pages.list.len() > 1 {
        for (r, dir, can, hit) in [
            (BACK, -1.0, b.pages.can_back(), Hit::Back),
            (FORWARD, 1.0, b.pages.can_forward(), Hit::Forward),
        ] {
            if can && hover == Some(hit) {
                c.fill_round(r, 5.0, t.fill_hover);
            }
            let ink = if can {
                t.muted
            } else {
                with_alpha(t.muted, 0.35)
            };
            chevron(c, r, dir, ink);
        }
    }
    // the filter: a magnifier, the text or `Filter`, the caret while it has the keys, ⓧ to clear
    let f = field_rect();
    c.fill_round(f, 6.0, t.fill_hover);
    stroke_round(
        c,
        f,
        6.0,
        if b.focused {
            with_alpha(t.accent, 0.8)
        } else {
            t.line
        },
    );
    let (gx, gy) = (f.x + 12.0, f.y + f.h / 2.0 - 1.0);
    let glass = D2D1_ROUNDED_RECT {
        rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
            left: gx - 4.0,
            top: gy - 4.0,
            right: gx + 4.0,
            bottom: gy + 4.0,
        },
        radiusX: 4.0,
        radiusY: 4.0,
    };
    // SAFETY: inside the frame
    unsafe {
        c.rt()
            .DrawRoundedRectangle(&glass, c.brush(t.muted), 1.3, None)
    };
    c.line(gx + 2.9, gy + 2.9, gx + 6.0, gy + 6.0, 1.4, t.muted);
    let tx = f.x + 26.0;
    match &b.filter_text {
        Some((_, text)) => {
            let ty = f.y + (f.h - text.h) / 2.0;
            if b.filter.all {
                c.fill(Rect::new(tx, ty, text.w, text.h), t.selection);
            }
            c.push_clip(Rect::new(f.x, f.y, f.w - 24.0, f.h));
            text.draw(c, tx, ty, t.ink);
            c.pop_clip();
            if b.focused && !b.filter.all {
                let units = b.filter.text[..b.filter.caret].encode_utf16().count() as u32;
                let x = tx + crate::render::caret_x(&text.layout, units);
                c.fill(Rect::new(x.round(), ty + 1.0, 1.0, text.h - 2.0), t.ink);
            }
            // ⓧ
            let r = clear_rect();
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            let tone = if hover == Some(Hit::ClearFilter) {
                0.65
            } else {
                0.45
            };
            c.fill_round(
                Rect::new(cx - 7.0, cy - 7.0, 14.0, 14.0),
                7.0,
                with_alpha(t.muted, tone),
            );
            c.line(cx - 3.0, cy - 3.0, cx + 3.0, cy + 3.0, 1.4, t.bg);
            c.line(cx - 3.0, cy + 3.0, cx + 3.0, cy - 3.0, 1.4, t.bg);
        }
        None => {
            if let Some(p) = &b.chrome.placeholder {
                p.draw(c, tx, f.y + (f.h - p.h) / 2.0, t.muted);
            }
            if b.focused {
                let h = b.chrome.placeholder.as_ref().map_or(16.0, |p| p.h);
                c.fill(
                    Rect::new(tx, f.y + (f.h - h) / 2.0 + 1.0, 1.0, h - 2.0),
                    t.ink,
                );
            }
        }
    }
}

fn paint_book(c: &Canvas, b: &mut Book, client_h: f32, hover: Option<Hit>, pressed: Option<Hit>) {
    let t = c.theme;
    let width = page::width();
    let view = client_h - page::TOP;
    let most = (b.page.laid.height - view).max(0.0);
    b.scroll = b.scroll.clamp(0.0, most);
    paint_header(c, b, hover);
    c.push_clip(Rect::new(0.0, page::TOP, width, view.max(0.0)));
    let dy = page::TOP - b.scroll;
    let visible = |y: f32, h: f32| y + dy + h >= page::TOP && y + dy <= client_h;
    for &(y, si) in &b.page.laid.titles {
        if visible(y, page::SECTION_TITLE)
            && let Some(title) = b.page.titles.get(&si)
        {
            title.draw(c, page::INSET, y + dy + 4.0, t.ink);
            c.fill(
                Rect::new(
                    page::INSET,
                    y + dy + page::SECTION_TITLE - 3.0,
                    width - page::INSET * 2.0,
                    1.0,
                ),
                t.line,
            );
        }
    }
    for &(x, y, si, gi) in &b.page.laid.groups {
        if visible(y, 16.0)
            && let Some(title) = b.page.groups.get(&(si, gi))
        {
            title.draw(c, x, y + dy, t.muted);
        }
    }
    for (i, &(rect, si, gi, ii)) in b.page.laid.rows.iter().enumerate() {
        if !visible(rect.y, rect.h) {
            continue;
        }
        let Some(p) = b.page.items.get(&(si, gi, ii)) else {
            continue;
        };
        let r = Rect::new(rect.x, rect.y + dy, rect.w, rect.h);
        let copied = b.copied.is_some_and(|(row, _)| row == i);
        if p.docs.typeset() {
            let boxed = b.boxed.filter(|(row, _)| *row == i).map(|(_, bx)| bx);
            let shown = if copied {
                b.copied.and_then(|(_, bx)| bx).or(boxed)
            } else {
                boxed
            };
            paint_typeset(c, b, p, r, shown, copied, pressed == Some(Hit::Row(i)));
        } else {
            paint_plain(
                c,
                b,
                p,
                r,
                copied,
                hover == Some(Hit::Row(i)),
                pressed == Some(Hit::Row(i)),
            );
        }
    }
    if let (Some(y), Some(none)) = (b.page.laid.none, &b.chrome.none) {
        none.draw(c, (width - none.w) / 2.0, y + dy, with_alpha(t.muted, 0.6));
    }
    c.pop_clip();
    if let Some(th) = thumb_rect(b, client_h) {
        let a = if hover == Some(Hit::Thumb) || b.drag.is_some() {
            0.45
        } else {
            0.25
        };
        c.fill_round(th, 3.0, with_alpha(t.ink, a));
    }
}

/// A typeset row: the box round what a click copies, each cell centred on the row's height, the note at the right.
fn paint_typeset(
    c: &Canvas,
    b: &Book,
    p: &Prepared,
    r: Rect,
    shown: Option<Bx>,
    copied: bool,
    pressed: bool,
) {
    let t = c.theme;
    if let Some(bx) = shown {
        let bx = rect_of(Bx {
            x: bx.x + r.x,
            y: bx.y + r.y,
            ..bx
        });
        c.fill_round(
            bx,
            4.0,
            with_alpha(t.ink, if copied || pressed { 0.12 } else { 0.05 }),
        );
        stroke_round(
            c,
            bx,
            4.0,
            with_alpha(t.ink, if copied { 0.6 } else { 0.3 }),
        );
    }
    let docs = &p.docs;
    if docs.figure {
        let (ox, oy) = page::figure_origin(&p.sizes, (r.w, r.h));
        let (ox, oy) = (r.x + ox, r.y + oy);
        if let Some(Cell::Doc(d)) = docs.cells.first() {
            sheets_typst::draw(c, d, ox, oy, t.ink, 1.0);
        }
        for (spot, size) in docs.spots.iter().zip(&p.sizes.spots) {
            sheets_typst::draw(
                c,
                &spot.2,
                ox + spot.0 - size.0 / 2.0,
                oy + spot.1 - size.1 / 2.0,
                t.ink,
                1.0,
            );
        }
        return;
    }
    let alpha = if copied || p.heading { 0.6 } else { 1.0 };
    let mut x = r.x + 6.0;
    if !docs.parts.is_empty() {
        for ((d, _), size) in docs.parts.iter().zip(&p.sizes.parts) {
            sheets_typst::draw(c, d, x, r.y + (r.h - size.1) / 2.0, t.ink, alpha);
            x += size.0 + page::PART_GAP;
        }
    } else {
        for (i, (cell, size)) in docs.cells.iter().zip(&p.sizes.cells).enumerate() {
            let y = r.y + (r.h - size.1) / 2.0;
            match (cell, p.words.get(i).and_then(Option::as_ref)) {
                (Cell::Words(_), Some(words)) => words.draw(c, x, y, with_alpha(t.ink, alpha)),
                (Cell::Doc(d), _) => sheets_typst::draw(c, d, x, y, t.ink, alpha),
                _ => {}
            }
            x += p
                .widths
                .get(i)
                .or(p.widths.last())
                .copied()
                .unwrap_or_default();
        }
    }
    let right = r.right() - 6.0;
    if copied {
        if let Some(tick) = &b.chrome.tick {
            tick.draw(c, right - tick.w, r.y + (r.h - tick.h) / 2.0, t.muted);
        }
    } else if let Some(d) = &docs.note
        && let Some((w, h)) = sheets_typst::size(d)
    {
        sheets_typst::draw(c, d, right - w, r.y + (r.h - h) / 2.0, t.ink, 0.65);
    } else if let Some(n) = &p.note {
        n.draw(c, right - n.w, r.y + (r.h - n.h) / 2.0, t.muted);
    }
}

/// A row with no Typst: its runs, tinted while hovered or pressed.
fn paint_plain(
    c: &Canvas,
    b: &Book,
    p: &Prepared,
    r: Rect,
    copied: bool,
    hovered: bool,
    pressed: bool,
) {
    let t = c.theme;
    if !p.heading {
        let fill = if pressed {
            0.08
        } else if hovered {
            0.05
        } else {
            0.0
        };
        if fill > 0.0 {
            c.fill_round(r, 6.0, with_alpha(t.ink, fill));
        }
    }
    let ink = if copied || p.heading { t.muted } else { t.ink };
    for (text, x, y) in &p.runs {
        text.draw(c, r.x + x, r.y + y, ink);
    }
    let side = if copied {
        b.chrome.tick.as_ref()
    } else {
        p.note.as_ref()
    };
    if let Some(n) = side {
        n.draw(c, r.right() - 6.0 - n.w, r.y + (r.h - n.h) / 2.0, t.muted);
    }
}

// ---- the dev pipe (bench.rs `extra`): `sheetpages`, `sheetfilter:<text>`, `sheetback`, `sheetforward`,
// `sheetnew:<x>,<y>`, `sheetclick:<x>,<y>[,right]`, `sheethover:<x>,<y>`, `sheetkey:<vk>` on the front sheet
// window, points in its client DIPs. Called inside the app's borrow, so anything that moves a window is posted.

/// A dev pipe command for the sheets, or None when `cmd` isn't one.
pub fn debug(cmd: &str) -> Option<serde_json::Value> {
    let (name, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
    let front = front();
    let hwnd = front.map(|(h, _)| h);
    let ok = serde_json::json!({ "ok": hwnd.is_some() });
    let nums = |arg: &str| -> Vec<f32> {
        arg.split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect()
    };
    let post = |hwnd: HWND, msg: u32, wp: usize, x: f32, y: f32| {
        let dpi = with(|s| s.open.iter().find(|w| w.hwnd == hwnd).map(|w| w.dpi))
            .flatten()
            .unwrap_or(96) as f32;
        let (px, py) = ((x * dpi / 96.0) as i32, (y * dpi / 96.0) as i32);
        let lp = ((py as u32 & 0xffff) << 16) | (px as u32 & 0xffff);
        // SAFETY: our window; the message comes back through the loop
        let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(wp), LPARAM(lp as isize)) };
    };
    match name {
        "sheetpages" => Some(pages_debug()),
        "sheetfilter" => {
            let h = hwnd?.0 as usize;
            let text = arg.to_string();
            later::free("sheet-debug", 0, move || {
                if let Some(hwnd) = alive(h) {
                    set_filter(hwnd, Some(&text));
                }
            });
            Some(ok)
        }
        "sheetback" | "sheetforward" => {
            go_later(hwnd?, if name == "sheetback" { -1 } else { 1 });
            Some(ok)
        }
        "sheetnew" => {
            let hwnd = hwnd?;
            let p = nums(arg);
            let (x, y) = (*p.first()?, *p.get(1)?);
            let line = on_book(hwnd, |b, _| {
                let cy = y - page::TOP + b.scroll;
                let &(_, si, gi, ii) = b.page.laid.rows.iter().find(|r| r.0.contains(x, cy))?;
                b.page.item(si, gi, ii).open.clone()
            })
            .flatten();
            let opened = line.is_some();
            if let Some(line) = line {
                later::free("sheet-debug", 0, move || open_line(hwnd, line, true));
            }
            Some(serde_json::json!({ "ok": opened }))
        }
        "sheetclick" | "sheethover" => {
            let hwnd = hwnd?;
            let p = nums(arg);
            let (x, y) = (*p.first()?, *p.get(1)?);
            post(hwnd, WM_MOUSEMOVE, 0, x, y);
            if name == "sheetclick" {
                let (down, up) = if arg.ends_with("right") {
                    (WM_RBUTTONDOWN, WM_RBUTTONUP)
                } else {
                    (WM_LBUTTONDOWN, WM_LBUTTONUP)
                };
                post(hwnd, down, 1, x, y);
                post(hwnd, up, 0, x, y);
            }
            Some(ok)
        }
        "sheetkey" => {
            let vk: usize = arg.trim().parse().ok()?;
            let hwnd = hwnd?;
            // SAFETY: our window; the key comes back through the loop
            let _ = unsafe { PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(vk), LPARAM(0)) };
            Some(ok)
        }
        _ => None,
    }
}

fn pages_debug() -> serde_json::Value {
    let front = front().map(|(h, _)| h);
    let windows: Vec<serde_json::Value> = with(|s| {
        s.open
            .iter()
            .map(|w| {
                let mut r = RECT::default();
                // SAFETY: our window, a live out rect
                let _ = unsafe { GetWindowRect(w.hwnd, &mut r) };
                let mut client = RECT::default();
                // SAFETY: as above
                let _ = unsafe { GetClientRect(w.hwnd, &mut client) };
                let mut origin = POINT::default();
                // SAFETY: as above
                let _ = unsafe {
                    windows::Win32::Graphics::Gdi::ClientToScreen(w.hwnd, &mut origin)
                };
                let mut v = serde_json::json!({
                    "key": w.key, "front": Some(w.hwnd) == front, "dpi": w.dpi,
                    "rect": [r.left, r.top, r.right, r.bottom],
                    "client": [origin.x, origin.y, origin.x + client.right, origin.y + client.bottom],
                });
                if let Body::Identity(b) = &w.body {
                    let view = client_h(w.hwnd, w.dpi);
                    let rows: Vec<serde_json::Value> = b
                        .page
                        .laid
                        .rows
                        .iter()
                        .map(|&(rect, si, gi, ii)| {
                            let item = b.page.item(si, gi, ii);
                            let p = b.page.items.get(&(si, gi, ii));
                            let y = rect.y + page::TOP - b.scroll;
                            serde_json::json!({
                                "x": rect.x, "y": y, "w": rect.w, "h": rect.h,
                                "shown": y + rect.h > page::TOP && y < view,
                                "copy": item.copy, "open": item.open,
                                "typeset": p.is_some_and(|p| p.docs.typeset()),
                                "figure": p.is_some_and(|p| p.docs.figure),
                                "cells": p.map(|p| p.sizes.cells.clone()),
                                "parts": p.map(|p| p.sizes.parts.clone()),
                                "spots": p.map(|p| p.docs.spots.iter().zip(&p.sizes.spots)
                                    .map(|(s, z)| (s.0, s.1, z.0, z.1, s.3.clone())).collect::<Vec<_>>()),
                                "widths": p.map(|p| p.widths.clone()),
                            })
                        })
                        .collect();
                    let o = v.as_object_mut().expect("an object");
                    o.insert("pages".into(), serde_json::json!(b.pages.list.iter().map(|p| &p.0).collect::<Vec<_>>()));
                    o.insert("at".into(), serde_json::json!(b.pages.at));
                    o.insert("title".into(), serde_json::json!(b.page.sheet.title));
                    o.insert("filter".into(), serde_json::json!(b.filter.text));
                    o.insert("focused".into(), serde_json::json!(b.focused));
                    o.insert("scroll".into(), serde_json::json!(b.scroll));
                    o.insert("height".into(), serde_json::json!(b.page.laid.height));
                    o.insert("none".into(), serde_json::json!(b.page.laid.none.is_some()));
                    o.insert("copied".into(), serde_json::json!(b.copied.map(|c| c.0)));
                    o.insert("boxed".into(), serde_json::json!(b.boxed.map(|(i, bx)| (i, bx.x, bx.y, bx.w, bx.h))));
                    o.insert("rows".into(), serde_json::json!(rows));
                }
                v
            })
            .collect()
    })
    .unwrap_or_default();
    serde_json::json!({ "windows": windows, "compiled": sheets_typst::count(),
        "clip": crate::clip::get(HWND::default()) })
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
        + clear
            .as_ref()
            .map_or(0.0, |t| clear_gap + t.w.ceil() + 12.0);
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
        mass.draw(
            c,
            r.x + (r.w - mass.w) / 2.0,
            r.bottom() - mass.h - 4.0,
            t.muted,
        );
    }
    // the hovered element's name in the first row's gap
    let detail = union(cell(1, 3), cell(1, 12));
    if let Some((_, name, sub)) = &p.detail {
        name.draw(
            c,
            detail.x + (detail.w - name.w) / 2.0,
            detail.y + 6.0,
            t.ink,
        );
        sub.draw(
            c,
            detail.x + (detail.w - sub.w) / 2.0,
            detail.y + 28.0,
            t.muted,
        );
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
}
