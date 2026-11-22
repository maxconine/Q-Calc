//! QA item 32: a matrix function call with no 2D matrix typed (`sum([1, 2])`, `svals([1 2`, `mean([1, 2`)
//! compiled a regex of every matrix function name on each evaluation (~1.7 ms a keystroke, release); now built once.
use qcalc_core::evaluate_sheet;
use qcalc_core::types::EvaluateOptions;
use std::time::Instant;

#[test]
fn matrix_calls_without_a_grid_stay_quick() {
    let o = EvaluateOptions::default();
    for line in ["sum([1, 2", "svals([1 2", "mean([1, 2"] {
        evaluate_sheet(&[line.to_string()], &o);
        let t = Instant::now();
        for _ in 0..100 {
            std::hint::black_box(evaluate_sheet(&[line.to_string()], &o));
        }
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(
            ms < 400.0 * ci_slack() as f64,
            "{line}: 100 evaluations took {ms:.0} ms"
        );
    }
}

#[test]
fn list_statistics_still_answer() {
    let o = EvaluateOptions::default();
    let show = |l: &str| evaluate_sheet(&[l.to_string()], &o).pop().unwrap().display;
    assert_eq!(show("sum([1, 2, 3])"), "6");
    assert_eq!(show("mean([1, 2, 3])"), "2");
    assert_eq!(show("size([1 2; 3 4])"), "[2 2]");
}

// shared CI runners are several times slower than a dev Mac; time limits get 5x there (answers are still checked)
fn ci_slack() -> u128 {
    if std::env::var_os("CI").is_some() {
        5
    } else {
        1
    }
}
