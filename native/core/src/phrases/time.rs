//! `src/engine/phraseTime.ts`: clock times and calendar dates in the local time zone. Everything relative to now
//! takes `now` (a JS time value) and the zone in.
//!
//! A day keeps the TS's numbers as f64, so a month that a plain-object lookup made a function (`constructor 5`,
//! see `super`'s docs) is NaN, and the dates built from it show as the TS shows them (`undefined 5 undefined 2026`).

use jiff::tz::TimeZone;

use super::jsdate::{MS_DAY, js_min, local_date, local_fields, utc_date};
use super::{Answered, PhraseAnswer, Thrown, caps, g, js_re, js_trim, proto_value, replace_first};
use crate::jsnum;
use crate::jsre::{self, Lazy, Regex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Day {
    pub y: f64,
    pub m: f64,
    pub d: f64,
}

/// `{ minutes, days, months }`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Span {
    pub minutes: f64,
    pub days: f64,
    pub months: f64,
}

const WEEKDAYS: [&str; 7] = [
    "sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday",
];
const MONTHS: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november",
    "december",
];
const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

// SHORT_WEEKDAYS[w]; a prototype member is a function, which is NaN in the arithmetic after it
fn short_weekday(w: &str) -> Option<f64> {
    Some(match w {
        "sun" => 0.0,
        "mon" => 1.0,
        "tue" | "tues" => 2.0,
        "wed" => 3.0,
        "thu" | "thur" | "thurs" => 4.0,
        "fri" => 5.0,
        "sat" => 6.0,
        _ => return proto_value(w).map(|_| f64::NAN),
    })
}

fn month_index(w: &str) -> Option<f64> {
    if let Some(i) = MONTHS.iter().position(|m| *m == w) {
        return Some(i as f64);
    }
    Some(match w {
        "jan" => 0.0,
        "feb" => 1.0,
        "mar" => 2.0,
        "apr" => 3.0,
        "jun" => 5.0,
        "jul" => 6.0,
        "aug" => 7.0,
        "sep" | "sept" => 8.0,
        "oct" => 9.0,
        "nov" => 10.0,
        "dec" => 11.0,
        _ => return proto_value(w).map(|_| f64::NAN),
    })
}

fn days_in_month(tz: &TimeZone, y: f64, m: f64) -> f64 {
    local_fields(tz, local_date(tz, y, m + 1.0, 0.0, 0.0, 0.0)).date
}

fn day_of(tz: &TimeZone, t: f64) -> Day {
    let f = local_fields(tz, t);
    Day {
        y: f.year,
        m: f.month,
        d: f.date,
    }
}

fn to_date(tz: &TimeZone, day: Day) -> f64 {
    local_date(tz, day.y, day.m, day.d, 0.0, 0.0)
}

fn add_days(tz: &TimeZone, day: Day, k: f64) -> Day {
    day_of(tz, local_date(tz, day.y, day.m, day.d + k, 0.0, 0.0))
}

// jan 31 + 1 month is the last day of february
fn add_months(tz: &TimeZone, day: Day, k: f64) -> Day {
    let total = day.y * 12.0 + day.m + k;
    let y = (total / 12.0).floor();
    let m = total - y * 12.0;
    Day {
        y,
        m,
        d: js_min(day.d, days_in_month(tz, y, m)),
    }
}

// whole calendar days, immune to daylight saving
fn day_diff(a: Day, b: Day) -> f64 {
    jsnum::round((utc_date(b.y, b.m, b.d) - utc_date(a.y, a.m, a.d)) / MS_DAY)
}

fn compare_days(a: Day, b: Day) -> f64 {
    day_diff(b, a)
}

fn name<const N: usize>(names: [&str; N], i: f64) -> &str {
    if i >= 0.0 && i < N as f64 && i.fract() == 0.0 {
        names[i as usize]
    } else {
        "undefined"
    }
}

pub fn format_date(tz: &TimeZone, day: Day) -> String {
    let wd = local_fields(tz, to_date(tz, day)).day;
    format!(
        "{} {} {} {}",
        name(WD, wd),
        jsnum::to_string(day.d),
        name(MON, day.m),
        jsnum::to_string(day.y)
    )
}

pub fn format_clock(minutes: f64) -> String {
    let t = ((jsnum::round(minutes) % 1440.0) + 1440.0) % 1440.0;
    let h = (t / 60.0).floor();
    let m = t % 60.0;
    let h12 = if h % 12.0 == 0.0 { 12.0 } else { h % 12.0 };
    let mm = jsnum::to_string(m);
    let mm = if mm.chars().count() < 2 {
        format!("0{mm}")
    } else {
        mm
    };
    format!(
        "{}:{mm} {}",
        jsnum::to_string(h12),
        if h < 12.0 { "am" } else { "pm" }
    )
}

pub fn format_duration(minutes: f64) -> String {
    let h = (minutes / 60.0).floor();
    let m = minutes - h * 60.0;
    let truthy = |x: f64| x != 0.0 && !x.is_nan();
    if truthy(h) && truthy(m) {
        return format!("{} hr {} min", jsnum::to_string(h), jsnum::to_string(m));
    }
    if truthy(h) {
        format!("{} hr", jsnum::to_string(h))
    } else {
        format!("{} min", jsnum::to_string(m))
    }
}

fn plural(n: f64, word: &str) -> String {
    format!(
        "{} {word}{}",
        jsnum::to_string(n),
        if n.abs() == 1.0 { "" } else { "s" }
    )
}

/// `3:45pm`, `9 am`, `noon`, or an unmistakable 24 hour time like `15:30`, as minutes after midnight.
pub fn parse_clock(s: &str) -> Option<f64> {
    static AMPM: Lazy = Lazy::new(|| js_re(r"^(\d{1,2})(?::(\d{2}))?\s*([ap])\.?m\.?$", ""));
    static H24: Lazy = Lazy::new(|| js_re(r"^(\d{2}):(\d{2})$", ""));
    let t = js_trim(s);
    match t {
        "noon" | "midday" => return Some(720.0),
        "midnight" => return Some(0.0),
        _ => {}
    }
    let num = |x: &str| jsnum::parse(x).unwrap_or(f64::NAN);
    if let Some(m) = caps(&AMPM, t) {
        let h = num(g(&m, 1));
        let min = if m.get(2).is_some() { num(g(&m, 2)) } else { 0.0 };
        if !(1.0..=12.0).contains(&h) || min > 59.0 {
            return None;
        }
        let pm = if g(&m, 3) == "p" { 12.0 } else { 0.0 };
        return Some(((h % 12.0) + pm) * 60.0 + min);
    }
    // `3:45` could be a duration or either half of the day, so only 13:00 to 23:59 reads as a time
    let m = caps(&H24, t)?;
    let (h, min) = (num(g(&m, 1)), num(g(&m, 2)));
    ((13.0..=23.0).contains(&h) && min <= 59.0).then_some(h * 60.0 + min)
}

#[derive(Clone, Copy)]
enum SpanKey {
    Minutes,
    Days,
    Months,
}

fn span_unit(w: &str) -> Option<(SpanKey, f64)> {
    Some(match w {
        "h" | "hr" | "hrs" | "hour" | "hours" => (SpanKey::Minutes, 60.0),
        "min" | "mins" | "minute" | "minutes" => (SpanKey::Minutes, 1.0),
        "day" | "days" => (SpanKey::Days, 1.0),
        "wk" | "wks" | "week" | "weeks" => (SpanKey::Days, 7.0),
        "month" | "months" => (SpanKey::Months, 1.0),
        "yr" | "yrs" | "year" | "years" => (SpanKey::Months, 12.0),
        _ => return None,
    })
}

/// `4 hr 10 min`, `3 weeks`, `1 year and 2 months`; clock and calendar units never mix.
pub fn parse_span(s: &str) -> Option<Span> {
    static SPLIT: Lazy = Lazy::new(|| js_re(r"\s*(?:,|\band\b)\s*|\s+(?=\d)", ""));
    static PART: Lazy = Lazy::new(|| js_re(r"^(\d+(?:\.\d+)?|a|an|one)\s*([a-z]+)$", ""));
    let t = js_trim(s);
    let mut parts = Vec::new();
    let mut last = 0;
    for m in SPLIT.find_iter(t).flatten() {
        parts.push(&t[last..m.start()]);
        last = m.end();
    }
    parts.push(&t[last..]);
    parts.retain(|p| !p.is_empty());
    if parts.is_empty() {
        return None;
    }
    let mut span = Span::default();
    let mut seen: Vec<&str> = Vec::new();
    for part in parts {
        let m = caps(&PART, part)?;
        let qty_text = g(&m, 1);
        let qty = if qty_text.starts_with(|c: char| c.is_ascii_digit()) {
            jsnum::parse(qty_text).unwrap_or(f64::NAN)
        } else {
            1.0
        };
        let word = g(&m, 2);
        let (key, per) = span_unit(word)?;
        if seen.contains(&word) {
            return None;
        }
        seen.push(word);
        let add = qty * per;
        if !jsnum::is_integer(add) {
            return None;
        }
        match key {
            SpanKey::Minutes => span.minutes += add,
            SpanKey::Days => span.days += add,
            SpanKey::Months => span.months += add,
        }
    }
    if span.minutes != 0.0 && (span.days != 0.0 || span.months != 0.0) {
        return None;
    }
    Some(span)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Roll {
    Year,
    Future,
    Past,
}

// holidays on fixed dates, without "day" and apostrophes: `christmas`, `new year's`, `valentine's day`
fn holiday(key: &str) -> Option<&'static str> {
    Some(match key {
        "christmas" | "xmas" => "december 25",
        "christmas eve" => "december 24",
        "new year" | "new years" => "january 1",
        "new years eve" | "new year eve" => "december 31",
        "halloween" => "october 31",
        "valentines" => "february 14",
        "st patricks" => "march 17",
        "fourth of july" | "independence" => "july 4",
        _ => return None,
    })
}

// the clock in a city or a zone abbreviation, as the people there read it
fn zone(key: &str) -> Option<&'static str> {
    Some(match key {
        "utc" | "gmt" => "UTC",
        "est" | "edt" | "eastern" | "new york" | "nyc" | "boston" | "miami" | "atlanta" | "washington" => {
            "America/New_York"
        }
        "cst" | "cdt" | "central" | "chicago" | "dallas" | "houston" => "America/Chicago",
        "mst" | "mdt" | "mountain" | "denver" => "America/Denver",
        "pst" | "pdt" | "pacific" | "los angeles" | "la" | "san francisco" | "sf" | "seattle" => {
            "America/Los_Angeles"
        }
        "toronto" => "America/Toronto",
        "mexico city" => "America/Mexico_City",
        "phoenix" => "America/Phoenix",
        "vancouver" => "America/Vancouver",
        "honolulu" | "hawaii" => "Pacific/Honolulu",
        "sao paulo" | "são paulo" => "America/Sao_Paulo",
        "buenos aires" => "America/Argentina/Buenos_Aires",
        "london" => "Europe/London",
        "dublin" => "Europe/Dublin",
        "lisbon" => "Europe/Lisbon",
        "paris" => "Europe/Paris",
        "madrid" | "barcelona" => "Europe/Madrid",
        "berlin" | "munich" => "Europe/Berlin",
        "amsterdam" => "Europe/Amsterdam",
        "brussels" => "Europe/Brussels",
        "zurich" => "Europe/Zurich",
        "rome" | "milan" => "Europe/Rome",
        "vienna" => "Europe/Vienna",
        "stockholm" => "Europe/Stockholm",
        "oslo" => "Europe/Oslo",
        "copenhagen" => "Europe/Copenhagen",
        "helsinki" => "Europe/Helsinki",
        "warsaw" => "Europe/Warsaw",
        "prague" => "Europe/Prague",
        "athens" => "Europe/Athens",
        "istanbul" => "Europe/Istanbul",
        "moscow" => "Europe/Moscow",
        "cairo" => "Africa/Cairo",
        "johannesburg" => "Africa/Johannesburg",
        "lagos" => "Africa/Lagos",
        "nairobi" => "Africa/Nairobi",
        "dubai" => "Asia/Dubai",
        "mumbai" | "delhi" | "new delhi" | "bangalore" | "bengaluru" | "india" => "Asia/Kolkata",
        "karachi" => "Asia/Karachi",
        "bangkok" => "Asia/Bangkok",
        "jakarta" => "Asia/Jakarta",
        "singapore" => "Asia/Singapore",
        "kuala lumpur" => "Asia/Kuala_Lumpur",
        "manila" => "Asia/Manila",
        "hong kong" => "Asia/Hong_Kong",
        "shanghai" | "beijing" | "china" => "Asia/Shanghai",
        "taipei" => "Asia/Taipei",
        "seoul" => "Asia/Seoul",
        "tokyo" | "japan" => "Asia/Tokyo",
        "sydney" => "Australia/Sydney",
        "melbourne" => "Australia/Melbourne",
        "brisbane" => "Australia/Brisbane",
        "perth" => "Australia/Perth",
        "auckland" => "Pacific/Auckland",
        // a prototype member is a function, which Intl rejects as a zone; the TS catches that and shows nothing
        _ => return None,
    })
}

// `time in tokyo`, `what time is it in new york`, `time in pst`
fn city_time(s: &str, now: f64) -> Option<PhraseAnswer> {
    static RE: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:the )?(?:current )?time|what time is it|what time) (?:is it )?in (.+)$",
            "",
        )
    });
    let m = caps(&RE, s)?;
    let tz = super::time_zone(zone(js_trim(g(&m, 1)))?)?;
    // Intl formats an invalid date by throwing, which the TS catches
    if now.is_nan() {
        return None;
    }
    let f = local_fields(&tz, now);
    Some(PhraseAnswer::shown(format_clock(f.hours * 60.0 + f.minutes)))
}

// `10:30` or `9:05` as a time of day, which parseClock leaves alone because on its own it could be a duration
fn loose_clock(s: &str) -> Option<f64> {
    static RE: Lazy = Lazy::new(|| js_re(r"^(\d{1,2}):(\d{2})$", ""));
    let m = caps(&RE, js_trim(s))?;
    let h = jsnum::parse(g(&m, 1)).unwrap_or(f64::NAN);
    let min = jsnum::parse(g(&m, 2)).unwrap_or(f64::NAN);
    (h <= 23.0 && min <= 59.0).then_some(h * 60.0 + min)
}

/// `parseDay`: `today`, `next friday`, `monday`, `dec 25`, `25 december 2026`. Errs where the TS throws (a
/// holiday lookup that finds a prototype member, then calls `.trim()` on it).
fn parse_day(tz: &TimeZone, s: &str, today: Day, roll: Roll) -> Result<Option<Day>, Thrown> {
    static QUOTES: Lazy = Lazy::new(|| js_re("[’']", ""));
    static DAY_WORD: Lazy = Lazy::new(|| js_re(r"\s+day$", ""));
    static WEEKDAY: Lazy = Lazy::new(|| js_re(r"^(?:(next|last)\s+)?([a-z]+)$", ""));
    static MD: Lazy = Lazy::new(|| {
        js_re(r"^([a-z]+)\.?\s+(\d{1,2})(?:st|nd|rd|th)?(?:,?\s+(\d{4}))?$", "")
    });
    static DM: Lazy = Lazy::new(|| {
        js_re(r"^(\d{1,2})(?:st|nd|rd|th)?\s+(?:of\s+)?([a-z]+)\.?(?:,?\s+(\d{4}))?$", "")
    });
    let t = js_trim(s);
    let unquoted = jsre::replace_all(&QUOTES, t, |_| String::new());
    let key = replace_first(&DAY_WORD, &unquoted, |_| String::new());
    if let Some(h) = holiday(&key) {
        return parse_day(tz, h, today, roll);
    }
    if proto_value(&key).is_some() {
        return Err(Thrown("TypeError: s.trim is not a function".into()));
    }
    match t {
        "today" => return Ok(Some(today)),
        "tomorrow" => return Ok(Some(add_days(tz, today, 1.0))),
        "yesterday" => return Ok(Some(add_days(tz, today, -1.0))),
        _ => {}
    }
    if let Some(wd) = caps(&WEEKDAY, t) {
        let which = wd.get(1).map(|w| w.as_str());
        let w = g(&wd, 2);
        let idx = match WEEKDAYS.iter().position(|d| *d == w) {
            Some(i) => Some(i as f64),
            None if which.is_some() => short_weekday(w),
            None => None,
        };
        if let Some(idx) = idx {
            let cur = local_fields(tz, to_date(tz, today)).day;
            let k = match which {
                Some("last") => -(((cur - idx + 6.0) % 7.0) + 1.0),
                Some(_) => ((idx - cur + 6.0) % 7.0) + 1.0,
                None => (idx - cur + 7.0) % 7.0,
            };
            return Ok(Some(add_days(tz, today, k)));
        }
    }
    let parts = caps(&MD, t)
        .map(|m| (g(&m, 1), g(&m, 2), m.get(3).map(|y| y.as_str())))
        .or_else(|| caps(&DM, t).map(|m| (g(&m, 2), g(&m, 1), m.get(3).map(|y| y.as_str()))));
    let Some((mon, d, year)) = parts else {
        return Ok(None);
    };
    let Some(m) = month_index(mon) else {
        return Ok(None);
    };
    let d = jsnum::parse(d).unwrap_or(f64::NAN);
    if !(1.0..=31.0).contains(&d) {
        return Ok(None);
    }
    let explicit = year.map(|y| jsnum::parse(y).unwrap_or(f64::NAN));
    let mut y = explicit.unwrap_or(today.y);
    // feb 29 without a year looks for the next leap year only when rolling
    if d > days_in_month(tz, y, m) && (explicit.is_some() || roll == Roll::Year) {
        return Ok(None);
    }
    if explicit.is_none() && roll != Roll::Year {
        let step = if roll == Roll::Future { 1.0 } else { -1.0 };
        for _ in 0..8 {
            let cand = Day { y, m, d };
            let ok = d <= days_in_month(tz, y, m);
            let c = compare_days(cand, today);
            if ok && (if roll == Roll::Future { c >= 0.0 } else { c <= 0.0 }) {
                return Ok(Some(cand));
            }
            y += step;
        }
        return Ok(None);
    }
    Ok(Some(Day { y, m, d }))
}

fn shift_day(tz: &TimeZone, day: Day, span: Span, sign: f64) -> Option<Day> {
    if span.minutes != 0.0 {
        return None;
    }
    Some(add_days(
        tz,
        add_months(tz, day, sign * span.months),
        sign * span.days,
    ))
}

fn shift_now(tz: &TimeZone, now: f64, span: Span, sign: f64) -> Option<String> {
    let today = day_of(tz, now);
    if span.minutes == 0.0 {
        return shift_day(tz, today, span, sign).map(|d| format_date(tz, d));
    }
    let f = local_fields(tz, now);
    let at = local_date(
        tz,
        f.year,
        f.month,
        f.date,
        f.hours,
        f.minutes + sign * span.minutes,
    );
    let af = local_fields(tz, at);
    let clock = format_clock(af.hours * 60.0 + af.minutes);
    let day = day_of(tz, at);
    Some(if compare_days(day, today) == 0.0 {
        clock
    } else {
        format!("{}, {clock}", format_date(tz, day))
    })
}

fn re(p: &str) -> Regex {
    js_re(p, "")
}

/// `answerTimePhrase` on a normalized line.
pub fn answer_time_phrase(s: &str, now: f64, tz: &TimeZone) -> Answered {
    static ARITH: Lazy = Lazy::new(|| re(r"^(.+?)\s*([+-])\s*(.+)$"));
    static RANGE: Lazy = Lazy::new(|| re(r"^(?:from\s+)?(.+?)\s+(?:to|until|till)\s+(.+)$"));
    static BETWEEN_CLOCKS: Lazy = Lazy::new(|| re(r"^between\s+(.+?)\s+and\s+(.+)$"));
    static UNTIL: Lazy = Lazy::new(|| re(r"^(?:(?:how long|time)\s+)?(?:until|till)\s+(.+)$"));
    static COUNT: Lazy = Lazy::new(|| re(r"^(?:how many\s+)?days?\s+(until|till|to|since)\s+(.+)$"));
    static BETWEEN: Lazy = Lazy::new(|| re(r"^(?:how many\s+)?days?\s+between\s+(.+?)\s+and\s+(.+)$"));
    static FROM_TO: Lazy = Lazy::new(|| re(r"^(?:how many\s+)?days?\s+from\s+(.+?)\s+(?:to|until)\s+(.+)$"));
    static SIGNED: Lazy = Lazy::new(|| re(r"^(?:how many\s+)?days?\s+from"));
    static AGO: Lazy = Lazy::new(|| re(r"^(.+?)\s+ago$"));
    static FROM_NOW: Lazy = Lazy::new(|| re(r"^(.+?)\s+from\s+now$"));
    static IN: Lazy = Lazy::new(|| re(r"^in\s+(.+)$"));
    static AROUND: Lazy = Lazy::new(|| re(r"^(.+?)\s+(from|after|before)\s+(.+)$"));

    let shown = |d: String| Ok(Some(PhraseAnswer::shown(d)));
    let today = day_of(tz, now);

    // clock time on its own, or plus/minus a clock span
    if let Some(clock) = parse_clock(s) {
        return shown(format_clock(clock));
    }
    let arith = caps(&ARITH, s);
    if let Some(a) = &arith {
        let (left, op, right) = (g(a, 1), g(a, 2), g(a, 3));
        // with a span after it, `10:30` is a time of day whatever the hour
        let t = parse_clock(left).or_else(|| loose_clock(left));
        let span = parse_span(right);
        if let (Some(t), Some(span)) = (t, span)
            && span.minutes != 0.0
            && span.days == 0.0
            && span.months == 0.0
        {
            let m = if op == "+" { span.minutes } else { -span.minutes };
            return shown(format_clock(t + m));
        }
        // `5pm - 9am`: how long from the second to the first
        if let (Some(a), Some(b)) = (parse_clock(left), parse_clock(right))
            && op == "-"
            && a > b
        {
            return shown(format_duration(a - b));
        }
    }

    if let Some(zone) = city_time(s, now) {
        return Ok(Some(zone));
    }

    // the length of time between two clock times; overnight wraps to the next day
    if let Some(r) = caps(&RANGE, s).or_else(|| caps(&BETWEEN_CLOCKS, s)) {
        let (a, b) = (parse_clock(g(&r, 1)), parse_clock(g(&r, 2)));
        if let (Some(a), Some(b)) = (a, b)
            && a != b
        {
            return shown(format_duration((b - a + 1440.0) % 1440.0));
        }
        // `march 1 to may 1`: the days between two dates
        let from = parse_day(tz, g(&r, 1), today, Roll::Year)?;
        let to = parse_day(tz, g(&r, 2), today, Roll::Year)?;
        if let (Some(from), Some(to)) = (from, to) {
            let n = day_diff(from, to);
            return Ok((n > 0.0).then(|| PhraseAnswer {
                display: plural(n, "day"),
                n: Some(n),
            }));
        }
    }

    if let Some(u) = caps(&UNTIL, s)
        && let Some(t) = parse_clock(g(&u, 1))
    {
        let f = local_fields(tz, now);
        let cur = f.hours * 60.0 + f.minutes + f.seconds / 60.0;
        let left = jsnum::round((t - cur + 1440.0) % 1440.0);
        return Ok((left > 0.0 && left < 1440.0).then(|| PhraseAnswer::shown(format_duration(left))));
    }

    // counting days
    if let Some(c) = caps(&COUNT, s) {
        let future = g(&c, 1) != "since";
        let roll = if future { Roll::Future } else { Roll::Past };
        let Some(day) = parse_day(tz, g(&c, 2), today, roll)? else {
            return Ok(None);
        };
        let n = if future {
            day_diff(today, day)
        } else {
            day_diff(day, today)
        };
        return Ok((n >= 0.0).then(|| PhraseAnswer {
            display: plural(n, "day"),
            n: Some(n),
        }));
    }
    if let Some(b) = caps(&BETWEEN, s).or_else(|| caps(&FROM_TO, s)) {
        let a = parse_day(tz, g(&b, 1), today, Roll::Year)?;
        let z = parse_day(tz, g(&b, 2), today, Roll::Year)?;
        let (Some(a), Some(z)) = (a, z) else {
            return Ok(None);
        };
        let n = day_diff(a, z);
        if jsre::test(&SIGNED, s) && n < 0.0 {
            return Ok(None);
        }
        return Ok(Some(PhraseAnswer {
            display: plural(n.abs(), "day"),
            n: Some(n.abs()),
        }));
    }

    // relative to now
    if let Some(a) = caps(&AGO, s) {
        return Ok(parse_span(g(&a, 1))
            .and_then(|span| shift_now(tz, now, span, -1.0))
            .map(PhraseAnswer::shown));
    }
    if let Some(f) = caps(&FROM_NOW, s).or_else(|| caps(&IN, s)) {
        return Ok(parse_span(g(&f, 1))
            .and_then(|span| shift_now(tz, now, span, 1.0))
            .map(PhraseAnswer::shown));
    }
    if let Some(a) = caps(&AROUND, s) {
        let span = parse_span(g(&a, 1));
        let day = parse_day(tz, g(&a, 3), today, Roll::Year)?;
        if let (Some(span), Some(day)) = (span, day) {
            let sign = if g(&a, 2) == "before" { -1.0 } else { 1.0 };
            return Ok(shift_day(tz, day, span, sign).map(|d| PhraseAnswer::shown(format_date(tz, d))));
        }
    }

    // a date on its own, or plus/minus a calendar span
    if let Some(bare) = parse_day(tz, s, today, Roll::Year)? {
        return shown(format_date(tz, bare));
    }
    if let Some(a) = &arith {
        let day = parse_day(tz, g(a, 1), today, Roll::Year)?;
        let span = parse_span(g(a, 3));
        if let (Some(day), Some(span)) = (day, span) {
            let sign = if g(a, 2) == "+" { 1.0 } else { -1.0 };
            return Ok(shift_day(tz, day, span, sign).map(|d| PhraseAnswer::shown(format_date(tz, d))));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_and_spans() {
        assert_eq!(format_clock(-60.0), "11:00 pm");
        assert_eq!(format_clock(0.0), "12:00 am");
        assert_eq!(format_clock(f64::NAN), "NaN:NaN pm");
        assert_eq!(format_duration(120.0), "2 hr");
        assert_eq!(format_duration(45.0), "45 min");
        assert_eq!(parse_clock("7:05 p.m."), Some(1145.0));
        assert_eq!(parse_clock("12:30"), None);
        assert_eq!(parse_span("1.5 days"), None);
        assert_eq!(parse_span("1.5 hr").map(|s| s.minutes), Some(90.0));
        assert_eq!(parse_span("2 days 3 hr"), None);
        assert_eq!(parse_span("1 year and 2 months").map(|s| s.months), Some(14.0));
        assert_eq!(parse_span("an hour, 5 min").map(|s| s.minutes), Some(65.0));
        assert_eq!(parse_span("1 hr 1 hr"), None);
    }

    #[test]
    fn prototype_months_show_as_the_ts_does() {
        let tz = super::super::time_zone("America/Los_Angeles").unwrap();
        let today = Day {
            y: 2026.0,
            m: 8.0,
            d: 24.0,
        };
        let day = parse_day(&tz, "constructor 5", today, Roll::Year).unwrap().unwrap();
        assert_eq!(format_date(&tz, day), "undefined 5 undefined 2026");
        assert!(parse_day(&tz, "constructor", today, Roll::Year).is_err());
    }
}
