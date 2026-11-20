//! Carrying on from the last answer (`src/lib/chain.ts`): an operator typed first continues from `ans`, like
//! pressing × after = on a calculator, and the tape writes the answer out.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::app::answer::answer_among;
use crate::app::js as jstr;
use crate::evaluate::parse_function_def;
use crate::jsre::{self, Lazy, js};

/// The last answer as it inserts, and whether it has a unit (only a unit answer chains into `in` / `to`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainAnswer {
    pub plain: String,
    pub unit: bool,
}

/// native only: a date as the bar answers one (`Wed 11 Nov 2026`), which `+ 1 week` continues from.
pub fn is_date_answer(plain: &str) -> bool {
    static DATE: Lazy = Lazy::new(|| {
        js(
            r"^(?:Sun|Mon|Tue|Wed|Thu|Fri|Sat) \d{1,2} [A-Z][a-z]{2} -?\d{1,6}$",
            "",
        )
    });
    jsre::test(&DATE, plain)
}

/// Whether `q` continues from the last answer. A leading minus never does: `-3` and `- 3` are both negative three.
pub fn chains_from_answer(q: &str, last: Option<&ChainAnswer>) -> bool {
    static CHAIN_OP: Lazy = Lazy::new(|| js(r"^\s*[*×·/÷^+!]", ""));
    static CHAIN_CONVERT: Lazy = Lazy::new(|| js(r"^\s*(?:in|to)\s+\S", "i"));
    // native only: a date goes on by a span, back as well as forward; `- 2 days` steps back, `-2` is still minus two
    static DATE_STEP: Lazy = Lazy::new(|| {
        js(
            r"^\s*[+\-−]\s*\(?\s*(?:\d+(?:\.\d+)?|an?|one)\s*[A-Za-z]",
            "",
        )
    });
    let Some(last) = last.filter(|l| !l.plain.is_empty()) else {
        return false;
    };
    if is_date_answer(&last.plain) {
        return jsre::test(&DATE_STEP, q);
    }
    if jsre::test(&CHAIN_OP, q) {
        return true;
    }
    last.unit && jsre::test(&CHAIN_CONVERT, q)
}

fn starts_convert(q: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*(?:in|to)\b", "i"));
    jsre::test(&RE, q)
}

/// The line evaluated: through the engine's own `ans`, which keeps full precision and the unit.
pub fn chained_expr(q: &str) -> String {
    if starts_convert(q) {
        format!("ans {}", jstr::trim_start(q))
    } else {
        format!("ans{q}")
    }
}

/// What the tape keeps: the answer written out, so the row still reads right once `ans` moves on.
pub fn chained_history_expr(q: &str, plain: &str) -> String {
    static PLAIN_UNSIGNED: Lazy = Lazy::new(|| js(r"^(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$", ""));
    static LOOSE: Lazy = Lazy::new(|| js(r"^[+\-−]", ""));
    let rest = jstr::trim_start(q);
    if starts_convert(rest) || is_date_answer(plain) {
        return format!("{plain} {rest}");
    }
    let base = if jsre::test(&LOOSE, rest) || jsre::test(&PLAIN_UNSIGNED, plain) {
        plain.to_string()
    } else {
        format!("({plain})")
    };
    format!("{base}{rest}")
}

/// A typed `ans` is written out in the tape too; a function body keeps it, since it reads ans when called.
pub fn ans_written_out(expr: &str, plain: &str) -> String {
    static ANS: Lazy = Lazy::new(|| js(r"\bans\b", "i"));
    static ALONE: Lazy = Lazy::new(|| js(r"^\s*ans\s*$", "i"));
    if !jsre::test(&ANS, expr) || parse_function_def(jstr::trim(expr)).is_some() {
        return expr.to_string();
    }
    let among = answer_among(plain, jsre::test(&ALONE, expr));
    // a replacement string, as JS reads one: `$$`, `$&`, `` $` `` and `$'` are patterns (a `$` answer can hold them)
    static NUMBER: Lazy = Lazy::new(|| js(r"^(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$", "i"));
    let number = jsre::test(&NUMBER, jstr::trim(&among));
    jsre::replace_all(&ANS, expr, |c| {
        let m = c.get(0).map_or(0..0, |m| m.range());
        // native only: a number against a number reads as one (`3 ans` was `3 4`); it keeps a bracket, `3 (4)`
        let before = expr[..m.start].trim_end().chars().last();
        let after = expr[m.end..].trim_start().chars().next();
        let glued = before.is_some_and(|b| b.is_ascii_alphanumeric() || b == '.' || b == ')')
            || after.is_some_and(|a| a.is_ascii_digit() || a == '.' || a == '(');
        if number && glued {
            return format!("({})", jstr::trim(&among));
        }
        jstr::expand_replacement(&among, expr, m.start, m.end)
    })
    .into_owned()
}

/// native only: `2ans` is 2·ans, as `2x` is 2·x; the word gets its space so every reader of `ans` sees it. A name
/// that runs on into it (`x2ans`) is left alone. A constant or a letter before it (`pi ans`, `x ans`) gets a `*`,
/// since the engine reads `ans` after a name as a call.
pub fn spaced_ans(q: &str) -> Cow<'_, str> {
    static GLUED: Lazy = Lazy::new(|| js(r"(?<![A-Za-z_\d.])(\d+(?:\.\d+)?|\.\d+)(ans)\b", "i"));
    static AFTER_NAME: Lazy =
        Lazy::new(|| js(r"(?<![A-Za-z_\d.])(pi|tau|phi|e|[A-Za-z])\s+(ans)\b", "i"));
    if !q.to_ascii_lowercase().contains("ans") {
        return Cow::Borrowed(q);
    }
    let spaced = jsre::replace_all(&GLUED, q, |c| format!("{} {}", jsre::group(c, 1), jsre::group(c, 2)));
    Cow::Owned(
        jsre::replace_all(&AFTER_NAME, &spaced, |c| format!("{}*{}", jsre::group(c, 1), jsre::group(c, 2)))
            .into_owned(),
    )
}

/// native only: whether a line uses `ans` by name.
pub fn mentions_ans(expr: &str) -> bool {
    static ANS: Lazy = Lazy::new(|| js(r"\bans\b", "i"));
    jsre::test(&ANS, expr)
}

/// native only: the preview's `ans`, as the number it stands for in a chip (`anschip`, macros.typ) marked "ans",
/// as Desmos draws it. A function body keeps the word, as in the tape.
pub fn ans_chipped(expr: &str, plain: &str) -> String {
    static ANS: Lazy = Lazy::new(|| js(r"\bans\b", "i"));
    static GROUPED: Lazy = Lazy::new(|| js(r"(?<=\d),(?=\d{3}(?!\d))", ""));
    if plain.is_empty() || !jsre::test(&ANS, expr) || parse_function_def(jstr::trim(expr)).is_some() {
        return expr.to_string();
    }
    // `3,000` would read as two arguments
    let value = jsre::replace_all(&GROUPED, jstr::trim(plain), |_| String::new()).into_owned();
    jsre::replace_all(&ANS, expr, |_| format!("anschip({value})")).into_owned()
}

