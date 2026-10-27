//! What the app logic calls in other packages' modules, collected in one place. Each keeps the TS name and
//! signature. Ports that have landed are `pub use`d; the rest go through the golden dispatch table
//! (`crate::golden::dispatch`, under their TS module and name, as the FFI bridge does), so they answer as soon as
//! their package registers them there and give their blank answer until then. The few that are one-line TS
//! predicates or constants are written out in full.
//!
//! Owners (`native/PORTING_PLAN.md`): units → WP-UNITS, chem → WP-CHEM, isolate/system →
//! WP-SOLVE, graph → WP-GRAPH, phrases/nativeEval → WP-PHRASES, typstMath → WP-NOTATION.

use serde_json::{Value as Json, json};

use crate::golden::{Outcome, dispatch};
use crate::jsre::{self, Lazy, js};
use crate::types::{AngleMode, DefaultUnits};

/// The answer of a TS function some package registered in the golden dispatch, or None while none has.
fn call(module: &str, name: &str, args: &[Json]) -> Option<Json> {
    match dispatch(module, name, args) {
        Outcome::Answer(a) => Some(a),
        _ => None,
    }
}

fn undef() -> Json {
    json!({ "$undef": true })
}

// ---- WP-UNITS (units.ts) ----

pub use crate::units::{
    in_ladder_unit, is_unit_name, sanitize_default_units_json as sanitize_default_units,
};

// ---- WP-CHEM (chem.ts) ----

/// The units `chem R …` takes, in completion's order (a constant of chem.ts).
pub const GAS_CONSTANT_UNITS: [&str; 6] = ["atm", "bar", "torr", "mmhg", "cal", "si"];

/// `isMolarMassInput` (chem.ts).
pub fn is_molar_mass_input(text: &str) -> bool {
    call("engine/chem", "isMolarMassInput", &[json!(text)]).and_then(|v| v.as_bool()) == Some(true)
}

/// `isReactionInput` (chem.ts).
pub fn is_reaction_input(text: &str) -> bool {
    call("engine/chem", "isReactionInput", &[json!(text)]).and_then(|v| v.as_bool()) == Some(true)
}

// ---- WP-SOLVE (isolate.ts, system.ts) ----

/// `isolatePrevious`: `isolate x` or `solve for x` alone, rewritten to work on the previous row's equation.
/// (isolate.ts)
pub fn isolate_previous(text: &str, previous: Option<&str>) -> Option<String> {
    let prev = previous.map_or_else(undef, |p| json!(p));
    call("engine/isolate", "isolatePrevious", &[json!(text), prev])?
        .as_str()
        .map(str::to_string)
}

/// What `sys`, `sys 3` or `system 7` asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SysCommand {
    /// Open this many equation fields.
    Count(usize),
    /// Say this instead (no count, or too many).
    Hint(String),
}

/// `sysCommand`, written out in full (seven lines of system.ts).
pub fn sys_command(text: &str) -> Option<SysCommand> {
    static RE: Lazy = Lazy::new(|| js(r"^sys(?:tem)?\s*(\d+)?\s*$", "i"));
    let c = RE.captures(text.trim()).ok()??;
    let Some(m) = c.get(1) else {
        return Some(SysCommand::Hint(
            "how many equations? sys2, sys3… up to sys5".into(),
        ));
    };
    let n = crate::app::js::number(m.as_str()).unwrap_or(f64::NAN);
    if !(1.0..=5.0).contains(&n) || n.fract() != 0.0 {
        return Some(SysCommand::Hint("up to 5 equations".into()));
    }
    Some(SysCommand::Count(n as usize))
}

/// `isSysCommand`, written out in full.
pub fn is_sys_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*sys(?:tem)?(?:\s|\d|$)", "i"));
    jsre::test(&RE, text) && sys_command(text).is_some()
}

/// `SystemAnswer`: what the `sys` fields solve to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemAnswer {
    pub display: String,
    pub exact: Option<String>,
    /// A message ("no solution"), not an answer.
    pub message: bool,
}

/// `solveLive` (system.ts).
pub fn solve_live(
    lines: &[String],
    defaults: &DefaultUnits,
    angle_mode: AngleMode,
) -> Option<SystemAnswer> {
    let a = call(
        "engine/system",
        "solveLive",
        &[json!(lines), json!(defaults), json!(angle_mode)],
    )?;
    Some(SystemAnswer {
        display: a.get("display")?.as_str()?.to_string(),
        exact: a.get("exact").and_then(Json::as_str).map(str::to_string),
        message: a.get("message") == Some(&Json::Bool(true)),
    })
}

// ---- WP-GRAPH (graph.ts) ----

/// `isGraphCommand`, written out in full.
pub fn is_graph_command(input: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*graph(?:\s|$)", "i"));
    jsre::test(&RE, input)
}

/// A function a `graph f(x) = …` line defines.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: String,
}

/// The parts of `GraphIntent` the bar reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphIntent {
    pub label: String,
    pub function_def: Option<FunctionDef>,
}

/// `parseGraphIntent` (graph.ts), the parts the bar reads.
pub fn parse_graph_intent(
    input: &str,
    functions: &std::collections::BTreeMap<String, crate::types::UserFunction>,
) -> Option<GraphIntent> {
    let i = call(
        "engine/graph",
        "parseGraphIntent",
        &[json!(input), json!({ "functions": functions })],
    )?;
    Some(GraphIntent {
        label: i
            .get("label")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        function_def: i
            .get("functionDef")
            .and_then(|f| serde_json::from_value(f.clone()).ok()),
    })
}

// ---- WP-PHRASES (phrases.ts, lib/nativeEval.ts) ----

/// `looksLikeNaturalLanguage`, written out in full: money, clock times, dates and phrase words, never calculus.
pub fn looks_like_natural_language(expr: &str) -> bool {
    static ONLY: crate::jsre::Lazy<Vec<jsre::Regex>> = crate::jsre::Lazy::new(|| {
        [
            (r"[$€£¥₹]", ""),
            (
                r"\b(?:usd|eur|gbp|jpy|cad|aud|chf|cny|inr|mxn|dollars?|euros?|yen)\b",
                "i",
            ),
            (r"(?:\d|\b)(?:am|pm)\b", "i"),
            (r"\d{1,2}:\d{2}", ""),
            (
                r"\b(?:today|tomorrow|yesterday|ago|until|since|between|from now|time in)\b",
                "i",
            ),
            (r"\b(?:mon|tues|wednes|thurs|fri|satur|sun)day\b", "i"),
            (
                r"\b(?:january|february|march|april|june|july|august|september|october|november|december)\b",
                "i",
            ),
            (r"%\s*(?:off|on)\b|\bas an? %|\bwhat %", "i"),
            (r"\b(?:tip|lunch|dinner|people|person|nights|percent)\b", "i"),
        ]
        .iter()
        .map(|(p, f)| js(p, f))
        .collect()
    });
    let t = crate::app::js::trim(expr);
    if t.is_empty() || crate::stubs::is_calculus_input(t) {
        return false;
    }
    ONLY.iter().any(|re| jsre::test(re, t))
}

/// A phrase's answer (`answerPhrase`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhraseAnswer {
    pub display: String,
    pub n: Option<f64>,
}

/// `answerPhrase(expr, { now, rates, sigFigs })` (phrases.ts); `rates` is the TS `CurrencyRates` table or null.
pub fn answer_phrase(
    expr: &str,
    now_ms: f64,
    sig_figs: f64,
    rates: &Json,
    time_zone: Option<&str>,
) -> Option<PhraseAnswer> {
    let mut options = json!({
        "now": { "$date": crate::app::js::iso_date(now_ms as i64) },
        "rates": rates,
        "sigFigs": sig_figs,
    });
    // an unknown zone name would make the phrase layer answer nothing; keep the default then
    if let Some(zone) = time_zone.filter(|z| crate::phrases::time_zone(z).is_some()) {
        options["timeZone"] = json!(zone);
    }
    let a = call("engine/phrases", "answerPhrase", &[json!(expr), options])?;
    Some(PhraseAnswer {
        display: a.get("display")?.as_str()?.to_string(),
        n: a.get("n").and_then(Json::as_f64),
    })
}

// ---- WP-NOTATION (lib/typstMath.ts) ----

/// `typstPreviewUseful` (typstMath.ts): the typeset preview shows something the bar doesn't.
pub fn typst_preview_useful(expr: &str) -> bool {
    call("lib/typstMath", "typstPreviewUseful", &[json!(expr)]).and_then(|v| v.as_bool())
        == Some(true)
}

/// `typstAnswer`, written out in full: `exact ≈ approx`, or whichever there is.
pub fn typst_answer(exact: Option<&str>, display: &str) -> String {
    let pretty = exact.map_or("", crate::app::js::trim);
    let shown = crate::app::js::trim(display);
    if !pretty.is_empty() && !shown.is_empty() && pretty != shown {
        return format!("{pretty} ≈ {shown}");
    }
    if pretty.is_empty() {
        shown.to_string()
    } else {
        pretty.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sys_commands() {
        assert_eq!(sys_command(" sys3 "), Some(SysCommand::Count(3)));
        assert!(matches!(sys_command("system"), Some(SysCommand::Hint(_))));
        assert!(matches!(sys_command("sys 9"), Some(SysCommand::Hint(_))));
        assert!(is_sys_command("sys 2"));
        assert!(!is_sys_command("system of"));
        assert!(is_graph_command("graph x"));
        assert!(!is_graph_command("graphs"));
    }

    #[test]
    fn natural_language() {
        assert!(looks_like_natural_language("$5 + $3"));
        assert!(looks_like_natural_language("3pm + 2 hours"));
        assert!(!looks_like_natural_language("2 + 2"));
    }
}
