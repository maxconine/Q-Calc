//! Golden replay for the phrase modules. A phrase call's `now` is the `options.now` Date it was recorded with
//! (`{"$date": iso}`), or the test files' `NOW` when it had none (those calls don't depend on the clock). The
//! zone is the recording machine's, `America/Los_Angeles`, unless the options name one in `timeZone` (an
//! addition the FFI uses; the TS has no such field).

use serde_json::{Value as Json, json};

use super::currency::{self, CurrencyRates};
use super::{PhraseOptions, native_eval, time, time_zone, try_answer_phrase};
use crate::golden::{decode, f64_arg, str_arg};

/// The time zone the fixtures were recorded in.
pub const RECORDED_ZONE: &str = "America/Los_Angeles";
/// `new Date(2026, 8, 24, 15, 12, 40)` there: the tests' `NOW`.
pub const RECORDED_NOW_MS: f64 = 1_790_287_960_000.0;

/// A recorder `{"$date": iso}` as a time value.
pub fn date_arg(v: &Json) -> Option<f64> {
    let iso = v.get("$date")?.as_str()?;
    iso.parse::<jiff::Timestamp>()
        .ok()
        .map(|t| t.as_millisecond() as f64)
}

fn money_json(m: Option<currency::Money>) -> Json {
    m.map_or(Json::Null, |m| {
        let mut o = json!({ "n": m.n });
        if let Some(c) = m.currency {
            o["currency"] = json!(c);
        }
        o
    })
}

pub fn dispatch(module: &str, name: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let s = |i| str_arg(args, i).unwrap_or_default();
    let n = |i| f64_arg(args, i).unwrap_or(f64::NAN);
    let arg = |i: usize| args.get(i).map(decode).unwrap_or(Json::Null);
    let opt_num = |v: Option<f64>| v.map_or(Json::Null, |n| json!(n));
    Some(Ok(match (module, name) {
        ("engine/phrases", "answerPhrase") => {
            let o = args.get(1).cloned().unwrap_or(Json::Null);
            let now = o.get("now").and_then(date_arg).unwrap_or(RECORDED_NOW_MS);
            let zone = o
                .get("timeZone")
                .and_then(Json::as_str)
                .unwrap_or(RECORDED_ZONE);
            let tz = time_zone(zone)?;
            let rates = o.get("rates").and_then(CurrencyRates::from_json);
            let options = PhraseOptions {
                now_ms: now,
                time_zone: &tz,
                rates: rates.as_ref(),
                sig_figs: o.get("sigFigs").and_then(Json::as_f64),
            };
            match try_answer_phrase(&s(0), &options) {
                Ok(a) => a.map_or(Json::Null, |a| a.to_json()),
                Err(e) => return Some(Err(e.0)),
            }
        }
        ("engine/phraseTime", "formatClock") => json!(time::format_clock(n(0))),
        ("engine/phraseTime", "formatDuration") => json!(time::format_duration(n(0))),
        ("engine/phraseTime", "parseClock") => opt_num(time::parse_clock(&s(0))),
        ("engine/phraseTime", "parseSpan") => time::parse_span(&s(0)).map_or(
            Json::Null,
            |sp| json!({ "minutes": sp.minutes, "days": sp.days, "months": sp.months }),
        ),
        ("engine/currency", "convertCurrency") => {
            let table = CurrencyRates::from_json(&arg(3));
            opt_num(currency::convert_currency(
                n(0),
                &s(1),
                &s(2),
                table.as_ref(),
            ))
        }
        ("engine/currency", "formatMoney") => json!(currency::format_money(n(0), &s(1))),
        ("engine/currency", "isRatesTable") => json!(currency::is_rates_table(&arg(0))),
        ("engine/currency", "parseMoney") => money_json(currency::parse_money(
            &s(0),
            args.get(1).and_then(Json::as_bool).unwrap_or(false),
        )),
        ("engine/currency", "parseNumber") => opt_num(currency::parse_number(&s(0))),
        ("engine/currency", "currencyWord") => {
            currency::currency_word(&s(0), args.get(1).and_then(Json::as_bool).unwrap_or(false))
                .map_or(Json::Null, |c| json!(c))
        }
        ("lib/nativeEval", "looksLikeNaturalLanguage") => {
            json!(native_eval::looks_like_natural_language(&s(0)))
        }
        ("lib/nativeEval", "looksLikeDictionaryQuery") => {
            json!(native_eval::looks_like_dictionary_query(&s(0)))
        }
        ("lib/nativeEval", "usableNativeDisplay") => {
            json!(native_eval::usable_native_display(&s(0)))
        }
        ("lib/nativeEval", "soulverAngleSafe") => {
            json!(native_eval::soulver_angle_safe(&s(0), &s(1)))
        }
        ("lib/nativeEval", "nativeEvalPayload") => native_eval::native_eval_payload(&arg(0)),
        ("lib/nativeEval", "mergeLiveAnswer") => {
            native_eval::merge_live_answer(&s(0), &s(1), &arg(2), &arg(3))
        }
        ("lib/nativeEval", "nativeDefinition") => native_eval::native_definition(&arg(0), &s(1)),
        ("lib/nativeEval", "nativeReplyToLive") => {
            native_eval::native_reply_to_live(&arg(0), &arg(1), &s(2))
        }
        _ => return None,
    }))
}
