//! Native only: implicit surfaces, F(x, y, z) = 0 that can't be solved for z (`x^2 + y^2 + z^2 = 1`, a torus).
//! The 3D view draws them as a triangle mesh, found by marching tetrahedra over the view's cube: each cube cell
//! is cut into six tetrahedra along its main diagonal (so neighbours share faces and the mesh has no cracks), and
//! each tetrahedron whose corners differ in sign gives one or two triangles, their corners on its edges where F
//! crosses 0 (a straight-line guess, then one false-position step on F itself). An edge where F jumps sign
//! across a pole (`1/z = x` at z = 0) is left out: F there isn't near 0.

use crate::graph::GraphOptions;
use crate::scientific::compile::{Compiled, compile_scientific3};
use crate::types::AngleMode;

/// The mesh's cells a side (29³ samples; a sphere filling the cube is about 5000 triangles).
pub const IMPLICIT_CELLS: usize = 28;
/// More triangles than this and the surface is noise at this scale (`sin(xyz) = 0`): it's cut short.
const MAX_TRIANGLES: usize = 24_000;

/// F(x, y, z), compiled once.
#[allow(missing_debug_implementations)]
pub struct Implicit3D {
    f: Option<Compiled>,
}

impl Implicit3D {
    pub fn new(expression: &str, options: &GraphOptions) -> Self {
        let ctx = options.sci(options.angle_mode.or(Some(AngleMode::Rad)));
        Implicit3D {
            f: compile_scientific3(expression, &ctx, ["x", "y", "z"]),
        }
    }

    pub fn ok(&self) -> bool {
        self.f.is_some()
    }

    pub fn at(&mut self, x: f64, y: f64, z: f64) -> Option<f64> {
        self.f.as_mut()?.number_at3(x, y, z)
    }
}

// a cell's corners by bit (x = 1, y = 2, z = 4), and its six tetrahedra about the 0–7 diagonal
const TETS: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 3, 2, 7],
    [0, 2, 6, 7],
    [0, 6, 4, 7],
    [0, 4, 5, 7],
    [0, 5, 1, 7],
];

/// The triangles of F = 0 in the box `x` × `y` × `z`, `n` cells a side: nine numbers each (three corners' x, y,
/// z), in graph units.
pub fn implicit_mesh(
    f: &mut Implicit3D,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
    n: usize,
) -> Vec<f64> {
    let mut out = Vec::new();
    if !f.ok() || n == 0 {
        return out;
    }
    let s = n + 1;
    let step = (
        (x.1 - x.0) / n as f64,
        (y.1 - y.0) / n as f64,
        (z.1 - z.0) / n as f64,
    );
    let at = |i: usize, j: usize, k: usize| {
        (
            x.0 + i as f64 * step.0,
            y.0 + j as f64 * step.1,
            z.0 + k as f64 * step.2,
        )
    };
    let mut v = vec![f64::NAN; s * s * s];
    for k in 0..s {
        for j in 0..s {
            for i in 0..s {
                let p = at(i, j, k);
                v[(k * s + j) * s + i] = f.at(p.0, p.1, p.2).unwrap_or(f64::NAN);
            }
        }
    }
    // where F crosses 0 between two corners, or None across a pole
    let mut cross = |a: ((f64, f64, f64), f64), b: ((f64, f64, f64), f64)| {
        let lerp = |a: (f64, f64, f64), b: (f64, f64, f64), t: f64| {
            (
                a.0 + (b.0 - a.0) * t,
                a.1 + (b.1 - a.1) * t,
                a.2 + (b.2 - a.2) * t,
            )
        };
        let p = lerp(a.0, b.0, a.1 / (a.1 - b.1));
        let fp = f.at(p.0, p.1, p.2)?;
        if fp.abs() > a.1.abs().max(b.1.abs()) {
            return None;
        }
        if fp == 0.0 {
            return Some(p);
        }
        // one false-position step on the half that still changes sign
        let (l, r) = if (fp < 0.0) == (a.1 < 0.0) {
            ((p, fp), b)
        } else {
            (a, (p, fp))
        };
        Some(lerp(l.0, r.0, l.1 / (l.1 - r.1)))
    };
    let push = |out: &mut Vec<f64>, t: [(f64, f64, f64); 3]| {
        for p in t {
            out.extend([p.0, p.1, p.2]);
        }
    };
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let corner = |c: usize| {
                    let (a, b, d) = (i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1));
                    (at(a, b, d), v[(d * s + b) * s + a])
                };
                let cs: [((f64, f64, f64), f64); 8] = std::array::from_fn(corner);
                if cs.iter().any(|c| !c.1.is_finite()) {
                    continue;
                }
                let neg = cs.iter().filter(|c| c.1 < 0.0).count();
                if neg == 0 || neg == 8 {
                    continue;
                }
                for t in TETS {
                    let q = t.map(|c| cs[c]);
                    let inside: Vec<usize> = (0..4).filter(|&m| q[m].1 < 0.0).collect();
                    let outside: Vec<usize> = (0..4).filter(|&m| q[m].1 >= 0.0).collect();
                    match inside.len() {
                        1 | 3 => {
                            let (lone, rest) = if inside.len() == 1 {
                                (inside[0], &outside)
                            } else {
                                (outside[0], &inside)
                            };
                            let ps: Vec<_> =
                                rest.iter().filter_map(|&m| cross(q[lone], q[m])).collect();
                            if ps.len() == 3 {
                                push(&mut out, [ps[0], ps[1], ps[2]]);
                            }
                        }
                        2 => {
                            let (a, b, c, d) = (inside[0], inside[1], outside[0], outside[1]);
                            let ps = [
                                cross(q[a], q[c]),
                                cross(q[a], q[d]),
                                cross(q[b], q[d]),
                                cross(q[b], q[c]),
                            ];
                            if let [Some(p0), Some(p1), Some(p2), Some(p3)] = ps {
                                push(&mut out, [p0, p1, p2]);
                                push(&mut out, [p0, p2, p3]);
                            }
                        }
                        _ => {}
                    }
                }
                if out.len() >= MAX_TRIANGLES * 9 {
                    return out;
                }
            }
        }
    }
    out
}

/// The first view of an implicit surface: a cube about the origin (the 3D view's z is always about 0) that
/// holds all of it, found on a coarse grid at ±10, else ±100, else ±1, and then framed again at its own size.
/// None when F changes sign nowhere in those.
pub fn implicit_surface_home(f: &mut Implicit3D) -> Option<(f64, f64)> {
    // the furthest from the origin (in any one axis) that a cell changing sign reaches, at ±h
    let reach = |f: &mut Implicit3D, h: f64| -> Option<f64> {
        const N: usize = 24;
        let s = N + 1;
        let d = 2.0 * h / N as f64;
        let mut v = vec![f64::NAN; s * s * s];
        for k in 0..s {
            for j in 0..s {
                for i in 0..s {
                    let p = (-h + i as f64 * d, -h + j as f64 * d, -h + k as f64 * d);
                    v[(k * s + j) * s + i] = f.at(p.0, p.1, p.2).unwrap_or(f64::NAN);
                }
            }
        }
        let mut most: Option<f64> = None;
        for k in 0..s {
            for j in 0..s {
                for i in 0..s {
                    let a = v[(k * s + j) * s + i];
                    if !a.is_finite() {
                        continue;
                    }
                    for (di, dj, dk) in [(1, 0, 0), (0, 1, 0), (0, 0, 1)] {
                        let (i2, j2, k2) = (i + di, j + dj, k + dk);
                        if i2 >= s || j2 >= s || k2 >= s {
                            continue;
                        }
                        let b = v[(k2 * s + j2) * s + i2];
                        if b.is_finite() && (a < 0.0) != (b < 0.0) {
                            let far = [i, j, k, i2, j2, k2]
                                .iter()
                                .map(|&m| (-h + m as f64 * d).abs())
                                .fold(0.0, f64::max);
                            most = Some(most.map_or(far, |m: f64| m.max(far)));
                        }
                    }
                }
            }
        }
        most
    };
    let first = [10.0, 100.0, 1.0]
        .into_iter()
        .find_map(|h| reach(f, h).map(|m| (h, m)))?;
    let (h, m) = first;
    // seen at its own size, to frame it tighter (a cell of ±h is a twelfth of h)
    let m = if m < h * 0.6 {
        reach(f, m * 1.1).unwrap_or(m)
    } else {
        m
    };
    let (lo, hi) = crate::graph::nice_window(0.0, 2.0 * m * 1.25);
    (hi > lo).then_some((lo, hi))
}
