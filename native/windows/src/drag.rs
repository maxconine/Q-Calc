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
    hit(
        &app.bar.layout,
        (sx - f.left) as f32 / s,
        (sy - f.top) as f32 / s,
    )
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
}
