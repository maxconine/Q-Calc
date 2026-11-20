//! Non-dynamics sheet rows state the conditions they need (QA item 63).
use qcalc_core::app::identities::native_identity_sheet_by_id;

fn note(sheet: &str, show: &str) -> String {
    native_identity_sheet_by_id(sheet)
        .unwrap_or_else(|| panic!("no sheet {sheet}"))
        .sections
        .iter()
        .flat_map(|s| s.groups.iter())
        .flat_map(|g| g.items.iter())
        .find(|i| i.show == show)
        .unwrap_or_else(|| panic!("no {sheet} row {show}"))
        .note
        .clone()
        .unwrap_or_default()
}

/// sF(s) can have a limit at 0 when f never settles (sin t gives 0), so the row names the real condition.
#[test]
fn final_value_theorem_needs_stable_poles() {
    let n = note("laplace", "f(∞) = lim_{s→0} s F(s)");
    assert!(n.contains("left half-plane"), "{n}");
}

/// The overshoot formula is for an underdamped loop, and the finite geometric sum divides by 1 − r.
#[test]
fn overshoot_and_geometric_sum_say_when_they_hold() {
    assert!(note("circuits", "%OS = 100 e^{−πζ/√(1 − ζ^{2})}").contains("ζ < 1"));
    assert!(note("precalculus", "S_{n} = a_{1}(1 − r^{n})/(1 − r)").contains("r ≠ 1"));
}
