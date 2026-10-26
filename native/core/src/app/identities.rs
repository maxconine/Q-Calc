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

/// One line: `show` reads with `^{…}` raised and `_{…}` lowered; `copy` is what a click puts on the clipboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<Vec<String>>,
    pub show: String,
    pub copy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityGroup {
    pub title: String,
    pub items: Vec<Identity>,
}

/// A `wide` section's groups go one under another at the sheet's full width.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySection {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wide: Option<bool>,
    pub groups: Vec<IdentityGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySheet {
    /// `identities`, `calculus`, `conversions` or `constants`.
    pub id: String,
    pub title: String,
    pub hint: String,
    pub sections: Vec<IdentitySection>,
}

/// The widths, in points, of a row's `lead` cells.
pub const LEAD_WIDTHS: [u32; 2] = [64, 28];

/// All four sheets, in the TS order.
pub fn sheets() -> &'static [IdentitySheet] {
    static SHEETS: LazyLock<Vec<IdentitySheet>> =
        LazyLock::new(|| serde_json::from_str(include_str!("identities.json")).unwrap_or_default());
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

/// Where a bar line opens on ↵: a sheet, the index of the section to show first, and what the bar says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    static PART_RES: LazyLock<Vec<jsre::Regex>> =
        LazyLock::new(|| PARTS.iter().map(|p| js(p.names, "")).collect());
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
    None
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
        assert_eq!(sheets().len(), 4);
        let t = identity_target_for("Identity  Summation").map(|t| t.hint);
        assert_eq!(t.as_deref(), Some("identity sheet: summation"));
        assert_eq!(identity_label("b^{log_{b}x} = x", &[]), "blogbx = x");
    }
}
