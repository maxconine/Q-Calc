//! Closed forms of calculus answers (`src/engine/closedForm.ts`): the job's exact value at 40 digits, then PSLQ
//! against families of constants at 24 digits, verified at 40. decimal.js is [`Real`] here, each
//! `Decimal.clone({precision, rounding: ROUND_HALF_EVEN})` a [`Ctx`].
//!
//! The TS search stops at a wall-clock deadline (`identifyJob(job, budgetMs = 8000)`). This port counts the same
//! ticks the TS checks the clock at (one per quadrature node pair and one per PSLQ iteration) against a fixed
//! budget, so an answer never depends on how busy the machine is. Each tick is also a [`crate::work::spend`], so
//! a cancelled search unwinds at the next one.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{Value as Json, json};

use crate::jsnum;
use crate::precise::real::{Ctx, Real, Rounding};

/// An expression in radians, as `calculus.ts` hands it to the search.
#[derive(Clone, Debug, PartialEq)]
pub enum Ast {
    Num(String),
    Var,
    Const(String),
    Op(String, Vec<Ast>),
    Fn(String, Vec<Ast>),
}

/// A bound: an expression, or ±∞.
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    Ast(Ast),
    Inf,
    NegInf,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobKind {
    Integral { a: Bound, b: Bound },
    Limit { to: Bound, side: i32 },
    Value { at: Ast },
}

/// `ClosedFormJob`: the expression, what to do with it, and the double answer it must agree with.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub f: Ast,
    pub kind: JobKind,
    pub approx: f64,
    pub err: f64,
    pub rad: bool,
}

// ---- JSON, in the TS's key order

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

impl Ast {
    pub fn to_json(&self) -> Json {
        match self {
            Ast::Num(v) => json!({"t": "num", "v": v}),
            Ast::Var => json!({"t": "var"}),
            Ast::Const(n) => json!({"t": "const", "name": n}),
            Ast::Op(o, a) => json!({"t": "op", "op": o, "a": a.iter().map(Ast::to_json).collect::<Vec<_>>()}),
            Ast::Fn(n, a) => json!({"t": "fn", "name": n, "a": a.iter().map(Ast::to_json).collect::<Vec<_>>()}),
        }
    }

    /// `JSON.stringify`, keys in the order the TS object literals write them.
    pub fn to_js(&self) -> String {
        let list = |a: &[Ast]| a.iter().map(Ast::to_js).collect::<Vec<_>>().join(",");
        match self {
            Ast::Num(v) => format!("{{\"t\":\"num\",\"v\":{}}}", js_string(v)),
            Ast::Var => "{\"t\":\"var\"}".into(),
            Ast::Const(n) => format!("{{\"t\":\"const\",\"name\":{}}}", js_string(n)),
            Ast::Op(o, a) => format!("{{\"t\":\"op\",\"op\":{},\"a\":[{}]}}", js_string(o), list(a)),
            Ast::Fn(n, a) => format!("{{\"t\":\"fn\",\"name\":{},\"a\":[{}]}}", js_string(n), list(a)),
        }
    }

    pub fn from_json(j: &Json) -> Option<Ast> {
        let kids = || -> Option<Vec<Ast>> { j.get("a")?.as_array()?.iter().map(Ast::from_json).collect() };
        Some(match j.get("t")?.as_str()? {
            "num" => Ast::Num(match j.get("v")? {
                Json::String(s) => s.clone(),
                Json::Number(n) => jsnum::to_string(n.as_f64()?),
                _ => return None,
            }),
            "var" => Ast::Var,
            "const" => Ast::Const(j.get("name")?.as_str()?.to_string()),
            "op" => Ast::Op(j.get("op")?.as_str()?.to_string(), kids()?),
            "fn" => Ast::Fn(j.get("name")?.as_str()?.to_string(), kids()?),
            _ => return None,
        })
    }
}

impl Bound {
    fn to_json(&self) -> Json {
        match self {
            Bound::Ast(a) => a.to_json(),
            Bound::Inf => json!("inf"),
            Bound::NegInf => json!("-inf"),
        }
    }

    fn to_js(&self) -> String {
        match self {
            Bound::Ast(a) => a.to_js(),
            Bound::Inf => "\"inf\"".into(),
            Bound::NegInf => "\"-inf\"".into(),
        }
    }

    fn from_json(j: &Json) -> Option<Bound> {
        match j.as_str() {
            Some("inf") => Some(Bound::Inf),
            Some("-inf") => Some(Bound::NegInf),
            Some(_) => None,
            None => Ast::from_json(j).map(Bound::Ast),
        }
    }
}

/// A recorder number (`{"$f": "NaN"}` and friends included).
fn num_of(j: Option<&Json>) -> Option<f64> {
    match j? {
        Json::Number(n) => n.as_f64(),
        Json::Object(o) => match o.get("$f")?.as_str()? {
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "-0" => Some(-0.0),
            _ => Some(f64::NAN),
        },
        _ => None,
    }
}

impl Job {
    pub fn to_json(&self) -> Json {
        let mut m = serde_json::Map::new();
        match &self.kind {
            JobKind::Integral { a, b } => {
                m.insert("kind".into(), json!("integral"));
                m.insert("a".into(), a.to_json());
                m.insert("b".into(), b.to_json());
            }
            JobKind::Limit { to, side } => {
                m.insert("kind".into(), json!("limit"));
                m.insert("to".into(), to.to_json());
                m.insert("side".into(), json!(side));
            }
            JobKind::Value { at } => {
                m.insert("kind".into(), json!("value"));
                m.insert("at".into(), at.to_json());
            }
        }
        m.insert("f".into(), self.f.to_json());
        m.insert("approx".into(), json!(self.approx));
        m.insert("err".into(), json!(self.err));
        m.insert("rad".into(), json!(self.rad));
        Json::Object(m)
    }

    pub fn from_json(j: &Json) -> Option<Job> {
        let f = Ast::from_json(j.get("f")?)?;
        let kind = match j.get("kind")?.as_str()? {
            "integral" => JobKind::Integral {
                a: Bound::from_json(j.get("a")?)?,
                b: Bound::from_json(j.get("b")?)?,
            },
            "limit" => JobKind::Limit {
                to: Bound::from_json(j.get("to")?)?,
                side: num_of(j.get("side")).unwrap_or(0.0) as i32,
            },
            "value" => JobKind::Value { at: Ast::from_json(j.get("at")?)? },
            _ => return None,
        };
        Some(Job {
            f,
            kind,
            approx: num_of(j.get("approx")).unwrap_or(f64::NAN),
            err: num_of(j.get("err")).unwrap_or(f64::NAN),
            rad: j.get("rad").and_then(Json::as_bool).unwrap_or(false),
        })
    }

    /// `jobKey`: the job as JSON without `approx` and `err`, keys in the TS's order.
    pub fn key(&self) -> String {
        let body = match &self.kind {
            JobKind::Integral { a, b } => format!(
                "\"kind\":\"integral\",\"f\":{},\"a\":{},\"b\":{}",
                self.f.to_js(),
                a.to_js(),
                b.to_js()
            ),
            JobKind::Limit { to, side } => format!(
                "\"kind\":\"limit\",\"f\":{},\"to\":{},\"side\":{side}",
                self.f.to_js(),
                to.to_js()
            ),
            JobKind::Value { at } => {
                format!("\"kind\":\"value\",\"f\":{},\"at\":{}", self.f.to_js(), at.to_js())
            }
        };
        format!("{{{body},\"rad\":{}}}", self.rad)
    }
}

// ---- work

/// Digits the relation search uses, and the stricter digits a found relation must still hold to.
const SEARCH_DIGITS: usize = 24;
const VERIFY_DIGITS: usize = 40;

/// Ticks the default search may take, in place of the TS's 8000 ms. Measured on the fixtures: the slowest answer
/// the TS finds in time (`∫0..∞ x³/(eˣ - 1)`, π⁴/15) needs a few thousand.
pub const DEFAULT_TICKS: u64 = 200_000;

/// Ticks per millisecond when a caller gives the TS's millisecond budget.
pub const TICKS_PER_MS: u64 = 25;

struct OutOfWork;

struct Clock {
    left: u64,
}

impl Clock {
    fn tick(&mut self) -> Result<(), OutOfWork> {
        crate::work::spend(1);
        if self.left == 0 {
            return Err(OutOfWork);
        }
        self.left -= 1;
        Ok(())
    }
}

type R<T> = Result<T, OutOfWork>;

fn ctx(precision: usize) -> Ctx {
    Ctx::new(precision).with_rounding(Rounding::HalfEven)
}

fn int(n: i64) -> Real {
    Real::from_i64(n)
}

fn pow10(c: &Ctx, e: i64) -> Real {
    c.pow(&int(10), &int(e))
}

// ---- compiled expressions

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fun {
    Sin,
    Cos,
    Tan,
    Sec,
    Csc,
    Cot,
    Asin,
    Acos,
    Atan,
    Asec,
    Acsc,
    Acot,
    Sinh,
    Cosh,
    Tanh,
    Sech,
    Csch,
    Coth,
    Asinh,
    Acosh,
    Atanh,
    Sqrt,
    Cbrt,
    NthRoot,
    Exp,
    Log,
    Log10,
    Abs,
}

enum Expr {
    Val(Real),
    Var,
    Op(Op, Box<Expr>, Option<Box<Expr>>),
    Fn(Fun, Box<Expr>, Option<Box<Expr>>),
}

fn compile(ast: &Ast, c: &Ctx) -> Option<Expr> {
    let two = |a: &[Ast]| -> Option<(Box<Expr>, Option<Box<Expr>>)> {
        let first = Box::new(compile(a.first()?, c)?);
        let second = match a.get(1) {
            Some(b) => Some(Box::new(compile(b, c)?)),
            None => None,
        };
        Some((first, second))
    };
    Some(match ast {
        Ast::Num(v) => Expr::Val(Real::parse(v)?),
        Ast::Var => Expr::Var,
        Ast::Const(n) => Expr::Val(if n == "pi" { c.pi() } else { c.e() }),
        Ast::Op(o, a) => {
            let op = match o.as_str() {
                "+" => Op::Add,
                "-" => Op::Sub,
                "*" => Op::Mul,
                "/" => Op::Div,
                "^" => Op::Pow,
                "neg" => Op::Neg,
                _ => return None,
            };
            let (x, y) = two(a)?;
            // a binary operator missing its second argument throws in the TS
            if op != Op::Neg && y.is_none() {
                return None;
            }
            Expr::Op(op, x, y)
        }
        Ast::Fn(name, a) => {
            let f = match name.as_str() {
                "sin" => Fun::Sin,
                "cos" => Fun::Cos,
                "tan" => Fun::Tan,
                "sec" => Fun::Sec,
                "csc" => Fun::Csc,
                "cot" => Fun::Cot,
                "asin" => Fun::Asin,
                "acos" => Fun::Acos,
                "atan" => Fun::Atan,
                "asec" => Fun::Asec,
                "acsc" => Fun::Acsc,
                "acot" => Fun::Acot,
                "sinh" => Fun::Sinh,
                "cosh" => Fun::Cosh,
                "tanh" => Fun::Tanh,
                "sech" => Fun::Sech,
                "csch" => Fun::Csch,
                "coth" => Fun::Coth,
                "asinh" => Fun::Asinh,
                "acosh" => Fun::Acosh,
                "atanh" => Fun::Atanh,
                "sqrt" => Fun::Sqrt,
                "cbrt" => Fun::Cbrt,
                "nthRoot" => Fun::NthRoot,
                "exp" => Fun::Exp,
                "log" => Fun::Log,
                "log10" => Fun::Log10,
                "abs" => Fun::Abs,
                _ => return None,
            };
            let (x, y) = two(a)?;
            Expr::Fn(f, x, y)
        }
    })
}

/// `power`: 0 to a positive power is 0, otherwise 0^x and a negative base to a fraction are NaN.
fn power(c: &Ctx, base: &Real, ex: &Real) -> Real {
    if base.is_zero() {
        return if ex.is_pos() && !ex.is_zero() { Real::zero() } else { Real::NAN };
    }
    if base.is_neg() && !ex.is_integer() {
        return Real::NAN;
    }
    c.pow(base, ex)
}

impl Expr {
    fn eval(&self, c: &Ctx, x: &Real) -> Real {
        match self {
            Expr::Val(v) => v.clone(),
            Expr::Var => x.clone(),
            Expr::Op(op, a, b) => {
                let b = b.as_deref();
                match op {
                    Op::Add => c.add(&a.eval(c, x), &b.map_or(Real::NAN, |b| b.eval(c, x))),
                    Op::Sub => c.sub(&a.eval(c, x), &b.map_or(Real::NAN, |b| b.eval(c, x))),
                    Op::Mul => {
                        // e^(-x) sin(x) far out: skip the costly sin of a huge argument once e^(-x) is 0
                        let l = a.eval(c, x);
                        if l.is_zero() {
                            l
                        } else {
                            c.mul(&l, &b.map_or(Real::NAN, |b| b.eval(c, x)))
                        }
                    }
                    Op::Div => {
                        let d = b.map_or(Real::NAN, |b| b.eval(c, x));
                        if d.is_zero() { Real::NAN } else { c.div(&a.eval(c, x), &d) }
                    }
                    Op::Pow => power(c, &a.eval(c, x), &b.map_or(Real::NAN, |b| b.eval(c, x))),
                    Op::Neg => a.eval(c, x).neg(),
                }
            }
            Expr::Fn(f, a, b) => {
                let v = a.eval(c, x);
                let one = Real::one();
                let inv = |v: &Real| c.div(&one, v);
                let guard = |ok: bool, r: &dyn Fn() -> Real| if ok { r() } else { Real::NAN };
                match f {
                    Fun::Sin => c.sin(&v),
                    Fun::Cos => c.cos(&v),
                    Fun::Tan => c.tan(&v),
                    Fun::Sec => inv(&c.cos(&v)),
                    Fun::Csc => inv(&c.sin(&v)),
                    Fun::Cot => inv(&c.tan(&v)),
                    Fun::Asin => guard(v.abs().lte(&one), &|| c.asin(&v)),
                    Fun::Acos => guard(v.abs().lte(&one), &|| c.acos(&v)),
                    Fun::Atan => c.atan(&v),
                    Fun::Asec => guard(v.abs().gte(&one), &|| c.acos(&inv(&v))),
                    Fun::Acsc => guard(v.abs().gte(&one), &|| c.asin(&inv(&v))),
                    Fun::Acot => {
                        if v.is_zero() {
                            c.div(&c.pi(), &int(2))
                        } else {
                            c.atan(&inv(&v))
                        }
                    }
                    Fun::Sinh => c.sinh(&v),
                    Fun::Cosh => c.cosh(&v),
                    Fun::Tanh => c.tanh(&v),
                    Fun::Sech => inv(&c.cosh(&v)),
                    Fun::Csch => guard(!v.is_zero(), &|| inv(&c.sinh(&v))),
                    Fun::Coth => guard(!v.is_zero(), &|| inv(&c.tanh(&v))),
                    Fun::Asinh => c.asinh(&v),
                    Fun::Acosh => guard(v.gte(&one), &|| c.acosh(&v)),
                    Fun::Atanh => guard(v.abs().lt(&one), &|| c.atanh(&v)),
                    Fun::Sqrt => guard(!v.is_neg(), &|| c.sqrt(&v)),
                    Fun::Cbrt => c.cbrt(&v),
                    Fun::NthRoot => {
                        let n = b.as_deref().map_or(int(2), |b| b.eval(c, x));
                        if !n.is_integer() || n.is_zero() {
                            return Real::NAN;
                        }
                        if v.is_neg() {
                            if c.modulo(&n, &int(2)).is_zero() {
                                Real::NAN
                            } else {
                                c.pow(&v.neg(), &inv(&n)).neg()
                            }
                        } else {
                            c.pow(&v, &inv(&n))
                        }
                    }
                    Fun::Exp => c.exp(&v),
                    Fun::Log => guard(v.is_pos() && !v.is_zero(), &|| c.ln(&v)),
                    Fun::Log10 => guard(v.is_pos() && !v.is_zero(), &|| c.log10(&v)),
                    Fun::Abs => v.abs(),
                }
            }
        }
    }
}

// ---- quadrature

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RuleKind {
    Tanh,
    Exp,
    Sinh,
}

/// A node of a rule on its standard range, before the affine map onto the job's bounds.
#[derive(Clone)]
struct Std {
    p: Real,
    q: Real,
    w: Real,
}

thread_local! {
    static TABLES: RefCell<HashMap<(RuleKind, usize, u32), Vec<Std>>> = RefCell::new(HashMap::new());
}

/// Standard nodes at t = k·2^-level for k ≥ 1 (odd k only past level 0), built from exp alone.
fn std_node(kind: RuleKind, w: usize, level: u32, i: usize) -> Std {
    TABLES.with(|t| {
        let mut t = t.borrow_mut();
        let list = t.entry((kind, w, level)).or_default();
        while list.len() <= i {
            let c = ctx(w);
            let half_pi = c.div(&c.pi(), &int(2));
            let k = if level == 0 { list.len() + 1 } else { 2 * list.len() + 1 };
            let tt = c.div(&int(k as i64), &Real::from_f64(2f64.powi(level as i32)));
            let et = c.exp(&tt);
            let one = Real::one();
            let sh = c.div(&c.sub(&et, &c.div(&one, &et)), &int(2));
            let ch = c.div(&c.add(&et, &c.div(&one, &et)), &int(2));
            let eu = c.exp(&c.mul(&half_pi, &sh));
            let ieu = c.div(&one, &eu);
            list.push(match kind {
                RuleKind::Tanh => {
                    let cosh_u = c.div(&c.add(&eu, &ieu), &int(2));
                    Std {
                        p: c.div(&one, &c.add(&c.mul(&eu, &eu), &one)),
                        q: Real::zero(),
                        w: c.div(&c.mul(&half_pi, &ch), &c.mul(&cosh_u, &cosh_u)),
                    }
                }
                RuleKind::Exp => Std { p: eu, q: ieu, w: c.mul(&half_pi, &ch) },
                RuleKind::Sinh => Std {
                    p: c.div(&c.sub(&eu, &ieu), &int(2)),
                    q: Real::zero(),
                    w: c.mul(&c.mul(&half_pi, &ch), &c.div(&c.add(&eu, &ieu), &int(2))),
                },
            });
        }
        list[i].clone()
    })
}

#[derive(Clone)]
struct QNode {
    x: Real,
    w: Real,
}

enum RuleMap {
    Tanh { a: Real, b: Real, r: Real, floor: Real },
    Exp { a: Real, flip: bool, floor: Real },
    Sinh,
}

struct Rule {
    kind: RuleKind,
    center: QNode,
    map: RuleMap,
}

impl Rule {
    /// Both mirrored nodes of a standard node, mapped onto the job's range; None once past the working precision.
    fn nodes(&self, c: &Ctx, n: &Std) -> [Option<QNode>; 2] {
        match &self.map {
            RuleMap::Tanh { a, b, r, floor } => {
                let dist = c.mul(&c.mul(r, &int(2)), &n.p);
                if dist.lt(floor) {
                    return [None, None];
                }
                let w = c.mul(r, &n.w);
                [
                    Some(QNode { x: c.sub(b, &dist), w: w.clone() }),
                    Some(QNode { x: c.add(a, &dist), w }),
                ]
            }
            RuleMap::Exp { a, flip, floor } => {
                let at = |y: &Real| if *flip { c.sub(a, y) } else { c.add(a, y) };
                [
                    Some(QNode { x: at(&n.p), w: c.mul(&n.w, &n.p) }),
                    if n.q.lt(floor) { None } else { Some(QNode { x: at(&n.q), w: c.mul(&n.w, &n.q) }) },
                ]
            }
            RuleMap::Sinh => [
                Some(QNode { x: n.p.clone(), w: n.w.clone() }),
                Some(QNode { x: n.p.neg(), w: n.w.clone() }),
            ],
        }
    }
}

fn tanh_sinh(a: Real, b: Real, c: &Ctx, w: usize) -> Rule {
    let half_pi = c.div(&c.pi(), &int(2));
    let r = c.div(&c.sub(&b, &a), &int(2));
    let floor = c.mul(&pow10(c, -(w as i64)), &Real::max_of(&[a.abs(), b.abs(), Real::one()]));
    Rule {
        kind: RuleKind::Tanh,
        center: QNode { x: c.div(&c.add(&a, &b), &int(2)), w: c.mul(&r, &half_pi) },
        map: RuleMap::Tanh { a, b, r, floor },
    }
}

/// [a, ∞) as x = a + y, or (-∞, a] as x = a - y, with y = exp(π/2 sinh t).
fn exp_sinh(a: Real, flip: bool, c: &Ctx, w: usize) -> Rule {
    let half_pi = c.div(&c.pi(), &int(2));
    let floor = c.mul(&pow10(c, -(w as i64)), &Real::max_of(&[a.abs(), Real::one()]));
    let x = if flip { c.sub(&a, &Real::one()) } else { c.add(&a, &Real::one()) };
    Rule { kind: RuleKind::Exp, center: QNode { x, w: half_pi }, map: RuleMap::Exp { a, flip, floor } }
}

fn sinh_sinh(c: &Ctx) -> Rule {
    let half_pi = c.div(&c.pi(), &int(2));
    Rule { kind: RuleKind::Sinh, center: QNode { x: Real::zero(), w: half_pi }, map: RuleMap::Sinh }
}

/// Refines until two levels agree; the error roughly squares each level.
fn quadrature(f: &Expr, rule: &Rule, c: &Ctx, digits: usize, w: usize, clock: &mut Clock) -> R<Option<Real>> {
    let tiny = pow10(c, -(w as i64));
    let cv = f.eval(c, &rule.center.x);
    if !cv.is_finite() {
        return Ok(None);
    }
    let mut raw = c.mul(&rule.center.w, &cv);
    let mut l1 = raw.abs();
    let mut level = |lv: u32, raw: &mut Real, l1: &mut Real| -> R<bool> {
        let mut done = [false, false];
        let mut small = [0, 0];
        let mut i = 0usize;
        while !(done[0] && done[1]) {
            clock.tick()?;
            let k = if lv == 0 { i + 1 } else { 2 * i + 1 };
            if k as f64 / 2f64.powi(lv as i32) > 9.0 {
                return Ok(false);
            }
            let pair = rule.nodes(c, &std_node(rule.kind, w, lv, i));
            for s in 0..2 {
                if done[s] {
                    continue;
                }
                let Some(n) = &pair[s] else {
                    done[s] = true;
                    continue;
                };
                let fx = f.eval(c, &n.x);
                if !fx.is_finite() {
                    return Ok(false);
                }
                let term = c.mul(&n.w, &fx);
                *raw = c.add(raw, &term);
                *l1 = c.add(l1, &term.abs());
                if term.abs().lte(&c.mul(&tiny, l1)) {
                    small[s] += 1;
                    if small[s] >= 2 && k as u64 > 1u64 << lv {
                        done[s] = true;
                    }
                } else {
                    small[s] = 0;
                }
            }
            i += 1;
        }
        Ok(true)
    };
    if !level(0, &mut raw, &mut l1)? {
        return Ok(None);
    }
    let mut h = Real::one();
    let mut prev = raw.clone();
    let mut prev_digits = 0.0;
    for lv in 1..=9 {
        h = c.div(&h, &int(2));
        if !level(lv, &mut raw, &mut l1)? {
            return Ok(None);
        }
        let value = c.mul(&raw, &h);
        let diff = c.sub(&value, &prev).abs();
        let scale = c.mul(&l1, &h);
        prev = value.clone();
        if diff.is_zero() {
            return Ok(Some(value));
        }
        let agreed = -c.log(&c.div(&diff, &scale), Some(&int(10))).to_f64();
        let d = digits as f64;
        if lv >= 3 && (agreed >= d || (agreed >= d / 2.0 + 2.0 && agreed >= 1.6 * prev_digits)) {
            return Ok(Some(value));
        }
        prev_digits = agreed;
    }
    Ok(None)
}

enum B {
    V(Real),
    Inf,
    NegInf,
}

fn bound_of(b: &Bound, c: &Ctx) -> Option<B> {
    match b {
        Bound::Inf => Some(B::Inf),
        Bound::NegInf => Some(B::NegInf),
        Bound::Ast(a) => {
            let v = compile(a, c)?.eval(c, &Real::zero());
            v.is_finite().then_some(B::V(v))
        }
    }
}

fn rank(b: &B) -> i32 {
    match b {
        B::Inf => 1,
        B::NegInf => -1,
        B::V(_) => 0,
    }
}

fn integral_at(f: &Ast, a: &Bound, b: &Bound, digits: usize, clock: &mut Clock) -> R<Option<Real>> {
    // a 1/√ singularity at a nonzero end loses half the working digits to the sliver past the last node
    let w = 2 * digits + 8;
    let c = ctx(w);
    let Some(fx) = compile(f, &c) else { return Ok(None) };
    let (Some(lo), Some(hi)) = (bound_of(a, &c), bound_of(b, &c)) else {
        return Ok(None);
    };
    let swapped = rank(&lo) > rank(&hi)
        || matches!((&lo, &hi), (B::V(x), B::V(y)) if x.gt(y));
    if swapped {
        return Ok(integral_at(f, b, a, digits, clock)?.map(|v| v.neg()));
    }
    let rule = match (lo, hi) {
        (B::NegInf, B::Inf) => sinh_sinh(&c),
        (B::V(x), B::Inf) => exp_sinh(x, false, &c, w),
        (B::NegInf, B::V(y)) => exp_sinh(y, true, &c, w),
        (B::V(x), B::V(y)) => {
            if x.eq_value(&y) {
                return Ok(Some(Real::zero()));
            }
            tanh_sinh(x, y, &c, w)
        }
        _ => return Ok(None),
    };
    quadrature(&fx, &rule, &c, digits, w, clock)
}

fn limit_at(f: &Ast, to: &Bound, side: i32, digits: usize) -> Option<Real> {
    let w = 2 * digits + 16;
    let c = ctx(w);
    let fx = compile(f, &c)?;
    let to = bound_of(to, &c)?;
    let h0 = pow10(&c, -((digits as f64 / 3.0).ceil() as i64) - 2);
    // richardson over h, h/2, h/4, h/8 removes the h, h², h³ terms
    let extrapolated = |g: &dyn Fn(&Real) -> Real| -> Option<Real> {
        let mut row = Vec::with_capacity(4);
        for i in 0..4 {
            let v = g(&c.div(&h0, &int(1 << i)));
            if !v.is_finite() {
                return None;
            }
            row.push(v);
        }
        for j in 1..4 {
            let fac = int(1 << j);
            let fac1 = int((1 << j) - 1);
            row = (0..row.len() - 1)
                .map(|i| c.div(&c.sub(&c.mul(&row[i + 1], &fac), &row[i]), &fac1))
                .collect();
        }
        row.into_iter().next()
    };
    match to {
        B::Inf | B::NegInf => {
            let s = int(if matches!(to, B::Inf) { 1 } else { -1 });
            extrapolated(&|h| fx.eval(&c, &c.div(&s, h)))
        }
        B::V(t) => {
            let sides: Vec<i64> = if side == 0 { vec![1, -1] } else { vec![i64::from(side)] };
            let mut vals = Vec::new();
            for s in sides {
                vals.push(extrapolated(&|h| fx.eval(&c, &c.add(&t, &c.mul(h, &int(s)))))?);
            }
            if vals.len() == 2 {
                let gap = c.sub(&vals[0], &vals[1]).abs();
                let tol = c.mul(&pow10(&c, -(digits as i64 - 3)), &Real::max_of(&[Real::one(), vals[0].abs()]));
                if gap.gt(&tol) {
                    return None;
                }
            }
            vals.into_iter().next()
        }
    }
}

fn value_at(f: &Ast, at: &Ast, digits: usize) -> Option<Real> {
    let c = ctx(digits + 12);
    let a = compile(at, &c)?.eval(&c, &Real::zero());
    if !a.is_finite() {
        return None;
    }
    let v = compile(f, &c)?.eval(&c, &a);
    v.is_finite().then_some(v)
}

fn value_of(job: &Job, digits: usize, clock: &mut Clock) -> R<Option<Real>> {
    Ok(match &job.kind {
        JobKind::Integral { a, b } => integral_at(&job.f, a, b, digits, clock)?,
        JobKind::Limit { to, side } => limit_at(&job.f, to, *side, digits),
        JobKind::Value { at } => value_at(&job.f, at, digits),
    })
}

// ---- constants

fn zeta3(c: &Ctx, w: usize) -> Real {
    // ζ(3) = 5/2 Σ (-1)^(k+1) / (k³ C(2k, k))
    let mut sum = Real::zero();
    let mut binom = int(2);
    let tiny = pow10(c, -(w as i64) - 2);
    for k in 1i64..10_000 {
        let term = c.div(&Real::one(), &c.mul(&binom, &int(k * k * k)));
        sum = if k % 2 == 1 { c.add(&sum, &term) } else { c.sub(&sum, &term) };
        if term.lt(&tiny) {
            break;
        }
        binom = c.div(&c.mul(&binom, &int(2 * (k + 1) * (2 * k + 1))), &int((k + 1) * (k + 1)));
    }
    c.div(&c.mul(&sum, &int(5)), &int(2))
}

fn catalan(c: &Ctx, w: usize) -> Real {
    // G = π/8 ln(2 + √3) + 3/8 Σ (n!)² / ((2n)! (2n+1)²)
    let mut sum = Real::zero();
    let mut ratio = Real::one();
    let tiny = pow10(c, -(w as i64) - 2);
    for n in 0i64..10_000 {
        let term = c.div(&ratio, &int((2 * n + 1) * (2 * n + 1)));
        sum = c.add(&sum, &term);
        if term.lt(&tiny) {
            break;
        }
        ratio = c.div(&c.mul(&ratio, &int((n + 1) * (n + 1))), &int((2 * n + 1) * (2 * n + 2)));
    }
    let ln = c.ln(&c.add(&c.sqrt(&int(3)), &int(2)));
    c.add(&c.mul(&c.div(&c.pi(), &int(8)), &ln), &c.div(&c.mul(&sum, &int(3)), &int(8)))
}

#[derive(Clone, Copy, Debug)]
enum Value {
    Pi(i64),
    InvPi(i64),
    Root(i64),
    PiRoot(i64),
    SqrtPi,
    Sqrt2Pi,
    E(i64),
    Ln(i64),
    Ln2Sq,
    PiLn2,
    LnSilver,
    Zeta3,
    Catalan,
    Sin(i64),
    Cos(i64),
}

#[derive(Clone, Debug)]
struct Atom {
    id: String,
    /// Typeset, and text the engine reads back.
    text: String,
    /// `1/π` is written `p/(qπ)` rather than `(p/q)(1/π)`.
    inverse: Option<String>,
    value: Value,
}

fn atom_value(v: Value, c: &Ctx, w: usize) -> Real {
    let pi = || c.pi();
    match v {
        Value::Pi(k) => {
            if k == 1 {
                pi()
            } else {
                c.pow(&pi(), &int(k))
            }
        }
        Value::InvPi(k) => c.div(&Real::one(), &if k == 1 { pi() } else { c.pow(&pi(), &int(k)) }),
        Value::Root(n) => c.sqrt(&int(n)),
        Value::PiRoot(n) => c.mul(&pi(), &c.sqrt(&int(n))),
        Value::SqrtPi => c.sqrt(&pi()),
        Value::Sqrt2Pi => c.sqrt(&c.mul(&pi(), &int(2))),
        Value::E(k) => c.exp(&int(k)),
        Value::Ln(n) => c.ln(&int(n)),
        Value::Ln2Sq => c.pow(&c.ln(&int(2)), &int(2)),
        Value::PiLn2 => c.mul(&pi(), &c.ln(&int(2))),
        Value::LnSilver => c.ln(&c.add(&c.sqrt(&int(2)), &Real::one())),
        Value::Zeta3 => zeta3(c, w),
        Value::Catalan => catalan(c, w),
        Value::Sin(n) => c.sin(&int(n)),
        Value::Cos(n) => c.cos(&int(n)),
    }
}

fn atom(id: &str) -> Atom {
    let (text, inverse, value) = match id {
        "pi" => ("π", None, Value::Pi(1)),
        "pi2" => ("π²", None, Value::Pi(2)),
        "pi3" => ("π³", None, Value::Pi(3)),
        "pi4" => ("π⁴", None, Value::Pi(4)),
        "invpi" => ("", Some("π"), Value::InvPi(1)),
        "invpi2" => ("", Some("π²"), Value::InvPi(2)),
        "sqrt2" => ("√2", None, Value::Root(2)),
        "sqrt3" => ("√3", None, Value::Root(3)),
        "sqrt5" => ("√5", None, Value::Root(5)),
        "sqrt6" => ("√6", None, Value::Root(6)),
        "pisqrt2" => ("π√2", None, Value::PiRoot(2)),
        "pisqrt3" => ("π√3", None, Value::PiRoot(3)),
        "sqrtpi" => ("√π", None, Value::SqrtPi),
        "sqrt2pi" => ("√(2π)", None, Value::Sqrt2Pi),
        "e" => ("e", None, Value::E(1)),
        "e2" => ("e²", None, Value::E(2)),
        "inve" => ("", Some("e"), Value::E(-1)),
        "ln2" => ("ln(2)", None, Value::Ln(2)),
        "ln3" => ("ln(3)", None, Value::Ln(3)),
        "ln5" => ("ln(5)", None, Value::Ln(5)),
        "ln2sq" => ("ln(2)²", None, Value::Ln2Sq),
        "piln2" => ("π ln(2)", None, Value::PiLn2),
        "lnsilver" => ("ln(1 + √2)", None, Value::LnSilver),
        "zeta3" => ("ζ(3)", None, Value::Zeta3),
        _ => ("catalan", None, Value::Catalan),
    };
    Atom { id: id.into(), text: text.into(), inverse: inverse.map(str::to_string), value }
}

/// Tried in order; the first relation that verifies wins, so simpler bases come first.
fn families() -> Vec<(Vec<Atom>, f64)> {
    let mut out: Vec<(Vec<Atom>, f64)> = vec![(Vec::new(), 1e6)];
    for a in [
        "pi", "pi2", "invpi", "sqrt2", "sqrt3", "ln2", "e", "inve", "sqrtpi", "pi3", "pi4", "invpi2", "sqrt5",
        "sqrt6", "pisqrt2", "pisqrt3", "sqrt2pi", "e2", "ln3", "ln5", "ln2sq", "piln2", "lnsilver", "zeta3",
        "catalan",
    ] {
        out.push((vec![atom(a)], 1e4));
    }
    for pair in [
        ["pi", "pi2"],
        ["pi", "ln2"],
        ["pi", "sqrt3"],
        ["pi", "sqrt2"],
        ["pi", "pisqrt3"],
        ["ln2", "ln3"],
        ["pi2", "ln2"],
        ["pi2", "ln2sq"],
        ["ln2", "ln2sq"],
        ["pi2", "zeta3"],
        ["pi", "catalan"],
        ["e", "inve"],
        ["e", "e2"],
        ["sqrt2", "lnsilver"],
    ] {
        out.push((pair.iter().map(|a| atom(a)).collect(), 1e3));
    }
    out.push((["pi", "pi2", "ln2", "ln2sq", "piln2"].iter().map(|a| atom(a)).collect(), 200.0));
    out
}

// ---- PSLQ

/// Integer relation among `xs` with every coefficient at most `bound`, or None.
fn pslq(xs: &[Real], c: &Ctx, digits: usize, bound: f64, clock: &mut Clock) -> R<Option<Vec<f64>>> {
    let n = xs.len();
    if n < 2 {
        return Ok(None);
    }
    let gamma = c.sqrt(&c.div(&int(4), &int(3)));
    let mut s = vec![Real::zero(); n];
    for k in 0..n {
        let mut acc = Real::zero();
        for x in &xs[k..] {
            acc = c.add(&acc, &c.pow(x, &int(2)));
        }
        s[k] = c.sqrt(&acc);
    }
    if s[0].is_zero() {
        return Ok(None);
    }
    let mut y: Vec<Real> = xs.iter().map(|v| c.div(v, &s[0])).collect();
    let ss: Vec<Real> = s.iter().map(|v| c.div(v, &s[0])).collect();
    // an exact zero among the tail means a trivial relation; the basis is degenerate
    if ss.iter().any(Real::is_zero) {
        return Ok(None);
    }
    let mut hm = vec![vec![Real::zero(); n - 1]; n];
    for i in 0..n {
        for j in 0..i.min(n - 1) {
            hm[i][j] = c.div(&c.mul(&y[i].neg(), &y[j]), &c.mul(&ss[j], &ss[j + 1]));
        }
        if i < n - 1 {
            hm[i][i] = c.div(&ss[i + 1], &ss[i]);
        }
    }
    // A is updated by the TS too, but nothing reads it; B holds the relation
    let mut bm: Vec<Vec<Real>> = (0..n)
        .map(|i| (0..n).map(|j| int(i64::from(i == j))).collect())
        .collect();
    let reduce = |i: usize, j: usize, y: &mut Vec<Real>, hm: &mut Vec<Vec<Real>>, bm: &mut Vec<Vec<Real>>| {
        if hm[j][j].is_zero() {
            return;
        }
        let q = c.div(&hm[i][j], &hm[j][j]).round_int(Rounding::HalfEven);
        if q.is_zero() {
            return;
        }
        y[j] = c.add(&y[j], &c.mul(&q, &y[i]));
        for k in 0..=j {
            hm[i][k] = c.sub(&hm[i][k], &c.mul(&q, &hm[j][k]));
        }
        for row in bm.iter_mut() {
            row[j] = c.add(&row[j], &c.mul(&q, &row[i]));
        }
    };
    for i in 1..n {
        for j in (0..i).rev() {
            reduce(i, j, &mut y, &mut hm, &mut bm);
        }
    }
    let eps = pow10(c, -(digits as i64 - 5));
    let limit = Real::from_f64(bound * (n as f64).sqrt());
    for _ in 0..400 * n {
        clock.tick()?;
        let mut m = 0;
        let mut best = int(-1);
        let mut g = gamma.clone();
        for i in 0..n - 1 {
            let v = c.mul(&g, &hm[i][i].abs());
            if v.gt(&best) {
                best = v;
                m = i;
            }
            g = c.mul(&g, &gamma);
        }
        y.swap(m, m + 1);
        hm.swap(m, m + 1);
        for r in bm.iter_mut() {
            r.swap(m, m + 1);
        }
        if m + 2 < n {
            let t0 = c.sqrt(&c.add(&c.pow(&hm[m][m], &int(2)), &c.pow(&hm[m][m + 1], &int(2))));
            if t0.is_zero() {
                return Ok(None);
            }
            let cc = c.div(&hm[m][m], &t0);
            let sn = c.div(&hm[m][m + 1], &t0);
            for row in hm.iter_mut().skip(m) {
                let t3 = row[m].clone();
                let t4 = row[m + 1].clone();
                row[m] = c.add(&c.mul(&cc, &t3), &c.mul(&sn, &t4));
                row[m + 1] = c.add(&c.mul(&sn.neg(), &t3), &c.mul(&cc, &t4));
            }
        }
        for i in m + 1..n {
            for j in (0..=(i - 1).min(m + 1)).rev() {
                reduce(i, j, &mut y, &mut hm, &mut bm);
            }
        }
        for j in 0..n {
            if y[j].abs().lt(&eps) {
                let rel: Vec<f64> = bm.iter().map(|r| r[j].to_f64()).collect();
                return Ok(rel.iter().all(|v| v.abs() <= bound).then_some(rel));
            }
        }
        // no relation this small exists once 1/max|H_jj| passes the bound
        let mut max_diag = Real::zero();
        for (j, row) in hm.iter().enumerate().take(n - 1) {
            if row[j].abs().gt(&max_diag) {
                max_diag = row[j].abs();
            }
        }
        if max_diag.is_zero() || c.div(&Real::one(), &max_diag).gt(&limit) {
            return Ok(None);
        }
    }
    Ok(None)
}

// ---- formatting

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0.0 {
        (a, b) = (b, a % b);
    }
    if a == 0.0 { 1.0 } else { a }
}

struct Term<'a> {
    p: f64,
    q: f64,
    atom: Option<&'a Atom>,
}

fn starts_letter(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

fn term_text(t: &Term) -> String {
    let p = t.p.abs();
    let ps = jsnum::to_string(p);
    let qs = jsnum::to_string(t.q);
    let Some(atom) = t.atom else {
        return if t.q == 1.0 { ps } else { format!("{ps}/{qs}") };
    };
    if let Some(inv) = &atom.inverse {
        let den = if t.q == 1.0 {
            inv.clone()
        } else {
            format!("({qs}{}{inv})", if starts_letter(inv) { " " } else { "" })
        };
        return format!("{ps}/{den}");
    }
    let lead = if starts_letter(&atom.text) && p != 1.0 {
        format!("{ps} ")
    } else if p == 1.0 {
        String::new()
    } else {
        ps
    };
    let body = format!("{lead}{}", atom.text);
    if t.q == 1.0 { body } else { format!("{body}/{qs}") }
}

/// x = -(c1·1 + c2·a2 + …)/c0, as `2 - π²/6`.
fn format_relation(rel: &[f64], atoms: &[Atom]) -> Option<String> {
    let c0 = rel[0];
    if c0 == 0.0 {
        return None;
    }
    let mut terms: Vec<Term> = Vec::new();
    for (i, &cv) in rel[1..].iter().enumerate() {
        if cv == 0.0 {
            continue;
        }
        let (mut p, mut q) = (-cv, c0);
        if q < 0.0 {
            p = -p;
            q = -q;
        }
        let g = gcd(p, q);
        terms.push(Term { p: p / g, q: q / g, atom: if i == 0 { None } else { Some(&atoms[i - 1]) } });
    }
    if terms.is_empty() {
        return Some("0".into());
    }
    if let Some(first_pos) = terms.iter().position(|t| t.p > 0.0)
        && terms[0].p < 0.0
        && first_pos > 0
    {
        let t = terms.remove(first_pos);
        terms.insert(0, t);
    }
    Some(
        terms
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let sign = if i == 0 {
                    if t.p < 0.0 { "-" } else { "" }
                } else if t.p < 0.0 {
                    " - "
                } else {
                    " + "
                };
                format!("{sign}{}", term_text(t))
            })
            .collect(),
    )
}

// ---- identify

fn holds(rel: &[f64], values: &[Real], c: &Ctx, digits: usize) -> bool {
    let mut sum = Real::zero();
    let mut scale = Real::zero();
    for (i, &k) in rel.iter().enumerate() {
        let t = c.mul(&values[i], &Real::from_f64(k));
        sum = c.add(&sum, &t);
        if t.abs().gt(&scale) {
            scale = t.abs();
        }
    }
    sum.abs().lte(&c.mul(&scale, &pow10(c, -(digits as i64 - 5))))
}

fn mentions_fn(ast: &Ast, names: &[&str]) -> bool {
    match ast {
        Ast::Fn(n, _) if names.contains(&n.as_str()) => true,
        Ast::Op(o, a) if o == "^" && matches!(a.first(), Some(Ast::Const(e)) if e == "e") && names.contains(&"exp") => {
            true
        }
        Ast::Op(_, a) | Ast::Fn(_, a) => a.iter().any(|x| mentions_fn(x, names)),
        _ => false,
    }
}

/// Constants the integrand itself suggests: sin(1) and cos(1) for ∫0..1 sin(x), ln(10) for ∫1..10 1/x.
fn context_families<'a>(job: &'a Job) -> Vec<(Vec<Atom>, f64)> {
    let bound = |b: &'a Bound| match b {
        Bound::Ast(a) => Some(a),
        _ => None,
    };
    let asts: Vec<&Ast> = match &job.kind {
        JobKind::Integral { a, b } => [bound(a), bound(b)].into_iter().flatten().collect(),
        JobKind::Limit { to, .. } => bound(to).into_iter().collect(),
        JobKind::Value { at } => vec![at],
    };
    let mut out = Vec::new();
    for end in asts {
        let Ast::Num(v) = end else { continue };
        let cv = jsnum::parse(v).unwrap_or(f64::NAN);
        if !jsnum::is_integer(cv) || cv == 0.0 || cv.abs() > 100.0 {
            continue;
        }
        let k = cv as i64;
        let cs = jsnum::to_string(cv);
        if job.rad && mentions_fn(&job.f, &["sin", "cos", "tan", "sec", "csc", "cot"]) {
            out.push((
                vec![
                    Atom { id: format!("sin{cs}"), text: format!("sin({cs})"), inverse: None, value: Value::Sin(k) },
                    Atom { id: format!("cos{cs}"), text: format!("cos({cs})"), inverse: None, value: Value::Cos(k) },
                ],
                1e3,
            ));
        }
        if mentions_fn(&job.f, &["exp"]) && k.abs() != 1 && k.abs() != 2 {
            let text = if k < 0 { format!("e^({cs})") } else { format!("e^{cs}") };
            out.push((vec![Atom { id: format!("exp{cs}"), text, inverse: None, value: Value::E(k) }], 1e3));
        }
        let integral = matches!(job.kind, JobKind::Integral { .. });
        if (integral || mentions_fn(&job.f, &["log", "log10"])) && k > 1 && ![2, 3, 5].contains(&k) {
            out.push((
                vec![Atom { id: format!("ln{cs}"), text: format!("ln({cs})"), inverse: None, value: Value::Ln(k) }],
                1e3,
            ));
        }
    }
    out
}

/// `identifyJob`: the closed form of the job's exact value, verified at higher precision; None when none is found
/// within `ticks` units of work.
pub fn identify_job(job: &Job, ticks: u64) -> Option<String> {
    let mut clock = Clock { left: ticks };
    identify(job, &mut clock).ok().flatten()
}

fn identify(job: &Job, clock: &mut Clock) -> R<Option<String>> {
    let Some(x) = value_of(job, VERIFY_DIGITS, clock)? else {
        return Ok(None);
    };
    if x.is_zero() {
        return Ok(None);
    }
    // the double answer is computed independently; disagreeing means this isn't the same number
    if (x.to_f64() - job.approx).abs() > 10.0 * job.err + 1e-12 * job.approx.abs() {
        return Ok(None);
    }
    let w1 = SEARCH_DIGITS + 10;
    let d1 = ctx(w1);
    let w2 = VERIFY_DIGITS + 10;
    let d2 = ctx(w2);
    let x1 = x.to_sd(w1, Rounding::HalfEven);
    let x2 = x.to_sd(w2, Rounding::HalfEven);
    let mut cache: HashMap<(String, usize), Real> = HashMap::new();
    let mut value = |a: &Atom, c: &Ctx, w: usize| {
        cache
            .entry((a.id.clone(), w))
            .or_insert_with(|| atom_value(a.value, c, w))
            .clone()
    };
    for (atoms, bound) in families().into_iter().chain(context_families(job)) {
        let mut basis = vec![x1.clone(), Real::one()];
        basis.extend(atoms.iter().map(|a| value(a, &d1, w1)));
        let Some(rel) = pslq(&basis, &d1, SEARCH_DIGITS, bound, clock)? else { continue };
        if rel[0] == 0.0 {
            continue;
        }
        let mut values = vec![x2.clone(), Real::one()];
        values.extend(atoms.iter().map(|a| value(a, &d2, w2)));
        if !holds(&rel, &values, &d2, VERIFY_DIGITS) {
            continue;
        }
        return Ok(format_relation(&rel, &atoms));
    }
    Ok(None)
}

// ---- the cache the live answer reads (`closedForms` in calculus.ts), shared by every thread as the TS module's

static KNOWN: std::sync::LazyLock<std::sync::Mutex<HashMap<String, Option<String>>>> =
    std::sync::LazyLock::new(Default::default);

fn known() -> std::sync::MutexGuard<'static, HashMap<String, Option<String>>> {
    KNOWN.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `knownClosedForm`: None when not asked yet; Some(None) when the search looked and found nothing.
pub fn known_closed_form(key: &str) -> Option<Option<String>> {
    known().get(key).cloned()
}

/// `rememberClosedForm`: the search's verified answer for a job.
pub fn remember_closed_form(job: &Job, exact: Option<String>) {
    known().insert(job.key(), exact);
}

/// Forgets every remembered closed form (each golden fixture starts from a fresh module, as a vitest file does).
pub fn forget_closed_forms() {
    known().clear();
}
