//! Native-only graphing on top of [`crate::graph`] (the TS engine has none of it, and `build_graph` stays the
//! web's for the golden suite):
//!
//! - **Input**: LaTeX (`y^{2}=10000x`), `3d` markers (`graph 3d x^2 - y^2`), and lines through points: two points
//!   (`graph line through (1, 2) and (3, 5)`), a point and a slope (`graph (1, 2) slope 3`), or three or more
//!   points joined piece by piece (`graph interpolate (0, 0), (1, 3), (2, 1)`).
//! - **Equations solved for y or z**: an equation linear in y (point-slope `y − 3 = 2(x − 1)`, `2y + 3x = 6`)
//!   is drawn as a curve, so it gets roots, turns and intercepts; one linear in z (`x + y + z = 1`) is a surface.
//! - **Implicit curves** (`x^2 + y^2 = 25`, `y^2 = 10000x`): a home window that frames the curve (x and y keep one
//!   scale), and its features: tops and bottoms (min/max of y), vertical tangents and axis intercepts.
//! - **Surfaces**: critical points (min, max, saddle, with z) and a grid of z for the 3D view.

// `!(x > y)` is true for NaN too, as graph.rs has it
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::format::format_number;
use crate::graph::{
    ContourKind, ContourLevel, CriticalKind, CriticalPoint, GraphBounds, GraphIntent, GraphOptions,
    GraphRange, SurfaceGrid, build_intent, build_native_graph, compile_graph_y, compile_graph_z,
    evaluate_graph_y, home_of, marching_squares, nice_window, parse_graph_intent, quantile,
    rewrite_params, sort_numbers, tidy, with_minus,
};
use crate::jsre::{self, Lazy, js};
use crate::latex::{latex_to_ascii, looks_like_latex};
use crate::scientific::functions::jsmath;
use crate::types::AngleMode;

/// What [`scan_home`] keeps of its best zoom: score, quarter-decade, the drawn box, whether it holds the curve.
type Zoom = (f64, i32, (f64, f64, f64, f64), bool);

/// What the native panel graphs for an input: the intent, the points a line was drawn through, and an error
/// that replaces the plot's (a duplicate x among the points, an implicit surface).
#[derive(Clone, Debug)]
pub struct NativeIntent {
    pub intent: GraphIntent,
    pub points: Vec<(f64, f64)>,
    pub error: Option<String>,
}

/// The plot's window in graph units.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub x: (f64, f64),
    pub y: (f64, f64),
}

impl Window {
    fn span(&self) -> f64 {
        (self.x.1 - self.x.0).max(self.y.1 - self.y.0)
    }
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x.0 && x <= self.x.1 && y >= self.y.0 && y <= self.y.1
    }
}

/// √(a² + b²) (clippy keeps `f64::hypot` out: the TS has its own)
fn len2(a: f64, b: f64) -> f64 {
    (a * a + b * b).sqrt()
}

const MAX_FEATURES: usize = 24;
const MAX_SURFACE_POINTS: usize = 12;
/// The 3D view's grid, cells a side.
pub const SURFACE_CELLS: usize = 48;

// ---- input ----

/// The `graph …` line with LaTeX read as ascii and a `3d` marker dropped (the panel picks the view itself).
fn normalize(input: &str) -> Option<String> {
    static CMD: Lazy = Lazy::new(|| js(r"(?s)^\s*graph\s+(.+?)\s*$", "i"));
    static THREE_D: Lazy =
        Lazy::new(|| js(r"^(?:3d|3-d)\s*[:,]?\s+|\s+(?:in\s+)?(?:3d|3-d)$", "i"));
    // a bounds group: braces around a comparison
    static BOUNDS: Lazy = Lazy::new(|| js(r"\{[^{}]*[<>≤≥][^{}]*\}", ""));
    let m = CMD.captures(input).ok()??;
    let body = jsre::replace_all(&THREE_D, jsre::group(&m, 1), |_| String::new()).into_owned();
    // LaTeX before the bounds only, so `{0 < x < 3}` stays bounds
    let cut = BOUNDS
        .find(&body)
        .ok()
        .flatten()
        .map_or(body.len(), |b| b.start());
    let (head, tail) = body.split_at(cut);
    static GLUED: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z_])(x)(y)(?![A-Za-z0-9_(])|(?<![A-Za-z_])(y)(x)(?![A-Za-z0-9_(])",
            "",
        )
    });
    let latex = looks_like_latex(head);
    let mut head = if latex {
        latex_to_ascii(head)
    } else {
        head.to_string()
    };
    // `xy` is x times y
    head = jsre::replace_all(&GLUED, &head, |c| {
        let g = |i| jsre::group(c, i);
        if g(1).is_empty() {
            format!("{}*{}", g(3), g(4))
        } else {
            format!("{}*{}", g(1), g(2))
        }
    })
    .into_owned();
    Some(
        format!("graph {} {}", head.trim(), tail.trim())
            .trim()
            .to_string(),
    )
}

/// LaTeX read as ascii, for the panel's label: `y^(2)` as `y^2`, no parens around the whole.
fn tidy_label(label: &str) -> String {
    static POW: Lazy = Lazy::new(|| js(r"\^\(([A-Za-z0-9.]+)\)", ""));
    let mut s = jsre::replace_all(&POW, label, |c| format!("^{}", jsre::group(c, 1))).into_owned();
    let balanced = |t: &str| {
        let mut depth = 0i32;
        t.chars().all(|c| {
            depth += match c {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            depth >= 0
        }) && depth == 0
    };
    while s.starts_with('(') && s.ends_with(')') && balanced(&s[1..s.len() - 1]) {
        s = s[1..s.len() - 1].to_string();
    }
    s
}

/// A number for an expression: the shortest decimal that is still exact enough.
fn num(v: f64) -> String {
    format_number(v, 15.0)
}

fn shown(v: f64) -> String {
    with_minus(&format_number(if v.abs() < 1e-12 { 0.0 } else { v }, 6.0))
}

/// `y = 1.5x + 0.5` (or `y = 3`, `y = −x`).
fn slope_intercept(m: f64, b: f64) -> String {
    let mut s = String::from("y = ");
    let mut any = false;
    if m != 0.0 {
        any = true;
        if m == -1.0 {
            s.push('−');
        } else if m != 1.0 {
            s.push_str(&shown(m));
        }
        s.push('x');
    }
    if b != 0.0 || !any {
        if any {
            s.push_str(if b < 0.0 { " − " } else { " + " });
            s.push_str(&shown(b.abs()));
        } else {
            s.push_str(&shown(b));
        }
    }
    s
}

/// Lines through points: `line through (1, 2) and (3, 5)`, `(1, 2) slope 3`, `interpolate (0,0), (1,3), (2,1)`.
fn point_intent(body: &str, options: &GraphOptions) -> Option<NativeIntent> {
    static POINT: Lazy = Lazy::new(|| js(r"\(\s*([^(),]+?)\s*,\s*([^(),]+?)\s*\)", ""));
    static SLOPE: Lazy = Lazy::new(|| {
        js(
            r"(?:\bslope\s*(?:of|=|:|is)?|\bm\s*=)\s*([-+−]?[0-9.]+(?:\s*/\s*[0-9.]+)?|[-+−]?\([^()]*\))",
            "i",
        )
    });
    static FILLER: Lazy = Lazy::new(|| {
        js(
            r"\b(?:line|lines|through|thru|and|to|with|from|between|points?|interpolate|interpolation|linear|lerp|passing|the|a|of)\b|[,;&]",
            "i",
        )
    });
    if !body.contains('(') {
        return None;
    }
    let mut points = Vec::new();
    let mut rest = String::new();
    let mut last = 0;
    for m in POINT.captures_iter(body) {
        let m = m.ok()?;
        let whole = m.get(0)?;
        let m = &m;
        rest.push_str(&body[last..whole.start()]);
        rest.push(' ');
        last = whole.end();
        let x = evaluate_graph_y(jsre::group(m, 1), 0.0, options)?;
        let y = evaluate_graph_y(jsre::group(m, 2), 0.0, options)?;
        points.push((x, y));
    }
    rest.push_str(&body[last..]);
    if points.is_empty() {
        return None;
    }
    let mut slope = None;
    let found = SLOPE.captures(rest.as_str()).ok().flatten().map(|m| {
        (
            jsre::group(&m, 0).to_string(),
            jsre::group(&m, 1).replace('−', "-"),
        )
    });
    if let Some((whole, value)) = found {
        slope = Some(evaluate_graph_y(&value, 0.0, options)?);
        rest = rest.replacen(&whole, " ", 1);
    }
    // nothing but points, a slope and joining words
    if !jsre::replace_all(&FILLER, &rest, |_| String::new())
        .trim()
        .is_empty()
    {
        return None;
    }
    let pt = |(x, y): (f64, f64)| format!("({}, {})", shown(x), shown(y));
    let line = |m: f64, (x1, y1): (f64, f64)| {
        GraphIntent::new(
            format!("({})*(x - ({})) + ({})", num(m), num(x1), num(y1)),
            "",
        )
    };
    let mut out = NativeIntent {
        intent: GraphIntent::new("", ""),
        points: points.clone(),
        error: None,
    };
    match (points.len(), slope) {
        (1, Some(m)) => {
            let p = points[0];
            out.intent = GraphIntent {
                label: format!(
                    "{} through {} with slope {}",
                    slope_intercept(m, p.1 - m * p.0),
                    pt(p),
                    shown(m)
                ),
                ..line(m, p)
            };
        }
        (2, None) => {
            let (a, b) = (points[0], points[1]);
            if a.0 == b.0 {
                if a.1 == b.1 {
                    return None;
                }
                out.intent = GraphIntent {
                    contour: Some(ContourKind::Zero),
                    ..GraphIntent::new(
                        format!("(x) - ({})", num(a.0)),
                        format!("x = {} through {} and {}", shown(a.0), pt(a), pt(b)),
                    )
                };
            } else {
                let m = (b.1 - a.1) / (b.0 - a.0);
                out.intent = GraphIntent {
                    label: format!(
                        "{} through {} and {}",
                        slope_intercept(m, a.1 - m * a.0),
                        pt(a),
                        pt(b)
                    ),
                    ..line(m, a)
                };
            }
        }
        (n, None) if n >= 3 => {
            let mut ps = points.clone();
            ps.sort_by(|p, q| p.0.total_cmp(&q.0));
            if ps.windows(2).any(|w| w[0].0 == w[1].0) {
                out.error = Some("Two points share an x, so no line joins them in order".into());
                out.intent = GraphIntent::new("x", "linear interpolation");
                return Some(out);
            }
            // y₀ + m₀(x − x₀) + Σ (mₖ − mₖ₋₁)·max(0, x − xₖ): straight between the points, bent at each
            let slopes: Vec<f64> = ps
                .windows(2)
                .map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
                .collect();
            let mut e = format!(
                "({}) + ({})*(x - ({}))",
                num(ps[0].1),
                num(slopes[0]),
                num(ps[0].0)
            );
            for k in 1..slopes.len() {
                let d = slopes[k] - slopes[k - 1];
                if d != 0.0 {
                    e.push_str(&format!(
                        " + ({})*(abs(x - ({x})) + (x - ({x})))/2",
                        num(d),
                        x = num(ps[k].0)
                    ));
                }
            }
            out.intent = GraphIntent {
                bounds: Some(GraphBounds {
                    x: vec![GraphRange {
                        lo: ps[0].0,
                        hi: ps[ps.len() - 1].0,
                        lo_open: false,
                        hi_open: false,
                    }],
                    y: Vec::new(),
                }),
                ..GraphIntent::new(e, format!("linear interpolation through {n} points"))
            };
        }
        _ => return None,
    }
    Some(out)
}

// ---- equations solved for y or z ----

/// `expression` (in x, y and `var`) as `var = …` when it is linear in `var` (`a + b·var`, b not always 0):
/// checked at a spread of points. `at(e, k)` evaluates `e` at the k-th of `samples` points.
fn solve_linear(
    expression: &str,
    var: &str,
    samples: usize,
    at: &dyn Fn(&str, usize) -> Option<f64>,
) -> Option<String> {
    let put = |t: &str| rewrite_params(expression, &[(var, t)]);
    let g0 = put("(0)");
    if g0 == expression {
        return None;
    }
    let g1 = put("(1)");
    let tests: Vec<(f64, String)> = [-3.7, 2.0, 5.3]
        .iter()
        .map(|t| (*t, put(&format!("({t})"))))
        .collect();
    let mut defined = 0;
    let mut moves = false;
    for k in 0..samples {
        let (Some(a), Some(one)) = (at(&g0, k), at(&g1, k)) else {
            continue;
        };
        let b = one - a;
        for (t, e) in &tests {
            let v = at(e, k)?;
            let tol = 1e-9 * a.abs().max((t * b).abs()).max(v.abs()).max(1e-12);
            if (v - (a + t * b)).abs() > tol {
                return None;
            }
        }
        defined += 1;
        moves |= b != 0.0;
    }
    (defined >= 3 && moves).then(|| format!("({g0})/(({g0}) - ({g1}))"))
}

const XS: [f64; 9] = [-7.3, -4.1, -2.2, -0.37, 0.61, 1.9, 3.3, 5.7, 8.8];
const YS: [f64; 9] = [2.9, -6.1, 0.47, 7.7, -1.3, 4.6, -3.9, -0.8, 5.2];

/// What the native panel graphs for `input`: [`parse_graph_intent`] after [`normalize`], with lines through
/// points and equations linear in y or z read as curves and surfaces.
pub fn native_intent(input: &str, options: &GraphOptions) -> Option<NativeIntent> {
    // a z after a coefficient (`20z`) is free too, so only a letter or _ before it makes it part of a name
    static FREE_Z: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_])z(?![A-Za-z0-9_])", ""));
    let input = normalize(input)?;
    let body = input.trim_start()[5..].trim();
    if let Some(p) = point_intent(body, options) {
        return Some(p);
    }
    let mut intent = parse_graph_intent(&input, &options.functions, &options.variables)?;
    intent.label = tidy_label(&intent.label);
    let mut error = None;
    if intent.contour == Some(ContourKind::Zero) && intent.bounds_error.is_none() {
        let mode = options.angle_mode.or(Some(AngleMode::Rad));
        if jsre::test(&FREE_Z, &intent.expression) && !options.variables.contains_key("z") {
            let at = |e: &str, k: usize| compile_graph_z(e, options, mode).at(XS[k], YS[k]);
            match solve_linear(&intent.expression, "z", XS.len(), &at) {
                Some(z) => {
                    intent.expression = z;
                    intent.contour = Some(ContourKind::Levels);
                }
                None => error = Some("Write a surface as z = … (one z, solved for)".into()),
            }
        } else {
            let at = |e: &str, k: usize| compile_graph_y(e, options, mode).at(XS[k]);
            if let Some(y) = solve_linear(&intent.expression, "y", XS.len(), &at) {
                // a line (point-slope `y − 3 = 2(x − 1)`, `2y + 3x = 6`) also shows as y = mx + b
                let curve = compile_graph_y(&y, options, mode);
                if let (Some(b), Some(one)) = (curve.at(0.0), curve.at(1.0)) {
                    let m = one - b;
                    let straight = XS.iter().all(|&x| {
                        curve.at(x).is_some_and(|v| {
                            (v - (b + m * x)).abs()
                                <= 1e-9 * v.abs().max((m * x).abs()).max(b.abs()).max(1e-12)
                        })
                    });
                    if straight {
                        let tidy = |v: f64| tidy(v, &|c| (c - v).abs() <= 1e-12 * v.abs().max(1.0));
                        intent.label =
                            format!("{} → {}", intent.label, slope_intercept(tidy(m), tidy(b)));
                    }
                }
                intent.expression = y;
                intent.contour = None;
            }
        }
    }
    Some(NativeIntent {
        intent,
        points: Vec::new(),
        error,
    })
}

// ---- implicit curves ----

fn grad(f: &dyn Fn(f64, f64) -> Option<f64>, x: f64, y: f64, h: f64) -> Option<(f64, f64)> {
    Some((
        (f(x + h, y)? - f(x - h, y)?) / (2.0 * h),
        (f(x, y + h)? - f(x, y - h)?) / (2.0 * h),
    ))
}

/// Newton's method for F(x, y) = (0, 0), the Jacobian by differences; None unless it settles inside `w`.
fn newton2(
    fs: &dyn Fn(f64, f64) -> Option<(f64, f64)>,
    start: (f64, f64),
    w: Window,
    step_cap: f64,
) -> Option<(f64, f64)> {
    let span = w.span();
    let h = span * 1e-5;
    let (mut x, mut y) = start;
    for _ in 0..40 {
        let (f1, f2) = fs(x, y)?;
        let (ax, bx) = fs(x + h, y)?;
        let (ay, by) = fs(x, y + h)?;
        let (j11, j21) = ((ax - f1) / h, (bx - f2) / h);
        let (j12, j22) = ((ay - f1) / h, (by - f2) / h);
        let det = j11 * j22 - j12 * j21;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let mut dx = (f1 * j22 - f2 * j12) / det;
        let mut dy = (f2 * j11 - f1 * j21) / det;
        let len = len2(dx, dy);
        if len > step_cap {
            dx *= step_cap / len;
            dy *= step_cap / len;
        }
        x -= dx;
        y -= dy;
        if !w.contains(x, y) {
            return None;
        }
        if len < span * 1e-13 {
            return Some((x, y));
        }
    }
    // converged as far as the differences allow
    let (f1, f2) = fs(x, y)?;
    (f1.is_finite() && f2.is_finite()).then_some((x, y))
}

fn tidy_near(v: f64, span: f64) -> f64 {
    let v = if v.abs() < span * 1e-10 { 0.0 } else { v };
    tidy(v, &|c| (c - v).abs() <= span * 1e-9)
}

fn push_unique(out: &mut Vec<CriticalPoint>, p: CriticalPoint, tol: f64) {
    if out.len() < MAX_FEATURES && !out.iter().any(|q| len2(q.x - p.x, q.y - p.y) <= tol) {
        out.push(p);
    }
}

/// x where `g` crosses 0 along [lo, hi], by sign changes on a fine grid and bisection.
fn crossings(g: &dyn Fn(f64) -> Option<f64>, lo: f64, hi: f64) -> Vec<f64> {
    const N: usize = 800;
    let mut out = Vec::new();
    let step = (hi - lo) / N as f64;
    let mut prev: Option<(f64, f64)> = None;
    let values: Vec<Option<f64>> = (0..=N).map(|i| g(lo + i as f64 * step)).collect();
    let zero_at = |i: usize| values.get(i).copied().flatten() == Some(0.0);
    for (i, &v) in values.iter().enumerate() {
        let t = lo + i as f64 * step;
        if let Some(v) = v {
            if v == 0.0 {
                // a curve lying along the axis (x² = x has the line x = 0) is zero all the way: it doesn't cross
                if !(i > 0 && zero_at(i - 1)) && !zero_at(i + 1) {
                    out.push(t);
                }
            } else if let Some((pt, pv)) = prev
                && pv != 0.0
                && pv.signum() != v.signum()
            {
                let (mut a, mut b, mut fa) = (pt, t, pv);
                for _ in 0..80 {
                    let m = (a + b) / 2.0;
                    let Some(fm) = g(m) else { break };
                    if fm == 0.0 || m == a || m == b {
                        a = m;
                        b = m;
                        break;
                    }
                    if fm.signum() == fa.signum() {
                        a = m;
                        fa = fm;
                    } else {
                        b = m;
                    }
                }
                // a sign change across a pole isn't a crossing
                let m = (a + b) / 2.0;
                if g(m).is_some_and(|v| v.abs() <= pv.abs().max(fa.abs()) + 1e-300) {
                    out.push(m);
                }
            }
        }
        prev = v.map(|v| (t, v));
    }
    out
}

/// An implicit curve's features inside the window: where it tops out and bottoms out (y's max and min along it),
/// where it turns vertical, and where it meets the axes. Found from the curve's own segments, refined by
/// Newton's method on (g, ∂g/∂x) and (g, ∂g/∂y).
pub fn implicit_features(
    f: &dyn Fn(f64, f64) -> Option<f64>,
    contours: &[ContourLevel],
    w: Window,
) -> Vec<CriticalPoint> {
    let span = w.span();
    let h = span * 1e-6;
    let tol = span * 1e-5;
    let cell = (w.x.1 - w.x.0) / 60.0;
    let mut turns: Vec<CriticalPoint> = Vec::new();
    let gx = |x: f64, y: f64| grad(f, x, y, h).map(|g| g.0);
    let gy = |x: f64, y: f64| grad(f, x, y, h).map(|g| g.1);
    for (which, d) in [(0, &gx as &dyn Fn(f64, f64) -> Option<f64>), (1, &gy)] {
        let system = |x: f64, y: f64| Some((f(x, y)?, d(x, y)?));
        for c in contours {
            for s in c.segments.as_chunks::<4>().0 {
                if turns.len() >= MAX_FEATURES {
                    break;
                }
                let (Some(a), Some(b)) = (d(s[0], s[1]), d(s[2], s[3])) else {
                    continue;
                };
                // a grid line can put an end exactly on the turn, so 0 counts as either side
                if (a == 0.0 && b == 0.0) || (a < 0.0 && b < 0.0) || (a > 0.0 && b > 0.0) {
                    continue;
                }
                let start = ((s[0] + s[2]) / 2.0, (s[1] + s[3]) / 2.0);
                let Some((x, y)) = newton2(&system, start, w, cell) else {
                    continue;
                };
                let Some((fx, fy)) = grad(f, x, y, h) else {
                    continue;
                };
                let norm = len2(fx, fy);
                // on the curve, and not a crossing of two branches (∇g = 0)
                let Some(g) = f(x, y) else { continue };
                if !(norm > 0.0) || g.abs() / norm > span * 1e-6 {
                    continue;
                }
                let kind = if which == 0 {
                    if fy.abs() < norm * 1e-3 {
                        continue;
                    }
                    // y'' = −g_xx / g_y where the tangent is flat
                    let k = 1e-3 * span;
                    let (Some(l), Some(r)) = (f(x - k, y), f(x + k, y)) else {
                        continue;
                    };
                    let gxx = (l - 2.0 * g + r) / (k * k);
                    let curv = -gxx / fy;
                    if curv.abs() * span < 1e-6 {
                        CriticalKind::Critical
                    } else if curv < 0.0 {
                        CriticalKind::Max
                    } else {
                        CriticalKind::Min
                    }
                } else {
                    if fx.abs() < norm * 1e-3 {
                        continue;
                    }
                    CriticalKind::VerticalTangent
                };
                let (x, y) = (tidy_near(x, span), tidy_near(y, span));
                push_unique(
                    &mut turns,
                    CriticalPoint {
                        x,
                        y,
                        kind,
                        z: None,
                    },
                    tol,
                );
            }
        }
    }
    turns.sort_by(|p, q| p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y)));
    let mut out = turns;
    if w.y.0 <= 0.0 && w.y.1 >= 0.0 {
        for x in crossings(&|x| f(x, 0.0), w.x.0, w.x.1) {
            let p = CriticalPoint {
                x: tidy_near(x, span),
                y: 0.0,
                kind: CriticalKind::XIntercept,
                z: None,
            };
            push_unique(&mut out, p, tol);
        }
    }
    if w.x.0 <= 0.0 && w.x.1 >= 0.0 {
        for y in crossings(&|y| f(0.0, y), w.y.0, w.y.1) {
            let p = CriticalPoint {
                x: 0.0,
                y: tidy_near(y, span),
                kind: CriticalKind::YIntercept,
                z: None,
            };
            push_unique(&mut out, p, tol);
        }
    }
    out
}

/// The window an implicit curve opens on, x and y at one scale: the closest zoom (in quarter decades from 0.01
/// to 10⁷) that holds the whole curve, or for a curve that runs off every window, the one where it spreads
/// widest both ways; then centred on what's drawn. A curve too small to see from the origin is looked for
/// first. (lo, hi, y centre).
pub fn implicit_home(f: &dyn Fn(f64, f64) -> Option<f64>, aspect: f64) -> Option<(f64, f64, f64)> {
    scan_home(f, aspect, (0.0, 0.0)).or_else(|| scan_home(f, aspect, find_point(f)?))
}

/// A point on g = 0 near the origin: from the grid point where |g| is least (at a few zooms), stepped onto the
/// curve along ∇g.
fn find_point(f: &dyn Fn(f64, f64) -> Option<f64>) -> Option<(f64, f64)> {
    const N: usize = 64;
    for s in [10.0, 100.0, 1000.0, 1e4, 1e5] {
        let d = 2.0 * s / N as f64;
        let mut best: Option<(f64, f64, f64)> = None;
        for j in 0..=N {
            for i in 0..=N {
                let (x, y) = (-s + i as f64 * d, -s + j as f64 * d);
                if let Some(g) = f(x, y)
                    && best.is_none_or(|b| g.abs() < b.2)
                {
                    best = Some((x, y, g.abs()));
                }
            }
        }
        let Some((mut x, mut y, _)) = best else {
            continue;
        };
        let h = s * 1e-7;
        for _ in 0..60 {
            let (Some(g), Some((gx, gy))) = (f(x, y), grad(f, x, y, h)) else {
                break;
            };
            let n2 = gx * gx + gy * gy;
            if !(n2 > 0.0) {
                break;
            }
            let (dx, dy) = (g * gx / n2, g * gy / n2);
            let len = len2(dx, dy).min(d);
            let scale = len / len2(dx, dy).max(1e-300);
            x -= dx * scale;
            y -= dy * scale;
            if len2(dx, dy) < s * 1e-12 {
                return Some((x, y));
            }
        }
        if let (Some(g), Some((gx, gy))) = (f(x, y), grad(f, x, y, h))
            && g.abs() <= 1e-6 * len2(gx, gy) * s
        {
            return Some((x, y));
        }
    }
    None
}

fn scan_home(
    f: &dyn Fn(f64, f64) -> Option<f64>,
    aspect: f64,
    (px, py): (f64, f64),
) -> Option<(f64, f64, f64)> {
    const NX: usize = 64;
    let ny = ((NX as f64 * aspect).round() as usize).max(8);
    let mut best: Option<Zoom> = None;
    let near = |k: i32| (k - 4).abs();
    let mut grid = vec![0.0; (NX + 1) * (ny + 1)];
    for k in -8..=28 {
        let s = jsmath::pow(10.0, k as f64 / 4.0);
        let (x0, y0) = (px - s, py - s * aspect);
        let (x1, y1) = (px + s, py + s * aspect);
        let (dx, dy) = (2.0 * s / NX as f64, 2.0 * s * aspect / ny as f64);
        for j in 0..=ny {
            for i in 0..=NX {
                grid[j * (NX + 1) + i] =
                    f(x0 + i as f64 * dx, y0 + j as f64 * dy).unwrap_or(f64::NAN);
            }
        }
        let segs = marching_squares(&grid, NX, ny, 0.0, x0, y0, dx, dy);
        if segs.is_empty() {
            continue;
        }
        let (mut bx0, mut bx1, mut by0, mut by1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for p in segs.as_chunks::<2>().0 {
            bx0 = bx0.min(p[0]);
            bx1 = bx1.max(p[0]);
            by0 = by0.min(p[1]);
            by1 = by1.max(p[1]);
        }
        let (w, h) = (2.0 * s, 2.0 * s * aspect);
        let held = bx0 > x0 + dx && bx1 < x1 - dx && by0 > y0 + dy && by1 < y1 - dy;
        let score = if held {
            1.0 + ((bx1 - bx0) / w).max((by1 - by0) / h)
        } else {
            ((bx1 - bx0) / w).min((by1 - by0) / h)
        };
        // ties go to the zoom nearest ±10
        let better = match best {
            None => true,
            Some((b, bk, _, _)) => {
                score > b + 0.02 || ((score - b).abs() <= 0.02 && near(k) < near(bk))
            }
        };
        if better {
            best = Some((score, k, (bx0, bx1, by0, by1), held));
        }
    }
    let (_, k, (bx0, bx1, by0, by1), held) = best?;
    let s = jsmath::pow(10.0, k as f64 / 4.0);
    let (cx, cy) = ((bx0 + bx1) / 2.0, (by0 + by1) / 2.0);
    let span = if held {
        ((bx1 - bx0).max((by1 - by0) / aspect) * 1.25).max(1e-6)
    } else {
        2.0 * s
    };
    let (lo, hi) = nice_window(cx, span);
    // the y centre on a round value of the scale, or 0 when that's near enough
    let step = jsmath::pow(10.0, jsmath::log10(span * aspect / 10.0).floor());
    let cy = if cy.abs() > span * aspect * 0.05 {
        (cy / step).round() * step
    } else {
        0.0
    };
    Some((lo, hi, cy))
}

/// `graph_home` for the native panel: (lo, hi, the y centre for a plot of x and y, when not 0).
pub fn native_home(input: &str, options: &GraphOptions) -> (f64, f64, Option<f64>) {
    let Some(n) = native_intent(input, options) else {
        return (-10.0, 10.0, None);
    };
    let intent = n.intent;
    if intent.contour == Some(ContourKind::Zero)
        && intent.bounds.is_none()
        && intent.bounds_error.is_none()
    {
        let g = compile_graph_z(
            &intent.expression,
            options,
            options.angle_mode.or(Some(AngleMode::Rad)),
        );
        if let Some((lo, hi, cy)) = implicit_home(&|x, y| g.at(x, y), options.aspect()) {
            return (lo, hi, (cy != 0.0).then_some(cy));
        }
    }
    let (lo, hi) = home_of(Some(intent.clone()), options, &|o| {
        Some(build_intent(intent.clone(), o, true))
    });
    let (lo, hi) = widen_to_features(&intent, options, (lo, hi));
    (lo, hi, None)
}

/// Native only: the most useful first view of a curve is one that shows what there is to see on it: every root,
/// every turn and where it crosses the y axis. The page's home stays within ±10 (`2x + 34` opened on [−10, 10],
/// with its root at −17 off the side), so look further out: windows 10 and 100 times as wide, stopping once
/// one finds no more than the last (all found) or too many to frame (sin x, whose features never end; it keeps
/// the page's view). Features within the page's window leave it as it is; any beyond widen it to hold them all,
/// with room around them.
fn widen_to_features(
    intent: &GraphIntent,
    options: &GraphOptions,
    (lo, hi): (f64, f64),
) -> (f64, f64) {
    const MANY: usize = 12;
    if intent.contour.is_some()
        || intent.bounds.is_some()
        || intent.expression.is_empty()
        || !(hi > lo)
    {
        return (lo, hi);
    }
    let reach = lo.abs().max(hi.abs());
    let mut found: Option<Vec<f64>> = None;
    for k in 1..=2 {
        let h = reach * 10f64.powi(k);
        let o = GraphOptions {
            domain: Some((-h, h)),
            sample_count: Some(4001.0),
            ..options.clone()
        };
        let g = build_intent(intent.clone(), &o, true);
        if g.error.is_some() {
            break;
        }
        // a root the curve really crosses or touches: e^x's values far left are tiny, not zero
        let d = h * 1e-7;
        let real_root = |r: &f64| {
            let (l, m, n) = (g.y_at(r - d), g.y_at(*r), g.y_at(r + d));
            match (l, m, n) {
                (Some(l), _, Some(n)) if l.signum() != n.signum() && l != 0.0 && n != 0.0 => true,
                // touching (x²), or where the curve starts or stops at 0 (√(x − 40))
                (l, Some(m), n) => {
                    m == 0.0 && l.is_some_and(|v| v != 0.0)
                        || m == 0.0 && n.is_some_and(|v| v != 0.0)
                }
                _ => false,
            }
        };
        let mut xs: Vec<f64> = g
            .roots
            .iter()
            .copied()
            .filter(real_root)
            .chain(g.critical_points.iter().map(|c| c.x))
            .filter(|x| x.is_finite())
            .collect();
        if xs.len() > MANY {
            break;
        }
        if g.y_at(0.0).is_some_and(f64::is_finite) {
            xs.push(0.0);
        }
        let same = found.as_ref().is_some_and(|f| f.len() == xs.len());
        found = Some(xs);
        if same {
            break;
        }
    }
    let Some(xs) = found.filter(|xs| !xs.is_empty()) else {
        return (lo, hi);
    };
    // already in view, and not on its edge
    let margin = (hi - lo) * 0.05;
    if xs.iter().all(|&x| x >= lo + margin && x <= hi - margin) {
        return (lo, hi);
    }
    let a = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let b = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // a third again around them; one alone gets the page's width around it
    let span = if b > a {
        ((b - a) * 1.35).max((hi - lo) / 4.0)
    } else {
        hi - lo
    };
    nice_window((a + b) / 2.0, span)
}

// ---- surfaces ----

/// A surface's critical points in the window (∇f = 0): grid points where |∇f| dips, refined by Newton's method
/// and named by the Hessian (min, max, saddle; `critical` when it can't tell). The nearest the middle first, at
/// most [`MAX_SURFACE_POINTS`], then in order of x.
pub fn surface_features(f: &dyn Fn(f64, f64) -> Option<f64>, w: Window) -> Vec<CriticalPoint> {
    const N: usize = 40;
    let (dx, dy) = ((w.x.1 - w.x.0) / N as f64, (w.y.1 - w.y.0) / N as f64);
    let mut z = vec![f64::NAN; (N + 1) * (N + 1)];
    for j in 0..=N {
        for i in 0..=N {
            z[j * (N + 1) + i] =
                f(w.x.0 + i as f64 * dx, w.y.0 + j as f64 * dy).unwrap_or(f64::NAN);
        }
    }
    let finite: Vec<f64> = z.iter().copied().filter(|v| v.is_finite()).collect();
    let (Some(lo), Some(hi)) = (
        finite.iter().copied().reduce(f64::min),
        finite.iter().copied().reduce(f64::max),
    ) else {
        return Vec::new();
    };
    let range = hi - lo;
    if !(range > 0.0) {
        return Vec::new();
    }
    let at = |i: usize, j: usize| z[j * (N + 1) + i];
    // |∇f|² by central differences, at the interior points
    let g2 = |i: usize, j: usize| {
        let gx = (at(i + 1, j) - at(i - 1, j)) / (2.0 * dx);
        let gy = (at(i, j + 1) - at(i, j - 1)) / (2.0 * dy);
        gx * gx + gy * gy
    };
    let span = w.span();
    let h = span * 1e-5;
    let mut found: Vec<CriticalPoint> = Vec::new();
    for j in 2..N - 1 {
        for i in 2..N - 1 {
            let c = g2(i, j);
            if !c.is_finite() {
                continue;
            }
            let mut lowest = true;
            for (di, dj) in [
                (-1i32, -1i32),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let v = g2((i as i32 + di) as usize, (j as i32 + dj) as usize);
                if !(c < v) {
                    lowest = false;
                    break;
                }
            }
            if !lowest {
                continue;
            }
            let start = (w.x.0 + i as f64 * dx, w.y.0 + j as f64 * dy);
            let gradient = |x: f64, y: f64| grad(f, x, y, h);
            let Some((x, y)) = newton2(&gradient, start, w, 2.0 * dx.max(dy)) else {
                continue;
            };
            let Some((gx, gy)) = gradient(x, y) else {
                continue;
            };
            if len2(gx, gy) * span > range * 1e-5 {
                continue;
            }
            let k = span * 1e-3;
            let (
                Some(c0),
                Some(xp),
                Some(xm),
                Some(yp),
                Some(ym),
                Some(pp),
                Some(pm),
                Some(mp),
                Some(mm),
            ) = (
                f(x, y),
                f(x + k, y),
                f(x - k, y),
                f(x, y + k),
                f(x, y - k),
                f(x + k, y + k),
                f(x + k, y - k),
                f(x - k, y + k),
                f(x - k, y - k),
            )
            else {
                continue;
            };
            let fxx = (xp - 2.0 * c0 + xm) / (k * k);
            let fyy = (yp - 2.0 * c0 + ym) / (k * k);
            let fxy = (pp - pm - mp + mm) / (4.0 * k * k);
            let det = fxx * fyy - fxy * fxy;
            let scale = fxx * fxx + fyy * fyy + fxy * fxy;
            let kind = if det.abs() <= 1e-6 * scale || scale == 0.0 {
                CriticalKind::Critical
            } else if det < 0.0 {
                CriticalKind::Saddle
            } else if fxx > 0.0 {
                CriticalKind::Min
            } else {
                CriticalKind::Max
            };
            let (x, y) = (tidy_near(x, span), tidy_near(y, span));
            let Some(zv) = f(x, y) else { continue };
            let p = CriticalPoint {
                x,
                y,
                kind,
                z: Some(if zv.abs() < range * 1e-12 { 0.0 } else { zv }),
            };
            if !found
                .iter()
                .any(|q| len2(q.x - p.x, q.y - p.y) <= span * 1e-5)
            {
                found.push(p);
            }
        }
    }
    let mid = ((w.x.0 + w.x.1) / 2.0, (w.y.0 + w.y.1) / 2.0);
    found.sort_by(|p, q| len2(p.x - mid.0, p.y - mid.1).total_cmp(&len2(q.x - mid.0, q.y - mid.1)));
    found.truncate(MAX_SURFACE_POINTS);
    found.sort_by(|p, q| p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y)));
    found
}

/// z over the window for the 3D view, with the heights it frames (2% clipped off each end, so a pole doesn't
/// flatten the rest).
pub fn surface_grid(f: &dyn Fn(f64, f64) -> Option<f64>, w: Window) -> SurfaceGrid {
    let n = SURFACE_CELLS;
    let (dx, dy) = ((w.x.1 - w.x.0) / n as f64, (w.y.1 - w.y.0) / n as f64);
    let mut z = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            z.push(f(w.x.0 + i as f64 * dx, w.y.0 + j as f64 * dy).unwrap_or(f64::NAN));
        }
    }
    let mut seen: Vec<f64> = z.iter().copied().filter(|v| v.is_finite()).collect();
    let (mut z_lo, mut z_hi) = (-1.0, 1.0);
    if !seen.is_empty() {
        sort_numbers(&mut seen);
        let (lo, hi) = (seen[0], seen[seen.len() - 1]);
        let (qlo, qhi) = (quantile(&seen, 0.02), quantile(&seen, 0.98));
        // clip only when the ends are far out
        let (a, b) = if (hi - lo) > 4.0 * (qhi - qlo) && qhi > qlo {
            (qlo, qhi)
        } else {
            (lo, hi)
        };
        if b > a {
            (z_lo, z_hi) = (a, b);
        } else {
            (z_lo, z_hi) = (a - 1.0, a + 1.0);
        }
    }
    SurfaceGrid {
        nx: n,
        ny: n,
        x_lo: w.x.0,
        x_hi: w.x.1,
        y_lo: w.y.0,
        y_hi: w.y.1,
        z,
        z_lo,
        z_hi,
    }
}

/// Native only: where the graph's curves cross one another over [lo, hi], as Desmos marks them: each pair's
/// difference sampled finely, every change of sign closed in on by bisection with the curves' own formulas (so
/// `x^2` and `12x` meet at exactly (12, 144)). A jump across an asymptote (`tan x` against `0`) isn't a crossing:
/// the two must agree where it ends. Curves that lie on one another (`2x` and `x + x`) have no single point, so
/// none is given. Implicit curves and surfaces don't take part.
pub fn native_intersections(
    inputs: &[String],
    options: &GraphOptions,
    lo: f64,
    hi: f64,
) -> Vec<CriticalPoint> {
    const SAMPLES: usize = 4001;
    const MANY: usize = 60;
    if !(hi > lo) || !lo.is_finite() || !hi.is_finite() || inputs.len() < 2 {
        return Vec::new();
    }
    let mut cheap = options.clone();
    cheap.domain = Some((lo, hi));
    cheap.sample_count = Some(16.0);
    cheap.find_roots = Some(false);
    let curves: Vec<_> = inputs
        .iter()
        .filter_map(|i| build_native_graph(i, &cheap))
        .filter(|r| r.error.is_none())
        .filter(|r| r.contours.is_none() && r.intent.contour.is_none() && r.surface.is_none())
        .collect();
    let fns: Vec<_> = curves.iter().filter_map(|r| r.y_fn()).collect();
    let step = (hi - lo) / (SAMPLES - 1) as f64;
    let x_at = |i: usize| lo + step * i as f64;
    // each curve sampled once, not once for every curve it's paired with; NaN where it has no finite value
    let ys: Vec<Vec<f64>> = fns
        .iter()
        .map(|f| {
            (0..SAMPLES)
                .map(|i| f(x_at(i)).filter(|v| v.is_finite()).unwrap_or(f64::NAN))
                .collect()
        })
        .collect();
    let mut out: Vec<CriticalPoint> = Vec::new();
    let mut found: Vec<f64> = Vec::new();
    for a in 0..fns.len() {
        for b in a + 1..fns.len() {
            let d = |x: f64| -> Option<f64> {
                let (p, q) = (fns[a](x)?, fns[b](x)?);
                (p.is_finite() && q.is_finite()).then_some(p - q)
            };
            let (ya, yb) = (&ys[a], &ys[b]);
            let ds = |i: usize| (!ya[i].is_nan() && !yb[i].is_nan()).then(|| ya[i] - yb[i]);
            // the same curve twice: no point is the crossing
            let zeros = (0..SAMPLES).filter(|&i| ds(i) == Some(0.0)).count();
            if zeros > SAMPLES / 20 {
                continue;
            }
            found.clear();
            for i in 0..SAMPLES - 1 {
                let (Some(u), Some(v)) = (ds(i), ds(i + 1)) else {
                    continue;
                };
                if u == 0.0 {
                    found.push(x_at(i));
                    continue;
                }
                if v == 0.0 || u.signum() == v.signum() {
                    continue;
                }
                let (mut l, mut r, mut dl) = (x_at(i), x_at(i + 1), u);
                for _ in 0..80 {
                    let m = (l + r) / 2.0;
                    // l and r are neighbouring doubles: halving again changes nothing
                    if m == l || m == r {
                        break;
                    }
                    let Some(dm) = d(m) else { break };
                    if dm == 0.0 {
                        l = m;
                        r = m;
                        break;
                    }
                    if dm.signum() == dl.signum() {
                        l = m;
                        dl = dm;
                    } else {
                        r = m;
                    }
                }
                found.push((l + r) / 2.0);
            }
            if ds(SAMPLES - 1) == Some(0.0) {
                found.push(x_at(SAMPLES - 1));
            }
            for &x in &found {
                let x = tidy_x(x, step);
                let (Some(p), Some(q)) = (fns[a](x), fns[b](x)) else {
                    continue;
                };
                // an asymptote's jump: the two are far apart where the sign changed
                let scale = 1.0 + p.abs().max(q.abs());
                if !((p - q).abs() <= 1e-6 * scale) {
                    continue;
                }
                let y = tidy_x((p + q) / 2.0, 1e-9 * scale);
                if !out
                    .iter()
                    .any(|c| (c.x - x).abs() <= step && (c.y - y).abs() <= 1e-6 * scale)
                {
                    out.push(CriticalPoint {
                        x,
                        y,
                        kind: CriticalKind::Intersection,
                        z: None,
                    });
                }
            }
            if out.len() > MANY {
                return out;
            }
        }
    }
    out
}

/// A crossing found by bisection to the last bit, rounded to a nearby short number when one is that close
/// (11.999999999999998 → 12), so it reads as the point it is.
fn tidy_x(x: f64, step: f64) -> f64 {
    for digits in 0..12 {
        let k = 10f64.powi(digits);
        let r = (x * k).round() / k;
        if (r - x).abs() <= (step * 1e-6).max(x.abs() * 1e-13) {
            return if r == 0.0 { 0.0 } else { r };
        }
    }
    x
}
