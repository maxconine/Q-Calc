// the ecb's daily reference rates, the source soulvercore uses on the mac: euro is the base, each rate is units per euro
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use roxmltree::{Document, Node};
use serde_json::{json, Value};

pub const URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const GESMES: &str = "http://www.gesmes.org/xml/2002-08-01";
const EUROFXREF: &str = "http://www.ecb.int/vocabulary/2002-08-01/eurofxref";
pub const DAY: u64 = 24 * 60 * 60;
// older than this and a conversion would quote a rate nobody trades at any more
const STALE: u64 = 7 * DAY;

#[derive(Clone, Debug, PartialEq)]
pub struct Rates {
    pub date: String,
    pub rates: BTreeMap<String, f64>,
}

fn is_code(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
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

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn cubes<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(|n| n.is_element() && n.tag_name().name() == "Cube" && n.tag_name().namespace() == Some(EUROFXREF))
}

fn only<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

// envelope, one outer cube, one dated cube, then currency cubes; anything else and nothing is trusted
pub fn parse(xml: &str) -> Option<Rates> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "Envelope" || root.tag_name().namespace() != Some(GESMES) {
        return None;
    }
    let day = only(cubes(only(cubes(root))?))?;
    let date = day.attribute("time").filter(|d| is_date(d))?.to_string();
    let mut rates = BTreeMap::new();
    for cube in cubes(day) {
        let (Some(code), Some(value)) = (cube.attribute("currency"), cube.attribute("rate").and_then(rate)) else { continue };
        if !is_code(code) || code == "EUR" {
            continue;
        }
        // the same currency twice can't be told apart, so the file isn't trusted
        if rates.insert(code.to_string(), value).is_some() {
            return None;
        }
    }
    (!rates.is_empty()).then_some(Rates { date, rates })
}

pub fn fetch() -> Option<Rates> {
    use ureq::config::AutoHeaderValue;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    let agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(AutoHeaderValue::None)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .new_agent();
    let mut response = agent.get(URL).call().ok()?;
    let text = response.body_mut().with_config().limit(1 << 20).read_to_string().ok()?;
    parse(&text)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cache {
    // last attempt, kept even when it failed, so the ecb is asked at most once a day
    pub checked: u64,
    pub fetched: u64,
    pub rates: Option<Rates>,
}

impl Cache {
    pub fn from_json(text: &str) -> Cache {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let rates = v["rates"].as_object().map(|all| {
            all.iter()
                .filter(|(code, _)| is_code(code) && *code != "EUR")
                .filter_map(|(code, n)| n.as_f64().filter(|n| n.is_finite() && *n > 0.0).map(|n| (code.clone(), n)))
                .collect::<BTreeMap<_, _>>()
        });
        let date = v["date"].as_str().filter(|d| is_date(d)).map(str::to_string);
        Cache {
            checked: v["checked"].as_u64().unwrap_or(0),
            fetched: v["fetched"].as_u64().unwrap_or(0),
            rates: match (date, rates) {
                (Some(date), Some(rates)) if !rates.is_empty() => Some(Rates { date, rates }),
                _ => None,
            },
        }
    }

    pub fn to_json(&self) -> String {
        let (date, rates) = self.rates.as_ref().map_or((None, None), |r| (Some(&r.date), Some(&r.rates)));
        json!({ "checked": self.checked, "fetched": self.fetched, "date": date, "rates": rates }).to_string()
    }

    pub fn load(path: &Path) -> Cache {
        fs::read_to_string(path).map(|text| Cache::from_json(&text)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, self.to_json()).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }

    // a clock that went backwards counts as due, or it could wait for years
    pub fn due(&self, now: u64) -> bool {
        self.checked == 0 || now < self.checked || now - self.checked >= DAY
    }

    // what the page gets as __QCALC_RATES; None leaves it unset
    pub fn page_value(&self, now: u64) -> Option<Value> {
        let rates = self.rates.as_ref()?;
        let fresh = now >= self.fetched && now - self.fetched < STALE;
        fresh.then(|| json!({ "base": "EUR", "rates": rates.rates }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAILY: &str = include_str!("fixtures/eurofxref-daily.xml");

    fn with_cubes(cubes: &str) -> String {
        DAILY.split_once("<Cube time").map(|(head, _)| head).unwrap().to_string()
            + "<Cube time='2026-09-24'>"
            + cubes
            + "</Cube></Cube></gesmes:Envelope>"
    }

    #[test]
    fn reads_the_daily_file() {
        let r = parse(DAILY).unwrap();
        assert_eq!(r.date, "2026-09-24");
        assert_eq!(r.rates.len(), 29);
        assert_eq!(r.rates["USD"], 1.1367);
        assert_eq!(r.rates["JPY"], 180.57);
        assert_eq!(r.rates["IDR"], 20384.10);
        assert_eq!(r.rates["ZAR"], 18.6836);
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
        assert_eq!(parse(&xml).unwrap().rates, BTreeMap::from([("USD".to_string(), 1.1)]));
        assert_eq!(rate(&"9".repeat(400)), None);
    }

    #[test]
    fn refuses_anything_off_shape() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("<html>busy</html>"), None);
        assert_eq!(parse(&DAILY.replace("</gesmes:Envelope>", "")), None);
        assert_eq!(parse(&DAILY.replace("eurofxref\"", "eurofxrefs\"")), None);
        assert_eq!(parse(&DAILY.replace("time='2026-09-24'", "time='yesterday'")), None);
        assert_eq!(parse(&with_cubes("<Cube currency='usd' rate='1.1'/>")), None);
        assert_eq!(parse(&with_cubes("<Cube currency='USD' rate='1.1'/><Cube currency='USD' rate='1.2'/>")), None);
        let two_days = DAILY.replacen("<Cube time='2026-09-24'>", "<Cube time='2026-09-23'><Cube currency='USD' rate='1.2'/></Cube><Cube time='2026-09-24'>", 1);
        assert_eq!(parse(&two_days), None);
    }

    #[test]
    fn cache_round_trips_and_drops_junk() {
        let cache = Cache { checked: 100, fetched: 90, rates: parse(DAILY) };
        assert_eq!(Cache::from_json(&cache.to_json()), cache);
        let junk = Cache::from_json(r#"{ "checked": 5, "date": "2026-09-24", "rates": { "USD": 1.1, "usd": 2, "XAU": -1, "EUR": 1, "GBP": "0.8" } }"#);
        assert_eq!(junk.rates.unwrap().rates, BTreeMap::from([("USD".to_string(), 1.1)]));
        assert_eq!(Cache::from_json("{}"), Cache::default());
        assert_eq!(Cache::from_json(r#"{ "date": "2026-09-24", "rates": {} }"#).rates, None);
    }

    #[test]
    fn asks_at_most_once_a_day() {
        let cache = Cache { checked: 10 * DAY, ..Cache::default() };
        assert!(!cache.due(10 * DAY + DAY - 1));
        assert!(cache.due(11 * DAY));
        assert!(cache.due(9 * DAY));
        assert!(Cache::default().due(0));
    }

    #[test]
    fn the_page_gets_fresh_rates_or_nothing() {
        let now = 1_000 * DAY;
        let cache = Cache { checked: now, fetched: now - DAY, rates: parse(DAILY) };
        let value = cache.page_value(now).unwrap();
        assert_eq!(value["base"], "EUR");
        assert_eq!(value["rates"]["GBP"], 0.85986);
        assert_eq!(Cache { fetched: now - 8 * DAY, ..cache.clone() }.page_value(now), None);
        assert_eq!(Cache { rates: None, ..cache }.page_value(now), None);
    }
}
