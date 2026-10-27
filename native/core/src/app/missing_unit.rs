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

/// `10 mA + 5 / 1 kohm`: the number over a quantity that one SI unit would fix, and that unit. `ok` says whether
/// a piece of text has a real answer. A guess to offer, never one to act on unasked.
pub fn missing_unit(expr: &str, ok: &dyn Fn(&str) -> bool) -> Option<UnitFix> {
    static BARE_NUMERATOR: Lazy = Lazy::new(|| {
        jsre::js(
            r"(?<![A-Za-z_.\d^])\d+(?:\.\d+)?(?:e[+-]?\d+)?(?=\s*\/)",
            "i",
        )
    });
    static QUOTED_UNIT: Lazy = Lazy::new(|| jsre::js(r#"\d\s*"[A-Za-zΩµμ°]"#, ""));
    let quoted = jsre::test(&QUOTED_UNIT, expr);
    for m in BARE_NUMERATOR.find_iter(expr).filter_map(Result::ok) {
        let at = m.end();
        for unit in SI_UNITS {
            let text = if quoted {
                format!(" \"{unit}\"")
            } else {
                format!(" {unit}")
            };
            let filled = format!("{}{text}{}", &expr[..at], &expr[at..]);
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
