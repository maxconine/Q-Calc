//! JS string rules the text ports need: `\s` and `String.prototype.trim` use JS's whitespace set, which differs
//! from Rust's `char::is_whitespace` (JS counts U+FEFF and not U+0085).

/// Whether `c` is JS whitespace or a line terminator (what `\s` matches and `trim` strips).
pub fn is_space(c: char) -> bool {
    c == '\u{feff}' || (c != '\u{85}' && c.is_whitespace())
}

/// `s.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `s.trimStart()`.
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

/// `s.trimEnd()`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// The `\s` class for a fancy-regex pattern, in JS's set (use inside `[...]`).
pub const SPACE_CLASS: &str = r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

/// A TS regex source with `\s` and `\S` (outside brackets) spelled as JS's whitespace set, for `jsre::js`.
pub fn spaces(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('s') if !in_class => out.push_str(&format!("[{SPACE_CLASS}]")),
                Some('S') if !in_class => out.push_str(&format!("[^{SPACE_CLASS}]")),
                Some('s') => out.push_str(SPACE_CLASS),
                Some(e) => {
                    out.push('\\');
                    out.push(e);
                }
                None => out.push('\\'),
            },
            '[' if !in_class => {
                in_class = true;
                out.push(c);
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_like_js() {
        assert_eq!(trim("\u{feff} a \u{a0}"), "a");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
        assert_eq!(trim_start("  a "), "a ");
        assert_eq!(trim_end("  a "), "  a");
        assert!(is_space('\u{3000}'));
    }
}
