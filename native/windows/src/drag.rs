//! Moving and widening the bar: dragged by any part of the composer that isn't a control (the line, the answer),
//! widened from its left and right edges (clamped per monitor: never under 680 DIPs, never wider than the work
//! area allows, as place.rs's `fitted_width` says), and the spot and width remembered for the monitor it ends on
//! (store.positions, logical, keyed by the monitor's device name, as src-tauri's place.rs keeps them; store.width).
//!
//! Windows does the dragging itself (WM_NCHITTEST says HTCAPTION / HTLEFT / HTRIGHT, its move-size loop runs);
//! WM_SIZING keeps the height and clamps the width, WM_EXITSIZEMOVE remembers the result.

use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, WMSZ_LEFT};

use crate::app::App;
use crate::place;
use crate::window::{self, BarLayout};

/// How close to the left or right edge (DIPs) the pointer widens the bar.
const GRIP: f32 = 6.0;

/// What is under window DIP point (x, y): a widening edge, the background that drags, or a control.
pub fn hit(l: &BarLayout, x: f32, y: f32) -> u32 {
    if y >= l.composer.y && y < l.composer.bottom() {
        if x < GRIP {
            return HTLEFT;
        }
        if x >= l.width - GRIP {
            return HTRIGHT;
        }
        let field = l.field.inset(0.0, -6.0);
        if field.contains(x, y) || (l.answer.w > 0.0 && l.answer.inset(-4.0, -6.0).contains(x, y)) {
            return HTCLIENT;
        }
        return HTCAPTION;
    }
    if let Some(h) = l.hint
        && h.contains(x, y)
    {
        return HTCAPTION;
    }
    HTCLIENT
}

/// WM_NCHITTEST: screen point in `lp`.
pub fn nc_hit_test(app: &App, lp: LPARAM) -> u32 {
    let (sx, sy) = (
        i32::from((lp.0 & 0xffff) as u16 as i16),
        i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    );
    let f = app.bar.frame();
    let s = app.bar.scale();
    let (x, y) = ((sx - f.left) as f32 / s, (sy - f.top) as f32 / s);
    // the edge tools take clicks and keep their hover instead of dragging the bar
    if crate::edge::tool_at(app, x, y).is_some() {
        return HTCLIENT;
    }
    hit(&app.bar.layout, x, y)
}

/// WM_SIZING: the edge being dragged (`wp`) and the proposed frame (`lp`, a RECT to adjust): the height stays,
/// the width is clamped for the monitor.
pub fn sizing(app: &mut App, wp: WPARAM, lp: LPARAM) {
    // SAFETY: WM_SIZING's lParam is a live RECT the loop reads back
    let r = unsafe { &mut *(lp.0 as *mut RECT) };
    let f = app.bar.frame();
    let monitor = window::monitor_at(windows::Win32::Foundation::POINT {
        x: (f.left + f.right) / 2,
        y: (f.top + f.bottom) / 2,
    });
    let scale = monitor.scale();
    let want = f64::from(r.right - r.left) / scale;
    let w = place::fitted_width(want, monitor.area.width / scale);
    let px = (w * scale).round() as i32;
    if wp.0 as u32 == WMSZ_LEFT {
        r.left = r.right - px;
    } else {
        r.right = r.left + px;
    }
    r.top = f.top;
    r.bottom = f.bottom;
    app.bar.width = w as f32;
    app.model.touch();
}

/// WM_MOVE / WM_EXITSIZEMOVE: the composer's spot on screen follows the window.
pub fn moved(app: &mut App) {
    if !app.bar.visible {
        return;
    }
    let f = app.bar.frame();
    let anchor = (app.bar.layout.anchor * app.bar.scale()).round() as i32;
    app.bar.composer_at = (f.left, f.top + anchor);
}

/// WM_EXITSIZEMOVE: the spot (and width) remembered for the monitor the bar ended on.
pub fn remember(app: &mut App) {
    moved(app);
    let f = app.bar.frame();
    let monitor = window::monitor_at(windows::Win32::Foundation::POINT {
        x: (f.left + f.right) / 2,
        y: f.top + (app.bar.layout.anchor * app.bar.scale()) as i32,
    });
    let scale = monitor.scale();
    let anchor = f64::from(app.bar.layout.anchor) * scale;
    let spot = place::remembered(
        monitor.area,
        scale,
        f64::from(f.left),
        f64::from(f.top),
        anchor,
    );
    app.store.positions.insert(monitor.key.clone(), spot);
    let w = f64::from(app.bar.width);
    app.store.width = if w > place::MIN_WIDTH { w } else { 0.0 };
    app.store.used_at = (crate::model::now_ms() / 1000) as u64;
    app.bar.monitor = Some(monitor);
    app.save_store();
    app.model.touch();
}

/// WM_DPICHANGED (dragged onto a monitor of another scale, or the scale changed): Windows' suggested frame (`lp`)
/// says where the bar now is, so the reframe that follows sizes it for the new scale on the new monitor instead of
/// fitting it back onto the old one.
pub fn dpi_changed(app: &mut App, lp: LPARAM) {
    if lp.0 == 0 {
        return;
    }
    // SAFETY: WM_DPICHANGED's lParam is a live RECT for this message
    let r = unsafe { *(lp.0 as *const RECT) };
    let monitor = window::monitor_at(windows::Win32::Foundation::POINT {
        x: (r.left + r.right) / 2,
        y: (r.top + r.bottom) / 2,
    });
    let (width, at) = reframed(
        r,
        monitor.area.width,
        monitor.scale(),
        app.bar.width,
        app.bar.layout.anchor,
    );
    app.bar.width = width;
    app.bar.composer_at = at;
    app.bar.monitor = Some(monitor);
}

/// WM_DPICHANGED's arithmetic: for Windows' suggested frame `r` (px) on a monitor of `scale` whose work area is
/// `area_px` wide, the bar's width (DIPs, kept unless the monitor is too narrow for it) and where the composer
/// stands (px: the frame's left, its top plus the anchor at the new scale).
pub fn reframed(r: RECT, area_px: f64, scale: f64, width: f32, anchor: f32) -> (f32, (i32, i32)) {
    let width = place::fitted_width(f64::from(width), area_px / scale) as f32;
    (
        width,
        (r.left, r.top + (f64::from(anchor) * scale).round() as i32),
    )
}

/// WM_NCLBUTTONDBLCLK on the background: the bar goes back to where it opens by default on this monitor (the
/// Mac's double-click to put back); the spot for the monitor is forgotten, the width kept.
pub fn put_back(app: &mut App) {
    if let Some(m) = &app.bar.monitor {
        app.store.positions.remove(&m.key);
    }
    app.save_store();
    app.ops.push(crate::app::Op::Show);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Rect;

    fn layout() -> BarLayout {
        BarLayout {
            width: 680.0,
            height: 72.0,
            anchor: 0.0,
            composer: Rect::new(0.0, 0.0, 680.0, 72.0),
            field: Rect::new(16.0, 18.0, 400.0, 36.0),
            answer: Rect::new(500.0, 18.0, 164.0, 36.0),
            ..BarLayout::default()
        }
    }

    #[test]
    fn edges_widen_controls_click_and_the_rest_drags() {
        let l = layout();
        assert_eq!(hit(&l, 2.0, 30.0), HTLEFT);
        assert_eq!(hit(&l, 678.0, 30.0), HTRIGHT);
        assert_eq!(hit(&l, 100.0, 30.0), HTCLIENT);
        assert_eq!(hit(&l, 550.0, 30.0), HTCLIENT);
        assert_eq!(hit(&l, 100.0, 4.0), HTCAPTION);
        assert_eq!(hit(&l, 450.0, 30.0), HTCAPTION);
        // below the composer (panels) is never the background
        assert_eq!(hit(&l, 100.0, 80.0), HTCLIENT);
    }

    fn rect(left: i32, top: i32, w: i32, h: i32) -> RECT {
        RECT {
            left,
            top,
            right: left + w,
            bottom: top + h,
        }
    }

    /// WM_DPICHANGED with synthetic frames: the bar keeps its width in DIPs across 100, 150 and 200%, the composer
    /// stands at the suggested frame's left and its top plus the anchor at the new scale, and a monitor too narrow
    /// at its scale narrows it (never under the standard width).
    #[test]
    fn dpi_changes_keep_the_width_and_place_the_composer() {
        let (w, anchor) = (900.0, 40.0);
        // a 1920 px wide work area at each scale, and a 3840 px one
        for (scale, left, top) in [(1.0, 100, 200), (1.5, -1800, 300), (2.0, 4000, 50)] {
            let suggested = rect(left, top, (900.0 * scale) as i32, (120.0 * scale) as i32);
            let (width, at) = reframed(suggested, 3840.0, scale, w, anchor);
            assert_eq!(width, 900.0, "{scale}");
            assert_eq!(at, (left, top + (40.0 * scale) as i32), "{scale}");
        }
        // 100% -> 200% on 1920 px: 960 DIPs across, so 920 at most: 900 stays
        assert_eq!(
            reframed(rect(0, 0, 1800, 240), 1920.0, 2.0, 900.0, anchor).0,
            900.0
        );
        // a wide bar (1500 DIPs) dragged from a 4K 100% monitor onto a 1920 px one at 150%: 1280 - 40
        assert_eq!(
            reframed(rect(10, 10, 2250, 180), 1920.0, 1.5, 1500.0, anchor).0,
            1240.0
        );
        // and at 200% on 1920 px: 920
        assert_eq!(
            reframed(rect(10, 10, 3000, 240), 1920.0, 2.0, 1500.0, anchor).0,
            920.0
        );
        // a tiny monitor at 200% never takes the bar under its standard width
        assert_eq!(
            reframed(rect(0, 0, 1360, 144), 1280.0, 2.0, 700.0, anchor).0,
            place::MIN_WIDTH as f32
        );
        // 200% -> 100%: back to the width it was given, the anchor at 1x
        let (width, at) = reframed(rect(5, 7, 1240, 120), 1920.0, 1.0, 1240.0, 36.5);
        assert_eq!(width, 1240.0);
        assert_eq!(at, (5, 7 + 37));
        // a fractional scale (125%) rounds the anchor to whole pixels
        assert_eq!(
            reframed(rect(0, 0, 900, 90), 1920.0, 1.25, 700.0, 36.5).1,
            (0, 46)
        );
    }
}
