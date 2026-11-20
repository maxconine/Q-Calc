//! Native only (QA item 56): small engineering shorthands, tried only after the web paths left a line blank.
//!
//! - `deg(pi)` is 180 and `rad(180)` is π: the angle converted, as a plain number.
//! - `10 Ω || 20 Ω` is the parallel combination `1/(1/a + 1/b)`, lowest precedence inside its brackets.
//! - `mol` counts in a line with other units: `8.314 J/(mol K) * 300 K` is 2494.2 J/mol. It isn't in the unit
//!   table (the web's goldens keep `1 mol` blank and its unit dimensions at seven), so the line is worked out with
//!   mol standing in for 1, 1.001 and 1.002, and the power of mol read from how the answer scales.
//! - `60 Hz 100 µF` multiplies, but only between circuit quantities (`5 ft 10 kg` and `1 h 30 min` stay as they were).

use crate::units::table::Dim;

/// The line rewritten with the shorthands spelled out, or None when it has none.
pub fn rewrite(text: &str) -> Option<String> {
    let mut s = angle_calls(text);
    if s.contains("||") {
        s = parallel(&s)?;
    }
    s = circuit_products(&s);
    (s != text).then_some(s)
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `deg(x)` → `((x)*180/pi)`, `rad(x)` → `((x)*pi/180)`, nested calls too.
fn angle_calls(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let word_start = i == 0 || !ident_char(chars[i - 1]);
        let name = ["deg", "rad"].into_iter().find(|n| {
            let n: Vec<char> = n.chars().collect();
            word_start && chars[i..].starts_with(&n) && chars.get(i + 3) == Some(&'(')
        });
        let Some(name) = name else {
            out.push(chars[i]);
            i += 1;
            continue;
        };
        // the matching bracket
        let open = i + 3;
        let mut depth = 0;
        let mut close = None;
        for (j, c) in chars.iter().enumerate().skip(open) {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            out.extend(&chars[i..]);
            break;
        };
        let inner = angle_calls(&chars[open + 1..close].iter().collect::<String>());
        if inner.trim().is_empty() {
            out.extend(&chars[i..=close]);
        } else if name == "deg" {
            out.push_str(&format!("(({inner})*180/pi)"));
        } else {
            out.push_str(&format!("(({inner})*pi/180)"));
        }
        i = close + 1;
    }
    out
}

/// Every bracket group (and the whole line) with `||` at its own level becomes `1/(1/(a)+1/(b)…)`.
/// None when the brackets don't balance or a side is empty.
fn parallel(text: &str) -> Option<String> {
    // innermost first: a stack of groups being built, each a list of `||` parts
    let mut stack: Vec<(char, Vec<String>)> = vec![(' ', vec![String::new()])];
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' | '[' | '{' => stack.push((c, vec![String::new()])),
            ')' | ']' | '}' => {
                let (open, parts) = stack.pop()?;
                if stack.is_empty() || open != matching(c) {
                    return None;
                }
                let inner = combine(parts)?;
                let top = stack.last_mut()?.1.last_mut()?;
                top.push(open);
                top.push_str(&inner);
                top.push(c);
            }
            '|' if chars.peek() == Some(&'|') => {
                chars.next();
                stack.last_mut()?.1.push(String::new());
            }
            _ => stack.last_mut()?.1.last_mut()?.push(c),
        }
    }
    if stack.len() != 1 {
        return None;
    }
    combine(stack.pop()?.1)
}

fn matching(close: char) -> char {
    match close {
        ')' => '(',
        ']' => '[',
        _ => '{',
    }
}

fn combine(parts: Vec<String>) -> Option<String> {
    if parts.len() == 1 {
        return parts.into_iter().next();
    }
    if parts.iter().any(|p| p.trim().is_empty()) {
        return None;
    }
    let sum: Vec<String> = parts.iter().map(|p| format!("1/({})", p.trim())).collect();
    Some(format!("1/({})", sum.join(" + ")))
}

fn circuit(dim: Dim) -> bool {
    matches!(
        dim,
        Dim::Frequency
            | Dim::Capacitance
            | Dim::Inductance
            | Dim::Resistance
            | Dim::Current
            | Dim::Voltage
            | Dim::Charge
    )
}

/// `60 Hz 100 µF` → `60 Hz * 100 µF`: a number and unit right after another, both circuit quantities.
fn circuit_products(text: &str) -> String {
    use crate::jsre::{self, Lazy};
    // the next amount is only looked at, so a chain (`60 Hz 100 µF 10 Ω`) joins in one pass
    static PAIR: Lazy = Lazy::new(|| {
        jsre::js(
            r"(\d\.?\s*)([A-Za-zµμΩ]+)\s+(?=\d*\.?\d+(?:e[+-]?\d+)?\s*([A-Za-zµμΩ]+)(?![A-Za-z0-9_(]))",
            "",
        )
    });
    jsre::replace_all(&PAIR, text, |c| {
        let (a, b) = (jsre::group(c, 2), jsre::group(c, 3));
        match (crate::units::unit_dim(a), crate::units::unit_dim(b)) {
            (Some(x), Some(y)) if circuit(x) && circuit(y) => {
                format!("{}{} * ", jsre::group(c, 1), a)
            }
            _ => jsre::group(c, 0).to_string(),
        }
    })
    .into_owned()
}

/// A line with `mol` (or `kmol`, `mmol`, `moles`) beside other units, worked out by `eval` with mol as a number:
/// the answer and its unit with mol put back (mol stands in as 1, 1.001 and 1.002; the power is how it scales). None when mol is the only unit, or the answer isn't a power of mol.
pub fn with_moles(
    text: &str,
    eval: impl Fn(&str) -> Option<crate::types::Value>,
) -> Option<crate::types::Value> {
    use crate::jsre::{self, Lazy};
    static MOL: Lazy = Lazy::new(|| {
        jsre::js(
            r"(?<![A-Za-z0-9_])(k|m|µ|μ|u|n)?(?:mol|mols|mole|moles)(?![A-Za-z0-9_(])",
            "",
        )
    });
    if !jsre::test(&MOL, text) {
        return None;
    }
    let at = |k: f64| {
        let line = jsre::replace_all(&MOL, text, |c| {
            let scale = match jsre::group(c, 1) {
                "k" => 1e3,
                "m" => 1e-3,
                "µ" | "μ" | "u" => 1e-6,
                "n" => 1e-9,
                _ => 1.0,
            };
            format!("({})", crate::jsnum::to_string(k * scale))
        });
        eval(&line).filter(|v| {
            v.kind == crate::types::ValueKind::Number
                && v.complex.is_none()
                && v.meas.as_ref().is_none_or(|m| m.unc.is_none())
                && v.n.is_finite()
                && v.n != 0.0
        })
    };
    // steps close to 1, so the answer keeps its prefix (`kJ`) while mol scales
    let (one, two, three) = (at(1.0)?, at(1.001)?, at(1.002)?);
    // `2 mol` alone stays blank (web parity): another unit must be typed
    static WORD: Lazy = Lazy::new(|| jsre::js(r"[A-Za-zµμΩ°]+", ""));
    let without = jsre::replace_all(&MOL, text, |_| " ".into()).into_owned();
    let other_unit = WORD
        .find_iter(without.as_str())
        .filter_map(Result::ok)
        .any(|w| crate::units::unit_dim(w.as_str()).is_some());
    if !other_unit {
        return None;
    }
    let unit = one.unit.clone().unwrap_or_default();
    if two.unit.clone().unwrap_or_default() != unit
        || three.unit.clone().unwrap_or_default() != unit
    {
        return None;
    }
    let power = (two.n / one.n).ln() / 1.001f64.ln();
    let p = power.round();
    let close = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol * b.abs().max(1.0);
    if !close(power, p, 1e-6) || p.abs() > 3.0 || !close(three.n / one.n, 1.002f64.powf(p), 1e-9) {
        return None;
    }
    Some(crate::types::Value {
        unit: Some(mol_label(&unit, p as i32)),
        unit_id: None,
        ..one
    })
}

/// `unit` times mol to the power `p`: `J` and −1 → `J/mol`, `N m / K` and −1 → `N m/(mol·K)`, `g` and 1 → `g·mol`.
fn mol_label(unit: &str, p: i32) -> String {
    let mol = match p.abs() {
        1 => "mol".to_string(),
        2 => "mol²".to_string(),
        _ => "mol³".to_string(),
    };
    let (num, den) = match unit.split_once('/') {
        Some((n, d)) => (n.trim().to_string(), Some(d.trim().to_string())),
        None => (unit.trim().to_string(), None),
    };
    let num = if num == "1" { String::new() } else { num };
    let bare = |d: &str| {
        d.trim_start_matches('(')
            .trim_end_matches(')')
            .replace(' ', "·")
    };
    if p == 0 {
        return unit.to_string();
    }
    if p > 0 {
        let top = if num.is_empty() {
            mol
        } else {
            format!("{num}·{mol}")
        };
        return match den {
            Some(d) => format!("{top}/{d}"),
            None => top,
        };
    }
    // over mol, `N m` is an energy
    let top = match num.as_str() {
        "" => "1".to_string(),
        "N m" => "J".to_string(),
        _ => num,
    };
    match den {
        Some(d) => format!("{top}/({mol}·{})", bare(&d)),
        None => format!("{top}/{mol}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites() {
        assert_eq!(rewrite("deg(pi)").as_deref(), Some("((pi)*180/pi)"));
        assert_eq!(rewrite("10 || 20").as_deref(), Some("1/(1/(10) + 1/(20))"));
        assert_eq!(
            rewrite("5 + (10 || 20)").as_deref(),
            Some("5 + (1/(1/(10) + 1/(20)))")
        );
        assert_eq!(rewrite("60 Hz 100 µF").as_deref(), Some("60 Hz * 100 µF"));
        assert_eq!(
            rewrite("2 pi 60 Hz 100 µF 10 Ω").as_deref(),
            Some("2 pi 60 Hz * 100 µF * 10 Ω")
        );
        assert_eq!(rewrite("5 ft 10 kg"), None);
        assert_eq!(rewrite("degrees(1)"), None);
        assert_eq!(rewrite("10 ||"), None);
        assert_eq!(mol_label("J", -1), "J/mol");
        assert_eq!(mol_label("J/K", -1), "J/(mol·K)");
        assert_eq!(mol_label("N m / K", -1), "J/(mol·K)");
        assert_eq!(mol_label("", 1), "mol");
        assert_eq!(mol_label("g/L", 1), "g·mol/L");
    }
}
