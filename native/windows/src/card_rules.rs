//! The message cards' and dropped files' rules, pure so they're tested everywhere. Mac spec:
//! `mac/Sources/Messages/CardRules.swift` (`kept`, `putBack`, `messageHint`), checked there by `test-cards.sh`.

/// The most cards under the bar at once (messageBanner.ts keeps three).
pub const MAX: usize = 3;

/// The cards kept when more than `max` are up, newest first as given. A card being answered (its reply line has
/// the keys, words typed in it, a reply on its way) is never the one dropped: three new messages arriving while a
/// reply was typed had taken the card, the draft and the keyboard focus with it. The others go oldest first; past
/// `max` answered cards, all of them stay.
pub fn kept<T>(cards: Vec<T>, max: usize, held: impl Fn(&T) -> bool) -> Vec<T> {
    if cards.len() <= max {
        return cards;
    }
    let flags: Vec<bool> = cards.iter().map(&held).collect();
    let mut room = max.saturating_sub(flags.iter().filter(|h| **h).count());
    let mut out = Vec::new();
    for (card, h) in cards.into_iter().zip(flags) {
        if h {
            out.push(card);
        } else if room > 0 {
            out.push(card);
            room -= 1;
        }
    }
    out
}

/// Files dropped and sent from the bar whose send never started (the relay out of reach, no key for them) come
/// back on the line, so ↵ tries again. `typed` is the `message <name> words` line the drop went from ("" for a
/// plain drop or `send file to <name>`): when its words weren't sent (`words_unsent`) the whole line comes back,
/// with the files that didn't go waiting on it; words that went leave `send file to <name>` for the files alone.
/// `files` may be empty (only the words failed). Either failure can come first: a `send file to <name>` line put
/// back already grows into the words line, and a words line already back takes the files. None when the line holds
/// something else by then (typed on since), which is left alone.
pub fn put_back<T: PartialEq + Clone>(
    files: &[T],
    name: &str,
    typed: &str,
    words_unsent: bool,
    line: &str,
    waiting: &[T],
) -> Option<(String, Vec<T>)> {
    let file_line = format!("send file to {name}");
    let back = if words_unsent && !typed.is_empty() {
        typed.to_string()
    } else {
        file_line.clone()
    };
    let mut merged = waiting.to_vec();
    for f in files {
        if !merged.contains(f) {
            merged.push(f.clone());
        }
    }
    if line.trim().is_empty() {
        return Some((back, files.to_vec()));
    }
    if line == file_line {
        return Some((back, merged));
    }
    if !typed.is_empty() && line == typed {
        return Some((typed.to_string(), merged));
    }
    None
}

/// A `message <name> …` line's hint while files wait on it (put back when the relay was out of reach): ↵ sends
/// them with the words, so the hint counts them (`↵ sends to max with 2 files`), or offers them alone with no
/// words yet (`↵ offers a.pdf to max`, as ↵ does). `dropped` is the files' label (`a.pdf`, `2 files`); any other
/// hint (no name yet, username not found, settings) is left as it was.
pub fn message_hint(hint: String, name: &str, dropped: &str) -> String {
    if dropped.is_empty() || name.is_empty() {
        return hint;
    }
    if hint == format!("then what to say to {name}") {
        return format!("↵ offers {dropped} to {name}");
    }
    if hint == format!("↵ sends to {name}") {
        return format!("↵ sends to {name} with {dropped}");
    }
    hint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answered_card_stays_past_three() {
        // newest first; card 4 (the oldest) is being answered
        let cards = vec![1, 2, 3, 4];
        assert_eq!(kept(cards.clone(), MAX, |c| *c == 4), vec![1, 2, 4]);
        assert_eq!(kept(cards.clone(), MAX, |_| false), vec![1, 2, 3]);
        assert_eq!(kept(vec![1, 2], MAX, |_| false), vec![1, 2]);
        // more answered than room: all of them stay
        assert_eq!(kept(cards, MAX, |c| *c != 1), vec![2, 3, 4]);
        assert_eq!(kept(vec![1, 2, 3, 4, 5], 3, |c| *c > 1), vec![2, 3, 4, 5]);
    }

    #[test]
    fn unsent_files_come_back_on_the_line() {
        // a cleared line takes `send file to max` and the file
        assert_eq!(
            put_back(&["a"], "max", "", false, "", &[]),
            Some(("send file to max".into(), vec!["a"]))
        );
        // a second file joins the first
        assert_eq!(
            put_back(&["b"], "max", "", false, "send file to max", &["a"]),
            Some(("send file to max".into(), vec!["a", "b"]))
        );
        // typed on since: left alone
        assert_eq!(put_back(&["a"], "max", "", false, "2+2", &[]), None);
    }

    #[test]
    fn unsent_words_bring_their_line_back() {
        let typed = "message max here you go";
        // the words failed first: the whole line, no files yet
        assert_eq!(
            put_back::<&str>(&[], "max", typed, true, "", &[]),
            Some((typed.into(), vec![]))
        );
        // then a file: it waits on the words line
        assert_eq!(
            put_back(&["a"], "max", typed, false, typed, &[]),
            Some((typed.into(), vec!["a"]))
        );
        // the file first, then the words: the file line grows into the words line
        assert_eq!(
            put_back::<&str>(&[], "max", typed, true, "send file to max", &["a"]),
            Some((typed.into(), vec!["a"]))
        );
        // words that went: the file alone
        assert_eq!(
            put_back(&["a"], "max", typed, false, "", &[]),
            Some(("send file to max".into(), vec!["a"]))
        );
    }

    #[test]
    fn a_message_hint_counts_the_waiting_files() {
        assert_eq!(
            message_hint("↵ sends to max".into(), "max", "2 files"),
            "↵ sends to max with 2 files"
        );
        assert_eq!(
            message_hint("then what to say to max".into(), "max", "a.pdf"),
            "↵ offers a.pdf to max"
        );
        assert_eq!(
            message_hint("↵ sends to max".into(), "max", ""),
            "↵ sends to max"
        );
        assert_eq!(
            message_hint("username not found".into(), "max", "a.pdf"),
            "username not found"
        );
    }
}
