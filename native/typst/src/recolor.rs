//! The web preview's colour rule, on the laid-out frames instead of the SVG text.
//!
//! `typstDocument` sets `fill: black`, and `cleanSvg` in `src/lib/typstRender.ts` turns every
//! `fill`/`stroke="#000"|"#000000"|"black"` into `currentColor`, so opaque black follows the page's text colour
//! (light or dark) and every other colour stays as written. Here opaque black becomes the requested colour in
//! glyph fills, glyph strokes, shape fills and shape strokes, the same set the regex reaches. Layout is
//! untouched, so one compile serves both appearances.

use typst::layout::{Frame, FrameItem, GroupItem, Page};
use typst::visualize::{Color, FixedStroke, Paint};

/// Whether a paint is the opaque black `cleanSvg` swaps. typst.ts writes colours as hex in the SVG, and an
/// opaque black in any colour space prints as `#000000`.
pub(crate) fn is_foreground(paint: &Paint) -> bool {
    matches!(paint, Paint::Solid(color) if is_black(color))
}

fn is_black(color: &Color) -> bool {
    color.to_vec4_u8() == [0, 0, 0, 255]
}

/// The page with opaque black replaced by `color` (straight RGBA). Opaque black itself is a no-op.
pub(crate) fn recolor_page(page: &Page, color: [u8; 4]) -> Page {
    if color == [0, 0, 0, 255] {
        return page.clone();
    }
    let paint = Paint::Solid(Color::from_u8(color[0], color[1], color[2], color[3]));
    let mut page = page.clone();
    page.frame = recolor_frame(&page.frame, &paint);
    page
}

fn swap(paint: &mut Paint, to: &Paint) {
    if is_foreground(paint) {
        *paint = to.clone();
    }
}

fn swap_stroke(stroke: &mut Option<FixedStroke>, to: &Paint) {
    if let Some(stroke) = stroke {
        swap(&mut stroke.paint, to);
    }
}

fn recolor_frame(frame: &Frame, to: &Paint) -> Frame {
    let mut out = Frame::new(frame.size(), frame.kind());
    if frame.has_baseline() {
        out.set_baseline(frame.baseline());
    }
    for (pos, item) in frame.items() {
        let item = match item {
            FrameItem::Group(group) => FrameItem::Group(GroupItem {
                frame: recolor_frame(&group.frame, to),
                ..group.clone()
            }),
            FrameItem::Text(text) => {
                let mut text = text.clone();
                swap(&mut text.fill, to);
                swap_stroke(&mut text.stroke, to);
                FrameItem::Text(text)
            }
            FrameItem::Shape(shape, span) => {
                let mut shape = shape.clone();
                if let Some(fill) = &mut shape.fill {
                    swap(fill, to);
                }
                swap_stroke(&mut shape.stroke, to);
                FrameItem::Shape(shape, *span)
            }
            other => other.clone(),
        };
        out.push(*pos, item);
    }
    out
}
