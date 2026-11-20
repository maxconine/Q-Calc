//! Native only (QA item 56): small engineering shorthands, tried only after the web paths left a line blank.
//!
//! - `deg(pi)` is 180 and `rad(180)` is π: the angle converted, as a plain number.
//! - `10 Ω || 20 Ω` is the parallel combination `1/(1/a + 1/b)`, lowest precedence inside its brackets.
//! - `mol` counts in a line with other units: `8.314 J/(mol K) * 300 K` is 2494.2 J/mol. It isn't in the unit
//!   table (the web's goldens keep `1 mol` blank and its unit dimensions at seven), so the line is worked out with
//!   mol standing in for 1, 1.001 and 1.002, and the power of mol read from how the answer scales.
//! - `60 Hz 100 µF` multiplies, but only between circuit quantities (`5 ft 10 kg` and `1 h 30 min` stay as they were).
//! - (QA item 62) a conductance these paths answer reads in siemens (`0.006 S`, not `0.006 1/Ω`; the web's own
//!   `1/6.8 ohm` keeps `1/Ω`, its goldens pin it), and `2 siemens` is read.
//! - (QA item 62) `tesla`, `mT`, `gauss` and `Wb` are read, and `T` is the tesla in a magnetic line: beside an
//!   area only (`2 T * 3 m^2` is 6 Wb), or with a magnetic unit or target (`… in Wb`). Elsewhere `T` stays the
//!   ton the web's goldens pin (`2 T * 3 A * 1 m` is `6000 kg m A`).

use crate::jsre::{self, Lazy};
use crate::units::table::Dim;

/// A whole word of letters (µ too), not a call: the magnetic and tesla words are looked for in these.
static WORD: Lazy = Lazy::new(|| jsre::js(r"(?<![A-Za-z0-9_])[A-Za-zµμ]+(?![A-Za-z0-9_(])", ""));

/// The line rewritten with the shorthands spelled out, or None when it has none.
pub fn rewrite(text: &str) -> Option<String> {
    let mut s = angle_calls(text);
    s = magnetic_words(&siemens_words(&s));
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
/// `carried`: the mol came in through `ans` or a variable (`2 ans` after `1 mol`), so it may stand alone.
pub fn with_moles(
    text: &str,
    carried: bool,
    eval: impl Fn(&str) -> Option<crate::types::Value>,
) -> Option<crate::types::Value> {
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
    static LETTERS: Lazy = Lazy::new(|| jsre::js(r"[A-Za-zµμΩ°]+", ""));
    let without = jsre::replace_all(&MOL, text, |_| " ".into()).into_owned();
    let other_unit = LETTERS
        .find_iter(without.as_str())
        .filter_map(Result::ok)
        .any(|w| crate::units::unit_dim(w.as_str()).is_some());
    if !other_unit && !carried {
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

/// `name = rhs` (a plain name, one `=`): the name and the right side.
pub fn assignment(text: &str) -> Option<(String, String)> {
    let (name, rhs) = text.split_once('=')?;
    let name = name.trim();
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_alphabetic() || first == '_')
        || !chars.all(ident_char)
        || name == "ans"
        || rhs.contains('=')
        || rhs.trim().is_empty()
    {
        return None;
    }
    Some((name.to_string(), rhs.trim().to_string()))
}

/// A moles answer as text the line reader takes back (`1 mol`, `2.4942 kJ/mol`, `24.6 L/mol`), for `ans` and
/// variables; None when its label won't read back.
pub fn mol_quantity(value: &crate::types::Value) -> Option<String> {
    let unit = value
        .unit
        .as_deref()?
        .replace('·', " ")
        .replace('²', "^2")
        .replace('³', "^3");
    Some(format!("{} {unit}", crate::jsnum::to_string(value.n)))
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

/// `siemens` → `(1/ohm)` (with a prefix, `msiemens` → `(1/kohm)`).
fn siemens_words(text: &str) -> String {
    static SIEMENS: Lazy =
        Lazy::new(|| jsre::js(r"(?<![A-Za-z0-9_])(k|m|µ|μ|u)?siemens(?![A-Za-z0-9_(])", ""));
    jsre::replace_all(&SIEMENS, text, |c| reciprocal_ohm(jsre::group(c, 1))).into_owned()
}

/// A siemens prefix as the reciprocal ohm the table has: `` → `(1/ohm)`, `m` → `(1/kohm)`, `k` → `(1/mohm)`,
/// `µ` → `(1/Mohm)`.
fn reciprocal_ohm(prefix: &str) -> String {
    let ohm = match prefix {
        "k" => "mohm",
        "m" => "kohm",
        "" => "ohm",
        _ => "Mohm",
    };
    format!("(1/{ohm})")
}

/// The size of a magnetic word in teslas (or webers for `Wb`), and whether it's a flux.
fn magnetic_size(word: &str) -> Option<(f64, bool)> {
    let (scale, rest) = match word {
        "gauss" | "G" => return Some((1e-4, false)),
        _ => match word.chars().next()? {
            'k' if word.len() > 1 => (1e3, &word[1..]),
            'm' if word.len() > 1 => (1e-3, &word[1..]),
            'u' if word.len() > 1 => (1e-6, &word[1..]),
            c @ ('µ' | 'μ') => (1e-6, &word[c.len_utf8()..]),
            'n' if word.len() > 1 => (1e-9, &word[1..]),
            _ => (1.0, word),
        },
    };
    match rest {
        "tesla" | "teslas" | "T" => Some((scale, false)),
        "Wb" | "weber" | "webers" => Some((scale, true)),
        _ => None,
    }
}

fn tesla_text(size: f64, flux: bool) -> String {
    let n = crate::jsnum::to_string(size);
    if flux {
        format!("({n} kg m^2/(A s^2))")
    } else {
        format!("({n} kg/(A s^2))")
    }
}

/// `tesla`, `mT`… in words the table doesn't have: `tesla`, `Wb`, `weber`, `gauss` and their prefixes (a bare
/// `T` or `mT` waits for [`magnetic`]).
fn magnetic_words(text: &str) -> String {
    jsre::replace_all(&WORD, text, |c| {
        let w = jsre::group(c, 0);
        match magnetic_size(w) {
            Some((size, flux)) if !w.ends_with('T') && w != "G" => tesla_text(size, flux),
            _ => w.to_string(),
        }
    })
    .into_owned()
}

/// A trailing `in Wb` / `to mT` / `in gauss`: the line before it, the label and its size in T or Wb.
pub fn magnetic_target(text: &str) -> Option<(String, &str, f64)> {
    let (head, word) = trailing_target(text)?;
    let (size, _) = magnetic_size(word)?;
    let label = match word {
        "tesla" | "teslas" => "T",
        "weber" | "webers" => "Wb",
        _ => word,
    };
    Some((head.to_string(), label, size))
}

/// A trailing `in <word>` / `to <word>`: the line before it (not empty) and the word.
fn trailing_target(text: &str) -> Option<(&str, &str)> {
    let (head, word) = text.trim_end().rsplit_once(' ')?;
    let head = head.trim_end();
    let head = head
        .strip_suffix(" in")
        .or_else(|| head.strip_suffix(" to"))?
        .trim_end();
    (!head.is_empty()).then_some((head, word))
}

/// A line where a capital `T` is the tesla, with it (and any other magnetic word) spelled in base units: `T`
/// beside nothing but areas (`2 T * 3 m^2`), or in a line that also has a magnetic unit or (`target`, already
/// taken off) a magnetic target. None
/// otherwise, so `2 T * 3 A * 1 m` stays tons (a web golden).
pub fn magnetic(text: &str, target: bool) -> Option<String> {
    let words: Vec<(usize, usize, String)> = WORD
        .find_iter(text)
        .filter_map(Result::ok)
        .map(|m| (m.start(), m.end(), m.as_str().to_string()))
        .collect();
    let is_t = |w: &str| matches!(magnetic_size(w), Some((_, false))) && w.ends_with('T');
    if !target && !words.iter().any(|(_, _, w)| is_t(w)) {
        return None;
    }
    // an area: a length unit squared
    let area = |end: usize, w: &str| {
        let after = &text[end..];
        let squared = after.starts_with("^2") || after.starts_with('²');
        squared && crate::units::unit_dim(w) == Some(Dim::Length)
    };
    let mut magnetic_word = false;
    let mut only_areas = true;
    for (_, end, w) in &words {
        // a prefixed tesla (`mT`, `µT`, `nT`) is magnetic on its own: milli-tons aren't typed (QA 70)
        if tesla_label(w).is_some() {
            magnetic_word = true;
            continue;
        }
        if is_t(w) || matches!(w.as_str(), "in" | "to") {
            continue;
        }
        if magnetic_size(w).is_some() {
            magnetic_word = true;
        } else if crate::units::unit_dim(w).is_some() && !area(*end, w) {
            only_areas = false;
        } else if crate::units::unit_dim(w).is_none() {
            // a name, a function or anything else: not ours to guess
            return None;
        }
    }
    let has_area = words.iter().any(|(_, end, w)| area(*end, w));
    if !(target || magnetic_word || (only_areas && has_area)) {
        return None;
    }
    let mut out = String::new();
    let mut at = 0;
    for (start, end, w) in &words {
        out.push_str(&text[at..*start]);
        let before = text[..*start].trim_end();
        let in_target = before.ends_with(" in") || before.ends_with(" to");
        match magnetic_size(w) {
            Some((size, flux)) if !in_target && w != "G" => out.push_str(&tesla_text(size, flux)),
            _ => out.push_str(w),
        }
        at = *end;
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// The label a magnetic answer reads in: `N / m A` is T, `N m / A` Wb.
pub fn magnetic_unit(unit: &str) -> Option<&'static str> {
    match unit {
        "N / m A" => Some("T"),
        "N m / A" => Some("Wb"),
        _ => None,
    }
}

/// A prefixed tesla symbol as its answer label: `mT`, `µT` (`uT`, `μT`), `nT`; None for `T`, `kT` (a knot) and
/// anything else.
/// With its size in T.
pub fn tesla_label(word: &str) -> Option<(&'static str, f64)> {
    match word {
        "mT" => Some(("mT", 1e-3)),
        "µT" | "μT" | "uT" => Some(("µT", 1e-6)),
        "nT" => Some(("nT", 1e-9)),
        _ => None,
    }
}

/// The one prefixed tesla a line is typed in (`5 mT + 1 mT` → mT), the label its field answer reads in; None
/// when there is none or more than one.
pub fn tesla_in(text: &str) -> Option<(&'static str, f64)> {
    let mut labels = WORD
        .find_iter(text)
        .filter_map(Result::ok)
        .filter_map(|m| tesla_label(m.as_str()));
    let first = labels.next()?;
    labels.all(|l| l == first).then_some(first)
}

/// The size of a siemens label in S: `S` 1, `mS` 0.001, `µS`, `kS`.
pub fn siemens_size(label: &str) -> Option<f64> {
    match label {
        "S" | "siemens" => Some(1.0),
        "mS" => Some(1e-3),
        "µS" | "μS" | "uS" => Some(1e-6),
        "kS" => Some(1e3),
        _ => None,
    }
}

/// `2 mS`, `5 µS`, `3 kS`: a prefixed siemens symbol spelled as the reciprocal ohm the table has (`(1/kohm)`);
/// None when the line has none. A bare `S` stays the second (web goldens), and so does `ms`.
pub fn siemens_symbols(text: &str) -> Option<String> {
    static SYMBOL: Lazy =
        Lazy::new(|| jsre::js(r"(?<![A-Za-z0-9_])(m|µ|μ|u|k)S(?![A-Za-z0-9_(])", ""));
    if !jsre::test(&SYMBOL, text) {
        return None;
    }
    Some(jsre::replace_all(&SYMBOL, text, |c| reciprocal_ohm(jsre::group(c, 1))).into_owned())
}

/// A trailing `in S` / `to mS` / `in siemens`: the line before it, the label and its size in S.
pub fn siemens_target(text: &str) -> Option<(String, &'static str, f64)> {
    let (head, word) = trailing_target(text)?;
    let size = siemens_size(word)?;
    let label = match word {
        "mS" => "mS",
        "kS" => "kS",
        "µS" | "μS" | "uS" => "µS",
        _ => "S",
    };
    Some((head.to_string(), label, size))
}

/// A conductance label as siemens: `1/Ω` → S, `1/kΩ` → mS, `1/MΩ` → µS, `1/mΩ` → kS.
pub fn siemens_unit(unit: &str) -> Option<&'static str> {
    match unit {
        "1/Ω" => Some("S"),
        "1/kΩ" => Some("mS"),
        "1/MΩ" => Some("µS"),
        "1/mΩ" => Some("kS"),
        _ => None,
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
        assert_eq!(rewrite("2 siemens").as_deref(), Some("2 (1/ohm)"));
        assert_eq!(rewrite("2 tesla").as_deref(), Some("2 (1 kg/(A s^2))"));
        assert_eq!(
            magnetic("2 T * 3 m^2", false).as_deref(),
            Some("2 (1 kg/(A s^2)) * 3 m^2")
        );
        assert_eq!(magnetic("2 T * 3 A * 1 m", false), None);
        assert_eq!(magnetic("2 T", false), None);
        assert_eq!(magnetic("2 T", true).as_deref(), Some("2 (1 kg/(A s^2))"));
        assert_eq!(
            magnetic("2 T * 3 mWb", false).as_deref(),
            Some("2 (1 kg/(A s^2)) * 3 (0.001 kg m^2/(A s^2))")
        );
        assert_eq!(siemens_unit("1/kΩ"), Some("mS"));
        assert_eq!(
            magnetic_target("2 tesla * 3 m^2 in mWb"),
            Some(("2 tesla * 3 m^2".into(), "mWb", 1e-3))
        );
        assert_eq!(mol_label("J", -1), "J/mol");
        assert_eq!(mol_label("J/K", -1), "J/(mol·K)");
        assert_eq!(mol_label("N m / K", -1), "J/(mol·K)");
        assert_eq!(mol_label("", 1), "mol");
        assert_eq!(mol_label("g/L", 1), "g·mol/L");
    }
}
