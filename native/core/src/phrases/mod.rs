//! Natural-language phrases (`src/engine/phrases.ts`): money, tips, splits, percentages, currency conversions,
//! arithmetic said in words, and (in [`time`]) clock times and dates. They once came from SoulverCore on the Mac;
//! `phrases.soulver.test.ts` keeps what it answered. Every pattern matches the whole line; anything looser stays
//! blank.
//!
//! Nothing here reads the clock: `now` (a JS time value, ms since 1970) and the time zone come in through
//! [`PhraseOptions`].
//!
//! Kept quirk, all through this module: the TS looks words up in plain objects (`WORDS[word]`), so a word naming
//! an `Object.prototype` member (`constructor`, `__proto__`) finds a function or an object instead of nothing.
//! [`proto_value`] reproduces those lookups and their `String()`, and the paths where the TS then throws a
//! `TypeError` return [`Thrown`] from [`try_answer_phrase`].

pub mod currency;
pub mod golden;
pub mod jsdate;
pub mod live;
pub mod money;
pub mod native_eval;
pub mod rates;
pub mod time;

use std::borrow::Cow;

pub use jiff::tz::TimeZone;

use crate::format::{DEFAULT_SIG_FIGS, format_number};
use crate::jsnum;
use crate::jsre::{self, Caps, Lazy, Regex};
use crate::types::{EvaluateOptions, ValueKind};
use currency::{
    Currency, CurrencyRates, Money, convert_currency, currency_word, format_money, parse_money,
    parse_number,
};

/// `PhraseAnswer`: what the bar shows, and the number `ans` takes (none for a percentage or a clock time).
#[derive(Clone, Debug, PartialEq)]
pub struct PhraseAnswer {
    pub display: String,
    pub n: Option<f64>,
}

impl PhraseAnswer {
    fn shown(display: impl Into<String>) -> Self {
        PhraseAnswer {
            display: display.into(),
            n: None,
        }
    }

    /// The TS JSON shape, `{display, n?}`.
    pub fn to_json(&self) -> serde_json::Value {
        let mut o = serde_json::json!({ "display": self.display });
        if let Some(n) = self.n {
            o["n"] = serde_json::json!(n);
        }
        o
    }
}

/// `PhraseOptions`, with the clock made explicit.
#[derive(Clone, Copy, Debug)]
pub struct PhraseOptions<'a> {
    /// `now` as a JS time value (ms since 1970, UTC).
    pub now_ms: f64,
    /// The zone the user's clock reads in (the TS uses the process's local zone).
    pub time_zone: &'a TimeZone,
    pub rates: Option<&'a CurrencyRates>,
    pub sig_figs: Option<f64>,
}

/// Where the TS throws a `TypeError` (a word that names an `Object.prototype` member, see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thrown(pub String);

pub type Answered = Result<Option<PhraseAnswer>, Thrown>;

/// A time zone from the embedded tz database, by IANA name.
pub fn time_zone(name: &str) -> Option<TimeZone> {
    jiff::tz::db().get(name).ok()
}

// ---- JS text helpers ----

// JS `\s` (and `String.prototype.trim`): Unicode spaces and line ends plus U+FEFF, without U+0085
const JS_SPACE: &str =
    r"\t\n\x0B\x0C\r \xA0\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `s.trim()`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// A TS regex source compiled with JS meanings for `\s`, `\S` and `.` (which stops at line ends), on top of
/// what [`jsre::js`] translates.
pub fn js_re(pattern: &str, flags: &str) -> Regex {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut in_class = false;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('s') if in_class => out.push_str(JS_SPACE),
                Some('s') => out.push_str(&format!("[{JS_SPACE}]")),
                Some('S') if !in_class => out.push_str(&format!("[^{JS_SPACE}]")),
                Some(e) => {
                    out.push('\\');
                    out.push(e);
                }
                None => out.push('\\'),
            },
            '[' if !in_class => {
                in_class = true;
                out.push(c);
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            '.' if !in_class => out.push_str(r"[^\n\r\x{2028}\x{2029}]"),
            _ => out.push(c),
        }
    }
    jsre::js(&out, flags)
}

/// The text a case-insensitive JS regex (no `u` flag) sees: there only ASCII letters fold, where Rust's `(?i)`
/// also folds the long s and the Kelvin sign into `s` and `k`. Neither is a word character to JS, so a NUL in
/// their place tests the same. For `test` only.
pub(crate) fn ascii_fold_safe(s: &str) -> Cow<'_, str> {
    if s.contains(['\u{17f}', '\u{212a}']) {
        Cow::Owned(s.replace(['\u{17f}', '\u{212a}'], "\0"))
    } else {
        Cow::Borrowed(s)
    }
}

/// `s.replace(re, f)` without the `g` flag.
pub fn replace_first<'t>(
    re: &Regex,
    s: &'t str,
    f: impl FnOnce(&Caps<'_>) -> String,
) -> Cow<'t, str> {
    match re.captures(s) {
        Ok(Some(caps)) => {
            let m = caps.get(0).expect("group 0");
            Cow::Owned(format!("{}{}{}", &s[..m.start()], f(&caps), &s[m.end()..]))
        }
        _ => Cow::Borrowed(s),
    }
}

/// The `String()` of `Object.prototype[key]`, when `key` names one of its members, which is what a TS lookup in a
/// plain object (`HOLIDAYS[key]`, `WORDS[key]`) finds for a key the object doesn't have.
pub fn proto_value(key: &str) -> Option<&'static str> {
    Some(match key {
        "constructor" => "function Object() { [native code] }",
        "__proto__" => "[object Object]",
        "__defineGetter__" => "function __defineGetter__() { [native code] }",
        "__defineSetter__" => "function __defineSetter__() { [native code] }",
        "__lookupGetter__" => "function __lookupGetter__() { [native code] }",
        "__lookupSetter__" => "function __lookupSetter__() { [native code] }",
        "hasOwnProperty" => "function hasOwnProperty() { [native code] }",
        "isPrototypeOf" => "function isPrototypeOf() { [native code] }",
        "propertyIsEnumerable" => "function propertyIsEnumerable() { [native code] }",
        "toString" => "function toString() { [native code] }",
        "toLocaleString" => "function toLocaleString() { [native code] }",
        "valueOf" => "function valueOf() { [native code] }",
        _ => return None,
    })
}

fn is_proto_currency(c: Currency) -> bool {
    c.len() != 3
}

fn whole(pattern: &str) -> Regex {
    js_re(&format!("^{pattern}$"), "")
}

fn caps<'t>(re: &Regex, s: &'t str) -> Option<Caps<'t>> {
    re.captures(s).ok().flatten()
}

fn g<'t>(m: &Caps<'t>, i: usize) -> &'t str {
    jsre::group(m, i)
}

// ---- phrases.ts ----

const PCT: &str = concat!(
    r"(",
    r"(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?|\.\d+",
    r")\s*%"
);
const LABEL: &str = r"(?:\s+(?:for\s+)?(?:lunch|dinner|breakfast|brunch|drinks|coffee))?";

fn normalize(expr: &str) -> String {
    static DASH: Lazy = Lazy::new(|| js_re("[−–]", ""));
    static SPACES: Lazy = Lazy::new(|| js_re(r"\s+", ""));
    static ASKED: Lazy = Lazy::new(|| js_re(r"\s*[?=]$", ""));
    static WHAT: Lazy = Lazy::new(|| js_re(r"^(?:what is|what's|whats)\s+", ""));
    static PCT_AFTER: Lazy = Lazy::new(|| js_re(r"(\d)\s*(?:percentage|per\s?cent|percent)\b", ""));
    static PCT_WORD: Lazy = Lazy::new(|| js_re(r"\b(?:percentage|per\s?cent|percent)\b", ""));
    static PLUS: Lazy = Lazy::new(|| js_re(r"\bplus\b", ""));
    static MINUS: Lazy = Lazy::new(|| js_re(r"\bminus\b", ""));
    let s = js_trim(expr).to_lowercase();
    let s = jsre::replace_all(&DASH, &s, |_| "-".into()).into_owned();
    let s = jsre::replace_all(&SPACES, &s, |_| " ".into()).into_owned();
    let s = replace_first(&ASKED, &s, |_| String::new()).into_owned();
    let s = replace_first(&WHAT, &s, |_| String::new()).into_owned();
    let s = jsre::replace_all(&PCT_AFTER, &s, |c| format!("{}%", g(c, 1))).into_owned();
    let s = jsre::replace_all(&PCT_WORD, &s, |_| "%".into()).into_owned();
    let s = jsre::replace_all(&PLUS, &s, |_| "+".into()).into_owned();
    let s = jsre::replace_all(&MINUS, &s, |_| "-".into()).into_owned();
    js_trim(&s).to_string()
}

/// `sameCurrency`: `Err` for two different currencies.
fn same_currency(a: &Money, b: &Money) -> Result<Option<Currency>, ()> {
    match (a.currency, b.currency) {
        (Some(x), Some(y)) if x != y => Err(()),
        (x, y) => Ok(x.or(y)),
    }
}

fn amount(n: f64, currency: Option<Currency>, sig_figs: f64) -> Option<PhraseAnswer> {
    if !n.is_finite() {
        return None;
    }
    let Some(currency) = currency else {
        return Some(PhraseAnswer {
            display: format_number(n, sig_figs),
            n: Some(n),
        });
    };
    let display = format_money(n, currency);
    let digits: String = display
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    // `Number(...)`: an empty string is 0
    let n = jsnum::parse(&digits).unwrap_or(f64::NAN);
    Some(PhraseAnswer {
        display,
        n: Some(n),
    })
}

// `$60 + 18% tip`, `$10 for lunch + 15% tip`, `60 with a 20% tip`
fn tip_total(s: &str) -> Option<Money> {
    static RE: Lazy = Lazy::new(|| {
        whole(&format!(
            r"(.+?){LABEL}\s*(?:\+|with(?: a)?|and(?: a)?)\s*{PCT}\s+tip"
        ))
    });
    let m = caps(&RE, s)?;
    let base = parse_money(g(&m, 1), false)?;
    let p = parse_number(g(&m, 2))?;
    Some(Money {
        n: base.n * (1.0 + p / 100.0),
        currency: base.currency,
    })
}

// a quantity with a unit (`50 kg`, `3.5 km`), which the percent phrases take as well as money. the unit stays as
// typed; the engine only says it is one
fn quantity(text: &str) -> Option<(f64, String)> {
    // JS `[a-zµ°]` with the `i` flag: ASCII letters both ways, and µ with every letter that upper-cases to Μ
    static RE: Lazy = Lazy::new(|| {
        js_re(
            r"^(-?\d*\.?\d+(?:[eE][+-]?\d+)?)\s*([a-zA-ZµμΜ°][a-zA-ZµμΜ°²³/^0-9]*(?: [a-zA-ZµμΜ°][a-zA-ZµμΜ°²³/^0-9]*)?)$",
            "",
        )
    });
    let m = caps(&RE, js_trim(text))?;
    let v = crate::evaluate_line(text, &EvaluateOptions::default()).value?;
    (v.kind == ValueKind::Number && v.unit.as_deref().is_some_and(|u| !u.is_empty())).then(|| {
        (
            jsnum::parse(g(&m, 1)).unwrap_or(f64::NAN),
            g(&m, 2).to_string(),
        )
    })
}

// the base scaled by `factor`: money as money, a quantity in its own unit
fn scaled_base(text: &str, factor: f64, sig_figs: f64) -> Option<PhraseAnswer> {
    if let Some(money) = parse_money(text, false) {
        return amount(money.n * factor, money.currency, sig_figs);
    }
    let (q, unit) = quantity(text)?;
    let n = q * factor;
    Some(PhraseAnswer {
        display: format!("{} {unit}", format_number(n, sig_figs)),
        n: Some(n),
    })
}

// `ans` would have to guess between 25 and 0.25, so a percentage carries no number
fn percent(n: f64, sig_figs: f64) -> Option<PhraseAnswer> {
    n.is_finite()
        .then(|| PhraseAnswer::shown(format!("{}%", format_number(n, sig_figs))))
}

fn percent_phrase(s: &str, sig_figs: f64) -> Option<PhraseAnswer> {
    static IS_OF: Lazy = Lazy::new(|| whole(&format!(r"(.+?) is {PCT} of what")));
    static OF_IS: Lazy = Lazy::new(|| whole(&format!(r"{PCT} of what is (.+)")));
    static BY: Lazy = Lazy::new(|| {
        whole(&format!(
            r"(increase|raise|decrease|reduce|lower) (.+?) by {PCT}"
        ))
    });
    static OFF: Lazy = Lazy::new(|| whole(&format!(r"{PCT} off (.+)")));
    static ON_OF: Lazy = Lazy::new(|| whole(&format!(r"{PCT} (on|of) (.+)")));
    static SIGNED: Lazy = Lazy::new(|| whole(&format!(r"(.+?){LABEL}\s*([+-])\s*{PCT}")));
    static ASKED: Lazy = Lazy::new(|| whole(r"what % of (.+?) is (.+)"));
    static STATED: Lazy = Lazy::new(|| whole(r"(.+?) is what % of (.+)"));
    static AS: Lazy = Lazy::new(|| whole(r"(.+?) as (?:an? )?% of (.+)"));
    static CHANGE: Lazy = Lazy::new(|| whole(r"% (?:change|difference) from (.+?) to (.+)"));

    // the whole from a part: `30 is 20% of what`, `20% of what is 30`
    let whole_of = caps(&IS_OF, s)
        .map(|m| (g(&m, 1), g(&m, 2)))
        .or_else(|| caps(&OF_IS, s).map(|m| (g(&m, 2), g(&m, 1))));
    if let Some((text, pct)) = whole_of {
        let p = parse_number(pct)?;
        if p == 0.0 {
            return None;
        }
        return scaled_base(text, 100.0 / p, sig_figs);
    }
    // `increase 50 by 10%`, `decrease $200 by 25%`
    if let Some(m) = caps(&BY, s) {
        let p = parse_number(g(&m, 3))?;
        let up = matches!(g(&m, 1), "increase" | "raise");
        if !up && p > 100.0 {
            return None;
        }
        let factor = if up { 1.0 + p / 100.0 } else { 1.0 - p / 100.0 };
        return scaled_base(g(&m, 2), factor, sig_figs);
    }
    if let Some(m) = caps(&OFF, s) {
        let p = parse_number(g(&m, 1))?;
        if p > 100.0 {
            return None;
        }
        return scaled_base(g(&m, 2), 1.0 - p / 100.0, sig_figs);
    }
    if let Some(m) = caps(&ON_OF, s) {
        let p = parse_number(g(&m, 1))?;
        let factor = if g(&m, 2) == "on" {
            1.0 + p / 100.0
        } else {
            p / 100.0
        };
        return scaled_base(g(&m, 3), factor, sig_figs);
    }
    if let Some(m) = caps(&SIGNED, s) {
        let p = parse_number(g(&m, 3))?;
        let factor = if g(&m, 2) == "+" {
            1.0 + p / 100.0
        } else {
            1.0 - p / 100.0
        };
        return scaled_base(g(&m, 1), factor, sig_figs);
    }
    // part and whole, in any of the three usual orders
    let ratio = caps(&ASKED, s).map(|m| (g(&m, 1), g(&m, 2))).or_else(|| {
        caps(&STATED, s)
            .or_else(|| caps(&AS, s))
            .map(|m| (g(&m, 2), g(&m, 1)))
    });
    if let Some((whole_text, part_text)) = ratio {
        let whole = parse_money(whole_text, false)?;
        let part = parse_money(part_text, false)?;
        if whole.n == 0.0 || same_currency(&whole, &part).is_err() {
            return None;
        }
        return percent((part.n / whole.n) * 100.0, sig_figs);
    }
    if let Some(m) = caps(&CHANGE, s) {
        let from = parse_money(g(&m, 1), false)?;
        let to = parse_money(g(&m, 2), false)?;
        if from.n == 0.0 || same_currency(&from, &to).is_err() {
            return None;
        }
        return percent(((to.n - from.n) / from.n) * 100.0, sig_figs);
    }
    None
}

// `60 split 3 ways`, `$60 / 3 people`, `$60 + 18% tip split 3 ways`
fn split_phrase(s: &str, sig_figs: f64) -> Option<PhraseAnswer> {
    static WAYS: Lazy = Lazy::new(|| {
        whole(r"(.+?) split (?:(\d+) ways|(?:between|among) (\d+)(?: people)?)(?: per person)?")
    });
    static PEOPLE: Lazy = Lazy::new(|| {
        whole(r"(.+?) (?:/|÷|divided by) (\d+) (?:people|persons|ways)(?: per person)?")
    });
    static PER: Lazy = Lazy::new(|| whole(r"(.+?) (?:/|÷|divided by) (\d+) per person"));
    static FOR: Lazy = Lazy::new(|| whole(r"(.+?) for (\d+) people per person"));
    let m = caps(&WAYS, s)
        .or_else(|| caps(&PEOPLE, s))
        .or_else(|| caps(&PER, s))
        .or_else(|| caps(&FOR, s))?;
    let count = m.get(2).or_else(|| m.get(3)).map_or("", |c| c.as_str());
    let ways = jsnum::parse(count).unwrap_or(f64::NAN);
    if !jsnum::is_integer(ways) || !(1.0..=1_000_000.0).contains(&ways) {
        return None;
    }
    let head = parse_money(g(&m, 1), false).or_else(|| tip_total(g(&m, 1)))?;
    amount(head.n / ways, head.currency, sig_figs)
}

// `$45.50 + $12.25`, `3 x $4.99`, `$100 + 8.25% tax`, `$30/hr * 40 hr`, `$50k / 12`, `5 dozen`
fn sum_phrase(s: &str, sig_figs: f64) -> Option<PhraseAnswer> {
    let sum = money::money_math(s)?;
    amount(sum.n, sum.currency, sig_figs)
}

// `$20 in euros`, `20 usd to gbp`, `£5 as yen`
fn conversion_phrase(s: &str, rates: Option<&CurrencyRates>) -> Answered {
    static RE: Lazy = Lazy::new(|| js_re(r"^(.+?)\s+(?:in|to|as|into)\s+(.+)$", ""));
    let Some(m) = caps(&RE, s) else {
        return Ok(None);
    };
    let Some(to) = currency_word(g(&m, 2), true) else {
        return Ok(None);
    };
    let Some(Money {
        n,
        currency: Some(from),
    }) = parse_money(g(&m, 1), true)
    else {
        return Ok(None);
    };
    let n = if from == to {
        Some(n)
    } else {
        // `toUpperCase` of a function or an object (a kept quirk: see the module docs)
        if rates.is_some() && n.is_finite() && (is_proto_currency(from) || is_proto_currency(to)) {
            return Err(Thrown("TypeError: toUpperCase is not a function".into()));
        }
        convert_currency(n, from, to, rates)
    };
    Ok(n.and_then(|n| amount(n, Some(to), DEFAULT_SIG_FIGS as f64)))
}

/// `answerPhrase`, or None where the TS throws (see [`try_answer_phrase`]).
pub fn answer_phrase(expr: &str, options: &PhraseOptions<'_>) -> Option<PhraseAnswer> {
    try_answer_phrase(expr, options).ok().flatten()
}

/// `answerPhrase`, with the TS's throws kept apart.
pub fn try_answer_phrase(expr: &str, options: &PhraseOptions<'_>) -> Answered {
    let s = normalize(expr);
    if s.is_empty() || s.encode_utf16().count() > 120 {
        return Ok(None);
    }
    let sig_figs = options.sig_figs.unwrap_or(DEFAULT_SIG_FIGS as f64);
    // first: `7 plus 2` would otherwise read as an amount of money and stop here
    if let Some(said) = word_math(&s, expr, sig_figs) {
        return Ok(Some(said));
    }
    if let Some(money) = parse_money(&s, false) {
        return Ok(money
            .currency
            .and_then(|c| amount(money.n, Some(c), sig_figs)));
    }
    if let Some(tip) = tip_total(&s) {
        return Ok(amount(tip.n, tip.currency, sig_figs));
    }
    if let Some(a) = split_phrase(&s, sig_figs).or_else(|| percent_phrase(&s, sig_figs)) {
        return Ok(Some(a));
    }
    if let Some(a) = conversion_phrase(&s, options.rates)? {
        return Ok(Some(a));
    }
    if let Some(a) = sum_phrase(&s, sig_figs) {
        return Ok(Some(a));
    }
    time::answer_time_phrase(&s, options.now_ms, options.time_zone)
}

// arithmetic said in words: `7 plus 2`, `10 divided by 4`, `3 squared`, `2 to the power of 10`, `square root of 81`,
// `half of 50`, `double 21`, `5 choose 2`. the words become symbols, and the line answers only when what they make
// is a plain number; `plus` and `minus` are symbols already (normalize)
fn word_math(s: &str, raw: &str, sig_figs: f64) -> Option<PhraseAnswer> {
    static SAID: Lazy = Lazy::new(|| {
        js_re(
            r"\b(?:plus|minus|divided|over|times|multiplied|added|squared|cubed|power|raised|root|modulo|half|third|quarter|double|twice|triple|choose|square|cube|\d+(?:st|nd|rd|th))\b",
            "i",
        )
    });
    static WHOLE_WORDS: crate::jsre::Lazy<Vec<(Regex, &'static str)>> =
        crate::jsre::Lazy::new(|| {
            [
                (r"^(\S+) choose (\S+)$", "nCr($1, $2)"),
                (r"^(?:the )?square of (.+)$", "($1)^2"),
                (r"^(?:the )?cube of (.+)$", "($1)^3"),
                (r"^half of (.+)$", "(1/2)*($1)"),
                (r"^(?:a|one) third of (.+)$", "(1/3)*($1)"),
                (r"^(?:a|one) quarter of (.+)$", "(1/4)*($1)"),
                (r"^(?:double|twice) (.+)$", "2*($1)"),
                (r"^triple (.+)$", "3*($1)"),
            ]
            .into_iter()
            .map(|(p, to)| (js_re(p, ""), to))
            .collect()
        });
    static OPERATOR_WORDS: crate::jsre::Lazy<Vec<(Regex, &'static str)>> =
        crate::jsre::Lazy::new(|| {
            [
                (r"\b(?:the )?square root of\b", "sqrt "),
                (r"\b(?:the )?cube root of\b", "cbrt "),
                (
                    r"\b(?:to the power of|raised to the power of|raised to)\b",
                    "^",
                ),
                (r"\bto the (\d+)(?:st|nd|rd|th)(?: power)?\b", "^$1"),
                (r"\bsquared\b", "^2"),
                (r"\bcubed\b", "^3"),
                (r"\b(?:divided by|over)\b", "/"),
                (r"\b(?:multiplied by|times)\b", "*"),
                (r"\b(?:added to)\b", "+"),
                (r"\bmodulo\b", "mod"),
            ]
            .into_iter()
            .map(|(p, to)| (js_re(p, ""), to))
            .collect()
        });
    static FUNCTIONS: Lazy = Lazy::new(|| js_re(r"\b(?:sqrt|cbrt|nCr|mod)\b", "i"));
    static WORD: Lazy = Lazy::new(|| js_re(r"[a-z]{2,}", ""));

    if !jsre::test(&SAID, &ascii_fold_safe(raw)) {
        return None;
    }
    // `$1` and `$2` in the TS replacement strings
    let fill = |to: &str, c: &Caps<'_>| to.replace("$1", g(c, 1)).replace("$2", g(c, 2));
    let mut text = s.to_string();
    for (re, to) in WHOLE_WORDS.iter() {
        text = replace_first(re, &text, |c| fill(to, c)).into_owned();
    }
    for (re, to) in OPERATOR_WORDS.iter() {
        text = jsre::replace_all(re, &text, |c| fill(to, c)).into_owned();
    }
    // `gi`: the long s and the Kelvin sign can't reach here, `s` is lower case and these are ASCII words
    let bare = jsre::replace_all(&FUNCTIONS, &text, |_| String::new());
    if jsre::test(&WORD, &bare) {
        return None;
    }
    let options = EvaluateOptions {
        sig_figs: Some(sig_figs),
        ..EvaluateOptions::default()
    };
    let v = crate::evaluate_line(&text, &options).value?;
    if v.kind != ValueKind::Number
        || v.unit.as_deref().is_some_and(|u| !u.is_empty())
        || !v.n.is_finite()
    {
        return None;
    }
    Some(PhraseAnswer {
        display: format_number(v.n, sig_figs),
        n: Some(v.n),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(expr: &str) -> Answered {
        let tz = time_zone("America/Los_Angeles").unwrap();
        let rates = CurrencyRates::new(
            "EUR",
            &[("USD".to_string(), 1.25), ("GBP".to_string(), 0.8)].into(),
        );
        let options = PhraseOptions {
            now_ms: golden::RECORDED_NOW_MS,
            time_zone: &tz,
            rates: Some(&rates),
            sig_figs: None,
        };
        try_answer_phrase(expr, &options)
    }

    fn shown(expr: &str) -> String {
        phrase(expr).unwrap().map(|a| a.display).unwrap_or_default()
    }

    #[test]
    fn answers_like_the_ts() {
        assert_eq!(shown("$60 + 18% tip split 3 ways"), "$23.60");
        assert_eq!(shown("$100 in gbp"), "£64.00");
        assert_eq!(shown("days until christmas"), "92 days");
        assert_eq!(shown("3 hours from now"), "6:12 pm");
        assert_eq!(shown("time in tokyo"), "7:12 am");
        assert_eq!(shown("next fri"), "Fri 25 Sep 2026");
        // kept quirks: plain-object lookups find Object.prototype's members
        assert!(phrase("constructor").is_err());
        assert_eq!(
            shown("5 constructor"),
            "5.00 function Object() { [native code] }"
        );
        assert_eq!(shown("next constructor"), "undefined NaN undefined NaN");
    }

    #[test]
    fn js_space_and_dot() {
        assert_eq!(js_trim("\u{feff} a \u{2028}"), "a");
        assert_eq!(js_trim("\u{85}a"), "\u{85}a");
        assert!(jsre::test(&js_re(r"^a\sb$", ""), "a\u{feff}b"));
        assert!(!jsre::test(&js_re(r"^a.b$", ""), "a\u{2028}b"));
        assert!(jsre::test(&js_re(r"^[\s,]+$", ""), " ,\u{a0}"));
    }
}
