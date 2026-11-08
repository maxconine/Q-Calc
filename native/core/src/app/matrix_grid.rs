//! The matrix grid (native only): `matrix 3x3` anywhere in a line opens rows × columns of fields that write
//! `[a b; c d]` back in its place, and a matrix already typed opens in the fields to edit. Offsets are UTF-16 units,
//! as the bars count them.

/// Most rows or columns the grid opens with.
pub const MAX: usize = 8;

/// Where the grid writes in the line, and what it opens with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridSpot {
    /// UTF-16 offsets of the text the grid replaces.
    pub start: usize,
    pub end: usize,
    /// Rows of cells, each as typed (`""` for a new cell).
    pub cells: Vec<Vec<String>>,
}

fn utf16_at(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

fn byte_at(text: &str, unit: usize) -> usize {
    let mut n = 0;
    for (b, c) in text.char_indices() {
        if n >= unit {
            return b;
        }
        n += c.len_utf16();
    }
    text.len()
}

/// Each `matrix` or `mat` standing as a word of its own in `lower`: (where, how long).
fn keywords(lower: &str) -> Vec<(usize, usize)> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find("mat").map(|i| i + from) {
        let len = if lower[i..].starts_with("matrix") {
            6
        } else {
            3
        };
        from = i + len;
        if lower[..i].chars().next_back().is_some_and(word)
            || lower[i + len..]
                .chars()
                .next()
                .is_some_and(char::is_alphabetic)
        {
            continue;
        }
        out.push((i, len));
    }
    out
}

/// `matrix 3x3` (`matrix 2×4`, `matrix 3`, `mat 2x2`): the grid it asks for, empty, in its place.
pub fn grid_command(text: &str) -> Option<GridSpot> {
    let lower = text.to_ascii_lowercase();
    for (i, kw) in keywords(&lower) {
        let rest = &text[i + kw..];
        let trimmed = rest.trim_start();
        let digits = |s: &str| -> (usize, usize) {
            let n: String = s.chars().take_while(char::is_ascii_digit).collect();
            (n.parse().unwrap_or(0), n.len())
        };
        let (rows, len) = digits(trimmed);
        if len == 0 {
            continue;
        }
        let mut end = i + kw + (rest.len() - trimmed.len()) + len;
        let after = &text[end..];
        let after_t = after.trim_start();
        let mut cols = rows;
        if let Some(x) = after_t
            .chars()
            .next()
            .filter(|c| matches!(c, 'x' | 'X' | '×' | '*'))
        {
            let more = after_t[x.len_utf8()..].trim_start();
            let (c, clen) = digits(more);
            if clen > 0 {
                cols = c;
                end += (after.len() - after_t.len())
                    + x.len_utf8()
                    + (after_t[x.len_utf8()..].len() - more.len())
                    + clen;
            }
        }
        if text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric())
        {
            continue;
        }
        if !(1..=MAX).contains(&rows) || !(1..=MAX).contains(&cols) {
            continue;
        }
        return Some(GridSpot {
            start: utf16_at(text, i),
            end: utf16_at(text, end),
            cells: vec![vec![String::new(); cols]; rows],
        });
    }
    None
}

/// What the line says while it holds `matrix 3x3`: how ↵ opens the grid, or why it can't.
pub fn grid_hint(text: &str) -> Option<String> {
    if let Some(spot) = grid_command(text) {
        return Some(format!(
            "↵ opens a {}×{} grid",
            spot.cells.len(),
            spot.cells.first().map_or(0, Vec::len)
        ));
    }
    // `matrix` being typed at the end of the line (`det(matrix`, `matrix 3x`, `mat 3x`): ask for its size; a bare
    // `mat` may be a name, so it waits for a digit
    let lower = text.trim_end().to_ascii_lowercase();
    let (i, kw) = *keywords(&lower).last()?;
    let size = lower[i + kw..].trim();
    if !size
        .chars()
        .all(|c| c.is_ascii_digit() || "x× ".contains(c))
        || (kw == 3 && size.is_empty())
    {
        return None;
    }
    let too_big = size
        .split(|c: char| !c.is_ascii_digit())
        .any(|n| n.parse::<usize>().is_ok_and(|n| n == 0 || n > MAX));
    Some(if too_big {
        format!("1 to {MAX} rows and columns · matrix {MAX}x{MAX} is the biggest")
    } else {
        "what size? rows × columns: matrix 2x2, matrix 3x3…".to_string()
    })
}

/// Splits `s` on `sep` outside brackets.
fn split_top(s: &str, sep: impl Fn(char) -> bool) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && sep(c) {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

/// The cells of a row written `1 2 3` or `1, 2, 3`. Spaced cells keep `-` and `+` with what follows when they
/// stand alone (`1 - 2` is one cell, as the reader takes it).
fn row_cells(row: &str) -> Vec<String> {
    if split_top(row, |c| c == ',').len() > 1 {
        return split_top(row, |c| c == ',')
            .iter()
            .map(|c| c.trim().to_string())
            .collect();
    }
    let words: Vec<String> = split_top(row, char::is_whitespace)
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect();
    let mut cells: Vec<String> = Vec::new();
    let mut join = false;
    for w in words {
        let op = matches!(w.as_str(), "+" | "-" | "*" | "/" | "^");
        if (join || op) && !cells.is_empty() {
            let last = cells.last_mut().expect("a cell");
            last.push(' ');
            last.push_str(&w);
        } else {
            cells.push(w.clone());
        }
        join = op || w.ends_with(['+', '-', '*', '/', '^']);
    }
    cells
}

/// The grid read from one bracketed matrix (`[1 2; 3 4]`, `[[1, 2], [3, 4]]`); None for anything else.
fn cells_of(literal: &str) -> Option<Vec<Vec<String>>> {
    let inner = literal.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.contains('|') || inner.is_empty() {
        return None;
    }
    let rows: Vec<Vec<String>> = if inner.starts_with('[') {
        split_top(inner, |c| c == ',')
            .iter()
            .map(|r| {
                let r = r.trim();
                r.strip_prefix('[')
                    .and_then(|r| r.strip_suffix(']'))
                    .map(|r| {
                        split_top(r, |c| c == ',')
                            .iter()
                            .map(|c| c.trim().to_string())
                            .collect()
                    })
            })
            .collect::<Option<_>>()?
    } else {
        split_top(inner, |c| c == ';')
            .iter()
            .map(|r| row_cells(r.trim()))
            .collect()
    };
    let cols = rows.first()?.len();
    let fits =
        rows.len() <= MAX && (1..=MAX).contains(&cols) && rows.iter().all(|r| r.len() == cols);
    fits.then_some(rows)
}

/// The matrices typed in a line, outermost brackets only: their byte ranges.
fn literals(text: &str) -> Vec<(usize, usize)> {
    let (mut out, mut depth, mut open) = (Vec::new(), 0i32, 0usize);
    for (b, c) in text.char_indices() {
        match c {
            '[' => {
                if depth == 0 {
                    open = b;
                }
                depth += 1;
            }
            ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push((open, b + 1));
                }
            }
            _ => {}
        }
    }
    out
}

/// The matrix the caret is in or just after, else the last one in the line, read into cells to edit.
pub fn grid_at(text: &str, caret: usize) -> Option<GridSpot> {
    let caret = byte_at(text, caret);
    let all: Vec<(usize, usize, Vec<Vec<String>>)> = literals(text)
        .into_iter()
        .filter_map(|(s, e)| Some((s, e, cells_of(&text[s..e])?)))
        .collect();
    let (s, e, cells) = all
        .iter()
        .find(|(s, e, _)| *s <= caret && caret <= *e)
        .or_else(|| all.last())?
        .clone();
    Some(GridSpot {
        start: utf16_at(text, s),
        end: utf16_at(text, e),
        cells,
    })
}

/// The cells written as a matrix: `[1 2; 3 4]`. An empty cell is 0; a cell with spaces in it loses them, so it
/// stays one entry (`a + b` is `a+b`).
pub fn grid_literal(cells: &[Vec<String>]) -> String {
    let rows: Vec<String> = cells
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| {
                    let c: String = c.chars().filter(|ch| !ch.is_whitespace()).collect();
                    if c.is_empty() { "0".to_string() } else { c }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    format!("[{}]", rows.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        let s = grid_command("matrix 3x3").expect("a grid");
        assert_eq!(
            (s.start, s.end, s.cells.len(), s.cells[0].len()),
            (0, 10, 3, 3)
        );
        let s = grid_command("det(matrix 2×4)").expect("a grid");
        assert_eq!(
            (s.start, s.end, s.cells.len(), s.cells[0].len()),
            (4, 14, 2, 4)
        );
        assert_eq!(
            grid_command("A = Matrix 2").map(|s| (s.start, s.end)),
            Some((4, 12))
        );
        assert!(grid_command("matrix 9x9").is_none());
        assert!(grid_command("mymatrix 2x2").is_none());
        assert!(grid_command("matrix").is_none());
        assert_eq!(
            grid_hint("det(matrix 3x2)").as_deref(),
            Some("↵ opens a 3×2 grid")
        );
        let ask = Some("what size? rows × columns: matrix 2x2, matrix 3x3…");
        assert_eq!(grid_hint("matrix").as_deref(), ask);
        assert_eq!(grid_hint("Matrix ").as_deref(), ask);
        assert_eq!(grid_hint("det(matrix").as_deref(), ask);
        assert_eq!(grid_hint("matrix 3x").as_deref(), ask);
        assert_eq!(grid_hint("matrix 3").as_deref(), Some("↵ opens a 3×3 grid"));
        assert_eq!(
            grid_hint("matrix 9x2").as_deref(),
            Some("1 to 8 rows and columns · matrix 8x8 is the biggest")
        );
        assert!(grid_hint("matrix 0").is_some_and(|h| h.starts_with("1 to 8")));
        // a word that only ends in it, or more typed after it
        assert!(grid_hint("mymatrix").is_none());
        assert!(grid_hint("matrix A").is_none());
        // `mat` is short for it, once a size starts
        assert_eq!(grid_command("mat 2x2").map(|s| s.cells.len()), Some(2));
        assert_eq!(
            grid_command("det(mat 3)").map(|s| (s.start, s.end)),
            Some((4, 9))
        );
        assert_eq!(grid_hint("mat 2x2").as_deref(), Some("↵ opens a 2×2 grid"));
        assert_eq!(grid_hint("mat 3x").as_deref(), ask);
        assert!(grid_hint("mat").is_none());
        assert!(grid_hint("math").is_none());
        assert!(grid_command("format 2x2").is_none());
        assert!(grid_hint("2+2").is_none());
    }

    #[test]
    fn reading_typed_matrices() {
        let s = grid_at("det([1 2; 3 4]) + 1", 6).expect("a grid");
        assert_eq!((s.start, s.end), (4, 14));
        assert_eq!(s.cells, vec![vec!["1", "2"], vec!["3", "4"]]);
        let s = grid_at("[[1, 2], [3, sqrt(2)]]", 0).expect("a grid");
        assert_eq!(s.cells, vec![vec!["1", "2"], vec!["3", "sqrt(2)"]]);
        // the caret outside every matrix: the last one
        let s = grid_at("[1 2] * [3; 4] + 5", 18).expect("a grid");
        assert_eq!(s.cells, vec![vec!["3"], vec!["4"]]);
        assert_eq!(
            grid_at("[1 - 2 3]", 0).expect("a grid").cells,
            vec![vec!["1 - 2", "3"]]
        );
        assert!(grid_at("[1 2 | 3]", 0).is_none());
        assert!(grid_at("[1 2; 3]", 0).is_none());
        assert!(grid_at("2 + 2", 0).is_none());
        // offsets in UTF-16 units
        assert_eq!(
            grid_at("π·[1 2]", 7).map(|s| (s.start, s.end)),
            Some((2, 7))
        );
    }

    #[test]
    fn writing() {
        let cells = vec![
            vec!["1".to_string(), "".into()],
            vec!["a + b".into(), "-3".into()],
        ];
        assert_eq!(grid_literal(&cells), "[1 0; a+b -3]");
    }
}
