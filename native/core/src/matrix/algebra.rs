//! Linear algebra over any `Cells` field: written once, run exactly over the rationals, the square roots and the
//! rational functions, and approximately over floats.

use super::exact_eval::{Cells, G, XFail, XR, fail, size};

/// Past this size a determinant over rational functions isn't expanded (2^n states).
const MAX_SYMBOLIC_DET: usize = 10;

pub(super) fn identity<F: Cells>(f: &F, n: usize) -> G<F::C> {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| if i == j { f.one() } else { f.zero() })
                .collect()
        })
        .collect()
}

pub(super) fn zeros<F: Cells>(f: &F, m: usize, n: usize) -> G<F::C> {
    vec![vec![f.zero(); n]; m]
}

pub(super) fn transpose<C: Clone>(g: &[Vec<C>]) -> G<C> {
    (0..g.first().map_or(0, Vec::len))
        .map(|j| g.iter().map(|r| r[j].clone()).collect())
        .collect()
}

pub(super) fn scale<F: Cells>(f: &F, g: &[Vec<F::C>], k: &F::C) -> G<F::C> {
    g.iter()
        .map(|r| r.iter().map(|v| f.mul(v, k)).collect())
        .collect()
}

pub(super) fn sub<F: Cells>(f: &F, a: &[Vec<F::C>], b: &[Vec<F::C>]) -> G<F::C> {
    a.iter()
        .zip(b)
        .map(|(r, s)| r.iter().zip(s).map(|(x, y)| f.sub(x, y)).collect())
        .collect()
}

pub(super) fn mul<F: Cells>(f: &F, a: &[Vec<F::C>], b: &[Vec<F::C>]) -> XR<G<F::C>> {
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

/// `a - λI`.
pub(super) fn shift<F: Cells>(f: &F, a: &[Vec<F::C>], lambda: &F::C) -> G<F::C> {
    a.iter()
        .enumerate()
        .map(|(i, r)| {
            r.iter()
                .enumerate()
                .map(|(j, v)| if i == j { f.sub(v, lambda) } else { v.clone() })
                .collect()
        })
        .collect()
}

pub(super) fn trace<F: Cells>(f: &F, a: &[Vec<F::C>]) -> F::C {
    a.iter()
        .enumerate()
        .fold(f.zero(), |s, (i, r)| f.add(&s, &r[i]))
}

pub(super) fn dot<F: Cells>(f: &F, u: &[F::C], v: &[F::C]) -> F::C {
    u.iter()
        .zip(v)
        .fold(f.zero(), |s, (x, y)| f.add(&s, &f.mul(x, y)))
}

pub(super) fn column<C: Clone>(g: &[Vec<C>], j: usize) -> Vec<C> {
    g.iter().map(|r| r[j].clone()).collect()
}

/// The matrix whose columns are these vectors.
pub(super) fn from_columns<C: Clone>(cols: &[Vec<C>]) -> G<C> {
    transpose(cols)
}

pub(super) fn power<F: Cells>(f: &F, sq: &[Vec<F::C>], k: i64) -> XR<G<F::C>> {
    let mut base = if k < 0 { inverse(f, sq)? } else { sq.to_vec() };
    let mut out = identity(f, sq.len());
    let mut k = k.abs();
    while k > 0 {
        if k % 2 == 1 {
            out = mul(f, &out, &base)?;
        }
        k /= 2;
        if k > 0 {
            base = mul(f, &base, &base)?;
        }
    }
    Ok(out)
}

pub(super) struct Reduced<C> {
    pub m: G<C>,
    pub pivots: Vec<usize>,
    pub det: C,
}

/// Row echelon (or, with `full`, reduced row echelon) by elimination, and the determinant it finds on the way.
pub(super) fn reduce<F: Cells>(f: &F, src: &[Vec<F::C>], full: bool) -> Reduced<F::C> {
    let mut m: G<F::C> = src.to_vec();
    let rows = m.len();
    let cols = m.first().map_or(0, Vec::len);
    let mut pivots = Vec::new();
    let mut det = f.one();
    let (mut r, mut c) = (0, 0);
    while c < cols && r < rows {
        // floats take the largest pivot; exact fields the first nonzero one
        let mut p = None;
        for i in r..rows {
            if f.is_zero(&m[i][c]) {
                continue;
            }
            match p {
                None => p = Some(i),
                Some(q) if f.weight(&m[i][c]) > f.weight(&m[q][c]) => p = Some(i),
                _ => {}
            }
        }
        let Some(p) = p else {
            for row in m.iter_mut().skip(r) {
                row[c] = f.zero();
            }
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
                .map(|(k, (v, p))| {
                    if k == c {
                        f.zero()
                    } else {
                        f.sub(v, &f.mul(&fac, p))
                    }
                })
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
pub(super) fn det_expand<F: Cells>(f: &F, g: &[Vec<F::C>]) -> Option<F::C> {
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
        let Some(cur) = dp[mask].clone() else {
            continue;
        };
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
            let term = if inversions % 2 == 1 {
                f.neg(&term)
            } else {
                term
            };
            let slot = &mut dp[mask | (1 << j)];
            *slot = Some(match slot.take() {
                Some(s) => f.add(&s, &term),
                None => term,
            });
        }
    }
    Some(dp[(1 << n) - 1].clone().unwrap_or_else(|| f.zero()))
}

/// The matrix without row i and column j.
pub(super) fn minor_of<C: Clone>(g: &[Vec<C>], i: usize, j: usize) -> G<C> {
    g.iter()
        .enumerate()
        .filter(|(r, _)| *r != i)
        .map(|(_, row)| {
            row.iter()
                .enumerate()
                .filter(|(c, _)| *c != j)
                .map(|(_, v)| v.clone())
                .collect()
        })
        .collect()
}

pub(super) fn det<F: Cells>(f: &F, g: &[Vec<F::C>]) -> XR<F::C> {
    if f.division_free() {
        return det_expand(f, g).ok_or(XFail::NotExact);
    }
    Ok(reduce(f, g, false).det)
}

/// `(-1)^(i+j)` times the minor that drops row i and column j.
pub(super) fn cofactor<F: Cells>(f: &F, g: &[Vec<F::C>], i: usize, j: usize) -> XR<F::C> {
    let d = det(f, &minor_of(g, i, j))?;
    Ok(if (i + j) % 2 == 1 { f.neg(&d) } else { d })
}

pub(super) fn cofactors<F: Cells>(f: &F, g: &[Vec<F::C>]) -> XR<G<F::C>> {
    let n = g.len();
    if n == 1 {
        return Ok(vec![vec![f.one()]]);
    }
    (0..n)
        .map(|i| (0..n).map(|j| cofactor(f, g, i, j)).collect())
        .collect()
}

pub(super) fn inverse<F: Cells>(f: &F, g: &[Vec<F::C>]) -> XR<G<F::C>> {
    let n = g.len();
    if f.division_free() {
        // adjugate over the determinant: no division until the very end
        let d = det(f, g)?;
        if f.is_zero(&d) {
            return fail("no inverse (determinant is 0)");
        }
        let adj = transpose(&cofactors(f, g)?);
        return Ok(adj
            .iter()
            .map(|r| r.iter().map(|c| f.div(c, &d)).collect())
            .collect());
    }
    let aug: G<F::C> = g
        .iter()
        .zip(identity(f, n))
        .map(|(r, e)| r.iter().cloned().chain(e).collect())
        .collect();
    let red = reduce(f, &aug, true);
    if red.pivots.len() < n || red.pivots[n - 1] != n - 1 {
        return fail("no inverse (determinant is 0)");
    }
    Ok(red.m.into_iter().map(|r| r[n..].to_vec()).collect())
}

pub(super) fn rank<F: Cells>(f: &F, g: &[Vec<F::C>]) -> usize {
    reduce(f, g, false).pivots.len()
}

/// A basis of the null space: one vector per free column, that column's entry 1.
pub(super) fn null_space<F: Cells>(f: &F, g: &[Vec<F::C>]) -> Vec<Vec<F::C>> {
    let n = g[0].len();
    let red = reduce(f, g, true);
    (0..n)
        .filter(|j| !red.pivots.contains(j))
        .map(|j| {
            let mut v = vec![f.zero(); n];
            v[j] = f.one();
            for (row, pc) in red.pivots.iter().enumerate() {
                v[*pc] = f.neg(&red.m[row][j]);
            }
            f.tidy_vec(v)
        })
        .collect()
}

/// The columns of `g` that hold pivots: a basis of its column space.
pub(super) fn pivot_columns<F: Cells>(f: &F, g: &[Vec<F::C>]) -> Vec<Vec<F::C>> {
    reduce(f, g, false)
        .pivots
        .iter()
        .map(|j| column(g, *j))
        .collect()
}

/// The nonzero rows of the reduced row echelon form: a basis of the row space.
pub(super) fn row_basis<F: Cells>(f: &F, g: &[Vec<F::C>]) -> Vec<Vec<F::C>> {
    let red = reduce(f, g, true);
    let r = red.pivots.len();
    red.m.into_iter().take(r).map(|v| f.tidy_vec(v)).collect()
}

pub(super) enum Solution<C> {
    Unique(Vec<C>),
    /// `particular + t·directions[0] + s·directions[1] + …`
    Family {
        particular: Vec<C>,
        directions: Vec<Vec<C>>,
    },
    Inconsistent,
}

/// `A x = b`.
pub(super) fn solve<F: Cells>(f: &F, a: &[Vec<F::C>], b: &[F::C]) -> Solution<F::C> {
    let n = a[0].len();
    let aug: G<F::C> = a
        .iter()
        .zip(b)
        .map(|(r, v)| {
            r.iter()
                .cloned()
                .chain(std::iter::once(v.clone()))
                .collect()
        })
        .collect();
    let red = reduce(f, &aug, true);
    if red.pivots.contains(&n) {
        return Solution::Inconsistent;
    }
    let mut x = vec![f.zero(); n];
    for (row, pc) in red.pivots.iter().enumerate() {
        x[*pc] = red.m[row][n].clone();
    }
    let directions: Vec<Vec<F::C>> = (0..n)
        .filter(|j| !red.pivots.contains(j))
        .map(|j| {
            let mut v = vec![f.zero(); n];
            v[j] = f.one();
            for (row, pc) in red.pivots.iter().enumerate() {
                v[*pc] = f.neg(&red.m[row][j]);
            }
            f.tidy_vec(v)
        })
        .collect();
    if directions.is_empty() {
        Solution::Unique(x)
    } else {
        Solution::Family {
            particular: x,
            directions,
        }
    }
}

/// The characteristic polynomial det(λI − A), coefficients from λ⁰ up (the last is 1), by Faddeev–LeVerrier.
pub(super) fn charpoly<F: Cells>(f: &F, a: &[Vec<F::C>]) -> XR<Vec<F::C>> {
    let n = a.len();
    let mut c = vec![f.zero(); n + 1];
    c[n] = f.one();
    let mut am = zeros(f, n, n);
    for k in 1..=n {
        let mk: G<F::C> = am
            .iter()
            .enumerate()
            .map(|(i, r)| {
                r.iter()
                    .enumerate()
                    .map(|(j, v)| {
                        if i == j {
                            f.add(v, &c[n - k + 1])
                        } else {
                            v.clone()
                        }
                    })
                    .collect()
            })
            .collect();
        am = mul(f, a, &mk)?;
        let kk = f.lift(&super::exact_eval::ri(k as i64));
        c[n - k] = f.neg(&f.div(&trace(f, &am), &kk));
    }
    Ok(c)
}

/// Gram–Schmidt: orthogonal (not yet unit) vectors spanning the same space; dependent ones drop out.
pub(super) fn orthogonalize<F: Cells>(f: &F, vs: &[Vec<F::C>]) -> Vec<Vec<F::C>> {
    let mut out: Vec<Vec<F::C>> = Vec::new();
    for v in vs {
        let mut w = v.clone();
        for u in &out {
            let k = f.div(&dot(f, v, u), &dot(f, u, u));
            w = w
                .iter()
                .zip(u)
                .map(|(x, y)| f.sub(x, &f.mul(&k, y)))
                .collect();
        }
        if w.iter().any(|x| !f.is_zero(x)) {
            out.push(f.tidy_vec(w));
        }
    }
    out
}

/// `v / |v|`, where the field holds the root.
pub(super) fn unit<F: Cells>(f: &F, v: &[F::C]) -> XR<Vec<F::C>> {
    let n2 = dot(f, v, v);
    if f.is_zero(&n2) {
        return fail("the zero vector has no direction");
    }
    let n = f.sqrt(&n2).ok_or(XFail::NotExact)?;
    Ok(v.iter().map(|x| f.div(x, &n)).collect())
}

/// `P A = L U`, with P None when no rows were swapped.
/// P (when rows swapped), L and U.
pub(super) type Lu<C> = (Option<G<C>>, G<C>, G<C>);

pub(super) fn lu<F: Cells>(f: &F, a: &[Vec<F::C>]) -> Lu<F::C> {
    let (m, n) = (a.len(), a[0].len());
    let mut u = a.to_vec();
    let mut l = identity(f, m);
    let mut perm: Vec<usize> = (0..m).collect();
    for k in 0..m.min(n) {
        let mut p = None;
        for i in k..m {
            if f.is_zero(&u[i][k]) {
                continue;
            }
            match p {
                None => p = Some(i),
                Some(q) if f.weight(&u[i][k]) > f.weight(&u[q][k]) => p = Some(i),
                _ => {}
            }
        }
        let Some(p) = p else { continue };
        if p != k {
            u.swap(p, k);
            perm.swap(p, k);
            let (lo, hi) = l.split_at_mut(p);
            lo[k][..k].swap_with_slice(&mut hi[0][..k]);
        }
        for i in k + 1..m {
            if f.is_zero(&u[i][k]) {
                continue;
            }
            let fac = f.div(&u[i][k], &u[k][k]);
            l[i][k] = fac.clone();
            let pivot_row = u[k].clone();
            u[i] = u[i]
                .iter()
                .zip(&pivot_row)
                .enumerate()
                .map(|(j, (v, p))| {
                    if j == k {
                        f.zero()
                    } else {
                        f.sub(v, &f.mul(&fac, p))
                    }
                })
                .collect();
        }
    }
    let swapped = perm.iter().enumerate().any(|(i, p)| i != *p);
    let pm = swapped.then(|| {
        (0..m)
            .map(|i| {
                (0..m)
                    .map(|j| if perm[i] == j { f.one() } else { f.zero() })
                    .collect()
            })
            .collect()
    });
    (pm, l, u)
}

pub(super) fn is_symmetric<F: Cells>(f: &F, a: &[Vec<F::C>]) -> bool {
    let n = a.len();
    n == a[0].len() && (0..n).all(|i| (0..i).all(|j| f.is_zero(&f.sub(&a[i][j], &a[j][i]))))
}

/// `A = L Lᵀ` for a symmetric positive definite A.
pub(super) fn cholesky<F: Cells>(f: &F, a: &[Vec<F::C>]) -> XR<G<F::C>> {
    if !is_symmetric(f, a) {
        return fail("cholesky needs a symmetric matrix");
    }
    let n = a.len();
    let mut l = zeros(f, n, n);
    for j in 0..n {
        let s = (0..j).fold(a[j][j].clone(), |s, k| {
            f.sub(&s, &f.mul(&l[j][k], &l[j][k]))
        });
        match f.sign(&s) {
            Some(1) => {}
            Some(_) => return fail("not positive definite, so no Cholesky factor"),
            None => return Err(XFail::NotExact),
        }
        l[j][j] = f.sqrt(&s).ok_or(XFail::NotExact)?;
        for i in j + 1..n {
            let s = (0..j).fold(a[i][j].clone(), |s, k| {
                f.sub(&s, &f.mul(&l[i][k], &l[j][k]))
            });
            l[i][j] = f.div(&s, &l[j][j]);
        }
    }
    Ok(l)
}

/// The Moore–Penrose pseudoinverse, through a rank factorization A = C R.
pub(super) fn pinv<F: Cells>(f: &F, a: &[Vec<F::C>]) -> XR<G<F::C>> {
    let (m, n) = (a.len(), a[0].len());
    let r = rank(f, a);
    if r == 0 {
        return Ok(zeros(f, n, m));
    }
    let at = transpose(a);
    if r == n {
        return mul(f, &inverse(f, &mul(f, &at, a)?)?, &at);
    }
    if r == m {
        return mul(f, &at, &inverse(f, &mul(f, a, &at)?)?);
    }
    let c = from_columns(&pivot_columns(f, a));
    let rr = row_basis_raw(f, a);
    let ct = transpose(&c);
    let rt = transpose(&rr);
    let left = mul(f, &rt, &inverse(f, &mul(f, &rr, &rt)?)?)?;
    let right = mul(f, &inverse(f, &mul(f, &ct, &c)?)?, &ct)?;
    mul(f, &left, &right)
}

/// The nonzero rows of the reduced row echelon form, untidied (so `A = C R` holds).
fn row_basis_raw<F: Cells>(f: &F, g: &[Vec<F::C>]) -> G<F::C> {
    let red = reduce(f, g, true);
    let r = red.pivots.len();
    red.m.into_iter().take(r).collect()
}

/// The sizes of the Jordan blocks for λ, largest first, from the ranks of (A − λI)^k.
pub(super) fn jordan_blocks<F: Cells>(
    f: &F,
    a: &[Vec<F::C>],
    lambda: &F::C,
    mult: usize,
) -> XR<Vec<usize>> {
    let n = a.len();
    let b = shift(f, a, lambda);
    let mut ranks = vec![n];
    let mut p = identity(f, n);
    // blocks of size ≥ k number r(k-1) − r(k); the ranks settle once all `mult` dimensions are used
    while ranks.len() <= mult {
        p = mul(f, &p, &b)?;
        let r = rank(f, &p);
        ranks.push(r);
        if n - r >= mult {
            break;
        }
    }
    let at_least: Vec<usize> = ranks.windows(2).map(|w| w[0] - w[1]).collect();
    let mut blocks = Vec::new();
    for (k, count) in at_least.iter().enumerate() {
        let next = at_least.get(k + 1).copied().unwrap_or(0);
        for _ in 0..count.saturating_sub(next) {
            blocks.push(k + 1);
        }
    }
    blocks.sort_unstable_by(|a, b| b.cmp(a));
    Ok(blocks)
}
