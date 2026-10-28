//! The typeset maths row under the composer (`TypstPreview.tsx`, MathText.swift, BarModel.swift
//! `refreshTypeset`): shown exactly when the web app shows it (`typstShown`): the setting on, no panel of its own
//! (the cheat sheet; other packages add theirs to `other_panel`), and Typst would show something the bar doesn't
//! (`typstPreviewUseful` of the line as it chains). The answer beside it is the one the slot shows (`typstAnswer`),
//! a solve's letter after ⇒, nothing for a message.
//!
//! Work: the request is decided on the UI thread (`refresh`, cheap, compared with the last one); the Typst thread
//! (`EngineThread::typst`, warmed at launch) writes the document with the core's `typst_document` and lays it out
//! as vector paths; a burst of keystrokes compiles only the newest. The paths come back to the model, and the
//! paint turns them into Direct2D path geometries the first time they are drawn (cached per document, one at a
//! time), filled or stroked in the text colour, so light and dark need no recompile. Mid-edit input Typst rejects
//! keeps the last picture at 40%. The hidden trim drops the geometries and Typst's caches.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use qcalc_core::app::typst_math;
use qcalc_ffi::{
    SolveOutcome, TypstLineCap, TypstLineJoin, TypstPaint, TypstPaths, TypstRenderer, TypstStyle,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
    D2D1_FILL_MODE_ALTERNATE, D2D1_FILL_MODE_WINDING,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
    D2D1_CAP_STYLE, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND, D2D1_CAP_STYLE_SQUARE,
    D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE, D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_SOLID,
    D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_LINE_JOIN_BEVEL,
    D2D1_LINE_JOIN_MITER_OR_BEVEL, D2D1_LINE_JOIN_ROUND, D2D1_QUADRATIC_BEZIER_SEGMENT,
    D2D1_STROKE_STYLE_PROPERTIES, ID2D1Bitmap, ID2D1Geometry, ID2D1PathGeometry, ID2D1RenderTarget,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::core::Interface;
use windows_numerics::{Matrix3x2, Vector2};

use crate::engine::{EngineThread, done};
use crate::model::BarModel;
use crate::render::{Canvas, Color, Gfx, Rect, rgb, size, with_alpha};

/// The row's padding under the picture, and the picture's height limit (`max-height: 72px`).
pub const PAD_BOTTOM: f32 = 8.0;
pub const MAX_H: f32 = 72.0;

/// What the row typesets (`TypesetRequest`), decided from what the bar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// the line, chained through `ans` as it evaluates (`chainedExpr`)
    pub expr: String,
    /// the exact form beside the shown answer, when the slot shows the plain one
    pub exact: Option<String>,
    pub display: String,
    /// a solve's letter (`x = ±2`); empty otherwise
    pub solved_for: String,
    /// the slot says something that isn't an answer (no real solution), so nothing follows `=`
    pub message: bool,
}

/// One typeset line: the paths, the document they came from, and whether they belong to an earlier text.
#[derive(Clone, Debug)]
pub struct Typeset {
    pub paths: Arc<TypstPaths>,
    pub doc: String,
    pub stale: bool,
}

/// The row's state on the model (`BarModel.typeset`).
#[derive(Debug, Default)]
pub struct TypesetState {
    asked: Option<Request>,
    pub shown: Option<Typeset>,
    latest: Arc<AtomicU64>,
}

enum Outcome {
    Hidden,
    Drawn(TypstPaths, String),
    Failed(String),
}

/// A panel of another package that takes the row's place (graph, sys, games…): they add their test here.
fn other_panel(m: &BarModel) -> bool {
    m.tape.help
}

/// The request for the bar as it is, or none when the row hides.
pub fn request(m: &BarModel) -> Option<Request> {
    let text = m.field.text();
    if !m.settings.typst_preview || text.trim().is_empty() || other_panel(m) {
        return None;
    }
    let live = m.live.as_ref().filter(|_| m.live_for == text);
    let solve = live.and_then(|a| a.solve.as_ref());
    let solved_for = solve
        .filter(|s| s.outcome == SolveOutcome::Roots)
        .map(|s| s.variable.clone())
        .unwrap_or_default();
    let st = &m.answer;
    let tabbed = st.form_index > 0 && st.forms.get(st.form_index).is_some();
    Some(Request {
        expr: if m.chained {
            qcalc_core::app::chain::chained_expr(text)
        } else {
            text.to_string()
        },
        exact: if tabbed {
            None
        } else {
            crate::answer::exact(m).map(str::to_string)
        },
        display: if tabbed {
            st.forms[st.form_index].clone()
        } else {
            // Ctrl+E's mode: in exponent form, as the slot (exponent.rs)
            live.map(|a| crate::exponent::shown(m, &a.display))
                .unwrap_or_default()
        },
        message: solve.is_some() && solved_for.is_empty(),
        solved_for,
    })
}

fn typeset(r: &TypstRenderer, req: &Request) -> Outcome {
    if !typst_math::typst_preview_useful(&req.expr) {
        return Outcome::Hidden;
    }
    let answer = if req.message {
        String::new()
    } else {
        typst_math::typst_answer(req.exact.as_deref(), &req.display)
    };
    let Some(doc) = typst_math::typst_document(&req.expr, &answer, &req.solved_for) else {
        return Outcome::Hidden;
    };
    match r.paths(doc.clone()) {
        Ok(p) => Outcome::Drawn(p, doc),
        Err(_) => Outcome::Failed(doc),
    }
}

/// What the bar shows changed (an answer landed, Tab, a closed form): typeset it again if the request differs.
/// The compile waits a moment (`SETTLE_MS`) first: a burst of keystrokes asks once, after it, so typing never
/// shares the machine with compiles it is already past.
pub fn refresh(m: &mut BarModel, engine: &EngineThread) {
    let _ = engine;
    let Some(req) = request(m) else {
        crate::answer::timer::stop(crate::answer::timer::TYPESET);
        let asked = m.typeset.asked.take().is_some();
        if m.typeset.shown.take().is_some() || asked {
            m.touch();
        }
        return;
    };
    if m.typeset.asked.as_ref() == Some(&req) {
        return;
    }
    m.typeset.asked = Some(req);
    crate::answer::timer::start(crate::answer::timer::TYPESET, SETTLE_MS);
}

/// How long the line rests before it is typeset (about a frame).
pub const SETTLE_MS: u32 = 20;

/// The line rested (answer.rs' timer): the Typst thread typesets what was asked; the picture is drawn when it
/// comes back, if it is still the one asked for.
pub fn compile(m: &mut BarModel, engine: &EngineThread) {
    let Some(req) = m.typeset.asked.clone() else {
        return;
    };
    let latest = m.typeset.latest.clone();
    let generation = latest.fetch_add(1, Ordering::Relaxed) + 1;
    engine.typst(move |r| {
        // a newer request came before this one's turn: only the newest compiles
        if latest.load(Ordering::Relaxed) != generation {
            return None;
        }
        let outcome = typeset(r, &req);
        done(move |app| {
            let st = &mut app.model.typeset;
            if st.asked.as_ref() != Some(&req) {
                return;
            }
            match outcome {
                Outcome::Hidden => st.shown = None,
                Outcome::Drawn(paths, doc) => {
                    st.shown = Some(Typeset {
                        paths: Arc::new(paths),
                        doc,
                        stale: false,
                    });
                }
                // mid-edit input Typst rejects keeps the last picture, dimmed
                Outcome::Failed(doc) => {
                    if let Some(s) = &mut st.shown
                        && s.doc != doc
                    {
                        s.stale = true;
                    }
                }
            }
            app.model.touch();
        })
    });
}
/// The row's height in a bar `width` wide (0 when hidden).
pub fn height(m: &BarModel, width: f32) -> f32 {
    m.typeset.shown.as_ref().map_or(0.0, |t| {
        fit(&t.paths, width).1 * t.paths.height_pt + PAD_BOTTOM
    })
}

/// (scale, the same) the page shrunk to the width offered and 72 high, never grown.
fn fit(p: &TypstPaths, width: f32) -> (f32, f32) {
    let avail = width - 2.0 * size::GUTTER;
    let s = 1f32
        .min(avail / p.width_pt.max(1.0))
        .min(MAX_H / p.height_pt.max(1.0))
        .max(0.01);
    (s, s)
}

// ---- drawing ----

enum Style {
    Fill,
    Stroke(f32, Option<ID2D1StrokeStyle>),
}

struct Drawn {
    geometry: ID2D1PathGeometry,
    paint: TypstPaint,
    style: Style,
    clips: Vec<usize>,
}

struct Built {
    items: Vec<Drawn>,
    clips: Vec<ID2D1PathGeometry>,
}

/// The hidden trim: the picture goes, and Typst's thread with all its caches (it starts again when needed).
pub fn trim(engine: &EngineThread) {
    PICTURE.with(|p| p.borrow_mut().take());
    engine.stop_typst();
}

fn v(points: &[f32], i: usize) -> Vector2 {
    Vector2 {
        X: points.get(i).copied().unwrap_or(0.0),
        Y: points.get(i + 1).copied().unwrap_or(0.0),
    }
}

/// `MathPreview.path`: verbs 0 move, 1 line, 2 quad, 3 cubic, 4 close.
fn geometry(
    gfx: &Gfx,
    verbs: &[u8],
    points: &[f32],
    fill: bool,
    even_odd: bool,
) -> Option<ID2D1PathGeometry> {
    // SAFETY: a new geometry from our factory, its sink opened, fed and closed here
    unsafe {
        let g = gfx.d2d.CreatePathGeometry().ok()?;
        let sink = g.Open().ok()?;
        sink.SetFillMode(if even_odd {
            D2D1_FILL_MODE_ALTERNATE
        } else {
            D2D1_FILL_MODE_WINDING
        });
        let begin = if fill {
            D2D1_FIGURE_BEGIN_FILLED
        } else {
            D2D1_FIGURE_BEGIN_HOLLOW
        };
        let mut i = 0;
        let mut open = false;
        let mut start = Vector2 { X: 0.0, Y: 0.0 };
        for &verb in verbs {
            let n = match verb {
                0 | 1 => 1,
                2 => 2,
                3 => 3,
                _ => 0,
            };
            if verb != 0 && n > 0 && !open {
                sink.BeginFigure(start, begin);
                open = true;
            }
            match verb {
                0 => {
                    if open {
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    }
                    start = v(points, i);
                    sink.BeginFigure(start, begin);
                    open = true;
                }
                1 => sink.AddLine(v(points, i)),
                2 => sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: v(points, i),
                    point2: v(points, i + 2),
                }),
                3 => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: v(points, i),
                    point2: v(points, i + 2),
                    point3: v(points, i + 4),
                }),
                _ => {
                    if open {
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        open = false;
                    }
                }
            }
            i += 2 * n;
        }
        if open {
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close().ok()?;
        Some(g)
    }
}

fn stroke_style(
    gfx: &Gfx,
    cap: TypstLineCap,
    join: TypstLineJoin,
    miter: f32,
    dash: &[f32],
    phase: f32,
    width: f32,
) -> Option<ID2D1StrokeStyle> {
    let cap: D2D1_CAP_STYLE = match cap {
        TypstLineCap::Butt => D2D1_CAP_STYLE_FLAT,
        TypstLineCap::Round => D2D1_CAP_STYLE_ROUND,
        TypstLineCap::Square => D2D1_CAP_STYLE_SQUARE,
    };
    let props = D2D1_STROKE_STYLE_PROPERTIES {
        startCap: cap,
        endCap: cap,
        dashCap: cap,
        lineJoin: match join {
            TypstLineJoin::Miter => D2D1_LINE_JOIN_MITER_OR_BEVEL,
            TypstLineJoin::Round => D2D1_LINE_JOIN_ROUND,
            TypstLineJoin::Bevel => D2D1_LINE_JOIN_BEVEL,
        },
        miterLimit: miter,
        dashStyle: if dash.is_empty() {
            D2D1_DASH_STYLE_SOLID
        } else {
            D2D1_DASH_STYLE_CUSTOM
        },
        // D2D's dashes are in stroke widths, Typst's in points
        dashOffset: phase / width.max(1e-3),
    };
    let dashes: Vec<f32> = dash.iter().map(|d| d / width.max(1e-3)).collect();
    // SAFETY: the properties and dashes outlive the call
    unsafe {
        gfx.d2d
            .CreateStrokeStyle(&props, (!dashes.is_empty()).then_some(&dashes[..]))
            .ok()
    }
}

fn build(gfx: &Gfx, t: &Typeset) -> Built {
    let p = &t.paths;
    let clips = p
        .clips
        .iter()
        .filter_map(|c| geometry(gfx, &c.verbs, &c.points, true, false))
        .collect::<Vec<_>>();
    let items = p
        .paths
        .iter()
        .filter_map(|item| {
            let (fill, even_odd) = match &item.style {
                TypstStyle::Fill { even_odd } => (true, *even_odd),
                TypstStyle::Stroke { .. } => (false, false),
            };
            let geometry = geometry(gfx, &item.verbs, &item.points, fill, even_odd)?;
            let style = match &item.style {
                TypstStyle::Fill { .. } => Style::Fill,
                TypstStyle::Stroke {
                    width,
                    cap,
                    join,
                    miter_limit,
                    dash,
                    dash_phase,
                } => Style::Stroke(
                    *width,
                    stroke_style(gfx, *cap, *join, *miter_limit, dash, *dash_phase, *width),
                ),
            };
            // the clip chain, outermost first
            let mut chain = Vec::new();
            let mut at = item.clip.map(|c| c as usize);
            while let Some(c) = at {
                if c >= p.clips.len() || chain.contains(&c) {
                    break;
                }
                chain.push(c);
                at = p.clips[c].parent.map(|c| c as usize);
            }
            chain.reverse();
            Some(Drawn {
                geometry,
                paint: item.paint,
                style,
                clips: chain,
            })
        })
        .collect();
    Built { items, clips }
}

fn color(paint: TypstPaint, ink: Color, alpha: f32) -> Color {
    match paint {
        TypstPaint::Foreground => with_alpha(ink, alpha),
        TypstPaint::Color { color: c } => {
            let base = rgb(
                (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b),
                f32::from(c.a) / 255.0,
            );
            with_alpha(base, alpha)
        }
    }
}

/// The picture on screen, drawn once into a bitmap of the bar's render target: each frame after that only copies
/// it. Made again when the document, the ink, the scale or the render target change.
struct Picture {
    key: (String, [u32; 4], usize, u32),
    bitmap: ID2D1Bitmap,
    size: (f32, f32),
}

thread_local! {
    static PICTURE: RefCell<Option<Picture>> = const { RefCell::new(None) };
}

fn draw_items(rt: &ID2D1RenderTarget, brush: &ID2D1SolidColorBrush, built: &Built, ink: Color) {
    // SAFETY: inside the target's BeginDraw/EndDraw; layers are pushed and popped here
    unsafe {
        for item in &built.items {
            let mut pushed = 0;
            for &k in &item.clips {
                let Some(g) = built.clips.get(k) else {
                    continue;
                };
                let mask: ID2D1Geometry = g.clone().into();
                let params = D2D1_LAYER_PARAMETERS {
                    contentBounds: D2D_RECT_F {
                        left: f32::MIN,
                        top: f32::MIN,
                        right: f32::MAX,
                        bottom: f32::MAX,
                    },
                    geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: 1.0,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                };
                rt.PushLayer(&params, None);
                let _ = std::mem::ManuallyDrop::into_inner(params.geometricMask);
                pushed += 1;
            }
            brush.SetColor(&color(item.paint, ink, 1.0));
            match &item.style {
                Style::Fill => rt.FillGeometry(&item.geometry, brush, None),
                Style::Stroke(w, style) => {
                    rt.DrawGeometry(&item.geometry, brush, *w, style.as_ref())
                }
            }
            for _ in 0..pushed {
                rt.PopLayer();
            }
        }
    }
}

/// The page drawn at `s` into a bitmap the bar's target can copy.
fn picture(c: &Canvas, t: &Typeset, s: f32) -> Option<(ID2D1Bitmap, (f32, f32))> {
    let built = build(c.gfx, t);
    let size = D2D_SIZE_F {
        width: (t.paths.width_pt * s).ceil().max(1.0),
        height: (t.paths.height_pt * s).ceil().max(1.0),
    };
    // SAFETY: a target made from the bar's (which is mid-frame), drawn and closed here
    unsafe {
        let brt = c
            .rt()
            .CreateCompatibleRenderTarget(
                Some(&size),
                None,
                None,
                D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
            )
            .ok()?;
        let brush = brt.CreateSolidColorBrush(&c.theme.ink, None).ok()?;
        brt.BeginDraw();
        brt.Clear(Some(&rgb(0, 0.0)));
        brt.SetTransform(&Matrix3x2 {
            M11: s,
            M12: 0.0,
            M21: 0.0,
            M22: s,
            M31: 0.0,
            M32: 0.0,
        });
        draw_items(&brt, &brush, &built, c.theme.ink);
        brt.EndDraw(None, None).ok()?;
        Some((brt.GetBitmap().ok()?, (size.width, size.height)))
    }
}

/// The row in `r`: the page centred at the top, shrunk to fit, in `--ink` (40% while stale).
pub fn paint(c: &Canvas, m: &BarModel, r: Rect) {
    let Some(t) = &m.typeset.shown else { return };
    if r.h <= 0.0 {
        return;
    }
    let (s, _) = fit(&t.paths, c.width);
    let ink = c.theme.ink;
    let key = (
        t.doc.clone(),
        [
            ink.r.to_bits(),
            ink.g.to_bits(),
            ink.b.to_bits(),
            ink.a.to_bits(),
        ],
        c.rt().as_raw() as usize,
        s.to_bits(),
    );
    PICTURE.with(|p| {
        let mut p = p.borrow_mut();
        if p.as_ref().is_none_or(|p| p.key != key) {
            *p = picture(c, t, s).map(|(bitmap, size)| Picture { key, bitmap, size });
        }
        let Some(pic) = p.as_ref() else { return };
        let x = (r.x + (r.w - pic.size.0) / 2.0).round();
        let dest = D2D_RECT_F {
            left: x,
            top: r.y,
            right: x + pic.size.0,
            bottom: r.y + pic.size.1,
        };
        // SAFETY: inside the frame's BeginDraw/EndDraw
        unsafe {
            c.rt().DrawBitmap(
                &pic.bitmap,
                Some(&dest),
                if t.stale { 0.4 } else { 1.0 },
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None,
            );
        }
    });
}
