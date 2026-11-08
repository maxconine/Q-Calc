//! Native graphing (`graph_native.rs`): LaTeX input, implicit curves and their key points, equations solved for
//! y or z, lines through points, and surfaces' critical points and 3D grid.

use qcalc_core::graph::{CriticalKind, GraphOptions, GraphResult, build_native_graph};
use qcalc_core::graph_native::native_home;

const PANEL: f64 = 160.0 / 600.0;

/// The graph as the panel opens it: at its home window, with its y centre.
fn opened(input: &str, aspect: f64, surface: bool) -> GraphResult {
    let mut o = GraphOptions {
        aspect: Some(aspect),
        surface,
        ..GraphOptions::default()
    };
    let (lo, hi, yc) = native_home(input, &o);
    o.domain = Some((lo, hi));
    o.y_center = yc;
    build_native_graph(input, &o).unwrap_or_else(|| panic!("not a graph: {input}"))
}

fn points(r: &GraphResult) -> Vec<(CriticalKind, f64, f64)> {
    r.critical_points
        .iter()
        .map(|c| (c.kind, c.x, c.y))
        .collect()
}

fn has(r: &GraphResult, kind: CriticalKind, x: f64, y: f64) -> bool {
    r.critical_points
        .iter()
        .any(|c| c.kind == kind && (c.x - x).abs() < 1e-6 && (c.y - y).abs() < 1e-6)
}

fn segments(r: &GraphResult) -> usize {
    r.contours
        .as_ref()
        .map_or(0, |c| c.iter().map(|l| l.segments.len() / 4).sum())
}

#[test]
fn a_sideways_parabola_in_latex_is_framed_whole() {
    let r = opened("graph y^{2}=10000x", PANEL, false);
    assert_eq!(r.error, None);
    assert_eq!(r.intent.label, "y^2 = 10000x");
    assert!(segments(&r) > 100);
    // zoomed out to where it bends (the default ±10 shows a sliver at x = 0)
    assert!(r.domain.1 - r.domain.0 > 1e5, "{:?}", r.domain);
    assert!(r.domain.0 < 0.0 && r.domain.1 > 1e5);
    assert!(
        has(&r, CriticalKind::VerticalTangent, 0.0, 0.0),
        "{:?}",
        points(&r)
    );
}

#[test]
fn implicit_curves_list_their_turns_and_intercepts() {
    let circle = opened("graph x^2+y^2=25", PANEL, false);
    // the whole circle fits: its height is inside the window's y
    assert!(circle.y_scale.min < -5.0 && circle.y_scale.max > 5.0);
    assert!(has(&circle, CriticalKind::Max, 0.0, 5.0));
    assert!(has(&circle, CriticalKind::Min, 0.0, -5.0));
    assert!(has(&circle, CriticalKind::VerticalTangent, 5.0, 0.0));
    assert!(has(&circle, CriticalKind::VerticalTangent, -5.0, 0.0));
    assert_eq!(circle.critical_points.len(), 4, "{:?}", points(&circle));

    // far from the origin and small: found, and the window centred on it
    let away = opened("graph (x-3)^2+(y-50)^2=4", PANEL, false);
    assert!(segments(&away) > 50);
    assert!(
        away.y_scale.min < 48.0 && away.y_scale.max > 52.0,
        "{:?}",
        away.y_scale
    );
    assert!(has(&away, CriticalKind::Max, 3.0, 52.0));

    let cubic = opened("graph y^3 - 3y = x", PANEL, false);
    assert!(has(&cubic, CriticalKind::VerticalTangent, -2.0, 1.0));
    assert!(has(&cubic, CriticalKind::VerticalTangent, 2.0, -1.0));
    assert!(has(&cubic, CriticalKind::XIntercept, 0.0, 0.0));
    assert!(
        cubic
            .critical_points
            .iter()
            .any(|c| c.kind == CriticalKind::YIntercept && (c.y - 3f64.sqrt()).abs() < 1e-6)
    );

    let vertical = opened("graph x = 3", PANEL, false);
    assert_eq!(vertical.error, None);
    assert!(has(&vertical, CriticalKind::XIntercept, 3.0, 0.0));
}

#[test]
fn point_slope_and_other_linear_equations_are_lines() {
    let r = opened("graph y - 3 = 2(x - 1)", PANEL, false);
    assert!(r.contours.is_none(), "drawn as a curve");
    assert_eq!(r.intent.label, "y - 3 = 2(x - 1) → y = 2x + 1");
    assert!(has(&r, CriticalKind::XIntercept, -0.5, 0.0));
    assert!(has(&r, CriticalKind::YIntercept, 0.0, 1.0));
    assert_eq!(r.y_at(4.0), Some(9.0));

    let r = opened("graph 2y + 3x = 6", PANEL, false);
    assert_eq!(r.intent.label, "2y + 3x = 6 → y = −1.5x + 3");
    assert!(has(&r, CriticalKind::XIntercept, 2.0, 0.0));

    // linear in y but not a line: a curve, with no line form
    let r = opened("graph xy = 1", PANEL, false);
    assert!(r.contours.is_none());
    assert_eq!(r.intent.label, "x*y = 1");
    assert_eq!(r.y_at(4.0), Some(0.25));

    // not linear in y: stays implicit
    assert!(
        opened("graph x^2 + y^2 = 4", PANEL, false)
            .contours
            .is_some()
    );
}

#[test]
fn lines_through_points() {
    let r = opened("graph line through (1, 2) and (3, 5)", PANEL, false);
    assert_eq!(r.intent.label, "y = 1.5x + 0.5 through (1, 2) and (3, 5)");
    assert!(has(&r, CriticalKind::Point, 1.0, 2.0) && has(&r, CriticalKind::Point, 3.0, 5.0));
    assert_eq!(r.critical_points[0].kind, CriticalKind::Point);
    assert_eq!(r.y_at(2.0), Some(3.5), "interpolated between the points");
    assert!(has(&r, CriticalKind::YIntercept, 0.0, 0.5));

    for input in [
        "graph (1,2) slope 3",
        "graph slope 3 through (1, 2)",
        "graph m = 3, (1, 2)",
    ] {
        let r = opened(input, PANEL, false);
        assert_eq!(r.error, None, "{input}");
        assert!(
            r.intent.label.starts_with("y = 3x − 1"),
            "{input}: {}",
            r.intent.label
        );
        assert_eq!(r.y_at(2.0), Some(5.0), "{input}");
    }

    // two points one above the other: x = 1
    let r = opened("graph (1, 2), (1, 5)", PANEL, false);
    assert_eq!(r.error, None);
    assert!(r.contours.is_some());

    // three or more: joined piece by piece, only between the first and last
    let r = opened("graph interpolate (0,0), (1,3), (2,1), (4,5)", PANEL, false);
    assert_eq!(r.error, None);
    assert_eq!(r.y_at(0.5), Some(1.5));
    assert_eq!(r.y_at(1.5), Some(2.0));
    assert_eq!(r.y_at(3.0), Some(3.0));
    assert_eq!(r.y_at(5.0), None);
    assert_eq!(
        r.critical_points
            .iter()
            .filter(|c| c.kind == CriticalKind::Point)
            .count(),
        4
    );
    assert!(
        opened("graph (0,0), (1,3), (1,4)", PANEL, false)
            .error
            .is_some()
    );

    // a point alone isn't a line, and calls stay calls
    assert!(opened("graph (1, 2)", PANEL, false).error.is_some());
    assert_eq!(opened("graph sin(x)", PANEL, false).error, None);
}

#[test]
fn surfaces_have_critical_points_and_a_grid() {
    let r = opened("graph z = x^2 - y^2", 1.0, true);
    assert_eq!(r.error, None);
    let saddle = &r.critical_points;
    assert_eq!(saddle.len(), 1, "{:?}", points(&r));
    assert_eq!(
        (saddle[0].kind, saddle[0].x, saddle[0].y, saddle[0].z),
        (CriticalKind::Saddle, 0.0, 0.0, Some(0.0))
    );
    let g = r.surface.as_ref().expect("a grid");
    assert_eq!(g.z.len(), (g.nx + 1) * (g.ny + 1));
    assert_eq!((g.z_lo, g.z_hi), (-100.0, 100.0));

    let bump = opened("graph z = exp(-(x^2+y^2)/8)", 1.0, true);
    assert_eq!(bump.critical_points[0].kind, CriticalKind::Max);
    assert_eq!(bump.critical_points[0].z, Some(1.0));

    let bowl = opened("graph 3d x^2+y^2", 1.0, true);
    assert_eq!(bowl.critical_points[0].kind, CriticalKind::Min);

    // no grid unless asked
    assert!(opened("graph z = x^2 - y^2", 1.0, false).surface.is_none());

    // an equation linear in z is a surface; one that isn't asks for z = …
    let plane = opened("graph x + y + z = 1", 1.0, true);
    assert_eq!(plane.error, None);
    assert!(plane.critical_points.is_empty());
    assert_eq!(plane.z_at(1.0, 2.0), Some(-2.0));
    assert!(opened("graph x^2+y^2+z^2=25", 1.0, true).error.is_some());
    // a coefficient on z is solved through: 20z = 2y + 12x is z = (2y + 12x)/20
    let scaled = opened("graph 20z=2y+12x", 1.0, true);
    assert_eq!(scaled.error, None);
    let z = scaled.z_at(1.0, 2.0).expect("a surface");
    assert!((z - 0.8).abs() < 1e-12, "{z}");
    let z = opened("graph 3z - x = y", 1.0, true)
        .z_at(3.0, 6.0)
        .expect("a surface");
    assert!((z - 3.0).abs() < 1e-12, "{z}");
    // and on y, as a curve
    let line = opened("graph 4y = 2x + 8", PANEL, false);
    assert_eq!(line.error, None);
    assert!(line.y_at(2.0).is_some_and(|y| (y - 3.0).abs() < 1e-12));
}

#[test]
fn a_line_along_an_axis_is_not_a_row_of_intercepts() {
    // x² = x is the lines x = 0 and x = 1; the first lies on the y axis
    let r = opened("graph x^2 = x", PANEL, false);
    let ys = r
        .critical_points
        .iter()
        .filter(|c| c.kind == CriticalKind::YIntercept)
        .count();
    assert_eq!(ys, 0, "{:?}", points(&r));
    assert!(
        has(&r, CriticalKind::XIntercept, 0.0, 0.0) && has(&r, CriticalKind::XIntercept, 1.0, 0.0),
        "{:?}",
        points(&r)
    );
    assert!(r.critical_points.len() <= 3, "{:?}", points(&r));
}

#[test]
fn bounds_reach_infinity_and_mark_their_ends() {
    let o = GraphOptions {
        domain: Some((-10.0, 10.0)),
        ..GraphOptions::default()
    };
    let ends = |s: &str| {
        let r = build_native_graph(s, &o).expect("a graph");
        assert_eq!(r.error, None, "{s}");
        r.critical_points
            .iter()
            .filter(|c| matches!(c.kind, CriticalKind::Endpoint | CriticalKind::OpenEndpoint))
            .map(|c| (c.kind, c.x, c.y))
            .collect::<Vec<_>>()
    };
    // ∞ (or inf) is open on that side; the end that's left out is an open endpoint, at the height the curve reaches
    assert_eq!(
        ends("graph x^2-3 {3<x<∞}"),
        [(CriticalKind::OpenEndpoint, 3.0, 6.0)]
    );
    assert_eq!(
        ends("graph x^2-3 {3<x<inf}"),
        [(CriticalKind::OpenEndpoint, 3.0, 6.0)]
    );
    assert_eq!(
        ends("graph x^2-3 {-∞<x<=2}"),
        [(CriticalKind::Endpoint, 2.0, 1.0)]
    );
    assert_eq!(
        ends("graph x^2-3 {-1<=x<5}"),
        [
            (CriticalKind::Endpoint, -1.0, -2.0),
            (CriticalKind::OpenEndpoint, 5.0, 22.0)
        ]
    );
    // an end outside the window isn't marked
    let far = build_native_graph("graph x^2-3 {-30<=x<=30}", &o).expect("a graph");
    assert!(
        far.critical_points
            .iter()
            .all(|c| !matches!(c.kind, CriticalKind::Endpoint | CriticalKind::OpenEndpoint))
    );
}

#[test]
fn the_home_view_shows_every_key_point() {
    let home = |s: &str| {
        let (lo, hi, _) = native_home(
            s,
            &GraphOptions {
                aspect: Some(PANEL),
                ..GraphOptions::default()
            },
        );
        (lo, hi)
    };
    // a far root and the y-intercept both in view
    let (lo, hi) = home("graph y = 2x+34");
    assert!(lo < -17.0 && hi > 0.0, "{lo} {hi}");
    // turns past ±10
    let (lo, hi) = home("graph x^3-1000x");
    assert!(lo < -31.7 && hi > 31.7, "{lo} {hi}");
    // a vertex far off, with the y axis
    let (lo, hi) = home("graph (x-50)^2+3");
    assert!(lo <= 0.0 && hi > 50.0, "{lo} {hi}");
    // a root where the curve starts
    let (lo, hi) = home("graph sqrt(x-40)");
    assert!(lo < 40.0 && hi > 40.0, "{lo} {hi}");
    // roots on the page's edge get room
    let (lo, hi) = home("graph 0.01x^3 - x");
    assert!(lo < -10.0 && hi > 10.0, "{lo} {hi}");
    // endless features (sin x) and none at all (e^x, whose tiny values aren't roots) keep the page's view
    assert_eq!(home("graph sin(x)"), (-10.0, 10.0));
    assert_eq!(home("graph e^x"), (-10.0, 10.0));
}

#[test]
fn a_log_x_axis_samples_evenly_in_log_x() {
    let o = GraphOptions {
        domain: Some((0.001, 1000.0)),
        x_log: true,
        ..GraphOptions::default()
    };
    let r = build_native_graph("graph log(x)", &o).expect("a graph");
    assert_eq!(r.error, None);
    let xs: Vec<f64> = r.points.iter().map(|p| p.x).collect();
    assert_eq!((xs[0], *xs.last().unwrap()), (0.001, 1000.0));
    // each decade gets about as many samples: the lowest isn't left bare
    let below = xs.iter().filter(|&&x| x < 0.01).count();
    assert!(below * 8 > xs.len() / 6, "{below} of {}", xs.len());
    // linear: almost none there
    let lin = build_native_graph("graph log(x)", &GraphOptions { x_log: false, ..o }).unwrap();
    assert!(lin.points.iter().filter(|p| p.x < 0.01).count() <= 2);
}
