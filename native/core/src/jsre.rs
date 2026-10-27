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

/// A regex compiled on first use: `static RE: Lazy = Lazy::new(|| js(r"...", ""))`; `Lazy<Vec<..>>` for a table of
/// them. Like `LazyLock`, except on the thread that called `own_compiled_here` (the Windows app's engine thread),
/// which keeps copies of its own that `release_compiled` frees while the bar is hidden (qcalc-lazy).
pub type Lazy<T = Regex> = qcalc_lazy::Lazy<T>;

/// Makes this thread the one whose compiled regexes `release_compiled` can free (one thread per process, ever).
/// For a host whose engine calls all run on one long-lived thread of its own (the Windows app's engine thread).
pub fn own_compiled_here() -> bool {
    qcalc_lazy::own_here()
}

/// Frees this thread's compiled regexes and tables if it called `own_compiled_here`; each is compiled again when
/// it's next used. Returns how many statics were freed (qcalc-lazy's `release`).
///
/// # Safety
/// No reference to a regex (or table) from a `Lazy` may be alive on this thread across the call: call it between
/// jobs, from the top of the thread's loop. Other threads are unaffected.
pub use qcalc_lazy::release as release_compiled;

/// Whether this thread is compiling one of the regexes `release_compiled` frees (qcalc-lazy's `building`): the
/// Windows app's allocator keeps those blocks in a heap of their own.
pub use qcalc_lazy::building as building_compiled;

/// Empties this thread's cache of run-time patterns (`compile_cached`); a regex still in use stays alive.
pub fn clear_cache() {
    CACHE.with(|c| c.borrow_mut().clear());
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::HashMap<String, std::rc::Rc<Regex>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Compiles a pattern from the TS source. Panics on a bad pattern, which only a code change can cause.
pub fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(re) => {
            if building_compiled() {
                // a freeable copy: its search caches are made now too, so they share its heap (qcalc-lazy)
                let _ = re.is_match("");
            }
            re
        }
        Err(e) => panic!("bad engine regex {pattern:?}: {e}"),
    }
}

/// `compile`, kept per thread for the next call with the same pattern (the last 64 patterns): for patterns built
/// at run time from the line's variable and function names, which would otherwise compile on every keystroke.
pub fn compile_cached(pattern: &str) -> std::rc::Rc<Regex> {
    use std::rc::Rc;
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
