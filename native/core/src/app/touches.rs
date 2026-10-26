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
    let after_eq = line.rfind('=').map_or_else(|| line.to_string(), |i| skip16(line, len16(&line[..i]) + 1));
    if line.contains('=') && jstr::trim(&after_eq) == shown_trim {
        return line.to_string();
    }
    format!("{line} = {shown}")
}
