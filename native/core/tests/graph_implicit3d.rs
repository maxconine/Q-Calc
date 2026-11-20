//! Implicit surfaces (QA item 47): `x^2 + y^2 + z^2 = 1` was "Write a surface as z = …"; it's now a triangle
//! mesh for the 3D view, framed around the surface.

use qcalc_core::graph::{GraphOptions, GraphResult, build_native_graph};
use qcalc_core::graph_native::native_home;
use std::time::Instant;

/// As the panel opens a surface in 3D: home first, then a square of x and y with the grid.
fn opened(input: &str) -> (f64, f64, GraphResult) {
    let mut o = GraphOptions {
        aspect: Some(1.0),
        surface: true,
        ..GraphOptions::default()
    };
    let (lo, hi, yc) = native_home(input, &o);
    o.domain = Some((lo, hi));
    o.y_center = yc;
    let r = build_native_graph(input, &o).unwrap_or_else(|| panic!("not a graph: {input}"));
    (lo, hi, r)
}

/// Every mesh corner's distance from the origin
fn radii(r: &GraphResult) -> Vec<f64> {
    let m = &r.surface.as_ref().expect("a 3D grid").mesh;
    assert_eq!(m.len() % 9, 0);
    m.chunks(3)
        .map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
        .collect()
}

#[test]
fn a_sphere_is_a_closed_mesh_on_its_radius() {
    let t = Instant::now();
    let (lo, hi, r) = opened("graph x^2+y^2+z^2=1");
    assert!(r.error.is_none(), "{:?}", r.error);
    assert!(lo < -1.0 && hi > 1.0 && hi - lo <= 4.0, "{lo}..{hi}");
    let rs = radii(&r);
    assert!(rs.len() > 900, "{} corners", rs.len());
    assert!(rs.iter().all(|d| (d - 1.0).abs() < 0.02), "off the sphere");
    // the cube's z is as wide as x, about 0
    let g = r.surface.as_ref().unwrap();
    assert_eq!((g.z_lo, g.z_hi), (lo, hi));
    // both halves, not just a hemisphere
    let zs: Vec<f64> = g.mesh.chunks(3).map(|p| p[2]).collect();
    assert!(zs.iter().any(|&z| z > 0.95) && zs.iter().any(|&z| z < -0.95));
    assert!(t.elapsed().as_millis() < 3000, "{:?}", t.elapsed());
}

#[test]
fn other_implicit_surfaces_draw() {
    for input in [
        "graph x^2 + y^2 + z^2 = 25",
        "graph x^2/4 + y^2 + z^2/9 = 1",
        "graph (sqrt(x^2+y^2) - 2)^2 + z^2 = 1",
        "graph x^2 + y^2 - z^2 = 1",
        "graph z^2 = x^2 + y^2",
        "graph z^3 = x*y + 1",
        "graph sin(x) + sin(y) + sin(z) = 0",
    ] {
        let (lo, hi, r) = opened(input);
        assert!(r.error.is_none(), "{input}: {:?}", r.error);
        let m = &r.surface.as_ref().unwrap().mesh;
        assert!(
            m.len() >= 9 * 50,
            "{input}: {} numbers on {lo}..{hi}",
            m.len()
        );
    }
    let (lo, hi, _) = opened("graph x^2 + y^2 + z^2 = 25");
    assert!(lo < -5.0 && hi > 5.0 && hi - lo <= 20.0, "{lo}..{hi}");
}

#[test]
fn a_pole_is_not_a_sheet() {
    // 1/z = x: F jumps sign across z = 0 without crossing it there
    let (_, _, r) = opened("graph 1/z = x");
    let m = &r.surface.as_ref().unwrap().mesh;
    assert!(!m.is_empty());
    for p in m.chunks(3) {
        assert!(
            (1.0 / p[2] - p[0]).abs() < 0.2 * (1.0 + p[0].abs()),
            "({}, {}, {})",
            p[0],
            p[1],
            p[2]
        );
    }
}

#[test]
fn surfaces_solved_for_z_stay_grids() {
    let (_, _, r) = opened("graph x + y + z = 1");
    let g = r.surface.as_ref().unwrap();
    assert!(g.mesh.is_empty() && g.nx > 0);
    let (_, _, r) = opened("graph z = x^2 - y^2");
    assert!(r.surface.unwrap().mesh.is_empty());
}
