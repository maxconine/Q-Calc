//! The live answer, right of the line (`LiveAnswerView` in BarView.swift, `LiveAnswer.tsx`), and everything that
//! decides what it says:
//!
//! - **What shows** (`specs`, pure): the steady answer (the last real one, dimmed, while an edit is half typed:
//!   `isEditOf`, `STEADY_MS`), a solve's message, a solve's roots (`x =` copies them all, each root copies itself at
//!   full precision, `, …` past `MAX_SHOWN_ROOTS`), an improper conversion as a message, the dual `exact ≈ decimal`
//!   (each half its own copy), or the one answer in the form Tab picked / the exact-or-approx setting.
//! - **Fit** (`measure`): the pieces at their sizes; a long answer shrinks to half before it is cut (`useFitFont`).
//!   Layouts are cached by text and size, so a keystroke that doesn't change the answer makes none.
//! - **Copy**: a click on a piece (`hit`) or Ctrl+C copies, unitless when "copy unitless" is on; the "✓ copied"
//!   flash hangs left of the answer for 1.2 s, and the answer's width counts it, so it shows whole.
//! - **Tab** (`cycle_forms`): the engine's `answer_forms` for the answer as shown, on `Lane::Forms`, then cycled
//!   locally; Shift+Tab goes back.
//! - **Closed forms**: the live job hands back `Engine::closed_form`'s future with its `CancelToken`
//!   (`closed_form`); the UI thread polls it whenever WM_ENGINE comes (`poll`), its waker posts WM_ENGINE, and a new
//!   keystroke cancels it. A found form replaces `live.exact` (π²/6 for 1.6449…).
//!
//! Timers here are one-shot thread timers (no window needed): the steady answer's end and the flash's.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use qcalc_core::app::answer as core_answer;
use qcalc_core::format::{format_number, pretty_answer};
use qcalc_ffi::{CancelToken, CoreError, Engine, FormSource, LiveAnswer, SolveOutcome};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_FEATURE, DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES, DWRITE_TEXT_RANGE,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, IDWriteTextFormat, IDWriteTextLayout,
    IDWriteTypography,
};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostMessageW, SetTimer};

use crate::app::{App, WM_ENGINE, with_app};
use crate::engine::{EngineThread, Lane, done};
use crate::model::BarModel;
use crate::render::{Canvas, Color, Gfx, Rect, measure as measure_layout, size, with_alpha};

/// --answer min width (minmax(120px, max-content)) and its padding (2 8)
pub const MIN_W: f32 = 120.0;
pub const PAD_X: f32 = 8.0;
/// the copied flash's life (CopiedCheck: in 120 ms, held, gone by 1.2 s)
pub const COPIED_MS: u32 = 1200;

// ---- state ----

type ClosedFuture = Pin<Box<dyn Future<Output = Result<Option<String>, CoreError>> + Send>>;

/// A closed-form search under way, from the engine thread: cancelled (and dropped) when the line changes.
pub struct ClosedJob {
    token: Arc<CancelToken>,
    future: ClosedFuture,
}

struct Closed {
    text: String,
    job: ClosedJob,
}

impl Drop for Closed {
    fn drop(&mut self) {
        self.job.token.cancel();
    }
}

/// The answer's own state on the model (`BarModel.answer`).
#[derive(Default)]
pub struct AnswerState {
    /// the last real answer, dimmed, while the line is briefly unreadable (useSteadyAnswer)
    pub steady: Option<String>,
    last_live_text: String,
    /// Tab's forms for the answer as it is (empty until Tab), and the one shown
    pub forms: Vec<String>,
    pub form_index: usize,
    /// a closed form the engine's search found after the answer, shown in place of `live.exact`
    pub upgraded_exact: Option<String>,
    closed: Option<Closed>,
    /// the "✓ copied" flash is up
    pub copied: bool,
}

impl AnswerState {
    /// The line changed: what belonged to the old text goes (forms, the closed form, the flash).
    pub fn text_changed(&mut self, text: &str) {
        self.forms.clear();
        self.form_index = 0;
        self.upgraded_exact = None;
        self.closed = None;
        if self.copied {
            self.copied = false;
            timer::stop(timer::COPIED);
        }
        if text.trim().is_empty() {
            self.steady = None;
        }
    }
}

pub fn is_blank(a: &LiveAnswer) -> bool {
    a.display.trim().is_empty()
}

fn live_of(m: &BarModel) -> Option<&LiveAnswer> {
    m.live.as_ref().filter(|a| !is_blank(a))
}

/// The closed form beside the decimal, upgraded by the search when it found one.
pub fn exact(m: &BarModel) -> Option<&str> {
    m.answer
        .upgraded_exact
        .as_deref()
        .or(m.live.as_ref().and_then(|a| a.exact.as_deref()))
        .filter(|e| !e.trim().is_empty())
}

/// Whether the slot shows `exact ≈ decimal` (never while Tab shows another form).
pub fn has_dual(m: &BarModel) -> bool {
    let Some(a) = live_of(m) else { return false };
    m.answer.form_index == 0 && core_answer::has_dual_answer(&a.display, exact(m))
}

/// The answer the slot shows and Ctrl+C copies: the cycled form, else exact or approximate per the setting.
pub fn shown_answer(m: &BarModel) -> String {
    let Some(a) = live_of(m) else {
        return String::new();
    };
    let st = &m.answer;
    if st.form_index > 0
        && let Some(f) = st.forms.get(st.form_index)
    {
        return f.clone();
    }
    if m.settings.answer_form == "exact"
        && let Some(e) = exact(m)
    {
        return e.to_string();
    }
    a.display.clone()
}

/// The engine's own pretty text for the live display; `pretty_text` for anything else.
fn pretty(m: &BarModel, s: &str) -> String {
    match &m.live {
        Some(a) if s == a.display && !a.pretty.is_empty() => radicals_if_math(&a.pretty),
        _ => pretty_text(s),
    }
}

/// `Pretty.answer` (TextRules.swift): `prettyAnswer` (a real minus, grouped digits), and in plain maths the web's
/// radicals as text: `2sqrt(2)` reads `2√2`, a long argument keeps its brackets, `pi` is π.
pub fn pretty_text(s: &str) -> String {
    radicals_if_math(&pretty_answer(s))
}

pub fn radicals_if_math(s: &str) -> String {
    const MATH: &str = "0123456789 \t+-−*/^().,πe√i·×sqrt";
    if s.is_empty() || !s.chars().all(|c| MATH.contains(c)) || (!s.contains("sqrt(") && !s.contains("pi")) {
        return s.to_string();
    }
    radicals(s)
}

/// `Pretty.radicals`: sqrt(2) → √2, sqrt(x+1) → √(x+1), pi → π.
pub fn radicals(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("sqrt(") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 5..];
        let close = after.find(')');
        let short = close.filter(|&j| {
            let arg = &after[..j];
            !arg.is_empty()
                && (arg.chars().all(|c| c.is_ascii_digit() || c == '.')
                    || (arg.chars().count() == 1
                        && arg.chars().all(|c| c.is_ascii_alphabetic() || c == 'π')))
        });
        match short {
            Some(j) => {
                out.push('√');
                out.push_str(&after[..j]);
                rest = &after[j + 1..];
            }
            None => {
                out.push_str("√(");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("pi", "π")
}

// ---- what shows ----

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    /// 24, the answer
    Display,
    /// 20, the halves of a dual and a solve's letter
    Part,
    /// 17, the ≈
    Row,
    /// 13, messages
    Body,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Answer,
    Muted,
    /// the steady answer: muted at half
    Steady,
}

/// One piece of the answer: its text, size and colour, what a click copies (none: not a button) and its padding.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub text: String,
    pub font: Font,
    pub ink: Ink,
    pub copy: Option<String>,
    pub pad: (f32, f32),
}

fn spec(text: impl Into<String>, font: Font, ink: Ink, copy: Option<String>, pad: (f32, f32)) -> Spec {
    Spec {
        text: text.into(),
        font,
        ink,
        copy,
        pad,
    }
}

/// What the slot shows for the model as it is, left to right.
pub fn specs(m: &BarModel) -> Vec<Spec> {
    let live = m.live.as_ref();
    let blank = live.is_none_or(is_blank);
    if blank {
        // .live-steady: never the answer's colour, gone after a pause
        if let Some(s) = &m.answer.steady
            && !m.field.text().trim().is_empty()
        {
            return vec![spec(pretty(m, s), Font::Display, Ink::Steady, None, (PAD_X, 0.0))];
        }
        return Vec::new();
    }
    let Some(a) = live else { return Vec::new() };
    let figs = m.settings.sig_figs.clamp(2, 16) as f64;
    if let Some(solve) = &a.solve {
        if solve.outcome != SolveOutcome::Roots {
            // no roots: a message
            return vec![spec(&a.display, Font::Body, Ink::Muted, None, (PAD_X, 0.0))];
        }
        if solve.roots.len() > 1 && !has_dual(m) {
            // .live-roots: the letter copies them all, each root itself at full precision
            let parts: Vec<&str> = a.display.split(", ").filter(|p| *p != "…").collect();
            let labels: Vec<String> = if parts.len() == solve.roots.len() {
                parts.iter().map(|p| p.to_string()).collect()
            } else {
                solve.roots.iter().map(|r| format_number(*r, figs)).collect()
            };
            let mut out = vec![spec(
                format!("{} =", solve.variable),
                Font::Part,
                Ink::Muted,
                Some(a.display.clone()),
                (2.0, 6.0),
            )];
            for (k, root) in solve.roots.iter().enumerate() {
                if k > 0 {
                    out.push(spec(",", Font::Display, Ink::Answer, None, (0.0, 4.0)));
                }
                out.push(spec(
                    radicals_if_math(&core_answer::pretty_roots(&labels[k])),
                    Font::Display,
                    Ink::Answer,
                    Some(format!("{root}")),
                    (3.0, 3.0),
                ));
            }
            if solve.more || parts.len() < a.display.split(", ").count() {
                out.push(spec(", …", Font::Display, Ink::Answer, None, (0.0, 0.0)));
            }
            return out;
        }
    }
    if a.improper {
        return vec![spec(&a.display, Font::Body, Ink::Muted, None, (PAD_X, 0.0))];
    }
    // `x =` before a single root (the web's varLabel)
    let lead = a
        .solve
        .as_ref()
        .filter(|s| s.outcome == SolveOutcome::Roots && !s.variable.is_empty())
        .map(|s| {
            spec(
                format!("{} =", s.variable),
                Font::Part,
                Ink::Muted,
                Some(a.display.clone()),
                (2.0, 0.0),
            )
        });
    let mut out: Vec<Spec> = lead.into_iter().collect();
    if has_dual(m)
        && let Some(e) = exact(m)
    {
        // .live-dual: two 20 pt halves around a 17 pt ≈, each its own copy
        out.push(spec(pretty_text(e), Font::Part, Ink::Answer, Some(e.to_string()), (PAD_X, PAD_X)));
        out.push(spec("≈", Font::Row, Ink::Muted, None, (0.0, 0.0)));
        out.push(spec(
            pretty(m, &a.display),
            Font::Part,
            Ink::Answer,
            Some(a.display.clone()),
            (PAD_X, 0.0),
        ));
        return out;
    }
    let shown = shown_answer(m);
    out.push(spec(pretty(m, &shown), Font::Display, Ink::Answer, Some(shown), (PAD_X, 0.0)));
    out
}

// ---- layout ----

/// A piece laid out: its text layout and width, where it sits (set by `place`) and what a click copies.
#[derive(Clone, Debug)]
pub struct Item {
    pub layout: IDWriteTextLayout,
    pub w: f32,
    pub ink: Ink,
    pub copy: Option<String>,
    pub pad: (f32, f32),
    /// the piece's box (padding included), once placed
    pub rect: Rect,
}

/// The answer as drawn: its pieces, the copied flash, and how wide it wants to be.
#[derive(Clone, Debug, Default)]
pub struct AnswerLayout {
    pub items: Vec<Item>,
    pub flash: Option<Item>,
    /// wanted width (0 for none)
    pub width: f32,
}

fn format_of(gfx: &Gfx, font: Font) -> &IDWriteTextFormat {
    match font {
        Font::Display => &gfx.fonts.display,
        Font::Part => &gfx.fonts.answer_part,
        Font::Row => &gfx.fonts.row,
        Font::Body => &gfx.fonts.body,
    }
}

fn font_size(font: Font) -> f32 {
    match font {
        Font::Display => size::DISPLAY,
        Font::Part => size::ANSWER_PART,
        Font::Row => size::ROW,
        Font::Body => size::BODY,
    }
}

/// (text, font, scale bits, cut bits, the layout, its width)
type Cached = (String, Font, u32, u32, IDWriteTextLayout, f32);

thread_local! {
    /// text layouts by (text, font, scale, cut width): a keystroke that leaves the answer as it was makes none
    static LAYOUTS: RefCell<Vec<Cached>> =
        const { RefCell::new(Vec::new()) };
    static TABULAR: RefCell<Option<IDWriteTypography>> = const { RefCell::new(None) };
}

const LAYOUT_CACHE: usize = 24;

/// Drops the cached layouts.
pub fn trim() {
    LAYOUTS.with(|c| c.borrow_mut().clear());
}

/// The hidden trim (app.rs): the answer's, the tape's and the cheat sheet's layouts, the typeset geometries and
/// Typst's caches go; all come back on the next showing as they are needed.
pub fn trim_hidden(app: &App) {
    trim();
    crate::history::trim();
    crate::cheats::trim();
    crate::typst_row::trim(&app.engine);
}

/// Tabular figures, so digits don't jitter as the answer changes (the Mac's monospaced digits).
fn tabular(gfx: &Gfx) -> Option<IDWriteTypography> {
    TABULAR.with(|t| {
        let mut t = t.borrow_mut();
        if t.is_none() {
            // SAFETY: a plain factory call and a setter on the new object
            *t = unsafe {
                gfx.dw.CreateTypography().ok().inspect(|typo| {
                    let _ = typo.AddFontFeature(DWRITE_FONT_FEATURE {
                        nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
                        parameter: 1,
                    });
                })
            };
        }
        t.clone()
    })
}

/// A one-line layout of `text` at `font` × `scale`, cut with an ellipsis at `cut` DIPs (0: not cut), and its width.
fn text_layout(gfx: &Gfx, text: &str, font: Font, scale: f32, cut: f32) -> Option<(IDWriteTextLayout, f32)> {
    let key = (scale.to_bits(), cut.to_bits());
    if let Some(hit) = LAYOUTS.with(|c| {
        c.borrow()
            .iter()
            .find(|e| e.0 == text && e.1 == font && e.2 == key.0 && e.3 == key.1)
            .map(|e| (e.4.clone(), e.5))
    }) {
        return Some(hit);
    }
    let format = format_of(gfx, font);
    let layout = gfx.layout(text, format, cut, 36.0)?;
    let len = text.encode_utf16().count() as u32;
    let range = DWRITE_TEXT_RANGE {
        startPosition: 0,
        length: len,
    };
    // SAFETY: setters on a live layout
    unsafe {
        if (scale - 1.0).abs() > 1e-3 {
            let _ = layout.SetFontSize(font_size(font) * scale, range);
        }
        if let Some(t) = tabular(gfx) {
            let _ = layout.SetTypography(&t, range);
        }
        if cut > 0.0 {
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            if let Ok(sign) = gfx.dw.CreateEllipsisTrimmingSign(format) {
                let _ = layout.SetTrimming(&trimming, &sign);
            }
        }
    }
    let w = measure_layout(&layout).0;
    let w = if cut > 0.0 { w.min(cut) } else { w };
    LAYOUTS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= LAYOUT_CACHE {
            c.remove(0);
        }
        c.push((text.to_string(), font, key.0, key.1, layout.clone(), w));
    });
    Some((layout, w))
}

fn lay(gfx: &Gfx, specs: &[Spec], scale: f32, cut: f32) -> Option<Vec<Item>> {
    specs
        .iter()
        .map(|s| {
            let (layout, w) = text_layout(gfx, &s.text, s.font, scale, cut)?;
            Some(Item {
                layout,
                w,
                ink: s.ink,
                copy: s.copy.clone(),
                pad: s.pad,
                rect: Rect::default(),
            })
        })
        .collect()
}

fn total(items: &[Item]) -> f32 {
    items.iter().map(|i| i.w + i.pad.0 + i.pad.1).sum()
}

/// The answer's pieces fitted to `max_w`: at their sizes, else shrunk (to half at most), else the one piece cut.
pub fn measure(gfx: &Gfx, m: &BarModel, max_w: f32) -> AnswerLayout {
    let specs = specs(m);
    if specs.is_empty() || max_w <= 0.0 {
        return AnswerLayout::default();
    }
    let flash = if m.answer.copied {
        text_layout(gfx, "✓ copied", Font::Body, 11.0 / size::BODY, 0.0).map(|(layout, w)| Item {
            layout,
            w,
            ink: Ink::Muted,
            copy: None,
            pad: (7.0, 7.0 + 6.0),
            rect: Rect::default(),
        })
    } else {
        None
    };
    let flash_w = flash.as_ref().map_or(0.0, |f| f.w + f.pad.0 + f.pad.1);
    let room = (max_w - flash_w).max(1.0);
    let Some(mut items) = lay(gfx, &specs, 1.0, 0.0) else {
        return AnswerLayout::default();
    };
    let natural = total(&items);
    if natural > room {
        // useFitFont / minimumScaleFactor(0.5): shrink before cutting
        let scale = (room / natural).clamp(0.5, 1.0) * 0.995;
        if let Some(shrunk) = lay(gfx, &specs, scale, 0.0) {
            items = shrunk;
        }
        if total(&items) > room && specs.len() == 1 {
            let pads = specs[0].pad.0 + specs[0].pad.1;
            if let Some(cut) = lay(gfx, &specs, 0.5, (room - pads).max(1.0)) {
                items = cut;
            }
        }
    }
    let width = (total(&items) + flash_w).max(MIN_W).min(max_w.max(MIN_W));
    AnswerLayout {
        items,
        flash,
        width,
    }
}

/// Places the pieces right-aligned in `r` (the slot), the flash before them.
pub fn place(l: &mut AnswerLayout, r: Rect) {
    let mut x = r.right();
    for item in l.items.iter_mut().rev() {
        let w = item.w + item.pad.0 + item.pad.1;
        x -= w;
        item.rect = Rect::new(x, r.y + (r.h - 32.0) / 2.0, w, 32.0);
    }
    if let Some(f) = &mut l.flash {
        let w = f.w + f.pad.0 + f.pad.1;
        x -= w;
        f.rect = Rect::new(x, r.y + (r.h - 18.0) / 2.0, w, 18.0);
    }
}

/// What a click at (x, y) copies, if it is on a piece that copies.
pub fn hit(l: &AnswerLayout, x: f32, y: f32) -> Option<String> {
    l.items
        .iter()
        .find(|i| i.copy.is_some() && i.rect.contains(x, y))
        .and_then(|i| i.copy.clone())
}

fn ink(c: &Canvas, ink: Ink, copied: bool) -> Color {
    let t = c.theme;
    match ink {
        Ink::Answer if copied => t.answer_hover,
        Ink::Answer => t.answer,
        Ink::Muted => t.muted,
        Ink::Steady => with_alpha(t.muted, 0.5),
    }
}

pub fn paint(c: &Canvas, l: &AnswerLayout, r: Rect) {
    if l.items.is_empty() || r.w <= 0.0 {
        return;
    }
    c.push_clip(Rect::new(r.x, r.y - 4.0, r.w, r.h + 8.0));
    let copied = l.flash.is_some();
    for item in &l.items {
        let x = item.rect.x + item.pad.0;
        c.text(&item.layout, x, r.y, ink(c, item.ink, copied));
    }
    if let Some(f) = &l.flash {
        // .live-check: caption, accent-hover, on the bar's own colour so it reads over anything under it
        c.fill_round(f.rect.inset(0.0, 0.0), size::RADIUS_S, c.theme.bg);
        let pill = Rect::new(f.rect.x, f.rect.y, f.rect.w - 6.0, f.rect.h);
        c.fill_round(pill, size::RADIUS_S, c.theme.bg);
        c.text_plain(&f.layout, f.rect.x + f.pad.0, r.y, c.theme.accent_hover);
    }
    c.pop_clip();
}

// ---- the live answer landing ----

/// On the engine thread, with the live answer: the closed-form search's future, when the answer has a job for it.
pub fn closed_form(engine: &Arc<Engine>, a: &LiveAnswer) -> Option<ClosedJob> {
    let job = a.closed_form_job.clone()?;
    let token = CancelToken::new();
    let (e, t) = (engine.clone(), token.clone());
    let future: ClosedFuture = Box::pin(async move { e.closed_form(job, t).await });
    Some(ClosedJob { token, future })
}

/// A live answer landed on the UI thread (still the lane's newest): the steady answer, the model, the closed form
/// and the typeset row.
pub fn landed(
    app: &mut App,
    typed: String,
    answer: LiveAnswer,
    chained: bool,
    closed: Option<ClosedJob>,
) {
    let m = &mut app.model;
    if m.field.text() != typed {
        return;
    }
    let was_real = live_of(m).is_some();
    if is_blank(&answer)
        && was_real
        && !typed.is_empty()
        && qcalc_core::app::steady::is_edit_of(&typed, &m.answer.last_live_text)
    {
        // keep the last real answer up, dimmed, while the edit is half-typed
        m.answer.steady = Some(shown_answer(m));
        timer::start(timer::STEADY, qcalc_core::app::steady::STEADY_MS as u32);
    } else if !is_blank(&answer) || typed.is_empty() {
        m.answer.steady = None;
    }
    if !is_blank(&answer) {
        m.answer.last_live_text = typed.clone();
    }
    m.set_live(&typed, answer, chained);
    m.answer.closed = closed.map(|job| Closed { text: typed, job });
    poll(app);
    crate::typst_row::refresh(&mut app.model, &app.engine);
}

/// The closed form's waker: a WM_ENGINE to the host, whose handler polls again.
struct Wake2Host(usize);

impl Wake for Wake2Host {
    fn wake(self: Arc<Self>) {
        // SAFETY: posting to our host window from whichever thread finished the work
        let _ = unsafe { PostMessageW(Some(HWND(self.0 as *mut _)), WM_ENGINE, WPARAM(0), LPARAM(0)) };
    }
}

/// Polls the closed-form search (on WM_ENGINE): a form found for the line as it still is replaces `live.exact`.
pub fn poll(app: &mut App) {
    let host = app.host.0 as usize;
    let m = &mut app.model;
    let Some(c) = &mut m.answer.closed else { return };
    let waker = Waker::from(Arc::new(Wake2Host(host)));
    let Poll::Ready(found) = c.job.future.as_mut().poll(&mut Context::from_waker(&waker)) else {
        return;
    };
    let text = std::mem::take(&mut c.text);
    m.answer.closed = None;
    if let Ok(Some(exact)) = found
        && !exact.trim().is_empty()
        && m.field.text() == text
    {
        crate::qlog!("closed form for {text:?}: {exact}");
        m.answer.upgraded_exact = Some(exact);
        m.touch();
        crate::typst_row::refresh(&mut app.model, &app.engine);
    }
}

// ---- Tab ----

/// Tab: the engine's answerForms for the answer as shown, cycled (useAnswerForms); Shift+Tab goes back.
pub fn cycle_forms(m: &mut BarModel, engine: &EngineThread, back: bool) {
    let Some(a) = live_of(m).cloned() else { return };
    let Some(value_json) = a.value_json.clone() else {
        return;
    };
    m.touch();
    let n = m.answer.forms.len();
    if n > 0 {
        m.answer.form_index = (m.answer.form_index + if back { n - 1 } else { 1 }) % n;
        crate::typst_row::refresh(m, engine);
        return;
    }
    let typed = m.field.text().to_string();
    let options = m.options();
    let source = FormSource {
        value_json,
        display: a.display.clone(),
        exact: exact(m).map(str::to_string),
        meas: a.meas.clone(),
        sig_figs: options.sig_figs,
        sig_fig_mode: options.sig_fig_mode,
        rationalize: options.rationalize,
    };
    engine.submit(Lane::Forms, move |cx| {
        let display = source.display.clone();
        let views = cx.engine.answer_forms(source);
        let list = forms_list(display, views.into_iter().map(|v| v.display));
        done(move |app| {
            let m = &mut app.model;
            if m.field.text() != typed {
                return;
            }
            m.answer.form_index = if list.len() > 1 {
                if back { list.len() - 1 } else { 1 }
            } else {
                0
            };
            m.answer.forms = list;
            m.touch();
            crate::typst_row::refresh(&mut app.model, &app.engine);
        })
    });
}

/// The display, then the engine's other forms, without repeats or `undefined`.
pub fn forms_list(display: String, others: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    std::iter::once(display)
        .chain(others)
        .filter(|f| !f.contains("undefined") && seen.insert(f.trim().to_string()))
        .collect()
}

// ---- copy ----

/// What a copied answer is, with "copy unitless" applied (`UnitlessCopy.copied`).
pub fn copied_text(unitless: bool, s: &str) -> String {
    if unitless {
        core_answer::unitless_answer(s)
    } else {
        s.to_string()
    }
}

/// Copies `value` (a piece clicked) and flashes.
pub fn copy(m: &mut BarModel, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    m.to_clipboard = Some(copied_text(m.settings.copy_unitless, value));
    flash(m);
}

/// Ctrl+C with nothing selected: the answer as shown.
pub fn copy_shown(m: &mut BarModel) {
    let shown = shown_answer(m);
    copy(m, &shown);
}

/// Ctrl+Shift+C: `line = answer`, the line as Typst when "copy Typst" is on.
pub fn copy_line(m: &mut BarModel) {
    if live_of(m).is_none() {
        return;
    }
    let line = qcalc_core::app::typst_math::copied_equation(m.field.text().trim(), m.settings.typst_copy);
    m.to_clipboard = Some(format!("{line} = {}", shown_answer(m)));
    flash(m);
}

/// The field's selection, copied as Typst when that setting is on (`copiedEquation`).
pub fn copy_selection(m: &mut BarModel) {
    let sel = m.field.selected_text().to_string();
    m.to_clipboard = Some(qcalc_core::app::typst_math::copied_equation(&sel, m.settings.typst_copy));
}

fn flash(m: &mut BarModel) {
    m.answer.copied = true;
    m.touch();
    timer::start(timer::COPIED, COPIED_MS);
}

// ---- one-shot thread timers ----

pub mod timer {
    //! SetTimer with no window: the callback runs from the UI thread's loop, changes the model, and posts WM_ENGINE
    //! to the host so the bar lays out and paints (app.rs runs `after` for every host message).
    use std::cell::Cell;

    use super::*;

    pub const STEADY: usize = 0;
    pub const COPIED: usize = 1;
    /// the recent view's next row to expire (the tape shrinks while the bar is up)
    pub const RECENT: usize = 2;
    /// the line resting before it is typeset (typst_row.rs)
    pub const TYPESET: usize = 3;

    thread_local! {
        static IDS: [Cell<usize>; 4] = const { [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)] };
        /// when the RECENT timer is due, ms since 1970
        static RECENT_AT: Cell<i64> = const { Cell::new(0) };
    }

    /// After a frame with the tape in its recent view: a timer for when its oldest row leaves it
    /// (`nextRecentExpiry`), only while there is one to go.
    pub fn recent_expiry(m: &BarModel) {
        let now = crate::model::now_ms();
        let next = if m.settings.history_show == "recent" && !m.tape_open {
            let rows = &m.rows.rows;
            rows[m.recent_start(now)..]
                .iter()
                .filter_map(|r| r.at)
                .map(|at| at + crate::model::RECENT_MS + 1)
                .min()
        } else {
            None
        };
        let due = next.unwrap_or(0);
        if RECENT_AT.with(Cell::get) == due {
            return;
        }
        RECENT_AT.with(|c| c.set(due));
        match next {
            Some(at) => start(RECENT, (at - now).clamp(1000, i64::from(u32::MAX)) as u32),
            None => stop(RECENT),
        }
    }

    pub fn start(which: usize, ms: u32) {
        stop(which);
        // SAFETY: a thread timer with our callback
        let id = unsafe { SetTimer(None, 0, ms, Some(fired)) };
        IDS.with(|ids| ids[which].set(id));
    }

    pub fn stop(which: usize) {
        let id = IDS.with(|ids| ids[which].replace(0));
        if id != 0 {
            // SAFETY: our own thread timer
            let _ = unsafe { KillTimer(None, id) };
        }
    }

    unsafe extern "system" fn fired(_: HWND, _: u32, id: usize, _: u32) {
        let which = IDS.with(|ids| ids.iter().position(|c| c.get() == id));
        // SAFETY: our own thread timer (one-shot)
        let _ = unsafe { KillTimer(None, id) };
        let Some(which) = which else { return };
        IDS.with(|ids| ids[which].set(0));
        let host = with_app(|app| {
            match which {
                STEADY => app.model.answer.steady = None,
                COPIED => app.model.answer.copied = false,
                // nothing to draw yet: the picture comes back from the Typst thread
                TYPESET => {
                    crate::typst_row::compile(&mut app.model, &app.engine);
                    return None;
                }
                _ => RECENT_AT.with(|c| c.set(0)),
            }
            app.model.touch();
            Some(app.host)
        });
        if let Some(Some(host)) = host {
            // SAFETY: our own window; its handler lays out and paints
            let _ = unsafe { PostMessageW(Some(host), WM_ENGINE, WPARAM(0), LPARAM(0)) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_list_dedupes() {
        let l = forms_list(
            "0.5".into(),
            ["1/2", " 0.5", "undefined m", "5e-1"].into_iter().map(String::from),
        );
        assert_eq!(l, ["0.5", "1/2", "5e-1"]);
    }

    fn live(display: &str, exact: Option<&str>) -> LiveAnswer {
        LiveAnswer {
            display: display.into(),
            pretty: String::new(),
            exact: exact.map(String::from),
            dual_label: String::new(),
            number: None,
            unit: None,
            kind: qcalc_ffi::AnswerKind::Expression,
            blank: None,
            completion: None,
            solve: None,
            closed_form_job: None,
            time_letters: None,
            quantity: None,
            variable: None,
            meas: None,
            value_json: None,
            expr: String::new(),
            phrase: false,
            improper: false,
            json: "{}".into(),
        }
    }

    fn model(text: &str, a: LiveAnswer) -> BarModel {
        let mut m = BarModel::new(crate::store::Settings::default());
        m.field.reset(text);
        m.live = Some(a);
        m.live_for = text.into();
        m
    }

    fn texts(m: &BarModel) -> Vec<(String, Option<String>)> {
        specs(m).into_iter().map(|s| (s.text, s.copy)).collect()
    }

    #[test]
    fn what_the_slot_shows() {
        // exact ≈ decimal, each half its own copy
        let mut m = model("sqrt(8)", live("2.82843", Some("2sqrt(2)")));
        assert_eq!(
            texts(&m),
            [
                ("2√2".into(), Some("2sqrt(2)".into())),
                ("≈".into(), None),
                ("2.82843".into(), Some("2.82843".into())),
            ]
        );
        assert_eq!(shown_answer(&m), "2sqrt(2)");
        m.settings.answer_form = "approx".into();
        assert_eq!(shown_answer(&m), "2.82843");
        // Tab's form replaces the dual
        m.answer.forms = vec!["2.82843".into(), "2sqrt(2)".into(), "2.82843e0".into()];
        m.answer.form_index = 2;
        assert_eq!(texts(&m), [("2.82843e0".into(), Some("2.82843e0".into()))]);
        // a closed form found later replaces live.exact
        let mut m = model("x", live("1.64493", None));
        assert!(!has_dual(&m));
        m.answer.upgraded_exact = Some("π^2/6".into());
        assert!(has_dual(&m));
        // blank mid-edit: the steady answer, not a button
        let mut m = model("2+", live("", None));
        m.answer.steady = Some("4".into());
        let s = specs(&m);
        assert_eq!((s[0].text.as_str(), s[0].ink, s[0].copy.is_none()), ("4", Ink::Steady, true));
        m.field.reset("");
        assert!(specs(&m).is_empty());
    }

    #[test]
    fn solves() {
        let mut a = live("1, 2, 3", None);
        a.solve = Some(qcalc_ffi::SolveInfo {
            variable: "x".into(),
            roots: vec![1.0, 2.0, 3.0],
            outcome: SolveOutcome::Roots,
            more: false,
        });
        let m = model("x^3-6x^2+11x-6=0", a);
        let t = texts(&m);
        assert_eq!(t[0], ("x =".into(), Some("1, 2, 3".into())));
        assert_eq!(t[1], ("1".into(), Some("1".into())));
        assert_eq!(t[2].1, None);
        assert_eq!(t[3], ("2".into(), Some("2".into())));
        assert_eq!(t.len(), 6);
        let mut a = live("no real solution", None);
        a.solve = Some(qcalc_ffi::SolveInfo {
            variable: "x".into(),
            roots: vec![],
            outcome: SolveOutcome::None,
            more: false,
        });
        let s = specs(&model("x^2=-4", a));
        assert_eq!((s.len(), s[0].font, s[0].copy.is_none()), (1, Font::Body, true));
    }

    #[test]
    fn fit_and_flash() {
        let gfx = Gfx::new().expect("DirectWrite");
        let mut m = model("2^100", live("1.26765e30", None));
        let plain = measure(&gfx, &m, 400.0);
        assert!(plain.flash.is_none());
        m.answer.copied = true;
        let flashed = measure(&gfx, &m, 400.0);
        // the flash is counted in, so it shows whole
        assert!(flashed.flash.is_some() && flashed.width > plain.width);
        // a long answer shrinks before it is cut
        let long = "123456789012345678901234567890123456789";
        let m = model("x", live(long, None));
        let full = measure(&gfx, &m, 2000.0);
        let fitted = measure(&gfx, &m, 300.0);
        assert!(fitted.width <= 300.0 + 0.5);
        assert!(fitted.items[0].w < full.items[0].w);
    }

    #[test]
    fn pretty_like_the_mac() {
        assert_eq!(pretty_text("2sqrt(2)"), "2√2");
        assert_eq!(pretty_text("sqrt(2+3)"), "√(2+3)");
        assert_eq!(pretty_text("3sqrt(π)"), "3√π");
        assert_eq!(pretty_text("-1234.5"), "−1,234.5");
        assert_eq!(pretty_text("5 km"), "5 km");
        assert_eq!(pretty_text("e - 2"), "e − 2");
    }

    #[test]
    fn unitless_copies() {
        assert_eq!(copied_text(true, "15 m²"), "15");
        assert_eq!(copied_text(false, "15 m²"), "15 m²");
        assert_eq!(copied_text(true, "$10.00"), "$10.00");
    }
}
