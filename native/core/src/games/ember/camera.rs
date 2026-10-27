//! Which part of a stage is on screen (`src/lib/emberCamera.ts`). Pure, so the renderer and the tests agree: it
//! frames both players with some room around them, never closer than one view, and zooms out as they split up.

use super::types::*;

/// the smallest rect of this aspect that holds one view: what zoom 1 shows
pub fn base_view(aspect: f64) -> (f64, f64) {
    let a = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        VIEW_W / VIEW_H
    };
    let h = VIEW_H.max(VIEW_W / a);
    (h * a, h)
}

/// one axis: a span of `size` centred on `mid`, kept inside 0..limit, or centred over it when it's the bigger
fn place(mid: f64, size: f64, limit: f64) -> f64 {
    if size >= limit {
        (limit - size) / 2.0
    } else {
        (limit - size).min((mid - size / 2.0).max(0.0))
    }
}

/// where the camera wants to be for this state. dead players count where they fell
pub fn camera_for(stage: &Stage, s: &GameState, aspect: f64) -> Camera {
    let [a, b] = &s.players;
    camera_at(stage.w, stage.h, (a.x, a.y), (b.x, b.y), aspect)
}

/// cameraFor from what it reads: the stage's size and the players' top-left corners
pub fn camera_at(stage_w: f64, stage_h: f64, a: (f64, f64), b: (f64, f64), aspect: f64) -> Camera {
    let (bw, bh) = base_view(aspect);
    let x0 = a.0.min(b.0);
    let x1 = a.0.max(b.0) + PLAYER_W;
    let y0 = a.1.min(b.1);
    let y1 = a.1.max(b.1) + PLAYER_H;
    // how many views wide the frame has to be to hold both with their margins
    let need = 1f64
        .max((x1 - x0 + 2.0 * CAM_MARGIN_X) / bw)
        .max((y1 - y0 + 2.0 * CAM_MARGIN_Y) / bh);
    // never wider than the whole stage needs, nor than the widest zoom
    let whole = 1f64.max(stage_w / bw).max(stage_h / bh);
    let k = need.min(whole).min(1.0 / CAM_MIN_ZOOM);
    let w = bw * k;
    let h = bh * k;
    Camera {
        x: place((x0 + x1) / 2.0, w, stage_w),
        y: place((y0 + y1) / 2.0, h, stage_h),
        w,
        h,
        zoom: 1.0 / k,
    }
}

/// the drawn camera closing on its target over `ticks` steps. from None (a new stage, a respawn far away) it jumps
pub fn ease_camera(from: Option<&Camera>, to: &Camera, ticks: f64) -> Camera {
    let Some(from) = from else { return *to };
    if from.w / from.h != to.w / to.h {
        return *to;
    }
    let t = 1.0 - (1.0 - CAM_EASE).powf(ticks.max(0.0));
    let mix = |p: f64, q: f64| {
        if (q - p).abs() < 0.01 {
            q
        } else {
            p + (q - p) * t
        }
    };
    let w = mix(from.w, to.w);
    Camera {
        x: mix(from.x, to.x),
        y: mix(from.y, to.y),
        w,
        h: w * (to.h / to.w),
        zoom: to.zoom * (to.w / w),
    }
}

/// world px to canvas px for a canvas `width` px wide showing `cam`
pub fn camera_scale(cam: &Camera, width: f64) -> f64 {
    width / cam.w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::ember::{LEVELS, compiled_at, engine::start_stage};

    // emberCamera.test.ts, the parts that don't need a hand-built stage
    #[test]
    fn one_view_at_zoom_one() {
        assert_eq!(base_view(16.0 / 9.0), (640.0, 360.0));
        assert_eq!(base_view(1.0), (640.0, 640.0));
        let (w, h) = base_view(f64::NAN);
        assert_eq!((w, h), (640.0, 360.0));
    }

    #[test]
    fn frames_a_one_screen_stage_whole() {
        let level = compiled_at(0).unwrap();
        assert_eq!(LEVELS[0].id, "first-light");
        let st = &level.stages[0];
        let s = start_stage(level, 0, None);
        let cam = camera_for(st, &s, 16.0 / 9.0);
        assert_eq!(cam.zoom, 1.0);
        assert_eq!((cam.w, cam.h), (640.0, 360.0));
        let eased = ease_camera(Some(&cam), &Camera { x: 100.0, ..cam }, 1.0);
        assert!(eased.x > 0.0 && eased.x < 100.0);
        assert_eq!(ease_camera(None, &cam, 1.0), cam);
        assert_eq!(camera_scale(&cam, 1280.0), 2.0);
    }
}
