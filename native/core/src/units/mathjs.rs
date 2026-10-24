//! The units mathjs reads by itself. Inside a function call the unit reader never sees the line, so mathjs
//! evaluates `sin(90 deg)` with its own `Unit`: an unknown name that is a mathjs unit (`deg`, `rad`, `m`, `kΩ`)
//! becomes a valueless unit, and `90 deg` a unit with a value. Only trig takes one (as an angle, `unitSafe` in
//! `scientific.ts`); every other scope function, and user functions, throw, so the line is blank. A unit that
//! cancels (`4 m / 2 m`) is a plain number again, as mathjs's `getNumericIfUnitless` makes it.
//!
//! The table is mathjs's own (`mathjs_table.rs`, generated), searched the way `Unit._findUnit` searches it.

use std::borrow::Cow;

use super::mathjs_table::{PREFIX_SETS, UNITS};
use crate::eval::ops::js_pow;
use crate::eval::{EvalError, EvalResult, Val};
use crate::syntax::Op;
use crate::types::AngleMode;

/// One unit of a mathjs `Unit`'s list: its value, prefix, power and offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub unit_value: f64,
    pub prefix_value: f64,
    pub power: f64,
    pub offset: f64,
}

/// A mathjs `Unit`: a normalized (SI) value or none, the base dimensions, and the units it was written in.
#[derive(Clone, Debug, PartialEq)]
pub struct MathUnit {
    pub value: Option<f64>,
    /// mass, length, time, current, temperature, luminous intensity, amount of substance, angle, bit.
    pub dims: [f64; 9],
    pub units: Vec<Part>,
}

const ANGLE: [f64; 9] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn prefix_set(name: &str) -> &'static [(&'static str, f64)] {
    PREFIX_SETS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(&[][..], |(_, p)| p)
}

/// `Unit._findUnit`: an exact unit name first, then the first unit (in mathjs's order) the name ends with whose
/// prefixes include what is left.
fn find_unit(name: &str) -> Option<(Part, [f64; 9])> {
    let part = |(_, value, offset, _, _): &(&str, f64, f64, [f64; 9], &str), prefix: f64| Part {
        unit_value: *value,
        prefix_value: prefix,
        power: 1.0,
        offset: *offset,
    };
    if let Some(u) = UNITS.iter().find(|u| u.0 == name) {
        let p = prefix_set(u.4).iter().find(|(n, _)| n.is_empty()).map_or(1.0, |p| p.1);
        return Some((part(u, p), u.3));
    }
    for u in UNITS {
        let Some(prefix) = name.strip_suffix(u.0) else {
            continue;
        };
        if let Some((_, p)) = prefix_set(u.4).iter().find(|(n, _)| *n == prefix) {
            return Some((part(u, *p), u.3));
        }
    }
    None
}

/// `Unit.isValuelessUnit` and `new Unit(null, name)`: the value a bare unit name evaluates to.
pub fn symbol(name: &str) -> Option<Val> {
    let (part, dims) = find_unit(name)?;
    Some(Val::Unit(MathUnit {
        value: None,
        dims,
        units: vec![part],
    }))
}

impl MathUnit {
    fn unitless(value: f64) -> MathUnit {
        MathUnit {
            value: Some(value),
            dims: [0.0; 9],
            units: Vec::new(),
        }
    }

    fn normalize(&self, value: f64) -> f64 {
        self.units.iter().fold(value, |res, u| {
            res * js_pow(u.unit_value * u.prefix_value, u.power)
        })
    }

    fn equal_base(&self, other: &[f64; 9]) -> bool {
        self.dims
            .iter()
            .zip(other)
            .all(|(a, b)| (a - b).abs() <= 1e-12)
    }

    fn the_value(&self) -> f64 {
        self.value.unwrap_or_else(|| self.normalize(1.0))
    }

    /// `toNumber('deg' | 'rad')`: the angle in that unit; anything that isn't an angle throws.
    pub fn angle_in(&self, mode: AngleMode) -> Result<f64, EvalError> {
        if !self.equal_base(&ANGLE) {
            return Err(EvalError::Other("Units do not match".into()));
        }
        let value = self.the_value();
        // offsets of angle units are all zero, so `to` keeps the value as it is
        let per = match mode {
            AngleMode::Rad => 1.0,
            AngleMode::Deg => std::f64::consts::PI / 180.0,
        };
        Ok(value / js_pow(per, 1.0))
    }
}

/// `getNumericIfUnitless`.
fn numeric_if_unitless(u: MathUnit) -> Val {
    match u.value {
        Some(v) if u.equal_base(&[0.0; 9]) => Val::Num(v),
        _ => Val::Unit(u),
    }
}

fn multiply(a: &MathUnit, b: &MathUnit) -> Val {
    let mut res = a.clone();
    for i in 0..9 {
        res.dims[i] = a.dims[i] + b.dims[i];
    }
    res.units.extend(b.units.iter().copied());
    res.value = if a.value.is_some() || b.value.is_some() {
        let x = a.value.unwrap_or_else(|| a.normalize(1.0));
        let y = b.value.unwrap_or_else(|| b.normalize(1.0));
        Some(x * y)
    } else {
        None
    };
    numeric_if_unitless(res)
}

fn divide(a: &MathUnit, b: &MathUnit) -> Val {
    let mut res = a.clone();
    for i in 0..9 {
        res.dims[i] = a.dims[i] - b.dims[i];
    }
    res.units.extend(b.units.iter().map(|p| Part {
        power: -p.power,
        ..*p
    }));
    res.value = if a.value.is_some() || b.value.is_some() {
        let x = a.value.unwrap_or_else(|| a.normalize(1.0));
        let y = b.value.unwrap_or_else(|| b.normalize(1.0));
        Some(x / y)
    } else {
        None
    };
    numeric_if_unitless(res)
}

fn as_unit(v: &Val) -> Option<Cow<'_, MathUnit>> {
    match v {
        Val::Unit(u) => Some(Cow::Borrowed(u)),
        Val::Num(n) => Some(Cow::Owned(MathUnit::unitless(*n))),
        Val::Bool(b) => Some(Cow::Owned(MathUnit::unitless(f64::from(u8::from(*b))))),
        _ => None,
    }
}

fn mismatch(what: &str) -> EvalResult {
    Err(EvalError::Other(format!("unit {what}")))
}

/// mathjs arithmetic with a unit on either side; None when neither operand is a unit.
pub fn binary(op: Op, a: &Val, b: &Val) -> Option<EvalResult> {
    if !matches!(a, Val::Unit(_)) && !matches!(b, Val::Unit(_)) {
        return None;
    }
    let (Some(x), Some(y)) = (as_unit(a), as_unit(b)) else {
        return Some(mismatch("with a non-number"));
    };
    Some(match op {
        Op::Multiply | Op::DotMultiply => Ok(multiply(&x, &y)),
        Op::Divide | Op::DotDivide => Ok(divide(&x, &y)),
        Op::Add | Op::Subtract => {
            // both must be units with values, of the same base
            let (Val::Unit(x), Val::Unit(y)) = (a, b) else {
                return Some(mismatch("plus a number"));
            };
            let (Some(xv), Some(yv)) = (x.value, y.value) else {
                return Some(mismatch("without a value"));
            };
            if !x.equal_base(&y.dims) {
                return Some(mismatch("mismatch"));
            }
            let mut res = x.clone();
            res.value = Some(if op == Op::Add { xv + yv } else { xv - yv });
            Ok(Val::Unit(res))
        }
        Op::Pow | Op::DotPow => {
            let (Val::Unit(x), Val::Num(p)) = (a, b) else {
                return Some(mismatch("power"));
            };
            let mut res = x.clone();
            for d in &mut res.dims {
                *d *= p;
            }
            for u in &mut res.units {
                u.power *= p;
            }
            res.value = res.value.map(|v| js_pow(v, *p));
            Ok(numeric_if_unitless(res))
        }
        _ => mismatch("operator"),
    })
}

/// `-5 deg` and `+5 deg`; None when the operand isn't a unit.
pub fn unary(op: Op, a: &Val) -> Option<EvalResult> {
    let Val::Unit(u) = a else { return None };
    Some(match (op, u.value) {
        (Op::UnaryMinus, Some(v)) => Ok(Val::Unit(MathUnit {
            value: Some(-v),
            ..u.clone()
        })),
        (Op::UnaryPlus, Some(_)) => Ok(a.clone()),
        _ => mismatch("operator"),
    })
}

/// `hasUnit`: a unit, or a list holding one.
pub fn has_unit(v: &Val) -> bool {
    match v {
        Val::Unit(_) => true,
        Val::List(items) => items.iter().any(has_unit),
        _ => false,
    }
}

const ANGLE_FNS: [&str; 6] = ["sin", "cos", "tan", "csc", "sec", "cot"];

/// `unitSafe`'s unit rule: trig of one angle takes it in the calculator's mode; any other unit throws.
pub fn unit_safe<'a>(name: &str, args: &'a [Val], mode: AngleMode) -> Result<Cow<'a, [Val]>, EvalError> {
    if !args.iter().any(has_unit) {
        return Ok(Cow::Borrowed(args));
    }
    if ANGLE_FNS.contains(&name)
        && let [Val::Unit(u)] = args
    {
        return Ok(Cow::Owned(vec![Val::Num(u.angle_in(mode)?)]));
    }
    Err(EvalError::Other("unit in a scalar function".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_units_like_mathjs() {
        assert!(symbol("deg").is_some());
        assert!(symbol("kohm").is_some());
        assert!(symbol("mrad").is_some());
        assert!(symbol("xyz").is_none());
        let Some(Val::Unit(deg)) = symbol("deg") else { panic!() };
        let Val::Unit(ninety) = multiply(&MathUnit::unitless(90.0), &deg) else {
            panic!()
        };
        assert_eq!(ninety.angle_in(AngleMode::Deg), Ok(90.0));
        let Some(Val::Unit(m)) = symbol("m") else { panic!() };
        assert_eq!(divide(&m, &m), Val::Num(1.0));
    }

}
