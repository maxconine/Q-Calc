//! The scientific layer (`src/engine/scientific.ts`): typed maths → mathjs syntax → a value.

pub mod functions;
pub mod preprocess;

use std::collections::BTreeMap;

use crate::eval::{self, Scope, Val};
use crate::format::format_number;
use crate::jsre::{self, Lazy};
use crate::syntax::parse;
use crate::types::{AngleMode, Complex, UserFunction, Value, ValueKind};

pub use preprocess::{names_pattern, preprocess_ascii, preprocess_checked, split_glued_functions};

/// Every built-in name, as `SCIENTIFIC_NAMES` lists them.
pub const NAMES: &str = "sqrt|cbrt|nthroot|nthRoot|sin|cos|tan|csc|sec|cot|asin|acos|atan|atan2|arcsin|arccos|arctan|arctan2|arccsc|arcsec|arccot|acsc|asec|acot|sinh|cosh|tanh|csch|sech|coth|asinh|acosh|atanh|arsinh|arcosh|artanh|arcsinh|arccosh|arctanh|arccsch|arcsech|arccoth|acsch|asech|acoth|ln|log|log2|log10|exp|abs|sign|floor|ceil|round|clamp|min|max|mean|median|mad|std|stdev|stdevp|var|varp|sum|total|length|count|quartile|quantile|corr|gcd|gcf|hcf|lcm|lcf|mod|hypot|factorial|nCr|nPr|combinations|permutations|randint|rand|random|re|im|real|imag|conj|arg|range|inclusiveRange|zeta|catalan|pi|tau|inf|infinity|ans";

/// Cap on list sizes from `[a...b]`, `random(N)` and `randint(lo, hi, count)`.
pub const MAX_LIST_ALLOC: usize = 10_000;

/// What `evalScientific` sees besides the text.
#[derive(Clone, Debug, Default)]
pub struct ScientificContext {
    pub ans: Option<f64>,
    pub angle_mode: AngleMode,
    pub variables: BTreeMap<String, f64>,
    pub functions: BTreeMap<String, UserFunction>,
    /// User-function nesting, so a function calling itself ends blank instead of overflowing the stack.
    pub depth: u32,
}

fn chop(n: f64) -> f64 {
    if n.is_finite() && n.abs() < 1e-12 {
        0.0
    } else {
        n
    }
}

fn snap_int(n: f64) -> f64 {
    if !n.is_finite() || n == 0.0 {
        return n;
    }
    let r = crate::jsnum::round(n);
    if r == 0.0 {
        return n;
    }
    if (n - r).abs() <= f64::EPSILON * r.abs().max(1.0) {
        r
    } else {
        n
    }
}

/// `5 + 5i`, `3 - i`, `-2i`: the real part first, a coefficient of 1 left out.
pub fn format_complex(re: f64, im: f64) -> String {
    let real = chop(re);
    let size = im.abs();
    let unit = if (size - 1.0).abs() < 1e-12 {
        "i".to_string()
    } else {
        format!("{}i", format_number(snap_int(size), 12.0))
    };
    if real.abs() < 1e-12 {
        return if im < 0.0 { format!("-{unit}") } else { unit };
    }
    format!(
        "{} {} {unit}",
        format_number(snap_int(real), 12.0),
        if im < 0.0 { '-' } else { '+' }
    )
}

const MAX_LIST_SHOWN: usize = 16;

fn list_entry(v: f64) -> String {
    if v.is_nan() {
        "undefined".into()
    } else if v.is_finite() {
        format_number(v, 12.0)
    } else {
        "overflow".into()
    }
}

/// `[1, 2, 3]`, or the first six and last four of a long list.
pub fn format_list(values: &[f64]) -> String {
    if values.len() <= MAX_LIST_SHOWN {
        return format!(
            "[{}]",
            values
                .iter()
                .map(|v| list_entry(*v))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let head: Vec<String> = values[..6].iter().map(|v| list_entry(*v)).collect();
    let tail: Vec<String> = values[values.len() - 4..]
        .iter()
        .map(|v| list_entry(*v))
        .collect();
    format!("[{}, …, {}]", head.join(", "), tail.join(", "))
}

fn flatten(v: &Val, out: &mut Vec<f64>) {
    match v {
        Val::List(items) => items.iter().for_each(|i| flatten(i, out)),
        other => out.push(other.to_number()),
    }
}

/// A typed imaginary unit: `i`, `2i`, `(1+2i)`; never the i inside pi, sin or phi.
pub fn typed_i(expr: &str) -> bool {
    static RE: Lazy =
        Lazy::new(|| jsre::js(r"(?<![A-Za-z_])(?:\d+\.?\d*|\.\d+)?i(?![A-Za-z0-9_])", ""));
    jsre::test(&RE, expr)
}

/// `fromMathjs` and `finish`: a mathjs value as an answer. NaN is undefined; an infinity is a result too big for a
/// double, unless a typed ∞ went in.
pub fn finish(v: &Val, expr: &str) -> Option<Value> {
    // WP-SOLVE: pub for compileScientific
    let out = match v {
        Val::Num(n) if n.is_nan() => Value::text("undefined"),
        Val::Num(n) => Value::number(snap_int(*n)),
        Val::Bool(b) => Value::number(f64::from(u8::from(*b))),
        Val::Complex { re, im } => {
            if !re.is_finite() || !im.is_finite() {
                Value::text("undefined")
            } else if im.abs() > 1e-12 {
                // a complex answer shows only when an i was typed; `sqrt(-1)` in plain real maths stays undefined
                if typed_i(expr) {
                    Value {
                        complex: Some(Complex { re: *re, im: *im }),
                        ..Value::text(format_complex(*re, *im))
                    }
                } else {
                    Value::text("undefined")
                }
            } else {
                Value::number(chop(*re))
            }
        }
        Val::List(_) => {
            let mut nums = Vec::new();
            flatten(v, &mut nums);
            Value::text(format_list(&nums))
        }
        _ => return None,
    };
    if out.kind == ValueKind::Number && !out.n.is_finite() {
        static TYPED_INF: Lazy = Lazy::new(|| jsre::js(r"Infinity", ""));
        return Some(Value::text(if jsre::test(&TYPED_INF, expr) {
            "undefined"
        } else {
            "overflow"
        }));
    }
    Some(out)
}

/// The scope an expression evaluates in.
pub fn scope_for(expr: &str, ctx: &ScientificContext) -> Scope {
    Scope {
        variables: ctx.variables.clone(),
        functions: ctx.functions.clone(),
        angle_mode: ctx.angle_mode,
        ans: ctx.ans,
        complex_typed: typed_i(expr),
        depth: ctx.depth,
    }
}

/// `evalScientific`: the float answer, before `crate::precise` replaces float artefacts with true digits.
pub fn eval_scientific(text: &str, ctx: &ScientificContext) -> Option<Value> {
    let expr = preprocess_checked(text, ctx)?;
    // parsing costs several times an evaluation
    crate::work::spend(300 + 10 * expr.chars().count() as u64);
    let node = parse(&expr).ok()?;
    let v = eval::evaluate(&node, &scope_for(&expr, ctx)).ok()?;
    finish(&v, &expr)
}

/// `formatAsFraction` lives in `crate::format`; this is the name the TS callers use.
pub use crate::format::format_as_fraction;

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(s: &str) -> String {
        eval_scientific(s, &ScientificContext::default())
            .map(|v| crate::format::format_value(&v, 12.0))
            .unwrap_or_default()
    }

    #[test]
    fn evaluates_typed_maths() {
        assert_eq!(shown("2+3*4"), "14");
        assert_eq!(shown("3!"), "6");
        assert_eq!(shown("2³"), "8");
        assert_eq!(shown("200 + 15%"), "230");
        assert_eq!(shown("1/0"), "undefined");
        assert_eq!(shown("1e308*10"), "overflow");
        assert_eq!(shown("√16"), "4");
        assert_eq!(shown("|-3|"), "3");
        assert_eq!(shown("2:30"), "");
        assert_eq!(shown("[1,2,3]"), "[1, 2, 3]");
    }

    #[test]
    fn complex_formatting() {
        assert_eq!(format_complex(5.0, 5.0), "5 + 5i");
        assert_eq!(format_complex(3.0, -1.0), "3 - i");
        assert_eq!(format_complex(0.0, -2.0), "-2i");
    }
}
