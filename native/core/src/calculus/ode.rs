//! Native only: ordinary differential equations typed on a line (the web has none, so `y' = 2x` was blank there).
//!
//! - first order, `y' = f(x)`: the integral table (`antiderivative`), `y = x^2 + C`
//! - linear with constant coefficients, first or second order: the characteristic roots, and for a forcing term
//!   made of polynomials, exponentials, sines and cosines a particular solution by undetermined coefficients
//!   (fitted numerically, then shown as fractions): `y' + 2y = e^x` is `y = e^x/3 + C e^(-2x)`
//! - first order linear with varying coefficients, `y' + p(x) y = q(x)` where q/p is a constant: `k + C e^(-∫p)`
//! - separable `y' = g(x) y^2`: `y = -1/(G + C)`
//!
//! Conditions after `;` (`y(0)=2`, `y'(0)=1`) settle the constants. Every answer, the general one with sample
//! constants and the one the conditions settle, is put back into the equation numerically before it's shown, and
//! anything else stays blank. Derivatives are in radians whatever the angle mode: calculus of trig functions is.

use crate::jsre::{self, Lazy};
use crate::scientific::ScientificContext;
use crate::scientific::functions::jsmath;
use crate::symbolic::Op;
use crate::types::AngleMode;

use super::antiderivative::Antiderivative;
use super::tree::{self, N, T, constant, op};
use super::{
    RealFn, TreeCtx, antiderivative, re, real_fn, simplify_keeping_constants, substitute, to_tree,
};

/// y, y′ and y″ in the equation's tree.
const Y: [&str; 3] = ["odey", "odeyp", "odeypp"];

/// What the line asks: `y(0)=2` is order 0 at 0, `y'(0)=1` order 1.
struct Condition {
    order: usize,
    at: f64,
    value: f64,
}

struct Ode {
    dep: String,
    var: String,
    /// typed as `y(x)` or `y'(x)`, so the answer is `y(x) = …`
    with_var: bool,
    order: usize,
    /// left side minus right side, in `var` and the `Y` placeholders
    eq: T,
    conditions: Vec<Condition>,
}

/// Native only: the answer to a differential equation line. None when the line isn't one; Some("") when it is
/// one this can't solve (it stays blank rather than reaching the other routes).
pub fn ode_answer(raw: &str, sci: &ScientificContext, sig_figs: f64) -> Option<String> {
    let mut rad = sci.clone();
    rad.angle_mode = AngleMode::Rad;
    let parsed = parse(raw, &rad)?;
    Some(
        parsed
            .and_then(|ode| solve(&ode, &rad, sig_figs))
            .unwrap_or_default(),
    )
}

// ---- reading the line

fn split_top(text: &str, sep: char) -> Vec<&str> {
    let (mut depth, mut start, mut out) = (0_i32, 0, Vec::new());
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

static LEIBNIZ: Lazy = Lazy::new(|| {
    re(
        r"(?<![A-Za-z])d\s*(\^\s*2|²)?\s*([A-Za-z])\s*/\s*d\s*([A-Za-z])\s*(\^\s*2|²)?",
        "",
    )
});
// a mark after a letter is a prime (after a digit it's feet, which this never sees)
static PRIME: Lazy = Lazy::new(|| {
    re(
        r"(?<![A-Za-z_])([A-Za-z])('{1,2})(?!')(\s*\(\s*([A-Za-z])\s*\))?",
        "",
    )
});
static WORD: Lazy = Lazy::new(|| re(r"[A-Za-z]+", ""));
static CONDITION: Lazy = Lazy::new(|| {
    re(
        r"^\s*([A-Za-z])\s*('{0,2})\s*\(\s*(.+?)\s*\)\s*=\s*(.+?)\s*$",
        "",
    )
});

/// None: not a differential equation. Some(None): one, but not one this reads.
fn parse(raw: &str, sci: &ScientificContext) -> Option<Option<Ode>> {
    let text = raw.trim().replace(['′', '’'], "'").replace('″', "''");
    let parts = split_top(&text, ';');
    let head = parts[0];
    // `dy/dx` and `d^2y/dx^2` as primes, keeping the letter they're by
    let mut var_from_leibniz: Option<String> = None;
    let mut leibniz_ok = true;
    let head = jsre::replace_all(&LEIBNIZ, head, |c| {
        let (top, dep, var, bottom) = (
            !jsre::group(c, 1).is_empty(),
            jsre::group(c, 2),
            jsre::group(c, 3),
            !jsre::group(c, 4).is_empty(),
        );
        if (bottom && !top) || dep == var {
            leibniz_ok = false;
        }
        if var_from_leibniz.as_deref().is_some_and(|v| v != var) {
            leibniz_ok = false;
        }
        var_from_leibniz = Some(var.to_string());
        format!("{dep}{}", if top { "''" } else { "'" })
    })
    .into_owned();
    let mut deps: Vec<String> = Vec::new();
    let mut order = 0;
    for c in PRIME.captures_iter(head.as_str()).flatten() {
        deps.push(jsre::group(&c, 1).to_string());
        order = order.max(jsre::group(&c, 2).len());
    }
    // `f'(x) = 2x` with f defined on the sheet is an equation in x
    if order == 0 || !head.contains('=') || deps.iter().any(|d| sci.functions.contains_key(d)) {
        return None;
    }
    // from here on it is a differential equation: anything that doesn't fit is blank
    let sides = split_top(&head, '=');
    let dep = deps[0].clone();
    if !leibniz_ok
        || deps.iter().any(|d| *d != dep)
        || sides.len() != 2
        || head.contains(['<', '>', '!'])
        || ["e", "i"].contains(&dep.as_str())
    {
        return Some(None);
    }
    let words: Vec<String> = WORD
        .find_iter(&head)
        .flatten()
        .map(|m| m.as_str().to_string())
        .collect();
    // a letter on its own, or glued to y as in `xy`
    let has = |l: &str| {
        words.iter().any(|w| {
            w == l || (w.contains(l) && w.chars().all(|c| l.contains(c) || dep.contains(c)))
        })
    };
    let var = match var_from_leibniz {
        Some(v) => v,
        None if dep != "x" && has("x") => "x".into(),
        None if dep != "t" && has("t") => "t".into(),
        None if dep == "x" => "t".into(),
        None => "x".into(),
    };
    if var == dep || ["e", "i"].contains(&var.as_str()) {
        return Some(None);
    }
    let Some(eq) = equation(sides[0], sides[1], &dep, &var, sci) else {
        return Some(None);
    };
    let mut conditions = Vec::new();
    for part in &parts[1..] {
        if part.trim().is_empty() {
            continue;
        }
        let Some(c) = CONDITION.captures(part.trim()).ok().flatten() else {
            return Some(None);
        };
        let primes = jsre::group(&c, 2).len();
        let (Some(at), Some(value)) = (
            number(jsre::group(&c, 3), sci),
            number(jsre::group(&c, 4), sci),
        ) else {
            return Some(None);
        };
        if jsre::group(&c, 1) != dep || primes >= order {
            return Some(None);
        }
        conditions.push(Condition {
            order: primes,
            at,
            value,
        });
    }
    let with_var = re(&format!(r"(?<![A-Za-z]){dep}'*\s*\(\s*{var}\s*\)"), "")
        .is_match(&head)
        .unwrap_or(false);
    Some(Some(Ode {
        dep,
        var,
        with_var,
        order,
        eq,
        conditions,
    }))
}

fn number(text: &str, sci: &ScientificContext) -> Option<f64> {
    let v = real_fn(text, sci, "odeunused")?(0.0);
    v.is_finite().then_some(v)
}

/// One side with y, y′, y″ as the placeholders: `xy'` is x·y′.
fn side(text: &str, dep: &str, var: &str) -> String {
    let split = jsre::replace_all(&WORD, text, |c| {
        let w = jsre::group(c, 0);
        if w.len() > 1
            && w.chars()
                .all(|ch| ch.to_string() == dep || ch.to_string() == var)
        {
            w.chars().map(String::from).collect::<Vec<_>>().join("*")
        } else {
            w.to_string()
        }
    });
    let primed = jsre::replace_all(&PRIME, &split, |c| {
        if jsre::group(c, 1) != dep {
            return jsre::group(c, 0).to_string();
        }
        let arg = jsre::group(c, 4);
        let rest = if arg.is_empty() || arg == var {
            String::new()
        } else {
            // `y'(2)` inside an equation isn't a function of x
            "(odebad)".into()
        };
        format!("({}){rest}", Y[jsre::group(c, 2).len()])
    });
    let bare = re(
        &format!(r"(?<![A-Za-z]){dep}(\s*\(\s*{var}\s*\))?(?![A-Za-z])"),
        "",
    );
    jsre::replace_all(&bare, &primed, |_| format!("({})", Y[0])).into_owned()
}

fn equation(lhs: &str, rhs: &str, dep: &str, var: &str, sci: &ScientificContext) -> Option<T> {
    let ctx = TreeCtx {
        sci,
        bound: Some(var.to_string()),
        depth: 0,
        keep: Y.iter().map(|s| s.to_string()).collect(),
    };
    let l = to_tree(&side(lhs, dep, var), &ctx)?;
    let r = to_tree(&side(rhs, dep, var), &ctx)?;
    Some(op(Op::Subtract, vec![l, r]))
}

// ---- numbers from the tree

fn at(eq: &T, var: &str, x: f64, y: [f64; 3]) -> Option<f64> {
    let scope = [
        (var.to_string(), x),
        (Y[0].to_string(), y[0]),
        (Y[1].to_string(), y[1]),
        (Y[2].to_string(), y[2]),
    ];
    tree::evaluate(eq, &scope)?.filter(|v| v.is_finite())
}

fn with(eq: &T, y: [f64; 3]) -> T {
    let map: Vec<(String, T)> = (0..3).map(|k| (Y[k].to_string(), constant(y[k]))).collect();
    substitute(eq, &map)
}

fn tidy(t: &T, var: &str) -> T {
    simplify_keeping_constants(t, Some(var)).unwrap_or_else(|| t.clone())
}

const XS: [f64; 6] = [0.31, 0.77, 1.29, 1.83, 2.41, 3.07];

fn close(a: f64, b: f64, scale: f64) -> bool {
    (a - b).abs() <= 1e-9 * scale.max(a.abs()).max(b.abs()).max(1.0)
}

/// The equation is linear in y, y′, y″: coefficients at each sample x, and the forcing term.
fn linear_at(ode: &Ode, x: f64) -> Option<([f64; 3], f64)> {
    let f0 = at(&ode.eq, &ode.var, x, [0.0; 3])?;
    let mut c = [0.0; 3];
    for (k, ck) in c.iter_mut().enumerate() {
        let mut e = [0.0; 3];
        e[k] = 1.0;
        *ck = at(&ode.eq, &ode.var, x, e)? - f0;
    }
    for y in [[0.7, -1.3, 2.1], [-2.6, 0.45, -0.8], [3.3, 1.9, 0.6]] {
        let want = f0 + c[0] * y[0] + c[1] * y[1] + c[2] * y[2];
        let got = at(&ode.eq, &ode.var, x, y)?;
        if !close(got, want, f0.abs() + c.iter().map(|v| v.abs()).sum::<f64>()) {
            return None;
        }
    }
    Some((c, -f0))
}

// ---- the answer's shape

#[derive(Clone, Copy, Debug)]
enum Coef {
    Num(f64),
    K(usize),
}

/// `coef · x^j · e^(a x) · cos(b x)` (or sin)
#[derive(Clone, Copy, Debug)]
struct Term {
    coef: Coef,
    j: u32,
    a: f64,
    b: f64,
    sin: bool,
}

enum Form {
    Sum(Vec<Term>),
    /// `G + C`
    Table(Antiderivative),
    /// `k + C e^(G)`
    Exp(f64, Antiderivative),
    /// `-1/(G + C)`
    Recip(Antiderivative),
}

impl Form {
    fn constants(&self) -> usize {
        match self {
            Form::Sum(terms) => terms
                .iter()
                .filter_map(|t| match t.coef {
                    Coef::K(i) => Some(i + 1),
                    Coef::Num(_) => None,
                })
                .max()
                .unwrap_or(0),
            _ => 1,
        }
    }
}

/// A plain number or fraction: (numerator text, denominator) with the sign kept apart by the caller.
/// Denominators to 100, and only within 1e-9, so an irrational root isn't passed off as a fraction.
fn fraction(v: f64) -> Option<(i64, i64)> {
    for q in 1..=100_i64 {
        let p = (v * q as f64).round();
        if (v - p / q as f64).abs() <= 1e-9 * v.abs().max(1.0) && p.abs() < 1e12 {
            return Some((p as i64, q));
        }
    }
    None
}

fn snap(v: f64) -> f64 {
    match fraction(v) {
        Some((p, q)) => p as f64 / q as f64,
        None => v,
    }
}

/// `c·var` as text: `x`, `-x`, `2x`, `x/3`, `-2x/3`, `0.618033988750x`.
fn linear(c: f64, var: &str, sig: f64) -> String {
    let sign = if c < 0.0 { "-" } else { "" };
    match fraction(c.abs()) {
        Some((1, 1)) => format!("{sign}{var}"),
        Some((p, 1)) => format!("{sign}{p}{var}"),
        Some((1, q)) => format!("{sign}{var}/{q}"),
        Some((p, q)) => format!("{sign}{p}{var}/{q}"),
        None => format!("{sign}{}{var}", crate::format::format_number(c.abs(), sig)),
    }
}

fn body(t: &Term, var: &str, sig: f64) -> String {
    let mut parts = Vec::new();
    match t.j {
        0 => {}
        1 => parts.push(var.to_string()),
        j => parts.push(format!("{var}^{j}")),
    }
    if t.a != 0.0 {
        let arg = linear(t.a, var, sig);
        parts.push(if arg == var {
            format!("e^{var}")
        } else {
            format!("e^({arg})")
        });
    }
    if t.b != 0.0 {
        let f = if t.sin { "sin" } else { "cos" };
        parts.push(format!("{f}({})", linear(t.b, var, sig)));
    }
    parts.join(" ")
}

/// `v·body` with v ≥ 0: `2e^x`, `e^x/3`, `2 cos(x)`, `0.7 e^x`.
fn scaled(v: f64, body: &str, sig: f64) -> String {
    let glue = |n: &str| {
        if body.is_empty() {
            n.to_string()
        } else if body.starts_with(['e', 'x', 't']) || body.len() == 1 {
            format!("{n}{body}")
        } else {
            format!("{n} {body}")
        }
    };
    match fraction(v) {
        Some((1, 1)) if !body.is_empty() => body.to_string(),
        Some((p, 1)) => glue(&p.to_string()),
        Some((1, q)) if !body.is_empty() => format!("{body}/{q}"),
        Some((p, q)) if !body.is_empty() => format!("{}/{q}", glue(&p.to_string())),
        Some((p, q)) => format!("{p}/{q}"),
        None => {
            let n = crate::format::format_number(v, sig);
            if body.is_empty() {
                n
            } else {
                format!("{n} {body}")
            }
        }
    }
}

fn names(n: usize) -> Vec<String> {
    if n == 1 {
        vec!["C".into()]
    } else {
        (1..=n).map(|i| format!("C{i}")).collect()
    }
}

/// Signed pieces joined: `a + b - c`; an empty sum is 0.
fn join(pieces: Vec<(bool, String)>) -> String {
    let mut out = String::new();
    for (negative, text) in pieces {
        if out.is_empty() {
            out = if negative { format!("-{text}") } else { text };
        } else {
            out.push_str(if negative { " - " } else { " + " });
            out.push_str(&text);
        }
    }
    if out.is_empty() { "0".into() } else { out }
}

/// `G + C` with the constant named (None) or a number.
fn plus_constant(g: &str, k: Option<f64>, sig: f64) -> String {
    match k {
        None => format!("{g} + C"),
        Some(0.0) => g.to_string(),
        Some(v) => join(vec![
            (false, g.to_string()),
            (v < 0.0, scaled(v.abs(), "", sig)),
        ]),
    }
}

/// The right side, with the constants named (None) or given values; `eval` for the text the engine evaluates
/// (`ln|x|` shows as that but evaluates as `log(abs(x))`).
fn render(form: &Form, values: Option<&[f64]>, var: &str, sig: f64, eval: bool) -> String {
    let pick = |a: &Antiderivative| if eval { a.eval.clone() } else { a.show.clone() };
    match form {
        Form::Sum(terms) => {
            let names = names(form.constants());
            let mut pieces = Vec::new();
            for t in terms {
                let b = body(t, var, sig);
                let v = match (t.coef, values) {
                    (Coef::Num(v), _) => v,
                    (Coef::K(i), Some(vs)) => vs[i],
                    (Coef::K(i), None) => {
                        let n = &names[i];
                        pieces.push((
                            false,
                            if b.is_empty() {
                                n.clone()
                            } else {
                                format!("{n} {b}")
                            },
                        ));
                        continue;
                    }
                };
                if v != 0.0 {
                    pieces.push((v < 0.0, scaled(v.abs(), &b, sig)));
                }
            }
            join(pieces)
        }
        Form::Table(g) => plus_constant(&pick(g), values.map(|v| v[0]), sig),
        Form::Exp(k, g) => {
            let g = &pick(g);
            let e = if g == var {
                format!("e^{var}")
            } else if g == &format!("ln|{var}|") {
                format!("|{var}|")
            } else {
                format!("e^({g})")
            };
            let over = !eval && g == &format!("-ln|{var}|");
            let exp = match values.map(|v| v[0]) {
                None if over => (false, format!("C/|{var}|")),
                None => (false, format!("C {e}")),
                Some(v) if over => (v < 0.0, format!("{}/|{var}|", scaled(v.abs(), "", sig))),
                Some(v) => (v < 0.0, scaled(v.abs(), &e, sig)),
            };
            let mut pieces = Vec::new();
            if *k != 0.0 {
                pieces.push((*k < 0.0, scaled(k.abs(), "", sig)));
            }
            if values.is_none_or(|v| v[0] != 0.0) {
                pieces.push(exp);
            }
            join(pieces)
        }
        Form::Recip(g) => format!(
            "-1/({})",
            plus_constant(&pick(g), values.map(|v| v[0]), sig)
        ),
    }
}

// ---- solving

fn solve(ode: &Ode, sci: &ScientificContext, sig: f64) -> Option<String> {
    solved(ode, sci, sig).map(|s| s.answer)
}

/// How the general solution was found, for the working.
enum How {
    /// `y' = f(x)`
    Integrate(T),
    /// constant coefficients `c2 y'' + c1 y' + c0 y = g`
    Characteristic([f64; 3]),
    /// `y' + p y = q`, with p
    Factor(T),
    /// `y' = g(x) y^2`, with g
    Separate(T),
}

/// What `solve` found, with what the working shows.
struct Solved {
    form: Form,
    how: How,
    values: Option<Vec<f64>>,
    lhs: String,
    answer: String,
}

fn solved(ode: &Ode, sci: &ScientificContext, sig: f64) -> Option<Solved> {
    let (form, how) = general(ode)?;
    let n = form.constants();
    // the general answer, with two sets of sample constants, has to satisfy the equation
    for sample in [[1.37, -0.61], [-0.83, 2.9]] {
        if !satisfies(
            ode,
            &render(&form, Some(&sample[..n.max(1)]), &ode.var, sig, true),
            sci,
            false,
        ) {
            return None;
        }
    }
    let values = if ode.conditions.is_empty() {
        None
    } else {
        if ode.conditions.len() != n {
            return None;
        }
        Some(settle(ode, &form, n, sci)?)
    };
    let shown = render(&form, values.as_deref(), &ode.var, sig, false);
    if values.is_some()
        && !satisfies(
            ode,
            &render(&form, values.as_deref(), &ode.var, sig, true),
            sci,
            true,
        )
    {
        return None;
    }
    let lhs = if ode.with_var {
        format!("{}({})", ode.dep, ode.var)
    } else {
        ode.dep.clone()
    };
    Some(Solved {
        answer: format!("{lhs} = {shown}"),
        form,
        how,
        values,
        lhs,
    })
}

fn general(ode: &Ode) -> Option<(Form, How)> {
    let samples: Vec<(f64, [f64; 3], f64)> = XS
        .iter()
        .filter_map(|&x| linear_at(ode, x).map(|(c, g)| (x, c, g)))
        .collect();
    let linear = samples.len() >= 4 && samples.len() + 1 >= XS.len();
    if linear {
        let (_, c, _) = samples[0];
        let constant_coefs = samples
            .iter()
            .all(|(_, ck, _)| (0..3).all(|k| close(ck[k], c[k], c.iter().map(|v| v.abs()).sum())));
        if samples.iter().any(|(_, ck, _)| ck[ode.order] == 0.0) {
            return None;
        }
        if constant_coefs && let Some(form) = constant_coefficients(ode, c) {
            return Some((form, How::Characteristic(c)));
        }
        if ode.order == 1 {
            return first_order_linear(ode, &samples);
        }
        return None;
    }
    if ode.order == 1 {
        return separable(ode);
    }
    None
}

// ---- native only: the working (`crate::steps`)

/// The working for a differential equation line, as plain math and why, the last step the answer the line shows.
/// None when the line isn't one this solves.
pub fn ode_steps(raw: &str, sci: &ScientificContext, sig: f64) -> Option<Vec<(String, String)>> {
    let mut rad = sci.clone();
    rad.angle_mode = AngleMode::Rad;
    let ode = parse(raw, &rad)??;
    let s = solved(&ode, &rad, sig)?;
    let (dep, var) = (&ode.dep, &ode.var);
    let lhs = &s.lhs;
    let mut out: Vec<(String, String)> = Vec::new();
    let pretty = |t: &T| super::pretty::pretty(t).s;
    let general = render(&s.form, None, var, sig, false);
    match &s.how {
        How::Integrate(f) => {
            out.push((
                format!("{lhs} = ∫ ({}) d{var}", pretty(f)),
                "integrate both sides".into(),
            ));
            let why = antiderivative::antiderivative_noted(f, var, false)
                .and_then(|(_, notes)| headline(&notes))
                .unwrap_or_else(|| "integrate".into());
            out.push((format!("{lhs} = {general}"), why));
        }
        // `y' = f(x)` with constant coefficients: just an integral
        How::Characteristic(c) if ode.order == 1 && c[0] == 0.0 => {
            let g = op(Op::UnaryMinus, vec![with(&ode.eq, [0.0; 3])]);
            let f = tidy(&op(Op::Divide, vec![g, constant(c[1])]), var);
            out.push((
                format!("{lhs} = ∫ ({}) d{var}", pretty(&f)),
                "integrate both sides".into(),
            ));
            let why = antiderivative::antiderivative_noted(&f, var, false)
                .and_then(|(_, notes)| headline(&notes))
                .unwrap_or_else(|| "integrate".into());
            out.push((format!("{lhs} = {general}"), why));
        }
        How::Characteristic(c) => {
            let poly = characteristic(c, ode.order, sig);
            out.push((
                format!("{poly} = 0"),
                format!("characteristic equation: try {dep} = e^(r{var})"),
            ));
            let Form::Sum(terms) = &s.form else {
                return None;
            };
            let (roots, kind) = root_text(c, ode.order, sig);
            out.push((roots, kind.0.into()));
            let homogeneous: Vec<Term> = terms
                .iter()
                .copied()
                .filter(|t| matches!(t.coef, Coef::K(_)))
                .collect();
            let particular: Vec<Term> = terms
                .iter()
                .copied()
                .filter(|t| matches!(t.coef, Coef::Num(_)))
                .collect();
            let h = render(&Form::Sum(homogeneous), None, var, sig, false);
            if particular.is_empty() {
                out.push((format!("{lhs} = {general}"), kind.1.into()));
            } else {
                out.push((format!("{dep}_h = {h}"), kind.1.into()));
                let p = render(&Form::Sum(particular), None, var, sig, false);
                out.push((
                    format!("{dep}_p = {p}"),
                    "particular solution by undetermined coefficients".into(),
                ));
                out.push((
                    format!("{lhs} = {general}"),
                    format!("{dep} = {dep}_p + {dep}_h"),
                ));
            }
        }
        How::Factor(p) => {
            let p_text = pretty(p);
            out.push((
                format!("μ = e^(∫ ({p_text}) d{var})"),
                format!("integrating factor for {dep}' + p {dep} = q, p = {p_text}"),
            ));
            out.push((
                format!("{lhs} = {general}"),
                format!("(μ {dep})' = μ q, integrate and divide by μ"),
            ));
        }
        How::Separate(g) => {
            let Form::Recip(found) = &s.form else {
                return None;
            };
            out.push((
                format!("d{dep}/{dep}^2 = ({}) d{var}", pretty(g)),
                "separate the variables".into(),
            ));
            out.push((
                format!("-1/{dep} = {} + C", found.show),
                "integrate both sides".into(),
            ));
            out.push((format!("{lhs} = {general}"), format!("solve for {dep}")));
        }
    }
    if let Some(values) = &s.values {
        let names = names(values.len());
        let conditions: Vec<String> = ode
            .conditions
            .iter()
            .map(|c| {
                format!(
                    "{dep}{}({}) = {}",
                    "'".repeat(c.order),
                    number_text(c.at, sig),
                    number_text(c.value, sig)
                )
            })
            .collect();
        let settled: Vec<String> = names
            .iter()
            .zip(values)
            .map(|(n, v)| format!("{n} = {}", number_text(*v, sig)))
            .collect();
        out.push((
            format!("{} ⇒ {}", conditions.join(", "), settled.join(", ")),
            format!("put in the conditions, solve for {}", names.join(", ")),
        ));
        out.push((s.answer.clone(), "the solution".into()));
    }
    // the last step is the answer the line shows
    if out.last().map(|l| &l.0) != Some(&s.answer) {
        return None;
    }
    Some(out)
}

/// The note that says most about how a term was integrated, with the table form under a substitution.
pub(crate) fn headline(notes: &[antiderivative::Note]) -> Option<String> {
    let top = notes.iter().max_by_key(|n| n.rank)?;
    if top.rank >= 3 && top.text.starts_with("substitute") {
        let next = notes.iter().filter(|n| n.rank < 3).max_by_key(|n| n.rank);
        if let Some(next) = next {
            return Some(format!("{}, then {}", top.text, next.text));
        }
    }
    Some(top.text.clone())
}

fn number_text(v: f64, sig: f64) -> String {
    let sign = if v < 0.0 { "-" } else { "" };
    format!("{sign}{}", scaled(v.abs(), "", sig))
}

/// `c2 r^2 + c1 r + c0`, the leading coefficient made 1 when it divides out as fractions.
fn characteristic(c: &[f64; 3], order: usize, sig: f64) -> String {
    let lead = c[order];
    let mut pieces: Vec<(bool, String)> = Vec::new();
    for k in (0..=order).rev() {
        let v = snap(c[k] / lead);
        if v == 0.0 {
            continue;
        }
        let body = match k {
            0 => String::new(),
            1 => "r".to_string(),
            _ => format!("r^{k}"),
        };
        pieces.push((v < 0.0, scaled(v.abs(), &body, sig)));
    }
    join(pieces)
}

/// The characteristic roots as the working writes them, and why for them and for the homogeneous solution.
fn root_text(c: &[f64; 3], order: usize, sig: f64) -> (String, (&'static str, &'static str)) {
    let n = |v: f64| number_text(snap(v), sig);
    if order == 1 {
        return (
            format!("r = {}", n(-c[0] / c[1])),
            ("solve for r", "one root r: C e^(r x)"),
        );
    }
    let disc = c[1] * c[1] - 4.0 * c[2] * c[0];
    let scale = c[1] * c[1] + (4.0 * c[2] * c[0]).abs();
    if disc.abs() <= 1e-12 * scale.max(1e-300) {
        let r = -c[1] / (2.0 * c[2]);
        return (
            format!("r = {} (twice)", n(r)),
            (
                "quadratic formula: one repeated root",
                "repeated root: (C1 + C2 x) e^(r x)",
            ),
        );
    }
    if disc > 0.0 {
        let (r1, r2) = (
            (-c[1] + disc.sqrt()) / (2.0 * c[2]),
            (-c[1] - disc.sqrt()) / (2.0 * c[2]),
        );
        let (hi, lo) = if r1 >= r2 { (r1, r2) } else { (r2, r1) };
        return (
            format!("r = {}, {}", n(hi), n(lo)),
            (
                "quadratic formula: two real roots",
                "real roots: C1 e^(r1 x) + C2 e^(r2 x)",
            ),
        );
    }
    let (a, b) = (-c[1] / (2.0 * c[2]), ((-disc).sqrt() / (2.0 * c[2])).abs());
    let im = scaled(snap(b), "i", sig);
    let shown = if snap(a) == 0.0 {
        format!("r = ±{im}")
    } else {
        format!("r = {} ± {im}", n(a))
    };
    (
        shown,
        (
            "quadratic formula: complex roots a ± bi",
            "complex roots: e^(a x)(C1 cos(b x) + C2 sin(b x))",
        ),
    )
}

/// `c2 y'' + c1 y' + c0 y = g(x)`
fn constant_coefficients(ode: &Ode, c: [f64; 3]) -> Option<Form> {
    let order = ode.order;
    // the characteristic roots, (re, im) with im ≥ 0 standing for the pair
    let mut roots: Vec<(f64, f64)> = Vec::new();
    let mut homogeneous: Vec<Term> = Vec::new();
    let term = |k: usize, j: u32, a: f64, b: f64, sin: bool| Term {
        coef: Coef::K(k),
        j,
        a: snap(a),
        b: snap(b),
        sin,
    };
    if order == 1 {
        let r = -c[0] / c[1];
        roots.push((snap(r), 0.0));
        homogeneous.push(term(0, 0, r, 0.0, false));
    } else {
        let disc = c[1] * c[1] - 4.0 * c[2] * c[0];
        let scale = c[1] * c[1] + (4.0 * c[2] * c[0]).abs();
        if disc.abs() <= 1e-12 * scale.max(1e-300) {
            let r = -c[1] / (2.0 * c[2]);
            roots.extend([(snap(r), 0.0), (snap(r), 0.0)]);
            homogeneous.push(term(0, 0, r, 0.0, false));
            homogeneous.push(term(1, 1, r, 0.0, false));
        } else if disc > 0.0 {
            let (r1, r2) = (
                (-c[1] + disc.sqrt()) / (2.0 * c[2]),
                (-c[1] - disc.sqrt()) / (2.0 * c[2]),
            );
            let (hi, lo) = if r1 >= r2 { (r1, r2) } else { (r2, r1) };
            roots.extend([(snap(hi), 0.0), (snap(lo), 0.0)]);
            homogeneous.push(term(0, 0, hi, 0.0, false));
            homogeneous.push(term(1, 0, lo, 0.0, false));
        } else {
            let (a, b) = (-c[1] / (2.0 * c[2]), ((-disc).sqrt() / (2.0 * c[2])).abs());
            roots.push((snap(a), snap(b)));
            homogeneous.push(term(0, 0, a, b, false));
            homogeneous.push(term(1, 0, a, b, true));
        }
    }
    let g = tidy(&op(Op::UnaryMinus, vec![with(&ode.eq, [0.0; 3])]), &ode.var);
    let forcing_zero = XS
        .iter()
        .all(|&x| tree::evaluate(&g, &[(ode.var.clone(), x)]).flatten() == Some(0.0));
    let mut terms = if forcing_zero {
        Vec::new()
    } else {
        particular(&g, &ode.var, c, &roots)?
    };
    terms.extend(homogeneous);
    Some(Form::Sum(terms))
}

/// The forcing term's exponential rates, frequencies and polynomial degree, or None when it has anything else.
struct Forcing {
    rates: Vec<f64>,
    freqs: Vec<f64>,
}

fn free_of(t: &T, var: &str) -> bool {
    match &**t {
        N::Sym(s) => s != var,
        N::Const(_) => true,
        N::Op(_, a) | N::Call(_, a) => a.iter().all(|x| free_of(x, var)),
    }
}

/// The slope of an argument that's linear in var.
fn slope(t: &T, var: &str) -> Option<f64> {
    let v = |x: f64| tree::evaluate(t, &[(var.to_string(), x)]).flatten();
    let (a, b, c, d) = (v(0.0)?, v(1.0)?, v(2.0)?, v(-1.7)?);
    let s = b - a;
    (close(c - b, s, a.abs() + b.abs()) && close(a - d, 1.7 * s, a.abs() + d.abs())).then_some(s)
}

fn scan(t: &T, var: &str, f: &mut Forcing) -> Option<u32> {
    if free_of(t, var) {
        return Some(0);
    }
    match &**t {
        N::Sym(_) => Some(1),
        N::Const(_) => Some(0),
        N::Op(Op::Add | Op::Subtract, a) => {
            let mut d = 0;
            for x in a {
                d = d.max(scan(x, var, f)?);
            }
            Some(d)
        }
        N::Op(Op::UnaryMinus | Op::UnaryPlus, a) => scan(&a[0], var, f),
        N::Op(Op::Multiply, a) => {
            let mut d = 0;
            for x in a {
                d += scan(x, var, f)?;
            }
            Some(d)
        }
        N::Op(Op::Divide, a) if free_of(&a[1], var) => scan(&a[0], var, f),
        N::Op(Op::Pow, a) if free_of(&a[1], var) => {
            let k = tree::evaluate(&a[1], &[]).flatten()?;
            let d = scan(&a[0], var, f)?;
            (k >= 0.0 && k.fract() == 0.0 && k <= 6.0).then_some(d * k as u32)
        }
        N::Op(Op::Pow, a) => {
            let base = match &*a[0] {
                N::Sym(s) if s == "e" => 1.0,
                _ if free_of(&a[0], var) => jsmath::ln(tree::evaluate(&a[0], &[]).flatten()?),
                _ => return None,
            };
            if !base.is_finite() {
                return None;
            }
            f.rates.push(base * slope(&a[1], var)?);
            Some(0)
        }
        N::Call(name, a) if a.len() == 1 => {
            let s = slope(&a[0], var)?;
            match name.as_str() {
                "exp" => f.rates.push(s),
                "sin" | "cos" => f.freqs.push(s.abs()),
                "sinh" | "cosh" => f.rates.extend([s, -s]),
                _ => return None,
            }
            Some(0)
        }
        _ => None,
    }
}

fn add_sums(v: &mut Vec<f64>, differences: bool) {
    let base = v.clone();
    for (i, a) in base.iter().enumerate() {
        for b in &base[i..] {
            v.push(a + b);
            if differences {
                v.push((a - b).abs());
            }
        }
    }
    v.push(0.0);
    let mut out: Vec<f64> = Vec::new();
    for x in v.iter().map(|x| snap(*x)) {
        if !out.iter().any(|y| close(*y, x, 1.0)) {
            out.push(x);
        }
    }
    *v = out;
}

/// L[x^j e^((a+ib)x)] at x, real or imaginary part.
fn apply(c: [f64; 3], t: &Term, x: f64) -> f64 {
    let (a, b, j) = (t.a, t.b, t.j as f64);
    let p = |k: f64| if k < 0.0 { 0.0 } else { jsmath::pow(x, k) };
    // e^(λx) = e^(ax)(cos bx + i sin bx); λ² = a² - b² + 2abi
    let grow = jsmath::exp(a * x);
    let (er, ei) = (grow * jsmath::cos(b * x), grow * jsmath::sin(b * x));
    let mul = |(ar, ai): (f64, f64), (br, bi): (f64, f64)| (ar * br - ai * bi, ar * bi + ai * br);
    let f0 = (p(j), 0.0);
    let f1 = (j * p(j - 1.0) + a * p(j), b * p(j));
    let f2 = (
        j * (j - 1.0) * p(j - 2.0) + 2.0 * a * j * p(j - 1.0) + (a * a - b * b) * p(j),
        2.0 * b * j * p(j - 1.0) + 2.0 * a * b * p(j),
    );
    let sum = (
        c[0] * f0.0 + c[1] * f1.0 + c[2] * f2.0,
        c[0] * f0.1 + c[1] * f1.1 + c[2] * f2.1,
    );
    let (r, i) = mul(sum, (er, ei));
    if t.sin { i } else { r }
}

fn particular(g: &T, var: &str, c: [f64; 3], roots: &[(f64, f64)]) -> Option<Vec<Term>> {
    let mut f = Forcing {
        rates: Vec::new(),
        freqs: Vec::new(),
    };
    let degree = scan(g, var, &mut f)?;
    add_sums(&mut f.rates, false);
    add_sums(&mut f.freqs, true);
    let mut basis = Vec::new();
    for &a in &f.rates {
        for &b in &f.freqs {
            let s = roots
                .iter()
                .filter(|(ra, rb)| close(*ra, a, 1.0) && close(*rb, b, 1.0))
                .count() as u32;
            // a double root at 0 counts twice in `roots`
            for j in (s..=degree + s).rev() {
                for sin in if b == 0.0 {
                    vec![false]
                } else {
                    vec![false, true]
                } {
                    basis.push(Term {
                        coef: Coef::Num(0.0),
                        j,
                        a,
                        b,
                        sin,
                    });
                }
            }
        }
    }
    let n = basis.len();
    if n == 0 || n > 40 {
        return None;
    }
    let xs: Vec<f64> = (0..(2 * n).max(24))
        .map(|i| -1.5 + 3.0 * i as f64 / (2 * n).max(24) as f64 + 0.013)
        .collect();
    let mut rows = Vec::new();
    let mut rhs = Vec::new();
    for &x in &xs {
        rows.push(basis.iter().map(|t| apply(c, t, x)).collect::<Vec<_>>());
        rhs.push(
            tree::evaluate(g, &[(var.to_string(), x)])
                .flatten()
                .filter(|v| v.is_finite())?,
        );
    }
    let k = least_squares(&rows, &rhs)?;
    let biggest = rhs.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
    for (row, want) in rows.iter().zip(&rhs) {
        let got: f64 = row.iter().zip(&k).map(|(a, b)| a * b).sum();
        if (got - want).abs() > 1e-8 * biggest {
            return None;
        }
    }
    let top = k.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    Some(
        basis
            .into_iter()
            .zip(k)
            .filter(|(_, v)| v.abs() > 1e-10 * top.max(1e-300))
            .map(|(t, v)| Term {
                coef: Coef::Num(snap(v)),
                ..t
            })
            .collect(),
    )
}

fn least_squares(rows: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let n = rows[0].len();
    let mut m = vec![vec![0.0; n + 1]; n];
    for (row, b) in rows.iter().zip(rhs) {
        for i in 0..n {
            for j in 0..n {
                m[i][j] += row[i] * row[j];
            }
            m[i][n] += row[i] * b;
        }
    }
    gauss(m)
}

/// Solves an augmented n×(n+1) system; None when it's singular.
fn gauss(mut m: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let n = m.len();
    let scale = m.iter().flatten().fold(0.0_f64, |a, v| a.max(v.abs()));
    for col in 0..n {
        let pivot = (col..n).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() <= 1e-13 * scale {
            return None;
        }
        m.swap(col, pivot);
        let lead = m[col].clone();
        for (row, r) in m.iter_mut().enumerate() {
            if row != col {
                let f = r[col] / lead[col];
                for (v, l) in r[col..].iter_mut().zip(&lead[col..]) {
                    *v -= f * l;
                }
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

/// `y' = f(x)`, or `y' + p(x) y = q(x)` with q/p a constant.
fn first_order_linear(ode: &Ode, samples: &[(f64, [f64; 3], f64)]) -> Option<(Form, How)> {
    let var = &ode.var;
    let zero = with(&ode.eq, [0.0; 3]);
    let c1 = op(
        Op::Subtract,
        vec![with(&ode.eq, [0.0, 1.0, 0.0]), zero.clone()],
    );
    let g = op(Op::UnaryMinus, vec![zero.clone()]);
    if samples.iter().all(|(_, c, _)| c[0] == 0.0) {
        // y' = g/c1
        let f = tidy(&op(Op::Divide, vec![g, c1]), var);
        let found = antiderivative::antiderivative(&f, var, false)?;
        return Some((Form::Table(found), How::Integrate(f)));
    }
    let c0 = op(Op::Subtract, vec![with(&ode.eq, [1.0, 0.0, 0.0]), zero]);
    let k = samples[0].2 / samples[0].1[0];
    if !samples.iter().all(|(_, c, g)| close(g / c[0], k, 1.0)) {
        return None;
    }
    // y = k + C e^(-∫ c0/c1)
    let p = op(Op::Divide, vec![c0, c1]);
    let minus_p = tidy(&op(Op::UnaryMinus, vec![p.clone()]), var);
    let found = antiderivative::antiderivative(&minus_p, var, false)?;
    Some((Form::Exp(snap(k), found), How::Factor(tidy(&p, var))))
}

/// `y' = g(x) y^2`: `-1/y = G + C`.
fn separable(ode: &Ode) -> Option<(Form, How)> {
    let var = &ode.var;
    let f_at = |x: f64, y: f64| -> Option<f64> {
        let f0 = at(&ode.eq, var, x, [y, 0.0, 0.0])?;
        let f1 = at(&ode.eq, var, x, [y, 1.0, 0.0])?;
        let f2 = at(&ode.eq, var, x, [y, 2.5, 0.0])?;
        // linear in y'
        if !close(f2 - f0, 2.5 * (f1 - f0), f0.abs() + f1.abs()) || f1 == f0 {
            return None;
        }
        Some(-f0 / (f1 - f0))
    };
    let ys = [0.6, 1.0, 1.7, 2.3];
    if f_at(XS[0], 1.0)? == 0.0 {
        return None;
    }
    let mut ok = 0;
    for &x in &XS {
        let Some(row) = ys.iter().map(|&y| f_at(x, y)).collect::<Option<Vec<_>>>() else {
            continue;
        };
        // f(x, y) = f(x, 1) y^2
        for (y, v) in ys.iter().zip(&row) {
            if !close(*v, row[1] * y * y, v.abs()) {
                return None;
            }
        }
        ok += 1;
    }
    if ok < 4 {
        return None;
    }
    // g(x) = f(x, 1) = -F(x, 1, 0) / (F(x, 1, 1) - F(x, 1, 0))
    let f0 = with(&ode.eq, [1.0, 0.0, 0.0]);
    let f1 = with(&ode.eq, [1.0, 1.0, 0.0]);
    let g = tidy(
        &op(
            Op::Divide,
            vec![
                op(Op::UnaryMinus, vec![f0.clone()]),
                op(Op::Subtract, vec![f1, f0]),
            ],
        ),
        var,
    );
    let found = antiderivative::antiderivative(&g, var, false)?;
    Some((Form::Recip(found), How::Separate(g)))
}

// ---- checking

/// f, f′, f″ at x by central differences with Richardson's correction.
fn derivatives(f: &RealFn, x: f64) -> Option<[f64; 3]> {
    derivatives_by(f, x, 1e-3)
}

fn derivatives_by(f: &RealFn, x: f64, step: f64) -> Option<[f64; 3]> {
    let y = f(x);
    let d = |h: f64| {
        let (hi, lo) = (f(x + h), f(x - h));
        ((hi - lo) / (2.0 * h), (hi - 2.0 * y + lo) / (h * h))
    };
    let (a1, a2) = d(2.0 * step);
    let (b1, b2) = d(step);
    let out = [y, (4.0 * b1 - a1) / 3.0, (4.0 * b2 - a2) / 3.0];
    out.iter().all(|v| v.is_finite()).then_some(out)
}

fn satisfies(ode: &Ode, answer: &str, sci: &ScientificContext, conditions: bool) -> bool {
    let Some(f) = real_fn(answer, sci, &ode.var) else {
        return false;
    };
    let mut checked = 0;
    for x in [
        -2.3, -1.7, -1.2, -0.9, -0.35, 0.3, 0.65, 1.1, 1.38, 1.55, 2.2, 2.75,
    ] {
        let (Some(y), Some(fine)) = (derivatives(&f, x), derivatives_by(&f, x, 1e-4)) else {
            continue;
        };
        // close to a pole (tan(x) by π/2) the differences don't settle, and the point says nothing
        if (0..=ode.order).any(|k| (y[k] - fine[k]).abs() > 1e-5 * y[k].abs().max(1.0)) {
            continue;
        }
        let Some(r) = at(&ode.eq, &ode.var, x, y) else {
            continue;
        };
        let Some(f0) = at(&ode.eq, &ode.var, x, [0.0; 3]) else {
            continue;
        };
        // the size of each part of the equation, so the check is relative
        let mut size = f0.abs();
        for k in 0..=ode.order {
            let mut only = [0.0; 3];
            only[k] = y[k];
            size += at(&ode.eq, &ode.var, x, only).map_or(0.0, |v| (v - f0).abs());
        }
        if r.abs() > 1e-6 * size.max(1e-6) {
            return false;
        }
        checked += 1;
    }
    if checked < 6 {
        return false;
    }
    !conditions
        || ode.conditions.iter().all(|c| {
            derivatives(&f, c.at)
                .is_some_and(|d| (d[c.order] - c.value).abs() <= 1e-6 * (1.0 + c.value.abs()))
        })
}

/// The constants the conditions settle, by Newton's method on the rendered answer (one step when it's linear in
/// them, as all but `-1/(G + C)` are).
fn settle(ode: &Ode, form: &Form, n: usize, sci: &ScientificContext) -> Option<Vec<f64>> {
    let residual = |k: &[f64]| -> Option<Vec<f64>> {
        let f = real_fn(&render(form, Some(k), &ode.var, 17.0, true), sci, &ode.var)?;
        ode.conditions
            .iter()
            .map(|c| derivatives(&f, c.at).map(|d| d[c.order] - c.value))
            .collect()
    };
    for start in [0.5, 1.3, -0.7, 2.0] {
        let mut k = vec![start; n];
        for _ in 0..30 {
            let r = residual(&k)?;
            if r.iter().all(|v| v.abs() < 1e-12) {
                break;
            }
            let mut m = vec![vec![0.0; n + 1]; n];
            for j in 0..n {
                let h = 1e-6 * k[j].abs().max(1.0);
                let mut kk = k.clone();
                kk[j] += h;
                let rj = residual(&kk)?;
                for i in 0..n {
                    m[i][j] = (rj[i] - r[i]) / h;
                }
            }
            for i in 0..n {
                m[i][n] = -r[i];
            }
            let Some(step) = gauss(m) else { break };
            for j in 0..n {
                k[j] += step[j];
            }
            if !k.iter().all(|v| v.is_finite()) {
                break;
            }
        }
        if let Some(r) = residual(&k)
            && r.iter().all(|v| v.abs() < 1e-7)
        {
            let snapped: Vec<f64> = k
                .iter()
                .map(|v| if v.abs() < 1e-11 { 0.0 } else { snap(*v) })
                .collect();
            return Some(snapped);
        }
    }
    None
}
