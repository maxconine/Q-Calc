//! The matrix functions by name, over any field. Indices are 1-based, as a textbook counts rows and columns.

use super::algebra::{self, Solution};
use super::eigen::{self, Spectrum};
use super::exact_eval::{Cells, Ev, G, X, XFail, XR, fail, ri};
use super::numeric;
use super::show::{
    Answer, Cell, Part, column as cell_column, grid_part, number_part, poly_part, text_part,
    vectors_part, verdict, words_part,
};

/// Free parameters of a general solution, in the order a textbook uses them.
const FREE: [&str; 8] = ["t", "s", "r", "u", "v", "w", "p", "q"];

fn done<C>(parts: Vec<Part>) -> X<C> {
    X::Done(Answer { parts })
}

fn takes<C>(key: &str, args: &[X<C>], n: usize, what: &str) -> XR<()> {
    if args.len() == n {
        Ok(())
    } else {
        fail(format!("{key} takes {what}"))
    }
}

/// A row or column as a flat list; None for anything else.
fn vector<C: Clone>(a: &X<C>) -> Option<Vec<C>> {
    match a {
        X::Grid(g) if g.len() == 1 => Some(g[0].clone()),
        X::Grid(g) if g[0].len() == 1 => Some(g.iter().map(|r| r[0].clone()).collect()),
        _ => None,
    }
}

fn is_row<C>(a: &X<C>) -> bool {
    matches!(a, X::Grid(g) if g.len() == 1 && g[0].len() > 1)
}

/// A vector laid out like `like`: a row stays a row.
fn shaped<C: Clone>(v: Vec<C>, like: &X<C>) -> X<C> {
    if is_row(like) {
        X::Grid(vec![v])
    } else {
        X::Grid(v.into_iter().map(|c| vec![c]).collect())
    }
}

impl<F: Cells> Ev<'_, '_, F> {
    fn index(&self, x: X<F::C>, max: usize, what: &str) -> XR<usize> {
        let c = self.scalar(x)?;
        match self.int_of(&c) {
            Some(k) if k >= 1 && k as usize <= max => Ok(k as usize - 1),
            _ => fail(format!("{what} must be a whole number from 1 to {max}")),
        }
    }

    /// The vectors a function is given: the columns of one matrix, or several vectors.
    fn columns_of(&self, key: &str, args: Vec<X<F::C>>) -> XR<Vec<Vec<F::C>>> {
        if args.len() == 1 {
            let g = self.grid(args.into_iter().next().ok_or(XFail::NotExact)?)?;
            return Ok((0..g[0].len()).map(|j| algebra::column(&g, j)).collect());
        }
        let vs = args
            .iter()
            .map(vector)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| XFail::Matrix(format!("{key} takes a matrix or some vectors")))?;
        if vs.iter().any(|v| v.len() != vs[0].len()) {
            return fail(format!("{key} needs vectors of the same length"));
        }
        Ok(vs)
    }

    fn cells(&self, g: &[Vec<F::C>]) -> Vec<Vec<Cell>> {
        self.fmt.cells(self.f, g)
    }

    fn vec_cells(&self, vs: &[Vec<F::C>]) -> Vec<Vec<Cell>> {
        vs.iter()
            .map(|v| v.iter().map(|c| self.fmt.cell(self.f, c)).collect())
            .collect()
    }

    fn float_cells(&self, g: &[Vec<f64>]) -> Vec<Vec<Cell>> {
        g.iter()
            .map(|r| r.iter().map(|v| self.fmt.ccell((*v, 0.0))).collect())
            .collect()
    }

    fn floats(&self, g: &[Vec<F::C>]) -> XR<G<f64>> {
        eigen::approx_grid(self.f, g).ok_or(XFail::NotExact)
    }

    /// `span{[1; 0], [0; 1]}`, or the zero space.
    fn span(&self, vs: &[Vec<F::C>], rows: bool) -> X<F::C> {
        if vs.is_empty() {
            return done(vec![words_part("{0} (only the zero vector)")]);
        }
        let cells = self.vec_cells(vs);
        let mut p = if rows {
            let parts: Vec<Part> = cells
                .iter()
                .map(|v| grid_part(None, std::slice::from_ref(v)))
                .collect();
            Part {
                label: None,
                display: parts
                    .iter()
                    .map(|p| p.display.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                exact: parts.iter().any(|p| p.exact.is_some()).then(|| {
                    parts
                        .iter()
                        .map(|p| p.exact.clone().unwrap_or_else(|| p.display.clone()))
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
                ..Part::default()
            }
        } else {
            vectors_part(None, &cells)
        };
        p.display = format!("span{{{}}}", p.display);
        p.exact = p.exact.map(|e| format!("span{{{e}}}"));
        // stored, a basis is the matrix with those columns (or rows)
        let g = if rows {
            vs.to_vec()
        } else {
            algebra::from_columns(vs)
        };
        p.literal = grid_part(None, &self.cells(&g)).literal;
        done(vec![p])
    }

    fn spectrum(&self, a: &[Vec<F::C>]) -> XR<Vec<eigen::Eig<F::C>>> {
        match eigen::eigenvalues(self.f, a)? {
            Spectrum::Values(v) => Ok(v),
            Spectrum::Symbolic(_) => fail(
                "eigenvectors of a matrix with symbols go up to triangular matrices; eig gives its eigenvalues",
            ),
        }
    }
}

pub(super) fn call<F: Cells>(ev: &Ev<F>, key: &str, mut args: Vec<X<F::C>>) -> XR<X<F::C>> {
    // `u · v`, `u × v` and `|A|` as typed (prep's hidden calls): the dot product of two vectors of one length, the
    // cross product of two 3-vectors, a square matrix's determinant or a vector's length; otherwise a product
    let key = match key {
        "qc_dot" | "qc_cross" if args.len() == 2 => {
            let (u, v) = (vector(&args[0]), vector(&args[1]));
            let n = if key == "qc_dot" {
                u.as_ref().map(Vec::len)
            } else {
                Some(3)
            };
            match (u, v) {
                // two rows or two columns; a column times a row is still their product
                (Some(u), Some(v))
                    if Some(u.len()) == n
                        && v.len() == u.len()
                        && is_row(&args[0]) == is_row(&args[1]) =>
                {
                    &key[3..]
                }
                _ => {
                    let b = args.remove(1);
                    return ev.mul(args.remove(0), b);
                }
            }
        }
        "qc_bars" if args.len() == 1 => match &args[0] {
            X::Grid(g) if g.len() == g[0].len() && g.len() > 1 => "det",
            X::Grid(_) => "norm",
            _ => return Err(XFail::NotExact),
        },
        k => k,
    };
    let f = ev.f;
    let fmt = &ev.fmt;
    let one = |args: &mut Vec<X<F::C>>| -> XR<X<F::C>> {
        takes(key, args, 1, "one matrix")?;
        Ok(args.remove(0))
    };
    match key {
        // ---- the float reader's functions ----
        "inv" | "inverse" => match one(&mut args)? {
            X::Num(c) if f.is_zero(&c) => fail("no inverse (it is 0)"),
            X::Num(c) => Ok(X::Num(f.div(&f.one(), &c))),
            a => Ok(X::Grid(algebra::inverse(f, &ev.square(a, "inverse")?)?)),
        },
        "det" | "determinant" => {
            let a = ev.square(one(&mut args)?, "determinant")?;
            Ok(X::Num(algebra::det(f, &a)?))
        }
        "transpose" => Ok(X::Grid(algebra::transpose(&ev.grid(one(&mut args)?)?))),
        "trace" | "tr" => Ok(X::Num(algebra::trace(
            f,
            &ev.square(one(&mut args)?, "trace")?,
        ))),
        "rank" => {
            let r = algebra::rank(f, &ev.grid(one(&mut args)?)?);
            Ok(X::Num(f.lift(&ri(r as i64))))
        }
        "rref" => Ok(X::Grid(
            algebra::reduce(f, &ev.grid(one(&mut args)?)?, true).m,
        )),
        "norm" => {
            let g = ev.grid(one(&mut args)?)?;
            let sum = g
                .iter()
                .flatten()
                .fold(f.zero(), |s, v| f.add(&s, &f.mul(v, v)));
            f.sqrt(&sum).map(X::Num).ok_or(XFail::NotExact)
        }
        "identity" | "eye" => {
            takes(key, &args, 1, "one size")?;
            let n = ev.index(args.remove(0), 20, "identity's size")?;
            Ok(X::Grid(algebra::identity(f, n + 1)))
        }
        "dot" => {
            takes(key, &args, 2, "two vectors")?;
            let (Some(u), Some(v)) = (vector(&args[0]), vector(&args[1])) else {
                return fail("dot needs two vectors");
            };
            if u.len() != v.len() {
                return fail(format!(
                    "dot needs vectors of the same length ({} and {})",
                    u.len(),
                    v.len()
                ));
            }
            Ok(X::Num(algebra::dot(f, &u, &v)))
        }
        "cross" => {
            takes(key, &args, 2, "two vectors")?;
            let (Some(u), Some(v)) = (vector(&args[0]), vector(&args[1])) else {
                return fail("cross needs two vectors of length 3");
            };
            if u.len() != 3 || v.len() != 3 {
                return fail("cross needs two vectors of length 3");
            }
            let m = |x: &F::C, y: &F::C| f.mul(x, y);
            let w = vec![
                f.sub(&m(&u[1], &v[2]), &m(&u[2], &v[1])),
                f.sub(&m(&u[2], &v[0]), &m(&u[0], &v[2])),
                f.sub(&m(&u[0], &v[1]), &m(&u[1], &v[0])),
            ];
            Ok(shaped(w, &args[0]))
        }

        // ---- eigen ----
        "eig" | "eigenvalues" | "eigvals" => {
            let a = ev.square(one(&mut args)?, "eigenvalues")?;
            Ok(match eigen::eigenvalues(f, &a)? {
                Spectrum::Values(v) => done(vec![eigen::values_part(f, fmt, &v)]),
                Spectrum::Symbolic(s) => done(vec![text_part(Some("λ = ".into()), s)]),
            })
        }
        "eigenvectors" | "eigvecs" | "eigvec" => {
            let a = ev.square(one(&mut args)?, "eigenvectors")?;
            let eigs = ev.spectrum(&a)?;
            let parts = eigs
                .iter()
                .map(|e| {
                    let c = eigen::eig_cell(f, fmt, e);
                    let vs = eigen::eigenvectors(f, fmt, &a, e);
                    let mut p = vectors_part(None, &vs);
                    p.label = Some(format!("λ = {}: ", c.dec));
                    if let Some(x) = p.exact.take().or_else(|| c.rich.then(|| p.display.clone())) {
                        p.exact = Some(x);
                        // the exact form names λ exactly too
                        p.label = Some(format!(
                            "λ = {}: ",
                            c.exact.clone().unwrap_or(c.dec.clone())
                        ));
                    }
                    p
                })
                .collect();
            Ok(done(parts))
        }
        "charpoly" | "chareq" => {
            let a = ev.square(one(&mut args)?, "the characteristic polynomial")?;
            let cp = algebra::charpoly(f, &a)?;
            let cells: Vec<Cell> = cp.iter().map(|c| fmt.cell(f, c)).collect();
            let mut p = poly_part(None, &cells, "λ");
            // the characteristic equation: det(A − λI) = 0
            if key != "chareq" {
                return Ok(done(vec![p]));
            }
            p.display.push_str(" = 0");
            p.exact = p.exact.map(|e| format!("{e} = 0"));
            // and its roots, the eigenvalues, as the next line of the working would have them
            let roots = match eigen::eigenvalues(f, &a) {
                Ok(Spectrum::Values(v)) => Some(eigen::values_part(f, fmt, &v)),
                Ok(Spectrum::Symbolic(s)) => Some(text_part(Some("λ = ".into()), s)),
                Err(_) => None,
            };
            Ok(done(std::iter::once(p).chain(roots).collect()))
        }
        "diagonalize" => {
            let a = ev.square(one(&mut args)?, "diagonalize")?;
            let eigs = ev.spectrum(&a)?;
            let (p, d) = eigen::diagonalize(f, fmt, &a, &eigs)?;
            Ok(done(vec![
                grid_part(Some("P = ".into()), &p),
                grid_part(Some("D = ".into()), &d),
            ]))
        }
        "jordan" => {
            let a = ev.square(one(&mut args)?, "the Jordan form")?;
            let eigs = ev.spectrum(&a)?;
            let j = eigen::jordan(f, fmt, &a, &eigs)?;
            Ok(done(vec![grid_part(None, &j)]))
        }

        // ---- systems ----
        "solve" | "lsq" | "leastsquares" => {
            let (a, b) = match args.len() {
                // one augmented matrix: the last column is the right side
                1 => {
                    let g = ev.grid(args.remove(0))?;
                    let n = g[0].len();
                    if n < 2 {
                        return fail(format!(
                            "{key} takes A and b, or one augmented matrix [A | b]"
                        ));
                    }
                    let b = algebra::column(&g, n - 1);
                    (
                        g.into_iter()
                            .map(|mut r| {
                                r.pop();
                                r
                            })
                            .collect::<G<F::C>>(),
                        b,
                    )
                }
                2 => {
                    let b = vector(&args[1])
                        .ok_or_else(|| XFail::Matrix(format!("{key}'s b must be a vector")))?;
                    (ev.grid(args.remove(0))?, b)
                }
                _ => {
                    return fail(format!(
                        "{key} takes A and b, or one augmented matrix [A | b]"
                    ));
                }
            };
            if a.len() != b.len() {
                return fail(format!(
                    "A has {} rows but b has {} entries",
                    a.len(),
                    b.len()
                ));
            }
            if key != "solve" {
                let at = algebra::transpose(&a);
                let ata = algebra::mul(f, &at, &a)?;
                let atb: Vec<F::C> = at.iter().map(|r| algebra::dot(f, r, &b)).collect();
                return match algebra::solve(f, &ata, &atb) {
                    Solution::Unique(x) => Ok(X::Grid(x.into_iter().map(|c| vec![c]).collect())),
                    // many least-squares solutions: the shortest one
                    _ => {
                        let p = algebra::pinv(f, &a)?;
                        let bc: G<F::C> = b.into_iter().map(|c| vec![c]).collect();
                        Ok(X::Grid(algebra::mul(f, &p, &bc)?))
                    }
                };
            }
            match algebra::solve(f, &a, &b) {
                Solution::Unique(x) => Ok(X::Grid(x.into_iter().map(|c| vec![c]).collect())),
                Solution::Inconsistent => fail(
                    "no solution: the equations contradict each other (try lsq for the closest fit)",
                ),
                Solution::Family {
                    particular,
                    directions,
                } => {
                    let p = grid_part(None, &cell_column(&ev.vec_cells(&[particular])[0]));
                    let ds: Vec<Part> = ev
                        .vec_cells(&directions)
                        .iter()
                        .map(|v| grid_part(None, &cell_column(v)))
                        .collect();
                    let zero_particular = p
                        .display
                        .split(|c: char| !c.is_ascii_digit() && c != '.')
                        .all(|d| d.is_empty() || d.chars().all(|c| c == '0' || c == '.'));
                    let write = |pick: &dyn Fn(&Part) -> String| {
                        let mut out = if zero_particular {
                            String::new()
                        } else {
                            pick(&p)
                        };
                        for (k, d) in ds.iter().enumerate() {
                            let term = format!("{}*{}", FREE[k % FREE.len()], pick(d));
                            out = if out.is_empty() {
                                term
                            } else {
                                format!("{out} + {term}")
                            };
                        }
                        out
                    };
                    let display = write(&|q: &Part| q.display.clone());
                    let rich = p.exact.is_some() || ds.iter().any(|d| d.exact.is_some());
                    let exact = rich
                        .then(|| write(&|q: &Part| q.exact.clone().unwrap_or(q.display.clone())));
                    let names = FREE[..ds.len().min(FREE.len())].join(", ");
                    Ok(done(vec![Part {
                        label: Some("x = ".into()),
                        display,
                        exact,
                        note: Some(format!("(any {names})")),
                        ..Part::default()
                    }]))
                }
            }
        }
        "coords" => {
            takes(key, &args, 2, "a vector and a basis matrix")?;
            let v = vector(&args[0])
                .ok_or_else(|| XFail::Matrix("coords takes a vector and a basis matrix".into()))?;
            let b = ev.grid(args.remove(1))?;
            match algebra::solve(f, &b, &v) {
                Solution::Unique(x) => Ok(X::Grid(x.into_iter().map(|c| vec![c]).collect())),
                Solution::Inconsistent => fail("that vector isn't in the span of the basis"),
                Solution::Family { .. } => fail("the basis vectors aren't independent"),
            }
        }

        // ---- subspaces ----
        "null" | "nullspace" | "kernel" => {
            let g = ev.grid(one(&mut args)?)?;
            Ok(ev.span(&algebra::null_space(f, &g), false))
        }
        "leftnull" => {
            let g = ev.grid(one(&mut args)?)?;
            Ok(ev.span(&algebra::null_space(f, &algebra::transpose(&g)), false))
        }
        "nullity" => {
            let g = ev.grid(one(&mut args)?)?;
            let n = g[0].len() - algebra::rank(f, &g);
            Ok(X::Num(f.lift(&ri(n as i64))))
        }
        "col" | "colspace" => {
            if args.len() == 2 {
                let g = ev.grid(args.remove(0))?;
                let j = ev.index(args.remove(0), g[0].len(), "the column")?;
                return Ok(X::Grid(
                    algebra::column(&g, j)
                        .into_iter()
                        .map(|c| vec![c])
                        .collect(),
                ));
            }
            let g = ev.grid(one(&mut args)?)?;
            Ok(ev.span(&algebra::pivot_columns(f, &g), false))
        }
        "row" | "rowspace" => {
            if args.len() == 2 {
                let g = ev.grid(args.remove(0))?;
                let i = ev.index(args.remove(0), g.len(), "the row")?;
                return Ok(X::Grid(vec![g[i].clone()]));
            }
            let g = ev.grid(one(&mut args)?)?;
            Ok(ev.span(&algebra::row_basis(f, &g), true))
        }
        "basis" => {
            let vs = ev.columns_of(key, args)?;
            let g = algebra::from_columns(&vs);
            Ok(ev.span(&algebra::pivot_columns(f, &g), false))
        }
        "orth" | "gramschmidt" => {
            let vs = ev.columns_of(key, args)?;
            let q = algebra::orthogonalize(f, &vs)
                .iter()
                .map(|v| algebra::unit(f, v))
                .collect::<XR<Vec<_>>>()?;
            if q.is_empty() {
                return fail("only zero vectors: nothing to orthonormalize");
            }
            Ok(X::Grid(algebra::from_columns(&q)))
        }
        "proj" => {
            takes(
                key,
                &args,
                2,
                "two vectors, or a vector and a matrix to project onto",
            )?;
            let u = vector(&args[0])
                .ok_or_else(|| XFail::Matrix("proj's first argument must be a vector".into()))?;
            let onto = match vector(&args[1]) {
                Some(v) if v.len() == u.len() => {
                    let vv = algebra::dot(f, &v, &v);
                    if f.is_zero(&vv) {
                        return fail("can't project onto the zero vector");
                    }
                    let k = f.div(&algebra::dot(f, &u, &v), &vv);
                    return Ok(shaped(v.iter().map(|x| f.mul(&k, x)).collect(), &args[0]));
                }
                Some(_) => return fail("proj needs vectors of the same length"),
                None => ev.grid(args[1].clone())?,
            };
            if onto.len() != u.len() {
                return fail("proj: the matrix's columns must be as long as the vector");
            }
            // onto the column space: A (AᵀA)⁻¹ Aᵀ u, with A's independent columns
            let a = algebra::from_columns(&algebra::pivot_columns(f, &onto));
            let at = algebra::transpose(&a);
            let uc: G<F::C> = u.into_iter().map(|c| vec![c]).collect();
            let x = algebra::mul(
                f,
                &algebra::inverse(f, &algebra::mul(f, &at, &a)?)?,
                &algebra::mul(f, &at, &uc)?,
            )?;
            let p = algebra::mul(f, &a, &x)?;
            Ok(shaped(algebra::column(&p, 0), &args[0]))
        }
        "angle" => {
            takes(key, &args, 2, "two vectors")?;
            let (Some(u), Some(v)) = (vector(&args[0]), vector(&args[1])) else {
                return fail("angle needs two vectors");
            };
            if u.len() != v.len() {
                return fail("angle needs vectors of the same length");
            }
            let uu = algebra::dot(f, &u, &u);
            let vv = algebra::dot(f, &v, &v);
            if f.is_zero(&uu) || f.is_zero(&vv) {
                return fail("the zero vector has no direction");
            }
            let uv = algebra::dot(f, &u, &v);
            let (Some(a), Some(b), Some(c)) = (f.approx(&uv), f.approx(&uu), f.approx(&vv)) else {
                return Err(XFail::NotExact);
            };
            let cos = (a / (b * c).sqrt()).clamp(-1.0, 1.0);
            // the calculator's acos, so the angle comes in the user's angle mode
            let text = match f.as_rat(&f.div(&f.mul(&uv, &uv), &f.mul(&uu, &vv))) {
                Some(c2) if c2 == ri(0) => "0".to_string(),
                Some(c2) if c2 == ri(1) => {
                    if a > 0.0 {
                        "1".into()
                    } else {
                        "-1".into()
                    }
                }
                _ => crate::jsnum::to_string(cos),
            };
            let angle = match ev.opts.scalar {
                Some(read) => read(&format!("acos({text})")).ok_or(XFail::NotExact)?,
                None => crate::scientific::functions::jsmath::acos(cos),
            };
            Ok(done(vec![text_part(
                None,
                crate::format::format_number(super::tidy(angle, 1.0), ev.opts.sig_figs),
            )]))
        }
        "unit" | "normalize" => {
            let a = one(&mut args)?;
            let v = vector(&a).ok_or_else(|| XFail::Matrix(format!("{key} takes a vector")))?;
            Ok(shaped(algebra::unit(f, &v)?, &a))
        }

        // ---- entries and builders ----
        "adj" | "adjugate" => {
            let a = ev.square(one(&mut args)?, "the adjugate")?;
            Ok(X::Grid(algebra::transpose(&algebra::cofactors(f, &a)?)))
        }
        "cofactor" | "minor" => {
            if key == "cofactor" && args.len() == 1 {
                let a = ev.square(args.remove(0), "the cofactor matrix")?;
                return Ok(X::Grid(algebra::cofactors(f, &a)?));
            }
            takes(
                key,
                &args,
                3,
                "a matrix and a row and column, like minor(A, 1, 2)",
            )?;
            let a = ev.square(args.remove(0), key)?;
            let n = a.len();
            if n < 2 {
                return fail(format!("{key} needs at least a 2×2 matrix"));
            }
            let i = ev.index(args.remove(0), n, "the row")?;
            let j = ev.index(args.remove(0), n, "the column")?;
            Ok(X::Num(if key == "minor" {
                algebra::det(f, &algebra::minor_of(&a, i, j))?
            } else {
                algebra::cofactor(f, &a, i, j)?
            }))
        }
        "entry" if args.len() == 2 => {
            // one index into a vector
            let v = vector(&args[0]).ok_or_else(|| {
                XFail::Matrix(
                    "one index is for a vector; a matrix takes a row and a column, like A[1, 2]"
                        .into(),
                )
            })?;
            let i = ev.index(args.remove(1), v.len(), "the index")?;
            Ok(X::Num(v[i].clone()))
        }
        "entry" => {
            takes(
                key,
                &args,
                3,
                "a matrix and a row and column, like entry(A, 1, 2)",
            )?;
            let g = ev.grid(args.remove(0))?;
            let i = ev.index(args.remove(0), g.len(), "the row")?;
            let j = ev.index(args.remove(0), g[0].len(), "the column")?;
            Ok(X::Num(g[i][j].clone()))
        }
        "kron" | "hadamard" | "augment" | "stack" => {
            takes(key, &args, 2, "two matrices")?;
            let b = ev.grid(args.remove(1))?;
            let a = ev.grid(args.remove(0))?;
            let (m, n, p, q) = (a.len(), a[0].len(), b.len(), b[0].len());
            match key {
                "kron" => Ok(X::Grid(
                    (0..m * p)
                        .map(|i| {
                            (0..n * q)
                                .map(|j| f.mul(&a[i / p][j / q], &b[i % p][j % q]))
                                .collect()
                        })
                        .collect(),
                )),
                "hadamard" => {
                    if m != p || n != q {
                        return fail("hadamard needs matrices of the same size");
                    }
                    Ok(X::Grid(
                        a.iter()
                            .zip(&b)
                            .map(|(r, s)| r.iter().zip(s).map(|(x, y)| f.mul(x, y)).collect())
                            .collect(),
                    ))
                }
                "augment" => {
                    if m != p {
                        return fail("augment needs matrices with the same number of rows");
                    }
                    Ok(X::Grid(
                        a.into_iter()
                            .zip(b)
                            .map(|(mut r, s)| {
                                r.extend(s);
                                r
                            })
                            .collect(),
                    ))
                }
                _ => {
                    if n != q {
                        return fail("stack needs matrices with the same number of columns");
                    }
                    Ok(X::Grid(a.into_iter().chain(b).collect()))
                }
            }
        }
        "zeros" | "ones" => {
            if args.is_empty() || args.len() > 2 {
                return fail(format!("{key} takes a size, like {key}(2, 3)"));
            }
            let m = ev.index(args.remove(0), 20, "the size")? + 1;
            let n = if args.is_empty() {
                m
            } else {
                ev.index(args.remove(0), 20, "the size")? + 1
            };
            let v = if key == "zeros" { f.zero() } else { f.one() };
            Ok(X::Grid(vec![vec![v; n]; m]))
        }
        "diag" => {
            let a = one(&mut args)?;
            if let Some(v) = vector(&a) {
                let n = v.len();
                return Ok(X::Grid(
                    (0..n)
                        .map(|i| {
                            (0..n)
                                .map(|j| if i == j { v[i].clone() } else { f.zero() })
                                .collect()
                        })
                        .collect(),
                ));
            }
            let g = ev.square(a, "diag")?;
            Ok(X::Grid(
                (0..g.len()).map(|i| vec![g[i][i].clone()]).collect(),
            ))
        }

        // ---- checks ----
        "issymmetric" => {
            let g = ev.grid(one(&mut args)?)?;
            Ok(done(vec![verdict(
                g.len() == g[0].len() && algebra::is_symmetric(f, &g),
                None,
            )]))
        }
        "isorthogonal" => {
            let g = ev.grid(one(&mut args)?)?;
            let n = g.len();
            if n != g[0].len() {
                return Ok(done(vec![verdict(false, Some("not square".into()))]));
            }
            let qtq = algebra::mul(f, &algebra::transpose(&g), &g)?;
            let id = algebra::identity(f, n);
            Ok(done(vec![verdict(
                algebra::sub(f, &qtq, &id)
                    .iter()
                    .flatten()
                    .all(|c| f.is_zero(c)),
                None,
            )]))
        }
        "isinvertible" => {
            let a = ev.square(one(&mut args)?, "isinvertible")?;
            let d = algebra::det(f, &a)?;
            let c = fmt.cell(f, &d);
            Ok(done(vec![verdict(
                !f.is_zero(&d),
                Some(format!("det = {}", c.exact.unwrap_or(c.dec))),
            )]))
        }
        "isdiagonalizable" => {
            let a = ev.square(one(&mut args)?, "isdiagonalizable")?;
            let eigs = ev.spectrum(&a)?;
            Ok(done(vec![match eigen::diagonalize(f, fmt, &a, &eigs) {
                Ok(_) => verdict(true, None),
                Err(XFail::Matrix(m)) => verdict(
                    false,
                    m.strip_prefix("not diagonalizable: ").map(str::to_string),
                ),
                Err(e) => return Err(e),
            }]))
        }
        "isposdef" | "posdef" => {
            let a = ev.square(one(&mut args)?, "isposdef")?;
            if !algebra::is_symmetric(f, &a) {
                return Ok(done(vec![verdict(false, Some("not symmetric".into()))]));
            }
            // every leading principal minor positive (Sylvester)
            for k in 1..=a.len() {
                let lead: G<F::C> = a[..k].iter().map(|r| r[..k].to_vec()).collect();
                match f.sign(&algebra::det(f, &lead)?) {
                    Some(1) => {}
                    Some(_) => {
                        return Ok(done(vec![verdict(
                            false,
                            Some(format!("the leading {k}×{k} minor isn't positive")),
                        )]));
                    }
                    None => return Err(XFail::NotExact),
                }
            }
            Ok(done(vec![verdict(true, None)]))
        }
        "islinind" | "independent" => {
            let vs = ev.columns_of(key, args)?;
            let r = algebra::rank(f, &algebra::from_columns(&vs));
            Ok(done(vec![verdict(
                r == vs.len(),
                Some(format!("rank {r} of {}", vs.len())),
            )]))
        }

        // ---- decompositions ----
        "lu" => {
            let g = ev.grid(one(&mut args)?)?;
            let (p, l, u) = algebra::lu(f, &g);
            let mut parts = Vec::new();
            if let Some(p) = p {
                parts.push(grid_part(Some("P = ".into()), &ev.cells(&p)));
            }
            parts.push(grid_part(Some("L = ".into()), &ev.cells(&l)));
            parts.push(grid_part(Some("U = ".into()), &ev.cells(&u)));
            Ok(done(parts))
        }
        "qr" => {
            let g = ev.grid(one(&mut args)?)?;
            let cols: Vec<Vec<F::C>> = (0..g[0].len()).map(|j| algebra::column(&g, j)).collect();
            let orth = algebra::orthogonalize(f, &cols);
            if orth.len() < cols.len() {
                return fail("qr needs independent columns");
            }
            let q = algebra::from_columns(
                &orth
                    .iter()
                    .map(|v| algebra::unit(f, v))
                    .collect::<XR<Vec<_>>>()?,
            );
            let r = algebra::mul(f, &algebra::transpose(&q), &g)?;
            Ok(done(vec![
                grid_part(Some("Q = ".into()), &ev.cells(&q)),
                grid_part(Some("R = ".into()), &ev.cells(&r)),
            ]))
        }
        "svd" => {
            let g = ev.floats(&ev.grid(one(&mut args)?)?)?;
            let (u, s, v) = numeric::svd(&g);
            let (m, n) = (g.len(), g[0].len());
            let sigma: G<f64> = (0..m)
                .map(|i| (0..n).map(|j| if i == j { s[i] } else { 0.0 }).collect())
                .collect();
            Ok(done(vec![
                grid_part(Some("U = ".into()), &ev.float_cells(&u)),
                grid_part(Some("Σ = ".into()), &ev.float_cells(&sigma)),
                grid_part(Some("V = ".into()), &ev.float_cells(&v)),
            ]))
        }
        "cond" => {
            let g = ev.floats(&ev.grid(one(&mut args)?)?)?;
            let (_, s, _) = numeric::svd(&g);
            let (max, min) = (
                s.first().copied().unwrap_or(0.0),
                s.last().copied().unwrap_or(0.0),
            );
            if min <= 1e-14 * max {
                return Ok(done(vec![words_part("∞ (singular)")]));
            }
            Ok(done(vec![number_part(None, &fmt.ccell((max / min, 0.0)))]))
        }
        "cholesky" | "chol" => {
            let a = ev.square(one(&mut args)?, "cholesky")?;
            Ok(X::Grid(algebra::cholesky(f, &a)?))
        }
        "pinv" => Ok(X::Grid(algebra::pinv(f, &ev.grid(one(&mut args)?)?)?)),
        "expm" | "sqrtm" => {
            let a = ev.square(one(&mut args)?, key)?;
            let g = ev.floats(&a)?;
            let out = if key == "expm" {
                numeric::expm(&g)
            } else {
                numeric::sqrtm(&g).ok_or_else(|| {
                    XFail::Matrix(
                        "no real square root (a negative or repeated zero eigenvalue)".into(),
                    )
                })?
            };
            Ok(done(vec![grid_part(None, &ev.float_cells(&out))]))
        }
        _ => Err(XFail::NotExact),
    }
}
