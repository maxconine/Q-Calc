//! The dynamics sheet (Mitiguy's notation) states each row's conditions as the maths allows (QA item 55).
use qcalc_core::app::identities::{Identity, native_identity_sheet_by_id};

fn dynamics_row(show: &str) -> Identity {
    native_identity_sheet_by_id("dynamics")
        .expect("the dynamics sheet")
        .sections
        .iter()
        .flat_map(|s| s.groups.iter())
        .flat_map(|g| g.items.iter())
        .find(|i| i.show == show)
        .unwrap_or_else(|| panic!("no dynamics row {show}"))
        .clone()
}

/// K = ½ v·L + ½ ω·H holds for any body point Bₚ (H about Bₚ carries the cross term), not only the mass centre.
#[test]
fn kinetic_energy_from_momenta_holds_for_any_body_point() {
    let row = dynamics_row(
        "^{N}K^{B} = (1/2) ^{N}v⃗^{B_{p}} · ^{N}L⃗^{B} + (1/2) ^{N}ω⃗^{B} · ^{N}H⃗^{B∕B_{p}}",
    );
    assert_eq!(row.note.as_deref(), Some("any Bₚ fixed on B"));
    let e = row.explain.expect("explained");
    assert!(
        e.assumptions[0].starts_with("Bₚ is any point fixed on B"),
        "{:?}",
        e.assumptions
    );
}

/// The rigid-body shift of H holds for any B_p, and the zero-products row leaves out the triangle and tetrahedron.
#[test]
fn shift_and_products_rows_say_when_they_hold() {
    let shift = dynamics_row("^{N}H⃗^{B∕P} = ^{N}H⃗^{B∕B_{p}} + ^{P}r⃗^{B_{p}} × ^{N}L⃗^{B}");
    let e = shift.explain.expect("explained");
    assert!(
        e.assumptions[0].starts_with("Holds for any point"),
        "{:?}",
        e.assumptions
    );
    let zero = dynamics_row("I_{xy} = I_{xz} = I_{yz} = 0");
    assert!(
        zero.note
            .as_deref()
            .unwrap_or("")
            .contains("not the triangle")
    );
}
