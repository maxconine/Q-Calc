//! Native only: currency lines the web's phrases leave blank. Each is tried only after `answerPhrase` answered
//! nothing, so every recorded answer stays as it was.
//! - sums before a conversion: `$5 * 3 to eur`, `$20/hr * 40 hr in gbp`
//! - different currencies in one sum, answered in the first: `$5 + €3`, `£10 - $2 to yen`
//! - a currency on its own, as one of it: `usd to eur`

use super::currency::{Currency, CurrencyRates, Money, convert_currency, currency_word, parse_money};
use super::{PhraseAnswer, amount, caps, g, js_re, money, normalize};
use crate::format::format_number;
use crate::jsre::Lazy;

/// The line's terms at its top-level `+` and `-`, each with its sign; None for one term.
fn terms(s: &str) -> Option<Vec<(f64, &str)>> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut sign) = (0_i32, 0, 1.0);
    let mut last: Option<char> = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '+' | '-' if depth == 0 && last.is_some_and(|l| !"+-*/x×(".contains(l)) => {
                out.push((sign, s[start..i].trim()));
                sign = if c == '-' { -1.0 } else { 1.0 };
                start = i + c.len_utf8();
            }
            _ => {}
        }
        if !c.is_whitespace() {
            last = Some(c);
        }
    }
    out.push((sign, s[start..].trim()));
    (out.len() > 1).then_some(out)
}

/// One amount of money, from the web's own readings.
fn money_of(text: &str) -> Option<(f64, Currency)> {
    if let Some(Money {
        n,
        currency: Some(c),
    }) = parse_money(text, true)
    {
        return Some((n, c));
    }
    let sum = money::money_math(text)?;
    Some((sum.n, sum.currency?))
}

/// A sum of amounts in more than one currency, in the first one's.
fn mixed_sum(s: &str, rates: &CurrencyRates) -> Option<(f64, Currency)> {
    let parts = terms(s)?
        .into_iter()
        .map(|(sign, t)| money_of(t).map(|(n, c)| (sign * n, c)))
        .collect::<Option<Vec<_>>>()?;
    let first = parts.first()?.1;
    if parts.iter().all(|p| p.1 == first) {
        return None;
    }
    let total = parts
        .iter()
        .map(|&(n, c)| convert_currency(n, c, first, Some(rates)))
        .sum::<Option<f64>>()?;
    Some((total, first))
}

/// What the line is worth: a sum the web reads, or one in mixed currencies.
fn worth(s: &str, rates: &CurrencyRates) -> Option<(f64, Currency)> {
    money_of(s).or_else(|| mixed_sum(s, rates))
}

/// `usd to eur`: the rate, and the day it's from: `1 USD = 0.8909 EUR (Nov 5)`.
fn rate_line(from: Currency, to: Currency, rates: &CurrencyRates) -> Option<PhraseAnswer> {
    if from == to {
        return None;
    }
    let rate = convert_currency(1.0, from, to, Some(rates))?;
    let day = rates.date.as_deref().and_then(short_date);
    let shown = format_number(rate, 5.0);
    Some(PhraseAnswer {
        display: match day {
            Some(day) => format!("1 {from} = {shown} {to} ({day})"),
            None => format!("1 {from} = {shown} {to}"),
        },
        n: Some(rate),
    })
}

/// `2026-11-05` as `Nov 5`.
fn short_date(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = iso.split('-').skip(1);
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    Some(format!("{} {day}", MONTHS.get(month.checked_sub(1)?)?))
}

/// Native only: the hover note on a converted amount (`100 usd to eur`): the rate it used and its day.
pub fn rate_note(expr: &str, rates: &CurrencyRates) -> Option<String> {
    static TO: Lazy = Lazy::new(|| js_re(r"^(.+?)\s+(?:in|to|as|into)\s+(.+)$", ""));
    let s = normalize(expr);
    let m = caps(&TO, &s)?;
    let to = currency_word(g(&m, 2), true)?;
    let left = g(&m, 1);
    let from = if left.chars().any(|c| c.is_ascii_digit()) {
        worth(left, rates)?.1
    } else {
        return None;
    };
    let line = rate_line(from, to, rates)?.display;
    Some(match line.split_once(" (") {
        Some((rate, day)) => format!("{rate} · ECB rate, {}", day.trim_end_matches(')')),
        None => format!("{line} · ECB rate"),
    })
}

/// The answer for a currency line the web's phrases don't answer.
pub fn answer(expr: &str, rates: Option<&CurrencyRates>, sig_figs: f64) -> Option<PhraseAnswer> {
    static TO: Lazy = Lazy::new(|| js_re(r"^(.+?)\s+(?:in|to|as|into)\s+(.+)$", ""));
    let rates = rates?;
    let s = normalize(expr);
    if let Some(m) = caps(&TO, &s)
        && let Some(to) = currency_word(g(&m, 2), true)
    {
        let left = g(&m, 1);
        if !left.chars().any(|c| c.is_ascii_digit()) {
            return rate_line(currency_word(left, true)?, to, rates);
        }
        let (n, from) = worth(left, rates)?;
        let n = convert_currency(n, from, to, Some(rates))?;
        return amount(n, Some(to), sig_figs);
    }
    let (n, c) = mixed_sum(&s, rates)?;
    amount(n, Some(c), sig_figs)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn rates() -> CurrencyRates {
        let r: BTreeMap<String, f64> = [("USD", 1.25), ("GBP", 0.8), ("JPY", 160.0)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        CurrencyRates::new("EUR", &r)
    }

    fn say(s: &str) -> Option<String> {
        answer(s, Some(&rates()), 12.0).map(|a| a.display)
    }

    #[test]
    fn currency_lines() {
        assert_eq!(say("$5 * 3 to eur").as_deref(), Some("€12.00"));
        assert_eq!(say("$20/hr * 40 hr to eur").as_deref(), Some("€640.00"));
        assert_eq!(say("$5 + €4").as_deref(), Some("$10.00"));
        assert_eq!(say("€4 - $5").as_deref(), Some("€0.00"));
        assert_eq!(say("$5 + €4 in gbp").as_deref(), Some("£6.40"));
        assert_eq!(say("usd to eur").as_deref(), Some("1 USD = 0.8 EUR"));
        let dated = CurrencyRates {
            date: Some("2026-11-05".into()),
            ..rates()
        };
        assert_eq!(
            answer("gbp to yen", Some(&dated), 12.0).map(|a| a.display).as_deref(),
            Some("1 GBP = 200 JPY (Nov 5)")
        );
        // one currency is the web's own sum
        assert_eq!(say("$5 + $3"), None);
        assert_eq!(say("5 + 3"), None);
        assert_eq!(answer("$5 + €4", None, 12.0), None);
        assert_eq!(
            rate_note("100 usd to gbp", &dated).as_deref(),
            Some("1 USD = 0.64 GBP · ECB rate, Nov 5")
        );
        assert_eq!(rate_note("5 + 3", &dated), None);
    }
}
