//! ∫ to ∞ of integrands that keep returning to zero: the double exponential rules can't settle them, so they are
//! summed panel by panel between zeros (QA item 9: `integral of sin(x)^2/x^2 from 0 to inf` was blank).
use qcalc_core::calculus::numeric::integrate;
use std::f64::consts::PI;

fn close(f: &dyn Fn(f64) -> f64, a: f64, b: f64, want: f64, digits: f64) {
    let e = integrate(f, a, b).unwrap_or_else(|| panic!("blank for ∫ {a}..{b}, want {want}"));
    let rel = ((e.value - want) / want).abs();
    assert!(
        rel < 10f64.powf(-digits),
        "∫ {a}..{b} = {} (err {}), want {want}",
        e.value,
        e.err
    );
    assert!(
        (e.value - want).abs() <= e.err.max(1e-15),
        "err {} doesn't cover the gap to {want}",
        e.err
    );
}

#[test]
fn oscillating_tails_converge() {
    let inf = f64::INFINITY;
    close(&|x: f64| x.sin().powi(2) / (x * x), 0.0, inf, PI / 2.0, 8.0);
    close(&|x: f64| x.sin() / x, 0.0, inf, PI / 2.0, 8.0);
    close(&|x: f64| x.sin() / x, -inf, inf, PI, 8.0);
    close(&|x: f64| x.sin() / x, -inf, 0.0, PI / 2.0, 8.0);
    close(&|x: f64| (x * x).sin(), 0.0, inf, (PI / 8.0).sqrt(), 6.0);
    close(
        &|x: f64| x.sin() / x.sqrt(),
        0.0,
        inf,
        (PI / 2.0).sqrt(),
        6.0,
    );
    close(
        &|x: f64| x.sin().powi(3) / x.powi(3),
        0.0,
        inf,
        3.0 * PI / 8.0,
        8.0,
    );
}

#[test]
fn divergent_tails_stay_blank() {
    let inf = f64::INFINITY;
    assert!(integrate(&|x: f64| x.sin(), 0.0, inf).is_none());
    assert!(integrate(&|x: f64| x * x.sin(), 0.0, inf).is_none());
    assert!(integrate(&|x: f64| x.sin().abs() / x, 1.0, inf).is_none());
    assert!(integrate(&|x: f64| x.sin().powi(2) / x, 1.0, inf).is_none());
    assert!(integrate(&|x: f64| x.cos() / x, 0.0, inf).is_none());
}
