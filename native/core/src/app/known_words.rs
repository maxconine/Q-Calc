//! Words the bar lights up once they're whole (`src/lib/knownWords.ts`): the engine's function names, the user's
//! functions, and `ans` once there's an answer.

use std::collections::BTreeSet;

use crate::app::blank_reason::Span;
use crate::app::js::index16;
use crate::jsre::{self, Lazy, js};
use crate::scientific::NAMES as SCIENTIFIC_NAMES;

fn names() -> &'static (BTreeSet<String>, Vec<String>) {
    static NAMES: std::sync::LazyLock<(BTreeSet<String>, Vec<String>)> =
        std::sync::LazyLock::new(|| {
            let set: BTreeSet<String> = SCIENTIFIC_NAMES
                .split('|')
                .filter(|n| !["pi", "tau", "inf", "infinity", "ans"].contains(n))
                .map(str::to_lowercase)
                .collect();
            // insertion order of the TS Set, then a stable sort by length: the first of equal-length names wins
            let mut seen = BTreeSet::new();
            let mut longest: Vec<String> = SCIENTIFIC_NAMES
                .split('|')
                .filter(|n| !["pi", "tau", "inf", "infinity", "ans"].contains(n))
                .map(str::to_lowercase)
                .filter(|n| seen.insert(n.clone()))
                .collect();
            longest.sort_by_key(|n| std::cmp::Reverse(n.len()));
            (set, longest)
        });
    &NAMES
}

/// The spans of the words the engine reads as a function (or `ans` with `ans` on). `logx` lights just the log, the
/// same way the engine splits it.
pub fn known_word_spans(text: &str, functions: &[String], ans: bool) -> Vec<Span> {
    static WORD: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_])[A-Za-z][A-Za-z0-9]*", ""));
    static VARIABLE: Lazy = Lazy::new(|| js(r"^(x|y|z|theta)$", ""));
    let (known, longest) = names();
    let mut extra: BTreeSet<String> = functions.iter().map(|f| f.to_lowercase()).collect();
    if ans {
        extra.insert("ans".into());
    }
    let mut spans = Vec::new();
    for m in WORD.find_iter(text).filter_map(Result::ok) {
        let word = m.as_str().to_lowercase();
        let start = index16(text, m.start());
        if known.contains(&word) || extra.contains(&word) {
            spans.push(Span {
                start,
                end: start + word.len(),
            });
            continue;
        }
        if let Some(f) = longest
            .iter()
            .find(|n| word.starts_with(n.as_str()) && jsre::test(&VARIABLE, &word[n.len()..]))
        {
            spans.push(Span {
                start,
                end: start + f.len(),
            });
        }
    }
    spans
}
