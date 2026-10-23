//! Closed forms beside a decimal (`src/engine/simplify.ts`): `2sqrt(3)`, `pi/4`, `(sqrt(6)-sqrt(2))/4`.

use crate::format::int_gcd;
use crate::jsnum;
use crate::jsre::{self, Lazy};

const NICE_DEN: &[f64] = &[2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 12.0];

fn nested() -> [(&'static str, f64); 8] {
    let (s6, s2, s3) = (6f64.sqrt(), 2f64.sqrt(), 3f64.sqrt());
    [
        ("(sqrt(6)-sqrt(2))/4", (s6 - s2) / 4.0),
        ("(sqrt(6)+sqrt(2))/4", (s6 + s2) / 4.0),
        ("-(sqrt(6)-sqrt(2))/4", -(s6 - s2) / 4.0),
        ("-(sqrt(6)+sqrt(2))/4", -(s6 + s2) / 4.0),
        ("2-sqrt(3)", 2.0 - s3),
        ("2+sqrt(3)", 2.0 + s3),
        ("-(2-sqrt(3))", -(2.0 - s3)),
        ("-(2+sqrt(3))", -(2.0 + s3)),
    ]
}

type Tol = fn(f64) -> f64;

// an exact form claims every digit the decimal shows, so it may only differ by float noise
fn strict(x: f64) -> f64 {
    1e-15 + 1e-14 * x.abs()
}

// solve checks each candidate on the equation itself, so it can afford to look wider
fn wide(x: f64) -> f64 {
    1e-11 + 1e-14 * x.abs()
}

fn close(a: f64, b: f64, tol: Tol) -> bool {
    (a - b).abs() <= tol(b)
}

fn to_fraction(x: f64, max_den: u32, eps: f64) -> Option<(f64, f64)> {
    if !x.is_finite() {
        return None;
    }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs = x.abs();
    let (mut best_n, mut best_d, mut best_err) = (1.0, 1.0, f64::INFINITY);
    for d in 1..=max_den {
        let d = f64::from(d);
        let nm = jsnum::round(abs * d);
        let err = (abs - nm / d).abs();
        if err < best_err {
            best_err = err;
            best_n = nm;
            best_d = d;
            if err < 1e-16 {
                break;
            }
        }
    }
    if best_err > eps {
        return None;
    }
    let g = int_gcd(best_n, best_d);
    Some((sign * (best_n / g), best_d / g))
}

fn snap_integer(n: f64, tol: Tol) -> Option<f64> {
    if !n.is_finite() {
        return None;
    }
    let r = jsnum::round(n);
    (close(n, r, tol) && r.abs() < 1e12).then_some(r)
}

fn s(n: f64) -> String {
    jsnum::to_string(n)
}

fn format_pi(num: f64, den: f64) -> String {
    let sign = if num < 0.0 { "-" } else { "" };
    let n = num.abs();
    if n == 0.0 {
        return "0".into();
    }
    let coeff = if n == 1.0 { String::new() } else { format!("{}*", s(n)) };
    let den = if den == 1.0 { String::new() } else { format!("/{}", s(den)) };
    format!("{sign}{coeff}pi{den}")
}

/// `n` as `coeff * sqrt(rad)` with `rad` square-free.
pub fn split_squares(n: f64) -> (f64, f64) {
    let mut rad = jsnum::round(n.abs());
    if rad <= 0.0 {
        return (0.0, 1.0);
    }
    let mut coeff = 1.0;
    let mut i = 2.0;
    while i * i <= rad {
        let sq = i * i;
        while rad % sq == 0.0 {
            rad /= sq;
            coeff *= i;
        }
        i += 1.0;
    }
    (coeff, rad)
}

fn radical_term(coeff: f64, rad: f64) -> String {
    if rad == 1.0 {
        return s(coeff);
    }
    let core = format!("sqrt({})", s(rad));
    if coeff == 1.0 { core } else { format!("{}{core}", s(coeff)) }
}

fn format_radical(sign: &str, coeff: f64, rad: f64, den: f64) -> String {
    let g = int_gcd(coeff, den);
    let (coeff, den) = (coeff / g, den / g);
    let core = radical_term(coeff, rad);
    if den == 1.0 { format!("{sign}{core}") } else { format!("{sign}{core}/{}", s(den)) }
}

fn format_unrationalized(sign: &str, num: (f64, f64), den: (f64, f64)) -> String {
    let g = int_gcd(num.0, den.0);
    let (nc, dc) = (num.0 / g, den.0 / g);
    let n = radical_term(nc, num.1);
    if dc == 1.0 && den.1 == 1.0 {
        return format!("{sign}{n}");
    }
    let d = radical_term(dc, den.1);
    if dc != 1.0 && den.1 != 1.0 { format!("{sign}{n}/({d})") } else { format!("{sign}{n}/{d}") }
}

fn as_pi_multiple(n: f64, tol: Tol) -> Option<String> {
    let x = n / std::f64::consts::PI;
    let (fnum, fden) = to_fraction(x, 24, tol(x))?;
    Some(if fnum == 0.0 { "0".into() } else { format_pi(fnum, fden) })
}

fn as_nested_radical(n: f64, tol: Tol) -> Option<String> {
    nested().iter().find(|(_, v)| close(n, *v, tol)).map(|(t, _)| (*t).to_string())
}

fn as_radical(n: f64, rationalize: bool, tol: Tol) -> Option<String> {
    let sign = if n < 0.0 { "-" } else { "" };
    let x = n.abs();
    let (fnum, fden) = to_fraction(x * x, 256, 2.0 * tol(x * x))?;
    if fnum <= 0.0 {
        return None;
    }
    if rationalize {
        let (coeff, rad) = split_squares(fnum * fden);
        if rad == 1.0 {
            return None;
        }
        return Some(format_radical(sign, coeff, rad, fden));
    }
    let num = split_squares(fnum);
    let den = split_squares(fden);
    if num.1 == 1.0 && den.1 == 1.0 {
        return None;
    }
    Some(format_unrationalized(sign, num, den))
}

fn as_nice_fraction(n: f64, tol: Tol) -> Option<String> {
    let (fnum, fden) = to_fraction(n, 12, tol(n))?;
    if !NICE_DEN.contains(&fden) {
        return None;
    }
    Some(if fnum == 0.0 { "0".into() } else { format!("{}/{}", s(fnum), s(fden)) })
}

/// The closed form is only shown beside the decimal for trig and square-root expressions.
pub fn wants_exact_form(expr: &str) -> bool {
    static RE: Lazy = Lazy::new(|| {
        jsre::js(
            r"√|\\sqrt|(?:^|[^A-Za-z_])(?:sqrt|arcsin|arccos|arctan2|arctan|arccsc|arcsec|arccot|asin|acos|atan2|atan|acsc|asec|acot|sin|cos|tan|csc|sec|cot)(?![A-Za-z_])",
            "i",
        )
    });
    jsre::test(&RE, expr)
}

/// Options for `exact_form`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactFormOptions {
    /// Move square roots out of the denominator; on unless `Some(false)`.
    pub rationalize: Option<bool>,
    /// Candidates the caller verifies itself.
    pub wide: bool,
}

/// None when no closed form is nicer than the decimal.
pub fn exact_form(n: f64, options: ExactFormOptions) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    // no closed form here is this small, and 0 would deny a real 1e-11
    if n.abs() >= 1e-15 && n.abs() < 1e-3 {
        return None;
    }
    let tol: Tol = if options.wide { wide } else { strict };
    if let Some(i) = snap_integer(n, tol) {
        return Some(s(i));
    }
    // past this any big enough radicand or numerator fits the tolerance by chance
    if n.abs() >= 1e4 {
        return None;
    }
    as_nested_radical(n, tol)
        .or_else(|| as_pi_multiple(n, tol))
        .or_else(|| as_radical(n, options.rationalize != Some(false), tol))
        .or_else(|| as_nice_fraction(n, tol))
}

/// `exact ≈ decimal` when the two differ.
pub fn dual_label(exact: Option<&str>, display: &str) -> String {
    match exact {
        Some(e) if e != display => format!("{e} ≈ {display}"),
        _ => display.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_forms() {
        let o = ExactFormOptions::default();
        assert_eq!(exact_form(2.0 * 3f64.sqrt(), o).as_deref(), Some("2sqrt(3)"));
        assert_eq!(exact_form(std::f64::consts::FRAC_PI_4, o).as_deref(), Some("pi/4"));
        assert_eq!(exact_form(1.0 / 2f64.sqrt(), o).as_deref(), Some("sqrt(2)/2"));
        let keep = ExactFormOptions { rationalize: Some(false), ..o };
        assert_eq!(exact_form(1.0 / 2f64.sqrt(), keep).as_deref(), Some("1/sqrt(2)"));
        assert_eq!(exact_form(4.0, o).as_deref(), Some("4"));
        assert_eq!(exact_form(0.0005, o), None);
        assert!(wants_exact_form("sin(30)"));
        assert!(!wants_exact_form("2+2"));
        assert_eq!(dual_label(Some("pi/4"), "0.785398"), "pi/4 ≈ 0.785398");
    }
}
