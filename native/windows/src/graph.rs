//! The graph panel under the composer: `graph x^3`, `graph f`, `graph f(x) = …`, `plot …`. Spec: the Mac's
//! GraphModel.swift / GraphPanel.swift and the web's GraphPanel.tsx (`.graph` in src/index.css), which the Mac
//! follows and which also draws level curves, the roots list and the snapping readout.
//!
//! - State (`GraphState`, `BarModel::graph`): the x window (opening on `graph_home`, moved by dragging, the wheel,
//!   a pinch and the zoom buttons), and the newest `Sample` the engine made for it. Nothing exists while the line
//!   isn't a graph command, so the panel costs nothing closed; `follow` (called from `evaluate_live`) is a prefix
//!   test on a keystroke.
//! - Sampling (`sample`): `graph_home` (once per expression), `graph`, then `graph_ticks` for both axes, on
//!   `Lane::Graph`, so a burst of pans or zooms samples only the newest window and an older result never lands.
//!   Between samples the old curve is drawn under the new window, so a drag follows the pointer at once.
//! - Drawing: each sample's curve and level curves become Direct2D path geometries once, in the sample's own unit
//!   square, and a transformed geometry maps them onto the plot for the window as it is now (strokes keep their
//!   width). Text layouts (head, chips, tick labels) are cached per sample and width.
//! - The readout under the pointer snaps to a root or extremum within 8 px; otherwise it asks the engine
//!   (`graph_readout_y`, or `graph_readout_z` for level curves) on `Lane::GraphReadout`. A click on a snapped point
//!   or a chip copies its x.
//!
//! Hooks: model.rs (`graph` field, `follow` in `evaluate_live`, `escape` in `key_down`), window.rs (`height` in
//! `Bar::layout`, `paint` in `Bar::paint`), app.rs (`click`, `mouse_move`, `mouse_up`, `wheel`, `leave`, `cursor`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use qcalc_ffi::{
    AngleUnit, ContourLevel, CriticalKind, CriticalPoint, GraphOptions, GraphPoint, GraphResult,
    GraphTick, merged_options,
};

use crate::engine::{EngineThread, Lane, done};
use crate::model::{BarModel, now_ms};

/// The web's viewBox: a 652 × 200 plot with padding 12 / 12 / 28 / 40, scaled to the panel's width.
pub const SVG_W: f32 = 652.0;
pub const SVG_H: f32 = 200.0;
const PAD_TOP: f32 = 12.0;
const PAD_RIGHT: f32 = 12.0;
const PAD_BOTTOM: f32 = 28.0;
const PAD_LEFT: f32 = 40.0;
const PLOT_W: f32 = SVG_W - PAD_LEFT - PAD_RIGHT;
const PLOT_H: f32 = SVG_H - PAD_TOP - PAD_BOTTOM;
/// PLOT_ASPECT: a contour plot keeps x and y at one scale
pub const PLOT_ASPECT: f64 = (PLOT_H / PLOT_W) as f64;
const MIN_SPAN: f64 = 1e-6;
const MAX_SPAN: f64 = 1e6;
/// viewBox units within which the readout locks onto a root or extremum
const SNAP: f32 = 8.0;
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

/// zoomAt: the window scaled by `factor` about the point `t` of the way across (0 left, 1 right).
pub fn zoom_at(lo: f64, hi: f64, t: f64, factor: f64) -> (f64, f64) {
    let t = t.clamp(0.0, 1.0);
    let anchor = lo + (hi - lo) * t;
    let span = (hi - lo) * factor;
    clamp_domain(anchor - span * t, anchor + span * (1.0 - t))
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

/// roundToPixel: `v` to the finest decimal a pixel `res` wide can tell apart.
pub fn round_to_pixel(v: f64, res: f64) -> f64 {
    if res.is_nan() || res <= 0.0 || !res.is_finite() || !v.is_finite() {
        return v;
    }
    let step = 10f64.powf(res.log10().floor());
    let r = (v / step).round() * step;
    format!("{r:.11e}").parse().unwrap_or(r)
}

/// A sampled curve in its sample's unit square (u across the x window, v down the y window): the runs to draw as
/// lines and the isolated samples to draw as dots. A run breaks at an undefined sample and where one step crosses
/// the whole view (a pole, as tan and 1/x have), as pointsToPath does.
/// Polylines and dots in a sample's unit square.
pub type Runs = (Vec<Vec<(f32, f32)>>, Vec<(f32, f32)>);

pub fn runs(points: &[GraphPoint], window: [f64; 4]) -> Runs {
    let [x0, x1, y0, y1] = window;
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
        let u = ((p.x - x0) / xw) as f32;
        let v = p.y.map(|y| ((y1 - y) / yh) as f32);
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

fn kind_name(k: CriticalKind) -> &'static str {
    match k {
        CriticalKind::Min => "min",
        CriticalKind::Max => "max",
        CriticalKind::Critical => "crit",
    }
}

// ---- state ----

static SERIAL: AtomicU64 = AtomicU64::new(1);

/// What the engine made for one x window.
#[derive(Debug)]
pub struct Sample {
    pub serial: u64,
    /// x_min, x_max, y_min, y_max it was sampled over
    pub window: [f64; 4],
    pub label: String,
    pub error: Option<String>,
    pub points: Vec<GraphPoint>,
    pub roots: Vec<f64>,
    pub critical: Vec<CriticalPoint>,
    pub contours: Vec<ContourLevel>,
    /// a function of x and y (level curves) or an implicit `f = g` curve
    pub contour: bool,
    /// level curves, with labels and a z readout
    pub levels: bool,
    pub unit: Option<AngleUnit>,
    pub x_ticks: Vec<GraphTick>,
    pub y_ticks: Vec<GraphTick>,
}

impl Sample {
    fn new(
        r: GraphResult,
        lo: f64,
        hi: f64,
        x_ticks: Vec<GraphTick>,
        y_ticks: Vec<GraphTick>,
    ) -> Sample {
        let (y0, y1) = if r.y_max > r.y_min {
            (r.y_min, r.y_max)
        } else {
            (-10.0, 10.0)
        };
        Sample {
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
            window: [lo, hi, y0, y1],
            label: r.label,
            error: r.error,
            points: r.points,
            roots: r.roots,
            critical: r.critical_points,
            contours: r.contours,
            contour: r.contour,
            levels: r.contour_levels,
            unit: r.angle_unit,
            x_ticks,
            y_ticks,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    /// viewBox x where the press was
    start: f32,
    domain: (f64, f64),
    moved: bool,
}

/// What the readout says, for the point it was asked about.
#[derive(Clone, Debug)]
pub struct Readout {
    pub x: f64,
    /// y on the curve (or the snapped point's y); None off the curve or for a contour plot
    pub y: Option<f64>,
    /// a snapped point's kind ("root", "min"…)
    pub kind: Option<&'static str>,
    pub text: String,
    /// a contour plot's readout sits at the pointer's x, without a hairline or dot
    pub contour: bool,
}

/// The panel's state (`BarModel::graph`).
pub struct GraphState {
    pub expression: String,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub home: Option<(f64, f64)>,
    pub sample: Option<Rc<Sample>>,
    /// the engine couldn't graph it at all
    pub failed: bool,
    /// the pointer over the panel, panel-relative DIPs
    pub hover: Option<(f32, f32)>,
    drag: Option<Drag>,
    pub readout: Option<Readout>,
    cache: RefCell<view::Cache>,
}

impl GraphState {
    fn new(expression: String, carry: Option<GraphState>) -> GraphState {
        // a new expression keeps the old picture until its own sample lands, so the panel doesn't jump
        let (window, sample, failed) = match carry {
            Some(o) => ((o.x_min, o.x_max, o.y_min, o.y_max), o.sample, o.failed),
            None => ((-10.0, 10.0, -10.0, 10.0), None, false),
        };
        GraphState {
            expression,
            x_min: window.0,
            x_max: window.1,
            y_min: window.2,
            y_max: window.3,
            home: None,
            sample,
            failed,
            hover: None,
            drag: None,
            readout: None,
            cache: RefCell::default(),
        }
    }

    pub fn input(&self) -> String {
        format!("graph {}", self.expression)
    }

    pub fn at_home(&self) -> bool {
        self.home
            .is_none_or(|(lo, hi)| (self.x_min - lo).abs() < 1e-9 && (self.x_max - hi).abs() < 1e-9)
    }

    fn label(&self) -> String {
        match &self.sample {
            Some(s) if !s.label.is_empty() => s.label.clone(),
            _ => self.expression.replace("sqrt(", "√("),
        }
    }

    /// viewBox x → x
    fn x_at(&self, vx: f32) -> f64 {
        self.x_min + f64::from((vx - PAD_LEFT) / PLOT_W) * (self.x_max - self.x_min)
    }

    /// x → viewBox x
    fn vx(&self, x: f64) -> f32 {
        PAD_LEFT + ((x - self.x_min) / (self.x_max - self.x_min)) as f32 * PLOT_W
    }

    fn vy(&self, y: f64) -> f32 {
        PAD_TOP + ((self.y_max - y) / (self.y_max - self.y_min)) as f32 * PLOT_H
    }

    fn y_at(&self, vy: f32) -> f64 {
        self.y_max - f64::from((vy - PAD_TOP) / PLOT_H) * (self.y_max - self.y_min)
    }

    /// Roots and extrema inside the plot, as (x, y, kind).
    fn features(&self) -> Vec<(f64, f64, &'static str)> {
        let Some(s) = &self.sample else {
            return Vec::new();
        };
        let inside = |x: f64, y: f64| {
            let (vx, vy) = (self.vx(x), self.vy(y));
            (PAD_LEFT..=PAD_LEFT + PLOT_W).contains(&vx)
                && (PAD_TOP..=PAD_TOP + PLOT_H).contains(&vy)
        };
        s.roots
            .iter()
            .map(|&x| (x, 0.0, "root"))
            .chain(s.critical.iter().map(|c| (c.x, c.y, kind_name(c.kind))))
            .filter(|&(x, y, _)| inside(x, y))
            .collect()
    }

    fn snap(&self, vx: f32) -> Option<(f64, f64, &'static str)> {
        self.features()
            .into_iter()
            .map(|f| ((self.vx(f.0) - vx).abs(), f))
            .filter(|(d, _)| *d <= SNAP)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, f)| f)
    }

    fn deg(&self) -> &'static str {
        match self.sample.as_ref().and_then(|s| s.unit) {
            Some(AngleUnit::Deg) => "°",
            _ => "",
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
    if m.graph.as_ref().is_some_and(|g| g.expression == body) {
        sample(m, engine, false);
        return;
    }
    let old = m.graph.take();
    m.graph = Some(GraphState::new(body, old));
    m.touch();
    sample(m, engine, true);
}

/// buildGraph for the window as it is (and, for a new expression, its home window first), off the UI thread.
fn sample(m: &BarModel, engine: &EngineThread, home: bool) {
    let Some(g) = &m.graph else { return };
    let input = g.input();
    let expr = g.expression.clone();
    let (lo, hi) = (g.x_min, g.x_max);
    let rows = m.rows.clone();
    let view = m.view(now_ms());
    let options = m.options();
    engine.submit(Lane::Graph, move |cx| {
        let context = cx.context(&rows, view);
        let settings = merged_options(options, context);
        let (mut lo, mut hi) = (lo, hi);
        if home {
            let w = cx.engine.graph_home(
                input.clone(),
                GraphOptions {
                    domain_lo: None,
                    domain_hi: None,
                    sample_count: None,
                    find_roots: true,
                    aspect: Some(PLOT_ASPECT),
                },
                settings.clone(),
            );
            if w.hi > w.lo {
                (lo, hi) = (w.lo, w.hi);
            }
        }
        let result = cx.engine.graph(
            input,
            GraphOptions {
                domain_lo: Some(lo),
                domain_hi: Some(hi),
                sample_count: None,
                find_roots: true,
                aspect: Some(PLOT_ASPECT),
            },
            settings,
        );
        let sample = result.map(|r| {
            let x_ticks = cx.engine.graph_ticks(lo, hi, r.angle_unit, 6);
            let (y0, y1) = if r.y_max > r.y_min {
                (r.y_min, r.y_max)
            } else {
                (-10.0, 10.0)
            };
            let y_ticks = cx.engine.graph_ticks(y0, y1, None, 5);
            Sample::new(r, lo, hi, x_ticks, y_ticks)
        });
        done(move |app| landed(&mut app.model, &expr, home, lo, hi, sample))
    });
}

fn landed(m: &mut BarModel, expr: &str, home: bool, lo: f64, hi: f64, sample: Option<Sample>) {
    let Some(g) = &mut m.graph else { return };
    if g.expression != expr {
        return;
    }
    if home {
        g.home = Some((lo, hi));
    }
    g.x_min = lo;
    g.x_max = hi;
    match sample {
        Some(s) => {
            g.failed = false;
            [_, _, g.y_min, g.y_max] = s.window;
            g.sample = Some(Rc::new(s));
        }
        None => {
            g.failed = true;
            g.sample = None;
        }
    }
    m.touch();
}

/// Moves the x window (a pan or a zoom) and samples it.
fn set_window(m: &mut BarModel, engine: &EngineThread, lo: f64, hi: f64) {
    let Some(g) = &mut m.graph else { return };
    (g.x_min, g.x_max) = clamp_domain(lo, hi);
    g.readout = None;
    m.touch();
    sample(m, engine, false);
}

pub fn zoom(m: &mut BarModel, engine: &EngineThread, factor: f64, t: f64) {
    let Some(g) = &m.graph else { return };
    let (lo, hi) = zoom_at(g.x_min, g.x_max, t, factor);
    set_window(m, engine, lo, hi);
}

pub fn home(m: &mut BarModel, engine: &EngineThread) {
    let Some((lo, hi)) = m.graph.as_ref().and_then(|g| g.home) else {
        return;
    };
    set_window(m, engine, lo, hi);
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

/// A one-line summary for the bench pipe's `state`.
pub fn debug(m: &BarModel) -> serde_json::Value {
    let Some(g) = &m.graph else {
        return serde_json::Value::Null;
    };
    let s = g.sample.as_ref();
    serde_json::json!({
        "expression": g.expression, "window": [g.x_min, g.x_max, g.y_min, g.y_max], "home": g.home.map(|h| [h.0, h.1]),
        "failed": g.failed, "label": s.map(|s| s.label.clone()), "error": s.and_then(|s| s.error.clone()),
        "points": s.map_or(0, |s| s.points.len()), "roots": s.map(|s| s.roots.clone()),
        "critical": s.map_or(0, |s| s.critical.len()), "contours": s.map_or(0, |s| s.contours.len()),
        "readout": g.readout.as_ref().map(|r| r.text.clone()),
    })
}

// ---- the view ----

pub use view::*;

mod view {
    use super::*;
    use crate::app::App;
    use crate::render::{Canvas, Color, Gfx, Rect, measure, size, with_alpha};
    use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
    use windows::Win32::Graphics::Direct2D::Common::{
        D2D_RECT_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN,
    };
    use windows::Win32::Graphics::Direct2D::{
        D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_ELLIPSE, D2D1_LINE_JOIN_ROUND,
        D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, ID2D1PathGeometry, ID2D1StrokeStyle,
    };
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR, DWRITE_FONT_WEIGHT_SEMI_BOLD,
        IDWriteTextFormat, IDWriteTextLayout,
    };
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows::Win32::UI::WindowsAndMessaging::{IDC_ARROW, IDC_HAND, IDC_SIZEWE};
    use windows::core::PCWSTR;
    use windows_numerics::{Matrix3x2, Vector2};

    /// The panel's own text formats and stroke style, made the first time a graph shows.
    struct Res {
        tick: IDWriteTextFormat,
        readout: IDWriteTextFormat,
        label: IDWriteTextFormat,
        caption: IDWriteTextFormat,
        kind: IDWriteTextFormat,
        round: Option<ID2D1StrokeStyle>,
    }

    thread_local! {
        static RES: RefCell<Option<Rc<Res>>> = const { RefCell::new(None) };
    }

    fn res(gfx: &Gfx) -> Option<Rc<Res>> {
        RES.with(|r| {
            if let Some(res) = &*r.borrow() {
                return Some(res.clone());
            }
            let fam = gfx.fonts.text_family.as_str();
            let f = |size: f32, w| crate::render::format(&gfx.dw, fam, size, w).ok();
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            };
            // SAFETY: the properties outlive the call
            let round = unsafe { gfx.d2d.CreateStrokeStyle(&props, None) }.ok();
            let res = Rc::new(Res {
                tick: f(10.0, DWRITE_FONT_WEIGHT_REGULAR)?,
                readout: f(11.0, DWRITE_FONT_WEIGHT_REGULAR)?,
                label: f(size::BODY, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
                caption: f(size::CAPTION, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
                kind: f(size::CAPTION, DWRITE_FONT_WEIGHT_MEDIUM)?,
                round,
            });
            *r.borrow_mut() = Some(res.clone());
            Some(res)
        })
    }

    // the panel's box (.graph): a hairline on top, padding 12 / gutter / 16, a 20-high head, 8 under it
    const TOP: f32 = 1.0 + 12.0;
    const HEAD_H: f32 = 20.0;
    const BODY_Y: f32 = TOP + HEAD_H + 8.0;
    const BOTTOM: f32 = 16.0;
    const CHIP_H: f32 = 22.0;
    const CHIP_GAP: f32 = 4.0;
    const BUTTON_W: f32 = 26.0;
    const RESET_W: f32 = 44.0;

    struct Chip {
        rect: Rect,
        kind: IDWriteTextLayout,
        kind_w: f32,
        coords: IDWriteTextLayout,
        x: f64,
    }

    /// Where everything goes for a width and a sample, panel-relative.
    struct Geo {
        height: f32,
        label: Option<(IDWriteTextLayout, f32)>,
        /// the error (or "can't graph"), in place of the plot
        message: Option<IDWriteTextLayout>,
        svg: Option<Rect>,
        titles: Vec<(f32, IDWriteTextLayout)>,
        empty: Option<(f32, IDWriteTextLayout)>,
        chips: Vec<Chip>,
        /// zoom out, zoom in, reset
        buttons: [Rect; 3],
    }

    type GeoKey = (u32, u64, bool, String);

    struct Paths {
        serial: u64,
        curve: Option<ID2D1PathGeometry>,
        dots: Vec<(f32, f32)>,
        levels: Vec<(ID2D1PathGeometry, f32)>,
    }

    #[derive(Default)]
    pub(super) struct Cache {
        geo: Option<(GeoKey, Rc<Geo>)>,
        paths: Option<Rc<Paths>>,
        texts: HashMap<String, (IDWriteTextLayout, f32)>,
        texts_serial: u64,
    }

    fn lay(gfx: &Gfx, s: &str, f: &IDWriteTextFormat, h: f32) -> Option<(IDWriteTextLayout, f32)> {
        let l = gfx.layout(s, f, 0.0, h)?;
        let w = measure(&l).0;
        Some((l, w))
    }

    fn geo(gfx: &Gfx, g: &GraphState, width: f32) -> Option<Rc<Geo>> {
        let serial = g.sample.as_ref().map_or(0, |s| s.serial);
        let key: GeoKey = (width.to_bits(), serial, g.failed, g.expression.clone());
        if let Some((k, geo)) = &g.cache.borrow().geo
            && *k == key
        {
            return Some(geo.clone());
        }
        let res = res(gfx)?;
        let f = &gfx.fonts;
        let inner = width - 2.0 * size::GUTTER;
        let label = lay(gfx, &g.label(), &res.label, HEAD_H);
        let right = width - size::GUTTER + 8.0;
        let buttons = [
            Rect::new(right - RESET_W - 2.0 * BUTTON_W, TOP, BUTTON_W, HEAD_H),
            Rect::new(right - RESET_W - BUTTON_W, TOP, BUTTON_W, HEAD_H),
            Rect::new(right - RESET_W, TOP, RESET_W, HEAD_H),
        ];
        let sample = g.sample.as_deref();
        let message = match sample.and_then(|s| s.error.as_deref()) {
            Some(e) => Some(e.to_string()),
            None if g.failed => Some(format!("can’t graph {}", g.expression)),
            None => None,
        };
        let mut geo = Geo {
            height: 0.0,
            label,
            message: None,
            svg: None,
            titles: Vec::new(),
            empty: None,
            chips: Vec::new(),
            buttons,
        };
        let mut y = BODY_Y;
        if let Some(m) = message {
            geo.message = gfx.layout_in(
                &m,
                &f.body,
                inner,
                18.0,
                windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
            );
            y += 12.0 + 18.0 + 12.0;
        } else {
            let h = (inner * SVG_H / SVG_W).min(SVG_H);
            let w = h * SVG_W / SVG_H;
            geo.svg = Some(Rect::new(size::GUTTER, y, w, h));
            y += h;
            // .graph-detail: critical points (or "None in this domain"), then roots, as chips
            if let Some(s) = sample.filter(|s| !s.contour) {
                y += 8.0;
                let section = |title: &str,
                               items: Vec<(&'static str, String, f64)>,
                               y: &mut f32,
                               geo: &mut Geo,
                               empty: bool| {
                    *y += 12.0;
                    if let Some((l, _)) = lay(gfx, title, &res.caption, 14.0) {
                        geo.titles.push((*y, l));
                    }
                    *y += 14.0 + 6.0;
                    if items.is_empty() {
                        if empty
                            && let Some((l, _)) = lay(gfx, "None in this domain", &f.small, 16.0)
                        {
                            geo.empty = Some((*y, l));
                            *y += 16.0;
                        }
                        return;
                    }
                    let built: Vec<_> = items
                        .into_iter()
                        .filter_map(|(kind, coords, x)| {
                            let (kl, kw) = lay(gfx, kind, &res.kind, CHIP_H)?;
                            let (cl, cw) = lay(gfx, &coords, &f.small, CHIP_H)?;
                            Some((kl, kw, cl, cw, x))
                        })
                        .collect();
                    let widths: Vec<f32> =
                        built.iter().map(|b| 8.0 + b.1 + 6.0 + b.3 + 8.0).collect();
                    let places = wrap(&widths, inner, CHIP_GAP);
                    let lines = places.last().map_or(0, |p| p.1 + 1);
                    for ((kl, kw, cl, _, x), ((cx, line), w)) in
                        built.into_iter().zip(places.into_iter().zip(widths))
                    {
                        geo.chips.push(Chip {
                            rect: Rect::new(
                                size::GUTTER + cx,
                                *y + line as f32 * (CHIP_H + CHIP_GAP),
                                w,
                                CHIP_H,
                            ),
                            kind: kl,
                            kind_w: kw,
                            coords: cl,
                            x,
                        });
                    }
                    *y += lines as f32 * (CHIP_H + CHIP_GAP) - CHIP_GAP;
                };
                let deg = g.deg();
                let crit = s
                    .critical
                    .iter()
                    .map(|c| {
                        (
                            kind_name(c.kind),
                            format!("({}{deg}, {})", fmt_g(c.x, 8), fmt_g(c.y, 8)),
                            c.x,
                        )
                    })
                    .collect();
                section("Critical points", crit, &mut y, &mut geo, s.error.is_none());
                if !s.roots.is_empty() {
                    let roots = s
                        .roots
                        .iter()
                        .map(|&x| ("root", format!("x = {}{deg}", fmt_g(x, 8)), x))
                        .collect();
                    section("Roots", roots, &mut y, &mut geo, false);
                }
            }
        }
        geo.height = y + BOTTOM;
        let geo = Rc::new(geo);
        g.cache.borrow_mut().geo = Some((key, geo.clone()));
        Some(geo)
    }

    /// The panel's height at this width (window.rs `Bar::layout`).
    pub fn height(gfx: &Gfx, g: &GraphState, width: f32) -> f32 {
        geo(gfx, g, width).map_or(TOP + HEAD_H + BOTTOM, |g| g.height)
    }

    fn paths(gfx: &Gfx, g: &GraphState) -> Option<Rc<Paths>> {
        let s = g.sample.as_ref()?;
        if let Some(p) = &g.cache.borrow().paths
            && p.serial == s.serial
        {
            return Some(p.clone());
        }
        let (lines, dots) = runs(&s.points, s.window);
        let [x0, x1, y0, y1] = s.window;
        let curve = path(gfx, lines.iter().map(|l| l.as_slice()));
        let n = s.contours.len();
        let levels = s
            .contours
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let segs: Vec<[(f32, f32); 2]> = c
                    .segments
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|q| {
                        let p = |x: f64, y: f64| {
                            (((x - x0) / (x1 - x0)) as f32, ((y1 - y) / (y1 - y0)) as f32)
                        };
                        [p(q[0], q[1]), p(q[2], q[3])]
                    })
                    .collect();
                let geo = path(gfx, segs.iter().map(|s| s.as_slice()))?;
                let opacity = if n > 1 {
                    0.35 + 0.65 * i as f32 / (n - 1) as f32
                } else {
                    1.0
                };
                Some((geo, opacity))
            })
            .collect();
        let p = Rc::new(Paths {
            serial: s.serial,
            curve,
            dots,
            levels,
        });
        g.cache.borrow_mut().paths = Some(p.clone());
        Some(p)
    }

    /// One path geometry of open polylines.
    fn path<'a>(
        gfx: &Gfx,
        lines: impl Iterator<Item = &'a [(f32, f32)]>,
    ) -> Option<ID2D1PathGeometry> {
        // SAFETY: a fresh geometry, opened, filled and closed here; the points are locals
        unsafe {
            let geo = gfx.d2d.CreatePathGeometry().ok()?;
            let sink = geo.Open().ok()?;
            let mut any = false;
            let mut pts: Vec<Vector2> = Vec::new();
            for line in lines {
                let Some((&(u, v), rest)) = line.split_first() else {
                    continue;
                };
                if rest.is_empty() {
                    continue;
                }
                sink.BeginFigure(Vector2 { X: u, Y: v }, D2D1_FIGURE_BEGIN_HOLLOW);
                pts.clear();
                pts.extend(rest.iter().map(|&(x, y)| Vector2 { X: x, Y: y }));
                sink.AddLines(&pts);
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                any = true;
            }
            sink.Close().ok()?;
            any.then_some(geo)
        }
    }

    /// A cached one-line layout of `s` in format `f` (tick and contour labels: the same strings every frame).
    fn text(
        c: &Canvas,
        g: &GraphState,
        s: &str,
        f: &IDWriteTextFormat,
        h: f32,
    ) -> Option<(IDWriteTextLayout, f32)> {
        let serial = g.sample.as_ref().map_or(0, |s| s.serial);
        let mut cache = g.cache.borrow_mut();
        if cache.texts_serial != serial {
            cache.texts.clear();
            cache.texts_serial = serial;
        }
        if let Some(t) = cache.texts.get(s) {
            return Some(t.clone());
        }
        let t = lay(c.gfx, s, f, h)?;
        cache.texts.insert(s.to_string(), t.clone());
        Some(t)
    }

    /// --graph-plot: color-mix(bg 70%, bg-tape)
    fn plot_bg(c: &Canvas) -> Color {
        let (a, b) = (c.theme.bg, c.theme.bg_tape);
        Color {
            r: a.r * 0.7 + b.r * 0.3,
            g: a.g * 0.7 + b.g * 0.3,
            b: a.b * 0.7 + b.b * 0.3,
            a: 1.0,
        }
    }

    fn ellipse(c: &Canvas, x: f32, y: f32, r: f32, fill: Color, stroke: Option<(Color, f32)>) {
        let e = D2D1_ELLIPSE {
            point: Vector2 { X: x, Y: y },
            radiusX: r,
            radiusY: r,
        };
        // SAFETY: inside the frame's BeginDraw/EndDraw; the ellipse is a local
        unsafe {
            c.target().FillEllipse(&e, c.brush(fill));
            if let Some((sc, w)) = stroke {
                c.target().DrawEllipse(&e, c.brush(sc), w, None);
            }
        }
    }

    /// The panel in `r` (window DIPs).
    pub fn paint(c: &Canvas, g: &GraphState, r: Rect) {
        let Some(res) = res(c.gfx) else { return };
        let Some(geo) = geo(c.gfx, g, r.w) else {
            return;
        };
        let t = c.theme;
        c.fill(r, t.bg_tape);
        c.fill(Rect::new(r.x, r.y, r.w, 1.0), t.line);
        // the head: label, domain, angle unit, then − + reset on the right
        let mut x = r.x + size::GUTTER;
        let hy = r.y + TOP;
        if let Some((l, w)) = &geo.label {
            c.text(l, x, hy, t.ink);
            x += w + 12.0;
        }
        let res_x = (g.x_max - g.x_min) / f64::from(PLOT_W);
        let domain = format!(
            "[{}, {}]",
            fmt_g(round_to_pixel(g.x_min, res_x), 8),
            fmt_g(round_to_pixel(g.x_max, res_x), 8)
        );
        let room = r.x + geo.buttons[0].x - x - 8.0;
        if let Some(l) = c.gfx.layout_in(
            &domain,
            &c.gfx.fonts.small,
            room.max(1.0),
            HEAD_H,
            windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
        ) {
            let w = measure(&l).0.min(room);
            c.text(&l, x, hy, t.muted);
            x += w + 12.0;
        }
        if let Some(unit) = g.sample.as_ref().and_then(|s| s.unit)
            && let Some((l, _)) = text(
                c,
                g,
                if unit == AngleUnit::Rad { "rad" } else { "deg" },
                &c.gfx.fonts.small,
                HEAD_H,
            )
        {
            c.text(&l, x, hy, t.muted);
        }
        let hover = g.hover;
        let at_home = g.at_home();
        for (i, b) in geo.buttons.iter().enumerate() {
            let br = Rect::new(r.x + b.x, r.y + b.y, b.w, b.h);
            let enabled = i < 2 || !at_home;
            let over = hover.is_some_and(|(hx, hy)| b.contains(hx, hy));
            if over && enabled {
                c.fill_round(br, size::RADIUS_S, t.fill_hover);
            }
            let ink = if !enabled {
                with_alpha(t.muted, 0.4)
            } else if over {
                t.ink
            } else {
                t.muted
            };
            let s = ["−", "+", "reset"][i];
            if let Some((l, w)) = text(c, g, s, &c.gfx.fonts.small, HEAD_H) {
                c.text(&l, br.x + (br.w - w) / 2.0, br.y, ink);
            }
        }
        if let Some(m) = &geo.message {
            c.text(m, r.x + size::GUTTER, r.y + BODY_Y + 12.0, t.muted);
        }
        if let Some(svg) = geo.svg {
            plot(
                c,
                g,
                &res,
                Rect::new(r.x + svg.x, r.y + svg.y, svg.w, svg.h),
            );
        }
        for (y, l) in &geo.titles {
            c.text(l, r.x + size::GUTTER, r.y + y, t.muted);
        }
        if let Some((y, l)) = &geo.empty {
            c.text(l, r.x + size::GUTTER, r.y + y, t.muted);
        }
        for chip in &geo.chips {
            let cr = Rect::new(
                r.x + chip.rect.x,
                r.y + chip.rect.y,
                chip.rect.w,
                chip.rect.h,
            );
            let over = hover.is_some_and(|(hx, hy)| chip.rect.contains(hx, hy));
            // .graph-detail-item: fill-track, accent 12% on hover
            let fill = if over {
                with_alpha(t.accent, 0.12)
            } else {
                with_alpha(t.ink, 0.06)
            };
            c.fill_round(cr, size::RADIUS_S, fill);
            c.text(
                &chip.kind,
                cr.x + 8.0,
                cr.y,
                if over { t.accent } else { t.muted },
            );
            c.text(
                &chip.coords,
                cr.x + 8.0 + chip.kind_w + 6.0,
                cr.y,
                if over { t.accent } else { t.ink },
            );
        }
    }

    /// The plot: background, grid, ticks, axes, curve or level curves, marks and the readout, in `svg` (the
    /// viewBox's rect, window DIPs).
    fn plot(c: &Canvas, g: &GraphState, res: &Res, svg: Rect) {
        let t = c.theme;
        let s = svg.w / SVG_W;
        let (left, top, pw, ph) = (
            svg.x + PAD_LEFT * s,
            svg.y + PAD_TOP * s,
            PLOT_W * s,
            PLOT_H * s,
        );
        let (right, bottom) = (left + pw, top + ph);
        let px = |x: f64| svg.x + g.vx(x) * s;
        let py = |y: f64| svg.y + g.vy(y) * s;
        let bg = plot_bg(c);
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left,
                top,
                right,
                bottom,
            },
            radiusX: 4.0,
            radiusY: 4.0,
        };
        // SAFETY: inside the frame; the rect is a local
        unsafe { c.target().FillRoundedRectangle(&rr, c.brush(bg)) };
        let grid = with_alpha(t.ink, 0.05);
        let axis = with_alpha(t.ink, 0.18);
        let crisp = |v: f32| (v * 2.0).round() / 2.0 + 0.25;
        if let Some(sample) = &g.sample {
            for tk in sample
                .x_ticks
                .iter()
                .filter(|tk| tk.value >= g.x_min && tk.value <= g.x_max)
            {
                let x = crisp(px(tk.value));
                c.line(x, top, x, bottom, 1.0, grid);
                if let Some((l, w)) = text(c, g, &tk.label.replace('-', "−"), &res.tick, 12.0) {
                    c.text_plain(&l, x - w / 2.0, svg.y + (SVG_H - 12.0) * s - 8.0, t.muted);
                }
            }
            for tk in sample
                .y_ticks
                .iter()
                .filter(|tk| tk.value >= g.y_min && tk.value <= g.y_max)
            {
                let y = crisp(py(tk.value));
                c.line(left, y, right, y, 1.0, grid);
                if let Some((l, w)) = text(c, g, &tk.label.replace('-', "−"), &res.tick, 12.0) {
                    c.text_plain(&l, left - 6.0 * s - w, y - 6.0, t.muted);
                }
            }
        }
        let ay = py(0.0);
        if (top..=bottom).contains(&ay) {
            let y = crisp(ay);
            c.line(left, y, right, y, 1.0, axis);
        }
        let ax = px(0.0);
        if (left..=right).contains(&ax) {
            let x = crisp(ax);
            c.line(x, top, x, bottom, 1.0, axis);
        }
        c.push_clip(Rect::new(left, top, pw, ph));
        let readout = g.readout.as_ref();
        if let Some(r) = readout.filter(|r| !r.contour) {
            let x = crisp(px(r.x));
            c.line(x, top, x, bottom, 1.0, with_alpha(t.ink, 0.22));
        }
        if let (Some(sample), Some(paths)) = (&g.sample, paths(c.gfx, g)) {
            let [x0, x1, y0, y1] = sample.window;
            let (xw, yh) = (g.x_max - g.x_min, g.y_max - g.y_min);
            let m = Matrix3x2 {
                M11: ((x1 - x0) / xw) as f32 * pw,
                M12: 0.0,
                M21: 0.0,
                M22: ((y1 - y0) / yh) as f32 * ph,
                M31: left + ((x0 - g.x_min) / xw) as f32 * pw,
                M32: top + ((g.y_max - y1) / yh) as f32 * ph,
            };
            let draw = |geo: &ID2D1PathGeometry, color: Color, width: f32| {
                // SAFETY: inside the frame; the matrix is a local and the geometry lives in the cache
                unsafe {
                    if let Ok(xf) = c.gfx.d2d.CreateTransformedGeometry(geo, &m) {
                        c.target()
                            .DrawGeometry(&xf, c.brush(color), width, res.round.as_ref());
                    }
                }
            };
            if let Some(curve) = &paths.curve {
                draw(curve, t.accent, 2.0);
            }
            for (geo, opacity) in &paths.levels {
                draw(geo, with_alpha(t.accent, *opacity), 1.5);
            }
            let map = |(u, v): (f32, f32)| (u * m.M11 + m.M31, v * m.M22 + m.M32);
            for &d in &paths.dots {
                let (x, y) = map(d);
                ellipse(c, x, y, 2.0, t.accent, None);
            }
            // level labels on a halo of the plot colour
            for lv in &sample.contours {
                let (Some(ax), Some(ay)) = (lv.at_x, lv.at_y) else {
                    continue;
                };
                let (x, y) = (px(ax), py(ay));
                if let Some((l, w)) = text(c, g, &lv.label.replace('-', "−"), &res.tick, 12.0) {
                    c.fill_round(
                        Rect::new(x - w / 2.0 - 2.0, y - 6.0, w + 4.0, 12.0),
                        3.0,
                        bg,
                    );
                    c.text_plain(&l, x - w / 2.0, y - 6.0, t.muted);
                }
            }
            // .graph-mark: roots and extrema, rings of the plot colour with an accent edge
            if !sample.contour {
                for (x, y, _) in g.features() {
                    ellipse(c, px(x), py(y), 2.75, bg, Some((t.accent, 1.5)));
                }
            }
        }
        if let Some(r) = readout.filter(|r| !r.contour)
            && let Some(y) = r.y
        {
            let (x, y) = (px(r.x), py(y));
            if (top..=bottom).contains(&y) {
                ellipse(c, x, y, 3.5, t.accent, Some((bg, 1.5)));
            }
        }
        c.pop_clip();
        // the readout's text near the top, on the side away from the edge
        if let Some(r) = readout {
            let rx = match g.hover {
                Some((hx, _)) if r.contour => hx,
                _ => px(r.x),
            };
            let on_right = rx > left + pw * 0.62;
            let kind = r
                .kind
                .and_then(|k| text(c, g, &format!("{k} "), &res.readout, 14.0));
            let main = lay(c.gfx, &r.text, &res.readout, 14.0);
            if let Some((l, w)) = main {
                let kw = kind.as_ref().map_or(0.0, |k| k.1);
                let total = kw + w;
                let x0 = if on_right { rx - 7.0 - total } else { rx + 7.0 };
                let y0 = top + 13.0 * s - 10.0;
                c.fill_round(
                    Rect::new(x0 - 2.0, y0, total + 4.0, 14.0),
                    3.0,
                    with_alpha(bg, 0.9),
                );
                if let Some((k, _)) = &kind {
                    c.text_plain(k, x0, y0, t.muted);
                }
                c.text_plain(&l, x0 + kw, y0, t.ink);
            }
        }
    }

    // ---- the pointer ----

    fn panel(app: &App) -> Option<Rect> {
        app.bar.layout.graph
    }

    /// The svg's rect (panel-relative) and the point in viewBox units.
    type Svg = Option<(Rect, f32, f32)>;

    /// (the panel-relative point, the plot's `Svg`)
    fn locate(app: &App, x: f32, y: f32) -> Option<((f32, f32), Svg)> {
        let r = panel(app)?;
        let g = app.model.graph.as_ref()?;
        let geo = g.cache.borrow().geo.as_ref().map(|(_, geo)| geo.clone())?;
        let p = (x - r.x, y - r.y);
        let svg = geo.svg.map(|svg| {
            let s = svg.w / SVG_W;
            (svg, (p.0 - svg.x) / s, (p.1 - svg.y) / s)
        });
        Some((p, svg))
    }

    fn in_plot(vx: f32, vy: f32) -> bool {
        (PAD_LEFT..=PAD_LEFT + PLOT_W).contains(&vx) && (PAD_TOP..=PAD_TOP + PLOT_H).contains(&vy)
    }

    /// A press in the panel: a button, a chip, or the start of a drag on the plot. False outside it.
    pub fn click(app: &mut App, x: f32, y: f32, double: bool) -> bool {
        let Some(r) = panel(app) else { return false };
        if !r.contains(x, y) {
            return false;
        }
        let Some((p, svg)) = locate(app, x, y) else {
            return true;
        };
        let geo = app
            .model
            .graph
            .as_ref()
            .and_then(|g| g.cache.borrow().geo.as_ref().map(|(_, geo)| geo.clone()));
        let Some(geo) = geo else { return true };
        if let Some(i) = geo.buttons.iter().position(|b| b.contains(p.0, p.1)) {
            match i {
                0 => zoom(&mut app.model, &app.engine, 1.5, 0.5),
                1 => zoom(&mut app.model, &app.engine, 1.0 / 1.5, 0.5),
                _ => home(&mut app.model, &app.engine),
            }
            return true;
        }
        if let Some(chip) = geo.chips.iter().find(|c| c.rect.contains(p.0, p.1)) {
            copy_x(&mut app.model, chip.x);
            return true;
        }
        if let Some((svg, vx, _)) = svg
            && svg.contains(p.0, p.1)
        {
            if double {
                home(&mut app.model, &app.engine);
                return true;
            }
            if let Some(g) = &mut app.model.graph {
                g.drag = Some(Drag {
                    start: vx,
                    domain: (g.x_min, g.x_max),
                    moved: false,
                });
            }
            // SAFETY: our bar
            unsafe { SetCapture(app.bar.hwnd) };
        }
        true
    }

    fn copy_x(m: &mut BarModel, x: f64) {
        let text = fmt_g(x, 8).replace('−', "-");
        m.hint = Some(format!("copied {text}"));
        m.to_clipboard = Some(text);
        m.touch();
    }

    /// The pointer moved over the bar: drag the window, or follow the readout and the buttons' hover.
    pub fn mouse_move(app: &mut App, x: f32, y: f32) -> bool {
        let Some(g) = &app.model.graph else {
            return false;
        };
        if let Some(drag) = g.drag {
            let Some((_, Some((_, vx, _)))) = locate(app, x, y) else {
                return true;
            };
            let dx = vx - drag.start;
            if !drag.moved && dx.abs() < 3.0 {
                return true;
            }
            let (lo, hi) = drag.domain;
            let shift = -f64::from(dx / PLOT_W) * (hi - lo);
            if let Some(g) = &mut app.model.graph {
                g.drag = Some(Drag {
                    moved: true,
                    ..drag
                });
                g.hover = None;
            }
            set_window(&mut app.model, &app.engine, lo + shift, hi + shift);
            return true;
        }
        let inside = panel(app).is_some_and(|r| r.contains(x, y));
        let Some((p, svg)) = locate(app, x, y).filter(|_| inside) else {
            return leave(app);
        };
        let was = app.model.graph.as_ref().and_then(|g| g.hover);
        if was.is_none() {
            track_leave(app);
        }
        if let Some(g) = &mut app.model.graph {
            g.hover = Some(p);
        }
        app.model.touch();
        match svg {
            Some((_, vx, vy)) if in_plot(vx, vy.clamp(PAD_TOP, PAD_TOP + PLOT_H)) => {
                readout(app, vx, vy)
            }
            _ => {
                if let Some(g) = &mut app.model.graph {
                    g.readout = None;
                }
                app.engine.cancel(Lane::GraphReadout);
            }
        }
        true
    }

    fn track_leave(app: &App) {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: app.bar.hwnd,
            dwHoverTime: 0,
        };
        // SAFETY: a live local struct; our bar
        let _ = unsafe { TrackMouseEvent(&mut tme) };
    }

    /// The readout for the pointer at viewBox (vx, vy): a snapped point at once, else the engine's y (or z).
    fn readout(app: &mut App, vx: f32, vy: f32) {
        let Some(g) = &mut app.model.graph else {
            return;
        };
        let Some(sample) = g.sample.clone() else {
            return;
        };
        if sample.error.is_some() {
            return;
        }
        let deg = g.deg();
        let span = g.x_max - g.x_min;
        let x = round_to_pixel(g.x_at(vx), span / f64::from(PLOT_W));
        if sample.contour {
            let vy = vy.clamp(PAD_TOP, PAD_TOP + PLOT_H);
            let y = round_to_pixel(g.y_at(vy), (g.y_max - g.y_min) / f64::from(PLOT_H));
            let point = format!("({}{deg}, {}{deg})", fmt_g(x, 8), fmt_g(y, 8));
            if !sample.levels {
                g.readout = Some(Readout {
                    x,
                    y: Some(y),
                    kind: None,
                    text: point,
                    contour: true,
                });
                return;
            }
            let (input, window) = (g.input(), (g.x_min, g.x_max));
            ask(app, move |cx, settings| {
                let z = cx
                    .engine
                    .graph_readout_z(input, x, y, options(window), settings);
                let text = format!("{point}  z = {}", z.map_or("—".into(), |z| fmt_g(z, 6)));
                Readout {
                    x,
                    y: Some(y),
                    kind: None,
                    text,
                    contour: true,
                }
            });
            return;
        }
        if let Some((fx, fy, kind)) = g.snap(vx) {
            g.readout = Some(Readout {
                x: fx,
                y: Some(fy),
                kind: Some(kind),
                text: format!("({}{deg}, {})", fmt_g(fx, 8), fmt_g(fy, 8)),
                contour: false,
            });
            app.engine.cancel(Lane::GraphReadout);
            return;
        }
        let (input, window) = (g.input(), (g.x_min, g.x_max));
        ask(app, move |cx, settings| {
            let y = cx
                .engine
                .graph_readout_y(input, x, options(window), settings);
            let text = format!(
                "({}{deg}, {})",
                fmt_g(x, 8),
                y.map_or("—".into(), |y| fmt_g(y, 6))
            );
            Readout {
                x,
                y,
                kind: None,
                text,
                contour: false,
            }
        });
    }

    fn options((lo, hi): (f64, f64)) -> GraphOptions {
        GraphOptions {
            domain_lo: Some(lo),
            domain_hi: Some(hi),
            sample_count: None,
            find_roots: false,
            aspect: Some(PLOT_ASPECT),
        }
    }

    /// A readout from the engine, on its own lane (only the newest pointer position is answered).
    fn ask(
        app: &App,
        f: impl FnOnce(&mut crate::engine::Cx, qcalc_ffi::EvalOptions) -> Readout + Send + 'static,
    ) {
        let m = &app.model;
        let Some(expr) = m.graph.as_ref().map(|g| g.expression.clone()) else {
            return;
        };
        let rows = m.rows.clone();
        let view = m.view(now_ms());
        let options = m.options();
        app.engine.submit(Lane::GraphReadout, move |cx| {
            let context = cx.context(&rows, view);
            let r = f(cx, merged_options(options, context));
            done(move |app| {
                if let Some(g) = &mut app.model.graph
                    && g.expression == expr
                    && g.hover.is_some()
                    && g.drag.is_none()
                {
                    g.readout = Some(r);
                    app.model.touch();
                }
            })
        });
    }

    /// The press ended: a drag stops; a click without one on a snapped point copies its x.
    pub fn mouse_up(app: &mut App, x: f32, y: f32) -> bool {
        let Some(drag) = app.model.graph.as_mut().and_then(|g| g.drag.take()) else {
            return false;
        };
        // SAFETY: we hold the capture
        let _ = unsafe { ReleaseCapture() };
        if !drag.moved
            && let Some((_, Some((_, vx, _)))) = locate(app, x, y)
            && let Some((fx, _, _)) = app.model.graph.as_ref().and_then(|g| g.snap(vx))
        {
            copy_x(&mut app.model, fx);
        }
        app.model.touch();
        mouse_move(app, x, y);
        true
    }

    /// The pointer left the bar (WM_MOUSELEAVE) or the panel.
    pub fn leave(app: &mut App) -> bool {
        let Some(g) = &mut app.model.graph else {
            return false;
        };
        if g.hover.is_some() || g.readout.is_some() {
            g.hover = None;
            g.readout = None;
            app.engine.cancel(Lane::GraphReadout);
            app.model.touch();
        }
        false
    }

    /// WM_MOUSEWHEEL / WM_MOUSEHWHEEL over the plot: up and down (and a touchpad's pinch, which arrives with
    /// Ctrl) zoom about the pointer, sideways pans. False when the wheel isn't over the plot.
    pub fn wheel(app: &mut App, horizontal: bool, wp: WPARAM, lp: LPARAM) -> bool {
        if app.model.graph.is_none() {
            return false;
        }
        let mut pt = POINT {
            x: i32::from((lp.0 & 0xffff) as u16 as i16),
            y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
        };
        // SAFETY: a live point; our bar
        let _ = unsafe { ScreenToClient(app.bar.hwnd, &mut pt) };
        let s = app.bar.scale();
        let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
        let Some((_, Some((svg, vx, _)))) = locate(app, x, y) else {
            return false;
        };
        let Some(r) = panel(app) else { return false };
        if !svg.contains(x - r.x, y - r.y) {
            return false;
        }
        let delta = f64::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
        let ctrl = wp.0 & 0x0008 != 0;
        let Some(g) = &app.model.graph else {
            return false;
        };
        // a notch (120) as a browser's 100 px of deltaY
        let px = -delta / 120.0 * 100.0;
        if horizontal {
            let shift = -px / f64::from(PLOT_W) * (g.x_max - g.x_min);
            let (lo, hi) = (g.x_min + shift, g.x_max + shift);
            set_window(&mut app.model, &app.engine, lo, hi);
            return true;
        }
        let k = if ctrl { 0.01 } else { 0.002 };
        let factor = (px * k).clamp(-0.5, 0.5).exp();
        let t = f64::from((vx - PAD_LEFT) / PLOT_W);
        zoom(&mut app.model, &app.engine, factor, t);
        true
    }

    /// The cursor over the panel: a hand on a button, chip or snapped point, ↔ on the plot.
    pub fn cursor(app: &App, x: f32, y: f32) -> Option<PCWSTR> {
        let r = panel(app)?;
        if !r.contains(x, y) {
            return None;
        }
        let g = app.model.graph.as_ref()?;
        let (p, svg) = locate(app, x, y)?;
        let geo = g.cache.borrow().geo.as_ref().map(|(_, geo)| geo.clone())?;
        if geo.buttons.iter().any(|b| b.contains(p.0, p.1))
            || geo.chips.iter().any(|c| c.rect.contains(p.0, p.1))
        {
            return Some(IDC_HAND);
        }
        match svg {
            Some((svg, vx, vy)) if svg.contains(p.0, p.1) && in_plot(vx, vy) => {
                Some(if g.drag.is_none() && g.snap(vx).is_some() {
                    IDC_HAND
                } else {
                    IDC_SIZEWE
                })
            }
            _ => Some(IDC_ARROW),
        }
    }
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
    fn domains_stay_ordered_and_bounded() {
        assert_eq!(clamp_domain(3.0, -3.0), (-3.0, 3.0));
        assert_eq!(clamp_domain(f64::NAN, 1.0), (-10.0, 10.0));
        let (lo, hi) = clamp_domain(1.0, 1.0);
        assert!((hi - lo - 1e-6).abs() < 1e-12);
        let (lo, hi) = clamp_domain(-1e7, 1e7);
        assert!((hi - lo - 1e6).abs() < 1e-3);
        // zooming about the left edge keeps it
        let (lo, hi) = zoom_at(-10.0, 10.0, 0.0, 0.5);
        assert_eq!((lo, hi), (-10.0, 0.0));
        let (lo, hi) = zoom_at(-10.0, 10.0, 0.5, 2.0);
        assert_eq!((lo, hi), (-20.0, 20.0));
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
        let (lines, dots) = runs(&pts, window);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![(0.0, 0.5), (0.1, 0.25)]);
        // the pole between 5 and 6 splits them: 5 alone is a dot, 6-7 a line
        assert_eq!(lines[1].len(), 2);
        assert_eq!(dots.len(), 2);
        assert_eq!(dots[0], (0.3, 0.5));
        // far samples are pinned
        assert_eq!(dots[1].1, -V_LIMIT);
    }

    #[test]
    fn chips_wrap() {
        assert_eq!(
            wrap(&[50.0, 50.0, 50.0], 120.0, 4.0),
            vec![(0.0, 0), (54.0, 0), (0.0, 1)]
        );
        assert_eq!(wrap(&[200.0], 120.0, 4.0), vec![(0.0, 0)]);
    }
}
