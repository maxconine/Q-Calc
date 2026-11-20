//! The grey inferred parens (`src/engine/parens.ts`): `5+3)*2` evaluates as `(5+3)*2`.

/// How many `(` to add in front and `)` behind, and the balanced text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParenFill {
    pub leading: usize,
    pub trailing: usize,
    pub filled: String,
}

/// Counts unmatched parens and the grey ( / ) needed to balance them.
pub fn infer_parens(expr: &str) -> ParenFill {
    let (mut depth, mut min_depth) = (0i64, 0i64);
    for ch in expr.chars() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            min_depth = min_depth.min(depth);
        }
    }
    let leading = (-min_depth) as usize;
    let trailing = (depth + leading as i64) as usize;
    ParenFill {
        leading,
        trailing,
        filled: format!("{}{expr}{}", "(".repeat(leading), ")".repeat(trailing)),
    }
}

pub fn fill_parens(expr: &str) -> String {
    infer_parens(expr).filled
}

/// native only (QA item 57): the grey parens typed in where they read. As `fill_parens`, but a Typst line written
/// between `$` signs (`$sqrt(2$`) gets its `)` inside the closing `$`, where the engine reads it (after it,
/// `$sqrt(2$)` was blank). Whitespace around the line is kept.
pub fn fill_typed(expr: &str) -> String {
    let body = expr.trim();
    if body.len() > 2
        && body.starts_with('$')
        && body.ends_with('$')
        && !body[1..body.len() - 1].contains('$')
    {
        let inner = &body[1..body.len() - 1];
        let fill = infer_parens(inner);
        if fill.leading + fill.trailing == 0 {
            return expr.to_string();
        }
        let at = expr.find('$').unwrap_or(0);
        let end = expr.rfind('$').unwrap_or(expr.len());
        return format!("{}${}${}", &expr[..at], fill.filled, &expr[end + 1..]);
    }
    let fill = infer_parens(expr);
    if fill.leading + fill.trailing == 0 {
        return expr.to_string();
    }
    fill.filled
}

/// Commits inferred parens when the caret is at the end with no selection (caret in UTF-16 units, as the field reports).
pub fn autofill_parens(expr: &str, caret: usize, selection_end: usize) -> Option<String> {
    if caret != selection_end || caret != expr.encode_utf16().count() {
        return None;
    }
    let fill = infer_parens(expr);
    (fill.leading > 0 || fill.trailing > 0).then_some(fill.filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balances_both_sides() {
        assert_eq!(fill_parens("5+3)*2"), "(5+3)*2");
        assert_eq!(fill_parens("sqrt(2"), "sqrt(2)");
        assert_eq!(fill_parens(")("), "()()");
        assert_eq!(autofill_parens("(2", 2, 2).as_deref(), Some("(2)"));
        assert_eq!(autofill_parens("(2", 1, 1), None);
        assert_eq!(fill_typed("$sqrt(2$"), "$sqrt(2)$");
        assert_eq!(fill_typed(" $frac(1, 2$ "), " $frac(1, 2)$ ");
        assert_eq!(fill_typed("2+3)*4"), "(2+3)*4");
        assert_eq!(fill_typed("$10 + (2"), "$10 + (2)");
        assert_eq!(fill_typed("5"), "5");
    }
}
