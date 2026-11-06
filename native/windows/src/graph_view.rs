//! The graph panel drawn, and the pointer over it (graph.rs has the state; GraphPanel.swift is the spec).
//!
//! - The panel: a head (the label, `x ∈ [lo, hi]`, the angle unit; a surface's `3D | contour`, + for another
//!   equation, the wrench for the settings, reset), the equations when there are several (a colour bar, the
//!   equation, an added one's field and ×), the settings block, the plot, then the key points as chips (a colour
//!   square each when there are several equations) and the roots no chip lists. All in mono type, the plot as a
//!   technical drawing: a square frame, hairline grids, tick marks out from the frame, square marks.
//! - Two passes: the plot's base (grid, axes, curves, level curves, surfaces, marks) is drawn into a bitmap once
//!   per change of graph, view, settings, theme or scale (`GraphState::generation`), and each frame draws that
//!   bitmap and the pointer's pass over it (crosshair, readout, picked mark), so moving the pointer never draws
//!   the plot again. The 3D base pass keeps its scene for the pointer pass to hit-test.
//! - The pointer: a drag pans (in 3D it orbits; shift, alt or the middle button pans; the right button orbits
//!   too and opens no menu), the wheel zooms (sideways pans in 2D), a double-click resets, a click on a picked mark
//!   or a chip copies it. Readouts the engine has to work out (a curve's y, a surface's z) go on
//!   `Lane::GraphReadout`; the 3D view reads the grid's height at once and the engine's exact z after.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use qcalc_ffi::{AngleUnit, GraphOptions, merged_options};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
    D2D1_CAP_STYLE_ROUND, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE, D2D1_DASH_STYLE_SOLID,
    D2D1_ELLIPSE, D2D1_LINE_JOIN_ROUND, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, ID2D1Bitmap, ID2D1PathGeometry, ID2D1RenderTarget,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, IDWriteTextFormat,
    IDWriteTextLayout,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_MENU,
    VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{IDC_ARROW, IDC_HAND, IDC_IBEAM, IDC_SIZEALL, IDC_SIZEWE};
use windows::core::{HSTRING, PCWSTR};
use windows_numerics::{Matrix3x2, Vector2};

use crate::app::App;
use crate::engine::{Lane, done};
use crate::graph::*;
use crate::graph3d::{self, Cube, Frame, Line, P2, P3, Piece, Plane, Scene};
use crate::graph_settings::{self as gs, Check, Ctl, FieldId, GraphPrefs, Seg};
use crate::model::{BarModel, now_ms};
use crate::render::{Canvas, Color, Gfx, Rect, Theme, caret_x, measure, size, with_alpha};

// ---- type ----

thread_local! {
    static FONTS: RefCell<HashMap<(bool, u32, i32), IDWriteTextFormat>> = RefCell::new(HashMap::new());
    static MONO: RefCell<Option<String>> = const { RefCell::new(None) };
    static ROUND: RefCell<Option<ID2D1StrokeStyle>> = const { RefCell::new(None) };
    /// the bench pipe's pointer stays put while the real one is elsewhere (its WM_MOUSELEAVE is ignored)
    static BENCH_POINTER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The plot's monospaced family: Cascadia Mono where Windows has it, else Consolas.
fn mono_family(gfx: &Gfx) -> String {
    MONO.with(|m| {
        if let Some(f) = &*m.borrow() {
            return f.clone();
        }
        let mut found = "Consolas".to_string();
        // SAFETY: out pointers are live locals
        unsafe {
            let mut fonts = None;
            if gfx.dw.GetSystemFontCollection(&mut fonts, false).is_ok()
                && let Some(fonts) = fonts
            {
                let (mut i, mut exists) = (0u32, windows::core::BOOL(0));
                if fonts
                    .FindFamilyName(&HSTRING::from("Cascadia Mono"), &mut i, &mut exists)
                    .is_ok()
                    && exists.as_bool()
                {
                    found = "Cascadia Mono".into();
                }
            }
        }
        *m.borrow_mut() = Some(found.clone());
        found
    })
}

/// A text format, made once: mono (the plot's) or the UI's.
fn font(gfx: &Gfx, mono: bool, size: f32, weight: DWRITE_FONT_WEIGHT) -> Option<IDWriteTextFormat> {
    let key = (mono, size.to_bits(), weight.0);
    if let Some(f) = FONTS.with(|f| f.borrow().get(&key).cloned()) {
        return Some(f);
    }
    let fam = if mono {
        mono_family(gfx)
    } else {
        gfx.fonts.text_family.clone()
    };
    let f = crate::render::format(&gfx.dw, &fam, size, weight).ok()?;
    FONTS.with(|fs| fs.borrow_mut().insert(key, f.clone()));
    Some(f)
}

fn round_stroke(gfx: &Gfx) -> Option<ID2D1StrokeStyle> {
    ROUND.with(|r| {
        if r.borrow().is_none() {
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
            *r.borrow_mut() = unsafe { gfx.d2d.CreateStrokeStyle(&props, None) }.ok();
        }
        r.borrow().clone()
    })
}

const REGULAR: DWRITE_FONT_WEIGHT = DWRITE_FONT_WEIGHT_REGULAR;

fn lay(gfx: &Gfx, s: &str, f: &IDWriteTextFormat) -> Option<(IDWriteTextLayout, f32, f32)> {
    let l = gfx.layout(s, f, 0.0, 0.0)?;
    let (w, h) = measure(&l);
    // SAFETY: a live layout
    let _ = unsafe { l.SetMaxHeight(h) };
    Some((l, w, h))
}

fn rgb(c: (f32, f32, f32), k: f32) -> Color {
    Color {
        r: c.0 * k,
        g: c.1 * k,
        b: c.2 * k,
        a: 1.0,
    }
}

/// An equation's colour: the accent for the bar's, then the palette.
fn layer_color(t: &Theme, i: usize) -> Color {
    layer_rgb(i).map_or(t.accent, |c| rgb(c, 1.0))
}

/// --graph-plot: color-mix(bg 70%, bg-tape)
fn plot_bg(t: &Theme) -> Color {
    let (a, b) = (t.bg, t.bg_tape);
    Color {
        r: a.r * 0.7 + b.r * 0.3,
        g: a.g * 0.7 + b.g * 0.3,
        b: a.b * 0.7 + b.b * 0.3,
        a: 1.0,
    }
}

/// The theme the plot is drawn in: the bar's, or the other one with reverse contrast.
fn plot_theme(t: &Theme, prefs: &GraphPrefs) -> Theme {
    if prefs.reverse_contrast {
        Theme::new(!t.dark)
    } else {
        *t
    }
}

// ---- drawing on a target ----

/// Drawing on a render target in its own units (the plot's viewBox, through the target's transform).
struct Pen<'a> {
    rt: &'a ID2D1RenderTarget,
    brush: ID2D1SolidColorBrush,
    gfx: &'a Gfx,
}

impl<'a> Pen<'a> {
    fn new(rt: &'a ID2D1RenderTarget, gfx: &'a Gfx) -> Option<Pen<'a>> {
        let c = D2D1_COLOR_F::default();
        // SAFETY: a brush on our live target
        let brush = unsafe { rt.CreateSolidColorBrush(&c, None) }.ok()?;
        Some(Pen { rt, brush, gfx })
    }

    fn b(&self, c: Color) -> &ID2D1SolidColorBrush {
        // SAFETY: a live brush
        unsafe { self.brush.SetColor(&c) };
        &self.brush
    }

    fn line(&self, a: P2, b: P2, w: f32, c: Color) {
        // SAFETY: inside the target's draw
        unsafe {
            self.rt.DrawLine(
                Vector2 { X: a.0, Y: a.1 },
                Vector2 { X: b.0, Y: b.1 },
                self.b(c),
                w,
                None,
            )
        };
    }

    /// A dashed line, `on` long and `off` apart.
    fn dashed(&self, a: P2, b: P2, w: f32, c: Color, on: f32, off: f32) {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len <= 0.0 || !len.is_finite() {
            return;
        }
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let mut d = 0.0;
        while d < len {
            let e = (d + on).min(len);
            self.line((a.0 + ux * d, a.1 + uy * d), (a.0 + ux * e, a.1 + uy * e), w, c);
            d += on + off;
        }
    }

    fn fill(&self, r: Rect, c: Color) {
        let d = D2D_RECT_F {
            left: r.x,
            top: r.y,
            right: r.x + r.w,
            bottom: r.y + r.h,
        };
        // SAFETY: as above
        unsafe { self.rt.FillRectangle(&d, self.b(c)) };
    }

    fn frame(&self, r: Rect, w: f32, c: Color) {
        let d = D2D_RECT_F {
            left: r.x,
            top: r.y,
            right: r.x + r.w,
            bottom: r.y + r.h,
        };
        // SAFETY: as above
        unsafe { self.rt.DrawRectangle(&d, self.b(c), w, None) };
    }

    fn ellipse(&self, c: P2, rx: f32, ry: f32, w: f32, color: Color) {
        let e = D2D1_ELLIPSE {
            point: Vector2 { X: c.0, Y: c.1 },
            radiusX: rx,
            radiusY: ry,
        };
        // SAFETY: as above
        unsafe { self.rt.DrawEllipse(&e, self.b(color), w, None) };
    }

    /// A closed polygon, filled and/or stroked.
    fn poly(&self, pts: &[P2], fill: Option<Color>, stroke: Option<(Color, f32)>) {
        let Some(geo) = polygon(self.gfx, pts) else {
            return;
        };
        // SAFETY: as above; the geometry is a local
        unsafe {
            if let Some(f) = fill {
                self.rt.FillGeometry(&geo, self.b(f), None);
            }
            if let Some((c, w)) = stroke {
                self.rt.DrawGeometry(&geo, self.b(c), w, None);
            }
        }
    }

    /// Text with `anchor` (0…1 across, 0…1 down) of its box on `at`; its size.
    fn text(&self, s: &str, f: &IDWriteTextFormat, c: Color, at: P2, anchor: (f32, f32)) -> (f32, f32) {
        let Some((l, w, h)) = lay(self.gfx, s, f) else {
            return (0.0, 0.0);
        };
        self.layout(&l, (at.0 - w * anchor.0, at.1 - h * anchor.1), c);
        (w, h)
    }

    fn layout(&self, l: &IDWriteTextLayout, at: P2, c: Color) {
        // SAFETY: as above
        unsafe {
            self.rt.DrawTextLayout(
                Vector2 { X: at.0, Y: at.1 },
                l,
                self.b(c),
                windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            )
        };
    }

    fn clip(&self, r: Rect) {
        let d = D2D_RECT_F {
            left: r.x,
            top: r.y,
            right: r.x + r.w,
            bottom: r.y + r.h,
        };
        // SAFETY: paired with unclip
        unsafe { self.rt.PushAxisAlignedClip(&d, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
    }

    fn unclip(&self) {
        // SAFETY: after clip
        unsafe { self.rt.PopAxisAlignedClip() };
    }
}

/// A closed polygon's geometry.
fn polygon(gfx: &Gfx, pts: &[P2]) -> Option<ID2D1PathGeometry> {
    let (first, rest) = pts.split_first()?;
    // SAFETY: a fresh geometry, opened, filled and closed here
    unsafe {
        let geo = gfx.d2d.CreatePathGeometry().ok()?;
        let sink = geo.Open().ok()?;
        sink.BeginFigure(Vector2 { X: first.0, Y: first.1 }, D2D1_FIGURE_BEGIN_FILLED);
        let v: Vec<Vector2> = rest.iter().map(|p| Vector2 { X: p.0, Y: p.1 }).collect();
        sink.AddLines(&v);
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close().ok()?;
        Some(geo)
    }
}

/// One path geometry of open polylines.
fn polylines<'a>(gfx: &Gfx, lines: impl Iterator<Item = &'a [(f32, f32)]>) -> Option<ID2D1PathGeometry> {
    // SAFETY: a fresh geometry, opened, filled and closed here; the points are locals
    unsafe {
        let geo = gfx.d2d.CreatePathGeometry().ok()?;
        let sink = geo.Open().ok()?;
        let mut any = false;
        let mut pts: Vec<Vector2> = Vec::new();
        for line in lines {
            let Some((&(u, v), rest)) = line.split_first() else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            sink.BeginFigure(Vector2 { X: u, Y: v }, D2D1_FIGURE_BEGIN_HOLLOW);
            pts.clear();
            pts.extend(rest.iter().map(|&(x, y)| Vector2 { X: x, Y: y }));
            sink.AddLines(&pts);
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            any = true;
        }
        sink.Close().ok()?;
        any.then_some(geo)
    }
}

// ---- the 2D mapping ----

/// The 2D plot's mapping, graph units to viewBox units and back, linear or log on each axis (Map2D).
#[derive(Clone, Copy, Debug)]
pub struct Map2D {
    pub log: (bool, bool),
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl Map2D {
    pub fn new(g: &GraphState) -> Map2D {
        let log = (g.log_x(), g.log_y());
        Map2D {
            log,
            x0: axis_t(g.x_min, log.0),
            x1: axis_t(g.x_max, log.0),
            y0: axis_t(g.y_min, log.1),
            y1: axis_t(g.y_max, log.1),
        }
    }
    pub fn px(&self, x: f64) -> f32 {
        PAD_LEFT + ((axis_t(x, self.log.0) - self.x0) / (self.x1 - self.x0)) as f32 * PLOT_W
    }
    pub fn py(&self, y: f64) -> f32 {
        PAD_TOP + ((self.y1 - axis_t(y, self.log.1)) / (self.y1 - self.y0)) as f32 * PLOT_H
    }
    pub fn data_x(&self, sx: f32) -> f64 {
        let t = self.x0 + f64::from((sx - PAD_LEFT) / PLOT_W) * (self.x1 - self.x0);
        if self.log.0 { 10f64.powf(t) } else { t }
    }
    pub fn data_y(&self, sy: f32) -> f64 {
        let t = self.y1 - f64::from((sy - PAD_TOP) / PLOT_H) * (self.y1 - self.y0);
        if self.log.1 { 10f64.powf(t) } else { t }
    }
    pub fn inside(p: P2) -> bool {
        (PAD_LEFT..=PAD_LEFT + PLOT_W).contains(&p.0) && (PAD_TOP..=PAD_TOP + PLOT_H).contains(&p.1)
    }
}

fn plot_rect() -> Rect {
    Rect::new(PAD_LEFT, PAD_TOP, PLOT_W, PLOT_H)
}

fn svg_h(g: &GraphState) -> f32 {
    if g.shows_3d() { SVG_H3D } else { SVG_H }
}

/// Where each critical point's mark sits (viewBox units), None off the plot: on the 2D plot, or projected into
/// the 3D cube (markPoints).
fn marks(g: &GraphState) -> Vec<Option<P2>> {
    let all = g.all_critical();
    if g.shows_3d() {
        let Some(c) = g.cube((SVG_W, SVG_H3D)) else {
            return vec![None; all.len()];
        };
        return all
            .iter()
            .map(|(_, p)| c.project(p.x, p.y, p.z.unwrap_or(f64::NAN)))
            .collect();
    }
    let m = Map2D::new(g);
    all.iter()
        .map(|(_, c)| {
            let q = (m.px(c.x), m.py(c.y));
            Map2D::inside(q).then_some(q)
        })
        .collect()
}

/// The mark under the pointer.
fn hovered_mark(g: &GraphState) -> Option<usize> {
    let p = g.pointer?;
    nearest(&marks(g), p, SNAP)
}

// ---- the panel's layout ----

// the panel's box (.graph): a hairline on top, padding 12 / gutter / 16, a 20-high head, 8 under it
const TOP: f32 = 1.0 + 12.0;
const HEAD_H: f32 = 20.0;
const BOTTOM: f32 = 16.0;
const CHIP_H: f32 = 22.0;
const CHIP_GAP: f32 = 4.0;
const EQ_ROW: f32 = 22.0;
const SET_ROW: f32 = 22.0;
const SET_PAD: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    View3D,
    Contour,
    Add,
    Wrench,
    Reset,
}

struct HeadItem {
    rect: Rect,
    btn: Btn,
    text: (IDWriteTextLayout, f32),
    /// the chosen one of 3D / contour, or the wrench while the settings are open
    on: bool,
    enabled: bool,
}

struct EqRow {
    rect: Rect,
    /// 0 the bar's, n the nth layer
    index: usize,
    id: Option<u32>,
    text: Option<(IDWriteTextLayout, f32)>,
    placeholder: bool,
    note: Option<(IDWriteTextLayout, f32)>,
    /// an added one's field (its text from `x`) and remove button
    field: Option<Rect>,
    remove: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    None,
    Check(Check),
    Seg(Seg, usize),
    Field(FieldId),
    ZoomSquare,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Look {
    Text,
    Check(bool),
    Cell(bool, bool),
    Field(bool),
    Button(bool),
}

struct SetItem {
    rect: Rect,
    act: Act,
    look: Look,
    text: Option<(IDWriteTextLayout, f32)>,
}

struct Chip {
    rect: Rect,
    /// into `all_critical`; None for a root
    index: Option<usize>,
    kind: (IDWriteTextLayout, f32),
    coords: IDWriteTextLayout,
    color: Option<Color>,
    copy: f64,
}

/// The 3D base pass's cube, plane and scene, kept for the pointer pass.
type Scene3 = (Cube, Frame, Scene);
/// A chip to lay out: its point's index, kind, coordinates, what it copies and its equation's colour.
type ChipItem = (Option<usize>, &'static str, String, f64, Option<Color>);

/// Where everything goes for a width and a generation, panel-relative.
pub struct Geo {
    height: f32,
    label: Option<IDWriteTextLayout>,
    domain: Option<IDWriteTextLayout>,
    domain_x: f32,
    head: Vec<HeadItem>,
    eqs: Vec<EqRow>,
    settings: Option<(Rect, Vec<SetItem>)>,
    message: Option<(f32, IDWriteTextLayout)>,
    svg: Option<Rect>,
    titles: Vec<(f32, IDWriteTextLayout)>,
    empty: Option<(f32, IDWriteTextLayout)>,
    chips: Vec<Chip>,
}

/// What the panel keeps between frames: its layout, each sample's paths, the base pass's bitmap and the 3D scene.
#[derive(Default)]
pub struct Cache {
    geo: Option<((u32, u64), Rc<Geo>)>,
    paths: Option<(u64, Rc<HashMap<u32, Paths>>)>,
    base: Option<(BaseKey, ID2D1Bitmap)>,
    scene: Option<(u64, Rc<Scene3>)>,
    base_draws: u32,
    base_ms: f32,
    overlay_ms: f32,
}

impl Cache {
    pub fn debug(&self) -> serde_json::Value {
        serde_json::json!({
            "baseDraws": self.base_draws, "baseMs": self.base_ms, "overlayMs": self.overlay_ms,
            "patches": self.scene.as_ref().map_or(0, |s| s.1.2.patches.len()),
            "pieces": self.scene.as_ref().map_or(0, |s| s.1.2.pieces.len()),
            "bitmap": self.base.is_some(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BaseKey {
    generation: u64,
    target: usize,
    w: u32,
    h: u32,
    dpi: u32,
    ink: u32,
    bg: u32,
}

/// A sample's curves and level curves for one equation, in the sample's unit square.
struct Paths {
    curve: Option<ID2D1PathGeometry>,
    dots: Vec<(f32, f32)>,
    levels: Vec<(ID2D1PathGeometry, f32)>,
}

fn paths_for(gfx: &Gfx, p: &Plot, s: &Sample) -> Paths {
    let (lines, dots) = runs(&p.points, s.window, s.log);
    let curve = polylines(gfx, lines.iter().map(|l| l.as_slice()));
    let [x0, x1, y0, y1] = s.window;
    let (tx0, tx1) = (axis_t(x0, s.log.0), axis_t(x1, s.log.0));
    let (ty0, ty1) = (axis_t(y0, s.log.1), axis_t(y1, s.log.1));
    let n = p.contours.len();
    let levels = p
        .contours
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let segs: Vec<[(f32, f32); 2]> = c
                .segments
                .as_chunks::<4>()
                .0
                .iter()
                .map(|q| {
                    let at = |x: f64, y: f64| {
                        (
                            ((axis_t(x, s.log.0) - tx0) / (tx1 - tx0)) as f32,
                            ((ty1 - axis_t(y, s.log.1)) / (ty1 - ty0)) as f32,
                        )
                    };
                    [at(q[0], q[1]), at(q[2], q[3])]
                })
                .collect();
            let geo = polylines(gfx, segs.iter().map(|s| s.as_slice()))?;
            let opacity = if n > 1 {
                0.35 + 0.65 * i as f32 / (n - 1) as f32
            } else {
                1.0
            };
            Some((geo, opacity))
        })
        .collect();
    Paths {
        curve,
        dots,
        levels,
    }
}

fn paths(gfx: &Gfx, g: &GraphState) -> Option<Rc<HashMap<u32, Paths>>> {
    let s = g.sample.as_ref()?;
    if let Some((serial, p)) = &g.cache.borrow().paths
        && *serial == s.serial
    {
        return Some(p.clone());
    }
    let mut map = HashMap::new();
    if let Some(p) = s.main.as_ref().filter(|p| p.ok()) {
        map.insert(0, paths_for(gfx, p, s));
    }
    for (id, p) in &s.layers {
        if let Some(p) = p.as_ref().filter(|p| p.ok()) {
            map.insert(*id, paths_for(gfx, p, s));
        }
    }
    let p = Rc::new(map);
    g.cache.borrow_mut().paths = Some((s.serial, p.clone()));
    Some(p)
}

fn small(gfx: &Gfx) -> Option<IDWriteTextFormat> {
    font(gfx, true, size::SMALL, REGULAR)
}

fn geo(gfx: &Gfx, g: &GraphState, width: f32) -> Option<Rc<Geo>> {
    let key = (width.to_bits(), g.generation);
    if let Some((k, geo)) = &g.cache.borrow().geo
        && *k == key
    {
        return Some(geo.clone());
    }
    let t_mono = small(gfx)?;
    let ui_small = font(gfx, false, size::SMALL, REGULAR)?;
    let icon = crate::render::format(&gfx.dw, "Segoe MDL2 Assets", 11.0, REGULAR).ok()?;
    let caption = font(gfx, true, size::CAPTION, REGULAR)?;
    let label_f = font(gfx, true, size::BODY, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
    let inner = width - 2.0 * size::GUTTER;
    let x0 = size::GUTTER;
    let prefs = gs::prefs();
    // the head, right to left: reset, the wrench, +, then a surface's 3D | contour
    let mut head = Vec::new();
    let mut right = width - size::GUTTER + 8.0;
    let mut push = |btn: Btn, s: &str, f: &IDWriteTextFormat, pad: f32, on: bool, enabled: bool| {
        let (l, w, _) = lay(gfx, s, f)?;
        let bw = w + pad;
        right -= bw;
        head.push(HeadItem {
            rect: Rect::new(right, TOP, bw, HEAD_H),
            btn,
            text: (l, w),
            on,
            enabled,
        });
        right -= 2.0;
        Some(())
    };
    push(Btn::Reset, "reset", &ui_small, 16.0, false, !g.at_home())?;
    push(Btn::Wrench, "\u{E90F}", &icon, 11.0, g.settings_open, true)?;
    push(Btn::Add, "\u{E710}", &icon, 11.0, false, true)?;
    if g.kind() == Kind::Surface && g.main().is_some_and(|p| p.ok()) {
        push(Btn::Contour, "contour", &ui_small, 16.0, !g.shows_3d(), true)?;
        push(Btn::View3D, "3D", &ui_small, 16.0, g.shows_3d(), true)?;
    }
    let room = right - 8.0 - x0;
    let label = gfx.layout_in(&g.label(), &label_f, (room * 0.6).max(40.0), HEAD_H, DWRITE_TEXT_ALIGNMENT_LEADING);
    let label_w = label.as_ref().map_or(0.0, |l| measure(l).0.min(room * 0.6));
    let res_x = (g.x_max - g.x_min) / f64::from(PLOT_W);
    let unit = match g.main().and_then(|p| p.unit) {
        Some(AngleUnit::Rad) => "  rad",
        Some(AngleUnit::Deg) => "  deg",
        None => "",
    };
    let domain = format!(
        "x ∈ [{}, {}]{unit}",
        fmt_g(round_to_pixel(g.x_min, res_x), 6),
        fmt_g(round_to_pixel(g.x_max, res_x), 6)
    );
    let domain_x = x0 + label_w + 12.0;
    let domain = gfx.layout_in(&domain, &t_mono, (right - 8.0 - domain_x).max(1.0), HEAD_H, DWRITE_TEXT_ALIGNMENT_LEADING);
    let mut y = TOP + HEAD_H + 8.0;
    // the equations, when there are several
    let mut eqs = Vec::new();
    if !g.layers.is_empty() {
        let mut row = |index: usize, id: Option<u32>, text: Option<String>, placeholder: bool, note: Option<String>, added: bool, y: f32| {
            let shown = text.as_deref().unwrap_or("");
            let text = if shown.is_empty() { None } else { lay(gfx, shown, &t_mono).map(|l| (l.0, l.1)) };
            let note = note.and_then(|n| lay(gfx, &n, &caption).map(|l| (l.0, l.1)));
            let r = Rect::new(x0, y, inner, EQ_ROW);
            eqs.push(EqRow {
                rect: r,
                index,
                id,
                text,
                placeholder,
                note,
                field: added.then(|| Rect::new(x0 + 15.0, y + 2.0, inner - 15.0 - 26.0, EQ_ROW - 4.0)),
                remove: added.then(|| Rect::new(x0 + inner - 20.0, y + 2.0, 18.0, 18.0)),
            });
        };
        row(0, None, Some(g.label()), false, None, false, y);
        y += EQ_ROW;
        for (i, l) in g.layers.iter().enumerate() {
            let focused = g.focus == Some(FieldId::Layer(l.id));
            let text = if focused { g.edit.text().to_string() } else { l.expression.replace("sqrt(", "√(") };
            let placeholder = text.is_empty() && !focused;
            let shown = if placeholder { "y = …  or  x^2 + y^2 = 9  or  z = …".to_string() } else { text };
            let plot = g.sample.as_ref().and_then(|s| s.layers.iter().find(|p| p.0 == l.id));
            let note = match plot {
                _ if l.blank() => None,
                Some((_, None)) => Some("can’t graph this".to_string()),
                Some((_, Some(p))) if p.error.is_some() => p.error.clone(),
                Some((_, Some(p))) if g.shows_3d() && p.surface.is_none() => Some("not a surface: shown in 2D".into()),
                _ => None,
            };
            row(i + 1, Some(l.id), Some(shown), placeholder, note, !l.from_bar, y);
            y += EQ_ROW;
        }
        y += 8.0;
    }
    // the settings
    let settings = if g.settings_open {
        let radians = g.radians;
        let ctls = gs::controls(&prefs, &g.settings_view(radians));
        let mut items = Vec::new();
        let (left, maxx) = (x0 + SET_PAD, x0 + inner - SET_PAD);
        let (mut cx, mut cy) = (left, y + SET_PAD);
        let place = |w: f32, cx: &mut f32, cy: &mut f32| {
            if *cx > left && *cx + w > maxx {
                *cx = left;
                *cy += SET_ROW + 4.0;
            }
            let r = Rect::new(*cx, *cy, w, SET_ROW);
            *cx += w + 10.0;
            r
        };
        for c in ctls {
            match c {
                Ctl::Break => {
                    cx = left;
                    cy += SET_ROW + 4.0;
                }
                Ctl::Text(s) => {
                    let Some((l, w, _)) = lay(gfx, &s, &t_mono) else { continue };
                    let r = place(w, &mut cx, &mut cy);
                    items.push(SetItem { rect: r, act: Act::None, look: Look::Text, text: Some((l, w)) });
                }
                Ctl::Check(k, s, on) => {
                    let Some((l, w, _)) = lay(gfx, s, &t_mono) else { continue };
                    let r = place(16.0 + w, &mut cx, &mut cy);
                    items.push(SetItem { rect: r, act: Act::Check(k), look: Look::Check(on), text: Some((l, w)) });
                }
                Ctl::Seg(k, options, chosen, enabled) => {
                    let cells: Vec<_> = options.iter().filter_map(|s| lay(gfx, s, &t_mono)).collect();
                    let total: f32 = cells.iter().map(|c| c.1 + 14.0).sum();
                    let r = place(total, &mut cx, &mut cy);
                    let mut x = r.x;
                    for (i, (l, w, _)) in cells.into_iter().enumerate() {
                        let cw = w + 14.0;
                        items.push(SetItem {
                            rect: Rect::new(x, r.y, cw, r.h),
                            act: if enabled { Act::Seg(k, i) } else { Act::None },
                            look: Look::Cell(i == chosen, enabled),
                            text: Some((l, w)),
                        });
                        x += cw;
                    }
                }
                Ctl::Field(id, value, placeholder, w) => {
                    let focused = g.focus == Some(id);
                    let shown = if focused { g.edit.text().to_string() } else { value };
                    let ph = shown.is_empty() && !focused;
                    let text = if ph { placeholder.to_string() } else { shown };
                    let text = if text.is_empty() { None } else { lay(gfx, &text, &t_mono).map(|l| (l.0, l.1)) };
                    let r = place(w, &mut cx, &mut cy);
                    items.push(SetItem { rect: r, act: Act::Field(id), look: Look::Field(ph), text });
                }
                Ctl::ZoomSquare(enabled) => {
                    let Some((l, w, _)) = lay(gfx, "Zoom Square", &t_mono) else { continue };
                    let r = place(w + 16.0, &mut cx, &mut cy);
                    items.push(SetItem {
                        rect: r,
                        act: if enabled { Act::ZoomSquare } else { Act::None },
                        look: Look::Button(enabled),
                        text: Some((l, w)),
                    });
                }
            }
        }
        let bottom = items.iter().map(|i| i.rect.bottom()).fold(y, f32::max) + SET_PAD;
        let r = Rect::new(x0, y, inner, bottom - y);
        y = bottom + 8.0;
        Some((r, items))
    } else {
        None
    };
    let mut geo = Geo {
        height: 0.0,
        label,
        domain,
        domain_x,
        head,
        eqs,
        settings,
        message: None,
        svg: None,
        titles: Vec::new(),
        empty: None,
        chips: Vec::new(),
    };
    let message = match g.main().and_then(|p| p.error.clone()) {
        Some(e) => Some(e),
        None if g.failed() => Some(format!("can’t graph {}", g.expression)),
        None => None,
    };
    if let Some(m) = message {
        geo.message = gfx
            .layout_in(&m, &gfx.fonts.body, inner, 18.0, DWRITE_TEXT_ALIGNMENT_LEADING)
            .map(|l| (y + 12.0, l));
        y += 12.0 + 18.0 + 12.0;
    } else {
        let sh = svg_h(g);
        let h = (inner * sh / SVG_W).min(sh);
        let w = h * SVG_W / sh;
        geo.svg = Some(Rect::new(x0, y, w, h));
        y += h;
        if g.sample.is_some() {
            y += 8.0;
            let several = !g.layers.is_empty();
            let t = Theme::new(false);
            let mut section = |title: &str, items: Vec<ChipItem>, y: &mut f32, empty: bool| {
                *y += 12.0;
                if let Some((l, _, _)) = lay(gfx, &title.to_uppercase(), &caption) {
                    geo.titles.push((*y, l));
                }
                *y += 14.0 + 6.0;
                if items.is_empty() {
                    if empty && let Some((l, _, _)) = lay(gfx, "None in this domain", &ui_small) {
                        geo.empty = Some((*y, l));
                        *y += 16.0;
                    }
                    return;
                }
                let built: Vec<_> = items
                    .into_iter()
                    .filter_map(|(index, kind, coords, copy, color)| {
                        let (kl, kw, _) = lay(gfx, kind, &caption)?;
                        let (cl, cw, _) = lay(gfx, &coords, &t_mono)?;
                        let w = 7.0 + if color.is_some() { 12.0 } else { 0.0 } + kw + 6.0 + cw + 7.0;
                        Some((index, kl, kw, cl, w, copy, color))
                    })
                    .collect();
                let widths: Vec<f32> = built.iter().map(|b| b.4).collect();
                let places = wrap(&widths, inner, CHIP_GAP);
                let lines = places.last().map_or(0, |p| p.1 + 1);
                for ((index, kl, kw, cl, w, copy, color), (cx, line)) in built.into_iter().zip(places) {
                    geo.chips.push(Chip {
                        rect: Rect::new(x0 + cx, *y + line as f32 * (CHIP_H + CHIP_GAP), w, CHIP_H),
                        index,
                        kind: (kl, kw),
                        coords: cl,
                        color,
                        copy,
                    });
                }
                *y += lines as f32 * (CHIP_H + CHIP_GAP) - CHIP_GAP;
            };
            let deg = g.deg();
            let crit = g
                .all_critical()
                .into_iter()
                .enumerate()
                .map(|(i, (layer, c))| {
                    let coords = match c.z {
                        Some(z) => format!("({}{deg}, {}{deg}, {})", coord(c.x), coord(c.y), coord(z)),
                        None => format!("({}{deg}, {})", coord(c.x), coord(c.y)),
                    };
                    // a surface's chip copies its height; a y-intercept's x is always 0, so its chip copies y
                    let copy = c.z.unwrap_or(if c.kind == qcalc_ffi::CriticalKind::YIntercept { c.y } else { c.x });
                    let color = several.then(|| layer_rgb(layer).map_or(t.accent, |c| rgb(c, 1.0)));
                    (Some(i), kind_name(c.kind), coords, copy, color)
                })
                .collect();
            section(g.detail_title(), crit, &mut y, true);
            let roots = g.roots();
            if !roots.is_empty() {
                let items = roots
                    .into_iter()
                    .map(|x| (None, "root", format!("x = {}{deg}", coord(x)), x, None))
                    .collect();
                section("Roots", items, &mut y, false);
            }
        }
    }
    geo.height = y + BOTTOM;
    let geo = Rc::new(geo);
    g.cache.borrow_mut().geo = Some((key, geo.clone()));
    Some(geo)
}

/// The panel's height at this width (window.rs `Bar::layout`).
pub fn height(gfx: &Gfx, g: &GraphState, width: f32) -> f32 {
    geo(gfx, g, width).map_or(TOP + HEAD_H + BOTTOM, |g| g.height)
}

/// A field in the panel has the keys: the line's caret hides (window.rs).
pub fn focused(m: &BarModel) -> bool {
    m.graph.as_ref().is_some_and(|g| g.focus.is_some())
}

// ---- painting ----

/// The panel in `r` (window DIPs).
pub fn paint(c: &Canvas, g: &GraphState, r: Rect) {
    let Some(geo) = geo(c.gfx, g, r.w) else {
        return;
    };
    let t = c.theme;
    c.fill(r, t.bg_tape);
    c.fill(Rect::new(r.x, r.y, r.w, 1.0), t.line);
    let hover = g.hover;
    let over = |rect: &Rect| hover.is_some_and(|(x, y)| rect.contains(x, y));
    let at = |rect: &Rect| Rect::new(r.x + rect.x, r.y + rect.y, rect.w, rect.h);
    // the head
    if let Some(l) = &geo.label {
        c.text(l, r.x + size::GUTTER, r.y + TOP, t.ink);
    }
    if let Some(l) = &geo.domain {
        c.text(l, r.x + geo.domain_x, r.y + TOP, t.muted);
    }
    for h in &geo.head {
        let br = at(&h.rect);
        let hot = over(&h.rect) && h.enabled;
        if h.on {
            c.fill_round(br, size::RADIUS_S, with_alpha(t.ink, 0.08));
        } else if hot {
            c.fill_round(br, size::RADIUS_S, t.fill_hover);
        }
        let ink = if !h.enabled {
            with_alpha(t.muted, 0.4)
        } else if hot || h.on {
            t.ink
        } else {
            t.muted
        };
        let (l, w) = &h.text;
        if let Some(lh) = Some(measure(l).1) {
            c.text(l, br.x + (br.w - w) / 2.0, br.y + (br.h - lh) / 2.0, ink);
        }
    }
    // the equations: a colour bar, the equation, a hairline under each
    let focus = g.focus;
    for e in &geo.eqs {
        let er = at(&e.rect);
        c.fill(Rect::new(er.x + 4.0, er.y + 4.0, 3.0, 14.0), layer_color(t, e.index));
        let tx = er.x + 15.0;
        let editing = e.id.is_some_and(|id| focus == Some(FieldId::Layer(id)));
        if let Some((l, w)) = &e.text {
            let lh = measure(l).1;
            let ty = er.y + (er.h - lh) / 2.0;
            if editing {
                paint_selection(c, &g.edit, l, tx, ty, lh);
            }
            c.text(l, tx, ty, if e.placeholder { t.muted } else { t.ink });
            if let Some((n, _)) = &e.note {
                let nh = measure(n).1;
                c.text(n, tx + w + 10.0, er.y + (er.h - nh) / 2.0, t.muted);
            }
            if editing {
                caret(c, &g.edit, Some(l), tx, ty, lh, t);
            }
        } else if editing {
            caret(c, &g.edit, None, tx, er.y + 4.0, 14.0, t);
        }
        if let Some(rm) = &e.remove {
            let rr = at(rm);
            if over(rm) {
                c.fill_round(rr, 4.0, t.fill_hover);
            }
            c.line(rr.x + 5.5, rr.y + 5.5, rr.x + 12.5, rr.y + 12.5, 1.25, t.muted);
            c.line(rr.x + 12.5, rr.y + 5.5, rr.x + 5.5, rr.y + 12.5, 1.25, t.muted);
        }
        c.fill(Rect::new(er.x, er.bottom() - 0.5, er.w, 0.5), with_alpha(t.ink, 0.1));
    }
    if let Some((box_r, items)) = &geo.settings {
        let br = at(box_r);
        c.fill(br, t.bg);
        stroke(c, br, 0.75, with_alpha(t.ink, 0.16));
        for it in items {
            paint_item(c, g, it, at(&it.rect), over(&it.rect));
        }
    }
    if let Some((y, m)) = &geo.message {
        c.text(m, r.x + size::GUTTER, r.y + y, t.muted);
    }
    if let Some(svg) = geo.svg {
        let spot = geo
            .chips
            .iter()
            .find(|chip| over(&chip.rect))
            .and_then(|chip| chip.index);
        plot(c, g, at(&svg), spot);
    }
    for (y, l) in &geo.titles {
        c.text(l, r.x + size::GUTTER, r.y + y, t.muted);
    }
    if let Some((y, l)) = &geo.empty {
        c.text(l, r.x + size::GUTTER, r.y + y, t.muted);
    }
    let pointed = hovered_mark(g);
    for chip in &geo.chips {
        let cr = at(&chip.rect);
        let lit = over(&chip.rect) || (chip.index.is_some() && chip.index == pointed);
        if lit {
            c.fill(cr, with_alpha(t.accent, 0.1));
        }
        stroke(c, cr, 0.75, if lit { with_alpha(t.accent, 0.6) } else { with_alpha(t.ink, 0.16) });
        let mut x = cr.x + 7.0;
        if let Some(col) = chip.color {
            c.fill(Rect::new(x, cr.y + (cr.h - 6.0) / 2.0, 6.0, 6.0), col);
            x += 12.0;
        }
        let (kl, kw) = &chip.kind;
        let kh = measure(kl).1;
        c.text(kl, x, cr.y + (cr.h - kh) / 2.0, if lit { t.accent } else { t.muted });
        let ch = measure(&chip.coords).1;
        c.text(&chip.coords, x + kw + 6.0, cr.y + (cr.h - ch) / 2.0, if lit { t.accent } else { t.ink });
    }
}

fn stroke(c: &Canvas, r: Rect, w: f32, col: Color) {
    let d = D2D_RECT_F {
        left: r.x + w / 2.0,
        top: r.y + w / 2.0,
        right: r.x + r.w - w / 2.0,
        bottom: r.y + r.h - w / 2.0,
    };
    // SAFETY: inside the frame
    unsafe { c.target().DrawRectangle(&d, c.brush(col), w, None) };
}

fn paint_selection(c: &Canvas, f: &crate::input::Field, l: &IDWriteTextLayout, x: f32, y: f32, h: f32) {
    let (a, b) = f.selection();
    if a == b {
        return;
    }
    let (xa, xb) = (caret_x(l, f.to_utf16(a)), caret_x(l, f.to_utf16(b)));
    c.fill(Rect::new(x + xa.min(xb), y, (xb - xa).abs(), h), c.theme.selection);
}

fn caret(c: &Canvas, f: &crate::input::Field, l: Option<&IDWriteTextLayout>, x: f32, y: f32, h: f32, t: &Theme) {
    let cx = l.map_or(0.0, |l| caret_x(l, f.to_utf16(f.caret())));
    c.fill(Rect::new((x + cx).round(), y, 1.0, h), t.accent);
}

fn paint_item(c: &Canvas, g: &GraphState, it: &SetItem, r: Rect, hot: bool) {
    let t = c.theme;
    let text_at = |x: f32, col: Color| {
        if let Some((l, _)) = &it.text {
            let h = measure(l).1;
            c.text(l, x, r.y + (r.h - h) / 2.0, col);
        }
    };
    match it.look {
        Look::Text => text_at(r.x, t.muted),
        Look::Check(on) => {
            let b = Rect::new(r.x, r.y + (r.h - 11.0) / 2.0, 11.0, 11.0);
            if on {
                c.fill(b, t.accent);
                c.line(b.x + 2.5, b.y + 5.5, b.x + 4.75, b.y + 8.0, 1.5, t.bg);
                c.line(b.x + 4.75, b.y + 8.0, b.x + 8.75, b.y + 3.0, 1.5, t.bg);
            } else {
                stroke(c, b, 1.0, with_alpha(t.ink, if hot { 0.6 } else { 0.35 }));
            }
            text_at(r.x + 16.0, t.ink);
        }
        Look::Cell(on, enabled) => {
            if on {
                c.fill(r, with_alpha(t.ink, 0.1));
            } else if hot && enabled {
                c.fill(r, t.fill_hover);
            }
            stroke(c, r, 0.75, with_alpha(t.ink, 0.2));
            let col = if !enabled {
                with_alpha(t.muted, 0.5)
            } else if on {
                t.ink
            } else {
                t.muted
            };
            text_at(r.x + 7.0, col);
        }
        Look::Field(placeholder) => {
            let editing = matches!(it.act, Act::Field(id) if g.focus == Some(id));
            c.fill(r, t.bg_tape);
            stroke(c, r, 0.75, if editing { t.accent } else { with_alpha(t.ink, if hot { 0.35 } else { 0.2 }) });
            c.push_clip(r.inset(2.0, 0.0));
            let (tx, lh) = (r.x + 5.0, it.text.as_ref().map_or(14.0, |l| measure(&l.0).1));
            let ty = r.y + (r.h - lh) / 2.0;
            if editing && let Some((l, _)) = &it.text {
                paint_selection(c, &g.edit, l, tx, ty, lh);
            }
            text_at(tx, if placeholder { with_alpha(t.muted, 0.7) } else { t.ink });
            if editing {
                caret(c, &g.edit, it.text.as_ref().map(|l| &l.0), tx, ty, lh, t);
            }
            c.pop_clip();
        }
        Look::Button(enabled) => {
            if hot && enabled {
                c.fill(r, t.fill_hover);
            }
            stroke(c, r, 0.75, with_alpha(t.ink, 0.2));
            text_at(r.x + 8.0, if enabled { t.ink } else { with_alpha(t.muted, 0.5) });
        }
    }
}

/// The plot in `svg` (window DIPs): the base pass's bitmap, then the pointer's pass over it.
fn plot(c: &Canvas, g: &GraphState, svg: Rect, spot: Option<usize>) {
    let sh = svg_h(g);
    let s = svg.w / SVG_W;
    let prefs = gs::prefs();
    let rt = c.target();
    if let Some(bitmap) = base(c, g, svg, s, &prefs) {
        let d = D2D_RECT_F {
            left: svg.x,
            top: svg.y,
            right: svg.x + svg.w,
            bottom: svg.y + svg.h,
        };
        // SAFETY: inside the frame; the bitmap was made from this target (its key holds the target's address)
        unsafe {
            rt.DrawBitmap(&bitmap, Some(&d), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None)
        };
    }
    let started = Instant::now();
    let mut old = Matrix3x2::default();
    // SAFETY: inside the frame; the transform is put back below
    unsafe {
        rt.GetTransform(&mut old);
        let m = Matrix3x2 {
            M11: s * old.M11,
            M12: 0.0,
            M21: 0.0,
            M22: s * old.M22,
            M31: old.M31 + svg.x * old.M11,
            M32: old.M32 + svg.y * old.M22,
        };
        rt.SetTransform(&m);
    }
    if let Some(pen) = Pen::new(rt, c.gfx) {
        let t = plot_theme(c.theme, &prefs);
        if g.shows_3d() {
            overlay_3d(&pen, g, &t, &prefs, spot, sh);
        } else {
            overlay_2d(&pen, g, &t, &prefs, spot);
        }
    }
    // SAFETY: as above
    unsafe { rt.SetTransform(&old) };
    g.cache.borrow_mut().overlay_ms = started.elapsed().as_secs_f32() * 1000.0;
}

/// The base pass, from the cache when nothing it draws changed.
fn base(c: &Canvas, g: &GraphState, svg: Rect, s: f32, prefs: &GraphPrefs) -> Option<ID2D1Bitmap> {
    let rt = c.target();
    let (mut dpi, mut _dy) = (96.0f32, 96.0f32);
    // SAFETY: out pointers are live locals
    unsafe { rt.GetDpi(&mut dpi, &mut _dy) };
    let key = BaseKey {
        generation: g.generation,
        target: windows::core::Interface::as_raw(rt) as usize,
        w: svg.w.to_bits(),
        h: svg.h.to_bits(),
        dpi: dpi.to_bits(),
        ink: c.theme.ink.r.to_bits() ^ c.theme.ink.g.to_bits().rotate_left(7),
        bg: c.theme.bg.r.to_bits() ^ c.theme.bg.b.to_bits().rotate_left(7) ^ u32::from(prefs.reverse_contrast),
    };
    if let Some((k, b)) = &g.cache.borrow().base
        && *k == key
    {
        return Some(b.clone());
    }
    let started = Instant::now();
    let size = D2D_SIZE_F {
        width: svg.w,
        height: svg.h,
    };
    // SAFETY: a compatible target of ours, drawn between its own Begin/EndDraw
    let bitmap = unsafe {
        let brt = rt
            .CreateCompatibleRenderTarget(Some(&size), None, None, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE)
            .ok()?;
        brt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        brt.BeginDraw();
        let m = Matrix3x2 {
            M11: s,
            M12: 0.0,
            M21: 0.0,
            M22: s,
            M31: 0.0,
            M32: 0.0,
        };
        brt.SetTransform(&m);
        let t = plot_theme(c.theme, prefs);
        // the panel's colour under the padding (the bitmap has no alpha), the other's with reverse contrast
        brt.Clear(Some(&t.bg_tape));
        if let Some(pen) = Pen::new(&brt, c.gfx) {
            if g.shows_3d() {
                base_3d(&pen, g, &t, prefs);
            } else {
                base_2d(&pen, g, &t, prefs);
            }
        }
        brt.EndDraw(None, None).ok()?;
        brt.GetBitmap().ok()?
    };
    let mut cache = g.cache.borrow_mut();
    cache.base = Some((key, bitmap.clone()));
    cache.base_draws += 1;
    cache.base_ms = started.elapsed().as_secs_f32() * 1000.0;
    Some(bitmap)
}

/// The 2D plot's base: a square frame, hairline major and minor grids (or a polar one), tick marks out from the
/// frame with mono numbers, solid axes with arrows when asked, each equation in its colour, level curves'
/// labels, and the roots and critical points as small hollow squares.
fn base_2d(p: &Pen, g: &GraphState, t: &Theme, prefs: &GraphPrefs) {
    let m = Map2D::new(g);
    let rect = plot_rect();
    let (bottom, right) = (PAD_TOP + PLOT_H, PAD_LEFT + PLOT_W);
    let ink = t.ink;
    let bg = plot_bg(t);
    p.fill(rect, bg);
    let (grid_c, minor_c) = (with_alpha(ink, 0.11), with_alpha(ink, 0.045));
    let (axis_c, frame_c) = (with_alpha(ink, 0.62), with_alpha(ink, 0.3));
    let big = prefs.large_text;
    let Some(tick_f) = font(p.gfx, true, if big { 12.0 } else { 9.5 }, REGULAR) else {
        return;
    };
    let empty = Vec::new();
    let (xt, yt) = g.sample.as_ref().map_or((&empty, &empty), |s| (&s.x_ticks, &s.y_ticks));
    let gx = axis_grid(g.x_min, g.x_max, m.log.0, g.x_step, xt);
    let gy = axis_grid(g.y_min, g.y_max, m.log.1, g.y_step, yt);
    p.clip(rect);
    if prefs.grid {
        if prefs.polar && !m.log.0 && !m.log.1 {
            // circles about the origin a step apart (minor ones a fifth), and spokes every 15°
            let step = gx.step().unwrap_or((g.x_max - g.x_min) / 6.0);
            let r_max = [(g.x_min, g.y_min), (g.x_min, g.y_max), (g.x_max, g.y_min), (g.x_max, g.y_max)]
                .iter()
                .map(|(x, y)| x.hypot(*y))
                .fold(0.0, f64::max);
            let circles = |every: f64, skip: i64, color: Color| {
                if every <= 0.0 || r_max / every > 1000.0 {
                    return;
                }
                let mut k = 1.0;
                while k * every <= r_max {
                    if skip == 0 || (k as i64) % skip != 0 {
                        let r = k * every;
                        let (cx, cy) = (m.px(0.0), m.py(0.0));
                        p.ellipse((cx, cy), m.px(r) - cx, cy - m.py(r), 0.5, color);
                    }
                    k += 1.0;
                }
            };
            if prefs.minor_grid {
                circles(step / 5.0, 5, minor_c);
            }
            circles(step, 0, grid_c);
            for d in (0..360).step_by(15) {
                let a = f64::from(d).to_radians();
                let col = if d % 45 == 0 { grid_c } else { minor_c };
                p.line((m.px(0.0), m.py(0.0)), (m.px(r_max * a.cos()), m.py(r_max * a.sin())), 0.5, col);
            }
        } else {
            if prefs.minor_grid {
                for &v in &gx.minor {
                    p.line((m.px(v), PAD_TOP), (m.px(v), bottom), 0.5, minor_c);
                }
                for &v in &gy.minor {
                    p.line((PAD_LEFT, m.py(v)), (right, m.py(v)), 0.5, minor_c);
                }
            }
            for (v, _) in &gx.major {
                p.line((m.px(*v), PAD_TOP), (m.px(*v), bottom), 0.5, grid_c);
            }
            for (v, _) in &gy.major {
                p.line((PAD_LEFT, m.py(*v)), (right, m.py(*v)), 0.5, grid_c);
            }
        }
    }
    p.unclip();
    // the frame, with tick marks out from it at the labelled values
    p.frame(rect, 0.75, frame_c);
    if prefs.show_x_axis {
        for (v, _) in &gx.major {
            p.line((m.px(*v), bottom), (m.px(*v), bottom + 4.0), 0.75, frame_c);
        }
        if prefs.minor_grid {
            for &v in &gx.minor {
                p.line((m.px(v), bottom), (m.px(v), bottom + 2.0), 0.5, frame_c);
            }
        }
    }
    if prefs.show_y_axis {
        for (v, _) in &gy.major {
            p.line((PAD_LEFT - 4.0, m.py(*v)), (PAD_LEFT, m.py(*v)), 0.75, frame_c);
        }
        if prefs.minor_grid {
            for &v in &gy.minor {
                p.line((PAD_LEFT - 2.0, m.py(v)), (PAD_LEFT, m.py(v)), 0.5, frame_c);
            }
        }
    }
    if prefs.axis_numbers {
        if prefs.show_x_axis {
            for (v, label) in &gx.major {
                p.text(label, &tick_f, t.muted, (m.px(*v), SVG_H - 11.0), (0.5, 0.5));
            }
        }
        if prefs.show_y_axis {
            for (v, label) in &gy.major {
                p.text(label, &tick_f, t.muted, (PAD_LEFT - 7.0, m.py(*v)), (1.0, 0.5));
            }
        }
    }
    // the axes (x = 0 and y = 0, none on a log axis), with arrowheads at their ends when asked
    let x_axis_y = (!m.log.1).then(|| m.py(0.0)).filter(|y| (PAD_TOP..=bottom).contains(y));
    let y_axis_x = (!m.log.0).then(|| m.px(0.0)).filter(|x| (PAD_LEFT..=right).contains(x));
    let arrow = |tip: P2, dx: f32, dy: f32| {
        let (sz, half) = (6.0, 3.0);
        p.poly(
            &[
                tip,
                (tip.0 - dx * sz - dy * half, tip.1 - dy * sz + dx * half),
                (tip.0 - dx * sz + dy * half, tip.1 - dy * sz - dx * half),
            ],
            Some(axis_c),
            None,
        );
    };
    if prefs.show_x_axis
        && let Some(y) = x_axis_y
    {
        p.line((PAD_LEFT, y), (right, y), 1.0, axis_c);
        if prefs.arrows {
            arrow((right, y), 1.0, 0.0);
            arrow((PAD_LEFT, y), -1.0, 0.0);
        }
    }
    if prefs.show_y_axis
        && let Some(x) = y_axis_x
    {
        p.line((x, PAD_TOP), (x, bottom), 1.0, axis_c);
        if prefs.arrows {
            arrow((x, PAD_TOP), 0.0, -1.0);
            arrow((x, bottom), 0.0, 1.0);
        }
    }
    if let Some(name_f) = font(p.gfx, true, if big { 13.0 } else { 11.0 }, DWRITE_FONT_WEIGHT_MEDIUM) {
        if prefs.show_x_axis && !g.x_label.is_empty() {
            let at = (right - if prefs.arrows { 10.0 } else { 4.0 }, x_axis_y.unwrap_or(bottom) - 4.0);
            p.text(&g.x_label, &name_f, ink, at, (1.0, 1.0));
        }
        if prefs.show_y_axis && !g.y_label.is_empty() {
            let at = (y_axis_x.unwrap_or(PAD_LEFT) + 6.0, PAD_TOP + if prefs.arrows { 8.0 } else { 3.0 });
            p.text(&g.y_label, &name_f, ink, at, (0.0, 0.0));
        }
    }
    // the curves and level curves, each equation in its colour, from the newest sample's paths
    p.clip(rect);
    if let (Some(s), Some(paths)) = (&g.sample, paths(p.gfx, g))
        && s.log == m.log
    {
        let [x0, x1, y0, y1] = s.window;
        let (sx0, sx1) = (axis_t(x0, s.log.0), axis_t(x1, s.log.0));
        let (sy0, sy1) = (axis_t(y0, s.log.1), axis_t(y1, s.log.1));
        let (cx0, cx1) = (axis_t(g.x_min, m.log.0), axis_t(g.x_max, m.log.0));
        let (cy0, cy1) = (axis_t(g.y_min, m.log.1), axis_t(g.y_max, m.log.1));
        let xf = Matrix3x2 {
            M11: ((sx1 - sx0) / (cx1 - cx0)) as f32 * PLOT_W,
            M12: 0.0,
            M21: 0.0,
            M22: ((sy1 - sy0) / (cy1 - cy0)) as f32 * PLOT_H,
            M31: PAD_LEFT + ((sx0 - cx0) / (cx1 - cx0)) as f32 * PLOT_W,
            M32: PAD_TOP + ((cy1 - sy1) / (cy1 - cy0)) as f32 * PLOT_H,
        };
        let round = round_stroke(p.gfx);
        let draw = |geo: &ID2D1PathGeometry, color: Color, width: f32| {
            // SAFETY: inside the target's draw; the matrix is a local and the geometry lives in the cache
            unsafe {
                if let Ok(tg) = p.gfx.d2d.CreateTransformedGeometry(geo, &xf) {
                    p.rt.DrawGeometry(&tg, p.b(color), width, round.as_ref());
                }
            }
        };
        let mut eqs: Vec<(u32, usize, &Plot)> = g
            .layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| Some((l.id, i + 1, g.layer_plot(l)?)))
            .collect();
        if let Some(main) = g.main().filter(|p| p.ok()) {
            eqs.push((0, 0, main));
        }
        for (id, index, plot) in &eqs {
            let Some(ps) = paths.get(id) else { continue };
            let color = layer_color(t, *index);
            if let Some(curve) = &ps.curve {
                draw(curve, color, 1.6);
            }
            let implicit = plot.kind == Kind::Implicit;
            for (geo, opacity) in &ps.levels {
                draw(geo, with_alpha(color, *opacity), if implicit { 1.6 } else { 1.0 });
            }
            for &(u, v) in &ps.dots {
                let (x, y) = (u * xf.M11 + xf.M31, v * xf.M22 + xf.M32);
                p.fill(Rect::new(x - 1.5, y - 1.5, 3.0, 3.0), color);
            }
        }
        // a surface's level curves labelled on a box of the plot's colour
        if let Some(main) = g.main().filter(|p| p.ok() && p.kind == Kind::Surface)
            && let Some(label_f) = font(p.gfx, true, if big { 10.5 } else { 8.5 }, REGULAR)
        {
            for lv in &main.contours {
                let (Some(ax), Some(ay)) = (lv.at_x, lv.at_y) else { continue };
                let at = (m.px(ax), m.py(ay));
                if let Some((l, w, h)) = lay(p.gfx, &tick_text(&lv.label), &label_f) {
                    p.fill(Rect::new(at.0 - w / 2.0 - 2.0, at.1 - h / 2.0, w + 4.0, h), bg);
                    p.layout(&l, (at.0 - w / 2.0, at.1 - h / 2.0), t.muted);
                }
            }
        }
    }
    p.unclip();
    // roots and critical points: small hollow squares in their equation's colour
    p.clip(Rect::new(PAD_LEFT - 4.0, PAD_TOP - 4.0, PLOT_W + 8.0, PLOT_H + 8.0));
    if !m.log.1 {
        let mut roots: Vec<(f64, usize)> = g
            .layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| g.layer_plot(l).filter(|p| p.kind == Kind::Curve).map(|p| (i + 1, p)))
            .flat_map(|(i, p)| p.roots.iter().map(move |&r| (r, i)))
            .collect();
        if let Some(main) = g.main().filter(|p| p.ok()) {
            roots.extend(main.roots.iter().map(|&r| (r, 0)));
        }
        for (x, i) in roots {
            let q = (m.px(x), m.py(0.0));
            if Map2D::inside(q) {
                ring(p, q, bg, layer_color(t, i));
            }
        }
    }
    for (q, (layer, _)) in marks(g).into_iter().zip(g.all_critical()) {
        if let Some(q) = q {
            ring(p, q, bg, layer_color(t, layer));
        }
    }
    p.unclip();
}

/// A mark: a small hollow square in the curve's colour.
fn ring(p: &Pen, at: P2, bg: Color, color: Color) {
    let r = Rect::new(at.0 - 3.0, at.1 - 3.0, 6.0, 6.0);
    p.fill(r, bg);
    p.frame(r, 1.25, color);
}

/// The picked-out mark: filled, with a square round it.
fn halo(p: &Pen, at: P2, accent: Color) {
    p.frame(Rect::new(at.0 - 7.0, at.1 - 7.0, 14.0, 14.0), 1.0, accent);
    p.fill(Rect::new(at.0 - 3.5, at.1 - 3.5, 7.0, 7.0), accent);
}

/// The reading's point on a curve: a small filled square.
fn dot(p: &Pen, at: P2, color: Color, bg: Color) {
    let r = Rect::new(at.0 - 3.0, at.1 - 3.0, 6.0, 6.0);
    p.fill(r, color);
    p.frame(r, 1.0, bg);
}

/// Lines of mono text in a square box with a hairline edge, beside `near` (flipped to stay in `bounds`).
/// The box's colours: the plot's, its edge's and the text's.
struct BoxLook {
    bg: Color,
    edge: Color,
    ink: Color,
}

fn readout_box(p: &Pen, lines: &[String], near: P2, bounds: Rect, look: BoxLook, big: bool) {
    let BoxLook { bg, edge, ink } = look;
    let Some(f) = font(p.gfx, true, if big { 12.5 } else { 10.5 }, REGULAR) else {
        return;
    };
    let laid: Vec<_> = lines.iter().filter_map(|s| lay(p.gfx, s, &f)).collect();
    let line_h = laid.first().map_or(12.0, |l| l.2);
    let w = laid.iter().map(|l| l.1).fold(0.0, f32::max);
    let h = line_h * laid.len() as f32;
    let mut x = near.0 + 12.0;
    let mut y = near.1 - h - 10.0;
    if x + w + 10.0 > bounds.right() {
        x = near.0 - 12.0 - w;
    }
    if y < bounds.y + 4.0 {
        y = near.1 + 10.0;
    }
    let frame = Rect::new(x - 5.0, y - 3.0, w + 10.0, h + 6.0);
    p.fill(frame, with_alpha(bg, 0.96));
    p.frame(frame, 0.75, edge);
    for (i, (l, _, _)) in laid.iter().enumerate() {
        p.layout(l, (x, y + i as f32 * line_h), ink);
    }
}

/// The 2D plot's pointer pass: the crosshair and reading (each curve's value at the pointer's x, or the point
/// under it on a plot of x and y), and the picked-out critical point with guides to the frame.
fn overlay_2d(p: &Pen, g: &GraphState, t: &Theme, prefs: &GraphPrefs, spot: Option<usize>) {
    let m = Map2D::new(g);
    let (bottom, right) = (PAD_TOP + PLOT_H, PAD_LEFT + PLOT_W);
    let (ink, accent, bg) = (t.ink, t.accent, plot_bg(t));
    let hair = with_alpha(ink, 0.35);
    let snapped = hovered_mark(g);
    let rect = plot_rect();
    if snapped.is_none()
        && g.drag.is_none_or(|d| !d.moved)
        && let Some(h) = g.pointer.filter(|h| Map2D::inside(*h))
    {
        p.dashed((h.0, PAD_TOP), (h.0, bottom), 0.5, hair, 2.0, 2.0);
        match &g.reading {
            Some(Reading::Curve { x, ys }) => {
                let shown: Vec<(f32, Color)> = ys
                    .iter()
                    .filter(|(_, y)| !(m.log.1 && *y <= 0.0))
                    .map(|(i, y)| (m.py(*y), layer_color(t, *i)))
                    .filter(|(py, _)| (PAD_TOP..=bottom).contains(py))
                    .collect();
                if let Some(near) = shown
                    .iter()
                    .min_by(|a, b| (a.0 - h.1).abs().total_cmp(&(b.0 - h.1).abs()))
                {
                    let y = m.data_y(near.0);
                    p.dashed((PAD_LEFT, near.0), (right, near.0), 0.5, hair, 2.0, 2.0);
                    let lines = [format!("x  {}{}", fmt4(*x), g.deg()), format!("y  {}", fmt4(y))];
                    readout_box(p, &lines, (h.0, near.0), rect, BoxLook { bg, edge: near.1, ink }, prefs.large_text);
                }
                for (py, color) in shown {
                    dot(p, (h.0, py), color, bg);
                }
            }
            Some(Reading::Point { x, y, z }) => {
                p.dashed((PAD_LEFT, h.1), (right, h.1), 0.5, hair, 2.0, 2.0);
                let mut lines = vec![format!("x  {}", fmt4(*x)), format!("y  {}", fmt4(*y))];
                if g.kind() == Kind::Surface {
                    lines.push(format!("z  {}", z.map_or("—".into(), fmt4)));
                }
                readout_box(p, &lines, h, rect, BoxLook { bg, edge: with_alpha(ink, 0.3), ink }, prefs.large_text);
            }
            _ => {}
        }
    }
    let all = g.all_critical();
    if let Some(i) = snapped.or(spot)
        && let Some((_, c0)) = all.get(i)
    {
        let q = (m.px(c0.x), m.py(c0.y));
        if Map2D::inside(q) {
            let guide = with_alpha(accent, 0.6);
            p.dashed((q.0, q.1 + 6.0), (q.0, bottom), 0.75, guide, 3.0, 2.0);
            p.dashed((q.0 - 6.0, q.1), (PAD_LEFT, q.1), 0.75, guide, 3.0, 2.0);
            halo(p, q, accent);
            if snapped.is_some() {
                let s = format!("{} {}", kind_name(c0.kind), point_text(c0, g.deg()));
                readout_box(p, &[s], q, rect, BoxLook { bg, edge: with_alpha(ink, 0.3), ink }, false);
            }
        }
    }
}

fn point_text(c: &qcalc_ffi::CriticalPoint, deg: &str) -> String {
    match c.z {
        Some(z) => format!("({}{deg}, {}{deg}, {})", coord(c.x), coord(c.y), coord(z)),
        None => format!("({}{deg}, {})", coord(c.x), coord(c.y)),
    }
}

// ---- 3D ----

/// The 3D view's x and y grid values on the z = 0 plane; y takes x's step, the two sharing a scale.
fn grids_3d(g: &GraphState, c: &Cube) -> (AxisGrid, AxisGrid) {
    let empty = Vec::new();
    let xt = g.sample.as_ref().map_or(&empty, |s| &s.x_ticks);
    let gx = axis_grid(c.x.0, c.x.1, false, g.x_step, xt);
    let gy = axis_grid(c.y.0, c.y.1, false, g.y_step.or(gx.step()), &[]);
    (gx, gy)
}

/// The scene for the state as it is, made once per generation (the base pass and the pointer's hits share it).
fn scene(g: &GraphState) -> Option<Rc<Scene3>> {
    if let Some((generation, s)) = &g.cache.borrow().scene
        && *generation == g.generation
    {
        return Some(s.clone());
    }
    let prefs = gs::prefs();
    let c = g.cube((SVG_W, SVG_H3D))?;
    let f = Frame::new(&c);
    let (gx, gy) = grids_3d(g, &c);
    let plane = Plane {
        grid: prefs.grid,
        minor: prefs.minor_grid,
        x_major: gx.major.iter().map(|m| m.0).collect(),
        x_minor: gx.minor,
        y_major: gy.major.iter().map(|m| m.0).collect(),
        y_minor: gy.minor,
        x_axis: prefs.show_x_axis,
        y_axis: prefs.show_y_axis,
    };
    let mut surfaces = Vec::new();
    if let Some(s) = g.main().filter(|p| p.ok()).and_then(|p| p.surface.as_ref()) {
        surfaces.push((s, 0));
    }
    for (i, l) in g.layers.iter().enumerate() {
        if let Some(s) = g.layer_plot(l).and_then(|p| p.surface.as_ref()) {
            surfaces.push((s, i + 1));
        }
    }
    let sc = graph3d::scene(&c, &f, &plane, &surfaces);
    let out = Rc::new((c, f, sc));
    g.cache.borrow_mut().scene = Some((g.generation, out.clone()));
    Some(out)
}

/// The 3D view's base: a cube that cuts each surface off at its walls, the z = 0 plane's grid, the axes through
/// the origin, and the surfaces shaded in their equations' colours with their mesh, back to front; the cube's
/// front edges, the axes' arrows, names and numbers on top, and the critical points marked.
fn base_3d(p: &Pen, g: &GraphState, t: &Theme, prefs: &GraphPrefs) {
    let (ink, bg) = (t.ink, plot_bg(t));
    let frame = Rect::new(0.5, 2.0, SVG_W - 1.0, SVG_H3D - 4.0);
    p.fill(frame, bg);
    p.frame(frame, 0.75, with_alpha(ink, 0.3));
    let Some(sc) = scene(g) else { return };
    let (c, f, scene) = (&sc.0, &sc.1, &sc.2);
    let edge_c = with_alpha(ink, 0.32);
    let (back, front) = c.edges();
    for (a, b) in &back {
        p.line(c.at(*a).0, c.at(*b).0, 0.75, edge_c);
    }
    // each equation's colours by shade, made once (65 levels), not twice a patch
    let shades = |layer: usize| -> Vec<(Color, Color)> {
        let base = layer_rgb(layer).unwrap_or((t.accent.r, t.accent.g, t.accent.b));
        (0..=graph3d::SHADES as u8)
            .map(|k| (rgb(base, graph3d::shade(k, false)), rgb(base, graph3d::shade(k, true))))
            .collect()
    };
    let mut colours: HashMap<usize, Vec<(Color, Color)>> = HashMap::new();
    let (major, minor, axis) = (with_alpha(ink, 0.16), with_alpha(ink, 0.06), with_alpha(ink, 0.7));
    for piece in &scene.pieces {
        match piece {
            Piece::Patch(pts, level, layer) => {
                let cs = colours.entry(*layer).or_insert_with(|| shades(*layer));
                let (fill, mesh) = cs[usize::from(*level).min(cs.len() - 1)];
                p.poly(pts, Some(fill), Some((mesh, 0.5)));
            }
            Piece::Segment(a, b, kind) => {
                let (col, w) = match kind {
                    Line::Minor => (minor, 0.5),
                    Line::Major => (major, 0.5),
                    Line::Axis => (axis, 1.0),
                };
                p.line(*a, *b, w, col);
            }
        }
    }
    for (a, b) in &front {
        p.line(c.at(*a).0, c.at(*b).0, 0.75, edge_c);
    }
    let big = prefs.large_text;
    let (Some(num_f), Some(name_f)) = (
        font(p.gfx, true, if big { 11.5 } else { 9.5 }, REGULAR),
        font(p.gfx, true, if big { 13.0 } else { 11.0 }, DWRITE_FONT_WEIGHT_MEDIUM),
    ) else {
        return;
    };
    let label = |s: &str, at: P3, dx: f32, dy: f32, name: bool| {
        let q = c.at(at).0;
        let (fnt, col) = if name { (&name_f, ink) } else { (&num_f, t.muted) };
        p.text(s, fnt, col, (q.0 + dx, q.1 + dy), (0.5, 0.5));
    };
    let arrowhead = |from: P3, to: P3| {
        let (tip, back) = (c.at(to).0, c.at(from).0);
        let len = (tip.0 - back.0).hypot(tip.1 - back.1).max(0.001);
        let (dx, dy) = ((tip.0 - back.0) / len, (tip.1 - back.1) / len);
        p.poly(
            &[
                tip,
                (tip.0 - dx * 6.0 - dy * 2.5, tip.1 - dy * 6.0 + dx * 2.5),
                (tip.0 - dx * 6.0 + dy * 2.5, tip.1 - dy * 6.0 - dx * 2.5),
            ],
            Some(axis),
            None,
        );
    };
    let (w0, cu, cv) = (f.w0, f.clamp_u, f.clamp_v);
    if prefs.show_x_axis {
        arrowhead([0.9, cv, w0], [1.05, cv, w0]);
        label(if g.x_label.is_empty() { "x" } else { &g.x_label }, [1.14, cv, w0], 0.0, 0.0, true);
    }
    if prefs.show_y_axis {
        arrowhead([cu, 0.9, w0], [cu, 1.05, w0]);
        label(if g.y_label.is_empty() { "y" } else { &g.y_label }, [cu, 1.14, w0], 0.0, 0.0, true);
    }
    arrowhead([cu, cv, 0.9], [cu, cv, 1.05]);
    label("z", [cu, cv, 1.13], 0.0, 0.0, true);
    if prefs.axis_numbers {
        // at most about four numbers an axis, so they stay clear of each other at the origin
        let thin = |major: &[(f64, String)]| -> Vec<(f64, String)> {
            if major.len() <= 5 {
                return major.to_vec();
            }
            let step = major[1].0 - major[0].0;
            let every = (major.len() as f64 / 4.0).ceil() as i64;
            major.iter().filter(|m| ((m.0 / step).round() as i64).rem_euclid(every) == 0).cloned().collect()
        };
        let (gx, gy) = grids_3d(g, c);
        if prefs.show_x_axis {
            for (v, s) in thin(&gx.major) {
                if v != 0.0 && c.u(v).abs() < 0.97 {
                    label(&s, [c.u(v), cv, w0], 0.0, 8.0, false);
                }
            }
        }
        if prefs.show_y_axis {
            for (v, s) in thin(&gy.major) {
                if v != 0.0 && c.v(v).abs() < 0.97 {
                    label(&s, [cu, c.v(v), w0], 0.0, 8.0, false);
                }
            }
        }
        for v in nice_ticks(c.z.0, c.z.1, 4) {
            if v != 0.0 && c.w(v).abs() < 0.97 {
                label(&tick_text(&fmt4(v)), [cu, cv, c.w(v)], -18.0, 0.0, false);
            }
        }
    }
    for (q, (layer, _)) in marks(g).into_iter().zip(g.all_critical()) {
        if let Some(q) = q {
            ring(p, q, bg, layer_color(t, layer));
        }
    }
}

/// The 3D view's pointer pass: the picked-out critical point with a drop to the plane and its label, or the point
/// of the surface under the pointer with guides to the plane and the axes and its (x, y, z).
fn overlay_3d(p: &Pen, g: &GraphState, t: &Theme, prefs: &GraphPrefs, spot: Option<usize>, sh: f32) {
    let (ink, accent, bg) = (t.ink, t.accent, plot_bg(t));
    let Some(sc) = g.cache.borrow().scene.as_ref().map(|s| s.1.clone()) else {
        return;
    };
    let (c, f) = (&sc.0, &sc.1);
    let frame = Rect::new(0.5, 2.0, SVG_W - 1.0, sh - 4.0);
    let snapped = hovered_mark(g);
    let all = g.all_critical();
    let ms = marks(g);
    if let Some(i) = snapped.or(spot)
        && let (Some((_, c0)), Some(Some(q))) = (all.get(i), ms.get(i))
    {
        let foot = c.unit(c.u(c0.x), c.v(c0.y), f.w0).0;
        p.dashed(*q, foot, 0.75, with_alpha(accent, 0.7), 3.0, 2.0);
        halo(p, *q, accent);
        if snapped.is_some() {
            let s = format!("{} {}", kind_name(c0.kind), point_text(c0, ""));
            readout_box(p, &[s], *q, frame, BoxLook { bg, edge: with_alpha(ink, 0.3), ink }, false);
        }
    }
    let mut on_surface = false;
    if snapped.is_none()
        && g.drag.is_none_or(|d| !d.moved)
        && let Some(Reading::Surface { x, y, z, layer }) = &g.reading
        && let Some(q) = c.project(*x, *y, *z)
    {
        on_surface = true;
        let guide = with_alpha(ink, 0.6);
        // down (or up) to the plane, then across it to the axes, where the numbers are
        let foot = c.unit(c.u(*x), c.v(*y), f.w0).0;
        p.dashed(q, foot, 0.75, guide, 2.0, 2.0);
        p.dashed(c.unit(c.u(*x), f.clamp_v, f.w0).0, foot, 0.75, guide, 2.0, 2.0);
        p.dashed(foot, c.unit(f.clamp_u, c.v(*y), f.w0).0, 0.75, guide, 2.0, 2.0);
        dot(p, q, if *layer == 0 { ink } else { layer_color(t, *layer) }, bg);
        let lines = [format!("x  {}", fmt4(*x)), format!("y  {}", fmt4(*y)), format!("z  {}", fmt4(*z))];
        readout_box(p, &lines, q, frame, BoxLook { bg, edge: layer_color(t, *layer), ink }, prefs.large_text);
    }
    if g.pointer.is_some() && snapped.is_none() && !on_surface
        && let Some(hint_f) = font(p.gfx, true, 9.0, REGULAR)
    {
        let s = "drag/right-drag: orbit · wheel: zoom · ⇧/middle-drag: pan · dbl-click: reset";
        p.text(s, &hint_f, t.muted, (frame.right() - 8.0, frame.y + 12.0), (1.0, 0.5));
    }
}

// ---- the pointer ----

fn panel(app: &App) -> Option<Rect> {
    app.bar.layout.graph
}

fn cached_geo(app: &App) -> Option<Rc<Geo>> {
    let g = app.model.graph.as_ref()?;
    g.cache.borrow().geo.as_ref().map(|(_, geo)| geo.clone())
}

/// The point in the plot's viewBox units, when (x, y) (window DIPs) is over the svg.
fn to_svg(app: &App, x: f32, y: f32) -> Option<P2> {
    let r = panel(app)?;
    let svg = cached_geo(app)?.svg?;
    let (px, py) = (x - r.x, y - r.y);
    if !svg.contains(px, py) {
        return None;
    }
    let s = svg.w / SVG_W;
    Some(((px - svg.x) / s, (py - svg.y) / s))
}

fn key_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    // SAFETY: a plain state query
    unsafe { GetKeyState(i32::from(vk.0)) < 0 }
}

/// A left press in the panel: a head button, an equation's field or ×, a setting, a chip, or the start of a drag
/// on the plot. A field with the keys gives them back first. False outside the panel.
pub fn click(app: &mut App, x: f32, y: f32, double: bool) -> bool {
    let Some(r) = panel(app) else { return false };
    let inside = r.contains(x, y);
    let Some(geo) = cached_geo(app) else {
        return inside;
    };
    let p = (x - r.x, y - r.y);
    let hit = |rect: &Rect| inside && rect.contains(p.0, p.1);
    // a click anywhere but the field with the keys gives them back
    let on_focused = app.model.graph.as_ref().and_then(|g| g.focus).is_some_and(|f| {
        geo.settings.as_ref().is_some_and(|(_, items)| items.iter().any(|i| i.act == Act::Field(f) && hit(&i.rect)))
            || geo.eqs.iter().any(|e| e.id.is_some_and(|id| FieldId::Layer(id) == f) && e.field.is_some_and(|fr| hit(&fr)))
    });
    if !on_focused {
        commit(&mut app.model, &app.engine);
    }
    if !inside {
        return false;
    }
    if let Some(h) = geo.head.iter().find(|h| hit(&h.rect) && h.enabled) {
        let (m, e) = (&mut app.model, &app.engine);
        match h.btn {
            Btn::View3D => set_3d(m, e, true),
            Btn::Contour => set_3d(m, e, false),
            Btn::Add => add_layer(m, e),
            Btn::Wrench => {
                if let Some(g) = &mut m.graph {
                    g.settings_open = !g.settings_open;
                    g.bump();
                }
                m.touch();
            }
            Btn::Reset => home(m, e),
        }
        return true;
    }
    for e in &geo.eqs {
        if let (Some(rm), Some(id)) = (e.remove, e.id)
            && hit(&rm)
        {
            remove_layer(&mut app.model, &app.engine, id);
            return true;
        }
        if let (Some(fr), Some(id)) = (e.field, e.id)
            && hit(&fr)
        {
            place_caret(app, FieldId::Layer(id), e.text.as_ref().map(|t| &t.0), p.0 - (e.rect.x + 15.0), double);
            return true;
        }
    }
    if let Some((_, items)) = &geo.settings
        && let Some(it) = items.iter().find(|i| hit(&i.rect))
    {
        match it.act {
            Act::Check(k) => set_pref(app, |p| gs::toggle(p, k)),
            Act::Seg(k, i) => seg(app, k, i),
            Act::Field(id) => place_caret(app, id, it.text.as_ref().map(|t| &t.0), p.0 - (it.rect.x + 5.0), double),
            Act::ZoomSquare => zoom_square(&mut app.model, &app.engine),
            Act::None => {}
        }
        return true;
    }
    if let Some(chip) = geo.chips.iter().find(|c| hit(&c.rect)) {
        copy(&mut app.model, chip.copy);
        return true;
    }
    if geo.svg.is_some_and(|s| hit(&s)) {
        if double {
            home(&mut app.model, &app.engine);
            return true;
        }
        start_drag(app, x, y, true, key_down(VK_SHIFT) || key_down(VK_MENU));
    }
    true
}

/// A field takes the keys with the caret where it was clicked (a double-click selects it all).
fn place_caret(app: &mut App, id: FieldId, layout: Option<&IDWriteTextLayout>, dx: f32, double: bool) {
    let already = app.model.graph.as_ref().is_some_and(|g| g.focus == Some(id));
    if !already {
        focus(&mut app.model, &app.engine, id);
    }
    let Some(g) = &mut app.model.graph else { return };
    if double {
        g.edit.select_all();
    } else if let Some(l) = layout.filter(|_| already) {
        let pos = crate::render::hit(l, dx, 0.0);
        g.edit.set_caret_utf16(pos, false);
    }
    g.bump();
    app.model.touch();
}

fn seg(app: &mut App, k: Seg, i: usize) {
    match k {
        Seg::TextSize => set_pref(app, |p| p.large_text = i == 1),
        Seg::GridKind => set_pref(app, |p| p.polar = i == 1),
        Seg::Angles => set_pref(app, |p| p.degrees = Some(i == 1)),
        Seg::XScale => set_log(&mut app.model, &app.engine, Some(i == 1), None),
        Seg::YScale => set_log(&mut app.model, &app.engine, None, Some(i == 1)),
    }
}

/// A pref changed: kept in settings.json (`graphPrefs`), the plot drawn again, and sampled again when the angles
/// changed.
fn set_pref(app: &mut App, f: impl FnOnce(&mut GraphPrefs)) {
    let before = gs::prefs();
    let mut p = before;
    f(&mut p);
    if p == before {
        return;
    }
    gs::set_prefs(p);
    app.store.graph_prefs = serde_json::to_value(p).unwrap_or_default();
    app.save_store();
    if let Some(g) = &mut app.model.graph {
        g.bump();
    }
    app.model.touch();
    if p.degrees != before.degrees {
        sample(&app.model, &app.engine, false);
    }
}

fn copy(m: &mut BarModel, v: f64) {
    let text = coord(v).replace('−', "-");
    m.hint = Some(format!("copied {text}"));
    m.to_clipboard = Some(text);
    m.touch();
}

fn start_drag(app: &mut App, x: f32, y: f32, left: bool, pan: bool) {
    let Some(geo) = cached_geo(app) else { return };
    let Some(svg) = geo.svg else { return };
    let Some(g) = &mut app.model.graph else { return };
    let s = svg.w / SVG_W;
    let sh = svg_h(g);
    g.drag = Some(Drag {
        last: (x, y),
        plot: (PLOT_W * s, (sh - PAD_TOP - PAD_BOTTOM) * s),
        pan,
        moved: false,
        left,
    });
    // SAFETY: our bar
    unsafe { SetCapture(app.bar.hwnd) };
}

/// WM_RBUTTONDOWN / WM_MBUTTONDOWN: over the plot a right-drag orbits in 3D (pans in 2D) and a middle-drag pans;
/// either opens no menu. False elsewhere.
pub fn press(app: &mut App, x: f32, y: f32, middle: bool) -> bool {
    if to_svg(app, x, y).is_none() {
        return false;
    }
    commit(&mut app.model, &app.engine);
    start_drag(app, x, y, false, middle || key_down(VK_SHIFT) || key_down(VK_MENU));
    true
}

/// The pointer moved over the bar: drag (pan or orbit), or follow it with the readout and the buttons' hover.
pub fn mouse_move(app: &mut App, x: f32, y: f32) -> bool {
    let Some(g) = &app.model.graph else {
        return false;
    };
    if let Some(drag) = g.drag {
        let (dx, dy) = (x - drag.last.0, y - drag.last.1);
        if !drag.moved && dx.abs().max(dy.abs()) < 2.0 {
            return true;
        }
        let three = g.shows_3d();
        if let Some(g) = &mut app.model.graph {
            g.drag = Some(Drag { last: (x, y), moved: true, ..drag });
            g.pointer = to_svg_from(g, app.bar.layout.graph, x, y);
        }
        let (fx, fy) = (f64::from(dx / drag.plot.0), f64::from(dy / drag.plot.1));
        // a pan follows the pointer; an orbit turns against it side to side
        let fx = if three && !drag.pan { -fx } else { fx };
        pan(&mut app.model, &app.engine, fx, fy, drag.pan);
        return true;
    }
    let inside = panel(app).is_some_and(|r| r.contains(x, y));
    if !inside {
        return leave(app);
    }
    let r = panel(app).unwrap_or_default();
    let was = g.hover;
    if was.is_none() {
        track_leave(app);
    }
    let pointer = to_svg(app, x, y);
    if let Some(g) = &mut app.model.graph {
        g.hover = Some((x - r.x, y - r.y));
        g.pointer = pointer;
    }
    app.model.touch();
    match pointer {
        Some(p) => readout(app, p),
        None => {
            if let Some(g) = &mut app.model.graph {
                g.reading = None;
            }
            app.engine.cancel(Lane::GraphReadout);
        }
    }
    true
}

fn to_svg_from(g: &GraphState, panel: Option<Rect>, x: f32, y: f32) -> Option<P2> {
    let r = panel?;
    let svg = g.cache.borrow().geo.as_ref()?.1.svg?;
    let (px, py) = (x - r.x, y - r.y);
    let s = svg.w / SVG_W;
    svg.contains(px, py).then(|| ((px - svg.x) / s, (py - svg.y) / s))
}

fn track_leave(app: &App) {
    let mut tme = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: app.bar.hwnd,
        dwHoverTime: 0,
    };
    // SAFETY: a live local struct; our bar
    let _ = unsafe { TrackMouseEvent(&mut tme) };
}

/// The reading for the pointer at `p` (viewBox): what can be read at once is set now, the rest asked of the engine.
fn readout(app: &mut App, p: P2) {
    let Some(g) = &mut app.model.graph else { return };
    if g.main().is_none_or(|m| !m.ok()) || hovered_mark(g).is_some() {
        g.reading = None;
        app.engine.cancel(Lane::GraphReadout);
        return;
    }
    let inputs = |g: &GraphState| -> Vec<(usize, String)> {
        std::iter::once((0, g.input()))
            .chain(g.layers.iter().enumerate().filter_map(|(i, l)| {
                g.layer_plot(l).filter(|p| p.kind == Kind::Curve).map(|_| (i + 1, l.input()))
            }))
            .collect()
    };
    let window = (g.x_min, g.x_max);
    if g.shows_3d() {
        let Some(sc) = g.cache.borrow().scene.as_ref().map(|s| s.1.clone()) else {
            return;
        };
        let Some((cube_p, layer)) = graph3d::patch_hit(p, &sc.2.patches) else {
            g.reading = None;
            app.engine.cancel(Lane::GraphReadout);
            return;
        };
        let [x, y, z] = sc.0.data(cube_p);
        g.reading = Some(Reading::Surface { x, y, z, layer });
        let input = if layer == 0 {
            g.input()
        } else {
            g.layers.get(layer - 1).map_or_else(|| g.input(), |l| l.input())
        };
        ask(app, move |cx, settings| {
            let exact = cx.engine.graph_readout_z(input, x, y, options(window), settings);
            exact.filter(|z| z.is_finite()).map(|z| Reading::Surface { x, y, z, layer })
        });
        return;
    }
    let m = Map2D::new(g);
    if !Map2D::inside(p) {
        g.reading = None;
        return;
    }
    let res = (g.x_max - g.x_min) / f64::from(PLOT_W);
    let x = if m.log.0 { m.data_x(p.0) } else { round_to_pixel(m.data_x(p.0), res) };
    match g.kind() {
        Kind::Curve => {
            let ins = inputs(g);
            ask(app, move |cx, settings| {
                let ys = ins
                    .into_iter()
                    .filter_map(|(i, input)| {
                        let y = cx.engine.graph_readout_y(input, x, options(window), settings.clone())?;
                        y.is_finite().then_some((i, y))
                    })
                    .collect();
                Some(Reading::Curve { x, ys })
            });
        }
        kind => {
            let y = m.data_y(p.1);
            let y = if m.log.1 { y } else { round_to_pixel(y, (g.y_max - g.y_min) / f64::from(PLOT_H)) };
            g.reading = Some(Reading::Point { x, y, z: None });
            if kind == Kind::Surface {
                let input = g.input();
                ask(app, move |cx, settings| {
                    let z = cx.engine.graph_readout_z(input, x, y, options(window), settings);
                    Some(Reading::Point { x, y, z })
                });
            } else {
                app.engine.cancel(Lane::GraphReadout);
            }
        }
    }
}

fn options((lo, hi): (f64, f64)) -> GraphOptions {
    GraphOptions {
        domain_lo: Some(lo),
        domain_hi: Some(hi),
        sample_count: None,
        find_roots: false,
        aspect: Some(PLOT_ASPECT),
        y_center: None,
        surface: false,
        x_log: false,
    }
}

/// A readout from the engine, on its own lane (only the newest pointer position is answered).
fn ask(
    app: &App,
    f: impl FnOnce(&mut crate::engine::Cx, qcalc_ffi::EvalOptions) -> Option<Reading> + Send + 'static,
) {
    let m = &app.model;
    let Some(expr) = m.graph.as_ref().map(|g| g.expression.clone()) else {
        return;
    };
    let rows = m.rows.clone();
    let view = m.view(now_ms());
    let options = m.options();
    let degrees = gs::prefs().degrees;
    app.engine.submit(Lane::GraphReadout, move |cx| {
        let context = cx.context(&rows, view);
        let r = f(cx, graph_settings(merged_options(options, context), degrees));
        done(move |app| {
            if let Some(g) = &mut app.model.graph
                && g.expression == expr
                && g.pointer.is_some()
                && g.drag.is_none()
                && let Some(r) = r
            {
                g.reading = Some(r);
                app.model.touch();
            }
        })
    });
}

/// A press ended: a drag stops; a left click without one on a picked mark copies it. False without a drag.
pub fn mouse_up(app: &mut App, x: f32, y: f32) -> bool {
    let Some(drag) = app.model.graph.as_mut().and_then(|g| g.drag.take()) else {
        return false;
    };
    // SAFETY: we hold the capture
    let _ = unsafe { ReleaseCapture() };
    if !drag.moved
        && drag.left
        && let Some(g) = &app.model.graph
        && let Some(i) = hovered_mark(g)
        && let Some((_, c)) = g.all_critical().get(i)
    {
        let v = c.z.unwrap_or(if c.kind == qcalc_ffi::CriticalKind::YIntercept { c.y } else { c.x });
        copy(&mut app.model, v);
    }
    app.model.touch();
    mouse_move(app, x, y);
    true
}

/// The pointer left the bar (WM_MOUSELEAVE) or the panel.
pub fn leave(app: &mut App) -> bool {
    if BENCH_POINTER.with(|b| b.get()) {
        return false;
    }
    let Some(g) = &mut app.model.graph else {
        return false;
    };
    if g.hover.is_some() || g.reading.is_some() || g.pointer.is_some() {
        g.hover = None;
        g.pointer = None;
        g.reading = None;
        app.engine.cancel(Lane::GraphReadout);
        app.model.touch();
    }
    false
}

/// WM_MOUSEWHEEL / WM_MOUSEHWHEEL over the plot: in 3D up zooms in; in 2D up and down (and a touchpad's pinch,
/// which arrives with Ctrl) zoom about the pointer, sideways pans. False when the wheel isn't over the plot.
pub fn wheel(app: &mut App, horizontal: bool, wp: WPARAM, lp: LPARAM) -> bool {
    if app.model.graph.is_none() {
        return false;
    }
    let mut pt = POINT {
        x: i32::from((lp.0 & 0xffff) as u16 as i16),
        y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    };
    // SAFETY: a live point; our bar
    let _ = unsafe { ScreenToClient(app.bar.hwnd, &mut pt) };
    let s = app.bar.scale();
    let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
    let Some(v) = to_svg(app, x, y) else {
        return false;
    };
    let notches = f64::from(((wp.0 >> 16) & 0xffff) as u16 as i16) / 120.0;
    let ctrl = wp.0 & 0x0008 != 0;
    let Some(g) = &app.model.graph else { return false };
    if g.shows_3d() {
        if !horizontal {
            zoom(&mut app.model, &app.engine, (-notches * 0.2).clamp(-0.5, 0.5).exp(), 0.5);
        }
        return true;
    }
    // a notch (120) as a browser's 100 px of deltaY
    let px = -notches * 100.0;
    if horizontal {
        pan(&mut app.model, &app.engine, -px / f64::from(PLOT_W), 0.0, false);
        return true;
    }
    let k = if ctrl { 0.01 } else { 0.002 };
    let factor = (px * k).clamp(-0.5, 0.5).exp();
    let at = f64::from((v.0 - PAD_LEFT) / PLOT_W);
    zoom(&mut app.model, &app.engine, factor, at);
    true
}

/// The cursor over the panel: a hand on a button, chip or mark, an I-beam on a field, a move cursor on the plot.
pub fn cursor(app: &App, x: f32, y: f32) -> Option<PCWSTR> {
    let r = panel(app)?;
    if !r.contains(x, y) {
        return None;
    }
    let g = app.model.graph.as_ref()?;
    let geo = cached_geo(app)?;
    let p = (x - r.x, y - r.y);
    let hit = |rect: &Rect| rect.contains(p.0, p.1);
    if let Some((_, items)) = &geo.settings
        && let Some(it) = items.iter().find(|i| hit(&i.rect))
    {
        return Some(match it.act {
            Act::Field(_) => IDC_IBEAM,
            Act::None => IDC_ARROW,
            _ => IDC_HAND,
        });
    }
    if geo.eqs.iter().any(|e| e.field.is_some_and(|f| hit(&f))) {
        return Some(IDC_IBEAM);
    }
    if geo.head.iter().any(|h| h.enabled && hit(&h.rect))
        || geo.chips.iter().any(|c| hit(&c.rect))
        || geo.eqs.iter().any(|e| e.remove.is_some_and(|f| hit(&f)))
    {
        return Some(IDC_HAND);
    }
    if geo.svg.is_some_and(|s| hit(&s)) {
        if g.drag.is_none() && hovered_mark(g).is_some() {
            return Some(IDC_HAND);
        }
        let flat = g.kind() == Kind::Curve && g.fixed_y.is_none() && !g.shows_3d();
        return Some(if flat { IDC_SIZEWE } else { IDC_SIZEALL });
    }
    Some(IDC_ARROW)
}

/// What only a showing needs goes while the bar is hidden: the base pass's bitmap (it holds the device).
pub fn release(app: &mut App) {
    if let Some(g) = &app.model.graph {
        g.cache.borrow_mut().base = None;
    }
}

// ---- the bench pipe ----

/// `graph …` on the bench pipe (bench.rs `extra`), for driving the panel in tests. Points are fractions of the
/// plot's svg (`hover 0.5 0.5`) or panel DIPs for `at`:
/// `rects`, `hover fx fy`, `click fx fy`, `dblclick fx fy`, `drag fx0 fy0 fx1 fy1 [left|right|middle] [shift]`,
/// `wheel fx fy notches`, `at x y` (a left click at panel DIPs), `button 3d|contour|add|wrench|reset`, `leave`.
pub fn bench(app: &App, args: &str) -> serde_json::Value {
    let words: Vec<&str> = args.split_whitespace().collect();
    let Some(&cmd) = words.first() else {
        return serde_json::json!({ "error": "graph what?" });
    };
    let nums: Vec<f32> = words[1..].iter().filter_map(|w| w.parse().ok()).collect();
    let (Some(r), Some(geo)) = (panel(app), cached_geo(app)) else {
        return serde_json::json!({ "error": "no graph" });
    };
    let svg = geo.svg.unwrap_or_default();
    let at = move |fx: f32, fy: f32| (r.x + svg.x + svg.w * fx, r.y + svg.y + svg.h * fy);
    if cmd == "rects" {
        let rect = |q: &Rect| [q.x, q.y, q.w, q.h];
        return serde_json::json!({
            "panel": rect(&r), "svg": geo.svg.map(|s| rect(&s)),
            "head": geo.head.iter().map(|h| serde_json::json!({ "btn": format!("{:?}", h.btn), "rect": rect(&h.rect), "on": h.on, "enabled": h.enabled })).collect::<Vec<_>>(),
            "eqs": geo.eqs.iter().map(|e| serde_json::json!({ "index": e.index, "rect": rect(&e.rect), "field": e.field.map(|f| rect(&f)), "remove": e.remove.map(|f| rect(&f)) })).collect::<Vec<_>>(),
            "settings": geo.settings.as_ref().map(|(b, items)| serde_json::json!({ "box": rect(b), "items": items.iter().map(|i| serde_json::json!({ "act": format!("{:?}", i.act), "look": format!("{:?}", i.look), "rect": rect(&i.rect) })).collect::<Vec<_>>() })),
            "chips": geo.chips.iter().map(|c| serde_json::json!({ "index": c.index, "rect": rect(&c.rect), "copy": c.copy })).collect::<Vec<_>>(),
        });
    }
    let arg = |i: usize| nums.get(i).copied().unwrap_or(0.5);
    let button = words.get(1).map(|w| w.to_ascii_lowercase()).unwrap_or_default();
    let head = geo
        .head
        .iter()
        .find(|h| format!("{:?}", h.btn).to_ascii_lowercase() == button)
        .map(|h| (r.x + h.rect.x + h.rect.w / 2.0, r.y + h.rect.y + h.rect.h / 2.0));
    let cmd = cmd.to_string();
    let rest: Vec<String> = words[1..].iter().map(|s| s.to_string()).collect();
    let (a0, a1, a2, a3) = (arg(0), arg(1), arg(2), arg(3));
    crate::bg::post(move |app| match cmd.as_str() {
        "hover" => {
            BENCH_POINTER.with(|b| b.set(true));
            let (x, y) = at(a0, a1);
            mouse_move(app, x, y);
        }
        "click" | "dblclick" => {
            let (x, y) = at(a0, a1);
            click(app, x, y, cmd == "dblclick");
            mouse_up(app, x, y);
        }
        "at" => {
            let (x, y) = (r.x + a0, r.y + a1);
            click(app, x, y, false);
            mouse_up(app, x, y);
        }
        "button" => {
            if let Some((x, y)) = head {
                click(app, x, y, false);
            }
        }
        "wheel" => {
            let (x, y) = at(a0, a1);
            let s = app.bar.scale();
            let mut pt = POINT { x: (x * s) as i32, y: (y * s) as i32 };
            // SAFETY: a live point; our bar
            let _ = unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(app.bar.hwnd, &mut pt) };
            let delta = (a2 * 120.0) as i16 as u16 as usize;
            let lp = LPARAM(((pt.y as u16 as isize) << 16) | (pt.x as u16 as isize));
            wheel(app, false, WPARAM(delta << 16), lp);
        }
        "drag" => {
            let (x0, y0) = at(a0, a1);
            let (x1, y1) = at(a2, a3);
            let shift = rest.iter().any(|w| w == "shift");
            let which = rest.iter().find(|w| ["left", "right", "middle"].contains(&w.as_str()));
            match which.map(String::as_str) {
                Some("right") => {
                    press(app, x0, y0, false);
                }
                Some("middle") => {
                    press(app, x0, y0, true);
                }
                _ => {
                    click(app, x0, y0, false);
                }
            }
            if shift && let Some(d) = app.model.graph.as_mut().and_then(|g| g.drag.as_mut()) {
                d.pan = true;
            }
            for k in 1..=8 {
                let t = k as f32 / 8.0;
                mouse_move(app, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            }
            mouse_up(app, x1, y1);
        }
        "leave" => {
            BENCH_POINTER.with(|b| b.set(false));
            leave(app);
        }
        _ => {}
    });
    serde_json::json!({ "ok": true })
}
