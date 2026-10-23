//! The text passes that turn typed maths into mathjs syntax: `preprocessAscii`, `preprocessChecked` and their
//! helpers from `src/engine/scientific.ts`, in the same order with the same patterns.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::{NAMES, ScientificContext};
use crate::jsnum;
use crate::jsre::{self, Lazy, Regex};

/// A regex alternation matching any of `names` literally, longest first so `ab` wins over `a`.
pub fn names_pattern(names: &[String]) -> String {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
    sorted.iter().map(|n| fancy_regex::escape(n).into_owned()).collect::<Vec<_>>().join("|")
}

fn js(pattern: &str, flags: &str) -> Regex {
    jsre::js(pattern, flags)
}

fn replace(re: &Regex, s: &str, f: impl FnMut(&jsre::Caps<'_>) -> String) -> String {
    jsre::replace_all(re, s, f).into_owned()
}

fn lit(re: &Regex, s: &str, to: &str) -> String {
    replace(re, s, |_| to.to_string())
}

fn g<'a>(c: &'a jsre::Caps<'a>, i: usize) -> &'a str {
    jsre::group(c, i)
}

const WRAP_SKIP: &[&str] = &["pi", "tau", "inf", "infinity", "ans", "e", "mod", "catalan"];
const ROOTS: &[&str] = &["sqrt", "cbrt", "nthroot"];
const BARE_STOP: &[&str] = &["in", "to", "as", "of", "per", "and", "or", "xor", "not", "mod", "at", "for", "from", "over"];
const NOT_GLUED: &[&str] = &["mod", "re", "im", "pi", "tau", "inf", "infinity", "ans"];

static GLUED_RE: Lazy = Lazy::new(|| {
    let mut fns: Vec<&str> = NAMES.split('|').filter(|n| !NOT_GLUED.contains(n)).collect();
    fns.sort_by_key(|n| std::cmp::Reverse(n.len()));
    js(&format!(r"(?<![A-Za-z_])({})([A-Za-z]|theta|θ)(?![A-Za-z0-9_(])", fns.join("|")), "")
});

/// `logx` and `sinθ` mean log x and sin θ. x, y, z and θ always split; another letter only once it's defined,
/// since `cost` or `logs` are words. A name that's itself defined stays whole.
pub fn split_glued_functions(expr: &str, known: &dyn Fn(&str) -> bool) -> String {
    replace(&GLUED_RE, expr, |c| {
        let (word, f, v) = (g(c, 0), g(c, 1), g(c, 2));
        if known(word) || !(matches!(v, "x" | "y" | "z" | "theta" | "θ") || known(v)) {
            word.to_string()
        } else {
            format!("{f} {v}")
        }
    })
}

struct Callable {
    names: Vec<String>,
    implicit: Regex,
}

/// Cached per user-function list, as in the TS.
fn callable_names(extra: &[String]) -> std::sync::Arc<Callable> {
    static CACHE: LazyLock<Mutex<HashMap<String, std::sync::Arc<Callable>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
    let key = extra.join("|");
    if let Ok(cache) = CACHE.lock()
        && let Some(c) = cache.get(&key)
    {
        return c.clone();
    }
    let mut names: Vec<String> = NAMES
        .split('|')
        .map(str::to_string)
        .chain(extra.iter().cloned())
        .filter(|n| !WRAP_SKIP.contains(&n.to_lowercase().as_str()))
        .collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
    let implicit = js(&format!(r"(\d)(\s*)({})(?![A-Za-z_])", names.join("|")), "i");
    let c = std::sync::Arc::new(Callable { names, implicit });
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, c.clone());
    }
    c
}

/// The argument of a bare function call (`sin 30`, `sin 2pi`); a power stays inside, so `log 10^3` is 3.
static BARE_ATOM_RE: Lazy = Lazy::new(|| {
    js(r"^(?:pi|tau|e|\d+(?:\.\d+)?(?:e[+-]?\d+)?(?:\*(?:pi|tau|\(pi\)|\(tau\))(?![A-Za-z0-9_]))?)(?:\s*\^\s*(?:-?\d+(?:\.\d+)?(?![\d.])|\([^()]*\)))*", "i")
});
static BARE_WORD_RE: Lazy =
    Lazy::new(|| js(r"^([A-Za-z][A-Za-z0-9_]*)(?![A-Za-z0-9_(])(?:\s*\^\s*(?:-?\d+(?:\.\d+)?(?![\d.])|\([^()]*\)))*", ""));

fn is_named(word: &str, names: &[String]) -> bool {
    let lower = word.to_lowercase();
    BARE_STOP.contains(&lower.as_str()) || names.iter().any(|n| n.to_lowercase() == lower)
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn starts_with_ci(rest: &[char], name: &str) -> bool {
    let n: Vec<char> = name.chars().collect();
    rest.len() >= n.len() && rest.iter().zip(&n).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
}

fn first_match_len(re: &Regex, s: &str) -> usize {
    match re.find(s) {
        Ok(Some(m)) => m.as_str().chars().count(),
        _ => 0,
    }
}

/// `sin x` → `sin(x)`, `sqrt 2` → `sqrt(2)`: a function written without brackets takes the next atom.
pub fn wrap_bare_functions(expr: &str, extra: &[String]) -> String {
    let callable = callable_names(extra);
    let names = &callable.names;
    let cs: Vec<char> = expr.chars().collect();
    let text = |from: usize, to: usize| -> String { cs[from.min(cs.len())..to.min(cs.len())].iter().collect() };
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let ch = cs[i];
        let start_ident = ch.is_ascii_alphabetic() && (i == 0 || !is_word_char(cs[i - 1]));
        if start_ident {
            let rest = &cs[i..];
            let found = names.iter().find(|n| {
                let len = n.chars().count();
                starts_with_ci(rest, n) && rest.get(len).is_none_or(|c| !(c.is_ascii_alphabetic() || *c == '_'))
            });
            if let Some(name) = found {
                let len = name.chars().count();
                let mut j = i + len;
                while cs.get(j) == Some(&' ') {
                    j += 1;
                }
                if cs.get(j) != Some(&'(') {
                    let arg_len = first_match_len(&BARE_ATOM_RE, &text(j, cs.len()));
                    let arg = text(j, j + arg_len);
                    let joined = if arg_len > 0 && ROOTS.contains(&name.to_lowercase().as_str()) {
                        String::new()
                    } else {
                        match BARE_WORD_RE.captures(text(j + arg_len, cs.len()).as_str()) {
                            Ok(Some(w)) if !is_named(g(&w, 1), names) => g(&w, 0).to_string(),
                            _ => String::new(),
                        }
                    };
                    let joined_len = joined.chars().count();
                    let after = text(j + arg_len + joined_len, cs.len());
                    static SPACE_THEN_WORD: Lazy = Lazy::new(|| js(r"^\s+[A-Za-z0-9]", ""));
                    static LEAD_LETTERS: Lazy = Lazy::new(|| js(r"^[A-Za-z]+", ""));
                    if !joined.is_empty() && jsre::test(&SPACE_THEN_WORD, &after) {
                        let next = match LEAD_LETTERS.find(after.trim_start()) {
                            Ok(Some(m)) => m.as_str().to_string(),
                            _ => String::new(),
                        };
                        if !is_named(&next, names) {
                            out.push_str(&text(i, j + arg_len + joined_len));
                            i = j + arg_len + joined_len;
                            continue;
                        }
                    }
                    if arg_len > 0 || joined_len > 0 {
                        out.push_str(&format!("{name}({arg}{joined})"));
                        i = j + arg_len + joined_len;
                        continue;
                    }
                }
                out.push_str(&text(i, i + len));
                i += len;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// ×, middle dot, dot operator and bullet operator from pasted math; the word `dot` too.
pub fn rewrite_typeset_mul(s: &str) -> String {
    static TYPESET: Lazy = Lazy::new(|| js(r"[×·⋅∙]", ""));
    static DOT: Lazy = Lazy::new(|| js(r"(?<![\\A-Za-z])dot(?![A-Za-z])", "i"));
    lit(&DOT, &lit(&TYPESET, s, "*"), "*")
}

/// Typed "p i" or "p·i" is the constant π.
pub fn stitch_constants(s: &str) -> String {
    static IMAG_I: Lazy = Lazy::new(|| js(r"\\imaginaryI\b", "i"));
    static MATHRM_I: Lazy = Lazy::new(|| js(r"\\mathrm\{i\}", "i"));
    static P_I: Lazy = Lazy::new(|| js(r"\bp(?:\s*(?:\*|·|⋅|\\cdot)\s*|\s+)i\b", "i"));
    static NUM_PI: Lazy = Lazy::new(|| js(r"(\d+(?:\.\d+)?)(\s*)\(pi\)", "i"));
    static NUM_NAME: Lazy = Lazy::new(|| js(r"(\d+(?:\.\d+)?)(pi|tau)\b", "i"));
    let s = lit(&IMAG_I, s, "i");
    let s = lit(&MATHRM_I, &s, "i");
    let s = lit(&P_I, &s, "(pi)");
    let s = replace(&NUM_PI, &s, |c| format!("{}*{}(pi)", g(c, 1), g(c, 2)));
    replace(&NUM_NAME, &s, |c| format!("{}*{}", g(c, 1), g(c, 2)))
}

/// Where the additive left side of byte `pos` begins: after the nearest unmatched `(` or same-level comma, else 0.
fn group_start(s: &str, pos: usize) -> usize {
    let b = s.as_bytes();
    let mut depth = 0;
    for i in (0..pos).rev() {
        match b[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    return i + 1;
                }
                depth -= 1;
            }
            b',' if depth == 0 => return i + 1,
            _ => {}
        }
    }
    0
}

/// `a ± b%` means `a * (1 ± b/100)`, where `a` is the whole additive left side (`200 + 15%` = 230).
fn rewrite_percent_add(expr: &str) -> String {
    static TERM: Lazy = Lazy::new(|| js(r"([+-])\s*(\d+(?:\.\d+)?)\s*%(?=\s*(?:[-+),]|$))", ""));
    static OPEN_END: Lazy = Lazy::new(|| js(r"[-+*/^(,]\s*$", ""));
    static EXP_END: Lazy = Lazy::new(|| js(r"\d[eE]$", ""));
    static ONLY_PERCENTS: Lazy = Lazy::new(|| js(r"^[\s+-]*(?:\d+(?:\.\d+)?\s*%[\s+-]*)+$", ""));
    let mut s = expr.to_string();
    let mut at = 0;
    while at <= s.len() {
        let Ok(Some(c)) = TERM.captures_from_pos(s.as_str(), at) else { break };
        let Some(m) = c.get(0) else { break };
        let (start_m, end_m) = (m.start(), m.end());
        let start = group_start(&s, start_m);
        let left = &s[start..start_m];
        let skip = left.trim().is_empty()
            || jsre::test(&OPEN_END, left)
            || jsre::test(&EXP_END, left)
            || jsre::test(&ONLY_PERCENTS, left);
        if skip {
            at = if end_m > start_m { end_m } else { end_m + 1 };
            continue;
        }
        let put = format!("(({})*(1{}{}/100))", left.trim(), g(&c, 1), g(&c, 2));
        s = format!("{}{}{}", &s[..start], put, &s[end_m..]);
        at = start + put.len();
    }
    s
}

const SUPERSCRIPT_DIGITS: &str = "⁻⁰¹²³⁴⁵⁶⁷⁸⁹";

const INVERSE_POWER_FNS: &[(&str, &str)] = &[
    ("csch", "acsch"),
    ("sech", "asech"),
    ("coth", "acoth"),
    ("sinh", "asinh"),
    ("cosh", "acosh"),
    ("tanh", "atanh"),
    ("csc", "acsc"),
    ("sec", "asec"),
    ("cot", "acot"),
    ("sin", "asin"),
    ("cos", "acos"),
    ("tan", "atan"),
];

/// sin^-1(x), cos^(-1)(x), tan⁻¹(x). Longer names first so sinh^-1 isn't read as sin.
fn rewrite_inverse_power(expr: &str) -> String {
    static RES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        INVERSE_POWER_FNS
            .iter()
            .map(|(f, inv)| (js(&format!(r"(?<![A-Za-z]){f}\s*(?:\^\s*(?:-1|\(\s*-1\s*\))|⁻¹)"), "i"), *inv))
            .collect()
    });
    let mut s = expr.to_string();
    for (re, inv) in RES.iter() {
        s = lit(re, &s, inv);
    }
    s
}

const POWER_FNS: &str = "sinh|cosh|tanh|csch|sech|coth|sin|cos|tan|csc|sec|cot|ln|log";

/// Byte index of the `)` closing the `(` at byte `open`, or None.
fn closing(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, b) in s.bytes().enumerate().skip(open) {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// `sin^2(x)`, `tan²(x)`, `ln^2(x)`: the function's answer to that power, as written in textbooks.
fn rewrite_function_power(expr: &str) -> String {
    static BARE: Lazy = Lazy::new(|| {
        js(&format!(r"(?<![A-Za-z])({POWER_FNS})\s*\^\s*(?:(\d+)|\(\s*(\d+)\s*\))\s+(\d+(?:\.\d+)?|pi|[A-Za-z])(?![A-Za-z0-9_.(])"), "i")
    });
    static CALL: Lazy = Lazy::new(|| js(&format!(r"(?<![A-Za-z])({POWER_FNS})\s*\^\s*(?:(\d+)|\(\s*(\d+)\s*\))\s*\("), "i"));
    let mut s = replace(&BARE, expr, |c| {
        let n = if c.get(2).is_some() { g(c, 2) } else { g(c, 3) };
        format!("({}({}))^{n}", g(c, 1), g(c, 4))
    });
    for _ in 0..20 {
        let Ok(Some(c)) = CALL.captures(s.as_str()) else { break };
        let Some(m) = c.get(0) else { break };
        let open = m.end() - 1;
        let Some(close) = closing(&s, open) else { break };
        let n = if c.get(2).is_some() { g(&c, 2) } else { g(&c, 3) };
        s = format!("{}({}{})^{n}{}", &s[..m.start()], g(&c, 1), &s[open..=close], &s[close + 1..]);
    }
    s
}

/// `sin(pi/6 rad)`: the unit is for the whole argument, not just the 6 it sits beside.
fn wrap_angle_argument(expr: &str) -> String {
    static RE: Lazy = Lazy::new(|| {
        js(r"(?<![A-Za-z])(sin|cos|tan|csc|sec|cot)\(\s*([^()]*?[/])([^()]*?)\s+(rad|rads|radian|radians|deg|degs|degree|degrees)\s*\)", "i")
    });
    static CONSTS: Lazy = Lazy::new(|| js(r"\b(?:pi|tau|e)\b", "i"));
    static LETTER: Lazy = Lazy::new(|| js(r"[A-Za-z]", ""));
    replace(&RE, expr, |c| {
        let arg = format!("{}{}", g(c, 2), g(c, 3));
        if jsre::test(&LETTER, &lit(&CONSTS, &arg, "")) {
            g(c, 0).to_string()
        } else {
            format!("{}(({arg}) {})", g(c, 1), g(c, 4))
        }
    })
}

static CANONICAL: LazyLock<HashMap<String, &'static str>> =
    LazyLock::new(|| NAMES.split('|').map(|n| (n.to_lowercase(), n)).collect());

/// Built-in names, written any case when called: `SIN(30)`, `Ln(2)`; `Pi` and `TAU` are the constants too.
fn lower_calls(expr: &str, extra: &[String]) -> String {
    static CALL: Lazy = Lazy::new(|| js(&format!(r"(?<![A-Za-z0-9_])({NAMES})(?=\s*[(^⁻])"), "i"));
    static CONST: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_])(?:pi|tau)(?![A-Za-z0-9_])", "i"));
    let s = replace(&CALL, expr, |c| {
        let m = g(c, 0);
        if extra.iter().any(|e| e == m) {
            m.to_string()
        } else {
            CANONICAL.get(&m.to_lowercase()).map_or_else(|| m.to_string(), |n| (*n).to_string())
        }
    });
    replace(&CONST, &s, |c| {
        let m = g(c, 0);
        if extra.iter().any(|e| e == m) { m.to_string() } else { m.to_lowercase() }
    })
}

/// One-argument `log(...)` is base 10, however its argument nests, and `log (100)` too.
fn rewrite_log10(expr: &str) -> String {
    static RE: Lazy = Lazy::new(|| js(r"\blog\s*\(", ""));
    let mut s = expr.to_string();
    let mut at = 0;
    while at <= s.len() {
        let Ok(Some(m)) = RE.find_from_pos(s.as_str(), at) else { break };
        let (start, end) = (m.start(), m.end());
        let open = end - 1;
        let b = s.as_bytes();
        let (mut depth, mut commas, mut close) = (0i32, 0, None);
        for (i, &c) in b.iter().enumerate().skip(open) {
            match c {
                b'(' | b'[' => depth += 1,
                b')' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                b',' if depth == 1 => commas += 1,
                _ => {}
            }
        }
        if close.is_none() || commas > 0 {
            at = end;
            continue;
        }
        s = format!("{}log10{}", &s[..start], &s[open..]);
        at = start + 5;
    }
    s
}

/// Index of the bracket that opens the group closing at `close`, or None.
fn opening_index(s: &[char], close: usize, open_ch: char, close_ch: char) -> Option<usize> {
    let mut depth = 0;
    for i in (0..=close).rev() {
        if s[i] == close_ch {
            depth += 1;
        } else if s[i] == open_ch {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// `n!` becomes `factorial(n)`, `(…)!` and `sqrt(4)!` take their whole group.
fn rewrite_factorial(expr: &str) -> String {
    let mut s: Vec<char> = expr.chars().collect();
    for _ in 0..32 {
        let Some(idx) = (0..s.len()).rev().find(|&i| s[i] == '!' && s.get(i + 1) != Some(&'=')) else { break };
        let mut j = idx as isize - 1;
        while j >= 0 && s[j as usize] == ' ' {
            j -= 1;
        }
        if j < 0 {
            break;
        }
        let j = j as usize;
        let c = s[j];
        let start = if c == ')' {
            let Some(open) = opening_index(&s, j, '(', ')') else { break };
            // `sqrt(4)!` takes the whole call, not just its parentheses
            let mut k = open;
            while k > 0 && is_word_char(s[k - 1]) {
                k -= 1;
            }
            while k < open && s[k].is_ascii_digit() {
                k += 1;
            }
            k.min(open)
        } else if c == ']' {
            let Some(open) = opening_index(&s, j, '[', ']') else { break };
            open
        } else if c.is_ascii_digit() || c == '.' {
            // a sign belongs to the number only inside an exponent (`1e+5!`), never `2+3!`
            let mut start = j;
            while start > 0 {
                let p = s[start - 1];
                let exp_sign = matches!(p, '+' | '-')
                    && start >= 3
                    && matches!(s[start - 2], 'e' | 'E')
                    && s[start - 3].is_ascii_digit();
                if p.is_ascii_digit() || matches!(p, '.' | 'e' | 'E') || exp_sign {
                    start -= 1;
                } else {
                    break;
                }
            }
            start
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut start = j;
            while start > 0 && is_word_char(s[start - 1]) {
                start -= 1;
            }
            start
        } else {
            break;
        };
        let operand: String = s[start..=j].iter().collect();
        let mut next: Vec<char> = s[..start].to_vec();
        next.extend(format!("factorial({operand})").chars());
        next.extend_from_slice(&s[idx + 1..]);
        s = next;
    }
    s.into_iter().collect()
}

/// Every rewrite from typed maths to mathjs syntax, in the TS order.
pub fn preprocess_ascii(expr: &str, extra: &[String]) -> String {
    static ROOT_LETTER: Lazy = Lazy::new(|| js(r"([√∛])([A-Za-z])(?![A-Za-z0-9_(])", ""));
    static SUPERS: Lazy = Lazy::new(|| js(r"[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+", ""));
    static ABS: Lazy = Lazy::new(|| js(r"(?<![\d)\]!])\|([^|]+)\|", ""));
    static STARSTAR: Lazy = Lazy::new(|| js(r"\*\*", ""));
    static POW_MOD: Lazy =
        Lazy::new(|| js(r"(?<=(?:^|[(,+]|[\w)]\s*-)\s*)(\d+)\s*\^\s*(\d+)\s*mod\s*(\d+)(?![\d.]|\s*[(^!])", ""));
    static PCT_OF: Lazy = Lazy::new(|| js(r"(\d+(?:\.\d+)?)\s*%\s*of\b", "i"));
    static PCT: Lazy = Lazy::new(|| js(r"(\d+(?:\.\d+)?)\s*%", ""));
    static INF: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_])inf(?:inity)?(?![A-Za-z0-9_])", "i"));
    static NCR: Lazy = Lazy::new(|| js(r"((?:\([^()]*\)|\d+(?:\.\d+)?))\s*nCr\s*((?:\([^()]*\)|\d+(?:\.\d+)?))", "i"));
    static NPR: Lazy = Lazy::new(|| js(r"((?:\([^()]*\)|\d+(?:\.\d+)?))\s*nPr\s*((?:\([^()]*\)|\d+(?:\.\d+)?))", "i"));
    static NCR_CALL: Lazy = Lazy::new(|| js(r"\bnCr\s*\(", ""));
    static NPR_CALL: Lazy = Lazy::new(|| js(r"\bnPr\s*\(", ""));
    static N_CALL: Lazy = Lazy::new(|| js(r"\bn\s*\(", ""));
    static COUNT_CALL: Lazy = Lazy::new(|| js(r"\bcount\s*\(", ""));
    static RANGE: Lazy = Lazy::new(|| js(r"\[([^\][]*?)\s*\.\.\.\s*([^\][]*?)\]", ""));
    static LOG_BASE: Lazy = Lazy::new(|| js(r"\blog_(\d+(?:\.\d+)?)\s*\(([^)]+)\)", ""));
    static WORDS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        [
            (r"\barcsin\b", "", "asin"),
            (r"\barccos\b", "", "acos"),
            (r"\barctan\b", "", "atan"),
            (r"\barccsc\b", "", "acsc"),
            (r"\barccot\b", "", "acot"),
            (r"\barcsec(?=\s*\()", "", "asec"),
            (r"\barctan2\b", "", "atan2"),
        ]
        .iter()
        .map(|(p, f, to)| (js(p, f), *to))
        .collect()
    });
    static HYPERBOLIC: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        [
            (r"\barcsinh\b|\barsinh\b", "", "asinh"),
            (r"\barccosh\b|\barcosh\b", "", "acosh"),
            (r"\barctanh\b|\bartanh\b", "", "atanh"),
            (r"\barccsch\b|\barcsch\b", "", "acsch"),
            (r"\barcsech\b", "", "asech"),
            (r"\barccoth\b", "", "acoth"),
            (r"\bnthroot\b", "", "nthRoot"),
            (r"\bgcf\b", "i", "gcf"),
            (r"\blcm\b", "i", "lcm"),
            (r"\blcf\b", "i", "lcf"),
        ]
        .iter()
        .map(|(p, f, to)| (js(p, f), *to))
        .collect()
    });
    static REAL: Lazy = Lazy::new(|| js(r"\breal\b", ""));
    static IMAG: Lazy = Lazy::new(|| js(r"\bimag\b", ""));

    // `√x` roots a single letter; `√2` and `√(x + 1)` already read right
    let s = replace(&ROOT_LETTER, &lower_calls(expr, extra), |c| format!("{}({})", g(c, 1), g(c, 2)));
    let s = rewrite_typeset_mul(&s)
        .replace('°', " deg")
        .replace('÷', "/")
        .replace('−', "-")
        .replace('π', "(pi)")
        .replace('τ', "(tau)")
        .replace('∞', "Infinity")
        .replace('√', "sqrt")
        .replace('∛', "cbrt")
        .replace('θ', "theta");
    let s = s.replace('ζ', "zeta");
    let s = replace(&SUPERS, &s, |c| {
        let digits: String = g(c, 0)
            .chars()
            .map(|ch| match SUPERSCRIPT_DIGITS.chars().position(|d| d == ch) {
                Some(0) => "-".to_string(),
                Some(p) => (p - 1).to_string(),
                None => String::new(),
            })
            .collect();
        format!("^({digits})")
    });
    let s = replace(&ABS, &s, |c| format!("abs({})", g(c, 1)));
    let s = lit(&STARSTAR, &s, "^");
    let s = replace(&POW_MOD, &s, |c| format!("powmod({},{},{})", g(c, 1), g(c, 2), g(c, 3)));
    let s = stitch_constants(&s);
    let s = replace(&PCT_OF, &s, |c| format!("({}/100)*", g(c, 1)));
    let s = rewrite_percent_add(&s);
    let mut s = replace(&PCT, &s, |c| format!("({}/100)", g(c, 1)));
    for (re, to) in WORDS.iter() {
        s = lit(re, &s, to);
    }
    let s = rewrite_inverse_power(&s);
    let s = rewrite_function_power(&s);
    let s = wrap_angle_argument(&s);
    let mut s = lit(&INF, &s, "Infinity");
    for (re, to) in HYPERBOLIC.iter() {
        s = lit(re, &s, to);
    }
    let s = replace(&NCR, &s, |c| format!("combinations({},{})", g(c, 1), g(c, 2)));
    let s = replace(&NPR, &s, |c| format!("permutations({},{})", g(c, 1), g(c, 2)));
    let s = lit(&NCR_CALL, &s, "combinations(");
    let mut s = lit(&NPR_CALL, &s, "permutations(");
    // `n` can be a user function; `count` is reserved, so it always counts
    if !extra.iter().any(|e| e == "n") {
        s = lit(&N_CALL, &s, "length(");
    }
    let s = lit(&COUNT_CALL, &s, "length(");
    let s = replace(&RANGE, &s, |c| format!("inclusiveRange({},{})", g(c, 1), g(c, 2)));
    let s = lit(&IMAG, &lit(&REAL, &s, "re"), "im");
    let s = replace(&LOG_BASE, &s, |c| format!("log({}, {})", g(c, 2), g(c, 1)));
    let callable = callable_names(extra);
    let s = replace(&callable.implicit, &s, |c| format!("{}*{}{}", g(c, 1), g(c, 2), g(c, 3)));
    let s = wrap_bare_functions(&s, extra);
    // after wrapping, so bare `log 1000` is base 10 too
    let s = rewrite_log10(&s);
    rewrite_factorial(&s)
}

static NAME_RE: Lazy = Lazy::new(|| js(&format!(r"\b({NAMES})\b"), "i"));

/// The text mathjs evaluates, or None when the line isn't maths this layer should answer (a clock `2:30`, an
/// assignment, `3!!`, `17 % 5`, leftover words).
pub fn preprocess_checked(text: &str, ctx: &ScientificContext) -> Option<String> {
    static PHI_CHARS: Lazy = Lazy::new(|| js(r"[φϕ]", ""));
    static ANS: Lazy = Lazy::new(|| js(r"\bans\b", "i"));
    static PHI: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_])phi(?![A-Za-z0-9_])", ""));
    static SCI: Lazy = Lazy::new(|| js(r"\d+(?:\.\d+)?e[+-]?\d+", "i"));
    static MATHY: Lazy = Lazy::new(|| js(r"\d|pi|tau|ans|phi|\be\b|(?<![A-Za-z_])i(?![A-Za-z0-9_])", "i"));
    static WORDY: Lazy = Lazy::new(|| js(r"[a-zA-Z_$€£¥₹#?=\\]", ""));
    static IE: Lazy = Lazy::new(|| js(r"[iIeE]", ""));
    static PLAIN: Lazy = Lazy::new(|| js(r"^[\d\s+\-*/^().,eE!|[\]:]+$", ""));
    static ASSIGN: Lazy = Lazy::new(|| js(r"(?<![=<>!])=(?!=)", ""));
    static DOUBLE_BANG: Lazy = Lazy::new(|| js(r"!\s*!", ""));
    static REMAINDER: Lazy = Lazy::new(|| js(r"%\s*[\d.(]", ""));

    let mut src = text.trim().to_string();
    if src.is_empty() {
        return None;
    }
    let fn_names: Vec<String> = ctx.functions.keys().cloned().collect();
    // a variable followed by `(` multiplies (`n(2+3)`), unless a function has the same name
    let var_names: Vec<String> = ctx.variables.keys().filter(|n| !ctx.functions.contains_key(*n)).cloned().collect();
    if !var_names.is_empty() {
        let re = js(&format!(r"(?<![A-Za-z0-9_])({})\s*\(", names_pattern(&var_names)), "");
        src = replace(&re, &src, |c| format!("{}*(", g(c, 1)));
    }
    // φ is the golden ratio's letter too, unless it's a variable of its own
    if !ctx.variables.contains_key("φ") {
        src = lit(&PHI_CHARS, &src, "phi");
    }
    let mut expr = preprocess_ascii(&src, &fn_names);
    if let Some(ans) = ctx.ans {
        expr = lit(&ANS, &expr, &format!("({})", jsnum::to_string(ans)));
    }
    let allow: Vec<String> = ctx.variables.keys().cloned().chain(fn_names.iter().cloned()).collect();
    let allow_re = (!allow.is_empty())
        .then(|| js(&format!(r"(?<![A-Za-z_])(?:{})(?![A-Za-z0-9_])", names_pattern(&allow)), "i"));
    let mut leftover = lit(&SCI, &lit(&PHI, &lit(&NAME_RE, &expr, ""), ""), "");
    if let Some(re) = &allow_re {
        leftover = lit(re, &leftover, "");
    }
    let named = jsre::test(&NAME_RE, &src) || jsre::test(&NAME_RE, &expr);
    if !named && allow_re.is_none() && !jsre::test(&MATHY, &expr) {
        return None;
    }
    if jsre::test(&WORDY, &lit(&IE, &leftover, "")) && !jsre::test(&PLAIN, &expr) && !named {
        return None;
    }
    // `2:30` is a clock reading, never a mathjs range
    if expr.contains(':') {
        return None;
    }
    // `pi = 3` would be an assignment, `3!!` reads as a double factorial as often as (3!)!, and `17 % 5` is a
    // remainder to a programmer, not 17% times 5
    if jsre::test(&ASSIGN, &expr) || jsre::test(&DOUBLE_BANG, &src) || jsre::test(&REMAINDER, &src) {
        return None;
    }
    Some(expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre(s: &str) -> String {
        preprocess_ascii(s, &[])
    }

    #[test]
    fn rewrites_like_the_ts() {
        assert_eq!(pre("3!"), "factorial(3)");
        assert_eq!(pre("(2+1)!"), "factorial((2+1))");
        assert_eq!(pre("sqrt(4)!"), "factorial(sqrt(4))");
        assert_eq!(pre("2+3!"), "2+factorial(3)");
        assert_eq!(pre("2³"), "2^(3)");
        assert_eq!(pre("10⁻³"), "10^(-3)");
        assert_eq!(pre("50%"), "(50/100)");
        assert_eq!(pre("200 + 15%"), "((200)*(1+15/100))");
        assert_eq!(pre("15% of 200"), "(15/100)* 200");
        assert_eq!(pre("2**3"), "2^3");
        assert_eq!(pre("6÷3−1"), "6/3-1");
        assert_eq!(pre("|-3|"), "abs(-3)");
        assert_eq!(pre("√2"), "sqrt(2)");
        assert_eq!(pre("sin 30"), "sin(30)");
        assert_eq!(pre("2sin(30)"), "2*sin(30)");
        assert_eq!(pre("log(100)"), "log10(100)");
        assert_eq!(pre("sin^2(30)"), "(sin(30))^2");
        assert_eq!(pre("tan⁻¹(1)"), "atan(1)");
        assert_eq!(pre("2pi"), "2*pi");
        assert_eq!(pre("3^100 mod 7"), "powmod(3,100,7)");
    }

    #[test]
    fn checks_like_the_ts() {
        let ctx = ScientificContext::default();
        assert_eq!(preprocess_checked("2+2", &ctx).as_deref(), Some("2+2"));
        assert_eq!(preprocess_checked("2:30", &ctx), None);
        assert_eq!(preprocess_checked("3!!", &ctx), None);
        assert_eq!(preprocess_checked("17 % 5", &ctx), None);
        assert_eq!(preprocess_checked("hello", &ctx), None);
        let with_ans = ScientificContext { ans: Some(0.5), ..ScientificContext::default() };
        assert_eq!(preprocess_checked("ans*2", &with_ans).as_deref(), Some("(0.5)*2"));
    }
}
