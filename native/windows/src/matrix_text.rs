//! The matrix answers' pure logic (native only, native/MATRIX_PLAN.md): the slot's text for a matrix answer
//! (`Pretty.matrix`, TextRules.swift), the label both halves of its dual share (`BarModel.dualLabel`), and the
//! grid's moves and writing (`GridState`, Matrices.swift) over the core's `app::matrix_grid`. Drawn and driven by
//! matrices.rs; pure, so it compiles and is tested everywhere.

use qcalc_core::app::matrix_grid::{self, GridSpot};

const MINUS: char = '−';

fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || c == 'λ' || c == 'μ'
}

/// A matrix answer as the typeset row writes it, after `radicals`: real minus signs, powers raised, a product by a
/// letter run together (`[1 −1; 1 √2]`, `λ² − 5λ + 5`, `ad − bc`); any other `*` is `·`. Display only.
pub fn pretty(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    // a minus leading an entry
    let mut a: Vec<char> = c
        .iter()
        .enumerate()
        .map(|(i, &ch)| {
            let after = c.get(i + 1).copied();
            let lead = i == 0 || c[i - 1].is_whitespace() || "([;,{".contains(c[i - 1]);
            let starts =
                after.is_some_and(|n| n.is_ascii_digit() || is_letter(n) || "(π√".contains(n));
            if ch == '-' && lead && starts {
                MINUS
            } else {
                ch
            }
        })
        .collect();
    // a minus between two terms
    for i in 1..a.len().saturating_sub(2) {
        if a[i] == ' '
            && a[i + 1] == '-'
            && a[i + 2] == ' '
            && !a[i - 1].is_whitespace()
            && a.get(i + 3).is_some_and(|n| !n.is_whitespace())
        {
            a[i + 1] = MINUS;
        }
    }
    // `2*k` → `2k`, other products `·`
    let mut b = String::with_capacity(s.len());
    for (i, &ch) in a.iter().enumerate() {
        if ch == '*' {
            let before =
                i > 0 && (a[i - 1].is_ascii_digit() || is_letter(a[i - 1]) || a[i - 1] == ')');
            let after = a
                .get(i + 1)
                .is_some_and(|&n| is_letter(n) || "(√[".contains(n));
            if !(before && after) {
                b.push('·');
            }
        } else {
            b.push(ch);
        }
    }
    // `^2` → `²` after a letter or a bracket
    const RAISED: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let c: Vec<char> = b.chars().collect();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;
    while i < c.len() {
        let digits = c[i + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
        if c[i] == '^'
            && digits > 0
            && i > 0
            && (is_letter(c[i - 1]) || c[i - 1] == ')' || c[i - 1] == ']')
        {
            for d in &c[i + 1..i + 1 + digits] {
                out.push(RAISED[d.to_digit(10).unwrap_or(0) as usize]);
            }
            i += 1 + digits;
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// What both halves of a matrix answer's dual start with, written once before the exact half: the parts that read
/// the same both ways, then a shared `name = ` (`λ = (5 ± √33)/2 ≈ 5.37228, −0.372281`); "" when nothing is shared.
pub fn dual_label(exact: &str, display: &str) -> String {
    const SEP: &str = ",  ";
    let e: Vec<&str> = exact.split(SEP).collect();
    let d: Vec<&str> = display.split(SEP).collect();
    let mut same = 0;
    while same + 1 < e.len().min(d.len()) && e[same] == d[same] {
        same += 1;
    }
    let mut prefix: String = e[..same].iter().map(|p| format!("{p}{SEP}")).collect();
    if let Some(at) = e.get(same).and_then(|p| p.find(" = ")) {
        let name = &e[same][..at];
        let label = format!("{name} = ");
        if !name.is_empty()
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && d.get(same).is_some_and(|p| p.starts_with(&label))
        {
            prefix += &label;
        }
    }
    if display.starts_with(&prefix) {
        prefix
    } else {
        String::new()
    }
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn byte_at(s: &str, unit: usize) -> usize {
    let mut n = 0;
    for (b, c) in s.char_indices() {
        if n >= unit {
            return b;
        }
        n += c.len_utf16();
    }
    s.len()
}

/// What ⌃G opens for the line: the matrix under the caret (else the last), else a new 2×2 at the caret.
pub fn spot_for(line: &str, caret16: usize) -> GridSpot {
    matrix_grid::grid_at(line, caret16)
        .or_else(|| matrix_grid::grid_command(line))
        .unwrap_or(GridSpot {
            start: caret16,
            end: caret16,
            cells: vec![vec![String::new(); 2]; 2],
        })
}

/// Where an arrow key goes from a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Cell(usize, usize),
    /// ↑ from the top row: back to the line, the grid still open
    Line,
    /// off the grid any other way: the key does what it would in the cell
    Stay,
}

/// The grid's cells and where in the line they're written (UTF-16 offsets of the matrix written last).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cells: Vec<Vec<String>>,
    pub start: usize,
    pub end: usize,
    /// the line as the grid last left it; any other line closes it
    pub written: String,
}

impl Grid {
    pub fn new(spot: GridSpot, line: &str) -> Grid {
        Grid {
            cells: spot.cells,
            start: spot.start,
            end: spot.end,
            written: line.to_string(),
        }
    }

    pub fn rows(&self) -> usize {
        self.cells.len()
    }

    pub fn cols(&self) -> usize {
        self.cells.first().map_or(0, Vec::len)
    }

    /// The cells into `line` in the grid's place: the new line and the caret after the matrix (bytes); None when
    /// the grid's spot no longer fits the line.
    pub fn write(&mut self, line: &str) -> Option<(String, usize)> {
        let len = utf16_len(line);
        if self.start > self.end || self.end > len {
            return None;
        }
        let (a, b) = (byte_at(line, self.start), byte_at(line, self.end));
        let literal = matrix_grid::grid_literal(&self.cells);
        let out = format!("{}{literal}{}", &line[..a], &line[b..]);
        self.end = self.start + utf16_len(&literal);
        self.written = out.clone();
        Some((out, a + literal.len()))
    }

    /// One row or column more (or fewer), 1 to the core's most; true when the size changed.
    pub fn resize(&mut self, dr: isize, dc: isize) -> bool {
        let most = matrix_grid::MAX as isize;
        let rows = (self.rows() as isize + dr).clamp(1, most) as usize;
        let cols = (self.cols() as isize + dc).clamp(1, most) as usize;
        if rows == self.rows() && cols == self.cols() {
            return false;
        }
        self.cells = (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| {
                        self.cells
                            .get(r)
                            .and_then(|row| row.get(c))
                            .cloned()
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        true
    }

    /// Tab / Shift+Tab: along the row, then on to the next, round.
    pub fn tab(&self, (r, c): (usize, usize), back: bool) -> (usize, usize) {
        let n = (self.rows() * self.cols()).max(1);
        let i = r * self.cols() + c;
        let j = (i + if back { n - 1 } else { 1 }) % n;
        (j / self.cols().max(1), j % self.cols().max(1))
    }

    /// An arrow from a cell (← → only once the caret is at that end of its text, `at_start` / `at_end`).
    pub fn step(
        &self,
        (r, c): (usize, usize),
        dr: isize,
        dc: isize,
        at_start: bool,
        at_end: bool,
    ) -> Step {
        if (dc < 0 && !at_start) || (dc > 0 && !at_end) {
            return Step::Stay;
        }
        let (nr, nc) = (r as isize + dr, c as isize + dc);
        if nr < 0 {
            return Step::Line;
        }
        if nr as usize >= self.rows() || nc < 0 || nc as usize >= self.cols() {
            return Step::Stay;
        }
        Step::Cell(nr as usize, nc as usize)
    }

    /// The focused cell kept inside the grid after a resize.
    pub fn clamp(&self, (r, c): (usize, usize)) -> (usize, usize) {
        (
            r.min(self.rows().saturating_sub(1)),
            c.min(self.cols().saturating_sub(1)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_answers_read_as_typeset() {
        assert_eq!(pretty("[1 -1; 1 2]"), "[1 −1; 1 2]");
        assert_eq!(pretty("λ^2 - 5*λ + 5"), "λ² − 5λ + 5");
        assert_eq!(pretty("a*d - b*c"), "ad − bc");
        assert_eq!(pretty("[2*k 2; 2 2*k]"), "[2k 2; 2 2k]");
        assert_eq!(
            pretty("x = [6; 0; 0] + t*[-2; 1; 0]"),
            "x = [6; 0; 0] + t[−2; 1; 0]"
        );
        assert_eq!(pretty("2 * 3"), "2 · 3");
        assert_eq!(pretty("[k^2 2*k; 0 k^2]"), "[k² 2k; 0 k²]");
        assert_eq!(pretty("λ = 5.37228, -0.372281"), "λ = 5.37228, −0.372281");
        assert_eq!(pretty("1e-5"), "1e-5");
        assert_eq!(pretty("span{-1}"), "span{−1}");
    }

    #[test]
    fn the_shared_label_is_written_once() {
        assert_eq!(
            dual_label("λ = (5 ± sqrt(33))/2", "λ = 5.37228, -0.372281"),
            "λ = "
        );
        assert_eq!(
            dual_label(
                "λ^2 - 5*λ + 5 = 0,  λ = (5 ± sqrt(5))/2",
                "λ^2 - 5*λ + 5 = 0,  λ = 3.61803, 1.38197"
            ),
            "λ^2 - 5*λ + 5 = 0,  λ = "
        );
        assert_eq!(dual_label("x = [4/5; 7/5]", "x = [0.8; 1.4]"), "x = ");
        assert_eq!(dual_label("[1/7 0; 0 1]", "[0.142857 0; 0 1]"), "");
        // a name that isn't one word doesn't count
        assert_eq!(dual_label("a b = 1/3", "a b = 0.333"), "");
    }

    #[test]
    fn the_grid_writes_in_its_place() {
        let line = "det(matrix 2x2) + 1";
        let spot = matrix_grid::grid_command(line).expect("a grid");
        let mut g = Grid::new(spot, line);
        assert_eq!((g.rows(), g.cols()), (2, 2));
        g.cells[0][0] = "1".into();
        g.cells[1][1] = "4".into();
        let (out, caret) = g.write(line).expect("written");
        assert_eq!(out, "det([1 0; 0 4]) + 1");
        assert_eq!(&out[..caret], "det([1 0; 0 4]");
        assert_eq!(g.written, out);
        // again over what it wrote, longer
        g.cells[0][1] = "-2.5".into();
        let (out, _) = g.write(&out).expect("written");
        assert_eq!(out, "det([1 -2.5; 0 4]) + 1");
        // after a non-ASCII prefix, offsets are UTF-16
        let line = "π·matrix 1x2";
        let mut g = Grid::new(matrix_grid::grid_command(line).expect("a grid"), line);
        g.cells[0] = vec!["a".into(), "b".into()];
        assert_eq!(g.write(line).expect("written").0, "π·[a b]");
        // a line that no longer holds the spot
        g.end = 99;
        assert!(g.write("x").is_none());
    }

    #[test]
    fn ctrl_g_opens_what_is_there() {
        let s = spot_for("det([1 2; 3 4])", 6);
        assert_eq!(s.cells, vec![vec!["1", "2"], vec!["3", "4"]]);
        let s = spot_for("2 + ", 4);
        assert_eq!((s.start, s.end, s.cells.len()), (4, 4, 2));
    }

    #[test]
    fn the_grid_moves_and_grows() {
        let mut g = Grid::new(
            GridSpot {
                start: 0,
                end: 0,
                cells: vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]],
            },
            "",
        );
        assert_eq!(g.tab((0, 1), false), (1, 0));
        assert_eq!(g.tab((1, 1), false), (0, 0));
        assert_eq!(g.tab((0, 0), true), (1, 1));
        assert_eq!(g.step((0, 0), -1, 0, true, true), Step::Line);
        assert_eq!(g.step((0, 0), 1, 0, true, true), Step::Cell(1, 0));
        assert_eq!(g.step((1, 0), 1, 0, true, true), Step::Stay);
        assert_eq!(g.step((0, 0), 0, 1, true, false), Step::Stay);
        assert_eq!(g.step((0, 0), 0, 1, false, true), Step::Cell(0, 1));
        assert_eq!(g.step((0, 0), 0, -1, true, true), Step::Stay);
        assert!(g.resize(0, 1));
        assert_eq!(g.cells, vec![vec!["1", "2", ""], vec!["3", "4", ""]]);
        assert!(g.resize(-1, 0));
        assert_eq!(g.clamp((1, 2)), (0, 2));
        assert!(!g.resize(-1, 0));
        for _ in 0..10 {
            g.resize(1, 0);
        }
        assert_eq!(g.rows(), matrix_grid::MAX);
    }
}
