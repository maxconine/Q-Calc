//! Fraction.js 5.3 (the `Fraction` type mathjs's `simplifyConstant` folds constants in), only what the symbolic
//! layer reaches: construction from a double (its Farey search, which is not the exact binary value), the four
//! operations, `pow`, `neg`, `abs` and `valueOf`.

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// A reduced fraction `s * n / d` with `n >= 0`, `d > 0` and `s = ±1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fraction {
    pub s: i8,
    pub n: BigInt,
    pub d: BigInt,
}

/// Fraction.js errors ("Division by Zero", "Invalid argument").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FractionError(pub &'static str);

/// `Number(bigint)`: the nearest double.
pub fn big_to_f64(b: &BigInt) -> f64 {
    b.to_string().parse::<f64>().unwrap_or(f64::NAN)
}

impl Fraction {
    /// `newFraction(n, d)`: sign from `n`, reduced; `d` must be positive (an error when zero).
    pub fn new(n: BigInt, d: BigInt) -> Result<Fraction, FractionError> {
        if d.is_zero() {
            return Err(FractionError("Division by Zero"));
        }
        let s: i8 = if n.is_negative() { -1 } else { 1 };
        let n = n.abs();
        let a = gcd(&n, &d);
        Ok(Fraction {
            s,
            n: &n / &a,
            d: &d / &a,
        })
    }

    pub fn from_int(v: i64) -> Fraction {
        Fraction::new(BigInt::from(v), BigInt::one()).expect("nonzero denominator")
    }

    fn signed_n(&self) -> BigInt {
        if self.s < 0 { -&self.n } else { self.n.clone() }
    }

    /// `new Fraction(x)` for a double: integers exactly, anything else by Fraction.js's Farey search
    /// (denominators up to 10^7, after scaling `x >= 1` into [0, 1) by a power of ten).
    pub fn from_f64(x: f64) -> Result<Fraction, FractionError> {
        if x.is_nan() {
            return Err(FractionError("Invalid argument"));
        }
        let mut p1 = x;
        let mut s: i8 = 1;
        if p1 < 0.0 {
            s = -1;
            p1 = -p1;
        }
        if !p1.is_finite() {
            // `BigInt(Infinity)` throws a RangeError
            return Err(FractionError("Invalid argument"));
        }
        let (n, d) = if p1 % 1.0 == 0.0 {
            (int_from_f64(p1), BigInt::one())
        } else {
            let mut z = 1.0f64;
            let (mut a, mut b, mut c, mut dd) = (0.0f64, 1.0f64, 1.0f64, 1.0f64);
            let big_n = 10_000_000.0f64;
            let (mut n, mut d) = (0.0f64, 1.0f64);
            if p1 >= 1.0 {
                let k = (1.0 + crate::scientific::functions::jsmath::log10(p1)).floor() as i32;
                z = format!("1e{k}").parse::<f64>().unwrap_or(f64::INFINITY);
                p1 /= z;
            }
            while b <= big_n && dd <= big_n {
                let m = (a + c) / (b + dd);
                if p1 == m {
                    if b + dd <= big_n {
                        n = a + c;
                        d = b + dd;
                    } else if dd > b {
                        n = c;
                        d = dd;
                    } else {
                        n = a;
                        d = b;
                    }
                    break;
                } else {
                    if p1 > m {
                        a += c;
                        b += dd;
                    } else {
                        c += a;
                        dd += b;
                    }
                    if b > big_n {
                        n = c;
                        d = dd;
                    } else {
                        n = a;
                        d = b;
                    }
                }
            }
            (int_from_f64(n) * int_from_f64(z), int_from_f64(d))
        };
        let n = if s < 0 { -n } else { n };
        Fraction::new(n, d)
    }

    /// `valueOf()`: `Number(s * n) / Number(d)`.
    pub fn value(&self) -> f64 {
        big_to_f64(&self.signed_n()) / big_to_f64(&self.d)
    }

    pub fn neg(&self) -> Fraction {
        Fraction::new(-self.signed_n(), self.d.clone()).expect("nonzero denominator")
    }

    pub fn abs(&self) -> Fraction {
        Fraction::new(self.n.clone(), self.d.clone()).expect("nonzero denominator")
    }

    pub fn add(&self, o: &Fraction) -> Fraction {
        Fraction::new(
            self.signed_n() * &o.d + o.signed_n() * &self.d,
            &self.d * &o.d,
        )
        .expect("nonzero denominator")
    }

    pub fn sub(&self, o: &Fraction) -> Fraction {
        Fraction::new(
            self.signed_n() * &o.d - o.signed_n() * &self.d,
            &self.d * &o.d,
        )
        .expect("nonzero denominator")
    }

    pub fn mul(&self, o: &Fraction) -> Fraction {
        Fraction::new(self.signed_n() * o.signed_n(), &self.d * &o.d).expect("nonzero denominator")
    }

    pub fn div(&self, o: &Fraction) -> Result<Fraction, FractionError> {
        Fraction::new(BigInt::from(self.s * o.s) * &self.n * &o.d, &self.d * &o.n)
    }

    /// `pow(b)`: exact when the result is rational, `Ok(None)` when it isn't (a negative base under a
    /// fractional exponent, or a root that doesn't come out whole). Exponents too large to compute are an
    /// error here (the TS would run out of memory).
    pub fn pow(&self, e: &Fraction) -> Result<Option<Fraction>, FractionError> {
        let exp =
            e.n.to_u32()
                .filter(|x| *x <= 100_000)
                .ok_or(FractionError("exponent too large"))?;
        if e.d.is_one() {
            if e.s < 0 {
                return Fraction::new((BigInt::from(self.s) * &self.d).pow(exp), self.n.pow(exp))
                    .map(Some);
            }
            return Fraction::new(self.signed_n().pow(exp), self.d.pow(exp)).map(Some);
        }
        if self.s < 0 {
            return Ok(None);
        }
        let root = e.d.to_u64().ok_or(FractionError("exponent too large"))?;
        let mut n = BigInt::one();
        let mut d = BigInt::one();
        let nf = factorize(&self.n);
        for (p, k) in &nf {
            if p.is_one() {
                continue;
            }
            if p.is_zero() {
                n = BigInt::zero();
                break;
            }
            let k = k * u64::from(exp);
            if k % root != 0 {
                return Ok(None);
            }
            n *= p.pow((k / root) as u32);
        }
        for (p, k) in &factorize(&self.d) {
            if p.is_one() {
                continue;
            }
            let k = k * u64::from(exp);
            if k % root != 0 {
                return Ok(None);
            }
            d *= p.pow((k / root) as u32);
        }
        if e.s < 0 {
            return Fraction::new(d, n).map(Some);
        }
        Fraction::new(n, d).map(Some)
    }
}

fn int_from_f64(x: f64) -> BigInt {
    // exact for any integral double
    let text = format!("{x:.0}");
    text.parse::<BigInt>()
        .unwrap_or_else(|_| BigInt::from_biguint(Sign::NoSign, Default::default()))
}

fn gcd(a: &BigInt, b: &BigInt) -> BigInt {
    if a.is_zero() {
        return b.clone();
    }
    if b.is_zero() {
        return a.clone();
    }
    a.gcd(b)
}

/// Fraction.js `factorize`: prime → multiplicity, in the order JS would enumerate the keys (integer keys
/// ascending; `n <= 1` maps itself to one).
fn factorize(n: &BigInt) -> Vec<(BigInt, u64)> {
    let mut out: Vec<(BigInt, u64)> = Vec::new();
    if *n <= BigInt::one() {
        out.push((n.clone(), 1));
        return out;
    }
    let mut n = n.clone();
    let add = |p: &BigInt, out: &mut Vec<(BigInt, u64)>| {
        if let Some(e) = out.iter_mut().find(|(q, _)| q == p) {
            e.1 += 1;
        } else {
            out.push((p.clone(), 1));
        }
    };
    for small in [2u32, 3, 5] {
        let p = BigInt::from(small);
        while (&n % &p).is_zero() {
            add(&p, &mut out);
            n /= &p;
        }
    }
    const STEPS: [u32; 8] = [4, 2, 4, 2, 4, 6, 2, 6];
    let mut p = BigInt::from(7);
    let mut si = 0;
    while &p * &p <= n {
        while (&n % &p).is_zero() {
            add(&p, &mut out);
            n /= &p;
        }
        p += STEPS[si];
        si = (si + 1) & 7;
    }
    if n > BigInt::one() {
        add(&n, &mut out);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farey_matches_fraction_js() {
        let f = Fraction::from_f64(0.1).unwrap();
        assert_eq!((f.n.to_i64(), f.d.to_i64()), (Some(1), Some(10)));
        let pi = Fraction::from_f64(std::f64::consts::PI).unwrap();
        assert_eq!(
            (pi.n.to_i64(), pi.d.to_i64()),
            (Some(11985110), Some(3814979))
        );
        let ln2 = Fraction::from_f64(std::f64::consts::LN_2).unwrap();
        assert_ne!(ln2.value(), std::f64::consts::LN_2);
    }

    #[test]
    fn pow_roots() {
        let four = Fraction::from_int(4);
        let half = Fraction::new(BigInt::from(1), BigInt::from(2)).unwrap();
        assert_eq!(four.pow(&half).unwrap(), Some(Fraction::from_int(2)));
        assert_eq!(Fraction::from_int(2).pow(&half).unwrap(), None);
        let inv = Fraction::from_int(-1);
        assert_eq!(four.pow(&inv).unwrap().map(|f| f.value()), Some(0.25));
    }
}
