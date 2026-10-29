//! `src/engine/dots.ts`: every way of writing a dotted variable, as the one form the time derivative reads
//! (`thetadot`, `xddot`): LaTeX `\dot\theta` / `\ddot{x}`, Typst `dot(theta)` / `dot.double(x)`, and the dotted
//! letters themselves (θ̇, ẍ).

use unicode_normalization::UnicodeNormalization;

use crate::jsre::{self, Lazy, js};

/// The combining dots and how many derivatives each is.
fn mark_order(c: char) -> u32 {
    match c {
        '\u{0307}' => 1,
        '\u{0308}' => 2,
        '\u{20DB}' => 3,
        _ => 0,
    }
}

// Greek as its ASCII name, so `thetadot` is one word (a θ beside `dot` reads as Typst's multiplication dot)
fn greek_name(name: &str) -> &str {
    match name {
        "θ" => "theta",
        "φ" => "phi",
        "ψ" => "psi",
        "ω" => "omega",
        "α" => "alpha",
        "β" => "beta",
        "γ" => "gamma",
        other => other,
    }
}

fn joined(name: &str, order: usize) -> String {
    format!(" {}{}ot ", greek_name(name), "d".repeat(order))
}

/// `\dot\theta`, `dot(theta)` and `θ̇` all as ` thetadot `.
pub fn join_dots(text: &str) -> String {
    static ANY: Lazy = Lazy::new(|| js(r"dot|[\u0307\u0308\u20DB]|[ẋẏżṙṡẍÿ]", ""));
    static PRECOMPOSED: Lazy = Lazy::new(|| js(r"[ẋẏżṙṡẍÿ]", ""));
    static LATEX: Lazy = Lazy::new(|| js(r"\\(d{1,3})ot\s*(?:\{\s*)?\\?([A-Za-z]+)(?:\s*\})?", ""));
    static TYPST: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z])dot(?:\.(double|triple))?\(\s*([A-Za-zθφψωαβγ]+)\s*\)",
            "",
        )
    });
    let nfd: String = text.nfd().collect();
    if !jsre::test(&ANY, &nfd) && !jsre::test(&PRECOMPOSED, text) {
        return text.to_string();
    }
    let s = jsre::replace_all(&LATEX, text, |c| {
        joined(jsre::group(c, 2), jsre::group(c, 1).len())
    });
    let s = jsre::replace_all(&TYPST, &s, |c| {
        let order = match jsre::group(c, 1) {
            "double" => 2,
            "triple" => 3,
            _ => 1,
        };
        joined(jsre::group(c, 2), order)
    });
    // a letter with a combining dot or two, precomposed (ẋ) or not (θ + U+0307)
    let chars: Vec<char> = s.nfd().collect();
    let letter = |c: Option<&char>| c.is_some_and(|c| c.is_alphabetic() && is_letter(*c));
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let before = if i > 0 { chars.get(i - 1) } else { None };
        let mut order = 0;
        let mut marks = String::new();
        while let Some(&next) = chars.get(i + 1) {
            let k = mark_order(next);
            if k == 0 {
                break;
            }
            marks.push(next);
            order += k;
            i += 1;
        }
        let umlaut = marks.contains('\u{0308}');
        // ö in Ångström and ï in naïve are letters of a word, not a second derivative; a lone ẍ or θ̈ is
        let in_word =
            umlaut && "aeiouyAEIOUY".contains(ch) && (letter(before) || letter(chars.get(i + 1)));
        let latin_or_greek =
            ch.is_ascii_alphabetic() || ('α'..='ω').contains(&ch) || ('Α'..='Ω').contains(&ch);
        if order > 0 && !in_word && latin_or_greek {
            out.push_str(&joined(&ch.to_string(), order.min(3) as usize));
        } else {
            out.push(ch);
            out.push_str(&marks);
        }
        i += 1;
    }
    // the rest back as it was typed (Å and é stay single characters)
    out.nfc().collect()
}

/// `/\p{L}/u`: a letter of any script.
fn is_letter(c: char) -> bool {
    // Rust's `is_alphabetic` is Alphabetic, a superset of L (it adds marks like U+0345 and letter numbers);
    // the few extras never sit beside a dot in a formula
    !matches!(
        c,
        '\u{0345}' | '\u{05B0}'..='\u{05C7}' | '\u{2160}'..='\u{2188}' | '\u{24B6}'..='\u{24E9}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_variables_join() {
        assert_eq!(join_dots("\\dot\\theta"), " thetadot ");
        assert_eq!(join_dots("\\ddot{x}"), " xddot ");
        assert_eq!(join_dots("dot.double(x)"), " xddot ");
        assert_eq!(join_dots("θ²θ̇"), "θ² thetadot ");
        assert_eq!(join_dots("ẋ sin θ"), " xdot  sin θ");
        assert_eq!(join_dots("naïve"), "naïve");
        assert_eq!(join_dots("2 dot 3"), "2 dot 3");
        assert_eq!(join_dots("5 Ångström in m"), "5 Ångström in m");
    }
}
