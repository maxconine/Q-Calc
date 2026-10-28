//! What a word in the typed line is, for the bar's hover label: a unit's spelled-out name (`kPa` is kilopascal,
//! from `unitSpans`), a function's full name (`gcf` is greatest common factor) and a constant's (`π` is pi).
//!
//! Native only. The web input names units on hover (`quick-unit-tip` in `QuickInput.tsx`, the name from
//! `unitSpans`); the native bar names functions and constants the same way.

use crate::app::blank_reason::Span;
use crate::app::js::index16;
use crate::app::known_words::known_word_spans;
use crate::app::messaging::is_message_command;
use crate::app::unit_spans::{UnitSpan, unit_spans};

/// The full name of each engine function (`crate::scientific::NAMES`), by its lowercase spelling.
fn function_name(word: &str) -> Option<&'static str> {
    Some(match word {
        "sqrt" => "square root",
        "cbrt" => "cube root",
        "nthroot" => "nth root",
        "sin" => "sine",
        "cos" => "cosine",
        "tan" => "tangent",
        "csc" => "cosecant",
        "sec" => "secant",
        "cot" => "cotangent",
        "asin" | "arcsin" => "inverse sine",
        "acos" | "arccos" => "inverse cosine",
        "atan" | "arctan" => "inverse tangent",
        "atan2" | "arctan2" => "two-argument inverse tangent",
        "acsc" | "arccsc" => "inverse cosecant",
        "asec" | "arcsec" => "inverse secant",
        "acot" | "arccot" => "inverse cotangent",
        "sinh" => "hyperbolic sine",
        "cosh" => "hyperbolic cosine",
        "tanh" => "hyperbolic tangent",
        "csch" => "hyperbolic cosecant",
        "sech" => "hyperbolic secant",
        "coth" => "hyperbolic cotangent",
        "asinh" | "arsinh" | "arcsinh" => "inverse hyperbolic sine",
        "acosh" | "arcosh" | "arccosh" => "inverse hyperbolic cosine",
        "atanh" | "artanh" | "arctanh" => "inverse hyperbolic tangent",
        "acsch" | "arccsch" => "inverse hyperbolic cosecant",
        "asech" | "arcsech" => "inverse hyperbolic secant",
        "acoth" | "arccoth" => "inverse hyperbolic cotangent",
        "ln" => "natural logarithm",
        "log" => "logarithm",
        "log2" => "base-2 logarithm",
        "log10" => "base-10 logarithm",
        "exp" => "exponential",
        "abs" => "absolute value",
        "sign" => "sign",
        "floor" => "floor (round down)",
        "ceil" => "ceiling (round up)",
        "round" => "round",
        "clamp" => "clamp",
        "min" => "minimum",
        "max" => "maximum",
        "mean" => "mean (average)",
        "median" => "median",
        "mad" => "mean absolute deviation",
        "std" | "stdev" => "standard deviation",
        "stdevp" => "population standard deviation",
        "var" => "variance",
        "varp" => "population variance",
        "sum" | "total" => "sum",
        "length" | "count" => "count",
        "quartile" => "quartile",
        "quantile" => "quantile",
        "corr" => "correlation",
        "gcd" => "greatest common divisor",
        "gcf" => "greatest common factor",
        "hcf" => "highest common factor",
        "lcm" | "lcf" => "least common multiple",
        "lcd" => "least common denominator",
        "mod" => "modulo",
        "hypot" => "hypotenuse",
        "factorial" => "factorial",
        "ncr" | "combinations" => "combinations",
        "npr" | "permutations" => "permutations",
        "randint" => "random integer",
        "rand" | "random" => "random number",
        "re" | "real" => "real part",
        "im" | "imag" => "imaginary part",
        "conj" => "complex conjugate",
        "arg" => "argument (angle)",
        "range" | "inclusiverange" => "range",
        "zeta" => "Riemann zeta function",
        "catalan" => "Catalan's constant",
        "ans" => "last answer",
        _ => return None,
    })
}

/// The constants the engine reads, spelled or as a sign.
fn constant_name(word: &str) -> Option<&'static str> {
    Some(match word {
        "pi" | "π" => "pi",
        "tau" | "τ" => "tau (2π)",
        "e" => "Euler's number",
        "i" => "imaginary unit",
        "phi" | "φ" | "ϕ" => "golden ratio",
        "inf" | "infinity" | "infty" | "∞" => "infinity",
        _ => return None,
    })
}

/// Each unit, function and constant in the line with its full name, UTF-16 offsets, in order. A unit's name wins
/// where a word reads as both; a variable or function of the user's own keeps its name to itself (`e` stays
/// Euler's number, as the engine reads it). A `message …` line is words, so nothing in it is named.
pub fn word_names(
    text: &str,
    variables: &[String],
    functions: &[String],
    ans: bool,
) -> Vec<UnitSpan> {
    if is_message_command(text) {
        return Vec::new();
    }
    let mut out = unit_spans(text, variables, functions);
    let free = |out: &[UnitSpan], s: &Span| !out.iter().any(|u| u.start < s.end && s.start < u.end);
    let units16: Vec<u16> = text.encode_utf16().collect();
    let word16 = |s: &Span| String::from_utf16_lossy(&units16[s.start..s.end]).to_lowercase();
    for s in known_word_spans(text, functions, ans) {
        let word = word16(&s);
        if functions.iter().any(|f| f.to_lowercase() == word) {
            continue;
        }
        if let Some(name) = function_name(&word)
            && free(&out, &s)
        {
            out.push(UnitSpan {
                start: s.start,
                end: s.end,
                name: name.into(),
            });
        }
    }
    for (start, end) in constant_words(text) {
        let word = &text[start..end];
        let mine = variables.iter().chain(functions).any(|v| v == word);
        if (mine && word != "e") || exponent_e(text, start, end) {
            continue;
        }
        let Some(name) = constant_name(&word.to_lowercase()) else {
            continue;
        };
        let s = Span {
            start: index16(text, start),
            end: index16(text, end),
        };
        if free(&out, &s) {
            out.push(UnitSpan {
                start: s.start,
                end: s.end,
                name: name.into(),
            });
        }
    }
    out.sort_by_key(|u| u.start);
    out
}

/// Whole ASCII words (`2pi` and `4i` too, but not inside another word or a name like `x_pi`) and the constant signs, as byte ranges.
fn constant_words(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if matches!(c, 'π' | 'τ' | 'φ' | 'ϕ' | '∞') {
            out.push((i, i + c.len_utf8()));
        } else if c.is_ascii_alphabetic()
            && !prev.is_some_and(|p| p.is_ascii_alphabetic() || p == '_')
        {
            let mut end = i + 1;
            let mut last = c;
            while let Some(&(j, d)) = it.peek() {
                if !d.is_ascii_alphabetic() {
                    break;
                }
                end = j + 1;
                last = d;
                it.next();
            }
            let next = text[end..].chars().next();
            if !next.is_some_and(|n| n.is_ascii_alphanumeric() || n == '_') {
                out.push((i, end));
            }
            prev = Some(last);
            continue;
        }
        prev = Some(c);
    }
    out
}

/// The `e` of `2e-3`: a number's exponent, not Euler's number.
fn exponent_e(text: &str, start: usize, end: usize) -> bool {
    if &text[start..end] != "e" && &text[start..end] != "E" {
        return false;
    }
    let before = text[..start].chars().last();
    let mut after = text[end..].chars();
    let sign = after.next();
    before.is_some_and(|c| c.is_ascii_digit() || c == '.')
        && matches!(sign, Some('+' | '-'))
        && after.next().is_some_and(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<(String, String)> {
        let units: Vec<u16> = text.encode_utf16().collect();
        word_names(text, &[], &[], true)
            .into_iter()
            .map(|u| (String::from_utf16_lossy(&units[u.start..u.end]), u.name))
            .collect()
    }

    fn pair(word: &str, name: &str) -> (String, String) {
        (word.into(), name.into())
    }

    #[test]
    fn functions_by_their_full_names() {
        assert_eq!(
            names("gcf(12, 18)"),
            vec![pair("gcf", "greatest common factor")]
        );
        assert_eq!(
            names("lcm(4, 6)"),
            vec![pair("lcm", "least common multiple")]
        );
        assert_eq!(
            names("LCF(4, 6)"),
            vec![pair("LCF", "least common multiple")]
        );
        assert_eq!(
            names("sin(1) + arccosh(2)"),
            vec![
                pair("sin", "sine"),
                pair("arccosh", "inverse hyperbolic cosine")
            ]
        );
        // `logx` names just the log, as it lights
        assert_eq!(names("logx"), vec![pair("log", "logarithm")]);
        assert_eq!(names("nCr(5, 2)"), vec![pair("nCr", "combinations")]);
    }

    #[test]
    fn units_keep_their_unitspans_names() {
        assert_eq!(names("5 kPa"), vec![pair("kPa", "kilopascal")]);
        assert_eq!(
            names("10 km to mi"),
            vec![pair("km", "kilometer"), pair("mi", "mile")]
        );
        // `min(…)` is the function; `5 min` the unit
        assert_eq!(names("min(1, 2)"), vec![pair("min", "minimum")]);
        assert_eq!(names("5 min").len(), 1);
        assert_ne!(names("5 min")[0].1, "minimum");
    }

    #[test]
    fn constants_spelled_or_signed() {
        assert_eq!(names("2π"), vec![pair("π", "pi")]);
        assert_eq!(names("2 pi"), vec![pair("pi", "pi")]);
        assert_eq!(names("e^2"), vec![pair("e", "Euler's number")]);
        assert_eq!(names("3 + 4i"), vec![pair("i", "imaginary unit")]);
        assert_eq!(names("∞"), vec![pair("∞", "infinity")]);
        assert_eq!(
            names("phi + tau"),
            vec![pair("phi", "golden ratio"), pair("tau", "tau (2π)")]
        );
        assert_eq!(names("ans + 1"), vec![pair("ans", "last answer")]);
    }

    #[test]
    fn not_constants() {
        // a number's exponent, words that only contain a constant, a name with one in it
        assert!(names("2e-3").is_empty());
        assert!(names("2e5").is_empty());
        assert!(names("pie").is_empty());
        assert!(names("x_pi").is_empty());
        // a variable of the user's own is theirs, but e is Euler's number whatever
        assert!(word_names("i + 1", &["i".into()], &[], false).is_empty());
        assert_eq!(word_names("e + 1", &["e".into()], &[], false).len(), 1);
        // a function of the user's own has no name to give
        assert!(word_names("f(2)", &[], &["f".into()], false).is_empty());
        // ans without an answer is just a word
        assert!(word_names("ans", &[], &[], false).is_empty());
        // a message is words
        assert!(names("message max pi day").is_empty());
    }

    #[test]
    fn utf16_offsets() {
        let out = word_names("π + sqrt(2)", &[], &[], false);
        assert_eq!((out[0].start, out[0].end), (0, 1));
        assert_eq!((out[1].start, out[1].end), (4, 8));
    }
}
