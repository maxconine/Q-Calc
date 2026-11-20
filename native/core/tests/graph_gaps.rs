//! QA item 39: graph gaps — `sinc`, polar `r = …`, region bounds `{x^2 + y^2 < 4}`, and a ring of maxima found
//! as a dozen points.
use qcalc_core::app::live::{LiveRequest, live};
use qcalc_core::graph::{CriticalKind, GraphOptions, GraphResult, build_native_graph, readout_z};
use qcalc_core::graph_native::native_home;
use qcalc_core::types::AngleMode;

fn opened(input: &str, surface: bool, angle: Option<AngleMode>) -> (GraphOptions, GraphResult) {
    let mut o = GraphOptions {
        aspect: Some(1.0),
        surface,
        angle_mode: angle,
        ..GraphOptions::default()
    };
    let (lo, hi, yc) = native_home(input, &o);
    o.domain = Some((lo, hi));
    o.y_center = yc;
    let r = build_native_graph(input, &o).unwrap_or_else(|| panic!("not a graph: {input}"));
    (o, r)
}

fn has(r: &GraphResult, kind: CriticalKind, x: f64, y: f64) -> bool {
    r.critical_points
        .iter()
        .any(|c| c.kind == kind && (c.x - x).abs() < 1e-4 && (c.y - y).abs() < 1e-4)
}

fn answer(t: &str) -> String {
    live(&LiveRequest {
        text: t.into(),
        ..Default::default()
    })
    .shown
}

#[test]
fn sinc_is_sin_x_over_x() {
    assert_eq!(answer("sinc(0)"), "1");
    assert_eq!(answer("sinc(pi)"), "0");
    assert_eq!(answer("sinc(2)"), "0.454648713413");
    let (_, r) = opened("graph sinc(x)", false, None);
    assert!(r.error.is_none(), "{:?}", r.error);
    assert!(
        has(&r, CriticalKind::Max, 0.0, 1.0),
        "{:?}",
        r.critical_points
    );
}

#[test]
fn polar_curves_draw() {
    // `r = 2` and `r = 1 + cos(θ)` drew nothing (r read as a free name)
    for input in [
        "graph r = 2",
        "graph r = 1 + cos(θ)",
        "graph r(theta) = cos(2 theta)",
    ] {
        let (_, r) = opened(input, false, None);
        assert!(r.error.is_none(), "{input}: {:?}", r.error);
        assert!(
            r.contours.as_ref().is_some_and(|c| !c.is_empty()),
            "{input}"
        );
        assert!(r.intent.label.starts_with("r"), "{}", r.intent.label);
    }
    let (_, r) = opened("graph r = 2", false, None);
    assert!(
        has(&r, CriticalKind::Max, 0.0, 2.0),
        "{:?}",
        r.critical_points
    );
    // the cardioid's far end at (2, 0), the same in degrees
    let (_, r) = opened("graph r = 1 + cos(θ)", false, None);
    assert!(
        has(&r, CriticalKind::VerticalTangent, 2.0, 0.0),
        "{:?}",
        r.critical_points
    );
    let (_, r) = opened("graph r = 1 + cos(θ)", false, Some(AngleMode::Deg));
    assert!(
        has(&r, CriticalKind::VerticalTangent, 2.0, 0.0),
        "{:?}",
        r.critical_points
    );
    // a rose with negative r has all four petals: tops at (0, ±1) and ends at (±1, 0)
    let (_, r) = opened("graph r = cos(2θ)", false, None);
    for (x, y, k) in [
        (0.0, 1.0, CriticalKind::Max),
        (0.0, -1.0, CriticalKind::Min),
    ] {
        assert!(has(&r, k, x, y), "{:?}", r.critical_points);
    }
    // a spiral isn't drawn with a seam: its turns are traced (QA item 97; it said why it wasn't drawn)
    let (_, r) = opened("graph r = θ", false, None);
    assert_eq!(r.error, None);
    assert!(r.contours.as_ref().is_some_and(|c| c[0].segments.len() > 4000));
}

#[test]
fn region_bounds_cut_a_surface() {
    // `{x^2+y^2 < 4}` was "Write bounds like {0 < x < 3}"
    let input = "graph z = x + y {x^2 + y^2 < 4}";
    let (o, r) = opened(input, true, None);
    assert!(
        r.error.is_none() && r.intent.bounds_error.is_none(),
        "{:?}",
        r.error
    );
    assert!(r.surface.is_some());
    assert_eq!(readout_z(input, &o, 1.0, 0.5), Some(1.5));
    assert_eq!(readout_z(input, &o, 3.0, 0.0), None);
    // ranges beside a region stay ranges
    let input = "graph z = x y {0 < x < 5, y < x}";
    let (o, r) = opened(input, true, None);
    assert!(
        r.intent.bounds_error.is_none(),
        "{:?}",
        r.intent.bounds_error
    );
    assert_eq!(readout_z(input, &o, 2.0, 1.0), Some(2.0));
    assert_eq!(readout_z(input, &o, 1.0, 2.0), None);
    assert_eq!(readout_z(input, &o, -1.0, -2.0), None);
}

#[test]
fn a_ring_of_tops_is_one_point() {
    // its top all round r = 1/√2 was listed as a dozen Max points the grid landed on
    for input in [
        "graph z = x^2+y^2 - (x^2+y^2)^2",
        "graph z = (x^2+y^2) e^(-(x^2+y^2))",
    ] {
        let (_, r) = opened(input, true, None);
        let tops = r
            .critical_points
            .iter()
            .filter(|c| c.kind == CriticalKind::Max)
            .count();
        assert_eq!(tops, 1, "{input}: {:?}", r.critical_points);
        assert!(has(&r, CriticalKind::Min, 0.0, 0.0));
    }
    // isolated tops of one height stay
    let (_, r) = opened("graph z = sin(x) sin(y)", true, None);
    let tops = r
        .critical_points
        .iter()
        .filter(|c| c.kind == CriticalKind::Max)
        .count();
    assert!(tops >= 2, "{:?}", r.critical_points);
}
