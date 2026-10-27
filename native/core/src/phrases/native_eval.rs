//! The still-relevant parts of `src/lib/nativeEval.ts`: which lines read as natural language, which answer wins
//! between the engine and a native (formerly SoulverCore) one (`mergeLiveAnswer`), and the reply handling. The
//! message passing to a native host (`evaluateNative`, `hasNativeEval`) is UI and isn't ported.
//!
//! Replies and live answers stay JSON, because the TS checks each field's type (`typeof o.n === 'number'`).

use serde_json::{Map, Value as Json, json};

use super::{ascii_fold_safe, js_re, js_trim};
use crate::jsre::{self, Lazy};
use crate::stubs::{is_calculus_input, is_molar_mass_input};
use crate::units::{IMPROPER_UNIT_CONVERSION, is_improper_unit_conversion};

fn test_i(re: &jsre::Regex, s: &str) -> bool {
    jsre::test(re, &ascii_fold_safe(s))
}

/// Bare words and `define ...` queries; the dictionary lookup itself is currently switched off.
pub fn looks_like_dictionary_query(expr: &str) -> bool {
    static DEFINE_PREFIX: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:define[:\s]+|definition of\s+|meaning of\s+|what does\s+.+\s+mean\??$)",
            "i",
        )
    });
    static SINGLE_WORD: Lazy = Lazy::new(|| js_re(r"^\p{L}[\p{L}'’-]*$", ""));
    let t = js_trim(expr);
    if t.is_empty() {
        return false;
    }
    if test_i(&DEFINE_PREFIX, t) {
        return true;
    }
    if looks_like_natural_language(t) || !jsre::test(&SINGLE_WORD, t) {
        return false;
    }
    // `.length` of the letters, in UTF-16 units
    static LETTER: Lazy = Lazy::new(|| js_re(r"\p{L}", ""));
    LETTER
        .find_iter(t)
        .flatten()
        .map(|m| m.as_str().encode_utf16().count())
        .sum::<usize>()
        >= 2
}

/// `nativeEvalPayload`: what was sent to the host, without `undefined` fields.
pub fn native_eval_payload(req: &Json) -> Json {
    let mut out = Map::new();
    out.insert("type".into(), json!("eval"));
    out.insert("id".into(), req.get("id").cloned().unwrap_or(Json::Null));
    out.insert(
        "expr".into(),
        req.get("expr").cloned().unwrap_or(Json::Null),
    );
    let finite = |k: &str| req.get(k).and_then(Json::as_f64).filter(|n| n.is_finite());
    if finite("sigFigs").is_some() {
        out.insert("sigFigs".into(), req["sigFigs"].clone());
    }
    if finite("ans").is_some() {
        out.insert("ans".into(), req["ans"].clone());
    }
    let has_keys = match req.get("variables") {
        Some(Json::Object(o)) => !o.is_empty(),
        Some(Json::Array(a)) => !a.is_empty(),
        Some(Json::String(s)) => !s.is_empty(),
        _ => false,
    };
    if has_keys {
        out.insert("variables".into(), req["variables"].clone());
    }
    Json::Object(out)
}

// the only things soulvercore may answer: phrases the js engine can't read, never math it left blank on purpose
static SOULVER_ONLY: crate::jsre::Lazy<Vec<jsre::Regex>> = crate::jsre::Lazy::new(|| {
    [
        (r"[$€£¥₹]", ""),
        (r"\b(?:usd|eur|gbp|jpy|cad|aud|chf|cny|inr|mxn|dollars?|euros?|yen)\b", "i"),
        (r"(?:\d|\b)(?:am|pm)\b", "i"),
        (r"\d{1,2}:\d{2}", ""),
        (r"\b(?:today|tomorrow|yesterday|ago|until|since|between|from now|time in)\b", "i"),
        (r"\b(?:mon|tues|wednes|thurs|fri|satur|sun)day\b", "i"),
        (
            r"\b(?:january|february|march|april|june|july|august|september|october|november|december)\b",
            "i",
        ),
        (r"%\s*(?:off|on)\b|\bas an? %|\bwhat %", "i"),
        (r"\b(?:tip|lunch|dinner|people|person|nights|percent)\b", "i"),
    ]
    .into_iter()
    .map(|(p, f)| js_re(p, f))
    .collect()
});

pub fn looks_like_natural_language(expr: &str) -> bool {
    let t = js_trim(expr);
    if t.is_empty() || is_calculus_input(t) {
        return false;
    }
    SOULVER_ONLY.iter().any(|re| test_i(re, t))
}

/// SoulverCore reported failed conversions as "Error: ..." instead of an empty result.
pub fn usable_native_display(display: &str) -> String {
    static INCOMPATIBLE: Lazy = Lazy::new(|| js_re(r"^error:\s*incompatible units\b", "i"));
    static ERROR: Lazy = Lazy::new(|| js_re(r"^error\b", "i"));
    let t = js_trim(display);
    if t.is_empty() {
        return String::new();
    }
    if is_improper_unit_conversion(t) || test_i(&INCOMPATIBLE, t) {
        return IMPROPER_UNIT_CONVERSION.into();
    }
    if test_i(&ERROR, t) {
        return String::new();
    }
    t.to_string()
}

/// SoulverCore's trig was always in radians, so in degree mode it must not answer anything with trig in it.
pub fn soulver_angle_safe(expr: &str, angle_mode: &str) -> bool {
    static TRIG_CALL: Lazy = Lazy::new(|| {
        js_re(
            r"(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan|sec|csc|cot)(?![A-Za-z])",
            "i",
        )
    });
    angle_mode == "rad" || !test_i(&TRIG_CALL, expr)
}

fn display_of(v: &Json) -> &str {
    v.get("display").and_then(Json::as_str).unwrap_or("")
}

/// `{display, n?}` with `n` left out when undefined.
fn live(display: &str, n: Option<Json>) -> Json {
    let mut o = json!({ "display": display });
    if let Some(n) = n.filter(|n| !n.is_null()) {
        o["n"] = n;
    }
    o
}

/// `mergeLiveAnswer`: the engine's answer, unless a native one for this very line reads as natural language.
/// `js_n` and `native` are JSON (`null` for undefined).
pub fn merge_live_answer(expr: &str, js_display: &str, js_n: &Json, native: &Json) -> Json {
    static SUM_LINE: Lazy =
        Lazy::new(|| js_re(r"[Σ∑Π∏]|^\s*\\?(?:sum|prod|product)(?![A-Za-z0-9])", "i"));
    if js_trim(expr).is_empty() {
        return json!({ "display": "" });
    }
    let js_n = Some(js_n.clone());
    // soulver doesn't know Σ ranges; a blank from the js side is the honest answer
    if test_i(&SUM_LINE, expr) {
        return live(js_display, if js_display.is_empty() { None } else { js_n });
    }
    // soulvercore would read `∫0..1 1/x` as something; a blank calculus answer stays blank. it has no periodic
    // table either, so `molar mass of H2O` stays the engine's
    if is_calculus_input(expr) || is_molar_mass_input(expr) {
        return live(js_display, js_n);
    }
    let native_display = match native {
        Json::Object(o)
            if o.get("expr").and_then(Json::as_str) == Some(expr)
                && o.get("kind").and_then(Json::as_str) != Some("definition") =>
        {
            usable_native_display(display_of(native))
        }
        _ => String::new(),
    };
    if !native_display.is_empty() && looks_like_natural_language(expr) {
        return live(&native_display, native.get("n").cloned());
    }
    if js_display.is_empty() {
        json!({ "display": "" })
    } else {
        live(js_display, js_n)
    }
}

/// `nativeDefinition`: a native dictionary answer for this line.
pub fn native_definition(native: &Json, expr: &str) -> Json {
    let ok = native.is_object()
        && native.get("kind").and_then(Json::as_str) == Some("definition")
        && native.get("expr").and_then(Json::as_str) == Some(expr)
        && !js_trim(display_of(native)).is_empty();
    if ok { native.clone() } else { Json::Null }
}

fn truthy_str<'a>(v: &'a Json, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Json::as_str).filter(|s| !s.is_empty())
}

/// `nativeReplyToLive`: a host reply for the current request, as a live answer, or null.
pub fn native_reply_to_live(reply: &Json, expected_id: &Json, current_expr: &str) -> Json {
    let same_id = match (reply.get("id"), expected_id) {
        (Some(Json::Number(a)), Json::Number(b)) => a.as_f64() == b.as_f64(),
        (Some(a), b) => a == b && !a.is_number(),
        (None, _) => false,
    };
    if !same_id || reply.get("expr").and_then(Json::as_str) != Some(current_expr) {
        return Json::Null;
    }
    let display = usable_native_display(display_of(reply));
    if display.is_empty() {
        return Json::Null;
    }
    let n = reply
        .get("n")
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite())
        .map(|_| reply["n"].clone());
    let mut out = live(&display, n);
    out["expr"] = json!(current_expr);
    let definition = reply.get("kind").and_then(Json::as_str) == Some("definition")
        || ["term", "pos", "body"]
            .iter()
            .any(|k| truthy_str(reply, k).is_some());
    if definition {
        out["kind"] = json!("definition");
        for k in ["term", "pos", "pronunciation", "body"] {
            if let Some(v) = truthy_str(reply, k) {
                out[k] = json!(v);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_language_and_merges() {
        assert!(looks_like_natural_language("$10 for lunch + 15% tip"));
        assert!(!looks_like_natural_language("(1+2)*3"));
        assert!(looks_like_dictionary_query("Apple"));
        assert!(!looks_like_dictionary_query("72 f"));
        assert_eq!(
            usable_native_display("  Error: incompatible units"),
            IMPROPER_UNIT_CONVERSION
        );
        assert_eq!(usable_native_display("  Error: ∞/m  "), "");
        let native = json!({"expr": "$10 + 15%", "display": "ok", "n": 1});
        assert_eq!(
            merge_live_answer("$10 + 15%", "", &Json::Null, &native),
            json!({"display": "ok", "n": 1})
        );
        assert_eq!(
            merge_live_answer("Σ(k, 1, 3)", "", &json!(6), &native),
            json!({"display": ""})
        );
        assert!(!soulver_angle_safe("what is sin(30)", "deg"));
        assert!(soulver_angle_safe("sinh(1) per day", "deg"));
    }
}
