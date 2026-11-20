//! QA backlog item 97: a temperature moved by a difference, polar spirals, and the squiggle under a LaTeX line.
use qcalc_core::app::live::{LiveRequest, live};
use qcalc_core::graph::{GraphOptions, build_native_graph};
use qcalc_core::graph_native::native_home;

fn shown(t: &str) -> (String, Option<String>) {
    let req = LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    };
    let r = live(&req);
    let squiggle = r
        .squiggle
        .map(|s| t.chars().skip(s.start).take(s.end - s.start).collect());
    (r.display, squiggle)
}

// 2026-11-20: `20 °C + 5 K` was blank (°C is an offset scale, so the web adds nothing to it)
#[test]
fn a_temperature_plus_a_difference() {
    for (t, same_as) in [
        ("20 °C + 5 K", "25 °C"),
        ("20 °C - 5 K", "15 °C"),
        ("20 degC + 5 K", "25 °C"),
        ("68 °F + 10 K", "86 °F"),
        ("68 °F - 9 °R", "59 °F"),
        ("20.1 °C + 0.2 K", "20.3 °C"),
        ("-5 °C + 10 kelvin", "5 °C"),
        ("20 celsius + 5 K", "25 °C"),
        ("20 °C + 5 K in °C", "25 °C in °C"),
    ] {
        let (got, _) = shown(t);
        assert!(!got.is_empty(), "{t}");
        assert_eq!(got, shown(same_as).0, "{t}");
    }
    assert_eq!(shown("20 °C + 5 K in °C").0, "25 °C");
    assert_eq!(shown("20 °C + 5 K to K").0, "298.15 K");
    // two temperatures don't add
    assert_eq!(shown("20 °C + 5 °C").0, "");
}

// 2026-11-20: `\frac{1}{2} + \sqrt{x}` squiggled `frac`, not the unknown x
#[test]
fn latex_commands_are_not_unknown_words() {
    assert_eq!(shown(r"\frac{1}{2} + \sqrt{x}").1.as_deref(), Some("x"));
    assert_eq!(shown(r"\left(x\right)").1.as_deref(), Some("x"));
    // a command the LaTeX reading doesn't know is still marked
    assert_eq!(shown(r"\frac{1}{2} + \foo{2}").1.as_deref(), Some("foo"));
}

fn spiral(t: &str) -> (Vec<f64>, Vec<(f64, f64)>) {
    let mut o = GraphOptions {
        aspect: Some(1.0),
        ..Default::default()
    };
    let (lo, hi, _) = native_home(t, &o);
    o.domain = Some((lo, hi));
    let r = build_native_graph(t, &o).unwrap();
    assert_eq!(r.error, None, "{t}");
    let segs = r.contours.unwrap().remove(0).segments;
    (segs, r.critical_points.iter().map(|c| (c.x, c.y)).collect())
}

// 2026-11-20: `r = θ` said "Only a polar curve that repeats each turn of θ draws (not a spiral)"
#[test]
fn polar_spirals_draw_their_turns() {
    use std::f64::consts::PI;
    // three turns from 0: out to 6π, every point on r = θ
    let (segs, features) = spiral("graph r = θ");
    assert!(segs.len() / 4 > 1000);
    let on = |x: f64, y: f64| {
        let (rho, phi) = ((x * x + y * y).sqrt(), y.atan2(x));
        (-1..5)
            .map(|k| (rho - (phi + 2.0 * PI * f64::from(k))).abs())
            .fold(f64::INFINITY, f64::min)
    };
    let far = segs
        .chunks(2)
        .map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt())
        .fold(0.0, f64::max);
    assert!((far - 6.0 * PI).abs() < 1e-6, "{far}");
    assert!(segs.chunks(2).all(|p| on(p[0], p[1]) < 1e-6));
    // its axis crossings, none past the last turn drawn (7π)
    assert!(
        features
            .iter()
            .any(|&(x, y)| (x - 4.0 * PI).abs() < 1e-6 && y.abs() < 1e-9)
    );
    assert!(
        features
            .iter()
            .all(|&(x, y)| (x * x + y * y).sqrt() <= 6.0 * PI + 1e-6)
    );
    assert!(features.iter().all(|&(x, y)| on(x, y) < 1e-6));
    // a typed range of θ and a growing spiral
    let (segs, _) = spiral("graph r = theta {0 < θ < 10pi}");
    let far = segs.chunks(2).map(|p| p[0].hypot(p[1])).fold(0.0, f64::max);
    assert!((far - 10.0 * PI).abs() < 1e-6, "{far}");
    let (segs, _) = spiral("graph r = e^(0.1θ)");
    let far = segs.chunks(2).map(|p| p[0].hypot(p[1])).fold(0.0, f64::max);
    assert!((far - (0.6 * PI).exp()).abs() < 1e-6, "{far}");
    // the window holds it
    let (lo, hi, _) = native_home(
        "graph r = θ",
        &GraphOptions {
            aspect: Some(1.0),
            ..Default::default()
        },
    );
    assert!(
        lo < -6.0 * PI + 2.0 && hi > 6.0 * PI && hi - lo < 80.0,
        "{lo}..{hi}"
    );
    // a curve that repeats each turn is drawn as before
    let (segs, _) = spiral("graph r = cos(2θ)");
    assert!(segs.chunks(2).all(|p| p[0].hypot(p[1]) <= 1.0 + 1e-3));
}
