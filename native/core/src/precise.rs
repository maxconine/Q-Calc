//! Truthful digits (`src/engine/precise.ts`): the float answer re-run without rounding error, replacing the
//! float only when the precise value is certain. `0.1+0.2` shows 0.3 at 16 figures because of this.
//!
//! The TS re-runs in decimal.js at 32 and 48 digits. The foundation's twin is exact rational arithmetic, which
//! agrees with it wherever both apply (+ − × ÷, integer powers, factorial, mod, abs, perfect roots).
//! PORT(WP-PRECISE): a high-precision real backend for the transcendental twins (`TWINS` in the TS: trig, logs,
//! non-integer powers, constants), with the `agrees` meaning check, `settle` for jumps, and user-function calls.

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::eval::builtins::{factorial as float_factorial, modulo as float_modulo};
use crate::jsnum;
use crate::scientific::{ScientificContext, preprocess_checked};
use crate::syntax::{Constant, Node, Op, parse};
use crate::types::{Value, ValueKind};

/// Work per answer, counted rather than timed so the digits never depend on how busy the machine is.
pub const BUDGET: i64 = 1000;
/// A function or a power costs this much more than an operator.
const CALL_COST: i64 = 50;
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
    Range,
    InexactInput,
}

#[derive(Debug)]
enum Abort {
    Unsupported,
    Budget,
}

type Twin = Result<(f64, BigRational), Abort>;

struct Walker<'a> {
    vars: &'a [(String, BigRational, f64)],
    work: &'a mut i64,
}

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
    if whole.is_empty() && frac.is_empty() || !whole.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = format!("{whole}{frac}");
    let int = BigInt::parse_bytes(if digits.is_empty() { b"0" } else { digits.as_bytes() }, 10)?;
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
    let (n, d) = if shift >= 0 { (num << shift as usize, den.clone()) } else { (num.clone(), den << (-shift) as usize) };
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

fn int_arg(x: &BigRational) -> Result<i64, Abort> {
    if x.is_integer() { x.to_integer().to_i64().ok_or(Abort::Unsupported) } else { Err(Abort::Unsupported) }
}

fn product(from: i64, to: i64) -> BigInt {
    (from.max(2)..=to).fold(BigInt::one(), |p, i| p * i)
}

fn exact_mod(a: &BigRational, b: &BigRational) -> Result<BigRational, Abort> {
    if b.is_zero() {
        return Err(Abort::Unsupported);
    }
    let limit = BigRational::from_integer(BigInt::from(MAX_SAFE as i64));
    if a.abs() > limit || b.abs() > limit {
        return Err(Abort::Unsupported);
    }
    let m = b.abs();
    // decimal.js `mod` is the truncated remainder; then shifted non-negative
    let trunc_rem = |x: &BigRational| x - &m * (x / &m).trunc();
    Ok(trunc_rem(&(trunc_rem(a) + &m)))
}

fn exact_root(x: &BigRational, n: u32) -> Option<BigRational> {
    let root = |v: &BigInt| -> Option<BigInt> {
        let r = v.nth_root(n);
        (r.pow(n) == *v).then_some(r)
    };
    if x.is_negative() {
        return (n % 2 == 1).then(|| exact_root(&-x, n)).flatten().map(|r| -r);
    }
    Some(BigRational::new(root(x.numer())?, root(x.denom())?))
}

impl Walker<'_> {
    fn tick(&mut self, v: (f64, BigRational)) -> Twin {
        *self.work -= 1;
        if *self.work < 0 {
            return Err(Abort::Budget);
        }
        // the engine's own float walk must be a real number at every step
        if v.0.is_nan() {
            return Err(Abort::Unsupported);
        }
        Ok(v)
    }

    fn walk(&mut self, node: &Node) -> Twin {
        match node {
            Node::Parenthesis(inner) => self.walk(inner),
            Node::Constant(Constant::Number { value, text }) => {
                if !value.is_finite() {
                    return Err(Abort::Unsupported);
                }
                let exact = parse_decimal(text).ok_or(Abort::Unsupported)?;
                self.tick((*value, exact))
            }
            Node::Symbol(name) => self
                .vars
                .iter()
                .find(|(n, ..)| n == name)
                .map(|(_, exact, f)| (*f, exact.clone()))
                .ok_or(Abort::Unsupported),
            Node::Operator { op, args, .. } => {
                let values = args.iter().map(|a| self.walk(a)).collect::<Result<Vec<_>, _>>()?;
                let v = self.op(*op, values)?;
                self.tick(v)
            }
            Node::Function { callee, args } => {
                let Node::Symbol(name) = callee.as_ref() else { return Err(Abort::Unsupported) };
                let values = args.iter().map(|a| self.walk(a)).collect::<Result<Vec<_>, _>>()?;
                let v = self.call(name, values)?;
                self.tick(v)
            }
            _ => Err(Abort::Unsupported),
        }
    }

    fn op(&mut self, op: Op, args: Vec<(f64, BigRational)>) -> Twin {
        use crate::eval::Val;
        let float = |args: &[(f64, BigRational)]| -> Result<f64, Abort> {
            let r = match args {
                [(a, _)] => crate::eval::ops::unary(op, &Val::Num(*a)),
                [(a, _), (b, _)] => crate::eval::ops::binary(op, &Val::Num(*a), &Val::Num(*b)),
                _ => return Err(Abort::Unsupported),
            };
            match r {
                Ok(Val::Num(n)) => Ok(n),
                _ => Err(Abort::Unsupported),
            }
        };
        if op == Op::Pow {
            *self.work -= CALL_COST;
        }
        let f = match (op, args.as_slice()) {
            (Op::Factorial, [(a, _)]) => float_factorial(*a),
            _ => float(&args)?,
        };
        let exact = match (op, args.as_slice()) {
            (Op::Add, [(_, a), (_, b)]) => a + b,
            (Op::Subtract, [(_, a), (_, b)]) => a - b,
            (Op::Multiply, [(_, a), (_, b)]) => a * b,
            (Op::Divide, [(_, a), (_, b)]) => {
                if b.is_zero() {
                    return Err(Abort::Unsupported);
                }
                a / b
            }
            (Op::Pow, [(_, a), (_, b)]) => {
                if a.is_negative() && !b.is_integer() {
                    return Err(Abort::Unsupported);
                }
                if a.is_zero() {
                    if b.is_negative() {
                        return Err(Abort::Unsupported);
                    }
                    if b.is_zero() { BigRational::one() } else { a.clone() }
                } else {
                    let size = to_f64(b) * to_f64(&a.abs()).log10();
                    if !size.is_finite() || size.abs() > 400.0 || !b.is_integer() {
                        return Err(Abort::Unsupported);
                    }
                    let e = int_arg(b)?;
                    let base = if e < 0 { a.recip() } else { a.clone() };
                    num_traits::pow(base, e.unsigned_abs() as usize)
                }
            }
            (Op::Mod, [(_, a), (_, b)]) => exact_mod(a, b)?,
            (Op::UnaryMinus, [(_, a)]) => -a,
            (Op::UnaryPlus, [(_, a)]) => a.clone(),
            (Op::Factorial, [(_, a)]) => self.factorial(a)?,
            _ => return Err(Abort::Unsupported),
        };
        Ok((f, exact))
    }

    fn factorial(&mut self, a: &BigRational) -> Result<BigRational, Abort> {
        let n = int_arg(a)?;
        if !(0..=170).contains(&n) {
            return Err(Abort::Unsupported);
        }
        Ok(BigRational::from_integer(product(2, n)))
    }

    fn call(&mut self, name: &str, args: Vec<(f64, BigRational)>) -> Twin {
        let product_work = if name == "factorial" { args.first().map_or(0, |(f, _)| (f / 5.0).ceil().max(0.0) as i64) } else { 0 };
        *self.work -= CALL_COST + product_work;
        if *self.work < 0 {
            return Err(Abort::Budget);
        }
        match (name, args.as_slice()) {
            ("factorial", [(f, a)]) => Ok((float_factorial(*f), self.factorial(a)?)),
            ("mod", [(fa, a), (fb, b)]) => {
                Ok((float_modulo(*fa, *fb).map_err(|_| Abort::Unsupported)?, exact_mod(a, b)?))
            }
            ("abs", [(f, a)]) => Ok((f.abs(), a.abs())),
            ("sqrt", [(f, a)]) if !a.is_negative() => Ok((f.sqrt(), exact_root(a, 2).ok_or(Abort::Unsupported)?)),
            ("cbrt", [(f, a)]) => Ok((f.cbrt(), exact_root(a, 3).ok_or(Abort::Unsupported)?)),
            _ => Err(Abort::Unsupported),
        }
    }
}

fn sig_digits(n: f64) -> usize {
    let s = jsnum::to_string(n.abs());
    let s = s.split('e').next().unwrap_or("").replace('.', "");
    s.trim_start_matches('0').trim_end_matches('0').len()
}

/// Why the float answer stays, or what replaces it.
pub fn verdict(text: &str, value: &Value, ctx: &ScientificContext) -> Verdict {
    if value.kind != ValueKind::Number || value.unit.is_some() || !value.n.is_finite() {
        return Verdict::Kept(Kept::NotANumber);
    }
    let f = value.n;
    // `ans` stays a name, not pasted digits, so it can be told apart from a typed number
    let bare = ScientificContext { ans: None, ..ctx.clone() };
    let mut vars: Vec<(String, f64)> = ctx.variables.iter().map(|(k, v)| (k.clone(), *v)).collect();
    if let Some(a) = ctx.ans {
        vars.retain(|(k, _)| k != "ans");
        vars.push(("ans".into(), a));
    }
    let Some(expr) = preprocess_checked(text, &bare) else { return Verdict::Kept(Kept::Unsupported) };
    let Ok(tree) = parse(&expr) else { return Verdict::Kept(Kept::Unsupported) };
    let mut work = BUDGET;
    let run = |vars: &[(String, f64)], work: &mut i64| -> Result<(f64, BigRational), Kept> {
        let mut exact_vars = Vec::with_capacity(vars.len());
        for (k, v) in vars {
            // a variable enters as the digits `String(v)` writes; a non-finite one can't
            match v.is_finite().then(|| parse_decimal(&jsnum::to_string(*v))).flatten() {
                Some(e) => exact_vars.push((k.clone(), e, *v)),
                None => exact_vars.push((k.clone(), BigRational::zero(), f64::NAN)),
            }
        }
        let mut w = Walker { vars: &exact_vars, work };
        w.walk(&tree).map_err(|e| match e {
            Abort::Budget => Kept::Budget,
            Abort::Unsupported => Kept::Unsupported,
        })
    };
    let (t, exact) = match run(&vars, &mut work) {
        Ok(v) => v,
        Err(k) => return Verdict::Kept(k),
    };
    if !(t == f || (t - f).abs() <= 1e-12 * t.abs().max(f.abs())) {
        return Verdict::Kept(Kept::Meaning);
    }
    let n = to_f64(&exact);
    if !n.is_finite() || (n == 0.0 && !exact.is_zero()) {
        return Verdict::Kept(Kept::Range);
    }
    let n = n + 0.0;
    if n == f {
        return Verdict::Same;
    }
    // a computed input like 1/3 is only known to the last bit of its double; the correction must survive that
    // bit moving either way, or `a*3 - 1` would turn a right 0 into -1e-16
    let shaky: Vec<&str> = vars.iter().filter(|(_, v)| sig_digits(*v) > 15).map(|(k, _)| k.as_str()).collect();
    if !shaky.is_empty() {
        for dir in [1.0, -1.0] {
            let nudged: Vec<(String, f64)> = vars
                .iter()
                .map(|(k, v)| (k.clone(), if shaky.contains(&k.as_str()) { v * (1.0 + dir * f64::EPSILON) } else { *v }))
                .collect();
            let m = match run(&nudged, &mut work) {
                Ok((_, e)) => {
                    let m = to_f64(&e);
                    if !m.is_finite() || (m == 0.0 && !e.is_zero()) {
                        return Verdict::Kept(Kept::Range);
                    }
                    m + 0.0
                }
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
        for s in ["0.1", "0.3", "1e-7", "123456789.123456789", "1e300", "5e-324", "2.2250738585072014e-308", "0.30000000000000004"] {
            assert_eq!(to_f64(&r(s)), s.parse::<f64>().unwrap(), "{s}");
        }
        assert_eq!(to_f64(&BigRational::new(1.into(), 3.into())), 1.0 / 3.0);
        assert_eq!(to_f64(&BigRational::new(2.into(), 3.into())), 2.0 / 3.0);
        assert_eq!(to_f64(&r("1e400")), f64::INFINITY);
        assert_eq!(to_f64(&r("1e-400")), 0.0);
    }

    #[test]
    fn corrects_float_artefacts() {
        let ctx = ScientificContext::default();
        let v = |n: f64| Value::number(n);
        assert_eq!(verdict("0.1+0.2", &v(0.1 + 0.2), &ctx), Verdict::Corrected(0.3));
        assert_eq!(verdict("2+2", &v(4.0), &ctx), Verdict::Same);
        assert_eq!(verdict("1e16+1-1e16", &v(0.0), &ctx), Verdict::Corrected(1.0));
        let third = ScientificContext { ans: Some(1.0 / 3.0), ..ScientificContext::default() };
        // String(1/3) times 3 is 0.9999999999999999, and nudging ans moves it less than 1e-13
        assert_eq!(verdict("ans*3", &v(1.0), &third), Verdict::Corrected(0.9999999999999999));
    }
}
