//! The game window's drawing: its own Direct2D factory and render target (hardware, since games animate; made
//! when the window opens and dropped with it, so nothing of the GPU's stays once the game is closed), DirectWrite
//! formats and layouts cached for the window's life, and `Ui`, what a frame paints with: shapes, svg polylines,
//! text, and the hit regions clicks and hover are matched against (an immediate-mode UI: each frame lays itself out
//! and paints, and the next click is tested against what that frame drew).

use std::collections::HashMap;

use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND,
    D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_GAMMA_2_2, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_LINE_JOIN_ROUND, D2D1_PRESENT_OPTIONS_IMMEDIATELY, D2D1_QUADRATIC_BEZIER_SEGMENT,
    D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_REGULAR, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteFontCollection, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{BOOL, HSTRING, Result};
use windows_numerics::{Matrix3x2, Vector2};

use super::svg::{Poly, Pt};
use super::theme::{Colors, Rgba, m};
pub use crate::render::Rect;

/// Which face: the UI's text face, its display face (big numbers), or the chalk (ultimate's slate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Text,
    Display,
    Chalk,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub size: f32,
    pub bold: bool,
    pub face: Face,
}

impl Font {
    pub const fn new(size: f32) -> Font {
        Font {
            size,
            bold: false,
            face: Face::Text,
        }
    }
    pub const fn bold(size: f32) -> Font {
        Font {
            size,
            bold: true,
            face: Face::Text,
        }
    }
    pub const fn digits(size: f32, bold: bool) -> Font {
        Font {
            size,
            bold,
            face: Face::Display,
        }
    }
    pub const fn chalk(size: f32) -> Font {
        Font {
            size,
            bold: false,
            face: Face::Chalk,
        }
    }
    pub const BODY: Font = Font::new(m::FS_BODY);
    pub const SMALL: Font = Font::new(m::FS_SMALL);
    pub const CAPTION: Font = Font::new(m::FS_CAPTION);

    fn key(&self) -> (u32, bool, Face) {
        ((self.size * 8.0) as u32, self.bold, self.face)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// What a click on a region does; the topmost region drawn wins.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    /// as if this key were pressed (the lobby's buttons are its keys)
    Key(String),
    /// the shell's esc (`shell.escape()`)
    Esc,
    Fullscreen,
    Resume,
    /// a lobby room by its place in the list
    Join(usize),
    /// a game on this computer, by option
    Local(usize),
    /// the open game's own button
    Act(u32),
    /// the open game's board: it gets the point in the region's own coordinates
    Board(u32),
}

/// What a frame left behind: the click regions, and whether something on it still moves.
#[derive(Default)]
pub struct Frame {
    pub hits: Vec<(Rect, Hit)>,
    pub next: bool,
}

type LayoutKey = (String, (u32, bool, Face), u32, Align, bool);

struct Target {
    rt: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
}

/// The window's drawing state, made with the window and dropped with it.
pub struct Gfx {
    hwnd: HWND,
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    target: Option<Target>,
    round: Option<ID2D1StrokeStyle>,
    flat: Option<ID2D1StrokeStyle>,
    dashes: HashMap<u32, ID2D1StrokeStyle>,
    formats: HashMap<(u32, bool, Face), IDWriteTextFormat>,
    layouts: HashMap<LayoutKey, IDWriteTextLayout>,
    text_family: String,
    display_family: String,
    chalk_family: String,
    pub dpi: f32,
    pub px: (u32, u32),
    pub hardware: bool,
}

fn has_family(fonts: &IDWriteFontCollection, name: &str) -> bool {
    let (mut index, mut exists) = (0u32, BOOL(0));
    // SAFETY: out pointers are live locals
    unsafe { fonts.FindFamilyName(&HSTRING::from(name), &mut index, &mut exists) }.is_ok()
        && exists.as_bool()
}

fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

fn v(p: Pt) -> Vector2 {
    Vector2 { X: p.0, Y: p.1 }
}

impl Gfx {
    pub fn new(hwnd: HWND, dpi: u32) -> Result<Gfx> {
        // SAFETY: plain factory creation; the options outlive the call
        let d2d: ID2D1Factory = unsafe {
            D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS::default()),
            )
        }?;
        // SAFETY: as above
        let dw: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        let mut collection = None;
        // SAFETY: out pointer is a live local
        unsafe { dw.GetSystemFontCollection(&mut collection, false) }?;
        let found = |name: &str| collection.as_ref().is_some_and(|c| has_family(c, name));
        let pick = |names: &[&str]| {
            names
                .iter()
                .find(|n| found(n))
                .map_or("Segoe UI".to_string(), |n| n.to_string())
        };
        Ok(Gfx {
            hwnd,
            text_family: pick(&["Segoe UI Variable Text", "Segoe UI"]),
            display_family: pick(&["Segoe UI Variable Display", "Segoe UI"]),
            chalk_family: pick(&["Segoe Print", "Ink Free", "Segoe UI"]),
            d2d,
            dw,
            target: None,
            round: None,
            flat: None,
            dashes: HashMap::new(),
            formats: HashMap::new(),
            layouts: HashMap::new(),
            dpi: dpi as f32,
            px: (1, 1),
            hardware: true,
        })
    }

    pub fn scale(&self) -> f32 {
        self.dpi / 96.0
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.px = (w.max(1), h.max(1));
        if let Some(t) = &self.target {
            let size = D2D_SIZE_U {
                width: self.px.0,
                height: self.px.1,
            };
            // SAFETY: a live target
            if unsafe { t.rt.Resize(&size) }.is_err() {
                self.target = None;
            }
        }
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi as f32;
        if let Some(t) = &self.target {
            // SAFETY: a live target
            unsafe { t.rt.SetDpi(self.dpi, self.dpi) };
        }
    }

    fn ensure(&mut self) -> Result<()> {
        if self.target.is_some() {
            return Ok(());
        }
        let make = |hardware: bool| {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: if hardware {
                    D2D1_RENDER_TARGET_TYPE_DEFAULT
                } else {
                    D2D1_RENDER_TARGET_TYPE_SOFTWARE
                },
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: self.dpi,
                dpiY: self.dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: D2D_SIZE_U {
                    width: self.px.0,
                    height: self.px.1,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
            };
            // SAFETY: the property structs outlive the call; the window is ours and live
            unsafe { self.d2d.CreateHwndRenderTarget(&props, &hwnd_props) }
        };
        // the GPU when there is one (QCALC_D2D=software tries without)
        let want_hw = !std::env::var("QCALC_D2D").is_ok_and(|v| v.eq_ignore_ascii_case("software"));
        let (rt, hardware) = match want_hw.then(|| make(true)) {
            Some(Ok(rt)) => (rt, true),
            _ => (make(false)?, false),
        };
        self.hardware = hardware;
        // SAFETY: a live target
        let brush = unsafe {
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.CreateSolidColorBrush(&color(Rgba::CLEAR), None)
        }?;
        self.target = Some(Target { rt, brush });
        Ok(())
    }

    fn stroke(&self, cap: bool, dashes: Option<&[f32]>) -> Result<ID2D1StrokeStyle> {
        let c = if cap {
            D2D1_CAP_STYLE_ROUND
        } else {
            D2D1_CAP_STYLE_FLAT
        };
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: c,
            endCap: c,
            dashCap: c,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: if dashes.is_some() {
                D2D1_DASH_STYLE_CUSTOM
            } else {
                D2D1_DASH_STYLE_SOLID
            },
            dashOffset: 0.0,
        };
        // SAFETY: the props outlive the call
        unsafe { self.d2d.CreateStrokeStyle(&props, dashes) }
    }

    /// One frame. A lost device drops the target; the next frame makes a new one.
    pub fn draw(
        &mut self,
        colors: Colors,
        mouse: Option<(f32, f32)>,
        t: f64,
        paint: impl FnOnce(&mut Ui),
    ) -> Result<Frame> {
        self.ensure()?;
        if self.round.is_none() {
            self.round = self.stroke(true, None).ok();
            self.flat = self.stroke(false, None).ok();
        }
        // a few hundred strings at most; a long session's churn is dropped wholesale
        if self.layouts.len() > 600 {
            self.layouts.clear();
        }
        let scale = self.scale();
        let Some(tg) = &self.target else {
            return Ok(Frame::default());
        };
        let mut ui = Ui {
            rt: &tg.rt,
            brush: &tg.brush,
            d2d: &self.d2d,
            dw: &self.dw,
            round_style: self.round.as_ref(),
            flat_style: self.flat.as_ref(),
            dashes: &mut self.dashes,
            formats: &mut self.formats,
            layouts: &mut self.layouts,
            families: [&self.text_family, &self.display_family, &self.chalk_family],
            c: colors,
            w: self.px.0 as f32 / scale,
            h: self.px.1 as f32 / scale,
            mouse,
            t,
            px_per_dip: scale,
            hits: Vec::new(),
            alpha: 1.0,
            transforms: vec![Matrix3x2::identity()],
            next_frame: false,
        };
        // SAFETY: paired Begin/EndDraw on a live target
        unsafe { tg.rt.BeginDraw() };
        paint(&mut ui);
        let hits = std::mem::take(&mut ui.hits);
        let next = ui.next_frame;
        drop(ui);
        // SAFETY: as above
        let end = unsafe { tg.rt.EndDraw(None, None) };
        if let Err(e) = &end
            && e.code() == D2DERR_RECREATE_TARGET
        {
            self.target = None;
            return Ok(Frame { hits, next: true });
        }
        end?;
        Ok(Frame { hits, next })
    }
}

/// What a frame paints with. Coordinates are DIPs (the target's DPI is the monitor's).
pub struct Ui<'a> {
    rt: &'a ID2D1HwndRenderTarget,
    brush: &'a ID2D1SolidColorBrush,
    d2d: &'a ID2D1Factory,
    dw: &'a IDWriteFactory,
    round_style: Option<&'a ID2D1StrokeStyle>,
    flat_style: Option<&'a ID2D1StrokeStyle>,
    dashes: &'a mut HashMap<u32, ID2D1StrokeStyle>,
    formats: &'a mut HashMap<(u32, bool, Face), IDWriteTextFormat>,
    layouts: &'a mut HashMap<LayoutKey, IDWriteTextLayout>,
    families: [&'a str; 3],
    pub c: Colors,
    /// the window in DIPs
    pub w: f32,
    pub h: f32,
    /// the pointer, in window DIPs
    pub mouse: Option<(f32, f32)>,
    /// real seconds since the window opened, for animations
    pub t: f64,
    /// device pixels per DIP
    pub px_per_dip: f32,
    hits: Vec<(Rect, Hit)>,
    alpha: f32,
    transforms: Vec<Matrix3x2>,
    next_frame: bool,
}

impl Ui<'_> {
    // ---- state

    /// Something on screen moves: draw another frame after this one.
    pub fn animate(&mut self) {
        self.next_frame = true;
    }

    /// Everything after this is drawn at `a` of its opacity (multiplied into what's there).
    pub fn set_alpha(&mut self, a: f32) -> f32 {
        std::mem::replace(&mut self.alpha, a)
    }

    fn paint(&self, c: Rgba) -> &ID2D1SolidColorBrush {
        let c = c.opacity(self.alpha);
        // SAFETY: a live brush
        unsafe { self.brush.SetColor(&color(c)) };
        self.brush
    }

    fn top(&self) -> Matrix3x2 {
        *self.transforms.last().unwrap_or(&Matrix3x2::identity())
    }

    /// Scales by k and moves by (dx, dy) in the current space: what follows draws in its own units.
    pub fn push_transform(&mut self, k: f32, dx: f32, dy: f32) {
        let local = Matrix3x2 {
            M11: k,
            M12: 0.0,
            M21: 0.0,
            M22: k,
            M31: dx,
            M32: dy,
        };
        let next = local * self.top();
        self.transforms.push(next);
        // SAFETY: inside a draw
        unsafe { self.rt.SetTransform(&next) };
    }

    /// Rotates by `deg` about (cx, cy) in the current space.
    pub fn push_rotation(&mut self, deg: f32, cx: f32, cy: f32) {
        let (s, c) = deg.to_radians().sin_cos();
        let local = Matrix3x2 {
            M11: c,
            M12: s,
            M21: -s,
            M22: c,
            M31: cx - cx * c + cy * s,
            M32: cy - cx * s - cy * c,
        };
        let next = local * self.top();
        self.transforms.push(next);
        // SAFETY: inside a draw
        unsafe { self.rt.SetTransform(&next) };
    }

    pub fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
        let t = self.top();
        // SAFETY: inside a draw
        unsafe { self.rt.SetTransform(&t) };
    }

    /// Maps a point in the current space to the window's (for hit regions drawn under a transform).
    fn to_window(&self, x: f32, y: f32) -> (f32, f32) {
        let t = self.top();
        (x * t.M11 + y * t.M21 + t.M31, x * t.M12 + y * t.M22 + t.M32)
    }

    pub fn clip(&self, r: Rect) {
        // SAFETY: paired with unclip by the caller, inside a draw
        unsafe {
            self.rt
                .PushAxisAlignedClip(&d2d_rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE)
        };
    }

    pub fn unclip(&self) {
        // SAFETY: after clip
        unsafe { self.rt.PopAxisAlignedClip() };
    }

    // ---- hits and hover

    /// Clicks in `r` (current space) do `hit`.
    pub fn hit(&mut self, r: Rect, hit: Hit) {
        let (x0, y0) = self.to_window(r.x, r.y);
        let (x1, y1) = self.to_window(r.right(), r.bottom());
        self.hits.push((Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()), hit));
    }

    /// Whether the pointer is over `r` (current space).
    pub fn hover(&self, r: Rect) -> bool {
        let Some((mx, my)) = self.mouse else {
            return false;
        };
        let (x0, y0) = self.to_window(r.x, r.y);
        let (x1, y1) = self.to_window(r.right(), r.bottom());
        mx >= x0.min(x1) && mx < x0.max(x1) && my >= y0.min(y1) && my < y0.max(y1)
    }

    /// The pointer in the current space's units, if it's in `r`.
    pub fn pointer_in(&self, r: Rect) -> Option<(f32, f32)> {
        let (mx, my) = self.mouse?;
        let t = self.top();
        let det = t.M11 * t.M22 - t.M12 * t.M21;
        if det.abs() < 1e-9 {
            return None;
        }
        let (x, y) = (mx - t.M31, my - t.M32);
        let lx = (x * t.M22 - y * t.M21) / det;
        let ly = (y * t.M11 - x * t.M12) / det;
        r.contains(lx, ly).then_some((lx, ly))
    }

    // ---- shapes

    pub fn clear(&self, c: Rgba) {
        // SAFETY: inside a draw
        unsafe { self.rt.Clear(Some(&color(c))) };
    }

    pub fn fill(&self, r: Rect, c: Rgba) {
        let b = self.paint(c);
        // SAFETY: inside a draw
        unsafe { self.rt.FillRectangle(&d2d_rect(r), b) };
    }

    pub fn round(&self, r: Rect, radius: f32, c: Rgba) {
        let b = self.paint(c);
        let rr = D2D1_ROUNDED_RECT {
            rect: d2d_rect(r),
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: inside a draw
        unsafe { self.rt.FillRoundedRectangle(&rr, b) };
    }

    /// A ring inside `r` (`strokeBorder`, an inset box-shadow).
    pub fn ring(&self, r: Rect, radius: f32, width: f32, c: Rgba) {
        let b = self.paint(c);
        let r = r.inset(width / 2.0, width / 2.0);
        let rr = D2D1_ROUNDED_RECT {
            rect: d2d_rect(r),
            radiusX: (radius - width / 2.0).max(0.0),
            radiusY: (radius - width / 2.0).max(0.0),
        };
        // SAFETY: inside a draw
        unsafe { self.rt.DrawRoundedRectangle(&rr, b, width, None) };
    }

    /// A stroke centred on `r`'s edge.
    pub fn outline(&self, r: Rect, radius: f32, width: f32, c: Rgba) {
        let b = self.paint(c);
        let rr = D2D1_ROUNDED_RECT {
            rect: d2d_rect(r),
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: inside a draw
        unsafe { self.rt.DrawRoundedRectangle(&rr, b, width, None) };
    }

    pub fn circle(&self, cx: f32, cy: f32, r: f32, c: Rgba) {
        self.ellipse(cx, cy, r, r, c);
    }

    pub fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgba) {
        let b = self.paint(c);
        let e = D2D1_ELLIPSE {
            point: Vector2 { X: cx, Y: cy },
            radiusX: rx,
            radiusY: ry,
        };
        // SAFETY: inside a draw
        unsafe { self.rt.FillEllipse(&e, b) };
    }

    pub fn circle_stroke(&self, cx: f32, cy: f32, r: f32, width: f32, c: Rgba) {
        let b = self.paint(c);
        let e = D2D1_ELLIPSE {
            point: Vector2 { X: cx, Y: cy },
            radiusX: r,
            radiusY: r,
        };
        // SAFETY: inside a draw
        unsafe { self.rt.DrawEllipse(&e, b, width, None) };
    }

    pub fn line(&self, a: Pt, b: Pt, width: f32, c: Rgba, round: bool) {
        let brush = self.paint(c);
        let style = if round { self.round_style } else { self.flat_style };
        // SAFETY: inside a draw
        unsafe { self.rt.DrawLine(v(a), v(b), brush, width, style) };
    }

    /// Strokes a polyline path (round caps and joins).
    pub fn poly(&self, p: &Poly, width: f32, c: Rgba) {
        let Ok(geo) = (unsafe { self.d2d.CreatePathGeometry() }) else {
            return;
        };
        // SAFETY: a fresh geometry, opened once and closed
        let ok = unsafe {
            geo.Open().and_then(|sink| {
                for part in &p.parts {
                    if part.len() < 2 {
                        continue;
                    }
                    sink.BeginFigure(v(part[0]), D2D1_FIGURE_BEGIN_HOLLOW);
                    let rest: Vec<Vector2> = part[1..].iter().map(|&q| v(q)).collect();
                    sink.AddLines(&rest);
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                }
                sink.Close()
            })
        };
        if ok.is_err() {
            return;
        }
        let b = self.paint(c);
        // SAFETY: inside a draw
        unsafe { self.rt.DrawGeometry(&geo, b, width, self.round_style) };
    }

    /// A quadratic curve, dashed (`dash` and `gap` in DIPs) with round caps.
    #[allow(clippy::too_many_arguments)]
    pub fn dashed_curve(&mut self, a: Pt, ctrl: Pt, b: Pt, width: f32, dash: f32, gap: f32, c: Rgba) {
        let key = ((dash * 100.0) as u32) << 16 | (gap * 100.0) as u32 ^ ((width * 10.0) as u32) << 24;
        if !self.dashes.contains_key(&key) {
            // custom dashes are in stroke widths
            let d = [dash / width, gap / width];
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_CUSTOM,
                dashOffset: 0.0,
            };
            // SAFETY: the props and dashes outlive the call
            if let Ok(s) = unsafe { self.d2d.CreateStrokeStyle(&props, Some(&d)) } {
                self.dashes.insert(key, s);
            }
        }
        let Some(style) = self.dashes.get(&key) else {
            return;
        };
        let Ok(geo) = (unsafe { self.d2d.CreatePathGeometry() }) else {
            return;
        };
        // SAFETY: a fresh geometry, opened once and closed
        let ok = unsafe {
            geo.Open().and_then(|sink| {
                sink.BeginFigure(v(a), D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: v(ctrl),
                    point2: v(b),
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()
            })
        };
        if ok.is_err() {
            return;
        }
        let brush = self.paint(c);
        // SAFETY: inside a draw
        unsafe { self.rt.DrawGeometry(&geo, brush, width, style) };
    }

    /// An ellipse filled with a radial gradient from `inner` at its centre to clear at `stop` of its radius.
    pub fn glow(&self, cx: f32, cy: f32, rx: f32, ry: f32, inner: Rgba, stop: f32) {
        let a = self.alpha;
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: color(inner.opacity(a)),
            },
            D2D1_GRADIENT_STOP {
                position: stop,
                color: color(inner.opacity(0.0)),
            },
        ];
        // SAFETY: the stops outlive the call; plain resource creation on a live target
        let Ok(coll) = (unsafe { self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) })
        else {
            return;
        };
        let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
            center: Vector2 { X: cx, Y: cy },
            gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
            radiusX: rx,
            radiusY: ry,
        };
        // SAFETY: as above
        let Ok(brush) = (unsafe { self.rt.CreateRadialGradientBrush(&props, None, &coll) }) else {
            return;
        };
        let e = D2D1_ELLIPSE {
            point: Vector2 { X: cx, Y: cy },
            radiusX: rx,
            radiusY: ry,
        };
        // SAFETY: inside a draw
        unsafe { self.rt.FillEllipse(&e, &brush) };
    }

    // ---- the CSS pieces

    /// --control-shadow under a control (a hairline ring and a 1 px drop).
    pub fn control_shadow(&self, r: Rect, radius: f32) {
        self.round(Rect::new(r.x, r.y + 1.0, r.w, r.h), radius, self.c.control_drop());
        self.outline(r.inset(-0.25, -0.25), radius + 0.25, 0.5, self.c.control_ring());
    }

    /// --shadow-card under a floating card or pill.
    pub fn card_shadow(&self, r: Rect, radius: f32) {
        for (i, k) in [(3.0, 0.35), (2.0, 0.6), (1.0, 1.0)] {
            let grow = i * 0.6;
            self.round(
                Rect::new(r.x - grow, r.y + 1.0 - grow + i * 0.3, r.w + 2.0 * grow, r.h + 2.0 * grow),
                radius + grow,
                self.c.card_drop().opacity(k * 0.6),
            );
        }
        self.outline(r.inset(-0.25, -0.25), radius + 0.25, 0.5, self.c.card_ring());
    }

    // ---- text

    fn format(&mut self, f: Font) -> Option<IDWriteTextFormat> {
        if let Some(fmt) = self.formats.get(&f.key()) {
            return Some(fmt.clone());
        }
        let family = match f.face {
            Face::Text => self.families[0],
            Face::Display => self.families[1],
            Face::Chalk => self.families[2],
        };
        let weight = if f.bold {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_REGULAR
        };
        // SAFETY: the strings outlive the call
        let fmt = unsafe {
            self.dw.CreateTextFormat(
                &HSTRING::from(family),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                f.size,
                &HSTRING::from("en-us"),
            )
        }
        .ok()?;
        self.formats.insert(f.key(), fmt.clone());
        Some(fmt)
    }

    /// A cached layout: one line (`wrap` 0) or wrapped to `wrap` DIPs, aligned in that width.
    fn layout(&mut self, text: &str, f: Font, wrap: f32, align: Align, trim: bool) -> Option<IDWriteTextLayout> {
        let key: LayoutKey = (text.to_string(), f.key(), (wrap * 4.0) as u32, align, trim);
        if let Some(l) = self.layouts.get(&key) {
            return Some(l.clone());
        }
        let fmt = self.format(f)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let w = if wrap > 0.0 { wrap } else { 100_000.0 };
        // SAFETY: wide outlives the call; the format is live
        let layout = unsafe { self.dw.CreateTextLayout(&wide, &fmt, w, 10_000.0) }.ok()?;
        // SAFETY: setters on a live layout
        unsafe {
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            let _ = layout.SetWordWrapping(if wrap > 0.0 && !trim {
                DWRITE_WORD_WRAPPING_WRAP
            } else {
                DWRITE_WORD_WRAPPING_NO_WRAP
            });
            if wrap > 0.0 {
                let _ = layout.SetTextAlignment(match align {
                    Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                    Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                });
            }
            if trim {
                let t = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                if let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(&fmt) {
                    let _ = layout.SetTrimming(&t, &sign);
                }
            }
        }
        self.layouts.insert(key, layout.clone());
        Some(layout)
    }

    fn metrics(l: &IDWriteTextLayout) -> (f32, f32) {
        let mut m = DWRITE_TEXT_METRICS::default();
        // SAFETY: out pointer is a live local
        if unsafe { l.GetMetrics(&mut m) }.is_err() {
            return (0.0, 0.0);
        }
        (m.widthIncludingTrailingWhitespace, m.height)
    }

    /// One line's width and height.
    pub fn measure(&mut self, text: &str, f: Font) -> (f32, f32) {
        self.layout(text, f, 0.0, Align::Left, false)
            .map_or((0.0, f.size * 1.3), |l| Self::metrics(&l))
    }

    fn draw_layout(&self, l: &IDWriteTextLayout, x: f32, y: f32, c: Rgba) {
        let b = self.paint(c);
        // SAFETY: inside a draw
        unsafe {
            self.rt.DrawTextLayout(
                Vector2 { X: x, Y: y },
                l,
                b,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            )
        };
    }

    /// One line with its top-left at (x, y); returns its width.
    pub fn text(&mut self, text: &str, f: Font, x: f32, y: f32, c: Rgba) -> f32 {
        let Some(l) = self.layout(text, f, 0.0, Align::Left, false) else {
            return 0.0;
        };
        let (w, _) = Self::metrics(&l);
        self.draw_layout(&l, x, y, c);
        w
    }

    /// One line centred on x.
    pub fn text_center(&mut self, text: &str, f: Font, cx: f32, y: f32, c: Rgba) -> f32 {
        let (w, _) = self.measure(text, f);
        self.text(text, f, cx - w / 2.0, y, c);
        w
    }

    /// One line in a box of width `w`, aligned, cut with an ellipsis when it doesn't fit.
    #[allow(clippy::too_many_arguments)]
    pub fn text_in(&mut self, text: &str, f: Font, x: f32, y: f32, w: f32, align: Align, c: Rgba) {
        if let Some(l) = self.layout(text, f, w.max(1.0), align, true) {
            self.draw_layout(&l, x, y, c);
        }
    }

    /// Wrapped to `w`; returns the height it took.
    #[allow(clippy::too_many_arguments)]
    pub fn text_wrap(&mut self, text: &str, f: Font, x: f32, y: f32, w: f32, align: Align, c: Rgba) -> f32 {
        let Some(l) = self.layout(text, f, w.max(1.0), align, false) else {
            return 0.0;
        };
        let (_, h) = Self::metrics(&l);
        self.draw_layout(&l, x, y, c);
        h
    }

    /// The height `text_wrap` would take.
    pub fn wrap_height(&mut self, text: &str, f: Font, w: f32) -> f32 {
        self.layout(text, f, w.max(1.0), Align::Center, false)
            .map_or(f.size * 1.3, |l| Self::metrics(&l).1)
    }

    /// The line height of a font (one line's layout height).
    pub fn line_h(&mut self, f: Font) -> f32 {
        self.measure("Ag", f).1
    }
}
