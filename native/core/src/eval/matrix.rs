//! mathjs list and matrix arithmetic in `number` mode, as the scientific reader meets it (`[1,2,3] * 2`,
//! `-[3]`, `[1,2] + [3,4]`, `[1,2] * [3,4]` as a dot product). Lines with a 2D literal, a matrix function or a
//! transpose mark go to `crate::matrix` first; what reaches mathjs is mostly flat lists.
//!
//! mathjs semantics kept:
//! - elementwise `+ - .* ./ .^`, comparisons and bitwise operators, a scalar spread over every entry, and sizes
//!   that must match exactly (no broadcasting between lists of different lengths);
//! - `*` of two lists is the matrix product (two flat lists: their dot product);
//! - `/` and `./` divide entries with mathjs's `divideScalar`, not the engine's override, so `[1,2]/0` is
//!   `[∞, ∞]` (shown as overflow) where `1/0` is undefined;
//! - `.^` goes through the engine's `pow` override, so `[-8] .^ (1/3)` is `[-2]`;
//! - the engine's `mod` is a plain JS function: a mathjs matrix turns into `NaN` there (`Number(matrix)` reads its
//!   `[3]` text), so any list makes `mod` undefined;
//! - `^` with a list, and `/` by a list, throw (the line goes blank).

use super::ops;
use super::{EvalError, EvalResult, Val};
use crate::syntax::Op;

fn size_error(op: Op) -> EvalError {
    EvalError::Other(format!("Dimension mismatch in {}", op.name()))
}

/// The scalar operator applied entry by entry.
fn entry(op: Op, a: &Val, b: &Val) -> EvalResult {
    match op {
        // mathjs's `divideScalar` on two numbers is plain JS division
        Op::Divide | Op::DotDivide => match (a, b) {
            (Val::Num(x), Val::Num(y)) => Ok(Val::Num(x / y)),
            _ => ops::binary(Op::Divide, a, b),
        },
        Op::DotMultiply => ops::binary(Op::Multiply, a, b),
        Op::DotPow => ops::binary(Op::Pow, a, b),
        _ => ops::binary(op, a, b),
    }
}

/// Entry by entry over two values, a scalar spreading over a list.
fn elementwise(op: Op, a: &Val, b: &Val) -> EvalResult {
    match (a, b) {
        (Val::List(x), Val::List(y)) => {
            if x.len() != y.len() {
                return Err(size_error(op));
            }
            x.iter()
                .zip(y)
                .map(|(p, q)| elementwise(op, p, q))
                .collect::<Result<_, _>>()
                .map(Val::List)
        }
        (Val::List(x), s) => x
            .iter()
            .map(|p| elementwise(op, p, s))
            .collect::<Result<_, _>>()
            .map(Val::List),
        (s, Val::List(y)) => y
            .iter()
            .map(|q| elementwise(op, s, q))
            .collect::<Result<_, _>>()
            .map(Val::List),
        _ => entry(op, a, b),
    }
}

/// How deep a list nests: 1 for a flat list, 2 for rows.
fn depth(v: &Val) -> usize {
    match v {
        Val::List(items) => 1 + items.first().map_or(0, depth),
        _ => 0,
    }
}

fn rows(v: &Val) -> Vec<Vec<Val>> {
    match v {
        Val::List(items) => items
            .iter()
            .map(|r| match r {
                Val::List(cells) => cells.clone(),
                other => vec![other.clone()],
            })
            .collect(),
        other => vec![vec![other.clone()]],
    }
}

fn sum_of_products(pairs: impl Iterator<Item = (Val, Val)>) -> EvalResult {
    let mut total: Option<Val> = None;
    for (p, q) in pairs {
        let term = ops::binary(Op::Multiply, &p, &q)?;
        total = Some(match total {
            None => term,
            Some(t) => ops::binary(Op::Add, &t, &term)?,
        });
    }
    Ok(total.unwrap_or(Val::Num(0.0)))
}

/// mathjs `multiply` of two lists: vector·vector is a number, otherwise the matrix product.
fn product(a: &Val, b: &Val) -> EvalResult {
    let (da, db) = (depth(a), depth(b));
    let err = || EvalError::Other("Dimension mismatch in multiplication".into());
    match (da, db) {
        (1, 1) => {
            let (Val::List(x), Val::List(y)) = (a, b) else {
                return Err(err());
            };
            if x.len() != y.len() || x.is_empty() {
                return Err(err());
            }
            sum_of_products(x.iter().cloned().zip(y.iter().cloned()))
        }
        (1, 2) => {
            // a row vector times a matrix is a vector
            let Val::List(x) = a else { return Err(err()) };
            let m = rows(b);
            if x.len() != m.len() || m.is_empty() {
                return Err(err());
            }
            let cols = m[0].len();
            (0..cols)
                .map(|j| sum_of_products(x.iter().cloned().zip(m.iter().map(|r| r[j].clone()))))
                .collect::<Result<_, _>>()
                .map(Val::List)
        }
        (2, 1) => {
            let Val::List(y) = b else { return Err(err()) };
            let m = rows(a);
            if m.first().is_none_or(|r| r.len() != y.len()) {
                return Err(err());
            }
            m.iter()
                .map(|r| sum_of_products(r.iter().cloned().zip(y.iter().cloned())))
                .collect::<Result<_, _>>()
                .map(Val::List)
        }
        (2, 2) => {
            let (x, y) = (rows(a), rows(b));
            if x.first().is_none_or(|r| r.len() != y.len()) || y.is_empty() {
                return Err(err());
            }
            let cols = y[0].len();
            x.iter()
                .map(|r| {
                    (0..cols)
                        .map(|j| {
                            sum_of_products(r.iter().cloned().zip(y.iter().map(|q| q[j].clone())))
                        })
                        .collect::<Result<_, _>>()
                        .map(Val::List)
                })
                .collect::<Result<_, _>>()
                .map(Val::List)
        }
        _ => Err(err()),
    }
}

/// A binary operator with a list on either side; None when neither side is a list.
pub fn binary(op: Op, a: &Val, b: &Val) -> Option<EvalResult> {
    let (la, lb) = (matches!(a, Val::List(_)), matches!(b, Val::List(_)));
    if !la && !lb {
        return None;
    }
    Some(match op {
        Op::Mod => Ok(Val::Num(f64::NAN)),
        Op::Multiply if la && lb => product(a, b),
        Op::Divide if lb => Err(EvalError::Other("divide by a matrix".into())),
        Op::Pow => Err(EvalError::Other("pow of a matrix".into())),
        Op::Add
        | Op::Subtract
        | Op::Multiply
        | Op::DotMultiply
        | Op::Divide
        | Op::DotDivide
        | Op::DotPow
        | Op::Equal
        | Op::Unequal
        | Op::Smaller
        | Op::Larger
        | Op::SmallerEq
        | Op::LargerEq
        | Op::BitAnd
        | Op::BitOr
        | Op::BitXor
        | Op::Xor
        | Op::LeftShift
        | Op::RightArithShift
        | Op::RightLogShift => elementwise(op, a, b),
        _ => Err(EvalError::Unsupported(format!("{} on a matrix", op.name()))),
    })
}

/// A unary operator on a list, entry by entry; None for anything else.
pub fn unary(op: Op, a: &Val) -> Option<EvalResult> {
    let Val::List(items) = a else { return None };
    Some(match op {
        Op::UnaryMinus | Op::UnaryPlus | Op::Not | Op::BitNot => items
            .iter()
            .map(|v| ops::unary(op, v))
            .collect::<Result<_, _>>()
            .map(Val::List),
        _ => Err(EvalError::Unsupported(format!("{} on a matrix", op.name()))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(xs: &[f64]) -> Val {
        Val::List(xs.iter().map(|x| Val::Num(*x)).collect())
    }

    #[test]
    fn lists_like_mathjs() {
        let two = Val::Num(2.0);
        assert_eq!(
            binary(Op::Multiply, &list(&[1.0, 2.0, 3.0]), &two),
            Some(Ok(list(&[2.0, 4.0, 6.0])))
        );
        assert_eq!(
            binary(Op::Multiply, &list(&[1.0, 2.0]), &list(&[3.0, 4.0])),
            Some(Ok(Val::Num(11.0)))
        );
        assert_eq!(
            binary(Op::Divide, &list(&[1.0]), &Val::Num(0.0)),
            Some(Ok(list(&[f64::INFINITY])))
        );
        assert!(matches!(
            binary(Op::Mod, &list(&[3.0]), &two),
            Some(Ok(Val::Num(n))) if n.is_nan()
        ));
        assert!(matches!(
            binary(Op::Add, &list(&[1.0, 2.0]), &list(&[1.0])),
            Some(Err(_))
        ));
        assert_eq!(
            unary(Op::UnaryMinus, &list(&[3.0])),
            Some(Ok(list(&[-3.0])))
        );
        assert_eq!(binary(Op::Add, &two, &two), None);
    }
}
