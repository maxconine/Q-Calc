//! Truthful digits (`src/engine/precise.ts`): the float answer is re-run in decimals at two precisions at once and
//! replaced by the higher one only when both agree with each other and every function agrees with the engine's own
//! at the same inputs. Anything unsupported, too costly or doubtful keeps the float answer. `0.1+0.2` shows 0.3 at
//! 16 figures, and `sin(30)` stores exactly 0.5, because of this.
//!
//! The decimals are [`real::Real`] at the precisions decimal.js would use (32 and 48 digits, then 48 and 80, and 20
//! for the per-function check), rounded half-up with Euclidean remainders, as mathjs configures its BigNumbers.
//! The port follows the TS step for step: the same walk over the parsed tree, the same work counting, the same
//! twins (`TWINS`), the same `agrees`, `settle` and `pairMod`, and the same nudging of inexact inputs.

pub mod real;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::eval::{Scope, Val, builtins, ops};
use crate::jsnum;
use crate::scientific::{ScientificContext, preprocess_checked};
use crate::syntax::{Constant, Node, Op, parse};
use crate::types::{AngleMode, Value, ValueKind};
use real::{Ctx, Modulo, Real, Rounding};

/// Work per answer, counted rather than timed so the digits never depend on how busy the machine is.
pub const BUDGET: i64 = 1000;
/// A function or a power costs this much more than an operator; about 5 ms of budget runs 20 of them.
const CALL_COST: i64 = 50;
/// Precision pairs, tried in turn while the answer hasn't settled and there's work left.
const LADDER: [(usize, usize); 2] = [(32, 48), (48, 80)];
/// Enough to tell a real mismatch from float rounding in the per-function check.
const CHECK: usize = 20;
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

/// Why the float answer stays, or what replaces it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Kept(Kept),
    Same,
    Corrected(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    NotANumber,
    Unsupported,
    Meaning,
    Budget,
    Unsettled,
    Range,
    InexactInput,
}

impl Kept {
    /// The TS `reason` string.
    pub fn reason(self) -> &'static str {
        match self {
            Kept::NotANumber => "not-a-number",
            Kept::Unsupported => "unsupported",
            Kept::Meaning => "meaning",
            Kept::Budget => "budget",
            Kept::Unsettled => "unsettled",
            Kept::Range => "range",
            Kept::InexactInput => "inexact input",
        }
    }
}

/// The TS throws `ABORT` (no twin) or `MEANING` (the twin disagrees); running out of work is read off the counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Abort {
    Unsupported,
    Meaning,
}

type R<T> = Result<T, Abort>;

fn abort<T>() -> R<T> {
    Err(Abort::Unsupported)
}

/// A decimal.js constructor as mathjs makes its BigNumbers: half-up, Euclidean `mod`.
fn dec(precision: usize) -> Ctx {
    Ctx::new(precision).with_modulo(Modulo::Euclid)
}

fn num(n: f64) -> Real {
    Real::from_f64(n)
}

/// `new D(String(n))`: the digits JS writes, so −0 arrives as 0.
fn from_number(n: f64) -> Real {
    Real::parse(&jsnum::to_string(n)).unwrap_or(Real::NAN)
}

// ---------------------------------------------------------------------------------------------------------------
// the walk

/// What the walk computes with: plain doubles through the engine's own functions, or decimal pairs.
trait Backend {
    type T: Clone;
    /// A number literal: its exact digits, as mathjs's BigNumber parser reads them.
    fn lit(&mut self, v: &Real) -> R<Self::T>;
    fn constant(&mut self, name: &str) -> R<Self::T>;
    fn op(&mut self, op: Op, args: Vec<Self::T>) -> R<Self::T>;
    fn call(&mut self, name: &str, args: Vec<Self::T>) -> R<Self::T>;
    fn tick(&mut self, v: Self::T) -> R<Self::T>;
}

struct Env<'a, T> {
    vars: BTreeMap<String, T>,
    ctx: &'a ScientificContext,
    depth: u32,
}

fn is_constant(name: &str) -> bool {
    matches!(name, "pi" | "tau" | "e")
}

/// The operators the twin follows, with their arity (`OPS` in the TS).
fn op_arity(op: Op) -> Option<usize> {
    Some(match op {
        Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::Pow | Op::Mod => 2,
        Op::UnaryMinus | Op::UnaryPlus | Op::Factorial => 1,
        _ => return None,
    })
}

fn walk<B: Backend>(node: &Node, b: &mut B, env: &Env<'_, B::T>) -> R<B::T> {
    match node {
        Node::Parenthesis(inner) => walk(inner, b, env),
        Node::Constant(Constant::Number { text, .. }) => {
            let v = Real::parse(text).ok_or(Abort::Unsupported)?;
            let lit = b.lit(&v)?;
            b.tick(lit)
        }
        Node::Symbol(name) => {
            if is_constant(name) {
                return b.constant(name);
            }
            env.vars.get(name).cloned().ok_or(Abort::Unsupported)
        }
        Node::Operator { op, args, .. } => {
            if op_arity(*op) != Some(args.len()) {
                return abort();
            }
            let values = args
                .iter()
                .map(|a| walk(a, b, env))
                .collect::<R<Vec<_>>>()?;
            let v = b.op(*op, values)?;
            b.tick(v)
        }
        Node::Function { callee, args } => {
            let Node::Symbol(name) = callee.as_ref() else {
                return abort();
            };
            let values = args
                .iter()
                .map(|a| walk(a, b, env))
                .collect::<R<Vec<_>>>()?;
            if let Some(user) = env.ctx.functions.get(name) {
                let v = call_user(&user.params, &user.body, values, b, env)?;
                return b.tick(v);
            }
            if !is_twinned(name) {
                return abort();
            }
            let v = b.call(name, values)?;
            b.tick(v)
        }
        _ => abort(),
    }
}

/// A user function: its body worked out as its own expression, with the parameters bound to the arguments.
fn call_user<B: Backend>(
    params: &[String],
    body: &str,
    args: Vec<B::T>,
    b: &mut B,
    env: &Env<'_, B::T>,
) -> R<B::T> {
    if env.depth > 8 || args.len() != params.len() {
        return abort();
    }
    let mut names = env.ctx.variables.clone();
    for p in params {
        names.insert(p.clone(), 0.0);
    }
    let body_ctx = ScientificContext {
        variables: names,
        ..env.ctx.clone()
    };
    let expr = preprocess_checked(body, &body_ctx).ok_or(Abort::Unsupported)?;
    let tree = parse(&expr).map_err(|_| Abort::Unsupported)?;
    let mut vars = env.vars.clone();
    for (p, a) in params.iter().zip(args) {
        vars.insert(p.clone(), a);
    }
    walk(
        &tree,
        b,
        &Env {
            vars,
            ctx: env.ctx,
            depth: env.depth + 1,
        },
    )
}

// ---------------------------------------------------------------------------------------------------------------
// the engine's side: plain doubles

fn engine_scope(mode: AngleMode) -> Scope {
    Scope {
        angle_mode: mode,
        ..Scope::default()
    }
}

/// A scope function called on doubles: Ok(Some(n)) for a number (NaN included), Ok(None) for another kind of value,
/// Err where the TS would throw (no such function, a throw inside it).
fn engine_call(name: &str, args: &[f64], scope: &Scope) -> R<Option<f64>> {
    let vals: Vec<Val> = args.iter().map(|&n| Val::Num(n)).collect();
    match builtins::call_builtin(name, &vals, scope) {
        Some(Ok(Val::Num(n))) => Ok(Some(n)),
        Some(Ok(_)) => Ok(None),
        Some(Err(_)) | None => abort(),
    }
}

/// The engine's own functions over plain numbers: the reference the decimal walk must mean the same as.
struct NumberBackend {
    scope: Scope,
}

fn real_number(v: Option<f64>) -> R<f64> {
    match v {
        Some(n) if !n.is_nan() => Ok(n),
        _ => abort(),
    }
}

impl Backend for NumberBackend {
    type T = f64;

    fn lit(&mut self, v: &Real) -> R<f64> {
        Ok(v.to_f64())
    }

    fn constant(&mut self, name: &str) -> R<f64> {
        match builtins::scope_constant(name) {
            Some(Val::Num(n)) => real_number(Some(n)),
            _ => abort(),
        }
    }

    fn op(&mut self, op: Op, args: Vec<f64>) -> R<f64> {
        let r = match (op, args.as_slice()) {
            (Op::Factorial, [a]) => Ok(Val::Num(builtins::factorial(*a))),
            (_, [a]) => ops::unary(op, &Val::Num(*a)),
            (_, [a, b]) => ops::binary(op, &Val::Num(*a), &Val::Num(*b)),
            _ => return abort(),
        };
        match r {
            Ok(Val::Num(n)) => real_number(Some(n)),
            _ => abort(),
        }
    }

    fn call(&mut self, name: &str, args: Vec<f64>) -> R<f64> {
        real_number(engine_call(name, &args, &self.scope)?)
    }

    fn tick(&mut self, v: f64) -> R<f64> {
        Ok(v)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the twins

/// π, e and the degree conversion at one precision (`kitFor`).
#[derive(Debug)]
struct Kit {
    pi: Real,
    e: Real,
    deg: Real,
    rad: bool,
}

impl Kit {
    fn to_rad(&self, d: &Ctx, x: &Real) -> Real {
        if self.rad {
            x.clone()
        } else {
            d.mul(x, &self.deg)
        }
    }

    fn in_mode(&self, d: &Ctx, x: &Real) -> Real {
        if self.rad {
            x.clone()
        } else {
            d.div(x, &self.deg)
        }
    }
}

/// Kits by (precision, radians).
type KitCache = Vec<((usize, bool), Rc<Kit>)>;

thread_local! {
    static KITS: RefCell<KitCache> = const { RefCell::new(Vec::new()) };
}

fn kit(precision: usize, mode: AngleMode) -> Rc<Kit> {
    let key = (precision, mode == AngleMode::Rad);
    KITS.with(|k| {
        if let Some((_, kit)) = k.borrow().iter().find(|(at, _)| *at == key) {
            return kit.clone();
        }
        let d = dec(precision);
        let pi = d.acos(&num(-1.0));
        let kit = Rc::new(Kit {
            deg: d.div(&pi, &num(180.0)),
            e: d.exp(&Real::one()),
            pi,
            rad: key.1,
        });
        k.borrow_mut().push((key, kit.clone()));
        kit
    })
}

/// Every name with a twin (`Object.hasOwn(TWINS, name)`).
fn is_twinned(name: &str) -> bool {
    matches!(
        name,
        "sqrt"
            | "cbrt"
            | "nthRoot"
            | "exp"
            | "ln"
            | "log10"
            | "log2"
            | "log"
            | "abs"
            | "sign"
            | "floor"
            | "ceil"
            | "round"
            | "min"
            | "max"
            | "mean"
            | "total"
            | "mod"
            | "factorial"
            | "combinations"
            | "permutations"
            | "nCr"
            | "nPr"
            | "sinh"
            | "cosh"
            | "tanh"
            | "csch"
            | "sech"
            | "coth"
            | "asinh"
            | "acosh"
            | "atanh"
            | "acsch"
            | "asech"
            | "acoth"
    ) || trig_twin(name).is_some()
}

/// The trig twins and their `arc…` aliases, by their base name.
fn trig_twin(name: &str) -> Option<&'static str> {
    Some(match name {
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "csc" => "csc",
        "sec" => "sec",
        "cot" => "cot",
        "asin" | "arcsin" => "asin",
        "acos" | "arccos" => "acos",
        "atan" | "arctan" => "atan",
        "atan2" | "arctan2" => "atan2",
        "acsc" | "arccsc" => "acsc",
        "asec" | "arcsec" => "asec",
        "acot" | "arccot" => "acot",
        _ => return None,
    })
}

/// Jumps and integer tests; everything else is continuous and must match the engine locally.
fn discontinuous(name: &str) -> bool {
    matches!(
        name,
        "floor"
            | "ceil"
            | "round"
            | "sign"
            | "mod"
            | "factorial"
            | "combinations"
            | "permutations"
            | "nCr"
            | "nPr"
    )
}

fn is_trig_name(name: &str) -> bool {
    matches!(name, "sin" | "cos" | "tan" | "csc" | "sec" | "cot")
}

fn int_arg(x: &Real) -> R<&Real> {
    if x.is_integer() { Ok(x) } else { abort() }
}

fn product(d: &Ctx, from: f64, to: f64) -> Real {
    let mut p = Real::one();
    let mut i = from;
    while i <= to {
        p = d.mul(&p, &num(i));
        i += 1.0;
    }
    p
}

fn factorial(d: &Ctx, x: &Real) -> R<Real> {
    let n = int_arg(x)?.to_f64();
    if !(0.0..=170.0).contains(&n) {
        return abort();
    }
    Ok(product(d, 2.0, n))
}

fn choose(d: &Ctx, args: &[Real], perm: bool) -> R<Real> {
    let [a, b] = args else {
        return abort();
    };
    let n = int_arg(a)?.to_f64();
    let k = int_arg(b)?.to_f64();
    if n < 0.0 || k < 0.0 || n > 10_000.0 {
        return abort();
    }
    if k > n {
        return Ok(Real::zero());
    }
    let falling = product(d, n - k + 1.0, n);
    Ok(if perm {
        falling
    } else {
        d.div(&falling, &product(d, 2.0, k))
    })
}

fn modulo(d: &Ctx, a: &Real, b: &Real) -> R<Real> {
    if b.is_zero() {
        return abort();
    }
    let limit = num(MAX_SAFE);
    if a.abs().gt(&limit) || b.abs().gt(&limit) {
        return abort();
    }
    let m = b.abs();
    Ok(d.modulo(&d.add(&d.modulo(a, &m), &m), &m))
}

fn inverse(d: &Ctx, x: &Real) -> R<Real> {
    if x.is_zero() {
        abort()
    } else {
        Ok(d.div(&Real::one(), x))
    }
}

/// decimal.js has no fast path for huge arguments, and past these bounds a double can't hold the answer anyway.
fn bounded(x: &Real, limit: f64) -> R<&Real> {
    if x.abs().gt(&num(limit)) {
        abort()
    } else {
        Ok(x)
    }
}

fn nonzero_trig(x: Real) -> R<Real> {
    if x.abs().lt(&num(1e-12)) {
        abort()
    } else {
        Ok(x)
    }
}

/// `TWINS[name](D, args, kit)`.
fn twin(name: &str, d: &Ctx, args: &[Real], k: &Kit) -> R<Real> {
    let one = || -> R<&Real> {
        match args {
            [x] => Ok(x),
            _ => abort(),
        }
    };
    let gt1 = |x: &Real| x.abs().gt(&Real::one());
    let lt1 = |x: &Real| x.abs().lt(&Real::one());
    if let Some(t) = trig_twin(name) {
        return match t {
            "sin" => Ok(d.sin(&k.to_rad(d, bounded(one()?, 1e100)?))),
            "cos" => Ok(d.cos(&k.to_rad(d, bounded(one()?, 1e100)?))),
            "tan" => {
                let r = k.to_rad(d, bounded(one()?, 1e100)?);
                Ok(d.div(&d.sin(&r), &nonzero_trig(d.cos(&r))?))
            }
            "csc" => inverse(
                d,
                &nonzero_trig(d.sin(&k.to_rad(d, bounded(one()?, 1e100)?)))?,
            ),
            "sec" => inverse(
                d,
                &nonzero_trig(d.cos(&k.to_rad(d, bounded(one()?, 1e100)?)))?,
            ),
            "cot" => {
                let r = k.to_rad(d, bounded(one()?, 1e100)?);
                Ok(d.div(&d.cos(&r), &nonzero_trig(d.sin(&r))?))
            }
            "asin" => {
                let x = one()?;
                if gt1(x) {
                    abort()
                } else {
                    Ok(k.in_mode(d, &d.asin(x)))
                }
            }
            "acos" => {
                let x = one()?;
                if gt1(x) {
                    abort()
                } else {
                    Ok(k.in_mode(d, &d.acos(x)))
                }
            }
            "atan" => Ok(k.in_mode(d, &d.atan(one()?))),
            "atan2" => match args {
                [y, x] => Ok(k.in_mode(d, &d.atan2(y, x))),
                _ => abort(),
            },
            "acsc" => {
                let x = one()?;
                if lt1(x) {
                    abort()
                } else {
                    Ok(k.in_mode(d, &d.asin(&inverse(d, x)?)))
                }
            }
            "asec" => {
                let x = one()?;
                if lt1(x) {
                    abort()
                } else {
                    Ok(k.in_mode(d, &d.acos(&inverse(d, x)?)))
                }
            }
            _ => {
                // acot
                let x = one()?;
                let r = if x.is_zero() {
                    d.div(&k.pi, &num(2.0))
                } else {
                    d.atan(&inverse(d, x)?)
                };
                Ok(k.in_mode(d, &r))
            }
        };
    }
    let positive = |x: &Real| -> R<()> {
        if x.lte(&Real::zero()) {
            abort()
        } else {
            Ok(())
        }
    };
    match name {
        "sqrt" => {
            let x = one()?;
            if x.is_neg() { abort() } else { Ok(d.sqrt(x)) }
        }
        "cbrt" => Ok(d.cbrt(one()?)),
        "nthRoot" => {
            let (x, n) = match args {
                [x] => (x, 2.0),
                [x, n] => (x, int_arg(n)?.to_f64()),
                _ => return abort(),
            };
            if n < 1.0 {
                return abort();
            }
            let inv = d.div(&Real::one(), &num(n));
            if !x.is_neg() {
                Ok(d.pow(x, &inv))
            } else if n % 2.0 != 0.0 {
                Ok(d.pow(&x.neg(), &inv).neg())
            } else {
                abort()
            }
        }
        "exp" => Ok(d.exp(bounded(one()?, 1000.0)?)),
        "ln" => {
            let x = one()?;
            positive(x)?;
            Ok(d.ln(x))
        }
        "log10" => {
            let x = one()?;
            positive(x)?;
            Ok(d.log(x, Some(&num(10.0))))
        }
        "log2" => {
            let x = one()?;
            positive(x)?;
            Ok(d.log(x, Some(&num(2.0))))
        }
        "log" => {
            let (x, base) = match args {
                [x] => (x, None),
                [x, b] => (x, Some(b)),
                _ => return abort(),
            };
            positive(x)?;
            let Some(base) = base else {
                return Ok(d.ln(x));
            };
            if base.lte(&Real::zero()) || base.eq_value(&Real::one()) {
                return abort();
            }
            Ok(d.div(&d.ln(x), &d.ln(base)))
        }
        "abs" => Ok(one()?.abs()),
        "sign" => {
            let x = one()?;
            Ok(Real::from_i64(if x.is_zero() {
                0
            } else if x.is_neg() {
                -1
            } else {
                1
            }))
        }
        "floor" => Ok(one()?.floor()),
        "ceil" => Ok(one()?.ceil()),
        "round" => match args {
            [x] => Ok(x.to_dp(0, Rounding::HalfUp)),
            [x, digits] => {
                let p = d.pow(&num(10.0), int_arg(digits)?);
                Ok(d.div(&d.mul(x, &p).to_dp(0, Rounding::HalfUp), &p))
            }
            _ => abort(),
        },
        "min" if !args.is_empty() => Ok(Real::min_of(args)),
        "max" if !args.is_empty() => Ok(Real::max_of(args)),
        "mean" if !args.is_empty() => Ok(d.div(&d.sum(args), &num(args.len() as f64))),
        "total" if !args.is_empty() => Ok(d.sum(args)),
        "mod" => match args {
            [a, b] => modulo(d, a, b),
            _ => abort(),
        },
        "factorial" => factorial(d, one()?),
        "combinations" | "nCr" => choose(d, args, false),
        "permutations" | "nPr" => choose(d, args, true),
        "sinh" => Ok(d.sinh(bounded(one()?, 1000.0)?)),
        "cosh" => Ok(d.cosh(bounded(one()?, 1000.0)?)),
        "tanh" => Ok(d.tanh(bounded(one()?, 1000.0)?)),
        "csch" => inverse(d, &d.sinh(bounded(one()?, 1000.0)?)),
        "sech" => inverse(d, &d.cosh(bounded(one()?, 1000.0)?)),
        "coth" => inverse(d, &d.tanh(bounded(one()?, 1000.0)?)),
        "asinh" => Ok(d.asinh(one()?)),
        "acosh" => {
            let x = one()?;
            if x.lt(&Real::one()) {
                abort()
            } else {
                Ok(d.acosh(x))
            }
        }
        "atanh" => {
            let x = one()?;
            if x.abs().gte(&Real::one()) {
                abort()
            } else {
                Ok(d.atanh(x))
            }
        }
        "acsch" => Ok(d.asinh(&inverse(d, one()?)?)),
        "asech" => {
            let x = one()?;
            if x.lte(&Real::zero()) || x.gt(&Real::one()) {
                abort()
            } else {
                Ok(d.acosh(&inverse(d, x)?))
            }
        }
        "acoth" => {
            let x = one()?;
            if x.abs().lte(&Real::one()) {
                abort()
            } else {
                Ok(d.atanh(&inverse(d, x)?))
            }
        }
        _ => abort(),
    }
}

/// Same meaning, same input: only the engine's own float rounding may separate them. A real mismatch (a factor, a
/// sign, a branch) is far past this. Degree trig also carries the float error of converting to radians, which
/// grows with the angle; past a point it can't be checked.
fn agrees(engine: f64, twin: f64, name: &str, x: f64, mode: AngleMode) -> R<bool> {
    if engine == twin {
        return Ok(true);
    }
    let mut slack = 1e-12;
    // decimal.js reduces huge angles at working precision, so it can't see the double's low digits
    if is_trig_name(name) && x.abs() > 1e15 {
        return abort();
    }
    if mode == AngleMode::Deg && is_trig_name(name) {
        let convert =
            4e-16 * 1f64.max((x * std::f64::consts::PI / 180.0).abs()) * (1.0 + twin * twin);
        if convert > 1e-6 {
            return abort();
        }
        slack += convert;
    }
    let diff = (engine - twin).abs();
    Ok(diff <= 1e-9 * engine.abs().max(twin.abs()) + slack)
}

// ---------------------------------------------------------------------------------------------------------------
// the decimal side: pairs

/// Every value carried at two precisions: where they differ is how far the low one can be off.
#[derive(Clone, Debug)]
struct Pair {
    lo: Real,
    hi: Real,
}

fn noise(p: &Pair, lo: &Ctx) -> Real {
    lo.mul(&lo.sub(&p.lo, &p.hi).abs(), &num(1000.0))
}

/// An inexact value going into a jump (floor, round, mod, an integer test) that is a short decimal to within its
/// noise is taken as that decimal, so `sin(30)*2` meets floor as 1, not 0.99999…
fn settle(p: &Pair, lo: &Ctx, hi: &Ctx) -> Pair {
    if p.lo.eq_value(&p.hi) {
        return p.clone();
    }
    let room = noise(p, lo);
    let at = |c: Real| Pair {
        lo: c.clone(),
        hi: c,
    };
    if p.hi.abs().lte(&room) {
        return at(Real::zero());
    }
    if room.gt(&hi.mul(&p.hi.abs(), &num(1e-20))) {
        return p.clone();
    }
    for dp in 0..=hi.precision {
        let c = p.hi.to_dp(dp as isize, hi.rounding);
        if hi.sub(&c, &p.hi).abs().lte(&room) {
            return if c.is_integer() || c.sd() <= 15 {
                at(c)
            } else {
                p.clone()
            };
        }
    }
    p.clone()
}

/// A factorial or nCr runs a loop of multiplications: five of them to a unit of work.
fn product_work(name: &str, args: &[Pair]) -> i64 {
    let len = match name {
        "factorial" => match args.first() {
            Some(a) => a.lo.to_f64(),
            None => return 0,
        },
        "combinations" | "permutations" | "nCr" | "nPr" => {
            2.0 * args.get(1).map_or(0.0, |a| a.lo.to_f64())
        }
        _ => return 0,
    };
    if len.is_finite() && len > 0.0 {
        (len / 5.0).ceil() as i64
    } else {
        0
    }
}

struct BigBackend<'w> {
    lo: Ctx,
    hi: Ctx,
    check: Ctx,
    kl: Rc<Kit>,
    kh: Rc<Kit>,
    kc: Rc<Kit>,
    mode: AngleMode,
    scope: Scope,
    work: &'w mut i64,
    scale: &'w mut f64,
}

impl BigBackend<'_> {
    fn both(&self, f: impl Fn(&Ctx, &Kit, bool) -> R<Real>) -> R<Pair> {
        Ok(Pair {
            lo: f(&self.lo, &self.kl, false)?,
            hi: f(&self.hi, &self.kh, true)?,
        })
    }

    fn jump_args(&self, name: &str, args: Vec<Pair>) -> Vec<Pair> {
        if discontinuous(name) {
            args.iter().map(|a| settle(a, &self.lo, &self.hi)).collect()
        } else {
            args
        }
    }

    fn pair_mod(&self, args: &[Pair]) -> R<Pair> {
        let [a, b] = args else {
            return abort();
        };
        let r = Pair {
            lo: modulo(&self.lo, &a.lo, &b.lo)?,
            hi: modulo(&self.hi, &a.hi, &b.hi)?,
        };
        // the remainder inherits the divisor's noise times the quotient
        let h = &self.hi;
        let m = b.hi.abs();
        let slack = h.add(
            &h.mul(&h.div(&a.hi, &m).abs(), &noise(b, &self.lo)),
            &noise(a, &self.lo),
        );
        if slack.gt(&h.mul(&m, &num(1e-6))) {
            return abort();
        }
        if r.hi.lte(&slack) || h.sub(&m, &r.hi).lte(&slack) {
            Ok(Pair {
                lo: Real::zero(),
                hi: Real::zero(),
            })
        } else {
            Ok(r)
        }
    }

    fn big_op(d: &Ctx, op: Op, args: &[Real]) -> R<Real> {
        Ok(match (op, args) {
            (Op::Add, [a, b]) => d.add(a, b),
            (Op::Subtract, [a, b]) => d.sub(a, b),
            (Op::Multiply, [a, b]) => d.mul(a, b),
            (Op::Divide, [a, b]) => {
                if b.is_zero() {
                    return abort();
                }
                d.div(a, b)
            }
            (Op::Pow, [a, b]) => {
                if a.is_neg() && !b.is_integer() {
                    return abort();
                }
                if a.is_zero() {
                    if b.is_neg() {
                        return abort();
                    }
                    return Ok(if b.is_zero() { Real::one() } else { a.clone() });
                }
                if (b.to_f64() * a.to_f64().abs().log10()).abs() > 400.0 {
                    return abort();
                }
                d.pow(a, b)
            }
            (Op::UnaryMinus, [a]) => a.neg(),
            (Op::UnaryPlus, [a]) => a.clone(),
            (Op::Factorial, [a]) => factorial(d, a)?,
            _ => return abort(),
        })
    }
}

impl Backend for BigBackend<'_> {
    type T = Pair;

    fn lit(&mut self, v: &Real) -> R<Pair> {
        Ok(Pair {
            lo: v.clone(),
            hi: v.clone(),
        })
    }

    fn constant(&mut self, name: &str) -> R<Pair> {
        self.both(|d, k, _| {
            Ok(match name {
                "pi" => k.pi.clone(),
                "tau" => d.mul(&k.pi, &num(2.0)),
                _ => k.e.clone(),
            })
        })
    }

    fn op(&mut self, op: Op, args: Vec<Pair>) -> R<Pair> {
        if op == Op::Pow {
            *self.work -= CALL_COST;
        }
        let name = match op {
            Op::Mod => "mod",
            Op::Factorial => "factorial",
            _ => "",
        };
        let xs = self.jump_args(name, args);
        if op == Op::Mod {
            return self.pair_mod(&xs);
        }
        let lo: Vec<Real> = xs.iter().map(|a| a.lo.clone()).collect();
        let hi: Vec<Real> = xs.iter().map(|a| a.hi.clone()).collect();
        Ok(Pair {
            lo: Self::big_op(&self.lo, op, &lo)?,
            hi: Self::big_op(&self.hi, op, &hi)?,
        })
    }

    fn call(&mut self, name: &str, args: Vec<Pair>) -> R<Pair> {
        *self.work -= CALL_COST + product_work(name, &args);
        if *self.work < 0 {
            return abort();
        }
        let xs = self.jump_args(name, args);
        if name == "mod" {
            return if xs.len() == 2 {
                self.pair_mod(&xs)
            } else {
                abort()
            };
        }
        let lo: Vec<Real> = xs.iter().map(|a| a.lo.clone()).collect();
        let hi: Vec<Real> = xs.iter().map(|a| a.hi.clone()).collect();
        let out = self.both(|d, k, high| twin(name, d, if high { &hi } else { &lo }, k))?;
        if !discontinuous(name) {
            let nums: Vec<f64> = lo.iter().map(Real::to_f64).collect();
            if !nums.iter().all(|n| n.is_finite()) {
                return abort();
            }
            let reference = engine_call(name, &nums, &self.scope)?;
            let exact: Vec<Real> = nums.iter().map(|&x| exactly(x)).collect();
            let same = twin(name, &self.check, &exact, &self.kc)?.to_f64();
            let x = nums.first().copied().unwrap_or(f64::NAN);
            match reference {
                Some(r) if agrees(r, same, name, x, self.mode)? => {}
                _ => return Err(Abort::Meaning),
            }
        }
        Ok(out)
    }

    fn tick(&mut self, v: Pair) -> R<Pair> {
        if !v.lo.is_finite() || !v.hi.is_finite() {
            return abort();
        }
        *self.work -= 1;
        if *self.work < 0 {
            return abort();
        }
        let mag = v.hi.to_f64().abs();
        if mag > *self.scale {
            *self.scale = mag;
        }
        Ok(v)
    }
}

/// The digits of a double exactly, so the twin sees the very input the engine saw (`0` for −0, as the TS writes).
fn exactly(x: f64) -> Real {
    if x == 0.0 {
        Real::zero()
    } else {
        Real::from_f64_exact(x)
    }
}

/// The precise value of `tree`, climbing the ladder until the two precisions agree.
fn precise(
    tree: &Node,
    vars: &BTreeMap<String, f64>,
    f: f64,
    ctx: &ScientificContext,
    mode: AngleMode,
    work: &mut i64,
) -> Result<f64, Kept> {
    let mut scale = 0.0;
    let mut out: Option<(Pair, Ctx)> = None;
    for (low, high) in LADDER {
        let result = {
            let mut b = BigBackend {
                lo: dec(low),
                hi: dec(high),
                check: dec(CHECK),
                kl: kit(low, mode),
                kh: kit(high, mode),
                kc: kit(CHECK, mode),
                mode,
                scope: engine_scope(mode),
                work,
                scale: &mut scale,
            };
            let mut env = BTreeMap::new();
            let mut built = Ok(());
            for (k, &v) in vars {
                if !v.is_finite() {
                    built = abort();
                    break;
                }
                env.insert(
                    k.clone(),
                    Pair {
                        lo: from_number(v),
                        hi: from_number(v),
                    },
                );
            }
            built.and_then(|()| {
                walk(
                    tree,
                    &mut b,
                    &Env {
                        vars: env,
                        ctx,
                        depth: 0,
                    },
                )
            })
        };
        let p = match result {
            Ok(p) => p,
            Err(e) => {
                if *work < 0 {
                    return Err(Kept::Budget);
                }
                return Err(match e {
                    Abort::Meaning => Kept::Meaning,
                    Abort::Unsupported => Kept::Unsupported,
                });
            }
        };
        let h = dec(high);
        // rounding error shrinks with precision, so when the low precision agrees with the high one to 6 places,
        // the high one is good to far more places than a double holds
        if p.lo.eq_value(&p.hi)
            || h.sub(&p.hi, &p.lo)
                .abs()
                .lte(&h.mul(&p.hi.abs(), &num(1e-6)))
        {
            let n = p.hi.to_f64();
            if !n.is_finite() || (n == 0.0 && !p.hi.is_zero()) {
                return Err(Kept::Range);
            }
            return Ok(n + 0.0);
        }
        out = Some((p, dec(low)));
    }
    // zero within its own noise, and the float answer is rounding noise too
    if let Some((p, lo)) = out
        && p.hi.abs().lte(&noise(&p, &lo))
        && f.abs() <= 1e-10 * 1f64.max(scale)
    {
        return Ok(0.0);
    }
    Err(Kept::Unsettled)
}

fn sig_digits(n: f64) -> usize {
    let s = jsnum::to_string(n.abs());
    let s = s.split('e').next().unwrap_or("").replacen('.', "", 1);
    s.trim_start_matches('0').trim_end_matches('0').len()
}

/// Why the float answer stays, or what replaces it.
pub fn verdict(text: &str, value: &Value, ctx: &ScientificContext) -> Verdict {
    verdict_with_budget(text, value, ctx, BUDGET)
}

/// [`verdict`] with a work budget other than [`BUDGET`].
pub fn verdict_with_budget(
    text: &str,
    value: &Value,
    ctx: &ScientificContext,
    budget: i64,
) -> Verdict {
    if value.kind != ValueKind::Number || value.unit.is_some() || !value.n.is_finite() {
        return Verdict::Kept(Kept::NotANumber);
    }
    let mut work = budget;
    let mode = if ctx.angle_mode == AngleMode::Rad {
        AngleMode::Rad
    } else {
        AngleMode::Deg
    };
    let f = value.n;
    // `ans` stays a name, not pasted digits, so it can be told apart from a typed number
    let bare = ScientificContext {
        ans: None,
        ..ctx.clone()
    };
    let mut vars = ctx.variables.clone();
    if let Some(a) = ctx.ans {
        vars.insert("ans".into(), a);
    }
    let Some(expr) = preprocess_checked(text, &bare) else {
        return Verdict::Kept(Kept::Unsupported);
    };
    let Ok(tree) = parse(&expr) else {
        return Verdict::Kept(Kept::Unsupported);
    };
    let mut nb = NumberBackend {
        scope: engine_scope(mode),
    };
    let t = match walk(
        &tree,
        &mut nb,
        &Env {
            vars: vars.clone(),
            ctx: &bare,
            depth: 0,
        },
    ) {
        Ok(t) => t,
        Err(_) => return Verdict::Kept(Kept::Unsupported),
    };
    if !(t == f || (t - f).abs() <= 1e-12 * t.abs().max(f.abs())) {
        return Verdict::Kept(Kept::Meaning);
    }

    let n = match precise(&tree, &vars, f, &bare, mode, &mut work) {
        Ok(n) => n,
        Err(k) => return Verdict::Kept(k),
    };
    if n == f {
        return Verdict::Same;
    }
    // a computed input like 1/3 is only known to the last bit of its double; the correction must survive that bit
    // moving either way, or `a*3 - 1` would turn a right 0 into -1e-16
    let shaky: Vec<&String> = vars
        .iter()
        .filter(|(_, v)| sig_digits(**v) > 15)
        .map(|(k, _)| k)
        .collect();
    if !shaky.is_empty() {
        for dir in [1.0, -1.0] {
            let mut nudged = vars.clone();
            for k in &shaky {
                let v = vars[*k];
                nudged.insert((*k).clone(), v * (1.0 + dir * f64::EPSILON));
            }
            let m = match precise(&tree, &nudged, f, &bare, mode, &mut work) {
                Ok(m) => m,
                Err(k) => return Verdict::Kept(k),
            };
            if m != n && (m - n).abs() > 1e-13 * n.abs() {
                return Verdict::Kept(Kept::InexactInput);
            }
        }
    }
    Verdict::Corrected(n)
}

/// The float answer with any float artefact replaced by the true digits.
pub fn truthful(text: &str, value: Value, ctx: &ScientificContext) -> Value {
    match verdict(text, &value, ctx) {
        Verdict::Corrected(n) => Value { n, ..value },
        _ => value,
    }
}

/// The TS `Verdict` as JSON: `{kind: 'kept', reason}`, `{kind: 'same'}` or `{kind: 'corrected', n}`.
pub fn verdict_json(v: Verdict) -> serde_json::Value {
    match v {
        Verdict::Kept(k) => serde_json::json!({ "kind": "kept", "reason": k.reason() }),
        Verdict::Same => serde_json::json!({ "kind": "same" }),
        Verdict::Corrected(n) => serde_json::json!({ "kind": "corrected", "n": n }),
    }
}

/// A `ScientificContext` from its TS JSON (`{angleMode, variables, functions, ans}`).
fn context_json(v: &serde_json::Value) -> ScientificContext {
    let mut ctx = ScientificContext {
        angle_mode: serde_json::from_value(v["angleMode"].clone()).unwrap_or_default(),
        ans: v["ans"].as_f64(),
        ..ScientificContext::default()
    };
    if let Some(vars) = v["variables"].as_object() {
        for (k, n) in vars {
            ctx.variables.insert(
                k.clone(),
                crate::golden::f64_arg(std::slice::from_ref(n), 0).unwrap_or(f64::NAN),
            );
        }
    }
    if let Some(fns) = v["functions"].as_object() {
        for (k, f) in fns {
            if let Ok(def) = serde_json::from_value(f.clone()) {
                ctx.functions.insert(k.clone(), def);
            }
        }
    }
    ctx
}

/// Replays `verdict` and `truthful` fixtures (WP-PRECISE's golden arm).
pub fn golden_dispatch(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    let text = args.first()?.as_str()?.to_string();
    let value: Value = serde_json::from_value(crate::golden::decode(args.get(1)?)).ok()?;
    let ctx = context_json(args.get(2).unwrap_or(&serde_json::Value::Null));
    match name {
        "verdict" => {
            let budget = crate::golden::f64_arg(args, 3).map_or(BUDGET, |b| b as i64);
            Some(verdict_json(verdict_with_budget(
                &text, &value, &ctx, budget,
            )))
        }
        "truthful" => serde_json::to_value(truthful(&text, value, &ctx)).ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// exact rationals, for callers that want them

/// A decimal literal exactly: `0.1` is 1/10, `2.5e-3` is 1/400.
pub fn parse_decimal(text: &str) -> Option<BigRational> {
    let t = text.trim();
    let (mant, exp) = match t.find(['e', 'E']) {
        Some(at) => (&t[..at], t[at + 1..].parse::<i64>().ok()?),
        None => (t, 0),
    };
    let (neg, mant) = match mant.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mant.strip_prefix('+').unwrap_or(mant)),
    };
    let (whole, frac) = mant.split_once('.').unwrap_or((mant, ""));
    if whole.is_empty() && frac.is_empty()
        || !whole
            .bytes()
            .chain(frac.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{frac}");
    let int = BigInt::parse_bytes(
        if digits.is_empty() {
            b"0"
        } else {
            digits.as_bytes()
        },
        10,
    )?;
    let scale = exp - frac.len() as i64;
    if scale.abs() > 100_000 {
        return None;
    }
    let ten = BigInt::from(10u32);
    let r = if scale >= 0 {
        BigRational::from_integer(int * ten.pow(scale as u32))
    } else {
        BigRational::new(int, ten.pow((-scale) as u32))
    };
    Some(if neg { -r } else { r })
}

/// The double nearest `r` (ties to even), as `Number(decimal)` gives; ±∞ past the range.
pub fn to_f64(r: &BigRational) -> f64 {
    if r.is_zero() {
        return 0.0;
    }
    let neg = r.is_negative();
    let num = r.numer().abs().to_biguint().unwrap_or_default();
    let den = r.denom().abs().to_biguint().unwrap_or_default();
    let v = ratio_to_f64(&num, &den);
    if neg { -v } else { v }
}

fn ratio_to_f64(num: &BigUint, den: &BigUint) -> f64 {
    // scale so the quotient has 54+ bits: one to round on, the rest exact, plus a sticky remainder
    let shift = den.bits() as i64 - num.bits() as i64 + 55;
    let (n, d) = if shift >= 0 {
        (num << shift as usize, den.clone())
    } else {
        (num.clone(), den << (-shift) as usize)
    };
    let (q, rem) = n.div_rem(&d);
    // value = q * 2^-shift, with q having 55 or 56 bits
    let bits = q.bits() as i64;
    let exp = bits - 1 - shift; // the value's binary exponent
    if exp > 1023 {
        return f64::INFINITY;
    }
    // how many low bits of q fall below the double's precision (more for subnormals)
    let keep = if exp < -1022 { 53 - (-1022 - exp) } else { 53 };
    if keep <= 0 {
        // below half the smallest subnormal rounds to 0; exactly half rounds to even (0)
        let half = keep == 0 && (q > (BigUint::one() << (bits - 1) as usize) || !rem.is_zero());
        return if half { f64::from_bits(1) } else { 0.0 };
    }
    let drop = (bits - keep) as usize;
    let mut m = &q >> drop;
    let low = &q - (&m << drop);
    let half = BigUint::one() << (drop - 1);
    let round_up = low > half || (low == half && (!rem.is_zero() || m.is_odd()));
    if round_up {
        m += 1u32;
    }
    let m = m.to_f64().unwrap_or(f64::INFINITY);
    let scale = drop as i64 - shift;
    m * 2f64.powi((scale / 2) as i32) * 2f64.powi((scale - scale / 2) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> BigRational {
        parse_decimal(s).unwrap()
    }

    #[test]
    fn decimals_parse_exactly() {
        assert_eq!(r("0.1"), BigRational::new(1.into(), 10.into()));
        assert_eq!(r(".5"), BigRational::new(1.into(), 2.into()));
        assert_eq!(r("5."), BigRational::from_integer(5.into()));
        assert_eq!(r("2.5e-3"), BigRational::new(1.into(), 400.into()));
        assert_eq!(r("1E5"), BigRational::from_integer(100_000.into()));
        assert!(parse_decimal("0xff").is_none());
    }

    #[test]
    fn rationals_round_to_the_nearest_double() {
        for s in [
            "0.1",
            "0.3",
            "1e-7",
            "123456789.123456789",
            "1e300",
            "5e-324",
            "2.2250738585072014e-308",
            "0.30000000000000004",
        ] {
            assert_eq!(to_f64(&r(s)), s.parse::<f64>().unwrap(), "{s}");
        }
        assert_eq!(to_f64(&BigRational::new(1.into(), 3.into())), 1.0 / 3.0);
        assert_eq!(to_f64(&r("1e400")), f64::INFINITY);
        assert_eq!(to_f64(&r("1e-400")), 0.0);
    }

    fn v(n: f64) -> Value {
        Value::number(n)
    }

    #[test]
    fn corrects_float_artefacts() {
        let ctx = ScientificContext::default();
        assert_eq!(
            verdict("0.1+0.2", &v(0.1 + 0.2), &ctx),
            Verdict::Corrected(0.3)
        );
        assert_eq!(verdict("2+2", &v(4.0), &ctx), Verdict::Same);
        assert_eq!(
            verdict("1e16+1-1e16", &v(0.0), &ctx),
            Verdict::Corrected(1.0)
        );
        // the TS is exact only to 48 digits, then 80: past that the float answer stands
        assert_eq!(verdict("1e100+1-1e100", &v(0.0), &ctx), Verdict::Same);
        let pow = 1.000_000_000_000_001_f64.powf(1e15);
        assert_eq!(
            verdict("(1+1e-15)^(1e15)", &v(pow), &ctx),
            Verdict::Corrected(2.718_281_828_459_043_8)
        );
        let third = ScientificContext {
            ans: Some(1.0 / 3.0),
            ..ScientificContext::default()
        };
        // String(1/3) times 3 is 0.9999999999999999, and nudging ans moves it less than 1e-13
        assert_eq!(
            verdict("ans*3", &v(1.0), &third),
            Verdict::Corrected(0.9999999999999999)
        );
    }

    #[test]
    fn keeps_what_it_cannot_check() {
        let ctx = ScientificContext::default();
        assert_eq!(
            verdict_with_budget("1/3", &v(1.0 / 3.0), &ctx, -1),
            Verdict::Kept(Kept::Budget)
        );
        assert_eq!(verdict("2+2", &v(5.0), &ctx), Verdict::Kept(Kept::Meaning));
        assert_eq!(
            verdict("3.5!", &v(11.631728396567446), &ctx),
            Verdict::Kept(Kept::Unsupported)
        );
    }

    #[test]
    fn hard_inputs_finish_quickly() {
        let ctx = ScientificContext::default();
        let start = std::time::Instant::now();
        for (text, f) in [
            ("1e-999999999+1", 1.0),
            ("floor(1e-999999999)", 0.0),
            ("170!", 7.257_415_615_307_998e306),
            ("1e300*1e300/1e300", 1e300),
            ("2^0.5^0.5^0.5", 1.5),
            ("(1+1e-15)^(1e15)", 3.035),
            ("0.1^300", 1e-300),
        ] {
            let _ = verdict(text, &v(f), &ctx);
        }
        assert!(start.elapsed().as_secs() < 5);
    }

    #[test]
    fn settles_jumps_at_short_decimals() {
        let (lo, hi) = (dec(32), dec(48));
        let third = Pair {
            lo: lo.div(&Real::one(), &num(3.0)),
            hi: hi.div(&Real::one(), &num(3.0)),
        };
        let nine = Pair {
            lo: lo.mul(&third.lo, &num(3.0)),
            hi: hi.mul(&third.hi, &num(3.0)),
        };
        let s = settle(&nine, &lo, &hi);
        assert_eq!(s.hi.to_string(), "1");
        assert_eq!(
            settle(&third, &lo, &hi).hi.to_string(),
            third.hi.to_string()
        );
    }
}
