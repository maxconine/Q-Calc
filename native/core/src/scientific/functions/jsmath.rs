//! JS `Math.*` over doubles, bit for bit as V8 14.6 (Node 26) computes them.
//!
//! V8 doesn't use the platform libm: `src/base/ieee754.cc` is its own port of fdlibm, and that is what the TS
//! engine's answers were recorded with. The platform's libm (and the `libm` crate, which is musl's newer
//! kernels and CORE-MATH's cbrt) round about one input in twenty differently by an ulp, which shows when a
//! later step snaps or cancels. So these are ports of V8's functions, statement for statement.
//!
//! `pow` is V8's own cases around a correctly rounded core (`crpow.rs`): V8 leaves the rest to the platform's
//! C library, which rounds differently on each. Arguments past 2^19·π/2 (which need the multi-word reduction,
//! `__kernel_rem_pio2`) fall back to the `libm` crate's reduction.

// fdlibm's constants are its own digits (not std's), and `x - x` is how it makes a NaN
#![allow(
    clippy::excessive_precision,
    clippy::many_single_char_names,
    clippy::approx_constant,
    clippy::eq_op,
    clippy::unreadable_literal
)]

mod crpow;

/// `a*b + c` as clang contracts it on arm64 (`-ffp-contract=on`): one rounding, as `llvm.fmuladd` lowers there.
/// `libm::fma` rather than `f64::mul_add`, which on a target without an fma instruction (x86-64 Windows) calls
/// the C runtime's `fma`.
fn ma(a: f64, b: f64, c: f64) -> f64 {
    libm::fma(a, b, c)
}

/// `Math.pow` and `**`: V8's `math::pow` with `--use-std-math-pow` (on by default), which is the C library's
/// `pow` plus the cases its optimizing compilers inline: `x ** 2` is `x * x` and `x ** 0.5` is `sqrt(x + 0)`.
/// ±1 to an infinite power and anything to NaN are NaN. The C library's pow differs by platform in the odd last
/// place, so the rest is C99's special cases around a correctly rounded core (`crpow`), which is what Apple's
/// and the UCRT's pow answer nearly always.
pub fn pow(x: f64, y: f64) -> f64 {
    if y.is_nan() || (y.is_infinite() && (x == 1.0 || x == -1.0)) {
        return f64::NAN;
    }
    if y == 2.0 {
        return x * x;
    }
    if y == 0.5 {
        return if x.is_infinite() {
            f64::INFINITY
        } else {
            (x + 0.0).sqrt()
        };
    }
    c_pow(x, y)
}

/// C99 `pow` for a non-NaN `y`, rounded correctly.
fn c_pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x.is_nan() {
        return f64::NAN;
    }
    if y.is_infinite() {
        let ax = x.abs();
        return if ax == 1.0 {
            1.0
        } else if (ax > 1.0) == (y > 0.0) {
            f64::INFINITY
        } else {
            0.0
        };
    }
    let y_int = y == y.trunc();
    // every double past 2^53 is even
    let y_odd = y_int && y.abs() < 9_007_199_254_740_992.0 && (y % 2.0).abs() == 1.0;
    if x == 0.0 {
        let r = if y < 0.0 { f64::INFINITY } else { 0.0 };
        return if y_odd { r.copysign(x) } else { r };
    }
    if x.is_infinite() {
        let r = if y < 0.0 { 0.0 } else { f64::INFINITY };
        return if y_odd && x < 0.0 { -r } else { r };
    }
    if x < 0.0 {
        if !y_int {
            return f64::NAN;
        }
        let r = crpow::pow_pos(-x, y);
        return if y_odd { -r } else { r };
    }
    crpow::pow_pos(x, y)
}

fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}

fn with_hi(x: f64, h: i32) -> f64 {
    f64::from_bits((u64::from(h as u32) << 32) | u64::from(lo(x)))
}

fn with_lo(x: f64, l: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF_0000_0000) | u64::from(l))
}

fn words(h: i32, l: u32) -> f64 {
    f64::from_bits((u64::from(h as u32) << 32) | u64::from(l))
}

// ---- trig ----

/// `__ieee754_rem_pio2` for |x| up to 2^19·π/2: n and x - n·π/2 as y0 + y1. None for larger arguments.
fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
    const NPIO2_HW: [i32; 32] = [
        0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB,
        0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB,
        0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C,
        0x4041475C, 0x4042106C, 0x4042D97C, 0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB,
        0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
    ];
    const HALF: f64 = 5.00000000000000000000e-01;
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;
    const PIO2_2: f64 = 6.07710050630396597660e-11;
    const PIO2_2T: f64 = 2.02226624879595063154e-21;
    const PIO2_3: f64 = 2.02226624871116645580e-21;
    const PIO2_3T: f64 = 8.47842766036889956997e-32;

    let hx = hi(x);
    let ix = hx & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return Some((0, x, 0.0));
    }
    if ix < 0x4002D97C {
        if hx > 0 {
            let mut z = x - PIO2_1;
            let (y0, y1);
            if ix != 0x3FF921FB {
                y0 = z - PIO2_1T;
                y1 = (z - y0) - PIO2_1T;
            } else {
                z -= PIO2_2;
                y0 = z - PIO2_2T;
                y1 = (z - y0) - PIO2_2T;
            }
            return Some((1, y0, y1));
        }
        let mut z = x + PIO2_1;
        let (y0, y1);
        if ix != 0x3FF921FB {
            y0 = z + PIO2_1T;
            y1 = (z - y0) + PIO2_1T;
        } else {
            z += PIO2_2;
            y0 = z + PIO2_2T;
            y1 = (z - y0) + PIO2_2T;
        }
        return Some((-1, y0, y1));
    }
    if ix <= 0x413921FB {
        let t = x.abs();
        let n = ma(t, INVPIO2, HALF) as i32;
        let f = f64::from(n);
        let mut r = ma(-f, PIO2_1, t);
        let mut w = f * PIO2_1T;
        let mut y0;
        if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
            y0 = r - w;
        } else {
            let j = ix >> 20;
            y0 = r - w;
            let mut i = j - ((hi(y0) >> 20) & 0x7FF);
            if i > 16 {
                let t = r;
                w = f * PIO2_2;
                r = t - w;
                w = ma(f, PIO2_2T, -((t - r) - w));
                y0 = r - w;
                i = j - ((hi(y0) >> 20) & 0x7FF);
                if i > 49 {
                    let t = r;
                    w = f * PIO2_3;
                    r = t - w;
                    w = ma(f, PIO2_3T, -((t - r) - w));
                    y0 = r - w;
                }
            }
        }
        let y1 = (r - y0) - w;
        return Some(if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) });
    }
    None
}

fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;
    let ix = hi(x) & 0x7FFFFFFF;
    if ix < 0x3E400000 && x as i32 == 0 {
        return 1.0;
    }
    let z = x * x;
    let r = z * ma(z, ma(z, ma(z, ma(z, ma(z, C6, C5), C4), C3), C2), C1);
    if ix < 0x3FD33333 {
        return 1.0 - ma(0.5, z, -ma(z, r, -(x * y)));
    }
    let qx = if ix > 0x3FE90000 {
        0.28125
    } else {
        words(ix - 0x00200000, 0)
    };
    let iz = ma(0.5, z, -qx);
    let a = 1.0 - qx;
    a - (iz - ma(z, r, -(x * y)))
}

fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;
    let ix = hi(x) & 0x7FFFFFFF;
    if ix < 0x3E400000 && x as i32 == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = ma(z, ma(z, ma(z, ma(z, S6, S5), S4), S3), S2);
    if iy == 0 {
        ma(v, ma(z, r, S1), x)
    } else {
        x - ma(-v, S1, ma(z, ma(0.5, y, -(v * r)), -y))
    }
}

fn kernel_tan(mut x: f64, mut y: f64, iy: i32) -> f64 {
    const T: [f64; 13] = [
        3.33333333333334091986e-01,
        1.33333333333201242699e-01,
        5.39682539762260521377e-02,
        2.18694882948595424599e-02,
        8.86323982359930005737e-03,
        3.59207910759131235356e-03,
        1.45620945432529025516e-03,
        5.88041240820264096874e-04,
        2.46463134818469906812e-04,
        7.81794442939557092300e-05,
        7.14072491382608190305e-05,
        -1.85586374855275456654e-05,
        2.59073051863633712884e-05,
    ];
    const PIO4: f64 = 7.85398163397448278999e-01;
    const PIO4LO: f64 = 3.06161699786838301793e-17;
    let hx = hi(x);
    let ix = hx & 0x7FFFFFFF;
    if ix < 0x3E300000 && x as i32 == 0 {
        if ((ix as u32 | lo(x)) | (iy + 1) as u32) == 0 {
            return 1.0 / x.abs();
        }
        if iy == 1 {
            return x;
        }
        // compute -1 / (x+y) carefully
        let w = x + y;
        let z = with_lo(w, 0);
        let v = y - (z - x);
        let a = -1.0 / w;
        let t = with_lo(a, 0);
        let s = ma(t, z, 1.0);
        return ma(a, ma(t, v, s), t);
    }
    if ix >= 0x3FE59428 {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    let mut r = ma(
        w,
        ma(w, ma(w, ma(w, ma(w, T[11], T[9]), T[7]), T[5]), T[3]),
        T[1],
    );
    let v = z * ma(
        w,
        ma(w, ma(w, ma(w, ma(w, T[12], T[10]), T[8]), T[6]), T[4]),
        T[2],
    );
    let s = z * x;
    r = ma(z, ma(s, r + v, y), y);
    r = ma(T[0], s, r);
    let w = x + r;
    if ix >= 0x3FE59428 {
        let v = f64::from(iy);
        return f64::from(1 - ((hx >> 30) & 2)) * ma(-2.0, x - (w * w / (w + v) - r), v);
    }
    if iy == 1 {
        return w;
    }
    let z = with_lo(w, 0);
    let v = r - (z - x);
    let a = -1.0 / w;
    let t = with_lo(a, 0);
    let s = ma(t, z, 1.0);
    ma(a, ma(t, v, s), t)
}

pub fn sin(x: f64) -> f64 {
    let ix = hi(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return kernel_sin(x, 0.0, 0);
    }
    if ix >= 0x7FF00000 {
        return x - x;
    }
    let Some((n, y0, y1)) = rem_pio2(x) else {
        return libm::sin(x);
    };
    match n & 3 {
        0 => kernel_sin(y0, y1, 1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, 1),
        _ => -kernel_cos(y0, y1),
    }
}

pub fn cos(x: f64) -> f64 {
    let ix = hi(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7FF00000 {
        return x - x;
    }
    let Some((n, y0, y1)) = rem_pio2(x) else {
        return libm::cos(x);
    };
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, 1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, 1),
    }
}

pub fn tan(x: f64) -> f64 {
    let ix = hi(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return kernel_tan(x, 0.0, 1);
    }
    if ix >= 0x7FF00000 {
        return x - x;
    }
    let Some((n, y0, y1)) = rem_pio2(x) else {
        return libm::tan(x);
    };
    kernel_tan(y0, y1, 1 - ((n & 1) << 1))
}

// ---- inverse trig ----

const PS0: f64 = 1.66666666666666657415e-01;
const PS1: f64 = -3.25565818622400915405e-01;
const PS2: f64 = 2.01212532134862925881e-01;
const PS3: f64 = -4.00555345006794114027e-02;
const PS4: f64 = 7.91534994289814532176e-04;
const PS5: f64 = 3.47933107596021167570e-05;
const QS1: f64 = -2.40339491173441421878e+00;
const QS2: f64 = 2.02094576023350569471e+00;
const QS3: f64 = -6.88283971605453293030e-01;
const QS4: f64 = 7.70381505559019352791e-02;
const PIO2_HI: f64 = 1.57079632679489655800e+00;
const PIO2_LO: f64 = 6.12323399573676603587e-17;

fn rational(z: f64) -> (f64, f64) {
    let p = z * ma(z, ma(z, ma(z, ma(z, ma(z, PS5, PS4), PS3), PS2), PS1), PS0);
    let q = ma(z, ma(z, ma(z, ma(z, QS4, QS3), QS2), QS1), 1.0);
    (p, q)
}

pub fn acos(x: f64) -> f64 {
    const PI: f64 = 3.14159265358979311600e+00;
    let hx = hi(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x3FF00000 {
        if ((ix - 0x3FF00000) as u32 | lo(x)) == 0 {
            return if hx > 0 { 0.0 } else { ma(2.0, PIO2_LO, PI) };
        }
        return f64::NAN;
    }
    if ix < 0x3FE00000 {
        if ix <= 0x3C600000 {
            return PIO2_HI + PIO2_LO;
        }
        let z = x * x;
        let (p, q) = rational(z);
        let r = p / q;
        return PIO2_HI - (x - ma(-x, r, PIO2_LO));
    }
    if hx < 0 {
        let z = (1.0 + x) * 0.5;
        let (p, q) = rational(z);
        let s = z.sqrt();
        let r = p / q;
        let w = ma(r, s, -PIO2_LO);
        return ma(-2.0, s + w, PI);
    }
    let z = (1.0 - x) * 0.5;
    let s = z.sqrt();
    let df = with_lo(s, 0);
    let c = ma(-df, df, z) / (s + df);
    let (p, q) = rational(z);
    let r = p / q;
    let w = ma(r, s, c);
    2.0 * (df + w)
}

pub fn asin(x: f64) -> f64 {
    const PIO4_HI: f64 = 7.85398163397448278999e-01;
    let hx = hi(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x3FF00000 {
        if ((ix - 0x3FF00000) as u32 | lo(x)) == 0 {
            return ma(x, PIO2_HI, x * PIO2_LO);
        }
        return f64::NAN;
    }
    if ix < 0x3FE00000 {
        if ix < 0x3E400000 {
            return x;
        }
        let t = x * x;
        let (p, q) = rational(t);
        let w = p / q;
        return ma(x, w, x);
    }
    let w = 1.0 - x.abs();
    let t = w * 0.5;
    let (p, q) = rational(t);
    let s = t.sqrt();
    let t = if ix >= 0x3FEF3333 {
        let w = p / q;
        PIO2_HI - ma(2.0, ma(s, w, s), -PIO2_LO)
    } else {
        let w = with_lo(s, 0);
        let c = ma(-w, w, t) / (s + w);
        let r = p / q;
        let p = ma(2.0 * s, r, -ma(-2.0, c, PIO2_LO));
        let q = ma(-2.0, w, PIO4_HI);
        PIO4_HI - (p - q)
    };
    if hx > 0 { t } else { -t }
}

pub fn atan(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01,
        7.85398163397448278999e-01,
        9.82793723247329054082e-01,
        1.57079632679489655800e+00,
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17,
        3.06161699786838301793e-17,
        1.39033110312309984516e-17,
        6.12323399573676603587e-17,
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];
    let hx = hi(x);
    let ix = hx & 0x7FFFFFFF;
    let mut x = x;
    if ix >= 0x44100000 {
        if x.is_nan() {
            return x + x;
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    let id: i32;
    if ix < 0x3FDC0000 {
        if ix < 0x3E400000 {
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3FF30000 {
            if ix < 0x3FE60000 {
                id = 0;
                x = ma(2.0, x, -1.0) / (2.0 + x);
            } else {
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x40038000 {
            id = 2;
            x = (x - 1.5) / ma(1.5, x, 1.0);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * ma(
        w,
        ma(w, ma(w, ma(w, ma(w, AT[10], AT[8]), AT[6]), AT[4]), AT[2]),
        AT[0],
    );
    let s2 = w * ma(w, ma(w, ma(w, ma(w, AT[9], AT[7]), AT[5]), AT[3]), AT[1]);
    if id < 0 {
        return ma(-x, s1 + s2, x);
    }
    let id = id as usize;
    let z = ATANHI[id] - (ma(x, s1 + s2, -ATANLO[id]) - x);
    if hx < 0 { -z } else { z }
}

pub fn atan2(y: f64, x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    const PI_O_4: f64 = 7.8539816339744827900E-01;
    const PI_O_2: f64 = 1.5707963267948965580E+00;
    const PI: f64 = 3.1415926535897931160E+00;
    const PI_LO: f64 = 1.2246467991473531772E-16;
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let (hx, lx) = (hi(x), lo(x));
    let ix = hx & 0x7FFFFFFF;
    let (hy, ly) = (hi(y), lo(y));
    let iy = hy & 0x7FFFFFFF;
    if (hx.wrapping_sub(0x3FF00000) as u32 | lx) == 0 {
        return atan(y);
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if (iy as u32 | ly) == 0 {
        return match m {
            0 | 1 => y,
            2 => PI + TINY,
            _ => -PI - TINY,
        };
    }
    if (ix as u32 | lx) == 0 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }
    if ix == 0x7FF00000 {
        if iy == 0x7FF00000 {
            return match m {
                0 => PI_O_4 + TINY,
                1 => -PI_O_4 - TINY,
                2 => ma(3.0, PI_O_4, TINY),
                _ => ma(-3.0, PI_O_4, -TINY),
            };
        }
        return match m {
            0 => 0.0,
            1 => -0.0,
            2 => PI + TINY,
            _ => -PI - TINY,
        };
    }
    if iy == 0x7FF00000 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        m &= 1;
        ma(0.5, PI_LO, PI_O_2)
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

// ---- exponentials and logarithms ----

pub fn exp(x: f64) -> f64 {
    const HALF: [f64; 2] = [0.5, -0.5];
    const O_THRESHOLD: f64 = 7.09782712893383973096e+02;
    const U_THRESHOLD: f64 = -7.45133219101941108420e+02;
    const LN2HI: [f64; 2] = [6.93147180369123816490e-01, -6.93147180369123816490e-01];
    const LN2LO: [f64; 2] = [1.90821492927058770002e-10, -1.90821492927058770002e-10];
    const INVLN2: f64 = 1.44269504088896338700e+00;
    const P1: f64 = 1.66666666666666019037e-01;
    const P2: f64 = -2.77777777770155933842e-03;
    const P3: f64 = 6.61375632143793436117e-05;
    const P4: f64 = -1.65339022054652515390e-06;
    const P5: f64 = 4.13813679705723846039e-08;
    const E: f64 = 2.718281828459045;
    const HUGE: f64 = 1.0e+300;
    const TWOM1000: f64 = 9.33263618503218878990e-302;
    const TWO1023: f64 = 8.988465674311579539e307;

    let mut x = x;
    let mut hx = hi(x) as u32;
    let xsb = ((hx >> 31) & 1) as usize;
    hx &= 0x7FFFFFFF;
    if hx >= 0x40862E42 {
        if hx >= 0x7FF00000 {
            if ((hx & 0xFFFFF) | lo(x)) != 0 {
                return x + x;
            }
            return if xsb == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE;
        }
        if x < U_THRESHOLD {
            return TWOM1000 * TWOM1000;
        }
    }
    let (mut hi_part, mut lo_part, mut k) = (0.0, 0.0, 0i32);
    if hx > 0x3FD62E42 {
        if hx < 0x3FF0A2B2 {
            if x == 1.0 {
                return E;
            }
            hi_part = x - LN2HI[xsb];
            lo_part = LN2LO[xsb];
            k = 1 - xsb as i32 - xsb as i32;
        } else {
            k = ma(INVLN2, x, HALF[xsb]) as i32;
            let t = f64::from(k);
            hi_part = ma(-t, LN2HI[0], x);
            lo_part = t * LN2LO[0];
        }
        x = hi_part - lo_part;
    } else if hx < 0x3E300000 {
        return 1.0 + x;
    }
    let t = x * x;
    let twopk = if k >= -1021 {
        words(0x3FF00000 + ((k as u32) << 20) as i32, 0)
    } else {
        words(0x3FF00000 + (((k + 1000) as u32) << 20) as i32, 0)
    };
    let c = ma(-t, ma(t, ma(t, ma(t, ma(t, P5, P4), P3), P2), P1), x);
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo_part - (x * c) / (2.0 - c)) - hi_part);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * TWO1023;
        }
        y * twopk
    } else {
        y * twopk * TWOM1000
    }
}

pub fn ln(x: f64) -> f64 {
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;
    const TWO54: f64 = 1.80143985094819840000e+16;
    const LG1: f64 = 6.666666666666735130e-01;
    const LG2: f64 = 3.999999999940941908e-01;
    const LG3: f64 = 2.857142874366239149e-01;
    const LG4: f64 = 2.222219843214978396e-01;
    const LG5: f64 = 1.818357216161805012e-01;
    const LG6: f64 = 1.531383769920937332e-01;
    const LG7: f64 = 1.479819860511658591e-01;
    let mut x = x;
    let mut hx = hi(x);
    let lx = lo(x);
    let mut k = 0i32;
    if hx < 0x00100000 {
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let i = (hx + 0x95F64) & 0x100000;
    x = with_hi(x, hx | (i ^ 0x3FF00000));
    k += i >> 20;
    let f = x - 1.0;
    if (0x000FFFFF & (2 + hx)) < 3 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return ma(dk, LN2_HI, dk * LN2_LO);
        }
        let r = f * f * ma(-0.33333333333333333, f, 0.5);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return ma(dk, LN2_HI, -(ma(-dk, LN2_LO, r) - f));
    }
    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    let mut i = hx - 0x6147A;
    let w = z * z;
    let j = 0x6B851 - hx;
    let t1 = w * ma(w, ma(w, LG6, LG4), LG2);
    let t2 = z * ma(w, ma(w, ma(w, LG7, LG5), LG3), LG1);
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - ma(-s, hfsq + r, hfsq)
        } else {
            ma(dk, LN2_HI, -((hfsq - ma(s, hfsq + r, dk * LN2_LO)) - f))
        }
    } else if k == 0 {
        ma(-s, f - r, f)
    } else {
        ma(dk, LN2_HI, -(ma(s, f - r, -(dk * LN2_LO)) - f))
    }
}

pub fn log1p(x: f64) -> f64 {
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;
    const TWO54: f64 = 1.80143985094819840000e+16;
    const LP: [f64; 7] = [
        6.666666666666735130e-01,
        3.999999999940941908e-01,
        2.857142874366239149e-01,
        2.222219843214978396e-01,
        1.818357216161805012e-01,
        1.531383769920937332e-01,
        1.479819860511658591e-01,
    ];
    let hx = hi(x);
    let ax = hx & 0x7FFFFFFF;
    let mut k = 1i32;
    let mut f = 0.0;
    let mut hu = 0i32;
    let mut c = 0.0;
    if hx < 0x3FDA827A {
        if ax >= 0x3FF00000 {
            return if x == -1.0 {
                f64::NEG_INFINITY
            } else {
                f64::NAN
            };
        }
        if ax < 0x3E200000 {
            if TWO54 + x > 0.0 && ax < 0x3C900000 {
                return x;
            }
            return ma(-(x * x), 0.5, x);
        }
        if hx > 0 || hx <= 0xBFD2BEC4_u32 as i32 {
            k = 0;
            f = x;
            hu = 1;
        }
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if k != 0 {
        let mut u;
        if hx < 0x43400000 {
            u = 1.0 + x;
            hu = hi(u);
            k = (hu >> 20) - 1023;
            c = if k > 0 { 1.0 - (u - x) } else { x - (u - 1.0) };
            c /= u;
        } else {
            u = x;
            hu = hi(u);
            k = (hu >> 20) - 1023;
            c = 0.0;
        }
        hu &= 0x000FFFFF;
        if hu < 0x6A09E {
            u = with_hi(u, hu | 0x3FF00000);
        } else {
            k += 1;
            u = with_hi(u, hu | 0x3FE00000);
            hu = (0x00100000 - hu) >> 2;
        }
        f = u - 1.0;
    }
    let hfsq = 0.5 * f * f;
    let kf = f64::from(k);
    if hu == 0 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            c = ma(kf, LN2_LO, c);
            return ma(kf, LN2_HI, c);
        }
        let r = hfsq * ma(-0.66666666666666666, f, 1.0);
        if k == 0 {
            return f - r;
        }
        return ma(kf, LN2_HI, -((r - ma(kf, LN2_LO, c)) - f));
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let r = z * ma(
        z,
        ma(
            z,
            ma(z, ma(z, ma(z, ma(z, LP[6], LP[5]), LP[4]), LP[3]), LP[2]),
            LP[1],
        ),
        LP[0],
    );
    if k == 0 {
        f - ma(-s, hfsq + r, hfsq)
    } else {
        ma(
            kf,
            LN2_HI,
            -((hfsq - ma(s, hfsq + r, ma(kf, LN2_LO, c))) - f),
        )
    }
}

fn k_log1p(f: f64) -> f64 {
    const LG1: f64 = 6.666666666666735130e-01;
    const LG2: f64 = 3.999999999940941908e-01;
    const LG3: f64 = 2.857142874366239149e-01;
    const LG4: f64 = 2.222219843214978396e-01;
    const LG5: f64 = 1.818357216161805012e-01;
    const LG6: f64 = 1.531383769920937332e-01;
    const LG7: f64 = 1.479819860511658591e-01;
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * ma(w, ma(w, LG6, LG4), LG2);
    let t2 = z * ma(w, ma(w, ma(w, LG7, LG5), LG3), LG1);
    let r = t2 + t1;
    let hfsq = 0.5 * f * f;
    s * (hfsq + r)
}

pub fn log2(x: f64) -> f64 {
    const TWO54: f64 = 1.80143985094819840000e+16;
    const IVLN2HI: f64 = 1.44269504072144627571e+00;
    const IVLN2LO: f64 = 1.67517131648865118353e-10;
    let mut x = x;
    let mut hx = hi(x);
    let lx = lo(x);
    let mut k = 0i32;
    if hx < 0x00100000 {
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if hx == 0x3FF00000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let i = (hx + 0x95F64) & 0x100000;
    x = with_hi(x, hx | (i ^ 0x3FF00000));
    k += i >> 20;
    let y = f64::from(k);
    let f = x - 1.0;
    let hfsq = 0.5 * f * f;
    let r = k_log1p(f);
    let h = with_lo(f - hfsq, 0);
    let l = (f - h) - hfsq + r;
    let mut val_hi = h * IVLN2HI;
    let mut val_lo = ma(l + h, IVLN2LO, l * IVLN2HI);
    let w = y + val_hi;
    val_lo += (y - w) + val_hi;
    val_hi = w;
    val_lo + val_hi
}

pub fn log10(x: f64) -> f64 {
    const TWO54: f64 = 1.80143985094819840000e+16;
    const IVLN10: f64 = 4.34294481903251816668e-01;
    const LOG10_2HI: f64 = 3.01029995663611771306e-01;
    const LOG10_2LO: f64 = 3.69423907715893078616e-13;
    let mut x = x;
    let mut hx = hi(x);
    let mut lx = lo(x);
    let mut k = 0i32;
    if hx < 0x00100000 {
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi(x);
        lx = lo(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if hx == 0x3FF00000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    let i = ((k as u32 & 0x80000000) >> 31) as i32;
    hx = (hx & 0x000FFFFF) | ((0x3FF - i) << 20);
    let y = f64::from(k + i);
    x = words(hx, lx);
    let z = ma(y, LOG10_2LO, IVLN10 * ln(x));
    ma(y, LOG10_2HI, z)
}

pub fn expm1(x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    const O_THRESHOLD: f64 = 7.09782712893383973096e+02;
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;
    const INVLN2: f64 = 1.44269504088896338700e+00;
    const Q1: f64 = -3.33333333333331316428e-02;
    const Q2: f64 = 1.58730158725481460165e-03;
    const Q3: f64 = -7.93650757867487942473e-05;
    const Q4: f64 = 4.00821782732936239552e-06;
    const Q5: f64 = -2.01099218183624371326e-07;
    const HUGE: f64 = 1.0e+300;
    let mut x = x;
    let mut hx = hi(x) as u32;
    let xsb = hx & 0x80000000;
    hx &= 0x7FFFFFFF;
    if hx >= 0x4043687A {
        if hx >= 0x40862E42 {
            if hx >= 0x7FF00000 {
                if ((hx & 0xFFFFF) | lo(x)) != 0 {
                    return x + x;
                }
                return if xsb == 0 { x } else { -1.0 };
            }
            if x > O_THRESHOLD {
                return HUGE * HUGE;
            }
        }
        if xsb != 0 && x + TINY < 0.0 {
            return TINY - 1.0;
        }
    }
    let k: i32;
    let mut c = 0.0;
    if hx > 0x3FD62E42 {
        let (h, l);
        if hx < 0x3FF0A2B2 {
            if xsb == 0 {
                h = x - LN2_HI;
                l = LN2_LO;
                k = 1;
            } else {
                h = x + LN2_HI;
                l = -LN2_LO;
                k = -1;
            }
        } else {
            k = ma(INVLN2, x, if xsb == 0 { 0.5 } else { -0.5 }) as i32;
            let t = f64::from(k);
            h = ma(-t, LN2_HI, x);
            l = t * LN2_LO;
        }
        x = h - l;
        c = (h - x) - l;
    } else if hx < 0x3C900000 {
        let t = HUGE + x;
        return x - (t - (HUGE + x));
    } else {
        k = 0;
    }
    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = ma(
        hxs,
        ma(hxs, ma(hxs, ma(hxs, ma(hxs, Q5, Q4), Q3), Q2), Q1),
        1.0,
    );
    let t = ma(-r1, hfx, 3.0);
    let mut e = hxs * ((r1 - t) / ma(-x, t, 6.0));
    if k == 0 {
        return x - ma(x, e, -hxs);
    }
    let twopk = words(0x3FF00000 + ((k as u32) << 20) as i32, 0);
    e = ma(x, e - c, -c);
    e -= hxs;
    if k == -1 {
        return ma(0.5, x - e, -0.5);
    }
    if k == 1 {
        return if x < -0.25 {
            -2.0 * (e - (x + 0.5))
        } else {
            ma(2.0, x - e, 1.0)
        };
    }
    if k <= -2 || k > 56 {
        let mut y = 1.0 - (e - x);
        if k == 1024 {
            y = y * 2.0 * 8.98846567431158e+307;
        } else {
            y *= twopk;
        }
        return y - 1.0;
    }
    if k < 20 {
        let t = words(0x3FF00000 - (0x200000 >> k), 0);
        let y = t - (e - x);
        y * twopk
    } else {
        let t = words((0x3FF - k) << 20, 0);
        let y = x - (e + t);
        (y + 1.0) * twopk
    }
}

pub fn cbrt(x: f64) -> f64 {
    const B1: u32 = 715094163;
    const B2: u32 = 696219795;
    const P0: f64 = 1.87595182427177009643;
    const P1: f64 = -1.88497979543377169875;
    const P2: f64 = 1.621429720105354466140;
    const P3: f64 = -0.758397934778766047437;
    const P4: f64 = 0.145996192886612446982;
    let mut hx = hi(x) as u32;
    let low = lo(x);
    let sign = hx & 0x80000000;
    hx ^= sign;
    if hx >= 0x7FF00000 {
        return x + x;
    }
    let mut t;
    if hx < 0x00100000 {
        if (hx | low) == 0 {
            return x;
        }
        t = words(0x43500000, 0);
        t *= x;
        let high = hi(t) as u32;
        t = words((sign | ((high & 0x7FFFFFFF) / 3 + B2)) as i32, 0);
    } else {
        t = words((sign | (hx / 3 + B1)) as i32, 0);
    }
    let r = (t * t) * (t / x);
    t *= (P0 + r * (P1 + r * P2)) + ((r * r) * r) * (P3 + r * P4);
    t = f64::from_bits((t.to_bits().wrapping_add(0x80000000)) & 0xFFFF_FFFF_C000_0000);
    let s = t * t;
    let mut r = x / s;
    let w = t + t;
    r = (r - t) / (w + r);
    t + t * r
}

// ---- hyperbolics ----

pub fn sinh(x: f64) -> f64 {
    const KSINH_OVERFLOW: f64 = 710.4758600739439;
    const TWO_M28: f64 = 3.725290298461914e-9;
    const LOG_MAXD: f64 = 709.7822265625;
    const SHUGE: f64 = 1.0e307;
    let h = if x < 0.0 { -0.5 } else { 0.5 };
    let ax = x.abs();
    if ax < 22.0 {
        if ax < TWO_M28 {
            return x;
        }
        let t = expm1(ax);
        if ax < 1.0 {
            return h * ma(2.0, t, -(t * t / (t + 1.0)));
        }
        return h * (t + t / (t + 1.0));
    }
    if ax < LOG_MAXD {
        return h * exp(ax);
    }
    if ax <= KSINH_OVERFLOW {
        let w = exp(0.5 * ax);
        let t = h * w;
        return t * w;
    }
    x * SHUGE
}

pub fn cosh(x: f64) -> f64 {
    const KCOSH_OVERFLOW: f64 = 710.4758600739439;
    const HUGE: f64 = 1.0e+300;
    let ix = hi(x) & 0x7FFFFFFF;
    if ix < 0x3FD62E43 {
        let t = expm1(x.abs());
        let w = 1.0 + t;
        if ix < 0x3C800000 {
            return w;
        }
        return 1.0 + (t * t) / (w + w);
    }
    if ix < 0x40360000 {
        let t = exp(x.abs());
        return ma(0.5, t, 0.5 / t);
    }
    if ix < 0x40862E42 {
        return 0.5 * exp(x.abs());
    }
    if x.abs() <= KCOSH_OVERFLOW {
        let w = exp(0.5 * x.abs());
        let t = 0.5 * w;
        return t * w;
    }
    if ix >= 0x7FF00000 {
        return x * x;
    }
    HUGE * HUGE
}

pub fn tanh(x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    let jx = hi(x);
    let ix = jx & 0x7FFFFFFF;
    if ix >= 0x7FF00000 {
        return if jx >= 0 {
            1.0 / x + 1.0
        } else {
            1.0 / x - 1.0
        };
    }
    let z;
    if ix < 0x40360000 {
        if ix < 0x3E300000 {
            return x;
        }
        if ix >= 0x3FF00000 {
            let t = expm1(2.0 * x.abs());
            z = 1.0 - 2.0 / (t + 2.0);
        } else {
            let t = expm1(-2.0 * x.abs());
            z = -t / (t + 2.0);
        }
    } else {
        z = 1.0 - TINY;
    }
    if jx >= 0 { z } else { -z }
}

pub fn asinh(x: f64) -> f64 {
    const LN2: f64 = 6.93147180559945286227e-01;
    let hx = hi(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x7FF00000 {
        return x + x;
    }
    if ix < 0x3E300000 {
        return x;
    }
    let w = if ix > 0x41B00000 {
        ln(x.abs()) + LN2
    } else if ix > 0x40000000 {
        let t = x.abs();
        ln(ma(2.0, t, 1.0 / (ma(x, x, 1.0).sqrt() + t)))
    } else {
        let t = x * x;
        log1p(x.abs() + t / (1.0 + (1.0 + t).sqrt()))
    };
    if hx > 0 { w } else { -w }
}

pub fn acosh(x: f64) -> f64 {
    const LN2: f64 = 6.93147180559945286227e-01;
    let hx = hi(x);
    let lx = lo(x);
    if hx < 0x3FF00000 {
        f64::NAN
    } else if hx >= 0x41B00000 {
        if hx >= 0x7FF00000 { x + x } else { ln(x) + LN2 }
    } else if ((hx - 0x3FF00000) as u32 | lx) == 0 {
        0.0
    } else if hx > 0x40000000 {
        let t = x * x;
        ln(ma(2.0, x, -(1.0 / (x + (t - 1.0).sqrt()))))
    } else {
        let t = x - 1.0;
        log1p(t + ma(2.0, t, t * t).sqrt())
    }
}

pub fn atanh(x: f64) -> f64 {
    let hx = hi(x);
    let lx = lo(x);
    let ix = hx & 0x7FFFFFFF;
    if (ix as u32 | ((lx | lx.wrapping_neg()) >> 31)) > 0x3FF00000 {
        return f64::NAN;
    }
    if ix == 0x3FF00000 {
        return if x > 0.0 {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        };
    }
    if ix < 0x3E300000 {
        return x;
    }
    let x = with_hi(x, ix);
    let t = if ix < 0x3FE00000 {
        let t = x + x;
        0.5 * log1p(t + t * x / (1.0 - x))
    } else {
        0.5 * log1p((x + x) / (1.0 - x))
    };
    if hx >= 0 { t } else { -t }
}

#[cfg(test)]
mod v8_tan {
    // V8's Math.tan digits (fdlibm), which a folded tan(1) prints; the platform tan is an ulp off on some
    #[test]
    fn matches_v8() {
        assert_eq!(super::tan(1.0), 1.5574077246549023);
        assert_eq!(super::tan(10.0), 0.6483608274590866);
        assert_eq!(super::tan(2.5f64.sqrt()), -96.68494328539326);
        assert_eq!(super::tan(0.5), 0.5463024898437905);
        assert_eq!(super::tan(-3.0), 0.1425465430742778);
        assert_eq!(super::tan(1e5), -0.035771662952898776);
        assert_eq!(super::tan(2.0), -2.185039863261519);
        assert_eq!(super::tan(4.7), 80.71276296747477);
    }
}
