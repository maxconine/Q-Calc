//! `src/lib/typstMath.ts`: the typed line as Typst math, for the typeset preview (`typstDocument`, `previewMath`)
//! and for copy (`toTypstMath`, `copiedEquation`), and whether the preview earns its place.
//!
//! The TS walks UTF-16 units; this walks `char`s. Every test is on an ASCII or BMP character, and an astral
//! character is either printed whole (a letter or symbol) or dropped whole in both, so the output is the same.
//! The limits of `∫_a^b` come from `bounds`, which counts UTF-16 units, so that step converts.

use std::sync::LazyLock;

use crate::app::bounds::{U16, bounds_in_units};
use crate::jsre::{self, Lazy};
use crate::jsstr;

/// Macros available to the typeset preview (`/macros.typ`).
pub const TYPST_PREAMBLE: &str = r##"#let choose = "choose"
#let multichoose = "multichoose"
#let multibinom(upper, ..lower) = {
  let helper = math.binom(upper, ..lower)
  $(helper)$
}
#let amat(..rows) = {
  math.mat(..rows, augment: -1)
}

#let emat(..rows) = {
  math.mat(..rows, delim: "(")
}

#let dmat(..rows) = {
  math.mat(..rows, delim: "|")
}

#let tan1 = $tan^(-1)$

#let dt = $space d t$
#let dtau = $space d tau$

#let bu = $bold(u)$
#let bv = $bold(v)$
#let bw = $bold(w)$
#let b0 = $bold(0)$
#let bx = $bold(x)$
#let bP = $bold(P)$
#let bF = $bold(F)$
#let bz = $bold(z)$
#let ba = $bold(a)$
#let bc = $bold(c)$

#let lp = $cal(L)$

#let pm = $plus.minus$

#let proj = $text("proj")$
#let span = $op("span")$
#let null = $op("null")$
#let row = $op("row")$
#let col = $op("col")$
#let rank = $op("rank")$
#let range = $op("range")$
#let nullity = $op("nullity")$
#let cov = $op("cov")$

#let sa = $stretch(->)$
#let ssa = $stretch(=>)$
#let se = $stretch(=)$

#let dd = math.dot.double

#let dx = $space d x$

#let ibar(content) = $lr(#content space |)$

#let tand = $space.quad text("and") space.quad$

#let imod(n) = $space (mod #n)$
#let ctimes = $times.o$
#let cplus = $plus.o$
#let cminus = $minus.o$

#let circ = math.op($circle.small$)

#let cC = $cal(C)$
#let cD = $cal(D)$
#let cB = $cal(B)$
#let cP = $cal(P)$

#let ip(x, y) = $lr(chevron.l #x, #y chevron.r)$
#let ict = math.op("ict")
#let cross = $times$
"##;

const CONTENT: &[&str] = &[
    "choose",
    "multichoose",
    "tan1",
    "dt",
    "dtau",
    "bu",
    "bv",
    "bw",
    "b0",
    "bx",
    "bP",
    "bF",
    "bz",
    "ba",
    "bc",
    "lp",
    "pm",
    "proj",
    "span",
    "null",
    "row",
    "col",
    "rank",
    "range",
    "nullity",
    "cov",
    "sa",
    "ssa",
    "se",
    "dd",
    "dx",
    "tand",
    "ctimes",
    "cplus",
    "cminus",
    "circ",
    "cC",
    "cD",
    "cB",
    "cP",
    "ict",
    "cross",
];

const CALLS: [&str; 7] = ["multibinom", "amat", "emat", "dmat", "ibar", "imod", "ip"];

// Typst looks a bare multi-letter word up as a variable, and an unknown one fails the whole preview. These are
// the names it knows; any other word is written upright in quotes.
const GREEK: [&str; 24] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi",
    "psi", "omega",
];

const OTHER_NAMES: &[&str] = &[
    "arccos", "arcsin", "arctan", "arg", "cos", "cosh", "cot", "coth", "csc", "csch", "ctg", "deg",
    "det", "dim", "exp", "gcd", "lcm", "hom", "id", "im", "inf", "ker", "lg", "lim", "liminf",
    "limsup", "ln", "log", "max", "min", "mod", "Pr", "sec", "sech", "sin", "sinc", "sinh", "sup",
    "tan", "tanh", "tg",
];

const TYPST_FUNCTIONS: [&str; 11] = [
    "sqrt", "root", "abs", "norm", "floor", "ceil", "round", "binom", "frac", "vec", "mat",
];

fn capitalized(g: &str) -> String {
    let mut c = g.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

fn is_content(w: &str) -> bool {
    CONTENT.contains(&w)
}

fn is_typst_name(w: &str) -> bool {
    GREEK.contains(&w) || GREEK.iter().any(|g| capitalized(g) == w) || OTHER_NAMES.contains(&w)
}

fn inverse(key: &str) -> Option<&'static str> {
    Some(match key {
        "asin" | "arcsin" => "arcsin",
        "acos" | "arccos" => "arccos",
        "atan" | "arctan" => "tan1",
        "arccsc" => r#"op("arccsc")"#,
        "arcsec" => r#"op("arcsec")"#,
        "arccot" => r#"op("arccot")"#,
        _ => return None,
    })
}

/// Exact form when the engine has one, otherwise the displayed answer.
pub fn typst_answer(exact: Option<&str>, display: &str) -> String {
    let pretty = exact.map_or("", jsstr::trim);
    let shown = jsstr::trim(display);
    if !pretty.is_empty() && !shown.is_empty() && pretty != shown {
        return format!("{pretty} ≈ {shown}");
    }
    if pretty.is_empty() { shown } else { pretty }.to_string()
}

fn re(pattern: &str, flags: &str) -> jsre::Regex {
    // `s` (dotAll) has no `jsre::js` flag; it's the inline `(?s)`
    let dot_all = if flags.contains('s') { "(?s)" } else { "" };
    jsre::js(
        &format!("{dot_all}{}", jsstr::spaces(pattern)),
        &flags.replace('s', ""),
    )
}

macro_rules! lazy {
    ($p:expr) => {
        lazy!($p, "")
    };
    ($p:expr, $f:expr) => {{
        static R: Lazy = Lazy::new(|| re($p, $f));
        &*R
    }};
}

fn caps<'t>(r: &jsre::Regex, s: &'t str) -> Option<jsre::Caps<'t>> {
    r.captures(s).ok().flatten()
}

fn opt<'t>(c: &jsre::Caps<'t>, i: usize) -> Option<&'t str> {
    c.get(i).map(|m| m.as_str())
}

// an answer's unit is upright text like the units typed (`69.8 L`, not an italic variable L)
fn answer_math(answer: &str) -> String {
    let r = lazy!(r"^([+\-−]?[\d.,]+(?:e[+\-−]?\d+)?(?:\s*±\s*[\d.,]+)?)\s+([A-Za-z°µμΩÅ][^\s]*)$");
    match caps(r, jsstr::trim(answer)) {
        Some(m) => format!(
            "{} {}",
            to_typst_math(jsre::group(&m, 1)),
            serde_json::to_string(jsre::group(&m, 2)).unwrap_or_default()
        ),
        None => to_typst_math(answer),
    }
}

/// The preview document. Glyphs are black, and the renderer swaps black for currentColor, so the page's own text
/// color (light or dark) applies without compiling again. `solved_for` names the variable of a solved equation:
/// `x^2 = 4 ⇒ x = ±2`, not `x^2 = 4 = ±2`.
pub fn typst_document(expr: &str, answer: &str, solved_for: &str) -> Option<String> {
    let math = preview_math(expr);
    if math.is_empty() {
        return None;
    }
    let ans = answer_math(answer);
    let lhs = to_typst_math(solved_for);
    let body = if !ans.is_empty() && !lhs.is_empty() {
        format!("{math} quad arrow.r.double quad {lhs} = {ans}")
    } else if !ans.is_empty() && jsre::test(lazy!(r"(^|[^<>!])="), answer) {
        format!("{math} quad arrow.r.double quad {ans}")
    } else if !ans.is_empty() {
        format!("{math} = {ans}")
    } else {
        math
    };
    Some(format!(
        "#import \"/macros.typ\": *\n#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(size: 18pt, fill: black)\n$ {body} $\n"
    ))
}

// names that typeset differently from how they're typed: a function set upright with its own spacing, a big
// operator, a root, a matrix. greek letters, e, i and the plain operators (ast, dot, times) don't count
fn is_typeset_name(w: &str) -> bool {
    const EXTRA: [&str; 10] = [
        "op",
        "integral",
        "sum",
        "product",
        "lim",
        "tan1",
        "choose",
        "multichoose",
        "dif",
        "partial",
    ];
    if GREEK.contains(&w) || GREEK.iter().any(|g| capitalized(g) == w) {
        return false;
    }
    is_typst_name(w) || TYPST_FUNCTIONS.contains(&w) || CALLS.contains(&w) || EXTRA.contains(&w)
}

/// Whether the preview earns its place for `expr`: only when the typeset math looks different from the typed
/// text. Words (units, dates, money, commands) come out as upright quoted text and hide it; so does plain
/// arithmetic, which would only say again what the bar says. Fractions, powers, subscripts, factorials, roots,
/// big operators, matrices and named functions show it.
pub fn typst_preview_useful(expr: &str) -> bool {
    // ± reads the same typeset as in the bar; left as +/- its slash would count as a fraction
    let typed = jsre::replace_all(lazy!(r"\+/-|-/\+"), expr, |_| "±".into()).into_owned();
    let math = preview_math(&typed);
    if math.is_empty() {
        return false;
    }
    // op("arccsc") and a matrix's delim: "[" are the quoted text that isn't a word
    // a quoted unit (`"Nm"`, `"Pa"`) is still math; any other quoted word (`"to"`, a date, money) hides it
    let bare = jsre::replace_all(lazy!(r#"op\("[^"]*"\)"#), &math, |_| "op".into()).into_owned();
    let bare = jsre::replace_all(lazy!(r#"delim: "[^"]*""#), &bare, |_| String::new()).into_owned();
    let bare = jsre::replace_all(lazy!(r#""([^"]*)""#), &bare, |c| {
        if crate::units::reads_as_unit(jsre::group(c, 1)) {
            "unit".into()
        } else {
            jsre::group(c, 0).into()
        }
    })
    .into_owned();
    if bare.contains('"') {
        return false;
    }
    // a clock time (`3:45pm + 4 hr`) reads fine as typed
    if jsre::test(lazy!(r"\d\s*:\s*\d"), &bare) {
        return false;
    }
    // an integral, sum, limit, derivative or solve line always shows: its answer is what the bar can't typeset
    if is_keyword_math(jsstr::trim(&typed)) {
        return true;
    }
    if simple_math(&bare) {
        return false;
    }
    // a / ^ or _ still waiting for what follows it isn't a fraction or a power yet
    if jsre::test(lazy!(r"!|[/^_](?=\s*[^\s)])"), &bare) {
        return true;
    }
    // a word right after a number is its unit (`10 min`), not a call (`min(3, 4)`)
    let names = jsre::replace_all(lazy!(r"(\d\s+)[A-Za-z]+\b(?!\s*\()"), &bare, |c| {
        format!("{}unit", jsre::group(c, 1))
    });
    lazy!(r"[A-Za-z][A-Za-z0-9.]*")
        .find_iter(names.as_ref())
        .flatten()
        .any(|m| is_typeset_name(m.as_str().split('.').next().unwrap_or("")))
}

// a number, or `ans` standing in for one
const PLAIN_NUMBER: &str = r"-?\s*(?:\d+(?:\.\d*)?|\.\d+|ans)";

/// `3.5/2` or `sin(30)`: one division of two numbers, or one function of a number, reads fine in the bar as typed.
fn simple_math(math: &str) -> bool {
    static DIVISION: LazyLock<jsre::Regex> =
        LazyLock::new(|| re(&format!(r"^{PLAIN_NUMBER}\s*/\s*{PLAIN_NUMBER}$"), ""));
    static CALL: LazyLock<jsre::Regex> = LazyLock::new(|| {
        re(
            &format!(r"^([A-Za-z][A-Za-z0-9]*)\s*\(\s*{PLAIN_NUMBER}\s*\)$"),
            "",
        )
    });
    let s = jsstr::trim(math);
    if jsre::test(&DIVISION, s) {
        return true;
    }
    // a root still looks different typeset, so it isn't simple
    caps(&CALL, s).is_some_and(|c| !matches!(jsre::group(&c, 1), "sqrt" | "root"))
}

/// The preview's math: the typed line as Typst, with the calculator's keyword forms written as the math they
/// mean (`int 0..1 x^2 dx` is ∫₀¹, `solve x^2 = 4` is the equation, `d/dx x^3` is a d/dx fraction). Copy keeps
/// `to_typst_math`, so what's pasted still says what was typed.
pub fn preview_math(expr: &str) -> String {
    // a line still being typed previews as the input shows it, with its faint parens closed
    let s = crate::parens::fill_parens(jsstr::trim(expr));
    if let Some(d) = derivative_math(&s) {
        return d;
    }
    to_typst_math(&keyword_math(&s))
}

/// The keyword forms the calculator reads, respelled with the limits and signs `to_typst_math` typesets.
fn keyword_math(s: &str) -> String {
    let int_range = lazy!(r"^(?:int|integral|∫)\s*(\S+?)\s*\.\.\s*(\S+)\s+(.+)$", "is");
    let int_words = lazy!(
        r"^(?:(?:the\s+)?integral\s+of|integrate)\s+(.+?)\s+from\s+(\S+)\s+to\s+(\S+)$",
        "is"
    );
    let sum_words = lazy!(
        r"^(sum|prod|product)\s+(?:of\s+)?(.+?)\s+from\s+(\S+)\s+to\s+(\S+)$",
        "is"
    );
    let lim_arrow = lazy!(
        r"^lim(?:it)?\s+([A-Za-z]|theta)\s*(?:->|→)\s*(\S+)\s+(.+)$",
        "is"
    );
    let lim_words = lazy!(
        r"^(?:the\s+)?limit\s+of\s+(.+?)\s+as\s+([A-Za-z]|theta)\s*(?:->|→|approaches)\s*(\S+)$",
        "is"
    );
    let solve_for = lazy!(
        r"^(?:solve|isolate)\s+(?:for\s+)?[A-Za-z]\w*\s*(?::|in\b)\s*(.+)$",
        "is"
    );
    let solve_tail = lazy!(
        r"^(?:solve\s+)?(.+?=.+?)\s+(?:for|isolate)\s+[A-Za-z]\w*$",
        "is"
    );
    let solve_head = lazy!(r"^solve\s+(.+=.+)$", "is");
    // a solve still being typed (`solve 6 (3 / (3 + R)`, or one with its = but no right side yet) previews its
    // equation so far
    let solve_open = lazy!(r"^solve\s+(?!(?:for|isolate)\b)(.+)$", "is");
    if let Some(m) = caps(int_range, s) {
        return format!(
            "∫_{{{}}}^{{{}}} {}",
            jsre::group(&m, 1),
            jsre::group(&m, 2),
            jsre::group(&m, 3)
        );
    }
    if let Some(m) = caps(int_words, s) {
        let body = jsre::group(&m, 1);
        let dif = if jsre::test(lazy!(r"\bd[A-Za-z]\s*$"), body) {
            String::new()
        } else {
            format!(" d{}", index_of(body, "x"))
        };
        return format!(
            "∫_{{{}}}^{{{}}} {body}{dif}",
            jsre::group(&m, 2),
            jsre::group(&m, 3)
        );
    }
    if let Some(m) = caps(sum_words, s) {
        let sign = if jsre::group(&m, 1).to_lowercase().starts_with("prod") {
            "Π"
        } else {
            "Σ"
        };
        let body = jsre::group(&m, 2);
        return format!(
            "{sign}_{{{}={}}}^{{{}}} {body}",
            index_of(body, "n"),
            jsre::group(&m, 3),
            jsre::group(&m, 4)
        );
    }
    if let Some(m) = caps(lim_arrow, s) {
        return format!(
            "lim_{{{}->{}}} {}",
            jsre::group(&m, 1),
            jsre::group(&m, 2),
            jsre::group(&m, 3)
        );
    }
    if let Some(m) = caps(lim_words, s) {
        return format!(
            "lim_{{{}->{}}} {}",
            jsre::group(&m, 2),
            jsre::group(&m, 3),
            jsre::group(&m, 1)
        );
    }
    for r in [solve_for, solve_tail, solve_head, solve_open] {
        if let Some(m) = caps(r, s) {
            return jsre::group(&m, 1).to_string();
        }
    }
    s.to_string()
}

fn is_keyword_math(s: &str) -> bool {
    derivative_math(s).is_some() || keyword_math(s) != s
}

/// The variable a sum or integral runs over: the usual one if it's there, else the body's only letter.
fn index_of(body: &str, usual: &str) -> String {
    let mut letters: Vec<&str> = Vec::new();
    for m in lazy!(r"(?<![A-Za-z])[A-Za-z](?![A-Za-z(])")
        .find_iter(body)
        .flatten()
    {
        if !letters.contains(&m.as_str()) {
            letters.push(m.as_str());
        }
    }
    if letters.contains(&usual) {
        return usual.into();
    }
    for v in ["n", "k", "i", "j", "x", "t"] {
        if letters.contains(&v) {
            return v.into();
        }
    }
    if letters.len() == 1 {
        letters[0].into()
    } else {
        usual.into()
    }
}

/// `d/dx x^3 at 2` as a d/dx fraction in front of the body, evaluated at the point when there is one.
fn derivative_math(s: &str) -> Option<String> {
    let d_dx = lazy!(
        r"^d(?:\^?([2-9]|[²³]))?\s*/\s*d([A-Za-z])(?:\^?([2-9]|[²³]))?\s+(.+)$",
        "s"
    );
    let d_words = lazy!(
        r"^(?:the\s+)?(?:(second|2nd)\s+)?derivative\s+of\s+(.+?)(?:\s+(?:with\s+respect\s+to|wrt)\s+([A-Za-z]))?$",
        "is"
    );
    let (order, v, rest) = if let Some(m) = caps(d_dx, s) {
        let order = opt(&m, 1)
            .or_else(|| opt(&m, 3))
            .and_then(order_number)
            .unwrap_or(1);
        (
            order,
            jsre::group(&m, 2).to_string(),
            jsre::group(&m, 4).to_string(),
        )
    } else {
        let m = caps(d_words, s)?;
        let rest = jsre::group(&m, 2).to_string();
        let v = match opt(&m, 3) {
            Some(v) => v.to_string(),
            None => {
                let body = jsre::replace_all(lazy!(r"\s+at\s+.+$", "i"), &rest, |_| String::new());
                index_of(&body, "x")
            }
        };
        (if opt(&m, 1).is_some() { 2 } else { 1 }, v, rest)
    };
    let at = caps(
        lazy!(r"^(.+?)\s+at\s+(?:[A-Za-z]\s*=\s*)?(.+)$", "is"),
        &rest,
    );
    let body = to_typst_math(at.as_ref().map_or(rest.as_str(), |a| jsre::group(a, 1)));
    if body.is_empty() {
        return None;
    }
    let d = if order == 1 {
        format!("frac(dif, dif {v})")
    } else {
        format!("frac(dif^{order}, dif {v}^{order})")
    };
    let point = at
        .as_ref()
        .map_or(String::new(), |a| to_typst_math(jsre::group(a, 2)));
    // only a sum needs parens to stay under the d/dx: d/dx x^3, but d/dx (x^2 + 1)
    let inner = if top_level_sum(&body) {
        format!("({body})")
    } else {
        body
    };
    Some(if point.is_empty() {
        format!("{d} {inner}")
    } else {
        format!("lr({d} {inner} |)_({v} = {point})")
    })
}

fn top_level_sum(math: &str) -> bool {
    let chars: Vec<char> = math.chars().collect();
    let mut depth = 0;
    for (i, &ch) in chars.iter().enumerate() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if depth == 0 && i > 0 && matches!(ch, '+' | '-' | '±') && chars[i - 1] == ' ' {
            return true;
        }
    }
    false
}

fn order_number(raw: &str) -> Option<u32> {
    match raw {
        "" => None,
        "²" => Some(2),
        "³" => Some(3),
        _ => raw.parse().ok(),
    }
}

/// Search-bar copy: Typst math when the setting is on, otherwise the typed text.
pub fn copied_equation(text: &str, typst_copy: bool) -> String {
    if !typst_copy {
        return text.to_string();
    }
    let t = to_typst_math(text);
    if t.is_empty() { text.to_string() } else { t }
}

pub fn to_typst_math(expr: &str) -> String {
    let mut s = jsstr::trim(expr).to_string();
    if s.is_empty() {
        return s;
    }
    if jsre::test(lazy!(r"\\[a-zA-Z]+"), &s) {
        s = crate::latex::latex_to_ascii(&s);
    }
    let chars: Vec<char> = s.chars().collect();
    jsstr::trim(&emit(&chars, 0, chars.len())).to_string()
}

type Piece = (String, usize);

fn sl(s: &[char], from: usize, to: usize) -> String {
    let to = to.min(s.len());
    s[from.min(to)..to].iter().collect()
}

fn at(s: &[char], i: usize) -> Option<char> {
    s.get(i).copied()
}

fn emit_str(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    emit(&chars, 0, chars.len())
}

fn emit(s: &[char], from: usize, to: usize) -> String {
    let mut i = from;
    let mut parts: Vec<String> = Vec::new();
    while i < to {
        while i < to && jsstr::is_space(s[i]) {
            i += 1;
        }
        if i >= to {
            break;
        }
        let Some((text, end)) = read_piece(s, i, to) else {
            break;
        };
        if end <= i {
            break;
        }
        i = end;
        if text.is_empty() {
            continue;
        }
        match parts.last_mut() {
            Some(last) if text.starts_with("_(") => *last += &text,
            _ => parts.push(text),
        }
    }
    parts.join(" ")
}

fn is_big_sign(c: char) -> bool {
    "Σ∑Π∏∫".contains(c)
}

fn read_piece(s: &[char], i: usize, to: usize) -> Option<Piece> {
    let ch = s[i];
    if matches!(ch, '(' | '[' | '{') {
        if ch == '['
            && let Some(grid) = read_matrix(s, i, to)
        {
            return Some(grid);
        }
        let Some(close) = match_close(s, i, to) else {
            // still being typed: an open bracket shows on its own
            let text = if ch == '(' {
                "(".to_string()
            } else {
                format!("\\{ch}")
            };
            return Some((text, i + 1));
        };
        let inner = emit(s, i + 1, close);
        let wrap = match ch {
            '(' => format!("({inner})"),
            '[' => format!("lr(\\[{inner}\\])"),
            _ => format!("lr(\\{{{inner}\\}})"),
        };
        return Some((wrap, close + 1));
    }
    // a stray closer is printed, and what follows it still is
    match ch {
        ')' => return Some((")".into(), i + 1)),
        ']' | '}' => return Some((format!("\\{ch}"), i + 1)),
        ',' => return Some((",".into(), i + 1)),
        '_' => return Some(read_subscript(s, i, to)),
        '√' | '∛' => return Some(read_radical(s, i, to)),
        _ => {}
    }
    if is_ident_start(Some(ch)) || is_big_sign(ch) {
        return Some(read_word(s, i, to));
    }
    if ch.is_ascii_digit() || (ch == '.' && at(s, i + 1).is_some_and(|c| c.is_ascii_digit())) {
        return Some(read_number(s, i, to));
    }
    Some(read_symbol(s, i, to))
}

const MATRIX_WORDS: [&str; 9] = [
    "inv",
    "inverse",
    "transpose",
    "det",
    "determinant",
    "trace",
    "tr",
    "rank",
    "rref",
];
const MATRIX_FNS: [&str; 13] = [
    "inv",
    "inverse",
    "transpose",
    "det",
    "determinant",
    "trace",
    "tr",
    "rank",
    "rref",
    "identity",
    "eye",
    "dot",
    "cross",
];

fn big_word(text: &str) -> Option<jsre::Caps<'_>> {
    caps(
        lazy!(r"^(sum|prod|product|int|integral)(?![A-Za-z])", "i"),
        text,
    )
}

fn read_word(s: &[char], i: usize, to: usize) -> Piece {
    let glyph = s[i];
    if (is_big_sign(glyph) || big_word(&sl(s, i, to)).is_some())
        && let Some(big) = read_big_op(s, i, to)
    {
        return big;
    }
    let mut j = i + 1;
    if is_ident_start(Some(glyph)) {
        while j < to && is_ident_part(at(s, j)) {
            j += 1;
        }
    }
    let word = sl(s, i, j);
    let mut k = j;
    while k < to && jsstr::is_space(s[k]) {
        k += 1;
    }
    // `det [[1,2],[3,4]]`, `inverse of A`
    let key = word.to_lowercase();
    if MATRIX_WORDS.contains(&key.as_str()) && k > j && at(s, k) != Some('(') {
        let rest = sl(s, k, to);
        let of =
            caps(lazy!(r"^of\s+", "i"), &rest).map_or(0, |m| jsre::group(&m, 0).chars().count());
        let a = k + of;
        let arg = (a < to && at(s, a).is_some_and(|c| c == '[' || c.is_ascii_alphabetic()))
            .then(|| read_piece(s, a, to))
            .flatten();
        if let Some((arg_text, end)) = arg
            && !arg_text.is_empty()
            && let Some(text) = matrix_fn(&key, &[arg_text])
        {
            return (text, end);
        }
    }
    // `or (3 > 4)` is the operator before a group, not a call of `or`
    if matches!(key.as_str(), "and" | "or") {
        return (bare_name(&word), j);
    }
    if at(s, k) == Some('(')
        && CALLS.contains(&word.as_str())
        && let Some(close) = match_close(s, k, to).filter(|&c| c > 0)
    {
        return (call_macro(&word, &sl(s, k + 1, close)), close + 1);
    }
    if at(s, k) == Some('(')
        && let Some(close) = match_close(s, k, to).filter(|&c| c > 0)
    {
        return (call_fn(&word, &sl(s, k + 1, close)), close + 1);
    }
    (bare_name(&word), j)
}

fn call_macro(name: &str, inner: &str) -> String {
    if matches!(name, "amat" | "emat" | "dmat" | "multibinom") {
        let rows: Vec<String> = split_top(inner, ';')
            .iter()
            .map(|row| {
                split_top(row, ',')
                    .iter()
                    .map(|cell| or_space(emit_str(cell)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        return format!("#{name}({})", rows.join("; "));
    }
    let args: Vec<String> = split_top(inner, ',')
        .iter()
        .map(|a| {
            let math = emit_str(a);
            if math.is_empty() {
                "$ $".into()
            } else {
                format!("${math}$")
            }
        })
        .collect();
    format!("#{name}({})", args.join(", "))
}

fn or_space(s: String) -> String {
    if s.is_empty() { " ".into() } else { s }
}

fn call_fn(word: &str, inner: &str) -> String {
    let key = word.to_lowercase();
    let args: Vec<String> = split_top(inner, ',').iter().map(|a| emit_str(a)).collect();
    let arg = |k: usize| args.get(k).map_or("", String::as_str);
    if MATRIX_FNS.contains(&key.as_str())
        && let Some(m) = matrix_fn(&key, &args)
    {
        return m;
    }
    match key.as_str() {
        "sqrt" => return format!("sqrt({})", arg(0)),
        "cbrt" => return format!("root(3, {})", arg(0)),
        "nthroot" if args.len() >= 2 => return format!("root({}, {})", arg(1), arg(0)),
        "ncr" | "combinations" => return format!("binom({}, {})", arg(0), arg(1)),
        "npr" | "permutations" => return format!("attach(P, bl: {}, br: {})", arg(0), arg(1)),
        "log2" => return format!("log_2({})", arg(0)),
        "log10" | "log" => return format!("log({})", arg(0)),
        "ln" => return format!("ln({})", arg(0)),
        "abs" => return format!("abs({})", arg(0)),
        _ => {}
    }
    match inverse(&key) {
        Some("tan1") => return format!("(#tan1)({})", args.join(", ")),
        Some(inv) => return format!("{inv}({})", args.join(", ")),
        None => {}
    }
    let name = if TYPST_FUNCTIONS.contains(&word) {
        word.to_string()
    } else {
        math_name(word)
    };
    // a quoted name would be text, which reads as a product; op keeps f-of-x spacing
    let head = if name.starts_with('"') {
        format!("op({name})")
    } else {
        name
    };
    format!("{head}({})", args.join(", "))
}

fn bare_name(word: &str) -> String {
    if is_content(word) {
        return format!("#{word}");
    }
    let key = word.to_lowercase();
    // `CONTENT.has(key) && key === word` is the case above again
    match key.as_str() {
        "dot" => return "dot.op".into(),
        "ans" => return "\"ans\"".into(),
        // the logical words as their symbols: ∧ and ∨
        "and" => return "and".into(),
        "or" => return "or".into(),
        "pi" => return "pi".into(),
        "theta" => return "theta".into(),
        "tau" => return "tau".into(),
        "infinity" | "infty" | "inf" => return "infinity".into(),
        _ => {}
    }
    match word {
        "π" => return "pi".into(),
        "θ" => return "theta".into(),
        "∞" => return "infinity".into(),
        _ => {}
    }
    match inverse(&key) {
        Some("tan1") => return "(#tan1)".into(),
        Some(inv) => return inv.into(),
        None => {}
    }
    if key == "sqrt" {
        return "sqrt".into();
    }
    math_name(word)
}

fn math_name(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    if (chars.len() == 1 && chars[0].is_ascii_alphabetic()) || is_typst_name(word) {
        return word.to_string();
    }
    // x1 is x sub 1
    if chars.len() > 1
        && chars[0].is_ascii_alphabetic()
        && chars[1..].iter().all(char::is_ascii_digit)
    {
        return format!("{}_({})", chars[0], sl(&chars, 1, chars.len()));
    }
    // dl, dy: a differential
    if chars.len() == 2 && chars[0] == 'd' && chars[1].is_ascii_alphabetic() {
        return format!("dif {}", chars[1]);
    }
    // km, kg, xy: upright text, where a bare word is an unknown variable to Typst
    format!("\"{}\"", word.replace(['"', '\\'], ""))
}

fn read_radical(s: &[char], i: usize, to: usize) -> Piece {
    let cube = s[i] == '∛';
    let mut k = i + 1;
    while k < to && jsstr::is_space(s[k]) {
        k += 1;
    }
    if at(s, k) == Some('(')
        && let Some(close) = match_close(s, k, to).filter(|&c| c > 0)
    {
        let inner = emit(s, k + 1, close);
        let text = if cube {
            format!("root(3, {inner})")
        } else {
            format!("sqrt({inner})")
        };
        return (text, close + 1);
    }
    let atom = if k < to { read_piece(s, k, to) } else { None };
    match atom {
        None => (if cube { "root(3, )" } else { "sqrt" }.into(), i + 1),
        Some((t, end)) => (
            if cube {
                format!("root(3, {t})")
            } else {
                format!("sqrt({t})")
            },
            end,
        ),
    }
}

fn read_number(s: &[char], i: usize, to: usize) -> Piece {
    let mut j = i;
    while j < to && (s[j].is_ascii_digit() || s[j] == '.') {
        j += 1;
    }
    let mantissa = sl(s, i, j);
    if matches!(at(s, j), Some('e' | 'E')) && j + 1 < to {
        let mut k = j + 1;
        let minus = matches!(at(s, k), Some('-' | '−'));
        if minus || at(s, k) == Some('+') {
            k += 1;
        }
        let exp = k;
        while k < to && s[k].is_ascii_digit() {
            k += 1;
        }
        // 1.2e+30 is typeset as 1.2 × 10³⁰
        if k > exp {
            return (
                format!(
                    "{mantissa} times 10^({}{})",
                    if minus { "-" } else { "" },
                    sl(s, exp, k)
                ),
                k,
            );
        }
    }
    (mantissa, j)
}

fn one(ch: char) -> Option<&'static str> {
    Some(match ch {
        '+' => "+",
        '-' | '−' | '–' => "-",
        // a centred dot, as multiplication is written by hand; `ast` (∗) reads the same when pasted back
        '*' => "dot",
        '×' => "times",
        '·' => "dot.op",
        '÷' => "div",
        '/' => "/",
        '^' => "^",
        '=' => "=",
        '<' => "<",
        '>' => ">",
        '!' => "!",
        '%' => "%",
        '.' => ".",
        'π' => "pi",
        'θ' => "theta",
        '∞' => "infinity",
        '±' => "#pm",
        '∓' => "minus.plus",
        '≈' => "approx",
        '°' => "degree",
        '…' => "dots.h",
        '|' => "|",
        // bitwise and, drawn as the wedge (a bare & is Typst's alignment point, which draws nothing)
        '&' => "and",
        '\'' | '′' => "'",
        ':' => ":",
        ';' => ";",
        '?' => "?",
        '~' => "tilde.op",
        _ => return None,
    })
}

fn read_symbol(s: &[char], i: usize, to: usize) -> Piece {
    let two = sl(s, i, to.min(i + 3));
    let starts = |p: &str| two.starts_with(p);
    if starts("...") {
        return ("dots.h".into(), i + 3);
    }
    if starts("..") {
        return ("dots.h".into(), i + 2);
    }
    if starts("->") || starts("→") {
        return ("arrow.r".into(), i + if starts("->") { 2 } else { 1 });
    }
    if starts("=>") {
        return ("arrow.r.double".into(), i + 2);
    }
    for (ascii, glyph, put) in [
        ("<=", "≤", "<="),
        (">=", "≥", ">="),
        ("!=", "≠", "!="),
        ("+-", "±", "#pm"),
    ] {
        if starts(ascii) || starts(glyph) {
            return (put.into(), i + if starts(ascii) { 2 } else { 1 });
        }
    }
    let ch = s[i];
    // `^-1` keeps its minus in the exponent; `ᵀ` and `⁻¹` are exponents too
    if ch == '^' {
        let rest = sl(s, i, to);
        if let Some(m) = caps(lazy!(r"^\^\s*[-−]\s*"), &rest) {
            let len = jsre::group(&m, 0).chars().count();
            if let Some((p, end)) = read_piece(s, i + len, to)
                && !p.is_empty()
            {
                return (format!("^(-{p})"), end);
            }
        }
    }
    if ch == 'ᵀ' {
        return ("^T".into(), i + 1);
    }
    let rest = sl(s, i, to);
    if let Some(m) = caps(lazy!(r"^[⁻⁺]?[⁰¹²³⁴⁵⁶⁷⁸⁹]+"), &rest) {
        let raised: Vec<char> = jsre::group(&m, 0).chars().collect();
        if raised.len() > 1 {
            let plain: String = raised
                .iter()
                .map(|&c| match "⁻⁺⁰¹²³⁴⁵⁶⁷⁸⁹".chars().position(|d| d == c) {
                    Some(0) => "-".to_string(),
                    Some(1) => "+".to_string(),
                    Some(n) => (n - 2).to_string(),
                    None => String::new(),
                })
                .collect();
            return (format!("^({plain})"), i + raised.len());
        }
    }
    if let Some(o) = one(ch) {
        return (o.into(), i + 1);
    }
    if ch == '$' || ch == '#' {
        return (format!("\\{ch}"), i + 1);
    }
    // α, Ω, ∂, ∇ and other letters and symbols typeset as themselves
    static GLYPH: Lazy = Lazy::new(|| jsre::compile(r"^[\p{L}\p{No}\p{Sm}\p{So}]$"));
    if (ch as u32) > 0x7f && jsre::test(&GLYPH, &ch.to_string()) {
        return (ch.to_string(), i + 1);
    }
    (String::new(), i + 1)
}

/// Every way a matrix is typed or answered: `[[1,2],[3,4]]`, `[1,2;3,4]`, `[1 2; 3 4]`, `[1 2 3]`. A comma list
/// `[1,2,3]` stays a list.
fn read_matrix(s: &[char], i: usize, to: usize) -> Option<Piece> {
    let close = match_close(s, i, to)?;
    let inner_text = sl(s, i + 1, close);
    let inner = jsstr::trim(&inner_text);
    let rows: Vec<Vec<String>> = if inner.starts_with('[') {
        let parts: Vec<String> = split_top(inner, ',')
            .iter()
            .map(|r| jsstr::trim(r).to_string())
            .collect();
        let well_formed = parts.iter().all(|r| {
            let c: Vec<char> = r.chars().collect();
            r.starts_with('[') && match_close(&c, 0, c.len()) == Some(c.len() - 1)
        });
        if !well_formed {
            return None;
        }
        parts
            .iter()
            .map(|r| {
                let c: Vec<char> = r.chars().collect();
                row_cells(&sl(&c, 1, c.len() - 1))
            })
            .collect()
    } else if split_top(inner, ';').len() > 1 {
        split_top(inner, ';').iter().map(|r| row_cells(r)).collect()
    } else if split_top(inner, ',').len() == 1 && spaced_cells(inner).len() > 1 {
        vec![spaced_cells(inner)]
    } else {
        return None;
    };
    let body = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| or_space(emit_str(c)))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect::<Vec<_>>()
        .join("; ");
    let text = format!("mat(delim: \"[\", {body})");
    // `A'`, `A transpose`, `A inverse` mark the matrix just before
    let after_text = sl(s, close + 1, to);
    let Some(after) = caps(
        lazy!(r"^(?:\s*['′]|\s+(transpose|inverse)\b)", "i"),
        &after_text,
    ) else {
        return Some((text, close + 1));
    };
    let mark = if opt(&after, 1).is_some_and(|w| w.to_lowercase().starts_with("inv")) {
        "(-1)"
    } else {
        "T"
    };
    Some((
        format!("{text}^{mark}"),
        close + 1 + jsre::group(&after, 0).chars().count(),
    ))
}

fn row_cells(row: &str) -> Vec<String> {
    let cells = split_top(row, ',');
    if cells.len() > 1 {
        cells
    } else {
        spaced_cells(row)
    }
}

/// `3/5 -7/10` is two entries; `1 + 2` and `2 * x` are one.
fn spaced_cells(row: &str) -> Vec<String> {
    let chars: Vec<char> = row.chars().collect();
    let mut depth = 0;
    let mut start = 0;
    let mut words: Vec<String> = Vec::new();
    for k in 0..=chars.len() {
        let ch = at(&chars, k).unwrap_or(' ');
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if jsstr::is_space(ch) && depth == 0 {
            if k > start {
                words.push(sl(&chars, start, k));
            }
            start = k + 1;
        }
    }
    let mut cells: Vec<String> = Vec::new();
    for w in words {
        let joins = cells.last().is_some_and(|prev| {
            prev.ends_with(|c: char| "+-−*/^×·÷".contains(c))
                || w.starts_with(|c: char| "*/^×·÷".contains(c))
                || (w.chars().count() == 1 && "+-−".contains(w.as_str()))
        });
        match cells.last_mut() {
            Some(prev) if joins => *prev = format!("{prev} {w}"),
            _ => cells.push(w),
        }
    }
    cells
}

/// The matrix functions, written the way they're printed: A^(-1), A^T, tr A, I_3, a · b.
fn matrix_fn(key: &str, args: &[String]) -> Option<String> {
    let a = args.first().map_or("", String::as_str);
    let b = args.get(1).map_or("", String::as_str);
    let applied = |head: &str| {
        if a.starts_with("mat(") && atomic(a) {
            format!("{head} {a}")
        } else {
            format!("{head}({a})")
        }
    };
    Some(match key {
        "inv" | "inverse" => format!("{}^(-1)", grouped(a)),
        "transpose" => format!("{}^T", grouped(a)),
        "det" | "determinant" => applied("det"),
        "trace" | "tr" => applied(r#"op("tr")"#),
        "rank" | "rref" => applied(&format!("op(\"{key}\")")),
        "identity" | "eye" => format!("I_({a})"),
        "dot" | "cross" if args.len() == 2 => format!(
            "{} {} {}",
            grouped(a),
            if key == "dot" { "dot" } else { "times" },
            grouped(b)
        ),
        _ => return None,
    })
}

fn grouped(t: &str) -> String {
    if atomic(t) {
        t.to_string()
    } else {
        format!("({t})")
    }
}

/// One piece of math, with no space outside its own brackets or quotes.
fn atomic(t: &str) -> bool {
    let mut depth = 0;
    let mut quoted = false;
    for ch in t.chars() {
        if ch == '"' {
            quoted = !quoted;
        } else if quoted {
            continue;
        } else if "([".contains(ch) {
            depth += 1;
        } else if ")]".contains(ch) {
            depth -= 1;
        } else if ch == ' ' && depth == 0 {
            return false;
        }
    }
    !t.is_empty()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BigOp {
    Sum,
    Product,
    Integral,
}

impl BigOp {
    fn head(self) -> &'static str {
        match self {
            BigOp::Sum => "sum",
            BigOp::Product => "product",
            BigOp::Integral => "integral",
        }
    }
}

fn op_of_word(w: &str) -> BigOp {
    let w = w.to_lowercase();
    if w.starts_with("prod") {
        BigOp::Product
    } else if w.starts_with("int") {
        BigOp::Integral
    } else {
        BigOp::Sum
    }
}

fn read_big_op(s: &[char], i: usize, to: usize) -> Option<Piece> {
    let mut j = i;
    let op = match s[j] {
        'Σ' | '∑' => {
            j += 1;
            BigOp::Sum
        }
        'Π' | '∏' => {
            j += 1;
            BigOp::Product
        }
        '∫' => {
            j += 1;
            BigOp::Integral
        }
        _ => {
            let rest = sl(s, j, to);
            let m = big_word(&rest)?;
            j += jsre::group(&m, 0).chars().count();
            op_of_word(jsre::group(&m, 1))
        }
    };
    while j < to && jsstr::is_space(s[j]) {
        j += 1;
    }
    if at(s, j) == Some('(') {
        let close = match_close(s, j, to)?;
        let args: Vec<String> = split_top(&sl(s, j + 1, close), ',')
            .iter()
            .map(|a| jsstr::trim(a).to_string())
            .collect();
        let rendered = render_call_op(op, &args)?;
        return Some((rendered, close + 1));
    }
    if let Some(bound) = read_limit_op(s, i, to, op) {
        return Some(bound);
    }
    let comma = top_index(s, j, to, ',')?;
    if op == BigOp::Integral {
        return None;
    }
    let body = emit(s, j, comma);
    let range = parse_range(jsstr::trim(&sl(s, comma + 1, to)))?;
    let head = if op == BigOp::Product {
        "product"
    } else {
        "sum"
    };
    Some((
        format!(
            "{head}_({} = {})^({}) {body}",
            range.index, range.from, range.to
        ),
        to,
    ))
}

/// `x^2 dx` names the variable; the differential is Typst's `dif`, not part of the integrand.
fn peel_differential(src: &str) -> (String, String) {
    match caps(lazy!(r"^(.*?)(?:\s*\*)?\s*\bd([A-Za-z])\s*$", "s"), src) {
        Some(m) => (
            jsstr::trim(jsre::group(&m, 1)).to_string(),
            format!("dif {}", jsre::group(&m, 2)),
        ),
        None => (jsstr::trim(src).to_string(), String::new()),
    }
}

fn render_call_op(op: BigOp, args: &[String]) -> Option<String> {
    if op == BigOp::Integral {
        return match args.len() {
            3 => {
                let (src, dif) = peel_differential(&args[0]);
                let body = emit_str(&src);
                let dif = if dif.is_empty() {
                    String::new()
                } else {
                    format!(" {dif}")
                };
                Some(
                    jsstr::trim(&format!(
                        "integral_({})^({}) {body}{dif}",
                        emit_str(&args[1]),
                        emit_str(&args[2])
                    ))
                    .to_string(),
                )
            }
            4 => Some(format!(
                "integral_({})^({}) {} dif {}",
                emit_str(&args[2]),
                emit_str(&args[3]),
                emit_str(&args[0]),
                jsstr::trim(&args[1])
            )),
            _ => None,
        };
    }
    let head = if op == BigOp::Product {
        "product"
    } else {
        "sum"
    };
    match args.len() {
        2 => {
            let range = parse_range(&args[1])?;
            Some(format!(
                "{head}_({} = {})^({}) {}",
                range.index,
                range.from,
                range.to,
                emit_str(&args[0])
            ))
        }
        3 => {
            if let Some(named) = caps(lazy!(r"^([A-Za-z]|theta)\s*=\s*(.+)$", "i"), &args[1]) {
                return Some(format!(
                    "{head}_({} = {})^({}) {}",
                    jsre::group(&named, 1),
                    emit_str(jsre::group(&named, 2)),
                    emit_str(&args[2]),
                    emit_str(&args[0])
                ));
            }
            Some(format!(
                "{head}_(n = {})^({}) {}",
                emit_str(&args[1]),
                emit_str(&args[2]),
                emit_str(&args[0])
            ))
        }
        4 => Some(format!(
            "{head}_({} = {})^({}) {}",
            jsstr::trim(&args[1]),
            emit_str(&args[2]),
            emit_str(&args[3]),
            emit_str(&args[0])
        )),
        _ => None,
    }
}

fn read_subscript(s: &[char], i: usize, to: usize) -> Piece {
    let mut j = i + 1;
    while j < to && jsstr::is_space(s[j]) {
        j += 1;
    }
    if j >= to {
        return (String::new(), i + 1);
    }
    if s[j] == '{' {
        return match match_close(s, j, to) {
            None => (format!("_({})", emit(s, j + 1, to)), to),
            Some(close) => (format!("_({})", emit(s, j + 1, close)), close + 1),
        };
    }
    match read_piece(s, j, to) {
        Some((t, end)) if !t.is_empty() => (format!("_({t})"), end),
        _ => (String::new(), j),
    }
}

/// `sum` and `int` are the same limits as Σ and ∫. `bounds_in` only sees the sign, so a word is rewritten as one.
fn limit_source(s: &[char], i: usize, to: usize) -> Option<String> {
    if is_big_sign(s[i]) {
        return Some(sl(s, i, to));
    }
    let rest = sl(s, i, to);
    let m = big_word(&rest)?;
    let glyph = match op_of_word(jsre::group(&m, 1)) {
        BigOp::Product => 'Π',
        BigOp::Integral => '∫',
        BigOp::Sum => 'Σ',
    };
    let len = jsre::group(&m, 0).chars().count();
    Some(format!("{glyph}{}", sl(s, i + len, to)))
}

/// The calculator stores a built-in integral as ∫_lower^upper. The underscore is the limit marker, not a
/// character to print.
fn read_limit_op(s: &[char], i: usize, to: usize, op: BigOp) -> Option<Piece> {
    let src = U16::new(&limit_source(s, i, to)?);
    let found = bounds_in_units(&src).into_iter().find(|b| b.sign == 0)?;
    let lower = emit_str(&src.slice(found.lower.start, found.lower.end));
    let upper = found
        .upper
        .map_or(String::new(), |u| emit_str(&src.slice(u.start, u.end)));
    let tail_src = src.slice(found.end, i64::MAX);
    let (body_src, dif) = if op == BigOp::Integral {
        peel_differential(&tail_src)
    } else {
        (jsstr::trim(&tail_src).to_string(), String::new())
    };
    let body = jsstr::trim(&emit_str(&body_src)).to_string();
    let mut limits = String::new();
    if !lower.is_empty() {
        limits += &format!("_({lower})");
    }
    if !upper.is_empty() {
        limits += &format!("^({upper})");
    }
    let mut tail = String::new();
    if !body.is_empty() {
        tail += &format!(" {body}");
    }
    if !dif.is_empty() {
        tail += &format!(" {dif}");
    }
    Some((format!("{}{limits}{tail}", op.head()), to))
}

struct Range {
    index: String,
    from: String,
    to: String,
}

fn parse_range(raw: &str) -> Option<Range> {
    let m = caps(
        lazy!(
            r"^(?:([A-Za-z]|theta)\s*(?:=|from|in)\s*)?(.+?)\s*(?:\.\.\.?|…|\bto\b)\s*(.+)$",
            "i"
        ),
        raw,
    )?;
    let from = jsstr::trim(jsre::group(&m, 2));
    let upper = jsstr::trim(jsre::group(&m, 3));
    if from.is_empty()
        || upper.is_empty()
        || from.chars().any(jsstr::is_space)
        || upper.chars().any(jsstr::is_space)
    {
        return None;
    }
    Some(Range {
        index: opt(&m, 1)
            .filter(|x| !x.is_empty())
            .unwrap_or("n")
            .to_string(),
        from: emit_str(from),
        to: emit_str(upper),
    })
}

fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if ch == sep && depth == 0 {
            out.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(ch);
    }
    out.push(cur);
    out
}

fn top_index(s: &[char], from: usize, to: usize, sep: char) -> Option<usize> {
    let mut depth = 0;
    for (i, &ch) in s.iter().enumerate().take(to).skip(from) {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if ch == sep && depth == 0 {
            return Some(i);
        }
    }
    None
}

fn match_close(s: &[char], open: usize, to: usize) -> Option<usize> {
    let first = *s.get(open)?;
    let want = match first {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    let mut depth = 0;
    for (i, &c) in s.iter().enumerate().take(to).skip(open) {
        if c == first {
            depth += 1;
        } else if c == want {
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

/// Replays a recorded call of `lib/typstMath`, `lib/radical` or `lib/bounds`; None for a function not ported here.
pub fn golden_dispatch(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    use crate::app::{bounds, radical};
    use serde_json::{Value as Json, json};
    let s = |i: usize| crate::golden::str_arg(args, i).unwrap_or_default();
    let n = |i: usize| crate::golden::f64_arg(args, i).unwrap_or(f64::NAN) as i64;
    let to_json = |v: Option<bounds::Edit>| serde_json::to_value(v).unwrap_or(Json::Null);
    let bounds_arg = || -> Option<Vec<bounds::Bound>> {
        serde_json::from_value(crate::golden::decode(args.first()?)).ok()
    };
    Some(match name {
        "toTypstMath" => json!(to_typst_math(&s(0))),
        "previewMath" => json!(preview_math(&s(0))),
        "typstPreviewUseful" => json!(typst_preview_useful(&s(0))),
        "typstDocument" => json!(typst_document(&s(0), &s(1), &s(2))),
        "copiedEquation" => json!(copied_equation(
            &s(0),
            args.get(1).and_then(Json::as_bool).unwrap_or(false)
        )),
        "typstAnswer" => json!(typst_answer(
            crate::golden::str_arg(args, 0).as_deref(),
            &s(1)
        )),
        "radicalSpans" => serde_json::to_value(radical::radical_spans(&s(0))).ok()?,
        "radicalAnswer" => json!(radical::radical_answer(&s(0))),
        "answerParts" => json!(radical::answer_parts(&s(0))),
        "boundsIn" => serde_json::to_value(bounds::bounds_in(&s(0))).ok()?,
        "isHidden" => json!(bounds::is_hidden(&bounds_arg()?, n(1))),
        "snapOffset" => json!(bounds::snap_offset(&bounds_arg()?, n(1), n(2), n(3))),
        "afterTyping" => to_json(bounds::after_typing(&s(0), n(1))),
        "boundKey" => to_json(bounds::bound_key(&s(0), n(1), &s(2))),
        "wordToSign" => to_json(bounds::word_to_sign(&s(0), n(1))),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_typst() {
        assert_eq!(copied_equation("2^8", true), "2 ^ 8");
        assert_eq!(copied_equation("√(2)+1", true), "sqrt(2) + 1");
        assert_eq!(
            copied_equation("Σ_n=1^10 n^2", true),
            "sum_(n = 1)^(10) n ^ 2"
        );
        assert_eq!(
            copied_equation("solve x^2 = 4", true),
            "\"solve\" x ^ 2 = 4"
        );
    }
}
