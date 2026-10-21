// the reference sheets the page opens in windows of their own (sheet.html), as the mac app does: the periodic table
// and the identity sheets. the page knows what's in them; this only says which window is which
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

const IDENTITIES: [Sheet; 3] = [
    Sheet { kind: "identities", id: "identities", title: "Identities", size: (740.0, 600.0), min: (740.0, 240.0) },
    Sheet { kind: "identities", id: "calculus", title: "Derivatives and Integrals", size: (740.0, 600.0), min: (740.0, 240.0) },
    Sheet { kind: "identities", id: "conversions", title: "Unit Conversions", size: (740.0, 600.0), min: (740.0, 240.0) },
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

    pub fn url(&self) -> String {
        if self.kind == "periodic" {
            "sheet.html?kind=periodic".to_string()
        } else {
            format!("sheet.html?kind=identities&id={}", self.id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_sheets_open() {
        assert_eq!(identities("calculus").map(|s| s.url()), Some("sheet.html?kind=identities&id=calculus".to_string()));
        assert!(identities("../settings").is_none());
        assert!(identities("").is_none());
        assert_eq!(PERIODIC.url(), "sheet.html?kind=periodic");
    }

    #[test]
    fn their_windows_are_told_apart_from_the_rest() {
        assert_eq!(PERIODIC.label(), "sheet-periodic");
        assert!(is_sheet(&identities("identities").unwrap().label()));
        assert!(!is_sheet("main"));
        assert!(!is_sheet("settings"));
    }
}
