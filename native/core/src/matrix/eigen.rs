//! Eigenvalues as exactly as they come: the characteristic polynomial over the matrix's own field, its rational roots
//! found by guessing from the float roots and checking exactly, then quadratic factors (`(5 ± sqrt(33))/2`,
//! `1 ± 2i`), and whatever is left to float digits. Eigenvectors come from the exact null space of A − λI wherever λ
//! is exact (a root like 1 + √2 is worked in the field with √2), and from complex floats otherwise.

use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::algebra;
use super::exact_eval::{Cells, G, XFail, XR, fail, rat_f64, ri};
use super::numeric;
use super::show::{C64, Cell, Fmt, Part};
use crate::exact::{Field, Rad, RadField, Rat, bgcd, rat, sqrt_rat};

/// Past this size the characteristic polynomial's float roots aren't trusted.
const MAX_EIGEN: usize = 16;

#[derive(Clone, Debug)]
pub(super) enum Kind<C> {
    /// A value in the matrix's own field.
    Field(C),
    /// `p + sign·q·√r` over the rationals (r < 0 is `q·√|r|·i`).
    Quad { p: Rat, q: Rat, r: BigInt, sign: i32 },
    /// Only known to float digits.
    Num,
}

#[derive(Clone, Debug)]
pub(super) struct Eig<C> {
    pub kind: Kind<C>,
    pub z: C64,
    pub mult: usize,
}

pub(super) enum Spectrum<C> {
    Values(Vec<Eig<C>>),
    /// A symbolic 2×2's eigenvalues, written out.
    Symbolic(String),
}

// ---- polynomials over the rationals (coefficients from x⁰ up) ----

fn r_add(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.d + &b.n * &a.d, &a.d * &b.d)
}
fn r_sub(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.d - &b.n * &a.d, &a.d * &b.d)
}
fn r_mul(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.n, &a.d * &b.d)
}
fn r_div(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.d, &a.d * &b.n)
}

fn peval(p: &[Rat], x: &Rat) -> Rat {
    p.iter().rev().fold(ri(0), |acc, c| r_add(&r_mul(&acc, x), c))
}

/// p / (x − r), for a root r.
fn deflate(p: &[Rat], r: &Rat) -> Vec<Rat> {
    let n = p.len() - 1;
    let mut q = vec![ri(0); n];
    let mut carry = ri(0);
    for k in (0..n).rev() {
        carry = r_add(&p[k + 1], &r_mul(&carry, r));
        q[k] = carry.clone();
    }
    q
}

/// p / (x² − s·x + t) when it divides evenly.
fn divide_quadratic(p: &[Rat], s: &Rat, t: &Rat) -> Option<Vec<Rat>> {
    let mut r: Vec<Rat> = p.to_vec();
    let n = p.len() - 1;
    if n < 2 {
        return None;
    }
    let mut q = vec![ri(0); n - 1];
    for k in (2..=n).rev() {
        let c = r[k].clone();
        q[k - 2] = c.clone();
        r[k] = ri(0);
        r[k - 1] = r_add(&r[k - 1], &r_mul(&c, s));
        r[k - 2] = r_sub(&r[k - 2], &r_mul(&c, t));
    }
    (r[0].n.is_zero() && r[1].n.is_zero()).then_some(q)
}

/// Rationals near x: the nearest whole number and the continued fraction's convergents.
fn candidates(x: f64) -> Vec<Rat> {
    let mut out = Vec::new();
    if !x.is_finite() || x.abs() > 1e12 {
        return out;
    }
    let tol = 1e-4 * x.abs().max(1.0);
    let (mut h0, mut h1) = (BigInt::zero(), BigInt::one());
    let (mut k0, mut k1) = (BigInt::one(), BigInt::zero());
    let mut y = x;
    for _ in 0..24 {
        let a = y.floor();
        let ai = BigInt::from(a as i64);
        let h2 = &ai * &h1 + &h0;
        let k2 = &ai * &k1 + &k0;
        (h0, h1, k0, k1) = (h1, h2.clone(), k1, k2.clone());
        if k2 > BigInt::from(1_000_000) {
            break;
        }
        let c = rat(h2, k2);
        if (rat_f64(&c) - x).abs() <= tol {
            out.push(c);
        }
        let frac = y - a;
        if frac.abs() < 1e-12 {
            break;
        }
        y = 1.0 / frac;
    }
    out
}

fn float_poly(p: &[Rat]) -> Vec<f64> {
    p.iter().map(rat_f64).collect()
}

#[derive(Clone, Debug)]
enum Root {
    Rat(Rat),
    Quad { p: Rat, q: Rat, r: BigInt, sign: i32 },
    Num(C64),
}

/// The roots of x² + b·x + c, exactly.
fn quadratic(b: &Rat, c: &Rat) -> Vec<Root> {
    let half = rat(BigInt::one(), BigInt::from(2));
    let disc = r_sub(&r_mul(b, b), &r_mul(&ri(4), c));
    let p = r_mul(&rat(-&b.n, b.d.clone()), &half);
    if disc.n.is_zero() {
        return vec![Root::Rat(p.clone()), Root::Rat(p)];
    }
    let neg = disc.n.is_negative();
    let Some(Rad { c: k, rad }) = sqrt_rat(&rat(disc.n.abs(), disc.d.clone())) else {
        return vec![];
    };
    let q = r_mul(&k, &half);
    if rad.is_one() && !neg {
        return vec![Root::Rat(r_add(&p, &q)), Root::Rat(r_sub(&p, &q))];
    }
    let r = if neg { -rad } else { rad };
    [1, -1]
        .into_iter()
        .map(|sign| Root::Quad {
            p: p.clone(),
            q: q.clone(),
            r: r.clone(),
            sign,
        })
        .collect()
}

/// The roots of a monic polynomial with rational coefficients, exactly where it can.
fn rational_roots(poly: &[Rat]) -> Vec<Root> {
    let mut p = poly.to_vec();
    let mut out = Vec::new();
    'outer: for _ in 0..64 {
        let deg = p.len() - 1;
        match deg {
            0 => break,
            1 => {
                out.push(Root::Rat(r_div(&rat(-&p[0].n, p[0].d.clone()), &p[1])));
                break;
            }
            2 => {
                let (b, c) = (r_div(&p[1], &p[2]), r_div(&p[0], &p[2]));
                out.extend(quadratic(&b, &c));
                break;
            }
            _ => {}
        }
        if p[0].n.is_zero() {
            out.push(Root::Rat(ri(0)));
            p.remove(0);
            continue;
        }
        let guesses = numeric::poly_roots(&float_poly(&p));
        // a rational root, checked exactly
        for z in &guesses {
            if z.1.abs() > 1e-6 * z.0.abs().max(1.0) {
                continue;
            }
            for c in candidates(z.0) {
                if peval(&p, &c).n.is_zero() {
                    p = deflate(&p, &c);
                    out.push(Root::Rat(c));
                    continue 'outer;
                }
            }
        }
        // a quadratic factor x² − s·x + t from a pair of float roots
        for i in 0..guesses.len() {
            for j in i + 1..guesses.len() {
                let (a, b) = (guesses[i], guesses[j]);
                let s = (a.0 + b.0, a.1 + b.1);
                let t = (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
                let tiny = |v: f64, w: f64| v.abs() <= 1e-6 * w.abs().max(1.0);
                if !tiny(s.1, s.0) || !tiny(t.1, t.0) {
                    continue;
                }
                for cs in candidates(s.0) {
                    for ct in candidates(t.0) {
                        if let Some(q) = divide_quadratic(&p, &cs, &ct) {
                            out.extend(quadratic(&rat(-&cs.n, cs.d.clone()), &ct));
                            p = q;
                            continue 'outer;
                        }
                    }
                }
            }
        }
        out.extend(guesses.into_iter().map(Root::Num));
        break;
    }
    out
}

fn quad_z(p: &Rat, q: &Rat, r: &BigInt, sign: i32) -> C64 {
    let root = r.abs().to_f64().unwrap_or(f64::NAN).sqrt();
    let off = sign as f64 * rat_f64(q) * root;
    if r.is_negative() {
        (rat_f64(p), off)
    } else {
        (rat_f64(p) + off, 0.0)
    }
}

/// Equal roots counted once, with their multiplicity.
fn group<C: Clone>(eigs: Vec<Eig<C>>, same: impl Fn(&Eig<C>, &Eig<C>) -> bool) -> Vec<Eig<C>> {
    let mut out: Vec<Eig<C>> = Vec::new();
    for e in eigs {
        match out.iter_mut().find(|o| same(o, &e)) {
            Some(o) => o.mult += e.mult,
            None => out.push(e),
        }
    }
    out.sort_by(|a, b| {
        b.z.0
            .partial_cmp(&a.z.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.z.1.partial_cmp(&a.z.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    out
}

fn close(a: C64, b: C64) -> bool {
    let len = |x: f64, y: f64| (x * x + y * y).sqrt();
    let scale = len(a.0, a.1).max(1.0);
    len(a.0 - b.0, a.1 - b.1) <= 1e-6 * scale
}

fn is_triangular<F: Cells>(f: &F, a: &[Vec<F::C>]) -> bool {
    let n = a.len();
    let lower = (0..n).all(|i| (i + 1..n).all(|j| f.is_zero(&a[i][j])));
    let upper = (0..n).all(|i| (0..i).all(|j| f.is_zero(&a[i][j])));
    lower || upper
}

pub(super) fn approx_grid<F: Cells>(f: &F, a: &[Vec<F::C>]) -> Option<G<f64>> {
    a.iter().map(|r| r.iter().map(|c| f.approx(c)).collect()).collect()
}

fn num_eig<C>(z: C64) -> Eig<C> {
    Eig {
        kind: Kind::Num,
        z,
        mult: 1,
    }
}

fn float_eigs<F: Cells>(f: &F, a: &[Vec<F::C>]) -> XR<Vec<Eig<F::C>>> {
    let g = approx_grid(f, a).ok_or(XFail::NotExact)?;
    let n = g.len();
    let symmetric = (0..n).all(|i| (0..i).all(|j| (g[i][j] - g[j][i]).abs() <= 1e-12 * (g[i][j].abs() + 1.0)));
    let zs: Vec<C64> = if symmetric {
        numeric::sym_eigen(&g).0.into_iter().map(|v| (v, 0.0)).collect()
    } else {
        let ff = super::exact_eval::F64Field::new();
        let cp = algebra::charpoly(&ff, &g)?;
        numeric::poly_roots(&cp)
    };
    Ok(group(zs.into_iter().map(num_eig).collect(), |a, b| close(a.z, b.z)))
}

pub(super) fn eigenvalues<F: Cells>(f: &F, a: &[Vec<F::C>]) -> XR<Spectrum<F::C>> {
    let n = a.len();
    if n > MAX_EIGEN {
        return fail(format!("eigenvalues go up to {MAX_EIGEN}×{MAX_EIGEN}"));
    }
    if is_triangular(f, a) {
        let eigs = (0..n)
            .map(|i| Eig {
                z: (f.approx(&a[i][i]).unwrap_or(f64::NAN), 0.0),
                kind: Kind::Field(a[i][i].clone()),
                mult: 1,
            })
            .collect();
        let grouped = group(eigs, |x, y| match (&x.kind, &y.kind) {
            (Kind::Field(p), Kind::Field(q)) => f.is_zero(&f.sub(p, q)),
            _ => false,
        });
        return Ok(Spectrum::Values(grouped));
    }
    if f.is_float() {
        return Ok(Spectrum::Values(float_eigs(f, a)?));
    }
    let cp = algebra::charpoly(f, a)?;
    if let Some(rats) = cp.iter().map(|c| f.as_rat(c)).collect::<Option<Vec<Rat>>>() {
        let eigs: Vec<Eig<F::C>> = rational_roots(&rats)
            .into_iter()
            .map(|r| match r {
                Root::Rat(v) => Eig {
                    z: (rat_f64(&v), 0.0),
                    kind: Kind::Field(f.lift(&v)),
                    mult: 1,
                },
                Root::Quad { p, q, r, sign } => Eig {
                    z: quad_z(&p, &q, &r, sign),
                    kind: Kind::Quad { p, q, r, sign },
                    mult: 1,
                },
                Root::Num(z) => num_eig(z),
            })
            .collect();
        let grouped = group(eigs, |x, y| match (&x.kind, &y.kind) {
            (Kind::Field(p), Kind::Field(q)) => f.is_zero(&f.sub(p, q)),
            (
                Kind::Quad { p, q, r, sign },
                Kind::Quad {
                    p: p2,
                    q: q2,
                    r: r2,
                    sign: s2,
                },
            ) => p == p2 && q == q2 && r == r2 && sign == s2,
            (Kind::Num, Kind::Num) => close(x.z, y.z),
            _ => false,
        });
        return Ok(Spectrum::Values(grouped));
    }
    if approx_grid(f, a).is_some() {
        return Ok(Spectrum::Values(float_eigs(f, a)?));
    }
    if n == 2 {
        // (tr ± √(tr² − 4·det)) / 2
        let tr = algebra::trace(f, a);
        let det = algebra::det(f, a)?;
        let four = f.lift(&ri(4));
        let disc = f.sub(&f.mul(&tr, &tr), &f.mul(&four, &det));
        let wrap = |s: String| {
            if s.chars().skip(1).any(|c| c == '+' || c == '-' || c == ' ' || c == '/') {
                format!("({s})")
            } else {
                s
            }
        };
        return Ok(Spectrum::Symbolic(format!(
            "({} ± sqrt({}))/2",
            f.show(&tr),
            f.show(&disc)
        )
        .replace("(0 ± ", "(±")
        .replace(&format!("sqrt({})", wrap(f.show(&disc))), &format!("sqrt({})", f.show(&disc)))));
    }
    fail("eigenvalues of a matrix with symbols go up to 2×2; charpoly gives its polynomial")
}

fn lcm(a: &BigInt, b: &BigInt) -> BigInt {
    a / bgcd(a, b) * b
}

/// `(5 ± sqrt(33))/2`, `1 ± 2i`, `(1 - i*sqrt(3))/2`; `pm` is `±`, `+` or `-`.
fn quad_text(p: &Rat, q: &Rat, r: &BigInt, pm: &str) -> String {
    let d = lcm(&p.d, &q.d);
    let a = &p.n * (&d / &p.d);
    let b = &q.n * (&d / &q.d);
    let surd = if *r == BigInt::from(-1) {
        "i".to_string()
    } else if r.is_negative() {
        format!("i*sqrt({})", -r)
    } else {
        format!("sqrt({r})")
    };
    let bs = if b.is_one() {
        surd
    } else if *r == BigInt::from(-1) {
        format!("{b}i")
    } else {
        format!("{b}*{surd}")
    };
    let inner = if a.is_zero() {
        match pm {
            "+" => bs,
            _ => format!("{pm}{bs}"),
        }
    } else {
        format!("{a} {pm} {bs}")
    };
    if d.is_one() {
        inner
    } else if a.is_zero() {
        format!("{inner}/{d}")
    } else {
        format!("({inner})/{d}")
    }
}

/// An eigenvalue as one entry (for D and J).
pub(super) fn eig_cell<F: Cells>(f: &F, fmt: &Fmt, e: &Eig<F::C>) -> Cell {
    match &e.kind {
        Kind::Field(c) => fmt.cell(f, c),
        Kind::Quad { p, q, r, sign } => {
            fmt.text_cell(quad_text(p, q, r, if *sign > 0 { "+" } else { "-" }), e.z)
        }
        Kind::Num => fmt.ccell(e.z),
    }
}

fn times(mult: usize) -> String {
    if mult > 1 {
        format!(" (×{mult})")
    } else {
        String::new()
    }
}

/// A conjugate pair to a few digits: `1 ± 2i`, `±i`.
fn conjugates(fmt: &Fmt, z: C64) -> String {
    if z.0.abs() <= 1e-12 * z.1.abs() {
        return format!("±{}", fmt.complex((0.0, z.1.abs())));
    }
    fmt.complex((z.0, z.1.abs())).replacen('+', " ± ", 1)
}

/// `λ = 5.37228, -0.372281`, exactly `λ = (5 ± sqrt(33))/2`.
pub(super) fn values_part<F: Cells>(f: &F, fmt: &Fmt, eigs: &[Eig<F::C>]) -> Part {
    let mut dec = Vec::new();
    let mut exact = Vec::new();
    let mut rich = false;
    let mut used = vec![false; eigs.len()];
    for i in 0..eigs.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let e = &eigs[i];
        // a conjugate or ± pair is written once
        let partner = (i + 1..eigs.len()).find(|&j| {
            !used[j]
                && eigs[j].mult == e.mult
                && match (&e.kind, &eigs[j].kind) {
                    (
                        Kind::Quad { p, q, r, .. },
                        Kind::Quad {
                            p: p2,
                            q: q2,
                            r: r2,
                            ..
                        },
                    ) => p == p2 && q == q2 && r == r2,
                    (Kind::Num, Kind::Num) => {
                        e.z.1 != 0.0 && close((e.z.0, -e.z.1), eigs[j].z)
                    }
                    _ => false,
                }
        });
        let m = times(e.mult);
        match (partner, &e.kind) {
            (Some(j), Kind::Quad { p, q, r, .. }) => {
                used[j] = true;
                rich = true;
                exact.push(format!("{}{m}", quad_text(p, q, r, "±")));
                if r.is_negative() {
                    dec.push(format!("{}{m}", conjugates(fmt, e.z)));
                } else {
                    dec.push(format!("{}{m}, {}{m}", fmt.complex(e.z), fmt.complex(eigs[j].z)));
                }
            }
            (Some(j), _) => {
                used[j] = true;
                let text = conjugates(fmt, e.z);
                dec.push(format!("{text}{m}"));
                exact.push(format!("{text}{m}"));
            }
            _ => {
                let c = eig_cell(f, fmt, e);
                rich |= c.rich;
                dec.push(format!("{}{m}", c.dec));
                exact.push(format!("{}{m}", c.exact.unwrap_or(c.dec)));
            }
        }
    }
    let display = dec.join(", ");
    let exact = exact.join(", ");
    Part {
        label: Some("λ = ".into()),
        exact: (rich && exact != display).then_some(exact),
        display,
        ..Part::default()
    }
}

/// A basis of the eigenspace for one eigenvalue, as entries.
pub(super) fn eigenvectors<F: Cells>(f: &F, fmt: &Fmt, a: &[Vec<F::C>], e: &Eig<F::C>) -> Vec<Vec<Cell>> {
    match &e.kind {
        Kind::Field(l) if !f.is_float() => algebra::null_space(f, &algebra::shift(f, a, l))
            .iter()
            .map(|v| v.iter().map(|c| fmt.cell(f, c)).collect())
            .collect(),
        Kind::Quad { p, q, r, sign } if r.is_positive() => {
            if let Some(rats) = a
                .iter()
                .map(|row| row.iter().map(|c| f.as_rat(c)).collect::<Option<Vec<_>>>())
                .collect::<Option<Vec<_>>>()
            {
                let rf = RadField::new();
                let lifted: G<Vec<Rat>> = rats
                    .iter()
                    .map(|row| row.iter().map(|c| rf.lift(c)).collect())
                    .collect();
                if let Some(root) = rf.root(&rat(r.clone(), BigInt::one())) {
                    let sq = rf.mul(&rf.lift(&r_mul(q, &ri(*sign as i64))), &root);
                    let lambda = rf.add(&rf.lift(p), &sq);
                    return algebra::null_space(&rf, &algebra::shift(&rf, &lifted, &lambda))
                        .iter()
                        .map(|v| v.iter().map(|c| fmt.cell(&rf, c)).collect())
                        .collect();
                }
            }
            numeric_vectors(f, fmt, a, e.z)
        }
        _ => numeric_vectors(f, fmt, a, e.z),
    }
}

fn numeric_vectors<F: Cells>(f: &F, fmt: &Fmt, a: &[Vec<F::C>], z: C64) -> Vec<Vec<Cell>> {
    let Some(g) = approx_grid(f, a) else {
        return Vec::new();
    };
    let n = g.len();
    let scale = g.iter().flatten().fold(1.0_f64, |m, v| m.max(v.abs()));
    let m: G<C64> = g
        .iter()
        .enumerate()
        .map(|(i, r)| {
            r.iter()
                .enumerate()
                .map(|(j, v)| if i == j { (v - z.0, -z.1) } else { (*v, 0.0) })
                .collect()
        })
        .collect();
    numeric::complex_null_space(&m, 1e-7 * scale * n as f64)
        .into_iter()
        .map(|v| v.into_iter().map(|c| fmt.ccell(c)).collect())
        .collect()
}

type Grid = Vec<Vec<Cell>>;

/// `A = P D P⁻¹`: P's columns are eigenvectors, D the eigenvalues; or why there's no such P.
pub(super) fn diagonalize<F: Cells>(
    f: &F,
    fmt: &Fmt,
    a: &[Vec<F::C>],
    eigs: &[Eig<F::C>],
) -> XR<(Grid, Grid)> {
    let n = a.len();
    let mut cols: Vec<Vec<Cell>> = Vec::new();
    let mut diag: Vec<Cell> = Vec::new();
    for e in eigs {
        let vs = eigenvectors(f, fmt, a, e);
        if vs.len() < e.mult {
            let c = eig_cell(f, fmt, e);
            return fail(format!(
                "not diagonalizable: λ = {} appears {} times but has {} independent eigenvector{}",
                c.exact.unwrap_or(c.dec),
                e.mult,
                vs.len(),
                if vs.len() == 1 { "" } else { "s" }
            ));
        }
        for v in vs.into_iter().take(e.mult) {
            cols.push(v);
            diag.push(eig_cell(f, fmt, e));
        }
    }
    if cols.len() != n {
        return Err(XFail::NotExact);
    }
    let zero = fmt.cell(f, &f.zero());
    let p = algebra::transpose(&cols);
    let d = (0..n)
        .map(|i| (0..n).map(|j| if i == j { diag[i].clone() } else { zero.clone() }).collect())
        .collect();
    Ok((p, d))
}

/// The Jordan form, from the block sizes of each eigenvalue.
pub(super) fn jordan<F: Cells>(f: &F, fmt: &Fmt, a: &[Vec<F::C>], eigs: &[Eig<F::C>]) -> XR<Vec<Vec<Cell>>> {
    let n = a.len();
    let zero = fmt.cell(f, &f.zero());
    let one = fmt.cell(f, &f.one());
    let mut j = vec![vec![zero; n]; n];
    let mut at = 0;
    for e in eigs {
        let blocks = match &e.kind {
            Kind::Field(l) if !f.is_float() => algebra::jordan_blocks(f, a, l, e.mult)?,
            _ if e.mult == 1 => vec![1],
            _ => {
                return fail("can't find this Jordan form exactly: a repeated eigenvalue isn't a whole number or fraction");
            }
        };
        let cell = eig_cell(f, fmt, e);
        for size in blocks {
            for k in 0..size {
                j[at + k][at + k] = cell.clone();
                if k + 1 < size {
                    j[at + k][at + k + 1] = one.clone();
                }
            }
            at += size;
        }
    }
    if at != n {
        return Err(XFail::NotExact);
    }
    Ok(j)
}
