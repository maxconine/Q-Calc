//! WP-SOLVE: `src/engine/unitSolve.ts`, one equation with units solved in SI with the unknown's unit read off
//! the others, and formulas with units rearranged with each quantity held as a letter.

use std::collections::BTreeMap;

use crate::format::format_value;
use crate::isolate::isolate;
use crate::jsre::{self, Lazy, js};
use crate::solve::{SolveContext, format_solve_full, solve_equation};
use crate::system::{infer_dims, prep_units};
use crate::types::{DefaultUnits, SolveInfo, SolveOutcome, Value, ValueKind};
use crate::units::{si_quantity, si_value};

/// An equation with units, solved in SI. `info` is None for a message (units that can't match).
#[derive(Clone, Debug)]
pub struct UnitSolved {
    pub display: String,
    pub info: Option<SolveInfo>,
    pub value: Option<Value>,
    pub message: bool,
}

/// What `solveWithUnitsOne` reads besides the equation (the TS `Ctx`).
#[derive(Debug)]
pub struct UnitCtx<'a> {
    pub solve: SolveContext<'a>,
    pub variable: Option<&'a str>,
    pub near: Option<&'a str>,
    /// Stored names, which are never units.
    pub names: &'a [String],
    pub defaults: &'a DefaultUnits,
    pub sig_figs: f64,
}

pub fn solve_with_units_one(equation: &str, ctx: &UnitCtx<'_>) -> Option<UnitSolved> {
    let mut unknowns: Vec<String> = ctx.variable.map(str::to_string).into_iter().collect();
    unknowns.extend(ctx.names.iter().cloned());
    let prep = prep_units(&[equation.to_string()], &unknowns)?;
    let plain = &prep.plain[0];
    let guess = ctx
        .near
        .map(|n| si_quantity(n).map_or_else(|| n.to_string(), |q| crate::jsnum::to_string(q.si)));
    let near = match (&guess, ctx.variable) {
        (Some(g), Some(_)) => format!(" near {g}"),
        _ => String::new(),
    };
    let text = match ctx.variable {
        Some(v) => format!("solve {plain} for {v}{near}"),
        None => plain.clone(),
    };
    let solved = solve_equation(&text, &ctx.solve)?;
    let info = solved.info.clone();
    if info.outcome != SolveOutcome::Roots {
        let d = format_solve_full(&solved, ctx.sig_figs, false);
        return Some(UnitSolved {
            display: d.display,
            info: Some(info),
            value: None,
            message: d.message,
        });
    }
    let Some(dims) = infer_dims(
        &prep.marked,
        &prep.quantities,
        std::slice::from_ref(&info.variable),
    ) else {
        return Some(UnitSolved {
            display: "units don't match".into(),
            info: None,
            value: None,
            message: true,
        });
    };
    let dim = dims
        .iter()
        .find(|(k, _)| *k == info.variable)
        .map(|(_, d)| d.clone())?;
    let plain_number = dim.iter().all(|x| *x == 0.0);
    let unit_id = prep
        .quantities
        .iter()
        .find(|q| q.unit_id.is_some() && q.dim == dim)
        .and_then(|q| q.unit_id.clone());
    let values: Vec<Value> = info
        .roots
        .iter()
        .map(|&root| {
            if plain_number {
                Some(Value::number(root))
            } else {
                si_value(root, &dim, unit_id.as_deref(), ctx.defaults)
            }
        })
        .map(|v| v.filter(|v| v.kind == ValueKind::Number))
        .collect::<Option<_>>()?;
    let texts: Vec<String> = values
        .iter()
        .map(|v| format_value(v, ctx.sig_figs))
        .collect();
    let more = info.more == Some(true);
    let r = &info.roots;
    let pair = values.len() == 2 && !more && r[0] < 0.0 && (r[0] + r[1]).abs() <= 1e-12 * r[1];
    let display = if pair {
        format!("±{}", texts[1])
    } else {
        texts.join(", ") + if more { ", …" } else { "" }
    };
    Some(UnitSolved {
        display,
        value: (values.len() == 1 && !more).then(|| values[0].clone()),
        info: Some(info),
        message: false,
    })
}

const SPARE_LETTERS: &str = "QWZYKJUVBDGHLNPRSTX";

/// A formula for `variable` with each quantity standing in as a letter while isolate rearranges.
pub fn isolate_with_units(
    equation: &str,
    variable: &str,
    variables: &BTreeMap<String, f64>,
) -> Option<String> {
    static LETTER: Lazy = Lazy::new(|| js(r"[A-Za-z]", ""));
    static MARK: Lazy = Lazy::new(|| js(r"\(__q(\d+)\)", ""));
    static UPPER: Lazy = Lazy::new(|| js(r"[A-Z]", ""));
    let mut names = vec![variable.to_string()];
    names.extend(variables.keys().cloned());
    let prep = prep_units(&[equation.to_string()], &names)?;
    if !clearly_units(&prep.texts) {
        return None;
    }
    let marked = &prep.marked[0];
    let used: Vec<char> = LETTER
        .find_iter(marked)
        .flatten()
        .filter_map(|m| m.as_str().chars().next())
        .collect();
    let spare: Vec<char> = SPARE_LETTERS
        .chars()
        .filter(|c| !used.contains(c) && c.to_string() != variable)
        .collect();
    if spare.len() < prep.texts.len() {
        return None;
    }
    let stand = &spare[..prep.texts.len()];
    let text = jsre::replace_all(&MARK, marked, |c| {
        let i: usize = jsre::group(c, 1).parse().unwrap_or(0);
        stand.get(i).map(char::to_string).unwrap_or_default()
    })
    .into_owned();
    let iso = isolate(&format!("isolate {variable} in {text}"), Some(variables))?;
    Some(
        jsre::replace_all(&UPPER, &iso.display, |c| {
            let ch = jsre::group(c, 0).chars().next().unwrap_or(' ');
            match stand.iter().position(|s| *s == ch) {
                Some(i) => format!("({})", prep.texts[i]),
                None => ch.to_string(),
            }
        })
        .into_owned(),
    )
}

/// Whether the quantities are unmistakably units: two letters or more, a compound, or a decimal amount.
pub fn clearly_units(texts: &[String]) -> bool {
    static SPLIT: Lazy = Lazy::new(|| js(r"(?s)^([\d.]+(?:e[+-]?\d+)?)\s*(.*)$", "i"));
    static COMPOUND: Lazy = Lazy::new(|| js(r"[/^·*]", ""));
    static TWO: Lazy = Lazy::new(|| js(r"[A-Za-zµμΩ°]{2}", ""));
    texts.iter().any(|t| {
        let Ok(Some(c)) = SPLIT.captures(t.trim()) else {
            return false;
        };
        let unit = jsre::group(&c, 2);
        jsre::group(&c, 1).contains('.') || jsre::test(&COMPOUND, unit) || jsre::test(&TWO, unit)
    })
}

/// The evaluate pipeline's check: clear units that won't rearrange stay blank.
pub fn clearly_units_blank(eq: &str, names: &[String]) -> bool {
    prep_units(&[eq.to_string()], names).is_some_and(|p| clearly_units(&p.texts))
}
