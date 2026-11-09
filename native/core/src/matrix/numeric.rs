//! Floating-point kernels the matrix reader leans on when an answer has no exact form: polynomial roots, the
//! symmetric eigenproblem, complex null spaces, the singular value decomposition, and the matrix exponential and
//! square root. Everything here is plain `f64` over `Vec<Vec<f64>>` (row-major), with complex numbers as `(re, im)`.

/// A complex number as `(re, im)`.
pub(super) type C64 = (f64, f64);

const EPS: f64 = f64::EPSILON;

fn cadd(a: C64, b: C64) -> C64 {
    (a.0 + b.0, a.1 + b.1)
}

fn csub(a: C64, b: C64) -> C64 {
    (a.0 - b.0, a.1 - b.1)
}

fn cmul(a: C64, b: C64) -> C64 {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn cdiv(a: C64, b: C64) -> C64 {
    // Smith's algorithm keeps the quotient from overflowing.
    if b.0.abs() >= b.1.abs() {
        let r = b.1 / b.0;
        let d = b.0 + b.1 * r;
        ((a.0 + a.1 * r) / d, (a.1 - a.0 * r) / d)
    } else {
        let r = b.0 / b.1;
        let d = b.0 * r + b.1;
        ((a.0 * r + a.1) / d, (a.1 * r - a.0) / d)
    }
}

fn cabs(a: C64) -> f64 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}

/// The polynomial `coeffs` (coeffs[k] multiplies x^k) and its derivative at `z`, by Horner's rule.
fn horner(coeffs: &[f64], z: C64) -> (C64, C64) {
    let mut p: C64 = (0.0, 0.0);
    let mut dp: C64 = (0.0, 0.0);
    for &c in coeffs.iter().rev() {
        dp = cadd(cmul(dp, z), p);
        p = cadd(cmul(p, z), (c, 0.0));
    }
    (p, dp)
}

/// Every complex root of the polynomial whose coefficient of x^k is `coeffs[k]`, with multiplicity.
///
/// Trailing zero coefficients are dropped; a constant (or empty) polynomial has no roots. The roots are found
/// together by Aberth–Ehrlich iteration, then each is polished by a few Newton steps that are only kept when they
/// shrink |p|. A repeated root comes back as a tight cluster (a triple root to about 1e-5). An imaginary part no
/// bigger than 1e-9·max(1, |root|) is set to exactly 0. Sorted by real part descending, then imaginary descending.
pub(super) fn poly_roots(coeffs: &[f64]) -> Vec<C64> {
    let mut c: Vec<f64> = coeffs.to_vec();
    while c.last().is_some_and(|&x| x == 0.0) {
        c.pop();
    }
    if c.len() <= 1 || c.iter().any(|x| !x.is_finite()) {
        return Vec::new();
    }
    // Roots at zero: factor out the lowest power of x.
    let zeros = c.iter().take_while(|&&x| x == 0.0).count();
    let c: Vec<f64> = c[zeros..].to_vec();
    let mut roots: Vec<C64> = vec![(0.0, 0.0); zeros];
    let n = c.len() - 1;
    let lead = c[n];
    let monic: Vec<f64> = c.iter().map(|x| x / lead).collect();

    if n == 1 {
        roots.push((-monic[0], 0.0));
    } else if n > 0 {
        // Starting points on a circle of radius between the smallest and largest root bound, off the real axis.
        let upper = 1.0 + monic[..n].iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        let geo =
            crate::scientific::functions::jsmath::pow(monic[0].abs(), 1.0 / n as f64).max(1e-3);
        let radius = geo.min(upper);
        let mut z: Vec<C64> = (0..n)
            .map(|k| {
                let t = 2.0 * std::f64::consts::PI * k as f64 / n as f64 + 0.4;
                (
                    radius * crate::scientific::functions::jsmath::cos(t),
                    radius * crate::scientific::functions::jsmath::sin(t),
                )
            })
            .collect();
        for _ in 0..800 {
            let mut done = true;
            for i in 0..n {
                let (p, dp) = horner(&monic, z[i]);
                if p == (0.0, 0.0) {
                    continue;
                }
                let ratio = cdiv(p, dp);
                let mut sum: C64 = (0.0, 0.0);
                for (j, &zj) in z.iter().enumerate() {
                    if j != i {
                        let d = csub(z[i], zj);
                        if d != (0.0, 0.0) {
                            sum = cadd(sum, cdiv((1.0, 0.0), d));
                        }
                    }
                }
                let denom = csub((1.0, 0.0), cmul(ratio, sum));
                let w = if cabs(denom) == 0.0 {
                    ratio
                } else {
                    cdiv(ratio, denom)
                };
                if !(w.0.is_finite() && w.1.is_finite()) {
                    continue;
                }
                z[i] = csub(z[i], w);
                if cabs(w) > 4.0 * EPS * (1.0 + cabs(z[i])) {
                    done = false;
                }
            }
            if done {
                break;
            }
        }
        // Newton polish against the original coefficients, kept only while it helps.
        for zi in z.iter_mut() {
            for _ in 0..4 {
                let (p, dp) = horner(&c, *zi);
                if cabs(dp) == 0.0 {
                    break;
                }
                let next = csub(*zi, cdiv(p, dp));
                if cabs(horner(&c, next).0) < cabs(p) {
                    *zi = next;
                } else {
                    break;
                }
            }
        }
        roots.extend(z);
    }

    for r in roots.iter_mut() {
        if r.1.abs() <= 1e-9 * cabs(*r).max(1.0) {
            r.1 = 0.0;
        }
    }
    // Real coefficients: complex roots come in conjugate pairs, so make each pair exactly conjugate.
    let mut paired = vec![false; roots.len()];
    for i in 0..roots.len() {
        if roots[i].1 <= 0.0 || paired[i] {
            continue;
        }
        let conj = (roots[i].0, -roots[i].1);
        let mate = (0..roots.len())
            .filter(|&j| !paired[j] && roots[j].1 < 0.0)
            .min_by(|&j, &k| cabs(csub(roots[j], conj)).total_cmp(&cabs(csub(roots[k], conj))));
        if let Some(j) = mate {
            let re = 0.5 * (roots[i].0 + roots[j].0);
            let im = 0.5 * (roots[i].1 - roots[j].1);
            roots[i] = (re, im);
            roots[j] = (re, -im);
            paired[i] = true;
            paired[j] = true;
        }
    }
    for r in roots.iter_mut() {
        if r.0 == 0.0 {
            r.0 = 0.0; // no -0
        }
    }
    roots.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.total_cmp(&a.1)));
    roots
}

/// Eigenvalues and eigenvectors of the symmetric matrix `a`, by cyclic Jacobi rotations.
///
/// Returns the eigenvalues in descending order and the matrix whose columns are the matching unit eigenvectors.
/// Only the symmetric part of `a` is used.
pub(super) fn sym_eigen(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| 0.5 * (a[i][j] + a[j][i])).collect())
        .collect();
    let mut v = identity(n);
    for _ in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| m[i][j] * m[i][j])
            .sum();
        let total: f64 = m.iter().flatten().map(|x| x * x).sum();
        if off <= EPS * EPS * total || off == 0.0 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = m[p][q];
                if apq == 0.0 {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let cs = 1.0 / (t * t + 1.0).sqrt();
                let sn = t * cs;
                for row in m.iter_mut() {
                    let mkp = row[p];
                    let mkq = row[q];
                    row[p] = cs * mkp - sn * mkq;
                    row[q] = sn * mkp + cs * mkq;
                }
                let (rp, rq) = (m[p].clone(), m[q].clone());
                for (k, (x, y)) in rp.iter().zip(&rq).enumerate() {
                    m[p][k] = cs * x - sn * y;
                    m[q][k] = sn * x + cs * y;
                }
                for row in v.iter_mut() {
                    let vkp = row[p];
                    let vkq = row[q];
                    row[p] = cs * vkp - sn * vkq;
                    row[q] = sn * vkp + cs * vkq;
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| m[j][j].total_cmp(&m[i][i]));
    let values = order.iter().map(|&i| m[i][i]).collect();
    let vectors = (0..n)
        .map(|r| order.iter().map(|&c| v[r][c]).collect())
        .collect();
    (values, vectors)
}

/// A basis of the null space of the complex matrix `a`, by Gauss–Jordan elimination with partial pivoting.
///
/// A pivot of magnitude at most `tol` counts as zero. Each basis vector is scaled so its largest-magnitude
/// component is exactly `(1, 0)`. A matrix with full column rank gives an empty basis.
pub(super) fn complex_null_space(a: &[Vec<C64>], tol: f64) -> Vec<Vec<C64>> {
    let rows = a.len();
    let cols = a.first().map_or(0, Vec::len);
    let mut m: Vec<Vec<C64>> = a.to_vec();
    let mut pivots: Vec<usize> = Vec::new();
    let mut r = 0;
    for c in 0..cols {
        if r == rows {
            break;
        }
        let best = (r..rows).max_by(|&i, &j| cabs(m[i][c]).total_cmp(&cabs(m[j][c])));
        let Some(best) = best else { break };
        if cabs(m[best][c]) <= tol {
            for row in m.iter_mut().skip(r) {
                row[c] = (0.0, 0.0);
            }
            continue;
        }
        m.swap(r, best);
        let piv = m[r][c];
        for x in m[r].iter_mut() {
            *x = cdiv(*x, piv);
        }
        m[r][c] = (1.0, 0.0);
        let pivot_row = m[r].clone();
        for (i, row) in m.iter_mut().enumerate() {
            let f = row[c];
            if i != r && f != (0.0, 0.0) {
                for (x, &p) in row.iter_mut().zip(&pivot_row) {
                    *x = csub(*x, cmul(f, p));
                }
                row[c] = (0.0, 0.0);
            }
        }
        pivots.push(c);
        r += 1;
    }
    let mut basis = Vec::new();
    for free in (0..cols).filter(|c| !pivots.contains(c)) {
        let mut v = vec![(0.0, 0.0); cols];
        v[free] = (1.0, 0.0);
        for (row, &pc) in pivots.iter().enumerate() {
            let x = m[row][free];
            v[pc] = (-x.0, -x.1);
        }
        let big = v
            .iter()
            .copied()
            .fold((0.0, 0.0), |b, x| if cabs(x) > cabs(b) { x } else { b });
        let mut v: Vec<C64> = v.into_iter().map(|x| cdiv(x, big)).collect();
        if let Some(i) = v.iter().position(|&x| cabs(csub(x, (1.0, 0.0))) < 1e-15) {
            v[i] = (1.0, 0.0);
        }
        basis.push(v);
    }
    basis
}

/// The full singular value decomposition A = U Σ Vᵀ of the m×n matrix `a`, by one-sided Jacobi rotations.
///
/// Returns U (m×m orthogonal), the min(m, n) singular values in descending order, and V (n×n orthogonal). When A is
/// rank-deficient or not square, the extra columns of U and V complete them to orthonormal bases. Signs are fixed
/// so the largest-magnitude entry of each column of V is positive, with U's matching column flipped alongside.
pub(super) fn svd(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<f64>, Vec<Vec<f64>>) {
    let m = a.len();
    let n = a.first().map_or(0, Vec::len);
    let (mut u, s, mut v) = if m >= n {
        svd_tall(a)
    } else {
        let at = transpose(a);
        let (u2, s2, v2) = svd_tall(&at);
        (v2, s2, u2)
    };
    let k = m.min(n);
    for j in 0..n {
        let big = (0..n)
            .map(|i| v[i][j])
            .fold(0.0_f64, |b, x| if x.abs() > b.abs() { x } else { b });
        if big < 0.0 {
            for row in v.iter_mut() {
                row[j] = -row[j];
            }
            if j < k {
                for row in u.iter_mut() {
                    row[j] = -row[j];
                }
            }
        }
    }
    (u, s, v)
}

/// The SVD of a matrix with at least as many rows as columns; see [`svd`].
fn svd_tall(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<f64>, Vec<Vec<f64>>) {
    let m = a.len();
    let n = a.first().map_or(0, Vec::len);
    let mut w: Vec<Vec<f64>> = a.to_vec();
    let mut v = identity(n);
    for _ in 0..80 {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let (mut alpha, mut beta, mut gamma) = (0.0, 0.0, 0.0);
                for row in w.iter() {
                    alpha += row[p] * row[p];
                    beta += row[q] * row[q];
                    gamma += row[p] * row[q];
                }
                if gamma == 0.0 || gamma.abs() <= EPS * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let cs = 1.0 / (1.0 + t * t).sqrt();
                let sn = cs * t;
                for row in w.iter_mut().chain(v.iter_mut()) {
                    let x = row[p];
                    let y = row[q];
                    row[p] = cs * x - sn * y;
                    row[q] = sn * x + cs * y;
                }
            }
        }
        if !rotated {
            break;
        }
    }
    let norms: Vec<f64> = (0..n)
        .map(|j| w.iter().map(|r| r[j] * r[j]).sum::<f64>().sqrt())
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| norms[j].total_cmp(&norms[i]));
    let s: Vec<f64> = order.iter().map(|&j| norms[j]).collect();
    let vs: Vec<Vec<f64>> = (0..n)
        .map(|r| order.iter().map(|&c| v[r][c]).collect())
        .collect();
    let smax = s.first().copied().unwrap_or(0.0);
    let cutoff = (m.max(n) as f64) * EPS * smax;
    // Columns of U for the nonzero singular values, then completed to an orthonormal basis of R^m.
    let mut ucols: Vec<Vec<f64>> = Vec::new();
    for (idx, &j) in order.iter().enumerate() {
        if s[idx] > cutoff && s[idx] > 0.0 {
            ucols.push(w.iter().map(|r| r[j] / s[idx]).collect());
        } else {
            break;
        }
    }
    let rank = ucols.len();
    let mut s = s;
    for x in s.iter_mut().skip(rank) {
        *x = 0.0;
    }
    complete_basis(&mut ucols, m);
    let u: Vec<Vec<f64>> = (0..m)
        .map(|r| ucols.iter().map(|c| c[r]).collect())
        .collect();
    (u, s, vs)
}

/// Extends the orthonormal vectors `cols` (each of length `dim`) to an orthonormal basis of R^dim.
fn complete_basis(cols: &mut Vec<Vec<f64>>, dim: usize) {
    for e in 0..dim {
        if cols.len() >= dim {
            break;
        }
        let mut x = vec![0.0; dim];
        x[e] = 1.0;
        for _ in 0..2 {
            for c in cols.iter() {
                let d: f64 = c.iter().zip(&x).map(|(a, b)| a * b).sum();
                for (xi, ci) in x.iter_mut().zip(c) {
                    *xi -= d * ci;
                }
            }
        }
        let nrm = x.iter().map(|t| t * t).sum::<f64>().sqrt();
        if nrm > 1e-8 {
            cols.push(x.into_iter().map(|t| t / nrm).collect());
        }
    }
}

/// The matrix exponential e^A of the square matrix `a`, by scaling and squaring with a Taylor approximant.
///
/// A is halved until its 1-norm is at most 1/2, the Taylor series is summed to machine precision, and the result is
/// squared back up.
pub(super) fn expm(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let norm = norm1(a);
    let mut squarings = 0;
    if norm > 0.5 {
        squarings = crate::scientific::functions::jsmath::log2(norm / 0.5).ceil() as i32;
    }
    let scale = 0.5_f64.powi(squarings);
    let b: Vec<Vec<f64>> = a
        .iter()
        .map(|r| r.iter().map(|x| x * scale).collect())
        .collect();
    let mut sum = identity(n);
    let mut term = identity(n);
    for k in 1..=30 {
        term = matmul(&term, &b);
        let inv = 1.0 / k as f64;
        for row in term.iter_mut() {
            for x in row.iter_mut() {
                *x *= inv;
            }
        }
        for (srow, trow) in sum.iter_mut().zip(&term) {
            for (s, t) in srow.iter_mut().zip(trow) {
                *s += t;
            }
        }
        if norm1(&term) <= EPS * norm1(&sum) {
            break;
        }
    }
    for _ in 0..squarings {
        sum = matmul(&sum, &sum);
    }
    sum
}

/// The principal square root of the square matrix `a`, by Denman–Beavers iteration.
///
/// None when the iteration fails to converge or a step meets a singular matrix (as with a negative real eigenvalue,
/// whose principal root is not real, or a singular `a` other than zero), or when the result doesn't square back to A.
pub(super) fn sqrtm(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    if a.iter().flatten().all(|&x| x == 0.0) {
        return Some(vec![vec![0.0; n]; n]);
    }
    let mut y: Vec<Vec<f64>> = a.to_vec();
    let mut z = identity(n);
    let mut converged = false;
    for _ in 0..100 {
        let yi = inverse(&y)?;
        let zi = inverse(&z)?;
        let ny: Vec<Vec<f64>> = avg(&y, &zi);
        let nz: Vec<Vec<f64>> = avg(&z, &yi);
        let diff: Vec<Vec<f64>> = ny
            .iter()
            .zip(&y)
            .map(|(r, s)| r.iter().zip(s).map(|(p, q)| p - q).collect())
            .collect();
        let change = norm1(&diff);
        y = ny;
        z = nz;
        if !y.iter().flatten().all(|x| x.is_finite()) {
            return None;
        }
        if change <= 1e-14 * norm1(&y).max(1e-300) {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }
    let sq = matmul(&y, &y);
    let resid: f64 = norm1(
        &sq.iter()
            .zip(a)
            .map(|(r, s)| r.iter().zip(s).map(|(p, q)| p - q).collect())
            .collect::<Vec<Vec<f64>>>(),
    );
    (resid <= 1e-8 * norm1(a).max(1.0)).then_some(y)
}

/// The n×n identity.
fn identity(n: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect()
}

/// The transpose of `a`.
fn transpose(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let cols = a.first().map_or(0, Vec::len);
    (0..cols)
        .map(|j| a.iter().map(|r| r[j]).collect())
        .collect()
}

/// The product `a · b`.
fn matmul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let cols = b.first().map_or(0, Vec::len);
    a.iter()
        .map(|r| {
            (0..cols)
                .map(|j| r.iter().zip(b).map(|(x, brow)| x * brow[j]).sum())
                .collect()
        })
        .collect()
}

/// The entrywise mean (a + b)/2.
fn avg(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    a.iter()
        .zip(b)
        .map(|(r, s)| r.iter().zip(s).map(|(p, q)| 0.5 * (p + q)).collect())
        .collect()
}

/// The 1-norm (largest absolute column sum) of `a`.
fn norm1(a: &[Vec<f64>]) -> f64 {
    let cols = a.first().map_or(0, Vec::len);
    (0..cols)
        .map(|j| a.iter().map(|r| r[j].abs()).sum::<f64>())
        .fold(0.0, f64::max)
}

/// The inverse of the square matrix `a` by Gauss–Jordan with partial pivoting; None when it is (numerically)
/// singular.
fn inverse(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let scale = a.iter().flatten().fold(0.0_f64, |m, x| m.max(x.abs()));
    if scale == 0.0 {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut inv = identity(n);
    for c in 0..n {
        let best = (c..n).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[best][c].abs() <= 1e-14 * scale {
            return None;
        }
        m.swap(c, best);
        inv.swap(c, best);
        let p = m[c][c];
        for k in 0..n {
            m[c][k] /= p;
            inv[c][k] /= p;
        }
        for i in 0..n {
            if i != c {
                let f = m[i][c];
                if f != 0.0 {
                    for k in 0..n {
                        m[i][k] -= f * m[c][k];
                        inv[i][k] -= f * inv[c][k];
                    }
                }
            }
        }
    }
    Some(inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    fn mclose(a: &[Vec<f64>], b: &[Vec<f64>], tol: f64) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(r, s)| {
                r.len() == s.len() && r.iter().zip(s).all(|(x, y)| close(*x, *y, tol))
            })
    }

    fn is_orthogonal(q: &[Vec<f64>]) -> bool {
        let qtq = matmul(&transpose(q), q);
        mclose(&qtq, &identity(q.len()), 1e-12)
    }

    #[test]
    fn quadratic_roots() {
        // x^2 - 5x - 2: (5 ± √33)/2
        let r = poly_roots(&[-2.0, -5.0, 1.0]);
        let s = 33f64.sqrt();
        assert_eq!(r.len(), 2);
        assert!(close(r[0].0, (5.0 + s) / 2.0, 1e-13) && r[0].1 == 0.0);
        assert!(close(r[1].0, (5.0 - s) / 2.0, 1e-13) && r[1].1 == 0.0);
    }

    #[test]
    fn cubic_with_complex_pair() {
        // (x-1)(x^2+1) = x^3 - x^2 + x - 1
        let r = poly_roots(&[-1.0, 1.0, -1.0, 1.0]);
        assert_eq!(r.len(), 3);
        assert!(close(r[0].0, 1.0, 1e-13) && r[0].1 == 0.0);
        assert!(close(r[1].0, 0.0, 1e-13) && close(r[1].1, 1.0, 1e-13));
        assert!(close(r[2].0, 0.0, 1e-13) && close(r[2].1, -1.0, 1e-13));
    }

    #[test]
    fn repeated_and_zero_roots() {
        // (x-2)^3 = x^3 - 6x^2 + 12x - 8
        let r = poly_roots(&[-8.0, 12.0, -6.0, 1.0]);
        assert_eq!(r.len(), 3);
        for z in &r {
            assert!(cabs(csub(*z, (2.0, 0.0))) < 1e-4, "{r:?}");
        }
        // x^2 (x + 3), with trailing zeros to trim
        let r = poly_roots(&[0.0, 0.0, 3.0, 1.0, 0.0, 0.0]);
        assert_eq!(r, vec![(0.0, 0.0), (0.0, 0.0), (-3.0, 0.0)]);
        assert!(poly_roots(&[5.0]).is_empty());
        assert!(poly_roots(&[]).is_empty());
    }

    #[test]
    fn high_degree_roots() {
        // (x-1)(x-2)...(x-10)
        let mut c = vec![1.0];
        for k in 1..=10 {
            let mut next = vec![0.0; c.len() + 1];
            for (i, &x) in c.iter().enumerate() {
                next[i + 1] += x;
                next[i] -= k as f64 * x;
            }
            c = next;
        }
        let r = poly_roots(&c);
        for (i, z) in r.iter().enumerate() {
            assert!(close(z.0, (10 - i) as f64, 1e-6) && z.1 == 0.0, "{r:?}");
        }
        // x^20 - 1: the 20th roots of unity
        let mut c = vec![0.0; 21];
        c[0] = -1.0;
        c[20] = 1.0;
        let r = poly_roots(&c);
        assert_eq!(r.len(), 20);
        for z in &r {
            assert!(close(cabs(*z), 1.0, 1e-12));
        }
        assert_eq!(r[0], (1.0, 0.0));
    }

    #[test]
    fn symmetric_eigen() {
        let a = vec![vec![2.0, 1.0], vec![1.0, 2.0]];
        let (vals, vecs) = sym_eigen(&a);
        assert!(close(vals[0], 3.0, 1e-13) && close(vals[1], 1.0, 1e-13));
        let h = 0.5f64.sqrt();
        assert!(close(vecs[0][0].abs(), h, 1e-13) && close(vecs[1][0].abs(), h, 1e-13));
        assert!(close(vecs[0][0] * vecs[1][0], 0.5, 1e-13));
        let a = vec![
            vec![4.0, 1.0, 2.0],
            vec![1.0, 3.0, 0.0],
            vec![2.0, 0.0, 5.0],
        ];
        let (vals, vecs) = sym_eigen(&a);
        assert!(vals.windows(2).all(|w| w[0] >= w[1]));
        assert!(is_orthogonal(&vecs));
        for (j, &l) in vals.iter().enumerate() {
            let col: Vec<Vec<f64>> = (0..3).map(|i| vec![vecs[i][j]]).collect();
            let av = matmul(&a, &col);
            for i in 0..3 {
                assert!(close(av[i][0], l * col[i][0], 1e-12));
            }
        }
    }

    #[test]
    fn null_space() {
        // [[1, 2], [2, 4]]: null space spanned by (-2, 1) → scaled (1, -0.5)
        let a = vec![vec![(1.0, 0.0), (2.0, 0.0)], vec![(2.0, 0.0), (4.0, 0.0)]];
        let ns = complex_null_space(&a, 1e-10);
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0][0], (1.0, 0.0));
        assert!(close(ns[0][1].0, -0.5, 1e-15) && ns[0][1].1 == 0.0);
        // [[-i, -1], [1, -i]] (A - iI for the rotation [[0,-1],[1,0]]): null vector (1, -i) up to scale
        let a = vec![
            vec![(0.0, -1.0), (-1.0, 0.0)],
            vec![(1.0, 0.0), (0.0, -1.0)],
        ];
        let ns = complex_null_space(&a, 1e-10);
        assert_eq!(ns.len(), 1);
        let v = &ns[0];
        for row in &a {
            let s = cadd(cmul(row[0], v[0]), cmul(row[1], v[1]));
            assert!(cabs(s) < 1e-14);
        }
        assert!(v.contains(&(1.0, 0.0)));
        let id = vec![vec![(1.0, 0.0), (0.0, 0.0)], vec![(0.0, 0.0), (1.0, 0.0)]];
        assert!(complex_null_space(&id, 1e-10).is_empty());
    }

    fn reconstruct(u: &[Vec<f64>], s: &[f64], v: &[Vec<f64>], m: usize, n: usize) -> Vec<Vec<f64>> {
        let mut sig = vec![vec![0.0; n]; m];
        for (i, &x) in s.iter().enumerate() {
            sig[i][i] = x;
        }
        matmul(&matmul(u, &sig), &transpose(v))
    }

    #[test]
    fn svd_shapes() {
        for a in [
            vec![vec![3.0, 2.0, 2.0], vec![2.0, 3.0, -2.0]],
            vec![vec![3.0, 2.0], vec![2.0, 3.0], vec![2.0, -2.0]],
            vec![vec![1.0, 2.0], vec![2.0, 4.0]],
            vec![vec![0.0, 0.0], vec![0.0, 0.0]],
            vec![
                vec![1.0, 2.0, 3.0],
                vec![4.0, 5.0, 6.0],
                vec![7.0, 8.0, 10.0],
            ],
        ] {
            let (m, n) = (a.len(), a[0].len());
            let (u, s, v) = svd(&a);
            assert_eq!(
                (u.len(), u[0].len(), s.len(), v.len(), v[0].len()),
                (m, m, m.min(n), n, n)
            );
            assert!(is_orthogonal(&u) && is_orthogonal(&v), "{a:?}");
            assert!(s.windows(2).all(|w| w[0] >= w[1]) && s.iter().all(|&x| x >= 0.0));
            assert!(mclose(&reconstruct(&u, &s, &v, m, n), &a, 1e-12), "{a:?}");
            for col in transpose(&v) {
                let big = col
                    .iter()
                    .fold(0.0_f64, |b, &x| if x.abs() > b.abs() { x } else { b });
                assert!(big > 0.0);
            }
        }
        let (_, s, _) = svd(&[vec![3.0, 2.0, 2.0], vec![2.0, 3.0, -2.0]]);
        assert!(close(s[0], 5.0, 1e-13) && close(s[1], 3.0, 1e-13));
        let (_, s, _) = svd(&[vec![1.0, 2.0], vec![2.0, 4.0]]);
        assert!(close(s[0], 5.0, 1e-13) && s[1] == 0.0);
    }

    #[test]
    fn matrix_exponential() {
        let e = expm(&[vec![0.0, 1.0], vec![0.0, 0.0]]);
        assert!(mclose(&e, &[vec![1.0, 1.0], vec![0.0, 1.0]], 1e-15));
        let e = expm(&[vec![1.0, 0.0], vec![0.0, 2.0]]);
        assert!(mclose(
            &e,
            &[
                vec![crate::scientific::functions::jsmath::exp(1.0), 0.0],
                vec![0.0, crate::scientific::functions::jsmath::exp(2.0)]
            ],
            1e-13
        ));
        // Rotation generator: e^[[0,-t],[t,0]] = [[cos t, -sin t],[sin t, cos t]]
        let t = 5.0f64;
        let e = expm(&[vec![0.0, -t], vec![t, 0.0]]);
        assert!(mclose(
            &e,
            &[
                vec![
                    crate::scientific::functions::jsmath::cos(t),
                    -crate::scientific::functions::jsmath::sin(t)
                ],
                vec![
                    crate::scientific::functions::jsmath::sin(t),
                    crate::scientific::functions::jsmath::cos(t)
                ]
            ],
            1e-12
        ));
        assert!(mclose(&expm(&vec![vec![0.0; 3]; 3]), &identity(3), 0.0));
    }

    #[test]
    fn matrix_square_root() {
        let r = sqrtm(&[vec![4.0, 0.0], vec![0.0, 9.0]]).unwrap();
        assert!(mclose(&r, &[vec![2.0, 0.0], vec![0.0, 3.0]], 1e-13));
        let a = vec![vec![4.0, 1.0], vec![1.0, 3.0]];
        let r = sqrtm(&a).unwrap();
        assert!(mclose(&matmul(&r, &r), &a, 1e-12));
        let (vals, _) = sym_eigen(&r);
        assert!(vals.iter().all(|&x| x > 0.0));
        // [[1,1],[0,1]] → [[1, 1/2],[0, 1]]
        let r = sqrtm(&[vec![1.0, 1.0], vec![0.0, 1.0]]).unwrap();
        assert!(mclose(&r, &[vec![1.0, 0.5], vec![0.0, 1.0]], 1e-13));
        assert!(sqrtm(&[vec![-1.0, 0.0], vec![0.0, 4.0]]).is_none());
        assert!(sqrtm(&[vec![-1.0, 0.0], vec![0.0, -1.0]]).is_none());
        assert_eq!(
            sqrtm(&[vec![0.0, 0.0], vec![0.0, 0.0]]),
            Some(vec![vec![0.0; 2]; 2])
        );
    }
}
