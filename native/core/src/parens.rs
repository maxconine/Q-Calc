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

/// Commits inferred parens when the caret is at the end with no selection (caret in chars).
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
    }
}
