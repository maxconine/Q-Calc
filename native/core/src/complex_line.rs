//! native only: a line in the engineer's complex notation. A lowercase `j` is the imaginary unit (`J` stays joules;
//! `kj` and `mj` stay energy), and may lead its number (`795 - j1060`, `e^(j0.5)`); `10∠30` and `polar(10, 30)` are
//! polar; `in polar` and `in rect` choose the answer's form; and a complex number can carry a unit (`(3 + j4) kΩ`),
//! which the unit reader (real numbers only) and the mathjs scope (no complex unit) each can't read. Only lines with
//! one of these reach here, so everything else stays on the engine's own path. The answer reads `j` when `j` was
//! typed. `i` and `j` are the same unit, spelled alike in all of this. A `j` you defined (`j = 5`) is your variable.

use std::collections::BTreeMap;

use crate::eval::complex::C;
use crate::eval::{self, Scope, Val};
use crate::format::format_number;
use crate::jsnum;
use crate::jsre::{self, Lazy, js};
use crate::scientific::functions::jsmath;
use crate::scientific::{ScientificContext, preprocess_ascii, scope_for, typed_i};
use crate::syntax::{Node, Op, parse};
use crate::types::{AngleMode, Complex, Value};
use crate::units::mathjs::{self, MathUnit};

/// The line in the terms the engine reads, and what it asked of the answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rewritten {
    pub expr: String,
    pub j: bool,
    pub polar: bool,
    /// `… in kΩ`: the unit the answer is asked in.
    pub target: Option<String>,
}

const NUMBER: &str = r"\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?";

/// `j` as `i`, a leading `j1060` as `(1060i)`; whether a `j`, and any such spelling, was met.
fn imaginary_words(
    text: &str,
    variables: &BTreeMap<String, f64>,
    functions: &[String],
) -> (String, bool, bool) {
    let chars: Vec<char> = text.chars().collect();
    let known = |w: &str| variables.contains_key(w) || functions.iter().any(|f| f == w);
    let (mut out, mut i, mut saw_j, mut spelled) = (String::new(), 0, false, false);
    while i < chars.len() {
        let c = chars[i];
        let after_word = i > 0 && (chars[i - 1].is_alphabetic() || chars[i - 1] == '_');
        // a number whole, so the e of 2e3 isn't a word
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                out.push(chars[i]);
                i += 1;
            }
            let mut k = i;
            if matches!(chars.get(k), Some('e' | 'E')) {
                k += 1;
                if matches!(chars.get(k), Some('+' | '-')) {
                    k += 1;
                }
                if chars.get(k).is_some_and(char::is_ascii_digit) {
                    while k < chars.len() && chars[k].is_ascii_digit() {
                        k += 1;
                    }
                    out.extend(&chars[i..k]);
                    i = k;
                }
            }
            continue;
        }
        if !(c.is_ascii_alphabetic() || c == '_') || after_word {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let head = word.chars().next().unwrap_or(' ');
        let tail = &word[1..];
        let imaginary = head == 'j' || head == 'i';
        if !imaginary
            || known(&word)
            || (!tail.is_empty() && !tail.chars().all(|d| d.is_ascii_digit()))
        {
            out.push_str(&word);
            continue;
        }
        // a call of the word's own, not the unit, when it's a name you gave it
        if known(&head.to_string()) {
            out.push_str(&word);
            continue;
        }
        // the fraction of `j0.5` or `j.5`
        let mut digits = tail.to_string();
        if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
            digits.push('.');
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                digits.push(chars[i]);
                i += 1;
            }
        }
        saw_j |= head == 'j';
        if digits.is_empty() {
            spelled |= head == 'j';
            out.push('i');
            if chars[i..].iter().find(|c| !c.is_whitespace()) == Some(&'(') {
                out.push('*');
                spelled = true;
            }
        } else {
            out.push_str(&format!("({digits}i)"));
            spelled = true;
        }
    }
    (out, saw_j, spelled)
}

/// `10∠30`, `10 angle 30°` and `polar(10, 30)` as `r e^(iθ)`: an angle takes its typed unit, else the mode.
fn polar_input(text: &str, mode: AngleMode) -> (String, bool) {
    static ANGLE: Lazy = Lazy::new(|| {
        js(
            &format!(
                r"(?<![A-Za-z0-9_.])((?:{NUMBER})|\([^()]*\))(?:\s*∠\s*|\s+angle\s+)(-?(?:{NUMBER})|\([^()]*\))\s*(°|deg(?:rees?)?\b|rad(?:ians?)?\b)?"
            ),
            "i",
        )
    });
    static CALL: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z0-9_.])polar\(\s*([^(),]+?)\s*,\s*([^(),]+?)\s*(°|deg(?:rees?)?\b|rad(?:ians?)?\b)?\s*\)",
            "i",
        )
    });
    let radians = |angle: &str, unit: &str| {
        let degrees = if unit.is_empty() {
            mode == AngleMode::Deg
        } else {
            !unit.to_lowercase().starts_with("rad")
        };
        if degrees {
            format!("({angle})*pi/180")
        } else {
            format!("({angle})")
        }
    };
    let mut hit = false;
    let mut swap = |t: &str, re: &jsre::Regex| {
        jsre::replace_all(re, t, |c| {
            hit = true;
            format!(
                "(({})*e^(i*{}))",
                jsre::group(c, 1),
                radians(jsre::group(c, 2), jsre::group(c, 3))
            )
        })
        .into_owned()
    };
    let once = swap(text, &ANGLE);
    let out = swap(&once, &CALL);
    (out, hit)
}

/// The line as this layer reads it, or None when it has none of the notation.
pub fn rewrite(
    text: &str,
    variables: &BTreeMap<String, f64>,
    functions: &[String],
    mode: AngleMode,
) -> Option<Rewritten> {
    static FORM: Lazy = Lazy::new(|| {
        js(
            r"\s+(?:in|to|as)\s+(polar|rect|rectangular|cartesian)\s*$",
            "i",
        )
    });
    static TARGET: Lazy = Lazy::new(|| {
        js(
            r"\s+(?:in|to|as)\s+([A-Za-zµμΩ][A-Za-z0-9µμΩ/^·*]*)\s*$",
            "",
        )
    });
    static TO_JOULES: Lazy = Lazy::new(|| js(r"\b(?:to|into|in|as)\s+j\s*$", ""));
    static FROM_JOULES: Lazy = Lazy::new(|| {
        js(
            &format!(r"^\s*(?:{NUMBER})\s*j\s+(?:to|into|in|as)\s+[A-Za-z]"),
            "",
        )
    });
    static ALIAS_ARG: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_.])(?:angle|phase)\s*\(", "i"));
    static ALIAS_ABS: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_.])(?:mag|magnitude)\s*\(", "i"));
    let (body, polar) = match FORM.captures(text).ok().flatten() {
        Some(c) => (
            text[..c.get(0)?.start()].to_string(),
            jsre::group(&c, 1).eq_ignore_ascii_case("polar"),
        ),
        None => (text.to_string(), false),
    };
    let asked_form = body.len() != text.len();
    // a conversion of energy keeps its joules: `5 j to cal`, `1 kcal to j`
    if !asked_form && (jsre::test(&TO_JOULES, &body) || jsre::test(&FROM_JOULES, &body)) {
        return None;
    }
    let (imag, j, spelled) = imaginary_words(&body, variables, functions);
    let (expr, polar_typed) = polar_input(&imag, mode);
    // polar is the engineer's notation too
    let j = j || polar_typed;
    if !(spelled || j || polar_typed || (asked_form && typed_i(&imag))) {
        return None;
    }
    let mut target = None;
    let asked = TARGET.captures(expr.as_str()).ok().flatten().map(|c| {
        (
            c.get(0).map_or(0, |m| m.start()),
            jsre::group(&c, 1).to_string(),
        )
    });
    let mut expr = expr;
    if let Some((at, unit)) = asked {
        let unit = unit.replace('Ω', "ohm").replace(['µ', 'μ'], "u");
        if mathjs::symbol(&unit).is_some() {
            expr.truncate(at);
            target = Some(unit);
        }
    }
    let expr: String =
        jsre::replace_all(&ALIAS_ARG, expr.as_str(), |_| "arg(".to_string()).into_owned();
    let expr: String =
        jsre::replace_all(&ALIAS_ABS, expr.as_str(), |_| "abs(".to_string()).into_owned();
    Some(Rewritten {
        expr,
        j,
        polar,
        target,
    })
}

/// A typed `i` with a unit (`(3 + 4i) Ω`) or a leading number (`i0.5`), once the engine's own path has no answer.
pub fn fallback(text: &str, ctx: &ScientificContext) -> Option<Value> {
    let (expr, _, _) = imaginary_words(
        text,
        &ctx.variables,
        &ctx.functions.keys().cloned().collect::<Vec<_>>(),
    );
    if !typed_i(&expr) {
        return None;
    }
    evaluate(
        &Rewritten {
            expr,
            ..Rewritten::default()
        },
        ctx,
    )
}

/// A value and its base dimensions, in SI.
#[derive(Clone, Copy)]
struct Q {
    z: C,
    dims: [f64; 9],
}

const NONE: [f64; 9] = [0.0; 9];
const ANGLE_DIM: usize = 7;

fn same(a: &[f64; 9], b: &[f64; 9]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// Units met in the line, the answer's first choice of how to say its dimensions.
#[derive(Default)]
struct Walk {
    typed: Vec<(String, f64, [f64; 9])>,
}

fn is_unit(name: &str, scope: &Scope) -> bool {
    const CONSTANTS: [&str; 7] = ["e", "E", "i", "pi", "PI", "tau", "phi"];
    !CONSTANTS.contains(&name)
        && !scope.variables.contains_key(name)
        && matches!(mathjs::symbol(name), Some(Val::Unit(_)))
}

fn has_unit(n: &Node, scope: &Scope) -> bool {
    match n {
        Node::Symbol(s) => is_unit(s, scope),
        Node::Parenthesis(inner) => has_unit(inner, scope),
        Node::Operator { args, .. } | Node::Function { args, .. } => {
            args.iter().any(|a| has_unit(a, scope))
        }
        _ => false,
    }
}

/// Functions this layer takes a complex argument to; the engine's own read only the real line.
const CALLS: [&str; 10] = [
    "sin", "cos", "tan", "sinh", "cosh", "tanh", "ln", "log", "exp", "arg",
];

fn has_complex_call(n: &Node) -> bool {
    match n {
        Node::Parenthesis(inner) => has_complex_call(inner),
        Node::Operator { args, .. } => args.iter().any(has_complex_call),
        Node::Function { callee, args } => {
            matches!(callee.as_ref(), Node::Symbol(s) if CALLS.contains(&s.to_lowercase().as_str()))
                || args.iter().any(has_complex_call)
        }
        _ => false,
    }
}

fn sin_cos(z: C) -> (C, C) {
    let (s, c) = (jsmath::sin(z.re), jsmath::cos(z.re));
    let (sh, ch) = (jsmath::sinh(z.im), jsmath::cosh(z.im));
    (C::new(s * ch, c * sh), C::new(c * ch, -s * sh))
}

fn unit_q(u: &MathUnit) -> Q {
    Q {
        z: C::new(u.si_factor(), 0.0),
        dims: u.dims,
    }
}

impl Walk {
    fn eval(&mut self, n: &Node, scope: &Scope) -> Option<Q> {
        if !has_unit(n, scope) && !has_complex_call(n) {
            return match eval::evaluate(n, scope).ok()? {
                Val::Num(x) => Some(Q {
                    z: C::new(x, 0.0),
                    dims: NONE,
                }),
                Val::Complex { re, im } => Some(Q {
                    z: C::new(re, im),
                    dims: NONE,
                }),
                _ => None,
            };
        }
        match n {
            Node::Symbol(s) => {
                let Some(Val::Unit(u)) = mathjs::symbol(s) else {
                    return None;
                };
                // an offset unit (°C) has no complex meaning
                if u.units.iter().any(|p| p.offset != 0.0) {
                    return None;
                }
                let q = unit_q(&u);
                self.typed.push((s.clone(), u.si_factor(), u.dims));
                Some(q)
            }
            Node::Parenthesis(inner) => self.eval(inner, scope),
            Node::Operator { op, args, .. } => {
                let a = self.eval(args.first()?, scope)?;
                if let Some(second) = args.get(1) {
                    let b = self.eval(second, scope)?;
                    return binary(*op, a, b);
                }
                match op {
                    Op::UnaryMinus => Some(Q {
                        z: a.z.neg(),
                        dims: a.dims,
                    }),
                    Op::UnaryPlus => Some(a),
                    _ => None,
                }
            }
            Node::Function { callee, args } => {
                let Node::Symbol(name) = callee.as_ref() else {
                    return None;
                };
                let a = self.eval(args.first()?, scope)?;
                let real = |x: f64, dims| {
                    Some(Q {
                        z: C::new(x, 0.0),
                        dims,
                    })
                };
                match name.to_lowercase().as_str() {
                    "abs" => real(a.z.abs(), a.dims),
                    "re" | "real" => real(a.z.re, a.dims),
                    "im" | "imag" => real(a.z.im, a.dims),
                    "conj" => Some(Q {
                        z: a.z.conjugate(),
                        dims: a.dims,
                    }),
                    // an angle, in the mode's unit
                    "arg" => {
                        let (unit, per) = if scope.angle_mode == AngleMode::Deg {
                            ("deg", std::f64::consts::PI / 180.0)
                        } else {
                            ("rad", 1.0)
                        };
                        let mut dims = NONE;
                        dims[ANGLE_DIM] = 1.0;
                        self.typed.push((unit.to_string(), per, dims));
                        real(a.z.arg(), dims)
                    }
                    // a real number keeps the engine's reading (and the mode); anything else is radians here
                    fname if CALLS.contains(&fname) => {
                        if a.z.im == 0.0 && same(&a.dims, &NONE) {
                            let arg = Node::Constant(crate::syntax::Constant::Number {
                                value: a.z.re,
                                text: jsnum::to_string(a.z.re),
                            });
                            let call = Node::Function {
                                callee: callee.clone(),
                                args: vec![arg],
                            };
                            return match eval::evaluate(&call, scope).ok()? {
                                Val::Num(x) => real(x, NONE),
                                Val::Complex { re, im } => Some(Q {
                                    z: C::new(re, im),
                                    dims: NONE,
                                }),
                                _ => None,
                            };
                        }
                        let mut rest = a.dims;
                        rest[ANGLE_DIM] = 0.0;
                        if !same(&rest, &NONE) {
                            return None;
                        }
                        complex_call(fname, a.z)
                    }
                    "sqrt" => {
                        let mut dims = a.dims;
                        dims.iter_mut().for_each(|d| *d /= 2.0);
                        Some(Q {
                            z: a.z.sqrt(),
                            dims,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

fn complex_call(name: &str, z: C) -> Option<Q> {
    let (s, c) = sin_cos(z);
    let (hs, hc) = sin_cos(C::new(-z.im, z.re)); // of i z: sin is i sinh, cos is cosh
    let i = C::new(0.0, 1.0);
    let out = match name {
        "sin" => s,
        "cos" => c,
        "tan" => s.div(c),
        "sinh" => hs.div(i),
        "cosh" => hc,
        "tanh" => hs.div(i).div(hc),
        "ln" => z.log(),
        "log" => z.log().div(C::new(jsmath::ln(10.0), 0.0)),
        "exp" => z.exp(),
        _ => return None,
    };
    Some(Q { z: out, dims: NONE })
}

fn binary(op: Op, a: Q, b: Q) -> Option<Q> {
    let plus = |x: [f64; 9], y: [f64; 9], s: f64| {
        let mut d = x;
        d.iter_mut().zip(y).for_each(|(p, q)| *p += s * q);
        d
    };
    match op {
        Op::Add if same(&a.dims, &b.dims) => Some(Q {
            z: a.z.add(b.z),
            dims: a.dims,
        }),
        Op::Subtract if same(&a.dims, &b.dims) => Some(Q {
            z: a.z.sub(b.z),
            dims: a.dims,
        }),
        Op::Multiply | Op::DotMultiply => Some(Q {
            z: a.z.mul(b.z),
            dims: plus(a.dims, b.dims, 1.0),
        }),
        Op::Divide | Op::DotDivide => Some(Q {
            z: a.z.div(b.z),
            dims: plus(a.dims, b.dims, -1.0),
        }),
        Op::Pow | Op::DotPow => {
            // an angle in a power is radians: e^(j30 deg)
            let mut angle_only = b.dims;
            angle_only[ANGLE_DIM] = 0.0;
            if !same(&angle_only, &NONE) {
                return None;
            }
            if same(&a.dims, &NONE) {
                return Some(Q {
                    z: a.z.pow(b.z),
                    dims: NONE,
                });
            }
            // a quantity only takes a real power
            if b.z.im != 0.0 {
                return None;
            }
            let mut dims = a.dims;
            dims.iter_mut().for_each(|d| *d *= b.z.re);
            Some(Q {
                z: a.z.pow(b.z),
                dims,
            })
        }
        _ => None,
    }
}

/// The SI names the answer falls back on when no typed unit fits, with the dimensions they mean.
const NAMED: [(&str, [f64; 9]); 11] = [
    ("Ω", [1.0, 2.0, -3.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("V", [1.0, 2.0, -3.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("A", [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("W", [1.0, 2.0, -3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("F", [-1.0, -2.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("H", [1.0, 2.0, -2.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("S", [-1.0, -2.0, 3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Hz", [0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("J", [1.0, 2.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("C", [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("rad", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
];

// a named unit's prefix for this size, keeping the number readable
fn scaled(name: &str, size: f64) -> (String, f64) {
    const PREFIXES: [(&str, f64); 8] = [
        ("G", 1e9),
        ("M", 1e6),
        ("k", 1e3),
        ("", 1.0),
        ("m", 1e-3),
        ("µ", 1e-6),
        ("n", 1e-9),
        ("p", 1e-12),
    ];
    if name == "rad" || size == 0.0 {
        return (name.to_string(), 1.0);
    }
    let (p, f) = PREFIXES
        .iter()
        .find(|(_, f)| size >= *f)
        .unwrap_or(&PREFIXES[7]);
    (format!("{p}{name}"), *f)
}

/// The line's answer, or None when it has none.
pub fn evaluate(line: &Rewritten, ctx: &ScientificContext) -> Option<Value> {
    // mathjs spells them ohm and u
    let spelled = line.expr.replace('Ω', "ohm").replace(['µ', 'μ'], "u");
    let ascii = preprocess_ascii(&spelled, &[]).replace(['×', '·'], "*");
    crate::work::spend(300 + 10 * ascii.chars().count() as u64);
    let node = parse(&ascii).ok()?;
    let mut scope = scope_for(&ascii, ctx);
    scope.complex_typed = true;
    let mut walk = Walk::default();
    let q = walk.eval(&node, &scope)?;
    if !q.z.re.is_finite() || !q.z.im.is_finite() {
        return Some(Value::text("undefined"));
    }
    let mut unit = None;
    let mut factor = 1.0;
    if !same(&q.dims, &NONE) {
        if let Some(t) = &line.target {
            let Some(Val::Unit(u)) = mathjs::symbol(t) else {
                return None;
            };
            if !same(&u.dims, &q.dims) {
                return None;
            }
            factor = u.si_factor();
            unit = Some(label(t));
        } else if let Some((name, f, _)) = walk.typed.iter().find(|(_, _, d)| same(d, &q.dims)) {
            factor = *f;
            unit = Some(label(name));
        } else {
            let (name, _) = NAMED.iter().find(|(_, d)| same(d, &q.dims))?;
            let (shown, f) = scaled(name, q.z.abs());
            factor = f;
            unit = Some(shown);
        }
    }
    let (re, im) = chopped(q.z.re / factor, q.z.im / factor);
    Some(answer(re, im, unit, line.j, line.polar))
}

// a unit as the bar writes it: `kohm` is kΩ, `uF` is µF
fn label(name: &str) -> String {
    let name = name.replace("ohm", "Ω");
    match name.strip_prefix('u') {
        Some(rest) if rest.chars().next().is_some_and(|c| c.is_alphabetic()) => format!("µ{rest}"),
        _ => name,
    }
}

// float noise in the smaller part: e^(jπ/2) has no real part
fn chopped(re: f64, im: f64) -> (f64, f64) {
    let size = re.abs().max(im.abs());
    let tiny = |x: f64| if x.abs() <= size * 1e-13 { 0.0 } else { x };
    (tiny(re), tiny(im))
}

fn answer(re: f64, im: f64, unit: Option<String>, j: bool, polar: bool) -> Value {
    if im == 0.0 && !polar {
        return Value {
            unit,
            ..Value::number(re)
        };
    }
    let mut v = Value {
        complex: Some(Complex { re, im, j, polar }),
        unit,
        ..Value::text(String::new())
    };
    v.text = Some(shown(&v, 12.0));
    v
}

/// A complex answer asked in `j`, or in polar, of the engine's own answer.
pub fn styled(mut v: Value, line: &Rewritten) -> Value {
    if let Some(z) = v.complex.as_mut() {
        z.j = line.j;
        z.polar = line.polar;
        let (re, im) = chopped(z.re, z.im);
        z.re = re;
        z.im = im;
    } else if line.polar && v.kind == crate::types::ValueKind::Number && v.n.is_finite() {
        return answer(v.n, 0.0, v.unit.clone(), line.j, true);
    }
    if v.complex.is_some() {
        v.text = Some(shown(&v, 12.0));
    }
    v
}

fn snap(x: f64) -> f64 {
    let r = x.round();
    if (x - r).abs() < 1e-9 * x.abs().max(1.0) {
        r
    } else {
        x
    }
}

/// `3 + j4`, `3 - 4i`, `j`: the real part first, a coefficient of 1 left out.
fn rectangular(re: f64, im: f64, j: bool, sig: f64) -> String {
    let size = im.abs();
    let imag = if (size - 1.0).abs() < 1e-12 {
        if j { "j".to_string() } else { "i".to_string() }
    } else if j {
        format!("j{}", format_number(snap(size), sig))
    } else {
        format!("{}i", format_number(snap(size), sig))
    };
    if re == 0.0 {
        return if im < 0.0 { format!("-{imag}") } else { imag };
    }
    format!(
        "{} {} {imag}",
        format_number(snap(re), sig),
        if im < 0.0 { '-' } else { '+' }
    )
}

/// `10∠30°`, in degrees whatever the mode, so it reads the same back.
fn polar_degrees(re: f64, im: f64, sig: f64) -> String {
    let size = C::new(re, im).abs();
    let angle = jsmath::atan2(im, re) * 180.0 / std::f64::consts::PI;
    format!(
        "{}∠{}°",
        format_number(snap(size), sig),
        format_number(snap(angle), sig)
    )
}

/// `10e^(j0.5)`: the exponent in radians.
fn polar_exponential(re: f64, im: f64, j: bool, sig: f64) -> String {
    let size = C::new(re, im).abs();
    let angle = jsmath::atan2(im, re);
    let letter = if j { 'j' } else { 'i' };
    let body = format_number(snap(angle.abs()), sig);
    let sign = if angle < 0.0 { "-" } else { "" };
    let exponent = if j {
        format!("{sign}{letter}{body}")
    } else {
        format!("{sign}{body}{letter}")
    };
    format!("{}e^({exponent})", format_number(snap(size), sig))
}

fn with_unit(z: String, polar: bool, unit: Option<&str>) -> String {
    match unit {
        // `(3 + j4) Ω`, so it reads back as written; polar is one term already
        Some(u) if polar || !z.contains(' ') => format!("{z} {u}"),
        Some(u) => format!("({z}) {u}"),
        None => z,
    }
}

/// A complex value as the bar shows it, at `sig` figures (12 at most, as the engine's own complex answers).
pub fn shown(v: &Value, sig: f64) -> String {
    let Some(z) = v.complex else {
        return String::new();
    };
    let sig = sig.min(12.0);
    let body = if z.polar {
        polar_degrees(z.re, z.im, sig)
    } else {
        rectangular(z.re, z.im, z.j, sig)
    };
    with_unit(body, z.polar, v.unit.as_deref())
}

/// The other ways ⌃⇧E can show a complex answer: rectangular, `r∠θ°` and `r e^(jθ)`.
pub fn forms(v: &Value, sig: f64) -> Vec<(String, Value)> {
    let Some(z) = v.complex else {
        return Vec::new();
    };
    let unit = v.unit.as_deref();
    let keep = |text: String, polar: bool| {
        let mut out = v.clone();
        out.text = Some(text.clone());
        out.complex = Some(Complex { polar, ..z });
        (text, out)
    };
    vec![
        keep(
            with_unit(rectangular(z.re, z.im, z.j, sig.min(12.0)), false, unit),
            false,
        ),
        keep(
            with_unit(polar_degrees(z.re, z.im, sig.min(12.0)), true, unit),
            true,
        ),
        keep(
            with_unit(
                polar_exponential(z.re, z.im, z.j, sig.min(12.0)),
                true,
                unit,
            ),
            true,
        ),
    ]
}

/// What a complex answer is written as to read back, at full precision: `3 + 4j`, `(3 + 4j) Ω`.
pub fn chain_text(v: &Value) -> Option<String> {
    let z = v.complex?;
    let letter = if z.j { 'j' } else { 'i' };
    let body = format!(
        "{} {} {}{letter}",
        jsnum::to_string(z.re),
        if z.im < 0.0 { '-' } else { '+' },
        jsnum::to_string(z.im.abs())
    );
    Some(match &v.unit {
        Some(u) => format!("({body}) {u}"),
        None => body,
    })
}
