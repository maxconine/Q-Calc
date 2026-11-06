//! The graph's settings, under the head's wrench (the Mac's GraphSettings.swift, Desmos's graph settings): what's
//! kept between launches (`GraphPrefs`, the `graphPrefs` object in settings.json) and what's set per graph
//! (`GraphState`'s labels, windows, steps and log axes). The Mac shows them in a popover; here they open as a
//! block of controls inside the panel, laid out in rows that wrap (graph.rs draws them from `controls`).
//!
//! Number fields read `pi`, `π`, `e` and a multiple of them (`2pi`), and are committed with ↵, Tab or a click
//! elsewhere (NumberField); the label fields apply as they're typed.

use std::cell::Cell;

use serde::{Deserialize, Serialize};

/// The display choices kept between launches (GraphPrefs).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphPrefs {
    pub large_text: bool,
    pub reverse_contrast: bool,
    pub grid: bool,
    pub polar: bool,
    pub arrows: bool,
    pub axis_numbers: bool,
    pub minor_grid: bool,
    pub show_x_axis: bool,
    pub show_y_axis: bool,
    pub lock_viewport: bool,
    /// degrees or radians for the graph's trig; None follows the calculator
    pub degrees: Option<bool>,
}

impl Default for GraphPrefs {
    fn default() -> Self {
        GraphPrefs {
            large_text: false,
            reverse_contrast: false,
            grid: true,
            polar: false,
            arrows: false,
            axis_numbers: true,
            minor_grid: true,
            show_x_axis: true,
            show_y_axis: true,
            lock_viewport: false,
            degrees: None,
        }
    }
}

impl GraphPrefs {
    /// From settings.json's `graphPrefs` (anything missing or odd is the default).
    pub fn from_value(v: &serde_json::Value) -> GraphPrefs {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
}

thread_local! {
    static PREFS: Cell<Option<GraphPrefs>> = const { Cell::new(None) };
}

/// The prefs, read from settings.json the first time a graph needs them.
pub fn prefs() -> GraphPrefs {
    PREFS.with(|p| {
        if let Some(prefs) = p.get() {
            return prefs;
        }
        let prefs = crate::store::settings_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .map(|v| GraphPrefs::from_value(&v["graphPrefs"]))
            .unwrap_or_default();
        p.set(Some(prefs));
        prefs
    })
}

/// Sets the prefs for this run (graph.rs then saves them into the store).
pub fn set_prefs(prefs: GraphPrefs) {
    PREFS.with(|p| p.set(Some(prefs)));
}

/// A text field in the settings, or an added equation's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldId {
    XLabel,
    YLabel,
    XLo,
    XHi,
    /// y's window in 2D, z's in 3D
    YLo,
    YHi,
    XStep,
    YStep,
    /// an equation added with the head's +
    Layer(u32),
}

impl FieldId {
    /// a label field applies as it's typed; the others when committed
    pub fn live(self) -> bool {
        matches!(self, FieldId::XLabel | FieldId::YLabel | FieldId::Layer(_))
    }
}

/// A two- or three-way choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seg {
    TextSize,
    GridKind,
    XScale,
    YScale,
    Angles,
}

/// A checkbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    ReverseContrast,
    Grid,
    Arrows,
    AxisNumbers,
    MinorGrid,
    XAxis,
    YAxis,
    LockViewport,
}

/// One control in the settings block, in reading order; `Break` starts a row.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctl {
    Text(String),
    Check(Check, &'static str, bool),
    Seg(Seg, Vec<&'static str>, usize, bool),
    Field(FieldId, String, &'static str, f32),
    ZoomSquare(bool),
    Break,
}

/// What the block shows for the graph: `three_d` swaps y's window for z's, `log` is (x, y) on log scales.
pub struct View {
    pub three_d: bool,
    pub x_label: String,
    pub y_label: String,
    pub x: (f64, f64),
    /// y's window, or z's in 3D
    pub y: (f64, f64),
    pub x_step: Option<f64>,
    pub y_step: Option<f64>,
    pub log: (bool, bool),
    /// the calculator's angle mode, for "follow the calculator"
    pub radians: bool,
}

/// The controls for `p` and `v` (GraphSettingsView's rows, flattened).
pub fn controls(p: &GraphPrefs, v: &View) -> Vec<Ctl> {
    let mut out = vec![
        Ctl::Seg(
            Seg::TextSize,
            vec!["A", "A+"],
            usize::from(p.large_text),
            true,
        ),
        Ctl::Check(
            Check::ReverseContrast,
            "reverse contrast",
            p.reverse_contrast,
        ),
        Ctl::Seg(
            Seg::Angles,
            vec!["rad", "deg"],
            usize::from(p.degrees.unwrap_or(!v.radians)),
            true,
        ),
        Ctl::Break,
        Ctl::Check(Check::Grid, "grid", p.grid),
        Ctl::Seg(
            Seg::GridKind,
            vec!["cartesian", "polar"],
            usize::from(p.polar),
            p.grid,
        ),
        Ctl::Check(Check::Arrows, "arrows", p.arrows),
        Ctl::Check(Check::AxisNumbers, "axis numbers", p.axis_numbers),
        Ctl::Check(Check::MinorGrid, "minor gridlines", p.minor_grid),
        Ctl::ZoomSquare(!v.log.0 && !v.log.1),
        Ctl::Break,
    ];
    let (y_name, y_lo, y_hi) = if v.three_d {
        ("z", FieldId::YLo, FieldId::YHi)
    } else {
        ("y", FieldId::YLo, FieldId::YHi)
    };
    out.extend([
        Ctl::Check(Check::XAxis, "x-axis", p.show_x_axis),
        Ctl::Field(FieldId::XLabel, v.x_label.clone(), "label", 64.0),
        Ctl::Field(FieldId::XLo, show(Some(v.x.0)), "", 64.0),
        Ctl::Text("≤ x ≤".into()),
        Ctl::Field(FieldId::XHi, show(Some(v.x.1)), "", 64.0),
        Ctl::Text("step".into()),
        Ctl::Field(FieldId::XStep, show(v.x_step), "auto", 48.0),
        Ctl::Break,
        Ctl::Check(Check::YAxis, "y-axis", p.show_y_axis),
        Ctl::Field(FieldId::YLabel, v.y_label.clone(), "label", 64.0),
        Ctl::Field(y_lo, show(Some(v.y.0)), "", 64.0),
        Ctl::Text(format!("≤ {y_name} ≤")),
        Ctl::Field(y_hi, show(Some(v.y.1)), "", 64.0),
        Ctl::Text("step".into()),
        Ctl::Field(FieldId::YStep, show(v.y_step), "auto", 48.0),
        Ctl::Break,
    ]);
    if !v.three_d {
        out.extend([
            Ctl::Text("x".into()),
            Ctl::Seg(
                Seg::XScale,
                vec!["linear", "log"],
                usize::from(v.log.0),
                true,
            ),
            Ctl::Text("y".into()),
            Ctl::Seg(
                Seg::YScale,
                vec!["linear", "log"],
                usize::from(v.log.1),
                true,
            ),
        ]);
    }
    out.push(Ctl::Check(
        Check::LockViewport,
        "lock viewport",
        p.lock_viewport,
    ));
    out
}

/// A checkbox clicked.
pub fn toggle(p: &mut GraphPrefs, c: Check) {
    let slot = match c {
        Check::ReverseContrast => &mut p.reverse_contrast,
        Check::Grid => &mut p.grid,
        Check::Arrows => &mut p.arrows,
        Check::AxisNumbers => &mut p.axis_numbers,
        Check::MinorGrid => &mut p.minor_grid,
        Check::XAxis => &mut p.show_x_axis,
        Check::YAxis => &mut p.show_y_axis,
        Check::LockViewport => &mut p.lock_viewport,
    };
    *slot = !*slot;
}

/// NumberField.show: `%.6g`, blank for none.
pub fn show(v: Option<f64>) -> String {
    match v {
        None => String::new(),
        Some(v) => {
            let v = if v.abs() < 1e-12 { 0.0 } else { v };
            crate::graph::fmt_g(v, 6).replace('−', "-")
        }
    }
}

/// NumberField.parse: a number, or `pi` / `π` / `e` and a multiple of them (`2pi`, `-e`, `3*pi`).
pub fn parse(raw: &str) -> Option<f64> {
    let s: String = raw
        .trim()
        .replace('−', "-")
        .replace('π', "pi")
        .chars()
        .filter(|c| *c != ' ')
        .collect();
    if s.is_empty() {
        return None;
    }
    if let Ok(d) = s.parse::<f64>() {
        return d.is_finite().then_some(d);
    }
    for (name, k) in [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)] {
        if let Some(rest) = s.strip_suffix(name) {
            let rest = rest.strip_suffix('*').unwrap_or(rest);
            return match rest {
                "" => Some(k),
                "-" => Some(-k),
                m => m
                    .parse::<f64>()
                    .ok()
                    .filter(|m| m.is_finite())
                    .map(|m| m * k),
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_pi_and_e() {
        assert_eq!(parse(" 2.5 "), Some(2.5));
        assert_eq!(parse("−3"), Some(-3.0));
        assert_eq!(parse("pi"), Some(std::f64::consts::PI));
        assert_eq!(parse("2π"), Some(2.0 * std::f64::consts::PI));
        assert_eq!(parse("-e"), Some(-std::f64::consts::E));
        assert_eq!(parse("3*pi"), Some(3.0 * std::f64::consts::PI));
        assert_eq!(parse(""), None);
        assert_eq!(parse("x"), None);
        assert_eq!(parse("inf"), None);
        assert_eq!(show(None), "");
        assert_eq!(show(Some(-10.0)), "-10");
        assert_eq!(show(Some(1e-15)), "0");
        assert_eq!(show(Some(1.0 / 3.0)), "0.333333");
    }

    #[test]
    fn prefs_default_and_read_partly() {
        let p = GraphPrefs::from_value(
            &serde_json::json!({ "polar": true, "minorGrid": false, "bogus": 1 }),
        );
        assert!(p.polar && !p.minor_grid && p.grid && p.axis_numbers);
        assert_eq!(
            GraphPrefs::from_value(&serde_json::Value::Null),
            GraphPrefs::default()
        );
        let back = serde_json::to_value(p).unwrap();
        assert_eq!(back["minorGrid"], false);
        let mut q = GraphPrefs::default();
        toggle(&mut q, Check::LockViewport);
        assert!(q.lock_viewport);
    }

    #[test]
    fn the_block_swaps_y_for_z_in_3d_and_drops_log_scales() {
        let v = |three_d| View {
            three_d,
            x_label: String::new(),
            y_label: String::new(),
            x: (-10.0, 10.0),
            y: (-5.0, 5.0),
            x_step: None,
            y_step: Some(2.0),
            log: (false, false),
            radians: true,
        };
        let flat = controls(&GraphPrefs::default(), &v(false));
        assert!(flat.contains(&Ctl::Text("≤ y ≤".into())));
        assert!(flat.iter().any(|c| matches!(c, Ctl::Seg(Seg::XScale, ..))));
        assert!(flat.contains(&Ctl::Field(FieldId::YStep, "2".into(), "auto", 48.0)));
        let cube = controls(&GraphPrefs::default(), &v(true));
        assert!(cube.contains(&Ctl::Text("≤ z ≤".into())));
        assert!(!cube.iter().any(|c| matches!(c, Ctl::Seg(Seg::XScale, ..))));
    }
}
