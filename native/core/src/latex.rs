//! LaTeX input from `src/engine/plainMath.ts`. The detector is ported; PORT(WP-NOTATION): `latexToAscii` with its
//! helpers (`replaceBraced`, `convertSqrts`, `convertLogs`, `bareTrigArguments`, `repeatingDecimals`).

use crate::jsre::{self, Lazy};

pub fn looks_like_latex(s: &str) -> bool {
    static RE: Lazy = Lazy::new(|| jsre::js(r"\\[a-zA-Z%]+|[\^_]\{|\\frac|\\sqrt", ""));
    jsre::test(&RE, s)
}

/// LaTeX as the ascii the engine evaluates.
pub fn latex_to_ascii(latex: &str) -> String {
    latex.trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn detects_latex() {
        assert!(super::looks_like_latex(r"\frac{1}{2}"));
        assert!(super::looks_like_latex("x^{2}"));
        assert!(!super::looks_like_latex("x^2"));
    }
}
