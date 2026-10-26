//! The page as resolution-independent paths, for `CGPath`: every glyph outline and shape flattened into page
//! coordinates (points, y down, origin top left), in paint order.
//!
//! It walks the frames the way `typst-render` does (same transforms, glyph offsets, outline scaling and y flip),
//! so filling these paths with tiny-skia gives the pixmap `typst-render` makes, up to anti-aliasing (a test
//! checks that). Glyphs drawn in opaque black, the web preview's `currentColor`, come out as
//! [`Paint::Foreground`], so the UI fills them with the label colour and they follow light and dark mode without
//! another compile.

use std::sync::Arc;

use ttf_parser::{GlyphId, OutlineBuilder};
use typst::layout::{Abs, Frame, FrameItem, Page, Point, Transform};
use typst::text::TextItem;
use typst::text::color::{glyph_frame, should_outline};
use typst::visualize::{
    Curve, CurveItem, FillRule, FixedStroke, Geometry, LineCap, LineJoin, Paint as TypstPaint,
};

use crate::recolor::is_foreground;

/// A path verb; each consumes points from [`Path::points`] (x, y pairs): move 1, line 1, quad 2, cubic 3,
/// close 0. The numbering is part of the FFI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Verb {
    Move = 0,
    Line = 1,
    Quad = 2,
    Cubic = 3,
    Close = 4,
}

impl Verb {
    /// How many points the verb takes.
    pub fn points(self) -> usize {
        match self {
            Verb::Move | Verb::Line => 1,
            Verb::Quad => 2,
            Verb::Cubic => 3,
            Verb::Close => 0,
        }
    }
}

/// A path in page points: `verbs` with their points flattened as `[x0, y0, x1, y1, …]`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub verbs: Vec<Verb>,
    pub points: Vec<f32>,
}

impl Path {
    fn push(&mut self, verb: Verb, pts: &[(f64, f64)]) {
        self.verbs.push(verb);
        for &(x, y) in pts {
            self.points.push(x as f32);
            self.points.push(y as f32);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }
}

/// What a path is painted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paint {
    /// Opaque black in the document: the UI's text colour (the web preview's `currentColor`).
    Foreground,
    /// Any other solid colour, straight (not premultiplied) sRGB.
    Rgba([u8; 4]),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Style {
    Fill {
        even_odd: bool,
    },
    Stroke {
        width: f32,
        cap: Cap,
        join: Join,
        miter_limit: f32,
        /// Dash lengths in points, empty for a solid line.
        dash: Vec<f32>,
        dash_phase: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

/// One painted path. `clip` indexes [`Vector::clips`]: the innermost clip region it is drawn inside.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: Path,
    pub paint: Paint,
    pub style: Style,
    pub clip: Option<usize>,
}

/// A clip region (nonzero fill rule), intersected with its `parent`'s.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub path: Path,
    pub parent: Option<usize>,
}

/// A page as paths.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vector {
    pub width_pt: f32,
    pub height_pt: f32,
    /// The page's own background, when it has one (the preview's `fill: none` has none).
    pub background: Option<Paint>,
    pub items: Vec<Item>,
    pub clips: Vec<Clip>,
}

/// An affine map `x' = sx·x + kx·y + tx`, `y' = ky·x + sy·y + ty` (tiny-skia's `from_row` order).
#[derive(Clone, Copy, Debug)]
struct Affine {
    sx: f64,
    ky: f64,
    kx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
}

impl Affine {
    const IDENTITY: Self = Self {
        sx: 1.0,
        ky: 0.0,
        kx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn from_typst(t: &Transform) -> Self {
        Self {
            sx: t.sx.get(),
            ky: t.ky.get(),
            kx: t.kx.get(),
            sy: t.sy.get(),
            tx: t.tx.to_pt(),
            ty: t.ty.to_pt(),
        }
    }

    /// `self ∘ other`: apply `other` first.
    fn pre_concat(self, o: Self) -> Self {
        Self {
            sx: self.sx * o.sx + self.kx * o.ky,
            ky: self.ky * o.sx + self.sy * o.ky,
            kx: self.sx * o.kx + self.kx * o.sy,
            sy: self.ky * o.kx + self.sy * o.sy,
            tx: self.sx * o.tx + self.kx * o.ty + self.tx,
            ty: self.ky * o.tx + self.sy * o.ty + self.ty,
        }
    }

    fn pre_translate(self, x: f64, y: f64) -> Self {
        self.pre_concat(Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        })
    }

    fn pre_scale(self, x: f64, y: f64) -> Self {
        self.pre_concat(Self {
            sx: x,
            sy: y,
            ..Self::IDENTITY
        })
    }

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.sx * x + self.kx * y + self.tx,
            self.ky * x + self.sy * y + self.ty,
        )
    }

    /// How much the map scales lengths, for stroke widths (exact for the uniform scales and mirrors math
    /// uses; a geometric mean otherwise).
    fn length_scale(&self) -> f64 {
        (self.sx * self.sy - self.kx * self.ky).abs().sqrt()
    }
}

/// Collects a glyph outline in font units, mapped through a transform.
struct OutlineSink<'a> {
    ts: &'a Affine,
    path: Path,
}

impl OutlineBuilder for OutlineSink<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.ts.apply(x.into(), y.into());
        self.path.push(Verb::Move, &[p]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.ts.apply(x.into(), y.into());
        self.path.push(Verb::Line, &[p]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let a = self.ts.apply(x1.into(), y1.into());
        let p = self.ts.apply(x.into(), y.into());
        self.path.push(Verb::Quad, &[a, p]);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let a = self.ts.apply(x1.into(), y1.into());
        let b = self.ts.apply(x2.into(), y2.into());
        let p = self.ts.apply(x.into(), y.into());
        self.path.push(Verb::Cubic, &[a, b, p]);
    }
    fn close(&mut self) {
        self.path.push(Verb::Close, &[]);
    }
}

fn paint_of(paint: &TypstPaint) -> Paint {
    match paint {
        _ if is_foreground(paint) => Paint::Foreground,
        TypstPaint::Solid(color) => Paint::Rgba(color.to_vec4_u8()),
        // gradients and tilings never come out of `typstDocument`; their mean colour would be a guess, so they
        // draw as text
        _ => Paint::Foreground,
    }
}

fn stroke_style(stroke: &FixedStroke, scale: f64) -> Style {
    let (dash, dash_phase) = match &stroke.dash {
        Some(d) => {
            // tiny-skia (and so typst-render) repeats an odd-length pattern once to make it even
            let n = if d.array.len() % 2 == 1 {
                2 * d.array.len()
            } else {
                d.array.len()
            };
            (
                d.array
                    .iter()
                    .map(|l| (l.to_pt() * scale) as f32)
                    .cycle()
                    .take(n)
                    .collect(),
                (d.phase.to_pt() * scale) as f32,
            )
        }
        None => (Vec::new(), 0.0),
    };
    Style::Stroke {
        width: (stroke.thickness.to_pt() * scale) as f32,
        cap: match stroke.cap {
            LineCap::Butt => Cap::Butt,
            LineCap::Round => Cap::Round,
            LineCap::Square => Cap::Square,
        },
        join: match stroke.join {
            LineJoin::Miter => Join::Miter,
            LineJoin::Round => Join::Round,
            LineJoin::Bevel => Join::Bevel,
        },
        miter_limit: stroke.miter_limit.get() as f32,
        dash,
        dash_phase,
    }
}

fn curve_path(curve: &Curve, ts: &Affine) -> Path {
    let mut path = Path::default();
    let pt = |p: &Point| ts.apply(p.x.to_pt(), p.y.to_pt());
    for item in &curve.0 {
        match item {
            CurveItem::Move(p) => path.push(Verb::Move, &[pt(p)]),
            CurveItem::Line(p) => path.push(Verb::Line, &[pt(p)]),
            CurveItem::Cubic(a, b, p) => path.push(Verb::Cubic, &[pt(a), pt(b), pt(p)]),
            CurveItem::Close => path.push(Verb::Close, &[]),
        }
    }
    path
}

struct Walker {
    out: Vector,
}

impl Walker {
    fn frame(&mut self, frame: &Frame, ts: Affine, clip: Option<usize>) {
        for (pos, item) in frame.items() {
            let at = ts.pre_translate(pos.x.to_pt(), pos.y.to_pt());
            match item {
                FrameItem::Group(group) => {
                    let inner = at.pre_concat(Affine::from_typst(&group.transform));
                    let mut clip = clip;
                    if let Some(curve) = &group.clip {
                        let path = curve_path(curve, &inner);
                        self.out.clips.push(Clip { path, parent: clip });
                        clip = Some(self.out.clips.len() - 1);
                    }
                    self.frame(&group.frame, inner, clip);
                }
                FrameItem::Text(text) => self.text(text, at, clip),
                FrameItem::Shape(shape, _) => {
                    let path = match &shape.geometry {
                        Geometry::Line(to) => {
                            let mut p = Path::default();
                            p.push(Verb::Move, &[at.apply(0.0, 0.0)]);
                            p.push(Verb::Line, &[at.apply(to.x.to_pt(), to.y.to_pt())]);
                            p
                        }
                        Geometry::Rect(size) => {
                            let (w, h) = (size.x.to_pt(), size.y.to_pt());
                            let mut p = Path::default();
                            p.push(Verb::Move, &[at.apply(0.0, 0.0)]);
                            p.push(Verb::Line, &[at.apply(w, 0.0)]);
                            p.push(Verb::Line, &[at.apply(w, h)]);
                            p.push(Verb::Line, &[at.apply(0.0, h)]);
                            p.push(Verb::Close, &[]);
                            p
                        }
                        Geometry::Curve(curve) => curve_path(curve, &at),
                    };
                    if path.is_empty() {
                        continue;
                    }
                    if let Some(fill) = &shape.fill {
                        self.out.items.push(Item {
                            path: path.clone(),
                            paint: paint_of(fill),
                            style: Style::Fill {
                                even_odd: shape.fill_rule == FillRule::EvenOdd,
                            },
                            clip,
                        });
                    }
                    if let Some(stroke) = &shape.stroke
                        && stroke.thickness > Abs::zero()
                    {
                        self.out.items.push(Item {
                            path,
                            paint: paint_of(&stroke.paint),
                            style: stroke_style(stroke, at.length_scale()),
                            clip,
                        });
                    }
                }
                // the preview has no images or links; tags are introspection only
                FrameItem::Image(..) | FrameItem::Link(..) | FrameItem::Tag(_) => {}
            }
        }
    }

    // as `typst_render::text::render_text`
    fn text(&mut self, text: &TextItem, ts: Affine, clip: Option<usize>) {
        let upem = text.font.units_per_em();
        let scale = text.size.to_pt() / upem;
        let mut x = Abs::zero();
        let mut y = Abs::zero();
        for glyph in &text.glyphs {
            let x_offset = x + glyph.x_offset.at(text.size);
            let y_offset = y + glyph.y_offset.at(text.size);
            if should_outline(&text.font, glyph) {
                // font units are y-up
                let gts = ts
                    .pre_translate(x_offset.to_pt(), -y_offset.to_pt())
                    .pre_scale(scale, -scale);
                let mut sink = OutlineSink {
                    ts: &gts,
                    path: Path::default(),
                };
                if text
                    .font
                    .ttf()
                    .outline_glyph(GlyphId(glyph.id), &mut sink)
                    .is_some()
                    && !sink.path.is_empty()
                {
                    let path = sink.path;
                    if let Some(stroke) = &text.stroke
                        && stroke.thickness > Abs::zero()
                    {
                        self.out.items.push(Item {
                            path: path.clone(),
                            paint: paint_of(&text.fill),
                            style: Style::Fill { even_odd: false },
                            clip,
                        });
                        self.out.items.push(Item {
                            path,
                            paint: paint_of(&stroke.paint),
                            style: stroke_style(stroke, ts.length_scale()),
                            clip,
                        });
                    } else {
                        self.out.items.push(Item {
                            path,
                            paint: paint_of(&text.fill),
                            style: Style::Fill { even_odd: false },
                            clip,
                        });
                    }
                }
            } else {
                // colour, bitmap and SVG glyphs, and tofu: typst gives them as a frame in font units
                let gts = ts
                    .pre_translate(x_offset.to_pt(), -y_offset.to_pt() - text.size.to_pt())
                    .pre_scale(scale, scale);
                let (frame, _) = glyph_frame(&text.font, glyph.id);
                self.frame(&frame, gts, clip);
            }
            x += glyph.x_advance.at(text.size);
            y += glyph.y_advance.at(text.size);
        }
    }
}

/// The page as paths. `typst-render` paints a page with no fill white, and SVG/PDF leave it clear; the
/// preview's `fill: none` is clear either way, so `background` is only set for an explicit fill.
pub(crate) fn page_vector(page: &Page) -> Vector {
    let size = page.frame.size();
    let mut walker = Walker {
        out: Vector {
            width_pt: size.x.to_pt() as f32,
            height_pt: size.y.to_pt() as f32,
            background: page.fill_or_transparent().map(|p| paint_of(&p)),
            ..Vector::default()
        },
    };
    walker.frame(&page.frame, Affine::IDENTITY, None);
    walker.out
}

/// Shared, immutable result.
pub type SharedVector = Arc<Vector>;
