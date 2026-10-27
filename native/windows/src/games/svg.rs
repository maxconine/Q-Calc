//! An svg path's `d` (absolute M, L, H, V, C, Q, Z, as the games' TS writes them; the Mac's SVGPath.swift) as
//! polylines, curves flattened, so a stroke can be drawn in part (`trim`, the chalk's draw-in).

pub type Pt = (f32, f32);

/// The subpaths, each a polyline; a closed one ends on its start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poly {
    pub parts: Vec<Vec<Pt>>,
}

const CURVE_STEPS: usize = 10;

pub fn parse(d: &str) -> Poly {
    let mut parts: Vec<Vec<Pt>> = Vec::new();
    let mut nums: Vec<f32> = Vec::new();
    let mut cmd = 'M';
    let mut cur: Pt = (0.0, 0.0);
    let mut start: Pt = (0.0, 0.0);

    let mut flush = |cmd: char, nums: &mut Vec<f32>, parts: &mut Vec<Vec<Pt>>| {
        let mut i = 0;
        let mut take = |n: usize| -> Option<Vec<f32>> {
            if i + n > nums.len() {
                return None;
            }
            let v = nums[i..i + n].to_vec();
            i += n;
            Some(v)
        };
        let line_to = |parts: &mut Vec<Vec<Pt>>, p: Pt| {
            if parts.is_empty() {
                parts.push(vec![p]);
            } else if let Some(last) = parts.last_mut() {
                last.push(p);
            }
        };
        match cmd {
            'M' | 'L' => {
                let mut first = cmd == 'M';
                while let Some(p) = take(2) {
                    cur = (p[0], p[1]);
                    if first {
                        parts.push(vec![cur]);
                        start = cur;
                        first = false;
                    } else {
                        line_to(parts, cur);
                    }
                }
            }
            'H' => {
                while let Some(p) = take(1) {
                    cur = (p[0], cur.1);
                    line_to(parts, cur);
                }
            }
            'V' => {
                while let Some(p) = take(1) {
                    cur = (cur.0, p[0]);
                    line_to(parts, cur);
                }
            }
            'C' => {
                while let Some(p) = take(6) {
                    let (p0, c1, c2, p3) = (cur, (p[0], p[1]), (p[2], p[3]), (p[4], p[5]));
                    for k in 1..=CURVE_STEPS {
                        let t = k as f32 / CURVE_STEPS as f32;
                        let u = 1.0 - t;
                        let x = u * u * u * p0.0
                            + 3.0 * u * u * t * c1.0
                            + 3.0 * u * t * t * c2.0
                            + t * t * t * p3.0;
                        let y = u * u * u * p0.1
                            + 3.0 * u * u * t * c1.1
                            + 3.0 * u * t * t * c2.1
                            + t * t * t * p3.1;
                        line_to(parts, (x, y));
                    }
                    cur = p3;
                }
            }
            'Q' => {
                while let Some(p) = take(4) {
                    let (p0, c, p2) = (cur, (p[0], p[1]), (p[2], p[3]));
                    for k in 1..=CURVE_STEPS {
                        let t = k as f32 / CURVE_STEPS as f32;
                        let u = 1.0 - t;
                        line_to(
                            parts,
                            (
                                u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p2.0,
                                u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p2.1,
                            ),
                        );
                    }
                    cur = p2;
                }
            }
            'Z' | 'z' => {
                line_to(parts, start);
                cur = start;
            }
            _ => {}
        }
        nums.clear();
    };

    let mut token = String::new();
    let end = |token: &mut String, nums: &mut Vec<f32>| {
        if let Ok(v) = token.parse::<f32>() {
            nums.push(v);
        }
        token.clear();
    };
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            end(&mut token, &mut nums);
            flush(cmd, &mut nums, &mut parts);
            cmd = ch;
        } else if ch == ' ' || ch == ',' {
            end(&mut token, &mut nums);
        } else if ch == '-' && !token.is_empty() && !token.ends_with(['e', 'E']) {
            end(&mut token, &mut nums);
            token.push('-');
        } else {
            token.push(ch);
        }
    }
    end(&mut token, &mut nums);
    flush(cmd, &mut nums, &mut parts);
    Poly { parts }
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

impl Poly {
    pub fn length(&self) -> f32 {
        self.parts
            .iter()
            .map(|p| p.windows(2).map(|w| dist(w[0], w[1])).sum::<f32>())
            .sum()
    }

    /// The first `t` (0…1) of the path by length (`trimmedPath(from: 0, to: t)`).
    pub fn trim(&self, t: f32) -> Poly {
        if t >= 1.0 {
            return self.clone();
        }
        let mut left = self.length() * t.max(0.0);
        let mut out = Vec::new();
        for part in &self.parts {
            if left <= 0.0 {
                break;
            }
            let mut kept = vec![part[0]];
            for w in part.windows(2) {
                if left <= 0.0 {
                    break;
                }
                let d = dist(w[0], w[1]);
                if d <= left {
                    kept.push(w[1]);
                    left -= d;
                } else {
                    let k = if d > 0.0 { left / d } else { 0.0 };
                    kept.push((
                        w[0].0 + (w[1].0 - w[0].0) * k,
                        w[0].1 + (w[1].1 - w[0].1) * k,
                    ));
                    left = 0.0;
                    break;
                }
            }
            out.push(kept);
        }
        Poly { parts: out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_commands_the_chalk_writes() {
        let p = parse("M0 0L10 0H20V10Z");
        assert_eq!(p.parts.len(), 1);
        assert_eq!(
            p.parts[0],
            vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (20.0, 0.0),
                (20.0, 10.0),
                (0.0, 0.0)
            ]
        );
        let c = parse("M0,0 C0,10 10,10 10,0 M5-5Q6,7 8,9");
        assert_eq!(c.parts.len(), 2);
        assert_eq!(c.parts[0].len(), 1 + CURVE_STEPS);
        assert_eq!(*c.parts[0].last().unwrap(), (10.0, 0.0));
        assert_eq!(c.parts[1][0], (5.0, -5.0));
        assert_eq!(parse("M1e1 2L3 4").parts[0][0], (10.0, 2.0));
    }

    #[test]
    fn trims_by_length() {
        let p = parse("M0 0L10 0L10 10");
        assert_eq!(p.length(), 20.0);
        let half = p.trim(0.5);
        assert_eq!(half.parts[0], vec![(0.0, 0.0), (10.0, 0.0)]);
        let q = p.trim(0.25);
        assert_eq!(q.parts[0], vec![(0.0, 0.0), (5.0, 0.0)]);
        assert_eq!(p.trim(1.0), p);
    }
}
