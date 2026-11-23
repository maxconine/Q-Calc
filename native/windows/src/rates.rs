//! Currency rates: the ECB's daily reference rates, fetched on a background thread and kept in
//! `<config dir>\rates.json`, the file the Tauri app and the engine share (`{checked, fetched, date, rates}`).
//!
//! The rules are the core's (`qcalc_core::phrases::rates`, the same as src-tauri/src/rates.rs and the Mac's
//! ECBRates.swift): strict parsing of the daily XML (one envelope, one dated cube, three-letter codes, plain
//! decimals, a currency twice refuses the file), the ECB asked at most once a day, rates over 7 days old unused
//! (the engine's `rates_status` applies that). Here only the HTTP (net.rs, WinHTTP) and the timing.
//!
//! The engine reads the file at launch (`reload_rates` in app.rs's warm-up). This thread starts after the first
//! frame (bg.rs), waits a few seconds more so launch work settles, refreshes when due, and when the file changed
//! asks the engine to read it again (Lane::Misc). It then sleeps until the next day's check is due.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qcalc_core::phrases::rates::{self, Cache, DAY_S, Fetch};

use crate::engine::Lane;
use crate::qlog;

const FIRST_WAIT: Duration = Duration::from_secs(5);
/// a failed try (offline, the ECB down, a captive portal's page) is tried again this soon, not a day later
/// (the Mac's ECBRates `retry`); cached rates are kept meanwhile
pub const RETRY_S: f64 = 30.0 * 60.0;

struct WinHttp;

impl Fetch for WinHttp {
    fn get(&self, url: &str, limit: usize) -> Option<String> {
        match crate::net::get(url, limit) {
            Ok(bytes) => String::from_utf8(bytes).ok(),
            Err(e) => {
                qlog!("rates: fetch failed: {e}");
                None
            }
        }
    }
}

fn now_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// How long to sleep before the next check: until a day after the last one (at least a minute, at most a day).
pub fn next_wait(cache: &Cache, now: f64) -> Duration {
    let due_in = cache.checked + DAY_S - now;
    Duration::from_secs_f64(due_in.clamp(60.0, DAY_S))
}

/// After a try at `now` that brought nothing usable: due again after `RETRY_S`, not a day (ECBRates `failed`).
pub fn failed(cache: &mut Cache, now: f64) {
    cache.checked = cache.checked.min(now - DAY_S + RETRY_S);
}

/// Whether the check just made (`before` to `after`) asked the ECB and got nothing: `checked` moved, `fetched`
/// didn't.
pub fn fetch_failed(before: &Cache, after: &Cache) -> bool {
    after.checked != before.checked && after.fetched == before.fetched
}

pub fn start() {
    let Some(path) = crate::store::config_dir().map(|d| d.join("rates.json")) else {
        return;
    };
    crate::bg::spawn("qcalc-rates", move || {
        std::thread::sleep(FIRST_WAIT);
        loop {
            let before = Cache::load(&path);
            let started = now_s();
            let (mut cache, changed) = rates::refresh(&path, started, &WinHttp, now_s);
            if fetch_failed(&before, &cache) {
                failed(&mut cache, started);
                let _ = cache.save(&path);
                qlog!("rates: no rates from the ECB; trying again in 30 minutes");
            }
            if cache.checked != before.checked {
                qlog!(
                    "rates: checked; {} currencies dated {}{}",
                    cache.rates.as_ref().map_or(0, |r| r.rates.len()),
                    cache.rates.as_ref().map_or("-", |r| r.date.as_str()),
                    if changed { ", engine reloads" } else { "" }
                );
            }
            if changed {
                crate::bg::post(|app| {
                    app.engine.submit(Lane::Misc, |cx| {
                        if let Err(e) = cx.engine.reload_rates() {
                            qlog!("rates: reload failed: {e}");
                        }
                        None
                    });
                });
            }
            std::thread::sleep(next_wait(&cache, now_s()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeps_until_the_next_day_is_due() {
        let at = 1_790_000_000.0;
        let cache = Cache {
            checked: at,
            ..Cache::default()
        };
        assert_eq!(next_wait(&cache, at), Duration::from_secs_f64(DAY_S));
        assert_eq!(
            next_wait(&cache, at + DAY_S - 3600.0),
            Duration::from_secs(3600)
        );
        // overdue, or a clock gone backwards: soon, but never a busy loop
        assert_eq!(next_wait(&cache, at + 2.0 * DAY_S), Duration::from_secs(60));
        assert_eq!(next_wait(&Cache::default(), at), Duration::from_secs(60));
    }

    #[test]
    fn a_failed_fetch_is_tried_again_in_half_an_hour() {
        let at = 1_790_000_000.0;
        let before = Cache {
            checked: at - 2.0 * DAY_S,
            fetched: at - 2.0 * DAY_S,
            ..Cache::default()
        };
        let mut after = Cache {
            checked: at,
            ..before.clone()
        };
        assert!(fetch_failed(&before, &after));
        failed(&mut after, at);
        assert_eq!(next_wait(&after, at), Duration::from_secs_f64(RETRY_S));
        // a fetch that worked waits the day
        let ok = Cache {
            checked: at,
            fetched: at + 1.0,
            ..before.clone()
        };
        assert!(!fetch_failed(&before, &ok));
        // not asked (not due): nothing failed
        assert!(!fetch_failed(&before, &before));
    }
}
