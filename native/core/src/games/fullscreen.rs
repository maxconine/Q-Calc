//! `src/lib/gameFullscreen.ts`: fullscreen for the games (⌃F), the pure parts. the chord, the wording of its hint,
//! and where a game's picture sits on a screen that isn't its shape. in the apps it's a big window of the game's
//! own rather than the whole display.

use crate::jsnum;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

/// A rect inside its container, and how many times the game's own size it is.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct GameBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

/// The keys of a chord: `key` as the page names it, `code` its qwerty position (`KeyF`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Chord {
    pub key: String,
    pub code: String,
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
    pub shift: bool,
}

/// ctrl+F and nothing else held: ⌘F, ⌃⇧F and ⌃⌥F are other shortcuts. by the layout's letter; a layout without
/// latin letters falls back to the key where f sits on qwerty, like the mac app's ⌘W.
pub fn is_fullscreen_chord(e: &Chord) -> bool {
    if !e.ctrl || e.meta || e.alt || e.shift {
        return false;
    }
    let key = e.key.to_lowercase();
    if key == "f" || key == "\u{6}" {
        return true;
    }
    let one = key.encode_utf16().count() == 1;
    let latin = one && key.as_str() >= "a" && key.as_str() <= "z";
    !latin && one && e.code == "KeyF"
}

/// Escape with nothing held, the other way out.
pub fn is_plain_escape(e: &Chord) -> bool {
    (e.key == "Escape" || e.key == "Esc") && !e.ctrl && !e.meta && !e.alt && !e.shift
}

/// Macs write the control key as a glyph. `windowed`: the apps open a big window rather than the whole screen.
pub fn fullscreen_hint(mac: bool, on: bool, windowed: bool) -> String {
    let chord = if mac { "⌃F" } else { "Ctrl+F" };
    if windowed {
        return if on {
            format!("{chord} or esc back to the bar")
        } else {
            format!("{chord} big window")
        };
    }
    if on {
        format!("{chord} or esc leaves fullscreen")
    } else {
        format!("{chord} fullscreen")
    }
}

/// What the button and the f key are called.
pub fn fullscreen_label(on: bool, windowed: bool) -> &'static str {
    match (windowed, on) {
        (true, true) => "back to the bar",
        (true, false) => "big window",
        (false, true) => "leave full screen",
        (false, false) => "full screen",
    }
}

/// The windows shell says so itself; a browser is asked what it runs on.
pub fn mac_like(windows_host: bool, platform: &str) -> bool {
    let p = platform.to_lowercase();
    !windows_host && (p.contains("mac") || p.contains("iphone") || p.contains("ipad"))
}

const EMPTY: GameBox = GameBox {
    x: 0.0,
    y: 0.0,
    w: 0.0,
    h: 0.0,
    scale: 0.0,
};

/// The largest centred rect of the game's shape that fits the container, on whole css pixels. `integer` keeps
/// the scale a whole number once there's room for one, for pixel art that mustn't blur.
pub fn letterbox(container: Size, base: Size, integer: bool) -> GameBox {
    if !(container.w > 0.0 && container.h > 0.0 && base.w > 0.0 && base.h > 0.0) {
        return EMPTY;
    }
    let mut scale = (container.w / base.w).min(container.h / base.h);
    if !scale.is_finite() || scale <= 0.0 {
        return EMPTY;
    }
    if integer && scale >= 1.0 {
        scale = scale.floor();
    }
    let w = container
        .w
        .floor()
        .min((base.w * scale + 1e-6).floor().max(1.0));
    // from the width, so the shape survives the rounding
    let h = container
        .h
        .floor()
        .min(jsnum::round(w * base.h / base.w).max(1.0));
    GameBox {
        x: ((container.w - w) / 2.0).floor(),
        y: ((container.h - h) / 2.0).floor(),
        w,
        h,
        scale: w / base.w,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FullscreenLayoutOptions {
    /// The game's picture at its normal size, in css pixels.
    pub base: Size,
    /// Height the panel spends around the picture at normal size: hud, keys line, gaps.
    pub chrome: f64,
    /// Room kept clear at each screen edge, at normal size.
    pub pad: f64,
    /// The hud and text never grow past this many times their normal size.
    pub max_ui: f64,
    pub integer: bool,
}

impl FullscreenLayoutOptions {
    pub fn new(base: Size) -> FullscreenLayoutOptions {
        FullscreenLayoutOptions {
            base,
            chrome: 0.0,
            pad: 0.0,
            max_ui: 2.0,
            integer: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FullscreenLayout {
    pub viewport: Size,
    /// Where the picture goes, inside the viewport.
    pub stage: GameBox,
    /// How much larger the hud and text are drawn.
    pub ui: f64,
}

/// The hud grows as if the whole small panel were scaled up to the screen (capped), and the picture takes the
/// largest box of its shape in what's left.
pub fn fullscreen_layout(viewport: Size, o: &FullscreenLayoutOptions) -> FullscreenLayout {
    let (base, chrome, pad) = (o.base, o.chrome, o.pad);
    let fit = (viewport.w / (base.w + 2.0 * pad)).min(viewport.h / (base.h + chrome + 2.0 * pad));
    let ui = if fit.is_finite() {
        (fit.max(1.0).min(o.max_ui.max(1.0)) * 100.0).floor() / 100.0
    } else {
        1.0
    };
    let top = (pad + chrome) * ui;
    let room = Size {
        w: viewport.w - 2.0 * pad * ui,
        h: viewport.h - top - pad * ui,
    };
    let bx = letterbox(room, base, o.integer);
    // the chrome's room is split evenly above and below the picture
    let stage = if bx.w != 0.0 {
        GameBox {
            x: bx.x + (pad * ui).floor(),
            y: bx.y + (pad * ui + chrome * ui / 2.0).floor(),
            ..bx
        }
    } else {
        bx
    };
    FullscreenLayout {
        viewport,
        stage,
        ui,
    }
}

/// The panel's own size at normal size, for the shell to give the game's window that shape.
pub fn natural_size(base: Size, chrome: f64, pad: f64) -> Size {
    Size {
        w: jsnum::round(base.w + 2.0 * pad),
        h: jsnum::round(base.h + chrome + 2.0 * pad),
    }
}

/// The canvas's backing store for a css size: device pixels, capped so a huge display doesn't cost a frame rate.
pub fn backing_size(css: Size, dpr: f64, max_dpr: f64) -> Size {
    let ratio = (if dpr.is_finite() { dpr } else { 1.0 })
        .max(1.0)
        .min(max_dpr);
    Size {
        w: jsnum::round(css.w * ratio).max(0.0),
        h: jsnum::round(css.h * ratio).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(w: f64, h: f64) -> Size {
        Size { w, h }
    }
    fn b(x: f64, y: f64, w: f64, h: f64, scale: f64) -> GameBox {
        GameBox { x, y, w, h, scale }
    }
    fn ctrl(key: &str, code: &str) -> Chord {
        Chord {
            key: key.into(),
            code: code.into(),
            ctrl: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_chord() {
        assert!(is_fullscreen_chord(&ctrl("f", "KeyF")));
        assert!(is_fullscreen_chord(&ctrl("F", "KeyF")));
        assert!(!is_fullscreen_chord(&Chord {
            ctrl: false,
            ..ctrl("f", "KeyF")
        }));
        assert!(!is_fullscreen_chord(&Chord {
            meta: true,
            ..ctrl("f", "KeyF")
        }));
        assert!(!is_fullscreen_chord(&Chord {
            shift: true,
            ..ctrl("F", "KeyF")
        }));
        assert!(!is_fullscreen_chord(&Chord {
            alt: true,
            ..ctrl("f", "KeyF")
        }));
        assert!(is_fullscreen_chord(&ctrl("f", "KeyY")));
        assert!(!is_fullscreen_chord(&ctrl("u", "KeyF")));
        assert!(is_fullscreen_chord(&ctrl("а", "KeyF")));
        assert!(!is_fullscreen_chord(&ctrl("ф", "KeyA")));
        assert!(is_fullscreen_chord(&ctrl("\u{6}", "")));
        assert!(!is_fullscreen_chord(&ctrl("d", "KeyD")));
        assert!(!is_fullscreen_chord(&ctrl("Enter", "Enter")));
        assert!(!is_fullscreen_chord(&ctrl("F5", "F5")));
        let esc = |meta| Chord {
            key: "Escape".into(),
            meta,
            ..Default::default()
        };
        assert!(is_plain_escape(&esc(false)));
        assert!(is_plain_escape(&Chord {
            key: "Esc".into(),
            ..Default::default()
        }));
        assert!(!is_plain_escape(&esc(true)));
    }

    #[test]
    fn hints_and_labels() {
        assert_eq!(fullscreen_hint(true, false, false), "⌃F fullscreen");
        assert_eq!(fullscreen_hint(false, false, false), "Ctrl+F fullscreen");
        assert_eq!(
            fullscreen_hint(true, true, false),
            "⌃F or esc leaves fullscreen"
        );
        assert_eq!(
            fullscreen_hint(false, true, false),
            "Ctrl+F or esc leaves fullscreen"
        );
        assert_eq!(fullscreen_hint(true, false, true), "⌃F big window");
        assert_eq!(
            fullscreen_hint(false, true, true),
            "Ctrl+F or esc back to the bar"
        );
        assert_eq!(fullscreen_label(false, true), "big window");
        assert_eq!(fullscreen_label(true, true), "back to the bar");
        assert_eq!(fullscreen_label(false, false), "full screen");
        assert_eq!(fullscreen_label(true, false), "leave full screen");
        assert!(mac_like(false, "MacIntel"));
        assert!(!mac_like(false, "Win32"));
        assert!(!mac_like(false, "Linux x86_64"));
        assert!(!mac_like(true, "MacIntel"));
        assert!(!mac_like(false, ""));
    }

    #[test]
    fn letterbox_fits() {
        let game = s(640.0, 360.0);
        assert_eq!(
            letterbox(s(1920.0, 1080.0), game, false),
            b(0.0, 0.0, 1920.0, 1080.0, 3.0)
        );
        assert_eq!(
            letterbox(s(1440.0, 900.0), game, false),
            b(0.0, 45.0, 1440.0, 810.0, 2.25)
        );
        assert_eq!(
            letterbox(s(3440.0, 1440.0), game, false),
            b(440.0, 0.0, 2560.0, 1440.0, 4.0)
        );
        for c in [
            s(1512.0, 982.0),
            s(1366.0, 768.0),
            s(1001.0, 563.0),
            s(333.0, 777.0),
            s(700.5, 393.7),
        ] {
            let bx = letterbox(c, game, false);
            assert!(bx.w <= c.w && bx.h <= c.h);
            assert!([bx.w, bx.h, bx.x, bx.y].iter().all(|v| v.fract() == 0.0));
            assert!((bx.w / bx.h - 16.0 / 9.0).abs() < 0.01);
            assert!(c.w - bx.w < 2.0 || c.h - bx.h < 2.0);
            assert!((c.w - bx.w - 2.0 * bx.x).abs() < 2.0);
            assert!((c.h - bx.h - 2.0 * bx.y).abs() < 2.0);
        }
        assert_eq!(
            letterbox(s(1440.0, 900.0), game, true),
            b(80.0, 90.0, 1280.0, 720.0, 2.0)
        );
        assert_eq!(
            letterbox(s(480.0, 400.0), game, true),
            b(0.0, 65.0, 480.0, 270.0, 0.75)
        );
        assert_eq!(letterbox(s(0.0, 900.0), game, false), EMPTY);
        assert_eq!(letterbox(s(100.0, -1.0), game, false), EMPTY);
        assert_eq!(letterbox(s(100.0, 100.0), s(0.0, 360.0), false), EMPTY);
        assert_eq!(letterbox(s(f64::NAN, 100.0), game, false), EMPTY);
    }

    #[test]
    fn the_fullscreen_layout() {
        let base = s(640.0, 360.0);
        let l = fullscreen_layout(s(1920.0, 1080.0), &FullscreenLayoutOptions::new(base));
        assert_eq!(l.stage, b(0.0, 0.0, 1920.0, 1080.0, 3.0));
        assert_eq!(l.ui, 2.0);
        let o = FullscreenLayoutOptions {
            chrome: 90.0,
            pad: 12.0,
            ..FullscreenLayoutOptions::new(base)
        };
        let l = fullscreen_layout(s(1440.0, 900.0), &o);
        assert_eq!(l.ui, 1.89);
        assert!(l.stage.h + 90.0 * l.ui + 2.0 * 12.0 * l.ui <= 900.0);
        assert!(l.stage.w + 2.0 * 12.0 * l.ui <= 1440.0);
        assert!((1440.0 - l.stage.w - 2.0 * l.stage.x).abs() <= 2.0);
        assert!(l.stage.y > 0.0 && l.stage.y + l.stage.h <= 900.0);
        let c90 = FullscreenLayoutOptions {
            chrome: 90.0,
            ..FullscreenLayoutOptions::new(base)
        };
        assert_eq!(fullscreen_layout(s(500.0, 300.0), &c90).ui, 1.0);
        assert_eq!(fullscreen_layout(s(5120.0, 2880.0), &c90).ui, 2.0);
        assert_eq!(
            fullscreen_layout(
                s(5120.0, 2880.0),
                &FullscreenLayoutOptions { max_ui: 3.0, ..c90 }
            )
            .ui,
            3.0
        );
        let small = fullscreen_layout(s(500.0, 300.0), &c90);
        assert!(small.stage.h <= 210.0 && small.stage.w > 0.0);
        assert_eq!(
            fullscreen_layout(s(0.0, 0.0), &FullscreenLayoutOptions::new(base))
                .stage
                .w,
            0.0
        );
    }

    #[test]
    fn natural_and_backing_sizes() {
        assert_eq!(natural_size(s(640.0, 360.0), 96.0, 12.0), s(664.0, 480.0));
        assert_eq!(natural_size(s(640.0, 360.0), 0.0, 0.0), s(640.0, 360.0));
        assert_eq!(backing_size(s(1440.0, 810.0), 2.0, 3.0), s(2880.0, 1620.0));
        assert_eq!(backing_size(s(853.0, 480.0), 1.5, 3.0), s(1280.0, 720.0));
        assert_eq!(backing_size(s(100.0, 50.0), 4.0, 3.0), s(300.0, 150.0));
        assert_eq!(backing_size(s(100.0, 50.0), 4.0, 2.0), s(200.0, 100.0));
        assert_eq!(backing_size(s(100.0, 50.0), 0.5, 3.0), s(100.0, 50.0));
        assert_eq!(backing_size(s(100.0, 50.0), f64::NAN, 3.0), s(100.0, 50.0));
    }
}
