//! QA item 61: golden inputs typed a character at a time through `app::live`.
//! A half-typed `sqrt(` inside a derivative panicked (the grey paren made `sqrt()`, and the tidy pass read its
//! missing argument), and a derivative term whose minus sat past its first factor printed `+ -`.
use qcalc_core::app::live::{LiveRequest, live};

fn shown(text: &str) -> String {
    live(&LiveRequest {
        text: text.into(),
        phrases: true,
        typst_preview: true,
        no_squiggle: true,
        now_ms: 1.7e12,
        ..Default::default()
    })
    .display
}

/// Every prefix answers without a panic; returns what each one shows.
fn typed(line: &str) -> Vec<(String, String)> {
    let ends = line
        .char_indices()
        .skip(1)
        .map(|(b, _)| b)
        .chain([line.len()]);
    ends.map(|end| (line[..end].to_string(), shown(&line[..end])))
        .collect()
}

#[test]
fn half_typed_call_in_a_derivative_is_blank_not_a_panic() {
    for line in [
        "d/dγ -1/m*(D*γ - L*v)/sqrt(γ^2 + v^2)",
        "∂/∂γ (-1/m (Dγ - Lv)/√(γ² + v²))",
        "d/dx 1/sqrt(x)",
        "d/dx x*sin(x)",
        "d/dt (r cos(phi))",
        "d/dt (x sin(theta))",
    ] {
        typed(line);
    }
    for half in [
        "d/dγ -1/m*(D*γ - L*v)/sqrt(",
        "∂/∂γ (-1/m (Dγ - Lv)/√(",
        "d/dx 1/sqrt(",
        "d/dx sqrt()",
        // these read `0` (an empty call taken for a constant)
        "d/dx ln(",
        "d/dx abs(",
        "d²/dx² sqrt(",
        "∂/∂x ln(",
        // the time derivative showed `ṙ cos()` / `ẋ sin()`
        "d/dt (r cos(",
        "d/dt (x sin(",
    ] {
        assert_eq!(shown(half), "", "{half}");
    }
    assert_eq!(
        shown("d/dγ -1/m*(D*γ - L*v)/sqrt(γ^2 + v^2"),
        shown("d/dγ -1/m*(D*γ - L*v)/sqrt(γ^2 + v^2)")
    );
}

#[test]
fn a_negative_derivative_term_reads_minus() {
    assert_eq!(shown("d/dx x cos(x)"), "cos(x) - πx sin(x)/180");
    assert_eq!(
        shown("∂^2/∂x ∂y sin(x y)"),
        "π cos(xy)/180 - π²xy sin(xy)/32400"
    );
    assert_eq!(shown("d/dx -x^3 + x"), "1 - 3x²");
    assert_eq!(shown("d/dx x^2 - 3x"), "2x - 3");
}
