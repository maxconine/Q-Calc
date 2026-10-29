//! The JSON escape hatch behind the typed calls whose engine code other packages are still porting.
//!
//! Each call names the TS function it stands for (`lib/completion` `completionFor`) and goes through
//! `qcalc_core::golden::dispatch`, the same table the parity runner uses. Until a package registers its
//! function there, the call gives its blank value. Once the package registers it, the call answers, and the Swift
//! signature doesn't change. Arguments use the recorder's encoding (`{"$date": ISO}` for a `Date`, built by
//! `qcalc_core::app::js::iso_date`; `{"$fn": tag}` for a callback the dispatch rebuilds).

use qcalc_core::golden::{Outcome, dispatch};
use serde_json::Value as Json;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unported_calls_are_blank() {
        assert_eq!(call("lib/nowhere", "nothing", &[]), None);
        assert!(matches!(
            call_checked("lib/nowhere", "nothing", &[]),
            Err(CoreError::Unported { .. })
        ));
        assert_eq!(
            call("engine/format", "formatNumber", &[json!(0.5), json!(6)]),
            Some(json!("0.5"))
        );
    }
}
