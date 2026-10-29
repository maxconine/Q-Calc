//! `src/lib/unitSpans.ts`: the words in a typed line that read as units, with what each one is.

use crate::jsre::{self, Lazy};
use crate::units::unit_info;

/// A unit word's place (UTF-16 indices, as the TS reports them) and its spelled-out name.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UnitSpan {
    pub start: usize,
    pub end: usize,
    pub name: String,
}

// words that are never a unit here, whatever the table says: constants and the conversion words themselves
const NOT_UNITS: [&str; 12] = [
    "e", "i", "pi", "tau", "ans", "to", "into", "as", "per", "of", "and", "or",
];
const CONVERSION_WORDS: [&str; 4] = ["to", "in", "into", "as"];
const TEMPERATURES: [&str; 4] = ["celsius", "fahrenheit", "kelvin", "rankine"];

/// A span in byte offsets while working; converted to UTF-16 at the end.
struct Found {
    start: usize,
    end: usize,
    name: String,
}

/// The `m` in `5 m`, both halves of `m/s`, the `mi` in `10 km to mi`. A word only counts where a unit goes
/// (after a number, in a unit's product or quotient, after `to`), so a variable `m`, the function `min(…)` or
/// the `in` of `10 km in mi` stays a word.
pub fn unit_spans(text: &str, variables: &[String], functions: &[String]) -> Vec<UnitSpan> {
    // a word may sit right against its number (`5m`), but not inside another word
    static WORD: Lazy =
        Lazy::new(|| jsre::js(r"(?<![A-Za-z_µμΩÅ°])°?[A-Za-zµμΩÅ][A-Za-zµμΩÅ]*", ""));
    static CALL: Lazy = Lazy::new(|| jsre::js(r"^\s*\(", ""));
    let mut spans: Vec<Found> = Vec::new();
    for m in WORD.find_iter(text).filter_map(Result::ok) {
        let word = m.as_str();
        let (start, end) = (m.start(), m.end());
        if NOT_UNITS.contains(&word.to_lowercase().as_str())
            || variables.iter().chain(functions).any(|t| t == word)
        {
            continue;
        }
        // a call, not a unit
        if jsre::test(&CALL, &text[end..]) {
            continue;
        }
        if !unit_place(text, start, &spans) {
            continue;
        }
        // `in` before another unit is the conversion (`5 in mm`, the second of `12122 in in mm`); before a
        // conversion word, an operator or nothing it's inches (`5 in to mm`, `5 in + 2`)
        static IN_CONVERTS: Lazy =
            Lazy::new(|| jsre::js(r"^\s+(?!(?:to|into|in|as)\b)[A-Za-zµμΩÅ°]", "i"));
        if word.eq_ignore_ascii_case("in") && jsre::test(&IN_CONVERTS, &text[end..]) {
            continue;
        }
        if let Some(info) = unit_info(word) {
            spans.push(Found {
                start,
                end,
                name: info.name,
            });
        }
    }
    let spans = temperatures(text, spans);
    let u16 = |b: usize| text[..b].encode_utf16().count();
    spans
        .into_iter()
        .map(|f| UnitSpan {
            start: u16(f.start),
            end: u16(f.end),
            name: f.name,
        })
        .collect()
}

// `20 C to F` is celsius to fahrenheit, as the calculator reads it, not coulombs to farads
fn temperatures(text: &str, spans: Vec<Found>) -> Vec<Found> {
    let word = |u: &Found| &text[u.start..u.end];
    let mut letters: Vec<&str> = spans
        .iter()
        .map(word)
        .filter(|w| *w == "C" || *w == "F")
        .collect();
    letters.sort_unstable();
    letters.dedup();
    let hot = spans
        .iter()
        .any(|u| TEMPERATURES.contains(&u.name.as_str()))
        || letters.len() == 2;
    if !hot {
        return spans;
    }
    spans
        .into_iter()
        .map(|u| {
            let name = match word(&u) {
                "C" => "celsius".to_string(),
                "F" => "fahrenheit".to_string(),
                _ => u.name.clone(),
            };
            Found { name, ..u }
        })
        .collect()
}

// whether what comes before `start` is where a unit goes
fn unit_place(text: &str, start: usize, found: &[Found]) -> bool {
    static NUMBERISH: Lazy = Lazy::new(|| jsre::js(r"[\d)²³⁰-⁹.]", ""));
    static OPERAND_END: Lazy = Lazy::new(|| jsre::js(r"[\d²³⁰-⁹)]$", ""));
    static POWER: Lazy = Lazy::new(|| jsre::js(r"^\^-?\d+$", ""));
    static PRIOR: Lazy = Lazy::new(|| jsre::js(r"([A-Za-z]+)$", ""));
    let before = text[..start].trim_end();
    let Some(last) = before.chars().last() else {
        return false;
    };
    let mut buf = [0u8; 4];
    // `5 m`, `(2 + 3) kg`, `3² m`
    if jsre::test(&NUMBERISH, last.encode_utf8(&mut buf)) {
        return true;
    }
    // `m/s`, `kg·m/s²`, `50/hr`: an operator right after a number or another unit
    if matches!(last, '/' | '*' | '·') {
        let operand = before[..before.len() - last.len_utf8()].trim_end();
        if jsre::test(&OPERAND_END, operand) {
            return true;
        }
        let len = operand.len();
        return found
            .iter()
            .any(|u| u.end == len || (u.end < len && jsre::test(&POWER, &text[u.end..len])));
    }
    // `10 km to mi`, and `ans in to mm`: ans is a number
    PRIOR.captures(before).ok().flatten().is_some_and(|c| {
        let prior = jsre::group(&c, 1).to_lowercase();
        CONVERSION_WORDS.contains(&prior.as_str()) || prior == "ans"
    })
}
