//! The formula sheets' typeset cells (SheetData.identitySheet, MathPreview.draw): a sheet's documents are
//! compiled on the Typst thread (`EngineThread::typst`) the first time it opens, before its page shows, and kept
//! as vector paths; they are turned into Direct2D geometries (typst_row.rs `build`) the first time a sheet window
//! draws them and filled in the theme's ink, so light and dark need no recompile. Both go when the last sheet
//! window closes (`forget`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use qcalc_ffi::TypstPaths;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS,
};
use windows_numerics::Matrix3x2;

use crate::app::App;
use crate::engine::done;
use crate::qlog;
use crate::render::{Canvas, Color};
use crate::typst_row::{self, Built};

#[derive(Default)]
struct Cache {
    paths: HashMap<String, Arc<TypstPaths>>,
    /// documents that wouldn't compile, so they aren't asked for again
    failed: std::collections::HashSet<String>,
    /// geometries, made with the sheets' own factory
    built: HashMap<String, Built>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The documents of `docs` not compiled yet.
pub fn missing(docs: Vec<String>) -> Vec<String> {
    CACHE.with(|c| {
        let c = c.borrow();
        docs.into_iter()
            .filter(|d| !c.paths.contains_key(d) && !c.failed.contains(d))
            .collect()
    })
}

/// Compiles `docs` on the Typst thread, then runs `then` on the UI thread (outside the app's borrow).
pub fn compile(app: &mut App, docs: Vec<String>, then: impl FnOnce() + Send + 'static) {
    if docs.is_empty() {
        crate::later::free("sheet-open", 0, then);
        return;
    }
    app.engine.typst(move |r| {
        let t = std::time::Instant::now();
        let n = docs.len();
        let out: Vec<(String, Option<TypstPaths>)> = docs
            .into_iter()
            .map(|d| {
                let p = r.paths(d.clone()).ok();
                (d, p)
            })
            .collect();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        done(move |_| {
            qlog!("sheet: typeset {n} cells in {ms:.0} ms");
            CACHE.with(|c| {
                let mut c = c.borrow_mut();
                for (doc, paths) in out {
                    match paths {
                        Some(p) => {
                            c.paths.insert(doc, Arc::new(p));
                        }
                        None => {
                            c.failed.insert(doc);
                        }
                    }
                }
            });
            crate::later::free("sheet-open", 0, then);
        })
    });
}

/// A document's size in points, once compiled.
pub fn size(doc: &str) -> Option<(f32, f32)> {
    CACHE.with(|c| c.borrow().paths.get(doc).map(|p| (p.width_pt, p.height_pt)))
}

/// Draws a compiled document with its top left at (x, y), in `ink` at `alpha`.
pub fn draw(c: &Canvas, doc: &str, x: f32, y: f32, ink: Color, alpha: f32) {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let cache = &mut *cache;
        if !cache.built.contains_key(doc) {
            let Some(p) = cache.paths.get(doc) else {
                return;
            };
            let built = typst_row::build(c.gfx, p);
            cache.built.insert(doc.to_string(), built);
        }
        let Some(built) = cache.built.get(doc) else {
            return;
        };
        let rt = c.rt();
        // SAFETY: inside the sheet's BeginDraw/EndDraw; the transform and the layer are put back here
        unsafe {
            let Ok(brush) = rt.CreateSolidColorBrush(&ink, None) else {
                return;
            };
            let mut before = Matrix3x2::default();
            rt.GetTransform(&mut before);
            let moved = Matrix3x2::translation(x, y) * before;
            rt.SetTransform(&moved);
            let faded = alpha < 0.999;
            if faded {
                let params = D2D1_LAYER_PARAMETERS {
                    contentBounds: D2D_RECT_F {
                        left: f32::MIN,
                        top: f32::MIN,
                        right: f32::MAX,
                        bottom: f32::MAX,
                    },
                    geometricMask: std::mem::ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: alpha,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                };
                rt.PushLayer(&params, None);
            }
            typst_row::draw_items(rt, &brush, built, ink);
            if faded {
                rt.PopLayer();
            }
            rt.SetTransform(&before);
        }
    });
}

/// The last sheet window closed: its geometries go (they belong to its factory), and the compiled paths too.
pub fn forget() {
    CACHE.with(|c| *c.borrow_mut() = Cache::default());
}

/// How many documents are compiled (the dev pipe's `sheetpages`).
pub fn count() -> usize {
    CACHE.with(|c| c.borrow().paths.len())
}
