//! The input's typing rules (Symbols.swift, from QuickInput.tsx): typed words become symbols (`prettyTokens`:
//! WORD_SYMBOLS, AT_ONCE, LONGER_WORDS, SHORTCUT_SYMBOLS), `[` pairing (`bracketKey`), an opening bracket typed over
//! a selection wraps it (`wrapSelection`), and a paste flattened to one line (`flattenPastedText`).
//!
//! Pure and cheap (a dozen precompiled regexes over one short line), so they run on the UI thread as the key is
//! typed, the way the Mac's `textDidChange` runs them. Offsets here are byte offsets on char boundaries; the field
//! converts at its edges.

use qcalc_core::jsre::{self, Lazy, Regex};

struct Rule {
    re: &'static Lazy,
    put: &'static str,
    /// the lim arrow keeps its lead (group 1)
    keep_lead: bool,
}

macro_rules! re {
    ($name:ident, $p:expr) => {
        static $name: Lazy = Lazy::new(|| jsre::compile($p));
    };
}

// (pattern, symbol); the patterns are Symbols.swift's, case-insensitive where it says so
re!(SQRT, r"(?i)(?<![\\A-Za-z])sqrt(?![A-Za-z])");
re!(PI, r"(?i)(?<![\\A-Za-z])pi(?![A-Za-z])");
re!(THETA, r"(?i)(?<![\\A-Za-z])theta(?![A-Za-z0-9])");
re!(INFTY, r"(?i)(?<![\\A-Za-z])infty(?![A-Za-z0-9])");
re!(INF, r"(?i)(?<![\\A-Za-z])inf(?![A-Za-z0-9])");
re!(CBRT, r"(?i)(?<![\\A-Za-z])cbrt(?![A-Za-z])");
re!(LIM, r"((?<![A-Za-z0-9_])lim(?:it)?\s*_?[({]?\s*[A-Za-z]\s*)->");
re!(PLUS_MINUS, r"(?i)(?<![\\A-Za-z])plus[\s.-]?minus(?![A-Za-z])");
re!(MINUS_PLUS, r"(?i)(?<![\\A-Za-z])minus[\s.-]?plus(?![A-Za-z])");
re!(DOT, r"(?i)(?<![\\A-Za-z])dot(?![A-Za-z])");
re!(SUM, r"(?<![\\A-Za-z_])sum(?![A-Za-z0-9])");
re!(PROD, r"(?<![\\A-Za-z_])prod(?![A-Za-z0-9])");
re!(INT, r"(?<![\\A-Za-z_])int(?![A-Za-z]|_[A-Za-z])");
re!(MP_SHORT, r"-[/.]\+");
re!(PM_SHORT, r"\+[/.]?-");
re!(TILDE, r"~");
re!(MESSAGE, r"(?i)^\s*message(?:\s|$)");

const fn rule(re: &'static Lazy, put: &'static str) -> Rule {
    Rule {
        re,
        put,
        keep_lead: false,
    }
}

static WORD_SYMBOLS: [Rule; 13] = [
    rule(&SQRT, "√"),
    rule(&PI, "π"),
    rule(&THETA, "θ"),
    rule(&INFTY, "∞"),
    rule(&INF, "∞"),
    rule(&CBRT, "∛"),
    Rule {
        re: &LIM,
        put: "→",
        keep_lead: true,
    },
    rule(&PLUS_MINUS, "±"),
    rule(&MINUS_PLUS, "∓"),
    rule(&DOT, "·"),
    rule(&SUM, "Σ"),
    rule(&PROD, "Π"),
    rule(&INT, "∫"),
];

static SHORTCUT_SYMBOLS: [Rule; 3] = [
    rule(&MP_SHORT, "∓"),
    rule(&PM_SHORT, "±"),
    rule(&TILDE, "±"),
];

/// symbols that convert the moment their word is typed, even at the end of the line
const AT_ONCE: [&str; 9] = ["√", "∛", "±", "∓", "∫", "π", "Σ", "Π", "∞"];

/// a symbol whose word turned out to begin a longer one (`pint`, `integral`) goes back to the word
const LONGER_WORDS: [(&str, &str, &[&str]); 5] = [
    ("π", "pi", &["pint", "pica", "pico", "pipe", "pixel"]),
    (
        "∫",
        "int",
        &["integral", "integrate", "integer", "interest", "into"],
    ),
    ("∞", "inf", &["infinity", "info", "infty"]),
    ("Σ", "sum", &["sums", "summary", "summation"]),
    ("Π", "prod", &["product"]),
];

fn longer_re(i: usize) -> &'static Regex {
    static RES: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
        LONGER_WORDS
            .iter()
            .map(|(sign, _, _)| jsre::compile(&format!("{sign}_?([A-Za-z]+)")))
            .collect()
    });
    &RES[i]
}

fn restore_longer_words(text: &str) -> String {
    let mut out = text.to_string();
    for (i, (_, word, longer)) in LONGER_WORDS.iter().enumerate() {
        if !out.contains(LONGER_WORDS[i].0) {
            continue;
        }
        out = jsre::replace_all(longer_re(i), &out, |c| {
            let rest = jsre::group(c, 1);
            let w = format!("{word}{rest}").to_lowercase();
            let hit = longer.iter().any(|l| {
                *l == w || (rest.chars().count() >= 2 && (l.starts_with(&w) || w.starts_with(l)))
            });
            if hit {
                format!("{word}{rest}")
            } else {
                jsre::group(c, 0).to_string()
            }
        })
        .into_owned();
    }
    out
}

fn replace_tokens(text: &str, keep_trailing: bool, keep_words: bool) -> String {
    let mut out = if keep_words {
        text.to_string()
    } else {
        restore_longer_words(text)
    };
    let rules: &mut dyn Iterator<Item = &Rule> = if keep_words {
        &mut SHORTCUT_SYMBOLS.iter()
    } else {
        &mut WORD_SYMBOLS.iter().chain(SHORTCUT_SYMBOLS.iter())
    };
    for r in rules {
        let len = out.len();
        let next = jsre::replace_all(r.re, &out, |c| {
            let m = c.get(0).expect("a match");
            let matched = m.as_str();
            let at_end = m.end() == len;
            // a word at the end may still grow into a longer one unless its symbol converts at once
            if keep_trailing
                && !AT_ONCE.contains(&r.put)
                && at_end
                && matched.chars().last().is_some_and(|c| c.is_ascii_alphabetic())
            {
                matched.to_string()
            } else if r.keep_lead {
                format!("{}{}", jsre::group(c, 1), r.put)
            } else {
                r.put.to_string()
            }
        });
        if let std::borrow::Cow::Owned(s) = next {
            out = s;
        }
    }
    out
}

/// prettyTokens(text, caret): the text with symbols, and where the caret goes (bytes).
pub fn pretty(text: &str, caret: usize, keep_words: bool) -> (String, usize) {
    if jsre::test(&MESSAGE, text) {
        return (text.to_string(), caret);
    }
    let c = caret.min(text.len());
    let head = replace_tokens(&text[..c], true, keep_words);
    let tail = replace_tokens(&text[c..], false, keep_words);
    let at = head.len();
    (head + &tail, at)
}

fn brackets_even(s: &str) -> bool {
    s.matches('[').count() == s.matches(']').count()
}

/// bracketKey: `[` brings its `]`, a typed `]` steps over one, backspace in `[]` takes both. `key` is "[", "]" or
/// "backspace"; the caret (no selection) is a byte offset. The new text and caret, or None to leave the key alone.
pub fn bracket_key(text: &str, start: usize, end: usize, key: &str) -> Option<(String, usize)> {
    if start != end {
        return None;
    }
    let next = text[start..].chars().next();
    match key {
        "[" => {
            if next.is_some_and(|c| !" \t]),;".contains(c)) || !brackets_even(text) {
                return None;
            }
            Some((format!("{}[]{}", &text[..start], &text[start..]), start + 1))
        }
        "]" => (next == Some(']') && brackets_even(text)).then(|| (text.to_string(), start + 1)),
        "backspace" => {
            (start > 0 && text[..start].ends_with('[') && next == Some(']') && brackets_even(text))
                .then(|| {
                    (
                        format!("{}{}", &text[..start - 1], &text[start + 1..]),
                        start - 1,
                    )
                })
        }
        _ => None,
    }
}

/// wrapSelection: an opening bracket typed over a selection wraps it, and the selection stays on what it wrapped:
/// the new text and the selection (start, end).
pub fn wrap(text: &str, start: usize, end: usize, key: char) -> Option<(String, usize, usize)> {
    let close = match key {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    if end <= start {
        return None;
    }
    Some((
        format!(
            "{}{key}{}{close}{}",
            &text[..start],
            &text[start..end],
            &text[end..]
        ),
        start + 1,
        end + 1,
    ))
}

/// A `+-` that meets at the caret after a backspace becomes ± (joinPlusMinusAt): the new text, or None.
pub fn join_plus_minus(text: &str, caret: usize) -> Option<String> {
    (caret > 0 && text[caret - 1..].starts_with("+-"))
        .then(|| format!("{}±{}", &text[..caret - 1], &text[caret + 1..]))
}

/// flattenPastedText: look-alike characters cleaned (cleanPastedText), line breaks as spaces.
pub fn flatten_paste(text: &str) -> String {
    let clean = qcalc_core::app::paste::clean_pasted_text(text);
    clean
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace(['\n', '\t'], " ")
}

/// A paste of several lines (or `;`) that is 2-5 equations: they open as a system (pasteSystem).
pub fn pasted_system(raw: &str) -> Option<Vec<String>> {
    if !raw.contains(['\r', '\n', ';']) {
        return None;
    }
    let clean = qcalc_core::app::paste::clean_pasted_text(raw);
    let lines = qcalc_core::system::pasted_equations(&clean)?;
    (!lines.is_empty()).then(|| lines.into_iter().take(5).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> String {
        pretty(s, s.len(), false).0
    }

    #[test]
    fn words_become_symbols_and_the_caret_follows() {
        assert_eq!(p("sqrt"), "√");
        assert_eq!(p("2pi"), "2π");
        assert_eq!(p("theta+1"), "θ+1");
        // a word at the caret may still grow, unless its symbol converts at once
        assert_eq!(p("dot"), "dot");
        assert_eq!(p("dot "), "· ");
        assert_eq!(p("sum"), "Σ");
        assert_eq!(p("x +- 1"), "x ± 1");
        assert_eq!(p("lim x->"), "lim x→");
        let (t, c) = pretty("sqrt(2)|", "sqrt(".len(), false);
        assert_eq!(t, "√(2)|");
        assert_eq!(c, "√(".len());
    }

    #[test]
    fn longer_words_come_back() {
        assert_eq!(p("πnt"), "pint");
        assert_eq!(p("∫egral"), "integral");
        assert_eq!(p("Σmary"), "summary");
        assert_eq!(p("2π"), "2π");
    }

    #[test]
    fn keep_words_and_message_keep_the_text() {
        assert_eq!(pretty("sqrt pi", 7, true).0, "sqrt pi");
        assert_eq!(pretty("2 +- 1", 6, true).0, "2 ± 1");
        assert_eq!(p("message bob pi"), "message bob pi");
        assert_eq!(p("min(1,2)"), "min(1,2)");
        assert_eq!(p("sinh(1)"), "sinh(1)");
    }

    #[test]
    fn brackets_pair_step_and_delete() {
        assert_eq!(bracket_key("2", 1, 1, "["), Some(("2[]".into(), 2)));
        assert_eq!(bracket_key("2x", 1, 1, "["), None);
        assert_eq!(bracket_key("[1]", 2, 2, "]"), Some(("[1]".into(), 3)));
        assert_eq!(bracket_key("[]", 1, 1, "backspace"), Some(("".into(), 0)));
        assert_eq!(bracket_key("[1]", 2, 2, "backspace"), None);
    }

    #[test]
    fn an_opening_bracket_wraps_the_selection() {
        assert_eq!(wrap("2+3*4", 0, 3, '('), Some(("(2+3)*4".into(), 1, 4)));
        assert_eq!(wrap("x", 0, 0, '('), None);
        assert_eq!(wrap("x", 0, 1, '+'), None);
    }

    #[test]
    fn pastes_flatten_and_equations_open_a_system() {
        assert_eq!(flatten_paste("1\r\n+2\t*3"), "1 +2 *3");
        assert_eq!(join_plus_minus("2+-1", 2).as_deref(), Some("2±1"));
        let sys = pasted_system("x + y = 3\nx - y = 1").expect("a system");
        assert_eq!(sys.len(), 2);
        assert_eq!(pasted_system("2+2"), None);
    }
}
