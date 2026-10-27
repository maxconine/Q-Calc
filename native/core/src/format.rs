//! Plain numeric answers as text: `src/engine/format.ts` plus `formatAsFraction` from `scientific.ts`.

use crate::jsnum;
use crate::jsre::{self, Lazy};
use crate::scientific::functions::jsmath;
use crate::types::{Value, ValueKind};

pub const DEFAULT_SIG_FIGS: u32 = 12;
pub const MIN_SIG_FIGS: u32 = 2;
pub const MAX_SIG_FIGS: u32 = 16;

/// Sig figs as the settings allow them: whole, 2 to 16, 12 when unreadable.
pub fn clamp_sig_figs(n: f64) -> u32 {
    if !n.is_finite() {
        return DEFAULT_SIG_FIGS;
    }
    jsnum::round(n).clamp(MIN_SIG_FIGS as f64, MAX_SIG_FIGS as f64) as u32
}

/// `1.5e+7`, trailing zeros of the mantissa dropped.
pub fn format_scientific(n: f64, sig_figs: u32) -> String {
    let s = jsnum::to_exponential(n, sig_figs.saturating_sub(1) as usize);
    let Some(at) = s.find('e') else { return s };
    let (mant, exp) = s.split_at(at);
    let mant = if mant.contains('.') {
        mant.trim_end_matches('0').trim_end_matches('.')
    } else {
        mant
    };
    format!("{mant}{exp}")
}

/// An answer's number as shown: rounded to `sig_figs`, scientific outside 1e-6..1e12.
pub fn format_number(n: f64, sig_figs: f64) -> String {
    if n == f64::INFINITY {
        return "∞".into();
    }
    if n == f64::NEG_INFINITY {
        return "-∞".into();
    }
    if !n.is_finite() {
        return "undefined".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    let figs = clamp_sig_figs(sig_figs);
    let mag = jsmath::log10(n.abs()).floor();
    if !(-6.0..12.0).contains(&mag) {
        return format_scientific(n, figs);
    }
    // more whole digits than the figures shown: rounding would put zeros where real digits are, so the whole
    // number shows instead
    if mag >= figs as f64 {
        return jsnum::to_string(jsnum::round(n));
    }
    let rounded = jsnum::parse(&jsnum::to_precision(n, figs as usize)).unwrap_or(n);
    if !rounded.is_finite() {
        return format_scientific(n, figs);
    }
    let mut s = jsnum::to_string(rounded);
    if s.contains('e') {
        return format_scientific(rounded, figs);
    }
    if let Some((_, frac)) = s.split_once('.') {
        let decimals = (figs as f64 - mag - 1.0).clamp(0.0, 100.0) as usize;
        if frac.len() > decimals {
            let fixed = jsnum::parse(&jsnum::to_fixed(rounded, decimals)).unwrap_or(rounded);
            s = jsnum::to_string(fixed);
        }
    }
    s
}

/// A value as shown, with its unit after a space.
pub fn format_value(value: &Value, sig_figs: f64) -> String {
    if value.kind == ValueKind::Text {
        return value.text.clone().unwrap_or_default();
    }
    let n = format_number(value.n, sig_figs);
    match &value.unit {
        Some(u) if !u.is_empty() => format!("{n} {u}"),
        _ => n,
    }
}

/// GCD of the truncated magnitudes; 0 only when both are 0.
pub fn int_gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.trunc().abs(), b.trunc().abs());
    // JS `while (b)`: NaN (from an infinite argument) is falsy and ends it too; `b != 0.0` alone looped forever
    while b != 0.0 && !b.is_nan() {
        (a, b) = (b, a % b);
    }
    a
}

/// `n` as `p/q` when it reads as an exact fraction with a denominator up to `max_den`; whole numbers as such.
pub fn format_as_fraction(n: f64, max_den: u32) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let x = n.abs();
    if x == 0.0 {
        return Some("0".into());
    }
    // relative, so 5e-13 isn't shown as 0
    if jsnum::round(x) != 0.0 && (x - jsnum::round(x)).abs() < 1e-12 * x {
        return Some(format!("{sign}{}", jsnum::to_string(jsnum::round(x))));
    }
    let (mut best_n, mut best_d, mut best_err) = (1.0, 1.0, f64::INFINITY);
    for d in 1..=max_den {
        let d = d as f64;
        let nm = jsnum::round(x * d);
        let err = (x - nm / d).abs();
        if err < best_err - 1e-18 {
            best_err = err;
            best_n = nm;
            best_d = d;
            if err < 1e-15 {
                break;
            }
        }
    }
    // only exact-looking values; an approximation like 355/113 for π stays decimal
    if best_err > 2e-9 * x {
        return None;
    }
    // float noise can't tell p/q apart once the fractions near x are closer together than a double's spacing
    let noise = best_err <= 4e-16 * x && x * best_d * best_d <= 2.5e11;
    if !noise && best_err.max(2.2e-16 * x) * best_d * best_d > 1e-8 {
        return None;
    }
    let g = int_gcd(best_n, best_d);
    let (p, q) = (jsnum::to_string(best_n / g), jsnum::to_string(best_d / g));
    Some(if best_d == g {
        format!("{sign}{p}")
    } else {
        format!("{sign}{p}/{q}")
    })
}

const MINUS: &str = "−";

/// Display only: groups the digits of a leading number and swaps hyphens for minus signs.
/// Copy and insert keep using the plain ascii answer, which re-parses.
pub fn pretty_answer(s: &str) -> String {
    static LEAD_NUMBER: Lazy = Lazy::new(|| {
        jsre::compile(
            r"^(-?)([$€£¥]?)([0-9]{1,3}(?:,[0-9]{3})+|[0-9]+)(\.[0-9]+)?(e[+-]?[0-9]+)?(?=\s|$)",
        )
    });
    static MATH_ONLY: Lazy = Lazy::new(|| jsre::compile(r"^[0-9\s+\-*/^().,πe√i·×sqrt]+$"));
    static HYPHEN: Lazy = Lazy::new(|| jsre::compile(r"(^|[\s(])-(?=[\s0-9(π√sei])"));
    if let Ok(Some(m)) = LEAD_NUMBER.captures(s) {
        let whole = m.get(0).map_or(0, |w| w.end());
        let sign = if jsre::group(&m, 1).is_empty() {
            ""
        } else {
            MINUS
        };
        let int = group_digits(&jsre::group(&m, 3).replace(',', ""));
        let exp = jsre::group(&m, 5).replacen('-', MINUS, 1);
        return format!(
            "{sign}{}{int}{}{exp}{}",
            jsre::group(&m, 2),
            jsre::group(&m, 4),
            &s[whole..]
        );
    }
    if s == "-∞" {
        return format!("{MINUS}∞");
    }
    if jsre::test(&MATH_ONLY, s) {
        return jsre::replace_all(&HYPHEN, s, |c| format!("{}{MINUS}", jsre::group(c, 1)))
            .into_owned();
    }
    s.to_string()
}

fn group_digits(int: &str) -> String {
    if int.len() < 4 {
        return int.to_string();
    }
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_round_like_the_ts_engine() {
        assert_eq!(format_number(0.1 + 0.2, 12.0), "0.3");
        assert_eq!(format_number(1.0 / 3.0, 12.0), "0.333333333333");
        assert_eq!(format_number(2.0 / 3.0, 6.0), "0.666667");
        assert_eq!(format_number(123456789.0, 6.0), "123456789");
        assert_eq!(format_number(1e12, 12.0), "1e+12");
        assert_eq!(format_number(1.5e-7, 12.0), "1.5e-7");
        assert_eq!(format_number(-0.0, 12.0), "0");
        assert_eq!(format_number(f64::NAN, 12.0), "undefined");
        assert_eq!(format_number(f64::NEG_INFINITY, 12.0), "-∞");
    }

    #[test]
    fn fractions() {
        assert_eq!(format_as_fraction(0.75, 10_000).as_deref(), Some("3/4"));
        assert_eq!(
            format_as_fraction(-1.0 / 3.0, 10_000).as_deref(),
            Some("-1/3")
        );
        assert_eq!(format_as_fraction(4.0, 10_000).as_deref(), Some("4"));
        assert_eq!(format_as_fraction(std::f64::consts::PI, 10_000), None);
    }

    #[test]
    fn pretty() {
        assert_eq!(pretty_answer("-1234567.5"), "−1,234,567.5");
        assert_eq!(pretty_answer("1e-7"), "1e−7");
        assert_eq!(pretty_answer("-2sqrt(3)"), "−2sqrt(3)");
        assert_eq!(pretty_answer("12-05-2024 x"), "12-05-2024 x");
    }
}
