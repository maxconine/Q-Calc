//! QA 69: the Mac frees its evaluation and app-logic threads' compiled regexes at a hide, as the Windows app does.
//! Answers are the same before a release, after it (compiled again on first use) and on the long-work pool, which
//! keeps the shared copies. One test: the owner slots are per process.

use qcalc_ffi::{Engine, EngineConfig, EvalOptions};

fn shown(engine: &Engine, lines: &[&str]) -> Vec<String> {
    engine
        .evaluate(
            lines.iter().map(|s| s.to_string()).collect(),
            EvalOptions::default(),
        )
        .into_iter()
        .map(|a| a.display)
        .collect()
}

#[test]
fn regexes_free_and_come_back() {
    let engine = Engine::new(EngineConfig {
        is_windows: false,
        locale: "en_US".into(),
        data_dir: None,
        time_zone: None,
    });
    let lines = [
        "2+2",
        "sin(90)",
        "72 f to c",
        "x^2 = 9",
        "5 km in miles",
        "d/dx x^3",
    ];
    let before = shown(&engine, &lines);
    assert!(engine.own_compiled());
    let owned = shown(&engine, &lines);
    assert_eq!(before, owned);
    let freed = engine.release_compiled();
    assert!(freed > 50, "freed only {freed}");
    // nothing compiled since, so nothing to free
    assert_eq!(engine.release_compiled(), 0);
    assert_eq!(shown(&engine, &lines), before);
    assert!(engine.release_compiled() > 50);
    assert_eq!(shown(&engine, &lines), before);
}
