//! The effects' pictures, pure: the 67's hand (SixtySevenArms.tsx's SVG paths, read by a small path parser) and
//! the 420's puff (a radial gradient clipped to its ellipse and blurred 18 px, as `.four-twenty-puff` draws it),
//! rendered once into premultiplied BGRA so the overlay only scales and fades it.

/// One step of an SVG path, absolute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Quad(f32, f32, f32, f32),
    Close,
}

/// Reads the absolute commands the hand uses (M L H V C Q Z).
pub fn parse(d: &str) -> Vec<Seg> {
    let mut tokens: Vec<Result<char, f32>> = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, tokens: &mut Vec<Result<char, f32>>| {
        if !num.is_empty() {
            if let Ok(v) = num.parse() {
                tokens.push(Err(v));
            }
            num.clear();
        }
    };
    for c in d.chars() {
        if c.is_ascii_alphabetic() {
            flush(&mut num, &mut tokens);
            tokens.push(Ok(c));
        } else if c == '-' && !num.is_empty() && !num.ends_with('e') {
            flush(&mut num, &mut tokens);
            num.push(c);
        } else if c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' {
            if c == '.' && num.contains('.') {
                flush(&mut num, &mut tokens);
            }
            num.push(c);
        } else {
            flush(&mut num, &mut tokens);
        }
    }
    flush(&mut num, &mut tokens);
    let mut out = Vec::new();
    let mut i = 0;
    let mut cmd = 'M';
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let take = |i: &mut usize, n: usize| -> Option<Vec<f32>> {
        let v: Vec<f32> = tokens[*i..]
            .iter()
            .take(n)
            .map_while(|t| t.err())
            .collect();
        (v.len() == n).then(|| {
            *i += n;
            v
        })
    };
    while i < tokens.len() {
        if let Ok(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                out.push(Seg::Close);
                continue;
            }
        }
        let seg = match cmd {
            'M' => take(&mut i, 2).map(|v| {
                cmd = 'L';
                Seg::Move(v[0], v[1])
            }),
            'L' => take(&mut i, 2).map(|v| Seg::Line(v[0], v[1])),
            'H' => take(&mut i, 1).map(|v| Seg::Line(v[0], y)),
            'V' => take(&mut i, 1).map(|v| Seg::Line(x, v[0])),
            'C' => take(&mut i, 6).map(|v| Seg::Cubic(v[0], v[1], v[2], v[3], v[4], v[5])),
            'Q' => take(&mut i, 4).map(|v| Seg::Quad(v[0], v[1], v[2], v[3])),
            _ => None,
        };
        let Some(seg) = seg else {
            break;
        };
        match seg {
            Seg::Move(a, b) | Seg::Line(a, b) | Seg::Quad(_, _, a, b) => (x, y) = (a, b),
            Seg::Cubic(_, _, _, _, a, b) => (x, y) = (a, b),
            Seg::Close => {}
        }
        out.push(seg);
    }
    out
}

/// The hand's palm (filled) and fingers (stroked, with their widths), in a 44 × 30 box.
pub const PALM: &str = "M4 20.8C4 16.2 7.4 13.6 12 13.5L20.2 13.8C23 14 24.2 16.4 24.2 19.8C24.2 24.3 21 26.9 15.6 26.9H11C6.8 26.9 4 24.6 4 20.8Z";
pub const FINGERS: [(&str, f32); 5] = [
    ("M20 15Q29 15.2 35.6 11.2", 3.4),
    ("M21 18.4Q31 19.2 38.8 15.9", 3.5),
    ("M21 21.9Q31 23 38.4 20.6", 3.3),
    ("M20 25.2Q28 26.5 34.2 25", 3.0),
    ("M10.5 14.5Q12.6 8.6 18 6", 4.0),
];
pub const HAND_W: f32 = 44.0;
pub const HAND_H: f32 = 30.0;

/// The puff's padding for its blur (3σ), DIPs.
pub const PUFF_PAD: f64 = 54.0;
/// blur(18px)
pub const PUFF_SIGMA: f64 = 18.0;

/// A premultiplied BGRA picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// The gradient's stops: (where, r, g, b, a), css's radial-gradient on `.four-twenty-puff`.
const STOPS: [(f64, f64, f64, f64, f64); 5] = [
    (0.0, 1.0, 1.0, 1.0, 0.62),
    (0.28, 1.0, 1.0, 1.0, 0.5),
    (0.5, 230.0 / 255.0, 230.0 / 255.0, 234.0 / 255.0, 0.26),
    (0.64, 215.0 / 255.0, 215.0 / 255.0, 220.0 / 255.0, 0.08),
    (0.72, 0.0, 0.0, 0.0, 0.0),
];

/// The gradient at `g` (0 the centre, 1 the farthest corner), premultiplied, interpolated premultiplied as css does.
fn stop_at(g: f64) -> [f64; 4] {
    let pm = |s: (f64, f64, f64, f64, f64)| [s.1 * s.4, s.2 * s.4, s.3 * s.4, s.4];
    if g <= STOPS[0].0 {
        return pm(STOPS[0]);
    }
    for w in STOPS.windows(2) {
        if g <= w[1].0 {
            let u = (g - w[0].0) / (w[1].0 - w[0].0);
            let (a, b) = (pm(w[0]), pm(w[1]));
            return [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * u);
        }
    }
    [0.0; 4]
}

/// The three box widths whose passes make a gaussian of `sigma` (px).
fn boxes(sigma: f64) -> [usize; 3] {
    let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut wl = ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - 3.0 * (wl * wl) as f64 - 12.0 * wl as f64 - 9.0)
        / (-4.0 * wl as f64 - 4.0))
        .round() as i64;
    [0, 1, 2].map(|i| (if i < m { wl } else { wu }).max(1) as usize)
}

/// One box pass along rows (`stride` 1) or columns, with transparent beyond the edges.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, rows: bool) {
    let (lines, len) = if rows { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| if rows { line * w + i } else { i * w + line };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = 0.0f32;
        for i in 0..=r.min(len - 1) {
            sum += src[at(line, i)];
        }
        for i in 0..len {
            dst[at(line, i)] = sum * norm;
            if i + r + 1 < len {
                sum += src[at(line, i + r + 1)];
            }
            if i >= r {
                sum -= src[at(line, i - r)];
            }
        }
    }
}

/// The puff of a `w` × `h` box, at `res` pixels a DIP: its padded picture.
pub fn puff(w: f64, h: f64, res: f64) -> Image {
    let pw = ((w + 2.0 * PUFF_PAD) * res).ceil().max(1.0) as usize;
    let ph = ((h + 2.0 * PUFF_PAD) * res).ceil().max(1.0) as usize;
    let mut planes = vec![vec![0f32; pw * ph]; 4];
    // farthest-corner: the ellipse through the box's corners, w/2·√2 by h/2·√2
    let (gx, gy) = (w / 2.0 * 2f64.sqrt(), h / 2.0 * 2f64.sqrt());
    let edge = w.min(h) / 2.0 * res;
    for j in 0..ph {
        let y = (j as f64 + 0.5) / res - PUFF_PAD - h / 2.0;
        for i in 0..pw {
            let x = (i as f64 + 0.5) / res - PUFF_PAD - w / 2.0;
            // border-radius 50%: clipped to the box's own ellipse, a pixel of softness
            let e = ((x / (w / 2.0)).powi(2) + (y / (h / 2.0)).powi(2)).sqrt();
            let cover = (0.5 + (1.0 - e) * edge).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let g = ((x / gx).powi(2) + (y / gy).powi(2)).sqrt();
            let c = stop_at(g);
            for k in 0..4 {
                planes[k][j * pw + i] = (c[k] * cover) as f32;
            }
        }
    }
    let mut tmp = vec![0f32; pw * ph];
    for plane in &mut planes {
        for b in boxes(PUFF_SIGMA * res) {
            let r = (b - 1) / 2;
            box_pass(plane, &mut tmp, pw, ph, r, true);
            box_pass(&tmp, plane, pw, ph, r, false);
        }
    }
    let mut px = vec![0u8; pw * ph * 4];
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for i in 0..pw * ph {
        let a = planes[3][i];
        // B G R A, colour never above alpha
        px[i * 4] = q(planes[2][i].min(a));
        px[i * 4 + 1] = q(planes[1][i].min(a));
        px[i * 4 + 2] = q(planes[0][i].min(a));
        px[i * 4 + 3] = q(a);
    }
    Image {
        w: pw as u32,
        h: ph as u32,
        px,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_hand() {
        let palm = parse(PALM);
        assert_eq!(palm[0], Seg::Move(4.0, 20.8));
        assert_eq!(palm[1], Seg::Cubic(4.0, 16.2, 7.4, 13.6, 12.0, 13.5));
        assert_eq!(palm[2], Seg::Line(20.2, 13.8));
        // H keeps the y
        assert!(palm.contains(&Seg::Line(11.0, 26.9)));
        assert_eq!(*palm.last().unwrap(), Seg::Close);
        assert_eq!(palm.len(), 8);
        for (d, _) in FINGERS {
            let f = parse(d);
            assert_eq!(f.len(), 2);
            assert!(matches!(f[1], Seg::Quad(..)));
        }
        assert_eq!(
            parse("M1-2L3 4"),
            vec![Seg::Move(1.0, -2.0), Seg::Line(3.0, 4.0)]
        );
    }

    #[test]
    fn gradient_stops() {
        assert_eq!(stop_at(0.0)[3], 0.62);
        assert_eq!(stop_at(0.8), [0.0; 4]);
        let mid = stop_at(0.39);
        assert!(mid[3] < 0.5 && mid[3] > 0.26);
        assert_eq!(boxes(9.0).len(), 3);
    }

    #[test]
    fn a_puff_is_soft_white_and_fades_out() {
        let img = puff(150.0, 100.0, 0.5);
        assert_eq!(img.w, ((150.0 + 108.0) * 0.5f64).ceil() as u32);
        let at = |x: u32, y: u32| {
            let i = ((y * img.w + x) * 4) as usize;
            [img.px[i], img.px[i + 1], img.px[i + 2], img.px[i + 3]]
        };
        let centre = at(img.w / 2, img.h / 2);
        assert!(centre[3] > 60 && centre[3] < 170, "{centre:?}");
        assert!(centre[0] <= centre[3] && centre[2] <= centre[3]);
        assert_eq!(at(0, 0)[3], 0);
        // softer at the box's edge than in the middle: the blur
        let edge = at(img.w / 2, (54.0 * 0.5) as u32);
        assert!(edge[3] < centre[3] && edge[3] > 0);
    }
}
