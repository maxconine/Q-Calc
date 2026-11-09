//! The graph's picture (the export card's Save picture; the Mac's `snapshot` / `savePicture` in GraphPanel.swift):
//! each equation typeset by Typst as the preview row writes it (`MathTypesetter.equations`), with its colour bar,
//! over the plot, at 3 pixels a point. Saved to the Desktop as `Q Calc graph <date> at <time>.png` and copied to the
//! clipboard as PNG (and as a bitmap, which most Windows apps paste).
//!
//! The typesetting runs on the Typst thread; the picture is drawn on the UI thread into a WIC bitmap with the
//! plot's own base pass (graph_view.rs `draw_picture`), once per click.

use std::path::PathBuf;

use qcalc_core::app::typst_math;
use qcalc_ffi::TypstPaths;
use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_MEDIUM;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat24bppBGR,
    GUID_WICPixelFormat32bppPBGRA, IWICBitmap, IWICImagingFactory, WICBitmapCacheOnLoad,
    WICBitmapEncoderNoCache, WICBitmapLockRead, WICRect,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::Shell::{FOLDERID_Desktop, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{HSTRING, w};
use windows_numerics::{Matrix3x2, Vector2};

use crate::app::App;
use crate::engine::done;
use crate::graph::{GraphState, PAD_LEFT, PAD_RIGHT, SVG_W};
use crate::render::{Color, Gfx, Theme};

/// Pixels a point.
const SCALE: f32 = 3.0;
/// Typst writes at 18 pt; the header's maths reads at about 13 pt, a tall one (a fraction) shrunk to fit its row.
const MATH: f32 = 13.0 / 18.0;
const MAX_ROW: f32 = 40.0;
/// the gap between a curve and its restriction
const BOUNDS_GAP: f32 = 18.0;

/// One equation of the header: its colour's number, its text (when Typst can't write it), and its restriction.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub layer: usize,
    pub text: String,
    pub bounds: String,
}

/// `x^2-3 {3<x<∞}` → (`x^2-3`, `3<x<∞`); no braces → (the whole, "") (splitBounds).
pub fn split_bounds(e: &str) -> (String, String) {
    let t = e.trim();
    let whole = (t.to_string(), String::new());
    if !t.ends_with('}') {
        return whole;
    }
    let Some(open) = t.rfind('{').filter(|&i| i > 0) else {
        return whole;
    };
    let head = t[..open].trim();
    let inner = t[open + 1..t.len() - 1].trim();
    if head.is_empty() {
        whole
    } else {
        (head.to_string(), inner.to_string())
    }
}

/// The picture's file name: `Q Calc graph 2026-11-08 at 14.03.22.png`.
pub fn file_name(y: u16, mo: u16, d: u16, h: u16, mi: u16, s: u16) -> String {
    format!("Q Calc graph {y:04}-{mo:02}-{d:02} at {h:02}.{mi:02}.{s:02}.png")
}

/// A header row's height: a typeset one as tall as its maths (at 13 pt, within 17 … 40), a text one 17; 3 under.
pub fn row_height(math: Option<f32>, bounds: Option<f32>) -> f32 {
    match math {
        None => 17.0 + 3.0,
        Some(m) => (m.max(bounds.unwrap_or(0.0)) * MATH).clamp(17.0, MAX_ROW) + 3.0,
    }
}

/// Save picture: the equations typeset on the Typst thread, then the picture drawn, saved and copied.
pub fn start(app: &mut App) {
    crate::graph::commit(&mut app.model, &app.engine);
    let Some(g) = &app.model.graph else { return };
    let mut rows = vec![Row {
        layer: 0,
        text: g.label(),
        bounds: split_bounds(&g.expression).1,
    }];
    let mut exprs = vec![g.expression.clone()];
    for (i, l) in g.layers.iter().enumerate().filter(|(_, l)| !l.blank()) {
        rows.push(Row {
            layer: i + 1,
            text: l.expression.replace("sqrt(", "√("),
            bounds: split_bounds(&l.expression).1,
        });
        exprs.push(l.expression.clone());
    }
    let expr = g.expression.clone();
    app.model.hint = Some("saving the graph…".into());
    app.model.touch();
    app.engine.typst(move |r| {
        // each equation and its restriction typeset on their own; None for what Typst can't write
        let pictures: Vec<(Option<TypstPaths>, Option<TypstPaths>)> = exprs
            .iter()
            .map(|e| {
                let (head, bounds) = split_bounds(e);
                let typeset = |s: &str| {
                    if s.is_empty() {
                        return None;
                    }
                    let doc = typst_math::native_typst_document(s, "", "")?;
                    r.paths(doc).ok()
                };
                (typeset(&head), typeset(&bounds))
            })
            .collect();
        done(move |app| {
            let ok = app
                .model
                .graph
                .as_ref()
                .is_some_and(|g| g.expression == expr);
            if ok {
                save(app, &rows, &pictures);
            }
        })
    });
}

/// The PNG drawn, saved to the Desktop and copied; the hint says what happened.
fn save(app: &mut App, rows: &[Row], pictures: &[(Option<TypstPaths>, Option<TypstPaths>)]) {
    let Some(g) = &app.model.graph else { return };
    let (theme, hwnd) = (app.theme, app.bar.hwnd);
    let path = desktop().map(|d| d.join(stamp()));
    let saved = path
        .as_ref()
        .and_then(|p| render(&app.gfx, g, &theme, rows, pictures, p));
    // nowhere to save: drawn to a temporary file for the clipboard alone
    let (bytes, wrote) = match saved {
        Some(b) => (Some(b), true),
        None => {
            let tmp = std::env::temp_dir().join("qcalc-graph.png");
            let b = render(&app.gfx, g, &theme, rows, pictures, &tmp);
            let _ = std::fs::remove_file(&tmp);
            (b, false)
        }
    };
    let copied = bytes.is_some_and(|(png, dib)| copy(hwnd, &png, &dib));
    app.model.hint = Some(
        match (wrote, copied) {
            (true, true) => "graph saved to the Desktop and copied",
            (true, false) => "graph saved to the Desktop",
            (false, true) => "graph copied",
            (false, false) => "couldn’t save the graph",
        }
        .into(),
    );
    crate::log::log(&format!(
        "graph export: {} rows, saved {wrote}, copied {copied}",
        rows.len()
    ));
    app.model.touch();
}

fn desktop() -> Option<PathBuf> {
    // SAFETY: the returned string is copied, then freed with CoTaskMemFree
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
    .filter(|p| p.is_dir())
}

fn stamp() -> String {
    // SAFETY: a plain query
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    file_name(t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Draws the picture into a WIC bitmap, writes it to `path` as a PNG, and gives back the file's bytes and the
/// pixels as a bottom-up 32-bit DIB.
fn render(
    gfx: &Gfx,
    g: &GraphState,
    theme: &Theme,
    rows: &[Row],
    pictures: &[(Option<TypstPaths>, Option<TypstPaths>)],
    path: &std::path::Path,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let heights: Vec<f32> = pictures
        .iter()
        .map(|(m, b)| {
            row_height(
                m.as_ref().map(|m| m.height_pt),
                b.as_ref().map(|b| b.height_pt),
            )
        })
        .collect();
    let head = 10.0 + heights.iter().sum::<f32>();
    let (w, h) = (SVG_W, crate::graph_view::svg_height(g));
    let (pw, ph) = (
        (w * SCALE).round() as u32,
        ((h + head) * SCALE).round() as u32,
    );
    // SAFETY: COM is up on the UI thread (OleInitialize); every object here is a local, drawn and released here
    unsafe {
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let bitmap: IWICBitmap = wic
            .CreateBitmap(pw, ph, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)
            .ok()?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rt = gfx.d2d.CreateWicBitmapRenderTarget(&bitmap, &props).ok()?;
        rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        rt.BeginDraw();
        let bg = crate::graph_view::picture_bg(theme);
        rt.Clear(Some(&bg));
        header(&rt, gfx, theme, rows, pictures, &heights);
        rt.SetTransform(&Matrix3x2 {
            M11: SCALE,
            M12: 0.0,
            M21: 0.0,
            M22: SCALE,
            M31: 0.0,
            M32: head * SCALE,
        });
        crate::graph_view::draw_picture(&rt, gfx, g, theme);
        rt.EndDraw(None, None).ok()?;
        drop(rt);
        // the PNG, through WIC's encoder, to the file
        let stream = wic.CreateStream().ok()?;
        let wide = HSTRING::from(path.as_os_str());
        stream.InitializeFromFilename(&wide, GENERIC_WRITE.0).ok()?;
        let encoder = wic
            .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
            .ok()?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame = None;
        let mut bag = None;
        encoder.CreateNewFrame(&mut frame, &mut bag).ok()?;
        let frame = frame?;
        frame.Initialize(bag.as_ref()).ok()?;
        frame.SetSize(pw, ph).ok()?;
        let mut format = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut format).ok()?;
        frame.WriteSource(&bitmap, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        encoder.Commit().ok()?;
        drop(stream);
        let png = std::fs::read(path).ok()?;
        // the pixels, bottom up, for CF_DIB
        let rect = WICRect {
            X: 0,
            Y: 0,
            Width: pw as i32,
            Height: ph as i32,
        };
        let lock = bitmap.Lock(&rect, WICBitmapLockRead.0 as u32).ok()?;
        let stride = lock.GetStride().ok()? as usize;
        let (mut size, mut data) = (0u32, std::ptr::null_mut());
        lock.GetDataPointer(&mut size, &mut data).ok()?;
        let px = std::slice::from_raw_parts(data, size as usize);
        Some((png, dib(pw, ph, stride, px)))
    }
}

/// The header: each equation's colour bar, then its maths (and restriction) typeset, or its text.
fn header(
    rt: &ID2D1RenderTarget,
    gfx: &Gfx,
    theme: &Theme,
    rows: &[Row],
    pictures: &[(Option<TypstPaths>, Option<TypstPaths>)],
    heights: &[f32],
) {
    let ink = theme.ink;
    let Some(font) = crate::render::format(
        &gfx.dw,
        &gfx.fonts.text_family,
        11.0,
        DWRITE_FONT_WEIGHT_MEDIUM,
    )
    .ok() else {
        return;
    };
    // SAFETY: inside the target's BeginDraw/EndDraw; the brush is a local
    let Some(brush) = unsafe { rt.CreateSolidColorBrush(&ink, None) }.ok() else {
        return;
    };
    let at = |x: f32, y: f32, k: f32| Matrix3x2 {
        M11: SCALE * k,
        M12: 0.0,
        M21: 0.0,
        M22: SCALE * k,
        M31: SCALE * x,
        M32: SCALE * y,
    };
    let text = |s: &str, x: f32, y: f32, h: f32| {
        if let Some(l) = gfx.layout(s, &font, 0.0, 0.0) {
            let lh = crate::render::measure(&l).1;
            // SAFETY: as above
            unsafe {
                rt.SetTransform(&at(0.0, 0.0, 1.0));
                brush.SetColor(&ink);
                rt.DrawTextLayout(
                    Vector2 {
                        X: x,
                        Y: y + (h - lh) / 2.0,
                    },
                    &l,
                    &brush,
                    windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
        }
    };
    let mut y = 8.0;
    for ((row, (math, bounds)), &rh) in rows.iter().zip(pictures).zip(heights) {
        let bar: Color = crate::graph_view::equation_color(theme, row.layer);
        // SAFETY: as above
        unsafe {
            rt.SetTransform(&at(0.0, 0.0, 1.0));
            brush.SetColor(&bar);
            rt.FillRectangle(
                &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                    left: PAD_LEFT,
                    top: y + (rh - 11.0) / 2.0,
                    right: PAD_LEFT + 3.0,
                    bottom: y + (rh - 11.0) / 2.0 + 11.0,
                },
                &brush,
            );
        }
        let mut x = PAD_LEFT + 10.0;
        let room = SVG_W - PAD_RIGHT - x;
        match math {
            Some(m) => {
                let bw = bounds.as_ref().map_or(0.0, |b| b.width_pt + BOUNDS_GAP);
                // a long or tall equation shrunk to the room it has
                let k = MATH
                    .min(room / (m.width_pt + bw).max(1.0))
                    .min((rh - 3.0) / m.height_pt.max(1.0));
                let mut pics = vec![(m, 0.0)];
                if let Some(b) = bounds {
                    pics.push((b, BOUNDS_GAP));
                }
                for (pic, gap) in pics {
                    x += gap * k;
                    let built = crate::typst_row::build(gfx, pic);
                    // SAFETY: as above
                    unsafe { rt.SetTransform(&at(x, y + (rh - pic.height_pt * k) / 2.0, k)) };
                    crate::typst_row::draw_items(rt, &brush, &built, ink);
                    x += pic.width_pt * k;
                }
                if bounds.is_none() && !row.bounds.is_empty() {
                    text(&format!("  {{{}}}", row.bounds), x, y, rh);
                }
            }
            None => text(&row.text, x, y, rh),
        }
        y += rh;
    }
}

/// Top-down premultiplied BGRA rows as a bottom-up 32-bit DIB (BITMAPINFOHEADER, then the pixels).
fn dib(w: u32, h: u32, stride: usize, px: &[u8]) -> Vec<u8> {
    let row = w as usize * 4;
    let mut out = Vec::with_capacity(40 + row * h as usize);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]); // BI_RGB
    out.extend_from_slice(&((row * h as usize) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..h as usize).rev() {
        let start = y * stride;
        out.extend_from_slice(px.get(start..start + row).unwrap_or(&[]));
    }
    out
}

/// The picture on the clipboard: as PNG (what the Mac copies) and as a DIB.
fn copy(owner: windows::Win32::Foundation::HWND, png: &[u8], dib: &[u8]) -> bool {
    let mut open = false;
    for _ in 0..10 {
        // SAFETY: closed below
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            open = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if !open {
        return false;
    }
    // SAFETY: the clipboard is open; each global is filled while locked and handed to the clipboard, which owns it
    let ok = unsafe {
        let _ = EmptyClipboard();
        let put = |format: u32, bytes: &[u8]| -> bool {
            let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else {
                return false;
            };
            let p = GlobalLock(g) as *mut u8;
            if p.is_null() {
                return false;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            let _ = GlobalUnlock(g);
            SetClipboardData(format, Some(HANDLE(g.0))).is_ok()
        };
        let png_ok = put(RegisterClipboardFormatW(w!("PNG")), png);
        // CF_DIB
        let dib_ok = put(8, dib);
        png_ok || dib_ok
    };
    // SAFETY: opened above
    let _ = unsafe { CloseClipboard() };
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restriction_is_split_off() {
        assert_eq!(
            split_bounds("x^2-3 {3<x<∞}"),
            ("x^2-3".into(), "3<x<∞".into())
        );
        assert_eq!(split_bounds(" sin(x) "), ("sin(x)".into(), String::new()));
        assert_eq!(split_bounds("{x}"), ("{x}".into(), String::new()));
        assert_eq!(split_bounds("f(x) = {1}"), ("f(x) =".into(), "1".into()));
    }

    #[test]
    fn the_file_and_the_header_rows() {
        assert_eq!(
            file_name(2026, 11, 8, 9, 3, 7),
            "Q Calc graph 2026-11-08 at 09.03.07.png"
        );
        assert_eq!(row_height(None, None), 20.0);
        // a one-line equation, 22 pt tall at Typst's 18, is about 16 at 13: 17 high
        assert_eq!(row_height(Some(22.0), None), 20.0);
        // a fraction is taller; a huge one stops at 40
        assert!((row_height(Some(36.0), None) - (26.0 + 3.0)).abs() < 1e-4);
        assert_eq!(row_height(Some(200.0), Some(10.0)), 43.0);
        // the restriction can be the taller
        assert!((row_height(Some(10.0), Some(36.0)) - 29.0).abs() < 1e-4);
    }
}
