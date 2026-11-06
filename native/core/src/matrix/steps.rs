//! The row operations behind `rref`, `inv`, `solve` (and `A \ b`), `det` and `rank`, each with the matrix after
//! it, the way a textbook works them: swap, scale the pivot row to 1, clear the rest of the pivot column.

use super::exact_eval::{Cells, Ev, G, XR};
use super::show::Cell;
use crate::app::typst_math::to_typst_math;
use crate::types::MatrixStep;

/// Work much past this is a wall of matrices nobody reads.
const MAX_CELLS: usize = 80;
const MAX_STEPS: usize = 40;

/// The functions that have steps, by the name the line calls.
pub(super) fn has_steps(key: &str) -> bool {
    matches!(
        key,
        "rref" | "inv" | "inverse" | "solve" | "det" | "determinant" | "rank"
    )
}

fn sub(i: usize) -> String {
    const DIGITS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
    (i + 1)
        .to_string()
        .chars()
        .map(|c| DIGITS[c.to_digit(10).unwrap_or(0) as usize])
        .collect()
}

fn plain(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// A factor written before a row: `3`, `(1/2)`, `−(1/2)`, `(k + 1)`, or nothing for 1.
fn factor_text(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('-')
        && (plain(rest) || !rest.contains([' ', '+', '-']))
    {
        return format!("−{}", factor_text(rest));
    }
    if s == "1" {
        String::new()
    } else if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        s.to_string()
    } else {
        format!("({s})")
    }
}

fn factor_typst(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('-')
        && (plain(rest) || !rest.contains([' ', '+', '-']))
    {
        return format!("-{}", factor_typst(rest));
    }
    if s == "1" {
        String::new()
    } else {
        let t = to_typst_math(s);
        if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
            format!("{t} ")
        } else {
            format!("({t}) ")
        }
    }
}

struct Log<'a, F: Cells> {
    f: &'a F,
    fmt: &'a super::show::Fmt,
    bar: Option<usize>,
    steps: Vec<MatrixStep>,
}

impl<F: Cells> Log<'_, F> {
    fn cell_text(c: &Cell) -> String {
        c.exact.clone().unwrap_or_else(|| c.dec.clone())
    }

    fn push(&mut self, op: String, op_typst: String, m: &G<F::C>) {
        let cells = self.fmt.cells(self.f, m);
        let display = format!(
            "[{}]",
            cells
                .iter()
                .map(|r| {
                    let mut out = Vec::new();
                    for (j, c) in r.iter().enumerate() {
                        if Some(j) == self.bar && j > 0 {
                            out.push("|".to_string());
                        }
                        out.push(super::exact_eval::compact(&Self::cell_text(c)));
                    }
                    out.join(" ")
                })
                .collect::<Vec<_>>()
                .join("; ")
        );
        let body = cells
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| to_typst_math(&Self::cell_text(c)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join("; ");
        let augment = match self.bar {
            Some(b) if b > 0 => format!("augment: #{b}, "),
            _ => String::new(),
        };
        self.steps.push(MatrixStep {
            op,
            op_typst,
            display,
            typst: format!("mat(delim: \"[\", {augment}{body})"),
        });
    }

    /// A closing remark, with no matrix.
    fn remark(&mut self, op: String, op_typst: String) {
        self.steps.push(MatrixStep {
            op,
            op_typst,
            ..MatrixStep::default()
        });
    }

    fn show(&self, c: &F::C) -> String {
        let cell = self.fmt.cell(self.f, c);
        Self::cell_text(&cell)
    }

    /// Elimination over the first `cols` columns. `full` makes each pivot 1 and clears above it too (Gauss–Jordan);
    /// otherwise rows below are cleared only (echelon form, as for a determinant). Returns the swaps made and the
    /// pivot columns.
    fn eliminate(&mut self, m: &mut G<F::C>, cols: usize, full: bool) -> (usize, Vec<usize>) {
        let f = self.f;
        let rows = m.len();
        let (mut r, mut swaps, mut pivots) = (0, 0, Vec::new());
        for c in 0..cols {
            if r == rows || self.steps.len() >= MAX_STEPS {
                break;
            }
            // exact fields take the first nonzero pivot (a 1 if one is there); floats the largest
            let candidates: Vec<usize> = (r..rows).filter(|&i| !f.is_zero(&m[i][c])).collect();
            let Some(&first) = candidates.first() else {
                continue;
            };
            let p = if f.is_float() {
                *candidates
                    .iter()
                    .max_by(|&&a, &&b| {
                        let (x, y) = (
                            f.approx(&m[a][c]).unwrap_or(0.0).abs(),
                            f.approx(&m[b][c]).unwrap_or(0.0).abs(),
                        );
                        x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or(&first)
            } else {
                let one = f.one();
                let unit = |i: &&usize| {
                    let d = f.sub(&m[**i][c], &one);
                    f.is_zero(&d)
                };
                *candidates.iter().find(unit).unwrap_or(&first)
            };
            if p != r {
                m.swap(p, r);
                swaps += 1;
                self.push(
                    format!("R{} ↔ R{}", sub(r), sub(p)),
                    format!("R_{} arrow.l.r R_{}", r + 1, p + 1),
                    m,
                );
            }
            if full {
                let pivot = m[r][c].clone();
                let inv = f.div(&f.one(), &pivot);
                if !f.is_zero(&f.sub(&inv, &f.one())) {
                    for x in m[r].iter_mut() {
                        *x = f.mul(x, &inv);
                    }
                    let k = self.show(&inv);
                    self.push(
                        format!("R{} → {}R{}", sub(r), factor_text(&k), sub(r)),
                        format!("R_{} arrow.l {}R_{}", r + 1, factor_typst(&k), r + 1),
                        m,
                    );
                }
            }
            let (mut ops, mut ops_typst) = (Vec::new(), Vec::new());
            let pivot = m[r][c].clone();
            let pivot_row = m[r].clone();
            for (i, row) in m.iter_mut().enumerate() {
                if i == r || (!full && i < r) || f.is_zero(&row[c]) {
                    continue;
                }
                let k = f.div(&row[c], &pivot);
                for (x, y) in row.iter_mut().zip(&pivot_row) {
                    *x = f.sub(x, &f.mul(&k, y));
                }
                // R₂ → R₂ − 3R₁, or + for a negative factor
                let neg = f.sign(&k) == Some(-1);
                let shown = self.show(&if neg { f.neg(&k) } else { k.clone() });
                let sign = if neg { "+" } else { "−" };
                let sign_typst = if neg { "+" } else { "-" };
                ops.push(format!(
                    "R{} → R{} {sign} {}R{}",
                    sub(i),
                    sub(i),
                    factor_text(&shown),
                    sub(r)
                ));
                ops_typst.push(format!(
                    "R_{} arrow.l R_{} {sign_typst} {}R_{}",
                    i + 1,
                    i + 1,
                    factor_typst(&shown),
                    r + 1
                ));
            }
            if !ops.is_empty() {
                self.push(ops.join(", "), ops_typst.join(", quad "), m);
            }
            pivots.push(c);
            r += 1;
        }
        (swaps, pivots)
    }
}

/// The steps for `key` applied to `args`, already evaluated; empty when it has none to show.
pub(super) fn run<F: Cells>(ev: &Ev<'_, '_, F>, key: &str, args: Vec<G<F::C>>, bar: Option<usize>) -> XR<Vec<MatrixStep>> {
    let f = ev.f;
    let cells: usize = args.iter().map(|g| g.len() * g.first().map_or(0, Vec::len)).sum();
    if cells > MAX_CELLS || args.is_empty() {
        return Ok(Vec::new());
    }
    let mut log = Log {
        f,
        fmt: &ev.fmt,
        bar: None,
        steps: Vec::new(),
    };
    let start = |log: &mut Log<'_, F>, m: &G<F::C>| {
        log.push("Start".into(), "\"Start\"".into(), m);
    };
    match (key, args.as_slice()) {
        ("rref", [a]) => {
            let mut m = a.clone();
            let cols = m.first().map_or(0, Vec::len);
            log.bar = bar.filter(|&b| b < cols);
            start(&mut log, &m);
            log.eliminate(&mut m, log.bar.unwrap_or(cols), true);
        }
        ("rank", [a]) => {
            let mut m = a.clone();
            let cols = m.first().map_or(0, Vec::len);
            start(&mut log, &m);
            let (_, pivots) = log.eliminate(&mut m, cols, false);
            log.remark(
                format!("rank = {} (rows with a pivot)", pivots.len()),
                format!("\"rank\" = {} quad \"(rows with a pivot)\"", pivots.len()),
            );
        }
        ("det" | "determinant", [a]) if a.len() == a.first().map_or(0, Vec::len) => {
            let mut m = a.clone();
            let n = m.len();
            start(&mut log, &m);
            let (swaps, pivots) = log.eliminate(&mut m, n, false);
            if pivots.len() < n {
                log.remark("det = 0 (a column has no pivot)".into(), "det = 0 quad \"(a column has no pivot)\"".into());
            } else {
                let diag: Vec<String> = (0..n).map(|i| log.show(&m[i][i])).collect();
                let mut d = (0..n).fold(f.one(), |acc, i| f.mul(&acc, &m[i][i]));
                if swaps % 2 == 1 {
                    d = f.neg(&d);
                }
                let product = diag
                    .iter()
                    .map(|s| if s.starts_with('-') || s.contains(' ') { format!("({s})") } else { s.clone() })
                    .collect::<Vec<_>>()
                    .join(" · ");
                let product_typst = diag
                    .iter()
                    .map(|s| {
                        let t = to_typst_math(s);
                        if s.starts_with('-') || s.contains(' ') { format!("({t})") } else { t }
                    })
                    .collect::<Vec<_>>()
                    .join(" dot ");
                let (product, product_typst) = if swaps % 2 == 1 {
                    (format!("−({product})"), format!("-({product_typst})"))
                } else {
                    (product, product_typst)
                };
                let swapped = match swaps {
                    0 => String::new(),
                    1 => " (one swap flips the sign)".into(),
                    k if k % 2 == 1 => format!(" ({k} swaps flip the sign)"),
                    k => format!(" ({k} swaps keep the sign)"),
                };
                let value = log.show(&d);
                log.remark(
                    format!("det = {product} = {value}{swapped}"),
                    format!(
                        "det = {product_typst} = {}{}",
                        to_typst_math(&value),
                        if swapped.is_empty() { String::new() } else { format!(" quad {:?}", swapped.trim()) }
                    ),
                );
            }
        }
        ("inv" | "inverse", [a]) if a.len() == a.first().map_or(0, Vec::len) => {
            let n = a.len();
            let id = super::algebra::identity(f, n);
            let mut m: G<F::C> = a.iter().zip(&id).map(|(r, i)| r.iter().chain(i).cloned().collect()).collect();
            log.bar = Some(n);
            start(&mut log, &m);
            let (_, pivots) = log.eliminate(&mut m, n, true);
            if pivots.len() < n {
                log.remark(
                    "no inverse: the left side can't become I".into(),
                    "\"no inverse: the left side can't become\" I".into(),
                );
            } else {
                log.remark(
                    "the right side is the inverse".into(),
                    "\"the right side is the inverse\"".into(),
                );
            }
        }
        ("solve", [a, b]) if a.len() == b.len() => {
            let n = a.first().map_or(0, Vec::len);
            let mut m: G<F::C> = a.iter().zip(b).map(|(r, s)| r.iter().chain(s).cloned().collect()).collect();
            log.bar = Some(n);
            start(&mut log, &m);
            log.eliminate(&mut m, n, true);
        }
        ("solve", [a]) => {
            // an augmented matrix typed whole: the last column is the right side
            let cols = a.first().map_or(0, Vec::len);
            if cols < 2 {
                return Ok(Vec::new());
            }
            let mut m = a.clone();
            log.bar = Some(cols - 1);
            start(&mut log, &m);
            log.eliminate(&mut m, cols - 1, true);
        }
        _ => return Ok(Vec::new()),
    }
    // only the starting matrix: nothing was done to it
    if log.steps.len() == 1 {
        return Ok(Vec::new());
    }
    Ok(log.steps)
}
