//! Carrying on from the last answer (`src/lib/chain.ts`): an operator typed first continues from `ans`, like
//! pressing × after = on a calculator, and the tape writes the answer out.

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

/// Whether `q` continues from the last answer. A leading minus never does: `-3` and `- 3` are both negative three.
pub fn chains_from_answer(q: &str, last: Option<&ChainAnswer>) -> bool {
    static CHAIN_OP: Lazy = Lazy::new(|| js(r"^\s*[*×·/÷^+!]", ""));
    static CHAIN_CONVERT: Lazy = Lazy::new(|| js(r"^\s*(?:in|to)\s+\S", "i"));
    let Some(last) = last.filter(|l| !l.plain.is_empty()) else {
        return false;
    };
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
    if starts_convert(rest) {
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
    jsre::replace_all(&ANS, expr, |c| {
        let m = c.get(0).map_or(0..0, |m| m.range());
        jstr::expand_replacement(&among, expr, m.start, m.end)
    })
    .into_owned()
}
