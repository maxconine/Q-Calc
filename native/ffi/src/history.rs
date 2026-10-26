//! The tape at the FFI boundary. The logic (`src/lib/history.ts`, `historyShow.ts`) lives in the core,
//! `qcalc_core::app::history` and `app::history_show`, and is re-exported here; this file keeps what is the FFI's
//! own: the typed `HistoryRow` record to and from the TS JSON shape, row ids from the clock, and the history file.

use qcalc_core::app::history_show as show;
use serde_json::{Map, Value as Json, json};

use crate::records::{HistoryKind, HistoryRow, HistoryShow, MeasInfo, SolveInfo, SolveOutcome};

pub use qcalc_core::app::history::{
    MAX_HISTORY, MAX_HISTORY_DISPLAY, MAX_HISTORY_EXACT, MAX_HISTORY_EXPR, MAX_HISTORY_JSON,
    Throws, ellipsize, file_text, history_functions, history_measures, history_quantities,
    history_variables, is_complex_quantity, is_literal_quantity, is_matrix_quantity,
    last_answer_row, last_history_number, normalize_history_row, persistable_history,
    slim_history_row,
};
pub use qcalc_core::app::history_show::{RECENT_MS, RECENT_ROWS, next_recent_expiry, recent_start};

fn str_of<'a>(row: &'a Json, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Json::as_str)
}

fn finite_of(row: &Json, key: &str) -> Option<f64> {
    row.get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite())
}

fn kind_of(row: &Json) -> Option<&'static str> {
    match str_of(row, "kind") {
        Some("definition") => Some("definition"),
        Some("function") => Some("function"),
        Some("system") => Some("system"),
        _ => None,
    }
}

fn to_core(s: HistoryShow) -> show::HistoryShow {
    match s {
        HistoryShow::Recent => show::HistoryShow::Recent,
        HistoryShow::Always => show::HistoryShow::Always,
        HistoryShow::Arrow => show::HistoryShow::Arrow,
    }
}

// ---- historyShow.ts, with the record enum ----

pub fn normalize_history_show(value: &Json) -> HistoryShow {
    match show::normalize_history_show(value) {
        show::HistoryShow::Recent => HistoryShow::Recent,
        show::HistoryShow::Always => HistoryShow::Always,
        show::HistoryShow::Arrow => HistoryShow::Arrow,
    }
}

/// `scopeStart`: variables and functions only count from the rows you can see.
pub fn scope_start(rows: &[Json], s: HistoryShow, now: f64) -> usize {
    show::scope_start(rows, to_core(s), now)
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
pub fn load_rows(text: &str) -> Vec<Json> {
    qcalc_core::app::history::load_rows(text, &mut new_row_id)
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
