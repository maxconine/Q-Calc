//! The expression trees `calculus.ts` builds from mathjs nodes, with mathjs's object identity.
//!
//! `calculus.ts` inlines a user function by substituting the *same* converted argument node for every use of a
//! parameter, so `f(t) = t*t` becomes a product whose two factors are one object. mathjs's `derivative` picks the
//! factor its product rule differentiates by `===`, so for such a product it differentiates both factors in both
//! terms (`d/dx (x*x)` is `1*1 + 1*1`). The TS engine then finds the derivative disagrees with finite differences
//! and leaves the line blank. To give the same answers this port keeps nodes behind [`Rc`] and compares them with
//! [`Rc::ptr_eq`] where mathjs compares with `===`; [`derivative`] is mathjs's rules (as `symbolic::derivative`
//! ports them) over these trees. mathjs's `clone()` is shallow, so reusing an `Rc` where mathjs clones changes no
//! identity this module can observe: no rule puts a clone beside its original in one product.

use std::rc::Rc;

use crate::eval::complex::C;
use crate::scientific::functions::jsmath;
use crate::symbolic::{self, Node, Op, PrintOptions};
use crate::syntax::Constant;

pub type T = Rc<N>;

#[derive(Clone, Debug)]
pub enum N {
    Const(f64),
    Sym(String),
    Op(Op, Vec<T>),
    Call(String, Vec<T>),
}

pub fn constant(v: f64) -> T {
    Rc::new(N::Const(v))
}

pub fn symbol(name: &str) -> T {
    Rc::new(N::Sym(name.to_string()))
}

pub fn op(o: Op, args: Vec<T>) -> T {
    Rc::new(N::Op(o, args))
}

pub fn call(name: &str, args: Vec<T>) -> T {
    Rc::new(N::Call(name.to_string(), args))
}

impl N {
    pub fn args(&self) -> &[T] {
        match self {
            N::Op(_, a) | N::Call(_, a) => a,
            _ => &[],
        }
    }

    pub fn op_is(&self, o: Op) -> bool {
        matches!(self, N::Op(x, _) if *x == o)
    }

    pub fn sym_name(&self) -> Option<&str> {
        match self {
            N::Sym(s) => Some(s),
            _ => None,
        }
    }

    pub fn const_value(&self) -> Option<f64> {
        match self {
            N::Const(v) => Some(*v),
            _ => None,
        }
    }

    pub fn call_name(&self) -> Option<&str> {
        match self {
            N::Call(n, _) => Some(n),
            _ => None,
        }
    }
}

/// The tree as a `symbolic` node, for `simplify` and `toString`.
pub fn to_node(n: &T) -> Node {
    match &**n {
        N::Const(v) => symbolic::num(*v),
        N::Sym(s) => symbolic::sym(s),
        N::Op(o, a) => symbolic::op(*o, a.iter().map(to_node).collect()),
        N::Call(name, a) => symbolic::call(name, a.iter().map(to_node).collect()),
    }
}

/// A `symbolic` node (what `simplify` gave) back as a tree; parentheses are dropped as `show` drops them.
pub fn from_node(n: &Node) -> Option<T> {
    Some(match n {
        Node::Constant(Constant::Number { value, .. }) => constant(*value),
        Node::Symbol(s) => symbol(s),
        Node::Parenthesis(c) => return from_node(c),
        Node::Operator { op: o, args, .. } => {
            op(*o, args.iter().map(from_node).collect::<Option<_>>()?)
        }
        Node::Function { args, .. } => call(
            symbolic::function_name(n),
            args.iter().map(from_node).collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

/// `node.toString()`.
pub fn text(n: &T) -> String {
    symbolic::to_string(&to_node(n), &PrintOptions::default())
}

/// `node.toString({parenthesis: 'auto'})`.
pub fn text_auto(n: &T) -> String {
    symbolic::to_string(
        &to_node(n),
        &PrintOptions {
            parenthesis: Some(symbolic::Parenthesis::Auto),
            implicit: None,
        },
    )
}

// ---- derivative

#[derive(Debug)]
pub struct DerivError;

type DR = Result<T, DerivError>;

struct Deriv<'a> {
    var: &'a str,
}

fn is_zero_const(n: &T) -> bool {
    n.const_value() == Some(0.0)
}

fn is_one_const(n: &T) -> bool {
    crate::symbolic::simplify::is_one_node(&to_node(n))
}

/// `math.derivative(tree, variable, {simplify: false})`.
pub fn derivative(tree: &T, var: &str) -> DR {
    Deriv { var }.derive(tree)
}

impl Deriv<'_> {
    fn is_const(&self, n: &T) -> bool {
        match &**n {
            N::Const(_) => true,
            N::Sym(s) => s != self.var,
            N::Op(_, a) | N::Call(_, a) => a.iter().all(|x| self.is_const(x)),
        }
    }

    fn derive(&self, n: &T) -> DR {
        match &**n {
            N::Const(_) => Ok(constant(0.0)),
            N::Sym(_) => Ok(constant(if self.is_const(n) { 0.0 } else { 1.0 })),
            N::Call(name, args) => self.function(n, name, args),
            N::Op(o, args) => self.operator(n, *o, args),
        }
    }

    fn function(&self, node: &T, name: &str, args: &[T]) -> DR {
        if self.is_const(node) {
            return Ok(constant(0.0));
        }
        let arg0 = args.first().cloned().unwrap_or_else(|| constant(0.0));
        let sq = |a: &T| op(Op::Pow, vec![a.clone(), constant(2.0)]);
        let mul = |a: T, b: T| op(Op::Multiply, vec![a, b]);
        let add = |a: T, b: T| op(Op::Add, vec![a, b]);
        let sub = |a: T, b: T| op(Op::Subtract, vec![a, b]);
        let one = || constant(1.0);
        let c1 = |f: &str| call(f, vec![arg0.clone()]);
        let mut div_out = false;
        let mut negative = false;
        let func: T = match (name, args.len()) {
            ("cbrt", _) => {
                div_out = true;
                mul(
                    constant(3.0),
                    op(
                        Op::Pow,
                        vec![
                            arg0.clone(),
                            op(Op::Divide, vec![constant(2.0), constant(3.0)]),
                        ],
                    ),
                )
            }
            ("sqrt" | "nthRoot", 1) => {
                div_out = true;
                mul(constant(2.0), c1("sqrt"))
            }
            ("sqrt" | "nthRoot", 2) => {
                let e = op(Op::Divide, vec![constant(1.0), args[1].clone()]);
                return self.derive(&op(Op::Pow, vec![arg0, e]));
            }
            ("log10" | "log", _) => {
                let ten = name == "log10";
                if !ten && args.len() == 1 {
                    div_out = true;
                    arg0.clone()
                } else if (args.len() == 1 && ten) || (args.len() == 2 && self.is_const(&args[1])) {
                    div_out = true;
                    let base = if ten { constant(10.0) } else { args[1].clone() };
                    mul(arg0.clone(), call("log", vec![base]))
                } else if args.len() == 2 {
                    return self.derive(&op(
                        Op::Divide,
                        vec![call("log", vec![arg0]), call("log", vec![args[1].clone()])],
                    ));
                } else {
                    return Err(DerivError);
                }
            }
            ("pow", 2) => return self.derive(&op(Op::Pow, vec![arg0, args[1].clone()])),
            ("exp", _) => c1("exp"),
            ("sin", _) => c1("cos"),
            ("cos", _) => op(Op::UnaryMinus, vec![c1("sin")]),
            ("tan", _) => sq(&c1("sec")),
            ("sec", _) => mul(node.clone(), c1("tan")),
            ("csc", _) => {
                negative = true;
                mul(node.clone(), c1("cot"))
            }
            ("cot", _) => {
                negative = true;
                sq(&c1("csc"))
            }
            ("asin", _) => {
                div_out = true;
                call("sqrt", vec![sub(one(), sq(&arg0))])
            }
            ("acos", _) => {
                div_out = true;
                negative = true;
                call("sqrt", vec![sub(one(), sq(&arg0))])
            }
            ("atan", _) => {
                div_out = true;
                add(sq(&arg0), one())
            }
            ("asec", _) => {
                div_out = true;
                mul(c1("abs"), call("sqrt", vec![sub(sq(&arg0), one())]))
            }
            ("acsc", _) => {
                div_out = true;
                negative = true;
                mul(c1("abs"), call("sqrt", vec![sub(sq(&arg0), one())]))
            }
            ("acot", _) => {
                div_out = true;
                negative = true;
                add(sq(&arg0), one())
            }
            ("sinh", _) => c1("cosh"),
            ("cosh", _) => c1("sinh"),
            ("tanh", _) => sq(&c1("sech")),
            ("sech", _) => {
                negative = true;
                mul(node.clone(), c1("tanh"))
            }
            ("csch", _) => {
                negative = true;
                mul(node.clone(), c1("coth"))
            }
            ("coth", _) => {
                negative = true;
                sq(&c1("csch"))
            }
            ("asinh", _) => {
                div_out = true;
                call("sqrt", vec![add(sq(&arg0), one())])
            }
            ("acosh", _) => {
                div_out = true;
                call("sqrt", vec![sub(sq(&arg0), one())])
            }
            ("atanh", _) => {
                div_out = true;
                sub(one(), sq(&arg0))
            }
            ("asech", _) => {
                div_out = true;
                negative = true;
                mul(arg0.clone(), call("sqrt", vec![sub(one(), sq(&arg0))]))
            }
            ("acsch", _) => {
                div_out = true;
                negative = true;
                mul(c1("abs"), call("sqrt", vec![add(sq(&arg0), one())]))
            }
            ("acoth", _) => {
                div_out = true;
                negative = true;
                sub(one(), sq(&arg0))
            }
            ("abs", _) => op(Op::Divide, vec![c1("abs"), arg0.clone()]),
            _ => return Err(DerivError),
        };
        let mut chain = self.derive(&arg0)?;
        if negative {
            chain = op(Op::UnaryMinus, vec![chain]);
        }
        Ok(op(
            if div_out { Op::Divide } else { Op::Multiply },
            vec![chain, func],
        ))
    }

    fn operator(&self, node: &T, o: Op, args: &[T]) -> DR {
        if self.is_const(node) {
            return Ok(constant(0.0));
        }
        let pow = |a: T, b: T| op(Op::Pow, vec![a, b]);
        let mul = |a: T, b: T| op(Op::Multiply, vec![a, b]);
        match (o, args.len()) {
            (Op::Add | Op::UnaryPlus, _) => Ok(op(
                o,
                args.iter()
                    .map(|a| self.derive(a))
                    .collect::<Result<_, _>>()?,
            )),
            (Op::UnaryMinus, 1) => Ok(op(o, vec![self.derive(&args[0])?])),
            (Op::Subtract, 2) => Ok(op(o, vec![self.derive(&args[0])?, self.derive(&args[1])?])),
            (Op::Multiply, _) => {
                let constant_terms: Vec<T> =
                    args.iter().filter(|a| self.is_const(a)).cloned().collect();
                if !constant_terms.is_empty() {
                    let varying: Vec<T> =
                        args.iter().filter(|a| !self.is_const(a)).cloned().collect();
                    let rest = if varying.len() == 1 {
                        varying[0].clone()
                    } else {
                        op(Op::Multiply, varying)
                    };
                    let mut out = constant_terms;
                    out.push(self.derive(&rest)?);
                    return Ok(op(Op::Multiply, out));
                }
                // product rule; mathjs tells the factor to differentiate by identity (`argInner === argOuter`)
                let mut terms = Vec::with_capacity(args.len());
                for outer in args {
                    let mut factors = Vec::with_capacity(args.len());
                    for inner in args {
                        factors.push(if Rc::ptr_eq(inner, outer) {
                            self.derive(inner)?
                        } else {
                            inner.clone()
                        });
                    }
                    terms.push(op(Op::Multiply, factors));
                }
                Ok(op(Op::Add, terms))
            }
            (Op::Divide, 2) => {
                let (a0, a1) = (&args[0], &args[1]);
                if self.is_const(a1) {
                    return Ok(op(Op::Divide, vec![self.derive(a0)?, a1.clone()]));
                }
                if self.is_const(a0) {
                    return Ok(mul(
                        op(Op::UnaryMinus, vec![a0.clone()]),
                        op(
                            Op::Divide,
                            vec![self.derive(a1)?, pow(a1.clone(), constant(2.0))],
                        ),
                    ));
                }
                Ok(op(
                    Op::Divide,
                    vec![
                        op(
                            Op::Subtract,
                            vec![
                                mul(self.derive(a0)?, a1.clone()),
                                mul(a0.clone(), self.derive(a1)?),
                            ],
                        ),
                        pow(a1.clone(), constant(2.0)),
                    ],
                ))
            }
            (Op::Pow, 2) => {
                let (base, ex) = (&args[0], &args[1]);
                if self.is_const(base) {
                    if base.const_value().is_some() && (is_zero_const(base) || is_one_const(base)) {
                        return Ok(constant(0.0));
                    }
                    return Ok(mul(
                        node.clone(),
                        mul(call("log", vec![base.clone()]), self.derive(ex)?),
                    ));
                }
                if self.is_const(ex) {
                    if ex.const_value().is_some() {
                        if is_zero_const(ex) {
                            return Ok(constant(0.0));
                        }
                        if is_one_const(ex) {
                            return self.derive(base);
                        }
                    }
                    let lowered = pow(
                        base.clone(),
                        op(Op::Subtract, vec![ex.clone(), constant(1.0)]),
                    );
                    return Ok(mul(ex.clone(), mul(self.derive(base)?, lowered)));
                }
                Ok(mul(
                    pow(base.clone(), ex.clone()),
                    op(
                        Op::Add,
                        vec![
                            mul(
                                self.derive(base)?,
                                op(Op::Divide, vec![ex.clone(), base.clone()]),
                            ),
                            mul(self.derive(ex)?, call("log", vec![base.clone()])),
                        ],
                    ),
                ))
            }
            _ => Err(DerivError),
        }
    }
}

// ---- evaluation with mathjs's own functions (`node.compile().evaluate(scope)` in `sameValue`)

#[derive(Clone, Copy)]
enum V {
    Num(f64),
    Cx(C),
}

/// The value of `n` with `scope`'s letters, as mathjs's number-mode `compile().evaluate()` gives it: Some(Some(x))
/// for a number, Some(None) for a non-number (a complex result), None for a throw.
pub fn evaluate(n: &T, scope: &[(String, f64)]) -> Option<Option<f64>> {
    Some(match eval(n, scope).ok()? {
        V::Num(x) => Some(x),
        V::Cx(_) => None,
    })
}

fn cx(re: f64) -> V {
    V::Cx(C::new(re, 0.0))
}

fn eval(n: &T, scope: &[(String, f64)]) -> Result<V, ()> {
    Ok(match &**n {
        N::Const(v) => V::Num(*v),
        N::Sym(s) => match scope.iter().find(|(k, _)| k == s) {
            Some((_, v)) => V::Num(*v),
            None => match s.as_str() {
                "pi" | "PI" => V::Num(std::f64::consts::PI),
                "e" | "E" => V::Num(std::f64::consts::E),
                _ => return Err(()),
            },
        },
        N::Op(o, args) => {
            let vals = args
                .iter()
                .map(|a| eval(a, scope))
                .collect::<Result<Vec<_>, _>>()?;
            let as_c = |v: V| match v {
                V::Num(x) => C::new(x, 0.0),
                V::Cx(c) => c,
            };
            match (o, vals.as_slice()) {
                (Op::UnaryMinus, [V::Num(a)]) => V::Num(-a),
                (Op::UnaryMinus, [a]) => V::Cx(as_c(*a).neg()),
                (Op::UnaryPlus, [a]) => *a,
                (Op::Add, [V::Num(a), V::Num(b)]) => V::Num(a + b),
                (Op::Subtract, [V::Num(a), V::Num(b)]) => V::Num(a - b),
                (Op::Multiply, [V::Num(a), V::Num(b)]) => V::Num(a * b),
                (Op::Divide, [V::Num(a), V::Num(b)]) => {
                    V::Num(if *b == 0.0 { f64::NAN } else { a / b })
                }
                (Op::Pow, [V::Num(a), V::Num(b)]) => V::Num(crate::eval::ops::real_pow(*a, *b)),
                (Op::Add, [a, b]) => V::Cx(as_c(*a).add(as_c(*b))),
                (Op::Subtract, [a, b]) => V::Cx(as_c(*a).sub(as_c(*b))),
                (Op::Multiply, [a, b]) => V::Cx(as_c(*a).mul(as_c(*b))),
                (Op::Divide, [a, b]) => V::Cx(as_c(*a).div(as_c(*b))),
                (Op::Pow, [a, b]) => V::Cx(as_c(*a).pow(as_c(*b))),
                // a product of more than two factors (built by the port, as mathjs builds them)
                (Op::Add | Op::Multiply, _) if vals.len() > 2 => {
                    let mut acc = vals[0];
                    for v in &vals[1..] {
                        acc = match (acc, v, o) {
                            (V::Num(a), V::Num(b), Op::Add) => V::Num(a + b),
                            (V::Num(a), V::Num(b), _) => V::Num(a * b),
                            (a, b, Op::Add) => V::Cx(as_c(a).add(as_c(*b))),
                            (a, b, _) => V::Cx(as_c(a).mul(as_c(*b))),
                        };
                    }
                    acc
                }
                _ => return Err(()),
            }
        }
        N::Call(name, args) => {
            let vals = args
                .iter()
                .map(|a| eval(a, scope))
                .collect::<Result<Vec<_>, _>>()?;
            let Some(&first) = vals.first() else {
                return Err(());
            };
            let x = match first {
                V::Num(x) => x,
                // abs of a complex number is a number; anything else of one stays complex
                V::Cx(c) => {
                    return Ok(if name == "abs" {
                        V::Num(c.abs())
                    } else {
                        V::Cx(C::new(f64::NAN, 0.0))
                    });
                }
            };
            let inv = |v: f64| 1.0 / v;
            match name.as_str() {
                "sin" => V::Num(jsmath::sin(x)),
                "cos" => V::Num(jsmath::cos(x)),
                "tan" => V::Num(jsmath::tan(x)),
                "sec" => V::Num(inv(jsmath::cos(x))),
                "csc" => V::Num(inv(jsmath::sin(x))),
                "cot" => V::Num(inv(jsmath::tan(x))),
                "asin" if (-1.0..=1.0).contains(&x) => V::Num(jsmath::asin(x)),
                "acos" if (-1.0..=1.0).contains(&x) => V::Num(jsmath::acos(x)),
                "asin" | "acos" => cx(f64::NAN),
                "atan" => V::Num(jsmath::atan(x)),
                "asec" if x.abs() >= 1.0 => V::Num(jsmath::acos(1.0 / x)),
                "acsc" if x.abs() >= 1.0 => V::Num(jsmath::asin(1.0 / x)),
                "asec" | "acsc" => cx(f64::NAN),
                "acot" => V::Num(jsmath::atan(1.0 / x)),
                "sinh" => V::Num(jsmath::sinh(x)),
                "cosh" => V::Num(jsmath::cosh(x)),
                "tanh" => V::Num(jsmath::tanh(x)),
                "sech" => V::Num(inv(jsmath::cosh(x))),
                "csch" => V::Num(inv(jsmath::sinh(x))),
                "coth" => V::Num(inv(jsmath::tanh(x))),
                "asinh" => V::Num(jsmath::asinh(x)),
                "acosh" if x >= 1.0 => V::Num(jsmath::acosh(x)),
                "atanh" if (-1.0..=1.0).contains(&x) => V::Num(jsmath::atanh(x)),
                "acosh" | "atanh" => cx(f64::NAN),
                "sqrt" | "nthRoot" if vals.len() == 1 => {
                    if x >= 0.0 {
                        V::Num(x.sqrt())
                    } else {
                        cx(f64::NAN)
                    }
                }
                "cbrt" => V::Num(jsmath::cbrt(x)),
                "exp" => V::Num(jsmath::exp(x)),
                "log" if vals.len() == 1 => {
                    if x >= 0.0 {
                        V::Num(jsmath::ln(x))
                    } else {
                        cx(f64::NAN)
                    }
                }
                "log10" => {
                    if x >= 0.0 {
                        V::Num(jsmath::log10(x))
                    } else {
                        cx(f64::NAN)
                    }
                }
                "abs" => V::Num(x.abs()),
                _ => return Err(()),
            }
        }
    })
}
