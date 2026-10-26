//! JS string semantics the history and chain ports need: lengths and slices in UTF-16 code units, `trim` with
//! JS's whitespace set, `Number(text)`, and the length of `JSON.stringify(value)`.

use serde_json::Value as Json;

/// JS `WhiteSpace` and `LineTerminator` (what `String.prototype.trim` removes). Unlike `char::is_whitespace`
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

pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_space)
}

pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_js_space)
}

/// `s.length`.
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.slice(0, end)` in UTF-16 units. A cut through a surrogate pair leaves a lone surrogate in JS; Rust strings
/// can't hold one, so it becomes U+FFFD (the only divergence, and only for astral characters at the cut).
pub fn slice16(s: &str, end: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(end).collect();
    String::from_utf16_lossy(&units)
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
                qcalc_core::jsnum::to_string(x).len()
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
        // [{"id":"a\"b","n":1,"x":0.1,"s":"π…\n","ok":true}]
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
        assert_eq!(slice16("abc", 2), "ab");
        assert_eq!(slice16("a😀", 2), "a\u{FFFD}");
        assert_eq!(trim("\u{FEFF} x \u{A0}"), "x");
    }
}
