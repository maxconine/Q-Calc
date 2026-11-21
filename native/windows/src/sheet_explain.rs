//! A formula sheet row's explanation (macos/IdentityWindow.swift `ExplanationView`, `showExplanation`,
//! `fillExplainer`, `openRelated`, `LinkTextView`): a click on an explained row copies it as before and then opens
//! this beside the row, on the side of the screen with more room, 420 DIPs wide and scrolling past 520. It reads the
//! title (15 semibold), the row's equation typeset as in the sheet, what it's for, then Assumptions (bullets),
//! Variables (symbol, meaning, then the unit muted), Example (problem, numbered steps, a bold Answer) and Related
//! last: each Related formula is a link in the accent colour. A link whose formula is a row of some sheet
//! (sheet_match.rs) opens that row's explanation here, with `‹ Back` bottom left; one that isn't closes this and
//! searches the sheets for its note (or the formula). With a `try` line the footer shows it, muted and monospaced,
//! and "Try it in the bar" puts it in the bar in place of what was there and shows the bar.
//!
//! **Keys** (sheets.rs passes the sheet's keys here first while this is up for it): Esc closes it (before the filter
//! is cleared); Tab moves into it, onto its first Related link, then the next (Shift+Tab the one before), past the
//! last to Back and Try it and round to the first; ↵ or Space follows the selected one, and the explanation it opens
//! starts again at its first link; Ctrl+[ / Ctrl+] and Alt+← / Alt+→ step back and forward through the Related chain.
//! UI Automation: the title, each paragraph, each link a hyperlink (focused as Tab selects it), the buttons.
//!
//! **Closing.** Esc, a click elsewhere on its sheet, another row, a page change, the sheet closing, or another
//! application coming to the front (a WinEvent hook while this is up).
//!
//! **Window.** A no-activate topmost popup with DWM's rounded corners and shadow, drawn with the sheets' own
//! factories (its equation is the sheet's compiled cells), made when it opens and destroyed when it closes; nothing
//! is kept while it's away.

use std::cell::RefCell;
use std::collections::HashMap;

use qcalc_core::app::identities::{self, Explanation, Identity, Shift};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_REGULAR, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_LINE_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow,
    EVENT_SYSTEM_FOREGROUND, GetWindowThreadProcessId, IDC_ARROW, IDC_HAND, LoadCursorW,
    RegisterClassExW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SetCursor, SetWindowPos,
    ShowWindow, WINEVENT_OUTOFCONTEXT, WM_DESTROY, WM_ERASEBKGND, WM_GETOBJECT, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR,
    WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::qlog;
use crate::render::{Canvas, Gfx, Rect, Surface, Theme, format, with_alpha};
use crate::sheet_page::{self as page, Cell};
use crate::sheets_typst;

const CLASS: PCWSTR = w!("QCalcExplain");
const WM_MOUSELEAVE: u32 = 0x02A3;

/// The popover's design (DIPs): its width, padding, tallest, and the footer's height.
pub const WIDTH: f32 = 420.0;
pub const PAD: f32 = 16.0;
pub const TALLEST: f32 = 520.0;
pub const FOOTER: f32 = 40.0;
/// between the equation's cells
const CELL_GAP: f32 = 24.0;

// ---- what it says, pure ----

/// How a run of text is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    /// 15 semibold
    Title,
    /// 12.5
    Body,
    /// 12.5 semibold (the Answer line)
    Bold,
    /// 13 (a variable's symbol, a Related formula)
    Symbol,
    /// 12, muted (a unit, a Related formula's note)
    Note,
    /// 11 semibold, muted (Assumptions, Variables...)
    Heading,
    /// 12.5, muted (a bullet, a step's number)
    Marker,
    /// 16 (the equation, where it isn't typeset)
    Equation,
}

impl Style {
    pub fn size(self) -> f32 {
        match self {
            Style::Title => 15.0,
            Style::Body | Style::Bold | Style::Marker => 12.5,
            Style::Symbol => 13.0,
            Style::Note => 12.0,
            Style::Heading => 11.0,
            Style::Equation => 16.0,
        }
    }

    pub fn bold(self) -> bool {
        matches!(self, Style::Title | Style::Bold | Style::Heading)
    }

    pub fn muted(self) -> bool {
        matches!(self, Style::Note | Style::Heading | Style::Marker)
    }
}

/// A run of words: how it's set, raised or lowered, and the Related link it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub shift: Option<Shift>,
    pub link: Option<usize>,
}

/// How a paragraph's lines start: flush, after a marker in a column of its own (bullets, steps), or after a
/// variable's symbol in the variables' shared column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Indent {
    None,
    Column(f32),
    Vars,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Para {
        marker: Vec<Span>,
        spans: Vec<Span>,
        indent: Indent,
        before: f32,
        after: f32,
        center: bool,
    },
    /// the row's equation typeset as the sheet draws it: its size
    Equation(f32, f32),
}

/// Text that raises `^{…}` and lowers `_{…}` as a row's `show` does.
pub fn rich(text: &str, style: Style, link: Option<usize>) -> Vec<Span> {
    identities::identity_runs(text)
        .into_iter()
        .map(|r| Span {
            text: r.text,
            style,
            shift: r.shift,
            link,
        })
        .collect()
}

fn para(spans: Vec<Span>, before: f32, after: f32) -> Block {
    Block::Para {
        marker: Vec::new(),
        spans,
        indent: Indent::None,
        before,
        after,
        center: false,
    }
}

fn heading(title: &str) -> Block {
    para(rich(title, Style::Heading, None), 12.0, 3.0)
}

fn table(marker: Vec<Span>, spans: Vec<Span>, indent: Indent) -> Block {
    Block::Para {
        marker,
        spans,
        indent,
        before: 0.0,
        after: 3.0,
        center: false,
    }
}

fn bullet(spans: Vec<Span>) -> Block {
    table(rich("•", Style::Marker, None), spans, Indent::Column(14.0))
}

/// The explanation as blocks: title, equation (typeset at `typeset`'s size, else its runs), about, assumptions,
/// variables, example, then Related, each formula a link when `links`.
pub fn blocks(
    e: &Explanation,
    item: &Identity,
    typeset: Option<(f32, f32)>,
    links: bool,
) -> Vec<Block> {
    let mut out = vec![para(rich(&e.title, Style::Title, None), 0.0, 0.0)];
    out.push(match typeset {
        Some((w, h)) => Block::Equation(w, h),
        None => {
            let mut spans = Vec::new();
            for (i, cell) in item.lead.iter().flatten().chain([&item.show]).enumerate() {
                if i > 0 {
                    spans.push(Span {
                        text: "    ".into(),
                        style: Style::Equation,
                        shift: None,
                        link: None,
                    });
                }
                spans.extend(rich(cell, Style::Equation, None));
            }
            Block::Para {
                marker: Vec::new(),
                spans,
                indent: Indent::None,
                before: 10.0,
                after: 4.0,
                center: true,
            }
        }
    });
    if !e.about.is_empty() {
        out.push(para(rich(&e.about, Style::Body, None), 8.0, 0.0));
    }
    if !e.assumptions.is_empty() {
        out.push(heading("Assumptions"));
        out.extend(
            e.assumptions
                .iter()
                .map(|a| bullet(rich(a, Style::Body, None))),
        );
    }
    if !e.vars.is_empty() {
        out.push(heading("Variables"));
        for v in &e.vars {
            let mut spans = rich(&v[1], Style::Body, None);
            if !v[2].is_empty() {
                spans.extend(rich(&format!(", in {}", v[2]), Style::Note, None));
            }
            out.push(table(rich(&v[0], Style::Symbol, None), spans, Indent::Vars));
        }
    }
    if let Some(x) = &e.example
        && !x.problem.is_empty()
    {
        out.push(heading("Example"));
        out.push(para(rich(&x.problem, Style::Body, None), 2.0, 0.0));
        for (i, step) in x.steps.iter().enumerate() {
            out.push(table(
                rich(&format!("{}.", i + 1), Style::Marker, None),
                rich(step, Style::Body, None),
                Indent::Column(20.0),
            ));
        }
        if !x.answer.is_empty() {
            let mut spans = rich("Answer  ", Style::Bold, None);
            spans.extend(rich(&x.answer, Style::Bold, None));
            out.push(para(spans, 6.0, 0.0));
        }
    }
    if !e.related.is_empty() {
        out.push(heading("Related"));
        for (i, r) in e.related.iter().enumerate() {
            if r[0].is_empty() {
                continue;
            }
            let mut spans = rich(&r[0], Style::Symbol, links.then_some(i));
            if !r[1].is_empty() {
                spans.push(Span {
                    text: "   ".into(),
                    style: Style::Note,
                    shift: None,
                    link: None,
                });
                spans.extend(rich(&r[1], Style::Note, None));
            }
            out.push(bullet(spans));
        }
    }
    out
}

/// A run's size and how far it's raised, from its style and shift.
pub fn shifted(style: Style, shift: Option<Shift>) -> (f32, f32) {
    let s = style.size();
    match shift {
        Some(Shift::Sup) => (s * 0.73, s * 0.38),
        Some(Shift::Sub) => (s * 0.73, -s * 0.23),
        Some(Shift::Supsub) => (s * 0.62, s * 0.15),
        None => (s, 0.0),
    }
}

/// A piece of text set somewhere: its measured layout, its top left and size, how it's set, its link.
#[derive(Clone, Debug)]
pub struct Placed<T> {
    pub text: T,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub style: Style,
    pub link: Option<usize>,
}

/// The blocks set in a column `width` wide: the pieces, each link's boxes, where the equation goes, the height.
#[derive(Clone, Debug)]
pub struct Set<T> {
    pub pieces: Vec<Placed<T>>,
    pub links: Vec<(usize, Vec<Rect>)>,
    pub equation: Option<(f32, f32, f32, f32)>,
    pub height: f32,
}

/// What a run measures: its handle, width (with trailing spaces), height and baseline.
pub type Measured<T> = (T, f32, f32, f32);

/// Lines broken at spaces; a raised or lowered run sits on the line's baseline, shifted. `measure(text, style,
/// shift)` sets one run.
pub fn set<T: Clone>(
    blocks: &[Block],
    width: f32,
    measure: &mut impl FnMut(&str, Style, Option<Shift>) -> Measured<T>,
) -> Set<T> {
    let mut out = Set {
        pieces: Vec::new(),
        links: Vec::new(),
        equation: None,
        height: 0.0,
    };
    // the variables' column: the widest symbol and some air, between 36 and 110
    let widest = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Para {
                marker,
                indent: Indent::Vars,
                ..
            } => Some(
                marker
                    .iter()
                    .map(|s| measure(&s.text, s.style, s.shift).1)
                    .sum::<f32>(),
            ),
            _ => None,
        })
        .fold(20.0f32, f32::max);
    let vars_column = (widest + 12.0).clamp(36.0, 110.0);
    let mut y = 0.0f32;
    for (n, block) in blocks.iter().enumerate() {
        match block {
            Block::Equation(w, h) => {
                y += 10.0;
                out.equation = Some(((width - w) / 2.0, y, *w, *h));
                y += h + 4.0;
            }
            Block::Para {
                marker,
                spans,
                indent,
                before,
                after,
                center,
            } => {
                if n > 0 {
                    y += before;
                }
                let start = match indent {
                    Indent::None => 0.0,
                    Indent::Column(c) => *c,
                    Indent::Vars => vars_column,
                };
                // the marker (or symbol) sits at the left of the first line; a symbol wider than its column pushes
                // the text along
                let mut first: Vec<(Placed<T>, f32, f32)> = Vec::new();
                let mut mx = 0.0;
                for s in marker {
                    let (t, w, h, base) = measure(&s.text, s.style, s.shift);
                    let raise = shifted(s.style, s.shift).1;
                    first.push((
                        Placed {
                            text: t,
                            x: mx,
                            y: 0.0,
                            w,
                            h,
                            style: s.style,
                            link: None,
                        },
                        base,
                        raise,
                    ));
                    mx += w;
                }
                let mut lines: Vec<Vec<(Placed<T>, f32, f32)>> = vec![first];
                let mut x = start.max(if marker.is_empty() { 0.0 } else { mx + 4.0 });
                for s in spans {
                    for word in words_of(&s.text) {
                        let (t, w, h, base) = measure(word, s.style, s.shift);
                        let raise = shifted(s.style, s.shift).1;
                        let line = lines.last_mut().expect("a line");
                        let has_words = line
                            .iter()
                            .any(|p| p.0.x >= start - 0.01 && p.0.style != Style::Marker)
                            || x > start + 0.01;
                        if x + w.min(width) > width + 0.5 && has_words && !word.trim().is_empty() {
                            lines.push(Vec::new());
                            x = start;
                            if word.trim().is_empty() {
                                continue;
                            }
                        }
                        lines.last_mut().expect("a line").push((
                            Placed {
                                text: t,
                                x,
                                y: 0.0,
                                w,
                                h,
                                style: s.style,
                                link: s.link,
                            },
                            base,
                            raise,
                        ));
                        x += w;
                    }
                }
                for line in lines {
                    if line.is_empty() {
                        continue;
                    }
                    let ascent = line.iter().map(|p| p.1 + p.2).fold(0.0f32, f32::max);
                    let descent = line
                        .iter()
                        .map(|p| p.0.h - p.1 - p.2)
                        .fold(0.0f32, f32::max);
                    let used = line.iter().map(|p| p.0.x + p.0.w).fold(0.0f32, f32::max);
                    let dx = if *center {
                        ((width - used) / 2.0).max(0.0)
                    } else {
                        0.0
                    };
                    for (mut p, base, raise) in line {
                        p.x += dx;
                        p.y = y + ascent - base - raise;
                        if let Some(l) = p.link {
                            let r = Rect::new(p.x, p.y, p.w, p.h);
                            match out.links.iter_mut().find(|(k, _)| *k == l) {
                                // the next word on the same line: one box
                                Some((_, rs))
                                    if rs.last().is_some_and(|q| {
                                        (r.x - q.right()).abs() < 1.0
                                            && r.y < q.bottom()
                                            && q.y < r.bottom()
                                    }) =>
                                {
                                    let q = rs.last_mut().expect("a box");
                                    let (top, bottom) = (q.y.min(r.y), q.bottom().max(r.bottom()));
                                    *q = Rect::new(q.x, top, r.right() - q.x, bottom - top);
                                }
                                Some((_, rs)) => rs.push(r),
                                None => out.links.push((l, vec![r])),
                            }
                        }
                        out.pieces.push(p);
                    }
                    y += ascent + descent + 1.5;
                }
                y += after;
            }
        }
    }
    out.height = y.ceil();
    out
}

/// A run cut into words, each with the spaces after it.
fn words_of(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, c) in text.char_indices() {
        if c == ' ' {
            in_space = true;
        } else if in_space {
            out.push(&text[start..i]);
            start = i;
            in_space = false;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// What the keys select in it: a Related link (by its place among the links), Back, or Try it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Link(usize),
    Back,
    Try,
}

/// Tab (`step` 1) or Shift+Tab (-1) from `at`: the links in order, then Back and Try it where there are, and round.
pub fn step_focus(
    at: Option<Focus>,
    step: i32,
    links: usize,
    back: bool,
    tried: bool,
) -> Option<Focus> {
    let mut all: Vec<Focus> = (0..links).map(Focus::Link).collect();
    if back {
        all.push(Focus::Back);
    }
    if tried {
        all.push(Focus::Try);
    }
    if all.is_empty() {
        return None;
    }
    let n = all.len() as i32;
    let next = match at.and_then(|f| all.iter().position(|x| *x == f)) {
        Some(i) => (i as i32 + step).rem_euclid(n),
        None if step > 0 => 0,
        None => n - 1,
    };
    Some(all[next as usize])
}

// ---- the window ----

/// A laid-out run of text.
#[derive(Clone)]
struct Run {
    layout: IDWriteTextLayout,
}

struct Explainer {
    hwnd: HWND,
    /// the sheet window it explains a row of
    owner: HWND,
    dpi: u32,
    surface: Surface,
    /// the explanations a Related link went through (what's shown, last), and the ones stepped back from
    chain: Vec<(Explanation, Identity)>,
    ahead: Vec<(Explanation, Identity)>,
    set: Set<Run>,
    /// the equation's typeset cells: (document, x, y) from the equation's top left
    cells: Vec<(String, f32, f32)>,
    /// the content's visible height and how far it's scrolled
    view: f32,
    scroll: f32,
    focus: Option<Focus>,
    hover: Option<Focus>,
    pressed: Option<Focus>,
    back: Option<Rect>,
    try_: Option<(Rect, Run, f32)>,
    /// Back's words and their width
    labels: Option<(Run, f32)>,
    try_line: Option<Run>,
    formats: HashMap<(u32, bool, bool), IDWriteTextFormat>,
    hook: Option<HWINEVENTHOOK>,
    tracking: bool,
}

thread_local! {
    static EXPLAIN: RefCell<Option<Explainer>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Explainer) -> R) -> Option<R> {
    EXPLAIN.with(|e| e.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Whether an explanation is up beside a row of this sheet window.
pub fn is_up_for(owner: HWND) -> bool {
    with(|e| e.owner == owner).unwrap_or(false)
}

/// A row's explanation beside it (`row`: its screen rect in physical px), in place of any that's up.
pub fn show(owner: HWND, row: [f64; 4], e: Explanation, item: Identity) {
    let same_owner = is_up_for(owner);
    if !same_owner {
        close();
    }
    let existing = with(|x| {
        x.chain = vec![(e.clone(), item.clone())];
        x.ahead.clear();
        x.focus = None;
        x.scroll = 0.0;
        x.hwnd
    });
    let hwnd = match existing {
        Some(h) => h,
        None => match create(owner, e, item) {
            Some(h) => h,
            None => return,
        },
    };
    fill();
    place(hwnd, row);
    // SAFETY: our window, shown without taking the keys
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    invalidate(hwnd);
}

/// The explanation goes (Esc, a page change, its sheet closing, another app in front).
pub fn close() {
    let gone = EXPLAIN.with(|e| e.try_borrow_mut().ok().and_then(|mut e| e.take()));
    if let Some(x) = gone {
        if let Some(h) = x.hook {
            // SAFETY: our own hook
            let _ = unsafe { UnhookWinEvent(h) };
        }
        crate::uia::forget(x.hwnd);
        let hwnd = x.hwnd;
        drop(x);
        // SAFETY: our window
        let _ = unsafe { DestroyWindow(hwnd) };
    }
}

/// The explanation goes if it is this sheet window's.
pub fn close_for(owner: HWND) {
    if is_up_for(owner) {
        close();
    }
}

/// The theme changed.
pub fn retheme() {
    if let Some(h) = with(|e| e.hwnd) {
        invalidate(h);
    }
}

/// Typing in the filter takes the keys back out of the explanation.
pub fn leave_links(owner: HWND) {
    let h = with(|e| {
        let was = e.focus.take();
        (e.owner == owner && was.is_some()).then_some(e.hwnd)
    })
    .flatten();
    if let Some(h) = h {
        invalidate(h);
    }
}

/// The sheet's keys while this is up for it: Esc, Tab / Shift+Tab, ↵ / Space on what's selected, the chain's back
/// and forward. True when taken.
pub fn key_down(owner: HWND, vk: u32, m: crate::keys::Mods) -> bool {
    if !is_up_for(owner) {
        return false;
    }
    let ctrl = m.ctrl && !m.alt && !m.shift;
    let plain = !m.ctrl && !m.alt && !m.win;
    let step = match vk {
        0x25 if m.alt && !m.ctrl => Some(-1),
        0x27 if m.alt && !m.ctrl => Some(1),
        0xDB if ctrl => Some(-1),
        0xDD if ctrl => Some(1),
        _ => None,
    };
    if let Some(step) = step {
        // while it's up these step through its chain, not the sheet's pages
        go(step);
        return true;
    }
    match vk {
        0x1B if plain => {
            close();
            true
        }
        0x09 if !m.ctrl && !m.alt && !m.win => {
            let to = with(|e| {
                let to = step_focus(
                    e.focus,
                    if m.shift { -1 } else { 1 },
                    e.set.links.len(),
                    e.back.is_some(),
                    e.try_.is_some(),
                );
                e.focus = to;
                (e.hwnd, to)
            });
            if let Some((hwnd, to)) = to {
                reveal(hwnd, to);
                invalidate(hwnd);
                crate::uia::changed(hwnd, None);
            }
            true
        }
        0x0D | 0x20 if plain => match with(|e| e.focus).flatten() {
            Some(f) => {
                activate(f);
                true
            }
            None => false,
        },
        _ => false,
    }
}

/// Scrolls so the selected link or button is in view.
fn reveal(hwnd: HWND, to: Option<Focus>) {
    let Some(Focus::Link(i)) = to else {
        return;
    };
    with(|e| {
        let Some((_, rs)) = e.set.links.get(i) else {
            return;
        };
        let top = rs.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let bottom = rs.iter().map(|r| r.bottom()).fold(0.0, f32::max);
        if top < e.scroll {
            e.scroll = top - 4.0;
        } else if bottom > e.scroll + e.view {
            e.scroll = bottom - e.view + 4.0;
        }
        e.scroll = e.scroll.clamp(0.0, (e.set.height - e.view).max(0.0));
    });
    invalidate(hwnd);
}

/// A link, Back or Try it, clicked or chosen with the keys.
fn activate(f: Focus) {
    match f {
        Focus::Link(i) => {
            let Some((owner, related, keys)) = with(|e| {
                let link = e.set.links.get(i)?.0;
                let r = e.chain.last()?.0.related.get(link)?.clone();
                Some((e.owner, r, e.focus.is_some()))
            })
            .flatten() else {
                return;
            };
            match crate::sheets::related_row(owner, &related[0]) {
                // its own explanation, in place of this one, Back to return
                Some((ex, item)) => {
                    with(|e| {
                        e.chain.push((ex, item));
                        e.ahead.clear();
                        e.scroll = 0.0;
                        e.focus = None;
                    });
                    fill_again(keys);
                }
                // no row is it: the sheets searched for its note's words, else the formula
                None => {
                    close();
                    let words = if related[1].is_empty() {
                        related[0].clone()
                    } else {
                        related[1].clone()
                    };
                    crate::sheets::search_for(owner, words);
                }
            }
        }
        Focus::Back => go(-1),
        Focus::Try => {
            let line = with(|e| {
                e.chain
                    .last()
                    .and_then(|c| c.0.try_line.clone())
                    .filter(|l| !l.is_empty())
            })
            .flatten();
            if let Some(line) = line {
                close();
                crate::sheets::try_line(line);
            }
        }
    }
}

/// Back (-1) to the explanation that linked here, or forward (1) again to the one stepped back from.
fn go(step: i32) {
    let moved = with(|e| {
        if step < 0 && e.chain.len() > 1 {
            let last = e.chain.pop().expect("two or more");
            e.ahead.push(last);
        } else if step > 0
            && let Some(next) = e.ahead.pop()
        {
            e.chain.push(next);
        } else {
            return None;
        }
        e.scroll = 0.0;
        Some(e.focus.is_some())
    })
    .flatten();
    if let Some(keys) = moved {
        with(|e| e.focus = None);
        fill_again(keys);
    }
}

/// The new explanation drawn; with the keys in it, Tab starts again at its first link.
fn fill_again(_keys: bool) {
    fill();
    if let Some((hwnd, rect)) = with(|e| {
        let mut r = RECT::default();
        // SAFETY: our window, a live out rect
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(e.hwnd, &mut r) };
        (e.hwnd, r)
    }) {
        // the popover keeps its top left and takes the new height
        let h = with(|e| frame_height(e)).unwrap_or(0);
        // SAFETY: our window
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        invalidate(hwnd);
        crate::uia::changed(hwnd, None);
    }
}

fn frame_height(e: &Explainer) -> i32 {
    let k = e.dpi as f32 / 96.0;
    ((e.view + PAD * 2.0 + footer(e)) * k).round() as i32
}

fn footer(e: &Explainer) -> f32 {
    if e.back.is_some() || e.try_.is_some() {
        FOOTER
    } else {
        0.0
    }
}

fn register() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    // SAFETY: a class with our static wndproc
    unsafe {
        let Ok(instance) = windows::Win32::System::LibraryLoader::GetModuleHandleW(None) else {
            return;
        };
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
    }
}

fn create(owner: HWND, e: Explanation, item: Identity) -> Option<HWND> {
    register();
    // SAFETY: our class; created hidden, placed and shown by `show`
    let hwnd = unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
        CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(
                WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0,
            ),
            CLASS,
            &windows::core::HSTRING::from(e.title.as_str()),
            WS_POPUP,
            0,
            0,
            1,
            1,
            Some(owner),
            None,
            Some(instance.into()),
            None,
        )
        .ok()?
    };
    // SAFETY: attributes on our own window with pointers to live locals (as notice.rs does)
    unsafe {
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&round as *const windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of_val(&round) as u32,
        );
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 0,
            cyBottomHeight: 1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
    let dpi = crate::compat::dpi_for_window(owner).max(96);
    // another application coming to the front puts it away, as a click outside a popover does
    // SAFETY: an out-of-context hook, delivered on this thread's message loop
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    let x = Explainer {
        hwnd,
        owner,
        dpi,
        surface: Surface::new_flip(hwnd, dpi),
        chain: vec![(e, item)],
        ahead: Vec::new(),
        set: Set {
            pieces: Vec::new(),
            links: Vec::new(),
            equation: None,
            height: 0.0,
        },
        cells: Vec::new(),
        view: 0.0,
        scroll: 0.0,
        focus: None,
        hover: None,
        pressed: None,
        back: None,
        try_: None,
        labels: None,
        try_line: None,
        formats: HashMap::new(),
        hook: (!hook.is_invalid()).then_some(hook),
        tracking: false,
    };
    EXPLAIN.with(|s| {
        if let Ok(mut s) = s.try_borrow_mut() {
            *s = Some(x);
        }
    });
    qlog!("sheet: explanation opened");
    Some(hwnd)
}

unsafe extern "system" fn foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let mut pid = 0u32;
    // SAFETY: a plain query
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != std::process::id() {
        // never inside our own borrow: posted
        crate::later::free("sheet-explain-away", 0, close);
    }
}

fn format_for(
    e: &mut Explainer,
    gfx: &Gfx,
    size: f32,
    bold: bool,
    mono: bool,
) -> Option<IDWriteTextFormat> {
    let key = ((size * 100.0) as u32, bold, mono);
    if let Some(f) = e.formats.get(&key) {
        return Some(f.clone());
    }
    let weight: DWRITE_FONT_WEIGHT = if bold {
        DWRITE_FONT_WEIGHT_SEMI_BOLD
    } else {
        DWRITE_FONT_WEIGHT_REGULAR
    };
    let family = if mono {
        "Consolas"
    } else {
        gfx.fonts.text_family.as_str()
    };
    let f = format(&gfx.dw, family, size, weight).ok()?;
    e.formats.insert(key, f.clone());
    Some(f)
}

/// One line of text: its layout, width, height and baseline.
fn line_of(
    gfx: &Gfx,
    s: &str,
    f: &IDWriteTextFormat,
) -> Option<(IDWriteTextLayout, f32, f32, f32)> {
    let wide: Vec<u16> = s.encode_utf16().collect();
    // SAFETY: wide outlives the call; the format is a live COM object
    let layout = unsafe { gfx.dw.CreateTextLayout(&wide, f, 100_000.0, 1_000.0) }.ok()?;
    // SAFETY: setters and queries on a live layout with live out locals
    unsafe {
        let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    }
    let (w, h) = crate::render::measure(&layout);
    let mut lines = [DWRITE_LINE_METRICS::default()];
    let mut n = 0u32;
    // SAFETY: a live layout, a one-element out slice and count
    let _ = unsafe { layout.GetLineMetrics(Some(&mut lines), &mut n) };
    Some((layout, w, h, lines[0].baseline))
}

/// The explanation that's up set in its column, its equation's cells placed, its footer laid out.
fn fill() {
    let made = crate::sheets::with_gfx(|gfx| {
        with(|e| {
            let Some((ex, item)) = e.chain.last().cloned() else {
                return;
            };
            // the equation typeset as the sheet draws it, where its cells are compiled
            let docs = page::docs(&item);
            let sizes: Option<Vec<(String, (f32, f32))>> = docs
                .cells
                .iter()
                .map(|c| match c {
                    Cell::Doc(d) => sheets_typst::size(d).map(|s| (d.clone(), s)),
                    Cell::Words(_) => None,
                })
                .collect();
            let sizes = sizes.filter(|s| !s.is_empty() && !docs.figure);
            let typeset = sizes.as_ref().map(|s| {
                let w = s.iter().map(|c| c.1.0).sum::<f32>() + CELL_GAP * (s.len() as f32 - 1.0);
                let h = s.iter().map(|c| c.1.1).fold(0.0, f32::max);
                (w, h)
            });
            let blocks = blocks(&ex, &item, typeset, true);
            let inner = WIDTH - PAD * 2.0;
            let mut measure =
                |text: &str, style: Style, shift: Option<Shift>| -> Measured<Option<Run>> {
                    let (size, _) = shifted(style, shift);
                    match format_for(e, gfx, size, style.bold(), false)
                        .and_then(|f| line_of(gfx, text, &f))
                    {
                        Some((layout, w, h, base)) => (Some(Run { layout }), w, h, base),
                        None => (None, 0.0, size * 1.3, size),
                    }
                };
            let set = set(&blocks, inner, &mut measure);
            e.cells.clear();
            if let (Some(sizes), Some((_, _, _, h))) = (&sizes, set.equation) {
                let mut x = 0.0;
                for (doc, (w, ch)) in sizes {
                    e.cells.push((doc.clone(), x, (h - ch) / 2.0));
                    x += w + CELL_GAP;
                }
            }
            e.set = Set {
                pieces: set
                    .pieces
                    .into_iter()
                    .filter_map(|p| {
                        Some(Placed {
                            text: p.text.clone()?,
                            x: p.x,
                            y: p.y,
                            w: p.w,
                            h: p.h,
                            style: p.style,
                            link: p.link,
                        })
                    })
                    .collect(),
                links: set.links,
                equation: set.equation,
                height: set.height,
            };
            // the footer: Back with somewhere to go back to, the Try line and its button
            let caption = format_for(e, gfx, 12.0, false, false);
            let mono = format_for(e, gfx, 11.0, false, true);
            let back = caption.as_ref().and_then(|f| line_of(gfx, "‹ Back", f));
            let tried = caption
                .as_ref()
                .and_then(|f| line_of(gfx, "Try it in the bar", f));
            let line = ex.try_line.clone().filter(|l| !l.is_empty());
            let fy = 0.0;
            e.back = (e.chain.len() > 1).then(|| {
                let w = back.as_ref().map_or(50.0, |b| b.1) + 20.0;
                Rect::new(PAD, fy, w, 24.0)
            });
            e.try_ = match (&line, tried) {
                (Some(_), Some((layout, w, _, _))) => {
                    let bw = w + 20.0;
                    Some((Rect::new(WIDTH - PAD - bw, fy, bw, 24.0), Run { layout }, w))
                }
                _ => None,
            };
            e.labels = back.map(|(l, w, ..)| (Run { layout: l }, w));
            e.try_line = line
                .as_ref()
                .and_then(|l| mono.as_ref().and_then(|f| line_of(gfx, l, f)))
                .map(|(layout, ..)| Run { layout });
            let room = TALLEST - footer(e) - PAD * 2.0;
            e.view = e.set.height.min(room).max(20.0);
            e.scroll = e.scroll.clamp(0.0, (e.set.height - e.view).max(0.0));
        })
    });
    if made.flatten().is_none() {
        qlog!("sheet: explanation couldn't be set");
    }
}

/// Beside the row, on the side of its screen with more room, its top level with the row's where it fits.
fn place(hwnd: HWND, row: [f64; 4]) {
    let m = crate::window::monitor_at(POINT {
        x: (row[0] + row[2] / 2.0) as i32,
        y: (row[1] + row[3] / 2.0) as i32,
    });
    let Some(h) = with(|e| {
        e.dpi = m.dpi;
        e.surface.set_dpi(m.dpi);
        frame_height(e)
    }) else {
        return;
    };
    let k = m.scale();
    let w = (f64::from(WIDTH) * k).round();
    let gap = 6.0 * k;
    let a = m.area;
    let right_room = a.x + a.width - (row[0] + row[2]);
    let left_room = row[0] - a.x;
    let x = if right_room >= left_room {
        (row[0] + row[2] + gap).min(a.x + a.width - w)
    } else {
        (row[0] - w - gap).max(a.x)
    };
    let h = f64::from(h);
    let y = row[1].clamp(a.y, (a.y + a.height - h).max(a.y));
    // SAFETY: our window
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST),
            x as i32,
            y as i32,
            w as i32,
            h as i32,
            SWP_NOACTIVATE,
        )
    };
}

fn invalidate(hwnd: HWND) {
    // SAFETY: our window
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
}

/// What's under a point (DIPs in the window): a link, Back or Try it.
fn hit(e: &Explainer, x: f32, y: f32) -> Option<Focus> {
    let ft = PAD + e.view + PAD;
    if let Some(b) = e.back
        && Rect::new(b.x, ft + b.y, b.w, b.h).contains(x, y)
    {
        return Some(Focus::Back);
    }
    if let Some((b, ..)) = &e.try_
        && Rect::new(b.x, ft + b.y, b.w, b.h).contains(x, y)
    {
        return Some(Focus::Try);
    }
    if y < PAD || y > PAD + e.view {
        return None;
    }
    let (cx, cy) = (x - PAD, y - PAD + e.scroll);
    e.set
        .links
        .iter()
        .position(|(_, rs)| rs.iter().any(|r| r.inset(-1.0, -1.0).contains(cx, cy)))
        .map(Focus::Link)
}

unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let dips = |lp: LPARAM| {
        let k = with(|e| e.dpi as f32 / 96.0).unwrap_or(1.0);
        (
            f32::from((lp.0 & 0xffff) as u16 as i16) / k,
            f32::from(((lp.0 >> 16) & 0xffff) as u16 as i16) / k,
        )
    };
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(3), // MA_NOACTIVATE
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            // SAFETY: our window
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            return LRESULT(0);
        }
        windows::Win32::UI::WindowsAndMessaging::WM_SIZE => {
            let (w, h) = ((lp.0 & 0xffff) as u32, ((lp.0 >> 16) & 0xffff) as u32);
            with(|e| e.surface.resize(w, h));
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let (x, y) = dips(lp);
            let changed = with(|e| {
                if !e.tracking {
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: a live struct for our window
                    let _ = unsafe { TrackMouseEvent(&mut t) };
                    e.tracking = true;
                }
                let h = hit(e, x, y);
                std::mem::replace(&mut e.hover, h) != h
            });
            if changed == Some(true) {
                invalidate(hwnd);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            with(|e| {
                e.tracking = false;
                e.hover = None;
            });
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = dips(lp);
            with(|e| e.pressed = hit(e, x, y));
            // SAFETY: our window takes the mouse until the button comes up
            unsafe { SetCapture(hwnd) };
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = dips(lp);
            // SAFETY: we may hold the capture
            let _ = unsafe { ReleaseCapture() };
            let act = with(|e| {
                let p = e.pressed.take();
                (p.is_some() && p == hit(e, x, y)).then_some(p).flatten()
            })
            .flatten();
            invalidate(hwnd);
            if let Some(f) = act {
                // posted: following a link destroys or refills this window
                crate::later::free("sheet-explain", 0, move || activate(f));
            }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let delta = f32::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
            with(|e| {
                e.scroll =
                    (e.scroll - delta / 120.0 * 48.0).clamp(0.0, (e.set.height - e.view).max(0.0));
            });
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            if (lp.0 & 0xffff) as u32 == 1 {
                let over = with(|e| e.hover.is_some()).unwrap_or(false);
                // SAFETY: system cursors
                unsafe {
                    if let Ok(c) = LoadCursorW(None, if over { IDC_HAND } else { IDC_ARROW }) {
                        SetCursor(Some(c));
                    }
                }
                return LRESULT(1);
            }
        }
        WM_GETOBJECT => {
            if let Some(r) = crate::uia::get_object(hwnd, wp, lp, tree) {
                return r;
            }
        }
        WM_DESTROY => return LRESULT(0),
        windows::Win32::UI::WindowsAndMessaging::WM_CREATE => {
            // SAFETY: lParam is our CREATESTRUCTW (nothing passed)
            let _ = unsafe { &*(lp.0 as *const CREATESTRUCTW) };
        }
        _ => {}
    }
    // SAFETY: the default for our own window
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn stroke(c: &Canvas, r: Rect, radius: f32, color: crate::render::Color, width: f32) {
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
    unsafe {
        c.rt()
            .DrawRoundedRectangle(&rr, c.brush(color), width, None)
    };
}

/// Frames drawn (the dev pipe's `sheetexplain`).
static PAINTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn paint(hwnd: HWND) {
    PAINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let theme: Theme = crate::sheets::theme();
    crate::sheets::with_gfx(|gfx| {
        with(|e| {
            let Explainer {
                surface,
                set,
                cells,
                view,
                scroll,
                focus,
                hover,
                pressed,
                back,
                try_,
                labels,
                try_line,
                ..
            } = e;
            let (view, scroll) = (*view, *scroll);
            let result = surface.draw(gfx, &theme, |c| {
                let t = c.theme;
                c.clear(t.bg);
                c.fill(Rect::new(0.0, 0.0, c.width, 1.0), t.line);
                let inner = WIDTH - PAD * 2.0;
                c.push_clip(Rect::new(PAD - 4.0, PAD - 2.0, inner + 8.0, view + 4.0));
                let (ox, oy) = (PAD, PAD - scroll);
                if let Some((ex, ey, _, _)) = set.equation {
                    for (doc, x, y) in cells.iter() {
                        sheets_typst::draw(c, doc, ox + ex + x, oy + ey + y, t.ink, 1.0);
                    }
                }
                for p in &set.pieces {
                    if p.y + p.h + oy < 0.0 || p.y + oy > PAD + view {
                        continue;
                    }
                    let link_here = p
                        .link
                        .and_then(|l| set.links.iter().position(|(k, _)| *k == l));
                    let ink = if p.link.is_some() {
                        t.accent
                    } else if p.style.muted() {
                        t.muted
                    } else {
                        t.ink
                    };
                    c.text(&p.text.layout, ox + p.x, oy + p.y, ink);
                    if let Some(n) = link_here
                        && *hover == Some(Focus::Link(n))
                    {
                        c.fill(
                            Rect::new(ox + p.x, oy + p.y + p.h - 2.0, p.w, 1.0),
                            t.accent,
                        );
                    }
                }
                // the link the keys are on
                if let Some(Focus::Link(n)) = focus
                    && let Some((_, rs)) = set.links.get(*n)
                {
                    for r in rs {
                        stroke(
                            c,
                            Rect::new(ox + r.x - 3.0, oy + r.y - 1.0, r.w + 6.0, r.h + 2.0),
                            4.0,
                            t.accent,
                            1.5,
                        );
                    }
                }
                c.pop_clip();
                // the scrollbar's thumb, while it scrolls
                if set.height > view + 0.5 {
                    let h = (view * view / set.height).max(24.0);
                    let y = PAD + (view - h) * (scroll / (set.height - view)).clamp(0.0, 1.0);
                    c.fill_round(
                        Rect::new(WIDTH - 8.0, y, 4.0, h),
                        2.0,
                        with_alpha(t.ink, 0.25),
                    );
                }
                // the footer
                let ft = PAD + view + PAD;
                let button = |r: Rect, f: Focus| {
                    let r = Rect::new(r.x, ft + r.y, r.w, r.h);
                    let tone = if *pressed == Some(f) {
                        0.12
                    } else if *hover == Some(f) {
                        0.08
                    } else {
                        0.05
                    };
                    c.fill_round(r, 6.0, with_alpha(t.ink, tone));
                    stroke(
                        c,
                        r,
                        6.0,
                        if *focus == Some(f) { t.accent } else { t.line },
                        if *focus == Some(f) { 1.5 } else { 1.0 },
                    );
                    r
                };
                let mut line_x = PAD;
                if let Some(b) = *back {
                    let r = button(b, Focus::Back);
                    if let Some((label, w)) = labels {
                        c.text(&label.layout, r.x + (r.w - *w) / 2.0, r.y + 4.0, t.ink);
                    }
                    line_x = r.right() + 10.0;
                }
                if let Some((b, label, w)) = try_ {
                    let r = button(*b, Focus::Try);
                    c.text(&label.layout, r.x + (r.w - *w) / 2.0, r.y + 4.0, t.ink);
                    if let Some(l) = try_line {
                        c.push_clip(Rect::new(line_x, ft, (r.x - line_x - 8.0).max(0.0), 24.0));
                        c.text(&l.layout, line_x, ft + 5.0, t.muted);
                        c.pop_clip();
                    }
                }
            });
            if let Err(err) = result {
                qlog!("sheet explanation paint failed: {err}");
            }
        })
    });
    let _ = hwnd;
}

/// The explanation for screen readers: its title, its text, each Related formula a hyperlink, the buttons.
fn tree(hwnd: HWND) -> Option<crate::uia::Tree> {
    use crate::uia::{Node, Role, Tree};
    let mut origin = POINT::default();
    // SAFETY: our window, a live out point
    let _ = unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut origin) };
    with(|e| {
        let (ex, _) = e.chain.last()?;
        let k = f64::from(e.dpi) / 96.0;
        let on_screen = |r: Rect| {
            [
                f64::from(origin.x) + f64::from(r.x) * k,
                f64::from(origin.y) + f64::from(r.y) * k,
                f64::from(r.w) * k,
                f64::from(r.h) * k,
            ]
        };
        let plain = |s: &str| -> String {
            identities::identity_runs(s)
                .into_iter()
                .map(|r| r.text)
                .collect()
        };
        let mut nodes = vec![Node {
            id: 1,
            role: Role::Heading,
            name: plain(&ex.title),
            ..Node::default()
        }];
        let mut text = plain(&ex.about);
        if let Some(x) = &ex.example {
            text.push_str(&format!(
                " Example: {} Answer: {}",
                plain(&x.problem),
                plain(&x.answer)
            ));
        }
        nodes.push(Node {
            id: 2,
            role: Role::Text,
            name: text,
            ..Node::default()
        });
        for (n, (link, rs)) in e.set.links.iter().enumerate() {
            let Some(first) = rs.first() else { continue };
            let formula = ex
                .related
                .get(*link)
                .map(|r| plain(&r[0]))
                .unwrap_or_default();
            nodes.push(Node {
                id: 10 + n as i32,
                role: Role::Link,
                name: formula,
                help: ex
                    .related
                    .get(*link)
                    .map(|r| plain(&r[1]))
                    .unwrap_or_default(),
                rect: on_screen(Rect::new(
                    PAD + first.x,
                    PAD + first.y - e.scroll,
                    first.w,
                    first.h,
                )),
                focusable: true,
                focused: e.focus == Some(Focus::Link(n)),
                ..Node::default()
            });
        }
        let ft = PAD + e.view + PAD;
        if let Some(b) = e.back {
            nodes.push(Node {
                id: 3,
                role: Role::Button,
                name: "Back".into(),
                help: "Back to the explanation before".into(),
                rect: on_screen(Rect::new(b.x, ft + b.y, b.w, b.h)),
                focusable: true,
                focused: e.focus == Some(Focus::Back),
                ..Node::default()
            });
        }
        if let Some((b, ..)) = &e.try_ {
            nodes.push(Node {
                id: 4,
                role: Role::Button,
                name: "Try it in the bar".into(),
                help: ex.try_line.clone().unwrap_or_default(),
                rect: on_screen(Rect::new(b.x, ft + b.y, b.w, b.h)),
                focusable: true,
                focused: e.focus == Some(Focus::Try),
                ..Node::default()
            });
        }
        Some(Tree {
            name: plain(&ex.title),
            nodes,
        })
    })
    .flatten()
}

/// The dev pipe's `sheetexplain`: what's up and what the keys are on.
pub fn debug() -> serde_json::Value {
    with(|e| {
        let (ex, _) = e.chain.last().map(|c| (c.0.title.clone(), ())).unwrap_or_default();
        let focus = match e.focus {
            Some(Focus::Link(n)) => e
                .set
                .links
                .get(n)
                .and_then(|(l, _)| e.chain.last()?.0.related.get(*l))
                .map(|r| r[0].clone())
                .unwrap_or_default(),
            Some(Focus::Back) => "Back".into(),
            Some(Focus::Try) => "Try".into(),
            None => String::new(),
        };
        let mut r = RECT::default();
        // SAFETY: our window, a live out rect
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(e.hwnd, &mut r) };
        serde_json::json!({
            "title": ex, "focus": focus, "links": e.set.links.len(), "chain": e.chain.len(),
            "back": e.back.is_some(), "try": e.try_.is_some(), "height": e.set.height, "view": e.view,
            "rect": [r.left, r.top, r.right, r.bottom],
            // SAFETY: a plain query on our window
            "visible": unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(e.hwnd) }.as_bool(),
            "paints": PAINTS.load(std::sync::atomic::Ordering::Relaxed),
            "px": [e.surface.px.0, e.surface.px.1], "live": e.surface.is_live(), "dpi": e.surface.dpi,
        })
    })
    .unwrap_or(serde_json::json!({ "title": null }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(text: &str, style: Style, shift: Option<Shift>) -> Measured<String> {
        let (size, _) = shifted(style, shift);
        (
            text.to_string(),
            text.chars().count() as f32 * size * 0.5,
            size * 1.3,
            size,
        )
    }

    fn torsion() -> (Explanation, Identity) {
        let sheet = page::page_sheet("continuum mechanics").unwrap();
        let item = sheet
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
            .find(|i| i.show == "τ = Tρ/J")
            .cloned()
            .expect("the torsion row");
        (item.explain.clone().expect("explained"), item)
    }

    #[test]
    fn an_explanation_reads_title_equation_about_and_related_last() {
        let (e, item) = torsion();
        let b = blocks(&e, &item, None, true);
        let Block::Para { spans, .. } = &b[0] else {
            panic!()
        };
        assert_eq!(spans[0].style, Style::Title);
        let Block::Para { center, .. } = &b[1] else {
            panic!()
        };
        assert!(center);
        // Related last, each formula a link to its place
        let links: Vec<usize> = b
            .iter()
            .filter_map(|x| match x {
                Block::Para { spans, .. } => spans.iter().find_map(|s| s.link),
                _ => None,
            })
            .collect();
        assert_eq!(
            links.len(),
            e.related.iter().filter(|r| !r[0].is_empty()).count()
        );
        assert!(links.windows(2).all(|w| w[0] < w[1]));
        // the related heading is after the example's
        let heads: Vec<String> = b
            .iter()
            .filter_map(|x| match x {
                Block::Para { spans, .. }
                    if spans.first().is_some_and(|s| s.style == Style::Heading) =>
                {
                    Some(spans[0].text.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(heads.last().map(String::as_str), Some("Related"));
        // typeset, the equation is a box of its own
        let b = blocks(&e, &item, Some((80.0, 20.0)), false);
        assert_eq!(b[1], Block::Equation(80.0, 20.0));
    }

    #[test]
    fn text_wraps_in_its_column_and_links_are_boxed() {
        let (e, item) = torsion();
        let b = blocks(&e, &item, None, true);
        let s = set(&b, WIDTH - PAD * 2.0, &mut fake);
        assert!(
            s.pieces
                .iter()
                .all(|p| p.x + p.w <= WIDTH - PAD * 2.0 + 40.0)
        );
        assert!(s.height > 200.0);
        assert!(!s.links.is_empty());
        // pieces go down the page
        let ys: Vec<f32> = s.pieces.iter().map(|p| p.y).collect();
        assert!(ys.first() < ys.last());
        // a raised run sits above the line's text
        let spans = rich("x^{2}", Style::Body, None);
        let s = set(&[para(spans, 0.0, 0.0)], 300.0, &mut fake);
        assert!(s.pieces[1].y < s.pieces[0].y);
        // a long paragraph wraps
        let long = "word ".repeat(80);
        let s = set(
            &[para(rich(&long, Style::Body, None), 0.0, 0.0)],
            300.0,
            &mut fake,
        );
        let lines: std::collections::BTreeSet<i32> = s.pieces.iter().map(|p| p.y as i32).collect();
        assert!(lines.len() > 3);
        assert!(s.pieces.iter().all(|p| p.x + p.w <= 300.0 + 0.5));
    }

    #[test]
    fn tab_goes_through_the_links_then_back_and_try_and_round() {
        assert_eq!(step_focus(None, 1, 2, true, true), Some(Focus::Link(0)));
        assert_eq!(
            step_focus(Some(Focus::Link(0)), 1, 2, true, true),
            Some(Focus::Link(1))
        );
        assert_eq!(
            step_focus(Some(Focus::Link(1)), 1, 2, true, true),
            Some(Focus::Back)
        );
        assert_eq!(
            step_focus(Some(Focus::Back), 1, 2, true, true),
            Some(Focus::Try)
        );
        assert_eq!(
            step_focus(Some(Focus::Try), 1, 2, true, true),
            Some(Focus::Link(0))
        );
        assert_eq!(
            step_focus(Some(Focus::Link(0)), -1, 2, false, true),
            Some(Focus::Try)
        );
        assert_eq!(step_focus(None, -1, 2, false, false), Some(Focus::Link(1)));
        assert_eq!(step_focus(None, 1, 0, false, false), None);
    }
}
