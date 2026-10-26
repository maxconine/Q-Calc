//! Calculus lines (`src/engine/calculus.ts`): derivatives (symbolic through mathjs's rules, checked against finite
//! differences), integrals and limits (numeric, `numeric.rs`), with a closed-form job for the background search
//! (`closed_form.rs`).
//!
//! TS quirk kept: a user function is inlined with one node object per argument, so `f(t) = t*t` differentiates
//! as mathjs differentiates a product of one object with itself (see `tree.rs`), and `f'(x)`, `d/dx f(x)` and
//! `f'(3)` come out blank, as in the TS.

pub mod closed_form;
pub mod golden;
pub mod numeric;
pub mod pretty;
pub mod together;
pub mod tree;

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::jsnum;
use crate::jsre::{self, Lazy};
use crate::scientific::{NAMES, ScientificContext, eval_scientific, preprocess_checked};
use crate::symbolic::Op;
use crate::types::{AngleMode, TimeLetters, UserFunction, Value, ValueKind};

use closed_form::{Ast, Bound, Job, JobKind};
use numeric::{derivative_at, integrate, justified, limit};
use pretty::{has_symbol, pretty};
use together::together;
use tree::{N, T, call, constant, op, symbol};

/// What a calculus line gave.
#[derive(Clone, Debug, Default)]
pub struct CalculusResult {
    pub value: Option<Value>,
    pub exact: Option<String>,
    /// The closed-form job the background search may upgrade (`closed_form::Job` as JSON).
    pub job: Option<serde_json::Value>,
    pub time: Option<TimeLetters>,
}

/// What `evaluateCalculus` sees: the scientific context plus the d/dt letter overrides.
#[derive(Clone, Debug, Default)]
pub struct CalcContext {
    pub sci: ScientificContext,
    pub time_varying: BTreeMap<String, bool>,
}

// ---- text helpers (the TS indexes strings; these index chars)

fn re(pattern: &str, flags: &str) -> jsre::Regex {
    let p = if flags.contains('s') {
        format!("(?s){pattern}")
    } else {
        pattern.to_string()
    };
    jsre::js(&p, if flags.contains('i') { "i" } else { "" })
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn cut(c: &[char], a: usize, b: usize) -> String {
    c[a.min(c.len())..b.min(c.len())].iter().collect()
}

fn caps<'t>(r: &jsre::Regex, s: &'t str) -> Option<jsre::Caps<'t>> {
    r.captures(s).ok().flatten()
}

fn g<'t>(c: &jsre::Caps<'t>, i: usize) -> Option<&'t str> {
    c.get(i).map(|m| m.as_str())
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

// ---- syntax

#[derive(Clone, Debug)]
enum Intent {
    /// One variable per differentiation, innermost first; `""` means pick one from the body.
    Derivative {
        body: String,
        vars: Vec<String>,
        at: Option<String>,
        partial: bool,
    },
    Integral {
        body: String,
        v: Option<String>,
        lower: String,
        upper: String,
    },
    Limit {
        body: String,
        v: String,
        to: String,
        side: i32,
    },
}

fn order_of(s: Option<&str>) -> Option<usize> {
    match s? {
        "" => None,
        "²" => Some(2),
        "³" => Some(3),
        d => d.parse().ok(),
    }
}

/// The last ` at ` outside parentheses.
fn split_at(s: &str) -> (String, Option<String>) {
    static AT: Lazy = Lazy::new(|| re(r"^\s+at\s+\S", "i"));
    static LEAD: Lazy = Lazy::new(|| re(r"^\s+at\s+", "i"));
    let c = chars(s);
    let mut depth = 0i32;
    let mut found: Option<usize> = None;
    for (i, &ch) in c.iter().enumerate() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if depth == 0 && is_space(ch) && jsre::test(&AT, &cut(&c, i, c.len())) {
            found = Some(i);
        }
    }
    match found {
        None => (s.trim().to_string(), None),
        Some(i) => {
            let tail = cut(&c, i, c.len());
            let at = jsre::replace_all(&LEAD, &tail, |_| String::new())
                .trim()
                .to_string();
            (cut(&c, 0, i).trim().to_string(), Some(at))
        }
    }
}

fn closing_paren(c: &[char], open: usize) -> Option<usize> {
    let close = match c.get(open)? {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        _ => return None,
    };
    let mut depth = 0;
    for (i, &ch) in c.iter().enumerate().skip(open) {
        if ch == c[open] {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn strip_outer_parens(s: &str) -> String {
    let mut t = s.trim().to_string();
    loop {
        let c = chars(&t);
        if t.starts_with('(') && t.ends_with(')') && closing_paren(&c, 0) == Some(c.len() - 1) {
            t = cut(&c, 1, c.len() - 1).trim().to_string();
        } else {
            return t;
        }
    }
}

/// Top-level comma-separated arguments of `(a, b, c)`.
fn split_args(inner: &str) -> Vec<String> {
    let c = chars(inner);
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, &ch) in c.iter().enumerate() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if ch == ',' && depth == 0 {
            out.push(cut(&c, start, i).trim().to_string());
            start = i + 1;
        }
    }
    out.push(cut(&c, start, c.len()).trim().to_string());
    out
}

/// A bound written before the body: up to the first space, or a `(` that starts the body (`∫0..1(x+1)`).
fn scan_bound(s: &str) -> Option<(String, String)> {
    let t = s.trim_start();
    let c = chars(t);
    if t.starts_with('{') {
        let close = closing_paren(&c, 0)?;
        return Some((
            cut(&c, 1, close),
            cut(&c, close + 1, c.len()).trim().to_string(),
        ));
    }
    let mut depth = 0;
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if depth == 0 && is_space(ch) {
            break;
        }
        if ch == '(' {
            if depth == 0 && i > 0 && !(c[i - 1].is_ascii_alphabetic() || c[i - 1] == '_') {
                break;
            }
            depth += 1;
        } else if ch == ')' {
            if depth == 0 {
                return None;
            }
            depth -= 1;
        }
        i += 1;
    }
    let bound = cut(&c, 0, i);
    if bound.is_empty() || depth != 0 {
        return None;
    }
    Some((bound, cut(&c, i, c.len()).trim().to_string()))
}

/// `x^2 dx` names its variable; the `dx` isn't part of the body.
fn strip_differential(body: &str) -> (String, Option<String>) {
    static DIFF: Lazy = Lazy::new(|| re(r"^(.*?)(?:\s*\*)?\s*\bd([A-Za-z])\s*$", "s"));
    match caps(&DIFF, body) {
        Some(m) if !g(&m, 1).unwrap_or("").trim().is_empty() => (
            g(&m, 1).unwrap_or("").trim().to_string(),
            g(&m, 2).map(str::to_string),
        ),
        _ => (body.trim().to_string(), None),
    }
}

fn integral_from_args(args: &[String]) -> Option<Intent> {
    static LETTER: Lazy = Lazy::new(|| re(r"^[A-Za-z]$", ""));
    if args.len() == 3 && args.iter().all(|a| !a.is_empty()) {
        let (body, v) = strip_differential(&args[0]);
        return Some(Intent::Integral {
            body,
            v,
            lower: args[1].clone(),
            upper: args[2].clone(),
        });
    }
    if args.len() == 4
        && jsre::test(&LETTER, &args[1])
        && !args[0].is_empty()
        && !args[2].is_empty()
        && !args[3].is_empty()
    {
        return Some(Intent::Integral {
            body: args[0].clone(),
            v: Some(args[1].clone()),
            lower: args[2].clone(),
            upper: args[3].clone(),
        });
    }
    None
}

/// `f from a to b`
fn from_to(s: &str) -> Option<(String, String, String)> {
    static FT: Lazy = Lazy::new(|| re(r"^(.+?)\s+from\s+(.+?)\s+to\s+(.+)$", "is"));
    let m = caps(&FT, s)?;
    Some((
        g(&m, 1)?.trim().to_string(),
        g(&m, 2)?.trim().to_string(),
        g(&m, 3)?.trim().to_string(),
    ))
}

const SIGN: &str = r"^(?:∫|int(?=_|\s|[^A-Za-z\s(]\S*\.\.))";

fn parse_integral(text: &str, functions: &BTreeMap<String, UserFunction>) -> Option<Intent> {
    static CALL: Lazy = Lazy::new(|| re(r"^(?:int|integral|integrate|∫)\s*\((.*)\)$", "is"));
    static NAME: Lazy = Lazy::new(|| re(r"^[A-Za-z]+", ""));
    static WORDS: Lazy = Lazy::new(|| {
        re(
            r"^(?:the\s+)?(?:integral|integrate)\s+(?:of\s+)?(.+)$",
            "is",
        )
    });
    static FROM: Lazy = Lazy::new(|| re(r"^from\s+(.+?)\s+to\s+(.+?)\s+of\s+(.+)$", "is"));
    static SIGN_RE: Lazy = Lazy::new(|| re(SIGN, "i"));
    static UNDER: Lazy = Lazy::new(|| re(r"^_\s*", ""));
    let c = chars(text);
    if let Some(m) = caps(&CALL, text)
        && let Some(open) = c.iter().position(|&x| x == '(')
        && closing_paren(&c, open) == Some(c.len() - 1)
    {
        if let Some(name) = NAME.find(text).ok().flatten()
            && functions.contains_key(name.as_str())
        {
            return None;
        }
        return integral_from_args(&split_args(g(&m, 1).unwrap_or("")));
    }
    if let Some(m) = caps(&WORDS, text) {
        let w = g(&m, 1).unwrap_or("");
        let ft = match caps(&FROM, w) {
            Some(f) => Some((
                g(&f, 3).unwrap_or("").to_string(),
                g(&f, 1).unwrap_or("").to_string(),
                g(&f, 2).unwrap_or("").to_string(),
            )),
            None => from_to(w),
        };
        let (body, lower, upper) = ft?;
        let (body, v) = strip_differential(&body);
        return Some(Intent::Integral {
            body,
            v,
            lower,
            upper,
        });
    }
    // the word works as the sign too, for anyone who keeps typed words as text
    let sign = SIGN_RE.find(text).ok().flatten()?;
    let rest = text[sign.end()..].trim().to_string();
    let (lower, upper, body);
    if let Some(u) = UNDER.find(&rest).ok().flatten() {
        let r = &rest[u.end()..];
        let lo = if r.starts_with('{') {
            scan_bound(r)
        } else {
            let rc = chars(r);
            match rc.iter().position(|&x| x == '^') {
                Some(k) if k > 0 => Some((cut(&rc, 0, k), cut(&rc, k, rc.len()))),
                _ => None,
            }
        };
        let (lo_b, lo_rest) = lo?;
        let r = lo_rest.trim();
        let after = r.strip_prefix('^')?;
        let (hi_b, hi_rest) = scan_bound(after)?;
        lower = lo_b;
        upper = hi_b;
        body = hi_rest;
    } else {
        let rc = chars(&rest);
        let dots = rest.find("..").map(|b| rest[..b].chars().count());
        match dots {
            Some(d) if d > 0 && !cut(&rc, 0, d).trim().chars().any(is_space) => {
                let (hi_b, hi_rest) = scan_bound(&cut(&rc, d + 2, rc.len()))?;
                lower = cut(&rc, 0, d);
                upper = hi_b;
                body = hi_rest;
            }
            _ => {
                let (b, l, u) = from_to(&rest)?;
                lower = l;
                upper = u;
                body = b;
            }
        }
    }
    if lower.trim().is_empty() || upper.trim().is_empty() || body.trim().is_empty() {
        return None;
    }
    let (body, v) = strip_differential(&body);
    Some(Intent::Integral {
        body,
        v,
        lower: lower.trim().to_string(),
        upper: upper.trim().to_string(),
    })
}

/// First `)` `]` or `}` that the integral itself did not open, else the end of the line.
fn group_end(c: &[char], from: usize) -> usize {
    let mut depth = 0;
    for (i, &ch) in c.iter().enumerate().skip(from) {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            if depth == 0 {
                return i;
            }
            depth -= 1;
        }
    }
    c.len()
}

fn is_differential_at(c: &[char], i: usize) -> bool {
    if c[i] != 'd' && c[i] != 'D' {
        return false;
    }
    if i > 0 && word_char(c[i - 1]) {
        return false;
    }
    if !c.get(i + 1).is_some_and(char::is_ascii_alphabetic) {
        return false;
    }
    !c.get(i + 2).is_some_and(|&x| word_char(x))
}

/// Where a sign-form integrand ends so `∫ x^2 dx + 1` is the integral plus one, not a failed body.
fn differential_cut(c: &[char], from: usize, limit: usize) -> Option<usize> {
    let mut depth = 0;
    for i in from..limit {
        let ch = c[i];
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            if depth == 0 {
                return None;
            }
            depth -= 1;
        } else if depth == 0 && is_differential_at(c, i) {
            let mut j = i + 2;
            while j < limit && is_space(c[j]) {
                j += 1;
            }
            if j < limit && "+-*/^×·⋅".contains(c[j]) {
                return Some(i + 2);
            }
        }
    }
    None
}

fn locate_integral(
    c: &[char],
    at: usize,
    functions: &BTreeMap<String, UserFunction>,
) -> Option<(usize, usize)> {
    static CALL: Lazy = Lazy::new(|| re(r"^(?:int|integral|integrate|∫)\s*\(", "i"));
    static SIGN_RE: Lazy = Lazy::new(|| re(SIGN, "i"));
    static WORD: Lazy = Lazy::new(|| re(r"^(?:the\s+)?(?:integral|integrate)\b", "i"));
    static EITHER: Lazy = Lazy::new(|| {
        re(
            r"^(?:the\s+)?(?:integral|integrate)\b|^(?:∫|int(?=_|\s|[^A-Za-z\s(]\S*\.\.))",
            "i",
        )
    });
    let slice = cut(c, at, c.len());
    if let Some(m) = CALL.find(&slice).ok().flatten() {
        let open = at + m.as_str().chars().count() - 1;
        let close = closing_paren(c, open)?;
        return parse_integral(&cut(c, at, close + 1), functions).map(|_| (at, close + 1));
    }
    let limit = group_end(c, at);
    let mut end = limit;
    while end > at && is_space(c[end - 1]) {
        end -= 1;
    }
    if jsre::test(&SIGN_RE, &slice)
        && !jsre::test(&WORD, &slice)
        && let Some(k) = differential_cut(c, at, end)
        && parse_integral(&cut(c, at, k), functions).is_some()
    {
        return Some((at, k));
    }
    if !jsre::test(&EITHER, &slice) {
        return None;
    }
    parse_integral(&cut(c, at, end), functions).map(|_| (at, end))
}

/// What folding integrals beside other terms into their values did (`embedIntegrals`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Embedded {
    /// No integral to fold.
    Nothing,
    /// An integral that couldn't be worked out: the line is blank.
    Failed,
    /// The line with an integral replaced by its value.
    Rewritten(String),
}

/// `embedIntegrals`: replaces integrals inside a larger line (`4*∫0..1 x^2`) with their values.
pub fn embed_integrals(expr: &str, ctx: &CalcContext) -> Embedded {
    static START: Lazy = Lazy::new(|| re(r"(?:the\s+)?(?:integrate|integral)|int|∫", "i"));
    let c = chars(expr);
    let mut found: Vec<(usize, usize)> = Vec::new();
    for m in START.find_iter(expr).flatten() {
        let at = expr[..m.start()].chars().count();
        if at > 0 && word_char(c[at - 1]) {
            continue;
        }
        if let Some(loc) = locate_integral(&c, at, &ctx.sci.functions) {
            found.push(loc);
        }
    }
    if found.is_empty() {
        return Embedded::Nothing;
    }
    let innermost: Vec<(usize, usize)> = found
        .iter()
        .enumerate()
        .filter(|(i, a)| {
            !found
                .iter()
                .enumerate()
                .any(|(j, b)| j != *i && b.0 >= a.0 && b.1 <= a.1 && (b.0 > a.0 || b.1 < a.1))
        })
        .map(|(_, a)| *a)
        .collect();
    let mut ready: Vec<(usize, usize, f64)> = Vec::new();
    for (s, e) in innermost {
        let Some(r) = evaluate_calculus(&cut(&c, s, e), ctx) else {
            return Embedded::Failed;
        };
        match r.value {
            Some(v) if v.kind == ValueKind::Number && v.n.is_finite() => ready.push((s, e, v.n)),
            _ => return Embedded::Failed,
        }
    }
    ready.sort_by_key(|r| std::cmp::Reverse(r.0));
    let mut text = c;
    for (s, e, n) in ready {
        let mut next: Vec<char> = text[..s].to_vec();
        next.extend(format!("({})", jsnum::to_string(n)).chars());
        next.extend_from_slice(&text[e.min(text.len())..]);
        text = next;
    }
    Embedded::Rewritten(text.into_iter().collect())
}

fn parse_side(to: &str) -> (String, i32) {
    static SIDE: Lazy = Lazy::new(|| re(r"^(.+?)(?:\^?\s*([+-])|([⁺⁻]))$", ""));
    match caps(&SIDE, to) {
        Some(m) if !g(&m, 1).unwrap_or("").trim().is_empty() => {
            let s = g(&m, 2).unwrap_or(if g(&m, 3) == Some("⁺") { "+" } else { "-" });
            (
                g(&m, 1).unwrap_or("").trim().to_string(),
                if s == "+" { 1 } else { -1 },
            )
        }
        _ => (to.to_string(), 0),
    }
}

const ARROW: &str = r"(?:->|→|\\to\b)";

fn parse_limit(text: &str) -> Option<Intent> {
    static WORDS: Lazy = Lazy::new(|| {
        re(
            &format!(
                r"^(?:the\s+)?limit\s+of\s+(.+?)\s+as\s+([A-Za-z])\s*(?:{ARROW}|approaches\s)\s*(.+)$"
            ),
            "is",
        )
    });
    static LIM: Lazy = Lazy::new(|| {
        re(
            &format!(r"^lim(?:it)?\s*(?:_\s*)?([({{]?)\s*([A-Za-z])\s*{ARROW}\s*(.*)$"),
            "s",
        )
    });
    if let Some(w) = caps(&WORDS, text) {
        let (to, side) = parse_side(g(&w, 3).unwrap_or("").trim());
        return Some(Intent::Limit {
            body: g(&w, 1).unwrap_or("").trim().to_string(),
            v: g(&w, 2).unwrap_or("").to_string(),
            to,
            side,
        });
    }
    let m = caps(&LIM, text)?;
    let v = g(&m, 2)?.to_string();
    let rest = g(&m, 3).unwrap_or("");
    let open = g(&m, 1).unwrap_or("");
    let (to_text, rest) = if !open.is_empty() {
        let o = open.chars().next()?;
        let close = if o == '(' { ')' } else { '}' };
        let rc = chars(rest);
        let mut depth = 1;
        let mut i = 0;
        while i < rc.len() {
            if rc[i] == o {
                depth += 1;
            } else if rc[i] == close {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += 1;
        }
        if i >= rc.len() {
            return None;
        }
        (cut(&rc, 0, i), cut(&rc, i + 1, rc.len()))
    } else {
        scan_bound(rest)?
    };
    let body = rest.trim().to_string();
    if body.is_empty() || to_text.trim().is_empty() {
        return None;
    }
    let (to, side) = parse_side(to_text.trim());
    Some(Intent::Limit { body, v, to, side })
}

/// A variable a derivative can be taken in: a letter, a Greek letter, or `theta`.
const VAR: &str = r"(?:theta|[A-Za-z]|(?![πτζθΠΣ])[α-ωΑ-Ω])";
const ORDER: &str = r"(?:\^?([2-9]|[²³]))?";
const LETTER: &str = r"A-Za-z0-9_α-ωΑ-Ω";

static PARTIAL_PART: Lazy = Lazy::new(|| re(&format!(r"∂\s*({VAR})(?:\^?([2-9]|[²³]))?"), ""));

/// `∂²/∂x∂y`: the denominator's variables, rightmost first since that one is applied first.
fn partial_vars(denominator: &str) -> Vec<String> {
    let mut out = Vec::new();
    for m in PARTIAL_PART.captures_iter(denominator).flatten() {
        let n = order_of(g(&m, 2)).unwrap_or(1);
        for _ in 0..n {
            out.push(g(&m, 1).unwrap_or("").to_string());
        }
    }
    out.reverse();
    out
}

fn parse_derivative(
    text: &str,
    functions: &BTreeMap<String, UserFunction>,
    variables: &BTreeMap<String, f64>,
) -> Option<Intent> {
    static ORDINARY: Lazy = Lazy::new(|| {
        re(
            &format!(r"^d{ORDER}\s*\/\s*d({VAR}){ORDER}(?![A-Za-z0-9_α-ωΑ-Ω])(.*)$"),
            "s",
        )
    });
    static PARTIAL: Lazy = Lazy::new(|| {
        re(
            &format!(
                r"^∂{ORDER}\s*\/\s*((?:∂\s*{VAR}(?:\^?(?:[2-9]|[²³]))?\s*)+?)(?![A-Za-z0-9_α-ωΑ-Ω])(?=[\s(]|$)(.*)$"
            ),
            "s",
        )
    });
    static WORDS: Lazy = Lazy::new(|| {
        re(
            r"^(?:the\s+)?(?:(second|2nd)\s+)?(?:partial\s+)?derivative\s+of\s+(.+)$",
            "is",
        )
    });
    static WRT: Lazy = Lazy::new(|| {
        re(
            &format!(r"^(.+?)\s+(?:with\s+respect\s+to|wrt|w\.r\.t\.?)\s+({VAR})$"),
            "is",
        )
    });
    static PRIME: Lazy = Lazy::new(|| re(r"^([A-Za-z][A-Za-z0-9]*)\s*('+|′+|″)\s*\((.+)\)$", "s"));
    static ONE_LETTER: Lazy = Lazy::new(|| re(r"^[A-Za-z]$", ""));
    if let Some(m) = caps(&ORDINARY, text) {
        let v = g(&m, 2)?.to_string();
        // `d/dx` with `d` and `dx` both defined is plain division
        let dv = format!("d{v}");
        if variables.contains_key(&dv) || functions.contains_key(&dv) {
            return None;
        }
        let top = order_of(g(&m, 1));
        let bottom = order_of(g(&m, 3));
        if top != bottom {
            return None;
        }
        let (body, at) = split_at(g(&m, 4).unwrap_or(""));
        if body.is_empty() {
            return None;
        }
        return Some(Intent::Derivative {
            body: strip_outer_parens(&body),
            vars: vec![v; top.unwrap_or(1)],
            at,
            partial: false,
        });
    }
    if let Some(p) = caps(&PARTIAL, text) {
        let vars = partial_vars(g(&p, 2).unwrap_or(""));
        if vars.len() != order_of(g(&p, 1)).unwrap_or(1) || vars.len() > 4 {
            return None;
        }
        let (body, at) = split_at(g(&p, 3).unwrap_or(""));
        if body.is_empty() {
            return None;
        }
        return Some(Intent::Derivative {
            body: strip_outer_parens(&body),
            vars,
            at,
            partial: true,
        });
    }
    if let Some(w) = caps(&WORDS, text) {
        let (sbody, sat) = split_at(g(&w, 2).unwrap_or(""));
        let wrt = caps(&WRT, &sbody);
        let body = match &wrt {
            Some(x) => g(x, 1).unwrap_or("").trim().to_string(),
            None => sbody.clone(),
        };
        if body.is_empty() {
            return None;
        }
        let var = wrt.as_ref().and_then(|x| g(x, 2)).unwrap_or("").to_string();
        let n = if g(&w, 1).is_some() { 2 } else { 1 };
        return Some(Intent::Derivative {
            body,
            vars: vec![var; n],
            at: sat,
            partial: false,
        });
    }
    let prime = caps(&PRIME, text)?;
    let name = g(&prime, 1)?;
    let def = functions.get(name)?;
    if def.params.len() != 1 {
        return None;
    }
    let marks = g(&prime, 2)?;
    let order = if marks == "″" {
        2
    } else {
        marks.chars().count()
    };
    if order > 3 {
        return None;
    }
    let arg = g(&prime, 3)?.trim().to_string();
    let param = def.params[0].clone();
    if jsre::test(&ONE_LETTER, &arg) && !variables.contains_key(&arg) {
        return Some(Intent::Derivative {
            body: format!("{name}({arg})"),
            vars: vec![arg; order],
            at: None,
            partial: false,
        });
    }
    Some(Intent::Derivative {
        body: format!("{name}({param})"),
        vars: vec![param; order],
        at: Some(arg),
        partial: false,
    })
}

fn parse_calculus(
    raw: &str,
    functions: &BTreeMap<String, UserFunction>,
    variables: &BTreeMap<String, f64>,
) -> Option<Intent> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    parse_derivative(text, functions, variables)
        .or_else(|| parse_integral(text, functions))
        .or_else(|| parse_limit(text))
}

/// Cheap syntactic check, true while a calculus line is still being typed.
pub fn is_calculus_input(text: &str) -> bool {
    static START: Lazy = Lazy::new(|| {
        re(
            r"^(?:∫|int(?:_|[^A-Za-z\s(]\S*\.\.)|d[²³]?\s*\/\s*d(?:[A-Za-z]|[α-ωΑ-Ω])|∂[²³]?\s*\/\s*∂|lim(?:it)?\b.*(?:->|→)|(?:the\s+)?(?:integral|integrate|(?:partial\s+)?derivative|limit)\s+of\b|[A-Za-z][A-Za-z0-9]*\s*(?:'+|′+|″)\s*\()",
            "i",
        )
    });
    let t = text.trim();
    jsre::test(&START, t) || parse_calculus(t, &BTreeMap::new(), &BTreeMap::new()).is_some()
}

// ---- expression trees

const DEG: &str = "deg__";
const TRIG: [&str; 6] = ["sin", "cos", "tan", "sec", "csc", "cot"];
const INVERSE_TRIG: [&str; 6] = ["asin", "acos", "atan", "asec", "acsc", "acot"];
/// Functions both mathjs `derivative` and the closed-form evaluator understand.
const SMOOTH: [&str; 27] = [
    "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "asec", "acsc", "acot",
    "sinh", "cosh", "tanh", "sech", "csch", "coth", "asinh", "acosh", "atanh", "sqrt", "cbrt",
    "nthRoot", "exp", "log", "log10",
];

fn smooth(name: &str) -> bool {
    SMOOTH.contains(&name) || name == "abs"
}

#[derive(Clone)]
struct TreeCtx<'a> {
    sci: &'a ScientificContext,
    bound: Option<String>,
    depth: u32,
    keep: Vec<String>,
}

fn deg_mode(sci: &ScientificContext) -> bool {
    sci.angle_mode != AngleMode::Rad
}

/// A preprocessed expression as a tree in radians: user functions inlined, known variables as numbers, `ln` as
/// mathjs's `log`, and in degree mode each trig call scaled by the `deg__` symbol.
fn to_tree(text: &str, ctx: &TreeCtx) -> Option<T> {
    if ctx.depth > 8 {
        return None;
    }
    let mut sci = ctx.sci.clone();
    for name in ctx.bound.iter().chain(&ctx.keep) {
        sci.variables.insert(name.clone(), 0.0);
    }
    let expr = preprocess_checked(text, &sci)?;
    let root = crate::syntax::parse(&expr).ok()?;
    convert(&root, ctx)
}

fn convert(n: &crate::syntax::Node, ctx: &TreeCtx) -> Option<T> {
    use crate::syntax::{Constant, Node};
    match n {
        Node::Parenthesis(c) => convert(c, ctx),
        Node::Constant(Constant::Number { value, .. }) => {
            value.is_finite().then(|| constant(*value))
        }
        Node::Symbol(name) => {
            if ctx.bound.as_deref() == Some(name) || ctx.keep.contains(name) {
                return Some(symbol(name));
            }
            if name == "pi" || name == "e" {
                return Some(symbol(name));
            }
            if name == "tau" {
                return Some(op(Op::Multiply, vec![constant(2.0), symbol("pi")]));
            }
            ctx.sci
                .variables
                .get(name)
                .filter(|v| v.is_finite())
                .map(|v| constant(*v))
        }
        Node::Operator { op: o, args, .. } => {
            if !matches!(
                o,
                Op::Add
                    | Op::Subtract
                    | Op::Multiply
                    | Op::Divide
                    | Op::Pow
                    | Op::UnaryMinus
                    | Op::UnaryPlus
            ) {
                return None;
            }
            let a = args
                .iter()
                .map(|x| convert(x, ctx))
                .collect::<Option<Vec<_>>>()?;
            Some(if *o == Op::UnaryPlus {
                a.into_iter().next()?
            } else {
                op(*o, a)
            })
        }
        Node::Function { args, .. } => {
            let a = args
                .iter()
                .map(|x| convert(x, ctx))
                .collect::<Option<Vec<_>>>()?;
            convert_call(crate::symbolic::function_name(n), a, ctx)
        }
        _ => None,
    }
}

fn convert_call(f: &str, args: Vec<T>, ctx: &TreeCtx) -> Option<T> {
    if let Some(user) = ctx.sci.functions.get(f) {
        return inline(user, args, ctx);
    }
    let name = if f == "ln" { "log" } else { f };
    if name == "log" && args.len() == 2 {
        return Some(op(
            Op::Divide,
            vec![
                call("log", vec![args[0].clone()]),
                call("log", vec![args[1].clone()]),
            ],
        ));
    }
    if !smooth(name) {
        return None;
    }
    if deg_mode(ctx.sci) {
        if TRIG.contains(&name) {
            return Some(call(
                name,
                vec![op(Op::Multiply, vec![args.first()?.clone(), symbol(DEG)])],
            ));
        }
        if INVERSE_TRIG.contains(&name) {
            return Some(op(Op::Divide, vec![call(name, args), symbol(DEG)]));
        }
    }
    Some(call(name, args))
}

/// A user function's body with its parameters replaced by the (already converted) arguments. Every use of a
/// parameter gets the same argument node, as in the TS (see `tree.rs`).
fn inline(def: &UserFunction, args: Vec<T>, ctx: &TreeCtx) -> Option<T> {
    if args.len() != def.params.len() {
        return None;
    }
    let placeholders: Vec<String> = (0..def.params.len())
        .map(|i| format!("pp{}q{i}", ctx.depth))
        .collect();
    let mut body = def.body.clone();
    for (i, p) in def.params.iter().enumerate() {
        let r = re(&format!("(?<![A-Za-z0-9_]){p}(?![A-Za-z0-9_])"), "");
        body = jsre::replace_all(&r, &body, |_| placeholders[i].clone()).into_owned();
    }
    let inner = TreeCtx {
        sci: ctx.sci,
        bound: None,
        depth: ctx.depth + 1,
        keep: placeholders.clone(),
    };
    let tree = to_tree(&body, &inner)?;
    let map: Vec<(String, T)> = placeholders.into_iter().zip(args).collect();
    Some(substitute(&tree, &map))
}

fn substitute(n: &T, map: &[(String, T)]) -> T {
    match &**n {
        N::Sym(s) => match map.iter().find(|(k, _)| k == s) {
            Some((_, v)) => v.clone(),
            None => n.clone(),
        },
        N::Op(o, a) => op(*o, a.iter().map(|x| substitute(x, map)).collect()),
        N::Call(name, a) => call(name, a.iter().map(|x| substitute(x, map)).collect()),
        N::Const(_) => n.clone(),
    }
}

fn mentions(n: &T, name: &str) -> bool {
    match &**n {
        N::Sym(s) => s == name,
        _ => n.args().iter().any(|a| mentions(a, name)),
    }
}

fn symbol_names(n: &T, out: &mut Vec<String>) {
    if let N::Sym(s) = &**n
        && s != "pi"
        && s != "e"
        && !out.contains(s)
    {
        out.push(s.clone());
    }
    for a in n.args() {
        symbol_names(a, out);
    }
}

// ---- symbolic derivatives

fn deg_value() -> T {
    op(Op::Divide, vec![symbol("pi"), constant(180.0)])
}

/// Radian trees back to what the user typed: `sin(x deg__)` is `sin(x)` again.
fn to_user_angles(n: &T, deg: bool) -> Option<T> {
    match &**n {
        N::Call(name, args) => {
            if TRIG.contains(&name.as_str()) && args.first().is_some_and(|a| mentions(a, DEG)) {
                let a = &args[0];
                let ok = matches!(&**a, N::Op(Op::Multiply, x) if x.get(1).and_then(|s| s.sym_name()) == Some(DEG));
                if !ok {
                    return None;
                }
                let inner = to_user_angles(&a.args()[0], deg)?;
                return Some(call(name, vec![inner]));
            }
            let inner = args
                .iter()
                .map(|x| to_user_angles(x, deg))
                .collect::<Option<Vec<_>>>()?;
            let back = call(name, inner);
            // a radian inverse trig value is the user's degrees value times deg__
            if deg && INVERSE_TRIG.contains(&name.as_str()) {
                return Some(op(Op::Multiply, vec![back, symbol(DEG)]));
            }
            Some(back)
        }
        N::Op(o, args) => {
            let a = args
                .iter()
                .map(|x| to_user_angles(x, deg))
                .collect::<Option<Vec<_>>>()?;
            Some(op(*o, a))
        }
        _ => Some(n.clone()),
    }
}

/// simplify folds `log(2)` into 0.693…; constant calls hide behind placeholder symbols while it runs. A call is
/// constant when it doesn't mention `v`, or with `v` None when it mentions no letter at all.
fn simplify_keeping_constants(n: &T, v: Option<&str>) -> Option<T> {
    fn hide(x: &T, v: Option<&str>, saved: &mut Vec<T>) -> T {
        if let N::Call(name, a) = &**x {
            // d/dx e^x comes back from mathjs as e^x log(e)
            if name == "log" && a.first().and_then(|s| s.sym_name()) == Some("e") {
                return constant(1.0);
            }
            let constant_call = match v {
                None => !has_symbol(x),
                Some(v) => !mentions(x, v),
            };
            if constant_call {
                saved.push(x.clone());
                return symbol(&format!("k{}__", saved.len() - 1));
            }
            return call(name, a.iter().map(|y| hide(y, v, saved)).collect());
        }
        match &**x {
            N::Op(o, a) => op(*o, a.iter().map(|y| hide(y, v, saved)).collect()),
            _ => x.clone(),
        }
    }
    fn show(x: &T, saved: &[T]) -> T {
        match &**x {
            N::Sym(s) => s
                .strip_prefix('k')
                .and_then(|r| r.strip_suffix("__"))
                .filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
                .and_then(|d| d.parse::<usize>().ok())
                .and_then(|i| saved.get(i).cloned())
                .unwrap_or_else(|| x.clone()),
            N::Op(o, a) => op(*o, a.iter().map(|y| show(y, saved)).collect()),
            N::Call(name, a) => call(name, a.iter().map(|y| show(y, saved)).collect()),
            N::Const(_) => x.clone(),
        }
    }
    let mut saved = Vec::new();
    let hidden = hide(n, v, &mut saved);
    let simplified = crate::symbolic::simplify(&tree::to_node(&hidden)).ok()?;
    Some(show(&tree::from_node(&simplified)?, &saved))
}

/// Text the engine reads back: natural log as `ln`.
fn engine_text(n: &T) -> String {
    fn rename(x: &T) -> T {
        match &**x {
            N::Call(name, a) => call(
                if name == "log" { "ln" } else { name },
                a.iter().map(rename).collect(),
            ),
            N::Op(o, a) => op(*o, a.iter().map(rename).collect()),
            _ => x.clone(),
        }
    }
    tree::text_auto(&rename(n))
}

const TIDY_SAMPLES: [[f64; 8]; 3] = [
    [0.83, 1.37, 0.61, 2.21, 0.47, 1.93, 1.28, 0.72],
    [1.61, 0.74, 2.58, 1.12, 0.45, 0.91, 2.67, 1.33],
    [-1.19, 2.31, -0.57, 1.46, -2.03, 0.66, -0.88, 1.74],
];

/// Same value wherever both are real; at least one sample has to be.
fn same_value(a: &T, b: &T) -> bool {
    let mut names = Vec::new();
    symbol_names(a, &mut names);
    symbol_names(b, &mut names);
    let mut compared = 0;
    for sample in &TIDY_SAMPLES {
        let scope: Vec<(String, f64)> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), sample[i % sample.len()]))
            .collect();
        let (Some(x), Some(y)) = (tree::evaluate(a, &scope), tree::evaluate(b, &scope)) else {
            return false;
        };
        let (Some(x), Some(y)) = (x, y) else { continue };
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        if (x - y).abs() > 1e-9 * x.abs().max(1.0) {
            return false;
        }
        compared += 1;
    }
    compared > 0
}

fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The one-denominator form when it reads shorter than what simplify gave.
fn tidy(n: T) -> T {
    let Some(t) = together(&n) else { return n };
    if !same_value(&n, &t) {
        return n;
    }
    if js_len(&pretty(&t).s) < js_len(&pretty(&n).s) {
        t
    } else {
        n
    }
}

struct Derived {
    tree: T,
    user: T,
}

/// d/dv of a radian tree; `user` is the same derivative written back in the user's angle mode.
fn differentiate(t: &T, v: &str, deg: bool) -> Option<Derived> {
    let d = tree::derivative(t, v).ok()?;
    let back = to_user_angles(&d, deg)?;
    let with_deg = substitute(&back, &[(DEG.to_string(), deg_value())]);
    let user = tidy(simplify_keeping_constants(&with_deg, Some(v))?);
    Some(Derived { tree: d, user })
}

// ---- evaluation

fn snap_int(n: f64) -> f64 {
    if !n.is_finite() || n == 0.0 {
        return n;
    }
    let r = jsnum::round(n);
    if r == 0.0 {
        return n;
    }
    if (n - r).abs() <= f64::EPSILON * r.abs().max(1.0) {
        r
    } else {
        n
    }
}

/// `compileScientific(text, ctx, v)` read as a real function: the number it gives, else NaN.
struct Compiled {
    node: crate::syntax::Node,
    scope: RefCell<crate::eval::Scope>,
    var: String,
    cost: u64,
}

impl Compiled {
    fn new(text: &str, sci: &ScientificContext, var: &str) -> Option<Compiled> {
        let mut ctx = sci.clone();
        ctx.variables.insert(var.to_string(), 0.0);
        let expr = preprocess_checked(text, &ctx)?;
        let node = crate::syntax::parse(&expr).ok()?;
        let mut scope = crate::scientific::scope_for(&expr, &ctx);
        // compileScientific runs outside evalScientific, where no typed i is in force
        scope.complex_typed = false;
        Some(Compiled {
            node,
            scope: RefCell::new(scope),
            var: var.to_string(),
            cost: 5 + js_len(&expr) as u64,
        })
    }

    fn call(&self, x: f64) -> f64 {
        self.scope
            .borrow_mut()
            .variables
            .insert(self.var.clone(), x);
        crate::work::spend(self.cost);
        let scope = self.scope.borrow().clone();
        let n = match crate::eval::evaluate(&self.node, &scope) {
            Ok(crate::eval::Val::Num(n)) if n.is_nan() => return f64::NAN,
            Ok(crate::eval::Val::Num(n)) => snap_int(n),
            Ok(crate::eval::Val::Bool(b)) => f64::from(u8::from(b)),
            Ok(crate::eval::Val::Complex { re, im }) => {
                if !re.is_finite() || !im.is_finite() || im.abs() > 1e-12 {
                    return f64::NAN;
                }
                if re.abs() < 1e-12 { 0.0 } else { re }
            }
            _ => return f64::NAN,
        };
        if n.is_finite() { n } else { f64::NAN }
    }
}

type RealFn = Box<dyn Fn(f64) -> f64>;

fn real_fn(text: &str, sci: &ScientificContext, v: &str) -> Option<RealFn> {
    let c = Compiled::new(text, sci, v)?;
    Some(Box::new(move |x| c.call(x)))
}

/// The body as one expression with user functions inlined, which evaluates far faster than nested calls.
fn body_fn(body: &str, sci: &ScientificContext, v: &str) -> Option<RealFn> {
    let calls_user = sci.functions.keys().any(|n| {
        let r = re(&format!(r"(?<![A-Za-z0-9_]){n}\s*\("), "");
        jsre::test(&r, body)
    });
    if calls_user {
        let ctx = TreeCtx {
            sci,
            bound: Some(v.to_string()),
            depth: 0,
            keep: Vec::new(),
        };
        if let Some(t) = to_tree(body, &ctx)
            && let Some(user) = to_user_angles(&t, deg_mode(sci))
        {
            let flat = substitute(&user, &[(DEG.to_string(), deg_value())]);
            if let Some(fast) = real_fn(&engine_text(&flat), sci, v) {
                return Some(fast);
            }
        }
    }
    real_fn(body, sci, v)
}

static INFINITY: Lazy = Lazy::new(|| re(r"^([+-]?)\s*(?:∞|inf|infty|infinity)$", "i"));

fn bound_value(text: &str, sci: &ScientificContext) -> Option<f64> {
    if let Some(m) = caps(&INFINITY, text.trim()) {
        return Some(if g(&m, 1) == Some("-") {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    eval_scientific(text, sci)
        .filter(|r| r.kind == ValueKind::Number && r.n.is_finite())
        .map(|r| r.n)
}

fn job_ast(n: &T, v: Option<&str>) -> Option<Ast> {
    match &**n {
        N::Const(x) => Some(Ast::Num(jsnum::to_string(*x))),
        N::Sym(s) => {
            if Some(s.as_str()) == v {
                Some(Ast::Var)
            } else if s == "pi" || s == "e" {
                Some(Ast::Const(s.clone()))
            } else if s == DEG {
                Some(Ast::Op(
                    "/".into(),
                    vec![Ast::Const("pi".into()), Ast::Num("180".into())],
                ))
            } else {
                None
            }
        }
        N::Op(o, a) => {
            let args = a
                .iter()
                .map(|x| job_ast(x, v))
                .collect::<Option<Vec<_>>>()?;
            let s = match o {
                Op::UnaryMinus => "neg",
                Op::Add => "+",
                Op::Subtract => "-",
                Op::Multiply => "*",
                Op::Divide => "/",
                Op::Pow => "^",
                _ => return None,
            };
            Some(Ast::Op(s.into(), args))
        }
        N::Call(name, a) => {
            let args = a
                .iter()
                .map(|x| job_ast(x, v))
                .collect::<Option<Vec<_>>>()?;
            Some(Ast::Fn(name.clone(), args))
        }
    }
}

fn bound_ast(text: &str, ctx: &TreeCtx) -> Option<Bound> {
    if let Some(m) = caps(&INFINITY, text.trim()) {
        return Some(if g(&m, 1) == Some("-") {
            Bound::NegInf
        } else {
            Bound::Inf
        });
    }
    let t = to_tree(
        text,
        &TreeCtx {
            bound: None,
            ..ctx.clone()
        },
    )?;
    job_ast(&t, None).map(Bound::Ast)
}

fn with_job(
    value: f64,
    err: f64,
    draft: Option<(Ast, JobKind)>,
    sci: &ScientificContext,
) -> CalculusResult {
    let mut result = CalculusResult {
        value: Some(Value::number(value)),
        ..CalculusResult::default()
    };
    // integers already are their own closed form
    let Some((f, kind)) = draft else {
        return result;
    };
    if !value.is_finite() || jsnum::is_integer(value) || value.abs() > 1e12 {
        return result;
    }
    let job = Job {
        f,
        kind,
        approx: value,
        err,
        rad: sci.angle_mode == AngleMode::Rad,
    };
    if let Some(known) = closed_form::known_closed_form(&job.key()) {
        result.exact = known.filter(|e| !e.is_empty());
        return result;
    }
    result.job = Some(job.to_json());
    result
}

fn free_letters(body: &str, sci: &ScientificContext) -> Vec<String> {
    static FREE: Lazy = Lazy::new(|| re(r"(?<![A-Za-z0-9_])([A-Za-z])(?![A-Za-z0-9_(])", ""));
    let mut found: Vec<String> = Vec::new();
    for m in FREE.captures_iter(body).flatten() {
        let c = g(&m, 1).unwrap_or("").to_string();
        // a bare function name isn't a variable: after f(x) = x², ∫0..3 f isn't ∫0..3 f df
        if c == "e" || c == "i" || sci.variables.contains_key(&c) || sci.functions.contains_key(&c)
        {
            continue;
        }
        if !found.contains(&c) {
            found.push(c);
        }
    }
    found
}

fn pick_variable(body: &str, sci: &ScientificContext, given: Option<&str>) -> Option<String> {
    if let Some(v) = given.filter(|v| !v.is_empty()) {
        return Some(v.to_string());
    }
    let free = free_letters(body, sci);
    if free.iter().any(|f| f == "x") || free.is_empty() {
        return Some("x".into());
    }
    (free.len() == 1).then(|| free[0].clone())
}

/// A derivative must agree with finite differences wherever both exist, or it isn't shown.
fn agrees_numerically(f: &dyn Fn(f64) -> f64, df: &dyn Fn(f64) -> f64, points: &[f64]) -> bool {
    for &p in points {
        let want = derivative_at(f, p);
        let got = df(p);
        let Some(want) = want else { continue };
        if !got.is_finite() || !want.value.is_finite() {
            continue;
        }
        if want.err > 1e-6 * want.value.abs().max(1.0) {
            continue;
        }
        if (got - want.value).abs() > 1e-6 * got.abs().max(1.0) + 10.0 * want.err {
            return false;
        }
    }
    true
}

const CHECK_POINTS: [f64; 8] = [0.7, 1.3, 2.9, -0.6, 4.2, 11.5, 0.2, -3.1];

static SYMBOL_RE: Lazy = Lazy::new(|| re(&format!(r"(?<![{LETTER}])({VAR})(?![{LETTER}(])"), ""));
static WORD_RE: Lazy = Lazy::new(|| {
    re(
        &format!(r"(?<![{LETTER}\\.])[A-Za-zα-ωΑ-Ω]{{2,}}(?![{LETTER}.]|\s*\()"),
        "",
    )
});

fn whole_word(w: &str) -> bool {
    let lower = w.to_lowercase();
    NAMES.to_lowercase().split('|').any(|n| n == lower)
        || [
            "e", "i", "theta", "mod", "deg", "rad", "and", "or", "not", "xor",
        ]
        .contains(&lower.as_str())
}

/// `Lv` in a derivative is L times v: an unknown word of letters is a product of one-letter constants.
fn split_symbols(body: &str, sci: &ScientificContext) -> String {
    jsre::replace_all(&WORD_RE, body, |m| {
        let word = g(m, 0).unwrap_or("");
        if whole_word(word) || sci.variables.contains_key(word) || sci.functions.contains_key(word)
        {
            return word.to_string();
        }
        word.chars().map(String::from).collect::<Vec<_>>().join(" ")
    })
    .into_owned()
}

/// Letters, Greek letters included, that aren't a known variable, function or constant, in order of appearance.
fn free_symbols(body: &str, sci: &ScientificContext) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for m in SYMBOL_RE.captures_iter(body).flatten() {
        let c = g(&m, 1).unwrap_or("").to_string();
        if c == "e" || c == "i" || sci.variables.contains_key(&c) || sci.functions.contains_key(&c)
        {
            continue;
        }
        if !found.contains(&c) {
            found.push(c);
        }
    }
    found
}

/// `at x = 1, y = 2`, `at (x, y) = (1, 2)`, or a bare `at 2` when there is one variable.
fn read_point(text: &str, vars: &[String]) -> Option<Vec<(String, String)>> {
    static NAMED: Lazy = Lazy::new(|| re(&format!(r"^({VAR})\s*=(?!=)\s*(.+)$"), "s"));
    static TUPLE: Lazy = Lazy::new(|| re(r"^\((.+)\)\s*=\s*\((.+)\)$", "s"));
    static ONE: Lazy = Lazy::new(|| re(&format!("^{VAR}$"), ""));
    static AND: Lazy = Lazy::new(|| re(r"\s+and\s+", "i"));
    let t = text.trim();
    if let Some(m) = caps(&TUPLE, t) {
        let names = split_args(g(&m, 1).unwrap_or(""));
        let values = split_args(g(&m, 2).unwrap_or(""));
        if names.len() != values.len()
            || !names.iter().all(|n| jsre::test(&ONE, n))
            || !values.iter().all(|v| !v.is_empty())
        {
            return None;
        }
        // Object.fromEntries: a repeated name keeps its first place and its last value
        let mut out: Vec<(String, String)> = Vec::new();
        for (n, v) in names.into_iter().zip(values) {
            match out.iter_mut().find(|(k, _)| *k == n) {
                Some(slot) => slot.1 = v,
                None => out.push((n, v)),
            }
        }
        return Some(out);
    }
    let parts: Vec<String> = split_args(t)
        .iter()
        .flat_map(|p| {
            AND.split(p)
                .filter_map(Result::ok)
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    let pairs: Vec<Option<(String, String)>> = parts
        .iter()
        .map(|p| {
            caps(&NAMED, p).map(|m| {
                (
                    g(&m, 1).unwrap_or("").to_string(),
                    g(&m, 2).unwrap_or("").trim().to_string(),
                )
            })
        })
        .collect();
    if pairs.iter().all(Option::is_some) {
        let mut out: Vec<(String, String)> = Vec::new();
        for (k, v) in pairs.into_iter().flatten() {
            if out.iter().any(|(x, _)| *x == k) {
                return None;
            }
            out.push((k, v));
        }
        return Some(out);
    }
    let mut distinct: Vec<&String> = Vec::new();
    for v in vars {
        if !distinct.contains(&v) {
            distinct.push(v);
        }
    }
    if distinct.len() == 1 && parts.len() == 1 && pairs[0].is_none() {
        return Some(vec![(vars[0].clone(), t.to_string())]);
    }
    None
}

fn run_derivative(
    body: &str,
    vars: &[String],
    at: Option<&str>,
    partial: bool,
    ctx: &CalcContext,
) -> Option<CalculusResult> {
    let sci = &ctx.sci;
    // d/dt of letters that aren't t (`x sin θ`) is a time derivative: they move with time
    if !partial
        && at.is_none()
        && vars.len() <= 2
        && vars.iter().all(|v| v == "t")
        && let Some(timed) = run_time_derivative(body, vars.len(), ctx)
    {
        return Some(timed);
    }
    let body = split_symbols(body, sci);
    let free = free_symbols(&body, sci);
    let first = match vars.first().filter(|v| !v.is_empty()) {
        Some(v) => v.clone(),
        None => {
            if free.iter().any(|f| f == "x") || free.is_empty() {
                "x".to_string()
            } else if free.len() == 1 {
                free[0].clone()
            } else {
                return None;
            }
        }
    };
    let vars: Vec<String> = vars
        .iter()
        .map(|v| {
            if v.is_empty() {
                first.clone()
            } else {
                v.clone()
            }
        })
        .collect();
    let point = match at {
        None => Vec::new(),
        Some(a) => read_point(a, &vars)?,
    };
    let mut symbols: Vec<String> = Vec::new();
    for s in vars.iter().chain(&free) {
        if !symbols.contains(s) {
            symbols.push(s.clone());
        }
    }
    if symbols.len() == 1 {
        let at_text = point
            .iter()
            .find(|(k, _)| *k == first)
            .map(|(_, v)| v.as_str());
        return run_ordinary(&body, &first, vars.len(), at_text, sci);
    }
    run_partial(&body, &vars, &symbols, &point, sci)
}

/// Sample values for the letters held constant while a partial derivative is checked.
const SAMPLE_SETS: [[f64; 7]; 2] = [
    [0.83, 1.37, -0.61, 2.21, 0.47, 1.93, -1.28],
    [1.61, -0.74, 0.58, 1.12, -1.45, 0.91, 2.67],
];

fn with_vars(sci: &ScientificContext, extra: &[(String, f64)]) -> ScientificContext {
    let mut s = sci.clone();
    for (k, v) in extra {
        s.variables.insert(k.clone(), *v);
    }
    s
}

/// A derivative in one of several letters, the others held constant; symbolic unless `at` gives every letter a
/// value.
fn run_partial(
    body: &str,
    vars: &[String],
    symbols: &[String],
    point: &[(String, String)],
    sci: &ScientificContext,
) -> Option<CalculusResult> {
    let deg = deg_mode(sci);
    let t = to_tree(
        body,
        &TreeCtx {
            sci,
            bound: None,
            depth: 0,
            keep: symbols.to_vec(),
        },
    )?;
    let mut current = t;
    let mut user: Option<T> = None;
    let mut texts = vec![body.to_string()];
    for v in vars {
        let d = differentiate(&current, v, deg)?;
        let d_text = engine_text(&d.user);
        for sample in &SAMPLE_SETS {
            let held: Vec<(String, f64)> = symbols
                .iter()
                .filter(|s| *s != v)
                .enumerate()
                .map(|(i, s)| (s.clone(), sample[i % sample.len()]))
                .collect();
            let sctx = with_vars(sci, &held);
            let f = real_fn(texts.last()?, &sctx, v)?;
            let df = real_fn(&d_text, &sctx, v)?;
            if !agrees_numerically(&f, &df, &CHECK_POINTS) {
                return None;
            }
        }
        current = d.tree;
        user = Some(d.user);
        texts.push(d_text);
    }
    let user = user?;
    if point.is_empty() {
        return Some(text_result(pretty(&user).s));
    }
    if point.iter().any(|(n, _)| !symbols.contains(n)) {
        return None;
    }
    let mut values: Vec<(String, f64)> = Vec::new();
    for (n, text) in point {
        values.push((n.clone(), bound_value(text, sci)?));
    }
    let vctx = with_vars(sci, &values);
    if symbols.iter().all(|s| values.iter().any(|(n, _)| n == s)) {
        // the function and each lower derivative have to exist there too
        let mut r = None;
        for t in &texts {
            let v = eval_scientific(t, &vctx)?;
            if v.kind != ValueKind::Number || !v.n.is_finite() {
                return None;
            }
            r = Some(v);
        }
        return Some(CalculusResult {
            value: r,
            ..CalculusResult::default()
        });
    }
    let map: Vec<(String, T)> = values
        .iter()
        .map(|(n, v)| (n.clone(), constant(*v)))
        .collect();
    let at = substitute(&user, &map);
    // simplify would make (v²)^(3/2) into v³
    let simplified = match together(&at) {
        Some(whole) if same_value(&at, &whole) => whole,
        _ => simplify_keeping_constants(&at, None)?,
    };
    Some(text_result(pretty(&simplified).s))
}

fn text_result(s: String) -> CalculusResult {
    CalculusResult {
        value: Some(Value::text(s)),
        ..CalculusResult::default()
    }
}

fn run_ordinary(
    body: &str,
    v: &str,
    order: usize,
    at_text: Option<&str>,
    sci: &ScientificContext,
) -> Option<CalculusResult> {
    let tctx = TreeCtx {
        sci,
        bound: Some(v.to_string()),
        depth: 0,
        keep: Vec::new(),
    };
    let t = to_tree(body, &tctx);
    let f = real_fn(body, sci, v)?;
    let at = match at_text {
        Some(a) => Some(bound_value(a, sci).filter(|x| x.is_finite())?),
        None => None,
    };
    // no derivative where the function itself is undefined (ln at -1)
    if let Some(a) = at
        && !f(a).is_finite()
    {
        return None;
    }
    let check_at: Vec<f64> = match at {
        Some(a) => vec![a],
        None => CHECK_POINTS.to_vec(),
    };
    let deg = deg_mode(sci);
    if let Some(t) = t {
        let mut steps: Option<Derived> = None;
        let mut current = t;
        let mut prev_fn: RealFn = f;
        for _ in 0..order {
            // each lower derivative has to exist there too: abs'' is 0 either side of 0 but abs' has no value at 0
            if let Some(a) = at
                && !prev_fn(a).is_finite()
            {
                return None;
            }
            let Some(d) = differentiate(&current, v, deg) else {
                steps = None;
                break;
            };
            let df = real_fn(&engine_text(&d.user), sci, v)?;
            if !agrees_numerically(&prev_fn, &df, &check_at) {
                return None;
            }
            current = d.tree.clone();
            steps = Some(d);
            prev_fn = df;
        }
        if let Some(steps) = &steps {
            if let Some(a) = at {
                let value = prev_fn(a);
                if !value.is_finite() {
                    return None;
                }
                let at_tree = to_tree(
                    at_text?,
                    &TreeCtx {
                        bound: None,
                        ..tctx.clone()
                    },
                );
                let d_ast = job_ast(&steps.tree, Some(v));
                let at_ast = at_tree.and_then(|x| job_ast(&x, None));
                let draft = match (d_ast, at_ast) {
                    (Some(f), Some(at)) => Some((f, JobKind::Value { at })),
                    _ => None,
                };
                return Some(with_job(value, 1e-13 * value.abs(), draft, sci));
            }
            return Some(text_result(pretty(&steps.user).s));
        }
        // no symbolic form (floor, mod, …): a first derivative at a point can still be numeric
        let a = at?;
        if order != 1 {
            return None;
        }
        // with one differentiation asked for, `prev_fn` is still the function itself
        return numeric_derivative(&prev_fn, a);
    }
    let a = at?;
    if order != 1 {
        return None;
    }
    let f = real_fn(body, sci, v)?;
    numeric_derivative(&f, a)
}

fn numeric_derivative(f: &dyn Fn(f64) -> f64, a: f64) -> Option<CalculusResult> {
    let est = derivative_at(f, a)?;
    if est.err > 1e-7 * est.value.abs().max(1.0) {
        return None;
    }
    let value = justified(est.value, est.err, Some(est.value.abs().max(1.0)))?;
    Some(CalculusResult {
        value: Some(Value::number(value)),
        ..CalculusResult::default()
    })
}

fn run_integral(
    body: &str,
    given: Option<&str>,
    lower: &str,
    upper: &str,
    sci: &ScientificContext,
) -> Option<CalculusResult> {
    let v = pick_variable(body, sci, given)?;
    let f = body_fn(body, sci, &v);
    let a = bound_value(lower, sci);
    let b = bound_value(upper, sci);
    let (f, a, b) = (f?, a?, b?);
    let est = integrate(&f, a, b)?;
    let tctx = TreeCtx {
        sci,
        bound: Some(v.clone()),
        depth: 0,
        keep: Vec::new(),
    };
    let f_ast = to_tree(body, &tctx).and_then(|t| job_ast(&t, Some(&v)));
    let lo = bound_ast(lower, &tctx);
    let hi = bound_ast(upper, &tctx);
    let draft = match (f_ast, lo, hi) {
        (Some(f), Some(a), Some(b)) => Some((f, JobKind::Integral { a, b })),
        _ => None,
    };
    Some(with_job(est.value, est.err, draft, sci))
}

fn run_limit(
    body: &str,
    v: &str,
    to: &str,
    side: i32,
    sci: &ScientificContext,
) -> Option<CalculusResult> {
    let f = body_fn(body, sci, v);
    let to_v = bound_value(to, sci);
    let (f, to_v) = (f?, to_v?);
    let est = limit(&f, to_v, side)?;
    if !est.value.is_finite() {
        return Some(CalculusResult {
            value: Some(Value::number(est.value)),
            ..CalculusResult::default()
        });
    }
    let tctx = TreeCtx {
        sci,
        bound: Some(v.to_string()),
        depth: 0,
        keep: Vec::new(),
    };
    let f_ast = to_tree(body, &tctx).and_then(|t| job_ast(&t, Some(v)));
    let to_ast = bound_ast(to, &tctx);
    let draft = match (f_ast, to_ast) {
        (Some(f), Some(to)) => Some((f, JobKind::Limit { to, side })),
        _ => None,
    };
    Some(with_job(est.value, est.err, draft, sci))
}

/// `evaluateCalculus`: None when `text` isn't calculus; a None value when it is but has no trustworthy answer.
pub fn evaluate_calculus(text: &str, ctx: &CalcContext) -> Option<CalculusResult> {
    let intent = parse_calculus(text, &ctx.sci.functions, &ctx.sci.variables)?;
    let out = match &intent {
        Intent::Derivative {
            body,
            vars,
            at,
            partial,
        } => run_derivative(body, vars, at.as_deref(), *partial, ctx),
        Intent::Integral {
            body,
            v,
            lower,
            upper,
        } => run_integral(body, v.as_deref(), lower, upper, &ctx.sci),
        Intent::Limit { body, v, to, side } => run_limit(body, v, to, *side, &ctx.sci),
    };
    Some(out.unwrap_or_default())
}

// ---- time derivatives: d/dt (x sin θ) = ẋ sin θ + x cos θ θ̇

/// The physics convention: positions and angles move, other letters are constants.
const VARYING_BY_DEFAULT: [&str; 12] = [
    "x", "y", "z", "r", "s", "q", "u", "v", "w", "theta", "φ", "ψ",
];
const GREEK_WORDS: [(&str, &str); 6] = [
    ("phi", "φ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
];
// stand-ins for ẋ, θ̈ and the rest while mathjs works: Greek capitals nobody types
const STAND_INS: [&str; 11] = ["Ξ", "Ψ", "Γ", "Λ", "Υ", "Ζ", "Η", "Ι", "Κ", "Μ", "Ν"];
const MARKS: [&str; 4] = ["", "\u{307}", "\u{308}", "\u{20db}"];

fn greek_word(w: &str) -> Option<&'static str> {
    GREEK_WORDS.iter().find(|(k, _)| *k == w).map(|(_, v)| *v)
}

/// A letter as the answer shows it: θ for theta.
fn shown_letter(name: &str) -> String {
    if name == "theta" {
        "θ".into()
    } else {
        name.to_string()
    }
}

/// d/dt or d²/dt² of an expression whose letters move with time. None when there's nothing to move.
fn run_time_derivative(text: &str, order: usize, ctx: &CalcContext) -> Option<CalculusResult> {
    static GREEK: Lazy = Lazy::new(|| re(r"\b(phi|psi|omega|alpha|beta|gamma)\b", ""));
    static DOTTED: Lazy = Lazy::new(|| {
        re(
            r"(?<![A-Za-z])(theta|phi|psi|omega|alpha|beta|gamma|[A-Za-zθφψωαβγ])(d{1,3})ot(?![A-Za-z])",
            "",
        )
    });
    static THETA: Lazy = Lazy::new(|| re("theta", ""));
    static EXPLICIT_T: Lazy = Lazy::new(|| re(r"(?<![A-Za-z])t(?![A-Za-z(])", ""));
    let sci = &ctx.sci;
    let padded = format!(" {text} ");
    let body = jsre::replace_all(&GREEK, &padded, |m| {
        greek_word(g(m, 1).unwrap_or("")).unwrap_or("").to_string()
    })
    .into_owned();
    let mut free: Vec<&str> = STAND_INS
        .iter()
        .copied()
        .filter(|c| !body.contains(c))
        .collect();
    // each dotted name, and each derivative of a moving letter, gets a stand-in (insertion-ordered)
    let mut stands: Vec<(String, String, usize)> = Vec::new();
    let mut stand_for =
        |base: &str, k: usize, stands: &mut Vec<(String, String, usize)>| -> Option<String> {
            if let Some((c, _, _)) = stands.iter().find(|(_, b, o)| b == base && *o == k) {
                return Some(c.clone());
            }
            if free.is_empty() {
                return None;
            }
            let c = free.remove(0).to_string();
            stands.push((c.clone(), base.to_string(), k));
            Some(c)
        };
    let mut missing = false;
    let body = jsre::replace_all(&DOTTED, &body, |m| {
        let base = g(m, 1).unwrap_or("");
        let ds = g(m, 2).unwrap_or("").chars().count();
        let name = greek_word(base).map_or_else(
            || {
                if base == "θ" {
                    "theta".to_string()
                } else {
                    base.to_string()
                }
            },
            str::to_string,
        );
        match stand_for(&name, ds, &mut stands) {
            Some(c) => format!(" {c} "),
            None => {
                missing = true;
                " null ".into()
            }
        }
    })
    .into_owned();
    if missing {
        return None;
    }
    // `ktheta` is k times θ, not six letters
    let split = split_symbols(&jsre::replace_all(&THETA, &body, |_| " theta ".into()), sci);
    let letters: Vec<String> = free_symbols(&split, sci)
        .into_iter()
        .filter(|n| !STAND_INS.contains(&n.as_str()) && n != "t")
        .collect();
    let mut dotted_bases: Vec<String> = Vec::new();
    for (_, b, _) in &stands {
        if !dotted_bases.contains(b) {
            dotted_bases.push(b.clone());
        }
    }
    if letters.is_empty() && stands.is_empty() {
        return None;
    }
    let varies = |name: &str| {
        dotted_bases.iter().any(|b| b == name)
            || ctx
                .time_varying
                .get(&shown_letter(name))
                .copied()
                .unwrap_or_else(|| VARYING_BY_DEFAULT.contains(&name))
    };
    let moving: Vec<String> = letters.iter().filter(|n| varies(n)).cloned().collect();
    let still: Vec<String> = letters.iter().filter(|n| !varies(n)).cloned().collect();
    let explicit_t = jsre::test(&EXPLICIT_T, &split);
    let mut rsci = sci.clone();
    rsci.angle_mode = AngleMode::Rad;
    let mut keep = letters.clone();
    keep.extend(stands.iter().map(|(c, _, _)| c.clone()));
    keep.push("t".into());
    let mut t = to_tree(
        &split,
        &TreeCtx {
            sci: &rsci,
            bound: None,
            depth: 0,
            keep,
        },
    )?;
    // a letter only ever written dotted (`1/2 m xdot^2`) moves too, and is listed so
    let only_dotted: Vec<String> = dotted_bases
        .iter()
        .filter(|b| !moving.contains(b))
        .cloned()
        .collect();
    let time = TimeLetters {
        varying: moving
            .iter()
            .chain(&only_dotted)
            .map(|n| shown_letter(n))
            .collect(),
        constant: still.iter().map(|n| shown_letter(n)).collect(),
        locked: dotted_bases.iter().map(|n| shown_letter(n)).collect(),
    };
    for _ in 0..order {
        let mut terms: Vec<T> = Vec::new();
        let mut now = Vec::new();
        symbol_names(&t, &mut now);
        if explicit_t && now.iter().any(|s| s == "t") {
            terms.push(tree::derivative(&t, "t").ok()?);
        }
        let qs: Vec<String> = moving
            .iter()
            .cloned()
            .chain(stands.iter().map(|(c, _, _)| c.clone()))
            .collect();
        for q in qs {
            if !now.contains(&q) {
                continue;
            }
            let known = stands
                .iter()
                .find(|(c, _, _)| *c == q)
                .map(|(_, b, o)| (b.clone(), *o));
            let (base, k) = known.unwrap_or_else(|| (q.clone(), 0));
            let next = stand_for(&base, k + 1, &mut stands);
            let next = next?;
            if k + 1 >= MARKS.len() {
                return None;
            }
            let d = tree::derivative(&t, &q).ok()?;
            terms.push(op(Op::Multiply, vec![d, symbol(&next)]));
        }
        let sum = terms
            .into_iter()
            .reduce(|a, b| op(Op::Add, vec![a, b]))
            .unwrap_or_else(|| constant(0.0));
        t = tidy(simplify_keeping_constants(&sum, None)?);
    }
    let mut shown = pretty(&t).s;
    for (c, base, k) in &stands {
        shown = shown.replace(c.as_str(), &format!("{}{}", shown_letter(base), MARKS[*k]));
    }
    use unicode_normalization::UnicodeNormalization;
    // one character where there is one (ẋ, not x and a combining dot); θ̇ has none and keeps its mark
    let nfc: String = shown.nfc().collect();
    Some(CalculusResult {
        value: Some(Value::text(nfc)),
        time: Some(time),
        ..CalculusResult::default()
    })
}

#[cfg(test)]
mod tests {
    use crate::types::{AngleMode, EvaluateOptions};

    fn sheet(lines: &[&str]) -> Vec<String> {
        let options = EvaluateOptions {
            angle_mode: Some(AngleMode::Rad),
            ..EvaluateOptions::default()
        };
        let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        crate::evaluate_sheet(&lines, &options)
            .into_iter()
            .map(|r| r.display)
            .collect()
    }

    /// Measured in the TS: a parameter used twice in a product is one mathjs node, so its derivative is blank.
    #[test]
    fn user_functions_differentiate_as_the_ts_does() {
        assert_eq!(
            sheet(&[
                "f(t) = t*t",
                "d/dx f(x)",
                "f'(3)",
                "f'(x)",
                "∫0..1 f(x)",
                "d/dx f(x)^2",
                "f''(x)"
            ])[1..],
            ["", "", "", "0.333333333333", "", ""]
        );
        assert_eq!(
            sheet(&["g(t)=t*sin(t)", "g'(x)", "g'(2)"])[1..],
            ["sin(x) + x cos(x)", "0.0770037537314"]
        );
        assert_eq!(sheet(&["h(t)=t*t*t", "h'(x)", "h'(2)"])[1..], ["", ""]);
        assert_eq!(sheet(&["k(t)=t^2*t", "k'(x)"])[1..], ["3x²"]);
        assert_eq!(
            sheet(&["p(a,b)=a*b", "d/dx p(x,x)", "∂/∂x p(x,y)"])[1..],
            ["2x", "y"]
        );
    }
}

#[cfg(test)]
mod probe {
    /// `CALC_PROBE=sheets.json cargo test calc_probe -- --ignored --nocapture` prints each sheet's displays, in the
    /// format of a TS run of the same file, for comparing the two by hand.
    #[test]
    #[ignore]
    fn calc_probe() {
        let Ok(path) = std::env::var("CALC_PROBE") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let sheets: Vec<(Vec<String>, crate::types::EvaluateOptions)> =
            serde_json::from_str(&text).unwrap();
        for (lines, o) in sheets {
            let out: Vec<(String, Option<String>)> = crate::evaluate_sheet(&lines, &o)
                .into_iter()
                .map(|r| (r.display, r.exact))
                .collect();
            println!(
                "{} => {}",
                serde_json::to_string(&lines).unwrap(),
                serde_json::to_string(&out).unwrap()
            );
        }
    }
}
