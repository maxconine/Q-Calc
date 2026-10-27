//! The effects' timing, pure: cubic-bézier easing, keyframe tracks, and each effect's numbers at a moment, from
//! the web components' keyframes (index.css `four-twenty-rise`, `sixty-seven-weigh`, the 69's spin) and the Mac's
//! native-only ones (EggOverlay.swift: the black hole, Euler's confetti, π's digits, the barrel roll). Times are
//! seconds since the effect began; every track holds its first value before it starts and its last after it ends
//! (`animation-fill-mode: both`, `fillMode = .both`).

use std::f64::consts::PI;

/// A CSS / Core Animation cubic-bézier timing function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bez(pub f64, pub f64, pub f64, pub f64);

pub const LINEAR: Bez = Bez(0.0, 0.0, 1.0, 1.0);
/// css `ease`, CA `.default`
pub const EASE: Bez = Bez(0.25, 0.1, 0.25, 1.0);
pub const EASE_IN: Bez = Bez(0.42, 0.0, 1.0, 1.0);
pub const EASE_OUT: Bez = Bez(0.0, 0.0, 0.58, 1.0);
pub const EASE_IN_OUT: Bez = Bez(0.42, 0.0, 0.58, 1.0);

fn coeffs(p1: f64, p2: f64) -> (f64, f64, f64) {
    let c = 3.0 * p1;
    let b = 3.0 * (p2 - p1) - c;
    (1.0 - c - b, b, c)
}

fn curve((a, b, c): (f64, f64, f64), t: f64) -> f64 {
    ((a * t + b) * t + c) * t
}

impl Bez {
    /// The eased value at progress `x` (0–1). Control points past 0–1 in y overshoot, as css does.
    pub fn at(self, x: f64) -> f64 {
        if self == LINEAR {
            return x.clamp(0.0, 1.0);
        }
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let cx = coeffs(self.0, self.2);
        let cy = coeffs(self.1, self.3);
        // Newton from t = x, then bisection if the slope gives out
        let mut t = x;
        for _ in 0..8 {
            let err = curve(cx, t) - x;
            if err.abs() < 1e-7 {
                return curve(cy, t);
            }
            let d = (3.0 * cx.0 * t + 2.0 * cx.1) * t + cx.2;
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        t = x;
        for _ in 0..40 {
            let v = curve(cx, t);
            if (v - x).abs() < 1e-7 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        curve(cy, t)
    }
}

/// Progress through an animation that begins at `begin` and lasts `dur`, held at both ends.
pub fn progress(t: f64, begin: f64, dur: f64) -> f64 {
    ((t - begin) / dur).clamp(0.0, 1.0)
}

/// Which segment of the key times `p` falls in, and how far through it (0–1); held before the first key and
/// after the last.
pub fn segment(p: f64, times: &[f64]) -> (usize, f64) {
    let n = times.len();
    if n < 2 || p <= times[0] {
        return (0, 0.0);
    }
    if p >= times[n - 1] {
        return (n - 2, 1.0);
    }
    let i = (0..n - 1).find(|&i| p < times[i + 1]).unwrap_or(n - 2);
    (i, (p - times[i]) / (times[i + 1] - times[i]))
}

/// A keyframe track: `values` at `times`, each segment eased by `eases[i]` (the last one repeats).
pub fn key(p: f64, times: &[f64], values: &[f64], eases: &[Bez]) -> f64 {
    let (i, u) = segment(p, times);
    let e = eases[i.min(eases.len() - 1)].at(u);
    values[i] + (values[i + 1] - values[i]) * e
}

// ---- 420: one puff (four-twenty-rise, 2.15 s ease-out; reduced: four-twenty-fade, 2.5 s ease)

pub const SMOKE_S: f64 = 2.15;
pub const SMOKE_STILL_S: f64 = 2.5;

/// Where a puff is: offset from its spot (DIPs), its scale, its opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PuffAt {
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    pub opacity: f64,
}

/// A puff at `t`, `delay` seconds late, with its drift / rise / sway.
pub fn puff(t: f64, delay: f64, drift: f64, rise: f64, sway: f64, reduce: bool) -> PuffAt {
    if reduce {
        let p = progress(t, delay, SMOKE_STILL_S);
        return PuffAt {
            dx: sway,
            dy: rise * 0.55,
            scale: 1.0,
            opacity: key(p, &[0.0, 0.18, 0.68, 1.0], &[0.0, 0.8, 0.8, 0.0], &[EASE]),
        };
    }
    let p = progress(t, delay, SMOKE_S);
    let times = [0.0, 0.48, 1.0];
    let e = [EASE_OUT];
    PuffAt {
        dx: key(p, &times, &[0.0, sway, drift], &e),
        dy: key(p, &times, &[16.0, rise * 0.55, rise], &e),
        scale: key(p, &times, &[0.45, 1.1, 1.6], &e),
        opacity: key(
            p,
            &[0.0, 0.14, 0.48, 1.0],
            &[0.0, 0.85, 0.55, 0.0],
            &e,
        ),
    }
}

// ---- 67: a hand (sixty-seven-weigh / -counter, 2.6 s; reduced: sixty-seven-still)

pub const ARMS_S: f64 = 2.6;
/// translateY(-110%) of the hand's 30 px
const UP: f64 = -33.0;
const HAND_TIMES: [f64; 11] = [0.0, 0.1, 0.21, 0.32, 0.43, 0.54, 0.65, 0.76, 0.86, 0.9, 1.0];
const WEIGH: Bez = Bez(0.38, 0.0, 0.26, 1.32);
const HAND_EASES: [Bez; 10] = [
    Bez(0.2, 0.8, 0.35, 1.25),
    WEIGH,
    WEIGH,
    WEIGH,
    WEIGH,
    WEIGH,
    WEIGH,
    Bez(0.3, 0.0, 0.3, 1.0),
    EASE_IN_OUT,
    Bez(0.5, 0.0, 0.8, 0.4),
];
const LEFT_Y: [f64; 11] = [UP, 33.0, 6.0, 29.0, 7.0, 28.0, 8.0, 27.0, 18.0, 21.0, UP];
const LEFT_DEG: [f64; 11] = [0.0, 9.0, -6.0, 7.0, -5.0, 6.0, -5.0, 5.0, 0.0, 0.0, 0.0];
const RIGHT_Y: [f64; 11] = [UP, 5.0, 29.0, 7.0, 28.0, 8.0, 27.0, 9.0, 18.0, 21.0, UP];
const RIGHT_DEG: [f64; 11] = [0.0, 6.0, -7.0, 5.0, -6.0, 5.0, -5.0, 4.0, 0.0, 0.0, 0.0];

/// A hand at `t`: (translateY, rotation in degrees, opacity).
pub fn hand(t: f64, right: bool, reduce: bool) -> (f64, f64, f64) {
    let p = progress(t, 0.0, ARMS_S);
    if reduce {
        let (y, deg) = if right { (8.0, 4.0) } else { (24.0, 5.0) };
        return (
            y,
            deg,
            key(p, &[0.0, 0.15, 0.85, 1.0], &[0.0, 1.0, 1.0, 0.0], &[EASE]),
        );
    }
    let (ys, degs) = if right {
        (&RIGHT_Y, &RIGHT_DEG)
    } else {
        (&LEFT_Y, &LEFT_DEG)
    };
    (
        key(p, &HAND_TIMES, ys, &HAND_EASES),
        key(p, &HAND_TIMES, degs, &HAND_EASES),
        1.0,
    )
}

// ---- 69: a digit's turn (2.2 s ease-in-out, once each way)

pub const FOLD_S: f64 = 2.2;

/// A digit at `t`: (rotation in degrees, opacity). `spin` is +1 for the 6, -1 for the 9.
pub fn digit(t: f64, spin: f64, reduce: bool) -> (f64, f64) {
    let p = progress(t, 0.0, FOLD_S);
    if reduce {
        return (
            0.0,
            key(p, &[0.0, 0.2, 0.8, 1.0], &[0.0, 1.0, 1.0, 0.0], &[EASE_IN_OUT]),
        );
    }
    (
        spin * 360.0 * EASE_IN_OUT.at(p),
        key(p, &[0.0, 0.12, 0.88, 1.0], &[0.0, 1.0, 1.0, 0.0], &[EASE_IN_OUT]),
    )
}

// ---- x/0: the black hole

pub const VOID_S: f64 = 1.7;
pub const VOID_STILL_S: f64 = 1.4;
/// the copy starts falling this late, for this long
pub const FALL_BEGIN: f64 = 0.28;
pub const FALL_S: f64 = 0.85;
/// the real answer stays hidden this long, then fades back over VEIL_FADE
pub const VEIL_S: f64 = 1.4;
pub const VEIL_FADE: f64 = 0.25;

/// The hole at `t`: (scale, opacity, the ring's turn in radians).
pub fn hole(t: f64, reduce: bool) -> (f64, f64, f64) {
    if reduce {
        let p = progress(t, 0.0, VOID_STILL_S);
        return (
            1.0,
            key(p, &[0.0, 0.2, 0.8, 1.0], &[0.0, 1.0, 1.0, 0.0], &[EASE_IN_OUT]),
            0.0,
        );
    }
    let p = progress(t, 0.0, VOID_S);
    (
        key(
            p,
            &[0.0, 0.18, 0.66, 0.74, 0.86],
            &[0.01, 1.0, 1.0, 1.25, 0.01],
            &[EASE_IN_OUT],
        ),
        key(p, &[0.0, 0.86, 1.0], &[1.0, 1.0, 0.0], &[EASE_IN_OUT]),
        4.0 * PI * p,
    )
}

/// The answer's copy falling in: its centre, scale, turn (radians) and opacity, on a tightening spiral from
/// `start` into `centre`.
pub fn fall(t: f64, start: (f64, f64), centre: (f64, f64)) -> (f64, f64, f64, f64, f64) {
    let p = progress(t, FALL_BEGIN, FALL_S);
    let span = (start.0 - centre.0).hypot(start.1 - centre.1) + 14.0;
    let angle = p * 2.2 * PI;
    let reach = span * 0.35 * (p * PI).sin();
    let x = start.0 + (centre.0 - start.0) * p * p + reach * angle.cos();
    let y = start.1 + (centre.1 - start.1) * p * p + reach * angle.sin();
    let e = EASE_IN.at(p);
    let scale = 1.0 + (0.05 - 1.0) * e;
    let turn = 2.4 * PI * e;
    let opacity = key(p, &[0.0, 0.6, 1.0], &[1.0, 1.0, 0.0], &[EASE_IN]);
    (x, y, scale, turn, opacity)
}

/// How much of the cover over the real answer is left (1 hidden, 0 back).
pub fn veil(t: f64) -> f64 {
    1.0 - EASE_OUT.at(progress(t, VEIL_S, VEIL_FADE))
}

// ---- e^(iπ)+1: the note and the confetti

pub const NOTE_TEXT: &str = "the most beautiful equation";

/// The note's opacity at `t`.
pub fn note(t: f64, reduce: bool) -> f64 {
    let (begin, dur) = if reduce { (0.0, 2.2) } else { (0.25, 2.1) };
    key(
        progress(t, begin, dur),
        &[0.0, 0.15, 0.8, 1.0],
        &[0.0, 0.9, 0.9, 0.0],
        &[EASE_IN_OUT],
    )
}

/// The confetti's colours (sRGB 0–1).
pub const CONFETTI: [(f32, f32, f32); 5] = [
    (0.39, 0.83, 0.57),
    (1.0, 0.78, 0.2),
    (0.98, 0.4, 0.45),
    (0.4, 0.6, 1.0),
    (0.75, 0.5, 1.0),
];

/// One bit of confetti (the Mac's emitter cell: 110 a second for 0.1 s per colour, 1.5 ± 0.3 s, 190 ± 70 px/s
/// upward within ±72°, falling at 330 px/s², spinning 3 ± 8 rad/s, scaled 0.9 ± 0.4, fading 0.6 a second).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bit {
    pub born: f64,
    pub life: f64,
    pub vx: f64,
    pub vy: f64,
    pub spin: f64,
    pub scale: f64,
    pub colour: usize,
}

/// A bit at `age` seconds: its offset from the burst, turn, scale and opacity (None once it's gone).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BitAt {
    pub dx: f64,
    pub dy: f64,
    pub turn: f64,
    pub opacity: f64,
}

pub const GRAVITY: f64 = 330.0;

impl Bit {
    pub fn at(&self, t: f64) -> Option<BitAt> {
        let age = t - self.born;
        let opacity = 1.0 - 0.6 * age;
        if age < 0.0 || age > self.life || opacity <= 0.0 {
            return None;
        }
        Some(BitAt {
            dx: self.vx * age,
            dy: self.vy * age + 0.5 * GRAVITY * age * age,
            turn: self.spin * age,
            opacity,
        })
    }
}

/// A small, seeded generator (xorshift64*), so a burst is the same every time and testable.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    /// uniform in 0..1
    pub fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }
    /// uniform in mid ± range
    pub fn around(&mut self, mid: f64, range: f64) -> f64 {
        mid + range * (2.0 * self.next() - 1.0)
    }
}

/// The whole burst.
pub fn burst(seed: u64) -> Vec<Bit> {
    let mut rng = Rng::new(seed);
    let mut bits = Vec::with_capacity(CONFETTI.len() * 11);
    for colour in 0..CONFETTI.len() {
        // 110 a second for a tenth of a second
        for k in 0..11 {
            let angle = rng.around(-PI / 2.0, PI * 0.4);
            let v = rng.around(190.0, 70.0);
            bits.push(Bit {
                born: f64::from(k) / 110.0,
                life: rng.around(1.5, 0.3),
                vx: v * angle.cos(),
                vy: v * angle.sin(),
                spin: rng.around(3.0, 8.0),
                scale: rng.around(0.9, 0.4),
                colour,
            });
        }
    }
    bits
}

// ---- π on Pi Day: the digits

pub const DIGITS_S: f64 = 2.3;

/// 3.14159 26535 89793 …: grouped by five after the point.
pub fn grouped(digits: &str) -> String {
    let mut out: String = digits.chars().take(2).collect();
    for (i, d) in digits.chars().skip(2).enumerate() {
        if i > 0 && i % 5 == 0 {
            out.push(' ');
        }
        out.push(d);
    }
    out
}

/// As many whole groups as fit in `room` at `char_w` a character.
pub fn fitted(text: &str, char_w: f64, room: f64) -> &str {
    let mut shown = text;
    while shown.chars().count() as f64 * char_w > room {
        match shown.rfind(' ') {
            Some(i) => shown = &shown[..i],
            None => break,
        }
    }
    shown
}

/// The digits' opacity at `t`.
pub fn digits_opacity(t: f64, reduce: bool) -> f64 {
    let times = if reduce {
        [0.0, 0.15, 0.78, 1.0]
    } else {
        [0.0, 0.04, 0.78, 1.0]
    };
    key(
        progress(t, 0.0, DIGITS_S),
        &times,
        &[0.0, 1.0, 1.0, 0.0],
        &[EASE_IN_OUT],
    )
}

/// How many characters are showing at `t` of `count` (the first seven, 3.14159, at once; then one at a time).
pub fn digits_shown(t: f64, count: usize, reduce: bool) -> usize {
    let n = count.max(8);
    if reduce {
        return count;
    }
    let dur = (0.03 * n as f64).min(1.2);
    let p = progress(t, 0.1, dur);
    let steps = n - 7;
    (7 + ((p * steps as f64).floor() as usize).min(steps)).min(count.max(7))
}

// ---- do a barrel roll

pub const ROLL_S: f64 = 1.0;

/// The bar's picture at `t`: (turn in radians, scale). Turns once, dipping to 0.9 halfway.
pub fn roll(t: f64) -> (f64, f64) {
    let p = progress(t, 0.0, ROLL_S);
    let times = [0.0, 0.5, 1.0];
    let eases = [EASE_IN, EASE_OUT];
    (
        key(p, &times, &[0.0, PI * 0.999, 2.0 * PI * 0.999], &eases),
        key(p, &times, &[1.0, 0.9, 1.0], &eases),
    )
}

/// The room a turning bar needs round it: its half-diagonal less its half-short-side, and a little more.
pub fn roll_room(w: f64, h: f64) -> f64 {
    ((w * w + h * h).sqrt() / 2.0 - w.min(h) / 2.0 + 24.0).max(200.0)
}

// ---- the Konami code's CRT look

/// The phosphor colour matrix (CRTLook: the light in each pixel as green on near black), on one sRGB colour.
pub fn phosphor(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = 0.3 * r + 0.59 * g + 0.11 * b;
    (
        (0.3 * l - 0.02).clamp(0.0, 1.0),
        (1.15 * l - 0.07).clamp(0.0, 1.0),
        (0.5 * l - 0.03).clamp(0.0, 1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn beziers_match_their_known_points() {
        for b in [LINEAR, EASE, EASE_IN, EASE_OUT, EASE_IN_OUT] {
            assert_eq!(b.at(0.0), 0.0);
            assert_eq!(b.at(1.0), 1.0);
        }
        assert!(close(EASE_IN_OUT.at(0.5), 0.5));
        assert!(close(LINEAR.at(0.3), 0.3));
        // ease at 0.5 is about 0.8024 (the css reference value)
        assert!(close(EASE.at(0.5), 0.8024));
        assert!(EASE_IN.at(0.25) < 0.25 && EASE_OUT.at(0.25) > 0.25);
        // the 67's weigh overshoots
        let over = (1..100).map(|i| WEIGH.at(f64::from(i) / 100.0)).fold(0.0, f64::max);
        assert!(over > 1.0, "{over}");
    }

    #[test]
    fn keyframes_hold_their_ends() {
        let times = [0.0, 0.5, 1.0];
        let values = [0.0, 10.0, 20.0];
        assert_eq!(key(-1.0, &times, &values, &[LINEAR]), 0.0);
        assert_eq!(key(2.0, &times, &values, &[LINEAR]), 20.0);
        assert!(close(key(0.25, &times, &values, &[LINEAR]), 5.0));
        assert!(close(key(0.75, &times, &values, &[LINEAR]), 15.0));
        // keys that end early hold the last value (the hole's 0.86)
        assert!(close(key(0.95, &[0.0, 0.86], &[1.0, 0.01], &[LINEAR]), 0.01));
    }

    #[test]
    fn a_puff_rises_and_fades() {
        let start = puff(0.0, 0.1, -58.0, -70.0, -30.0, false);
        assert_eq!(start.opacity, 0.0);
        assert!(close(start.dy, 16.0) && close(start.scale, 0.45));
        let mid = puff(0.1 + 0.48 * SMOKE_S, 0.1, -58.0, -70.0, -30.0, false);
        assert!(close(mid.dx, -30.0) && close(mid.dy, -38.5) && close(mid.scale, 1.1));
        assert!(close(mid.opacity, 0.55));
        let end = puff(5.0, 0.1, -58.0, -70.0, -30.0, false);
        assert!(close(end.dx, -58.0) && close(end.dy, -70.0) && close(end.scale, 1.6));
        assert_eq!(end.opacity, 0.0);
        let still = puff(0.18 * SMOKE_STILL_S, 0.0, -58.0, -70.0, -30.0, true);
        assert!(close(still.opacity, 0.8) && close(still.dx, -30.0));
    }

    #[test]
    fn hands_come_down_weigh_and_go_back() {
        assert_eq!(hand(0.0, false, false), (UP, 0.0, 1.0));
        let (y, deg, _) = hand(0.1 * ARMS_S, false, false);
        assert!(close(y, 33.0) && close(deg, 9.0));
        let (y, deg, _) = hand(0.1 * ARMS_S, true, false);
        assert!(close(y, 5.0) && close(deg, 6.0));
        assert!(close(hand(ARMS_S, true, false).0, UP));
        let (_, _, o) = hand(ARMS_S / 2.0, false, true);
        assert!(close(o, 1.0));
    }

    #[test]
    fn digits_turn_once_each_way() {
        assert_eq!(digit(0.0, 1.0, false), (0.0, 0.0));
        assert!(close(digit(FOLD_S, 1.0, false).0, 360.0));
        assert!(close(digit(FOLD_S, -1.0, false).0, -360.0));
        assert!(close(digit(FOLD_S / 2.0, 1.0, false).1, 1.0));
        assert_eq!(digit(FOLD_S / 2.0, 1.0, true).0, 0.0);
    }

    #[test]
    fn the_hole_opens_swallows_and_closes() {
        let (s, o, _) = hole(0.0, false);
        assert!(close(s, 0.01) && close(o, 1.0));
        assert!(close(hole(0.4 * VOID_S, false).0, 1.0));
        assert!(close(hole(0.74 * VOID_S, false).0, 1.25));
        assert!(close(hole(VOID_S, false).0, 0.01));
        assert!(close(hole(VOID_S, false).1, 0.0));
        // the copy sits on the answer until it falls, and ends in the hole, gone
        let (x, y, s, _, o) = fall(0.0, (300.0, 50.0), (280.0, 50.0));
        assert!(close(x, 300.0) && close(y, 50.0) && close(s, 1.0) && close(o, 1.0));
        let (x, y, s, _, o) = fall(FALL_BEGIN + FALL_S, (300.0, 50.0), (280.0, 50.0));
        assert!(close(x, 280.0) && close(y, 50.0) && close(s, 0.05) && close(o, 0.0));
        assert_eq!(veil(1.0), 1.0);
        assert_eq!(veil(VEIL_S + VEIL_FADE), 0.0);
    }

    #[test]
    fn confetti_bursts_up_and_falls() {
        let bits = burst(7);
        assert_eq!(bits.len(), 55);
        assert_eq!(bits, burst(7));
        for b in &bits {
            assert!(b.vy < 0.0, "every bit leaves upward");
            assert!(b.born <= 0.1 && (1.2..=1.8).contains(&b.life));
            let gone = b.at(b.born + b.life + 0.01);
            assert!(gone.is_none());
        }
        let b = bits[0];
        let a = b.at(b.born + 1.0).unwrap();
        assert!(close(a.dy, b.vy + 0.5 * GRAVITY));
        assert!(close(a.opacity, 0.4));
        assert!(b.at(b.born - 0.01).is_none());
        assert!(note(0.0, false) == 0.0 && close(note(0.25 + 0.5 * 2.1, false), 0.9));
    }

    #[test]
    fn pi_digits_group_fit_and_roll_out() {
        let g = grouped("3.14159265358979323846");
        assert_eq!(g, "3.14159 26535 89793 23846");
        assert_eq!(fitted(&g, 7.0, 100.0), "3.14159 26535");
        assert_eq!(fitted(&g, 7.0, 1000.0), g);
        assert_eq!(fitted(&g, 7.0, 10.0), "3.14159");
        let n = g.chars().count();
        assert_eq!(digits_shown(0.0, n, false), 7);
        assert_eq!(digits_shown(10.0, n, false), n);
        assert_eq!(digits_shown(0.0, n, true), n);
        let mid = digits_shown(0.1 + 0.015 * n as f64, n, false);
        assert!(mid > 7 && mid < n);
    }

    #[test]
    fn the_roll_turns_once() {
        assert_eq!(roll(0.0), (0.0, 1.0));
        let (a, s) = roll(0.5);
        assert!(close(a, PI * 0.999) && close(s, 0.9));
        let (a, s) = roll(5.0);
        assert!(close(a, 2.0 * PI * 0.999) && close(s, 1.0));
        assert!(roll_room(680.0, 72.0) > 300.0);
        assert_eq!(roll_room(100.0, 100.0), 200.0);
    }

    #[test]
    fn phosphor_turns_light_green() {
        let (r, g, b) = phosphor(1.0, 1.0, 1.0);
        assert!(g > r && g > b && g > 0.9);
        assert_eq!(phosphor(0.0, 0.0, 0.0), (0.0, 0.0, 0.0));
    }
}
