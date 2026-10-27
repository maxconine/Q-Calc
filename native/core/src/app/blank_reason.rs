//! Why a line has no answer (`src/lib/blankReason.ts`): where the squiggle goes (an unknown word, a unit the
//! conversion can't reach, a stray operator) and the one-line hint ↵ shows when it saved nothing.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::app::js::{self as jstr, index16, len16, slice16};
use crate::jsre::{self, Lazy, js};
use crate::scientific::NAMES as SCIENTIFIC_NAMES;

/// A range of the input in UTF-16 code units (JS string indices, `NSRange` on the Mac), `end` exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// The user's names, which never count as unknown words.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Names {
    #[serde(default)]
    pub variables: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
}

const OPERATORS: &str = "+-−*×·/÷^=,.";
const CAN_LEAD: &str = "+-−.";

fn is_operator(c: char) -> bool {
    OPERATORS.contains(c)
}

fn can_lead(c: char) -> bool {
    CAN_LEAD.contains(c)
}

fn known() -> &'static BTreeSet<String> {
    static KNOWN: std::sync::LazyLock<BTreeSet<String>> = std::sync::LazyLock::new(|| {
        format!("{SCIENTIFIC_NAMES}|e|i|in|to|as|of|mod|and|or|not|xor|graph|sys")
            .to_lowercase()
            .split('|')
            .map(str::to_string)
            .collect()
    });
    &KNOWN
}

/// Where a blank answer stops reading: an unknown word, a unit the conversion can't reach, or a stray operator.
/// `ok` says whether a piece of text evaluates. None when there's no confident answer; a single operator at the end
/// is unfinished, not wrong, so it's never flagged.
pub fn blank_reason(expr: &str, ok: &dyn Fn(&str) -> bool, names: &Names) -> Option<Span> {
    if jstr::trim(expr).is_empty() || ok(expr) {
        return None;
    }
    // `2x+3==11` would solve without its second `=`, so that's what's wrong, not the x
    if expr.contains('=') {
        return stray_operator(expr, ok)
            .or_else(|| unknown_word(expr, ok, names))
            .or_else(|| unreachable_unit(expr, ok));
    }
    unknown_word(expr, ok, names)
        .or_else(|| unreachable_unit(expr, ok))
        .or_else(|| stray_operator(expr, ok))
}

fn unknown_word(expr: &str, ok: &dyn Fn(&str) -> bool, names: &Names) -> Option<Span> {
    static WORD: Lazy = Lazy::new(|| js(r"[A-Za-z]+", ""));
    let user: BTreeSet<String> = names
        .variables
        .iter()
        .chain(&names.functions)
        .map(|n| n.to_lowercase())
        .collect();
    let words: Vec<(&str, usize)> = WORD
        .find_iter(expr)
        .filter_map(Result::ok)
        .map(|m| (m.as_str(), index16(expr, m.start())))
        .collect();
    for (i, &(word, start)) in words.iter().enumerate() {
        let lower = word.to_lowercase();
        if known().contains(&lower) || user.contains(&lower) {
            continue;
        }
        let prev = i.checked_sub(1).map(|p| words[p].0);
        let next = words.get(i + 1).map(|w| w.0);
        // units can take two words (sq ft, fl oz, nautical mile)
        if ok(&format!("1 {word}"))
            || next.is_some_and(|n| ok(&format!("1 {word} {n}")))
            || prev.is_some_and(|p| ok(&format!("1 {p} {word}")))
        {
            continue;
        }
        return Some(Span {
            start,
            end: start + word.len(),
        });
    }
    None
}

fn unreachable_unit(expr: &str, ok: &dyn Fn(&str) -> bool) -> Option<Span> {
    static CONVERT: Lazy = Lazy::new(|| js(r"^(.*\S)\s+(?:in|to|as)\s+(\S.*?)\s*$", "i"));
    let c = CONVERT.captures(expr).ok()??;
    let left = jsre::group(&c, 1);
    let target = jsre::group(&c, 2);
    if !ok(left) || !ok(&format!("1 {target}")) {
        return None;
    }
    let start = index16(expr, expr.rfind(target)?);
    Some(Span {
        start,
        end: start + len16(target),
    })
}

/// The characters [`stray_operator`] may evaluate in all, counted like `work.ts` budgets: a typed line spends a
/// few thousand. The TS reader has no limit, and took 6 s on `e^/^/…` 1,000 characters long.
const STRAY_BUDGET: usize = 40_000;

fn stray_operator(expr: &str, ok: &dyn Fn(&str) -> bool) -> Option<Span> {
    let units: Vec<u16> = expr.encode_utf16().collect();
    let unit_char = |i: usize| units.get(i).and_then(|&u| char::from_u32(u32::from(u)));
    let last = len16(jstr::trim_end(expr)) as isize - 1;
    // the first unit that isn't a space
    let first_text = units
        .iter()
        .position(|&u| !char::from_u32(u32::from(u)).is_some_and(jstr::is_js_space))
        .unwrap_or(units.len());
    let mut i = 0usize;
    let mut budget = STRAY_BUDGET;
    while (i as isize) <= last {
        let Some(ch) = unit_char(i).filter(|&c| is_operator(c)) else {
            i += 1;
            continue;
        };
        // read off the units rather than slicing the text before and after each operator, which made a long run
        // of operators quadratic (`-` 20,000 times took 0.8 s live): `before` trims to nothing when every unit
        // before is a space, and `after`'s first character is the first unit past the spaces (a surrogate there
        // is never an operator)
        let space = |u: u16| char::from_u32(u32::from(u)).is_some_and(jstr::is_js_space);
        let leading = i <= first_text && !can_lead(ch);
        let first_after = units[i + 1..]
            .iter()
            .find(|&&u| !space(u))
            .and_then(|&u| char::from_u32(u32::from(u)));
        let doubled = first_after.is_some_and(|c| is_operator(c) && !can_lead(c));
        let trailing = i as isize == last && (ch == '=' || ch == ',');
        if !leading && !doubled && !trailing {
            i += 1;
            continue;
        }
        // the doubled one is the second of the pair
        let at = match first_after {
            Some(c) if doubled => units
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|&(_, &u)| u32::from(u) == c as u32)
                .map_or(i, |(j, _)| j),
            _ => i,
        };
        let without = format!(
            "{}{}",
            slice16(expr, 0, at),
            slice16(expr, at + 1, usize::MAX)
        );
        // each try evaluates the whole line again, so a long run of operators (`e^/^/^/…`) cost the square of
        // its length: past the budget there's no confident answer
        budget = budget.checked_sub(without.len())?;
        if ok(&without) {
            return Some(Span {
                start: at,
                end: at + 1,
            });
        }
        i += 1;
    }
    None
}

/// What ↵ that saved nothing knows about the line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterFacts {
    pub expr: String,
    /// What `blank_reason` found, already worked out for the squiggle.
    #[serde(default)]
    pub span: Option<Span>,
    /// The live answer said the units don't convert.
    #[serde(default)]
    pub improper: Option<bool>,
    #[serde(default)]
    pub natural_language: Option<bool>,
    /// A `graph` with nothing in scope to draw.
    #[serde(default)]
    pub bare_graph: Option<bool>,
    /// There are saved rows ↑ can bring back.
    #[serde(default)]
    pub history: Option<bool>,
}

/// One line for ↵ that saved nothing: what stopped the answer and where to look, never the answer.
pub fn enter_hint(f: &EnterFacts) -> String {
    static TRAILING_OPERATOR: Lazy = Lazy::new(|| js(r"([+\-−*×·/÷^])\s*$", ""));
    static CONVERT_BEFORE: Lazy = Lazy::new(|| js(r"\b(?:to|in|as)\s+$", "i"));
    static E_OR_I: Lazy = Lazy::new(|| js(r"\b(?:e|i)\b", ""));
    static UNKNOWNS: Lazy = Lazy::new(|| js(r"(?<![A-Za-z])[A-Za-z](?![A-Za-z(])", ""));
    let flag = |b: Option<bool>| b == Some(true);
    let expr = jstr::trim(&f.expr);
    if expr.is_empty() {
        return if flag(f.history) {
            "nothing to save · ↑ brings back earlier ones"
        } else {
            "type a calculation first, like 12 * 7"
        }
        .into();
    }
    if flag(f.bare_graph) {
        return "graph needs something to plot, like graph x^2".into();
    }
    if flag(f.improper) {
        return "those units measure different things".into();
    }
    let opens = expr.matches('(').count();
    let closes = expr.matches(')').count();
    if closes > opens {
        return "a ) has no ( to match".into();
    }
    let trailing = TRAILING_OPERATOR
        .captures(expr)
        .ok()
        .flatten()
        .map(|c| jsre::group(&c, 1).to_string());
    if let (Some(op), None) = (&trailing, &f.span) {
        return format!("unfinished: something goes after the {op}");
    }
    if let Some(span) = f.span {
        let word = slice16(&f.expr, span.start, span.end);
        let mut chars = word.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && is_operator(c)
            && len16(&word) == 1
        {
            return if span.end >= len16(jstr::trim_end(&f.expr)) {
                format!("unfinished: something goes after the {word}")
            } else {
                format!("check the extra {word}")
            };
        }
        if jsre::test(&CONVERT_BEFORE, &slice16(&f.expr, 0, span.start)) {
            return format!("can’t convert that to {word}");
        }
        if len16(&word) == 1 {
            let stripped = jsre::replace_all(&E_OR_I, expr, |_| String::new());
            let unknowns: BTreeSet<String> = UNKNOWNS
                .find_iter(&*stripped)
                .filter_map(Result::ok)
                .map(|m| m.as_str().to_lowercase())
                .collect();
            if expr.contains('=') && unknowns.len() > 1 {
                let k = unknowns.len();
                return format!("{k} unknowns: sys {k} solves equations together");
            }
            return format!("{word} has no value yet · {word} = 5 sets it");
        }
        return format!("“{word}” isn’t a unit, function or name I know");
    }
    if flag(f.natural_language) {
        return "couldn’t read that as math · ? shows what works".into();
    }
    "no answer for that · ? shows what you can type".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_stray_operators_and_words() {
        let ok = |t: &str| matches!(t, "2+3" | "2+2=" | "1 kg" | "12 kg" | "1 lb");
        assert_eq!(
            blank_reason("2+*3", &|t| t == "2+3", &Names::default()),
            Some(Span { start: 2, end: 3 })
        );
        assert_eq!(
            blank_reason("12 kg in lbz", &ok, &Names::default()),
            Some(Span { start: 9, end: 12 })
        );
        assert_eq!(blank_reason("2+", &ok, &Names::default()), None);
    }
}
