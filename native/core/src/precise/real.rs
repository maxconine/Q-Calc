//! High-precision decimal reals: the decimal.js number the TS engine computes truthful digits, closed forms and
//! sums with, as a clean Rust type.
//!
//! A [`Real`] is a decimal floating-point value with no precision of its own (like a decimal.js `Decimal`, which
//! keeps every digit it was built from). Every operation takes a [`Ctx`] (decimal.js's `Decimal.clone({precision,
//! rounding, modulo})`) and rounds its result to `ctx.precision` significant digits in `ctx.rounding`. Building a
//! value (`Real::parse`, `Real::from_f64`, `Real::from_i64`) is exact, as `new Decimal(…)` is.
//!
//! Arithmetic and the elementary functions are **correctly rounded**: each is computed by `dashu-float` in base 10
//! at three guard digits past the target precision (its Ziv loops certify the guard digits), then rounded once to
//! the target with the exact direction of the remaining error, so the double rounding never misplaces a tie.
//! decimal.js is correctly rounded for `+ − × ÷`, `sqrt`, `cbrt`, `exp`, `ln` and base-10 `log`, and "almost always"
//! for the rest (`pow`, other logs, trig): the two agree except within an ulp of a rounding boundary.
//!
//! The special values follow decimal.js: `NaN`, `±Infinity` and `-0` exist, an invalid argument gives `NaN` rather
//! than an error (`sqrt(-1)`, `ln(-1)`, `asin(2)`), and `x/0` is `±Infinity`.
//!
//! ```
//! use qcalc_core::precise::real::{Ctx, Real};
//! let d = Ctx::new(32);
//! let third = d.div(&Real::from_i64(1), &Real::from_i64(3));
//! assert_eq!(third.to_string(), "0.33333333333333333333333333333333");
//! assert_eq!(d.pi().to_string(), "3.1415926535897932384626433832795");
//! ```

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;

use dashu_base::{Approximation, DivRem, Sign, UnsignedAbs};
use dashu_float::round::Rounding as Adjust;
use dashu_float::round::mode::HalfEven;
use dashu_float::{ConstCache, Context, FBig, FpError, Repr};
use dashu_int::{IBig, UBig};

/// Digits computed past the target before the final rounding.
const GUARD: usize = 3;
/// `Number.MAX_SAFE_INTEGER`, where decimal.js's `pow` stops squaring.
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
/// A remainder whose quotient would need more digits than this is not computed (decimal.js would run for ever).
const MAX_QUOTIENT_DIGITS: isize = 100_000;

type Rep = Repr<10>;
type Work = FBig<HalfEven, 10>;

thread_local! {
    /// π and the logarithm constants, kept between calls at the highest precision asked for so far.
    static CONSTS: RefCell<ConstCache> = const { RefCell::new(ConstCache::new()) };
}

/// decimal.js rounding modes, in its numbering (`Decimal.ROUND_UP` is 0 … `ROUND_HALF_FLOOR` is 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Away from zero.
    Up,
    /// Towards zero (truncation).
    Down,
    /// Towards +∞.
    Ceil,
    /// Towards −∞.
    Floor,
    /// To nearest, ties away from zero: decimal.js's default, and the twin's.
    HalfUp,
    /// To nearest, ties towards zero.
    HalfDown,
    /// To nearest, ties to even (banker's): what `closedForm.ts` uses.
    HalfEven,
    /// To nearest, ties towards +∞.
    HalfCeil,
    /// To nearest, ties towards −∞.
    HalfFloor,
}

/// How `modulo` picks its quotient (decimal.js `Decimal.modulo`): rounded to an integer in a rounding mode (the
/// default, `Round(Down)`, is the truncated remainder with the sign of the dividend), or Euclid's (always ≥ 0),
/// which mathjs configures for its BigNumbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Modulo {
    Round(Rounding),
    Euclid,
}

/// A decimal.js constructor: how many significant digits results keep, how they round, and how `mod` divides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ctx {
    pub precision: usize,
    pub rounding: Rounding,
    pub modulo: Modulo,
}

/// A decimal floating-point number: finite (with a signed zero), ±∞ or NaN.
#[derive(Clone, PartialEq, Eq)]
pub struct Real(Option<Rep>);

// ---------------------------------------------------------------------------------------------------------------
// rounding

fn pow10(n: usize) -> UBig {
    UBig::from(10u8).pow(n)
}

fn digit_count(m: &UBig) -> usize {
    if *m == UBig::ZERO {
        0
    } else {
        m.to_string().len()
    }
}

fn signed(neg: bool, m: UBig) -> IBig {
    IBig::from_parts(if neg { Sign::Negative } else { Sign::Positive }, m)
}

fn zero(neg: bool) -> Rep {
    if neg { Rep::neg_zero() } else { Rep::zero() }
}

/// Whether the rounding mode moves magnitude `q` (with discarded part `r` of `full`, sign `neg`) up by one.
fn bumps(mode: Rounding, neg: bool, q: &UBig, r: &UBig, full: &UBig) -> bool {
    if *r == UBig::ZERO {
        return false;
    }
    let twice = r * 2u8;
    let half = twice.cmp(full);
    let odd = q % 2u8 == 1;
    match mode {
        Rounding::Up => true,
        Rounding::Down => false,
        Rounding::Ceil => !neg,
        Rounding::Floor => neg,
        Rounding::HalfUp => half != Ordering::Less,
        Rounding::HalfDown => half == Ordering::Greater,
        Rounding::HalfEven => half == Ordering::Greater || half == Ordering::Equal && odd,
        Rounding::HalfCeil => half == Ordering::Greater || half == Ordering::Equal && !neg,
        Rounding::HalfFloor => half == Ordering::Greater || half == Ordering::Equal && neg,
    }
}

/// Rounds `sig·10^exp`, nudged an infinitesimal amount in the direction `dir` (the sign of the true value minus
/// this one; 0 when it is exact), to a multiple of `10^keep` in `mode`.
fn round_to_exp(sig: &IBig, exp: isize, dir: i8, keep: isize, mode: Rounding) -> Rep {
    let neg = sig.sign() == Sign::Negative;
    let m = sig.unsigned_abs();
    if m == UBig::ZERO {
        return zero(neg);
    }
    if dir == 0 && exp >= keep {
        return Rep::new(sig.clone(), exp);
    }
    // the nudge, as a change of magnitude
    let mut s: i8 = if neg { -dir } else { dir };
    // far below a tenth of the last kept digit, every mode rounds as it would for any such amount: stand in a
    // single digit there rather than scale the value by a huge power of ten
    let (m, exp) = if exp + (digit_count(&m) as isize) < keep - 1 {
        s = 0;
        (UBig::ONE, keep - 2)
    } else {
        (m, exp)
    };
    // one digit below the kept ones at least, then one more that carries the nudge
    let low = exp.min(keep - 1);
    let mut m = m * pow10((exp - low) as usize);
    m *= 10u8;
    match s {
        1 => m += 1u8,
        -1 => m -= 1u8,
        _ => {}
    }
    let drop = (keep - (low - 1)) as usize;
    let full = pow10(drop);
    let (mut q, r) = m.div_rem(&full);
    if bumps(mode, neg, &q, &r, &full) {
        q += 1u8;
    }
    if q == UBig::ZERO {
        return zero(neg);
    }
    Rep::new(signed(neg, q), keep)
}

/// Rounds to `precision` significant digits (decimal.js `finalise`).
fn round_sd(rep: &Rep, dir: i8, precision: usize, mode: Rounding) -> Rep {
    if rep.is_infinite() || *rep.significand() == IBig::ZERO {
        return rep.clone();
    }
    let digits = digit_count(&rep.significand().unsigned_abs()) as isize;
    if dir == 0 && digits <= precision as isize {
        return rep.clone();
    }
    let keep = rep.exponent() + digits - precision as isize;
    round_to_exp(rep.significand(), rep.exponent(), dir, keep, mode)
}

fn sign_of(rep: &Rep) -> i8 {
    match rep.significand().sign() {
        Sign::Negative => -1,
        Sign::Positive if *rep.significand() == IBig::ZERO => 0,
        Sign::Positive => 1,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// values

impl Real {
    pub const NAN: Real = Real(None);

    pub fn zero() -> Real {
        Real(Some(Rep::zero()))
    }

    pub fn neg_zero() -> Real {
        Real(Some(Rep::neg_zero()))
    }

    pub fn one() -> Real {
        Real(Some(Rep::one()))
    }

    pub fn infinity(neg: bool) -> Real {
        Real(Some(if neg {
            Rep::neg_infinity()
        } else {
            Rep::infinity()
        }))
    }

    /// `significand × 10^exponent`, exactly.
    pub fn from_parts(significand: IBig, exponent: isize) -> Real {
        Real(Some(Rep::new(significand, exponent)))
    }

    pub fn from_i64(n: i64) -> Real {
        Real::from_parts(IBig::from(n), 0)
    }

    /// `new Decimal(n)` for a JS number: the digits `String(n)` writes, keeping the sign of zero.
    pub fn from_f64(n: f64) -> Real {
        if n.is_nan() {
            return Real::NAN;
        }
        if n.is_infinite() {
            return Real::infinity(n < 0.0);
        }
        if n == 0.0 {
            return if n.is_sign_negative() {
                Real::neg_zero()
            } else {
                Real::zero()
            };
        }
        Real::parse(&crate::jsnum::to_string(n)).unwrap_or(Real::NAN)
    }

    /// The exact binary value of a double (`0.1` is 0.1000000000000000055511151231257827…).
    pub fn from_f64_exact(n: f64) -> Real {
        if !n.is_finite() || n == 0.0 {
            return Real::from_f64(n);
        }
        let bits = n.to_bits();
        let neg = bits >> 63 == 1;
        let biased = ((bits >> 52) & 0x7ff) as i64;
        let frac = bits & ((1u64 << 52) - 1);
        let (mant, e2) = if biased == 0 {
            (frac, -1074)
        } else {
            (frac | (1u64 << 52), biased - 1075)
        };
        let m = UBig::from(mant);
        if e2 >= 0 {
            return Real::from_parts(signed(neg, m << e2 as usize), 0);
        }
        // m / 2^k is m * 5^k / 10^k
        let k = (-e2) as usize;
        Real::from_parts(signed(neg, m * UBig::from(5u8).pow(k)), -(k as isize))
    }

    /// Reads decimal.js's number syntax exactly: `-1.25e-3`, `.5`, `5.`, `Infinity`, `NaN`, and integer `0x`/`0b`/`0o`
    /// literals. None for anything else (where decimal.js throws).
    pub fn parse(text: &str) -> Option<Real> {
        let t = text.trim();
        let (neg, body) = match t.as_bytes().first() {
            Some(b'-') => (true, &t[1..]),
            Some(b'+') => (false, &t[1..]),
            _ => (false, t),
        };
        match body {
            "Infinity" => return Some(Real::infinity(neg)),
            "NaN" => return Some(Real::NAN),
            _ => {}
        }
        for (prefix, radix) in [
            ("0x", 16),
            ("0X", 16),
            ("0b", 2),
            ("0B", 2),
            ("0o", 8),
            ("0O", 8),
        ] {
            if let Some(digits) = body.strip_prefix(prefix) {
                let m = UBig::from_str_radix(digits, radix).ok()?;
                return Some(Real::from_parts(signed(neg, m), 0).with_zero_sign(neg));
            }
        }
        let (mant, exp) = match body.find(['e', 'E']) {
            Some(at) => (&body[..at], body[at + 1..].parse::<isize>().ok()?),
            None => (body, 0),
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
        let m: UBig = digits.trim_start_matches('0').parse().unwrap_or(UBig::ZERO);
        let exp = exp.checked_sub(frac.len() as isize)?;
        Some(Real::from_parts(signed(neg, m), exp).with_zero_sign(neg))
    }

    fn with_zero_sign(self, neg: bool) -> Real {
        if neg && self.is_zero() {
            Real::neg_zero()
        } else {
            self
        }
    }

    /// The double nearest this value (`Decimal.toNumber`), keeping −0; ±∞ past the double range.
    pub fn to_f64(&self) -> f64 {
        match &self.0 {
            None => f64::NAN,
            Some(r) if r.is_infinite() => {
                if r.exponent() > 0 {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                }
            }
            Some(r) if *r.significand() == IBig::ZERO => {
                if r.is_neg_zero() {
                    -0.0
                } else {
                    0.0
                }
            }
            Some(r) => {
                // Rust's parser rounds a decimal string correctly, as JS's Number() does
                let exp = r.exponent();
                if exp > 400 + 400 {
                    return if sign_of(r) < 0 {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    };
                }
                format!("{}e{}", r.significand(), exp)
                    .parse()
                    .unwrap_or(f64::NAN)
            }
        }
    }

    /// The finite value's representation (`None` for NaN and the infinities).
    pub fn repr(&self) -> Option<&Repr<10>> {
        self.0.as_ref().filter(|r| r.is_finite())
    }

    /// `(significand, exponent)` with no trailing zeros in the significand; None for NaN and ±∞.
    pub fn parts(&self) -> Option<(IBig, isize)> {
        self.repr().map(|r| (r.significand().clone(), r.exponent()))
    }

    pub fn is_nan(&self) -> bool {
        self.0.is_none()
    }

    pub fn is_finite(&self) -> bool {
        self.0.as_ref().is_some_and(Rep::is_finite)
    }

    pub fn is_infinite(&self) -> bool {
        self.0.as_ref().is_some_and(Rep::is_infinite)
    }

    /// ±0.
    pub fn is_zero(&self) -> bool {
        self.repr().is_some_and(|r| *r.significand() == IBig::ZERO)
    }

    /// decimal.js `isNeg`: true for −0 and −∞ too, false for NaN.
    pub fn is_neg(&self) -> bool {
        match &self.0 {
            None => false,
            Some(r) if r.is_infinite() => r.exponent() < 0,
            Some(r) => r.is_neg_zero() || r.significand().sign() == Sign::Negative,
        }
    }

    /// decimal.js `isPos`: true for +0 and +∞, false for NaN.
    pub fn is_pos(&self) -> bool {
        !self.is_nan() && !self.is_neg()
    }

    /// A finite whole number (decimal.js `isInteger`).
    pub fn is_integer(&self) -> bool {
        self.repr()
            .is_some_and(|r| r.exponent() >= 0 || *r.significand() == IBig::ZERO)
    }

    /// Significant digits, not counting trailing zeros (decimal.js `sd()`); 1 for zero, NaN and ±∞ give 0.
    pub fn sd(&self) -> usize {
        match self.repr() {
            None => 0,
            Some(r) if *r.significand() == IBig::ZERO => 1,
            Some(r) => digit_count(&r.significand().unsigned_abs()),
        }
    }

    /// The power of ten of the leading digit (decimal.js `e`): 2 for 123, −3 for 0.00123, 0 for zero.
    pub fn exponent(&self) -> Option<isize> {
        self.repr().map(|r| {
            if *r.significand() == IBig::ZERO {
                0
            } else {
                r.exponent() + digit_count(&r.significand().unsigned_abs()) as isize - 1
            }
        })
    }

    pub fn neg(&self) -> Real {
        match &self.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => Real::infinity(r.exponent() > 0),
            Some(r) if *r.significand() == IBig::ZERO => {
                if r.is_neg_zero() {
                    Real::zero()
                } else {
                    Real::neg_zero()
                }
            }
            Some(r) => Real::from_parts(-r.significand().clone(), r.exponent()),
        }
    }

    pub fn abs(&self) -> Real {
        if self.is_neg() {
            self.neg()
        } else {
            self.clone()
        }
    }

    /// Compares by value (−0 equals +0); None when either is NaN.
    pub fn partial_cmp_value(&self, other: &Real) -> Option<Ordering> {
        let (a, b) = (self.0.as_ref()?, other.0.as_ref()?);
        let rank = |r: &Rep| -> i8 {
            if r.is_infinite() {
                if r.exponent() > 0 { 1 } else { -1 }
            } else {
                0
            }
        };
        match (rank(a), rank(b)) {
            (0, 0) => {}
            (x, y) => return Some(x.cmp(&y)),
        }
        let (sa, sb) = (sign_of(a), sign_of(b));
        if sa != sb || sa == 0 {
            return Some(sa.cmp(&sb));
        }
        let fa = Work::from_repr_const(a.clone());
        let fb = Work::from_repr_const(b.clone());
        fa.partial_cmp(&fb)
    }

    pub fn eq_value(&self, other: &Real) -> bool {
        self.partial_cmp_value(other) == Some(Ordering::Equal)
    }

    pub fn lt(&self, other: &Real) -> bool {
        self.partial_cmp_value(other) == Some(Ordering::Less)
    }

    pub fn lte(&self, other: &Real) -> bool {
        matches!(
            self.partial_cmp_value(other),
            Some(Ordering::Less | Ordering::Equal)
        )
    }

    pub fn gt(&self, other: &Real) -> bool {
        self.partial_cmp_value(other) == Some(Ordering::Greater)
    }

    pub fn gte(&self, other: &Real) -> bool {
        matches!(
            self.partial_cmp_value(other),
            Some(Ordering::Greater | Ordering::Equal)
        )
    }

    /// The least of the values (`Decimal.min`): a copy, not rounded; NaN if any is NaN.
    pub fn min_of(values: &[Real]) -> Real {
        Real::extreme(values, Ordering::Less)
    }

    /// The greatest of the values (`Decimal.max`).
    pub fn max_of(values: &[Real]) -> Real {
        Real::extreme(values, Ordering::Greater)
    }

    fn extreme(values: &[Real], want: Ordering) -> Real {
        let mut best = match values.first() {
            Some(v) => v.clone(),
            None => return Real::NAN,
        };
        for v in &values[1..] {
            match v.partial_cmp_value(&best) {
                None => return Real::NAN,
                Some(o) if o == want => best = v.clone(),
                _ => {}
            }
        }
        if best.is_nan() { Real::NAN } else { best }
    }

    /// Rounded to `dp` decimal places (decimal.js `toDecimalPlaces(dp, rm)`).
    pub fn to_dp(&self, dp: isize, mode: Rounding) -> Real {
        match self.repr() {
            Some(r) => Real(Some(round_to_exp(
                r.significand(),
                r.exponent(),
                0,
                -dp,
                mode,
            )))
            .keep_zero_sign(self),
            None => self.clone(),
        }
    }

    /// Rounded to `sd` significant digits (decimal.js `toSignificantDigits(sd, rm)`).
    pub fn to_sd(&self, sd: usize, mode: Rounding) -> Real {
        match self.repr() {
            Some(r) => Real(Some(round_sd(r, 0, sd.max(1), mode))).keep_zero_sign(self),
            None => self.clone(),
        }
    }

    fn keep_zero_sign(self, from: &Real) -> Real {
        if self.is_zero() && from.is_neg() {
            Real::neg_zero()
        } else {
            self
        }
    }

    /// Rounded to a whole number in `mode` (decimal.js `round()` with the constructor's mode).
    pub fn round_int(&self, mode: Rounding) -> Real {
        self.to_dp(0, mode)
    }

    pub fn floor(&self) -> Real {
        self.to_dp(0, Rounding::Floor)
    }

    pub fn ceil(&self) -> Real {
        self.to_dp(0, Rounding::Ceil)
    }

    pub fn trunc(&self) -> Real {
        self.to_dp(0, Rounding::Down)
    }

    /// decimal.js `toString()`: plain digits, or exponential when the leading digit's power is below −7 or at
    /// least 21 (the default `toExpNeg`/`toExpPos`). −0 prints as `0`.
    pub fn to_js_string(&self) -> String {
        self.format_with(-7, 21)
    }

    /// Like [`Real::to_js_string`] with decimal.js's `toExpNeg`/`toExpPos` set (`closedForm.ts` sets ±9e15).
    pub fn format_with(&self, exp_neg: isize, exp_pos: isize) -> String {
        let r = match &self.0 {
            None => return "NaN".into(),
            Some(r) if r.is_infinite() => {
                return if r.exponent() > 0 {
                    "Infinity".into()
                } else {
                    "-Infinity".into()
                };
            }
            Some(r) => r,
        };
        if *r.significand() == IBig::ZERO {
            return "0".into();
        }
        let neg = r.significand().sign() == Sign::Negative;
        let digits = r.significand().unsigned_abs().to_string();
        let e = r.exponent() + digits.len() as isize - 1;
        let body = if e <= exp_neg || e >= exp_pos {
            let (head, tail) = digits.split_at(1);
            let frac = if tail.is_empty() {
                String::new()
            } else {
                format!(".{tail}")
            };
            format!("{head}{frac}e{}{e}", if e < 0 { "" } else { "+" })
        } else if e < 0 {
            format!("0.{}{digits}", "0".repeat((-e - 1) as usize))
        } else if r.exponent() >= 0 {
            format!("{digits}{}", "0".repeat(r.exponent() as usize))
        } else {
            let (int, frac) = digits.split_at((e + 1) as usize);
            format!("{int}.{frac}")
        };
        if neg { format!("-{body}") } else { body }
    }
}

impl fmt::Display for Real {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_js_string())
    }
}

impl fmt::Debug for Real {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Real({})", self.to_js_string())
    }
}

impl From<i64> for Real {
    fn from(n: i64) -> Real {
        Real::from_i64(n)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// operations

/// A dashu result at the working precision, as a value plus the direction of its remaining error.
fn settle_work(r: Result<Approximation<Work, Adjust>, FpError>) -> Option<(Rep, i8)> {
    match r {
        Ok(Approximation::Exact(v)) => Some((v.into_repr(), 0)),
        Ok(Approximation::Inexact(v, adj)) => {
            let rep = v.into_repr();
            let dir = match adj {
                Adjust::AddOne => -1,
                Adjust::SubOne => 1,
                Adjust::NoOp => sign_of(&rep),
            };
            Some((rep, dir))
        }
        Err(_) => None,
    }
}

fn with_consts<T>(f: impl FnOnce(&mut ConstCache) -> T) -> T {
    CONSTS.with(|c| f(&mut c.borrow_mut()))
}

impl Ctx {
    /// decimal.js's defaults at `precision` digits: half-up rounding, truncated remainders.
    pub const fn new(precision: usize) -> Ctx {
        Ctx {
            precision,
            rounding: Rounding::HalfUp,
            modulo: Modulo::Round(Rounding::Down),
        }
    }

    pub const fn with_rounding(self, rounding: Rounding) -> Ctx {
        Ctx { rounding, ..self }
    }

    pub const fn with_modulo(self, modulo: Modulo) -> Ctx {
        Ctx { modulo, ..self }
    }

    pub const fn with_precision(self, precision: usize) -> Ctx {
        Ctx { precision, ..self }
    }

    fn work(&self) -> Context<HalfEven> {
        Context::new(self.precision.max(1) + GUARD)
    }

    fn done(&self, r: Option<(Rep, i8)>) -> Real {
        match r {
            Some((rep, dir)) => Real(Some(round_sd(
                &rep,
                dir,
                self.precision.max(1),
                self.rounding,
            ))),
            None => Real::NAN,
        }
    }

    /// Runs a dashu function at the working precision and rounds it to this context.
    fn run(
        &self,
        f: impl FnOnce(
            &Context<HalfEven>,
            &mut ConstCache,
        ) -> Result<Approximation<Work, Adjust>, FpError>,
    ) -> Real {
        let work = self.work();
        self.done(with_consts(|c| settle_work(f(&work, c))))
    }

    /// The value rounded to this precision (decimal.js `finalise`, as `+x` or `toSD(precision)`).
    pub fn round(&self, x: &Real) -> Real {
        match &x.0 {
            Some(r) if r.is_finite() => {
                Real(Some(round_sd(r, 0, self.precision.max(1), self.rounding)))
            }
            _ => x.clone(),
        }
    }

    pub fn add(&self, x: &Real, y: &Real) -> Real {
        let (Some(a), Some(b)) = (&x.0, &y.0) else {
            return Real::NAN;
        };
        match (a.is_infinite(), b.is_infinite()) {
            (true, true) => {
                return if x.is_neg() == y.is_neg() {
                    x.clone()
                } else {
                    Real::NAN
                };
            }
            (true, false) => return x.clone(),
            (false, true) => return y.clone(),
            _ => {}
        }
        // decimal.js: x + 0 is x and 0 + y is y, signs of zero included; both zero gives x
        if y.is_zero() {
            return self.round(x);
        }
        if x.is_zero() {
            return self.round(y);
        }
        self.run(|w, _| w.add(a, b))
    }

    pub fn sub(&self, x: &Real, y: &Real) -> Real {
        let (Some(a), Some(b)) = (&x.0, &y.0) else {
            return Real::NAN;
        };
        if a.is_infinite() || b.is_infinite() {
            return self.add(x, &y.neg());
        }
        // decimal.js: x − 0 is x, 0 − y is −y, and 0 − 0 is +0
        match (x.is_zero(), y.is_zero()) {
            (true, true) => return Real::zero(),
            (false, true) => return self.round(x),
            (true, false) => return self.round(&y.neg()),
            _ => {}
        }
        self.run(|w, _| w.sub(a, b))
    }

    pub fn mul(&self, x: &Real, y: &Real) -> Real {
        let (Some(a), Some(b)) = (&x.0, &y.0) else {
            return Real::NAN;
        };
        let neg = x.is_neg() != y.is_neg();
        if a.is_infinite() || b.is_infinite() {
            return if x.is_zero() || y.is_zero() {
                Real::NAN
            } else {
                Real::infinity(neg)
            };
        }
        if x.is_zero() || y.is_zero() {
            return if neg { Real::neg_zero() } else { Real::zero() };
        }
        self.run(|w, _| w.mul(a, b))
    }

    pub fn div(&self, x: &Real, y: &Real) -> Real {
        let (Some(a), Some(b)) = (&x.0, &y.0) else {
            return Real::NAN;
        };
        let neg = x.is_neg() != y.is_neg();
        match (a.is_infinite(), b.is_infinite()) {
            (true, true) => return Real::NAN,
            (true, false) => return Real::infinity(neg),
            (false, true) => return if neg { Real::neg_zero() } else { Real::zero() },
            _ => {}
        }
        match (x.is_zero(), y.is_zero()) {
            (true, true) => return Real::NAN,
            (false, true) => return Real::infinity(neg),
            (true, false) => return if neg { Real::neg_zero() } else { Real::zero() },
            _ => {}
        }
        self.run(|w, _| w.div(a, b))
    }

    /// The sum of all the values, rounded once (`Decimal.sum`); 0 for none.
    pub fn sum(&self, values: &[Real]) -> Real {
        let Some(first) = values.first() else {
            return Real::zero();
        };
        let exact = Ctx::new(0);
        // exact unless the exponents are too far apart to line up; then far more digits than kept are as good
        let wide = self.with_precision(self.precision * 2 + 40);
        let total = values[1..].iter().fold(first.clone(), |acc, v| {
            let gap = match (acc.parts(), v.parts()) {
                (Some((_, a)), Some((_, b))) => (a - b).abs(),
                _ => 0,
            };
            if gap > MAX_QUOTIENT_DIGITS {
                wide.add(&acc, v)
            } else {
                exact.add_exact(&acc, v)
            }
        });
        self.round(&total)
    }

    /// `x + y` with no rounding (finite values), NaN/∞ as `add`.
    fn add_exact(&self, x: &Real, y: &Real) -> Real {
        match (x.repr(), y.repr()) {
            (Some(a), Some(b)) if !x.is_zero() && !y.is_zero() => {
                let (sa, ea) = (a.significand().clone(), a.exponent());
                let (sb, eb) = (b.significand().clone(), b.exponent());
                let e = ea.min(eb);
                let ta = sa * IBig::from(pow10((ea - e) as usize));
                let tb = sb * IBig::from(pow10((eb - e) as usize));
                let s = ta + tb;
                if s == IBig::ZERO {
                    Real::zero()
                } else {
                    Real::from_parts(s, e)
                }
            }
            _ => Ctx::new(1_000_000).add(x, y),
        }
    }

    /// The remainder of `x / y` with the quotient chosen by `self.modulo` (decimal.js `mod`): NaN for `y = 0`.
    pub fn modulo(&self, x: &Real, y: &Real) -> Real {
        if x.is_nan() || y.is_nan() || x.is_infinite() || y.is_zero() {
            return Real::NAN;
        }
        if y.is_infinite() || x.is_zero() {
            return self.round(x);
        }
        let (Some((sa, ea)), Some((sb, eb))) = (x.parts(), y.parts()) else {
            return Real::NAN;
        };
        if ea - eb > MAX_QUOTIENT_DIGITS {
            return Real::NAN;
        }
        // x / y = n / d exactly, with n and d integers; a quotient far below 1 rounds as 1/10 would
        let (n, d) = if eb - ea > MAX_QUOTIENT_DIGITS {
            (UBig::ONE, UBig::from(10u8))
        } else {
            let e = ea.min(eb);
            (
                sa.unsigned_abs() * pow10((ea - e) as usize),
                sb.unsigned_abs() * pow10((eb - e) as usize),
            )
        };
        let (mut q, r) = n.div_rem(&d);
        let qneg = x.is_neg() != y.is_neg();
        let bump = match self.modulo {
            Modulo::Round(mode) => bumps(mode, qneg, &q, &r, &d),
            // q = sign(y) · floor(x / |y|): the remainder is never negative
            Modulo::Euclid => r != UBig::ZERO && x.is_neg(),
        };
        if bump {
            q += 1u8;
        }
        let q = Real::from_parts(signed(qneg, q), 0);
        let exact = Ctx::new(0);
        let qy = exact.mul_exact(&q, y);
        self.sub(x, &qy)
    }

    fn mul_exact(&self, x: &Real, y: &Real) -> Real {
        match (x.parts(), y.parts()) {
            (Some((a, ea)), Some((b, eb))) => Real::from_parts(a * b, ea + eb),
            _ => Real::NAN,
        }
    }

    pub fn sqrt(&self, x: &Real) -> Real {
        if x.is_nan() || x.is_zero() || x.is_infinite() && x.is_pos() {
            return x.clone();
        }
        if x.is_neg() {
            return Real::NAN;
        }
        let a = x.repr().cloned().unwrap_or_else(Rep::zero);
        self.run(|w, _| w.sqrt(&a))
    }

    pub fn cbrt(&self, x: &Real) -> Real {
        if !x.is_finite() || x.is_zero() {
            return x.clone();
        }
        let a = x.abs().repr().cloned().unwrap_or_else(Rep::zero);
        let r = self.run(|w, _| w.cbrt(&a));
        if x.is_neg() { r.neg() } else { r }
    }

    /// `x^y` as decimal.js `pow`.
    pub fn pow(&self, x: &Real, y: &Real) -> Real {
        // NaN, ±∞ or ±0 on either side: decimal.js hands the doubles to Math.pow
        if !x.is_finite() || !y.is_finite() || x.is_zero() || y.is_zero() {
            return Real::from_f64(crate::eval::ops::js_pow(x.to_f64(), y.to_f64()));
        }
        let one = Real::one();
        if x.eq_value(&one) {
            return x.clone();
        }
        if y.eq_value(&one) {
            return self.round(x);
        }
        let a = x.repr().cloned().unwrap_or_else(Rep::zero);
        let yn = y.to_f64().abs();
        if y.is_integer() && yn <= MAX_SAFE {
            let (sy, ey) = y.parts().unwrap_or((IBig::ZERO, 0));
            let n = sy * IBig::from(pow10(ey.max(0) as usize));
            return self.run(|w, _| w.powi(&a, n));
        }
        let mut neg = false;
        if x.is_neg() {
            if !y.is_integer() {
                return Real::NAN;
            }
            // the sign follows the integer exponent's parity
            let (sy, ey) = y.parts().unwrap_or((IBig::ZERO, 0));
            neg = ey == 0 && sy.unsigned_abs() % 2u8 == 1;
            if x.eq_value(&one.neg()) {
                return if neg { one.neg() } else { one };
            }
        }
        // x^y is exp(y ln x); when |y ln x| is far below the last digit kept, that rounds to 1 (to the nearest),
        // and working it out took time and memory in proportion to y's exponent: `2^1023e-123456789` ran for
        // seconds and hundreds of MB
        if matches!(self.rounding, Rounding::HalfUp | Rounding::HalfEven)
            && negligible_exponent(x, y, self.precision)
        {
            return Real::one();
        }
        let base = x.abs().repr().cloned().unwrap_or_else(Rep::zero);
        let b = y.repr().cloned().unwrap_or_else(Rep::zero);
        let r = self.run(|w, c| w.powf(&base, &b, Some(c)));
        if neg { r.neg() } else { r }
    }

    pub fn exp(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => {
                if x.is_neg() {
                    Real::zero()
                } else {
                    x.clone()
                }
            }
            _ if x.is_zero() => Real::one(),
            // e^x for |x| far below the last digit kept rounds to 1 (to the nearest); working it out took time in
            // proportion to x's exponent (`exp(1e-123456789)`: 11.6 s)
            _ if matches!(self.rounding, Rounding::HalfUp | Rounding::HalfEven)
                && magnitude(x).is_some_and(|m| m < -(self.precision as i64) - 2) =>
            {
                Real::one()
            }
            Some(r) => self.run(|w, c| w.exp(r, Some(c))),
        }
    }

    pub fn ln(&self, x: &Real) -> Real {
        if x.is_nan() || x.is_neg() && !x.is_zero() {
            return Real::NAN;
        }
        if x.is_zero() {
            return Real::infinity(true);
        }
        if x.is_infinite() {
            return x.clone();
        }
        if x.eq_value(&Real::one()) {
            return Real::zero();
        }
        let a = x.repr().cloned().unwrap_or_else(Rep::zero);
        self.run(|w, c| w.ln(&a, Some(c)))
    }

    /// `log10(x)`, exact for powers of ten.
    pub fn log10(&self, x: &Real) -> Real {
        match self.log_special(x) {
            Some(r) => r,
            None => {
                let a = x.repr().cloned().unwrap_or_else(Rep::zero);
                self.run(|w, c| w.log10(&a, Some(c)))
            }
        }
    }

    /// `log2(x)`, exact for powers of two.
    pub fn log2(&self, x: &Real) -> Real {
        match self.log_special(x) {
            Some(r) => r,
            None => {
                let a = x.repr().cloned().unwrap_or_else(Rep::zero);
                self.run(|w, c| w.log2(&a, Some(c)))
            }
        }
    }

    /// decimal.js `x.log(base)` (base 10 when None): NaN for a base that is negative, 0, 1 or not finite.
    pub fn log(&self, x: &Real, base: Option<&Real>) -> Real {
        let Some(base) = base else {
            return self.log10(x);
        };
        if base.is_nan()
            || base.is_neg()
            || !base.is_finite()
            || base.is_zero()
            || base.eq_value(&Real::one())
        {
            return Real::NAN;
        }
        if base.eq_value(&Real::from_i64(10)) {
            return self.log10(x);
        }
        if base.eq_value(&Real::from_i64(2)) {
            return self.log2(x);
        }
        if let Some(r) = self.log_special(x) {
            return r;
        }
        // ln(x)/ln(b) with enough digits that the one rounding is right
        let wide = self
            .with_precision(self.precision + 10)
            .with_rounding(Rounding::HalfEven);
        let q = wide.div(&wide.ln(x), &wide.ln(base));
        self.round(&q)
    }

    fn log_special(&self, x: &Real) -> Option<Real> {
        if x.is_nan() || x.is_neg() && !x.is_zero() {
            return Some(Real::NAN);
        }
        if x.is_zero() {
            return Some(Real::infinity(true));
        }
        if x.is_infinite() {
            return Some(x.clone());
        }
        if x.eq_value(&Real::one()) {
            return Some(Real::zero());
        }
        None
    }

    fn finite_arg(x: &Real) -> Option<Rep> {
        x.repr().cloned()
    }

    pub fn sin(&self, x: &Real) -> Real {
        match Ctx::finite_arg(x) {
            None => Real::NAN,
            Some(_) if x.is_zero() => x.clone(),
            Some(a) => self.run(|w, c| w.sin(&a, Some(c))),
        }
    }

    pub fn cos(&self, x: &Real) -> Real {
        match Ctx::finite_arg(x) {
            None => Real::NAN,
            Some(_) if x.is_zero() => Real::one(),
            Some(a) => self.run(|w, c| w.cos(&a, Some(c))),
        }
    }

    pub fn tan(&self, x: &Real) -> Real {
        match Ctx::finite_arg(x) {
            None => Real::NAN,
            Some(_) if x.is_zero() => x.clone(),
            Some(a) => self.run(|w, c| w.tan(&a, Some(c))),
        }
    }

    pub fn asin(&self, x: &Real) -> Real {
        match Ctx::finite_arg(x) {
            None => Real::NAN,
            Some(_) if x.is_zero() => x.clone(),
            Some(_) if x.abs().gt(&Real::one()) => Real::NAN,
            Some(a) => self.run(|w, c| w.asin(&a, Some(c))),
        }
    }

    pub fn acos(&self, x: &Real) -> Real {
        match Ctx::finite_arg(x) {
            None => Real::NAN,
            Some(_) if x.abs().gt(&Real::one()) => Real::NAN,
            Some(_) if x.eq_value(&Real::one()) => Real::zero(),
            Some(a) => self.run(|w, c| w.acos(&a, Some(c))),
        }
    }

    pub fn atan(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => {
                let h = self.half_pi();
                if x.is_neg() { h.neg() } else { h }
            }
            _ if x.is_zero() => x.clone(),
            Some(a) => self.run(|w, c| w.atan(a, Some(c))),
        }
    }

    fn half_pi(&self) -> Real {
        let wide = self
            .with_precision(self.precision + 4)
            .with_rounding(Rounding::Down);
        self.mul(&wide.pi(), &Real::parse("0.5").unwrap_or(Real::NAN))
    }

    /// The angle of the point `(x, y)` in (−π, π], with decimal.js's special cases for zeros and infinities.
    pub fn atan2(&self, y: &Real, x: &Real) -> Real {
        if y.is_nan() || x.is_nan() {
            return Real::NAN;
        }
        let with_sign = |r: Real, neg: bool| if neg { r.abs().neg() } else { r.abs() };
        if y.is_infinite() && x.is_infinite() {
            let wide = self
                .with_precision(self.precision + 4)
                .with_rounding(Rounding::Down);
            let k = Real::parse(if x.is_pos() { "0.25" } else { "0.75" }).unwrap_or(Real::NAN);
            return with_sign(self.mul(&wide.pi(), &k), y.is_neg());
        }
        if x.is_infinite() || y.is_zero() {
            let r = if x.is_neg() { self.pi() } else { Real::zero() };
            return with_sign(r, y.is_neg());
        }
        if y.is_infinite() || x.is_zero() {
            return with_sign(self.half_pi(), y.is_neg());
        }
        let (Some(b), Some(a)) = (y.repr().cloned(), x.repr().cloned()) else {
            return Real::NAN;
        };
        self.run(|w, c| w.atan2(&b, &a, Some(c)))
    }

    pub fn sinh(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => x.clone(),
            _ if x.is_zero() => x.clone(),
            Some(a) => self.run(|w, c| w.sinh(a, Some(c))),
        }
    }

    pub fn cosh(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => Real::infinity(false),
            _ if x.is_zero() => Real::one(),
            Some(a) => self.run(|w, c| w.cosh(a, Some(c))),
        }
    }

    pub fn tanh(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => {
                if x.is_neg() {
                    Real::one().neg()
                } else {
                    Real::one()
                }
            }
            _ if x.is_zero() => x.clone(),
            Some(a) => self.run(|w, c| w.tanh(a, Some(c))),
        }
    }

    pub fn asinh(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            Some(r) if r.is_infinite() => x.clone(),
            _ if x.is_zero() => x.clone(),
            _ if self.is_its_own_odd_series(x) => self.round(x),
            Some(a) => self.run(|w, c| w.asinh(a, Some(c))),
        }
    }

    pub fn acosh(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            _ if x.lt(&Real::one()) => Real::NAN,
            Some(r) if r.is_infinite() => x.clone(),
            _ if x.eq_value(&Real::one()) => Real::zero(),
            Some(a) => self.run(|w, c| w.acosh(a, Some(c))),
        }
    }

    pub fn atanh(&self, x: &Real) -> Real {
        match &x.0 {
            None => Real::NAN,
            _ if x.is_zero() => x.clone(),
            _ if x.abs().gt(&Real::one()) => Real::NAN,
            _ if x.abs().eq_value(&Real::one()) => Real::infinity(x.is_neg()),
            _ if self.is_its_own_odd_series(x) => self.round(x),
            Some(a) => self.run(|w, c| w.atanh(a, Some(c))),
        }
    }

    /// asinh x = x − x³/6 + … and atanh x = x + x³/3 + …: when x² is far below the last digit kept and x has
    /// fewer digits than that, the series rounds (to the nearest) to x itself. Working it out took time in
    /// proportion to x's exponent (`asinh(1e-123456789)`: 6 s).
    fn is_its_own_odd_series(&self, x: &Real) -> bool {
        let p = self.precision as i64;
        matches!(self.rounding, Rounding::HalfUp | Rounding::HalfEven)
            && magnitude(x).is_some_and(|m| m < -(p / 2) - 3)
            && x.parts()
                .is_some_and(|(s, _)| (s.unsigned_abs().to_string().len() as i64) < p)
    }

    /// π (decimal.js `acos(-1)`).
    pub fn pi(&self) -> Real {
        let work = self.work();
        self.done(with_consts(|c| {
            settle_work(Ok(c.pi::<10, HalfEven>(work.precision())))
        }))
    }

    /// e (decimal.js `exp(1)`).
    pub fn e(&self) -> Real {
        self.exp(&Real::one())
    }
}

/// m with |x| < 10^m, from x's significand digits and decimal exponent; None for NaN or ±∞.
fn magnitude(x: &Real) -> Option<i64> {
    let (s, e) = x.parts()?;
    Some(s.unsigned_abs().to_string().len() as i64 + e as i64)
}

/// Whether |y ln x| is below 10^-(precision + 2) for finite, nonzero x and y, from their decimal magnitudes.
fn negligible_exponent(x: &Real, y: &Real, precision: usize) -> bool {
    let digits = |s: &IBig| s.unsigned_abs().to_string().len() as i64;
    let (Some((sx, ex)), Some(y_mag)) = (x.parts(), magnitude(y)) else {
        return false;
    };
    // |log10 x| ≤ |ex| + digits, so |ln x| < 2.31 (|ex| + digits) + 1
    let ln_x = 2.31 * ((ex as i64).unsigned_abs() as f64 + digits(&sx) as f64) + 1.0;
    let ln_mag = crate::scientific::functions::jsmath::log10(ln_x).ceil() as i64;
    y_mag + ln_mag < -(precision as i64) - 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Real {
        Real::parse(s).unwrap()
    }

    #[test]
    fn parses_and_prints_like_decimal_js() {
        assert_eq!(r("0.1").to_string(), "0.1");
        assert_eq!(r(".5").to_string(), "0.5");
        assert_eq!(r("5.").to_string(), "5");
        assert_eq!(r("-2.5e-3").to_string(), "-0.0025");
        assert_eq!(r("1e21").to_string(), "1e+21");
        assert_eq!(r("1.5e-7").to_string(), "1.5e-7");
        assert_eq!(r("123e18").to_string(), "123000000000000000000");
        assert_eq!(r("0xff").to_string(), "255");
        assert!(r("-0").is_neg() && r("-0").is_zero());
        assert!(Real::parse("1e").is_none() && Real::parse("abc").is_none());
        assert!(r("NaN").is_nan() && r("-Infinity").is_infinite());
    }

    #[test]
    fn doubles_convert_both_ways() {
        assert_eq!(Real::from_f64(0.1).to_string(), "0.1");
        assert_eq!(Real::from_f64(1e-7).to_string(), "1e-7");
        assert_eq!(
            Real::from_f64_exact(0.1).to_string(),
            "0.1000000000000000055511151231257827021181583404541015625"
        );
        assert_eq!(Real::from_f64_exact(0.1).to_f64(), 0.1);
        assert_eq!(Real::from_f64_exact(5e-324).to_f64(), 5e-324);
        assert_eq!(r("0.30000000000000004").to_f64(), 0.30000000000000004);
        assert!(Real::from_f64(-0.0).to_f64().is_sign_negative());
    }

    #[test]
    fn arithmetic_is_correctly_rounded() {
        let d = Ctx::new(32);
        assert_eq!(
            d.div(&r("2"), &r("3")).to_string(),
            "0.66666666666666666666666666666667"
        );
        assert_eq!(d.add(&r("1e40"), &r("1")).to_string(), "1e+40");
        assert_eq!(
            Ctx::new(48).add(&r("1e40"), &r("1")).to_string(),
            "1.0000000000000000000000000000000000000001e+40"
        );
        // half-up at the 32nd digit
        assert_eq!(
            d.add(&r("0.123456789012345678901234567890125"), &r("0"))
                .to_string(),
            "0.12345678901234567890123456789013"
        );
        let even = d.with_rounding(Rounding::HalfEven);
        assert_eq!(
            even.round(&r("0.123456789012345678901234567890125"))
                .to_string(),
            "0.12345678901234567890123456789012"
        );
        assert_eq!(d.mul(&r("-0"), &r("5")).to_string(), "0");
        assert!(d.mul(&r("-0"), &r("5")).is_neg());
        assert!(d.add(&r("-0"), &r("0")).is_neg());
        assert!(!d.sub(&r("-0"), &r("-0")).is_neg());
        assert!(d.div(&r("1"), &r("0")).is_infinite());
    }

    #[test]
    fn functions_match_decimal_js() {
        let d = Ctx::new(32);
        // the digits decimal.js 10.6 gives at precision 32
        assert_eq!(
            d.sin(&r("1e100")).to_string(),
            "-0.37237612366127668826208669555316"
        );
        assert_eq!(
            d.pow(&r("1.000000000000001"), &r("1e15")).to_string(),
            "2.7182818284590438762193732418313"
        );
        assert_eq!(d.e().to_string(), "2.7182818284590452353602874713527");
        assert_eq!(
            d.ln(&r("2")).to_string(),
            "0.69314718055994530941723212145818"
        );
        assert_eq!(d.log10(&r("1000")).to_string(), "3");
        assert_eq!(d.log2(&r("8")).to_string(), "3");
        assert_eq!(
            d.sqrt(&r("2")).to_string(),
            "1.4142135623730950488016887242097"
        );
        assert_eq!(d.sqrt(&r("2.25")).to_string(), "1.5");
        assert_eq!(d.cbrt(&r("-27")).to_string(), "-3");
        assert_eq!(d.pow(&r("8"), &r("2")).to_string(), "64");
        assert_eq!(d.pow(&r("2"), &r("-2")).to_string(), "0.25");
        assert_eq!(d.pow(&r("-8"), &r("3")).to_string(), "-512");
        assert!(d.pow(&r("-8"), &r("0.5")).is_nan());
        assert_eq!(d.atan2(&r("0"), &r("-1")).to_string(), d.pi().to_string());
        assert!(d.asin(&r("1.5")).is_nan() && d.sqrt(&r("-1")).is_nan() && d.ln(&r("-1")).is_nan());
        assert!(d.atanh(&r("1")).is_infinite());
    }

    #[test]
    fn rounding_helpers() {
        assert_eq!(r("2.5").to_dp(0, Rounding::HalfUp).to_string(), "3");
        assert_eq!(r("-2.5").to_dp(0, Rounding::HalfUp).to_string(), "-3");
        assert_eq!(r("2.5").to_dp(0, Rounding::HalfEven).to_string(), "2");
        assert_eq!(r("-0.4").round_int(Rounding::HalfUp).to_string(), "0");
        assert!(r("-0.4").round_int(Rounding::HalfUp).is_neg());
        assert_eq!(r("-1.5").floor().to_string(), "-2");
        assert_eq!(r("-1.5").ceil().to_string(), "-1");
        assert_eq!(r("0.004").to_dp(2, Rounding::HalfUp).to_string(), "0");
        assert_eq!(r("0.006").to_dp(2, Rounding::HalfUp).to_string(), "0.01");
        assert_eq!(r("123456").to_sd(2, Rounding::HalfUp).to_string(), "120000");
        assert_eq!(r("100").sd(), 1);
        assert_eq!(r("0.00123").exponent(), Some(-3));
        assert!(r("5").is_integer() && !r("5.5").is_integer() && r("1e30").is_integer());
    }

    /// Differential check against decimal.js: `REAL_CASES=<file> cargo test -p qcalc-core --release -- --ignored
    /// decimal_js_cases`, with lines `op|precision|rounding|modulo|a|b|decimal.js result` written by a node script.
    #[test]
    #[ignore = "needs a case file written by decimal.js"]
    fn decimal_js_cases() {
        let Ok(path) = std::env::var("REAL_CASES") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let (mut total, mut bad) = (0, 0);
        for line in text.lines() {
            let f: Vec<&str> = line.split('|').collect();
            let [op, prec, rm, modulo, a, b, want] = f[..] else {
                continue;
            };
            let d = Ctx::new(prec.parse().unwrap())
                .with_rounding(if rm == "6" {
                    Rounding::HalfEven
                } else {
                    Rounding::HalfUp
                })
                .with_modulo(if modulo == "9" {
                    Modulo::Euclid
                } else {
                    Modulo::Round(Rounding::Down)
                });
            let (x, y) = (r(a), r(b));
            let got = match op {
                "plus" => d.add(&x, &y),
                "minus" => d.sub(&x, &y),
                "times" => d.mul(&x, &y),
                "div" => d.div(&x, &y),
                "pow" => d.pow(&x, &y),
                "mod" => d.modulo(&x, &y),
                "atan2" => d.atan2(&x, &y),
                "logb" => d.log(&x, Some(&y.abs())),
                "log10" => d.log10(&x),
                "log2" => d.log2(&x),
                "sin" => d.sin(&x),
                "cos" => d.cos(&x),
                "tan" => d.tan(&x),
                "asin" => d.asin(&x),
                "acos" => d.acos(&x),
                "atan" => d.atan(&x),
                "exp" => d.exp(&x),
                "ln" => d.ln(&x),
                "sqrt" => d.sqrt(&x),
                "cbrt" => d.cbrt(&x),
                "sinh" => d.sinh(&x),
                "cosh" => d.cosh(&x),
                "tanh" => d.tanh(&x),
                "asinh" => d.asinh(&x),
                "acosh" => d.acosh(&x),
                "atanh" => d.atanh(&x),
                _ => continue,
            };
            total += 1;
            if got.to_string() != want {
                bad += 1;
                if bad <= 40 {
                    eprintln!("{op}({a}, {b}) @{prec}/{rm}: decimal.js {want}, got {got}");
                }
            }
        }
        eprintln!("{bad} of {total} differ");
    }

    #[test]
    fn far_apart_exponents_stay_cheap() {
        let d = Ctx::new(32);
        let tiny = r("1e-999999999");
        assert_eq!(tiny.to_dp(2, Rounding::HalfUp).to_string(), "0");
        assert_eq!(tiny.to_dp(2, Rounding::Up).to_string(), "0.01");
        assert_eq!(tiny.ceil().to_string(), "1");
        assert_eq!(d.add(&tiny, &r("1")).to_string(), "1");
        assert_eq!(d.sum(&[tiny.clone(), r("1")]).to_string(), "1");
        assert_eq!(d.modulo(&tiny, &r("3")).to_string(), "1e-999999999");
        let euclid = d.with_modulo(Modulo::Euclid);
        assert_eq!(euclid.modulo(&tiny.neg(), &r("3")).to_string(), "3");
        assert!(d.modulo(&r("1e999999999"), &r("3")).is_nan());
    }

    #[test]
    fn remainders() {
        let d = Ctx::new(32);
        assert_eq!(d.modulo(&r("-7"), &r("3")).to_string(), "-1");
        let euclid = d.with_modulo(Modulo::Euclid);
        assert_eq!(euclid.modulo(&r("-7"), &r("3")).to_string(), "2");
        assert_eq!(euclid.modulo(&r("-7"), &r("-3")).to_string(), "2");
        assert_eq!(d.modulo(&r("0.3"), &r("0.1")).to_string(), "0");
        assert!(d.modulo(&r("1"), &r("0")).is_nan());
        assert_eq!(d.sum(&[r("0.1"), r("0.2"), r("-0.3")]).to_string(), "0");
    }
}
