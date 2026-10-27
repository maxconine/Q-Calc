//! The pieces every game panel is built from, as src/index.css draws them (the Mac's GameControls.swift):
//! `.pong-button`, `.pong-head`, `.pong-leave`, `.pong-line`, `.pong-note` (and `.pong-wait`'s pulse),
//! `.pong-link`, `.pong-code`, `.pong-score`, the toast, and the full-screen button and its glyph. Each draws at a
//! point and returns how much room it took.

use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::theme::{Rgba, m};

/// `.pong-wait`: opacity down to 0.45 and back every 1.6 s.
pub fn wait_alpha(ui: &mut Ui) -> f32 {
    ui.animate();
    let t = (ui.t % 1.6) / 1.6;
    let k = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * t).cos();
    1.0 - 0.55 * k as f32
}

/// A `.pong-button`'s size: 34 high and at least 112 wide; `choice` is the lobby's first pair (46, 148, 17 px).
pub fn button_size(ui: &mut Ui, title: &str, kbd: Option<&str>, choice: bool) -> (f32, f32) {
    let f = if choice {
        Font::new(m::FS_ROW)
    } else {
        Font::BODY
    };
    let (tw, _) = ui.measure(title, f);
    let kw = kbd.map_or(0.0, |k| ui.measure(k, Font::CAPTION).0 + m::SP2);
    let w = (tw + kw + 12.0).max(if choice { 148.0 } else { 112.0 });
    (w.ceil(), if choice { 46.0 } else { 34.0 })
}

/// A `.pong-button` in `r`; `on` rings it in accent.
pub fn button(
    ui: &mut Ui,
    r: Rect,
    title: &str,
    kbd: Option<&str>,
    on: bool,
    choice: bool,
    hit: Hit,
) {
    let c = ui.c;
    let hover = ui.hover(r);
    ui.control_shadow(r, m::RADIUS_S);
    ui.round(
        r,
        m::RADIUS_S,
        if hover {
            c.control.mix(0.94, c.ink)
        } else {
            c.control
        },
    );
    if on {
        ui.ring(r, m::RADIUS_S, 1.5, c.ring());
    }
    let f = if choice {
        Font::new(m::FS_ROW)
    } else {
        Font::BODY
    };
    let (tw, th) = ui.measure(title, f);
    let (kw, kh) = kbd.map_or((0.0, 0.0), |k| ui.measure(k, Font::CAPTION));
    let total = tw + if kbd.is_some() { m::SP2 + kw } else { 0.0 };
    let x = r.x + (r.w - total) / 2.0;
    ui.text(title, f, x, r.y + (r.h - th) / 2.0, c.ink);
    if let Some(k) = kbd {
        // baselines line up: the caption sits a little lower in its box
        ui.text(
            k,
            Font::CAPTION,
            x + tw + m::SP2,
            r.y + (r.h - kh) / 2.0 + 0.5,
            c.muted,
        );
    }
    ui.hit(r, hit);
}

/// A button row (`flex-wrap: wrap`, centred on cx, `gap` apart, wrapping at `max_w`); returns its height.
pub struct ButtonSpec<'a> {
    pub title: &'a str,
    pub kbd: Option<String>,
    pub on: bool,
    pub hit: Hit,
}

pub fn button_row(
    ui: &mut Ui,
    cx: f32,
    y: f32,
    max_w: f32,
    items: &[ButtonSpec],
    choice: bool,
) -> f32 {
    let gap = m::SP3;
    let sizes: Vec<(f32, f32)> = items
        .iter()
        .map(|b| button_size(ui, b.title, b.kbd.as_deref(), choice))
        .collect();
    // rows by FlowRow's rule
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut cur_w = 0.0;
    for (i, &(w, _)) in sizes.iter().enumerate() {
        let add = if cur.is_empty() { w } else { cur_w + gap + w };
        if !cur.is_empty() && add > max_w {
            rows.push(std::mem::take(&mut cur));
            cur_w = w;
        } else {
            cur_w = add;
        }
        cur.push(i);
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    let mut yy = y;
    for (ri, row) in rows.iter().enumerate() {
        let w: f32 = row.iter().map(|&i| sizes[i].0).sum::<f32>() + gap * (row.len() as f32 - 1.0);
        let h = row.iter().map(|&i| sizes[i].1).fold(0.0, f32::max);
        let mut x = cx - w / 2.0;
        for &i in row {
            let b = &items[i];
            button(
                ui,
                Rect::new(x, yy, sizes[i].0, sizes[i].1),
                b.title,
                b.kbd.as_deref(),
                b.on,
                choice,
                b.hit.clone(),
            );
            x += sizes[i].0 + gap;
        }
        yy += h + if ri + 1 < rows.len() { gap } else { 0.0 };
    }
    yy - y
}

/// `.pong-leave`: the small "esc" (or another word) whose right edge is at `right`; returns its width.
pub fn esc_button(ui: &mut Ui, right: f32, y: f32, title: &str, hit: Hit) -> f32 {
    let (tw, th) = ui.measure(title, Font::SMALL);
    let r = Rect::new(right - tw - 2.0 * m::SP2, y, tw + 2.0 * m::SP2, th + 6.0);
    let hover = ui.hover(r);
    let c = ui.c;
    if hover {
        ui.round(r, m::RADIUS_S, c.fill_hover());
    }
    ui.text(
        title,
        Font::SMALL,
        r.x + m::SP2,
        r.y + 3.0,
        if hover { c.ink } else { c.muted },
    );
    ui.hit(r, hit);
    r.w
}

/// The full-screen glyph: corners inward when on, outward when off, in a 16-unit box at (x, y) of `size`.
pub fn fullscreen_glyph(ui: &mut Ui, x: f32, y: f32, size: f32, on: bool, c: Rgba) {
    let k = size / 16.0;
    let p = |px: f32, py: f32| (x + px * k, y + py * k);
    let corners: [[(f32, f32); 3]; 4] = if on {
        [
            [(6., 2.), (6., 6.), (2., 6.)],
            [(10., 2.), (10., 6.), (14., 6.)],
            [(6., 14.), (6., 10.), (2., 10.)],
            [(10., 14.), (10., 10.), (14., 10.)],
        ]
    } else {
        [
            [(2., 6.), (2., 2.), (6., 2.)],
            [(14., 6.), (14., 2.), (10., 2.)],
            [(2., 10.), (2., 14.), (6., 14.)],
            [(14., 10.), (14., 14.), (10., 14.)],
        ]
    };
    for cn in corners {
        let poly = super::svg::Poly {
            parts: vec![vec![
                p(cn[0].0, cn[0].1),
                p(cn[1].0, cn[1].1),
                p(cn[2].0, cn[2].1),
            ]],
        };
        ui.poly(&poly, 1.5, c);
    }
}

/// The small full-screen button (24 × 22) at (x, y); `corner` is the one in the game's corner, half see-through.
pub fn fullscreen_button(ui: &mut Ui, x: f32, y: f32, on: bool, corner: bool) {
    let r = Rect::new(x, y, 24.0, 22.0);
    let hover = ui.hover(r);
    let c = ui.c;
    let was = ui.set_alpha(if corner && !hover { 0.5 } else { 1.0 });
    if hover {
        ui.round(r, m::RADIUS_S, c.fill_hover());
    }
    fullscreen_glyph(
        ui,
        x + 5.5,
        y + 4.5,
        13.0,
        on,
        if hover { c.ink } else { c.muted },
    );
    ui.set_alpha(was);
    ui.hit(r, Hit::Fullscreen);
}

/// `.pong-head`: the name, the line beside it, and at the right the full-screen button and esc. Returns its height.
pub fn head(ui: &mut Ui, x: f32, y: f32, w: f32, name: &str, sub: &str, fullscreen: bool) -> f32 {
    let c = ui.c;
    let (nw, nh) = ui.measure(name, Font::bold(m::FS_BODY));
    let (_, sh) = ui.measure(sub, Font::SMALL);
    let h = nh.max(22.0);
    ui.text(name, Font::bold(m::FS_BODY), x, y + (h - nh) / 2.0, c.ink);
    let right = x + w + m::SP2;
    let esc_w = esc_button(ui, right, y + (h - sh - 6.0) / 2.0, "esc", Hit::Esc);
    let fs_x = right - esc_w - 24.0;
    fullscreen_button(ui, fs_x, y + (h - 22.0) / 2.0, fullscreen, false);
    let sub_x = x + nw + m::SP3;
    ui.text_in(
        sub,
        Font::SMALL,
        sub_x,
        y + (h - sh) / 2.0 + 1.0,
        (fs_x - m::SP3 - sub_x).max(0.0),
        Align::Left,
        c.muted,
    );
    h
}

/// `.pong-line`: 13 px ink, centred, wrapping. Returns its height.
pub fn line(ui: &mut Ui, cx: f32, y: f32, w: f32, text: &str) -> f32 {
    let c = ui.c.ink;
    ui.text_wrap(text, Font::BODY, cx - w / 2.0, y, w, Align::Center, c)
}

/// `.pong-note`: 12 px muted, centred, wrapping; `wait` pulses it. Returns its height.
pub fn note(ui: &mut Ui, cx: f32, y: f32, w: f32, text: &str, wait: bool) -> f32 {
    let was = if wait {
        let a = wait_alpha(ui);
        ui.set_alpha(a)
    } else {
        ui.set_alpha(1.0)
    };
    let c = ui.c.muted;
    let h = ui.text_wrap(text, Font::SMALL, cx - w / 2.0, y, w, Align::Center, c);
    ui.set_alpha(was);
    h
}

/// A run of a note line: plain, a link (`.pong-link`: the note's colour, ink on hover, or always ink), or bold ink.
pub struct Run<'a> {
    pub text: &'a str,
    pub hit: Option<Hit>,
    pub ink: bool,
    pub bold: bool,
}

impl<'a> Run<'a> {
    pub fn plain(text: &'a str) -> Run<'a> {
        Run {
            text,
            hit: None,
            ink: false,
            bold: false,
        }
    }
    pub fn link(text: &'a str, hit: Hit) -> Run<'a> {
        Run {
            text,
            hit: Some(hit),
            ink: false,
            bold: false,
        }
    }
    pub fn ink_link(text: &'a str, hit: Hit) -> Run<'a> {
        Run {
            text,
            hit: Some(hit),
            ink: true,
            bold: false,
        }
    }
}

/// Runs on one line, centred on cx, in `f` (muted unless a run says ink); `base` colours the plain runs.
pub fn runs(ui: &mut Ui, cx: f32, y: f32, f: Font, base: Rgba, parts: &[Run]) -> f32 {
    let fonts: Vec<Font> = parts
        .iter()
        .map(|r| if r.bold { Font { bold: true, ..f } } else { f })
        .collect();
    let sizes: Vec<(f32, f32)> = parts
        .iter()
        .zip(&fonts)
        .map(|(r, f)| ui.measure(r.text, *f))
        .collect();
    let total: f32 = sizes.iter().map(|s| s.0).sum();
    let h = sizes.iter().map(|s| s.1).fold(0.0, f32::max);
    let mut x = cx - total / 2.0;
    let c = ui.c;
    for ((r, (w, _)), font) in parts.iter().zip(&sizes).zip(&fonts) {
        let rect = Rect::new(x, y, *w, h);
        let color = match &r.hit {
            Some(_) if r.ink || ui.hover(rect) => c.ink,
            Some(_) => base,
            None if r.bold || r.ink => c.ink,
            None => base,
        };
        ui.text(r.text, *font, x, y, color);
        if let Some(hit) = &r.hit {
            ui.hit(rect, hit.clone());
        }
        x += w;
    }
    h
}

/// `.pong-code`: digits in 44 × 54 tiles centred on cx; `entry` frames them for typing with the caret's tile
/// ringed, `busy` greys them while joining. Returns its height.
pub fn code(
    ui: &mut Ui,
    cx: f32,
    y: f32,
    digits: &[String],
    entry: bool,
    busy: bool,
    caret: Option<usize>,
) -> f32 {
    let c = ui.c;
    let n = digits.len() as f32;
    let w = n * 44.0 + (n - 1.0).max(0.0) * m::SP2;
    let mut x = cx - w / 2.0;
    let f = Font::digits(30.0, true);
    for (i, d) in digits.iter().enumerate() {
        let r = Rect::new(x, y, 44.0, 54.0);
        ui.round(r, m::RADIUS_S, c.bg_tape);
        if entry {
            ui.ring(
                r,
                m::RADIUS_S,
                1.0,
                if caret == Some(i) { c.ring() } else { c.line },
            );
        }
        let color = if busy {
            c.muted
        } else if entry {
            c.ink
        } else {
            c.accent
        };
        let (tw, th) = ui.measure(d, f);
        ui.text(d, f, x + (44.0 - tw) / 2.0, y + (54.0 - th) / 2.0, color);
        x += 44.0 + m::SP2;
    }
    54.0
}

/// `.pong-score`: the two names either side of the score. Returns its height.
#[allow(clippy::too_many_arguments)]
pub fn score(
    ui: &mut Ui,
    x: f32,
    y: f32,
    w: f32,
    left: &str,
    right: &str,
    s: (u32, u32),
    lc: Rgba,
    rc: Rgba,
) -> f32 {
    let c = ui.c;
    let f = Font::digits(m::FS_DISPLAY, true);
    let a = format!("{} ", s.0);
    let b = format!(" {}", s.1);
    let (aw, h) = ui.measure(&a, f);
    let (dw, _) = ui.measure("–", Font::digits(m::FS_DISPLAY, false));
    let (bw, _) = ui.measure(&b, f);
    let total = aw + dw + bw;
    let mut sx = x + (w - total) / 2.0;
    sx += ui.text(&a, f, sx, y, c.ink);
    sx += ui.text("–", Font::digits(m::FS_DISPLAY, false), sx, y, c.muted);
    ui.text(&b, f, sx, y, c.ink);
    let (_, sh) = ui.measure(left, Font::SMALL);
    // the names sit on the score's baseline, roughly
    let ny = y + h * 0.78 - sh * 0.78;
    let side = (w - total) / 2.0 - m::SP3;
    ui.text_in(left, Font::SMALL, x, ny, side, Align::Left, lc);
    ui.text_in(right, Font::SMALL, x + w - side, ny, side, Align::Right, rc);
    h
}

/// A pill over the game (`.game-toast`), centred on cx with its top at y.
pub fn toast(ui: &mut Ui, cx: f32, y: f32, text: &str) {
    let c = ui.c;
    let (tw, th) = ui.measure(text, Font::SMALL);
    let w = (tw + 2.0 * m::SP3).min(ui.w - 24.0);
    let r = Rect::new(cx - w / 2.0, y, w, th + 10.0);
    ui.card_shadow(r, r.h / 2.0);
    ui.round(r, r.h / 2.0, c.bg);
    ui.text_in(
        text,
        Font::SMALL,
        r.x + m::SP3,
        r.y + 5.0,
        w - 2.0 * m::SP3,
        Align::Center,
        c.ink,
    );
}

/// A banner pill in 13 px (pong's "first to 7"), centred in `area`.
pub fn banner(ui: &mut Ui, area: Rect, text: &str) {
    let c = ui.c;
    let (tw, th) = ui.measure(text, Font::BODY);
    let w = (tw + 2.0 * m::SP3).min(area.w - 2.0 * m::SP3);
    let r = Rect::new(
        area.x + (area.w - w) / 2.0,
        area.y + (area.h - th - 10.0) / 2.0,
        w,
        th + 10.0,
    );
    ui.card_shadow(r, r.h / 2.0);
    ui.round(r, r.h / 2.0, c.bg);
    ui.text_in(
        text,
        Font::BODY,
        r.x + m::SP3,
        r.y + 5.0,
        w - 2.0 * m::SP3,
        Align::Center,
        c.ink,
    );
}
