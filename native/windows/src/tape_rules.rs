//! The tape's selection, insert and reading rules, pure (the Mac's `Bar/TapeRules.swift`, checked there by
//! `scripts/test-tape.sh`; the same checks are the tests below). history.rs and model.rs call them.

/// The row picked after row `deleted` goes from a tape now `count` rows long. With the deleted row picked (Backspace,
/// Ctrl+Backspace), the row that took its place (or the new last row); with another row picked, that same row; with
/// none picked (a row's Delete menu while the line is in use), none, so a plain Backspace on the line doesn't then
/// delete another row.
pub fn selection_after_delete(
    deleted: usize,
    selected: Option<usize>,
    count: usize,
) -> Option<usize> {
    let s = selected?;
    if count == 0 {
        return None;
    }
    if s == deleted {
        return Some(deleted.min(count - 1));
    }
    Some((if s > deleted { s - 1 } else { s }).min(count - 1))
}

/// Whether Ctrl+Z picks the row it brings back: only while the tape is being walked, for the reason above.
pub fn selects_restored(tape_open: bool, selected: Option<usize>) -> bool {
    tape_open || selected.is_some()
}

/// Where the tape scrolls when its rows change: the picked row (a delete or Ctrl+Z mid-tape stays put), else the
/// newest.
pub fn scroll_target(selected: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match selected {
        Some(s) if s < count => Some(s),
        _ => Some(count - 1),
    }
}

/// Whether an answer put in from the tape stands alone on the line (the page's `!rest.trim()`): nothing else is
/// there, or all of it is selected and goes. Otherwise it goes in as `answerAmong` wraps it (`5(-2)`, not `5-2`).
pub fn answer_alone(line: &str, selection: &str) -> bool {
    let rest = line.trim();
    rest.is_empty() || (!selection.is_empty() && rest == selection.trim())
}

/// A plain Backspace or Delete right after Shift-click or Shift+Enter put an answer in (the row stays picked so more
/// can follow) edits the line, not deletes that row; once ↑ ↓ ← → moved on, Backspace deletes the picked row again.
pub fn delete_edits_line(
    kept: Option<(usize, usize)>,
    selected: Option<usize>,
    pick: usize,
) -> bool {
    match (kept, selected) {
        (Some((row, p)), Some(s)) => row == s && p == pick,
        _ => false,
    }
}

/// The roots a solved tape row shows, each its own button (the page's RootPicks): as the row lists them (`-3, 3`),
/// else each at the row's figures (`±1.41421` is `-1.41421` and `1.41421`).
pub fn root_labels(display: &str, each: &[String]) -> Vec<String> {
    let shown: Vec<String> = display
        .split(", ")
        .filter(|p| *p != "…")
        .map(str::to_string)
        .collect();
    if shown.len() == each.len() {
        shown
    } else {
        each.to_vec()
    }
}

/// What a screen reader hears as ↑ ↓ land on a row, as the row reads: a note has no answer, so no "equals".
pub fn spoken(expr: &str, display: &str, note: bool) -> String {
    if note {
        return format!("Note {expr}");
    }
    let expr = spoken_limits(expr);
    if display.is_empty() {
        expr
    } else {
        format!("{expr} equals {display}")
    }
}

/// A row's `∫_0^1` and `Σ_(n=1)^10` are said "integral from 0 to 1" and "sum from n = 1 to 10", not read out as
/// typed. The limits are a bracketed run, or what runs up to a space, `^`, `_` or `(`.
pub fn spoken_limits(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let limit = |from: usize| -> Option<(String, usize)> {
        if from >= chars.len() {
            return None;
        }
        if chars[from] == '(' {
            let mut depth = 0;
            for (j, &c) in chars.iter().enumerate().skip(from) {
                if c == '(' {
                    depth += 1;
                }
                if c == ')' {
                    depth -= 1;
                    if depth == 0 {
                        return Some((chars[from + 1..j].iter().collect(), j + 1));
                    }
                }
            }
            return None;
        }
        let mut j = from;
        while j < chars.len() && !chars[j].is_whitespace() && !matches!(chars[j], '^' | '_' | '(') {
            j += 1;
        }
        (j > from).then(|| (chars[from..j].iter().collect(), j))
    };
    let said = |bound: &str| {
        bound
            .replace('=', " = ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if "∫∑Σ∏Π".contains(chars[i])
            && i + 1 < chars.len()
            && chars[i + 1] == '_'
            && let Some((lower, after_lower)) = limit(i + 2)
        {
            let name = match chars[i] {
                '∫' => "integral",
                '∑' | 'Σ' => "sum",
                _ => "product",
            };
            let mut words = format!("{name} from {}", said(&lower));
            let mut end = after_lower;
            if after_lower < chars.len()
                && chars[after_lower] == '^'
                && let Some((upper, after_upper)) = limit(after_lower + 1)
            {
                words += &format!(" to {}", said(&upper));
                end = after_upper;
            }
            if out.chars().last().is_some_and(|c| !c.is_whitespace()) {
                out.push(' ');
            }
            out += &words;
            if end < chars.len() && !chars[end].is_whitespace() {
                out.push(' ');
            }
            i = end;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // the Mac's Tests/tape/main.swift, case for case
    #[test]
    fn deletes_keep_the_pick_where_it_belongs() {
        // the picked row goes: the next one takes its place, or the new last row
        assert_eq!(selection_after_delete(2, Some(2), 5), Some(2));
        assert_eq!(selection_after_delete(4, Some(4), 4), Some(3));
        // another row picked stays picked, its index shifted past the deleted one
        assert_eq!(selection_after_delete(1, Some(3), 4), Some(2));
        assert_eq!(selection_after_delete(3, Some(1), 4), Some(1));
        // a menu Delete with nothing picked picks nothing (Backspace then edits the line)
        assert_eq!(selection_after_delete(1, None, 4), None);
        assert_eq!(selection_after_delete(0, Some(0), 0), None);
        assert!(selects_restored(true, None));
        assert!(selects_restored(false, Some(2)));
        assert!(!selects_restored(false, None));
        assert_eq!(scroll_target(Some(2), 5), Some(2));
        assert_eq!(scroll_target(None, 5), Some(4));
        assert_eq!(scroll_target(Some(9), 5), Some(4));
        assert_eq!(scroll_target(None, 0), None);
    }

    #[test]
    fn an_answer_alone_or_among() {
        assert!(answer_alone("", ""));
        assert!(answer_alone("   ", ""));
        assert!(!answer_alone("5", ""));
        assert!(answer_alone("12+3", "12+3"));
        assert!(!answer_alone("12+3", "3"));
    }

    #[test]
    fn backspace_after_a_kept_insert_edits_the_line() {
        assert!(delete_edits_line(Some((3, 1)), Some(3), 1));
        assert!(!delete_edits_line(Some((3, 1)), Some(3), 2));
        assert!(!delete_edits_line(Some((3, 1)), Some(2), 1));
        assert!(!delete_edits_line(None, Some(3), 1));
        assert!(!delete_edits_line(Some((3, 1)), None, 1));
    }

    #[test]
    fn roots_each_their_own_label() {
        let each = vec!["-1.41421".to_string(), "1.41421".to_string()];
        assert_eq!(root_labels("±1.41421", &each), each);
        assert_eq!(
            root_labels("-3, 3", &["-3".into(), "3".into()]),
            ["-3", "3"]
        );
        assert_eq!(
            root_labels("1, 2, …", &["1".into(), "2".into()]),
            ["1", "2"]
        );
    }

    #[test]
    fn what_a_reader_hears() {
        assert_eq!(spoken("call Sam", "", true), "Note call Sam");
        assert_eq!(spoken("2+2", "4", false), "2+2 equals 4");
        assert_eq!(spoken("x", "", false), "x");
        assert_eq!(spoken_limits("∫_0^1 x dx"), "integral from 0 to 1 x dx");
        assert_eq!(spoken_limits("Σ_(n=1)^10 n"), "sum from n = 1 to 10 n");
        assert_eq!(
            spoken_limits("2∏_(k=1)^(n+1)k"),
            "2 product from k = 1 to n+1 k"
        );
        assert_eq!(spoken_limits("∫_C f"), "integral from C f");
        assert_eq!(spoken_limits("∫ x dx"), "∫ x dx");
        assert_eq!(spoken("∫_0^1", "", true), "Note ∫_0^1");
    }
}
