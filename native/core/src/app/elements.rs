//! `src/lib/elements.ts`: the periodic table's data (IUPAC abridged standard atomic weights, the conventional value
//! where the standard one is an interval, rounded to at most 3 decimals; a bracketed mass is the mass number of the
//! longest-lived isotope of an element with no standard weight), its grid, and formulas built from atoms with their
//! molar masses.
//!
//! The native periodic table window draws from [`periodic_tiles`], the same tiles `openNativePeriodicTable` in
//! `src/lib/periodic.ts` posts to the Mac shell, so the data lives in one place.

use serde::{Deserialize, Serialize};

use crate::jsnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementCategory {
    Alkali,
    Alkaline,
    Transition,
    Post,
    Metalloid,
    Nonmetal,
    Halogen,
    Noble,
    Lanthanide,
    Actinide,
}

use ElementCategory as C;

impl ElementCategory {
    /// The TS name (`'alkali'`, `'post'`…).
    pub fn as_str(self) -> &'static str {
        match self {
            C::Alkali => "alkali",
            C::Alkaline => "alkaline",
            C::Transition => "transition",
            C::Post => "post",
            C::Metalloid => "metalloid",
            C::Nonmetal => "nonmetal",
            C::Halogen => "halogen",
            C::Noble => "noble",
            C::Lanthanide => "lanthanide",
            C::Actinide => "actinide",
        }
    }
}

/// One element, as the TS `Element`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub n: u32,
    pub symbol: String,
    pub name: String,
    pub mass: String,
    pub period: u32,
    /// None for the f-block rows under the table.
    pub group: Option<u32>,
    pub category: ElementCategory,
}

/// A table row: number, symbol, name, mass, period, group, category.
type Row = (
    u32,
    &'static str,
    &'static str,
    &'static str,
    u32,
    Option<u32>,
    ElementCategory,
);

const fn el(
    n: u32,
    symbol: &'static str,
    name: &'static str,
    mass: &'static str,
    period: u32,
    group: Option<u32>,
    category: ElementCategory,
) -> Row {
    (n, symbol, name, mass, period, group, category)
}

const ROWS: [Row; 118] = [
    el(1, "H", "Hydrogen", "1.008", 1, Some(1), C::Nonmetal),
    el(2, "He", "Helium", "4.003", 1, Some(18), C::Noble),
    el(3, "Li", "Lithium", "6.94", 2, Some(1), C::Alkali),
    el(4, "Be", "Beryllium", "9.012", 2, Some(2), C::Alkaline),
    el(5, "B", "Boron", "10.81", 2, Some(13), C::Metalloid),
    el(6, "C", "Carbon", "12.011", 2, Some(14), C::Nonmetal),
    el(7, "N", "Nitrogen", "14.007", 2, Some(15), C::Nonmetal),
    el(8, "O", "Oxygen", "15.999", 2, Some(16), C::Nonmetal),
    el(9, "F", "Fluorine", "18.998", 2, Some(17), C::Halogen),
    el(10, "Ne", "Neon", "20.180", 2, Some(18), C::Noble),
    el(11, "Na", "Sodium", "22.990", 3, Some(1), C::Alkali),
    el(12, "Mg", "Magnesium", "24.305", 3, Some(2), C::Alkaline),
    el(13, "Al", "Aluminum", "26.982", 3, Some(13), C::Post),
    el(14, "Si", "Silicon", "28.085", 3, Some(14), C::Metalloid),
    el(15, "P", "Phosphorus", "30.974", 3, Some(15), C::Nonmetal),
    el(16, "S", "Sulfur", "32.06", 3, Some(16), C::Nonmetal),
    el(17, "Cl", "Chlorine", "35.45", 3, Some(17), C::Halogen),
    el(18, "Ar", "Argon", "39.95", 3, Some(18), C::Noble),
    el(19, "K", "Potassium", "39.098", 4, Some(1), C::Alkali),
    el(20, "Ca", "Calcium", "40.078", 4, Some(2), C::Alkaline),
    el(21, "Sc", "Scandium", "44.956", 4, Some(3), C::Transition),
    el(22, "Ti", "Titanium", "47.867", 4, Some(4), C::Transition),
    el(23, "V", "Vanadium", "50.942", 4, Some(5), C::Transition),
    el(24, "Cr", "Chromium", "51.996", 4, Some(6), C::Transition),
    el(25, "Mn", "Manganese", "54.938", 4, Some(7), C::Transition),
    el(26, "Fe", "Iron", "55.845", 4, Some(8), C::Transition),
    el(27, "Co", "Cobalt", "58.933", 4, Some(9), C::Transition),
    el(28, "Ni", "Nickel", "58.693", 4, Some(10), C::Transition),
    el(29, "Cu", "Copper", "63.546", 4, Some(11), C::Transition),
    el(30, "Zn", "Zinc", "65.38", 4, Some(12), C::Transition),
    el(31, "Ga", "Gallium", "69.723", 4, Some(13), C::Post),
    el(32, "Ge", "Germanium", "72.630", 4, Some(14), C::Metalloid),
    el(33, "As", "Arsenic", "74.922", 4, Some(15), C::Metalloid),
    el(34, "Se", "Selenium", "78.971", 4, Some(16), C::Nonmetal),
    el(35, "Br", "Bromine", "79.904", 4, Some(17), C::Halogen),
    el(36, "Kr", "Krypton", "83.798", 4, Some(18), C::Noble),
    el(37, "Rb", "Rubidium", "85.468", 5, Some(1), C::Alkali),
    el(38, "Sr", "Strontium", "87.62", 5, Some(2), C::Alkaline),
    el(39, "Y", "Yttrium", "88.906", 5, Some(3), C::Transition),
    el(40, "Zr", "Zirconium", "91.222", 5, Some(4), C::Transition),
    el(41, "Nb", "Niobium", "92.906", 5, Some(5), C::Transition),
    el(42, "Mo", "Molybdenum", "95.95", 5, Some(6), C::Transition),
    el(43, "Tc", "Technetium", "[98]", 5, Some(7), C::Transition),
    el(44, "Ru", "Ruthenium", "101.07", 5, Some(8), C::Transition),
    el(45, "Rh", "Rhodium", "102.91", 5, Some(9), C::Transition),
    el(46, "Pd", "Palladium", "106.42", 5, Some(10), C::Transition),
    el(47, "Ag", "Silver", "107.87", 5, Some(11), C::Transition),
    el(48, "Cd", "Cadmium", "112.41", 5, Some(12), C::Transition),
    el(49, "In", "Indium", "114.82", 5, Some(13), C::Post),
    el(50, "Sn", "Tin", "118.71", 5, Some(14), C::Post),
    el(51, "Sb", "Antimony", "121.76", 5, Some(15), C::Metalloid),
    el(52, "Te", "Tellurium", "127.60", 5, Some(16), C::Metalloid),
    el(53, "I", "Iodine", "126.90", 5, Some(17), C::Halogen),
    el(54, "Xe", "Xenon", "131.29", 5, Some(18), C::Noble),
    el(55, "Cs", "Cesium", "132.91", 6, Some(1), C::Alkali),
    el(56, "Ba", "Barium", "137.33", 6, Some(2), C::Alkaline),
    el(57, "La", "Lanthanum", "138.91", 6, None, C::Lanthanide),
    el(58, "Ce", "Cerium", "140.12", 6, None, C::Lanthanide),
    el(59, "Pr", "Praseodymium", "140.91", 6, None, C::Lanthanide),
    el(60, "Nd", "Neodymium", "144.24", 6, None, C::Lanthanide),
    el(61, "Pm", "Promethium", "[145]", 6, None, C::Lanthanide),
    el(62, "Sm", "Samarium", "150.36", 6, None, C::Lanthanide),
    el(63, "Eu", "Europium", "151.96", 6, None, C::Lanthanide),
    el(64, "Gd", "Gadolinium", "157.25", 6, None, C::Lanthanide),
    el(65, "Tb", "Terbium", "158.93", 6, None, C::Lanthanide),
    el(66, "Dy", "Dysprosium", "162.50", 6, None, C::Lanthanide),
    el(67, "Ho", "Holmium", "164.93", 6, None, C::Lanthanide),
    el(68, "Er", "Erbium", "167.26", 6, None, C::Lanthanide),
    el(69, "Tm", "Thulium", "168.93", 6, None, C::Lanthanide),
    el(70, "Yb", "Ytterbium", "173.05", 6, None, C::Lanthanide),
    el(71, "Lu", "Lutetium", "174.97", 6, None, C::Lanthanide),
    el(72, "Hf", "Hafnium", "178.49", 6, Some(4), C::Transition),
    el(73, "Ta", "Tantalum", "180.95", 6, Some(5), C::Transition),
    el(74, "W", "Tungsten", "183.84", 6, Some(6), C::Transition),
    el(75, "Re", "Rhenium", "186.21", 6, Some(7), C::Transition),
    el(76, "Os", "Osmium", "190.23", 6, Some(8), C::Transition),
    el(77, "Ir", "Iridium", "192.22", 6, Some(9), C::Transition),
    el(78, "Pt", "Platinum", "195.08", 6, Some(10), C::Transition),
    el(79, "Au", "Gold", "196.97", 6, Some(11), C::Transition),
    el(80, "Hg", "Mercury", "200.59", 6, Some(12), C::Transition),
    el(81, "Tl", "Thallium", "204.38", 6, Some(13), C::Post),
    el(82, "Pb", "Lead", "207.2", 6, Some(14), C::Post),
    el(83, "Bi", "Bismuth", "208.98", 6, Some(15), C::Post),
    el(84, "Po", "Polonium", "[209]", 6, Some(16), C::Post),
    el(85, "At", "Astatine", "[210]", 6, Some(17), C::Halogen),
    el(86, "Rn", "Radon", "[222]", 6, Some(18), C::Noble),
    el(87, "Fr", "Francium", "[223]", 7, Some(1), C::Alkali),
    el(88, "Ra", "Radium", "[226]", 7, Some(2), C::Alkaline),
    el(89, "Ac", "Actinium", "[227]", 7, None, C::Actinide),
    el(90, "Th", "Thorium", "232.04", 7, None, C::Actinide),
    el(91, "Pa", "Protactinium", "231.04", 7, None, C::Actinide),
    el(92, "U", "Uranium", "238.03", 7, None, C::Actinide),
    el(93, "Np", "Neptunium", "[237]", 7, None, C::Actinide),
    el(94, "Pu", "Plutonium", "[244]", 7, None, C::Actinide),
    el(95, "Am", "Americium", "[243]", 7, None, C::Actinide),
    el(96, "Cm", "Curium", "[247]", 7, None, C::Actinide),
    el(97, "Bk", "Berkelium", "[247]", 7, None, C::Actinide),
    el(98, "Cf", "Californium", "[251]", 7, None, C::Actinide),
    el(99, "Es", "Einsteinium", "[252]", 7, None, C::Actinide),
    el(100, "Fm", "Fermium", "[257]", 7, None, C::Actinide),
    el(101, "Md", "Mendelevium", "[258]", 7, None, C::Actinide),
    el(102, "No", "Nobelium", "[259]", 7, None, C::Actinide),
    el(103, "Lr", "Lawrencium", "[266]", 7, None, C::Actinide),
    el(
        104,
        "Rf",
        "Rutherfordium",
        "[267]",
        7,
        Some(4),
        C::Transition,
    ),
    el(105, "Db", "Dubnium", "[268]", 7, Some(5), C::Transition),
    el(106, "Sg", "Seaborgium", "[267]", 7, Some(6), C::Transition),
    el(107, "Bh", "Bohrium", "[270]", 7, Some(7), C::Transition),
    el(108, "Hs", "Hassium", "[271]", 7, Some(8), C::Transition),
    el(109, "Mt", "Meitnerium", "[278]", 7, Some(9), C::Transition),
    el(
        110,
        "Ds",
        "Darmstadtium",
        "[281]",
        7,
        Some(10),
        C::Transition,
    ),
    el(
        111,
        "Rg",
        "Roentgenium",
        "[282]",
        7,
        Some(11),
        C::Transition,
    ),
    el(
        112,
        "Cn",
        "Copernicium",
        "[285]",
        7,
        Some(12),
        C::Transition,
    ),
    el(113, "Nh", "Nihonium", "[286]", 7, Some(13), C::Post),
    el(114, "Fl", "Flerovium", "[289]", 7, Some(14), C::Post),
    el(115, "Mc", "Moscovium", "[290]", 7, Some(15), C::Post),
    el(116, "Lv", "Livermorium", "[293]", 7, Some(16), C::Post),
    el(117, "Ts", "Tennessine", "[294]", 7, Some(17), C::Halogen),
    el(118, "Og", "Oganesson", "[294]", 7, Some(18), C::Noble),
];

/// Every element, in order of atomic number (`ELEMENTS`).
pub fn elements() -> &'static [Element] {
    static ALL: std::sync::LazyLock<Vec<Element>> = std::sync::LazyLock::new(|| {
        ROWS.iter()
            .map(
                |&(n, symbol, name, mass, period, group, category)| Element {
                    n,
                    symbol: symbol.into(),
                    name: name.into(),
                    mass: mass.into(),
                    period,
                    group,
                    category,
                },
            )
            .collect()
    });
    &ALL
}

/// The element with this symbol (`BY_SYMBOL`), case-sensitive.
pub fn by_symbol(symbol: &str) -> Option<&'static Element> {
    elements().iter().find(|e| e.symbol == symbol)
}

pub fn has_standard_weight(e: &Element) -> bool {
    !e.mass.starts_with('[')
}

/// What a click inserts: the weight, or a bracketed mass number without its brackets
/// (`e.mass.replace(/^\[(\d+)\]$/, '$1')`).
pub fn insertable_mass(e: &Element) -> String {
    match e.mass.strip_prefix('[').and_then(|m| m.strip_suffix(']')) {
        Some(d) if !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()) => d.to_string(),
        _ => e.mass.clone(),
    }
}

/// A 1-based grid cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCell {
    pub row: i64,
    pub col: i64,
}

/// Rows 1–7 are the table, row 8 is a gap, 9 and 10 hold the lanthanides and actinides from column 3.
pub fn grid_cell(e: &Element) -> GridCell {
    if let Some(group) = e.group {
        return GridCell {
            row: i64::from(e.period),
            col: i64::from(group),
        };
    }
    let first = if e.period == 6 { 57 } else { 89 };
    GridCell {
        row: i64::from(e.period) + 3,
        col: 3 + i64::from(e.n) - first,
    }
}

const SUBSCRIPTS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];

/// ASCII digits as subscript digits; anything else unchanged.
pub fn subscript_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0'..='9' => SUBSCRIPTS[c as usize - '0' as usize],
            _ => c,
        })
        .collect()
}

fn subscript(n: f64) -> String {
    if n == 1.0 {
        String::new()
    } else {
        subscript_digits(&jsnum::to_string(n))
    }
}

// thousandths, so a sum of masses comes out exact: 2 × 1.008 + 15.999 is 18.015, not 18.015000000000001
fn milli(e: &Element) -> f64 {
    jsnum::round(jsnum::parse(&insertable_mass(e)).unwrap_or(f64::NAN) * 1000.0)
}

fn grams(m: f64) -> String {
    jsnum::to_string(m / 1000.0)
}

/// `text` is the part as the formula writes it, H₂; `mass` is all of that element's share.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormulaPart {
    pub symbol: String,
    pub count: f64,
    pub text: String,
    pub mass: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Formula {
    pub formula: String,
    pub plain: String,
    pub mass: String,
    pub parts: Vec<FormulaPart>,
}

/// Atoms and counts, in the order given, as one formula and its molar mass in g/mol; None for an unknown symbol
/// or a count that isn't positive.
pub fn formula_of<'a>(atoms: impl IntoIterator<Item = (&'a str, f64)>) -> Option<Formula> {
    let mut counts: Vec<(&'static Element, f64)> = Vec::new();
    for (symbol, n) in atoms {
        let e = by_symbol(symbol)?;
        // `!(n > 0)` also turns away NaN
        if n.is_nan() || n <= 0.0 {
            return None;
        }
        match counts.iter_mut().find(|(c, _)| c.symbol == e.symbol) {
            Some((_, k)) => *k += n,
            None => counts.push((e, n)),
        }
    }
    if counts.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let parts: Vec<FormulaPart> = counts
        .iter()
        .map(|&(e, count)| {
            let m = milli(e) * count;
            total += m;
            FormulaPart {
                symbol: e.symbol.clone(),
                count,
                text: format!("{}{}", e.symbol, subscript(count)),
                mass: grams(m),
            }
        })
        .collect();
    Some(Formula {
        formula: parts.iter().map(|p| p.text.as_str()).collect(),
        plain: parts
            .iter()
            .map(|p| {
                if p.count == 1.0 {
                    p.symbol.clone()
                } else {
                    format!("{}{}", p.symbol, jsnum::to_string(p.count))
                }
            })
            .collect(),
        mass: grams(total),
        parts,
    })
}

/// The periodic table's picks, a symbol a click, grouped by element in the order first picked: H, H, O is H₂O.
pub fn formula_of_picks<S: AsRef<str>>(symbols: &[S]) -> Option<Formula> {
    formula_of(symbols.iter().map(|s| (s.as_ref(), 1.0)))
}

/// One tile of the native periodic table window: what `openNativePeriodicTable` posts to the Mac shell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodicTile {
    pub n: u32,
    pub symbol: String,
    pub name: String,
    pub mass: String,
    /// What a click inserts in the bar ([`insertable_mass`]).
    pub insert: String,
    pub row: i64,
    pub col: i64,
    pub category: ElementCategory,
}

/// Every element as a tile of the native window, in order of atomic number.
pub fn periodic_tiles() -> Vec<PeriodicTile> {
    elements()
        .iter()
        .map(|e| {
            let GridCell { row, col } = grid_cell(e);
            PeriodicTile {
                n: e.n,
                symbol: e.symbol.clone(),
                name: e.name.clone(),
                mass: e.mass.clone(),
                insert: insertable_mass(e),
                row,
                col,
                category: e.category,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_whole() {
        let all = elements();
        assert_eq!(all.len(), 118);
        assert!(all.iter().enumerate().all(|(i, e)| e.n as usize == i + 1));
        assert_eq!(insertable_mass(by_symbol("Tc").unwrap()), "98");
        assert_eq!(
            grid_cell(by_symbol("Lu").unwrap()),
            GridCell { row: 9, col: 17 }
        );
    }

    #[test]
    fn formulas_sum_in_thousandths() {
        let water = formula_of_picks(&["H", "H", "O"]).unwrap();
        assert_eq!(water.formula, "H₂O");
        assert_eq!(water.mass, "18.015");
        assert!(formula_of([("H", 0.0)]).is_none());
        assert!(formula_of_picks(&["Xx"]).is_none());
        let tiles = periodic_tiles();
        assert_eq!(tiles[0].insert, "1.008");
        assert_eq!(tiles[56].row, 9);
    }
}
