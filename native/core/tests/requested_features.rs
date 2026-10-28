//! Every engine or app-logic feature the user asked for in their Q Calc chats, checked against the native core.
//! One test per request (or a few close ones); `native/REQUESTED.md` lists them all with dates and names these
//! tests. UI-only requests are listed there instead.
use std::collections::BTreeMap;

use qcalc_core::app::chain::ChainAnswer;
use qcalc_core::app::live::{LiveCommand, LiveRequest, live};
use qcalc_core::evaluate::{evaluate_line, evaluate_sheet};
use qcalc_core::types::{AngleMode, EvaluateOptions};

/// The app's defaults (6 sig figs, degrees).
fn opts() -> EvaluateOptions {
    qcalc_core::app::settings::default_settings().eval_options()
}

fn rad() -> EvaluateOptions {
    EvaluateOptions {
        angle_mode: Some(AngleMode::Rad),
        ..opts()
    }
}

fn show(t: &str) -> String {
    evaluate_line(t, &opts()).display
}

fn sheet(lines: &[&str]) -> Vec<String> {
    let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    evaluate_sheet(&lines, &opts())
        .into_iter()
        .map(|r| r.display)
        .collect()
}

fn req(text: &str) -> LiveRequest {
    LiveRequest {
        text: text.into(),
        phrases: true,
        options: opts(),
        ..Default::default()
    }
}

/// The bar's answer slot for a line typed after `last` (the last answer, as the tape hands it on).
fn after(last: &str, unit: bool, text: &str) -> (String, bool, String) {
    let mut r = req(text);
    if unit {
        r.options.quantities.insert("ans".into(), last.into());
    } else {
        r.options.ans = last.parse().ok();
    }
    r.chain_answer = Some(ChainAnswer {
        plain: last.into(),
        unit,
    });
    let a = live(&r);
    (a.slot, a.chained, a.tape_expr)
}

// 2026-09-30: "+ 2" works on the previous answer (an operator first continues from ans)
#[test]
fn an_operator_first_chains_from_the_last_answer() {
    let (slot, chained, tape) = after("5", false, "+2");
    assert_eq!((slot.as_str(), chained, tape.as_str()), ("7", true, "5+2"));
    let (slot, chained, _) = after("27.9629 mm", true, "/10");
    assert_eq!((slot.as_str(), chained), ("2.79629 mm", true));
    // a leading minus is a negative number, not a chain
    assert!(!after("5", false, "-3").1);
}

// 2026-09-30: stress/strain sheets mixed units; keep what was typed
#[test]
fn stress_and_strain_sheets_keep_their_units() {
    assert_eq!(
        sheet(&[
            "F = 10 kN",
            "A = 20 mm^2",
            "sigma = F/A",
            "L = 500 mm",
            "eps = 0.0025",
            "delta = eps*L",
            "E = 200 GPa",
            "sigma/E",
            "E*0.001",
        ]),
        [
            "10 kN", "20 mm²", "500 MPa", "500 mm", "0.0025", "1.25 mm", "200 GPa", "0.0025",
            "200 MPa"
        ]
    );
    assert_eq!(show("10 mi / 2 hr"), "5 mph");
}

// 2026-09-30: the answer's unit follows the system typed (3mm+8mm in mm, 3in+4in in inches)
#[test]
fn answers_stay_in_the_system_typed() {
    assert_eq!(show("3mm+8mm"), "11 mm");
    assert_eq!(show("3in+4in"), "7 in");
    assert_eq!(show("3in*4in"), "12 in²");
    assert_eq!(show("3mm*8mm"), "24 mm²");
}

// 2026-09-30: "free up E": capital E is a variable, lowercase e stays Euler's number
#[test]
fn capital_e_is_free_for_a_variable() {
    assert_eq!(
        sheet(&["E = 5", "E*2", "2E", "1E3"]),
        ["5", "10", "10", "1000"]
    );
    assert_eq!(show("e"), "2.71828");
}

// 2026-09-30: matrix operations
#[test]
fn matrix_operations() {
    assert_eq!(show("[1,2;3,4]+[5,6;7,8]"), "[6 8; 10 12]");
    assert_eq!(show("[5,6;7,8]-[1,2;3,4]"), "[4 4; 4 4]");
    assert_eq!(show("3*[1,2;3,4]"), "[3 6; 9 12]");
    assert_eq!(show("[1,2;3,4]*[5,6;7,8]"), "[19 22; 43 50]");
    assert_eq!(show("transpose([1,2;3,4])"), "[1 3; 2 4]");
    assert_eq!(show("[1,2;3,4]'"), "[1 3; 2 4]");
    assert_eq!(show("det([1,2,3;0,1,4;5,6,0])"), "1");
    assert_eq!(show("inv([1,2;3,4])"), "[-2 1; 1.5 -0.5]");
    assert_eq!(show("[1,2;3,4]^-1"), "[-2 1; 1.5 -0.5]");
    assert_eq!(show("trace([1,2;3,4])"), "5");
    // sizes that don't match have no answer
    assert_eq!(show("[1,2]+[1,2,3]"), "");
}

// 2026-09-30: isolate a variable; the R_eq line that didn't work
#[test]
fn isolate_a_variable() {
    assert_eq!(show("isolate x in y = 2x + 3"), "x = (y - 3)/2");
    assert_eq!(
        show("isolate R_eq in 1 / R_eq = 1 / R_1 + 1 / R_2"),
        "R_eq = R_1*R_2/(R_2 + R_1)"
    );
    assert_eq!(
        show(r#""isolate" R_("eq") "in" 1 / R_("eq") = 1 / R_(1) + 1 / R_(2)"#),
        "R_eq = R_1*R_2/(R_2 + R_1)"
    );
}

// 2026-09-30: "solve x" isolates when there's no number to find; alone it reads the last line
#[test]
fn solve_falls_back_to_isolating() {
    assert_eq!(show("solve x + y = 3 for x"), "x = 3 - y");
    assert_eq!(show("solve(x + y = 3, x)"), "x = 3 - y");
    assert_eq!(show("solve x^2 + y = 3 for x"), "x = ±sqrt(3 - y)");
    for word in ["solve x", "solve for x", "isolate x"] {
        let mut r = req(word);
        r.last_expr = Some("y = 2x + 3".into());
        assert_eq!(live(&r).slot, "x = (y - 3)/2", "{word}");
    }
}

// 2026-09-30: partial derivatives
#[test]
fn partial_derivatives() {
    assert_eq!(show("∂/∂x x^2 y"), "2xy");
    assert_eq!(show("d/dx x^2*y"), "2xy");
}

// 2026-09-30: sys3 opens three equations, solves before all are filled, and with units
#[test]
fn systems_of_equations() {
    use qcalc_core::system::{solve_live, sys_command};
    assert_eq!(sys_command("sys3"), Some(Ok(3)));
    assert_eq!(sys_command("sys 3"), Some(Ok(3)));
    assert_eq!(live(&req("sys3")).command, Some(LiveCommand::Sys));
    let du = Default::default();
    let solve = |lines: &[&str]| {
        let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        solve_live(&lines, &du, AngleMode::Deg).map(|a| a.display)
    };
    assert_eq!(
        solve(&["x + y = 3", "x - y = 1", ""]).as_deref(),
        Some("x = 2, y = 1")
    );
    assert_eq!(
        solve(&["F1 + F2 = 10 N", "F1 - F2 = 2 N"]).as_deref(),
        Some("F1 = 6 N, F2 = 4 N")
    );
}

// 2026-10-26: paste a worksheet's equations into the solver (a moment sum with no = is = 0)
#[test]
fn pasted_equations_solve() {
    use qcalc_core::system::{pasted_equations, solve_live};
    let eqs = pasted_equations("20(7.5)-B_y(5)+40(2)\nB_y+D_y = 60").unwrap();
    assert_eq!(eqs, ["20(7.5)-B_y(5)+40(2) = 0", "B_y+D_y = 60"]);
    let got = solve_live(&eqs, &Default::default(), AngleMode::Deg).unwrap();
    assert_eq!(got.display, "B_y = 46, D_y = 14");
}

// 2026-10-06: solve with units, parentheses and a named variable
#[test]
fn solve_with_units() {
    assert_eq!(
        show(r#"solve(0.5 = e ^ (- t / 0.384 "milliseconds"), t)"#),
        "0.266169 ms"
    );
}

// 2026-10-07: solve(…, x = 0.2) takes an initial guess
#[test]
fn solve_with_a_starting_guess() {
    assert_eq!(
        evaluate_line("solve(x ^ 2 - cos(x) = 0, x = 0.2)", &rad()).display,
        "0.824132"
    );
    // in degrees cos reads degrees
    assert_eq!(show("solve(x ^ 2 - cos(x) = 0, x = 0.2)"), "0.999924");
}

// 2026-10-08: time derivatives of the five forms asked about
#[test]
fn time_derivatives() {
    assert_eq!(show(r"d/dt $x\sin\theta$"), "ẋ sin(θ) + xθ̇ cos(θ)");
    assert_eq!(show(r"d/dt $L\cos^2\theta$"), "-2Lθ̇ sin(θ) cos(θ)");
    assert_eq!(show(r"d/dt $\theta^2\dot\theta$"), "2θ θ̇² + θ²θ̈");
    assert_eq!(show(r"d/dt $\sqrt{x^2+h^2}$"), show("d/dt sqrt(x^2+h^2)"));
    assert_eq!(show("d/dt sqrt(x^2+h^2)"), "xẋ/√(x² + h²)");
    assert_eq!(show(r"d/dt $e^{k\theta}\,\dot\theta$"), "e^(kθ) (k θ̇² + θ̈)");
    // typed plainly too
    assert_eq!(show("d/dt x sin(θ)"), "ẋ sin(θ) + xθ̇ cos(θ)");
}

// 2026-10-26: f(t) = t*t then d/dt f(t) (substituted arguments were shared)
#[test]
fn derivative_of_a_defined_function() {
    assert_eq!(
        sheet(&["f(t) = t*t", "d/dt f(t)", "f'(t)"])[1..],
        ["2t", "2t"]
    );
}

// 2026-10-09 / 2026-10-10: inverse trig answers say deg or rad, however the inverse is written
#[test]
fn inverse_trig_names_its_angle_unit() {
    assert_eq!(show("asin(0.5)"), "30 deg");
    assert_eq!(show("arctan(1)"), "45 deg");
    assert_eq!(show("tan ^(-1) (1781 / 1135)"), "57.4914 deg");
    assert_eq!(evaluate_line("asin(0.5)", &rad()).display, "0.523599 rad");
    assert_eq!(evaluate_line("atan(1)", &rad()).display, "0.785398 rad");
}

// 2026-10-09: torque per angle stays N m / deg (or / rad), and kg m²/s² simplifies
#[test]
fn torque_per_angle_keeps_its_units() {
    assert_eq!(show(r#"549.16 "Nm" / (0.293 deg)"#), "1874.27 N m / deg");
    assert_eq!(
        evaluate_line(r#"549.16 "Nm" / (0.293 deg)"#, &rad()).display,
        "107388 N m / rad"
    );
    assert_eq!(show("2 kg m^2/s^2"), "2 J");
}

// 2026-10-09 / 2026-10-23: conversions inside a line, and `ans mm to in` gives ans a unit then converts
#[test]
fn conversions_on_the_last_answer() {
    assert_eq!(show(r#"(1.1009 "in" "to" "mm") / 10"#), "2.79629 mm");
    assert_eq!(after("1.1009", false, "ans in to mm").0, "27.9629 mm");
    assert_eq!(after("25.4", false, "ans mm to in").0, "1 in");
    assert_eq!(after("1.1009 in", true, "to mm").0, "27.9629 mm");
    assert_eq!(after("1.1009 in", true, "in mm").0, "27.9629 mm");
    // and arithmetic keeps the answer's unit
    assert_eq!(after("3 m", true, "ans * 2").0, "6 m");
}

// 2026-10-11: phi, hex and binary, space-thousands as a setting (a space multiplies by default), complex numbers
#[test]
fn phi_bases_spaces_and_complex() {
    assert_eq!(show("phi"), "1.61803");
    assert_eq!(show("0xff"), "255");
    assert_eq!(show("255 in hex"), "0xFF");
    assert_eq!(show("0b101"), "5");
    assert_eq!(show("(1+2i)*(3-i)"), "5 + 5i");
    assert_eq!(show("3 4"), "12");
    assert!(!qcalc_core::app::settings::default_settings().space_thousands);
    let spaced = EvaluateOptions {
        space_thousands: Some(true),
        ..opts()
    };
    assert_eq!(evaluate_line("1 000 000 + 1", &spaced).display, "1000001");
}

// 2026-10-14: a sum missing a unit (the volt) gets the unit offered, ↵ again takes it
#[test]
fn a_missing_volt_is_offered() {
    let line = "(10 mA + 5 / 1 kohm) / (1 / 1 kohm + 1 / (2 kohm + 4 kohm))";
    assert_eq!(show(line), "improper unit conversion");
    let ok = |s: &str| {
        let r = evaluate_line(s, &opts());
        !r.display.is_empty() && r.error.is_none() && !r.display.contains("improper")
    };
    let fix = qcalc_core::app::missing_unit::missing_unit(line, &ok).unwrap();
    assert_eq!(fix.text, " V");
    assert_eq!(fix.hint, "5 has no unit · ↵ again makes it 5 V");
    assert_eq!(show(&fix.filled), "12.8571 V");
}

// 2026-10-16: solve tan(2θ) = 4/3 gives every angle, each its own root to pick
#[test]
fn every_angle_of_a_trig_solve() {
    let a = live(&req("solve tan(2θ) = 4/3"));
    assert_eq!(
        a.roots.as_deref(),
        Some(&["26.5651", "116.565", "206.565", "296.565"].map(String::from)[..])
    );
    assert_eq!(
        evaluate_line("solve tan(2θ) = 4/3", &rad()).display,
        "0.463648, 2.03444, 3.60524, 5.17604"
    );
}

// 2026-10-06: hello says hi back
#[test]
fn hello_says_hi() {
    for t in ["hello", "hello there"] {
        let a = live(&req(t));
        assert_eq!(
            (a.slot.as_str(), a.command),
            ("hi 👋", Some(LiveCommand::Greeting))
        );
    }
}

// 2026-10-09: game or games lists the games; a name opens one
#[test]
fn games_list() {
    for t in ["game", "games"] {
        assert_eq!(live(&req(t)).command, Some(LiveCommand::Games), "{t}");
    }
    use qcalc_core::app::games::{GameKind, game_command};
    assert_eq!(game_command("pong"), Some(GameKind::Pong));
}

// 2026-10-12 / 2026-10-23: the conversions, constants and identity sheets, with subfields
#[test]
fn reference_sheets_open_by_name() {
    let sheet_for = |t: &str| match live(&req(t)).command {
        Some(LiveCommand::Identity { sheet, section }) => (sheet, section),
        other => panic!("{t}: {other:?}"),
    };
    assert_eq!(sheet_for("conversion").0, "conversions");
    assert_eq!(sheet_for("metric prefixes").0, "conversions");
    assert_eq!(sheet_for("constants").0, "constants");
    assert_eq!(sheet_for("idt constants").0, "constants");
    assert_eq!(sheet_for("trig").0, "identities");
    assert_eq!(sheet_for("identities").0, "identities");
    assert_eq!(sheet_for("idt integration"), ("calculus".into(), 1));
    assert_eq!(sheet_for("idt partial fractions"), ("identities".into(), 3));
    assert_eq!(sheet_for("idt summation"), ("identities".into(), 4));
    // the constants asked for are on the sheet, and the prefixes on the conversions
    let text = |id: &str| {
        serde_json::to_string(qcalc_core::app::identities::identity_sheet_by_id(id).unwrap())
            .unwrap()
    };
    for c in ["Bohr magneton", "Stefan–Boltzmann", "flux quantum"] {
        assert!(text("constants").contains(c), "{c}");
    }
    for p in ["kilo", "mega", "nano"] {
        assert!(text("conversions").contains(p), "{p}");
    }
}

// 2026-10-23: chem R is the gas constant, with a word to pick its units
#[test]
fn gas_constant_in_its_units() {
    assert_eq!(show("chem R"), "8.31446 J·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R atm"), "0.0820574 L·atm·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R bar"), "0.0831446 L·bar·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R torr"), "62.3636 L·torr·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R mmhg"), "62.3636 L·mmHg·mol⁻¹·K⁻¹");
    assert_eq!(show("chem R cal"), "1.9872 cal·mol⁻¹·K⁻¹");
}

// 2026-10-23: picking several elements builds one formula instead of stacking them
#[test]
fn periodic_picks_make_one_formula() {
    let f = qcalc_core::app::elements::formula_of_picks(&["H", "H", "O"]).unwrap();
    assert_eq!((f.formula.as_str(), f.mass.as_str()), ("H₂O", "18.015"));
    assert_eq!(live(&req("periodic")).command, Some(LiveCommand::Periodic));
}

// 2026-10-23: graph bounds (several per axis) and graphs of f(x, y) as level curves
#[test]
fn graph_bounds_and_two_variable_functions() {
    use qcalc_core::graph::{ContourKind, GraphRange, parse_graph_intent};
    let (f, v) = (BTreeMap::new(), BTreeMap::new());
    let g = parse_graph_intent("graph y = x ^ 2 {2 < y < 5}", &f, &v).unwrap();
    assert_eq!(g.bounds.unwrap().y, [GraphRange { lo: 2.0, hi: 5.0 }]);
    let g = parse_graph_intent("graph x^2 {0 < x < 1} {2 < x < 3}", &f, &v).unwrap();
    assert_eq!(
        g.bounds.unwrap().x,
        [
            GraphRange { lo: 0.0, hi: 1.0 },
            GraphRange { lo: 2.0, hi: 3.0 }
        ]
    );
    let g = parse_graph_intent("graph f(x,y) = x^2 + y^2", &f, &v).unwrap();
    assert_eq!(
        (g.expression.as_str(), g.contour),
        ("x^2 + y^2", Some(ContourKind::Levels))
    );
    assert_eq!(
        live(&req("graph f(x,y) = x^2 + y^2")).command,
        Some(LiveCommand::Graph)
    );
}

// 2026-10-23: message <name> <words>, and (2026-10-27) open messages with a past contact
#[test]
fn message_commands() {
    use qcalc_core::app::messaging::{ThreadCommand, parse_message, parse_messages_command};
    let m = parse_message("message max hello").unwrap();
    assert_eq!((m.to.as_str(), m.text.as_str()), ("max", "hello"));
    assert_eq!(
        live(&req("message max hello")).command,
        Some(LiveCommand::Message)
    );
    assert_eq!(
        parse_messages_command("open messages with max2"),
        Some(ThreadCommand::Open {
            to: "max2".into(),
            name: "max2".into()
        })
    );
    assert_eq!(
        parse_messages_command("messages"),
        Some(ThreadCommand::List)
    );
}

// 2026-10-23: the walkthrough retires after a few opens
#[test]
fn the_tutorial_retires_after_a_few_opens() {
    use qcalc_core::app::onboarding::Onboarding;
    use qcalc_core::app::tour::{TOUR_OFF, TOUR_ON, TOUR_OPENS, enrol_tour, tour_running};
    let fresh = enrol_tour(&Onboarding::default(), true);
    assert!(tour_running(&fresh));
    let later = Onboarding {
        opens: TOUR_OPENS + 1,
        hints: TOUR_ON,
        ..Default::default()
    };
    assert!(!tour_running(&later));
    assert_ne!(enrol_tour(&later, true).hints & TOUR_OFF, 0);
}

// 2026-09-30: every keybind is the user's to choose (and 2026-10-18: a custom hotkey like alt+q is kept)
#[test]
fn keybinds_are_the_users() {
    use qcalc_core::app::keybinds::{KeyAction, Keybinds, keybind_for, set_keybind};
    let binds = set_keybind(&Keybinds::new(), KeyAction::Show, "alt+q", false);
    assert_eq!(keybind_for(&binds, KeyAction::Show, false), "alt+q");
    let binds = set_keybind(&binds, KeyAction::Angle, "ctrl+r", false);
    assert_eq!(keybind_for(&binds, KeyAction::Angle, false), "ctrl+r");
    assert_eq!(keybind_for(&binds, KeyAction::Clear, false), "ctrl+c");
}

// 2026-10-02: copy unitless is a setting, on by default; 2026-10-27: 6 significant figures by default
#[test]
fn copy_unitless_and_six_sig_figs_by_default() {
    let s = qcalc_core::app::settings::default_settings();
    assert!(s.copy_unitless);
    assert_eq!(s.sig_figs, 6);
    assert_eq!(show("1/3"), "0.333333");
}

// 2026-10-02 / 2026-10-27: Tab (or →) fills in the grey parens; nothing to fill on an empty line
#[test]
fn tab_fills_the_grey_parens() {
    use qcalc_core::parens::{autofill_parens, infer_parens};
    assert_eq!(
        autofill_parens("(838*2123", 9, 9).as_deref(),
        Some("(838*2123)")
    );
    assert_eq!(autofill_parens("5+3)*2", 6, 6).as_deref(), Some("(5+3)*2"));
    let empty = infer_parens("");
    assert_eq!((empty.leading, empty.trailing), (0, 0));
}

// 2026-10-14: Tab completes a word in the middle of a longer expression
#[test]
fn tab_completes_inside_an_expression() {
    use qcalc_core::app::completion::{CompletionNames, completion_for};
    let names = CompletionNames::default();
    assert_eq!(completion_for("3*(2 + sqr", &names), "t");
    assert_eq!(completion_for("2 + 3 kilom", &names), "eter");
}

// 2026-10-06: units are coloured (the spans the field paints)
#[test]
fn units_are_found_to_colour() {
    let spans = qcalc_core::app::unit_spans::unit_spans("3 mm + 2 in", &[], &[]);
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["millimeter", "inch"]);
}

// 2026-10-23: Soulver went; the phrases it answered still answer
#[test]
fn soulver_style_phrases_still_answer() {
    assert_eq!(show("5 km in miles"), "3.10686 mi");
    assert_eq!(show("what is 20% of 50"), "10");
}
