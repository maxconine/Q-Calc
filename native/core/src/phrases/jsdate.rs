//! The parts of JS `Date` the time phrases use, with the ECMAScript algorithms (MakeDay, MakeTime, TimeClip,
//! the 0–99 → 1900s year rule) on f64 so out-of-range and NaN dates come out as the TS shows them. Local time is
//! the given [`TimeZone`]: local → UTC picks the earlier offset in a repeated hour and moves a skipped time
//! forward (the spec's and V8's rule, jiff's "compatible").

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

pub const MS_DAY: f64 = 86_400_000.0;
const MAX_TIME: f64 = 8.64e15;
// 400 Gregorian years: the calendar (and a zone's rules from then on) repeat
const CYCLE_MS: f64 = 146_097.0 * MS_DAY;
// what jiff's timestamps cover, with room for offsets
const EARLIEST_MS: f64 = -377_000_000_000_000.0;
const LATEST_MS: f64 = 253_000_000_000_000.0;
const Y2000_MS: f64 = 946_684_800_000.0;

/// `Math.min(a, b)`: NaN wins.
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn days_from_civil(y: i128, m: i128, d: i128) -> i128 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// MakeDay(year, month, date), month 0-based.
pub fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !(year.is_finite() && month.is_finite() && date.is_finite()) {
        return f64::NAN;
    }
    let (y, m, dt) = (year.trunc(), month.trunc(), date.trunc());
    let ym = y + (m / 12.0).floor();
    if ym.abs() > 1e15 {
        return f64::NAN;
    }
    let mn = m.rem_euclid(12.0);
    days_from_civil(ym as i128, mn as i128 + 1, 1) as f64 + dt - 1.0
}

pub fn make_time(h: f64, min: f64, s: f64, ms: f64) -> f64 {
    if !(h.is_finite() && min.is_finite() && s.is_finite() && ms.is_finite()) {
        return f64::NAN;
    }
    h.trunc() * 3_600_000.0 + min.trunc() * 60_000.0 + s.trunc() * 1000.0 + ms.trunc()
}

fn make_date(day: f64, time: f64) -> f64 {
    let t = day * MS_DAY + time;
    if t.is_finite() { t } else { f64::NAN }
}

pub fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > MAX_TIME {
        f64::NAN
    } else {
        t.trunc() + 0.0
    }
}

// the Date constructor's and Date.UTC's year: 0 to 99 mean 1900 to 1999
fn full_year(y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    let yi = y.trunc();
    if (0.0..=99.0).contains(&yi) {
        1900.0 + yi
    } else {
        y
    }
}

// a time value moved into the range the tz database covers: the earliest offset before it, whole 400-year
// cycles back from after it (the zone's rule for the future repeats with the calendar)
fn in_range(t: f64) -> (f64, f64) {
    if t < EARLIEST_MS {
        (EARLIEST_MS, 0.0)
    } else if t > LATEST_MS {
        let k = ((t - Y2000_MS) / CYCLE_MS).floor();
        (t - k * CYCLE_MS, k * CYCLE_MS)
    } else {
        (t, 0.0)
    }
}

/// The zone's offset at UTC time value `t`, in ms.
pub fn offset_at(tz: &TimeZone, t: f64) -> f64 {
    let (at, _) = in_range(t);
    Timestamp::from_millisecond(at as i64)
        .map_or(0.0, |ts| f64::from(tz.to_offset(ts).seconds()) * 1000.0)
}

/// UTC(t): local time value to UTC.
pub fn utc_from_local(tz: &TimeZone, local: f64) -> f64 {
    if !local.is_finite() {
        return f64::NAN;
    }
    let (at, shift) = in_range(local);
    let days = (at / MS_DAY).floor();
    let ms = at - days * MS_DAY;
    let (y, m, d) = civil_from_days(days as i64);
    let ms = ms as i64;
    let dt = DateTime::new(
        y as i16,
        m as i8,
        d as i8,
        (ms / 3_600_000) as i8,
        (ms / 60_000 % 60) as i8,
        (ms / 1000 % 60) as i8,
        (ms % 1000) as i32 * 1_000_000,
    );
    let Ok(dt) = dt else { return f64::NAN };
    match tz.to_ambiguous_timestamp(dt).compatible() {
        Ok(ts) => ts.as_millisecond() as f64 + shift,
        Err(_) => f64::NAN,
    }
}

/// `new Date(y, m, d, h, min)` in local time, as a time value.
pub fn local_date(tz: &TimeZone, y: f64, m: f64, d: f64, h: f64, min: f64) -> f64 {
    let local = make_date(make_day(full_year(y), m, d), make_time(h, min, 0.0, 0.0));
    time_clip(utc_from_local(tz, local))
}

/// `Date.UTC(y, m, d)`.
pub fn utc_date(y: f64, m: f64, d: f64) -> f64 {
    time_clip(make_date(make_day(full_year(y), m, d), 0.0))
}

/// A time value's local fields, as the getters return them (all NaN for an invalid date).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fields {
    pub year: f64,
    pub month: f64,
    pub date: f64,
    pub hours: f64,
    pub minutes: f64,
    pub seconds: f64,
    pub day: f64,
}

pub fn fields_in(t: f64, offset: f64) -> Fields {
    if t.is_nan() {
        let n = f64::NAN;
        return Fields {
            year: n,
            month: n,
            date: n,
            hours: n,
            minutes: n,
            seconds: n,
            day: n,
        };
    }
    let local = t + offset;
    let days = (local / MS_DAY).floor();
    let ms = local - days * MS_DAY;
    let (y, m, d) = civil_from_days(days as i64);
    Fields {
        year: y as f64,
        month: (m - 1) as f64,
        date: d as f64,
        hours: (ms / 3_600_000.0).floor(),
        minutes: (ms / 60_000.0).floor() % 60.0,
        seconds: (ms / 1000.0).floor() % 60.0,
        day: (days + 4.0).rem_euclid(7.0),
    }
}

/// The local getters of time value `t`.
pub fn local_fields(tz: &TimeZone, t: f64) -> Fields {
    if t.is_nan() {
        return fields_in(t, 0.0);
    }
    fields_in(t, offset_at(tz, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn la() -> TimeZone {
        jiff::tz::db().get("America/Los_Angeles").unwrap()
    }

    #[test]
    fn local_dates_like_v8() {
        let tz = la();
        // new Date(2026, 8, 24, 15, 12) is 22:12Z in summer time
        assert_eq!(
            local_date(&tz, 2026.0, 8.0, 24.0, 15.0, 12.0),
            1_790_287_920_000.0
        );
        // 2:30 on the spring-forward night is moved to 3:30
        let t = local_date(&tz, 2026.0, 2.0, 8.0, 2.0, 30.0);
        assert_eq!(local_fields(&tz, t).hours, 3.0);
        // 1:30 on the fall-back night is the first one (summer time)
        let t = local_date(&tz, 2026.0, 10.0, 1.0, 1.0, 30.0);
        assert_eq!(offset_at(&tz, t), -7.0 * 3_600_000.0);
        // overflowing fields roll, years 0-99 are 1900s, and out of range is NaN
        let f = local_fields(&tz, local_date(&tz, 2026.0, 0.0, 32.0, 0.0, 0.0));
        assert_eq!((f.month, f.date), (1.0, 1.0));
        assert_eq!(
            local_fields(&tz, local_date(&tz, 50.0, 0.0, 1.0, 0.0, 0.0)).year,
            1950.0
        );
        assert!(local_date(&tz, 300_000.0, 0.0, 1.0, 0.0, 0.0).is_nan());
        assert_eq!(
            local_fields(&tz, local_date(&tz, 200_000.0, 6.0, 1.0, 12.0, 0.0)).hours,
            12.0
        );
        assert_eq!(utc_date(1970.0, 0.0, 2.0), MS_DAY);
        assert_eq!(fields_in(0.0, 0.0).day, 4.0);
    }
}
