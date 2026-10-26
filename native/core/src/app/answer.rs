//! What an answer inserts, copies and shows (`src/lib/answer.ts`): the insertable text of a live answer or a
//! history row, the ← → picks on a highlighted row, unitless copies and the root list's display.

use serde::{Deserialize, Serialize};

use crate::app::js as jstr;
use crate::format::{DEFAULT_SIG_FIGS, format_number, pretty_answer};
use crate::jsre::{self, Lazy, js};
use crate::units::is_improper_unit_conversion;
use serde_json::Value as Json;

const DEFAULT: f64 = DEFAULT_SIG_FIGS as f64;

fn strip_grouping_commas(s: &str) -> String {
    static RE: Lazy = Lazy::new(|| js(r",(?=\d{3}(?:\D|$))", ""));
    jsre::replace_all(&RE, s, |_| String::new()).into_owned()
}

/// What a live answer inserts: the displayed magnitude at `sig_figs`, never the engine's full-precision `n`; a ±
/// answer in parentheses, so `ans*2` doesn't bind to the uncertainty alone; a unit or text answer as shown.
pub fn insertable_answer(display: &str, n: Option<f64>, sig_figs: f64) -> String {
    static PLAIN_NUMBER: Lazy =
        Lazy::new(|| js(r"^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$", ""));
    let stripped = strip_grouping_commas(display);
    let plain = jstr::trim(&stripped);
    if is_improper_unit_conversion(plain) {
        return String::new();
    }
    if plain.contains('±') {
        return format!("({plain})");
    }
    if !plain.is_empty() && !jsre::test(&PLAIN_NUMBER, plain) {
        return plain.to_string();
    }
    if let Some(n) = n.filter(|n| n.is_finite()) {
        return format_number(n, sig_figs);
    }
    plain.to_string()
}

fn is_wrapped(s: &str) -> bool {
    if !s.starts_with('(') || !s.ends_with(')') {
        return false;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut depth = 0i64;
    for (i, &c) in units.iter().enumerate() {
        if c == u16::from(b'(') {
            depth += 1;
        } else if c == u16::from(b')') {
            depth -= 1;
            if depth == 0 && i < units.len() - 1 {
                return false;
            }
        }
    }
    true
}

/// An answer placed among other text keeps its value: `ans^2` with ans -3 is (-3)^2, not -3^2.
pub fn answer_among(answer: &str, alone: bool) -> String {
    static ATOM: Lazy =
        Lazy::new(|| js(r"^(?:(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?|[A-Za-zπτ]+)$", "i"));
    static CALL: Lazy = Lazy::new(|| js(r"^[A-Za-z]\w*(?=\()", ""));
    let a = jstr::trim(answer);
    if alone
        || a.is_empty()
        || jsre::test(&ATOM, a)
        || is_wrapped(&jsre::replace_all(&CALL, a, |_| String::new()))
    {
        return answer.to_string();
    }
    format!("({a})")
}

/// Display only: `±1000000` and `-2, 3` get the same grouping and minus as a single answer.
pub fn pretty_roots(s: &str) -> String {
    s.split(", ")
        .map(|part| match part.strip_prefix('±') {
            Some(rest) => format!("±{}", pretty_answer(rest)),
            None => pretty_answer(part),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// a number as answers show it: sign, grouping commas, decimals, ×10ⁿ or e notation, a ± part, π and roots in an
// exact form, a fraction. no latin letter but the e of e notation, so a unit can't hide inside it
const SHOWN_NUMBER: &str = r"[+\-−±]?(?:[\d.,√∛π/^⁰¹²³⁴⁵⁶⁷⁸⁹⁻·×]|(?<=\d)e[+\-−]?(?=\d)|\s(?=[±×·]))*\d[\d.⁰¹²³⁴⁵⁶⁷⁸⁹⁻]*(?:\s*±\s*[\d.,]+)?";
// a unit as answers show it: letters, °, µ, Ω and Å, with powers, products and quotients (`kg·m/s²`, `ft·lbf`)
const SHOWN_UNIT: &str = r"[A-Za-z°µμΩÅ][A-Za-z°µμΩÅ²³⁻¹^\d·*/]*";

/// An answer copied without its unit: `15 m²` is `15`, `x = 6 m, y = 4 m` is `x = 6, y = 4`. A number that runs
/// into more units (`5 ft 3 in`), money (`$10.00`), a date or a message is copied as shown.
pub fn unitless_answer(text: &str) -> String {
    static NUMBER_WITH_UNIT: Lazy = Lazy::new(|| {
        js(
            &format!(
                r"(^|=\s*|\(\s*|,\s*|\sor\s+)({SHOWN_NUMBER})\s+{SHOWN_UNIT}(?=\s*$|\s*,|\s*\)|\s+or\b)"
            ),
            "",
        )
    });
    jsre::replace_all(&NUMBER_WITH_UNIT, text, |c| {
        format!("{}{}", jsre::group(c, 1), jstr::trim(jsre::group(c, 2)))
    })
    .into_owned()
}

/// Which side of a dual answer the bar reads as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerForm {
    #[default]
    Exact,
    Approx,
}

/// What ↵ on a highlighted history row inserts by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryInsert {
    #[default]
    Expr,
    Answer,
}

/// The settings choices, with their labels.
pub const HISTORY_INSERT_OPTIONS: [(HistoryInsert, &str); 2] = [
    (HistoryInsert::Expr, "Expression"),
    (HistoryInsert::Answer, "Answer"),
];

/// `normalizeHistoryInsert`: anything but `"answer"` is the expression.
pub fn normalize_history_insert(value: &serde_json::Value) -> HistoryInsert {
    if value.as_str() == Some("answer") {
        HistoryInsert::Answer
    } else {
        HistoryInsert::Expr
    }
}

fn str_of<'a>(row: &'a Json, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Json::as_str)
}

fn solve_of(row: &Json) -> Option<&Json> {
    row.get("solve").filter(|s| s.is_object())
}

fn outcome_of(row: &Json) -> Option<&str> {
    solve_of(row)?.get("outcome").and_then(Json::as_str)
}

fn roots_of(row: &Json) -> Option<Vec<f64>> {
    if outcome_of(row) != Some("roots") {
        return None;
    }
    let roots: Vec<f64> = solve_of(row)?
        .get("roots")?
        .as_array()?
        .iter()
        .filter_map(Json::as_f64)
        .collect();
    (roots.len() >= 2).then_some(roots)
}

/// A solved row's roots as each one inserts, when it has more than one; None for any other row. Rows are the TS
/// `HistoryRow` JSON, as in [`crate::app::history`].
pub fn solve_roots(row: &Json, sig_figs: f64) -> Option<Vec<String>> {
    roots_of(row).map(|r| r.into_iter().map(|x| format_number(x, sig_figs)).collect())
}

/// A solved row with several roots inserts its first one (each root in the tape is its own button), never the
/// whole list; a message row inserts nothing (`Some("")`); None for any other row.
pub fn solve_insert(row: &Json, sig_figs: f64) -> Option<String> {
    solve_of(row)?;
    if outcome_of(row) != Some("roots") {
        return Some(String::new());
    }
    solve_roots(row, sig_figs).and_then(|r| r.into_iter().next())
}

/// `hasDualAnswer` of a row.
pub fn row_has_dual_answer(row: &Json) -> bool {
    has_dual_answer(str_of(row, "display").unwrap_or(""), str_of(row, "exact"))
}

/// Whether a row shows two answers, `exact ≈ approx`.
pub fn has_dual_answer(display: &str, exact: Option<&str>) -> bool {
    let display = jstr::trim(display);
    let Some(exact) = exact.map(jstr::trim) else {
        return false;
    };
    if display.is_empty() || exact.is_empty() || exact == display {
        return false;
    }
    !is_improper_unit_conversion(display) && !is_improper_unit_conversion(exact)
}

/// The side of a dual answer the setting reads.
pub fn visible_answer<'a>(display: &'a str, exact: Option<&'a str>, form: AnswerForm) -> &'a str {
    match exact {
        Some(e) if form == AnswerForm::Exact && !e.is_empty() => e,
        _ => display,
    }
}

/// What a history row's answer inserts.
pub fn insertable_history_answer(row: &Json, form: AnswerForm, sig_figs: f64) -> String {
    if let Some(solved) = solve_insert(row, sig_figs) {
        return solved;
    }
    if form == AnswerForm::Exact
        && let Some(e) = str_of(row, "exact").filter(|e| !e.is_empty())
    {
        return insertable_answer(e, None, DEFAULT);
    }
    // a measured answer inserts as shown (`5.00`), not re-rounded from `n`
    let measured = row.get("meas").is_some_and(|m| !m.is_null());
    let n = if measured {
        None
    } else {
        row.get("n").and_then(Json::as_f64)
    };
    insertable_answer(str_of(row, "display").unwrap_or(""), n, sig_figs)
}

/// The side of the history rows ← and → last chose: ↑ and ↓ keep to it from row to row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistorySide {
    Expr,
    #[default]
    Answer,
}

/// Where ↑ or ↓ land on the next row: on the expression side, its expression; on the answer side, the answer the
/// row reads as.
pub fn history_pick_on(side: HistorySide, start: usize) -> usize {
    match side {
        HistorySide::Expr => 0,
        HistorySide::Answer => start,
    }
}

/// The side a pick is on.
pub fn history_side_of(pick: usize) -> HistorySide {
    if pick == 0 {
        HistorySide::Expr
    } else {
        HistorySide::Answer
    }
}

/// What ← and → step through on a highlighted history row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryPicks {
    /// Each answer the row shows, as each inserts (pick 1…; pick 0 is the expression).
    pub answers: Vec<String>,
    /// The pick ↑ lands on: the answer the row reads as, or the expression when there is none.
    pub start: usize,
}

/// The expression (pick 0), then each answer the row shows, as each inserts: a solve's roots one by one, the exact
/// value and then the approximation, or the one answer.
pub fn history_picks(row: &Json, form: AnswerForm, sig_figs: f64) -> HistoryPicks {
    let none = HistoryPicks {
        answers: Vec::new(),
        start: 0,
    };
    match str_of(row, "kind") {
        Some("function" | "system") => return none,
        Some("definition") => {
            return HistoryPicks {
                answers: vec![insertable_history_reuse(
                    row,
                    form,
                    HistoryInsert::Answer,
                    sig_figs,
                )],
                start: 1,
            };
        }
        _ => {}
    }
    if solve_of(row).is_some() && outcome_of(row) != Some("roots") {
        return none;
    }
    if let Some(roots) = solve_roots(row, sig_figs) {
        return HistoryPicks {
            answers: roots,
            start: 1,
        };
    }
    if row_has_dual_answer(row) {
        return HistoryPicks {
            answers: vec![
                insertable_answer(str_of(row, "exact").unwrap_or(""), None, DEFAULT),
                insertable_history_answer(row, AnswerForm::Approx, sig_figs),
            ],
            start: if form == AnswerForm::Exact { 1 } else { 2 },
        };
    }
    HistoryPicks {
        answers: vec![insertable_history_answer(row, form, sig_figs)],
        start: 1,
    }
}

/// What ↵ on a highlighted history row inserts; clicks always insert the side they hit.
pub fn insertable_history_reuse(
    row: &Json,
    form: AnswerForm,
    insert: HistoryInsert,
    sig_figs: f64,
) -> String {
    if str_of(row, "kind") == Some("definition") || insert == HistoryInsert::Expr {
        return str_of(row, "expr").unwrap_or("").to_string();
    }
    insertable_history_answer(row, form, sig_figs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_and_copies() {
        assert_eq!(insertable_answer("1,234.5", Some(1234.5), 12.0), "1234.5");
        assert_eq!(insertable_answer("5 ± 1", None, 12.0), "(5 ± 1)");
        assert_eq!(answer_among("-3", false), "(-3)");
        assert_eq!(answer_among("sqrt(2)", false), "sqrt(2)");
        assert_eq!(unitless_answer("x = 6 m, y = 4 m"), "x = 6, y = 4");
        assert_eq!(unitless_answer("$10.00"), "$10.00");
        assert_eq!(pretty_roots("±1000000"), "±1,000,000");
    }
}
