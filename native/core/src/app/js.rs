//! JS string semantics the app logic needs: lengths, indices and slices count UTF-16 code units, as `String.length`,
//! `slice` and `RegExp` match indices do in the TS. Spans the UI gets back are in these units too, which is what
//! `NSString` and `NSRange` count on the Mac.

/// `s.length`.
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The UTF-16 index of byte offset `byte` (which must sit on a char boundary).
pub fn index16(s: &str, byte: usize) -> usize {
    len16(&s[..byte.min(s.len())])
}

/// The byte offset of UTF-16 index `i`, clamped to the string; an index inside a surrogate pair rounds up.
pub fn byte_of(s: &str, i: usize) -> usize {
    let mut units = 0;
    for (b, c) in s.char_indices() {
        if units >= i {
            return b;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// `s.slice(start, end)` for non-negative indices (clamped like JS).
pub fn slice16(s: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let end = end.min(units.len());
    let start = start.min(end);
    String::from_utf16_lossy(&units[start..end])
}

/// `s.slice(0, n)`.
pub fn take16(s: &str, n: usize) -> String {
    slice16(s, 0, n)
}

/// `s.slice(n)`.
pub fn skip16(s: &str, n: usize) -> String {
    slice16(s, n, usize::MAX)
}

/// JS `\s`: Unicode white space and U+FEFF, but not U+0085, which Rust counts and JS doesn't.
pub fn is_space(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

/// `s.trimEnd()`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `s.trimStart()`.
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

/// `s.trim()`.
pub fn trim(s: &str) -> &str {
    trim_start(trim_end(s))
}

/// The UTF-16 unit at index `i` as a char, when it is a whole BMP char (what `s[i]` compares against).
pub fn char_at16(s: &str, i: usize) -> Option<char> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let u = *units.get(i)?;
    char::from_u32(u32::from(u))
}

/// A JS replacement string for a match at bytes `start..end` of `subject` with no capture groups: `$$` is `$`,
/// `$&` the match, `` $` `` the text before it and `$'` the text after; any other `$` stays as written.
pub fn expand_replacement(template: &str, subject: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('$') => out.push('$'),
            Some('&') => out.push_str(&subject[start..end]),
            Some('`') => out.push_str(&subject[..start]),
            Some('\'') => out.push_str(&subject[end..]),
            _ => {
                out.push('$');
                continue;
            }
        }
        chars.next();
    }
    out
}

/// `Number(text)`: blank is 0, hex/octal/binary literals and `Infinity` read, anything else unreadable is NaN.
pub fn number(text: &str) -> f64 {
    let t = trim(text);
    if t.is_empty() {
        return 0.0;
    }
    let radix = |digits: &str, r: u32| {
        if digits.is_empty() {
            return f64::NAN;
        }
        digits
            .chars()
            .try_fold(0.0_f64, |acc, c| {
                c.to_digit(r).map(|d| acc * f64::from(r) + f64::from(d))
            })
            .unwrap_or(f64::NAN)
    };
    let lower = t.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        return radix(h, 16);
    }
    if let Some(o) = lower.strip_prefix("0o") {
        return radix(o, 8);
    }
    if let Some(b) = lower.strip_prefix("0b") {
        return radix(b, 2);
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    let ok = body
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && body
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.');
    if !ok {
        return f64::NAN;
    }
    body.parse::<f64>().map_or(f64::NAN, |v| sign * v)
}

/// `JSON.stringify(v)` as JS writes it: numbers in JS form (`5`, not `5.0`), no spaces.
pub fn stringify(v: &serde_json::Value) -> String {
    use serde_json::Value as J;
    match v {
        J::Null => "null".into(),
        J::Bool(b) => b.to_string(),
        J::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) if f.is_finite() => crate::jsnum::to_string(f),
            _ => "null".into(),
        },
        J::String(s) => serde_json::to_string(s).unwrap_or_default(),
        J::Array(a) => format!(
            "[{}]",
            a.iter().map(stringify).collect::<Vec<_>>().join(",")
        ),
        J::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!(
                    "{}:{}",
                    serde_json::to_string(k).unwrap_or_default(),
                    stringify(v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_json_like_js() {
        assert_eq!(number(" 0x10 "), 16.0);
        assert_eq!(number(""), 0.0);
        assert!(number("12px").is_nan());
        assert!(number("inf").is_nan());
        assert_eq!(number("-Infinity"), f64::NEG_INFINITY);
        assert_eq!(
            stringify(&serde_json::json!({"n": 5.0, "s": "a"})),
            r#"{"n":5,"s":"a"}"#
        );
    }

    #[test]
    fn counts_like_js() {
        assert_eq!(len16("a👋"), 3);
        assert_eq!(slice16("a👋b", 3, 4), "b");
        assert_eq!(index16("a👋b", 5), 3);
        assert_eq!(byte_of("a👋b", 3), 5);
        assert_eq!(trim(" \u{feff}x "), "x");
    }
}
