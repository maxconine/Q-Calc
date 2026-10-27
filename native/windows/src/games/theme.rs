//! The games' look, value for value from the Mac's GameTheme.swift (src/index.css :root and its dark block):
//! colours with CSS `color-mix(in srgb, …)` worked out the way the browser does, and the spacing and type scale.

/// A colour, straight (not premultiplied) sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const CLEAR: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const WHITE: Rgba = Rgba::hex(0xffffff, 1.0);

    pub const fn hex(hex: u32, a: f32) -> Rgba {
        Rgba {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a,
        }
    }

    /// `color-mix(in srgb, self p, other)`: premultiplied, as CSS mixes.
    pub fn mix(self, p: f32, other: Rgba) -> Rgba {
        let q = 1.0 - p;
        let a = self.a * p + other.a * q;
        if a <= 0.0 {
            return Rgba::CLEAR;
        }
        Rgba {
            r: (self.r * self.a * p + other.r * other.a * q) / a,
            g: (self.g * self.a * p + other.g * other.a * q) / a,
            b: (self.b * self.a * p + other.b * other.a * q) / a,
            a,
        }
    }

    pub fn opacity(self, k: f32) -> Rgba {
        Rgba {
            a: self.a * k,
            ..self
        }
    }
}

/// Every colour the games' CSS uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    pub dark: bool,
    pub bg: Rgba,
    pub bg_tape: Rgba,
    pub ink: Rgba,
    pub muted: Rgba,
    pub line: Rgba,
    pub accent: Rgba,
    pub control: Rgba,
    /// behind the card while full screen
    pub page: Rgba,
}

impl Colors {
    pub fn new(dark: bool) -> Colors {
        let pick = |l: u32, d: u32| Rgba::hex(if dark { d } else { l }, 1.0);
        Colors {
            dark,
            bg: pick(0xffffff, 0x2c2c2e),
            bg_tape: pick(0xf5f5f7, 0x252527),
            ink: pick(0x1d1d1f, 0xf5f5f7),
            muted: pick(0x86868b, 0x98989d),
            line: if dark {
                Rgba::hex(0xffffff, 0.09)
            } else {
                Rgba::hex(0x000000, 0.08)
            },
            accent: pick(0x1d7a4c, 0x63d392),
            control: pick(0xffffff, 0x58585c),
            page: pick(0xd8d8d4, 0x1c1c1e),
        }
    }

    /// --fill-hover, --fill-selected, --fill-track: ink at 5, 8 and 6%.
    pub fn fill_hover(&self) -> Rgba {
        self.ink.mix(0.05, Rgba::CLEAR)
    }
    pub fn fill_selected(&self) -> Rgba {
        self.ink.mix(0.08, Rgba::CLEAR)
    }
    pub fn fill_track(&self) -> Rgba {
        self.ink.mix(0.06, Rgba::CLEAR)
    }
    /// color-mix(in srgb, var(--bg) 70%, var(--bg-tape)): the court, the board's frame, the dial's bezel.
    pub fn court(&self) -> Rgba {
        self.bg.mix(0.7, self.bg_tape)
    }
    /// The hairline of --control-shadow and its drop.
    pub fn control_ring(&self) -> Rgba {
        Rgba::hex(0, if self.dark { 0.3 } else { 0.12 })
    }
    pub fn control_drop(&self) -> Rgba {
        Rgba::hex(0, if self.dark { 0.25 } else { 0.1 })
    }
    /// --shadow-card's ring and drop.
    pub fn card_ring(&self) -> Rgba {
        if self.dark {
            Rgba::hex(0xffffff, 0.1)
        } else {
            Rgba::hex(0, 0.1)
        }
    }
    pub fn card_drop(&self) -> Rgba {
        Rgba::hex(0, if self.dark { 0.3 } else { 0.06 })
    }
    /// The accent ring an `on` button wears (accent 55%).
    pub fn ring(&self) -> Rgba {
        self.accent.mix(0.55, Rgba::CLEAR)
    }
}

/// The CSS spacing and type scale (--sp-*, --fs-*, --radius*).
pub mod m {
    pub const SP1: f32 = 4.0;
    pub const SP2: f32 = 8.0;
    pub const SP3: f32 = 12.0;
    pub const SP4: f32 = 16.0;
    pub const SP5: f32 = 24.0;
    pub const GUTTER: f32 = 16.0;
    pub const FS_DISPLAY: f32 = 24.0;
    pub const FS_ROW: f32 = 17.0;
    pub const FS_BODY: f32 = 13.0;
    pub const FS_SMALL: f32 = 12.0;
    pub const FS_CAPTION: f32 = 11.0;
    pub const RADIUS: f32 = 12.0;
    pub const RADIUS_S: f32 = 6.0;
    pub const TAPE_ROW: f32 = 36.0;
    /// .spotlight's width: the card the games sit in
    pub const PANEL_W: f32 = 680.0;
}

/// A CSS cubic-bezier timing function at progress t.
pub fn ease(t: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let bx = |s: f64| 3.0 * x1 * s * (1.0 - s) * (1.0 - s) + 3.0 * x2 * s * s * (1.0 - s) + s * s * s;
    let by = |s: f64| 3.0 * y1 * s * (1.0 - s) * (1.0 - s) + 3.0 * y2 * s * s * (1.0 - s) + s * s * s;
    let (mut lo, mut hi, mut s) = (0.0, 1.0, t);
    for _ in 0..30 {
        s = (lo + hi) / 2.0;
        if bx(s) < t {
            lo = s;
        } else {
            hi = s;
        }
    }
    by(s)
}

/// `ease-out`: cubic-bezier(0, 0, 0.58, 1).
pub fn ease_out(t: f64) -> f64 {
    ease(t, 0.0, 0.0, 0.58, 1.0)
}

/// Keyframes (progress, value) eased between with `f`; 0 outside [0, 1).
pub fn keyframes(u: f64, frames: &[(f64, f64)], f: impl Fn(f64) -> f64) -> f64 {
    if !(0.0..1.0).contains(&u) {
        return 0.0;
    }
    for w in frames.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if u <= t1 {
            return v0 + (v1 - v0) * f((u - t0) / (t1 - t0));
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_with_clear_keeps_the_colour_at_less_alpha() {
        let c = Colors::new(false);
        let h = c.fill_hover();
        assert!((h.a - 0.05).abs() < 1e-6);
        assert!((h.r - c.ink.r).abs() < 1e-6);
        // the court is between bg and bg-tape
        let court = c.court();
        assert!(court.r <= c.bg.r && court.r >= c.bg_tape.r);
    }

    #[test]
    fn eases_run_from_0_to_1() {
        assert_eq!(ease(0.0, 0.42, 0.0, 0.58, 1.0), 0.0);
        assert_eq!(ease(1.0, 0.42, 0.0, 0.58, 1.0), 1.0);
        assert!((ease(0.5, 0.42, 0.0, 0.58, 1.0) - 0.5).abs() < 1e-6);
        // the click overshoots
        assert!((1..100).any(|i| ease(f64::from(i) / 100.0, 0.3, 1.6, 0.5, 1.0) > 1.0));
        let shake = [(0.0, 0.0), (0.5, -6.0), (1.0, 0.0)];
        assert_eq!(keyframes(0.5, &shake, |k| k), -6.0);
        assert_eq!(keyframes(1.0, &shake, |k| k), 0.0);
    }
}
