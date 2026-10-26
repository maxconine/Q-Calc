//! Pasted text cleaned of look-alike characters the engine can't read (`src/lib/paste.ts`). Only swaps that can't
//! change a meaning: en and em dashes stay, since `5–10` is a range as often as a minus.

use crate::jsre::{self, Lazy, js};

/// `cleanPastedText`.
pub fn clean_pasted_text(text: &str) -> String {
    static GROUPED: Lazy = Lazy::new(|| {
        js(
            r"(?<![\w.,])\d{1,3}(?:[\u{00a0}\u{2007}\u{2009}\u{202f}]\d{3})+(?!\d)",
            "",
        )
    });
    let lines: String = text
        .chars()
        .map(|c| match c {
            '\u{0085}' | '\u{2028}' | '\u{2029}' => '\n',
            c => c,
        })
        .filter(|c| {
            !matches!(
                c,
                '\u{ad}' | '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}'
            )
        })
        .collect();
    // a thin or no-break space inside a number groups its thousands (1 000 000 from a web page); joined here, before
    // those spaces become plain ones, which between two numbers would multiply them
    let joined = jsre::replace_all(&GROUPED, &lines, |c| {
        c.get(0)
            .map_or("", |m| m.as_str())
            .chars()
            .filter(|c| !matches!(c, '\u{00a0}' | '\u{2007}' | '\u{2009}' | '\u{202f}'))
            .collect()
    })
    .into_owned();
    let mut out = String::with_capacity(joined.len());
    for c in joined.chars() {
        match c {
            '\u{a0}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' => {
                out.push(' ')
            }
            '\u{2010}' | '\u{2011}' | '\u{fe63}' => out.push('-'),
            '⁄' | '∕' => out.push('/'),
            '∗' => out.push('*'),
            '\u{ff01}'..='\u{ff5e}' => {
                out.push(char::from_u32(c as u32 - 0xfee0).unwrap_or(c));
            }
            // the ligatures' NFKC forms
            'ﬀ' => out.push_str("ff"),
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'ﬅ' | 'ﬆ' => out.push_str("st"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_look_alikes() {
        assert_eq!(
            clean_pasted_text("1\u{202f}000\u{202f}000 ＋ 2"),
            "1000000 + 2"
        );
        assert_eq!(clean_pasted_text("a\u{200b}b ﬁ 5–10"), "ab fi 5–10");
        assert_eq!(clean_pasted_text("x\u{2028}y"), "x\ny");
    }
}
