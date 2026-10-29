//! Σ and Π (`src/engine/sums.ts`): the spellings (`sum n^2, n=1..10`, `Σ_(n=1)^10 n^2`, `sum(n^2, n, 1, 10)`,
//! `sum from n = 1 to 10 of n^2`), exact finite sums (closed polynomial sums, exact rationals, then compensated
//! floats), and infinite series whose convergence their shape settles (geometric, p-series through Hurwitz ζ,
//! alternating, exponential), with closed forms such as `pi^2/6`.
//!
//! Text positions are byte offsets into one string at a time; every character the scans test is ASCII, so they
//! agree with the TS's UTF-16 indexes wherever it matters.

// `!(x > 0.0)` is the TS test, true for NaN too
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{FromPrimitive, One, Signed, ToPrimitive, Zero};
use serde_json::{Value as Json, json};

use crate::eval::{self, Scope, Val};
use crate::jsnum;
use crate::jsre::{self, Lazy, Regex, js};
use crate::plain_math::{PlainContext, try_plain_math};
use crate::scientific::ScientificContext;
use crate::scientific::compile::{Compiled, compile_scientific, parse_scientific};
use crate::scientific::functions::jsmath;
use crate::simplify::{ExactFormOptions, exact_form, wants_exact_form};
use crate::syntax::{Constant, Node, Op};
use crate::types::{DefaultUnits, Value, ValueKind};
use crate::work::{WorkBudget, spend};

/// What `sumAnswer` reads besides the text (`SumContext`).
#[derive(Clone, Debug, Default)]
pub struct SumContext {
    pub sci: ScientificContext,
    pub default_units: DefaultUnits,
    pub rationalize: Option<bool>,
}

/// A Σ/Π answer: `value` None is a blank line.
#[derive(Clone, Debug, Default)]
pub struct SumAnswer {
    pub value: Option<Value>,
    pub exact: Option<String>,
}

#[derive(Clone, Debug)]
struct Spec {
    op_sum: bool,
    body: String,
    index: Option<String>,
    from: String,
    to: String,
}

// work caps per keystroke; past them the answer is blank, never a partial sum
const BUDGET: u64 = 2_500_000;
const EXACT_NODE_COST: u64 = 10;
const EXACT_TERMS: f64 = 5_000.0;
const FLOAT_TERMS: usize = 200_000;
const PROD_FLOAT_TERMS: f64 = 1_000.0;
const MAX_BOUND: f64 = 1e15;
const MAX_EXACT_TEXT: usize = 24;
const EPS: f64 = f64::EPSILON;

/// `Math.max` of two: NaN wins, as in JS.
fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Typed Σ and Π read exactly like the words `sum` and `prod`.
pub fn normalize_sums(text: &str) -> String {
    if !text.contains(['Σ', '∑', 'Π', '∏']) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        match c {
            'Σ' | '∑' | 'Π' | '∏' => {
                out.push_str(if c == 'Σ' || c == '∑' {
                    "sum"
                } else {
                    "prod"
                });
                if chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric) {
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

static HINT_RE: Lazy =
    Lazy::new(|| js(r"(?<![A-Za-z_])\\?(?:sum|prod|product)(?![A-Za-z0-9])", "i"));
static LEAD_RE: Lazy = Lazy::new(|| js(r"^\\?(sum|prod|product)(?![A-Za-z0-9])\s*", "i"));
static CALL_RE: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_\\])(sum|prod|product)\s*\(", "i"));
static SCRIPT_RE: Lazy =
    Lazy::new(|| js(r"(?<![A-Za-z_\\])(sum|prod|product)(?![A-Za-z0-9])\s*", "i"));

fn closer(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// The byte index of the bracket closing the one at `open`, or None when they don't pair.
fn close_of(s: &str, open: usize) -> Option<usize> {
    let mut stack: Vec<char> = Vec::new();
    for (i, ch) in s[open..].char_indices() {
        if let Some(c) = closer(ch) {
            stack.push(c);
        } else if matches!(ch, ')' | ']' | '}') {
            if stack.pop() != Some(ch) {
                return None;
            }
            if stack.is_empty() {
                return Some(open + i);
            }
        }
    }
    None
}

/// Top-level pieces between commas, outside any brackets.
fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i64;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if depth == 0 && ch == ',' {
            out.push(s[start..i].to_string());
            start = i + 1;
        }
    }
    out.push(s[start..].to_string());
    out
}

#[derive(Clone, Copy)]
struct Cut {
    at: usize,
    len: usize,
}

/// Top-level matches of `re` (a comma, or a word like ` for `).
fn top_matches(s: &str, re: &Regex) -> Vec<Cut> {
    let mut out = Vec::new();
    // the bracket depth before each match, counted on from the last one (matches come in order)
    let (mut depth, mut counted) = (0i64, 0);
    for m in re.find_iter(s).flatten() {
        for ch in s[counted..m.start()].chars() {
            if "([{".contains(ch) {
                depth += 1;
            } else if ")]}".contains(ch) {
                depth -= 1;
            }
        }
        counted = m.start();
        if depth == 0 {
            out.push(Cut {
                at: m.start(),
                len: m.end() - m.start(),
            });
        }
    }
    out
}

fn is_ws(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn has_ws(s: &str) -> bool {
    s.chars().any(is_ws)
}

fn jtrim(s: &str) -> &str {
    s.trim_matches(is_ws)
}

struct Range {
    index: Option<String>,
    from: String,
    to: String,
}

fn parse_range(text: &str) -> Option<Range> {
    static BRACKET: Lazy = Lazy::new(|| js(r"^([A-Za-z]|theta)\s*(?:=|\bin\b)\s*\[(.*)\]$", "i"));
    static RANGE_RE: Lazy = Lazy::new(|| {
        js(
            r"^\s*(?:([A-Za-z]|theta)\s*(?:=|\bfrom\b|\bin\b)\s*)?(.+?)\s*(?:\.\.\.?|…|\bto\b)\s*(.+?)\s*$",
            "i",
        )
    });
    let mut s = jtrim(text).to_string();
    if let Ok(Some(b)) = BRACKET.captures(s.as_str()) {
        s = format!("{}={}", jsre::group(&b, 1), jsre::group(&b, 2));
    }
    let m = RANGE_RE.captures(s.as_str()).ok()??;
    let from = jtrim(jsre::group(&m, 2)).to_string();
    let to = jtrim(jsre::group(&m, 3)).to_string();
    // `n = 1..10 + 5` is ambiguous about where the range ends
    if from.is_empty() || to.is_empty() || has_ws(&to) || has_ws(&from) {
        return None;
    }
    Some(Range {
        index: m.get(1).map(|g| g.as_str().to_string()),
        from,
        to,
    })
}

fn with_range(op_sum: bool, body: &str, r: Range) -> Spec {
    Spec {
        op_sum,
        body: body.to_string(),
        index: r.index,
        from: r.from,
        to: r.to,
    }
}

fn spec_from_call(op_sum: bool, args: &[String], ctx: &SumContext) -> Option<Spec> {
    static AT: Lazy = Lazy::new(|| js(r"^\s*([A-Za-z]|theta)\s*=\s*(\S+)\s*$", ""));
    static INDEX: Lazy = Lazy::new(|| js(r"^(?:[A-Za-z]|theta)$", ""));
    let body = jtrim(args.first()?);
    if body.is_empty() {
        return None;
    }
    match args.len() {
        2 => parse_range(&args[1]).map(|r| with_range(op_sum, body, r)),
        3 => {
            if let Ok(Some(at)) = AT.captures(args[1].as_str()) {
                return Some(Spec {
                    op_sum,
                    body: body.into(),
                    index: Some(jsre::group(&at, 1).into()),
                    from: jsre::group(&at, 2).into(),
                    to: jtrim(&args[2]).into(),
                });
            }
            // `sum(k^2, 1, 10)` with k unknown; with no free letter it's a list
            pick_index(body, ctx).map(|_| Spec {
                op_sum,
                body: body.into(),
                index: None,
                from: jtrim(&args[1]).into(),
                to: jtrim(&args[2]).into(),
            })
        }
        4 => {
            let index = jtrim(&args[1]);
            if !jsre::test(&INDEX, index) {
                return None;
            }
            // `sum(x, y, 1, 10)` with y a variable not in the body is a list of four numbers
            let mentions = jsre::test(
                &js(&format!("(?<![A-Za-z0-9_]){index}(?![A-Za-z0-9_])"), ""),
                body,
            );
            if ctx.sci.variables.contains_key(index) && !mentions {
                return None;
            }
            Some(Spec {
                op_sum,
                body: body.into(),
                index: Some(index.into()),
                from: jtrim(&args[2]).into(),
                to: jtrim(&args[3]).into(),
            })
        }
        _ => None,
    }
}

struct Group {
    inner: String,
    end: usize,
}

fn grab_group(s: &str, at: usize, stop: fn(char) -> bool) -> Option<Group> {
    let first = s.get(at..)?.chars().next();
    if matches!(first, Some('{' | '(')) {
        let close = close_of(s, at)?;
        return Some(Group {
            inner: s[at + 1..close].to_string(),
            end: close + 1,
        });
    }
    let mut end = at;
    for ch in s[at..].chars() {
        if stop(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    (end > at).then(|| Group {
        inner: s[at..end].to_string(),
        end,
    })
}

/// `_{n=1}^{10} n^2`, also `_(n=1)^10 n^2`, the way sums are typeset. The usize is how much of `rest` it took.
fn spec_from_scripts(op_sum: bool, rest: &str) -> Option<(Spec, usize)> {
    static AT: Lazy = Lazy::new(|| js(r"^\s*([A-Za-z]|theta)\s*=\s*(.+?)\s*$", ""));
    let shift = if rest.starts_with("\\limits") { 7 } else { 0 };
    let s = &rest[shift..];
    if !s.starts_with('_') {
        return None;
    }
    let lower = grab_group(s, 1, |c| c == '^' || is_ws(c))?;
    if !s[lower.end..].starts_with('^') {
        return None;
    }
    let upper = grab_group(s, lower.end + 1, is_ws)?;
    // A `)` that the summand itself did not open belongs to the expression around the sum.
    let mut depth = 0i64;
    let mut end = upper.end;
    for (i, ch) in s[upper.end..].char_indices() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            if depth == 0 {
                break;
            }
            depth -= 1;
        }
        end = upper.end + i + ch.len_utf8();
    }
    let body = jtrim(&s[upper.end..end]);
    if body.is_empty() {
        return None;
    }
    let at = AT.captures(lower.inner.as_str()).ok().flatten();
    let spec = Spec {
        op_sum,
        body: body.into(),
        index: at.as_ref().map(|c| jsre::group(c, 1).to_string()),
        from: at.as_ref().map_or_else(
            || jtrim(&lower.inner).to_string(),
            |c| jsre::group(c, 2).to_string(),
        ),
        to: jtrim(&upper.inner).into(),
    };
    Some((spec, shift + end))
}

/// `sum n^2, n = 1..10`, `sum n^2 for n = 1 to 10`, `sum from n = 1 to 10 of n^2`.
fn spec_from_words(op_sum: bool, rest: &str) -> Option<Spec> {
    static OF: Lazy = Lazy::new(|| js(r"^of\s+", "i"));
    static COMMA: Lazy = Lazy::new(|| js(r"[,;]", "i"));
    static FOR: Lazy = Lazy::new(|| js(r"\s(?:for|from|over)\s", "i"));
    static LEAD: Lazy = Lazy::new(|| {
        js(
            r"^(?:(?:from|for|over)\s+)?(.+?)(?:\s+of\s+|\s*[,;:]\s*)(.+)$",
            "i",
        )
    });
    let stripped = jsre::replace_all(&OF, rest, |_| String::new()).into_owned();
    let s = jtrim(&stripped);
    if s.is_empty() {
        return None;
    }
    // the last comma, but the first `for`: `n^2 for n from 1 to 10`
    let cuts = [
        top_matches(s, &COMMA).last().copied(),
        top_matches(s, &FOR).first().copied(),
    ];
    for c in cuts.into_iter().flatten() {
        let body = jtrim(&s[..c.at]);
        if let Some(r) = parse_range(&s[c.at + c.len..])
            && !body.is_empty()
        {
            return Some(with_range(op_sum, body, r));
        }
    }
    if let Ok(Some(lead)) = LEAD.captures(s) {
        let body = jtrim(jsre::group(&lead, 2));
        if let Some(r) = parse_range(jsre::group(&lead, 1))
            && !body.is_empty()
        {
            return Some(with_range(op_sum, body, r));
        }
    }
    None
}

fn op_of(word: &str) -> bool {
    word.eq_ignore_ascii_case("sum")
}

fn whole_line_spec(text: &str, ctx: &SumContext) -> Option<Spec> {
    let m = LEAD_RE.captures(text).ok()??;
    let op_sum = op_of(jsre::group(&m, 1));
    let rest = &text[m.get(0)?.end()..];
    if rest.starts_with('(')
        && let Some(close) = close_of(rest, 0)
        && close == rest.len() - 1
        && let Some(spec) = spec_from_call(op_sum, &split_top(&rest[1..close]), ctx)
    {
        return Some(spec);
    }
    spec_from_scripts(op_sum, rest)
        .map(|(s, _)| s)
        .or_else(|| spec_from_words(op_sum, rest))
}

fn known(name: &str) -> bool {
    static KNOWN: std::sync::LazyLock<std::collections::HashSet<String>> =
        std::sync::LazyLock::new(|| {
            format!("{}|e", crate::scientific::NAMES)
                .to_lowercase()
                .split('|')
                .map(str::to_string)
                .collect()
        });
    KNOWN.contains(&name.to_lowercase())
}

/// The one free single letter in the body, when there is exactly one.
fn pick_index(body: &str, ctx: &SumContext) -> Option<String> {
    static WORD: Lazy = Lazy::new(|| js(r"[A-Za-z][A-Za-z0-9_]*", ""));
    let mut letters: Vec<String> = Vec::new();
    for m in WORD.find_iter(body).flatten() {
        let t = m.as_str();
        if t.len() != 1 || known(t) {
            continue;
        }
        if ctx.sci.variables.contains_key(t) || ctx.sci.functions.contains_key(t) {
            continue;
        }
        if !letters.iter().any(|l| l == t) {
            letters.push(t.to_string());
        }
    }
    (letters.len() == 1).then(|| letters.remove(0))
}

// ---- exact rationals (bigint, capped at 2^320) ----

#[derive(Clone, Debug, PartialEq)]
struct Q {
    n: BigInt,
    d: BigInt,
}

/// A rational, `Null` when it isn't known exactly, `Undef` when it is undefined (÷0, (−1)!).
#[derive(Clone, Debug)]
enum Ex {
    Q(Q),
    Null,
    Undef,
}

fn limit() -> &'static BigInt {
    static L: std::sync::LazyLock<BigInt> = std::sync::LazyLock::new(|| BigInt::one() << 320);
    &L
}

fn one() -> Q {
    Q {
        n: BigInt::one(),
        d: BigInt::one(),
    }
}

fn zero() -> Q {
    Q {
        n: BigInt::zero(),
        d: BigInt::one(),
    }
}

fn int_q(n: BigInt) -> Q {
    Q {
        n,
        d: BigInt::one(),
    }
}

fn bgcd(a: &BigInt, b: &BigInt) -> BigInt {
    let g = a.abs().gcd(&b.abs());
    if g.is_zero() { BigInt::one() } else { g }
}

fn mk_q(mut n: BigInt, mut d: BigInt) -> Option<Q> {
    if d.is_negative() {
        n = -n;
        d = -d;
    }
    let g = bgcd(&n, &d);
    n /= &g;
    d /= &g;
    (n.abs() <= *limit() && d <= *limit()).then_some(Q { n, d })
}

fn q_add(a: &Q, b: &Q) -> Option<Q> {
    mk_q(&a.n * &b.d + &b.n * &a.d, &a.d * &b.d)
}

fn q_sub(a: &Q, b: &Q) -> Option<Q> {
    mk_q(&a.n * &b.d - &b.n * &a.d, &a.d * &b.d)
}

fn q_mul(a: &Q, b: &Q) -> Option<Q> {
    mk_q(&a.n * &b.n, &a.d * &b.d)
}

fn q_div(a: &Q, b: &Q) -> Ex {
    if b.n.is_zero() {
        return Ex::Undef;
    }
    ex(mk_q(&a.n * &b.d, &a.d * &b.n))
}

fn ex(q: Option<Q>) -> Ex {
    q.map_or(Ex::Null, Ex::Q)
}

fn q_int(a: &Q) -> bool {
    a.d.is_one()
}

fn bits(n: &BigInt) -> u64 {
    // `toString(2).length`
    n.bits().max(1)
}

fn q_pow(a: &Q, e: &BigInt) -> Ex {
    if e.is_zero() {
        return Ex::Q(one());
    }
    if a.n.is_zero() {
        return if e.is_negative() {
            Ex::Undef
        } else {
            Ex::Q(zero())
        };
    }
    let b = bits(&a.n.abs()).max(bits(&a.d));
    if e.abs() * BigInt::from(b - 1) > BigInt::from(320) {
        return Ex::Null;
    }
    let k = e.abs();
    let r = match k.to_u32() {
        Some(k) => mk_q(a.n.pow(k), a.d.pow(k)),
        // only ±1 gets here (any bigger base fails the size check)
        None => {
            let odd = k.is_odd();
            mk_q(
                if a.n.is_negative() && odd {
                    -BigInt::one()
                } else {
                    BigInt::one()
                },
                BigInt::one(),
            )
        }
    };
    let Some(r) = r else { return Ex::Null };
    if e.is_negative() {
        q_div(&one(), &r)
    } else {
        Ex::Q(r)
    }
}

fn q_from_number(x: f64) -> Option<Q> {
    static NUM: Lazy = Lazy::new(|| js(r"^(-?)(\d+)(?:\.(\d+))?(?:e([+-]?\d+))?$", ""));
    if !x.is_finite() {
        return None;
    }
    let text = jsnum::to_string(x);
    let m = NUM.captures(text.as_str()).ok()??;
    let frac = jsre::group(&m, 3);
    let e: i64 = jsre::group(&m, 4)
        .trim_start_matches('+')
        .parse()
        .unwrap_or(0);
    let exp = e - frac.len() as i64;
    if exp.abs() > 400 {
        return None;
    }
    let mut n: BigInt = format!("{}{frac}", jsre::group(&m, 2)).parse().ok()?;
    if !jsre::group(&m, 1).is_empty() {
        n = -n;
    }
    let ten = BigInt::from(10);
    if exp >= 0 {
        mk_q(n * ten.pow(exp as u32), BigInt::one())
    } else {
        mk_q(n, ten.pow((-exp) as u32))
    }
}

fn big_to_f64(n: &BigInt) -> f64 {
    crate::precise::to_f64(&BigRational::from_integer(n.clone()))
}

fn q_to_number(q: &Q) -> f64 {
    big_to_f64(&q.n) / big_to_f64(&q.d)
}

fn q_text(q: &Q) -> String {
    if q.d.is_one() {
        q.n.to_string()
    } else {
        format!("{}/{}", q.n, q.d)
    }
}

fn q_neg(q: &Q) -> Q {
    Q {
        n: -q.n.clone(),
        d: q.d.clone(),
    }
}

fn is_one(q: Option<&Q>) -> bool {
    q.is_some_and(|q| q.n.is_one() && q.d.is_one())
}

// ---- the tree ----

fn unwrap(node: &Node) -> &Node {
    let mut n = node;
    while let Node::Parenthesis(inner) = n {
        n = inner;
    }
    n
}

/// `fnName`: an operator's or a called symbol's name.
fn fn_name(node: &Node) -> &str {
    match node {
        Node::Operator { op, .. } => op.name(),
        Node::Function { .. } => node.call_name().unwrap_or(""),
        _ => "",
    }
}

fn args_of(node: &Node) -> &[Node] {
    match node {
        Node::Operator { args, .. } | Node::Function { args, .. } => args,
        _ => &[],
    }
}

/// mathjs's `traverse`, children in `forEach` order (a call's name comes first); `count` adds the IndexNode an
/// accessor holds, which this tree doesn't keep as a node.
fn traverse(node: &Node, f: &mut impl FnMut(&Node), count: &mut usize) {
    f(node);
    *count += 1;
    match node {
        Node::Constant(_) | Node::Symbol(_) => {}
        Node::Operator { args, .. } => args.iter().for_each(|a| traverse(a, f, count)),
        Node::Function { callee, args } => {
            traverse(callee, f, count);
            args.iter().for_each(|a| traverse(a, f, count));
        }
        Node::Parenthesis(inner) => traverse(inner, f, count),
        Node::Array(items) => items.iter().for_each(|a| traverse(a, f, count)),
        Node::Range { start, end, step } => {
            traverse(start, f, count);
            traverse(end, f, count);
            if let Some(s) = step {
                traverse(s, f, count);
            }
        }
        Node::Conditional {
            condition,
            then,
            otherwise,
        } => {
            traverse(condition, f, count);
            traverse(then, f, count);
            traverse(otherwise, f, count);
        }
        Node::Relational { params, .. } => params.iter().for_each(|a| traverse(a, f, count)),
        Node::Assignment {
            target,
            index,
            value,
        } => {
            traverse(target, f, count);
            if let Some(ix) = index {
                *count += 1;
                ix.iter().for_each(|a| traverse(a, f, count));
            }
            traverse(value, f, count);
        }
        Node::FunctionAssignment { body, .. } => traverse(body, f, count),
        Node::Accessor { object, index, .. } => {
            traverse(object, f, count);
            *count += 1;
            index.iter().for_each(|a| traverse(a, f, count));
        }
        Node::Block(items) => items.iter().for_each(|(a, _)| traverse(a, f, count)),
    }
}

fn node_count(node: &Node) -> usize {
    let mut count = 0;
    traverse(node, &mut |_| {}, &mut count);
    count
}

fn has_index(node: &Node, index: &str) -> bool {
    let mut found = false;
    let mut count = 0;
    traverse(
        node,
        &mut |n| {
            if matches!(n, Node::Symbol(s) if s == index) {
                found = true;
            }
        },
        &mut count,
    );
    found
}

type Vars = std::collections::BTreeMap<String, f64>;

/// Null when the node isn't a rational of +, −, ×, ÷, integer powers and factorials.
fn exact_eval(node: &Node, index: &str, k: Option<&BigInt>, vars: &Vars) -> Ex {
    match node {
        Node::Parenthesis(inner) => exact_eval(inner, index, k, vars),
        Node::Constant(Constant::Number { value, .. }) => ex(q_from_number(*value)),
        Node::Constant(_) => Ex::Null,
        Node::Symbol(name) => {
            if name == index {
                return k.map_or(Ex::Null, |k| Ex::Q(int_q(k.clone())));
            }
            // a float variable like 1/3 isn't known exactly
            match vars.get(name) {
                Some(v) if jsnum::is_safe_integer(*v) => {
                    Ex::Q(int_q(BigInt::from_f64(*v).unwrap_or_default()))
                }
                _ => Ex::Null,
            }
        }
        Node::Operator { op, args, .. } => {
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                match exact_eval(a, index, k, vars) {
                    Ex::Q(q) => vals.push(q),
                    other => return other,
                }
            }
            let (a, b) = (vals.first(), vals.get(1));
            match (op, a, b) {
                (Op::UnaryMinus, Some(a), _) => ex(mk_q(-a.n.clone(), a.d.clone())),
                (Op::UnaryPlus, Some(a), _) => Ex::Q(a.clone()),
                (Op::Add, Some(a), Some(b)) => ex(q_add(a, b)),
                (Op::Subtract, Some(a), Some(b)) => ex(q_sub(a, b)),
                (Op::Multiply, Some(a), Some(b)) => ex(q_mul(a, b)),
                (Op::Divide, Some(a), Some(b)) => q_div(a, b),
                (Op::Pow, Some(a), Some(b)) => {
                    if q_int(b) {
                        q_pow(a, &b.n)
                    } else {
                        Ex::Null
                    }
                }
                _ => Ex::Null,
            }
        }
        Node::Function { args, .. } => {
            let name = fn_name(node);
            if !["factorial", "abs", "floor", "ceil"].contains(&name) || args.len() != 1 {
                return Ex::Null;
            }
            let a = match exact_eval(&args[0], index, k, vars) {
                Ex::Q(q) => q,
                other => return other,
            };
            match name {
                "abs" => Ex::Q(Q {
                    n: a.n.abs(),
                    d: a.d,
                }),
                "floor" | "ceil" => {
                    let mut q = &a.n / &a.d;
                    let exact = &q * &a.d == a.n;
                    if name == "floor" && a.n.is_negative() && !exact {
                        q -= 1;
                    }
                    if name == "ceil" && a.n.is_positive() && !exact {
                        q += 1;
                    }
                    Ex::Q(int_q(q))
                }
                _ => {
                    if !q_int(&a) {
                        return Ex::Null;
                    }
                    if a.n.is_negative() {
                        return Ex::Undef;
                    }
                    if a.n > BigInt::from(200) {
                        return Ex::Null;
                    }
                    let top = a.n.to_u32().unwrap_or(0);
                    let mut f = BigInt::one();
                    for i in 2..=top {
                        f *= i;
                    }
                    ex(mk_q(f, BigInt::one()))
                }
            }
        }
        _ => Ex::Null,
    }
}

/// A bound on the polynomial degree in the index, or None when the body isn't a polynomial.
fn degree(node: &Node, index: &str, vars: &Vars) -> Option<f64> {
    match node {
        Node::Parenthesis(inner) => degree(inner, index, vars),
        Node::Constant(_) => Some(0.0),
        Node::Symbol(name) => Some(if name == index { 1.0 } else { 0.0 }),
        Node::Operator { op, args, .. } => {
            let ds: Vec<Option<f64>> = args.iter().map(|a| degree(a, index, vars)).collect();
            if ds.iter().any(Option::is_none) {
                return None;
            }
            let a = ds.first().copied().flatten();
            let b = ds.get(1).copied().flatten();
            match op {
                Op::UnaryMinus | Op::UnaryPlus => a,
                Op::Add | Op::Subtract => Some(jmax(a?, b?)),
                Op::Multiply => Some(a? + b?),
                Op::Divide => (b? == 0.0).then_some(a?),
                Op::Pow => {
                    if b? != 0.0 {
                        return None;
                    }
                    if a? == 0.0 {
                        return Some(0.0);
                    }
                    match exact_eval(&args[1], index, None, vars) {
                        Ex::Q(e) if q_int(&e) && !e.n.is_negative() && e.n <= BigInt::from(64) => {
                            Some(a? * e.n.to_f64().unwrap_or(0.0))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        Node::Function { args, .. } => args.iter().all(|a| !has_index(a, index)).then_some(0.0),
        _ => None,
    }
}

fn binom(n: &BigInt, k: usize) -> BigInt {
    let mut out = BigInt::one();
    for i in 0..k {
        let i = BigInt::from(i);
        out = out * (n - &i) / (i + 1);
    }
    out
}

/// Σ p(a+k) for k < count, from the forward differences of p at a (exact for any count).
fn poly_sum(node: &Node, index: &str, vars: &Vars, a: &BigInt, count: &BigInt, deg: usize) -> Ex {
    let m = if *count < BigInt::from(deg + 1) {
        count.to_usize().unwrap_or(0)
    } else {
        deg + 1
    };
    let mut row: Vec<Q> = Vec::with_capacity(m);
    for j in 0..m {
        match exact_eval(node, index, Some(&(a + BigInt::from(j))), vars) {
            Ex::Q(q) => row.push(q),
            other => return other,
        }
    }
    let mut total = zero();
    for j in 0..m {
        let Some(term) = mk_q(&row[0].n * binom(count, j + 1), row[0].d.clone()) else {
            return Ex::Null;
        };
        let Some(next) = q_add(&total, &term) else {
            return Ex::Null;
        };
        total = next;
        let mut diffs = Vec::with_capacity(row.len());
        for i in 0..row.len().saturating_sub(1) {
            let Some(d) = q_sub(&row[i + 1], &row[i]) else {
                return Ex::Null;
            };
            diffs.push(d);
        }
        row = diffs;
    }
    Ex::Q(total)
}

fn exact_answer(q: &Q) -> SumAnswer {
    let n = q_to_number(q);
    if !n.is_finite() {
        return SumAnswer::default();
    }
    let text = q_text(q);
    SumAnswer {
        value: Some(Value::number(if n == 0.0 { 0.0 } else { n })),
        exact: (utf16_len(&text) <= MAX_EXACT_TEXT).then_some(text),
    }
}

fn undefined_answer() -> SumAnswer {
    SumAnswer {
        value: Some(Value::text("undefined")),
        exact: None,
    }
}

fn number_answer(n: f64) -> SumAnswer {
    SumAnswer {
        value: Some(Value::number(n)),
        exact: None,
    }
}

fn without_index(ctx: &SumContext, index: &str) -> ScientificContext {
    let mut sci = ctx.sci.clone();
    sci.variables.remove(index);
    sci
}

fn term_number(term: &mut Compiled, k: f64) -> Option<Value> {
    term.at(k)
}

fn finite(spec: &Spec, index: &str, a: f64, b: f64, ctx: &SumContext) -> SumAnswer {
    let sctx = without_index(ctx, index);
    let vars = &sctx.variables;
    let parsed = parse_scientific(&spec.body, &sctx, index);
    let term = compile_scientific(&spec.body, &sctx, index);
    let (Some(parsed), Some(mut term)) = (parsed, term) else {
        return SumAnswer::default();
    };
    let node = &parsed.node;
    let count = b - a + 1.0;
    let budget = WorkBudget::new(BUDGET);
    let exact_cost = EXACT_NODE_COST * node_count(node) as u64;
    let is_sum = spec.op_sum;
    let big = |x: f64| BigInt::from_f64(x).unwrap_or_default();

    if is_sum
        && let Some(deg) = degree(node, index, vars)
        && deg <= 64.0
    {
        match poly_sum(node, index, vars, &big(a), &big(count), deg as usize) {
            Ex::Undef => return undefined_answer(),
            Ex::Q(q) => return exact_answer(&q),
            Ex::Null => {}
        }
    }

    if count <= EXACT_TERMS {
        let mut acc: Option<Q> = Some(if is_sum { zero() } else { one() });
        let mut k = a;
        while k <= b {
            let Some(cur) = acc.as_ref() else { break };
            let v = exact_eval(node, index, Some(&big(k)), vars);
            acc = match v {
                Ex::Undef => return undefined_answer(),
                Ex::Null => None,
                Ex::Q(v) => {
                    if is_sum {
                        q_add(cur, &v)
                    } else {
                        q_mul(cur, &v)
                    }
                }
            };
            spend(exact_cost);
            if (k as i64) & 63 == 0 && budget.over() {
                acc = None;
            }
            k += 1.0;
        }
        if let Some(acc) = acc {
            return exact_answer(&acc);
        }
    }

    if count
        > if is_sum {
            FLOAT_TERMS as f64
        } else {
            PROD_FLOAT_TERMS
        }
    {
        return SumAnswer::default();
    }
    let mut sum = if is_sum { 0.0 } else { 1.0 };
    let mut comp = 0.0;
    let mut abs_sum = 0.0;
    let mut k = a;
    while k <= b {
        let Some(v) = term_number(&mut term, k) else {
            return SumAnswer::default();
        };
        if v.kind != ValueKind::Number || !v.n.is_finite() {
            return undefined_answer();
        }
        let x = v.n;
        if is_sum {
            // Neumaier summation
            let t = sum + x;
            comp += if f64::abs(sum) >= x.abs() {
                sum - t + x
            } else {
                x - t + sum
            };
            sum = t;
            abs_sum += x.abs();
        } else {
            sum *= x;
        }
        if (k as i64) & 255 == 0 && budget.over() {
            return SumAnswer::default();
        }
        k += 1.0;
    }
    if !is_sum {
        return if sum.is_finite() {
            number_answer(sum)
        } else {
            SumAnswer::default()
        };
    }
    sum += comp;
    // each term carries a few ulps of its own; what cancels below that is noise, and too much of it is no answer
    let err = 16.0 * EPS * abs_sum;
    if sum.abs() <= err {
        return number_answer(0.0);
    }
    if err > 1e-13 * sum.abs() {
        return SumAnswer::default();
    }
    number_answer(sum)
}

/// One term c·rⁿ·n^(−p)/(n!)^f, with c and r exact when they are rationals.
#[derive(Clone, Debug)]
struct Term {
    c: f64,
    cq: Option<Q>,
    r: f64,
    rq: Option<Q>,
    p: f64,
    f: f64,
}

struct Walk<'a> {
    index: &'a str,
    vars: &'a Vars,
    scope: &'a Scope,
}

struct Const {
    v: f64,
    q: Option<Q>,
}

fn const_of(node: &Node, w: &Walk<'_>) -> Option<Const> {
    let v = match eval::evaluate(node, w.scope) {
        Ok(Val::Num(v)) if v.is_finite() => v,
        _ => return None,
    };
    let q = match exact_eval(node, w.index, None, w.vars) {
        Ex::Q(q) => Some(q),
        _ => None,
    };
    Some(Const { v, q })
}

#[derive(Clone, Copy)]
struct Affine {
    al: f64,
    be: f64,
}

fn affine(node: &Node, w: &Walk<'_>) -> Option<Affine> {
    if !has_index(node, w.index) {
        return const_of(node, w).map(|c| Affine { al: 0.0, be: c.v });
    }
    let n = unwrap(node);
    if matches!(n, Node::Symbol(_)) {
        return Some(Affine { al: 1.0, be: 0.0 });
    }
    let Node::Operator { op, args, .. } = n else {
        return None;
    };
    let xs: Vec<Option<Affine>> = args.iter().map(|a| affine(a, w)).collect();
    let x = xs.first().copied().flatten();
    let y = xs.get(1).copied().flatten();
    match op {
        Op::UnaryMinus => x.map(|x| Affine {
            al: -x.al,
            be: -x.be,
        }),
        Op::UnaryPlus => x,
        Op::Add => Some(Affine {
            al: x?.al + y?.al,
            be: x?.be + y?.be,
        }),
        Op::Subtract => Some(Affine {
            al: x?.al - y?.al,
            be: x?.be - y?.be,
        }),
        Op::Multiply => {
            let (x, y) = (x?, y?);
            if x.al == 0.0 {
                Some(Affine {
                    al: x.be * y.al,
                    be: x.be * y.be,
                })
            } else if y.al == 0.0 {
                Some(Affine {
                    al: y.be * x.al,
                    be: y.be * x.be,
                })
            } else {
                None
            }
        }
        Op::Divide => {
            let (x, y) = (x?, y?);
            (y.al == 0.0 && y.be != 0.0).then(|| Affine {
                al: x.al / y.be,
                be: x.be / y.be,
            })
        }
        _ => None,
    }
}

fn q_pow_num(q: Option<&Q>, e: f64) -> Option<Q> {
    let q = q?;
    if !jsnum::is_integer(e) {
        return None;
    }
    match q_pow(q, &BigInt::from_f64(e)?) {
        Ex::Q(r) => Some(r),
        _ => None,
    }
}

fn pow_term(t: &Term, e: f64) -> Option<Term> {
    if !jsnum::is_integer(e) && (t.c < 0.0 || t.r < 0.0) {
        return None;
    }
    Some(Term {
        c: jsmath::pow(t.c, e),
        cq: q_pow_num(t.cq.as_ref(), e),
        r: jsmath::pow(t.r, e),
        rq: q_pow_num(t.rq.as_ref(), e),
        p: t.p * e,
        f: t.f * e,
    })
}

fn exponential(base: &Const, ex: Affine) -> Option<Term> {
    let ints = jsnum::is_integer(ex.al) && jsnum::is_integer(ex.be);
    if base.v == 0.0 || (base.v < 0.0 && !ints) {
        return None;
    }
    Some(Term {
        c: jsmath::pow(base.v, ex.be),
        cq: if ints {
            q_pow_num(base.q.as_ref(), ex.be)
        } else {
            None
        },
        r: jsmath::pow(base.v, ex.al),
        rq: if ints {
            q_pow_num(base.q.as_ref(), ex.al)
        } else {
            None
        },
        p: 0.0,
        f: 0.0,
    })
}

fn unit_term(p: f64, f: f64) -> Term {
    Term {
        c: 1.0,
        cq: Some(one()),
        r: 1.0,
        rq: Some(one()),
        p,
        f,
    }
}

fn classify(node: &Node, w: &Walk<'_>) -> Option<Term> {
    if !has_index(node, w.index) {
        let c = const_of(node, w)?;
        return Some(Term {
            c: c.v,
            cq: c.q,
            r: 1.0,
            rq: Some(one()),
            p: 0.0,
            f: 0.0,
        });
    }
    let n = unwrap(node);
    if matches!(n, Node::Symbol(_)) {
        return Some(unit_term(-1.0, 0.0));
    }
    let args = args_of(n);
    if let Node::Function { .. } = n {
        let name = fn_name(n);
        if args.len() != 1 {
            return None;
        }
        let arg = unwrap(&args[0]);
        if name == "factorial" && matches!(arg, Node::Symbol(_)) {
            return Some(unit_term(0.0, -1.0));
        }
        if name == "exp" {
            let ex = affine(arg, w)?;
            return exponential(
                &Const {
                    v: std::f64::consts::E,
                    q: None,
                },
                ex,
            );
        }
        if name == "sqrt" {
            let t = classify(arg, w)?;
            return pow_term(&t, 0.5);
        }
        return None;
    }
    let Node::Operator { op, .. } = n else {
        return None;
    };
    if *op == Op::Pow {
        let (base, exn) = (&args[0], &args[1]);
        if !has_index(base, w.index) {
            let b = const_of(base, w);
            let e = affine(exn, w);
            return exponential(&b?, e?);
        }
        if has_index(exn, w.index) {
            return None;
        }
        let e = const_of(exn, w);
        let t = classify(base, w);
        return pow_term(&t?, e?.v);
    }
    let ts: Vec<Option<Term>> = args.iter().map(|a| classify(a, w)).collect();
    let x = ts.first().cloned().flatten();
    let y = ts.get(1).cloned().flatten();
    match op {
        Op::UnaryMinus => x.map(|x| Term {
            c: -x.c,
            cq: x.cq.as_ref().map(q_neg),
            ..x
        }),
        Op::UnaryPlus => x,
        Op::Multiply => {
            let (x, y) = (x?, y?);
            Some(Term {
                c: x.c * y.c,
                cq: match (&x.cq, &y.cq) {
                    (Some(a), Some(b)) => q_mul(a, b),
                    _ => None,
                },
                r: x.r * y.r,
                rq: match (&x.rq, &y.rq) {
                    (Some(a), Some(b)) => q_mul(a, b),
                    _ => None,
                },
                p: x.p + y.p,
                f: x.f + y.f,
            })
        }
        Op::Divide => {
            let (x, y) = (x?, y?);
            if y.c == 0.0 || y.r == 0.0 {
                return None;
            }
            let div = |a: &Option<Q>, b: &Option<Q>| match (a, b) {
                (Some(a), Some(b)) => match q_div(a, b) {
                    Ex::Q(q) => Some(q),
                    _ => None,
                },
                _ => None,
            };
            Some(Term {
                c: x.c / y.c,
                cq: div(&x.cq, &y.cq),
                r: x.r / y.r,
                rq: div(&x.rq, &y.rq),
                p: x.p - y.p,
                f: x.f - y.f,
            })
        }
        _ => None,
    }
}

fn additive_parts<'a>(node: &'a Node, sign: f64, out: &mut Vec<(&'a Node, f64)>) {
    let n = unwrap(node);
    match n {
        Node::Operator {
            op: op @ (Op::Add | Op::Subtract),
            args,
            ..
        } if args.len() == 2 => {
            additive_parts(&args[0], sign, out);
            additive_parts(&args[1], if *op == Op::Add { sign } else { -sign }, out);
        }
        Node::Operator {
            op: Op::UnaryMinus,
            args,
            ..
        } if args.len() == 1 => additive_parts(&args[0], -sign, out),
        _ => out.push((n, sign)),
    }
}

const BERNOULLI: [f64; 12] = [
    1.0 / 6.0,
    -1.0 / 30.0,
    1.0 / 42.0,
    -1.0 / 30.0,
    5.0 / 66.0,
    -691.0 / 2730.0,
    7.0 / 6.0,
    -3617.0 / 510.0,
    43867.0 / 798.0,
    -174611.0 / 330.0,
    854513.0 / 138.0,
    -236364091.0 / 2730.0,
];

/// Hurwitz ζ(s, x) = Σ (x+k)^−s for k ≥ 0, by Euler–Maclaurin (its continuation below s = 1).
pub fn hurwitz(s: f64, x: f64) -> Option<f64> {
    if !(x > 0.0) || s == 1.0 || !(s > 0.0) || s > 400.0 {
        return None;
    }
    let pow = jsmath::pow;
    if s > 20.0 {
        // the tail past y is under y^(1−s)/(s−1)
        let mut sum = 0.0;
        let mut y = x;
        loop {
            sum += pow(y, -s);
            if pow(y, 1.0 - s) / (s - 1.0) < 1e-18 * sum {
                return Some(sum);
            }
            y += 1.0;
        }
    }
    let shift = (25.0 - x).ceil().max(0.0);
    let mut sum = 0.0;
    let mut k = 0.0;
    while k < shift {
        sum += pow(x + k, -s);
        k += 1.0;
    }
    let y = x + shift;
    sum += pow(y, 1.0 - s) / (s - 1.0) + pow(y, -s) / 2.0;
    let mut rising = s;
    let mut fact = 2.0;
    for j in 1..=BERNOULLI.len() {
        let jf = j as f64;
        let t = (BERNOULLI[j - 1] / fact) * rising * pow(y, -s - 2.0 * jf + 1.0);
        sum += t;
        if t.abs() < 1e-18 * sum.abs() {
            return Some(sum);
        }
        rising *= (s + 2.0 * jf - 1.0) * (s + 2.0 * jf);
        fact *= (2.0 * jf + 1.0) * (2.0 * jf + 2.0);
    }
    None
}

/// ζ(2k) = q·π^(2k)
fn zeta_even(p: f64) -> Option<Q> {
    let (n, d): (u64, u64) = match p {
        2.0 => (1, 6),
        4.0 => (1, 90),
        6.0 => (1, 945),
        8.0 => (1, 9450),
        10.0 => (1, 93555),
        12.0 => (691, 638_512_875),
        _ => return None,
    };
    Some(Q {
        n: BigInt::from(n),
        d: BigInt::from(d),
    })
}

fn pi_power(q: &Q, p: f64) -> String {
    let sign = if q.n.is_negative() { "-" } else { "" };
    let n = q.n.abs();
    format!(
        "{sign}{}pi^{}{}",
        if n.is_one() {
            String::new()
        } else {
            n.to_string()
        },
        jsnum::to_string(p),
        if q.d.is_one() {
            String::new()
        } else {
            format!("/{}", q.d)
        }
    )
}

#[derive(Default)]
struct Part {
    v: f64,
    q: Option<Q>,
    exact: Option<String>,
}

fn part(v: f64) -> Part {
    Part {
        v,
        ..Part::default()
    }
}

/// Σ over n ≥ a of a term whose convergence is settled by its shape; None when it isn't certain.
fn term_sum(t: &Term, a: f64) -> Option<Part> {
    if t.c == 0.0 {
        return Some(Part {
            v: 0.0,
            q: Some(zero()),
            exact: None,
        });
    }
    if a < 0.0 || (t.p > 0.0 && a < 1.0) {
        return None;
    }
    let ar = t.r.abs();
    if t.f == 1.0 {
        if t.p != 0.0 {
            return None;
        }
        if a == 0.0 {
            let k = t.rq.as_ref().filter(|q| q_int(q)).map(|q| big_to_f64(&q.n));
            let exact = (is_one(t.cq.as_ref()) && k.is_some_and(|k| (1.0..=20.0).contains(&k)))
                .then(|| {
                    let k = k.unwrap_or(1.0);
                    if k == 1.0 {
                        "e".to_string()
                    } else {
                        format!("e^{}", jsnum::to_string(k))
                    }
                });
            return Some(Part {
                v: t.c * jsmath::exp(t.r),
                q: None,
                exact,
            });
        }
        if a == 1.0 && is_one(t.cq.as_ref()) && is_one(t.rq.as_ref()) {
            return Some(Part {
                v: std::f64::consts::E - 1.0,
                q: None,
                exact: Some("e - 1".into()),
            });
        }
        return series(t, a);
    }
    if t.f != 0.0 {
        return None;
    }
    if ar > 1.0 {
        return None;
    }
    if ar < 1.0 {
        if t.p == 0.0 {
            let v = (t.c * jsmath::pow(t.r, a)) / (1.0 - t.r);
            let rp = match &t.rq {
                Some(rq) if a <= 4096.0 => q_pow(rq, &BigInt::from_f64(a).unwrap_or_default()),
                _ => Ex::Null,
            };
            let den = t.rq.as_ref().and_then(|rq| q_sub(&one(), rq));
            let top = match (&t.cq, &rp) {
                (Some(cq), Ex::Q(rp)) => q_mul(cq, rp),
                _ => None,
            };
            let q = match (top, den) {
                (Some(top), Some(den)) => q_div(&top, &den),
                _ => Ex::Null,
            };
            return Some(match q {
                Ex::Q(q) => Part {
                    v: q_to_number(&q),
                    q: Some(q),
                    exact: None,
                },
                _ => part(v),
            });
        }
        if t.p == 1.0 && a == 1.0 {
            let v = -t.c * jsmath::log1p(-t.r);
            let inv = t.rq.as_ref().and_then(|rq| q_sub(&one(), rq));
            let x = inv.map(|inv| q_div(&one(), &inv));
            let sign = if is_one(t.cq.as_ref()) {
                Some("")
            } else if t
                .cq
                .as_ref()
                .is_some_and(|q| q.n == -BigInt::one() && q.d.is_one())
            {
                Some("-")
            } else {
                None
            };
            let exact = match (x, sign) {
                (Some(Ex::Q(x)), Some(sign)) => Some(format!("{sign}ln({})", q_text(&x))),
                _ => None,
            };
            return Some(Part { v, q: None, exact });
        }
        return series(t, a);
    }
    let even_p = if jsnum::is_integer(t.p) && t.p % 2.0 == 0.0 {
        zeta_even(t.p)
    } else {
        None
    };
    if t.r == 1.0 {
        if !(t.p > 1.0) {
            return None;
        }
        let z = hurwitz(t.p, a)?;
        let mut exact = None;
        if let (Some(ev), Some(cq), true) = (&even_p, &t.cq, a == 1.0) {
            // the TS's `qMul(...)!` would throw past 2^320; such a coefficient leaves no closed form here
            exact = q_mul(cq, ev).map(|q| pi_power(&q, t.p));
        } else if let Some(ev) = even_p
            .as_ref()
            .filter(|_| is_one(t.cq.as_ref()) && a <= 30.0)
        {
            let mut head = Some(zero());
            let mut n = 1.0;
            while n < a {
                let Some(h) = head.as_ref() else { break };
                let pw = BigInt::from_f64(n).unwrap_or_default().pow(t.p as u32);
                head = q_add(
                    h,
                    &Q {
                        n: BigInt::one(),
                        d: pw,
                    },
                );
                n += 1.0;
            }
            if let Some(h) = head.map(|h| q_text(&h))
                && utf16_len(&h) <= MAX_EXACT_TEXT
            {
                exact = Some(format!("{} - {h}", pi_power(ev, t.p)));
            }
        }
        return Some(Part {
            v: t.c * z,
            q: None,
            exact,
        });
    }
    // r = −1: alternating, which converges for any p > 0
    if !(t.p > 0.0) || a > 100.0 {
        return None;
    }
    let sign = if a % 2.0 == 0.0 { 1.0 } else { -1.0 };
    let v = if t.p == 1.0 {
        let mut head = 0.0;
        let mut n = 1.0;
        while n < a {
            head += (if n % 2.0 == 0.0 { 1.0 } else { -1.0 }) / n;
            n += 1.0;
        }
        -std::f64::consts::LN_2 - head
    } else {
        let z1 = hurwitz(t.p, a / 2.0)?;
        let z2 = hurwitz(t.p, (a + 1.0) / 2.0)?;
        sign * jsmath::pow(2.0, -t.p) * (z1 - z2)
    };
    let mut exact = None;
    if a == 1.0
        && let Some(cq) = &t.cq
    {
        if t.p == 1.0 && cq.n.abs().is_one() && cq.d.is_one() {
            exact = Some(
                if cq.n.is_positive() {
                    "-ln(2)"
                } else {
                    "ln(2)"
                }
                .to_string(),
            );
        } else if let Some(ev) = &even_p {
            // Σ (−1)ⁿ n^−p from 1 is −(1 − 2^(1−p))·ζ(p)
            let half = Q {
                n: BigInt::one(),
                d: BigInt::from(2).pow(t.p as u32 - 1),
            };
            if let Some(eta) = q_sub(&one(), &half).and_then(|x| q_mul(&x, ev))
                && let Some(q) = q_mul(cq, &q_neg(&eta))
            {
                exact = Some(pi_power(&q, t.p));
            }
        }
    }
    Some(Part {
        v: t.c * v,
        q: None,
        exact,
    })
}

/// Direct summation with a proven bound on what's left; |r| < 1, or a factorial below. FLOAT_TERMS caps the work.
fn series(t: &Term, a: f64) -> Option<Part> {
    let lnr = jsmath::ln(t.r.abs());
    let neg = t.r < 0.0;
    let mut ln_fact = 0.0;
    let mut k = 2.0;
    while k <= a {
        ln_fact += jsmath::ln(k);
        k += 1.0;
    }
    let mut sum = 0.0;
    let mut abs_sum = 0.0;
    // exp(x) carries about |x| ulps of error
    let mut spread = 8.0;
    let fact_on = t.f != 0.0 && !t.f.is_nan();
    let mut n = a;
    for _ in 0..FLOAT_TERMS {
        if n > a {
            ln_fact += if fact_on { jsmath::ln(n) } else { 0.0 };
        }
        let x = if n == 0.0 {
            0.0
        } else {
            n * lnr - t.p * jsmath::ln(n) - t.f * ln_fact
        };
        spread = jmax(spread, 8.0 + x.abs());
        let mag = if n == 0.0 {
            if t.p < 0.0 { 0.0 } else { 1.0 }
        } else {
            jsmath::exp(x)
        };
        let term = t.c * mag * (if neg && n % 2.0 == 1.0 { -1.0 } else { 1.0 });
        sum += term;
        abs_sum += term.abs();
        // every later ratio |t(m+1)/t(m)| is at most rho, so the rest is under |t|·rho/(1−rho)
        let rho = if n == 0.0 {
            f64::INFINITY
        } else {
            (t.r.abs() * jmax(1.0, jsmath::pow((n + 1.0) / n, -t.p)))
                / (if fact_on { n + 1.0 } else { 1.0 })
        };
        if rho < 1.0 && (term.abs() * rho) / (1.0 - rho) <= 1e-17 * sum.abs() {
            if spread * EPS * abs_sum > 1e-13 * sum.abs() {
                return None;
            }
            return Some(part(sum));
        }
        n += 1.0;
    }
    None
}

fn infinite(spec: &Spec, index: &str, a: f64, ctx: &SumContext) -> SumAnswer {
    if !spec.op_sum {
        return SumAnswer::default();
    }
    let sctx = without_index(ctx, index);
    let parsed = parse_scientific(&spec.body, &sctx, index);
    let term = compile_scientific(&spec.body, &sctx, index);
    let (Some(parsed), Some(mut term)) = (parsed, term) else {
        return SumAnswer::default();
    };
    let w = Walk {
        index,
        vars: &sctx.variables,
        scope: &parsed.scope,
    };
    let mut pieces = Vec::new();
    additive_parts(&parsed.node, 1.0, &mut pieces);
    let mut terms = Vec::with_capacity(pieces.len());
    for (node, sign) in pieces {
        let Some(t) = classify(node, &w) else {
            return SumAnswer::default();
        };
        if !t.c.is_finite() || !t.r.is_finite() || !t.p.is_finite() {
            return SumAnswer::default();
        }
        terms.push(if sign > 0.0 {
            t
        } else {
            Term {
                c: -t.c,
                cq: t.cq.as_ref().map(q_neg),
                ..t
            }
        });
    }
    if !agrees(&terms, &mut term, a) {
        return SumAnswer::default();
    }
    let mut parts = Vec::with_capacity(terms.len());
    for t in &terms {
        match term_sum(t, a) {
            Some(p) if p.v.is_finite() => parts.push(p),
            _ => return SumAnswer::default(),
        }
    }
    let v = parts.iter().fold(0.0, |s, p| s + p.v);
    let mut exact = None;
    if parts.iter().all(|p| p.q.is_some()) {
        let mut q = Some(zero());
        for p in &parts {
            q = q.and_then(|q| p.q.as_ref().and_then(|pq| q_add(&q, pq)));
        }
        if let Some(q) = q {
            return exact_answer(&q);
        }
    } else if parts.len() == 1 {
        exact = parts[0].exact.clone();
    }
    SumAnswer {
        value: Some(Value::number(if v.abs() < 1e-300 { 0.0 } else { v })),
        exact,
    }
}

fn factorial_of(n: f64) -> f64 {
    let mut f = 1.0_f64;
    let mut k = 2.0;
    // past 170! it's ∞ and stays ∞: stop there (counting on to n took 4.5 s from 1e9, days from 1e15, in the TS
    // too; and past 2^53 the count stops moving)
    while k <= n && f.is_finite() {
        f *= k;
        k += 1.0;
    }
    f
}

/// The shape read off the tree must give the same numbers as evaluating the body.
fn agrees(terms: &[Term], term: &mut Compiled, a: f64) -> bool {
    let mut checked = 0;
    for n in [a, a + 1.0, a + 2.0, a + 5.0, a + 11.0] {
        let Some(actual) = term.at(n) else {
            return false;
        };
        if actual.kind != ValueKind::Number || !actual.n.is_finite() {
            return false;
        }
        let mut model = 0.0;
        for t in terms {
            let np = if n == 0.0 && t.p == 0.0 {
                1.0
            } else {
                jsmath::pow(n, -t.p)
            };
            model += (t.c * jsmath::pow(t.r, n) * np) / jsmath::pow(factorial_of(n), t.f);
        }
        if !model.is_finite() || model.abs() < 1e-250 {
            continue;
        }
        if (model - actual.n).abs() > 1e-9 * jmax(model.abs(), actual.n.abs()) {
            return false;
        }
        checked += 1;
    }
    checked >= 2
}

fn plain(ctx: &SumContext, default_units: DefaultUnits) -> PlainContext {
    PlainContext {
        sci: ctx.sci.clone(),
        default_units,
        keep_units: false,
    }
}

fn bound(text: &str, ctx: &SumContext) -> Option<f64> {
    let v = try_plain_math(text, &plain(ctx, DefaultUnits::default()))?;
    (v.kind == ValueKind::Number
        && v.unit.is_none()
        && jsnum::is_safe_integer(v.n)
        && v.n.abs() <= MAX_BOUND)
        .then_some(v.n)
}

fn evaluate_spec(spec: &Spec, ctx: &SumContext) -> SumAnswer {
    static LATEX: Lazy = Lazy::new(|| js(r"\\[a-zA-Z]|[\^_]\{", ""));
    static INDEX: Lazy = Lazy::new(|| js(r"^(?:[A-Za-z]|theta)$", ""));
    static INFINITY_RE: Lazy = Lazy::new(|| js(r"^\+?(?:∞|inf|infty|infinity|\\infty|oo)$", "i"));
    let body = if jsre::test(&LATEX, &spec.body) {
        crate::latex::latex_to_ascii(&spec.body)
    } else {
        spec.body.clone()
    };
    let s = Spec {
        body: body.clone(),
        ..spec.clone()
    };
    // a typed θ reaches here as theta, the one multi-letter index
    let Some(index) = spec.index.clone().or_else(|| pick_index(&body, ctx)) else {
        return SumAnswer::default();
    };
    if !jsre::test(&INDEX, &index) || index == "e" || ctx.sci.functions.contains_key(&index) {
        return SumAnswer::default();
    }
    let Some(a) = bound(&spec.from, ctx) else {
        return SumAnswer::default();
    };
    if jsre::test(&INFINITY_RE, &spec.to) {
        return infinite(&s, &index, a, ctx);
    }
    match bound(&spec.to, ctx) {
        Some(b) if b >= a => finite(&s, &index, a, b, ctx),
        _ => SumAnswer::default(),
    }
}

/// A `Π(2, 3, 4)` list multiplies its entries; a list inside (`[1, 2]`) is left alone.
fn product_of_list(args: &[String]) -> Option<String> {
    if args
        .iter()
        .any(|a| jtrim(a).is_empty() || a.contains(['[', ']']))
    {
        return None;
    }
    Some(format!(
        "({})",
        args.iter()
            .map(|a| format!("({})", jtrim(a)))
            .collect::<Vec<_>>()
            .join("*")
    ))
}

/// First closer the sum itself did not open, so `(sum n, n=1..4)+1` keeps the `+1`.
fn depth_end(s: &str, from: usize) -> usize {
    let mut depth = 0i64;
    for (i, ch) in s[from..].char_indices() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            if depth == 0 {
                return from + i;
            }
            depth -= 1;
        }
    }
    s.len()
}

fn is_number(r: &SumAnswer) -> Option<f64> {
    r.value
        .as_ref()
        .filter(|v| v.kind == ValueKind::Number)
        .map(|v| v.n)
}

enum Embedded {
    Text(String),
    Fail(SumAnswer),
}

/// A `sum_k=1^4 k` inside a larger line, such as `(sum_k=1^4 k)+2`.
fn embed_script_sums(expr: &str, ctx: &SumContext) -> Option<Embedded> {
    let mut out = String::new();
    let mut last = 0;
    let mut replaced = false;
    let mut pos = 0;
    while pos <= expr.len() {
        let Ok(Some(m)) = SCRIPT_RE.captures_from_pos(expr, pos) else {
            break;
        };
        let whole = m.get(0)?;
        let rest_at = whole.end();
        pos = if whole.end() > whole.start() {
            whole.end()
        } else {
            whole.end() + 1
        };
        if expr[rest_at..].starts_with('(') {
            continue;
        }
        let op_sum = op_of(jsre::group(&m, 1));
        let mut spec = spec_from_scripts(op_sum, &expr[rest_at..]);
        if spec.is_none() {
            let end = depth_end(expr, rest_at);
            if let Some(word) = spec_from_words(op_sum, jtrim(&expr[rest_at..end])) {
                spec = Some((word, end - rest_at));
            }
        }
        let Some((spec, consumed)) = spec else {
            continue;
        };
        let r = evaluate_spec(&spec, ctx);
        let Some(n) = is_number(&r) else {
            return Some(Embedded::Fail(r));
        };
        out.push_str(&expr[last..whole.start()]);
        out.push_str(&format!("({})", jsnum::to_string(n)));
        last = rest_at + consumed;
        pos = last;
        replaced = true;
    }
    replaced.then(|| Embedded::Text(out + &expr[last..]))
}

/// Σ and Π lines (already `sum` and `prod` via [`normalize_sums`]), or None when the line holds none and the
/// usual evaluation should run.
pub fn sum_answer(expr: &str, ctx: &SumContext) -> Option<SumAnswer> {
    if !jsre::test(&HINT_RE, expr) {
        return None;
    }
    if let Some(whole) = whole_line_spec(jtrim(expr), ctx) {
        return Some(evaluate_spec(&whole, ctx));
    }

    let embedded = embed_script_sums(expr, ctx);
    let mut expr = expr.to_string();
    match embedded {
        Some(Embedded::Fail(r)) => return Some(r),
        Some(Embedded::Text(ref t)) => expr = t.clone(),
        None => {}
    }

    let mut out = String::new();
    let mut last = 0;
    let mut replaced = false;
    let mut all_exact = true;
    let mut pos = 0;
    while pos <= expr.len() {
        let Ok(Some(m)) = CALL_RE.captures_from_pos(expr.as_str(), pos) else {
            break;
        };
        let whole = m.get(0)?;
        pos = whole.end();
        let open = whole.end() - 1;
        let Some(close) = close_of(&expr, open) else {
            break;
        };
        let op_sum = op_of(jsre::group(&m, 1));
        let args = split_top(&expr[open + 1..close]);
        let put = if let Some(spec) = spec_from_call(op_sum, &args, ctx) {
            let r = evaluate_spec(&spec, ctx);
            let Some(n) = is_number(&r) else {
                return Some(r);
            };
            if r.exact.is_none() && !jsnum::is_integer(n) {
                all_exact = false;
            }
            Some(format!("({})", jsnum::to_string(n)))
        } else if !op_sum {
            product_of_list(&args)
        } else {
            None
        };
        let Some(put) = put else { continue };
        out.push_str(&expr[last..whole.start()]);
        out.push_str(&put);
        last = close + 1;
        pos = close + 1;
        replaced = true;
    }
    let full = plain(ctx, ctx.default_units.clone());
    if !replaced {
        embedded.as_ref()?;
        return Some(SumAnswer {
            value: try_plain_math(&expr, &full),
            exact: None,
        });
    }
    let text = out + &expr[last..];
    let value = try_plain_math(&text, &full);
    let form = value
        .as_ref()
        .filter(|v| {
            all_exact && v.kind == ValueKind::Number && v.n.is_finite() && wants_exact_form(&text)
        })
        .and_then(|v| {
            exact_form(
                v.n,
                ExactFormOptions {
                    rationalize: ctx.rationalize,
                    wide: false,
                },
            )
        });
    Some(SumAnswer { value, exact: form })
}

/// Golden replay of `engine/sums` calls.
pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    match name {
        "hurwitz" => {
            let n = |i: usize| args.get(i).and_then(Json::as_f64).unwrap_or(f64::NAN);
            Some(hurwitz(n(0), n(1)).map_or(Json::Null, |v| json!(v)))
        }
        "normalizeSums" => Some(json!(normalize_sums(
            args.first().and_then(Json::as_str).unwrap_or("")
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(s: &str) -> Option<(String, Option<String>)> {
        let r = sum_answer(&normalize_sums(s), &SumContext::default())?;
        Some((
            r.value
                .map(|v| crate::format::format_value(&v, 12.0))
                .unwrap_or_default(),
            r.exact,
        ))
    }

    #[test]
    fn spellings_and_forms() {
        assert_eq!(shown("Σ n^2, n=1..10").unwrap().0, "385");
        assert_eq!(shown("sum(n^2, n, 1, 10)").unwrap().0, "385");
        assert_eq!(shown("Σ_(n=1)^10 n^2").unwrap().0, "385");
        assert_eq!(
            shown("Σ 1/n^2, n=1..∞").unwrap().1.as_deref(),
            Some("pi^2/6")
        );
        assert!(shown("Σ 5").is_none());
        assert!(shown("Σ(1, 2, 3)").is_none());
        assert_eq!(shown("Π(2, 3, 4)").unwrap().0, "24");
    }

    #[test]
    fn hurwitz_matches_the_ts() {
        assert_eq!(hurwitz(2.0, 1.0), Some(1.6449340668482262));
        assert_eq!(hurwitz(1.0, 1.0), None);
    }
}
