//! QA item 54: history files from every shape that was ever written load without loss, and each row kind
//! (notes included) survives load → save → load and a commit after it. Fixtures are built from the TS
//! `HistoryRow` shape and the native builds' git history, never from a real ~/Library file.
use qcalc_ffi::{
    Engine, EngineConfig, EvalContext, HistoryKind, HistoryRow, LiveRequest, SolveOutcome,
    default_eval_options,
};
use std::path::PathBuf;

fn engine(dir: &PathBuf) -> std::sync::Arc<Engine> {
    Engine::new(EngineConfig {
        is_windows: false,
        locale: "en_US".into(),
        data_dir: Some(dir.to_string_lossy().into_owned()),
        time_zone: None,
    })
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "qcalc-history-migration-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn request(text: &str) -> LiveRequest {
    LiveRequest {
        text: text.into(),
        options: default_eval_options(),
        context: EvalContext::default(),
        chained: false,
        now_ms: 1_790_000_000_000,
        phrases: false,
    }
}

/// Every kind the tape has held, in the file's envelope (native builds since 220c81d).
const EVERY_KIND: &str = r#"{"version":1,"rows":[
 {"id":"a1","expr":"2^10","display":"1024","n":1024,"at":1790000000000},
 {"id":"a2","expr":"x = 3.0 ± 0.1","display":"3.0 ± 0.1","n":3,"meas":{"sig":2,"dp":1,"unc":0.1,"uncDigits":"1"},"at":1790000000001},
 {"id":"a3","expr":"v = 5 m/s","display":"5 m/s","n":5,"quantity":"5 m/s","at":1790000000002},
 {"id":"a4","expr":"A = [1, 2; 3, 4]","display":"[1 2; 3 4]","quantity":"[1, 2; 3, 4]","at":1790000000003},
 {"id":"a5","expr":"f(x) = x^2 + 1","display":"f(x) = x^2 + 1","kind":"function","fnName":"f","fnParams":["x"],"fnBody":"x^2 + 1","at":1790000000004},
 {"id":"a6","expr":"sys","display":"x = 1, y = 2","kind":"system","equations":["x + y = 3","x - y = -1"],"at":1790000000005},
 {"id":"a7","expr":"k := 4","display":"4","n":4,"kind":"definition","at":1790000000006},
 {"id":"a8","expr":"x^2 = -1","display":"x = ±i","solve":{"variable":"x","roots":[],"outcome":"complex"},"at":1790000000007},
 {"id":"a9","expr":"x^2 = 9","display":"x = ±3","solve":{"variable":"x","roots":[-3,3],"outcome":"roots","more":true},"at":1790000000008},
 {"id":"b1","expr":"call Sam at 3pm about the lab report","display":"","kind":"note","at":1790000000009}
]}"#;

#[test]
fn every_row_kind_round_trips_through_the_file() {
    let dir = temp_dir("kinds");
    std::fs::write(dir.join("history.json"), EVERY_KIND).unwrap();
    let e = engine(&dir);
    let rows = e.load_history();
    assert_eq!(rows.len(), 10, "{rows:#?}");
    let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
    assert_eq!(kinds[4], Some(HistoryKind::Function));
    assert_eq!(kinds[5], Some(HistoryKind::System));
    assert_eq!(kinds[6], Some(HistoryKind::Definition));
    // the note came back as a plain row with an empty answer
    assert_eq!(kinds[9], Some(HistoryKind::Note));
    assert_eq!(rows[1].meas.as_ref().and_then(|m| m.unc), Some(0.1));
    assert_eq!(rows[3].quantity.as_deref(), Some("[1, 2; 3, 4]"));
    assert_eq!(rows[5].equations.as_ref().map(Vec::len), Some(2));
    assert_eq!(rows[7].solve.as_ref().map(|s| s.outcome), Some(SolveOutcome::Complex));
    assert!(rows[8].solve.as_ref().is_some_and(|s| s.more));
    // saved and read again: nothing changes
    let saved = e.save_history(rows.clone()).unwrap();
    assert_eq!(saved, rows);
    assert_eq!(engine(&dir).load_history(), rows);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_note_keeps_its_kind_and_length_through_a_commit_and_a_relaunch() {
    let dir = temp_dir("note");
    let e = engine(&dir);
    let long = "meeting notes ".repeat(40); // 560 characters: over a line's 240, under a note's 1000
    let note = HistoryRow {
        id: "n1".into(),
        expr: long.trim().into(),
        display: String::new(),
        exact: None,
        n: None,
        meas: None,
        quantity: None,
        kind: Some(HistoryKind::Note),
        equations: None,
        fn_name: None,
        fn_params: None,
        fn_body: None,
        solve: None,
        at: Some(1_790_000_000_000),
    };
    // the next ↵ hands the tape (with the note) back through the engine
    let done = e.commit(request("1 + 1"), vec![note.clone()]);
    assert!(done.saved);
    assert_eq!(done.history[0], note, "the commit dropped the note's kind");
    e.save_history(done.history.clone()).unwrap();
    let back = engine(&dir).load_history();
    assert_eq!(back[0], note);
    assert_eq!(back[1].display, "2");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn older_shapes_load() {
    let e = Engine::new(EngineConfig {
        is_windows: false,
        locale: "en_US".into(),
        data_dir: None,
        time_zone: None,
    });
    // the web page's rows (a `latex` field, no id, no `at`), as a bare array (the first native builds)
    let web = r#"[{"latex":"\\frac{1}{2}","display":"0.5","n":0.5},{"expr":"3*4","display":"12","n":12}]"#;
    let rows = e.history_from_json(web.into());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].expr, "\\frac{1}{2}");
    assert!(!rows[0].id.is_empty() && rows[0].id != rows[1].id);
    assert_eq!(rows[1].n, Some(12.0));
    // a later envelope with fields this build doesn't know: rows still read, the extras dropped
    let newer = r#"{"version":2,"saved":"x","rows":[{"id":"z","expr":"2+2","display":"4","n":4,"pinned":true,"color":"red"}]}"#;
    let rows = e.history_from_json(newer.into());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].display, "4");
    // a solve outcome this build doesn't know drops the solve, not the row
    let odd = r#"[{"id":"s","expr":"x^2 = 2","display":"x = ±1.414","solve":{"variable":"x","roots":[1.4],"outcome":"later"}}]"#;
    let rows = e.history_from_json(odd.into());
    assert_eq!(rows.len(), 1);
    assert!(rows[0].solve.is_none());
}

#[test]
fn an_unreadable_file_is_kept_aside_before_it_is_overwritten() {
    let dir = temp_dir("unread");
    // one null row empties the whole history (the TS loadHistory's throw, kept for parity)
    let damaged = r#"{"version":1,"rows":[{"id":"a","expr":"2+2","display":"4"},null]}"#;
    std::fs::write(dir.join("history.json"), damaged).unwrap();
    let e = engine(&dir);
    assert!(e.load_history().is_empty());
    assert_eq!(std::fs::read_to_string(dir.join("history.unread.json")).unwrap(), damaged);
    e.save_history(Vec::new()).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("history.unread.json")).unwrap(), damaged);
    // an empty history (or none) keeps nothing aside
    let clean = temp_dir("empty");
    std::fs::write(clean.join("history.json"), r#"{"version":1,"rows":[]}"#).unwrap();
    assert!(engine(&clean).load_history().is_empty());
    assert!(!clean.join("history.unread.json").exists());
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(clean);
}
