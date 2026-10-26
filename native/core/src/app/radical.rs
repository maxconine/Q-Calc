//! `src/lib/radical.ts`: where each √ or ∛ typed in the bar ends (for the bar drawn over its radicand), and exact
//! answers' `sqrt(…)` shown as `√…`.
//!
//! Span offsets are UTF-16 units, as the TS (and AppKit's `NSRange`) count them. The text is scanned by `char`
//! and each offset converted; every marker it tests is a BMP character, so both scans stop at the same places.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::jsre::{self, Lazy};
use crate::jsstr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadicalSpan {
    pub sign: usize,
    pub end: usize,
}

// what the engine takes as a bare radicand (BARE_ATOM_RE in engine/scientific.ts, read on the raw text): a number
// with an optional π or pi coefficient and powers, or π or τ alone; `√2x` is √2 times x, `√2π` is √(2π) but `√2τ`
// is √2·τ; a coefficient right before a letter (or a symbol that becomes one) stays outside: `√2πx` is √2·π·x
static BARE_RADICAND: Lazy = Lazy::new(|| {
    jsre::js(
        &jsstr::spaces(
            r"^(?:[πτ]|\d+(?:\.\d+)?(?:e[+-]?\d+)?(?:(?:[*×·⋅∙]?(?:π|pi|tau)|[*×·⋅∙]τ)(?![A-Za-z0-9_√∛∞θ]))?)(?:\s*\^\s*(?:[-−]?\d+(?:\.\d+)?(?![\d.])|[πτ]|\([^()πτ%|°]*\)))*",
        ),
        "i",
    )
});

static WORDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut out: Vec<String> = Vec::new();
    for w in format!("{}|theta", crate::scientific::NAMES)
        .to_lowercase()
        .split('|')
    {
        if !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
    }
    out
});

/// The engine roots a lone letter only (`√xy` is √x·y), so the bar covers just that letter; a word like `√sin`
/// isn't one.
fn bare_letter(rest: &[char]) -> bool {
    let Some(&first) = rest.first() else {
        return false;
    };
    if !first.is_ascii_alphabetic()
        || rest
            .get(1)
            .is_some_and(|&c| c.is_ascii_digit() || c == '_' || c == '(')
    {
        return false;
    }
    let word: String = rest
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase();
    word.len() == 1 || !WORDS.iter().any(|w| word.starts_with(w.as_str()))
}

/// The index after the `)` closing the `(` at `open`, or the end while it's still being typed.
fn closing_paren(text: &[char], open: usize) -> usize {
    let mut depth = 0;
    for (i, &c) in text.iter().enumerate().skip(open) {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        }
    }
    text.len()
}

fn radicand_end(text: &[char], after: usize) -> Option<usize> {
    let mut j = after;
    while text.get(j) == Some(&' ') {
        j += 1;
    }
    if text.get(j) == Some(&'(') {
        return Some(closing_paren(text, j));
    }
    let rest = &text[j.min(text.len())..];
    if bare_letter(rest) {
        return Some(j + 1);
    }
    let rest: String = rest.iter().collect();
    let m = BARE_RADICAND.find(&rest).ok().flatten()?;
    let mut end = j + m.as_str().chars().count();
    // `√4%` is √0.04: the percent turns into a group before the root is read
    let after_text: String = text[end..].iter().collect();
    let pct = after_text.trim_start_matches(jsstr::is_space);
    if pct.starts_with('%') && m.as_str().ends_with(|c: char| c.is_ascii_digit()) {
        end += after_text.chars().count() - pct.chars().count() + 1;
    }
    Some(end)
}

/// Each √ or ∛ with the end of the radicand the engine reads under it; one it can't read gets no span.
pub fn radical_spans(text: &str) -> Vec<RadicalSpan> {
    let chars: Vec<char> = text.chars().collect();
    // UTF-16 offset of each char index (and of the end)
    let mut units = Vec::with_capacity(chars.len() + 1);
    let mut u = 0;
    for c in &chars {
        units.push(u);
        u += c.len_utf16();
    }
    units.push(u);
    let mut out = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != '√' && c != '∛' {
            continue;
        }
        if let Some(end) = radicand_end(&chars, i + 1)
            && end > i + 1
        {
            out.push(RadicalSpan {
                sign: units[i],
                end: units[end],
            });
        }
    }
    out
}

/// Display only: an exact answer's `sqrt(3)` reads `√3`, `sqrt(2 + sqrt(3))` reads `√(2 + √3)`.
pub fn radical_answer(s: &str) -> String {
    static HAS: Lazy = Lazy::new(|| jsre::js(r"sqrt\(|cbrt\(", ""));
    static ANSWER_MATH: Lazy =
        Lazy::new(|| jsre::js(&jsstr::spaces(r"^[\d\s+\-−±∓*/^().,πτei]*$"), ""));
    if !jsre::test(&HAS, s) {
        return s.to_string();
    }
    let opened = jsre::replace_all(&HAS, s, |_| "(".into());
    if !jsre::test(&ANSWER_MATH, &opened) {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        let name = if rest.starts_with("sqrt(") {
            "sqrt"
        } else if rest.starts_with("cbrt(") {
            "cbrt"
        } else {
            out.push(chars[i]);
            i += 1;
            continue;
        };
        let open = i + 4;
        let close = closing_paren(&chars, open);
        let inner_text: String = chars
            [(open + 1).min(close.saturating_sub(1))..close.saturating_sub(1)]
            .iter()
            .collect();
        let inner = radical_answer(&inner_text);
        let sign = if name == "sqrt" { '√' } else { '∛' };
        // dropping the parens must not pull what follows under the root: `sqrt(2)pi` stays `√(2)π`
        let plain_number = {
            let (int, frac) = inner.split_once('.').unwrap_or((&inner, ""));
            !int.is_empty()
                && int.bytes().all(|b| b.is_ascii_digit())
                && (!inner.contains('.')
                    || (!frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit())))
        };
        let follows = chars
            .get(close)
            .is_some_and(|&c| c.is_ascii_alphanumeric() || ".^*×·⋅∙πτ(%".contains(c));
        if plain_number && !follows {
            out.push(sign);
            out += &inner;
        } else {
            out += &format!("{sign}({inner})");
        }
        i = close;
    }
    out
}

/// A one-string answer (`x = a ≈ b`, an example or a held one) in pieces, so each side reads its roots like the
/// live answer: `s.split(/(^\S+ = | ≈ )/).filter(Boolean)`.
pub fn answer_parts(s: &str) -> Vec<String> {
    static SEP: Lazy = Lazy::new(|| jsre::js(&jsstr::spaces(r"(^\S+ = | ≈ )"), ""));
    let mut out = Vec::new();
    let mut last = 0;
    for m in SEP.find_iter(s).flatten() {
        out.push(s[last..m.start()].to_string());
        out.push(m.as_str().to_string());
        last = m.end();
    }
    out.push(s[last..].to_string());
    out.retain(|p| !p.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_and_answers() {
        assert_eq!(radical_spans("√2x"), vec![RadicalSpan { sign: 0, end: 2 }]);
        assert_eq!(radical_spans("√(2"), vec![RadicalSpan { sign: 0, end: 3 }]);
        assert_eq!(radical_answer("-sqrt(2)/2"), "-√2/2");
        assert_eq!(radical_answer("2sqrt(3)"), "2√3");
        assert_eq!(
            answer_parts("x = ±sqrt(2) ≈ ±1.4"),
            vec!["x = ", "±sqrt(2)", " ≈ ", "±1.4"]
        );
    }
}
