//! Systems of equations: `sys2`…`sys5` (or `sys 3`, `system 4`) and ↵ open one equation field per row under the
//! composer, solved live as they're typed and saved as one `system` history row that reopens them. Spec: the Mac's
//! Systems.swift (`SysCommand`, `SystemPanel`), the web's SystemPanel.tsx and QuickCalc.tsx (`onEnter`, `onDown`,
//! the Ctrl+Backspace reset), `.sys` in src/index.css.
//!
//! - State (`SysState`, `BarModel::systems`): the rows (input.rs `Field`s, so editing, undo and word moves are the
//!   line's own), which row has the keys (`focus`, None for the main line), the engine's answer. Nothing exists
//!   until ↵ opens it; `follow` (from `evaluate_live`) closes it when the line stops saying `sys N`.
//! - Solving: `engine/system` `solveLive` through `Engine::call_json` on `Lane::System`, so only the newest rows
//!   are solved; the answer shows in the bar's answer slot (`slot`), partial as soon as the equations typed so
//!   far settle a value, units carried through (the settings' default units).
//! - Keys: ↵ goes to the next row and saves from the last; ↑ ↓ (and Tab) move between rows, ↑ from the first back
//!   to the line, ↓ from the line to the first row; Esc closes the panel and clears the line; Ctrl+Backspace (or
//!   Ctrl+Delete) on the line resets to calculating. Ctrl+V of several equations into a row fills the system from
//!   that row on (`pastedEquations`).
//!
//! For other packages: `open_with` (a worksheet paste of equations into the line, owned by input.rs), `paste_main`
//! (the line's paste, deciding by itself), `reopen` (a history row of kind `system`).

use std::cell::RefCell;

use qcalc_ffi::{HistoryKind, HistoryRow};
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

use crate::app::App;
use crate::engine::{EngineThread, Lane, Rows, done};
use crate::input::Field;
use crate::keys::Mods;
use crate::model::{BarModel, Outcome, now_ms};
use crate::render::{Canvas, Rect, caret_x, hit, measure, size, with_alpha};

pub const MAX_ROWS: usize = 5;
/// .sys: margin 0 12 12, padding 8; .sys-eq 40 high, 2 apart
const MARGIN: f32 = 12.0;
const PAD: f32 = 8.0;
const ROW_H: f32 = 40.0;
const ROW_GAP: f32 = 2.0;
const ROW_PAD_X: f32 = 12.0;

// ---- pure logic ----

/// sysCommand: `sys` / `system` with how many equations. Some(Ok(n)) for 1…5, Some(Err(hint)) for a `sys` line
/// that doesn't say a usable count yet, None for anything else.
pub fn count(text: &str) -> Option<Result<usize, &'static str>> {
    let t = text.trim();
    let lower = t.get(..3)?;
    if !lower.eq_ignore_ascii_case("sys") {
        return None;
    }
    let mut rest = &t[3..];
    if rest.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("tem")) {
        rest = &rest[3..];
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(Err("how many equations? sys2, sys3… up to sys5"));
    }
    if !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match rest.parse::<usize>() {
        Ok(n) if (1..=MAX_ROWS).contains(&n) => Some(Ok(n)),
        _ => Some(Err("up to 5 equations")),
    }
}

/// The rows as saved: trimmed, the empty ones left out of what's written.
fn written(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The history row a system saves as (`expr` `sys`, the equations, what the slot showed), or None when there's
/// nothing to save. `last` is the tape's last row: the same system twice in a row saves once.
fn row_for(
    lines: &[String],
    shown: &str,
    now: i64,
    last: Option<&HistoryRow>,
) -> Option<Option<HistoryRow>> {
    let shown = shown.trim();
    if written(lines).is_empty() || shown.is_empty() {
        return None;
    }
    let equations: Vec<String> = lines.iter().map(|l| l.trim().to_string()).collect();
    if let Some(last) = last
        && last.kind == Some(HistoryKind::System)
        && last.display == shown
        && last.equations.as_ref() == Some(&equations)
    {
        return Some(None);
    }
    Some(Some(HistoryRow {
        id: format!("{now}-sys"),
        expr: "sys".into(),
        display: shown.to_string(),
        exact: None,
        n: None,
        meas: None,
        quantity: None,
        kind: Some(HistoryKind::System),
        equations: Some(equations),
        fn_name: None,
        fn_params: None,
        fn_body: None,
        solve: None,
        at: Some(now),
    }))
}

// ---- state ----

/// The engine's answer for the rows (`SystemAnswer`).
#[derive(Clone, Debug, PartialEq)]
pub struct SysAnswer {
    pub display: String,
    /// why there is no solution, not a solution
    pub message: bool,
}

#[derive(Default)]
struct RowView {
    cached: Option<(String, IDWriteTextLayout)>,
    scroll: f32,
}

/// The open panel (`BarModel::systems`).
pub struct SysState {
    pub rows: Vec<Field>,
    /// the row with the keys; None while the line has them
    pub focus: Option<usize>,
    pub answer: Option<SysAnswer>,
    /// the rows changed and haven't been sent to the engine yet
    unsolved: bool,
    /// a drag-select in a row is under way
    selecting: bool,
    views: RefCell<Vec<RowView>>,
}

impl SysState {
    fn new(lines: &[String]) -> SysState {
        SysState {
            rows: lines
                .iter()
                .map(|l| {
                    let mut f = Field::default();
                    f.reset(l);
                    f
                })
                .collect(),
            focus: Some(0),
            answer: None,
            unsolved: true,
            selecting: false,
            views: RefCell::new(lines.iter().map(|_| RowView::default()).collect()),
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.rows.iter().map(|f| f.text().to_string()).collect()
    }
}

/// Whether a row (not the line) has the keys: the line's caret hides (window.rs).
pub fn focused(m: &BarModel) -> bool {
    m.systems.as_ref().is_some_and(|s| s.focus.is_some())
}

/// What the answer slot shows while the panel is open: the solution, or why there is none.
pub fn slot(m: &BarModel) -> Option<String> {
    let s = m.systems.as_ref()?;
    Some(s.answer.as_ref().map_or_else(String::new, |a| {
        if a.message {
            a.display.clone()
        } else {
            a.display.replace('-', "−")
        }
    }))
}

/// The slot says why there's no solution (drawn muted).
pub fn slot_is_message(m: &BarModel) -> bool {
    m.systems
        .as_ref()
        .and_then(|s| s.answer.as_ref())
        .is_some_and(|a| a.message)
}

// ---- opening, solving, saving ----

/// The line changed: the panel stays only while the line says `sys N` for its N; rows that changed are solved.
pub fn follow(m: &mut BarModel, engine: &EngineThread) {
    let Some(s) = &m.systems else { return };
    if count(m.field.text()) != Some(Ok(s.rows.len())) {
        close(m, engine);
        return;
    }
    if s.unsolved {
        solve(m, engine);
    }
}

fn close(m: &mut BarModel, engine: &EngineThread) {
    if m.systems.take().is_some() {
        engine.cancel(Lane::System);
        m.touch();
    }
}

/// The rows, opened with these equations (openSystem): the line says `sys N`, the first row has the keys.
pub fn open_lines(m: &mut BarModel, engine: &EngineThread, lines: &[String]) {
    let lines: Vec<String> = lines.iter().take(MAX_ROWS).cloned().collect();
    if lines.is_empty() {
        return;
    }
    m.selected = None;
    m.tape_open = false;
    m.systems = Some(SysState::new(&lines));
    m.field.set_text(&format!("sys {}", lines.len()));
    m.field.break_run();
    m.text_changed(engine);
}

/// A worksheet paste of equations into the line (input.rs owns the paste): opens them as `sys N`.
pub fn open_with(app: &mut App, equations: Vec<String>) {
    open_lines(&mut app.model, &app.engine, &equations);
}

/// A history row of kind `system` reopens its equations (openHistorySystem). True when row `i` is one and it
/// opened: the caller then runs `text_changed`, which solves them.
pub fn reopen(m: &mut BarModel, i: usize) -> bool {
    let Some(row) = m.rows.rows.get(i) else {
        return false;
    };
    if row.kind != Some(HistoryKind::System) {
        return false;
    }
    let Some(eqs) = row.equations.clone().filter(|e| !e.is_empty()) else {
        return false;
    };
    let eqs: Vec<String> = eqs.into_iter().take(MAX_ROWS).collect();
    m.selected = None;
    m.tape_open = false;
    m.systems = Some(SysState::new(&eqs));
    m.field.set_text(&format!("sys {}", eqs.len()));
    m.field.break_run();
    m.touch();
    true
}

/// solveLive for the rows as they are, off the UI thread; the answer lands only if the rows are still the same.
fn solve(m: &mut BarModel, engine: &EngineThread) {
    let Some(s) = &mut m.systems else { return };
    s.unsolved = false;
    let lines = s.lines();
    let angle = if m.settings.angle_mode == "rad" {
        "rad"
    } else {
        "deg"
    };
    let args = serde_json::json!([lines, m.settings.default_units, angle]).to_string();
    engine.submit(Lane::System, move |cx| {
        let answer = cx
            .engine
            .call_json("engine/system".into(), "solveLive".into(), args)
            .ok()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
            .and_then(|v| {
                Some(SysAnswer {
                    display: v.get("display")?.as_str()?.to_string(),
                    message: v.get("message") == Some(&serde_json::Value::Bool(true)),
                })
            });
        done(move |app| {
            if let Some(s) = &mut app.model.systems
                && s.lines() == lines
            {
                s.answer = answer;
                app.model.touch();
            }
        })
    });
}

/// Saves the system as one history row and goes back to calculating; says why when there's nothing to save.
fn commit(m: &mut BarModel, engine: &EngineThread) {
    let Some(s) = &m.systems else { return };
    let lines = s.lines();
    let message = s.answer.as_ref().is_some_and(|a| a.message);
    let shown = s
        .answer
        .as_ref()
        .map(|a| a.display.clone())
        .unwrap_or_default();
    let row = if message {
        None
    } else {
        row_for(&lines, &shown, now_ms(), m.rows.rows.last())
    };
    let Some(row) = row else {
        m.hint = Some("nothing to save yet · finish the equations, or esc to close".into());
        m.touch();
        return;
    };
    if let Some(row) = row {
        let mut rows = m.rows.rows.to_vec();
        rows.push(row);
        m.rows = Rows::new(rows.clone());
        crate::history::save(engine, rows);
    }
    reset(m, engine);
}

/// Back to calculating: the panel closed, the line empty and focused (resetToCalculate).
fn reset(m: &mut BarModel, engine: &EngineThread) {
    m.systems = None;
    engine.cancel(Lane::System);
    m.field.set_text("");
    m.text_changed(engine);
}

/// Ctrl+V of text with line breaks or `;` (into row `at`, or the line when None): two to five equations open as
/// a system after the rows before `at`; anything else goes in as plain text. Decided off the UI thread.
fn paste_lines(m: &mut BarModel, engine: &EngineThread, raw: String, at: Option<usize>) {
    let before: Vec<String> = match (at, &m.systems) {
        (Some(i), Some(s)) => s.lines().into_iter().take(i).collect(),
        _ => Vec::new(),
    };
    engine.submit(Lane::Misc, move |cx| {
        let clean = cx
            .engine
            .call_json(
                "lib/paste".into(),
                "cleanPastedText".into(),
                serde_json::json!([raw]).to_string(),
            )
            .ok()
            .and_then(|j| serde_json::from_str::<String>(&j).ok())
            .unwrap_or_else(|| raw.clone());
        let lines = cx
            .engine
            .call_json(
                "engine/system".into(),
                "pastedEquations".into(),
                serde_json::json!([clean]).to_string(),
            )
            .ok()
            .and_then(|j| serde_json::from_str::<Option<Vec<String>>>(&j).ok())
            .flatten();
        done(move |app| {
            let m = &mut app.model;
            match lines {
                Some(lines) if !lines.is_empty() => {
                    let all: Vec<String> = before.into_iter().chain(lines).collect();
                    open_lines(m, &app.engine, &all);
                }
                _ => match at {
                    Some(i) => {
                        if let Some(s) = &mut m.systems
                            && let Some(f) = s.rows.get_mut(i)
                        {
                            f.paste(&raw);
                            s.unsolved = true;
                            m.touch();
                            solve(m, &app.engine);
                        }
                    }
                    None => {
                        m.field.paste(&raw);
                        m.text_changed(&app.engine);
                    }
                },
            }
        })
    });
}

/// The line's paste (for input.rs): true when it took it (text with line breaks or `;`, decided off the UI
/// thread: equations open as a system, anything else lands in the line as a one-line paste).
pub fn paste_main(app: &mut App, raw: &str) -> bool {
    if !raw.contains(['\r', '\n', ';']) {
        return false;
    }
    paste_lines(&mut app.model, &app.engine, raw.to_string(), None);
    true
}

// ---- keys ----

const BACK: u32 = 0x08;
const TAB: u32 = 0x09;
const ENTER: u32 = 0x0D;
const ESC: u32 = 0x1B;
const END: u32 = 0x23;
const HOME: u32 = 0x24;
const LEFT: u32 = 0x25;
const UP: u32 = 0x26;
const RIGHT: u32 = 0x27;
const DOWN: u32 = 0x28;
const DELETE: u32 = 0x2E;

/// A key-down, before the line's own keys (model.rs). Ok when the panel took it; Err hands the clipboard reader
/// back for the line.
pub fn key_down<F: FnOnce() -> Option<String>>(
    m: &mut BarModel,
    vk: u32,
    mods: Mods,
    engine: &EngineThread,
    clipboard: F,
) -> Result<Outcome, F> {
    let plain = !mods.ctrl && !mods.alt && !mods.shift && !mods.win;
    let ctrl = mods.ctrl && !mods.alt;
    let Some(i) = m.systems.as_ref().and_then(|s| s.focus) else {
        // the line has the keys
        let opening = count(m.field.text());
        return match vk {
            ENTER if plain && m.selected.is_none() => {
                let Some(Ok(n)) = opening else {
                    return Err(clipboard);
                };
                match &mut m.systems {
                    Some(s) if s.rows.len() == n => {
                        // nothing typed yet: ↵ goes down to the first equation rather than saving an empty system
                        if s.rows.iter().all(|f| f.text().trim().is_empty()) {
                            s.focus = Some(0);
                            m.touch();
                        } else {
                            commit(m, engine);
                        }
                    }
                    _ => {
                        m.systems = Some(SysState::new(&vec![String::new(); n]));
                        m.hint = None;
                        m.touch();
                    }
                }
                Ok(Outcome::Handled)
            }
            DOWN if plain && m.systems.is_some() && m.selected.is_none() => {
                focus(m, 0);
                Ok(Outcome::Handled)
            }
            ESC if m.systems.is_some() => {
                reset(m, engine);
                Ok(Outcome::Handled)
            }
            BACK | DELETE if ctrl && !mods.shift && m.systems.is_some() => {
                reset(m, engine);
                Ok(Outcome::Handled)
            }
            _ => Err(clipboard),
        };
    };
    let n = m.systems.as_ref().map_or(0, |s| s.rows.len());
    match vk {
        ESC => {
            reset(m, engine);
            return Ok(Outcome::Handled);
        }
        ENTER => {
            if i + 1 < n {
                focus(m, i + 1);
            } else {
                commit(m, engine);
            }
            return Ok(Outcome::Handled);
        }
        UP if !ctrl => {
            if i == 0 {
                if let Some(s) = &mut m.systems {
                    s.focus = None;
                }
                m.touch();
            } else {
                focus(m, i - 1);
            }
            return Ok(Outcome::Handled);
        }
        DOWN if !ctrl => {
            if i + 1 < n {
                focus(m, i + 1);
            }
            return Ok(Outcome::Handled);
        }
        TAB if !ctrl => {
            if mods.shift {
                if i > 0 {
                    focus(m, i - 1);
                }
            } else if i + 1 < n {
                focus(m, i + 1);
            }
            return Ok(Outcome::Handled);
        }
        _ => {}
    }
    let Some(s) = &mut m.systems else {
        return Err(clipboard);
    };
    let f = &mut s.rows[i];
    if ctrl && !mods.shift && vk == 0x43 && f.has_selection() {
        m.to_clipboard = Some(f.selected_text().to_string());
        return Ok(Outcome::Handled);
    }
    if let Some(action) = m.keybinds.action(mods, vk) {
        return Ok(m.perform(action, engine));
    }
    let before = f.text().to_string();
    let shift = mods.shift;
    match vk {
        LEFT => f.left(ctrl, shift),
        RIGHT => f.right(ctrl, shift),
        HOME => f.home(shift),
        END => f.end(shift),
        BACK => {
            f.backspace(ctrl);
        }
        DELETE => {
            f.delete(ctrl);
        }
        0x41 if ctrl => f.select_all(),
        0x58 if ctrl => {
            if let Some(cut) = f.cut() {
                m.to_clipboard = Some(cut);
            }
        }
        0x56 if ctrl => {
            if let Some(text) = clipboard() {
                if text.contains(['\r', '\n', ';']) {
                    paste_lines(m, engine, text, Some(i));
                } else {
                    f.paste(&text);
                }
            }
        }
        0x5A if ctrl && shift => {
            f.redo();
        }
        0x5A if ctrl => {
            f.undo();
        }
        0x59 if ctrl => {
            f.redo();
        }
        _ => return Ok(Outcome::Ignored),
    }
    edited(m, engine, i, &before);
    Ok(Outcome::Handled)
}

fn focus(m: &mut BarModel, i: usize) {
    if let Some(s) = &mut m.systems
        && i < s.rows.len()
    {
        s.focus = Some(i);
        s.rows[i].end(false);
        m.touch();
    }
}

/// After a key in row `i`: repaint, and solve again if its text changed.
fn edited(m: &mut BarModel, engine: &EngineThread, i: usize, before: &str) {
    m.touch();
    let Some(s) = &mut m.systems else { return };
    if s.rows.get(i).is_some_and(|f| f.text() != before) {
        s.unsolved = true;
        m.hint = None;
        solve(m, engine);
    }
}

/// WM_CHAR while a row has the keys: true when it took it.
pub fn char16(m: &mut BarModel, unit: u16, engine: &EngineThread) -> bool {
    let Some(i) = m.systems.as_ref().and_then(|s| s.focus) else {
        return false;
    };
    let Some(s) = &mut m.systems else {
        return false;
    };
    let before = s.rows[i].text().to_string();
    s.rows[i].char16(unit);
    edited(m, engine, i, &before);
    true
}

/// The IME's text while a row has the keys.
pub fn insert_text(m: &mut BarModel, text: &str, engine: &EngineThread) -> bool {
    let Some(i) = m.systems.as_ref().and_then(|s| s.focus) else {
        return false;
    };
    let Some(s) = &mut m.systems else {
        return false;
    };
    let before = s.rows[i].text().to_string();
    s.rows[i].insert(text);
    s.rows[i].break_run();
    edited(m, engine, i, &before);
    true
}

// ---- layout and drawing ----

/// The panel's height (window.rs `Bar::layout`).
pub fn height(s: &SysState) -> f32 {
    let n = s.rows.len() as f32;
    2.0 * PAD + n * ROW_H + (n - 1.0).max(0.0) * ROW_GAP + MARGIN
}

/// Row `i`'s rect for the panel at `r`.
fn row_rect(r: Rect, i: usize) -> Rect {
    Rect::new(
        r.x + MARGIN + PAD,
        r.y + PAD + i as f32 * (ROW_H + ROW_GAP),
        r.w - 2.0 * (MARGIN + PAD),
        ROW_H,
    )
}

fn row_at(app: &App, x: f32, y: f32) -> Option<usize> {
    let r = app.bar.layout.systems?;
    let n = app.model.systems.as_ref()?.rows.len();
    (0..n).find(|&i| row_rect(r, i).contains(x, y))
}

/// The panel in `r`: a card of the tape's colour, a field per row (the focused one ringed in accent).
pub fn paint(c: &Canvas, s: &SysState, r: Rect, window_focused: bool) {
    let t = c.theme;
    let card = Rect::new(r.x + MARGIN, r.y, r.w - 2.0 * MARGIN, r.h - MARGIN);
    c.fill_round(card, size::RADIUS_CARD, t.bg_tape);
    // SAFETY: inside the frame; the rect is a local
    unsafe {
        let rr = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                left: card.x + 0.25,
                top: card.y + 0.25,
                right: card.right() - 0.25,
                bottom: card.bottom() - 0.25,
            },
            radiusX: size::RADIUS_CARD,
            radiusY: size::RADIUS_CARD,
        };
        let edge = if t.dark {
            crate::render::rgb(0xffffff, 0.08)
        } else {
            crate::render::rgb(0, 0.1)
        };
        c.target()
            .DrawRoundedRectangle(&rr, c.brush(edge), 0.5, None);
    }
    let mut views = s.views.borrow_mut();
    views.resize_with(s.rows.len(), RowView::default);
    for (i, (f, view)) in s.rows.iter().zip(views.iter_mut()).enumerate() {
        let rr = row_rect(r, i);
        c.fill_round(rr, size::RADIUS_S, t.bg);
        let has = s.focus == Some(i);
        if has {
            // SAFETY: as above
            unsafe {
                let ring = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                        left: rr.x + 0.5,
                        top: rr.y + 0.5,
                        right: rr.right() - 0.5,
                        bottom: rr.bottom() - 0.5,
                    },
                    radiusX: size::RADIUS_S,
                    radiusY: size::RADIUS_S,
                };
                c.target().DrawRoundedRectangle(
                    &ring,
                    c.brush(with_alpha(t.accent, 0.55)),
                    1.0,
                    None,
                );
            }
        }
        let text = rr.inset(ROW_PAD_X, 0.0);
        paint_row(
            c,
            f,
            view,
            text,
            has && window_focused,
            &format!("equation {}", i + 1),
        );
    }
}

fn paint_row(
    c: &Canvas,
    f: &Field,
    view: &mut RowView,
    r: Rect,
    caret_on: bool,
    placeholder: &str,
) {
    let t = c.theme;
    let font = &c.gfx.fonts.row;
    c.push_clip(Rect::new(r.x - 2.0, r.y, r.w + 4.0, r.h));
    if f.text().is_empty() {
        view.scroll = 0.0;
        if let Some(l) = c.gfx.layout(placeholder, font, 0.0, r.h) {
            c.text_plain(&l, r.x, r.y, with_alpha(t.muted, 0.8));
        }
    }
    let layout = match &view.cached {
        Some((s, l)) if s == f.text() => Some(l.clone()),
        _ => {
            let l = c.gfx.layout(f.text(), font, 0.0, r.h);
            if let Some(l) = &l {
                view.cached = Some((f.text().to_string(), l.clone()));
            }
            l
        }
    };
    let Some(layout) = layout else {
        c.pop_clip();
        return;
    };
    let caret = caret_x(&layout, f.to_utf16(f.caret()));
    let room = 4.0;
    if caret - view.scroll > r.w - room {
        view.scroll = caret - r.w + room;
    }
    if caret - view.scroll < 0.0 {
        view.scroll = caret;
    }
    let (w, _) = measure(&layout);
    if w - view.scroll < r.w - room {
        view.scroll = (w - r.w + room).max(0.0);
    }
    let x0 = r.x - view.scroll;
    if f.has_selection() {
        let (a, b) = f.selection();
        let (xa, xb) = (
            caret_x(&layout, f.to_utf16(a)),
            caret_x(&layout, f.to_utf16(b)),
        );
        c.fill_round(
            Rect::new(x0 + xa, r.y + r.h * 0.5 - 11.0, xb - xa, 22.0),
            3.0,
            t.selection,
        );
    }
    if !f.text().is_empty() {
        c.text(&layout, x0, r.y, t.ink);
    }
    if caret_on {
        let mid = r.y + r.h * 0.5;
        let half = size::ROW * 0.62;
        c.line(
            x0 + caret + 0.5,
            mid - half,
            x0 + caret + 0.5,
            mid + half,
            1.5,
            t.ink,
        );
    }
    c.pop_clip();
}

/// The UTF-16 position in row `i` under window point (x, y), from its last layout.
fn hit_row(app: &App, i: usize, x: f32, y: f32) -> u32 {
    let (Some(r), Some(s)) = (app.bar.layout.systems, &app.model.systems) else {
        return 0;
    };
    let views = s.views.borrow();
    let Some(view) = views.get(i) else { return 0 };
    let text = row_rect(r, i).inset(ROW_PAD_X, 0.0);
    match &view.cached {
        Some((_, l)) => hit(l, x - text.x + view.scroll, y - text.y),
        None => 0,
    }
}

// ---- the pointer ----

/// A press: in a row it takes the keys and places the caret (double-click: a word; Shift: extends; a drag
/// selects). A press anywhere else gives the keys back to the line. A press on the answer copies the solution.
/// True when the panel took the press.
pub fn click(app: &mut App, x: f32, y: f32, double: bool, shift: bool) -> bool {
    if app.model.systems.is_none() {
        return false;
    }
    if let Some(i) = row_at(app, x, y) {
        let pos = hit_row(app, i, x, y);
        let hwnd = app.bar.hwnd;
        let Some(s) = &mut app.model.systems else {
            return true;
        };
        s.focus = Some(i);
        let f = &mut s.rows[i];
        if double {
            let at = f.index_of_utf16(pos);
            f.select_word_at(at);
        } else {
            f.set_caret_utf16(pos, shift);
            s.selecting = true;
            // SAFETY: our bar
            unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd) };
        }
        app.model.touch();
        return true;
    }
    if app.bar.layout.systems.is_some_and(|r| r.contains(x, y)) {
        return true;
    }
    if app.bar.layout.answer.contains(x, y)
        && let Some(a) = app.model.systems.as_ref().and_then(|s| s.answer.clone())
        && !a.message
        && !a.display.is_empty()
    {
        app.model.to_clipboard = Some(a.display);
        app.model.hint = Some("copied".into());
        app.model.touch();
        return true;
    }
    if let Some(s) = &mut app.model.systems
        && s.focus.take().is_some()
    {
        app.model.touch();
    }
    false
}

/// A drag-select in a row.
pub fn mouse_move(app: &mut App, x: f32, y: f32) -> bool {
    let Some(i) = app
        .model
        .systems
        .as_ref()
        .filter(|s| s.selecting)
        .and_then(|s| s.focus)
    else {
        return false;
    };
    let pos = hit_row(app, i, x, y);
    if let Some(s) = &mut app.model.systems {
        s.rows[i].set_caret_utf16(pos, true);
    }
    app.model.touch();
    true
}

pub fn mouse_up(app: &mut App) -> bool {
    let Some(s) = &mut app.model.systems else {
        return false;
    };
    if !std::mem::take(&mut s.selecting) {
        return false;
    }
    // SAFETY: we hold the capture
    let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
    true
}

/// The I-beam over a row.
pub fn cursor(app: &App, x: f32, y: f32) -> Option<windows::core::PCWSTR> {
    row_at(app, x, y).map(|_| windows::Win32::UI::WindowsAndMessaging::IDC_IBEAM)
}

/// A one-line summary for the bench pipe's `state`.
pub fn debug(m: &BarModel) -> serde_json::Value {
    let Some(s) = &m.systems else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "rows": s.lines(), "focus": s.focus,
        "answer": s.answer.as_ref().map(|a| a.display.clone()), "message": s.answer.as_ref().is_some_and(|a| a.message),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sys_commands() {
        assert_eq!(count("sys2"), Some(Ok(2)));
        assert_eq!(count(" sys 3 "), Some(Ok(3)));
        assert_eq!(count("System 5"), Some(Ok(5)));
        assert_eq!(
            count("SYS"),
            Some(Err("how many equations? sys2, sys3… up to sys5"))
        );
        assert_eq!(count("sys 6"), Some(Err("up to 5 equations")));
        assert_eq!(count("sys 0"), Some(Err("up to 5 equations")));
        assert_eq!(count("sys x"), None);
        assert_eq!(count("sysx"), None);
        assert_eq!(count("2+2"), None);
        assert_eq!(count("sy"), None);
        assert_eq!(count("sÿs"), None);
    }

    #[test]
    fn a_system_saves_once_as_one_row() {
        let lines = vec!["x + y = 3".to_string(), " x - y = 1 ".to_string()];
        assert!(row_for(&lines, "", 5, None).is_none());
        assert!(row_for(&["".into(), " ".into()], "x = 2", 5, None).is_none());
        let row = row_for(&lines, "x = 2, y = 1", 5, None)
            .flatten()
            .expect("a row");
        assert_eq!(row.expr, "sys");
        assert_eq!(row.kind, Some(HistoryKind::System));
        assert_eq!(
            row.equations.as_deref(),
            Some(&["x + y = 3".to_string(), "x - y = 1".to_string()][..])
        );
        assert_eq!(row.id, "5-sys");
        assert_eq!(written(&lines), "x + y = 3; x - y = 1");
        // the same again right after: nothing new
        assert!(matches!(
            row_for(&lines, "x = 2, y = 1", 6, Some(&row)),
            Some(None)
        ));
    }
}
