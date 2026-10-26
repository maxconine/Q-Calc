//! Replays golden fixtures (TS engine calls recorded by `native/golden-capture`) against the Rust port.
//!
//! A fixture is `{fn, args, out | throws, nondet?, now?, src}` with the TS module implied by its area file
//! (`engine.format.jsonl` → `engine/format`; every `eval.*` area → `engine/evaluate`). Values use the recorder's
//! encoding: `{"$f": "NaN" | "Infinity" | "-Infinity" | "-0"}`, `{"$undef": true}`, `{"$fn": tag}`.
//!
//! Each package registers its TS functions in [`dispatch`] (one `match` arm group per module, append-only), so
//! porting agents see their pass rate move as they go.

use serde_json::{Value as Json, json};

use crate::types::{EvaluateOptions, LineResult};

/// What replaying one fixture gave.
#[derive(Debug)]
pub enum Outcome {
    /// The Rust answer, as JSON in the TS shape.
    Answer(Json),
    /// The Rust port threw where the TS may have too.
    Threw(String),
    /// Nothing ported answers this function yet.
    Unported,
    /// The fixture can't be replayed (a callback argument the port can't rebuild).
    Unreplayable,
}

/// The TS module an area's fixtures come from.
pub fn module_of(area: &str) -> String {
    if area.starts_with("eval.") {
        return "engine/evaluate".into();
    }
    area.replacen('.', "/", 1)
}

/// Recorder encoding to plain JSON: specials become `null` (and `-0` becomes `-0.0`).
pub fn decode(v: &Json) -> Json {
    match v {
        Json::Object(o) if o.len() == 1 && o.contains_key("$f") => match o["$f"].as_str() {
            Some("-0") => json!(-0.0),
            _ => Json::Null,
        },
        Json::Object(o) if o.len() == 1 && o.contains_key("$undef") => Json::Null,
        Json::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.clone(), decode(v))).collect()),
        Json::Array(a) => Json::Array(a.iter().map(decode).collect()),
        other => other.clone(),
    }
}

/// A number argument, with the recorder's specials.
pub fn f64_arg(args: &[Json], i: usize) -> Option<f64> {
    match args.get(i)? {
        Json::Number(n) => n.as_f64(),
        Json::Object(o) => match o.get("$f").and_then(Json::as_str)? {
            "NaN" => Some(f64::NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "-0" => Some(-0.0),
            _ => None,
        },
        _ => None,
    }
}

pub fn str_arg(args: &[Json], i: usize) -> Option<String> {
    args.get(i)?.as_str().map(str::to_string)
}

fn has_callback(v: &Json) -> bool {
    match v {
        Json::Object(o) => o.contains_key("$fn") || o.values().any(has_callback),
        Json::Array(a) => a.iter().any(has_callback),
        _ => false,
    }
}

fn options_arg(args: &[Json], i: usize) -> EvaluateOptions {
    args.get(i)
        .map(decode)
        .and_then(|o| serde_json::from_value(o).ok())
        .unwrap_or_default()
}

fn opt_str(s: Option<String>) -> Json {
    s.map_or(Json::Null, Json::String)
}

fn line_json(r: &LineResult) -> Json {
    serde_json::to_value(r).unwrap_or(Json::Null)
}

/// Replays one call of TS function `name` from `module` with recorder-encoded `args`.
pub fn dispatch(module: &str, name: &str, args: &[Json]) -> Outcome {
    // ---- WP-APP-LOGIC: src/lib modules, with blankReason's evalNonBlank callback rebuilt ----
    if let Some(out) = crate::app::golden::dispatch(module, name, args) {
        return out;
    }
    if args.iter().any(has_callback) {
        return Outcome::Unreplayable;
    }
    let s = |i| str_arg(args, i).unwrap_or_default();
    let n = |i| f64_arg(args, i);
    let answer = match (module, name) {
        // ---- foundation: engine/evaluate ----
        ("engine/evaluate", "evaluateLine") => {
            line_json(&crate::evaluate_line(&s(0), &options_arg(args, 1)))
        }
        ("engine/evaluate", "evaluateSheet") => {
            let lines: Vec<String> = args
                .first()
                .and_then(Json::as_array)
                .map(|a| {
                    a.iter()
                        .map(|l| {
                            l.as_str()
                                .or_else(|| l["text"].as_str())
                                .unwrap_or("")
                                .to_string()
                        })
                        .collect()
                })
                .unwrap_or_default();
            Json::Array(
                crate::evaluate_sheet(&lines, &options_arg(args, 1))
                    .iter()
                    .map(line_json)
                    .collect(),
            )
        }
        ("engine/evaluate", "parseAssignment") => crate::evaluate::parse_assignment(&s(0)).map_or(
            Json::Null,
            |(variable, expr)| json!({ "variable": variable, "expr": expr }),
        ),
        ("engine/evaluate", "stripTrailingEquals") => {
            json!(crate::evaluate::strip_trailing_equals(&s(0)))
        }
        ("engine/evaluate", "parseFunctionDef") => crate::evaluate::parse_function_def(&s(0))
            .map_or(
                Json::Null,
                |(name, params, body)| json!({ "name": name, "params": params, "body": body }),
            ),
        // ---- foundation: engine/format ----
        ("engine/format", "formatNumber") => json!(crate::format::format_number(
            n(0).unwrap_or(f64::NAN),
            n(1).unwrap_or(12.0)
        )),
        ("engine/format", "formatScientific") => {
            json!(crate::format::format_scientific(
                n(0).unwrap_or(f64::NAN),
                n(1).unwrap_or(12.0).max(0.0) as u32
            ))
        }
        ("engine/format", "clampSigFigs") => {
            json!(crate::format::clamp_sig_figs(n(0).unwrap_or(f64::NAN)))
        }
        ("engine/format", "prettyAnswer") => json!(crate::format::pretty_answer(&s(0))),
        ("engine/format", "formatValue") => {
            let Some(v) = args
                .first()
                .map(decode)
                .and_then(|v| serde_json::from_value(v).ok())
            else {
                return Outcome::Threw("bad value".into());
            };
            json!(crate::format::format_value(&v, n(1).unwrap_or(12.0)))
        }
        ("engine/format", "num") => {
            serde_json::to_value(crate::types::Value::number(n(0).unwrap_or(f64::NAN)))
                .unwrap_or(Json::Null)
        }
        ("engine/format", "textVal") => {
            serde_json::to_value(crate::types::Value::text(s(0))).unwrap_or(Json::Null)
        }
        // ---- foundation: engine/scientific (the text passes and formatting; functions are WP-FUNCTIONS) ----
        ("engine/scientific", "formatAsFraction") => opt_str(crate::format::format_as_fraction(
            n(0).unwrap_or(f64::NAN),
            n(1).unwrap_or(10_000.0) as u32,
        )),
        ("engine/scientific", "formatComplex") => {
            json!(crate::scientific::format_complex(
                n(0).unwrap_or(f64::NAN),
                n(1).unwrap_or(f64::NAN)
            ))
        }
        ("engine/scientific", "rewriteTypesetMul") => {
            json!(crate::scientific::preprocess::rewrite_typeset_mul(&s(0)))
        }
        ("engine/scientific", "stitchConstants") => {
            json!(crate::scientific::preprocess::stitch_constants(&s(0)))
        }
        ("engine/scientific", "preprocessAscii") => {
            let extra: Vec<String> =
                args.get(1)
                    .and_then(Json::as_array)
                    .map_or_else(Vec::new, |a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    });
            json!(crate::scientific::preprocess_ascii(&s(0), &extra))
        }
        ("engine/scientific", "wrapBareFunctions") => {
            let extra: Vec<String> =
                args.get(1)
                    .and_then(Json::as_array)
                    .map_or_else(Vec::new, |a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    });
            json!(crate::scientific::preprocess::wrap_bare_functions(
                &s(0),
                &extra
            ))
        }
        // ---- foundation: engine/parens ----
        ("engine/parens", "inferParens") => {
            let p = crate::parens::infer_parens(&s(0));
            json!({ "leading": p.leading, "trailing": p.trailing, "filled": p.filled })
        }
        ("engine/parens", "fillParens") => json!(crate::parens::fill_parens(&s(0))),
        ("engine/parens", "autofillParens") => {
            let caret = n(1).unwrap_or(0.0) as usize;
            let end = n(2).map_or(caret, |e| e as usize);
            opt_str(crate::parens::autofill_parens(&s(0), caret, end))
        }
        // ---- foundation: engine/simplify ----
        ("engine/simplify", "wantsExactForm") => json!(crate::simplify::wants_exact_form(&s(0))),
        ("engine/simplify", "exactForm") => {
            let o = args.get(1).map(decode).unwrap_or(Json::Null);
            let opts = crate::simplify::ExactFormOptions {
                rationalize: o.get("rationalize").and_then(Json::as_bool),
                wide: o.get("wide").and_then(Json::as_bool).unwrap_or(false),
            };
            opt_str(crate::simplify::exact_form(n(0).unwrap_or(f64::NAN), opts))
        }
        ("engine/simplify", "dualLabel") => json!(crate::simplify::dual_label(
            str_arg(args, 0).as_deref(),
            &s(1)
        )),
        ("engine/simplify", "splitSquares") => {
            let (coeff, rad) = crate::simplify::split_squares(n(0).unwrap_or(f64::NAN));
            json!({ "coeff": coeff, "rad": rad })
        }
        // ---- foundation: engine/plainMath (LaTeX itself is WP-NOTATION) ----
        ("engine/plainMath", "looksLikeLatex") => json!(crate::latex::looks_like_latex(&s(0))),
        ("engine/typstInput", "looksLikeTypst") => {
            json!(crate::typst_input::looks_like_typst(&s(0)))
        }
        // ---- WP-SYMBOLIC: mathjs simplify / derivative / rationalize / toString / parse ----
        (
            "symbolic/simplify"
            | "symbolic/derivative"
            | "symbolic/rationalize"
            | "symbolic/toString"
            | "symbolic/parse",
            _,
        ) => return symbolic_dispatch(name, args),
        // ---- WP-FUNCTIONS: engine/scientific, engine/angles, engine/dots ----
        ("engine/scientific", "evalScientific") => crate::scientific::functions::golden_eval(args),
        ("engine/angles", "angleExponent") => json!(crate::angles::angle_exponent(&s(0))),
        ("engine/dots", "joinDots") => json!(crate::dots::join_dots(&s(0))),
        // ---- WP-UNITS: engine/units, lib/unitSpans, lib/draft (lib/missingUnit takes a test's closure) ----
        ("engine/units" | "lib/unitSpans" | "lib/draft", _) => {
            match crate::units::golden::dispatch(module, name, args) {
                Some(answer) => answer,
                None => return Outcome::Unported,
            }
        }
        // ---- WP-PRECISE: engine/precise ----
        ("engine/precise", _) => match crate::precise::golden_dispatch(name, args) {
            Some(answer) => answer,
            None => return Outcome::Unported,
        },
        // ---- WP-PHRASES: engine/phrases, phraseTime, currency, lib/nativeEval ----
        ("engine/phrases" | "engine/phraseTime" | "engine/currency" | "lib/nativeEval", _) => {
            match crate::phrases::golden::dispatch(module, name, args) {
                Some(Ok(answer)) => answer,
                Some(Err(e)) => return Outcome::Threw(e),
                None => return Outcome::Unported,
            }
        }
        // ---- WP-MATRIX: engine/matrix ----
        ("engine/matrix", _) => match crate::matrix::golden_dispatch(name, args) {
            Some(answer) => answer,
            None => return Outcome::Unported,
        },
        // ---- WP-MEASURE: engine/measure ----
        ("engine/measure", _) => match crate::measure::golden_dispatch(name, args) {
            Some(answer) => answer,
            None => return Outcome::Unported,
        },
        // ---- WP-SOLVE: engine/solve, engine/isolate, engine/system ----
        ("engine/solve" | "engine/isolate" | "engine/system", _) => {
            match crate::solve::golden_dispatch_module(module, name, args) {
                Some(answer) => answer,
                None => return Outcome::Unported,
            }
        }
        // ---- WP-CALCULUS: engine/calculus, engine/calcNumeric, engine/closedForm ----
        ("engine/calculus" | "engine/calcNumeric" | "engine/closedForm", _) => {
            match crate::calculus::golden::dispatch(module, name, args) {
                Some(answer) => answer,
                None => return Outcome::Unported,
            }
        }
        _ => return Outcome::Unported,
    };
    Outcome::Answer(answer)
}

/// WP-SYMBOLIC: replays a recorded mathjs symbolic call (trees in `symbolic::json`'s format).
fn symbolic_dispatch(name: &str, args: &[Json]) -> Outcome {
    use crate::symbolic::{self, PrintOptions, json as tree};
    let node_arg = |i: usize| -> Result<symbolic::Node, Outcome> {
        match args.get(i) {
            Some(Json::String(text)) => symbolic::parse(text).map_err(|e| Outcome::Threw(e.0)),
            Some(j) => tree::from_tree(j).map_err(|_| Outcome::Unreplayable),
            None => Err(Outcome::Unreplayable),
        }
    };
    let node_out = |r: Result<symbolic::Node, symbolic::SymError>| match r {
        Ok(n) => Outcome::Answer(json!({
            "text": symbolic::to_string(&n, &PrintOptions::default()),
            "tree": tree::to_tree(&n),
        })),
        Err(e) => Outcome::Threw(e.0),
    };
    let n = match node_arg(0) {
        Ok(n) => n,
        Err(o) => return o,
    };
    match name {
        "parse" => node_out(Ok(n)),
        "simplify" if args.len() == 1 => node_out(symbolic::simplify(&n)),
        "rationalize" if args.len() == 1 => node_out(symbolic::rationalize(&n)),
        "derivative" => {
            let var = args.get(1).and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.get("S").and_then(Json::as_str).map(str::to_string))
            });
            let Some(var) = var else {
                return Outcome::Unreplayable;
            };
            let simplify = args
                .get(2)
                .and_then(|o| o.get("simplify"))
                .and_then(Json::as_bool)
                .unwrap_or(true);
            node_out(symbolic::derivative_text(&n, &var, simplify))
        }
        "toString" => {
            let o = args.get(1);
            let field = |k: &str| o.and_then(|o| o.get(k)).and_then(Json::as_str);
            let opts = PrintOptions {
                parenthesis: field("parenthesis").map(|p| match p {
                    "auto" => symbolic::Parenthesis::Auto,
                    "all" => symbolic::Parenthesis::All,
                    _ => symbolic::Parenthesis::Keep,
                }),
                implicit: field("implicit").map(|p| match p {
                    "show" => symbolic::Implicit::Show,
                    _ => symbolic::Implicit::Hide,
                }),
            };
            Outcome::Answer(json!(symbolic::to_string(&n, &opts)))
        }
        _ => Outcome::Unreplayable,
    }
}

/// Whether two doubles are the same answer: equal, or within 4 units in the last place (the TS and Rust
/// libms may round a transcendental differently by an ulp; displays are compared exactly regardless).
pub fn same_number(a: f64, b: f64) -> bool {
    a == b && a.is_sign_negative() == b.is_sign_negative()
        || (a.is_finite()
            && b.is_finite()
            && a != 0.0
            && (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(b.abs()))
}

/// Compares a Rust answer with the recorded TS one; on a mismatch, the JSON path of the first difference.
pub fn compare(expected: &Json, actual: &Json, path: &str) -> Result<(), String> {
    let mismatch = || Err(format!("{path}: want {expected}, got {actual}"));
    match (expected, actual) {
        (Json::Object(e), _) if e.len() == 1 && e.contains_key("$f") => {
            let want = e["$f"].as_str().unwrap_or("");
            match (want, actual) {
                // serde_json writes non-finite doubles as null
                ("NaN" | "Infinity" | "-Infinity", Json::Null) => Ok(()),
                // JS writes counts like `-minDepth` as -0; nothing downstream tells it from 0
                ("-0", Json::Number(n)) if n.as_f64() == Some(0.0) => Ok(()),
                _ => mismatch(),
            }
        }
        (Json::Object(e), _) if e.len() == 1 && e.contains_key("$undef") => {
            if actual.is_null() {
                Ok(())
            } else {
                mismatch()
            }
        }
        (Json::Number(e), Json::Number(a)) => {
            let (Some(e), Some(a)) = (e.as_f64(), a.as_f64()) else {
                return mismatch();
            };
            if same_number(e, a) {
                Ok(())
            } else {
                mismatch()
            }
        }
        (Json::Array(e), Json::Array(a)) => {
            if e.len() != a.len() {
                return Err(format!("{path}: want {} items, got {}", e.len(), a.len()));
            }
            e.iter()
                .zip(a)
                .enumerate()
                .try_for_each(|(i, (x, y))| compare(x, y, &format!("{path}[{i}]")))
        }
        (Json::Object(e), Json::Object(a)) => {
            for (k, ev) in e {
                match a.get(k) {
                    Some(av) => compare(ev, av, &format!("{path}.{k}"))?,
                    None => return Err(format!("{path}.{k}: want {ev}, got nothing")),
                }
            }
            match a.keys().find(|k| !e.contains_key(*k) && !a[*k].is_null()) {
                Some(extra) => Err(format!("{path}.{extra}: want nothing, got {}", a[extra])),
                None => Ok(()),
            }
        }
        (e, a) if e == a => Ok(()),
        _ => mismatch(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_with_recorder_specials() {
        assert!(compare(&json!({"$f": "NaN"}), &Json::Null, "").is_ok());
        assert!(compare(&json!({"n": 0.3}), &json!({"n": 0.30000000000000004}), "").is_ok());
        assert!(compare(&json!({"n": 0.3}), &json!({"n": 0.31}), "").is_err());
        assert!(compare(&json!({"a": 1}), &json!({"a": 1, "b": 2}), "").is_err());
        assert!(compare(&json!({"a": 1}), &json!({"a": 1, "b": null}), "").is_ok());
    }

    #[test]
    fn dispatches_foundation_functions() {
        let Outcome::Answer(a) = dispatch("engine/format", "formatNumber", &[json!(0.1), json!(6)])
        else {
            panic!()
        };
        assert_eq!(a, json!("0.1"));
        assert!(matches!(
            dispatch("engine/notAModule", "notAFunction", &[json!("5 m")]),
            Outcome::Unported
        ));
        assert!(matches!(
            dispatch(
                "lib/blankReason",
                "blankReason",
                &[json!("x"), json!({"$fn": "anon"})]
            ),
            Outcome::Unreplayable
        ));
    }
}
