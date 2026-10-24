//! What the bar's commit and chaining need from `src/lib/chain.ts` and `src/lib/answer.ts`, ported line by line
//! and replayed against the `lib.chain` and `lib.answer` fixtures (`tests/golden_ffi.rs`).
//!
//! Interim home: WP-APP-LOGIC owns those modules. When its `app/chain.rs` and `app/answer.rs` land in
//! `qcalc-core`, these become re-exports.

use qcalc_core::format::format_number;
use qcalc_core::jsre::{self, Lazy, js};
use qcalc_core::units::is_improper_unit_conversion;
use serde_json::Value as Json;

use crate::jsstr;
use crate::records::ChainAnswer;

static PLAIN_UNSIGNED: Lazy = Lazy::new(|| js(r"^(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$", ""));

// ---- chain.ts ----

/// `chainsFromAnswer`: an operator typed first continues from the last answer; a leading minus never does.
pub fn chains_from_answer(q: &str, last: Option<&ChainAnswer>) -> bool {
    static CHAIN_OP: Lazy = Lazy::new(|| js(r"^\s*[*×·/÷^+!]", ""));
    static CHAIN_CONVERT: Lazy = Lazy::new(|| js(r"^\s*(?:in|to)\s+\S", "i"));
    let Some(last) = last.filter(|l| !l.plain.is_empty()) else {
        return false;
    };
    if jsre::test(&CHAIN_OP, q) {
        return true;
    }
    last.unit && jsre::test(&CHAIN_CONVERT, q)
}

static IN_TO: Lazy = Lazy::new(|| js(r"^\s*(?:in|to)\b", "i"));

/// `chainedExpr`: evaluated through the engine's own `ans`.
pub fn chained_expr(q: &str) -> String {
    if jsre::test(&IN_TO, q) {
        format!("ans {}", jsstr::trim_start(q))
    } else {
        format!("ans{q}")
    }
}

/// `chainedHistoryExpr`: what the tape keeps, the answer written out.
pub fn chained_history_expr(q: &str, plain: &str) -> String {
    static LOOSE: Lazy = Lazy::new(|| js(r"^[+\-−]", ""));
    let rest = jsstr::trim_start(q);
    if jsre::test(&IN_TO, rest) {
        return format!("{plain} {rest}");
    }
    if jsre::test(&LOOSE, rest) || jsre::test(&PLAIN_UNSIGNED, plain) {
        format!("{plain}{rest}")
    } else {
        format!("({plain}){rest}")
    }
}

/// `ansWrittenOut`: a typed `ans` written out in the tape; a function body keeps it.
pub fn ans_written_out(expr: &str, plain: &str) -> String {
    static ANS: Lazy = Lazy::new(|| js(r"\bans\b", "i"));
    static ALONE: Lazy = Lazy::new(|| js(r"^\s*ans\s*$", "i"));
    if !jsre::test(&ANS, expr) || qcalc_core::evaluate::parse_function_def(jsstr::trim(expr)).is_some() {
        return expr.to_string();
    }
    let with = answer_among(plain, jsre::test(&ALONE, expr));
    jsre::replace_all(&ANS, expr, |c| js_replacement(&with, jsre::group(c, 0))).into_owned()
}

/// A JS replacement string with no groups: `$$` is `$`, `$&` the match; anything else stays as written.
fn js_replacement(with: &str, matched: &str) -> String {
    if !with.contains('$') {
        return with.to_string();
    }
    let mut out = String::new();
    let mut chars = with.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('$') => {
                    chars.next();
                    out.push('$');
                    continue;
                }
                Some('&') => {
                    chars.next();
                    out.push_str(matched);
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

// ---- answer.ts ----

fn is_wrapped(s: &str) -> bool {
    if !s.starts_with('(') || !s.ends_with(')') {
        return false;
    }
    let last = s.chars().count() - 1;
    let mut depth = 0i32;
    for (i, ch) in s.chars().enumerate() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 && i < last {
                return false;
            }
        }
    }
    true
}

/// `answerAmong`: an answer placed among other text keeps its value (`ans^2` with ans -3 is `(-3)^2`).
pub fn answer_among(answer: &str, alone: bool) -> String {
    static ATOM: Lazy = Lazy::new(|| js(r"^(?:(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?|[A-Za-zπτ]+)$", "i"));
    static CALL_NAME: Lazy = Lazy::new(|| js(r"^[A-Za-z]\w*(?=\()", ""));
    let a = jsstr::trim(answer);
    if alone || a.is_empty() || jsre::test(&ATOM, a) || is_wrapped(&CALL_NAME.replace(a, "")) {
        return answer.to_string();
    }
    format!("({a})")
}

/// `insertableAnswer`: the displayed magnitude at sig figs, never the engine's full-precision `n`.
pub fn insertable_answer(display: &str, n: Option<f64>, sig_figs: f64) -> String {
    static GROUPING: Lazy = Lazy::new(|| js(r",(?=\d{3}(?:\D|$))", ""));
    static PLAIN_NUMBER: Lazy = Lazy::new(|| js(r"^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$", ""));
    let stripped = jsre::replace_all(&GROUPING, display, |_| String::new());
    let plain = jsstr::trim(&stripped);
    if is_improper_unit_conversion(plain) {
        return String::new();
    }
    // parenthesized so `ans*2` doesn't bind to the uncertainty alone
    if plain.contains('±') {
        return format!("({plain})");
    }
    if !plain.is_empty() && !jsre::test(&PLAIN_NUMBER, plain) {
        return plain.to_string();
    }
    match n.filter(|n| n.is_finite()) {
        Some(n) => format_number(n, sig_figs),
        None => plain.to_string(),
    }
}

fn roots_of(row: &Json) -> Option<Vec<f64>> {
    let s = row.get("solve").filter(|s| s.is_object())?;
    if s.get("outcome").and_then(Json::as_str) != Some("roots") {
        return None;
    }
    let roots: Vec<f64> = s.get("roots")?.as_array()?.iter().filter_map(Json::as_f64).collect();
    (roots.len() >= 2).then_some(roots)
}

/// `solveRoots`: a solved row's roots as each one inserts, when it has more than one.
pub fn solve_roots(row: &Json, sig_figs: f64) -> Option<Vec<String>> {
    roots_of(row).map(|r| r.into_iter().map(|x| format_number(x, sig_figs)).collect())
}

/// `solveInsert`: a solved row inserts its first root; a message row inserts nothing.
pub fn solve_insert(row: &Json, sig_figs: f64) -> Option<String> {
    let s = row.get("solve").filter(|s| s.is_object())?;
    if s.get("outcome").and_then(Json::as_str) != Some("roots") {
        return Some(String::new());
    }
    solve_roots(row, sig_figs).and_then(|r| r.into_iter().next())
}

/// `insertableHistoryAnswer`.
pub fn insertable_history_answer(row: &Json, exact_form: bool, sig_figs: f64) -> String {
    if let Some(solved) = solve_insert(row, sig_figs) {
        return solved;
    }
    let exact = row.get("exact").and_then(Json::as_str).filter(|e| !e.is_empty());
    if exact_form && let Some(exact) = exact {
        return insertable_answer(exact, None, 12.0);
    }
    let measured = row.get("meas").is_some_and(|m| !m.is_null());
    let n = if measured { None } else { row.get("n").and_then(Json::as_f64) };
    insertable_answer(row.get("display").and_then(Json::as_str).unwrap_or(""), n, sig_figs)
}

/// `isGraphCommand` (`src/engine/graph.ts`): true even for an incomplete command.
pub fn is_graph_command(input: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*graph(?:\s|$)", "i"));
    jsre::test(&RE, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_and_writes_out() {
        let last = ChainAnswer { plain: "-3".into(), unit: false };
        assert!(chains_from_answer("*2", Some(&last)));
        assert!(!chains_from_answer("-2", Some(&last)));
        assert_eq!(chained_expr(" in ft"), "ans in ft");
        assert_eq!(chained_history_expr("^2", "-3"), "(-3)^2");
        assert_eq!(ans_written_out("ans^2", "-3"), "(-3)^2");
        assert_eq!(ans_written_out("f(x) = ans*x", "-3"), "f(x) = ans*x");
        assert_eq!(insertable_answer("1,234.5", Some(1234.5), 6.0), "1234.5");
    }
}
