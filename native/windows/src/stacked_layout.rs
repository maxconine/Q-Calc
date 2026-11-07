//! How the input line draws its maths (StackedLayout.swift, RadicalText.swift `RootSign`): stacked fractions and
//! powers (stacked.rs says what's stacked), and a typed root's sign and bar as one stroke.
//!
//! A stack is drawn from the line's one DirectWrite layout. Each slot's characters are made small in it and the
//! hidden ones (`/ ^ ( ) ·`) all but vanish, so the layout keeps every character's advance; but DirectWrite never
//! lets character spacing take an advance below nothing (the Mac's TextKit has the same rule, its `pull`), so the
//! layout can't put a denominator back under its numerator. The line's places are worked out here instead
//! (`Geom::xs`: the layout's own advances, plus each stack's room and pull-back), and the layout is drawn in runs,
//! each moved sideways and up or down to where it goes and clipped to its own glyphs. Everything the field draws by
//! position (selection, caret, squiggle, offered unit) asks `xs`, so a line with stacks draws as one without.

use crate::render::{Canvas, Color, Rect, caret_x, size, with_alpha};
use crate::stacked::{self, Look, Parsed};
use std::cell::{OnceCell, RefCell};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN};
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

/// A fraction's bar: from `x`, `w` wide, `axis` above the line's baseline; its fraction takes `start..end`.
#[derive(Clone, Copy, Debug)]
struct Bar {
    x: f32,
    w: f32,
    axis: f32,
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

/// Where a stacked line's places and pieces go, from the layout's left and top.
pub struct Geom {
    /// the caret place before each UTF-16 position, 0 to the line's length
    pub xs: Vec<f32>,
    pub width: f32,
    /// the line's baseline from the layout's top
    pub base: f32,
    /// each character's size and raise
    sizes: Vec<f32>,
    raise: Vec<f32>,
    looks: Vec<Look>,
    runs: Vec<Run>,
    bars: Vec<Bar>,
    holes: Vec<Hole>,
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

/// The slots' sizes and the hidden characters' sliver set on the line's layout (before the limits' own and the
/// gaps): what `build` measures. Returns each character's size and raise, and each slot's look.
pub fn apply_sizes(l: &IDWriteTextLayout, p: &Parsed, n: usize) -> (Vec<f32>, Vec<f32>, Vec<Look>) {
    let (sizes, raise, looks) = stacked::looks(p, n, size::DISPLAY);
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
    (sizes, raise, looks)
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

/// The stacked line's places and pieces, from its layout once the sizes (`apply_sizes`), the limits' and the gaps'
/// spacing are on it.
pub fn build(
    c: &Canvas,
    l: &IDWriteTextLayout,
    text: &str,
    p: &Parsed,
    (sizes, raise, looks): (Vec<f32>, Vec<f32>, Vec<Look>),
    h: f32,
) -> Geom {
    let n = sizes.len();
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
    let (xs, bars, holes, extra) = places(p, &sizes, &raise, &adv);
    let width = xs[n];
    let runs = runs(p, &raise, &extra, &from);
    PLACES.with(|s| *s.borrow_mut() = (text.to_string(), xs.clone()));
    Geom {
        xs,
        width,
        base,
        sizes,
        raise,
        looks,
        runs,
        bars,
        holes,
        parsed: p.clone(),
    }
}

/// The places (StackTypesetter's pen): each character's advance `adv`, plus the room each stack makes and the
/// pull back to its denominator, inner stacks first so an outer slot's width takes in the stacks inside it.
/// Returns the caret places, the bars, the holes and each character's added advance.
fn places(
    p: &Parsed,
    sizes: &[f32],
    raise: &[f32],
    adv: &[f32],
) -> (Vec<f32>, Vec<Bar>, Vec<Hole>, Vec<f32>) {
    let n = adv.len();
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
        let f = stacked::small(sizes[s.op]);
        let hole = f * 0.62;
        let (parent, base) = (sizes[s.op], raise[s.op]);
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
            let pad = f * 0.18;
            let w = nw.max(dw) + 2.0 * pad;
            let room = (w - nw) / 2.0;
            let axis = base + parent * 0.25;
            if let Some(o) = num.open {
                extra[o] += room + if num.is_empty() { hole } else { 0.0 };
                if num.is_empty() {
                    let raise = axis + 1.5 + f * 0.08;
                    holes.push((
                        o,
                        Hole {
                            open: o,
                            x: room,
                            raise,
                            size: f,
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
                let raise = axis - 1.5 - f * 0.72;
                holes.push((
                    o,
                    Hole {
                        open: o,
                        x: 0.0,
                        raise,
                        size: f,
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
                    axis,
                    start: s.start,
                    end: s.end,
                },
            ));
        } else {
            if let Some(o) = s.slot.open
                && s.slot.is_empty()
            {
                extra[o] += hole;
                let raise = base + parent * 0.36;
                holes.push((
                    o,
                    Hole {
                        open: o,
                        x: 0.0,
                        raise,
                        size: f,
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
    (xs, bars, holes, extra)
}

/// The runs to draw: unbroken stretches of shown characters at one height that the line keeps together as the
/// layout has them.
fn runs(p: &Parsed, raise: &[f32], extra: &[f32], from: &[f32]) -> Vec<Run> {
    let n = raise.len();
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

    /// Draws the line's text in its runs, `x0`, `y` the layout's top-left; the layout's drawing effects colour it.
    pub fn draw(&self, c: &Canvas, l: &IDWriteTextLayout, x0: f32, y: f32, h: f32, color: Color) {
        for r in &self.runs {
            let at = x0 + self.xs[r.a];
            c.push_clip(Rect::new(
                at - r.pad_l,
                y - 14.0,
                r.w + r.pad_l + r.pad_r,
                h + 28.0,
            ));
            c.text(l, at - r.from, y - r.raise, color);
            c.pop_clip();
        }
    }

    /// The bars and the empty slots' boxes.
    pub fn decorate(&self, c: &Canvas, x0: f32, y: f32) {
        let t = c.theme;
        let base = y + self.base;
        for b in &self.bars {
            c.fill(Rect::new(x0 + b.x, base - b.axis - 0.6, b.w, 1.2), t.ink);
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

    /// The caret at UTF-16 `caret` when it's in a slot: (x, top, height), the slot's size, from the layout's
    /// top-left; None outside the slots (the line's own caret).
    pub fn caret(&self, caret: usize) -> Option<(f32, f32, f32)> {
        let p = &self.parsed;
        let look = stacked::look_at(p, &self.looks, caret)?;
        let slot = p.slot(caret)?;
        let mut x = self.x(caret as u32);
        // in an empty slot, the middle of its box
        if slot.is_empty()
            && let Some(o) = slot.open
            && let Some(h) = self.holes.iter().find(|h| h.open == o)
        {
            x = h.x + h.size * 0.31;
        }
        Some((
            x,
            self.base - look.offset - look.size * 0.92,
            look.size * 1.15,
        ))
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
}

// ---- a typed root: sign and bar, one stroke (RootSign)

/// The root's one line: a small tick up to the hook, down to the foot, up to the top, and on over what it takes
/// to `end`. `left` and `width` are the sign's own room; `size` the sign's size; y grows down.
pub fn root_points(left: f32, width: f32, end: f32, baseline: f32, size: f32) -> [(f32, f32); 5] {
    // Segoe UI's cap height; the top is clear of what's under it, as Desmos leaves it
    let top = baseline - size * 0.7 - size * 0.2;
    let foot = baseline + size * 0.02;
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
    let stroke = (size / 17.0).max(1.0);
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
            let top = baseline - size * 0.7 - size * 0.24;
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

    #[test]
    fn a_denominator_goes_back_under_its_numerator() {
        // (12)/(3): every shown character 10 wide at its size, the hidden ones nothing
        let t = u("(12)/(3)");
        let p = stacked::parse(&t, &[]);
        let (sizes, raise, _) = stacked::looks(&p, t.len(), 24.0);
        let adv: Vec<f32> = (0..t.len())
            .map(|k| if p.hidden.contains(&k) { 0.0 } else { 10.0 })
            .collect();
        let (xs, bars, _, _) = places(&p, &sizes, &raise, &adv);
        let bar = bars[0];
        // the bar is as wide as the numerator and its padding, and starts the line
        let pad = stacked::small(24.0) * 0.18;
        assert!((bar.w - (20.0 + 2.0 * pad)).abs() < 1e-3);
        assert!(bar.x.abs() < 1e-3);
        // the numerator starts at the padding, the denominator centred under it
        assert!((xs[1] - pad).abs() < 1e-3);
        assert!((xs[6] - (bar.w - 10.0) / 2.0).abs() < 1e-3);
        // and the line goes on past the bar's end
        assert!((xs[8] - bar.w).abs() < 1e-3);
    }

    #[test]
    fn a_fraction_that_starts_the_line_is_centred_too() {
        // 1/12345: a bare numerator at the line's start, the line itself starting further on
        let t = u("1/(12345)");
        let p = stacked::parse(&t, &[]);
        let (sizes, raise, _) = stacked::looks(&p, t.len(), 24.0);
        let adv: Vec<f32> = (0..t.len())
            .map(|k| if p.hidden.contains(&k) { 0.0 } else { 10.0 })
            .collect();
        let (xs, bars, _, _) = places(&p, &sizes, &raise, &adv);
        let bar = bars[0];
        assert!(bar.x.abs() < 1e-3);
        // the numerator's middle over the bar's middle
        assert!(((xs[0] + 5.0) - (bar.x + bar.w / 2.0)).abs() < 1e-3);
        assert!(((xs[3] + 25.0) - (bar.x + bar.w / 2.0)).abs() < 1e-3);
    }

    #[test]
    fn the_root_stroke_runs_from_the_tick_over_the_radicand() {
        let pts = root_points(10.0, 16.0, 60.0, 30.0, 24.0);
        assert_eq!(pts[4].0, 60.0);
        assert_eq!(pts[3].1, pts[4].1);
        // the foot is the lowest point, just under the baseline; the top clear of the text
        assert!(pts[2].1 > 30.0 && pts[2].1 < 31.0);
        assert!(pts[4].1 < 30.0 - 24.0 * 0.7);
    }
}
