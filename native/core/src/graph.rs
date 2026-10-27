//! Graphs (`src/engine/graph.ts`): the `graph …` command's intent (curves, `y = …`, `f(x) = …`, implicit
//! `x^2 + y^2 = 25`, surfaces in x and y, `{0 < x < 3}` bounds), compiled sampling with refinement at domain
//! edges, steep stretches and jumps, the auto y-scale, roots, critical points, the home window, axis ticks, and
//! level curves by marching squares.
//!
//! The readouts the TS returns as closures (`y`, `z`) are [`GraphReadout`] here: [`GraphResult::y_at`] and
//! [`GraphResult::z_at`] answer exactly what `result.y(x)` and `result.z(x, y)` do.

// `!(x > y)` is the TS test, true for NaN too
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::cell::RefCell;
use std::collections::BTreeMap;

use serde_json::{Map, Value as Json, json};

use crate::format::format_number;
use crate::jsnum;
use crate::jsre::{self, Lazy, js};
use crate::scientific::compile::{Compiled, compile_scientific, compile_scientific2};
use crate::scientific::functions::jsmath;
use crate::scientific::{ScientificContext, eval_scientific, names_pattern, split_glued_functions};
use crate::types::{AngleMode, UserFunction, ValueKind};

pub const DEFAULT_GRAPH_DOMAIN: (f64, f64) = (-10.0, 10.0);
/// Two turns either side, for x in degrees.
const DEGREE_GRAPH_DOMAIN: (f64, f64) = (-360.0, 360.0);
/// Includes both endpoints.
pub const DEFAULT_GRAPH_SAMPLES: f64 = 401.0;
/// Height over width of the graph panel's plot.
pub const CONTOUR_ASPECT: f64 = 160.0 / 600.0;
// grid columns sampled across the view; rows follow the aspect, so cells are square
const CONTOUR_COLUMNS: usize = 120;
const CONTOUR_LINES: f64 = 6.0;
const MINUS: &str = "−";

/// `GraphOptions`.
#[derive(Clone, Debug, Default)]
pub struct GraphOptions {
    pub domain: Option<(f64, f64)>,
    pub sample_count: Option<f64>,
    pub variables: BTreeMap<String, f64>,
    pub functions: BTreeMap<String, UserFunction>,
    pub angle_mode: Option<AngleMode>,
    pub ans: Option<f64>,
    /// Default true.
    pub find_roots: Option<bool>,
    /// The plot's height over its width, so a contour plot's x and y share a scale.
    pub aspect: Option<f64>,
}

/// The tallest and widest plots a contour is sampled for (height over width), and the most samples a curve
/// takes. The panel never asks for more, but a panel of no width would give an infinite aspect, which made the
/// grid an allocation of terabytes (the TS engine has no limit either).
const ASPECT_RANGE: (f64, f64) = (1.0 / 16.0, 16.0);
const MAX_SAMPLES: f64 = 100_000.0;

impl GraphOptions {
    /// `aspect`, or the default for none (or none that makes sense), kept within [`ASPECT_RANGE`].
    fn aspect(&self) -> f64 {
        match self.aspect {
            Some(a) if a.is_finite() && a > 0.0 => a.clamp(ASPECT_RANGE.0, ASPECT_RANGE.1),
            _ => CONTOUR_ASPECT,
        }
    }

    fn sci(&self, mode: Option<AngleMode>) -> ScientificContext {
        ScientificContext {
            ans: self.ans,
            angle_mode: mode.unwrap_or_default(),
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            depth: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContourKind {
    /// Level curves of f(x, y).
    Levels,
    /// Only where f is 0 (`x^2 + y^2 = 25`).
    Zero,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphRange {
    pub lo: f64,
    pub hi: f64,
}

/// The pieces of the plot to keep: a union of ranges per axis, intersected across x and y.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphBounds {
    pub x: Vec<GraphRange>,
    pub y: Vec<GraphRange>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphIntent {
    /// In the free variable `x` (and `y` for a contour).
    pub expression: String,
    pub label: String,
    pub function_def: Option<FunctionDef>,
    pub bounds: Option<GraphBounds>,
    pub bounds_error: Option<String>,
    pub contour: Option<ContourKind>,
}

impl GraphIntent {
    fn new(expression: impl Into<String>, label: impl Into<String>) -> Self {
        GraphIntent {
            expression: expression.into(),
            label: label.into(),
            function_def: None,
            bounds: None,
            bounds_error: None,
            contour: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphPoint {
    pub x: f64,
    /// None where the curve is undefined or breaks.
    pub y: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalKind {
    Min,
    Max,
    Critical,
}

impl CriticalKind {
    pub fn name(self) -> &'static str {
        match self {
            CriticalKind::Min => "min",
            CriticalKind::Max => "max",
            CriticalKind::Critical => "critical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CriticalPoint {
    pub x: f64,
    pub y: f64,
    pub kind: CriticalKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YScale {
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelAt {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContourLevel {
    pub level: f64,
    pub label: String,
    /// x1, y1, x2, y2 for each piece of the line.
    pub segments: Vec<f64>,
    /// Where its label sits, clear of the edges and the other labels.
    pub at: Option<LabelAt>,
}

/// What `result.y` / `result.z` evaluate: the compiled curve (or surface) and the bounds it is cut to.
#[derive(Clone, Debug)]
pub enum GraphReadout {
    None,
    Curve {
        expression: String,
        options: GraphOptions,
        mode: AngleMode,
        bounds: Option<GraphBounds>,
    },
    Surface {
        expression: String,
        options: GraphOptions,
        mode: AngleMode,
        bounds: Option<GraphBounds>,
    },
}

#[derive(Clone, Debug)]
pub struct GraphResult {
    pub intent: GraphIntent,
    pub points: Vec<GraphPoint>,
    pub domain: (f64, f64),
    pub y_scale: YScale,
    pub critical_points: Vec<CriticalPoint>,
    pub roots: Vec<f64>,
    /// None when the curve doesn't depend on the angle mode.
    pub angle_unit: Option<AngleMode>,
    /// Present for a function of x and y; `y_scale` is then its y range, at the plot's aspect.
    pub contours: Option<Vec<ContourLevel>>,
    pub error: Option<String>,
    pub readout: GraphReadout,
}

impl GraphResult {
    /// `result.y(x)`: the curve at any x, inside the bounds; None for a contour plot.
    pub fn y_at(&self, x: f64) -> Option<f64> {
        match &self.readout {
            GraphReadout::Curve {
                expression,
                options,
                mode,
                bounds,
            } => {
                let curve = compile_graph_y(expression, options, Some(*mode));
                restrict(&curve, bounds.as_ref(), x)
            }
            _ => None,
        }
    }

    /// `result.z(x, y)`: a contour plot's f(x, y) inside the bounds; None for a curve.
    pub fn z_at(&self, x: f64, y: f64) -> Option<f64> {
        match &self.readout {
            GraphReadout::Surface {
                expression,
                options,
                mode,
                bounds,
            } => {
                if !inside(bounds.as_ref(), x, y) {
                    return None;
                }
                compile_graph_z(expression, options, Some(*mode)).at(x, y)
            }
            _ => None,
        }
    }

    /// Whether `result.z` exists (the TS sets it on every contour result).
    pub fn has_z(&self) -> bool {
        matches!(self.readout, GraphReadout::Surface { .. })
    }
}

// ---- JS helpers ----

fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn min_of(xs: impl IntoIterator<Item = f64>) -> f64 {
    xs.into_iter().fold(f64::INFINITY, jmin)
}

fn max_of(xs: impl IntoIterator<Item = f64>) -> f64 {
    xs.into_iter().fold(f64::NEG_INFINITY, jmax)
}

fn sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `Number(x.toPrecision(d))`.
fn precision(x: f64, d: usize) -> f64 {
    jsnum::to_precision(x, d).parse().unwrap_or(f64::NAN)
}

/// V8's `Math.hypot` for two arguments: scaled by the larger, with Kahan compensation.
fn hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a.is_infinite() || b.is_infinite() {
        return f64::INFINITY;
    }
    let max = jmax(a, b);
    if max.is_nan() {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

fn is_ws(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn jtrim(s: &str) -> &str {
    s.trim_matches(is_ws)
}

/// `with_minus`: display only; copied text keeps `-`.
pub fn with_minus(s: &str) -> String {
    match s.strip_prefix('-') {
        Some(rest) => format!("{MINUS}{rest}"),
        None => s.to_string(),
    }
}

// ---- intent ----

/// An insertion-ordered string map with JS object semantics (a repeated key keeps its first place).
fn map_set(map: &mut Vec<(String, String)>, k: &str, v: &str) {
    match map.iter_mut().find(|(key, _)| key == k) {
        Some(e) => e.1 = v.to_string(),
        None => map.push((k.to_string(), v.to_string())),
    }
}

/// All at once, so f(y, x) swaps rather than merging.
fn rewrite_params(body: &str, pairs: &[(&str, &str)]) -> String {
    let mut map = Vec::new();
    for (k, v) in pairs {
        map_set(&mut map, k, v);
    }
    let from: Vec<String> = map
        .iter()
        .filter(|(k, v)| k != v)
        .map(|(k, _)| k.clone())
        .collect();
    if from.is_empty() {
        return body.to_string();
    }
    let re = js(
        &format!("(?<![A-Za-z_])(?:{})(?![A-Za-z0-9_])", names_pattern(&from)),
        "",
    );
    jsre::replace_all(&re, body, |c| {
        let n = jsre::group(c, 0);
        map.iter()
            .find(|(k, _)| k == n)
            .map_or_else(|| n.to_string(), |(_, v)| v.clone())
    })
    .into_owned()
}

fn curve_intent(
    rest: &str,
    functions: &BTreeMap<String, UserFunction>,
    variables: &BTreeMap<String, f64>,
) -> Option<GraphIntent> {
    static Y_EQ: Lazy = Lazy::new(|| js(r"(?s)^y\s*=\s*(.+)$", "i"));
    static Z_EQ: Lazy = Lazy::new(|| js(r"(?s)^z\s*=\s*(.+)$", "i"));
    // one `=`, not part of <=, >= or ==
    static IMPLICIT_EQ: Lazy = Lazy::new(|| js(r"^([^=<>!]+)=([^=<>]+)$", ""));
    static BARE_NAME: Lazy = Lazy::new(|| js(r"^([A-Za-z][A-Za-z0-9]*)$", ""));
    static FREE_Y: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_])y(?![A-Za-z0-9_])", ""));

    if let Some((name, params, body)) = crate::evaluate::parse_function_def(rest) {
        let label = format!("{name}({})", params.join(", "));
        let def = FunctionDef {
            name,
            params: params.clone(),
            body: body.clone(),
        };
        if params.len() == 2 {
            return Some(GraphIntent {
                function_def: Some(def),
                contour: Some(ContourKind::Levels),
                ..GraphIntent::new(
                    rewrite_params(&body, &[(&params[0], "x"), (&params[1], "y")]),
                    label,
                )
            });
        }
        if params.len() != 1 {
            return Some(GraphIntent {
                function_def: Some(def),
                ..GraphIntent::new("", rest)
            });
        }
        return Some(GraphIntent {
            function_def: Some(def),
            ..GraphIntent::new(rewrite_params(&body, &[(&params[0], "x")]), label)
        });
    }

    if let Ok(Some(m)) = Y_EQ.captures(rest) {
        let expression = jtrim(jsre::group(&m, 1));
        if expression.is_empty() {
            return None;
        }
        return Some(GraphIntent::new(expression, format!("y = {expression}")));
    }
    if let Ok(Some(m)) = Z_EQ.captures(rest) {
        let e = jtrim(jsre::group(&m, 1));
        return Some(GraphIntent {
            contour: Some(ContourKind::Levels),
            ..GraphIntent::new(e, format!("z = {e}"))
        });
    }

    if let Ok(Some(m)) = BARE_NAME.captures(rest)
        && let Some(def) = functions.get(jsre::group(&m, 1))
    {
        let name = jsre::group(&m, 1);
        let p = &def.params;
        // inlined so sampling compiles one expression instead of making a call per sample
        if p.len() == 2 {
            return Some(GraphIntent {
                contour: Some(ContourKind::Levels),
                ..GraphIntent::new(
                    rewrite_params(&def.body, &[(&p[0], "x"), (&p[1], "y")]),
                    format!("{name}({}, {})", p[0], p[1]),
                )
            });
        }
        if p.len() != 1 {
            return Some(GraphIntent::new("", name));
        }
        return Some(GraphIntent::new(
            rewrite_params(&def.body, &[(&p[0], "x")]),
            format!("{name}({})", p[0]),
        ));
    }

    // `x^2 + y^2 = 25` is the curve where the sides meet
    if let Ok(Some(eq)) = IMPLICIT_EQ.captures(rest) {
        let (l, r) = (jtrim(jsre::group(&eq, 1)), jtrim(jsre::group(&eq, 2)));
        if !l.is_empty() && !r.is_empty() {
            return Some(GraphIntent {
                contour: Some(ContourKind::Zero),
                ..GraphIntent::new(format!("({l}) - ({r})"), format!("{l} = {r}"))
            });
        }
    }
    // a free y makes it a surface, unless y is a variable
    if jsre::test(&FREE_Y, rest) && !variables.contains_key("y") {
        return Some(GraphIntent {
            contour: Some(ContourKind::Levels),
            ..GraphIntent::new(rest, rest)
        });
    }
    Some(GraphIntent::new(rest, rest))
}

struct Peeled {
    rest: String,
    groups: Vec<String>,
    open: Option<String>,
}

fn trim_end_commas(s: &str) -> String {
    s.trim_end_matches(|c: char| is_ws(c) || c == ',')
        .to_string()
}

// trailing `{…}` groups holding a comparison, and one still open at the end
fn peel_bounds(text: &str) -> Peeled {
    static TYPING: Lazy = Lazy::new(|| js(r"\{([^{}]*)$", ""));
    static GROUP: Lazy = Lazy::new(|| js(r"\{([^{}]*)\}\s*$", ""));
    static CMP_CHAR: Lazy = Lazy::new(|| js(r"[<>≤≥]", ""));
    let mut groups = Vec::new();
    let mut rest = text.to_string();
    let mut open = None;
    let typing = TYPING.captures(rest.as_str()).ok().flatten().map(|t| {
        (
            jsre::group(&t, 1).to_string(),
            t.get(0).map_or(0, |m| m.start()),
        )
    });
    if let Some((inner, at)) = typing
        && jsre::test(&CMP_CHAR, &inner)
    {
        open = Some(inner);
        rest = trim_end_commas(&rest[..at]);
    }
    while let Some((inner, at)) = GROUP.captures(rest.as_str()).ok().flatten().map(|g| {
        (
            jsre::group(&g, 1).to_string(),
            g.get(0).map_or(0, |m| m.start()),
        )
    }) {
        if !jsre::test(&CMP_CHAR, &inner) {
            break;
        }
        groups.insert(0, inner);
        rest = trim_end_commas(&rest[..at]);
    }
    Peeled { rest, groups, open }
}

fn is_less(op: &str) -> bool {
    op.contains('<') || op == "≤"
}

/// `piece.split(CMP)` with the separators kept, as a capturing split does.
fn split_cmp(piece: &str) -> Vec<String> {
    static CMP: Lazy = Lazy::new(|| js(r"(<=|>=|=<|=>|≤|≥|<|>)", ""));
    let mut out = Vec::new();
    let mut last = 0;
    for m in CMP.find_iter(piece).flatten() {
        out.push(jtrim(&piece[last..m.start()]).to_string());
        out.push(jtrim(m.as_str()).to_string());
        last = m.end();
    }
    out.push(jtrim(&piece[last..]).to_string());
    out
}

fn split_top_commas(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i64;
    let mut from = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(text[from..i].to_string());
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(text[from..].to_string());
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// `0 < x < 3`, `y >= 2`, `5 > y > 2`, comma-separated; None when a piece isn't one.
fn parse_bounds(
    groups: &[String],
    names: &[(String, Axis)],
    sci: &ScientificContext,
) -> Option<GraphBounds> {
    let mut out = GraphBounds::default();
    let num = |text: &str| -> Option<f64> {
        if text.is_empty() {
            return None;
        }
        eval_scientific(text, sci)
            .filter(|v| v.kind == ValueKind::Number && !v.n.is_nan())
            .map(|v| v.n)
    };
    let axis_of = |name: &str| names.iter().find(|(k, _)| k == name).map(|(_, a)| *a);
    for piece in groups.iter().flat_map(|g| split_top_commas(g)) {
        if jtrim(&piece).is_empty() {
            continue;
        }
        let t = split_cmp(&piece);
        let axis;
        let mut lo = f64::NEG_INFINITY;
        let mut hi = f64::INFINITY;
        if t.len() == 3 {
            let (a, op, b) = (&t[0], &t[1], &t[2]);
            let left = axis_of(a);
            axis = left.or_else(|| axis_of(b))?;
            let bound = num(if left.is_some() { b } else { a })?;
            // x < 3 and 3 > x both cap x
            if is_less(op) == left.is_some() {
                hi = bound;
            } else {
                lo = bound;
            }
        } else if t.len() == 5 {
            let (a, op1, v, op2, b) = (&t[0], &t[1], &t[2], &t[3], &t[4]);
            axis = axis_of(v)?;
            let na = num(a);
            let nb = num(b);
            if is_less(op1) != is_less(op2) {
                return None;
            }
            let (na, nb) = (na?, nb?);
            (lo, hi) = if is_less(op1) { (na, nb) } else { (nb, na) };
        } else {
            return None;
        }
        match axis {
            Axis::X => out.x.push(GraphRange { lo, hi }),
            Axis::Y => out.y.push(GraphRange { lo, hi }),
        }
    }
    Some(out)
}

/// `graph f` looks `f` up in `functions`; trailing braces like `{0 < x < 3}` are bounds.
pub fn parse_graph_intent(
    input: &str,
    functions: &BTreeMap<String, UserFunction>,
    variables: &BTreeMap<String, f64>,
) -> Option<GraphIntent> {
    static GRAPH_CMD: Lazy = Lazy::new(|| js(r"(?s)^\s*graph\s+(.+?)\s*$", "i"));
    let m = GRAPH_CMD.captures(input).ok()??;
    let peeled = peel_bounds(jtrim(jsre::group(&m, 1)));
    let rest = split_glued_functions(&peeled.rest, &|_| false);
    if rest.is_empty() {
        return None;
    }
    let intent = curve_intent(&rest, functions, variables)?;
    if intent.expression.is_empty() || (peeled.groups.is_empty() && peeled.open.is_none()) {
        return Some(intent);
    }

    // a function's own parameters name its axes: f(t) bounds t, g(u, v) bounds u and v
    let params = intent
        .function_def
        .as_ref()
        .map(|d| d.params.clone())
        .or_else(|| functions.get(&rest).map(|f| f.params.clone()));
    let mut names: Vec<(String, Axis)> = Vec::new();
    let mut put = |k: &str, a: Axis| match names.iter_mut().find(|(key, _)| key == k) {
        Some(e) => e.1 = a,
        None => names.push((k.to_string(), a)),
    };
    match &params {
        Some(p) if p.len() == 2 => {
            put(&p[0], Axis::X);
            put(&p[1], Axis::Y);
        }
        _ => {
            put("y", Axis::Y);
            put(
                params
                    .as_ref()
                    .and_then(|p| p.first())
                    .map_or("x", String::as_str),
                Axis::X,
            );
        }
    }
    let sci = ScientificContext {
        variables: variables.clone(),
        functions: functions.clone(),
        ..ScientificContext::default()
    };
    let Some(mut bounds) = parse_bounds(&peeled.groups, &names, &sci) else {
        return Some(GraphIntent {
            bounds_error: Some("Write bounds like {0 < x < 3} or {2 < y < 5}".into()),
            ..intent
        });
    };
    // a group still being typed counts once it reads as bounds
    if let Some(open) = &peeled.open
        && let Some(typing) = parse_bounds(std::slice::from_ref(open), &names, &sci)
    {
        bounds.x.extend(typing.x);
        bounds.y.extend(typing.y);
    }
    if bounds.x.is_empty() && bounds.y.is_empty() {
        return Some(intent);
    }
    let label = format!(
        "{} {}",
        intent.label,
        peeled
            .groups
            .iter()
            .map(|g| format!("{{{}}}", jtrim(g)))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Some(GraphIntent {
        bounds: Some(bounds),
        label: jtrim(&label).to_string(),
        ..intent
    })
}

/// True even for an incomplete command.
pub fn is_graph_command(input: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*graph(?:\s|$)", "i"));
    jsre::test(&RE, input)
}

// ---- sampling ----

fn in_ranges(v: f64, rs: &[GraphRange]) -> bool {
    rs.is_empty() || rs.iter().any(|r| v >= r.lo && v <= r.hi)
}

// the span the ranges cover, when it is finite
fn hull(rs: Option<&Vec<GraphRange>>) -> Option<(f64, f64)> {
    let rs = rs.filter(|r| !r.is_empty())?;
    let lo = min_of(rs.iter().map(|r| r.lo));
    let hi = max_of(rs.iter().map(|r| r.hi));
    (lo.is_finite() && hi.is_finite() && hi > lo).then_some((lo, hi))
}

fn inside(b: Option<&GraphBounds>, x: f64, y: f64) -> bool {
    b.is_none_or(|b| in_ranges(x, &b.x) && in_ranges(y, &b.y))
}

/// A compiled curve: `compileGraphY`.
#[derive(Debug)]
pub struct GraphY {
    f: Option<RefCell<Compiled>>,
}

impl GraphY {
    pub fn at(&self, x: f64) -> Option<f64> {
        let f = self.f.as_ref()?;
        if !x.is_finite() {
            return None;
        }
        f.borrow_mut()
            .at(x)
            .filter(|v| v.kind == ValueKind::Number && v.n.is_finite())
            .map(|v| v.n)
    }
}

/// `compileGraphY(expression, { ...options, angleMode: mode })`.
pub fn compile_graph_y(
    expression: &str,
    options: &GraphOptions,
    mode: Option<AngleMode>,
) -> GraphY {
    let f = if jtrim(expression).is_empty() {
        None
    } else {
        compile_scientific(expression, &options.sci(mode), "x").map(RefCell::new)
    };
    GraphY { f }
}

/// A compiled surface: `compileGraphZ`.
#[derive(Debug)]
pub struct GraphZ {
    f: Option<RefCell<Compiled>>,
}

impl GraphZ {
    pub fn at(&self, x: f64, y: f64) -> Option<f64> {
        self.f
            .as_ref()?
            .borrow_mut()
            .at2(x, y)
            .filter(|v| v.kind == ValueKind::Number && v.n.is_finite())
            .map(|v| v.n)
    }
}

pub fn compile_graph_z(
    expression: &str,
    options: &GraphOptions,
    mode: Option<AngleMode>,
) -> GraphZ {
    let f = if jtrim(expression).is_empty() {
        None
    } else {
        compile_scientific2(expression, &options.sci(mode), "x", "y").map(RefCell::new)
    };
    GraphZ { f }
}

/// `evaluateGraphY`: y at one x.
pub fn evaluate_graph_y(expression: &str, x: f64, options: &GraphOptions) -> Option<f64> {
    if jtrim(expression).is_empty() || !x.is_finite() {
        return None;
    }
    let mut sci = options.sci(options.angle_mode);
    sci.variables.insert("x".into(), x);
    eval_scientific(expression, &sci)
        .filter(|v| v.kind == ValueKind::Number && v.n.is_finite())
        .map(|v| v.n)
}

/// `compileGraphZ(expression, options)(x, y)`, the FFI's `evaluateGraphZ`.
pub fn evaluate_graph_z(expression: &str, x: f64, y: f64, options: &GraphOptions) -> Option<f64> {
    compile_graph_z(expression, options, options.angle_mode).at(x, y)
}

/// None outside the bounds, so the path breaks there and bisection finds the edges.
fn restrict(f: &GraphY, bounds: Option<&GraphBounds>, x: f64) -> Option<f64> {
    let Some(b) = bounds else { return f.at(x) };
    if !in_ranges(x, &b.x) {
        return None;
    }
    f.at(x).filter(|y| in_ranges(*y, &b.y))
}

fn normalize_domain(domain: Option<(f64, f64)>, fallback: (f64, f64)) -> (f64, f64) {
    let (lo, hi) = domain.unwrap_or(fallback);
    if !lo.is_finite() || !hi.is_finite() || lo == hi {
        return fallback;
    }
    if lo < hi { (lo, hi) } else { (hi, lo) }
}

fn sample_with(
    f: &dyn Fn(f64) -> Option<f64>,
    domain: (f64, f64),
    count: Option<f64>,
) -> Vec<GraphPoint> {
    let (x_min, x_max) = domain;
    let n = jmax(2.0, count.unwrap_or(DEFAULT_GRAPH_SAMPLES).floor());
    // (a NaN count stays NaN: no samples, as in the TS)
    let n = if n > MAX_SAMPLES { MAX_SAMPLES } else { n };
    let mut points = Vec::new();
    let mut i = 0.0;
    while i < n {
        let x = x_min + (x_max - x_min) * (i / (n - 1.0));
        points.push(GraphPoint { x, y: f(x) });
        i += 1.0;
    }
    points
}

/// `sampleGraph`.
pub fn sample_graph(expression: &str, options: &GraphOptions) -> Vec<GraphPoint> {
    let g = compile_graph_y(expression, options, options.angle_mode);
    sample_with(
        &|x| g.at(x),
        normalize_domain(options.domain, DEFAULT_GRAPH_DOMAIN),
        options.sample_count,
    )
}

// samples at the x bounds' own ends, so a piece starts and stops exactly there
fn with_ends(
    points: Vec<GraphPoint>,
    f: &dyn Fn(f64) -> Option<f64>,
    bounds: Option<&GraphBounds>,
) -> Vec<GraphPoint> {
    let Some(b) = bounds.filter(|b| !b.x.is_empty()) else {
        return points;
    };
    if points.len() < 2 {
        return points;
    }
    let lo = points[0].x;
    let hi = points[points.len() - 1].x;
    let ends: Vec<f64> =
        b.x.iter()
            .flat_map(|r| [r.lo, r.hi])
            .filter(|&x| x > lo && x < hi && !points.iter().any(|p| p.x == x))
            .collect();
    if ends.is_empty() {
        return points;
    }
    let mut all = points;
    all.extend(ends.iter().map(|&x| GraphPoint { x, y: f(x) }));
    // Array.prototype.sort is stable, as sort_by is
    all.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    all
}

struct Refiner<'a> {
    f: &'a dyn Fn(f64) -> Option<f64>,
    min_width: f64,
    steep: f64,
    view: YScale,
    budget: i64,
    out: Vec<GraphPoint>,
}

impl Refiner<'_> {
    fn sample(&mut self, x: f64) -> GraphPoint {
        self.budget -= 1;
        GraphPoint { x, y: (self.f)(x) }
    }

    fn off_view(&self, a: f64, b: f64) -> bool {
        (a > self.view.max && b > self.view.max) || (a < self.view.min && b < self.view.min)
    }

    // pushes the samples strictly between a and b; the caller pushes b
    fn between(&mut self, a: GraphPoint, b: GraphPoint) {
        match (a.y, b.y) {
            (None, None) => {}
            (None, _) | (_, None) => {
                // bisect for the last defined x
                let mut def = if a.y.is_none() { b } else { a };
                let mut undef = if a.y.is_none() { a } else { b };
                let mut moved = false;
                let mut k = 0;
                while k < 40 && self.budget > 0 {
                    let m = self.sample((def.x + undef.x) / 2.0);
                    if m.x == def.x || m.x == undef.x {
                        break;
                    }
                    if m.y.is_none() {
                        undef = m;
                    } else {
                        def = m;
                        moved = true;
                    }
                    k += 1;
                }
                if !moved {
                    return;
                }
                if a.y.is_none() {
                    self.out.push(def);
                    self.between(def, b);
                } else {
                    self.between(a, def);
                    self.out.push(def);
                }
            }
            (Some(ay), Some(by)) => {
                if (by - ay).abs() <= self.steep || self.off_view(ay, by) || self.budget <= 0 {
                    return;
                }
                if b.x - a.x <= self.min_width {
                    self.out.push(GraphPoint {
                        x: (a.x + b.x) / 2.0,
                        y: None,
                    });
                    return;
                }
                let m = self.sample((a.x + b.x) / 2.0);
                self.between(a, m);
                self.out.push(m);
                self.between(m, b);
            }
        }
    }
}

/// Adds samples where the uniform grid is too coarse for the view: bisects to where the curve's domain ends,
/// subdivides steep visible stretches, and breaks the path where a jump survives to a vanishing interval.
fn refine_samples(
    points: &[GraphPoint],
    f: &dyn Fn(f64) -> Option<f64>,
    view: YScale,
) -> Vec<GraphPoint> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let span = points[points.len() - 1].x - points[0].x;
    let mut r = Refiner {
        f,
        min_width: span * 1e-9,
        steep: (view.max - view.min) * 0.02,
        view,
        budget: points.len() as i64 * 8,
        out: vec![points[0]],
    };
    for i in 1..points.len() {
        r.between(points[i - 1], points[i]);
        r.out.push(points[i]);
    }
    r.out
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = (sorted.len() - 1) as f64 * q;
    let base = pos.floor();
    let rest = pos - base;
    let base = base as usize;
    let a = sorted[base];
    let b = sorted[(base + 1).min(sorted.len() - 1)];
    a + rest * (b - a)
}

fn sort_numbers(v: &mut [f64]) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
}

/// A central percentile band keeps poles from dominating the view, but only when there are real outliers. A
/// range that nearly reaches zero is widened to include the x-axis.
pub fn auto_y_scale(points: &[GraphPoint]) -> YScale {
    let (pad, percentile, fallback) = (0.08, 0.02, 10.0);
    let mut sorted: Vec<f64> = points
        .iter()
        .filter_map(|p| p.y.filter(|y| y.is_finite()))
        .collect();
    if sorted.is_empty() {
        return YScale {
            min: -fallback,
            max: fallback,
        };
    }
    sort_numbers(&mut sorted);
    let lo = quantile(&sorted, percentile);
    let hi = quantile(&sorted, 1.0 - percentile);
    let full_min = sorted[0];
    let full_max = sorted[sorted.len() - 1];
    let mut min = if lo.is_finite() { lo } else { full_min };
    let mut max = if hi.is_finite() { hi } else { full_max };
    // trimming is for poles; a curve without them (eˣ, a semicircle) is shown whole
    if max > min && full_max - full_min <= 2.0 * (max - min) {
        min = full_min;
        max = full_max;
    }
    if min > 0.0 && min <= 0.25 * (max - min) {
        min = 0.0;
    } else if max < 0.0 && -max <= 0.25 * (max - min) {
        max = 0.0;
    }
    if min == max {
        let bump = jmax(1.0, min.abs() * 0.1);
        min -= bump;
        max += bump;
    }
    let margin = (max - min) * pad;
    YScale {
        min: min - margin,
        max: max + margin,
    }
}

/// The shortest decimal near `x` that `accept` still takes: 0, then 1, 2, … significant digits.
pub fn tidy(x: f64, accept: &dyn Fn(f64) -> bool) -> f64 {
    if x != 0.0 && accept(0.0) {
        return 0.0;
    }
    for digits in 1..17 {
        let c = precision(x, digits);
        if c == x {
            return x;
        }
        if accept(c) {
            return c;
        }
    }
    x
}

fn golden_section(
    f: &dyn Fn(f64) -> Option<f64>,
    lo: f64,
    hi: f64,
    kind: CriticalKind,
) -> Option<(f64, f64)> {
    let inv_phi = (5f64.sqrt() - 1.0) / 2.0;
    let s = if kind == CriticalKind::Max { -1.0 } else { 1.0 };
    let g = |x: f64| f(x).map(|y| s * y);
    let (mut a, mut b) = (lo, hi);
    let mut c = b - inv_phi * (b - a);
    let mut d = a + inv_phi * (b - a);
    let mut gc = g(c);
    let mut gd = g(d);
    let mut k = 0;
    while k < 200 && c < d {
        let (Some(vc), Some(vd)) = (gc, gd) else {
            return None;
        };
        if vc < vd {
            b = d;
            d = c;
            gd = gc;
            c = b - inv_phi * (b - a);
            gc = g(c);
        } else {
            a = c;
            c = d;
            gc = gd;
            d = a + inv_phi * (b - a);
            gd = g(d);
        }
        k += 1;
    }
    let x = (a + b) / 2.0;
    f(x).map(|y| (x, y))
}

fn dedupe_by_x<T: Copy>(pts: Vec<T>, x: impl Fn(&T) -> f64) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for p in pts {
        if let Some(last) = out.last()
            && (x(last) - x(&p)).abs() <= 1e-6
        {
            continue;
        }
        out.push(p);
    }
    out
}

/// Where the sampled slope changes sign. With `f`, each is refined by golden-section search and poles dropped.
pub fn find_critical_points(
    points: &[GraphPoint],
    f: Option<&dyn Fn(f64) -> Option<f64>>,
) -> Vec<CriticalPoint> {
    let mut out = Vec::new();
    for i in 1..points.len().saturating_sub(1) {
        let (prev, cur, next) = (points[i - 1], points[i], points[i + 1]);
        let (Some(py), Some(cy), Some(ny)) = (prev.y, cur.y, next.y) else {
            continue;
        };
        let dx1 = cur.x - prev.x;
        let dx2 = next.x - cur.x;
        if dx1 == 0.0 || dx2 == 0.0 {
            continue;
        }
        let d1 = (cy - py) / dx1;
        let d2 = (ny - cy) / dx2;
        if !d1.is_finite() || !d2.is_finite() {
            continue;
        }
        let (s1, s2) = (sign(d1), sign(d2));
        if s1 == 0.0 || s2 == 0.0 || s1 == s2 {
            continue;
        }
        let kind = if s1 > 0.0 {
            CriticalKind::Max
        } else {
            CriticalKind::Min
        };
        let Some(f) = f else {
            out.push(CriticalPoint {
                x: cur.x,
                y: cy,
                kind,
            });
            continue;
        };
        let Some((bx, by)) = golden_section(f, prev.x, next.x, kind) else {
            continue;
        };
        // a smooth turn moves y by less than the local variation; a pole runs away
        let variation = (cy - py).abs() + (ny - cy).abs();
        if (by - cy).abs() > 4.0 * variation {
            continue;
        }
        // golden section only pins x to about √ε, so report the simplest x f can't tell apart
        let s = if kind == CriticalKind::Max { -1.0 } else { 1.0 };
        let tol = 8.0 * f64::EPSILON * jmax(by.abs(), cy.abs());
        let x = tidy(bx, &|c| {
            if c < prev.x || c > next.x {
                return false;
            }
            f(c).is_some_and(|y| s * y <= s * by + tol)
        });
        let y = if x == bx {
            by
        } else {
            f(x).unwrap_or(f64::NAN)
        };
        out.push(CriticalPoint { x, y, kind });
    }
    dedupe_by_x(out, |p| p.x)
}

fn bisect_root(
    f: &dyn Fn(f64) -> Option<f64>,
    lo: f64,
    y_lo: f64,
    hi: f64,
    y_hi: f64,
) -> Option<f64> {
    let scale = jmax(y_lo.abs(), y_hi.abs());
    let (mut a, mut b, mut ya) = (lo, hi, y_lo);
    for _ in 0..200 {
        let mid = (a + b) / 2.0;
        if mid <= a || mid >= b {
            break;
        }
        let ym = f(mid)?;
        if ym == 0.0 {
            a = mid;
            b = mid;
            break;
        }
        if sign(ym) == sign(ya) {
            a = mid;
            ya = ym;
        } else {
            b = mid;
        }
    }
    let fa = f(a)?;
    let fb = f(b)?;
    let (x, y) = if fa.abs() <= fb.abs() {
        (a, fa)
    } else {
        (b, fb)
    };
    if y.abs() > 1e-6 * scale {
        return None;
    }
    // prefer 0.3 over 0.30000000000000004 and 0 over 5e-324 when f can't tell them apart
    let tol = y.abs() + 8.0 * f64::EPSILON * scale;
    Some(tidy(x, &|c| {
        if c < lo || c > hi {
            return false;
        }
        f(c).is_some_and(|yc| yc.abs() <= tol)
    }))
}

/// With `f`, sign changes are refined by bisection and poles and jumps dropped. A run of zero samples is one
/// root; an all-zero curve has none.
pub fn find_roots(points: &[GraphPoint], f: Option<&dyn Fn(f64) -> Option<f64>>) -> Vec<f64> {
    let mut out = Vec::new();
    let is_zero = |y: Option<f64>| y.is_some_and(|y| y.abs() <= 1e-12);
    if points.is_empty() {
        return out;
    }
    let last = points.len() - 1;
    let mut i = 0;
    while i <= last {
        let p = points[i];
        if is_zero(p.y) {
            let mut j = i;
            while j < last && is_zero(points[j + 1].y) {
                j += 1;
            }
            // report the end of the run that meets the curve (`floor(x)`, `max(0, x)` give 0)
            if i > 0 || j < last {
                let k = if i == 0 { j } else { i };
                let x = points[k].x;
                let y = points[k].y.unwrap_or(0.0).abs();
                let lo = points[k.saturating_sub(1)].x;
                let hi = points[(k + 1).min(last)].x;
                // a grid x like 0.3000000000000007 is reported as 0.3 when f(0.3) is as small
                out.push(match f {
                    Some(f) => tidy(x, &|c| {
                        c > lo && c < hi && f(c).map_or(f64::INFINITY, f64::abs) <= y
                    }),
                    None => x,
                });
            }
            i = j + 1;
            continue;
        }
        if i == 0 {
            i += 1;
            continue;
        }
        let prev = points[i - 1];
        let (Some(py), Some(y)) = (prev.y, p.y) else {
            i += 1;
            continue;
        };
        if is_zero(prev.y) || sign(py) == sign(y) {
            i += 1;
            continue;
        }
        let x = match f {
            Some(f) => bisect_root(f, prev.x, py, p.x, y),
            None => Some(prev.x + (py / (py - y)) * (p.x - prev.x)),
        };
        if let Some(x) = x {
            out.push(x);
        }
        i += 1;
    }
    dedupe_by_x(out, |x| *x)
}

/// Compares a degree and a radian compilation at a few probe points.
fn depends_on_angle(a: &dyn Fn(f64) -> Option<f64>, b: &dyn Fn(f64) -> Option<f64>) -> bool {
    [0.37, 1.1, 2.9, -4.3, 7.7]
        .iter()
        .any(|&x| match (a(x), b(x)) {
            (Some(ya), Some(yb)) => (ya - yb).abs() > 1e-9 * jmax(1.0, ya.abs()),
            (ya, yb) => ya != yb,
        })
}

fn other_mode(mode: AngleMode) -> AngleMode {
    if mode == AngleMode::Deg {
        AngleMode::Rad
    } else {
        AngleMode::Deg
    }
}

fn degree_or_default(unit: Option<AngleMode>) -> (f64, f64) {
    if unit == Some(AngleMode::Deg) {
        DEGREE_GRAPH_DOMAIN
    } else {
        DEFAULT_GRAPH_DOMAIN
    }
}

/// `buildGraph`: None when the input is not a graph command.
pub fn build_graph(input: &str, options: &GraphOptions) -> Option<GraphResult> {
    let intent = parse_graph_intent(input, &options.functions, &options.variables)?;

    if intent.expression.is_empty() || intent.bounds_error.is_some() {
        let msg = intent.bounds_error.clone().unwrap_or_else(|| {
            if intent
                .function_def
                .as_ref()
                .is_some_and(|d| d.params.len() != 1)
            {
                "Graphing takes a function of x, or of x and y".into()
            } else {
                "Unknown function — define it first (e.g. f(x) = …)".into()
            }
        });
        return Some(GraphResult {
            intent,
            points: Vec::new(),
            domain: normalize_domain(options.domain, DEFAULT_GRAPH_DOMAIN),
            y_scale: YScale {
                min: -10.0,
                max: 10.0,
            },
            critical_points: Vec::new(),
            roots: Vec::new(),
            angle_unit: None,
            contours: None,
            error: Some(msg),
            readout: GraphReadout::None,
        });
    }

    // an inline def is usable before history registers it
    let mut base = options.clone();
    if let Some(d) = &intent.function_def {
        base.functions.insert(
            d.name.clone(),
            UserFunction {
                params: d.params.clone(),
                body: d.body.clone(),
            },
        );
    }
    let mode = options.angle_mode.unwrap_or(AngleMode::Rad);
    if intent.contour.is_some() {
        return Some(build_contour(intent, &base, mode));
    }
    let curve = compile_graph_y(&intent.expression, &base, Some(mode));
    let other = compile_graph_y(&intent.expression, &base, Some(other_mode(mode)));
    let angle_unit = depends_on_angle(&|x| curve.at(x), &|x| other.at(x)).then_some(mode);
    let bounds = intent.bounds.clone();
    let f = |x: f64| restrict(&curve, bounds.as_ref(), x);
    let domain = normalize_domain(options.domain, degree_or_default(angle_unit));
    let uniform = with_ends(
        sample_with(&f, domain, base.sample_count),
        &f,
        bounds.as_ref(),
    );
    let y_scale = auto_y_scale(&uniform);
    let readout = GraphReadout::Curve {
        expression: intent.expression.clone(),
        options: base.clone(),
        mode,
        bounds: bounds.clone(),
    };
    if !uniform.iter().any(|p| p.y.is_some()) {
        let somewhere = bounds.is_some()
            && sample_with(&|x| curve.at(x), domain, base.sample_count)
                .iter()
                .any(|p| p.y.is_some());
        return Some(GraphResult {
            intent,
            points: uniform,
            domain,
            y_scale,
            critical_points: Vec::new(),
            roots: Vec::new(),
            angle_unit,
            contours: None,
            error: Some(
                if somewhere {
                    "Nothing to plot within the bounds here"
                } else {
                    "Could not evaluate expression over the domain"
                }
                .into(),
            ),
            readout,
        });
    }

    let points = refine_samples(&uniform, &f, y_scale);
    let critical_points = find_critical_points(&points, Some(&f));
    let roots = if options.find_roots != Some(false) {
        find_roots(&points, Some(&f))
    } else {
        Vec::new()
    };
    Some(GraphResult {
        intent,
        points,
        domain,
        y_scale,
        critical_points,
        roots,
        angle_unit,
        contours: None,
        error: None,
        readout,
    })
}

/// What `buildGraph(input, options).y` / `.z` evaluate, without sampling: for a readout under the pointer, so
/// the UI needn't rebuild the graph per hover. Native only (the TS keeps the closures).
pub fn graph_readout(input: &str, options: &GraphOptions) -> GraphReadout {
    let Some(intent) = parse_graph_intent(input, &options.functions, &options.variables) else {
        return GraphReadout::None;
    };
    if intent.expression.is_empty() || intent.bounds_error.is_some() {
        return GraphReadout::None;
    }
    let mut base = options.clone();
    if let Some(d) = &intent.function_def {
        base.functions.insert(
            d.name.clone(),
            UserFunction {
                params: d.params.clone(),
                body: d.body.clone(),
            },
        );
    }
    let mode = options.angle_mode.unwrap_or(AngleMode::Rad);
    let (expression, bounds) = (intent.expression, intent.bounds);
    if intent.contour.is_some() {
        GraphReadout::Surface {
            expression,
            options: base,
            mode,
            bounds,
        }
    } else {
        GraphReadout::Curve {
            expression,
            options: base,
            mode,
            bounds,
        }
    }
}

/// `result.y(x)` for `input`'s graph.
pub fn readout_y(input: &str, options: &GraphOptions, x: f64) -> Option<f64> {
    readout_result(graph_readout(input, options)).y_at(x)
}

/// `result.z?.(x, y)` for `input`'s graph.
pub fn readout_z(input: &str, options: &GraphOptions, x: f64, y: f64) -> Option<f64> {
    readout_result(graph_readout(input, options)).z_at(x, y)
}

fn readout_result(readout: GraphReadout) -> GraphResult {
    GraphResult {
        intent: GraphIntent::new("", ""),
        points: Vec::new(),
        domain: DEFAULT_GRAPH_DOMAIN,
        y_scale: YScale {
            min: -10.0,
            max: 10.0,
        },
        critical_points: Vec::new(),
        roots: Vec::new(),
        angle_unit: None,
        contours: None,
        error: None,
        readout,
    }
}

/// A 1, 2 or 5 × 10ⁿ step at or above (`ceil`) or at or below (`floor`) `raw`.
fn nice_step(raw: f64, ceil: bool) -> f64 {
    let mag = jsmath::pow(10.0, jsmath::log10(raw).floor());
    let steps = [1.0, 2.0, 5.0, 10.0].map(|m| m * mag);
    let found = if ceil {
        steps.iter().find(|&&s| s >= raw * (1.0 - 1e-9))
    } else {
        steps.iter().rev().find(|&&s| s <= raw * (1.0 + 1e-9))
    };
    found.copied().unwrap_or(f64::NAN)
}

// at least `span` wide about `mid`, with ends on a round step
fn nice_window(mid: f64, span: f64) -> (f64, f64) {
    let step = nice_step(span / 10.0, false);
    let lo = ((mid - span / 2.0) / step + 1e-9).floor() * step;
    let hi = ((mid + span / 2.0) / step - 1e-9).ceil() * step;
    (precision(lo, 12), precision(hi, 12))
}

/// `graphHome`: the window a graph opens on, ±10 (±360 for trig in degrees), narrowed around the roots and
/// extrema when they all sit in a small middle part of it.
pub fn graph_home(input: &str, options: &GraphOptions) -> (f64, f64) {
    // finite bounds frame the view, with a little room either side
    let intent = parse_graph_intent(input, &options.functions, &options.variables);
    let ok = intent.as_ref().filter(|i| i.bounds_error.is_none());
    let bx = ok.and_then(|i| hull(i.bounds.as_ref().map(|b| &b.x)));
    let by = ok
        .filter(|i| i.contour.is_some())
        .and_then(|i| hull(i.bounds.as_ref().map(|b| &b.y)));
    if bx.is_some() || by.is_some() {
        let span = jmax(
            bx.map_or(0.0, |b| b.1 - b.0),
            by.map_or(0.0, |b| (b.1 - b.0) / options.aspect()),
        );
        return nice_window(bx.map_or(0.0, |b| (b.0 + b.1) / 2.0), span * 1.2);
    }
    let g = build_graph(
        input,
        &GraphOptions {
            domain: None,
            ..options.clone()
        },
    );
    let Some(g) = g else {
        return DEFAULT_GRAPH_DOMAIN;
    };
    if g.error.is_some() {
        return g.domain;
    }
    let (lo, hi) = g.domain;
    // y bounds frame the pieces they keep
    let drawn: Vec<f64> =
        if intent.as_ref().is_some_and(|i| i.bounds.is_some()) && g.contours.is_none() {
            g.points
                .iter()
                .filter(|p| p.y.is_some())
                .map(|p| p.x)
                .collect()
        } else {
            Vec::new()
        };
    if drawn.len() > 1 {
        let a = min_of(drawn.iter().copied());
        let b = max_of(drawn.iter().copied());
        if b > a && b - a < 0.6 * (hi - lo) {
            return nice_window((a + b) / 2.0, (b - a) * 1.2);
        }
    }
    let xs: Vec<f64> = g
        .roots
        .iter()
        .copied()
        .chain(g.critical_points.iter().map(|c| c.x))
        .collect();
    if xs.len() < 2 {
        return g.domain;
    }
    let a = min_of(xs.iter().copied());
    let b = max_of(xs.iter().copied());
    let span = jmax((b - a) * 1.5, (hi - lo) / 10.0);
    if !(b > a) || span > 0.6 * (hi - lo) {
        return g.domain;
    }
    nice_window((a + b) / 2.0, span)
}

const FULL: GraphRange = GraphRange {
    lo: f64::NEG_INFINITY,
    hi: f64::INFINITY,
};

/// Level curves of f(x, y) by marching squares, clipped to the bounds.
fn build_contour(intent: GraphIntent, options: &GraphOptions, mode: AngleMode) -> GraphResult {
    let raw = compile_graph_z(&intent.expression, options, Some(mode));
    let other = compile_graph_z(&intent.expression, options, Some(other_mode(mode)));
    let angle_unit = depends_on_angle(&|x| raw.at(x, 0.61 - x / 3.0), &|x| {
        other.at(x, 0.61 - x / 3.0)
    })
    .then_some(mode);
    let domain = normalize_domain(options.domain, degree_or_default(angle_unit));
    let (x0, x1) = domain;
    let aspect = options.aspect();
    let b = intent.bounds.clone();
    let by = hull(b.as_ref().map(|b| &b.y));
    let y_mid = by.map_or(0.0, |b| (b.0 + b.1) / 2.0);
    let y_half = ((x1 - x0) * aspect) / 2.0;
    let y_scale = YScale {
        min: y_mid - y_half,
        max: y_mid + y_half,
    };

    let nx = CONTOUR_COLUMNS;
    let ny = jmax(4.0, jsnum::round(nx as f64 * aspect)) as usize;
    let dx = (x1 - x0) / nx as f64;
    let dy = (y_scale.max - y_scale.min) / ny as f64;
    let mut grid = vec![0.0; (nx + 1) * (ny + 1)];
    let mut seen = Vec::new();
    for j in 0..=ny {
        let y = y_scale.min + j as f64 * dy;
        for i in 0..=nx {
            let x = x0 + i as f64 * dx;
            let v = raw.at(x, y).unwrap_or(f64::NAN);
            grid[j * (nx + 1) + i] = v;
            if !v.is_nan() && inside(b.as_ref(), x, y) {
                seen.push(v);
            }
        }
    }
    let readout = GraphReadout::Surface {
        expression: intent.expression.clone(),
        options: options.clone(),
        mode,
        bounds: b.clone(),
    };
    let mut result = GraphResult {
        intent,
        points: Vec::new(),
        domain,
        y_scale,
        critical_points: Vec::new(),
        roots: Vec::new(),
        angle_unit,
        contours: None,
        error: None,
        readout,
    };
    if seen.is_empty() {
        let somewhere = b.is_some() && grid.iter().any(|v| !v.is_nan());
        result.error = Some(
            if somewhere {
                "Nothing to plot within the bounds here"
            } else {
                "Could not evaluate expression over the domain"
            }
            .into(),
        );
        return result;
    }

    let levels = if result.intent.contour == Some(ContourKind::Zero) {
        vec![0.0]
    } else {
        contour_levels(&mut seen)
    };
    // the bounds as rectangles, each segment kept only where it's inside one
    let rects: Option<Vec<(GraphRange, GraphRange)>> = b.as_ref().map(|b| {
        let xs = if b.x.is_empty() {
            vec![FULL]
        } else {
            b.x.clone()
        };
        let ys = if b.y.is_empty() {
            vec![FULL]
        } else {
            b.y.clone()
        };
        xs.iter()
            .flat_map(|rx| ys.iter().map(move |ry| (*rx, *ry)))
            .collect()
    });
    let mut contours: Vec<ContourLevel> = levels
        .iter()
        .map(|&level| {
            let found = marching_squares(&grid, nx, ny, level, x0, y_scale.min, dx, dy);
            let segments = match &rects {
                None => found,
                Some(rects) => {
                    let mut segs = Vec::new();
                    for k in (0..found.len()).step_by(4) {
                        for (rx, ry) in rects {
                            if let Some(c) = clip_segment(
                                found[k],
                                found[k + 1],
                                found[k + 2],
                                found[k + 3],
                                rx,
                                ry,
                            ) {
                                segs.extend(c);
                            }
                        }
                    }
                    segs
                }
            };
            ContourLevel {
                level,
                label: with_minus(&format_number(level, 4.0)),
                segments,
                at: None,
            }
        })
        .collect();
    // a lone f = 0 curve needs no label
    if result.intent.contour == Some(ContourKind::Levels) {
        place_labels(&mut contours, domain, y_scale);
    }
    result.contours = Some(contours);
    result
}

// a handful of round values across the middle of what f takes, so a pole doesn't crowd them out
fn contour_levels(values: &mut [f64]) -> Vec<f64> {
    sort_numbers(values);
    let lo = quantile(values, 0.02);
    let hi = quantile(values, 0.98);
    if !(hi > lo) {
        return Vec::new();
    }
    let step = nice_step((hi - lo) / CONTOUR_LINES, true);
    let mut out = Vec::new();
    let mut k = (lo / step).floor() + 1.0;
    while k * step < hi && out.len() < 12 {
        out.push(precision(k * step, 12));
        k += 1.0;
    }
    out
}

/// x1, y1, x2, y2 per crossing, with saddles split by the cell's centre.
#[allow(clippy::too_many_arguments)]
pub fn marching_squares(
    grid: &[f64],
    nx: usize,
    ny: usize,
    level: f64,
    x0: f64,
    y0: f64,
    dx: f64,
    dy: f64,
) -> Vec<f64> {
    let mut out = Vec::new();
    let w = nx + 1;
    let mut ex = [0.0f64; 4];
    let mut ey = [0.0f64; 4];
    let get = |k: usize| grid.get(k).copied().unwrap_or(f64::NAN);
    for j in 0..ny {
        for i in 0..nx {
            let a = get(j * w + i);
            let b = get(j * w + i + 1);
            let c = get((j + 1) * w + i + 1);
            let d = get((j + 1) * w + i);
            if (a + b + c + d).is_nan() {
                continue;
            }
            let idx = u8::from(a > level)
                | (u8::from(b > level) << 1)
                | (u8::from(c > level) << 2)
                | (u8::from(d > level) << 3);
            if idx == 0 || idx == 15 {
                continue;
            }
            let x = x0 + i as f64 * dx;
            let y = y0 + j as f64 * dy;
            let t = |p: f64, q: f64| (level - p) / (q - p);
            // edges 0 bottom, 1 right, 2 top, 3 left, where they cross
            let mut cross = Vec::with_capacity(4);
            if (idx & 1) != (idx & 2) >> 1 {
                ex[0] = x + t(a, b) * dx;
                ey[0] = y;
                cross.push(0);
            }
            if (idx & 2) >> 1 != (idx & 4) >> 2 {
                ex[1] = x + dx;
                ey[1] = y + t(b, c) * dy;
                cross.push(1);
            }
            if (idx & 4) >> 2 != (idx & 8) >> 3 {
                ex[2] = x + t(d, c) * dx;
                ey[2] = y + dy;
                cross.push(2);
            }
            if (idx & 8) >> 3 != (idx & 1) {
                ex[3] = x;
                ey[3] = y + t(a, d) * dy;
                cross.push(3);
            }
            let pairs = if cross.len() == 4 {
                let high = (a + b + c + d) / 4.0 > level;
                // a high centre joins the high corners, so the lines cut off the low ones
                if (idx == 5) == high {
                    vec![0, 1, 2, 3]
                } else {
                    vec![0, 3, 1, 2]
                }
            } else {
                cross
            };
            let mut k = 0;
            while k + 1 < pairs.len() {
                out.extend([
                    ex[pairs[k]],
                    ey[pairs[k]],
                    ex[pairs[k + 1]],
                    ey[pairs[k + 1]],
                ]);
                k += 2;
            }
        }
    }
    out
}

// liang–barsky against a box whose sides may be infinite
fn clip_segment(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    rx: &GraphRange,
    ry: &GraphRange,
) -> Option<[f64; 4]> {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let mut t0 = 0.0;
    let mut t1 = 1.0;
    for (p, q) in [
        (-dx, x1 - rx.lo),
        (dx, rx.hi - x1),
        (-dy, y1 - ry.lo),
        (dy, ry.hi - y1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            if r > t0 {
                t0 = r;
            }
        } else {
            if r < t0 {
                return None;
            }
            if r < t1 {
                t1 = r;
            }
        }
    }
    Some([x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy])
}

// each level's label at the point of its line farthest from the edges and the labels already placed
fn place_labels(contours: &mut [ContourLevel], domain: (f64, f64), y_scale: YScale) {
    let span = domain.1 - domain.0;
    // a label is about 4% of the plot wide and 2% tall
    let w = 0.04 * span;
    let h = 0.02 * span;
    let mut placed: Vec<LabelAt> = Vec::new();
    for c in contours.iter_mut() {
        let mut best = 0.0;
        let s = &c.segments;
        let mut k = 0;
        while k + 3 < s.len() {
            let x = (s[k] + s[k + 2]) / 2.0;
            let y = (s[k + 1] + s[k + 3]) / 2.0;
            let mut score = jmin(
                jmin(x - domain.0 - w, domain.1 - x - w),
                jmin(y - y_scale.min - h, y_scale.max - y - h),
            );
            for p in &placed {
                score = jmin(score, hypot((x - p.x) / 2.0, y - p.y) - 2.0 * h);
            }
            if score > best {
                best = score;
                c.at = Some(LabelAt { x, y });
            }
            k += 4;
        }
        if let Some(at) = c.at {
            placed.push(at);
        }
    }
}

// ---- ticks ----

#[derive(Clone, Debug, PartialEq)]
pub struct GraphTick {
    pub value: f64,
    pub label: String,
}

/// Plain axes step 1, 2 or 5 × 10ⁿ. An angle axis steps in multiples of π (rad) or 15° to 720° (deg) while those
/// stay readable, labelled `π/2`, `−2π`, `90°`.
pub fn graph_ticks(lo: f64, hi: f64, unit: Option<AngleMode>, target: f64) -> Vec<GraphTick> {
    let span = hi - lo;
    if !(span > 0.0) || !span.is_finite() {
        return Vec::new();
    }
    if let Some(angle) = angle_ticks(lo, hi, unit) {
        return angle;
    }
    let step = nice_step(span / target, true);
    let mut out = Vec::new();
    let mut k = (lo / step - 1e-9).ceil();
    while k * step <= hi + step * 1e-9 && out.len() < 12 {
        let value = precision(k * step, 12);
        out.push(GraphTick {
            value,
            label: with_minus(&format_number(value, 6.0)),
        });
        k += 1.0;
    }
    out
}

fn angle_ticks(lo: f64, hi: f64, unit: Option<AngleMode>) -> Option<Vec<GraphTick>> {
    let unit = unit?;
    let span = hi - lo;
    // at most 7 steps keeps labels about 90px apart in the panel
    let fits = |step: f64| span / step <= 7.0;
    if unit == AngleMode::Deg {
        let step = [15.0, 30.0, 45.0, 90.0, 180.0, 360.0, 720.0]
            .into_iter()
            .find(|&s| fits(s))?;
        if span / step < 2.0 {
            return None;
        }
        return Some(step_ticks(lo, hi, step, &|k| {
            format!(
                "{}{}°",
                if k * step < 0.0 { MINUS } else { "" },
                jsnum::to_string((k * step).abs())
            )
        }));
    }
    // steps of π/4 up to 8π
    let pi = std::f64::consts::PI;
    let quarter = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0]
        .into_iter()
        .find(|&q| fits((q * pi) / 4.0))?;
    if span / ((quarter * pi) / 4.0) < 2.0 {
        return None;
    }
    Some(step_ticks(lo, hi, (quarter * pi) / 4.0, &|k| {
        pi_label(k * quarter)
    }))
}

fn step_ticks(lo: f64, hi: f64, step: f64, label: &dyn Fn(f64) -> String) -> Vec<GraphTick> {
    let mut out = Vec::new();
    let mut k = (lo / step - 1e-9).ceil();
    while k * step <= hi + step * 1e-9 {
        out.push(GraphTick {
            value: k * step,
            label: label(k),
        });
        k += 1.0;
    }
    out
}

/// `quarters`·π/4 as `0`, `π/4`, `−3π/2`, `2π`.
fn pi_label(quarters: f64) -> String {
    if quarters == 0.0 {
        return "0".into();
    }
    let sign = if quarters < 0.0 { MINUS } else { "" };
    let mut num = quarters.abs();
    let mut den = 4.0;
    while den > 1.0 && num % 2.0 == 0.0 {
        num /= 2.0;
        den /= 2.0;
    }
    format!(
        "{sign}{}π{}",
        if num == 1.0 {
            String::new()
        } else {
            jsnum::to_string(num)
        },
        if den == 1.0 {
            String::new()
        } else {
            format!("/{}", jsnum::to_string(den))
        }
    )
}

// ---- JSON (the TS shapes, for golden replay and the FFI's escape hatch) ----

fn num_json(x: f64) -> Json {
    json!(x)
}

fn opt_num(x: Option<f64>) -> Json {
    x.map_or(Json::Null, num_json)
}

fn ranges_json(rs: &[GraphRange]) -> Json {
    Json::Array(
        rs.iter()
            .map(|r| json!({"lo": num_json(r.lo), "hi": num_json(r.hi)}))
            .collect(),
    )
}

pub fn intent_json(i: &GraphIntent) -> Json {
    let mut o = Map::new();
    o.insert("expression".into(), json!(i.expression));
    o.insert("label".into(), json!(i.label));
    if let Some(d) = &i.function_def {
        o.insert(
            "functionDef".into(),
            json!({"name": d.name, "params": d.params, "body": d.body}),
        );
    }
    if let Some(c) = i.contour {
        o.insert(
            "contour".into(),
            json!(if c == ContourKind::Zero {
                "zero"
            } else {
                "levels"
            }),
        );
    }
    if let Some(b) = &i.bounds {
        o.insert(
            "bounds".into(),
            json!({"x": ranges_json(&b.x), "y": ranges_json(&b.y)}),
        );
    }
    if let Some(e) = &i.bounds_error {
        o.insert("boundsError".into(), json!(e));
    }
    Json::Object(o)
}

pub fn points_json(points: &[GraphPoint]) -> Json {
    Json::Array(
        points
            .iter()
            .map(|p| json!({"x": num_json(p.x), "y": opt_num(p.y)}))
            .collect(),
    )
}

fn critical_json(cs: &[CriticalPoint]) -> Json {
    Json::Array(
        cs.iter()
            .map(|c| json!({"x": num_json(c.x), "y": num_json(c.y), "kind": c.kind.name()}))
            .collect(),
    )
}

fn mode_json(m: Option<AngleMode>) -> Json {
    match m {
        Some(AngleMode::Deg) => json!("deg"),
        Some(AngleMode::Rad) => json!("rad"),
        None => Json::Null,
    }
}

/// The TS `GraphResult` as JSON; the readout closures are `{"$fn": "anon"}`, as the recorder writes them.
pub fn result_json(r: &GraphResult) -> Json {
    let anon = json!({"$fn": "anon"});
    let mut o = Map::new();
    o.insert("intent".into(), intent_json(&r.intent));
    o.insert("points".into(), points_json(&r.points));
    o.insert("domain".into(), json!([r.domain.0, r.domain.1]));
    o.insert(
        "yScale".into(),
        json!({"min": num_json(r.y_scale.min), "max": num_json(r.y_scale.max)}),
    );
    o.insert("criticalPoints".into(), critical_json(&r.critical_points));
    o.insert(
        "roots".into(),
        Json::Array(r.roots.iter().map(|x| json!({"x": num_json(*x)})).collect()),
    );
    o.insert("angleUnit".into(), mode_json(r.angle_unit));
    o.insert("y".into(), anon.clone());
    if r.has_z() {
        o.insert("z".into(), anon);
    }
    if let Some(cs) = &r.contours {
        o.insert(
            "contours".into(),
            Json::Array(
                cs.iter()
                    .map(|c| {
                        json!({
                            "level": num_json(c.level),
                            "label": c.label,
                            "segments": c.segments,
                            "at": c.at.map_or(Json::Null, |a| json!({"x": a.x, "y": a.y})),
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(e) = &r.error {
        o.insert("error".into(), json!(e));
    }
    Json::Object(o)
}

fn ticks_json(ts: &[GraphTick]) -> Json {
    Json::Array(
        ts.iter()
            .map(|t| json!({"value": num_json(t.value), "label": t.label}))
            .collect(),
    )
}

/// `GraphOptions` from the TS JSON shape (recorder specials already decoded).
pub fn options_from_json(v: &Json) -> GraphOptions {
    let num = |k: &str| v.get(k).and_then(Json::as_f64);
    GraphOptions {
        domain: v
            .get("domain")
            .and_then(Json::as_array)
            .and_then(|d| Some((d.first()?.as_f64()?, d.get(1)?.as_f64()?))),
        sample_count: num("sampleCount"),
        variables: v
            .get("variables")
            .and_then(|x| serde_json::from_value(x.clone()).ok())
            .unwrap_or_default(),
        functions: v
            .get("functions")
            .and_then(|x| serde_json::from_value(x.clone()).ok())
            .unwrap_or_default(),
        angle_mode: v
            .get("angleMode")
            .and_then(|x| serde_json::from_value(x.clone()).ok()),
        ans: num("ans"),
        find_roots: v.get("findRoots").and_then(Json::as_bool),
        aspect: num("aspect"),
    }
}

fn points_from_json(v: Option<&Json>) -> Vec<GraphPoint> {
    v.and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    Some(GraphPoint {
                        x: p.get("x")?.as_f64()?,
                        y: p.get("y").and_then(Json::as_f64),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Golden replay (and the FFI's JSON calls) of `engine/graph`.
pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    let args: Vec<Json> = args.iter().map(crate::golden::decode).collect();
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or("").to_string();
    let n = |i: usize| args.get(i).and_then(Json::as_f64);
    let opts = |i: usize| args.get(i).map(options_from_json).unwrap_or_default();
    Some(match name {
        "isGraphCommand" => json!(is_graph_command(&s(0))),
        "parseGraphIntent" => {
            let o = opts(1);
            parse_graph_intent(&s(0), &o.functions, &o.variables)
                .map_or(Json::Null, |i| intent_json(&i))
        }
        "buildGraph" => build_graph(&s(0), &opts(1)).map_or(Json::Null, |r| result_json(&r)),
        "evaluateGraphY" => opt_num(evaluate_graph_y(&s(0), n(1)?, &opts(2))),
        "evaluateGraphZ" => opt_num(evaluate_graph_z(&s(0), n(1)?, n(2)?, &opts(3))),
        "compileGraphY" | "compileGraphZ" => json!({"$fn": "anon"}),
        "sampleGraph" => points_json(&sample_graph(&s(0), &opts(1))),
        "graphTicks" => {
            let unit = args
                .get(2)
                .and_then(|u| serde_json::from_value::<AngleMode>(u.clone()).ok());
            ticks_json(&graph_ticks(n(0)?, n(1)?, unit, n(3).unwrap_or(5.0)))
        }
        "graphHome" => {
            let (lo, hi) = graph_home(&s(0), &opts(1));
            json!([lo, hi])
        }
        "autoYScale" => {
            if args
                .get(1)
                .is_some_and(|o| o.as_object().is_some_and(|o| !o.is_empty()))
            {
                return None;
            }
            let y = auto_y_scale(&points_from_json(args.first()));
            json!({"min": y.min, "max": y.max})
        }
        "findCriticalPoints" if args.len() == 1 => {
            critical_json(&find_critical_points(&points_from_json(args.first()), None))
        }
        "findRoots" if args.len() == 1 => Json::Array(
            find_roots(&points_from_json(args.first()), None)
                .into_iter()
                .map(|x| json!({"x": x}))
                .collect(),
        ),
        "marchingSquares" => {
            let grid: Vec<f64> = args
                .first()?
                .as_array()?
                .iter()
                .map(|v| v.as_f64().unwrap_or(f64::NAN))
                .collect();
            json!(marching_squares(
                &grid,
                n(1)? as usize,
                n(2)? as usize,
                n(3)?,
                n(4)?,
                n(5)?,
                n(6)?,
                n(7)?
            ))
        }
        "withMinus" => json!(with_minus(&s(0))),
        // native only: `buildGraph(input, options).y(x)` and `.z(x, y)` without sampling
        "graphReadoutY" => opt_num(readout_y(&s(0), &opts(2), n(1)?)),
        "graphReadoutZ" => opt_num(readout_z(&s(0), &opts(3), n(1)?, n(2)?)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readouts_follow_the_bounds() {
        let o = GraphOptions::default();
        assert_eq!(readout_y("graph x^2 {0 < x < 3}", &o, 2.0), Some(4.0));
        assert_eq!(readout_y("graph x^2 {0 < x < 3}", &o, 5.0), None);
        assert_eq!(readout_z("graph x^2 + y^2", &o, 1.0, 2.0), Some(5.0));
        assert_eq!(readout_y("graph x^2 + y^2", &o, 1.0), None);
        let g = build_graph("graph (x-1)(x+2)", &o).unwrap();
        assert_eq!(g.roots, vec![-2.0, 1.0]);
        assert_eq!(g.y_at(3.0), Some(10.0));
    }

    #[test]
    fn ticks() {
        let labels: Vec<String> = graph_ticks(-1.0, 1.0, None, 5.0)
            .into_iter()
            .map(|t| t.label)
            .collect();
        assert_eq!(labels, ["−1", "−0.5", "0", "0.5", "1"]);
    }
}
