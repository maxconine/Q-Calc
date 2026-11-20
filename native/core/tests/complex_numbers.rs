//! Complex answers (QA item 11): float noise in a part, `j` numbers with a power of ten, roots and logs of a
//! complex, and `angle(…)`/`phase(…)` with an `i`. A complex answer still needs a typed `i` or `j`.
use qcalc_core::evaluate::evaluate_line;
use qcalc_core::types::EvaluateOptions;

fn show(t: &str) -> String {
    evaluate_line(t, &EvaluateOptions::default()).display
}

#[test]
fn no_zero_real_part_from_float_noise() {
    for (t, want) in [
        ("(1+i)^10", "32i"),
        ("(1+i)^2", "2i"),
        ("e^(i*pi/2)", "i"),
        ("(1+j)^10", "j32"),
        ("3+0i", "3"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
}

#[test]
fn j_number_with_a_power_of_ten() {
    assert_eq!(show("1e3+j2e3"), "1000 + j2000");
    assert_eq!(show("1e3 + j2e-3"), "1000 + j0.002");
    assert_eq!(show("j2e3"), "j2000");
}

#[test]
fn roots_and_logs_of_a_complex() {
    assert_eq!(show("sqrt(3+4i)"), "2 + i");
    assert!(show("sqrt(i)").starts_with("0.707106781"), "{}", show("sqrt(i)"));
    assert!(show("ln(-1)+i").starts_with("4.14159265"), "{}", show("ln(-1)+i"));
    assert!(show("log(i)").starts_with("0.68218817"), "{}", show("log(i)"));
    // still undefined in plain real maths, as the web engine
    for t in ["sqrt(-4)", "sqrt(-100)", "ln(-1)", "log(-10)"] {
        assert_eq!(show(t), "undefined", "{t}");
    }
}

#[test]
fn angle_and_phase_of_an_i_number() {
    assert_eq!(show("angle(3+4i)"), show("phase(3+j4)"));
    assert!(show("angle(3+4i)").starts_with("53.1301"));
    assert_eq!(show("mag(3+4i)"), "5");
}
