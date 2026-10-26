//! Which part of the tape shows and counts (`src/lib/historyShow.ts`): the recent rows above the bar, the whole
//! tape, or nothing until ↑; variables only count from rows you can see.

use serde::{Deserialize, Serialize};

use crate::app::history::HistoryRow;

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

/// `normalizeHistoryShow`: anything unknown is `recent`.
pub fn normalize_history_show(value: &serde_json::Value) -> HistoryShow {
    match value.as_str() {
        Some("always") => HistoryShow::Always,
        Some("arrow") => HistoryShow::Arrow,
        _ => HistoryShow::Recent,
    }
}

fn is_recent(row: &HistoryRow, now: f64) -> bool {
    row.at.is_some_and(|at| at <= now && now - at <= RECENT_MS)
}

/// Index of the first row in the recent view; `history.len()` when there is none. Always a tail of the tape, so
/// row indices mean the same thing in both.
pub fn recent_start(history: &[HistoryRow], now: f64) -> usize {
    let mut i = history.len();
    while i > 0 && history.len() - i < RECENT_ROWS && is_recent(&history[i - 1], now) {
        i -= 1;
    }
    i
}

/// Where the rows that give variables and functions start: the whole tape when it's always open, otherwise the
/// recent rows, even when ↑ has opened the rest.
pub fn scope_start(history: &[HistoryRow], show: HistoryShow, now: f64) -> usize {
    if show == HistoryShow::Always {
        0
    } else {
        recent_start(history, now)
    }
}

/// When the recent view next changes on its own (ms since the epoch), or None if it won't.
pub fn next_recent_expiry(history: &[HistoryRow], now: f64) -> Option<f64> {
    history[recent_start(history, now)..]
        .iter()
        .map(|row| row.at.unwrap_or(f64::NAN) + RECENT_MS + 1.0)
        .reduce(|a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) })
}
