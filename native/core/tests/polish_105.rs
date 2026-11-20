//! QA item 105: the bar colours a clock line's zones alike (`3pm est in pst` coloured `pst` only: the unit table
//! knows a `pst` but not `est`) and its am/pm isn't a unit (picometers); other lines colour as `unitSpans` does.
use qcalc_core::app::unit_spans::unit_spans;
use qcalc_core::app::word_names::unit_colours;

fn coloured(text: &str) -> Vec<String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    unit_colours(text, &[], &[], false)
        .iter()
        .map(|u| String::from_utf16_lossy(&units[u.start..u.end]))
        .collect()
}

#[test]
fn clock_zones_colour_alike() {
    assert_eq!(coloured("3pm est in pst"), ["est", "pst"]);
    assert_eq!(coloured("3pm EST in PST"), ["EST", "PST"]);
    assert_eq!(coloured("3:30 pm est to pst"), ["est", "pst"]);
    assert_eq!(coloured("9am cst in utc"), ["cst", "utc"]);
    assert_eq!(coloured("3pm in utc"), ["utc"]);
}

#[test]
fn other_lines_colour_as_unit_spans() {
    // another unit about: the pm is picometers
    assert_eq!(coloured("5 pm to nm"), ["pm", "nm"]);
    for t in [
        "10 km to mi",
        "5 m/s * 3 s",
        "20 C to F",
        "12 in to cm",
        "est",
        "x = 3",
    ] {
        assert_eq!(
            unit_colours(t, &[], &[], false),
            unit_spans(t, &[], &[]),
            "{t}"
        );
    }
}
