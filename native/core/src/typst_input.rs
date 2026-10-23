//! `src/engine/typstInput.ts`: Typst math typed or pasted (`$frac(1, 2)$`, `root(3, 8)`) as ascii.
//! The detector is ported; PORT(WP-NOTATION): the rewriter (`emit`, `leibniz`, `tidy`).

use crate::jsre::{self, Lazy};

static MARK: Lazy = Lazy::new(|| {
    jsre::js(
        r#"^\$(?:\$[\s\S]+\$|[^$\n]+)\$\s*$|(?:^|[^A-Za-z])(?:frac|binom|root|attach|lr|overline|underline|limits|scripts|upright|bold|italic|bb|cal|frak)\s*\(|\b(?:plus\.minus|minus\.plus|plusminus|arrow\.[a-z]|dot\.(?:c|op|circle)|dots(?:\.[a-z]+)?|eq\.(?:not|triple)|gt\.eq|lt\.eq|times(?:\.circle)?)\b|\b(?:dot|ast)\b(?!\s*[(.])|\bdiv\b|\bdif\b|\bintegral_|\bbase\s*:"#,
        "",
    )
});
// words in quotes are Typst text in math; calculator input never quotes
static QUOTED_TEXT: Lazy = Lazy::new(|| jsre::js(r#"(?:^|[\s(_])"[A-Za-z][^"\n]*""#, ""));
static LATEX_CMD: Lazy = Lazy::new(|| jsre::js(r"\\[a-zA-Z]+", ""));

pub fn looks_like_typst(text: &str) -> bool {
    let s = text.trim();
    if s.is_empty() || jsre::test(&LATEX_CMD, s) {
        return false;
    }
    jsre::test(&MARK, s) || jsre::test(&QUOTED_TEXT, s)
}

/// Typst math as the ascii the rest of the engine evaluates; ordinary calculator text is returned unchanged.
pub fn typst_to_ascii(text: &str) -> String {
    text.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn detects_typst() {
        assert!(super::looks_like_typst("$frac(1, 2)$"));
        assert!(super::looks_like_typst("root(3, 8)"));
        assert!(!super::looks_like_typst("2+3"));
        assert!(!super::looks_like_typst(r"\frac{1}{2}"));
    }
}
