//! LaTeX input from `src/engine/plainMath.ts`: `looksLikeLatex` and `latexToAscii` with its helpers
//! (`replaceBraced`, `convertSqrts`, `convertLogs`, `bareTrigArguments`, `poweredCalls`, `repeatingDecimals`).
//!
//! The TS walks UTF-16 indices; this walks byte offsets of the same string. Every index it compares is an ASCII
//! marker (`\`, `{`, `(`, `[`, `_`, space), so both find the same places.

use num_bigint::BigInt;

use crate::jsre::{self, Lazy};
use crate::jsstr;
use crate::scientific::preprocess::{rewrite_typeset_mul, stitch_constants, wrap_bare_functions};

pub fn looks_like_latex(s: &str) -> bool {
    static RE: Lazy = Lazy::new(|| jsre::js(r"\\[a-zA-Z%]+|[\^_]\{|\\frac|\\sqrt", ""));
    jsre::test(&RE, s)
}

fn re(pattern: &str) -> jsre::Regex {
    jsre::js(&jsstr::spaces(pattern), "")
}

fn sub(s: &str, re: &jsre::Regex, with: &str) -> String {
    jsre::replace_all(re, s, |_| with.to_string()).into_owned()
}

/// `s.replace(re, template)` where the template uses `$1` (and `$2`) only.
fn sub_groups(s: &str, re: &jsre::Regex, f: impl Fn(&[&str]) -> String) -> String {
    jsre::replace_all(re, s, |c| {
        let groups: Vec<&str> = (0..c.len()).map(|i| jsre::group(c, i)).collect();
        f(&groups)
    })
    .into_owned()
}

/// LaTeX as the ascii the engine evaluates.
pub fn latex_to_ascii(latex: &str) -> String {
    macro_rules! lazy {
        ($p:expr) => {{
            static R: Lazy = Lazy::new(|| re($p));
            &*R
        }};
    }
    let mut s = jsstr::trim(latex).to_string();
    if s.is_empty() {
        return s;
    }
    s = s.replace(r"\left", "").replace(r"\right", "");
    s = sub(&s, lazy!(r"\\,|\\;|\\!|\\:|\\ "), "");
    s = sub(&s, lazy!(r"\\ldots|\\cdots|\\dots"), "...");
    s = rewrite_typeset_mul(&sub(&s, lazy!(r"\\cdot|\\times"), "*"));
    s = s.replace(r"\div", "/");
    // ± stays ±: the engine reads it as an uncertainty, or leaves the line blank
    s = sub(&s, lazy!(r"\\pm\b"), "±");
    s = sub(&s, lazy!(r"\\mp\b"), "∓");
    s = sub(&s, lazy!(r"\\pi\b"), "(pi)");
    s = sub(&s, lazy!(r"\\tau\b"), "(tau)");
    s = sub(&s, lazy!(r"\\infty\b"), "Infinity");
    s = sub(&s, lazy!(r"\\to\b"), "->");
    s = sub(&s, lazy!(r"\\exponentialE\b"), "(e)");
    s = s.replace(r"\mathrm{e}", "(e)");
    s = sub(&s, lazy!(r"\\imaginaryI\b|\\imaginaryJ\b"), "i");
    s = stitch_constants(&s);
    s = sub(&s, lazy!(r"\\theta\b"), "theta");
    s = sub(&s, lazy!(r"\\degree\b"), "");
    s = sub_groups(&s, lazy!(r"\\text\{([^}]*)\}"), |g| format!(" {} ", g[1]));
    s = sub_groups(&s, lazy!(r"\\operatorname\{([^}]+)\}"), |g| {
        g[1].to_string()
    });
    s = sub_groups(&s, lazy!(r"\\mathrm\{([^}]+)\}"), |g| g[1].to_string());
    s = sub(&s, lazy!(r"\\[dtc]frac\b"), r"\frac");
    s = replace_braced(&s, r"\frac", 2, |a| format!("(({})/({}))", a[0], a[1]));
    s = replace_braced(&s, r"\binom", 2, |a| {
        format!("combinations({},{})", a[0], a[1])
    });
    s = convert_sqrts(&s);
    s = replace_braced(&s, r"\abs", 1, |a| format!("abs({})", a[0]));
    s = sub(&s, lazy!(r"\\lfloor\s*"), "floor(").replace(r"\rfloor", ")");
    s = sub(&s, lazy!(r"\\lceil\s*"), "ceil(").replace(r"\rceil", ")");
    s = convert_logs(&s);
    s = sub_groups(&s, lazy!(r"\\ar(sinh|cosh|tanh)"), |g| format!("a{}", g[1]));
    s = sub_groups(&s, lazy!(r"\\(sinh|cosh|tanh|csch|sech|coth)\b"), |g| {
        g[1].to_string()
    });
    s = sub_groups(&s, lazy!(r"\\arc(sin|cos|tan|csc|sec|cot)\b"), |g| {
        format!("a{}", g[1])
    });
    s = sub_groups(
        &s,
        lazy!(r"\\(sin|cos|tan|csc|sec|cot)\s*\^\s*\{-1\}"),
        |g| format!("a{}", g[1]),
    );
    s = bare_trig_arguments(&s);
    // spaced, so `x\sin(…)` is x times sin, not one word
    s = sub_groups(&s, lazy!(r"\\(sin|cos|tan|csc|sec|cot)\b"), |g| {
        format!(" {}", g[1])
    });
    s = sub(&s, lazy!(r"\\exp\b"), "exp");
    s = repeating_decimals(&s);
    s = sub_groups(&s, lazy!(r"\\overline\{([^}]+)\}"), |g| {
        format!("conj({})", g[1])
    });
    s = sub_groups(&s, lazy!(r"\|([^|]+)\|"), |g| format!("abs({})", g[1]));
    s = sub_groups(&s, lazy!(r"\^\{([^{}]+)\}"), |g| format!("^({})", g[1]));
    // a subscript names a different thing (`x_{1}`, `10_{2}`), so it stays and the line goes blank
    s = sub_groups(&s, lazy!(r"_\{([^{}]+)\}"), |g| format!("_{}", g[1]));
    s = s.replace(r"\%", "%");
    s = s.replace(r"\$", "$");
    s = s.replace('\\', "");
    s = s.replace(['{', '}'], "");
    let spaced = sub(&s, lazy!(r"\s+"), " ");
    wrap_bare_functions(jsstr::trim(&spaced), &[])
}

const TRIG: &str = "sinh|cosh|tanh|csch|sech|coth|sin|cos|tan|csc|sec|cot";

/// A function written without brackets takes the next symbol or number, and a power written on the function is a
/// power of its value: `x\sin\theta` is x·sin(θ), `\cos^2\theta` is cos(θ)², `\sin 30` is sin(30).
fn bare_trig_arguments(s: &str) -> String {
    static BARE_TRIG: Lazy = Lazy::new(|| {
        re(&format!(
            r"\\({TRIG})\s*(?:\^\s*(\{{[^{{}}]*\}}|\d))?\s*(theta\b|\\[A-Za-z]+|[A-Za-z](?![A-Za-z])|\d+(?:\.\d+)?)"
        ))
    });
    let bare = jsre::replace_all(&BARE_TRIG, s, |c| {
        let pow = c.get(2).map(|m| m.as_str());
        let power = pow.map_or(String::new(), |p| {
            format!("^({})", p.replace(['{', '}'], ""))
        });
        let arg = jsre::group(c, 3);
        let arg = arg.strip_prefix('\\').unwrap_or(arg);
        format!(" {}({arg}){power}", jsre::group(c, 1))
    });
    powered_calls(&bare)
}

/// `\cos^2(60)` is cos(60)²: the power moves past the bracket it was written in front of.
fn powered_calls(s: &str) -> String {
    static RE: Lazy = Lazy::new(|| re(&format!(r"\\({TRIG})\s*\^\s*(\{{[^{{}}]*\}}|\d)\s*\(")));
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut at = 0;
    let mut from = 0;
    while let Ok(Some(m)) = RE.captures_from_pos(s, from) {
        let Some(whole) = m.get(0) else { break };
        from = whole.end();
        let open = whole.end() - 1;
        let mut depth = 0;
        let mut close = None;
        for (i, &b) in bytes.iter().enumerate().skip(open) {
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
        }
        let Some(close) = close else { continue };
        out += &format!(
            "{} {}{}^({})",
            &s[at..whole.start()],
            jsre::group(&m, 1),
            &s[open..=close],
            jsre::group(&m, 2).replace(['{', '}'], "")
        );
        at = close + 1;
        from = at;
    }
    out + &s[at..]
}

/// `0.1\overline{6}` is 0.1666…, as a fraction so no digits are lost.
fn repeating_decimals(s: &str) -> String {
    static RE: Lazy = Lazy::new(|| re(r"(?<![\d.])(\d*)\.(\d*)\\overline\{(\d+)\}"));
    jsre::replace_all(&RE, s, |c| {
        let (whole, fixed, rep) = (jsre::group(c, 1), jsre::group(c, 2), jsre::group(c, 3));
        let num = |t: &str| t.parse::<BigInt>().unwrap_or_default();
        let top = num(&format!("{fixed}{rep}")) - num(if fixed.is_empty() { "0" } else { fixed });
        let bottom = "9".repeat(rep.len()) + &"0".repeat(fixed.len());
        format!(
            "({}+{top}/{bottom})",
            if whole.is_empty() { "0" } else { whole }
        )
    })
    .into_owned()
}

fn skip_spaces(s: &[u8], mut p: usize) -> usize {
    while p < s.len() && s[p] == b' ' {
        p += 1;
    }
    p
}

fn convert_sqrts(src: &str) -> String {
    let mut s = src.to_string();
    let mut i = 0;
    while let Some(at) = s.get(i..).and_then(|t| t.find(r"\sqrt")).map(|k| k + i) {
        let b = s.as_bytes();
        let mut p = skip_spaces(b, at + 5);
        let mut n: Option<String> = None;
        if b.get(p) == Some(&b'[') {
            let Some(close) = s[p..].find(']').map(|k| k + p) else {
                i = at + 1;
                continue;
            };
            n = Some(s[p + 1..close].to_string());
            p = close + 1;
        }
        p = skip_spaces(b, p);
        if b.get(p) != Some(&b'{') {
            i = at + 1;
            continue;
        }
        let Some((inner, end)) = grab_group(&s, p, b'{', b'}') else {
            i = at + 1;
            continue;
        };
        // `n ? …` : an empty index is falsy
        let put = match n.filter(|n| !n.is_empty()) {
            Some(n) => format!("nthRoot({inner},{n})"),
            None => format!("sqrt({inner})"),
        };
        s = format!("{}{put}{}", &s[..at], &s[end..]);
        i = at + put.len();
    }
    s
}

fn convert_logs(src: &str) -> String {
    static LN_PAREN: Lazy = Lazy::new(|| re(r"\\ln\s*\(([^)]*)\)"));
    static LN: Lazy = Lazy::new(|| re(r"\\ln\b"));
    let s = replace_braced(src, r"\ln", 1, |a| format!("ln({})", a[0]));
    let s = sub_groups(&s, &LN_PAREN, |g| format!("ln({})", g[1]));
    let mut s = sub(&s, &LN, "ln");
    let mut i = 0;
    while let Some(at) = s.get(i..).and_then(|t| t.find(r"\log")).map(|k| k + i) {
        let b = s.as_bytes();
        let mut p = skip_spaces(b, at + 4);
        let mut base = "10".to_string();
        if b.get(p) == Some(&b'_') {
            p += 1;
            if b.get(p) == Some(&b'{') {
                if let Some((inner, end)) = grab_group(&s, p, b'{', b'}') {
                    base = inner;
                    p = end;
                }
            } else if let Some(c) = s.get(p..).and_then(|t| t.chars().next()) {
                base = c.to_string();
                p += c.len_utf8();
            }
        }
        p = skip_spaces(b, p);
        let mut arg: Option<String> = None;
        let grabbed = match b.get(p) {
            Some(b'{') => grab_group(&s, p, b'{', b'}'),
            Some(b'(') => grab_group(&s, p, b'(', b')'),
            _ => None,
        };
        if let Some((inner, end)) = grabbed {
            arg = Some(inner);
            p = end;
        }
        let put = match (arg, base == "10") {
            (None, true) => "log10".to_string(),
            (None, false) => format!("log(_,{base})"),
            (Some(a), true) => format!("log10({a})"),
            (Some(a), false) => format!("log({a},{base})"),
        };
        s = format!("{}{put}{}", &s[..at], &s[p..]);
        i = at + put.len();
    }
    s
}

fn replace_braced(
    src: &str,
    cmd: &str,
    arity: usize,
    build: impl Fn(&[String]) -> String,
) -> String {
    let mut s = src.to_string();
    let mut i = 0;
    while let Some(at) = s.get(i..).and_then(|t| t.find(cmd)).map(|k| k + i) {
        let mut p = at + cmd.len();
        let mut args = Vec::new();
        let mut ok = true;
        for _ in 0..arity {
            p = skip_spaces(s.as_bytes(), p);
            match grab_group(&s, p, b'{', b'}') {
                Some((inner, end)) => {
                    args.push(inner);
                    p = end;
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            i = at + 1;
            continue;
        }
        let put = build(&args);
        s = format!("{}{put}{}", &s[..at], &s[p..]);
        // an argument can hold the same command (`\frac{\frac{1}{2}}{2}`)
        i = at;
    }
    s
}

/// The group opening at byte `open`: its inside and the byte after its close.
fn grab_group(s: &str, open: usize, left: u8, right: u8) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if b.get(open) != Some(&left) {
        return None;
    }
    let mut depth = 0;
    for (i, &c) in b.iter().enumerate().skip(open) {
        if c == left {
            depth += 1;
        } else if c == right {
            depth -= 1;
            if depth == 0 {
                return Some((s[open + 1..i].to_string(), i + 1));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_latex() {
        assert!(looks_like_latex(r"\frac{1}{2}"));
        assert!(looks_like_latex("x^{2}"));
        assert!(!looks_like_latex("x^2"));
    }

    #[test]
    fn converts_latex() {
        assert_eq!(latex_to_ascii(r"\frac{1}{2}"), "((1)/(2))");
        assert_eq!(latex_to_ascii(r"\sqrt[3]{8}"), "nthRoot(8,3)");
        assert_eq!(latex_to_ascii(r"\log_2{8}"), "log(8,2)");
        assert_eq!(latex_to_ascii(r"0.1\overline{6}"), "(0+15/90)");
    }
}
