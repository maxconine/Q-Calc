//! Answers the native engine gives where the TS one doesn't (left blank or "undefined" there).
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn euler_with_a_typed_pi() {
    for t in ["e^(iπ)+1", "e^(i π)+1", "exp(i pi)+1", "exp(iπ)+1"] {
        assert_eq!(show(t), "0", "{t}");
    }
    assert_eq!(show("e^(2iπ)"), "1");
    assert_eq!(show("2iπ"), "6.28318530718i");
    assert_eq!(show("τi"), "6.28318530718i");
    assert_eq!(show("e π"), show("e*pi"));
}

#[test]
fn exp_stays_real_for_reals() {
    assert_eq!(show("exp(2)"), "7.38905609893");
}

#[test]
fn and_is_bitwise_and() {
    // `and` reads as `&`
    for (t, want) in [
        ("12 and 10", "8"),
        ("12 and 3", "0"),
        ("1 and 1", "1"),
        ("1 and 0", "0"),
        ("6 AND 3", "2"),
    ] {
        assert_eq!(show(t), want, "{t}");
        assert_eq!(
            show(t),
            show(&t.to_lowercase().replace(" and ", " & ")),
            "{t}"
        );
    }
    assert_eq!(show("(2>1) and (3>4)"), "0");
    assert_eq!(show("(2>1) and (4>3)"), "1");
}

#[test]
fn logic_or() {
    // `or` reads as `|`: on 0 and 1 (or true/false comparisons) it's logical or, on whole numbers bitwise
    for (t, want) in [
        ("1 or 0", "1"),
        ("0 or 1", "1"),
        ("1 or 1", "1"),
        ("0 or 0", "0"),
        ("12 or 3", "15"),
        ("8 OR 1", "9"),
        ("1 or 0 or 0", "1"),
        ("0 or 0 or 0", "0"),
        ("(2>1) or (3>4)", "1"),
        ("(1>2) or (3>4)", "0"),
        ("2>1 or 3>4", "1"),
        ("1>2 or 3>4", "0"),
        ("(1 or 0) and 1", "1"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    assert_eq!(show("12 or 3"), show("12 | 3"));
}

#[test]
fn and_or_in_phrases_stay_words() {
    // the words in a phrase aren't operators: `between 1 and 2` stays blank (as on the page), not `1 & 2`
    assert_eq!(show("between 1 and 2"), "");
}

#[test]
fn and_or_are_wedge_and_vee_in_the_typst_preview() {
    use qcalc_core::app::typst_math::to_typst_math;
    // ∧ for & and `and`, ∨ for `or` (a bare & was Typst's alignment point and drew nothing)
    for (line, want) in [
        ("sin(22&3)", "sin(22 and 3)"),
        ("(22&3*21)22/23", "(22 and 3 dot 21) 22 / 23"),
        ("12 and 3", "12 and 3"),
        ("1 or 0", "1 or 0"),
        ("(2>1) or (3>4)", "(2 > 1) or (3 > 4)"),
    ] {
        assert_eq!(to_typst_math(line), want, "{line}");
    }
}

#[test]
fn every_typed_character_previews_and_every_preview_compiles() {
    use qcalc_core::app::typst_math::{preview_math, typst_preview_useful};
    // as in typstMath.test.ts; the renderer compiles them in qcalc-typst's preview_fuzz
    for (line, want) in [
        ("⌊x⌋", "floor(x)"),
        ("⌈2.5⌉", "ceil(2.5)"),
        ("‖v‖", "norm(v)"),
        ("⟨x, y⟩", "lr(chevron.l x , y chevron.r)"),
        ("⌊x", "floor.l x"),
        ("f″(x)", "f '' (x)"),
        ("€5 + £3", "€ 5 + £ 3"),
        ("¥ ¢ ‰ § ¶ —", "¥ ¢ ‰ § ¶ —"),
        ("@ ` \\ ‹ « “ ‘", "\\@ \\` backslash ‹ « “ ‘"),
        ("x^2 \"", "x ^ 2 \\\""),
        ("0b1010", "\"0b1010\""),
        ("0xff + 1/2", "\"0xff\" + 1 / 2"),
        ("0o17", "\"0o17\""),
        ("sin(2;3)", "sin(2 \\; 3)"),
        ("∛", "\\∛"),
        ("√", "\\√"),
        ("∜(16)", "root(4, 16)"),
        ("√()", "sqrt(zws)"),
        ("2^", "2 \\^"),
        ("/3", "\\/ 3"),
        ("⁻¹", "zws^(-1)"),
        ("_3", "zws_(3)"),
        ("abs()", "abs(zws)"),
        ("frac(1)", "op(\"frac\")(1)"),
        ("floor(1, 2)", "op(\"floor\")(1, 2)"),
        ("ip(x)", "op(\"ip\")(x)"),
        ("amat(1, 2; 3, 4)", "amat(1, 2; 3, 4)"),
        ("cbrt(sqrt", "root(3, op(\"sqrt\"))"),
        ("dx_1", "#dx _(1)"),
        ("√(1, 2)", "sqrt(lr(1 , 2))"),
        ("log(8, 2)", "log(8, 2)"),
    ] {
        assert_eq!(preview_math(line), want, "{line}");
    }
    for line in ["0b1010", "0xff", "0o17", "€5 + £3", "2^"] {
        assert!(!typst_preview_useful(line), "{line}");
    }
    for line in ["⌊x⌋", "‖v‖", "sin(2;3)", "0xff + 1/2", "x^2 \""] {
        assert!(typst_preview_useful(line), "{line}");
    }
}

#[test]
fn spaced_and_starred_factors_differentiate_as_powers() {
    // x written out six times, spaced or starred, is x^6: same answer, same form
    let want = show("d/dx 4x^3 + 12(x^6)");
    assert_eq!(want, "12x² + 72x⁵");
    for t in [
        "d/dx 4x^3 + 12(x*x x x x x)",
        "d/dx 4x^3 + 12(x x x x x x)",
        "d/dx 4x^3 + 12 x x x x x x",
        "d/dx 4x^3 + 12x*x*x*x*x*x",
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    assert_eq!(show("d/dx 4 x^3 + 12 x x"), show("d/dx 4x^3 + 12x^2"));
    assert_eq!(show("d/dx x x^2"), "3x²");
    assert_eq!(show("d/dx x*y x"), "2xy");
    assert_eq!(show("int 0..1 x*x x dx"), show("int 0..1 x^3 dx"));
}

#[test]
fn spaced_numbers_multiply_in_calculus() {
    // `2 3 x` is 6x in a derivative as on a plain line (it had come back blank)
    assert_eq!(show("2 3"), "6");
    for t in ["d/dx 2 3 x", "d/dx 2 3*x", "d/dx 2*3 x", "d/dx 6x"] {
        assert_eq!(show(t), "6", "{t}");
    }
    assert_eq!(show("d/dx 3.5 2 x"), "7");
    assert_eq!(show("int 0..1 2 3 x dx"), show("int 0..1 6x dx"));
}

#[test]
fn exponent_form_of_an_answer() {
    use qcalc_core::format::exponent_form;
    for (shown, want) in [
        ("3,000", "3 × 10³"),
        ("3000", "3 × 10³"),
        ("15,372", "1.5372 × 10⁴"),
        ("0.00052", "5.2 × 10⁻⁴"),
        ("-42", "−4.2 × 10¹"),
        ("−42", "−4.2 × 10¹"),
        ("7", "7 × 10⁰"),
        ("3,000 m", "3 × 10³ m"),
        ("1.5e12", "1.5 × 10¹²"),
        ("2.5 × 10⁻³", "2.5 × 10⁻³"),
        ("3 000", "3 × 10³"),
        // a prefixed unit gives its prefix to the power
        ("1 GB", "1 × 10⁹ B"),
        ("1.5 GB", "1.5 × 10⁹ B"),
        ("5 km", "5 × 10³ m"),
        ("2,000 km", "2 × 10⁶ m"),
        ("3 ms", "3 × 10⁻³ s"),
        ("4 MHz", "4 × 10⁶ Hz"),
        ("7 kg", "7 × 10³ g"),
        // units that only look prefixed, or have none, stay
        ("5 Pa", "5 × 10⁰ Pa"),
        ("3 min", "3 × 10⁰ min"),
        ("60 mph", "6 × 10¹ mph"),
        ("2 m", "2 × 10⁰ m"),
        ("1 GiB", "1 × 10⁰ GiB"),
        ("1 nmi", "1 × 10⁰ nmi"),
        ("1 cd", "1 × 10⁰ cd"),
        ("1 ft", "1 × 10⁰ ft"),
        ("1 kN", "1 × 10³ N"),
    ] {
        assert_eq!(exponent_form(shown, 6).as_deref(), Some(want), "{shown}");
    }
    assert_eq!(show("1 gigabyte"), "1 GB");
    assert_eq!(
        exponent_form(&show("1 gigabyte"), 6).as_deref(),
        Some("1 × 10⁹ B")
    );
    for shown in ["1/3", "7:55 pm", "", "abc", "2x"] {
        assert_eq!(exponent_form(shown, 6), None, "{shown}");
    }
}

#[test]
fn lcd_is_the_least_common_denominator() {
    assert_eq!(show("lcd(4,6)"), "12");
    assert_eq!(show("LCD(3, 4, 6)"), "12");
    assert_eq!(show("lcd(4,6)"), show("lcm(4,6)"));
    let names = qcalc_core::app::word_names::word_names("lcd(4,6) + gcf(12,18)", &[], &[], false);
    let said: Vec<&str> = names.iter().map(|w| w.name.as_str()).collect();
    assert!(said.contains(&"least common denominator"), "{said:?}");
    assert!(said.contains(&"greatest common factor"), "{said:?}");
}

#[test]
fn the_preview_shows_the_number_ans_stands_for() {
    use qcalc_core::app::chain::ans_written_out;
    use qcalc_core::app::typst_math::{preview_math, typst_preview_useful};
    // a quoted `"ans"` read as a word and hid the row
    assert!(!typst_preview_useful("ans * (10^5)/30"));
    for (line, plain, want) in [
        ("ans * (10^5)/30", "3333.33", "3333.33 dot (10 ^ 5) / 30"),
        ("ans * (10^5)/30", "-3", "(- 3) dot (10 ^ 5) / 30"),
        ("ans^2", "3 km", "(3 \"km\") ^ 2"),
        ("sqrt(ans)", "2", "sqrt(2)"),
    ] {
        let written = ans_written_out(line, plain);
        assert!(typst_preview_useful(&written), "{line}");
        assert_eq!(preview_math(&written), want, "{line}");
    }
    // plain arithmetic still reads as typed
    assert!(!typst_preview_useful(&ans_written_out("ans*2", "5")));
    assert!(!qcalc_core::format::format_number(3000.0, 10.0).contains(','));
}

#[test]
fn a_typed_angle_unit_works_in_either_mode() {
    use qcalc_core::app::angle_marks::angle_marks;
    use qcalc_core::evaluate::evaluate_line;
    use qcalc_core::types::EvaluateOptions;
    for mode in ["deg", "rad"] {
        let o: EvaluateOptions =
            serde_json::from_value(serde_json::json!({ "angleMode": mode })).unwrap();
        for (line, want) in [
            ("cos(π/4 rad)", "0.707106781187"),
            ("cos(π/4rad)", "0.707106781187"),
            ("cos(pi/4 rad)", "0.707106781187"),
            ("cos(π / 4 radians)", "0.707106781187"),
            ("cos(3π/4 rad)", "-0.707106781187"),
            ("tan(τ/8 rad)", "1"),
            ("sin(π/2 rad)+1", "2"),
            ("cos(π/2 rad - π/4 rad)", "0.707106781187"),
            ("cos(-π/4 rad)", "0.707106781187"),
            ("sin(180/2 deg)", "1"),
            ("sin(90 deg)", "1"),
            ("sin(90°)", "1"),
            ("sin(30 degrees)", "0.5"),
            ("cos(π rad)", "-1"),
            ("cos(sin(π/2 rad) * π/3 rad)", "0.5"),
        ] {
            assert_eq!(evaluate_line(line, &o).display, want, "{mode} {line}");
        }
    }
    // the faint mark only says the mode where no unit was typed
    for typed in [
        "cos(π/4 rad)",
        "cos(π/4rad)",
        "cos(π/4 radians)",
        "sin(90 deg)",
        "sin(90°)",
        "sin(30 degrees)",
    ] {
        assert_eq!(angle_marks(typed), Vec::<usize>::new(), "{typed}");
    }
    assert_eq!(angle_marks("cos(π/4)").len(), 1);
}

#[test]
fn j_is_the_imaginary_unit() {
    use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
    use qcalc_core::types::EvaluateOptions;
    let options = |mode: &str| -> EvaluateOptions {
        serde_json::from_value(serde_json::json!({ "angleMode": mode, "sigFigs": 6 })).unwrap()
    };
    for mode in ["deg", "rad"] {
        let o = options(mode);
        for (line, want) in [
            ("(795-j1060)/(20-j38)", "30.4664 + j4.88612"),
            ("(795-1060j)/(20-38j)", "30.4664 + j4.88612"),
            ("(795-1060i)/(20-38i)", "30.4664 + 4.88612i"),
            ("3j", "j3"),
            ("j^2", "-1"),
            ("j(3+j4)", "-4 + j3"),
            // an exponent is radians unless it says otherwise, in either mode
            ("e^(jπ/2)", "j"),
            ("5e^(j0.5)", "4.38791 + j2.39713"),
            ("5e^(i0.5)", "4.38791 + 2.39713i"),
            ("e^(j30 deg)", "0.866025 + j0.5"),
            ("10∠30°", "8.66025 + j5"),
            ("10∠30 rad", "1.54251 - j9.88032"),
            ("3+j4 in polar", "5∠53.1301°"),
            ("(3+j4) kΩ", "(3 + j4) kΩ"),
            ("(3+4i) Ω", "(3 + 4i) Ω"),
            ("(3+j4) kΩ in Ω", "(3000 + j4000) Ω"),
            ("12 V/(3+j4) kΩ", "(1.44 - j1.92) mA"),
            ("(3+j4) kΩ * 2 mA", "(6 + j8) V"),
            ("(3+j4)(3-j4) Ω", "25 Ω"),
            ("1/(j*2*pi*60 Hz*10 µF)", "-j265.258 Ω"),
            ("sin(j)", "j1.1752"),
            ("sin(i)", "1.1752i"),
            // i and j are the same unit, spelled the same ways
            ("(795-i1060)/(20-i38)", "30.4664 + 4.88612i"),
            ("i333Ω", "333i Ω"),
            ("12 V/(3+4i) kΩ", "(1.44 - 1.92i) mA"),
            ("(3+j4) Ω + 2 V", ""),
        ] {
            assert_eq!(evaluate_line(line, &o).display, want, "{mode} {line}");
        }
    }
    // `∠` and `arg` follow the mode
    assert_eq!(
        evaluate_line("10∠30", &options("deg")).display,
        "8.66025 + j5"
    );
    assert_eq!(
        evaluate_line("10∠30", &options("rad")).display,
        "1.54251 - j9.88032"
    );
    assert_eq!(
        evaluate_line("arg(3+j4)", &options("deg")).display,
        "53.1301 deg"
    );
    let o = options("deg");
    // joules keep the capital and the prefixes; your own j is yours
    assert_ne!(evaluate_line("5 kj", &o).display, "");
    // as do conversions of energy
    assert!(evaluate_line("1 kcal to j", &o).display.contains('J'));
    assert!(evaluate_line("3165 j to cal", &o).display.contains("cal"));
    assert!(!evaluate_line("2 J", &o).display.contains('j'));
    let sheet = |lines: &[&str]| -> Vec<String> {
        let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        evaluate_sheet(&lines, &o)
            .into_iter()
            .map(|r| r.display)
            .collect()
    };
    assert_eq!(sheet(&["j = 5", "j*2"]), ["5", "10"]);
    // an answer chains as itself
    assert_eq!(sheet(&["3+j4", "ans*2"]), ["3 + j4", "6 + j8"]);
    assert_eq!(
        sheet(&["z = (3+j4) Ω", "12 V/z"]),
        ["(3 + j4) Ω", "(1.44 - j1.92) A"]
    );
}

#[test]
fn a_complex_answer_has_polar_forms() {
    use qcalc_core::app::answer_forms::{FormSource, answer_forms};
    use qcalc_core::evaluate::evaluate_line;
    use qcalc_core::types::EvaluateOptions;
    let o: EvaluateOptions = serde_json::from_value(serde_json::json!({ "sigFigs": 6 })).unwrap();
    let line = evaluate_line("(3+j4) Ω", &o);
    let forms: Vec<String> = answer_forms(&FormSource {
        value: line.value.clone().unwrap(),
        display: line.display.clone(),
        exact: None,
        meas: None,
        sig_figs: 6.0,
        sig_fig_mode: None,
        rationalize: None,
    })
    .into_iter()
    .map(|f| f.display)
    .collect();
    assert_eq!(forms, ["5∠53.1301° Ω", "5e^(j0.927295) Ω"]);
}

#[test]
fn a_straight_line_lists_its_intercepts() {
    use qcalc_core::graph::{CriticalKind, GraphOptions, build_graph, build_native_graph};
    let at = |input: &str| {
        let o = GraphOptions {
            domain: Some((-10.0, 10.0)),
            ..GraphOptions::default()
        };
        build_native_graph(input, &o)
            .unwrap_or_else(|| panic!("not a graph: {input}"))
            .critical_points
            .iter()
            .map(|c| (c.x, c.y, c.kind))
            .collect::<Vec<_>>()
    };
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;

    let line = at("graph y=-5x+7.5");
    assert_eq!(line.len(), 2, "{line:?}");
    assert!(close(line[0].0, 1.5) && line[0].1 == 0.0 && line[0].2 == CriticalKind::XIntercept);
    assert!(line[1].0 == 0.0 && close(line[1].1, 7.5) && line[1].2 == CriticalKind::YIntercept);

    // flat: it never crosses the x axis
    let flat = at("graph y=3");
    assert_eq!(flat.len(), 1, "{flat:?}");
    assert_eq!(flat[0].2, CriticalKind::YIntercept);

    // the window and the bounds decide what's listed
    let far = GraphOptions {
        domain: Some((100.0, 200.0)),
        ..GraphOptions::default()
    };
    assert!(
        build_native_graph("graph 2x+1", &far)
            .unwrap()
            .critical_points
            .is_empty()
    );
    assert!(at("graph 2x+1 {5<x<8}").is_empty());

    // not lines: only their turns, as before
    for curve in [
        "graph x^2-4",
        "graph sin(x)",
        "graph abs(x)-1",
        "graph x^3-3x",
    ] {
        assert!(
            at(curve)
                .iter()
                .all(|c| !matches!(c.2, CriticalKind::XIntercept | CriticalKind::YIntercept)),
            "{curve}"
        );
    }

    // the web's result stays as it was
    let web = build_graph("graph y=-5x+7.5", &GraphOptions::default()).unwrap();
    assert!(web.critical_points.is_empty());
}

#[test]
fn a_line_can_define_its_letters() {
    use qcalc_core::types::AngleMode;
    let rad = EvaluateOptions {
        angle_mode: Some(AngleMode::Rad),
        ..EvaluateOptions::default()
    };
    let at = |t: &str| evaluate_line(t, &rad).display;
    assert_eq!(at("x^2; x=5"), "25");
    assert_eq!(at("x^2 + y; x=5; y=2"), "27");
    assert_eq!(at("x + y; x=5; y=x+1"), "11");
    assert_eq!(at("d/dx x^2; x=3"), "6");
    assert_eq!(evaluate_line("x^2; x=5", &rad).raw, "x^2; x=5");
    // the bar's route, through the sheet, answers the same
    let sheet = qcalc_core::evaluate_sheet(&["x^2; x=5".to_string()], &rad);
    assert_eq!(sheet[0].display, "25");
    // the preview gives the answer to the expression, not the last definition
    let doc =
        qcalc_core::app::typst_math::native_typst_document("x^2 + y; x=5; y=2", "27", "").unwrap();
    assert!(doc.contains("= 27 quad (x = 5, y = 2)"), "{doc}");
    // it stays on its own line
    assert_eq!(at("x^2"), "");
    // anything else with a semicolon goes on as before
    assert_eq!(at("1;2"), "");
    assert_eq!(at("x^2; 5"), "");
}

#[test]
fn an_integral_with_no_bounds_answers_with_c() {
    use qcalc_core::types::AngleMode;
    let rad = EvaluateOptions {
        angle_mode: Some(AngleMode::Rad),
        ..EvaluateOptions::default()
    };
    let at = |t: &str| evaluate_line(t, &rad).display;
    assert_eq!(at("∫ sec(x)^2 dx"), "tan(x) + C");
    assert_eq!(at("∫ x^2 dx"), "x^3/3 + C");
    assert_eq!(at("∫ sin(x) dx"), "-cos(x) + C");
    assert_eq!(at("∫ 1/x dx"), "ln|x| + C");
    assert_eq!(at("∫ e^x dx"), "e^x + C");
    assert_eq!(at("∫ 3x^2 + 2 dx"), "x^3 + 2x + C");
    assert_eq!(at("∫ x^2 dx; x=3"), "9 + C");
    assert!(
        at("∫ sec(x)^2 dx; x=5").starts_with("-3.3805"),
        "{}",
        at("∫ sec(x)^2 dx; x=5")
    );
    // what isn't in the table has no answer, not a wrong one
    assert_eq!(at("∫ e^(x^2) dx"), "");
    assert_eq!(at("∫ sin(x^2) dx"), "");
    // bounds still integrate numerically
    assert!(at("∫_0^1 x^2 dx").starts_with("0.3333"));
    // in degrees the trig comes with its factor, and still differentiates back
    let deg = EvaluateOptions {
        angle_mode: Some(AngleMode::Deg),
        ..EvaluateOptions::default()
    };
    assert_eq!(
        evaluate_line("∫ sec(x)^2 dx", &deg).display,
        "(180/π) tan(x) + C"
    );
}

#[test]
fn an_integral_in_words() {
    use qcalc_core::types::AngleMode;
    let rad = EvaluateOptions {
        angle_mode: Some(AngleMode::Rad),
        ..EvaluateOptions::default()
    };
    let at = |t: &str| evaluate_line(t, &rad).display;
    for (line, want) in [
        ("integral of x^2 dx", "x^3/3 + C"),
        ("integral of x^2", "x^3/3 + C"),
        ("the integral of x e^x dx", "x e^x - e^x + C"),
        ("Integral of sin(x)", "-cos(x) + C"),
        ("integrate x e^x dx", "x e^x - e^x + C"),
        ("integrate x e^x", "x e^x - e^x + C"),
        ("antiderivative of 1/x", "ln|x| + C"),
        ("integral of t^2 dt", "t^3/3 + C"),
        ("integral of t^2", "t^3/3 + C"),
        ("integrate t^2 with respect to t", "t^3/3 + C"),
        ("∫ x e^x", "x e^x - e^x + C"),
        ("integral x^2 dx", "x^3/3 + C"),
        // no variable to integrate in: nothing
        ("integral of 5", ""),
    ] {
        assert_eq!(at(line), want, "{line}");
    }
    // bounds in words are still the numeric integral
    assert!(
        at("integral of x^2 from 0 to 1").starts_with("0.3333"),
        "{}",
        at("integral of x^2 from 0 to 1")
    );
}

#[test]
fn an_integral_by_parts_or_substitution() {
    use qcalc_core::types::AngleMode;
    let rad = EvaluateOptions {
        angle_mode: Some(AngleMode::Rad),
        ..EvaluateOptions::default()
    };
    let at = |t: &str| evaluate_line(t, &rad).display;
    for (line, want) in [
        // by parts
        ("∫ x e^x dx", "x e^x - e^x + C"),
        ("∫ x sin(x) dx", "-x cos(x) + sin(x) + C"),
        ("∫ x^2 cos(x) dx", "x^2 sin(x) + 2x cos(x) - 2 sin(x) + C"),
        ("∫ ln(x) dx", "x ln(x) - x + C"),
        ("∫ x ln(x) dx", "x^2 ln(x)/2 - x^2/4 + C"),
        ("∫ e^x sin(x) dx", "e^x sin(x)/2 - e^x cos(x)/2 + C"),
        ("∫ atan(x) dx", "x atan(x) - ln(x^2 + 1)/2 + C"),
        (
            "∫ e^(2x) cos(3x) dx",
            "2 e^(2x) cos(3x)/13 + 3 e^(2x) sin(3x)/13 + C",
        ),
        // substitution
        ("∫ 2x cos(x^2) dx", "sin(x^2) + C"),
        ("∫ x e^(x^2) dx", "e^(x^2)/2 + C"),
        ("∫ sin(x)^3 cos(x) dx", "sin(x)^4/4 + C"),
        ("∫ x/(x^2+1) dx", "ln|x^2 + 1|/2 + C"),
        ("∫ cos(x)/sin(x) dx", "ln|sin(x)| + C"),
        ("∫ ln(x)/x dx", "ln(x)^2/2 + C"),
        ("∫ 1/(x ln(x)) dx", "ln|ln(x)| + C"),
        ("∫ 3x^2 sqrt(x^3+1) dx", "2 (x^3 + 1)^(3/2)/3 + C"),
        // what neither reaches stays blank
        ("∫ x tan(x) dx", ""),
        ("∫ e^(x^2) dx", ""),
        ("∫ sin(x)/x dx", ""),
        ("∫ x^2 cos(x^3) sin(x) dx", ""),
    ] {
        assert_eq!(at(line), want, "{line}");
    }
    // in degrees, trig inside a product stays blank rather than risk the factor
    let deg = EvaluateOptions {
        angle_mode: Some(AngleMode::Deg),
        ..EvaluateOptions::default()
    };
    assert_eq!(evaluate_line("∫ x sin(x) dx", &deg).display, "");
    assert_eq!(evaluate_line("∫ x e^x dx", &deg).display, "x e^x - e^x + C");
}

#[test]
fn feet_and_inches_marks() {
    assert_eq!(show("6'2\""), "6 ft 2 in");
    assert_eq!(show("6′2″"), "6 ft 2 in");
    assert_eq!(show("6'2"), "6 ft 2 in");
    assert_eq!(show("6'2\" + 1'11\""), "8 ft 1 in");
    assert_eq!(show("5 ft + 3 in"), "5 ft 3 in");
    assert_eq!(show("10''"), show("10\""));
    assert_eq!(show("6'2\" to in"), "74 in");
    assert_eq!(show("11.8379in + 14.6503in"), "26.4882 in");
    assert_eq!(
        evaluate_line("6′2″", &EvaluateOptions::default()).raw,
        "6′2″"
    );
    // ⌥↑ / ⌥↓: inches, feet and inches, feet
    let v = evaluate_line("6'2\"", &EvaluateOptions::default())
        .value
        .unwrap();
    let step = |d| {
        qcalc_core::feet_inches::step_feet_inches(&v, "6 ft 2 in", d, 12.0)
            .unwrap()
            .1
    };
    assert_eq!(step(-1), "74 in");
    assert_eq!(step(1), "6.16666666667 ft");
}

#[test]
fn differential_equations_have_general_solutions() {
    for (t, want) in [
        ("y' = 2x", "y = x^2 + C"),
        ("dy/dx = y", "y = C e^x"),
        ("y'' + y = 0", "y = C1 cos(x) + C2 sin(x)"),
        ("y' + 2y = e^x", "y = e^x/3 + C e^(-2x)"),
        ("y' = x y", "y = C e^(x^2/2)"),
        ("y' = y + x", "y = -x - 1 + C e^x"),
        ("y' + xy = x", "y = 1 + C e^(-x^2/2)"),
        ("y' = x y^2", "y = -1/(x^2/2 + C)"),
        ("y' = 1/x", "y = ln|x| + C"),
        ("y'' - 3y' + 2y = 0", "y = C1 e^(2x) + C2 e^x"),
        ("y'' + 2y' + y = 0", "y = C1 e^(-x) + C2 x e^(-x)"),
        (
            "y'' + 2y' + 5y = 0",
            "y = C1 e^(-x) cos(2x) + C2 e^(-x) sin(2x)",
        ),
        (
            "y'' + y = sin(x)",
            "y = -x cos(x)/2 + C1 cos(x) + C2 sin(x)",
        ),
        ("y'' = 6x", "y = x^3 + C1 + C2 x"),
        ("d^2y/dx^2 + 4y = 0", "y = C1 cos(2x) + C2 sin(2x)"),
        ("x'' + x = 0", "x = C1 cos(t) + C2 sin(t)"),
        ("y'(x) = 2x", "y(x) = x^2 + C"),
        ("y′ = 2x", "y = x^2 + C"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

#[test]
fn conditions_after_a_semicolon_settle_the_constants() {
    for (t, want) in [
        ("y' = y; y(0)=2", "y = 2e^x"),
        ("y' = 2x; y(0)=1", "y = x^2 + 1"),
        ("y'' + y = 0; y(0)=1; y'(0)=0", "y = cos(x)"),
        ("y' + 2y = e^x; y(0)=1", "y = e^x/3 + 2e^(-2x)/3"),
        ("y' = y^2; y(0)=1", "y = -1/(x - 1)"),
        ("dy/dt = -3y; y(0)=5", "y = 5e^(-3t)"),
        ("y' = cos(x); y(pi)=0", "y = sin(x)"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

#[test]
fn a_differential_equation_it_cant_solve_stays_blank() {
    for t in [
        "y' = tan(x)",
        "y' = x/y",
        "y' = k y",
        "y'=e^(y)",
        "y' = y'",
        "sin(y') = x",
        // two constants, one condition
        "y'' + y = 0; y(0)=1",
        "y'' + x y = 0",
    ] {
        assert_eq!(show(t), "", "{t}");
    }
    // a mark after a digit is still feet
    assert_eq!(show("6'2\""), "6 ft 2 in");
}

#[test]
fn fractional_inches() {
    assert_eq!(show("5 3/8\" + 2 7/16\""), "7 13/16 in");
    assert_eq!(show("6'5 3/8\""), "6 ft 5 3/8 in");
    assert_eq!(show("(6'5 3/8\") / 2"), "3 ft 2 11/16 in");
    assert_eq!(show("3/8\" * 2"), "3/4 in");
    assert_eq!(show("10\" / 3 + 1/16\""), "≈ 3 3/8 in");
    assert_eq!(show("5 3/8\" to mm"), "136.525 mm");
}

#[test]
fn unit_sums_keep_sig_figs() {
    let sf = EvaluateOptions {
        sig_fig_mode: Some(true),
        ..EvaluateOptions::default()
    };
    let at = |t: &str| evaluate_line(t, &sf).display;
    assert_eq!(at("12.0 kg + 3.15 kg"), "15.2 kg");
    assert_eq!(at("12.0 m - 3.15 m"), "8.9 m");
    assert_eq!(at("1.20 m + 35 cm"), "1.55 m");
    assert_eq!(at("2.50 m * 3.1 s"), "7.8 m·s");
    // off, nothing rounds
    assert_eq!(show("12.0 kg + 3.15 kg"), "15.15 kg");
}

// ---- native only: the working for a line (`steps`, `lib/steps`)

mod working {
    use qcalc_core::steps::{Step, step_document, steps};
    use qcalc_core::types::{AngleMode, EvaluateOptions};

    fn rad() -> EvaluateOptions {
        EvaluateOptions {
            angle_mode: Some(AngleMode::Rad),
            sig_figs: Some(6.0),
            ..EvaluateOptions::default()
        }
    }

    fn whys(s: &[Step]) -> Vec<&str> {
        s.iter().map(|x| x.why.as_str()).collect()
    }

    /// The last step ends in the answer the bar shows.
    fn ends_in_answer(t: &str, o: &EvaluateOptions) -> Vec<Step> {
        let s = steps(t, o).unwrap_or_else(|| panic!("no working for {t}"));
        let shown = qcalc_core::evaluate::evaluate_line(t, o).display;
        let last = &s.last().unwrap().plain;
        assert!(last.ends_with(&shown), "{t}: {last} vs {shown}");
        assert!(
            s.iter().all(|x| !x.math.is_empty() && !x.why.is_empty()),
            "{t}"
        );
        s
    }

    #[test]
    fn derivatives_name_their_rules() {
        let o = rad();
        let s = ends_in_answer("d/dx x^2 sin(x)", &o);
        assert!(whys(&s)[0].starts_with("power rule"));
        assert!(whys(&s).last().unwrap().starts_with("product rule"));
        assert!(s.last().unwrap().math.starts_with("frac(dif, dif x)"));
        let s = ends_in_answer("d/dx (x^2+1)/(x-1)", &o);
        assert!(whys(&s).last().unwrap().starts_with("quotient rule"));
        let s = ends_in_answer("d/dx sin(x^2)", &o);
        assert!(whys(&s).last().unwrap().starts_with("chain rule"));
        let s = ends_in_answer("d/dx 3x^4 + 2x", &o);
        assert!(whys(&s).iter().any(|w| w.starts_with("constant multiple")));
        assert!(whys(&s).last().unwrap().starts_with("sum rule"));
        let s = ends_in_answer("d/dx x^3 at 2", &o);
        assert_eq!(s.last().unwrap().plain, "3(2)² = 12");
        // degrees: the π/180 the bar's answer has is in the working too
        let s = ends_in_answer("d/dx x^2 sin(x)", &EvaluateOptions::default());
        assert!(whys(&s).iter().any(|w| w.contains("in degrees")));
    }

    #[test]
    fn integrals_say_how() {
        let o = rad();
        let s = ends_in_answer("∫ x e^x dx", &o);
        assert_eq!(whys(&s)[0], "integrate by parts: u = x, dv = e^x dx");
        let s = ends_in_answer("integral of x e^x", &o);
        assert_eq!(whys(&s)[0], "integrate by parts: u = x, dv = e^x dx");
        let s = ends_in_answer("∫ 2x cos(x^2) dx", &o);
        assert!(
            whys(&s)[0].starts_with("substitute u = x^2"),
            "{:?}",
            whys(&s)
        );
        let s = ends_in_answer("∫ x^2 + 3x - 1/x dx", &o);
        assert_eq!(whys(&s)[0], "sum rule: integrate term by term");
        assert_eq!(s.len(), 5);
        let s = ends_in_answer("∫ 3 sin(2x) dx", &o);
        assert_eq!(whys(&s)[0], "∫ sin u du = −cos u, u = 2x");
        assert!(s[0].math.starts_with("integral"));
    }

    #[test]
    fn equations_isolate_or_use_the_formula() {
        let o = rad();
        let s = ends_in_answer("3(x-1) = x + 5", &o);
        assert_eq!(
            whys(&s),
            [
                "the equation",
                "expand both sides",
                "subtract x and add 3 on both sides",
                "divide both sides by 2",
                "the answer"
            ]
        );
        let s = ends_in_answer("x^2 - 5x + 6 = 0", &o);
        assert!(whys(&s)[1].starts_with("quadratic formula, a = 1, b = -5, c = 6"));
        assert_eq!(s.last().unwrap().plain, "x = 2, 3");
        let s = ends_in_answer("x^2 = 8", &o);
        assert_eq!(whys(&s)[1], "take the square root of both sides");
        ends_in_answer("x^2+1=0", &o);
        ends_in_answer("solve x^2+x-1=0", &o);
    }

    #[test]
    fn conversions_show_the_factor() {
        let o = rad();
        let s = ends_in_answer("5 ft to m", &o);
        assert_eq!(s[0].why, "1 ft = 0.3048 m");
        let s = ends_in_answer("60 mph to km/h", &o);
        assert_eq!(s.len(), 3);
        assert_eq!(s[1].why, "1 km/h = 0.277778 m/s");
    }

    #[test]
    fn differential_equations_name_the_method() {
        let o = rad();
        let s = ends_in_answer("y'' + 3y' + 2y = 0", &o);
        assert_eq!(s[0].plain, "r^2 + 3r + 2 = 0");
        assert_eq!(s[1].plain, "r = -1, -2");
        let s = ends_in_answer("y'' + y = 0; y(0)=1; y'(0)=0", &o);
        assert_eq!(s[3].plain, "y(0) = 1, y'(0) = 0 ⇒ C1 = 1, C2 = 0");
        let s = ends_in_answer("y' = x y", &o);
        assert!(s[0].why.starts_with("integrating factor"));
        let s = ends_in_answer("y' = x y^2", &o);
        assert_eq!(s[0].why, "separate the variables");
        let s = ends_in_answer("y' + 2y = e^x", &o);
        assert!(whys(&s).contains(&"particular solution by undetermined coefficients"));
        let s = ends_in_answer("y' = 2x", &o);
        assert_eq!(s[0].why, "integrate both sides");
    }

    #[test]
    fn nothing_else_has_working() {
        let o = rad();
        for t in [
            "2+2",
            "sin(30)",
            "",
            "hello",
            "x^3 = 8",
            "d/dx",
            "∫ e^(x^2) dx",
            "5 ft + 3 in",
            "graph x^2",
        ] {
            assert!(steps(t, &o).is_none(), "{t}");
        }
        assert!(step_document("x^2").contains("$ x^2 $"));
    }
}

/// The bar's hover note on a converted amount, called as the Mac calls it: the ECB page JSON with its date, and now.
#[test]
fn a_converted_amount_notes_its_rate() {
    use qcalc_core::golden::Outcome;
    use serde_json::json;
    let page =
        r#"{"base":"EUR","rates":{"GBP":0.8721,"JPY":171.2,"USD":1.1224},"date":"2026-11-05"}"#;
    let now = 1_793_957_056_000.0;
    let note = |text: &str| match qcalc_core::app::golden::dispatch(
        "lib/currency",
        "rateNote",
        &[json!(text), json!(page), json!(now)],
    ) {
        Some(Outcome::Answer(v)) => v.as_str().map(str::to_string),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        note("100 usd in eur").as_deref(),
        Some("1 USD = 0.89095 EUR · ECB rate, Nov 5")
    );
    assert_eq!(
        note("100 usd to gbp").as_deref(),
        Some("1 USD = 0.777 GBP · ECB rate, Nov 5")
    );
    assert_eq!(note("2 + 2"), None);
}

/// `with respect to t` names the variable: the t isn't coloured as tonnes, while `5 kg to t` still is a unit.
#[test]
fn with_respect_to_is_not_a_unit() {
    use qcalc_core::app::unit_spans::unit_spans;
    assert!(unit_spans("integrate t^2 with respect to t", &[], &[]).is_empty());
    assert_eq!(unit_spans("5 kg to t", &[], &[]).len(), 2);
}
