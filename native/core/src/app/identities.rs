//! The reference sheets (`src/lib/identities.ts`): trig and log identities, derivatives and integrals, unit
//! conversions and CODATA constants, which bar line opens which, and the raised/lowered runs a line draws as.
//!
//! The sheets' data is `identities.json`, written from the TS constants (`IDENTITY_SHEET`, `CALCULUS_SHEET`,
//! `CONVERSION_SHEET`, `CONSTANTS_SHEET`, in that order) by `JSON.stringify`, so it is the TS data exactly.
//! Regenerate it when the TS sheets change.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::app::js as jstr;
use crate::jsre::{self, Lazy, js};

/// One line: `show` reads with `^{…}` raised and `_{…}` lowered; `copy` is what a click puts on the clipboard, and
/// an empty one makes the line a table's heading, which isn't clicked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<Vec<String>>,
    pub show: String,
    pub copy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Native only: what a click on each lead cell copies, in place of `copy`; an empty one (a label) leaves `copy`.
    #[serde(rename = "leadCopy", default, skip_serializing_if = "Option::is_none")]
    pub lead_copy: Option<Vec<String>>,
    /// Native only: the bar line a click runs (the formulas page), in place of a copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    /// Native only: Typst maths for each lead cell and then `show`, for the apps that typeset the sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typst: Option<Vec<String>>,
    /// Native only: `note` as Typst maths, when it's maths rather than words.
    #[serde(rename = "noteTypst", default, skip_serializing_if = "Option::is_none")]
    pub note_typst: Option<String>,
    /// Native only: a whole Typst page drawn in place of the row (the unit circle). A sheet with one shows only it,
    /// where the app can typeset; elsewhere the row is blank and the sheet's other sections carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<String>,
    /// Native only: the row broken into pieces that copy alone (a conversion's number and unit), drawn in a line;
    /// a piece with no `copy` is only drawn, and a click on it copies the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<RowPart>>,
    /// Native only: the labels on a figure, each its own Typst document, clicked to copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spots: Option<Vec<FigureSpot>>,
}

/// A piece of a row: its Typst maths, and what a click on it copies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowPart {
    pub typst: String,
    pub copy: String,
}

/// A label on a figure, centred on (x, y) in the figure's points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FigureSpot {
    pub x: f64,
    pub y: f64,
    pub doc: String,
    pub copy: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdentityGroup {
    pub title: String,
    pub items: Vec<Identity>,
}

/// A `wide` section's groups go one under another at the sheet's full width.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdentitySection {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wide: Option<bool>,
    /// Native only: this section's lead columns, in place of `LEAD_WIDTHS`, for a table with wide cells.
    #[serde(
        rename = "leadWidths",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub lead_widths: Option<Vec<u32>>,
    pub groups: Vec<IdentityGroup>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdentitySheet {
    /// `identities`, `calculus`, `conversions` or `constants`.
    pub id: String,
    pub title: String,
    pub hint: String,
    /// Native only: a named formula's sheet, and the lines that open it (`Ohm's law`, `ohm law`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    pub sections: Vec<IdentitySection>,
}

/// The widths, in points, of a row's `lead` cells.
pub const LEAD_WIDTHS: [u32; 2] = [64, 28];

/// The web's four sheets, in the TS order, then the native apps' own (`native_sheets.json`).
pub fn sheets() -> &'static [IdentitySheet] {
    static SHEETS: LazyLock<Vec<IdentitySheet>> = LazyLock::new(|| {
        let mut all: Vec<IdentitySheet> =
            serde_json::from_str(include_str!("identities.json")).unwrap_or_default();
        let native: Vec<IdentitySheet> =
            serde_json::from_str(include_str!("native_sheets.json")).unwrap_or_default();
        all.extend(native);
        let parts: Vec<IdentitySheet> = all.iter().flat_map(part_sheets).collect();
        all.extend(parts);
        all
    });
    &SHEETS
}

fn sheet(id: &str) -> &'static IdentitySheet {
    sheets().iter().find(|s| s.id == id).unwrap_or(&sheets()[0])
}

/// The sheet with this id (the Windows sheet window's page is told only the id).
pub fn identity_sheet_by_id(id: &str) -> Option<&'static IdentitySheet> {
    sheets().iter().find(|s| s.id == id)
}

// `identities <part>` opens the sheet holding that part, at it; the first name is the one completion offers
struct Part {
    names: &'static str,
    sheet: &'static str,
    section: &'static str,
}

const PARTS: [Part; 9] = [
    Part {
        names: r"^trig(?:onometry|onometric)?$",
        sheet: "identities",
        section: "Trig",
    },
    Part {
        names: r"^(?:logs?|logarithms?|exponents?)$",
        sheet: "identities",
        section: "Logs and exponents",
    },
    Part {
        names: r"^algebra$",
        sheet: "identities",
        section: "Algebra",
    },
    Part {
        names: r"^(?:integration|integrals?|antiderivatives?)$",
        sheet: "calculus",
        section: "Integrals",
    },
    Part {
        names: r"^(?:partial(?: fractions?)?|pf)$",
        sheet: "identities",
        section: "Partial fractions",
    },
    Part {
        names: r"^(?:summations?|sums?|series|sigma)$",
        sheet: "identities",
        section: "Summation",
    },
    Part {
        names: r"^(?:derivatives?|differentiation)$",
        sheet: "calculus",
        section: "Derivatives",
    },
    Part {
        names: r"^(?:(?:physical )?constants?|codata)$",
        sheet: "constants",
        section: "CODATA 2022",
    },
    Part {
        names: r"^(?:conversions?|units?|prefixes)$",
        sheet: "conversions",
        section: "Metric prefixes",
    },
];

/// The subfields `identities` takes, in completion's order.
pub const IDENTITY_PARTS: [&str; 9] = [
    "trig",
    "logs",
    "algebra",
    "integration",
    "partial fractions",
    "summation",
    "derivatives",
    "constants",
    "conversions",
];

/// Native only: the sheets that aren't the web's, each with the lines that open it. A sheet's id is always one of
/// them, because the Mac app looks a sheet up by typing its id.
const NATIVE_COMMANDS: [(&str, &str); 28] = [
    (
        "precalculus",
        r"^(?:precalculus|pre[-\s]calc(?:ulus)?)(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "differential equations",
        r"^(?:differential\s+equations?|diffeq|odes?|pdes?)(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "numerical methods",
        r"^(?:numerical\s+(?:methods|analysis))(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "dynamics",
        r"^(?:dynamics|statics\s+and\s+dynamics)(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "heat transfer",
        r"^heat\s+transfer(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "computer science",
        r"^(?:computer\s+science|comp\s*sci|cs\s+(?:formulas|sheet))(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "machine learning",
        r"^(?:machine\s+learning|ml\s+(?:formulas|sheet))(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "astronomy",
        r"^(?:astronomy|astrophysics)(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "finance",
        r"^(?:finance|financial)(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "chemistry",
        r"^(?:chemistry(?:\s+(?:formulas|equations|sheet))?|chem\s+(?:formulas|equations|sheet))$",
    ),
    (
        "biology",
        r"^(?:biology|bio)(?:\s+(?:formulas|equations|sheet))?$",
    ),
    (
        "continuum mechanics",
        r"^(?:continuum(?:\s+mechanics)?|(?:mechanics|strength)\s+of\s+materials|solid\s+mechanics)(?:\s+(?:formulas|equations|sheet))?$",
    ),
    (
        "fourier",
        r"^(?:fourier(?:\s+series)?(?:\s+(?:formulas?|table|sheet))?|ctfs|dtfs)$",
    ),
    (
        "euler",
        r"^euler(?:['’]?s)?(?:\s+(?:formulas?|identit(?:y|ies)|sheet))?$",
    ),
    (
        "complex numbers",
        r"^(?:special\s+)?complex(?:\s+numbers?)?(?:\s+(?:in\s+polar(?:\s+form)?|polar|table|sheet))?$|^polar\s+form$",
    ),
    (
        "ct transform table",
        r"^(?:ct|continuous[-\s]time)\s+(?:fourier\s+)?(?:transforms?(?:\s+table)?|table)$|^ctft(?:\s+table)?$|^fourier\s+transforms?(?:\s+table)?$",
    ),
    (
        "dt transform table",
        r"^(?:dt|discrete[-\s]time)\s+(?:fourier\s+)?(?:transforms?(?:\s+table)?|table)$|^dtft(?:\s+table)?$",
    ),
    ("laplace", r"^laplace(?:\s+transforms?)?(?:\s+table)?$"),
    ("z transform", r"^z[-\s]?transforms?(?:\s+table)?$"),
    (
        "unit circle",
        r"^unit\s+circle(?:\s+table)?$|^(?:exact\s+)?trig\s+(?:values|table)$|^special\s+angles$",
    ),
    (
        "geometry",
        r"^geometry(?:\s+(?:formulas|sheet))?$|^(?:area|volume)\s+formulas$",
    ),
    (
        "statistics",
        r"^(?:statistics|stats)(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "linear algebra",
        r"^(?:linear\s+algebra|linalg)(?:\s+(?:formulas|sheet))?$|^matrix\s+(?:formulas|rules)$",
    ),
    ("physics", r"^physics(?:\s+(?:formulas|equations|sheet))?$"),
    (
        "circuits",
        r"^(?:circuits?|electronics|electrical(?:\s+engineering)?|ee\s+(?:formulas|equations|sheet))(?:\s+(?:formulas|equations|sheet))?$",
    ),
    (
        "taylor series",
        r"^(?:taylor|maclaurin)(?:\s+series)?(?:\s+(?:formulas|table|sheet))?$",
    ),
    (
        "vector calculus",
        r"^vector\s+calc(?:ulus)?(?:\s+(?:formulas|sheet))?$",
    ),
    (
        "formulas",
        r"^formulas?$|^formula\s+sheets?$|^cheat\s*sheets?$|^sheets$",
    ),
];

/// A cell of a typeset sheet: `math` as Typst, centred on its maths axis (the hidden bracket gives every cell the
/// same height at least, so cells in a row line up; it goes last, or a leading minus would read as subtraction), in the colour the app draws text in.
pub fn sheet_math_document(math: &str) -> String {
    format!(
        "#set page(width: auto, height: auto, margin: 1pt, fill: none)\n#set text(size: 13pt, fill: black)\n$ {math} #box(width: 0pt, hide($($)) $\n"
    )
}

/// Native only: a sheet of just one part of another, so `beams` shows beams and nothing else. Its id is
/// `<sheet>: <section>`, or `<sheet>: <section>: <group>` for the groups in `GROUP_PARTS`.
fn part_sheets(sheet: &IdentitySheet) -> Vec<IdentitySheet> {
    if sheet.id == "formulas" || sheet.names.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for section in &sheet.sections {
        out.push(IdentitySheet {
            id: format!("{}: {}", sheet.id, section.title),
            title: section.title.clone(),
            hint: section.title.to_lowercase(),
            names: None,
            sections: vec![section.clone()],
        });
        for group in &section.groups {
            let id = format!("{}: {}: {}", sheet.id, section.title, group.title);
            if GROUP_PARTS.contains(&id.as_str()) {
                out.push(IdentitySheet {
                    id,
                    title: group.title.clone(),
                    hint: group.title.to_lowercase(),
                    names: None,
                    sections: vec![IdentitySection {
                        groups: vec![group.clone()],
                        ..section.clone()
                    }],
                });
            }
        }
    }
    out
}

/// The groups that open on their own (see `PART_WORDS`); any section does.
const GROUP_PARTS: [&str; 2] = [
    "continuum mechanics: Axial loading and indeterminate problems: Pressure vessels (thin wall)",
    "physics: Mechanics: Constant acceleration",
];

/// Section titles that say nothing without their sheet: `laplace pairs` opens, `pairs` doesn't.
const VAGUE_TITLES: [&str; 6] = [
    "definition",
    "pairs",
    "properties",
    "general",
    "basics",
    "operators",
];

/// Other words for a part, beyond its own title.
const PART_WORDS: [(&str, &str); 19 + 15 + 8] = [
    (
        r"^(?:phasors?|ac\s+(?:power|circuits?)|power\s+factor|three[-\s]phase|transformers?)$",
        "circuits: AC power and phasors",
    ),
    (
        r"^(?:rlc|filters?|bode(?:\s+plots?)?|resonance|damping)$",
        "circuits: RLC and filters",
    ),
    (
        r"^(?:op[-\s]?amps?|operational\s+amplifiers?)$",
        "circuits: Op amps",
    ),
    (
        r"^(?:transistors?|diodes?|bjts?|mosfets?|semiconductors?)$",
        "circuits: Diodes and transistors",
    ),
    (
        r"^(?:digital(?:\s+logic)?|logic\s+gates?|boolean(?:\s+algebra)?|de\s+morgan|adc|dac|nyquist)$",
        "circuits: Digital and data conversion",
    ),
    (
        r"^(?:decibels?|noise|shannon|communications?|friis)$",
        "circuits: Signals, noise and communication",
    ),
    (
        r"^(?:transmission\s+lines?|electromagnetics|skin\s+depth|vswr)$",
        "circuits: Electromagnetics and transmission lines",
    ),
    (
        r"^(?:motors?|power\s+electronics|buck(?:[-\s]boost)?|boost(?:\s+converter)?)$",
        "circuits: Motors and power electronics",
    ),
    (r"^(?:thermo|heat\s+engines?)$", "physics: Thermodynamics"),
    (
        r"^(?:e\s*&\s*m|electromagnetism|magnetism|maxwell(?:'?s)?(?:\s+equations)?)$",
        "physics: Electricity and magnetism",
    ),
    (
        r"^(?:optics|lenses|sound|waves)$",
        "physics: Waves, sound and optics",
    ),
    (
        r"^(?:relativity|quantum|nuclear|radioactivity)$",
        "physics: Relativity, quantum and nuclear",
    ),
    (
        r"^(?:stoichiometry|moles|molarity|solutions)$",
        "chemistry: Amounts and solutions",
    ),
    (r"^(?:gases|ideal\s+gas(?:\s+law)?)$", "chemistry: Gas laws"),
    (
        r"^(?:thermochem|gibbs(?:\s+free\s+energy)?)$",
        "chemistry: Thermochemistry",
    ),
    (
        r"^(?:equilibrium|acids?(?:\s+and\s+bases)?|ph|henderson[-\s]hasselbalch)$",
        "chemistry: Equilibrium, acids and bases",
    ),
    (r"^(?:rate\s+laws?|arrhenius)$", "chemistry: Kinetics"),
    (
        r"^(?:nernst|galvanic\s+cells?)$",
        "chemistry: Electrochemistry",
    ),
    (
        r"^(?:colligative|osmotic\s+pressure)$",
        "chemistry: Colligative properties and light",
    ),
    (
        r"^(?:population\s+growth|ecology|logistic\s+growth)$",
        "biology: Populations and ecology",
    ),
    (
        r"^(?:hardy[-\s]weinberg|chi[-\s]square)$",
        "biology: Genetics",
    ),
    (
        r"^(?:michaelis[-\s]menten|enzyme\s+kinetics)$",
        "biology: Enzymes",
    ),
    (r"^(?:photosynthesis|respiration)$", "biology: Metabolism"),
    (
        r"^(?:stress|strain|principal\s+stress(?:es)?|mohr'?s?(?:\s+circle)?)$",
        "continuum mechanics: Stress and strain",
    ),
    (
        r"^(?:constitutive|hooke'?s?(?:\s+law)?|elastic\s+moduli|moduli)$",
        "continuum mechanics: Constitutive relations",
    ),
    (
        r"^(?:tensors?|index\s+notation)$",
        "continuum mechanics: Tensor analysis",
    ),
    (
        r"^(?:axial(?:\s+loading)?|(?:statically\s+)?indeterminate(?:\s+problems)?)$",
        "continuum mechanics: Axial loading and indeterminate problems",
    ),
    (
        r"^pressure\s+vessels?$",
        "continuum mechanics: Axial loading and indeterminate problems: Pressure vessels (thin wall)",
    ),
    (r"^shafts?$", "continuum mechanics: Torsion"),
    (
        r"^(?:beams?(?:\s+(?:theory|equations|formulas|deflections?))?|bending|deflections?)$",
        "continuum mechanics: Beams",
    ),
    (
        r"^(?:fluids?(?:\s+mechanics)?|bernoulli(?:'?s)?(?:\s+(?:principle|equation))?|hydrostatics)$",
        "continuum mechanics: Fluids",
    ),
    (
        r"^(?:control\s+volumes?|conservation(?:\s+laws)?|reynolds\s+transport|navier[-\s]stokes)$",
        "continuum mechanics: Conservation laws on a control volume",
    ),
    (r"^kinematics$", "physics: Mechanics: Constant acceleration"),
    (r"^dft$", "dt transform table: DFT"),
    (r"^de\s+moivre$", "euler: Euler's formulas"),
    (r"^polar$", "complex numbers: Rectangular and polar"),
    (
        r"^(?:logs|logarithms?|exponents)$",
        "identities: Logs and exponents",
    ),
    (r"^(?:partial|pf)$", "identities: Partial fractions"),
    (
        r"^(?:sums|summations|series|sigma)$",
        "identities: Summation",
    ),
    (r"^(?:trig(?:onometry|onometric)?)$", "identities: Trig"),
    (
        r"^(?:integration|integrals?|antiderivatives?)$",
        "calculus: Integrals",
    ),
    (
        r"^(?:derivatives?|differentiation)$",
        "calculus: Derivatives",
    ),
];

/// The part a line names, by another word for it, its section's title, or `<sheet> <title>`.
fn part_for(line: &str) -> Option<String> {
    // a named formula first
    static NAMES: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
        let mut map = std::collections::HashMap::new();
        for sheet in sheets() {
            for name in sheet.names.iter().flatten() {
                map.insert(name_key(name), sheet.id.clone());
            }
        }
        map
    });
    if let Some(id) = NAMES.get(&name_key(line)) {
        return Some(id.clone());
    }
    // a sheet's own name is that sheet, whatever other sheet has a section called the same
    if native_sheet_for(line).is_some() {
        return None;
    }
    static TITLES: LazyLock<std::collections::HashMap<String, Option<String>>> =
        LazyLock::new(|| {
            let mut map = std::collections::HashMap::new();
            for sheet in sheets()
                .iter()
                .filter(|s| !s.id.contains(": ") && s.id != "formulas")
            {
                for section in &sheet.sections {
                    let title = section.title.to_lowercase();
                    let id = format!("{}: {}", sheet.id, section.title);
                    map.insert(format!("{} {title}", sheet.id), Some(id.clone()));
                    if !VAGUE_TITLES.contains(&title.as_str()) {
                        // in two sheets it's vague too
                        map.entry(title)
                            .and_modify(|at: &mut Option<String>| *at = None)
                            .or_insert(Some(id));
                    }
                }
            }
            map
        });
    static RES: crate::jsre::Lazy<Vec<jsre::Regex>> =
        crate::jsre::Lazy::new(|| PART_WORDS.iter().map(|(names, _)| js(names, "i")).collect());
    let lower = line.to_lowercase();
    PART_WORDS
        .iter()
        .zip(RES.iter())
        .find(|(_, re)| jsre::test(re, &lower))
        .map(|((_, id), _)| (*id).to_string())
        .or_else(|| TITLES.get(&lower).cloned().flatten())
}

/// How a formula's name compares: any case, spaces evened out, with or without its apostrophes.
fn name_key(name: &str) -> String {
    name.to_lowercase()
        .replace(['\'', '’'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every line a sheet, part or formula opens by, for completion: shortest first, so `newton` offers the shortest.
pub fn sheet_line_names() -> &'static [String] {
    static LINES: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut all = std::collections::BTreeSet::new();
        for sheet in sheets() {
            if let Some(names) = &sheet.names {
                for name in names {
                    all.insert(name.to_lowercase());
                    all.insert(name_key(name));
                }
            } else if sheet.id.contains(": ") {
                let title = sheet.title.to_lowercase();
                if !VAGUE_TITLES.contains(&title.as_str()) {
                    all.insert(title);
                }
            } else {
                all.insert(sheet.id.clone());
            }
        }
        let mut lines: Vec<String> = all.into_iter().collect();
        lines.sort_by_key(|l| l.len());
        lines
    });
    &LINES
}

/// Whether a line's first word could start a part's title (for a host's quick test of what could be a command).
pub fn part_could_start(first: &str) -> bool {
    static FIRST: LazyLock<std::collections::HashSet<String>> = LazyLock::new(|| {
        sheets()
            .iter()
            .flat_map(|s| {
                s.names
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .chain([s.title.clone(), s.id.clone()])
            })
            .filter_map(|name| {
                let first: String = name
                    .to_lowercase()
                    .chars()
                    .take_while(|c| c.is_alphanumeric())
                    .collect();
                (!first.is_empty()).then_some(first)
            })
            .collect()
    });
    FIRST.contains(first)
}

/// What the native apps open for a line: just the part it names, when it names one (`trig` shows only trig), else
/// what `identity_target_for`, held to the web's answers, says. `idt <part>` is the same.
pub fn native_target_for(text: &str) -> Option<IdentityTarget> {
    static PART_COMMAND: Lazy = Lazy::new(|| js(r"^(?:identities|identity|idt)\s+(.+)$", "i"));
    let line = jstr::trim(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let named = PART_COMMAND
        .captures(line.as_str())
        .ok()
        .flatten()
        .map(|c| jsre::group(&c, 1).to_string());
    part_for(named.as_deref().unwrap_or(&line))
        .map(|id| at(&id, None))
        .or_else(|| identity_target_for(text))
}

/// A sheet as the native apps typeset it: the web's four sheets, and their parts, keep their data as the web has it,
/// so their Typst comes from a table beside it (`native_typst_cells.json`).
pub fn native_identity_sheet_by_id(id: &str) -> Option<IdentitySheet> {
    static CELLS: LazyLock<
        std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    > = LazyLock::new(|| {
        let all: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(include_str!("native_typst_cells.json")).unwrap_or_default();
        all.into_iter()
            .filter(|(id, _)| id != "parts")
            .filter_map(|(id, table)| Some((id, serde_json::from_value(table).ok()?)))
            .collect()
    });
    #[derive(Deserialize)]
    struct Parts {
        parts: std::collections::HashMap<String, Vec<RowPart>>,
    }
    static PARTS: LazyLock<std::collections::HashMap<String, Vec<RowPart>>> = LazyLock::new(|| {
        serde_json::from_str::<Parts>(include_str!("native_typst_cells.json"))
            .map(|p| p.parts)
            .unwrap_or_default()
    });
    let mut sheet = identity_sheet_by_id(id)?.clone();
    let base = id.split(": ").next().unwrap_or(id);
    if let Some(table) = CELLS.get(base) {
        for item in sheet
            .sections
            .iter_mut()
            .flat_map(|s| &mut s.groups)
            .flat_map(|g| &mut g.items)
            .filter(|i| i.typst.is_none())
        {
            let cells: Option<Vec<String>> = item
                .lead
                .iter()
                .flatten()
                .chain(std::iter::once(&item.show))
                .map(|cell| table.get(cell).cloned())
                .collect();
            item.typst = cells;
            item.parts = PARTS.get(&item.show).cloned();
        }
    }
    Some(sheet)
}

/// The native sheet a line (or `idt <line>`) names.
fn native_sheet_for(line: &str) -> Option<&'static str> {
    static RES: crate::jsre::Lazy<Vec<jsre::Regex>> = crate::jsre::Lazy::new(|| {
        NATIVE_COMMANDS
            .iter()
            .map(|(_, names)| js(names, "i"))
            .collect()
    });
    NATIVE_COMMANDS
        .iter()
        .zip(RES.iter())
        .find(|(_, re)| jsre::test(re, line))
        .map(|((id, _), _)| *id)
}

/// What completion finishes a whole line to: these sheets' main commands (native only).
pub const NATIVE_SHEET_NAMES: [&str; 26] = [
    "electrical engineering",
    "chemistry",
    "biology",
    "beams",
    "torsion",
    "fluids",
    "stress and strain",
    "tensor analysis",
    "constitutive relations",
    "continuum mechanics",
    "formulas",
    "fourier",
    "euler",
    "complex numbers",
    "ct transform table",
    "dt transform table",
    "laplace",
    "z transform",
    "unit circle",
    "geometry",
    "statistics",
    "linear algebra",
    "circuits",
    "taylor series",
    "vector calculus",
    "cheat sheets",
];

/// Where a bar line opens on ↵: a sheet, the index of the section to show first, and what the bar says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdentityTarget {
    pub sheet: IdentitySheet,
    pub section: usize,
    pub hint: String,
}

fn at(sheet_id: &str, section_title: Option<&str>) -> IdentityTarget {
    let sheet = sheet(sheet_id);
    let section = section_title
        .and_then(|t| sheet.sections.iter().position(|s| s.title == t))
        .unwrap_or(0);
    let hint = if section > 0 {
        format!(
            "{}: {}",
            sheet.hint,
            sheet.sections[section].title.to_lowercase()
        )
    } else {
        sheet.hint.clone()
    };
    IdentityTarget {
        sheet: sheet.clone(),
        section,
        hint,
    }
}

/// The sheet and section a bar line opens, or None.
pub fn identity_target_for(text: &str) -> Option<IdentityTarget> {
    static SPACES: Lazy = Lazy::new(|| js(r"\s+", ""));
    static PART_COMMAND: Lazy = Lazy::new(|| js(r"^(?:identities|identity|idt)\s+(.+)$", "i"));
    static TRIG: Lazy = Lazy::new(|| js(r"^trig$", "i"));
    static PARTIAL: Lazy = Lazy::new(|| js(r"^partial fractions$", "i"));
    static IDT: Lazy = Lazy::new(|| js(r"^idt$", "i"));
    static IDENTITY_WORDS: Lazy = Lazy::new(|| {
        js(
            r"^(?:(?:trig(?:onometric|onometry)?|log(?:arithm(?:ic)?)?|exponent|algebra(?:ic)?|math)\s+)?identit(?:y|ies)$",
            "i",
        )
    });
    static CONVERSION_WORDS: Lazy = Lazy::new(|| {
        js(
            r"^(?:(?:(?:unit|metric)\s+)?conversions?(?:\s+(?:table|sheet|factors))?|(?:(?:metric|si)\s+)?prefix(?:es)?(?:\s+table)?|metric(?:\s+(?:table|system))?|units?\s+(?:table|sheet))$",
            "i",
        )
    });
    static CALCULUS_WORDS: Lazy = Lazy::new(|| {
        js(
            r"^(?:(?:common\s+)?(?:derivatives?|integrals?)|calculus(?:\s+(?:identities|sheet|rules))?|derivative\s+rules)$",
            "i",
        )
    });
    static CONSTANTS_WORDS: Lazy = Lazy::new(|| {
        js(
            r"^(?:(?:physical|physics|fundamental)\s+)?constants(?:\s+(?:table|sheet))?$|^codata$",
            "i",
        )
    });
    static PART_RES: crate::jsre::Lazy<Vec<jsre::Regex>> =
        crate::jsre::Lazy::new(|| PARTS.iter().map(|p| js(p.names, "")).collect());
    let t = jsre::replace_all(&SPACES, jstr::trim(text), |_| " ".into()).into_owned();
    let part = PART_COMMAND
        .captures(t.as_str())
        .ok()
        .flatten()
        .map(|c| jsre::group(&c, 1).to_lowercase());
    let named = match &part {
        Some(p) => PARTS
            .iter()
            .zip(PART_RES.iter())
            .find(|(_, re)| jsre::test(re, p))
            .map(|(part, _)| part),
        None if jsre::test(&TRIG, &t) => Some(&PARTS[0]),
        None => None,
    };
    if let Some(p) = named {
        return Some(at(p.sheet, Some(p.section)));
    }
    if jsre::test(&PARTIAL, &t) {
        return Some(at("identities", Some("Partial fractions")));
    }
    if jsre::test(&IDENTITY_WORDS, &t) || jsre::test(&IDT, &t) {
        return Some(at("identities", None));
    }
    if jsre::test(&CALCULUS_WORDS, &t) {
        return Some(at("calculus", None));
    }
    if jsre::test(&CONVERSION_WORDS, &t) {
        return Some(at("conversions", None));
    }
    if jsre::test(&CONSTANTS_WORDS, &t) {
        return Some(at("constants", None));
    }
    let line = part.as_deref().unwrap_or(&t);
    native_sheet_for(line).map(|id| at(id, None)).or_else(|| {
        // the web's sheets keep what they answered, all of which the golden suite holds them to
        part_for(line)
            .filter(|id| {
                !["identities:", "calculus:", "conversions:", "constants:"]
                    .iter()
                    .any(|w| id.starts_with(w))
            })
            .map(|id| at(&id, None))
    })
}

/// The sheet a bar line opens on ↵, or None.
pub fn identity_sheet_for(text: &str) -> Option<IdentitySheet> {
    identity_target_for(text).map(|t| t.sheet)
}

pub fn is_identity_command(text: &str) -> bool {
    identity_target_for(text).is_some()
}

/// How a run of text sits: raised, lowered, or lowered inside raised (as in `b^{log_{b}x}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shift {
    Sup,
    Sub,
    Supsub,
}

/// A run of text, raised or lowered or neither.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shift: Option<Shift>,
}

/// Splits a `show` line into plain, raised and lowered runs; braces nest one level, for `b^{log_{b}x}`.
pub fn identity_runs(show: &str) -> Vec<Run> {
    fn push(runs: &mut Vec<Run>, c: char, shift: Option<Shift>) {
        match runs.last_mut() {
            Some(last) if last.shift == shift => last.text.push(c),
            _ => runs.push(Run {
                text: c.to_string(),
                shift,
            }),
        }
    }
    fn walk(chars: &[char], i: &mut usize, runs: &mut Vec<Run>, shift: Option<Shift>, close: bool) {
        while *i < chars.len() {
            let c = chars[*i];
            if close && c == '}' {
                *i += 1;
                return;
            }
            if (c == '^' || c == '_') && chars.get(*i + 1) == Some(&'{') {
                *i += 2;
                let inner = if c == '^' { Shift::Sup } else { Shift::Sub };
                // deeper than one level keeps the outer shift: it would be too small to read
                let next = match shift {
                    None => inner,
                    Some(Shift::Sup) if inner == Shift::Sub => Shift::Supsub,
                    Some(s) => s,
                };
                walk(chars, i, runs, Some(next), true);
                continue;
            }
            push(runs, c, shift);
            *i += 1;
        }
    }
    let chars: Vec<char> = show.chars().collect();
    let mut runs = Vec::new();
    let mut i = 0;
    walk(&chars, &mut i, &mut runs, None, false);
    runs
}

/// The plain text a screen reader gets for a line.
pub fn identity_label(show: &str, lead: &[String]) -> String {
    let plain = |t: &str| {
        identity_runs(t)
            .into_iter()
            .map(|r| r.text)
            .collect::<String>()
    };
    lead.iter()
        .map(String::as_str)
        .chain(std::iter::once(show))
        .map(plain)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A sheet as the Mac window draws it: each line already split into runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSheet {
    pub id: String,
    pub title: String,
    pub section: usize,
    pub sections: Vec<NativeSection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSection {
    pub title: String,
    pub wide: bool,
    pub groups: Vec<NativeGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeGroup {
    pub title: String,
    pub items: Vec<NativeItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeItem {
    pub runs: Vec<Run>,
    pub copy: String,
    pub note: String,
    pub lead: Vec<Vec<Run>>,
}

/// What `openNativeIdentitySheet` posts to the Mac app, which draws the sheet in its own window.
pub fn native_identity_sheet(target: &IdentityTarget) -> NativeSheet {
    NativeSheet {
        id: target.sheet.id.clone(),
        title: target.sheet.title.clone(),
        section: target.section,
        sections: target
            .sheet
            .sections
            .iter()
            .map(|s| NativeSection {
                title: s.title.clone(),
                wide: s.wide == Some(true),
                groups: s
                    .groups
                    .iter()
                    .map(|g| NativeGroup {
                        title: g.title.clone(),
                        items: g
                            .items
                            .iter()
                            .map(|item| NativeItem {
                                runs: identity_runs(&item.show),
                                copy: item.copy.clone(),
                                note: item.note.clone().unwrap_or_default(),
                                lead: item
                                    .lead
                                    .iter()
                                    .flatten()
                                    .map(|l| identity_runs(l))
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheets_load_and_open() {
        assert_eq!(
            sheets().iter().filter(|s| !s.id.contains(": ")).count(),
            4 + NATIVE_COMMANDS.len()
        );
        let t = identity_target_for("Identity  Summation").map(|t| t.hint);
        assert_eq!(t.as_deref(), Some("identity sheet: summation"));
        assert_eq!(identity_label("b^{log_{b}x} = x", &[]), "blogbx = x");
    }

    fn opens(line: &str) -> Option<String> {
        identity_target_for(line).map(|t| t.sheet.id)
    }

    #[test]
    fn the_native_sheets_open_by_their_words() {
        for (line, id) in [
            ("fourier", "fourier"),
            ("Fourier series", "fourier"),
            ("Euler", "euler"),
            ("euler's formulas", "euler"),
            ("complex numbers", "complex numbers"),
            ("Complex Numbers", "complex numbers"),
            ("CT transform table", "ct transform table"),
            ("ct table", "ct transform table"),
            ("ctft", "ct transform table"),
            ("fourier transform", "ct transform table"),
            ("DT transform table", "dt transform table"),
            ("dtft", "dt transform table"),
            ("idt fourier", "fourier"),
            ("laplace transform", "laplace"),
            ("z-transform", "z transform"),
            ("unit circle", "unit circle"),
            ("stats", "statistics"),
            ("continuum mechanics", "continuum mechanics"),
            ("cheat sheet", "formulas"),
            ("electrical engineering", "circuits"),
            ("electrical", "circuits"),
            ("formula", "formulas"),
            ("Formulas", "formulas"),
        ] {
            assert_eq!(opens(line).as_deref(), Some(id), "{line}");
        }
        // what they don't take from: the web's words, and maths with these words in it
        assert_eq!(opens("physics constants").as_deref(), Some("constants"));
        assert_eq!(opens("idt series").as_deref(), Some("identities"));
        for line in [
            "euler + 1",
            "dt",
            "ct",
            "z",
            "complex(1,2)",
            "dt = 5",
            "area",
        ] {
            assert_eq!(opens(line), None, "{line}");
        }
    }

    #[test]
    fn the_unit_circle_labels_copy_their_values() {
        let sheet = identity_sheet_by_id("unit circle").unwrap();
        let figure = sheet.sections[0].groups[0].items[0].clone();
        let spots = figure.spots.unwrap();
        // 16 angles, each with its degrees, radians and two coordinates to click
        assert_eq!(spots.iter().filter(|s| !s.copy.is_empty()).count(), 64);
        let copies = |c: &str| spots.iter().any(|s| s.copy == c);
        for value in ["30°", "π/6", "√3/2", "-√2/2", "11π/6", "-1", "0"] {
            assert!(copies(value), "{value}");
        }
        // never both coordinates at once
        assert!(spots.iter().all(|s| !s.copy.contains(',')));
    }

    #[test]
    fn a_function_keeps_the_brackets_round_its_fraction() {
        let rows = |id: &str| -> Vec<Identity> {
            identity_sheet_by_id(id)
                .unwrap()
                .sections
                .iter()
                .flat_map(|s| &s.groups)
                .flat_map(|g| g.items.clone())
                .collect()
        };
        let chem = rows("chemistry");
        let buffer = chem.iter().find(|i| i.show.starts_with("pH = pK")).unwrap();
        assert!(
            buffer.typst.as_ref().unwrap()[0].contains("log_(10)(("),
            "{:?}",
            buffer.typst
        );
        let bio = rows("biology");
        let nernst = bio.iter().find(|i| i.show.contains("61.5")).unwrap();
        assert!(
            nernst.typst.as_ref().unwrap()[0].contains("log_(10)(("),
            "{:?}",
            nernst.typst
        );
    }

    #[test]
    fn a_formula_opens_by_its_name() {
        for (line, show) in [
            ("Ohm's law", "V = I R"),
            ("ohms law", "V = I R"),
            ("OHM LAW", "V = I R"),
            ("newton's 2nd", "F = m a"),
            ("Newtons second law", "F = m a"),
            ("F = ma", "F = m a"),
            ("KVL", "Σ V = 0"),
            ("bernoulli", "p + ρv^{2}/2 + ρgz = constant"),
            ("ideal gas law", "P V = n R T"),
            ("quadratic formula", "x = (−b ± √(b^{2} − 4ac))/(2a)"),
        ] {
            let sheet = native_target_for(line).unwrap().sheet;
            let first = &sheet.sections[0].groups[0].items[0];
            assert_eq!(first.show, show, "{line}");
        }
        // alone it's still a unit or a calculation
        for line in ["ohm", "newton", "5 ohm", "ohm + 1"] {
            assert!(native_target_for(line).is_none(), "{line}");
        }
    }

    #[test]
    fn a_conversion_copies_its_number_or_its_unit() {
        let sheet = native_identity_sheet_by_id("conversions").unwrap();
        let rows: Vec<&Identity> = sheet
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
            .collect();
        let parts = |show: &str| -> Vec<String> {
            rows.iter()
                .find(|i| i.show == show)
                .and_then(|i| i.parts.as_ref())
                .map(|p| p.iter().map(|p| p.copy.clone()).collect())
                .unwrap_or_default()
        };
        assert_eq!(parts("1 in = 2.54 cm"), ["1", "in", "", "2.54", "cm"]);
        assert_eq!(parts("1 Å = 10^{−10} m"), ["1", "Å", "", "1e-10", "m"]);
        assert_eq!(parts("1 rad = 57.2958°"), ["1", "rad", "", "57.2958", "°"]);
        assert_eq!(
            parts("1 au = 1.495979 × 10^{8} km"),
            ["1", "au", "", "1.495979e8", "km"]
        );
        // a formula isn't a number and a unit
        assert!(parts("K = °C + 273.15").is_empty());
        // the web's sheet stays as the web has it
        let plain = identity_sheet_by_id("conversions").unwrap();
        assert!(plain.sections[1].groups[0].items[0].parts.is_none());
    }

    #[test]
    fn a_part_opens_on_its_own() {
        let target = |line: &str| native_target_for(line).unwrap().sheet;
        // one section, nothing else
        for (line, title) in [
            ("Beams", "Beams"),
            ("beam theory", "Beams"),
            ("torsion", "Torsion"),
            ("stress and strain", "Stress and strain"),
            ("control volume", "Conservation laws on a control volume"),
            ("tensors", "Tensor analysis"),
            ("laplace pairs", "Pairs"),
            ("dt transform table properties", "Properties"),
            ("trig", "Trig"),
            ("op amps", "Op amps"),
            ("MOSFET", "Diodes and transistors"),
            ("bode", "RLC and filters"),
            ("phasors", "AC power and phasors"),
            (
                "transmission lines",
                "Electromagnetics and transmission lines",
            ),
            ("control systems", "Control systems"),
            ("thermodynamics", "Thermodynamics"),
            ("electrochemistry", "Electrochemistry"),
            ("enzymes", "Enzymes"),
            ("genetics", "Genetics"),
            ("gas laws", "Gas laws"),
            ("idt trig", "Trig"),
            ("integrals", "Integrals"),
            ("probability", "Probability"),
        ] {
            let sheet = target(line);
            assert_eq!(sheet.sections.len(), 1, "{line}");
            assert_eq!(sheet.sections[0].title, title, "{line}");
            // the Mac app finds it by its id
            assert_eq!(identity_sheet_by_id(&sheet.id).unwrap().title, sheet.title);
            // and it opens whichever way it's asked
            assert!(
                native_target_for(line)
                    .unwrap()
                    .hint
                    .contains(&title.to_lowercase())
            );
        }
        // just one group of a section
        let vessels = target("pressure vessels");
        assert_eq!(vessels.sections[0].groups.len(), 1);
        assert_eq!(vessels.title, "Pressure vessels (thin wall)");
        assert_eq!(
            target("kinematics").sections[0].groups[0].title,
            "Constant acceleration"
        );
        // the whole sheet still opens by its name
        assert_eq!(
            target("differential equations").id,
            "differential equations"
        );
        assert_eq!(target("statics").title, "Statics");
        assert!(target("continuum mechanics").sections.len() > 5);
        assert!(target("identities").sections.len() > 3);
        // words that mean nothing alone, and maths, stay what they were
        for line in ["pairs", "properties", "definition", "beams + 1", "log", "x"] {
            assert!(native_target_for(line).is_none(), "{line}");
        }
    }

    #[test]
    fn every_native_sheet_is_whole() {
        for (id, _) in NATIVE_COMMANDS {
            // the Mac app finds a sheet by typing its id
            assert_eq!(opens(id).as_deref(), Some(id));
            let sheet = identity_sheet_by_id(id).unwrap();
            assert!(!sheet.sections.is_empty(), "{id}");
            for item in sheet
                .sections
                .iter()
                .flat_map(|s| &s.groups)
                .flat_map(|g| &g.items)
            {
                assert_eq!(
                    item.show.matches('{').count(),
                    item.show.matches('}').count(),
                    "{}",
                    item.show
                );
                assert!(!identity_runs(&item.show).is_empty());
            }
        }
        // the sheet of sheets lists commands that work
        let index = identity_sheet_by_id("formulas").unwrap();
        for item in index
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
        {
            let line = item.open.as_deref().unwrap_or_default();
            assert!(line == "periodic" || opens(line).is_some(), "{line}");
        }
        for name in NATIVE_SHEET_NAMES {
            assert!(opens(name).is_some(), "{name}");
        }
    }

    #[test]
    fn a_click_copies_only_what_is_useful() {
        let rows = |id: &str| -> Vec<Identity> {
            identity_sheet_by_id(id)
                .unwrap()
                .sections
                .iter()
                .flat_map(|s| &s.groups)
                .flat_map(|g| g.items.clone())
                .collect()
        };
        // a label is never copied
        let fourier = rows("fourier");
        assert_eq!(fourier[0].lead.as_deref().unwrap(), ["CT"]);
        assert_eq!(fourier[0].copy, "a_k = (1/T) ∫_T x(t) e^(-jkω_o t) dt");
        // a table's sides copy themselves, without their conditions
        let ct = rows("ct transform table");
        let pair = ct.iter().find(|i| i.show == "1/(a + jω)").unwrap();
        assert_eq!(pair.copy, "1/(a + jω)");
        assert_eq!(pair.lead_copy.as_deref().unwrap(), ["e^(-at) u(t)"]);
        // nothing copies the two sides at once or an arrow
        for id in [
            "ct transform table",
            "dt transform table",
            "laplace",
            "z transform",
        ] {
            for row in rows(id) {
                assert!(!row.copy.contains('⟷'), "{}", row.copy);
            }
        }
        // the unit circle's cells copy their own value
        let circle = rows("unit circle");
        let thirty = circle
            .iter()
            .find(|i| i.lead.as_deref().is_some_and(|l| l[0] == "30°"))
            .unwrap();
        assert_eq!(
            thirty.lead_copy.as_deref().unwrap(),
            ["30°", "π/6", "1/2", "√3/2"]
        );
        // headings copy nothing
        assert!(circle[0].copy.is_empty());
    }

    #[test]
    fn the_formula_sheets_rows_are_all_there() {
        let shows = |id: &str| -> Vec<String> {
            identity_sheet_by_id(id)
                .unwrap()
                .sections
                .iter()
                .flat_map(|s| &s.groups)
                .flat_map(|g| &g.items)
                .map(|i| format!("{}|{}", i.lead.as_deref().unwrap_or(&[]).join("|"), i.show))
                .collect()
        };
        let ct = shows("ct transform table");
        for pair in [
            "e^{−at} u(t), a > 0|1/(a + jω)",
            "t e^{−at} u(t), a > 0|1/(a + jω)^{2}",
            "δ(t)|1",
            "e^{jω_{o}t}|2πδ(ω − ω_{o})",
            "cos(ω_{o}t)|πδ(ω − ω_{o}) + πδ(ω + ω_{o})",
            "sin(Wt)/(πt)|1 for |ω| < W, 0 else",
            "d^{n}x(t)/dt^{n}|(jω)^{n} X(jω)",
            "y(t) = x(t) ∗ h(t)|Y(jω) = X(jω) H(jω)",
            "y(t) = x(t) p(t)|Y(jω) = (1/2π) X(jω) ∗ P(jω)",
            "Periodic p(t) with F.S. {ω_{o}, a_{k}}|P(jω) = Σ_{k=−∞}^{∞} 2πa_{k} δ(ω − kω_{o})",
        ] {
            assert!(ct.iter().any(|r| r == pair), "{pair}");
        }
        let dt = shows("dt transform table");
        for pair in [
            "a^{n} u[n], |a| < 1|1/(1 − ae^{−jω})",
            "(n + 1) a^{n} u[n], |a| < 1|1/(1 − ae^{−jω})^{2}",
            "δ[n − n_{o}]|e^{−jωn_{o}}",
            "cos(ω_{o}n)|πδ(ω − ω_{o}) + πδ(ω + ω_{o})",
            "x[n − n_{o}]|e^{−jωn_{o}} X(e^{jω})",
            "y[n] = x[n] ∗ h[n]|Y(e^{jω}) = X(e^{jω}) H(e^{jω})",
        ] {
            assert!(dt.iter().any(|r| r == pair), "{pair}");
        }
    }
}
