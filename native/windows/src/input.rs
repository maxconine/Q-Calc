//! The input line: its text, caret, selection and undo (`Field`, pure and unit-tested), how it is drawn
//! (`FieldView`) and IME (`ime`). The port of InputField.swift and the typing rules of QuickInput.tsx.
//!
//! Positions are byte offsets into the UTF-8 text, always on char boundaries; DirectWrite and the engine's spans
//! count UTF-16, so `to_utf16` / `index_of_utf16` convert at the edges.
//!
//! Typing (`char16`): words become symbols as they're finished (text_rules.rs `pretty`, unless "keep words"), `[`
//! brings its `]` and `]` steps over one, `(` `[` `{` over a selection wraps it (and keeps it selected, one undo
//! step), a ∫ Σ Π sign followed by a character starts its lower limit (core `bounds::after_typing`). Keys that move
//! between a sign's limits (→, tab, ↑, ↓, ↵, space, `^`, backspace) go through `bound_key` first; the caret never
//! stops among a limit's hidden `_ ^ ..` (`snap_offset`). Undo is the core's (`app::undo`): typing and deleting
//! group into runs (a second apart at most), symbol swaps don't break them, a click or an arrow does.
//!
//! Drawing (`FieldView::paint`): the faint lead (inferred `(`s and the chained `ans`), the text with unit and
//! known-word colours, a radical's bar over its radicand, ∫ Σ Π limits small beside the sign (upper over lower,
//! empty ones as muted boxes), the blank-reason squiggle, the missing unit offered in a gap, the completion's ghost
//! and the inferred `)`s after the end, the IME composition underlined at the caret, and a blinking caret.

use qcalc_core::app::bounds::{self, Bound};
use qcalc_core::app::undo::{self as core_undo, EditKind, Undo, UndoState};

/// What the input draws over its text (BarModel.swift `Decorations`), from the engine thread with the live answer.
/// Offsets are UTF-16.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decorations {
    pub leading: usize,
    pub trailing: usize,
    pub units: Vec<(u32, u32)>,
    pub words: Vec<(u32, u32)>,
    /// where a blank answer stops reading (after typing pauses)
    pub squiggle: Option<(u32, u32)>,
    /// a unit offered where a number was owed one (missingUnit): drawn muted in a gap at `at`
    pub offer: Option<(u32, String)>,
}

impl Decorations {
    /// Whether every span still lies inside a text of `len16` units (old spans stay up until new ones land).
    pub fn fits(&self, len16: u32) -> bool {
        self.units
            .iter()
            .chain(&self.words)
            .chain(self.squiggle.iter())
            .all(|&(_, b)| b <= len16)
            && self.offer.as_ref().is_none_or(|(a, _)| *a <= len16)
    }
}

/// The ranges (UTF-16) under each √ or ∛: a bracketed group, or a run of letters, digits, `.` (radicandRanges).
pub fn radicand_ranges(s: &str) -> Vec<(u32, u32)> {
    let u: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < u.len() {
        if (u[i] == 0x221A || u[i] == 0x221B) && i + 1 < u.len() {
            let mut j = i + 1;
            if u[j] == 0x28 {
                let mut depth = 0;
                while j < u.len() {
                    if u[j] == 0x28 {
                        depth += 1;
                    }
                    if u[j] == 0x29 {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    j += 1;
                }
            } else {
                while j < u.len()
                    && (char::from_u32(u32::from(u[j])).is_some_and(char::is_alphanumeric)
                        || u[j] == 0x2E)
                {
                    j += 1;
                }
            }
            if j > i + 1 {
                out.push(((i + 1) as u32, j as u32));
            }
        }
        i += 1;
    }
    out
}

/// Whether `s` has a ∫ Σ Π sign: without one there are no limits and nothing to ask.
pub fn has_bound_sign(s: &str) -> bool {
    s.contains(|c| bounds::BOUND_SIGNS.contains(c))
}

fn is_message(s: &str) -> bool {
    s.trim_start().to_lowercase().starts_with("message")
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// The line being edited.
#[derive(Clone, Debug)]
pub struct Field {
    text: String,
    caret: usize,
    anchor: usize,
    undo: Undo,
    /// a high surrogate from WM_CHAR waiting for its low half
    pending_high: Option<u16>,
    /// the ∫ Σ Π limits in the text (`bounds_in`), kept with it
    limits: Vec<Bound>,
    /// "keep words": typed words stay words (settings.keepWords)
    pub keep_words: bool,
    clock: fn() -> f64,
}

impl Default for Field {
    fn default() -> Self {
        Field {
            text: String::new(),
            caret: 0,
            anchor: 0,
            undo: core_undo::undo_start(""),
            pending_high: None,
            limits: Vec::new(),
            keep_words: false,
            clock: now_ms,
        }
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

impl Field {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// (start, end) of the selection, in bytes; equal when there is none.
    pub fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    pub fn selected_text(&self) -> &str {
        let (a, b) = self.selection();
        &self.text[a..b]
    }

    pub fn caret_at_end(&self) -> bool {
        !self.has_selection() && self.caret == self.text.len()
    }

    /// The limits beside each ∫ Σ Π (UTF-16 offsets).
    pub fn limits(&self) -> &[Bound] {
        &self.limits
    }

    #[cfg(test)]
    fn set_clock(&mut self, clock: fn() -> f64) {
        self.clock = clock;
    }

    /// The text changed: its limits again.
    fn refreshed(&mut self) {
        self.limits = if has_bound_sign(&self.text) {
            bounds::bounds_in(&self.text)
        } else {
            Vec::new()
        };
    }

    /// Records the edit just made (undo.ts `recordEdit`): `kind` joins a run of the same kind within a second.
    fn recorded(&mut self, kind: Option<EditKind>) {
        self.refreshed();
        let (a, b) = self.selection();
        let next = UndoState {
            value: self.text.clone(),
            start: self.to_utf16(a) as usize,
            end: self.to_utf16(b) as usize,
        };
        self.undo = core_undo::record_edit(&self.undo, next, kind, (self.clock)());
    }

    /// Ends a typing run (the caret moved, the line was replaced).
    pub fn break_run(&mut self) {
        self.undo.run = None;
    }

    /// Replaces the whole line (a history insert, a commit's new input, an accepted offer); its own undo step.
    pub fn set_text(&mut self, text: &str) {
        if text == self.text {
            return;
        }
        self.text = text.to_string();
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.recorded(None);
    }

    /// Replaces the whole line with the caret at `caret` (bytes): the grey parens typed in (line.rs
    /// `fill_parens`); its own undo step.
    pub fn set_text_at(&mut self, text: &str, caret: usize) {
        if text == self.text {
            return;
        }
        let mut at = caret.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        self.replace_all(text.to_string(), at, None);
    }

    /// The whole line and the caret (bytes) after a rule's edit, recorded as `kind`.
    fn replace_all(&mut self, text: String, caret: usize, kind: Option<EditKind>) {
        self.text = text;
        self.caret = caret.min(self.text.len());
        self.anchor = self.caret;
        self.recorded(kind);
    }

    /// Clears the line and its undo (a new showing, a restored draft).
    pub fn reset(&mut self, text: &str) {
        let (keep, clock) = (self.keep_words, self.clock);
        *self = Field::default();
        self.keep_words = keep;
        self.clock = clock;
        self.text = text.to_string();
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.undo = core_undo::undo_start(text);
        self.refreshed();
    }

    /// Text in place of the selection, as typing would do it (history rows, accepted completions, the IME).
    pub fn insert(&mut self, s: &str) {
        let (a, b) = self.selection();
        self.text.replace_range(a..b, s);
        self.caret = a + s.len();
        self.anchor = self.caret;
        self.recorded(None);
    }

    /// The IME's committed text: typed, so it joins a typing run and becomes symbols.
    pub fn compose(&mut self, s: &str) {
        let (a, b) = self.selection();
        self.text.replace_range(a..b, s);
        self.caret = a + s.len();
        self.anchor = self.caret;
        self.after_typing();
        self.recorded(Some(EditKind::Insert));
    }

    /// Paste: cleaned of look-alikes, one line (line breaks and tabs as spaces); its own undo step.
    pub fn paste(&mut self, s: &str) {
        let one_line = crate::text_rules::flatten_paste(s);
        let one_line = one_line.trim_end_matches(' ');
        if !one_line.is_empty() {
            self.insert(one_line);
            self.break_run();
        }
    }

    /// prettyTokens and afterTyping, after a typed character.
    fn after_typing(&mut self) {
        let (t, c) = crate::text_rules::pretty(&self.text, self.caret, self.keep_words);
        if t != self.text {
            self.text = t;
            self.caret = c;
            self.anchor = c;
        }
        if has_bound_sign(&self.text) && !is_message(&self.text) {
            let caret16 = i64::from(self.to_utf16(self.caret));
            if let Some(e) = bounds::after_typing(&self.text, caret16) {
                self.text = e.text;
                self.caret = self.index_of_utf16(e.caret.max(0) as u32);
                self.anchor = self.caret;
            }
        }
    }

    /// One typed character.
    pub fn type_char(&mut self, c: char) {
        let (a, b) = self.selection();
        if let Some((t, s, e)) = crate::text_rules::wrap(&self.text, a, b, c) {
            self.text = t;
            self.anchor = s;
            self.caret = e;
            self.recorded(None);
            return;
        }
        if (c == '[' || c == ']')
            && let Some((t, caret)) =
                crate::text_rules::bracket_key(&self.text, a, b, if c == '[' { "[" } else { "]" })
        {
            self.replace_all(t, caret, Some(EditKind::Insert));
            return;
        }
        // a space or `^` that moves between a sign's limits
        if (c == ' ' || c == '^') && self.bound_key(if c == ' ' { " " } else { "^" }) {
            return;
        }
        self.text.replace_range(a..b, c.encode_utf8(&mut [0; 4]));
        self.caret = a + c.len_utf8();
        self.anchor = self.caret;
        self.after_typing();
        self.recorded(Some(EditKind::Insert));
    }

    /// WM_CHAR: one UTF-16 unit. Control characters are keys (handled on WM_KEYDOWN), not text.
    pub fn char16(&mut self, unit: u16) -> bool {
        match unit {
            0xD800..=0xDBFF => {
                self.pending_high = Some(unit);
                false
            }
            0xDC00..=0xDFFF => {
                let Some(high) = self.pending_high.take() else {
                    return false;
                };
                match char::decode_utf16([high, unit]).next() {
                    Some(Ok(c)) => {
                        self.type_char(c);
                        true
                    }
                    _ => false,
                }
            }
            u if u < 0x20 || u == 0x7F => false,
            u => {
                self.pending_high = None;
                match char::from_u32(u32::from(u)) {
                    Some(c) => {
                        self.type_char(c);
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// boundKey (and wordToSign for → and tab): a key that moves between a ∫ Σ Π's limits, with the caret alone.
    /// `key` is the web's name: "ArrowRight", "ArrowUp", "ArrowDown", "Tab", "Enter", "Backspace", " ", "^".
    /// True when it made the edit.
    pub fn bound_key(&mut self, key: &str) -> bool {
        if self.has_selection() {
            return false;
        }
        let caret16 = i64::from(self.to_utf16(self.caret));
        let mut edit = if self.limits.is_empty() {
            None
        } else {
            bounds::bound_key(&self.text, caret16, key)
        };
        if edit.is_none()
            && (key == "Tab" || key == "ArrowRight")
            && !self.keep_words
            && !is_message(&self.text)
        {
            edit = bounds::word_to_sign(&self.text, caret16);
        }
        let Some(e) = edit else { return false };
        let caret = self.index16_in(&e.text, e.caret.max(0) as u32);
        if e.text == self.text {
            self.caret = caret;
            self.anchor = caret;
            self.break_run();
        } else {
            self.replace_all(e.text, caret, None);
        }
        true
    }

    fn prev(&self, i: usize) -> usize {
        self.text[..i]
            .char_indices()
            .next_back()
            .map_or(0, |(j, _)| j)
    }

    fn next(&self, i: usize) -> usize {
        self.text[i..]
            .chars()
            .next()
            .map_or(i, |c| i + c.len_utf8())
    }

    /// Ctrl+←: back over spaces, then over a word (or a run of other symbols).
    pub fn word_left(&self, mut i: usize) -> usize {
        let ch = |i: usize| self.text[..i].chars().next_back();
        while ch(i).is_some_and(char::is_whitespace) {
            i = self.prev(i);
        }
        match ch(i) {
            Some(c) if is_word(c) => {
                while ch(i).is_some_and(is_word) {
                    i = self.prev(i);
                }
            }
            Some(_) => {
                while ch(i).is_some_and(|c| !is_word(c) && !c.is_whitespace()) {
                    i = self.prev(i);
                }
            }
            None => {}
        }
        i
    }

    /// Ctrl+→: over a word (or a run of symbols), then over spaces.
    pub fn word_right(&self, mut i: usize) -> usize {
        let ch = |i: usize| self.text[i..].chars().next();
        match ch(i) {
            Some(c) if is_word(c) => {
                while ch(i).is_some_and(is_word) {
                    i = self.next(i);
                }
            }
            Some(c) if !c.is_whitespace() => {
                while ch(i).is_some_and(|c| !is_word(c) && !c.is_whitespace()) {
                    i = self.next(i);
                }
            }
            _ => {}
        }
        while ch(i).is_some_and(char::is_whitespace) {
            i = self.next(i);
        }
        i
    }

    /// The caret never stops among a limit's hidden `_ ^ ..` (snapOffset), moving in `dir`.
    fn snapped(&self, i: usize, dir: i64) -> usize {
        if self.limits.is_empty() {
            return i;
        }
        let o = i64::from(self.to_utf16(i));
        if !bounds::is_hidden(&self.limits, o) {
            return i;
        }
        let len = i64::from(self.to_utf16(self.text.len()));
        let s = bounds::snap_offset(&self.limits, o, dir, len);
        self.index_of_utf16(s.max(0) as u32)
    }

    fn move_to(&mut self, i: usize, extend: bool) {
        let dir = if i >= self.caret { 1 } else { -1 };
        let i = if extend { i } else { self.snapped(i, dir) };
        self.caret = i;
        if !extend {
            self.anchor = i;
        }
        self.break_run();
    }

    pub fn left(&mut self, word: bool, extend: bool) {
        let i = if self.has_selection() && !extend {
            self.selection().0
        } else if word {
            self.word_left(self.caret)
        } else {
            self.prev(self.caret)
        };
        self.move_to(i, extend);
    }

    pub fn right(&mut self, word: bool, extend: bool) {
        let i = if self.has_selection() && !extend {
            self.selection().1
        } else if word {
            self.word_right(self.caret)
        } else {
            self.next(self.caret)
        };
        self.move_to(i, extend);
    }

    pub fn home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.move_to(self.text.len(), extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
        self.break_run();
    }

    /// The word around byte `i` (double-click).
    pub fn select_word_at(&mut self, i: usize) {
        let i = i.min(self.text.len());
        let start = self.word_left(self.word_right(i).min(self.next(i)));
        let end = self.word_right(start);
        let end = self.text[..end].trim_end().len().max(start);
        self.anchor = start;
        self.caret = end;
        self.break_run();
    }

    /// Backspace (Ctrl: the word before); false when there was nothing to delete.
    pub fn backspace(&mut self, word: bool) -> bool {
        if !self.has_selection() {
            if self.caret == 0 {
                return false;
            }
            if !word {
                if self.bound_key("Backspace") {
                    return true;
                }
                if let Some((t, c)) =
                    crate::text_rules::bracket_key(&self.text, self.caret, self.caret, "backspace")
                {
                    self.replace_all(t, c, Some(EditKind::Delete));
                    return true;
                }
            }
            self.anchor = if word {
                self.word_left(self.caret)
            } else {
                self.prev(self.caret)
            };
        }
        let (a, b) = self.selection();
        self.text.replace_range(a..b, "");
        self.caret = a;
        self.anchor = a;
        // a +- that meets at the caret becomes ±
        if let Some(t) = crate::text_rules::join_plus_minus(&self.text, a) {
            self.text = t;
            self.caret = a - 1 + '±'.len_utf8();
            self.anchor = self.caret;
        }
        self.recorded(Some(EditKind::Delete));
        true
    }

    /// Delete (Ctrl: the word after).
    pub fn delete(&mut self, word: bool) -> bool {
        if !self.has_selection() {
            if self.caret == self.text.len() {
                return false;
            }
            if !word && self.bound_key("Delete") {
                return true;
            }
            self.anchor = if word {
                self.word_right(self.caret)
            } else {
                self.next(self.caret)
            };
        }
        let (a, b) = self.selection();
        self.text.replace_range(a..b, "");
        self.caret = a;
        self.anchor = a;
        self.recorded(Some(EditKind::Delete));
        true
    }

    /// Ctrl+X: the cut text.
    pub fn cut(&mut self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        let s = self.selected_text().to_string();
        let (a, b) = self.selection();
        self.text.replace_range(a..b, "");
        self.caret = a;
        self.anchor = a;
        self.recorded(None);
        Some(s)
    }

    pub fn undo(&mut self) -> bool {
        match core_undo::undo(&self.undo) {
            Some(u) => {
                self.undo = u;
                self.restore();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match core_undo::redo(&self.undo) {
            Some(u) => {
                self.undo = u;
                self.restore();
                true
            }
            None => false,
        }
    }

    fn restore(&mut self) {
        let s = &self.undo.current;
        self.text = s.value.clone();
        let (a, b) = (s.start as u32, s.end as u32);
        self.anchor = self.index_of_utf16(a);
        self.caret = self.index_of_utf16(b);
        self.refreshed();
    }

    /// Byte offset → UTF-16 position.
    pub fn to_utf16(&self, i: usize) -> u32 {
        self.text[..i.min(self.text.len())].encode_utf16().count() as u32
    }

    fn index16_in(&self, text: &str, pos: u32) -> usize {
        let mut n = 0u32;
        for (i, c) in text.char_indices() {
            if n >= pos {
                return i;
            }
            n += c.len_utf16() as u32;
        }
        text.len()
    }

    /// UTF-16 position → byte offset (on a char boundary).
    pub fn index_of_utf16(&self, pos: u32) -> usize {
        self.index16_in(&self.text, pos)
    }

    /// The caret at a UTF-16 position (a click); `extend` keeps the anchor (drag, Shift+click).
    pub fn set_caret_utf16(&mut self, pos: u32, extend: bool) {
        let i = self.index_of_utf16(pos);
        self.move_to(i, extend);
    }
}

#[cfg(windows)]
pub use view::*;

#[cfg(windows)]
mod view {
    use super::{Decorations, Field, radicand_ranges};
    use crate::render::{Canvas, Color, Rect, caret_x, hit, measure, size, with_alpha};
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_TEXT_RANGE, IDWriteTextLayout, IDWriteTextLayout1,
    };
    use windows::core::Interface;

    /// The IME's composition in progress: its text and where its cursor is (UTF-16 within it).
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Composition {
        pub text: String,
        pub cursor: u32,
    }

    /// What `paint` draws besides the field: the bar's state for this frame.
    pub struct Extras<'a> {
        pub deco: Option<&'a Decorations>,
        /// the faint `ans` a chained line continues from
        pub chained: bool,
        pub completion: Option<&'a str>,
        /// the empty bar's rotating example (or "Calculate")
        pub placeholder: &'a str,
        /// the caret's blink phase: false draws no caret this frame
        pub caret_on: bool,
    }

    #[derive(PartialEq)]
    struct Key {
        text: String,
        offer: Option<(u32, String)>,
        bounds_on: bool,
    }

    /// Small layouts drawn every frame (ghost parens, `ans`, the completion, the placeholder) kept a while.
    const GHOSTS: usize = 12;

    /// How the field is drawn: its scroll and the layout of the current text (made again only when it changes).
    #[derive(Default)]
    pub struct FieldView {
        /// how far the text is scrolled left, DIPs
        pub scroll: f32,
        cached: Option<(Key, IDWriteTextLayout)>,
        /// the text's left edge at the last paint (after the lead), window DIPs
        text_x: f32,
        /// the field's rect at the last paint (for clicks and the IME window)
        pub rect: Rect,
        /// the caret's x at the last paint, window DIPs
        pub caret_at: f32,
        /// the IME's composition, drawn inline at the caret
        pub composition: Option<Composition>,
        ghosts: Vec<(String, IDWriteTextLayout)>,
        /// the faint `ans`'s box at the last paint, window DIPs (its hover label, word_tip.rs)
        pub ans_rect: Option<Rect>,
    }

    /// One limit to draw small beside its sign: its text, where it goes, and which.
    struct Limit {
        start: u32,
        end: u32,
        upper: bool,
    }

    fn range(a: u32, b: u32) -> DWRITE_TEXT_RANGE {
        DWRITE_TEXT_RANGE {
            startPosition: a,
            length: b.saturating_sub(a),
        }
    }

    const LIMIT_SCALE: f32 = 0.56;
    const HIDDEN_SIZE: f32 = 0.01;

    impl FieldView {
        /// Where UTF-16 `a..b` of the text was drawn at the last paint: (left, right), window DIPs (word_tip.rs).
        pub fn span_x(&self, a: u32, b: u32) -> Option<(f32, f32)> {
            let (_, l) = self.cached.as_ref()?;
            Some((self.text_x + caret_x(l, a), self.text_x + caret_x(l, b)))
        }

        /// A small one-line layout at the display size, from the cache when it was drawn lately.
        fn ghost(&mut self, c: &Canvas, s: &str, h: f32) -> Option<IDWriteTextLayout> {
            if let Some(i) = self.ghosts.iter().position(|(t, _)| t == s) {
                let g = self.ghosts.remove(i);
                let l = g.1.clone();
                self.ghosts.insert(0, g);
                return Some(l);
            }
            let l = c.gfx.layout(s, &c.gfx.fonts.display, 0.0, h)?;
            self.ghosts.insert(0, (s.to_string(), l.clone()));
            self.ghosts.truncate(GHOSTS);
            Some(l)
        }

        /// The text's layout: unit / known-word colours are drawn over it; a sign's limits shrink in it, their
        /// separators all but vanish, and an offered unit's gap opens after its number.
        fn layout(
            &mut self,
            c: &Canvas,
            field: &Field,
            deco: Option<&Decorations>,
            h: f32,
        ) -> Option<IDWriteTextLayout> {
            let key = Key {
                text: field.text().to_string(),
                offer: deco.and_then(|d| d.offer.clone()),
                bounds_on: !field.limits().is_empty(),
            };
            if let Some((k, l)) = &self.cached
                && *k == key
            {
                return Some(l.clone());
            }
            let l = c.gfx.layout(field.text(), &c.gfx.fonts.display, 0.0, h)?;
            let l1: Option<IDWriteTextLayout1> = l.cast().ok();
            // SAFETY: setters on a live layout, ranges inside the text
            unsafe {
                for b in field.limits() {
                    let (sign, end) = (b.sign as u32, b.end as u32);
                    let lower = (b.lower.start as u32, b.lower.end as u32);
                    let upper = b.upper.map(|u| (u.start as u32, u.end as u32));
                    let _ = l.SetFontSize(size::DISPLAY * HIDDEN_SIZE, range(sign + 1, end));
                    let small = size::DISPLAY * LIMIT_SCALE;
                    let _ = l.SetFontSize(small, range(lower.0, lower.1));
                    if let Some(u) = upper {
                        let _ = l.SetFontSize(small, range(u.0, u.1));
                    }
                    // the upper limit sits over the lower: its first character pulls the pen back under it
                    let lw = text_width(c, field.text(), lower, small);
                    let uw = upper.map_or(0.0, |u| text_width(c, field.text(), u, small));
                    let empty = small * 0.62;
                    if let Some(l1) = &l1 {
                        match upper {
                            Some(u) if u.1 > u.0 => {
                                let _ = l1.SetCharacterSpacing(-lw, 0.0, 0.0, range(u.0, u.0 + 1));
                                let room = lw.max(uw) - uw + 2.0;
                                let _ = l1.SetCharacterSpacing(0.0, room, 0.0, range(u.1 - 1, u.1));
                            }
                            _ => {
                                // an empty slot's box keeps its room after the sign
                                let at = end.saturating_sub(1).max(sign);
                                let room = if lw > 0.0 {
                                    (empty - 0.0).max(2.0)
                                } else {
                                    empty * 2.0 + 4.0
                                };
                                let _ = l1.SetCharacterSpacing(0.0, room, 0.0, range(at, at + 1));
                            }
                        }
                    }
                }
                if let Some(d) = deco
                    && let (Some((at, text)), Some(l1)) = (&d.offer, &l1)
                    && *at > 0
                {
                    let w = c
                        .gfx
                        .layout(text, &c.gfx.fonts.display, 0.0, h)
                        .map_or(0.0, |o| measure(&o).0);
                    let _ = l1.SetCharacterSpacing(0.0, w, 0.0, range(at - 1, *at));
                }
            }
            self.cached = Some((key, l.clone()));
            Some(l)
        }

        /// Draws the line in `r`.
        pub fn paint(&mut self, c: &Canvas, field: &Field, r: Rect, focused: bool, x: &Extras) {
            self.rect = r;
            let t = c.theme;
            c.push_clip(Rect::new(r.x - 2.0, r.y - 6.0, r.w + 4.0, r.h + 12.0));
            let len16 = field.to_utf16(field.text().len());
            let deco = x.deco.filter(|d| d.fits(len16));
            // the faint lead: the inferred `(`s, then the chained `ans`
            let mut lead_w = 0.0;
            let leading = deco.map_or(0, |d| d.leading);
            if !field.text().is_empty() && (leading > 0 || x.chained) {
                let parens = "(".repeat(leading);
                if !parens.is_empty()
                    && let Some(l) = self.ghost(c, &parens, r.h)
                {
                    c.text_plain(&l, r.x, r.y, t.muted);
                    lead_w += measure(&l).0;
                }
                // drawn as Tab types it (line.rs `ans_ghost`), with no gap after it, so typing it in moves nothing
                let ghost = if x.chained {
                    crate::line::ans_ghost(field.text())
                } else {
                    String::new()
                };
                if !ghost.is_empty()
                    && let Some(l) = self.ghost(c, &ghost, r.h)
                {
                    c.text_plain(&l, r.x + lead_w, r.y, with_alpha(t.muted, 0.75));
                    let w = measure(&l).0;
                    self.ans_rect = Some(Rect::new(r.x + lead_w, r.y, w, r.h));
                    lead_w += w;
                }
            }
            if !x.chained {
                self.ans_rect = None;
            }
            let r = Rect::new(r.x + lead_w, r.y, (r.w - lead_w).max(1.0), r.h);
            if field.text().is_empty() && self.composition.is_none() {
                self.scroll = 0.0;
                if !x.placeholder.is_empty()
                    && let Some(l) = self.ghost(c, x.placeholder, r.h)
                {
                    c.text_plain(&l, r.x, r.y, with_alpha(t.muted, 0.8));
                    for (a, b) in radicand_ranges(x.placeholder) {
                        radical_bar(c, &l, r.x, r.y, a, b, with_alpha(t.muted, 0.8));
                    }
                }
            }
            let Some(layout) = self.layout(c, field, deco, r.h) else {
                c.pop_clip();
                return;
            };
            let caret16 = field.to_utf16(field.caret());
            let comp_w = self.composition.as_ref().map_or(0.0, |comp| {
                c.gfx
                    .layout(&comp.text, &c.gfx.fonts.display, 0.0, r.h)
                    .map_or(0.0, |l| measure(&l).0)
            });
            let caret = caret_x(&layout, caret16);
            let (w, _) = measure(&layout);
            let ghost = x
                .completion
                .filter(|_| field.caret_at_end() && self.composition.is_none());
            // keep the caret (and a composition) in sight, with a little room past it
            let room = 4.0;
            let want = caret + comp_w;
            if want - self.scroll > r.w - room {
                self.scroll = want - r.w + room;
            }
            if caret - self.scroll < 0.0 {
                self.scroll = caret;
            }
            if w + comp_w - self.scroll < r.w - room {
                self.scroll = (w + comp_w - r.w + room).max(0.0);
            }
            let x0 = r.x - self.scroll;
            self.text_x = x0;
            if field.has_selection() {
                let (a, b) = field.selection();
                let (xa, xb) = (
                    caret_x(&layout, field.to_utf16(a)),
                    caret_x(&layout, field.to_utf16(b)),
                );
                c.fill_round(
                    Rect::new(x0 + xa, r.y + r.h * 0.5 - 15.0, xb - xa, 30.0),
                    3.0,
                    t.selection,
                );
            }
            // a composition opens a gap at the caret: the text after it moves right
            let split = self.composition.is_some().then_some(caret);
            let draw = |color: Color, from: f32, to: f32| {
                if let Some(at) = split {
                    clip_x(c, x0 + from, x0 + to.min(at), r, || {
                        c.text(&layout, x0, r.y, color)
                    });
                    clip_x(c, x0 + from.max(at) + comp_w, x0 + to + comp_w, r, || {
                        c.text(&layout, x0 + comp_w, r.y, color)
                    });
                } else if from < -999.0 {
                    c.text(&layout, x0, r.y, color);
                } else {
                    clip_x(c, x0 + from, x0 + to, r, || c.text(&layout, x0, r.y, color));
                }
            };
            if !field.text().is_empty() {
                // .quick-word in accent mixed 85% with ink, .quick-unit in the accent: drawing effects on the
                // layout, so the line is still one draw
                // SAFETY: setters on a live layout, ranges inside the text
                unsafe {
                    let _ = layout.SetDrawingEffect(None, range(0, len16));
                    if let Some(d) = deco.filter(|d| !d.words.is_empty() || !d.units.is_empty()) {
                        if !d.words.is_empty()
                            && let Some(b) = c.own_brush(mix(t.accent, t.ink, 0.15))
                        {
                            for &(a, z) in &d.words {
                                let _ = layout.SetDrawingEffect(&b, range(a, z));
                            }
                        }
                        if !d.units.is_empty()
                            && let Some(b) = c.own_brush(t.accent)
                        {
                            for &(a, z) in &d.units {
                                let _ = layout.SetDrawingEffect(&b, range(a, z));
                            }
                        }
                    }
                }
                draw(t.ink, -1000.0, w + 1000.0);
            }
            let shift = |p: u32| {
                if split.is_some() && p >= caret16 {
                    comp_w
                } else {
                    0.0
                }
            };
            // a radical's bar over what it takes
            for (a, b) in radicand_ranges(field.text()) {
                radical_bar(c, &layout, x0 + shift(a), r.y, a, b, t.ink);
            }
            self.limits(c, field, &layout, x0, r);
            if let Some(d) = deco {
                // .quick-squiggle: a wavy line, muted at 70%, under where a blank answer stops reading
                if let Some((a, b)) = d.squiggle {
                    let (xa, xb) = (caret_x(&layout, a), caret_x(&layout, b));
                    squiggle(
                        c,
                        x0 + xa,
                        x0 + xb,
                        r.y + r.h * 0.5 + 13.0,
                        with_alpha(t.muted, 0.7),
                    );
                }
                // the unit offered after its number, muted, in the gap opened for it
                if let Some((at, text)) = &d.offer
                    && *at > 0
                    && let Some(l) = self.ghost(c, text, r.h)
                {
                    let ow = measure(&l).0;
                    let end = caret_x(&layout, *at);
                    c.text_plain(&l, x0 + end - ow, r.y, t.muted);
                }
            }
            // after the end: the completion's rest (caret at the end), then the inferred `)`s
            let mut tail = x0 + w + comp_w;
            if let Some(g) = ghost
                && let Some(l) = self.ghost(c, g, r.h)
            {
                c.text_plain(&l, tail, r.y, with_alpha(t.muted, 0.8));
                tail += measure(&l).0;
            }
            let trailing = deco.map_or(0, |d| d.trailing);
            if trailing > 0
                && !field.text().is_empty()
                && let Some(l) = self.ghost(c, &")".repeat(trailing), r.h)
            {
                c.text_plain(&l, tail, r.y, t.muted);
            }
            // the IME's composition, underlined, at the caret
            if let Some(comp) = &self.composition
                && let Some(l) = c.gfx.layout(&comp.text, &c.gfx.fonts.display, 0.0, r.h)
            {
                let cx = x0 + caret;
                c.text(&l, cx, r.y, t.ink);
                let base = r.y + r.h * 0.5 + 12.0;
                c.line(cx, base, cx + comp_w, base, 1.0, t.ink);
                self.caret_at = cx + caret_x(&l, comp.cursor);
            } else {
                self.caret_at = x0 + caret;
            }
            if focused && x.caret_on {
                let mid = r.y + r.h * 0.5;
                let half = size::DISPLAY * 0.62;
                c.line(
                    self.caret_at + 0.5,
                    mid - half,
                    self.caret_at + 0.5,
                    mid + half,
                    1.5,
                    // grey, never the accent: a green caret read as a selection or a mode
                    t.muted,
                );
            }
            c.pop_clip();
        }

        /// ∫ Σ Π limits: drawn small beside the sign, upper over lower; empty ones as muted boxes.
        fn limits(&self, c: &Canvas, field: &Field, layout: &IDWriteTextLayout, x0: f32, r: Rect) {
            let limits = field.limits();
            if limits.is_empty() {
                return;
            }
            let t = c.theme;
            let small = size::DISPLAY * LIMIT_SCALE;
            let mid = r.y + r.h * 0.5;
            let box_w = small * 0.62;
            let box_h = small * 0.8;
            let mut slots: Vec<(Limit, f32)> = Vec::new();
            for b in limits {
                let sign_end = caret_x(layout, b.sign as u32 + 1);
                slots.push((
                    Limit {
                        start: b.lower.start as u32,
                        end: b.lower.end as u32,
                        upper: false,
                    },
                    sign_end,
                ));
                let (us, ue) = b.upper.map_or((b.end as u32, b.end as u32), |u| {
                    (u.start as u32, u.end as u32)
                });
                slots.push((
                    Limit {
                        start: us,
                        end: ue,
                        upper: true,
                    },
                    sign_end,
                ));
            }
            // the limits' text was laid out small and kept in the line (so the caret has its places); it is
            // drawn again raised or lowered, over a background-coloured patch of the line
            for (s, _) in &slots {
                if s.end > s.start {
                    let xa = x0 + caret_x(layout, s.start);
                    let xb = x0 + caret_x(layout, s.end);
                    c.fill(Rect::new(xa.min(xb), r.y, (xb - xa).abs(), r.h), t.bg);
                }
            }
            for (s, sign_end) in &slots {
                if s.end > s.start {
                    let u: Vec<u16> = field
                        .text()
                        .encode_utf16()
                        .skip(s.start as usize)
                        .take((s.end - s.start) as usize)
                        .collect();
                    let text = String::from_utf16_lossy(&u);
                    let xa = x0 + caret_x(layout, s.start);
                    let y = if s.upper {
                        mid - small * 1.15
                    } else {
                        mid - small * 0.15
                    };
                    if let Some(l) = c.gfx.layout(&text, &c.gfx.fonts.display, 0.0, small * 1.3) {
                        // SAFETY: a setter on a live layout
                        unsafe {
                            let _ = l.SetFontSize(small, range(0, s.end - s.start));
                        }
                        c.text(&l, xa, y, t.ink);
                    }
                } else {
                    let bx = x0 + sign_end + if s.upper { small * 0.3 } else { small * 0.05 };
                    let by = if s.upper {
                        mid - small * 0.95
                    } else {
                        mid + small * 0.05
                    };
                    c.fill_round(
                        Rect::new(bx, by, box_w, box_h),
                        small * 0.14,
                        with_alpha(t.muted, 0.18),
                    );
                }
            }
        }

        /// The UTF-16 position under window DIP point (x, y) (clicks), from the last layout.
        pub fn hit(&self, x: f32, y: f32) -> u32 {
            match &self.cached {
                Some((_, l)) => hit(l, x - self.text_x, y - self.rect.y),
                None => 0,
            }
        }
    }

    fn text_width(c: &Canvas, text: &str, (a, b): (u32, u32), size: f32) -> f32 {
        if b <= a {
            return 0.0;
        }
        let u: Vec<u16> = text
            .encode_utf16()
            .skip(a as usize)
            .take((b - a) as usize)
            .collect();
        let s = String::from_utf16_lossy(&u);
        let Some(l) = c.gfx.layout(&s, &c.gfx.fonts.display, 0.0, 40.0) else {
            return 0.0;
        };
        // SAFETY: a setter on a live layout
        unsafe {
            let _ = l.SetFontSize(size, range(0, b - a));
        }
        measure(&l).0
    }

    fn clip_x(c: &Canvas, xa: f32, xb: f32, r: Rect, f: impl FnOnce()) {
        if xb <= xa {
            return;
        }
        c.push_clip(Rect::new(xa, r.y - 6.0, xb - xa, r.h + 12.0));
        f();
        c.pop_clip();
    }

    fn mix(a: Color, b: Color, t: f32) -> Color {
        Color {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }

    /// The bar over a radicand (UTF-16 range a..b), as Radical.tsx draws it.
    fn radical_bar(
        c: &Canvas,
        l: &IDWriteTextLayout,
        x0: f32,
        y: f32,
        a: u32,
        b: u32,
        color: Color,
    ) {
        let (xa, xb) = (caret_x(l, a), caret_x(l, b));
        let top = y + 36.0 * 0.5 - size::DISPLAY * 0.5;
        c.fill(Rect::new(x0 + xa, top, xb - xa, 1.3), color);
    }

    /// A wavy underline from xa to xb at y.
    fn squiggle(c: &Canvas, xa: f32, xb: f32, y: f32, color: Color) {
        let step = 2.0;
        let mut x = xa;
        let mut up = true;
        while x < xb {
            let nx = (x + step).min(xb);
            let (y0, y1) = if up { (y, y - 1.5) } else { (y - 1.5, y) };
            c.line(x, y0, nx, y1, 1.0, color);
            x = nx;
            up = !up;
        }
    }
}

/// IME: the composition is drawn inline (underlined at the caret); the candidate window is placed under it; the
/// result string is inserted as typed text.
#[cfg(windows)]
pub mod ime {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::Input::Ime::{
        CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM, GCS_COMPSTR, GCS_CURSORPOS,
        GCS_RESULTSTR, IME_COMPOSITION_STRING, ImmGetCompositionStringW, ImmGetContext,
        ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow,
    };

    thread_local! {
        /// where the IME's windows were last put (window, x, y)
        static PLACED: std::cell::Cell<Option<(usize, i32, i32)>> = const { std::cell::Cell::new(None) };
    }

    /// `place`, skipped when the spot is the one already set: every frame asks, and the IME calls cost ~0.1 ms
    /// (PERFORMANCE.md).
    pub fn place_if_moved(hwnd: HWND, x: i32, y: i32) {
        if PLACED.with(|p| p.get()) != Some((hwnd.0 as usize, x, y)) {
            place(hwnd, x, y);
        }
    }

    /// The next `place_if_moved` places again (a composition starts, the window was activated).
    pub fn forget() {
        PLACED.with(|p| p.set(None));
    }

    /// Puts the IME's composition and candidate windows at the caret (window pixels; `y` the line's bottom).
    pub fn place(hwnd: HWND, x: i32, y: i32) {
        PLACED.with(|p| p.set(Some((hwnd.0 as usize, x, y))));
        // SAFETY: the context is released below; the forms are live locals
        unsafe {
            let imc = ImmGetContext(hwnd);
            if imc.is_invalid() {
                return;
            }
            let form = COMPOSITIONFORM {
                dwStyle: CFS_POINT,
                ptCurrentPos: POINT { x, y },
                rcArea: RECT::default(),
            };
            let _ = ImmSetCompositionWindow(imc, &form);
            let cand = CANDIDATEFORM {
                dwIndex: 0,
                dwStyle: CFS_CANDIDATEPOS,
                ptCurrentPos: POINT { x, y },
                rcArea: RECT::default(),
            };
            let _ = ImmSetCandidateWindow(imc, &cand);
            let _ = ImmReleaseContext(hwnd, imc);
        }
    }

    fn string(hwnd: HWND, which: IME_COMPOSITION_STRING) -> Option<String> {
        // SAFETY: the context is released below; the buffer is sized from the first call
        unsafe {
            let imc = ImmGetContext(hwnd);
            if imc.is_invalid() {
                return None;
            }
            let bytes = ImmGetCompositionStringW(imc, which, None, 0);
            let out = if bytes > 0 {
                let mut buf = vec![0u16; bytes as usize / 2];
                let got = ImmGetCompositionStringW(
                    imc,
                    which,
                    Some(buf.as_mut_ptr().cast()),
                    bytes as u32,
                );
                (got > 0).then(|| String::from_utf16_lossy(&buf[..got as usize / 2]))
            } else {
                None
            };
            let _ = ImmReleaseContext(hwnd, imc);
            out
        }
    }

    /// WM_IME_COMPOSITION with GCS_RESULTSTR: the committed text.
    pub fn result(hwnd: HWND) -> Option<String> {
        string(hwnd, GCS_RESULTSTR)
    }

    /// WM_IME_COMPOSITION with GCS_COMPSTR: the composition so far and its cursor (UTF-16).
    pub fn composition(hwnd: HWND) -> (String, u32) {
        let text = string(hwnd, GCS_COMPSTR).unwrap_or_default();
        // SAFETY: as above; GCS_CURSORPOS returns the position, no buffer
        let cursor = unsafe {
            let imc = ImmGetContext(hwnd);
            if imc.is_invalid() {
                0
            } else {
                let p = ImmGetCompositionStringW(imc, GCS_CURSORPOS, None, 0);
                let _ = ImmReleaseContext(hwnd, imc);
                p.max(0) as u32
            }
        };
        let len = text.encode_utf16().count() as u32;
        (text, cursor.min(len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> f64 {
        1000.0
    }

    fn typed(s: &str) -> Field {
        let mut f = Field::default();
        f.set_clock(fixed);
        for u in s.encode_utf16() {
            f.char16(u);
        }
        f
    }

    #[test]
    fn typing_and_backspace() {
        let mut f = typed("2+2");
        assert_eq!(f.text(), "2+2");
        assert!(f.backspace(false));
        assert_eq!(f.text(), "2+");
        f.home(false);
        assert!(!f.backspace(false));
        assert!(f.delete(false));
        assert_eq!(f.text(), "+");
    }

    #[test]
    fn control_characters_are_not_text_and_surrogates_join() {
        let mut f = typed("a\u{8}\u{1}\u{7f}b");
        assert_eq!(f.text(), "ab");
        f = typed("x😀y");
        assert_eq!(f.text(), "x😀y");
        assert_eq!(f.to_utf16(f.text().len()), 4);
        assert_eq!(f.index_of_utf16(3), "x😀".len());
    }

    #[test]
    fn selection_with_shift_and_replace() {
        let mut f = typed("sin(90)");
        f.left(false, true);
        f.left(false, true);
        assert_eq!(f.selected_text(), "0)");
        f.insert("x)");
        assert_eq!(f.text(), "sin(9x)");
        f.select_all();
        assert_eq!(f.selected_text(), "sin(9x)");
        assert_eq!(f.cut().as_deref(), Some("sin(9x)"));
        assert_eq!(f.text(), "");
    }

    #[test]
    fn word_moves_and_deletes() {
        let mut f = typed("72 f to c");
        f.left(true, false);
        assert_eq!(f.caret(), "72 f to ".len());
        f.left(true, false);
        f.left(true, false);
        assert_eq!(f.caret(), "72 ".len());
        f.right(true, false);
        assert_eq!(f.caret(), "72 f ".len());
        f.end(false);
        assert!(f.backspace(true));
        assert_eq!(f.text(), "72 f to ");
        f.home(false);
        assert!(f.delete(true));
        assert_eq!(f.text(), "f to ");
        // symbols are their own word
        let mut g = typed("(2)+3");
        g.left(true, false);
        assert_eq!(g.caret(), "(2)+".len());
        g.left(true, false);
        assert_eq!(g.caret(), "(2".len());
    }

    #[test]
    fn a_left_arrow_collapses_a_selection_to_its_start() {
        let mut f = typed("12345");
        f.left(false, true);
        f.left(false, true);
        f.left(false, false);
        assert_eq!(f.caret(), 3);
        assert!(!f.has_selection());
    }

    #[test]
    fn undo_takes_back_a_typing_run_at_once() {
        let mut f = typed("12 34");
        f.break_run();
        for u in "+5".encode_utf16() {
            f.char16(u);
        }
        assert_eq!(f.text(), "12 34+5");
        assert!(f.undo());
        assert_eq!(f.text(), "12 34");
        assert!(f.undo());
        assert_eq!(f.text(), "");
        assert!(f.redo());
        assert_eq!(f.text(), "12 34");
        f.set_text("9");
        assert!(f.undo());
        assert_eq!(f.text(), "12 34");
        // deleting is its own run
        f.end(false);
        f.backspace(false);
        f.backspace(false);
        assert!(f.undo());
        assert_eq!(f.text(), "12 34");
    }

    #[test]
    fn words_become_symbols_as_typed_unless_kept() {
        let f = typed("sqrt(2)");
        assert_eq!(f.text(), "√(2)");
        assert_eq!(f.caret(), f.text().len());
        let f = typed("2pi r");
        assert_eq!(f.text(), "2π r");
        let mut k = Field {
            keep_words: true,
            ..Field::default()
        };
        for u in "sqrt pi".encode_utf16() {
            k.char16(u);
        }
        assert_eq!(k.text(), "sqrt pi");
        // one undo takes the run back, symbols and all
        let mut f = typed("pi");
        assert!(f.undo());
        assert_eq!(f.text(), "");
    }

    #[test]
    fn brackets_pair_and_wrap_a_selection_in_one_step() {
        let mut f = typed("2[");
        assert_eq!(f.text(), "2[]");
        assert_eq!(f.caret(), 2);
        f.char16(u16::from(b'1'));
        f.char16(u16::from(b']'));
        assert_eq!(f.text(), "2[1]");
        assert_eq!(f.caret(), 4);
        let mut g = typed("2+3");
        g.select_all();
        g.char16(u16::from(b'('));
        assert_eq!(g.text(), "(2+3)");
        assert_eq!(g.selected_text(), "2+3");
        assert!(g.undo());
        assert_eq!(g.text(), "2+3");
        let mut h = typed("[");
        assert!(h.backspace(false));
        assert_eq!(h.text(), "");
    }

    #[test]
    fn a_sign_starts_its_limits_and_keys_move_between_them() {
        let mut f = typed("sum0");
        assert_eq!(f.text(), "Σ_0");
        assert!(f.bound_key("Tab"));
        assert_eq!(f.text(), "Σ_0^");
        for u in "5".encode_utf16() {
            f.char16(u);
        }
        assert_eq!(f.text(), "Σ_0^5");
        assert_eq!(f.limits().len(), 1);
        // the caret never stops on a hidden `^`
        f.left(false, false);
        f.left(false, false);
        assert!(!bounds::is_hidden(
            f.limits(),
            i64::from(f.to_utf16(f.caret()))
        ));
    }

    #[test]
    fn paste_is_one_line_and_one_step() {
        let mut f = Field::default();
        f.paste("1\r\n+2\t*3\n");
        assert_eq!(f.text(), "1 +2 *3");
        assert!(f.undo());
        assert_eq!(f.text(), "");
    }

    #[test]
    fn double_click_selects_the_word() {
        let mut f = typed("2 * x + 1");
        f.select_word_at(5);
        assert_eq!(f.selected_text(), "x");
        f.select_word_at(0);
        assert_eq!(f.selected_text(), "2");
    }

    #[test]
    fn radicands_and_decorations_that_fit() {
        assert_eq!(radicand_ranges("2√2+√(1+x)"), vec![(2, 3), (5, 10)]);
        let d = Decorations {
            units: vec![(2, 4)],
            ..Decorations::default()
        };
        assert!(d.fits(4));
        assert!(!d.fits(3));
    }
}
