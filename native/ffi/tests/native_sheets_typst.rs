//! Every cell of the native formula sheets compiles in Typst, so none of them falls out of a sheet when it opens.
#![cfg(feature = "typst")]

use qcalc_core::app::identities::{
    IdentitySheet, native_identity_sheet_by_id, sheet_math_document, sheets,
};

/// Every sheet with its Typst filled in, as the apps get it.
fn typeset_sheets() -> Vec<IdentitySheet> {
    sheets()
        .iter()
        .filter_map(|s| native_identity_sheet_by_id(&s.id))
        .collect()
}
use qcalc_ffi::TypstRenderer;

#[test]
fn every_formula_sheet_cell_compiles() {
    let typst = TypstRenderer::new();
    let mut bad = Vec::new();
    let mut compile = |doc: String, what: &str| {
        if let Err(e) = typst.paths(doc) {
            bad.push(format!("{what}: {e:?}"));
        }
    };
    let mut cells = 0;
    for sheet in &typeset_sheets() {
        for item in sheet
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
        {
            if let Some(figure) = &item.figure {
                compile(figure.clone(), "figure");
                for spot in item.spots.iter().flatten() {
                    compile(spot.doc.clone(), &spot.copy);
                }
            }
            for part in item.parts.iter().flatten() {
                compile(sheet_math_document(&part.typst), &part.typst);
            }
            let Some(typeset) = &item.typst else { continue };
            for cell in typeset {
                cells += 1;
                compile(sheet_math_document(cell), cell);
            }
            if let Some(note) = &item.note_typst {
                compile(
                    sheet_math_document(&format!("#text(size: 10.5pt)[${note}$]")),
                    note,
                );
            }
        }
    }
    assert!(cells > 300, "{cells}");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Nothing in a row runs into the next column or its note, at the widths the sheet window lays them out.
#[test]
fn every_row_fits() {
    let typst = TypstRenderer::new();
    let width = |doc: String| typst.paths(doc).map_or(0.0, |p| f64::from(p.width_pt));
    let mut bad = Vec::new();
    for sheet in &typeset_sheets() {
        for section in &sheet.sections {
            let span = if section.wide == Some(true) {
                704.0
            } else {
                344.0
            };
            let columns: Vec<f64> = section
                .lead_widths
                .clone()
                .unwrap_or_else(|| vec![64, 28])
                .iter()
                .map(|w| f64::from(*w))
                .collect();
            for item in section.groups.iter().flat_map(|g| &g.items) {
                let Some(cells) = &item.typst else { continue };
                let note = match (&item.note_typst, &item.note) {
                    (Some(maths), _) => width(sheet_math_document(&format!(
                        "#text(size: 10.5pt)[${maths}$]"
                    ))),
                    // words in the app's own font, about this wide at 11 pt
                    (None, Some(words)) => words.chars().count() as f64 * 5.6,
                    _ => 0.0,
                };
                let mut x = 6.0;
                for (i, cell) in cells.iter().enumerate() {
                    let w = width(sheet_math_document(cell));
                    if i + 1 < cells.len() {
                        let column = columns[i.min(columns.len() - 1)];
                        if w > column {
                            bad.push(format!(
                                "{}: {cell} is {w:.0} wide in a {column} column",
                                sheet.id
                            ));
                        }
                        x += column;
                    } else if x + w + 8.0 + note + 6.0 > span {
                        bad.push(format!(
                            "{}: {cell} needs {:.0} of {span}",
                            sheet.id,
                            x + w + 8.0 + note + 6.0
                        ));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
