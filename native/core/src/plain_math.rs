//! `src/engine/plainMath.ts`: one typed line to a value through units first, then the scientific layer, with
//! thousands separators, `× 10^n`, pasted spaces and missing parens normalised on the way.

use std::cell::Cell;

use crate::jsnum;
use crate::jsre::{self, Lazy};
use crate::parens::fill_parens;
use crate::scientific::{ScientificContext, eval_scientific, preprocess::rewrite_typeset_mul};
use crate::types::{DefaultUnits, Value};
use crate::{latex, precise, typst_input, units};

thread_local! {
    // the "spaces group thousands" setting, for the sheet being worked out; off unless evaluate_sheet turns it on
    static SPACE_THOUSANDS: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` with the space-thousands setting on or off, restoring it after (`withSpaceThousands`).
pub fn with_space_thousands<T>(on: bool, f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SPACE_THOUSANDS.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(SPACE_THOUSANDS.with(|s| s.replace(on)));
    f()
}

fn js(p: &str, f: &str) -> jsre::Regex {
    jsre::js(p, f)
}

/// `what is 40% of 90?` becomes `40% of 90`.
fn unwrap_question(text: &str) -> String {
    static TRAILING: Lazy = Lazy::new(|| js(r"\s*\?+\s*$", ""));
    static LEADING: Lazy = Lazy::new(|| {
        js(
            r"^(?:what(?:'s|\s+is|s)|how\s+much\s+is|calculate|compute)\s+",
            "i",
        )
    });
    let src = text.trim();
    if src.is_empty() {
        return String::new();
    }
    let bare = jsre::replace_all(&TRAILING, src, |_| String::new());
    let bare = jsre::replace_all(&LEADING, &bare, |_| String::new())
        .trim()
        .to_string();
    if bare.is_empty() {
        src.to_string()
    } else {
        bare
    }
}

fn has_nlp_words(s: &str) -> bool {
    static NLP: Lazy = Lazy::new(|| {
        js(
            r"\b(of|off|from|today|tomorrow|yesterday|tax|tip|people|nights|was|until|between|per|earnings|lunch|miles|weeks?|days?|hours?|months?)\b",
            "i",
        )
    });
    static TEXT: Lazy = Lazy::new(|| js(r"\\text\{([^}]*)\}", ""));
    static MATHRM: Lazy = Lazy::new(|| js(r"\\mathrm\{([^}]*)\}", ""));
    static CMD: Lazy = Lazy::new(|| js(r"\\[a-zA-Z]+", ""));
    static IN: Lazy = Lazy::new(|| js(r"\bin\b", "i"));
    static NOT_IN: Lazy = Lazy::new(|| js(r"\bsin\b|\bmin\b|\binfinity\b", "i"));
    let t = jsre::replace_all(&TEXT, s, |c| format!(" {} ", jsre::group(c, 1))).into_owned();
    let t = jsre::replace_all(&MATHRM, &t, |c| format!(" {} ", jsre::group(c, 1))).into_owned();
    let t = jsre::replace_all(&CMD, &t, |_| " ".into()).into_owned();
    jsre::test(&NLP, &t) || jsre::test(&IN, &jsre::replace_all(&NOT_IN, &t, |_| String::new()))
}

const PASTED_SPACE: &str = "\u{00a0}\u{2009}\u{202f}";

fn is_digit(c: Option<&char>) -> bool {
    c.is_some_and(char::is_ascii_digit)
}

/// A typed space between two numbers multiplies them (`3 4` is 12), outside a matrix, where it separates entries.
fn multiply_spaced_numbers(s: &str) -> String {
    static ARGUMENT: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z])(?:sinh|cosh|tanh|csch|sech|coth|sin|cos|tan|csc|sec|cot|ln|log)\s*(?:\^\s*[-−]?|_)\(?\d*\.?\d*\)?$",
            "i",
        )
    });
    static WHOLE_END: Lazy = Lazy::new(|| js(r"(?<![\d.])\d+$", ""));
    static FRACTION_START: Lazy = Lazy::new(|| js(r"^\d+[/⁄]\d", ""));
    static POINT_END: Lazy = Lazy::new(|| js(r"\d\.$", ""));
    let cs: Vec<char> = s.chars().collect();
    let mut depth = 0usize;
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        let ch = cs[i];
        if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            depth = depth.saturating_sub(1);
        }
        let prev = if i > 0 { cs.get(i - 1) } else { None };
        if ch == ' ' && depth == 0 && prev.is_some_and(|p| p.is_ascii_digit() || *p == '.') {
            let mut k = i;
            while cs.get(k) == Some(&' ') {
                k += 1;
            }
            let before: String = cs[..i].iter().collect();
            let after: String = cs[k..].iter().collect();
            // not after a function's own power or base (`sin^2 30`, `log_2 8`), and not a mixed number (`2 1/2`)
            let argument = jsre::test(&ARGUMENT, &before);
            let mixed = jsre::test(&WHOLE_END, &before) && jsre::test(&FRACTION_START, &after);
            if !argument && !mixed && (is_digit(prev) || jsre::test(&POINT_END, &before)) {
                let next_starts_number =
                    is_digit(cs.get(k)) || (cs.get(k) == Some(&'.') && is_digit(cs.get(k + 1)));
                if next_starts_number {
                    out.push('*');
                    i = k;
                    continue;
                }
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// `1,000,000` becomes `1000000`; commas inside a call's arguments and lists stay. `0xff`, `0b1010` and `0o17` are
/// whole numbers. A pasted thin or no-break space groups thousands; a typed one only with the setting on.
fn strip_thousands(s: &str) -> String {
    static COMMAS: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z_][A-Za-z0-9_]*\([^()]*|\[[^\][]*)\b\d{1,3}(?:,\d{3})+\b(?!,?\d)",
            "",
        )
    });
    static PASTED: Lazy = Lazy::new(|| {
        js(
            &format!(r"(?<![\w.,])\d{{1,3}}(?:[{PASTED_SPACE}]\d{{3}})+(?!\d)"),
            "",
        )
    });
    static RADIX: Lazy =
        Lazy::new(|| js(r"(?<![\w.])0(?:x[0-9a-f]+|b[01]+|o[0-7]+)(?![\w.])", "i"));
    static TYPED: Lazy = Lazy::new(|| js(r"(?<![\w.,])\d{1,3}(?: \d{3})+(?!\d)", ""));
    let joined = jsre::replace_all(&COMMAS, s, |c| jsre::group(c, 0).replace(',', "")).into_owned();
    let joined = jsre::replace_all(&PASTED, &joined, |c| {
        jsre::group(c, 0)
            .chars()
            .filter(|ch| !PASTED_SPACE.contains(*ch))
            .collect()
    })
    .into_owned();
    let joined = jsre::replace_all(&RADIX, &joined, |c| {
        let m = jsre::group(c, 0);
        let lower = m.to_ascii_lowercase();
        let radix = match &lower[1..2] {
            "x" => 16,
            "b" => 2,
            _ => 8,
        };
        match u128::from_str_radix(&lower[2..], radix) {
            Ok(v) if (v as f64) <= 9_007_199_254_740_991.0 => jsnum::to_string(v as f64),
            _ => m.to_string(),
        }
    })
    .into_owned();
    let grouped = if SPACE_THOUSANDS.with(Cell::get) {
        jsre::replace_all(&TYPED, &joined, |c| jsre::group(c, 0).replace(' ', "")).into_owned()
    } else {
        joined
    };
    multiply_spaced_numbers(&grouped)
}

const SUPERSCRIPT: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹";

/// `3.00 × 10^8 m/s` becomes `3.00e8 m/s`, so the unit reads; only where nothing binds tighter on either side.
fn fold_powers_of_ten(s: &str) -> String {
    static RE: Lazy = Lazy::new(|| {
        js(
            r"(?<=(?:^|[-+*(,=−])\s*)(?<!\^\s*[-+−]\s*)(?<![\d.][eE][-+−])(\d+(?:\.\d*)?|\.\d+)\s*[×*·⋅]\s*10\s*(?:\^\s*(?:\(\s*([-−]?\d+)\s*\)|([-−]?\d+))|([⁻]?[⁰¹²³⁴⁵⁶⁷⁸⁹]+))(?![\d.^!(%⁰¹²³⁴⁵⁶⁷⁸⁹⁻])",
            "",
        )
    });
    jsre::replace_all(&RE, s, |c| {
        let power = if c.get(2).is_some() {
            jsre::group(c, 2).to_string()
        } else if c.get(3).is_some() {
            jsre::group(c, 3).to_string()
        } else {
            jsre::group(c, 4)
                .chars()
                .map(|ch| {
                    if ch == '⁻' {
                        "-".to_string()
                    } else {
                        SUPERSCRIPT
                            .chars()
                            .position(|d| d == ch)
                            .unwrap_or(0)
                            .to_string()
                    }
                })
                .collect()
        };
        format!("{}e{}", jsre::group(c, 1), power.replacen('−', "-", 1))
    })
    .into_owned()
}

/// Typst and LaTeX to ascii, typeset operators, thousands separators and missing parens.
pub fn normalize_math_text(text: &str) -> String {
    let typst = if typst_input::looks_like_typst(text) {
        typst_input::typst_to_ascii(text)
    } else {
        text.to_string()
    };
    let ascii = if latex::looks_like_latex(&typst) {
        latex::latex_to_ascii(&typst)
    } else {
        typst
    };
    fill_parens(&fold_powers_of_ten(&strip_thousands(&rewrite_typeset_mul(
        &ascii,
    ))))
}

/// Everything `tryPlainMath` reads besides the text.
#[derive(Clone, Debug, Default)]
pub struct PlainContext {
    pub sci: ScientificContext,
    pub default_units: DefaultUnits,
    /// Keep the units as typed (`F = 10 kN` stores kN, not its lbf counterpart).
    pub keep_units: bool,
}

/// One line's value through the unit reader, then the scientific layer with truthful digits; None is blank.
pub fn try_plain_math(text: &str, ctx: &PlainContext) -> Option<Value> {
    static ANS_WORD: Lazy = Lazy::new(|| js(r"\bans\b", "i"));
    static LEADING_ANS: Lazy = Lazy::new(|| js(r"^ans\s*[A-Za-zµμ°Ω]", "i"));
    static ANS_START: Lazy = Lazy::new(|| js(r"^ans", "i"));
    static PCT_OF: Lazy = Lazy::new(|| js(r"\d+(?:\.\d+)?\s*%\s*of\b", "i"));
    static OF: Lazy = Lazy::new(|| js(r"\s*of\b", "i"));
    let src = unwrap_question(text);
    if src.is_empty() {
        return None;
    }
    let filled = normalize_math_text(&src);
    let sci = &ctx.sci;
    // a known variable is never read as a unit symbol; a plain previous answer takes a unit the way a variable does
    let has_ans = sci.ans.is_some() && jsre::test(&ANS_WORD, &filled);
    let mut vars = sci.variables.clone();
    if has_ans && let Some(a) = sci.ans {
        vars.insert("ans".into(), a);
    }
    let leading = (has_ans && jsre::test(&LEADING_ANS, &filled)).then(|| {
        jsre::replace_all(&ANS_START, &filled, |_| {
            jsnum::to_string(sci.ans.unwrap_or_default())
        })
        .into_owned()
    });
    let first = if has_ans {
        jsre::replace_all(&ANS_WORD, &filled, |_| "ans".into()).into_owned()
    } else {
        filled.clone()
    };
    let converted = units::try_convert(
        &first,
        &ctx.default_units,
        &vars,
        ctx.keep_units,
        sci.angle_mode,
    )
    .or_else(|| {
        leading.as_ref().and_then(|l| {
            units::try_convert(
                l,
                &ctx.default_units,
                &sci.variables,
                ctx.keep_units,
                sci.angle_mode,
            )
        })
    });
    if converted.is_some() {
        return converted;
    }
    let cleaned = jsre::replace_all(&PCT_OF, &src, |c| {
        jsre::replace_all(&OF, jsre::group(c, 0), |_| String::new()).into_owned()
    });
    if has_nlp_words(&cleaned) && !latex::looks_like_latex(&src) {
        return None;
    }
    let value = eval_scientific(&filled, sci)?;
    Some(precise::truthful(&filled, value, sci))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(s: &str) -> String {
        try_plain_math(s, &PlainContext::default())
            .map(|v| crate::format::format_value(&v, 16.0))
            .unwrap_or_default()
    }

    #[test]
    fn normalises_typed_numbers() {
        assert_eq!(normalize_math_text("1,234,567 + 1"), "1234567 + 1");
        assert_eq!(normalize_math_text("max(1,200)"), "max(1,200)");
        assert_eq!(normalize_math_text("0xff+1"), "255+1");
        assert_eq!(normalize_math_text("3 4"), "3*4");
        assert_eq!(normalize_math_text("2 1/2"), "2 1/2");
        assert_eq!(normalize_math_text("3.00 × 10^8"), "3.00e8");
        assert_eq!(normalize_math_text("(2+3"), "(2+3)");
        assert_eq!(normalize_math_text("1\u{202f}000"), "1000");
        assert_eq!(
            with_space_thousands(true, || normalize_math_text("1 000 000")),
            "1000000"
        );
    }

    #[test]
    fn answers_with_true_digits() {
        assert_eq!(shown("0.1+0.2"), "0.3");
        assert_eq!(shown("what is 2+2?"), "4");
        assert_eq!(shown("1,000 * 3"), "3000");
        assert_eq!(shown("2 days"), "2 d");
    }
}
