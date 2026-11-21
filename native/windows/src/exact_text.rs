//! Native only (QA items 104 and 105): answer text drawn as maths in the answer slot and the tape, display only
//! (copy, insert and the goldens keep the engine's text). The port of ExactText.swift and `Pretty.unitPowers`
//! (TextRules.swift); pure, so it is tested here.
//!
//! - **An exact form with `pi`** (a sum's `pi^2/6`, `1 - pi/4`): `π²/6`, `1 − π/4`, `3π/2`, `√3·π/6`.
//! - **A typed target unit's powers** (`1e+18 m2`, `kg * m^2/s^2`, `s^-1`): `m²`, `m²/s²`, `s⁻¹`, through the core's
//!   `unit_powers_shown`, memoised (it reads the unit table).

use std::cell::RefCell;
use std::collections::HashMap;

const MINUS: char = '−';

/// What reads as maths alone once `pi` is π (Pretty.answer's `mathOnly`).
fn math_only(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace() || "+-*/^().,πe√i·×±∓sqrt".contains(c))
}

fn superscript(d: char) -> Option<char> {
    Some(match d {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        _ => return None,
    })
}

/// The exact form as maths: real minus signs, roots drawn, a power of digits raised (`π^2` → `π²`), a number or π
/// times π or a root run together (`3*pi` → `3π`), any other `*` a `·`. None when it has no `pi` or isn't maths
/// alone (words, units), so the answer's usual pretty text goes on as before.
pub fn exact_answer(s: &str) -> Option<String> {
    if !s.contains("pi") {
        return None;
    }
    let pi = s.replace("pi", "π");
    if !math_only(&pi) {
        return None;
    }
    let chars: Vec<char> = pi.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            // looseMinus `(^|[\s(])-(?=[\s\d(π√sei])` and binaryMinus `(?<=\S) - (?=\S)`
            '-' => {
                let before = if i == 0 { None } else { Some(chars[i - 1]) };
                let after = chars.get(i + 1).copied();
                let loose = before.is_none_or(|b| b.is_whitespace() || b == '(')
                    && after.is_some_and(|a| {
                        a.is_whitespace() || a.is_ascii_digit() || "(π√sei".contains(a)
                    });
                out.push(if loose { MINUS } else { '-' });
                i += 1;
            }
            // timesSymbol `(?<=[\dπ])\s*\*\s*(?=[π√(]|sqrt)` goes; any other `*` is `·`
            '*' => {
                let mut b = out.len();
                while b > 0 && out[b - 1].is_whitespace() {
                    b -= 1;
                }
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                // the character before in the text as written (`sqrt(3)*pi` ends in `)`, so it keeps its `·`)
                let before = chars[..i]
                    .iter()
                    .rev()
                    .find(|c| !c.is_whitespace())
                    .copied();
                let rest: String = chars[j..].iter().take(4).collect();
                let together = before.is_some_and(|p| p.is_ascii_digit() || p == 'π')
                    && (rest.starts_with(['π', '√', '(']) || rest.starts_with("sqrt"));
                if together {
                    out.truncate(b);
                    i = j;
                } else {
                    out.push('·');
                    i += 1;
                }
            }
            // shortRoot `sqrt\(([0-9.]+|[A-Za-zπ])\)` → `√2`; any other `sqrt(` → `√(`
            's' if chars[i..].starts_with(&['s', 'q', 'r', 't', '(']) => {
                let arg_start = i + 5;
                let mut j = arg_start;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    j += 1;
                }
                let short = if j > arg_start && chars.get(j) == Some(&')') {
                    Some(j)
                } else if chars
                    .get(arg_start)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == 'π')
                    && chars.get(arg_start + 1) == Some(&')')
                {
                    Some(arg_start + 1)
                } else {
                    None
                };
                out.push('√');
                match short {
                    Some(close) => {
                        out.extend(&chars[arg_start..close]);
                        i = close + 1;
                    }
                    None => {
                        out.push('(');
                        i = arg_start;
                    }
                }
            }
            // digitPower `\^(\d+)` raised
            '^' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => {
                i += 1;
                while let Some(&d) = chars.get(i).filter(|d| d.is_ascii_digit()) {
                    out.extend(superscript(d));
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    // binaryMinus: a spaced hyphen between two terms the loose rule left (`x - 1` after a letter)
    let mut t: String = out.into_iter().collect();
    if t.contains(" - ") {
        t = t.replace(" - ", &format!(" {MINUS} "));
    }
    Some(t)
}

/// Whether an answer may carry a typed unit's power (`Pretty.answer`'s guard): it starts with a number or a sign and
/// has a letter followed by `^digit`, `^-digit`, or a bare 2 or 3 not followed by a letter or digit.
fn may_have_unit_power(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    if !(first.is_ascii_digit() || first == '-' || first == '±') {
        return false;
    }
    let c: Vec<char> = s.chars().collect();
    (1..c.len()).any(|i| {
        let p = c[i - 1];
        if !(p.is_ascii_alphabetic() || "µμΩÅ".contains(p)) {
            return false;
        }
        match c[i] {
            '^' => c.get(i + 1).is_some_and(|&d| {
                d.is_ascii_digit() || (d == '-' && c.get(i + 2).is_some_and(char::is_ascii_digit))
            }),
            '2' | '3' => !c.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric()),
            _ => false,
        }
    })
}

thread_local! {
    static POWERS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// A typed target unit's powers as superscripts (core `unit_powers_shown`), memoised: the tape draws the same rows
/// again and again. Unchanged when it can't have one.
pub fn unit_powers(s: &str) -> std::borrow::Cow<'_, str> {
    if !may_have_unit_power(s) {
        return std::borrow::Cow::Borrowed(s);
    }
    POWERS.with(|memo| {
        let mut memo = memo.borrow_mut();
        if let Some(hit) = memo.get(s) {
            return std::borrow::Cow::Owned(hit.clone());
        }
        let shown = qcalc_core::units::unit_powers_shown(s);
        if memo.len() > 256 {
            memo.clear();
        }
        memo.insert(s.to_string(), shown.clone());
        std::borrow::Cow::Owned(shown)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_forms_with_pi_read_as_maths() {
        assert_eq!(exact_answer("pi^2/6").as_deref(), Some("π²/6"));
        assert_eq!(exact_answer("pi^4/90").as_deref(), Some("π⁴/90"));
        assert_eq!(exact_answer("1 - pi/4").as_deref(), Some("1 − π/4"));
        assert_eq!(exact_answer("3*pi/2").as_deref(), Some("3π/2"));
        assert_eq!(exact_answer("sqrt(3)*pi/6").as_deref(), Some("√3·π/6"));
        assert_eq!(exact_answer("-pi/2").as_deref(), Some("−π/2"));
        assert_eq!(exact_answer("2 - pi^2/6").as_deref(), Some("2 − π²/6"));
        assert_eq!(exact_answer("sqrt(2 + pi)").as_deref(), Some("√(2 + π)"));
        assert_eq!(exact_answer("pi*sqrt(2)").as_deref(), Some("π√2"));
        // no pi, or words and units: as before
        assert_eq!(exact_answer("1/2"), None);
        assert_eq!(exact_answer("pi m"), None);
        assert_eq!(exact_answer("2 pints"), None);
    }

    #[test]
    fn a_typed_units_powers_are_raised() {
        assert!(may_have_unit_power("1e+18 m2"));
        assert!(may_have_unit_power("100 kg * m^2/s^2"));
        assert!(may_have_unit_power("1 s^-1"));
        assert!(!may_have_unit_power("42"));
        assert!(!may_have_unit_power("x2 + 1"));
        assert!(!may_have_unit_power("2 H2O"));
        assert_eq!(unit_powers("1e+18 m2"), "1e+18 m²");
        assert_eq!(unit_powers("100 kg * m^2/s^2"), "100 kg * m²/s²");
        assert_eq!(unit_powers("1 s^-1"), "1 s⁻¹");
        assert_eq!(unit_powers("12 in"), "12 in");
    }
}
