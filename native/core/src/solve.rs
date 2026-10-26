//! WP-SOLVE: `src/engine/solve.ts`, one equation in one unknown, solved exactly where a polynomial allows and
//! numerically otherwise, plus `formatSolve`.
//!
//! The TS throws a budget symbol out of any evaluation once it is spent; here every budget shares a `failed`
//! flag, evaluations answer nothing once it is set, and the solve returns None at the end. Every evaluation is
//! made in the TS order (both sides, short-circuits and all), so `evals` matches.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value as Json, json};

use crate::eval::{self, Scope};
use crate::format::{format_as_fraction, format_number, int_gcd};
use crate::jsnum;
use crate::jsre::{self, Lazy, js};
use crate::plain_math::normalize_math_text;
use crate::scientific::functions::jsmath;
use crate::scientific::{self, NAMES, ScientificContext, names_pattern, preprocess_ascii};
use crate::simplify::{ExactFormOptions, exact_form, split_squares};
use crate::syntax::Node;
use crate::types::{AngleMode, SolveInfo, SolveOutcome, UserFunction, ValueKind};
use crate::work::WorkBudget;

pub const MAX_SHOWN_ROOTS: usize = 4;
pub const MAX_EVALS: u32 = 4000;
const MAX_WORK: u64 = 600_000;
const MAX_REFINED: usize = 12;
const MAX_POLE_CELLS: usize = 6;
const NOISE: f64 = 8.0 * f64::EPSILON;
const MIN_VALUE: f64 = 5e-324;

fn re_unknown(s: &str) -> bool {
    s == "θ" || (s.len() == 1 && s.as_bytes()[0].is_ascii_alphabetic())
}

fn names() -> Vec<String> {
    NAMES.split('|').map(str::to_string).collect()
}

fn known_name(n: &str) -> bool {
    let l = n.to_lowercase();
    l == "e" || l == "i" || l == "nan" || NAMES.split('|').any(|k| k.to_lowercase() == l)
}

fn callable() -> Vec<String> {
    const SKIP: [&str; 6] = ["pi", "tau", "inf", "infinity", "ans", "mod"];
    names()
        .into_iter()
        .filter(|n| !SKIP.contains(&n.to_lowercase().as_str()))
        .collect()
}

/// What a solve sees besides the line.
#[derive(Clone, Debug)]
pub struct SolveContext<'a> {
    pub ans: Option<f64>,
    pub angle_mode: AngleMode,
    pub variables: &'a BTreeMap<String, f64>,
    pub functions: &'a BTreeMap<String, UserFunction>,
    pub rationalize: Option<bool>,
}

impl SolveContext<'_> {
    pub fn sci(&self) -> ScientificContext {
        ScientificContext {
            ans: self.ans,
            angle_mode: self.angle_mode,
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            depth: 0,
        }
    }
    fn fn_names(&self) -> Vec<String> {
        self.functions.keys().cloned().collect()
    }
}

/// `parseEquation`'s answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Equation {
    pub lhs: String,
    pub rhs: String,
    pub for_var: Option<String>,
    pub assign_var: Option<String>,
    pub explicit: bool,
    pub near: Option<String>,
}

/// `solve(eq, t)` as the words it means; anything else as given.
pub fn solve_call(text: &str) -> String {
    static HEAD: Lazy = Lazy::new(|| js(r"^solve\s*\(", "i"));
    static SYSTEM: Lazy = Lazy::new(|| js(r"^solve\s*\(\s*\{", "i"));
    static GUESSED: Lazy = Lazy::new(|| js(r"(?s)^([A-Za-z][A-Za-z0-9_]*|θ)\s*=\s*(.+)$", ""));
    static NAME: Lazy = Lazy::new(|| js(r"^(?:[A-Za-z][A-Za-z0-9_]*|θ)$", ""));
    let s = text.trim();
    let Ok(Some(m)) = HEAD.find(s) else {
        return text.to_string();
    };
    if !s.ends_with(')') || jsre::test(&SYSTEM, s) {
        return text.to_string();
    }
    let open = m.end() - 1;
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut comma: Option<usize> = None;
    for (i, &ch) in b.iter().enumerate().skip(open) {
        match ch {
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                depth -= 1;
                if depth == 0 && i != b.len() - 1 {
                    return text.to_string();
                }
            }
            b',' if depth == 1 => comma = Some(i),
            _ => {}
        }
    }
    let inner = &s[open + 1..s.len() - 1];
    let Some(comma) = comma else {
        return format!("solve {}", inner.trim());
    };
    let last = s[comma + 1..s.len() - 1].trim();
    let guessed = GUESSED.captures(last).ok().flatten();
    let variable = guessed
        .as_ref()
        .map_or(last, |c| jsre::group(c, 1))
        .to_string();
    if !jsre::test(&NAME, &variable) {
        return text.to_string();
    }
    let near = guessed
        .as_ref()
        .map(|c| format!(" near {}", jsre::group(c, 2).trim()))
        .unwrap_or_default();
    format!("solve {} for {variable}{near}", s[open + 1..comma].trim())
}

/// `… for x near 0.2`: the line without its guess, and the guess as typed.
pub fn split_near(text: &str) -> (String, Option<String>) {
    static RE: Lazy = Lazy::new(|| {
        js(
            r"(?s)^(.*\bfor\s+(?:[A-Za-z][A-Za-z0-9_]*|θ))\s+near\s+(.+)$",
            "i",
        )
    });
    match RE.captures(text.trim()).ok().flatten() {
        Some(c) => (
            jsre::group(&c, 1).to_string(),
            Some(jsre::group(&c, 2).trim().to_string()),
        ),
        None => (text.to_string(), None),
    }
}

/// None unless the line is shaped like an equation.
pub fn parse_equation(text: &str) -> Option<Equation> {
    static GRAPH: Lazy = Lazy::new(|| js(r"^graph(?:\s|$)", "i"));
    static SOLVE_CMD: Lazy = Lazy::new(|| js(r"(?s)^solve\s+(.+)$", "i"));
    static FOR_VAR: Lazy = Lazy::new(|| js(r"(?s)^(.+?)\s+for\s+([A-Za-z]|θ)$", ""));
    static REJECT: Lazy = Lazy::new(|| js(r"==|[<>!]=|[≤≥≠<>]", ""));
    static BARE_NAME: Lazy = Lazy::new(|| js(r"^(?:[A-Za-z][A-Za-z0-9]*|θ)$", ""));
    let (g_text, near) = split_near(&solve_call(text));
    let mut s = g_text.trim().to_string();
    if s.is_empty() || jsre::test(&GRAPH, &s) {
        return None;
    }
    let cmd = SOLVE_CMD
        .captures(s.as_str())
        .ok()
        .flatten()
        .map(|c| jsre::group(&c, 1).trim().to_string());
    if let Some(c) = &cmd {
        s = c.clone();
    }
    let named = FOR_VAR.captures(s.as_str()).ok().flatten().map(|c| {
        (
            jsre::group(&c, 1).trim().to_string(),
            jsre::group(&c, 2).to_string(),
        )
    });
    let for_var = named.as_ref().map(|n| n.1.clone());
    if let Some((body, _)) = &named {
        s = body.clone();
    }
    let explicit = cmd.is_some() || named.is_some();
    if jsre::test(&REJECT, &s) {
        return None;
    }
    let parts: Vec<&str> = s.split('=').collect();
    if parts.len() == 1 {
        return cmd.is_some().then(|| Equation {
            lhs: s.clone(),
            rhs: "0".into(),
            for_var,
            assign_var: None,
            explicit,
            near,
        });
    }
    if parts.len() != 2 {
        return None;
    }
    let lhs = parts[0].trim().to_string();
    let rhs = parts[1].trim().to_string();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let assign_var =
        (!explicit && jsre::test(&BARE_NAME, &lhs) && !known_name(&lhs)).then(|| lhs.clone());
    Some(Equation {
        lhs,
        rhs,
        for_var,
        assign_var,
        explicit,
        near,
    })
}

/// True for lines solve would read as an equation (`x = 5` is an assignment).
pub fn is_equation(text: &str) -> bool {
    let Some(eq) = parse_equation(text) else {
        return false;
    };
    let Some(v) = &eq.assign_var else {
        return true;
    };
    let re = jsre::compile(&format!(
        "(?<![A-Za-z_])(?:{})(?![A-Za-z0-9_])",
        names_pattern(std::slice::from_ref(v))
    ));
    jsre::test(&re, &eq.rhs)
}

/// `GROUPED_BEFORE_LETTER` with its variable-length look-behind checked by hand.
fn ungroup_before_letter(s: &str) -> String {
    static BODY: Lazy = Lazy::new(|| js(r"\d{1,3}(?:,\d{3})+(?=[A-Za-zθ(])", ""));
    let blocked = |p: usize| -> bool {
        let before = &s[..p];
        if let Some(c) = before.chars().last()
            && (c.is_ascii_digit() || c == '.' || c == ',')
        {
            return true;
        }
        // inside a call's parens: `name(` with no paren since
        if let Some(i) = before.rfind(['(', ')'])
            && before.as_bytes()[i] == b'('
        {
            let run: Vec<u8> = before[..i]
                .bytes()
                .rev()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .collect();
            if run.iter().any(|b| b.is_ascii_alphabetic() || *b == b'_') {
                return true;
            }
        }
        if let Some(i) = before.rfind(['[', ']'])
            && before.as_bytes()[i] == b'['
        {
            return true;
        }
        false
    };
    let mut out = String::new();
    let mut last = 0;
    let mut pos = 0;
    while pos <= s.len() {
        let Ok(Some(m)) = BODY.find_from_pos(s, pos) else {
            break;
        };
        if blocked(m.start()) {
            pos = m.start() + s[m.start()..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        out.push_str(&s[last..m.start()]);
        out.push_str(&m.as_str().replace(',', ""));
        last = m.end();
        pos = m.end();
    }
    out.push_str(&s[last..]);
    out
}

fn call_pattern(fn_names: &[String]) -> String {
    let mut all = callable();
    all.extend(fn_names.iter().cloned());
    names_pattern(&all)
}

fn ambiguous_call(side: &str, fn_names: &[String]) -> bool {
    let re = jsre::compile(&format!(
        r"(?<![A-Za-z_])(?:{})\s+[0-9]+(?:\.[0-9]+)?\s*(?:[A-Za-z]|θ)(?![A-Za-z])",
        call_pattern(fn_names)
    ));
    jsre::test(&re, side)
}

pub fn normalize_side(side: &str, fn_names: &[String]) -> String {
    let s = ungroup_before_letter(&normalize_math_text(side));
    let re = jsre::compile(&format!(
        r"(?<![A-Za-z_])({})\s+([A-Za-z]|θ)(?![A-Za-z0-9_(^!])",
        call_pattern(fn_names)
    ));
    let s = jsre::replace_all(&re, &s, |c| {
        format!("{}({})", jsre::group(c, 1), jsre::group(c, 2))
    })
    .into_owned();
    static LETTER_PAREN: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_])([A-Za-z]|θ)\s*\(", ""));
    jsre::replace_all(&LETTER_PAREN, &s, |c| {
        let l = jsre::group(c, 1);
        if fn_names.iter().any(|f| f == l) {
            jsre::group(c, 0).to_string()
        } else {
            format!("{l}*(")
        }
    })
    .into_owned()
}

/// Identifiers on one side, after compile's preprocessing; None on a parse error.
pub fn free_symbols(side: &str, fn_names: &[String]) -> Option<Vec<String>> {
    let node =
        crate::syntax::parse(&preprocess_ascii(&normalize_side(side, fn_names), fn_names)).ok()?;
    let mut out: Vec<String> = Vec::new();
    let theta = side.contains('θ');
    crate::symbolic::traverse(&node, &mut |n, callee| {
        if let Node::Symbol(name) = n
            && !callee
        {
            let name = if name == "theta" && theta {
                "θ".to_string()
            } else {
                name.clone()
            };
            if !out.contains(&name) {
                out.push(name);
            }
        }
    });
    Some(out)
}

/// The letter to solve for, or None when the line isn't a solve.
pub fn pick_unknown(eq: &Equation, ctx: &SolveContext<'_>) -> Option<String> {
    let fn_names = ctx.fn_names();
    let left = free_symbols(&eq.lhs, &fn_names)?;
    let right = free_symbols(&eq.rhs, &fn_names)?;
    let vars = ctx.variables;
    let fns = ctx.functions;
    if let Some(a) = &eq.assign_var
        && (!right.contains(a) || vars.contains_key(a))
    {
        return None;
    }
    let mut all: Vec<String> = Vec::new();
    for n in left.iter().chain(&right) {
        if !all.contains(n) {
            all.push(n.clone());
        }
    }
    let free: Vec<&String> = all
        .iter()
        .filter(|n| !known_name(n) && !vars.contains_key(*n) && !fns.contains_key(*n))
        .collect();
    if let Some(fv) = &eq.for_var {
        if !all.contains(fv) || free.iter().any(|n| *n != fv) {
            return None;
        }
        return Some(fv.clone());
    }
    if free.len() == 1 {
        return re_unknown(free[0]).then(|| free[0].clone());
    }
    if !free.is_empty() {
        return None;
    }
    let stored: Vec<&String> = all
        .iter()
        .filter(|n| vars.contains_key(*n) && re_unknown(n))
        .collect();
    (stored.len() == 1).then(|| stored[0].clone())
}

// ---- compiling a side ----

/// `compileScientific`: the text prepared once, evaluated with `variable` set.
#[derive(Debug)]
pub struct Compiled {
    node: Node,
    scope: Scope,
    variable: String,
    expr: String,
    cost: u64,
}

impl Compiled {
    pub fn new(text: &str, ctx: &ScientificContext, variable: &str) -> Option<Compiled> {
        let mut ctx = ctx.clone();
        ctx.variables.insert(variable.to_string(), 0.0);
        let expr = scientific::preprocess_checked(text, &ctx)?;
        let node = crate::syntax::parse(&expr).ok()?;
        let scope = Scope {
            variables: ctx.variables.clone(),
            functions: ctx.functions.clone(),
            angle_mode: ctx.angle_mode,
            ans: ctx.ans,
            // compileScientific leaves the module's complexTyped as it was: off
            complex_typed: false,
            depth: ctx.depth,
        };
        let cost = 5 + expr.encode_utf16().count() as u64;
        Some(Compiled {
            node,
            scope,
            variable: variable.to_string(),
            expr,
            cost,
        })
    }

    /// The value at `x` as a finite number, or None.
    pub fn number(&mut self, x: f64) -> Option<f64> {
        self.scope.variables.insert(self.variable.clone(), x);
        crate::work::spend(self.cost);
        let v = eval::evaluate(&self.node, &self.scope).ok()?;
        let out = scientific::finish(&v, &self.expr)?;
        (out.kind == ValueKind::Number && out.n.is_finite()).then_some(out.n)
    }
}

struct Budget {
    evals: Cell<u32>,
    work: WorkBudget,
    failed: Rc<Cell<bool>>,
}

impl Budget {
    fn new(failed: Rc<Cell<bool>>) -> Rc<Budget> {
        Rc::new(Budget {
            evals: Cell::new(0),
            work: WorkBudget::new(MAX_WORK),
            failed,
        })
    }
    fn tick(&self) -> bool {
        if self.failed.get() {
            return false;
        }
        let e = self.evals.get() + 1;
        self.evals.set(e);
        if e > MAX_EVALS || (e % 64 == 0 && self.work.over()) {
            self.failed.set(true);
            return false;
        }
        true
    }
}

struct Side {
    code: RefCell<Compiled>,
    budget: Rc<Budget>,
}

impl Side {
    fn new(side: &str, v: &str, ctx: &ScientificContext, budget: &Rc<Budget>) -> Option<Side> {
        let var = if v == "θ" { "theta" } else { v };
        Some(Side {
            code: RefCell::new(Compiled::new(side, ctx, var)?),
            budget: budget.clone(),
        })
    }
    fn at(&self, x: f64) -> Option<f64> {
        if !self.budget.tick() {
            return None;
        }
        self.code.borrow_mut().number(x)
    }
}

// ---- JS helpers ----

fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

fn pow(x: f64, y: f64) -> f64 {
    jsmath::pow(x, y)
}

fn cmp_f(a: f64, b: f64) -> std::cmp::Ordering {
    let d = a - b;
    if d < 0.0 {
        std::cmp::Ordering::Less
    } else if d > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

fn js_max(xs: &[f64]) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for &x in xs {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}

/// graph.ts `tidy`: the shortest decimal near `x` that `accept` still takes.
pub fn tidy(x: f64, mut accept: impl FnMut(f64) -> bool) -> f64 {
    if x != 0.0 && accept(0.0) {
        return 0.0;
    }
    for digits in 1..17 {
        let c = jsnum::parse(&jsnum::to_precision(x, digits)).unwrap_or(f64::NAN);
        if c == x {
            return x;
        }
        if accept(c) {
            return c;
        }
    }
    x
}

/// graph.ts `dependsOnAngle`.
pub fn depends_on_angle(
    mut a: impl FnMut(f64) -> Option<f64>,
    mut b: impl FnMut(f64) -> Option<f64>,
) -> bool {
    [0.37, 1.1, 2.9, -4.3, 7.7].iter().any(|&x| {
        let ya = a(x);
        let yb = b(x);
        match (ya, yb) {
            (Some(ya), Some(yb)) => (ya - yb).abs() > 1e-9 * 1f64.max(ya.abs()),
            (ya, yb) => ya.is_some() != yb.is_some(),
        }
    })
}

#[derive(Clone, Copy, Debug)]
struct Pt {
    x: f64,
    y: Option<f64>,
}

const INV_PHI: f64 = 0.618_033_988_749_895; // (Math.sqrt(5) - 1) / 2

fn inv_phi() -> f64 {
    (5f64.sqrt() - 1.0) / 2.0
}

/// graph.ts `goldenSection`.
fn golden_section(
    f: &dyn Fn(f64) -> Option<f64>,
    lo: f64,
    hi: f64,
    max: bool,
) -> Option<(f64, f64)> {
    let ip = inv_phi();
    debug_assert!((ip - INV_PHI).abs() < 1e-15);
    let s = if max { -1.0 } else { 1.0 };
    let g = |x: f64| f(x).map(|y| s * y);
    let (mut a, mut b) = (lo, hi);
    let mut c = b - ip * (b - a);
    let mut d = a + ip * (b - a);
    let mut gc = g(c);
    let mut gd = g(d);
    let mut k = 0;
    while k < 200 && c < d {
        let (Some(vc), Some(vd)) = (gc, gd) else {
            return None;
        };
        if vc < vd {
            b = d;
            d = c;
            gd = gc;
            c = b - ip * (b - a);
            gc = g(c);
        } else {
            a = c;
            c = d;
            gc = gd;
            d = a + ip * (b - a);
            gd = g(d);
        }
        k += 1;
    }
    let x = (a + b) / 2.0;
    f(x).map(|y| (x, y))
}

/// graph.ts `findCriticalPoints` with `f`: (x, y) of each turn.
fn find_critical_points(points: &[Pt], f: &dyn Fn(f64) -> Option<f64>) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for i in 1..points.len().saturating_sub(1) {
        let (prev, cur, next) = (points[i - 1], points[i], points[i + 1]);
        let (Some(py), Some(cy), Some(ny)) = (prev.y, cur.y, next.y) else {
            continue;
        };
        let dx1 = cur.x - prev.x;
        let dx2 = next.x - cur.x;
        if dx1 == 0.0 || dx2 == 0.0 {
            continue;
        }
        let d1 = (cy - py) / dx1;
        let d2 = (ny - cy) / dx2;
        if !d1.is_finite() || !d2.is_finite() {
            continue;
        }
        let (s1, s2) = (sign(d1), sign(d2));
        if s1 == 0.0 || s2 == 0.0 || s1 == s2 {
            continue;
        }
        let max = s1 > 0.0;
        let Some(best) = golden_section(f, prev.x, next.x, max) else {
            continue;
        };
        let variation = (cy - py).abs() + (ny - cy).abs();
        if (best.1 - cy).abs() > 4.0 * variation {
            continue;
        }
        let sg = if max { -1.0 } else { 1.0 };
        let tol = 8.0 * f64::EPSILON * best.1.abs().max(cy.abs());
        let x = tidy(best.0, |c| {
            if c < prev.x || c > next.x {
                return false;
            }
            f(c).is_some_and(|y| sg * y <= sg * best.1 + tol)
        });
        let y = if x == best.0 {
            best.1
        } else {
            f(x).unwrap_or(0.0)
        };
        out.push((x, y));
    }
    let mut dedup: Vec<(f64, f64)> = Vec::new();
    for p in out {
        if dedup.last().is_some_and(|l| (l.0 - p.0).abs() <= 1e-6) {
            continue;
        }
        dedup.push(p);
    }
    dedup
}

// ---- the solver ----

/// One of a solve's exact roots: a fraction or a closed form.
#[derive(Clone, Debug, PartialEq)]
pub enum Exact {
    Frac { n: f64, d: f64 },
    Text(String),
}

#[derive(Clone, Debug)]
struct Root {
    x: f64,
    sure: bool,
    exact: Option<Exact>,
}

impl Root {
    fn new(x: f64, sure: bool) -> Root {
        Root {
            x,
            sure,
            exact: None,
        }
    }
}

/// What a solve found, and how it shows.
#[derive(Clone, Debug)]
pub struct Solved {
    pub info: SolveInfo,
    pub exact: Option<Vec<Exact>>,
    pub closed_form: Option<String>,
    pub loose: Option<Vec<bool>>,
    pub evals: u32,
}

#[derive(Clone, Debug)]
struct Found {
    info: SolveInfo,
    exact: Option<Vec<Exact>>,
    closed_form: Option<String>,
    loose: Option<Vec<bool>>,
}

#[derive(Clone, Debug)]
struct Poly {
    c: Vec<f64>,
    max_y: f64,
}

const NODES: [f64; 5] = [-2.0, -1.0, 0.0, 1.0, 2.0];
const CHECKS: [f64; 8] = [0.37, -2.9, 7.3, 31.7, -113.5, 1234.5, -98765.4, 3.21e6];

fn fit_polynomial(s: &Side) -> Option<Poly> {
    let ys: Vec<Option<f64>> = NODES.iter().map(|&x| s.at(x)).collect();
    if ys.iter().any(Option::is_none) {
        return None;
    }
    let ys: Vec<f64> = ys.into_iter().map(|y| y.unwrap_or(0.0)).collect();
    let (a, b, c0, d, e) = (ys[0], ys[1], ys[2], ys[3], ys[4]);
    let mut c = vec![
        c0,
        (a - 8.0 * b + 8.0 * d - e) / 12.0,
        (-a + 16.0 * b - 30.0 * c0 + 16.0 * d - e) / 24.0,
        (-a + 2.0 * b - 2.0 * d + e) / 12.0,
        (a - 4.0 * b + 6.0 * c0 - 4.0 * d + e) / 24.0,
    ];
    let max_y = js_max(&ys.iter().map(|y| y.abs()).collect::<Vec<_>>());
    for (k, ck) in c.iter_mut().enumerate().skip(1) {
        if ck.abs() * pow(2.0, k as f64) <= 1e-13 * max_y {
            *ck = 0.0;
        }
    }
    for &t in &CHECKS {
        let y = s.at(t)?;
        let mut p = 0.0;
        let mut mag = y.abs();
        for (k, ck) in c.iter().enumerate() {
            let term = ck * pow(t, k as f64);
            p += term;
            mag += term.abs();
        }
        if (y - p).abs() > 1e-9 * mag {
            return None;
        }
    }
    Some(Poly { c, max_y })
}

fn horner(c: &[f64], x: f64) -> f64 {
    let mut y = 0.0;
    for k in (0..c.len()).rev() {
        y = y * x + c[k];
    }
    y
}

fn degree_of(c: &[f64]) -> usize {
    let mut n = c.len() - 1;
    while n > 0 && c[n] == 0.0 {
        n -= 1;
    }
    n
}

fn to_frac(x: f64) -> Option<(f64, f64)> {
    if x == 0.0 {
        return Some((0.0, 1.0));
    }
    for d in 1..=1000 {
        let d = f64::from(d);
        let n = jsnum::round(x * d);
        if (n / d - x).abs() <= 1e-11 * x.abs() {
            return Some((n, d));
        }
    }
    None
}

fn ratio(n: f64, d: f64) -> (f64, f64) {
    let g = int_gcd(n, d) * if d < 0.0 { -1.0 } else { 1.0 };
    (n / g, d / g)
}

fn integer_coeffs(c: &[f64]) -> Option<Vec<f64>> {
    let fr: Vec<(f64, f64)> = c.iter().map(|&x| to_frac(x)).collect::<Option<_>>()?;
    let mut lcm = 1.0;
    for f in &fr {
        lcm = (lcm * f.1) / int_gcd(lcm, f.1);
        if !jsnum::is_safe_integer(lcm) {
            return None;
        }
    }
    let ints: Vec<f64> = fr.iter().map(|f| (f.0 * lcm) / f.1).collect();
    ints.iter()
        .all(|x| jsnum::is_safe_integer(*x))
        .then_some(ints)
}

fn fitted_roots(c: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    let n = degree_of(c);
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        let x = -c[0] / c[1];
        return if x > lo && x < hi {
            vec![x]
        } else {
            Vec::new()
        };
    }
    let deriv: Vec<f64> = c[1..=n]
        .iter()
        .enumerate()
        .map(|(k, v)| v * (k as f64 + 1.0))
        .collect();
    let crit = fitted_roots(&deriv, lo, hi);
    let mut breaks = vec![lo];
    breaks.extend(crit);
    breaks.push(hi);
    let mut out = Vec::new();
    for i in 1..breaks.len() {
        let mut a = breaks[i - 1];
        let mut b = breaks[i];
        let mut ya = horner(c, a);
        let yb = horner(c, b);
        if ya == 0.0 && i > 1 {
            out.push(a);
        }
        if sign(ya) * sign(yb) >= 0.0 {
            continue;
        }
        for _ in 0..200 {
            let m = (a + b) / 2.0;
            if m <= a || m >= b {
                break;
            }
            let ym = horner(c, m);
            if sign(ym) == sign(ya) {
                a = m;
                ya = ym;
            } else {
                b = m;
            }
        }
        out.push((a + b) / 2.0);
    }
    out
}

fn next_float(x: f64, dir: f64) -> f64 {
    if x == 0.0 {
        return dir * MIN_VALUE;
    }
    let ulp = pow(2.0, (jsmath_log2(x.abs())).floor() - 52.0);
    x + dir * ulp.max(MIN_VALUE)
}

fn jsmath_log2(x: f64) -> f64 {
    jsmath::log2(x)
}

fn linear(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| lo + (hi - lo) * (i as f64 / (n as f64 - 1.0)))
        .collect()
}

fn wide_grid() -> &'static [f64] {
    static GRID: std::sync::LazyLock<Vec<f64>> = std::sync::LazyLock::new(|| {
        let mut xs: Vec<f64> = Vec::new();
        let add = |v: f64, xs: &mut Vec<f64>| {
            if !xs.contains(&v) {
                xs.push(v);
            }
        };
        for v in linear(-10.0, 10.0, 401) {
            add(v, &mut xs);
        }
        for k in 1..=96 {
            let v = pow(10.0, 1.0 + f64::from(k) / 16.0);
            add(v, &mut xs);
            add(-v, &mut xs);
        }
        for k in 1..=36 {
            let v = pow(10.0, -f64::from(k) / 4.0);
            add(v, &mut xs);
            add(-v, &mut xs);
        }
        xs.sort_by(|a, b| cmp_f(*a, *b));
        xs
    });
    &GRID
}

fn stable_roots(a: f64, b: f64, c: f64) -> Vec<f64> {
    let d = b * b - 4.0 * a * c;
    let q = -(b + if b < 0.0 { -1.0 } else { 1.0 } * d.max(0.0).sqrt()) / 2.0;
    if q == 0.0 {
        return vec![0.0];
    }
    let mut r = vec![q / a, c / q];
    r.sort_by(|x, y| cmp_f(*x, *y));
    r
}

fn dedupe(roots: Vec<Root>) -> Vec<Root> {
    let mut sorted = roots;
    sorted.sort_by(|a, b| cmp_f(a.x, b.x));
    let mut out: Vec<Root> = Vec::new();
    for r in sorted {
        if let Some(last) = out.last_mut() {
            let tol = if last.sure && r.sure { 1e-12 } else { 1e-7 };
            if (r.x - last.x).abs() <= tol * r.x.abs().max(last.x.abs()) {
                if !last.sure && r.sure {
                    *last = r;
                }
                continue;
            }
        }
        out.push(r);
    }
    out
}

fn pick_shown(roots: &[Root]) -> Vec<Root> {
    if roots.len() <= MAX_SHOWN_ROOTS {
        return roots.to_vec();
    }
    let mut near = roots.to_vec();
    near.sort_by(|a, b| cmp_f(a.x.abs(), b.x.abs()));
    near.truncate(MAX_SHOWN_ROOTS);
    near.sort_by(|a, b| cmp_f(a.x, b.x));
    near
}

#[derive(Clone, Copy)]
enum JobKind {
    Crossing(Pt, Pt),
    Touching(Pt, Pt, Pt),
    Pole(Pt, Pt, bool),
}

struct Job {
    at: f64,
    crossing: bool,
    kind: JobKind,
}

struct Solver<'c> {
    l: Side,
    r: Side,
    lhs: String,
    rhs: String,
    v: String,
    ctx: &'c ScientificContext,
    rationalize: Option<bool>,
    mode: AngleMode,
    poly: Option<(Poly, Poly)>,
    failed: Rc<Cell<bool>>,
}

impl Solver<'_> {
    fn f(&self, x: f64) -> Option<f64> {
        let a = self.l.at(x)?;
        let b = self.r.at(x)?;
        Some(a - b)
    }

    fn run(&mut self) -> Option<Found> {
        let pl = fit_polynomial(&self.l);
        let pr = pl.as_ref().and_then(|_| fit_polynomial(&self.r));
        if let (Some(pl), Some(pr)) = (pl, pr) {
            self.poly = Some((pl, pr));
            if let Some(out) = self.polynomial() {
                return Some(out);
            }
            self.poly = None;
        }
        self.numeric()
    }

    fn found(
        &self,
        outcome: SolveOutcome,
        roots: &[Root],
        more: bool,
        closed_form: Option<String>,
    ) -> Found {
        let shown = pick_shown(roots);
        let mut info = SolveInfo {
            variable: self.v.clone(),
            roots: shown.iter().map(|r| r.x).collect(),
            outcome,
            more: None,
        };
        if more || roots.len() > shown.len() {
            info.more = Some(true);
        }
        let exact = (!shown.is_empty() && shown.iter().all(|r| r.exact.is_some()))
            .then(|| shown.iter().filter_map(|r| r.exact.clone()).collect());
        let loose = shown
            .iter()
            .any(|r| !r.sure)
            .then(|| shown.iter().map(|r| !r.sure).collect());
        Found {
            info,
            exact,
            closed_form,
            loose,
        }
    }

    fn flat_far(&self, y: f64) -> bool {
        for t in [1e30, -1e30] {
            let a = self.l.at(t);
            let b = self.r.at(t);
            if let (Some(a), Some(b)) = (a, b)
                && (a - b - y).abs() > 1e-9 * a.abs().max(b.abs())
            {
                return false;
            }
        }
        true
    }

    fn scale(&self, x: f64) -> f64 {
        if let Some((l, r)) = &self.poly {
            let mut s = 0.0;
            for k in 0..=4 {
                s += (l.c[k].abs() + r.c[k].abs()) * pow(x.abs(), k as f64);
            }
            return s;
        }
        let a = self.l.at(x).unwrap_or(0.0).abs();
        let b = self.r.at(x).unwrap_or(0.0).abs();
        a.max(b)
    }

    fn bisect(&self, a: f64, ya: f64, b: f64, yb: f64) -> Option<f64> {
        let (mut lo, mut hi, mut ylo) = (a, b, ya);
        for _ in 0..200 {
            let mid = (lo + hi) / 2.0;
            if mid <= lo || mid >= hi {
                break;
            }
            let ym = self.f(mid)?;
            if ym == 0.0 {
                return Some(mid);
            }
            if sign(ym) == sign(ylo) {
                lo = mid;
                ylo = ym;
            } else {
                hi = mid;
            }
        }
        let flo = self.f(lo);
        let fhi = self.f(hi);
        let (flo, fhi) = (flo?, fhi?);
        let (x, y) = if flo.abs() <= fhi.abs() {
            (lo, flo)
        } else {
            (hi, fhi)
        };
        if y.abs() > 1e-6 * ya.abs().max(yb.abs()) {
            return None;
        }
        let tol = y.abs() + NOISE * self.scale(x);
        Some(tidy(x, |c| {
            if c < a || c > b {
                return false;
            }
            self.f(c).is_some_and(|yc| yc.abs() <= tol)
        }))
    }

    fn settle(&self, x: f64) -> Option<Root> {
        let y = self.f(x)?;
        if y == 0.0 {
            return Some(Root::new(x, true));
        }
        let h = 1e-9 * x.abs().max(1e-6);
        for (a, b) in [(x - h, x), (x, x + h)] {
            let ya = if a == x { Some(y) } else { self.f(a) };
            let yb = if b == x { Some(y) } else { self.f(b) };
            let (Some(ya), Some(yb)) = (ya, yb) else {
                continue;
            };
            if ya == 0.0 {
                return Some(Root::new(a, true));
            }
            if yb == 0.0 {
                return Some(Root::new(b, true));
            }
            if sign(ya) != sign(yb) {
                return self.bisect(a, ya, b, yb).map(|r| Root::new(r, true));
            }
        }
        (y.abs() <= NOISE * self.scale(x)).then(|| Root::new(x, true))
    }

    fn accept(&self, root: Root, exact: Exact, value: f64) -> Root {
        if root.sure && (value - root.x).abs() > 1e-12 * root.x.abs() {
            return root;
        }
        let fx = self.f(root.x).unwrap_or(f64::INFINITY).abs();
        let fc = self.f(value);
        match fc {
            Some(fc) if fc.abs() <= fx.max(NOISE * self.scale(value)) => Root {
                x: value,
                sure: true,
                exact: Some(exact),
            },
            _ => root,
        }
    }

    fn exact_root(&self, root: Root) -> Root {
        static SEVEN: Lazy = Lazy::new(|| js(r"\d{7}", ""));
        static INT: Lazy = Lazy::new(|| js(r"^-?\d+$", ""));
        static FRAC: Lazy = Lazy::new(|| js(r"^(-?\d+)(?:\/(\d+))?$", ""));
        let s = if root.x.abs() <= 1e-11 {
            Some("0".to_string())
        } else if root.x.abs() <= 1e4 {
            exact_form(
                root.x,
                ExactFormOptions {
                    rationalize: self.rationalize,
                    wide: true,
                },
            )
        } else {
            Some(jsnum::to_string(jsnum::round(root.x)))
        };
        let Some(s) = s.filter(|s| !s.is_empty()) else {
            return root;
        };
        let stripped = if jsre::test(&INT, &s) { "" } else { s.as_str() };
        if jsre::test(&SEVEN, stripped) {
            return root;
        }
        if let Ok(Some(c)) = FRAC.captures(s.as_str()) {
            let n = jsnum::parse(jsre::group(&c, 1)).unwrap_or(f64::NAN);
            let d = c
                .get(2)
                .map_or(1.0, |m| jsnum::parse(m.as_str()).unwrap_or(f64::NAN));
            let (rn, rd) = ratio(n, d);
            return self.accept(root, Exact::Frac { n: rn, d: rd }, n / d);
        }
        let ctx = ScientificContext {
            angle_mode: self.mode,
            ..ScientificContext::default()
        };
        match scientific::eval_scientific(&s, &ctx) {
            Some(c) if c.kind == ValueKind::Number && c.n.is_finite() => {
                self.accept(root, Exact::Text(s), c.n)
            }
            _ => root,
        }
    }

    fn settle_all(&self, cands: &[(f64, Option<(f64, f64)>)]) -> Option<Vec<Root>> {
        let mut out = Vec::new();
        for &(x, exact) in cands {
            let root = self.settle(x)?;
            out.push(match exact {
                Some((n, d)) => self.accept(root, Exact::Frac { n, d }, n / d),
                None => self.exact_root(root),
            });
        }
        Some(dedupe(out))
    }

    fn polynomial(&self) -> Option<Found> {
        let (l, r) = self.poly.as_ref()?;
        let d: Vec<f64> =
            l.c.iter()
                .enumerate()
                .map(|(k, &a)| {
                    let b = r.c[k];
                    let size = a.abs().max(b.abs());
                    let tol = if k == 0 {
                        1e-12 * size
                    } else {
                        ((1e-14 * (l.max_y + r.max_y)) / pow(2.0, k as f64)).min(1e-9 * size)
                    };
                    if (a - b).abs() <= tol { 0.0 } else { a - b }
                })
                .collect();
        let n = degree_of(&d);
        if n == 0 {
            return self.flat_far(d[0]).then(|| {
                self.found(
                    if d[0] == 0.0 {
                        SolveOutcome::All
                    } else {
                        SolveOutcome::Contradiction
                    },
                    &[],
                    false,
                    None,
                )
            });
        }
        let ints = integer_coeffs(&d[..=n]);
        if n == 1 {
            let exact = ints.as_ref().map(|i| ratio(-i[0], i[1]));
            let x = exact.map_or(-d[0] / d[1], |e| e.0 / e.1);
            let roots = self.settle_all(&[(x, exact)])?;
            return Some(self.found(SolveOutcome::Roots, &roots, false, None));
        }
        if n == 2 {
            return self.quadratic(&d, ints.as_deref());
        }
        let bound = 1.0 + js_max(&d[..n].iter().map(|c| (c / d[n]).abs()).collect::<Vec<_>>());
        let deriv: Vec<f64> = d[1..=n]
            .iter()
            .enumerate()
            .map(|(k, c)| c * (k as f64 + 1.0))
            .collect();
        let crit = fitted_roots(&deriv, -bound, bound);
        let mut breaks = vec![-bound];
        breaks.extend(crit);
        breaks.push(bound);
        let ys: Vec<Option<f64>> = breaks.iter().map(|&x| self.f(x)).collect();
        if ys.iter().any(Option::is_none) {
            return None;
        }
        let ys: Vec<f64> = ys.into_iter().map(|y| y.unwrap_or(0.0)).collect();
        let mut cands: Vec<(f64, Option<(f64, f64)>)> = Vec::new();
        for i in 0..breaks.len() {
            let y = ys[i];
            if i > 0 && i < breaks.len() - 1 && y.abs() <= NOISE * self.scale(breaks[i]) {
                cands.push((breaks[i], None));
            }
            if i == 0 || y == 0.0 || ys[i - 1] == 0.0 || sign(y) == sign(ys[i - 1]) {
                continue;
            }
            let r = self.bisect(breaks[i - 1], ys[i - 1], breaks[i], y)?;
            cands.push((r, None));
        }
        let roots = self.settle_all(&cands)?;
        Some(self.found(
            if roots.is_empty() {
                SolveOutcome::None
            } else {
                SolveOutcome::Roots
            },
            &roots,
            false,
            None,
        ))
    }

    fn quadratic(&self, d: &[f64], ints: Option<&[f64]>) -> Option<Found> {
        let (c, b, a) = (d[0], d[1], d[2]);
        if let Some(ints) = ints {
            let (cc, bb, aa) = (ints[0], ints[1], ints[2]);
            let disc = bb * bb - 4.0 * aa * cc;
            let size = bb * bb + 4.0 * (aa * cc).abs();
            if jsnum::is_safe_integer(size) && (disc == 0.0 || disc.abs() * 1e10 >= size) {
                if disc < 0.0 {
                    return Some(self.found(SolveOutcome::None, &[], false, None));
                }
                if disc == 0.0 {
                    let r = ratio(-bb, 2.0 * aa);
                    let roots = self.settle_all(&[(r.0 / r.1, Some(r))])?;
                    return Some(self.found(SolveOutcome::Roots, &roots, false, None));
                }
                let s = jsnum::round(disc.sqrt());
                if s * s == disc {
                    let cands: Vec<(f64, Option<(f64, f64)>)> =
                        [ratio(-bb - s, 2.0 * aa), ratio(-bb + s, 2.0 * aa)]
                            .iter()
                            .map(|r| (r.0 / r.1, Some(*r)))
                            .collect();
                    let roots = self.settle_all(&cands)?;
                    return Some(self.found(SolveOutcome::Roots, &roots, false, None));
                }
                let cands: Vec<(f64, Option<(f64, f64)>)> = stable_roots(aa, bb, cc)
                    .into_iter()
                    .map(|x| (x, None))
                    .collect();
                let roots = self.settle_all(&cands)?;
                let closed = if disc <= 1e10 {
                    self.closed_form(aa, bb, disc, &roots)
                } else {
                    None
                };
                return Some(self.found(SolveOutcome::Roots, &roots, false, closed));
            }
        }
        let (l, r) = self.poly.as_ref()?;
        let noise = 32.0
            * f64::EPSILON
            * (l.max_y + r.max_y)
            * (2.0 * b.abs() + 4.0 * a.abs() + 4.0 * c.abs());
        let disc = b * b - 4.0 * a * c;
        if disc < -noise {
            return Some(self.found(SolveOutcome::None, &[], false, None));
        }
        if disc.abs() <= noise
            && disc > 0.0
            && let Some(pair) = self.straddle(&stable_roots(a, b, c))
        {
            return Some(pair);
        }
        let cands: Vec<f64> = if disc.abs() <= noise {
            vec![-b / (2.0 * a)]
        } else {
            stable_roots(a, b, c)
        };
        let cands: Vec<(f64, Option<(f64, f64)>)> = cands.into_iter().map(|x| (x, None)).collect();
        let roots = self.settle_all(&cands)?;
        Some(self.found(SolveOutcome::Roots, &roots, false, None))
    }

    fn straddle(&self, pair: &[f64]) -> Option<Found> {
        let lo = pair.first().copied().unwrap_or(f64::NAN);
        let hi = pair.get(1).copied().unwrap_or(f64::NAN);
        let gap = hi - lo;
        if !(gap > 0.0) {
            return None;
        }
        let ys: Vec<Option<f64>> = [lo - gap, (lo + hi) / 2.0, hi + gap]
            .iter()
            .map(|&x| self.f(x))
            .collect();
        if ys.iter().any(|y| y.is_none_or(|y| y == 0.0)) {
            return None;
        }
        let s: Vec<f64> = ys.iter().map(|y| sign(y.unwrap_or(0.0))).collect();
        if s[0] != s[2] || s[1] == s[0] {
            return None;
        }
        let roots = self.settle_all(&[(lo, None), (hi, None)])?;
        (roots.len() == 2).then(|| self.found(SolveOutcome::Roots, &roots, false, None))
    }

    fn closed_form(&self, aa: f64, bb: f64, disc: f64, roots: &[Root]) -> Option<String> {
        if roots.len() != 2 {
            return None;
        }
        let (k, rad) = split_squares(disc);
        let mut p = -bb;
        let mut q = k;
        let mut d = 2.0 * aa;
        if d < 0.0 {
            p = -p;
            d = -d;
        }
        let g = int_gcd(int_gcd(p, q), d);
        p /= g;
        q /= g;
        d /= g;
        let mut values = [(p - q * rad.sqrt()) / d, (p + q * rad.sqrt()) / d];
        values.sort_by(|x, y| cmp_f(*x, *y));
        let ok = roots.iter().enumerate().all(|(i, r)| {
            let fc = self.f(values[i]);
            let fx = self.f(r.x).unwrap_or(f64::INFINITY).abs();
            fc.is_some_and(|fc| fc.abs() <= 4.0 * fx.max(NOISE * self.scale(values[i])))
        });
        if !ok {
            return None;
        }
        let s = jsnum::to_string;
        let term = format!(
            "{}sqrt({})",
            if q == 1.0 { String::new() } else { s(q) },
            s(rad)
        );
        if p == 0.0
            && d != 1.0
            && self.rationalize == Some(false)
            && let Some(plain) = exact_form(
                values[1],
                ExactFormOptions {
                    rationalize: Some(false),
                    wide: false,
                },
            )
        {
            return Some(format!("±{plain}"));
        }
        Some(if p == 0.0 {
            if d == 1.0 {
                format!("±{term}")
            } else {
                format!("±{term}/{}", s(d))
            }
        } else if d == 1.0 {
            format!("{} ± {term}", s(p))
        } else {
            format!("({} ± {term})/{}", s(p), s(d))
        })
    }

    fn numeric(&self) -> Option<Found> {
        let period = if self.mode == AngleMode::Deg {
            360.0
        } else {
            2.0 * std::f64::consts::PI
        };
        let periodic = self.periodic(period);
        let xs: Vec<f64> = if periodic {
            linear(0.0, period, 401)
        } else {
            wide_grid().to_vec()
        };
        let mut sizes: Vec<f64> = Vec::new();
        let mut sides: Vec<(Option<f64>, Option<f64>)> = Vec::new();
        let pts: Vec<Pt> = xs
            .iter()
            .map(|&x| {
                let a = self.l.at(x);
                let b = if a.is_none() { None } else { self.r.at(x) };
                sizes.push(a.unwrap_or(0.0).abs().max(b.unwrap_or(0.0).abs()));
                sides.push((a, b));
                Pt {
                    x,
                    y: match (a, b) {
                        (Some(a), Some(b)) => Some(a - b),
                        _ => None,
                    },
                }
            })
            .collect();
        let defined = pts.iter().filter(|p| p.y.is_some()).count();
        if defined == 0 {
            return None;
        }
        let at = |i: isize| -> Option<Pt> {
            (i >= 0 && (i as usize) < pts.len()).then(|| pts[i as usize])
        };
        let small = |i: isize| -> bool {
            at(i)
                .and_then(|p| p.y)
                .is_some_and(|y| y.abs() <= 1e-12 * sizes[i as usize])
        };
        if (0..pts.len()).all(|i| pts[i].y.is_none() || small(i as isize)) {
            return (defined == pts.len() && self.flat_far(0.0))
                .then(|| self.found(SolveOutcome::All, &[], false, None));
        }
        let mut roots: Vec<Root> = Vec::new();
        let mut jobs: Vec<Job> = Vec::new();
        let cross = |a: Pt, b: Pt, jobs: &mut Vec<Job>| {
            jobs.push(Job {
                at: a.x.abs().min(b.x.abs()),
                crossing: true,
                kind: JobKind::Crossing(a, b),
            });
        };
        let live = |i: isize| at(i).and_then(|p| p.y).is_some() && !small(i);
        let y_of = |i: isize| at(i).and_then(|p| p.y).unwrap_or(0.0);
        let mut i: isize = 0;
        while (i as usize) < pts.len() {
            let p = pts[i as usize];
            if small(i) {
                let mut j = i;
                while ((j + 1) as usize) < pts.len() && small(j + 1) {
                    j += 1;
                }
                let zeros: Vec<&Pt> = pts[i as usize..=j as usize]
                    .iter()
                    .filter(|q| q.y == Some(0.0))
                    .collect();
                let reach = 1f64
                    .max(pts[i as usize].x.abs())
                    .max(pts[j as usize].x.abs());
                let flat_zero =
                    zeros.len() > 1 && zeros[zeros.len() - 1].x - zeros[0].x > 1e-6 * reach;
                if flat_zero || pts[j as usize].x - pts[i as usize].x > 1e-3 * reach {
                    return None;
                }
                let hole = |m: isize| {
                    at(m).is_some_and(|p| p.y.is_none())
                        && at(m - 1).and_then(|p| p.y).is_some()
                        && at(m + 1).and_then(|p| p.y).is_some()
                };
                if hole(i - 1) || hole(j + 1) {
                    i = j + 1;
                    continue;
                }
                let opposite = live(i - 1) && live(j + 1) && sign(y_of(i - 1)) != sign(y_of(j + 1));
                if opposite {
                    cross(pts[(i - 1) as usize], pts[(j + 1) as usize], &mut jobs);
                } else {
                    let mut k = i as usize;
                    for m in i as usize..=j as usize {
                        let (ym, yk) = (pts[m].y.unwrap_or(0.0), pts[k].y.unwrap_or(0.0));
                        if ym.abs() < yk.abs() || (ym == yk && pts[m].x.abs() < pts[k].x.abs()) {
                            k = m;
                        }
                    }
                    roots.push(Root::new(pts[k].x, false));
                }
                i = j + 1;
                continue;
            }
            if !live(i) {
                i += 1;
                continue;
            }
            let py = p.y.unwrap_or(0.0);
            if live(i - 1) && sign(y_of(i - 1)) != sign(py) {
                cross(pts[(i - 1) as usize], p, &mut jobs);
            }
            if live(i - 1) && live(i + 1) {
                let prev = pts[(i - 1) as usize];
                let next = pts[(i + 1) as usize];
                let (prev_y, next_y) = (prev.y.unwrap_or(0.0), next.y.unwrap_or(0.0));
                let s = sign(py);
                let low = py.abs();
                let rise = prev_y.abs() - low + next_y.abs() - low;
                let dips = sign(prev_y) == s
                    && sign(next_y) == s
                    && low < prev_y.abs()
                    && low < next_y.abs();
                if dips && low <= rise {
                    jobs.push(Job {
                        at: p.x.abs(),
                        crossing: false,
                        kind: JobKind::Touching(prev, p, next),
                    });
                }
            }
            i += 1;
        }
        let mut pole_cells: Vec<usize> = Vec::new();
        for i in 1..pts.len() {
            let ii = i as isize;
            if !live(ii - 1) || !live(ii) || sign(y_of(ii - 1)) != sign(y_of(ii)) {
                continue;
            }
            let flips = |k: usize| {
                let (a, b) = if k == 0 {
                    (sides[i - 1].0, sides[i].0)
                } else {
                    (sides[i - 1].1, sides[i].1)
                };
                a.unwrap_or(0.0) * b.unwrap_or(0.0) < 0.0
            };
            if flips(0) || flips(1) {
                pole_cells.push(i);
            }
        }
        let cell_at = |p: usize| pts[p - 1].x.abs().min(pts[p].x.abs());
        pole_cells.sort_by(|p, q| cmp_f(cell_at(*p), cell_at(*q)));
        for &i in pole_cells.iter().take(MAX_POLE_CELLS) {
            let a = pts[i - 1];
            let b = pts[i];
            let left = sides[i - 1].0.unwrap_or(0.0) * sides[i].0.unwrap_or(0.0) < 0.0;
            jobs.push(Job {
                at: a.x.abs().min(b.x.abs()),
                crossing: true,
                kind: JobKind::Pole(a, b, left),
            });
        }
        jobs.sort_by(|a, b| cmp_f(a.at, b.at));
        for job in jobs.iter().take(MAX_REFINED) {
            let r = match job.kind {
                JobKind::Crossing(a, b) => {
                    self.crossing(a.x, a.y.unwrap_or(0.0), b.x, b.y.unwrap_or(0.0))
                }
                JobKind::Touching(p, c, n) => self.touching(p, c, n),
                JobKind::Pole(a, b, left) => self.beside_pole(a, b, left),
            };
            if let Some(r) = r {
                roots.push(r);
            }
        }
        roots.extend(self.domain_edges(&pts));
        let in_window: Vec<Root> = if periodic {
            roots
                .into_iter()
                .filter(|r| r.x < period * (1.0 - 1e-9))
                .collect()
        } else {
            roots
        };
        let all = dedupe(in_window.into_iter().map(|r| self.exact_root(r)).collect());
        let skipped = jobs.iter().skip(MAX_REFINED).any(|j| j.crossing);
        Some(self.found(
            if all.is_empty() {
                SolveOutcome::NoneFound
            } else {
                SolveOutcome::Roots
            },
            &all,
            skipped,
            None,
        ))
    }

    fn crossing(&self, a: f64, ya: f64, b: f64, yb: f64) -> Option<Root> {
        let x = self.bisect(a, ya, b, yb)?;
        (!self.hole_beside(x)).then(|| Root::new(x, true))
    }

    fn beside_pole(&self, a: Pt, b: Pt, left: bool) -> Option<Root> {
        let s = |x: f64| if left { self.l.at(x) } else { self.r.at(x) };
        let sa = s(a.x)?;
        let mut lo = a.x;
        let mut hi = b.x;
        for _ in 0..64 {
            let m = (lo + hi) / 2.0;
            if m <= lo || m >= hi {
                break;
            }
            let Some(sm) = s(m) else {
                lo = self.last_defined(lo, m);
                hi = self.last_defined(hi, m);
                break;
            };
            if sign(sm) == sign(sa) {
                lo = m;
            } else {
                hi = m;
            }
        }
        let ylo = self.f(lo);
        let yhi = self.f(hi);
        let (ay, by) = (a.y.unwrap_or(0.0), b.y.unwrap_or(0.0));
        if let Some(ylo) = ylo
            && ylo != 0.0
            && sign(ylo) != sign(ay)
        {
            return self.crossing(a.x, ay, lo, ylo);
        }
        if let Some(yhi) = yhi
            && yhi != 0.0
            && sign(yhi) != sign(by)
        {
            return self.crossing(hi, yhi, b.x, by);
        }
        None
    }

    fn last_defined(&self, mut def: f64, mut undef: f64) -> f64 {
        for _ in 0..64 {
            let m = (def + undef) / 2.0;
            if m == def || m == undef {
                break;
            }
            if self.f(m).is_none() {
                undef = m;
            } else {
                def = m;
            }
        }
        def
    }

    fn hole_beside(&self, x: f64) -> bool {
        for dir in [-1.0, 1.0] {
            let a = next_float(x, dir);
            if self.f(a).is_none() && self.f(next_float(a, dir)).is_some() {
                return true;
            }
        }
        false
    }

    fn touching(&self, prev: Pt, cur: Pt, next: Pt) -> Option<Root> {
        let f = |x: f64| self.f(x);
        let best = *find_critical_points(&[prev, cur, next], &f).first()?;
        let dx = NOISE * best.0.abs();
        let nudge = js_max(
            &[best.0 - dx, best.0 + dx]
                .iter()
                .map(|&c| (self.f(c).unwrap_or(best.1) - best.1).abs())
                .collect::<Vec<_>>(),
        );
        let tidied = 8.0 * f64::EPSILON * cur.y.unwrap_or(0.0).abs();
        if best.1.abs() > NOISE * self.scale(best.0) + nudge + tidied {
            return None;
        }
        Some(Root::new(best.0, false))
    }

    fn domain_edges(&self, pts: &[Pt]) -> Vec<Root> {
        let mut edges: Vec<(Pt, Pt)> = Vec::new();
        for i in 1..pts.len() {
            if pts[i - 1].y.is_none() != pts[i].y.is_none() {
                edges.push((pts[i - 1], pts[i]));
            }
        }
        edges.sort_by(|p, q| cmp_f(p.0.x.abs(), q.0.x.abs()));
        let mut out = Vec::new();
        for (a, b) in edges.into_iter().take(4) {
            let mut def = if a.y.is_none() { b.x } else { a.x };
            let mut undef = if a.y.is_none() { a.x } else { b.x };
            let mut pinned = false;
            for _ in 0..80 {
                let m = (def + undef) / 2.0;
                pinned = m == def || m == undef;
                if pinned {
                    break;
                }
                if self.f(m).is_none() {
                    undef = m;
                } else {
                    def = m;
                }
            }
            let dir = sign(undef - def);
            let probe = if pinned {
                next_float(undef, dir)
            } else {
                undef + (undef - def)
            };
            if self.f(probe).is_some() {
                continue;
            }
            let Some(y) = self.f(def) else {
                continue;
            };
            let inner = if pinned {
                self.f(next_float(def, -dir))
            } else {
                None
            };
            let moved = inner.map_or(0.0, |v| (v - y).abs());
            let tol = if pinned {
                (NOISE * 1f64.max(self.scale(def))).max(4.0 * moved)
            } else {
                0.0
            };
            if y.abs() <= tol {
                out.push(Root::new(def, true));
            }
        }
        out
    }

    fn periodic(&self, period: f64) -> bool {
        let probes = [0.37, 1.9, -4.3, 7.7, 23.1];
        let repeats = probes.iter().all(|&t| {
            let a = self.f(t);
            let b = self.f(t + period);
            match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() <= 1e-9 * 1f64.max(a.abs()),
                (a, b) => a.is_none() && b.is_none(),
            }
        });
        if !repeats {
            return false;
        }
        let mut other = self.ctx.clone();
        other.angle_mode = if self.mode == AngleMode::Deg {
            AngleMode::Rad
        } else {
            AngleMode::Deg
        };
        let budget = Budget::new(self.failed.clone());
        let Some(l2) = Side::new(&self.lhs, &self.v, &other, &budget) else {
            return false;
        };
        let Some(r2) = Side::new(&self.rhs, &self.v, &other, &budget) else {
            return false;
        };
        let g = |x: f64| {
            let a = l2.at(x);
            let b = r2.at(x);
            Some(a? - b?)
        };
        depends_on_angle(|x| self.f(x), g)
    }
}

fn guess_value(text: &str, ctx: &ScientificContext) -> Option<f64> {
    let mut c = Compiled::new(&normalize_math_text(text), ctx, "__none__")?;
    c.number(0.0)
}

fn newton_root(f: &dyn Fn(f64) -> Option<f64>, start: f64) -> Option<f64> {
    let mut x = start;
    for _ in 0..100 {
        let y = f(x)?;
        if !y.is_finite() {
            return None;
        }
        let h = 1e-6 * 1f64.max(x.abs());
        let a = f(x + h);
        let b = f(x - h);
        let (a, b) = (a?, b?);
        let slope = (a - b) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            return None;
        }
        let step = y / slope;
        x -= step;
        if !x.is_finite() {
            return None;
        }
        if step.abs() <= 1e-14 * 1f64.max(x.abs()) {
            break;
        }
    }
    let y = f(x);
    y.filter(|y| y.abs() <= 1e-9 * 1f64.max(x.abs())).map(|_| x)
}

fn near_guess(found: Option<Found>, l: &Side, r: &Side, guess: f64, v: &str) -> Option<Found> {
    let f = |x: f64| {
        let a = l.at(x);
        let b = r.at(x);
        Some(a? - b?)
    };
    let roots: Vec<f64> = match &found {
        Some(fd) if fd.info.outcome == SolveOutcome::Roots => fd.info.roots.clone(),
        _ => Vec::new(),
    };
    let pick = |x: f64| -> Found {
        let i = roots
            .iter()
            .position(|r| (r - x).abs() <= 1e-9 * 1f64.max(x.abs()));
        let fd = found.as_ref();
        Found {
            info: SolveInfo {
                variable: v.to_string(),
                roots: vec![i.map_or(x, |i| roots[i])],
                outcome: SolveOutcome::Roots,
                more: None,
            },
            exact: i.and_then(|i| {
                fd.and_then(|f| f.exact.as_ref())
                    .and_then(|e| e.get(i))
                    .map(|e| vec![e.clone()])
            }),
            closed_form: None,
            loose: i.and_then(|i| {
                fd.and_then(|f| f.loose.as_ref())
                    .and_then(|e| e.get(i))
                    .map(|e| vec![*e])
            }),
        }
    };
    if let Some(n) = newton_root(&f, guess) {
        return Some(pick(n));
    }
    if !roots.is_empty() {
        let mut best = roots[0];
        for &r in &roots[1..] {
            if (r - guess).abs() < (best - guess).abs() {
                best = r;
            }
        }
        return Some(pick(best));
    }
    found
}

/// Full solve; None means not a solve.
pub fn solve_equation(text: &str, ctx: &SolveContext<'_>) -> Option<Solved> {
    let eq = parse_equation(text)?;
    let fn_names = ctx.fn_names();
    if ambiguous_call(&eq.lhs, &fn_names) || ambiguous_call(&eq.rhs, &fn_names) {
        return None;
    }
    let v = pick_unknown(&eq, ctx)?;
    let failed = Rc::new(Cell::new(false));
    let budget = Budget::new(failed.clone());
    let sci = ctx.sci();
    let lhs = normalize_side(&eq.lhs, &fn_names);
    let rhs = normalize_side(&eq.rhs, &fn_names);
    let l = Side::new(&lhs, &v, &sci, &budget)?;
    let r = Side::new(&rhs, &v, &sci, &budget)?;
    let mut solver = Solver {
        l,
        r,
        lhs,
        rhs,
        v: v.clone(),
        ctx: &sci,
        rationalize: ctx.rationalize,
        mode: ctx.angle_mode,
        poly: None,
        failed: failed.clone(),
    };
    let found = solver.run();
    let guess = eq.near.as_deref().and_then(|n| guess_value(n, &sci));
    let out = match guess {
        Some(g) => near_guess(found, &solver.l, &solver.r, g, &v),
        None => found,
    };
    if failed.get() {
        return None;
    }
    let out = out?;
    Some(Solved {
        info: out.info,
        exact: out.exact,
        closed_form: out.closed_form,
        loose: out.loose,
        evals: budget.evals.get(),
    })
}

fn terminates(d: f64) -> bool {
    let mut x = d.abs();
    while x % 2.0 == 0.0 {
        x /= 2.0;
    }
    while x % 5.0 == 0.0 {
        x /= 5.0;
    }
    x == 1.0
}

/// `formatSolve`'s answer.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveDisplay {
    pub display: String,
    pub exact: Option<String>,
    pub message: bool,
}

/// `formatSolve`: values only, the `x =` label is the UI's.
pub fn format_solve_full(s: &Solved, sig_figs: f64, fraction_mode: bool) -> SolveDisplay {
    let info = &s.info;
    let message = |d: &str| SolveDisplay {
        display: d.to_string(),
        exact: None,
        message: true,
    };
    match info.outcome {
        SolveOutcome::All => return message(&format!("true for all {}", info.variable)),
        SolveOutcome::None => return message("no real solution"),
        SolveOutcome::Contradiction => return message("no solution"),
        SolveOutcome::NoneFound => return message("no solution found"),
        SolveOutcome::Roots => {}
    }
    let roots = &info.roots;
    let more_flag = info.more == Some(true);
    let more = if more_flag { ", …" } else { "" };
    let pair = roots.len() == 2
        && !more_flag
        && roots[0] < 0.0
        && (roots[0] + roots[1]).abs() <= 1e-12 * roots[1];
    let mut whole = if roots.len() == 1 || pair {
        sig_figs
    } else {
        sig_figs.min(6.0)
    };
    let distinct = |f: f64| {
        let mut set: Vec<String> = Vec::new();
        for r in roots {
            let t = format_number(*r, f);
            if !set.contains(&t) {
                set.push(t);
            }
        }
        set.len() == roots.len()
    };
    while whole < sig_figs && !distinct(whole) {
        whole += 1.0;
    }
    let figs = |i: usize| {
        if s.loose.as_ref().and_then(|l| l.get(i)).copied() == Some(true) {
            whole.min(6.0)
        } else {
            whole
        }
    };
    let decimal = |r: f64, i: usize| format_number(r, figs(i));
    let shown = |r: f64, i: usize| {
        (if fraction_mode {
            format_as_fraction(r, 10_000)
        } else {
            None
        })
        .unwrap_or_else(|| decimal(r, i))
    };
    let display = if pair {
        format!("±{}", shown(roots[1], 1))
    } else {
        roots
            .iter()
            .enumerate()
            .map(|(i, r)| shown(*r, i))
            .collect::<Vec<_>>()
            .join(", ")
            + more
    };
    let mut exact = s.closed_form.clone();
    if exact.is_none()
        && let Some(ex) = &s.exact
    {
        let parts: Vec<String> = ex
            .iter()
            .enumerate()
            .map(|(i, e)| match e {
                Exact::Text(t) => t.clone(),
                Exact::Frac { n, d } if *d == 1.0 => jsnum::to_string(*n),
                Exact::Frac { n, d } => {
                    if fraction_mode || !terminates(*d) {
                        format!("{}/{}", jsnum::to_string(*n), jsnum::to_string(*d))
                    } else {
                        decimal(roots[i], i)
                    }
                }
            })
            .collect();
        exact = Some(if pair {
            format!("±{}", parts.get(1).cloned().unwrap_or_default())
        } else {
            parts.join(", ") + more
        });
    }
    SolveDisplay {
        exact: exact.filter(|e| *e != display),
        display,
        message: false,
    }
}

/// The evaluate pipeline's form: the display and exact form.
pub fn format_solve(s: &Solved, sig_figs: f64, fraction_mode: bool) -> (String, Option<String>) {
    let d = format_solve_full(s, sig_figs, fraction_mode);
    (d.display, d.exact)
}

// ---- golden replay ----

pub(crate) fn exact_json(e: &Exact) -> Json {
    match e {
        Exact::Frac { n, d } => json!({ "n": n, "d": d }),
        Exact::Text(t) => json!(t),
    }
}

fn solved_json(s: &Solved) -> Json {
    let mut o = serde_json::Map::new();
    o.insert(
        "info".into(),
        serde_json::to_value(&s.info).unwrap_or(Json::Null),
    );
    if let Some(e) = &s.exact {
        o.insert(
            "exact".into(),
            Json::Array(e.iter().map(exact_json).collect()),
        );
    }
    if let Some(c) = &s.closed_form {
        o.insert("closedForm".into(), json!(c));
    }
    if let Some(l) = &s.loose {
        o.insert("loose".into(), json!(l));
    }
    o.insert("evals".into(), json!(s.evals));
    Json::Object(o)
}

fn equation_json(e: &Equation) -> Json {
    let mut o = serde_json::Map::new();
    o.insert("lhs".into(), json!(e.lhs));
    o.insert("rhs".into(), json!(e.rhs));
    if let Some(v) = &e.for_var {
        o.insert("forVar".into(), json!(v));
    }
    if let Some(v) = &e.assign_var {
        o.insert("assignVar".into(), json!(v));
    }
    o.insert("explicit".into(), json!(e.explicit));
    if let Some(v) = &e.near {
        o.insert("near".into(), json!(v));
    }
    Json::Object(o)
}

/// Owned parts of a recorded `ScientificContext & {rationalize}`.
pub(crate) struct CtxArg {
    pub ans: Option<f64>,
    pub angle_mode: AngleMode,
    pub variables: BTreeMap<String, f64>,
    pub functions: BTreeMap<String, UserFunction>,
    pub rationalize: Option<bool>,
}

impl CtxArg {
    pub fn from_json(j: Option<&Json>) -> CtxArg {
        let j = j.map(crate::golden::decode).unwrap_or(Json::Null);
        CtxArg {
            ans: j.get("ans").and_then(Json::as_f64),
            angle_mode: j
                .get("angleMode")
                .and_then(|a| serde_json::from_value(a.clone()).ok())
                .unwrap_or_default(),
            variables: j
                .get("variables")
                .and_then(|a| serde_json::from_value(a.clone()).ok())
                .unwrap_or_default(),
            functions: j
                .get("functions")
                .and_then(|a| serde_json::from_value(a.clone()).ok())
                .unwrap_or_default(),
            rationalize: j.get("rationalize").and_then(Json::as_bool),
        }
    }
    pub fn ctx(&self) -> SolveContext<'_> {
        SolveContext {
            ans: self.ans,
            angle_mode: self.angle_mode,
            variables: &self.variables,
            functions: &self.functions,
            rationalize: self.rationalize,
        }
    }
}

/// WP-SOLVE's golden arms: one per TS module.
pub fn golden_dispatch_module(module: &str, name: &str, args: &[Json]) -> Option<Json> {
    match module {
        "engine/solve" => golden_dispatch(name, args),
        "engine/isolate" => crate::isolate::golden_dispatch(name, args),
        "engine/system" => crate::system::golden_dispatch(name, args),
        _ => None,
    }
}

pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    let s = |i: usize| crate::golden::str_arg(args, i).unwrap_or_default();
    Some(match name {
        "solveCall" => json!(solve_call(&s(0))),
        "splitNear" => {
            let (text, near) = split_near(&s(0));
            match near {
                Some(n) => json!({ "text": text, "near": n }),
                None => json!({ "text": text }),
            }
        }
        "parseEquation" => parse_equation(&s(0)).map_or(Json::Null, |e| equation_json(&e)),
        "isEquation" => json!(is_equation(&s(0))),
        "pickUnknown" => {
            let e = args
                .first()
                .map(crate::golden::decode)
                .unwrap_or(Json::Null);
            let st = |k: &str| e.get(k).and_then(Json::as_str).map(str::to_string);
            let eq = Equation {
                lhs: st("lhs").unwrap_or_default(),
                rhs: st("rhs").unwrap_or_default(),
                for_var: st("forVar"),
                assign_var: st("assignVar"),
                explicit: e.get("explicit").and_then(Json::as_bool).unwrap_or(false),
                near: st("near"),
            };
            let c = CtxArg::from_json(args.get(1));
            pick_unknown(&eq, &c.ctx()).map_or(Json::Null, Json::String)
        }
        "solveEquation" => {
            let c = CtxArg::from_json(args.get(1));
            solve_equation(&s(0), &c.ctx()).map_or(Json::Null, |x| solved_json(&x))
        }
        _ => return None,
    })
}
