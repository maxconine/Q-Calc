//! Native only: indefinite integrals, from a table (the web's calculus has none, so `∫ x^2 dx` was blank there). A
//! sum of terms the table knows, each with a constant in front and a linear inner part: `∫ 3 sec(2x)^2 dx`. Anything
//! else goes to `product`: integration by parts for a few forms (`x^n e^x`, `x^n sin(x)`, `x^n ln(x)`, `e^x sin(x)`)
//! and substitution where the rest is a number times g' (`2x cos(x^2)`). Anything else has no answer rather than a
//! guess, and every answer is checked by differentiating it numerically before it's
//! shown (`super::indefinite_integral`).

use std::cell::RefCell;
use std::f64::consts::{E, PI};

use crate::scientific::functions::jsmath;
use crate::symbolic::Op;

use super::tree::{self, N, T};

/// How a term scales for the angle mode. In degrees `sin(x)` means sin(x°), whose integral picks up 180/π, and
/// `atan(x)` answers in degrees, so it's put back by π/180.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scale {
    None,
    Trig,
    Inverse,
}

/// One term: `coef`, any constant letters, the angle scale, then the function.
#[derive(Clone, Debug)]
struct Term {
    coef: f64,
    factor: Vec<String>,
    scale: Scale,
    show: String,
    eval: String,
}

fn term(coef: f64, scale: Scale, show: impl Into<String>, eval: impl Into<String>) -> Term {
    Term {
        coef,
        factor: Vec::new(),
        scale,
        show: show.into(),
        eval: eval.into(),
    }
}

fn same(show_and_eval: String, coef: f64, scale: Scale) -> Term {
    term(coef, scale, show_and_eval.clone(), show_and_eval)
}

/// The antiderivative: what the answer shows, and what evaluates.
#[derive(Clone, Debug)]
pub struct Antiderivative {
    pub show: String,
    pub eval: String,
}

struct Table<'a> {
    v: &'a str,
    deg: bool,
    /// native only: how each piece was integrated, for the working (`crate::steps`); kept only along the way
    /// that answered
    notes: &'a RefCell<Vec<Note>>,
}

/// One way a piece was integrated, as the working says it: `power rule`, `integrate by parts: u = x, dv = e^x dx`.
/// The higher the rank, the more it says how the whole term was done.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub rank: u8,
    pub text: String,
}

/// The inner part of a table form, named when it isn't the variable itself: `, u = 2x`.
fn inner_note(u: &T) -> String {
    if u.sym_name().is_some() {
        String::new()
    } else {
        format!(", u = {}", compact(u))
    }
}

fn free_of(t: &T, v: &str) -> bool {
    match &**t {
        N::Sym(s) => s != v,
        N::Const(_) => true,
        N::Op(_, a) | N::Call(_, a) => a.iter().all(|x| free_of(x, v)),
    }
}

fn text(t: &T) -> String {
    tree::text_auto(&natural_log(t))
}

/// The tree calls ln `log`, which the calculator reads as base 10.
fn natural_log(t: &T) -> T {
    match &**t {
        N::Call(name, a) => {
            let name = if name == "log" && a.len() == 1 {
                "ln"
            } else {
                name
            };
            tree::call(name, a.iter().map(natural_log).collect())
        }
        N::Op(o, a) => tree::op(*o, a.iter().map(natural_log).collect()),
        _ => t.clone(),
    }
}

/// `t` as the answer shows it: `2x` and `x^2`, not `2 * x` and `x ^ 2`.
fn compact(t: &T) -> String {
    let s = text(t).replace(" ^ ", "^");
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(i) = rest.find(" * ") {
        let before = &rest[..i];
        let after = &rest[i + 3..];
        out.push_str(before);
        let digit = before.ends_with(|c: char| c.is_ascii_digit());
        let letter = after.starts_with(|c: char| c.is_alphabetic());
        out.push_str(if digit && letter { "" } else { " " });
        rest = after;
    }
    out.push_str(rest);
    out
}

impl Table<'_> {
    fn note(&self, rank: u8, text: String) {
        self.notes.borrow_mut().push(Note { rank, text });
    }

    fn trig(&self) -> Scale {
        if self.deg { Scale::Trig } else { Scale::None }
    }

    fn inverse(&self) -> Scale {
        if self.deg {
            Scale::Inverse
        } else {
            Scale::None
        }
    }

    /// The slope of `u` when it's linear in the variable with a number for a slope.
    fn slope(&self, u: &T) -> Option<f64> {
        if free_of(u, self.v) {
            return Some(0.0);
        }
        let a = u.args();
        match &**u {
            N::Sym(_) => Some(1.0),
            N::Op(Op::Add, _) => Some(self.slope(&a[0])? + self.slope(&a[1])?),
            N::Op(Op::Subtract, _) => Some(self.slope(&a[0])? - self.slope(&a[1])?),
            N::Op(Op::UnaryMinus, _) => Some(-self.slope(&a[0])?),
            N::Op(Op::Multiply, _) => {
                if let Some(c) = a[0].const_value() {
                    Some(c * self.slope(&a[1])?)
                } else {
                    Some(a[1].const_value()? * self.slope(&a[0])?)
                }
            }
            N::Op(Op::Divide, _) => Some(self.slope(&a[0])? / a[1].const_value()?),
            _ => None,
        }
        .filter(|s: &f64| s.is_finite())
    }

    fn scaled(&self, terms: Vec<Term>, by: &T) -> Vec<Term> {
        let number = by.const_value();
        terms
            .into_iter()
            .map(|mut t| {
                match number {
                    Some(n) => t.coef *= n,
                    None => t.factor.push(text(by)),
                }
                t
            })
            .collect()
    }

    /// ∫ f(u) dx for a named function of a linear u.
    fn function(&self, name: &str, u: &T) -> Option<Vec<Term>> {
        let a = self.slope(u).filter(|a| *a != 0.0)?;
        let s = text(u);
        let k = 1.0 / a;
        let trig = self.trig();
        let form = match name {
            "sin" => "∫ sin u du = −cos u",
            "cos" => "∫ cos u du = sin u",
            "tan" => "∫ tan u du = −ln|cos u|",
            "cot" => "∫ cot u du = ln|sin u|",
            "sec" => "∫ sec u du = ln|sec u + tan u|",
            "csc" => "∫ csc u du = −ln|csc u + cot u|",
            "sinh" => "∫ sinh u du = cosh u",
            "cosh" => "∫ cosh u du = sinh u",
            "exp" => "∫ eᵘ du = eᵘ",
            "ln" | "log" => "by parts: ∫ ln u du = u ln u − u",
            "sqrt" => "power rule: ∫ √u du = (2/3) u^(3/2)",
            "atan" => "by parts: ∫ atan u du = u atan u − ln(u² + 1)/2",
            "asin" => "by parts: ∫ asin u du = u asin u + √(1 − u²)",
            "acos" => "by parts: ∫ acos u du = u acos u − √(1 − u²)",
            _ => "",
        };
        let terms = match name {
            "sin" => vec![same(format!("cos({s})"), -k, trig)],
            "cos" => vec![same(format!("sin({s})"), k, trig)],
            "tan" => vec![term(
                -k,
                trig,
                format!("ln|cos({s})|"),
                format!("ln(abs(cos({s})))"),
            )],
            "cot" => vec![term(
                k,
                trig,
                format!("ln|sin({s})|"),
                format!("ln(abs(sin({s})))"),
            )],
            "sec" => vec![term(
                k,
                trig,
                format!("ln|sec({s}) + tan({s})|"),
                format!("ln(abs(sec({s}) + tan({s})))"),
            )],
            "csc" => vec![term(
                -k,
                trig,
                format!("ln|csc({s}) + cot({s})|"),
                format!("ln(abs(csc({s}) + cot({s})))"),
            )],
            "sinh" => vec![same(format!("cosh({s})"), k, Scale::None)],
            "cosh" => vec![same(format!("sinh({s})"), k, Scale::None)],
            "exp" => vec![same(format!("e^({s})"), k, Scale::None)],
            // the tree calls ln `log`
            "ln" | "log" => {
                let w = wrap(u);
                vec![
                    term(
                        k,
                        Scale::None,
                        format!("{w} ln({s})"),
                        format!("({s})*ln({s})"),
                    ),
                    term(-k, Scale::None, w, format!("({s})")),
                ]
            }
            "sqrt" => vec![term(
                k * 2.0 / 3.0,
                Scale::None,
                format!("{}^(3/2)", wrap(u)),
                format!("({s})^(3/2)"),
            )],
            // by parts: u f(u) less ∫ u f'(u); in degrees the inverse answers in degrees, so the rest is scaled
            "atan" => {
                let w = wrap(u);
                vec![
                    term(
                        k,
                        Scale::None,
                        format!("{w} atan({s})"),
                        format!("({s})*atan({s})"),
                    ),
                    term(
                        -k / 2.0,
                        trig,
                        format!("ln({w}^2 + 1)"),
                        format!("ln(({s})^2+1)"),
                    ),
                ]
            }
            "asin" | "acos" => {
                let w = wrap(u);
                let sign = if name == "asin" { 1.0 } else { -1.0 };
                vec![
                    term(
                        k,
                        Scale::None,
                        format!("{w} {name}({s})"),
                        format!("({s})*{name}({s})"),
                    ),
                    term(
                        sign * k,
                        trig,
                        format!("sqrt(1 - {w}^2)"),
                        format!("sqrt(1-({s})^2)"),
                    ),
                ]
            }
            _ => return None,
        };
        self.note(1, format!("{form}{}", inner_note(u)));
        Some(terms)
    }

    /// ∫ f(u)^2 dx for the squares in the table.
    fn squared(&self, name: &str, u: &T) -> Option<Vec<Term>> {
        let a = self.slope(u).filter(|a| *a != 0.0)?;
        let s = text(u);
        let k = 1.0 / a;
        let trig = self.trig();
        let form = match name {
            "sec" => "∫ sec² u du = tan u",
            "csc" => "∫ csc² u du = −cot u",
            "sin" => "half-angle identity: sin² u = (1 − cos 2u)/2",
            "cos" => "half-angle identity: cos² u = (1 + cos 2u)/2",
            "tan" => "tan² u = sec² u − 1",
            _ => "",
        };
        let terms = match name {
            "sec" => vec![same(format!("tan({s})"), k, trig)],
            "csc" => vec![same(format!("cot({s})"), -k, trig)],
            "sin" => vec![
                same(format!("({s})"), k / 2.0, Scale::None),
                same(format!("sin(2({s}))"), -k / 4.0, trig),
            ],
            "cos" => vec![
                same(format!("({s})"), k / 2.0, Scale::None),
                same(format!("sin(2({s}))"), k / 4.0, trig),
            ],
            "tan" => vec![
                same(format!("tan({s})"), k, trig),
                same(format!("({s})"), -k, Scale::None),
            ],
            _ => return None,
        };
        self.note(1, format!("{form}{}", inner_note(u)));
        Some(terms)
    }

    /// ∫ u^n dx.
    fn power(&self, u: &T, n: f64) -> Option<Vec<Term>> {
        let a = self.slope(u).filter(|a| *a != 0.0)?;
        let s = text(u);
        let bare = u.sym_name().is_some();
        if n == -1.0 {
            self.note(1, format!("∫ 1/u du = ln|u|{}", inner_note(u)));
            return Some(vec![term(
                1.0 / a,
                Scale::None,
                format!("ln|{s}|"),
                format!("ln(abs({s}))"),
            )]);
        }
        let m = n + 1.0;
        let base = if bare { s } else { format!("({s})") };
        let exponent = crate::symbolic::format_number(m);
        let shown = if m == 1.0 {
            base
        } else if m < 0.0 || m.fract() != 0.0 {
            format!("{base}^({exponent})")
        } else {
            format!("{base}^{exponent}")
        };
        self.note(
            1,
            format!("power rule: ∫ uⁿ du = uⁿ⁺¹/(n + 1){}", inner_note(u)),
        );
        Some(vec![same(shown, 1.0 / (m * a), Scale::None)])
    }

    /// ∫ c/b dx, for a constant c already taken out.
    fn reciprocal(&self, b: &T) -> Option<Vec<Term>> {
        let args = b.args();
        match &**b {
            N::Sym(_) => self.power(b, -1.0),
            N::Op(Op::Add | Op::Subtract, _) if self.slope(b).is_some() => self.power(b, -1.0),
            N::Op(Op::Multiply, _) if self.slope(b).is_some() => self.power(b, -1.0),
            N::Op(Op::Pow, _) => {
                let n = args[1].const_value()?;
                if let (2.0, N::Call(name, inner)) = (n, &*args[0]) {
                    let square = match name.as_str() {
                        "cos" => "sec",
                        "sin" => "csc",
                        _ => return self.power(&args[0], -n),
                    };
                    return self.squared(square, &inner[0]);
                }
                self.power(&args[0], -n)
            }
            N::Call(name, inner) => match name.as_str() {
                "cos" => self.function("sec", &inner[0]),
                "sin" => self.function("csc", &inner[0]),
                "tan" => self.function("cot", &inner[0]),
                "sqrt" => self
                    .root_of_difference(&inner[0])
                    .or_else(|| self.power(&inner[0], -0.5)),
                _ => None,
            },
            _ => None,
        }
        .or_else(|| self.sum_of_squares(b))
    }

    /// ∫ 1/(k + x^2) dx.
    fn sum_of_squares(&self, b: &T) -> Option<Vec<Term>> {
        if !b.op_is(Op::Add) {
            return None;
        }
        let args = b.args();
        let (k, square) = match (args[0].const_value(), args[1].const_value()) {
            (Some(k), None) => (k, &args[1]),
            (None, Some(k)) => (k, &args[0]),
            _ => return None,
        };
        if k <= 0.0 || !self.is_variable_squared(square) {
            return None;
        }
        let root = k.sqrt();
        let inner = self.over_root(root);
        self.note(1, "∫ du/(a² + u²) = atan(u/a)/a".into());
        Some(vec![same(
            format!("atan({inner})"),
            1.0 / root,
            self.inverse(),
        )])
    }

    /// ∫ 1/√(k − x^2) dx.
    fn root_of_difference(&self, inside: &T) -> Option<Vec<Term>> {
        if !inside.op_is(Op::Subtract) {
            return None;
        }
        let args = inside.args();
        let k = args[0].const_value().filter(|k| *k > 0.0)?;
        if !self.is_variable_squared(&args[1]) {
            return None;
        }
        let inner = self.over_root(k.sqrt());
        self.note(1, "∫ du/√(a² − u²) = asin(u/a)".into());
        Some(vec![same(format!("asin({inner})"), 1.0, self.inverse())])
    }

    fn is_variable_squared(&self, t: &T) -> bool {
        t.op_is(Op::Pow)
            && t.args()[0].sym_name() == Some(self.v)
            && t.args()[1].const_value() == Some(2.0)
    }

    fn over_root(&self, root: f64) -> String {
        if root == 1.0 {
            self.v.to_string()
        } else {
            format!("{}/{}", self.v, crate::symbolic::format_number(root))
        }
    }

    /// `integrate_inner`, keeping notes only when it answers.
    fn integrate(&self, t: &T) -> Option<Vec<Term>> {
        let mark = self.notes.borrow().len();
        let out = self.integrate_inner(t);
        if out.is_none() {
            self.notes.borrow_mut().truncate(mark);
        }
        out
    }

    fn integrate_inner(&self, t: &T) -> Option<Vec<Term>> {
        let args = t.args();
        if free_of(t, self.v) {
            self.note(0, "constant rule: ∫ k dx = k x".into());
            return Some(self.scaled(vec![same(self.v.to_string(), 1.0, Scale::None)], t));
        }
        match &**t {
            N::Sym(_) => self.power(t, 1.0),
            N::Op(Op::Add, _) => {
                let mut all = self.integrate(&args[0])?;
                all.extend(self.integrate(&args[1])?);
                Some(all)
            }
            N::Op(Op::Subtract, _) => {
                let mut all = self.integrate(&args[0])?;
                let mut rest = self.integrate(&args[1])?;
                for r in &mut rest {
                    r.coef = -r.coef;
                }
                all.append(&mut rest);
                Some(all)
            }
            N::Op(Op::UnaryMinus, _) => {
                let mut all = self.integrate(&args[0])?;
                for r in &mut all {
                    r.coef = -r.coef;
                }
                Some(all)
            }
            N::Op(Op::UnaryPlus, _) => self.integrate(&args[0]),
            N::Op(Op::Multiply, _) => {
                if free_of(&args[0], self.v) {
                    return Some(self.scaled(self.integrate(&args[1])?, &args[0]));
                }
                if free_of(&args[1], self.v) {
                    return Some(self.scaled(self.integrate(&args[0])?, &args[1]));
                }
                self.secant_tangent(&args[0], &args[1])
                    .or_else(|| self.product(t))
            }
            N::Op(Op::Divide, _) => {
                if free_of(&args[1], self.v) {
                    let terms = self.integrate(&args[0])?;
                    return Some(match args[1].const_value() {
                        Some(n) => terms
                            .into_iter()
                            .map(|mut x| {
                                x.coef /= n;
                                x
                            })
                            .collect(),
                        None => terms
                            .into_iter()
                            .map(|mut x| {
                                x.factor.push(format!("1/({})", text(&args[1])));
                                x
                            })
                            .collect(),
                    });
                }
                if free_of(&args[0], self.v)
                    && let Some(r) = self.reciprocal(&args[1])
                {
                    return Some(self.scaled(r, &args[0]));
                }
                self.product(t)
            }
            N::Op(Op::Pow, _) => {
                let (base, exponent) = (&args[0], &args[1]);
                if let Some(n) = exponent.const_value() {
                    if let (2.0, N::Call(name, inner)) = (n, &**base) {
                        return self.squared(name, &inner[0]);
                    }
                    return self.power(base, n);
                }
                // c^u
                let a = self.slope(exponent).filter(|a| *a != 0.0)?;
                let c = match &**base {
                    N::Sym(s) if s == "e" => E,
                    _ => base.const_value().filter(|c| *c > 0.0 && *c != 1.0)?,
                };
                let shown = format!("{}^({})", text(base), text(exponent));
                let form = if c == E {
                    "∫ eᵘ du = eᵘ"
                } else {
                    "∫ aᵘ du = aᵘ/ln a"
                };
                self.note(1, format!("{form}{}", inner_note(exponent)));
                Some(vec![same(shown, 1.0 / (a * jsmath::ln(c)), Scale::None)])
            }
            N::Call(name, inner) if inner.len() == 1 => self.function(name, &inner[0]),
            _ => None,
        }
    }

    /// A product (or quotient) of parts that each depend on the variable: by parts, or by substitution.
    fn product(&self, t: &T) -> Option<Vec<Term>> {
        if self.deg && has_angle(t) {
            return None;
        }
        let mut coef = 1.0;
        let mut letters = Vec::new();
        let mut parts = Vec::new();
        self.factors(t, false, &mut coef, &mut letters, &mut parts);
        let terms = match parts.as_slice() {
            [] => return None,
            [one] => self.integrate(one)?,
            _ => {
                let mark = self.notes.borrow().len();
                let found = self.by_parts(&parts).or_else(|| {
                    self.notes.borrow_mut().truncate(mark);
                    self.substitution(&parts)
                });
                if found.is_none() {
                    self.notes.borrow_mut().truncate(mark);
                }
                found?
            }
        };
        let mut terms: Vec<Term> = terms
            .into_iter()
            .map(|mut x| {
                x.coef *= coef;
                x
            })
            .collect();
        for l in &letters {
            terms = self.scaled(terms, l);
        }
        Some(terms)
    }

    /// The factors of a product, quotients turned to negative powers; numbers and constant letters apart.
    fn factors(&self, t: &T, invert: bool, coef: &mut f64, letters: &mut Vec<T>, out: &mut Vec<T>) {
        let args = t.args();
        match &**t {
            N::Op(Op::Multiply, _) => {
                for a in args {
                    self.factors(a, invert, coef, letters, out);
                }
            }
            N::Op(Op::Divide, _) => {
                self.factors(&args[0], invert, coef, letters, out);
                self.factors(&args[1], !invert, coef, letters, out);
            }
            N::Op(Op::UnaryMinus, _) => {
                *coef = -*coef;
                self.factors(&args[0], invert, coef, letters, out);
            }
            N::Op(Op::UnaryPlus, _) => self.factors(&args[0], invert, coef, letters, out),
            _ => {
                if let Some(c) = t.const_value() {
                    if invert {
                        *coef /= c;
                    } else {
                        *coef *= c;
                    }
                } else if free_of(t, self.v) {
                    letters.push(if invert {
                        tree::op(Op::Divide, vec![tree::constant(1.0), t.clone()])
                    } else {
                        t.clone()
                    });
                } else if !invert {
                    out.push(t.clone());
                } else if let (true, Some(n)) = (
                    t.op_is(Op::Pow),
                    t.args().get(1).and_then(|e| e.const_value()),
                ) {
                    out.push(tree::op(
                        Op::Pow,
                        vec![t.args()[0].clone(), tree::constant(-n)],
                    ));
                } else {
                    out.push(tree::op(Op::Pow, vec![t.clone(), tree::constant(-1.0)]));
                }
            }
        }
    }

    /// ∫ f(g) g' dx = F(g): one factor is a function of some g, and the rest is a number times g'.
    fn substitution(&self, parts: &[T]) -> Option<Vec<Term>> {
        let u = tree::symbol(PLACE);
        for (i, f) in parts.iter().enumerate() {
            let mut tries: Vec<(T, T)> = vec![(f.clone(), u.clone())];
            let a = f.args();
            match &**f {
                N::Call(name, inner) if inner.len() == 1 => {
                    tries.push((inner[0].clone(), tree::call(name, vec![u.clone()])));
                }
                N::Op(Op::Pow, _) => {
                    if a[1].const_value().is_some() {
                        tries.push((
                            a[0].clone(),
                            tree::op(Op::Pow, vec![u.clone(), a[1].clone()]),
                        ));
                        if let N::Call(name, inner) = &*a[0]
                            && inner.len() == 1
                        {
                            let outer = tree::call(name, vec![u.clone()]);
                            tries.push((
                                inner[0].clone(),
                                tree::op(Op::Pow, vec![outer, a[1].clone()]),
                            ));
                        }
                    } else if free_of(&a[0], self.v) {
                        tries.push((
                            a[1].clone(),
                            tree::op(Op::Pow, vec![a[0].clone(), u.clone()]),
                        ));
                    }
                }
                _ => {}
            }
            let others: Vec<T> = parts
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p.clone())
                .collect();
            let rest = others
                .into_iter()
                .reduce(|l, r| tree::op(Op::Multiply, vec![l, r]))?;
            for (g, outer) in tries {
                if self.slope(&g).is_some() || (self.deg && has_angle(&g)) {
                    continue;
                }
                let Ok(dg) = tree::derivative(&g, self.v) else {
                    continue;
                };
                let Some(c) = constant_ratio(&rest, &dg, self.v) else {
                    continue;
                };
                let inner = Table {
                    v: PLACE,
                    deg: self.deg,
                    notes: self.notes,
                };
                let Some(terms) = inner.integrate(&outer) else {
                    continue;
                };
                self.note(3, format!("substitute u = {}", compact(&g)));
                let eval_g = format!("({})", text(&g));
                return Some(
                    terms
                        .into_iter()
                        .map(|mut x| {
                            x.coef *= c;
                            x.show = put_back(&x.show, &g);
                            x.eval = x.eval.replace(PLACE, &eval_g);
                            x
                        })
                        .collect(),
                );
            }
        }
        None
    }

    /// `n` when `f` is the variable to a number.
    fn monomial(&self, f: &T) -> Option<f64> {
        match &**f {
            N::Sym(s) if s == self.v => Some(1.0),
            N::Op(Op::Pow, a) if a[0].sym_name() == Some(self.v) => a[1].const_value(),
            _ => None,
        }
    }

    /// The variable to `m`, as the answer writes it.
    fn power_text(&self, m: f64) -> String {
        let v = self.v;
        if m == 1.0 {
            v.to_string()
        } else if m > 0.0 && m.fract() == 0.0 {
            format!("{v}^{}", crate::symbolic::format_number(m))
        } else {
            format!("{v}^({})", crate::symbolic::format_number(m))
        }
    }

    /// The `j`th antiderivative of e^u, c^u, sin(u) or cos(u) for a linear u: its number, shown, and evaluated.
    fn repeated(&self, f: &T, j: i32) -> Option<(f64, String)> {
        let a = f.args();
        let (base, u) = match &**f {
            N::Op(Op::Pow, _) if free_of(&a[0], self.v) => (a[0].clone(), a[1].clone()),
            N::Call(name, inner) if name == "exp" && inner.len() == 1 => {
                (tree::symbol("e"), inner[0].clone())
            }
            N::Call(name, inner) if (name == "sin" || name == "cos") && inner.len() == 1 => {
                let k = self.slope(&inner[0]).filter(|k| *k != 0.0)?;
                // sin → -cos → -sin → cos → sin, and cos is where that comes round to cos
                let start = if name == "sin" { 0 } else { 3 };
                let (sign, which) = match (start + j).rem_euclid(4) {
                    1 => (-1.0, "cos"),
                    2 => (-1.0, "sin"),
                    3 => (1.0, "cos"),
                    _ => (1.0, "sin"),
                };
                return Some((sign / k.powi(j), format!("{which}({})", compact(&inner[0]))));
            }
            _ => return None,
        };
        let k = self.slope(&u).filter(|k| *k != 0.0)?;
        // only e: 2^x would bring 1/ln(2) to every power, which reads as a long decimal
        if base.sym_name() != Some("e") {
            return None;
        }
        Some((1.0 / k.powi(j), format!("e^({})", compact(&u))))
    }

    /// Integration by parts for two factors: x^n times e^u, sin or cos (n a whole number), x^n ln(x), and
    /// e^u times sin or cos.
    fn by_parts(&self, parts: &[T]) -> Option<Vec<Term>> {
        let [l, r] = parts else {
            return None;
        };
        for (p, f) in [(l, r), (r, l)] {
            if let Some(n) = self.monomial(p) {
                if (1.0..=8.0).contains(&n) && n.fract() == 0.0 && self.repeated(f, 1).is_some() {
                    let n = n as i32;
                    let times = if n == 1 {
                        String::new()
                    } else {
                        format!(" {n} times")
                    };
                    self.note(
                        3,
                        format!(
                            "integrate by parts{times}: u = {}, dv = {} d{}",
                            self.power_text(f64::from(n)),
                            compact(f),
                            self.v
                        ),
                    );
                    let mut terms = Vec::new();
                    let mut falling = 1.0;
                    for k in 0..=n {
                        let (c, shown) = self.repeated(f, k + 1)?;
                        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                        let m = (n - k) as f64;
                        let (show, eval) = if m == 0.0 {
                            (shown.clone(), shown)
                        } else {
                            let x = self.power_text(m);
                            (format!("{x} {shown}"), format!("({x})*({shown})"))
                        };
                        terms.push(term(sign * falling * c, Scale::None, show, eval));
                        falling *= (n - k) as f64;
                    }
                    return Some(terms);
                }
                let is_ln = matches!(&**f, N::Call(name, a) if (name == "ln" || name == "log") && a.len() == 1 && a[0].sym_name() == Some(self.v));
                if is_ln && n != -1.0 {
                    let m = n + 1.0;
                    let x = self.power_text(m);
                    let v = self.v;
                    let dv = if n == 0.0 {
                        String::new()
                    } else {
                        format!("{} ", self.power_text(n))
                    };
                    self.note(3, format!("integrate by parts: u = ln {v}, dv = {dv}d{v}"));
                    return Some(vec![
                        term(
                            1.0 / m,
                            Scale::None,
                            format!("{x} ln({v})"),
                            format!("({x})*ln({v})"),
                        ),
                        term(-1.0 / (m * m), Scale::None, x.clone(), x),
                    ]);
                }
            }
            // e^u sin(w) and e^u cos(w)
            if let (Some((_, e)), N::Call(name, inner)) = (self.repeated(p, 0), &**f) {
                if p.call_name().is_some_and(|n| n != "exp") || inner.len() != 1 {
                    continue;
                }
                if name != "sin" && name != "cos" {
                    continue;
                }
                let a = 1.0 / self.repeated(p, 1)?.0;
                let b = self.slope(&inner[0]).filter(|b| *b != 0.0)?;
                let w = compact(&inner[0]);
                let d = a * a + b * b;
                let (first, second, sign) = if name == "sin" {
                    ("sin", "cos", -1.0)
                } else {
                    ("cos", "sin", 1.0)
                };
                let one = format!("{e} {first}({w})");
                let two = format!("{e} {second}({w})");
                self.note(
                    3,
                    format!(
                        "integrate by parts twice: u = {first}({w}), dv = {e} d{}, then solve for the integral",
                        self.v
                    ),
                );
                return Some(vec![
                    term(
                        a / d,
                        Scale::None,
                        one.clone(),
                        format!("({e})*{first}({w})"),
                    ),
                    term(
                        sign * b / d,
                        Scale::None,
                        two,
                        format!("({e})*{second}({w})"),
                    ),
                ]);
            }
        }
        None
    }

    /// sec(u) tan(u) and csc(u) cot(u).
    fn secant_tangent(&self, l: &T, r: &T) -> Option<Vec<Term>> {
        let (N::Call(a, x), N::Call(b, y)) = (&**l, &**r) else {
            return None;
        };
        if x.len() != 1 || y.len() != 1 || text(&x[0]) != text(&y[0]) {
            return None;
        }
        let u = &x[0];
        let k = 1.0 / self.slope(u).filter(|s| *s != 0.0)?;
        let s = text(u);
        match (a.as_str(), b.as_str()) {
            ("sec", "tan") | ("tan", "sec") => {
                self.note(1, format!("∫ sec u tan u du = sec u{}", inner_note(u)));
                Some(vec![same(format!("sec({s})"), k, self.trig())])
            }
            ("csc", "cot") | ("cot", "csc") => {
                self.note(1, format!("∫ csc u cot u du = −csc u{}", inner_note(u)));
                Some(vec![same(format!("csc({s})"), -k, self.trig())])
            }
            _ => None,
        }
    }
}

/// The letter that stands for the inner part while a substitution is integrated.
const PLACE: &str = "qcalcu";

/// `u` as a factor is written: bare when it's one letter or a call, else in parentheses.
fn wrap(u: &T) -> String {
    match &**u {
        N::Sym(_) | N::Call(..) => text(u),
        N::Const(c) if *c >= 0.0 => text(u),
        _ => format!("({})", text(u)),
    }
}

/// A function of an angle, which in degrees means something the tree's evaluation doesn't.
fn has_angle(t: &T) -> bool {
    match &**t {
        N::Call(name, a) => {
            matches!(
                name.as_str(),
                "sin"
                    | "cos"
                    | "tan"
                    | "sec"
                    | "csc"
                    | "cot"
                    | "asin"
                    | "acos"
                    | "atan"
                    | "asec"
                    | "acsc"
                    | "acot"
            ) || a.iter().any(has_angle)
        }
        N::Op(_, a) => a.iter().any(has_angle),
        _ => false,
    }
}

/// `PLACE` in a shown term put back as `g`: bare where it already sits in parentheses or is atomic.
fn put_back(show: &str, g: &T) -> String {
    let bare = compact(g);
    let atomic = wrap(g) == bare;
    let mut out = String::new();
    let mut rest = show;
    while let Some(i) = rest.find(PLACE) {
        let before = &rest[..i];
        let after = &rest[i + PLACE.len()..];
        let enclosed = before.ends_with(['(', '|']) && after.starts_with([')', '|']);
        out.push_str(before);
        if atomic || enclosed {
            out.push_str(&bare);
        } else {
            out.push_str(&format!("({bare})"));
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `num / den` when it's the same number wherever both are defined.
fn constant_ratio(num: &T, den: &T, v: &str) -> Option<f64> {
    let mut seen: Option<f64> = None;
    let mut agreed = 0;
    for x in [0.37, 0.61, 1.13, 1.71, 2.39, 3.3, 4.7] {
        let scope = [(v.to_string(), x)];
        let (Some(Some(n)), Some(Some(d))) =
            (tree::evaluate(num, &scope), tree::evaluate(den, &scope))
        else {
            continue;
        };
        if !n.is_finite() || !d.is_finite() || d.abs() < 1e-9 {
            continue;
        }
        let r = n / d;
        match seen {
            None => seen = Some(r),
            Some(s) if (r - s).abs() <= 1e-9 * s.abs().max(1.0) => {}
            Some(_) => return None,
        }
        agreed += 1;
    }
    let r = seen.filter(|r| *r != 0.0 && agreed >= 3)?;
    Some(match fraction(r) {
        Some((p, q)) => p as f64 / q as f64,
        None => r,
    })
}

/// A coefficient as the answer writes it: `1/3`, `2`, or the decimal when it's neither.
fn fraction(c: f64) -> Option<(i64, i64)> {
    for q in 1..=64_i64 {
        let p = c * q as f64;
        if (p - p.round()).abs() < 1e-9 && p.abs() < 1e9 {
            return Some((p.round() as i64, q));
        }
    }
    None
}

fn single_letter(show: &str) -> bool {
    let mut chars = show.chars();
    chars.next().is_some_and(char::is_alphabetic) && !chars.next().is_some_and(char::is_alphabetic)
}

fn write(terms: &[Term]) -> Antiderivative {
    let mut show = String::new();
    let mut eval = String::new();
    for (i, t) in terms.iter().enumerate() {
        let negative = t.coef < 0.0;
        if i == 0 {
            if negative {
                show.push('-');
            }
        } else {
            show.push_str(if negative { " - " } else { " + " });
        }
        let size = t.coef.abs();
        let lead = match t.scale {
            Scale::None => "",
            Scale::Trig => "(180/π) ",
            Scale::Inverse => "(π/180) ",
        };
        let factors = if t.factor.is_empty() {
            String::new()
        } else {
            format!("{} ", t.factor.join(" "))
        };
        let body = match fraction(size) {
            Some((1, 1)) => format!("{lead}{factors}{}", t.show),
            // 2x, but 2 ln|x|
            Some((p, 1)) if lead.is_empty() && factors.is_empty() && single_letter(&t.show) => {
                format!("{p}{}", t.show)
            }
            Some((p, 1)) => format!("{p} {lead}{factors}{}", t.show),
            Some((1, q)) => format!("{lead}{factors}{}/{q}", t.show),
            Some((p, q)) => format!("{p} {lead}{factors}{}/{q}", t.show),
            None => format!(
                "{} {lead}{factors}{}",
                crate::symbolic::format_number(size),
                t.show
            ),
        };
        show.push_str(&body);
        if i > 0 {
            eval.push('+');
        }
        let mut product = format!("({})", crate::symbolic::format_number(t.coef));
        match t.scale {
            Scale::None => {}
            Scale::Trig => product.push_str(&format!("*({})", 180.0 / PI)),
            Scale::Inverse => product.push_str(&format!("*({})", PI / 180.0)),
        }
        for f in &t.factor {
            product.push_str(&format!("*({f})"));
        }
        eval.push_str(&format!("{product}*({})", t.eval));
    }
    Antiderivative { show, eval }
}

/// The antiderivative from the table, unchecked; the caller checks it against the integrand.
pub fn antiderivative(body: &T, v: &str, deg: bool) -> Option<Antiderivative> {
    antiderivative_noted(body, v, deg).map(|(found, _)| found)
}

/// native only: `antiderivative` and how it was found, for the working.
pub fn antiderivative_noted(body: &T, v: &str, deg: bool) -> Option<(Antiderivative, Vec<Note>)> {
    let notes = RefCell::new(Vec::new());
    let terms = Table {
        v,
        deg,
        notes: &notes,
    }
    .integrate(body)?;
    (!terms.is_empty()).then(|| (write(&terms), notes.into_inner()))
}
