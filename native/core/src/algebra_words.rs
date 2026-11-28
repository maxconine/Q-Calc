//! native only: lines asking for a piece of algebra or number theory by name, which the web left blank.
//!
//! - `simplify (x^2-1)/(x-1)` → `x + 1`, `expand (x+1)^2` → `x² + 2x + 1`
//! - `factor 12` / `prime factors of 12` → `2² × 3`, `factors of 12` → `1, 2, 3, 4, 6, 12`,
//!   `factor x^2 - 1` → `(x - 1)(x + 1)` (whole-number roots and rational ones)
//! - `is 7 prime`, `isprime(9)` → `7 is prime` / `9 isn't prime: 3²`
//!
//! A name the user defined (a variable or function called `factor`) keeps its own meaning.

use crate::calculus::pretty::pretty;
use crate::calculus::tree::{self, T};
use crate::jsre::{self, Lazy, js};

/// What a line here answers: text to show, or an expression for the engine to read instead.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Text(String),
    Rewrite(String),
}

/// The answer, or None when the line isn't one of these.
pub fn answer(raw: &str, known: &dyn Fn(&str) -> bool) -> Option<Answer> {
    static WORD: Lazy = Lazy::new(|| {
        js(
            r"^\s*(simplify|expand|factori[sz]e|factor|prime\s+factors\s+of|factors\s+of|divisors\s+of|is\s*prime|isprime)\b\s*(.*?)\s*$",
            "i",
        )
    });
    static IS_PRIME: Lazy = Lazy::new(|| {
        js(
            r"^\s*(?:is\s+)?(\d+)\s+(?:is\s+)?(?:a\s+)?prime\s*\??\s*$",
            "i",
        )
    });
    if let Some(c) = IS_PRIME.captures(raw).ok().flatten() {
        return prime_text(jsre::group(&c, 1)).map(Answer::Text);
    }
    let c = WORD.captures(raw).ok().flatten()?;
    let word = jsre::group(&c, 1).to_lowercase();
    let word = word.split_whitespace().collect::<Vec<_>>().join(" ");
    let first = word.split(' ').next().unwrap_or_default();
    if known(first) {
        return None;
    }
    let body = unwrap_parens(jsre::group(&c, 2));
    if body.is_empty() {
        return None;
    }
    match word.as_str() {
        "simplify" => cancel(body)
            .or_else(|| algebra(body, false))
            .map(Answer::Text),
        "expand" => algebra(body, true).map(Answer::Text),
        "factor" | "factorize" | "factorise" | "prime factors of" => match whole(body) {
            Some(n) => factor_int(n).map(Answer::Text),
            None if word != "prime factors of" => factor_poly(body).map(Answer::Text),
            None => None,
        },
        "factors of" | "divisors of" => divisors(whole(body)?).map(Answer::Text),
        "isprime" | "is prime" => prime_text(body).map(Answer::Text),
        _ => None,
    }
}

/// `(x+1)^2` as typed after the word, without one pair of brackets around all of it.
fn unwrap_parens(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('(') && t.ends_with(')') {
        let mut depth = 0i32;
        for (k, ch) in t.char_indices() {
            depth += match ch {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            if depth == 0 && k + 1 < t.len() {
                return t;
            }
        }
        return t[1..t.len() - 1].trim();
    }
    t
}

fn whole(s: &str) -> Option<u64> {
    let t = s.trim().replace(',', "");
    if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) || t.len() > 15 {
        return None;
    }
    t.parse().ok()
}

fn tree_of(text: &str) -> Option<T> {
    let pre = crate::scientific::preprocess_ascii(text, &[]);
    let node = crate::symbolic::parse(&pre).ok()?;
    tree::from_node(&node)
}

fn shown(n: &crate::syntax::Node) -> Option<String> {
    Some(pretty(&tree::from_node(n)?).s)
}

/// `simplify` and `expand` (mathjs `rationalize`, which multiplies out), shown as the derivative is.
fn algebra(text: &str, expand: bool) -> Option<String> {
    let pre = crate::scientific::preprocess_ascii(text, &[]);
    let node = crate::symbolic::parse(&pre).ok()?;
    let out = crate::symbolic::simplify::with_line_budget(|| {
        if expand {
            crate::symbolic::rationalize::rationalize(&node).ok()
        } else {
            crate::symbolic::simplify::simplify(&node).ok()
        }
    })?;
    shown(&out)
}

fn primes_of(mut n: u64) -> Vec<(u64, u32)> {
    let mut out = Vec::new();
    let mut p = 2;
    while p * p <= n {
        let mut k = 0;
        while n % p == 0 {
            n /= p;
            k += 1;
        }
        if k > 0 {
            out.push((p, k));
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if n > 1 {
        out.push((n, 1));
    }
    out
}

fn sup(k: u32) -> String {
    const D: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    k.to_string()
        .bytes()
        .map(|b| D[(b - b'0') as usize])
        .collect()
}

fn product_text(f: &[(u64, u32)]) -> String {
    f.iter()
        .map(|(p, k)| {
            if *k == 1 {
                p.to_string()
            } else {
                format!("{p}{}", sup(*k))
            }
        })
        .collect::<Vec<_>>()
        .join(" × ")
}

fn factor_int(n: u64) -> Option<String> {
    match n {
        0 => Some("0 has no prime factors".into()),
        1 => Some("1 has no prime factors".into()),
        _ => {
            let f = primes_of(n);
            Some(if f == [(n, 1)] {
                format!("{n} is prime")
            } else {
                product_text(&f)
            })
        }
    }
}

fn divisors(n: u64) -> Option<String> {
    if n == 0 || n > 1_000_000_000_000 {
        return None;
    }
    let mut low = Vec::new();
    let mut high = Vec::new();
    let mut d = 1;
    while d * d <= n {
        if n % d == 0 {
            low.push(d);
            if d * d != n {
                high.push(n / d);
            }
        }
        d += 1;
    }
    high.reverse();
    low.extend(high);
    (low.len() <= 200).then(|| {
        low.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    })
}

fn prime_text(s: &str) -> Option<String> {
    let n = whole(s)?;
    Some(match n {
        0 | 1 => format!("{n} isn’t prime"),
        _ => {
            let f = primes_of(n);
            if f == [(n, 1)] {
                format!("{n} is prime")
            } else {
                format!("{n} isn’t prime: {}", product_text(&f))
            }
        }
    })
}

// ---- factoring a polynomial in one letter

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// Integer coefficients, lowest power first, read by sampling: None unless the line is a polynomial of degree
/// 2 to 6 in one letter with whole-number coefficients.
fn coefficients(t: &T, var: &str) -> Option<Vec<i64>> {
    coefficients_any(t, var).filter(|c| c.len() >= 3)
}

/// `(x^2 - 1)/(x - 1)` → `x + 1`: a fraction of polynomials in one letter whose bottom divides the top.
fn cancel(text: &str) -> Option<String> {
    let t = tree_of(text)?;
    let tree::N::Op(crate::symbolic::Op::Divide, parts) = &*t else {
        return None;
    };
    let [top, bottom] = parts.as_slice() else {
        return None;
    };
    let mut vars = Vec::new();
    letters(&t, &mut vars);
    let [var] = vars.as_slice() else {
        return None;
    };
    let (mut n, d) = (coefficients_any(top, var)?, coefficients_any(bottom, var)?);
    if d.len() < 2 || n.len() < d.len() {
        return None;
    }
    let lead = *d.last()?;
    let mut q = vec![0i64; n.len() - d.len() + 1];
    for i in (0..q.len()).rev() {
        let top_coeff = n[i + d.len() - 1];
        if top_coeff % lead != 0 {
            return None;
        }
        let k = top_coeff / lead;
        q[i] = k;
        for (j, dj) in d.iter().enumerate() {
            n[i + j] -= k * dj;
        }
    }
    n.iter().all(|&r| r == 0).then(|| poly_text(&q, var))
}

/// Whole-number coefficients of a polynomial of degree at most 6, lowest power first.
fn coefficients_any(t: &T, var: &str) -> Option<Vec<i64>> {
    const MAX: usize = 6;
    let at = |x: f64| tree::evaluate(t, &[(var.to_string(), x)]).flatten();
    // Newton's forward differences over x = 0..=MAX+1; a polynomial of degree d has zero differences past d
    let ys: Vec<f64> = (0..=MAX + 1).map(|x| at(x as f64)).collect::<Option<_>>()?;
    let mut table = vec![ys];
    for k in 1..=MAX + 1 {
        let prev = &table[k - 1];
        table.push(prev.windows(2).map(|w| w[1] - w[0]).collect());
    }
    if table[MAX + 1].iter().any(|d| d.abs() > 1e-6) {
        return None;
    }
    // falling-factorial basis to powers, exactly, in rationals with a common denominator of MAX!
    let deltas: Vec<f64> = table.iter().map(|r| r[0]).collect();
    let mut coeffs = vec![0f64; MAX + 1];
    // C(x, k) = x(x-1)…(x-k+1)/k!
    let mut basis = vec![1f64]; // x(x-1)…(x-k+1), lowest power first
    let mut fact = 1f64;
    for (k, d) in deltas.iter().enumerate().take(MAX + 1) {
        if k > 0 {
            fact *= k as f64;
            let shift = (k - 1) as f64;
            let mut next = vec![0f64; basis.len() + 1];
            for (i, b) in basis.iter().enumerate() {
                next[i + 1] += b;
                next[i] -= shift * b;
            }
            basis = next;
        }
        for (i, b) in basis.iter().enumerate() {
            coeffs[i] += d * b / fact;
        }
    }
    let mut out: Vec<i64> = Vec::new();
    for c in coeffs {
        let r = c.round();
        if (c - r).abs() > 1e-6 || r.abs() > 1e12 {
            return None;
        }
        out.push(r as i64);
    }
    while out.len() > 1 && *out.last()? == 0 {
        out.pop();
    }
    Some(out)
}

fn letters(t: &T, out: &mut Vec<String>) {
    match &**t {
        tree::N::Sym(s) => {
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        tree::N::Op(_, a) | tree::N::Call(_, a) => a.iter().for_each(|x| letters(x, out)),
        tree::N::Const(_) => {}
    }
}

fn poly_text(c: &[i64], var: &str) -> String {
    let mut s = String::new();
    for (i, &k) in c.iter().enumerate().rev() {
        if k == 0 {
            continue;
        }
        let neg = k < 0;
        let a = k.unsigned_abs();
        let body = match (i, a) {
            (0, _) => a.to_string(),
            (_, 1) => var.to_string(),
            _ => format!("{a}{var}"),
        } + &if i > 1 { sup(i as u32) } else { String::new() };
        if s.is_empty() {
            s = if neg { format!("-{body}") } else { body };
        } else {
            s += if neg { " - " } else { " + " };
            s += &body;
        }
    }
    if s.is_empty() { "0".into() } else { s }
}

fn eval_poly(c: &[i64], num: i64, den: i64) -> i128 {
    // den^deg · p(num/den), exactly
    let deg = c.len() - 1;
    c.iter()
        .enumerate()
        .map(|(i, &k)| {
            k as i128 * (num as i128).pow(i as u32) * (den as i128).pow((deg - i) as u32)
        })
        .sum()
}

fn divide_root(c: &[i64], num: i64, den: i64) -> Vec<i64> {
    // c / (den·x - num), synthetic division from the top
    let deg = c.len() - 1;
    let mut q = vec![0i64; deg];
    let mut rem = c.to_vec();
    for i in (1..=deg).rev() {
        let k = rem[i] / den;
        q[i - 1] = k;
        rem[i] -= k * den;
        rem[i - 1] += k * num;
    }
    q
}

fn factor_poly(text: &str) -> Option<String> {
    let t = tree_of(text)?;
    let mut vars = Vec::new();
    letters(&t, &mut vars);
    vars.retain(|v| v != "pi" && v != "e");
    let [var] = vars.as_slice() else {
        return None;
    };
    let mut c = coefficients(&t, var)?;
    let mut parts: Vec<String> = Vec::new();
    // a common whole-number factor, with the sign of the top term
    let mut g = c.iter().fold(0, |a, &b| gcd(a, b));
    if *c.last()? < 0 {
        g = -g;
    }
    if g != 1 {
        c.iter_mut().for_each(|k| *k /= g);
    }
    // a power of the letter
    let low = c.iter().take_while(|&&k| k == 0).count();
    if low > 0 {
        c.drain(..low);
        parts.push(format!(
            "{var}{}",
            if low > 1 {
                sup(low as u32)
            } else {
                String::new()
            }
        ));
    }
    let mut found = true;
    while found && c.len() > 2 && c[0].abs() <= 1_000_000 && c.last()?.abs() <= 1_000_000 {
        found = false;
        let (a0, an) = (c[0].abs(), c.last()?.abs());
        'search: for p in 1..=a0.max(1) {
            if a0 != 0 && a0 % p != 0 {
                continue;
            }
            for q in 1..=an {
                if an % q != 0 || gcd(p, q) != 1 {
                    continue;
                }
                for num in [p, -p] {
                    if eval_poly(&c, num, q) == 0 {
                        c = divide_root(&c, num, q);
                        parts.push(format!("({})", poly_text(&[-num, q], var)));
                        found = true;
                        break 'search;
                    }
                }
            }
        }
    }
    if parts.is_empty() && g == 1 {
        return None;
    }
    if c.len() >= 2 {
        parts.push(format!("({})", poly_text(&c, var)));
    } else {
        g *= c[0];
    }
    // a repeated factor as a power: (x - 1)(x - 1) → (x - 1)²
    let mut grouped: Vec<(String, u32)> = Vec::new();
    for p in parts {
        match grouped.iter_mut().find(|(q, _)| *q == p) {
            Some(e) => e.1 += 1,
            None => grouped.push((p, 1)),
        }
    }
    let body: String = grouped
        .iter()
        .map(|(p, k)| {
            if *k == 1 {
                p.clone()
            } else {
                format!("{p}{}", sup(*k))
            }
        })
        .collect();
    Some(match g {
        1 => body,
        -1 => format!("-{body}"),
        _ => format!("{g}{body}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Option<Answer> {
        answer(s, &|_| false)
    }
    fn text(s: &str) -> String {
        match a(s) {
            Some(Answer::Text(t)) => t,
            other => panic!("{s}: {other:?}"),
        }
    }

    #[test]
    fn numbers() {
        assert_eq!(text("factor 12"), "2² × 3");
        assert_eq!(text("prime factors of 360"), "2³ × 3² × 5");
        assert_eq!(text("factors of 12"), "1, 2, 3, 4, 6, 12");
        assert_eq!(text("is 7 prime"), "7 is prime");
        assert_eq!(text("isprime(9)"), "9 isn’t prime: 3²");
        assert_eq!(text("factor(13)"), "13 is prime");
        assert_eq!(answer("factor 12", &|n| n == "factor"), None);
    }

    #[test]
    fn polynomials() {
        assert_eq!(text("simplify (x^2-1)/(x-1)"), "x + 1");
        assert_eq!(text("simplify (x^3 - 8)/(x - 2)"), "x² + 2x + 4");
        assert_eq!(text("expand (x+1)^2"), "x² + 2x + 1");
        assert_eq!(text("factor x^2 - 1"), "(x - 1)(x + 1)");
        assert_eq!(text("factor(x^2 + 5x + 6)"), "(x + 2)(x + 3)");
        assert_eq!(text("factor 2x^2 - 8"), "2(x - 2)(x + 2)");
        assert_eq!(text("factor x^2 - 2x + 1"), "(x - 1)²");
        assert_eq!(text("factor 6x^2 + x - 2"), "(2x - 1)(3x + 2)");
        assert_eq!(text("factor x^3 - x"), "x(x - 1)(x + 1)");
        assert_eq!(a("factor x^2 + 1"), None);
    }
}
