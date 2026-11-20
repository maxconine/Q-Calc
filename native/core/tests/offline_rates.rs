//! QA item 67: a money line while no exchange rates are on hand (a Mac offline since install, or for over a week)
//! says the rates haven't loaded; it said “usd” isn’t a unit and underlined it.

use qcalc_core::app::live::{LiveRequest, NO_RATES_HINT, commit, live, waits_for_rates};
use qcalc_core::app::settings::default_settings;
use serde_json::{Value as Json, json};

const NOW: f64 = 1_795_694_400_000.0;

fn request(text: &str, rates: Json) -> LiveRequest {
    LiveRequest {
        text: text.into(),
        options: default_settings().eval_options(),
        phrases: true,
        now_ms: NOW,
        rates,
        ..LiveRequest::default()
    }
}

#[test]
fn money_lines_without_rates_say_so() {
    for t in [
        "20 usd in eur",
        "$20 to eur",
        "usd to eur",
        "€5 + $3",
        "100 jpy in usd",
        "20 dollars in euros",
    ] {
        let req = request(t, Json::Null);
        assert_eq!(live(&req).display, "", "{t}");
        assert!(waits_for_rates(&req, t), "{t}");
        assert_eq!(
            commit(&req, &[], "r1").hint.as_deref(),
            Some(NO_RATES_HINT),
            "{t}"
        );
        assert_eq!(live(&req).squiggle, None, "{t}");
    }
    // one currency alone needs no rate; other blank lines keep their own hints
    assert_eq!(live(&request("20 eur", Json::Null)).display, "€20.00");
    let other = request("5 m to kg", Json::Null);
    assert!(!waits_for_rates(&other, "5 m to kg"));
    assert_ne!(
        commit(&other, &[], "r1").hint.as_deref(),
        Some(NO_RATES_HINT)
    );
}

#[test]
fn rates_on_hand_convert_as_before() {
    let rates = json!({"base": "EUR", "rates": {"USD": 1.25}, "date": "2026-11-19"});
    let req = request("20 usd in eur", rates.clone());
    assert_eq!(live(&req).display, "€16.00");
    assert!(!waits_for_rates(&req, "20 usd in eur"));
    assert_eq!(
        live(&request("usd to eur", rates)).display,
        "1 USD = 0.8 EUR (Nov 19)"
    );
    // phrases off: not a money line at all
    let mut off = request("20 usd in eur", Json::Null);
    off.phrases = false;
    assert!(!waits_for_rates(&off, "20 usd in eur"));
}
