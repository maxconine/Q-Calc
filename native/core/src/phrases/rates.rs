//! The ECB's daily reference rates, which currency conversions use: euro is the base, each rate is units per
//! euro. The same source, cache file and rules as `src-tauri/src/rates.rs` and `macos/ECBRates.swift`:
//! `<data dir>/rates.json` (`~/Library/Application Support/Q Calc/rates.json` on the Mac) holding
//! `{checked, fetched, date, rates}` in seconds since 1970, the ECB asked at most once a day, and rates more than
//! 7 days old not used.
//!
//! This is the one module of the core with I/O: [`Cache::load`]/[`Cache::save`] read and write the cache file.
//! The network sits behind [`Fetch`], so the core stays testable and offline; the app supplies the HTTP client.
//! Nothing here reads the clock: `now` comes in.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use roxmltree::{Document, Node};
use serde_json::{Value as Json, json};

use super::currency::CurrencyRates;

pub const URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const GESMES: &str = "http://www.gesmes.org/xml/2002-08-01";
const EUROFXREF: &str = "http://www.ecb.int/vocabulary/2002-08-01/eurofxref";
pub const DAY_S: f64 = 24.0 * 60.0 * 60.0;
// older than this and a conversion would quote a rate nobody trades at any more
pub const STALE_S: f64 = 7.0 * DAY_S;
/// The most of the daily file a fetch reads.
pub const MAX_BYTES: usize = 1 << 20;

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

// plain decimals only: no signs, exponents, inf or nan
fn rate(s: &str) -> Option<f64> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, "0"));
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !digits(frac) {
        return None;
    }
    s.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0)
}

fn finite_positive(n: &Json) -> Option<f64> {
    n.as_f64().filter(|n| n.is_finite() && *n > 0.0)
}

/// One day's rates as the ECB published them.
#[derive(Clone, Debug, PartialEq)]
pub struct Daily {
    pub date: String,
    pub rates: BTreeMap<String, f64>,
}

fn cubes<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(|n| {
        n.is_element() && n.tag_name().name() == "Cube" && n.tag_name().namespace() == Some(EUROFXREF)
    })
}

fn only<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

/// The daily file: the envelope, one outer cube, one dated cube, then currency cubes; anything else and nothing
/// is trusted. Bad codes and rates are skipped; a currency twice refuses the file.
pub fn parse_xml(xml: &str) -> Option<Daily> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "Envelope" || root.tag_name().namespace() != Some(GESMES) {
        return None;
    }
    let day = only(cubes(only(cubes(root))?))?;
    let date = day.attribute("time").filter(|d| is_date(d))?.to_string();
    let mut rates = BTreeMap::new();
    for cube in cubes(day) {
        let (Some(code), Some(value)) = (
            cube.attribute("currency"),
            cube.attribute("rate").and_then(rate),
        ) else {
            continue;
        };
        if !is_code(code) || code == "EUR" {
            continue;
        }
        if rates.insert(code.to_string(), value).is_some() {
            return None;
        }
    }
    (!rates.is_empty()).then_some(Daily { date, rates })
}

/// The cache file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cache {
    /// The last attempt, kept even when it failed, so the ECB is asked at most once a day.
    pub checked: f64,
    pub fetched: f64,
    pub rates: Option<Daily>,
}

impl Cache {
    /// Field by field, so one bad value can't throw the rest away. Seconds may be whole (the Windows app) or
    /// fractional (`ECBRates.swift`).
    pub fn from_json(text: &str) -> Cache {
        let v: Json = serde_json::from_str(text).unwrap_or(Json::Null);
        let rates = v["rates"].as_object().map(|all| {
            all.iter()
                .filter(|(code, _)| is_code(code) && *code != "EUR")
                .filter_map(|(code, n)| finite_positive(n).map(|n| (code.clone(), n)))
                .collect::<BTreeMap<_, _>>()
        });
        let date = v["date"].as_str().filter(|d| is_date(d)).map(str::to_string);
        let seconds = |k: &str| v[k].as_f64().filter(|n| n.is_finite() && *n >= 0.0).unwrap_or(0.0);
        Cache {
            checked: seconds("checked"),
            fetched: seconds("fetched"),
            rates: match (date, rates) {
                (Some(date), Some(rates)) if !rates.is_empty() => Some(Daily { date, rates }),
                _ => None,
            },
        }
    }

    /// Whole seconds, which both apps read back.
    pub fn to_json(&self) -> String {
        let (date, rates) = self
            .rates
            .as_ref()
            .map_or((None, None), |r| (Some(&r.date), Some(&r.rates)));
        let whole = |s: f64| s.max(0.0).floor() as u64;
        json!({ "checked": whole(self.checked), "fetched": whole(self.fetched), "date": date, "rates": rates })
            .to_string()
    }

    pub fn load(path: &Path) -> Cache {
        fs::read_to_string(path)
            .map(|text| Cache::from_json(&text))
            .unwrap_or_default()
    }

    /// Written to a temporary file and renamed, so a crash can't leave half a cache.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, self.to_json())?;
        fs::rename(&tmp, path)
    }

    /// A clock that went backwards counts as due, or it could wait for years.
    pub fn due(&self, now_s: f64) -> bool {
        self.checked == 0.0 || now_s < self.checked || now_s - self.checked >= DAY_S
    }

    /// The rates, unless they are stale.
    pub fn fresh_rates(&self, now_s: f64) -> Option<&Daily> {
        let fresh = now_s >= self.fetched && now_s - self.fetched < STALE_S;
        self.rates.as_ref().filter(|_| fresh)
    }

    /// What the page got as `window.__QCALC_RATES`: `{base, rates}`, or None when there's nothing fresh.
    pub fn page_value(&self, now_s: f64) -> Option<Json> {
        self.fresh_rates(now_s)
            .map(|r| json!({ "base": "EUR", "rates": r.rates }))
    }

    /// The cache as [`Rates`] for an engine to hold.
    pub fn rates(&self) -> Option<Rates> {
        self.rates.as_ref().map(|r| Rates {
            base: "EUR".into(),
            date: Some(r.date.clone()),
            rates: r.rates.clone(),
            fetched: Some(self.fetched),
        })
    }
}

/// The HTTP GET behind a refresh: the body of `url` as text, at most `limit` bytes, or None on any failure.
pub trait Fetch {
    fn get(&self, url: &str, limit: usize) -> Option<String>;
}

/// Asks the ECB when a day has passed since the last try, and writes the cache back. Returns the cache and
/// whether its page value changed. `now_s` is the clock when the fetch began, `done_s` when it ended.
pub fn refresh(path: &Path, now_s: f64, fetch: &dyn Fetch, done_s: impl FnOnce() -> f64) -> (Cache, bool) {
    let mut cache = Cache::load(path);
    if !cache.due(now_s) {
        return (cache, false);
    }
    let before = cache.page_value(now_s);
    cache.checked = now_s;
    if let Some(daily) = fetch
        .get(URL, MAX_BYTES)
        .filter(|text| text.len() <= MAX_BYTES)
        .and_then(|text| parse_xml(&text))
    {
        cache.rates = Some(daily);
        cache.fetched = done_s();
    }
    // a failed write only means asking again next launch
    let _ = cache.save(path);
    let changed = cache.page_value(cache.fetched.max(now_s)) != before;
    (cache, changed)
}

/// Rates an engine holds: the cache's, or a `{base, rates}` table handed in (which is always fresh).
#[derive(Clone, Debug, PartialEq)]
pub struct Rates {
    pub base: String,
    pub date: Option<String>,
    pub rates: BTreeMap<String, f64>,
    /// When they were fetched, in seconds since 1970. None for a `{base, rates}` table.
    pub fetched: Option<f64>,
}

/// Reads the cache file's JSON or a `{base, rates}` table (`window.__QCALC_RATES`). `Ok(None)` is well-formed
/// JSON with no usable rates (a failed first fetch); `Err` is not a JSON object.
pub fn parse_json(text: &str) -> Result<Option<Rates>, String> {
    let v: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Some(o) = v.as_object() else {
        return Err("rates JSON must be an object".into());
    };
    let base = o.get("base").and_then(Json::as_str);
    let rates: BTreeMap<String, f64> = o
        .get("rates")
        .and_then(Json::as_object)
        .map(|all| {
            all.iter()
                .filter(|(code, _)| is_code(code) && *code != base.unwrap_or("EUR"))
                .filter_map(|(code, n)| finite_positive(n).map(|n| (code.clone(), n)))
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
    Ok(Cache::from_json(text).rates())
}

impl Rates {
    pub fn fresh(&self, now_ms: i64) -> bool {
        let Some(fetched) = self.fetched else {
            return true;
        };
        let now = now_ms as f64 / 1000.0;
        now >= fetched && now - fetched < STALE_S
    }

    /// What the phrase layer takes, or None when stale.
    pub fn currency_rates(&self, now_ms: i64) -> Option<CurrencyRates> {
        self.fresh(now_ms)
            .then(|| CurrencyRates::new(&self.base, &self.rates))
    }

    /// The same as `CurrencyRates` JSON (`{base, rates}`), or null when stale.
    pub fn table(&self, now_ms: i64) -> Json {
        if self.fresh(now_ms) {
            json!({ "base": self.base, "rates": self.rates })
        } else {
            Json::Null
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const DAILY: &str = include_str!("fixtures/eurofxref-daily.xml");

    fn with_cubes(cubes: &str) -> String {
        DAILY.split_once("<Cube time").map(|(head, _)| head).unwrap().to_string()
            + "<Cube time='2026-09-24'>"
            + cubes
            + "</Cube></Cube></gesmes:Envelope>"
    }

    #[test]
    fn reads_the_daily_file() {
        let r = parse_xml(DAILY).unwrap();
        assert_eq!(r.date, "2026-09-24");
        assert_eq!(r.rates.len(), 29);
        assert_eq!(r.rates["USD"], 1.1367);
        assert_eq!(r.rates["IDR"], 20384.10);
        assert!(!r.rates.contains_key("EUR"));
    }

    #[test]
    fn skips_bad_codes_and_numbers() {
        let xml = with_cubes(
            "<Cube currency='USD' rate='1.1'/><Cube currency='usd' rate='1.2'/><Cube currency='GBPX' rate='0.8'/>\
             <Cube currency='JPY' rate='-3'/><Cube currency='CHF' rate='0'/><Cube currency='SEK' rate='1e3'/>\
             <Cube currency='NOK' rate='inf'/><Cube currency='DKK' rate='NaN'/><Cube currency='PLN' rate=' 4.3'/>\
             <Cube currency='HUF' rate='3.'/><Cube currency='CZK'/><Cube rate='2.0'/><Cube currency='EUR' rate='1'/>",
        );
        assert_eq!(parse_xml(&xml).unwrap().rates, BTreeMap::from([("USD".to_string(), 1.1)]));
        assert_eq!(rate(&"9".repeat(400)), None);
    }

    #[test]
    fn refuses_anything_off_shape() {
        assert_eq!(parse_xml(""), None);
        assert_eq!(parse_xml("<html>busy</html>"), None);
        assert_eq!(parse_xml(&DAILY.replace("</gesmes:Envelope>", "")), None);
        assert_eq!(parse_xml(&DAILY.replace("eurofxref\"", "eurofxrefs\"")), None);
        assert_eq!(parse_xml(&DAILY.replace("time='2026-09-24'", "time='yesterday'")), None);
        assert_eq!(
            parse_xml(&with_cubes("<Cube currency='USD' rate='1.1'/><Cube currency='USD' rate='1.2'/>")),
            None
        );
        let two_days = DAILY.replacen(
            "<Cube time='2026-09-24'>",
            "<Cube time='2026-09-23'><Cube currency='USD' rate='1.2'/></Cube><Cube time='2026-09-24'>",
            1,
        );
        assert_eq!(parse_xml(&two_days), None);
    }

    #[test]
    fn cache_round_trips_and_drops_junk() {
        let cache = Cache {
            checked: 100.0,
            fetched: 90.0,
            rates: parse_xml(DAILY),
        };
        assert_eq!(Cache::from_json(&cache.to_json()), cache);
        let junk = Cache::from_json(
            r#"{ "checked": 5.5, "date": "2026-09-24", "rates": { "USD": 1.1, "usd": 2, "XAU": -1, "EUR": 1, "GBP": "0.8" } }"#,
        );
        assert_eq!(junk.checked, 5.5);
        assert_eq!(junk.rates.unwrap().rates, BTreeMap::from([("USD".to_string(), 1.1)]));
        assert_eq!(Cache::from_json("{}"), Cache::default());
    }

    #[test]
    fn asks_at_most_once_a_day_and_goes_stale() {
        let cache = Cache {
            checked: 10.0 * DAY_S,
            ..Cache::default()
        };
        assert!(!cache.due(11.0 * DAY_S - 1.0));
        assert!(cache.due(11.0 * DAY_S));
        assert!(cache.due(9.0 * DAY_S));
        assert!(Cache::default().due(0.0));
        let now = 1000.0 * DAY_S;
        let cache = Cache {
            checked: now,
            fetched: now - DAY_S,
            rates: parse_xml(DAILY),
        };
        assert_eq!(cache.page_value(now).unwrap()["rates"]["GBP"], 0.85986);
        assert_eq!(
            Cache {
                fetched: now - 8.0 * DAY_S,
                ..cache
            }
            .page_value(now),
            None
        );
    }

    struct Fake(Option<&'static str>, Cell<usize>);
    impl Fetch for Fake {
        fn get(&self, url: &str, _limit: usize) -> Option<String> {
            assert_eq!(url, URL);
            self.1.set(self.1.get() + 1);
            self.0.map(str::to_string)
        }
    }

    #[test]
    fn refreshes_through_the_fetcher() {
        let dir = std::env::temp_dir().join(format!("qcalc-phrases-rates-{}", std::process::id()));
        let path = dir.join("rates.json");
        let _ = fs::remove_file(&path);
        let now = 1_790_000_000.0;
        let ok = Fake(Some(DAILY), Cell::new(0));
        let (cache, changed) = refresh(&path, now, &ok, || now + 1.0);
        assert!(changed && cache.rates.is_some() && cache.fetched == now + 1.0);
        // not again the same day, and the file holds what was fetched
        let (again, changed) = refresh(&path, now + 60.0, &ok, || now);
        assert!(!changed && ok.1.get() == 1 && again.rates == cache.rates);
        // a failed fetch a day later keeps the old rates and records the attempt
        let down = Fake(Some("<html>busy</html>"), Cell::new(0));
        let (kept, _) = refresh(&path, now + DAY_S, &down, || now);
        assert_eq!((kept.checked, kept.rates.is_some()), (now + DAY_S, true));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_rates_from_either_shape() {
        let text = r#"{"checked":1790000000.5,"date":"2026-09-24","fetched":1790000000.25,
            "rates":{"USD":1.1367,"usd":2,"XAU":-1,"EUR":1,"GBP":"0.8","JPY":180.57}}"#;
        let r = parse_json(text).unwrap().unwrap();
        assert_eq!(r.rates.len(), 2);
        let at = 1_790_000_000_250;
        assert!(r.fresh(at + 86_400_000));
        assert!(!r.fresh(at + 8 * 86_400_000));
        assert!(!r.fresh(at - 1000));
        assert_eq!(r.table(at)["base"], "EUR");
        assert_eq!(r.currency_rates(at).unwrap().rates.len(), 2);
        let page = parse_json(r#"{"base":"EUR","rates":{"USD":1.25}}"#).unwrap().unwrap();
        assert!(page.fresh(0));
        assert_eq!(parse_json(r#"{"checked":5}"#).unwrap(), None);
        assert!(parse_json("[1]").is_err());
        assert!(parse_json("nope").is_err());
    }
}
