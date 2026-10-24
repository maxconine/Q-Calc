//! Evaluates a parsed expression the way `math.evaluate` does in mathjs `number` mode, with the engine's own
//! overrides (`divide` by 0 is undefined, `pow` takes odd roots of negatives, `mod` is non-negative) and the scope
//! from `builtinScope` in `src/engine/scientific.ts`.
//!
//! Values are doubles, as in the TS; exact digits come afterwards from `crate::precise`.

pub mod builtins;
pub mod complex; // WP-FUNCTIONS
pub mod ops;

use std::collections::BTreeMap;

use crate::syntax::{Constant, Node, Op};
use crate::types::{AngleMode, UserFunction};

/// A mathjs value as the engine meets it.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Num(f64),
    Bool(bool),
    Complex {
        re: f64,
        im: f64,
    },
    /// An array or matrix, nested as written.
    List(Vec<Val>),
    Str(String),
    /// A function used as a value (`sin` alone); never an answer.
    Function(String),
    Null,
    Undefined,
}

impl Val {
    /// `Number(x)` in JS: what a scalar function in the TS scope sees when handed this value.
    pub fn to_number(&self) -> f64 {
        match self {
            Val::Num(n) => *n,
            Val::Bool(b) => f64::from(u8::from(*b)),
            Val::Null => 0.0,
            Val::List(_) => f64::NAN, // WP-FUNCTIONS: Number(matrix) is NaN, even of `[5]` (mathjs's valueOf)
            Val::Str(s) => {
                let t = s.trim();
                if t.is_empty() {
                    0.0
                } else {
                    t.parse().unwrap_or(f64::NAN)
                }
            }
            Val::Complex { re, im } => complex::to_number(*re, *im), // WP-FUNCTIONS: Number(String(z))
            Val::Function(_) | Val::Undefined => f64::NAN,
        }
    }

    /// JS truthiness, for `and`, `or`, `not` and `?:`.
    pub fn truthy(&self) -> bool {
        match self {
            Val::Num(n) => *n != 0.0 && !n.is_nan(),
            Val::Bool(b) => *b,
            Val::Complex { re, im } => *re != 0.0 || *im != 0.0,
            Val::List(items) => !items.is_empty(),
            Val::Str(s) => !s.is_empty(),
            Val::Function(_) => true,
            Val::Null | Val::Undefined => false,
        }
    }
}

/// Why an expression has no value. The TS catches every mathjs throw and shows the line blank; so does the port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    UndefinedSymbol(String),
    UndefinedFunction(String),
    TooManyArguments(String),
    /// A value or operation the port doesn't handle yet (a unit, a matrix op): blank, as an exception would be.
    Unsupported(String),
    /// User functions calling themselves without end.
    TooDeep,
    Other(String),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::UndefinedSymbol(s) => write!(f, "Undefined symbol {s}"),
            EvalError::UndefinedFunction(s) => write!(f, "Undefined function {s}"),
            EvalError::TooManyArguments(s) => write!(f, "too many arguments to {s}"),
            EvalError::Unsupported(s) => write!(f, "unsupported: {s}"),
            EvalError::TooDeep => write!(f, "too deep"),
            EvalError::Other(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for EvalError {}

pub type EvalResult = Result<Val, EvalError>;

/// What an evaluation can see: the variables, the user's functions and the settings that change meaning.
#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub variables: BTreeMap<String, f64>,
    pub functions: BTreeMap<String, UserFunction>,
    pub angle_mode: AngleMode,
    pub ans: Option<f64>,
    /// An `i` was typed, so `sqrt(-4)` is `2i` rather than undefined.
    pub complex_typed: bool,
    /// How many user-function calls deep this evaluation is.
    pub depth: u32,
}

/// User functions may call each other this deep before the line goes blank (the TS overflows its stack instead).
pub const MAX_DEPTH: u32 = 32;

/// mathjs constants outside the engine's scope (`math.pi`, `math.i`, …).
fn math_constant(name: &str) -> Option<Val> {
    use std::f64::consts as c;
    Some(match name {
        "e" | "E" => Val::Num(c::E),
        "pi" | "PI" => Val::Num(c::PI),
        "tau" => Val::Num(c::TAU),
        "phi" => Val::Num(builtins::PHI),
        "i" => Val::Complex { re: 0.0, im: 1.0 },
        "Infinity" => Val::Num(f64::INFINITY),
        "NaN" => Val::Num(f64::NAN),
        "LN2" => Val::Num(c::LN_2),
        "LN10" => Val::Num(c::LN_10),
        "LOG2E" => Val::Num(c::LOG2_E),
        "LOG10E" => Val::Num(c::LOG10_E),
        "SQRT1_2" => Val::Num(c::FRAC_1_SQRT_2),
        "SQRT2" => Val::Num(c::SQRT_2),
        "true" => Val::Bool(true),
        "false" => Val::Bool(false),
        "null" => Val::Null,
        _ => return None,
    })
}

/// Looks a name up the way mathjs does: the scope first (variables, `ans`, the engine's constants), then mathjs.
fn symbol(name: &str, scope: &Scope) -> EvalResult {
    if let Some(v) = scope.variables.get(name) {
        return Ok(Val::Num(*v));
    }
    if name == "ans"
        && let Some(a) = scope.ans
    {
        return Ok(Val::Num(a));
    }
    if let Some(v) = builtins::scope_constant(name) {
        return Ok(v);
    }
    if scope.functions.contains_key(name) || builtins::is_builtin(name) {
        return Ok(Val::Function(name.to_string()));
    }
    math_constant(name).ok_or_else(|| EvalError::UndefinedSymbol(name.to_string()))
}

/// Evaluates `node` in `scope`.
pub fn evaluate(node: &Node, scope: &Scope) -> EvalResult {
    match node {
        Node::Constant(c) => Ok(match c {
            Constant::Number { value, .. } => Val::Num(*value),
            Constant::Str(s) => Val::Str(s.clone()),
            Constant::Bool(b) => Val::Bool(*b),
            Constant::Null => Val::Null,
            Constant::Undefined => Val::Undefined,
        }),
        Node::Symbol(name) => symbol(name, scope),
        Node::Parenthesis(inner) => evaluate(inner, scope),
        Node::Operator { op, args, .. } => operator(*op, args, scope),
        Node::Function { callee, args } => {
            let Node::Symbol(name) = callee.as_ref() else {
                return Err(EvalError::Unsupported("call of a non-name".into()));
            };
            let values = args
                .iter()
                .map(|a| evaluate(a, scope))
                .collect::<Result<Vec<_>, _>>()?;
            call(name, &values, scope)
        }
        Node::Array(items) => Ok(Val::List(
            items
                .iter()
                .map(|a| evaluate(a, scope))
                .collect::<Result<_, _>>()?,
        )),
        Node::Conditional {
            condition,
            then,
            otherwise,
        } => {
            if evaluate(condition, scope)?.truthy() {
                evaluate(then, scope)
            } else {
                evaluate(otherwise, scope)
            }
        }
        Node::Relational { ops, params } => {
            let values = params
                .iter()
                .map(|p| evaluate(p, scope))
                .collect::<Result<Vec<_>, _>>()?;
            for (i, op) in ops.iter().enumerate() {
                if !ops::binary(*op, &values[i], &values[i + 1])?.truthy() {
                    return Ok(Val::Bool(false));
                }
            }
            Ok(Val::Bool(true))
        }
        Node::Range { .. } => Err(EvalError::Unsupported("range".into())),
        Node::Assignment { .. } | Node::FunctionAssignment { .. } => {
            Err(EvalError::Unsupported("assignment".into()))
        }
        Node::Accessor { .. } => Err(EvalError::Unsupported("index".into())),
        Node::Block(_) => Err(EvalError::Unsupported("block".into())),
    }
}

fn operator(op: Op, args: &[Node], scope: &Scope) -> EvalResult {
    match (op, args) {
        (Op::And, [a, b]) => {
            let left = evaluate(a, scope)?;
            Ok(Val::Bool(left.truthy() && evaluate(b, scope)?.truthy()))
        }
        (Op::Or, [a, b]) => {
            let left = evaluate(a, scope)?;
            Ok(Val::Bool(left.truthy() || evaluate(b, scope)?.truthy()))
        }
        (Op::Nullish, [a, b]) => match evaluate(a, scope)? {
            Val::Null | Val::Undefined => evaluate(b, scope),
            v => Ok(v),
        },
        (Op::Factorial, [a]) => builtins::call_builtin("factorial", &[evaluate(a, scope)?], scope)
            .unwrap_or_else(|| Err(EvalError::UndefinedFunction("factorial".into()))),
        (_, [a]) => ops::unary(op, &evaluate(a, scope)?),
        (_, [a, b]) => {
            let (x, y) = (evaluate(a, scope)?, evaluate(b, scope)?);
            ops::binary(op, &x, &y)
        }
        _ => Err(EvalError::Unsupported(op.name().into())),
    }
}

/// Calls a function by name: a user function, then the engine's scope, then mathjs's own.
pub fn call(name: &str, args: &[Val], scope: &Scope) -> EvalResult {
    if let Some(def) = scope.functions.get(name) {
        return call_user(name, def, args, scope);
    }
    if scope.variables.contains_key(name) {
        return Err(EvalError::Other(format!("{name} is not a function")));
    }
    builtins::call_builtin(name, args, scope)
        .unwrap_or_else(|| Err(EvalError::UndefinedFunction(name.to_string())))
}

/// `userFunction` in the TS: the body is worked out as its own line, with the parameters as variables.
fn call_user(name: &str, def: &UserFunction, args: &[Val], scope: &Scope) -> EvalResult {
    if args.len() > def.params.len() {
        return Err(EvalError::TooManyArguments(name.to_string()));
    }
    if scope.depth >= MAX_DEPTH {
        return Err(EvalError::TooDeep);
    }
    let mut variables = scope.variables.clone();
    for (i, p) in def.params.iter().enumerate() {
        let n = args.get(i).map_or(f64::NAN, Val::to_number);
        variables.insert(p.clone(), if n.is_finite() { n } else { f64::NAN });
    }
    let ctx = crate::scientific::ScientificContext {
        ans: scope.ans,
        angle_mode: scope.angle_mode,
        variables,
        functions: scope.functions.clone(),
        depth: scope.depth + 1,
    };
    match crate::scientific::eval_scientific(&def.body, &ctx) {
        Some(v) if v.kind == crate::types::ValueKind::Number => Ok(Val::Num(v.n)),
        _ => Ok(Val::Num(f64::NAN)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn eval(s: &str) -> Val {
        evaluate(&parse(s).unwrap(), &Scope::default()).unwrap()
    }

    #[test]
    fn arithmetic_like_mathjs() {
        assert_eq!(eval("2+3*4"), Val::Num(14.0));
        assert_eq!(eval("2^3^2"), Val::Num(512.0));
        assert_eq!(eval("-2^2"), Val::Num(-4.0));
        assert_eq!(eval("2^-2^2"), Val::Num(0.0625));
        assert_eq!(eval("(-8)^(1/3)"), Val::Num(-2.0));
        assert!(matches!(eval("(-4)^0.5"), Val::Num(n) if n.is_nan()));
        assert!(matches!(eval("1/0"), Val::Num(n) if n.is_nan()));
        assert_eq!(eval("-7 mod 3"), Val::Num(2.0));
        assert_eq!(eval("factorial(5)"), Val::Num(120.0));
        assert_eq!(eval("2 == 2"), Val::Bool(true));
        assert_eq!(eval("1 < 2 < 3"), Val::Bool(true));
    }
}
