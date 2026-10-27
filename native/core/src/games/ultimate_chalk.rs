//! `src/components/ultimateChalk.ts`: the chalk look for ultimate tic-tac-toe. every line, x and o as an svg path
//! with a hand-drawn wobble, baked into the path from a seed, so a mark keeps its shape from one redraw to the next.
//! Paths are svg text (`M x y C …`), as the page drew them; the shell turns them into its own paths.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::{LazyLock, Mutex};

use super::{js_round, mulberry32};

type Pt = (f64, f64);

fn f(n: f64) -> String {
    crate::jsnum::to_string(js_round(n * 10.0) / 10.0)
}

// a stroke from a to b that bows a little either side and overshoots its ends, the way chalk does
fn wobble(a: Pt, b: Pt, amp: f64, r: &mut dyn FnMut() -> f64) -> String {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = {
        let h = dx.hypot(dy);
        if h == 0.0 { 1.0 } else { h }
    };
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);
    let mut jit = |r: &mut dyn FnMut() -> f64| (r() - 0.5) * 2.0 * amp;
    // the TS evaluates these left to right: r() for the overshoot, then jit() for the normal
    let s0 = a.0 - ux * r() * amp + nx * jit(r) * 0.5;
    let s1 = a.1 - uy * r() * amp + ny * jit(r) * 0.5;
    let e0 = b.0 + ux * r() * amp + nx * jit(r) * 0.5;
    let e1 = b.1 + uy * r() * amp + ny * jit(r) * 0.5;
    let o1 = jit(r);
    let o2 = jit(r);
    let c1 = (a.0 + dx / 3.0 + nx * o1, a.1 + dy / 3.0 + ny * o1);
    let c2 = (a.0 + 2.0 * dx / 3.0 + nx * o2, a.1 + 2.0 * dy / 3.0 + ny * o2);
    format!("M{} {}C{} {} {} {} {} {}", f(s0), f(s1), f(c1.0), f(c1.1), f(c2.0), f(c2.1), f(e0), f(e1))
}

// a loop around (cx, cy) that isn't quite round and runs a little past where it started
fn chalk_loop(cx: f64, cy: f64, radius: f64, r: &mut dyn FnMut() -> f64) -> String {
    let n = 14;
    let start = r() * PI * 2.0;
    let sweep = PI * 2.0 * (1.06 + r() * 0.06);
    let squash = 0.92 + r() * 0.1;
    let mut pts: Vec<Pt> = vec![];
    for i in 0..=n {
        let t = start + sweep * i as f64 / n as f64;
        let rad = radius * (0.94 + r() * 0.1) * (1.0 - i as f64 / n as f64 * 0.05);
        pts.push((cx + t.cos() * rad, cy + t.sin() * rad * squash));
    }
    // catmull-rom through the points, as cubic beziers
    let mut d = format!("M{} {}", f(pts[0].0), f(pts[0].1));
    for i in 0..pts.len() - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        d += &format!("C{} {} {} {} {} {}", f(c1.0), f(c1.1), f(c2.0), f(c2.1), f(p2.0), f(p2.1));
    }
    d
}

static CACHE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached(key: String, make: impl FnOnce() -> String) -> String {
    if let Some(d) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return d;
    }
    let d = make();
    if let Ok(mut c) = CACHE.lock() {
        c.insert(key, d.clone());
    }
    d
}

fn seed_of(seed: f64) -> u32 {
    // `seed >>> 0`
    (seed.trunc() as i64).rem_euclid(1 << 32) as u32
}

/// An x or an o filling the square at (x, y) of side size; seed picks its wobble.
pub fn chalk_mark(kind: char, x: f64, y: f64, size: f64, seed: f64) -> String {
    cached(format!("{kind}{x},{y},{size},{seed}"), || {
        let mut r = mulberry32(seed_of(seed * 7919.0 + if kind == 'x' { 1.0 } else { 2.0 }));
        let pad = size * 0.25;
        let amp = size * 0.045;
        if kind == 'o' {
            return chalk_loop(x + size / 2.0, y + size / 2.0, size / 2.0 - pad, &mut r);
        }
        wobble((x + pad, y + pad), (x + size - pad, y + size - pad), amp, &mut r)
            + &wobble((x + size - pad, y + pad), (x + pad, y + size - pad), amp, &mut r)
    })
}

/// A chalk line from a to b.
pub fn chalk_line(a: Pt, b: Pt, seed: f64, amp: f64) -> String {
    cached(format!("l{},{},{},{},{seed},{amp}", a.0, a.1, b.0, b.1), || wobble(a, b, amp, &mut mulberry32(seed_of(seed))))
}

/// A rough box around (x, y, w, h): four strokes that don't quite meet.
pub fn chalk_box(x: f64, y: f64, w: f64, h: f64, seed: f64) -> String {
    cached(format!("b{x},{y},{w},{h},{seed}"), || {
        let mut r = mulberry32(seed_of(seed));
        let a = 1.2;
        wobble((x, y), (x + w, y), a, &mut r)
            + &wobble((x + w, y), (x + w, y + h), a, &mut r)
            + &wobble((x + w, y + h), (x, y + h), a, &mut r)
            + &wobble((x, y + h), (x, y), a, &mut r)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // from the TS (node, components/ultimateChalk.ts)
    #[test]
    fn matches_the_page() {
        assert_eq!(chalk_line((0.0, 0.0), (100.0, 0.0), 1.0, 1.1), "M-0.7 0.5C33.3 -0.2 66.7 1.1 101.1 0.2");
        assert_eq!(chalk_mark('x', 0.0, 0.0, 30.0, 4.0), "M6.8 7.6C13.2 11.8 18.1 16.9 23.1 22.6M23.4 7C17.6 12.6 11.8 16.8 6.7 23.9");
        assert_eq!(
            chalk_mark('o', 30.0, 60.0, 30.0, 17.0),
            "M50.8 79.6C50.3 79.9 48.8 81.2 47.7 81.5C46.5 81.9 45.1 82 44 81.8C42.8 81.6 41.8 80.8 40.8 80.2C39.8 79.5 38.6 78.8 38.1 77.9C37.6 76.9 37.6 75.6 37.8 74.5C37.9 73.4 38.4 72.4 39 71.4C39.6 70.5 40.5 69.4 41.5 68.9C42.5 68.5 43.9 68.4 45 68.4C46.2 68.5 47.3 68.9 48.4 69.4C49.4 69.8 50.7 70.4 51.3 71.3C51.9 72.2 52.1 73.5 52.1 74.6C52.1 75.7 52 76.9 51.5 77.8C51 78.7 50.1 79.7 49.2 80.3C48.2 80.8 46.4 81.1 45.9 81.2"
        );
        assert_eq!(
            chalk_box(-4.0, -4.0, 98.0, 98.0, 300.0),
            "M-5 -3.9C28.7 -2.9 61.3 -5 94.2 -4M93.9 -4C93.1 28.7 94.8 61.3 94.2 94.4M94.5 94.1C61.3 94.8 28.7 94.2 -4.4 93.6M-4.3 94.6C-4.8 61.3 -3 28.7 -3.6 -4.3"
        );
        assert_eq!(chalk_line((96.0, -4.0), (96.0, 306.0), 101.0, 1.6), "M95.6 -4.9C96.9 99.3 97.4 202.7 95.7 306.9");
    }
}
