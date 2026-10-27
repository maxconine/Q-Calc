//! The effect window: a transparent, click-through, topmost layered window over the bar (and the room round it),
//! drawn with software Direct2D into a DIB and handed to DWM with `UpdateLayeredWindow`. The lighter of the two
//! ways: DirectComposition would need a D3D device (~50 MB of driver memory and 150+ ms to make, README "View /
//! paint model"), where this is a DC render target and one bitmap the window's size.
//!
//! Each effect runs on its own short-lived thread (`qcalc-effect`), which owns the window, the factory, the
//! render target, the bitmaps and the frame loop (paced by `DwmFlush`, capped near 60 fps), so the UI thread does
//! nothing per frame and a keystroke never waits on a frame. When the effect ends, is cancelled, or the bar hides,
//! the thread destroys the window and returns: everything it made goes with it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qcalc_ffi::SmokePuff;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT, D2D1_BEZIER_SEGMENT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_BRUSH_PROPERTIES,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_FLAT,
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_SOLID,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_OPTIONS,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LINE_JOIN_ROUND,
    D2D1_QUADRATIC_BEZIER_SEGMENT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory,
    ID2D1Bitmap, ID2D1BitmapBrush, ID2D1DCRenderTarget, ID2D1Factory, ID2D1PathGeometry,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FEATURE, DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmFlush, DwmSetWindowAttribute};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP,
    HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowRect,
    IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SW_SHOWNOACTIVATE,
    ShowWindow, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, HSTRING, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use super::anim;
use super::shapes::{self, Seg};
use crate::render::{Color, Rect};

/// What an effect is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Smoke,
    Arms,
    Fold,
    Void,
    Euler,
    PiDigits,
    Roll,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Smoke => "smoke",
            Kind::Arms => "arms",
            Kind::Fold => "fold",
            Kind::Void => "void",
            Kind::Euler => "euler",
            Kind::PiDigits => "pidigits",
            Kind::Roll => "roll",
        }
    }

    /// The room round the bar the effect draws in, DIPs.
    fn room(self, bar_w: f64, bar_h: f64) -> f64 {
        match self {
            Kind::Smoke | Kind::Euler => 200.0,
            Kind::Roll => anim::roll_room(bar_w, bar_h),
            Kind::Arms | Kind::Fold | Kind::Void | Kind::PiDigits => 24.0,
        }
    }
}

/// Everything an effect needs, captured on the UI thread when it starts.
pub struct Spec {
    pub kind: Kind,
    /// the core's play time, ms
    pub play_ms: f64,
    pub reduce: bool,
    pub bar: usize,
    pub dpi: f32,
    /// the bar's window, DIPs (its size)
    pub bar_w: f32,
    pub bar_h: f32,
    /// in the bar's DIPs
    pub composer: Rect,
    pub answer: Rect,
    pub answer_text: String,
    pub bg: Color,
    pub muted: Color,
    pub answer_color: Color,
    pub family: String,
    pub text_family: String,
    pub puffs: Vec<SmokePuff>,
    pub pi: String,
    /// the barrel roll's picture of the bar: BGRA rows, its pixel size
    pub picture: Option<(Vec<u8>, u32, u32)>,
}

/// What the UI thread and the effect's thread share.
#[derive(Default)]
pub struct Shared {
    /// the UI thread wants it gone (it uncloaks the bar itself)
    pub cancel: AtomicBool,
    pub done: AtomicBool,
    /// the effect cloaked the bar (the roll)
    pub cloaked: AtomicBool,
    pub hwnd: AtomicUsize,
}

pub fn spawn(spec: Spec) -> Option<Arc<Shared>> {
    let shared = Arc::new(Shared::default());
    let theirs = shared.clone();
    std::thread::Builder::new()
        .name("qcalc-effect".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            let kind = spec.kind;
            if let Err(e) = run(spec, &theirs) {
                crate::qlog!("effect {}: {e}", kind.name());
            }
            theirs.done.store(true, Ordering::SeqCst);
        })
        .ok()?;
    Some(shared)
}

pub fn cloak(bar: usize, on: bool) {
    let v = BOOL::from(on);
    // SAFETY: an attribute on our own bar, with a pointer to a live local
    let _ = unsafe {
        DwmSetWindowAttribute(
            HWND(bar as *mut _),
            DWMWA_CLOAK,
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: the default for our own window
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// The window's bitmap: a top-down 32-bit DIB in a memory DC.
struct Dib {
    screen: HDC,
    mem: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
}

impl Dib {
    fn new(w: i32, h: i32) -> Result<Dib> {
        // SAFETY: plain GDI creation; the out pointer is a live local
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(b) => b,
                Err(e) => {
                    let _ = DeleteDC(mem);
                    ReleaseDC(None, screen);
                    return Err(e);
                }
            };
            let old = SelectObject(mem, bitmap.into());
            Ok(Dib {
                screen,
                mem,
                bitmap,
                old,
                w,
                h,
            })
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: undoing new() in reverse
        unsafe {
            SelectObject(self.mem, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.mem);
            ReleaseDC(None, self.screen);
        }
    }
}

/// A 2D affine transform, row-vector (D2D's Matrix3x2).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Aff([f32; 6]);

impl Aff {
    const ID: Aff = Aff([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    fn translate(x: f64, y: f64) -> Aff {
        Aff([1.0, 0.0, 0.0, 1.0, x as f32, y as f32])
    }
    fn scale(s: f64) -> Aff {
        Aff([s as f32, 0.0, 0.0, s as f32, 0.0, 0.0])
    }
    /// clockwise on screen, as css rotate
    fn rotate(rad: f64) -> Aff {
        let (s, c) = (rad.sin() as f32, rad.cos() as f32);
        Aff([c, s, -s, c, 0.0, 0.0])
    }
    /// this, then `b`
    fn then(self, b: Aff) -> Aff {
        let a = self.0;
        let b = b.0;
        Aff([
            a[0] * b[0] + a[1] * b[2],
            a[0] * b[1] + a[1] * b[3],
            a[2] * b[0] + a[3] * b[2],
            a[2] * b[1] + a[3] * b[3],
            a[4] * b[0] + a[5] * b[2] + b[4],
            a[4] * b[1] + a[5] * b[3] + b[5],
        ])
    }
    /// scale and turn about (cx, cy), landing it at (x, y)
    fn about(cx: f64, cy: f64, scale: f64, turn: f64, x: f64, y: f64) -> Aff {
        Aff::translate(-cx, -cy)
            .then(Aff::scale(scale))
            .then(Aff::rotate(turn))
            .then(Aff::translate(x, y))
    }
    fn m(self) -> Matrix3x2 {
        let a = self.0;
        Matrix3x2 {
            M11: a[0],
            M12: a[1],
            M21: a[2],
            M22: a[3],
            M31: a[4],
            M32: a[5],
        }
    }
}

fn v(x: f64, y: f64) -> Vector2 {
    Vector2 {
        X: x as f32,
        Y: y as f32,
    }
}

fn rgba(r: f32, g: f32, b: f32, a: f64) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r,
        g,
        b,
        a: a.clamp(0.0, 1.0) as f32,
    }
}

fn faded(c: Color, a: f64) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        a: (f64::from(c.a) * a).clamp(0.0, 1.0) as f32,
        ..c
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x as f32,
        top: y as f32,
        right: (x + w) as f32,
        bottom: (y + h) as f32,
    }
}

/// What a kind keeps across frames.
enum Res {
    Smoke(Vec<(ID2D1Bitmap, SmokePuff)>),
    Arms {
        hands: [(ID2D1PathGeometry, Vec<(ID2D1PathGeometry, f32)>); 2],
        round: ID2D1StrokeStyle,
    },
    Fold([(IDWriteTextLayout, f64); 2]),
    Void {
        copy: Option<IDWriteTextLayout>,
        /// the answer's inner box (where the slot drew it) and the text's own box in it
        inner: Rect,
        text: Rect,
        dashed: ID2D1StrokeStyle,
    },
    Euler {
        note: IDWriteTextLayout,
        note_size: (f64, f64),
        bits: Vec<anim::Bit>,
    },
    PiDigits {
        layout: IDWriteTextLayout,
        size: (f64, f64),
        char_w: f64,
        count: usize,
        at: (f64, f64),
    },
    Roll(ID2D1BitmapBrush),
}

struct Scene {
    spec: Spec,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    /// the bar's top-left in this window, DIPs
    room: f64,
    res: Res,
}

fn text_layout(
    dw: &IDWriteFactory,
    text: &str,
    family: &str,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    w: f32,
    h: f32,
) -> Result<IDWriteTextLayout> {
    // SAFETY: the strings outlive the calls; the layout is ours
    unsafe {
        let format = dw.CreateTextFormat(
            &HSTRING::from(family),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            &HSTRING::from("en-us"),
        )?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = dw.CreateTextLayout(&wide, &format, w.max(1.0), h)?;
        let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        Ok(layout)
    }
}

fn measure(layout: &IDWriteTextLayout) -> (f64, f64) {
    let mut m = DWRITE_TEXT_METRICS::default();
    // SAFETY: a live out struct
    if unsafe { layout.GetMetrics(&mut m) }.is_err() {
        return (0.0, 0.0);
    }
    (
        f64::from(m.widthIncludingTrailingWhitespace),
        f64::from(m.height),
    )
}

fn geometry(factory: &ID2D1Factory, segs: &[Seg], mirror: bool, filled: bool) -> Result<ID2D1PathGeometry> {
    let mx = |x: f32| f64::from(if mirror { shapes::HAND_W - x } else { x });
    let p = |x: f32, y: f32| v(mx(x), f64::from(y));
    // SAFETY: building a geometry we own; the sink is closed before use
    unsafe {
        let g = factory.CreatePathGeometry()?;
        let sink = g.Open()?;
        let mut open = false;
        let begin = if filled {
            D2D1_FIGURE_BEGIN_FILLED
        } else {
            D2D1_FIGURE_BEGIN_HOLLOW
        };
        for s in segs {
            match *s {
                Seg::Move(x, y) => {
                    if open {
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    }
                    sink.BeginFigure(p(x, y), begin);
                    open = true;
                }
                Seg::Line(x, y) => sink.AddLine(p(x, y)),
                Seg::Cubic(a, b, c, d, e, f) => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: p(a, b),
                    point2: p(c, d),
                    point3: p(e, f),
                }),
                Seg::Quad(a, b, c, d) => sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: p(a, b),
                    point2: p(c, d),
                }),
                Seg::Close => {
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    open = false;
                }
            }
        }
        if open {
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close()?;
        Ok(g)
    }
}

fn stroke(factory: &ID2D1Factory, cap: bool, dashes: Option<&[f32]>) -> Result<ID2D1StrokeStyle> {
    let c = if cap {
        D2D1_CAP_STYLE_ROUND
    } else {
        D2D1_CAP_STYLE_FLAT
    };
    let props = D2D1_STROKE_STYLE_PROPERTIES {
        startCap: c,
        endCap: c,
        dashCap: D2D1_CAP_STYLE_FLAT,
        lineJoin: D2D1_LINE_JOIN_ROUND,
        miterLimit: 10.0,
        dashStyle: if dashes.is_some() {
            D2D1_DASH_STYLE_CUSTOM
        } else {
            D2D1_DASH_STYLE_SOLID
        },
        dashOffset: 0.0,
    };
    // SAFETY: the props and dashes outlive the call
    unsafe { factory.CreateStrokeStyle(&props, dashes) }
}

fn bitmap(rt: &ID2D1DCRenderTarget, px: &[u8], w: u32, h: u32, dpi: f32, opaque: bool) -> Result<ID2D1Bitmap> {
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: if opaque {
                D2D1_ALPHA_MODE_IGNORE
            } else {
                D2D1_ALPHA_MODE_PREMULTIPLIED
            },
        },
        dpiX: dpi,
        dpiY: dpi,
    };
    // SAFETY: px holds h rows of w BGRA pixels and outlives the call (D2D copies it)
    unsafe {
        rt.CreateBitmap(
            D2D_SIZE_U { width: w, height: h },
            Some(px.as_ptr().cast()),
            w * 4,
            &props,
        )
    }
}

impl Scene {
    fn new(spec: Spec, factory: &ID2D1Factory, room: f64) -> Result<Scene> {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: spec.dpi,
            dpiY: spec.dpi,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        // SAFETY: plain creation; props outlive the calls
        let (rt, brush) = unsafe {
            let rt = factory.CreateDCRenderTarget(&props)?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            let brush = rt.CreateSolidColorBrush(&rgba(0.0, 0.0, 0.0, 1.0), None)?;
            (rt, brush)
        };
        // SAFETY: plain factory creation (the process's shared DirectWrite factory)
        let dw: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        let bar = Rect::new(room as f32, room as f32, spec.bar_w, spec.bar_h);
        let res = match spec.kind {
            Kind::Smoke => {
                // blurred 18 px, so half a pixel a DIP is plenty; each size made once
                let res = 0.5;
                let mut made: Vec<((u32, u32), ID2D1Bitmap)> = Vec::new();
                let mut out = Vec::new();
                for p in &spec.puffs {
                    let id = (p.w as u32, p.h as u32);
                    let bmp = match made.iter().find(|(k, _)| *k == id) {
                        Some((_, b)) => b.clone(),
                        None => {
                            let img = shapes::puff(p.w, p.h, res);
                            let b = bitmap(&rt, &img.px, img.w, img.h, 96.0, false)?;
                            made.push((id, b.clone()));
                            b
                        }
                    };
                    out.push((bmp, p.clone()));
                }
                Res::Smoke(out)
            }
            Kind::Arms => {
                let palm = shapes::parse(shapes::PALM);
                let hand = |mirror: bool| -> Result<(ID2D1PathGeometry, Vec<(ID2D1PathGeometry, f32)>)> {
                    let fingers = shapes::FINGERS
                        .iter()
                        .map(|(d, w)| Ok((geometry(factory, &shapes::parse(d), mirror, false)?, *w)))
                        .collect::<Result<Vec<_>>>()?;
                    Ok((geometry(factory, &palm, mirror, true)?, fingers))
                };
                Res::Arms {
                    hands: [hand(false)?, hand(true)?],
                    round: stroke(factory, true, None)?,
                }
            }
            Kind::Fold => {
                let one = |s: &str| -> Result<(IDWriteTextLayout, f64)> {
                    let l = text_layout(&dw, s, &spec.family, 28.0, DWRITE_FONT_WEIGHT_MEDIUM, 0.0, 28.0)?;
                    let w = measure(&l).0.ceil();
                    Ok((l, w))
                };
                Res::Fold([one("6")?, one("9")?])
            }
            Kind::Void => {
                let a = spec.answer;
                let inner = Rect::new(a.x + room as f32 + 8.0, a.y + room as f32, (a.w - 16.0).max(1.0), a.h);
                let copy = text_layout(&dw, &spec.answer_text, &spec.family, 24.0, DWRITE_FONT_WEIGHT_REGULAR, inner.w, inner.h)?;
                // as answer.rs lays the slot out: trailing, cut with an ellipsis
                // SAFETY: setters on a live layout
                unsafe {
                    let _ = copy.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
                    let fmt = dw.CreateTextFormat(
                        &HSTRING::from(spec.family.as_str()),
                        None,
                        DWRITE_FONT_WEIGHT_REGULAR,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        24.0,
                        &HSTRING::from("en-us"),
                    )?;
                    if let Ok(sign) = dw.CreateEllipsisTrimmingSign(&fmt) {
                        let trimming = DWRITE_TRIMMING {
                            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                            delimiter: 0,
                            delimiterCount: 0,
                        };
                        let _ = copy.SetTrimming(&trimming, &sign);
                    }
                }
                let tw = (measure(&copy).0 as f32).min(inner.w);
                let text = Rect::new(inner.right() - tw, inner.y, tw, inner.h);
                Res::Void {
                    copy: (!spec.reduce).then_some(copy),
                    inner,
                    text,
                    dashed: stroke(factory, false, Some(&[2.5, 1.5, 1.0, 1.5]))?,
                }
            }
            Kind::Euler => {
                let note = text_layout(&dw, anim::NOTE_TEXT, &spec.text_family, 12.0, DWRITE_FONT_WEIGHT_REGULAR, 0.0, 18.0)?;
                let (w, _) = measure(&note);
                let seed = Instant::now().elapsed().subsec_nanos() as u64 ^ 0x9e37_79b9_7f4a_7c15;
                Res::Euler {
                    note,
                    note_size: (w.ceil() + 16.0, 18.0 + 6.0),
                    bits: anim::burst(seed),
                }
            }
            Kind::PiDigits => {
                let a = spec.answer;
                let right = f64::from(a.right() - 8.0) + room;
                let zero = text_layout(&dw, "0", &spec.text_family, 11.0, DWRITE_FONT_WEIGHT_REGULAR, 0.0, 15.0)?;
                tabular(&dw, &zero, 1);
                let char_w = measure(&zero).0;
                let all = anim::grouped(&spec.pi);
                let room_w = right - (f64::from(bar.x) + 16.0);
                let shown = anim::fitted(&all, char_w, room_w).to_string();
                let count = shown.chars().count();
                let layout = text_layout(&dw, &shown, &spec.text_family, 11.0, DWRITE_FONT_WEIGHT_REGULAR, 0.0, 15.0)?;
                tabular(&dw, &layout, count as u32);
                let (w, h) = measure(&layout);
                let (w, h) = (w.ceil(), h.ceil().max(15.0));
                let composer_bottom = f64::from(spec.composer.bottom()) + room;
                let y = (f64::from(a.bottom()) + room + 1.0).min(composer_bottom - h - 1.0);
                Res::PiDigits {
                    layout,
                    size: (w, h),
                    char_w,
                    count,
                    at: (right - w, y),
                }
            }
            Kind::Roll => {
                let Some((px, w, h)) = &spec.picture else {
                    return Err(windows::core::Error::from_hresult(windows::core::HRESULT(-1)));
                };
                let bmp = bitmap(&rt, px, *w, *h, spec.dpi, true)?;
                let props = D2D1_BITMAP_BRUSH_PROPERTIES {
                    extendModeX: D2D1_EXTEND_MODE_CLAMP,
                    extendModeY: D2D1_EXTEND_MODE_CLAMP,
                    interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                };
                // SAFETY: a brush over our bitmap
                let brush = unsafe { rt.CreateBitmapBrush(&bmp, Some(&props), None) }?;
                Res::Roll(brush)
            }
        };
        Ok(Scene {
            spec,
            rt,
            brush,
            room,
            res,
        })
    }

    fn bar(&self) -> Rect {
        Rect::new(self.room as f32, self.room as f32, self.spec.bar_w, self.spec.bar_h)
    }

    fn color(&self, c: D2D1_COLOR_F) -> &ID2D1SolidColorBrush {
        // SAFETY: a live brush
        unsafe { self.brush.SetColor(&c) };
        &self.brush
    }

    fn set(&self, a: Aff) {
        // SAFETY: inside BeginDraw
        unsafe { self.rt.SetTransform(&a.m()) };
    }

    /// One frame at `t` seconds.
    fn draw(&self, t: f64) {
        let rt = &self.rt;
        let bar = self.bar();
        let (bx, by, bw, bh) = (
            f64::from(bar.x),
            f64::from(bar.y),
            f64::from(bar.w),
            f64::from(bar.h),
        );
        let reduce = self.spec.reduce;
        // SAFETY: every D2D call below is between this BeginDraw and its EndDraw, on our own target
        unsafe {
            self.set(Aff::ID);
            rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            match &self.res {
                Res::Smoke(puffs) => {
                    // the layer: 56 px either side of the bar, puffs placed from the bar's top (FourTwentySmoke)
                    let bar_x = 56.0;
                    let layer_w = bw + 2.0 * bar_x;
                    let left0 = bx - bar_x;
                    for (bmp, p) in puffs {
                        let left = if p.at == -1.0 {
                            bar_x + p.x
                        } else if p.at == -2.0 {
                            layer_w - bar_x + p.x
                        } else {
                            bar_x + (layer_w - 2.0 * bar_x) * p.at + p.x
                        };
                        let f = anim::puff(t, p.delay, p.drift, p.rise, p.sway, reduce);
                        if f.opacity <= 0.001 {
                            continue;
                        }
                        let (cx, cy) = (left0 + left + f.dx, by + p.y + f.dy);
                        let w = (p.w + 2.0 * shapes::PUFF_PAD) * f.scale;
                        let h = (p.h + 2.0 * shapes::PUFF_PAD) * f.scale;
                        rt.DrawBitmap(
                            bmp,
                            Some(&rect(cx - w / 2.0, cy - h / 2.0, w, h)),
                            f.opacity as f32,
                            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                            None,
                        );
                    }
                }
                Res::Arms { hands, round } => {
                    // the bar's top 72 px, clipped (overflow hidden)
                    rt.PushAxisAlignedClip(&rect(bx, by, bw, 72.0), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    for (i, (palm, fingers)) in hands.iter().enumerate() {
                        let right = i == 1;
                        let (ty, deg, opacity) = anim::hand(t, right, reduce);
                        let left = if right { bw / 2.0 + 48.0 } else { bw / 2.0 - 92.0 };
                        let (ox, oy) = (
                            f64::from(shapes::HAND_W) * if right { 0.8 } else { 0.2 },
                            f64::from(shapes::HAND_H) * 0.7,
                        );
                        let place = Aff::translate(-ox, -oy)
                            .then(Aff::rotate(deg.to_radians()))
                            .then(Aff::translate(bx + left + ox, by + ty + oy));
                        // a soft shadow under it, then a dark rim (drop-shadows 0 1px 3px .3 and 0 0 1px .65)
                        for (dy, grow, a) in [(1.0, 4.0, 0.12 * opacity), (0.0, 2.0, 0.5 * opacity)] {
                            self.set(place.then(Aff::translate(0.0, dy)));
                            let b = self.color(rgba(0.0, 0.0, 0.0, a));
                            rt.DrawGeometry(palm, b, grow as f32, Some(round));
                            rt.FillGeometry(palm, b, None);
                            for (f, w) in fingers {
                                rt.DrawGeometry(f, b, *w + grow as f32, Some(round));
                            }
                        }
                        self.set(place);
                        let b = self.color(rgba(0xe4 as f32 / 255.0, 0xe4 as f32 / 255.0, 0xea as f32 / 255.0, opacity));
                        rt.FillGeometry(palm, b, None);
                        for (f, w) in fingers {
                            rt.DrawGeometry(f, b, *w, Some(round));
                        }
                    }
                    self.set(Aff::ID);
                    rt.PopAxisAlignedClip();
                }
                Res::Fold(digits) => {
                    for (i, (layout, w)) in digits.iter().enumerate() {
                        let spin = if i == 0 { 1.0 } else { -1.0 };
                        let (deg, opacity) = anim::digit(t, spin, reduce);
                        let x = if i == 0 { bx + bw * 0.14 } else { bx + bw - bw * 0.14 - w };
                        let y = by + bh / 2.0;
                        let (cx, cy) = (x + w / 2.0, y + 14.0);
                        self.set(Aff::about(cx, cy, 1.0, deg.to_radians(), cx, cy));
                        let b = self.color(faded(self.spec.answer_color, opacity));
                        rt.DrawTextLayout(v(x, y), layout, b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                    }
                }
                Res::Void { copy, inner, text, dashed } => {
                    let r = if reduce { 8.0 } else { 12.0 };
                    let mid_y = f64::from(inner.y + inner.h / 2.0);
                    let (cx, cy) = if reduce {
                        (f64::from(text.x) - r - 10.0, mid_y)
                    } else {
                        (f64::from(text.x + text.w / 2.0), mid_y)
                    };
                    if let Some(copy) = copy {
                        // the real answer hides under a cover of the bar's own colour while its copy falls in
                        let cover = anim::veil(t);
                        if cover > 0.0 {
                            let a = self.spec.answer;
                            let b = self.color(faded(self.spec.bg, cover));
                            rt.FillRectangle(&rect(f64::from(a.x) + self.room, f64::from(a.y) + self.room, f64::from(a.w), f64::from(a.h)), b);
                        }
                        // the spiral starts where the slot drew the text and ends in the hole, on its centre
                        let start = (f64::from(text.x + text.w / 2.0), mid_y);
                        let (x, y, s, turn, o) = anim::fall(t, start, (cx, cy));
                        if o > 0.0 {
                            self.set(Aff::about(start.0, start.1, s, turn, x, y));
                            let b = self.color(faded(self.spec.answer_color, o));
                            rt.DrawTextLayout(v(f64::from(inner.x), f64::from(inner.y)), copy, b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        }
                    }
                    let (scale, opacity, ring_turn) = anim::hole(t, reduce);
                    if opacity > 0.0 {
                        let ring_r = (1.45 * r) as f32;
                        // the warm disk: a glow, then the dashed ring turning
                        self.set(Aff::about(cx, cy, scale, ring_turn, cx, cy));
                        let e = D2D1_ELLIPSE { point: v(cx, cy), radiusX: ring_r, radiusY: ring_r };
                        rt.DrawEllipse(&e, self.color(rgba(1.0, 0.5, 0.2, 0.22 * opacity)), 6.0, None);
                        rt.DrawEllipse(&e, self.color(rgba(1.0, 0.62, 0.25, 0.85 * opacity)), 2.0, Some(dashed));
                        // the core: black with a violet glow
                        self.set(Aff::about(cx, cy, scale, 0.0, cx, cy));
                        for (grow, a) in [(5.0, 0.12), (3.0, 0.2), (1.5, 0.35)] {
                            let g = D2D1_ELLIPSE { point: v(cx, cy), radiusX: (r + grow) as f32, radiusY: (r + grow) as f32 };
                            rt.FillEllipse(&g, self.color(rgba(0.55, 0.3, 1.0, a * opacity)));
                        }
                        let core = D2D1_ELLIPSE { point: v(cx, cy), radiusX: r as f32, radiusY: r as f32 };
                        rt.FillEllipse(&core, self.color(rgba(0.0, 0.0, 0.0, opacity)));
                    }
                }
                Res::Euler { note, note_size, bits } => {
                    let a = self.spec.answer;
                    let (fx, fy) = if a.w > 0.0 {
                        (f64::from(a.right()) + self.room - 24.0, f64::from(a.y + a.h / 2.0) + self.room)
                    } else {
                        (bx + bw / 2.0, by + f64::from(self.spec.composer.y) + 36.0)
                    };
                    // the note: a pill of the bar's colour just under the bar
                    let o = anim::note(t, reduce);
                    if o > 0.0 {
                        let (pw, ph) = *note_size;
                        let x = fx.clamp(bx + pw / 2.0, (bx + bw - pw / 2.0).max(bx + pw / 2.0)) - pw / 2.0;
                        let y = by + bh + 6.0;
                        let pill = D2D1_ROUNDED_RECT { rect: rect(x, y, pw, ph), radiusX: (ph / 2.0) as f32, radiusY: (ph / 2.0) as f32 };
                        rt.FillRoundedRectangle(&pill, self.color(faded(self.spec.bg, 0.9 * o)));
                        rt.DrawTextLayout(v(x + 8.0, y + 3.0), note, self.color(faded(self.spec.muted, o)), D2D1_DRAW_TEXT_OPTIONS_NONE);
                    }
                    if !reduce {
                        for bit in bits {
                            let Some(at) = bit.at(t) else { continue };
                            let (r, g, b) = anim::CONFETTI[bit.colour];
                            self.set(Aff::about(0.0, 0.0, bit.scale, at.turn, fx + at.dx, fy + at.dy));
                            rt.FillRectangle(&rect(-4.0, -2.0, 8.0, 4.0), self.color(rgba(r, g, b, at.opacity)));
                        }
                    }
                }
                Res::PiDigits { layout, size, char_w, count, at } => {
                    let o = anim::digits_opacity(t, reduce);
                    if o > 0.0 {
                        let shown = anim::digits_shown(t, *count, reduce);
                        let w = (shown as f64 * char_w).min(size.0);
                        rt.PushAxisAlignedClip(&rect(at.0 - 1.0, at.1, w + 1.0, size.1), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                        rt.DrawTextLayout(v(at.0, at.1), layout, self.color(faded(self.spec.muted, o)), D2D1_DRAW_TEXT_OPTIONS_NONE);
                        rt.PopAxisAlignedClip();
                    }
                }
                Res::Roll(picture) => {
                    let (turn, s) = anim::roll(t);
                    let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
                    let place = Aff::about(cx, cy, s, turn, cx, cy);
                    // a soft shadow under the picture
                    for k in 0..4 {
                        let g = 3.0 * f64::from(k);
                        self.set(place.then(Aff::translate(0.0, 6.0)));
                        let r = D2D1_ROUNDED_RECT { rect: rect(bx - g, by - g, bw + 2.0 * g, bh + 2.0 * g), radiusX: (8.0 + g) as f32, radiusY: (8.0 + g) as f32 };
                        rt.FillRoundedRectangle(&r, self.color(rgba(0.0, 0.0, 0.0, 0.06)));
                    }
                    self.set(place);
                    picture.SetTransform(&Aff::translate(bx, by).m());
                    let r = D2D1_ROUNDED_RECT { rect: rect(bx, by, bw, bh), radiusX: 8.0, radiusY: 8.0 };
                    rt.FillRoundedRectangle(&r, picture);
                }
            }
            self.set(Aff::ID);
        }
    }
}

/// tabular figures, so π's digits line up as the Mac's monospaced ones do
fn tabular(dw: &IDWriteFactory, layout: &IDWriteTextLayout, len: u32) {
    // SAFETY: plain DirectWrite calls on live objects
    unsafe {
        if let Ok(t) = dw.CreateTypography() {
            let _ = t.AddFontFeature(DWRITE_FONT_FEATURE {
                nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
                parameter: 1,
            });
            let _ = layout.SetTypography(&t, DWRITE_TEXT_RANGE { startPosition: 0, length: len.max(1) });
        }
    }
}

fn bar_rect(bar: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    // SAFETY: a live out rect
    unsafe { GetWindowRect(bar, &mut r) }.ok()?;
    Some(r)
}

const CLASS: windows::core::PCWSTR = w!("QCalcEffect");

fn run(spec: Spec, shared: &Shared) -> Result<()> {
    let started = Instant::now();
    let bar = HWND(spec.bar as *mut _);
    let total = spec.play_ms / 1000.0 + if spec.kind == Kind::Roll { 0.0 } else { 0.3 };
    let scale = f64::from(spec.dpi) / 96.0;
    let room = spec.kind.room(f64::from(spec.bar_w), f64::from(spec.bar_h));
    let room_px = (room * scale).round();
    let room = room_px / scale;
    let w = (f64::from(spec.bar_w) * scale).round() as i32 + 2 * room_px as i32;
    let h = (f64::from(spec.bar_h) * scale).round() as i32 + 2 * room_px as i32;
    let Some(r0) = bar_rect(bar) else {
        return Ok(());
    };
    let kind = spec.kind;
    // SAFETY: plain factory creation, released when this returns
    let factory: ID2D1Factory = unsafe {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&D2D1_FACTORY_OPTIONS::default()))
    }?;
    let scene = Scene::new(spec, &factory, room)?;
    let dib = Dib::new(w, h)?;
    let bind = RECT { left: 0, top: 0, right: w, bottom: h };
    // SAFETY: the DIB's DC outlives the target's use of it (scene drops before dib)
    unsafe { scene.rt.BindDC(dib.mem, &bind) }?;
    // SAFETY: a class with our trivial wndproc (registering twice fails harmlessly) and a window we destroy below
    let hwnd = unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(overlay_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("Q Calc effect"),
            WS_POPUP,
            r0.left - room_px as i32,
            r0.top - room_px as i32,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };
    shared.hwnd.store(hwnd.0 as usize, Ordering::SeqCst);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let size = SIZE { cx: w, cy: h };
    let src = POINT { x: 0, y: 0 };
    let mut shown = false;
    let mut frames = 0u32;
    let mut draw_ms = 0.0f64;
    let mut hidden = false;
    let mut last = Instant::now() - Duration::from_millis(100);
    let t0 = Instant::now();
    crate::qlog!(
        "effect {} up in {:.1} ms ({w}x{h} px)",
        kind.name(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    loop {
        if shared.cancel.load(Ordering::SeqCst) {
            break;
        }
        // SAFETY: a plain query on our bar
        if !unsafe { IsWindowVisible(bar) }.as_bool() {
            hidden = true;
            break;
        }
        let t = t0.elapsed().as_secs_f64();
        if t >= total {
            break;
        }
        // cap near 60 fps on faster displays
        if last.elapsed() >= Duration::from_millis(14) {
            last = Instant::now();
            let d = Instant::now();
            // SAFETY: Begin/EndDraw paired on our target
            unsafe {
                scene.rt.BeginDraw();
                scene.draw(t);
                scene.rt.EndDraw(None, None)?;
            }
            // the bar may have moved: follow it
            let r = bar_rect(bar).unwrap_or(r0);
            let at = POINT {
                x: r.left - room_px as i32,
                y: r.top - room_px as i32,
            };
            // SAFETY: our window and DIB, live locals
            unsafe {
                UpdateLayeredWindow(
                    hwnd,
                    Some(dib.screen),
                    Some(&at),
                    Some(&size),
                    Some(dib.mem),
                    Some(&src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                )?;
            }
            draw_ms += d.elapsed().as_secs_f64() * 1000.0;
            frames += 1;
            if !shown {
                shown = true;
                // SAFETY: our window; NOACTIVATE keeps the keys with the bar
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                if kind == Kind::Roll {
                    // the picture is up: hide the bar under it (it keeps the keys), unless the UI just cancelled
                    cloak(bar.0 as usize, true);
                    shared.cloaked.store(true, Ordering::SeqCst);
                    if shared.cancel.load(Ordering::SeqCst) {
                        cloak(bar.0 as usize, false);
                    }
                }
            }
        }
        let mut msg = MSG::default();
        // SAFETY: the standard pump for this thread's window
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if DwmFlush().is_err() {
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
    if kind == Kind::Roll && shared.cloaked.load(Ordering::SeqCst) && !shared.cancel.load(Ordering::SeqCst) {
        cloak(bar.0 as usize, false);
    }
    // SAFETY: our own window, on its own thread
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    crate::qlog!(
        "effect {} down after {:.0} ms: {frames} frames, {:.2} ms a frame{}",
        kind.name(),
        t0.elapsed().as_secs_f64() * 1000.0,
        if frames > 0 { draw_ms / f64::from(frames) } else { 0.0 },
        if hidden { " (bar hidden)" } else { "" }
    );
    drop(scene);
    drop(dib);
    drop(factory);
    give_back();
    Ok(())
}

/// Asks the heaps to decommit what the effect freed, so private bytes fall back too.
fn give_back() {
    use windows::Win32::System::Memory::{HeapOptimizeResources, HeapSetInformation};
    /// HEAP_OPTIMIZE_RESOURCES_INFORMATION (heapapi.h), version 1
    #[repr(C)]
    struct Optimize {
        version: u32,
        flags: u32,
    }
    let info = Optimize {
        version: 1,
        flags: 0,
    };
    // SAFETY: a documented process-wide request with a live, correctly sized struct
    let _ = unsafe {
        HeapSetInformation(
            None,
            HeapOptimizeResources,
            Some((&info as *const Optimize).cast()),
            std::mem::size_of::<Optimize>(),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_compose_in_order() {
        let a = Aff::translate(10.0, 0.0).then(Aff::scale(2.0));
        // (1, 1) → (11, 1) → (22, 2)
        let p = |m: Aff, x: f32, y: f32| (x * m.0[0] + y * m.0[2] + m.0[4], x * m.0[1] + y * m.0[3] + m.0[5]);
        assert_eq!(p(a, 1.0, 1.0), (22.0, 2.0));
        // a quarter turn clockwise on screen: right becomes down
        let (x, y) = p(Aff::rotate(std::f64::consts::FRAC_PI_2), 1.0, 0.0);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        // about a centre: the centre lands where asked
        let m = Aff::about(5.0, 5.0, 3.0, 1.0, 20.0, 30.0);
        let (x, y) = p(m, 5.0, 5.0);
        assert!((x - 20.0).abs() < 1e-4 && (y - 30.0).abs() < 1e-4);
    }

    #[test]
    fn room_fits_each_effect() {
        assert_eq!(Kind::Smoke.room(680.0, 72.0), 200.0);
        assert!(Kind::Roll.room(680.0, 72.0) > 300.0);
        assert!(Kind::Arms.room(680.0, 72.0) < 50.0);
    }
}
