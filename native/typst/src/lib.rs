//! `qcalc-typst`: the typeset maths preview, natively (`native/ARCHITECTURE.md` §6).
//!
//! The web app builds a Typst document with `typstDocument` (`src/lib/typstMath.ts`, ported by WP-NOTATION) and
//! compiles it with typst.ts, the Typst compiler built to WASM, into an SVG (`src/lib/typstRender.ts`). This
//! crate compiles the same source with the same Typst (0.14.2), the same macros (`/macros.typ`) and the same
//! three fonts, and hands the page back two ways:
//!
//! - [`Renderer::render`]: a premultiplied RGBA pixmap at the screen's backing scale, for a `CGImage`;
//! - [`Renderer::vector`]: the page as paths in points, for `CGPath`, resolution independent, with the text
//!   colour left to the UI ([`vector::Paint::Foreground`]).
//!
//! What `typstRender.ts` does, and where it is here:
//!
//! | `typstRender.ts` | here |
//! |---|---|
//! | `engine()` loads the WASM and fonts once | [`Renderer::new`] (fonts and library once per process) |
//! | `warmupTypst` typesets `$ x^2 $` after 400 ms | [`Renderer::warm_up`] / [`Renderer::warm_up_in_background`] |
//! | `svgCache`, 48 sources | document cache of 48 sources, plus pixmap and path caches |
//! | `compiler.reset()` every 200 compiles | `typst::comemo::evict` after every compile, keeping 30 compiles of memos |
//! | `cleanSvg` black → `currentColor` | opaque black → the requested colour ([`recolor`]); `Foreground` in paths |
//! | one compile at a time, newest wins | one compile at a time (a mutex); the UI drops stale generations |
//! | `.typst-preview svg { max-height: 72px; max-width: 100% }` | [`RenderOptions::max_width_pt`]/`max_height_pt` |
//!
//! A compile error (mid-edit input) is an `Err`; the web preview then keeps the last good render, dimmed, and the
//! UI does the same with the last image it got.

mod recolor;
pub mod vector;
mod world;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use typst::layout::{Page, PagedDocument};

pub use vector::Vector;
pub use world::{FONT_NOTICE, MACROS, font_count};

use world::PreviewWorld;

/// The document `warmupTypst` typesets (`src/lib/typstRender.ts`).
pub const WARMUP_SOURCE: &str = "#set page(width: auto, height: auto)\n$ x^2 $";

/// `CACHE_SIZE` in `typstRender.ts`.
const DOCUMENT_CACHE: usize = 48;
/// Pixmaps are the big entries (a wide formula at 2× is a few hundred KB), and the UI asks for at most two
/// colours of the newest few sources, so fewer are kept.
const IMAGE_CACHE: usize = 16;
const VECTOR_CACHE: usize = 48;
/// comemo memos unused for this many compiles are dropped (typst-cli's watch mode keeps 10). The web preview
/// instead resets the compiler every 200 compiles.
const EVICT_AGE: usize = 30;

/// Why a source gave no picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Typst rejected the source: the first error's message, then its hints.
    Compile { message: String },
    /// Typst panicked. The renderer is reset, so the next call starts clean.
    Crashed { message: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Compile { message } => write!(f, "typst: {message}"),
            Error::Crashed { message } => write!(f, "typst crashed: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// How to rasterise a page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    /// Pixels per point at full size: the screen's backing scale (2 on Retina) when one point is one UI point,
    /// as in the web view, where the SVG's `pt` sizes are CSS pixels.
    pub pixel_per_pt: f32,
    /// Shrink (never grow) so the page fits this many points across, as `max-width: 100%` does. 0 = no limit.
    pub max_width_pt: f32,
    /// Shrink so the page fits this many points high (the web preview's 72 px). 0 = no limit.
    pub max_height_pt: f32,
    /// The colour opaque black becomes: the UI's text colour, straight sRGB RGBA.
    pub color: [u8; 4],
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            pixel_per_pt: 2.0,
            max_width_pt: 0.0,
            max_height_pt: 72.0,
            color: [0, 0, 0, 255],
        }
    }
}

impl RenderOptions {
    /// The scale `fit` gives a page of this size: `pixel_per_pt` times the shrink that fits the limits.
    pub fn effective_scale(&self, width_pt: f32, height_pt: f32) -> f32 {
        let mut fit = 1.0f32;
        if self.max_width_pt > 0.0 && width_pt > self.max_width_pt {
            fit = fit.min(self.max_width_pt / width_pt);
        }
        if self.max_height_pt > 0.0 && height_pt > self.max_height_pt {
            fit = fit.min(self.max_height_pt / height_pt);
        }
        self.pixel_per_pt.max(0.01) * fit
    }

    fn key(&self) -> [u32; 4] {
        [
            self.pixel_per_pt.to_bits(),
            self.max_width_pt.to_bits(),
            self.max_height_pt.to_bits(),
            u32::from_be_bytes(self.color),
        ]
    }
}

/// A rendered page: premultiplied RGBA, rows top to bottom, `width * 4` bytes per row
/// (`CGImageAlphaInfo.premultipliedLast`, sRGB).
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// The pixels per point actually used (after fitting).
    pub scale: f32,
    /// The page's size in points before fitting.
    pub width_pt: f32,
    pub height_pt: f32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// The size to show it at, in UI points: pixels over the backing scale.
    pub fn display_size(&self, pixel_per_pt: f32) -> (f32, f32) {
        (
            self.width as f32 / pixel_per_pt,
            self.height as f32 / pixel_per_pt,
        )
    }
}

/// Counters for the UI's diagnostics and the benchmarks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub compiles: u64,
    pub compile_errors: u64,
    pub document_hits: u64,
    pub image_hits: u64,
    pub vector_hits: u64,
    pub last_compile_ms: f64,
    pub warm: bool,
}

/// A small most-recently-used list; the caches hold tens of entries, so a linear scan is cheapest.
#[derive(Debug)]
struct Lru<K, V> {
    cap: usize,
    entries: Vec<(K, V)>,
}

impl<K: PartialEq, V: Clone> Lru<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            entries: Vec::new(),
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(i);
        let value = entry.1.clone();
        self.entries.push(entry);
        Some(value)
    }

    fn put(&mut self, key: K, value: V) {
        if let Some(i) = self.entries.iter().position(|(k, _)| *k == key) {
            self.entries.remove(i);
        }
        self.entries.push((key, value));
        if self.entries.len() > self.cap {
            self.entries.remove(0);
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

type Compiled = Result<Arc<Page>, Error>;

struct Inner {
    world: PreviewWorld,
    macros: String,
    // failures are cached too: the same mid-edit source shouldn't be compiled twice
    documents: Lru<String, Compiled>,
    images: Lru<(String, [u32; 4]), Arc<Image>>,
    vectors: Lru<String, Arc<Vector>>,
    stats: Stats,
}

impl Inner {
    fn new(macros: String) -> Self {
        Self {
            world: PreviewWorld::new(&macros),
            macros,
            documents: Lru::new(DOCUMENT_CACHE),
            images: Lru::new(IMAGE_CACHE),
            vectors: Lru::new(VECTOR_CACHE),
            stats: Stats::default(),
        }
    }

    fn page(&mut self, source: &str) -> Compiled {
        if let Some(hit) = self.documents.get(&source.to_string()) {
            self.stats.document_hits += 1;
            return hit;
        }
        let started = Instant::now();
        self.world.set_main(source);
        let result = typst::compile::<PagedDocument>(&self.world).output;
        typst::comemo::evict(EVICT_AGE);
        self.stats.compiles += 1;
        self.stats.last_compile_ms = started.elapsed().as_secs_f64() * 1e3;
        let compiled = match result {
            // `width: auto, height: auto` gives one page; a source with a page break shows its first, as the
            // web preview's SVG shows them stacked but the row is sized for one
            Ok(mut document) if !document.pages.is_empty() => {
                Ok(Arc::new(document.pages.swap_remove(0)))
            }
            Ok(_) => Err(Error::Compile {
                message: "the document has no pages".into(),
            }),
            Err(diagnostics) => {
                self.stats.compile_errors += 1;
                let message = diagnostics
                    .iter()
                    .find(|d| d.severity == typst::diag::Severity::Error)
                    .or(diagnostics.first())
                    .map(|d| {
                        let mut message = d.message.to_string();
                        for hint in &d.hints {
                            message.push_str("\nhint: ");
                            message.push_str(hint);
                        }
                        message
                    })
                    .unwrap_or_else(|| "unknown error".into());
                Err(Error::Compile { message })
            }
        };
        self.documents.put(source.to_string(), compiled.clone());
        compiled
    }
}

/// The preview renderer: compiles Typst sources and draws them. Cheap to share (`Arc`), safe to call from any
/// thread; calls are serialised, like the web preview's single compile queue.
pub struct Renderer {
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("stats", &self.stats())
            .finish()
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}

impl Renderer {
    /// A renderer with the preview's macros (`TYPST_PREAMBLE`) at `/macros.typ`. The first one in a process
    /// parses the fonts and builds Typst's standard library (a few ms); the first compile costs more (see
    /// [`Renderer::warm_up`]).
    pub fn new() -> Self {
        Self::with_macros(MACROS)
    }

    /// A renderer with other macros at `/macros.typ` (for when the core's `typstMath` port owns them).
    pub fn with_macros(macros: &str) -> Self {
        Self {
            inner: Mutex::new(Inner::new(macros.to_string())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // a panic is caught inside, so poisoning only follows one; the state is rebuilt below anyway
        self.inner.lock().unwrap_or_else(|poisoned| {
            let mut guard = poisoned.into_inner();
            let macros = guard.macros.clone();
            *guard = Inner::new(macros);
            self.inner.clear_poison();
            guard
        })
    }

    /// Runs `f` with the state, turning a Typst panic into [`Error::Crashed`] and resetting the state.
    fn with<T>(&self, f: impl FnOnce(&mut Inner) -> Result<T, Error>) -> Result<T, Error> {
        let mut guard = self.lock();
        match catch_unwind(AssertUnwindSafe(|| f(&mut guard))) {
            Ok(result) => result,
            Err(payload) => {
                let macros = guard.macros.clone();
                *guard = Inner::new(macros);
                Err(Error::Crashed {
                    message: panic_message(payload),
                })
            }
        }
    }

    /// Typeset `$ x^2 $` once, so the first keystroke doesn't pay for building the library's scopes, shaping
    /// and the math font's tables (`warmupTypst`). Returns how long it took, in ms.
    pub fn warm_up(&self) -> f64 {
        let started = Instant::now();
        let _ = self.render(WARMUP_SOURCE, &RenderOptions::default());
        let mut inner = self.lock();
        inner.stats.warm = true;
        started.elapsed().as_secs_f64() * 1e3
    }

    /// [`Renderer::warm_up`] on a background thread (the UI calls this at launch).
    pub fn warm_up_in_background(self: &Arc<Self>) -> std::thread::JoinHandle<f64> {
        let renderer = Arc::clone(self);
        std::thread::Builder::new()
            .name("qcalc-typst-warmup".into())
            .spawn(move || renderer.warm_up())
            .expect("spawn the typst warm-up thread")
    }

    /// Compile only: the page's size in points, or why it failed. Cached like every other call.
    pub fn compile(&self, source: &str) -> Result<(f32, f32), Error> {
        self.with(|inner| {
            let page = inner.page(source)?;
            let size = page.frame.size();
            Ok((size.x.to_pt() as f32, size.y.to_pt() as f32))
        })
    }

    /// The page as a pixmap, through `typst-render` (tiny-skia), after the black → `color` swap.
    pub fn render(&self, source: &str, options: &RenderOptions) -> Result<Arc<Image>, Error> {
        self.with(|inner| {
            let key = (source.to_string(), options.key());
            if let Some(hit) = inner.images.get(&key) {
                inner.stats.image_hits += 1;
                return Ok(hit);
            }
            let page = inner.page(source)?;
            let size = page.frame.size();
            let (width_pt, height_pt) = (size.x.to_pt() as f32, size.y.to_pt() as f32);
            let scale = options.effective_scale(width_pt, height_pt);
            let page = recolor::recolor_page(&page, options.color);
            let pixmap = typst_render::render(&page, scale);
            let image = Arc::new(Image {
                width: pixmap.width(),
                height: pixmap.height(),
                scale,
                width_pt,
                height_pt,
                rgba: pixmap.take(),
            });
            inner.images.put(key, Arc::clone(&image));
            Ok(image)
        })
    }

    /// The page as paths in points (see [`vector`]).
    pub fn vector(&self, source: &str) -> Result<Arc<Vector>, Error> {
        self.with(|inner| {
            let key = source.to_string();
            if let Some(hit) = inner.vectors.get(&key) {
                inner.stats.vector_hits += 1;
                return Ok(hit);
            }
            let page = inner.page(source)?;
            let vector = Arc::new(vector::page_vector(&page));
            inner.vectors.put(key, Arc::clone(&vector));
            Ok(vector)
        })
    }

    /// Drop every cached document, pixmap and path, and Typst's memos (memory pressure, or settings that
    /// change every render).
    pub fn clear_caches(&self) {
        let mut inner = self.lock();
        inner.documents.clear();
        inner.images.clear();
        inner.vectors.clear();
        typst::comemo::evict(0);
    }

    pub fn stats(&self) -> Stats {
        self.lock().stats
    }
}
