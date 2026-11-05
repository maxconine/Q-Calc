//! Exact matrix arithmetic: the same lines the float reader takes, worked out over the most exact field that holds
//! every entry, tried in turn: rationals (`[1/3 2; 0.5 1]`), sums of square roots (`[sqrt(2) 1; 1 sqrt(2)]`), then
//! rational functions in the unknown names inside a matrix (`[k 2; 3 k]`).
//!
//! A line this can't do exactly (an entry like `sin(30)` or `pi`, a function it doesn't know) is `NotExact`, and the
//! float reader's answer stands alone. Over the rational functions, a pivot that is a nonzero expression counts as
//! nonzero, so `rref([k 1; 2 1])` is the identity, as it is for every k but 2.

use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::{MatrixOptions, callee_name, is_call, is_t};
use crate::exact::{
    Field, Mono, Rad, RadField, Rat, RatField, Rf, RfField, guarded, has_sign_after_first,
    join_signed, key_exps, mono_key, pconst, rat, rf_cancel, sqrt_rat,
};
use crate::jsnum;
use crate::syntax::{Constant, Node, Op};

/// Past this many entries the exact pass is skipped; the float answer is shown alone.
const MAX_CELLS: usize = 144;
/// A division-free determinant (rational functions) expands over column subsets: 2^n states.
const MAX_SYMBOLIC_DET: usize = 10;
const MAX_POWER: i64 = 256;
const MAX_IDENTITY: i64 = 20;

/// A value: a scalar, a grid, or a square root that only a final answer may be (`norm([1 2 3])` is √14).
#[derive(Clone, Debug)]
pub(super) enum X<C> {
    Num(C),
    Grid(Vec<Vec<C>>),
    Surd(String, f64),
}

pub(super) enum XFail {
    /// Not something this field can hold; the next field, or the float reader, takes it.
    NotExact,
    /// A real error, worded as the float reader words it.
    Matrix(String),
}

type XR<T> = Result<T, XFail>;

fn fail<T>(msg: impl Into<String>) -> XR<T> {
    Err(XFail::Matrix(msg.into()))
}

/// A field matrix cells live in, plus how a cell is read from a leaf and read back as a number.
pub(super) trait Cells: Field {
    fn lift(&self, r: &Rat) -> Self::C;
    fn param(&self, _name: &str) -> Option<Self::C> {
        None
    }
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

pub(super) fn rat_f64(r: &Rat) -> f64 {
    match (r.n.to_f64(), r.d.to_f64()) {
        (Some(n), Some(d)) if n.is_finite() && d.is_finite() => n / d,
        _ => {
            // a huge numerator or denominator: scale both down by the same power of ten
            let shift = r.n.to_string().len().max(r.d.to_string().len()).saturating_sub(300) as u32;
            let p = BigInt::from(10).pow(shift);
            (&r.n / &p).to_f64().unwrap_or(f64::NAN) / (&r.d / &p).to_f64().unwrap_or(f64::NAN)
        }
    }
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
        let only_const = |m: &crate::exact::Mono| m.keys().all(String::is_empty);
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
        let num = if has_sign_after_first(&n) { format!("({n})") } else { n };
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

/// A float as the rational it was typed as: `0.1` is 1/10, a stored 1/3 is 1/3. None past what reads cleanly.
pub(super) fn rat_of_f64(v: f64) -> Option<Rat> {
    if !v.is_finite() {
        return None;
    }
    if jsnum::is_safe_integer(v) {
        return Some(rat(BigInt::from(v as i64), BigInt::one()));
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

fn size<C>(g: &[Vec<C>]) -> String {
    format!("{}×{}", g.len(), g.first().map_or(0, Vec::len))
}

fn as_grid<C>(a: X<C>) -> XR<Vec<Vec<C>>> {
    match a {
        X::Num(c) => Ok(vec![vec![c]]),
        X::Grid(g) => Ok(g),
        X::Surd(..) => Err(XFail::NotExact),
    }
}

fn square<C>(a: X<C>, what: &str) -> XR<Vec<Vec<C>>> {
    let g = as_grid(a)?;
    if g.len() != g[0].len() {
        return fail(format!(
            "{what} needs a square matrix (this is {})",
            size(&g)
        ));
    }
    Ok(g)
}

struct Ev<'a, 'o, F: Cells> {
    f: &'a F,
    opts: &'a MatrixOptions<'o>,
}

impl<F: Cells> Ev<'_, '_, F> {
    fn scalar(&self, a: X<F::C>) -> XR<F::C> {
        match a {
            X::Num(c) => Ok(c),
            _ => Err(XFail::NotExact),
        }
    }

    fn int_of(&self, c: &F::C) -> Option<i64> {
        let r = self.f.as_rat(c)?;
        r.d.is_one().then(|| r.n.to_i64()).flatten()
    }

    fn eval(&self, node: &Node, in_array: bool) -> XR<X<F::C>> {
        let f = self.f;
        match node {
            Node::Parenthesis(c) => self.eval(c, in_array),
            Node::Constant(Constant::Number { value, .. }) => {
                let r = rat_of_f64(*value).ok_or(XFail::NotExact)?;
                Ok(X::Num(f.lift(&r)))
            }
            Node::Symbol(name) => {
                if let Some(v) = self.opts.variables.get(name) {
                    let r = rat_of_f64(*v).ok_or(XFail::NotExact)?;
                    return Ok(X::Num(f.lift(&r)));
                }
                if in_array && let Some(c) = f.param(name) {
                    return Ok(X::Num(c));
                }
                Err(XFail::NotExact)
            }
            Node::Array(items) => self.array(items),
            Node::Function { callee, args } => {
                let name = callee_name(callee).map_err(|_| XFail::NotExact)?;
                if let Some(k) = self.opts.variables.get(name)
                    && args.len() == 1
                {
                    let k = f.lift(&rat_of_f64(*k).ok_or(XFail::NotExact)?);
                    return self.mul(X::Num(k), self.eval(&args[0], in_array)?);
                }
                let key = name.to_lowercase();
                if key == "sqrt" && args.len() == 1 {
                    let c = self.scalar(self.eval(&args[0], in_array)?)?;
                    let r = f.as_rat(&c).ok_or(XFail::NotExact)?;
                    if r.n.is_negative() {
                        return Err(XFail::NotExact);
                    }
                    return f.root(&r).map(X::Num).ok_or(XFail::NotExact);
                }
                if !is_call(&key) {
                    return Err(XFail::NotExact);
                }
                let want = if key == "dot" || key == "cross" { 2 } else { 1 };
                if args.len() != want {
                    return Err(XFail::NotExact);
                }
                let mut values = args
                    .iter()
                    .map(|a| self.eval(a, in_array))
                    .collect::<XR<Vec<_>>>()?;
                let first = values.remove(0);
                let second = values.pop();
                self.call(&key, first, second)
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
                    return Ok(self.transpose(self.eval(l, in_array)?));
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
                        X::Surd(..) => Err(XFail::NotExact),
                        X::Num(k) if f.is_zero(&k) => fail("can't divide by 0"),
                        X::Num(k) => self.mul(a, X::Num(f.div(&f.one(), &k))),
                    },
                    Op::Pow => self.power(a, b),
                    _ => Err(XFail::NotExact),
                }
            }
            _ => Err(XFail::NotExact),
        }
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
                .map(|v| match v {
                    X::Num(c) => c,
                    _ => unreachable!(),
                })
                .collect();
            return Ok(X::Grid(vec![row]));
        }
        if items.iter().all(|v| matches!(v, X::Grid(g) if g.len() == 1)) {
            let rows: Vec<Vec<F::C>> = items
                .into_iter()
                .map(|v| match v {
                    X::Grid(mut g) => g.remove(0),
                    _ => unreachable!(),
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
            (X::Surd(..), _) | (_, X::Surd(..)) => Err(XFail::NotExact),
            _ => fail(format!("can't {verb} a number and a matrix")),
        }
    }

    fn scale(&self, g: &[Vec<F::C>], k: &F::C) -> Vec<Vec<F::C>> {
        g.iter()
            .map(|r| r.iter().map(|v| self.f.mul(v, k)).collect())
            .collect()
    }

    fn mul_grids(&self, a: &[Vec<F::C>], b: &[Vec<F::C>]) -> XR<Vec<Vec<F::C>>> {
        let f = self.f;
        let inner = a[0].len();
        if inner != b.len() {
            let hint = if a.len() == b.len() && inner == b[0].len() {
                "; for entry by entry, use dot for vectors"
            } else {
                ""
            };
            return fail(format!(
                "can't multiply {} by {}: columns of the first must equal rows of the second{hint}",
                size(a),
                size(b)
            ));
        }
        Ok(a.iter()
            .map(|r| {
                (0..b[0].len())
                    .map(|j| {
                        r.iter()
                            .enumerate()
                            .fold(f.zero(), |s, (k, v)| f.add(&s, &f.mul(v, &b[k][j])))
                    })
                    .collect()
            })
            .collect())
    }

    fn mul(&self, a: X<F::C>, b: X<F::C>) -> XR<X<F::C>> {
        Ok(match (a, b) {
            (X::Num(x), X::Num(y)) => X::Num(self.f.mul(&x, &y)),
            (X::Num(x), X::Grid(g)) | (X::Grid(g), X::Num(x)) => X::Grid(self.scale(&g, &x)),
            (X::Grid(a), X::Grid(b)) => X::Grid(self.mul_grids(&a, &b)?),
            _ => return Err(XFail::NotExact),
        })
    }

    fn transpose(&self, a: X<F::C>) -> X<F::C> {
        match a {
            X::Grid(g) => X::Grid(
                (0..g[0].len())
                    .map(|j| g.iter().map(|r| r[j].clone()).collect())
                    .collect(),
            ),
            other => other,
        }
    }

    fn identity(&self, n: usize) -> Vec<Vec<F::C>> {
        (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| if i == j { self.f.one() } else { self.f.zero() })
                    .collect()
            })
            .collect()
    }

    fn det_of(&self, g: &[Vec<F::C>]) -> XR<F::C> {
        if self.f.division_free() {
            return det_expand(self.f, g).ok_or(XFail::NotExact);
        }
        Ok(reduce(self.f, g, false).det)
    }

    fn inverse(&self, g: &[Vec<F::C>]) -> XR<Vec<Vec<F::C>>> {
        let f = self.f;
        let n = g.len();
        if f.division_free() {
            // adjugate over the determinant: no division until the very end
            let d = self.det_of(g)?;
            if f.is_zero(&d) {
                return fail("no inverse (determinant is 0)");
            }
            let mut out = vec![vec![f.zero(); n]; n];
            for (i, row) in out.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    let c = cofactor(f, g, j, i).ok_or(XFail::NotExact)?;
                    *cell = f.div(&c, &d);
                }
            }
            return Ok(out);
        }
        let aug: Vec<Vec<F::C>> = g
            .iter()
            .zip(self.identity(n))
            .map(|(r, e)| r.iter().cloned().chain(e).collect())
            .collect();
        let red = reduce(f, &aug, true);
        if red.pivots.len() < n || red.pivots[n - 1] != n - 1 {
            return fail("no inverse (determinant is 0)");
        }
        Ok(red.m.into_iter().map(|r| r[n..].to_vec()).collect())
    }

    fn vector(a: &X<F::C>) -> Option<Vec<F::C>> {
        let X::Grid(g) = a else { return None };
        if g.len() == 1 {
            return Some(g[0].clone());
        }
        if g[0].len() == 1 {
            return Some(g.iter().map(|r| r[0].clone()).collect());
        }
        None
    }

    fn call(&self, key: &str, a: X<F::C>, b: Option<X<F::C>>) -> XR<X<F::C>> {
        let f = self.f;
        match key {
            "inv" | "inverse" => match a {
                X::Num(c) if f.is_zero(&c) => fail("no inverse (it is 0)"),
                X::Num(c) => Ok(X::Num(f.div(&f.one(), &c))),
                a => Ok(X::Grid(self.inverse(&square(a, "inverse")?)?)),
            },
            "det" | "determinant" => Ok(X::Num(self.det_of(&square(a, "determinant")?)?)),
            "transpose" => Ok(self.transpose(a)),
            "trace" | "tr" => {
                let g = square(a, "trace")?;
                Ok(X::Num(
                    g.iter()
                        .enumerate()
                        .fold(f.zero(), |s, (i, r)| f.add(&s, &r[i])),
                ))
            }
            "rank" => {
                let g = as_grid(a)?;
                let r = reduce(f, &g, false).pivots.len();
                Ok(X::Num(f.lift(&rat(BigInt::from(r), BigInt::one()))))
            }
            "rref" => Ok(X::Grid(reduce(f, &as_grid(a)?, true).m)),
            "identity" | "eye" => {
                let n = self.int_of(&self.scalar(a)?);
                match n {
                    Some(n) if (1..=MAX_IDENTITY).contains(&n) => {
                        Ok(X::Grid(self.identity(n as usize)))
                    }
                    _ => fail(format!(
                        "identity needs a whole size from 1 to {MAX_IDENTITY}"
                    )),
                }
            }
            "dot" => {
                let b = b.ok_or(XFail::NotExact)?;
                let (Some(u), Some(v)) = (Self::vector(&a), Self::vector(&b)) else {
                    return fail("dot needs two vectors");
                };
                if u.len() != v.len() {
                    return fail(format!(
                        "dot needs vectors of the same length ({} and {})",
                        u.len(),
                        v.len()
                    ));
                }
                Ok(X::Num(
                    u.iter()
                        .zip(&v)
                        .fold(f.zero(), |s, (x, y)| f.add(&s, &f.mul(x, y))),
                ))
            }
            "cross" => {
                let b = b.ok_or(XFail::NotExact)?;
                let (Some(u), Some(v)) = (Self::vector(&a), Self::vector(&b)) else {
                    return fail("cross needs two vectors of length 3");
                };
                if u.len() != 3 || v.len() != 3 {
                    return fail("cross needs two vectors of length 3");
                }
                let m = |x: &F::C, y: &F::C| f.mul(x, y);
                let w = [
                    f.sub(&m(&u[1], &v[2]), &m(&u[2], &v[1])),
                    f.sub(&m(&u[2], &v[0]), &m(&u[0], &v[2])),
                    f.sub(&m(&u[0], &v[1]), &m(&u[1], &v[0])),
                ];
                let row = matches!(&a, X::Grid(g) if g.len() == 1);
                Ok(X::Grid(if row {
                    vec![w.to_vec()]
                } else {
                    w.into_iter().map(|x| vec![x]).collect()
                }))
            }
            "norm" => {
                let g = as_grid(a)?;
                let sum = g
                    .iter()
                    .flatten()
                    .fold(f.zero(), |s, v| f.add(&s, &f.mul(v, v)));
                let r = f.as_rat(&sum).ok_or(XFail::NotExact)?;
                let Rad { c, rad } = sqrt_rat(&r).ok_or(XFail::NotExact)?;
                if rad.is_one() {
                    return Ok(X::Num(f.lift(&c)));
                }
                let unit = format!("sqrt({rad})");
                let text = if c.n.is_one() && c.d.is_one() {
                    unit
                } else if c.d.is_one() {
                    format!("{}*{unit}", c.n)
                } else if c.n.is_one() {
                    format!("{unit}/{}", c.d)
                } else {
                    format!("{}*{unit}/{}", c.n, c.d)
                };
                Ok(X::Surd(text, rat_f64(&c) * rad.to_f64().unwrap_or(f64::NAN).sqrt()))
            }
            _ => Err(XFail::NotExact),
        }
    }

    fn power(&self, a: X<F::C>, e: X<F::C>) -> XR<X<F::C>> {
        let f = self.f;
        let e = match e {
            X::Num(e) => e,
            X::Grid(_) => return fail("can't raise to a matrix power"),
            X::Surd(..) => return Err(XFail::NotExact),
        };
        let Some(k) = self.int_of(&e) else {
            return match a {
                X::Grid(_) if f.as_rat(&e).is_some() => fail("a matrix power must be a whole number"),
                _ => Err(XFail::NotExact),
            };
        };
        if k.abs() > MAX_POWER {
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
        let sq = square(a, "a power")?;
        let mut base = if k < 0 { self.inverse(&sq)? } else { sq.clone() };
        let mut out = self.identity(sq.len());
        let mut k = k.abs();
        while k > 0 {
            if k % 2 == 1 {
                out = self.mul_grids(&out, &base)?;
            }
            k /= 2;
            if k > 0 {
                base = self.mul_grids(&base, &base)?;
            }
        }
        Ok(X::Grid(out))
    }
}

pub(super) struct Reduced<C> {
    pub m: Vec<Vec<C>>,
    pub pivots: Vec<usize>,
    pub det: C,
}

/// Row echelon (or, with `full`, reduced row echelon) by exact elimination, and the determinant it finds on the way.
pub(super) fn reduce<F: Field>(f: &F, src: &[Vec<F::C>], full: bool) -> Reduced<F::C> {
    let mut m: Vec<Vec<F::C>> = src.to_vec();
    let rows = m.len();
    let cols = m.first().map_or(0, Vec::len);
    let mut pivots = Vec::new();
    let mut det = f.one();
    let (mut r, mut c) = (0, 0);
    while c < cols && r < rows {
        let Some(p) = (r..rows).find(|&i| !f.is_zero(&m[i][c])) else {
            det = f.zero();
            c += 1;
            continue;
        };
        if p != r {
            m.swap(p, r);
            det = f.neg(&det);
        }
        let piv = m[r][c].clone();
        det = f.mul(&det, &piv);
        if full {
            m[r] = m[r].iter().map(|v| f.div(v, &piv)).collect();
        }
        let start = if full { 0 } else { r + 1 };
        for i in start..rows {
            if i == r || f.is_zero(&m[i][c]) {
                continue;
            }
            let fac = f.div(&m[i][c], &m[r][c]);
            let pivot_row = m[r].clone();
            m[i] = m[i]
                .iter()
                .zip(&pivot_row)
                .enumerate()
                .map(|(k, (v, p))| if k == c { f.zero() } else { f.sub(v, &f.mul(&fac, p)) })
                .collect();
        }
        pivots.push(c);
        r += 1;
        c += 1;
    }
    if r < rows {
        det = f.zero();
    }
    Reduced { m, pivots, det }
}

/// The determinant by expansion over column subsets: no division, so a polynomial stays a polynomial.
pub(super) fn det_expand<F: Field>(f: &F, g: &[Vec<F::C>]) -> Option<F::C> {
    let n = g.len();
    if n == 0 {
        return Some(f.one());
    }
    if n > MAX_SYMBOLIC_DET {
        return None;
    }
    let mut dp: Vec<Option<F::C>> = vec![None; 1 << n];
    dp[0] = Some(f.one());
    for mask in 0..(1usize << n) {
        let Some(cur) = dp[mask].clone() else { continue };
        if f.is_zero(&cur) {
            continue;
        }
        let row = mask.count_ones() as usize;
        if row == n {
            continue;
        }
        for j in 0..n {
            if mask & (1 << j) != 0 || f.is_zero(&g[row][j]) {
                continue;
            }
            // each column already used to the right of j is one inversion
            let inversions = (mask >> (j + 1)).count_ones();
            let term = f.mul(&cur, &g[row][j]);
            let term = if inversions % 2 == 1 { f.neg(&term) } else { term };
            let slot = &mut dp[mask | (1 << j)];
            *slot = Some(match slot.take() {
                Some(s) => f.add(&s, &term),
                None => term,
            });
        }
    }
    Some(dp[(1 << n) - 1].clone().unwrap_or_else(|| f.zero()))
}

/// `(-1)^(i+j)` times the minor that drops row i and column j.
pub(super) fn cofactor<F: Field>(f: &F, g: &[Vec<F::C>], i: usize, j: usize) -> Option<F::C> {
    let minor: Vec<Vec<F::C>> = g
        .iter()
        .enumerate()
        .filter(|(r, _)| *r != i)
        .map(|(_, row)| {
            row.iter()
                .enumerate()
                .filter(|(c, _)| *c != j)
                .map(|(_, v)| v.clone())
                .collect()
        })
        .collect();
    let d = det_expand(f, &minor)?;
    Some(if (i + j) % 2 == 1 { f.neg(&d) } else { d })
}

/// The unknown names inside matrix literals, in the order they first appear; None when a name outside any matrix
/// isn't known (`det(A)` with no A is still being typed, not a symbol).
fn params_of(node: &Node, opts: &MatrixOptions<'_>) -> Option<Vec<String>> {
    fn walk(node: &Node, in_array: bool, opts: &MatrixOptions<'_>, out: &mut Vec<String>) -> bool {
        match node {
            Node::Symbol(name) => {
                if opts.variables.contains_key(name) || is_t(node) {
                    return true;
                }
                if !in_array
                    || matches!(name.as_str(), "pi" | "e" | "tau" | "i" | "ans" | "Infinity" | "NaN")
                    || opts.functions.iter().any(|f| f == name)
                {
                    return false;
                }
                if !out.contains(name) {
                    out.push(name.clone());
                }
                true
            }
            Node::Array(items) => items.iter().all(|i| walk(i, true, opts, out)),
            Node::Function { args, .. } => args.iter().all(|a| walk(a, in_array, opts, out)),
            Node::Operator { args, .. } => args.iter().all(|a| walk(a, in_array, opts, out)),
            Node::Parenthesis(c) => walk(c, in_array, opts, out),
            Node::Constant(_) => true,
            _ => false,
        }
    }
    let mut out = Vec::new();
    walk(node, false, opts, &mut out).then_some(out)
}

fn cells_of(node: &Node) -> usize {
    match node {
        Node::Array(items) => items.iter().map(cells_of).sum::<usize>().max(1),
        Node::Function { args, .. } | Node::Operator { args, .. } => args.iter().map(cells_of).sum(),
        Node::Parenthesis(c) => cells_of(c),
        _ => 0,
    }
}

/// An exact answer: its text (`[1/2 sqrt(3); 0 k]`), a literal to store, and its value as floats when it has one.
#[derive(Debug)]
pub(super) enum Exact {
    Message(String),
    Value {
        text: String,
        literal: Option<String>,
        approx: Option<super::M>,
        /// Some entry is a fraction or an irrational, so the text says more than whole numbers would.
        rich: bool,
    },
}

/// The float answer already shows this rational whole: a whole number, or a decimal that ends within the digits a
/// matrix entry gets (`0.00001`, `1.5`), where a fraction beside it would add nothing.
fn shows_whole(r: &Rat) -> bool {
    if r.d.is_one() {
        return true;
    }
    let mut d = r.d.clone();
    for p in [2, 5] {
        let p = BigInt::from(p);
        while (&d % &p).is_zero() {
            d /= &p;
        }
    }
    if !d.is_one() {
        return false;
    }
    let s = jsnum::to_string(rat_f64(r));
    let mant = s.split('e').next().unwrap_or("");
    let digits = mant.chars().filter(char::is_ascii_digit).skip_while(|c| *c == '0');
    digits.count() <= 6
}

/// A cell written to read back in a spaced row: `k^2-6`, `2*sqrt(3)`, `1/2`.
fn compact(s: &str) -> String {
    s.split_whitespace().collect()
}

fn finish<F: Cells>(f: &F, v: X<F::C>) -> Exact {
    let rich_cell = |c: &F::C| match f.as_rat(c) {
        Some(r) => !shows_whole(&r),
        None => true,
    };
    // a row times a column is a number
    let v = match v {
        X::Grid(mut g) if g.len() == 1 && g[0].len() == 1 => X::Num(g.remove(0).remove(0)),
        v => v,
    };
    match v {
        X::Surd(text, approx) => Exact::Value {
            text,
            literal: None,
            approx: Some(super::M::Num(approx)),
            rich: true,
        },
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
                    .map(|r| r.iter().map(|c| compact(&f.show(c))).collect::<Vec<_>>().join(" "))
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
            let approx: Option<Vec<Vec<f64>>> = g
                .iter()
                .map(|r| r.iter().map(|c| f.approx(c)).collect())
                .collect();
            Exact::Value {
                text,
                literal: Some(literal),
                approx: approx.map(super::M::Grid),
                rich: g.iter().flatten().any(rich_cell),
                }
        }
    }
}

fn attempt<F: Cells>(f: &F, node: &Node, opts: &MatrixOptions<'_>) -> Option<Option<Exact>> {
    let ev = Ev { f, opts };
    let out = guarded(|| Some(ev.eval(node, false)));
    match out {
        None => Some(None), // a division by zero inside the field: no exact answer
        Some(Ok(v)) => Some(Some(finish(f, v))),
        Some(Err(XFail::Matrix(m))) => Some(Some(Exact::Message(m))),
        Some(Err(XFail::NotExact)) => None,
    }
}

/// The exact answer to a matrix line, or None when it has none.
pub(super) fn exact_answer(node: &Node, opts: &MatrixOptions<'_>) -> Option<Exact> {
    if cells_of(node) > MAX_CELLS {
        return None;
    }
    if let Some(out) = attempt(&RatField, node, opts) {
        return out;
    }
    if let Some(out) = attempt(&RadField::new(), node, opts) {
        return out;
    }
    let params = params_of(node, opts)?;
    if params.is_empty() {
        return None;
    }
    attempt(&RfField { params }, node, opts).flatten()
}
