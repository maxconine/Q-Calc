//! A selected part of the line answered on its own in the answer slot (the page's `selDisplay`, QuickCalc.tsx;
//! the Mac's `inputSelected`, BarModel.swift): selecting `3*4` in `2 + 3*4` shows a muted `12` until the
//! selection goes. Evaluated alone with no history, on its own engine lane; not for the whole line, a graph's
//! line, or a part that reads the same as its value. A screen reader hears it (a11y.rs `announce`).
//!
//! Cost: `after` compares the selection with the last one seen (no allocation unless it changed), so a keystroke
//! without a selection pays a comparison.

use qcalc_ffi::{EvalContext, LiveRequest};

use crate::app::App;
use crate::engine::{Lane, done};
use crate::model::{BarModel, now_ms};
use crate::render::{Canvas, Gfx, Rect, measure};

/// The part to answer for `line` with the selection `(a, b)` (bytes), or None.
pub fn part_of(line: &str, a: usize, b: usize) -> Option<&str> {
    if b <= a || b > line.len() || !line.is_char_boundary(a) || !line.is_char_boundary(b) {
        return None;
    }
    let part = line[a..b].trim();
    let whole = line.trim();
    if part.is_empty() || part == whole || whole.to_lowercase().starts_with("graph") {
        return None;
    }
    Some(part)
}

/// What the slot shows for the part, or None when it says nothing new.
pub fn shown_for(part: &str, display: &str, improper: bool) -> Option<String> {
    (!display.is_empty() && display != part && !improper).then(|| display.to_string())
}

/// After each message (line.rs `after`): a new selection is sent to the engine; a gone one clears the slot.
pub fn after(app: &mut App) {
    let m = &mut app.model;
    let (a, b) = m.field.selection();
    let seen = &m.line.sel_seen;
    // the usual keystroke: no selection now or before
    if a == b && seen.1 == seen.2 {
        return;
    }
    if seen.1 == a && seen.2 == b && seen.0 == m.field.text() {
        return;
    }
    m.line.sel_seen = if a == b {
        (String::new(), 0, 0)
    } else {
        (m.field.text().to_string(), a, b)
    };
    let line = m.field.text().to_string();
    let Some(part) = part_of(&line, a, b).filter(|_| m.graph.is_none() && m.systems.is_none())
    else {
        app.engine.cancel(Lane::Selection);
        if m.line.selection_answer.take().is_some() {
            m.touch();
        }
        return;
    };
    let part = part.to_string();
    let options = m.options();
    app.engine.submit(Lane::Selection, move |cx| {
        let answer = cx.engine.live(LiveRequest {
            text: part.clone(),
            options,
            context: EvalContext::default(),
            chained: false,
            now_ms: now_ms(),
            phrases: false,
        });
        let shown = shown_for(&part, &answer.display, answer.improper);
        done(move |app| {
            let m = &mut app.model;
            if m.field.text() != line || m.line.sel_seen.0 != line {
                return;
            }
            if let Some(s) = &shown {
                crate::a11y::announce(&format!("selection: {s}"));
            }
            m.line.selection_answer = shown;
            m.touch();
        })
    });
}

/// The selection's answer while the line has text.
fn shown(m: &BarModel) -> Option<&str> {
    if m.field.text().is_empty() || !m.field.has_selection() {
        return None;
    }
    m.line.selection_answer.as_deref()
}

/// The slot's width for it (commands.rs `slot_width`), or None when there's no selection answer.
pub fn slot_width(gfx: &Gfx, m: &BarModel) -> Option<f32> {
    let s = shown(m)?;
    let l = gfx.layout(
        &crate::answer::pretty_text(s),
        &gfx.fonts.display,
        0.0,
        36.0,
    )?;
    Some(measure(&l).0 + 2.0 * crate::answer::PAD_X + 1.0)
}

/// Paints it muted in the slot (commands.rs `paint_slot`); false when there's none.
pub fn paint_slot(c: &Canvas, m: &BarModel, r: Rect) -> bool {
    let Some(s) = shown(m) else { return false };
    let inner = r.inset(crate::answer::PAD_X, 0.0);
    if let Some(l) = c.gfx.layout_in(
        &crate::answer::pretty_text(s),
        &c.gfx.fonts.display,
        inner.w,
        inner.h,
        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING,
    ) {
        c.text(&l, inner.x, inner.y, c.theme.muted);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_of_the_line_is_answered() {
        assert_eq!(part_of("2 + 3*4", 4, 7), Some("3*4"));
        assert_eq!(part_of("2 + 3*4", 0, 7), None);
        assert_eq!(part_of("2 + 3*4", 3, 3), None);
        assert_eq!(part_of("graph x^2", 6, 9), None);
        assert_eq!(part_of("2 + 3*4", 3, 4), None);
        assert_eq!(shown_for("3*4", "12", false).as_deref(), Some("12"));
        assert_eq!(shown_for("12", "12", false), None);
        assert_eq!(shown_for("3 m", "3 m", true), None);
    }
}
