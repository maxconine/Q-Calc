//! Double-precision quadrature, limits and derivatives (`src/engine/calcNumeric.ts`). Every result carries an
//! error bound, or is None. Float maths goes through `jsmath` (V8's `Math`) so every sample matches the TS.

use crate::jsnum;
use crate::scientific::functions::jsmath;
use crate::work::WorkBudget;

/// A real function of one variable, as the engine compiles a body.
pub type RealFn<'a> = &'a dyn Fn(f64) -> f64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub value: f64,
    pub err: f64,
}

/// An estimate with the L1 size of the sampled terms, the scale its error is judged against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sized {
    pub value: f64,
    pub err: f64,
    pub l1: f64,
}

const HALF_PI: f64 = std::f64::consts::PI / 2.0;
const MAX_LEVEL: u32 = 7;
const MAX_EVALS: i64 = 40_000;
// the answer is recomputed on every keystroke; a slow user function gets a blank rather than lag
const MAX_WORK: u64 = 4_000_000;

struct Budget {
    evals: i64,
    work: WorkBudget,
}

fn spent(b: &mut Budget) -> bool {
    b.evals -= 1;
    b.evals < 0 || (b.evals % 64 == 0 && b.work.over())
}

fn log10(x: f64) -> f64 {
    jsmath::log10(x)
}

/// `Number(value.toPrecision(p))`.
fn to_precision_num(value: f64, p: usize) -> f64 {
    jsnum::parse(&jsnum::to_precision(value, p)).unwrap_or(f64::NAN)
}

/// Keeps only the digits `err` vouches for, less one and at most 13; None when fewer than 3 survive.
pub fn justified(value: f64, err: f64, scale: Option<f64>) -> Option<f64> {
    let scale = scale.unwrap_or(value.abs());
    if !value.is_finite() {
        return None;
    }
    if err.is_nan() || err < 0.0 {
        return None;
    }
    // indistinguishable from zero at the scale of the problem (∫-1..1 x³, √x at 0)
    if value.abs() <= 10.0 * err {
        return if 10.0 * err <= 1e-9 * scale.max(1e-300) {
            Some(0.0)
        } else {
            None
        };
    }
    let digits = if err > 0.0 {
        log10(value.abs()).floor() - log10(err).floor() - 1.0
    } else {
        13.0
    };
    if digits < 3.0 {
        return None;
    }
    Some(to_precision_num(value, digits.min(13.0) as usize))
}

#[derive(Clone, Copy, Debug)]
struct Cut {
    f: f64,
    dist: f64,
}

struct Side {
    sum: f64,
    l1: f64,
    cut: Option<Cut>,
    ok: bool,
}

#[derive(Clone, Copy)]
struct NodeW {
    x: f64,
    w: f64,
    dist: f64,
}

/// One side of a double exponential rule at step h (odd multiples only when `odd`).
fn sweep(
    f: RealFn,
    node: &dyn Fn(f64) -> Option<NodeW>,
    h: f64,
    odd: bool,
    sign: f64,
    budget: &mut Budget,
) -> Side {
    let mut sum = 0.0;
    let mut l1 = 0.0;
    let mut small = 0;
    // a float blowup far out on an infinite range is harmless once the terms there no longer matter
    let mut negligible = false;
    let mut last = Cut { f: 0.0, dist: 0.0 };
    let mut k: i64 = 1;
    loop {
        let t = sign * k as f64 * h;
        k += if odd { 2 } else { 1 };
        if t.abs() > 7.0 {
            return Side {
                sum,
                l1,
                cut: None,
                ok: false,
            };
        }
        let Some(n) = node(t) else {
            // far out on an infinite range the terms must already have died away
            if last.dist == f64::INFINITY {
                return Side {
                    sum,
                    l1,
                    cut: None,
                    ok: negligible,
                };
            }
            return Side {
                sum,
                l1,
                cut: Some(last),
                ok: true,
            };
        };
        if spent(budget) {
            return Side {
                sum,
                l1,
                cut: None,
                ok: false,
            };
        }
        let fx = f(n.x);
        if !fx.is_finite() {
            if negligible && n.dist == f64::INFINITY {
                return Side {
                    sum,
                    l1,
                    cut: None,
                    ok: true,
                };
            }
            // 0/0 or x/0 right at an end: the cut's tail estimate keeps it honest
            if n.dist < 1e-12 && last.dist > 0.0 {
                return Side {
                    sum,
                    l1,
                    cut: Some(last),
                    ok: true,
                };
            }
            return Side {
                sum,
                l1,
                cut: None,
                ok: false,
            };
        }
        let term = n.w * fx;
        sum += term;
        l1 += term.abs();
        last = Cut {
            f: fx,
            dist: n.dist,
        };
        negligible = term.abs() <= 1e-15 * l1;
        if term.abs() <= 1e-18 * l1 || n.w == 0.0 {
            small += 1;
            if small >= 3 && t.abs() > 2.0 {
                return Side {
                    sum,
                    l1,
                    cut: None,
                    ok: true,
                };
            }
        } else {
            small = 0;
        }
    }
}

struct Rule {
    node: Box<dyn Fn(f64) -> Option<NodeW>>,
    center: (f64, f64),
}

fn tanh_sinh_rule(a: f64, b: f64) -> Rule {
    let r = (b - a) / 2.0;
    Rule {
        center: ((a + b) / 2.0, r * HALF_PI),
        node: Box::new(move |t| {
            let u = HALF_PI * jsmath::sinh(t.abs());
            let e2u = jsmath::exp(2.0 * u);
            // distance from the nearest end, computed directly so it keeps its digits
            let delta = 1.0 / (e2u + 1.0);
            let dist = 2.0 * r * delta;
            let ch = jsmath::cosh(u);
            let w = (r * HALF_PI * jsmath::cosh(t)) / (ch * ch);
            let x = if t > 0.0 { b - dist } else { a + dist };
            if !w.is_finite() || dist == 0.0 || x == a || x == b {
                return None;
            }
            Some(NodeW { x, w, dist })
        }),
    }
}

/// [a, ∞): x = a + exp(π/2 sinh t).
fn exp_sinh_rule(a: f64) -> Rule {
    Rule {
        center: (a + 1.0, HALF_PI),
        node: Box::new(move |t| {
            let e = jsmath::exp(HALF_PI * jsmath::sinh(t));
            let x = a + e;
            let w = HALF_PI * jsmath::cosh(t) * e;
            if !x.is_finite() || !w.is_finite() || x == a {
                return None;
            }
            Some(NodeW {
                x,
                w,
                dist: if t < 0.0 { e } else { f64::INFINITY },
            })
        }),
    }
}

/// (-∞, ∞): x = sinh(π/2 sinh t).
fn sinh_sinh_rule() -> Rule {
    Rule {
        center: (0.0, HALF_PI),
        node: Box::new(|t| {
            let u = HALF_PI * jsmath::sinh(t);
            let x = jsmath::sinh(u);
            let w = HALF_PI * jsmath::cosh(t) * jsmath::cosh(u);
            if !x.is_finite() || !w.is_finite() {
                return None;
            }
            Some(NodeW {
                x,
                w,
                dist: f64::INFINITY,
            })
        }),
    }
}

/// Double exponential quadrature, refined until two levels agree.
fn double_exponential(f: RealFn, rule: &Rule, budget: &mut Budget) -> Option<Sized> {
    let c = f(rule.center.0);
    budget.evals -= 1;
    if !c.is_finite() {
        return None;
    }
    let mut h = 1.0;
    let mut raw = rule.center.1 * c;
    let mut l1 = raw.abs();
    // the sliver past the closest node any level reached, on each side, is left out of the sum
    let mut cuts: [Option<Cut>; 2] = [None, None];
    let mut run = |odd: bool, h: f64, raw: &mut f64, l1: &mut f64, budget: &mut Budget| -> bool {
        for (i, sign) in [1.0, -1.0].into_iter().enumerate() {
            let s = sweep(f, &rule.node, h, odd, sign, budget);
            if !s.ok {
                return false;
            }
            *raw += s.sum;
            *l1 += s.l1;
            if let Some(cut) = s.cut
                && cuts[i].is_none_or(|p| cut.dist < p.dist)
            {
                cuts[i] = Some(cut);
            }
        }
        true
    };
    if !run(false, h, &mut raw, &mut l1, budget) {
        return None;
    }
    let mut prev = raw * h;
    let mut diff = f64::INFINITY;
    let mut scale = 0.0;
    for level in 1..=MAX_LEVEL {
        h /= 2.0;
        if !run(true, h, &mut raw, &mut l1, budget) {
            return None;
        }
        let value = raw * h;
        diff = (value - prev).abs();
        scale = l1 * h;
        prev = value;
        // convergence is quadratic: once two levels agree this closely the newer one is at roundoff
        if level >= 3 && diff <= 1e-14 * scale {
            return Some(Sized {
                value,
                err: diff.max(4e-16 * scale) + tail(&cuts),
                l1: scale,
            });
        }
    }
    if diff <= 1e-9 * scale {
        return Some(Sized {
            value: prev,
            err: diff + tail(&cuts),
            l1: scale,
        });
    }
    None
}

fn tail(cuts: &[Option<Cut>; 2]) -> f64 {
    // the TS reduces over [cuts[1], cuts[-1]]
    cuts.iter()
        .fold(0.0, |t, c| t + c.map_or(0.0, |c| c.f.abs() * c.dist * 10.0))
}

const GK_X: [f64; 8] = [
    0.9914553711208126,
    0.9491079123427585,
    0.8648644233597691,
    0.7415311855993945,
    0.5860872354676911,
    0.4058451513773972,
    0.20778495500789848,
    0.0,
];
const GK_WK: [f64; 8] = [
    0.022935322010529224,
    0.06309209262997856,
    0.10479001032225019,
    0.14065325971552592,
    0.1690047266392679,
    0.19035057806478542,
    0.20443294007529889,
    0.20948214108472782,
];
const GK_WG: [f64; 4] = [
    0.1294849661688697,
    0.27970539148927664,
    0.3818300505051189,
    0.4179591836734694,
];

#[derive(Clone, Copy)]
struct Panel {
    a: f64,
    b: f64,
    value: f64,
    err: f64,
    l1: f64,
}

fn gk15(f: RealFn, a: f64, b: f64) -> Option<Panel> {
    let c = (a + b) / 2.0;
    let r = (b - a) / 2.0;
    let mut k = 0.0;
    let mut g = 0.0;
    let mut l1 = 0.0;
    for i in 0..8 {
        let xs: &[f64] = if i == 7 {
            &[c]
        } else {
            &[c - r * GK_X[i], c + r * GK_X[i]]
        };
        for &x in xs {
            let fx = f(x);
            if !fx.is_finite() {
                return None;
            }
            k += GK_WK[i] * fx;
            l1 += GK_WK[i] * fx.abs();
            if i % 2 == 1 {
                g += GK_WG[(i - 1) / 2] * fx;
            }
        }
    }
    let value = k * r;
    let gauss = g * r;
    Some(Panel {
        a,
        b,
        value,
        err: (value - gauss).abs(),
        l1: l1 * r.abs(),
    })
}

/// Adaptive Gauss–Kronrod 7/15, splitting the worst panel until the total error is small enough.
pub fn gauss_kronrod(
    f: RealFn,
    a: f64,
    b: f64,
    max_panels: usize,
    over: &dyn Fn() -> bool,
) -> Option<Sized> {
    let first = gk15(f, a, b)?;
    let mut panels = vec![first];
    for n in 1..max_panels {
        let mut total = 0.0;
        let mut err = 0.0;
        let mut l1 = 0.0;
        let mut worst = 0;
        for (i, p) in panels.iter().enumerate() {
            total += p.value;
            err += p.err;
            l1 += p.l1;
            if p.err > panels[worst].err {
                worst = i;
            }
        }
        if err <= (1e-11 * l1).max(1e-300) {
            return Some(Sized {
                value: total,
                err: err.max(4e-16 * l1),
                l1,
            });
        }
        if n % 16 == 0 && over() {
            return None;
        }
        let p = panels[worst];
        let m = (p.a + p.b) / 2.0;
        if m <= p.a || m >= p.b {
            return None;
        }
        let left = gk15(f, p.a, m)?;
        let right = gk15(f, m, p.b)?;
        panels.splice(worst..=worst, [left, right]);
    }
    None
}

fn finite(f: RealFn, a: f64, b: f64, budget: &mut Budget) -> Option<Sized> {
    let est = double_exponential(f, &tanh_sinh_rule(a, b), budget);
    // a spike or kink the double exponential rule stepped over shows up as disagreement here
    let work = budget.work;
    let max = if est.is_some() {
        200
    } else {
        (MAX_EVALS / 30) as usize
    };
    let gk = gauss_kronrod(f, a, b, max, &|| work.over());
    // a check that only ever saw zeros has checked nothing
    let check = gk.filter(|g| g.l1 > 0.0);
    if let (Some(e), Some(c)) = (est, check)
        && (e.value - c.value).abs() > 10.0 * (e.err + c.err) + 1e-9 * e.l1
    {
        return None;
    }
    let out = est.or(check);
    // every sample exactly 0 could still have stepped over a bump
    match out {
        Some(o) if o.l1 == 0.0 && hidden_bump(f, a, b) => None,
        o => o,
    }
}

/// The sharpest peaks or dips an even scan sees, as split points.
fn peaks(f: RealFn, a: f64, b: f64, budget: &mut Budget) -> Vec<f64> {
    let n = 512;
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    for i in 1..n {
        let x = a + ((b - a) * i as f64) / n as f64;
        let y = f(x);
        // a pole inside is for the rules to find
        if !y.is_finite() || spent(budget) {
            return Vec::new();
        }
        xs.push(x);
        ys.push(y);
    }
    let top = ys.iter().fold(f64::NEG_INFINITY, |m, y| m.max(y.abs()));
    let mut out: Vec<(usize, f64)> = Vec::new();
    for i in 1..ys.len() - 1 {
        let (l, c, r) = (ys[i - 1], ys[i], ys[i + 1]);
        let k = (2.0 * c - l - r).abs();
        if ((c > l && c > r) || (c < l && c < r)) && k > 1e-6 * top {
            out.push((i, k));
        }
    }
    // a stable sort by sharpness, as Array.prototype.sort is
    out.sort_by(|p, q| q.1.partial_cmp(&p.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut cuts: Vec<f64> = out
        .iter()
        .take(4)
        .map(|&(i, _)| {
            let sign = if ys[i] > ys[i - 1] { -1.0 } else { 1.0 };
            extremum(f, xs[i - 1], xs[i + 1], sign)
        })
        .collect();
    cuts.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept = Vec::new();
    for (i, &x) in cuts.iter().enumerate() {
        if x > a && x < b && (i == 0 || x != cuts[i - 1]) {
            kept.push(x);
        }
    }
    kept
}

/// Golden section down to adjacent floats, so the split lands on the peak or kink itself.
fn extremum(f: RealFn, lo: f64, hi: f64, sign: f64) -> f64 {
    let g = |x: f64| sign * f(x);
    let rr = (5f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (lo, hi);
    let mut c = b - rr * (b - a);
    let mut d = a + rr * (b - a);
    let mut gc = g(c);
    let mut gd = g(d);
    let mut k = 0;
    while k < 80 && c < d {
        if gc <= gd {
            b = d;
            d = c;
            gd = gc;
            c = b - rr * (b - a);
            gc = g(c);
        } else {
            a = c;
            c = d;
            gc = gd;
            d = a + rr * (b - a);
            gd = g(d);
        }
        k += 1;
    }
    (a + b) / 2.0
}

/// Evenly across the range, and closing in on each end by halves.
fn hidden_bump(f: RealFn, a: f64, b: f64) -> bool {
    let n = 4000;
    let mut xs: Vec<f64> = (0..n - 1)
        .map(|i| a + ((b - a) * (i + 1) as f64) / n as f64)
        .collect();
    for k in 12..=40 {
        let p = jsmath::pow(2.0, -(k as f64));
        xs.push(a + (b - a) * p);
        xs.push(b - (b - a) * p);
    }
    xs.iter().any(|&x| {
        let y = f(x);
        y.is_finite() && y != 0.0
    })
}

/// ∫ f from a to b, with ±∞ bounds allowed.
pub fn integrate(f: RealFn, a: f64, b: f64) -> Option<Estimate> {
    if a.is_nan() || b.is_nan() {
        return None;
    }
    if a == b {
        return Some(Estimate {
            value: 0.0,
            err: 0.0,
        });
    }
    if a > b {
        let r = integrate(f, b, a)?;
        return Some(Estimate {
            value: -r.value,
            err: r.err,
        });
    }
    let mut budget = Budget {
        evals: MAX_EVALS,
        work: WorkBudget::new(MAX_WORK),
    };
    let est = if a == f64::NEG_INFINITY && b == f64::INFINITY {
        double_exponential(f, &sinh_sinh_rule(), &mut budget)
    } else if b == f64::INFINITY {
        double_exponential(f, &exp_sinh_rule(a), &mut budget)
    } else if a == f64::NEG_INFINITY {
        let g = |y: f64| f(b - y);
        double_exponential(&g, &exp_sinh_rule(0.0), &mut budget)
    } else {
        let mut ends = vec![a];
        ends.extend(peaks(f, a, b, &mut budget));
        ends.push(b);
        let mut est = Some(Sized {
            value: 0.0,
            err: 0.0,
            l1: 0.0,
        });
        let mut i = 1;
        while i < ends.len() {
            let Some(e) = est else { break };
            est = finite(f, ends[i - 1], ends[i], &mut budget).map(|p| Sized {
                value: e.value + p.value,
                err: e.err + p.err,
                l1: e.l1 + p.l1,
            });
            i += 1;
        }
        est
    }?;
    if !est.value.is_finite() {
        return None;
    }
    // an infinite range that only ever sampled zeros has no scan to check it
    if est.l1 == 0.0 && !(a.is_finite() && b.is_finite()) {
        return None;
    }
    let value = justified(est.value, est.err, Some(est.l1))?;
    // err covers the rounding too, so the shown value is within err of the truth
    Some(Estimate {
        value,
        err: est.err + (value - est.value).abs(),
    })
}

/// Neville extrapolation of g(h) to h = 0 over h, h/2, h/4, …, stopping once roundoff wins.
fn extrapolate(vals: &[f64]) -> Option<Estimate> {
    let mut rows: Vec<Vec<f64>> = Vec::new();
    let mut best: Option<Estimate> = None;
    for (i, &v) in vals.iter().enumerate() {
        let mut row = vec![v];
        if let Some(prev) = rows.last() {
            let mut fac = 2.0;
            let mut j = 1;
            while j <= prev.len().min(8) {
                let next = row[j - 1] + (row[j - 1] - prev[j - 1]) / (fac - 1.0);
                row.push(next);
                let err = (row[j] - row[j - 1])
                    .abs()
                    .max((row[j] - prev[j - 1]).abs());
                // JS `err < best.err` is false for NaN, as here
                if best.is_none_or(|b| err < b.err) {
                    best = Some(Estimate { value: row[j], err });
                }
                j += 1;
                fac *= 2.0;
            }
            if let Some(b) = best
                && (row[row.len() - 1] - prev[prev.len() - 1]).abs() > 4.0 * b.err
                && i > 8
            {
                break;
            }
        }
        rows.push(row);
    }
    best
}

/// The raw sequence settling on its own, for limits too ragged to extrapolate (x sin(1/x) at 0).
fn settles(vals: &[f64]) -> Option<Estimate> {
    if vals.len() < 24 {
        return None;
    }
    let tail = &vals[vals.len() - 12..];
    let last = tail[tail.len() - 1];
    let err = js_max(tail.iter().map(|v| (v - last).abs()));
    Some(Estimate {
        value: last,
        err: err * 2.0,
    })
}

/// `Math.max(...xs)`: NaN if any is NaN.
fn js_max(xs: impl Iterator<Item = f64>) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for x in xs {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}

fn js_min(xs: impl Iterator<Item = f64>) -> f64 {
    let mut m = f64::INFINITY;
    for x in xs {
        if x.is_nan() {
            return f64::NAN;
        }
        if x < m {
            m = x;
        }
    }
    m
}

/// Aitken's Δ² for errors like C·h^p with any p (√x at 0), trusted where six in a row agree.
fn aitken(v: &[f64]) -> Option<Estimate> {
    let mut acc = Vec::new();
    let mut k = 0;
    while k + 2 < v.len() {
        let den = v[k + 2] - 2.0 * v[k + 1] + v[k];
        acc.push(if den == 0.0 {
            v[k + 2]
        } else {
            v[k] - jsmath::pow(v[k + 1] - v[k], 2.0) / den
        });
        k += 1;
    }
    let mut best: Option<Estimate> = None;
    let mut k = 0;
    while k + 6 <= acc.len() {
        let w = &acc[k..k + 6];
        let spread = js_max(w.iter().copied()) - js_min(w.iter().copied());
        if best.is_none_or(|b| spread * 2.0 < b.err) {
            best = Some(Estimate {
                value: w[5],
                err: spread * 2.0,
            });
        }
        k += 1;
    }
    best
}

/// The samples before roundoff takes over.
fn clean_prefix(vals: &[f64]) -> &[f64] {
    let mut start = 0;
    while start < vals.len() && !vals[start].is_finite() {
        start += 1;
    }
    let mut min_step = f64::INFINITY;
    for k in start + 1..vals.len() {
        if !vals[k].is_finite() {
            return &vals[start..k];
        }
        let step = (vals[k] - vals[k - 1]).abs();
        if min_step < f64::INFINITY && step > 100.0 * min_step {
            return &vals[start..k];
        }
        min_step = min_step.min(step);
    }
    &vals[start.min(vals.len())..]
}

fn js_sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// ±∞ when |g| grows steadily from the start, like 1/x at 0.
fn diverges(vals: &[f64]) -> Option<f64> {
    let finite = &vals[1.min(vals.len())..];
    let cut = finite.iter().position(|v| !v.is_finite());
    let (head, rest) = match cut {
        None => (finite, &finite[finite.len()..]),
        Some(c) => (&finite[..c], &finite[c..]),
    };
    if head.len() < 5 {
        return None;
    }
    let sign = js_sign(head[0]);
    // `!sign` is true for 0 and NaN
    if sign == 0.0 || sign.is_nan() || head.iter().any(|&v| js_sign(v) != sign) {
        return None;
    }
    for i in 1..head.len() {
        let grew = head[i].abs() - head[i - 1].abs();
        if grew.is_nan() || grew <= 0.0 {
            return None;
        }
        if i > 1 && grew < 0.9 * (head[i - 1].abs() - head[i - 2].abs()) {
            return None;
        }
    }
    let last = head[head.len() - 1].abs();
    // an overflow or tan at its pole is undefined, so a blowup can end in NaN
    if rest
        .iter()
        .any(|&v| v != sign * f64::INFINITY && !v.is_nan())
    {
        return None;
    }
    if rest.iter().any(|v| v.is_nan()) && last < 1e6 {
        return None;
    }
    if !rest.is_empty() || last > 1e6 {
        Some(sign * f64::INFINITY)
    } else {
        None
    }
}

/// Every sample further from `value` than the one before.
fn drifts_away(vals: &[f64], value: f64) -> bool {
    let dev: Vec<f64> = vals
        .iter()
        .filter(|v| v.is_finite())
        .map(|v| (v - value).abs())
        .collect();
    dev.len() >= 16 && (1..dev.len()).all(|i| dev[i] > dev[i - 1])
}

const LIMIT_STEPS: usize = 44;

fn one_sided(f: RealFn, to: f64, side: f64) -> Option<Estimate> {
    let fin = to.is_finite();
    let g = |h: f64| {
        if fin {
            f(to + side * h)
        } else {
            f(js_sign(to) / h)
        }
    };
    let mut h = if fin {
        0.125 * to.abs().max(1.0)
    } else {
        0.125
    };
    let mut vals = Vec::with_capacity(LIMIT_STEPS);
    for _ in 0..LIMIT_STEPS {
        vals.push(g(h));
        h /= 2.0;
    }
    let clean = clean_prefix(&vals);
    // richardson fits whole powers of h, aitken any single power; the tighter bound wins
    let mut best: Option<Estimate> = None;
    if clean.len() >= 8 {
        for t in [extrapolate(clean), settles(clean), aitken(clean)]
            .into_iter()
            .flatten()
        {
            if t.value.is_finite() && best.is_none_or(|b| t.err < b.err) {
                best = Some(t);
            }
        }
    }
    // extrapolating 1/h gives a confident 0 (an anti-limit); the samples must already be close
    if let Some(b) = best {
        let last = clean.last().copied().unwrap_or(f64::NAN);
        let near = (last - b.value).abs() <= 1e-2 * b.value.abs().max(1.0);
        if near && b.err <= 1e-7 * b.value.abs().max(1.0) && !drifts_away(&vals, b.value) {
            return Some(b);
        }
    }
    diverges(&vals).map(|inf| Estimate {
        value: inf,
        err: 0.0,
    })
}

/// side 0 is two-sided; both sides must agree or there's no limit.
pub fn limit(f: RealFn, to: f64, side: i32) -> Option<Estimate> {
    if to.is_nan() {
        return None;
    }
    let side = if to.is_finite() {
        side
    } else if to > 0.0 {
        -1
    } else {
        1
    };
    let est = if side != 0 {
        one_sided(f, to, f64::from(side))?
    } else {
        let right = one_sided(f, to, 1.0)?;
        let left = one_sided(f, to, -1.0)?;
        if !right.value.is_finite() || !left.value.is_finite() {
            return (right.value == left.value).then_some(right);
        }
        let err = right.err.max(left.err);
        if (right.value - left.value).abs() > 2.0 * err + 1e-9 * right.value.abs().max(1.0) {
            return None;
        }
        Estimate {
            value: (right.value + left.value) / 2.0,
            err: err + (right.value - left.value).abs(),
        }
    };
    if !est.value.is_finite() {
        return Some(est);
    }
    let value = justified(est.value, est.err, Some(est.value.abs().max(1.0)))?;
    Some(Estimate {
        value,
        err: est.err + (value - est.value).abs(),
    })
}

/// Ridders' method: central differences extrapolated as the step shrinks.
pub fn derivative_at(f: RealFn, x: f64) -> Option<Estimate> {
    const CON: f64 = 1.4;
    const CON2: f64 = CON * CON;
    let mut table: Vec<Vec<f64>> = Vec::new();
    let mut h = 0.1 * x.abs().max(1.0);
    let mut best: Option<Estimate> = None;
    for i in 0..14 {
        let d = (f(x + h) - f(x - h)) / (2.0 * h);
        if !d.is_finite() {
            return None;
        }
        table.push(vec![d]);
        let mut fac = CON2;
        for j in 1..=i {
            let prev = table[i][j - 1];
            let up = table[i - 1][j - 1];
            let v = (prev * fac - up) / (fac - 1.0);
            table[i].push(v);
            let err = (v - prev).abs().max((v - up).abs());
            if best.is_none_or(|b| err <= b.err) {
                best = Some(Estimate { value: v, err });
            }
            fac *= CON2;
        }
        if i > 0
            && let Some(b) = best
            && (table[i][i] - table[i - 1][i - 1]).abs() >= 2.0 * b.err
        {
            break;
        }
        h /= CON;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrates_and_differentiates() {
        let sq = |x: f64| x * x;
        assert_eq!(
            integrate(&sq, 0.0, 1.0).map(|e| e.value),
            Some(0.3333333333333)
        );
        let e = |x: f64| jsmath::exp(-x);
        assert_eq!(
            integrate(&e, 0.0, f64::INFINITY).map(|e| e.value),
            Some(1.0)
        );
        let d = derivative_at(&jsmath::sin, 1.0).unwrap();
        assert!((d.value - jsmath::cos(1.0)).abs() < 1e-12);
        let s = |x: f64| jsmath::sin(x) / x;
        assert_eq!(limit(&s, 0.0, 0).map(|e| e.value), Some(1.0));
        assert_eq!(justified(2.0000000003, 1e-9, None), Some(2.0));
    }
}
