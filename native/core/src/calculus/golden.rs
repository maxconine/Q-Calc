//! Golden replay for `engine/calculus`, `engine/calcNumeric` and `engine/closedForm` (WP-CALCULUS).

use serde_json::{Value as Json, json};

use super::closed_form::{self, Job};
use super::{CalcContext, CalculusResult};
use crate::golden::{decode, f64_arg, str_arg};
use crate::types::EvaluateOptions;

fn ctx_arg(args: &[Json], i: usize) -> CalcContext {
    let o: EvaluateOptions =
        args.get(i).map(decode).and_then(|o| serde_json::from_value(o).ok()).unwrap_or_default();
    CalcContext {
        sci: crate::scientific::ScientificContext {
            ans: o.ans,
            angle_mode: o.angle(),
            variables: o.variables,
            functions: o.functions,
            depth: 0,
        },
        time_varying: o.time_varying,
    }
}

/// `CalculusResult` as the TS writes it.
pub fn result_json(r: &CalculusResult) -> Json {
    let mut m = serde_json::Map::new();
    m.insert("value".into(), r.value.as_ref().map_or(Json::Null, |v| serde_json::to_value(v).unwrap_or(Json::Null)));
    if let Some(t) = &r.time {
        m.insert("time".into(), serde_json::to_value(t).unwrap_or(Json::Null));
    }
    if let Some(e) = &r.exact {
        m.insert("exact".into(), json!(e));
    }
    if let Some(j) = &r.job {
        m.insert("job".into(), j.clone());
    }
    Json::Object(m)
}

/// Replays one call, or None when the function isn't one of this package's.
pub fn dispatch(module: &str, name: &str, args: &[Json]) -> Option<Json> {
    let s = |i| str_arg(args, i).unwrap_or_default();
    let job = || args.first().and_then(Job::from_json);
    Some(match (module, name) {
        ("engine/calculus", "evaluateCalculus") => {
            super::evaluate_calculus(&s(0), &ctx_arg(args, 1)).map_or(Json::Null, |r| result_json(&r))
        }
        ("engine/calculus", "isCalculusInput") => json!(super::is_calculus_input(&s(0))),
        ("engine/calculus", "jobKey") => job().map_or(Json::Null, |j| json!(j.key())),
        ("engine/calculus", "knownClosedForm") => match closed_form::known_closed_form(&s(0)) {
            None => Json::Null,
            Some(e) => e.map_or(Json::Null, Json::String),
        },
        ("engine/calculus", "rememberClosedForm") => {
            if let Some(j) = job() {
                closed_form::remember_closed_form(&j, str_arg(args, 1));
            }
            // a vitest file's module state is its own; later fixtures must not see this one's answer
            closed_form::forget_closed_forms();
            Json::Null
        }
        ("engine/calcNumeric", "justified") => {
            json!(super::numeric::justified(
                f64_arg(args, 0).unwrap_or(f64::NAN),
                f64_arg(args, 1).unwrap_or(f64::NAN),
                f64_arg(args, 2)
            ))
        }
        ("engine/closedForm", "identifyJob") => {
            let ticks = f64_arg(args, 1).map_or(closed_form::DEFAULT_TICKS, |ms| {
                (ms.max(0.0) as u64).saturating_mul(closed_form::DEFAULT_TICKS) / closed_form::DEFAULT_MS
            });
            job().and_then(|j| closed_form::identify_job(&j, ticks)).map_or(Json::Null, Json::String)
        }
        _ => return None,
    })
}
