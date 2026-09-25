// window placement in physical pixels: monitors can each have their own scale, so logical
// coordinates don't line up across them. saved spots are logical so they survive a scale change.

// how far down the work area the composer sits, as on the mac
pub const TOP: f64 = 0.28;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

// the whole window stays on the work area; if it can't fit, the top-left wins
pub fn fit(area: Area, x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
    (x.min(area.right() - width).max(area.x), y.min(area.bottom() - height).max(area.y))
}

// centred, with the composer TOP of the way down; `anchor` is the composer's distance below the window top
pub fn centred(area: Area, width: f64, anchor: f64) -> (f64, f64) {
    (area.x + (area.width - width) / 2.0, area.y + area.height * TOP - anchor)
}

// where a showing opens: the spot remembered for this monitor, else the default
pub fn show_origin(area: Area, scale: f64, width: f64, height: f64, saved: Option<[f64; 2]>) -> (f64, f64) {
    match saved {
        Some([dx, dy]) => fit(area, area.x + dx * scale, area.y + dy * scale, width, height),
        None => centred(area, width, 0.0),
    }
}

// the composer's top-left relative to the work area, in logical pixels
pub fn remembered(area: Area, scale: f64, x: f64, y: f64, anchor: f64) -> [f64; 2] {
    [(x - area.x) / scale, (y - area.y) / scale + anchor / scale]
}

// the composer stays put while history grows above it and graphs below it
pub fn resized(area: Option<Area>, x: f64, y: f64, width: f64, height: f64, anchor: f64, next_anchor: f64) -> (f64, f64) {
    let y = y + anchor - next_anchor;
    match area {
        Some(area) => fit(area, x, y, width, height),
        None => (x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Area = Area { x: 0.0, y: 0.0, width: 1920.0, height: 1040.0 };
    // a 150% monitor to the right, and a 100% one to the left at negative coordinates
    const RIGHT: Area = Area { x: 1920.0, y: 0.0, width: 2560.0, height: 1400.0 };
    const LEFT: Area = Area { x: -1280.0, y: 200.0, width: 1280.0, height: 984.0 };

    #[test]
    fn default_spot_is_centred_28_percent_down() {
        assert_eq!(show_origin(PRIMARY, 1.0, 680.0, 72.0, None), (620.0, 1040.0 * 0.28));
        assert_eq!(show_origin(LEFT, 1.0, 680.0, 72.0, None), (-980.0, 200.0 + 984.0 * 0.28));
    }

    #[test]
    fn default_spot_uses_the_monitors_own_scale() {
        let (w, h) = (680.0 * 1.5, 72.0 * 1.5);
        assert_eq!(show_origin(RIGHT, 1.5, w, h, None), (1920.0 + (2560.0 - 1020.0) / 2.0, 1400.0 * TOP));
    }

    #[test]
    fn a_remembered_spot_comes_back_at_any_scale() {
        let saved = remembered(RIGHT, 1.5, 2100.0, 450.0, 0.0);
        assert_eq!(saved, [120.0, 300.0]);
        assert_eq!(show_origin(RIGHT, 1.5, 1020.0, 108.0, Some(saved)), (2100.0, 450.0));
        // same monitor after a switch to 200%: the same logical spot
        assert_eq!(show_origin(RIGHT, 2.0, 1360.0, 144.0, Some(saved)), (2160.0, 600.0));
    }

    #[test]
    fn remembering_records_the_composer_not_the_window_top() {
        // history open above the composer: the window top sits 180 physical px higher
        let saved = remembered(PRIMARY, 1.0, 400.0, 100.0, 180.0);
        assert_eq!(saved, [400.0, 280.0]);
        assert_eq!(show_origin(PRIMARY, 1.0, 680.0, 72.0, Some(saved)), (400.0, 280.0));
    }

    #[test]
    fn a_spot_off_the_work_area_is_pulled_back() {
        assert_eq!(show_origin(PRIMARY, 1.0, 680.0, 72.0, Some([1800.0, 1030.0])), (1240.0, 968.0));
        assert_eq!(show_origin(LEFT, 1.0, 680.0, 72.0, Some([-50.0, -50.0])), (-1280.0, 200.0));
    }

    #[test]
    fn growing_history_moves_the_window_up_around_the_composer() {
        assert_eq!(resized(Some(PRIMARY), 620.0, 291.0, 680.0, 252.0, 0.0, 180.0), (620.0, 111.0));
        assert_eq!(resized(Some(PRIMARY), 620.0, 111.0, 680.0, 72.0, 180.0, 0.0), (620.0, 291.0));
        // a graph grows downward: the top stays
        assert_eq!(resized(Some(PRIMARY), 620.0, 291.0, 680.0, 452.0, 0.0, 0.0), (620.0, 291.0));
    }

    #[test]
    fn a_window_too_big_to_fit_keeps_its_top_on_screen() {
        assert_eq!(resized(Some(PRIMARY), 620.0, 20.0, 680.0, 560.0, 0.0, 300.0), (620.0, 0.0));
        let short = Area { height: 400.0, ..PRIMARY };
        assert_eq!(resized(Some(short), 620.0, 100.0, 680.0, 560.0, 0.0, 0.0), (620.0, 0.0));
        assert_eq!(resized(None, 620.0, 20.0, 680.0, 560.0, 0.0, 300.0), (620.0, -280.0));
    }
}
