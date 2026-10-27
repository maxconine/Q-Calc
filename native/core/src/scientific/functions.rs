//! The rest of `scalarScope` in `src/engine/scientific.ts` (trig in both angle modes with `sinCos` snapping,
//! inverse trig, hyperbolics, logs, rounding, statistics, gcd/lcm, nCr/nPr, random, ranges, zeta, powmod) and the
//! mathjs functions the engine reaches outside its scope (`sum`, `std`, `hypot`, `re`/`im`/`conj`/`arg`, `range`…).
//!
//! Each scope entry's `max_args` is the JS function's `length` (0 for variadic), which `unitSafe` enforces; a
//! mathjs function's is its longest signature.
//!
//! The scope functions are plain JS functions, so they see their arguments the JS way: arithmetic and comparisons
//! coerce with `Number(x)` ([`Val::to_number`]), while `Number.isInteger`, `Number.isFinite` and `===` don't
//! coerce at all. Where the TS uses the strict forms, so does this port (`round(true)` is `true`, `csch(false)`
//! is 1/0).

pub mod jsmath;

use std::f64::consts::PI;

use crate::eval::builtins::{Builtin, as_ratio, near_integer};
use crate::eval::complex::C;
use crate::eval::{EvalError, EvalResult, Scope, Val, ops};
use crate::format::int_gcd;
use crate::jsnum;
use crate::scientific::MAX_LIST_ALLOC;
use crate::syntax::Op;
use crate::types::AngleMode;

// ---- argument helpers ----

/// `Number(args[i])`; a missing argument is `undefined`, so NaN.
fn n(args: &[Val], i: usize) -> f64 {
    args.get(i).map_or(f64::NAN, Val::to_number)
}

fn num(x: f64) -> EvalResult {
    Ok(Val::Num(x))
}

/// `x === 0`: only a number is.
fn strict_zero(args: &[Val], i: usize) -> bool {
    matches!(args.get(i), Some(Val::Num(x)) if *x == 0.0)
}

/// `Number.isInteger(x)`: only a number can be.
fn strict_integer(v: Option<&Val>) -> bool {
    matches!(v, Some(Val::Num(x)) if jsnum::is_integer(*x))
}

/// `nums` in the TS: arrays and matrices flattened, every entry through `Number`.
pub fn nums(args: &[Val]) -> Vec<f64> {
    fn walk(v: &Val, out: &mut Vec<f64>) {
        match v {
            Val::List(items) => items.iter().for_each(|i| walk(i, out)),
            other => out.push(other.to_number()),
        }
    }
    let mut out = Vec::new();
    args.iter().for_each(|a| walk(a, &mut out));
    out
}

fn list(values: Vec<f64>) -> Val {
    Val::List(values.into_iter().map(Val::Num).collect())
}

fn alloc_check(count: f64) -> Result<(), EvalError> {
    if !count.is_finite() || count < 0.0 || count > MAX_LIST_ALLOC as f64 {
        return Err(EvalError::Other("list allocation exceeds limit".into()));
    }
    Ok(())
}

// ---- JS Math over doubles ----

/// `Math.min(...xs)`: NaN wins, -0 is below 0, and no arguments is +∞.
fn js_min(xs: &[f64]) -> f64 {
    let mut m = f64::INFINITY;
    for &x in xs {
        if x.is_nan() {
            return f64::NAN;
        }
        if x < m || (x == 0.0 && m == 0.0 && x.is_sign_negative()) {
            m = x;
        }
    }
    m
}

/// `Math.max(...xs)`: NaN wins, 0 is above -0, and no arguments is -∞.
fn js_max(xs: &[f64]) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for &x in xs {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m || (x == 0.0 && m == 0.0 && m.is_sign_negative()) {
            m = x;
        }
    }
    m
}

/// `Math.sign`.
fn js_sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `[...xs].sort((a, b) => a - b)`: a NaN difference counts as equal, and the sort is stable, as V8's is.
fn sorted(xs: &[f64]) -> Vec<f64> {
    let mut s = xs.to_vec();
    s.sort_by(|a, b| {
        let d = a - b;
        if d > 0.0 {
            std::cmp::Ordering::Greater
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    });
    s
}

// ---- statistics (the TS helpers, line for line) ----

fn avg(xs: &[f64]) -> f64 {
    xs.iter().fold(0.0, |s, x| s + x) / xs.len() as f64
}

fn variance_of(xs: &[f64], population: bool) -> f64 {
    let m = avg(xs);
    let d = if population {
        xs.len() as f64
    } else {
        xs.len() as f64 - 1.0
    };
    if d <= 0.0 {
        return f64::NAN;
    }
    xs.iter().fold(0.0, |s, x| s + (x - m) * (x - m)) / d
}

fn median_of(s: &[f64]) -> f64 {
    if s.is_empty() || s.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let m = (s.len() - 1) as f64 / 2.0;
    (s[m.floor() as usize] + s[m.ceil() as usize]) / 2.0
}

fn tukey_quartile(data: &[f64], q: f64) -> f64 {
    if ![0.0, 1.0, 2.0, 3.0, 4.0].contains(&q) || data.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let s = sorted(data);
    if q == 0.0 || q == 4.0 {
        return if s.is_empty() {
            f64::NAN
        } else if q == 0.0 {
            s[0]
        } else {
            s[s.len() - 1]
        };
    }
    if q == 2.0 {
        return median_of(&s);
    }
    let len = s.len();
    let (lower, upper) = if len.is_multiple_of(2) {
        (&s[..len / 2], &s[len / 2..])
    } else {
        (&s[..(len - 1) / 2], &s[len.div_ceil(2)..])
    };
    if q <= 1.0 {
        median_of(lower)
    } else {
        median_of(upper)
    }
}

fn quantile_linear(data: &[f64], p: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) || data.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let s = sorted(data);
    if s.is_empty() {
        return f64::NAN;
    }
    let idx = (s.len() - 1) as f64 * p;
    let (lo, hi) = (idx.floor(), idx.ceil());
    if lo == hi {
        return s[lo as usize];
    }
    s[lo as usize] * (hi - idx) + s[hi as usize] * (idx - lo)
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    if len < 2 {
        return f64::NAN;
    }
    let ma = avg(&a[..len]);
    let mb = avg(&b[..len]);
    let (mut top, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..len {
        let xa = a[i] - ma;
        let xb = b[i] - mb;
        top += xa * xb;
        da += xa * xa;
        db += xb * xb;
    }
    let den = (da * db).sqrt();
    if den == 0.0 { f64::NAN } else { top / den }
}

fn int_lcm(a: f64, b: f64) -> f64 {
    if a == 0.0 || b == 0.0 {
        return 0.0;
    }
    (a * b).abs() / int_gcd(a, b)
}

/// gcd(a/b, c/d) = gcd(a, c) / lcm(b, d) and lcm(a/b, c/d) = lcm(a, c) / gcd(b, d); non-fractions give NaN.
fn ratio_fold(args: &[Val], gcd: bool) -> f64 {
    let ratios: Option<Vec<(f64, f64)>> = nums(args).into_iter().map(as_ratio).collect();
    let Some(ratios) = ratios.filter(|r| !r.is_empty()) else {
        return f64::NAN;
    };
    let (n, d) = ratios
        .into_iter()
        .reduce(|(n1, d1), (n2, d2)| {
            if gcd {
                (int_gcd(n1, n2), int_lcm(d1, d2))
            } else {
                (int_lcm(n1, n2), int_gcd(d1, d2))
            }
        })
        .unwrap_or((f64::NAN, 1.0));
    n / d
}

// ---- rounding ----

/// `String(v).split('e')` then `Number(`${m}e${e + k}`)`: a decimal shift on the digits as written.
fn shift(v: f64, k: f64) -> f64 {
    let s = jsnum::to_string(v);
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let e = jsnum::parse(e).unwrap_or(f64::NAN);
    js_number(&format!("{m}e{}", jsnum::to_string(e + k)))
}

/// `Number(text)` for the strings `shift` builds (`Infinitye5` and `1eNaN` are NaN).
fn js_number(text: &str) -> f64 {
    let t = text.trim();
    if t.is_empty() {
        return 0.0;
    }
    if t.contains("Infinity") || t.contains("NaN") {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// Half away from zero, on the decimal digits as written: `round(1.005, 2)` is 1.01 and `round(-2.5)` is -3.
/// Anything but a finite number comes back as it went in (`!Number.isFinite(x)`), complex and lists included.
fn round(args: &[Val], _: &Scope) -> EvalResult {
    let digits = match args.get(1) {
        None | Some(Val::Undefined) => 0.0,
        Some(Val::Num(d)) if jsnum::is_integer(*d) => *d,
        Some(_) => return num(f64::NAN),
    };
    let x = match args.first() {
        Some(Val::Num(x)) if x.is_finite() => *x,
        Some(other) => return Ok(other.clone()),
        None => return Ok(Val::Undefined),
    };
    num(js_sign(x) * shift(jsnum::round(shift(x.abs(), digits)), -digits))
}

// ---- angles ----

fn rad(scope: &Scope) -> bool {
    scope.angle_mode == AngleMode::Rad
}

fn to_rad(x: f64, scope: &Scope) -> f64 {
    if rad(scope) { x } else { (x * PI) / 180.0 }
}

fn from_rad(x: f64, scope: &Scope) -> f64 {
    if rad(scope) { x } else { (x * 180.0) / PI }
}

/// sin and cos at a multiple of π/2 miss 0 by float noise that grows with the angle; a tiny angle's tiny sine
/// is real.
pub fn sin_cos(x: f64, scope: &Scope) -> (f64, f64) {
    let r = to_rad(x, scope);
    let noise = 1e-14 * r.abs();
    let snap = |v: f64| if v.abs() <= noise { 0.0 } else { v };
    (snap(jsmath::sin(r)), snap(jsmath::cos(r)))
}

fn sin(a: &[Val], s: &Scope) -> EvalResult {
    num(sin_cos(n(a, 0), s).0)
}
fn cos(a: &[Val], s: &Scope) -> EvalResult {
    num(sin_cos(n(a, 0), s).1)
}
fn tan(a: &[Val], s: &Scope) -> EvalResult {
    let (sn, c) = sin_cos(n(a, 0), s);
    num(if c == 0.0 { f64::NAN } else { sn / c })
}
fn csc(a: &[Val], s: &Scope) -> EvalResult {
    let (sn, _) = sin_cos(n(a, 0), s);
    num(if sn == 0.0 { f64::NAN } else { 1.0 / sn })
}
fn sec(a: &[Val], s: &Scope) -> EvalResult {
    let (_, c) = sin_cos(n(a, 0), s);
    num(if c == 0.0 { f64::NAN } else { 1.0 / c })
}
fn cot(a: &[Val], s: &Scope) -> EvalResult {
    let (sn, c) = sin_cos(n(a, 0), s);
    num(if sn == 0.0 { f64::NAN } else { c / sn })
}

fn asin(a: &[Val], s: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if !(-1.0..=1.0).contains(&x) && !x.is_nan() {
        f64::NAN
    } else {
        from_rad(jsmath::asin(x), s)
    })
}
fn acos(a: &[Val], s: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if !(-1.0..=1.0).contains(&x) && !x.is_nan() {
        f64::NAN
    } else {
        from_rad(jsmath::acos(x), s)
    })
}
fn atan(a: &[Val], s: &Scope) -> EvalResult {
    num(from_rad(jsmath::atan(n(a, 0)), s))
}
fn atan2(a: &[Val], s: &Scope) -> EvalResult {
    num(from_rad(jsmath::atan2(n(a, 0), n(a, 1)), s))
}
fn acsc(a: &[Val], s: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if x.abs() < 1.0 {
        f64::NAN
    } else {
        from_rad(jsmath::asin(1.0 / x), s)
    })
}
fn asec(a: &[Val], s: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if x.abs() < 1.0 {
        f64::NAN
    } else {
        from_rad(jsmath::acos(1.0 / x), s)
    })
}
fn acot(a: &[Val], s: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(from_rad(
        if strict_zero(a, 0) {
            PI / 2.0
        } else {
            jsmath::atan(1.0 / x)
        },
        s,
    ))
}

// ---- hyperbolics ----

fn csch(a: &[Val], _: &Scope) -> EvalResult {
    num(if strict_zero(a, 0) {
        f64::NAN
    } else {
        1.0 / jsmath::sinh(n(a, 0))
    })
}
fn coth(a: &[Val], _: &Scope) -> EvalResult {
    num(if strict_zero(a, 0) {
        f64::NAN
    } else {
        1.0 / jsmath::tanh(n(a, 0))
    })
}
fn acosh(a: &[Val], _: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if x < 1.0 { f64::NAN } else { jsmath::acosh(x) })
}
fn atanh(a: &[Val], _: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if x <= -1.0 || x >= 1.0 {
        f64::NAN
    } else {
        jsmath::atanh(x)
    })
}
fn acsch(a: &[Val], _: &Scope) -> EvalResult {
    num(if strict_zero(a, 0) {
        f64::NAN
    } else {
        jsmath::asinh(1.0 / n(a, 0))
    })
}
fn asech(a: &[Val], _: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if x <= 0.0 || x > 1.0 {
        f64::NAN
    } else {
        jsmath::acosh(1.0 / x)
    })
}
fn acoth(a: &[Val], _: &Scope) -> EvalResult {
    let x = n(a, 0);
    num(if x.abs() <= 1.0 {
        f64::NAN
    } else {
        jsmath::atanh(1.0 / x)
    })
}

// ---- logs and powers ----

fn positive_log(x: f64, f: fn(f64) -> f64) -> EvalResult {
    num(if x <= 0.0 { f64::NAN } else { f(x) })
}

/// `log(x)` with one argument is mathjs's (complex for a negative); with a base it's the engine's.
/// (One-argument `log(...)` is rewritten to `log10` before evaluation, so the first form is rare.)
fn log(a: &[Val], _: &Scope) -> EvalResult {
    match a.get(1) {
        None | Some(Val::Null | Val::Undefined) => match a.first() {
            Some(Val::Complex { re, im }) => Ok(C::new(*re, *im).log().val()),
            Some(Val::Num(_) | Val::Bool(_)) => {
                let x = n(a, 0);
                Ok(if x >= 0.0 {
                    Val::Num(jsmath::ln(x))
                } else {
                    C::new(x, 0.0).log().val()
                })
            }
            _ => Err(EvalError::Other(
                "Unexpected type of argument in function log".into(),
            )),
        },
        Some(base_val) => {
            let (x, base) = (n(a, 0), base_val.to_number());
            let one = matches!(base_val, Val::Num(b) if *b == 1.0);
            num(if x <= 0.0 || base <= 0.0 || one {
                f64::NAN
            } else {
                jsmath::ln(x) / jsmath::ln(base)
            })
        }
    }
}

/// mathjs `nthRoot(a, root = 2)`, elementwise over a matrix on either side (`nthroot(180, [5])` is `[2.8…]`).
fn nth_root(a: &[Val], _: &Scope) -> EvalResult {
    nth_root_each(a)
}

fn nth_root_each(a: &[Val]) -> EvalResult {
    let each = |items: &[Val], f: &dyn Fn(&Val) -> EvalResult| -> EvalResult {
        Ok(Val::List(items.iter().map(f).collect::<Result<_, _>>()?))
    };
    match a {
        [Val::List(xs)] => each(xs, &|x| nth_root_each(std::slice::from_ref(x))),
        [Val::List(xs), Val::List(rs)] => {
            if xs.len() != rs.len() {
                return Err(EvalError::Other("Dimension mismatch in nthRoot".into()));
            }
            Ok(Val::List(
                xs.iter()
                    .zip(rs)
                    .map(|(x, r)| nth_root_each(&[x.clone(), r.clone()]))
                    .collect::<Result<_, _>>()?,
            ))
        }
        [Val::List(xs), r] => each(xs, &|x| nth_root_each(&[x.clone(), r.clone()])),
        [x, Val::List(rs)] => each(rs, &|r| nth_root_each(&[x.clone(), r.clone()])),
        _ => nth_root_scalar(a),
    }
}

fn nth_root_scalar(a: &[Val]) -> EvalResult {
    let typed = |v: &Val| match v {
        Val::Num(_) | Val::Bool(_) => Ok(v.to_number()),
        _ => Err(EvalError::Other(
            "Unexpected type of argument in function nthRoot".into(),
        )),
    };
    let x = typed(
        a.first()
            .ok_or_else(|| EvalError::Other("Too few arguments".into()))?,
    )?;
    let mut root = match a.get(1) {
        Some(r) => typed(r)?,
        None => 2.0,
    };
    let inv = root < 0.0;
    if inv {
        root = -root;
    }
    if root == 0.0 {
        return Err(EvalError::Other("Root must be non-zero".into()));
    }
    if x < 0.0 && root.abs() % 2.0 != 1.0 {
        return Err(EvalError::Other(
            "Root must be odd when a is negative.".into(),
        ));
    }
    if x == 0.0 {
        return num(if inv { f64::INFINITY } else { 0.0 });
    }
    if !x.is_finite() {
        return num(if inv { 0.0 } else { x });
    }
    let r = ops::js_pow(x.abs(), 1.0 / root);
    let r = if x < 0.0 { -r } else { r };
    num(if inv { 1.0 / r } else { r })
}

/// `b^e mod m` in exact integers.
fn powmod(a: &[Val], _: &Scope) -> EvalResult {
    let mut v = [0i128; 3];
    for (i, slot) in v.iter_mut().enumerate() {
        match a.get(i) {
            Some(Val::Num(x)) if jsnum::is_safe_integer(*x) => *slot = *x as i128,
            _ => return Err(EvalError::Other("inexact mod".into())),
        }
    }
    let [b, e, m] = v;
    if m == 0 {
        return num(f64::NAN);
    }
    let modulus = m.abs();
    // BigInt `%` keeps the dividend's sign, as Rust's does
    let mut base = b % modulus;
    let mut out = 1 % modulus;
    let mut k = e;
    while k > 0 {
        if k & 1 == 1 {
            out = (out * base) % modulus;
        }
        base = (base * base) % modulus;
        k >>= 1;
    }
    num(out as f64)
}

// ---- mathjs's zeta, on doubles ----

fn nearly_equal(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return false;
    }
    if !a.is_finite() || !b.is_finite() {
        return a == b;
    }
    a == b || (a - b).abs() <= (1e-12 * a.abs().max(b.abs())).max(1e-15)
}

fn smaller_eq(a: f64, b: f64) -> bool {
    a <= b || nearly_equal(a, b)
}

/// The scope's `factorial`: a number (or `true`, `null`, nothing) goes through the TS function; a complex or a
/// list reaches `math.gamma(n + 1)` as the string JS's `+` makes of it, which mathjs rejects.
pub fn factorial_call(a: &[Val], _: &Scope) -> EvalResult {
    match a.first() {
        Some(Val::Complex { .. } | Val::List(_) | Val::Function(_)) => Err(type_error("gamma")),
        _ => num(crate::eval::builtins::factorial(n(a, 0))),
    }
}

/// mathjs `factorial` of a number: gamma(n + 1).
fn math_factorial(x: f64) -> f64 {
    crate::eval::builtins::gamma(x + 1.0)
}

fn zeta_d(k: f64, n: f64) -> f64 {
    let mut s = k;
    let mut j = k;
    while smaller_eq(j, n) {
        let factor = (math_factorial(n + (j - 1.0)) * ops::real_pow(4.0, j))
            / (math_factorial(n - j) * math_factorial(2.0 * j));
        s += factor;
        j += 1.0;
    }
    n * s
}

fn zeta_f(s: f64, n: f64) -> f64 {
    let c = 1.0 / (zeta_d(0.0, n) * (1.0 - ops::real_pow(2.0, 1.0 - s)));
    let mut sum = 0.0;
    let mut k = 1.0;
    while smaller_eq(k, n) {
        let sign = ops::js_pow(-1.0, k - 1.0);
        sum += (sign * zeta_d(k, n)) / ops::real_pow(k, s);
        k += 1.0;
    }
    c * sum
}

fn zeta_series(s: f64) -> f64 {
    const DIGITS: f64 = 20.0;
    if s > -(DIGITS - 1.0) / 2.0 {
        return zeta_f(s, DIGITS);
    }
    // the functional equation reflects s below the series' reach
    let mut c = ops::real_pow(2.0, s) * ops::real_pow(PI, s - 1.0);
    c *= jsmath::sin((PI / 2.0) * s);
    c *= crate::eval::builtins::gamma(1.0 - s);
    c * zeta_series(1.0 - s)
}

/// mathjs `zeta` of a number.
pub fn zeta_number(s: f64) -> f64 {
    if nearly_equal(s, 0.0) {
        return -0.5;
    }
    if nearly_equal(s, 1.0) {
        return f64::NAN;
    }
    if !s.is_finite() {
        // mathjs: `isNegative(NaN)` is false, so zeta(NaN) is 1
        return if s < 0.0 { f64::NAN } else { 1.0 };
    }
    zeta_series(s)
}

fn zeta(a: &[Val], _: &Scope) -> EvalResult {
    match a.first() {
        Some(Val::Num(_) | Val::Bool(_)) => num(zeta_number(n(a, 0))),
        // a complex argument with no imaginary part is the real zeta; any other comes out complex, so NaN
        Some(Val::Complex { re, im }) => num(if *im == 0.0 {
            zeta_number(*re)
        } else {
            f64::NAN
        }),
        _ => Err(EvalError::Other(
            "Unexpected type of argument in function zeta".into(),
        )),
    }
}

// ---- combinatorics ----

/// mathjs `product(i, n)`: split in halves.
fn product(i: f64, n: f64) -> f64 {
    if n < i {
        return 1.0;
    }
    if n == i {
        return n;
    }
    let half = ((n + i) as i64 >> 1) as f64;
    product(i, half) * product(half + 1.0, n)
}

/// mathjs `combinationsNumber`, which balances multiplying and dividing to stay exact as long as it can.
fn combinations_number(n: f64, k: f64) -> f64 {
    let n_minus_k = n - k;
    let mut answer = 1.0;
    let first = if k < n_minus_k {
        n_minus_k + 1.0
    } else {
        k + 1.0
    };
    let mut divisor = 2.0;
    let last = if k < n_minus_k { k } else { n_minus_k };
    let mut next = first;
    while next <= n {
        answer *= next;
        while divisor <= last && answer % divisor == 0.0 {
            answer /= divisor;
            divisor += 1.0;
        }
        next += 1.0;
    }
    if divisor <= last {
        answer /= product(divisor, last);
    }
    answer
}

/// The TS `combinationsFloat`: mathjs's own nCr overflows inside (NaN for nCr(1000, 500)) even when the answer
/// fits a float.
fn combinations_float(n: f64, k: f64) -> f64 {
    let m = k.min(n - k);
    if m > 1100.0 {
        return f64::INFINITY;
    }
    let mut r = 1.0;
    let mut i = 1.0;
    while i <= m {
        r = (r * (n - m + i)) / i;
        i += 1.0;
    }
    r
}

/// `choose` in the TS: 0 when choosing more than there are, undefined for non-integers or negatives.
fn choose(a: &[Val], f: fn(f64, f64) -> f64) -> EvalResult {
    if !strict_integer(a.first()) || !strict_integer(a.get(1)) {
        return num(f64::NAN);
    }
    let (n, k) = (n(a, 0), n(a, 1));
    if n < 0.0 || k < 0.0 {
        return num(f64::NAN);
    }
    num(if k > n { 0.0 } else { f(n, k) })
}

fn combinations(a: &[Val], _: &Scope) -> EvalResult {
    choose(a, |n, k| {
        // past 1100 the balanced loop ends at ∞ (or ∞/∞) and the fallback says ∞, without a long loop first
        if k.min(n - k) > 1100.0 {
            return f64::INFINITY;
        }
        let v = combinations_number(n, k);
        if v.is_nan() {
            combinations_float(n, k)
        } else {
            v
        }
    })
}

fn permutations(a: &[Val], _: &Scope) -> EvalResult {
    choose(a, |n, k| {
        // k factors of at least 1, and more than 170 of them is past any double
        if k > 170.0 && n - k + 1.0 >= 1.0 {
            return f64::INFINITY;
        }
        product(n - k + 1.0, n)
    })
}

// ---- lists ----

fn inclusive_range(a: &[Val], _: &Scope) -> EvalResult {
    let (start, end) = (n(a, 0), n(a, 1));
    if !start.is_finite() || !end.is_finite() {
        return Err(EvalError::Other("invalid range".into()));
    }
    let step = if start <= end { 1.0 } else { -1.0 };
    let count = (end - start).abs().floor() + 1.0;
    alloc_check(count)?;
    let mut out = Vec::new();
    let mut i = start;
    while if step > 0.0 { i <= end } else { i >= end } {
        out.push(i);
        i += step;
    }
    Ok(list(out))
}

/// A small generator for `random` and `randint`, seeded from the process's hash keys. Their answers are
/// random in the TS too (the fixtures skip them).
fn random_unit() -> f64 {
    use std::cell::Cell;
    use std::hash::{BuildHasher, Hasher};
    thread_local! {
        static STATE: Cell<u64> = Cell::new({
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(0x9e37_79b9_7f4a_7c15);
            h.finish() | 1
        });
    }
    STATE.with(|s| {
        // xorshift64*
        let mut x = s.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn random(a: &[Val], _: &Scope) -> EvalResult {
    let xs = nums(a);
    match xs.as_slice() {
        [] => num(random_unit()),
        [count] if *count >= 2.0 && jsnum::is_integer(*count) => {
            alloc_check(*count)?;
            Ok(list((0..*count as usize).map(|_| random_unit()).collect()))
        }
        [lo, hi, ..] => num(lo + random_unit() * (hi - lo)),
        [x] => num(random_unit() * x),
    }
}

fn randint(a: &[Val], _: &Scope) -> EvalResult {
    let xs = nums(a);
    if !xs.iter().all(|x| jsnum::is_integer(*x)) {
        return num(f64::NAN);
    }
    let a0 = xs.first().copied().unwrap_or(0.0);
    let a1 = xs.get(1).copied().unwrap_or(a0);
    let (lo, hi) = (a0.min(a1), a0.max(a1));
    let one = || lo + (random_unit() * (hi - lo + 1.0)).floor();
    match xs.get(2) {
        None => num(one()),
        Some(&count) => {
            alloc_check(count)?;
            Ok(list((0..count as usize).map(|_| one()).collect()))
        }
    }
}

// ---- the statistics functions ----

fn min(a: &[Val], _: &Scope) -> EvalResult {
    num(js_min(&nums(a)))
}
fn max(a: &[Val], _: &Scope) -> EvalResult {
    num(js_max(&nums(a)))
}
fn mean(a: &[Val], _: &Scope) -> EvalResult {
    num(avg(&nums(a)))
}
fn median(a: &[Val], _: &Scope) -> EvalResult {
    num(median_of(&sorted(&nums(a))))
}
fn mad(a: &[Val], _: &Scope) -> EvalResult {
    let xs = nums(a);
    let m = avg(&xs);
    num(xs.iter().fold(0.0, |p, c| p + (c - m).abs()) / xs.len() as f64)
}
fn quartile(a: &[Val], _: &Scope) -> EvalResult {
    let mut xs = nums(a);
    let q = xs.pop().unwrap_or(2.0);
    num(tukey_quartile(&xs, q))
}
fn quantile(a: &[Val], _: &Scope) -> EvalResult {
    let mut xs = nums(a);
    let p = xs.pop().unwrap_or(0.5);
    num(quantile_linear(&xs, p))
}
fn corr(a: &[Val], _: &Scope) -> EvalResult {
    if a.len() == 2 {
        let (xs, ys) = (nums(&a[..1]), nums(&a[1..]));
        return num(if xs.len() == ys.len() {
            pearson(&xs, &ys)
        } else {
            f64::NAN
        });
    }
    let xs = nums(a);
    num(if xs.len().is_multiple_of(2) {
        let mid = xs.len() / 2;
        pearson(&xs[..mid], &xs[mid..])
    } else {
        f64::NAN
    })
}

fn clamp(a: &[Val], _: &Scope) -> EvalResult {
    let (v, lo, hi) = (n(a, 0), n(a, 1), n(a, 2));
    if !(v.is_finite() && lo.is_finite() && hi.is_finite()) {
        return num(f64::NAN);
    }
    let (lo, hi) = (js_min(&[lo, hi]), js_max(&[lo, hi]));
    num(js_min(&[js_max(&[v, lo]), hi]))
}

// ---- mathjs functions outside the engine's scope ----

fn type_error(name: &str) -> EvalError {
    EvalError::Other(format!("Unexpected type of argument in function {name}"))
}

/// Every entry of a list, depth first (mathjs `deepForEach`).
fn deep(values: &[Val]) -> Vec<Val> {
    fn walk(v: &Val, out: &mut Vec<Val>) {
        match v {
            Val::List(items) => items.iter().for_each(|i| walk(i, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    values.iter().for_each(|v| walk(v, &mut out));
    out
}

/// What a mathjs statistic walks: one list's every entry (`std([1, 2, 3])`), or the arguments themselves
/// (`std(1, 2, 3)`), where a list among them is a single matrix value that the arithmetic meets whole, as
/// `deepForEach` over an Array of Matrices does.
fn collection(a: &[Val]) -> Vec<Val> {
    match a {
        [Val::List(_)] => deep(a),
        _ => a.to_vec(),
    }
}

fn add_all(values: &[Val]) -> EvalResult {
    let mut sum: Option<Val> = None;
    for v in values {
        sum = Some(match sum {
            None => v.clone(),
            Some(s) => ops::binary(Op::Add, &s, v)?,
        });
    }
    Ok(sum.unwrap_or(Val::Num(0.0)))
}

/// mathjs `sum`: one list, or scalars only.
fn sum(a: &[Val], _: &Scope) -> EvalResult {
    match a {
        [Val::List(_)] => add_all(&deep(a)),
        [Val::List(_), _] => Err(EvalError::Unsupported("sum along a dimension".into())),
        _ if a.iter().any(|v| matches!(v, Val::List(_))) => Err(EvalError::Other(
            "Scalar values expected in function sum".into(),
        )),
        _ => add_all(a),
    }
}

/// mathjs `prod`.
fn prod(a: &[Val], _: &Scope) -> EvalResult {
    if matches!(a, [Val::List(_), Val::Num(_) | Val::Bool(_)]) {
        return Err(EvalError::Other("prod(A, dim) is not yet supported".into()));
    }
    let values = collection(a);
    if values.iter().any(|v| matches!(v, Val::List(_))) {
        // `multiplyScalar` of a matrix
        return Err(type_error("multiplyScalar"));
    }
    if values.is_empty() {
        return Err(EvalError::Other(
            "Cannot calculate prod of an empty array".into(),
        ));
    }
    let mut out = values[0].clone();
    for v in &values[1..] {
        out = ops::binary(Op::Multiply, &out, v)?;
    }
    Ok(out)
}

/// mathjs `variance` with the default (unbiased) normalisation.
fn variance_val(a: &[Val]) -> EvalResult {
    if matches!(
        a,
        [Val::List(_), Val::Num(_) | Val::Bool(_)] | [Val::List(_), _, Val::Str(_)]
    ) {
        return Err(EvalError::Unsupported("variance along a dimension".into()));
    }
    let values = collection(a);
    if values.is_empty() {
        return Err(EvalError::Other(
            "Function variance requires one or more parameters (0 provided)".into(),
        ));
    }
    let count = values.len() as f64;
    let mean = ops::binary(Op::Divide, &add_all(&values)?, &Val::Num(count))?;
    let mut total: Option<Val> = None;
    for v in &values {
        let diff = ops::binary(Op::Subtract, v, &mean)?;
        let sq = ops::binary(Op::Multiply, &diff, &diff)?;
        total = Some(match total {
            None => sq,
            Some(t) => ops::binary(Op::Add, &t, &sq)?,
        });
    }
    let total = total.unwrap_or(Val::Num(0.0));
    if matches!(total, Val::Num(x) if x.is_nan()) {
        return Ok(total);
    }
    if values.len() == 1 {
        return Ok(Val::Num(0.0));
    }
    ops::binary(Op::Divide, &total, &Val::Num(count - 1.0))
}

fn variance(a: &[Val], _: &Scope) -> EvalResult {
    variance_val(a)
}

/// mathjs `std`: the square root of the variance (mathjs's own `sqrt`, complex below 0).
fn std_dev(a: &[Val], _: &Scope) -> EvalResult {
    match variance_val(a)? {
        Val::Num(v) if v >= 0.0 => num(v.sqrt()),
        Val::Num(v) => Ok(C::new(v, 0.0).sqrt().val()),
        Val::Complex { re, im } => Ok(C::new(re, im).sqrt().val()),
        _ => Err(type_error("sqrt")),
    }
}

/// mathjs `hypot`, from es6-shim: scaled by the largest so far.
fn hypot(a: &[Val], _: &Scope) -> EvalResult {
    let values = match a {
        [Val::List(_)] => deep(a),
        _ if a.iter().any(|v| matches!(v, Val::List(_))) => return Err(type_error("hypot")),
        _ => a.to_vec(),
    };
    let mut nums = Vec::with_capacity(values.len());
    for v in &values {
        match v {
            Val::Num(_) | Val::Bool(_) => nums.push(v.to_number()),
            _ => return Err(type_error("hypot")),
        }
    }
    num(hypot_numbers(&nums))
}

/// mathjs `hypot` of plain numbers (also what `simplify` folds `hypot(a, b)` with).
pub fn hypot_numbers(values: &[f64]) -> f64 {
    let (mut result, mut largest) = (0.0, 0.0);
    for v in values {
        let value = v.abs();
        if largest < value && !nearly_equal(largest, value) {
            result *= (largest / value) * (largest / value);
            result += 1.0;
            largest = value;
        } else {
            result += if value > 0.0 {
                (value / largest) * (value / largest)
            } else {
                value
            };
        }
    }
    largest * result.sqrt()
}

/// mathjs `re`, `im`, `conj` and `arg`: elementwise over lists, a number's own parts otherwise.
fn part(a: &[Val], f: fn(C) -> Val, name: &str) -> EvalResult {
    fn each(v: &Val, f: fn(C) -> Val, name: &str) -> EvalResult {
        match v {
            Val::List(items) => Ok(Val::List(
                items
                    .iter()
                    .map(|i| each(i, f, name))
                    .collect::<Result<_, _>>()?,
            )),
            Val::Complex { re, im } => Ok(f(C::new(*re, *im))),
            Val::Num(_) | Val::Bool(_) => Ok(f(C::new(v.to_number(), 0.0))),
            _ => Err(type_error(name)),
        }
    }
    match a {
        [one] => each(one, f, name),
        _ => Err(EvalError::Other(format!(
            "Wrong number of arguments to {name}"
        ))),
    }
}

fn re(a: &[Val], _: &Scope) -> EvalResult {
    part(a, |z| Val::Num(z.re), "re")
}
fn im(a: &[Val], _: &Scope) -> EvalResult {
    part(a, |z| Val::Num(z.im), "im")
}
fn conj(a: &[Val], _: &Scope) -> EvalResult {
    // a number's conjugate is the number itself, not a complex
    part(
        a,
        |z| {
            if z.im == 0.0 {
                Val::Num(z.re)
            } else {
                z.conjugate().val()
            }
        },
        "conj",
    )
}
fn arg(a: &[Val], _: &Scope) -> EvalResult {
    part(a, |z| Val::Num(z.arg()), "arg")
}

/// mathjs `range(start, end[, step][, includeEnd])` over numbers, through the expression parser's transform, which
/// includes the end unless a last boolean says otherwise: `range(0, 3)` is `[0, 1, 2, 3]` there.
fn range(a: &[Val], _: &Scope) -> EvalResult {
    let number = |v: &Val| match v {
        Val::Num(x) => Ok(*x),
        _ => Err(type_error("range")),
    };
    let (start, end, step, include) = match a {
        [s, e] => (number(s)?, number(e)?, 1.0, true),
        [s, e, Val::Bool(inc)] => (number(s)?, number(e)?, 1.0, *inc),
        [s, e, st] => (number(s)?, number(e)?, number(st)?, true),
        [s, e, st, Val::Bool(inc)] => (number(s)?, number(e)?, number(st)?, *inc),
        _ => return Err(type_error("range")),
    };
    if step == 0.0 {
        return Err(EvalError::Other("Step must be non-zero".into()));
    }
    let smaller = |x: f64, y: f64| x < y && !nearly_equal(x, y);
    let larger = |x: f64, y: f64| x > y && !nearly_equal(x, y);
    let ongoing = |x: f64| match (step > 0.0, include) {
        (true, false) => smaller(x, end),
        (true, true) => x <= end || nearly_equal(x, end),
        (false, false) => larger(x, end),
        (false, true) => x >= end || nearly_equal(x, end),
    };
    let mut out = Vec::new();
    let mut x = start;
    while ongoing(x) {
        // mathjs has no cap; a range this long would never finish there either
        if out.len() >= 10 * MAX_LIST_ALLOC {
            return Err(EvalError::Other("range too long".into()));
        }
        out.push(x);
        x += step;
    }
    Ok(list(out))
}

/// mathjs `gamma` of a number.
fn gamma(a: &[Val], _: &Scope) -> EvalResult {
    match a {
        [Val::Num(_) | Val::Bool(_)] => num(crate::eval::builtins::gamma(n(a, 0))),
        _ => Err(type_error("gamma")),
    }
}

// ---- the table ----

macro_rules! entry {
    ($name:literal, $max:expr, $f:expr) => {
        Builtin {
            name: $name,
            max_args: $max,
            call: $f,
        }
    };
}

/// Looked up after the foundation's own table in `crate::eval::builtins`.
pub const TABLE: &[Builtin] = &[
    entry!("zeta", 1, zeta),
    entry!("ln", 1, |a, _| positive_log(n(a, 0), jsmath::ln)),
    entry!("log2", 1, |a, _| positive_log(n(a, 0), jsmath::log2)),
    entry!("log10", 1, |a, _| positive_log(n(a, 0), jsmath::log10)),
    entry!("log", 2, log),
    // native only: a complex power of e, as `e^(i*pi)` already gives (the TS scope's exp is real only)
    entry!("exp", 1, |a, _| match a.first() {
        Some(Val::Complex { re, im }) => Ok(C::new(*re, *im).exp().val()),
        _ => num(jsmath::exp(n(a, 0))),
    }),
    entry!("nthroot", 2, nth_root),
    entry!("nthRoot", 2, nth_root),
    entry!("sign", 1, |a, _| num(js_sign(n(a, 0)))),
    entry!("floor", 1, |a, _| {
        let x = n(a, 0);
        num(near_integer(x).unwrap_or_else(|| x.floor()))
    }),
    entry!("ceil", 1, |a, _| {
        let x = n(a, 0);
        num(near_integer(x).unwrap_or_else(|| x.ceil()))
    }),
    entry!("round", 2, round),
    entry!("clamp", 3, clamp),
    entry!("min", 0, min),
    entry!("max", 0, max),
    entry!("mean", 0, mean),
    entry!("median", 0, median),
    entry!("stdev", 0, |a, _| num(variance_of(&nums(a), false).sqrt())),
    entry!("stdevp", 0, |a, _| num(variance_of(&nums(a), true).sqrt())),
    entry!("var", 0, |a, _| num(variance_of(&nums(a), false))),
    entry!("varp", 0, |a, _| num(variance_of(&nums(a), true))),
    entry!("mad", 0, mad),
    entry!("total", 0, |a, _| {
        num(nums(a).iter().fold(0.0, |p, c| p + c))
    }),
    entry!("length", 0, |a, _| num(nums(a).len() as f64)),
    entry!("quartile", 0, quartile),
    entry!("quantile", 0, quantile),
    entry!("corr", 0, corr),
    entry!("random", 0, random),
    entry!("randint", 0, randint),
    entry!("gcd", 0, |a, _| num(ratio_fold(a, true))),
    entry!("gcf", 0, |a, _| num(ratio_fold(a, true))),
    entry!("hcf", 0, |a, _| num(ratio_fold(a, true))),
    entry!("lcm", 0, |a, _| num(ratio_fold(a, false))),
    entry!("lcf", 0, |a, _| num(ratio_fold(a, false))),
    entry!("powmod", 3, powmod),
    entry!("combinations", 2, combinations),
    entry!("permutations", 2, permutations),
    entry!("nCr", 2, combinations),
    entry!("nPr", 2, permutations),
    entry!("inclusiveRange", 2, inclusive_range),
    entry!("csc", 1, csc),
    entry!("sec", 1, sec),
    entry!("cot", 1, cot),
    entry!("sin", 1, sin),
    entry!("cos", 1, cos),
    entry!("tan", 1, tan),
    entry!("asin", 1, asin),
    entry!("acos", 1, acos),
    entry!("atan", 1, atan),
    entry!("atan2", 2, atan2),
    entry!("acsc", 1, acsc),
    entry!("asec", 1, asec),
    entry!("acot", 1, acot),
    entry!("arcsin", 1, asin),
    entry!("arccos", 1, acos),
    entry!("arctan", 1, atan),
    entry!("arctan2", 2, atan2),
    entry!("arccsc", 1, acsc),
    entry!("arcsec", 1, asec),
    entry!("arccot", 1, acot),
    entry!("sinh", 1, |a, _| num(jsmath::sinh(n(a, 0)))),
    entry!("cosh", 1, |a, _| num(jsmath::cosh(n(a, 0)))),
    entry!("tanh", 1, |a, _| num(jsmath::tanh(n(a, 0)))),
    entry!("csch", 1, csch),
    entry!("sech", 1, |a, _| num(1.0 / jsmath::cosh(n(a, 0)))),
    entry!("coth", 1, coth),
    entry!("asinh", 1, |a, _| num(jsmath::asinh(n(a, 0)))),
    entry!("acosh", 1, acosh),
    entry!("atanh", 1, atanh),
    entry!("acsch", 1, acsch),
    entry!("asech", 1, asech),
    entry!("acoth", 1, acoth),
    // mathjs's own, reached when the scope has no such name
    entry!("sum", 0, sum),
    entry!("prod", 0, prod),
    entry!("std", 0, std_dev),
    entry!("variance", 0, variance),
    entry!("hypot", 0, hypot),
    entry!("re", 1, re),
    entry!("im", 1, im),
    entry!("conj", 1, conj),
    entry!("arg", 1, arg),
    entry!("range", 4, range),
    entry!("gamma", 1, gamma),
];

/// Replays a recorded `evalScientific(text, ctx)` call (`crate::golden`).
pub fn golden_eval(args: &[serde_json::Value]) -> serde_json::Value {
    use crate::scientific::{ScientificContext, eval_scientific};
    let text = args.first().and_then(|t| t.as_str()).unwrap_or("");
    let ctx = args.get(1).map(crate::golden::decode).unwrap_or_default();
    let context = ScientificContext {
        ans: ctx.get("ans").and_then(serde_json::Value::as_f64),
        angle_mode: if ctx.get("angleMode").and_then(|m| m.as_str()) == Some("rad") {
            AngleMode::Rad
        } else {
            AngleMode::Deg
        },
        variables: ctx
            .get("variables")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        functions: ctx
            .get("functions")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        depth: 0,
    };
    eval_scientific(text, &context)
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: &[f64], mode: AngleMode) -> f64 {
        let scope = Scope {
            angle_mode: mode,
            ..Scope::default()
        };
        let vals: Vec<Val> = args.iter().map(|x| Val::Num(*x)).collect();
        match crate::eval::builtins::call_builtin(name, &vals, &scope) {
            Some(Ok(Val::Num(x))) => x,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn v8_math_bit_for_bit() {
        // values Node 26 (V8 14.6, arm64) gives, where a correctly rounded or platform libm differs by an ulp
        assert_eq!(
            jsmath::sin(0.230_753_421_783_447_27),
            0.228_711_040_232_516_4
        );
        assert_eq!(jsmath::sin(1.747_537_612_915_039), 0.984_421_874_091_603_5);
        assert_eq!(
            jsmath::ln(0.644_221_067_428_588_9),
            -0.439_713_339_377_634_46
        );
        assert_eq!(jsmath::exp(57.264_252_757_933_95), 7.405_407_866_466_488e24);
        assert_eq!(jsmath::exp(1.0), std::f64::consts::E);
        assert_eq!(jsmath::log10(1000.0), 3.0);
        assert_eq!(jsmath::tan(240.0 * PI / 180.0), 1.732_050_807_568_875_4);
    }

    #[test]
    fn trig_snaps_float_noise() {
        assert_eq!(call("sin", &[180.0], AngleMode::Deg), 0.0);
        assert_eq!(call("cos", &[90.0], AngleMode::Deg), 0.0);
        assert!(call("tan", &[90.0], AngleMode::Deg).is_nan());
        assert_eq!(call("asin", &[1.0], AngleMode::Deg), 90.0);
    }

    #[test]
    fn rounding_and_combinatorics() {
        assert_eq!(call("round", &[1.005, 2.0], AngleMode::Deg), 1.01);
        assert_eq!(call("round", &[-2.5], AngleMode::Deg), -3.0);
        assert_eq!(call("nCr", &[5.0, 2.0], AngleMode::Deg), 10.0);
        assert_eq!(call("nCr", &[3.0, 4.0], AngleMode::Deg), 0.0);
        assert_eq!(call("nPr", &[5.0, 2.0], AngleMode::Deg), 20.0);
        assert_eq!(call("powmod", &[3.0, 100.0, 7.0], AngleMode::Deg), 4.0);
        assert_eq!(call("gcd", &[12.0, 18.0], AngleMode::Deg), 6.0);
        assert!((call("zeta", &[3.0], AngleMode::Deg) - 1.202056903159616).abs() < 1e-15);
    }
}
