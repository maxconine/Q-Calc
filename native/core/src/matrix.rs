//! The matrix reader: `src/engine/matrix.ts`. Lines with a 2D literal (`[1 2; 3 4]`, `[[1,2],[3,4]]`), a spaced
//! row (`[1 2 3]`), a matrix function (`det`, `inv`, `rref`, `dot`, `cross`…), a word form (`the inverse of A`) or a
//! transpose mark (`A^T`, `Aᵀ`, `A'`, `A⁻¹`) are worked out here with the TS's own elimination, not mathjs's.
//!
//! Quirks kept from the TS:
//! - with every matrix function name taken by user functions, the call pattern is `(?:)\s*\(` and any `(` counts;
//! - a power of a matrix past 20×20 fails with identity's size message, since the power starts from `identity(n)`;
//! - a non-name callee (never produced by the engine's text passes) gives no answer, as the TS's TypeError does.
//!
//! Indices are byte offsets into one string throughout, which matches the TS's UTF-16 arithmetic because every
//! character the index code looks at (`[`, `(`, names, digits) is ASCII.

use std::collections::BTreeMap;

use crate::format::{format_as_fraction, format_number};
use crate::jsnum;
use crate::jsre::{self, Lazy, js};
use crate::symbolic::print::{PrintOptions, to_string};
use crate::syntax::{Constant, Node, Op};

/// A matrix answer: a matrix as `[1 2; 3 4]`, a number, or a message such as `not square`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatrixAnswer {
    pub n: Option<f64>,
    /// Literal a stored matrix or `ans` is read back from; exact wherever an entry is a simple fraction.
    pub literal: Option<String>,
    pub display: String,
    /// Fraction form, when some entry isn't whole and every entry is a simple fraction.
    pub exact: Option<String>,
    pub message: bool,
}

/// The calculator's reading of an entry's text as a plain number.
pub type ScalarReader<'a> = dyn Fn(&str) -> Option<f64> + 'a;

/// `MatrixOptions`.
#[allow(missing_debug_implementations)]
pub struct MatrixOptions<'a> {
    pub variables: &'a BTreeMap<String, f64>,
    pub sig_figs: f64,
    pub fraction_mode: bool,
    /// The user's own function names, which win over `det(…)` and friends.
    pub functions: &'a [String],
    /// The calculator's reading of a plain-number entry (`sin(30)`, `sqrt(2)`); None when it can't.
    pub scalar: Option<&'a ScalarReader<'a>>,
}

type Grid = Vec<Vec<f64>>;

#[derive(Clone, Debug)]
enum M {
    Num(f64),
    Grid(Grid),
}

enum Fail {
    /// `MatrixError`: shown as a message.
    Matrix(String),
    /// `Blank`: a name nobody defined.
    Blank,
    /// Any other throw: not a matrix answer at all.
    Other,
}

type R<T> = Result<T, Fail>;

fn err<T>(msg: impl Into<String>) -> R<T> {
    Err(Fail::Matrix(msg.into()))
}

/// Denominators up to this read as fractions (`2/3`); past it an entry stays decimal.
const MAX_DEN: u32 = 1000;
/// Past this (after scaling rows alike) an inverse has too few trustworthy digits to show.
const MAX_CONDITION: f64 = 1e8;
const MAX_IDENTITY: f64 = 20.0;
/// Matrix answers are wide; a few digits per entry keep a row on screen.
const MATRIX_SIG_FIGS: f64 = 6.0;

const WORDS: &str = "inv|inverse|det|determinant|transpose|trace|tr|rank|rref|norm";
const FNS: &[&str] = &[
    "inv",
    "inverse",
    "det",
    "determinant",
    "transpose",
    "trace",
    "tr",
    "rank",
    "rref",
    "norm",
    "identity",
    "eye",
    "dot",
    "cross",
];

static TWO_D: Lazy = Lazy::new(|| js(r"\[\s*\[|\[[^\][]*;[^\][]*\]", ""));
/// `[1 2 3]`, a row the way answers write it; a list takes commas.
static SPACED_ROW: Lazy = Lazy::new(|| {
    js(
        r"\[[^\][,;]*[\d.)A-Za-z]\s+[-+]?[\d.(A-Za-z][^\][,;]*\]",
        "",
    )
});
static MARKS: Lazy = Lazy::new(|| {
    js(
        r"\^\s*\(?\s*T\s*\)?(?![A-Za-z0-9_])|ᵀ|⁻¹|[\])]\s*'|[\])]\s+(?:transpose|inverse)\b",
        "i",
    )
});
static WORD_FORM: Lazy = Lazy::new(|| {
    js(
        &format!(r"(?<![A-Za-z_])(?:the\s+)?(?:{WORDS})\s+(?:of\s+)?[\[(A-Za-z0-9]"),
        "i",
    )
});
static FN_CALL: Lazy = Lazy::new(|| fn_call_re(&[]));

fn fn_call_re(names: &[String]) -> jsre::Regex {
    let own: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let list: Vec<&str> = FNS
        .iter()
        .copied()
        .filter(|n| !own.iter().any(|o| o == n))
        .collect();
    js(&format!(r"(?<![A-Za-z_])(?:{})\s*\(", list.join("|")), "i")
}

/// A line for the matrix engine: a 2D literal, a matrix function, a word form, or a transpose mark.
pub fn looks_like_matrix(expr: &str, functions: &[String]) -> bool {
    let called = if functions.is_empty() {
        jsre::test(&FN_CALL, expr)
    } else {
        jsre::test(&fn_call_re(functions), expr)
    };
    if called {
        return true;
    }
    if !expr.contains('[') {
        return false;
    }
    jsre::test(&TWO_D, expr)
        || jsre::test(&SPACED_ROW, expr)
        || jsre::test(&MARKS, expr)
        || jsre::test(&WORD_FORM, expr)
}

fn size(a: &Grid) -> String {
    format!("{}×{}", a.len(), a.first().map_or(0, Vec::len))
}

/// `grid`: rows of finite numbers, all the same width.
fn grid(rows: Vec<Vec<f64>>) -> R<Grid> {
    if rows.is_empty() {
        return err("empty matrix");
    }
    let width = rows[0].len();
    for r in &rows {
        if r.len() != width || width == 0 {
            return err("rows differ in length");
        }
        if r.iter().any(|v| !v.is_finite()) {
            return err("an entry is undefined");
        }
    }
    Ok(rows)
}

/// A number is a 1×1 matrix wherever an operation needs one (`det(5)` is 5).
fn as_grid(a: M) -> Grid {
    match a {
        M::Num(n) => vec![vec![n]],
        M::Grid(g) => g,
    }
}

fn square(a: M, what: &str) -> R<Grid> {
    let g = as_grid(a);
    if g.len() != g[0].len() {
        return err(format!(
            "{what} needs a square matrix (this is {})",
            size(&g)
        ));
    }
    Ok(g)
}

fn add(a: M, b: M, sign: f64) -> R<M> {
    let verb = if sign > 0.0 { "add" } else { "subtract" };
    match (a, b) {
        (M::Num(x), M::Num(y)) => Ok(M::Num(x + sign * y)),
        (M::Grid(a), M::Grid(b)) => {
            if a.len() != b.len() || a[0].len() != b[0].len() {
                return err(format!(
                    "can't {verb} {} and {}: sizes must match",
                    size(&a),
                    size(&b)
                ));
            }
            Ok(M::Grid(
                a.iter()
                    .zip(&b)
                    .map(|(r, s)| r.iter().zip(s).map(|(v, w)| v + sign * w).collect())
                    .collect(),
            ))
        }
        _ => err(format!("can't {verb} a number and a matrix")),
    }
}

fn scale(a: &Grid, k: f64) -> Grid {
    a.iter()
        .map(|r| r.iter().map(|v| v * k).collect())
        .collect()
}

fn mul_grids(a: &Grid, b: &Grid) -> R<Grid> {
    let inner = a[0].len();
    if inner != b.len() {
        let hint = if a.len() == b.len() && inner == b[0].len() {
            "; for entry by entry, use dot for vectors"
        } else {
            ""
        };
        return err(format!(
            "can't multiply {} by {}: columns of the first must equal rows of the second{hint}",
            size(a),
            size(b)
        ));
    }
    Ok(a.iter()
        .map(|r| {
            (0..b[0].len())
                .map(|j| r.iter().enumerate().fold(0.0, |s, (k, v)| s + v * b[k][j]))
                .collect()
        })
        .collect())
}

fn mul(a: M, b: M) -> R<M> {
    Ok(match (a, b) {
        (M::Num(x), M::Num(y)) => M::Num(x * y),
        (M::Num(x), M::Grid(g)) | (M::Grid(g), M::Num(x)) => M::Grid(scale(&g, x)),
        (M::Grid(a), M::Grid(b)) => M::Grid(mul_grids(&a, &b)?),
    })
}

fn transpose(a: M) -> M {
    match a {
        M::Num(n) => M::Num(n),
        M::Grid(g) => M::Grid(
            (0..g[0].len())
                .map(|j| g.iter().map(|r| r[j]).collect())
                .collect(),
        ),
    }
}

/// `Math.max(...a.flat().map(Math.abs), Number.MIN_VALUE)`; a NaN entry makes it NaN, as in JS.
fn scale_of(a: &Grid) -> f64 {
    let mut m = 5e-324_f64;
    for v in a.iter().flatten() {
        let x = v.abs();
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}

struct Eliminated {
    m: Grid,
    pivots: Vec<usize>,
    det: f64,
}

/// Row echelon by partial pivoting: the reduced grid, the pivot columns, and the determinant's sign and product.
fn eliminate(src: &Grid, reduce: bool, scale_from: &Grid) -> Eliminated {
    let mut m = src.clone();
    let rows = m.len();
    let cols = m[0].len();
    let tiny = 1e-12 * scale_of(scale_from);
    let mut pivots = Vec::new();
    let mut det = 1.0;
    let mut r = 0;
    let mut c = 0;
    while c < cols && r < rows {
        let mut p = r;
        for i in r + 1..rows {
            if m[i][c].abs() > m[p][c].abs() {
                p = i;
            }
        }
        if m[p][c].abs() <= tiny {
            det = 0.0;
            for row in m.iter_mut().skip(r) {
                row[c] = 0.0;
            }
            c += 1;
            continue;
        }
        if p != r {
            m.swap(p, r);
            det = -det;
        }
        let piv = m[r][c];
        det *= piv;
        if reduce {
            m[r] = m[r].iter().map(|v| v / piv).collect();
        }
        let start = if reduce { 0 } else { r + 1 };
        for i in start..rows {
            if i == r {
                continue;
            }
            let f = m[i][c] / m[r][c];
            // `if (f)`: zero and NaN skip the row
            if f != 0.0 && !f.is_nan() {
                let pivot_row = m[r].clone();
                m[i] = m[i]
                    .iter()
                    .enumerate()
                    .map(|(k, v)| if k == c { 0.0 } else { v - f * pivot_row[k] })
                    .collect();
            }
        }
        pivots.push(c);
        r += 1;
        c += 1;
    }
    Eliminated { m, pivots, det }
}

fn det(a: M) -> R<M> {
    let sq = square(a, "determinant")?;
    Ok(M::Num(eliminate(&sq, false, &sq).det))
}

fn row_norm(g: &Grid) -> f64 {
    js_max(g.iter().map(|r| r.iter().fold(0.0, |t, v| t + v.abs())))
}

/// `Math.max(...xs)`: -∞ for none, NaN if any is NaN.
fn js_max(xs: impl Iterator<Item = f64>) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for x in xs {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}

/// ∞-norm condition number of `a` with its rows scaled to the same size, so `[1e5 0; 0 1e-5]` counts as tame.
fn condition(a: &Grid, inverse: &Grid) -> f64 {
    let row_max: Vec<f64> = a
        .iter()
        .map(|r| js_max(r.iter().map(|v| v.abs())))
        .collect();
    let s: Grid = a
        .iter()
        .enumerate()
        .map(|(i, r)| r.iter().map(|v| v / row_max[i]).collect())
        .collect();
    let si: Grid = inverse
        .iter()
        .map(|r| r.iter().enumerate().map(|(j, v)| v * row_max[j]).collect())
        .collect();
    row_norm(&s) * row_norm(&si)
}

fn inv(a: M) -> R<M> {
    if let M::Num(n) = a {
        if n == 0.0 {
            return err("no inverse (it is 0)");
        }
        return Ok(M::Num(1.0 / n));
    }
    let sq = square(a, "inverse")?;
    let n = sq.len();
    let aug: Grid = sq
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut row = r.clone();
            row.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            row
        })
        .collect();
    let e = eliminate(&aug, true, &sq);
    if e.pivots.len() < n || e.pivots[n - 1] != n - 1 {
        return err("no inverse (determinant is 0)");
    }
    let out: Grid = e.m.iter().map(|r| r[n..].to_vec()).collect();
    // `!(cond <= MAX)`: a NaN condition fails too
    let cond = condition(&sq, &out);
    if cond.is_nan() || cond > MAX_CONDITION {
        return err("no inverse (nearly singular: too close to determinant 0 to trust)");
    }
    Ok(M::Grid(out))
}

fn trace(a: M) -> R<M> {
    let sq = square(a, "trace")?;
    Ok(M::Num(
        sq.iter().enumerate().fold(0.0, |s, (i, r)| s + r[i]),
    ))
}

fn rref(a: M) -> M {
    let g = as_grid(a);
    M::Grid(eliminate(&g, true, &g).m)
}

fn rank(a: M) -> M {
    let g = as_grid(a);
    M::Num(eliminate(&g, false, &g).pivots.len() as f64)
}

fn identity(n: M) -> R<Grid> {
    match n {
        M::Num(n) if jsnum::is_integer(n) && (1.0..=MAX_IDENTITY).contains(&n) => {
            let n = n as usize;
            Ok((0..n)
                .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
                .collect())
        }
        _ => err(format!(
            "identity needs a whole size from 1 to {MAX_IDENTITY}"
        )),
    }
}

/// A row or column as a flat list; None for anything else.
fn vector(a: &M) -> Option<Vec<f64>> {
    let M::Grid(g) = a else { return None };
    if g.len() == 1 {
        return Some(g[0].clone());
    }
    if g[0].len() == 1 {
        return Some(g.iter().map(|r| r[0]).collect());
    }
    None
}

fn dot(a: M, b: M) -> R<M> {
    let (Some(u), Some(v)) = (vector(&a), vector(&b)) else {
        return err("dot needs two vectors");
    };
    if u.len() != v.len() {
        return err(format!(
            "dot needs vectors of the same length ({} and {})",
            u.len(),
            v.len()
        ));
    }
    Ok(M::Num(u.iter().zip(&v).fold(0.0, |s, (x, y)| s + x * y)))
}

fn cross(a: M, b: M) -> R<M> {
    let (Some(u), Some(v)) = (vector(&a), vector(&b)) else {
        return err("cross needs two vectors of length 3");
    };
    if u.len() != 3 || v.len() != 3 {
        return err("cross needs two vectors of length 3");
    }
    let w = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    // the answer is laid out like the first vector
    let row = matches!(&a, M::Grid(g) if g.len() == 1);
    Ok(M::Grid(if row {
        vec![w.to_vec()]
    } else {
        w.iter().map(|x| vec![*x]).collect()
    }))
}

fn norm(a: M) -> M {
    let g = as_grid(a);
    M::Num(g.iter().flatten().fold(0.0, |s, v| s + v * v).sqrt())
}

fn power(a: M, e: M) -> R<M> {
    let e = match (&a, e) {
        (M::Num(x), M::Num(y)) => {
            return Ok(M::Num(crate::eval::ops::js_pow(*x, y)));
        }
        (_, M::Grid(_)) => return err("can't raise to a matrix power"),
        (_, M::Num(e)) => e,
    };
    if !jsnum::is_integer(e) {
        return err("a matrix power must be a whole number");
    }
    let sq = square(a, "a power")?;
    let mut base = if e < 0.0 {
        as_grid(inv(M::Grid(sq.clone()))?)
    } else {
        sq.clone()
    };
    let mut out = identity(M::Num(sq.len() as f64))?;
    let mut k = e.abs();
    while k > 0.0 {
        if k % 2.0 != 0.0 {
            out = mul_grids(&out, &base)?;
        }
        base = mul_grids(&base, &base)?;
        k = (k / 2.0).floor();
    }
    Ok(M::Grid(out))
}

fn is_call(key: &str) -> bool {
    FNS.contains(&key)
}

fn call(key: &str, mut args: Vec<M>) -> R<M> {
    let first = args.remove(0);
    match key {
        "inv" | "inverse" => inv(first),
        "det" | "determinant" => det(first),
        "transpose" => Ok(transpose(first)),
        "trace" | "tr" => trace(first),
        "rank" => Ok(rank(first)),
        "rref" => Ok(rref(first)),
        "norm" => Ok(norm(first)),
        "identity" | "eye" => identity(first).map(M::Grid),
        "dot" => dot(first, args.remove(0)),
        "cross" => cross(first, args.remove(0)),
        _ => Err(Fail::Other),
    }
}

fn is_t(n: &Node) -> bool {
    matches!(n, Node::Symbol(s) if s == "T")
}

/// The name a callee answers to (`fn.name`); a non-name callee has none, where the TS throws a TypeError.
fn callee_name(callee: &Node) -> R<&str> {
    match callee {
        Node::Symbol(s) => Ok(s),
        _ => Err(Fail::Other),
    }
}

fn children(node: &Node) -> Vec<&Node> {
    match node {
        Node::Constant(_) | Node::Symbol(_) => vec![],
        Node::Operator { args, .. } => args.iter().collect(),
        Node::Function { callee, args } => std::iter::once(callee.as_ref()).chain(args).collect(),
        Node::Parenthesis(c) => vec![c],
        Node::Array(items) => items.iter().collect(),
        Node::Range { start, end, step } => {
            let mut v: Vec<&Node> = vec![start, end];
            if let Some(s) = step {
                v.push(s);
            }
            v
        }
        Node::Conditional {
            condition,
            then,
            otherwise,
        } => vec![condition, then, otherwise],
        Node::Relational { params, .. } => params.iter().collect(),
        Node::Assignment {
            target,
            index,
            value,
        } => {
            let mut v: Vec<&Node> = vec![target];
            if let Some(i) = index {
                v.extend(i);
            }
            v.push(value);
            v
        }
        Node::FunctionAssignment { body, .. } => vec![body],
        Node::Accessor { object, index, .. } => {
            std::iter::once(object.as_ref()).chain(index).collect()
        }
        Node::Block(items) => items.iter().map(|(n, _)| n).collect(),
    }
}

fn has_matrix_part(node: &Node) -> R<bool> {
    let here = match node {
        Node::Array(_) => true,
        Node::Function { callee, .. } => is_call(&callee_name(callee)?.to_lowercase()),
        Node::Operator {
            op: Op::Pow, args, ..
        } => args.get(1).is_some_and(is_t),
        _ => false,
    };
    if here {
        return Ok(true);
    }
    for c in children(node) {
        if has_matrix_part(c)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "tau" => 2.0 * std::f64::consts::PI,
        _ => return None,
    })
}

fn eval_node(node: &Node, opts: &MatrixOptions<'_>) -> R<M> {
    let vars = opts.variables;
    // an entry like `sqrt(2)` or `sin(30)` is plain calculator math
    if !matches!(node, Node::Constant(_) | Node::Symbol(_))
        && let Some(scalar) = opts.scalar
        && !has_matrix_part(node)?
    {
        return match scalar(&to_string(node, &PrintOptions::default())) {
            Some(v) if v.is_finite() => Ok(M::Num(v)),
            _ => Err(Fail::Blank),
        };
    }
    match node {
        Node::Parenthesis(c) => eval_node(c, opts),
        Node::Constant(Constant::Number { value, .. }) => Ok(M::Num(*value)),
        Node::Constant(_) => Err(Fail::Blank),
        Node::Symbol(name) => vars
            .get(name)
            .copied()
            .or_else(|| constant(name))
            .map(M::Num)
            .ok_or(Fail::Blank),
        Node::Array(items) => {
            let items = items
                .iter()
                .map(|i| eval_node(i, opts))
                .collect::<R<Vec<M>>>()?;
            if items.iter().all(|v| matches!(v, M::Num(_))) {
                // a flat list is one row
                let row = items
                    .iter()
                    .map(|v| match v {
                        M::Num(n) => *n,
                        M::Grid(_) => f64::NAN,
                    })
                    .collect();
                return Ok(M::Grid(grid(if items.is_empty() {
                    vec![]
                } else {
                    vec![row]
                })?));
            }
            // `[[1, 2], [3, 4]]`: each item is one row
            if items
                .iter()
                .all(|v| matches!(v, M::Grid(g) if g.len() == 1))
            {
                let rows = items
                    .into_iter()
                    .map(|v| match v {
                        M::Grid(mut g) => g.remove(0),
                        M::Num(_) => vec![],
                    })
                    .collect();
                return Ok(M::Grid(grid(rows)?));
            }
            err("rows differ in length")
        }
        Node::Function { callee, args } => {
            let name = callee_name(callee)?;
            // `k (…)` with a stored k multiplies
            if let Some(k) = vars.get(name)
                && args.len() == 1
            {
                return mul(M::Num(*k), eval_node(&args[0], opts)?);
            }
            let key = name.to_lowercase();
            if !is_call(&key) {
                return err(format!("{name} doesn't take a matrix"));
            }
            let want = if key == "dot" || key == "cross" { 2 } else { 1 };
            if args.len() != want {
                return err(format!(
                    "{key} takes {}",
                    if want == 1 {
                        "one matrix"
                    } else {
                        "two vectors"
                    }
                ));
            }
            let values = args
                .iter()
                .map(|a| eval_node(a, opts))
                .collect::<R<Vec<M>>>()?;
            call(&key, values)
        }
        Node::Operator { op, args, .. } => {
            let l = args.first();
            let r = args.get(1);
            if *op == Op::UnaryMinus
                && let Some(l) = l
            {
                return mul(M::Num(-1.0), eval_node(l, opts)?);
            }
            if *op == Op::UnaryPlus
                && let Some(l) = l
            {
                return eval_node(l, opts);
            }
            let (Some(l), Some(r)) = (l, r) else {
                return Err(Fail::Blank);
            };
            // `A^T` is the transpose
            if *op == Op::Pow && is_t(r) {
                return Ok(transpose(eval_node(l, opts)?));
            }
            let a = eval_node(l, opts)?;
            let b = eval_node(r, opts)?;
            match op {
                Op::Add => add(a, b, 1.0),
                Op::Subtract => add(a, b, -1.0),
                Op::Multiply => mul(a, b),
                Op::Divide => match b {
                    M::Grid(_) => err("can't divide by a matrix; multiply by its inverse instead"),
                    M::Num(0.0) => err("can't divide by 0"),
                    M::Num(k) => mul(a, M::Num(1.0 / k)),
                },
                Op::Pow => power(a, b),
                _ => err(format!("{} doesn't take a matrix", op.symbol())),
            }
        }
        _ => Err(Fail::Blank),
    }
}

/// `[1 2; 3 4]` is `[1, 2; 3, 4]`: spaces separate entries inside a bracket with no commas.
fn spaced(expr: &str) -> String {
    static BRACKET: Lazy = Lazy::new(|| js(r"\[([^\][]*)\]", ""));
    static GAP: Lazy = Lazy::new(|| js(r"(?<=[\d.)A-Za-z])\s+(?=[\d.(A-Za-z]|[-+][\d.(])", ""));
    jsre::replace_all(&BRACKET, expr, |c| {
        let body = jsre::group(c, 1);
        if body.contains(',') {
            return jsre::group(c, 0).to_string();
        }
        format!("[{}]", jsre::replace_all(&GAP, body, |_| ", ".into()))
    })
    .into_owned()
}

/// The end of the operand starting at `i`: a bracket or paren group, or a name or number.
fn operand_end(s: &str, i: usize) -> Option<usize> {
    static OPERAND: Lazy = Lazy::new(|| js(r"^[A-Za-z_][A-Za-z0-9_]*|^\d+(?:\.\d+)?", ""));
    let bytes = s.as_bytes();
    let open = *bytes.get(i)?;
    if open == b'[' || open == b'(' {
        let close = if open == b'[' { b']' } else { b')' };
        let mut depth = 0;
        for (k, &ch) in bytes.iter().enumerate().skip(i) {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
        }
        return None;
    }
    let m = OPERAND.find(&s[i..]).ok().flatten()?;
    Some(i + m.end())
}

/// `det A`, `the inverse of A`, `transpose [1 2; 3 4]` are calls; `A transpose` and `A inverse` are marks.
fn word_forms(expr: &str) -> String {
    static WORD: Lazy = Lazy::new(|| {
        js(
            &format!(r"(?<![A-Za-z_])(?:the\s+)?({WORDS})\s+(?:of\s+)?(?=[\[(A-Za-z0-9])"),
            "i",
        )
    });
    static TRANSPOSE: Lazy = Lazy::new(|| js(r"([\])A-Za-z0-9])\s+transpose\b", "i"));
    static INVERSE: Lazy = Lazy::new(|| js(r"([\])A-Za-z0-9])\s+inverse\b", "i"));
    let mut out = expr.to_string();
    for _ in 0..8 {
        let Ok(Some(m)) = WORD.captures(out.as_str()) else {
            break;
        };
        let whole = m.get(0).expect("match");
        let (index, start) = (whole.start(), whole.end());
        let word = jsre::group(&m, 1).to_lowercase();
        let Some(end) = operand_end(&out, start) else {
            break;
        };
        out = format!(
            "{}{word}({}){}",
            &out[..index],
            &out[start..end],
            &out[end..]
        );
    }
    let out = jsre::replace_all(&TRANSPOSE, &out, |c| format!("{}^T", jsre::group(c, 1)));
    jsre::replace_all(&INVERSE, &out, |c| format!("{}^(-1)", jsre::group(c, 1))).into_owned()
}

fn prep(expr: &str) -> String {
    static SCALE: Lazy = Lazy::new(|| js(r"(?<=[\w.)\]])\s*\[", ""));
    static PRIME: Lazy = Lazy::new(|| js(r"([\])])\s*'", ""));
    // the input's word rule (prettyTokens, QuickInput's and the native bars') turns a typed `dot(` into `·(`: a `·`
    // with nothing to multiply before it is still the dot product's call, as `dot([1; 2; 3], [4 5 6])` is 32
    static DOT_CALL: Lazy = Lazy::new(|| js(r"(^|[^\w.)\]\s])(\s*)·(?=\s*\()", ""));
    let expr = jsre::replace_all(&DOT_CALL, expr, |c| {
        format!("{}{}dot", jsre::group(c, 1), jsre::group(c, 2))
    });
    // `3[[1, 2]]` and `k [1 2]` scale; mathjs would read a bracket after a value as an index
    let s = word_forms(&spaced(&expr));
    let s = jsre::replace_all(&SCALE, &s, |_| "*[".into()).into_owned();
    let s = jsre::replace_all(&PRIME, &s, |c| format!("{}^T", jsre::group(c, 1))).into_owned();
    s.replace('ᵀ', "^T")
        .replace("⁻¹", "^(-1)")
        .replace(['−', '–'], "-")
        .replace(['×', '·'], "*")
        .replace('÷', "/")
        .replace('π', "pi")
}

fn tidy(n: f64, scale: f64) -> f64 {
    let v = if n.abs() <= 1e-12 * scale { 0.0 } else { n };
    // float noise off a whole number: -1.9999999999999996 is -2
    let whole = jsnum::round(v);
    if (v - whole).abs() <= 1e-12 * v.abs().max(1.0) {
        whole
    } else {
        v
    }
}

fn fraction(v: f64) -> Option<String> {
    if jsnum::is_integer(v) {
        Some(jsnum::to_string(v))
    } else {
        format_as_fraction(v, MAX_DEN)
    }
}

/// `[1 2; 3 4]`, a row as `[1 2 3]`, a column as `[1; 2; 3]`.
fn format_grid(a: &Grid, cell: impl Fn(f64) -> String) -> String {
    let rows: Vec<String> = a
        .iter()
        .map(|r| r.iter().map(|v| cell(*v)).collect::<Vec<_>>().join(" "))
        .collect();
    format!("[{}]", rows.join("; "))
}

fn literal_of(a: &Grid) -> String {
    let rows: Vec<String> = a
        .iter()
        .map(|r| {
            let cells: Vec<String> = r
                .iter()
                .map(|v| fraction(*v).unwrap_or_else(|| jsnum::to_string(*v)))
                .collect();
            format!("[{}]", cells.join(", "))
        })
        .collect();
    format!("[{}]", rows.join(", "))
}

/// `matrixAnswer`: None when the line isn't the matrix reader's.
pub fn matrix_answer(expr: &str, opts: &MatrixOptions<'_>) -> Option<MatrixAnswer> {
    static OPEN: Lazy = Lazy::new(|| js(r"\[[^\]]*$", ""));
    if !looks_like_matrix(expr, opts.functions) {
        return None;
    }
    let Ok(node) = crate::syntax::parse(&prep(expr)) else {
        // `[1, 2; 3]` fails in the parser; a bracket that doesn't close is still being typed
        return (jsre::test(&TWO_D, expr) && !jsre::test(&OPEN, expr)).then(|| MatrixAnswer {
            display: "rows differ in length".into(),
            message: true,
            ..MatrixAnswer::default()
        });
    };
    let value = match eval_node(&node, opts) {
        Ok(v) => v,
        Err(Fail::Matrix(msg)) => {
            return Some(MatrixAnswer {
                display: msg,
                message: true,
                ..MatrixAnswer::default()
            });
        }
        Err(Fail::Blank) => return Some(MatrixAnswer::default()),
        Err(Fail::Other) => return None,
    };
    // a row times a column is a number (`[1 2 3] * [1; 2; 3]` is 14)
    let value = match value {
        M::Grid(g) if g.len() == 1 && g[0].len() == 1 => M::Num(g[0][0]),
        v => v,
    };
    match value {
        M::Num(v) => {
            if !v.is_finite() {
                return Some(MatrixAnswer {
                    display: "undefined".into(),
                    message: true,
                    ..MatrixAnswer::default()
                });
            }
            let n = tidy(v, 1.0);
            let exact = if jsnum::is_integer(n) {
                None
            } else {
                fraction(n)
            };
            let display = match &exact {
                Some(e) if opts.fraction_mode => e.clone(),
                _ => format_number(n, opts.sig_figs),
            };
            Some(MatrixAnswer {
                exact: exact.filter(|e| *e != display),
                display,
                n: Some(n),
                ..MatrixAnswer::default()
            })
        }
        M::Grid(g) => {
            let s = scale_of(&g);
            let clean: Grid = g
                .iter()
                .map(|r| r.iter().map(|v| tidy(*v, s)).collect())
                .collect();
            let flat: Vec<f64> = clean.iter().flatten().copied().collect();
            let fractions = flat.iter().any(|v| !jsnum::is_integer(*v))
                && flat.iter().all(|v| fraction(*v).is_some());
            let exact = fractions.then(|| format_grid(&clean, |v| fraction(v).unwrap_or_default()));
            // `Math.min` keeps a NaN
            let figs = if opts.sig_figs.is_nan() {
                f64::NAN
            } else {
                opts.sig_figs.min(MATRIX_SIG_FIGS)
            };
            let display = match &exact {
                Some(e) if opts.fraction_mode => e.clone(),
                _ => format_grid(&clean, |v| format_number(v, figs)),
            };
            Some(MatrixAnswer {
                exact: exact.filter(|e| *e != display),
                display,
                literal: Some(literal_of(&clean)),
                ..MatrixAnswer::default()
            })
        }
    }
}

/// Golden replay for `engine/matrix` (only `looksLikeMatrix` is called directly by the tests).
pub fn golden_dispatch(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    let s = |i: usize| args.get(i).and_then(|a| a.as_str()).unwrap_or_default();
    match name {
        "looksLikeMatrix" => {
            let fns: Vec<String> = args
                .get(1)
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Some(serde_json::Value::Bool(looks_like_matrix(s(0), &fns)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(expr: &str) -> String {
        let vars = BTreeMap::new();
        let opts = MatrixOptions {
            variables: &vars,
            sig_figs: 12.0,
            fraction_mode: false,
            functions: &[],
            scalar: None,
        };
        matrix_answer(expr, &opts)
            .map(|m| m.display)
            .unwrap_or_default()
    }

    #[test]
    fn matrices_like_the_ts() {
        assert_eq!(shown("[1 2; 3 4]^-1"), "[-2 1; 1.5 -0.5]");
        assert_eq!(shown("det of [1 2; 3 4]"), "-2");
        assert_eq!(shown("[1 2 3] * [1; 2; 3]"), "14");
        // a column and a row of one length dot (eval.matrix, matrix.edge.test.ts)
        assert_eq!(shown("dot([1; 2; 3], [4 5 6])"), "32");
        assert_eq!(shown("dot([1 2 3], [4; 5; 6])"), "32");
        assert_eq!(shown("[1 2 3; 4 5 6]'"), "[1 4; 2 5; 3 6]");
        assert_eq!(shown("inv([1 2; 2 4])"), "no inverse (determinant is 0)");
        assert!(!looks_like_matrix("[1, 2, 3]", &[]));
        assert!(!looks_like_matrix("mean([1, 2, 3])", &[]));
    }

    /// The bar's word rule writes a typed `dot(` as `·(` (prettyTokens): still the dot product, column or row.
    #[test]
    fn dot_typed_through_the_bar() {
        let line = |t: &str| crate::evaluate::evaluate_line(t, &Default::default()).display;
        for t in [
            "dot([1; 2; 3], [4 5 6])",
            "·([1; 2; 3], [4 5 6])",
            "·([1 2 3], [4 5 6])",
            "· ([1 2 3], [4; 5; 6])",
            "x = ·([1; 2; 3], [4; 5; 6])",
        ] {
            assert_eq!(line(t), "32", "{t}");
        }
        // a `·` after a value multiplies, as before
        assert_eq!(line("[1 2 3] · [4; 5; 6]"), "32");
        assert_eq!(line("[1; 2; 3] · [4 5 6]"), "[4 5 6; 8 10 12; 12 15 18]");
        assert_eq!(line("2 · (3)"), "6");
        assert_eq!(
            line("dot([1 2], [1 2 3])"),
            "dot needs vectors of the same length (2 and 3)"
        );
    }
}
