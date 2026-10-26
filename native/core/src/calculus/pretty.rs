//! Typesetting derivatives (`pretty` in `calculus.ts`): `2x sin(x) + x² cos(x)`, still text the engine reads back.

use super::tree::{N, T, constant, op, text};
use crate::jsnum;
use crate::symbolic::Op;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Num,
    Sym,
    Call,
    Group,
    Other,
}

#[derive(Clone, Debug)]
pub struct Printed {
    pub s: String,
    pub prec: u8,
    pub kind: Kind,
}

pub const SUM: u8 = 1;
pub const PRODUCT: u8 = 2;
pub const UNARY: u8 = 3;
pub const POWER: u8 = 4;
pub const ATOM: u8 = 5;

fn p(s: String, prec: u8, kind: Kind) -> Printed {
    Printed { s, prec, kind }
}

/// `String(Number(v.toPrecision(12)))`.
pub fn number_text(v: f64) -> String {
    jsnum::to_string(jsnum::parse(&jsnum::to_precision(v, 12)).unwrap_or(f64::NAN))
}

/// `-x` without the minus, when the node has one to lose.
pub fn negate(n: &T) -> Option<T> {
    match &**n {
        N::Op(Op::UnaryMinus, a) => Some(a[0].clone()),
        N::Const(v) if *v < 0.0 => Some(constant(-v)),
        N::Op(o @ (Op::Multiply | Op::Divide), a) => {
            let first = negate(&a[0])?;
            let mut args = vec![first];
            args.extend(a[1..].iter().cloned());
            Some(op(*o, args))
        }
        _ => None,
    }
}

fn wrap(x: &Printed, min: u8) -> String {
    if x.prec < min { format!("({})", x.s) } else { x.s.clone() }
}

fn factors(n: &T, out: &mut Vec<T>) {
    if n.op_is(Op::Multiply) {
        for a in n.args() {
            factors(a, out);
        }
    } else {
        out.push(n.clone());
    }
}

pub fn has_symbol(n: &T) -> bool {
    match &**n {
        N::Sym(s) => s != "pi" && s != "e",
        _ => n.args().iter().any(has_symbol),
    }
}

/// `x` before `y`: letters and their powers in a product go in alphabetical order.
fn letter_order(a: &T, b: &T) -> std::cmp::Ordering {
    let name = |f: &T| -> String {
        match &**f {
            N::Sym(s) => s.clone(),
            _ => f.args()[0].sym_name().unwrap_or("").to_string(),
        }
    };
    // JS compares UTF-16 units; the letters here are all in the BMP, where that is code point order
    name(a).cmp(&name(b))
}

const SUPERSCRIPT: [(char, char); 11] = [
    ('0', '⁰'),
    ('1', '¹'),
    ('2', '²'),
    ('3', '³'),
    ('4', '⁴'),
    ('5', '⁵'),
    ('6', '⁶'),
    ('7', '⁷'),
    ('8', '⁸'),
    ('9', '⁹'),
    ('-', '⁻'),
];

fn superscript(s: &str) -> String {
    s.chars()
        .filter_map(|c| SUPERSCRIPT.iter().find(|(d, _)| *d == c).map(|(_, u)| *u))
        .collect()
}

fn is_integer(v: f64) -> bool {
    jsnum::is_integer(v)
}

pub fn pretty(n: &T) -> Printed {
    match &**n {
        N::Const(v) => p(number_text(*v), if *v < 0.0 { UNARY } else { ATOM }, Kind::Num),
        N::Sym(s) => p(
            match s.as_str() {
                "pi" => "π".into(),
                "theta" => "θ".into(),
                _ => s.clone(),
            },
            ATOM,
            Kind::Sym,
        ),
        N::Call(name, a) => {
            let args: Vec<Printed> = a.iter().map(pretty).collect();
            if name == "sqrt" || name == "cbrt" {
                let root = if name == "sqrt" { "√" } else { "∛" };
                let x = &args[0];
                let s = if x.prec == ATOM && x.kind != Kind::Call {
                    format!("{root}{}", x.s)
                } else {
                    format!("{root}({})", x.s)
                };
                return p(s, ATOM, Kind::Call);
            }
            if name == "abs" {
                return p(format!("|{}|", args[0].s), ATOM, Kind::Call);
            }
            if name == "exp" {
                let x = &args[0];
                let s = if x.prec == ATOM && x.kind != Kind::Call {
                    format!("e^{}", x.s)
                } else {
                    format!("e^({})", x.s)
                };
                return p(s, POWER, Kind::Call);
            }
            let shown = match name.as_str() {
                "log" => "ln",
                "log10" => "log",
                other => other,
            };
            let inner: Vec<&str> = args.iter().map(|x| x.s.as_str()).collect();
            p(format!("{shown}({})", inner.join(", ")), ATOM, Kind::Call)
        }
        N::Op(o, args) => operator(n, *o, args),
    }
}

fn operator(n: &T, o: Op, args: &[T]) -> Printed {
    match o {
        Op::UnaryMinus => {
            let a = pretty(&args[0]);
            p(format!("-{}", wrap(&a, PRODUCT)), UNARY, Kind::Other)
        }
        Op::Add | Op::Subtract => {
            let mut terms: Vec<(bool, T)> = Vec::new();
            fn collect(x: &T, neg: bool, terms: &mut Vec<(bool, T)>) {
                match &**x {
                    N::Op(Op::Add, a) => a.iter().for_each(|y| collect(y, neg, terms)),
                    N::Op(Op::Subtract, a) => {
                        collect(&a[0], neg, terms);
                        collect(&a[1], !neg, terms);
                    }
                    _ => match negate(x) {
                        Some(f) => terms.push((!neg, f)),
                        None => terms.push((neg, x.clone())),
                    },
                }
            }
            collect(n, false, &mut terms);
            let mut s = String::new();
            for (i, (neg, t)) in terms.iter().enumerate() {
                let body = wrap(&pretty(t), PRODUCT);
                s.push_str(&match (i, neg) {
                    (0, true) => format!("-{body}"),
                    (0, false) => body,
                    (_, true) => format!(" - {body}"),
                    (_, false) => format!(" + {body}"),
                });
            }
            p(s, SUM, Kind::Other)
        }
        Op::Multiply => {
            let mut fs = Vec::new();
            factors(n, &mut fs);
            let mut coeff = 1.0;
            let mut rest: Vec<T> = Vec::new();
            for f0 in fs {
                let mut f = f0;
                while f.op_is(Op::UnaryMinus) {
                    coeff = -coeff;
                    f = f.args()[0].clone();
                }
                match f.const_value() {
                    Some(v) => coeff *= v,
                    None => rest.push(f),
                }
            }
            // π, then letters and their powers, then the rest, with constant calls like ln(2) last
            let rank = |f: &T| -> u8 {
                match &**f {
                    N::Sym(s) => u8::from(s != "pi"),
                    N::Op(Op::Pow, a) if a[0].sym_name().is_some_and(|s| s != "e") => 1,
                    N::Call(_, a) if !a.iter().any(has_symbol) => 3,
                    _ => 2,
                }
            };
            let mut idx: Vec<usize> = (0..rest.len()).collect();
            idx.sort_by(|&a, &b| {
                let (ra, rb) = (rank(&rest[a]), rank(&rest[b]));
                ra.cmp(&rb)
                    .then_with(|| {
                        if ra == 1 {
                            letter_order(&rest[a], &rest[b])
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .then(a.cmp(&b))
            });
            let parts: Vec<Printed> = idx.iter().map(|&i| pretty(&rest[i])).collect();
            let mut s = String::new();
            let mut prev: Option<Kind> = None;
            let abs = coeff.abs();
            if abs != 1.0 || parts.is_empty() {
                s = number_text(abs);
                prev = Some(Kind::Num);
            }
            for x in &parts {
                let t = if x.prec < POWER && x.kind != Kind::Call { format!("({})", x.s) } else { x.s.clone() };
                let kind = if t.starts_with('(') { Kind::Group } else { x.kind };
                let one_letter = {
                    let mut cs = t.chars();
                    match (cs.next(), cs.next()) {
                        (Some(c), None) => {
                            c.is_ascii_alphabetic()
                                || c == 'π'
                                || ('α'..='ω').contains(&c)
                                || ('Α'..='Ω').contains(&c)
                        }
                        _ => false,
                    }
                };
                // `2x`, `2(x + 1)`, `2√x` and `πx` sit tight; everything else is spaced
                let tight = (prev == Some(Kind::Num)
                    && (kind == Kind::Sym || kind == Kind::Group || t.starts_with('√') || t.starts_with('∛')))
                    || (prev == Some(Kind::Sym) && one_letter);
                // `x (x + 1)` would read back as a call to x
                let sep = if prev == Some(Kind::Sym) && kind == Kind::Group { "·" } else { " " };
                if prev.is_none() || tight {
                    s.push_str(&t);
                } else {
                    s.push_str(sep);
                    s.push_str(&t);
                }
                prev = Some(kind);
            }
            if coeff < 0.0 {
                p(format!("-{s}"), UNARY, Kind::Other)
            } else {
                p(s, PRODUCT, if prev == Some(Kind::Num) { Kind::Num } else { Kind::Other })
            }
        }
        Op::Divide => {
            let mut top = args[0].clone();
            let mut bottoms = vec![args[1].clone()];
            while top.op_is(Op::Divide) {
                bottoms.insert(0, top.args()[1].clone());
                top = top.args()[0].clone();
            }
            let flipped = negate(&top);
            let numer = pretty(flipped.as_ref().unwrap_or(&top));
            let den = pretty(&if bottoms.len() > 1 { op(Op::Multiply, bottoms) } else { bottoms.remove(0) });
            let den_s = if den.prec <= PRODUCT || (den.kind == Kind::Num && den.s.starts_with('-')) {
                format!("({})", den.s)
            } else {
                den.s
            };
            let s = format!("{}/{den_s}", wrap(&numer, PRODUCT));
            if flipped.is_some() { p(format!("-{s}"), UNARY, Kind::Other) } else { p(s, PRODUCT, Kind::Other) }
        }
        Op::Pow => {
            let base = pretty(&args[0]);
            let ex = &args[1];
            let base_text = if base.prec <= POWER || (base.kind == Kind::Num && base.s.starts_with('-')) {
                format!("({})", base.s)
            } else {
                base.s.clone()
            };
            let int_ex = match &**ex {
                N::Const(v) if is_integer(*v) => Some(*v),
                N::Op(Op::UnaryMinus, a) => a[0].const_value().filter(|v| is_integer(*v)).map(|v| -v),
                _ => None,
            };
            let kind = if base.kind == Kind::Sym && base.s != "e" { Kind::Sym } else { Kind::Call };
            if let Some(k) = int_ex
                && k.abs() < 1000.0
            {
                return p(format!("{base_text}{}", superscript(&jsnum::to_string(k))), POWER, kind);
            }
            let e = pretty(ex);
            let es = if e.prec == ATOM && e.kind != Kind::Call { e.s } else { format!("({})", e.s) };
            p(format!("{base_text}^{es}"), POWER, kind)
        }
        _ => p(text(n), SUM, Kind::Other),
    }
}
