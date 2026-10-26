//! The ECB rates the engine holds (`set_rates`, `reload_rates`). Parsing, freshness and the cache file live in
//! `qcalc_core::phrases::rates` (WP-PHRASES), in the shape and rules of `src-tauri/src/rates.rs` and
//! `macos/ECBRates.swift`; this adapts them to the FFI's records and errors.

pub use qcalc_core::phrases::rates::Rates;

use crate::records::{CoreError, RatesStatus};

/// The cache file's JSON or a `{base, rates}` table. `Ok(None)` is a well-formed cache with no usable rates.
pub fn parse(text: &str) -> Result<Option<Rates>, CoreError> {
    qcalc_core::phrases::rates::parse_json(text).map_err(CoreError::bad)
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

    #[test]
    fn status_of_the_cache() {
        let text = r#"{"checked":1790000000.5,"date":"2026-09-24","fetched":1790000000.25,
            "rates":{"USD":1.1367,"JPY":180.57}}"#;
        let r = parse(text).unwrap().unwrap();
        let s = status(Some(&r), 1_790_000_000_250);
        assert_eq!(
            (s.currencies, s.fresh, s.date.as_deref()),
            (2, true, Some("2026-09-24"))
        );
        assert!(parse("nope").is_err());
    }
}
