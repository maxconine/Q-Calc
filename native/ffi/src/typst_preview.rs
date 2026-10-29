//! The typeset maths preview (`qcalc-typst`), behind the `typst` feature (WP-TYPST-RENDER).
//!
//! ```swift
//! let typst = TypstRenderer()                       // at launch, then:
//! DispatchQueue.global(qos: .utility).async { _ = typst.warmUp() }
//! // per keystroke, on the serial engine queue, after engine.typstSource(...)
//! let image = try typst.render(source: doc, options: TypstRenderOptions(pixelPerPt: Float(window.backingScaleFactor),
//!     maxWidthPt: Float(row.width), maxHeightPt: 72, color: labelRGBA))
//! // or, resolution independent and recoloured for free on an appearance change:
//! let paths = try typst.paths(source: doc)
//! ```
//!
//! A `TypstError.Compile` is mid-edit input: keep showing the last image, dimmed, as the web preview does.

use std::sync::Arc;

use qcalc_typst::vector::{Cap, Join, Paint, Style, Verb};
use qcalc_typst::{Error, RenderOptions, Renderer};

#[derive(Debug, uniffi::Error)]
pub enum TypstError {
    /// Typst rejected the source (mid-edit input): its first error, then hints.
    Compile { message: String },
    /// Typst panicked; the renderer reset itself.
    Crashed { message: String },
}

impl std::fmt::Display for TypstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypstError::Compile { message } => write!(f, "typst: {message}"),
            TypstError::Crashed { message } => write!(f, "typst crashed: {message}"),
        }
    }
}

impl std::error::Error for TypstError {}

impl From<Error> for TypstError {
    fn from(e: Error) -> Self {
        match e {
            Error::Compile { message } => TypstError::Compile { message },
            Error::Crashed { message } => TypstError::Crashed { message },
        }
    }
}

/// Straight (not premultiplied) sRGB.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct TypstColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct TypstRenderOptions {
    /// The backing scale factor (2 on Retina): one Typst point is one UI point, as in the web view.
    pub pixel_per_pt: f32,
    /// Shrink to fit this width in points (`max-width: 100%`); 0 = no limit.
    pub max_width_pt: f32,
    /// Shrink to fit this height in points (the web preview's 72); 0 = no limit.
    pub max_height_pt: f32,
    /// The text colour: what the document's black becomes.
    pub color: TypstColor,
}

/// A premultiplied RGBA image (`CGImageAlphaInfo.premultipliedLast`, sRGB, `width * 4` bytes a row).
#[derive(Clone, Debug, uniffi::Record)]
pub struct TypstImage {
    pub width: u32,
    pub height: u32,
    /// Pixels per point used, after fitting; show it at `width / pixelPerPt` points.
    pub scale: f32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum TypstPaint {
    /// The UI's text colour (the document's black).
    Foreground,
    Color {
        color: TypstColor,
    },
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum TypstLineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum TypstLineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum TypstStyle {
    Fill {
        even_odd: bool,
    },
    Stroke {
        width: f32,
        cap: TypstLineCap,
        join: TypstLineJoin,
        miter_limit: f32,
        dash: Vec<f32>,
        dash_phase: f32,
    },
}

/// One painted path in page points (y down). `verbs`: 0 move (1 point), 1 line (1), 2 quad (2), 3 cubic (3),
/// 4 close (0); `points` are x, y pairs in order.
#[derive(Clone, Debug, uniffi::Record)]
pub struct TypstPath {
    pub verbs: Vec<u8>,
    pub points: Vec<f32>,
    pub paint: TypstPaint,
    pub style: TypstStyle,
    /// Index into `TypstPaths.clips` of the clip it is drawn inside.
    pub clip: Option<u32>,
}

/// A clip path (nonzero), inside its parent's.
#[derive(Clone, Debug, uniffi::Record)]
pub struct TypstClip {
    pub verbs: Vec<u8>,
    pub points: Vec<f32>,
    pub parent: Option<u32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TypstPaths {
    pub width_pt: f32,
    pub height_pt: f32,
    /// The page's background; none for the preview (`fill: none`).
    pub background: Option<TypstPaint>,
    pub paths: Vec<TypstPath>,
    pub clips: Vec<TypstClip>,
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct TypstSize {
    pub width_pt: f32,
    pub height_pt: f32,
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct TypstStats {
    pub compiles: u64,
    pub compile_errors: u64,
    pub cache_hits: u64,
    pub last_compile_ms: f64,
    pub warm: bool,
}

fn paint(p: Paint) -> TypstPaint {
    match p {
        Paint::Foreground => TypstPaint::Foreground,
        Paint::Rgba([r, g, b, a]) => TypstPaint::Color {
            color: TypstColor { r, g, b, a },
        },
    }
}

fn verbs(v: &[Verb]) -> Vec<u8> {
    v.iter().map(|&verb| verb as u8).collect()
}

/// Compiles and draws preview documents. Thread-safe; calls are serialised inside.
#[derive(Debug, uniffi::Object)]
pub struct TypstRenderer {
    inner: Arc<Renderer>,
}

#[uniffi::export]
impl TypstRenderer {
    /// Fonts and Typst's library are built once per process (a few ms).
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            // the macros from the core's typstMath port, the same text `typst_source` documents import
            inner: Arc::new(Renderer::with_macros(
                qcalc_core::app::typst_math::TYPST_PREAMBLE,
            )),
        })
    }

    /// Typeset `$ x^2 $` so the first keystroke is fast (`warmupTypst`); call off the main thread at launch.
    /// Returns the milliseconds it took.
    pub fn warm_up(&self) -> f64 {
        self.inner.warm_up()
    }

    /// The page's size in points, or why it doesn't compile.
    pub fn page_size(&self, source: String) -> Result<TypstSize, TypstError> {
        let (width_pt, height_pt) = self.inner.compile(&source)?;
        Ok(TypstSize {
            width_pt,
            height_pt,
        })
    }

    pub fn render(
        &self,
        source: String,
        options: TypstRenderOptions,
    ) -> Result<TypstImage, TypstError> {
        let c = options.color;
        let image = self.inner.render(
            &source,
            &RenderOptions {
                pixel_per_pt: options.pixel_per_pt,
                max_width_pt: options.max_width_pt,
                max_height_pt: options.max_height_pt,
                color: [c.r, c.g, c.b, c.a],
            },
        )?;
        Ok(TypstImage {
            width: image.width,
            height: image.height,
            scale: image.scale,
            width_pt: image.width_pt,
            height_pt: image.height_pt,
            rgba: image.rgba.clone(),
        })
    }

    pub fn paths(&self, source: String) -> Result<TypstPaths, TypstError> {
        let v = self.inner.vector(&source)?;
        Ok(TypstPaths {
            width_pt: v.width_pt,
            height_pt: v.height_pt,
            background: v.background.map(paint),
            paths: v
                .items
                .iter()
                .map(|item| TypstPath {
                    verbs: verbs(&item.path.verbs),
                    points: item.path.points.clone(),
                    paint: paint(item.paint),
                    style: match &item.style {
                        Style::Fill { even_odd } => TypstStyle::Fill {
                            even_odd: *even_odd,
                        },
                        Style::Stroke {
                            width,
                            cap,
                            join,
                            miter_limit,
                            dash,
                            dash_phase,
                        } => TypstStyle::Stroke {
                            width: *width,
                            cap: match cap {
                                Cap::Butt => TypstLineCap::Butt,
                                Cap::Round => TypstLineCap::Round,
                                Cap::Square => TypstLineCap::Square,
                            },
                            join: match join {
                                Join::Miter => TypstLineJoin::Miter,
                                Join::Round => TypstLineJoin::Round,
                                Join::Bevel => TypstLineJoin::Bevel,
                            },
                            miter_limit: *miter_limit,
                            dash: dash.clone(),
                            dash_phase: *dash_phase,
                        },
                    },
                    clip: item.clip.map(|i| i as u32),
                })
                .collect(),
            clips: v
                .clips
                .iter()
                .map(|c| TypstClip {
                    verbs: verbs(&c.path.verbs),
                    points: c.path.points.clone(),
                    parent: c.parent.map(|i| i as u32),
                })
                .collect(),
        })
    }

    /// Drop cached documents, images, paths and Typst's memos (memory pressure).
    pub fn clear_caches(&self) {
        self.inner.clear_caches();
    }

    pub fn stats(&self) -> TypstStats {
        let s = self.inner.stats();
        TypstStats {
            compiles: s.compiles,
            compile_errors: s.compile_errors,
            cache_hits: s.document_hits + s.image_hits + s.vector_hits,
            last_compile_ms: s.last_compile_ms,
            warm: s.warm,
        }
    }
}

/// The fonts' licence (SIL OFL 1.1), for the About window.
#[uniffi::export]
pub fn typst_font_notice() -> String {
    qcalc_typst::FONT_NOTICE.to_string()
}
