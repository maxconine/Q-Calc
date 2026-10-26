//! JS string semantics the app logic needs: lengths, indices and slices count UTF-16 code units, as `String.length`,
//! `slice` and `RegExp` match indices do in the TS; `trim` with JS's whitespace set; `Number(text)`; the length of
//! `JSON.stringify(value)`. Spans the UI gets back are in UTF-16 units too, which is what `NSString` and `NSRange`
//! count on the Mac. (Moved here from the FFI crate's interim `jsstr.rs`, which now re-exports it.)

use serde_json::Value as Json;

/// JS `WhiteSpace` and `LineTerminator` (what `String.prototype.trim` and `\s` take). Unlike `char::is_whitespace`
/// it includes U+FEFF and leaves out U+0085.
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `s.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// `s.trimEnd()`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_space)
}

/// `s.trimStart()`.
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_js_space)
}

/// `s.length`.
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The UTF-16 index of byte offset `byte` (which must sit on a char boundary).
pub fn index16(s: &str, byte: usize) -> usize {
    len16(&s[..byte.min(s.len())])
}

/// `s.slice(start, end)` for non-negative indices (clamped like JS). A cut through a surrogate pair leaves a lone
/// surrogate in JS; Rust strings can't hold one, so it becomes U+FFFD (only for astral characters at the cut).
pub fn slice16(s: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let end = end.min(units.len());
    let start = start.min(end);
    String::from_utf16_lossy(&units[start..end])
}

/// `s.slice(0, n)`.
pub fn take16(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// `s.slice(n)`.
pub fn skip16(s: &str, n: usize) -> String {
    slice16(s, n, usize::MAX)
}

/// `Number(text)`: decimal and e-notation, `Infinity`, `0x`/`0o`/`0b` integers, and '' as 0; None for NaN.
pub fn number(text: &str) -> Option<f64> {
    let t = trim(text);
    if t.is_empty() {
        return Some(0.0);
    }
    match t {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = t.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return None;
            }
            // exact up to 2^53, then rounded like JS (digit by digit is close enough for history displays)
            return Some(digits.chars().fold(0.0, |acc, c| {
                acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            }));
        }
    }
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exp) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = digits(int) && digits(frac) && !(int.is_empty() && frac.is_empty());
    let exp_ok = exp.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    if !mantissa_ok || !exp_ok {
        return None;
    }
    t.parse::<f64>().ok()
}

/// `JSON.stringify(value).length`: numbers as JS writes them, strings escaped as JS escapes them, in UTF-16 units.
pub fn json_len(v: &Json) -> usize {
    match v {
        Json::Null => 4,
        Json::Bool(b) => {
            if *b {
                4
            } else {
                5
            }
        }
        Json::Number(n) => n.as_f64().map_or(4, |x| {
            if x.is_finite() {
                crate::jsnum::to_string(x).len()
            } else {
                4
            }
        }),
        Json::String(s) => string_len(s),
        Json::Array(a) => 2 + a.iter().map(json_len).sum::<usize>() + a.len().saturating_sub(1),
        // undefined fields are left out of the objects before they get here, as JSON.stringify drops them
        Json::Object(o) => {
            2 + o
                .iter()
                .map(|(k, v)| string_len(k) + 1 + json_len(v))
                .sum::<usize>()
                + o.len().saturating_sub(1)
        }
    }
}

fn string_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\u{08}' | '\u{0C}' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf16(),
        })
        .sum::<usize>()
}

/// A JS replacement string for a match at bytes `start..end` of `subject` with no capture groups: `$$` is `$`,
/// `$&` the match, `` $` `` the text before it and `$'` the text after; any other `$` stays as written.
pub fn expand_replacement(template: &str, subject: &str, start: usize, end: usize) -> String {
    if !template.contains('$') {
        return template.to_string();
    }
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

/// `new Date(ms).toISOString()`.
pub fn iso_date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// JS truthiness (`Boolean(v)`) of a JSON value; a missing one is `undefined`, so false.
pub fn truthy(v: Option<&Json>) -> bool {
    match v {
        None | Some(Json::Null) => false,
        Some(Json::Bool(b)) => *b,
        Some(Json::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Json::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_like_js() {
        assert_eq!(number(" 12 "), Some(12.0));
        assert_eq!(number(""), Some(0.0));
        assert_eq!(number("1e3"), Some(1000.0));
        assert_eq!(number(".5"), Some(0.5));
        assert_eq!(number("5."), Some(5.0));
        assert_eq!(number("0x1F"), Some(31.0));
        assert_eq!(number("-0x1F"), None);
        assert_eq!(number("1,000"), None);
        assert_eq!(number("inf"), None);
        assert_eq!(number("NaN"), None);
        assert_eq!(number("."), None);
        assert_eq!(number("5 cm"), None);
        assert_eq!(number("-Infinity"), Some(f64::NEG_INFINITY));
    }

    #[test]
    fn json_lengths_like_js() {
        let v = json!([{"id": "a\"b", "n": 1.0, "x": 0.1, "s": "π…\n", "ok": true}]);
        assert_eq!(
            json_len(&v),
            r#"[{"id":"a\"b","n":1,"x":0.1,"s":"π…\n","ok":true}]"#
                .encode_utf16()
                .count()
        );
        assert_eq!(json_len(&json!("\u{1}")), 8);
    }

    #[test]
    fn utf16_slices() {
        assert_eq!(len16("a😀"), 3);
        assert_eq!(take16("abc", 2), "ab");
        assert_eq!(take16("a😀", 2), "a\u{FFFD}");
        assert_eq!(slice16("a👋b", 3, 4), "b");
        assert_eq!(index16("a👋b", 5), 3);
        assert_eq!(trim("\u{FEFF} x \u{A0}"), "x");
        assert_eq!(expand_replacement("$$5 $&", "ans", 0, 3), "$5 ans");
        assert_eq!(iso_date(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_date(1_790_287_960_000), "2026-09-24T22:12:40.000Z");
        assert_eq!(iso_date(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(iso_date(951_782_400_123), "2000-02-29T00:00:00.123Z");
    }
}
