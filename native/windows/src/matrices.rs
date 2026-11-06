//! Matrices on the bar (native only, native/MATRIX_PLAN.md Phase 5; spec: the Mac's Matrices.swift, PARITY.md).
//!
//! - The grid: `matrix 3x3` anywhere in a line shows `↵ opens a 3×3 grid` in the slot; ↵ opens rows × columns of
//!   cells in a card under the bar, written back into the line as `[a b; c d]` on every keystroke, so the answer
//!   stays live (`det(matrix 2x2)`). Ctrl+G opens the matrix under the caret (else the last, else a new 2×2), and
//!   again closes it. Tab / Shift+Tab along the rows, ↵ down the column (from the last row back to the line), arrows
//!   between cells (← → only from the ends of a cell's text, ↑ from the top row to the line), Esc closes, − + for
//!   rows and columns (1 to 8). Any other edit of the line closes it. Pure moves and writing: matrix_text.rs.
//! - The steps: an answer with row operations (rref, inv, solve, A \ b, det, rank) gets `show steps · N` under the
//!   preview; it (or Ctrl+R) opens a card of each operation with the matrix after it, typeset on the Typst thread.
//!   Esc shuts it; an emptied line starts with it shut. With the preview off, rows show the matrix as text.
//! - The slot: a matrix answer's text (`pretty`), its dual's shared label once (`without_dual_label`).
//!
//! Hooks: model.rs (`follow` in `evaluate_live`, `key_down`, `char16`, `insert_text`), answer.rs (`slot_hint`,
//! `pretty`, `refresh_steps` when an answer lands), typst_row.rs (`trim`), window.rs (`height`, `paint`,
//! `grid_focused`), app.rs (`click`, `wheel`).

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use qcalc_core::app::matrix_grid;
use qcalc_ffi::{MatrixView, TypstPaths};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
};

use crate::app::App;
use crate::engine::{EngineThread, Lane, done};
use crate::input::Field;
use crate::keys::Mods;
use crate::matrix_text::{self, Grid, Step};
use crate::model::{BarModel, Outcome};
use crate::render::{Canvas, Rect, caret_x, measure, size, with_alpha};
use crate::typst_row::{Picture, Typeset};

/// The card: 12 in from the sides and below, 10 inside (`.sys`'s look)
const MARGIN: f32 = 12.0;
const PAD: f32 = 10.0;
const CELL_W: f32 = 64.0;
const CELL_H: f32 = 34.0;
const CELL_GAP: f32 = 4.0;
/// room for the drawn brackets either side of the cells
const BRACKET: f32 = 10.0;
const SIZER_H: f32 = 18.0;
const SIZER_GAP: f32 = 8.0;
const LABEL_W: f32 = 66.0;
const BUTTON: f32 = 18.0;
/// `show steps · N`: 12 pt muted under the preview
const CHIP_H: f32 = 21.0;
const CHIP_OPEN_H: f32 = 17.0;
const CHIP_W: f32 = 150.0;
const OP_W: f32 = 168.0;
const STEP_GAP: f32 = 6.0;
const STEP_MAX_H: f32 = 72.0;
const TEXT_ROW_H: f32 = 22.0;
const STEPS_MOST: f32 = 320.0;

// ---- state ----

/// The open grid: the cells as fields (so editing, undo and word moves are the line's own) and which has the keys.
pub struct GridView {
    grid: Grid,
    cells: Vec<Vec<Field>>,
    /// the cell with the keys; None while the line has them
    focus: Option<(usize, usize)>,
}

impl GridView {
    fn new(grid: Grid) -> GridView {
        let cells = grid
            .cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(|t| {
                        let mut f = Field::default();
                        f.reset(t);
                        f
                    })
                    .collect()
            })
            .collect();
        GridView {
            grid,
            cells,
            focus: Some((0, 0)),
        }
    }

    fn field(&mut self, (r, c): (usize, usize)) -> Option<&mut Field> {
        self.cells.get_mut(r)?.get_mut(c)
    }
}

/// One step as the card shows it: the operation, and the matrix after it (typeset once Typst has it).
#[derive(Clone)]
struct StepRow {
    op: String,
    display: String,
    /// a closing remark (`det = …`): words only
    remark: bool,
    paths: Option<Typeset>,
}

/// The bar's matrices (`BarModel::matrix`).
#[derive(Default)]
pub struct MatrixState {
    pub grid: Option<GridView>,
    /// `↵ opens a 3×3 grid`, while the line asks for one and it isn't open
    hint: Option<String>,
    pub steps_shown: bool,
    steps_asked: Option<Vec<String>>,
    rows: Vec<StepRow>,
    latest: Arc<AtomicU64>,
    scroll: f32,
}

impl std::fmt::Debug for MatrixState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MatrixState")
            .field("grid", &self.grid.as_ref().map(|g| &g.grid))
            .field("steps_shown", &self.steps_shown)
            .finish_non_exhaustive()
    }
}

thread_local! {
    /// the steps' typeset matrices, one bitmap each
    static PICTURES: RefCell<Vec<Option<Picture>>> = const { RefCell::new(Vec::new()) };
}

/// The hidden trim (typst_row.rs): the steps' bitmaps go.
pub fn trim() {
    PICTURES.with(|p| p.borrow_mut().clear());
}

fn view(m: &BarModel) -> Option<&MatrixView> {
    let a = m.live.as_ref().filter(|_| m.live_for == m.field.text())?;
    a.matrix.as_ref().filter(|v| !v.message)
}

// ---- the slot ----

/// What the slot says while the line asks for a grid that isn't open.
pub fn slot_hint(m: &BarModel) -> Option<String> {
    m.matrix.hint.clone()
}

/// A matrix answer's text in the slot (`[1 −1; 1 √2]`, `λ² − 5λ`), for its display or exact form; None otherwise.
pub fn pretty(m: &BarModel, s: &str) -> Option<String> {
    let a = m.live.as_ref()?;
    view(m)?;
    let exact = crate::answer::exact(m);
    (s == a.display || exact == Some(s)).then(|| matrix_text::pretty(&crate::answer::radicals(s)))
}

/// `d` (the approximate half of a matrix answer's dual) without the label the exact half already shows.
pub fn without_dual_label(m: &BarModel, exact: Option<&str>, d: String) -> String {
    let (Some(a), Some(e)) = (m.live.as_ref(), exact) else {
        return d;
    };
    if view(m).is_none() {
        return d;
    }
    let label = matrix_text::dual_label(e, &a.display);
    match d.strip_prefix(label.as_str()) {
        Some(rest) if !label.is_empty() => rest.to_string(),
        _ => d,
    }
}

// ---- the grid ----

/// From `evaluate_live`: the grid stays open only while the line is the one it wrote; a line that asks for a grid
/// shows the hint and asks the engine for nothing (true: the live answer is blank).
pub fn follow(m: &mut BarModel, engine: &EngineThread) -> bool {
    let typed = m.field.text().to_string();
    // a new line starts with the steps shut
    if typed.is_empty() {
        m.matrix.steps_shown = false;
    }
    if m.matrix
        .grid
        .as_ref()
        .is_some_and(|g| g.grid.written != typed)
    {
        m.matrix.grid = None;
        m.touch();
    }
    let hint = if m.matrix.grid.is_none() {
        matrix_grid::grid_hint(&typed)
    } else {
        None
    };
    if hint != m.matrix.hint {
        m.matrix.hint = hint;
        m.touch();
    }
    if m.matrix.hint.is_none() {
        return false;
    }
    engine.cancel(Lane::Live);
    m.chained = false;
    m.live = None;
    m.live_for = typed;
    m.touch();
    true
}

/// A cell (not the line) has the keys: the line's caret hides (window.rs).
pub fn grid_focused(m: &BarModel) -> bool {
    m.matrix.grid.as_ref().is_some_and(|g| g.focus.is_some())
}

fn open(m: &mut BarModel, engine: &EngineThread, spot: matrix_grid::GridSpot) {
    let grid = Grid::new(spot, m.field.text());
    m.matrix.grid = Some(GridView::new(grid));
    m.matrix.steps_shown = false;
    m.matrix.hint = None;
    m.selected = None;
    m.tape_open = false;
    write(m, engine);
}

fn close(m: &mut BarModel) {
    if m.matrix.grid.take().is_some() {
        m.touch();
    }
}

/// Ctrl+G: the matrix under the caret in the grid (else the last in the line, else a new 2×2 at the caret).
fn toggle_grid(m: &mut BarModel, engine: &EngineThread) {
    if m.matrix.grid.is_some() {
        close(m);
        return;
    }
    let caret = m.field.to_utf16(m.field.caret()) as usize;
    let spot = matrix_text::spot_for(m.field.text(), caret);
    open(m, engine, spot);
}

/// The cells into the line in the grid's place, the caret after the matrix; the answer follows.
fn write(m: &mut BarModel, engine: &EngineThread) {
    let line = m.field.text().to_string();
    let Some(g) = &mut m.matrix.grid else { return };
    g.grid.cells = g
        .cells
        .iter()
        .map(|row| row.iter().map(|f| f.text().to_string()).collect())
        .collect();
    let Some((out, caret)) = g.grid.write(&line) else {
        close(m);
        return;
    };
    m.field.set_text_at(&out, caret);
    m.field.break_run();
    // the grid wrote what the line now says, so `follow` keeps it
    if let Some(g) = &mut m.matrix.grid {
        g.grid.written = m.field.text().to_string();
    }
    m.text_changed(engine);
    m.touch();
}

fn focus(m: &mut BarModel, at: Option<(usize, usize)>) {
    if let Some(g) = &mut m.matrix.grid {
        g.focus = at;
        if let Some(f) = at.and_then(|at| g.field(at)) {
            f.end(false);
        }
        m.touch();
    }
}

/// A row or column more (or fewer).
fn resize(m: &mut BarModel, engine: &EngineThread, dr: isize, dc: isize) {
    let Some(g) = &mut m.matrix.grid else { return };
    g.grid.cells = g
        .cells
        .iter()
        .map(|row| row.iter().map(|f| f.text().to_string()).collect())
        .collect();
    if !g.grid.resize(dr, dc) {
        return;
    }
    let keep = g.focus.map(|f| g.grid.clamp(f));
    let mut fresh = GridView::new(g.grid.clone());
    fresh.focus = keep;
    *g = fresh;
    write(m, engine);
}

// ---- the steps ----

/// Ctrl+R or the chip: the row operations behind the answer, or put them away.
fn toggle_steps(m: &mut BarModel, engine: &EngineThread) {
    if view(m).is_none_or(|v| v.steps.is_empty()) {
        m.matrix.steps_shown = false;
        return;
    }
    m.matrix.steps_shown = !m.matrix.steps_shown;
    m.matrix.scroll = 0.0;
    refresh_steps(m, engine);
    m.touch();
}

/// The card's rows for the answer shown (answer.rs, when an answer lands): the operations at once, their
/// matrices typeset after on the Typst thread.
pub fn refresh_steps(m: &mut BarModel, engine: &EngineThread) {
    if m.field.text().is_empty() {
        m.matrix.steps_shown = false;
    }
    let steps = view(m)
        .filter(|_| m.matrix.steps_shown)
        .map(|v| v.steps.clone())
        .filter(|s| !s.is_empty());
    let Some(steps) = steps else {
        if m.matrix.steps_asked.take().is_some() || !m.matrix.rows.is_empty() {
            m.matrix.rows.clear();
            m.touch();
        }
        return;
    };
    let preview = m.settings.typst_preview;
    let docs: Vec<String> = steps
        .iter()
        .map(|s| {
            if preview {
                s.matrix_document.clone()
            } else {
                String::new()
            }
        })
        .collect();
    if m.matrix.steps_asked.as_ref() == Some(&docs) {
        return;
    }
    m.matrix.steps_asked = Some(docs.clone());
    let rows: Vec<StepRow> = steps
        .iter()
        .map(|s| StepRow {
            op: s.op.clone(),
            display: s.display.clone(),
            remark: s.matrix_document.is_empty(),
            paths: None,
        })
        .collect();
    // text first only when there's nothing up yet, so a card being edited doesn't jump
    if m.matrix.rows.is_empty() || !preview {
        m.matrix.rows = rows.clone();
        m.touch();
    }
    if !preview {
        return;
    }
    let latest = m.matrix.latest.clone();
    let generation = latest.fetch_add(1, Ordering::Relaxed) + 1;
    engine.typst(move |r| {
        if latest.load(Ordering::Relaxed) != generation {
            return None;
        }
        let paths: Vec<Option<TypstPaths>> = docs
            .iter()
            .map(|d| {
                if d.is_empty() {
                    None
                } else {
                    r.paths(d.clone()).ok()
                }
            })
            .collect();
        done(move |app| {
            let st = &mut app.model.matrix;
            if st.steps_asked.as_ref() != Some(&docs) {
                return;
            }
            st.rows = rows
                .into_iter()
                .zip(paths)
                .zip(docs)
                .map(|((mut row, p), doc)| {
                    row.paths = p.map(|p| Typeset {
                        paths: Arc::new(p),
                        doc,
                        stale: false,
                    });
                    row
                })
                .collect();
            app.model.touch();
        })
    });
}

fn chip_shown(m: &BarModel) -> Option<usize> {
    if m.matrix.grid.is_some() {
        return None;
    }
    view(m).map(|v| v.steps.len()).filter(|&n| n > 0)
}

// ---- keys ----

const TAB: u32 = 0x09;
const ENTER: u32 = 0x0D;
const ESC: u32 = 0x1B;
const END: u32 = 0x23;
const HOME: u32 = 0x24;
const LEFT: u32 = 0x25;
const UP: u32 = 0x26;
const RIGHT: u32 = 0x27;
const DOWN: u32 = 0x28;
const BACK: u32 = 0x08;
const DELETE: u32 = 0x2E;
const KEY_G: u32 = 0x47;
const KEY_R: u32 = 0x52;

/// A key-down, before the line's own keys (model.rs). Ok when the grid or steps took it; Err hands the clipboard
/// reader back.
pub fn key_down<F: FnOnce() -> Option<String>>(
    m: &mut BarModel,
    vk: u32,
    mods: Mods,
    engine: &EngineThread,
    clipboard: F,
) -> Result<Outcome, F> {
    let plain = !mods.ctrl && !mods.alt && !mods.shift && !mods.win;
    let ctrl_only = mods.ctrl && !mods.alt && !mods.shift && !mods.win;
    // Ctrl+G the grid, Ctrl+R the steps (the Mac's ⌃G ⌃R), unless the user bound the chord to something else
    if ctrl_only && (vk == KEY_G || vk == KEY_R) && m.keybinds.action(mods, vk).is_none() {
        if vk == KEY_G {
            toggle_grid(m, engine);
        } else {
            toggle_steps(m, engine);
        }
        m.touch();
        return Ok(Outcome::Handled);
    }
    let Some(at) = m.matrix.grid.as_ref().and_then(|g| g.focus) else {
        // the line has the keys
        return match vk {
            ENTER if plain && m.selected.is_none() && m.matrix.grid.is_none() => {
                match matrix_grid::grid_command(m.field.text()) {
                    Some(spot) => {
                        open(m, engine, spot);
                        Ok(Outcome::Handled)
                    }
                    None => Err(clipboard),
                }
            }
            ESC if m.matrix.grid.is_some() => {
                close(m);
                Ok(Outcome::Handled)
            }
            ESC if m.matrix.steps_shown => {
                m.matrix.steps_shown = false;
                refresh_steps(m, engine);
                m.touch();
                Ok(Outcome::Handled)
            }
            _ => Err(clipboard),
        };
    };
    let ctrl = mods.ctrl && !mods.alt;
    let Some(g) = &m.matrix.grid else {
        return Err(clipboard);
    };
    let rows = g.grid.rows();
    let arrow = |m: &mut BarModel, dr: isize, dc: isize| -> bool {
        let Some(g) = &m.matrix.grid else {
            return false;
        };
        let f = &g.cells[at.0][at.1];
        let (a, b) = f.selection();
        let at_start = a == b && a == 0;
        let at_end = a == b && b == f.text().len();
        match g.grid.step(at, dr, dc, at_start, at_end) {
            Step::Cell(r, c) => {
                focus(m, Some((r, c)));
                true
            }
            Step::Line => {
                focus(m, None);
                true
            }
            Step::Stay => dc == 0,
        }
    };
    match vk {
        ESC => {
            close(m);
            return Ok(Outcome::Handled);
        }
        ENTER => {
            if at.0 + 1 < rows {
                focus(m, Some((at.0 + 1, at.1)));
            } else {
                close(m);
            }
            return Ok(Outcome::Handled);
        }
        TAB if !ctrl => {
            let next = g.grid.tab(at, mods.shift);
            focus(m, Some(next));
            return Ok(Outcome::Handled);
        }
        UP if !ctrl => {
            arrow(m, -1, 0);
            return Ok(Outcome::Handled);
        }
        DOWN if !ctrl => {
            arrow(m, 1, 0);
            return Ok(Outcome::Handled);
        }
        LEFT if !ctrl && !mods.shift && arrow(m, 0, -1) => return Ok(Outcome::Handled),
        RIGHT if !ctrl && !mods.shift && arrow(m, 0, 1) => return Ok(Outcome::Handled),
        _ => {}
    }
    // copying a cell's selection beats the copy-answer shortcut on the same chord
    let selected = m
        .matrix
        .grid
        .as_mut()
        .and_then(|g| g.field(at))
        .filter(|f| f.has_selection())
        .map(|f| f.selected_text().to_string());
    if ctrl && !mods.shift && vk == 0x43 && selected.is_some() {
        m.to_clipboard = selected;
        return Ok(Outcome::Handled);
    }
    if let Some(action) = m.keybinds.action(mods, vk) {
        return Ok(m.perform(action, engine));
    }
    let Some(f) = m.matrix.grid.as_mut().and_then(|g| g.field(at)) else {
        return Err(clipboard);
    };
    let before = f.text().to_string();
    let shift = mods.shift;
    let mut cut = None;
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
        0x58 if ctrl => cut = f.cut(),
        0x56 if ctrl => {
            if let Some(text) = clipboard() {
                // a cell is one entry: a pasted line keeps its first line
                f.paste(text.lines().next().unwrap_or(""));
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
    if cut.is_some() {
        m.to_clipboard = cut;
    }
    edited(m, engine, at, &before);
    Ok(Outcome::Handled)
}

/// After a key in a cell: repaint, and write the line again if its text changed.
fn edited(m: &mut BarModel, engine: &EngineThread, at: (usize, usize), before: &str) {
    m.touch();
    let changed = m
        .matrix
        .grid
        .as_mut()
        .and_then(|g| g.field(at))
        .is_some_and(|f| f.text() != before);
    if changed {
        write(m, engine);
    }
}

/// WM_CHAR while a cell has the keys: true when it took it.
pub fn char16(m: &mut BarModel, unit: u16, engine: &EngineThread) -> bool {
    let Some(at) = m.matrix.grid.as_ref().and_then(|g| g.focus) else {
        return false;
    };
    let Some(f) = m.matrix.grid.as_mut().and_then(|g| g.field(at)) else {
        return false;
    };
    let before = f.text().to_string();
    f.char16(unit);
    edited(m, engine, at, &before);
    true
}

/// The IME's text while a cell has the keys.
pub fn insert_text(m: &mut BarModel, text: &str, engine: &EngineThread) -> bool {
    let Some(at) = m.matrix.grid.as_ref().and_then(|g| g.focus) else {
        return false;
    };
    let Some(f) = m.matrix.grid.as_mut().and_then(|g| g.field(at)) else {
        return false;
    };
    let before = f.text().to_string();
    f.insert(text);
    f.break_run();
    edited(m, engine, at, &before);
    true
}

// ---- layout ----

fn grid_height(g: &GridView) -> f32 {
    let n = g.grid.rows() as f32;
    2.0 * PAD + n * CELL_H + (n - 1.0).max(0.0) * CELL_GAP + SIZER_GAP + SIZER_H + MARGIN
}

/// A step's height in a card `inner` wide, and the scale its matrix is drawn at.
fn step_size(row: &StepRow, inner: f32) -> (f32, f32) {
    let Some(t) = &row.paths else {
        return (TEXT_ROW_H, 1.0);
    };
    let room = (inner - OP_W - 10.0).max(40.0);
    let s = 1f32
        .min(STEP_MAX_H / t.paths.height_pt.max(1.0))
        .min(room / t.paths.width_pt.max(1.0))
        .max(0.01);
    ((t.paths.height_pt * s).max(TEXT_ROW_H), s)
}

fn steps_total(m: &BarModel, width: f32) -> f32 {
    let inner = width - 2.0 * (MARGIN + PAD);
    let rows = &m.matrix.rows;
    rows.iter().map(|r| step_size(r, inner).0).sum::<f32>()
        + (rows.len().saturating_sub(1)) as f32 * STEP_GAP
        + 4.0
}

/// The area's height (window.rs `Bar::layout`): the grid, or the chip and its card; 0 for neither.
pub fn height(m: &BarModel, width: f32) -> f32 {
    if let Some(g) = &m.matrix.grid {
        return grid_height(g);
    }
    if chip_shown(m).is_none() {
        return 0.0;
    }
    if !m.matrix.steps_shown || m.matrix.rows.is_empty() {
        return CHIP_H;
    }
    CHIP_OPEN_H + steps_total(m, width).min(STEPS_MOST) + 2.0 * PAD + MARGIN
}

fn card_of(r: Rect, top: f32) -> Rect {
    Rect::new(
        r.x + MARGIN,
        top,
        r.w - 2.0 * MARGIN,
        r.bottom() - MARGIN - top,
    )
}

fn cell_rect(card: Rect, r: usize, c: usize) -> Rect {
    Rect::new(
        card.x + PAD + BRACKET + c as f32 * (CELL_W + CELL_GAP),
        card.y + PAD + r as f32 * (CELL_H + CELL_GAP),
        CELL_W,
        CELL_H,
    )
}

/// The size buttons: rows −, rows +, columns −, columns +.
fn buttons(card: Rect, g: &GridView) -> [Rect; 4] {
    let n = g.grid.rows() as f32;
    let y = card.y + PAD + n * CELL_H + (n - 1.0).max(0.0) * CELL_GAP + SIZER_GAP;
    let x0 = card.x + PAD + LABEL_W;
    let x1 = x0 + 2.0 * BUTTON + 4.0 + 14.0 + LABEL_W;
    [
        Rect::new(x0, y, BUTTON, BUTTON),
        Rect::new(x0 + BUTTON + 4.0, y, BUTTON, BUTTON),
        Rect::new(x1, y, BUTTON, BUTTON),
        Rect::new(x1 + BUTTON + 4.0, y, BUTTON, BUTTON),
    ]
}

// ---- drawing ----

fn card(c: &Canvas, r: Rect) {
    let t = c.theme;
    c.fill_round(r, size::RADIUS_CARD, t.bg_tape);
    let edge = crate::contrast::or(
        if t.dark {
            crate::render::rgb(0xffffff, 0.08)
        } else {
            crate::render::rgb(0, 0.1)
        },
        t.line,
    );
    stroke_round(c, r.inset(0.25, 0.25), size::RADIUS_CARD, edge, 0.5);
}

fn stroke_round(c: &Canvas, r: Rect, radius: f32, color: crate::render::Color, width: f32) {
    use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
    use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
    let rr = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: r.x,
            top: r.y,
            right: r.right(),
            bottom: r.bottom(),
        },
        radiusX: radius,
        radiusY: radius,
    };
    // SAFETY: inside the frame; the rect is a local
    unsafe {
        c.target()
            .DrawRoundedRectangle(&rr, c.brush(color), width, None)
    };
}

/// The area in `r` (window.rs): the grid's card, or the chip and the steps' card.
pub fn paint(c: &Canvas, m: &BarModel, r: Rect, window_focused: bool) {
    if r.h <= 0.0 {
        return;
    }
    if let Some(g) = &m.matrix.grid {
        paint_grid(c, g, card_of(r, r.y), window_focused);
        return;
    }
    let Some(n) = chip_shown(m) else { return };
    let t = c.theme;
    let open = m.matrix.steps_shown && !m.matrix.rows.is_empty();
    let chip = Rect::new(
        r.x + size::GUTTER,
        r.y - 4.0,
        r.w - 2.0 * size::GUTTER,
        18.0,
    );
    let label = if m.matrix.steps_shown {
        "▾ hide steps".to_string()
    } else {
        format!("▸ show steps · {n}")
    };
    if let Some(l) = c.gfx.layout(&label, &c.gfx.fonts.small, 0.0, chip.h) {
        c.text_plain(&l, chip.x, chip.y, t.muted);
    }
    if !open {
        return;
    }
    let card_r = card_of(r, r.y + CHIP_OPEN_H);
    card(c, card_r);
    let inner = card_r.inset(PAD, PAD);
    c.push_clip(inner);
    let mut y = inner.y + 2.0 - m.matrix.scroll;
    PICTURES.with(|p| {
        let mut pics = p.borrow_mut();
        pics.resize_with(m.matrix.rows.len(), || None);
        for (row, pic) in m.matrix.rows.iter().zip(pics.iter_mut()) {
            let (h, s) = step_size(row, inner.w);
            if y + h >= inner.y && y <= inner.bottom() {
                let op = matrix_text::pretty(&crate::answer::radicals(&row.op));
                if row.remark {
                    if let Some(l) = c.gfx.layout_in(
                        &op,
                        &c.gfx.fonts.body,
                        inner.w,
                        h,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                    ) {
                        c.text_plain(&l, inner.x, y, t.ink);
                    }
                } else {
                    if let Some(l) = c.gfx.layout_in(
                        &op,
                        &c.gfx.fonts.body,
                        OP_W,
                        h,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                    ) {
                        c.text_plain(&l, inner.x, y, t.muted);
                    }
                    let x = inner.x + OP_W + 10.0;
                    match &row.paths {
                        Some(ts) => {
                            let top = y + (h - ts.paths.height_pt * s) / 2.0;
                            crate::typst_row::draw(c, pic, ts, s, |_| x.round(), top.round());
                        }
                        None => {
                            if let Some(l) = c.gfx.layout_in(
                                &row.display,
                                &c.gfx.fonts.body,
                                inner.right() - x,
                                h,
                                DWRITE_TEXT_ALIGNMENT_LEADING,
                            ) {
                                c.text_plain(&l, x, y, t.ink);
                            }
                        }
                    }
                }
            }
            y += h + STEP_GAP;
        }
    });
    c.pop_clip();
}

fn paint_grid(c: &Canvas, g: &GridView, card_r: Rect, window_focused: bool) {
    let t = c.theme;
    card(c, card_r);
    let (rows, cols) = (g.grid.rows(), g.grid.cols());
    c.push_clip(card_r.inset(1.0, 1.0));
    // the matrix's square brackets either side of the cells
    let top = card_r.y + PAD;
    let bottom = top + rows as f32 * CELL_H + (rows.saturating_sub(1)) as f32 * CELL_GAP;
    let left = card_r.x + PAD + 2.0;
    let right = cell_rect(card_r, 0, cols.saturating_sub(1)).right() + BRACKET - 2.0;
    let ink = with_alpha(t.muted, 0.7);
    for (x, dx) in [(left, 5.0), (right, -5.0)] {
        c.line(x + dx, top + 0.6, x, top + 0.6, 1.2, ink);
        c.line(x, top + 0.6, x, bottom - 0.6, 1.2, ink);
        c.line(x, bottom - 0.6, x + dx, bottom - 0.6, 1.2, ink);
    }
    for (r, row) in g.cells.iter().enumerate() {
        for (k, f) in row.iter().enumerate() {
            let cr = cell_rect(card_r, r, k);
            c.fill_round(cr, size::RADIUS_S, t.bg);
            let has = g.focus == Some((r, k));
            if has {
                stroke_round(
                    c,
                    cr.inset(0.5, 0.5),
                    size::RADIUS_S,
                    with_alpha(t.accent, 0.55),
                    1.0,
                );
            }
            paint_cell(c, f, cr.inset(6.0, 0.0), has && window_focused);
        }
    }
    // the sizes and their buttons, and the keys
    let b = buttons(card_r, g);
    let font = &c.gfx.fonts.small;
    let labels = [
        (format!("{rows} rows"), b[0].x - LABEL_W),
        (format!("{cols} columns"), b[2].x - LABEL_W),
    ];
    for (text, x) in labels {
        if let Some(l) = c.gfx.layout(&text, font, 0.0, BUTTON) {
            c.text_plain(&l, x, b[0].y, t.muted);
        }
    }
    for (i, br) in b.iter().enumerate() {
        c.fill_round(*br, BUTTON / 2.0, with_alpha(t.ink, 0.06));
        let (cx, cy) = (br.x + BUTTON / 2.0, br.y + BUTTON / 2.0);
        c.line(cx - 4.0, cy, cx + 4.0, cy, 1.4, t.muted);
        if i % 2 == 1 {
            c.line(cx, cy - 4.0, cx, cy + 4.0, 1.4, t.muted);
        }
    }
    let keys_x = b[3].right() + 14.0;
    let keys_w = card_r.right() - PAD - keys_x;
    if keys_w > 40.0
        && let Some(l) = c.gfx.layout_in(
            "tab next · ↵ down · esc done",
            font,
            keys_w,
            BUTTON,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        )
    {
        c.text_plain(&l, keys_x, b[0].y, t.muted);
    }
    c.pop_clip();
}

/// One cell: its text centred (scrolled to the caret when it's longer), `0` muted when empty.
fn paint_cell(c: &Canvas, f: &Field, r: Rect, caret_on: bool) {
    let t = c.theme;
    let font = &c.gfx.fonts.row;
    c.push_clip(r);
    let text = if f.text().is_empty() { "0" } else { f.text() };
    if let Some(l) = c.gfx.layout(text, font, 0.0, r.h) {
        let (w, _) = measure(&l);
        let caret = caret_x(&l, f.to_utf16(f.caret()));
        let x0 = if w <= r.w {
            r.x + (r.w - w) / 2.0
        } else {
            r.x - (caret - r.w + 4.0).clamp(0.0, w - r.w + 4.0)
        };
        if f.text().is_empty() {
            c.text_plain(&l, x0, r.y, with_alpha(t.muted, 0.5));
        } else {
            if f.has_selection() {
                let (a, b) = f.selection();
                let (xa, xb) = (caret_x(&l, f.to_utf16(a)), caret_x(&l, f.to_utf16(b)));
                c.fill_round(
                    Rect::new(x0 + xa, r.y + r.h * 0.5 - 11.0, xb - xa, 22.0),
                    3.0,
                    t.selection,
                );
            }
            c.text(&l, x0, r.y, t.ink);
        }
        if caret_on {
            let x = if f.text().is_empty() {
                r.x + r.w / 2.0
            } else {
                x0 + caret
            };
            let (mid, half) = (r.y + r.h * 0.5, size::ROW * 0.62);
            c.line(x + 0.5, mid - half, x + 0.5, mid + half, 1.5, t.ink);
        }
    }
    c.pop_clip();
}

// ---- the pointer ----

/// A press in the area: a cell takes the keys (double-click: its text selected), − + resize, the chip opens or
/// shuts the steps. A press anywhere else gives the keys back to the line. True when it was taken.
pub fn click(app: &mut App, x: f32, y: f32, double: bool) -> bool {
    let Some(r) = app.bar.layout.matrix else {
        if let Some(g) = &mut app.model.matrix.grid
            && g.focus.take().is_some()
        {
            app.model.touch();
        }
        return false;
    };
    let engine = &app.engine;
    let m = &mut app.model;
    if let Some(g) = &mut m.matrix.grid {
        let card_r = card_of(r, r.y);
        let (rows, cols) = (g.grid.rows(), g.grid.cols());
        for i in 0..rows {
            for k in 0..cols {
                if cell_rect(card_r, i, k).contains(x, y) {
                    g.focus = Some((i, k));
                    if let Some(f) = g.field((i, k)) {
                        if double {
                            f.select_all();
                        } else {
                            f.end(false);
                        }
                    }
                    m.touch();
                    return true;
                }
            }
        }
        let b = buttons(card_r, g);
        let sizes = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        if let Some(k) = b.iter().position(|br| br.inset(-3.0, -3.0).contains(x, y)) {
            let (dr, dc) = sizes[k];
            resize(m, engine, dr, dc);
            return true;
        }
        if r.contains(x, y) {
            return true;
        }
        if g.focus.take().is_some() {
            m.touch();
        }
        return false;
    }
    if !r.contains(x, y) {
        return false;
    }
    let chip = Rect::new(r.x + size::GUTTER, r.y - 4.0, CHIP_W, 20.0);
    if chip.contains(x, y) {
        toggle_steps(m, engine);
    }
    true
}

/// The wheel over the steps' card scrolls it.
pub fn wheel(app: &mut App, wp: WPARAM, lp: LPARAM) -> bool {
    let Some(r) = app.bar.layout.matrix else {
        return false;
    };
    if !app.model.matrix.steps_shown {
        return false;
    }
    let mut pt = windows::Win32::Foundation::POINT {
        x: i32::from((lp.0 & 0xffff) as u16 as i16),
        y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    };
    // SAFETY: our window, a live point
    let _ = unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(app.bar.hwnd, &mut pt) };
    let s = app.bar.scale();
    let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
    if !r.contains(x, y) {
        return false;
    }
    let delta = f32::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
    let most = (steps_total(&app.model, app.bar.layout.width) - STEPS_MOST).max(0.0);
    let st = &mut app.model.matrix;
    st.scroll = (st.scroll - delta / 120.0 * 3.0 * 17.5).clamp(0.0, most);
    app.model.touch();
    true
}

/// A one-line summary for the bench pipe's `state`.
pub fn debug(m: &BarModel) -> serde_json::Value {
    serde_json::json!({
        "grid": m.matrix.grid.as_ref().map(|g| serde_json::json!({
            "cells": g.cells.iter().map(|r| r.iter().map(|f| f.text().to_string()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "focus": g.focus,
        })),
        "hint": m.matrix.hint,
        "steps": m.matrix.steps_shown.then(|| m.matrix.rows.iter().map(|r| r.op.clone()).collect::<Vec<_>>()),
    })
}
