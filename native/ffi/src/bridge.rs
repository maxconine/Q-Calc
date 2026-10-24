//! The JSON escape hatch behind the typed calls whose engine code other packages are still porting.
//!
//! Each call names the TS function it stands for (`lib/completion` `completionFor`) and goes through
//! `qcalc_core::golden::dispatch`, the same table the parity runner uses. Until a package registers its
//! function there, the call gives its blank value. Once the package registers it, the call answers, and the Swift
//! signature doesn't change. Arguments use the recorder's encoding (`{"$date": ISO}` for a `Date`,
//! `{"$fn": tag}` for a callback the dispatch rebuilds).

use qcalc_core::golden::{Outcome, dispatch};
use serde_json::{Value as Json, json};

use crate::records::CoreError;

/// The TS function's answer as plain JSON, or None while nothing answers it (unported, or a callback the
/// dispatch can't rebuild yet).
pub fn call(module: &str, function: &str, args: &[Json]) -> Option<Json> {
    match dispatch(module, function, args) {
        Outcome::Answer(a) => Some(a),
        Outcome::Threw(_) | Outcome::Unported | Outcome::Unreplayable => None,
    }
}

/// `call` for `call_json`, with the reason when nothing answers.
pub fn call_checked(module: &str, function: &str, args: &[Json]) -> Result<Json, CoreError> {
    match dispatch(module, function, args) {
        Outcome::Answer(a) => Ok(a),
        Outcome::Threw(e) => Err(CoreError::BadRequest {
            message: format!("{module} {function} threw: {e}"),
        }),
        Outcome::Unported => Err(CoreError::Unported {
            function: format!("{module} {function}"),
        }),
        Outcome::Unreplayable => Err(CoreError::BadRequest {
            message: format!("{module} {function}: a callback argument the engine can't rebuild"),
        }),
    }
}

/// A JS `Date` argument as the recorder writes it: `{"$date": "2026-09-24T22:12:40.000Z"}`.
pub fn date(now_ms: i64) -> Json {
    json!({ "$date": iso_date(now_ms) })
}

/// `new Date(ms).toISOString()`.
pub fn iso_date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The `ok` callback `blankReason` takes, as the corpus tags it: a line is ok when the engine answers it.
/// `chained` says the bar evaluates `chainedExpr(text)` (an extra field the TS tag doesn't carry).
pub fn eval_non_blank(options: &Json, chained: bool) -> Json {
    let mut tag = json!({ "kind": "evalNonBlank", "options": options });
    if chained {
        tag["chained"] = Json::Bool(true);
    }
    json!({ "$fn": tag })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates_like_js() {
        assert_eq!(iso_date(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_date(1_790_287_960_000), "2026-09-24T22:12:40.000Z");
        assert_eq!(iso_date(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(iso_date(951_782_400_123), "2000-02-29T00:00:00.123Z");
    }

    #[test]
    fn unported_calls_are_blank() {
        assert_eq!(call("lib/nowhere", "nothing", &[]), None);
        assert!(matches!(call_checked("lib/nowhere", "nothing", &[]), Err(CoreError::Unported { .. })));
        assert_eq!(call("engine/format", "formatNumber", &[json!(0.5), json!(6)]), Some(json!("0.5")));
    }
}
