//! The input field: the line's text, caret, selection and undo (`Field`, pure and unit-tested), how it is drawn
//! (`FieldView`: horizontal scroll that keeps the caret in sight, selection, caret, placeholder) and IME (`ime`).
//!
//! Positions are byte offsets into the UTF-8 text, always on char boundaries; DirectWrite and the engine's spans
//! count UTF-16, so `to_utf16` / `index_of_utf16` convert at the edges.
//!
//! Keys (model.rs routes them here first): ← → (Ctrl: by word, Shift: extend), Home / End, Backspace / Delete
//! (Ctrl: a word), Ctrl+A / C / X / V / Z / Y (and Ctrl+Shift+Z), typed characters via WM_CHAR (surrogate pairs
//! joined), the IME's result string via WM_IME_COMPOSITION. The composition itself is drawn by the system IME
//! window, placed at the caret (`ime::place`). Mouse: click places the caret, drag and Shift+click select,
//! double-click selects a word.
//!
//! Later work owned here: the pretty display of the line (`Symbols.pretty`: `*` as `×`, `sqrt` as `√`...), the
//! inferred parens and unit / known-word colours (`Decorations` in BarModel.swift), the squiggle under a blank
//! answer's span, the completion's ghost text, a blinking caret.

/// Undo steps kept.
const UNDO_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: usize,
}

/// The line being edited.
#[derive(Clone, Debug)]
pub struct Field {
    text: String,
    caret: usize,
    anchor: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: EditKind,
    /// a high surrogate from WM_CHAR waiting for its low half
    pending_high: Option<u16>,
}

impl Default for Field {
    fn default() -> Self {
        Field {
            text: String::new(),
            caret: 0,
            anchor: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            last: EditKind::Other,
            pending_high: None,
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

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            caret: self.caret,
            anchor: self.anchor,
        }
    }

    /// Records the state before an edit; runs of typing (or of deleting) are one undo step.
    fn record(&mut self, kind: EditKind) {
        if kind == EditKind::Other || kind != self.last || self.undo.is_empty() {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last = kind;
    }

    /// Ends a typing run (the caret moved, the line was replaced).
    pub fn break_run(&mut self) {
        self.last = EditKind::Other;
    }

    /// Replaces the whole line (a history insert, a commit's new input, the bench); undoable.
    pub fn set_text(&mut self, text: &str) {
        if text == self.text {
            return;
        }
        self.record(EditKind::Other);
        self.text = text.to_string();
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.last = EditKind::Other;
    }

    /// Clears the line and its undo (a new showing).
    pub fn reset(&mut self, text: &str) {
        *self = Field::default();
        self.text = text.to_string();
        self.caret = self.text.len();
        self.anchor = self.caret;
    }

    /// Typed or pasted text in place of the selection.
    pub fn insert(&mut self, s: &str) {
        let kind = if s.chars().count() == 1 && !self.has_selection() {
            EditKind::Typing
        } else {
            EditKind::Other
        };
        // a space ends a typing run, as in most editors
        self.record(if s == " " { EditKind::Other } else { kind });
        let (a, b) = self.selection();
        self.text.replace_range(a..b, s);
        self.caret = a + s.len();
        self.anchor = self.caret;
    }

    /// Paste: line breaks become spaces (the bar is one line), tabs too.
    pub fn paste(&mut self, s: &str) {
        let one_line: String = s
            .trim_end_matches(['\r', '\n'])
            .chars()
            .map(|c| {
                if c == '\r' || c == '\n' || c == '\t' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        if !one_line.is_empty() {
            self.insert(&one_line);
            self.break_run();
        }
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
                        self.insert(c.encode_utf8(&mut [0; 4]));
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
                        self.insert(c.encode_utf8(&mut [0; 4]));
                        true
                    }
                    None => false,
                }
            }
        }
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

    fn move_to(&mut self, i: usize, extend: bool) {
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
            self.anchor = if word {
                self.word_left(self.caret)
            } else {
                self.prev(self.caret)
            };
        }
        self.delete_selection(EditKind::Deleting);
        true
    }

    /// Delete (Ctrl: the word after).
    pub fn delete(&mut self, word: bool) -> bool {
        if !self.has_selection() {
            if self.caret == self.text.len() {
                return false;
            }
            self.anchor = if word {
                self.word_right(self.caret)
            } else {
                self.next(self.caret)
            };
        }
        self.delete_selection(EditKind::Deleting);
        true
    }

    fn delete_selection(&mut self, kind: EditKind) {
        self.record(kind);
        let (a, b) = self.selection();
        self.text.replace_range(a..b, "");
        self.caret = a;
        self.anchor = a;
    }

    /// Ctrl+X: the cut text.
    pub fn cut(&mut self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        let s = self.selected_text().to_string();
        self.delete_selection(EditKind::Other);
        Some(s)
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(s);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(s);
        true
    }

    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.caret = s.caret.min(self.text.len());
        self.anchor = s.anchor.min(self.text.len());
        self.last = EditKind::Other;
    }

    /// Byte offset → UTF-16 position.
    pub fn to_utf16(&self, i: usize) -> u32 {
        self.text[..i.min(self.text.len())].encode_utf16().count() as u32
    }

    /// UTF-16 position → byte offset (on a char boundary).
    pub fn index_of_utf16(&self, pos: u32) -> usize {
        let mut n = 0u32;
        for (i, c) in self.text.char_indices() {
            if n >= pos {
                return i;
            }
            n += c.len_utf16() as u32;
        }
        self.text.len()
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
    use super::Field;
    use crate::render::{Canvas, Rect, caret_x, hit, measure, size, with_alpha};
    use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

    /// How the field is drawn: its scroll and the layout of the current text (made again only when it changes).
    #[derive(Default)]
    pub struct FieldView {
        /// how far the text is scrolled left, DIPs
        pub scroll: f32,
        cached: Option<(String, IDWriteTextLayout)>,
        /// the field's rect at the last paint (for clicks and the IME window)
        pub rect: Rect,
        /// the caret's x at the last paint, window DIPs
        pub caret_at: f32,
    }

    impl FieldView {
        fn layout(&mut self, c: &Canvas, text: &str, h: f32) -> Option<IDWriteTextLayout> {
            if let Some((t, l)) = &self.cached
                && t == text
            {
                return Some(l.clone());
            }
            let l = c.gfx.layout(text, &c.gfx.fonts.display, 0.0, h)?;
            self.cached = Some((text.to_string(), l.clone()));
            Some(l)
        }

        /// Draws the line in `r`: placeholder, selection, text, caret.
        pub fn paint(
            &mut self,
            c: &Canvas,
            field: &Field,
            r: Rect,
            focused: bool,
            placeholder: &str,
        ) {
            self.rect = r;
            let t = c.theme;
            c.push_clip(Rect::new(r.x - 2.0, r.y, r.w + 4.0, r.h));
            if field.text().is_empty() {
                self.scroll = 0.0;
                if let Some(l) = c.gfx.layout(placeholder, &c.gfx.fonts.display, 0.0, r.h) {
                    c.text_plain(&l, r.x, r.y, with_alpha(t.muted, 0.8));
                }
            }
            let Some(layout) = self.layout(c, field.text(), r.h) else {
                c.pop_clip();
                return;
            };
            let caret = caret_x(&layout, field.to_utf16(field.caret()));
            // keep the caret in sight, with a little room past it
            let room = 4.0;
            if caret - self.scroll > r.w - room {
                self.scroll = caret - r.w + room;
            }
            if caret - self.scroll < 0.0 {
                self.scroll = caret;
            }
            let (w, _) = measure(&layout);
            if w - self.scroll < r.w - room {
                self.scroll = (w - r.w + room).max(0.0);
            }
            let x0 = r.x - self.scroll;
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
            if !field.text().is_empty() {
                c.text(&layout, x0, r.y, t.ink);
            }
            self.caret_at = x0 + caret;
            if focused {
                let mid = r.y + r.h * 0.5;
                let half = size::DISPLAY * 0.62;
                c.line(
                    x0 + caret + 0.5,
                    mid - half,
                    x0 + caret + 0.5,
                    mid + half,
                    1.5,
                    t.accent,
                );
            }
            c.pop_clip();
        }

        /// The UTF-16 position under window DIP point (x, y) (clicks), from the last layout.
        pub fn hit(&self, x: f32, y: f32) -> u32 {
            match &self.cached {
                Some((_, l)) => hit(l, x - self.rect.x + self.scroll, y - self.rect.y),
                None => 0,
            }
        }
    }
}

/// IME: the composition window follows the caret; the result string is inserted as typed text.
#[cfg(windows)]
pub mod ime {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::Input::Ime::{
        CFS_POINT, COMPOSITIONFORM, GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext,
        ImmReleaseContext, ImmSetCompositionWindow,
    };

    /// Puts the IME's composition window at the caret (window pixels).
    pub fn place(hwnd: HWND, x: i32, y: i32) {
        // SAFETY: the context is released below; the form is a live local
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
            let _ = ImmReleaseContext(hwnd, imc);
        }
    }

    /// WM_IME_COMPOSITION with GCS_RESULTSTR: the committed text.
    pub fn result(hwnd: HWND) -> Option<String> {
        // SAFETY: the context is released below; the buffer is sized from the first call
        unsafe {
            let imc = ImmGetContext(hwnd);
            if imc.is_invalid() {
                return None;
            }
            let bytes = ImmGetCompositionStringW(imc, GCS_RESULTSTR, None, 0);
            let out = if bytes > 0 {
                let mut buf = vec![0u16; bytes as usize / 2];
                let got = ImmGetCompositionStringW(
                    imc,
                    GCS_RESULTSTR,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Field {
        let mut f = Field::default();
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
        let mut g = typed("sqrt(2)+3");
        g.left(true, false);
        assert_eq!(g.caret(), "sqrt(2)+".len());
        g.left(true, false);
        assert_eq!(g.caret(), "sqrt(2".len());
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
        let mut f = typed("12");
        f.insert(" ");
        for u in "34".encode_utf16() {
            f.char16(u);
        }
        assert_eq!(f.text(), "12 34");
        assert!(f.undo());
        assert_eq!(f.text(), "12 ");
        assert!(f.undo());
        assert_eq!(f.text(), "12");
        assert!(f.redo());
        assert_eq!(f.text(), "12 ");
        f.set_text("9");
        assert!(f.undo());
        assert_eq!(f.text(), "12 ");
    }

    #[test]
    fn paste_is_one_line() {
        let mut f = Field::default();
        f.paste("1\r\n+2\t*3\n");
        assert_eq!(f.text(), "1  +2 *3");
    }

    #[test]
    fn double_click_selects_the_word() {
        let mut f = typed("2 * pi + 1");
        f.select_word_at(5);
        assert_eq!(f.selected_text(), "pi");
        f.select_word_at(0);
        assert_eq!(f.selected_text(), "2");
    }
}
