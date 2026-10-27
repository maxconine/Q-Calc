//! `src/engine/typstInput.ts`: Typst math typed or pasted (`$frac(1, 2)$`, `root(3, 8)`,
//! `integral_0^1 x^2 dif x`) rewritten into the ascii the rest of the engine evaluates. Ordinary calculator text
//! is returned unchanged.
//!
//! The TS walks UTF-16 units; this walks `char`s. Every test is on an ASCII character, and an astral character
//! comes through as itself where the TS copies its two halves one after the other, so both give the same text.
//!
//! Kept quirk-free in one place: the TS looks words up in a plain object (`OPS[word]`), so `constructor` or
//! `toString` typed in Typst math would print a JS function's source; here they stay words.

use crate::jsre::{self, Lazy};
use crate::jsstr;

static MARK: Lazy = Lazy::new(|| {
    jsre::js(
        &jsstr::spaces(
            r#"^\$(?:\$[\s\S]+\$|[^$\n]+)\$\s*$|(?:^|[^A-Za-z])(?:frac|binom|root|attach|lr|overline|underline|limits|scripts|upright|bold|italic|bb|cal|frak)\s*\(|\b(?:plus\.minus|minus\.plus|plusminus|arrow\.[a-z]|dot\.(?:c|op|circle)|dots(?:\.[a-z]+)?|eq\.(?:not|triple)|gt\.eq|lt\.eq|times(?:\.circle)?)\b|\b(?:dot|ast)\b(?!\s*[(.])|\bdiv\b|\bdif\b|\bintegral_|\bbase\s*:"#,
        ),
        "",
    )
});
// words in quotes are Typst text in math (`"isolate" R_("eq") "in" …`); calculator input never quotes
static QUOTED_TEXT: Lazy =
    Lazy::new(|| jsre::js(&jsstr::spaces(r#"(?:^|[\s(_])"[A-Za-z][^"\n]*""#), ""));
static LATEX_CMD: Lazy = Lazy::new(|| jsre::js(r"\\[a-zA-Z]+", ""));

pub fn looks_like_typst(text: &str) -> bool {
    let s = jsstr::trim(text);
    if s.is_empty() || jsre::test(&LATEX_CMD, s) {
        return false;
    }
    jsre::test(&MARK, s) || jsre::test(&QUOTED_TEXT, s)
}

fn op(word: &str) -> Option<&'static str> {
    Some(match word {
        "plus.minus" | "plusminus" | "pm" => "±",
        "minus.plus" => "∓",
        "times" | "times.circle" | "cdot" | "dot" | "dot.c" | "dot.op" | "dot.circle" | "ast"
        | "ast.op" | "star" | "cross" => "*",
        "div" => "/",
        "arrow.r" | "arrow.r.long" | "arrow.r.double" | "arrow.r.double.long" | "arrow.r.hook" => {
            "->"
        }
        "arrow.l.r" => "<->",
        "eq.not" | "neq" => "!=",
        "gt.eq" | "geq" => ">=",
        "lt.eq" | "leq" => "<=",
        "approx" => "≈",
        "dots" | "dots.h" | "dots.c" | "dots.v" | "dots.down" | "dots.up" | "ldots" | "cdots" => {
            "..."
        }
        "tan1" => "arctan",
        "prime" => "'",
        _ => return None,
    })
}

const GAP: [&str; 16] = [
    "thin",
    "med",
    "thick",
    "quad",
    "wide",
    "space",
    "space.quad",
    "space.en",
    "space.thin",
    "space.med",
    "space.thick",
    "space.nobreak",
    "neg",
    "nothing",
    "degree",
    "zws",
];

const UNWRAP: [&str; 13] = [
    "bold",
    "italic",
    "upright",
    "bb",
    "cal",
    "frak",
    "serif",
    "mono",
    "display",
    "inline",
    "limits",
    "scripts",
    "underline",
];

const OPAQUE: [&str; 3] = ["mat", "vec", "cases"];

/// Typst math as the ascii the rest of the engine evaluates; ordinary calculator text is returned unchanged.
pub fn typst_to_ascii(text: &str) -> String {
    if !looks_like_typst(text) {
        return text.to_string();
    }
    let mut s = jsstr::trim(text).to_string();
    let n = s.chars().count();
    if s.starts_with("$$") && s.ends_with("$$") && n > 4 {
        s = jsstr::trim(&s[2..s.len() - 2]).to_string();
    } else if s.starts_with('$') && s.ends_with('$') && n > 2 {
        s = jsstr::trim(&s[1..s.len() - 1]).to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    tidy(&leibniz(&emit(&chars, 0, chars.len())))
}

fn tidy(s: &str) -> String {
    macro_rules! re {
        ($p:expr) => {{
            static R: Lazy = Lazy::new(|| jsre::js(&jsstr::spaces($p), ""));
            &*R
        }};
    }
    let s = jsre::replace_all(re!(r"\s+"), s, |_| " ".into()).into_owned();
    let s = jsre::replace_all(re!(r"\s+\("), &s, |_| "(".into()).into_owned();
    let s = jsre::replace_all(re!(r"\(\s+"), &s, |_| "(".into()).into_owned();
    let s = jsre::replace_all(re!(r"\s+\)"), &s, |_| ")".into()).into_owned();
    let s = jsre::replace_all(re!(r"\s*([_^])\s*"), &s, |c| jsre::group(c, 1).into()).into_owned();
    jsstr::trim(&s).to_string()
}

/// `frac(dif, dif x)` is the `d/dx` the derivative parser already reads.
fn leibniz(s: &str) -> String {
    static A: Lazy = Lazy::new(|| {
        jsre::js(
            &jsstr::spaces(r"\(\(\s*d\s*\)\s*/\s*\(\s*d([A-Za-z])\s*\)\s*\)"),
            "",
        )
    });
    static B: Lazy = Lazy::new(|| {
        jsre::js(
            &jsstr::spaces(r"\(\s*d\s*\)\s*/\s*\(\s*d([A-Za-z])\s*\)"),
            "",
        )
    });
    let s = jsre::replace_all(&A, s, |c| format!("d/d{}", jsre::group(c, 1))).into_owned();
    jsre::replace_all(&B, &s, |c| format!("d/d{}", jsre::group(c, 1))).into_owned()
}

fn at(s: &[char], i: usize) -> Option<char> {
    s.get(i).copied()
}

fn text(s: &[char], from: usize, to: usize) -> String {
    let to = to.min(s.len());
    s[from.min(to)..to].iter().collect()
}

fn emit(s: &[char], from: usize, to: usize) -> String {
    let mut i = from;
    let mut out = String::new();
    let mut gap = false;
    while i < to {
        let start = i;
        while i < to && jsstr::is_space(s[i]) {
            i += 1;
        }
        if i > start {
            gap = true;
        }
        if i >= to {
            break;
        }
        if s[i] == '#' {
            i += 1;
            continue;
        }
        if s[i] == '\\' && i + 1 < to {
            i += 1;
            continue;
        }
        let Some((piece, end)) = read_piece(s, i, to) else {
            break;
        };
        if piece.is_empty() {
            gap = true;
        } else {
            let out_word = out
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == ')');
            let piece_word = piece
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '(');
            if gap && out_word && piece_word {
                out.push(' ');
            }
            out += &piece;
            gap = false;
        }
        i = end;
    }
    out
}

fn read_piece(s: &[char], i: usize, to: usize) -> Option<(String, usize)> {
    let ch = s[i];
    if ch == '"' {
        let end = (i + 1..s.len()).find(|&k| s[k] == '"');
        let stop = match end {
            Some(e) if e < to => e,
            _ => to,
        };
        return Some((text(s, i + 1, stop), end.map_or(to, |e| e + 1)));
    }
    if matches!(ch, '(' | '[' | '{') {
        let Some(close) = match_close(s, i, to) else {
            return Some((ch.to_string(), i + 1));
        };
        let inner = emit(s, i + 1, close);
        return Some((format!("({inner})"), close + 1));
    }
    if matches!(ch, ')' | ']' | '}') {
        return None;
    }
    if ch.is_ascii_digit() || (ch == '.' && at(s, i + 1).is_some_and(|c| c.is_ascii_digit())) {
        return Some(read_number(s, i, to));
    }
    if is_ident_start(Some(ch)) {
        return Some(read_word(s, i, to));
    }
    Some(read_symbol(s, i, to))
}

fn read_word(s: &[char], i: usize, to: usize) -> (String, usize) {
    let mut j = i + 1;
    while j < to && is_ident_part(at(s, j)) {
        j += 1;
    }
    while j < to && at(s, j) == Some('.') && is_ident_start(at(s, j + 1)) {
        j += 2;
        while j < to && is_ident_part(at(s, j)) {
            j += 1;
        }
    }
    let word = text(s, i, j);
    let mut k = j;
    while k < to && jsstr::is_space(s[k]) {
        k += 1;
    }
    if at(s, k) == Some('(')
        && let Some(close) = match_close(s, k, to).filter(|&c| c > 0)
    {
        return (call(&word, &text(s, k + 1, close)), close + 1);
    }
    if word == "dif" {
        if let Some((letter, end)) = single_letter(s, k, to) {
            return (format!("d{letter}"), end);
        }
        return ("d".into(), j);
    }
    if (word == "integral" || word == "integrate") && at(s, k) == Some('_') {
        return ("int".into(), j);
    }
    if GAP.contains(&word.as_str()) {
        return (String::new(), j);
    }
    if let Some(o) = op(&word) {
        return (o.into(), j);
    }
    (word, j)
}

fn single_letter(s: &[char], i: usize, to: usize) -> Option<(char, usize)> {
    let c = at(s, i).filter(char::is_ascii_alphabetic)?;
    let next = at(s, i + 1);
    if i + 1 < to && (is_ident_part(next) || next == Some('.')) {
        return None;
    }
    Some((c, i + 1))
}

fn call(name: &str, inner: &str) -> String {
    if OPAQUE.contains(&name) {
        return format!("{name}({inner})");
    }
    let args = split_args(inner);
    let arg = |k: usize| args.get(k).map_or("", String::as_str);
    match name {
        "frac" if args.len() == 2 => {
            return format!("(({})/({}))", emit_arg(arg(0)), emit_arg(arg(1)));
        }
        "binom" if args.len() == 2 => {
            return format!("combinations({},{})", emit_arg(arg(0)), emit_arg(arg(1)));
        }
        "root" if args.len() >= 2 => {
            return format!("nthroot({},{})", emit_arg(arg(1)), emit_arg(arg(0)));
        }
        "root" => return format!("sqrt({})", emit_arg(arg(0))),
        "lr" => return unwrap_lr(inner),
        "attach" => return attach(&args),
        "overline" => return overline(&args),
        "op" | "text" => return quoted(arg(0)),
        _ => {}
    }
    if UNWRAP.contains(&name) {
        return args
            .iter()
            .map(|a| emit_arg(a))
            .collect::<Vec<_>>()
            .join(", ");
    }
    let rendered: Vec<String> = args.iter().map(|a| emit_arg_named(a)).collect();
    format!("{name}({})", rendered.join(", "))
}

fn emit_arg(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    jsstr::trim(&emit(&chars, 0, chars.len())).to_string()
}

fn emit_arg_named(raw: &str) -> String {
    static NAMED: Lazy = Lazy::new(|| {
        jsre::js(
            &jsstr::spaces(r"^([A-Za-z][A-Za-z0-9]*)\s*:\s*([\s\S]+)$"),
            "",
        )
    });
    if let Ok(Some(m)) = NAMED.captures(jsstr::trim(raw))
        && jsre::group(&m, 1) == "base"
    {
        return emit_arg(jsre::group(&m, 2));
    }
    emit_arg(raw)
}

fn unwrap_lr(inner: &str) -> String {
    let t = jsstr::trim(inner);
    let n = t.chars().count();
    if t.starts_with('|') && t.ends_with('|') && n >= 2 {
        return format!("abs({})", emit_arg(&t[1..t.len() - 1]));
    }
    let wrapped = |l: &str, r: &str| t.starts_with(l) && t.ends_with(r);
    if wrapped("(", ")") || wrapped("[", "]") || wrapped(r"\{", r"\}") || wrapped("{", "}") {
        // JS `slice(1, -1)` on a one-character `(` is empty
        let body = if t.starts_with(r"\{") {
            t.get(2..t.len().saturating_sub(2)).unwrap_or("")
        } else {
            t.get(1..t.len().saturating_sub(1)).unwrap_or("")
        };
        return format!("({})", emit_arg(body));
    }
    format!("({})", emit_arg(t))
}

fn attach(args: &[String]) -> String {
    static SIDE: Lazy =
        Lazy::new(|| jsre::js(&jsstr::spaces(r"^(t|b|tl|tr|bl|br)\s*:\s*([\s\S]+)$"), ""));
    let base = emit_arg(args.first().map_or("", String::as_str));
    let mut top = String::new();
    let mut bot = String::new();
    for a in args.iter().skip(1) {
        let Ok(Some(m)) = SIDE.captures(jsstr::trim(a)) else {
            continue;
        };
        let v = emit_arg(jsre::group(&m, 2));
        match jsre::group(&m, 1) {
            "t" | "tr" | "tl" => top = v,
            _ => bot = v,
        }
    }
    let top = if top.is_empty() {
        String::new()
    } else {
        format!("^({top})")
    };
    let bot = if bot.is_empty() {
        String::new()
    } else {
        format!("_({bot})")
    };
    format!("{base}{top}{bot}")
}

fn overline(args: &[String]) -> String {
    let body = emit_arg(args.first().map_or("", String::as_str));
    if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
        return format!(r"\overline{{{body}}}");
    }
    format!("conj({body})")
}

fn quoted(raw: &str) -> String {
    let t = jsstr::trim(raw);
    if let Some(inner) = t.strip_prefix('"').and_then(|r| r.strip_suffix('"'))
        && !inner.contains('"')
    {
        return inner.to_string();
    }
    emit_arg(raw)
}

fn read_number(s: &[char], i: usize, to: usize) -> (String, usize) {
    let mut j = i;
    while j < to && (s[j].is_ascii_digit() || s[j] == '.') {
        j += 1;
    }
    if matches!(at(s, j), Some('e' | 'E')) && j + 1 < to {
        let mut k = j + 1;
        if matches!(at(s, k), Some('+' | '-')) {
            k += 1;
        }
        let exp = k;
        while k < to && s[k].is_ascii_digit() {
            k += 1;
        }
        if k > exp {
            j = k;
        }
    }
    (text(s, i, j), j)
}

fn read_symbol(s: &[char], i: usize, to: usize) -> (String, usize) {
    let three = text(s, i, to.min(i + 3));
    for (lead, put, len) in [
        ("...", "...", 3),
        ("->", "->", 2),
        ("=>", "->", 2),
        ("<->", "<->", 3),
        ("!=", "!=", 2),
        ("<=", "<=", 2),
        (">=", ">=", 2),
    ] {
        if three.starts_with(lead) {
            return (put.into(), i + len);
        }
    }
    let ch = s[i];
    let put = match ch {
        '×' | '·' | '⋅' => "*".to_string(),
        '÷' => "/".to_string(),
        _ => ch.to_string(),
    };
    (put, i + 1)
}

fn split_args(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut quote = false;
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '"' && (i == 0 || chars[i - 1] != '\\') {
            quote = !quote;
        } else if quote {
            continue;
        } else if matches!(ch, '(' | '[' | '{') {
            depth += 1;
        } else if matches!(ch, ')' | ']' | '}') {
            depth -= 1;
        } else if ch == ',' && depth == 0 {
            out.push(text(&chars, start, i));
            start = i + 1;
        }
    }
    out.push(text(&chars, start, chars.len()));
    out
}

fn match_close(s: &[char], open: usize, to: usize) -> Option<usize> {
    let first = s[open];
    let want = match first {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    let mut depth = 0;
    let mut quote = false;
    for i in open..to.min(s.len()) {
        let ch = s[i];
        if ch == '"' && (i == 0 || s[i - 1] != '\\') {
            quote = !quote;
        } else if quote {
            continue;
        } else if ch == first {
            depth += 1;
        } else if ch == want {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn is_ident_start(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

fn is_ident_part(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_typst() {
        assert!(looks_like_typst("$frac(1, 2)$"));
        assert!(looks_like_typst("root(3, 8)"));
        assert!(!looks_like_typst("2+3"));
        assert!(!looks_like_typst(r"\frac{1}{2}"));
    }

    #[test]
    fn converts_typst() {
        assert_eq!(typst_to_ascii("$frac(1, 2)$"), "((1)/(2))");
        assert_eq!(typst_to_ascii("root(3, 8)"), "nthroot(8,3)");
        assert_eq!(typst_to_ascii("2 dot 3"), "2*3");
        assert_eq!(
            typst_to_ascii("$10 for lunch + 15% tip"),
            "$10 for lunch + 15% tip"
        );
    }
}
