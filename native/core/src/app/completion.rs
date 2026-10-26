//! The faint rest of a word the bar offers to finish (`src/lib/completion.ts`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::app::identities::IDENTITY_PARTS;
use crate::app::js::{len16, skip16};
use crate::app::needs::{GAS_CONSTANT_UNITS, is_unit_name};
use crate::jsre::{self, Lazy, js};
use crate::scientific::NAMES as SCIENTIFIC_NAMES;

/// Ranked: the first name a prefix reaches is the one offered.
pub const COMPLETION_FUNCTIONS: &[&str] = &[
    "sqrt",
    "sin",
    "cos",
    "tan",
    "log",
    "abs",
    "exp",
    "floor",
    "ceil",
    "round",
    "factorial",
    "asin",
    "acos",
    "atan",
    "sinh",
    "cosh",
    "tanh",
    "arcsin",
    "arccos",
    "arctan",
    "gcd",
    "gcf",
    "lcf",
    "lcm",
    "mean",
    "median",
    "sum",
    "max",
    "min",
    "nthRoot",
    "hypot",
    "combinations",
    "permutations",
];

pub const COMPLETION_UNITS: &[&str] = &[
    "kilogram",
    "kilometer",
    "meter",
    "centimeter",
    "millimeter",
    "mile",
    "minute",
    "milliliter",
    "milligram",
    "gram",
    "pound",
    "ounce",
    "inch",
    "feet",
    "foot",
    "yard",
    "liter",
    "gallon",
    "quart",
    "pint",
    "cup",
    "tablespoon",
    "teaspoon",
    "second",
    "hour",
    "day",
    "week",
    "month",
    "year",
    "celsius",
    "fahrenheit",
    "kelvin",
    "joule",
    "calorie",
    "kilocalorie",
    "watt",
    "kilowatt",
    "newton",
    "pascal",
    "atmosphere",
    "degree",
    "radian",
    "kilobyte",
    "megabyte",
    "gigabyte",
    "terabyte",
    "acre",
    "hectare",
    "hertz",
    "knot",
    "horsepower",
    "volt",
    "ampere",
];

// these become √ and Σ, which read their argument without parens
const NO_PAREN: [&str; 2] = ["sqrt", "sum"];

// a prefix on its own (`kilo`) still reads as the start of a longer unit
const PREFIX_WORDS: [&str; 8] = [
    "kilo", "mega", "giga", "tera", "milli", "micro", "nano", "centi",
];

/// The user's names: variables and functions finish first, contacts finish `message <name>`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionNames {
    #[serde(default)]
    pub variables: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
    #[serde(default)]
    pub ans: bool,
    #[serde(default)]
    pub contacts: Vec<String>,
}

fn names() -> &'static BTreeSet<String> {
    static NAMES: std::sync::LazyLock<BTreeSet<String>> = std::sync::LazyLock::new(|| {
        format!("{SCIENTIFIC_NAMES}|e|in|to|as|of|per|and|or|mod")
            .to_lowercase()
            .split('|')
            .map(str::to_string)
            .collect()
    });
    &NAMES
}

fn is_whole_word(word: &str, user: &[&String]) -> bool {
    let lower = word.to_lowercase();
    if names().contains(&lower) || user.iter().any(|n| n.to_lowercase() == lower) {
        return true;
    }
    !PREFIX_WORDS.contains(&lower.as_str()) && is_unit_name(word)
}

/// The rest of `prefix` among `list`, longer than it (`find` order).
fn finish<'a>(
    list: impl IntoIterator<Item = &'a str>,
    prefix: &str,
    lower: bool,
) -> Option<&'a str> {
    let p = if lower {
        prefix.to_lowercase()
    } else {
        prefix.to_string()
    };
    list.into_iter().find(|n| {
        len16(n) > len16(prefix)
            && if lower {
                n.to_lowercase().starts_with(&p)
            } else {
                n.starts_with(&p)
            }
    })
}

/// The rest of the word being typed at the end of `text`, or "" when nothing is a clear pick. After a number or
/// `in`/`to` a unit is likelier than a function; elsewhere functions come first.
pub fn completion_for(text: &str, names: &CompletionNames) -> String {
    static TYPED: Lazy = Lazy::new(|| js(r"^\s*([A-Za-z]{3,})$", ""));
    static MESSAGE_TO: Lazy = Lazy::new(|| js(r"^\s*message\s+([a-z0-9_-]+)$", "i"));
    static MESSAGE: Lazy = Lazy::new(|| js(r"^\s*message\s", "i"));
    static PERIODIC: Lazy = Lazy::new(|| js(r"^\s*periodic\s+([A-Za-z]*)$", "i"));
    static GAS: Lazy = Lazy::new(|| js(r"^\s*chem\s+r\s+([A-Za-z]+)$", "i"));
    static PART: Lazy = Lazy::new(|| {
        js(
            r"^\s*(?:identities|identity|idt)\s+([A-Za-z][A-Za-z ]*)$",
            "i",
        )
    });
    static WORD: Lazy = Lazy::new(|| js(r"(^|[^A-Za-z])([A-Za-z]{2,})$", ""));
    static UNIT_SPOT: Lazy = Lazy::new(|| js(r"(?:\d|\)|\b(?:in|to))\s*$", "i"));
    let cap = |re: &jsre::Regex| {
        re.captures(text)
            .ok()
            .flatten()
            .map(|c| jsre::group(&c, 1).to_string())
    };
    // `mes` finishes `message`, its name finishes from the accepted ones, and what's said is never completed
    if let Some(typed) = cap(&TYPED)
        && "message".starts_with(&typed.to_lowercase())
    {
        return skip16("message", len16(&typed));
    }
    if let Some(to) = cap(&MESSAGE_TO) {
        let lower = to.to_lowercase();
        return names
            .contacts
            .iter()
            .find(|n| len16(n) > len16(&to) && n.starts_with(&lower))
            .map_or_else(String::new, |n| skip16(n, len16(&to)));
    }
    if jsre::test(&MESSAGE, text) {
        return String::new();
    }
    // `periodic table` is a command, not a unit: `periodic ta` finishes it, and `table` never becomes `tablespoon`
    if let Some(p) = cap(&PERIODIC) {
        return if "table".starts_with(&p.to_lowercase()) {
            skip16("table", len16(&p))
        } else {
            String::new()
        };
    }
    // `chem R a` finishes `atm`: the units the gas constant comes in, not the unit list
    if let Some(g) = cap(&GAS) {
        return finish(GAS_CONSTANT_UNITS, &g.to_lowercase(), false)
            .map_or_else(String::new, |u| skip16(u, len16(&g)));
    }
    // `idt in` finishes the part of the sheet to open; one that's no part goes on to the usual picks
    if let Some(part) = cap(&PART).map(|p| p.to_lowercase())
        && let Some(hit) = finish(IDENTITY_PARTS, &part, false)
    {
        return skip16(hit, len16(&part));
    }
    let Some(word) = WORD
        .captures(text)
        .ok()
        .flatten()
        .map(|c| jsre::group(&c, 2).to_string())
    else {
        return String::new();
    };
    let before = &text[..text.len() - word.len()];
    let user: Vec<&String> = names.variables.iter().chain(&names.functions).collect();
    if is_whole_word(&word, &user) {
        return String::new();
    }
    let unit_spot = jsre::test(&UNIT_SPOT, before);
    let pick = |list: &mut dyn Iterator<Item = &str>, tail: &str| -> String {
        match finish(list, &word, true) {
            Some(hit) => format!(
                "{}{}",
                skip16(hit, len16(&word)),
                if NO_PAREN.contains(&hit) { "" } else { tail }
            ),
            None => String::new(),
        }
    };
    let or = |a: String, b: &dyn Fn() -> String| if a.is_empty() { b() } else { a };
    let function = || {
        or(
            pick(&mut names.functions.iter().map(String::as_str), "("),
            &|| pick(&mut COMPLETION_FUNCTIONS.iter().copied(), "("),
        )
    };
    let unit = || pick(&mut COMPLETION_UNITS.iter().copied(), "");
    let var = pick(&mut names.variables.iter().map(String::as_str), "");
    if !var.is_empty() {
        return var;
    }
    if unit_spot {
        or(unit(), &function)
    } else {
        or(function(), &unit)
    }
}
