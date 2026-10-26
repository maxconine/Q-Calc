//! `src/lib/bounds.ts`: the limits typed next to a ∫, Σ or Π (`_lo^hi`, or `∫lo..hi`), where the caret may sit
//! among them, and the edits that move between them.
//!
//! Offsets are UTF-16 units, as the TS (and an `NSTextView`'s `NSRange`) count them, so the text is walked as
//! UTF-16. Every test is on a BMP character; a surrogate half is "some other character", as in JS.

use serde::{Deserialize, Serialize};

use crate::jsstr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub start: i64,
    pub end: i64,
}

/// A ∫, Σ or Π with its limits. Slots are the limits' own text; everything else between the sign and `end`
/// (`_`, `^`, `..`, braces) is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bound {
    pub sign: i64,
    pub lower: Slot,
    /// None until the upper limit is started.
    pub upper: Option<Slot>,
    /// The first offset after the limits.
    pub end: i64,
}

/// The text after an edit and where the caret goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub text: String,
    pub caret: i64,
}

pub const BOUND_SIGNS: &str = "∫Σ∑Π∏";

/// Text as UTF-16 units, indexable like a JS string.
pub(crate) struct U16(pub Vec<u16>);

impl U16 {
    pub(crate) fn new(s: &str) -> Self {
        U16(s.encode_utf16().collect())
    }
    fn len(&self) -> i64 {
        self.0.len() as i64
    }
    /// `s[i]`: None past either end.
    fn at(&self, i: i64) -> Option<char> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.0.get(i))
            .map(|&u| char::from_u32(u32::from(u)).unwrap_or('\u{fffd}'))
    }
    fn is(&self, i: i64, c: char) -> bool {
        self.at(i) == Some(c)
    }
    fn starts_with(&self, word: &str, i: i64) -> bool {
        let Ok(i) = usize::try_from(i) else {
            return false;
        };
        let w: Vec<u16> = word.encode_utf16().collect();
        self.0.get(i..i + w.len()) == Some(&w[..])
    }
    /// `s.slice(from, to)` with JS clamping.
    pub(crate) fn slice(&self, from: i64, to: i64) -> String {
        let n = self.len();
        let clamp = |x: i64| (if x < 0 { (n + x).max(0) } else { x.min(n) }) as usize;
        let (a, b) = (clamp(from), clamp(to));
        if a >= b {
            return String::new();
        }
        String::from_utf16_lossy(&self.0[a..b])
    }
}

fn is_space(c: Option<char>) -> bool {
    c.is_some_and(jsstr::is_space)
}

fn is_sign(c: Option<char>) -> bool {
    c.is_some_and(|c| BOUND_SIGNS.contains(c))
}

fn pair(c: Option<char>) -> Option<char> {
    match c? {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// A group's matching close; -1 while it's still open.
fn close_of(s: &U16, open: i64, same_kind: bool) -> i64 {
    let mut stack: Vec<char> = Vec::new();
    let first = s.at(open);
    for i in open..s.len() {
        let ch = s.at(i);
        if same_kind {
            if ch == first {
                if let Some(p) = pair(ch) {
                    stack.push(p);
                } else {
                    // JS pushes `undefined`
                    stack.push('\0');
                }
            } else if ch.is_some() && ch == pair(first) {
                stack.pop();
                if stack.is_empty() {
                    return i;
                }
            }
            continue;
        }
        if let Some(p) = pair(ch) {
            stack.push(p);
        } else if matches!(ch, Some(')' | ']' | '}')) {
            if stack.pop() != ch {
                return -1;
            }
            if stack.is_empty() {
                return i;
            }
        }
    }
    -1
}

struct Group {
    slot: Slot,
    after: i64,
}

fn braced(text: &U16, at: i64, same_kind: bool) -> Group {
    let close = close_of(text, at, same_kind);
    // still being typed: the group runs to the end
    if close < 0 {
        return Group {
            slot: Slot {
                start: at + 1,
                end: text.len(),
            },
            after: text.len(),
        };
    }
    Group {
        slot: Slot {
            start: at + 1,
            end: close,
        },
        after: close + 1,
    }
}

/// The ∫'s upper limit the way calculus.ts scanBound reads it: up to a space, or a `(` that starts the body.
fn integral_upper(text: &U16, from: i64) -> Group {
    let mut at = from;
    while is_space(text.at(at)) {
        at += 1;
    }
    if text.is(at, '{') {
        return braced(text, at, true);
    }
    let mut depth = 0;
    let mut i = at;
    while i < text.len() {
        let ch = text.at(i);
        if depth == 0 && is_space(ch) {
            break;
        }
        if ch == Some('(') {
            if depth == 0
                && i > at
                && !text
                    .at(i - 1)
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                break;
            }
            depth += 1;
        } else if ch == Some(')') {
            if depth == 0 {
                break;
            }
            depth -= 1;
        }
        i += 1;
    }
    Group {
        slot: Slot { start: at, end: i },
        after: i,
    }
}

/// Σ and Π limits the way sums.ts grabGroup reads them; `caret` also stops the lower limit.
fn sum_group(text: &U16, at: i64, caret: bool) -> Group {
    if text.is(at, '{') || text.is(at, '(') {
        return braced(text, at, false);
    }
    let mut i = at;
    while i < text.len() {
        let ch = text.at(i);
        if is_space(ch) || (caret && ch == Some('^')) {
            break;
        }
        i += 1;
    }
    Group {
        slot: Slot { start: at, end: i },
        after: i,
    }
}

fn bound_at(text: &U16, sign: i64) -> Option<Bound> {
    let integral = text.is(sign, '∫');
    let j = sign + 1;
    if j == text.len() {
        return Some(Bound {
            sign,
            lower: Slot { start: j, end: j },
            upper: None,
            end: j,
        });
    }
    if text.is(j, '_') {
        let mut at = j + 1;
        let lower = if integral {
            while is_space(text.at(at)) {
                at += 1;
            }
            if text.is(at, '{') {
                braced(text, at, true)
            } else {
                let k = (at.max(0)..text.len()).find(|&k| text.is(k, '^'));
                let end = k.unwrap_or(text.len());
                Group {
                    slot: Slot { start: at, end },
                    after: end,
                }
            }
        } else {
            sum_group(text, at, true)
        };
        at = lower.after;
        if integral {
            while is_space(text.at(at)) {
                at += 1;
            }
        }
        if !text.is(at, '^') {
            return Some(Bound {
                sign,
                lower: lower.slot,
                upper: None,
                end: lower.after,
            });
        }
        let upper = if integral {
            integral_upper(text, at + 1)
        } else {
            sum_group(text, at + 1, false)
        };
        return Some(Bound {
            sign,
            lower: lower.slot,
            upper: Some(upper.slot),
            end: upper.after,
        });
    }
    if !integral {
        return None;
    }
    // `∫0..1 x^2`: the lower limit runs to the `..` and holds no space
    let mut i = j;
    while i < text.len() && !is_space(text.at(i)) && !text.starts_with("..", i) {
        i += 1;
    }
    if text.is(j, '(') || is_space(text.at(j)) {
        return None;
    }
    if !text.starts_with("..", i) {
        return (i == text.len()).then_some(Bound {
            sign,
            lower: Slot { start: j, end: i },
            upper: None,
            end: i,
        });
    }
    let upper = integral_upper(text, i + 2);
    Some(Bound {
        sign,
        lower: Slot { start: j, end: i },
        upper: Some(upper.slot),
        end: upper.after,
    })
}

pub(crate) fn bounds_in_units(text: &U16) -> Vec<Bound> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if is_sign(text.at(i))
            && let Some(b) = bound_at(text, i)
        {
            out.push(b);
            // a sign inside a limit is part of that limit's text
            i = i.max(b.end - 1);
        }
        i += 1;
    }
    out
}

/// Each sign in `text` whose limits are typed next to it, in order.
pub fn bounds_in(text: &str) -> Vec<Bound> {
    bounds_in_units(&U16::new(text))
}

fn in_slot(s: Option<Slot>, o: i64) -> bool {
    s.is_some_and(|s| o >= s.start && o <= s.end)
}

/// Offsets hidden among `_`, `^`, `..` and braces, where a caret would type into the wrong place.
pub fn is_hidden(bounds: &[Bound], o: i64) -> bool {
    bounds
        .iter()
        .any(|b| o > b.sign && o < b.end && !in_slot(Some(b.lower), o) && !in_slot(b.upper, o))
}

/// The nearest offset a caret can sit at, moving in `dir` from a hidden one.
pub fn snap_offset(bounds: &[Bound], o: i64, dir: i64, length: i64) -> i64 {
    let mut p = o;
    while p >= 0 && p <= length && is_hidden(bounds, p) {
        p += dir;
    }
    if p < 0 || p > length {
        p = o;
        while is_hidden(bounds, p) {
            p -= dir;
        }
    }
    p
}

fn splice(text: &U16, from: i64, to: i64, put: &str, caret: i64) -> Edit {
    Edit {
        text: format!("{}{put}{}", text.slice(0, from), text.slice(to, text.len())),
        caret,
    }
}

/// What can start a limit right after the sign (`[^\s(_^{},;)]`); `(` and a space start the other forms.
fn starts_limit(c: Option<char>) -> bool {
    c.is_some_and(|c| !jsstr::is_space(c) && !"(_^{},;)".contains(c))
}

/// After one typed character at `caret`: a sign followed by a character starts its lower limit (`∫0` becomes
/// `∫_0`), and `..` in an `_` lower limit moves to the upper (`∫_0..` becomes `∫_0^`).
pub fn after_typing(text: &str, caret: i64) -> Option<Edit> {
    let t = U16::new(text);
    if caret >= 2 && caret == t.len() && is_sign(t.at(caret - 2)) && starts_limit(t.at(caret - 1)) {
        return Some(splice(&t, caret - 1, caret - 1, "_", caret + 1));
    }
    if caret >= 2 && t.starts_with("..", caret - 2) {
        let b = bounds_in_units(&t)
            .into_iter()
            .find(|x| t.is(x.sign + 1, '_') && x.upper.is_none() && caret == x.lower.end);
        if let Some(b) = b
            && caret - 2 >= b.lower.start
        {
            return Some(splice(&t, caret - 2, caret, "^", caret - 1));
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Which {
    Lower,
    Upper,
}

fn slot_of(bounds: &[Bound], o: i64) -> Option<(Bound, Which)> {
    for b in bounds {
        if in_slot(Some(b.lower), o) {
            return Some((*b, Which::Lower));
        }
        if in_slot(b.upper, o) {
            return Some((*b, Which::Upper));
        }
    }
    None
}

/// The first offset of the body after a sign's limits.
fn body_start(text: &U16, b: &Bound) -> i64 {
    let mut i = b.end;
    while is_space(text.at(i)) {
        i += 1;
    }
    i
}

fn open_parens(s: &str) -> bool {
    s.matches('(').count() > s.matches(')').count()
}

/// A character between the sign and the end of its limits that isn't a limit's own text.
fn hidden_char(bounds: &[Bound], c: i64) -> bool {
    bounds.iter().any(|b| {
        c > b.sign
            && c < b.end
            && !(c >= b.lower.start && c < b.lower.end)
            && !b.upper.is_some_and(|u| c >= u.start && c < u.end)
    })
}

fn same(text: &str, caret: i64) -> Option<Edit> {
    Some(Edit {
        text: text.to_string(),
        caret,
    })
}

/// A key pressed with the caret at `caret` (no selection), when it moves between the limits: → or tab at the
/// end of a limit goes to the next one, backspace at the start of one steps back. None leaves the key to the input.
pub fn bound_key(text: &str, caret: i64, key: &str) -> Option<Edit> {
    let t = U16::new(text);
    let bounds = bounds_in_units(&t);
    if bounds.is_empty() {
        return None;
    }
    let at = slot_of(&bounds, caret);
    // ↑ or enter in the lower limit goes up to the upper one, ↓ in the upper comes back down
    if (key == "ArrowUp" || key == "Enter")
        && let Some((b, Which::Lower)) = at
    {
        let end = b.lower.end;
        return bound_key(text, end, "ArrowRight").or_else(|| same(text, end));
    }
    if key == "ArrowDown"
        && let Some((b, Which::Upper)) = at
    {
        return same(text, b.lower.end);
    }
    if matches!(key, "ArrowRight" | "Tab" | " " | "^") {
        let (b, which) = at?;
        let slot = match which {
            Which::Lower => b.lower,
            Which::Upper => b.upper?,
        };
        let empty = slot.start == slot.end;
        // a bare sign with nothing after it: a space or `(` still starts the other forms
        let bare = b.end == b.sign + 1;
        if caret != slot.end || open_parens(&t.slice(slot.start, slot.end)) {
            return None;
        }
        if which == Which::Lower {
            if key == " " {
                return if empty && !bare {
                    same(text, caret)
                } else {
                    None
                };
            }
            if let Some(u) = b.upper {
                return same(text, u.start);
            }
            if key == "^" {
                return None;
            }
            if bare {
                return Some(splice(&t, b.end, b.end, "_^", b.end + 2));
            }
            if !t.is(b.sign + 1, '_') {
                return Some(splice(&t, b.end, b.end, "..", b.end + 2));
            }
            return Some(splice(&t, b.end, b.end, "^", b.end + 1));
        }
        if key == "^" {
            return None;
        }
        if key == " " {
            return if empty { same(text, caret) } else { None };
        }
        // an empty upper keeps its braces, or the body would be read as the limit
        if empty && !t.is(slot.start - 1, '{') {
            let rest = t.slice(b.end, t.len());
            let rest = jsstr::trim_start(&rest);
            return Some(Edit {
                text: format!("{}{{}} {rest}", t.slice(0, b.end)),
                caret: b.end + 3,
            });
        }
        let body = body_start(&t, &b);
        if body > b.end {
            return same(text, body);
        }
        if body < t.len() {
            return same(text, b.end);
        }
        return Some(splice(&t, b.end, b.end, " ", b.end + 1));
    }
    if key == "Backspace" {
        for b in &bounds {
            let lower_empty = b.lower.start == b.lower.end;
            let upper_empty = b.upper.is_none_or(|u| u.start == u.end);
            let body_at = body_start(&t, b);
            if caret == b.lower.start && b.end > b.sign + 1 {
                if lower_empty && upper_empty && body_at == t.len() {
                    return Some(splice(&t, b.sign + 1, t.len(), "", b.sign + 1));
                }
                return same(text, b.sign);
            }
            if let Some(u) = b.upper {
                if caret == u.start {
                    if !upper_empty || body_at < t.len() {
                        return same(text, b.lower.end);
                    }
                    let from = if t.is(b.lower.end, '}') {
                        b.lower.end + 1
                    } else {
                        b.lower.end
                    };
                    return Some(splice(&t, from, t.len(), "", b.lower.end));
                }
                if caret > u.end && caret == body_at {
                    // nothing typed in the body yet: the space goes, otherwise just step back into the limit
                    if caret == t.len() {
                        return Some(splice(&t, b.end, caret, "", u.end));
                    }
                    return same(text, u.end);
                }
            }
        }
        if hidden_char(&bounds, caret - 1) {
            return same(text, snap_offset(&bounds, caret - 1, -1, t.len()));
        }
        return None;
    }
    if key == "Delete" && hidden_char(&bounds, caret) {
        return same(text, snap_offset(&bounds, caret + 1, 1, t.len()));
    }
    None
}

/// `int`, `sum` or `prod` still waiting at the end, turned into its sign so → or tab can start the limits.
pub fn word_to_sign(text: &str, caret: i64) -> Option<Edit> {
    let t = U16::new(text);
    if caret != t.len() {
        return None;
    }
    // /(?<![\\A-Za-z_])(int|sum|prod)$/
    let (word, sign) = [("int", "∫"), ("sum", "Σ"), ("prod", "Π")]
        .into_iter()
        .find(|(w, _)| text.ends_with(w))?;
    let at = t.len() - word.len() as i64;
    if t.at(at - 1)
        .is_some_and(|c| c == '\\' || c.is_ascii_alphabetic() || c == '_')
    {
        return None;
    }
    Some(Edit {
        text: format!("{}{sign}", t.slice(0, at)),
        caret: at + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_limits() {
        let b = bounds_in("∫_0^1 x");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].lower, Slot { start: 2, end: 3 });
        assert_eq!(b[0].upper, Some(Slot { start: 4, end: 5 }));
        assert_eq!(
            after_typing("∫0", 2),
            Some(Edit {
                text: "∫_0".into(),
                caret: 3
            })
        );
        assert_eq!(word_to_sign("2 sum", 5).unwrap().text, "2 Σ");
    }
}
