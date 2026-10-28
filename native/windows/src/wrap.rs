//! Small text that may take a second line on a narrow bar: the hint under the line and the tutorial's step
//! (BarView.swift's `.lineLimit(2)` on both, native only; the page cuts them at one). One line is 18 DIPs high, as
//! before; a text that wraps takes `TWO_LINES`, cut with an ellipsis past the second. The line count is cached by
//! (text, width), so a keystroke's layout pass costs a lookup.

use std::cell::RefCell;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_WORD_WRAPPING_WRAP, IDWriteTextFormat, IDWriteTextLayout,
};

use crate::render::Gfx;

/// One line of 12 pt small text in the bar (window.rs's hint rect).
pub const ONE_LINE: f32 = 18.0;
/// Two lines of it.
pub const TWO_LINES: f32 = 34.0;

thread_local! {
    static LINES: RefCell<Vec<(String, u32, usize)>> = const { RefCell::new(Vec::new()) };
}

/// The text laid out to wrap in `w`, at most two lines, cut with an ellipsis past them, centred in `h`.
pub fn layout(
    gfx: &Gfx,
    text: &str,
    format: &IDWriteTextFormat,
    w: f32,
    h: f32,
) -> Option<IDWriteTextLayout> {
    let l = gfx.layout_in(text, format, w, h, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    // SAFETY: setters on a live layout
    unsafe {
        let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
        // two lines start at the top, so a third (clipped by the caller) can't push the first up
        let _ = l.SetParagraphAlignment(if h >= TWO_LINES {
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR
        } else {
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER
        });
    }
    Some(l)
}

/// How many lines `text` takes wrapped in `w` (1 or 2; more counts as 2).
pub fn lines(gfx: &Gfx, text: &str, format: &IDWriteTextFormat, w: f32) -> usize {
    let key = w.to_bits();
    if let Some(n) = LINES.with(|c| {
        c.borrow()
            .iter()
            .find(|(t, k, _)| *k == key && t == text)
            .map(|e| e.2)
    }) {
        return n;
    }
    let n = layout(gfx, text, format, w, 1000.0).map_or(1, |l| {
        let mut m = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
        // SAFETY: out pointer is a live local
        if unsafe { l.GetMetrics(&mut m) }.is_ok() {
            (m.lineCount as usize).clamp(1, 2)
        } else {
            1
        }
    });
    LINES.with(|c| {
        let mut c = c.borrow_mut();
        c.insert(0, (text.to_string(), key, n));
        c.truncate(8);
    });
    n
}

/// The rect height for `text` in `w`: one line or two.
pub fn height(gfx: &Gfx, text: &str, format: &IDWriteTextFormat, w: f32) -> f32 {
    if lines(gfx, text, format, w) > 1 {
        TWO_LINES
    } else {
        ONE_LINE
    }
}

/// Forgets the cached counts (a hide).
pub fn trim() {
    LINES.with(|c| c.borrow_mut().clear());
}
