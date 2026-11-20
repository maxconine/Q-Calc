//! First run, the walkthrough and the `?` sheet (QA item 26): every example and step shown with an answer has one,
//! and the walkthrough's basics finish only once a copy marks its copy step (the Mac app never marked it, so the
//! tour stayed on `⌘C copies the answer`; `TourRules`/`tourAfterCopy` in the Mac app now do).
use qcalc_core::app::onboarding as ob;
use qcalc_core::app::tour::{self as tr, TourFacts, TourId};
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

/// what the bar's example slot shows (the Mac app types `+/-` as `±` before evaluating)
fn answer(t: &str) -> String {
    let typed = t.replace("+/-", "±");
    ob::example_answer(Some(&evaluate_line(&typed, &EvaluateOptions::default())))
}

#[test]
fn every_example_with_an_answer_has_one() {
    for e in ob::example_list(Some("⌃⌥Space")) {
        if !e.plain {
            assert!(!answer(&e.expr).is_empty(), "{} has no answer", e.expr);
        }
    }
    assert_eq!(answer("x^2 = 2"), "x = ±sqrt(2) ≈ ±1.41421356237");
    assert_eq!(answer("(5.0 +/- 0.2) * 3"), "15.0 ± 0.6");
}

#[test]
fn every_step_and_extra_with_an_answer_has_one() {
    for s in tr::tour_steps("⌃⌥Space")
        .into_iter()
        .chain(tr::tour_extras())
    {
        if !s.plain {
            assert!(!answer(&s.expr).is_empty(), "{} has no answer", s.expr);
        }
    }
}

#[test]
fn the_basics_need_the_copy_step() {
    let hotkey = "⌃⌥Space";
    // second open, then the saves the steps ask for
    let mut done = tr::TOUR_ON | tr::tour_bit(TourId::Hotkey);
    for (expr, unit, chained) in [
        ("12 * 7", false, false),
        ("5 ft to cm", true, false),
        ("* 2", false, true),
        ("2^10", false, false),
    ] {
        done |= tr::tour_used(&TourFacts {
            expr: expr.into(),
            unit,
            chained,
            ..Default::default()
        });
    }
    assert_eq!(
        tr::tour_step(done, hotkey).map(|s| s.id),
        Some(TourId::Copy),
        "saves alone leave the copy step"
    );
    assert!(tr::tour_extra(done, hotkey).is_none());
    done |= tr::tour_bit(TourId::Copy) | 16;
    assert_eq!(tr::tour_step(done, hotkey), None);
    assert_eq!(
        tr::tour_extra(done, hotkey).map(|s| s.id),
        Some(TourId::Solve)
    );
    // with no shortcut working there's no hotkey step to wait on
    assert_eq!(tr::tour_progress(0, "").of, 4);
    // `?` and `periodic` marked when used (the Mac app now does) aren't offered as extras after
    let all = done
        | tr::tour_bit(TourId::Solve)
        | tr::tour_bit(TourId::Graph)
        | tr::tour_bit(TourId::Periodic)
        | tr::tour_bit(TourId::Help);
    assert!(tr::tour_extra(all, hotkey).is_none());
}

#[test]
fn cheat_rows_that_calculate_do() {
    for (t, _) in ob::native_type_cheats() {
        let calc = [
            "ft to cm",
            "80 + 15%",
            "x^2 = 2",
            "sum n, n=1..9",
            "d/dx x^2",
            "∫ x e^x dx",
            "nCr(5,2)",
            "sin(90 deg)",
            "(3+j4)/(1-j)",
            "3+j4 in polar",
            "(3+j4) kΩ",
            "H2O",
        ];
        if calc.contains(&t.as_str()) {
            assert!(!answer(&t).is_empty(), "{t} has no answer");
        }
    }
}
