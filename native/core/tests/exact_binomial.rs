//! Closed forms of an integer plus a square root (`a ± b√r` over a small denominator): what a binomial
//! denominator rationalizes to, and the trig values built from √5 and √2 (QA item 4).
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn exact(t: &str) -> Option<String> {
    let o = qcalc_core::app::settings::default_settings().eval_options();
    evaluate_line(t, &o).exact
}

#[test]
fn binomial_denominators_rationalize() {
    for (t, want) in [
        ("1/(1+sqrt(2))", "sqrt(2)-1"),
        ("1/(sqrt(3)-1)", "(1+sqrt(3))/2"),
        ("1/(2-sqrt(3))", "2+sqrt(3)"),
        ("2/(1-sqrt(5))", "(-1-sqrt(5))/2"),
        ("1/(3+2sqrt(2))", "3-2sqrt(2)"),
        ("2/(3+sqrt(7))", "3-sqrt(7)"),
    ] {
        assert_eq!(exact(t).as_deref(), Some(want), "{t}");
    }
}

#[test]
fn typed_binomials_keep_the_decimal_alone() {
    // nothing new to say: the closed form is what was typed (and the TS recordings show none)
    for t in ["1+sqrt(2)", "(1+sqrt(5))/2", "1+√(2)/2", "3+2sqrt(2)"] {
        assert_eq!(exact(t), None, "{t}");
    }
}

#[test]
fn trig_values_with_sqrt5_and_sqrt2() {
    for (t, want) in [
        ("tan(22.5)", "sqrt(2)-1"),
        ("tan(67.5)", "1+sqrt(2)"),
        ("sin(18)", "(sqrt(5)-1)/4"),
        ("cos(36)", "(1+sqrt(5))/4"),
        ("tan(75)", "2+sqrt(3)"),
        // the existing forms win
        ("sin(15)", "(sqrt(6)-sqrt(2))/4"),
        ("sin(45)", "sqrt(2)/2"),
        ("sqrt(12)", "2sqrt(3)"),
        ("cos(60)", "1/2"),
    ] {
        assert_eq!(exact(t).as_deref(), Some(want), "{t}");
    }
}

#[test]
fn no_false_closed_forms() {
    for t in ["sin(1)", "sin(10)", "cos(20)", "sqrt(2)+0.123456", "sin(1)+sqrt(2)"] {
        assert_eq!(exact(t), None, "{t}");
    }
    // only trig and root lines get a closed form at all
    assert_eq!(evaluate_line("0.414213562373095", &EvaluateOptions::default()).exact, None);
}
