//! The graph's settings, under the head's wrench, and its export, under the camera (the Mac's GraphSettings.swift,
//! Desmos's graph settings): what's kept between launches (`GraphPrefs`, the `graphPrefs` object in settings.json)
//! and what's set per graph (`GraphState`'s labels, windows, steps and log axes). Both open as a rounded card inside
//! the panel (a popup would take the keys from the bar, which hides it): rows of a name, controls that wrap, and a
//! line saying what they do (graph_view.rs lays them out from `settings` and `export`).
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

    /// GraphPrefs.load: as kept, with the settings taken out of the panel (text size, reverse contrast, arrows,
    /// hiding an axis, lock viewport, the graph's own degrees) back at their defaults, so none is left on with no
    /// way to see it.
    pub fn load(v: &serde_json::Value) -> GraphPrefs {
        let kept = GraphPrefs::from_value(v);
        GraphPrefs {
            grid: kept.grid,
            polar: kept.polar,
            axis_numbers: kept.axis_numbers,
            minor_grid: kept.minor_grid,
            ..GraphPrefs::default()
        }
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
            .map(|v| GraphPrefs::load(&v["graphPrefs"]))
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

/// A two-way choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seg {
    GridKind,
    XScale,
    YScale,
}

/// A setting that's on or off: a rounded cell, in ink with a tick when on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Grid,
    MinorGrid,
    AxisNumbers,
    /// the export's "Label key points" (this graph's, not kept)
    LabelPoints,
}

/// A button: Zoom square (a cell), or the export's Save picture (filled in the accent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    ZoomSquare,
    Save,
}

/// One control in a row, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctl {
    /// a word between fields (`to`, `step`)
    Text(String),
    /// a caption-sized note beside the controls
    Note(String),
    Cell(Check, &'static str, bool, bool),
    Seg(Seg, Vec<&'static str>, usize, bool),
    Field(FieldId, String, &'static str, f32),
    Button(Button, &'static str, bool),
}

/// A row of the card: its name (blank lines the controls up under the others'), its controls and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: &'static str,
    pub ctls: Vec<Ctl>,
    pub about: Option<&'static str>,
}

/// What the cards show for the graph: `three_d` swaps y's window for z's (and has no log scales), `log` is (x, y)
/// on log scales.
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
    pub label_points: bool,
}

/// An axis's window `lo to hi`, its step and (2D) linear or log.
fn range(ids: [FieldId; 3], w: (f64, f64), s: Option<f64>, log: Option<(Seg, bool)>) -> Vec<Ctl> {
    let mut out = vec![
        Ctl::Field(ids[0], show(Some(w.0)), "", 96.0),
        Ctl::Text("to".into()),
        Ctl::Field(ids[1], show(Some(w.1)), "", 96.0),
        Ctl::Text("step".into()),
        Ctl::Field(ids[2], show(s), "auto", 56.0),
    ];
    if let Some((seg, on)) = log {
        out.push(Ctl::Seg(seg, vec!["Linear", "Log"], usize::from(on), true));
    }
    out
}

/// The settings card (GraphSettingsView): the grid, the x and y (or z) ranges with their steps and scales, and
/// Zoom square. Only what changes the view is here: the bar has its own deg/rad switch.
pub fn settings(p: &GraphPrefs, v: &View) -> Vec<Row> {
    let mut out = vec![
        Row {
            name: "Grid",
            ctls: vec![
                Ctl::Cell(Check::Grid, "Grid", p.grid, true),
                Ctl::Seg(
                    Seg::GridKind,
                    vec!["Cartesian", "Polar"],
                    usize::from(p.polar),
                    p.grid,
                ),
                Ctl::Cell(Check::MinorGrid, "Minor lines", p.minor_grid, true),
                Ctl::Cell(Check::AxisNumbers, "Numbers", p.axis_numbers, true),
            ],
            about: Some(
                "Lines behind the graph. Cartesian is squares; polar is rings a step apart with spokes every 15°.",
            ),
        },
        Row {
            name: "x range",
            ctls: range(
                [FieldId::XLo, FieldId::XHi, FieldId::XStep],
                v.x,
                v.x_step,
                (!v.three_d).then_some((Seg::XScale, v.log.0)),
            ),
            about: Some(
                "The window across. Type numbers or pi; Step is the gap between numbered lines (blank picks one).",
            ),
        },
        Row {
            name: if v.three_d { "z range" } else { "y range" },
            ctls: range(
                [FieldId::YLo, FieldId::YHi, FieldId::YStep],
                v.y,
                v.y_step,
                (!v.three_d).then_some((Seg::YScale, v.log.1)),
            ),
            about: Some(if v.three_d {
                "How high the box reaches. Step is the gap between numbered lines."
            } else {
                "The window up and down. Log spaces 1, 10, 100… evenly, for values that grow fast."
            }),
        },
    ];
    if !v.three_d {
        out.push(Row {
            name: "",
            ctls: vec![
                Ctl::Button(Button::ZoomSquare, "Zoom square", !v.log.0 && !v.log.1),
                Ctl::Note("Reset (top right) goes back to the view the graph opened on.".into()),
            ],
            about: None,
        });
    }
    out
}

/// The export card (GraphExportView): label the key points, name the axes, save the picture.
pub fn export(v: &View) -> Vec<Row> {
    let (y_name, y_hint) = if v.three_d {
        ("z axis", "z")
    } else {
        ("y axis", "y")
    };
    vec![Row {
        name: "Export",
        ctls: vec![
            Ctl::Cell(Check::LabelPoints, "Label key points", v.label_points, true),
            Ctl::Text("x axis".into()),
            Ctl::Field(FieldId::XLabel, v.x_label.clone(), "x", 90.0),
            Ctl::Text(y_name.into()),
            Ctl::Field(FieldId::YLabel, v.y_label.clone(), y_hint, 90.0),
            Ctl::Button(Button::Save, "Save picture", true),
        ],
        about: Some(
            "Saves a PNG of the graph and its equations to the Desktop and copies it. Axis names show on the graph too.",
        ),
    }]
}

/// A kept checkbox clicked (the export's own is the graph's).
pub fn toggle(p: &mut GraphPrefs, c: Check) {
    match c {
        Check::Grid => p.grid = !p.grid,
        Check::MinorGrid => p.minor_grid = !p.minor_grid,
        Check::AxisNumbers => p.axis_numbers = !p.axis_numbers,
        Check::LabelPoints => {}
    }
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
    fn prefs_default_and_read_partly_and_the_removed_ones_reset() {
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
        toggle(&mut q, Check::Grid);
        assert!(!q.grid);
        // GraphPrefs.load: what left the panel goes back to its default
        let kept = serde_json::json!({ "polar": true, "largeText": true, "reverseContrast": true, "arrows": true,
            "showXAxis": false, "showYAxis": false, "lockViewport": true, "degrees": true, "axisNumbers": false });
        let l = GraphPrefs::load(&kept);
        assert!(l.polar && !l.axis_numbers);
        assert!(!l.large_text && !l.reverse_contrast && !l.arrows && !l.lock_viewport);
        assert!(l.show_x_axis && l.show_y_axis && l.degrees.is_none());
    }

    #[test]
    fn the_cards_swap_y_for_z_in_3d_and_drop_log_scales() {
        let v = |three_d| View {
            three_d,
            x_label: String::new(),
            y_label: String::new(),
            x: (-10.0, 10.0),
            y: (-5.0, 5.0),
            x_step: None,
            y_step: Some(2.0),
            log: (false, false),
            label_points: true,
        };
        let flat = settings(&GraphPrefs::default(), &v(false));
        assert_eq!(
            flat.iter().map(|r| r.name).collect::<Vec<_>>(),
            ["Grid", "x range", "y range", ""]
        );
        let ctls: Vec<&Ctl> = flat.iter().flat_map(|r| &r.ctls).collect();
        assert!(ctls.iter().any(|c| matches!(c, Ctl::Seg(Seg::XScale, ..))));
        assert!(ctls.contains(&&Ctl::Field(FieldId::YStep, "2".into(), "auto", 56.0)));
        assert!(flat.iter().all(|r| r.name.is_empty() || r.about.is_some()));
        let cube = settings(&GraphPrefs::default(), &v(true));
        assert_eq!(cube.len(), 3);
        assert_eq!(cube[2].name, "z range");
        assert!(
            !cube
                .iter()
                .flat_map(|r| &r.ctls)
                .any(|c| matches!(c, Ctl::Seg(Seg::XScale | Seg::YScale, ..)))
        );
        let ex = export(&v(true));
        assert!(ex[0].ctls.contains(&Ctl::Text("z axis".into())));
        assert!(ex[0].ctls.contains(&Ctl::Cell(
            Check::LabelPoints,
            "Label key points",
            true,
            true
        )));
    }
}
