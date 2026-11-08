//! Native only: lengths in feet and inches. `'` and `"` already read as ft and in (the unit table's aliases);
//! this adds the other ways people type them (`′`, `″`, `''`, `6'2`), answers a line that mixed the two in the same
//! form (`6 ft 2 in`), and lets ⌥↑ / ⌥↓ move that answer to inches or feet alone.
//!
//! A mark after a letter or a bracket is a prime (`f'(x)`, `y'`), so only a mark straight after a digit is a length.
//! A line with a degree sign keeps `′` and `″` as minutes and seconds of arc.

use std::borrow::Cow;

use crate::format::format_number;
use crate::jsre::{self, Lazy, js};
use crate::types::{LineResult, Value, ValueKind};

/// The line with every foot or inch mark spelled `'` or `"`: `6′2″`, `10''` and `6'2` read as `6'2"`, `10"`; and a
/// mixed number of inches (`5 3/8"`, `6'5 3/8"`, `2-7/16 in`) as one amount, `(5+3/8) in`.
pub fn foot_marks(text: &str) -> Cow<'_, str> {
    let marked = marks(text);
    static MIXED: Lazy = Lazy::new(|| {
        js(
            r#"(?:(')\s*|(?<![\d./]))(\d+)[ -](\d+)/(\d+)\s*(?:"|(?:in|inch|inches)\b)"#,
            "",
        )
    });
    if !jsre::test(&MIXED, &marked) {
        return marked;
    }
    let out = jsre::replace_all(&MIXED, &marked, |c| {
        // after feet, `6'5 3/8"` adds: `6' + (5+3/8) in`
        let feet = if jsre::group(c, 1).is_empty() {
            ""
        } else {
            "' + "
        };
        format!(
            "{feet}({}+{}/{}) in",
            jsre::group(c, 2),
            jsre::group(c, 3),
            jsre::group(c, 4)
        )
    })
    .into_owned();
    Cow::Owned(out)
}

/// Inches with a fraction were typed (`3/8"`, `5 3/8"`), so the answer is in fractions of an inch too: the finest
/// denominator typed, and at least sixteenths.
fn typed_inch_fraction(line: &str) -> Option<f64> {
    static FRACTION: Lazy = Lazy::new(|| js(r#"\d+/(\d+)\)?\s*(?:"|(?:in|inch|inches)\b)"#, ""));
    let mut finest: Option<f64> = None;
    for c in FRACTION.captures_iter(line).flatten() {
        let d: f64 = c.get(1)?.as_str().parse().ok()?;
        finest = Some(finest.map_or(d, |f: f64| f.max(d)));
    }
    finest.map(|d| d.clamp(16.0, 64.0))
}

/// `7.8125` as `7 13/16`, to the nearest `1/denominator`; `≈` in front when that rounded it.
pub fn inch_fraction(inches: f64, denominator: f64) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let ticks = (inches.abs() * denominator).round();
    let close = (ticks - inches.abs() * denominator).abs() < 1e-6;
    let whole = (ticks / denominator).floor();
    let mut num = ticks - whole * denominator;
    let mut den = denominator;
    while num > 0.0 && num % 2.0 == 0.0 && den % 2.0 == 0.0 {
        num /= 2.0;
        den /= 2.0;
    }
    let about = if close { "" } else { "≈ " };
    match (whole == 0.0, num == 0.0) {
        (_, true) => format!("{about}{sign}{whole}"),
        (true, false) => format!("{about}{sign}{num}/{den}"),
        (false, false) => format!("{about}{sign}{whole} {num}/{den}"),
    }
}

/// Feet and fractional inches: `6 ft 2 3/8 in`, or inches alone (`7 13/16 in`) when `split` is false.
fn feet_and_fraction(inches: f64, denominator: f64, split: bool) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let ticks = (inches.abs() * denominator).round();
    let about = if (ticks - inches.abs() * denominator).abs() < 1e-6 {
        ""
    } else {
        "≈ "
    };
    let feet = if split {
        (ticks / (12.0 * denominator)).floor()
    } else {
        0.0
    };
    let rest = inch_fraction(
        (ticks - feet * 12.0 * denominator) / denominator,
        denominator,
    );
    match (feet == 0.0, rest == "0") {
        (true, _) => format!("{about}{sign}{rest} in"),
        (false, true) => format!("{about}{sign}{feet} ft"),
        (false, false) => format!("{about}{sign}{feet} ft {rest} in"),
    }
}

fn marks(text: &str) -> Cow<'_, str> {
    if !text.contains(['\'', '′', '″']) {
        return Cow::Borrowed(text);
    }
    let arc = text.contains('°');
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 1);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let after_digit = i > 0 && chars[i - 1].is_ascii_digit();
        let foot = c == '\'' || (c == '′' && !arc);
        if after_digit && foot && matches!(chars.get(i + 1), Some('\'' | '′')) {
            // `10''`: two marks are inches
            out.push('"');
            i += 2;
            continue;
        }
        if after_digit && c == '″' && !arc {
            out.push('"');
            i += 1;
            continue;
        }
        if after_digit && foot {
            out.push('\'');
            i += 1;
            // `6'2`: the number straight after a foot mark is inches, when nothing else follows it
            let start = i;
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            if j > start && chars[start].is_ascii_digit() {
                let next = chars[j..].iter().find(|c| !c.is_whitespace());
                let ends = next.is_none_or(|c| "+-−*/×÷),;=]".contains(*c));
                let marked = matches!(chars.get(j), Some('"' | '″' | '\'' | '′'));
                if ends && !marked {
                    out.extend(&chars[start..j]);
                    out.push('"');
                    i = j;
                }
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    if out == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(out)
    }
}

/// Whether the line typed both a length in feet and one in inches, without asking for a unit of its own.
fn mixes_feet_and_inches(line: &str) -> bool {
    static FEET: Lazy = Lazy::new(|| js(r#"\d\)?\s*(?:'|(?:ft|foot|feet)\b)"#, "i"));
    static INCHES: Lazy = Lazy::new(|| js(r#"\d\)?\s*(?:"|(?:in|inch|inches)\b)"#, "i"));
    jsre::test(&FEET, line) && jsre::test(&INCHES, line) && !converts(line)
}

fn unit_id(value: &Value) -> Option<&str> {
    if value.kind != ValueKind::Number || !value.n.is_finite() || value.complex.is_some() {
        return None;
    }
    value
        .unit_id
        .as_deref()
        .filter(|u| *u == "ft" || *u == "in")
}

fn inches_of(value: &Value) -> Option<f64> {
    match unit_id(value)? {
        "ft" => Some(value.n * 12.0),
        _ => Some(value.n),
    }
}

/// `74 in` as `6 ft 2 in`; the inches keep the answer's significant figures.
pub fn feet_and_inches(inches: f64, sig_figs: f64) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let total: f64 = format_number(inches.abs(), sig_figs)
        .parse()
        .unwrap_or(inches.abs());
    let mut feet = (total / 12.0).floor();
    let mut rest: f64 = format_number(total - feet * 12.0, sig_figs)
        .parse()
        .unwrap_or(0.0);
    if rest >= 12.0 {
        feet += 1.0;
        rest = 0.0;
    }
    let rest_text = format_number(rest, sig_figs);
    match (feet == 0.0, rest == 0.0) {
        (true, _) => format!("{sign}{rest_text} in"),
        (false, true) => format!("{sign}{} ft", format_number(feet, sig_figs)),
        (false, false) => format!("{sign}{} ft {rest_text} in", format_number(feet, sig_figs)),
    }
}

/// A line that mixed feet and inches answers in both (`6'2" + 1'11"` is `8 ft 1 in`).
pub fn answer_in_feet_and_inches(line: &str, result: &mut LineResult, sig_figs: f64) {
    if result.meas.is_some() {
        return;
    }
    let Some(inches) = result.value.as_ref().and_then(inches_of) else {
        return;
    };
    let mixed = mixes_feet_and_inches(line);
    // `5 3/8" + 2 7/16"` is `7 13/16 in`
    if let Some(denominator) = typed_inch_fraction(line)
        && !converts(line)
        && (mixed || result.value.as_ref().and_then(unit_id) == Some("in"))
    {
        result.display = feet_and_fraction(inches, denominator, mixed);
        return;
    }
    if mixed {
        result.display = feet_and_inches(inches, sig_figs);
    }
}

fn converts(line: &str) -> bool {
    static CONVERTS: Lazy = Lazy::new(|| js(r"\b(?:to|into|as)\b|→|->|\bin\s+[A-Za-z]", "i"));
    // `(5+3/8) in` is inches, not a conversion
    jsre::test(&CONVERTS, &line.replace(") in", ")\""))
}

/// Native only: ⌥↑ / ⌥↓ on a length in feet or inches: inches, feet and inches, then feet. `shown` is the answer
/// as the bar shows it now. None at either end, or for any other unit.
pub fn step_feet_inches(
    value: &Value,
    shown: &str,
    dir: i32,
    sig_figs: f64,
) -> Option<(Value, String)> {
    let inches = inches_of(value)?;
    static SPLIT: Lazy = Lazy::new(|| js(r"\d\s*ft\s+[\d./ ]+\s*in$", ""));
    let at = if jsre::test(&SPLIT, shown.trim()) {
        1
    } else if unit_id(value)? == "in" {
        0
    } else {
        2
    };
    let to = at + dir.signum();
    if !(0..=2).contains(&to) {
        return None;
    }
    let unit = |id: &str, n: f64| Value {
        n,
        unit: Some(id.into()),
        unit_id: Some(id.into()),
        ..value.clone()
    };
    Some(match to {
        0 => (
            unit("in", inches),
            format!("{} in", format_number(inches, sig_figs)),
        ),
        1 => (unit("ft", inches / 12.0), feet_and_inches(inches, sig_figs)),
        _ => {
            let feet = inches / 12.0;
            (
                unit("ft", feet),
                format!("{} ft", format_number(feet, sig_figs)),
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks() {
        assert_eq!(foot_marks("6′2″"), "6'2\"");
        assert_eq!(foot_marks("10''"), "10\"");
        assert_eq!(foot_marks("10′′"), "10\"");
        assert_eq!(foot_marks("6'2"), "6'2\"");
        assert_eq!(foot_marks("6'2 + 1'3"), "6'2\" + 1'3\"");
        assert_eq!(foot_marks("6'2\""), "6'2\"");
        assert_eq!(foot_marks("6'2 m"), "6'2 m");
        assert_eq!(foot_marks("3'*2"), "3'*2");
        // primes stay primes
        assert_eq!(foot_marks("f'(2)"), "f'(2)");
        assert_eq!(foot_marks("y'' + y"), "y'' + y");
        assert_eq!(foot_marks("(x^2)'"), "(x^2)'");
        // minutes and seconds of arc
        assert_eq!(foot_marks("30°15′20″"), "30°15′20″");
    }

    #[test]
    fn splits() {
        assert_eq!(feet_and_inches(74.0, 12.0), "6 ft 2 in");
        assert_eq!(feet_and_inches(72.0, 12.0), "6 ft");
        assert_eq!(feet_and_inches(5.5, 12.0), "5.5 in");
        assert_eq!(feet_and_inches(-74.0, 12.0), "-6 ft 2 in");
        assert_eq!(feet_and_inches(11.999_999_999_999, 6.0), "1 ft");
        assert_eq!(feet_and_fraction(7.8125, 16.0, false), "7 13/16 in");
        assert_eq!(feet_and_fraction(74.375, 16.0, true), "6 ft 2 3/8 in");
        assert_eq!(feet_and_fraction(0.1, 16.0, false), "≈ 1/8 in");
        assert_eq!(feet_and_fraction(72.0, 16.0, true), "6 ft");
        assert_eq!(foot_marks("5 3/8\""), "(5+3/8) in");
        assert_eq!(foot_marks("6'5 3/8\""), "6' + (5+3/8) in");
        assert_eq!(foot_marks("2-7/16 in"), "(2+7/16) in");
        assert_eq!(foot_marks("1/2 + 3/8\""), "1/2 + 3/8\"");
    }
}
