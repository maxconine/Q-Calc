//! One denominator (`together` in `calculus.ts`):
//! `((γ²D - γLv)/√(γ² + v²)² - D)/(m √(γ² + v²))` is `-v·(Dv + Lγ)/(m (γ² + v²)^(3/2))`.
//!
//! The TS keeps powers in JS `Map`s, whose iteration follows insertion; [`Pows`] keeps that order.

use std::collections::HashMap;

use super::tree::{N, T, call, constant, op, text};
use crate::jsnum;
use crate::scientific::functions::jsmath;
use crate::symbolic::Op;

const MAX_TERMS: usize = 40;
const MAX_EXPAND: f64 = 6.0;

struct TooBig;

type Res<X> = Result<X, TooBig>;

fn is_whole(x: f64) -> bool {
    (x - jsnum::round(x)).abs() < 1e-9
}

/// A JS `Map<string, number>`: insertion-ordered.
#[derive(Clone, Debug, Default)]
struct Pows(Vec<(String, f64)>);

impl Pows {
    fn one(k: &str, e: f64) -> Pows {
        Pows(vec![(k.to_string(), e)])
    }
    fn get(&self, k: &str) -> Option<f64> {
        self.0.iter().find(|(x, _)| x == k).map(|(_, e)| *e)
    }
    fn has(&self, k: &str) -> bool {
        self.get(k).is_some()
    }
    fn set(&mut self, k: &str, e: f64) {
        match self.0.iter_mut().find(|(x, _)| x == k) {
            Some(slot) => slot.1 = e,
            None => self.0.push((k.to_string(), e)),
        }
    }
    fn delete(&mut self, k: &str) {
        self.0.retain(|(x, _)| x != k);
    }
    fn iter(&self) -> impl Iterator<Item = (&str, f64)> {
        self.0.iter().map(|(k, e)| (k.as_str(), *e))
    }
    fn filtered(&self) -> Pows {
        Pows(self.0.iter().filter(|(_, e)| !is_whole(*e) || jsnum::round(*e) != 0.0).cloned().collect())
    }
}

#[derive(Clone, Debug)]
struct Term {
    c: f64,
    p: Pows,
}

fn term(c: f64) -> Term {
    Term { c, p: Pows::default() }
}

struct Atom {
    node: T,
    sum: Option<Vec<Term>>,
}

/// `Number(e.toFixed(9))` as text.
fn fixed9(e: f64) -> String {
    jsnum::to_string(jsnum::parse(&jsnum::to_fixed(e, 9)).unwrap_or(f64::NAN))
}

fn sorted_keys(p: &Pows) -> Vec<(String, f64)> {
    let mut v = p.0.clone();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn term_key(p: &Pows) -> String {
    sorted_keys(&p.filtered())
        .iter()
        .map(|(k, e)| format!("{k}^{}", fixed9(*e)))
        .collect::<Vec<_>>()
        .join("*")
}

/// JS's default sort: by UTF-16 code units.
fn js_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn sum_key(terms: &[Term]) -> String {
    let mut keys: Vec<String> = terms
        .iter()
        .map(|t| {
            let c = jsnum::to_string(jsnum::parse(&jsnum::to_precision(t.c, 12)).unwrap_or(f64::NAN));
            format!("{c}:{}", term_key(&t.p))
        })
        .collect();
    keys.sort_by(|a, b| js_cmp(a, b));
    keys.join("+")
}

fn js_max0(terms: &[Term]) -> f64 {
    let mut m = 0.0f64;
    for t in terms {
        let a = t.c.abs();
        if a.is_nan() {
            return f64::NAN;
        }
        if a > m {
            m = a;
        }
    }
    m
}

fn combine(terms: Vec<Term>) -> Res<Vec<Term>> {
    let scale = js_max0(&terms);
    let mut out: Vec<(String, Term)> = Vec::new();
    for t in terms {
        let p = t.p.filtered();
        let key = term_key(&p);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, prev)) => prev.c += t.c,
            None => out.push((key, Term { c: t.c, p })),
        }
    }
    let kept: Vec<Term> = out.into_iter().map(|(_, t)| t).filter(|t| t.c.abs() > 1e-12 * scale).collect();
    if kept.len() > MAX_TERMS {
        return Err(TooBig);
    }
    Ok(kept)
}

fn mul_term(a: &Term, b: &Term) -> Term {
    let mut p = a.p.clone();
    for (k, e) in b.p.iter() {
        let next = p.get(k).unwrap_or(0.0) + e;
        p.set(k, next);
    }
    Term { c: a.c * b.c, p }
}

fn mul_sum(a: &[Term], b: &[Term]) -> Res<Vec<Term>> {
    combine(a.iter().flat_map(|x| b.iter().map(move |y| mul_term(x, y))).collect())
}

fn whole_power(a: &[Term], k: i64) -> Res<Vec<Term>> {
    let mut out = vec![term(1.0)];
    for _ in 0..k {
        out = mul_sum(&out, a)?;
    }
    Ok(out)
}

/// `p/q` with a small `q`, or None.
fn to_fraction(x: f64) -> Option<(f64, f64)> {
    if !x.is_finite() {
        return None;
    }
    for q in 1..=100_000 {
        let q = f64::from(q);
        let p = jsnum::round(x * q);
        if (p / q - x).abs() <= 1e-10 * x.abs().max(1.0) {
            return Some((p, q));
        }
    }
    None
}

fn constant_of(n: &T) -> Option<f64> {
    match &**n {
        N::Const(v) => Some(*v),
        N::Op(Op::UnaryMinus, a) => constant_of(&a[0]).map(|v| -v),
        N::Op(Op::Divide, a) => {
            let x = constant_of(&a[0])?;
            let y = constant_of(&a[1])?;
            if y == 0.0 { None } else { Some(x / y) }
        }
        _ => None,
    }
}

fn degree(t: &Term) -> f64 {
    t.p.iter().fold(0.0, |d, (_, e)| d + e)
}

fn product(fs: Vec<T>) -> T {
    if fs.len() == 1 { fs.into_iter().next().expect("one") } else { op(Op::Multiply, fs) }
}

/// The recursive `gcd` of `together`: `b ? gcd(b, a % b) : |a|`.
fn gcd(a: f64, b: f64) -> f64 {
    if b != 0.0 && !b.is_nan() { gcd(b, a % b) } else { a.abs() }
}

fn order_terms(a: &Term, b: &Term) -> std::cmp::Ordering {
    let d = degree(b) - degree(a);
    if d != 0.0 && !d.is_nan() {
        return if d < 0.0 { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
    }
    if term_key(&a.p) < term_key(&b.p) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
}

#[derive(Default)]
struct Terms {
    atoms: HashMap<String, Atom>,
}

impl Terms {
    fn atom(&mut self, key: &str, node: T, e: f64, sum: Option<Vec<Term>>) -> Vec<Term> {
        self.atoms.entry(key.to_string()).or_insert(Atom { node, sum });
        vec![Term { c: 1.0, p: Pows::one(key, e) }]
    }

    fn of(&mut self, n: &T) -> Res<Vec<Term>> {
        match &**n {
            N::Const(v) => {
                if !v.is_finite() {
                    return Err(TooBig);
                }
                Ok(if *v == 0.0 { Vec::new() } else { vec![term(*v)] })
            }
            N::Sym(s) => Ok(self.atom(s, n.clone(), 1.0, None)),
            N::Call(name, a) => {
                if name == "sqrt" {
                    let base = self.of(&a[0])?;
                    return self.power(base, 0.5);
                }
                Ok(self.atom(&text(n), n.clone(), 1.0, None))
            }
            N::Op(o, a) => match o {
                Op::Add => {
                    let mut all = Vec::new();
                    for x in a {
                        all.extend(self.of(x)?);
                    }
                    combine(all)
                }
                Op::Subtract => {
                    let mut all = self.of(&a[0])?;
                    all.extend(self.of(&a[1])?.into_iter().map(|t| Term { c: -t.c, ..t }));
                    combine(all)
                }
                Op::UnaryMinus => Ok(self.of(&a[0])?.into_iter().map(|t| Term { c: -t.c, ..t }).collect()),
                Op::UnaryPlus => self.of(&a[0]),
                Op::Multiply => {
                    let mut acc = self.of(&a[0])?;
                    for x in &a[1..] {
                        let next = self.of(x)?;
                        acc = mul_sum(&acc, &next)?;
                    }
                    Ok(acc)
                }
                Op::Divide => {
                    let top = self.of(&a[0])?;
                    let bottom = self.of(&a[1])?;
                    let inv = self.power(bottom, -1.0)?;
                    mul_sum(&top, &inv)
                }
                Op::Pow => match constant_of(&a[1]) {
                    None => Ok(self.atom(&text(n), n.clone(), 1.0, None)),
                    Some(k) => {
                        let base = self.of(&a[0])?;
                        self.power(base, k)
                    }
                },
                _ => Err(TooBig),
            },
        }
    }

    fn power(&mut self, base: Vec<Term>, k: f64) -> Res<Vec<Term>> {
        if k == 0.0 {
            return Ok(vec![term(1.0)]);
        }
        if base.is_empty() {
            if k < 0.0 {
                return Err(TooBig);
            }
            return Ok(Vec::new());
        }
        if is_whole(k) && k > 0.0 && k <= MAX_EXPAND {
            return whole_power(&base, jsnum::round(k) as i64);
        }
        if base.len() == 1 {
            let t = &base[0];
            if is_whole(k) {
                let p = Pows(t.p.iter().map(|(a, e)| (a.to_string(), e * k)).collect());
                return Ok(vec![Term { c: jsmath::pow(t.c, k), p }]);
            }
            // (x²)^(1/2) is |x|, not x
            if t.c > 0.0
                && t.p.iter().all(|(_, e)| !is_whole(e) || jsnum::round(e) % 2.0 != 0.0 || is_whole(e * k))
            {
                let mut out = vec![term(jsmath::pow(t.c, k))];
                let pows: Vec<(String, f64)> = t.p.0.clone();
                for (a, e) in pows {
                    let even = is_whole(e) && jsnum::round(e) % 2.0 == 0.0;
                    let piece = if even {
                        let inner = self.atoms.get(&a).map(|x| x.node.clone()).ok_or(TooBig)?;
                        self.atom(&format!("abs({a})"), call("abs", vec![inner]), e * k, None)
                    } else {
                        vec![Term { c: 1.0, p: Pows::one(&a, e * k) }]
                    };
                    out = mul_sum(&out, &piece)?;
                }
                return Ok(out);
            }
        }
        let key = format!("({})", sum_key(&base));
        let node = self.node(&base)?;
        Ok(self.atom(&key, node, k, Some(base)))
    }

    /// Atoms to powers, as a product; roots print as √.
    fn factor_nodes(&self, p: &Pows) -> Res<Option<Vec<T>>> {
        let mut out = Vec::new();
        for (k, e) in sorted_keys(p) {
            if is_whole(e) && jsnum::round(e) == 0.0 {
                continue;
            }
            let inner = k.strip_prefix("abs(").and_then(|r| r.strip_suffix(')'));
            // |v|² prints as v²
            let node_of = |key: &str| self.atoms.get(key).map(|a| a.node.clone()).ok_or(TooBig);
            let base = match inner {
                Some(i) if !i.is_empty() && self.atoms.contains_key(i) && is_whole(e) && jsnum::round(e) % 2.0 == 0.0 => {
                    node_of(i)?
                }
                _ => node_of(&k)?,
            };
            if is_whole(e) {
                let r = jsnum::round(e);
                out.push(if r == 1.0 { base } else { op(Op::Pow, vec![base, constant(r)]) });
            } else if (e - 0.5).abs() < 1e-9 {
                out.push(call("sqrt", vec![base]));
            } else {
                match to_fraction(e) {
                    Some((fp, fq)) if fq <= 12.0 => out.push(op(
                        Op::Pow,
                        vec![base, op(Op::Divide, vec![constant(fp), constant(fq)])],
                    )),
                    _ => return Ok(None),
                }
            }
        }
        Ok(Some(out))
    }

    fn node(&self, terms: &[Term]) -> Res<T> {
        let mut sorted = terms.to_vec();
        sorted.sort_by(order_terms);
        let mut parts = Vec::new();
        for t in &sorted {
            let fs = self.factor_nodes(&t.p)?.unwrap_or_else(|| vec![constant(f64::NAN)]);
            let c = t.c.abs();
            let body = if fs.is_empty() {
                constant(c)
            } else if c == 1.0 {
                product(fs)
            } else {
                let mut all = vec![constant(c)];
                all.extend(fs);
                product(all)
            };
            parts.push((t.c < 0.0, body));
        }
        let mut it = parts.into_iter();
        let (neg, body) = it.next().ok_or(TooBig)?;
        let mut out = if neg { op(Op::UnaryMinus, vec![body]) } else { body };
        for (neg, body) in it {
            out = op(if neg { Op::Subtract } else { Op::Add }, vec![out, body]);
        }
        Ok(out)
    }
}

/// The expression over a single denominator with the numerator multiplied out and its common factors pulled out
/// front; None when it doesn't come out (too many terms, unwhole coefficients, odd powers).
pub fn together(n: &T) -> Option<T> {
    run(n).ok().flatten()
}

fn run(n: &T) -> Res<Option<T>> {
    let mut terms = Terms::default();
    let mut sum = terms.of(n)?;
    if sum.is_empty() {
        return Ok(Some(constant(0.0)));
    }
    // v² is |v|², so it can cancel with a |v| from a root
    sum = combine(
        sum.into_iter()
            .map(|t| {
                let mut p = t.p.clone();
                for (k, e) in t.p.iter() {
                    let abs = format!("abs({k})");
                    if terms.atoms.contains_key(&abs) && is_whole(e) && jsnum::round(e) % 2.0 == 0.0 {
                        p.delete(k);
                        let next = p.get(&abs).unwrap_or(0.0) + e;
                        p.set(&abs, next);
                    }
                }
                Term { c: t.c, p }
            })
            .collect(),
    )?;
    let mut den = Pows::default();
    for t in &sum {
        for (k, e) in t.p.iter() {
            if e < 0.0 {
                let next = den.get(k).unwrap_or(0.0).min(e);
                den.set(k, next);
            }
        }
    }
    let mut top: Vec<Term> = Vec::new();
    for t in &sum {
        let mut expanded = vec![term(t.c)];
        let mut p = t.p.clone();
        for (k, e) in den.iter() {
            let next = p.get(k).unwrap_or(0.0) - e;
            p.set(k, next);
        }
        for (k, e) in p.iter() {
            // a sum back to a whole power multiplies out, so its terms can cancel
            let inner = terms.atoms.get(k).and_then(|a| a.sum.clone());
            let whole = (e + 1e-9).floor();
            let rest = e - whole;
            match inner {
                Some(inner) if (1.0..=MAX_EXPAND).contains(&whole) => {
                    expanded = mul_sum(&expanded, &whole_power(&inner, whole as i64)?)?;
                    if rest.abs() > 1e-9 {
                        expanded = expanded.iter().map(|x| mul_term(x, &Term { c: 1.0, p: Pows::one(k, rest) })).collect();
                    }
                }
                _ => {
                    expanded = expanded.iter().map(|x| mul_term(x, &Term { c: 1.0, p: Pows::one(k, e) })).collect();
                }
            }
        }
        top.extend(expanded);
    }
    top = combine(top)?;
    if top.is_empty() {
        return Ok(Some(constant(0.0)));
    }

    // what every numerator term shares comes out front, and cancels against the denominator
    let mut outer = Pows::default();
    for (k, _) in top[0].p.iter() {
        let least = top.iter().map(|t| t.p.get(k).unwrap_or(0.0)).fold(f64::INFINITY, |m, x| {
            if m.is_nan() || x.is_nan() { f64::NAN } else { m.min(x) }
        });
        if least > 1e-9 {
            outer.set(k, least);
        }
    }
    top = top
        .into_iter()
        .map(|t| {
            let mut p = t.p.clone();
            for (k, e) in outer.iter() {
                let next = p.get(k).unwrap_or(f64::NAN) - e;
                p.set(k, next);
            }
            Term { c: t.c, p }
        })
        .collect();
    let mut bottom = Pows(den.iter().map(|(k, e)| (k.to_string(), -e)).collect());
    for (k, e) in outer.clone().iter() {
        let m = e.min(bottom.get(k).unwrap_or(0.0));
        if m > 0.0 {
            outer.set(k, e - m);
            let next = bottom.get(k).unwrap_or(f64::NAN) - m;
            bottom.set(k, next);
        }
    }

    // whole-number coefficients, with the sign of the leading term out front
    let mut fractions = Vec::new();
    for t in &top {
        match to_fraction(t.c) {
            Some(f) => fractions.push(f),
            None => return Ok(None),
        }
    }
    let g = fractions.iter().fold(0.0, |a, f| gcd(a, f.0));
    let l = fractions.iter().fold(1.0, |a, f| (a * f.1) / gcd(a, f.1));
    let mut ordered = top.clone();
    ordered.sort_by(order_terms);
    let sign = if ordered[0].c < 0.0 { -1.0 } else { 1.0 };
    let scale = (sign * g) / l;
    top = top.into_iter().map(|t| Term { c: jsnum::round(t.c / scale), p: t.p }).collect();
    let (mut p_out, q_out) = to_fraction(scale.abs()).unwrap_or((0.0, 0.0));
    if q_out == 0.0 || q_out.is_nan() {
        return Ok(None);
    }

    // a numerator that is itself one of the denominator's sums cancels with it
    if top.len() > 1 {
        let key = format!("({})", sum_key(&top));
        if bottom.has(&key) && bottom.get(&key).unwrap_or(0.0) >= 1.0 {
            let next = bottom.get(&key).unwrap_or(0.0) - 1.0;
            bottom.set(&key, next);
            top = vec![term(1.0)];
        }
    }
    if top.len() == 1 {
        let t = top.remove(0);
        for (k, e) in t.p.iter() {
            let next = outer.get(k).unwrap_or(0.0) + e;
            outer.set(k, next);
        }
        p_out *= t.c.abs();
    }

    let (Some(upper), Some(lower)) = (terms.factor_nodes(&outer)?, terms.factor_nodes(&bottom)?) else {
        return Ok(None);
    };
    let mut num_factors = Vec::new();
    if p_out != 1.0 || (upper.is_empty() && top.is_empty()) {
        num_factors.push(constant(p_out));
    }
    num_factors.extend(upper);
    if !top.is_empty() {
        num_factors.push(terms.node(&top)?);
    }
    let mut den_factors = Vec::new();
    if q_out != 1.0 {
        den_factors.push(constant(q_out));
    }
    den_factors.extend(lower);
    let mut out = product(num_factors);
    if !den_factors.is_empty() {
        out = op(Op::Divide, vec![out, product(den_factors)]);
    }
    Ok(Some(if sign < 0.0 { op(Op::UnaryMinus, vec![out]) } else { out }))
}
