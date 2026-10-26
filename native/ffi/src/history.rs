//! The tape: `src/lib/history.ts` and `src/lib/historyShow.ts` ported line by line, plus the history file.
//!
//! Rows travel as JSON objects in the TS `HistoryRow` shape, because the TS functions take loosely typed rows
//! (a legacy `latex` field, a non-string `quantity`, a `solve` of the wrong shape) and their checks are part of
//! the behaviour. The typed `HistoryRow` record converts to and from that shape at the FFI boundary.
//! Replayed against the `lib.history` and `lib.historyShow` fixtures by `tests/golden_ffi.rs`.
//!
//! Interim home: WP-APP-LOGIC owns `src/lib/history.ts` in the plan. When its `app/history.rs` lands in
//! `qcalc-core`, these bodies become calls to it.

use std::collections::BTreeSet;

use qcalc_core::evaluate::{parse_assignment, parse_function_def};
use qcalc_core::types::{EvaluateOptions, ValueKind};
use serde_json::{Map, Value as Json, json};

use crate::jsstr::{self, len16};
use crate::records::{HistoryKind, HistoryRow, HistoryShow, MeasInfo, SolveInfo, SolveOutcome};

pub const MAX_HISTORY: usize = 10;
pub const MAX_HISTORY_EXPR: usize = 240;
const MAX_MATRIX_LITERAL: usize = 4000;
pub const MAX_HISTORY_DISPLAY: usize = 96;
pub const MAX_HISTORY_EXACT: usize = 72;
pub const MAX_HISTORY_JSON: usize = 16_384;

pub const RECENT_MS: f64 = 5.0 * 60.0 * 1000.0;
pub const RECENT_ROWS: usize = 5;

// ---- small JS helpers over loose JSON ----

fn str_of<'a>(row: &'a Json, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Json::as_str)
}

/// `typeof x === 'number' && Number.isFinite(x)` (JSON numbers are always finite; NaN arrives as null).
fn finite_of(row: &Json, key: &str) -> Option<f64> {
    row.get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite())
}

/// `x != null`.
fn present(row: &Json, key: &str) -> bool {
    row.get(key).is_some_and(|v| !v.is_null())
}

/// JS truthiness.
fn truthy(v: Option<&Json>) -> bool {
    match v {
        None | Some(Json::Null) => false,
        Some(Json::Bool(b)) => *b,
        Some(Json::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Json::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn kind_of(row: &Json) -> Option<&'static str> {
    match str_of(row, "kind") {
        Some("definition") => Some("definition"),
        Some("function") => Some("function"),
        Some("system") => Some("system"),
        _ => None,
    }
}

fn is_row_of_kind(row: &Json) -> bool {
    kind_of(row).is_some()
}

// ---- history.ts ----

/// `ellipsize`: at most `max` UTF-16 units, the cut marked with `…`.
pub fn ellipsize(text: &str, max: i64) -> String {
    if max <= 0 {
        return String::new();
    }
    let max = max as usize;
    if len16(text) <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".into();
    }
    format!("{}…", jsstr::trim_end(&jsstr::slice16(text, max - 1)))
}

pub fn is_matrix_quantity(q: Option<&str>) -> bool {
    q.is_some_and(|q| q.starts_with("[["))
}

pub fn is_complex_quantity(q: Option<&str>) -> bool {
    static RE: qcalc_core::jsre::Lazy =
        qcalc_core::jsre::Lazy::new(|| qcalc_core::jsre::js(r"^-?[\d.e+-]+ [+-] [\d.e+-]+i$", "i"));
    q.is_some_and(|q| qcalc_core::jsre::test(&RE, q))
}

pub fn is_literal_quantity(q: Option<&str>) -> bool {
    is_matrix_quantity(q) || is_complex_quantity(q)
}

fn slim_expr(expr: &str) -> String {
    if len16(expr) <= MAX_HISTORY_EXPR {
        return expr.to_string();
    }
    let trimmed = jsstr::trim(expr);
    if let Some((name, params, body)) = parse_function_def(trimmed) {
        return with_prefix(&format!("{name}({}) = ", params.join(", ")), &body);
    }
    if let Some((variable, rhs)) = parse_assignment(trimmed) {
        return with_prefix(&format!("{variable} = "), &rhs);
    }
    ellipsize(expr, MAX_HISTORY_EXPR as i64)
}

// a definition keeps its name whole and cuts the right-hand side instead
fn with_prefix(prefix: &str, rest: &str) -> String {
    let room = (MAX_HISTORY_EXPR as i64 - len16(prefix) as i64).max(8);
    format!("{prefix}{}", ellipsize(rest, room))
}

fn sanitize_solve(raw: Option<&Json>) -> Option<Json> {
    let s = raw.filter(|v| v.is_object())?;
    let variable = str_of(s, "variable")?;
    let mut chars = variable.chars();
    let one = chars
        .next()
        .filter(|c| c.is_ascii_alphabetic() || *c == 'θ');
    if one.is_none() || chars.next().is_some() {
        return None;
    }
    let outcome = str_of(s, "outcome")?;
    if !matches!(
        outcome,
        "roots" | "none" | "contradiction" | "noneFound" | "all"
    ) {
        return None;
    }
    let roots = s.get("roots")?.as_array()?;
    if roots.len() > 8 || !roots.iter().all(|r| r.as_f64().is_some_and(f64::is_finite)) {
        return None;
    }
    let mut out = json!({ "variable": variable, "roots": roots, "outcome": outcome });
    if s.get("more") == Some(&Json::Bool(true)) {
        out["more"] = Json::Bool(true);
    }
    Some(out)
}

fn slim_equations(raw: Option<&Json>) -> Option<Json> {
    let lines: Vec<Json> = raw?
        .as_array()?
        .iter()
        .filter_map(Json::as_str)
        .take(5)
        .map(|line| Json::String(ellipsize(jsstr::trim(line), 160)))
        .collect();
    (!lines.is_empty()).then_some(Json::Array(lines))
}

/// `sanitizeMeas`, from the core (WP-MEASURE).
pub fn sanitize_meas(raw: Option<&Json>) -> Option<Json> {
    qcalc_core::measure::sanitize_meas(raw?)
}

fn slim_function_fields(row: &Json, kind: Option<&str>, expr: &str, out: &mut Map<String, Json>) {
    if kind != Some("function") {
        return;
    }
    let fn_name = str_of(row, "fnName").filter(|n| !n.is_empty());
    let fn_body = str_of(row, "fnBody");
    let fn_params = row.get("fnParams").and_then(Json::as_array);
    // only re-read the expr for fields the row is missing
    let parsed = if fn_name.is_some() && fn_body.is_some() && fn_params.is_some() {
        None
    } else {
        parse_function_def(jsstr::trim(expr))
    };
    let Some(name) = fn_name
        .map(str::to_string)
        .or_else(|| parsed.as_ref().map(|p| p.0.clone()))
    else {
        return;
    };
    let params: Vec<Json> = match fn_params {
        Some(a) => a.iter().filter(|p| p.is_string()).cloned().collect(),
        None => parsed
            .as_ref()
            .map(|p| p.1.iter().map(|s| json!(s)).collect())
            .unwrap_or_default(),
    };
    out.insert("fnName".into(), json!(name));
    out.insert("fnParams".into(), Json::Array(params));
    let body = fn_body.map(str::to_string).or_else(|| parsed.map(|p| p.2));
    if let Some(body) = body {
        let body = if len16(&body) > MAX_HISTORY_EXPR {
            ellipsize(&body, MAX_HISTORY_EXPR as i64)
        } else {
            body
        };
        out.insert("fnBody".into(), json!(body));
    }
}

/// `slimHistoryRow`: the row as stored, or None when it holds nothing.
pub fn slim_history_row(row: &Json) -> Option<Json> {
    let expr = slim_expr(str_of(row, "expr").unwrap_or(""));
    let display = ellipsize(
        str_of(row, "display").unwrap_or(""),
        MAX_HISTORY_DISPLAY as i64,
    );
    let exact = str_of(row, "exact")
        .filter(|e| !e.is_empty())
        .map(|e| ellipsize(e, MAX_HISTORY_EXACT as i64))
        .filter(|e| *e != display);
    if jsstr::trim(&expr).is_empty() && jsstr::trim(&display).is_empty() {
        return None;
    }
    let kind = kind_of(row);
    let id = ellipsize(str_of(row, "id").unwrap_or(""), 48);
    let mut out = Map::new();
    out.insert(
        "id".into(),
        json!(if id.is_empty() { "row".to_string() } else { id }),
    );
    out.insert(
        "expr".into(),
        json!(if kind == Some("system") {
            "sys"
        } else {
            expr.as_str()
        }),
    );
    out.insert("display".into(), json!(display));
    if let Some(exact) = exact {
        out.insert("exact".into(), json!(exact));
    }
    if kind != Some("function")
        && let Some(n) = finite_of(row, "n")
    {
        out.insert("n".into(), json!(n));
    }
    if kind.is_none()
        && let Some(meas) = sanitize_meas(row.get("meas"))
    {
        out.insert("meas".into(), meas);
    }
    if kind.is_none()
        && let Some(q) = str_of(row, "quantity")
    {
        // never truncated: a cut-off quantity would parse as a different one
        let room = if is_matrix_quantity(Some(q)) {
            MAX_MATRIX_LITERAL
        } else {
            MAX_HISTORY_EXPR
        };
        out.insert(
            "quantity".into(),
            json!(if len16(q) <= room { q } else { "" }),
        );
    }
    if let Some(kind) = kind {
        out.insert("kind".into(), json!(kind));
    }
    slim_function_fields(row, kind, &expr, &mut out);
    if kind == Some("system")
        && let Some(eqs) = slim_equations(row.get("equations"))
    {
        out.insert("equations".into(), eqs);
    }
    if kind.is_none()
        && let Some(solve) = sanitize_solve(row.get("solve"))
    {
        out.insert("solve".into(), solve);
    }
    if let Some(at) = finite_of(row, "at").filter(|a| *a > 0.0) {
        out.insert("at".into(), json!(at));
    }
    Some(Json::Object(out))
}

/// `normalizeHistoryRow`: any stored row (legacy `latex` rows too) as a row, or None. A row that is `null`
/// makes the TS throw, which `loadHistory` turns into an empty history; `Err` reports that.
pub fn normalize_history_row(row: &Json, fallback_id: &str) -> Result<Option<Json>, Throws> {
    if row.is_null() {
        return Err(Throws);
    }
    let strings = |key: &str| -> Option<Json> {
        row.get(key)
            .and_then(Json::as_array)
            .map(|a| Json::Array(a.iter().filter(|p| p.is_string()).cloned().collect()))
    };
    let mut r = Map::new();
    let id = str_of(row, "id")
        .filter(|id| !id.is_empty())
        .unwrap_or(fallback_id);
    r.insert("id".into(), json!(id));
    let expr = str_of(row, "expr")
        .or_else(|| str_of(row, "latex"))
        .unwrap_or("");
    r.insert("expr".into(), json!(expr));
    r.insert(
        "display".into(),
        json!(str_of(row, "display").unwrap_or("")),
    );
    for key in ["exact", "fnName", "fnBody"] {
        if let Some(s) = str_of(row, key) {
            r.insert(key.into(), json!(s));
        }
    }
    if let Some(n) = row.get("n").filter(|n| n.is_number()) {
        r.insert("n".into(), n.clone());
    }
    for key in ["meas", "quantity", "solve", "at"] {
        if let Some(v) = row.get(key).filter(|v| !v.is_null()) {
            r.insert(key.into(), v.clone());
        }
    }
    if let Some(kind) = kind_of(row) {
        r.insert("kind".into(), json!(kind));
    }
    for key in ["fnParams", "equations"] {
        if let Some(v) = strings(key) {
            r.insert(key.into(), v);
        }
    }
    Ok(slim_history_row(&Json::Object(r)))
}

/// Where the TS throws a `TypeError` (reading a field of `null`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Throws;

// keyed so variable and function names stay in separate namespaces
fn defined_name(row: &Json) -> Option<String> {
    if kind_of(row) == Some("definition") || truthy(row.get("solve")) {
        return None;
    }
    let expr = jsstr::trim(str_of(row, "expr").unwrap_or(""));
    let is_fn = kind_of(row) == Some("function");
    let name = if is_fn {
        str_of(row, "fnName")
            .map(str::to_string)
            .or_else(|| parse_function_def(expr).map(|p| p.0))
    } else {
        parse_function_def(expr).map(|p| p.0)
    };
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        return Some(format!("fn:{name}"));
    }
    if is_fn {
        return None;
    }
    parse_assignment(expr).map(|(v, _)| format!("var:{v}"))
}

// a definition outlives the last ten rows, so `x = 5` still works long after
fn with_sticky_definitions(rows: &[Json]) -> Vec<Json> {
    let start = rows.len().saturating_sub(MAX_HISTORY);
    let recent = &rows[start..];
    let mut seen: BTreeSet<String> = recent.iter().filter_map(defined_name).collect();
    let mut kept = Vec::new();
    for row in rows[..start].iter().rev() {
        let Some(name) = defined_name(row) else {
            continue;
        };
        if !seen.insert(name) {
            continue;
        }
        kept.push(row.clone());
    }
    kept.reverse();
    kept.extend(recent.iter().cloned());
    kept
}

/// `persistableHistory`: what is kept, slimmed, under `MAX_HISTORY_JSON` characters.
pub fn persistable_history(rows: &[Json]) -> Vec<Json> {
    let mut out: Vec<Json> = with_sticky_definitions(rows)
        .iter()
        .filter_map(slim_history_row)
        .collect();
    let mut start = 0;
    while start < out.len()
        && jsstr::json_len(&Json::Array(out[start..].to_vec())) > MAX_HISTORY_JSON
    {
        start += 1;
    }
    out.drain(..start);
    out
}

fn assignment_of(row: &Json) -> Option<String> {
    parse_assignment(jsstr::trim(str_of(row, "expr").unwrap_or(""))).map(|(v, _)| v)
}

/// `historyVariables`, as `[name, value]` pairs in the JS object's insertion order.
pub fn history_variables(rows: &[Json]) -> Vec<(String, f64)> {
    let mut vars: Vec<(String, f64)> = Vec::new();
    let remove = |vars: &mut Vec<(String, f64)>, name: &str| vars.retain(|(k, _)| k != name);
    let set = |vars: &mut Vec<(String, f64)>, name: String, n: f64| match vars
        .iter_mut()
        .find(|(k, _)| *k == name)
    {
        Some(slot) => slot.1 = n,
        None => vars.push((name, n)),
    };
    for row in rows {
        if is_row_of_kind(row) || truthy(row.get("solve")) {
            continue;
        }
        let Some(variable) = assignment_of(row) else {
            continue;
        };
        if present(row, "quantity") {
            remove(&mut vars, &variable);
            continue;
        }
        if let Some(n) = finite_of(row, "n") {
            set(&mut vars, variable, n);
            continue;
        }
        // legacy rows may lack `n`
        let display = match row.get("display") {
            Some(Json::String(s)) => s.clone(),
            Some(Json::Number(n)) => qcalc_core::jsnum::to_string(n.as_f64().unwrap_or(f64::NAN)),
            Some(Json::Bool(b)) => b.to_string(),
            Some(Json::Null) => "null".into(),
            _ => "undefined".into(),
        };
        let shown = jsstr::trim(&display);
        if !shown.is_empty()
            && let Some(n) = jsstr::number(shown).filter(|n| n.is_finite())
        {
            set(&mut vars, variable, n);
            continue;
        }
        let options = EvaluateOptions {
            variables: vars.iter().cloned().collect(),
            ..EvaluateOptions::default()
        };
        let result = qcalc_core::evaluate_line(str_of(row, "expr").unwrap_or(""), &options);
        if let Some(v) = result
            .value
            .filter(|v| v.kind == ValueKind::Number && v.n.is_finite())
        {
            set(&mut vars, variable, v.n);
        }
    }
    vars
}

fn counts_as_ans(row: &Json) -> bool {
    finite_of(row, "n").is_some() || is_literal_quantity(str_of(row, "quantity"))
}

/// `historyMeasures`: raw `meas` per variable, and `ans`.
pub fn history_measures(rows: &[Json]) -> Map<String, Json> {
    let mut out = Map::new();
    let mut last: Option<&Json> = None;
    for row in rows {
        if is_row_of_kind(row) {
            continue;
        }
        if counts_as_ans(row) {
            last = Some(row);
        }
        if truthy(row.get("solve")) {
            continue;
        }
        let Some(variable) = assignment_of(row) else {
            continue;
        };
        if truthy(row.get("meas")) && !present(row, "quantity") {
            out.insert(variable, row["meas"].clone());
        } else {
            out.remove(&variable);
        }
    }
    if let Some(last) = last.filter(|r| truthy(r.get("meas")) && !present(r, "quantity")) {
        out.insert("ans".into(), last["meas"].clone());
    }
    out
}

/// `historyQuantities`: unit and literal answers per variable, and `ans`.
pub fn history_quantities(rows: &[Json]) -> Map<String, Json> {
    let mut out = Map::new();
    let mut last: Option<&Json> = None;
    for row in rows {
        if is_row_of_kind(row) {
            continue;
        }
        if counts_as_ans(row) {
            last = Some(row);
        }
        if truthy(row.get("solve")) {
            continue;
        }
        let Some(variable) = assignment_of(row) else {
            continue;
        };
        if present(row, "quantity") {
            out.insert(variable, row["quantity"].clone());
        } else {
            out.remove(&variable);
        }
    }
    if let Some(last) = last.filter(|r| present(r, "quantity")) {
        out.insert("ans".into(), last["quantity"].clone());
    }
    out
}

/// `historyFunctions`: name → `{params, body}`.
pub fn history_functions(rows: &[Json]) -> Map<String, Json> {
    let mut fns = Map::new();
    for row in rows {
        let kind = kind_of(row);
        if kind == Some("definition") || kind == Some("system") {
            continue;
        }
        if kind == Some("function")
            && let Some(name) = str_of(row, "fnName").filter(|n| !n.is_empty())
            && present(row, "fnBody")
        {
            let params = row
                .get("fnParams")
                .filter(|p| p.is_array())
                .cloned()
                .unwrap_or(json!([]));
            fns.insert(
                name.to_string(),
                json!({ "params": params, "body": row["fnBody"] }),
            );
            continue;
        }
        if let Some((name, params, body)) =
            parse_function_def(jsstr::trim(str_of(row, "expr").unwrap_or("")))
        {
            fns.insert(name, json!({ "params": params, "body": body }));
        }
    }
    fns
}

fn several_roots(row: &Json) -> bool {
    row.get("solve").is_some_and(|s| {
        s.get("outcome").and_then(Json::as_str) == Some("roots")
            && s.get("roots")
                .and_then(Json::as_array)
                .is_some_and(|r| r.len() > 1)
    })
}

/// `lastHistoryNumber`: what `ans` is, unless it's a unit, literal or several-root answer.
pub fn last_history_number(rows: &[Json]) -> Option<f64> {
    for row in rows.iter().rev() {
        if row.is_null() || is_row_of_kind(row) {
            continue;
        }
        // a matrix answer is `ans` now, through historyQuantities
        if is_literal_quantity(str_of(row, "quantity")) {
            return None;
        }
        // several roots are no one number; the one wanted is clicked in the tape
        if several_roots(row) {
            return None;
        }
        // a unit answer's `ans` comes from historyQuantities instead
        if let Some(n) = finite_of(row, "n") {
            return (!present(row, "quantity")).then_some(n);
        }
    }
    None
}

/// `lastAnswerRow` (`src/components/QuickCalc.tsx`): the row `ans` means.
pub fn last_answer_row(rows: &[Json]) -> Option<&Json> {
    for row in rows.iter().rev() {
        if row.is_null() || is_row_of_kind(row) {
            continue;
        }
        let solve = row.get("solve").filter(|s| truthy(Some(s)));
        // `ans` never becomes "no real solution"
        if solve.is_some_and(|s| s.get("outcome").and_then(Json::as_str) != Some("roots")) {
            continue;
        }
        // nor a list of roots: the one wanted is clicked in the tape
        if solve.is_some_and(|s| {
            s.get("roots")
                .and_then(Json::as_array)
                .is_some_and(|r| r.len() > 1)
        }) {
            return None;
        }
        if finite_of(row, "n").is_some()
            || !jsstr::trim(str_of(row, "display").unwrap_or("")).is_empty()
        {
            return Some(row);
        }
    }
    None
}

// ---- historyShow.ts ----

pub fn normalize_history_show(value: &Json) -> HistoryShow {
    match value.as_str() {
        Some("always") => HistoryShow::Always,
        Some("arrow") => HistoryShow::Arrow,
        _ => HistoryShow::Recent,
    }
}

fn is_recent(row: &Json, now: f64) -> bool {
    row.get("at")
        .and_then(Json::as_f64)
        .is_some_and(|at| at <= now && now - at <= RECENT_MS)
}

/// `recentStart`: the first row of the recent view; `rows.len()` when there is none.
pub fn recent_start(rows: &[Json], now: f64) -> usize {
    let mut i = rows.len();
    while i > 0 && rows.len() - i < RECENT_ROWS && is_recent(&rows[i - 1], now) {
        i -= 1;
    }
    i
}

/// `scopeStart`: variables and functions only count from the rows you can see.
pub fn scope_start(rows: &[Json], show: HistoryShow, now: f64) -> usize {
    if show == HistoryShow::Always {
        0
    } else {
        recent_start(rows, now)
    }
}

/// `nextRecentExpiry`: when the recent view next changes on its own.
pub fn next_recent_expiry(rows: &[Json], now: f64) -> Option<f64> {
    rows[recent_start(rows, now)..]
        .iter()
        .filter_map(|r| r.get("at").and_then(Json::as_f64))
        .map(|at| at + RECENT_MS + 1.0)
        .reduce(f64::min)
}

// ---- typed rows at the FFI boundary ----

pub fn meas_json(m: &MeasInfo) -> Json {
    let mut out = Map::new();
    if let Some(s) = m.sig {
        out.insert("sig".into(), json!(s));
    }
    if let Some(d) = m.dp {
        out.insert("dp".into(), json!(d));
    }
    if let Some(u) = m.unc {
        out.insert("unc".into(), json!(u));
    }
    if let Some(d) = &m.unc_digits {
        out.insert("uncDigits".into(), json!(d));
    }
    Json::Object(out)
}

pub fn meas_from_json(v: &Json) -> Option<MeasInfo> {
    let o = v.as_object()?;
    let int = |k: &str| {
        o.get(k)
            .and_then(Json::as_f64)
            .filter(|x| x.is_finite())
            .map(|x| x as i32)
    };
    Some(MeasInfo {
        sig: int("sig"),
        dp: int("dp"),
        unc: o.get("unc").and_then(Json::as_f64),
        unc_digits: o
            .get("uncDigits")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

fn outcome_str(o: SolveOutcome) -> &'static str {
    match o {
        SolveOutcome::Roots => "roots",
        SolveOutcome::None => "none",
        SolveOutcome::Contradiction => "contradiction",
        SolveOutcome::NoneFound => "noneFound",
        SolveOutcome::All => "all",
    }
}

pub fn solve_json(s: &SolveInfo) -> Json {
    let mut out =
        json!({ "variable": s.variable, "roots": s.roots, "outcome": outcome_str(s.outcome) });
    if s.more {
        out["more"] = Json::Bool(true);
    }
    out
}

pub fn solve_from_json(v: &Json) -> Option<SolveInfo> {
    let outcome = match v.get("outcome")?.as_str()? {
        "roots" => SolveOutcome::Roots,
        "none" => SolveOutcome::None,
        "contradiction" => SolveOutcome::Contradiction,
        "noneFound" => SolveOutcome::NoneFound,
        "all" => SolveOutcome::All,
        _ => return None,
    };
    Some(SolveInfo {
        variable: v.get("variable")?.as_str()?.to_string(),
        roots: v
            .get("roots")?
            .as_array()?
            .iter()
            .filter_map(Json::as_f64)
            .collect(),
        outcome,
        more: v.get("more") == Some(&Json::Bool(true)),
    })
}

/// A typed row as the TS `HistoryRow` JSON (absent fields left out, as `JSON.stringify` drops `undefined`).
pub fn row_json(r: &HistoryRow) -> Json {
    let mut o = Map::new();
    o.insert("id".into(), json!(r.id));
    o.insert("expr".into(), json!(r.expr));
    o.insert("display".into(), json!(r.display));
    let mut opt = |k: &str, v: Option<Json>| {
        if let Some(v) = v {
            o.insert(k.into(), v);
        }
    };
    opt("exact", r.exact.as_ref().map(|s| json!(s)));
    opt("n", r.n.filter(|n| n.is_finite()).map(|n| json!(n)));
    opt("meas", r.meas.as_ref().map(meas_json));
    opt("quantity", r.quantity.as_ref().map(|s| json!(s)));
    opt(
        "kind",
        r.kind.map(|k| {
            json!(match k {
                HistoryKind::Definition => "definition",
                HistoryKind::Function => "function",
                HistoryKind::System => "system",
            })
        }),
    );
    opt("fnName", r.fn_name.as_ref().map(|s| json!(s)));
    opt("fnParams", r.fn_params.as_ref().map(|s| json!(s)));
    opt("fnBody", r.fn_body.as_ref().map(|s| json!(s)));
    opt("equations", r.equations.as_ref().map(|s| json!(s)));
    opt("solve", r.solve.as_ref().map(solve_json));
    opt("at", r.at.map(|a| json!(a)));
    Json::Object(o)
}

/// A slimmed JSON row as the typed record.
pub fn row_from_json(v: &Json) -> HistoryRow {
    let s = |k: &str| str_of(v, k).map(str::to_string);
    let strings = |k: &str| {
        v.get(k).and_then(Json::as_array).map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
    };
    HistoryRow {
        id: s("id").unwrap_or_default(),
        expr: s("expr").unwrap_or_default(),
        display: s("display").unwrap_or_default(),
        exact: s("exact"),
        n: finite_of(v, "n"),
        meas: v.get("meas").and_then(meas_from_json),
        quantity: s("quantity"),
        kind: match kind_of(v) {
            Some("definition") => Some(HistoryKind::Definition),
            Some("function") => Some(HistoryKind::Function),
            Some("system") => Some(HistoryKind::System),
            _ => None,
        },
        equations: strings("equations"),
        fn_name: s("fnName"),
        fn_params: strings("fnParams"),
        fn_body: s("fnBody"),
        solve: v.get("solve").and_then(solve_from_json),
        at: finite_of(v, "at").map(|a| a as i64),
    }
}

/// `newRowId`: base-36 time and a random-looking tail, unique within the process.
pub fn new_row_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // a splitmix of time and counter, so ids made in the same millisecond differ
    let mut z = now ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    format!(
        "{}-{}",
        base36(now),
        &format!("{:0>6}", base36(z ^ (z >> 31)))[..6]
    )
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

// ---- the history file ----

/// The file's rows, from `{"version": 1, "rows": […]}` or a bare array, normalized and slimmed (`loadHistory`).
/// Anything unreadable is an empty history, as in the TS.
pub fn load_rows(text: &str) -> Vec<Json> {
    let Ok(parsed) = serde_json::from_str::<Json>(text) else {
        return Vec::new();
    };
    let rows = match &parsed {
        Json::Array(a) => a.clone(),
        Json::Object(o) => match o.get("rows") {
            Some(Json::Array(a)) => a.clone(),
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for row in &rows {
        match normalize_history_row(row, &new_row_id()) {
            Ok(Some(r)) => out.push(r),
            Ok(None) => {}
            // TS quirk kept: one null row throws inside loadHistory's try, so the whole history loads empty
            Err(Throws) => return Vec::new(),
        }
    }
    persistable_history(&out)
}

/// What `save_history` writes.
pub fn file_text(rows: &[Json]) -> String {
    serde_json::to_string(&json!({ "version": 1, "rows": rows })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsizes_in_utf16() {
        assert_eq!(ellipsize("abcdefghij", 6), "abcde…");
        assert_eq!(ellipsize("ab", 1), "…");
        assert_eq!(ellipsize("hello", 0), "");
        assert_eq!(ellipsize("ab cd", 4), "ab…");
    }

    #[test]
    fn typed_rows_round_trip() {
        let row = json!({"id": "a", "expr": "x = 2", "display": "2", "n": 2.0, "at": 5,
            "solve": {"variable": "x", "roots": [1.0, 2.0], "outcome": "roots"}});
        let typed = row_from_json(&row);
        assert_eq!(typed.n, Some(2.0));
        assert_eq!(typed.solve.as_ref().map(|s| s.roots.len()), Some(2));
        assert_eq!(row_json(&typed), row);
    }

    #[test]
    fn loads_versioned_and_bare_files() {
        let rows =
            load_rows(r#"{"version":1,"rows":[{"id":"1","expr":"2+2","display":"4","n":4}]}"#);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            load_rows(r#"[{"latex":"1+1","display":"2"}]"#)[0]["expr"],
            "1+1"
        );
        assert!(load_rows(r#"[{"expr":"1+1","display":"2"}, null]"#).is_empty());
        assert!(load_rows("nope").is_empty());
        assert_ne!(new_row_id(), new_row_id());
    }
}
