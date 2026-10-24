//! V8's `Math.tan` (fdlibm's `tan`, `__kernel_tan` and the small/medium `__ieee754_rem_pio2`, as in V8's
//! `src/base/ieee754.cc`), so a folded `tan(1)` prints the digits mathjs prints. The platform libm's tan
//! differs from fdlibm's by an ulp on some inputs (`tan(1)`, `tan(10)`); sin, cos, exp and log agreed on every
//! corpus input and stay on the platform libm. Arguments past 2^19·π/2 fall back to the platform tan.

// fdlibm's constants, digit for digit
#![allow(clippy::excessive_precision, clippy::approx_constant)]

fn high(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

fn low(x: f64) -> u32 {
    x.to_bits() as u32
}

fn with_low_zero(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0xffff_ffff_0000_0000)
}

const T: [f64; 13] = [
    3.333_333_333_333_340_919_86e-01,
    1.333_333_333_332_012_426_99e-01,
    5.396_825_397_622_605_213_77e-02,
    2.186_948_829_485_954_245_99e-02,
    8.863_239_823_599_300_057_37e-03,
    3.592_079_107_591_312_353_56e-03,
    1.456_209_454_325_290_255_16e-03,
    5.880_412_408_202_640_968_74e-04,
    2.464_631_348_184_699_068_12e-04,
    7.817_944_429_395_570_923_00e-05,
    7.140_724_913_826_081_903_05e-05,
    -1.855_863_748_552_754_566_54e-05,
    2.590_730_518_636_337_128_84e-05,
];
const PIO4: f64 = 7.853_981_633_974_482_789_99e-01;
const PIO4LO: f64 = 3.061_616_997_868_383_017_93e-17;

fn kernel_tan(mut x: f64, mut y: f64, iy: i32) -> f64 {
    let hx = high(x);
    let ix = hx & 0x7fff_ffff;
    if ix < 0x3e30_0000 && (x as i32) == 0 {
        if ((ix as u32 | low(x)) | (iy + 1) as u32) == 0 {
            return 1.0 / x.abs();
        }
        if iy == 1 {
            return x;
        }
        let w = x + y;
        let z = with_low_zero(w);
        let v = y - (z - x);
        let a = -1.0 / w;
        let t = with_low_zero(a);
        let s = 1.0 + t * z;
        return t + a * (s + t * v);
    }
    if ix >= 0x3FE5_9428 {
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
    let mut r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    r = y + z * (s * (r + v) + y);
    r += T[0] * s;
    let w = x + r;
    if ix >= 0x3FE5_9428 {
        let v = f64::from(iy);
        return f64::from(1 - ((hx >> 30) & 2)) * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        return w;
    }
    let z = with_low_zero(w);
    let v = r - (z - x);
    let a = -1.0 / w;
    let t = with_low_zero(a);
    let s = 1.0 + t * z;
    t + a * (s + t * v)
}

const INVPIO2: f64 = 6.366_197_723_675_813_824_33e-01;
const PIO2_1: f64 = 1.570_796_326_734_125_614_17e+00;
const PIO2_1T: f64 = 6.077_100_506_506_192_249_32e-11;
const PIO2_2: f64 = 6.077_100_506_303_965_976_60e-11;
const PIO2_2T: f64 = 2.022_266_248_795_950_631_54e-21;
const PIO2_3: f64 = 2.022_266_248_711_166_455_80e-21;
const PIO2_3T: f64 = 8.478_427_660_368_899_569_97e-32;

/// `__ieee754_rem_pio2` for |x| up to 2^19·π/2: `x - n·π/2` as a double-double, and `n`.
fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
    let hx = high(x);
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return Some((0, x, 0.0));
    }
    if ix < 0x4002_d97c {
        return Some(if hx > 0 {
            let mut z = x - PIO2_1;
            if ix != 0x3ff9_21fb {
                let y0 = z - PIO2_1T;
                (1, y0, (z - y0) - PIO2_1T)
            } else {
                z -= PIO2_2;
                let y0 = z - PIO2_2T;
                (1, y0, (z - y0) - PIO2_2T)
            }
        } else {
            let mut z = x + PIO2_1;
            if ix != 0x3ff9_21fb {
                let y0 = z + PIO2_1T;
                (-1, y0, (z - y0) + PIO2_1T)
            } else {
                z += PIO2_2;
                let y0 = z + PIO2_2T;
                (-1, y0, (z - y0) + PIO2_2T)
            }
        });
    }
    if ix > 0x4139_21fb {
        return None;
    }
    let t = x.abs();
    let n = (t * INVPIO2 + 0.5) as i32;
    let f = f64::from(n);
    let mut r = t - f * PIO2_1;
    let mut w = f * PIO2_1T;
    // fdlibm skips this check when n < 32 and x isn't near a multiple of π/2; the check gives the same answer
    let j = ix >> 20;
    let mut y0 = r - w;
    let i = j - ((high(y0) >> 20) & 0x7ff);
    if i > 16 {
        let t = r;
        w = f * PIO2_2;
        r = t - w;
        w = f * PIO2_2T - ((t - r) - w);
        y0 = r - w;
        let i = j - ((high(y0) >> 20) & 0x7ff);
        if i > 49 {
            let t = r;
            w = f * PIO2_3;
            r = t - w;
            w = f * PIO2_3T - ((t - r) - w);
            y0 = r - w;
        }
    }
    let y1 = (r - y0) - w;
    Some(if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) })
}

/// `Math.tan(x)` as V8 computes it.
pub fn tan(x: f64) -> f64 {
    let ix = high(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return kernel_tan(x, 0.0, 1);
    }
    if ix >= 0x7ff0_0000 {
        return f64::NAN;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => kernel_tan(y0, y1, 1 - ((n & 1) << 1)),
        None => x.tan(),
    }
}

#[cfg(test)]
mod tests {
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
