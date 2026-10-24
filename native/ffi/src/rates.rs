//! The ECB rates cache the engine reads (`set_rates`, `reload_rates`). It uses the shape and rules of
//! `src-tauri/src/rates.rs` and `macos/ECBRates.swift`: euro base, units per euro, 3-letter upper-case codes,
//! finite positive rates, and a dated cube. Rates more than 7 days old by the request's clock aren't used.
//! The XML parsing and the fetch stay in ECBRates.swift until WP-PHRASES ports the parser and `qcalc-net` does
//! the fetch.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::records::{CoreError, RatesStatus};

const DAY_S: f64 = 24.0 * 60.0 * 60.0;
const STALE_S: f64 = 7.0 * DAY_S;

/// Usable rates.
#[derive(Clone, Debug, PartialEq)]
pub struct Rates {
    pub base: String,
    pub date: Option<String>,
    pub rates: BTreeMap<String, f64>,
    /// When they were fetched, in seconds since 1970. None for a `{base, rates}` table, which is always fresh.
    pub fetched: Option<f64>,
}

fn is_code(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Reads the cache file's JSON, field by field so one bad value can't throw the rest away; or a `{base, rates}`
/// table (`window.__QCALC_RATES`). `Ok(None)` is a well-formed cache with no usable rates (a failed first fetch).
pub fn parse(text: &str) -> Result<Option<Rates>, CoreError> {
    let v: Json = serde_json::from_str(text).map_err(CoreError::bad)?;
    let Some(o) = v.as_object() else {
        return Err(CoreError::bad("rates JSON must be an object"));
    };
    let base = o.get("base").and_then(Json::as_str);
    let rates: BTreeMap<String, f64> = o
        .get("rates")
        .and_then(Json::as_object)
        .map(|all| {
            all.iter()
                .filter(|(code, _)| is_code(code) && *code != base.unwrap_or("EUR"))
                .filter_map(|(code, n)| {
                    n.as_f64()
                        .filter(|n| n.is_finite() && *n > 0.0)
                        .map(|n| (code.clone(), n))
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(base) = base {
        // the page's table: whatever base it names, no date or freshness
        return Ok((!rates.is_empty()).then(|| Rates {
            base: base.to_string(),
            date: None,
            rates,
            fetched: None,
        }));
    }
    let date = o
        .get("date")
        .and_then(Json::as_str)
        .filter(|d| is_date(d))
        .map(str::to_string);
    let fetched = o.get("fetched").and_then(Json::as_f64).unwrap_or(0.0);
    Ok(match date {
        Some(date) if !rates.is_empty() => Some(Rates {
            base: "EUR".into(),
            date: Some(date),
            rates,
            fetched: Some(fetched),
        }),
        _ => None,
    })
}

impl Rates {
    pub fn fresh(&self, now_ms: i64) -> bool {
        let Some(fetched) = self.fetched else {
            return true;
        };
        let now = now_ms as f64 / 1000.0;
        now >= fetched && now - fetched < STALE_S
    }

    /// What the phrase layer takes as `rates` (`CurrencyRates`), or null when stale.
    pub fn table(&self, now_ms: i64) -> Json {
        if self.fresh(now_ms) {
            json!({ "base": self.base, "rates": self.rates })
        } else {
            Json::Null
        }
    }
}

pub fn status(rates: Option<&Rates>, now_ms: i64) -> RatesStatus {
    RatesStatus {
        date: rates.and_then(|r| r.date.clone()),
        currencies: rates.map_or(0, |r| r.rates.len() as u32),
        fresh: rates.is_some_and(|r| r.fresh(now_ms)),
        fetched_ms: rates.and_then(|r| r.fetched).map(|s| (s * 1000.0) as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_MS: i64 = 86_400_000;

    #[test]
    fn reads_the_cache_ecbrates_writes() {
        // ECBRates.swift writes seconds as doubles, sorted keys
        let text = r#"{"checked":1790000000.5,"date":"2026-09-24","fetched":1790000000.25,
            "rates":{"USD":1.1367,"usd":2,"XAU":-1,"EUR":1,"GBP":"0.8","JPY":180.57}}"#;
        let r = parse(text).unwrap().unwrap();
        assert_eq!(r.rates.len(), 2);
        assert_eq!(r.rates["USD"], 1.1367);
        let at = 1_790_000_000_250;
        assert!(r.fresh(at + DAY_MS));
        assert!(!r.fresh(at + 8 * DAY_MS));
        assert!(!r.fresh(at - 1000));
        assert_eq!(r.table(at)["base"], "EUR");
        assert_eq!(r.table(at + 8 * DAY_MS), Json::Null);
        let s = status(Some(&r), at);
        assert_eq!(
            (s.currencies, s.fresh, s.date.as_deref()),
            (2, true, Some("2026-09-24"))
        );
    }

    #[test]
    fn page_tables_and_junk() {
        let r = parse(r#"{"base":"EUR","rates":{"USD":1.25}}"#)
            .unwrap()
            .unwrap();
        assert!(r.fresh(0));
        assert_eq!(parse(r#"{"checked":5}"#).unwrap(), None);
        assert!(parse("[1]").is_err());
        assert!(parse("nope").is_err());
        assert_eq!(parse(r#"{"date":"2026-09-24","rates":{}}"#).unwrap(), None);
        assert_eq!(
            parse(r#"{"date":"yesterday","rates":{"USD":1}}"#).unwrap(),
            None
        );
    }
}
