//! Regexes written the way the TS source writes them. The engine's text passes are regex rewrites with
//! look-behind, which the `regex` crate lacks, so ports use `fancy-regex` through these helpers.
//!
//! Porting notes (JS → fancy-regex): JS `\d`, `\w` and `\b` are ASCII where Rust's are Unicode; `js()` rewrites
//! them so TS patterns paste unchanged. `$1` in replacements becomes `${1}` (or use a closure), the `g` flag is
//! `replace_all`, and look-behind may be variable-length in both. JS indexes UTF-16 units, Rust bytes: port
//! index arithmetic over `chars()`, never over byte offsets from one string applied to another.

use std::borrow::Cow;

pub use fancy_regex::Regex;

/// The groups of one match in a `str`.
pub type Caps<'t> = fancy_regex::Captures<'t, str>;

/// A regex compiled once, on first use: `static RE: Lazy = jsre::lazy(r"...")`.
pub type Lazy = std::sync::LazyLock<Regex>;

/// Compiles a pattern from the TS source. Panics on a bad pattern, which only a code change can cause.
///
/// No fully compiled DFAs: the lazy DFA answers the same, and a full DFA per small regex (the meta engine builds
/// one whenever `regex-automata/dfa-build` is on, which typst's syntect turns on in the app) costs memory and
/// compile time for the hundreds of engine regexes.
pub fn compile(pattern: &str) -> Regex {
    match fancy_regex::RegexBuilder::new(pattern).delegate_dfa_size_limit(0).build() {
        Ok(re) => re,
        Err(e) => panic!("bad engine regex {pattern:?}: {e}"),
    }
}

/// `compile`, kept per thread for the next call with the same pattern (the last 64 patterns): for patterns built
/// at run time from the line's variable and function names, which would otherwise compile on every keystroke.
pub fn compile_cached(pattern: &str) -> std::rc::Rc<Regex> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    thread_local! {
        static CACHE: RefCell<HashMap<String, Rc<Regex>>> = RefCell::new(HashMap::new());
    }
    CACHE.with(|cache| {
        if let Some(re) = cache.borrow().get(pattern) {
            return re.clone();
        }
        let re = Rc::new(compile(pattern));
        let mut cache = cache.borrow_mut();
        if cache.len() >= 64 {
            cache.clear();
        }
        cache.insert(pattern.to_string(), re.clone());
        re
    })
}

/// `js`, kept per thread like `compile_cached`.
pub fn js_cached(pattern: &str, flags: &str) -> std::rc::Rc<Regex> {
    compile_cached(&translate(pattern, flags))
}

/// Compiles a JS regex source as written in the TS (pass `"i"` for the `i` flag). ASCII-only JS classes are
/// kept ASCII: `\d`, `\w`, `\D`, `\W` and `\b` are rewritten to their ASCII forms, and `\/` to `/`, so a pattern can
/// be pasted from the TS unchanged.
pub fn js(pattern: &str, flags: &str) -> Regex {
    compile(&translate(pattern, flags))
}

// JS `\b` sits between an ASCII word character and anything else; Rust's would count θ as a word character
const ASCII_BOUNDARY: &str =
    "(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))";

fn translate(pattern: &str, flags: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    if flags.contains('i') {
        out.push_str("(?i)");
    }
    let mut in_class = false;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(e) = chars.next() else {
                    out.push('\\');
                    break;
                };
                let put = match (e, in_class) {
                    ('d', true) => "0-9",
                    ('d', false) => "[0-9]",
                    ('w', true) => "A-Za-z0-9_",
                    ('w', false) => "[A-Za-z0-9_]",
                    ('D', false) => "[^0-9]",
                    ('W', false) => "[^A-Za-z0-9_]",
                    ('b', false) => ASCII_BOUNDARY,
                    ('/', _) => "/",
                    _ => {
                        out.push('\\');
                        out.push(e);
                        continue;
                    }
                };
                out.push_str(put);
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                // a leading ] or ^] is literal in both dialects; copy it so it doesn't end the class
                let rest = chars.as_str();
                if let Some(tail) = rest.strip_prefix("^]") {
                    out.push_str("^\\]");
                    chars = tail.chars();
                } else if let Some(tail) = rest.strip_prefix(']') {
                    // JS `[]` matches nothing and `[^]` anything; neither appears in the engine
                    out.push_str("\\]");
                    chars = tail.chars();
                }
            }
            '[' => out.push_str("\\["),
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// `s.replace(re, f)` with the `g` flag. A regex that runs out of backtracking budget leaves the text as it was.
pub fn replace_all<'t>(re: &Regex, s: &'t str, f: impl FnMut(&Caps<'_>) -> String) -> Cow<'t, str> {
    let mut f = f;
    let mut out = String::new();
    let mut last = 0;
    let mut changed = false;
    for caps in re.captures_iter(s) {
        let Ok(caps) = caps else {
            return Cow::Borrowed(s);
        };
        let Some(m) = caps.get(0) else { continue };
        out.push_str(&s[last..m.start()]);
        out.push_str(&f(&caps));
        last = m.end();
        changed = true;
    }
    if !changed {
        return Cow::Borrowed(s);
    }
    out.push_str(&s[last..]);
    Cow::Owned(out)
}

/// `re.test(s)`; a regex that runs out of budget counts as no match.
pub fn test(re: &Regex, s: &str) -> bool {
    re.is_match(s).unwrap_or(false)
}

/// The text of group `i`, or "" when it didn't take part (JS gives `undefined`, which the TS mostly treats as "").
pub fn group<'t>(caps: &Caps<'t>, i: usize) -> &'t str {
    caps.get(i).map_or("", |m| m.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_behind_rewrites_like_js() {
        static RE: Lazy = Lazy::new(|| compile(r"(?<![A-Za-z])pi(?![A-Za-z])"));
        assert_eq!(
            replace_all(&RE, "2pi + spin", |_| "(pi)".into()),
            "2(pi) + spin"
        );
        assert!(test(&RE, "pi"));
    }

    #[test]
    fn js_classes_stay_ascii() {
        let re = js(r"\bpi\b", "");
        assert!(test(&re, "2 pi"));
        // JS: θ is not a word character, so `\b` sits between θ and p
        assert!(test(&re, "θpi"));
        let digits = js(r"^[\d.]+\d$", "");
        assert!(test(&digits, "1.5"));
        assert!(!test(&digits, "١٢"));
        let nested = js(r"[^\][]*", "");
        assert!(test(&nested, "abc"));
        assert!(test(&js(r"\bSIN\b", "i"), "sin(3)"));
        assert!(test(&js(r"a\/b", ""), "a/b"));
    }
}
