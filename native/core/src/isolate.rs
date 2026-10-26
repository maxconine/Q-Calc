//! WP-SOLVE: `src/engine/isolate.ts`, rearranging a formula for one letter symbolically: undo each operation
//! around a letter that appears once, or collect a polynomial of degree ≤ 2, then check each answer numerically.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::jsre::{self, Lazy, js};
use crate::plain_math::normalize_math_text;
use crate::scientific::{NAMES, preprocess_ascii};
use crate::solve::{solve_call, split_near};
use crate::symbolic::{self, Node, Op, PrintOptions};
use crate::system_numeric::{Pv, eval_plain, no_overrides};

/// `isolate x in a*x + b = c`; a line with no `=` is set to 0.
#[derive(Clone, Debug, PartialEq)]
pub struct IsolateCmd {
    pub variable: String,
    pub lhs: String,
    pub rhs: String,
}

const VAR: &str = r"([A-Za-z][A-Za-z0-9]*(?:_[A-Za-z0-9]+)?|θ)";

fn subscripts(text: &str) -> String {
    static RE: Lazy = Lazy::new(|| {
        js(
            r#"([A-Za-z][A-Za-z0-9]*)_\s*[({]\s*"?([A-Za-z0-9]+)"?\s*[)}]"#,
            "",
        )
    });
    jsre::replace_all(&RE, text, |c| {
        format!("{}_{}", jsre::group(c, 1), jsre::group(c, 2))
    })
    .into_owned()
}

fn re(pattern: &str, flags: &str) -> jsre::Regex {
    js(&pattern.replace("VAR", VAR), flags)
}

static LEAD: Lazy = Lazy::new(|| re(r"(?s)^isolate\s+VAR(?:\s+in\s+|\s*[:,;]\s*|\s+)(.+)$", "i"));
static TRAIL: Lazy = Lazy::new(|| re(r"(?s)^(.+?)\s*[,;]?\s+isolate\s+VAR$", "i"));
static SOLVE_LEAD: Lazy = Lazy::new(|| {
    re(
        r"(?s)^solve\s+(?:for\s+)?VAR(?:\s+in\s+|\s*[:,;]\s*)(.+)$",
        "i",
    )
});
static SOLVE_FOR_LEAD: Lazy = Lazy::new(|| re(r"(?s)^solve\s+for\s+VAR\s+(.+)$", "i"));
static SOLVE_TRAIL: Lazy = Lazy::new(|| re(r"(?s)^(.+?)\s*[,;]?\s+solve\s+(?:for\s+)?VAR$", "i"));
static FOR_TRAIL: Lazy = Lazy::new(|| re(r"(?s)^(solve\s+)?(.+?)\s*[,;]?\s+for\s+VAR$", "i"));
static SOLVE_ALONE: Lazy = Lazy::new(|| re(r"^\s*(isolate|solve)\s+(?:for\s+)?VAR\s*$", "i"));

fn isolate_names(w: &str) -> bool {
    let l = w.to_lowercase();
    l == "e" || l == "i" || l == "theta" || NAMES.split('|').any(|n| n.to_lowercase() == l)
}

fn caps<'t>(r: &jsre::Regex, s: &'t str) -> Option<jsre::Caps<'t>> {
    r.captures(s).ok().flatten()
}

pub fn parse_isolate(text: &str) -> Option<IsolateCmd> {
    static REJECT: Lazy = Lazy::new(|| js(r"==|[<>!]=|[≤≥≠<>]", ""));
    let s = subscripts(text.trim());
    let (variable, body) = if let Some(c) = caps(&LEAD, &s) {
        (
            jsre::group(&c, 1).to_string(),
            jsre::group(&c, 2).to_string(),
        )
    } else if let Some(c) = caps(&TRAIL, &s) {
        (
            jsre::group(&c, 2).to_string(),
            jsre::group(&c, 1).to_string(),
        )
    } else {
        return None;
    };
    let body = body.trim();
    if jsre::test(&REJECT, body) {
        return None;
    }
    let parts: Vec<&str> = body.split('=').collect();
    if parts.len() > 2 {
        return None;
    }
    let lhs = parts[0].trim().to_string();
    let rhs = parts.get(1).unwrap_or(&"0").trim().to_string();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some(IsolateCmd { variable, lhs, rhs })
}

/// A solve that names its letter: `solve x in …`, `solve for x: …`, `… for x`, `…, solve x`.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedSolve {
    pub variable: String,
    pub eq: String,
    pub numeric: String,
    pub isolate: String,
    pub near: Option<String>,
}

pub fn parse_named_solve(text: &str) -> Option<NamedSolve> {
    static GRAPH_SYS: Lazy = Lazy::new(|| js(r"^(?:graph|sys)\b", "i"));
    static SOLVE_WORD: Lazy = Lazy::new(|| js(r"^solve\s+", "i"));
    let (g_text, near) = split_near(&solve_call(text));
    let s = subscripts(g_text.trim());
    let (variable, eq) =
        if let Some(c) = caps(&SOLVE_LEAD, &s).or_else(|| caps(&SOLVE_FOR_LEAD, &s)) {
            (
                jsre::group(&c, 1).to_string(),
                jsre::group(&c, 2).to_string(),
            )
        } else if let Some(c) = caps(&SOLVE_TRAIL, &s) {
            (
                jsre::group(&c, 2).to_string(),
                jsre::group(&c, 1).to_string(),
            )
        } else if let Some(c) = caps(&FOR_TRAIL, &s) {
            if c.get(1).is_none() && !jsre::group(&c, 2).contains('=') {
                return None;
            }
            (
                jsre::group(&c, 3).to_string(),
                jsre::group(&c, 2).to_string(),
            )
        } else {
            return None;
        };
    if variable.is_empty() || eq.is_empty() {
        return None;
    }
    let eq = jsre::replace_all(&SOLVE_WORD, eq.trim(), |_| String::new()).into_owned();
    if jsre::test(&GRAPH_SYS, &eq) {
        return None;
    }
    let isolate = format!("isolate {variable} in {eq}");
    parse_isolate(&isolate)?;
    let near_text = near
        .as_ref()
        .map(|n| format!(" near {n}"))
        .unwrap_or_default();
    Some(NamedSolve {
        numeric: format!(
            "solve {} for {variable}{near_text}",
            split_short_words(&eq, &variable)
        ),
        variable,
        eq,
        isolate,
        near,
    })
}

/// Lines isolate or a named solve owns.
pub fn is_isolate_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*isolate\b|\bisolate\s+[A-Za-zθ]", "i"));
    jsre::test(&RE, text) || jsre::test(&SOLVE_ALONE, text) || parse_named_solve(text).is_some()
}

fn row_equation(expr: &str) -> Option<String> {
    static SOLVE_WORD: Lazy = Lazy::new(|| js(r"^solve\s+", "i"));
    static GRAPH_SYS: Lazy = Lazy::new(|| js(r"^(?:graph|sys)\b", "i"));
    static FN_DEF: Lazy = Lazy::new(|| js(r"(?s)^[A-Za-z][A-Za-z0-9]*\s*\(.*\)\s*=", ""));
    if let Some(n) = parse_named_solve(expr) {
        return Some(n.eq);
    }
    if let Some(i) = parse_isolate(expr) {
        return Some(format!("{} = {}", i.lhs, i.rhs));
    }
    let eq = jsre::replace_all(&SOLVE_WORD, expr.trim(), |_| String::new()).into_owned();
    if eq.split('=').count() != 2 || jsre::test(&GRAPH_SYS, &eq) || jsre::test(&FN_DEF, &eq) {
        return None;
    }
    Some(eq)
}

/// `isolate x` or `solve for x` alone, rewritten to work on the previous line's equation.
pub fn isolate_previous(text: &str, previous: Option<&str>) -> Option<String> {
    let s = subscripts(text);
    let c = caps(&SOLVE_ALONE, &s)?;
    let eq = row_equation(previous?)?;
    let v = jsre::group(&c, 2);
    Some(if jsre::group(&c, 1).to_lowercase() == "solve" {
        format!("solve {eq} for {v}")
    } else {
        format!("isolate {v} in {eq}")
    })
}

/// Letters written together are a product in a formula (`PV = nRT`).
pub fn split_short_words(side: &str, keep: &str) -> String {
    static RE: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_.])[A-Za-z]{2,3}(?![A-Za-z0-9_]|\s*\()", ""));
    jsre::replace_all(&RE, side, |c| {
        let w = jsre::group(c, 0);
        if w == keep || isolate_names(w) {
            w.to_string()
        } else {
            w.chars().map(String::from).collect::<Vec<_>>().join(" ")
        }
    })
    .into_owned()
}

// ---------- node helpers ----------

type N = Node;

fn c(n: f64) -> N {
    symbolic::num(n)
}
fn s(name: &str) -> N {
    symbolic::sym(name)
}
fn bin(o: char, a: N, b: N) -> N {
    let op = match o {
        '+' => Op::Add,
        '-' => Op::Subtract,
        '*' => Op::Multiply,
        '/' => Op::Divide,
        _ => Op::Pow,
    };
    symbolic::op(op, vec![a, b])
}
fn neg(a: N) -> N {
    symbolic::op(Op::UnaryMinus, vec![a])
}
fn call(name: &str, args: Vec<N>) -> N {
    symbolic::call(name, args)
}

fn strip(n: &N) -> &N {
    symbolic::content(n)
}

fn op_of(n: &N) -> Option<(Op, &[N])> {
    match n {
        Node::Operator { op, args, .. } => Some((*op, args)),
        _ => None,
    }
}

fn is_var(n: &N, v: &str) -> bool {
    matches!(n, Node::Symbol(name) if name == v)
}

fn count(n: &N, v: &str) -> usize {
    let mut k = 0;
    symbolic::traverse(n, &mut |m, callee| {
        if !callee && is_var(m, v) {
            k += 1;
        }
    });
    k
}

fn has(n: &N, v: &str) -> bool {
    count(n, v) > 0
}

fn parse_side(side: &str, v: &str, known: &BTreeMap<String, f64>) -> Option<N> {
    let text = preprocess_ascii(&split_short_words(&normalize_math_text(side), v), &[]);
    let node = crate::syntax::parse(&text).ok()?;
    let mut ok = true;
    symbolic::traverse(&node, &mut |n, _| {
        if matches!(
            n,
            Node::Assignment { .. } | Node::FunctionAssignment { .. } | Node::Array(_)
        ) {
            ok = false;
        }
    });
    if !ok {
        return None;
    }
    Some(symbolic::transform_with(&node, &mut |n, callee| {
        if let Node::Symbol(name) = n
            && !callee
        {
            return match known.get(name) {
                Some(x) if name != v && x.is_finite() => Some(c(*x)),
                _ => None,
            };
        }
        // log is natural from here on (its arguments aren't visited, as in the TS)
        if n.call_name() == Some("ln") {
            return Some(call("log", symbolic::args(n).to_vec()));
        }
        None
    }))
}

/// simplify, except a function of plain numbers stays exact.
fn simp(n: &N) -> N {
    let mut held: Vec<N> = Vec::new();
    let hold = symbolic::transform(n, &mut |m| {
        if !matches!(m, Node::Function { .. }) {
            return None;
        }
        let symbolic_arg = symbolic::args(m)
            .iter()
            .any(|a| !symbolic::filter(a, &mut |x, _| matches!(x, Node::Symbol(_))).is_empty());
        if symbolic_arg {
            return None;
        }
        held.push(m.clone());
        Some(s(&format!("__held{}", held.len() - 1)))
    });
    let out = symbolic::simplify(&hold).unwrap_or(hold);
    symbolic::transform(&out, &mut |m| match m {
        Node::Symbol(name) => name
            .strip_prefix("__held")
            .and_then(|k| k.parse::<usize>().ok())
            .and_then(|k| held.get(k).cloned()),
        _ => None,
    })
}

fn const_value(n: &N) -> Option<f64> {
    let st = strip(n);
    if let Some(v) = symbolic::number_value(st) {
        return Some(v);
    }
    let (op, args) = op_of(st)?;
    match op {
        Op::UnaryMinus => const_value(args.first()?).map(|a| -a),
        Op::Divide => {
            let a = const_value(args.first()?)?;
            let b = const_value(args.get(1)?)?;
            (b != 0.0).then(|| a / b)
        }
        _ => None,
    }
}

fn is_zero(n: &N) -> bool {
    const_value(&simp(n)) == Some(0.0)
}

// ---------- path 1: unwind ----------

fn inverse(name: &str, y: N) -> Option<Vec<N>> {
    Some(match name {
        "sqrt" => vec![bin('^', y, c(2.0))],
        "cbrt" => vec![bin('^', y, c(3.0))],
        "exp" => vec![call("log", vec![y])],
        "log" => vec![bin('^', s("e"), y)],
        "log10" => vec![bin('^', c(10.0), y)],
        "log2" => vec![bin('^', c(2.0), y)],
        "sin" => vec![call("asin", vec![y])],
        "cos" => vec![call("acos", vec![y])],
        "tan" => vec![call("atan", vec![y])],
        "asin" => vec![call("sin", vec![y])],
        "acos" => vec![call("cos", vec![y])],
        "atan" => vec![call("tan", vec![y])],
        "sinh" => vec![call("asinh", vec![y])],
        "cosh" => vec![call("acosh", vec![y])],
        "tanh" => vec![call("atanh", vec![y])],
        "asinh" => vec![call("sinh", vec![y])],
        "acosh" => vec![call("cosh", vec![y])],
        "atanh" => vec![call("tanh", vec![y])],
        "abs" => vec![y.clone(), neg(y)],
        _ => return None,
    })
}

fn has_inverse(name: &str) -> bool {
    inverse(name, c(0.0)).is_some()
}

fn unwind(node: &N, others: Vec<N>, v: &str) -> Option<Vec<N>> {
    let mut node = node.clone();
    let mut others = others;
    for _ in 0..64 {
        node = strip(&node).clone();
        if is_var(&node, v) {
            return Some(others);
        }
        if let Some((op, args)) = op_of(&node) {
            let args = args.to_vec();
            match op {
                Op::UnaryPlus => {
                    node = args.first()?.clone();
                    continue;
                }
                Op::UnaryMinus => {
                    others = others.into_iter().map(neg).collect();
                    node = args.first()?.clone();
                    continue;
                }
                _ => {}
            }
            let (a, b) = (args.first()?.clone(), args.get(1)?.clone());
            let in_a = has(&a, v);
            match op {
                Op::Add => {
                    let (inner, rest) = if in_a { (a, b) } else { (b, a) };
                    others = others
                        .into_iter()
                        .map(|y| bin('-', y, rest.clone()))
                        .collect();
                    node = inner;
                }
                Op::Subtract => {
                    others = if in_a {
                        others.into_iter().map(|y| bin('+', y, b.clone())).collect()
                    } else {
                        others.into_iter().map(|y| bin('-', a.clone(), y)).collect()
                    };
                    node = if in_a { a } else { b };
                }
                Op::Multiply => {
                    let (inner, rest) = if in_a { (a, b) } else { (b, a) };
                    others = others
                        .into_iter()
                        .map(|y| bin('/', y, rest.clone()))
                        .collect();
                    node = inner;
                }
                Op::Divide => {
                    others = if in_a {
                        others.into_iter().map(|y| bin('*', y, b.clone())).collect()
                    } else {
                        others.into_iter().map(|y| bin('/', a.clone(), y)).collect()
                    };
                    node = if in_a { a } else { b };
                }
                Op::Pow => {
                    if in_a {
                        let k = const_value(&b);
                        match k {
                            Some(k) if crate::jsnum::is_integer(k) && k % 2.0 == 0.0 => {
                                let root = |y: N| {
                                    if k == 2.0 {
                                        call("sqrt", vec![y])
                                    } else {
                                        bin('^', y, bin('/', c(1.0), c(k)))
                                    }
                                };
                                others = others
                                    .into_iter()
                                    .flat_map(|y| [root(y.clone()), neg(root(y))])
                                    .collect();
                            }
                            Some(k) if k == 3.0 => {
                                others =
                                    others.into_iter().map(|y| call("cbrt", vec![y])).collect();
                            }
                            Some(k) if k == 0.5 => {
                                others = others.into_iter().map(|y| bin('^', y, c(2.0))).collect();
                            }
                            _ => {
                                others = others
                                    .into_iter()
                                    .map(|y| bin('^', y, bin('/', c(1.0), b.clone())))
                                    .collect();
                            }
                        }
                        node = a;
                    } else {
                        let natural = matches!(strip(&a), Node::Symbol(n) if n == "e");
                        others = others
                            .into_iter()
                            .map(|y| {
                                if natural {
                                    call("log", vec![y])
                                } else {
                                    bin('/', call("log", vec![y]), call("log", vec![a.clone()]))
                                }
                            })
                            .collect();
                        node = b;
                    }
                }
                _ => return None,
            }
            continue;
        }
        if let Node::Function { args, .. } = &node {
            let name = node.call_name().unwrap_or("").to_string();
            let args = args.clone();
            if name == "nthRoot" && args.len() == 2 && !has(&args[1], v) {
                others = others
                    .into_iter()
                    .map(|y| bin('^', y, args[1].clone()))
                    .collect();
                node = args[0].clone();
                continue;
            }
            if name == "log" && args.len() == 2 && !has(&args[1], v) {
                others = others
                    .into_iter()
                    .map(|y| bin('^', args[1].clone(), y))
                    .collect();
                node = args[0].clone();
                continue;
            }
            if !has_inverse(&name) || args.len() != 1 {
                return None;
            }
            others = others
                .into_iter()
                .flat_map(|y| inverse(&name, y).unwrap_or_default())
                .collect();
            node = args[0].clone();
            continue;
        }
        return None;
    }
    None
}

// ---------- path 2: a polynomial ----------

const MAX_DEGREE: usize = 6;

struct Rational {
    num: Vec<N>,
    den: Vec<N>,
}

fn padd(p: &[N], q: &[N]) -> Vec<N> {
    (0..p.len().max(q.len()))
        .map(|i| match (p.get(i), q.get(i)) {
            (Some(a), Some(b)) => bin('+', a.clone(), b.clone()),
            (Some(a), None) | (None, Some(a)) => a.clone(),
            (None, None) => c(0.0),
        })
        .collect()
}

fn pmul(p: &[N], q: &[N]) -> Option<Vec<N>> {
    if p.len() + q.len() - 1 > MAX_DEGREE + 1 {
        return None;
    }
    let mut out: Vec<Option<N>> = vec![None; p.len() + q.len() - 1];
    for (i, a) in p.iter().enumerate() {
        for (j, b) in q.iter().enumerate() {
            let t = bin('*', a.clone(), b.clone());
            out[i + j] = Some(match out[i + j].take() {
                Some(o) => bin('+', o, t),
                None => t,
            });
        }
    }
    Some(
        out.into_iter()
            .map(|t| t.unwrap_or_else(|| c(0.0)))
            .collect(),
    )
}

fn pneg(p: &[N]) -> Vec<N> {
    p.iter().cloned().map(neg).collect()
}

/// None for "not a rational function" and for the TS's degree throw alike.
fn rational(node: &N, v: &str) -> Option<Rational> {
    let node = strip(node);
    let arith = matches!(
        op_of(node),
        Some((
            Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::UnaryMinus | Op::UnaryPlus,
            _
        ))
    );
    if !has(node, v) && (!arith || const_value(node).is_some()) {
        return Some(Rational {
            num: vec![node.clone()],
            den: vec![c(1.0)],
        });
    }
    if matches!(node, Node::Symbol(_)) {
        return Some(Rational {
            num: vec![c(0.0), c(1.0)],
            den: vec![c(1.0)],
        });
    }
    let (op, args) = op_of(node)?;
    match op {
        Op::UnaryPlus => return rational(args.first()?, v),
        Op::UnaryMinus => {
            let r = rational(args.first()?, v)?;
            return Some(Rational {
                num: pneg(&r.num),
                den: r.den,
            });
        }
        Op::Pow => {
            let k = const_value(args.get(1)?)?;
            if !crate::jsnum::is_integer(k) || k.abs() > MAX_DEGREE as f64 {
                return None;
            }
            let r = rational(args.first()?, v)?;
            let mut out = Rational {
                num: vec![c(1.0)],
                den: vec![c(1.0)],
            };
            for _ in 0..k.abs() as usize {
                out = Rational {
                    num: pmul(&out.num, &r.num)?,
                    den: pmul(&out.den, &r.den)?,
                };
            }
            return Some(if k < 0.0 {
                Rational {
                    num: out.den,
                    den: out.num,
                }
            } else {
                out
            });
        }
        _ => {}
    }
    let a = rational(args.first()?, v)?;
    let b = rational(args.get(1)?, v)?;
    match op {
        Op::Add | Op::Subtract => {
            let bn = if op == Op::Add {
                b.num.clone()
            } else {
                pneg(&b.num)
            };
            Some(Rational {
                num: padd(&pmul(&a.num, &b.den)?, &pmul(&bn, &a.den)?),
                den: pmul(&a.den, &b.den)?,
            })
        }
        Op::Multiply => Some(Rational {
            num: pmul(&a.num, &b.num)?,
            den: pmul(&a.den, &b.den)?,
        }),
        Op::Divide => Some(Rational {
            num: pmul(&a.num, &b.den)?,
            den: pmul(&a.den, &b.num)?,
        }),
        _ => None,
    }
}

struct Found {
    roots: Vec<N>,
    text: Option<String>,
}

fn poly_solve(f: &N, v: &str) -> Option<Found> {
    let r = rational(f, v)?;
    let mut cs: Vec<N> = r.num.iter().map(simp).collect();
    while cs.last().is_some_and(is_zero) {
        cs.pop();
    }
    if cs.len() == 2 {
        return Some(Found {
            roots: vec![simp(&bin('/', neg(cs[0].clone()), cs[1].clone()))],
            text: None,
        });
    }
    if cs.len() == 3 {
        let (c0, c1, c2) = (cs[0].clone(), cs[1].clone(), cs[2].clone());
        if is_zero(&c1) {
            let sq = simp(&bin('/', neg(c0), c2));
            let root = call("sqrt", vec![sq]);
            let text = format!("±{}", show(&root));
            return Some(Found {
                roots: vec![root.clone(), neg(root)],
                text: Some(text),
            });
        }
        let disc = simp(&bin(
            '-',
            bin('^', c1.clone(), c(2.0)),
            bin('*', bin('*', c(4.0), c2.clone()), c0),
        ));
        let mut top = simp(&neg(c1));
        let mut bottom = simp(&bin('*', c(2.0), c2));
        if negative(&bottom) {
            top = simp(&neg(top));
            bottom = simp(&neg(bottom));
        }
        let root = |sign: char| {
            bin(
                '/',
                bin(sign, top.clone(), call("sqrt", vec![disc.clone()])),
                bottom.clone(),
            )
        };
        let pm = format!("({} ± sqrt({}))", show(&top), show(&disc));
        let text = if const_value(&bottom) == Some(1.0) {
            pm[1..pm.len() - 1].to_string()
        } else {
            format!("{pm}/{}", wrap(&bottom))
        };
        return Some(Found {
            roots: vec![root('+'), root('-')],
            text: Some(text),
        });
    }
    None
}

// ---------- checking and showing ----------

#[derive(PartialEq, Eq, Clone, Copy)]
enum Check {
    Everywhere,
    Somewhere,
    No,
}

fn checks(lhs: &N, rhs: &N, root: &N, v: &str) -> Check {
    let plus = agreement(lhs, rhs, root, v, false);
    if plus.0 > 0 && plus.1 == 0 {
        return Check::Everywhere;
    }
    if plus.0 > 0 || agreement(lhs, rhs, root, v, true).0 > 0 {
        Check::Somewhere
    } else {
        Check::No
    }
}

fn agreement(lhs: &N, rhs: &N, root: &N, v: &str, signed: bool) -> (u32, u32) {
    let mut names: Vec<String> = Vec::new();
    for n in [lhs, rhs, root] {
        symbolic::traverse(n, &mut |m, callee| {
            if let Node::Symbol(name) = m
                && !callee
                && !names.contains(name)
            {
                names.push(name.clone());
            }
        });
    }
    let free: Vec<String> = names
        .into_iter()
        .filter(|n| n != v && !["e", "pi", "tau", "i", "Infinity"].contains(&n.as_str()))
        .collect();
    let po = PrintOptions::default();
    let text = format!(
        "({}) - ({})",
        symbolic::to_string(lhs, &po),
        symbolic::to_string(rhs, &po)
    );
    let Ok(f) = crate::syntax::parse(&text) else {
        return (0, 0);
    };
    let (mut agreed, mut disagreed) = (0, 0);
    for t in 0..8 {
        let scope: Vec<(String, f64)> = free
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let u = 0.37 + ((((i + 1) as f64) * 0.618 + f64::from(t) * 0.414) % 1.0) * 1.7;
                (
                    n.clone(),
                    if signed && (t as usize + i) % 2 == 1 {
                        -u
                    } else {
                        u
                    },
                )
            })
            .collect();
        let look = |name: &str| scope.iter().find(|(k, _)| k == name).map(|(_, x)| *x);
        let x = match eval_plain(root, &look, &no_overrides) {
            Ok(Pv::R(x)) if x.is_finite() => x,
            _ => continue,
        };
        let look2 = |name: &str| if name == v { Some(x) } else { look(name) };
        let y = match eval_plain(&f, &look2, &no_overrides) {
            Ok(Pv::R(y)) if y.is_finite() => y,
            _ => continue,
        };
        let scale = 1f64.max(x.abs());
        if y.abs() > 1e-7 * scale {
            disagreed += 1;
        } else {
            agreed += 1;
        }
    }
    (agreed, disagreed)
}

fn negative(n: &N) -> bool {
    let st = strip(n);
    if matches!(op_of(st), Some((Op::UnaryMinus, _))) {
        return true;
    }
    const_value(st).is_some_and(|k| k < 0.0)
}

fn one_bar(n: &N) -> N {
    symbolic::transform(n, &mut |m| {
        let st = strip(m);
        let Some((Op::Divide, args)) = op_of(st) else {
            return None;
        };
        let mut top = one_bar(&args[0]);
        let mut bottom = one_bar(&args[1]);
        loop {
            let t = strip(&top).clone();
            let Some((Op::Divide, targs)) = op_of(&t) else {
                break;
            };
            top = targs[0].clone();
            bottom = bin('*', targs[1].clone(), bottom);
        }
        Some(bin('/', top, bottom))
    })
}

fn number_first(n: &N) -> N {
    symbolic::transform(n, &mut |m| {
        let Some((Op::Multiply, args)) = op_of(m) else {
            return None;
        };
        let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
            return None;
        };
        (const_value(b).is_some() && const_value(a).is_none())
            .then(|| bin('*', number_first(b), number_first(a)))
    })
}

fn show(n: &N) -> String {
    static LOG: Lazy = Lazy::new(|| js(r"\blog\(", ""));
    static NUM_TIMES: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z_\d.])(\d+(?:\.\d+)?)(?: \* | )(?=[A-Za-z(])",
            "",
        )
    });
    static SPACED: Lazy = Lazy::new(|| js(r" ([*/^]) ", ""));
    let opts = PrintOptions {
        parenthesis: Some(symbolic::Parenthesis::Auto),
        implicit: Some(symbolic::Implicit::Hide),
    };
    let text = symbolic::to_string(&one_bar(&number_first(n)), &opts);
    let t = jsre::replace_all(&LOG, &text, |_| "ln(".into()).into_owned();
    let t = jsre::replace_all(&NUM_TIMES, &t, |c| jsre::group(c, 1).to_string()).into_owned();
    jsre::replace_all(&SPACED, &t, |c| jsre::group(c, 1).to_string()).into_owned()
}

fn wrap(n: &N) -> String {
    static PLAIN: Lazy = Lazy::new(|| js(r"^(?:[A-Za-z][A-Za-z0-9]*|\d+(?:\.\d+)?)$", ""));
    let st = show(n);
    if jsre::test(&PLAIN, &st) {
        st
    } else {
        format!("({st})")
    }
}

fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn tidy(n: &N) -> Vec<N> {
    static DEC: Lazy = Lazy::new(|| js(r"\d\.\d", ""));
    let mut out = vec![simp(n)];
    if let Ok(r) = symbolic::rationalize(n) {
        out.push(r);
    }
    let cost = |t: &N| {
        let st = show(t);
        js_len(&st) + if jsre::test(&DEC, &st) { 1000 } else { 0 }
    };
    out.sort_by_key(cost);
    out
}

fn plus_minus(a: &N, b: &N, sa: &str, sb: &str) -> Option<String> {
    if sb == format!("-{sa}") || sb == format!("-({sa})") {
        return Some(format!("±{sa}"));
    }
    let mid = simp(&bin('/', bin('+', a.clone(), b.clone()), c(2.0)));
    if is_zero(&mid) {
        return Some(format!(
            "±{}",
            show(&simp(&bin('/', bin('-', a.clone(), b.clone()), c(2.0))))
        ));
    }
    let mut half = simp(&bin('/', bin('-', a.clone(), b.clone()), c(2.0)));
    if negative(&half) {
        half = simp(&neg(half));
    }
    let text = format!("{} ± {}", show(&mid), show(&half));
    (js_len(&text) <= js_len(&format!("{sa}, {sb}"))).then_some(text)
}

/// `isolateVariable`'s answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Isolated {
    pub variable: String,
    pub display: String,
    pub forms: Vec<String>,
}

/// `isolate x in …` solved symbolically; None when x can't be moved out alone.
pub fn isolate(text: &str, variables: Option<&BTreeMap<String, f64>>) -> Option<Isolated> {
    let cmd = parse_isolate(text)?;
    let v = if cmd.variable == "θ" {
        "theta".to_string()
    } else {
        cmd.variable.clone()
    };
    let empty = BTreeMap::new();
    let known = variables.unwrap_or(&empty);
    let lhs = parse_side(&cmd.lhs, &v, known)?;
    let rhs = parse_side(&cmd.rhs, &v, known)?;
    let in_l = count(&lhs, &v);
    let in_r = count(&rhs, &v);
    if in_l == 0 && in_r == 0 {
        return None;
    }
    let mut found: Option<Found> = None;
    if in_l + in_r == 1 {
        let roots = if in_l > 0 {
            unwind(&lhs, vec![rhs.clone()], &v)
        } else {
            unwind(&rhs, vec![lhs.clone()], &v)
        };
        if let Some(roots) = roots {
            found = Some(Found {
                roots: roots.iter().map(simp).collect(),
                text: None,
            });
        }
    }
    let poly = poly_solve(&bin('-', lhs.clone(), rhs.clone()), &v);
    let len = |f: &Found| {
        let t = f
            .text
            .clone()
            .unwrap_or_else(|| f.roots.iter().map(show).collect::<Vec<_>>().join(","));
        js_len(&t) + 4 * t.matches('/').count()
    };
    if let Some(p) = poly {
        let take = match &found {
            None => true,
            Some(f) => p.roots.len() == f.roots.len() && len(&p) < len(f),
        };
        if take {
            found = Some(p);
        }
    }
    let found = found?;
    let mut good: Vec<N> = Vec::new();
    let mut all = true;
    for r in &found.roots {
        let tries: Vec<N> = if has(r, &v) {
            Vec::new()
        } else if found.text.is_some() {
            vec![r.clone()]
        } else {
            tidy(r)
        };
        let verdicts: Vec<Check> = tries.iter().map(|t| checks(&lhs, &rhs, t, &v)).collect();
        let at = verdicts
            .iter()
            .position(|c| *c == Check::Everywhere)
            .or_else(|| verdicts.iter().position(|c| *c == Check::Somewhere));
        match at {
            Some(i) => good.push(tries[i].clone()),
            None => all = false,
        }
    }
    if good.is_empty() {
        return None;
    }
    let mut forms: Vec<String> = Vec::new();
    for g in &good {
        let st = show(g);
        if !forms.contains(&st) {
            forms.push(st);
        }
    }
    let mut body = forms.join(", ");
    if all && let Some(t) = &found.text {
        body = t.clone();
    } else if forms.len() == 2
        && let Some(pm) = plus_minus(&good[0], &good[1], &forms[0], &forms[1])
    {
        body = pm;
    }
    let name = cmd.variable;
    Some(Isolated {
        display: format!("{name} = {body}"),
        variable: name,
        forms,
    })
}

/// The evaluate pipeline's form: the display.
pub fn isolate_variable(text: &str, known: Option<&BTreeMap<String, f64>>) -> Option<String> {
    isolate(text, known).map(|i| i.display)
}

pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    let st = |i: usize| crate::golden::str_arg(args, i).unwrap_or_default();
    Some(match name {
        "parseIsolate" => parse_isolate(&st(0)).map_or(
            Json::Null,
            |c| json!({ "variable": c.variable, "lhs": c.lhs, "rhs": c.rhs }),
        ),
        "parseNamedSolve" => parse_named_solve(&st(0)).map_or(Json::Null, |n| {
            let mut o = json!({
                "variable": n.variable, "eq": n.eq, "numeric": n.numeric, "isolate": n.isolate,
            });
            if let Some(near) = n.near {
                o["near"] = json!(near);
            }
            o
        }),
        "isIsolateCommand" => json!(is_isolate_command(&st(0))),
        "isolatePrevious" => json!(isolate_previous(
            &st(0),
            crate::golden::str_arg(args, 1).as_deref()
        )),
        "isolateVariable" => {
            let vars: Option<BTreeMap<String, f64>> = args
                .get(1)
                .map(crate::golden::decode)
                .and_then(|o| o.get("variables").cloned())
                .and_then(|v| serde_json::from_value(v).ok());
            isolate(&st(0), vars.as_ref()).map_or(
                Json::Null,
                |i| json!({ "variable": i.variable, "display": i.display, "forms": i.forms }),
            )
        }
        _ => return None,
    })
}
