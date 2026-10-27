//! The tape (history) above the line: its view and its persistence.
//!
//! Persistence goes through the engine, off the UI thread (Lane::Misc): `load` at launch (`Engine::load_history`
//! reads `<config dir>\history.json`, migrating legacy rows) and `save` after every change
//! (`Engine::save_history` writes what `persistable_history` keeps). The model holds the rows as `engine::Rows`.
//!
//! The view (`HistoryTape` in BarView.swift): rows of 36 DIPs, at most 5 visible, the tape's own colour and a
//! hairline under it; each row the expression (13, muted) left and the answer (17) right in 46% of the width; the
//! highlighted row filled, its picked half in ink. Later work owned here: hover, scrolling with the wheel, the
//! delete / undo-delete of a row, the timer that lets recent rows expire while the bar is up.

use qcalc_ffi::HistoryRow;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
};

use crate::engine::{EngineThread, Lane, done};
use crate::model::BarModel;
use crate::qlog;
use crate::render::{Canvas, Rect, size};

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

/// The rows to draw (at most 5, the selected one in sight) of the visible range.
pub fn shown(model: &BarModel, visible: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let n = visible.len().min(size::TAPE_VISIBLE);
    if n == 0 {
        return visible.end..visible.end;
    }
    // the last n rows, unless the highlighted one is above them: then it is the top row
    let mut start = visible.end - n;
    if let Some(sel) = model.selected
        && sel < start
        && sel >= visible.start
    {
        start = sel;
    }
    start..start + n
}

/// The tape's height for `rows` shown rows (0 for none).
pub fn height(rows: usize) -> f32 {
    if rows == 0 {
        0.0
    } else {
        rows as f32 * size::TAPE_ROW + 2.0 * PAD_Y + 1.0
    }
}

/// Where row `i` of `shown` is.
pub fn row_rect(width: f32, shown: &std::ops::Range<usize>, i: usize) -> Rect {
    let k = (i - shown.start) as f32;
    Rect::new(
        PAD_X,
        PAD_Y + k * size::TAPE_ROW,
        width - 2.0 * PAD_X,
        size::TAPE_ROW,
    )
}

/// The row under a point, and whether the click is on its answer half.
pub fn hit(width: f32, shown: &std::ops::Range<usize>, x: f32, y: f32) -> Option<(usize, bool)> {
    shown.clone().find_map(|i| {
        let r = row_rect(width, shown, i);
        r.contains(x, y)
            .then(|| (i, x > r.right() - answer_w(r.w) - 6.0))
    })
}

fn answer_w(row_w: f32) -> f32 {
    (row_w * 0.46).max(120.0)
}

pub fn paint(c: &Canvas, model: &BarModel, shown: std::ops::Range<usize>) {
    if shown.is_empty() {
        return;
    }
    let t = c.theme;
    let h = height(shown.len());
    c.fill(Rect::new(0.0, 0.0, c.width, h), t.bg_tape);
    c.fill(Rect::new(0.0, h - 1.0, c.width, 1.0), t.line);
    for i in shown.clone() {
        let Some(row) = model.rows.rows.get(i) else {
            continue;
        };
        let r = row_rect(c.width, &shown, i);
        let selected = model.selected == Some(i);
        if selected {
            c.fill_round(r, size::RADIUS_S, t.fill_selected);
        }
        let inner = r.inset(8.0, 0.0);
        let aw = answer_w(inner.w);
        let ew = (inner.w - aw - 12.0).max(0.0);
        let expr = row.expr.replace("sqrt(", "√(");
        let expr_ink = if selected && !model.pick_answer {
            t.ink
        } else {
            t.muted
        };
        if let Some(l) = c.gfx.layout_in(
            &expr,
            &c.gfx.fonts.body,
            ew,
            r.h,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        ) {
            c.text(&l, inner.x, inner.y, expr_ink);
        }
        let answer = model.row_piece(row, true);
        let answer = if row.kind == Some(qcalc_ffi::HistoryKind::Definition) {
            String::new()
        } else {
            answer
        };
        let ans_ink = if selected && model.pick_answer {
            t.accent
        } else {
            t.answer
        };
        if let Some(l) = c.gfx.layout_in(
            &answer,
            &c.gfx.fonts.row,
            aw,
            r.h,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        ) {
            c.text(&l, inner.right() - aw, inner.y, ans_ink);
        }
    }
}
