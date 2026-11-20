//! QA item 14 (graph 2D): roots, key points and home windows the native panel showed wrong.

use qcalc_core::graph::{CriticalKind, GraphOptions, GraphResult, build_native_graph};
use qcalc_core::graph_native::native_home;

const PANEL: f64 = 160.0 / 600.0;

fn opened(input: &str) -> GraphResult {
    let mut o = GraphOptions {
        aspect: Some(PANEL),
        ..GraphOptions::default()
    };
    let (lo, hi, yc) = native_home(input, &o);
    o.domain = Some((lo, hi));
    o.y_center = yc;
    build_native_graph(input, &o).unwrap_or_else(|| panic!("not a graph: {input}"))
}

fn near(r: &GraphResult, kind: CriticalKind, x: f64, y: f64) -> bool {
    r.critical_points
        .iter()
        .any(|c| c.kind == kind && (c.x - x).abs() < 1e-3 && (c.y - y).abs() < 1e-3)
}

#[test]
fn a_fading_tail_is_not_a_root() {
    // exp(-x²) fell under 1e-12 past ±5.3 and listed roots there
    assert!(opened("graph exp(-x^2)").roots.is_empty());
    assert!(opened("graph e^x").roots.is_empty());
    // real roots and touching roots stay
    assert_eq!(opened("graph x^2").roots, vec![0.0]);
    assert_eq!(opened("graph floor(x)").roots, vec![0.0]);
    assert_eq!(opened("graph x^2 - 4").roots, vec![-2.0, 2.0]);
}

#[test]
fn a_symmetric_pole_is_not_a_filled_hole() {
    // 1/x² and ln(x²) look alike either side of 0 but run off: no max at (0, 10¹⁵) or min at (0, −38)
    assert!(opened("graph 1/x^2").critical_points.is_empty());
    let r = opened("graph ln(x^2)");
    assert!(
        r.critical_points.iter().all(|c| c.x != 0.0),
        "{:?}",
        r.critical_points
    );
    // a real hole is still filled and listed
    assert!(near(&opened("graph sin(x)/x"), CriticalKind::Max, 0.0, 1.0));
}

#[test]
fn a_node_is_not_a_turn() {
    // the folium crosses itself at the origin, where ∇g = 0: no "min" or vertical tangent there
    let r = opened("graph x^3 + y^3 = 6xy");
    assert!(
        r.critical_points.iter().all(|c| !matches!(
            c.kind,
            CriticalKind::Min | CriticalKind::Max | CriticalKind::VerticalTangent
        ) || c.x.abs() > 0.1),
        "{:?}",
        r.critical_points
    );
    assert!(near(&r, CriticalKind::VerticalTangent, 3.1748, 2.5198));
    // sin²x = cos y crosses itself at (±π/2, 0)
    let r = opened("graph sin(x)^2 = cos(y)");
    assert!(
        r.critical_points.iter().all(|c| c.y.abs() > 0.1),
        "{:?}",
        r.critical_points
    );
}

#[test]
fn an_open_curve_opens_at_ten() {
    // curves that run off every window opened on a sliver through their narrowest part
    for input in [
        "graph x^2 - y^2 = 1",
        "graph x^3 + y^3 = 6xy",
        "graph y^2 = x^3",
        "graph y^2 = x^2 (x + 1)",
    ] {
        let (lo, hi, _) = native_home(
            input,
            &GraphOptions {
                aspect: Some(PANEL),
                ..GraphOptions::default()
            },
        );
        assert!(hi - lo >= 20.0, "{input}: [{lo}, {hi}]");
    }
    // the nodal cubic's loop tops are listed in that view
    let r = opened("graph y^2 = x^2 (x + 1)");
    assert!(near(&r, CriticalKind::Max, -2.0 / 3.0, 0.3849));
    // a closed curve still opens framed
    let (lo, hi, _) = native_home(
        "graph x^2 + y^2 = 25",
        &GraphOptions {
            aspect: Some(PANEL),
            ..GraphOptions::default()
        },
    );
    assert!(hi - lo < 60.0 && lo < -5.0 && hi > 5.0);
}
