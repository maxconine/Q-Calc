//! native only: where the bar draws a faint `°` or `rad` to say which angle mode a trig function's argument is read
//! in (`50sin(26.87)` reads `50sin(26.87°)` in degrees). The bar picks the text from the mode; this finds the places.

/// The functions whose argument is an angle (their inverses and the hyperbolic ones take plain numbers).
const TRIG: [&str; 6] = ["sin", "cos", "tan", "sec", "csc", "cot"];

/// The UTF-16 offsets just past each trig argument that doesn't say its own unit: before a call's `)`, the end of
/// the line for one still open, or after a bare number (`sin 30`). An argument with `°`, `deg`, `rad`, `grad`, `gon`,
/// `′` or `″` in it gets none, nor does an empty one.
pub fn angle_marks(text: &str) -> Vec<usize> {
    angle_spans(text).into_iter().map(|(_, at)| at).collect()
}

/// Each marked argument as (where it starts, where its mark goes), in UTF-16 offsets: the bar hides a mark while
/// the caret is anywhere from the start to the mark, so it never sits in the gap the mark opens.
pub fn angle_spans(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let u16_at: Vec<usize> = {
        let mut out = Vec::with_capacity(chars.len() + 1);
        let mut n = 0;
        for c in &chars {
            out.push(n);
            n += c.len_utf16();
        }
        out.push(n);
        out
    };
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let is_letter = |i: usize| chars.get(i).is_some_and(|c| c.is_alphabetic() || *c == '_');
    let mut marks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let Some(name) = TRIG.iter().find(|n| {
            let n: Vec<char> = n.chars().collect();
            lower.get(i..i + n.len()) == Some(&n[..])
        }) else {
            i += 1;
            continue;
        };
        let end = i + name.len();
        // asin, arcsin, sinh, cosec… are other words
        if (i > 0 && is_letter(i - 1)) || is_letter(end) {
            i += 1;
            continue;
        }
        let mut k = end;
        while chars.get(k).is_some_and(|c| *c == ' ') {
            k += 1;
        }
        let (start, stop, at) = if chars.get(k) == Some(&'(') {
            let mut depth = 0;
            let mut close = None;
            for (j, c) in chars.iter().enumerate().skip(k) {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            match close {
                Some(j) => (k + 1, j, j),
                None => (k + 1, chars.len(), chars.len()),
            }
        } else {
            // a bare number: `sin 30`, `sin30`, `sin -45`
            let mut j = k;
            if chars.get(j).is_some_and(|c| *c == '-' || *c == '−') {
                j += 1;
            }
            let digits = j;
            while chars.get(j).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
                j += 1;
            }
            if j == digits {
                i = end;
                continue;
            }
            (k, j, j)
        };
        let arg: String = lower[start..stop].iter().collect();
        let says_unit = ["°", "deg", "rad", "grad", "gon", "′", "″", "'"].iter().any(|u| arg.contains(u));
        if !arg.trim().is_empty() && !says_unit {
            marks.push((u16_at[start], u16_at[at]));
        }
        i = end;
    }
    marks.sort_unstable_by_key(|m| (m.1, m.0));
    marks.dedup_by_key(|m| m.1);
    marks
}

#[cfg(test)]
mod tests {
    use super::{angle_marks, angle_spans};

    #[test]
    fn marks_each_trig_argument() {
        assert_eq!(angle_marks("50sin(26.87)"), vec![11]);
        assert_eq!(angle_marks("sin(30"), vec![6]);
        assert_eq!(angle_marks("sin 30 + 1"), vec![6]);
        assert_eq!(angle_marks("sin30"), vec![5]);
        assert_eq!(angle_marks("cos(x) + tan(45)"), vec![5, 15]);
        assert_eq!(angle_marks("sin(cos(30))"), vec![10, 11]);
        assert_eq!(angle_marks("SIN(30)"), vec![6]);
    }

    #[test]
    fn leaves_what_says_its_unit_or_isnt_trig() {
        for t in ["sin(30°)", "sin(30 deg)", "sin(pi rad)", "asin(0.5)", "arcsin(0.5)", "sinh(2)", "sin()", "sin x", "since 3", "2+2", "cosec(3)"] {
            assert!(angle_marks(t).is_empty(), "{t}: {:?}", angle_marks(t));
        }
    }

    #[test]
    fn spans_start_at_the_argument() {
        assert_eq!(angle_spans("12sin(123+12)"), vec![(6, 12)]);
        assert_eq!(angle_spans("sin 30 + 1"), vec![(4, 6)]);
        assert_eq!(angle_spans("sin(30"), vec![(4, 6)]);
    }

    #[test]
    fn counts_utf16() {
        // π is one UTF-16 unit, an emoji two
        assert_eq!(angle_marks("π+sin(2)"), vec![7]);
        assert_eq!(angle_marks("😀 sin(2)"), vec![8]);
    }
}
