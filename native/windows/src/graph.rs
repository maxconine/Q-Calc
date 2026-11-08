//! The graph panel under the composer: `graph x^3`, `graph f(x) = …`, `plot …`, `graph a; b; c`. Spec: the Mac's
//! GraphModel.swift / GraphPanel.swift / GraphSettings.swift (native only: implicit curves, surfaces in 3D, key
//! points, several equations and the graph settings), which grew out of the web's GraphPanel.tsx.
//!
//! - State (`GraphState`, `BarModel::graph`): the x window (opening on `graph_home`, with the y centre it gives
//!   for a plot of x and y), the equations after the first (`layers`: those after a `;` in the bar, then those
//!   added with the head's +), what the settings set for this graph (axis labels, a fixed y or z window, steps,
//!   log axes), the 3D view's turn and tilt, and the newest `Sample`. Nothing exists while the line isn't a graph
//!   command, so the panel costs nothing closed; `follow` (from `evaluate_live`) is a prefix test on a keystroke.
//! - Sampling (`sample`): `graph_home` (once per expression), `graph` for each equation over the first's window
//!   (a plot of x and y again over the final y window), then `graph_ticks`, all in one job on `Lane::Graph`, so a
//!   burst of pans or zooms samples only the newest window and an older result never lands. A surface opens in 3D
//!   and is sampled again as a square with its grid.
//! - Drawing and the pointer: graph_view.rs (the base pass cached as a bitmap, the pointer's pass over it), and
//!   graph3d.rs (the cube). The graph's settings: graph_settings.rs, kept in settings.json's `graphPrefs`.
//!
//! Hooks: model.rs (`graph` field, `follow` in `evaluate_live`, `escape`, `key_down` and `char16`), window.rs
//! (`height` in `Bar::layout`, `paint` and `focused` in `Bar::paint`), app.rs (`click`, `press`, `mouse_move`,
//! `mouse_up`, `wheel`, `leave`, `cursor`, `release` in `release_hidden`), store.rs (`graph_prefs`), bench.rs
//! (`bench`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use qcalc_ffi::{
    AngleUnit, ContourLevel, CriticalKind, CriticalPoint, Engine, EvalOptions, GraphOptions,
    GraphPoint, GraphResult, GraphTick, SurfaceGrid, merged_options,
};

use crate::engine::{EngineThread, Lane, done};
use crate::graph_settings::{self as gs, FieldId};
use crate::graph3d::{HOME_PITCH, HOME_YAW, PITCH_MAX, PITCH_MIN};
use crate::input::Field;
use crate::keys::Mods;
use crate::model::{BarModel, Outcome, now_ms};

pub use crate::graph_view::{
    bench, click, cursor, focused, height, leave, mouse_move, mouse_up, paint, press, release,
    wheel,
};

/// The web's viewBox: a 652 × 200 plot with padding 12 / 12 / 28 / 40, scaled to the panel's width; the 3D view
/// is 360 tall so a surface has room to tilt.
pub const SVG_W: f32 = 652.0;
pub const SVG_H: f32 = 200.0;
pub const SVG_H3D: f32 = 360.0;
pub const PAD_TOP: f32 = 12.0;
pub const PAD_RIGHT: f32 = 12.0;
pub const PAD_BOTTOM: f32 = 28.0;
pub const PAD_LEFT: f32 = 40.0;
pub const PLOT_W: f32 = SVG_W - PAD_LEFT - PAD_RIGHT;
pub const PLOT_H: f32 = SVG_H - PAD_TOP - PAD_BOTTOM;
/// PLOT_ASPECT: a plot of x and y keeps them at one scale
pub const PLOT_ASPECT: f64 = (PLOT_H / PLOT_W) as f64;
const MIN_SPAN: f64 = 1e-6;
const MAX_SPAN: f64 = 1e6;
/// viewBox units within which the pointer picks out a mark
pub const SNAP: f32 = 9.0;
/// far off-plot samples are pinned this many plot heights away (the clip hides them)
const V_LIMIT: f32 = 60.0;

// ---- pure logic ----

/// `graph <body>` / `plot <body>` (BarCommand.graphRE on the trimmed line, runs of spaces as one): the body.
pub fn command(text: &str) -> Option<String> {
    let t = text.trim();
    let rest = ["graph", "plot"].iter().find_map(|w| {
        let head = t.get(..w.len())?;
        let rest = &t[w.len()..];
        (head.eq_ignore_ascii_case(w) && rest.starts_with(char::is_whitespace)).then_some(rest)
    })?;
    let body = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    (!body.is_empty()).then_some(body)
}

/// `graph a; b; c` (splitGraphs): the first equation and the rest, split at the `;`s outside brackets; empty ones
/// after the first are dropped.
pub fn split_graphs(body: &str) -> Vec<String> {
    let (mut parts, mut depth, mut cur) = (Vec::new(), 0usize, String::new());
    for ch in body.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if ch == ';' && depth == 0 {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    parts.push(cur);
    let mut out = vec![parts[0].trim().to_string()];
    out.extend(
        parts[1..]
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
    );
    out
}

/// clampDomain: ordered, finite, between 1e-6 and 1e6 wide.
pub fn clamp_domain(lo: f64, hi: f64) -> (f64, f64) {
    if !lo.is_finite() || !hi.is_finite() {
        return (-10.0, 10.0);
    }
    let (mut lo, mut hi) = if lo > hi { (hi, lo) } else { (lo, hi) };
    let span = hi - lo;
    if span < MIN_SPAN {
        let mid = (lo + hi) / 2.0;
        lo = mid - MIN_SPAN / 2.0;
        hi = mid + MIN_SPAN / 2.0;
    } else if span > MAX_SPAN {
        let mid = (lo + hi) / 2.0;
        lo = mid - MAX_SPAN / 2.0;
        hi = mid + MAX_SPAN / 2.0;
    }
    (lo, hi)
}

/// `lo…hi` moved by `by` of its width (a log axis by that much of its decades).
pub fn shifted(lo: f64, hi: f64, by: f64, log: bool) -> (f64, f64) {
    if log && lo > 0.0 && hi > lo {
        let (a, b) = (lo.log10(), hi.log10());
        let d = by * (b - a);
        return (10f64.powf(a + d), 10f64.powf(b + d));
    }
    let d = by * (hi - lo);
    (lo + d, hi + d)
}

/// `lo…hi` scaled by `factor` about `at` of the way across (a log axis in its decades).
pub fn scaled(lo: f64, hi: f64, factor: f64, at: f64, log: bool) -> (f64, f64) {
    if log && lo > 0.0 && hi > lo {
        let (a, b) = (lo.log10(), hi.log10());
        let w = (b - a) * factor;
        let pivot = a + (b - a) * at;
        return (10f64.powf(pivot - w * at), 10f64.powf(pivot - w * at + w));
    }
    let w = (hi - lo) * factor;
    let pivot = lo + (hi - lo) * at;
    (pivot - w * at, pivot - w * at + w)
}

/// `%g` with `sig` significant digits, a real minus sign, no `-0`: the panel's numbers (formatCoord).
pub fn fmt_g(v: f64, sig: usize) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let sig = sig.max(1);
    if v == 0.0 {
        return "0".into();
    }
    let sci = format!("{:.*e}", sig - 1, v);
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let out = if exp < -4 || exp >= sig as i32 {
        format!("{}e{exp}", trim_zeros(mant))
    } else {
        let decimals = (sig as i32 - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{v:.decimals$}")).to_string()
    };
    let out = if out == "-0" { "0".to_string() } else { out };
    out.replace('-', "−")
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

/// A coordinate in the readouts (`%.4g`, nothing under 1e-9).
pub fn fmt4(v: f64) -> String {
    fmt_g(if v.abs() < 1e-9 { 0.0 } else { v }, 4)
}

/// A coordinate on a chip (formatCoord: 8 figures, nothing under 1e-12).
pub fn coord(v: f64) -> String {
    fmt_g(if v.abs() < 1e-12 { 0.0 } else { v }, 8)
}

/// roundToPixel: `v` to the finest decimal a pixel `res` wide can tell apart.
pub fn round_to_pixel(v: f64, res: f64) -> f64 {
    if res.is_nan() || res <= 0.0 || !res.is_finite() || !v.is_finite() {
        return v;
    }
    let step = 10f64.powf(res.log10().floor());
    let r = (v / step).round() * step;
    format!("{r:.11e}").parse().unwrap_or(r)
}

/// An axis value as the plot draws it: log10 on a log axis (nothing at or below 0).
pub fn axis_t(v: f64, log: bool) -> f64 {
    if !log {
        v
    } else if v > 0.0 {
        v.log10()
    } else {
        f64::NAN
    }
}

/// Polylines and dots in a sample's unit square (u across the x window, v down the y window, each in log10 on a
/// log axis). A run breaks at an undefined sample and where one step crosses the whole view (a pole, as tan and
/// 1/x have), as pointsToPath does.
pub type Runs = (Vec<Vec<(f32, f32)>>, Vec<(f32, f32)>);

pub fn runs(points: &[GraphPoint], window: [f64; 4], log: (bool, bool)) -> Runs {
    let (x0, x1) = (axis_t(window[0], log.0), axis_t(window[1], log.0));
    let (y0, y1) = (axis_t(window[2], log.1), axis_t(window[3], log.1));
    let (xw, yh) = (x1 - x0, y1 - y0);
    let mut lines = Vec::new();
    let mut dots = Vec::new();
    let mut run: Vec<(f32, f32)> = Vec::new();
    let flush = |run: &mut Vec<(f32, f32)>,
                 lines: &mut Vec<Vec<(f32, f32)>>,
                 dots: &mut Vec<(f32, f32)>| {
        match run.len() {
            0 => {}
            1 => dots.push(run[0]),
            _ => lines.push(std::mem::take(run)),
        }
        run.clear();
    };
    for p in points {
        let u = ((axis_t(p.x, log.0) - x0) / xw) as f32;
        let v = p.y.map(|y| ((y1 - axis_t(y, log.1)) / yh) as f32);
        let Some(v) = v.filter(|v| v.is_finite() && u.is_finite()) else {
            flush(&mut run, &mut lines, &mut dots);
            continue;
        };
        if let Some(&(_, last)) = run.last()
            && ((last < 0.0 && v > 1.0) || (last > 1.0 && v < 0.0))
        {
            flush(&mut run, &mut lines, &mut dots);
        }
        run.push((u, v.clamp(-V_LIMIT, V_LIMIT)));
    }
    flush(&mut run, &mut lines, &mut dots);
    (lines, dots)
}

/// Chips laid out left to right, wrapping at `max_w`: each one's (x, line).
pub fn wrap(widths: &[f32], max_w: f32, gap: f32) -> Vec<(f32, usize)> {
    let mut out = Vec::with_capacity(widths.len());
    let (mut x, mut line) = (0.0f32, 0usize);
    for &w in widths {
        if x > 0.0 && x + w > max_w {
            x = 0.0;
            line += 1;
        }
        out.push((x, line));
        x += w + gap;
    }
    out
}

pub fn kind_name(k: CriticalKind) -> &'static str {
    match k {
        CriticalKind::Min => "min",
        CriticalKind::Max => "max",
        CriticalKind::Critical => "critical",
        CriticalKind::XIntercept => "x-intercept",
        CriticalKind::YIntercept => "y-intercept",
        CriticalKind::Saddle => "saddle",
        CriticalKind::VerticalTangent => "vertical tangent",
        CriticalKind::Point => "point",
        CriticalKind::Endpoint => "endpoint",
        CriticalKind::OpenEndpoint => "open end",
        CriticalKind::Intersection => "intersection",
    }
}

/// Each equation's colour (layerRGB): the first in the accent (None), the rest from a palette that reads on both
/// backgrounds: red, blue, purple, orange, teal, grey.
pub fn layer_rgb(i: usize) -> Option<(f32, f32, f32)> {
    const PALETTE: [(f32, f32, f32); 6] = [
        (0.86, 0.30, 0.27),
        (0.25, 0.52, 0.86),
        (0.62, 0.42, 0.86),
        (0.93, 0.58, 0.16),
        (0.20, 0.70, 0.72),
        (0.60, 0.60, 0.62),
    ];
    (i > 0).then(|| PALETTE[(i - 1) % PALETTE.len()])
}

/// The tick rule (graphTicks'): steps of 1, 2 or 5 × 10ⁿ, about `count` across, the origin always one of them.
pub fn nice_ticks(lo: f64, hi: f64, count: usize) -> Vec<f64> {
    let raw = (hi - lo) / count.max(1) as f64;
    if !(raw > 0.0 && raw.is_finite()) {
        return Vec::new();
    }
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let step = if f < 1.5 {
        1.0
    } else if f < 3.5 {
        2.0
    } else if f < 7.5 {
        5.0
    } else {
        10.0
    } * mag;
    let mut out = Vec::new();
    let mut v = (lo / step).ceil() * step;
    while v <= hi + step * 1e-9 && out.len() < 40 {
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    out
}

/// One axis's grid lines: the labelled values, and the minor ones between.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisGrid {
    pub major: Vec<(f64, String)>,
    pub minor: Vec<f64>,
}

impl AxisGrid {
    pub fn step(&self) -> Option<f64> {
        (self.major.len() >= 2).then(|| self.major[1].0 - self.major[0].0)
    }
}

/// A tick's label as the page draws it: no grouping, a real minus.
pub fn tick_text(label: &str) -> String {
    label.replace('-', "−")
}

/// axisGrid: the step set in the settings, else decades on a log axis, else the engine's ticks; minor lines a
/// fifth of a step (a quarter of a 2), or 2…9 of each decade.
pub fn axis_grid(lo: f64, hi: f64, log: bool, step: Option<f64>, engine: &[GraphTick]) -> AxisGrid {
    let mut out = AxisGrid::default();
    if !(hi > lo && lo.is_finite() && hi.is_finite()) {
        return out;
    }
    let subdivide = |out: &mut AxisGrid, base: f64, step: f64| {
        let mant = step / 10f64.powf(step.log10().floor());
        let sub = if (mant - 2.0).abs() < 0.01 { 4 } else { 5 };
        let ms = step / f64::from(sub);
        if ms <= 0.0 || (hi - lo) / ms > 1000.0 {
            return;
        }
        let mut m = ((lo - base) / ms).ceil();
        while base + m * ms <= hi {
            if (m.round() as i64).rem_euclid(i64::from(sub)) != 0 {
                out.minor.push(base + m * ms);
            }
            m += 1.0;
        }
    };
    if let Some(step) = step.filter(|s| *s > 0.0 && (hi - lo) / s <= 200.0) {
        let mut k = (lo / step).ceil();
        while k * step <= hi + step * 1e-9 {
            let v = if (k * step).abs() < step * 1e-9 {
                0.0
            } else {
                k * step
            };
            out.major.push((v, fmt_g(v, 6)));
            k += 1.0;
        }
        subdivide(&mut out, 0.0, step);
    } else if log && lo > 0.0 {
        let (a, b) = (lo.log10().floor() as i32, hi.log10().ceil() as i32);
        let every = ((b - a) / 8).max(1);
        for k in a..=b {
            let v = 10f64.powi(k);
            if v >= lo * (1.0 - 1e-9) && v <= hi * (1.0 + 1e-9) && (k - a) % every == 0 {
                out.major.push((v, fmt_g(v, 6)));
            }
            if b - a <= 12 {
                for m in 2..=9 {
                    let mv = f64::from(m) * v;
                    if mv >= lo && mv <= hi {
                        out.minor.push(mv);
                    }
                }
            }
        }
    } else {
        out.major = engine
            .iter()
            .filter(|t| t.value >= lo && t.value <= hi)
            .map(|t| (t.value, tick_text(&t.label)))
            .collect();
        if let Some(step) = out.step().filter(|s| *s > 0.0) {
            let base = out.major[0].0;
            subdivide(&mut out, base, step);
        }
    }
    out
}

/// The mark nearest `p` within `snap` (viewBox units), if any.
pub fn nearest(marks: &[Option<(f32, f32)>], p: (f32, f32), snap: f32) -> Option<usize> {
    marks
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let m = (*m)?;
            let d = (m.0 - p.0).hypot(m.1 - p.1);
            (d <= snap).then_some((i, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

// ---- state ----

static SERIAL: AtomicU64 = AtomicU64::new(1);

/// What was graphed: a curve in x, an implicit curve (`y^2 = 4x`) or a surface (`z = x^2 - y^2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Curve,
    Implicit,
    Surface,
}

/// One equation as the engine graphed it.
#[derive(Debug)]
pub struct Plot {
    pub label: String,
    pub error: Option<String>,
    pub points: Vec<GraphPoint>,
    pub roots: Vec<f64>,
    pub critical: Vec<CriticalPoint>,
    pub contours: Vec<ContourLevel>,
    pub kind: Kind,
    pub surface: Option<SurfaceGrid>,
    pub unit: Option<AngleUnit>,
}

impl Plot {
    fn new(r: GraphResult) -> Plot {
        Plot {
            label: r.label,
            error: r.error,
            points: r.points,
            roots: r.roots,
            critical: r.critical_points,
            contours: r.contours,
            kind: if !r.contour {
                Kind::Curve
            } else if r.contour_levels {
                Kind::Surface
            } else {
                Kind::Implicit
            },
            surface: r.surface,
            unit: r.angle_unit,
        }
    }

    pub fn ok(&self) -> bool {
        self.error.is_none()
    }
}

/// What the engine made for one window.
#[derive(Debug)]
pub struct Sample {
    pub serial: u64,
    /// x_min, x_max, y_min, y_max it was sampled over
    pub window: [f64; 4],
    /// (x, y) on log scales when sampled
    pub log: (bool, bool),
    /// the bar's equation; None when the engine couldn't graph it at all
    pub main: Option<Plot>,
    /// the other equations by id; None for one the engine couldn't graph
    pub layers: Vec<(u32, Option<Plot>)>,
    pub x_ticks: Vec<GraphTick>,
    pub y_ticks: Vec<GraphTick>,
}

impl Sample {
    pub fn layer(&self, id: u32) -> Option<&Plot> {
        self.layers.iter().find(|l| l.0 == id)?.1.as_ref()
    }
}

/// One more equation on the graph (GraphLayer), sampled over the same window as the first.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: u32,
    pub expression: String,
    /// after a `;` in the bar (edited there), rather than added in the panel
    pub from_bar: bool,
}

impl Layer {
    pub fn input(&self) -> String {
        format!("graph {}", self.expression)
    }
    pub fn blank(&self) -> bool {
        self.expression.trim().is_empty()
    }
}

/// What the pointer reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    /// a curve plot at the pointer's x: each curve's y there (0 the bar's equation, n the nth layer)
    Curve { x: f64, ys: Vec<(usize, f64)> },
    /// a plot of x and y: the point under the pointer, and a surface's height there
    Point { x: f64, y: f64, z: Option<f64> },
    /// the 3D view: the point of a surface under the pointer
    Surface {
        x: f64,
        y: f64,
        z: f64,
        layer: usize,
    },
}

/// A press being dragged over the plot.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    /// where the pointer was last, panel DIPs
    pub last: (f32, f32),
    /// the plot's size in DIPs, for the fractions a drag moves by
    pub plot: (f32, f32),
    /// shift, alt or the middle button: pan rather than orbit
    pub pan: bool,
    pub moved: bool,
    /// the left button (a click copies a mark); else the right or middle
    pub left: bool,
}

/// The panel's state (`BarModel::graph`).
pub struct GraphState {
    pub expression: String,
    pub layers: Vec<Layer>,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub home: Option<(f64, f64)>,
    /// the y a plot of x and y centres on, from `graph_home`
    pub y_center: Option<f64>,
    pub home_y_center: Option<f64>,
    /// the 2D plot's y window when set (the settings, Zoom Square); None fits y to the curves
    pub fixed_y: Option<(f64, f64)>,
    /// the 3D cube's z when set
    pub fixed_z: Option<(f64, f64)>,
    pub x_label: String,
    pub y_label: String,
    pub x_step: Option<f64>,
    pub y_step: Option<f64>,
    pub x_log: bool,
    pub y_log: bool,
    /// a surface in 3D (the default) or as level curves; None until the engine says it's a surface
    pub view3d: Option<bool>,
    pub yaw: f64,
    pub pitch: f64,
    pub sample: Option<Rc<Sample>>,
    /// the settings block is open (the head's wrench)
    pub settings_open: bool,
    /// a field in the panel with the keys, and its text
    pub focus: Option<FieldId>,
    pub edit: Field,
    /// the pointer over the panel, panel DIPs
    pub hover: Option<(f32, f32)>,
    /// the pointer over the plot, viewBox units
    pub pointer: Option<(f32, f32)>,
    pub drag: Option<Drag>,
    pub reading: Option<Reading>,
    /// the calculator's angle mode is radians (the settings' angles follow it until set)
    pub radians: bool,
    /// bumped by everything that changes what the panel lays out or the plot's base pass draws
    pub generation: u64,
    pub cache: RefCell<crate::graph_view::Cache>,
}

impl GraphState {
    fn new(expression: String, carry: Option<GraphState>) -> GraphState {
        let mut g = GraphState {
            expression,
            layers: Vec::new(),
            x_min: -10.0,
            x_max: 10.0,
            y_min: -10.0,
            y_max: 10.0,
            home: None,
            y_center: None,
            home_y_center: None,
            fixed_y: None,
            fixed_z: None,
            x_label: String::new(),
            y_label: String::new(),
            x_step: None,
            y_step: None,
            x_log: false,
            y_log: false,
            view3d: None,
            yaw: HOME_YAW,
            pitch: HOME_PITCH,
            sample: None,
            settings_open: false,
            focus: None,
            edit: Field::default(),
            hover: None,
            pointer: None,
            drag: None,
            reading: None,
            radians: false,
            generation: 0,
            cache: RefCell::default(),
        };
        // a new first equation keeps the old picture until its own sample lands (so the panel doesn't jump), the
        // panel's equations and the axis settings
        if let Some(o) = carry {
            (g.x_min, g.x_max, g.y_min, g.y_max) = (o.x_min, o.x_max, o.y_min, o.y_max);
            g.sample = o.sample;
            g.layers = o.layers.into_iter().filter(|l| !l.from_bar).collect();
            (g.x_label, g.y_label, g.x_step, g.y_step, g.x_log, g.y_log) =
                (o.x_label, o.y_label, o.x_step, o.y_step, o.x_log, o.y_log);
            g.settings_open = o.settings_open;
            g.generation = o.generation + 1;
        }
        g
    }

    pub fn input(&self) -> String {
        format!("graph {}", self.expression)
    }

    /// Something the layout or the plot's base pass reads changed.
    pub fn bump(&mut self) {
        self.generation += 1;
    }

    pub fn main(&self) -> Option<&Plot> {
        self.sample.as_ref()?.main.as_ref()
    }

    pub fn failed(&self) -> bool {
        self.sample.as_ref().is_some_and(|s| s.main.is_none())
    }

    pub fn kind(&self) -> Kind {
        self.main().map_or(Kind::Curve, |p| p.kind)
    }

    pub fn shows_3d(&self) -> bool {
        self.kind() == Kind::Surface && self.view3d == Some(true)
    }

    pub fn log_x(&self) -> bool {
        self.x_log && self.x_min > 0.0 && self.x_max > self.x_min
    }

    pub fn log_y(&self) -> bool {
        self.y_log && self.y_min > 0.0 && self.y_max > self.y_min
    }

    pub fn at_home(&self) -> bool {
        self.home
            .is_none_or(|(lo, hi)| self.x_min == lo && self.x_max == hi)
            && self.y_center == self.home_y_center
            && self.fixed_y.is_none()
            && self.fixed_z.is_none()
            && (!self.shows_3d() || (self.yaw == HOME_YAW && self.pitch == HOME_PITCH))
    }

    /// The 3D cube's z window.
    pub fn z_window(&self) -> (f64, f64) {
        let surface = self
            .main()
            .and_then(|p| p.surface.as_ref())
            .map(|s| (s.z_lo, s.z_hi));
        crate::graph3d::z_window(self.fixed_z, (self.x_min, self.x_max), surface)
    }

    /// The 3D view's cube for a plot `size` (viewBox units) wide and tall.
    pub fn cube(&self, size: (f32, f32)) -> Option<crate::graph3d::Cube> {
        let grid = self.main().and_then(|p| p.surface.as_ref());
        let x = grid.map_or((self.x_min, self.x_max), |s| (s.x_lo, s.x_hi));
        let (yc, half) = (
            self.y_center.unwrap_or(0.0),
            (self.x_max - self.x_min) / 2.0,
        );
        let y = grid.map_or((yc - half, yc + half), |s| (s.y_lo, s.y_hi));
        crate::graph3d::Cube::new(x, y, self.z_window(), self.yaw, self.pitch, size)
    }

    pub fn label(&self) -> String {
        match self.main() {
            Some(p) if !p.label.is_empty() => p.label.clone(),
            _ => self.expression.replace("sqrt(", "√("),
        }
    }

    pub fn deg(&self) -> &'static str {
        match self.main().and_then(|p| p.unit) {
            Some(AngleUnit::Deg) => "°",
            _ => "",
        }
    }

    /// A layer's plot, when the engine graphed it without an error.
    pub fn layer_plot(&self, l: &Layer) -> Option<&Plot> {
        if l.blank() {
            return None;
        }
        self.sample.as_ref()?.layer(l.id).filter(|p| p.ok())
    }

    /// Every equation's critical points, the bar's first, each with its equation's number (0 the bar's), which
    /// colours its mark and chip (allCritical); on the 3D view only surfaces' points count.
    pub fn all_critical(&self) -> Vec<(usize, CriticalPoint)> {
        let three = self.shows_3d();
        let mut out: Vec<(usize, CriticalPoint)> = self
            .main()
            .filter(|p| p.ok())
            .map(|p| p.critical.iter().map(|c| (0, *c)).collect())
            .unwrap_or_default();
        for (i, l) in self.layers.iter().enumerate() {
            if let Some(p) = self.layer_plot(l)
                && (!three || p.surface.is_some())
            {
                out.extend(p.critical.iter().map(|c| (i + 1, *c)));
            }
        }
        out
    }

    /// The heading over the chips: what the list holds for this kind of graph.
    pub fn detail_title(&self) -> &'static str {
        match self.kind() {
            Kind::Surface => "Critical points",
            Kind::Implicit => "Key points",
            Kind::Curve => {
                let points = self
                    .main()
                    .is_some_and(|p| p.critical.iter().any(|c| c.kind == CriticalKind::Point));
                if points { "Points" } else { "Critical points" }
            }
        }
    }

    /// The bar's roots that no x-intercept chip already lists (a line's x-intercept is its root: it shows once,
    /// as a chip).
    pub fn roots(&self) -> Vec<f64> {
        let Some(p) = self.main().filter(|p| p.ok() && p.kind == Kind::Curve) else {
            return Vec::new();
        };
        p.roots
            .iter()
            .copied()
            .filter(|&r| {
                !p.critical.iter().any(|c| {
                    c.kind == CriticalKind::XIntercept && (c.x - r).abs() <= 1e-9 * r.abs().max(1.0)
                })
            })
            .collect()
    }

    /// The text a field starts from when it takes the keys.
    fn field_text(&self, id: FieldId) -> String {
        let y = if self.shows_3d() {
            self.z_window()
        } else {
            (self.y_min, self.y_max)
        };
        match id {
            FieldId::XLabel => self.x_label.clone(),
            FieldId::YLabel => self.y_label.clone(),
            FieldId::XLo => gs::show(Some(self.x_min)),
            FieldId::XHi => gs::show(Some(self.x_max)),
            FieldId::YLo => gs::show(Some(y.0)),
            FieldId::YHi => gs::show(Some(y.1)),
            FieldId::XStep => gs::show(self.x_step),
            FieldId::YStep => gs::show(self.y_step),
            FieldId::Layer(id) => self
                .layers
                .iter()
                .find(|l| l.id == id)
                .map_or(String::new(), |l| l.expression.clone()),
        }
    }

    /// What the settings block shows.
    pub fn settings_view(&self, radians: bool) -> gs::View {
        let three_d = self.shows_3d();
        gs::View {
            three_d,
            x_label: self.x_label.clone(),
            y_label: self.y_label.clone(),
            x: (self.x_min, self.x_max),
            y: if three_d {
                self.z_window()
            } else {
                (self.y_min, self.y_max)
            },
            x_step: self.x_step,
            y_step: self.y_step,
            log: (self.x_log, self.y_log),
            radians,
        }
    }
}

// ---- following the line, sampling ----

/// The line changed (or its context did): open, keep, re-sample or close the panel. On a keystroke that isn't a
/// graph command this is a prefix test.
pub fn follow(m: &mut BarModel, engine: &EngineThread) {
    let Some(body) = command(m.field.text()) else {
        if m.graph.take().is_some() {
            engine.cancel(Lane::Graph);
            engine.cancel(Lane::GraphReadout);
            m.touch();
        }
        return;
    };
    let parts = split_graphs(&body);
    let (primary, extras) = (parts[0].clone(), &parts[1..]);
    let radians = m.settings.angle_mode == "rad";
    if let Some(g) = &mut m.graph
        && g.radians != radians
    {
        g.radians = radians;
        g.bump();
    }
    if let Some(g) = &mut m.graph
        && g.expression == primary
    {
        let old: Vec<&str> = g
            .layers
            .iter()
            .filter(|l| l.from_bar)
            .map(|l| l.expression.as_str())
            .collect();
        if old != extras {
            g.layers = bar_layers(g, extras);
            g.bump();
            m.touch();
        }
        sample(m, engine, false);
        return;
    }
    let old = m.graph.take();
    let mut g = GraphState::new(primary, old);
    g.layers = bar_layers(&g, extras);
    g.radians = radians;
    m.graph = Some(g);
    m.touch();
    sample(m, engine, true);
}

/// The layers after a `;` in the bar (a layer typed on keeps its id), then those added in the panel.
fn bar_layers(g: &GraphState, extras: &[String]) -> Vec<Layer> {
    let old: Vec<&Layer> = g.layers.iter().filter(|l| l.from_bar).collect();
    let mut next = g.layers.iter().map(|l| l.id).max().unwrap_or(0) + 1;
    let mut out: Vec<Layer> = extras
        .iter()
        .enumerate()
        .map(|(i, e)| match old.get(i) {
            Some(l) => Layer {
                expression: e.clone(),
                ..(*l).clone()
            },
            None => {
                next += 1;
                Layer {
                    id: next - 1,
                    expression: e.clone(),
                    from_bar: true,
                }
            }
        })
        .collect();
    out.extend(g.layers.iter().filter(|l| !l.from_bar).cloned());
    out
}

/// What one sampling asks for.
struct Job {
    input: String,
    expr: String,
    layers: Vec<(u32, String)>,
    lo: f64,
    hi: f64,
    y_center: Option<f64>,
    fixed_y: Option<(f64, f64)>,
    in3d: bool,
    x_log: bool,
    y_log: bool,
    home: bool,
}

/// What came back.
struct Landed {
    expr: String,
    home: bool,
    lo: f64,
    hi: f64,
    y_center: Option<f64>,
    y: (f64, f64),
    sample: Sample,
}

/// The settings a graph is drawn with: the calculator's, with the graph's own degrees or radians.
pub fn graph_settings(options: EvalOptions, degrees: Option<bool>) -> EvalOptions {
    let mut s = options;
    if let Some(d) = degrees {
        s.angle_radians = !d;
    }
    s
}

/// buildGraph for the window as it is (and, for a new expression, its home window first), off the UI thread.
pub fn sample(m: &BarModel, engine: &EngineThread, home: bool) {
    let Some(g) = &m.graph else { return };
    let job = Job {
        input: g.input(),
        expr: g.expression.clone(),
        layers: g
            .layers
            .iter()
            .filter(|l| !l.blank())
            .map(|l| (l.id, l.input()))
            .collect(),
        lo: g.x_min,
        hi: g.x_max,
        y_center: g.y_center,
        fixed_y: g.fixed_y,
        in3d: g.shows_3d(),
        x_log: g.x_log,
        y_log: g.y_log,
        home,
    };
    let rows = m.rows.clone();
    let view = m.view(now_ms());
    let options = m.options();
    let degrees = gs::prefs().degrees;
    engine.submit(Lane::Graph, move |cx| {
        let context = cx.context(&rows, view);
        let settings = graph_settings(merged_options(options, context), degrees);
        let out = run(&cx.engine, job, settings);
        done(move |app| landed(&mut app.model, &app.engine, out))
    });
}

fn run(e: &Engine, job: Job, settings: EvalOptions) -> Landed {
    let (mut lo, mut hi, mut yc) = (job.lo, job.hi, job.y_center);
    if job.home {
        let w = e.graph_home(
            job.input.clone(),
            GraphOptions {
                domain_lo: None,
                domain_hi: None,
                sample_count: None,
                find_roots: true,
                aspect: Some(PLOT_ASPECT),
                y_center: None,
                surface: false,
                x_log: false,
            },
            settings.clone(),
        );
        if w.hi > w.lo {
            (lo, hi) = (w.lo, w.hi);
        }
        yc = w.y_center;
        if job.x_log && lo <= 0.0 {
            (lo, hi) = if hi > 0.0 {
                (hi / 1000.0, hi)
            } else {
                (0.1, 100.0)
            };
        }
    }
    let in3d = job.in3d;
    // the options for a y window: a plot of x and y samples exactly it
    let options = |y: Option<(f64, f64)>| {
        let (mut aspect, mut center) = (if in3d { 1.0 } else { PLOT_ASPECT }, yc);
        if !in3d
            && let Some(y) = y
            && hi > lo
        {
            aspect = (y.1 - y.0) / (hi - lo);
            center = Some((y.0 + y.1) / 2.0);
        }
        GraphOptions {
            domain_lo: Some(lo),
            domain_hi: Some(hi),
            sample_count: None,
            find_roots: true,
            aspect: Some(aspect),
            y_center: center,
            surface: in3d,
            x_log: job.x_log && lo > 0.0,
        }
    };
    // the 3D view samples a square of x and y
    let first = options(if in3d { None } else { job.fixed_y });
    let result = e.graph(job.input.clone(), first.clone(), settings.clone());
    let mut others: Vec<(u32, Option<GraphResult>)> = job
        .layers
        .iter()
        .map(|(id, input)| (*id, e.graph(input.clone(), first.clone(), settings.clone())))
        .collect();
    // y: as set; or fitted to every curve when the first is one (a plot of x and y keeps its own)
    let (mut y0, mut y1) = result
        .as_ref()
        .map_or((-10.0, 10.0), |r| (r.y_min, r.y_max));
    if let (Some(fy), false) = (job.fixed_y, in3d) {
        (y0, y1) = fy;
    } else if let Some(r) = result.as_ref().filter(|r| !r.contour && !in3d) {
        let curves = || {
            std::iter::once(r).chain(
                others
                    .iter()
                    .filter_map(|o| o.1.as_ref())
                    .filter(|o| !o.contour && o.error.is_none()),
            )
        };
        if job.y_log {
            let ys = curves()
                .flat_map(|c| c.points.iter().filter_map(|p| p.y))
                .filter(|y| *y > 0.0 && y.is_finite());
            let (a, b) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| {
                (a.min(y), b.max(y))
            });
            (y0, y1) = if a.is_finite() {
                if a < b {
                    (a / 1.5, b * 1.5)
                } else {
                    (a / 10.0, a * 10.0)
                }
            } else {
                (0.1, 100.0)
            };
        } else {
            for o in curves().skip(1).filter(|o| o.y_max > o.y_min) {
                y0 = y0.min(o.y_min);
                y1 = y1.max(o.y_max);
            }
        }
        // the others' plots of x and y again, over this y
        let again = options(Some((y0, y1)));
        for (id, o) in &mut others {
            if o.as_ref().is_some_and(|o| o.contour)
                && let Some((_, input)) = job.layers.iter().find(|l| l.0 == *id)
            {
                *o = e.graph(input.clone(), again.clone(), settings.clone());
            }
        }
    }
    let unit = result.as_ref().and_then(|r| r.angle_unit);
    let (x_ticks, y_ticks) = if result.is_some() {
        (
            e.graph_ticks(lo, hi, unit, 6),
            e.graph_ticks(y0, y1, None, 5),
        )
    } else {
        (Vec::new(), Vec::new())
    };
    Landed {
        expr: job.expr,
        home: job.home,
        lo,
        hi,
        y_center: yc,
        y: (y0, y1),
        sample: Sample {
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
            window: [lo, hi, y0, y1],
            log: (job.x_log && lo > 0.0, job.y_log && y0 > 0.0),
            main: result.map(Plot::new),
            layers: others
                .into_iter()
                .map(|(id, r)| (id, r.map(Plot::new)))
                .collect(),
            x_ticks,
            y_ticks,
        },
    }
}

fn landed(m: &mut BarModel, engine: &EngineThread, out: Landed) {
    let Some(g) = &mut m.graph else { return };
    if g.expression != out.expr {
        return;
    }
    if out.home {
        g.home = Some((out.lo, out.hi));
        g.home_y_center = out.y_center;
    }
    (g.x_min, g.x_max, g.y_center) = (out.lo, out.hi, out.y_center);
    if out.y.1 > out.y.0 {
        (g.y_min, g.y_max) = out.y;
    }
    // a surface opens in 3D: sample it again as a square, with its grid
    let first3d = out
        .sample
        .main
        .as_ref()
        .is_some_and(|p| p.kind == Kind::Surface)
        && g.view3d.is_none();
    if first3d {
        g.view3d = Some(true);
    }
    g.sample = Some(Rc::new(out.sample));
    g.bump();
    m.touch();
    if first3d {
        sample(m, engine, false);
    }
}

// ---- pan, zoom, orbit ----

/// A drag of `dx`, `dy` (fractions of the plot, down positive): the x window moves with the pointer (a plot of x
/// and y, or a set y window, moves in y too). In 3D a drag turns the surface instead, unless `move3d` (shift, alt,
/// the middle button), which slides the window along the floor (panGraph).
pub fn pan(m: &mut BarModel, engine: &EngineThread, dx: f64, dy: f64, move3d: bool) {
    let lock = gs::prefs().lock_viewport;
    let Some(g) = &mut m.graph else { return };
    if g.shows_3d() && !move3d {
        g.yaw += dx * std::f64::consts::PI;
        g.pitch = (g.pitch - dy * 1.6).clamp(PITCH_MIN, PITCH_MAX);
        g.reading = None;
        g.bump();
        m.touch();
        return;
    }
    if lock {
        return;
    }
    if g.shows_3d() {
        let (s, c) = g.yaw.sin_cos();
        let bx = -dx * 2.0;
        let by = dy * 2.0 / g.pitch.sin().max(0.3);
        let du = bx * c - by * s;
        let dv = -bx * s - by * c;
        let w = g.x_max - g.x_min;
        g.x_min += du * w / 2.0;
        g.x_max += du * w / 2.0;
        g.y_center =
            Some(g.y_center.unwrap_or((g.y_min + g.y_max) / 2.0) + dv * (g.y_max - g.y_min) / 2.0);
    } else {
        (g.x_min, g.x_max) = shifted(g.x_min, g.x_max, -dx, g.log_x());
        if let Some((lo, hi)) = g.fixed_y {
            let y = shifted(lo, hi, dy, g.log_y());
            g.fixed_y = Some(y);
            (g.y_min, g.y_max) = y;
        } else if g.kind() != Kind::Curve {
            let d = dy * (g.y_max - g.y_min);
            g.y_center = Some(g.y_center.unwrap_or((g.y_min + g.y_max) / 2.0) + d);
            g.y_min += d;
            g.y_max += d;
        }
    }
    g.reading = None;
    g.bump();
    m.touch();
    sample(m, engine, false);
}

/// zoomAt: the x window about `at` (a fraction across), kept within 1e-6 … 1e6 wide; a y or z window that's set
/// (or a plot of x and y's) scales with it.
pub fn zoom(m: &mut BarModel, engine: &EngineThread, factor: f64, at: f64) {
    if gs::prefs().lock_viewport || !factor.is_finite() || factor <= 0.0 {
        return;
    }
    let Some(g) = &mut m.graph else { return };
    let (lo, hi) = scaled(g.x_min, g.x_max, factor, at.clamp(0.0, 1.0), g.log_x());
    if hi - lo < MIN_SPAN
        || (hi - lo > MAX_SPAN && !g.log_x())
        || !lo.is_finite()
        || !hi.is_finite()
    {
        return;
    }
    (g.x_min, g.x_max) = (lo, hi);
    if let Some((a, b)) = g.fixed_y {
        let y = scaled(a, b, factor, 0.5, g.log_y());
        g.fixed_y = Some(y);
        (g.y_min, g.y_max) = y;
    } else if g.kind() != Kind::Curve && !g.shows_3d() {
        (g.y_min, g.y_max) = scaled(g.y_min, g.y_max, factor, 0.5, false);
    }
    if let Some((a, b)) = g.fixed_z {
        g.fixed_z = Some(scaled(a, b, factor, 0.5, false));
    }
    g.reading = None;
    g.bump();
    m.touch();
    sample(m, engine, false);
}

/// Reset: the home window, its y centre, no set y or z, the home turn.
pub fn home(m: &mut BarModel, engine: &EngineThread) {
    let Some(g) = &mut m.graph else { return };
    let Some((lo, hi)) = g.home else { return };
    (g.x_min, g.x_max) = (lo, hi);
    g.y_center = g.home_y_center;
    g.fixed_y = None;
    g.fixed_z = None;
    g.yaw = HOME_YAW;
    g.pitch = HOME_PITCH;
    if g.x_log && lo <= 0.0 {
        (g.x_min, g.x_max) = if hi > 0.0 {
            (hi / 1000.0, hi)
        } else {
            (0.1, 100.0)
        };
    }
    g.reading = None;
    g.bump();
    m.touch();
    sample(m, engine, false);
}

/// A surface between its 3D view and its level curves.
pub fn set_3d(m: &mut BarModel, engine: &EngineThread, on: bool) {
    let Some(g) = &mut m.graph else { return };
    if g.kind() != Kind::Surface || g.view3d == Some(on) {
        return;
    }
    g.view3d = Some(on);
    g.reading = None;
    g.bump();
    m.touch();
    sample(m, engine, false);
}

// ---- equations added in the panel, and the settings' windows and axes ----

pub fn add_layer(m: &mut BarModel, engine: &EngineThread) {
    let Some(g) = &mut m.graph else { return };
    let id = g.layers.iter().map(|l| l.id).max().unwrap_or(0) + 1;
    g.layers.push(Layer {
        id,
        expression: String::new(),
        from_bar: false,
    });
    focus(m, engine, FieldId::Layer(id));
}

pub fn remove_layer(m: &mut BarModel, engine: &EngineThread, id: u32) {
    let Some(g) = &mut m.graph else { return };
    g.layers.retain(|l| l.id != id);
    if g.focus == Some(FieldId::Layer(id)) {
        g.focus = None;
    }
    g.bump();
    m.touch();
    sample(m, engine, false);
}

fn set_x(m: &mut BarModel, engine: &EngineThread, lo: f64, hi: f64) {
    let Some(g) = &mut m.graph else { return };
    if !(lo.is_finite() && hi.is_finite() && hi > lo) || (g.x_log && lo <= 0.0) {
        return;
    }
    (g.x_min, g.x_max) = (lo, hi);
    g.bump();
    m.touch();
    sample(m, engine, false);
}

fn set_y(m: &mut BarModel, engine: &EngineThread, lo: f64, hi: f64) {
    let Some(g) = &mut m.graph else { return };
    if !(lo.is_finite() && hi.is_finite() && hi > lo) || (g.y_log && lo <= 0.0) {
        return;
    }
    g.fixed_y = Some((lo, hi));
    (g.y_min, g.y_max) = (lo, hi);
    g.bump();
    m.touch();
    sample(m, engine, false);
}

fn set_z(m: &mut BarModel, lo: f64, hi: f64) {
    let Some(g) = &mut m.graph else { return };
    if lo.is_finite() && hi.is_finite() && hi > lo {
        g.fixed_z = Some((lo, hi));
        g.bump();
        m.touch();
    }
}

/// Zoom Square: y as many units a point as x, about the middle of the y shown (in 3D, z back to the cube's).
pub fn zoom_square(m: &mut BarModel, engine: &EngineThread) {
    let Some(g) = &mut m.graph else { return };
    if g.log_x() || g.log_y() {
        return;
    }
    if g.shows_3d() {
        g.fixed_z = None;
        g.bump();
        m.touch();
        return;
    }
    let (mid, half) = (
        (g.y_min + g.y_max) / 2.0,
        (g.x_max - g.x_min) * PLOT_ASPECT / 2.0,
    );
    set_y(m, engine, mid - half, mid + half);
}

/// A log x or y axis; a window reaching 0 or below moves above it (three decades under its top).
pub fn set_log(m: &mut BarModel, engine: &EngineThread, x: Option<bool>, y: Option<bool>) {
    let Some(g) = &mut m.graph else { return };
    if let Some(x) = x {
        g.x_log = x;
        if x && g.x_min <= 0.0 {
            (g.x_min, g.x_max) = if g.x_max > 0.0 {
                (g.x_max / 1000.0, g.x_max)
            } else {
                (0.1, 100.0)
            };
        }
    }
    if let Some(y) = y {
        g.y_log = y;
        if y {
            let top = g.fixed_y.map_or(g.y_max, |f| f.1).max(0.0);
            if g.fixed_y.map_or(g.y_min, |f| f.0) <= 0.0 {
                g.fixed_y = Some(if top > 0.0 {
                    (top / 1000.0, top)
                } else {
                    (0.1, 100.0)
                });
            }
        }
    }
    g.bump();
    m.touch();
    sample(m, engine, false);
}

// ---- the panel's fields ----

/// A field in the panel takes the keys (the one before is committed).
pub fn focus(m: &mut BarModel, engine: &EngineThread, id: FieldId) {
    commit(m, engine);
    let Some(g) = &mut m.graph else { return };
    let text = g.field_text(id);
    g.edit = Field::default();
    g.edit.keep_words = !matches!(id, FieldId::Layer(_));
    g.edit.reset(&text);
    g.edit.select_all();
    g.focus = Some(id);
    g.bump();
    m.touch();
}

/// The field with the keys gives them back to the line: a number field's value applies.
pub fn commit(m: &mut BarModel, engine: &EngineThread) {
    let Some(g) = &mut m.graph else { return };
    let Some(id) = g.focus.take() else { return };
    let text = g.edit.text().to_string();
    g.bump();
    let v = gs::parse(&text);
    let blank = text.trim().is_empty();
    let (x, y3, three) = ((g.x_min, g.x_max), g.z_window(), g.shows_3d());
    let y = if three { y3 } else { (g.y_min, g.y_max) };
    m.touch();
    match id {
        FieldId::XLo => set_x(m, engine, v.unwrap_or(x.0), x.1),
        FieldId::XHi => set_x(m, engine, x.0, v.unwrap_or(x.1)),
        FieldId::YLo | FieldId::YHi => {
            let (lo, hi) = if id == FieldId::YLo {
                (v.unwrap_or(y.0), y.1)
            } else {
                (y.0, v.unwrap_or(y.1))
            };
            if v.is_none() {
                return;
            }
            if three {
                set_z(m, lo, hi);
            } else {
                set_y(m, engine, lo, hi);
            }
        }
        FieldId::XStep | FieldId::YStep if blank || v.is_some() => {
            let step = v.filter(|s| *s > 0.0 && s.is_finite());
            if let Some(g) = &mut m.graph {
                if id == FieldId::XStep {
                    g.x_step = step;
                } else {
                    g.y_step = step;
                }
            }
        }
        _ => {}
    }
}

/// A typed change to the field with the keys: labels and equations apply as they're typed.
fn edited(m: &mut BarModel, engine: &EngineThread) {
    let Some(g) = &mut m.graph else { return };
    let text = g.edit.text().to_string();
    match g.focus {
        Some(FieldId::XLabel) => g.x_label = text,
        Some(FieldId::YLabel) => g.y_label = text,
        Some(FieldId::Layer(id)) => {
            if let Some(l) = g.layers.iter_mut().find(|l| l.id == id) {
                l.expression = text;
            }
            g.bump();
            m.touch();
            sample(m, engine, false);
            return;
        }
        _ => {}
    }
    g.bump();
    m.touch();
}

/// A key while a field in the panel has the keys: editing there, ↵ / Tab commit, Esc gives them back. None when
/// no field has them.
pub fn key_down(m: &mut BarModel, vk: u32, mods: Mods, engine: &EngineThread) -> Option<Outcome> {
    const BACK: u32 = 0x08;
    const TAB: u32 = 0x09;
    const ENTER: u32 = 0x0D;
    const ESC: u32 = 0x1B;
    const END: u32 = 0x23;
    const HOME: u32 = 0x24;
    const LEFT: u32 = 0x25;
    const RIGHT: u32 = 0x27;
    const DELETE: u32 = 0x2E;
    let id = m.graph.as_ref()?.focus?;
    match vk {
        ESC => {
            // a number typed and not committed goes back to what it was
            if id.live() {
                commit(m, engine);
            } else if let Some(g) = &mut m.graph {
                g.focus = None;
            }
            g_touch(m);
            return Some(Outcome::Handled);
        }
        ENTER | TAB => {
            commit(m, engine);
            return Some(Outcome::Handled);
        }
        _ => {}
    }
    let ctrl = mods.ctrl && !mods.alt;
    let shift = mods.shift;
    let f = &mut m.graph.as_mut()?.edit;
    let before = f.text().to_string();
    match vk {
        LEFT => f.left(ctrl, shift),
        RIGHT => f.right(ctrl, shift),
        HOME => f.home(shift),
        END => f.end(shift),
        BACK => {
            f.backspace(ctrl);
        }
        DELETE => {
            f.delete(ctrl);
        }
        0x41 if ctrl => f.select_all(),
        0x43 if ctrl => {
            let s = f.selected_text().to_string();
            if !s.is_empty() {
                m.to_clipboard = Some(s);
            }
        }
        0x58 if ctrl => {
            if let Some(cut) = f.cut() {
                m.to_clipboard = Some(cut);
            }
        }
        0x56 if ctrl => {
            if let Some(text) = crate::clip::get(windows::Win32::Foundation::HWND::default()) {
                f.paste(&text.replace(['\r', '\n'], " "));
            }
        }
        0x5A if ctrl && shift => {
            f.redo();
        }
        0x5A if ctrl => {
            f.undo();
        }
        0x59 if ctrl => {
            f.redo();
        }
        // Ctrl+W and the other chords are still the bar's
        _ if ctrl || mods.alt => return None,
        _ => {}
    }
    if m.graph.as_ref().is_some_and(|g| g.edit.text() != before) {
        edited(m, engine);
    }
    g_touch(m);
    Some(Outcome::Handled)
}

fn g_touch(m: &mut BarModel) {
    if let Some(g) = &mut m.graph {
        g.bump();
    }
    m.touch();
}

/// WM_CHAR while a field in the panel has the keys.
pub fn char16(m: &mut BarModel, unit: u16, engine: &EngineThread) -> bool {
    let Some(g) = m.graph.as_mut().filter(|g| g.focus.is_some()) else {
        return false;
    };
    if g.edit.char16(unit) {
        edited(m, engine);
    }
    true
}

/// Esc with a graph up and the line not empty clears the line (BarModel.escape).
pub fn escape(m: &mut BarModel, engine: &EngineThread) -> bool {
    if m.graph.is_none() || m.field.text().is_empty() {
        return false;
    }
    m.field.set_text("");
    m.text_changed(engine);
    true
}

/// A summary for the bench pipe's `state`.
pub fn debug(m: &BarModel) -> serde_json::Value {
    let Some(g) = &m.graph else {
        return serde_json::Value::Null;
    };
    let main = g.main();
    let plot = |p: &Plot| {
        serde_json::json!({
            "label": p.label, "error": p.error, "points": p.points.len(), "roots": p.roots,
            "kind": format!("{:?}", p.kind), "contours": p.contours.len(),
            "contourSegments": p.contours.iter().map(|c| c.segments.len() / 4).sum::<usize>(),
            "surface": p.surface.as_ref().map(|s| serde_json::json!({ "nx": s.nx, "ny": s.ny, "z": [s.z_lo, s.z_hi] })),
        })
    };
    let critical: Vec<_> = g
        .all_critical()
        .iter()
        .map(|(l, c)| serde_json::json!({ "layer": l, "kind": kind_name(c.kind), "x": c.x, "y": c.y, "z": c.z }))
        .collect();
    let cache = g.cache.borrow();
    serde_json::json!({
        "expression": g.expression, "window": [g.x_min, g.x_max, g.y_min, g.y_max], "home": g.home.map(|h| [h.0, h.1]),
        "yCenter": g.y_center, "homeYCenter": g.home_y_center, "fixedY": g.fixed_y.map(|f| [f.0, f.1]),
        "fixedZ": g.fixed_z.map(|f| [f.0, f.1]), "zWindow": [g.z_window().0, g.z_window().1],
        "failed": g.failed(), "label": main.map(|p| p.label.clone()), "error": main.and_then(|p| p.error.clone()),
        "points": main.map_or(0, |p| p.points.len()), "roots": main.map(|p| p.roots.clone()), "rootsListed": g.roots(),
        "kind": format!("{:?}", g.kind()), "view3d": g.shows_3d(), "yaw": g.yaw, "pitch": g.pitch,
        "contours": main.map_or(0, |p| p.contours.len()),
        "critical": critical.len(), "criticalPoints": critical, "title": g.detail_title(),
        "layers": g.layers.iter().map(|l| serde_json::json!({
            "id": l.id, "expression": l.expression, "fromBar": l.from_bar,
            "plot": g.sample.as_ref().and_then(|s| s.layer(l.id)).map(plot),
        })).collect::<Vec<_>>(),
        "main": main.map(plot), "atHome": g.at_home(), "settingsOpen": g.settings_open,
        "focus": g.focus.map(|f| format!("{f:?}")), "edit": g.edit.text(),
        "labels": [g.x_label, g.y_label], "steps": [g.x_step, g.y_step], "log": [g.x_log, g.y_log],
        "prefs": serde_json::to_value(gs::prefs()).unwrap_or_default(),
        "pointer": g.pointer.map(|p| [p.0, p.1]), "reading": g.reading.as_ref().map(|r| format!("{r:?}")),
        "draw": cache.debug(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_commands() {
        assert_eq!(command("graph x^3").as_deref(), Some("x^3"));
        assert_eq!(
            command("  GRAPH   sin(x)  +  1 ").as_deref(),
            Some("sin(x) + 1")
        );
        assert_eq!(command("plot f").as_deref(), Some("f"));
        assert_eq!(command("graph f(x) = x^2").as_deref(), Some("f(x) = x^2"));
        assert_eq!(command("graph"), None);
        assert_eq!(command("graph "), None);
        assert_eq!(command("graphx"), None);
        assert_eq!(command("2+2"), None);
        assert_eq!(command("é"), None);
    }

    #[test]
    fn several_equations_split_at_top_level_semicolons() {
        assert_eq!(split_graphs("x^2; sin(x) ;  x+1"), ["x^2", "sin(x)", "x+1"]);
        assert_eq!(split_graphs("f(1;2); y"), ["f(1;2)", "y"]);
        assert_eq!(split_graphs("[a;b]; {c;d}"), ["[a;b]", "{c;d}"]);
        // empty ones after the first go; a lone first stays
        assert_eq!(split_graphs("x;;  ; y;"), ["x", "y"]);
        assert_eq!(split_graphs(" x "), ["x"]);
        assert_eq!(split_graphs("; y"), ["", "y"]);
        // a stray closing bracket doesn't go below the top
        assert_eq!(split_graphs("x); y"), ["x)", "y"]);
    }

    #[test]
    fn domains_stay_ordered_and_bounded() {
        assert_eq!(clamp_domain(3.0, -3.0), (-3.0, 3.0));
        assert_eq!(clamp_domain(f64::NAN, 1.0), (-10.0, 10.0));
        let (lo, hi) = clamp_domain(1.0, 1.0);
        assert!((hi - lo - 1e-6).abs() < 1e-12);
        let (lo, hi) = clamp_domain(-1e7, 1e7);
        assert!((hi - lo - 1e6).abs() < 1e-3);
    }

    #[test]
    fn pans_and_zooms_work_in_decades_on_a_log_axis() {
        assert_eq!(scaled(-10.0, 10.0, 0.5, 0.0, false), (-10.0, 0.0));
        assert_eq!(scaled(-10.0, 10.0, 2.0, 0.5, false), (-20.0, 20.0));
        assert_eq!(shifted(0.0, 10.0, 0.5, false), (5.0, 15.0));
        let (lo, hi) = shifted(1.0, 100.0, 0.5, true);
        assert!((lo - 10.0).abs() < 1e-9 && (hi - 1000.0).abs() < 1e-9);
        let (lo, hi) = scaled(1.0, 10000.0, 0.5, 0.5, true);
        assert!((lo - 10.0).abs() < 1e-9 && (hi - 1000.0).abs() < 1e-9);
        // a log scale needs a window above 0
        assert_eq!(shifted(-1.0, 1.0, 0.5, true), (0.0, 2.0));
    }

    #[test]
    fn numbers_read_like_percent_g() {
        assert_eq!(fmt_g(0.0, 8), "0");
        assert_eq!(fmt_g(-2.5, 8), "−2.5");
        assert_eq!(fmt_g(1.0 / 3.0, 8), "0.33333333");
        assert_eq!(fmt_g(1234567.0, 6), "1.23457e6");
        assert_eq!(fmt_g(0.00001234, 4), "1.234e−5");
        assert_eq!(fmt_g(-1e-20, 3), "−1e−20");
        assert_eq!(fmt_g(10.0, 8), "10");
        assert_eq!(fmt4(-1e-10), "0");
        assert_eq!(fmt4(1.23456), "1.235");
        assert_eq!(round_to_pixel(1.23456, 0.01), 1.23);
        assert_eq!(round_to_pixel(-0.30000000000000004, 0.05), -0.3);
    }

    #[test]
    fn curves_break_at_gaps_and_poles() {
        let p = |x: f64, y: Option<f64>| GraphPoint { x, y };
        let window = [0.0, 10.0, -1.0, 1.0];
        let pts = [
            p(0.0, Some(0.0)),
            p(1.0, Some(0.5)),
            p(2.0, None),
            p(3.0, Some(0.0)),
            p(4.0, None),
            p(5.0, Some(500.0)),
            p(6.0, Some(-500.0)),
            p(7.0, Some(-400.0)),
        ];
        let (lines, dots) = runs(&pts, window, (false, false));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![(0.0, 0.5), (0.1, 0.25)]);
        // the pole between 5 and 6 splits them: 5 alone is a dot, 6-7 a line
        assert_eq!(lines[1].len(), 2);
        assert_eq!(dots.len(), 2);
        assert_eq!(dots[0], (0.3, 0.5));
        // far samples are pinned
        assert_eq!(dots[1].1, -V_LIMIT);
        // on a log y axis, y at or below 0 is a gap and the rest sit in decades
        let log = [
            p(1.0, Some(1.0)),
            p(2.0, Some(10.0)),
            p(3.0, Some(0.0)),
            p(4.0, Some(100.0)),
        ];
        let (lines, dots) = runs(&log, [1.0, 4.0, 1.0, 100.0], (false, true));
        assert_eq!(lines, vec![vec![(0.0, 1.0), (1.0 / 3.0, 0.5)]]);
        assert_eq!(dots, vec![(1.0, 0.0)]);
    }

    #[test]
    fn chips_wrap() {
        assert_eq!(
            wrap(&[50.0, 50.0, 50.0], 120.0, 4.0),
            vec![(0.0, 0), (54.0, 0), (0.0, 1)]
        );
        assert_eq!(wrap(&[200.0], 120.0, 4.0), vec![(0.0, 0)]);
    }

    #[test]
    fn ticks_step_by_one_two_or_five() {
        assert_eq!(nice_ticks(-10.0, 10.0, 4), [-10.0, -5.0, 0.0, 5.0, 10.0]);
        assert_eq!(
            nice_ticks(0.0, 1.0, 5),
            [0.0, 0.2, 0.4, 0.6000000000000001, 0.8, 1.0]
        );
        assert_eq!(
            nice_ticks(-3.0, 3.0, 6),
            [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]
        );
        assert!(nice_ticks(1.0, 1.0, 4).is_empty());
        assert!(nice_ticks(f64::NAN, 1.0, 4).is_empty());
    }

    #[test]
    fn grids_take_a_set_step_or_decades_or_the_engines_ticks() {
        // a step of 2: minor lines a quarter of it
        let g = axis_grid(-4.0, 4.0, false, Some(2.0), &[]);
        assert_eq!(
            g.major.iter().map(|m| m.0).collect::<Vec<_>>(),
            [-4.0, -2.0, 0.0, 2.0, 4.0]
        );
        assert_eq!(g.major[0].1, "−4");
        assert_eq!(g.minor.len(), 12);
        assert!(g.minor.contains(&-3.5) && !g.minor.contains(&-2.0));
        // a step of 5: fifths
        let g = axis_grid(0.0, 10.0, false, Some(5.0), &[]);
        assert_eq!(g.minor, [1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0, 9.0]);
        // log: decades, with 2…9 between
        let g = axis_grid(1.0, 1000.0, true, None, &[]);
        assert_eq!(
            g.major.iter().map(|m| m.0).collect::<Vec<_>>(),
            [1.0, 10.0, 100.0, 1000.0]
        );
        assert_eq!(g.minor.len(), 24);
        // otherwise the engine's ticks, as labelled
        let t = |v: f64, l: &str| GraphTick {
            value: v,
            label: l.into(),
        };
        let g = axis_grid(
            -1.0,
            11.0,
            false,
            None,
            &[t(-5.0, "-5"), t(0.0, "0"), t(5.0, "5"), t(10.0, "10")],
        );
        assert_eq!(
            g.major,
            vec![(0.0, "0".into()), (5.0, "5".into()), (10.0, "10".into())]
        );
        assert_eq!(g.step(), Some(5.0));
        assert!(g.minor.contains(&-1.0) && g.minor.contains(&11.0));
    }

    #[test]
    fn the_pointer_picks_the_nearest_mark_within_reach() {
        let marks = [
            Some((10.0, 10.0)),
            None,
            Some((14.0, 10.0)),
            Some((100.0, 100.0)),
        ];
        assert_eq!(nearest(&marks, (13.0, 10.0), SNAP), Some(2));
        assert_eq!(nearest(&marks, (5.0, 10.0), SNAP), Some(0));
        assert_eq!(nearest(&marks, (50.0, 50.0), SNAP), None);
        assert_eq!(nearest(&[], (0.0, 0.0), SNAP), None);
    }

    #[test]
    fn equations_take_their_colours_in_turn() {
        assert_eq!(layer_rgb(0), None);
        assert_eq!(layer_rgb(1), Some((0.86, 0.30, 0.27)));
        assert_eq!(layer_rgb(2), Some((0.25, 0.52, 0.86)));
        assert_eq!(layer_rgb(7), layer_rgb(1));
    }

    fn plot(kind: Kind, critical: Vec<CriticalPoint>, roots: Vec<f64>) -> Plot {
        Plot {
            label: String::new(),
            error: None,
            points: Vec::new(),
            roots,
            critical,
            contours: Vec::new(),
            kind,
            surface: None,
            unit: None,
        }
    }

    fn state_with(main: Plot, layers: Vec<(Layer, Plot)>) -> GraphState {
        let mut g = GraphState::new("x".into(), None);
        g.layers = layers.iter().map(|l| l.0.clone()).collect();
        g.sample = Some(Rc::new(Sample {
            serial: 1,
            window: [-10.0, 10.0, -10.0, 10.0],
            log: (false, false),
            main: Some(main),
            layers: layers.into_iter().map(|(l, p)| (l.id, Some(p))).collect(),
            x_ticks: Vec::new(),
            y_ticks: Vec::new(),
        }));
        g
    }

    fn cp(kind: CriticalKind, x: f64, y: f64) -> CriticalPoint {
        CriticalPoint {
            x,
            y,
            kind,
            z: None,
        }
    }

    #[test]
    fn a_lines_root_shows_once_and_every_equations_points_are_listed() {
        let line = plot(
            Kind::Curve,
            vec![
                cp(CriticalKind::XIntercept, 1.5, 0.0),
                cp(CriticalKind::YIntercept, 0.0, 7.5),
            ],
            vec![1.5],
        );
        let layer = Layer {
            id: 1,
            expression: "x^2-4".into(),
            from_bar: true,
        };
        let parabola = plot(
            Kind::Curve,
            vec![cp(CriticalKind::Min, 0.0, -4.0)],
            vec![-2.0, 2.0],
        );
        let g = state_with(line, vec![(layer, parabola)]);
        assert!(g.roots().is_empty());
        let all = g.all_critical();
        assert_eq!(all.iter().map(|a| a.0).collect::<Vec<_>>(), [0, 0, 1]);
        assert_eq!(g.detail_title(), "Critical points");
        let pts = plot(Kind::Curve, vec![cp(CriticalKind::Point, 1.0, 2.0)], vec![]);
        assert_eq!(state_with(pts, vec![]).detail_title(), "Points");
        let circle = plot(
            Kind::Implicit,
            vec![cp(CriticalKind::Max, 0.0, 5.0)],
            vec![],
        );
        let g = state_with(circle, vec![]);
        assert_eq!(g.detail_title(), "Key points");
        assert!(g.roots().is_empty());
        let curve = plot(Kind::Curve, vec![], vec![-2.0, 2.0]);
        assert_eq!(state_with(curve, vec![]).roots(), [-2.0, 2.0]);
    }

    #[test]
    fn bar_layers_keep_their_ids_and_the_panels_follow() {
        let mut g = GraphState::new("x".into(), None);
        g.layers = vec![
            Layer {
                id: 1,
                expression: "a".into(),
                from_bar: true,
            },
            Layer {
                id: 2,
                expression: "typed".into(),
                from_bar: false,
            },
        ];
        let next = bar_layers(&g, &["a2".into(), "b".into()]);
        assert_eq!(
            next.iter()
                .map(|l| (l.id, l.expression.as_str(), l.from_bar))
                .collect::<Vec<_>>(),
            [(1, "a2", true), (3, "b", true), (2, "typed", false)]
        );
    }

    #[test]
    fn home_includes_the_turn_only_in_3d() {
        let mut g = state_with(plot(Kind::Surface, vec![], vec![]), vec![]);
        g.home = Some((-10.0, 10.0));
        assert!(g.at_home());
        g.yaw = 1.0;
        assert!(g.at_home());
        g.view3d = Some(true);
        assert!(!g.at_home());
        g.yaw = HOME_YAW;
        assert!(g.at_home());
        g.fixed_z = Some((0.0, 1.0));
        assert!(!g.at_home());
    }
}
