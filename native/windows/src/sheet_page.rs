//! A formula sheet's page, the pure part (macos/IdentityWindow.swift, native/mac/Sources/Sheets/SheetData.swift):
//! which sheet a page shows (`page_sheet`), the Typst documents its cells compile to (`docs`), the filter
//! (`Filter`, `words`, `matches`), where its titles and rows go (`lay_out`), what a point on a typeset row boxes
//! and copies (`target`), and a window's pages with back and forward (`Pages`). sheets.rs draws them.

use qcalc_core::app::identities::{self, Identity, IdentitySheet};

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

/// The sheet as the native apps typeset it, with the rows with nothing to show dropped, as SheetData.raw does. Every
/// section is kept: a sheet with a picture shows only the picture (`whole`), but a search finds its other rows (the
/// unit circle's exact values).
pub fn page_sheet(id: &str) -> Option<IdentitySheet> {
    let mut sheet = identities::native_identity_sheet_by_id(id)?;
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

/// A section that holds a picture.
pub fn pictured(section: &identities::IdentitySection) -> bool {
    section
        .groups
        .iter()
        .any(|g| g.items.iter().any(|i| i.figure.is_some()))
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

// ---- the rows a page lists ----

/// A row's place: its sheet (`OWN`, the page's own, else the catalog's), section, group and item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RowKey {
    pub sheet: u32,
    pub s: u32,
    pub g: u32,
    pub i: u32,
}

/// The page's own sheet, in a `RowKey`.
pub const OWN: u32 = u32::MAX;

impl RowKey {
    pub fn new(sheet: u32, s: usize, g: usize, i: usize) -> RowKey {
        RowKey {
            sheet,
            s: s as u32,
            g: g as u32,
            i: i as u32,
        }
    }

    pub fn own(&self) -> bool {
        self.sheet == OWN
    }
}

/// A search's rows from other sheets, at most this many.
pub const ELSEWHERE_MOST: usize = 60;

/// A section a page lists: its title, where it's from, and the rows each of its groups keeps, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub title: String,
    pub sheet: u32,
    pub s: usize,
    pub own: bool,
    /// a picture's section: no headings over it
    pub figure: bool,
    pub groups: Vec<(usize, Vec<usize>)>,
}

/// What a page lists, for its filter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub shown: Vec<Shown>,
    pub searching: bool,
    /// no row had every word: these have all but one
    pub closest: bool,
    /// some row (not a picture) matched
    pub matched: bool,
    /// the sheet is only a picture, which has nothing to filter
    pub only_figure: bool,
}

fn all_rows(section: &identities::IdentitySection) -> Vec<(usize, Vec<usize>)> {
    section
        .groups
        .iter()
        .enumerate()
        .map(|(gi, g)| (gi, (0..g.items.len()).collect()))
        .collect()
}

/// The whole sheet, no search: a sheet with a picture shows the picture alone; its other rows come up in a search.
pub fn whole(sheet: &IdentitySheet) -> Listing {
    let any = sheet.sections.iter().any(pictured);
    Listing {
        shown: sheet
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| !any || pictured(s))
            .map(|(si, s)| Shown {
                title: s.title.clone(),
                sheet: OWN,
                s: si,
                own: true,
                figure: pictured(s),
                groups: all_rows(s),
            })
            .collect(),
        only_figure: sheet.sections.iter().all(pictured),
        ..Listing::default()
    }
}

/// A search's rows (`hits`, best first, the page's own keyed `OWN`) under their sections: the page's picture first,
/// whatever the search; then each row once (a one-formula sheet repeats a big sheet's row), its table's heading rows
/// ahead of it, under its section ("<sheet> · <section>" for another sheet's, at most 60 of those).
pub fn found<'a>(
    sheet: &'a IdentitySheet,
    hits: &[RowKey],
    closest: bool,
    other: impl Fn(u32) -> Option<&'a IdentitySheet>,
) -> Listing {
    let mut out = Listing {
        searching: true,
        closest,
        only_figure: sheet.sections.iter().all(pictured),
        ..Listing::default()
    };
    for (si, s) in sheet
        .sections
        .iter()
        .enumerate()
        .filter(|(_, s)| pictured(s))
    {
        out.shown.push(Shown {
            title: s.title.clone(),
            sheet: OWN,
            s: si,
            own: true,
            figure: true,
            groups: all_rows(s),
        });
    }
    let mut seen = std::collections::HashSet::new();
    let mut elsewhere = 0;
    let mut at: std::collections::HashMap<(u32, usize), usize> = std::collections::HashMap::new();
    for &key in hits {
        let from = if key.own() {
            Some(sheet)
        } else {
            other(key.sheet)
        };
        let (s, g, i) = (key.s as usize, key.g as usize, key.i as usize);
        let Some((from, section)) = from.and_then(|f| Some((f, f.sections.get(s)?))) else {
            continue;
        };
        let Some(item) = section.groups.get(g).and_then(|gr| gr.items.get(i)) else {
            continue;
        };
        if !key.own() && elsewhere >= ELSEWHERE_MOST {
            continue;
        }
        if !seen.insert(item.copy.as_str()) {
            continue;
        }
        if !key.own() {
            elsewhere += 1;
        }
        out.matched = true;
        let e = *at.entry((key.sheet, s)).or_insert_with(|| {
            out.shown.push(Shown {
                title: if key.own() {
                    section.title.clone()
                } else {
                    format!("{} · {}", from.title, section.title)
                },
                sheet: key.sheet,
                s,
                own: key.own(),
                figure: pictured(section),
                groups: Vec::new(),
            });
            out.shown.len() - 1
        });
        let groups = &mut out.shown[e].groups;
        match groups.iter_mut().find(|x| x.0 == g) {
            Some(gr) => gr.1.push(i),
            None => {
                // a table's heading rows go first, as on the sheet
                let mut rows: Vec<usize> = section.groups[g]
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| x.copy.is_empty())
                    .map(|(n, _)| n)
                    .collect();
                rows.push(i);
                groups.push((g, rows));
            }
        }
    }
    out
}

/// A quiet line in a search's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    /// no row has every word · closest matches
    Closest,
    /// in other sheets
    Elsewhere,
}

/// Where a page's titles and rows go, from the top of the scrolled content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    /// section titles: (y, title)
    pub titles: Vec<(f32, String)>,
    /// group titles: (x, y, title)
    pub groups: Vec<(f32, f32, String)>,
    /// rows: (rect, row)
    pub rows: Vec<(Bx, RowKey)>,
    /// quiet lines: (y, which)
    pub notes: Vec<(f32, Note)>,
    /// each of the page's own sections' top, for opening at one
    pub tops: Vec<(usize, f32)>,
    /// the `no matches` line's top, when the filter left nothing
    pub none: Option<f32>,
    /// a row opens a sheet (the status reads `click to open`)
    pub opens: bool,
    /// a row explains itself (the status reads `click to copy and explain`)
    pub explains: bool,
    /// a search's first row to pick (↵ in the filter): its index in `rows`
    pub first: Option<usize>,
    pub height: f32,
}

/// The listed rows, each group in whichever column is shorter so far (a wide section's one under another at the full
/// width), a picture with no headings over it; a search's quiet lines over its closest matches and where the other
/// sheets' start. `section(sheet, s)` finds a listed section, `height(row)` is a row's height.
pub fn lay_out<'a>(
    listing: &Listing,
    section: impl Fn(u32, usize) -> Option<&'a identities::IdentitySection>,
    height: impl Fn(RowKey) -> f32,
) -> Laid {
    let width = width();
    let mut out = Laid::default();
    let mut y = 0.0f32;
    let say = |out: &mut Laid, y: &mut f32, note: Note| {
        out.notes.push((*y + 2.0, note));
        *y += 20.0;
    };
    if listing.closest && listing.matched {
        say(&mut out, &mut y, Note::Closest);
    }
    // "in other sheets" once this sheet's are all above
    let last_own = listing.shown.iter().rposition(|e| e.own && !e.figure);
    let mut said_elsewhere = false;
    for (n, entry) in listing.shown.iter().enumerate() {
        let Some(sec) = section(entry.sheet, entry.s) else {
            continue;
        };
        let bare = entry.figure;
        if !entry.own && !said_elsewhere {
            said_elsewhere = true;
            if last_own.is_none_or(|l| n > l) {
                say(&mut out, &mut y, Note::Elsewhere);
            }
        }
        if entry.own && !out.tops.iter().any(|t| t.0 == entry.s) {
            out.tops.push((entry.s, y));
        }
        if !bare {
            out.titles.push((y, entry.title.clone()));
        }
        let wide = sec.wide == Some(true);
        let first = if bare { y - 6.0 } else { y + SECTION_TITLE };
        let mut columns = if wide {
            vec![first]
        } else {
            vec![first, first]
        };
        let span = if wide { width - INSET * 2.0 } else { COLUMN };
        for (gi, items) in &entry.groups {
            let Some(group) = sec.groups.get(*gi) else {
                continue;
            };
            let c = usize::from(columns.len() > 1 && columns[1] < columns[0]);
            let x = INSET + c as f32 * (COLUMN + INSET);
            let mut gy = columns[c] + 6.0;
            if !bare {
                out.groups.push((x + 6.0, gy + 2.0, group.title.clone()));
                gy += GROUP_TITLE;
            }
            for &ii in items {
                let Some(item) = group.items.get(ii) else {
                    continue;
                };
                let key = RowKey::new(entry.sheet, entry.s, *gi, ii);
                if item.open.is_some() {
                    out.opens = true;
                } else if (!item.copy.is_empty() || item.figure.is_some()) && item.explain.is_some()
                {
                    out.explains = true;
                }
                if listing.searching
                    && out.first.is_none()
                    && item.figure.is_none()
                    && !item.copy.is_empty()
                {
                    out.first = Some(out.rows.len());
                }
                let h = height(key);
                out.rows.push((Bx::new(x, gy, span, h), key));
                gy += h;
            }
            columns[c] = gy;
        }
        y = columns.iter().copied().fold(y, f32::max) + 14.0;
    }
    // a sheet that is only a picture has nothing to filter, so it never says so
    if listing.searching && !listing.matched && !listing.only_figure {
        out.none = Some(y + 10.0);
        y += 40.0;
    }
    out.height = (y + INSET - 14.0).max(0.0);
    out
}

// ---- the search's index ----

/// Every sheet's rows in one search (sheet_search.rs), built off the UI thread once a sheet opens (sheets.rs), or
/// one sheet's (a search typed before that's ready). A row's key's sheet is its place in `sheets`.
#[derive(Default)]
pub struct Catalog {
    pub sheets: Vec<IdentitySheet>,
    ids: std::collections::HashMap<String, u32>,
    refs: Vec<RowKey>,
    search: crate::sheet_search::Search,
}

impl Catalog {
    /// Every sheet the native app searches (the web's four, then its own; not the part sheets, which repeat them).
    pub fn every() -> Catalog {
        let mut c = Catalog::default();
        for id in identities::native_sheet_ids() {
            if let Some(sheet) = page_sheet(&id) {
                c.add(sheet);
            }
        }
        c.search.done_adding();
        c
    }

    /// A sheet's rows into the index (a picture isn't searched; the rows beside it are); its place.
    pub fn add(&mut self, sheet: IdentitySheet) -> u32 {
        let n = self.sheets.len() as u32;
        for (s, section) in sheet.sections.iter().enumerate() {
            for (g, group) in section.groups.iter().enumerate() {
                for (i, item) in group.items.iter().enumerate() {
                    if item.copy.is_empty() || item.figure.is_some() {
                        continue;
                    }
                    let text = crate::sheet_search::RowText::of(item);
                    self.search
                        .add(&text, [&sheet.title, &section.title, &group.title]);
                    self.refs.push(RowKey::new(n, s, g, i));
                }
            }
        }
        self.ids.insert(sheet.id.clone(), n);
        self.sheets.push(sheet);
        n
    }

    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.ids.get(id).copied()
    }

    pub fn sheet(&self, n: u32) -> Option<&IdentitySheet> {
        self.sheets.get(n as usize)
    }

    pub fn rows(&self) -> usize {
        self.refs.len()
    }

    /// The rows a query finds, best first, the open sheet's ahead of a tie and keyed `OWN`; and whether they are
    /// only the closest matches.
    pub fn find(&mut self, query: &str, open: Option<u32>) -> (Vec<RowKey>, bool) {
        use crate::sheet_search::{OPEN_SHEET_BONUS, Search};
        let outcome = self.search.search(query);
        let refs = &self.refs;
        let hits = Search::ranked(&outcome, |n| {
            if Some(refs[n].sheet) == open {
                OPEN_SHEET_BONUS
            } else {
                0
            }
        })
        .into_iter()
        .map(|n| {
            let mut k = refs[n];
            if Some(k.sheet) == open {
                k.sheet = OWN;
            }
            k
        })
        .collect();
        (hits, outcome.closest)
    }
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
            explain: None,
            tags: None,
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

    /// A sheet laid out for a query, searched in `c` (the sheet alone when it's empty), rows `h` tall.
    fn laid(sheet: &IdentitySheet, query: &str, c: &mut Catalog, h: f32) -> Laid {
        if c.sheets.is_empty() {
            c.add(sheet.clone());
        }
        let listing = if query.trim().is_empty() {
            whole(sheet)
        } else {
            let open = c.index_of(&sheet.id);
            let (hits, closest) = c.find(query, open);
            found(sheet, &hits, closest, |n| c.sheet(n))
        };
        lay_out(
            &listing,
            |n, s| if n == OWN { Some(sheet) } else { c.sheet(n) }.and_then(|x| x.sections.get(s)),
            |_| h,
        )
    }

    fn item_at<'a>(sheet: &'a IdentitySheet, c: &'a Catalog, k: RowKey) -> &'a Identity {
        let from = if k.own() {
            sheet
        } else {
            c.sheet(k.sheet).unwrap()
        };
        &from.sections[k.s as usize].groups[k.g as usize].items[k.i as usize]
    }

    #[test]
    fn the_unit_circle_shows_its_picture_and_finds_its_values() {
        let s = page_sheet("unit circle").unwrap();
        assert!(has_figure(&s));
        // every section is kept, so a search finds the exact values; the page shows the picture alone
        assert!(s.sections.len() > 1);
        let fig = s.sections.iter().position(pictured).unwrap();
        let d = docs(&s.sections[fig].groups[0].items[0]);
        assert!(d.figure && d.spots.len() > 20);
        assert!(sheet_docs(&s).len() > 20);
        let mut c = Catalog::default();
        let l = laid(&s, "", &mut c, 690.0);
        assert_eq!(l.rows.len(), 1);
        assert!(l.titles.is_empty() && l.groups.is_empty() && l.none.is_none());
        // `sin 30` lists rows under the picture
        let l = laid(&s, "sin 30", &mut c, ROW);
        assert!(l.rows.len() > 1, "{:?}", l.rows);
        assert!(!l.titles.is_empty());
        // ↵ picks the first row past the picture and its table's heading
        let first = l.first.unwrap();
        assert!(first >= 1 && !item_at(&s, &c, l.rows[first].1).copy.is_empty());
        // the picture stays whatever the search, and the rows beside it can say `no matches`
        let l = laid(&s, "zzzqqq", &mut c, 690.0);
        assert_eq!(l.rows.len(), 1);
        assert!(l.none.is_some());
        // the table is still there as its own part
        let t = page_sheet("unit circle: Exact values").unwrap();
        assert!(!has_figure(&t));
    }

    #[test]
    fn a_search_lists_rows_best_first_with_their_headings() {
        let s = page_sheet("continuum mechanics").unwrap();
        let mut c = Catalog::default();
        let all = laid(&s, "", &mut c, ROW);
        let torsion = laid(&s, "torsion", &mut c, ROW);
        assert!(torsion.rows.len() < all.rows.len() && !torsion.rows.is_empty());
        assert!(all.none.is_none() && all.first.is_none());
        assert_eq!(torsion.first, Some(0));
        // greek spelled in latin: `tmax` finds τ_max first
        let tmax = laid(&s, "tmax", &mut c, ROW);
        let first = item_at(&s, &c, tmax.rows[tmax.first.unwrap()].1);
        assert!(first.show.contains("τ_{max}"), "{}", first.show);
        let none = laid(&s, "zzzqqq", &mut c, ROW);
        assert!(none.rows.is_empty() && none.none.is_some());
        // a table's heading row comes with its match
        let t = page_sheet("unit circle: Exact values").unwrap();
        let mut c = Catalog::default();
        let l = laid(&t, "135", &mut c, ROW);
        let kept: Vec<&Identity> = l.rows.iter().map(|r| item_at(&t, &c, r.1)).collect();
        assert!(kept[0].copy.is_empty(), "the heading first");
        assert!(kept.len() >= 2);
        assert_eq!(l.first, Some(1));
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
        let l = laid(&s, "", &mut Catalog::default(), 30.0);
        assert!(l.opens);
        assert!(l.rows.iter().all(|r| r.0.h == 30.0));
        assert!(l.rows.iter().all(|r| r.0.right() <= width() - INSET + 0.01));
        assert!(l.tops.windows(2).all(|w| w[1].1 > w[0].1));
    }

    #[test]
    fn a_search_reads_every_sheet() {
        let mut c = Catalog::every();
        assert!(c.rows() > 1000, "{}", c.rows());
        let finance = page_sheet("finance").unwrap();
        let l = laid(&finance, "ohm", &mut c, ROW);
        // the other sheets' rows, under a quiet line and `<sheet> · <section>`
        assert!(l.notes.iter().any(|n| n.1 == Note::Elsewhere));
        assert!(l.titles.iter().any(|t| t.1.contains(" · ")));
        assert!(l.rows.len() <= ELSEWHERE_MOST + 40);
        assert!(l.rows.iter().all(|r| !r.1.own()));
        // a misspelling with no row having every word: closest matches
        let l = laid(&finance, "effective anual yield", &mut c, ROW);
        assert!(!l.rows.is_empty());
        let first = item_at(&finance, &c, l.rows[l.first.unwrap()].1);
        assert!(first.explain.is_some(), "{}", first.show);
        // each row once
        let copies: Vec<&str> = l
            .rows
            .iter()
            .map(|r| item_at(&finance, &c, r.1).copy.as_str())
            .filter(|c| !c.is_empty())
            .collect();
        let unique: std::collections::HashSet<&&str> = copies.iter().collect();
        assert_eq!(unique.len(), copies.len());
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
