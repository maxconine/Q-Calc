//! The tape (history) above the line: its view, its keys and clicks, and its persistence.
//!
//! Persistence goes through the engine, off the UI thread (Lane::Misc): `load` at launch (`Engine::load_history`
//! reads `<config dir>\history.json`, migrating legacy rows) and `save` after every change
//! (`Engine::save_history` writes what `persistable_history` keeps). The model holds the rows as `engine::Rows`.
//!
//! The view (`HistoryTape.tsx`, `HistoryTape` in BarView.swift): rows of 36 DIPs, at most 5 in sight, the tape's
//! own colour and a hairline under it, scrolled by the wheel or the touchpad. Each row is the expression (13,
//! muted) left and its answers (17) right in up to 46% of the width, as cells (`cells`): a solve's letter (copies
//! every root) and each root (inserts itself), `exact ≈ approx` (each inserts its side), or the one answer
//! (matrices, complex numbers and lists are the engine's text). A system row's expression reopens its panel
//! (through `BarModel::insert_row`, where systems.rs hooks in).
//!
//! Keys (from model.rs): ↑ ↓ walk the rows and keep to the side ← → last chose (`land`), ← → step through the
//! row's picks (`historyPicks`: the expression, then each answer), ↵ inserts the pick, Alt+↵ the other half
//! (`enter_row`), Ctrl+Backspace deletes the row and Ctrl+Z puts it back. Text layouts are cached per row, so a
//! keystroke redraws the tape without making any.

use std::cell::RefCell;
use std::ops::Range;

use qcalc_core::app::answer::{self as core_answer, AnswerForm, HistoryInsert};
use qcalc_ffi::{HistoryKind, HistoryRow, SolveOutcome};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
use windows::Win32::Graphics::DirectWrite::{IDWriteTextFormat, IDWriteTextLayout};
use windows::Win32::Graphics::Gdi::ScreenToClient;

use crate::app::App;
use crate::engine::{EngineThread, Lane, Rows, done};
use crate::model::{BarModel, now_ms};
use crate::qlog;
use crate::render::{Canvas, Gfx, Rect, measure, size, with_alpha};

/// Loads the tape at launch; the rows land in the model through `App::set_history_loaded`.
pub fn load(engine: &EngineThread) {
    engine.submit(Lane::Misc, |cx| {
        let t = std::time::Instant::now();
        let rows = cx.engine.load_history();
        qlog!(
            "history: {} rows loaded in {:.1} ms",
            rows.len(),
            t.elapsed().as_secs_f64() * 1000.0
        );
        done(move |app| app.set_history_loaded(rows))
    });
}

/// Writes the tape (the engine slims it and caps it).
pub fn save(engine: &EngineThread, rows: Vec<HistoryRow>) {
    engine.submit(Lane::Misc, move |cx| {
        if let Err(e) = cx.engine.save_history(rows) {
            qlog!("history: save failed: {e}");
        }
        None
    });
}

/// Tape padding (6 8) and its hairline.
pub const PAD_Y: f32 = 6.0;
pub const PAD_X: f32 = 8.0;

// ---- state ----

/// The tape's own state on the model (`BarModel.tape`).
#[derive(Clone, Debug, Default)]
pub struct TapeState {
    /// on the highlighted row, what ← → picked: 0 the expression, 1… its answers (historyPicks)
    pub pick: usize,
    /// ↑ ↓ keep to the side ← → last chose (historyPickOn)
    side_expr: bool,
    /// the row Ctrl+Backspace took, for Ctrl+Z, while the tape is still the one it came out of
    deleted: Option<(HistoryRow, usize, u64)>,
    /// DIPs the tape is scrolled up from its newest row
    pub scroll: f32,
    /// `?` / `help`: the cheat sheet shows where the tape is (cheats.rs)
    pub help: bool,
    /// the global shortcut, for the cheat sheet's first row
    pub hotkey: String,
    last_text: String,
}

fn form(m: &BarModel) -> AnswerForm {
    if m.settings.answer_form == "approx" {
        AnswerForm::Approx
    } else {
        AnswerForm::Exact
    }
}

fn figs(m: &BarModel) -> f64 {
    m.settings.sig_figs.clamp(2, 16) as f64
}

fn json(row: &HistoryRow) -> serde_json::Value {
    qcalc_ffi::history::row_json(row)
}

/// The line changed: typing past `?` keeps only the new text (afterHelpInput); the cheat sheet follows the line.
pub fn text_changed(m: &mut BarModel) {
    let now = m.field.text().to_string();
    let next = qcalc_core::app::onboarding::after_help_input(&m.tape.last_text, &now);
    if next != now {
        m.field.set_text(&next);
    }
    m.tape.help = qcalc_core::app::onboarding::is_help_command(&next);
    m.tape.last_text = next;
}

/// The ← → picks of a row (`historyPicks`).
pub fn picks(m: &BarModel, row: &HistoryRow) -> core_answer::HistoryPicks {
    core_answer::history_picks(&json(row), form(m), figs(m))
}

/// ↑ ↓ landed on row `selected`: the pick on the side last chosen, and the row in sight.
pub fn land(m: &mut BarModel, fresh: bool) {
    if fresh {
        m.tape.side_expr = false;
    }
    m.tape.help = false;
    let Some(i) = m.selected else { return };
    let Some(row) = m.rows.rows.get(i).cloned() else {
        return;
    };
    m.tape.pick = if m.tape.side_expr {
        0
    } else {
        picks(m, &row).start
    };
    // the row in sight: scroll the least that shows it
    let visible = m.visible(now_ms());
    if visible.contains(&i) {
        let k = visible.len().min(size::TAPE_VISIBLE) as f32;
        let from_bottom = (visible.end - 1 - i) as f32 * size::TAPE_ROW;
        let s = &mut m.tape.scroll;
        if from_bottom < *s {
            *s = from_bottom;
        }
        if from_bottom + size::TAPE_ROW > *s + k * size::TAPE_ROW {
            *s = from_bottom + size::TAPE_ROW - k * size::TAPE_ROW;
        }
    }
}

/// ← → on a highlighted row (onSide).
pub fn pick_side(m: &mut BarModel, dir: i32) {
    let Some(row) = m.selected.and_then(|i| m.rows.rows.get(i)).cloned() else {
        return;
    };
    let n = picks(m, &row).answers.len() as i32;
    let next = (m.tape.pick as i32 + dir).clamp(0, n) as usize;
    m.tape.side_expr = next == 0;
    m.tape.pick = next;
    m.touch();
}

/// A piece into the line at the caret; the tape closes.
pub fn insert_piece(m: &mut BarModel, piece: &str) {
    m.selected = None;
    m.tape_open = false;
    m.field.insert(piece);
    m.field.break_run();
    m.touch();
}

/// ↵ on the highlighted row inserts what ← → picked (insertHistoryPick); Alt+↵ the other half
/// (insertHistoryOther). A system row reopens its equations.
pub fn enter_row(m: &mut BarModel, engine: &EngineThread, alt: bool) {
    let Some(i) = m.selected else { return };
    let Some(row) = m.rows.rows.get(i).cloned() else {
        return;
    };
    // a system row goes through the model's insert_row, whose systems hook reopens its panel
    if row.kind == Some(HistoryKind::System) {
        m.insert_row(i, false);
        m.text_changed(engine);
        return;
    }
    let piece = if alt {
        let expr = matches!(
            row.kind,
            Some(HistoryKind::Definition | HistoryKind::Function)
        ) || m.settings.history_insert == "answer";
        if expr {
            row.expr.clone()
        } else {
            core_answer::insertable_history_answer(&json(&row), form(m), figs(m))
        }
    } else {
        let answers = picks(m, &row).answers;
        match m.tape.pick {
            0 => row.expr.clone(),
            k if !answers.is_empty() => answers[k.min(answers.len()) - 1].clone(),
            _ => row.expr.clone(),
        }
    };
    insert_piece(m, &piece);
    m.text_changed(engine);
}

/// Ctrl+Backspace on a highlighted row: it goes (saved), and Ctrl+Z can put it back.
pub fn delete_selected(m: &mut BarModel, engine: &EngineThread) -> bool {
    let Some(i) = m.selected else { return false };
    let mut next = m.rows.rows.to_vec();
    if i >= next.len() {
        return false;
    }
    let row = next.remove(i);
    m.rows = Rows::new(next.clone());
    m.tape.deleted = Some((row, i, m.rows.generation));
    m.selected = if next.is_empty() {
        None
    } else {
        Some(i.min(next.len() - 1))
    };
    if m.selected.is_none() {
        m.tape_open = false;
    } else {
        land(m, false);
    }
    m.touch();
    save(engine, next);
    m.evaluate_live(engine);
    true
}

/// Ctrl+Z after a delete (and nothing saved since): the row comes back where it was.
pub fn undo_delete(m: &mut BarModel, engine: &EngineThread) -> bool {
    let Some((row, i, generation)) = m.tape.deleted.take() else {
        return false;
    };
    if generation != m.rows.generation {
        return false;
    }
    let mut next = m.rows.rows.to_vec();
    let at = i.min(next.len());
    next.insert(at, row);
    m.rows = Rows::new(next.clone());
    m.tape_open = true;
    m.selected = Some(at);
    land(m, false);
    m.touch();
    save(engine, next);
    m.evaluate_live(engine);
    true
}

// ---- cells ----

/// What a cell does when clicked.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    None,
    Insert(String),
    Copy(String),
    OpenSystem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// 13 muted, ink when picked
    Expr,
    /// 17, the answer colour
    Answer,
    /// 13 muted: a solve's letter, the ≈, a message
    Quiet,
}

/// One part of a row, left to right: the expression first, then its answers.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub look: Look,
    pub act: Act,
    /// which ← → pick it is
    pub pick: Option<usize>,
}

fn cell(text: impl Into<String>, look: Look, act: Act, pick: Option<usize>) -> Cell {
    Cell {
        text: text.into(),
        look,
        act,
        pick,
    }
}

/// The row as the web's HistoryTape draws it.
pub fn cells(row: &HistoryRow, form: AnswerForm, figs: f64, insert: HistoryInsert) -> Vec<Cell> {
    let j = json(row);
    let system = row.kind == Some(HistoryKind::System);
    let mut out = vec![cell(
        row.expr.replace("sqrt(", "√("),
        Look::Expr,
        if system {
            Act::OpenSystem
        } else {
            Act::Insert(row.expr.clone())
        },
        Some(0),
    )];
    let solved_for = row
        .solve
        .as_ref()
        .filter(|s| s.outcome == SolveOutcome::Roots)
        .map(|s| format!("{} =", s.variable));
    let roots = core_answer::solve_roots(&j, figs);
    let root_cells = |out: &mut Vec<Cell>, lead: bool| {
        let roots = roots.clone().unwrap_or_default();
        let shown: Vec<&str> = row.display.split(", ").filter(|p| *p != "…").collect();
        if lead && let Some(s) = &row.solve {
            out.push(cell(
                format!("{} =", s.variable),
                Look::Quiet,
                Act::Copy(row.display.clone()),
                None,
            ));
        }
        for (k, root) in roots.iter().enumerate() {
            let label = if shown.len() == roots.len() {
                shown[k].to_string()
            } else {
                root.clone()
            };
            if k > 0 {
                out.push(cell(", ", Look::Answer, Act::None, None));
            }
            out.push(cell(
                roots_text(&label),
                Look::Answer,
                Act::Insert(root.clone()),
                Some(k + 1),
            ));
        }
        if row.solve.as_ref().is_some_and(|s| s.more) {
            out.push(cell(", …", Look::Answer, Act::None, None));
        }
    };
    if core_answer::row_has_dual_answer(&j)
        && let Some(exact) = &row.exact
    {
        if let Some(s) = &solved_for {
            out.push(cell(s, Look::Quiet, Act::None, None));
        }
        let insert_exact = core_answer::solve_insert(&j, figs)
            .unwrap_or_else(|| core_answer::insertable_answer(exact, None, 12.0));
        let exact_text = if row.solve.is_some() {
            roots_text(exact)
        } else {
            crate::answer::pretty_text(exact)
        };
        out.push(cell(
            exact_text,
            Look::Answer,
            Act::Insert(insert_exact),
            roots.is_none().then_some(1),
        ));
        out.push(cell("≈", Look::Quiet, Act::None, None));
        if roots.is_some() {
            root_cells(&mut out, false);
        } else {
            let approx = if row.solve.is_some() {
                roots_text(&row.display)
            } else {
                crate::answer::pretty_text(&row.display)
            };
            out.push(cell(
                approx,
                Look::Answer,
                Act::Insert(core_answer::insertable_history_answer(
                    &j,
                    AnswerForm::Approx,
                    figs,
                )),
                Some(2),
            ));
        }
        return out;
    }
    if roots.is_some() {
        root_cells(&mut out, true);
        return out;
    }
    // one answer: a definition's word, a solve's list or message as written, else the pretty answer
    let shown = core_answer::visible_answer(&row.display, row.exact.as_deref(), form);
    let message = row
        .solve
        .as_ref()
        .is_some_and(|s| s.outcome != SolveOutcome::Roots);
    let text = if row.kind == Some(HistoryKind::Definition) {
        shown.to_string()
    } else if row.solve.is_some() {
        roots_text(shown)
    } else {
        crate::answer::pretty_text(shown)
    };
    if let Some(s) = &solved_for {
        out.push(cell(s, Look::Quiet, Act::None, None));
    }
    let act = if row.kind == Some(HistoryKind::Definition) {
        Act::Insert(core_answer::insertable_history_reuse(
            &j, form, insert, figs,
        ))
    } else {
        Act::Insert(core_answer::insertable_history_answer(&j, form, figs))
    };
    if !text.is_empty() && !system {
        out.push(cell(
            text,
            if message { Look::Quiet } else { Look::Answer },
            act,
            Some(1),
        ));
    }
    out
}

fn roots_text(s: &str) -> String {
    crate::answer::radicals_if_math(&core_answer::pretty_roots(s))
}

// ---- layout ----

/// Where the tape is: the rows in the view, the ones drawn, and the scroll.
#[derive(Clone, Debug, Default)]
pub struct TapeLayout {
    /// every row the view shows (the recent ones, or all)
    pub visible: Range<usize>,
    /// the rows at least partly in sight
    pub drawn: Range<usize>,
    pub scroll: f32,
    pub height: f32,
}

/// The tape for the rows the view shows: at most 5 in sight, scrolled.
pub fn layout(model: &BarModel, visible: Range<usize>) -> TapeLayout {
    let n = visible.len();
    let k = n.min(size::TAPE_VISIBLE);
    if k == 0 {
        return TapeLayout {
            drawn: visible.end..visible.end,
            visible,
            ..TapeLayout::default()
        };
    }
    let max = (n - k) as f32 * size::TAPE_ROW;
    let scroll = model.tape.scroll.clamp(0.0, max);
    // rows from the bottom: the window is [scroll, scroll + k rows)
    let first_from_bottom = (scroll / size::TAPE_ROW).floor() as usize;
    let last_from_bottom = ((scroll + k as f32 * size::TAPE_ROW) / size::TAPE_ROW).ceil() as usize;
    let end = visible.end - first_from_bottom.min(n);
    let start = visible
        .end
        .saturating_sub(last_from_bottom)
        .max(visible.start);
    TapeLayout {
        drawn: start..end,
        scroll,
        height: height(k),
        visible,
    }
}

/// The rows to draw (at most 5, the selected one in sight) of the visible range, unscrolled.
pub fn shown(model: &BarModel, visible: Range<usize>) -> Range<usize> {
    layout(model, visible).drawn
}

/// The tape's height for `rows` shown rows (0 for none).
pub fn height(rows: usize) -> f32 {
    if rows == 0 {
        0.0
    } else {
        rows as f32 * size::TAPE_ROW + 2.0 * PAD_Y + 1.0
    }
}

/// Where row `i` is, scrolled (it may be partly out of the window).
pub fn row_rect(width: f32, t: &TapeLayout, i: usize) -> Rect {
    let k = t.visible.len().min(size::TAPE_VISIBLE) as f32;
    let bottom = PAD_Y + k * size::TAPE_ROW;
    let y = bottom - (t.visible.end - i) as f32 * size::TAPE_ROW + t.scroll;
    Rect::new(PAD_X, y, width - 2.0 * PAD_X, size::TAPE_ROW)
}

fn window(t: &TapeLayout) -> Rect {
    let k = t.visible.len().min(size::TAPE_VISIBLE) as f32;
    Rect::new(0.0, PAD_Y, f32::MAX, k * size::TAPE_ROW)
}

/// A row ready to draw: its cells' layouts, placed in a row of the cached width.
#[derive(Clone)]
struct RowView {
    key: (String, String, String, Option<String>, bool, u32, u32),
    /// (cell, its layout, x from the row's left, width)
    cells: Vec<(Cell, IDWriteTextLayout, f32, f32)>,
}

thread_local! {
    static ROWS: RefCell<Vec<RowView>> = const { RefCell::new(Vec::new()) };
}

const ROW_CACHE: usize = 48;

/// Drops the cached row layouts (the hidden trim).
pub fn trim() {
    ROWS.with(|r| r.borrow_mut().clear());
}

fn format_of(gfx: &Gfx, look: Look) -> &IDWriteTextFormat {
    match look {
        Look::Answer => &gfx.fonts.row,
        Look::Expr | Look::Quiet => &gfx.fonts.body,
    }
}

/// .tape-row grid: the expression in `1fr`, the answers in minmax(120px, 46%), 12 apart, 8 in.
fn build(gfx: &Gfx, m: &BarModel, row: &HistoryRow, w: f32) -> Option<RowView> {
    let insert = if m.settings.history_insert == "answer" {
        HistoryInsert::Answer
    } else {
        HistoryInsert::Expr
    };
    let cs = cells(row, form(m), figs(m), insert);
    let inner = w - 16.0;
    let col_max = (inner * 0.46).max(120.0);
    let gap = 6.0;
    // the answers, right to left, natural widths; the first (leftmost) one cut when they don't fit
    let mut answers: Vec<(Cell, IDWriteTextLayout, f32)> = Vec::new();
    for c in cs.iter().skip(1) {
        let l = gfx.layout(&c.text, format_of(gfx, c.look), 0.0, size::TAPE_ROW)?;
        let lw = measure(&l).0;
        answers.push((c.clone(), l, lw));
    }
    // a root list's commas sit tight against the roots
    let gaps: Vec<f32> = (0..answers.len())
        .map(|k| {
            let tight = |i: usize| answers[i].0.text.starts_with(',');
            if k == 0 || tight(k) {
                // nothing before a comma: it sits on its root as written ("−2, 2")
                0.0
            } else if tight(k - 1) {
                // the comma's own space is the gap after it
                1.0
            } else {
                gap
            }
        })
        .collect();
    let natural: f32 = answers.iter().map(|a| a.2).sum::<f32>() + gaps.iter().sum::<f32>();
    if natural > col_max && answers.len() == 1 {
        let c = &answers[0].0;
        let l = gfx.layout_in(
            &c.text,
            format_of(gfx, c.look),
            col_max,
            size::TAPE_ROW,
            windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING,
        )?;
        answers[0].1 = l;
        answers[0].2 = col_max;
    }
    let used: f32 = answers.iter().map(|a| a.2).sum::<f32>() + gaps.iter().sum::<f32>();
    let col = used.clamp(if answers.is_empty() { 0.0 } else { 120.0 }, col_max);
    let mut out = Vec::new();
    let expr_w = (inner - col - 12.0).max(0.0);
    let e = &cs[0];
    let el = gfx.layout_in(
        &e.text,
        format_of(gfx, e.look),
        expr_w,
        size::TAPE_ROW,
        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
    )?;
    let ew = measure(&el).0.min(expr_w);
    out.push((e.clone(), el, 8.0, ew));
    let mut x = 8.0 + inner;
    let mut placed: Vec<(Cell, IDWriteTextLayout, f32, f32)> = Vec::new();
    for ((c, l, lw), g) in answers.into_iter().zip(gaps).rev() {
        x -= lw;
        placed.push((c, l, x, lw));
        x -= g;
    }
    placed.reverse();
    out.extend(placed);
    Some(RowView {
        key: key_of(m, row, w),
        cells: out,
    })
}

fn key_of(
    m: &BarModel,
    row: &HistoryRow,
    w: f32,
) -> (String, String, String, Option<String>, bool, u32, u32) {
    (
        row.id.clone(),
        row.expr.clone(),
        row.display.clone(),
        row.exact.clone(),
        m.settings.answer_form == "approx",
        (figs(m) as u32) << 1 | u32::from(m.settings.history_insert == "answer"),
        w.to_bits(),
    )
}

fn view(gfx: &Gfx, m: &BarModel, row: &HistoryRow, w: f32) -> Option<RowView> {
    let key = key_of(m, row, w);
    if let Some(v) = ROWS.with(|r| r.borrow().iter().find(|v| v.key == key).cloned()) {
        return Some(v);
    }
    let v = build(gfx, m, row, w)?;
    ROWS.with(|r| {
        let mut r = r.borrow_mut();
        if r.len() >= ROW_CACHE {
            r.remove(0);
        }
        r.push(v.clone());
    });
    Some(v)
}

/// The cell under a point: its row, the cell's index in the row (None: the row's background) and what it does.
pub fn cell_at(
    gfx: &Gfx,
    m: &BarModel,
    width: f32,
    t: &TapeLayout,
    x: f32,
    y: f32,
) -> Option<(usize, Option<usize>, Act)> {
    if !window(t).contains(x, y) {
        return None;
    }
    t.drawn.clone().find_map(|i| {
        let r = row_rect(width, t, i);
        if !r.contains(x, y) {
            return None;
        }
        let row = m.rows.rows.get(i)?;
        let v = view(gfx, m, row, r.w)?;
        // the expression takes its column; an answer cell its own box (4 around it)
        let local = x - r.x;
        let exprs = &v.cells[0];
        let first_answer = v.cells.get(1).map_or(r.w, |c| c.2);
        if local < first_answer - 6.0 {
            return Some((i, Some(0), exprs.0.act.clone()));
        }
        v.cells
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, c)| local >= c.2 - 4.0 && local < c.2 + c.3 + 4.0)
            .map(|(k, c)| (i, Some(k), c.0.act.clone()))
            .or(Some((i, None, Act::None)))
    })
}

/// The cell under a point: its row and what it does.
pub fn hit(
    gfx: &Gfx,
    m: &BarModel,
    width: f32,
    t: &TapeLayout,
    x: f32,
    y: f32,
) -> Option<(usize, Act)> {
    cell_at(gfx, m, width, t, x, y).map(|(i, _, act)| (i, act))
}

/// A tape cell's tooltip (the web's HistoryTape titles), None for a cell that does nothing.
pub fn tip(row: &HistoryRow, cells: &[Cell], k: usize, form: AnswerForm) -> Option<&'static str> {
    let c = cells.get(k)?;
    if k == 0 {
        return Some(match row.kind {
            Some(HistoryKind::System) => "Edit equations",
            Some(HistoryKind::Definition) => "Insert word at the cursor",
            _ => "Insert expression at the cursor",
        });
    }
    match &c.act {
        Act::None => None,
        Act::Copy(_) => Some("Copy all the solutions"),
        Act::OpenSystem => Some("Edit equations"),
        Act::Insert(_) => {
            let roots = cells.iter().filter(|c| c.text.starts_with(',')).count() > 0
                || cells.iter().any(|c| matches!(c.act, Act::Copy(_)));
            let dual = cells.iter().any(|c| c.text == "≈");
            Some(if dual {
                // the ≈ splits the exact side (before it) from the approximation (after)
                let approx = cells.iter().position(|c| c.text == "≈").is_some_and(|q| k > q);
                if approx && roots {
                    "Insert this solution at the cursor"
                } else if approx {
                    "Insert approximation at the cursor"
                } else {
                    "Insert exact value at the cursor"
                }
            } else if roots {
                "Insert this solution at the cursor"
            } else if row.kind == Some(HistoryKind::Definition) {
                "Insert word at the cursor"
            } else if form == AnswerForm::Exact && row.exact.is_some() {
                "Insert exact value at the cursor"
            } else {
                "Insert approximation at the cursor"
            })
        }
    }
}

/// The tooltip and the cell's box (window DIPs) for the cell under a point.
pub fn tip_at(app: &App, row: usize, k: usize) -> Option<(&'static str, Rect)> {
    let t = &app.bar.layout.tape;
    let r = row_rect(app.bar.layout.width, t, row);
    let hr = app.model.rows.rows.get(row)?;
    let v = view(&app.gfx, &app.model, hr, r.w)?;
    let cells: Vec<Cell> = v.cells.iter().map(|c| c.0.clone()).collect();
    let text = tip(hr, &cells, k, form(&app.model))?;
    let (_, _, x, w) = &v.cells[k];
    Some((text, Rect::new(r.x + x, r.y, *w, r.h)))
}

/// A click on the tape: the cell's piece goes in at the caret (or is copied, or a system reopens).
pub fn click(app: &mut App, x: f32, y: f32) -> bool {
    let t = app.bar.layout.tape.clone();
    let Some((i, act)) = hit(&app.gfx, &app.model, app.bar.layout.width, &t, x, y) else {
        return false;
    };
    match act {
        Act::None => {}
        Act::Insert(piece) => {
            if !piece.is_empty() {
                insert_piece(&mut app.model, &piece);
                app.text_changed();
            }
        }
        Act::Copy(text) => crate::answer::copy(&mut app.model, &text),
        Act::OpenSystem => {
            app.model.insert_row(i, false);
            app.text_changed();
        }
    }
    true
}

/// The wheel or the touchpad over the bar: the tape scrolls (120 a notch is a row).
pub fn wheel(app: &mut App, wp: WPARAM, lp: LPARAM) {
    let delta = f32::from((wp.0 >> 16) as u16 as i16);
    let mut pt = POINT {
        x: i32::from((lp.0 & 0xffff) as u16 as i16),
        y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    };
    // SAFETY: our window, a live point
    let _ = unsafe { ScreenToClient(app.bar.hwnd, &mut pt) };
    let y = pt.y as f32 / app.bar.scale();
    let t = &app.bar.layout.tape;
    if t.visible.len() <= size::TAPE_VISIBLE || y > t.height {
        return;
    }
    let max = (t.visible.len() - size::TAPE_VISIBLE) as f32 * size::TAPE_ROW;
    let next = (t.scroll + delta / 120.0 * size::TAPE_ROW).clamp(0.0, max);
    if (next - app.model.tape.scroll).abs() > 0.01 {
        app.model.tape.scroll = next;
        app.model.touch();
    }
}

// ---- paint ----

/// .tape-row.selected .picked: a 1 pt ring and a tint, both the answer colour
fn picked_mark(c: &Canvas, r: Rect) {
    c.fill_round(r, 4.0, with_alpha(c.theme.answer, 0.14));
    let rr = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: r.x + 0.5,
            top: r.y + 0.5,
            right: r.right() - 0.5,
            bottom: r.bottom() - 0.5,
        },
        radiusX: 4.0,
        radiusY: 4.0,
    };
    let brush = c.brush(with_alpha(c.theme.answer, 0.55));
    // SAFETY: inside the frame's BeginDraw/EndDraw
    unsafe { c.rt().DrawRoundedRectangle(&rr, brush, 1.0, None) };
}

pub fn paint(c: &Canvas, model: &BarModel, t: &TapeLayout) {
    if t.drawn.is_empty() {
        return;
    }
    let th = c.theme;
    c.fill(Rect::new(0.0, 0.0, c.width, t.height), th.bg_tape);
    c.fill(Rect::new(0.0, t.height - 1.0, c.width, 1.0), th.line);
    let win = window(t);
    c.push_clip(Rect::new(0.0, win.y, c.width, win.h));
    // the pointer's row and cell (hover.rs): .tape-row:hover, .tape-q:hover, .tape-a:hover, .tape-root:hover
    let hovered = crate::hover::tape_cell();
    for i in t.drawn.clone() {
        let Some(row) = model.rows.rows.get(i) else {
            continue;
        };
        let r = row_rect(c.width, t, i);
        let selected = model.selected == Some(i);
        let over = hovered.filter(|h| h.0 == i);
        if selected {
            c.fill_round(r, size::RADIUS_S, th.fill_selected);
        } else if over.is_some() {
            c.fill_round(r, size::RADIUS_S, th.fill_hover);
        }
        let Some(v) = view(c.gfx, model, row, r.w) else {
            continue;
        };
        let roots = v.cells.iter().any(|c| c.0.text.starts_with(','));
        for (k, (cell, layout, x, w)) in v.cells.iter().enumerate() {
            let picked = selected && cell.pick == Some(model.tape.pick);
            let hot = over.is_some_and(|h| h.1 == Some(k)) && cell.act != Act::None;
            if picked {
                picked_mark(c, Rect::new(r.x + x - 3.0, r.y + 4.0, w + 6.0, r.h - 8.0));
            } else if hot && k > 0 && (roots || matches!(cell.act, Act::Copy(_))) {
                c.fill_round(
                    Rect::new(r.x + x - 3.0, r.y + 4.0, w + 6.0, r.h - 8.0),
                    4.0,
                    with_alpha(th.answer, 0.12),
                );
            }
            let ink = match cell.look {
                Look::Expr if picked || hot => th.ink,
                Look::Quiet if hot => th.answer_hover,
                Look::Expr | Look::Quiet => th.muted,
                Look::Answer if hot => th.answer_hover,
                Look::Answer => th.answer,
            };
            c.text(layout, r.x + x, r.y, ink);
        }
    }
    c.pop_clip();
    crate::answer::timer::recent_expiry(model);
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcalc_ffi::SolveInfo;

    fn row(expr: &str, display: &str, exact: Option<&str>) -> HistoryRow {
        HistoryRow {
            id: "r".into(),
            expr: expr.into(),
            display: display.into(),
            exact: exact.map(String::from),
            n: display.parse().ok(),
            meas: None,
            quantity: None,
            kind: None,
            equations: None,
            fn_name: None,
            fn_params: None,
            fn_body: None,
            solve: None,
            at: None,
        }
    }

    fn acts(c: &[Cell]) -> Vec<(String, Act, Option<usize>)> {
        c.iter()
            .map(|c| (c.text.clone(), c.act.clone(), c.pick))
            .collect()
    }

    #[test]
    fn one_answer() {
        let c = cells(
            &row("2+2", "4", None),
            AnswerForm::Exact,
            6.0,
            HistoryInsert::Expr,
        );
        assert_eq!(
            acts(&c),
            [
                ("2+2".into(), Act::Insert("2+2".into()), Some(0)),
                ("4".into(), Act::Insert("4".into()), Some(1)),
            ]
        );
    }

    #[test]
    fn dual_answer() {
        let c = cells(
            &row("sqrt(2)", "1.41421", Some("√2")),
            AnswerForm::Exact,
            6.0,
            HistoryInsert::Expr,
        );
        assert_eq!(c[0].text, "√(2)");
        assert_eq!(c[1].text, "√2");
        assert_eq!(c[1].pick, Some(1));
        assert_eq!(c[2].text, "≈");
        assert_eq!(c[3].pick, Some(2));
        assert_eq!(c[3].act, Act::Insert("1.41421".into()));
    }

    #[test]
    fn solve_rows() {
        let mut r = row("x^2=4", "-2, 2", None);
        r.solve = Some(SolveInfo {
            variable: "x".into(),
            roots: vec![-2.0, 2.0],
            outcome: SolveOutcome::Roots,
            more: false,
        });
        let c = cells(&r, AnswerForm::Exact, 6.0, HistoryInsert::Expr);
        assert_eq!(c[1].text, "x =");
        assert_eq!(c[1].act, Act::Copy("-2, 2".into()));
        assert_eq!(c[2].act, Act::Insert("-2".into()));
        assert_eq!(c[2].pick, Some(1));
        assert_eq!(c[3].text, ", ");
        assert_eq!(c[3].act, Act::None);
        assert_eq!(c[4].text, "2");
        assert_eq!(c[4].pick, Some(2));
        let mut none = row("x^2=-4", "no real solution", None);
        none.solve = Some(SolveInfo {
            variable: "x".into(),
            roots: vec![],
            outcome: SolveOutcome::None,
            more: false,
        });
        let c = cells(&none, AnswerForm::Exact, 6.0, HistoryInsert::Expr);
        assert_eq!(c[1].look, Look::Quiet);
        assert_eq!(c[1].act, Act::Insert(String::new()));
    }

    /// The tape's keys through the model as the bar routes them (no window; the engine thread runs but its
    /// answers never come back, so only the synchronous parts are checked).
    #[test]
    fn tape_keys() {
        use crate::keys::Mods;
        use crate::model::Outcome;
        const UP: u32 = 0x26;
        const LEFT: u32 = 0x25;
        const ENTER: u32 = 0x0D;
        const BACK: u32 = 0x08;
        let engine = EngineThread::start(
            qcalc_ffi::EngineConfig {
                is_windows: true,
                locale: "en_US".into(),
                data_dir: None,
                time_zone: None,
            },
            || {},
        );
        let mut m = BarModel::new(crate::store::Settings::default());
        m.rows = Rows::new(vec![
            row("2+2", "4", None),
            row("sqrt(8)", "2.82843", Some("2sqrt(2)")),
            row("255 in hex", "0xFF", None),
        ]);
        let none = Mods::default();
        let alt = Mods { alt: true, ..none };
        let ctrl = Mods { ctrl: true, ..none };
        let key = |m: &mut BarModel, vk: u32, mods: Mods| m.key_down(vk, mods, &engine, || None);
        // ↑ lands on the newest row's answer; ↵ inserts it
        key(&mut m, UP, none);
        assert_eq!((m.selected, m.tape.pick), (Some(2), 1));
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "0xFF");
        assert_eq!(m.selected, None);
        // Alt+↵ inserts the other half than the setting (expression by default, so the answer)
        m.field.reset("");
        key(&mut m, UP, none);
        key(&mut m, UP, none);
        assert_eq!(m.selected, Some(1));
        // a dual row lands on the exact side when the setting is exact
        assert_eq!(m.tape.pick, 1);
        key(&mut m, ENTER, alt);
        assert_eq!(m.field.text(), "2sqrt(2)");
        // ← picks the expression, and ↑ keeps to that side
        m.field.reset("");
        key(&mut m, UP, none);
        key(&mut m, LEFT, none);
        assert_eq!(m.tape.pick, 0);
        key(&mut m, UP, none);
        assert_eq!((m.selected, m.tape.pick), (Some(1), 0));
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "sqrt(8)");
        // Ctrl+Backspace deletes the highlighted row, Ctrl+Z puts it back
        m.field.reset("");
        key(&mut m, UP, none);
        key(&mut m, UP, none);
        assert_eq!(m.selected, Some(1));
        key(&mut m, BACK, ctrl);
        assert_eq!(m.rows.rows.len(), 2);
        assert_eq!(m.rows.rows[1].expr, "255 in hex");
        key(&mut m, 0x5A, ctrl);
        assert_eq!(m.rows.rows.len(), 3);
        assert_eq!(m.rows.rows[1].expr, "sqrt(8)");
        assert_eq!(m.selected, Some(1));
        // Ctrl+W asks the app to close a sheet or hide
        assert_eq!(key(&mut m, 0x57, ctrl), Outcome::CloseSheet);
        // `?` shows the cheat sheet; Esc closes it
        m.field.reset("");
        m.char16(u16::from(b'?'), &engine);
        assert!(m.tape.help);
        key(&mut m, 0x1B, none);
        assert!(!m.tape.help);
        engine.shutdown();
    }

    #[test]
    fn tips_say_what_a_click_does() {
        let f = AnswerForm::Exact;
        let one = row("2+2", "4", None);
        let c = cells(&one, f, 6.0, HistoryInsert::Expr);
        assert_eq!(tip(&one, &c, 0, f), Some("Insert expression at the cursor"));
        assert_eq!(tip(&one, &c, 1, f), Some("Insert approximation at the cursor"));
        let dual = row("sqrt(2)", "1.41421", Some("√2"));
        let c = cells(&dual, f, 6.0, HistoryInsert::Expr);
        assert_eq!(tip(&dual, &c, 1, f), Some("Insert exact value at the cursor"));
        assert_eq!(tip(&dual, &c, 2, f), None);
        assert_eq!(tip(&dual, &c, 3, f), Some("Insert approximation at the cursor"));
        let mut solve = row("x^2=4", "-2, 2", None);
        solve.solve = Some(SolveInfo {
            variable: "x".into(),
            roots: vec![-2.0, 2.0],
            outcome: SolveOutcome::Roots,
            more: false,
        });
        let c = cells(&solve, f, 6.0, HistoryInsert::Expr);
        assert_eq!(tip(&solve, &c, 1, f), Some("Copy all the solutions"));
        assert_eq!(tip(&solve, &c, 2, f), Some("Insert this solution at the cursor"));
        assert_eq!(tip(&solve, &c, 3, f), None);
        let mut sys = row("x+y=3; x-y=1", "x = 2, y = 1", None);
        sys.kind = Some(HistoryKind::System);
        let c = cells(&sys, f, 6.0, HistoryInsert::Expr);
        assert_eq!(tip(&sys, &c, 0, f), Some("Edit equations"));
    }

    #[test]
    fn system_rows_reopen() {
        let mut r = row("x+y=3; x-y=1", "x = 2, y = 1", None);
        r.kind = Some(HistoryKind::System);
        let c = cells(&r, AnswerForm::Exact, 6.0, HistoryInsert::Expr);
        assert_eq!(c[0].act, Act::OpenSystem);
    }
}
