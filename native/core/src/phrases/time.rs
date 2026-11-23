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
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];
const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
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
        let min = if m.get(2).is_some() {
            num(g(&m, 2))
        } else {
            0.0
        };
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
    // native only: a span worked out on the line, `.8*2hr`, `2hr*.8`, `(2 * 3) days`, `90/60 hr`
    if let Some(span) = worked_span(t) {
        return Some(span);
    }
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

/// native only: numbers multiplied and divided, in brackets or not, with one span unit among them: `.8*2hr` is
/// 96 min, `2hr*.8` too, `(2 * 3) days` 6 days. A plain `4 hr 10 min` is left to `parse_span`'s own reading.
fn worked_span(t: &str) -> Option<Span> {
    if !t.contains(['*', '/', '(']) {
        return None;
    }
    let start = t.find(|c: char| c.is_ascii_alphabetic())?;
    let len = t[start..]
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(t.len() - start);
    let word = &t[start..start + len];
    if t[start + len..].contains(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let (key, per) = span_unit(word)?;
    // the unit stands for 1, so `2hr*.8` is 2*1*.8 hours; a unit glued to a number (`2hr`) multiplies it
    let rest = format!("{}{}", &t[..start], &t[start + len..]);
    let rest = js_trim(&rest);
    let value = if rest.is_empty() {
        1.0
    } else {
        let mut p = Arith {
            s: rest.as_bytes(),
            i: 0,
        };
        let v = p.product()?;
        p.space();
        (p.i == p.s.len()).then_some(v)?
    };
    let add = value * per;
    // whole minutes, days or months, allowing for float noise (1/3 hr is 20 min)
    let whole = add.round();
    if !add.is_finite() || (add - whole).abs() > 1e-9 * whole.abs().max(1.0) {
        return None;
    }
    let mut span = Span::default();
    match key {
        SpanKey::Minutes => span.minutes = whole,
        SpanKey::Days => span.days = whole,
        SpanKey::Months => span.months = whole,
    }
    Some(span)
}

/// numbers with `*`, `/`, brackets and juxtaposition (`2 (3)`), for `worked_span`
struct Arith<'a> {
    s: &'a [u8],
    i: usize,
}

impl Arith<'_> {
    fn space(&mut self) {
        while self.s.get(self.i) == Some(&b' ') {
            self.i += 1;
        }
    }
    fn product(&mut self) -> Option<f64> {
        let mut v = self.factor()?;
        loop {
            self.space();
            match self.s.get(self.i) {
                Some(b'*') => {
                    self.i += 1;
                    v *= self.factor()?;
                }
                Some(b'/') => {
                    self.i += 1;
                    v /= self.factor()?;
                }
                // a number or bracket right after another, where the unit was taken out (`2 (3)`, `.8*2 `)
                Some(c) if c.is_ascii_digit() || *c == b'.' || *c == b'(' => v *= self.factor()?,
                _ => return Some(v),
            }
        }
    }
    fn factor(&mut self) -> Option<f64> {
        self.space();
        if self.s.get(self.i) == Some(&b'(') {
            self.i += 1;
            let v = self.product()?;
            self.space();
            if self.s.get(self.i) != Some(&b')') {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let from = self.i;
        while self
            .s
            .get(self.i)
            .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
        {
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[from..self.i]).ok()?;
        if text.is_empty() || text == "." {
            return None;
        }
        text.parse::<f64>().ok()
    }
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
        "est" | "edt" | "eastern" | "new york" | "nyc" | "boston" | "miami" | "atlanta"
        | "washington" => "America/New_York",
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
    Some(PhraseAnswer::shown(format_clock(
        f.hours * 60.0 + f.minutes,
    )))
}

static ZONE_PAIR: Lazy =
    Lazy::new(|| js_re(r"^(.+?)\s+(?:in|to|into|as)\s+(.+?)(?:\s+time)?$", ""));

/// `3pm est` / `9am london time` / `3pm`: the clock and the zone after it, if any.
fn said_clock(t: &str) -> Option<(f64, Option<TimeZone>)> {
    static FROM: Lazy = Lazy::new(|| js_re(r"^(.+?)(?:\s+time)?$", ""));
    let t = js_trim(t);
    if let Some(c) = parse_clock(t).or_else(|| loose_clock(t)) {
        return Some((c, None));
    }
    let body = g(&caps(&FROM, t)?, 1);
    // `3pm new york`, `3:30 pm est`: the first split that reads as a clock and then a place
    body.match_indices(' ').find_map(|(i, _)| {
        let c = parse_clock(&body[..i]).or_else(|| loose_clock(&body[..i]))?;
        Some((c, Some(any_zone(&body[i + 1..])?)))
    })
}

/// Native only: a clock time in one zone read in another. `3pm est in pst`, `9am london time to tokyo`, `3pm in
/// tokyo` (from the user's own zone), `3pm tokyo time` (into it). Today's date in the zone it was said in; when
/// the other zone is on another day, the answer says which (`8:00 am next day`).
pub fn zone_convert(s: &str, now: f64, here: &TimeZone) -> Option<PhraseAnswer> {
    if now.is_nan() {
        return None;
    }
    let (clock, from, to) = if let Some(m) = caps(&ZONE_PAIR, s) {
        let (clock, from) = said_clock(g(&m, 1))?;
        (
            clock,
            from.unwrap_or_else(|| here.clone()),
            any_zone(g(&m, 2))?,
        )
    } else {
        // `3pm tokyo time`: into the user's zone; a bare clock is the TS's own answer
        let (clock, from) = said_clock(s)?;
        (clock, from?, here.clone())
    };
    let start = jiff::Timestamp::from_millisecond(now as i64)
        .ok()?
        .to_zoned(from.clone());
    let minutes = clock as i64;
    let at = start
        .date()
        .at((minutes / 60) as i8, (minutes % 60) as i8, 0, 0)
        .to_zoned(from)
        .ok()?;
    let there = at.with_time_zone(to);
    let shown = zoned_clock(&there);
    let shown = match there.date().cmp(&at.date()) {
        std::cmp::Ordering::Greater => format!("{shown} next day"),
        std::cmp::Ordering::Less => format!("{shown} previous day"),
        std::cmp::Ordering::Equal => shown,
    };
    Some(PhraseAnswer::shown(shown))
}

/// A zoned moment's clock as the phrases show one (`3:00 pm`).
fn zoned_clock(at: &jiff::Zoned) -> String {
    format_clock(at.hour() as f64 * 60.0 + at.minute() as f64)
}

/// Whether a parsed day is a real date (no NaN year, month or day).
fn finite_day(day: &Day) -> bool {
    day.y.is_finite() && day.m.is_finite() && day.d.is_finite()
}

/// Native only: `utc+5:30`, `gmt-8`, `utc+5` as a fixed offset from UTC.
fn offset_zone(key: &str) -> Option<TimeZone> {
    static RE: Lazy = Lazy::new(|| js_re(r"^(?:utc|gmt)\s*([+-])\s*(\d{1,2})(?::?(\d{2}))?$", ""));
    let m = caps(&RE, js_trim(key))?;
    let h: i32 = g(&m, 2).parse().ok()?;
    let min: i32 = m.get(3).map_or(Some(0), |x| x.as_str().parse().ok())?;
    if h > 14 || min > 59 {
        return None;
    }
    let secs = (h * 3600 + min * 60) * if g(&m, 1) == "-" { -1 } else { 1 };
    Some(TimeZone::fixed(jiff::tz::Offset::from_seconds(secs).ok()?))
}

/// A named zone or a fixed offset.
fn any_zone(key: &str) -> Option<TimeZone> {
    zone(js_trim(key))
        .and_then(super::time_zone)
        .or_else(|| offset_zone(key))
}

/// Native only: an ISO date (`2027-01-01`) inside a phrase, written as the phrases read one (`january 1 2027`).
fn iso_dates(s: &str) -> Option<String> {
    static RE: Lazy = Lazy::new(|| js_re(r"\b(\d{4})-(\d{1,2})-(\d{1,2})\b", ""));
    let mut any = false;
    let out = jsre::replace_all(&RE, s, |m| {
        let mon: usize = g(m, 2).parse().unwrap_or(0);
        let d: u32 = g(m, 3).parse().unwrap_or(0);
        match (mon, d) {
            (1..=12, 1..=31) => {
                any = true;
                format!("{} {d} {}", MONTHS[mon - 1], g(m, 1))
            }
            _ => g(m, 0).to_string(),
        }
    })
    .into_owned();
    any.then_some(out)
}

/// Native only: phrases about now the web leaves blank, tried after it answered nothing (`s` normalized).
/// - `now` / `date and time`: today and the clock; `time` / `what time is it`: the clock; `date` / `what day is
///   it`: today
/// - `what day is july 4 2027` / `what day of the week is christmas`: the date with its weekday
/// - `week number` (ISO 8601, `week 48`), `week number of july 4 2027`
/// - `unix time` / `unix timestamp` / `epoch time`: seconds since 1970
/// - `time in utc+5:30` / `time in gmt-8`, and the dates of the web's phrases written ISO-style (`days until
///   2027-01-01`)
pub fn native_time(s: &str, now: f64, tz: &TimeZone) -> Option<PhraseAnswer> {
    static NOW: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:right )?now|(?:the )?(?:current )?date and time|(?:the )?(?:current )?time and date|(?:the )?date and time now)$",
            "",
        )
    });
    static CLOCK: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:the )?(?:current )?time(?: now)?|what time is it(?: now)?|the time is|time is it)$",
            "",
        )
    });
    static TODAY: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:the |today's |todays |current |today )?date|what day is (?:it|today)(?: today)?|what's today|which day is it|day today|what is today)$",
            "",
        )
    });
    static WHAT_DAY: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:what|which) (?:day(?: of the week)?|weekday|date) (?:is|was|will be|falls on|does) (.+?)(?: fall on| be| on)?$",
            "",
        )
    });
    static WEEK: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:the |current |iso )?week (?:number|no\.?|#)|week of (?:the )?year|what week is it|which week is it)(?:\s+(?:of|for|on)\s+(.+))?$",
            "",
        )
    });
    static UNIX: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:current |the )?(?:unix|epoch|posix)(?: time(?:stamp)?| seconds)?|(?:current )?timestamp|seconds since (?:the )?(?:epoch|1970))(?: now)?$",
            "",
        )
    });
    static IN_ZONE: Lazy = Lazy::new(|| {
        js_re(
            r"^(?:(?:the )?(?:current )?time|what time is it|what time) (?:is it )?in (.+)$",
            "",
        )
    });
    if now.is_nan() {
        return None;
    }
    let s = js_trim(s);
    let today = day_of(tz, now);
    let f = local_fields(tz, now);
    let clock = format_clock(f.hours * 60.0 + f.minutes);
    if jsre::test(&NOW, s) {
        return Some(PhraseAnswer::shown(format!(
            "{}, {clock}",
            format_date(tz, today)
        )));
    }
    if jsre::test(&CLOCK, s) {
        return Some(PhraseAnswer::shown(clock));
    }
    if jsre::test(&TODAY, s) {
        return Some(PhraseAnswer::shown(format_date(tz, today)));
    }
    if jsre::test(&UNIX, s) {
        let secs = (now / 1000.0).floor();
        return Some(PhraseAnswer {
            display: jsnum::to_string(secs),
            n: Some(secs),
        });
    }
    let iso = iso_dates(s);
    let s = iso.as_deref().unwrap_or(s);
    if let Some(m) = caps(&WEEK, s) {
        let day = match m.get(1) {
            Some(d) => parse_day(tz, d.as_str(), today, Roll::Year).ok()??,
            None => today,
        };
        if !finite_day(&day) {
            return None;
        }
        let date = jiff::civil::Date::new(day.y as i16, day.m as i8 + 1, day.d as i8).ok()?;
        let week = date.iso_week_date().week();
        return Some(PhraseAnswer {
            display: format!("week {week}"),
            n: Some(f64::from(week)),
        });
    }
    if let Some(m) = caps(&WHAT_DAY, s) {
        let day = parse_day(tz, g(&m, 1), today, Roll::Year).ok()??;
        return Some(PhraseAnswer::shown(format_date(tz, day)));
    }
    if let Some(m) = caps(&IN_ZONE, s) {
        let there = offset_zone(g(&m, 1))?;
        let at = jiff::Timestamp::from_millisecond(now as i64)
            .ok()?
            .to_zoned(there);
        return Some(PhraseAnswer::shown(zoned_clock(&at)));
    }
    if let Some(a) = now_shift(s, now, tz)
        .or_else(|| now_in(s, now))
        .or_else(|| day_at_clock(s, now, tz))
        .or_else(|| day_minus_day(s, now, tz))
    {
        return Some(a);
    }
    // `days until 2027-01-01`: the web's own phrase, with the date written as it reads one
    iso.as_deref()
        .and_then(|t| answer_time_phrase(t, now, tz).ok().flatten())
}

/// A moment as the phrases show one: the clock alone when it is on `today`, else the date and the clock.
fn moment(tz: &TimeZone, at: f64, today: Option<Day>) -> Option<String> {
    if at.is_nan() {
        return None;
    }
    let f = local_fields(tz, at);
    let clock = format_clock(f.hours * 60.0 + f.minutes);
    let day = day_of(tz, at);
    Some(match today {
        Some(t) if compare_days(day, t) == 0.0 => clock,
        _ => format!("{}, {clock}", format_date(tz, day)),
    })
}

/// Native only (QA item 48): `now + 2 hours`, `now - 30 min`, `now + 1 day + 3 hr`: the moment after the spans,
/// shown as `2 hours from now` is (the clock today, else the date too); calendar spans alone keep the clock.
fn now_shift(s: &str, now: f64, tz: &TimeZone) -> Option<PhraseAnswer> {
    static RE: Lazy = Lazy::new(|| js_re(r"^(?:right )?now\s*([+-])\s*(.+)$", ""));
    let m = caps(&RE, s)?;
    let chain = span_chain(g(&m, 1), g(&m, 2))?;
    let mut at = now;
    for (sign, span) in &chain {
        let f = local_fields(tz, at);
        let day = shift_day(
            tz,
            day_of(tz, at),
            Span {
                minutes: 0.0,
                ..*span
            },
            *sign,
        )?;
        at = local_date(
            tz,
            day.y,
            day.m,
            day.d,
            f.hours,
            f.minutes + sign * span.minutes + f.seconds / 60.0,
        );
    }
    let today = day_of(tz, now);
    let clocked = chain.iter().any(|(_, span)| span.minutes != 0.0);
    moment(tz, at, clocked.then_some(today)).map(PhraseAnswer::shown)
}

/// Native only (QA item 48): `now in tokyo`: the date and the clock there.
fn now_in(s: &str, now: f64) -> Option<PhraseAnswer> {
    static RE: Lazy = Lazy::new(|| js_re(r"^(?:right )?now\s+(?:in|at)\s+(.+?)(?:\s+time)?$", ""));
    let there = any_zone(g(&caps(&RE, s)?, 1))?;
    moment(&there, now, None).map(PhraseAnswer::shown)
}

/// Native only (QA item 48): a day and a clock time, `tomorrow 3pm`, `3pm tomorrow`, `friday at 9:30 am est`,
/// optionally read in another zone (`tomorrow 3pm in tokyo`). Shown with its date (in the zone it is read in).
fn day_at_clock(s: &str, now: f64, here: &TimeZone) -> Option<PhraseAnswer> {
    static AT: Lazy = Lazy::new(|| js_re(r"^(?:on\s+)?(.+?)(?:,?\s+at)?$", ""));
    let today = day_of(here, now);
    let day_clock = |t: &str| -> Option<(Day, f64, Option<TimeZone>)> {
        let t = js_trim(t);
        t.match_indices(' ').find_map(|(i, _)| {
            let (a, b) = (&t[..i], js_trim(&t[i + 1..]));
            let b = b.strip_prefix("at ").unwrap_or(b);
            let day = |d: &str| {
                let d = g(&caps(&AT, js_trim(d))?, 1);
                parse_day(here, d, today, Roll::Future).ok().flatten()
            };
            if let (Some(d), Some((c, z))) = (day(a), said_clock(b)) {
                return Some((d, c, z));
            }
            // `3pm tomorrow`, `9am est on friday`
            let (c, z) = said_clock(a)?;
            let b = b.strip_prefix("on ").unwrap_or(b);
            Some((day(b)?, c, z))
        })
    };
    let (day, clock, from, to) = match caps(&ZONE_PAIR, s)
        .and_then(|m| Some((day_clock(g(&m, 1))?, any_zone(g(&m, 2))?)))
    {
        Some(((day, clock, from), to)) => (day, clock, from.unwrap_or_else(|| here.clone()), to),
        None => {
            // `tomorrow 3pm` here, or `tomorrow 3pm tokyo time` into the user's zone
            let (day, clock, from) = day_clock(s)?;
            (
                day,
                clock,
                from.unwrap_or_else(|| here.clone()),
                here.clone(),
            )
        }
    };
    if !finite_day(&day) {
        return None;
    }
    let at = local_date(&from, day.y, day.m, day.d, 0.0, clock);
    moment(&to, at, None).map(PhraseAnswer::shown)
}

/// Native only (QA item 48): `2027-01-01 - today`, `today - march 1`: the signed days from the second to the
/// first.
fn day_minus_day(s: &str, now: f64, tz: &TimeZone) -> Option<PhraseAnswer> {
    static RE: Lazy = Lazy::new(|| js_re(r"^(.+?)\s*-\s*(.+)$", ""));
    let m = caps(&RE, s)?;
    let today = day_of(tz, now);
    let day = |t: &str| match js_trim(t) {
        "now" | "right now" => Some(today),
        t => parse_day(tz, t, today, Roll::Year).ok().flatten(),
    };
    let (a, b) = (day(g(&m, 1))?, day(g(&m, 2))?);
    let n = day_diff(b, a);
    n.is_finite().then(|| PhraseAnswer {
        display: plural(n, "day"),
        n: Some(n),
    })
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
        js_re(
            r"^([a-z]+)\.?\s+(\d{1,2})(?:st|nd|rd|th)?(?:,?\s+(\d{4}))?$",
            "",
        )
    });
    static DM: Lazy = Lazy::new(|| {
        js_re(
            r"^(\d{1,2})(?:st|nd|rd|th)?\s+(?:of\s+)?([a-z]+)\.?(?:,?\s+(\d{4}))?$",
            "",
        )
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
    // native only: a date as answers show it, `Wed 11 Nov 2026`, so `+ 1 week` continues from one; the weekday
    // is the date's own and is skipped
    static LEAD_WEEKDAY: Lazy = Lazy::new(|| js_re(r"^([a-z]+)\.?,?\s+(?=.*\d)", ""));
    if let Some(lead) = caps(&LEAD_WEEKDAY, t) {
        let w = g(&lead, 1);
        if WEEKDAYS.contains(&w) || short_weekday(w).is_some_and(|i| !i.is_nan()) {
            return parse_day(tz, &t[g(&lead, 0).len()..], today, roll);
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
            if ok
                && (if roll == Roll::Future {
                    c >= 0.0
                } else {
                    c <= 0.0
                })
            {
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

/// native only: the right side of `today + 6 days + 10 days`, `today - 3 days + 1 day` or `9:00 + (1 hr + 30 min)`:
/// each span with its sign, in order. `op` is the sign before the first one; a bracketed group takes it whole.
fn span_chain(op: &str, right: &str) -> Option<Vec<(f64, Span)>> {
    let first = if op == "+" { 1.0 } else { -1.0 };
    let t = js_trim(right);
    let (grouped, body) = match t.strip_prefix('(').and_then(|b| b.strip_suffix(')')) {
        Some(b) if !b.contains(['(', ')']) => (true, b),
        _ => (false, t),
    };
    let mut out = Vec::new();
    let (mut sign, mut start) = (first, 0);
    for (i, c) in body.char_indices().chain([(body.len(), '+')]) {
        if c == '+' || c == '-' {
            out.push((sign, parse_span(&body[start..i])?));
            let s = if c == '+' { 1.0 } else { -1.0 };
            sign = if grouped { first * s } else { s };
            start = i + 1;
        }
    }
    Some(out)
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
    static COUNT: Lazy =
        Lazy::new(|| re(r"^(?:how many\s+)?days?\s+(until|till|to|since)\s+(.+)$"));
    static BETWEEN: Lazy =
        Lazy::new(|| re(r"^(?:how many\s+)?days?\s+between\s+(.+?)\s+and\s+(.+)$"));
    static FROM_TO: Lazy =
        Lazy::new(|| re(r"^(?:how many\s+)?days?\s+from\s+(.+?)\s+(?:to|until)\s+(.+)$"));
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
        if let (Some(t), Some(chain)) = (t, span_chain(op, right))
            && chain
                .iter()
                .all(|(_, span)| span.minutes != 0.0 && span.days == 0.0 && span.months == 0.0)
        {
            let m: f64 = chain.iter().map(|(sign, span)| sign * span.minutes).sum();
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
        return Ok(
            (left > 0.0 && left < 1440.0).then(|| PhraseAnswer::shown(format_duration(left)))
        );
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
            return Ok(
                shift_day(tz, day, span, sign).map(|d| PhraseAnswer::shown(format_date(tz, d)))
            );
        }
    }

    // a date on its own, or plus/minus a calendar span
    if let Some(bare) = parse_day(tz, s, today, Roll::Year)? {
        return shown(format_date(tz, bare));
    }
    if let Some(a) = &arith {
        let day = parse_day(tz, g(a, 1), today, Roll::Year)?;
        if let (Some(day), Some(chain)) = (day, span_chain(g(a, 2), g(a, 3))) {
            let shifted = chain
                .into_iter()
                .try_fold(day, |d, (sign, span)| shift_day(tz, d, span, sign));
            return Ok(shifted.map(|d| PhraseAnswer::shown(format_date(tz, d))));
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
        assert_eq!(
            parse_span("1 year and 2 months").map(|s| s.months),
            Some(14.0)
        );
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
        let day = parse_day(&tz, "constructor 5", today, Roll::Year)
            .unwrap()
            .unwrap();
        assert_eq!(format_date(&tz, day), "undefined 5 undefined 2026");
        assert!(parse_day(&tz, "constructor", today, Roll::Year).is_err());
    }
}
