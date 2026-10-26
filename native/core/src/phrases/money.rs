//! `src/engine/phraseMoney.ts`: money said the way people type it into a calculator (`$45.50 + $12.25`,
//! `3 x $4.99`, `$100 + 8.25% tax`, `my rent is $1500 + utilities $200`), a rate times an amount (`$30/hr * 40 hr`,
//! `10 nights at $120`), and shorthand numbers (`$50k / 12`, `1.5k + 500`, `5 dozen`). A line only counts with
//! money or a shorthand in it, so plain maths stays the engine's.

use super::currency::{Currency, NUM, currency_word, parse_number, symbol};
use super::js_re;
use crate::jsre::{self, Caps, Lazy};

/// `Sum`: a total, with its currency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sum {
    pub n: f64,
    pub currency: Option<Currency>,
}

// the scale after a number. `m` and `b` are a million and a billion only on money (`$1.5m`), since `5 m` is
// metres; `k` only touching the number (`1.5k`), since `5 k` could be kelvin
fn scale(k: &str) -> f64 {
    match k {
        "k" | "thousand" => 1e3,
        "m" | "mm" | "mn" | "million" => 1e6,
        "b" | "bn" | "billion" => 1e9,
        "dozen" => 12.0,
        _ => 1.0,
    }
}

const SCALE: &str = r"(?:(k|mm|mn|m|bn|b)\b|\s*(thousand|million|billion|dozen)\b)?";

// money or a number: an optional symbol before, the number, an optional scale, an optional symbol after
fn value_pattern() -> String {
    format!(r"([$€£¥₹])?\s?({NUM}){SCALE}\s?([$€£¥₹])?")
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Val {
    n: f64,
    currency: Option<Currency>,
    scaled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Value(Val),
    Pct(f64),
    Op(char),
}

fn value(m: &Caps<'_>, at: usize) -> Option<Val> {
    let part = |i: usize| m.get(at + i).map(|g| g.as_str());
    let (pre, num, scaled, dozen, post) = (part(0), part(1), part(2), part(3), part(4));
    if pre.is_some() && post.is_some() {
        return None;
    }
    let n = parse_number(num.unwrap_or(""))?;
    let k = scaled.or(dozen);
    let sym = pre.or(post);
    if scaled.is_some_and(|s| s != "k") && sym.is_none() {
        return None;
    }
    Some(Val {
        n: n * k.map_or(1.0, scale),
        currency: sym.and_then(symbol),
        scaled: k.is_some(),
    })
}

// the line as values, percentages and operators; any other word is a label and drops out
fn tokens(s: &str) -> Option<Vec<Token>> {
    // the TS regex is sticky (`y`); here it is anchored and run on the rest of the line, which is the same
    // because the pattern starts with nothing that looks behind
    static RE: Lazy = Lazy::new(|| {
        js_re(
            &format!(
                r"^\s*(?:({NUM})\s*%|{}|([+\-*/×÷x])|([a-z']+))",
                value_pattern()
            ),
            "",
        )
    });
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let rest = &s[at..];
        let Ok(Some(m)) = RE.captures(rest) else {
            break;
        };
        let whole = m.get(0).expect("group 0");
        if whole.end() == 0 {
            break;
        }
        at += whole.end();
        if let Some(p) = m.get(1) {
            out.push(Token::Pct(parse_number(p.as_str())?));
        } else if m.get(3).is_some() {
            out.push(Token::Value(value(&m, 2)?));
        } else if let Some(op) = m.get(7) {
            out.push(Token::Op(match op.as_str() {
                "×" | "x" => '*',
                "÷" => '/',
                o => o.chars().next().unwrap_or('+'),
            }));
        } else if let Some(w) = m.get(8).map(|w| w.as_str()) {
            if w == "x" || w == "times" {
                out.push(Token::Op('*'));
            } else if matches!(w, "in" | "to" | "into" | "as") || currency_word(w, true).is_some() {
                // `$20 in euros`, `100 usd to eur`: a conversion or a currency said in words, which isn't a sum
                return None;
            }
            // any other word is a label
        }
    }
    super::js_trim(&s[at..]).is_empty().then_some(out)
}

// left to right: * and / bind first; `+ 10%` and `- 10%` scale everything before them, as a calculator tape does
fn evaluate(list: &[Token]) -> Option<Sum> {
    let mut currency: Option<Currency> = None;
    let mut money = |c: Option<Currency>| -> bool {
        let Some(c) = c else { return true };
        if currency.is_some_and(|cur| cur != c) {
            return false;
        }
        currency = Some(c);
        true
    };
    let mut total = 0.0;
    let mut sign = 1.0;
    let mut i = 0;
    let mut first = true;
    while i < list.len() {
        let mut t = list[i];
        if !first {
            match t {
                Token::Op(op @ ('+' | '-')) => sign = if op == '+' { 1.0 } else { -1.0 },
                _ => return None,
            }
            i += 1;
            t = *list.get(i)?;
        } else if let Token::Op(op @ ('+' | '-')) = t {
            sign = if op == '-' { -1.0 } else { 1.0 };
            i += 1;
            t = *list.get(i)?;
        }
        first = false;
        if let Token::Pct(p) = t {
            // a percentage only scales what came before it
            if i == 0 {
                return None;
            }
            total *= 1.0 + (sign * p) / 100.0;
            i += 1;
            continue;
        }
        let Token::Value(v) = t else { return None };
        if !money(v.currency) {
            return None;
        }
        let mut term = v.n;
        let mut term_money = v.currency.is_some();
        i += 1;
        while i < list.len() {
            let Token::Op(op @ ('*' | '/')) = list[i] else {
                break;
            };
            let Some(Token::Value(next)) = list.get(i + 1).copied() else {
                return None;
            };
            if !money(next.currency) {
                return None;
            }
            if op == '*' {
                if term_money && next.currency.is_some() {
                    return None;
                }
                term *= next.n;
                term_money |= next.currency.is_some();
            } else {
                if next.n == 0.0 || (!term_money && next.currency.is_some()) {
                    return None;
                }
                term /= next.n;
                // money over money is a plain ratio
                if term_money && next.currency.is_some() {
                    term_money = false;
                }
            }
            i += 2;
        }
        total += sign * term;
    }
    total.is_finite().then_some(Sum { n: total, currency })
}

// `$30/hr * 40 hr`, `$15 per hour * 8 hours`, `$5 per day for 30 days`, `10 nights at $120`, `3 tickets at $25 each`
fn rate_times(s: &str) -> Option<Sum> {
    static PER: Lazy = Lazy::new(|| {
        js_re(
            &format!(
                r"^{}\s*(?:/|per|an?)\s*[a-z]+\s*(?:\*|x|×|times|for)\s*({NUM})(?:\s*[a-z]+)?$",
                value_pattern()
            ),
            "",
        )
    });
    static AT: Lazy = Lazy::new(|| {
        js_re(
            &format!(
                r"^({NUM})\s+[a-z]+\s+(?:at|@)\s+{}(?:\s*(?:/|per|an?)\s*[a-z]+|\s+each)?$",
                value_pattern()
            ),
            "",
        )
    });
    if let Ok(Some(per)) = PER.captures(s) {
        let v = value(&per, 1);
        let k = parse_number(jsre::group(&per, 6));
        return match (v, k) {
            (
                Some(
                    v @ Val {
                        currency: Some(c), ..
                    },
                ),
                Some(k),
            ) => Some(Sum {
                n: v.n * k,
                currency: Some(c),
            }),
            _ => None,
        };
    }
    if let Ok(Some(at)) = AT.captures(s) {
        let k = parse_number(jsre::group(&at, 1));
        let v = value(&at, 2);
        return match (v, k) {
            (
                Some(
                    v @ Val {
                        currency: Some(c), ..
                    },
                ),
                Some(k),
            ) => Some(Sum {
                n: v.n * k,
                currency: Some(c),
            }),
            _ => None,
        };
    }
    None
}

/// `moneyMath`: money arithmetic or a shorthand number, or None when the line has neither money nor a shorthand.
pub fn money_math(s: &str) -> Option<Sum> {
    if let Some(rate) = rate_times(s) {
        return Some(rate);
    }
    let list = tokens(s)?;
    if list.is_empty() {
        return None;
    }
    let values = list.iter().filter_map(|t| match t {
        Token::Value(v) => Some(v),
        _ => None,
    });
    if !values.clone().any(|v| v.currency.is_some() || v.scaled) {
        return None;
    }
    // `$5 $6`: two amounts with nothing between them aren't a sum
    if list
        .windows(2)
        .any(|w| matches!(w, [Token::Value(_), Token::Value(_)]))
    {
        return None;
    }
    evaluate(&list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(s: &str) -> Option<(f64, Option<&'static str>)> {
        money_math(s).map(|m| (m.n, m.currency))
    }

    #[test]
    fn sums_money_and_shorthand() {
        assert_eq!(sum("$45.50 + $12.25"), Some((57.75, Some("USD"))));
        assert_eq!(
            sum("$100 + 10% + 5%").map(|s| (s.0 * 100.0).round()),
            Some(11550.0)
        );
        assert_eq!(
            sum("my rent is $1500 + utilities $200"),
            Some((1700.0, Some("USD")))
        );
        assert_eq!(sum("1.5k + 500"), Some((2000.0, None)));
        assert_eq!(sum("$1.5m / 3"), Some((500000.0, Some("USD"))));
        assert_eq!(sum("5 m + 3"), None);
        assert_eq!(sum("$5 * $5"), None);
        assert_eq!(sum("$5 $6"), None);
        assert_eq!(sum("$50 + €20"), None);
        assert_eq!(sum("$20 in euros"), None);
        assert_eq!(sum("10 nights at $120"), Some((1200.0, Some("USD"))));
        assert_eq!(sum("$30/hr * 40 hr"), Some((1200.0, Some("USD"))));
    }
}
