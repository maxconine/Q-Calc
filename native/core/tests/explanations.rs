//! The reference sheets' explanations (`src/app/explanations/*.json`): every key is a row, every equation row has
//! one, each is whole, and each Try line answers in the bar.
//!
//! `EXPLAIN_FILE=mechanics cargo test -p qcalc-core --test explanations -- --nocapture show` prints that file's Try
//! lines beside the bar's answers and the answers the examples state, to check the numbers by eye.
use std::collections::{BTreeMap, BTreeSet};

use qcalc_core::app::identities::{EXPLANATION_FILES, Explanation, explain_key, sheets};
use qcalc_core::app::live::{LiveRequest, live};

fn file(text: &str) -> BTreeMap<String, Explanation> {
    serde_json::from_str(text).expect("an explanations file is a map of key to explanation")
}

/// Every equation row's key (not the conversions, constants or the formulas page, which only opens sheets).
fn row_keys() -> BTreeSet<String> {
    sheets()
        .iter()
        .filter(|s| {
            let sheet = s.id.split(": ").next().unwrap_or("");
            !matches!(sheet, "conversions" | "constants" | "formulas")
        })
        .flat_map(|s| s.sections.iter())
        .flat_map(|s| s.groups.iter())
        .flat_map(|g| g.items.iter())
        .filter(|i| !i.copy.is_empty() && i.open.is_none() && i.figure.is_none())
        .map(explain_key)
        .collect()
}

fn bar(line: &str) -> (String, bool) {
    let req = LiveRequest {
        text: line.into(),
        phrases: true,
        options: qcalc_core::app::settings::default_settings().eval_options(),
        ..Default::default()
    };
    let a = live(&req);
    (a.slot, a.message)
}

#[test]
fn every_file_reads_and_every_key_is_a_row() {
    let rows = row_keys();
    let mut strays = Vec::new();
    for (name, text) in EXPLANATION_FILES {
        for key in file(text).keys() {
            if !rows.contains(key) {
                strays.push(format!("{name}: {key}"));
            }
        }
    }
    assert!(
        strays.is_empty(),
        "keys that are no row:\n{}",
        strays.join("\n")
    );
}

#[test]
fn each_explanation_is_whole() {
    let mut thin = Vec::new();
    for (name, text) in EXPLANATION_FILES {
        for (key, e) in file(text) {
            let mut missing = Vec::new();
            if e.title.trim().is_empty() {
                missing.push("title");
            }
            if e.about.trim().is_empty() {
                missing.push("about");
            }
            match &e.example {
                Some(x) if !x.problem.trim().is_empty() && !x.answer.trim().is_empty() => {}
                _ => missing.push("example"),
            }
            if e.vars
                .iter()
                .any(|v| v[0].trim().is_empty() || v[1].trim().is_empty())
            {
                missing.push("a var's symbol or meaning");
            }
            if !missing.is_empty() {
                thin.push(format!("{name}: {key}: {}", missing.join(", ")));
            }
        }
    }
    assert!(
        thin.is_empty(),
        "explanations missing parts:\n{}",
        thin.join("\n")
    );
}

#[test]
fn every_try_line_answers_in_the_bar() {
    let mut bad = Vec::new();
    for (name, text) in EXPLANATION_FILES {
        for (key, e) in file(text) {
            let Some(line) = e.try_line else { continue };
            let (slot, message) = bar(&line);
            if slot.trim().is_empty() || message {
                bad.push(format!("{name}: {key}: `{line}` → {slot:?}"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "Try lines with no answer:\n{}",
        bad.join("\n")
    );
}

#[test]
fn every_equation_row_is_explained() {
    let all: BTreeSet<String> = EXPLANATION_FILES
        .iter()
        .flat_map(|(_, text)| file(text).into_keys())
        .collect();
    let missing: Vec<String> = row_keys()
        .into_iter()
        .filter(|k| !all.contains(k))
        .collect();
    assert!(
        missing.is_empty(),
        "{} rows have no explanation:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

/// The rows carry their explanations: the torsion formula's sheet has it on its row.
#[test]
fn a_row_carries_its_explanation() {
    let sheet =
        qcalc_core::app::identities::native_identity_sheet_by_id("formula: torsion formula")
            .unwrap();
    let row = &sheet.sections[0].groups[0].items[0];
    assert_eq!(row.show, "τ = Tρ/J");
    let e = row
        .explain
        .as_ref()
        .expect("the torsion formula is explained");
    assert!(e.related.iter().any(|r| r[0].contains("c^{4}")));
}

#[test]
fn show() {
    let Ok(want) = std::env::var("EXPLAIN_FILE") else {
        return;
    };
    for (name, text) in EXPLANATION_FILES {
        if name != want {
            continue;
        }
        for (key, e) in file(text) {
            let stated = e.example.map(|x| x.answer).unwrap_or_default();
            match e.try_line {
                Some(line) => println!(
                    "{key}\n    try: {line}\n    bar: {}\n    says: {stated}",
                    bar(&line).0
                ),
                None => println!("{key}\n    (no try)\n    says: {stated}"),
            }
        }
    }
}
