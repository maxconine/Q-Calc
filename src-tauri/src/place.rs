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

// space the page asks for around the bar while the 420 smoke plays; the bar keeps its size and spot inside it
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Room {
    pub top: f64,
    pub side: f64,
    pub bottom: f64,
}

impl Room {
    pub const CLOSED: Room = Room { top: 0.0, side: 0.0, bottom: 0.0 };

    pub fn scaled(self, scale: f64) -> Room {
        Room { top: self.top * scale, side: self.side * scale, bottom: self.bottom * scale }
    }
}

// the bar's standard width, and the least it can be dragged to
pub const MIN_WIDTH: f64 = 680.0;
// how much of the work area's width a widened bar leaves free
const WIDTH_MARGIN: f64 = 40.0;

// a dragged width, logical: never under the standard one, nor wider than its monitor allows, as on the mac
pub fn fitted_width(width: f64, area_width: f64) -> f64 {
    width.max(MIN_WIDTH).min((area_width - WIDTH_MARGIN).max(MIN_WIDTH))
}

// a dragged spot lasts `keep` seconds after the bar was last put away, and then it opens centred again. 0 keeps the
// spot for good; a clock set back counts as no time gone
pub fn spot_expired(now: u64, used_at: u64, keep: i64) -> bool {
    keep > 0 && now.saturating_sub(used_at) >= keep as u64
}

// dragging a side edge by `dx` (logical, positive widens): the new width, and how far the left edge moves
pub fn widened(width: f64, dx: f64, left: bool, area_width: f64) -> (f64, f64) {
    let next = fitted_width(width + dx, area_width);
    (next, if left { width - next } else { 0.0 })
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

// the composer stays put while history grows above it and graphs below it. only the body (the window without its
// room) has to stay on the work area; the smoke's room may run off it
#[allow(clippy::too_many_arguments)]
pub fn resized(
    area: Option<Area>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    anchor: f64,
    next_anchor: f64,
    was: Room,
    room: Room,
) -> (f64, f64) {
    let (x, y) = (x + was.side, y + anchor - next_anchor + room.top);
    let (width, height) = (width - 2.0 * room.side, height - room.top - room.bottom);
    let (x, y) = match area {
        Some(area) => fit(area, x, y, width, height),
        None => (x, y),
    };
    (x - room.side, y - room.top)
}

// the window a game opens in (ctrl+F), logical and inside its frame: the panel's shape at its normal size (what the
// page says it is), as large as fits in most of the work area with windows' title bar and frame round it, and never
// smaller than that normal size
const GAME_SHARE: f64 = 0.8;
const GAME_FRAME: (f64, f64) = (16.0, 40.0);
// a 16:9 picture with a hud, for a page that didn't say
const GAME_FALLBACK: (f64, f64) = (672.0, 460.0);

fn game_shape(natural: Option<(f64, f64)>) -> (f64, f64) {
    natural.filter(|&(w, h)| (100.0..=4000.0).contains(&w) && (100.0..=4000.0).contains(&h)).unwrap_or(GAME_FALLBACK)
}

pub fn game_window(area_width: f64, area_height: f64, natural: Option<(f64, f64)>) -> (f64, f64) {
    let (w, h) = game_shape(natural);
    let fit = ((area_width * GAME_SHARE - GAME_FRAME.0) / w).min((area_height * GAME_SHARE - GAME_FRAME.1) / h);
    let scale = if fit.is_finite() { fit.max(1.0) } else { 1.0 };
    ((w * scale).round().min(area_width - GAME_FRAME.0), (h * scale).round().min(area_height - GAME_FRAME.1))
}

// the smallest it resizes to: the normal size, or what the work area has
pub fn game_window_min(area_width: f64, area_height: f64, natural: Option<(f64, f64)>) -> (f64, f64) {
    let (w, h) = game_shape(natural);
    (w.min(area_width - GAME_FRAME.0), h.min(area_height - GAME_FRAME.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_window_is_most_of_the_screen_in_the_panels_shape() {
        let (w, h) = game_window(1920.0, 1040.0, Some((672.0, 392.0)));
        assert!(w <= 1920.0 * 0.8 && h <= 1040.0 * 0.8);
        assert!((w - (1920.0 * 0.8 - 16.0)).abs() < 1.0 || (h - (1040.0 * 0.8 - 40.0)).abs() < 1.0);
        assert!((w / h - 672.0 / 392.0).abs() < 0.01);
        // never smaller than the panel where there's room, never past a small screen
        let (w, h) = game_window(800.0, 500.0, Some((672.0, 392.0)));
        assert_eq!((w, h), (672.0, 392.0));
        let (w, h) = game_window(600.0, 400.0, Some((672.0, 392.0)));
        assert!(w <= 584.0 && h <= 360.0);
        // a shape for a page that didn't say, or said nonsense
        assert_eq!(game_window(1920.0, 1040.0, None), game_window(1920.0, 1040.0, Some((0.0, 0.0))));
        assert_eq!(game_window_min(1920.0, 1040.0, Some((672.0, 392.0))), (672.0, 392.0));
    }

    #[test]
    fn a_dragged_width_stays_between_the_standard_and_the_monitor() {
        assert_eq!(fitted_width(500.0, 1920.0), MIN_WIDTH);
        assert_eq!(fitted_width(900.0, 1920.0), 900.0);
        assert_eq!(fitted_width(5000.0, 1920.0), 1880.0);
        // a monitor narrower than the bar can't shrink it
        assert_eq!(fitted_width(900.0, 600.0), MIN_WIDTH);
    }

    #[test]
    fn the_left_edge_moves_by_what_the_width_gained() {
        assert_eq!(widened(680.0, 100.0, false, 1920.0), (780.0, 0.0));
        assert_eq!(widened(680.0, 100.0, true, 1920.0), (780.0, -100.0));
        // nothing to give at the standard width, so the edge stays
        assert_eq!(widened(680.0, -50.0, true, 1920.0), (680.0, 0.0));
        assert_eq!(widened(1800.0, 500.0, true, 1920.0), (1880.0, -80.0));
    }

    #[test]
    fn a_spot_lasts_as_long_as_the_setting_says() {
        assert!(!spot_expired(1_000, 950, 60));
        assert!(spot_expired(1_000, 940, 60));
        assert!(spot_expired(1_000, 0, 60));
        // 0 keeps it for good, and a clock set back is no time gone
        assert!(!spot_expired(1_000_000, 0, 0));
        assert!(!spot_expired(900, 1_000, 60));
    }

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
        assert_eq!(resized(Some(PRIMARY), 620.0, 291.0, 680.0, 252.0, 0.0, 180.0, SHUT, SHUT), (620.0, 111.0));
        assert_eq!(resized(Some(PRIMARY), 620.0, 111.0, 680.0, 72.0, 180.0, 0.0, SHUT, SHUT), (620.0, 291.0));
        // a graph grows downward: the top stays
        assert_eq!(resized(Some(PRIMARY), 620.0, 291.0, 680.0, 452.0, 0.0, 0.0, SHUT, SHUT), (620.0, 291.0));
    }

    #[test]
    fn a_window_too_big_to_fit_keeps_its_top_on_screen() {
        assert_eq!(resized(Some(PRIMARY), 620.0, 20.0, 680.0, 560.0, 0.0, 300.0, SHUT, SHUT), (620.0, 0.0));
        let short = Area { height: 400.0, ..PRIMARY };
        assert_eq!(resized(Some(short), 620.0, 100.0, 680.0, 560.0, 0.0, 0.0, SHUT, SHUT), (620.0, 0.0));
        assert_eq!(resized(None, 620.0, 20.0, 680.0, 560.0, 0.0, 300.0, SHUT, SHUT), (620.0, -280.0));
    }

    const SHUT: Room = Room::CLOSED;
    // what the page asks for: 160 above, 100 each side, 64 below
    const ROOM: Room = Room { top: 160.0, side: 100.0, bottom: 64.0 };

    #[test]
    fn the_smoke_room_grows_around_the_bar_and_gives_it_back_exactly() {
        let room = ROOM.scaled(1.5);
        assert_eq!(room, Room { top: 240.0, side: 150.0, bottom: 96.0 });
        let (w, h) = (1020.0, 108.0);
        let open = resized(Some(RIGHT), 2500.0, 400.0, w + 300.0, h + 336.0, 0.0, 240.0, SHUT, room);
        assert_eq!(open, (2350.0, 160.0));
        assert_eq!(resized(Some(RIGHT), open.0, open.1, w, h, 240.0, 0.0, room, SHUT), (2500.0, 400.0));
    }

    #[test]
    fn with_history_open_the_room_goes_above_it() {
        let open = resized(Some(PRIMARY), 620.0, 111.0, 880.0, 476.0, 180.0, 340.0, SHUT, ROOM);
        assert_eq!(open, (520.0, -49.0));
        assert_eq!(resized(Some(PRIMARY), open.0, open.1, 680.0, 252.0, 340.0, 180.0, ROOM, SHUT), (620.0, 111.0));
    }

    #[test]
    fn in_a_corner_the_bar_holds_still_and_the_room_runs_off_the_screen() {
        let open = resized(Some(PRIMARY), 0.0, 0.0, 880.0, 296.0, 0.0, 160.0, SHUT, ROOM);
        assert_eq!(open, (-100.0, -160.0));
        assert_eq!(resized(Some(PRIMARY), open.0, open.1, 680.0, 72.0, 160.0, 0.0, ROOM, SHUT), (0.0, 0.0));
        let right = resized(Some(PRIMARY), 1240.0, 968.0, 880.0, 296.0, 0.0, 160.0, SHUT, ROOM);
        assert_eq!(right, (1140.0, 808.0));
    }
}
