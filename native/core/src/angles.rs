//! `src/engine/angles.ts`: whether a plain-number answer is an angle, so it can say deg or rad (`asin(0.5)` is
//! 30 deg in deg mode, `2 asin(0.5)` is 60 deg, but `asin(0.5) / acos(0.5)` and `sin(asin(0.5))` are plain
//! numbers), and function calls among typed units worked out to their numbers.

use crate::jsnum;
use crate::jsre::{self, Lazy, js};
use crate::plain_math::normalize_math_text;
use crate::scientific::{NAMES, preprocess_ascii};
use crate::syntax::{Constant, Node, Op, parse};
use crate::types::AngleMode;

const INVERSE: &[&str] = &["asin", "acos", "atan", "atan2", "acsc", "asec", "acot"];
const TRIG: &[&str] = &["sin", "cos", "tan", "csc", "sec", "cot"];
// the answer is an angle when its arguments are
const SAME: &[&str] = &[
    "abs", "round", "floor", "ceil", "min", "max", "mean", "median", "sum", "mod",
];

/// How many times an angle the expression is: 1 for an angle, 0 for a plain number, None when it can't tell.
/// Only expressions with an inverse trig function are looked at; anything else is a plain number.
pub fn angle_exponent(expr: &str) -> Option<f64> {
    // after the engine's own rewrite, every way of writing one (tan^-1, tan⁻¹, arctan, \arctan) reads atan
    static MENTIONS_INVERSE: Lazy = Lazy::new(|| js(r"\ba(?:sin|cos|tan|csc|sec|cot|tan2)\b", "i"));
    let text = preprocess_ascii(&normalize_math_text(expr), &[]);
    if !jsre::test(&MENTIONS_INVERSE, &text) {
        return Some(0.0);
    }
    let node = parse(&text.replace(['×', '·'], "*")).ok()?;
    power(&node)
}

fn power(n: &Node) -> Option<f64> {
    match n {
        Node::Constant(_) | Node::Symbol(_) => Some(0.0),
        Node::Parenthesis(inner) => power(inner),
        Node::Function { callee, args } => {
            let name = match callee.as_ref() {
                Node::Symbol(s) => s.to_lowercase(),
                _ => String::new(),
            };
            let args: Vec<f64> = args.iter().map(power).collect::<Option<_>>()?;
            if INVERSE.contains(&name.as_str()) {
                return args.iter().all(|a| *a == 0.0).then_some(1.0);
            }
            if TRIG.contains(&name.as_str()) {
                return Some(0.0);
            }
            // round's second argument is how many digits, not an angle
            if name == "round" {
                return Some(args.first().copied().unwrap_or(0.0));
            }
            if SAME.contains(&name.as_str()) {
                let first = args.first().copied();
                return args
                    .iter()
                    .all(|a| Some(*a) == first)
                    .then(|| first.unwrap_or(0.0));
            }
            args.iter().all(|a| *a == 0.0).then_some(0.0)
        }
        Node::Operator { op, args, .. } => {
            let values: Vec<f64> = args.iter().map(power).collect::<Option<_>>()?;
            let a = values.first().copied();
            let b = values.get(1).copied();
            match op {
                Op::UnaryMinus | Op::UnaryPlus => a,
                // `asin(0.5) + 10` is 40 degrees: a plain number added to an angle is in its unit
                Op::Add | Op::Subtract => {
                    let (a, b) = (a?, b?);
                    if a == b {
                        Some(a)
                    } else if a == 0.0 {
                        Some(b)
                    } else if b == 0.0 {
                        Some(a)
                    } else {
                        None
                    }
                }
                Op::Multiply => Some(a? + b?),
                Op::Divide => Some(a? - b?),
                Op::Pow => {
                    if a? == 0.0 {
                        return Some(0.0);
                    }
                    match args.get(1) {
                        Some(Node::Constant(Constant::Number { value, .. })) => Some(a? * value),
                        _ => None,
                    }
                }
                _ => values.iter().all(|x| *x == 0.0).then_some(0.0),
            }
        }
        _ => None,
    }
}

/// The stub's integer form (`evaluate` only asks whether it is 1); a fractional exponent can't be 1 either.
pub fn angle_exponent_int(expr: &str) -> Option<i32> {
    angle_exponent(expr).and_then(|x| (jsnum::is_integer(x) && x.abs() < 1e9).then_some(x as i32))
}

/// The index of the `)` closing the `(` at `open`, or None.
fn closing(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, b) in text.bytes().enumerate().skip(open) {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn built_in(name: &str) -> bool {
    NAMES.split('|').any(|n| n.eq_ignore_ascii_case(name))
}

/// The expression with its function calls worked out, so they can sit among typed units, which the unit reader
/// can't call functions in: `10 N * cos(30)` → `10 N * (0.866…)`, `180 deg - asin(0.5)` → `180 deg - (30 deg)`,
/// `sqrt((3 m)^2 + (4 m)^2)` → `((3 m)^2 + (4 m)^2)^0.5`. An inverse trig answer goes in as an angle in the mode's
/// unit; a call with units inside is left for its inner calls. None when nothing changed.
pub fn calls_worked_out(
    expr: &str,
    mode: AngleMode,
    evaluate: &dyn Fn(&str) -> Option<f64>,
) -> Option<String> {
    // a root sign with brackets is the same call: √(16 m^2), ∛(8 m^3). a bare one (√2π°) is left to the engine's
    // own reading of how far it reaches, which the bar drawn over it in the input follows
    static BARE_ROOT: Lazy = Lazy::new(|| js(r"[√∛](?!\s*\()", ""));
    static SQRT: Lazy = Lazy::new(|| js(r"√\s*\(", ""));
    static CBRT: Lazy = Lazy::new(|| js(r"∛\s*\(", ""));
    if jsre::test(&BARE_ROOT, expr) {
        return None;
    }
    let spelled = jsre::replace_all(&SQRT, expr, |_| "sqrt(".into());
    let spelled = jsre::replace_all(&CBRT, &spelled, |_| "cbrt(".into()).into_owned();
    let mut changed = false;
    let mut left = EVALUATE_BUDGET;
    let result = work(&spelled, mode, evaluate, &mut changed, &mut left);
    // past the budget nothing is worked out: the line stays as blank as it was
    (changed && left > 0).then_some(result)
}

/// The characters [`work`] may hand to `evaluate` for one expression, counted like `work.ts` budgets. A call that
/// doesn't evaluate is worked out from inside, so its inner calls are evaluated again: a deep nest of calls that
/// never evaluates (`abs(abs(…abs(-1)…))` 1000 deep, past the parser's nesting limit) would cost the square of its
/// length, seconds of work (the TS engine has the same cost). Typed lines spend a few thousand at most.
const EVALUATE_BUDGET: usize = 40_000;

fn work(
    text: &str,
    mode: AngleMode,
    evaluate: &dyn Fn(&str) -> Option<f64>,
    changed: &mut bool,
    left: &mut usize,
) -> String {
    // a call to a built-in, however it's written: sqrt(, SIN(, \arcsin(, sin^-1(, sin^(-1)(, sin^{-1}(, sin⁻¹(
    // and with a power on the name: sin^2(, cos²(
    static CALL: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z_])\\?([A-Za-z][A-Za-z0-9]*?)\s*(?:\^\s*(?:-?\d+|\(\s*-?\d+\s*\)|\{\s*-?\d+\s*\})|[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+)?\s*\(",
            "",
        )
    });
    static GLUE: Lazy = Lazy::new(|| js(r"[A-Za-z0-9.)²³]\s*$", ""));
    static POWERED: Lazy = Lazy::new(|| js(r"[\^⁻⁰¹²³⁴⁵⁶⁷⁸⁹]", ""));
    let mut out = String::new();
    let mut from = 0;
    let mut at = 0;
    while at <= text.len() {
        let Ok(Some(caps)) = CALL.captures_from_pos(text, at) else {
            break;
        };
        let Some(m) = caps.get(0) else { break };
        // an empty match can't happen (the pattern ends in `(`), so the search always moves on
        at = m.end();
        let raw_name = jsre::group(&caps, 1);
        if !built_in(raw_name) {
            continue;
        }
        let name = raw_name.to_lowercase();
        let open = m.end() - 1;
        let Some(close) = closing(text, open) else {
            break;
        };
        let call = &text[m.start()..=close];
        let before = &text[from..m.start()];
        // `10 N cos(30)`: a call right after a quantity multiplies it
        let glue = if jsre::test(&GLUE, &format!("{out}{before}")) {
            "*"
        } else {
            ""
        };
        if call.len() >= *left {
            *left = 0;
            break;
        }
        *left -= call.len();
        let put = match evaluate(call).filter(|n| n.is_finite()) {
            Some(n) => {
                if angle_exponent(call) == Some(1.0) {
                    let unit = if mode == AngleMode::Rad { "rad" } else { "deg" };
                    format!("({} {unit})", jsnum::to_string(n))
                } else {
                    format!("({})", jsnum::to_string(n))
                }
            }
            None => {
                let inner = work(&text[open + 1..close], mode, evaluate, changed, left);
                // a root of a quantity is a power of it: sqrt(16 m^2) is (16 m^2)^0.5
                let root_power = match name.as_str() {
                    "sqrt" => Some("0.5"),
                    "cbrt" => Some("(1/3)"),
                    _ => None,
                };
                let power = if jsre::test(&POWERED, m.as_str()) {
                    None
                } else {
                    root_power
                };
                match power {
                    Some(p) => format!("({inner})^{p}"),
                    None => {
                        // nothing to do with the call itself; its inner calls may still have been worked out
                        out.push_str(before);
                        out.push_str(&text[m.start()..=open]);
                        out.push_str(&inner);
                        out.push(')');
                        from = close + 1;
                        at = from;
                        continue;
                    }
                }
            }
        };
        *changed = true;
        out.push_str(before);
        out.push_str(glue);
        out.push_str(&put);
        from = close + 1;
        at = from;
    }
    out.push_str(&text[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_of_inverse_trig() {
        assert_eq!(angle_exponent("2 + 3"), Some(0.0));
        assert_eq!(angle_exponent("3 * acos(0.1) - 4"), Some(1.0));
        assert_eq!(angle_exponent("asin(0.5) / acos(0.5)"), Some(0.0));
        assert_eq!(angle_exponent("asin(0.5)^2"), Some(2.0));
        assert_eq!(angle_exponent("asin(asin(0.5))"), None);
        assert_eq!(angle_exponent("cos(asin(0.5))"), Some(0.0));
    }

    #[test]
    fn calls_among_units() {
        let eval = |c: &str| match c {
            "cos(60)" => Some(0.5),
            "asin(0.5)" => Some(30.0),
            _ => None,
        };
        assert_eq!(
            calls_worked_out("10 N cos(60)", AngleMode::Deg, &eval).as_deref(),
            Some("10 N *(0.5)")
        );
        assert_eq!(
            calls_worked_out("180 deg - asin(0.5)", AngleMode::Deg, &eval).as_deref(),
            Some("180 deg - (30 deg)")
        );
        assert_eq!(
            calls_worked_out("sqrt(16 m^2)", AngleMode::Deg, &eval).as_deref(),
            Some("(16 m^2)^0.5")
        );
        assert_eq!(calls_worked_out("5 m", AngleMode::Deg, &eval), None);
    }
}
