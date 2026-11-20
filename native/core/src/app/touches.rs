//! What ⌘⇧C copies (`src/lib/touches.ts`): the line and its answer as they read on screen, `√8 = 2√2 ≈ 2.828`.

use crate::app::answer::{AnswerForm, has_dual_answer, visible_answer};
use crate::app::js::{self as jstr, len16, skip16};

/// `lineCopyText(expr, { display, exact, solve }, form)`; `solve` is the solve's variable and outcome.
pub fn line_copy_text(
    expr: &str,
    display: &str,
    exact: Option<&str>,
    solve: Option<(&str, &str)>,
    form: AnswerForm,
) -> String {
    let line = jstr::trim(expr);
    let shown = match exact {
        Some(e) if !e.is_empty() && has_dual_answer(display, exact) => format!("{e} ≈ {display}"),
        _ => visible_answer(display, exact, form).to_string(),
    };
    let shown_trim = jstr::trim(&shown);
    if line.is_empty() || shown_trim.is_empty() {
        return shown_trim.to_string();
    }
    if let Some((variable, outcome)) = solve {
        return if outcome == "roots" {
            format!("{line}, {variable} = {shown}")
        } else {
            format!("{line}, {shown}")
        };
    }
    // `x = 5` already says its answer
    let after_eq = line
        .rfind('=')
        .map_or_else(|| line.to_string(), |i| skip16(line, len16(&line[..i]) + 1));
    if line.contains('=') && jstr::trim(&after_eq) == shown_trim {
        return line.to_string();
    }
    format!("{line} = {shown}")
}

/// What a copied line or answer finally is, as the web's `copyValue` makes it: nothing for "improper unit
/// conversion", and a balanced reaction in ASCII (`2H2 + O2 -> 2H2O`, `chemCopyText`).
pub fn copy_value(text: &str) -> String {
    if crate::units::is_improper_unit_conversion(text) {
        return String::new();
    }
    crate::chem::chem_copy_text(text)
}

/// native: what a click or ⌘C on the live answer copies (QuickCalc's `copyAnswer`): without its unit when
/// `unitless`, then `copy_value`. The Mac's copy had left a reaction's subscripts in and copied the unit error.
pub fn answer_copy(shown: &str, unitless: bool) -> String {
    if unitless {
        copy_value(&crate::app::answer::unitless_answer(shown))
    } else {
        copy_value(shown)
    }
}

/// native: what ⌘⇧C copies for the live line (QuickCalc's `copyLine`): the line as the tape keeps it (a chain's
/// answer written out before it, a typed `ans` as its number), its grey parens filled in, then `= answer` as
/// `line_copy_text` writes it (`√8 = 2√2 ≈ 2.828`, `x^2 = 9, x = -3, 3`); a reaction copies its answer alone. ""
/// for a blank answer or a `graph` line. The Mac had copied `text = shown` as typed (`+ 2 = 7`, `(1+2 = 3`).
pub fn live_line_copy(
    text: &str,
    chained: bool,
    ans_plain: Option<&str>,
    display: &str,
    exact: Option<&str>,
    solve: Option<(&str, &str)>,
    form: AnswerForm,
) -> String {
    // native only: the unit error is no answer to copy beside the line (the page copies `… = improper unit conversion`)
    if jstr::trim(display).is_empty()
        || crate::units::is_improper_unit_conversion(display)
        || crate::app::needs::is_graph_command(text)
    {
        return String::new();
    }
    let plain = ans_plain.filter(|p| !p.is_empty());
    let expr = match plain {
        Some(p) if chained => crate::app::chain::chained_history_expr(text, p),
        Some(p) => crate::app::chain::ans_written_out(text, p),
        None => text.to_string(),
    };
    if crate::chem::is_reaction_input(&expr) {
        return copy_value(display);
    }
    let p = crate::parens::infer_parens(&expr);
    let whole = format!(
        "{}{}{}",
        "(".repeat(p.leading),
        jstr::trim(&expr),
        ")".repeat(p.trailing)
    );
    copy_value(&line_copy_text(&whole, display, exact, solve, form))
}

/// native: ⌘⇧C on a tape row picked with ↑ (QuickCalc's `copyLine` for the selected row): a definition or
/// function its line, a note its text, a system its solution, else the row's line and answer by
/// `line_copy_text`. The Mac had copied the live line instead (nothing, with the line empty).
pub fn row_line_copy(row: &serde_json::Value, form: AnswerForm) -> String {
    let field = |k: &str| row.get(k).and_then(serde_json::Value::as_str).unwrap_or("");
    match field("kind") {
        "definition" | "function" | "note" => return field("expr").to_string(),
        "system" => return field("display").to_string(),
        _ => {}
    }
    let solve = row.get("solve").filter(|s| s.is_object()).map(|s| {
        let get = |k: &str| s.get(k).and_then(serde_json::Value::as_str).unwrap_or("");
        (get("variable"), get("outcome"))
    });
    let exact = row.get("exact").and_then(serde_json::Value::as_str);
    copy_value(&line_copy_text(
        field("expr"),
        field("display"),
        exact,
        solve,
        form,
    ))
}
