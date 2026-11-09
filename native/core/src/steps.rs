//! Native only: the working for a line, so an answer can be checked by hand (the Mac's "Working" panel). Each step
//! is its maths, in Typst math (`app::typst_math`) and as plain text for copying, and why, in a short phrase.
//!
//! - derivatives (`d/dx …`): the sum, constant multiple, power, product, quotient and chain rules, innermost first
//! - indefinite integrals from the table (`calculus::antiderivative`): the form, by parts, or the substitution
//! - one equation in one unknown: a linear one by isolating it, a quadratic by the formula
//! - unit conversions: the factor through SI
//! - differential equations (`calculus::ode`): the method, and the constants the conditions settle
//!
//! Anything else has no working. The last step is always the answer the bar shows, checked against it (by its text,
//! or by its value), and when it doesn't match there is no working at all rather than working to another answer.

use serde::Serialize;

use crate::app::typst_math::{preview_math, to_typst_math};
use crate::calculus::pretty::pretty;
use crate::calculus::tree::{self, N, T, call, constant, op, symbol};
use crate::calculus::{self as calc, antiderivative, ode};
use crate::evaluate::evaluate_line;
use crate::format::{DEFAULT_SIG_FIGS, format_number};
use crate::jsre::{self, Lazy};
use crate::scientific::ScientificContext;
use crate::symbolic::Op;
use crate::types::{EvaluateOptions, LineResult, SolveOutcome};

/// One step of the working.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Step {
    /// Typst math, without the `$`s
    pub math: String,
    /// why, in a short phrase
    pub why: String,
    /// the same maths as plain text, for copying
    pub plain: String,
}

/// More steps than this is more than a panel under the bar should hold: no working.
const MAX_STEPS: usize = 14;

/// The working for `text` with the bar's options, or None when there is none for it.
pub fn steps(text: &str, options: &EvaluateOptions) -> Option<Vec<Step>> {
    let text = text.trim();
    if text.is_empty() || text.contains('\n') {
        return None;
    }
    let shown = evaluate_line(text, options);
    if shown.display.trim().is_empty() || shown.error.is_some() {
        return None;
    }
    let sig = options.sig_figs.unwrap_or(f64::from(DEFAULT_SIG_FIGS));
    let sci = ScientificContext {
        ans: options.ans,
        angle_mode: options.angle(),
        variables: options.variables.clone(),
        functions: options.functions.clone(),
        depth: 0,
    };
    let found = differential_equation(text, &sci, sig, &shown)
        .or_else(|| derivative(text, &sci, sig, &shown))
        .or_else(|| integral(text, &sci, &shown))
        .or_else(|| equation(text, &sci, sig, &shown))
        .or_else(|| conversion(text, &shown))?;
    (!found.is_empty() && found.len() <= MAX_STEPS).then_some(found)
}

/// The document a step's maths compiles to: the typeset preview's (its macros, black glyphs the renderer swaps
/// for the text colour), at the size the panel draws.
pub fn step_document(math: &str) -> String {
    let math = jsre::replace_all(lazy_root(), math, |_| "sqrt(#box(height: 0.95em);".into());
    format!(
        "#import \"/macros.typ\": *\n#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(size: 15pt, fill: black)\n$ {math} $\n"
    )
}

fn lazy_root() -> &'static jsre::Regex {
    static ROOT: Lazy = Lazy::new(|| jsre::js(r"(?<![A-Za-z0-9_\x22])sqrt\(", ""));
    &ROOT
}

// ---- writing steps

/// `x²` as `x^2`: Typst sets a superscript character apart from its base, where a power sits on it.
fn powers(plain: &str) -> String {
    const SUP: [(char, char); 12] = [
        ('⁰', '0'),
        ('¹', '1'),
        ('²', '2'),
        ('³', '3'),
        ('⁴', '4'),
        ('⁵', '5'),
        ('⁶', '6'),
        ('⁷', '7'),
        ('⁸', '8'),
        ('⁹', '9'),
        ('⁻', '-'),
        ('ⁿ', 'n'),
    ];
    let digit = |c: char| SUP.iter().find(|(s, _)| *s == c).map(|(_, d)| *d);
    let mut out = String::new();
    let mut run = String::new();
    for c in plain.chars() {
        match digit(c) {
            Some(d) => run.push(d),
            None => {
                if !run.is_empty() {
                    out.push_str(&format!("^({run})"));
                    run.clear();
                }
                out.push(c);
            }
        }
    }
    if !run.is_empty() {
        out.push_str(&format!("^({run})"));
    }
    out
}

fn typst(plain: &str) -> String {
    to_typst_math(&powers(plain))
        .replace("op(\"∫\")", "integral")
        .replace("\"∫\"", "integral")
}

fn step(plain: impl Into<String>, why: impl Into<String>) -> Step {
    let plain = plain.into();
    Step {
        math: typst(&plain),
        why: why.into(),
        plain,
    }
}

/// Whether `s` needs brackets to be one factor: a sum, a product written with spaces, or a quotient.
fn compound(s: &str) -> bool {
    let mut depth = 0_i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '|' if depth >= 0 => depth += 1,
            ')' | ']' => depth -= 1,
            '+' | '−' | ' ' | '/' if depth == 0 => return true,
            '-' if depth == 0 && i > 0 => return true,
            _ => {}
        }
    }
    false
}

fn group(s: &str) -> String {
    if compound(s) {
        format!("({s})")
    } else {
        s.to_string()
    }
}

/// Signed pieces as one sum: `a + b − c`.
fn sum(pieces: &[(bool, String)]) -> String {
    let mut out = String::new();
    for (negative, p) in pieces {
        if out.is_empty() {
            out = if *negative {
                format!("-{}", group(p))
            } else {
                p.clone()
            };
        } else {
            out.push_str(if *negative { " - " } else { " + " });
            // `a + -b` reads as `a + (-b)`
            out.push_str(&if *negative || p.starts_with('-') {
                group(p)
            } else {
                p.clone()
            });
        }
    }
    if out.is_empty() { "0".into() } else { out }
}

fn same_text(a: &str, b: &str) -> bool {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    squash(a) == squash(b)
}

// ---- derivatives

struct Differentiate<'a> {
    v: &'a str,
    deg: bool,
    steps: Vec<Step>,
}

fn free_of(t: &T, v: &str) -> bool {
    match &**t {
        N::Sym(s) => s != v,
        N::Const(_) => true,
        N::Op(_, a) | N::Call(_, a) => a.iter().all(|x| free_of(x, v)),
    }
}

/// The argument of a trig call in degrees without its `deg__`: `sin(x deg__)` is about x.
fn degree_argument(t: &T) -> Option<T> {
    match &**t {
        N::Op(Op::Multiply, a) if a.len() == 2 && a[1].sym_name() == Some(calc::DEG_SYMBOL) => {
            Some(a[0].clone())
        }
        _ => None,
    }
}

impl Differentiate<'_> {
    /// `t` as typed. An inverse trig call answering in degrees is `asin(x)/deg__` in the tree, which reads back
    /// as `asin(x)` here (`as_typed` would write it as `asin(x) (π/180)/(π/180)`).
    fn shown(&self, t: &T) -> Option<String> {
        fn bare(t: &T) -> T {
            match &**t {
                N::Op(Op::Divide, a)
                    if a.len() == 2
                        && a[1].sym_name() == Some(calc::DEG_SYMBOL)
                        && a[0].call_name().is_some() =>
                {
                    bare(&a[0])
                }
                N::Op(o, a) => op(*o, a.iter().map(bare).collect()),
                N::Call(name, a) => call(name, a.iter().map(bare).collect()),
                _ => t.clone(),
            }
        }
        Some(pretty(&calc::as_typed(&bare(t), false)?).s)
    }

    fn derivative(&self, t: &T) -> Option<T> {
        calc::derivative_shown(t, self.v, self.deg)
    }

    fn dd(&self, body: &str) -> String {
        format!("d/d{} {}", self.v, group(body))
    }

    /// The derivative of `t` as the answer shows it, after a step for it (and for each part it needs, first).
    fn walk(&mut self, t: &T) -> Option<String> {
        if self.steps.len() > MAX_STEPS {
            return None;
        }
        if free_of(t, self.v) {
            return Some("0".into());
        }
        if t.sym_name() == Some(self.v) {
            return Some("1".into());
        }
        let result = pretty(&self.derivative(t)?).s;
        // `3x` needs no step of its own
        if let N::Op(Op::Multiply, a) = &**t
            && a.iter().filter(|x| !free_of(x, self.v)).count() == 1
            && a.iter().any(|x| x.sym_name() == Some(self.v))
        {
            return Some(result);
        }
        let (rule, why) = self.rule(t)?;
        let lhs = self.dd(&self.shown(t)?);
        let plain = match rule {
            Some(r) if !same_text(&r, &result) => format!("{lhs} = {r} = {result}"),
            _ => format!("{lhs} = {result}"),
        };
        self.steps.push(Step {
            math: derivative_typst(&plain, self.v),
            why,
            plain,
        });
        Some(result)
    }

    /// The rule for `t`'s outermost operation: the derivative written with its parts' derivatives (None when that
    /// says no more than the result), and why.
    fn rule(&mut self, t: &T) -> Option<(Option<String>, String)> {
        let v = self.v.to_string();
        let a = t.args();
        Some(match &**t {
            N::Op(Op::Add | Op::Subtract, _) => {
                let mut pieces = Vec::new();
                for (i, x) in a.iter().enumerate() {
                    let d = self.walk(x)?;
                    if d != "0" {
                        pieces.push((i > 0 && t.op_is(Op::Subtract), d));
                    }
                }
                let why = if t.op_is(Op::Subtract) {
                    "difference rule: (f − g)' = f' − g'"
                } else {
                    "sum rule: (f + g)' = f' + g'"
                };
                (Some(sum(&pieces)), why.into())
            }
            N::Op(Op::UnaryMinus, _) => {
                let d = self.walk(&a[0])?;
                (
                    Some(format!("-{}", group(&d))),
                    "constant multiple rule: (−f)' = −f'".into(),
                )
            }
            N::Op(Op::Multiply, _) => {
                let (constants, varying): (Vec<T>, Vec<T>) =
                    a.iter().cloned().partition(|x| free_of(x, &v));
                if !constants.is_empty() {
                    let c: Vec<String> = constants
                        .iter()
                        .map(|x| self.shown(x).map(|s| group(&s)))
                        .collect::<Option<_>>()?;
                    let rest = if varying.len() == 1 {
                        varying[0].clone()
                    } else {
                        op(Op::Multiply, varying)
                    };
                    let d = self.walk(&rest)?;
                    let c = c.join(" ");
                    return Some((
                        Some(format!("{c} · {}", group(&d))),
                        format!("constant multiple rule: (c f)' = c f', c = {c}"),
                    ));
                }
                let mut pieces = Vec::new();
                let shown: Vec<String> = a.iter().map(|x| self.shown(x)).collect::<Option<_>>()?;
                let ds: Vec<String> = a.iter().map(|x| self.walk(x)).collect::<Option<_>>()?;
                for i in 0..a.len() {
                    if ds[i] == "0" {
                        continue;
                    }
                    let mut f: Vec<String> = Vec::new();
                    for j in 0..a.len() {
                        f.push(group(if i == j { &ds[j] } else { &shown[j] }));
                    }
                    pieces.push((false, f.join(" · ")));
                }
                let why = if a.len() == 2 {
                    format!(
                        "product rule: (f g)' = f' g + f g', f = {}, g = {}",
                        shown[0], shown[1]
                    )
                } else {
                    "product rule, one factor at a time".into()
                };
                (Some(sum(&pieces)), why)
            }
            N::Op(Op::Divide, _) => {
                let (top, bottom) = (&a[0], &a[1]);
                if bottom.sym_name() == Some(calc::DEG_SYMBOL)
                    && let N::Call(name, inner) = &**top
                {
                    return self.function(name, &inner[0], true);
                }
                if free_of(bottom, &v) {
                    let d = self.walk(top)?;
                    let c = group(&self.shown(bottom)?);
                    return Some((
                        Some(format!("{}/{c}", group(&d))),
                        format!("constant multiple rule: (f/c)' = f'/c, c = {c}"),
                    ));
                }
                let (f, g) = (self.shown(top)?, self.shown(bottom)?);
                let df = self.walk(top)?;
                let dg = self.walk(bottom)?;
                let numerator = sum(&[
                    (false, format!("{} · {}", group(&df), group(&g))),
                    (true, format!("{} · {}", group(&f), group(&dg))),
                ]);
                (
                    Some(format!("({numerator})/{}^2", group(&g))),
                    format!("quotient rule: (f/g)' = (f' g − f g')/g², f = {f}, g = {g}"),
                )
            }
            N::Op(Op::Pow, _) => {
                let (base, ex) = (&a[0], &a[1]);
                if free_of(ex, &v) {
                    let n = group(&self.shown(ex)?);
                    if base.sym_name() == Some(self.v) {
                        return Some((
                            None,
                            format!("power rule: d/d{v} {v}ⁿ = n {v}ⁿ⁻¹, n = {n}"),
                        ));
                    }
                    let u = self.shown(base)?;
                    let du = self.walk(base)?;
                    let lowered = op(Op::Subtract, vec![ex.clone(), constant(1.0)]);
                    let m = simplified_number(&lowered).unwrap_or_else(|| format!("{n} - 1"));
                    return Some((
                        Some(format!("{n} {}^{} · {}", group(&u), group(&m), group(&du))),
                        format!("power rule and chain rule: (uⁿ)' = n uⁿ⁻¹ u', u = {u}"),
                    ));
                }
                if free_of(base, &v) {
                    let b = self.shown(base)?;
                    let e_base = base.sym_name() == Some("e");
                    let whole = self.shown(t)?;
                    if ex.sym_name() == Some(self.v) {
                        let why = if e_base {
                            format!("d/d{v} e^{v} = e^{v}")
                        } else {
                            format!("d/d{v} aˣ = aˣ ln a, a = {b}")
                        };
                        return Some((None, why));
                    }
                    let u = self.shown(ex)?;
                    let du = self.walk(ex)?;
                    let outer = if e_base {
                        whole
                    } else {
                        format!("{whole} ln({b})")
                    };
                    return Some((
                        Some(format!("{} · {}", group(&outer), group(&du))),
                        format!("chain rule: (e^u)' = e^u u', u = {u}"),
                    ));
                }
                (None, "write as e^(g ln f) and use the chain rule".into())
            }
            N::Call(name, args) if args.len() == 1 => {
                let (inner, deg) = match degree_argument(&args[0]) {
                    Some(x) if self.deg => (x, true),
                    _ => (args[0].clone(), false),
                };
                return self.function(name, &inner, deg);
            }
            _ => return None,
        })
    }

    /// A named function of `inner`: its own derivative, and the chain rule when `inner` isn't the variable.
    /// `deg`: a trig call (or an inverse trig answer) in degrees.
    fn function(&mut self, name: &str, inner: &T, deg: bool) -> Option<(Option<String>, String)> {
        let shown_name = match name {
            "log" => "ln",
            other => other,
        };
        let unit = if deg { ", in degrees (× π/180)" } else { "" };
        let unit = if deg && name.starts_with('a') {
            ", in degrees (× 180/π)"
        } else {
            unit
        };
        if inner.sym_name() == Some(self.v) {
            return Some((None, format!("derivative of {shown_name}{unit}")));
        }
        // f'(u) written with u in it
        let u = tree::symbol(PLACE);
        let at_u: T = if deg && !name.starts_with('a') {
            call(
                name,
                vec![op(Op::Multiply, vec![u.clone(), symbol(calc::DEG_SYMBOL)])],
            )
        } else if deg {
            op(
                Op::Divide,
                vec![call(name, vec![u.clone()]), symbol(calc::DEG_SYMBOL)],
            )
        } else {
            call(name, vec![u.clone()])
        };
        let outer = calc::derivative_shown(&at_u, PLACE, self.deg)?;
        let typed_inner = calc::as_typed(inner, self.deg)?;
        let outer = pretty(&calc::substituted(
            &outer,
            &[(PLACE.to_string(), typed_inner)],
        ))
        .s;
        let shown_inner = self.shown(inner)?;
        let du = self.walk(inner)?;
        Some((
            Some(format!("{} · {}", group(&outer), group(&du))),
            format!("chain rule: f(u)' = f'(u) u', f = {shown_name}{unit}, u = {shown_inner}"),
        ))
    }
}

/// The letter that stands for a function's inside while its derivative is written.
const PLACE: &str = "u";

/// `n - 1` as a number when it is one.
fn simplified_number(t: &T) -> Option<String> {
    let v = tree::evaluate(t, &[])??;
    v.is_finite().then(|| pretty(&constant(v)).s)
}

/// A derivative step's Typst: each `d/dx` as the fraction the preview draws.
fn derivative_typst(plain: &str, v: &str) -> String {
    let marker = format!("d/d{v} ");
    plain
        .split(" = ")
        .map(|side| match side.strip_prefix(&marker) {
            Some(body) => format!("frac(dif, dif {v}) {}", typst(body)),
            None => typst(side),
        })
        .collect::<Vec<_>>()
        .join(" = ")
}

fn derivative(
    text: &str,
    sci: &ScientificContext,
    sig: f64,
    shown: &LineResult,
) -> Option<Vec<Step>> {
    let line = calc::derivative_line(text, sci)?;
    let mut d = Differentiate {
        v: &line.v,
        deg: line.deg,
        steps: Vec::new(),
    };
    let result = d.walk(&line.tree)?;
    if d.steps.is_empty() {
        return None;
    }
    let mut steps = d.steps;
    match line.at {
        None => {
            // the last step's result is the line's answer
            if result != shown.display {
                return None;
            }
        }
        Some(a) => {
            let f = calc::shown_fn(
                &calc::derivative_shown(&line.tree, &line.v, line.deg)?,
                &line.v,
                sci,
            )?;
            let value = f(a);
            if !value.is_finite() {
                return None;
            }
            let answer = format_number(value, sig);
            if answer != shown.display {
                return None;
            }
            let at = format_number(a, sig);
            let with = jsre::replace_all(
                &jsre::js(&format!(r"(?<![A-Za-z]){}(?![A-Za-z])", line.v), ""),
                &result,
                |_| format!("({at})"),
            )
            .into_owned();
            let plain = if same_text(&with, &answer) {
                with
            } else {
                format!("{with} = {answer}")
            };
            steps.push(step(plain, format!("put in {} = {at}", line.v)));
        }
    }
    Some(steps)
}

// ---- integrals

fn integral(text: &str, sci: &ScientificContext, shown: &LineResult) -> Option<Vec<Step>> {
    let (tree, v, _) = calc::integral_line(text, sci)?;
    let deg = sci.angle_mode == crate::types::AngleMode::Deg;
    let (whole, notes) = antiderivative::antiderivative_noted(&tree, &v, deg)?;
    let answer = format!("{} + C", calc::bare_powers(&whole.show));
    if answer != shown.display {
        return None;
    }
    let int = |body: &str| format!("∫ {} d{v}", group(body));
    let shown_of = |t: &T| pretty(t).s;
    let body = shown_of(&tree);
    // the sum's terms, each integrated on its own
    let mut terms: Vec<(bool, T)> = Vec::new();
    fn split(t: &T, negative: bool, out: &mut Vec<(bool, T)>) {
        match &**t {
            N::Op(Op::Add, a) => a.iter().for_each(|x| split(x, negative, out)),
            N::Op(Op::Subtract, a) if a.len() == 2 => {
                split(&a[0], negative, out);
                split(&a[1], !negative, out);
            }
            _ => out.push((negative, t.clone())),
        }
    }
    split(&tree, false, &mut terms);
    let mut steps = Vec::new();
    if terms.len() > 1 {
        let mut split_up = String::new();
        for (i, (neg, t)) in terms.iter().enumerate() {
            split_up.push_str(match (i, neg) {
                (0, false) => "",
                (0, true) => "-",
                (_, false) => " + ",
                (_, true) => " - ",
            });
            split_up.push_str(&int(&shown_of(t)));
        }
        steps.push(step(
            format!("{} = {split_up}", int(&body)),
            "sum rule: integrate term by term",
        ));
        let mut evals = Vec::new();
        for (neg, t) in &terms {
            let (found, notes) = antiderivative::antiderivative_noted(t, &v, deg)?;
            let why = ode::headline(&notes).unwrap_or_else(|| "from the table".into());
            steps.push(step(
                format!("{} = {}", int(&shown_of(t)), calc::bare_powers(&found.show)),
                why,
            ));
            evals.push((*neg, found.eval));
        }
        // the pieces add up to the answer, up to a constant
        let joined: Vec<String> = evals
            .iter()
            .map(|(neg, e)| format!("{}({e})", if *neg { "-" } else { "+" }))
            .collect();
        let pieces = calc::text_fn(&joined.concat(), sci, &v)?;
        let all = calc::text_fn(&whole.eval, sci, &v)?;
        let mut gap: Option<f64> = None;
        let mut agreed = 0;
        for x in [0.37, 0.61, 1.13, 1.71, 2.39] {
            let d = pieces(x) - all(x);
            if !d.is_finite() {
                continue;
            }
            match gap {
                None => gap = Some(d),
                Some(g) if (d - g).abs() <= 1e-7 * (1.0 + g.abs()) => {}
                Some(_) => return None,
            }
            agreed += 1;
        }
        if agreed < 3 {
            return None;
        }
        steps.push(step(
            format!("{} = {answer}", int(&body)),
            "add the pieces and the constant of integration C",
        ));
    } else {
        let why = ode::headline(&notes).unwrap_or_else(|| "from the table".into());
        steps.push(step(
            format!("{} = {}", int(&body), calc::bare_powers(&whole.show)),
            why,
        ));
        steps.push(step(
            format!("{} = {answer}", int(&body)),
            "add the constant of integration C",
        ));
    }
    Some(steps)
}

// ---- equations in one unknown

/// `a`, `b`, `c` of `a v^2 + b v + c` when `t` is that polynomial wherever it's defined.
fn quadratic(t: &T, v: &str) -> Option<[f64; 3]> {
    let at = |x: f64| {
        tree::evaluate(t, &[(v.to_string(), x)])
            .flatten()
            .filter(|y| y.is_finite())
    };
    let (f0, f1, fm) = (at(0.0)?, at(1.0)?, at(-1.0)?);
    let c = f0;
    let b = (f1 - fm) / 2.0;
    let a = (f1 + fm) / 2.0 - c;
    for x in [2.0, -3.0, 0.5, 7.0, -11.0] {
        let want = a * x * x + b * x + c;
        let got = at(x)?;
        if (got - want).abs() > 1e-9 * (a.abs() * x * x + b.abs() * x.abs() + c.abs()).max(1.0) {
            return None;
        }
    }
    let tidy = |n: f64| {
        let r = n.round();
        if (n - r).abs() < 1e-12 * n.abs().max(1.0) {
            r
        } else {
            n
        }
    };
    Some([tidy(a), tidy(b), tidy(c)])
}

/// A number as the working writes it: a short fraction when it is one, else the bar's sig figs.
fn num(v: f64, sig: f64) -> String {
    for q in 1..=64_i64 {
        let p = v * q as f64;
        if (p - p.round()).abs() < 1e-9 * p.abs().max(1.0) && p.abs() < 1e9 {
            let p = p.round() as i64;
            return if q == 1 {
                p.to_string()
            } else {
                format!("{p}/{q}")
            };
        }
    }
    format_number(v, sig)
}

/// `a v + b` written out.
fn linear_text(a: f64, b: f64, v: &str, sig: f64) -> String {
    let mut pieces = Vec::new();
    if a != 0.0 {
        let k = num(a.abs(), sig);
        let k = if k == "1" {
            String::new()
        } else if k.contains('/') {
            format!("({k})")
        } else {
            k
        };
        pieces.push((a < 0.0, format!("{k}{v}")));
    }
    if b != 0.0 || pieces.is_empty() {
        pieces.push((
            b < 0.0 && !pieces.is_empty(),
            num(if pieces.is_empty() { b } else { b.abs() }, sig),
        ));
    }
    sum(&pieces)
}

fn quadratic_text(q: [f64; 3], v: &str, sig: f64) -> String {
    let [a, b, c] = q;
    let mut pieces = Vec::new();
    let k = num(a.abs(), sig);
    pieces.push((
        a < 0.0,
        format!("{}{v}^2", if k == "1" { String::new() } else { k }),
    ));
    if b != 0.0 {
        let k = num(b.abs(), sig);
        pieces.push((
            b < 0.0,
            format!("{}{v}", if k == "1" { String::new() } else { k }),
        ));
    }
    if c != 0.0 {
        pieces.push((c < 0.0, num(c.abs(), sig)));
    }
    sum(&pieces)
}

fn equation(
    text: &str,
    sci: &ScientificContext,
    sig: f64,
    shown: &LineResult,
) -> Option<Vec<Step>> {
    let solve = shown.solve.as_ref()?;
    let v = solve.variable.as_str();
    static SOLVE: Lazy = Lazy::new(|| jsre::js(r"^\s*solve\s+", "i"));
    let body = jsre::replace_all(&SOLVE, text, |_| String::new()).into_owned();
    let sides: Vec<&str> = body.split('=').collect();
    let [left, right] = sides.as_slice() else {
        return None;
    };
    if left.contains(['<', '>', '!']) || right.trim().is_empty() {
        return None;
    }
    let l = calc::tree_in(left, sci, v)?;
    let r = calc::tree_in(right, sci, v)?;
    let lq = quadratic(&l, v)?;
    let rq = quadratic(&r, v)?;
    let q = [lq[0] - rq[0], lq[1] - rq[1], lq[2] - rq[2]];
    let typed = format!("{} = {}", left.trim(), right.trim());
    let mut steps = vec![step(typed.clone(), "the equation")];
    let answer_text = |root_text: String| format!("{v} = {root_text}");
    let shown_answer = match &shown.exact {
        Some(exact) if !exact.trim().is_empty() && exact != &shown.display => {
            format!("{exact} ≈ {}", shown.display)
        }
        _ => shown.display.clone(),
    };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    if q[0] == 0.0 {
        // linear: a v + b = 0, isolated
        if q[1] == 0.0 || solve.roots.len() != 1 {
            return None;
        }
        let root = -q[2] / q[1];
        if !close(root, solve.roots[0]) {
            return None;
        }
        let (a1, b1, a2, b2) = (lq[1], lq[2], rq[1], rq[2]);
        let expanded = format!(
            "{} = {}",
            linear_text(a1, b1, v, sig),
            linear_text(a2, b2, v, sig)
        );
        if !same_text(&expanded, &typed) {
            steps.push(step(expanded, "expand both sides"));
        }
        let mut moves = Vec::new();
        if a2 != 0.0 {
            let k = linear_text(a2.abs(), 0.0, v, sig);
            moves.push(if a2 > 0.0 {
                format!("subtract {k}")
            } else {
                format!("add {k}")
            });
        }
        if b1 != 0.0 {
            let k = num(b1.abs(), sig);
            moves.push(if b1 > 0.0 {
                format!("subtract {k}")
            } else {
                format!("add {k}")
            });
        }
        let a = a1 - a2;
        let b = b2 - b1;
        if !moves.is_empty() {
            let why = if moves.len() == 1 {
                let side = if moves[0].starts_with("add") {
                    "to"
                } else {
                    "from"
                };
                format!("{} {side} both sides", moves[0])
            } else {
                format!("{} on both sides", moves.join(" and "))
            };
            steps.push(step(
                format!("{} = {}", linear_text(a, 0.0, v, sig), num(b, sig)),
                why,
            ));
        }
        if a != 1.0 {
            steps.push(step(
                format!("{v} = {}/{}", group(&num(b, sig)), group(&num(a, sig))),
                format!("divide both sides by {}", num(a, sig)),
            ));
        }
        steps.push(step(answer_text(shown_answer), "the answer"));
        return Some(steps);
    }
    // quadratic: the formula
    let [a, b, c] = q;
    let disc = b * b - 4.0 * a * c;
    let standard = format!("{} = 0", quadratic_text(q, v, sig));
    // a negative number in brackets, where it's multiplied or squared
    let n = |x: f64| {
        let t = num(x, sig);
        if t.starts_with('-') || t.contains('/') {
            format!("({t})")
        } else {
            t
        }
    };
    if b == 0.0 {
        // `a v^2 + c = 0`: the square root of -c/a
        let k = -c / a;
        let mut moves = Vec::new();
        if c != 0.0 {
            moves.push(format!(
                "{}^2 = {}",
                linear_text(a, 0.0, v, sig),
                num(-c, sig)
            ));
        }
        if a != 1.0 {
            moves.push(format!("{v}^2 = {}", num(k, sig)));
        }
        for m in moves {
            if !same_text(&m, &typed) {
                steps.push(step(m, "isolate the square"));
            }
        }
        if k < 0.0 {
            if solve.outcome != SolveOutcome::None || !solve.roots.is_empty() {
                return None;
            }
            steps.push(step(
                format!("{v}^2 = {} < 0", num(k, sig)),
                "a square is never negative: no real root",
            ));
            steps.push(step(shown.display.clone(), "the answer"));
            return Some(steps);
        }
        let root = k.sqrt();
        let roots = if k == 0.0 {
            vec![0.0]
        } else {
            vec![-root, root]
        };
        if roots.len() != solve.roots.len()
            || !roots.iter().zip(&solve.roots).all(|(x, y)| close(*x, *y))
        {
            return None;
        }
        steps.push(step(
            format!("{v} = ±√({})", num(k, sig)),
            "take the square root of both sides",
        ));
        steps.push(step(answer_text(shown_answer), "the answer"));
        return Some(steps);
    }
    if !same_text(&standard, &typed) {
        steps.push(step(standard, "move everything to one side"));
    }
    steps.push(step(
        format!(
            "{v} = ({} ± √({}^2 - 4 · {} · {}))/(2 · {})",
            num(-b, sig),
            n(b),
            n(a),
            n(c),
            n(a)
        ),
        format!(
            "quadratic formula, a = {}, b = {}, c = {}",
            num(a, sig),
            num(b, sig),
            num(c, sig)
        ),
    ));
    let d = num(disc, sig);
    if disc < 0.0 {
        if solve.outcome != SolveOutcome::None || !solve.roots.is_empty() {
            return None;
        }
        steps.push(step(
            format!("b^2 - 4ac = {d}"),
            "the discriminant is negative: no real root",
        ));
        steps.push(step(shown.display.clone(), "the answer"));
        return Some(steps);
    }
    let mut roots = if disc == 0.0 {
        vec![-b / (2.0 * a)]
    } else {
        vec![
            (-b - disc.sqrt()) / (2.0 * a),
            (-b + disc.sqrt()) / (2.0 * a),
        ]
    };
    roots.sort_by(f64::total_cmp);
    if roots.len() != solve.roots.len()
        || !roots.iter().zip(&solve.roots).all(|(x, y)| close(*x, *y))
    {
        return None;
    }
    steps.push(step(
        format!("{v} = ({} ± √{})/{}", num(-b, sig), n(disc), n(2.0 * a)),
        format!("the discriminant b² − 4ac = {d}"),
    ));
    steps.push(step(answer_text(shown_answer), "the answer"));
    Some(steps)
}

// ---- unit conversions

fn conversion(text: &str, shown: &LineResult) -> Option<Vec<Step>> {
    static LINE: Lazy = Lazy::new(|| {
        jsre::js(
            r"^\s*([+\-]?(?:\d[\d,]*\.?\d*|\.\d+)(?:e[+\-]?\d+)?)\s*([^\d\s].*?)\s+(?:to|in|into|as)\s+(.+?)\s*$",
            "i",
        )
    });
    shown.quantity.as_ref()?;
    let c = LINE.captures(text).ok()??;
    let (amount, from, to) = (
        jsre::group(&c, 1).replace(',', ""),
        jsre::group(&c, 2).trim().to_string(),
        jsre::group(&c, 3).trim().to_string(),
    );
    let amount: f64 = amount.parse().ok()?;
    let one_from = crate::units::si_quantity(&format!("1 {from}"))?;
    let one_to = crate::units::si_quantity(&format!("1 {to}"))?;
    if one_from.dim != one_to.dim || one_from.si == 0.0 || one_to.si == 0.0 {
        return None;
    }
    let answer = crate::units::si_quantity(&shown.display)?;
    let value = amount * one_from.si / one_to.si;
    if answer.dim != one_to.dim
        || (answer.si - value * one_to.si).abs() > 1e-6 * answer.si.abs().max(1e-300)
    {
        return None;
    }
    // the SI unit, as the engine writes it
    let si_unit = crate::units::si_value(1.0, &one_to.dim, None, &Default::default())
        .map(|v| crate::format::format_value(&v, 12.0))
        .and_then(|t| t.strip_prefix("1 ").map(str::to_string))?;
    let f = |x: f64| format_number(x, 6.0);
    let a = format_number(amount, 12.0);
    let mut steps = Vec::new();
    let through_si = si_unit != to;
    let from_si = si_unit == from;
    let unit_step = |plain: String, why: String| Step {
        math: preview_math(&plain),
        why,
        plain,
    };
    if from_si {
        steps.push(unit_step(
            format!("{a} {from} = {a} ÷ {} {to}", f(one_to.si)),
            format!("1 {to} = {} {si_unit}", f(one_to.si)),
        ));
    } else if !through_si {
        steps.push(unit_step(
            format!("{a} {from} = {a} × {} {to}", f(one_from.si)),
            format!("1 {from} = {} {to}", f(one_from.si)),
        ));
    } else {
        let si = amount * one_from.si;
        steps.push(unit_step(
            format!(
                "{a} {from} = {a} × {} {si_unit} = {} {si_unit}",
                f(one_from.si),
                f(si)
            ),
            format!("1 {from} = {} {si_unit}", f(one_from.si)),
        ));
        steps.push(unit_step(
            format!("{} {si_unit} = {} ÷ {} {to}", f(si), f(si), f(one_to.si)),
            format!("1 {to} = {} {si_unit}", f(one_to.si)),
        ));
    }
    steps.push(unit_step(
        format!("{a} {from} = {}", shown.display),
        "the answer".into(),
    ));
    Some(steps)
}

// ---- differential equations

fn differential_equation(
    text: &str,
    sci: &ScientificContext,
    sig: f64,
    shown: &LineResult,
) -> Option<Vec<Step>> {
    let work = ode::ode_steps(text, sci, sig)?;
    if work.last().map(|(m, _)| m.as_str()) != Some(shown.display.as_str()) {
        return None;
    }
    Some(
        work.into_iter()
            .map(|(plain, why)| step(plain, why))
            .collect(),
    )
}
