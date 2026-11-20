//! Isolate answers tidied (QA item 35): a product's numbers and powers gathered after isolate
//! (`(1/(2pi))^(-2)` → `4pi^2`), `0.5 a` in a formula is a coefficient and a letter, not half a
//! year, and `KE`/`PE` are one name.
use qcalc_core::evaluate::evaluate_line;

fn display(t: &str) -> String {
    let o = qcalc_core::app::settings::default_settings().eval_options();
    evaluate_line(t, &o).display
}

#[test]
fn powers_and_numbers_gathered() {
    for (t, want) in [
        ("T = 2 pi sqrt(L/g) for g", "g = 4pi^2*L/T^2"),
        ("T = 2 pi sqrt(L/g) for L", "L = g*T^2/(4pi^2)"),
        ("T = 2 pi sqrt(m/k) for k", "k = 4pi^2*m/T^2"),
        ("f = 1/(2 pi sqrt(L C)) for C", "C = 1/(4pi^2*L*f^2)"),
        ("E = 0.5 m v^2 for v", "v = ±sqrt(2E/m)"),
    ] {
        assert_eq!(display(t), want, "{t}");
    }
}

#[test]
fn a_decimal_coefficient_on_a_letter_is_not_a_unit() {
    assert_eq!(display("s = u t + 0.5 a t^2 for u"), "u = (s - a*t^2/2)/t");
    assert_eq!(display("s = u t + 0.5 a t^2 for a"), "a = 2(s/t - u)/t");
    // a number and a unit alone is still a unit solve
    assert_eq!(display("x * 3 m = 12 m^2"), "4 m");
}

#[test]
fn physics_names_stay_one_word() {
    assert_eq!(display("KE = 0.5 m v^2 for m"), "m = 2KE/v^2");
    assert_eq!(display("KE = 0.5 m v^2 for v"), "v = ±sqrt(2KE/m)");
    assert_eq!(display("PE = m g h for h"), "h = PE/(g*m)");
    // other letters written together are still a product
    assert_eq!(display("PV = nRT for T"), "T = P*V/(R*n)");
}

#[test]
fn tidy_forms_left_alone() {
    for (t, want) in [
        ("V = 4/3 pi r^3 for r", "r = cbrt(3V/(4pi))"),
        ("v^2 = u^2 + 2 a s for s", "s = (v^2 - u^2)/(2a)"),
        ("F = G m1 m2 / r^2 for r", "r = ±sqrt(G*m1*m2/F)"),
        ("x = A cos(w t) for t", "t = acos(x/A)/w"),
    ] {
        assert_eq!(display(t), want, "{t}");
    }
}

#[test]
fn logs_of_the_letter_return() {
    // polishing a power whose exponent isn't a whole number once recursed forever
    assert_eq!(display("solve x in ln(x) = y"), "x = e^y");
    assert_eq!(display("solve x in log(x) = y"), "x = 10^y");
    assert_eq!(display("y = x^1.5 for x"), "x = y^(2/3)");
}
