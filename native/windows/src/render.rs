//! Drawing: the Direct2D and DirectWrite factories, the text formats, the theme's colours, DPI, and the
//! per-window render target.
//!
//! - `Gfx` (one per app, made at launch): the D2D and DWrite factories and the `Fonts`.
//! - `Surface` (one per window): an `ID2D1HwndRenderTarget`, software-rasterised by default (`hardware()` says
//!   why), with `D2D1_PRESENT_OPTIONS_IMMEDIATELY` (no wait for vblank, so a keystroke's frame goes out at once), its DPI set so every coordinate is in DIPs, one solid
//!   brush recoloured per draw. It is created lazily, kept across hides (showing is instant), and dropped by
//!   `release` after the bar has been hidden a while (app.rs) to give memory back.
//! - `Canvas`: what a view's `paint` gets: DIP rects, colours from `Theme`, text layouts.
//! - `Theme`: the web app's CSS colours (native/mac/Sources/Bar/Theme.swift, value for value), light and dark.
//!
//! The UI font is Segoe UI Variable (Display for the big line, Text for small text), falling back to Segoe UI on
//! Windows 10; maths that wants a maths face (the typeset row's fallback, sheets) uses `Fonts::math`, Cambria Math.
//! DirectWrite's system fallback fills any glyph the face lacks (², √, π, Greek...).

use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, HWND};
use windows::Win32::Foundation::{E_FAIL, HMODULE};
use windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_IGNORE;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_IMMEDIATELY, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, ID2D1Bitmap1, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1,
    ID2D1RenderTarget,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS,
    D3D11_CREATE_DEVICE_SINGLETHREADED, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_HIT_TEST_METRICS, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_MWA_NO_ALT_ENTER,
    DXGI_MWA_NO_WINDOW_CHANGES, DXGI_PRESENT, DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
};
use windows::core::{BOOL, HSTRING, Interface, Result};
use windows_numerics::Vector2;

/// The theme's sizes (Theme.swift: --fs-*, --radius*, --gutter, --tape-row-height, composer 72).
pub mod size {
    pub const DISPLAY: f32 = 24.0;
    pub const ANSWER_PART: f32 = 20.0;
    pub const ROW: f32 = 17.0;
    pub const BODY: f32 = 13.0;
    pub const SMALL: f32 = 12.0;
    pub const CAPTION: f32 = 11.0;
    pub const RADIUS: f32 = 12.0;
    pub const RADIUS_CARD: f32 = 10.0;
    pub const RADIUS_S: f32 = 6.0;
    pub const TAPE_ROW: f32 = 36.0;
    pub const TAPE_VISIBLE: usize = 5;
    pub const GUTTER: f32 = 16.0;
    pub const COMPOSER: f32 = 72.0;
    /// --track-display: -0.021em
    pub fn display_tracking(size: f32) -> f32 {
        -0.021 * size
    }
}

pub type Color = D2D1_COLOR_F;

pub const fn rgb(hex: u32, a: f32) -> Color {
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a,
    }
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

/// The bar's colours (src/index.css :root and its dark block).
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color,
    pub bg_tape: Color,
    pub ink: Color,
    pub muted: Color,
    pub answer: Color,
    pub answer_hover: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub line: Color,
    /// ink at 5% / 8%
    pub fill_hover: Color,
    pub fill_selected: Color,
    /// the input's selection
    pub selection: Color,
}

impl Theme {
    pub fn new(dark: bool) -> Theme {
        let pick = |l: u32, d: u32| rgb(if dark { d } else { l }, 1.0);
        let ink = pick(0x1d1d1f, 0xf5f5f7);
        let accent = pick(0x1d7a4c, 0x63d392);
        Theme {
            dark,
            bg: pick(0xffffff, 0x2c2c2e),
            bg_tape: pick(0xf5f5f7, 0x252527),
            ink,
            muted: pick(0x86868b, 0x98989d),
            answer: pick(0x1d1d1f, 0xf5f5f7),
            answer_hover: pick(0x48484a, 0xd1d1d6),
            accent,
            accent_hover: pick(0x159a5a, 0x7ee2a8),
            line: if dark {
                rgb(0xffffff, 0.09)
            } else {
                rgb(0x000000, 0.08)
            },
            fill_hover: with_alpha(ink, 0.05),
            fill_selected: with_alpha(ink, 0.08),
            selection: with_alpha(accent, if dark { 0.38 } else { 0.25 }),
        }
    }

    /// `settings.theme`: "light", "dark", or "system" (the apps' light/dark setting).
    pub fn for_setting(setting: &str) -> Theme {
        crate::contrast::apply(Theme::new(match setting {
            "light" => false,
            "dark" => true,
            _ => system_dark(),
        }))
    }
}

/// Windows' "app mode" (Settings > Personalization > Colors), `AppsUseLightTheme`.
pub fn system_dark() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let mut light: u32 = 1;
    let mut size = 4u32;
    // SAFETY: the out pointers are live locals of the right size
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            &HSTRING::from("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut light as *mut u32).cast()),
            Some(&mut size),
        )
    };
    status.is_ok() && light == 0
}

/// A rectangle in DIPs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn inset(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            (self.w - 2.0 * dx).max(0.0),
            (self.h - 2.0 * dy).max(0.0),
        )
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.x,
            top: self.y,
            right: self.right(),
            bottom: self.bottom(),
        }
    }
}

/// The text formats every view shares.
pub struct Fonts {
    /// the UI family actually found ("Segoe UI Variable Display" or "Segoe UI")
    pub family: String,
    pub text_family: String,
    /// 24, the line and the answer
    pub display: IDWriteTextFormat,
    /// 20, answer parts
    pub answer_part: IDWriteTextFormat,
    /// 17, history rows' answers
    pub row: IDWriteTextFormat,
    /// 13
    pub body: IDWriteTextFormat,
    /// 12, hints
    pub small: IDWriteTextFormat,
    /// 11 medium, badges
    pub caption: IDWriteTextFormat,
    /// Cambria Math 24
    pub math: IDWriteTextFormat,
}

/// The app's factories and fonts.
pub struct Gfx {
    pub d2d: ID2D1Factory,
    pub dw: IDWriteFactory,
    pub fonts: Fonts,
    /// the WARP device flip surfaces draw with, made by the first one
    flip: std::cell::RefCell<Option<FlipDevice>>,
}

/// A WARP (software) D3D11 device, its DXGI factory and the D2D device on it, shared by every flip `Surface`.
/// The window targets already ran on WARP inside Direct2D, so this loads nothing new.
#[derive(Clone)]
struct FlipDevice {
    d3d: ID3D11Device,
    factory: IDXGIFactory2,
    d2d: ID2D1Device,
}

fn has_family(fonts: &IDWriteFontCollection, name: &str) -> bool {
    let mut index = 0u32;
    let mut exists = BOOL(0);
    // SAFETY: out pointers are live locals
    unsafe { fonts.FindFamilyName(&HSTRING::from(name), &mut index, &mut exists) }.is_ok()
        && exists.as_bool()
}

impl Gfx {
    pub fn new() -> Result<Gfx> {
        // SAFETY: plain factory creation; the options struct outlives the call
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
        let family = if found("Segoe UI Variable Display") {
            "Segoe UI Variable Display"
        } else {
            "Segoe UI"
        };
        let text_family = if found("Segoe UI Variable Text") {
            "Segoe UI Variable Text"
        } else {
            "Segoe UI"
        };
        let math_family = if found("Cambria Math") {
            "Cambria Math"
        } else {
            family
        };
        let fonts = Fonts {
            display: format(&dw, family, size::DISPLAY, DWRITE_FONT_WEIGHT_REGULAR)?,
            answer_part: format(&dw, family, size::ANSWER_PART, DWRITE_FONT_WEIGHT_REGULAR)?,
            row: format(&dw, text_family, size::ROW, DWRITE_FONT_WEIGHT_REGULAR)?,
            body: format(&dw, text_family, size::BODY, DWRITE_FONT_WEIGHT_REGULAR)?,
            small: format(&dw, text_family, size::SMALL, DWRITE_FONT_WEIGHT_REGULAR)?,
            caption: format(&dw, text_family, size::CAPTION, DWRITE_FONT_WEIGHT_MEDIUM)?,
            math: format(&dw, math_family, size::DISPLAY, DWRITE_FONT_WEIGHT_REGULAR)?,
            family: family.into(),
            text_family: text_family.into(),
        };
        Ok(Gfx {
            d2d,
            dw,
            fonts,
            flip: std::cell::RefCell::new(None),
        })
    }

    fn flip_device(&self) -> Result<FlipDevice> {
        if let Some(d) = self.flip.borrow().as_ref() {
            return Ok(d.clone());
        }
        // made ahead on a thread of its own, at launch or as the bar started to show (warp.rs)
        if let Some((d3d, factory)) = crate::warp::take() {
            // SAFETY: live objects the prefetch thread made and let go of; the cast and the call are on them
            let dev = unsafe {
                let dxgi: IDXGIDevice = d3d.cast()?;
                let d2d = self.d2d.cast::<ID2D1Factory1>()?.CreateDevice(&dxgi)?;
                FlipDevice { d3d, factory, d2d }
            };
            *self.flip.borrow_mut() = Some(dev.clone());
            return Ok(dev);
        }
        let mut d3d = None;
        // SAFETY: plain device creation into a live out pointer; the casts and lookups are on live objects
        let dev = unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                // Direct2D needs BGRA; only the UI thread draws
                D3D11_CREATE_DEVICE_BGRA_SUPPORT
                    | D3D11_CREATE_DEVICE_SINGLETHREADED
                    | D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let d3d: ID3D11Device = d3d.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
            let dxgi: IDXGIDevice = d3d.cast()?;
            let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
            let d2d = self.d2d.cast::<ID2D1Factory1>()?.CreateDevice(&dxgi)?;
            FlipDevice { d3d, factory, d2d }
        };
        *self.flip.borrow_mut() = Some(dev.clone());
        // WARP's DLL stays loaded across hides (warp.rs)
        crate::warp::held();
        Ok(dev)
    }

    /// The device was lost: the next flip surface makes a new one.
    pub fn drop_flip_device(&self) {
        *self.flip.borrow_mut() = None;
        crate::warp::dropped();
    }

    /// A layout of one line of `text` (no wrapping), vertically centred in `h`. `max_w` 0 means unbounded.
    pub fn layout(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        max_w: f32,
        h: f32,
    ) -> Option<IDWriteTextLayout> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let w = if max_w > 0.0 { max_w } else { 100_000.0 };
        // SAFETY: wide outlives the call; the format is a live COM object
        let layout = unsafe { self.dw.CreateTextLayout(&wide, format, w, h) }.ok()?;
        // SAFETY: setters on a live layout
        unsafe {
            let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        }
        Some(layout)
    }

    /// `layout`, right- (or left-) aligned in `w` and cut with an ellipsis when it doesn't fit.
    pub fn layout_in(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        w: f32,
        h: f32,
        align: DWRITE_TEXT_ALIGNMENT,
    ) -> Option<IDWriteTextLayout> {
        let layout = self.layout(text, format, w.max(1.0), h)?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        // SAFETY: a live layout; the ellipsis sign is created from the same factory
        unsafe {
            let _ = layout.SetTextAlignment(align);
            if let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(format) {
                let _ = layout.SetTrimming(&trimming, &sign);
            }
        }
        Some(layout)
    }
}

pub fn format(
    dw: &IDWriteFactory,
    family: &str,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
) -> Result<IDWriteTextFormat> {
    // SAFETY: the strings outlive the call
    unsafe {
        dw.CreateTextFormat(
            &HSTRING::from(family),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            &HSTRING::from("en-us"),
        )
    }
}

/// A layout's width (with trailing spaces) and height.
pub fn measure(layout: &IDWriteTextLayout) -> (f32, f32) {
    let mut m = DWRITE_TEXT_METRICS::default();
    // SAFETY: out pointer is a live local
    if unsafe { layout.GetMetrics(&mut m) }.is_err() {
        return (0.0, 0.0);
    }
    (m.widthIncludingTrailingWhitespace, m.height)
}

/// The x of the caret before UTF-16 position `pos` in a one-line layout.
pub fn caret_x(layout: &IDWriteTextLayout, pos: u32) -> f32 {
    let (mut x, mut y) = (0f32, 0f32);
    let mut m = DWRITE_HIT_TEST_METRICS::default();
    // SAFETY: out pointers are live locals
    let _ = unsafe { layout.HitTestTextPosition(pos, false, &mut x, &mut y, &mut m) };
    x
}

/// The UTF-16 position nearest to `x` (the caret goes before or after the hit glyph, whichever is closer).
pub fn hit(layout: &IDWriteTextLayout, x: f32, y: f32) -> u32 {
    let (mut trailing, mut inside) = (BOOL(0), BOOL(0));
    let mut m = DWRITE_HIT_TEST_METRICS::default();
    // SAFETY: out pointers are live locals
    if unsafe { layout.HitTestPoint(x, y, &mut trailing, &mut inside, &mut m) }.is_err() {
        return 0;
    }
    m.textPosition + if trailing.as_bool() { m.length } else { 0 }
}

/// Whether render targets use the GPU. Software (Direct2D's own rasteriser, no D3D device) is the default: for a
/// window this small it draws a frame as fast (keystroke medians measured equal), it is made in ~15 ms where a
/// hardware target took 150-300 ms (D3D and the GPU driver load on the first showing: warm launch went from ~360
/// to ~65 ms), and it keeps ~50 MB of driver and device memory out of the process (private bytes 69 → 18 MB).
/// `QCALC_D2D=hardware` switches, for a panel that turns out to need the GPU (big graphs, games).
pub fn hardware() -> bool {
    std::env::var("QCALC_D2D").is_ok_and(|v| v.eq_ignore_ascii_case("hardware"))
}

/// What a `Surface` draws into.
enum Target {
    /// Direct2D's own window target (software unless `hardware()`)
    Hwnd(ID2D1HwndRenderTarget),
    /// a device context on the shared WARP device, into a flip-model swap chain (`Surface::flip`)
    Flip(Flip),
}

struct Flip {
    chain: IDXGISwapChain1,
    ctx: ID2D1DeviceContext,
    /// the back buffer as the context's target, for buffers of `size`; made again after a resize
    bitmap: Option<(ID2D1Bitmap1, (u32, u32))>,
}

/// A window's render target, made when first needed and kept across hides.
pub struct Surface {
    hwnd: HWND,
    rt: Option<(Target, ID2D1SolidColorBrush)>,
    pub dpi: f32,
    pub px: (u32, u32),
    flip: bool,
}

impl Surface {
    pub fn new(hwnd: HWND, dpi: u32) -> Surface {
        Surface {
            hwnd,
            rt: None,
            dpi: dpi as f32,
            px: (1, 1),
            flip: false,
        }
    }

    /// A surface that presents through a flip-model swap chain on the shared WARP device (render.rs `FlipDevice`)
    /// rather than an `ID2D1HwndRenderTarget`: the same software rasteriser, but a frame is handed to DWM in about
    /// half the time (no GDI blit; PERFORMANCE.md). For windows without child windows (the bar, settings, the
    /// sheets). The window target can also go blank for good: once it reports the window occluded it skips every
    /// present while EndDraw still succeeds, which left the settings window white.
    /// `QCALC_D2D=hwnd` (or `hardware`) keeps the window target.
    pub fn new_flip(hwnd: HWND, dpi: u32) -> Surface {
        let flip = std::env::var("QCALC_D2D").map_or(true, |v| v.is_empty());
        Surface {
            flip,
            ..Surface::new(hwnd, dpi)
        }
    }

    pub fn scale(&self) -> f32 {
        self.dpi / 96.0
    }

    pub fn is_live(&self) -> bool {
        self.rt.is_some()
    }

    fn ensure(&mut self, gfx: &Gfx) -> Result<()> {
        if self.rt.is_some() {
            return Ok(());
        }
        if self.flip {
            match self.make_flip(gfx) {
                Ok(t) => {
                    self.rt = Some(t);
                    return Ok(());
                }
                Err(e) => {
                    crate::qlog!("flip surface unavailable ({e}); drawing with a window target");
                    self.flip = false;
                }
            }
        }
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: if hardware() {
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
                width: self.px.0.max(1),
                height: self.px.1.max(1),
            },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };
        // SAFETY: the property structs outlive the call; hwnd is our live window
        let rt = unsafe { gfx.d2d.CreateHwndRenderTarget(&props, &hwnd_props) }?;
        // SAFETY: a live render target
        let brush = unsafe {
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.CreateSolidColorBrush(&rgb(0, 1.0), None)
        }?;
        self.rt = Some((Target::Hwnd(rt), brush));
        Ok(())
    }

    /// A swap chain for our window on the shared device, and a device context to draw into it.
    fn make_flip(&self, gfx: &Gfx) -> Result<(Target, ID2D1SolidColorBrush)> {
        let dev = gfx.flip_device()?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: self.px.0.max(1),
            Height: self.px.1.max(1),
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            // a buffer the window outgrew isn't stretched (as the window target's wasn't)
            Scaling: DXGI_SCALING_NONE,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };
        // SAFETY: the device and window are live; the descriptions are locals
        unsafe {
            let chain = dev
                .factory
                .CreateSwapChainForHwnd(&dev.d3d, self.hwnd, &desc, None, None)?;
            // no Alt+Enter full screen, no DXGI watching the window's messages
            let _ = dev.factory.MakeWindowAssociation(
                self.hwnd,
                DXGI_MWA_NO_ALT_ENTER | DXGI_MWA_NO_WINDOW_CHANGES,
            );
            let ctx = dev
                .d2d
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            ctx.SetDpi(self.dpi, self.dpi);
            let brush = ctx.CreateSolidColorBrush(&rgb(0, 1.0), None)?;
            Ok((
                Target::Flip(Flip {
                    chain,
                    ctx,
                    bitmap: None,
                }),
                brush,
            ))
        }
    }

    /// The window's client size changed (physical pixels).
    pub fn resize(&mut self, w: u32, h: u32) {
        self.px = (w.max(1), h.max(1));
        // a swap chain's buffers follow at the next draw
        if let Some((Target::Hwnd(rt), _)) = &self.rt {
            let size = D2D_SIZE_U {
                width: self.px.0,
                height: self.px.1,
            };
            // SAFETY: a live target; the size is a local
            if unsafe { rt.Resize(&size) }.is_err() {
                self.rt = None;
            }
        }
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi as f32;
        if let Some((t, _)) = &self.rt {
            // SAFETY: a live target
            unsafe {
                match t {
                    Target::Hwnd(rt) => rt.SetDpi(self.dpi, self.dpi),
                    Target::Flip(f) => f.ctx.SetDpi(self.dpi, self.dpi),
                }
            }
        }
    }

    /// Drops the target and its device resources (memory back while hidden); the next draw makes them again.
    pub fn release(&mut self) {
        self.rt = None;
    }

    /// One frame. A lost device drops the target, and the next draw makes a new one.
    pub fn draw(
        &mut self,
        gfx: &Gfx,
        theme: &Theme,
        paint: impl FnOnce(&mut Canvas),
    ) -> Result<()> {
        self.ensure(gfx)?;
        let px = self.px;
        let Some((t, brush)) = &mut self.rt else {
            return Ok(());
        };
        let rt: &ID2D1RenderTarget = match t {
            Target::Hwnd(rt) => rt,
            Target::Flip(f) => {
                if f.bitmap.as_ref().is_none_or(|b| b.1 != px) {
                    // SAFETY: the old target is let go before the buffers are resized; the new one wraps buffer 0,
                    // which D3D11 keeps pointing at the back buffer across presents
                    unsafe {
                        f.ctx.SetTarget(None);
                        f.bitmap = None;
                        let (w, h) = (px.0.max(1), px.1.max(1));
                        if f.chain
                            .GetDesc1()
                            .is_ok_and(|d| (d.Width, d.Height) != (w, h))
                        {
                            f.chain.ResizeBuffers(
                                0,
                                w,
                                h,
                                DXGI_FORMAT_UNKNOWN,
                                DXGI_SWAP_CHAIN_FLAG(0),
                            )?;
                        }
                        let buffer: IDXGISurface = f.chain.GetBuffer(0)?;
                        let props = D2D1_BITMAP_PROPERTIES1 {
                            pixelFormat: D2D1_PIXEL_FORMAT {
                                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                                alphaMode: D2D1_ALPHA_MODE_IGNORE,
                            },
                            dpiX: self.dpi,
                            dpiY: self.dpi,
                            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET
                                | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                            ..Default::default()
                        };
                        let bitmap = f.ctx.CreateBitmapFromDxgiSurface(&buffer, Some(&props))?;
                        f.ctx.SetTarget(&bitmap);
                        f.bitmap = Some((bitmap, px));
                    }
                }
                // SAFETY: a live context
                unsafe { f.ctx.SetDpi(self.dpi, self.dpi) };
                &f.ctx
            }
        };
        let scale = self.dpi / 96.0;
        let mut canvas = Canvas {
            rt,
            brush,
            gfx,
            theme,
            width: px.0 as f32 / scale,
            height: px.1 as f32 / scale,
        };
        // SAFETY: Begin/EndDraw on a live target, paired
        unsafe { rt.BeginDraw() };
        paint(&mut canvas);
        // SAFETY: as above
        let mut end = unsafe { rt.EndDraw(None, None) };
        if end.is_ok()
            && let Target::Flip(f) = t
        {
            // SAFETY: a live swap chain; no vsync wait, as D2D1_PRESENT_OPTIONS_IMMEDIATELY
            end = unsafe { f.chain.Present(0, DXGI_PRESENT(0)) }.ok();
        }
        if let Err(e) = &end
            && (e.code() == D2DERR_RECREATE_TARGET
                || e.code() == DXGI_ERROR_DEVICE_REMOVED
                || e.code() == DXGI_ERROR_DEVICE_RESET)
        {
            if matches!(t, Target::Flip(_)) {
                gfx.drop_flip_device();
            }
            self.rt = None;
            return Ok(());
        }
        end
    }
}

/// What a view paints with: DIP coordinates, the theme, text layouts.
pub struct Canvas<'a> {
    rt: &'a ID2D1RenderTarget,
    brush: &'a ID2D1SolidColorBrush,
    pub gfx: &'a Gfx,
    pub theme: &'a Theme,
    /// the window in DIPs
    pub width: f32,
    pub height: f32,
}

impl Canvas<'_> {
    fn paint(&self, c: Color) -> &ID2D1SolidColorBrush {
        // SAFETY: a live brush
        unsafe { self.brush.SetColor(&c) };
        self.brush
    }

    /// The render target, for views that draw geometry (graph.rs, systems.rs); only inside the frame.
    pub fn target(&self) -> &ID2D1RenderTarget {
        self.rt
    }

    /// The frame's one brush, recoloured to `c`.
    pub fn brush(&self, c: Color) -> &ID2D1SolidColorBrush {
        self.paint(c)
    }

    /// A brush of its own in `c`, for a text layout's drawing effects (a coloured range in one draw).
    pub fn own_brush(&self, c: Color) -> Option<ID2D1SolidColorBrush> {
        // SAFETY: a brush from our live target
        unsafe { self.rt.CreateSolidColorBrush(&c, None) }.ok()
    }

    pub fn clear(&self, c: Color) {
        // SAFETY: inside BeginDraw/EndDraw
        unsafe { self.rt.Clear(Some(&c)) };
    }

    pub fn fill(&self, r: Rect, c: Color) {
        let b = self.paint(c);
        // SAFETY: inside BeginDraw/EndDraw; the rect is a local
        unsafe { self.rt.FillRectangle(&r.d2d(), b) };
    }

    pub fn fill_round(&self, r: Rect, radius: f32, c: Color) {
        let b = self.paint(c);
        let rr = D2D1_ROUNDED_RECT {
            rect: r.d2d(),
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: as above
        unsafe { self.rt.FillRoundedRectangle(&rr, b) };
    }

    pub fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Color) {
        let b = self.paint(c);
        // SAFETY: as above
        unsafe {
            self.rt.DrawLine(
                Vector2 { X: x0, Y: y0 },
                Vector2 { X: x1, Y: y1 },
                b,
                width,
                None,
            )
        };
    }

    /// A layout with its top-left at (x, y), in `c` (colour fonts allowed, for emoji).
    pub fn text(&self, layout: &IDWriteTextLayout, x: f32, y: f32, c: Color) {
        let b = self.paint(c);
        // SAFETY: as above
        unsafe {
            self.rt.DrawTextLayout(
                Vector2 { X: x, Y: y },
                layout,
                b,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            )
        };
    }

    /// Plain text in a rect, without colour fonts (cheapest).
    pub fn text_plain(&self, layout: &IDWriteTextLayout, x: f32, y: f32, c: Color) {
        let b = self.paint(c);
        // SAFETY: as above
        unsafe {
            self.rt.DrawTextLayout(
                Vector2 { X: x, Y: y },
                layout,
                b,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            )
        };
    }

    /// The render target, for drawing the helpers here don't cover (geometries, strokes, layers).
    pub fn rt(&self) -> &ID2D1RenderTarget {
        self.rt
    }

    /// Clips what follows to `r` until `pop_clip`.
    pub fn push_clip(&self, r: Rect) {
        // SAFETY: paired with pop_clip by the caller, inside a draw
        unsafe {
            self.rt
                .PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_ALIASED)
        };
    }

    pub fn pop_clip(&self) {
        // SAFETY: after push_clip
        unsafe { self.rt.PopAxisAlignedClip() };
    }

    /// The target and the brush, for a view that draws geometry itself (effects.rs's Pi Day slice).
    pub fn raw(&self) -> (&ID2D1RenderTarget, &ID2D1SolidColorBrush) {
        (self.rt, self.brush)
    }
}
