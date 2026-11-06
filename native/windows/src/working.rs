//! The working, Ctrl+W (native only; Working.swift, BarModel.swift `findWorking` / `toggleWorking`, BarView.swift's
//! `WorkingHint` and `WorkingPanel`): the steps to the line's answer, so it can be checked by hand.
//!
//! - **What**: the core's `lib/steps` `steps` (called here as `qcalc_core::steps::steps`, the same function
//!   `Engine::call_json` dispatches to, without the JSON) gives each step's Typst maths, plain text and why, for
//!   `d/dx …`, `∫ … dx`, one equation in one unknown, a unit conversion and a differential equation. The core checks
//!   the last step against its answer; here working whose last `plain` doesn't end in the shown `display` is
//!   dropped too (`keep`). A chained, blank or improper answer has none.
//! - **When**: asked after each live answer lands (model.rs `evaluate_live` calls `landed` with the job's context)
//!   and the line has rested `SETTLE_MS`, on the engine thread's own lane (`Lane::Working`, only the newest line's),
//!   never on the keystroke path. Not on a thread of its own, as the Mac's: there the core compiles a second set
//!   of its regexes, ~7 MB the hidden bar kept. The steps are typeset (each step's `stepDocument` on the Typst
//!   thread, typst_row.rs's renderer) only while the panel is up.
//! - **Hint**: with working, `show working · Ctrl+W` (11 pt muted, a button) at the composer's bottom right, left
//!   of the clear tool's corner; `hide working` while the panel is up. Hidden while the hint line or a tutorial step
//!   shows.
//! - **Panel**: under the bar where the graph and systems panels go, in their look (`bg_tape`, a hairline on top,
//!   12 / 16 / 16 padding): `Working`, `click a step to copy it`, `Ctrl+W hides`; then numbered rows, each its
//!   typeset step (its plain text until typeset) and why at the right (12 pt muted, up to 3 lines, 260 wide). Hover
//!   tints a row, a click copies its `plain` with the copied flash, the wheel scrolls it past 320.
//! - **Keys**: the native action `working` (exponent.rs `NATIVE_ACTIONS`, default Ctrl+W, rebindable in Settings ›
//!   Keybinds, in the `?` sheet's keys). Esc closes the panel before the cheat sheet; hiding the bar closes it. With
//!   the panel up on a line without working nothing shows; it comes back with the next line that has some.
//!
//! **Ctrl+W, the working and the sheets** (`toggles`): on Windows Ctrl+W also closes the reference sheet in front
//! of the bar, else hides the bar (keys.rs `close_front_sheet`). Ctrl+W toggles the working only when some of the
//! working is on screen to act on, that is when the line has working (its hint or its panel shows); on any other
//! line it passes on and closes the front sheet (or hides the bar) as before. A chord the user bound to `working`
//! that isn't Ctrl+W always toggles, as the Mac's ⌃W does: on a line without working it arms the panel for the next
//! line that has some.
//!
//! Cost: on a keystroke, a one-shot timer re-armed as the answer lands; the steps run once the line rests (about
//! an evaluation's time, on the engine thread); nothing drawn or laid out unless the line has working.

use std::cell::RefCell;
use std::sync::Arc;

use qcalc_core::types::{AngleMode, EvaluateOptions, UserFunction};
use qcalc_ffi::{EvalContext, EvalOptions, TypstPaths};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, ID2D1Bitmap,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_WRAP, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::core::{Interface, PCWSTR};

use crate::app::App;
use crate::engine::{EngineThread, Lane, done};
use crate::model::BarModel;
use crate::render::{Canvas, Gfx, Rect, measure, size};

/// The panel's padding: top, sides (the gutter), bottom.
const TOP: f32 = 12.0;
const BOTTOM: f32 = 16.0;
/// The head's line and the gap under it.
const HEAD_H: f32 = 18.0;
const HEAD_GAP: f32 = 8.0;
/// Rows: the gap between them, their padding, the number's column, the gaps beside the maths.
const ROW_GAP: f32 = 2.0;
const ROW_PAD_Y: f32 = 4.0;
const ROW_PAD_X: f32 = 8.0;
const NUM_W: f32 = 14.0;
const GAP: f32 = 12.0;
/// A row's least inner height (one line of 13 pt).
const LINE: f32 = 18.0;
/// Why's widest, and its most lines.
const WHY_W: f32 = 260.0;
const WHY_LINES: u32 = 3;
/// The rows scroll past this.
const MAX_ROWS_H: f32 = 320.0;
/// The hint's room from the composer's right edge (the gutter and the clear tool's corner) and bottom.
const HINT_RIGHT: f32 = size::GUTTER + 44.0;
const HINT_BOTTOM: f32 = 3.0;
/// The chord the working shares with closing the front sheet.
pub const CLOSE_CHORD: &str = "ctrl+w";

/// One step: its Typst maths, plain text (what a click copies), why, and the document its maths compiles to.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub math: String,
    pub plain: String,
    pub why: String,
    pub doc: String,
}

/// The working for one line, and its steps' pictures once typeset.
#[derive(Debug)]
pub struct Working {
    pub text: String,
    pub steps: Vec<Step>,
    pictures: Vec<Option<Arc<TypstPaths>>>,
    asked: bool,
}

/// The model's side (`BarModel.working`).
#[derive(Debug, Default)]
pub struct WorkingState {
    /// the working the core found for the line last answered; None when it has none
    pub found: Option<Working>,
    /// the panel is up (Ctrl+W); it shows only while the line has working
    pub shown: bool,
    hover: Option<usize>,
    scroll: f32,
}

// ---- pure ----

/// The options the steps are worked with (`findWorking`): the bar's angle, sig figs, fractions and thousands, and
/// the history's `ans`, variables and functions.
pub fn options(o: &EvalOptions, context: &EvalContext) -> EvaluateOptions {
    EvaluateOptions {
        angle_mode: Some(if o.angle_radians {
            AngleMode::Rad
        } else {
            AngleMode::Deg
        }),
        sig_figs: Some(f64::from(o.sig_figs)),
        fraction_mode: Some(o.fraction_mode),
        space_thousands: Some(o.space_thousands),
        ans: context.ans,
        variables: context
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        functions: context
            .functions
            .iter()
            .map(|(k, f)| {
                (
                    k.clone(),
                    UserFunction {
                        params: f.params.clone(),
                        body: f.body.clone(),
                    },
                )
            })
            .collect(),
        ..EvaluateOptions::default()
    }
}

/// The steps the bar shows: none when there are none, or when the last one's plain text doesn't end in the answer
/// the slot shows (never working to another answer).
pub fn keep(steps: Option<Vec<qcalc_core::steps::Step>>, display: &str) -> Option<Vec<Step>> {
    let steps = steps?;
    let last = steps.last()?;
    if display.trim().is_empty() || !last.plain.trim_end().ends_with(display.trim()) {
        return None;
    }
    Some(
        steps
            .into_iter()
            .map(|s| Step {
                doc: qcalc_core::steps::step_document(&s.math),
                math: s.math,
                plain: s.plain,
                why: s.why,
            })
            .collect(),
    )
}

/// Whether the working's key toggles the panel (see the module's doc): with working on the line, always; without,
/// only when the chord isn't Ctrl+W, which then closes the front sheet (or hides the bar) as it always has.
pub fn toggles(has_working: bool, chord: &str) -> bool {
    has_working || chord != CLOSE_CHORD
}

/// The hint's words: `show working · Ctrl+W`, or `hide working` while the panel is up.
pub fn hint_text(shown: bool, key_title: &str) -> String {
    if shown {
        "hide working".into()
    } else if key_title.is_empty() {
        "show working".into()
    } else {
        format!("show working · {key_title}")
    }
}

/// A step's picture's scale in a row with `avail` DIPs for it: 1 pt a DIP, narrower in proportion when it hasn't
/// the room, never grown.
pub fn fit(width_pt: f32, avail: f32) -> f32 {
    (avail.max(1.0) / width_pt.max(1.0)).min(1.0)
}

/// The rows' tops (from the rows' top, unscrolled) and their total height.
pub fn stack(heights: &[f32]) -> (Vec<f32>, f32) {
    let mut y = 0.0;
    let tops = heights
        .iter()
        .map(|h| {
            let top = y;
            y += h + ROW_GAP;
            top
        })
        .collect();
    (tops, (y - ROW_GAP).max(0.0))
}

/// The rows' scroll kept in reach: 0 up to what the 320 visible leave hidden.
pub fn clamp_scroll(scroll: f32, total: f32) -> f32 {
    scroll.clamp(0.0, (total - MAX_ROWS_H).max(0.0))
}

/// The row at `y` (from the rows' top, scrolled), if any.
pub fn row_at(tops: &[f32], heights: &[f32], y: f32) -> Option<usize> {
    tops.iter()
        .zip(heights)
        .position(|(t, h)| y >= *t && y < t + h)
}

// ---- the model ----

/// The working the bar has for the line as it is: none on a blank line, a command, the `sys` rows or a graph.
fn current(m: &BarModel) -> Option<&Working> {
    let w = m.working.found.as_ref()?;
    if m.field.text().trim().is_empty()
        || m.command.is_some()
        || m.systems.is_some()
        || m.graph.is_some()
    {
        return None;
    }
    Some(w)
}

/// The panel is on screen.
pub fn panel_shown(m: &BarModel) -> bool {
    m.working.shown && current(m).is_some()
}

/// The hint is on screen: the line has working, and neither the hint line nor a tutorial step is up.
fn hint_shown(m: &BarModel) -> bool {
    current(m).is_some()
        && m.hint.as_ref().is_none_or(|h| h.is_empty())
        && !crate::tour::line_shown(m)
}

fn key_title(m: &BarModel) -> String {
    let chord = crate::exponent::native_chord(&m.settings.keybinds, "working");
    if chord.is_empty() {
        String::new()
    } else {
        crate::hotkey::title(&chord)
    }
}

/// How long the line rests after its answer lands before its working is asked: a burst of keystrokes asks once,
/// after it, so the steps never wait in the engine thread's queue ahead of a keystroke's answer.
pub const SETTLE_MS: u32 = 30;

/// A live answer landed (model.rs `evaluate_live`'s done, after answer.rs): once the line rests, its working is
/// asked on the engine thread's own lane, with the context the answer was worked out in. A chained, blank or
/// improper answer has none.
pub fn landed(app: &mut App, typed: &str, context: EvalContext) {
    let m = &mut app.model;
    if m.field.text() != typed {
        return;
    }
    let Some(a) = m.live.as_ref() else { return };
    if m.chained || crate::answer::is_blank(a) || a.improper {
        crate::later::cancel("working");
        app.engine.cancel(Lane::Working);
        if m.working.found.take().is_some() {
            m.touch();
        }
        return;
    }
    let display = a.display.clone();
    let options = options(&m.options(), &context);
    let text = typed.to_string();
    crate::later::after("working", SETTLE_MS, move |app| {
        if app.model.field.text() == text {
            ask(&app.engine, text, options, display);
        }
    });
}

/// The steps for `text` on the engine thread (`Lane::Working`: only the newest line's), back to `found`. On the
/// engine thread the core's regexes are the ones the answers already compiled, freed with them at a hide.
fn ask(engine: &EngineThread, text: String, options: EvaluateOptions, display: String) {
    engine.submit(Lane::Working, move |_| {
        let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            qcalc_core::steps::steps(&text, &options)
        }))
        .ok()
        .flatten();
        let steps = keep(raw, &display);
        done(move |app| found(app, text, steps))
    });
}

/// The working's thread answered for `text` (on the UI thread): kept if the line is still that.
fn found(app: &mut App, text: String, steps: Option<Vec<Step>>) {
    let m = &mut app.model;
    if m.field.text() != text {
        return;
    }
    let Some(steps) = steps else {
        if m.working.found.take().is_some() {
            m.touch();
        }
        return;
    };
    if let Some(w) = &mut m.working.found
        && w.steps == steps
    {
        w.text = text;
        return;
    }
    m.working.found = Some(Working {
        text,
        pictures: vec![None; steps.len()],
        steps,
        asked: false,
    });
    m.working.hover = None;
    m.working.scroll = 0.0;
    typeset(m, &app.engine);
    m.touch();
}

/// The steps' pictures, once, for the steps now shown, while the panel is up.
fn typeset(m: &mut BarModel, engine: &EngineThread) {
    if !m.working.shown {
        return;
    }
    let Some(w) = &mut m.working.found else {
        return;
    };
    if w.asked {
        return;
    }
    w.asked = true;
    let steps = w.steps.clone();
    engine.typst(move |r| {
        let pictures: Vec<Option<Arc<TypstPaths>>> = steps
            .iter()
            .map(|s| r.paths(s.doc.clone()).ok().map(Arc::new))
            .collect();
        done(move |app| {
            if let Some(w) = &mut app.model.working.found
                && w.steps == steps
            {
                w.pictures = pictures;
                app.model.touch();
            }
        })
    });
}

/// The panel up or down. On a line with no working it waits, and comes up with the next line that has some.
pub fn toggle(m: &mut BarModel, engine: &EngineThread) {
    m.working.shown = !m.working.shown;
    m.working.hover = None;
    m.working.scroll = 0.0;
    typeset(m, engine);
    if current(m).is_some() {
        crate::a11y::announce(if m.working.shown {
            "working shown"
        } else {
            "working hidden"
        });
    }
    m.touch();
}

/// The working's key (exponent.rs `key_down`, for the native action `working` on `chord`): true when it toggled
/// the panel; false passes Ctrl+W on to close the front sheet (`toggles`).
pub fn key(m: &mut BarModel, engine: &EngineThread, chord: &str) -> bool {
    if !toggles(current(m).is_some(), chord) {
        return false;
    }
    toggle(m, engine);
    true
}

/// Esc: the panel closes first (before the cheat sheet and the tape).
pub fn escape(m: &mut BarModel) -> bool {
    if !panel_shown(m) {
        return false;
    }
    m.working.shown = false;
    m.working.hover = None;
    m.touch();
    true
}

/// The bar hid: the panel closes; the pictures' bitmaps and the text layouts go.
pub fn will_hide(m: &mut BarModel) {
    m.working.shown = false;
    m.working.hover = None;
    BITMAPS.with(|b| b.borrow_mut().clear());
    TEXTS.with(|t| t.borrow_mut().take());
}

/// A click on step `i`: its plain text on the clipboard, with the copied flash.
pub fn copy_step(m: &mut BarModel, i: usize) -> bool {
    let Some(plain) = current(m)
        .and_then(|w| w.steps.get(i))
        .map(|s| s.plain.clone())
    else {
        return false;
    };
    m.to_clipboard = Some(plain);
    crate::answer::flash(m);
    true
}

// ---- drawing ----

/// The text layouts for the steps shown (made once per working, and for the hint and head once per key).
struct Texts {
    key: Vec<String>,
    nums: Vec<IDWriteTextLayout>,
    /// why: its layout (right-aligned in WHY_W, up to 3 lines), width and height
    whys: Vec<(IDWriteTextLayout, f32, f32)>,
    /// the plain text drawn until the step is typeset, its width
    plains: Vec<(IDWriteTextLayout, f32)>,
    head: Option<(String, [(IDWriteTextLayout, f32); 3])>,
    hint: Option<(String, IDWriteTextLayout, f32, f32)>,
}

thread_local! {
    static TEXTS: RefCell<Option<Texts>> = const { RefCell::new(None) };
    static SEMIBOLD: RefCell<Option<IDWriteTextFormat>> = const { RefCell::new(None) };
    /// each step's picture drawn into a bitmap of the bar's target: (doc, ink, target, scale), the bitmap, its size
    static BITMAPS: RefCell<Vec<Option<StepBitmap>>> = const { RefCell::new(Vec::new()) };
}

type BitmapKey = (String, [u32; 4], usize, u32);
type StepBitmap = (BitmapKey, ID2D1Bitmap, (f32, f32));

fn with_texts<R>(gfx: &Gfx, w: &Working, f: impl FnOnce(&mut Texts) -> R) -> R {
    TEXTS.with(|t| {
        let mut t = t.borrow_mut();
        let key: Vec<String> = w.steps.iter().map(|s| s.plain.clone()).collect();
        if t.as_ref().is_none_or(|t| t.key != key) {
            let (head, hint) = t.take().map_or((None, None), |t| (t.head, t.hint));
            let whys = w
                .steps
                .iter()
                .filter_map(|s| why_layout(gfx, &s.why))
                .collect();
            let nums = (1..=w.steps.len())
                .filter_map(|n| {
                    gfx.layout_in(
                        &n.to_string(),
                        &gfx.fonts.small,
                        NUM_W,
                        LINE,
                        DWRITE_TEXT_ALIGNMENT_TRAILING,
                    )
                })
                .collect();
            let plains = w
                .steps
                .iter()
                .filter_map(|s| {
                    let l = gfx.layout(&s.plain, &gfx.fonts.body, 0.0, LINE)?;
                    let width = measure(&l).0;
                    Some((l, width))
                })
                .collect();
            *t = Some(Texts {
                key,
                nums,
                whys,
                plains,
                head,
                hint,
            });
        }
        f(t.as_mut().expect("made above"))
    })
}

fn why_layout(gfx: &Gfx, why: &str) -> Option<(IDWriteTextLayout, f32, f32)> {
    let l = gfx.layout_in(
        why,
        &gfx.fonts.small,
        WHY_W,
        1000.0,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    )?;
    let mut m = DWRITE_TEXT_METRICS::default();
    // SAFETY: setters and a query on a live layout; the out struct is a local
    unsafe {
        let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
        let _ = l.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
        l.GetMetrics(&mut m).ok()?;
        if m.lineCount > WHY_LINES {
            let line = m.height / m.lineCount as f32;
            let _ = l.SetMaxHeight(line * WHY_LINES as f32);
            l.GetMetrics(&mut m).ok()?;
        }
        let _ = l.SetMaxHeight(m.height);
    }
    Some((l, m.width.min(WHY_W), m.height))
}

fn semibold(gfx: &Gfx) -> Option<IDWriteTextFormat> {
    SEMIBOLD.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            *s = crate::render::format(
                &gfx.dw,
                &gfx.fonts.text_family,
                size::BODY,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
            )
            .ok();
        }
        s.clone()
    })
}

/// One row's geometry: its height, its picture's scale, why's width.
struct Row {
    h: f32,
    scale: f32,
}

/// The rows of the working in a bar `width` wide: each row's height and scale, their tops and total.
fn rows(gfx: &Gfx, w: &Working, width: f32) -> (Vec<Row>, Vec<f32>, f32) {
    let rows: Vec<Row> = with_texts(gfx, w, |t| {
        w.steps
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let (why_w, why_h) = t.whys.get(i).map_or((0.0, 0.0), |(_, ww, wh)| (*ww, *wh));
                let avail = width
                    - 2.0 * size::GUTTER
                    - 2.0 * ROW_PAD_X
                    - NUM_W
                    - GAP
                    - if why_w > 0.0 { GAP + why_w } else { 0.0 };
                let (scale, math_h) = match w.pictures.get(i).and_then(Option::as_ref) {
                    Some(p) => {
                        let s = fit(p.width_pt, avail);
                        (s, p.height_pt * s)
                    }
                    None => (1.0, LINE),
                };
                Row {
                    h: math_h.max(why_h).max(LINE) + 2.0 * ROW_PAD_Y,
                    scale,
                }
            })
            .collect()
    });
    let heights: Vec<f32> = rows.iter().map(|r| r.h).collect();
    let (tops, total) = stack(&heights);
    (rows, tops, total)
}

/// The panel's height in a bar `width` wide (0 while it doesn't show): window.rs `Bar::layout`.
pub fn height(gfx: &Gfx, m: &BarModel, width: f32) -> f32 {
    if !panel_shown(m) {
        return 0.0;
    }
    let Some(w) = current(m) else { return 0.0 };
    let (_, _, total) = rows(gfx, w, width);
    TOP + HEAD_H + HEAD_GAP + total.min(MAX_ROWS_H) + BOTTOM
}

/// Where the rows start in the panel `r`, and the rect they're seen through.
fn rows_view(r: Rect) -> Rect {
    let y = r.y + TOP + HEAD_H + HEAD_GAP;
    Rect::new(
        r.x + size::GUTTER,
        y,
        r.w - 2.0 * size::GUTTER,
        (r.bottom() - BOTTOM - y).max(0.0),
    )
}

/// The panel in `r` (window.rs `Bar::paint`).
pub fn paint(c: &Canvas, m: &BarModel, r: Rect) {
    let Some(w) = current(m) else { return };
    let t = c.theme;
    c.fill(r, t.bg_tape);
    c.fill(Rect::new(r.x, r.y, r.w, 1.0), t.line);
    let title = key_title(m);
    let hy = r.y + TOP;
    with_texts(c.gfx, w, |tx| {
        if tx.head.as_ref().is_none_or(|(k, _)| *k != title) {
            tx.head = head(c.gfx, &title).map(|h| (title.clone(), h));
        }
        if let Some((_, [(name, nw), (how, _), (hides, hw)])) = &tx.head {
            let x = r.x + size::GUTTER;
            c.text(name, x, hy, t.ink);
            c.text(how, x + nw + 12.0, hy, t.muted);
            if *hw > 0.0 {
                c.text(hides, r.right() - size::GUTTER - hw, hy, t.muted);
            }
        }
    });
    let view = rows_view(r);
    let (rows, tops, total) = rows(c.gfx, w, r.w);
    let scroll = clamp_scroll(m.working.scroll, total);
    c.push_clip(view);
    with_texts(c.gfx, w, |tx| {
        for (i, row) in rows.iter().enumerate() {
            let y = view.y + tops[i] - scroll;
            if y + row.h < view.y || y > view.bottom() {
                continue;
            }
            let rr = Rect::new(view.x, y, view.w, row.h);
            if m.working.hover == Some(i) {
                c.fill_round(rr, size::RADIUS_S, t.fill_hover);
            }
            let mid = y + row.h / 2.0;
            let mut x = rr.x + ROW_PAD_X;
            if let Some(n) = tx.nums.get(i) {
                c.text(n, x, mid - LINE / 2.0, t.muted);
            }
            x += NUM_W + GAP;
            match w.pictures.get(i).and_then(Option::as_ref) {
                Some(p) => draw_picture(c, i, &w.steps[i].doc, p, row.scale, x, mid),
                None => {
                    if let Some((l, _)) = tx.plains.get(i) {
                        c.text(l, x, mid - LINE / 2.0, t.ink);
                    }
                }
            }
            if let Some((l, _, h)) = tx.whys.get(i) {
                c.text(l, rr.right() - ROW_PAD_X - WHY_W, mid - h / 2.0, t.muted);
            }
        }
    });
    c.pop_clip();
}

fn head(gfx: &Gfx, title: &str) -> Option<[(IDWriteTextLayout, f32); 3]> {
    let one = |text: &str, format: &IDWriteTextFormat| {
        let l = gfx.layout(text, format, 0.0, HEAD_H)?;
        let w = if text.is_empty() { 0.0 } else { measure(&l).0 };
        Some((l, w))
    };
    let bold = semibold(gfx)?;
    let hides = if title.is_empty() {
        String::new()
    } else {
        format!("{title} hides")
    };
    Some([
        one("Working", &bold)?,
        one("click a step to copy it", &gfx.fonts.small)?,
        one(&hides, &gfx.fonts.small)?,
    ])
}

/// Step `i`'s picture at `scale`, left at `x`, centred on `mid`: drawn once into a bitmap, copied after.
fn draw_picture(
    c: &Canvas,
    i: usize,
    doc: &str,
    p: &Arc<TypstPaths>,
    scale: f32,
    x: f32,
    mid: f32,
) {
    let ink = c.theme.ink;
    let key: BitmapKey = (
        doc.to_string(),
        [
            ink.r.to_bits(),
            ink.g.to_bits(),
            ink.b.to_bits(),
            ink.a.to_bits(),
        ],
        c.rt().as_raw() as usize,
        scale.to_bits(),
    );
    BITMAPS.with(|b| {
        let mut b = b.borrow_mut();
        if b.len() <= i {
            b.resize_with(i + 1, || None);
        }
        if b[i].as_ref().is_none_or(|(k, _, _)| *k != key) {
            b[i] = crate::typst_row::bitmap(c, p, scale).map(|(bm, size)| (key, bm, size));
        }
        let Some((_, bitmap, (w, h))) = &b[i] else {
            return;
        };
        let top = (mid - h / 2.0).round();
        let dest = D2D_RECT_F {
            left: x.round(),
            top,
            right: x.round() + w,
            bottom: top + h,
        };
        // SAFETY: inside the frame's BeginDraw/EndDraw
        unsafe {
            c.rt().DrawBitmap(
                bitmap,
                Some(&dest),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None,
            );
        }
    });
}

/// The hint's rect in the composer `composer`, when it shows.
fn hint_rect(gfx: &Gfx, m: &BarModel, composer: Rect) -> Option<Rect> {
    if !hint_shown(m) {
        return None;
    }
    let text = hint_text(m.working.shown, &key_title(m));
    let w = current(m)?;
    with_texts(gfx, w, |tx| {
        if tx.hint.as_ref().is_none_or(|(k, ..)| *k != text) {
            let l = gfx.layout(&text, &gfx.fonts.caption, 0.0, LINE)?;
            let (hw, _) = measure(&l);
            tx.hint = Some((text.clone(), l, hw, LINE));
        }
        let (_, _, hw, hh) = tx.hint.as_ref()?;
        Some(Rect::new(
            composer.right() - HINT_RIGHT - hw,
            composer.bottom() - HINT_BOTTOM - hh,
            *hw,
            *hh,
        ))
    })
}

/// The hint, at the composer's bottom right (window.rs `Bar::paint`).
pub fn paint_hint(c: &Canvas, m: &BarModel, composer: Rect) {
    let Some(r) = hint_rect(c.gfx, m, composer) else {
        return;
    };
    let Some(w) = current(m) else { return };
    with_texts(c.gfx, w, |tx| {
        if let Some((_, l, _, _)) = &tx.hint {
            c.text(l, r.x, r.y, c.theme.muted);
        }
    });
}

// ---- the pointer ----

/// The step under (x, y) in the panel, if any.
fn step_at(app: &App, x: f32, y: f32) -> Option<usize> {
    let r = app.bar.layout.working?;
    let view = rows_view(r);
    if !view.contains(x, y) {
        return None;
    }
    let w = current(&app.model)?;
    let (rows, tops, total) = rows(&app.gfx, w, r.w);
    let heights: Vec<f32> = rows.iter().map(|r| r.h).collect();
    row_at(
        &tops,
        &heights,
        y - view.y + clamp_scroll(app.model.working.scroll, total),
    )
}

fn over_hint(app: &App, x: f32, y: f32) -> bool {
    hint_rect(&app.gfx, &app.model, app.bar.layout.composer).is_some_and(|r| r.contains(x, y))
}

/// A click (app.rs `click`): the hint toggles the panel, a step copies itself.
pub fn click(app: &mut App, x: f32, y: f32) -> bool {
    if over_hint(app, x, y) {
        toggle(&mut app.model, &app.engine);
        return true;
    }
    if let Some(i) = step_at(app, x, y) {
        return copy_step(&mut app.model, i);
    }
    app.bar.layout.working.is_some_and(|r| r.contains(x, y))
}

/// The pointer moved (app.rs WM_MOUSEMOVE): the row under it is tinted. Never takes the move.
pub fn mouse_move(app: &mut App, x: f32, y: f32) -> bool {
    let hover = step_at(app, x, y);
    if hover != app.model.working.hover {
        app.model.working.hover = hover;
        app.model.touch();
    }
    false
}

/// The pointer left the bar.
pub fn leave(app: &mut App) {
    if app.model.working.hover.take().is_some() {
        app.model.touch();
    }
}

/// The hand over the hint and the steps.
pub fn cursor(app: &App, x: f32, y: f32) -> Option<PCWSTR> {
    (over_hint(app, x, y) || step_at(app, x, y).is_some())
        .then_some(windows::Win32::UI::WindowsAndMessaging::IDC_HAND)
}

/// The wheel over the panel scrolls its steps (when they're taller than 320).
pub fn wheel(app: &mut App, wp: WPARAM, lp: LPARAM) -> bool {
    let Some(r) = app.bar.layout.working else {
        return false;
    };
    let mut pt = POINT {
        x: i32::from((lp.0 & 0xffff) as u16 as i16),
        y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    };
    // SAFETY: a live point; our bar
    let _ = unsafe { ScreenToClient(app.bar.hwnd, &mut pt) };
    let s = app.bar.scale();
    if !r.contains(pt.x as f32 / s, pt.y as f32 / s) {
        return false;
    }
    let Some(w) = current(&app.model) else {
        return false;
    };
    let (_, _, total) = rows(&app.gfx, w, r.w);
    let delta = f32::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
    let before = app.model.working.scroll;
    let next = clamp_scroll(clamp_scroll(before, total) - delta / 120.0 * 48.0, total);
    if (next - before).abs() > 0.01 {
        app.model.working.scroll = next;
        app.model.touch();
    }
    true
}

// ---- the bench pipe ----

/// `working`: the state, with the hint's and steps' screen rects (px) for a real click. `working press` presses
/// Ctrl+W through the bar's own key path; `working copy <n>` clicks step n.
pub fn debug(app: &App) -> serde_json::Value {
    let m = &app.model;
    let (f, s) = (app.bar.frame(), app.bar.scale());
    let screen = |r: Rect| {
        [
            f.left + (r.x * s) as i32,
            f.top + (r.y * s) as i32,
            (r.w * s) as i32,
            (r.h * s) as i32,
        ]
    };
    let hint = hint_rect(&app.gfx, m, app.bar.layout.composer);
    let rows_px: Vec<_> = match (app.bar.layout.working, current(m)) {
        (Some(r), Some(w)) => {
            let view = rows_view(r);
            let (rows, tops, total) = rows(&app.gfx, w, r.w);
            let scroll = clamp_scroll(m.working.scroll, total);
            rows.iter()
                .zip(&tops)
                .map(|(row, top)| screen(Rect::new(view.x, view.y + top - scroll, view.w, row.h)))
                .collect()
        }
        _ => Vec::new(),
    };
    serde_json::json!({
        "has": current(m).is_some(),
        "for": m.working.found.as_ref().map(|w| w.text.clone()),
        "shown": m.working.shown,
        "panel": app.bar.layout.working.map(screen),
        "hint": hint.map(|_| hint_text(m.working.shown, &key_title(m))),
        "hintRect": hint.map(screen),
        "rows": rows_px,
        "hover": m.working.hover,
        "steps": current(m).map(|w| w.steps.iter().zip(&w.pictures).map(|(s, p)| serde_json::json!({
            "plain": s.plain, "why": s.why, "math": s.math, "typeset": p.is_some(),
        })).collect::<Vec<_>>()),
    })
}

/// `working press`: Ctrl+W as the bar's key path takes it (model.rs `key_down`, then app.rs's close-sheet arm).
pub fn press(app: &mut App) {
    let ctrl = crate::keys::Mods {
        ctrl: true,
        ..Default::default()
    };
    let outcome = app.model.key_down(0x57, ctrl, &app.engine, || None);
    if outcome == crate::model::Outcome::CloseSheet && !crate::keys::close_front_sheet(app) {
        app.ops.push(crate::app::Op::Hide);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(plain: &str) -> qcalc_core::steps::Step {
        qcalc_core::steps::Step {
            math: plain.into(),
            why: "because".into(),
            plain: plain.into(),
        }
    }

    #[test]
    fn working_to_another_answer_is_dropped() {
        let steps = || Some(vec![step("d/dx x^2"), step("= 2x")]);
        let kept = keep(steps(), "2x").expect("ends in the answer");
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].plain, "= 2x");
        assert!(kept[1].doc.contains("$ = 2x $"));
        assert_eq!(keep(steps(), "3x"), None);
        assert_eq!(keep(steps(), ""), None);
        assert_eq!(keep(None, "2x"), None);
        assert_eq!(keep(Some(Vec::new()), "2x"), None);
    }

    #[test]
    fn ctrl_w_toggles_only_with_working_on_screen() {
        // the line has working: Ctrl+W shows (or hides) it
        assert!(toggles(true, "ctrl+w"));
        // none: Ctrl+W closes the front sheet (or hides the bar) as before
        assert!(!toggles(false, "ctrl+w"));
        // a chord of the user's own arms the panel for the next line, as the Mac's ⌃W
        assert!(toggles(false, "ctrl+alt+w"));
        assert!(toggles(true, "ctrl+alt+w"));
    }

    #[test]
    fn working_is_a_rebindable_native_key() {
        use std::collections::BTreeMap;
        let none = BTreeMap::new();
        assert_eq!(
            crate::exponent::native_for(&none, "ctrl+w"),
            Some("working")
        );
        assert_eq!(crate::exponent::native_label("working"), "Show working");
        let core = qcalc_core::app::keybinds::Keybinds::new();
        assert_eq!(
            crate::exponent::native_problem("ctrl+w", "working", &none, &core),
            None
        );
        // Ctrl+E is the exponent form's: refused
        assert!(crate::exponent::native_problem("ctrl+e", "working", &none, &core).is_some());
        let moved = crate::exponent::set_native(&none, "working", "ctrl+alt+w");
        assert_eq!(crate::exponent::native_for(&moved, "ctrl+w"), None);
        assert_eq!(
            crate::exponent::native_for(&moved, "ctrl+alt+w"),
            Some("working")
        );
        assert!(crate::exponent::set_native(&moved, "working", "ctrl+w").is_empty());
    }

    #[test]
    fn the_hint_names_the_key() {
        assert_eq!(hint_text(false, "Ctrl+W"), "show working · Ctrl+W");
        assert_eq!(hint_text(false, ""), "show working");
        assert_eq!(hint_text(true, "Ctrl+W"), "hide working");
    }

    #[test]
    fn rows_stack_scroll_and_hit() {
        let heights = [26.0, 40.0, 26.0];
        let (tops, total) = stack(&heights);
        assert_eq!(tops, vec![0.0, 28.0, 70.0]);
        assert_eq!(total, 96.0);
        assert_eq!(stack(&[]).1, 0.0);
        assert_eq!(row_at(&tops, &heights, 0.0), Some(0));
        assert_eq!(row_at(&tops, &heights, 27.0), None);
        assert_eq!(row_at(&tops, &heights, 50.0), Some(1));
        assert_eq!(row_at(&tops, &heights, 200.0), None);
        // nothing to scroll under 320; past it, up to what's hidden
        assert_eq!(clamp_scroll(30.0, 96.0), 0.0);
        assert_eq!(clamp_scroll(500.0, 400.0), 80.0);
        assert_eq!(clamp_scroll(-5.0, 400.0), 0.0);
        // a picture is never grown, and shrinks to the room
        assert_eq!(fit(100.0, 300.0), 1.0);
        assert_eq!(fit(400.0, 200.0), 0.5);
    }

    #[test]
    fn options_carry_the_bar_and_the_history() {
        let o = EvalOptions {
            angle_radians: true,
            sig_figs: 8,
            fraction_mode: true,
            space_thousands: false,
            ..EvalOptions::default()
        };
        let mut context = EvalContext {
            ans: Some(4.0),
            ..EvalContext::default()
        };
        context.variables.insert("a".into(), 3.0);
        context.functions.insert(
            "f".into(),
            qcalc_ffi::UserFn {
                params: vec!["x".into()],
                body: "x^2".into(),
            },
        );
        let e = options(&o, &context);
        assert_eq!(e.angle_mode, Some(AngleMode::Rad));
        assert_eq!(e.sig_figs, Some(8.0));
        assert_eq!(e.fraction_mode, Some(true));
        assert_eq!(e.ans, Some(4.0));
        assert_eq!(e.variables["a"], 3.0);
        assert_eq!(e.functions["f"].body, "x^2");
    }

    #[test]
    fn the_core_has_working_for_the_lines_the_spec_names() {
        let o = options(&EvalOptions::default(), &EvalContext::default());
        let display = |t: &str| qcalc_core::evaluate::evaluate_line(t, &o).display;
        for line in ["d/dx x^2", "3x+2=11", "x^2-5x+6=0", "5 ft to m"] {
            let raw = qcalc_core::steps::steps(line, &o);
            assert!(
                keep(raw, &display(line)).is_some(),
                "{line} should have working"
            );
        }
        assert_eq!(keep(qcalc_core::steps::steps("2+2", &o), "4"), None);
    }
}
