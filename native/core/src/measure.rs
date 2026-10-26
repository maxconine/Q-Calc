//! Uncertainty and significant figures: `src/engine/measure.ts`. Sig figs come from the typed literals, `±`
//! propagates worst case, and anything not modelled gives no answer rather than one that drops the `±`.
//!
//! Internally sig figs and decimal places are doubles, as in the TS: exact parts carry infinite of both, and a
//! measured zero can have zero or negative sig figs (`1.0 - 0.999` is 0.001 to one place, sig -1).

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json, json};

use crate::jsnum;
use crate::jsre::{self, Lazy, js};
use crate::parens::fill_parens;
use crate::scientific::functions::jsmath;
use crate::scientific::preprocess::rewrite_typeset_mul;
use crate::scientific::{ScientificContext, eval_scientific, preprocess_ascii};
use crate::syntax::{Constant, Node, Op};
use crate::types::{AngleMode, Meas, UserFunction, ValueKind};

/// `MeasureContext`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MeasureContext<'a> {
    pub ans: Option<f64>,
    pub angle_mode: AngleMode,
    pub variables: Option<&'a BTreeMap<String, f64>>,
    pub functions: Option<&'a BTreeMap<String, UserFunction>>,
    pub measures: Option<&'a BTreeMap<String, Meas>>,
}

/// A value with its sig figs and ±; `meas` is None when nothing is worth storing (an exact answer).
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub v: f64,
    pub meas: Option<Meas>,
}

/// A walked part. Exact parts carry infinite sig figs and decimal places; `digits` marks a ± still as typed.
#[derive(Clone, Debug, PartialEq)]
pub struct M {
    pub v: f64,
    pub sig: f64,
    pub dp: f64,
    pub unc: f64,
    pub digits: Option<String>,
}

const NUM: &str = r"(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?";

static LITERAL_RE: Lazy = Lazy::new(|| js(&format!(r"(?<![A-Za-z_0-9.]){NUM}"), "i"));
// ± binds tighter than any operator; a measurement is one number (`5.0 ± 0.2 * 3` = 15.0 ± 0.6)
static PLUS_MINUS_RE: Lazy = Lazy::new(|| {
    js(
        &format!(r"(?<![A-Za-z_0-9.])({NUM})\s*±\s*({NUM})(\s*%)?"),
        "i",
    )
});

// keep the sig figs of the argument
const SIG_FNS: &[&str] = &[
    "sqrt", "cbrt", "sin", "cos", "tan", "csc", "sec", "cot", "asin", "acos", "atan", "acsc",
    "asec", "acot", "sinh", "cosh", "tanh", "csch", "sech", "coth", "asinh", "acosh", "atanh",
    "exp", "abs",
];
// keep as many decimal places as the argument has sig figs
const LOG_FNS: &[&str] = &["ln", "log", "log10", "log2"];
const MIN_SIG_FNS: &[&str] = &["min", "max", "mean", "median", "hypot", "atan2"];
const SUM_FNS: &[&str] = &["sum", "total"];

fn root_power(name: &str) -> Option<f64> {
    match name {
        "sqrt" => Some(0.5),
        "cbrt" => Some(1.0 / 3.0),
        _ => None,
    }
}

fn exact(v: f64) -> M {
    M {
        v,
        sig: f64::INFINITY,
        dp: f64::INFINITY,
        unc: 0.0,
        digits: None,
    }
}

/// `10 ** x` in JS.
fn ten_to(x: f64) -> f64 {
    jsmath::pow(10.0, x)
}

// float noise below a power of ten (0.9999999999999999) counts as that power
fn mag(v: f64) -> f64 {
    (jsmath::log10(v.abs()) + 1e-12).floor()
}

// the magnitude after rounding, so 9.9989 to one place is 10.0 with 3 sig figs, not 1.0e1
fn rounded_mag(v: f64, dp: f64) -> f64 {
    let scale = ten_to(dp.clamp(-300.0, 300.0));
    let r = jsnum::round(v.abs() * scale) / scale;
    if r == 0.0 || !r.is_finite() {
        mag(v)
    } else {
        js_max(mag(v), mag(r))
    }
}

/// `Math.max(a, b)`, NaN if either is.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// `Math.min(a, b)`, NaN if either is.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn by_sig(v: f64, sig: f64, unc: f64) -> M {
    let m = |dp| M {
        v,
        sig,
        dp,
        unc,
        digits: None,
    };
    if !sig.is_finite() {
        return m(f64::INFINITY);
    }
    if v == 0.0 {
        return m(sig - 1.0);
    }
    let dp = sig - 1.0 - mag(v);
    m(sig - 1.0 - rounded_mag(v, dp))
}

fn by_dp(v: f64, dp: f64, unc: f64) -> M {
    let sig = if !dp.is_finite() {
        f64::INFINITY
    } else if v == 0.0 {
        0.0
    } else {
        rounded_mag(v, dp) + 1.0 + dp
    };
    M {
        v,
        sig,
        dp,
        unc,
        digits: None,
    }
}

/// `2.50` → 3 s.f.; `6.02e23` → 3 s.f.; `300.` → 3 s.f.; a bare integer is exact.
pub fn literal_meas(text: &str) -> M {
    let v = jsnum::parse(text).unwrap_or(f64::NAN);
    let lower = text.to_lowercase();
    let mut parts = lower.split('e');
    let mant = parts.next().unwrap_or("");
    let exp = parts.next();
    let dot = mant.find('.');
    if dot.is_none() && exp.is_none() {
        return exact(v);
    }
    let decimals = dot.map_or(0, |d| mant.len() - d - 1) as f64;
    let digits = mant.replacen('.', "", 1);
    let digits = digits.trim_start_matches('0');
    let dp = decimals - exp.map_or(0.0, |e| jsnum::parse(e).unwrap_or(f64::NAN));
    M {
        v,
        sig: if v == 0.0 {
            0.0
        } else {
            (digits.chars().count() as f64).max(1.0)
        },
        dp,
        unc: 0.0,
        digits: None,
    }
}

/// `0.10` → `10`, `1e-1` → `1`; a whole number's trailing zeros don't count (`10` → `1`).
pub fn typed_digits(text: &str) -> String {
    let lower = text.to_lowercase();
    let mant = lower.split('e').next().unwrap_or("");
    let digits = mant.replacen('.', "", 1);
    let digits = digits.trim_start_matches('0');
    if mant.contains('.') {
        digits.to_string()
    } else {
        digits.trim_end_matches('0').to_string()
    }
}

fn from_stored(v: f64, meas: Option<&Meas>) -> M {
    let Some(meas) = meas else { return exact(v) };
    let unc = meas.unc.unwrap_or(0.0);
    let mut m = match (meas.dp, meas.sig) {
        (Some(dp), _) => by_dp(v, f64::from(dp), unc),
        (None, Some(sig)) => by_sig(v, f64::from(sig), unc),
        (None, None) => M { unc, ..exact(v) },
    };
    if let Some(d) = meas.unc_digits.as_ref().filter(|d| !d.is_empty()) {
        m.digits = Some(d.clone());
    }
    m
}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

/// sig and dp are kept only as a pair; nothing worth storing gives None.
fn to_stored(m: &M) -> Option<Meas> {
    let mut out = Meas::default();
    if let (Some(sig), Some(dp)) = (finite(m.sig), finite(m.dp)) {
        out.sig = Some(sig as i32);
        out.dp = Some(dp as i32);
    }
    if m.unc > 0.0 {
        out.unc = Some(m.unc);
        if let Some(d) = m.digits.as_ref().filter(|d| !d.is_empty()) {
            out.unc_digits = Some(d.clone());
        }
    }
    (out.sig.is_some() || out.unc.is_some()).then_some(out)
}

/// `sanitizeMeas`, for metadata read back from history JSON: the stored shape, or None.
pub fn sanitize_meas(raw: &Json) -> Option<Json> {
    let r = raw.as_object()?;
    let num = |k: &str| r.get(k).and_then(Json::as_f64).filter(|x| x.is_finite());
    let digits = r.get("uncDigits").and_then(Json::as_str).filter(|d| {
        let b = d.as_bytes();
        !b.is_empty()
            && b.len() <= 16
            && (b'1'..=b'9').contains(&b[0])
            && b.iter().all(u8::is_ascii_digit)
    });
    let mut out = Map::new();
    if let (Some(sig), Some(dp)) = (num("sig"), num("dp")) {
        out.insert("sig".into(), json!(sig));
        out.insert("dp".into(), json!(dp));
    }
    if let Some(unc) = num("unc").filter(|u| *u > 0.0) {
        out.insert("unc".into(), json!(unc));
        if let Some(d) = digits {
            out.insert("uncDigits".into(), json!(d));
        }
    }
    (out.contains_key("sig") || out.contains_key("unc")).then_some(Json::Object(out))
}

pub fn has_plus_minus(text: &str) -> bool {
    text.contains('±') || text.contains('∓')
}

fn call_value(name: &str, args: &[f64], ctx: &MeasureContext<'_>) -> Option<f64> {
    let text = format!(
        "{name}({})",
        args.iter()
            .map(|a| jsnum::to_string(*a))
            .collect::<Vec<_>>()
            .join(",")
    );
    let sci = ScientificContext {
        angle_mode: ctx.angle_mode,
        ..ScientificContext::default()
    };
    let out = eval_scientific(&text, &sci)?;
    (out.kind == ValueKind::Number && out.n.is_finite()).then_some(out.n)
}

fn walk_binary(op: Op, a: &M, b: &M) -> Option<M> {
    match op {
        Op::Add => Some(by_dp(a.v + b.v, js_min(a.dp, b.dp), a.unc + b.unc)),
        Op::Subtract => Some(by_dp(a.v - b.v, js_min(a.dp, b.dp), a.unc + b.unc)),
        Op::Multiply => {
            let unc = a.v.abs() * b.unc + b.v.abs() * a.unc;
            // a measured zero has a place but no sig figs: (1.0 - 1.0) × 2.001 is 0.0, not 0 to the tens
            if a.sig <= 0.0 && b.sig <= 0.0 {
                return Some(by_dp(a.v * b.v, a.dp + b.dp, unc));
            }
            let zero = if a.sig <= 0.0 {
                Some((a, b))
            } else if b.sig <= 0.0 {
                Some((b, a))
            } else {
                None
            };
            if let Some((z, other)) = zero {
                return Some(by_dp(
                    a.v * b.v,
                    z.dp - jsnum::round(jsmath::log10(other.v.abs())),
                    unc,
                ));
            }
            Some(by_sig(a.v * b.v, js_min(a.sig, b.sig), unc))
        }
        Op::Divide => {
            if b.v == 0.0 {
                return None;
            }
            let v = a.v / b.v;
            let unc = (a.unc + v.abs() * b.unc) / b.v.abs();
            if a.sig <= 0.0 && b.sig > 0.0 {
                return Some(by_dp(v, a.dp + jsnum::round(jsmath::log10(b.v.abs())), unc));
            }
            Some(by_sig(v, js_min(a.sig, b.sig), unc))
        }
        Op::Pow => {
            if b.unc > 0.0 {
                return None;
            }
            let v = crate::eval::ops::js_pow(a.v, b.v);
            if !v.is_finite() || (a.unc > 0.0 && a.v == 0.0) {
                return None;
            }
            let unc = if a.unc > 0.0 {
                (v * b.v).abs() * a.unc / a.v.abs()
            } else {
                0.0
            };
            // exponents are exact: `2.0^0.5` keeps 2 sig figs, like `sqrt(2.0)`
            Some(by_sig(v, a.sig, unc))
        }
        _ => None,
    }
}

fn walk_function(name: &str, args: &[M], ctx: &MeasureContext<'_>) -> Option<M> {
    let values: Vec<f64> = args.iter().map(|a| a.v).collect();
    let v = call_value(name, &values, ctx)?;
    let sum = SUM_FNS.contains(&name);
    if MIN_SIG_FNS.contains(&name) || sum {
        if args.iter().any(|a| a.unc > 0.0) {
            return None;
        }
        if sum {
            let dp = args.iter().map(|a| a.dp).fold(f64::INFINITY, js_min);
            return Some(by_dp(v, dp, 0.0));
        }
        let sig = args.iter().map(|a| a.sig).fold(f64::INFINITY, js_min);
        return Some(by_sig(v, sig, 0.0));
    }
    let log = LOG_FNS.contains(&name);
    if !log && !SIG_FNS.contains(&name) {
        return None;
    }
    let a = args.first()?;
    let base = args.get(1);
    if args.len() > if name == "log" { 2 } else { 1 } {
        return None;
    }
    if base.is_some_and(|b| b.sig.is_finite() || b.unc > 0.0) {
        return None;
    }
    let mut unc = 0.0;
    if a.unc > 0.0 {
        if let Some(power) = root_power(name) {
            if a.v == 0.0 {
                return None;
            }
            unc = (v * power).abs() * a.unc / a.v.abs();
        } else {
            let extra: Vec<f64> = base.map(|b| b.v).into_iter().collect();
            let with = |x: f64| [vec![x], extra.clone()].concat();
            let hi = call_value(name, &with(a.v + a.unc), ctx)?;
            let lo = call_value(name, &with(a.v - a.unc), ctx)?;
            unc = js_max((hi - v).abs(), (lo - v).abs());
        }
    }
    Some(if log {
        by_dp(v, a.sig, unc)
    } else {
        by_sig(v, a.sig, unc)
    })
}

struct Walker<'c, 'a> {
    literals: Vec<M>,
    ctx: &'c MeasureContext<'a>,
}

impl Walker<'_, '_> {
    fn walk(&self, node: &Node) -> Option<M> {
        static LIT: Lazy = Lazy::new(|| js(r"^__L(\d+)$", ""));
        match node {
            Node::Parenthesis(c) => self.walk(c),
            Node::Constant(Constant::Number { value, .. }) => Some(exact(*value)),
            Node::Symbol(name) => {
                if let Ok(Some(c)) = LIT.captures(name.as_str()) {
                    return jsre::group(&c, 1)
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| self.literals.get(i).cloned());
                }
                let measures = self.ctx.measures;
                match name.as_str() {
                    "pi" => return Some(exact(std::f64::consts::PI)),
                    "tau" => return Some(exact(std::f64::consts::PI * 2.0)),
                    "e" => return Some(exact(std::f64::consts::E)),
                    "ans" => {
                        return self
                            .ctx
                            .ans
                            .map(|a| from_stored(a, measures.and_then(|m| m.get("ans"))));
                    }
                    _ => {}
                }
                let v = *self.ctx.variables?.get(name)?;
                v.is_finite()
                    .then(|| from_stored(v, measures.and_then(|m| m.get(name))))
            }
            Node::Operator { op, args, .. } => {
                let walked = args
                    .iter()
                    .map(|a| self.walk(a))
                    .collect::<Option<Vec<M>>>()?;
                let a = walked.first()?;
                match op {
                    Op::UnaryMinus => {
                        return Some(M {
                            v: -a.v,
                            ..a.clone()
                        });
                    }
                    Op::UnaryPlus => return Some(a.clone()),
                    _ => {}
                }
                if walked.len() != 2 {
                    return None;
                }
                walk_binary(*op, a, &walked[1])
            }
            Node::Function { callee, args } => {
                let Node::Symbol(name) = callee.as_ref() else {
                    return None;
                };
                if name.is_empty() || self.ctx.functions.is_some_and(|f| f.contains_key(name)) {
                    return None;
                }
                let walked = args
                    .iter()
                    .map(|a| self.walk(a))
                    .collect::<Option<Vec<M>>>()?;
                if walked.is_empty() {
                    return None;
                }
                walk_function(name, &walked, self.ctx)
            }
            _ => None,
        }
    }
}

/// Sig figs come from the literals and ± propagates worst case. None for anything not modelled.
pub fn measure(text: &str, ctx: &MeasureContext<'_>) -> Option<Measured> {
    let src = fill_parens(&rewrite_typeset_mul(text.trim()));
    if src.is_empty() || src.contains('|') {
        return None;
    }
    let mut literals: Vec<M> = Vec::new();
    let hold = |m: M, literals: &mut Vec<M>| {
        literals.push(m);
        format!("(__L{})", literals.len() - 1)
    };
    let src = jsre::replace_all(&PLUS_MINUS_RE, &src, |c| {
        let m = literal_meas(jsre::group(c, 1));
        let u = jsre::group(c, 2);
        let un = jsnum::parse(u).unwrap_or(f64::NAN);
        let held = if c.get(3).is_some_and(|p| !p.as_str().is_empty()) {
            M {
                unc: (m.v * un / 100.0).abs(),
                ..m
            }
        } else {
            M {
                unc: un,
                digits: Some(typed_digits(u)),
                ..m
            }
        };
        hold(held, &mut literals)
    })
    .into_owned();
    // any other ± (`x ± 1`, `(1+2) ± 1`) is not modelled; no answer beats one that drops the ±
    if src.contains('±') {
        return None;
    }
    let fn_names: Vec<String> = ctx
        .functions
        .map(|f| f.keys().cloned().collect())
        .unwrap_or_default();
    let expr = jsre::replace_all(&LITERAL_RE, &preprocess_ascii(&src, &fn_names), |c| {
        hold(literal_meas(jsre::group(c, 0)), &mut literals)
    })
    .into_owned();
    let root = crate::syntax::parse(&expr).ok()?;
    let m = Walker { literals, ctx }.walk(&root)?;
    if !m.v.is_finite() || !m.unc.is_finite() {
        return None;
    }
    Some(Measured {
        v: m.v,
        meas: to_stored(&m),
    })
}

/// `-0.0` becomes `0.0`.
fn unsigned(s: String) -> String {
    if jsnum::parse(&s) == Some(0.0) {
        s.strip_prefix('-').map(str::to_string).unwrap_or(s)
    } else {
        s
    }
}

fn strip_exp_plus(s: &str) -> String {
    s.replacen("e+", "e", 1)
}

/// JS `toFixed(d)` with `d` already clamped to 0..=20 (a fraction truncates, as `ToIntegerOrInfinity` does).
fn fixed(v: f64, d: f64) -> String {
    jsnum::to_fixed(v, d as usize)
}

/// Keeps trailing zeros; ambiguous integer zeros go scientific (`1.00e2`).
pub fn format_sig(v: f64, sig: f64, dp: f64) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 || sig < 1.0 {
        return Some(unsigned(fixed(v, dp.clamp(0.0, 20.0).max(0.0))));
    }
    if sig > 16.0 {
        return None;
    }
    // round at the decimal place first, so 0.959 to one place is 1.0, not 0.96 or 1
    let scale = if dp.is_finite() {
        ten_to(dp.clamp(-300.0, 300.0))
    } else {
        0.0
    };
    let r = if scale != 0.0 && !scale.is_nan() {
        v.signum() * jsnum::round(v.abs() * scale) / scale
    } else {
        v
    };
    let shown = if r == 0.0 || !r.is_finite() { v } else { r };
    let s = jsnum::to_precision(shown, sig as usize);
    if s.contains('e') {
        return Some(strip_exp_plus(&s));
    }
    if !s.contains('.') && s.ends_with('0') {
        return Some(strip_exp_plus(&jsnum::to_exponential(
            shown,
            sig as usize - 1,
        )));
    }
    Some(s)
}

fn round_sig(v: f64, sig: f64) -> f64 {
    if v == 0.0 || !sig.is_finite() || !(1.0..=16.0).contains(&sig) {
        return v;
    }
    jsnum::parse(&jsnum::to_precision(v, sig as usize)).unwrap_or(v)
}

/// Whether `unc` still has exactly the typed digits, perhaps moved by a power of ten (`0.1 m` → `10 cm`).
fn is_typed(unc: f64, digits: Option<&str>) -> bool {
    let Some(digits) = digits.filter(|d| !d.is_empty()) else {
        return false;
    };
    if digits.len() > 16 {
        return false;
    }
    let s = jsnum::to_precision(unc, digits.len());
    if (jsnum::parse(&s).unwrap_or(f64::NAN) - unc).abs() > 1e-9 * unc {
        return false;
    }
    let mant = s.split('e').next().unwrap_or("").replacen('.', "", 1);
    mant.trim_start_matches('0') == digits
}

/// `10.0 ± 0.7`: the uncertainty at one sig fig, or two when it leads with a 1 (`5.0 ± 0.14`), the value to the
/// same place. A ± still as typed keeps exactly its typed digits (`10 ± 1`, `2.00 ± 0.35`).
pub fn format_uncertain(v: f64, unc: f64, unc_digits: Option<&str>) -> Option<String> {
    if !v.is_finite() || unc.is_nan() || unc <= 0.0 || !unc.is_finite() {
        return None;
    }
    static LEADS_ONE: Lazy = Lazy::new(|| js(r"^[0.]*1", ""));
    let two = jsnum::parse(&jsnum::to_precision(unc, 2)).unwrap_or(f64::NAN);
    let mut figs = if jsre::test(&LEADS_ONE, &jsnum::to_string(two)) {
        2
    } else {
        1
    };
    if is_typed(unc, unc_digits) {
        figs = unc_digits.map_or(1, str::len);
    }
    let u = jsnum::parse(&jsnum::to_precision(unc, figs)).unwrap_or(f64::NAN);
    let place = mag(u) - figs as f64 + 1.0;
    let decimals = (-place).clamp(0.0, 20.0);
    let value = if place > 0.0 {
        let p = ten_to(place);
        fixed(jsnum::round(v / p) * p, 0.0)
    } else {
        fixed(v, decimals)
    };
    Some(format!("{} ± {}", unsigned(value), fixed(u, decimals)))
}

/// None when the normal display applies. Sig-fig rounding (if the mode is on) happens before ± rounding.
pub fn format_measured(v: f64, meas: &Meas, sig_fig_mode: bool) -> Option<String> {
    let sig = meas.sig.filter(|_| sig_fig_mode).map(f64::from);
    if let Some(unc) = meas.unc.filter(|u| *u != 0.0 && !u.is_nan()) {
        let v = sig.map_or(v, |s| round_sig(v, s));
        return format_uncertain(v, unc, meas.unc_digits.as_deref());
    }
    format_sig(v, sig?, meas.dp.map_or(f64::INFINITY, f64::from))
}

fn m_json(m: &M) -> Json {
    let mut o = json!({ "v": m.v, "sig": m.sig, "dp": m.dp, "unc": m.unc });
    if let Some(d) = &m.digits {
        o["digits"] = json!(d);
    }
    o
}

/// Golden replay for `engine/measure`.
pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let n = |i: usize| crate::golden::f64_arg(args, i).unwrap_or(f64::NAN);
    let opt = |v: Option<String>| v.map_or(Json::Null, Json::String);
    Some(match name {
        "literalMeas" => m_json(&literal_meas(s(0))),
        "typedDigits" => json!(typed_digits(s(0))),
        "hasPlusMinus" => json!(has_plus_minus(s(0))),
        "sanitizeMeas" => sanitize_meas(args.first()?).unwrap_or(Json::Null),
        "formatSig" => opt(format_sig(n(0), n(1), n(2))),
        "formatUncertain" => opt(format_uncertain(
            n(0),
            n(1),
            args.get(2).and_then(Json::as_str),
        )),
        "measure" if args.len() == 1 => match measure(s(0), &MeasureContext::default()) {
            None => Json::Null,
            Some(m) => {
                let mut o = json!({ "v": m.v });
                if let Some(meas) = m.meas {
                    o["meas"] = serde_json::to_value(meas).ok()?;
                }
                o
            }
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str) -> Option<String> {
        let m = measure(text, &MeasureContext::default())?;
        format_measured(m.v, m.meas.as_ref()?, false)
    }

    #[test]
    fn propagates_like_the_ts() {
        assert_eq!(
            shown("(2.0 ± 0.1) + (3.0 ± 0.1)").as_deref(),
            Some("5.0 ± 0.2")
        );
        assert_eq!(shown("5.0 ± 0.2 * 3").as_deref(), Some("15.0 ± 0.6"));
        assert_eq!(format_sig(100.0, 3.0, 0.0).as_deref(), Some("1.00e2"));
        assert_eq!(
            format_uncertain(9.96, 0.74, None).as_deref(),
            Some("10.0 ± 0.7")
        );
        assert_eq!(typed_digits("0.10"), "10");
        assert!(measure("hello", &MeasureContext::default()).is_none());
    }
}
