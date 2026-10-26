//! The tape (`src/lib/history.ts`): what a history row holds, how it is slimmed for storage and read back, and the
//! context it gives the next line (variables, functions, measures, unit quantities, `ans`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::js::{self, len16};
use crate::app::needs::sanitize_meas;
use crate::evaluate::{parse_assignment, parse_function_def};
use crate::jsre::{self, Lazy};
use crate::types::{EvaluateOptions, Meas, SolveInfo, UserFunction, ValueKind};

/// A row that isn't a plain calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    /// A dictionary definition (legacy; inserts its expression).
    Definition,
    /// `f(x) = …`.
    Function,
    /// A `sys` row, with its equations.
    System,
}

impl RowKind {
    fn parse(v: &Json) -> Option<RowKind> {
        match v.as_str()? {
            "definition" => Some(RowKind::Definition),
            "function" => Some(RowKind::Function),
            "system" => Some(RowKind::System),
            _ => None,
        }
    }
}

/// One saved calculation, in the TS `HistoryRow` JSON shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub id: String,
    pub expr: String,
    pub display: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meas: Option<Meas>,
    /// Parseable unit text, so `ans` and variables keep their unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<RowKind>,
    /// Equation fields for a `sys` row, so history can reopen them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equations: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_params: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_body: Option<String>,
    /// A solved equation's roots; its expr is never re-read as an assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solve: Option<SolveInfo>,
    /// Commit time in ms since the epoch; rows from before this was kept count as old.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

fn str_field(o: &Json, key: &str) -> Option<String> {
    o.get(key)?.as_str().map(str::to_string)
}

fn strings(v: Option<&Json>) -> Option<Vec<String>> {
    Some(
        v?.as_array()?
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

/// A stored `meas` read leniently: each field kept when it has the right type.
pub fn meas_from_json(v: &Json) -> Option<Meas> {
    let o = v.as_object()?;
    let num = |k: &str| o.get(k).and_then(Json::as_f64).filter(|x| x.is_finite());
    Some(Meas {
        sig: num("sig").map(|x| x as u32),
        dp: num("dp").map(|x| x as i32),
        unc: num("unc"),
        unc_digits: o.get("uncDigits").and_then(Json::as_str).map(str::to_string),
    })
}

impl HistoryRow {
    /// A row from JSON, keeping each field only when it has its TS type (as the TS `typeof` checks do).
    pub fn from_json(v: &Json) -> HistoryRow {
        HistoryRow {
            id: str_field(v, "id").unwrap_or_default(),
            expr: str_field(v, "expr").unwrap_or_default(),
            display: str_field(v, "display").unwrap_or_default(),
            exact: str_field(v, "exact"),
            n: v.get("n").and_then(Json::as_f64),
            meas: v.get("meas").and_then(meas_from_json),
            quantity: str_field(v, "quantity"),
            kind: v.get("kind").and_then(RowKind::parse),
            equations: strings(v.get("equations")),
            fn_name: str_field(v, "fnName"),
            fn_params: strings(v.get("fnParams")),
            fn_body: str_field(v, "fnBody"),
            solve: v
                .get("solve")
                .and_then(|s| serde_json::from_value(s.clone()).ok()),
            at: v.get("at").and_then(Json::as_f64),
        }
    }

    /// The row as the TS JSON (`undefined` fields left out).
    pub fn to_json(&self) -> Json {
        serde_json::to_value(self).unwrap_or(Json::Null)
    }

    fn finite_n(&self) -> Option<f64> {
        self.n.filter(|n| n.is_finite())
    }
}

pub const MAX_HISTORY: usize = 10;
pub const MAX_HISTORY_EXPR: usize = 240;
/// A stored matrix is read back whole, so its literal gets more room than an expression.
const MAX_MATRIX_LITERAL: usize = 4000;
pub const MAX_HISTORY_DISPLAY: usize = 96;
pub const MAX_HISTORY_EXACT: usize = 72;
pub const MAX_HISTORY_JSON: usize = 16_384;

/// A matrix answer keeps its literal (`[[1, 2], [3, 4]]`) as its quantity.
pub fn is_matrix_quantity(quantity: Option<&str>) -> bool {
    quantity.is_some_and(|q| q.starts_with("[["))
}

/// A complex answer keeps its parts (`5 + 5i`, at full precision) as its quantity.
pub fn is_complex_quantity(quantity: Option<&str>) -> bool {
    static RE: Lazy = Lazy::new(|| jsre::js(r"^-?[\d.e+-]+ [+-] [\d.e+-]+i$", "i"));
    quantity.is_some_and(|q| jsre::test(&RE, q))
}

/// An answer that isn't one number but still chains, written out as its quantity: a matrix or a complex number.
pub fn is_literal_quantity(quantity: Option<&str>) -> bool {
    is_matrix_quantity(quantity) || is_complex_quantity(quantity)
}

/// A row id: the commit time in base 36 and six random base-36 digits (`newRowId`). The caller passes both, so the
/// core stays free of the clock and of randomness.
pub fn new_row_id(now_ms: u64, random: u64) -> String {
    fn base36(mut n: u64) -> String {
        if n == 0 {
            return "0".into();
        }
        let mut out = Vec::new();
        while n > 0 {
            out.push(char::from_digit((n % 36) as u32, 36).unwrap_or('0'));
            n /= 36;
        }
        out.iter().rev().collect()
    }
    let tail = base36(random % 2_176_782_336); // 36^6
    format!("{}-{:0>6}", base36(now_ms), tail)
}

/// `text` cut to `max` UTF-16 units, ending in `…`.
pub fn ellipsize(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if len16(text) <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".into();
    }
    format!("{}…", js::trim_end(&js::take16(text, max - 1)))
}

fn slim_expr(expr: &str) -> String {
    if len16(expr) <= MAX_HISTORY_EXPR {
        return expr.to_string();
    }
    let trimmed = js::trim(expr);
    if let Some((name, params, body)) = parse_function_def(trimmed) {
        return with_prefix(&format!("{name}({}) = ", params.join(", ")), &body);
    }
    if let Some((variable, rhs)) = parse_assignment(trimmed) {
        return with_prefix(&format!("{variable} = "), &rhs);
    }
    ellipsize(expr, MAX_HISTORY_EXPR)
}

// a definition keeps its name whole and cuts the right-hand side instead
fn with_prefix(prefix: &str, rest: &str) -> String {
    let room = MAX_HISTORY_EXPR.saturating_sub(len16(prefix)).max(8);
    format!("{prefix}{}", ellipsize(rest, room))
}

fn sanitize_solve(raw: Option<&SolveInfo>) -> Option<SolveInfo> {
    static VARIABLE: Lazy = Lazy::new(|| jsre::js(r"^(?:[A-Za-z]|θ)$", ""));
    let s = raw?;
    if !jsre::test(&VARIABLE, &s.variable) {
        return None;
    }
    if s.roots.len() > 8 || !s.roots.iter().all(|r| r.is_finite()) {
        return None;
    }
    Some(SolveInfo {
        variable: s.variable.clone(),
        roots: s.roots.clone(),
        outcome: s.outcome,
        more: (s.more == Some(true)).then_some(true),
    })
}

fn slim_equations(raw: Option<&Vec<String>>) -> Option<Vec<String>> {
    let lines: Vec<String> = raw?
        .iter()
        .take(5)
        .map(|line| ellipsize(js::trim(line), 160))
        .collect();
    (!lines.is_empty()).then_some(lines)
}

type FnFields = (Option<String>, Option<Vec<String>>, Option<String>);

fn slim_function_fields(row: &HistoryRow, kind: Option<RowKind>, expr: &str) -> FnFields {
    if kind != Some(RowKind::Function) {
        return (None, None, None);
    }
    let has_name = row.fn_name.as_deref().is_some_and(|n| !n.is_empty());
    let has_body = row.fn_body.is_some();
    // only re-read the expr for fields the row is missing
    let parsed = if has_name && has_body && row.fn_params.is_some() {
        None
    } else {
        parse_function_def(js::trim(expr))
    };
    let name = if has_name {
        row.fn_name.clone()
    } else {
        parsed.as_ref().map(|p| p.0.clone())
    };
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return (None, None, None);
    };
    let params = row
        .fn_params
        .clone()
        .or_else(|| parsed.as_ref().map(|p| p.1.clone()))
        .unwrap_or_default();
    let body = if has_body {
        row.fn_body.clone()
    } else {
        parsed.map(|p| p.2)
    };
    let Some(mut body) = body else {
        return (Some(name), Some(params), None);
    };
    if len16(&body) > MAX_HISTORY_EXPR {
        body = ellipsize(&body, MAX_HISTORY_EXPR);
    }
    (Some(name), Some(params), Some(body))
}

/// The row as it is stored: long text cut, fields a kind doesn't use dropped, metadata sanitised. None when there
/// is nothing to keep.
pub fn slim_history_row(row: &HistoryRow) -> Option<HistoryRow> {
    let expr = slim_expr(&row.expr);
    let display = ellipsize(&row.display, MAX_HISTORY_DISPLAY);
    let exact = row
        .exact
        .as_deref()
        .filter(|e| !e.is_empty())
        .map(|e| ellipsize(e, MAX_HISTORY_EXACT))
        .filter(|e| *e != display);
    if js::trim(&expr).is_empty() && js::trim(&display).is_empty() {
        return None;
    }
    let kind = row.kind;
    let (fn_name, fn_params, fn_body) = slim_function_fields(row, kind, &expr);
    let solve = if kind.is_some() {
        None
    } else {
        sanitize_solve(row.solve.as_ref())
    };
    let equations = if kind == Some(RowKind::System) {
        slim_equations(row.equations.as_ref())
    } else {
        None
    };
    let id = ellipsize(&row.id, 48);
    // never truncated: a cut-off quantity would parse as a different one
    let quantity = match (&kind, &row.quantity) {
        (None, Some(q)) => {
            let room = if is_matrix_quantity(Some(q)) {
                MAX_MATRIX_LITERAL
            } else {
                MAX_HISTORY_EXPR
            };
            Some(if len16(q) <= room {
                q.clone()
            } else {
                String::new()
            })
        }
        _ => None,
    };
    Some(HistoryRow {
        id: if id.is_empty() { "row".into() } else { id },
        expr: if kind == Some(RowKind::System) {
            "sys".into()
        } else {
            expr
        },
        display,
        exact,
        n: if kind == Some(RowKind::Function) {
            None
        } else {
            row.finite_n()
        },
        meas: if kind.is_some() {
            None
        } else {
            sanitize_meas(row.meas.as_ref())
        },
        quantity,
        kind,
        equations,
        fn_name,
        fn_params,
        fn_body,
        solve,
        at: row.at.filter(|a| a.is_finite() && *a > 0.0),
    })
}

/// A row read back from storage, whatever an older version (or a hand edit) left in it: wrong types dropped, the
/// legacy `latex` field read as the expression, a missing id replaced by `fallback_id`.
pub fn normalize_history_row(raw: &Json, fallback_id: &str) -> Option<HistoryRow> {
    let mut row = HistoryRow::from_json(raw);
    if row.id.is_empty() {
        row.id = fallback_id.to_string();
    }
    if raw.get("expr").and_then(Json::as_str).is_none() {
        row.expr = str_field(raw, "latex").unwrap_or_default();
    }
    slim_history_row(&row)
}

// keyed so variable and function names stay in separate namespaces
fn defined_name(row: &HistoryRow) -> Option<String> {
    if row.kind == Some(RowKind::Definition) || row.solve.is_some() {
        return None;
    }
    let expr = js::trim(&row.expr);
    let fn_name = if row.kind == Some(RowKind::Function) {
        row.fn_name
            .clone()
            .or_else(|| parse_function_def(expr).map(|p| p.0))
    } else {
        parse_function_def(expr).map(|p| p.0)
    };
    if let Some(f) = fn_name.filter(|f| !f.is_empty()) {
        return Some(format!("fn:{f}"));
    }
    if row.kind == Some(RowKind::Function) {
        return None;
    }
    parse_assignment(expr).map(|(v, _)| format!("var:{v}"))
}

// a definition outlives the last ten rows, so `x = 5` still works long after
fn with_sticky_definitions(rows: &[HistoryRow]) -> Vec<HistoryRow> {
    let cut = rows.len().saturating_sub(MAX_HISTORY);
    let recent = &rows[cut..];
    let mut seen: BTreeSet<String> = recent.iter().filter_map(defined_name).collect();
    let mut kept: Vec<HistoryRow> = Vec::new();
    for row in rows[..cut].iter().rev() {
        let Some(name) = defined_name(row) else {
            continue;
        };
        if !seen.insert(name) {
            continue;
        }
        kept.insert(0, row.clone());
    }
    kept.extend(recent.iter().cloned());
    kept
}

fn rows_json_len(rows: &[HistoryRow]) -> usize {
    let json = Json::Array(rows.iter().map(HistoryRow::to_json).collect());
    len16(&js::stringify(&json))
}

/// What is written to storage: the last ten rows plus older definitions still in use, slimmed, and the oldest
/// dropped until the JSON fits `MAX_HISTORY_JSON`.
pub fn persistable_history(rows: &[HistoryRow]) -> Vec<HistoryRow> {
    let mut out: Vec<HistoryRow> = with_sticky_definitions(rows)
        .iter()
        .filter_map(slim_history_row)
        .collect();
    while !out.is_empty() && rows_json_len(&out) > MAX_HISTORY_JSON {
        out.remove(0);
    }
    out
}

fn skips_context(row: &HistoryRow) -> bool {
    row.kind.is_some()
}

/// The variables the rows define, in order (a later one wins; a unit-valued one is left to `history_quantities`).
pub fn history_variables(rows: &[HistoryRow]) -> BTreeMap<String, f64> {
    let mut vars: BTreeMap<String, f64> = BTreeMap::new();
    for row in rows {
        if skips_context(row) || row.solve.is_some() {
            continue;
        }
        let Some((variable, _)) = parse_assignment(js::trim(&row.expr)) else {
            continue;
        };
        if row.quantity.is_some() {
            vars.remove(&variable);
            continue;
        }
        if let Some(n) = row.finite_n() {
            vars.insert(variable, n);
            continue;
        }
        // legacy rows may lack `n`
        let shown = js::trim(&row.display);
        let from_display = js::number(shown);
        if !shown.is_empty() && from_display.is_finite() {
            vars.insert(variable, from_display);
            continue;
        }
        let result = crate::evaluate_line(
            &row.expr,
            &EvaluateOptions {
                variables: vars.clone(),
                ..EvaluateOptions::default()
            },
        );
        if let Some(v) = result.value
            && v.kind == ValueKind::Number
            && v.n.is_finite()
        {
            vars.insert(variable, v.n);
        }
    }
    vars
}

/// The sig figs and ± each variable (and `ans`) carries.
pub fn history_measures(rows: &[HistoryRow]) -> BTreeMap<String, Meas> {
    let mut out = BTreeMap::new();
    let mut last: Option<&HistoryRow> = None;
    for row in rows {
        if skips_context(row) {
            continue;
        }
        if row.finite_n().is_some() || is_literal_quantity(row.quantity.as_deref()) {
            last = Some(row);
        }
        if row.solve.is_some() {
            continue;
        }
        let Some((variable, _)) = parse_assignment(js::trim(&row.expr)) else {
            continue;
        };
        match (&row.meas, &row.quantity) {
            (Some(m), None) => {
                out.insert(variable, m.clone());
            }
            _ => {
                out.remove(&variable);
            }
        }
    }
    if let Some(last) = last
        && let (Some(m), None) = (&last.meas, &last.quantity)
    {
        out.insert("ans".into(), m.clone());
    }
    out
}

/// The unit-valued (and matrix and complex) variables, and `ans` when the last answer is one, as quantity text.
pub fn history_quantities(rows: &[HistoryRow]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut last: Option<&HistoryRow> = None;
    for row in rows {
        if skips_context(row) {
            continue;
        }
        if row.finite_n().is_some() || is_literal_quantity(row.quantity.as_deref()) {
            last = Some(row);
        }
        if row.solve.is_some() {
            continue;
        }
        let Some((variable, _)) = parse_assignment(js::trim(&row.expr)) else {
            continue;
        };
        match &row.quantity {
            Some(q) => {
                out.insert(variable, q.clone());
            }
            None => {
                out.remove(&variable);
            }
        }
    }
    if let Some(q) = last.and_then(|l| l.quantity.clone()) {
        out.insert("ans".into(), q);
    }
    out
}

/// The functions the rows define, a later definition winning.
pub fn history_functions(rows: &[HistoryRow]) -> BTreeMap<String, UserFunction> {
    let mut fns = BTreeMap::new();
    for row in rows {
        if matches!(row.kind, Some(RowKind::Definition | RowKind::System)) {
            continue;
        }
        if row.kind == Some(RowKind::Function)
            && let (Some(name), Some(body)) = (&row.fn_name, &row.fn_body)
            && !name.is_empty()
        {
            fns.insert(
                name.clone(),
                UserFunction {
                    params: row.fn_params.clone().unwrap_or_default(),
                    body: body.clone(),
                },
            );
            continue;
        }
        if let Some((name, params, body)) = parse_function_def(js::trim(&row.expr)) {
            fns.insert(name, UserFunction { params, body });
        }
    }
    fns
}

/// `ans` as a plain number: the last answer row's `n`, unless that answer is a unit, matrix, complex or a list of
/// roots (which reach `ans` some other way, or not at all).
pub fn last_history_number(rows: &[HistoryRow]) -> Option<f64> {
    for row in rows.iter().rev() {
        if skips_context(row) {
            continue;
        }
        // a matrix answer is `ans` now, through historyQuantities
        if is_literal_quantity(row.quantity.as_deref()) {
            return None;
        }
        // several roots are no one number; the one wanted is clicked in the tape
        if let Some(s) = &row.solve
            && s.outcome == crate::types::SolveOutcome::Roots
            && s.roots.len() > 1
        {
            return None;
        }
        // a unit answer's `ans` comes from historyQuantities instead
        if let Some(n) = row.finite_n() {
            return row.quantity.is_none().then_some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slims_and_reads_back() {
        let row = normalize_history_row(&json!({"latex": "2+3", "display": "5", "at": 0}), "x");
        assert_eq!(
            row.map(|r| r.to_json()),
            Some(json!({"id": "x", "expr": "2+3", "display": "5"}))
        );
        assert_eq!(ellipsize("abc", 2), "a…");
    }

    #[test]
    fn ids_look_like_the_ts_ones() {
        assert_eq!(new_row_id(36, 35), "10-00000z");
    }
}
