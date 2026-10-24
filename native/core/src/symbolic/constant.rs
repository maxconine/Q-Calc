//! mathjs `simplifyConstant` (`function/algebra/simplifyConstant.js`): folds constant subtrees, in exact
//! Fraction.js fractions when `exactFractions` holds, through the same functions the engine's `math` instance
//! has, including the engine's overrides (`scientific.ts` imports `divide` by 0 = NaN, `pow` taking odd roots of
//! negatives, and a non-negative `mod`, which replace mathjs's for `simplifyConstant` too).
//!
//! What a fold evaluates to follows mathjs's typed dispatch: Fraction with Fraction stays exact; a Fraction with
//! a double converts the double to a Fraction only when that is lossless (else the call throws and `_eval`
//! retries in doubles, turning the answer back into a fraction when it is one); a function without a Fraction
//! signature (`sqrt`, `sin`, …) throws on Fractions and is retried the same way; a non-real answer (`sqrt(-2)`,
//! `(-8)^(1/3)` through Fraction.js) is a Complex, which can't become a node, so simplify throws. A function
//! mathjs doesn't have (`ln`) stays a call.

use super::fraction::{Fraction, big_to_f64};
use super::simplify::{Ctx, SimplifyOptions, all_children, is_associative, make_node};
use super::{Constant, Node, Op, SymError, err, function_name, num, op};
use crate::eval::builtins::modulo;
use crate::eval::ops::real_pow;

/// What `foldFraction` returns: a node, or a value not yet turned back into one.
#[derive(Clone, Debug)]
enum Val {
    Node(Node),
    Num(f64),
    Frac(Fraction),
    Str(String),
    Complex(f64, f64),
}

impl Val {
    fn is_node(&self) -> bool {
        matches!(self, Val::Node(_))
    }
}

/// `simplifyConstant(node, options)`.
pub fn simplify_constant(node: &Node, options: &SimplifyOptions) -> Result<Node, SymError> {
    ensure_node(fold(node, options)?)
}

/// `_exactFraction(n, options)`.
fn exact_fraction(n: f64, o: &SimplifyOptions) -> Val {
    if o.exact_fractions != Some(false)
        && n.is_finite()
        && let Ok(f) = Fraction::from_f64(n)
    {
        let limit = o.fractions_limit.unwrap_or(f64::INFINITY);
        if f.value() == n && big_to_f64(&f.n) < limit && big_to_f64(&f.d) < limit {
            return Val::Frac(f);
        }
    }
    Val::Num(n)
}

/// `_toNumber(value, options)` for a result `_eval` had to retry.
fn to_number(v: Val, o: &SimplifyOptions) -> Val {
    match v {
        Val::Num(n) => exact_fraction(n, o),
        Val::Complex(re, im) => {
            if im == 0.0 {
                exact_fraction(re, o)
            } else {
                Val::Complex(re, im)
            }
        }
        Val::Str(s) => exact_fraction(crate::jsnum::parse(s.trim()).unwrap_or(f64::NAN), o),
        other => other,
    }
}

fn negated(n: Node) -> Node {
    op(Op::UnaryMinus, vec![n])
}

/// `_ensureNode` / `_toNode`.
fn ensure_node(v: Val) -> Result<Node, SymError> {
    Ok(match v {
        Val::Node(n) => n,
        Val::Num(n) if n < 0.0 => negated(num(-n)),
        Val::Num(n) => num(n),
        Val::Frac(f) => {
            let value = big_to_f64(&f.n);
            let numerator = if f.s < 0 {
                negated(num(value))
            } else {
                num(value)
            };
            if f.d == num_bigint::BigInt::from(1) {
                numerator
            } else {
                op(Op::Divide, vec![numerator, num(big_to_f64(&f.d))])
            }
        }
        Val::Str(s) => Node::Constant(Constant::Str(s)),
        Val::Complex(..) => return err("Cannot convert Complex number to Node"),
    })
}

/// `foldFraction(node, options)`.
fn fold(node: &Node, o: &SimplifyOptions) -> Result<Val, SymError> {
    let ctx = Ctx::default();
    match node {
        Node::Symbol(_) => Ok(Val::Node(node.clone())),
        Node::Constant(c) => Ok(match c {
            Constant::Number { value, .. } => exact_fraction(*value, o),
            Constant::Str(s) => Val::Str(s.clone()),
            _ => Val::Node(node.clone()),
        }),
        Node::Function { args, .. } if !matches!(function_name(node), "add" | "multiply") => {
            let name = function_name(node).to_string();
            let folded = args
                .iter()
                .map(|a| fold(a, o))
                .collect::<Result<Vec<_>, _>>()?;
            if !folded.iter().any(Val::is_node)
                && let Ok(v) = eval(&name, &folded, o)
            {
                return Ok(v);
            }
            let args = folded
                .into_iter()
                .map(ensure_node)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Val::Node(super::call(&name, args)))
        }
        Node::Function { .. } | Node::Operator { .. } => {
            let fname = match node {
                Node::Operator { op, .. } => op.name().to_string(),
                _ => function_name(node).to_string(),
            };
            if let Node::Operator { args, .. } = node
                && args.len() == 1
            {
                let a = fold(&args[0], o)?;
                return if a.is_node() {
                    Ok(Val::Node(make_node(node, vec![ensure_node(a)?])))
                } else {
                    eval(&fname, &[a], o)
                };
            }
            if is_associative(node, &ctx) {
                let args = all_children(node, &ctx)
                    .iter()
                    .map(|a| fold(a, o))
                    .collect::<Result<Vec<_>, _>>()?;
                if ctx_commutative(&fname) {
                    let (consts, mut vars): (Vec<Val>, Vec<Val>) =
                        args.iter().cloned().partition(|v| !v.is_node());
                    if consts.len() > 1 {
                        let folded = fold_op(&fname, consts, node, o)?;
                        vars.insert(0, folded);
                        return fold_op(&fname, vars, node, o);
                    }
                    return fold_op(&fname, args, node, o);
                }
                return fold_op(&fname, args, node, o);
            }
            let args = super::args(node)
                .iter()
                .map(|a| fold(a, o))
                .collect::<Result<Vec<_>, _>>()?;
            fold_op(&fname, args, node, o)
        }
        Node::Parenthesis(c) => fold(c, o),
        Node::Array(items) => {
            let folded = items
                .iter()
                .map(|a| fold(a, o))
                .collect::<Result<Vec<_>, _>>()?;
            let items = folded
                .into_iter()
                .map(ensure_node)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Val::Node(Node::Array(items)))
        }
        Node::Accessor { .. } => Ok(Val::Node(node.clone())),
        other => err(format!(
            "Unimplemented node type in simplifyConstant: {}",
            node_type(other)
        )),
    }
}

fn ctx_commutative(fname: &str) -> bool {
    matches!(fname, "add" | "unaryPlus" | "multiply" | "paren")
}

fn node_type(n: &Node) -> &'static str {
    match n {
        Node::Assignment { .. } => "AssignmentNode",
        Node::Block(_) => "BlockNode",
        Node::FunctionAssignment { .. } => "FunctionAssignmentNode",
        Node::Range { .. } => "RangeNode",
        Node::Conditional { .. } => "ConditionalNode",
        Node::Relational { .. } => "RelationalNode",
        _ => "Node",
    }
}

/// `foldOp(fn, args, makeNode, options)`.
fn fold_op(
    fname: &str,
    args: Vec<Val>,
    template: &Node,
    o: &SimplifyOptions,
) -> Result<Val, SymError> {
    let mut it = args.into_iter();
    let Some(first) = it.next() else {
        return err("no arguments");
    };
    let mut sofar: Vec<Val> = vec![first];
    for next in it {
        if !next.is_node() {
            let last = sofar.pop().expect("nonempty");
            if last.is_node() {
                sofar = vec![last, next];
                continue;
            }
            match eval(fname, &[last.clone(), next.clone()], o) {
                Ok(v) => {
                    sofar.push(v);
                    continue;
                }
                Err(_) => sofar.push(last),
            }
        }
        let top = ensure_node(sofar.pop().expect("nonempty"))?;
        sofar.push(Val::Node(top));
        let newtree = if sofar.len() == 1 {
            ensure_node(sofar.pop().expect("one"))?
        } else {
            let parts = sofar
                .drain(..)
                .map(ensure_node)
                .collect::<Result<Vec<_>, _>>()?;
            make_node(template, parts)
        };
        sofar = vec![Val::Node(make_node(
            template,
            vec![newtree, ensure_node(next)?],
        ))];
    }
    if sofar.len() == 1 {
        return Ok(sofar.pop().expect("one"));
    }
    let second = sofar.pop().expect("two");
    let first = sofar.pop().expect("two");
    Ok(Val::Node(make_node(
        template,
        vec![ensure_node(first)?, ensure_node(second)?],
    )))
}

/// `_eval(fnname, args, options)`: the call as typed, retried in doubles when it throws.
fn eval(fname: &str, args: &[Val], o: &SimplifyOptions) -> Result<Val, SymError> {
    match call(fname, args) {
        Ok(v) => Ok(v),
        Err(_) => {
            let plain: Vec<Val> = args
                .iter()
                .map(|a| match a {
                    Val::Frac(f) => Val::Num(f.value()),
                    other => other.clone(),
                })
                .collect();
            Ok(to_number(call(fname, &plain)?, o))
        }
    }
}

/// A double to a Fraction as mathjs's implicit conversion does: only when lossless.
fn convert(x: f64) -> Result<Fraction, SymError> {
    match Fraction::from_f64(x) {
        Ok(f) if f.value() == x => Ok(f),
        _ => err(format!(
            "Cannot implicitly convert a number to a Fraction when there will be a loss of precision (value: {})",
            crate::jsnum::to_string(x)
        )),
    }
}

/// mathjs's own `pow` for doubles (what its Fraction `pow` falls back to): a negative base under a
/// non-integer exponent is Complex.
fn mathjs_pow(x: f64, y: f64) -> Val {
    if crate::jsnum::is_integer(y) || x >= 0.0 {
        if (x * x < 1.0 && y == f64::INFINITY) || (x * x > 1.0 && y == f64::NEG_INFINITY) {
            return Val::Num(0.0);
        }
        return Val::Num(crate::eval::ops::js_pow(x, y));
    }
    if (x * x < 1.0 && y == f64::INFINITY) || (x * x > 1.0 && y == f64::NEG_INFINITY) {
        return Val::Num(0.0);
    }
    // (x + 0i)^y
    let r = (-x).powf(y);
    let t = std::f64::consts::PI * y;
    Val::Complex(r * t.cos(), r * t.sin())
}

fn frac_binary(fname: &str, a: &Fraction, b: &Fraction) -> Result<Val, SymError> {
    Ok(match fname {
        "add" => Val::Frac(a.add(b)),
        "subtract" => Val::Frac(a.sub(b)),
        "multiply" => Val::Frac(a.mul(b)),
        "divide" => Val::Frac(a.div(b).map_err(|e| SymError(e.0.into()))?),
        "pow" => match a.pow(b).map_err(|e| SymError(e.0.into()))? {
            Some(f) => Val::Frac(f),
            None => mathjs_pow(a.value(), b.value()),
        },
        _ => return err(format!("no Fraction signature for {fname}")),
    })
}

fn num_binary(fname: &str, a: f64, b: f64) -> Result<Val, SymError> {
    Ok(Val::Num(match fname {
        "add" => a + b,
        "subtract" => a - b,
        "multiply" => a * b,
        // the engine's override: dividing by zero is undefined
        "divide" => {
            if b == 0.0 {
                f64::NAN
            } else {
                a / b
            }
        }
        "pow" => real_pow(a, b),
        "mod" => modulo(a, b).map_err(|e| SymError(format!("{e:?}")))?,
        "log" => return Ok(log_of(a, Some(b))),
        "nthRoot" => {
            if a < 0.0 && b % 2.0 == 0.0 {
                return err("Root must be odd when a is negative.");
            }
            if b == 0.0 {
                return err("Root must be non-zero");
            }
            let r = a.abs().powf(1.0 / b);
            if a < 0.0 { -r } else { r }
        }
        "atan2" => a.atan2(b),
        "hypot" => a.hypot(b),
        "round" => {
            let f = 10f64.powf(b);
            crate::jsnum::round(a * f) / f
        }
        "max" => a.max(b),
        "min" => a.min(b),
        _ => return err(format!("{fname} is not a function")),
    }))
}

fn log_of(x: f64, base: Option<f64>) -> Val {
    let ln = |v: f64| -> Val {
        if v < 0.0 {
            Val::Complex((-v).ln(), std::f64::consts::PI)
        } else {
            Val::Num(v.ln())
        }
    };
    match base {
        None => ln(x),
        Some(b) => match (ln(x), ln(b)) {
            (Val::Num(p), Val::Num(q)) => Val::Num(p / q),
            _ => Val::Complex(f64::NAN, f64::NAN),
        },
    }
}

/// A real function of one double, or Complex where mathjs answers complex.
fn num_unary(fname: &str, x: f64) -> Result<Val, SymError> {
    let complex = Val::Complex(f64::NAN, f64::NAN);
    let v = match fname {
        "unaryMinus" => -x,
        "unaryPlus" => x,
        "sqrt" => {
            if x < 0.0 {
                return Ok(complex);
            }
            x.sqrt()
        }
        "cbrt" => x.cbrt(),
        "exp" => x.exp(),
        "expm1" => x.exp_m1(),
        "log" => return Ok(log_of(x, None)),
        "log10" => {
            if x < 0.0 {
                return Ok(complex);
            }
            x.log10()
        }
        "log2" => {
            if x < 0.0 {
                return Ok(complex);
            }
            x.log2()
        }
        "log1p" => {
            if x < -1.0 {
                return Ok(complex);
            }
            x.ln_1p()
        }
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "sec" => 1.0 / x.cos(),
        "csc" => 1.0 / x.sin(),
        "cot" => 1.0 / x.tan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "sech" => 1.0 / x.cosh(),
        "csch" => 1.0 / x.sinh(),
        "coth" => 1.0 / x.tanh(),
        "asin" => {
            if x.abs() > 1.0 {
                return Ok(complex);
            }
            x.asin()
        }
        "acos" => {
            if x.abs() > 1.0 {
                return Ok(complex);
            }
            x.acos()
        }
        "atan" => x.atan(),
        "acot" => (1.0 / x).atan(),
        "asec" => {
            if x.abs() < 1.0 {
                return Ok(complex);
            }
            (1.0 / x).acos()
        }
        "acsc" => {
            if x.abs() < 1.0 {
                return Ok(complex);
            }
            (1.0 / x).asin()
        }
        "asinh" => x.asinh(),
        "acosh" => {
            if x < 1.0 {
                return Ok(complex);
            }
            x.acosh()
        }
        "atanh" => {
            if x.abs() > 1.0 {
                return Ok(complex);
            }
            x.atanh()
        }
        "acsch" => (1.0 / x).asinh(),
        "asech" => {
            if !(0.0..=1.0).contains(&x) {
                return Ok(complex);
            }
            (1.0 / x).acosh()
        }
        "acoth" => {
            if x.abs() < 1.0 {
                return Ok(complex);
            }
            (1.0 / x).atanh()
        }
        "abs" => x.abs(),
        "sign" => {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "fix" => x.trunc(),
        "round" => crate::jsnum::round(x),
        "square" => x * x,
        "cube" => x * x * x,
        "gamma" => crate::eval::builtins::gamma(x),
        "factorial" => {
            if x < 0.0 {
                return err("Value must be non-negative");
            }
            crate::eval::builtins::factorial(x)
        }
        _ => return err(format!("{fname} is not a function")),
    };
    Ok(Val::Num(v))
}

/// One typed call: Fractions where mathjs has a Fraction signature, else doubles.
fn call(fname: &str, args: &[Val]) -> Result<Val, SymError> {
    if args.iter().any(|a| matches!(a, Val::Complex(..))) {
        return Ok(Val::Complex(f64::NAN, f64::NAN));
    }
    if args.iter().any(|a| matches!(a, Val::Str(_) | Val::Node(_))) {
        return err(format!("unsupported argument to {fname}"));
    }
    match (fname, args) {
        // the engine's `mod` is a plain function of the values
        ("mod", [a, b]) => {
            let (x, y) = (value(a), value(b));
            num_binary("mod", x, y)
        }
        (_, [Val::Num(a), Val::Num(b)]) => num_binary(fname, *a, *b),
        ("add" | "subtract" | "multiply" | "divide" | "pow", [a, b]) => {
            let a = as_frac(a)?;
            let b = as_frac(b)?;
            frac_binary(fname, &a, &b)
        }
        ("unaryMinus", [Val::Frac(f)]) => Ok(Val::Frac(f.neg())),
        ("unaryPlus", [Val::Frac(f)]) => Ok(Val::Frac(f.clone())),
        ("abs", [Val::Frac(f)]) => Ok(Val::Frac(f.abs())),
        ("sign" | "floor" | "ceil" | "fix" | "round" | "square" | "cube", [Val::Frac(f)]) => {
            match num_unary(fname, f.value())? {
                Val::Num(v) => Fraction::from_f64(v)
                    .map(Val::Frac)
                    .map_err(|e| SymError(e.0.into())),
                other => Ok(other),
            }
        }
        (_, [Val::Num(x)]) => num_unary(fname, *x),
        _ => err(format!("no signature of {fname} for these arguments")),
    }
}

fn value(v: &Val) -> f64 {
    match v {
        Val::Num(x) => *x,
        Val::Frac(f) => f.value(),
        _ => f64::NAN,
    }
}

fn as_frac(v: &Val) -> Result<Fraction, SymError> {
    match v {
        Val::Frac(f) => Ok(f.clone()),
        Val::Num(x) => convert(*x),
        _ => err("not a number"),
    }
}
