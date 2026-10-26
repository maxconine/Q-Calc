use std::sync::Arc;

use qcalc_typst::vector::{Cap, Join, Paint, Style, Vector, Verb};
use qcalc_typst::{Error, MACROS, RenderOptions, Renderer, WARMUP_SOURCE};
use tiny_skia as sk;

fn doc(math: &str) -> String {
    // `typstDocument`'s shape
    format!(
        "#import \"/macros.typ\": *\n#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(size: 18pt, fill: black)\n$ {math} $\n"
    )
}

const LIGHT: [u8; 4] = [0x1d, 0x1d, 0x1f, 255];
const DARK: [u8; 4] = [0xf5, 0xf5, 0xf7, 255];

#[test]
fn macros_are_the_web_preamble() {
    let ts = include_str!("../../../src/lib/typstMath.ts");
    let start = "export const TYPST_PREAMBLE = `";
    let from = ts.find(start).unwrap() + start.len();
    let to = from + ts[from..].find('`').unwrap();
    assert_eq!(
        MACROS,
        &ts[from..to],
        "macros.typ drifted from TYPST_PREAMBLE"
    );
}

#[test]
fn renders_in_the_text_colour_for_light_and_dark() {
    let r = Renderer::new();
    let src = doc("frac(1, 2) + sqrt(x^2) = mat(1, 2; 3, 4)");
    let opts = |color| RenderOptions {
        pixel_per_pt: 2.0,
        max_width_pt: 0.0,
        max_height_pt: 0.0,
        color,
    };
    let light = r.render(&src, &opts(LIGHT)).unwrap();
    let dark = r.render(&src, &opts(DARK)).unwrap();
    assert_eq!(light.width, (light.width_pt * 2.0).round() as u32);
    assert_eq!(light.height, (light.height_pt * 2.0).round() as u32);
    assert_eq!((light.width, light.height), (dark.width, dark.height));
    let mut inked = 0;
    let unpremultiply = |p: &[u8], c: usize| (p[c] as u32 * 255 / p[3] as u32) as u8;
    for (l, d) in light.rgba.chunks(4).zip(dark.rgba.chunks(4)) {
        // same coverage; colour is the requested one, premultiplied (pixglyph's coverage tops out at 254)
        assert!(l[3].abs_diff(d[3]) <= 1);
        if l[3] >= 250 && d[3] >= 250 {
            inked += 1;
            for c in 0..3 {
                assert!(unpremultiply(l, c).abs_diff(LIGHT[c]) <= 2, "{l:?}");
                assert!(unpremultiply(d, c).abs_diff(DARK[c]) <= 2, "{d:?}");
            }
        }
        for c in 0..3 {
            assert!(l[c] <= l[3] && d[c] <= d[3], "premultiplied");
        }
    }
    assert!(inked > 100);
    // `fill: none`: the corners are clear
    assert_eq!(&light.rgba[..4], &[0, 0, 0, 0]);
}

#[test]
fn other_colours_stay_as_written() {
    let r = Renderer::new();
    let src = doc("x + #text(fill: rgb(\"#ff0000\"))[y]");
    let img = r
        .render(
            &src,
            &RenderOptions {
                color: DARK,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    let red = img
        .rgba
        .chunks(4)
        .filter(|p| p[3] >= 250 && p[0] >= 250 && p[1] == 0)
        .count();
    let fg = img
        .rgba
        .chunks(4)
        .filter(|p| p[3] >= 250 && p[1] >= 240)
        .count();
    assert!(red > 20 && fg > 20, "red {red}, foreground {fg}");
    let v = r.vector(&src).unwrap();
    assert!(
        v.items
            .iter()
            .any(|i| i.paint == Paint::Rgba([255, 0, 0, 255]))
    );
    assert!(v.items.iter().any(|i| i.paint == Paint::Foreground));
}

#[test]
fn shrinks_to_fit_like_the_web_row() {
    let r = Renderer::new();
    let tall = doc("mat(1; 2; 3; 4; 5; 6)");
    let img = r.render(&tall, &RenderOptions::default()).unwrap();
    assert!(img.height_pt > 72.0);
    // 72 pt high at 2 px/pt
    assert!(img.height.abs_diff(144) <= 1, "{}", img.height);
    let (_, h) = img.display_size(2.0);
    assert!((h - 72.0).abs() <= 0.5);
    let wide = doc(&["x"; 60].join(" + "));
    let img = r
        .render(
            &wide,
            &RenderOptions {
                max_width_pt: 300.0,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    assert!(img.width_pt > 300.0);
    assert!(img.width.abs_diff(600) <= 1, "{}", img.width);
}

#[test]
fn mid_edit_input_is_an_error_and_cached() {
    let r = Renderer::new();
    let err = r.render(&doc("frac(1,"), &RenderOptions::default());
    assert!(matches!(err, Err(Error::Compile { .. })), "{err:?}");
    let err = r.compile(&doc("undefinedthing")).unwrap_err();
    let Error::Compile { message } = &err else {
        panic!("{err:?}")
    };
    // the message, then typst's hints
    assert!(message.starts_with("unknown variable: undefinedthing\nhint: "));
    let before = r.stats();
    let _ = r.compile(&doc("undefinedthing"));
    let after = r.stats();
    assert_eq!(after.compiles, before.compiles);
    assert_eq!(after.document_hits, before.document_hits + 1);
    // a good source after errors still renders
    assert!(r.render(&doc("x^2"), &RenderOptions::default()).is_ok());
}

#[test]
fn no_files_packages_or_clock() {
    let r = Renderer::new();
    for src in [
        "#image(\"/etc/hosts\")",
        "#read(\"/etc/passwd\")",
        "#import \"@preview/cetz:0.3.0\": *",
        "#datetime.today().display()",
    ] {
        assert!(r.compile(src).is_err(), "{src}");
    }
}

#[test]
fn caches_and_is_deterministic() {
    let r = Renderer::new();
    let src = doc("sum_(k=1)^n k^2");
    let a = r.render(&src, &RenderOptions::default()).unwrap();
    let b = r.render(&src, &RenderOptions::default()).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    let fresh = Renderer::new()
        .render(&src, &RenderOptions::default())
        .unwrap();
    assert_eq!(a.rgba, fresh.rgba);
    r.clear_caches();
    let c = r.render(&src, &RenderOptions::default()).unwrap();
    assert_eq!(a.rgba, c.rgba);
}

#[test]
fn warms_up_in_the_background() {
    let r = Arc::new(Renderer::new());
    let ms = r.warm_up_in_background().join().unwrap();
    assert!(ms >= 0.0);
    assert!(r.stats().warm);
    // the warm-up page has no `fill: none`, so it is white, as on the web
    let img = r.render(WARMUP_SOURCE, &RenderOptions::default()).unwrap();
    assert_eq!(&img.rgba[..4], &[255, 255, 255, 255]);
}

/// Fills the vector output with tiny-skia, black on clear.
fn rasterise(v: &Vector, scale: f32) -> sk::Pixmap {
    let w = (v.width_pt * scale).round().max(1.0) as u32;
    let h = (v.height_pt * scale).round().max(1.0) as u32;
    let mut pm = sk::Pixmap::new(w, h).unwrap();
    let ts = sk::Transform::from_scale(scale, scale);
    let mut paint = sk::Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    for item in &v.items {
        assert!(item.clip.is_none());
        let mut pb = sk::PathBuilder::new();
        let mut p = item.path.points.chunks(2);
        for verb in &item.path.verbs {
            let mut next = || {
                let c = p.next().unwrap();
                (c[0], c[1])
            };
            match verb {
                Verb::Move => {
                    let (x, y) = next();
                    pb.move_to(x, y)
                }
                Verb::Line => {
                    let (x, y) = next();
                    pb.line_to(x, y)
                }
                Verb::Quad => {
                    let (a, b) = next();
                    let (x, y) = next();
                    pb.quad_to(a, b, x, y)
                }
                Verb::Cubic => {
                    let (a, b) = next();
                    let (c, d) = next();
                    let (x, y) = next();
                    pb.cubic_to(a, b, c, d, x, y)
                }
                Verb::Close => pb.close(),
            }
        }
        let Some(path) = pb.finish() else { continue };
        match &item.style {
            Style::Fill { even_odd } => {
                let rule = if *even_odd {
                    sk::FillRule::EvenOdd
                } else {
                    sk::FillRule::Winding
                };
                pm.fill_path(&path, &paint, rule, ts, None);
            }
            Style::Stroke {
                width, cap, join, ..
            } => {
                let stroke = sk::Stroke {
                    width: *width,
                    line_cap: match cap {
                        Cap::Butt => sk::LineCap::Butt,
                        Cap::Round => sk::LineCap::Round,
                        Cap::Square => sk::LineCap::Square,
                    },
                    line_join: match join {
                        Join::Miter => sk::LineJoin::Miter,
                        Join::Round => sk::LineJoin::Round,
                        Join::Bevel => sk::LineJoin::Bevel,
                    },
                    ..sk::Stroke::default()
                };
                pm.stroke_path(&path, &paint, &stroke, ts, None);
            }
        }
    }
    pm
}

/// The paths are the pixmap. At 8 px/pt (144 ppem) typst-render fills glyph outlines with tiny-skia instead of
/// pixglyph, so filling the paths with tiny-skia must give the same pixels. (At 2-3 px/pt pixglyph's
/// anti-aliasing differs from tiny-skia's on about 5% of the ink, with the same total ink.)
#[test]
fn vector_paths_match_the_pixmap() {
    let r = Renderer::new();
    let corpus = include_str!("corpus.jsonl");
    let mut checked = 0;
    let mut worst = 0.0f64;
    for (i, line) in corpus.lines().enumerate() {
        if i % 4 != 0 {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let src = v["doc"].as_str().unwrap();
        let Ok(vector) = r.vector(src) else { continue };
        let scale = 8.0;
        let img = r
            .render(
                src,
                &RenderOptions {
                    pixel_per_pt: scale,
                    max_width_pt: 0.0,
                    max_height_pt: 0.0,
                    color: [0, 0, 0, 255],
                },
            )
            .unwrap();
        let pm = rasterise(&vector, scale);
        assert_eq!((pm.width(), pm.height()), (img.width, img.height));
        let (mut diff, mut ink, mut vink, mut flips, mut solid) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for (a, b) in pm.data().chunks(4).zip(img.rgba.chunks(4)) {
            diff += a[3].abs_diff(b[3]) as u64;
            ink += b[3] as u64;
            vink += a[3] as u64;
            if (a[3] > 127) != (b[3] > 127) {
                flips += 1;
            }
            if b[3] > 127 {
                solid += 1;
            }
        }
        assert!(ink > 0, "{src}");
        // the share of the ink that differs
        let off = diff as f64 / ink as f64;
        worst = worst.max(off);
        // what is left is typst-render drawing rectangles (fraction bars) without anti-aliasing
        assert!(
            off < 0.01,
            "{}: {:.2}% of the ink differs",
            v["expr"],
            off * 100.0
        );
        assert!((vink as f64 / ink as f64 - 1.0).abs() < 0.005);
        assert!(flips * 100 < solid, "{}", v["expr"]);
        checked += 1;
    }
    println!(
        "{checked} pages: at most {:.2}% of the ink differs",
        worst * 100.0
    );
    assert!(checked > 250);
}
