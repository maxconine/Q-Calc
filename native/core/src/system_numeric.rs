//! WP-SOLVE: `src/engine/systemNumeric.ts`, square nonlinear systems by damped Newton from a fixed spread of
//! starts, plus the plain mathjs evaluation (`node.compile().evaluate(scope)` on the bare `math` instance, not the
//! engine's scope) that the system, isolate and unit solvers use.
//!
//! Plain mathjs differs from the engine's scope: `log` is natural, trig is in radians, `1/0` is Infinity, and a
//! negative square root is a Complex, which mathjs never turns back into a number by itself. Values here are a
//! double or a complex; any complex answer is "not a number", as `typeof r !== 'number'` says in the TS.

#![allow(clippy::needless_range_loop)] // index loops mirror the TS

use serde_json::{Value as Json, json};

use crate::eval::complex::C;
use crate::scientific::functions::jsmath;
use crate::syntax::{Constant, Node, Op};
use crate::types::AngleMode;

/// A plain mathjs value: a number or a Complex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pv {
    R(f64),
    C(C),
}

impl Pv {
    fn c(self) -> C {
        match self {
            Pv::R(x) => C::new(x, 0.0),
            Pv::C(z) => z,
        }
    }
}

type Res = Result<Pv, ()>;

/// Scope functions that shadow mathjs's own (the calculator's `log`, degree trig); None when not shadowed.
pub type Overrides<'a> = &'a dyn Fn(&str, &[f64]) -> Option<f64>;

fn constant(name: &str) -> Option<Pv> {
    use std::f64::consts as c;
    Some(Pv::R(match name {
        "e" | "E" => c::E,
        "pi" | "PI" => c::PI,
        "tau" => c::TAU,
        "phi" => crate::eval::builtins::PHI,
        "Infinity" => f64::INFINITY,
        "NaN" => f64::NAN,
        "LN2" => c::LN_2,
        "LN10" => c::LN_10,
        "LOG2E" => c::LOG2_E,
        "LOG10E" => c::LOG10_E,
        "SQRT1_2" => c::FRAC_1_SQRT_2,
        "SQRT2" => c::SQRT_2,
        "true" => 1.0,
        "false" => 0.0,
        "i" => return Some(Pv::C(C::new(0.0, 1.0))),
        _ => return None,
    }))
}

fn is_integer(y: f64) -> bool {
    y.is_finite() && y == y.trunc()
}

/// mathjs `_pow` for numbers with `predictable: false`.
fn pow_number(x: f64, y: f64) -> Pv {
    if is_integer(y) || x >= 0.0 {
        return Pv::R(jsmath::pow(x, y));
    }
    if (x * x < 1.0 && y == f64::INFINITY) || (x * x > 1.0 && y == f64::NEG_INFINITY) {
        return Pv::R(0.0);
    }
    Pv::C(C::new(x, 0.0).pow(C::new(y, 0.0)))
}

fn binary(op: Op, a: Pv, b: Pv) -> Res {
    if let (Pv::R(x), Pv::R(y)) = (a, b) {
        return Ok(match op {
            Op::Add => Pv::R(x + y),
            Op::Subtract => Pv::R(x - y),
            Op::Multiply | Op::DotMultiply => Pv::R(x * y),
            Op::Divide | Op::DotDivide => Pv::R(x / y),
            Op::Pow | Op::DotPow => pow_number(x, y),
            Op::Mod => {
                if y == 0.0 {
                    Pv::R(x)
                } else {
                    Pv::R(x - y * (x / y).floor())
                }
            }
            _ => return Err(()),
        });
    }
    let (x, y) = (a.c(), b.c());
    Ok(Pv::C(match op {
        Op::Add => x.add(y),
        Op::Subtract => x.sub(y),
        Op::Multiply | Op::DotMultiply => x.mul(y),
        Op::Divide | Op::DotDivide => x.div(y),
        Op::Pow | Op::DotPow => x.pow(y),
        _ => return Err(()),
    }))
}

fn real_or_complex(x: f64, ok: bool, f: fn(f64) -> f64) -> Res {
    if ok { Ok(Pv::R(f(x))) } else { Err(()) }
}

/// mathjs's own function of numbers (number mode, `predictable: false`).
fn mathjs_fn(name: &str, args: &[Pv]) -> Res {
    let real = |i: usize| match args.get(i) {
        Some(Pv::R(x)) => Ok(*x),
        _ => Err(()),
    };
    let one = |f: fn(f64) -> f64| -> Res {
        if args.len() != 1 {
            return Err(());
        }
        Ok(Pv::R(f(real(0)?)))
    };
    match name {
        "sqrt" => {
            if args.len() != 1 {
                return Err(());
            }
            match args[0] {
                Pv::R(x) if x >= 0.0 => Ok(Pv::R(x.sqrt())),
                v => Ok(Pv::C(v.c().sqrt())),
            }
        }
        "abs" => match args {
            [Pv::R(x)] => Ok(Pv::R(x.abs())),
            [Pv::C(z)] => Ok(Pv::R(z.abs())),
            _ => Err(()),
        },
        "exp" => match args {
            [Pv::R(x)] => Ok(Pv::R(jsmath::exp(*x))),
            [Pv::C(z)] => Ok(Pv::C(z.exp())),
            _ => Err(()),
        },
        "log" => {
            let ln = |v: Pv| match v {
                Pv::R(x) if x >= 0.0 => Pv::R(jsmath::ln(x)),
                v => Pv::C(v.c().log()),
            };
            match args {
                [x] => Ok(ln(*x)),
                [x, b] => binary(Op::Divide, ln(*x), ln(*b)),
                _ => Err(()),
            }
        }
        "log10" | "log2" => {
            let x = real(0)?;
            if args.len() != 1 || x < 0.0 {
                return Err(());
            }
            Ok(Pv::R(if name == "log10" {
                jsmath::log10(x)
            } else {
                jsmath::log2(x)
            }))
        }
        "cbrt" => one(jsmath::cbrt),
        "sin" => one(jsmath::sin),
        "cos" => one(jsmath::cos),
        "tan" => one(jsmath::tan),
        "sec" => one(|x| 1.0 / jsmath::cos(x)),
        "csc" => one(|x| 1.0 / jsmath::sin(x)),
        "cot" => one(|x| 1.0 / jsmath::tan(x)),
        "asin" => real_or_complex(real(0)?, (-1.0..=1.0).contains(&real(0)?), jsmath::asin),
        "acos" => real_or_complex(real(0)?, (-1.0..=1.0).contains(&real(0)?), jsmath::acos),
        "atan" => one(jsmath::atan),
        "atan2" => Ok(Pv::R(jsmath::atan2(real(0)?, real(1)?))),
        "sinh" => one(jsmath::sinh),
        "cosh" => one(jsmath::cosh),
        "tanh" => one(jsmath::tanh),
        "asinh" => one(jsmath::asinh),
        "acosh" => real_or_complex(real(0)?, real(0)? >= 1.0, jsmath::acosh),
        "atanh" => real_or_complex(real(0)?, (-1.0..=1.0).contains(&real(0)?), jsmath::atanh),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "sign" => one(|x| {
            if x == 0.0 || x.is_nan() {
                x
            } else {
                x.signum()
            }
        }),
        "round" => one(|x| x.signum() * (x.abs() + 0.5).floor()),
        "nthRoot" => {
            let a = real(0)?;
            let mut root = if args.len() > 1 { real(1)? } else { 2.0 };
            let inv = root < 0.0;
            if inv {
                root = -root;
            }
            if root == 0.0 || (a < 0.0 && root.abs() % 2.0 != 1.0) {
                return Err(());
            }
            if a == 0.0 {
                return Ok(Pv::R(if inv { f64::INFINITY } else { 0.0 }));
            }
            if !a.is_finite() {
                return Ok(Pv::R(if inv { 0.0 } else { a }));
            }
            let mut x = jsmath::pow(a.abs(), 1.0 / root);
            if a < 0.0 {
                x = -x;
            }
            Ok(Pv::R(if inv { 1.0 / x } else { x }))
        }
        "min" | "max" if !args.is_empty() => {
            let xs = args.iter().map(|_| ()).enumerate().map(|(i, _)| real(i));
            let mut best: Option<f64> = None;
            for x in xs {
                let x = x?;
                best = Some(match best {
                    None => x,
                    Some(b) if (name == "min" && x < b) || (name == "max" && x > b) => x,
                    Some(b) => b,
                });
            }
            Ok(Pv::R(best.unwrap_or(f64::NAN)))
        }
        "hypot" => {
            let mut s = 0.0f64;
            for i in 0..args.len() {
                s += real(i)?.powi(2);
            }
            Ok(Pv::R(s.sqrt()))
        }
        "factorial" => {
            let x = real(0)?;
            if x < 0.0 {
                return Err(());
            }
            Ok(Pv::R(crate::eval::builtins::factorial(x)))
        }
        _ => Err(()),
    }
}

/// `node.compile().evaluate(scope)` on the plain `math` instance; Err is any throw.
#[allow(clippy::result_unit_err)]
pub fn eval_plain(node: &Node, vars: &dyn Fn(&str) -> Option<f64>, fns: Overrides<'_>) -> Res {
    match node {
        Node::Constant(Constant::Number { value, .. }) => Ok(Pv::R(*value)),
        Node::Constant(Constant::Bool(b)) => Ok(Pv::R(f64::from(u8::from(*b)))),
        Node::Constant(_) => Err(()),
        Node::Symbol(name) => vars(name).map(Pv::R).or_else(|| constant(name)).ok_or(()),
        Node::Parenthesis(c) => eval_plain(c, vars, fns),
        Node::Operator { op, args, .. } => match (op, args.as_slice()) {
            (Op::UnaryMinus, [a]) => Ok(match eval_plain(a, vars, fns)? {
                Pv::R(x) => Pv::R(-x),
                Pv::C(z) => Pv::C(z.neg()),
            }),
            (Op::UnaryPlus, [a]) => eval_plain(a, vars, fns),
            (Op::Factorial, [a]) => mathjs_fn("factorial", &[eval_plain(a, vars, fns)?]),
            (_, [a, b]) => {
                let x = eval_plain(a, vars, fns)?;
                let y = eval_plain(b, vars, fns)?;
                binary(*op, x, y)
            }
            _ => Err(()),
        },
        Node::Function { callee, args } => {
            let Node::Symbol(name) = callee.as_ref() else {
                return Err(());
            };
            let vals = args
                .iter()
                .map(|a| eval_plain(a, vars, fns))
                .collect::<Result<Vec<_>, _>>()?;
            // a scope function is a plain JS function: Number() of a Complex is NaN
            let nums: Vec<f64> = vals
                .iter()
                .map(|v| match v {
                    Pv::R(x) => *x,
                    Pv::C(_) => f64::NAN,
                })
                .collect();
            if let Some(r) = fns(name, &nums) {
                return Ok(Pv::R(r));
            }
            if vars(name).is_some() {
                return Err(());
            }
            mathjs_fn(name, &vals)
        }
        _ => Err(()),
    }
}

/// A plain evaluation as a finite number, else None.
pub fn plain_number(
    node: &Node,
    vars: &dyn Fn(&str) -> Option<f64>,
    fns: Overrides<'_>,
) -> Option<f64> {
    match eval_plain(node, vars, fns) {
        Ok(Pv::R(x)) if x.is_finite() => Some(x),
        _ => None,
    }
}

pub fn no_overrides(_: &str, _: &[f64]) -> Option<f64> {
    None
}

/// `math.evaluate(text)` as a finite number (`guessNumber`).
pub fn guess_number(text: &str) -> Option<f64> {
    let node = crate::syntax::parse(text.trim()).ok()?;
    plain_number(&node, &|_| None, &no_overrides)
}

/// `angleScope`: the calculator's log names, and degree trig in degree mode.
pub fn angle_scope(mode: AngleMode) -> impl Fn(&str, &[f64]) -> Option<f64> {
    const DEG: f64 = std::f64::consts::PI / 180.0;
    move |name: &str, a: &[f64]| {
        let x = a.first().copied().unwrap_or(f64::NAN);
        let calc = match name {
            "ln" => Some(jsmath::ln(x)),
            "log" => Some(match a.get(1) {
                None => jsmath::log10(x),
                Some(b) => jsmath::ln(x) / jsmath::ln(*b),
            }),
            "log10" => Some(jsmath::log10(x)),
            "log2" => Some(jsmath::log2(x)),
            _ => None,
        };
        if calc.is_some() {
            return calc;
        }
        if mode != AngleMode::Deg {
            return match name {
                "arcsin" => Some(jsmath::asin(x)),
                "arccos" => Some(jsmath::acos(x)),
                "arctan" => Some(jsmath::atan(x)),
                _ => None,
            };
        }
        Some(match name {
            "arcsin" | "asin" => jsmath::asin(x) / DEG,
            "arccos" | "acos" => jsmath::acos(x) / DEG,
            "arctan" | "atan" => jsmath::atan(x) / DEG,
            "sin" => jsmath::sin(x * DEG),
            "cos" => jsmath::cos(x * DEG),
            "tan" => jsmath::tan(x * DEG),
            "sec" => 1.0 / jsmath::cos(x * DEG),
            "csc" => 1.0 / jsmath::sin(x * DEG),
            "cot" => 1.0 / jsmath::tan(x * DEG),
            _ => return None,
        })
    }
}

const MAX_SHOWN: usize = 4;
const MAX_EVALS: u32 = 40_000;

/// A starting value per unknown, by name.
pub type Guess = Vec<(String, f64)>;

/// What `solveNumericSystem` found.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericFound {
    pub solutions: Vec<Vec<f64>>,
    pub more: bool,
}

/// Solutions of a square nonlinear system, found numerically; None when nothing converges.
pub fn solve_numeric_system(
    diffs: &[Node],
    variables: &[String],
    mode: AngleMode,
    guess: Option<&Guess>,
) -> Option<NumericFound> {
    let n = variables.len();
    if n == 0 || diffs.len() != n {
        return None;
    }
    let base = angle_scope(mode);
    let evals = std::cell::Cell::new(0u32);
    let f = |x: &[f64]| -> Option<Vec<f64>> {
        evals.set(evals.get() + 1);
        if evals.get() > MAX_EVALS {
            return None;
        }
        let vars = |name: &str| variables.iter().position(|v| v == name).map(|i| x[i]);
        let mut out = Vec::with_capacity(diffs.len());
        for d in diffs {
            out.push(plain_number(d, &vars, &base)?);
        }
        Some(out)
    };
    let start: Option<Vec<f64>> = guess.and_then(|g| {
        variables
            .iter()
            .map(|v| {
                g.iter()
                    .find(|(k, _)| k == v)
                    .map(|(_, x)| *x)
                    .filter(|x| x.is_finite())
            })
            .collect()
    });
    if let Some(s) = &start
        && let Some(x) = solve_from(&f, s)
    {
        return Some(NumericFound {
            solutions: vec![snap(&f, &x)],
            more: false,
        });
    }
    let mut found: Vec<Vec<f64>> = Vec::new();
    for st in starts(n) {
        let Some(x) = solve_from(&f, &st) else {
            if evals.get() > MAX_EVALS {
                break;
            }
            continue;
        };
        let t = snap(&f, &x);
        if !found.iter().any(|s| same(s, &t)) {
            found.push(t);
        }
    }
    if found.is_empty() {
        return None;
    }
    if let Some(s) = &start {
        let dist = |p: &[f64]| norm(&p.iter().zip(s).map(|(v, w)| v - w).collect::<Vec<_>>());
        let mut best = found[0].clone();
        for p in &found[1..] {
            if dist(p) < dist(&best) {
                best = p.clone();
            }
        }
        return Some(NumericFound {
            solutions: vec![best],
            more: false,
        });
    }
    found.sort_by(|a, b| {
        let d = norm(a) - norm(b);
        let d = if d == 0.0 || d.is_nan() {
            a[0] - b[0]
        } else {
            d
        };
        d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    let more = found.len() > MAX_SHOWN;
    let mut shown: Vec<Vec<f64>> = found.into_iter().take(MAX_SHOWN).collect();
    shown.sort_by(|a, b| {
        for i in 0..a.len() {
            if a[i] != b[i] {
                return (a[i] - b[i])
                    .partial_cmp(&0.0)
                    .unwrap_or(std::cmp::Ordering::Equal);
            }
        }
        std::cmp::Ordering::Equal
    });
    Some(NumericFound {
        solutions: shown,
        more,
    })
}

fn starts(n: usize) -> Vec<Vec<f64>> {
    const VALUES: [f64; 20] = [
        1.0, -1.0, 0.5, -0.5, 2.0, -2.0, 0.1, 3.0, -3.0, 5.0, -5.0, 10.0, 0.3, -0.7, 1.7, 30.0,
        -30.0, 90.0, 7.0, -10.0,
    ];
    let mut out = vec![vec![1.0; n], vec![0.5; n], vec![-1.0; n]];
    // JS doubles: the product passes 2^53, so it rounds just as the TS does
    let mut seed = 12345.0f64;
    while out.len() < 40 {
        let row = (0..n)
            .map(|_| {
                seed = (seed * 1_103_515_245.0 + 12345.0) % 2_147_483_648.0;
                VALUES[(seed % 20.0) as usize]
            })
            .collect();
        out.push(row);
    }
    out
}

fn norm(x: &[f64]) -> f64 {
    x.iter().fold(0.0, |s, v| s + v * v).sqrt()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).fold(0.0, |s, (v, w)| s + v * w)
}

fn same(a: &[f64], b: &[f64]) -> bool {
    let size = a.iter().chain(b).fold(1f64, |m, v| m.max(v.abs()));
    a.iter().zip(b).all(|(v, w)| (v - w).abs() <= 1e-5 * size)
}

type F<'a> = dyn Fn(&[f64]) -> Option<Vec<f64>> + 'a;

fn snap(f: &F<'_>, x: &[f64]) -> Vec<f64> {
    let residual = |p: &[f64]| f(p).map_or(f64::INFINITY, |r| dot(&r, &r).sqrt());
    let mut best = x.to_vec();
    let mut best_res = residual(&best);
    let near = |v: f64| {
        let r = crate::jsnum::round(v);
        if (v - r).abs() <= 1e-3 * 1f64.max(r.abs()) {
            r
        } else {
            v
        }
    };
    let whole: Vec<f64> = best.iter().map(|v| near(*v)).collect();
    let whole_res = residual(&whole);
    if whole_res <= best_res {
        best = whole;
        best_res = whole_res;
    }
    for i in 0..x.len() {
        let r = crate::jsnum::round(best[i]);
        if r == best[i] || (best[i] - r).abs() > 1e-3 * 1f64.max(r.abs()) {
            continue;
        }
        let mut trial = best.clone();
        trial[i] = r;
        let res = residual(&trial);
        if res <= best_res {
            best = trial;
            best_res = res;
        }
    }
    best.into_iter()
        .map(|v| if v == 0.0 { 0.0 } else { v })
        .collect()
}

fn solve_from(f: &F<'_>, x0: &[f64]) -> Option<Vec<f64>> {
    let mut x = x0.to_vec();
    let mut fx = f(&x)?;
    let start = dot(&fx, &fx).sqrt();
    let mut last: Option<Vec<f64>> = None;
    for _ in 0..200 {
        if fx.iter().all(|v| *v == 0.0) {
            return Some(x);
        }
        let j = jacobian(f, &x, &fx)?;
        let dx = newton_step(&j, &fx)?;
        last = Some(dx.clone());
        if dx
            .iter()
            .zip(&x)
            .all(|(d, xi)| d.abs() <= 1e-15 * xi.abs().max(1e-300))
        {
            break;
        }
        let size = norm(&dx);
        let mut moved = false;
        let mut alpha = 1.0f64;
        while alpha > 1e-6 {
            let xt: Vec<f64> = x.iter().zip(&dx).map(|(v, d)| v + alpha * d).collect();
            if let Some(ft) = f(&xt)
                && let Some(next) = newton_step(&j, &ft)
                && norm(&next) <= (1.0 - alpha / 4.0) * size
            {
                x = xt;
                fx = ft;
                moved = true;
                break;
            }
            alpha /= 2.0;
        }
        if !moved {
            break;
        }
    }
    let last = last?;
    let nx = norm(&x);
    let settled = last
        .iter()
        .zip(&x)
        .all(|(d, xi)| d.abs() <= 1e-8 * xi.abs().max(nx * 1e-8).max(1e-300));
    let residual = dot(&fx, &fx).sqrt();
    let touching = residual <= 1e-14 * start;
    ((settled && residual <= 1e-6 * start) || touching).then_some(x)
}

fn newton_step(j: &[Vec<f64>], f: &[f64]) -> Option<Vec<f64>> {
    if let Some(d) = linear_solve(j, &f.iter().map(|v| -v).collect::<Vec<_>>()) {
        return Some(d);
    }
    let n = j[0].len();
    let a: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|k| j.iter().fold(0.0, |s, row| s + row[i] * row[k]))
                .collect()
        })
        .collect();
    let diag_max = (0..n).fold(1e-300f64, |m, i| {
        if a[i][i].is_nan() || m.is_nan() {
            f64::NAN
        } else {
            m.max(a[i][i])
        }
    });
    let mu = 1e-10 * diag_max;
    let g: Vec<f64> = (0..n)
        .map(|i| {
            -j.iter()
                .enumerate()
                .fold(0.0, |s, (k, row)| s + row[i] * f[k])
        })
        .collect();
    let m: Vec<Vec<f64>> = a
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .map(|(k, v)| if i == k { v + mu } else { *v })
                .collect()
        })
        .collect();
    linear_solve(&m, &g)
}

fn jacobian(f: &F<'_>, x: &[f64], fx: &[f64]) -> Option<Vec<Vec<f64>>> {
    let m = fx.len();
    let size = x.iter().fold(f64::NEG_INFINITY, |s, v| s.max(v.abs()));
    let mut jm = vec![vec![0.0; x.len()]; m];
    for j in 0..x.len() {
        let h = 1e-7 * x[j].abs().max(size).max(1e-300);
        let mut xp = x.to_vec();
        xp[j] = x[j] + h;
        let fp = f(&xp)?;
        for i in 0..m {
            jm[i][j] = (fp[i] - fx[i]) / h;
        }
    }
    Some(jm)
}

fn linear_solve(m: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut a: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.push(b[i]);
            r
        })
        .collect();
    for c in 0..n {
        let mut p = c;
        for r in c + 1..n {
            if a[r][c].abs() > a[p][c].abs() {
                p = r;
            }
        }
        if a[p][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..=n {
                a[r][k] -= f * a[c][k];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = a[r][n];
        for k in r + 1..n {
            s -= a[r][k] * x[k];
        }
        x[r] = s / a[r][r];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    match name {
        "guessNumber" => Some(json!(guess_number(
            &crate::golden::str_arg(args, 0).unwrap_or_default()
        ))),
        _ => None,
    }
}
