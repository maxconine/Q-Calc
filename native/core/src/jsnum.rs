//! JavaScript's number-to-text rules, bit for bit: `String(n)`, `toPrecision`, `toExponential`, `toFixed` and
//! `Math.round`. The TS engine formats every answer with these, so parity depends on matching them exactly,
//! including their round-half-up on the exact binary value (Rust's `format!` rounds half to even).

use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};

/// The exact decimal expansion of a finite, non-zero `|x|`: digits `d1 d2 …` (no leading or trailing zeros) and
/// `point`, so that `|x| = 0.d1d2… × 10^point`.
fn exact_digits(x: f64) -> (Vec<u8>, i32) {
    let bits = x.abs().to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp2) = if exp_bits == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_bits - 1075)
    };
    let (int, scale) = if exp2 >= 0 {
        (BigUint::from(mant) << exp2 as usize, 0i32)
    } else {
        // mant / 2^k is mant * 5^k / 10^k
        let k = (-exp2) as u32;
        (BigUint::from(mant) * BigUint::from(5u32).pow(k), k as i32)
    };
    let s = int.to_str_radix(10);
    let mut digits: Vec<u8> = s.bytes().map(|b| b - b'0').collect();
    let point = digits.len() as i32 - scale;
    while digits.last() == Some(&0) {
        digits.pop();
    }
    (digits, point)
}

/// Keeps the first `keep` digits of an exact expansion, rounding half up; `point` moves when 9s carry over.
/// `keep` may be 0 or negative (the value rounds to 0 or to one unit at that place).
fn round_half_up(digits: &[u8], point: i32, keep: i32) -> (Vec<u8>, i32) {
    if keep < 0 {
        return (Vec::new(), point);
    }
    let keep = keep as usize;
    if digits.len() <= keep {
        return (digits.to_vec(), point);
    }
    let mut out = digits[..keep].to_vec();
    if digits[keep] < 5 {
        return (out, point);
    }
    let mut i = keep;
    while i > 0 {
        i -= 1;
        if out[i] == 9 {
            out[i] = 0;
        } else {
            out[i] += 1;
            return (out, point);
        }
    }
    // carried past the first digit: a 1 one place higher, then zeros
    let mut carried = vec![1];
    carried.extend(std::iter::repeat_n(0, keep));
    carried.truncate(keep.max(1));
    (carried, point + 1)
}

fn digits_text(digits: &[u8]) -> String {
    digits.iter().map(|d| char::from(b'0' + d)).collect()
}

/// Shortest digits that round-trip, from Rust's own shortest formatting (the same choice V8 makes).
fn shortest_digits(x: f64) -> (Vec<u8>, i32) {
    let s = format!("{:e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: Vec<u8> = mant
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    (digits, exp + 1)
}

fn special(x: f64) -> Option<String> {
    if x.is_nan() {
        Some("NaN".into())
    } else if x == f64::INFINITY {
        Some("Infinity".into())
    } else if x == f64::NEG_INFINITY {
        Some("-Infinity".into())
    } else {
        None
    }
}

fn sign(x: f64) -> &'static str {
    if x < 0.0 { "-" } else { "" }
}

fn exponent_text(e: i32) -> String {
    if e < 0 {
        format!("e-{}", -e)
    } else {
        format!("e+{e}")
    }
}

/// `String(x)` / `x.toString()`.
pub fn to_string(x: f64) -> String {
    if let Some(s) = special(x) {
        return s;
    }
    if x == 0.0 {
        return "0".into();
    }
    let (digits, n) = shortest_digits(x);
    let k = digits.len() as i32;
    let d = digits_text(&digits);
    let body = if k <= n && n <= 21 {
        format!("{d}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &d[..n as usize], &d[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{d}", "0".repeat((-n) as usize))
    } else {
        let rest = if k > 1 {
            format!(".{}", &d[1..])
        } else {
            String::new()
        };
        format!("{}{rest}{}", &d[..1], exponent_text(n - 1))
    };
    format!("{}{body}", sign(x))
}

/// `x.toExponential(f)` for `f` in 0..=100.
pub fn to_exponential(x: f64, f: usize) -> String {
    if let Some(s) = special(x) {
        return s;
    }
    if x == 0.0 {
        let frac = if f > 0 {
            format!(".{}", "0".repeat(f))
        } else {
            String::new()
        };
        return format!("0{frac}e+0");
    }
    let (digits, point) = exact_digits(x);
    let (mut r, point) = round_half_up(&digits, point, f as i32 + 1);
    r.resize(f + 1, 0);
    let d = digits_text(&r);
    let frac = if f > 0 {
        format!(".{}", &d[1..])
    } else {
        String::new()
    };
    format!("{}{}{frac}{}", sign(x), &d[..1], exponent_text(point - 1))
}

/// `x.toPrecision(p)` for `p` in 1..=100.
pub fn to_precision(x: f64, p: usize) -> String {
    if let Some(s) = special(x) {
        return s;
    }
    if x == 0.0 {
        return if p > 1 {
            format!("0.{}", "0".repeat(p - 1))
        } else {
            "0".into()
        };
    }
    let (digits, point) = exact_digits(x);
    let (mut r, point) = round_half_up(&digits, point, p as i32);
    r.resize(p, 0);
    let d = digits_text(&r);
    let e = point - 1;
    let body = if e < -6 || e >= p as i32 {
        let frac = if p > 1 {
            format!(".{}", &d[1..])
        } else {
            String::new()
        };
        format!("{}{frac}{}", &d[..1], exponent_text(e))
    } else if e >= 0 {
        let whole = (e + 1) as usize;
        if whole >= p {
            d
        } else {
            format!("{}.{}", &d[..whole], &d[whole..])
        }
    } else {
        format!("0.{}{d}", "0".repeat((-e - 1) as usize))
    };
    format!("{}{body}", sign(x))
}

/// `x.toFixed(f)` for `f` in 0..=100.
pub fn to_fixed(x: f64, f: usize) -> String {
    if let Some(s) = special(x) {
        return s;
    }
    if x.abs() >= 1e21 {
        return to_string(x);
    }
    let neg = x < 0.0;
    let text = if x == 0.0 {
        "0".to_string()
    } else {
        let (digits, point) = exact_digits(x);
        let (mut r, point) = round_half_up(&digits, point, point + f as i32);
        // the integer n with n / 10^f nearest |x|, as a digit string
        let len = (point + f as i32).max(0) as usize;
        r.resize(len, 0);
        let n = if r.is_empty() {
            "0".to_string()
        } else {
            digits_text(&r)
        };
        n.trim_start_matches('0').to_string()
    };
    let n = if text.is_empty() {
        "0".to_string()
    } else {
        text
    };
    let body = if f == 0 {
        n
    } else {
        let padded = if n.len() <= f {
            format!("{}{n}", "0".repeat(f + 1 - n.len()))
        } else {
            n
        };
        let at = padded.len() - f;
        format!("{}.{}", &padded[..at], &padded[at..])
    };
    format!("{}{body}", if neg { "-" } else { "" })
}

/// `Math.round(x)`: to the nearest integer, a tie going towards +∞.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() || x == x.trunc() {
        return x;
    }
    let r = x.floor();
    let out = if x - r >= 0.5 { r + 1.0 } else { r };
    // Math.round(-0.4) is -0
    if out == 0.0 && x < 0.0 { -0.0 } else { out }
}

/// `Number(text)` for the plain decimal and e-notation numbers the parser produces.
pub fn parse(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return Some(0.0);
    }
    t.parse::<f64>().ok()
}

/// `Number.isSafeInteger(x)`.
pub fn is_safe_integer(x: f64) -> bool {
    x.is_finite() && x == x.trunc() && x.abs() <= 9_007_199_254_740_991.0
}

/// `Number.isInteger(x)`.
pub fn is_integer(x: f64) -> bool {
    x.is_finite() && x == x.trunc()
}

/// The exact value of a finite double as a big rational's numerator and denominator.
pub fn exact_ratio(x: f64) -> Option<(bool, BigUint, BigUint)> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 {
        return Some((false, BigUint::zero(), BigUint::one()));
    }
    let bits = x.abs().to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp2) = if exp_bits == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_bits - 1075)
    };
    let (num, den) = if exp2 >= 0 {
        (BigUint::from(mant) << exp2 as usize, BigUint::one())
    } else {
        (BigUint::from(mant), BigUint::one() << (-exp2) as usize)
    };
    Some((x < 0.0, num, den))
}

/// Exactly representable small integers, for loops that index by value.
pub fn as_usize(x: f64) -> Option<usize> {
    if is_integer(x) && x >= 0.0 {
        BigUint::from(x as u64).to_usize()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_string_matches_js() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (-2.5, "-2.5"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for &(x, want) in cases {
            assert_eq!(to_string(x), want, "{x:e}");
        }
    }

    #[test]
    fn to_precision_rounds_half_up_on_the_exact_value() {
        assert_eq!(to_precision(2.5, 1), "3");
        assert_eq!(to_precision(0.5, 1), "0.5");
        assert_eq!(to_precision(1.005, 3), "1.00");
        assert_eq!(to_precision(123.456, 4), "123.5");
        assert_eq!(to_precision(123456.0, 2), "1.2e+5");
        assert_eq!(to_precision(0.000001234, 2), "0.0000012");
        assert_eq!(to_precision(0.0000001234, 2), "1.2e-7");
        assert_eq!(to_precision(9.99, 2), "10");
        assert_eq!(to_precision(99.99, 2), "1.0e+2");
        assert_eq!(to_precision(-1.25, 2), "-1.3");
        assert_eq!(to_precision(0.0, 3), "0.00");
    }

    #[test]
    fn to_exponential_and_fixed() {
        assert_eq!(to_exponential(123456.0, 2), "1.23e+5");
        assert_eq!(to_exponential(0.00015, 0), "1e-4");
        assert_eq!(to_exponential(9.5, 0), "1e+1");
        assert_eq!(to_exponential(1.0, 3), "1.000e+0");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(0.000001, 3), "0.000");
        assert_eq!(to_fixed(-0.0001, 2), "-0.00");
        assert_eq!(to_fixed(123.456, 1), "123.5");
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(1e21, 2), "1e+21");
    }

    #[test]
    fn math_round_ties_go_up() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(-2.6), -3.0);
        assert_eq!(round(0.49999999999999994), 0.0);
        assert!(round(-0.4).is_sign_negative());
    }
}
