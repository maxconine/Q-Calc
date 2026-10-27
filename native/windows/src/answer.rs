//! The live answer, right of the line (`LiveAnswerView` in BarView.swift): the engine's pretty answer at the
//! display size, right-aligned, cut with an ellipsis when it can't fit. A click on it copies it (app.rs).
//!
//! Later work owned here: the fit-font shrink (`useFitFont`), the dual label and ≈ for exact/approx pairs, solve
//! roots with `x =`, Tab's answer forms, the closed-form upgrade, the steady (dimmed) last answer while an edit is
//! half typed, the copied flash, time letters' chips.

use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING;

use crate::render::{Canvas, Gfx, Rect, measure};

/// --answer min width (minmax(120px, max-content)) and its padding (2 8)
pub const MIN_W: f32 = 120.0;
pub const PAD_X: f32 = 8.0;

/// How wide the answer wants to be (0 for none), before the field takes the rest.
pub fn wanted_width(gfx: &Gfx, text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let Some(layout) = gfx.layout(text, &gfx.fonts.display, 0.0, 36.0) else {
        return 0.0;
    };
    (measure(&layout).0 + 2.0 * PAD_X).max(MIN_W)
}

pub fn paint(c: &Canvas, text: &str, r: Rect, dim: bool) {
    if text.is_empty() || r.w <= 0.0 {
        return;
    }
    let inner = r.inset(PAD_X, 0.0);
    if let Some(layout) = c.gfx.layout_in(
        text,
        &c.gfx.fonts.display,
        inner.w,
        inner.h,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    ) {
        let color = if dim { c.theme.muted } else { c.theme.answer };
        c.text(&layout, inner.x, inner.y, color);
    }
}
