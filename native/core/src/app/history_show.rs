//! Which part of the tape shows and counts (`src/lib/historyShow.ts`): the recent rows above the bar, the whole
//! tape, or nothing until ↑; variables only count from rows you can see.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// recent: the last few calculations sit above the bar on their own; always: the full tape is open; arrow:
/// nothing until ↑.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryShow {
    #[default]
    Recent,
    Always,
    Arrow,
}

pub const RECENT_MS: f64 = 5.0 * 60.0 * 1000.0;
pub const RECENT_ROWS: usize = 5;


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

