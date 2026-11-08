//! A formula sheet's page, the pure part (macos/IdentityWindow.swift, native/mac/Sources/Sheets/SheetData.swift):
//! which sheet a page shows (`page_sheet`), the Typst documents its cells compile to (`docs`), the filter
//! (`Filter`, `words`, `matches`), where its titles and rows go (`lay_out`), what a point on a typeset row boxes
//! and copies (`target`), and a window's pages with back and forward (`Pages`). sheets.rs draws them.

use qcalc_core::app::identities::{self, Identity, IdentitySheet};
use unicode_normalization::UnicodeNormalization;

// ---- design (IdentityWindowController), DIPs ----

pub const INSET: f32 = 16.0;
/// the header, then the filter field under it: the rows start here
pub const TOP: f32 = 60.0;
pub const COLUMN: f32 = 344.0;
pub const ROW: f32 = 22.0;
pub const GROUP_TITLE: f32 = 20.0;
pub const SECTION_TITLE: f32 = 26.0;
pub const MAX_HEIGHT: f32 = 620.0;
/// between a conversion's pieces
pub const PART_GAP: f32 = 3.0;
/// round the text a click copies
pub const PAD: f32 = 3.0;
/// the filter field: x, y, height (it spans the page less the insets)
pub const FILTER_Y: f32 = 31.0;
pub const FILTER_H: f32 = 22.0;
/// the pages a window keeps to go back through
pub const PAGES_KEPT: usize = 50;

pub fn width() -> f32 {
    INSET * 3.0 + COLUMN * 2.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bx {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Bx {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Bx {
        Bx { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn union(&self, o: &Bx) -> Bx {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Bx::new(
            x,
            y,
            self.right().max(o.right()) - x,
            self.bottom().max(o.bottom()) - y,
        )
    }
    pub fn grown(&self, dx: f32, dy: f32) -> Bx {
        Bx::new(
            self.x - dx,
            self.y - dy,
            self.w + 2.0 * dx,
            self.h + 2.0 * dy,
        )
    }
}

// ---- the sheet a page shows ----

/// The sheet as the native apps typeset it, with only its pictured sections when it has a picture (the table is
/// still there by its own name) and the rows with nothing to show dropped, as SheetData.raw does.
pub fn page_sheet(id: &str) -> Option<IdentitySheet> {
    let mut sheet = identities::native_identity_sheet_by_id(id)?;
    let pictured = |s: &identities::IdentitySection| {
        s.groups
            .iter()
            .any(|g| g.items.iter().any(|i| i.figure.is_some()))
    };
    if sheet.sections.iter().any(pictured) {
        sheet.sections.retain(pictured);
    }
    for section in &mut sheet.sections {
        for group in &mut section.groups {
            group
                .items
                .retain(|i| i.figure.is_some() || !i.show.trim().is_empty());
        }
        section.groups.retain(|g| !g.items.is_empty());
    }
    sheet.sections.retain(|s| !s.groups.is_empty());
    (!sheet.sections.is_empty()).then_some(sheet)
}

pub fn has_figure(sheet: &IdentitySheet) -> bool {
    sheet
        .sections
        .iter()
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.items)
        .any(|i| i.figure.is_some())
}

/// A section's lead columns: its own widths, else the page's `LEAD_WIDTHS`.
pub fn lead_widths(section: &identities::IdentitySection) -> Vec<f32> {
    section
        .lead_widths
        .as_deref()
        .unwrap_or(&identities::LEAD_WIDTHS)
        .iter()
        .map(|&w| w as f32)
        .collect()
}

fn lead_width(widths: &[f32], i: usize) -> f32 {
    widths.get(i).or(widths.last()).copied().unwrap_or_default()
}

/// One typeset cell: words, drawn in the app's font, or a Typst document.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Words(String),
    Doc(String),
}

/// What a row typesets: its cells (the lead cells, then the row), the note when it's maths, a conversion's
/// pieces (document, copy), and a figure's labels (x, y, document, copy). A figure's page is its one cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Docs {
    pub cells: Vec<Cell>,
    pub note: Option<String>,
    pub parts: Vec<(String, String)>,
    pub spots: Vec<(f32, f32, String, String)>,
    pub figure: bool,
}

impl Docs {
    pub fn typeset(&self) -> bool {
        !self.cells.is_empty()
    }
}

/// A cell that is only a quoted string (`"Time"`) is words.
fn words_of(cell: &str) -> Option<&str> {
    let inner = cell.strip_prefix('"')?.strip_suffix('"')?;
    (cell.chars().count() > 1 && !inner.contains('"')).then_some(inner)
}

pub fn docs(item: &Identity) -> Docs {
    let mut out = Docs::default();
    if let Some(figure) = &item.figure {
        out.cells = vec![Cell::Doc(figure.clone())];
        out.figure = true;
        out.spots = item
            .spots
            .iter()
            .flatten()
            .map(|s| (s.x as f32, s.y as f32, s.doc.clone(), s.copy.clone()))
            .collect();
        return out;
    }
    out.parts = item
        .parts
        .iter()
        .flatten()
        .map(|p| (identities::sheet_math_document(&p.typst), p.copy.clone()))
        .collect();
    if let Some(cells) = &item.typst {
        out.cells = cells
            .iter()
            .map(|c| match words_of(c) {
                Some(w) => Cell::Words(w.to_string()),
                None => Cell::Doc(identities::sheet_math_document(c)),
            })
            .collect();
        out.note = item
            .note_typst
            .as_ref()
            .map(|m| identities::sheet_math_document(&format!("#text(size: 10.5pt)[${m}$]")));
    }
    out
}

/// Every document a sheet compiles, once each.
pub fn sheet_docs(sheet: &IdentitySheet) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in sheet
        .sections
        .iter()
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.items)
    {
        let d = docs(item);
        let all = d
            .cells
            .into_iter()
            .filter_map(|c| match c {
                Cell::Doc(doc) => Some(doc),
                Cell::Words(_) => None,
            })
            .chain(d.note)
            .chain(d.parts.into_iter().map(|p| p.0))
            .chain(d.spots.into_iter().map(|s| s.2));
        for doc in all {
            if seen.insert(doc.clone()) {
                out.push(doc);
            }
        }
    }
    out
}

/// The sizes of what a row draws, in points: each cell, each piece, each label, the note.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sizes {
    pub cells: Vec<(f32, f32)>,
    pub parts: Vec<(f32, f32)>,
    pub spots: Vec<(f32, f32)>,
}

/// The tallest cell plus 10, never under the plain row's 22.
pub fn row_height(sizes: &Sizes) -> f32 {
    let tallest = sizes.cells.iter().map(|c| c.1).fold(0.0, f32::max);
    ROW.max(if sizes.cells.is_empty() {
        0.0
    } else {
        tallest + 10.0
    })
}

// ---- the filter ----

/// Case, accents and width folded, `−` read as `-`.
pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .map(|c| match c {
            '\u{2212}' => '-',
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            '\u{3000}' => ' ',
            c => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// Letters and digits alone, so `tmax` finds τ_max's `t_max` and `x2` finds x^{2}.
fn compact(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The filter's words.
pub fn words(filter: &str) -> Vec<String> {
    fold(filter)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Every word is somewhere in the texts, or in one of their words squeezed to its letters and digits.
pub fn matches(words: &[String], texts: &[&str]) -> bool {
    let hay = fold(&texts.join(" "));
    // squeezed a word at a time, so `sin` doesn't run across `s in`
    let tight: Vec<String> = hay.split_whitespace().map(compact).collect();
    words.iter().all(|word| {
        hay.contains(word.as_str()) || {
            let w = compact(word);
            !w.is_empty() && tight.iter().any(|t| t.contains(&w))
        }
    })
}

/// What a filter is matched on: the row's words, its note, what it copies and opens, its cells' and pieces' copies.
pub fn search_text(item: &Identity) -> Vec<String> {
    let runs = |show: &str| -> String {
        identities::identity_runs(show)
            .into_iter()
            .map(|r| r.text)
            .collect()
    };
    let mut out: Vec<String> = item.lead.iter().flatten().map(|l| runs(l)).collect();
    out.push(runs(&item.show));
    out.extend(item.note.clone());
    out.push(item.copy.clone());
    out.extend(item.open.clone());
    out.extend(item.lead_copy.iter().flatten().cloned());
    out.extend(item.parts.iter().flatten().map(|p| p.copy.clone()));
    out
}

/// The filter field's text and caret (a byte offset); `all` while Ctrl+A has it all selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub text: String,
    pub caret: usize,
    pub all: bool,
}

impl Filter {
    fn take_all(&mut self) -> bool {
        if std::mem::take(&mut self.all) {
            self.text.clear();
            self.caret = 0;
            return true;
        }
        false
    }

    fn prev(&self) -> usize {
        self.text[..self.caret]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.text[self.caret..]
            .chars()
            .next()
            .map_or(self.caret, |c| self.caret + c.len_utf8())
    }

    pub fn insert(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        if s.is_empty() {
            return;
        }
        self.take_all();
        self.text.insert_str(self.caret, &s);
        self.caret += s.len();
    }

    /// Backspace; with `word`, back to the start of the word before the caret.
    pub fn back(&mut self, word: bool) {
        if self.take_all() {
            return;
        }
        let to = if word {
            let before = &self.text[..self.caret];
            let trimmed = before.trim_end();
            trimmed
                .char_indices()
                .rev()
                .find(|(_, c)| c.is_whitespace())
                .map_or(0, |(i, c)| i + c.len_utf8())
        } else {
            self.prev()
        };
        self.text.replace_range(to..self.caret, "");
        self.caret = to;
    }

    pub fn delete(&mut self) {
        if self.take_all() {
            return;
        }
        let to = self.next();
        self.text.replace_range(self.caret..to, "");
    }

    pub fn left(&mut self) {
        if std::mem::take(&mut self.all) {
            self.caret = 0;
        } else {
            self.caret = self.prev();
        }
    }

    pub fn right(&mut self) {
        if std::mem::take(&mut self.all) {
            self.caret = self.text.len();
        } else {
            self.caret = self.next();
        }
    }

    pub fn home(&mut self) {
        self.all = false;
        self.caret = 0;
    }

    pub fn end(&mut self) {
        self.all = false;
        self.caret = self.text.len();
    }

    pub fn select_all(&mut self) {
        self.all = !self.text.is_empty();
    }

    pub fn clear(&mut self) {
        *self = Filter::default();
    }

    pub fn set(&mut self, text: &str) {
        self.clear();
        self.insert(text);
    }
}

// ---- layout ----

/// Where a page's titles and rows go, from the top of the scrolled content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    /// section titles: (y, section)
    pub titles: Vec<(f32, usize)>,
    /// group titles: (x, y, section, group)
    pub groups: Vec<(f32, f32, usize, usize)>,
    /// rows: (rect, section, group, item)
    pub rows: Vec<(Bx, usize, usize, usize)>,
    /// each shown section's top, for opening at one
    pub tops: Vec<(usize, f32)>,
    /// the `no matches` line's top, when the filter left nothing
    pub none: Option<f32>,
    /// a row opens a sheet (the status reads `click to open`)
    pub opens: bool,
    pub height: f32,
}

/// The rows the filter leaves, each group in whichever column is shorter so far (a wide section's one under
/// another at the full width), a picture with no headings over it. `height(s, g, i)` is a row's height.
pub fn lay_out(
    sheet: &IdentitySheet,
    filter: &str,
    height: impl Fn(usize, usize, usize) -> f32,
) -> Laid {
    let width = width();
    let words = words(filter);
    let mut out = Laid::default();
    let mut y = 0.0f32;
    let mut matched = false;
    for (si, section) in sheet.sections.iter().enumerate() {
        let bare = section
            .groups
            .iter()
            .any(|g| g.items.iter().any(|i| i.figure.is_some()));
        // each group's rows the filter keeps: a heading row stays when its table has a match below it
        let groups: Vec<(usize, Vec<usize>)> = section
            .groups
            .iter()
            .enumerate()
            .filter_map(|(gi, group)| {
                if words.is_empty() || bare {
                    return Some((gi, (0..group.items.len()).collect()));
                }
                let kept: Vec<usize> = group
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        item.copy.is_empty() || {
                            let texts = search_text(item);
                            let mut all: Vec<&str> = vec![&section.title, &group.title];
                            all.extend(texts.iter().map(String::as_str));
                            matches(&words, &all)
                        }
                    })
                    .map(|(i, _)| i)
                    .collect();
                if !kept.iter().any(|&i| !group.items[i].copy.is_empty()) {
                    return None;
                }
                matched = true;
                Some((gi, kept))
            })
            .collect();
        if groups.is_empty() {
            continue;
        }
        out.tops.push((si, y));
        if !bare {
            out.titles.push((y, si));
        }
        let wide = section.wide == Some(true);
        let first = if bare { y - 6.0 } else { y + SECTION_TITLE };
        let mut columns = if wide {
            vec![first]
        } else {
            vec![first, first]
        };
        let span = if wide { width - INSET * 2.0 } else { COLUMN };
        for (gi, items) in groups {
            let c = usize::from(columns.len() > 1 && columns[1] < columns[0]);
            let x = INSET + c as f32 * (COLUMN + INSET);
            let mut gy = columns[c] + 6.0;
            if !bare {
                out.groups.push((x + 6.0, gy + 2.0, si, gi));
                gy += GROUP_TITLE;
            }
            for ii in items {
                let item = &section.groups[gi].items[ii];
                if item.open.is_some() {
                    out.opens = true;
                }
                let h = height(si, gi, ii);
                out.rows.push((Bx::new(x, gy, span, h), si, gi, ii));
                gy += h;
            }
            columns[c] = gy;
        }
        y = columns.iter().copied().fold(y, f32::max) + 14.0;
    }
    // a sheet that is only a picture has nothing to filter, so it never says so
    let only_figure = sheet.sections.iter().all(|s| {
        s.groups
            .iter()
            .any(|g| g.items.iter().any(|i| i.figure.is_some()))
    });
    if !words.is_empty() && !matched && !only_figure {
        out.none = Some(y + 10.0);
        y += 40.0;
    }
    out.height = (y + INSET - 14.0).max(0.0);
    out
}

// ---- what a point boxes and copies ----

/// Where a figure's page starts in its row.
pub fn figure_origin(sizes: &Sizes, row: (f32, f32)) -> (f32, f32) {
    let page = sizes.cells.first().copied().unwrap_or_default();
    ((row.0 - page.0) / 2.0, (row.1 - page.1) / 2.0)
}

/// On a typeset row, the box round the text under `p` (row coordinates) and what a click there copies: a
/// figure's label, a conversion's piece (else the whole row), a lead cell that copies itself, else the formula. A
/// row that opens a sheet boxes all of it.
pub fn target(
    item: &Identity,
    docs: &Docs,
    sizes: &Sizes,
    widths: &[f32],
    row: (f32, f32),
    p: (f32, f32),
) -> Option<(Bx, String)> {
    if docs.figure {
        let (ox, oy) = figure_origin(sizes, row);
        for (spot, size) in docs.spots.iter().zip(&sizes.spots) {
            if spot.3.is_empty() {
                continue;
            }
            let b = Bx::new(
                ox + spot.0 - size.0 / 2.0,
                oy + spot.1 - size.1 / 2.0,
                size.0,
                size.1,
            )
            .grown(PAD, 2.0);
            if b.contains(p.0, p.1) {
                return Some((b, spot.3.clone()));
            }
        }
        return None;
    }
    if !docs.typeset() || item.copy.is_empty() {
        return None;
    }
    if !docs.parts.is_empty() {
        // a piece under the pointer copies alone; anywhere else on the row, all of it
        let mut left = 6.0;
        let mut line: Option<Bx> = None;
        for ((_, copy), size) in docs.parts.iter().zip(&sizes.parts) {
            let b = Bx::new(
                left - PAD + 1.0,
                (row.1 - size.1) / 2.0 - 2.0,
                size.0 + PAD * 2.0 - 2.0,
                size.1 + 4.0,
            );
            line = Some(line.map_or(b, |l| l.union(&b)));
            if !copy.is_empty() && p.0 >= b.x && p.0 < b.right() {
                return Some((b, copy.clone()));
            }
            left += size.0 + PART_GAP;
        }
        return line.map(|l| (l, item.copy.clone()));
    }
    // each cell's box at its column's place: the whole cell and a little more
    let mut x = 6.0;
    let boxes: Vec<Bx> = sizes
        .cells
        .iter()
        .enumerate()
        .map(|(i, size)| {
            let b = Bx::new(
                x - PAD,
                (row.1 - size.1) / 2.0 - 3.0,
                size.0 + PAD * 2.0,
                size.1 + 6.0,
            );
            x += lead_width(widths, i);
            b
        })
        .collect();
    let last = *boxes.last()?;
    if item.open.is_some() {
        return Some((
            boxes.iter().fold(last, |a, b| a.union(b)),
            item.copy.clone(),
        ));
    }
    let lead = item.lead.as_ref().map_or(0, Vec::len);
    let copies = item.lead_copy.as_deref().unwrap_or(&[]);
    let mut right = 6.0;
    for (i, b) in boxes.iter().enumerate().take(lead.min(boxes.len() - 1)) {
        right += lead_width(widths, i);
        if p.0 < right {
            if let Some(copy) = copies.get(i).filter(|c| !c.is_empty()) {
                return Some((*b, copy.clone()));
            }
            break;
        }
    }
    Some((last, item.copy.clone()))
}

// ---- a window's pages ----

/// The pages shown in one window and which is up; each remembers where it was scrolled to.
#[derive(Clone, Debug, PartialEq)]
pub struct Pages {
    pub list: Vec<(String, f32)>,
    pub at: usize,
}

impl Pages {
    pub fn new(id: &str) -> Pages {
        Pages {
            list: vec![(id.to_string(), 0.0)],
            at: 0,
        }
    }

    pub fn current(&self) -> &str {
        &self.list[self.at].0
    }

    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.list.len()
    }

    /// Another sheet replaces the page (the ones after it go, the last 50 kept); false for the same one.
    pub fn show(&mut self, id: &str, scrolled: f32) -> bool {
        if self.current() == id {
            return false;
        }
        self.list[self.at].1 = scrolled;
        self.list.truncate(self.at + 1);
        self.list.push((id.to_string(), 0.0));
        if self.list.len() > PAGES_KEPT {
            let extra = self.list.len() - PAGES_KEPT;
            self.list.drain(..extra);
        }
        self.at = self.list.len() - 1;
        true
    }

    /// A step back (-1) or forward (1): where the page it lands on was scrolled to, or None with nowhere to go.
    pub fn go(&mut self, step: i32, scrolled: f32) -> Option<f32> {
        let to = self.at as i64 + i64::from(step);
        if to < 0 || to as usize >= self.list.len() {
            return None;
        }
        self.list[self.at].1 = scrolled;
        self.at = to as usize;
        Some(self.list[self.at].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(show: &str, copy: &str) -> Identity {
        Identity {
            lead: None,
            show: show.into(),
            copy: copy.into(),
            note: None,
            lead_copy: None,
            open: None,
            typst: None,
            note_typst: None,
            figure: None,
            parts: None,
            spots: None,
        }
    }

    #[test]
    fn words_cells_are_quoted_strings_and_the_rest_is_maths() {
        let mut i = item("Time", "t");
        i.typst = Some(vec![
            "\"Time\"".into(),
            "x^2".into(),
            "\"a\" + \"b\"".into(),
        ]);
        i.note_typst = Some("x > 0".into());
        let d = docs(&i);
        assert_eq!(d.cells[0], Cell::Words("Time".into()));
        assert!(matches!(&d.cells[1], Cell::Doc(doc) if doc.contains("x^2")));
        assert!(matches!(&d.cells[2], Cell::Doc(_)));
        assert!(
            d.note
                .as_ref()
                .unwrap()
                .contains("#text(size: 10.5pt)[$x > 0$]")
        );
        assert!(d.typeset());
        assert!(!docs(&item("a", "a")).typeset());
    }

    #[test]
    fn the_unit_circle_shows_only_its_picture() {
        let s = page_sheet("unit circle").unwrap();
        assert!(has_figure(&s));
        assert_eq!(s.sections.len(), 1);
        let fig = &s.sections[0].groups[0].items[0];
        let d = docs(fig);
        assert!(d.figure && d.spots.len() > 20);
        assert!(sheet_docs(&s).len() > 20);
        // the table is still there as its own part
        let t = page_sheet("unit circle: Exact values").unwrap();
        assert!(!has_figure(&t));
    }

    #[test]
    fn the_web_sheets_typeset_from_the_cell_table() {
        let s = page_sheet("conversions").unwrap();
        let typeset = s
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
            .filter(|i| docs(i).typeset())
            .count();
        assert!(typeset > 10, "{typeset}");
        let parts = s
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
            .find(|i| i.show == "1 in = 2.54 cm")
            .map(docs)
            .unwrap();
        assert_eq!(parts.parts.len(), 5);
        assert_eq!(parts.parts[3].1, "2.54");
    }

    #[test]
    fn folding_reads_case_accents_width_and_the_minus() {
        assert_eq!(fold("Thévenin"), "thevenin");
        assert_eq!(fold("Ａb−c"), "ab-c");
        assert_eq!(words("  Ohm  LAW "), vec!["ohm", "law"]);
    }

    #[test]
    fn a_filter_word_matches_text_or_a_squeezed_word() {
        let w = words("tmax");
        assert!(matches(&w, &["τ_max = T r / J", "t_max"]));
        assert!(!matches(&w, &["t", "max"]));
        // sin doesn't run across `s in`
        assert!(!matches(&words("sin"), &["s in"]));
        assert!(matches(&words("sin cos"), &["sin^2 + cos^2 = 1"]));
        assert!(!matches(&words("sin tan"), &["sin^2 + cos^2 = 1"]));
    }

    #[test]
    fn the_filter_keeps_matching_rows_their_tables_and_titles() {
        let s = page_sheet("continuum mechanics").unwrap();
        let all = lay_out(&s, "", |_, _, _| ROW);
        let torsion = lay_out(&s, "torsion", |_, _, _| ROW);
        assert!(torsion.rows.len() < all.rows.len() && !torsion.rows.is_empty());
        // `torsion` keeps the Torsion section whole
        let si = s
            .sections
            .iter()
            .position(|x| x.title == "Torsion")
            .unwrap();
        let in_torsion = s.sections[si]
            .groups
            .iter()
            .map(|g| g.items.len())
            .sum::<usize>();
        assert_eq!(
            torsion.rows.iter().filter(|r| r.1 == si).count(),
            in_torsion
        );
        let none = lay_out(&s, "zzzqqq", |_, _, _| ROW);
        assert!(none.rows.is_empty() && none.none.is_some());
        assert!(all.none.is_none());
        // the figure is never filtered, and a picture alone never says `no matches`
        let uc = page_sheet("unit circle").unwrap();
        let l = lay_out(&uc, "zzzqqq", |_, _, _| 690.0);
        assert_eq!(l.rows.len(), 1);
        assert!(l.none.is_none() && l.titles.is_empty() && l.groups.is_empty());
    }

    #[test]
    fn a_heading_row_stays_with_its_table() {
        let s = page_sheet("unit circle: Exact values").unwrap();
        let l = lay_out(&s, "135", |_, _, _| ROW);
        let items = &s.sections[0].groups[0].items;
        let kept: Vec<&Identity> = l.rows.iter().map(|r| &items[r.3]).collect();
        assert!(kept[0].copy.is_empty(), "the heading first");
        assert!(kept.len() >= 2 && kept.len() < items.len());
    }

    #[test]
    fn rows_take_their_tallest_cell_and_lay_out_down_the_page() {
        assert_eq!(row_height(&Sizes::default()), ROW);
        let tall = Sizes {
            cells: vec![(10.0, 8.0), (20.0, 30.0)],
            ..Sizes::default()
        };
        assert_eq!(row_height(&tall), 40.0);
        let s = page_sheet("formulas").unwrap();
        let l = lay_out(&s, "", |_, _, _| 30.0);
        assert!(l.opens);
        assert!(l.rows.iter().all(|r| r.0.h == 30.0));
        assert!(l.rows.iter().all(|r| r.0.right() <= width() - INSET + 0.01));
        assert!(l.tops.windows(2).all(|w| w[1].1 > w[0].1));
    }

    #[test]
    fn a_point_boxes_the_cell_it_copies() {
        let mut i = item("10^3", "1e3");
        i.lead = Some(vec!["kilo".into(), "k".into()]);
        i.lead_copy = Some(vec![String::new(), "k".into()]);
        i.typst = Some(vec!["\"kilo\"".into(), "k".into(), "10^3".into()]);
        let d = docs(&i);
        let sizes = Sizes {
            cells: vec![(30.0, 14.0), (8.0, 14.0), (24.0, 16.0)],
            ..Sizes::default()
        };
        let w = [64.0, 28.0];
        // the label boxes and copies the formula
        let (b, c) = target(&i, &d, &sizes, &w, (300.0, 26.0), (10.0, 13.0)).unwrap();
        assert_eq!(c, "1e3");
        assert_eq!(b.x, 6.0 + 64.0 + 28.0 - PAD);
        // the symbol copies itself
        let (b, c) = target(&i, &d, &sizes, &w, (300.0, 26.0), (75.0, 13.0)).unwrap();
        assert_eq!((c.as_str(), b.x), ("k", 70.0 - PAD));
        assert_eq!(b.w, 8.0 + 2.0 * PAD);
        // a row that opens a sheet boxes all of it
        i.open = Some("beams".into());
        let (b, _) = target(&i, &d, &sizes, &w, (300.0, 26.0), (75.0, 13.0)).unwrap();
        assert_eq!(b.x, 6.0 - PAD);
        // a heading row boxes nothing
        assert!(target(&item("a", ""), &d, &sizes, &w, (300.0, 26.0), (10.0, 10.0)).is_none());
    }

    #[test]
    fn a_conversion_copies_its_number_or_unit_alone() {
        let mut i = item("1 in = 2.54 cm", "1 in = 2.54 cm");
        i.typst = Some(vec!["1 \"in\" = 2.54 \"cm\"".into()]);
        let mut d = docs(&i);
        d.parts = vec![
            ("a".into(), "1".into()),
            ("b".into(), "in".into()),
            ("c".into(), String::new()),
            ("d".into(), "2.54".into()),
        ];
        let sizes = Sizes {
            cells: vec![(80.0, 14.0)],
            parts: vec![(7.0, 14.0), (12.0, 14.0), (9.0, 14.0), (25.0, 14.0)],
            ..Sizes::default()
        };
        let at = |x: f32| target(&i, &d, &sizes, &[], (300.0, 26.0), (x, 13.0)).unwrap();
        assert_eq!(at(7.0).1, "1");
        assert_eq!(at(18.0).1, "in");
        // the `=` and past the end copy the row
        assert_eq!(at(30.0).1, "1 in = 2.54 cm");
        assert_eq!(at(200.0).1, "1 in = 2.54 cm");
        assert_eq!(at(45.0).1, "2.54");
    }

    #[test]
    fn a_figure_copies_each_label_alone() {
        let mut i = item(" ", "");
        i.figure = Some("page".into());
        let mut d = docs(&i);
        d.spots = vec![
            (100.0, 100.0, "a".into(), "0°".into()),
            (200.0, 100.0, "b".into(), String::new()),
        ];
        let sizes = Sizes {
            cells: vec![(680.0, 680.0)],
            spots: vec![(20.0, 14.0), (20.0, 14.0)],
            ..Sizes::default()
        };
        let row = (704.0, 690.0);
        let (ox, oy) = figure_origin(&sizes, row);
        assert_eq!((ox, oy), (12.0, 5.0));
        let hit = target(&i, &d, &sizes, &[], row, (ox + 100.0, oy + 100.0)).unwrap();
        assert_eq!(hit.1, "0°");
        assert!(target(&i, &d, &sizes, &[], row, (ox + 200.0, oy + 100.0)).is_none());
        assert!(target(&i, &d, &sizes, &[], row, (ox + 150.0, oy + 100.0)).is_none());
    }

    #[test]
    fn pages_go_back_and_forward_and_remember_their_scroll() {
        let mut p = Pages::new("formulas");
        assert!(!p.can_back() && !p.can_forward());
        assert!(!p.show("formulas", 10.0));
        assert!(p.show("beams", 40.0));
        assert!(p.show("torsion", 5.0));
        assert_eq!(p.current(), "torsion");
        assert_eq!(p.go(-1, 12.0), Some(5.0));
        assert_eq!(p.current(), "beams");
        assert_eq!(p.go(-1, 7.0), Some(40.0));
        assert_eq!(p.go(-1, 0.0), None);
        assert_eq!(p.go(1, 0.0), Some(7.0));
        assert_eq!(p.go(1, 0.0), Some(12.0));
        assert!(!p.can_forward());
        // a new page from the middle drops what was ahead
        p.go(-2, 0.0);
        p.show("fluids", 0.0);
        assert_eq!(p.list.len(), 2);
        assert!(!p.can_forward());
        for k in 0..60 {
            p.show(&format!("s{k}"), 0.0);
        }
        assert_eq!(p.list.len(), PAGES_KEPT);
        assert_eq!(p.at, PAGES_KEPT - 1);
    }

    #[test]
    fn the_filter_field_edits() {
        let mut f = Filter::default();
        f.insert("ohm law");
        assert_eq!((f.text.as_str(), f.caret), ("ohm law", 7));
        f.back(true);
        assert_eq!(f.text, "ohm ");
        f.left();
        f.left();
        f.insert("τ");
        assert_eq!(f.text, "ohτm ");
        f.back(false);
        f.delete();
        assert_eq!(f.text, "oh ");
        f.home();
        f.delete();
        assert_eq!((f.text.as_str(), f.caret), ("h ", 0));
        f.end();
        f.select_all();
        f.insert("x\u{1b}");
        assert_eq!((f.text.as_str(), f.caret), ("x", 1));
        f.select_all();
        f.back(false);
        assert!(f.text.is_empty());
        f.set("beams");
        assert_eq!(f.caret, 5);
    }
}
