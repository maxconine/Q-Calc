//! `src/engine/currency.ts`: money parsing, conversion and display for the phrase layer. Rates are always handed
//! in, never fetched here (see [`super::rates`]).

use std::collections::BTreeMap;

use serde_json::Value as Json;

use super::{js_re, js_trim, proto_value};
use crate::jsnum;
use crate::jsre::{self, Lazy};

/// One unit of `base` buys `rates[code]` of each currency (`CurrencyRates`). Values stay JSON, because the TS
/// checks each one's type when it reads it (`typeof r === 'number'`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurrencyRates {
    pub base: String,
    pub rates: BTreeMap<String, Json>,
}

impl CurrencyRates {
    /// A `{base, rates}` table as JSON (`isRatesTable` decides whether one is usable).
    pub fn from_json(v: &Json) -> Option<Self> {
        if !is_rates_table(v) {
            return None;
        }
        Some(CurrencyRates {
            base: v["base"].as_str()?.to_string(),
            rates: v["rates"]
                .as_object()
                .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        })
    }

    /// Plain numbers per code, as [`super::rates::Rates`] holds them.
    pub fn new(base: &str, rates: &BTreeMap<String, f64>) -> Self {
        CurrencyRates {
            base: base.to_string(),
            rates: rates
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect(),
        }
    }
}

/// A currency as the TS holds it: an ISO code, or (a kept quirk) the `String()` of an `Object.prototype` member
/// that a plain-object lookup such as `WORDS['constructor']` returned. See [`super::proto_value`].
pub type Currency = &'static str;

/// `Money`: an amount, with a currency when one was typed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Money {
    pub n: f64,
    pub currency: Option<Currency>,
}

// only major codes: a wider list starts colliding with unit names
const CODES: [&str; 25] = [
    "USD", "EUR", "GBP", "JPY", "CAD", "AUD", "CHF", "CNY", "INR", "MXN", "NZD", "SEK", "NOK", "DKK", "HKD",
    "SGD", "KRW", "BRL", "ZAR", "PLN", "CZK", "HUF", "TRY", "ILS", "THB",
];

pub(super) fn symbol(s: &str) -> Option<Currency> {
    Some(match s {
        "$" => "USD",
        "€" => "EUR",
        "£" => "GBP",
        "¥" => "JPY",
        "₹" => "INR",
        _ => return None,
    })
}

fn shown_symbol(code: &str) -> Option<&'static str> {
    Some(match code {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        "INR" => "₹",
        _ => return proto_value(code),
    })
}

fn no_decimals(code: &str) -> bool {
    matches!(code, "JPY" | "KRW" | "HUF")
}

fn word(s: &str) -> Option<Currency> {
    Some(match s {
        "dollar" | "dollars" => "USD",
        "euro" | "euros" => "EUR",
        "yen" => "JPY",
        "yuan" | "renminbi" => "CNY",
        "rupee" | "rupees" => "INR",
        "sterling" => "GBP",
        _ => return proto_value(s),
    })
}

// pounds are also a weight, so they only count as money next to another currency
fn pounds_word(s: &str) -> Option<Currency> {
    match s {
        "pound" | "pounds" => Some("GBP"),
        _ => proto_value(s),
    }
}

/// `NUM`: `1,000`, `12`, `3.5`, `.5`.
pub const NUM: &str = r"(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?|\.\d+";

static WHOLE_NUM: Lazy = Lazy::new(|| js_re(&format!("^(?:{NUM})$"), ""));

pub fn parse_number(s: &str) -> Option<f64> {
    let t = js_trim(s);
    if !jsre::test(&WHOLE_NUM, t) {
        return None;
    }
    let n = jsnum::parse(&t.replace(',', "")).unwrap_or(f64::NAN);
    n.is_finite().then_some(n)
}

// JS `/^[a-z]{3}$/i`: without the `u` flag only ASCII letters fold, so this is exact (Rust's `(?i)` would also
// take the Kelvin sign and the long s)
static THREE_LETTERS: Lazy = Lazy::new(|| js_re(r"^[a-zA-Z]{3}$", ""));
static US_DOLLARS: Lazy = Lazy::new(|| js_re(r"^us\s+(?=dollars?$)", ""));

pub fn currency_word(s: &str, pounds: bool) -> Option<Currency> {
    let t = js_trim(s);
    if let Some(c) = symbol(t).or_else(|| proto_value(t)) {
        return Some(c);
    }
    let up = t.to_uppercase();
    if jsre::test(&THREE_LETTERS, t)
        && let Some(code) = CODES.iter().find(|c| **c == up)
    {
        return Some(code);
    }
    let lower = t.to_lowercase();
    let low = super::replace_first(&US_DOLLARS, &lower, |_| String::new());
    word(&low).or_else(|| if pounds { pounds_word(&low) } else { None })
}

static PRE: Lazy = Lazy::new(|| js_re(&format!(r"^([$€£¥₹])\s?({NUM})$"), ""));
static POST: Lazy = Lazy::new(|| js_re(&format!(r"^({NUM})\s?([$€£¥₹])$"), ""));
// JS `[a-z]` with the `i` flag, written out
static WORDED: Lazy = Lazy::new(|| js_re(&format!(r"^({NUM})\s+([a-zA-Z]+(?:\s[a-zA-Z]+)?)$"), ""));

/// `80`, `$80`, `€20`, `20€`, `20 usd`, `20 dollars`.
pub fn parse_money(s: &str, pounds: bool) -> Option<Money> {
    let t = js_trim(s);
    if let Ok(Some(m)) = PRE.captures(t) {
        let n = parse_number(jsre::group(&m, 2))?;
        return Some(Money {
            n,
            currency: symbol(jsre::group(&m, 1)),
        });
    }
    if let Ok(Some(m)) = POST.captures(t) {
        let n = parse_number(jsre::group(&m, 1))?;
        return Some(Money {
            n,
            currency: symbol(jsre::group(&m, 2)),
        });
    }
    if let Ok(Some(m)) = WORDED.captures(t) {
        let n = parse_number(jsre::group(&m, 1));
        let currency = currency_word(jsre::group(&m, 2), pounds);
        return match (n, currency) {
            (Some(n), Some(c)) => Some(Money {
                n,
                currency: Some(c),
            }),
            _ => None,
        };
    }
    parse_number(t).map(|n| Money { n, currency: None })
}

// `table.rates[key]`: an own key wins; a missing key that names an `Object.prototype` member finds a function,
// which isn't nullish (so `??` stops there) and isn't a number. `None` is nullish.
fn slot(table: &CurrencyRates, key: &str) -> Option<Option<f64>> {
    match table.rates.get(key) {
        Some(Json::Null) => None,
        Some(v) => Some(v.as_f64()),
        None => proto_value(key).map(|_| None),
    }
}

fn rate_of(code: &str, table: &CurrencyRates) -> Option<f64> {
    if code == table.base.to_uppercase() {
        return Some(1.0);
    }
    let r = slot(table, code).or_else(|| slot(table, &code.to_lowercase()))??;
    (r.is_finite() && r > 0.0).then_some(r)
}

pub fn convert_currency(
    amount: f64,
    from: &str,
    to: &str,
    table: Option<&CurrencyRates>,
) -> Option<f64> {
    let table = table?;
    if !amount.is_finite() {
        return None;
    }
    let a = rate_of(&from.to_uppercase(), table)?;
    let b = rate_of(&to.to_uppercase(), table)?;
    let out = (amount / a) * b;
    out.is_finite().then_some(out)
}

/// `isRatesTable`: an object with a string `base` and an object (or array) `rates`.
pub fn is_rates_table(v: &Json) -> bool {
    let Some(o) = v.as_object() else {
        return false;
    };
    o.get("base").is_some_and(Json::is_string)
        && o.get("rates").is_some_and(|r| r.is_object() || r.is_array())
}

pub fn format_money(n: f64, currency: &str) -> String {
    let dp = if no_decimals(currency) { 0 } else { 2 };
    let fixed = jsnum::to_fixed(n.abs(), dp);
    let sign = if n < 0.0 && jsnum::parse(&fixed).unwrap_or(f64::NAN) != 0.0 {
        "-"
    } else {
        ""
    };
    match shown_symbol(currency) {
        Some(sym) => format!("{sign}{sym}{fixed}"),
        None => format!("{sign}{fixed} {currency}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rates() -> CurrencyRates {
        CurrencyRates::from_json(&json!({"base": "EUR", "rates": {"USD": 1.25, "GBP": 0.8, "JPY": 160, "CAD": 1.5}}))
            .unwrap()
    }

    #[test]
    fn parses_money() {
        assert_eq!(
            parse_money("$1,000.50", false),
            Some(Money {
                n: 1000.5,
                currency: Some("USD")
            })
        );
        assert_eq!(parse_money("20€", false).unwrap().currency, Some("EUR"));
        assert_eq!(parse_money("20 us dollars", false).unwrap().currency, Some("USD"));
        assert_eq!(parse_money("5 pounds", false), None);
        assert_eq!(parse_money("5 pounds", true).unwrap().currency, Some("GBP"));
        assert_eq!(parse_money("5 kg", false), None);
        assert_eq!(parse_money(".5", false).unwrap().n, 0.5);
        assert_eq!(parse_money("1,00", false), None);
        // the long s and the Kelvin sign aren't letters to a JS regex without `u`
        assert_eq!(currency_word("uſd", false), None);
    }

    #[test]
    fn converts() {
        let r = rates();
        assert_eq!(convert_currency(100.0, "USD", "GBP", Some(&r)), Some(64.0));
        assert_eq!(convert_currency(1.0, "usd", "jpy", Some(&r)), Some(128.0));
        assert_eq!(convert_currency(1.0, "USD", "XYZ", Some(&r)), None);
        assert_eq!(convert_currency(1.0, "USD", "GBP", None), None);
        // a lower-case key is read too, and `??` only falls through on a nullish rate
        let lower = CurrencyRates::from_json(&json!({"base": "EUR", "rates": {"usd": 2, "GBP": null, "gbp": 4}}))
            .unwrap();
        assert_eq!(convert_currency(2.0, "USD", "GBP", Some(&lower)), Some(4.0));
    }

    #[test]
    fn formats_money() {
        assert_eq!(format_money(-0.001, "USD"), "$0.00");
        assert_eq!(format_money(-5.0, "USD"), "-$5.00");
        assert_eq!(format_money(1234.5, "JPY"), "¥1235");
        assert_eq!(format_money(3.0, "CHF"), "3.00 CHF");
        // kept quirk: SHOWN_SYMBOL is a plain object
        assert_eq!(format_money(3.0, "toString"), "function toString() { [native code] }3.00");
    }
}
