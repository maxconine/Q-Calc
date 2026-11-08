//! The generic matrix evaluator: a line worked out over the most exact field that holds every entry, tried in turn —
//! rationals (`[1/3 2; 0.5 1]`), sums of square roots (`[sqrt(2) 1; 1 sqrt(2)]`), rational functions in the unknown
//! names inside a matrix (`[k 2; 3 k]`) — and, for the functions only this evaluator knows (`eig`, `null`, `svd`…),
//! plain floats as the last resort.
//!
//! A line a field can't hold (an entry like `sin(30)` in an exact field, a name it doesn't know) is `NotExact`, and the
//! next field takes it. Over the rational functions a pivot that is a nonzero expression counts as nonzero, so
//! `rref([k 1; 2 1])` is the identity, as it is for every k but 2.

use std::cell::Cell as StdCell;

use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::show::{Answer, Fmt, shows_whole};
use super::{MatrixOptions, callee_name, has_matrix_part, is_t};
use crate::exact::{
    Field, Mono, RadField, Rat, RatField, Rf, RfField, bgcd, guarded, has_sign_after_first,
    join_signed, key_exps, mono_key, pconst, rat, rf_cancel,
};
use crate::jsnum;
use crate::symbolic::print::{PrintOptions, to_string};
use crate::syntax::{Constant, Node, Op};

/// Past this many entries the exact fields are skipped.
const MAX_CELLS: usize = 400;

pub(super) type G<C> = Vec<Vec<C>>;

/// A value: a scalar, a grid, or a finished answer that is neither (eigenvalues, a span, P and D), which only a
/// whole line may be.
#[derive(Clone, Debug)]
pub(super) enum X<C> {
    Num(C),
    Grid(G<C>),
    Done(Answer),
}

pub(super) enum XFail {
    /// Not something this field can hold; the next field takes it.
    NotExact,
    /// A real error, shown as the answer.
    Matrix(String),
}

pub(super) type XR<T> = Result<T, XFail>;

pub(super) fn fail<T>(msg: impl Into<String>) -> XR<T> {
    Err(XFail::Matrix(msg.into()))
}

pub(super) fn size<C>(g: &[Vec<C>]) -> String {
    format!("{}×{}", g.len(), g.first().map_or(0, Vec::len))
}

/// A field matrix cells live in, plus how a cell is read from a leaf and read back as a number.
pub(super) trait Cells: Field {
    fn lift(&self, r: &Rat) -> Self::C;
    fn param(&self, _name: &str) -> Option<Self::C> {
        None
    }
    /// √r for a rational r ≥ 0, where the field holds it.
    fn root(&self, _r: &Rat) -> Option<Self::C> {
        None
    }
    /// The cell as a plain rational, when it is one.
    fn as_rat(&self, c: &Self::C) -> Option<Rat>;
    fn approx(&self, c: &Self::C) -> Option<f64>;
    /// Division can't be trusted to cancel (several parameters), so determinants expand instead.
    fn division_free(&self) -> bool {
        false
    }
    /// The cell as an answer writes it.
    fn show(&self, c: &Self::C) -> String {
        self.str(c)
    }
    /// Floats only: a leaf the exact fields can't read (`pi`, `sin(30)`).
    fn float_leaf(&self, _v: f64) -> Option<Self::C> {
        None
    }
    fn is_float(&self) -> bool {
        false
    }
    /// √c, where the field holds it.
    fn sqrt(&self, c: &Self::C) -> Option<Self::C> {
        let r = self.as_rat(c)?;
        if r.n.is_negative() {
            return None;
        }
        self.root(&r)
    }
    /// -1, 0 or 1, when the field knows.
    fn sign(&self, c: &Self::C) -> Option<i32> {
        if self.is_zero(c) {
            return Some(0);
        }
        let v = self.approx(c)?;
        Some(if v < 0.0 { -1 } else { 1 })
    }
    /// How good a pivot a cell is; exact fields take the first nonzero one.
    fn weight(&self, _c: &Self::C) -> f64 {
        0.0
    }
    /// A basis vector as a textbook writes it: whole numbers where it can be.
    fn tidy_vec(&self, v: Vec<Self::C>) -> Vec<Self::C> {
        v
    }
}

pub(super) fn rat_f64(r: &Rat) -> f64 {
    match (r.n.to_f64(), r.d.to_f64()) {
        (Some(n), Some(d)) if n.is_finite() && d.is_finite() => n / d,
        _ => {
            // a huge numerator or denominator: scale both down by the same power of ten
            let shift =
                r.n.to_string()
                    .len()
                    .max(r.d.to_string().len())
                    .saturating_sub(300) as u32;
            let p = BigInt::from(10).pow(shift);
            (&r.n / &p).to_f64().unwrap_or(f64::NAN) / (&r.d / &p).to_f64().unwrap_or(f64::NAN)
        }
    }
}

pub(super) fn ri(n: i64) -> Rat {
    rat(BigInt::from(n), BigInt::one())
}

impl Cells for RatField {
    fn lift(&self, r: &Rat) -> Rat {
        r.clone()
    }
    fn as_rat(&self, c: &Rat) -> Option<Rat> {
        Some(c.clone())
    }
    fn approx(&self, c: &Rat) -> Option<f64> {
        Some(rat_f64(c))
    }
    fn tidy_vec(&self, v: Vec<Rat>) -> Vec<Rat> {
        // times the denominators' lcm, over the numerators' gcd
        let mut l = BigInt::one();
        let mut g = BigInt::zero();
        for x in &v {
            l = &l / bgcd(&l, &x.d) * &x.d;
            g = bgcd(&g, &x.n);
        }
        if g.is_zero() {
            return v;
        }
        v.iter()
            .map(|x| rat(&x.n * &l / &x.d / &g, BigInt::one()))
            .collect()
    }
}

impl Cells for RadField {
    fn lift(&self, r: &Rat) -> Vec<Rat> {
        self.rat_of(r)
    }
    fn root(&self, r: &Rat) -> Option<Vec<Rat>> {
        self.sqrt_of(r)
    }
    fn as_rat(&self, c: &Vec<Rat>) -> Option<Rat> {
        let c = self.fit(c);
        c[1..].iter().all(|x| x.n.is_zero()).then(|| c[0].clone())
    }
    /// As the calculator writes a surd: `2sqrt(2)`, `sqrt(2)/2`, `3sqrt(5)/2`, `1 + sqrt(3)`.
    fn show(&self, c: &Vec<Rat>) -> String {
        let primes = self.primes.borrow();
        let mut parts = Vec::new();
        for (mask, k) in self.fit(c).iter().enumerate() {
            if k.n.is_zero() {
                continue;
            }
            let mut radicand = BigInt::one();
            for (bit, p) in primes.iter().enumerate() {
                if (mask >> bit) & 1 == 1 {
                    radicand *= p;
                }
            }
            let (num, den) = (k.n.abs(), k.d.clone());
            let body = if radicand.is_one() {
                crate::exact::rstr(&rat(num, den))
            } else {
                let coeff = if num.is_one() {
                    String::new()
                } else {
                    num.to_string()
                };
                let over = if den.is_one() {
                    String::new()
                } else {
                    format!("/{den}")
                };
                format!("{coeff}sqrt({radicand}){over}")
            };
            parts.push((k.n.is_negative(), body));
        }
        if parts.is_empty() {
            "0".into()
        } else {
            join_signed(&parts)
        }
    }
    fn approx(&self, c: &Vec<Rat>) -> Option<f64> {
        let primes = self.primes.borrow();
        let mut sum = 0.0;
        for (mask, k) in self.fit(c).iter().enumerate() {
            let mut rad = 1.0;
            for (bit, p) in primes.iter().enumerate() {
                if (mask >> bit) & 1 == 1 {
                    rad *= p.to_f64()?;
                }
            }
            sum += rat_f64(k) * rad.sqrt();
        }
        Some(sum)
    }
}

/// A polynomial in the parameters, highest degree first as a textbook writes it: `k^2 - 6`, `a*d - b*c`.
fn poly_desc(p: &Mono, params: &[String]) -> String {
    if p.is_empty() {
        return "0".into();
    }
    let mut terms: Vec<(Vec<i64>, BigInt)> =
        p.iter().map(|(k, c)| (key_exps(k), c.clone())).collect();
    terms.sort_by(|(a, _), (b, _)| {
        let (da, db): (i64, i64) = (a.iter().sum(), b.iter().sum());
        db.cmp(&da).then_with(|| {
            let n = a.len().max(b.len());
            let at = |v: &Vec<i64>, i: usize| v.get(i).copied().unwrap_or(0);
            (0..n)
                .map(|i| at(b, i).cmp(&at(a, i)))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let parts: Vec<(bool, String)> = terms
        .into_iter()
        .map(|(exps, c)| {
            let mon = exps
                .iter()
                .enumerate()
                .filter(|(_, e)| **e != 0)
                .map(|(i, e)| {
                    let name = params.get(i).cloned().unwrap_or_default();
                    if *e == 1 { name } else { format!("{name}^{e}") }
                })
                .collect::<Vec<_>>()
                .join("*");
            let mag = c.abs();
            let body = if mon.is_empty() {
                mag.to_string()
            } else if mag.is_one() {
                mon
            } else {
                format!("{mag}*{mon}")
            };
            (c.is_negative(), body)
        })
        .collect();
    join_signed(&parts)
}

impl Cells for RfField {
    fn lift(&self, r: &Rat) -> Rf {
        Rf {
            n: pconst(r.n.clone()),
            d: pconst(r.d.clone()),
        }
    }
    fn param(&self, name: &str) -> Option<Rf> {
        let i = self.params.iter().position(|p| p == name)?;
        let mut exps = vec![0; self.params.len()];
        exps[i] = 1;
        let mut n = crate::exact::OMap::new();
        n.set(mono_key(&exps), BigInt::one());
        Some(Rf {
            n,
            d: pconst(BigInt::one()),
        })
    }
    fn as_rat(&self, c: &Rf) -> Option<Rat> {
        let only_const = |m: &Mono| m.keys().all(String::is_empty);
        if !only_const(&c.n) || !only_const(&c.d) {
            return None;
        }
        let n = c.n.get(&String::new()).cloned().unwrap_or_default();
        let d = c.d.get(&String::new()).cloned().unwrap_or_else(BigInt::one);
        (!d.is_zero()).then(|| rat(n, d))
    }
    fn approx(&self, c: &Rf) -> Option<f64> {
        self.as_rat(c).map(|r| rat_f64(&r))
    }
    fn show(&self, c: &Rf) -> String {
        let c = rf_cancel(c, &self.params);
        let n = poly_desc(&c.n, &self.params);
        let d = poly_desc(&c.d, &self.params);
        if d == "1" {
            return n;
        }
        let num = if has_sign_after_first(&n) {
            format!("({n})")
        } else {
            n
        };
        let den = if has_sign_after_first(&d) || d.contains('*') {
            format!("({d})")
        } else {
            d
        };
        format!("{num}/{den}")
    }
    fn division_free(&self) -> bool {
        self.params.len() > 1
    }
}

/// Plain floats, with zero meaning "small next to the entries typed".
pub(super) struct F64Field {
    scale: StdCell<f64>,
}

impl F64Field {
    pub(super) fn new() -> Self {
        F64Field {
            scale: StdCell::new(0.0),
        }
    }
    fn eps(&self) -> f64 {
        let s = self.scale.get();
        1e-10 * if s > 0.0 { s } else { 1.0 }
    }
}

impl Field for F64Field {
    type C = f64;
    fn zero(&self) -> f64 {
        0.0
    }
    fn one(&self) -> f64 {
        1.0
    }
    fn add(&self, a: &f64, b: &f64) -> f64 {
        a + b
    }
    fn sub(&self, a: &f64, b: &f64) -> f64 {
        a - b
    }
    fn mul(&self, a: &f64, b: &f64) -> f64 {
        a * b
    }
    fn div(&self, a: &f64, b: &f64) -> f64 {
        a / b
    }
    fn neg(&self, a: &f64) -> f64 {
        -a
    }
    fn is_zero(&self, a: &f64) -> bool {
        a.abs() <= self.eps()
    }
    fn str(&self, a: &f64) -> String {
        jsnum::to_string(*a)
    }
}

impl Cells for F64Field {
    fn lift(&self, r: &Rat) -> f64 {
        rat_f64(r)
    }
    fn as_rat(&self, c: &f64) -> Option<Rat> {
        jsnum::is_safe_integer(*c).then(|| ri(*c as i64))
    }
    fn approx(&self, c: &f64) -> Option<f64> {
        Some(*c)
    }
    fn float_leaf(&self, v: f64) -> Option<f64> {
        if !v.is_finite() {
            return None;
        }
        self.scale.set(self.scale.get().max(v.abs()));
        Some(v)
    }
    fn is_float(&self) -> bool {
        true
    }
    fn root(&self, r: &Rat) -> Option<f64> {
        Some(rat_f64(r).sqrt())
    }
    fn sqrt(&self, c: &f64) -> Option<f64> {
        if *c < -self.eps() {
            return None;
        }
        Some(c.max(0.0).sqrt())
    }
    fn weight(&self, c: &f64) -> f64 {
        c.abs()
    }
}

/// A float as the rational it was typed as: `0.1` is 1/10, a stored 1/3 is 1/3. None past what reads cleanly.
pub(super) fn rat_of_f64(v: f64) -> Option<Rat> {
    if !v.is_finite() {
        return None;
    }
    if jsnum::is_safe_integer(v) {
        return Some(ri(v as i64));
    }
    for q in 2..=1000_i64 {
        let p = (v * q as f64).round();
        if p.abs() < 9e15 && p / q as f64 == v {
            return Some(rat(BigInt::from(p as i64), BigInt::from(q)));
        }
    }
    // the shortest decimal JS prints, read as written
    let s = jsnum::to_string(v);
    let (mant, exp) = match s.split_once('e') {
        Some((m, e)) => (m, e.trim_start_matches('+').parse::<i64>().ok()?),
        None => (s.as_str(), 0),
    };
    let (neg, mant) = match mant.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mant),
    };
    let (whole, frac) = mant.split_once('.').unwrap_or((mant, ""));
    let digits: BigInt = format!("{whole}{frac}").parse().ok()?;
    let scale = exp - frac.len() as i64;
    if scale.abs() > 30 {
        return None;
    }
    let ten = BigInt::from(10).pow(scale.unsigned_abs() as u32);
    let digits = if neg { -digits } else { digits };
    Some(if scale >= 0 {
        rat(digits * ten, BigInt::one())
    } else {
        rat(digits, ten)
    })
}

fn constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "tau" => std::f64::consts::TAU,
        _ => return None,
    })
}

pub(super) struct Ev<'a, 'o, F: Cells> {
    pub f: &'a F,
    pub opts: &'a MatrixOptions<'o>,
    pub fmt: Fmt,
}

impl<F: Cells> Ev<'_, '_, F> {
    /// A number leaf: exact fields read it as typed, floats as it is.
    fn number(&self, v: f64) -> XR<F::C> {
        if self.f.is_float() {
            return self.f.float_leaf(v).ok_or(XFail::NotExact);
        }
        let r = rat_of_f64(v).ok_or(XFail::NotExact)?;
        Ok(self.f.lift(&r))
    }

    pub(super) fn eval(&self, node: &Node, in_array: bool) -> XR<X<F::C>> {
        let f = self.f;
        // floats read an entry like `sin(30)` or `2^0.5` as the calculator does
        if f.is_float()
            && !matches!(
                node,
                Node::Constant(_) | Node::Symbol(_) | Node::Parenthesis(_)
            )
            && let Some(scalar) = self.opts.scalar
            && !has_matrix_part(node).map_err(|_| XFail::NotExact)?
        {
            let v = scalar(&to_string(node, &PrintOptions::default())).ok_or(XFail::NotExact)?;
            return self.number(v).map(X::Num);
        }
        match node {
            Node::Parenthesis(c) => self.eval(c, in_array),
            Node::Constant(Constant::Number { value, .. }) => self.number(*value).map(X::Num),
            Node::Symbol(name) => {
                if let Some(v) = self.opts.variables.get(name) {
                    return self.number(*v).map(X::Num);
                }
                if (in_array || loose_unknown(name))
                    && let Some(c) = f.param(name)
                {
                    return Ok(X::Num(c));
                }
                match constant(name) {
                    Some(v) if f.is_float() => self.number(v).map(X::Num),
                    _ => Err(XFail::NotExact),
                }
            }
            Node::Array(items) => self.array(items),
            Node::Function { callee, args } => {
                let name = callee_name(callee).map_err(|_| XFail::NotExact)?;
                if let Some(k) = self.opts.variables.get(name)
                    && args.len() == 1
                {
                    let k = self.number(*k)?;
                    return self.mul(X::Num(k), self.eval(&args[0], in_array)?);
                }
                let key = name.to_lowercase();
                if key == "sqrt" && args.len() == 1 {
                    let c = self.scalar(self.eval(&args[0], in_array)?)?;
                    return f.sqrt(&c).map(X::Num).ok_or(XFail::NotExact);
                }
                if !super::is_call(&key) {
                    return Err(XFail::NotExact);
                }
                let values = args
                    .iter()
                    .map(|a| self.eval(a, in_array))
                    .collect::<XR<Vec<_>>>()?;
                super::functions::call(self, &key, values)
            }
            Node::Operator { op, args, .. } => {
                let (l, r) = (args.first(), args.get(1));
                if *op == Op::UnaryMinus
                    && let Some(l) = l
                {
                    return self.mul(X::Num(f.neg(&f.one())), self.eval(l, in_array)?);
                }
                if *op == Op::UnaryPlus
                    && let Some(l) = l
                {
                    return self.eval(l, in_array);
                }
                let (Some(l), Some(r)) = (l, r) else {
                    return Err(XFail::NotExact);
                };
                if *op == Op::Pow && is_t(r) {
                    return Ok(X::Grid(super::algebra::transpose(
                        &self.grid(self.eval(l, in_array)?)?,
                    )));
                }
                let a = self.eval(l, in_array)?;
                let b = self.eval(r, in_array)?;
                match op {
                    Op::Add => self.add(a, b, false),
                    Op::Subtract => self.add(a, b, true),
                    Op::Multiply => self.mul(a, b),
                    Op::Divide => match b {
                        X::Grid(_) => {
                            fail("can't divide by a matrix; multiply by its inverse instead")
                        }
                        X::Done(_) => Err(XFail::NotExact),
                        X::Num(k) if f.is_zero(&k) => fail("can't divide by 0"),
                        X::Num(k) => self.mul(a, X::Num(f.div(&f.one(), &k))),
                    },
                    Op::Pow => self.power(a, b),
                    // `A .* B` and `A ./ B` go entry by entry; a number scales
                    Op::DotMultiply | Op::DotDivide => match (a, b) {
                        (X::Grid(x), X::Grid(y)) => {
                            if x.len() != y.len() || x[0].len() != y[0].len() {
                                return fail(format!(
                                    "{} needs matrices of the same size",
                                    op.symbol()
                                ));
                            }
                            let mut out = x;
                            for (row, other) in out.iter_mut().zip(&y) {
                                for (c, d) in row.iter_mut().zip(other) {
                                    if *op == Op::DotMultiply {
                                        *c = f.mul(c, d);
                                    } else if f.is_zero(d) {
                                        return fail("can't divide by 0");
                                    } else {
                                        *c = f.div(c, d);
                                    }
                                }
                            }
                            Ok(X::Grid(out))
                        }
                        (_, X::Num(k)) if *op == Op::DotDivide && f.is_zero(&k) => {
                            fail("can't divide by 0")
                        }
                        (a, X::Num(k)) if *op == Op::DotDivide => {
                            self.mul(a, X::Num(f.div(&f.one(), &k)))
                        }
                        (a, b) if *op == Op::DotMultiply => self.mul(a, b),
                        _ => Err(XFail::NotExact),
                    },
                    _ => Err(XFail::NotExact),
                }
            }
            _ => Err(XFail::NotExact),
        }
    }

    pub(super) fn scalar(&self, a: X<F::C>) -> XR<F::C> {
        match a {
            X::Num(c) => Ok(c),
            X::Grid(mut g) if g.len() == 1 && g[0].len() == 1 => Ok(g.remove(0).remove(0)),
            _ => Err(XFail::NotExact),
        }
    }

    /// A number is a 1×1 matrix wherever an operation needs one.
    pub(super) fn grid(&self, a: X<F::C>) -> XR<G<F::C>> {
        match a {
            X::Num(c) => Ok(vec![vec![c]]),
            X::Grid(g) => Ok(g),
            X::Done(_) => Err(XFail::NotExact),
        }
    }

    pub(super) fn square(&self, a: X<F::C>, what: &str) -> XR<G<F::C>> {
        let g = self.grid(a)?;
        if g.len() != g[0].len() {
            return fail(format!(
                "{what} needs a square matrix (this is {})",
                size(&g)
            ));
        }
        Ok(g)
    }

    pub(super) fn int_of(&self, c: &F::C) -> Option<i64> {
        let r = self.f.as_rat(c)?;
        r.d.is_one().then(|| r.n.to_i64()).flatten()
    }

    fn array(&self, items: &[Node]) -> XR<X<F::C>> {
        let items = items
            .iter()
            .map(|i| self.eval(i, true))
            .collect::<XR<Vec<_>>>()?;
        if items.is_empty() {
            return fail("empty matrix");
        }
        if items.iter().all(|v| matches!(v, X::Num(_))) {
            let row = items
                .into_iter()
                .filter_map(|v| match v {
                    X::Num(c) => Some(c),
                    _ => None,
                })
                .collect();
            return Ok(X::Grid(vec![row]));
        }
        if items
            .iter()
            .all(|v| matches!(v, X::Grid(g) if g.len() == 1))
        {
            let rows: G<F::C> = items
                .into_iter()
                .filter_map(|v| match v {
                    X::Grid(mut g) => Some(g.remove(0)),
                    _ => None,
                })
                .collect();
            let width = rows[0].len();
            if rows.iter().any(|r| r.len() != width || width == 0) {
                return fail("rows differ in length");
            }
            return Ok(X::Grid(rows));
        }
        fail("rows differ in length")
    }

    fn add(&self, a: X<F::C>, b: X<F::C>, minus: bool) -> XR<X<F::C>> {
        let f = self.f;
        let op = |x: &F::C, y: &F::C| if minus { f.sub(x, y) } else { f.add(x, y) };
        let verb = if minus { "subtract" } else { "add" };
        match (a, b) {
            (X::Num(x), X::Num(y)) => Ok(X::Num(op(&x, &y))),
            (X::Grid(a), X::Grid(b)) => {
                if a.len() != b.len() || a[0].len() != b[0].len() {
                    return fail(format!(
                        "can't {verb} {} and {}: sizes must match",
                        size(&a),
                        size(&b)
                    ));
                }
                Ok(X::Grid(
                    a.iter()
                        .zip(&b)
                        .map(|(r, s)| r.iter().zip(s).map(|(v, w)| op(v, w)).collect())
                        .collect(),
                ))
            }
            (X::Done(_), _) | (_, X::Done(_)) => Err(XFail::NotExact),
            _ => fail(format!("can't {verb} a number and a matrix")),
        }
    }

    pub(super) fn mul(&self, a: X<F::C>, b: X<F::C>) -> XR<X<F::C>> {
        let f = self.f;
        Ok(match (a, b) {
            (X::Num(x), X::Num(y)) => X::Num(f.mul(&x, &y)),
            (X::Num(x), X::Grid(g)) | (X::Grid(g), X::Num(x)) => {
                X::Grid(super::algebra::scale(f, &g, &x))
            }
            (X::Grid(a), X::Grid(b)) => X::Grid(super::algebra::mul(f, &a, &b)?),
            _ => return Err(XFail::NotExact),
        })
    }

    fn power(&self, a: X<F::C>, e: X<F::C>) -> XR<X<F::C>> {
        let f = self.f;
        let e = match e {
            X::Num(e) => e,
            X::Grid(_) => return fail("can't raise to a matrix power"),
            X::Done(_) => return Err(XFail::NotExact),
        };
        let Some(k) = self.int_of(&e) else {
            return match a {
                X::Grid(_) if f.as_rat(&e).is_some() => {
                    fail("a matrix power must be a whole number")
                }
                X::Num(x) if f.is_float() => {
                    let (Some(x), Some(e)) = (f.approx(&x), f.approx(&e)) else {
                        return Err(XFail::NotExact);
                    };
                    self.number(crate::eval::ops::js_pow(x, e)).map(X::Num)
                }
                _ => Err(XFail::NotExact),
            };
        };
        if k.abs() > 256 {
            return Err(XFail::NotExact);
        }
        if let X::Num(x) = &a {
            if k < 0 && f.is_zero(x) {
                return Err(XFail::NotExact);
            }
            let base = if k < 0 { f.div(&f.one(), x) } else { x.clone() };
            let mut out = f.one();
            for _ in 0..k.abs() {
                out = f.mul(&out, &base);
            }
            return Ok(X::Num(out));
        }
        let sq = self.square(a, "a power")?;
        Ok(X::Grid(super::algebra::power(f, &sq, k)?))
    }
}

/// The unknown names inside matrix literals, in the order they first appear; None when a name outside any matrix
/// isn't known (`det(A)` with no A is still being typed, not a symbol).
/// An unknown that counts outside a matrix's brackets too, as in `det(A - λI)` or `x*[1 2]`: one letter (not `e` or
/// `i`) or `lambda`. Longer unknown names there are still a line the matrix reader can't answer.
pub(super) fn loose_unknown(name: &str) -> bool {
    let mut chars = name.chars();
    let one = matches!((chars.next(), chars.next()), (Some(c), None) if c.is_alphabetic() && c != 'e' && c != 'i');
    one || name == "lambda"
}

fn params_of(node: &Node, opts: &MatrixOptions<'_>) -> Option<Vec<String>> {
    fn has_array(node: &Node) -> bool {
        match node {
            Node::Array(_) => true,
            Node::Function { args, .. } | Node::Operator { args, .. } => args.iter().any(has_array),
            Node::Parenthesis(c) => has_array(c),
            _ => false,
        }
    }
    // `det(A)` before A is stored is still being typed: a lone letter is an unknown only beside a written matrix
    let loose = has_array(node);
    let walk_loose = move |name: &str| loose && loose_unknown(name);
    fn walk(
        node: &Node,
        in_array: bool,
        opts: &MatrixOptions<'_>,
        loose: &dyn Fn(&str) -> bool,
        out: &mut Vec<String>,
    ) -> bool {
        match node {
            Node::Symbol(name) => {
                if opts.variables.contains_key(name) || is_t(node) {
                    return true;
                }
                if !(in_array || loose(name))
                    || matches!(
                        name.as_str(),
                        "pi" | "e" | "tau" | "i" | "ans" | "Infinity" | "NaN"
                    )
                    || opts.functions.iter().any(|f| f == name)
                {
                    return false;
                }
                if !out.contains(name) {
                    out.push(name.clone());
                }
                true
            }
            Node::Array(items) => items.iter().all(|i| walk(i, true, opts, loose, out)),
            Node::Function { args, .. } => args.iter().all(|a| walk(a, in_array, opts, loose, out)),
            Node::Operator { args, .. } => args.iter().all(|a| walk(a, in_array, opts, loose, out)),
            Node::Parenthesis(c) => walk(c, in_array, opts, loose, out),
            Node::Constant(_) => true,
            _ => false,
        }
    }
    let mut out = Vec::new();
    walk(node, false, opts, &walk_loose, &mut out).then_some(out)
}

fn cells_of(node: &Node) -> usize {
    match node {
        Node::Array(items) => items.iter().map(cells_of).sum::<usize>().max(1),
        Node::Function { args, .. } | Node::Operator { args, .. } => {
            args.iter().map(cells_of).sum()
        }
        Node::Parenthesis(c) => cells_of(c),
        _ => 0,
    }
}

/// An answer from the generic evaluator.
#[derive(Debug)]
pub(super) enum Exact {
    Message(String),
    /// One number or matrix: its exact text (`[1/2 sqrt(3); 0 k]`), a literal to store, and its value as floats when it
    /// has one. `rich` when some entry is a fraction the floats round, an irrational, or a symbol.
    Value {
        text: String,
        literal: Option<String>,
        approx: Option<super::M>,
        rich: bool,
    },
    /// Anything else: eigenvalues, a span, P and D.
    Worded(Answer),
}

/// A cell written to read back in a spaced row: `k^2-6`, `2*sqrt(3)`, `1/2`.
pub(super) fn compact(s: &str) -> String {
    s.split_whitespace().collect()
}

fn finish<F: Cells>(f: &F, v: X<F::C>) -> Exact {
    let rich_cell = |c: &F::C| {
        !f.is_float()
            && match f.as_rat(c) {
                Some(r) => !shows_whole(&r),
                None => true,
            }
    };
    // a row times a column is a number
    let v = match v {
        X::Grid(mut g) if g.len() == 1 && g[0].len() == 1 => X::Num(g.remove(0).remove(0)),
        v => v,
    };
    match v {
        X::Done(a) => Exact::Worded(a),
        X::Num(c) => Exact::Value {
            text: f.show(&c),
            literal: None,
            approx: f.approx(&c).map(super::M::Num),
            rich: rich_cell(&c),
        },
        X::Grid(g) => {
            let text = format!(
                "[{}]",
                g.iter()
                    .map(|r| r
                        .iter()
                        .map(|c| compact(&f.show(c)))
                        .collect::<Vec<_>>()
                        .join(" "))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            let literal = format!(
                "[{}]",
                g.iter()
                    .map(|r| format!(
                        "[{}]",
                        r.iter().map(|c| f.show(c)).collect::<Vec<_>>().join(", ")
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let approx: Option<G<f64>> = g
                .iter()
                .map(|r| r.iter().map(|c| f.approx(c)).collect())
                .collect();
            Exact::Value {
                text,
                literal: (!f.is_float()).then_some(literal),
                approx: approx.map(super::M::Grid),
                rich: g.iter().flatten().any(rich_cell),
            }
        }
    }
}

fn attempt<F: Cells>(f: &F, node: &Node, opts: &MatrixOptions<'_>) -> Option<Option<Exact>> {
    let ev = Ev {
        f,
        opts,
        fmt: Fmt::new(opts.sig_figs),
    };
    let out = guarded(|| Some(ev.eval(node, false)));
    match out {
        None => Some(None), // a division by zero inside the field: no exact answer
        Some(Ok(v)) => Some(Some(finish(f, v))),
        Some(Err(XFail::Matrix(m))) => Some(Some(Exact::Message(m))),
        Some(Err(XFail::NotExact)) => None,
    }
}

/// The answer to a matrix line from the generic evaluator, or None when no field holds it. `floats` lets plain floats
/// take a line no exact field holds (for the functions only this evaluator knows).
pub(super) fn exact_answer(node: &Node, opts: &MatrixOptions<'_>, floats: bool) -> Option<Exact> {
    if cells_of(node) <= MAX_CELLS {
        if let Some(out) = attempt(&RatField, node, opts) {
            return out;
        }
        if let Some(out) = attempt(&RadField::new(), node, opts) {
            return out;
        }
        if let Some(params) = params_of(node, opts)
            && !params.is_empty()
            && let Some(out) = attempt(&RfField { params }, node, opts)
        {
            return out;
        }
    }
    if floats {
        return attempt(&F64Field::new(), node, opts).flatten();
    }
    None
}

fn steps_in<F: Cells>(
    f: &F,
    key: &str,
    args: &[Node],
    opts: &MatrixOptions<'_>,
    bar: Option<usize>,
) -> Option<Vec<crate::types::MatrixStep>> {
    let ev = Ev {
        f,
        opts,
        fmt: Fmt::new(opts.sig_figs),
    };
    let run = || {
        let grids = args
            .iter()
            .map(|a| ev.eval(a, false).and_then(|x| ev.grid(x)))
            .collect::<XR<Vec<_>>>()?;
        super::steps::run(&ev, key, grids, bar)
    };
    match guarded(|| Some(run())) {
        None => Some(Vec::new()),
        Some(Ok(s)) => Some(s),
        Some(Err(XFail::Matrix(_))) => Some(Vec::new()),
        Some(Err(XFail::NotExact)) => None,
    }
}

/// The row operations behind a line like `rref(A)` or `A \ b`, in the most exact field that holds its entries.
/// `bar` is where a typed `[A | b]` divides its columns.
pub(super) fn steps(
    node: &Node,
    opts: &MatrixOptions<'_>,
    bar: Option<usize>,
) -> Vec<crate::types::MatrixStep> {
    let Node::Function { callee, args } = node else {
        return Vec::new();
    };
    let Ok(name) = callee_name(callee) else {
        return Vec::new();
    };
    let key = name.to_lowercase();
    if !super::steps::has_steps(&key) || cells_of(node) > MAX_CELLS {
        return Vec::new();
    }
    if let Some(out) = steps_in(&RatField, &key, args, opts, bar) {
        return out;
    }
    if let Some(out) = steps_in(&RadField::new(), &key, args, opts, bar) {
        return out;
    }
    if let Some(params) = params_of(node, opts)
        && !params.is_empty()
        && let Some(out) = steps_in(&RfField { params }, &key, args, opts, bar)
    {
        return out;
    }
    steps_in(&F64Field::new(), &key, args, opts, bar).unwrap_or_default()
}
