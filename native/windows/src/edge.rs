//! The edge tools (EdgeTools in BarView.swift, EdgeTools.tsx): settings top left, deg · a/b · sf · ×10ⁿ (native
//! only: Ctrl+E's exponent form, exponent.rs) top right, clear
//! bottom right, over the composer's corners, shown while the pointer is on the composer (its line or the background
//! that drags, which Windows reports as non-client). Each is 11 pt medium, muted and opaque (an active toggle in the
//! answer's colour), with a hover and press fill, and takes clicks rather than starting a drag; its tip, an ink pill,
//! after a 450 ms rest; the tip ends with the action's key, from the user's keybinds ("Degrees · click for
//! radians · Ctrl+D").

use crate::keys::{KeyAction, Keybinds};
use crate::store::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Settings,
    Angle,
    Fraction,
    SigFigs,
    Exponent,
    Clear,
}

pub const TOOLS: [Tool; 6] = [
    Tool::Settings,
    Tool::Angle,
    Tool::Fraction,
    Tool::SigFigs,
    Tool::Exponent,
    Tool::Clear,
];

/// The pointer's part in them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeState {
    /// the pointer is on the composer: the tools show
    pub shown: bool,
    pub hover: Option<Tool>,
    pub pressed: Option<Tool>,
    /// the tool whose tip is up
    pub tip: Option<Tool>,
    /// a WM_MOUSELEAVE is asked for
    pub tracking: bool,
    /// a WM_NCMOUSELEAVE is asked for (the pointer is on the background that drags)
    pub nc_tracking: bool,
}

impl Tool {
    /// The bar's shortcut it does (the exponent form's is the native app's own, exponent.rs).
    pub fn action(self) -> Option<KeyAction> {
        Some(match self {
            Tool::Settings => KeyAction::Settings,
            Tool::Angle => KeyAction::Angle,
            Tool::Fraction => KeyAction::Fraction,
            Tool::SigFigs => KeyAction::SigFigs,
            Tool::Clear => KeyAction::Clear,
            Tool::Exponent => return None,
        })
    }

    /// What it says ("" for the ones drawn as marks: the gear and a/b).
    pub fn label(self, s: &Settings) -> &'static str {
        match self {
            Tool::Settings | Tool::Fraction => "",
            Tool::Angle => {
                if s.angle_mode == "rad" {
                    "rad"
                } else {
                    "deg"
                }
            }
            Tool::SigFigs => "sf",
            Tool::Exponent => "×10ⁿ",
            Tool::Clear => "clear",
        }
    }

    /// In the answer's colour: the angle always, a toggle while it's on.
    pub fn active(self, s: &Settings) -> bool {
        match self {
            Tool::Angle => true,
            Tool::Fraction => s.fraction_mode,
            Tool::SigFigs => s.sig_fig_mode,
            Tool::Exponent => s.exponent_mode,
            Tool::Settings | Tool::Clear => false,
        }
    }

    /// The tip, with its key when it has one.
    pub fn tip(self, s: &Settings, keybinds: &Keybinds) -> String {
        if self == Tool::Exponent {
            return crate::exponent::tip(s.exponent_mode, &s.keybinds);
        }
        let on = |b: bool| if b { "on" } else { "off" };
        let text = match self {
            Tool::Settings => "Open settings".to_string(),
            Tool::Angle => if s.angle_mode == "rad" {
                "Radians · click for degrees"
            } else {
                "Degrees · click for radians"
            }
            .to_string(),
            Tool::Fraction => format!("Answers as fractions: {}", on(s.fraction_mode)),
            Tool::SigFigs => format!("Sig figs from what you type: {}", on(s.sig_fig_mode)),
            Tool::Clear => "Clear history and variables".to_string(),
            Tool::Exponent => String::new(),
        };
        match self.action().and_then(|a| keybinds.chord_for(a)) {
            Some(chord) if !chord.is_empty() => {
                format!("{text} · {}", crate::hotkey::title(&chord))
            }
            _ => text,
        }
    }

    /// Its tip opens above it (the bottom one) or below (the top ones).
    pub fn tip_above(self) -> bool {
        self == Tool::Clear
    }
}

#[cfg(windows)]
pub use ui::*;

#[cfg(windows)]
mod ui {
    use std::cell::RefCell;

    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_SEMI_BOLD, IDWriteTextFormat,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
    };

    use super::{EdgeState, Tool};
    use crate::app::App;
    use crate::keys::KeyAction;
    use crate::later;
    use crate::model::{BarModel, Outcome};
    use crate::render::{Canvas, Gfx, Rect, format, measure, size};

    const H: f32 = 23.0;
    /// not in the windows crate's Controls module
    pub const WM_NCMOUSELEAVE: u32 = 0x02A2;
    const TIP_MS: u32 = 450;

    thread_local! {
        /// the gear's icon font, made at the first hover
        static ICONS: RefCell<Option<Option<IDWriteTextFormat>>> = const { RefCell::new(None) };
        /// .edge-frac-mark's 6.5 pt semibold
        static MARK: RefCell<Option<Option<IDWriteTextFormat>>> = const { RefCell::new(None) };
        /// inside `nc_pointer`
        static IN_NC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    fn mark(gfx: &Gfx) -> Option<IDWriteTextFormat> {
        MARK.with(|m| {
            m.borrow_mut()
                .get_or_insert_with(|| {
                    format(
                        &gfx.dw,
                        &gfx.fonts.text_family,
                        6.5,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    )
                    .ok()
                })
                .clone()
        })
    }

    fn icons(gfx: &Gfx) -> Option<IDWriteTextFormat> {
        ICONS.with(|i| {
            i.borrow_mut()
                .get_or_insert_with(|| {
                    ["Segoe Fluent Icons", "Segoe MDL2 Assets"]
                        .iter()
                        .find_map(|f| format(&gfx.dw, f, 10.5, DWRITE_FONT_WEIGHT_MEDIUM).ok())
                })
                .clone()
        })
    }

    fn width(gfx: &Gfx, m: &BarModel, tool: Tool) -> f32 {
        match tool {
            Tool::Settings => 12.0 + 11.0,
            Tool::Fraction => 18.0 + 7.0,
            _ => {
                let label = tool.label(&m.settings);
                gfx.layout(label, &gfx.fonts.caption, 0.0, H)
                    .map_or(20.0, |l| measure(&l).0 + 18.0)
            }
        }
    }

    /// Where each tool is over the composer (window DIPs).
    pub fn rects(gfx: &Gfx, m: &BarModel, composer: Rect) -> Vec<(Tool, Rect)> {
        let mut out = vec![(
            Tool::Settings,
            Rect::new(
                composer.x + 3.0,
                composer.y,
                width(gfx, m, Tool::Settings),
                H,
            ),
        )];
        let mut right = composer.right() - 6.0;
        for tool in [Tool::Exponent, Tool::SigFigs, Tool::Fraction, Tool::Angle] {
            let w = width(gfx, m, tool);
            right -= w;
            out.push((tool, Rect::new(right, composer.y, w, H)));
        }
        let w = width(gfx, m, Tool::Clear);
        out.push((
            Tool::Clear,
            Rect::new(composer.right() - 6.0 - w, composer.bottom() - H, w, H),
        ));
        out
    }

    pub fn tool_at(app: &App, x: f32, y: f32) -> Option<Tool> {
        if !app.bar.layout.composer.contains(x, y) {
            return None;
        }
        rects(&app.gfx, &app.model, app.bar.layout.composer)
            .into_iter()
            .find(|(_, r)| r.contains(x, y))
            .map(|(t, _)| t)
    }

    /// The pointer over the composer's background, which drags the bar (HTCAPTION, so Windows sends it as
    /// WM_NCMOUSEMOVE, not WM_MOUSEMOVE): the tools show there too. `lp` is the screen point.
    pub fn nc_pointer(app: &mut App, lp: windows::Win32::Foundation::LPARAM) {
        if !app.model.edge.nc_tracking {
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: app.bar.hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: a live struct for our bar
            let _ = unsafe { TrackMouseEvent(&mut t) };
            // one leave is tracked at a time: this replaces the client area's
            app.model.edge.nc_tracking = true;
            app.model.edge.tracking = false;
        }
        let (sx, sy) = (
            i32::from((lp.0 & 0xffff) as u16 as i16),
            i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
        );
        let at = window_point(app, sx, sy);
        // the tools only: the rest of the bar keeps the client area's pointer, as before. No client tracking from
        // here: asked for off the client area, Windows posts WM_MOUSELEAVE at once
        IN_NC.with(|n| n.set(true));
        pointer(app, Some(at));
        IN_NC.with(|n| n.set(false));
    }

    /// WM_NCMOUSELEAVE: the pointer left the background that drags.
    pub fn nc_left(app: &mut App) {
        // stale when a client request (ours or graph.rs's) has replaced the background's since
        if !app.model.edge.nc_tracking {
            return;
        }
        app.model.edge.nc_tracking = false;
        pointer(app, None);
    }

    /// WM_MOUSELEAVE while the pointer is still inside the bar, over it or over the tooltip a word's name just put
    /// up there: tracking asked for in the moment the tip appeared (its first WM_MOUSEMOVE) ends at once, and the
    /// leave hid the tip, which came back 350 ms on: a blink. False for a real leave; true asks again on the next
    /// move.
    pub fn spurious_leave(app: &mut App) -> bool {
        if !pointer_inside(app) {
            return false;
        }
        app.model.edge.tracking = false;
        recheck_leave();
        true
    }

    /// After a leave that wasn't: until a move asks for tracking again, look every 150 ms, so a pointer that jumps
    /// straight off the bar (no move over it on the way) still takes the tips and the highlight with it.
    fn recheck_leave() {
        crate::later::after("leave-recheck", 150, |app| {
            if app.model.edge.tracking {
                return;
            }
            if pointer_inside(app) {
                recheck_leave();
            } else {
                crate::commands::pointer(app, None);
                crate::graph::leave(app);
            }
        });
    }

    /// The pointer is inside the showing bar, over it or over the tooltip on it.
    fn pointer_inside(app: &App) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, GetWindowRect, WindowFromPoint,
        };
        let mut pt = windows::Win32::Foundation::POINT::default();
        let mut r = windows::Win32::Foundation::RECT::default();
        // SAFETY: live out structs; our own window
        let inside =
            unsafe { GetCursorPos(&mut pt).is_ok() && GetWindowRect(app.bar.hwnd, &mut r).is_ok() }
                && pt.x >= r.left
                && pt.x < r.right
                && pt.y >= r.top
                && pt.y < r.bottom;
        if !inside || !app.bar.visible {
            return false;
        }
        // SAFETY: a plain query
        let under = unsafe { WindowFromPoint(pt) };
        let tip = crate::hover::tip::hwnd();
        under == app.bar.hwnd || (tip != 0 && under.0 as usize == tip)
    }

    /// Whether the pointer is still over the bar itself (not a tooltip or another window over it): moving between
    /// the line and the background that drags leaves one area for the other, not the bar.
    fn still_over(app: &App) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, WindowFromPoint};
        let mut pt = windows::Win32::Foundation::POINT::default();
        // SAFETY: a live out point
        unsafe { GetCursorPos(&mut pt) }.is_ok() && unsafe { WindowFromPoint(pt) } == app.bar.hwnd
    }

    fn window_point(app: &App, sx: i32, sy: i32) -> (f32, f32) {
        let f = app.bar.frame();
        let s = app.bar.scale();
        ((sx - f.left) as f32 / s, (sy - f.top) as f32 / s)
    }

    /// The pointer moved over the bar (None: it left the window).
    pub fn pointer(app: &mut App, at: Option<(f32, f32)>) {
        if at.is_some() && !app.model.edge.tracking && !IN_NC.with(|n| n.get()) {
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: app.bar.hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: a live struct for our bar
            let _ = unsafe { TrackMouseEvent(&mut t) };
            app.model.edge.tracking = true;
        }
        if at.is_some() && !IN_NC.with(|n| n.get()) {
            // on the line: any background request was replaced (by ours or graph.rs's)
            app.model.edge.nc_tracking = false;
        }
        if at.is_none() {
            if !IN_NC.with(|n| n.get()) {
                app.model.edge.tracking = false;
            }
            // a leave between the line and the background keeps the tools; the next move updates them
            if still_over(app) {
                return;
            }
        }
        let shown = at.is_some_and(|(x, y)| app.bar.layout.composer.contains(x, y));
        let hover = if shown {
            at.and_then(|(x, y)| tool_at(app, x, y))
        } else {
            None
        };
        let e = &mut app.model.edge;
        if e.shown == shown && e.hover == hover {
            return;
        }
        let moved = e.hover != hover;
        *e = EdgeState {
            shown,
            hover,
            pressed: e.pressed.filter(|p| Some(*p) == hover),
            tip: if moved { None } else { e.tip },
            tracking: e.tracking,
            nc_tracking: e.nc_tracking,
        };
        app.model.touch();
        if moved {
            later::cancel("edge-tip");
            if let Some(tool) = hover {
                later::after("edge-tip", TIP_MS, move |app| {
                    if app.model.edge.hover == Some(tool) {
                        app.model.edge.tip = Some(tool);
                        app.model.touch();
                    }
                });
            }
        }
    }

    /// A click: true when it was on a tool.
    pub fn click(app: &mut App, x: f32, y: f32) -> bool {
        if !app.model.edge.shown {
            return false;
        }
        let Some(tool) = tool_at(app, x, y) else {
            return false;
        };
        app.model.edge.tip = None;
        later::cancel("edge-tip");
        match tool {
            Tool::Settings => crate::settings_window::open(app),
            Tool::Clear => {
                if app.model.perform(KeyAction::Clear, &app.engine) == Outcome::ClearHistory {
                    app.set_history(Vec::new());
                    app.model.hint = Some("history cleared".into());
                }
            }
            Tool::Exponent => crate::exponent::toggle(&mut app.model, &app.engine),
            _ => {
                if let Some(action) = tool.action() {
                    app.model.perform(action, &app.engine);
                }
            }
        }
        app.model.touch();
        true
    }

    /// The bar hid: nothing is hovered.
    pub fn will_hide(m: &mut BarModel) {
        m.edge = EdgeState::default();
        later::cancel("edge-tip");
    }

    pub fn paint(c: &Canvas, m: &BarModel, composer: Rect) {
        let e = &m.edge;
        if !e.shown {
            return;
        }
        let t = c.theme;
        let s = &m.settings;
        for (tool, r) in rects(c.gfx, m, composer) {
            let hovered = e.hover == Some(tool);
            if e.pressed == Some(tool) {
                c.fill_round(r, size::RADIUS_S, t.fill_selected);
            } else if hovered {
                c.fill_round(r, size::RADIUS_S, t.fill_hover);
            }
            let ink = if tool.active(s) || (hovered && tool == Tool::Settings) {
                t.answer
            } else {
                // opaque: at 72% they read as disabled
                t.muted
            };
            match tool {
                Tool::Settings => {
                    // the gear (Segoe's Settings glyph)
                    if let Some(f) = icons(c.gfx)
                        && let Some(l) = c.gfx.layout("\u{E713}", &f, 0.0, r.h)
                    {
                        let (w, _) = measure(&l);
                        c.text(&l, r.x + (r.w - w) / 2.0, r.y, ink);
                    }
                }
                Tool::Fraction => {
                    // a over b with a 7 pt bar
                    let mid = r.y + r.h / 2.0;
                    let cx = r.x + r.w / 2.0;
                    if let Some(f) = mark(c.gfx) {
                        for (glyph, y) in [("a", mid - 9.0), ("b", mid + 0.5)] {
                            if let Some(l) = c.gfx.layout(glyph, &f, 0.0, 9.0) {
                                let (w, _) = measure(&l);
                                c.text(&l, cx - w / 2.0, y, ink);
                            }
                        }
                    }
                    c.line(cx - 3.5, mid, cx + 3.5, mid, 1.0, ink);
                }
                _ => {
                    if let Some(l) = c.gfx.layout(tool.label(s), &c.gfx.fonts.caption, 0.0, r.h) {
                        c.text(&l, r.x + 9.0, r.y, ink);
                    }
                }
            }
        }
        // the tip: an ink pill with the background's colour for text, 4 off the button
        if let Some(tool) = e.tip
            && let Some((_, r)) = rects(c.gfx, m, composer)
                .into_iter()
                .find(|(t, _)| *t == tool)
        {
            let tip = tool.tip(s, &m.keybinds);
            if let Some(l) = c.gfx.layout(&tip, &c.gfx.fonts.caption, 0.0, 15.0) {
                let (w, _) = measure(&l);
                let pw = w + 14.0;
                let ph = 23.0;
                let x = if tool == Tool::Settings {
                    r.x
                } else {
                    r.right() - pw
                };
                let y = if tool.tip_above() {
                    r.y - 4.0 - ph
                } else {
                    r.bottom() + 4.0
                };
                let pill = Rect::new(x.max(2.0).min(c.width - pw - 2.0), y, pw, ph);
                c.fill_round(pill, size::RADIUS_S, t.ink);
                c.text(&l, pill.x + 7.0, pill.y + 4.0, t.bg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn tips_end_with_the_key() {
        let s = Settings::default();
        let k = Keybinds::new(&BTreeMap::new());
        assert_eq!(
            Tool::Angle.tip(&s, &k),
            "Degrees · click for radians · Ctrl+D"
        );
        assert_eq!(Tool::Settings.tip(&s, &k), "Open settings · Ctrl+,");
        assert_eq!(
            Tool::Fraction.tip(&s, &k),
            "Answers as fractions: off · Ctrl+F"
        );
        assert_eq!(
            Tool::SigFigs.tip(&s, &k),
            "Sig figs from what you type: off · Ctrl+S"
        );
        assert_eq!(
            Tool::Clear.tip(&s, &k),
            "Clear history and variables · Ctrl+Shift+Backspace"
        );
        let unbound = Keybinds::new(&BTreeMap::from([("clear".to_string(), String::new())]));
        assert_eq!(Tool::Clear.tip(&s, &unbound), "Clear history and variables");
        let rad = Settings {
            angle_mode: "rad".into(),
            fraction_mode: true,
            ..Settings::default()
        };
        assert_eq!(Tool::Angle.label(&rad), "rad");
        assert!(Tool::Fraction.active(&rad));
        assert!(!Tool::SigFigs.active(&rad));
        assert!(Tool::Clear.tip_above() && !Tool::Angle.tip_above());
        assert_eq!(
            Tool::Exponent.tip(&s, &k),
            "Answers in exponent form: off · Ctrl+E"
        );
        assert_eq!(Tool::Exponent.label(&s), "×10ⁿ");
        assert!(!Tool::Exponent.active(&s));
    }
}
