//! How the input line draws its maths (StackedLayout.swift, RadicalText.swift `RootSign`): stacked fractions and
//! powers (stacked.rs says what's stacked and how big each slot is, by TeX's rules), brackets grown around them, room
//! around the signs, and a typed root's sign and bar as one stroke (as tall as a stack it takes).
//!
//! A stack is drawn from the line's one DirectWrite layout. Each slot's characters are made small in it and the
//! hidden ones (`/ ^ ( ) ·`) all but vanish, so the layout keeps every character's advance; but DirectWrite never
//! lets character spacing take an advance below nothing (the Mac's TextKit has the same rule, its `pull`), so the
//! layout can't put a denominator back under its numerator. The line's places are worked out here instead
//! (`Geom::xs`: the layout's own advances, plus each stack's room and pull-back), and the layout is drawn in runs,
//! each moved sideways and up or down to where it goes and clipped to its own glyphs. Everything the field draws by
//! position (selection, caret, squiggle, offered unit) asks `xs`, so a line with stacks draws as one without.
//!
//! The line sits on a plain line's baseline in the composer's 72 DIPs (the 36 DIP row and 18 above and below it);
//! a line too tall for that moves up or down until all of it shows (`Geom::dy`, updateInset), and one that still
//! doesn't fit at 90% fractions is set in TeX's text style (stacked.rs `fits`).

use crate::render::{Canvas, Color, Rect, caret_x, size, with_alpha};
use crate::stacked::{self, Look, Parsed, tex};
use std::cell::{OnceCell, RefCell};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_LINE_JOIN_ROUND,
    D2D1_STROKE_STYLE_PROPERTIES, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_LINE_METRICS, DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_RANGE, IDWriteTextLayout,
};
use windows_numerics::Vector2;

/// A hidden character's size, a sliver of the line's (as the ∫ Σ Π limits' separators).
const HIDDEN: f32 = size::DISPLAY * 0.01;
/// How far the line may reach above and below its 36 DIP row: the composer's room (stackRoom).
pub const ROOM: f32 = 18.0;

/// A fraction's bar: from `x`, `w` wide, `axis` above the line's baseline, `thick` thick; its fraction takes
/// `start..end`.
#[derive(Clone, Copy, Debug)]
struct Bar {
    x: f32,
    w: f32,
    axis: f32,
    thick: f32,
    start: usize,
    end: usize,
}

/// An empty slot's muted box: its left, how far its slot's baseline is raised, its slot's size; `open` is the
/// slot's hidden `(`.
#[derive(Clone, Copy, Debug)]
struct Hole {
    open: usize,
    x: f32,
    raise: f32,
    size: f32,
}

/// Shown brackets drawn as tall as the stack they hold (TallBracket): `( )` or `[ ]` at `open` and `close`, centred
/// `axis` over the line's baseline, `half` each way; `width` the glyph's own advance and `room` the more it gets on
/// its inner side.
#[derive(Clone, Copy, Debug)]
struct Tall {
    open: usize,
    close: usize,
    square: bool,
    axis: f32,
    half: f32,
    size: f32,
    width: f32,
    room: f32,
}

/// A run of the layout drawn as one piece: characters `a..b`, which the layout put at `from` and the line puts at
/// `xs[a]`, raised by `raise`.
#[derive(Clone, Copy, Debug)]
struct Run {
    a: usize,
    from: f32,
    w: f32,
    raise: f32,
    /// room past the run's own advances for glyphs that overhang, where nothing drawn is beside it
    pad_l: f32,
    pad_r: f32,
}

/// Each character's size and raise, each slot's look, and a fraction's size in the line (`top`: 90%, or 70% on a
/// line too tall for the bar).
pub struct Sizes {
    sizes: Vec<f32>,
    raise: Vec<f32>,
    looks: Vec<Look>,
    top: f32,
}

/// Where a stacked line's places and pieces go, from the layout's left and top.
pub struct Geom {
    /// the caret place before each UTF-16 position, 0 to the line's length
    pub xs: Vec<f32>,
    pub width: f32,
    /// the line's baseline from the layout's top (moved by `dy`)
    pub base: f32,
    /// how far the line is moved down so all of it shows (up when negative)
    pub dy: f32,
    /// each character's size and raise
    sizes: Vec<f32>,
    raise: Vec<f32>,
    looks: Vec<Look>,
    top: f32,
    runs: Vec<Run>,
    bars: Vec<Bar>,
    holes: Vec<Hole>,
    talls: Vec<Tall>,
    /// how far back the caret at each place is drawn: the middle of a spaced sign's room
    shift: Vec<f32>,
    /// the line's stacks
    parsed: Parsed,
}

thread_local! {
    /// the places of the line last laid out, for ↑ ↓ (the keys are handled where the view isn't)
    static PLACES: RefCell<(String, Vec<f32>)> = const { RefCell::new((String::new(), Vec::new())) };
    static ROUND: OnceCell<Option<ID2D1StrokeStyle>> = const { OnceCell::new() };
}

fn range(a: usize, b: usize) -> DWRITE_TEXT_RANGE {
    DWRITE_TEXT_RANGE {
        startPosition: a as u32,
        length: b.saturating_sub(a) as u32,
    }
}

/// Whether a line with no stack still gets a geometry while the setting is on: one with a sign to give room to.
pub fn spaced(text: &str) -> bool {
    text.contains(['+', '-', '−', '=', '<', '>', '≤', '≥', '±', '≠', ';', ','])
}

/// The slots' sizes and the hidden characters' sliver set on the line's layout (before the limits' own and the
/// gaps): what `build` measures. A line too tall for the composer at Desmos's 90% fractions is set in TeX's text
/// style (70%).
pub fn apply_sizes(l: &IDWriteTextLayout, p: &Parsed, n: usize) -> Sizes {
    let top = if p.is_empty() || stacked::fits(p, size::DISPLAY, 36.0 + 2.0 * ROOM) {
        stacked::FRACTION_SCALE
    } else {
        tex::SCRIPT
    };
    let (sizes, raise, looks) = stacked::looks(p, n, size::DISPLAY, top);
    // SAFETY: setters on a live layout, ranges inside the text
    unsafe {
        for look in &looks {
            if look.hi > look.lo {
                let _ = l.SetFontSize(look.size, range(look.lo, look.hi));
            }
        }
        for &k in &p.hidden {
            let _ = l.SetFontSize(HIDDEN, range(k, k + 1));
        }
    }
    Sizes {
        sizes,
        raise,
        looks,
        top,
    }
}

/// A one-line layout's baseline from its top.
pub fn baseline(l: &IDWriteTextLayout) -> f32 {
    let mut lines = [DWRITE_LINE_METRICS::default()];
    let mut count = 0u32;
    let (mut x, mut y) = (0f32, 0f32);
    let mut m = Default::default();
    // SAFETY: a live layout and live out locals
    unsafe {
        let _ = l.GetLineMetrics(Some(&mut lines), &mut count);
        let _ = l.HitTestTextPosition(0, false, &mut x, &mut y, &mut m);
    }
    y + lines[0].baseline
}

/// The line's baseline from the top of an `h`-high layout, and its line height, at the display size (a line made
/// only of small slots would otherwise sit lower).
fn reference(c: &Canvas, h: f32) -> (f32, f32) {
    let Some(l) = c.gfx.layout("0", &c.gfx.fonts.display, 0.0, h) else {
        return (h * 0.5 + 8.0, 32.0);
    };
    let mut lines = [DWRITE_LINE_METRICS::default()];
    let mut count = 0u32;
    let (mut x, mut y) = (0f32, 0f32);
    let mut m = Default::default();
    // SAFETY: a live layout and live out locals
    unsafe {
        let _ = l.GetLineMetrics(Some(&mut lines), &mut count);
        let _ = l.HitTestTextPosition(0, false, &mut x, &mut y, &mut m);
    }
    (y + lines[0].baseline, lines[0].height)
}

/// How far a line `(height, depth)` from its baseline at `at` (from the view's top, `room` tall) moves so all of
/// it shows: its baseline stays a plain line's until the stacks would run off the bottom or the top
/// (updateInset).
pub fn shift_into(e: (f32, f32), rooted: bool, at: f32, room: f32) -> f32 {
    let (lowest, highest) = stacked::baseline_range(e, rooted, size::DISPLAY, room);
    let to = if lowest > highest {
        (lowest + highest) / 2.0
    } else {
        at.clamp(lowest, highest)
    };
    to - at
}

/// The stacked line's places and pieces, from its layout once the sizes (`apply_sizes`), the limits' and the gaps'
/// spacing are on it.
pub fn build(c: &Canvas, l: &IDWriteTextLayout, text: &str, p: &Parsed, sz: Sizes, h: f32) -> Geom {
    let u: Vec<u16> = text.encode_utf16().collect();
    let n = sz.sizes.len().min(u.len());
    let (base, line_h) = reference(c, h);
    // SAFETY: a setter on a live layout: the line keeps the display size's height and baseline
    unsafe {
        let _ = l.SetLineSpacing(
            DWRITE_LINE_SPACING_METHOD_UNIFORM,
            line_h,
            line_h * 0.5 + (base - h * 0.5),
        );
    }
    let from: Vec<f32> = (0..=n).map(|i| caret_x(l, i as u32)).collect();
    let adv: Vec<f32> = (0..n).map(|k| from[k + 1] - from[k]).collect();
    let pl = places(p, &u[..n], &sz, &adv);
    let width = pl.xs[n];
    let runs = runs(p, &sz.raise, &pl.extra, &from);
    let dy = if p.is_empty() {
        0.0
    } else {
        let e = stacked::extent(p, 0, n, size::DISPLAY, size::DISPLAY, sz.top);
        shift_into(e, stacked::rooted(p), base + ROOM, h + 2.0 * ROOM)
    };
    PLACES.with(|s| *s.borrow_mut() = (text.to_string(), pl.xs.clone()));
    Geom {
        xs: pl.xs,
        width,
        base: base + dy,
        dy,
        sizes: sz.sizes,
        raise: sz.raise,
        looks: sz.looks,
        top: sz.top,
        runs,
        bars: pl.bars,
        holes: pl.holes,
        talls: pl.talls,
        shift: pl.shift,
        parsed: p.clone(),
    }
}

/// What `places` works out.
struct Places {
    xs: Vec<f32>,
    bars: Vec<Bar>,
    holes: Vec<Hole>,
    talls: Vec<Tall>,
    /// each character's added advance
    extra: Vec<f32>,
    shift: Vec<f32>,
}

/// A slot's size and raise, from its look.
fn look_of(looks: &[Look], slot: stacked::Slot, or: (f32, f32)) -> (f32, f32) {
    looks
        .iter()
        .find(|l| l.lo == slot.lo && l.hi == slot.hi)
        .map_or(or, |l| (l.size, l.offset))
}

/// The places (StackTypesetter's pen): each character's advance `adv`, plus the room each stack makes and the
/// pull back to its denominator, inner stacks first so an outer slot's width takes in the stacks inside it; then
/// the room grown brackets get, and the room around signs.
fn places(p: &Parsed, u: &[u16], sz: &Sizes, adv: &[f32]) -> Places {
    let n = adv.len();
    let (sizes, raise, looks, top) = (&sz.sizes, &sz.raise, &sz.looks, sz.top);
    let mut extra = vec![0.0f32; n];
    let mut lead = 0.0f32;
    // each bar and box with the character it hangs from, `x` from that character's place (known only at the end)
    let mut bars: Vec<(usize, Bar)> = Vec::new();
    let mut holes: Vec<(usize, Hole)> = Vec::new();
    let width =
        |extra: &[f32], a: usize, b: usize| -> f32 { (a..b).map(|k| adv[k] + extra[k]).sum() };
    let mut order: Vec<&stacked::Stack> = p.stacks.iter().collect();
    order.sort_by_key(|s| s.end - s.start);
    for s in order {
        let (parent, base) = (sizes[s.op], raise[s.op]);
        let f = stacked::slot_size(parent, s.fraction, size::DISPLAY, top);
        let hole = f * 0.62;
        if let (true, Some(num)) = (s.fraction, s.num) {
            let den = s.slot;
            let nw = if num.is_empty() {
                hole
            } else {
                width(&extra, num.lo, num.hi)
            };
            let dw = if den.is_empty() {
                hole
            } else {
                width(&extra, den.lo, den.hi)
            };
            let pad = f * stacked::BAR_OVERHANG;
            let w = nw.max(dw) + 2.0 * pad;
            let room = (w - nw) / 2.0;
            if let Some(o) = num.open {
                extra[o] += room + if num.is_empty() { hole } else { 0.0 };
                if num.is_empty() {
                    let (size, raise) = look_of(looks, num, (f, base));
                    holes.push((
                        o,
                        Hole {
                            open: o,
                            x: room,
                            raise,
                            size,
                        },
                    ));
                }
            } else if num.lo > 0 {
                extra[num.lo - 1] += room;
            } else {
                // the line starts with this fraction: the whole line starts that much further on
                lead += room;
            }
            // the `/` takes the pen back to where the denominator starts, centred under the numerator
            extra[s.op] += (w - dw) / 2.0 - room - nw;
            if let Some(o) = den.open
                && den.is_empty()
            {
                extra[o] += hole;
                let (size, raise) = look_of(looks, den, (f, base));
                holes.push((
                    o,
                    Hole {
                        open: o,
                        x: 0.0,
                        raise,
                        size,
                    },
                ));
            }
            // and the denominator's end takes it on past the wider of the two
            let last = den.close.unwrap_or(den.hi.saturating_sub(1));
            if last < n {
                extra[last] += (w - dw) / 2.0;
            }
            let shift = if num.open.is_none() { room } else { 0.0 };
            bars.push((
                num.open.unwrap_or(num.lo),
                Bar {
                    x: -shift,
                    w,
                    axis: base + parent * tex::AXIS,
                    thick: (parent * tex::RULE).max(1.0),
                    start: s.start,
                    end: s.end,
                },
            ));
        } else {
            if let Some(o) = s.slot.open
                && s.slot.is_empty()
            {
                extra[o] += hole;
                let (size, raise) = look_of(looks, s.slot, (f, base));
                holes.push((
                    o,
                    Hole {
                        open: o,
                        x: 0.0,
                        raise,
                        size,
                    },
                ));
            }
            if let Some(c) = s.slot.close {
                extra[c] += f * 0.1;
            }
        }
    }
    for &d in &p.dots {
        if d < n {
            extra[d] += sizes[d] * 0.14;
        }
    }
    // shown brackets around a stack: their glyphs hidden, drawn as tall as what they hold, with a little more room
    // as they grow
    let mut talls = Vec::new();
    for b in p.tall.iter().filter(|b| b.close < n) {
        let s = sizes[b.open];
        let e = stacked::extent(p, b.open + 1, b.close, s, size::DISPLAY, top);
        let half = tex::bracket_half(e.0, e.1, s);
        let room = (s * 0.25).min((half * 2.0 - s).max(0.0) * 0.08);
        talls.push(Tall {
            open: b.open,
            close: b.close,
            square: u[b.open] == 0x5B,
            axis: raise[b.open] + s * tex::AXIS,
            half,
            size: s,
            width: adv[b.open],
            room,
        });
        extra[b.open] += room;
        extra[b.close - 1] += room;
    }
    // room around the signs (setting on only), and the caret in the middle of it
    let mut shift = vec![0.0f32; n + 1];
    let (room, sh) = stacked::spacing(u, &p.hidden, |k| sizes[k]);
    for (k, v) in room {
        extra[k] += v;
    }
    for (k, v) in sh {
        shift[k] = v;
    }
    let mut xs = Vec::with_capacity(n + 1);
    let mut x = lead;
    for k in 0..n {
        xs.push(x);
        x += adv[k] + extra[k];
    }
    xs.push(x);
    let bars = bars
        .into_iter()
        .map(|(a, b)| Bar {
            x: xs[a] + b.x,
            ..b
        })
        .collect();
    let holes = holes
        .into_iter()
        .map(|(a, h)| Hole {
            x: xs[a] + h.x,
            ..h
        })
        .collect();
    Places {
        xs,
        bars,
        holes,
        talls,
        extra,
        shift,
    }
}

/// The runs to draw: unbroken stretches of shown characters at one height that the line keeps together as the
/// layout has them.
fn runs(p: &Parsed, raise: &[f32], extra: &[f32], from: &[f32]) -> Vec<Run> {
    let n = extra.len();
    let mut out = Vec::new();
    let shown = |k: usize| !p.hidden.contains(&k);
    let mut a = 0;
    while a < n {
        if !shown(a) {
            a += 1;
            continue;
        }
        let mut b = a + 1;
        while b < n && shown(b) && raise[b] == raise[a] && extra[b - 1] == 0.0 {
            b += 1;
        }
        // room for a glyph's overhang, as far as the layout's next shown glyph (hidden ones are slivers, so a
        // run beside a hidden `/` or `(` gets next to none)
        let pad_l = match (0..a).rev().find(|&k| shown(k)) {
            Some(k) => (from[a] - from[k + 1]).min(3.0),
            None => 3.0,
        };
        let pad_r = match (b..n).find(|&k| shown(k)) {
            Some(k) => (from[k] - from[b]).min(3.0),
            None => 3.0,
        };
        out.push(Run {
            a,
            from: from[a],
            w: from[b] - from[a],
            raise: raise[a],
            pad_l,
            pad_r,
        });
        a = b;
    }
    out
}

/// The places of `text` as last laid out, for ↑ ↓; by position when the line has changed since.
pub fn places_of(text: &str) -> Vec<f32> {
    PLACES.with(|s| {
        let s = s.borrow();
        if s.0 == text {
            s.1.clone()
        } else {
            (0..=text.encode_utf16().count())
                .map(|i| i as f32)
                .collect()
        }
    })
}

/// ↑ ↓ between a fraction's numerator and denominator, before the history takes them (BarTextView.stackArrow).
pub fn key_down(m: &mut crate::model::BarModel, vk: u32, mods: crate::keys::Mods) -> bool {
    const UP: u32 = 0x26;
    const DOWN: u32 = 0x28;
    if (vk != UP && vk != DOWN)
        || mods.ctrl
        || mods.alt
        || mods.shift
        || mods.win
        || m.selected.is_some()
        || m.field.stacks().is_empty()
    {
        return false;
    }
    let xs = places_of(m.field.text());
    let moved = m
        .field
        .stack_vertical(vk == UP, |i| xs.get(i).copied().unwrap_or(f32::MAX));
    if moved {
        m.touch();
    }
    moved
}

thread_local! {
    /// keys with modifiers from the bench pipe, waiting for the UI thread (`debug_key`)
    static KEYS: RefCell<Vec<(u32, crate::keys::Mods)>> = const { RefCell::new(Vec::new()) };
}

/// The bench pipe's `mkey <mods> <vk>` (`mkey ctrl+shift 37`, `mkey - 8`): the key through the bar's own handler
/// with those modifiers held, as the e2e runner can't hold them (bench.rs `extra`). Debug only.
pub fn debug_key(cmd: &str) -> serde_json::Value {
    let mut parts = cmd.split_whitespace().skip(1);
    // `mkey click <x> <y>`: a click on the line at window DIPs (x, y), as app.rs `click` places the caret
    if cmd.starts_with("mkey click ") {
        let mut n = parts.skip(1).filter_map(|v| v.parse::<f32>().ok());
        let (Some(x), Some(y)) = (n.next(), n.next()) else {
            return serde_json::json!({ "error": "mkey click <x> <y>" });
        };
        crate::later::after("stacked-debug-click", 0, move |app| {
            let pos = app.bar.field_view.hit(x, y);
            app.model.field.set_caret_utf16(pos, false);
            app.model.touch();
        });
        return serde_json::json!({ "ok": true });
    }
    let mods = parts.next().unwrap_or("-");
    let Some(vk) = parts.next().and_then(|v| v.parse::<u32>().ok()) else {
        return serde_json::json!({ "error": "mkey <mods> <vk>" });
    };
    let m = crate::keys::Mods {
        ctrl: mods.contains("ctrl"),
        alt: mods.contains("alt"),
        shift: mods.contains("shift"),
        win: false,
    };
    KEYS.with(|k| k.borrow_mut().push((vk, m)));
    crate::later::after("stacked-debug-key", 0, |app| {
        for (vk, mods) in KEYS.with(|k| std::mem::take(&mut *k.borrow_mut())) {
            let _ = app.model.key_down(vk, mods, &app.engine, || None);
        }
    });
    serde_json::json!({ "ok": true })
}

impl Geom {
    /// The caret place before UTF-16 `i`.
    pub fn x(&self, i: u32) -> f32 {
        self.xs
            .get(i as usize)
            .or(self.xs.last())
            .copied()
            .unwrap_or(0.0)
    }

    /// Where the caret at UTF-16 `i` is drawn: beside a spaced sign, in the middle of its room (caretShift).
    pub fn caret_x(&self, i: u32) -> f32 {
        self.x(i) - self.shift.get(i as usize).copied().unwrap_or(0.0)
    }

    /// A character's size and raise (the line's own past its end).
    pub fn look_of(&self, k: usize) -> (f32, f32) {
        (
            self.sizes.get(k).copied().unwrap_or(size::DISPLAY),
            self.raise.get(k).copied().unwrap_or(0.0),
        )
    }

    /// The selection's highlight from UTF-16 `a` to `b`: (left, right), out to the bars of the fractions it takes.
    pub fn span(&self, a: u32, b: u32) -> (f32, f32) {
        let (mut l, mut r) = (self.x(a), self.x(b));
        for bar in &self.bars {
            if bar.start >= a as usize && bar.end <= b as usize {
                l = l.min(bar.x);
                r = r.max(bar.x + bar.w);
            }
        }
        (l, r)
    }

    /// Draws the line's text in its runs, `x0`, `y` the row's top-left; the layout's drawing effects colour it.
    pub fn draw(&self, c: &Canvas, l: &IDWriteTextLayout, x0: f32, y: f32, h: f32, color: Color) {
        for r in &self.runs {
            let at = x0 + self.xs[r.a];
            c.push_clip(Rect::new(
                at - r.pad_l,
                y - ROOM,
                r.w + r.pad_l + r.pad_r,
                h + 2.0 * ROOM,
            ));
            c.text(l, at - r.from, y + self.dy - r.raise, color);
            c.pop_clip();
        }
    }

    /// The bars, the grown brackets and the empty slots' boxes.
    pub fn decorate(&self, c: &Canvas, x0: f32, y: f32) {
        let t = c.theme;
        let base = y + self.base;
        for b in &self.bars {
            c.fill(
                Rect::new(x0 + b.x, base - b.axis - b.thick / 2.0, b.w, b.thick),
                t.ink,
            );
        }
        for b in &self.talls {
            let (top, bottom) = (base - b.axis - b.half, base - b.axis + b.half);
            let w = b.width + b.room;
            tall_bracket(
                c,
                (x0 + self.x(b.open as u32), w),
                (top, bottom),
                b.size,
                (b.square, false),
                t.ink,
            );
            let r = x0 + self.x(b.close as u32) - b.room;
            tall_bracket(c, (r, w), (top, bottom), b.size, (b.square, true), t.ink);
        }
        for h in &self.holes {
            c.fill_round(
                Rect::new(
                    x0 + h.x,
                    base - h.raise - h.size * 0.72,
                    h.size * 0.62,
                    h.size * 0.8,
                ),
                h.size * 0.14,
                with_alpha(t.muted, 0.18),
            );
        }
    }

    /// A selection's highlight band from UTF-16 `a` to `b` inside one slot: (top, height) from the layout's top, the
    /// slot's height; None for one that takes whole stacks (the line's height).
    pub fn band(&self, a: u32, b: u32) -> Option<(f32, f32)> {
        let slot = self.parsed.slot(a as usize)?;
        if !slot.holds(b as usize) {
            return None;
        }
        let look = stacked::look_at(&self.parsed, &self.looks, a as usize)?;
        Some((self.base - look.offset - look.size * 0.95, look.size * 1.25))
    }

    /// The caret at UTF-16 `caret` while the line has stacks: (x, top, height), the size of the slot it's in (the
    /// line's outside them), from the layout's top-left; None on a line with no stack (the line's own caret).
    pub fn caret(&self, caret: usize) -> Option<(f32, f32, f32)> {
        let p = &self.parsed;
        if p.is_empty() {
            return None;
        }
        let (size, offset) = stacked::look_at(p, &self.looks, caret)
            .map_or((size::DISPLAY, 0.0), |l| (l.size, l.offset));
        let mut x = self.caret_x(caret as u32);
        // in an empty slot, the middle of its box
        if let Some(slot) = p.slot(caret)
            && slot.is_empty()
            && let Some(o) = slot.open
            && let Some(h) = self.holes.iter().find(|h| h.open == o)
        {
            x = h.x + h.size * 0.31;
        }
        Some((x, self.base - offset - size * 0.92, size * 1.15))
    }

    /// The caret place nearest a click at (x, y) from the layout's top-left, by its slot's height too.
    pub fn stop_near(&self, x: f32, y: f32) -> u32 {
        let p = &self.parsed;
        let mut best = (0usize, f32::MAX);
        for (i, &px) in self.xs.iter().enumerate() {
            if p.dead.contains(&i) {
                continue;
            }
            let (size, raise) = stacked::look_at(p, &self.looks, i)
                .map_or((size::DISPLAY, 0.0), |l| (l.size, l.offset));
            let py = self.base - raise - size * 0.35;
            let d = (px - x).abs() + 1.5 * (py - y).abs();
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0 as u32
    }

    /// Where the blank-answer squiggle under UTF-16 `a..b` goes, from the layout's top: under the lowest character
    /// it marks (a slot's sit lower, smaller), never past the composer's bottom (`h` the row's height).
    pub fn squiggle_y(&self, a: u32, b: u32, h: f32) -> f32 {
        let low = (a as usize..b as usize)
            .filter(|k| !self.parsed.hidden.contains(k) && *k < self.sizes.len())
            .map(|k| self.base - self.raise[k] + self.sizes[k] * tex::DEPTH)
            .fold(f32::MIN, f32::max);
        let low = if low == f32::MIN {
            self.base + size::DISPLAY * tex::DEPTH
        } else {
            low
        };
        (low + 1.0).min(h + ROOM - 1.0)
    }

    /// A root over the radicand `a..b` (UTF-16) at `size` that takes a stack: its bar's top and its foot, from the
    /// layout's top, as tall as what it takes (TeX's rule 11: the bar its gap over what's under it, the foot under
    /// it), and the line's baseline under it. None for a root over plain text.
    pub fn root_over(&self, a: usize, b: usize, size: f32) -> Option<(f32, f32, f32)> {
        let p = &self.parsed;
        let s = p.stacks.iter().find(|s| s.start >= a && s.end <= b)?;
        let baseline = self.base - self.raise.get(s.op).copied().unwrap_or(0.0);
        let e = stacked::extent(p, a, b, size, size::DISPLAY, self.top);
        let plain = root_points(0.0, 1.0, 1.0, baseline, size);
        let lw = stacked::root_line_width(size);
        let top = plain[3]
            .1
            .min(baseline - e.0 - tex::radical_gap(size) - lw / 2.0);
        let foot = plain[2].1.max(baseline + e.1 + tex::RULE * size);
        Some((top, foot, baseline))
    }
}

// ---- a bracket as tall as what it holds (TallBracketShape)

/// A bracket drawn as tall as what it holds: a round one a crescent thickest at its middle, a square one a bar with
/// feet. In a box `width` wide from `left` (the opening one's glyph and the room after it, the closing one's room
/// before it and its glyph), from `top` to `bottom`, y growing down. The line and the history's rows draw it.
pub fn tall_bracket(
    c: &Canvas,
    (left, width): (f32, f32),
    (top, bottom): (f32, f32),
    size: f32,
    (square, closing): (bool, bool),
    color: Color,
) {
    let t = (size * 0.07).max(1.0);
    // mirrored for the closing one: x measured from the box's right edge
    let x = |v: f32| if closing { left + width - v } else { left + v };
    let span = |a: f32, b: f32| (x(a).min(x(b)), (x(b) - x(a)).abs());
    if square {
        let foot = (width * 0.6).min(size * 0.3);
        let stem = width * 0.3;
        let (sx, sw) = span(stem, stem + t);
        let (fx, fw) = span(stem, stem + foot);
        c.fill(Rect::new(sx, top, sw, bottom - top), color);
        c.fill(Rect::new(fx, top, fw, t * 0.8), color);
        c.fill(Rect::new(fx, bottom - t * 0.8, fw, t * 0.8), color);
        return;
    }
    let (tip, bulge, h) = (width * 0.78, width * 0.12, bottom - top);
    let v = |x: f32, y: f32| Vector2 { X: x, Y: y };
    // SAFETY: a fresh geometry opened, filled and closed here, drawn inside the frame
    unsafe {
        let Ok(geo) = c.gfx.d2d.CreatePathGeometry() else {
            return;
        };
        let Ok(sink) = geo.Open() else { return };
        sink.BeginFigure(v(x(tip), top), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
            point1: v(x(bulge), top + h * 0.2),
            point2: v(x(bulge), bottom - h * 0.2),
            point3: v(x(tip), bottom),
        });
        sink.AddLine(v(x(tip + t * 0.25), bottom));
        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
            point1: v(x(bulge + t * 1.6), bottom - h * 0.22),
            point2: v(x(bulge + t * 1.6), top + h * 0.22),
            point3: v(x(tip + t * 0.25), top),
        });
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        if sink.Close().is_err() {
            return;
        }
        c.rt().FillGeometry(&geo, c.brush(color), None);
    }
}

// ---- a typed root: sign and bar, one stroke (RootSign)

/// The root's one line: a small tick up to the hook, down to the foot, up to the top, and on over what it takes
/// to `end`. `left` and `width` are the sign's own room; `size` the sign's size; y grows down.
pub fn root_points(left: f32, width: f32, end: f32, baseline: f32, size: f32) -> [(f32, f32); 5] {
    // Segoe UI's cap height; the top is clear of what's under it, as Desmos leaves it
    let top = baseline - size * tex::CAP - size * 0.2;
    let foot = baseline + size * 0.02;
    root_points_between(left, width, end, top, foot)
}

/// The same line between a given top and foot: a root over a stack reaches from above its numerator to below its
/// denominator.
pub fn root_points_between(
    left: f32,
    width: f32,
    end: f32,
    top: f32,
    foot: f32,
) -> [(f32, f32); 5] {
    let height = foot - top;
    [
        (left + width * 0.1, foot - height * 0.36),
        (left + width * 0.26, foot - height * 0.44),
        (left + width * 0.5, foot),
        (left + width * 0.86, top),
        (end, top),
    ]
}

fn round(c: &Canvas) -> Option<ID2D1StrokeStyle> {
    ROUND.with(|r| {
        r.get_or_init(|| {
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            };
            // SAFETY: the properties outlive the call
            unsafe { c.gfx.d2d.CreateStrokeStyle(&props, None) }.ok()
        })
        .clone()
    })
}

/// A root's sign and bar as one stroke, from the sign's left (`left`, `width` its room) over to `end`, on the
/// baseline `baseline`, at `size`; a cube root's small 3 in its crook.
pub fn draw_root(
    c: &Canvas,
    (left, width): (f32, f32),
    end: f32,
    baseline: f32,
    size: f32,
    cube: bool,
    color: Color,
) {
    let pts = root_points(left, width, end, baseline, size);
    stroke_root(c, &pts, (left, baseline), size, cube, color);
}

/// The stroke through `pts` (`root_points`), and a cube root's 3 in the crook of a sign at `left` on `baseline`.
pub fn stroke_root(
    c: &Canvas,
    pts: &[(f32, f32); 5],
    (left, baseline): (f32, f32),
    size: f32,
    cube: bool,
    color: Color,
) {
    let stroke = stacked::root_line_width(size);
    // SAFETY: a fresh geometry opened, filled and closed here, drawn inside the frame
    unsafe {
        let Ok(geo) = c.gfx.d2d.CreatePathGeometry() else {
            return;
        };
        let Ok(sink) = geo.Open() else { return };
        sink.BeginFigure(
            Vector2 {
                X: pts[0].0,
                Y: pts[0].1,
            },
            D2D1_FIGURE_BEGIN_HOLLOW,
        );
        let rest: Vec<Vector2> = pts[1..]
            .iter()
            .map(|&(x, y)| Vector2 { X: x, Y: y })
            .collect();
        sink.AddLines(&rest);
        sink.EndFigure(D2D1_FIGURE_END_OPEN);
        if sink.Close().is_err() {
            return;
        }
        let style = round(c);
        c.rt()
            .DrawGeometry(&geo, c.brush(color), stroke, style.as_ref());
    }
    if cube {
        let small = size * 0.42;
        if let Some(l) = c.gfx.layout("3", &c.gfx.fonts.display, 0.0, small * 1.4) {
            // SAFETY: a setter on a live layout
            unsafe {
                let _ = l.SetFontSize(small, range(0, 1));
            }
            // its top at the crook: the cap height and a little above the baseline
            let top = baseline - size * tex::CAP - size * 0.24;
            c.text_plain(&l, left, top - small * 0.25, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// `t`'s places with every shown character 10 wide at its size and the hidden ones nothing
    fn laid(t: &str) -> (Parsed, Places) {
        let v = u(t);
        let p = stacked::parse(&v, &[]);
        let (sizes, raise, looks) = stacked::looks(&p, v.len(), 24.0, stacked::FRACTION_SCALE);
        let adv: Vec<f32> = (0..v.len())
            .map(|k| if p.hidden.contains(&k) { 0.0 } else { 10.0 })
            .collect();
        let sz = Sizes {
            sizes,
            raise,
            looks,
            top: stacked::FRACTION_SCALE,
        };
        let pl = places(&p, &v, &sz, &adv);
        (p, pl)
    }

    #[test]
    fn a_denominator_goes_back_under_its_numerator() {
        let (_, pl) = laid("(12)/(3)");
        let bar = pl.bars[0];
        // the bar is as wide as the numerator and its overhang, and starts the line
        let pad = 21.6 * stacked::BAR_OVERHANG;
        assert!((bar.w - (20.0 + 2.0 * pad)).abs() < 1e-3);
        assert!(bar.x.abs() < 1e-3);
        assert!((bar.thick - 1.0).abs() < 1e-3);
        // the numerator starts at the overhang, the denominator centred under it, the line on past the bar
        assert!((pl.xs[1] - pad).abs() < 1e-3);
        assert!((pl.xs[6] - (bar.w - 10.0) / 2.0).abs() < 1e-3);
        assert!((pl.xs[8] - bar.w).abs() < 1e-3);
    }

    #[test]
    fn a_fraction_that_starts_the_line_is_centred_too() {
        let (_, pl) = laid("1/(12345)");
        let bar = pl.bars[0];
        assert!(bar.x.abs() < 1e-3);
        assert!(((pl.xs[0] + 5.0) - (bar.x + bar.w / 2.0)).abs() < 1e-3);
        assert!(((pl.xs[3] + 25.0) - (bar.x + bar.w / 2.0)).abs() < 1e-3);
    }

    #[test]
    fn signs_get_room_and_brackets_grow() {
        // 1+2: 4.8 each side of the + at 24, the caret beside it in the middle of that room
        let (_, pl) = laid("1+2");
        assert!((pl.xs[1] - 14.8).abs() < 1e-3 && (pl.xs[2] - 29.6).abs() < 1e-3);
        assert!((pl.shift[1] - 2.4).abs() < 1e-3 && (pl.shift[2] - 2.4).abs() < 1e-3);
        // brackets around a fraction: as tall as it, centred on the axis, with room inside
        let (p, pl) = laid("2(1/(2)+1)");
        assert_eq!(pl.talls.len(), 1);
        let b = pl.talls[0];
        let e = stacked::extent(&p, 2, 9, 24.0, 24.0, stacked::FRACTION_SCALE);
        // TeX's delimiters: 90.1% of what they hold, and taller than a digit
        assert!(b.axis + b.half >= 0.9 * e.0 && b.half - b.axis >= 0.85 * e.1);
        assert!(b.axis + b.half > 24.0 * tex::HEIGHT);
        assert!(b.room > 0.0 && pl.extra[1] >= b.room);
    }

    #[test]
    fn a_tall_line_moves_until_all_of_it_shows() {
        // a plain line's baseline stays; one too deep for the composer goes up, one too tall down
        assert_eq!(shift_into((17.8, 5.0), false, 44.0, 72.0), 0.0);
        assert!(shift_into((17.8, 40.0), false, 44.0, 72.0) < 0.0);
        assert!(shift_into((50.0, 2.0), false, 44.0, 72.0) > 0.0);
    }

    #[test]
    fn the_root_stroke_runs_from_the_tick_over_the_radicand() {
        let pts = root_points(10.0, 16.0, 60.0, 30.0, 24.0);
        assert_eq!(pts[4].0, 60.0);
        assert_eq!(pts[3].1, pts[4].1);
        // the foot is the lowest point, just under the baseline; the top clear of the text
        assert!(pts[2].1 > 30.0 && pts[2].1 < 31.0);
        assert!(pts[4].1 < 30.0 - 24.0 * 0.7);
        let tall = root_points_between(0.0, 10.0, 50.0, 2.0, 60.0);
        assert_eq!((tall[3].1, tall[2].1), (2.0, 60.0));
    }
}
