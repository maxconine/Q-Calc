//! QA item 57: a line answers as its grey parens show it. The plain path filled them itself, but a matrix call,
//! `solve(`, `d/dx (`, `∫(` or bare Typst `frac(` stayed blank until the `)` was typed, though Tab/→ (which types
//! the grey ones in) made them answer; a `$…$` Typst line got its `)` after the closing `$` (blank) when copied or
//! filled.
use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::live::{LiveRequest, commit, live};
use qcalc_core::app::touches::live_line_copy;
use qcalc_core::parens::fill_typed;

fn shown(t: &str) -> String {
    let req = LiveRequest {
        text: t.into(),
        phrases: true,
        ..Default::default()
    };
    live(&req).display
}

#[test]
fn unclosed_lines_answer_as_the_grey_parens_read() {
    for t in [
        "det([1 2; 3 4]",
        "inv([1 2; 3 4]",
        "solve(x^2 = 9",
        "d/dx (x^3",
        "∫(x^2, 0, 1",
        "frac(1, 2",
        "sin(cos(2",
        "(5 m + 3 m",
        "5 m)/2 s",
        "$sqrt(2$",
        "$frac(1, 2$",
        "sqrt(sqrt(16",
        "1/(2 pi 60 Hz",
    ] {
        let typed = shown(t);
        assert!(!typed.is_empty(), "{t} blank");
        assert_eq!(
            typed,
            shown(&fill_typed(t)),
            "{t} reads otherwise once filled"
        );
    }
    // ↵ saves the answer too, with the line as typed
    let req = LiveRequest {
        text: "det([1 2; 3 4]".into(),
        phrases: true,
        ..Default::default()
    };
    assert_eq!(
        commit(&req, &[], "r")
            .row
            .and_then(|r| r["display"].as_str().map(str::to_string))
            .unwrap_or_default(),
        "-2"
    );
    // still blank when filling doesn't help, and a typed error is kept
    assert_eq!(shown("(("), "");
    assert_eq!(shown("[1, 2; 3, (4]"), shown("[1, 2; 3, (4])"));
}

#[test]
fn a_typst_line_takes_its_parens_inside_the_dollars() {
    assert_eq!(fill_typed("$sqrt(2$"), "$sqrt(2)$");
    assert_eq!(fill_typed("$10 + (2"), "$10 + (2)");
    assert_eq!(
        live_line_copy(
            "$sqrt(2$",
            false,
            None,
            "1.41421",
            None,
            None,
            AnswerForm::Exact
        ),
        "$sqrt(2)$ = 1.41421"
    );
    assert_eq!(
        live_line_copy("(1+2", false, None, "3", None, None, AnswerForm::Exact),
        "(1+2) = 3"
    );
}
