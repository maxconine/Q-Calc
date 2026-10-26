//! `src/engine/letters.ts`: `xy` reads as `x y` when every letter is a defined variable and `xy` means nothing
//! on its own.

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::jsre::{self, Lazy, js};
use crate::scientific::NAMES;

// the words solve, sums, graph and calculus read, which `for y` or `at 3` would lose if split
const KEYWORDS: &str = "at|for|from|over|solve|isolate|prod|product|lim|limit|int|integral|integrate|derivative|the|second|approaches|graph|theta|inf|infinity";

static TAKEN: LazyLock<HashSet<String>> = LazyLock::new(|| {
    format!("{NAMES}|e|ans|in|to|as|of|per|and|or|not|xor|mod|{KEYWORDS}")
        .to_lowercase()
        .split('|')
        .map(str::to_string)
        .collect()
});

/// `xy` as `x y` when every letter is defined. A word before `(` stays whole, since that's a function call.
/// `params` are a function's own names, which win over a unit they spell: f(m,s)=ms is m times s.
pub fn split_letters(expr: &str, known: &dyn Fn(&str) -> bool, params: &[String]) -> String {
    static LHS: Lazy = Lazy::new(|| js(r"^\s*[A-Za-z][A-Za-z0-9]*\s*=(?!=)", ""));
    static WORD: Lazy =
        Lazy::new(|| js(r"(?<![A-Za-z_\\.])[A-Za-z]{2,}(?![A-Za-z0-9_.]|\s*\()", ""));
    static DIFFERENTIAL: Lazy = Lazy::new(|| js(r"^d[A-Za-z]$", ""));
    // `xy = 5` names xy; only what follows the = is split
    let lhs = LHS.find(expr).ok().flatten().map_or("", |m| m.as_str());
    // in calculus `dx` is the differential, never d times x
    let calculus = crate::stubs::is_calculus_input(expr);
    let param = |c: &str| params.iter().any(|p| p == c);
    let rest = jsre::replace_all(&WORD, &expr[lhs.len()..], |caps| {
        let word = jsre::group(caps, 0);
        let letters: Vec<String> = word.chars().map(String::from).collect();
        let own = !params.is_empty()
            && !param(word)
            && letters.iter().all(|c| param(c) || known(c))
            && letters.iter().any(|c| param(c));
        if own {
            return letters.join(" ");
        }
        if known(word) || TAKEN.contains(&word.to_lowercase()) || crate::units::reads_as_unit(word)
        {
            return word.to_string();
        }
        if calculus && jsre::test(&DIFFERENTIAL, word) {
            return word.to_string();
        }
        // spaces, so `2xy^2` and `1/xy` read exactly as `2x y^2` and `1/x y` would
        if letters.iter().all(|c| known(c)) {
            letters.join(" ")
        } else {
            word.to_string()
        }
    });
    format!("{lhs}{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_defined_letters() {
        let known = |n: &str| n == "x" || n == "y";
        assert_eq!(split_letters("2xy^2", &known, &[]), "2x y^2");
        assert_eq!(split_letters("xy = 5", &known, &[]), "xy = 5");
        assert_eq!(split_letters("xz", &known, &[]), "xz");
        assert_eq!(split_letters("sin(xy)", &known, &[]), "sin(x y)");
        let none = |_: &str| false;
        assert_eq!(split_letters("ms", &none, &["m".into(), "s".into()]), "m s");
    }
}
