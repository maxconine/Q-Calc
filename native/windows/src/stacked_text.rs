//! A history row's expression with its fractions, powers and roots drawn stacked, as the bar draws them while
//! "Stacked fractions and powers" is on (StackedText.swift): stacked.rs says what's stacked, and the sizes, gaps and
//! shifts are the bar's (TeX's, `slot_size` / `num_offset` / `den_offset` / `sup_shift` with the row's size as the
//! line's). Built once per row into a `Picture` (history.rs keeps it with the row's layouts) and drawn on the row's
//! baseline, which moves only as far as the row needs to show all of it (`4/(233*212^3)` is never cut off).

use crate::render::{Canvas, Color, Gfx, Rect, measure};
use crate::stacked::{self, Parsed, tex};
use crate::stacked_layout;
use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout};

/// One thing to draw, from the picture's left and baseline (y grows down).
#[derive(Clone)]
enum Item {
    /// a run of text: its layout and its top-left
    Text(IDWriteTextLayout, f32, f32),
    /// a fraction's bar, an empty slot's box (muted)
    Rule(Rect),
    Hole(Rect),
    /// a root's line (`root_points_between`) at a size, and a cube root's 3 at (left, baseline)
    Root([(f32, f32); 5], f32, Option<(f32, f32)>),
    /// a grown bracket: (left, width), (top, bottom), size, (square, closing)
    Bracket((f32, f32), (f32, f32), f32, (bool, bool)),
}

impl Item {
    fn moved(self, dx: f32, dy: f32) -> Item {
        match self {
            Item::Text(l, x, y) => Item::Text(l, x + dx, y + dy),
            Item::Rule(r) => Item::Rule(Rect::new(r.x + dx, r.y + dy, r.w, r.h)),
            Item::Hole(r) => Item::Hole(Rect::new(r.x + dx, r.y + dy, r.w, r.h)),
            Item::Root(p, s, cube) => Item::Root(
                p.map(|(x, y)| (x + dx, y + dy)),
                s,
                cube.map(|(x, y)| (x + dx, y + dy)),
            ),
            Item::Bracket((l, w), (t, b), s, k) => {
                Item::Bracket((l + dx, w), (t + dy, b + dy), s, k)
            }
        }
    }
}

/// A row's expression laid out stacked: its width, how far it reaches over its baseline and under it.
#[derive(Clone, Default)]
pub struct Picture {
    pub w: f32,
    pub h: f32,
    pub d: f32,
    items: Vec<Item>,
}

/// A laid-out piece: its items from its own left and baseline.
struct Piece {
    w: f32,
    h: f32,
    d: f32,
    items: Vec<Item>,
}

impl Piece {
    fn empty() -> Piece {
        Piece {
            w: 0.0,
            h: 0.0,
            d: 0.0,
            items: Vec::new(),
        }
    }

    /// `other` put after this one, its baseline `raise` over this one's.
    fn push(&mut self, other: Piece, raise: f32) {
        let x = self.w;
        self.items
            .extend(other.items.into_iter().map(|i| i.moved(x, -raise)));
        self.w += other.w;
        self.h = self.h.max(other.h + raise);
        self.d = self.d.max(other.d - raise);
    }
}

/// Whether `text` has anything to stack (a fraction or power the bar would stack).
pub fn stacks(text: &str) -> bool {
    stacked::may_stack(text) && !stacked::parse(&w(text), &[]).is_empty()
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `text` (a row's expression, `sqrt(` already shown as `√(`) laid out stacked at `size`.
pub fn picture(gfx: &Gfx, text: &str, size: f32) -> Picture {
    let u = w(text);
    let p = stacked::parse(&u, &[]);
    let b = Builder {
        gfx,
        u: &u,
        p: &p,
        line: size,
    };
    let piece = b.piece(0, u.len(), size);
    Picture {
        w: piece.w,
        h: piece.h,
        d: piece.d,
        items: piece.items,
    }
}

struct Builder<'a> {
    gfx: &'a Gfx,
    u: &'a [u16],
    p: &'a Parsed,
    /// the row's own size: the bar's sizes go by it
    line: f32,
}

const ROOT: u16 = 0x221A;
const CUBE: u16 = 0x221B;

impl Builder<'_> {
    /// The characters in `a..b` at `size`, a stack, root or grown group that starts in it drawn as one piece.
    fn piece(&self, a: usize, b: usize, size: f32) -> Piece {
        let (u, p) = (self.u, self.p);
        let close_of = stacked::bracket_pairs(u);
        let mut out = Piece::empty();
        let mut run: Vec<u16> = Vec::new();
        let mut after_stack = false;
        let mut i = a;
        while i < b {
            let c = u[i];
            // a root over a bracketed group: the sign and its bar drawn around it
            if (c == ROOT || c == CUBE)
                && i + 1 < b
                && u[i + 1] == 0x28
                && let Some(k) = close_of[i + 1].filter(|&k| k < b)
            {
                self.flush(&mut out, &mut run, size, &mut after_stack);
                out.push(self.root(i + 1, k + 1, size, c == CUBE), 0.0);
                after_stack = true;
                i = k + 1;
                continue;
            }
            // a root over a run of digits, letters and π: its bar over it
            if (c == ROOT || c == CUBE) && i + 1 < b && !p.stacks.iter().any(|s| s.start == i) {
                let mut j = i + 1;
                while j < b
                    && !p.hidden.contains(&j)
                    && (char::from_u32(u32::from(u[j])).is_some_and(char::is_alphanumeric)
                        || u[j] == 0x2E)
                {
                    j += 1;
                }
                if j > i + 1 {
                    self.flush(&mut out, &mut run, size, &mut after_stack);
                    out.push(self.root(i + 1, j, size, c == CUBE), 0.0);
                    after_stack = true;
                    i = j;
                    continue;
                }
            }
            if let Some(t) = p.tall.iter().find(|t| t.open == i && t.close < b) {
                self.flush(&mut out, &mut run, size, &mut after_stack);
                out.push(self.group(t.open + 1, t.close, size, c == 0x5B), 0.0);
                after_stack = true;
                i = t.close + 1;
                continue;
            }
            if let Some(s) = p.stacks.iter().find(|s| s.start == i && s.end <= b) {
                self.flush(&mut out, &mut run, size, &mut after_stack);
                if let (true, Some(num)) = (s.fraction, s.num) {
                    out.push(self.fraction(num, s.slot, size), 0.0);
                } else {
                    let f = stacked::slot_size(size, false, self.line, stacked::FRACTION_SCALE);
                    let raise = stacked::sup_shift(p, s, size, self.line, stacked::FRACTION_SCALE);
                    out.push(self.slot(s.slot, f, false), raise);
                }
                after_stack = true;
                i = s.end;
                continue;
            }
            if p.hidden.contains(&i) {
                if p.dots.contains(&i) {
                    self.flush(&mut out, &mut run, size, &mut after_stack);
                    out.w += size * 0.14;
                }
                i += 1;
                continue;
            }
            run.push(c);
            i += 1;
        }
        self.flush(&mut out, &mut run, size, &mut after_stack);
        out
    }

    /// A run of plain text at `size`, its signs spaced as the bar spaces them.
    fn flush(&self, out: &mut Piece, run: &mut Vec<u16>, size: f32, after_stack: &mut bool) {
        if run.is_empty() {
            return;
        }
        let s = spaced(run, *after_stack);
        run.clear();
        *after_stack = false;
        let Some(l) = self.gfx.layout(&s, &self.gfx.fonts.body, 0.0, size * 2.0) else {
            return;
        };
        let n = s.encode_utf16().count() as u32;
        // SAFETY: a setter on a live layout, the range its whole text
        unsafe {
            let _ = l.SetFontSize(
                size,
                DWRITE_TEXT_RANGE {
                    startPosition: 0,
                    length: n,
                },
            );
        }
        let base = stacked_layout::baseline(&l);
        let width = measure(&l).0;
        let tails = s.encode_utf16().any(tex::tail);
        out.push(
            Piece {
                w: width,
                h: size * tex::HEIGHT,
                d: if tails { size * tex::DEPTH } else { 0.0 },
                items: vec![Item::Text(l, 0.0, -base)],
            },
            0.0,
        );
    }

    /// A slot's characters at `f`, an empty one as a muted box; `overhang`: room either side for the bar.
    fn slot(&self, s: stacked::Slot, f: f32, overhang: bool) -> Piece {
        let mut out = if s.is_empty() {
            Piece {
                w: f * 0.62,
                h: f * tex::HEIGHT,
                d: 0.0,
                items: vec![Item::Hole(Rect::new(0.0, -f * 0.72, f * 0.62, f * 0.8))],
            }
        } else {
            self.piece(s.lo, s.hi, f)
        };
        if overhang {
            let pad = f * stacked::BAR_OVERHANG;
            out.items = out.items.into_iter().map(|i| i.moved(pad, 0.0)).collect();
            out.w += 2.0 * pad;
        }
        out
    }

    /// TeX's rule 15 at the row's size: the numerator its gap over the bar, the denominator its gap under it.
    fn fraction(&self, num: stacked::Slot, den: stacked::Slot, size: f32) -> Piece {
        let f = stacked::slot_size(size, true, self.line, stacked::FRACTION_SCALE);
        let (n, d) = (self.slot(num, f, true), self.slot(den, f, true));
        let axis = size * tex::AXIS;
        let up = stacked::num_offset(axis, size, n.d, stacked::FRACTION_SCALE);
        let down = -stacked::den_offset(axis, size, d.h, stacked::FRACTION_SCALE);
        let wide = n.w.max(d.w);
        let side = size * 0.08;
        let thick = (size * tex::RULE).max(1.0);
        let mut items = vec![Item::Rule(Rect::new(
            side,
            -axis - thick / 2.0,
            wide,
            thick,
        ))];
        let (nx, dx) = (side + (wide - n.w) / 2.0, side + (wide - d.w) / 2.0);
        items.extend(n.items.into_iter().map(|i| i.moved(nx, -up)));
        items.extend(d.items.into_iter().map(|i| i.moved(dx, down)));
        Piece {
            w: wide + 2.0 * side,
            h: (up + n.h).max(axis + thick),
            d: down + d.d,
            items,
        }
    }

    /// A root over `a..b`: its sign before it and its bar TeX's gap over it (rule 11), its foot under it.
    fn root(&self, a: usize, b: usize, size: f32, cube: bool) -> Piece {
        let inner = self.piece(a, b, size);
        let lw = stacked::root_line_width(size);
        let sign = size * 0.66;
        let top = -(inner.h.max(size * tex::HEIGHT) + tex::radical_gap(size) + lw / 2.0)
            .max(stacked::root_top(size) - lw / 2.0);
        let foot = (inner.d + tex::RULE * size).max(size * 0.02);
        let pts = stacked_layout::root_points_between(0.0, sign, sign + inner.w, top, foot);
        let mut items = vec![Item::Root(pts, size, cube.then_some((0.0, 0.0)))];
        items.extend(inner.items.into_iter().map(|i| i.moved(sign, 0.0)));
        Piece {
            w: sign + inner.w,
            h: -top + lw / 2.0,
            d: foot + lw / 2.0,
            items,
        }
    }

    /// Shown brackets around a stack, as tall as what they hold (TeX's \left \right).
    fn group(&self, a: usize, b: usize, size: f32, square: bool) -> Piece {
        let inner = self.piece(a, b, size);
        let half = tex::bracket_half(inner.h, inner.d, size);
        let axis = size * tex::AXIS;
        let bw = size * 0.33;
        let (top, bottom) = (-axis - half, half - axis);
        let mut items = vec![
            Item::Bracket((0.0, bw), (top, bottom), size, (square, false)),
            Item::Bracket((bw + inner.w, bw), (top, bottom), size, (square, true)),
        ];
        items.extend(inner.items.into_iter().map(|i| i.moved(bw, 0.0)));
        Piece {
            w: inner.w + 2.0 * bw,
            h: inner.h.max(-top),
            d: inner.d.max(bottom),
            items,
        }
    }
}

/// Room around `+ − =` between two terms and after `; ,`, as the bar spaces them: thin spaces in the run.
fn spaced(run: &[u16], after_stack: bool) -> String {
    let mut out = Vec::with_capacity(run.len() * 2);
    let binary = |c: u16| "+-−=<>≤≥±≠".encode_utf16().any(|b| b == c);
    for (k, &c) in run.iter().enumerate() {
        let prev = (k > 0).then(|| run[k - 1]);
        let term = prev.map_or(after_stack, |p| {
            stacked::is_word(p) || [0x29, 0x5D, 0x21, 0x25].contains(&p)
        });
        let exponent = (c == 0x2D || c == 0x2B)
            && matches!(prev, Some(0x65 | 0x45))
            && k > 1
            && stacked::is_digit(run[k - 2]);
        if binary(c) && term && !exponent && (k + 1 < run.len() || after_stack) {
            out.extend([0x2009, c, 0x2009]);
        } else if c == 0x3B || c == 0x2C {
            out.extend([c, 0x2009]);
        } else {
            out.push(c);
        }
    }
    String::from_utf16_lossy(&out)
}

impl Picture {
    /// Draws the picture with its left at `x` and its baseline at `base`, in `color`.
    pub fn draw(&self, c: &Canvas, x: f32, base: f32, color: Color) {
        for item in &self.items {
            match item.clone().moved(x, base) {
                Item::Text(l, lx, ly) => c.text(&l, lx, ly, color),
                Item::Rule(r) => c.fill(r, color),
                Item::Hole(r) => {
                    c.fill_round(r, r.h * 0.18, crate::render::with_alpha(color, 0.18))
                }
                Item::Root(pts, size, cube) => {
                    let at = cube.unwrap_or((pts[0].0, base));
                    stacked_layout::stroke_root(c, &pts, at, size, cube.is_some(), color);
                }
                Item::Bracket(lw, tb, size, k) => {
                    stacked_layout::tall_bracket(c, lw, tb, size, k, color)
                }
            }
        }
    }

    /// Where the baseline goes in a row `h` tall whose plain text sits on `plain`: there, unless the picture would
    /// run off the top or the bottom (then as far as it must, centred when it can't fit at all).
    pub fn baseline_in(&self, plain: f32, h: f32) -> f32 {
        let (lowest, highest) = (self.h + 1.0, h - self.d - 1.0);
        if lowest > highest {
            (lowest + highest) / 2.0
        } else {
            plain.clamp(lowest, highest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_get_thin_spaces_as_the_bar_spaces_them() {
        assert_eq!(spaced(&w("1+2"), false), "1\u{2009}+\u{2009}2");
        assert_eq!(spaced(&w("-3"), false), "-3");
        assert_eq!(spaced(&w("1e-5"), false), "1e-5");
        assert_eq!(spaced(&w("+1"), true), "\u{2009}+\u{2009}1");
        assert_eq!(spaced(&w(";x=2"), true), ";\u{2009}x\u{2009}=\u{2009}2");
    }

    #[test]
    fn a_row_stacks_what_the_bar_stacks() {
        assert!(stacks("1/(2)") && stacks("x^2") && !stacks("2x/3") && !stacks("1+2"));
    }

    #[test]
    fn a_tall_picture_moves_the_baseline_only_as_far_as_it_must() {
        let p = Picture {
            w: 10.0,
            h: 20.0,
            d: 14.0,
            items: Vec::new(),
        };
        // a plain row's baseline at 23 would cut the depth off: up as far as its depth needs
        assert_eq!(p.baseline_in(23.0, 36.0), 21.0);
        let q = Picture {
            h: 9.0,
            d: 3.0,
            ..p.clone()
        };
        assert_eq!(q.baseline_in(23.0, 36.0), 23.0);
    }
}
