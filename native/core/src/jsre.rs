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
pub fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
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

/// `pattern` with its look-arounds taken out (and atomic groups made plain): a pattern that matches wherever
/// `pattern` does, and more, which the `regex` crate's automata run without backtracking. None when it can't be
/// made (a backreference, or something left that doesn't compile).
fn loosened(pattern: &str) -> Option<String> {
    let b = pattern.as_bytes();
    let mut out = String::with_capacity(pattern.len());
    let mut i = 0;
    let mut in_class = false;
    // the index just past the group opening at `start` (its `(`), skipping escapes and classes
    let group_end = |start: usize| -> Option<usize> {
        let (mut depth, mut k, mut class) = (0usize, start, false);
        while k < b.len() {
            match b[k] {
                b'\\' => k += 1,
                b'[' if !class => class = true,
                b']' if class => class = false,
                b'(' if !class => depth += 1,
                b')' if !class => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(k + 1);
                    }
                }
                _ => {}
            }
            k += 1;
        }
        None
    };
    while i < b.len() {
        let rest = &pattern[i..];
        if b[i] == b'\\' {
            let next = *b.get(i + 1)?;
            if next.is_ascii_digit() && next != b'0' || next == b'k' {
                return None;
            }
            let len = 1 + rest[1..].chars().next()?.len_utf8();
            out.push_str(&rest[..len]);
            i += len;
            continue;
        }
        if in_class {
            in_class = b[i] != b']';
        } else if b[i] == b'[' {
            in_class = true;
        } else if ["(?=", "(?!", "(?<=", "(?<!"]
            .iter()
            .any(|p| rest.starts_with(p))
        {
            i = group_end(i)?;
            continue;
        } else if rest.starts_with("(?>") {
            out.push_str("(?:");
            i += 3;
            continue;
        }
        let c = rest.chars().next()?;
        out.push(c);
        i += c.len_utf8();
    }
    Some(out)
}

/// How [`cannot_match`] rules a text out for one pattern, kept per thread by the regex's address.
struct Prefilter {
    pattern: String,
    /// Every match starts with one of the first set and ends with one of the second (either may be missing): a
    /// text holding none of a set can't match. Next to no memory, so it's tried first.
    literals: Vec<aho_corasick::AhoCorasick>,
    /// The loosened pattern, and once compiled (about 15 KB) the regex: at once for a pattern without literals,
    /// otherwise after the literals let [`UPGRADE`] texts through that the pattern didn't match.
    loose: Option<String>,
    compiled: Option<std::rc::Rc<Regex>>,
    misses: u32,
    /// Uses so far, up to [`WARM`], when the rest is built.
    uses: u32,
}

/// Texts the literals may let through to the backtracking VM for nothing before the loosened regex is compiled.
const UPGRADE: u32 = 256;

/// Uses of a pattern before its prefilter is built.
const WARM: u32 = 3;

impl Prefilter {
    /// Nothing built yet: rules nothing out.
    fn pending(pattern: &str) -> Prefilter {
        Prefilter {
            pattern: pattern.to_string(),
            literals: Vec::new(),
            loose: None,
            compiled: None,
            misses: 0,
            uses: 1,
        }
    }

    fn of(pattern: &str) -> Prefilter {
        use regex_syntax::hir::literal::{ExtractKind, Extractor};
        let loose = loosened(pattern);
        let literals: Vec<aho_corasick::AhoCorasick> = loose
            .as_deref()
            .and_then(|l| regex_syntax::Parser::new().parse(l).ok())
            .map(|hir| {
                [ExtractKind::Prefix, ExtractKind::Suffix]
                    .into_iter()
                    .filter_map(|kind| {
                        let seq = Extractor::new().kind(kind).extract(&hir);
                        let lits = seq.literals()?;
                        if lits.is_empty() || lits.iter().any(|l| l.as_bytes().is_empty()) {
                            return None;
                        }
                        aho_corasick::AhoCorasick::new(lits.iter().map(|l| l.as_bytes())).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut p = Prefilter {
            pattern: pattern.to_string(),
            literals,
            loose,
            compiled: None,
            misses: 0,
            uses: WARM,
        };
        if p.literals.is_empty() {
            p.compile();
        }
        p
    }

    fn compile(&mut self) {
        self.compiled = self
            .loose
            .take()
            .and_then(|l| Regex::new(&l).ok())
            .map(std::rc::Rc::new);
    }

    fn rules_out(&self, s: &str) -> bool {
        self.literals.iter().any(|lits| !lits.is_match(s))
            || self
                .compiled
                .as_ref()
                .is_some_and(|re| matches!(re.is_match(s), Ok(false)))
    }
}

thread_local! {
    static PREFILTERS: std::cell::RefCell<std::collections::HashMap<usize, Prefilter>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A quick test that `re` can't match `s`, without its look-arounds' backtracking (see [`Prefilter`]).
fn cannot_match(re: &Regex, s: &str) -> bool {
    let pattern = re.as_str();
    let key = re as *const Regex as usize;
    PREFILTERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        match cache.get_mut(&key) {
            Some(p) if p.pattern == pattern => {
                // a pattern earns its prefilter on its WARM-th use, so the first evaluation (the app's warm-up)
                // doesn't pay to analyse every pattern it touches once
                if p.uses < WARM {
                    p.uses += 1;
                    if p.uses == WARM {
                        *p = Prefilter::of(pattern);
                    }
                }
            }
            _ => {
                if cache.len() >= 4096 {
                    cache.clear();
                }
                cache.insert(key, Prefilter::pending(pattern));
            }
        }
        let out = cache.get(&key).is_some_and(|p| p.rules_out(s));
        // the fuzz harness builds with this to check, on every call, that a ruled-out text really can't match
        #[cfg(feature = "check-prefilter")]
        assert!(
            !(out && re.is_match(s).unwrap_or(false)),
            "prefilter ruled out a match: {pattern:?} on {s:?}"
        );
        out
    })
}

/// `re` ran on a text its prefilter let through and didn't match: past [`UPGRADE`] of those, compile the loose
/// regex.
fn missed(re: &Regex) {
    let key = re as *const Regex as usize;
    PREFILTERS.with(|cache| {
        if let Some(p) = cache.borrow_mut().get_mut(&key)
            && p.compiled.is_none()
            && p.loose.is_some()
        {
            p.misses += 1;
            if p.misses >= UPGRADE {
                p.compile();
            }
        }
    });
}

/// `s.replace(re, f)` with the `g` flag. A regex that runs out of backtracking budget leaves the text as it was.
pub fn replace_all<'t>(re: &Regex, s: &'t str, f: impl FnMut(&Caps<'_>) -> String) -> Cow<'t, str> {
    if cannot_match(re, s) {
        return Cow::Borrowed(s);
    }
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
        missed(re);
        return Cow::Borrowed(s);
    }
    out.push_str(&s[last..]);
    Cow::Owned(out)
}

/// `re.test(s)`; a regex that runs out of budget counts as no match.
pub fn test(re: &Regex, s: &str) -> bool {
    if cannot_match(re, s) {
        return false;
    }
    let hit = re.is_match(s).unwrap_or(false);
    if !hit {
        missed(re);
    }
    hit
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
    fn loosened_patterns_drop_look_arounds_only() {
        let l = |p: &str| loosened(p);
        assert_eq!(l(r"(?<![A-Za-z])pi(?![A-Za-z(])").as_deref(), Some("pi"));
        assert_eq!(l(r"a(?=b(c))d|(?<=x)e").as_deref(), Some("ad|e"));
        assert_eq!(l(r"[(?=]x\(?=y").as_deref(), Some(r"[(?=]x\(?=y"));
        assert_eq!(l(r"(?>ab)+(?<n>c)").as_deref(), Some("(?:ab)+(?<n>c)"));
        assert_eq!(l(r"(a)\1"), None);
        // the quick test never turns a match into none
        let re = js(r"\bsin\b(?=\s*\()", "i");
        assert!(test(&re, "2 SIN (x)"));
        assert!(!test(&re, "sinx(1)"));
        assert_eq!(replace_all(&re, "cos(1)", |_| "s".into()), "cos(1)");
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
