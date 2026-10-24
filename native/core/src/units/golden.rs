//! Golden-fixture replay for WP-UNITS: `engine/units`, `lib/unitSpans` and `lib/draft` calls in their TS shapes.
//! `crate::golden::dispatch` hands these modules here.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::golden::{decode, f64_arg, str_arg};
use crate::types::{AngleMode, DefaultUnits, Value};

fn value_arg(args: &[Json], i: usize) -> Option<Value> {
    serde_json::from_value(decode(args.get(i)?)).ok()
}

fn defaults_arg(args: &[Json], i: usize) -> DefaultUnits {
    match args.get(i).map(decode) {
        Some(Json::Object(o)) => o
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect(),
        _ => DefaultUnits::new(),
    }
}

fn vars_arg(args: &[Json], i: usize) -> BTreeMap<String, f64> {
    match args.get(i).map(decode) {
        Some(Json::Object(o)) => o
            .into_iter()
            .filter_map(|(k, v)| v.as_f64().map(|n| (k, n)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn angle_arg(args: &[Json], i: usize) -> Option<AngleMode> {
    match args.get(i).and_then(Json::as_str) {
        Some("deg") => Some(AngleMode::Deg),
        Some("rad") => Some(AngleMode::Rad),
        _ => None,
    }
}

fn names_arg(args: &[Json], i: usize) -> Vec<String> {
    args.get(i)
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn js(v: Option<Value>) -> Json {
    v.map_or(Json::Null, |v| serde_json::to_value(v).unwrap_or(Json::Null))
}

/// The answer to one recorded call, or None when the function isn't one of this package's.
pub fn dispatch(module: &str, name: &str, args: &[Json]) -> Option<Json> {
    let s = |i| str_arg(args, i).unwrap_or_default();
    let n = |i| f64_arg(args, i).unwrap_or(f64::NAN);
    let truthy = |i: usize| args.get(i).and_then(Json::as_bool).unwrap_or(false);
    Some(match (module, name) {
        ("engine/units", "isImproperUnitConversion") => {
            json!(super::is_improper_unit_conversion(&s(0)))
        }
        ("engine/units", "tryConvert") => js(super::try_convert_opt(
            &s(0),
            &defaults_arg(args, 1),
            &vars_arg(args, 2),
            truthy(3),
            angle_arg(args, 4),
        )),
        ("engine/units", "readsAsUnit") => json!(super::reads_as_unit(&s(0))),
        ("engine/units", "isUnitName") => json!(super::is_unit_name(&s(0))),
        ("engine/units", "unitInfo") => super::unit_info(&s(0)).map_or(Json::Null, |i| {
            json!({ "symbol": i.symbol, "name": i.name })
        }),
        ("engine/units", "quantityText") => match value_arg(args, 0) {
            Some(v) => json!(super::quantity_text_opt(&v, angle_arg(args, 1))),
            None => Json::Null,
        },
        ("engine/units", "unitAlternatives") => {
            let limit = f64_arg(args, 1).map_or(3, |l| l.max(0.0) as usize);
            match value_arg(args, 0) {
                Some(v) => json!(
                    super::unit_alternatives(&v, limit)
                        .into_iter()
                        .map(|v| js(Some(v)))
                        .collect::<Vec<_>>()
                ),
                None => json!([]),
            }
        }
        ("engine/units", "stepPrefix") => {
            js(value_arg(args, 0).and_then(|v| super::step_prefix(&v, if n(1) > 0.0 { 1 } else { -1 })))
        }
        ("engine/units", "inLadderUnit") => {
            js(value_arg(args, 0).and_then(|v| super::in_ladder_unit(&v, &s(1))))
        }
        ("engine/units", "siQuantity") => super::si_quantity(&s(0))
            .map_or(Json::Null, |q| serde_json::to_value(q).unwrap_or(Json::Null)),
        ("engine/units", "siValue") => {
            let dim: Vec<f64> = args
                .get(1)
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(Json::as_f64).collect())
                .unwrap_or_default();
            js(super::si_value(
                n(0),
                &dim,
                str_arg(args, 2).as_deref(),
                &defaults_arg(args, 3),
            ))
        }
        ("engine/units", "sanitizeDefaultUnits") => json!(super::sanitize_default_units_json(
            &args.first().map(decode).unwrap_or(Json::Null)
        )),
        ("engine/units", "defaultUnitsEqual") => json!(super::default_units_equal(
            &defaults_arg(args, 0),
            &defaults_arg(args, 1)
        )),
        ("lib/unitSpans", "unitSpans") => json!(
            crate::app::unit_spans::unit_spans(&s(0), &names_arg(args, 1), &names_arg(args, 2))
        ),
        ("lib/draft", "clampDraftSeconds") => json!(crate::app::draft::clamp_draft_seconds(n(0))),
        ("lib/draft", "hideAction") => {
            json!(crate::app::draft::hide_action(&s(0), &s(1), n(2)).as_str())
        }
        ("lib/draft", "shouldRestoreDraft") => {
            json!(crate::app::draft::should_restore_draft(n(0), n(1), n(2)))
        }
        _ => return None,
    })
}
