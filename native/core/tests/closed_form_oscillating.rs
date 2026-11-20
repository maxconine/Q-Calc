//! QA backlog item 49: ∫ to ∞ of a decaying wave had a decimal answer only (items 9, 40), as the closed-form
//! search's double exponential rule can't settle it. It is now summed between zeros at high precision with Levin.
use qcalc_core::calculus::closed_form::{DEFAULT_TICKS, Job, identify_job};

fn sin_over(den: &str) -> String {
    format!(r#"{{"t":"op","op":"/","a":[{{"t":"fn","name":"sin","a":[{{"t":"var"}}]}},{den}]}}"#)
}

fn find(f: &str, a: &str, b: &str, approx: f64) -> Option<String> {
    let text = format!(
        r#"{{"kind":"integral","f":{f},"a":{a},"b":{b},"approx":{approx},"err":1e-9,"rad":true}}"#
    );
    let job = Job::from_json(&serde_json::from_str(&text).unwrap()).unwrap();
    identify_job(&job, DEFAULT_TICKS)
}

const X: &str = r#"{"t":"var"}"#;
const ZERO: &str = r#"{"t":"num","v":"0"}"#;

#[test]
fn sin_x_over_x_is_half_pi() {
    let pi = std::f64::consts::PI;
    assert_eq!(
        find(&sin_over(X), ZERO, r#""inf""#, pi / 2.0).as_deref(),
        Some("π/2")
    );
    assert_eq!(
        find(&sin_over(X), r#""-inf""#, ZERO, pi / 2.0).as_deref(),
        Some("π/2")
    );
    assert_eq!(
        find(&sin_over(X), r#""-inf""#, r#""inf""#, pi).as_deref(),
        Some("π")
    );
}

fn pow(base: &str, e: &str) -> String {
    format!(r#"{{"t":"op","op":"^","a":[{base},{{"t":"num","v":"{e}"}}]}}"#)
}

fn sin_of(arg: &str) -> String {
    format!(r#"{{"t":"fn","name":"sin","a":[{arg}]}}"#)
}

fn over(n: &str, d: &str) -> String {
    format!(r#"{{"t":"op","op":"/","a":[{n},{d}]}}"#)
}

#[test]
fn other_waves() {
    let pi = std::f64::consts::PI;
    let inf = r#""inf""#;
    // sin²x/x² only touches zero: one-sign panels
    let f = over(&pow(&sin_of(X), "2"), &pow(X, "2"));
    assert_eq!(find(&f, ZERO, inf, pi / 2.0).as_deref(), Some("π/2"));
    let f = over(&pow(&sin_of(X), "3"), &pow(X, "3"));
    assert_eq!(find(&f, ZERO, inf, 3.0 * pi / 8.0).as_deref(), Some("3π/8"));
    // Fresnel
    let f = sin_of(&pow(X, "2"));
    assert_eq!(
        find(&f, ZERO, inf, (pi / 8.0).sqrt()).as_deref(),
        Some("√(2π)/4")
    );
    let f = over(&sin_of(X), &pow(X, "0.5"));
    assert_eq!(
        find(&f, ZERO, inf, (pi / 2.0).sqrt()).as_deref(),
        Some("√(2π)/2")
    );
    // degrees: sin(x°)/x is π/2 still; sin²(x°)/x² is π²/360
    let deg = r#"{"t":"op","op":"*","a":[{"t":"var"},{"t":"op","op":"/","a":[{"t":"const","name":"pi"},{"t":"num","v":"180"}]}]}"#;
    let f = over(&sin_of(deg), X);
    assert_eq!(find(&f, ZERO, inf, pi / 2.0).as_deref(), Some("π/2"));
    let f = over(&pow(&sin_of(deg), "2"), &pow(X, "2"));
    assert_eq!(
        find(&f, ZERO, inf, pi * pi / 360.0).as_deref(),
        Some("π²/360")
    );
    // a double answer that disagrees is not this number
    assert_eq!(find(&sin_over(X), ZERO, inf, 1.6), None);
}
