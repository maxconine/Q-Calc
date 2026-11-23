//! The web app's `--shadow-card` and `--control-shadow`, light and dark (src/index.css), for the bar's cards (the
//! `sys` panel, the matrix grid and steps cards) and the reply lines (a message card's, a thread's). Mac spec:
//! `mac/Sources/Bar/Surfaces.swift`. The cards drew a black hairline in dark too (no edge on a #252527 card) and
//! no drop; dark now takes the page's dark values.

use crate::render::{Canvas, Color, Rect, Theme, rgb};

/// `--shadow-card`'s 0.5 px ring: black 10% / white 10%
pub fn card_ring(t: &Theme) -> Color {
    crate::contrast::or(
        if t.dark {
            rgb(0xffffff, 0.1)
        } else {
            rgb(0, 0.1)
        },
        t.line,
    )
}

/// `--shadow-card`'s 1 px 3 px drop: black 6% / 30%
pub fn card_shadow(t: &Theme) -> f32 {
    if t.dark { 0.3 } else { 0.06 }
}

/// `--control-shadow`'s 0.5 px ring: black 12% / 30%
pub fn control_ring(t: &Theme) -> Color {
    crate::contrast::or(rgb(0, if t.dark { 0.3 } else { 0.12 }), t.line)
}

/// `--control-shadow`'s 1 px 2 px drop: black 10% / 25%
pub fn control_shadow(t: &Theme) -> f32 {
    if t.dark { 0.25 } else { 0.1 }
}

/// A soft black drop under `r` (moved down `dy`, spread `blur`), as a few widening rounded rects whose alphas add
/// up to `alpha` where they all overlap: the shape under the card is covered, only its blurred edge shows. None
/// in a contrast theme.
pub fn drop(c: &Canvas, r: Rect, radius: f32, alpha: f32, blur: f32, dy: f32) {
    if crate::contrast::on() || alpha <= 0.0 {
        return;
    }
    const STEPS: usize = 3;
    // each layer's alpha so that STEPS of them composite to `alpha`
    let a = 1.0 - (1.0 - alpha).powf(1.0 / STEPS as f32);
    for i in 0..STEPS {
        let grow = blur * (i as f32 + 0.5) / STEPS as f32 - blur / 2.0;
        let s = Rect::new(
            r.x - grow,
            r.y + dy - grow,
            r.w + 2.0 * grow,
            r.h + 2.0 * grow,
        );
        c.fill_round(s, radius + grow.max(0.0), rgb(0, a));
    }
}

/// A card: the drop, the tape's colour, the 0.5 px ring.
pub fn card(c: &Canvas, r: Rect, radius: f32) {
    let t = c.theme;
    drop(c, r, radius, card_shadow(t), 3.0, 1.0);
    c.fill_round(r, radius, t.bg_tape);
    stroke(c, r.inset(0.25, 0.25), radius, card_ring(t), 0.5);
}

/// A reply line's frame: the drop, then the accent's 2 px ring with the keys or the control ring without, then
/// the control's fill.
pub fn control(c: &Canvas, f: Rect, radius: f32, focused: bool, fill: Color) {
    let t = c.theme;
    drop(c, f, radius, control_shadow(t), 2.0, 1.0);
    if focused {
        c.fill_round(f, radius, t.accent);
        c.fill_round(f.inset(2.0, 2.0), radius - 2.0, fill);
    } else {
        c.fill_round(f, radius, control_ring(t));
        c.fill_round(f.inset(0.5, 0.5), radius - 0.5, fill);
    }
}

fn stroke(c: &Canvas, r: Rect, radius: f32, color: Color, width: f32) {
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
            .DrawRoundedRectangle(&rr, c.brush(color), width, None);
    }
}
