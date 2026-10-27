//! The web canvas's 2D context over Direct2D (the Mac's Canvas2D.swift over Core Graphics), so ember's and trails'
//! drawing ports line for line. The same rules as the web: the current path is kept in device space as it's built
//! (each point through the transform of the moment), fills and strokes don't consume it, line width, dashes and
//! gradients follow the transform at the time of the fill or stroke.
//!
//! Coordinates are canvas pixels, y down, from the top-left of the canvas: one canvas pixel is one device pixel.
//! A canvas draws onto the game window's target (`Canvas::on`, inside the frame), or into an `Offscreen` of its own
//! (a bitmap render target compatible with the window's, drawImage-able).
//!
//! Composites: source-over; lighter (the device context's additive primitive blend); destination-out (the run of
//! primitives is recorded into a command list and drawn once with the destination-out composite, which is the same
//! as drawing each: dst·(1−a₁)(1−a₂)… either way). Shadows (trails' glow) are a soft halo of widening strokes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::rc::Rc;

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F,
    D2D1_COMPOSITE_MODE_DESTINATION_OUT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN, D2D1_FILL_MODE_ALTERNATE, D2D1_FILL_MODE_WINDING, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_BRUSH_PROPERTIES, D2D1_CAP_STYLE, D2D1_CAP_STYLE_FLAT,
    D2D1_CAP_STYLE_ROUND, D2D1_CAP_STYLE_SQUARE, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE, D2D1_DASH_STYLE_CUSTOM,
    D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2,
    D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_LINE_JOIN,
    D2D1_LINE_JOIN_MITER, D2D1_LINE_JOIN_ROUND, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_PRIMITIVE_BLEND_ADD,
    D2D1_PRIMITIVE_BLEND_SOURCE_OVER, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_STROKE_STYLE_PROPERTIES,
    ID2D1Bitmap, ID2D1BitmapRenderTarget, ID2D1Brush, ID2D1CommandList, ID2D1DeviceContext, ID2D1Factory,
    ID2D1Geometry, ID2D1Image, ID2D1PathGeometry, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_METRICS, DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP,
    IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{HSTRING, Interface};
use windows_numerics::{Matrix3x2, Vector2};

use super::gfx::{Rect, Ui};

// ---- colours

/// A css colour: channels 0–255, alpha 0–1 (Col in Canvas2D.swift).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Col {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Col {
    pub const fn rgb(r: f64, g: f64, b: f64) -> Col {
        Col { r, g, b, a: 1.0 }
    }
    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Col {
        Col { r, g, b, a }
    }
    pub const CLEAR: Col = Col::rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Col = Col::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Col = Col::rgb(255.0, 255.0, 255.0);

    pub fn alpha(self, a: f64) -> Col {
        Col { a, ..self }
    }

    fn d2d(self, k: f64) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: (self.r / 255.0) as f32,
            g: (self.g / 255.0) as f32,
            b: (self.b / 255.0) as f32,
            a: (self.a.clamp(0.0, 1.0) * k) as f32,
        }
    }
}

/// `#rrggbb` / `#rgb`, for the few colours written as text.
pub fn hex(s: &str) -> Col {
    let t = s.trim().trim_start_matches('#');
    let full: String = if t.len() == 3 { t.chars().flat_map(|c| [c, c]).collect() } else { t.to_string() };
    match u32::from_str_radix(&full, 16) {
        Ok(n) if full.len() == 6 => Col::rgb(f64::from((n >> 16) & 255), f64::from((n >> 8) & 255), f64::from(n & 255)),
        _ => Col::WHITE,
    }
}

// ---- the affine transform (canvas order: x' = a x + c y + e, y' = b x + d y + f)

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Affine {
        Affine { a, b, c, d, e, f }
    }
    pub fn translate(x: f64, y: f64) -> Affine {
        Affine::new(1.0, 0.0, 0.0, 1.0, x, y)
    }
    pub fn scale(x: f64, y: f64) -> Affine {
        Affine::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }
    pub fn rotate(t: f64) -> Affine {
        let (s, c) = t.sin_cos();
        Affine::new(c, s, -s, c, 0.0, 0.0)
    }
    /// self, then t
    pub fn then(self, t: Affine) -> Affine {
        Affine {
            a: self.a * t.a + self.b * t.c,
            b: self.a * t.b + self.b * t.d,
            c: self.c * t.a + self.d * t.c,
            d: self.c * t.b + self.d * t.d,
            e: self.e * t.a + self.f * t.c + t.e,
            f: self.e * t.b + self.f * t.d + t.f,
        }
    }
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-15 || !det.is_finite() {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine { a, b, c, d, e: -(self.e * a + self.f * c), f: -(self.e * b + self.f * d) })
    }
    fn m(&self) -> Matrix3x2 {
        Matrix3x2 {
            M11: self.a as f32,
            M12: self.b as f32,
            M21: self.c as f32,
            M22: self.d as f32,
            M31: self.e as f32,
            M32: self.f as f32,
        }
    }
    /// whether it keeps axis-aligned rectangles axis-aligned
    fn axis_aligned(&self) -> bool {
        self.b.abs() < 1e-9 && self.c.abs() < 1e-9
    }
}

// ---- paints

#[derive(Clone, Debug, PartialEq)]
pub enum GradKind {
    Linear(f64, f64, f64, f64),
    /// x0 y0 r0 x1 y1 r1
    Radial(f64, f64, f64, f64, f64, f64),
}

/// createLinearGradient / createRadialGradient: stops in user space, made into a brush when it's used.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradKind,
    stops: Vec<(f64, Col)>,
}

impl Gradient {
    pub fn linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Gradient {
        Gradient { kind: GradKind::Linear(x0, y0, x1, y1), stops: Vec::new() }
    }
    pub fn radial(x0: f64, y0: f64, r0: f64, x1: f64, y1: f64, r1: f64) -> Gradient {
        Gradient { kind: GradKind::Radial(x0, y0, r0, x1, y1, r1), stops: Vec::new() }
    }
    pub fn stop(&mut self, offset: f64, c: Col) -> &mut Gradient {
        self.stops.push((offset.clamp(0.0, 1.0), c));
        self
    }
    /// the stops in order (ties as added), and for a radial with an inner radius, remapped onto 0–r1 as D2D's
    /// single-circle gradient has them
    fn d2d_stops(&self, k: f64) -> Vec<D2D1_GRADIENT_STOP> {
        let mut s: Vec<(usize, f64, Col)> = self.stops.iter().enumerate().map(|(i, (o, c))| (i, *o, *c)).collect();
        s.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        let (inner, outer) = match self.kind {
            GradKind::Radial(_, _, r0, _, _, r1) if r1 > 0.0 => (r0.max(0.0) / r1, 1.0),
            _ => (0.0, 1.0),
        };
        let mut out: Vec<D2D1_GRADIENT_STOP> = Vec::with_capacity(s.len() + 1);
        if inner > 0.0
            && let Some(first) = s.first()
        {
            out.push(D2D1_GRADIENT_STOP { position: 0.0, color: first.2.d2d(k) });
        }
        for (_, o, c) in s {
            out.push(D2D1_GRADIENT_STOP { position: (inner + o * (outer - inner)) as f32, color: c.d2d(k) });
        }
        if out.len() == 1 {
            let c = out[0].color;
            out.push(D2D1_GRADIENT_STOP { position: 1.0, color: c });
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(Col),
    Grad(Rc<Gradient>),
}

impl From<Col> for Paint {
    fn from(c: Col) -> Paint {
        Paint::Color(c)
    }
}

impl From<Gradient> for Paint {
    fn from(g: Gradient) -> Paint {
        Paint::Grad(Rc::new(g))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Composite {
    SourceOver,
    Lighter,
    DestinationOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    Miter,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Baseline {
    Alphabetic,
    Middle,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CFont {
    pub size: f64,
    pub bold: bool,
}

#[derive(Clone, Debug)]
struct State {
    fill: Paint,
    stroke: Paint,
    line_width: f64,
    cap: Cap,
    join: Join,
    alpha: f64,
    composite: Composite,
    dash: Vec<f64>,
    dash_offset: f64,
    font: CFont,
    align: TextAlign,
    baseline: Baseline,
    smoothing: bool,
    shadow_blur: f64,
    shadow_color: Col,
    transform: Affine,
    /// how many clips were pushed when this state was saved
    clips: usize,
}

impl Default for State {
    fn default() -> State {
        State {
            fill: Paint::Color(Col::BLACK),
            stroke: Paint::Color(Col::BLACK),
            line_width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            alpha: 1.0,
            composite: Composite::SourceOver,
            dash: Vec::new(),
            dash_offset: 0.0,
            font: CFont { size: 10.0, bold: false },
            align: TextAlign::Left,
            baseline: Baseline::Alphabetic,
            smoothing: true,
            shadow_blur: 0.0,
            shadow_color: Col::CLEAR,
            transform: Affine::IDENTITY,
            clips: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Seg {
    Move(f64, f64),
    Line(f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

enum Clip {
    Axis,
    Layer,
}

// ---- what canvases share: factories, fonts

/// a laid-out line: the layout, its width, ascent and descent
type Laid = (IDWriteTextLayout, f64, f64, f64);

/// The factories and the fonts every canvas of a game shares (made from the frame's `Ui`).
pub struct Res {
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    formats: RefCell<HashMap<(u32, bool), IDWriteTextFormat>>,
    layouts: RefCell<HashMap<(String, u32, bool), Laid>>,
    family: String,
}

impl Res {
    pub fn new(ui: &Ui) -> Res {
        let (_, _, d2d, dw) = ui.parts();
        Res {
            d2d: d2d.clone(),
            dw: dw.clone(),
            formats: RefCell::new(HashMap::new()),
            layouts: RefCell::new(HashMap::new()),
            family: "Segoe UI".into(),
        }
    }

    fn format(&self, f: CFont) -> Option<IDWriteTextFormat> {
        let key = ((f.size * 16.0) as u32, f.bold);
        if let Some(x) = self.formats.borrow().get(&key) {
            return Some(x.clone());
        }
        let weight: DWRITE_FONT_WEIGHT = if f.bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_REGULAR };
        // SAFETY: the strings outlive the call
        let fmt = unsafe {
            self.dw.CreateTextFormat(
                &HSTRING::from(self.family.as_str()),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                f.size as f32,
                &HSTRING::from("en-us"),
            )
        }
        .ok()?;
        // SAFETY: a setter on a live format
        let _ = unsafe { fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP) };
        self.formats.borrow_mut().insert(key, fmt.clone());
        Some(fmt)
    }

    /// (layout, width, ascent, descent)
    fn layout(&self, text: &str, f: CFont) -> Option<(IDWriteTextLayout, f64, f64, f64)> {
        let key = (text.to_string(), (f.size * 16.0) as u32, f.bold);
        if let Some(x) = self.layouts.borrow().get(&key) {
            return Some(x.clone());
        }
        let fmt = self.format(f)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: wide outlives the call
        let l = unsafe { self.dw.CreateTextLayout(&wide, &fmt, 100_000.0, 10_000.0) }.ok()?;
        let mut m = DWRITE_TEXT_METRICS::default();
        // SAFETY: live out struct
        let _ = unsafe { l.GetMetrics(&mut m) };
        let mut lines = [DWRITE_LINE_METRICS::default(); 1];
        let mut n = 0u32;
        // SAFETY: a one-line buffer; a longer text says so in n and fills nothing, which we don't need
        let _ = unsafe { l.GetLineMetrics(Some(&mut lines), &mut n) };
        let ascent = f64::from(lines[0].baseline);
        let descent = (f64::from(lines[0].height) - ascent).max(0.0);
        let out = (l, f64::from(m.widthIncludingTrailingWhitespace), ascent, descent);
        let mut all = self.layouts.borrow_mut();
        if all.len() > 400 {
            all.clear();
        }
        all.insert(key, out.clone());
        Some(out)
    }
}

// ---- offscreen canvases

/// An offscreen canvas: a bitmap render target compatible with the window's, `w` × `h` pixels, transparent.
pub struct Offscreen {
    rt: ID2D1BitmapRenderTarget,
    pub w: f64,
    pub h: f64,
    /// the window target's generation it was made for
    pub generation: u32,
    bitmap: Option<ID2D1Bitmap>,
}

impl Offscreen {
    /// Draws into it (it keeps what was there); `paint` gets a canvas over it.
    pub fn draw(&mut self, res: &Res, paint: impl FnOnce(&mut Canvas)) {
        let rt: ID2D1RenderTarget = match self.rt.cast() {
            Ok(r) => r,
            Err(_) => return,
        };
        let dc = rt.cast::<ID2D1DeviceContext>().ok();
        // SAFETY: paired Begin/EndDraw on our own bitmap target
        unsafe { rt.BeginDraw() };
        {
            let mut c = Canvas::make(rt.clone(), dc, res, Affine::IDENTITY, self.w, self.h);
            paint(&mut c);
            c.finish();
        }
        // SAFETY: as above
        let _ = unsafe { rt.EndDraw(None, None) };
        self.bitmap = None;
    }

    /// What it holds now, for drawImage.
    pub fn image(&mut self) -> Option<ID2D1Bitmap> {
        if self.bitmap.is_none() {
            // SAFETY: a live bitmap target
            self.bitmap = unsafe { self.rt.GetBitmap() }.ok();
        }
        self.bitmap.clone()
    }
}

// ---- the canvas

pub struct Canvas<'a> {
    rt: ID2D1RenderTarget,
    dc: Option<ID2D1DeviceContext>,
    res: &'a Res,
    brush: Option<ID2D1SolidColorBrush>,
    /// canvas pixels to the target's units
    base: Affine,
    pub width: f64,
    pub height: f64,
    st: State,
    stack: Vec<State>,
    path: Vec<Seg>,
    /// the path's current point and its subpath's start, device space
    cur: Option<(f64, f64)>,
    start: Option<(f64, f64)>,
    clips: Vec<Clip>,
    blend: Composite,
    rec: Option<(ID2D1CommandList, ID2D1Image)>,
    pub generation: u32,
}

impl<'a> Canvas<'a> {
    fn make(rt: ID2D1RenderTarget, dc: Option<ID2D1DeviceContext>, res: &'a Res, base: Affine, w: f64, h: f64) -> Canvas<'a> {
        // SAFETY: a brush on a live target
        let brush = unsafe { rt.CreateSolidColorBrush(&Col::BLACK.d2d(1.0), None) }.ok();
        Canvas {
            rt,
            dc,
            res,
            brush,
            base,
            width: w,
            height: h,
            st: State::default(),
            stack: Vec::new(),
            path: Vec::new(),
            cur: None,
            start: None,
            clips: Vec::new(),
            blend: Composite::SourceOver,
            rec: None,
            generation: 0,
        }
    }

    /// A canvas over `r` (window DIPs) of the frame's target, a canvas pixel to a device pixel. The caller ends it
    /// with `finish` and then `ui.sync_transform()`.
    pub fn on(ui: &Ui, res: &'a Res, r: Rect) -> Canvas<'a> {
        let (rt, dc, _, _) = ui.parts();
        let s = f64::from(ui.px_per_dip);
        let base = Affine::scale(1.0 / s, 1.0 / s).then(Affine::translate(f64::from(r.x), f64::from(r.y)));
        let mut c = Canvas::make(rt.clone(), dc.cloned(), res, base, (f64::from(r.w) * s).round(), (f64::from(r.h) * s).round());
        c.generation = ui.generation;
        c
    }

    /// The shared factories and fonts.
    pub fn res(&self) -> &'a Res {
        self.res
    }

    /// A new offscreen canvas of `w` × `h` pixels, compatible with this canvas's target.
    pub fn offscreen(&self, w: f64, h: f64) -> Option<Offscreen> {
        let (w, h) = (w.max(1.0).round(), h.max(1.0).round());
        let size = D2D_SIZE_F { width: w as f32, height: h as f32 };
        let px = D2D_SIZE_U { width: w as u32, height: h as u32 };
        let fmt = D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED };
        // SAFETY: the sizes outlive the call
        let rt = unsafe { self.rt.CreateCompatibleRenderTarget(Some(&size), Some(&px), Some(&fmt), D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE) }.ok()?;
        // SAFETY: plain setter; one DIP is one pixel there
        unsafe { rt.SetDpi(96.0, 96.0) };
        // start transparent
        // SAFETY: paired Begin/EndDraw
        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&Col::CLEAR.d2d(1.0)));
            let _ = rt.EndDraw(None, None);
        }
        Some(Offscreen { rt, w, h, generation: self.generation, bitmap: None })
    }

    /// Ends what's open: a destination-out run, clips, the blend.
    pub fn finish(&mut self) {
        self.flush();
        while let Some(c) = self.clips.pop() {
            self.pop_clip(c);
        }
        self.set_blend(Composite::SourceOver);
    }

    // ---- state

    pub fn fill_style(&mut self, p: impl Into<Paint>) {
        self.st.fill = p.into();
    }
    pub fn stroke_style(&mut self, p: impl Into<Paint>) {
        self.st.stroke = p.into();
    }
    pub fn set_line_width(&mut self, w: f64) {
        if w > 0.0 && w.is_finite() {
            self.st.line_width = w;
        }
    }
    pub fn line_width(&self) -> f64 {
        self.st.line_width
    }
    pub fn set_line_cap(&mut self, c: Cap) {
        self.st.cap = c;
    }
    pub fn set_line_join(&mut self, j: Join) {
        self.st.join = j;
    }
    pub fn set_alpha(&mut self, a: f64) {
        if (0.0..=1.0).contains(&a) {
            self.st.alpha = a;
        } else if a > 1.0 {
            self.st.alpha = 1.0;
        }
    }
    pub fn alpha(&self) -> f64 {
        self.st.alpha
    }
    pub fn set_composite(&mut self, c: Composite) {
        self.st.composite = c;
    }
    pub fn set_line_dash(&mut self, d: &[f64]) {
        if d.iter().all(|x| *x >= 0.0 && x.is_finite()) {
            self.st.dash = if d.len() % 2 == 1 { [d, d].concat() } else { d.to_vec() };
        }
    }
    pub fn set_dash_offset(&mut self, o: f64) {
        self.st.dash_offset = o;
    }
    pub fn set_font(&mut self, size: f64, bold: bool) {
        self.st.font = CFont { size, bold };
    }
    pub fn set_text_align(&mut self, a: TextAlign) {
        self.st.align = a;
    }
    pub fn set_text_baseline(&mut self, b: Baseline) {
        self.st.baseline = b;
    }
    pub fn set_smoothing(&mut self, on: bool) {
        self.st.smoothing = on;
    }
    pub fn set_shadow(&mut self, blur: f64, color: Col) {
        self.st.shadow_blur = blur.max(0.0);
        self.st.shadow_color = color;
    }

    pub fn save(&mut self) {
        let mut s = self.st.clone();
        s.clips = self.clips.len();
        self.stack.push(s);
    }

    pub fn restore(&mut self) {
        let Some(s) = self.stack.pop() else { return };
        while self.clips.len() > s.clips {
            if let Some(c) = self.clips.pop() {
                self.pop_clip(c);
            }
        }
        self.st = s;
    }

    // ---- the transform

    pub fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.st.transform = Affine::new(a, b, c, d, e, f);
    }
    pub fn reset_transform(&mut self) {
        self.st.transform = Affine::IDENTITY;
    }
    pub fn translate(&mut self, x: f64, y: f64) {
        self.st.transform = Affine::translate(x, y).then(self.st.transform);
    }
    pub fn scale(&mut self, x: f64, y: f64) {
        self.st.transform = Affine::scale(x, y).then(self.st.transform);
    }
    pub fn rotate(&mut self, a: f64) {
        self.st.transform = Affine::rotate(a).then(self.st.transform);
    }

    // ---- the path

    pub fn begin_path(&mut self) {
        self.path.clear();
        self.cur = None;
        self.start = None;
    }

    fn dev(&self, x: f64, y: f64) -> (f64, f64) {
        self.st.transform.apply(x, y)
    }

    fn move_dev(&mut self, p: (f64, f64)) {
        self.path.push(Seg::Move(p.0, p.1));
        self.cur = Some(p);
        self.start = Some(p);
    }

    fn line_dev(&mut self, p: (f64, f64)) {
        if self.cur.is_none() {
            self.move_dev(p);
            return;
        }
        self.path.push(Seg::Line(p.0, p.1));
        self.cur = Some(p);
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            let p = self.dev(x, y);
            self.move_dev(p);
        }
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            let p = self.dev(x, y);
            self.line_dev(p);
        }
    }

    pub fn close_path(&mut self) {
        if self.cur.is_some() {
            self.path.push(Seg::Close);
            self.cur = self.start;
        }
    }

    pub fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        if self.cur.is_none() {
            let p = self.dev(cx, cy);
            self.move_dev(p);
        }
        let p0 = self.cur.unwrap_or((0.0, 0.0));
        let c = self.dev(cx, cy);
        let p = self.dev(x, y);
        let c1 = (p0.0 + 2.0 / 3.0 * (c.0 - p0.0), p0.1 + 2.0 / 3.0 * (c.1 - p0.1));
        let c2 = (p.0 + 2.0 / 3.0 * (c.0 - p.0), p.1 + 2.0 / 3.0 * (c.1 - p.1));
        self.path.push(Seg::Cubic(c1.0, c1.1, c2.0, c2.1, p.0, p.1));
        self.cur = Some(p);
    }

    pub fn bezier_curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        if self.cur.is_none() {
            let p = self.dev(c1x, c1y);
            self.move_dev(p);
        }
        let c1 = self.dev(c1x, c1y);
        let c2 = self.dev(c2x, c2y);
        let p = self.dev(x, y);
        self.path.push(Seg::Cubic(c1.0, c1.1, c2.0, c2.1, p.0, p.1));
        self.cur = Some(p);
    }

    /// arcTo: the arc of radius r tangent to (current → p1) and (p1 → p2), joined by a line from the current point.
    pub fn arc_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, r: f64) {
        if r < 0.0 {
            return;
        }
        if self.cur.is_none() {
            let p = self.dev(x1, y1);
            self.move_dev(p);
        }
        let inv = self.st.transform.invert();
        let Some(inv) = inv else { return };
        let c = self.cur.unwrap_or((0.0, 0.0));
        let (x0, y0) = inv.apply(c.0, c.1);
        let (v1x, v1y) = (x0 - x1, y0 - y1);
        let (v2x, v2y) = (x2 - x1, y2 - y1);
        let (l1, l2) = (v1x.hypot(v1y), v2x.hypot(v2y));
        let cross = v1x * v2y - v1y * v2x;
        if r == 0.0 || l1 < 1e-12 || l2 < 1e-12 || cross.abs() < 1e-12 {
            self.line_to(x1, y1);
            return;
        }
        let (u1x, u1y, u2x, u2y) = (v1x / l1, v1y / l1, v2x / l2, v2y / l2);
        let cos = (u1x * u2x + u1y * u2y).clamp(-1.0, 1.0);
        let theta = cos.acos();
        let d = r / (theta / 2.0).tan();
        let (t1x, t1y) = (x1 + u1x * d, y1 + u1y * d);
        let (t2x, t2y) = (x1 + u2x * d, y1 + u2y * d);
        let (bx, by) = (u1x + u2x, u1y + u2y);
        let bl = bx.hypot(by);
        let h = r / (theta / 2.0).sin();
        let (cx, cy) = (x1 + bx / bl * h, y1 + by / bl * h);
        self.line_to(t1x, t1y);
        let a0 = (t1y - cy).atan2(t1x - cx);
        let a1 = (t2y - cy).atan2(t2x - cx);
        let mut sweep = a1 - a0;
        while sweep > PI {
            sweep -= 2.0 * PI;
        }
        while sweep < -PI {
            sweep += 2.0 * PI;
        }
        let m = Affine::scale(r, r).then(Affine::translate(cx, cy)).then(self.st.transform);
        self.unit_arc(m, a0, sweep);
    }

    /// Cubics along the unit circle from a0 by sweep, through m.
    fn unit_arc(&mut self, m: Affine, a0: f64, sweep: f64) {
        let n = ((sweep.abs() / (PI / 2.0)).ceil() as usize).max(1);
        let step = sweep / n as f64;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let mut a = a0;
        for _ in 0..n {
            let b = a + step;
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            let p1 = m.apply(ca - k * sa, sa + k * ca);
            let p2 = m.apply(cb + k * sb, sb - k * cb);
            let p3 = m.apply(cb, sb);
            self.path.push(Seg::Cubic(p1.0, p1.1, p2.0, p2.1, p3.0, p3.1));
            self.cur = Some(p3);
            a = b;
        }
    }

    /// arc: angles in radians, y down; anticlockwise as on the web.
    pub fn arc(&mut self, x: f64, y: f64, r: f64, a0: f64, a1: f64, ccw: bool) {
        if r >= 0.0 && x.is_finite() && y.is_finite() && r.is_finite() {
            self.ellipse_arc(x, y, r, r, 0.0, a0, a1, ccw);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64, rot: f64, a0: f64, a1: f64, ccw: bool) {
        if rx >= 0.0 && ry >= 0.0 {
            self.ellipse_arc(x, y, rx, ry, rot, a0, a1, ccw);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ellipse_arc(&mut self, x: f64, y: f64, rx: f64, ry: f64, rot: f64, a0: f64, a1: f64, ccw: bool) {
        // the spec's sweep: a full turn or more draws the whole ellipse; otherwise the short way round in the asked
        // direction
        let tau = 2.0 * PI;
        let sweep = if !ccw && a1 - a0 >= tau {
            tau
        } else if ccw && a0 - a1 >= tau {
            -tau
        } else {
            let mut s = (a1 - a0) % tau;
            if !ccw && s < 0.0 {
                s += tau;
            }
            if ccw && s > 0.0 {
                s -= tau;
            }
            s
        };
        let m = Affine::scale(if rx == 0.0 { 1e-9 } else { rx }, if ry == 0.0 { 1e-9 } else { ry })
            .then(Affine::rotate(rot))
            .then(Affine::translate(x, y))
            .then(self.st.transform);
        let start = m.apply(a0.cos(), a0.sin());
        if self.cur.is_none() {
            self.move_dev(start);
        } else {
            self.line_dev(start);
        }
        if sweep != 0.0 {
            self.unit_arc(m, a0, sweep);
        }
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
            return;
        }
        let p = self.dev(x, y);
        self.move_dev(p);
        let p = self.dev(x + w, y);
        self.line_dev(p);
        let p = self.dev(x + w, y + h);
        self.line_dev(p);
        let p = self.dev(x, y + h);
        self.line_dev(p);
        self.close_path();
    }

    /// roundRect with one radius for every corner, scaled down as the spec says when they don't fit.
    pub fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64) {
        let mut r = radius.max(0.0);
        let k = 1f64.min(w.abs() / (2.0 * r.max(1e-12))).min(h.abs() / (2.0 * r.max(1e-12)));
        if r > 0.0 {
            r *= k;
        }
        if r == 0.0 {
            self.rect(x, y, w, h);
            return;
        }
        self.move_to(x + r, y);
        self.line_to(x + w - r, y);
        self.arc_to(x + w, y, x + w, y + r, r);
        self.line_to(x + w, y + h - r);
        self.arc_to(x + w, y + h, x + w - r, y + h, r);
        self.line_to(x + r, y + h);
        self.arc_to(x, y + h, x, y + h - r, r);
        self.line_to(x, y + r);
        self.arc_to(x, y, x + r, y, r);
        self.close_path();
        self.move_to(x, y);
    }

    // ---- geometry

    /// The path as a geometry, each point through `back` (device space to the space it's drawn in).
    fn geometry(&self, segs: &[Seg], back: Option<&Affine>, filled: bool, even_odd: bool) -> Option<ID2D1PathGeometry> {
        if segs.is_empty() {
            return None;
        }
        let tp = |x: f64, y: f64| {
            let (x, y) = back.map_or((x, y), |b| b.apply(x, y));
            Vector2 { X: x as f32, Y: y as f32 }
        };
        // SAFETY: a fresh geometry, opened once and closed
        unsafe {
            let geo = self.res.d2d.CreatePathGeometry().ok()?;
            let sink = geo.Open().ok()?;
            sink.SetFillMode(if even_odd { D2D1_FILL_MODE_ALTERNATE } else { D2D1_FILL_MODE_WINDING });
            let begin = if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
            let mut open = false;
            for s in segs {
                match *s {
                    Seg::Move(x, y) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(tp(x, y), begin);
                        open = true;
                    }
                    Seg::Line(x, y) => {
                        if open {
                            sink.AddLine(tp(x, y));
                        }
                    }
                    Seg::Cubic(a, b, c, d, e, f) => {
                        if open {
                            sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: tp(a, b), point2: tp(c, d), point3: tp(e, f) });
                        }
                    }
                    Seg::Close => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            open = false;
                        }
                    }
                }
            }
            if open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(geo)
        }
    }

    fn set_rt_transform(&self, t: Affine) {
        // SAFETY: inside a draw
        unsafe { self.rt.SetTransform(&t.m()) };
    }

    // ---- composites

    fn set_blend(&mut self, c: Composite) {
        if c == self.blend {
            return;
        }
        self.blend = c;
        if let Some(dc) = &self.dc {
            // SAFETY: a setter inside a draw
            unsafe { dc.SetPrimitiveBlend(if c == Composite::Lighter { D2D1_PRIMITIVE_BLEND_ADD } else { D2D1_PRIMITIVE_BLEND_SOURCE_OVER }) };
        }
    }

    /// Before a primitive: the composite of the moment. False: don't draw it (destination-out with no way to).
    fn prep(&mut self) -> bool {
        match self.st.composite {
            Composite::DestinationOut => {
                self.set_blend(Composite::SourceOver);
                if self.rec.is_some() {
                    return true;
                }
                let Some(dc) = &self.dc else { return false };
                // SAFETY: the run goes into a command list until the composite changes
                let started = unsafe {
                    let old = dc.GetTarget();
                    let list = dc.CreateCommandList();
                    match (old, list) {
                        (Ok(old), Ok(list)) => {
                            dc.SetTarget(&list);
                            Some((list, old))
                        }
                        _ => None,
                    }
                };
                self.rec = started;
                self.rec.is_some()
            }
            c => {
                self.flush();
                self.set_blend(c);
                true
            }
        }
    }

    /// A destination-out run ends: the recorded primitives cut out of what's there.
    fn flush(&mut self) {
        let Some((list, old)) = self.rec.take() else { return };
        let Some(dc) = &self.dc else { return };
        // SAFETY: back to the real target; the list is closed and drawn once, untransformed (it recorded the
        // transforms it was drawn with)
        unsafe {
            dc.SetTarget(&old);
            if list.Close().is_ok() {
                dc.SetTransform(&Matrix3x2::identity());
                let img: ID2D1Image = list.cast().unwrap_or_else(|_| old.clone());
                dc.DrawImage(&img, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_DESTINATION_OUT);
            }
        }
    }

    fn solid(&self, c: Col) -> Option<ID2D1Brush> {
        let b = self.brush.as_ref()?;
        // SAFETY: a live brush
        unsafe {
            b.SetColor(&c.d2d(self.st.alpha));
            b.SetOpacity(1.0);
        }
        b.cast().ok()
    }

    /// A brush for a paint; gradients in user space are placed with `grad_t` (user to the drawing space).
    fn brush_for(&self, p: &Paint, grad_t: Affine) -> Option<ID2D1Brush> {
        match p {
            Paint::Color(c) => self.solid(*c),
            Paint::Grad(g) => {
                let stops = g.d2d_stops(1.0);
                // SAFETY: plain resource creation on a live target; the structs outlive the calls
                unsafe {
                    let coll = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok()?;
                    let bp = D2D1_BRUSH_PROPERTIES { opacity: self.st.alpha as f32, transform: grad_t.m() };
                    match g.kind {
                        GradKind::Linear(x0, y0, x1, y1) => {
                            let lp = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                                startPoint: Vector2 { X: x0 as f32, Y: y0 as f32 },
                                endPoint: Vector2 { X: x1 as f32, Y: y1 as f32 },
                            };
                            self.rt.CreateLinearGradientBrush(&lp, Some(&bp), &coll).ok()?.cast().ok()
                        }
                        GradKind::Radial(x0, y0, _, x1, y1, r1) => {
                            let rp = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                                center: Vector2 { X: x1 as f32, Y: y1 as f32 },
                                gradientOriginOffset: Vector2 { X: (x0 - x1) as f32, Y: (y0 - y1) as f32 },
                                radiusX: r1 as f32,
                                radiusY: r1 as f32,
                            };
                            self.rt.CreateRadialGradientBrush(&rp, Some(&bp), &coll).ok()?.cast().ok()
                        }
                    }
                }
            }
        }
    }

    fn stroke_style_obj(&self, scale: f64) -> Option<ID2D1StrokeStyle> {
        let cap: D2D1_CAP_STYLE = match self.st.cap {
            Cap::Butt => D2D1_CAP_STYLE_FLAT,
            Cap::Round => D2D1_CAP_STYLE_ROUND,
            Cap::Square => D2D1_CAP_STYLE_SQUARE,
        };
        let join: D2D1_LINE_JOIN = match self.st.join {
            Join::Miter => D2D1_LINE_JOIN_MITER,
            Join::Round => D2D1_LINE_JOIN_ROUND,
        };
        let w = (self.st.line_width * scale).max(1e-6);
        let dashed = !self.st.dash.is_empty() && self.st.dash.iter().any(|d| *d > 0.0);
        // custom dashes and their offset are in stroke widths
        let dashes: Vec<f32> = self.st.dash.iter().map(|d| (d / w).max(1e-4) as f32).collect();
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: cap,
            endCap: cap,
            dashCap: cap,
            lineJoin: join,
            miterLimit: 10.0,
            dashStyle: if dashed { D2D1_DASH_STYLE_CUSTOM } else { D2D1_DASH_STYLE_SOLID },
            dashOffset: if dashed { (self.st.dash_offset / w) as f32 } else { 0.0 },
        };
        // SAFETY: the props and dashes outlive the call
        unsafe { self.res.d2d.CreateStrokeStyle(&props, if dashed { Some(&dashes) } else { None }) }.ok()
    }

    // ---- painting

    fn fill_segs(&mut self, segs: &[Seg], even_odd: bool) {
        if segs.is_empty() || !self.prep() {
            return;
        }
        let Some(geo) = self.geometry(segs, None, true, even_odd) else { return };
        let paint = self.st.fill.clone();
        let Some(b) = self.brush_for(&paint, self.st.transform) else { return };
        if self.st.shadow_blur > 0.0 && self.st.shadow_color.a > 0.0 {
            self.halo(&geo, None);
        }
        self.set_rt_transform(self.base);
        // SAFETY: inside a draw
        unsafe { self.rt.FillGeometry(&geo, &b, None) };
    }

    /// The shadow, as the web draws it under a shape: a soft halo of the shadow colour, widening strokes fading out.
    fn halo(&mut self, geo: &ID2D1PathGeometry, stroke_w: Option<f64>) {
        let blur = self.st.shadow_blur;
        let c = self.st.shadow_color;
        let Some(brush) = self.brush.clone() else { return };
        let Some(style) = ({
            let saved = (self.st.cap, self.st.join, std::mem::take(&mut self.st.dash));
            self.st.cap = Cap::Round;
            self.st.join = Join::Round;
            let s = self.stroke_style_obj(1.0);
            (self.st.cap, self.st.join, self.st.dash) = saved;
            s
        }) else {
            return;
        };
        self.set_rt_transform(self.base);
        let base_w = stroke_w.unwrap_or(0.0);
        let n = 6;
        for i in (1..=n).rev() {
            let k = i as f64 / n as f64;
            let w = base_w + 2.0 * blur * k;
            let a = c.a * (1.0 - k).powf(1.5) * 0.55 + c.a * 0.04;
            // SAFETY: a live brush, inside a draw
            unsafe {
                brush.SetColor(&c.alpha(a).d2d(self.st.alpha));
                self.rt.DrawGeometry(geo, &brush, w as f32, &style);
            }
        }
    }

    pub fn fill(&mut self) {
        let segs = std::mem::take(&mut self.path);
        self.fill_segs(&segs, false);
        self.path = segs;
    }

    pub fn fill_even_odd(&mut self) {
        let segs = std::mem::take(&mut self.path);
        self.fill_segs(&segs, true);
        self.path = segs;
    }

    pub fn stroke(&mut self) {
        if self.path.is_empty() || !self.prep() {
            return;
        }
        let t = self.st.transform;
        let Some(inv) = t.invert() else { return };
        let segs = std::mem::take(&mut self.path);
        let user = self.geometry(&segs, Some(&inv), false, false);
        let shadow = self.st.shadow_blur > 0.0 && self.st.shadow_color.a > 0.0;
        let dev_geo = if shadow { self.geometry(&segs, None, false, false) } else { None };
        self.path = segs;
        let Some(geo) = user else { return };
        if let Some(dg) = &dev_geo {
            // the line's width on the device, for the halo around it
            let k = (t.a * t.d - t.b * t.c).abs().sqrt();
            self.halo(dg, Some(self.st.line_width * k));
        }
        let paint = self.st.stroke.clone();
        let Some(b) = self.brush_for(&paint, Affine::IDENTITY) else { return };
        let style = self.stroke_style_obj(1.0);
        self.set_rt_transform(t.then(self.base));
        // SAFETY: inside a draw
        unsafe { self.rt.DrawGeometry(&geo, &b, self.st.line_width as f32, style.as_ref()) };
    }

    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) || w == 0.0 || h == 0.0 {
            return;
        }
        let t = self.st.transform;
        if t.axis_aligned() && self.st.shadow_blur <= 0.0 {
            // the common case, without a geometry
            if !self.prep() {
                return;
            }
            let paint = self.st.fill.clone();
            let Some(b) = self.brush_for(&paint, Affine::IDENTITY) else { return };
            self.set_rt_transform(t.then(self.base));
            let r = D2D_RECT_F { left: x as f32, top: y as f32, right: (x + w) as f32, bottom: (y + h) as f32 };
            // SAFETY: inside a draw
            unsafe { self.rt.FillRectangle(&r, &b) };
            return;
        }
        let saved = (std::mem::take(&mut self.path), self.cur, self.start);
        self.rect(x, y, w, h);
        let segs = std::mem::take(&mut self.path);
        (self.path, self.cur, self.start) = saved;
        self.fill_segs(&segs, false);
    }

    pub fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let saved = (std::mem::take(&mut self.path), self.cur, self.start);
        self.rect(x, y, w, h);
        self.stroke();
        (self.path, self.cur, self.start) = saved;
    }

    /// Clears to transparent (the device-space box of the rect; the transform is axis-aligned where it's used).
    pub fn clear_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.flush();
        let t = self.st.transform.then(self.base);
        let (x0, y0) = t.apply(x, y);
        let (x1, y1) = t.apply(x + w, y + h);
        let r = D2D_RECT_F { left: x0.min(x1) as f32, top: y0.min(y1) as f32, right: x0.max(x1) as f32, bottom: y0.max(y1) as f32 };
        // SAFETY: a paired clip inside a draw
        unsafe {
            self.rt.SetTransform(&Matrix3x2::identity());
            self.rt.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            self.rt.Clear(Some(&Col::CLEAR.d2d(1.0)));
            self.rt.PopAxisAlignedClip();
        }
    }

    /// clip(): an axis-aligned rect clips as one; anything else through a layer with the path as its mask.
    pub fn clip(&mut self) {
        self.flush();
        let segs = self.path.clone();
        let rect = rect_of(&segs);
        match rect {
            Some((x0, y0, x1, y1)) => {
                let (a, b) = (self.base.apply(x0, y0), self.base.apply(x1, y1));
                let r = D2D_RECT_F { left: a.0.min(b.0) as f32, top: a.1.min(b.1) as f32, right: a.0.max(b.0) as f32, bottom: a.1.max(b.1) as f32 };
                // SAFETY: inside a draw; popped by restore or finish
                unsafe {
                    self.rt.SetTransform(&Matrix3x2::identity());
                    self.rt.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                }
                self.clips.push(Clip::Axis);
            }
            None => {
                let geo = if segs.is_empty() {
                    None
                } else {
                    self.geometry(&segs, None, true, false)
                };
                let mask: Option<ID2D1Geometry> = match geo {
                    Some(g) => g.cast().ok(),
                    // an empty path clips everything out
                    None => {
                        // SAFETY: a fresh empty rectangle geometry
                        unsafe { self.res.d2d.CreateRectangleGeometry(&D2D_RECT_F::default()) }.ok().and_then(|g| g.cast().ok())
                    }
                };
                // SAFETY: a layer on a live target; popped by restore or finish
                let pushed = unsafe {
                    self.rt.CreateLayer(None).is_ok_and(|layer| {
                        self.set_rt_transform(Affine::IDENTITY);
                        let params = D2D1_LAYER_PARAMETERS {
                            contentBounds: D2D_RECT_F { left: -1e7, top: -1e7, right: 1e7, bottom: 1e7 },
                            geometricMask: std::mem::ManuallyDrop::new(mask),
                            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                            maskTransform: self.base.m(),
                            opacity: 1.0,
                            opacityBrush: std::mem::ManuallyDrop::new(None),
                            layerOptions: D2D1_LAYER_OPTIONS_NONE,
                        };
                        self.rt.PushLayer(&params, &layer);
                        let _ = std::mem::ManuallyDrop::into_inner(params.geometricMask);
                        true
                    })
                };
                if pushed {
                    self.clips.push(Clip::Layer);
                }
            }
        }
    }

    fn pop_clip(&mut self, c: Clip) {
        self.flush();
        // SAFETY: popping what clip pushed, in order
        unsafe {
            match c {
                Clip::Axis => self.rt.PopAxisAlignedClip(),
                Clip::Layer => self.rt.PopLayer(),
            }
        }
    }

    /// drawImage(img, dx, dy, dw, dh)
    pub fn draw_image(&mut self, img: Option<&ID2D1Bitmap>, dx: f64, dy: f64, dw: f64, dh: f64) {
        let Some(img) = img else { return };
        if dw == 0.0 || dh == 0.0 || !self.prep() {
            return;
        }
        self.set_rt_transform(self.st.transform.then(self.base));
        let r = D2D_RECT_F { left: dx as f32, top: dy as f32, right: (dx + dw) as f32, bottom: (dy + dh) as f32 };
        let mode = if self.st.smoothing { D2D1_BITMAP_INTERPOLATION_MODE_LINEAR } else { D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR };
        // SAFETY: inside a draw; the bitmap was made on a target compatible with this one
        unsafe { self.rt.DrawBitmap(img, Some(&r), self.st.alpha as f32, mode, None) };
    }

    // ---- text

    pub fn measure_text(&self, text: &str) -> f64 {
        self.res.layout(text, self.st.font).map_or(0.0, |l| l.1)
    }

    pub fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        let Paint::Color(c) = self.st.fill else { return };
        let Some((layout, w, ascent, descent)) = self.res.layout(text, self.st.font) else { return };
        if !self.prep() {
            return;
        }
        let ox = match self.st.align {
            TextAlign::Left => x,
            TextAlign::Center => x - w / 2.0,
            TextAlign::Right => x - w,
        };
        let baseline = match self.st.baseline {
            Baseline::Alphabetic => y,
            Baseline::Middle => y + (ascent - descent) / 2.0,
            Baseline::Top => y + ascent,
            Baseline::Bottom => y - descent,
        };
        let Some(b) = self.solid(c) else { return };
        self.set_rt_transform(self.st.transform.then(self.base));
        // SAFETY: inside a draw
        unsafe {
            self.rt.DrawTextLayout(
                Vector2 { X: ox as f32, Y: (baseline - ascent) as f32 },
                &layout,
                &b,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            )
        };
    }
}

impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The path, if it is one axis-aligned rectangle: x0 y0 x1 y1 (device space).
fn rect_of(segs: &[Seg]) -> Option<(f64, f64, f64, f64)> {
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        match *s {
            Seg::Move(x, y) if i == 0 => pts.push((x, y)),
            Seg::Line(x, y) => pts.push((x, y)),
            Seg::Close if i == segs.len() - 1 => {}
            _ => return None,
        }
    }
    if pts.len() != 4 {
        return None;
    }
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let [p0, p1, p2, p3] = [pts[0], pts[1], pts[2], pts[3]];
    let horizontal_first = near(p0.1, p1.1) && near(p1.0, p2.0) && near(p2.1, p3.1) && near(p3.0, p0.0);
    let vertical_first = near(p0.0, p1.0) && near(p1.1, p2.1) && near(p2.0, p3.0) && near(p3.1, p0.1);
    (horizontal_first || vertical_first).then(|| (p0.0.min(p2.0), p0.1.min(p2.1), p0.0.max(p2.0), p0.1.max(p2.1)))
}

/// Math.round: half up, toward +infinity.
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_composes_as_the_canvas_does() {
        // translate then scale (canvas: ctx.translate(10, 0); ctx.scale(2, 2)): a point (1, 1) lands at (12, 2)
        let t = Affine::scale(2.0, 2.0).then(Affine::translate(10.0, 0.0));
        assert_eq!(t.apply(1.0, 1.0), (12.0, 2.0));
        let inv = t.invert().unwrap();
        let (x, y) = inv.apply(12.0, 2.0);
        assert!((x - 1.0).abs() < 1e-12 && (y - 1.0).abs() < 1e-12);
        let r = Affine::rotate(PI / 2.0);
        let (x, y) = r.apply(1.0, 0.0);
        assert!(x.abs() < 1e-12 && (y - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_radial_with_an_inner_radius_maps_its_stops_outward() {
        let mut g = Gradient::radial(0.0, 0.0, 25.0, 0.0, 0.0, 100.0);
        g.stop(0.0, Col::BLACK).stop(1.0, Col::WHITE);
        let s = g.d2d_stops(1.0);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].position, 0.0);
        assert!((s[1].position - 0.25).abs() < 1e-6);
        assert_eq!(s[2].position, 1.0);
    }

    #[test]
    fn rectangles_are_found_for_axis_clips() {
        let segs = [Seg::Move(0.0, 0.0), Seg::Line(10.0, 0.0), Seg::Line(10.0, 5.0), Seg::Line(0.0, 5.0), Seg::Close];
        assert_eq!(rect_of(&segs), Some((0.0, 0.0, 10.0, 5.0)));
        let tri = [Seg::Move(0.0, 0.0), Seg::Line(10.0, 0.0), Seg::Line(10.0, 5.0), Seg::Close];
        assert_eq!(rect_of(&tri), None);
        assert_eq!(hex("#9b7bff"), Col::rgb(155.0, 123.0, 255.0));
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(2.5), 3.0);
    }
}
