//! Complex arithmetic as mathjs does it: mathjs hands every `Complex` operation to complex.js (v2.4), so these
//! are complex.js's own formulas, step for step, so the doubles match (`(1+2i)/(3-i)` is `0.1 + 0.7000000000000001i`
//! there too).
//!
//! A complex value only reaches an answer when an `i` was typed (`crate::scientific::typed_i`); otherwise the
//! engine shows it as undefined. Scalar functions of the engine's scope see a complex argument through JS
//! `Number(z)`, which mathjs makes `Number(z.toString())` ([`to_number`]).

use super::{EvalError, EvalResult, Val};
use crate::jsnum;
use crate::scientific::functions::jsmath;
use crate::syntax::Op;

/// complex.js `Complex.EPSILON`.
pub const EPSILON: f64 = 1e-15;

/// `Number(z)`: mathjs sets `Complex.prototype.valueOf` to `toString`, so a complex is a number only when its
/// printed form is real (`3`, or `3 + 1e-16i` printed as `3`); `Infinity` for any infinite part, NaN otherwise.
pub fn to_number(re: f64, im: f64) -> f64 {
    if re.is_nan() || im.is_nan() {
        return f64::NAN;
    }
    if !re.is_finite() || !im.is_finite() {
        return f64::INFINITY;
    }
    let a = if re.abs() < EPSILON { 0.0 } else { re };
    let b = if im.abs() < EPSILON { 0.0 } else { im };
    // `"" + a` then `Number(...)` gives back a exactly; -0 prints as "0"
    if b == 0.0 { a + 0.0 } else { f64::NAN }
}

/// complex.js `hypot`: plain Pythagoras below 1e8, scaled above it.
pub fn hypot(x: f64, y: f64) -> f64 {
    let (mut x, mut y) = (x.abs(), y.abs());
    if x < y {
        std::mem::swap(&mut x, &mut y);
    }
    if x < 1e8 {
        return (x * x + y * y).sqrt();
    }
    y /= x;
    x * (1.0 + y * y).sqrt()
}

/// complex.js `logHypot`: log(sqrt(a² + b²)) without overflow.
pub fn log_hypot(a: f64, b: f64) -> f64 {
    let (aa, ab) = (a.abs(), b.abs());
    if a == 0.0 {
        return jsmath::ln(ab);
    }
    if b == 0.0 {
        return jsmath::ln(aa);
    }
    if aa < 3000.0 && ab < 3000.0 {
        return jsmath::ln(a * a + b * b) * 0.5;
    }
    let (a, b) = (a * 0.5, b * 0.5);
    0.5 * jsmath::ln(a * a + b * b) + std::f64::consts::LN_2
}

/// A complex number as complex.js holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

const NAN: C = C {
    re: f64::NAN,
    im: f64::NAN,
};
const INFINITY: C = C {
    re: f64::INFINITY,
    im: f64::INFINITY,
};
const ZERO: C = C { re: 0.0, im: 0.0 };
const ONE: C = C { re: 1.0, im: 0.0 };

// complex.js's method names, so the port reads like it
#[allow(clippy::should_implement_trait)]
impl C {
    pub fn new(re: f64, im: f64) -> Self {
        C { re, im }
    }

    /// `isInfinite` is "not finite", so a NaN part counts too.
    fn is_infinite(self) -> bool {
        !(self.re.is_finite() && self.im.is_finite())
    }

    fn is_zero(self) -> bool {
        self.re == 0.0 && self.im == 0.0
    }

    pub fn add(self, z: C) -> C {
        let (t, u) = (self.is_infinite(), z.is_infinite());
        if t || u {
            return if t && u { NAN } else { INFINITY };
        }
        C::new(self.re + z.re, self.im + z.im)
    }

    pub fn sub(self, z: C) -> C {
        let (t, u) = (self.is_infinite(), z.is_infinite());
        if t || u {
            return if t && u { NAN } else { INFINITY };
        }
        C::new(self.re - z.re, self.im - z.im)
    }

    pub fn mul(self, z: C) -> C {
        let (t, u) = (self.is_infinite(), z.is_infinite());
        if t && z.is_zero() || u && self.is_zero() {
            return NAN;
        }
        if t || u {
            return INFINITY;
        }
        if z.im == 0.0 && self.im == 0.0 {
            return C::new(self.re * z.re, 0.0);
        }
        C::new(
            self.re * z.re - self.im * z.im,
            self.re * z.im + self.im * z.re,
        )
    }

    pub fn div(self, z: C) -> C {
        let (t, u) = (self.is_infinite(), z.is_infinite());
        let (tz, zz) = (self.is_zero(), z.is_zero());
        if tz && zz || t && u {
            return NAN;
        }
        if zz || t {
            return INFINITY;
        }
        if tz || u {
            return ZERO;
        }
        if z.im == 0.0 {
            return C::new(self.re / z.re, self.im / z.re);
        }
        if z.re.abs() < z.im.abs() {
            let x = z.re / z.im;
            let t = z.re * x + z.im;
            C::new((self.re * x + self.im) / t, (self.im * x - self.re) / t)
        } else {
            let x = z.im / z.re;
            let t = z.im * x + z.re;
            C::new((self.re + self.im * x) / t, (self.im - self.re * x) / t)
        }
    }

    pub fn pow(self, z: C) -> C {
        if z.is_zero() {
            return ONE;
        }
        if z.im == 0.0 {
            if self.im == 0.0 && self.re > 0.0 {
                return C::new(jsmath_pow(self.re, z.re), 0.0);
            } else if self.re == 0.0 {
                // a whole power of a pure imaginary goes round the four quarters exactly
                let quarter = (z.re % 4.0 + 4.0) % 4.0;
                let p = jsmath_pow(self.im, z.re);
                if quarter == 0.0 {
                    return C::new(p, 0.0);
                } else if quarter == 1.0 {
                    return C::new(0.0, p);
                } else if quarter == 2.0 {
                    return C::new(-p, 0.0);
                } else if quarter == 3.0 {
                    return C::new(0.0, -p);
                }
            }
        }
        if self.is_zero() && z.re > 0.0 {
            return ZERO;
        }
        let arg = jsmath::atan2(self.im, self.re);
        let loh = log_hypot(self.re, self.im);
        let re = jsmath::exp(z.re * loh - z.im * arg);
        let im = z.im * loh + z.re * arg;
        C::new(re * jsmath::cos(im), re * jsmath::sin(im))
    }

    pub fn sqrt(self) -> C {
        let (a, b) = (self.re, self.im);
        if b == 0.0 {
            return if a >= 0.0 {
                C::new(a.sqrt(), 0.0)
            } else {
                C::new(0.0, (-a).sqrt())
            };
        }
        let r = hypot(a, b);
        let re = (0.5 * (r + a.abs())).sqrt();
        let im = b.abs() / (2.0 * re);
        if a >= 0.0 {
            C::new(re, if b < 0.0 { -im } else { im })
        } else {
            C::new(im, if b < 0.0 { -re } else { re })
        }
    }

    pub fn exp(self) -> C {
        let er = jsmath::exp(self.re);
        if self.im == 0.0 {
            return C::new(er, 0.0);
        }
        C::new(er * jsmath::cos(self.im), er * jsmath::sin(self.im))
    }

    pub fn log(self) -> C {
        if self.im == 0.0 && self.re > 0.0 {
            return C::new(jsmath::ln(self.re), 0.0);
        }
        C::new(log_hypot(self.re, self.im), jsmath::atan2(self.im, self.re))
    }

    pub fn abs(self) -> f64 {
        hypot(self.re, self.im)
    }

    pub fn arg(self) -> f64 {
        jsmath::atan2(self.im, self.re)
    }

    pub fn neg(self) -> C {
        C::new(-self.re, -self.im)
    }

    pub fn conjugate(self) -> C {
        C::new(self.re, -self.im)
    }

    pub fn val(self) -> Val {
        Val::Complex {
            re: self.re,
            im: self.im,
        }
    }
}

/// `Math.pow` (JS `**`), for the real powers inside complex.js.
fn jsmath_pow(a: f64, b: f64) -> f64 {
    super::ops::js_pow(a, b)
}

/// complex.js `abs`, which mathjs `abs` of a complex is.
pub fn abs(re: f64, im: f64) -> f64 {
    hypot(re, im)
}

/// A scalar operand as complex.js sees it after mathjs's conversions (boolean and null to number to complex).
fn as_complex(v: &Val) -> Option<C> {
    match v {
        Val::Complex { re, im } => Some(C::new(*re, *im)),
        Val::Num(n) => Some(C::new(*n, 0.0)),
        Val::Bool(b) => Some(C::new(f64::from(u8::from(*b)), 0.0)),
        Val::Null => Some(ZERO),
        _ => None,
    }
}

/// mathjs `nearlyEqual` with the default config (relTol 1e-12, absTol 1e-15).
fn nearly_equal(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return false;
    }
    if !a.is_finite() || !b.is_finite() {
        return a == b;
    }
    a == b || (a - b).abs() <= (1e-12 * a.abs().max(b.abs())).max(1e-15)
}

/// A binary operator with a complex operand and a scalar or complex other; None when neither side is complex
/// (or the other side is a list, which `eval::matrix` owns).
pub fn binary(op: Op, a: &Val, b: &Val) -> Option<EvalResult> {
    if !matches!(a, Val::Complex { .. }) && !matches!(b, Val::Complex { .. }) {
        return None;
    }
    let (x, y) = (as_complex(a)?, as_complex(b)?);
    Some(Ok(match op {
        Op::Add => x.add(y).val(),
        Op::Subtract => x.sub(y).val(),
        Op::Multiply | Op::DotMultiply => x.mul(y).val(),
        Op::Divide | Op::DotDivide => x.div(y).val(),
        Op::Pow | Op::DotPow => x.pow(y).val(),
        // the engine's `mod` is a plain JS function, so it sees `Number(z)`
        Op::Mod => Val::Num(
            match super::builtins::modulo(a.to_number(), b.to_number()) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            },
        ),
        Op::Equal => Val::Bool(nearly_equal(x.re, y.re) && nearly_equal(x.im, y.im)),
        Op::Unequal => Val::Bool(!(nearly_equal(x.re, y.re) && nearly_equal(x.im, y.im))),
        _ => {
            return Some(Err(EvalError::Other(format!(
                "No ordering relation or {} for complex numbers",
                op.name()
            ))));
        }
    }))
}

/// A unary operator on a complex value; None for anything else.
pub fn unary(op: Op, a: &Val) -> Option<EvalResult> {
    let Val::Complex { re, im } = a else {
        return None;
    };
    let z = C::new(*re, *im);
    Some(match op {
        Op::UnaryMinus => Ok(z.neg().val()),
        Op::UnaryPlus => Ok(z.val()),
        Op::Not => Ok(Val::Bool(z.is_zero())),
        _ => Err(EvalError::Other(format!(
            "Unexpected type of argument in function {}",
            op.name()
        ))),
    })
}

/// `String(z)` (complex.js `toString`), for messages and tests.
pub fn to_text(re: f64, im: f64) -> String {
    if re.is_nan() || im.is_nan() {
        return "NaN".into();
    }
    if !re.is_finite() || !im.is_finite() {
        return "Infinity".into();
    }
    let a = if re.abs() < EPSILON { 0.0 } else { re };
    let mut b = if im.abs() < EPSILON { 0.0 } else { im };
    if b == 0.0 {
        return jsnum::to_string(a);
    }
    let mut out = String::new();
    if a != 0.0 {
        out.push_str(&jsnum::to_string(a));
        out.push(' ');
        if b < 0.0 {
            b = -b;
            out.push('-');
        } else {
            out.push('+');
        }
        out.push(' ');
    } else if b < 0.0 {
        b = -b;
        out.push('-');
    }
    if b != 1.0 {
        out.push_str(&jsnum::to_string(b));
    }
    out.push('i');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_js_arithmetic() {
        let z = C::new(1.0, 2.0).div(C::new(3.0, -1.0));
        assert_eq!((z.re, z.im), (0.1, 0.7000000000000001));
        assert_eq!(C::new(0.0, 1.0).pow(C::new(2.0, 0.0)), C::new(-1.0, 0.0));
        assert_eq!(C::new(0.0, 1.0).div(ZERO).re, f64::INFINITY);
        assert_eq!(to_text(3.0, -4.0), "3 - 4i");
        assert_eq!(to_text(0.0, -1.0), "-i");
        assert_eq!(to_number(2.0, 1e-16), 2.0);
        assert!(to_number(0.0, 1.0).is_nan());
    }
}
