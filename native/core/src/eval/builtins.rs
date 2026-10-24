//! The engine's function scope (`builtinScope`/`scalarScope` in `src/engine/scientific.ts`).
//!
//! The foundation registers what plain arithmetic needs (`factorial`, which `n!` becomes, `mod`, `abs`, and the
//! `sqrt`/`cbrt` that `√`/`∛` become). Package "functions" registers the rest in
//! `crate::scientific::functions::TABLE`; this file looks there second, so the two never edit the same table.

use super::{EvalError, EvalResult, Scope, Val};
use crate::jsnum;
use crate::scientific::functions::jsmath; // WP-FUNCTIONS: V8's Math.*

/// The golden ratio, unless `phi` is a variable.
pub const PHI: f64 = std::f64::consts::GOLDEN_RATIO;
/// Catalan's constant as the TS writes it.
pub const CATALAN: f64 = 0.915965594177219;

/// A scope function: its JS arity (`fn.length`, 0 for variadic) and body. Extra arguments throw, as in `unitSafe`.
#[derive(Clone, Copy, Debug)]
pub struct Builtin {
    pub name: &'static str,
    pub max_args: usize,
    pub call: fn(&[Val], &Scope) -> EvalResult,
}

/// The engine's constants: in the scope, so they win over variables of the same name (none can be assigned).
pub fn scope_constant(name: &str) -> Option<Val> {
    Some(Val::Num(match name {
        "e" => std::f64::consts::E,
        "pi" => std::f64::consts::PI,
        "tau" => std::f64::consts::TAU,
        "catalan" => CATALAN,
        "phi" => PHI,
        _ => return None,
    }))
}

fn arg(args: &[Val], i: usize) -> f64 {
    args.get(i).map_or(f64::NAN, Val::to_number)
}

fn num(n: f64) -> EvalResult {
    Ok(Val::Num(n))
}

/// Within a few float steps of an integer, which is all `0.29*100` misses 29 by.
pub fn near_integer(x: f64) -> Option<f64> {
    let r = jsnum::round(x);
    ((x - r).abs() <= 4.0 * f64::EPSILON * r.abs().max(1.0)).then_some(r)
}

/// Non-negative remainder; `x mod 0` is undefined; past 2^53 there is no honest answer.
pub fn modulo(a: f64, b: f64) -> Result<f64, EvalError> {
    if b == 0.0 {
        return Ok(f64::NAN);
    }
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    if a.abs() > MAX_SAFE || b.abs() > MAX_SAFE {
        return Err(EvalError::Other("inexact mod".into()));
    }
    let m = b.abs();
    let r = ((a % b) + m) % m;
    // `1 mod 0.1` lands one float step short of 0.1
    Ok(if near_integer(a / b).is_some() {
        0.0
    } else {
        r
    })
}

/// Exact n/d for x, or None when x isn't a fraction with a small denominator (like π or √2).
pub fn as_ratio(x: f64) -> Option<(f64, f64)> {
    for d in 1..=10_000 {
        let d = f64::from(d);
        let n = jsnum::round(x * d);
        if (n / d - x).abs() <= 1e-12 * x.abs().max(1.0) {
            return Some((n, d));
        }
    }
    None
}

/// mathjs `product(i, n)`: split in halves, which rounds differently from a running product.
fn product(i: f64, n: f64) -> f64 {
    if n < i {
        return 1.0;
    }
    if n == i {
        return n;
    }
    let half = ((n + i) as i64 >> 1) as f64;
    product(i, half) * product(half + 1.0, n)
}

const GAMMA_G: f64 = 4.7421875;
// copied digit for digit from mathjs, so the doubles are the ones it rounds to
#[allow(
    clippy::excessive_precision,
    clippy::inconsistent_digit_grouping,
    clippy::unreadable_literal
)]
const GAMMA_P: [f64; 15] = [
    0.99999999999999709182,
    57.156235665862923517,
    -59.597960355475491248,
    14.136097974741747174,
    -0.49191381609762019978,
    0.33994649984811888699e-4,
    0.46523628927048575665e-4,
    -0.98374475304879564677e-4,
    0.15808870322491248884e-3,
    -0.21026444172410488319e-3,
    0.21743961811521264320e-3,
    -0.16431810653676389022e-3,
    0.84418223983852743293e-4,
    -0.26190838401581408670e-4,
    0.36899182659531622704e-5,
];

/// mathjs `gammaNumber`.
pub fn gamma(n: f64) -> f64 {
    if jsnum::is_integer(n) {
        if n <= 0.0 {
            return if n.is_finite() {
                f64::INFINITY
            } else {
                f64::NAN
            };
        }
        if n > 171.0 {
            return f64::INFINITY;
        }
        return product(1.0, n - 1.0);
    }
    if n < 0.5 {
        return std::f64::consts::PI / (jsmath::sin(std::f64::consts::PI * n) * gamma(1.0 - n)); // WP-FUNCTIONS
    }
    if n >= 171.35 {
        return f64::INFINITY;
    }
    if n > 85.0 {
        let two = n * n;
        let three = two * n;
        let four = three * n;
        let five = four * n;
        return (2.0 * std::f64::consts::PI / n).sqrt()
            * (n / std::f64::consts::E).powf(n)
            * (1.0 + 1.0 / (12.0 * n) + 1.0 / (288.0 * two)
                - 139.0 / (51840.0 * three)
                - 571.0 / (2_488_320.0 * four)
                + 163_879.0 / (209_018_880.0 * five)
                + 5_246_819.0 / (75_246_796_800.0 * five * n));
    }
    let n = n - 1.0;
    let mut x = GAMMA_P[0];
    for (i, p) in GAMMA_P.iter().enumerate().skip(1) {
        x += p / (n + i as f64);
    }
    let t = n + GAMMA_G + 0.5;
    (2.0 * std::f64::consts::PI).sqrt() * t.powf(n + 0.5) * jsmath::exp(-t) * x // WP-FUNCTIONS
}

/// n! for integers, Γ(n+1) for other reals; negative integers are undefined.
pub fn factorial(n: f64) -> f64 {
    if jsnum::is_integer(n) {
        if n < 0.0 {
            return f64::NAN;
        }
        if n > 170.0 {
            return f64::INFINITY;
        }
        return gamma(n + 1.0);
    }
    if n.is_nan() {
        return f64::NAN;
    }
    if n == f64::INFINITY {
        return f64::INFINITY;
    }
    gamma(n + 1.0)
}

fn sqrt(args: &[Val], scope: &Scope) -> EvalResult {
    let x = arg(args, 0);
    // with an i typed, the root of a negative is imaginary; in plain real maths it's undefined
    if x < 0.0 {
        return Ok(if scope.complex_typed {
            Val::Complex {
                re: 0.0,
                im: (-x).sqrt(),
            }
        } else {
            Val::Num(f64::NAN)
        });
    }
    num(x.sqrt())
}

fn abs(args: &[Val], _: &Scope) -> EvalResult {
    match args.first() {
        Some(Val::Complex { re, im }) => num(super::complex::abs(*re, *im)), // WP-FUNCTIONS
        _ => num(arg(args, 0).abs()),
    }
}

const FOUNDATION: &[Builtin] = &[
    Builtin {
        name: "factorial",
        max_args: 1,
        call: |a, _| num(factorial(arg(a, 0))),
    },
    Builtin {
        name: "mod",
        max_args: 2,
        call: |a, _| num(modulo(arg(a, 0), arg(a, 1))?),
    },
    Builtin {
        name: "abs",
        max_args: 1,
        call: abs,
    },
    Builtin {
        name: "sqrt",
        max_args: 1,
        call: sqrt,
    },
    Builtin {
        name: "cbrt",
        max_args: 1,
        call: |a, _| num(jsmath::cbrt(arg(a, 0))), // WP-FUNCTIONS: V8's Math.cbrt
    },
];

fn find(name: &str) -> Option<&'static Builtin> {
    FOUNDATION
        .iter()
        .chain(crate::scientific::functions::TABLE.iter())
        .find(|b| b.name == name)
}

/// Whether `name` is a function in the engine's scope.
pub fn is_builtin(name: &str) -> bool {
    find(name).is_some()
}

/// Calls a scope function, or None when there is none by that name.
pub fn call_builtin(name: &str, args: &[Val], scope: &Scope) -> Option<EvalResult> {
    let b = find(name)?;
    if b.max_args > 0 && args.len() > b.max_args {
        return Some(Err(EvalError::TooManyArguments(name.to_string())));
    }
    // WP-UNITS: `unitSafe`, trig takes an angle unit in the calculator's mode, anything else throws
    let args = &*match crate::units::mathjs::unit_safe(name, args, scope.angle_mode) {
        Ok(a) => a,
        Err(e) => return Some(Err(e)),
    };
    if args.iter().any(|a| matches!(a, Val::Str(_))) {
        // a unit or string inside a scalar function: blank, as `unitSafe` throws
        return Some(Err(EvalError::Unsupported(format!("text in {name}"))));
    }
    Some((b.call)(args, scope))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factorials_and_gamma() {
        assert_eq!(factorial(0.0), 1.0);
        assert_eq!(factorial(5.0), 120.0);
        // mathjs multiplies in halves, one ulp off the true 170!; the precise twin corrects it later
        assert_eq!(factorial(170.0), 7.257_415_615_307_998e306);
        assert_eq!(factorial(171.0), f64::INFINITY);
        assert!(factorial(-1.0).is_nan());
        assert!((factorial(0.5) - 0.886_226_925_452_758).abs() < 1e-14);
    }

    #[test]
    fn modulo_is_non_negative() {
        assert_eq!(modulo(-7.0, 3.0).unwrap(), 2.0);
        assert_eq!(modulo(1.0, 0.1).unwrap(), 0.0);
        assert!(modulo(1.0, 0.0).unwrap().is_nan());
        assert!(modulo(1e300, 7.0).is_err());
    }
}
