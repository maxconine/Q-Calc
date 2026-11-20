//! Native-only phrase answers (QA item 6): the other dollars, `₩`, a code written first, and a clock time read in
//! another zone.

use qcalc_core::phrases::currency::CurrencyRates;
use qcalc_core::phrases::{PhraseOptions, answer_phrase, native_money, time_zone};

fn say(expr: &str) -> String {
    let tz = time_zone("America/Los_Angeles").unwrap();
    let rates = CurrencyRates::new(
        "EUR",
        &[
            ("USD".to_string(), 1.25),
            ("CAD".to_string(), 1.5),
            ("AUD".to_string(), 2.0),
            ("KRW".to_string(), 1250.0),
            ("CHF".to_string(), 1.0),
        ]
        .into(),
    );
    // 2026-11-20 12:00 UTC: 4:00 am in Los Angeles, 9:00 pm in Tokyo
    let options = PhraseOptions {
        now_ms: 1_795_694_400_000.0,
        time_zone: &tz,
        rates: Some(&rates),
        sig_figs: None,
    };
    answer_phrase(expr, &options)
        .map(|a| a.display)
        .unwrap_or_default()
}

#[test]
fn regional_dollars_are_not_us_dollars() {
    // was $20.00: the web skipped the `C` as a word and read US dollars
    assert_eq!(say("C$20 in usd"), "$16.67");
    assert_eq!(say("A$20 in usd"), "$12.50");
    assert_eq!(say("C$20"), "20.00 CAD");
    assert_eq!(say("C$1.5k in usd"), "$1250.00");
    assert_eq!(say("₩5000 in usd"), "$5.00");
    assert_eq!(say("CHF 20 in usd"), "$25.00");
    assert_eq!(say("US$20 in eur"), "€16.00");
    // a word ending in c before `$` is left as the web reads it
    assert_eq!(native_money::regional("abc$5"), None);
    // a code in lower case before a number is not rewritten (`try 5`)
    assert_eq!(native_money::regional("try 5"), None);
}

#[test]
fn clock_in_another_zone() {
    assert_eq!(say("3pm est in pst"), "12:00 pm");
    assert_eq!(say("3:30 pm est to pst"), "12:30 pm");
    assert_eq!(say("3pm pst in tokyo"), "8:00 am next day");
    assert_eq!(say("3pm in tokyo"), "8:00 am next day");
    assert_eq!(say("9am london time in la"), "1:00 am");
    assert_eq!(say("3pm tokyo time"), "10:00 pm previous day");
    assert_eq!(say("noon london in sydney"), "11:00 pm");
    // the web's own answers stay
    assert_eq!(say("3pm"), "3:00 pm");
    assert_eq!(say("time in tokyo"), "9:00 pm");
    assert_eq!(say("3 in tokyo"), "");
}
