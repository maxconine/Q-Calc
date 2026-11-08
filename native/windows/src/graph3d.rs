//! The graph panel's 3D view, as maths only (graph.rs draws it): the Mac's `Surface3D` (GraphPanel.swift), a
//! Desmos-3D cube with x and y the window and z `fixed_z` or as wide as x about 0; the patches of each surface cut
//! off at the cube's top and bottom (Sutherland–Hodgman), lit by their normals and depth-sorted with the z = 0
//! plane's grid and the axes, cut into short lengths; and the nearest drawn patch under the pointer (`patch_hit`).
//! Pure, so it's tested here; the base pass keeps the result (`Scene`) for the pointer pass to hit-test.

use qcalc_ffi::SurfaceGrid;

/// A point in the cube, each axis −1…1.
pub type P3 = [f64; 3];
/// A point on the plot, viewBox units.
pub type P2 = (f32, f32);
/// An edge of the cube.
pub type Edge = (P3, P3);

/// The home turn about the vertical and tilt (radians), and the tilt's limits.
pub const HOME_YAW: f64 = -0.62;
pub const HOME_PITCH: f64 = 0.52;
pub const PITCH_MIN: f64 = 0.05;
pub const PITCH_MAX: f64 = 1.45;
/// Shade levels a surface's colour is made at (Mac: 64 steps can't be told apart).
pub const SHADES: usize = 64;

/// The projection: the window's x and y and the cube's z each onto −1…1, turned by `yaw` about the vertical and
/// tilted by `pitch`, with a little perspective. y runs toward the viewer at yaw 0, so the axes are right-handed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cube {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub z: (f64, f64),
    pub yaw: f64,
    pub pitch: f64,
    cx: f32,
    cy: f32,
    s: f32,
}

impl Cube {
    /// The cube for a plot `size` (viewBox units) wide and tall; None for an empty window.
    pub fn new(
        x: (f64, f64),
        y: (f64, f64),
        z: (f64, f64),
        yaw: f64,
        pitch: f64,
        size: (f32, f32),
    ) -> Option<Cube> {
        let ok = |r: (f64, f64)| r.0.is_finite() && r.1.is_finite() && r.1 > r.0;
        if !ok(x) || !ok(y) || !ok(z) {
            return None;
        }
        let (w, h) = (size.0 - 24.0, size.1 - 24.0);
        Some(Cube {
            x,
            y,
            z,
            yaw,
            pitch,
            // room for the cube and its axis names at any turn and tilt
            s: (w / 3.4).min(h / 3.9),
            cx: 12.0 + w / 2.0,
            cy: 12.0 + h / 2.0,
        })
    }

    pub fn u(&self, x: f64) -> f64 {
        (x - self.x.0) / (self.x.1 - self.x.0) * 2.0 - 1.0
    }
    pub fn v(&self, y: f64) -> f64 {
        (y - self.y.0) / (self.y.1 - self.y.0) * 2.0 - 1.0
    }
    pub fn w(&self, z: f64) -> f64 {
        (z - self.z.0) / (self.z.1 - self.z.0) * 2.0 - 1.0
    }
    /// cube units back to the graph's
    pub fn data(&self, p: P3) -> P3 {
        let back = |t: f64, (lo, hi): (f64, f64)| lo + (t + 1.0) / 2.0 * (hi - lo);
        [back(p[0], self.x), back(p[1], self.y), back(p[2], self.z)]
    }

    /// A point of the unit cube on the plot, and how far back it is (larger is farther).
    pub fn unit(&self, u: f64, v: f64, w: f64) -> (P2, f64) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let x = u * cy - v * sy;
        let y = -(u * sy + v * cy);
        let up = w * cp + y * sp;
        let depth = y * cp - w * sp;
        let k = 1.0 / (1.0 + 0.12 * depth);
        (
            (
                self.cx + (x * k) as f32 * self.s,
                self.cy - (up * k) as f32 * self.s,
            ),
            depth,
        )
    }

    pub fn at(&self, p: P3) -> (P2, f64) {
        self.unit(p[0], p[1], p[2])
    }

    /// A point of the graph on the plot; None outside the cube.
    pub fn project(&self, x: f64, y: f64, z: f64) -> Option<P2> {
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return None;
        }
        let (u, v, w) = (self.u(x), self.v(y), self.w(z));
        (u.abs() <= 1.0001 && v.abs() <= 1.0001 && w.abs() <= 1.0001).then(|| self.unit(u, v, w).0)
    }

    /// The cube's 12 edges, the three at its nearest corner last (`front`: they go over everything).
    pub fn edges(&self) -> (Vec<Edge>, Vec<Edge>) {
        let mut corners = Vec::with_capacity(8);
        for a in [-1.0, 1.0] {
            for b in [-1.0, 1.0] {
                for c in [-1.0, 1.0] {
                    corners.push([a, b, c]);
                }
            }
        }
        let nearest = corners
            .iter()
            .copied()
            .min_by(|a, b| self.at(*a).1.total_cmp(&self.at(*b).1))
            .unwrap_or([1.0; 3]);
        let (mut back, mut front) = (Vec::new(), Vec::new());
        for a in &corners {
            for b in &corners {
                let diff = (0..3).filter(|&i| a[i] != b[i]).collect::<Vec<_>>();
                if diff.len() == 1 && a[diff[0]] < b[diff[0]] {
                    if *a == nearest || *b == nearest {
                        front.push((*a, *b));
                    } else {
                        back.push((*a, *b));
                    }
                }
            }
        }
        (back, front)
    }
}

/// The 3D cube's z: `fixed`, or as wide as x about 0. A surface under a tenth that tall would be a flat sheet in
/// that, so its own heights are fitted instead (zWindow).
pub fn z_window(
    fixed: Option<(f64, f64)>,
    x: (f64, f64),
    surface: Option<(f64, f64)>,
) -> (f64, f64) {
    if let Some(z) = fixed {
        return z;
    }
    let half = (x.1 - x.0) / 2.0;
    if let Some((lo, hi)) = surface
        && hi > lo
        && hi - lo < half * 0.2
    {
        let (lo, hi) = (lo.min(0.0), hi.max(0.0));
        let pad = (hi - lo) * 0.15;
        return (lo - pad, hi + pad);
    }
    (-half, half)
}

/// A patch of a surface (cube units) cut off at the cube's top and bottom (Sutherland–Hodgman, twice).
pub fn clip(poly: &[P3]) -> Vec<P3> {
    fn cut(pts: &[P3], keep: impl Fn(f64) -> bool, level: f64) -> Vec<P3> {
        let mut out = Vec::with_capacity(pts.len() + 2);
        for (i, &a) in pts.iter().enumerate() {
            let b = pts[(i + 1) % pts.len()];
            let (ina, inb) = (keep(a[2]), keep(b[2]));
            if ina {
                out.push(a);
            }
            if ina != inb {
                let t = (level - a[2]) / (b[2] - a[2]);
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, level]);
            }
        }
        out
    }
    if poly.iter().all(|p| p[2].abs() <= 1.0) {
        return poly.to_vec();
    }
    if poly.is_empty() {
        return Vec::new();
    }
    let top = cut(poly, |w| w <= 1.0, 1.0);
    if top.is_empty() {
        return top;
    }
    cut(&top, |w| w >= -1.0, -1.0)
}

pub fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn normalize(a: P3) -> P3 {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if n > 0.0 {
        [a[0] / n, a[1] / n, a[2] / n]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// How a length of line in the scene is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// the plane's minor grid (ink 6%, 0.5)
    Minor,
    /// the plane's major grid (ink 16%, 0.5)
    Major,
    /// an axis through the origin (ink 70%, 1)
    Axis,
}

/// A piece of the scene, drawn back to front.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// a patch of a surface: its corners on the plot, its shade (0…SHADES) and its equation (0 the bar's)
    Patch(Vec<P2>, u8, usize),
    Segment(P2, P2, Line),
}

/// A drawn patch, for finding the point under the pointer: its corners on the plot and in the cube.
#[derive(Clone, Debug)]
pub struct Patch {
    pub depth: f64,
    pub screen: Vec<P2>,
    pub cube: Vec<P3>,
    pub layer: usize,
}

/// The base pass's scene: the pieces sorted back to front, and the patches for the pointer pass.
#[derive(Default, Debug)]
pub struct Scene {
    pub pieces: Vec<Piece>,
    pub patches: Vec<Patch>,
}

/// Where the plane and the axes go: the plane at z = 0 (the floor when 0 is out of the cube), the axes through
/// the origin (along the walls when it's outside).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub w0: f64,
    pub clamp_u: f64,
    pub clamp_v: f64,
}

impl Frame {
    pub fn new(c: &Cube) -> Frame {
        Frame {
            w0: if c.z.0 <= 0.0 && 0.0 <= c.z.1 {
                c.w(0.0)
            } else {
                -1.0
            },
            clamp_u: c.u(0.0).clamp(-1.0, 1.0),
            clamp_v: c.v(0.0).clamp(-1.0, 1.0),
        }
    }
}

/// What the scene shows besides the surfaces: the plane's grid values (graph units) and which axes.
#[derive(Default, Clone, Debug)]
pub struct Plane {
    pub grid: bool,
    pub minor: bool,
    pub x_major: Vec<f64>,
    pub x_minor: Vec<f64>,
    pub y_major: Vec<f64>,
    pub y_minor: Vec<f64>,
    pub x_axis: bool,
    pub y_axis: bool,
}

/// The light the patches are shaded by.
const LIGHT: P3 = [-0.35, -0.45, 0.82];

/// The scene: the plane's grid and the axes in short lengths, each surface's patches cut off at the cube's top
/// and bottom and shaded by their normals, all sorted back to front (scene3D).
pub fn scene(c: &Cube, f: &Frame, plane: &Plane, surfaces: &[(&SurfaceGrid, usize)]) -> Scene {
    let mut pieces: Vec<(f64, Piece)> = Vec::with_capacity(2600);
    let mut patches = Vec::new();
    let add_line = |pieces: &mut Vec<(f64, Piece)>, a: P3, b: P3, kind: Line, parts: usize| {
        for k in 0..parts {
            let (t0, t1) = (k as f64 / parts as f64, (k + 1) as f64 / parts as f64);
            let at = |t: f64| {
                [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ]
            };
            let (q0, d0) = c.at(at(t0));
            let (q1, d1) = c.at(at(t1));
            // a hair in front of a surface at the same place
            pieces.push(((d0 + d1) / 2.0 - 0.004, Piece::Segment(q0, q1, kind)));
        }
    };
    let w0 = f.w0;
    if plane.grid {
        if plane.minor {
            for &t in &plane.x_minor {
                add_line(
                    &mut pieces,
                    [c.u(t), -1.0, w0],
                    [c.u(t), 1.0, w0],
                    Line::Minor,
                    6,
                );
            }
            for &t in &plane.y_minor {
                add_line(
                    &mut pieces,
                    [-1.0, c.v(t), w0],
                    [1.0, c.v(t), w0],
                    Line::Minor,
                    6,
                );
            }
        }
        for &t in &plane.x_major {
            add_line(
                &mut pieces,
                [c.u(t), -1.0, w0],
                [c.u(t), 1.0, w0],
                Line::Major,
                8,
            );
        }
        for &t in &plane.y_major {
            add_line(
                &mut pieces,
                [-1.0, c.v(t), w0],
                [1.0, c.v(t), w0],
                Line::Major,
                8,
            );
        }
    }
    if plane.x_axis {
        add_line(
            &mut pieces,
            [-1.0, f.clamp_v, w0],
            [1.0, f.clamp_v, w0],
            Line::Axis,
            16,
        );
    }
    if plane.y_axis {
        add_line(
            &mut pieces,
            [f.clamp_u, -1.0, w0],
            [f.clamp_u, 1.0, w0],
            Line::Axis,
            16,
        );
    }
    add_line(
        &mut pieces,
        [f.clamp_u, f.clamp_v, -1.0],
        [f.clamp_u, f.clamp_v, 1.0],
        Line::Axis,
        16,
    );
    let light = normalize(LIGHT);
    for &(grid, layer) in surfaces {
        let (nx, ny) = (grid.nx as usize, grid.ny as usize);
        if nx == 0 || ny == 0 || grid.z.len() < (nx + 1) * (ny + 1) {
            continue;
        }
        // the grid's corners in the cube once, not four times
        let mut cube: Vec<Option<P3>> = vec![None; (nx + 1) * (ny + 1)];
        for j in 0..=ny {
            for i in 0..=nx {
                let z = grid.z[j * (nx + 1) + i];
                if z.is_finite() {
                    let x = grid.x_lo + i as f64 / nx as f64 * (grid.x_hi - grid.x_lo);
                    let y = grid.y_lo + j as f64 / ny as f64 * (grid.y_hi - grid.y_lo);
                    cube[j * (nx + 1) + i] = Some([c.u(x), c.v(y), c.w(z)]);
                }
            }
        }
        for j in 0..ny {
            for i in 0..nx {
                let k = j * (nx + 1) + i;
                let (Some(a), Some(b), Some(cc), Some(d)) =
                    (cube[k], cube[k + 1], cube[k + nx + 2], cube[k + nx + 1])
                else {
                    continue;
                };
                // wholly above or below the cube: nothing to draw
                if [a, b, cc, d].iter().all(|p| p[2] > 1.0)
                    || [a, b, cc, d].iter().all(|p| p[2] < -1.0)
                {
                    continue;
                }
                let poly = clip(&[a, b, cc, d]);
                if poly.len() < 3 {
                    continue;
                }
                let n = normalize(cross(
                    [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]],
                    [d[0] - b[0], d[1] - b[1], d[2] - b[2]],
                ));
                let lit = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).abs();
                let level = ((lit * SHADES as f64).round() as usize).min(SHADES) as u8;
                let projected: Vec<(P2, f64)> = poly.iter().map(|p| c.at(*p)).collect();
                let depth = projected.iter().map(|p| p.1).sum::<f64>() / projected.len() as f64;
                let screen: Vec<P2> = projected.iter().map(|p| p.0).collect();
                pieces.push((depth, Piece::Patch(screen.clone(), level, layer)));
                patches.push(Patch {
                    depth,
                    screen,
                    cube: poly,
                    layer,
                });
            }
        }
    }
    pieces.sort_by(|a, b| b.0.total_cmp(&a.0));
    Scene {
        pieces: pieces.into_iter().map(|p| p.1).collect(),
        patches,
    }
}

/// A shade level's brightness (0.42…1 of the colour), and the mesh line's (× 0.62).
pub fn shade(level: u8, mesh: bool) -> f32 {
    let t = 0.42 + 0.58 * f32::from(level) / SHADES as f32;
    if mesh { t * 0.62 } else { t }
}

/// The nearest drawn patch under `p` and the place in the cube there (barycentric on the patch's fan), and its
/// equation (patchHit).
pub fn patch_hit(p: P2, patches: &[Patch]) -> Option<(P3, usize)> {
    let bary = |a: P2, b: P2, c: P2| -> Option<(f64, f64, f64)> {
        let d = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
        if d.abs() <= 1e-9 {
            return None;
        }
        let l1 = ((b.1 - c.1) * (p.0 - c.0) + (c.0 - b.0) * (p.1 - c.1)) / d;
        let l2 = ((c.1 - a.1) * (p.0 - c.0) + (a.0 - c.0) * (p.1 - c.1)) / d;
        let l3 = 1.0 - l1 - l2;
        (l1 >= -1e-6 && l2 >= -1e-6 && l3 >= -1e-6).then_some((
            f64::from(l1),
            f64::from(l2),
            f64::from(l3),
        ))
    };
    let mut best: Option<(f64, P3, usize)> = None;
    for patch in patches {
        if best.as_ref().is_some_and(|b| patch.depth >= b.0) || patch.screen.len() < 3 {
            continue;
        }
        // a cheap box test first: most patches are nowhere near the pointer
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for q in &patch.screen {
            x0 = x0.min(q.0);
            y0 = y0.min(q.1);
            x1 = x1.max(q.0);
            y1 = y1.max(q.1);
        }
        if p.0 < x0 || p.0 > x1 || p.1 < y0 || p.1 > y1 {
            continue;
        }
        for k in 1..patch.screen.len() - 1 {
            let Some((l1, l2, l3)) = bary(patch.screen[0], patch.screen[k], patch.screen[k + 1])
            else {
                continue;
            };
            let (a, b, c) = (patch.cube[0], patch.cube[k], patch.cube[k + 1]);
            let at = |i: usize| l1 * a[i] + l2 * b[i] + l3 * c[i];
            best = Some((patch.depth, [at(0), at(1), at(2)], patch.layer));
            break;
        }
    }
    best.map(|b| (b.1, b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Cube {
        Cube::new(
            (-10.0, 10.0),
            (-10.0, 10.0),
            (-10.0, 10.0),
            HOME_YAW,
            HOME_PITCH,
            (652.0, 360.0),
        )
        .unwrap()
    }

    #[test]
    fn projection_centres_the_origin_and_keeps_the_cube_on_the_plot() {
        let c = cube();
        let (o, _) = c.unit(0.0, 0.0, 0.0);
        assert!((o.0 - 326.0).abs() < 0.01 && (o.1 - 180.0).abs() < 0.01);
        for a in [-1.0, 1.0] {
            for b in [-1.0, 1.0] {
                for w in [-1.0, 1.0] {
                    let (q, _) = c.unit(a, b, w);
                    assert!(
                        q.0 > 0.0 && q.0 < 652.0 && q.1 > 0.0 && q.1 < 360.0,
                        "{q:?}"
                    );
                }
            }
        }
        // up is up, and outside the cube is nothing
        assert!(c.project(0.0, 0.0, 5.0).unwrap().1 < o.1);
        assert!(c.project(0.0, 0.0, 11.0).is_none());
        assert!(c.project(f64::NAN, 0.0, 0.0).is_none());
        assert_eq!(c.data([1.0, -1.0, 0.0]), [10.0, -10.0, 0.0]);
        assert!(Cube::new((1.0, 1.0), (0.0, 1.0), (0.0, 1.0), 0.0, 0.5, (652.0, 360.0)).is_none());
    }

    #[test]
    fn three_edges_meet_at_the_nearest_corner() {
        let (back, front) = cube().edges();
        assert_eq!((back.len(), front.len()), (9, 3));
    }

    #[test]
    fn patches_are_cut_off_at_the_walls() {
        // all inside: unchanged
        let inside = [[0.0, 0.0, 0.5], [1.0, 0.0, 0.5], [1.0, 1.0, 0.5]];
        assert_eq!(clip(&inside), inside.to_vec());
        // half above the top: cut at w = 1
        let tall = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 2.0],
            [0.0, 0.0, 2.0],
        ];
        let cut = clip(&tall);
        assert_eq!(cut.len(), 4);
        assert!(cut.iter().all(|p| p[2] <= 1.0 + 1e-12));
        assert!(cut.iter().any(|p| (p[2] - 1.0).abs() < 1e-12));
        // through both: cut twice
        let through = [
            [0.0, 0.0, -3.0],
            [1.0, 0.0, -3.0],
            [1.0, 0.0, 3.0],
            [0.0, 0.0, 3.0],
        ];
        assert!(clip(&through).iter().all(|p| p[2].abs() <= 1.0 + 1e-12));
        // wholly above: nothing
        assert!(clip(&[[0.0, 0.0, 2.0], [1.0, 0.0, 2.0], [1.0, 1.0, 3.0]]).is_empty());
    }

    #[test]
    fn z_is_a_cube_unless_the_surface_is_flat() {
        assert_eq!(
            z_window(None, (-10.0, 10.0), Some((-100.0, 100.0))),
            (-10.0, 10.0)
        );
        assert_eq!(z_window(Some((0.0, 4.0)), (-10.0, 10.0), None), (0.0, 4.0));
        let (lo, hi) = z_window(None, (-360.0, 360.0), Some((-1.0, 1.0)));
        assert!((lo + 1.3).abs() < 1e-9 && (hi - 1.3).abs() < 1e-9);
    }

    fn saddle() -> SurfaceGrid {
        let n = 8u32;
        let mut z = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                let x = -10.0 + f64::from(i) * 20.0 / f64::from(n);
                let y = -10.0 + f64::from(j) * 20.0 / f64::from(n);
                z.push((x * x - y * y) / 10.0);
            }
        }
        SurfaceGrid {
            nx: n,
            ny: n,
            x_lo: -10.0,
            x_hi: 10.0,
            y_lo: -10.0,
            y_hi: 10.0,
            z,
            z_lo: -10.0,
            z_hi: 10.0,
        }
    }

    #[test]
    fn the_scene_is_sorted_back_to_front_and_hit_by_the_pointer() {
        let c = cube();
        let f = Frame::new(&c);
        assert!((f.w0).abs() < 1e-12);
        let grid = saddle();
        let plane = Plane {
            grid: true,
            x_major: vec![-5.0, 0.0, 5.0],
            y_major: vec![0.0],
            x_axis: true,
            y_axis: true,
            ..Plane::default()
        };
        let s = scene(&c, &f, &plane, &[(&grid, 0)]);
        assert_eq!(s.patches.len(), 64);
        assert!(
            s.pieces
                .iter()
                .any(|p| matches!(p, Piece::Segment(_, _, Line::Axis)))
        );
        // the origin's patch: the hit lands at (0, 0, 0) in the cube
        let (o, _) = c.unit(0.0, 0.0, 0.0);
        let (hit, layer) = patch_hit(o, &s.patches).unwrap();
        assert_eq!(layer, 0);
        let d = c.data(hit);
        assert!(
            d[0].abs() < 0.5 && d[1].abs() < 0.5 && d[2].abs() < 0.5,
            "{d:?}"
        );
        assert!(patch_hit((-50.0, -50.0), &s.patches).is_none());
        assert!(shade(SHADES as u8, false) == 1.0 && shade(0, true) < shade(0, false));
    }
}
