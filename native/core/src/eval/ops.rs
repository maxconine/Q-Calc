//! mathjs operators over numbers and booleans, with the engine's overrides from `scientific.ts`
//! (`divide`, `pow`, `mod` are imported over mathjs's own there).

use super::builtins::{as_ratio, modulo};
use super::{EvalError, EvalResult, Val};
use crate::jsnum;
use crate::syntax::Op;

/// A number for arithmetic: mathjs converts booleans and null; anything else is a type mathjs would reject or
/// handle elementwise (lists, complex), which later packages port.
fn scalar(v: &Val, op: Op) -> Result<f64, EvalError> {
    match v {
        Val::Num(n) => Ok(*n),
        Val::Bool(b) => Ok(f64::from(u8::from(*b))),
        Val::Null => Ok(0.0),
        other => Err(EvalError::Unsupported(format!(
            "{} on {}",
            op.name(),
            kind(other)
        ))),
    }
}

fn kind(v: &Val) -> &'static str {
    match v {
        Val::Num(_) => "number",
        Val::Bool(_) => "boolean",
        Val::Complex { .. } => "complex",
        Val::List(_) => "matrix",
        Val::Str(_) => "string",
        Val::Function(_) => "function",
        Val::Null => "null",
        Val::Undefined => "undefined",
    }
}

/// JS `a ** b`: like IEEE pow except that ±1 to an infinite power, and anything to NaN, is NaN.
pub fn js_pow(a: f64, b: f64) -> f64 {
    if b.is_nan() || (a.abs() == 1.0 && b.is_infinite()) {
        return f64::NAN;
    }
    a.powf(b)
}

/// An odd root of a negative is real: `(-8)^(1/3)` is -2 and `(-8)^(2/3)` is 4; an even one stays undefined.
pub fn real_pow(a: f64, b: f64) -> f64 {
    if a == 0.0 && b < 0.0 {
        return f64::NAN;
    }
    if a >= 0.0 || jsnum::is_integer(b) {
        return js_pow(a, b);
    }
    let Some((n, d)) = as_ratio(b) else {
        return f64::NAN;
    };
    if d % 2.0 == 0.0 {
        return f64::NAN;
    }
    let v = js_pow(-a, b);
    if n.abs() % 2.0 == 0.0 { v } else { -v }
}

/// mathjs `nearlyEqual` with its default tolerances.
fn nearly_equal(a: f64, b: f64) -> bool {
    const REL: f64 = 1e-12;
    const ABS: f64 = 1e-15;
    if a.is_nan() || b.is_nan() {
        return false;
    }
    if !a.is_finite() || !b.is_finite() {
        return a == b;
    }
    if a == b {
        return true;
    }
    (a - b).abs() <= (REL * a.abs().max(b.abs())).max(ABS)
}

fn int32(x: f64, op: Op) -> Result<i64, EvalError> {
    if jsnum::is_integer(x) {
        Ok(x as i64)
    } else {
        Err(EvalError::Other(format!(
            "Integers expected in function {}",
            op.name()
        )))
    }
}

pub fn unary(op: Op, a: &Val) -> EvalResult {
    // PORT(WP-FUNCTIONS / WP-MATRIX): complex and list operands dispatch here, as in `binary`.
    if let Some(r) = super::complex::unary(op, a) {
        return r;
    } // WP-FUNCTIONS
    match op {
        Op::Not => Ok(Val::Bool(!a.truthy())),
        Op::UnaryPlus => Ok(Val::Num(scalar(a, op)?)),
        Op::UnaryMinus => Ok(Val::Num(-scalar(a, op)?)),
        Op::BitNot => Ok(Val::Num(!int32(scalar(a, op)?, op)? as f64)),
        _ => Err(EvalError::Unsupported(op.name().into())),
    }
}

pub fn binary(op: Op, a: &Val, b: &Val) -> EvalResult {
    // PORT(WP-FUNCTIONS): a complex operand goes to `eval::complex::binary` here, one line.
    if let Some(r) = super::complex::binary(op, a, b) {
        return r;
    } // WP-FUNCTIONS
    // PORT(WP-MATRIX): a list operand goes to `eval::matrix::binary` here, one line.
    if op == Op::Xor {
        return Ok(Val::Bool(a.truthy() != b.truthy()));
    }
    let (x, y) = (scalar(a, op)?, scalar(b, op)?);
    Ok(match op {
        Op::Add => Val::Num(x + y),
        Op::Subtract => Val::Num(x - y),
        Op::Multiply | Op::DotMultiply => Val::Num(x * y),
        // dividing by zero is undefined here and never infinity, so any infinity left over is a real overflow
        Op::Divide | Op::DotDivide => Val::Num(if y == 0.0 { f64::NAN } else { x / y }),
        Op::Pow | Op::DotPow => Val::Num(real_pow(x, y)),
        Op::Mod => Val::Num(modulo(x, y)?),
        Op::Equal => Val::Bool(nearly_equal(x, y)),
        Op::Unequal => Val::Bool(!nearly_equal(x, y)),
        Op::Smaller => Val::Bool(x < y && !nearly_equal(x, y)),
        Op::Larger => Val::Bool(x > y && !nearly_equal(x, y)),
        Op::SmallerEq => Val::Bool(x <= y || nearly_equal(x, y)),
        Op::LargerEq => Val::Bool(x >= y || nearly_equal(x, y)),
        Op::BitAnd => Val::Num((int32(x, op)? & int32(y, op)?) as f64),
        Op::BitOr => Val::Num((int32(x, op)? | int32(y, op)?) as f64),
        Op::BitXor => Val::Num((int32(x, op)? ^ int32(y, op)?) as f64),
        Op::LeftShift => Val::Num(((int32(x, op)?) << (int32(y, op)?.clamp(0, 63))) as f64),
        Op::RightArithShift => Val::Num(((int32(x, op)?) >> (int32(y, op)?.clamp(0, 63))) as f64),
        Op::RightLogShift => {
            Val::Num(((int32(x, op)? as u64) >> (int32(y, op)?.clamp(0, 63))) as f64)
        }
        _ => return Err(EvalError::Unsupported(op.name().into())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_pow_edges() {
        assert!(js_pow(1.0, f64::INFINITY).is_nan());
        assert!(js_pow(-1.0, f64::NEG_INFINITY).is_nan());
        assert_eq!(js_pow(f64::NAN, 0.0), 1.0);
        assert!((real_pow(-8.0, 2.0 / 3.0) - 4.0).abs() < 1e-12);
        assert!((real_pow(-8.0, 1.0 / 3.0) + 2.0).abs() < 1e-12);
    }
}
