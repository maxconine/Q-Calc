//! QA item 66: the bar's hover label names tonight's new words — `sinc`, `cis`, `deg()`/`rad()`, `diff`, the
//! matrix functions, `mol`, `||` — and a clock phrase's zones (`3pm est in pst` had `pm` as picometer, `pst` as `pst`).
use qcalc_core::app::word_names::word_names;
use qcalc_core::evaluate::evaluate_sheet;
use qcalc_core::scientific::NAMES;
use qcalc_core::types::EvaluateOptions;

fn names(text: &str) -> Vec<(String, String)> {
    let units: Vec<u16> = text.encode_utf16().collect();
    word_names(text, &[], &[], true)
        .into_iter()
        .map(|u| (String::from_utf16_lossy(&units[u.start..u.end]), u.name))
        .collect()
}

fn named(text: &str, word: &str) -> Option<String> {
    names(text)
        .into_iter()
        .find(|(w, _)| w == word)
        .map(|(_, n)| n)
}

fn answers(text: &str) -> bool {
    !evaluate_sheet(&[text.to_string()], &EvaluateOptions::default())
        .pop()
        .unwrap()
        .display
        .is_empty()
}

#[test]
fn every_engine_function_has_a_name() {
    for n in NAMES.split('|') {
        if ["pi", "tau", "inf", "infinity", "ans"].contains(&n) {
            continue;
        }
        let line = format!("{n}(1)");
        assert!(named(&line, n).is_some(), "{n} has no hover name");
    }
}

#[test]
fn called_words_the_engine_reads() {
    for (line, word, name) in [
        ("sinc(1)", "sinc", "sinc (sin x / x)"),
        ("cis(30)", "cis", "cis (cos θ + i sin θ)"),
        ("deg(pi)", "deg", "radians to degrees"),
        ("rad(180)", "rad", "degrees to radians"),
        ("diff(x^3, x)", "diff", "derivative"),
        ("rect(2, 30)", "rect", "rectangular form"),
        ("polar(3+4i)", "polar", "polar form"),
        ("angle(3+4i)", "angle", "angle"),
        ("det([1 2; 3 4])", "det", "determinant"),
        ("inv([1 2; 3 4])", "inv", "inverse"),
        ("rref([1 2; 3 4])", "rref", "reduced row echelon form"),
        ("cross([1,2,3],[4,5,6])", "cross", "cross product"),
        ("lu([4 3; 6 3])", "lu", "LU decomposition"),
        ("size([1 2 3; 4 5 6])", "size", "size (rows, columns)"),
        ("numel([1 2; 3 4])", "numel", "number of entries"),
    ] {
        assert_eq!(named(line, word).as_deref(), Some(name), "{line}");
        // only words the engine really reads get a name
        assert!(answers(line), "{line} doesn't answer");
    }
    // a word only named as a call: `5 rad` stays the unit, `size` alone is nothing
    assert_ne!(named("5 rad", "rad").as_deref(), Some("degrees to radians"));
    assert!(named("size", "size").is_none());
    // a function of the user's own keeps its name to itself
    assert!(word_names("det(2)", &[], &["det".into()], false).is_empty());
}

#[test]
fn moles_and_parallel() {
    assert_eq!(
        named("8.314 J/(mol K) * 300 K", "mol").as_deref(),
        Some("mole")
    );
    assert_eq!(
        named("2 kmol * 18 g/mol", "kmol").as_deref(),
        Some("kilomole")
    );
    assert_eq!(named("2 mol NaCl in g", "mol").as_deref(), Some("mole"));
    assert!(named("molar", "mol").is_none());
    assert_eq!(named("10 Ω || 20 Ω", "||").as_deref(), Some("in parallel"));
    assert_eq!(named("10 || 20", "||").as_deref(), Some("in parallel"));
    // a norm's bars aren't
    assert!(named("||[3, 4]||", "||").is_none());
    assert!(named("||a|| + ||b||", "||").is_none());
}

#[test]
fn clock_phrases() {
    assert_eq!(
        names("3pm est in pst"),
        vec![
            ("est".into(), "Eastern time".into()),
            ("pst".into(), "Pacific time".into())
        ]
    );
    assert_eq!(
        named("time in utc", "utc").as_deref(),
        Some("Coordinated Universal Time")
    );
    assert!(named("3pm in tokyo", "pm").is_none());
    assert!(named("3:30 pm tomorrow", "pm").is_none());
    // the bar answers these as clock times too (3:00 pm)
    assert!(named("3pm", "pm").is_none());
    assert!(named("5 pm", "pm").is_none());
    // pm beside another unit is picometers
    assert_eq!(named("5 pm to nm", "pm").as_deref(), Some("picometer"));
    assert_eq!(named("5 PM", "PM").as_deref().is_some(), true);
    assert_eq!(named("now + 2 hours", "hours").as_deref(), Some("hour"));
}
