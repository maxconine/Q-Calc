//! The matrix grid's text rules (`qcalc_core::app::matrix_grid`), for the bars. Native only.

use qcalc_core::app::matrix_grid as core;

/// Where the grid writes in the line (UTF-16 offsets), and the cells it opens with.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct GridSpot {
    pub start: u32,
    pub end: u32,
    pub cells: Vec<Vec<String>>,
}

impl From<core::GridSpot> for GridSpot {
    fn from(s: core::GridSpot) -> Self {
        GridSpot {
            start: s.start as u32,
            end: s.end as u32,
            cells: s.cells,
        }
    }
}

/// `matrix 3x3` anywhere in the line: the empty grid ↵ opens, in its place.
#[uniffi::export]
pub fn matrix_grid_command(text: String) -> Option<GridSpot> {
    core::grid_command(&text).map(Into::into)
}

/// What the answer slot says while the line holds `matrix 3x3` (`↵ opens a 3×3 grid`).
#[uniffi::export]
pub fn matrix_grid_hint(text: String) -> Option<String> {
    core::grid_hint(&text)
}

/// The typed matrix the caret is in (else the last one), read into cells to edit.
#[uniffi::export]
pub fn matrix_grid_at(text: String, caret: u32) -> Option<GridSpot> {
    core::grid_at(&text, caret as usize).map(Into::into)
}

/// The cells written back as a matrix: `[1 2; 3 4]`, an empty cell as 0.
#[uniffi::export]
pub fn matrix_grid_literal(cells: Vec<Vec<String>>) -> String {
    core::grid_literal(&cells)
}

/// The most rows or columns the grid has.
#[uniffi::export]
pub fn matrix_grid_max() -> u32 {
    core::MAX as u32
}
