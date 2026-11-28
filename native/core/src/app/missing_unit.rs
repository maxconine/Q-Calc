//! `src/lib/missingUnit.ts`: a sum whose terms don't agree because one number was owed a unit.

use crate::jsre::{self, Lazy};

/// The fix offered: where the unit goes, the text put there, the filled line and the hint.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UnitFix {
    /// UTF-16 index, as the TS reports it.
    pub at: usize,
    pub text: String,
    pub filled: String,
    pub hint: String,
}

// one SI unit per kind of quantity, so at most one of them can make a sum's terms agree
const SI_UNITS: [&str; 14] = [
    "V", "A", "ohm", "W", "J", "N", "Pa", "C", "F", "H", "Hz", "m", "s", "kg",
];

/// The characters [`missing_unit`] may evaluate in all (`work.ts` counts): each try is the whole line again, 14
/// per bare numerator, so `791 / 791 / …` twenty long took 0.9 s. A typed line spends a few hundred. No such
/// limit in the TS.
const BUDGET: usize = 20_000;
/// And the engine work its tries may do (`work::spend` units), for lines each try of which is slow: 42 tries of
/// `solve for R in 1/Rft√bob = 1/R1 + 1/R2/R2/…` took 2.6 s.
const WORK: u64 = 2_000_000;

/// `10 mA + 5 / 1 kohm`: the number over a quantity that one SI unit would fix, and that unit. `ok` says whether
/// a piece of text has a real answer. A guess to offer, never one to act on unasked.
pub fn missing_unit(expr: &str, ok: &dyn Fn(&str) -> bool) -> Option<UnitFix> {
    // every try shares one line's simplify budget: 280 tries of `d/dx 2/dx 2/dx … 2 as`, 0.2 s each, ran for a
    // minute
    crate::symbolic::simplify::with_line_budget(|| tries(expr, ok))
}

fn tries(expr: &str, ok: &dyn Fn(&str) -> bool) -> Option<UnitFix> {
    static BARE_NUMERATOR: Lazy = Lazy::new(|| {
        jsre::js(
            r"(?<![A-Za-z_.\d^])\d+(?:\.\d+)?(?:e[+-]?\d+)?(?=\s*\/)",
            "i",
        )
    });
    static QUOTED_UNIT: Lazy = Lazy::new(|| jsre::js(r#"\d\s*"[A-Za-zΩµμ°]"#, ""));
    let quoted = jsre::test(&QUOTED_UNIT, expr);
    let mut budget = BUDGET;
    let work = crate::work::WorkBudget::new(WORK);
    // native only: a bare number added to a quantity (`6.3489 in + (-5.8221)`, `1 hour + 30`) is offered the
    // line's own units first, in the order they appear
    if let Some(fix) = added_term(expr, quoted, ok, &mut budget, &work) {
        return Some(fix);
    }
    for m in BARE_NUMERATOR.find_iter(expr).filter_map(Result::ok) {
        let at = m.end();
        for unit in SI_UNITS {
            let text = if quoted {
                format!(" \"{unit}\"")
            } else {
                format!(" {unit}")
            };
            let filled = format!("{}{text}{}", &expr[..at], &expr[at..]);
            budget = budget.checked_sub(filled.len())?;
            if work.over() {
                return None;
            }
            if ok(&filled) {
                let n = m.as_str();
                return Some(UnitFix {
                    at: expr[..at].encode_utf16().count(),
                    text,
                    filled,
                    hint: format!("{n} has no unit · ↵ again makes it {n} {unit}"),
                });
            }
        }
    }
    None
}

/// Native only: a number that is a term of a sum with no unit of its own, tried with each unit the line already
/// uses (`161.262 mm - 5.8221` offers mm). None when no unit of the line makes it read.
fn added_term(
    expr: &str,
    quoted: bool,
    ok: &dyn Fn(&str) -> bool,
    budget: &mut usize,
    work: &crate::work::WorkBudget,
) -> Option<UnitFix> {
    // a number with no unit, power, digit or call after it
    static BARE: Lazy = Lazy::new(|| {
        jsre::js(
            r#"(?<![A-Za-z_.\d^])\d+(?:\.\d+)?(?:e[+-]?\d+)?(?![\d.]|\s*[A-Za-z_°µμΩ'"%(\[^/*×·÷!])"#,
            "i",
        )
    });
    // the line's units: a word right after a number (`5 mm`, `2.5in`, `3 °C`)
    static UNIT: Lazy = Lazy::new(|| jsre::js(r"\d\s*(°?[A-Za-zµμΩ]+)", ""));
    let mut units: Vec<&str> = Vec::new();
    for c in UNIT.captures_iter(expr).filter_map(Result::ok) {
        let u = jsre::group(&c, 1);
        if !units.contains(&u) {
            units.push(u);
        }
    }
    // a sum, or a list of quantities (`min(1 ft, 2)`)
    if units.is_empty() || !expr.contains(['+', '-', '−', ',']) {
        return None;
    }
    for m in BARE.find_iter(expr).filter_map(Result::ok) {
        let at = m.end();
        for unit in &units {
            let text = if quoted {
                format!(" \"{unit}\"")
            } else {
                format!(" {unit}")
            };
            let filled = format!("{}{text}{}", &expr[..at], &expr[at..]);
            *budget = budget.checked_sub(filled.len())?;
            if work.over() {
                return None;
            }
            if ok(&filled) {
                let n = m.as_str();
                return Some(UnitFix {
                    at: expr[..at].encode_utf16().count(),
                    text,
                    filled,
                    hint: format!("{n} has no unit · ↵ again makes it {n} {unit}"),
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvaluateOptions;

    fn ok(text: &str) -> bool {
        let shown = crate::evaluate_line(text, &EvaluateOptions::default()).display;
        !shown.is_empty() && !crate::units::is_improper_unit_conversion(&shown)
    }

    // the recorded fixtures pass a test's own closure, so they can't be replayed; this is that test
    #[test]
    fn offers_the_unit_a_sum_was_owed() {
        for (text, filled) in [
            ("10 mA + 5 / 1 kohm", "10 mA + 5 V / 1 kohm"),
            ("5/1 kohm + 10 mA", "5 V/1 kohm + 10 mA"),
            ("10 mA + 5 / 2 kohm", "10 mA + 5 V / 2 kohm"),
            ("10 V - 3 / 1.5 A", "10 V - 3 W / 1.5 A"),
            // the quoted (Typst) spelling needs `typstToAscii`, which WP-NOTATION ports
        ] {
            assert!(!ok(text), "{text}");
            let fix = missing_unit(text, &ok);
            assert_eq!(
                fix.as_ref().map(|f| f.filled.as_str()),
                Some(filled),
                "{text}"
            );
        }
        // native only: a bare number added to a quantity takes the line's own unit
        for (text, filled) in [
            ("6.3489 in + (-5.8221)", "6.3489 in + (-5.8221 in)"),
            ("161.262mm - 5.8221", "161.262mm - 5.8221 mm"),
            ("3 + 5 m", "3 m + 5 m"),
            ("1 hour + 30", "1 hour + 30 hour"),
        ] {
            assert!(!ok(text), "{text}");
            assert_eq!(
                missing_unit(text, &ok).map(|f| f.filled),
                Some(filled.to_string()),
                "{text}"
            );
        }
        assert_eq!(
            missing_unit("6.3489 in + (-5.8221)", &ok).map(|f| f.hint),
            Some("5.8221 has no unit · ↵ again makes it 5.8221 in".to_string())
        );
        assert_eq!(
            missing_unit("10 mA + 5 / 1 kohm", &ok).map(|f| f.hint),
            Some("5 has no unit · ↵ again makes it 5 V".to_string())
        );
        for text in [
            "10 m + 2 * 5 s",
            "10 m to kg",
            "10 mA + 5 V / 1 kohm^2",
            "10 mA + 5 kohm",
            "2 kg + 3 m",
        ] {
            assert_eq!(missing_unit(text, &ok), None, "{text}");
        }
    }
}
