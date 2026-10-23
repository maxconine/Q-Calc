// the reference sheets the page opens in windows of their own (sheet.html), as the mac app does: the periodic table
// and the identity sheets. the page knows what's in them; this only says which window is which
use crate::place;

pub struct Sheet {
    kind: &'static str,
    id: &'static str,
    pub title: &'static str,
    // logical; the table scales to its window, a sheet scrolls in it
    pub size: (f64, f64),
    pub min: (f64, f64),
}

// how far down and right each further sheet opens from the one before
pub const CASCADE: f64 = 28.0;

// the titles are the page's own (lib/identities.ts), so the window says what the sheet's head does
pub const PERIODIC: Sheet = Sheet { kind: "periodic", id: "periodic", title: "Periodic Table", size: (860.0, 520.0), min: (430.0, 260.0) };

const IDENTITIES: [Sheet; 4] = [
    Sheet { kind: "identities", id: "identities", title: "Identities", size: (740.0, 600.0), min: (740.0, 240.0) },
    Sheet { kind: "identities", id: "calculus", title: "Derivatives and Integrals", size: (740.0, 600.0), min: (740.0, 240.0) },
    Sheet { kind: "identities", id: "conversions", title: "Unit Conversions", size: (740.0, 600.0), min: (740.0, 240.0) },
    Sheet { kind: "identities", id: "constants", title: "Physical Constants", size: (740.0, 600.0), min: (740.0, 240.0) },
];

// only the sheets there are: the id the page sends ends up in a window label and a url
pub fn identities(id: &str) -> Option<&'static Sheet> {
    IDENTITIES.iter().find(|s| s.id == id)
}

pub fn is_sheet(label: &str) -> bool {
    label.starts_with("sheet-")
}

impl Sheet {
    pub fn label(&self) -> String {
        format!("sheet-{}", self.id)
    }

    // `section` is the identity sheet's section to open at (`idt summation`), a number, so it's safe in the url
    pub fn url(&self, section: u64) -> String {
        if self.kind == "periodic" {
            "sheet.html?kind=periodic".to_string()
        } else if section > 0 {
            format!("sheet.html?kind=identities&id={}&section={section}", self.id)
        } else {
            format!("sheet.html?kind=identities&id={}", self.id)
        }
    }

    // the periodic table scales to its window, keeping its shape; an identity sheet scrolls, so it can be shorter,
    // down to its least height at the monitor's `scale`
    pub fn give(&self, scale: f64) -> place::Give {
        if self.kind == "periodic" {
            place::Give::Scale(0.6)
        } else {
            place::Give::Height(self.min.1 * scale)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_sheets_open() {
        assert_eq!(identities("calculus").map(|s| s.url(0)), Some("sheet.html?kind=identities&id=calculus".to_string()));
        assert_eq!(identities("identities").map(|s| s.url(4)), Some("sheet.html?kind=identities&id=identities&section=4".to_string()));
        assert_eq!(identities("constants").map(|s| s.title), Some("Physical Constants"));
        assert!(identities("../settings").is_none());
        assert!(identities("").is_none());
        assert_eq!(PERIODIC.url(3), "sheet.html?kind=periodic");
    }

    #[test]
    fn their_windows_are_told_apart_from_the_rest() {
        assert_eq!(PERIODIC.label(), "sheet-periodic");
        assert!(is_sheet(&identities("identities").unwrap().label()));
        assert!(!is_sheet("main"));
        assert!(!is_sheet("settings"));
    }
}
