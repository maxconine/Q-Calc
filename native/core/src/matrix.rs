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

mod algebra;
mod eigen;
mod exact_eval;
mod functions;
mod numeric;
mod show;
mod steps;

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
    /// What a stored matrix or `ans` keeps when it says more than `literal`: exact entries (`1/1009`, `sqrt(2)`,
    /// `k`) where `literal` has the TS's floats. Native only.
    pub stored: Option<String>,
    /// The answer in labeled parts (one unlabeled part for a plain matrix or number). Native only.
    pub parts: Vec<crate::types::MatrixPart>,
    /// The row operations behind it, for `rref`, `inv`, `solve`, `det` and `rank`. Native only.
    pub steps: Vec<crate::types::MatrixStep>,
}

impl MatrixAnswer {
    /// The literal a stored matrix or `ans` is read back from.
    pub fn to_store(&self) -> Option<&String> {
        self.stored.as_ref().or(self.literal.as_ref())
    }
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

/// Functions only the generic evaluator knows (native only). They count on a line with a matrix in it (or a stored
/// one), so `row`, `col` or `solve` elsewhere keep their meaning; `zeros`, `ones` and `rot` make a matrix from sizes.
const NEW_FNS: &[&str] = &[
    "eig",
    "eigenvalues",
    "eigvals",
    "eigenvectors",
    "eigvecs",
    "eigvec",
    "charpoly",
    "chareq",
    "diagonalize",
    "jordan",
    "solve",
    "lsq",
    "leastsquares",
    "coords",
    "null",
    "nullspace",
    "kernel",
    "leftnull",
    "nullity",
    "col",
    "colspace",
    "row",
    "rowspace",
    "basis",
    "orth",
    "gramschmidt",
    "proj",
    "angle",
    "unit",
    "normalize",
    "adj",
    "adjugate",
    "cofactor",
    "minor",
    "entry",
    "kron",
    "hadamard",
    "augment",
    "stack",
    "zeros",
    "ones",
    "diag",
    "issymmetric",
    "isorthogonal",
    "isinvertible",
    "isdiagonalizable",
    "isposdef",
    "posdef",
    "islinind",
    "independent",
    "lu",
    "qr",
    "svd",
    "cond",
    // prep's own: `u · v`, `u × v`, `|A|`
    "qc_dot",
    "qc_cross",
    "qc_bars",
    "cholesky",
    "chol",
    "pinv",
    "expm",
    "sqrtm",
];
/// New names that make a matrix with no matrix typed.
const MAKERS: &str = "zeros|ones|rot";
/// New names that read as words: `eigenvalues of A`, `null A`.
const NEW_WORDS: &str = "eigenvalues|eigenvectors|eig|charpoly|chareq|diagonalize|jordan|null|nullspace|kernel|leftnull|\
nullity|colspace|rowspace|adj|adjugate|pinv|svd|lu|qr|cholesky|cond|expm|orth|gramschmidt|isinvertible|\
isdiagonalizable|issymmetric|isorthogonal|isposdef";

static NEW_CALL: Lazy = Lazy::new(|| new_call_re(&[]));
static MAKER_CALL: Lazy = Lazy::new(|| js(&format!(r"(?<![A-Za-z_])(?:{MAKERS})\s*\("), "i"));
static NEW_WORD_FORM: Lazy = Lazy::new(|| {
    js(
        &format!(r"(?<![A-Za-z_])(?:the\s+)?(?:{NEW_WORDS})\s+(?:of\s+)?[\[(A-Za-z0-9]"),
        "i",
    )
});

fn new_call_re(names: &[String]) -> jsre::Regex {
    let own: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let list: Vec<&str> = NEW_FNS
        .iter()
        .copied()
        .filter(|n| !own.iter().any(|o| o == n))
        .collect();
    js(&format!(r"(?<![A-Za-z_])(?:{})\s*\(", list.join("|")), "i")
}

/// Textbook phrases as the names they mean: `the null space of A` is `null of A`.
/// Whole-line questions as a textbook asks them, made calls: `projection of u onto v`, `angle between u and v`,
/// `the dot product of u and v`, `magnitude of v`, `unit vector v`, `orthonormal basis of A`, `gram schmidt A`,
/// `basis for the null space of A`, `dim null A`, `least squares A b`, `is A invertible?`, `are … linearly
/// independent?` (native only).
fn sentences(expr: &str) -> String {
    const ARG: &str = r"(.+?)";
    static RULES: std::sync::LazyLock<Vec<(jsre::Regex, &'static str)>> = std::sync::LazyLock::new(
        || {
            let r = |p: String, to: &'static str| {
                (
                    js(
                        &format!(
                            r"^\s*(?:find\s+|compute\s+|what\s+is\s+|what's\s+)?(?:the\s+|an?\s+)?{p}\s*\??\s*$"
                        ),
                        "i",
                    ),
                    to,
                )
            };
            vec![
                r(
                    format!(r"(?:vector\s+)?proj(?:ection)?\s+of\s+{ARG}\s+(?:onto|on)\s+{ARG}"),
                    "proj",
                ),
                r(format!(r"angle\s+between\s+{ARG}\s+and\s+{ARG}"), "angle"),
                r(
                    format!(r"(?:dot|inner|scalar)\s+product\s+of\s+{ARG}\s+and\s+{ARG}"),
                    "dot",
                ),
                r(
                    format!(r"(?:cross|vector)\s+product\s+of\s+{ARG}\s+and\s+{ARG}"),
                    "cross",
                ),
                r(format!(r"(?:magnitude|length)\s+of\s+{ARG}"), "norm"),
                r(
                    format!(
                        r"unit\s+vector\s+(?:of\s+|for\s+|along\s+|in\s+the\s+direction\s+of\s+)?{ARG}"
                    ),
                    "unit",
                ),
                r(
                    format!(r"orthonormal\s+basis\s+(?:of|for)\s+{ARG}"),
                    "gramschmidt",
                ),
                r(
                    format!(r"(?:gram[-\s]*schmidt|orthonormali[sz]e)\s+(?:of\s+|on\s+)?{ARG}"),
                    "gramschmidt",
                ),
                r(
                    format!(
                        r"basis\s+(?:of|for)\s+(?:the\s+)?(?:null\s*space|kernel)\s+of\s+{ARG}"
                    ),
                    "nullspace",
                ),
                r(
                    format!(r"basis\s+(?:of|for)\s+(?:the\s+)?column\s+space\s+of\s+{ARG}"),
                    "colspace",
                ),
                r(
                    format!(r"basis\s+(?:of|for)\s+(?:the\s+)?row\s+space\s+of\s+{ARG}"),
                    "rowspace",
                ),
                r(
                    format!(
                        r"(?:dim(?:ension)?\s+(?:of\s+)?(?:the\s+)?(?:null\s*space|null|kernel)|nullity)\s+(?:of\s+)?{ARG}"
                    ),
                    "nullity",
                ),
                r(
                    format!(
                        r"dim(?:ension)?\s+(?:of\s+)?(?:the\s+)?(?:column\s+space|col|row\s+space|row)\s+(?:of\s+)?{ARG}"
                    ),
                    "rank",
                ),
                r(
                    format!(
                        r"(?:least\s+squares|lsq)\s+(?:solution\s+|fit\s+)?(?:of\s+|for\s+|to\s+)?{ARG}"
                    ),
                    "lsq",
                ),
                r(
                    format!(r"(?:is|check\s+if)\s+{ARG}\s+invertible"),
                    "isinvertible",
                ),
                r(
                    format!(r"(?:is|check\s+if)\s+{ARG}\s+symmetric"),
                    "issymmetric",
                ),
                r(
                    format!(r"(?:is|check\s+if)\s+{ARG}\s+orthogonal"),
                    "isorthogonal",
                ),
                r(
                    format!(r"(?:is|check\s+if)\s+{ARG}\s+diagonali[sz]able"),
                    "isdiagonalizable",
                ),
                r(
                    format!(r"(?:is|check\s+if)\s+{ARG}\s+positive\s+definite"),
                    "isposdef",
                ),
                r(
                    format!(r"(?:are|is|check\s+if)\s+{ARG}\s+linearly\s+independent"),
                    "islinind",
                ),
            ]
        },
    );
    for (re, to) in RULES.iter() {
        let Ok(Some(c)) = re.captures(expr) else {
            continue;
        };
        let args: Vec<String> = (1..c.len())
            .filter_map(|i| c.get(i))
            .map(|m| m.as_str().trim().to_string())
            .collect();
        // `least squares A b` names two operands with a space between
        let args = match *to {
            "lsq" if args.len() == 1 => two_operands(&args[0]).unwrap_or(args),
            // `are u, v and w linearly independent`
            "islinind" => args[0]
                .split(" and ")
                .flat_map(|p| p.split(','))
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
            _ => args,
        };
        return format!("{to}({})", args.join(", "));
    }
    expr.to_string()
}

/// `[1 1; 1 2] [1; 2]` or `A, b`: two operands side by side.
fn two_operands(s: &str) -> Option<Vec<String>> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => {
                depth -= 1;
                if depth == 0 {
                    let rest = s[i + 1..].trim_start();
                    let rest = rest.strip_prefix(',').unwrap_or(rest).trim();
                    let rest = rest.strip_prefix("and ").unwrap_or(rest).trim();
                    return (!rest.is_empty())
                        .then(|| vec![s[..=i].trim().to_string(), rest.to_string()]);
                }
            }
            ',' if depth == 0 => {
                return Some(vec![
                    s[..i].trim().to_string(),
                    s[i + 1..].trim().to_string(),
                ]);
            }
            _ => {}
        }
    }
    None
}

/// `u · v` and `u × v` with one product on the line are the dot and cross products when both sides are vectors
/// (`qc_dot`, `qc_cross` decide); `u . v`, spaced, is a dot too; `|A|` is a determinant, or a vector's length.
fn products(expr: &str) -> String {
    static SPACED_DOT: Lazy = Lazy::new(|| js(r"(?<=[\])A-Za-z])\s+\.\s+(?=[\[(A-Za-z])", ""));
    static BARS: Lazy = Lazy::new(|| js(r"^\s*\|([^|]+)\|\s*$", ""));
    let s = jsre::replace_all(&SPACED_DOT, expr, |_| " · ".into()).into_owned();
    if let Ok(Some(c)) = BARS.captures(s.as_str()) {
        return format!("qc_bars({})", jsre::group(&c, 1).trim());
    }
    // the right side of an assignment
    let (head, body) = match top_level(&s, &['=']).as_slice() {
        [i] => (&s[..=*i], &s[*i + 1..]),
        _ => ("", s.as_str()),
    };
    let at = top_level(body, &['·', '×']);
    let [i] = at.as_slice() else { return s };
    let ch = body[*i..].chars().next().expect("found");
    let (l, r) = (body[..*i].trim(), body[*i + ch.len_utf8()..].trim());
    let sum = |t: &str| !top_level(t.trim_start_matches(['-', '+']), &['+', '-']).is_empty();
    if l.is_empty() || r.is_empty() || sum(l) || sum(r) {
        return s;
    }
    let name = if ch == '·' { "qc_dot" } else { "qc_cross" };
    format!("{head} {name}({l}, {r})").trim_start().to_string()
}

/// Byte offsets of `chars` outside brackets and parentheses.
fn top_level(s: &str, chars: &[char]) -> Vec<usize> {
    let mut depth = 0i32;
    let mut out = Vec::new();
    for (i, ch) in s.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            c if depth == 0 && chars.contains(&c) => out.push(i),
            _ => {}
        }
    }
    out
}

fn phrases(expr: &str) -> String {
    let expr = &sentences(expr);
    static PHRASES: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z_])(characteristic\s+(?:polynomial|equation|poly)|char\.?\s*poly(?:nomial)?|char\.?\s*eq(?:uation)?|left\s+null\s*space|null\s*space|column\s+space|row\s+space|jordan\s+(?:normal\s+)?form|pseudo-?\s*inverse|eigen\s+values|eigen\s+vectors|singular\s+value\s+decomposition|condition\s+number|least\s+squares|lu\s+decomposition|qr\s+decomposition|cholesky\s+decomposition)(?![A-Za-z_])",
            "i",
        )
    });
    jsre::replace_all(&PHRASES, expr, |c| {
        let p = jsre::group(c, 1).to_lowercase();
        let p: String = p.split_whitespace().collect::<Vec<_>>().join(" ");
        match p.as_str() {
            s if s.starts_with("char") && s.contains("eq") => "chareq",
            s if s.starts_with("char") => "charpoly",
            s if s.starts_with("left null") => "leftnull",
            s if s.starts_with("null") => "null",
            "column space" => "colspace",
            "row space" => "rowspace",
            s if s.starts_with("jordan") => "jordan",
            s if s.starts_with("pseudo") => "pinv",
            "eigen values" => "eigenvalues",
            "eigen vectors" => "eigenvectors",
            "singular value decomposition" => "svd",
            "condition number" => "cond",
            "least squares" => "lsq",
            "lu decomposition" => "lu",
            "qr decomposition" => "qr",
            _ => "cholesky",
        }
        .to_string()
    })
    .into_owned()
}

/// A line the generic evaluator takes: one of its own functions with a matrix about.
fn looks_new(expr: &str, functions: &[String]) -> bool {
    if jsre::test(&MAKER_CALL, expr) && !functions.iter().any(|f| MAKERS.split('|').any(|m| m == f))
    {
        return true;
    }
    if !expr.contains('[') {
        return false;
    }
    let expr = phrases(expr);
    let called = if functions.is_empty() {
        jsre::test(&NEW_CALL, &expr)
    } else {
        jsre::test(&new_call_re(functions), &expr)
    };
    called || jsre::test(&NEW_WORD_FORM, &expr)
}

/// Some call in the line is one only the generic evaluator knows.
fn uses_new(node: &Node, functions: &[String]) -> bool {
    if let Node::Function { callee, .. } = node
        && let Node::Symbol(name) = callee.as_ref()
    {
        let key = name.to_lowercase();
        if NEW_FNS.contains(&key.as_str()) && !functions.iter().any(|f| f.to_lowercase() == key) {
            return true;
        }
    }
    children(node).into_iter().any(|c| uses_new(c, functions))
}

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
    if called || looks_new(expr, functions) {
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
    FNS.contains(&key) || NEW_FNS.contains(&key)
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

/// `I` is the identity, as big as the line's first matrix is tall (`det(A - λI)`, `A + 2I`); `λI` is `λ*I`.
/// Native only.
fn identity_symbol(expr: &str) -> String {
    static GLUED: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_])([A-Za-zλμ])I(?![A-Za-z0-9_(])", ""));
    static LONE: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_.])I(?![A-Za-z0-9_(])", ""));
    if !expr.contains('I') || !expr.contains('[') {
        return expr.to_string();
    }
    static NUMBER: Lazy = Lazy::new(|| js(r"(\d)I(?![A-Za-z0-9_(])", ""));
    let s = jsre::replace_all(&GLUED, expr, |c| format!("{}*I", jsre::group(c, 1))).into_owned();
    let s = jsre::replace_all(&NUMBER, &s, |c| format!("{}*I", jsre::group(c, 1))).into_owned();
    if !jsre::test(&LONE, &s) {
        return s;
    }
    // the rows of the first bracketed matrix: `[[1, 2], [3, 4]]` or `[1 2; 3 4]`
    let Some(open) = s.find('[') else { return s };
    let Some(end) = operand_end(&s, open) else {
        return s;
    };
    let inner = &s[open + 1..end - 1];
    let rows = if inner.trim_start().starts_with('[') {
        let mut depth = 0;
        inner
            .chars()
            .filter(|&c| {
                match c {
                    '[' | '(' => depth += 1,
                    ']' | ')' => depth -= 1,
                    _ => {}
                }
                depth == 0 && c == ','
            })
            .count()
            + 1
    } else {
        inner.matches(';').count() + 1
    };
    jsre::replace_all(&LONE, &s, |_| format!("eye({rows})")).into_owned()
}

/// `A[2, 1]` is an entry, `A[2, :]` a row, `A[:, 1]` a column, `v[3]` one entry of a vector (native only): a
/// bracket of one or two indices right after a matrix or a call, with nothing between. A stored `A` is its literal
/// by now (`[[1, 2], [3, 4]][2, 1]`); a bracket with a `;`, another bracket or a space before it is a matrix.
fn indexing(expr: &str) -> String {
    let mut out = expr.to_string();
    for _ in 0..4 {
        let bytes = out.as_bytes();
        let mut found = None;
        for i in 1..bytes.len() {
            if bytes[i] != b'[' || !matches!(bytes[i - 1], b']' | b')') {
                continue;
            }
            let Some(end) = operand_end(&out, i) else {
                continue;
            };
            let inner = &out[i + 1..end - 1];
            if inner.contains(['[', ';', '|']) {
                continue;
            }
            let items: Vec<&str> = inner.split(',').map(str::trim).collect();
            if items.is_empty() || items.len() > 2 || items.iter().any(|t| t.is_empty()) {
                continue;
            }
            // where the indexed thing starts: its bracket, or its call's name
            let (close, open) = if bytes[i - 1] == b']' {
                (b']', b'[')
            } else {
                (b')', b'(')
            };
            let mut depth = 0;
            let mut start = None;
            for k in (0..i).rev() {
                if bytes[k] == close {
                    depth += 1;
                } else if bytes[k] == open {
                    depth -= 1;
                    if depth == 0 {
                        start = Some(k);
                        break;
                    }
                }
            }
            let Some(mut start) = start else { continue };
            if open == b'(' {
                while start > 0
                    && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
                {
                    start -= 1;
                }
            }
            found = Some((
                start,
                i,
                end,
                items.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            ));
            break;
        }
        let Some((start, i, end, items)) = found else {
            break;
        };
        let m = &out[start..i];
        let call = match items.as_slice() {
            [a] => format!("entry({m}, {a})"),
            [a, b] if a == ":" && b == ":" => m.to_string(),
            [a, b] if b == ":" => format!("row({m}, {a})"),
            [a, b] if a == ":" => format!("col({m}, {b})"),
            [a, b] => format!("entry({m}, {a}, {b})"),
            _ => break,
        };
        out = format!("{}{call}{}", &out[..start], &out[end..]);
    }
    out
}

/// `det A`, `the inverse of A`, `transpose [1 2; 3 4]` are calls; `A transpose` and `A inverse` are marks.
fn word_forms(expr: &str) -> String {
    static WORD: Lazy = Lazy::new(|| {
        js(
            &format!(
                r"(?<![A-Za-z_])(?:the\s+)?({WORDS}|{NEW_WORDS})\s+(?:of\s+)?(?=[\[(A-Za-z0-9])"
            ),
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
    // `x^T x`: a product after the transpose mark, not `T x`
    static T_THEN: Lazy = Lazy::new(|| js(r"(\^\s*T|ᵀ)\s+(?=[\[(A-Za-z0-9])", ""));
    let expr =
        jsre::replace_all(&T_THEN, expr, |c| format!("{} * ", jsre::group(c, 1))).into_owned();
    let expr = expr.as_str();
    // the input's word rule (prettyTokens, QuickInput's and the native bars') turns a typed `dot(` into `·(`: a `·`
    // with nothing to multiply before it is still the dot product's call, as `dot([1; 2; 3], [4 5 6])` is 32
    static DOT_CALL: Lazy = Lazy::new(|| js(r"(^|[^\w.)\]\s])(\s*)·(?=\s*\()", ""));
    let expr = jsre::replace_all(&DOT_CALL, expr, |c| {
        format!("{}{}dot", jsre::group(c, 1), jsre::group(c, 2))
    });
    let expr = identity_symbol(&indexing(&rotations(&augmented(&products(&phrases(
        &expr,
    ))))));
    // `3[[1, 2]]` and `k [1 2]` scale; mathjs would read a bracket after a value as an index
    let s = word_forms(&backslash(&spaced(&expr)));
    // `null` is the parser's constant, so the call goes by its long name
    static NULL_CALL: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_])null(?=\s*\()", "i"));
    let s = jsre::replace_all(&NULL_CALL, &s, |_| "nullspace".into()).into_owned();
    let s = jsre::replace_all(&SCALE, &s, |_| "*[".into()).into_owned();
    let s = jsre::replace_all(&PRIME, &s, |c| format!("{}^T", jsre::group(c, 1))).into_owned();
    s.replace('ᵀ', "^T")
        .replace("⁻¹", "^(-1)")
        .replace(['−', '–'], "-")
        .replace(['×', '·'], "*")
        .replace('÷', "/")
        .replace('π', "pi")
}

/// `[1 2 | 3; 4 5 | 6]`, an augmented matrix, is the matrix with b as its last column. A row with one bar is
/// augmented; abs bars come in pairs.
fn augmented(expr: &str) -> String {
    static BRACKET: Lazy = Lazy::new(|| js(r"\[([^\][]*)\]", ""));
    if !expr.contains('|') {
        return expr.to_string();
    }
    jsre::replace_all(&BRACKET, expr, |c| {
        let body = jsre::group(c, 1);
        if !body.contains('|') || !body.split(';').all(|row| row.matches('|').count() == 1) {
            return jsre::group(c, 0).to_string();
        }
        let sep = if body.contains(',') { "," } else { " " };
        format!("[{}]", body.replace('|', sep))
    })
    .into_owned()
}

/// `rot(θ)` is the rotation matrix, its entries read by the calculator in the angle mode.
fn rotations(expr: &str) -> String {
    static ROT: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_])rot\s*(?=\()", "i"));
    let mut out = expr.to_string();
    for _ in 0..4 {
        let Ok(Some(m)) = ROT.find(out.as_str()) else {
            break;
        };
        let open = m.end();
        let Some(end) = operand_end(&out, open) else {
            break;
        };
        let t = out[open + 1..end - 1].to_string();
        out = format!(
            "{}[[cos({t}), -sin({t})], [sin({t}), cos({t})]]{}",
            &out[..m.start()],
            &out[end..]
        );
    }
    out
}

/// `A \ b`, MATLAB's solve, is `solve(A, b)`.
fn backslash(expr: &str) -> String {
    static NAME: Lazy = Lazy::new(|| js(r"[A-Za-z0-9_.]+$", ""));
    let mut out = expr.to_string();
    for _ in 0..4 {
        let Some(i) = out.find('\\') else { break };
        let left = out[..i].trim_end();
        let start = match left.as_bytes().last() {
            Some(b')') | Some(b']') => {
                let (close, open) = if left.ends_with(')') {
                    (b')', b'(')
                } else {
                    (b']', b'[')
                };
                let mut depth = 0;
                let mut at = None;
                for (k, ch) in left.bytes().enumerate().rev() {
                    if ch == close {
                        depth += 1;
                    } else if ch == open {
                        depth -= 1;
                        if depth == 0 {
                            at = Some(k);
                            break;
                        }
                    }
                }
                match at {
                    Some(k) => k,
                    None => break,
                }
            }
            _ => match NAME.find(left).ok().flatten() {
                Some(m) => m.start(),
                None => break,
            },
        };
        let rest = &out[i + 1..];
        let skip = rest.len() - rest.trim_start().len();
        let r0 = i + 1 + skip;
        let Some(end) = operand_end(&out, r0) else {
            break;
        };
        out = format!(
            "{}solve({}, {}){}",
            &out[..start],
            &left[start..],
            &out[r0..end],
            &out[end..]
        );
    }
    out
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
    // a matrix typed on its own answers with itself; its exact form would only repeat the line
    let bare = matches!(&node, Node::Array(_));
    let mut ans = node_answer(&node, bare, opts)?;
    if !ans.message {
        if ans.parts.is_empty() && !ans.display.is_empty() {
            ans.parts = vec![show::single_part(
                &ans.display,
                ans.exact.as_deref(),
                ans.to_store().map(String::as_str),
            )];
        }
        ans.steps = exact_eval::steps(&node, opts, bar_of(expr));
    }
    Some(ans)
}

/// Where a typed `[A | b]` divides its columns: the entries before the bar in its first row.
fn bar_of(expr: &str) -> Option<usize> {
    static BARRED: Lazy = Lazy::new(|| js(r"\[([^\][]*\|[^\][]*)\]", ""));
    let m = BARRED.captures(expr).ok().flatten()?;
    let body = jsre::group(&m, 1);
    let first = body.split(';').next()?;
    let left = first.split('|').next()?;
    let n = if left.contains(',') {
        left.split(',').filter(|c| !c.trim().is_empty()).count()
    } else {
        left.split_whitespace().count()
    };
    (n > 0).then_some(n)
}

fn node_answer(node: &Node, bare: bool, opts: &MatrixOptions<'_>) -> Option<MatrixAnswer> {
    if uses_new(node, opts.functions) {
        return Some(generic_answer(
            exact_eval::exact_answer(node, opts, true),
            bare,
            opts,
        ));
    }
    let exact = exact_eval::exact_answer(node, opts, false);
    let from_exact = |exact: Option<exact_eval::Exact>| match &exact {
        Some(exact_eval::Exact::Value {
            approx: Some(approx),
            ..
        }) => Some(with_exact(
            numeric_answer(approx.clone(), opts),
            exact,
            bare,
            opts,
        )),
        _ => None,
    };
    match eval_node(node, opts) {
        Ok(value) => Some(with_exact(numeric_answer(value, opts), exact, bare, opts)),
        // the floats gave up (`nearly singular`) where exact arithmetic didn't
        Err(Fail::Matrix(msg)) => Some(from_exact(exact).unwrap_or_else(|| message(msg))),
        // a name the floats don't know is a symbol inside a matrix: `[k 2; 3 k]`
        Err(Fail::Blank) => match exact {
            Some(exact_eval::Exact::Value {
                approx: None,
                text,
                literal,
                ..
            }) => Some(MatrixAnswer {
                // blank as in the TS, but kept, so `A = [k 2; 3 k]` stores A
                display: if bare { String::new() } else { text },
                stored: literal,
                ..MatrixAnswer::default()
            }),
            Some(exact_eval::Exact::Message(msg)) => Some(message(msg)),
            other => Some(from_exact(other).unwrap_or_default()),
        },
        Err(Fail::Other) => None,
    }
}

/// A line only the generic evaluator knows: its answer alone, with no float reader to lead.
fn generic_answer(
    exact: Option<exact_eval::Exact>,
    bare: bool,
    opts: &MatrixOptions<'_>,
) -> MatrixAnswer {
    if let Some(exact_eval::Exact::Value {
        approx: Some(approx),
        ..
    }) = &exact
    {
        let approx = approx.clone();
        return with_exact(numeric_answer(approx, opts), exact, bare, opts);
    }
    match exact {
        None => MatrixAnswer::default(),
        Some(exact_eval::Exact::Message(msg)) => message(msg),
        Some(exact_eval::Exact::Worded(a)) => {
            let display = a.display();
            let exact = a.exact().filter(|e| *e != display);
            let (display, exact) = match exact {
                Some(e) if opts.fraction_mode && !e.contains("sqrt") && !e.contains('i') => {
                    (e, None)
                }
                other => (display, other),
            };
            MatrixAnswer {
                display,
                exact,
                // one matrix is kept when stored; several are stored by `L, U = lu(A)`
                stored: match a.parts.as_slice() {
                    [p] => p.literal.clone(),
                    _ => None,
                },
                // a lone number (`nullity`) is a number
                n: match a.parts.as_slice() {
                    [p] if p.label.is_none() && !p.words => {
                        p.literal.as_deref().and_then(|l| l.parse::<f64>().ok())
                    }
                    _ => None,
                },
                parts: show::public_parts(&a),
                ..MatrixAnswer::default()
            }
        }
        // a symbolic answer
        Some(exact_eval::Exact::Value { text, literal, .. }) => MatrixAnswer {
            display: text,
            stored: literal,
            ..MatrixAnswer::default()
        },
    }
}

fn message(msg: String) -> MatrixAnswer {
    MatrixAnswer {
        display: msg,
        message: true,
        ..MatrixAnswer::default()
    }
}

/// The float answer with the exact one beside it, where the exact one says more: a fraction past what the floats
/// read back (`1/1234`), or a square root (`2*sqrt(3)`).
fn with_exact(
    mut ans: MatrixAnswer,
    exact: Option<exact_eval::Exact>,
    bare: bool,
    opts: &MatrixOptions<'_>,
) -> MatrixAnswer {
    let Some(exact_eval::Exact::Value {
        text,
        literal,
        rich: true,
        ..
    }) = exact
    else {
        return ans;
    };
    if ans.message || ans.exact.is_some() || text == ans.display {
        return ans;
    }
    ans.stored = literal;
    if bare {
        return ans;
    }
    if opts.fraction_mode && !text.contains("sqrt") {
        ans.display = text;
    } else {
        ans.exact = Some(text);
    }
    ans
}

/// The float reader's answer: a number, or a grid shown to a few digits with its fraction form beside it.
fn numeric_answer(value: M, opts: &MatrixOptions<'_>) -> MatrixAnswer {
    // a row times a column is a number (`[1 2 3] * [1; 2; 3]` is 14)
    let value = match value {
        M::Grid(g) if g.len() == 1 && g[0].len() == 1 => M::Num(g[0][0]),
        v => v,
    };
    match value {
        M::Num(v) => {
            if !v.is_finite() {
                return message("undefined".into());
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
            MatrixAnswer {
                exact: exact.filter(|e| *e != display),
                display,
                n: Some(n),
                ..MatrixAnswer::default()
            }
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
            MatrixAnswer {
                exact: exact.filter(|e| *e != display),
                display,
                literal: Some(literal_of(&clean)),
                ..MatrixAnswer::default()
            }
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
