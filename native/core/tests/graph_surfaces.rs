//! Surfaces (item 15): the 3D view's first window and critical points.

use qcalc_core::graph::{CriticalKind, GraphOptions, GraphResult, build_native_graph};
use qcalc_core::graph_native::native_home;

/// The surface as the panel opens it in 3D (a square of x and y about the y centre).
fn opened(input: &str) -> (f64, f64, Option<f64>, GraphResult) {
    let mut o = GraphOptions {
        aspect: Some(1.0),
        surface: true,
        ..GraphOptions::default()
    };
    let (lo, hi, yc) = native_home(input, &o);
    o.domain = Some((lo, hi));
    o.y_center = yc;
    let r = build_native_graph(input, &o).unwrap_or_else(|| panic!("not a graph: {input}"));
    (lo, hi, yc, r)
}

fn has(r: &GraphResult, kind: CriticalKind, x: f64, y: f64) -> bool {
    r.critical_points
        .iter()
        .any(|c| c.kind == kind && (c.x - x).abs() < 1e-6 && (c.y - y).abs() < 1e-6)
}

#[test]
fn a_hemisphere_opens_where_it_is_defined() {
    // at ±10 it was a dot in the middle, 21 of 2401 grid points, and its top went unfound
    let (lo, hi, yc, r) = opened("graph z = sqrt(1-x^2-y^2)");
    assert!(lo < -1.0 && hi > 1.0 && hi - lo <= 4.0, "{lo}..{hi}");
    assert_eq!(yc, None);
    assert!(
        has(&r, CriticalKind::Max, 0.0, 0.0),
        "{:?}",
        r.critical_points
    );
    let (lo, hi, _, _) = opened("graph z = sqrt(25-x^2-y^2)");
    assert!(lo < -5.0 && hi > 5.0 && hi - lo < 20.0, "{lo}..{hi}");
}

#[test]
fn a_bump_is_framed_and_not_clipped() {
    // at ±10 the peak was clipped off as an outlier: z framed 0 … 0.06 for a top of 1
    let (lo, hi, _, r) = opened("graph z = e^(-(x^2+y^2))");
    assert!(lo < -1.5 && hi > 1.5 && hi - lo < 10.0, "{lo}..{hi}");
    let s = r.surface.expect("grid");
    assert!(s.z_hi > 0.9, "{}", s.z_hi);
}

#[test]
fn a_surface_window_is_centred_when_it_holds_the_origin() {
    // the features' x alone made it −4.5 … 6 (y is centred on 0)
    let (lo, hi, _, _) = opened("graph z = sin(sqrt(x^2+y^2))");
    assert_eq!(lo, -hi);
}

#[test]
fn minima_tied_on_the_grid_are_found() {
    // x⁴ + y⁴ − 4xy at ±10: the grid's gradients tie at (1, 1) and (0.5, 0.5), and a strict test lost both
    let (lo, hi, _, r) = opened("graph z = x^4+y^4-4xy");
    assert!(
        has(&r, CriticalKind::Min, 1.0, 1.0),
        "{:?}",
        r.critical_points
    );
    assert!(has(&r, CriticalKind::Min, -1.0, -1.0));
    assert!(has(&r, CriticalKind::Saddle, 0.0, 0.0));
    assert!(hi - lo < 10.0, "{lo}..{hi}");
}

#[test]
fn plain_surfaces_keep_their_window() {
    for i in [
        "graph z = x^2 - y^2",
        "graph z = x",
        "graph z = 5",
        "graph x+y+z=1",
        "graph z = cos x + cos y",
        "graph z = ln(x)",
    ] {
        let (lo, hi, yc, _) = opened(i);
        assert_eq!((lo, hi, yc), (-10.0, 10.0, None), "{i}");
    }
    // a plane has no critical point
    assert!(opened("graph z = x").3.critical_points.is_empty());
}
