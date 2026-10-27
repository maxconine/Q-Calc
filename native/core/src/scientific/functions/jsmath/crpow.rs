//! `pow` rounded correctly (to nearest, ties to even), the same on every platform.
//!
//! V8's `Math.pow` is the C library's `pow`, so the TS engine's answers carry whatever pow the machine that
//! recorded them had (Apple's). Each C library rounds the odd hard case its own way (the MSVC UCRT's and Apple's
//! disagree about one input in two thousand; fdlibm's, in the `libm` crate, about one in ten), so the native
//! engine can't call the platform's and stay platform independent. The correctly rounded answer is the one a
//! good libm gives nearly always, so this computes x^y = exp(y·ln x) in double-double (about 2^-95 relative)
//! and rounds that once. An answer within 2^-36 of an ulp of a double or of a midpoint is taken to be exactly
//! that, which is what an exact power (2^-1075, 262143^3) is; a pow that isn't exact lands that close about
//! once in 2^35.
//!
//! Only the core `x > 0`, both finite, is here; `pow` handles zeros, infinities, NaN and negative bases.

/// A double-double: `hi + lo`, `|lo| <= ulp(hi) / 2`.
#[derive(Clone, Copy, Debug)]
struct Dd {
    hi: f64,
    lo: f64,
}

const fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    let lo = (a - (s - bb)) + (b - bb);
    Dd { hi: s, lo }
}

const fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd {
        hi: s,
        lo: b - (s - a),
    }
}

/// Dekker's split: `a = hi + lo`, each half 26 bits, for `|a| < 2^996`.
const fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a;
    let hi = c - (c - a);
    (hi, a - hi)
}

/// `a * b` exactly as a double-double (Dekker, no fma: the same on every target).
const fn two_prod(a: f64, b: f64) -> Dd {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let lo = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    Dd { hi: p, lo }
}

const fn add(a: Dd, b: Dd) -> Dd {
    let s = two_sum(a.hi, b.hi);
    let t = two_sum(a.lo, b.lo);
    let s = quick_two_sum(s.hi, s.lo + t.hi);
    quick_two_sum(s.hi, s.lo + t.lo)
}

const fn add_d(a: Dd, b: f64) -> Dd {
    let s = two_sum(a.hi, b);
    quick_two_sum(s.hi, s.lo + a.lo)
}

const fn mul(a: Dd, b: Dd) -> Dd {
    let p = two_prod(a.hi, b.hi);
    quick_two_sum(p.hi, p.lo + (a.hi * b.lo + a.lo * b.hi))
}

const fn mul_d(a: Dd, b: f64) -> Dd {
    let p = two_prod(a.hi, b);
    quick_two_sum(p.hi, p.lo + a.lo * b)
}

const fn div_d(a: Dd, b: f64) -> Dd {
    let q = a.hi / b;
    let p = two_prod(q, b);
    let r = ((a.hi - p.hi) - p.lo + a.lo) / b;
    quick_two_sum(q, r)
}

const fn scale(a: Dd, s: f64) -> Dd {
    Dd {
        hi: a.hi * s,
        lo: a.lo * s,
    }
}

const ONE: Dd = Dd { hi: 1.0, lo: 0.0 };

/// 1/k! for k = 2..=10, as double-doubles.
const INV_FACT: [Dd; 9] = {
    let mut out = [ONE; 9];
    let mut f = 1.0;
    let mut k = 2;
    while k <= 10 {
        f *= k as f64; // exact through 10!
        out[k - 2] = div_d(ONE, f);
        k += 1;
    }
    out
};

/// ln 2 as a double-double.
const LN2: Dd = Dd {
    hi: f64::from_bits(0x3FE6_2E42_FEFA_39EF),
    lo: f64::from_bits(0x3C7A_BC9E_3B39_803F),
};

/// e^r - 1 for `|r| <= 0.35`: Taylor at r / 2^10, then (1+e)^2 - 1 = 2e + e² ten times. Relative to the
/// answer, so it keeps its digits near 0.
fn expm1_small(r: Dd) -> Dd {
    let r = scale(r, 1.0 / 1024.0);
    let mut p = INV_FACT[8];
    for c in INV_FACT[..8].iter().rev() {
        p = add(*c, mul(r, p));
    }
    let mut e = add(r, mul(r, mul(r, p)));
    for _ in 0..10 {
        e = add(scale(e, 2.0), mul(e, e));
    }
    e
}

/// ln x for normal or subnormal `x > 0`: x = m·2^k with m in [√½, √2), one Newton step on fdlibm's ln m.
fn ln(x: f64) -> Dd {
    let (mut x, mut k) = (x, 0i32);
    if x < f64::MIN_POSITIVE {
        x *= f64::from_bits(0x4350_0000_0000_0000); // 2^54
        k -= 54;
    }
    let bits = x.to_bits();
    k += ((bits >> 52) as i32) - 1023;
    let mut m = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    if m >= std::f64::consts::SQRT_2 {
        m *= 0.5;
        k += 1;
    }
    let l0 = libm::log(m);
    // u = m·e^-l0 = 1 + δ; ln m = l0 + ln(1 + δ) = l0 + δ - δ²/2
    let em1 = expm1_small(Dd { hi: -l0, lo: 0.0 });
    let delta = add(two_sum(m - 1.0, 0.0), mul_d(em1, m)); // m - 1 is exact (Sterbenz)
    let lnm = add_d(add_d(delta, -0.5 * delta.hi * delta.hi), l0);
    if k == 0 {
        return lnm;
    }
    let kf = f64::from(k);
    add(add(two_prod(kf, LN2.hi), two_prod(kf, LN2.lo)), lnm)
}

/// `x^y` for finite `x > 0` and finite `y`, correctly rounded.
pub(super) fn pow_pos(x: f64, y: f64) -> f64 {
    if x == 1.0 || y == 0.0 {
        return 1.0;
    }
    let l = ln(x);
    let p = y * l.hi;
    if p > 710.0 {
        return f64::INFINITY;
    }
    if p < -746.0 {
        return 0.0;
    }
    let t = add(two_prod(y, l.hi), Dd { hi: y * l.lo, lo: 0.0 });
    // e^t = 2^n·e^r, |r| <= ln2/2
    let n = (t.hi / LN2.hi).round();
    let r = add(t, scale(add(two_prod(n, LN2.hi), two_prod(n, LN2.lo)), -1.0));
    let v = add_d(expm1_small(r), 1.0);
    round_scaled(v, n as i32)
}

/// `v·2^n` rounded once, `v` near 1.
fn round_scaled(v: Dd, n: i32) -> f64 {
    let (hi, mut lo) = (v.hi, v.lo);
    // the neighbour of hi on lo's side, and the gap to it
    let toward = if lo < 0.0 {
        f64::from_bits(hi.to_bits() - 1)
    } else {
        f64::from_bits(hi.to_bits() + 1)
    };
    let gap = (toward - hi).abs();
    let snap = gap * f64::from_bits(0x3DB0_0000_0000_0000); // 2^-36
    let mut hi = hi;
    if lo.abs() <= snap {
        lo = 0.0; // exactly hi
    } else if (lo.abs() - gap / 2.0).abs() <= snap {
        lo = (gap / 2.0).copysign(lo); // exactly the midpoint
        if hi.to_bits() & 1 == 1 {
            // ties to even, in the double range; the subnormal case re-rounds below
            if n >= -1021 {
                hi = toward;
                lo = -lo;
            }
        } else if n >= -1021 {
            lo = 0.0;
        }
    }
    if n >= -1021 {
        if n > 1023 {
            return (hi * 2.0) * 2f64.powi(n - 1);
        }
        return hi * 2f64.powi(n);
    }
    // subnormal: hi rounds once on the way down; lo decides only when hi lands on a midpoint
    let up = 2f64.powi(600);
    let r = (hi * 2f64.powi(n + 600)) / up; // the first step is exact, the second rounds
    let back = (r * up) * 2f64.powi(-n - 600); // r·2^-n, exact
    let d = hi - back;
    let half = 2f64.powi(-1075 - n); // half a subnormal ulp, in hi's units
    if d.abs() == half && lo != 0.0 {
        if (d > 0.0) == (lo > 0.0) {
            let tiny = f64::from_bits(1);
            return if d > 0.0 { r + tiny } else { r - tiny };
        }
        return r;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::pow_pos;

    #[test]
    fn exact_and_rounded() {
        assert_eq!(pow_pos(2.0, 10.0), 1024.0);
        assert_eq!(pow_pos(4.0, 1.5), 8.0);
        assert_eq!(pow_pos(10.0, -2.0), 0.01);
        assert_eq!(pow_pos(2.0, -1074.0), f64::from_bits(1));
        assert_eq!(pow_pos(2.0, -1075.0), 0.0); // a tie, to even
        assert_eq!(pow_pos(2.0, 1023.0), 2f64.powi(1023));
        assert_eq!(pow_pos(2.0, 1024.0), f64::INFINITY);
        assert_eq!(pow_pos(1e308, -0.5), 1e-154);
        assert_eq!(pow_pos(15.0, -0.5), 0.25819888974716115);
        assert_eq!(pow_pos(2.0, 1.5), 2.8284271247461903);
        assert_eq!(pow_pos(2.0, -0.5), 0.7071067811865476);
        // 262143³ = 2^54 - 3·2^36 + 3·2^18 - 1 needs 54 bits: a midpoint, which rounds to even
        assert_eq!(pow_pos(262_143.0, 3.0), 18_014_192_351_838_208.0);
    }
}
