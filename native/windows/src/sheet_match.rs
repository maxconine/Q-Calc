//! Which sheet row an explanation's Related formula is (macos/SheetMatch.swift, pure and tested here as
//! macos/sheet_match_test.swift tests it there). Cells are written as in `show` (`x^{2}`, `y_{n}`); a row's runs give
//! it back with `markup`. Tiers, best first:
//! 0. the same characters, leaving out spaces, brackets, ^ _ and the way minus, times and ½ are written, either way
//!    round
//! 1. a row that chains equalities with the formula's sides in order (`P = VI = I^{2}R` is `P = V I = I^{2} R = V^{2}/R`)
//! 2. the same, loosely: a sum's or integral's limits left out (`Σ x[n] e^{−jωn}` is `Σ_{n=−∞}^{∞} x[n] e^{−jωn}`),
//!    `…` standing for anything (`ln(1 + x) = x − x^{2}/2 + …`), a formula that goes on past the row
//!    (`BW = ω_{0}/Q = 1/(RC)` is `BW = ω_{0}/Q`), and a transform pair across a row's cells (`1 ↔ 2πδ(ω)` is the
//!    row `1 ‖ 2πδ(ω)`)
//! 3. a right side that is a whole cell of a table row (`I = (1/2) m r^{2}` is the cell `(1/2) m r^{2}`), or an
//!    expression alone that is a row's left side
//! 4. the same right side under another short name (`U = (1/2) C V^{2}` is `E = (1/2) C V^{2}`)

use qcalc_core::app::identities::{self, Identity, IdentitySheet, Shift};

/// A formula with spaces, brackets, `^ _` and the ways minus, times, `∕` and the vulgar fractions are written left out.
pub fn key(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            ' ' | '\t' | '{' | '}' | '(' | ')' | '^' | '_' | '*' | '·' | '×' | '⋅' => {}
            '\u{2212}' => out.push('-'),
            '\u{2215}' => out.push('/'),
            '½' => out.push_str("1/2"),
            '⅓' => out.push_str("1/3"),
            '⅔' => out.push_str("2/3"),
            '¼' => out.push_str("1/4"),
            '¾' => out.push_str("3/4"),
            c => out.push(c),
        }
    }
    out
}

/// A cell's runs written back as `show` writes them, so its sides and limits can be told apart.
pub fn markup(cell: &str) -> String {
    identities::identity_runs(cell)
        .into_iter()
        .map(|r| match r.shift {
            Some(Shift::Sup) => format!("^{{{}}}", r.text),
            Some(Shift::Sub) => format!("_{{{}}}", r.text),
            Some(Shift::Supsub) => format!("^{{_{{{}}}}}", r.text),
            None => r.text,
        })
        .collect()
}

/// Length as Swift counts it, near enough: characters, a combining mark (a⃗'s arrow) not counted on its own.
fn len(s: &str) -> usize {
    s.chars()
        .filter(|c| !matches!(*c, '\u{300}'..='\u{36f}' | '\u{20d0}'..='\u{20ff}'))
        .count()
}

fn has_letter(s: &str) -> bool {
    s.chars().any(char::is_alphabetic)
}

/// A Related formula, read once and held against each row.
#[derive(Clone, Debug)]
pub struct Formula {
    want: String,
    flipped: Option<String>,
    keyed: Vec<String>,
    loose_sides: Vec<String>,
    halves: Vec<String>,
}

impl Formula {
    pub fn new(formula: &str) -> Option<Formula> {
        let want = key(formula);
        if want.is_empty() {
            return None;
        }
        let sides = split(formula, '=');
        Some(Formula {
            want,
            flipped: (sides.len() == 2).then(|| key(&format!("{}={}", sides[1], sides[0]))),
            keyed: sides.iter().map(|s| key(s)).collect(),
            loose_sides: sides.iter().map(|s| loose(s)).collect(),
            halves: if formula.contains('↔') {
                split(formula, '↔').iter().map(|s| loose(s)).collect()
            } else {
                Vec::new()
            },
        })
    }

    /// How well this is one of the row's cells (lead cells then the formula), lower is better; None when it isn't.
    pub fn tier(&self, cells: &[String]) -> Option<u8> {
        let keys: Vec<String> = cells.iter().map(|c| key(c)).collect();
        if keys
            .iter()
            .any(|k| *k == self.want || Some(k) == self.flipped.as_ref())
        {
            return Some(0);
        }
        if self.keyed.iter().any(String::is_empty) {
            return None;
        }
        let rows: Vec<Vec<String>> = cells.iter().map(|c| split(c, '=')).collect();
        if self.keyed.len() >= 2
            && rows.iter().any(|r| {
                r.len() > 1
                    && chained(
                        &self.keyed,
                        &r.iter().map(|s| key(s)).collect::<Vec<_>>(),
                        false,
                    )
            })
        {
            return Some(1);
        }
        let loose_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.iter().map(|s| loose(s)).collect())
            .collect();
        if loose_rows
            .iter()
            .any(|r| chained(&self.loose_sides, r, true))
        {
            return Some(2);
        }
        if pair(&self.halves, &loose_rows) {
            return Some(2);
        }
        let last = &self.loose_sides[self.loose_sides.len() - 1];
        // a table row's cell with no `=` (a moment of inertia, a beam's deflection, a Laplace pair's side)
        if self.keyed.len() >= 2
            && loose_rows
                .iter()
                .any(|r| r.len() == 1 && len(&r[0]) >= 4 && has_letter(&r[0]) && alike(last, &r[0]))
        {
            return Some(3);
        }
        // an expression alone that is the left side of a row (`(a⃗ × b⃗) · (c⃗ × d⃗)`)
        if self.keyed.len() == 1
            && self.want.chars().count() >= 5
            && has_letter(&self.want)
            && loose_rows
                .iter()
                .any(|r| r.len() >= 2 && r[0] == self.loose_sides[0])
        {
            return Some(3);
        }
        // the same right side under another short name
        if self.loose_sides.len() == 2
            && len(&self.loose_sides[0]) <= 4
            && len(&self.loose_sides[1]) >= 6
            && loose_rows
                .iter()
                .any(|r| r.len() == 2 && len(&r[0]) <= 4 && alike(&self.loose_sides[1], &r[1]))
        {
            return Some(4);
        }
        None
    }
}

/// `f(t) ↔ F(s)` as two cells of the row, in order.
fn pair(halves: &[String], cells: &[Vec<String>]) -> bool {
    if halves.len() != 2 || halves.iter().any(String::is_empty) || cells.len() < 2 {
        return false;
    }
    let row: Vec<String> = cells.iter().map(|c| c.join("=")).collect();
    let Some(at) = row.iter().position(|c| alike(&halves[0], c)) else {
        return false;
    };
    row[at + 1..].iter().any(|c| alike(&halves[1], c))
}

/// The formula's sides against a row's: the same sides (either way round for two), the formula's in order among a
/// longer row's, or, loosely, a shorter row's (two or more sides, the same left side) in order among the formula's.
fn chained(part: &[String], whole: &[String], loose: bool) -> bool {
    let eq = |a: &str, b: &str| if loose { alike(a, b) } else { a == b };
    if part.len() == whole.len() {
        if part.iter().zip(whole).all(|(a, b)| eq(a, b)) {
            return true;
        }
        return part.len() == 2 && eq(&part[0], &whole[1]) && eq(&part[1], &whole[0]);
    }
    if part.len() >= 2 && whole.len() > part.len() {
        let reversed: Vec<String> = part.iter().rev().cloned().collect();
        return in_order(part, whole, &eq) || in_order(&reversed, whole, &eq);
    }
    if loose && whole.len() >= 2 && part.len() > whole.len() && eq(&part[0], &whole[0]) {
        return in_order(whole, part, &eq);
    }
    false
}

/// Every one of `part`, in this order, among `whole`.
fn in_order(part: &[String], whole: &[String], eq: &impl Fn(&str, &str) -> bool) -> bool {
    let mut rest = whole;
    for side in part {
        let Some(at) = rest.iter().position(|w| eq(side, w)) else {
            return false;
        };
        rest = &rest[at + 1..];
    }
    true
}

/// A key with a big operator's limits left out (`Σ_{k=0}^{n}`, `∫_{0}^{t}`); `…` is kept for `alike`.
pub fn loose(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        out.push(c);
        if !"Σ∑∏∫∮".contains(c) {
            continue;
        }
        while chars.len() - i >= 2 && matches!(chars[i], '_' | '^') && chars[i + 1] == '{' {
            let mut depth = 0;
            i += 1;
            while i < chars.len() {
                let d = chars[i];
                i += 1;
                if d == '{' {
                    depth += 1;
                }
                if d == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
        }
    }
    key(&out)
}

/// Two loose keys the same, a `…` in the first standing for anything (at least three characters written around it).
pub fn alike(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if !a.contains('…') {
        return false;
    }
    let pieces: Vec<&str> = a.split('…').collect();
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if pieces.iter().map(|p| len(p)).sum::<usize>() < 3
        || !b.starts_with(first)
        || !b.ends_with(last)
        || b.len() < first.len() + last.len()
    {
        return false;
    }
    let mut rest = &b[first.len()..b.len() - last.len()];
    for piece in &pieces[1..pieces.len() - 1] {
        if piece.is_empty() {
            continue;
        }
        let Some(at) = rest.find(piece) else {
            return false;
        };
        rest = &rest[at + piece.len()..];
    }
    true
}

/// Split at `separator` outside braces (`Σ_{n=0}` holds no side).
pub fn split(text: &str, separator: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut depth = 0i32;
    for c in text.chars() {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth = (depth - 1).max(0);
        }
        if c == separator && depth == 0 {
            parts.push(String::new());
        } else if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts
}

/// A row's cells as the matcher reads them: its lead cells then its formula, each rebuilt from its runs.
pub fn cells(item: &Identity) -> Vec<String> {
    item.lead
        .iter()
        .flatten()
        .chain(std::iter::once(&item.show))
        .map(|c| markup(c))
        .collect()
}

/// The row a Related formula is, among the explained rows of `sheets` (searched in order): the best tier wins (an
/// exact cell anywhere over a chain over a looser match), the first such row on a tie. (sheet, section, group, item)
pub fn explained_row<'a>(
    formula: &str,
    sheets: impl IntoIterator<Item = (usize, &'a IdentitySheet)>,
) -> Option<(usize, usize, usize, usize)> {
    let wanted = Formula::new(formula)?;
    let mut best: Option<(u8, (usize, usize, usize, usize))> = None;
    for (n, sheet) in sheets {
        for (s, section) in sheet.sections.iter().enumerate() {
            for (g, group) in section.groups.iter().enumerate() {
                for (i, item) in group.items.iter().enumerate() {
                    if item.explain.is_none() {
                        continue;
                    }
                    let Some(tier) = wanted.tier(&cells(item)) else {
                        continue;
                    };
                    if best.is_some_and(|(b, _)| tier >= b) {
                        continue;
                    }
                    best = Some((tier, (n, s, g, i)));
                    if tier == 0 {
                        return best.map(|b| b.1);
                    }
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// Whether the formula is the cell (any tier).
pub fn same(formula: &str, cell: &str) -> bool {
    Formula::new(formula).is_some_and(|f| f.tier(&[cell.to_string()]).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(formula: &str, row: &[&str]) -> Option<u8> {
        let row: Vec<String> = row.iter().map(|s| s.to_string()).collect();
        Formula::new(formula)?.tier(&row)
    }

    #[test]
    fn related_formulas_find_their_rows_as_on_the_mac() {
        let yes = [
            ("τ = Tρ/J", "τ = Tρ/J"),
            ("y_{n+1} = y_{n} + h f", "y_{n+1} = y_n + h f"),
            ("Tρ/J = τ", "τ = Tρ/J"),
            ("a − b", "a - b"),
            ("P = VI = I^{2}R", "P = V I = I^{2} R = V^{2}/R"),
            ("Y = 1/Z", "Y = 1/Z = G + jB"),
            ("PF = cos(θ)", "PF = P/|S| = cos(θ)"),
            ("S = P + jQ", "S = V_{rms} I_{rms}^{*} = P + jQ"),
            ("P = Fv", "P = dW/dt = F v"),
            (
                "1/(1 − x) = Σ x^{n}",
                "1/(1 − x) = Σ x^{n} = 1 + x + x^{2} + x^{3} + …",
            ),
            ("cos(θ) = PF", "PF = P/|S| = cos(θ)"),
            (
                "X(e^{jω}) = Σ x[n] e^{−jωn}",
                "X(e^{jω}) = Σ_{n=−∞}^{∞} x[n] e^{−jωn}",
            ),
            (
                "ln(1 + x) = x − x^{2}/2 + …",
                "ln(1 + x) = x − x^{2}/2 + x^{3}/3 − …",
            ),
            ("BW = ω_{0}/Q = 1/(RC)", "BW = ω_{0}/Q"),
            ("I = (1/2) m r^{2}", "(1/2) m r^{2}"),
            ("I = ½ m r^{2}", "(1/2) m r^{2}"),
            ("U = (1/2) C V^{2}", "E = (1/2) C V^{2}"),
            (
                "(a⃗ × b⃗) · (c⃗ × d⃗)",
                "(a⃗ × b⃗) · (c⃗ × d⃗) = (a⃗ · c⃗)(b⃗ · d⃗) − (a⃗ · d⃗)(b⃗ · c⃗)",
            ),
        ];
        for (f, c) in yes {
            assert!(same(f, c), "{f} should be {c}");
        }
        let no = [
            ("P = Fv", "W = F d cos(θ)"),
            ("W = F · d", "W = F d cos(θ)"),
            ("U = (1/2) C V^{2}", "U = (1/2) L I^{2}"),
            ("x = 1", "y = x = 2"),
            ("", ""),
            ("n^{r}", "SNR = 6.02n + 1.76 dB"),
            ("c_{1} + c_{2} = y(0)", "C_{1} + C_{2}"),
            ("τ_{max} = pr/(2t)", "σ_{a} = pr/(2t)"),
            ("A + B + C = 180°", "180°"),
            (
                "Σ_{k=1}^{n} k = n(n + 1)/2",
                "Σ_{k=1}^{n} k^{2} = n(n + 1)(2n + 1)/6",
            ),
            ("I = (1/3) m r^{2} …", "I = (1/2) m r^{2}"),
            ("P", "P = V I = I^{2} R"),
        ];
        for (f, c) in no {
            assert!(!same(f, c), "{f} shouldn't be {c}");
        }
        assert_eq!(tier("1 ↔ 2πδ(ω)", &["1", "2πδ(ω)"]), Some(2));
        assert_eq!(tier("x(−t) ↔ X(−jω)", &["X(−jω)", "x(−t)"]), None);
        assert_eq!(
            tier("P = VI = I^{2}R", &["P = V I = I^{2} R = V^{2}/R"]),
            Some(1)
        );
        assert_eq!(tier("U = (1/2) C V^{2}", &["U = (1/2) C V^{2}"]), Some(0));
    }

    #[test]
    fn a_row_is_written_back_from_its_runs() {
        assert_eq!(markup("x^{2} + y_{n}"), "x^{2} + y_{n}");
        assert_eq!(markup("b^{log_{b}x}"), "b^{log}^{_{b}}^{x}");
        assert_eq!(
            split("Σ_{n=0}^{N} x = y", '='),
            vec!["Σ_{n=0}^{N} x ".to_string(), " y".to_string()]
        );
    }

    #[test]
    fn a_related_link_opens_the_row_it_names() {
        let ids = identities::native_sheet_ids();
        let sheets: Vec<IdentitySheet> = ids
            .iter()
            .filter_map(|id| identities::native_identity_sheet_by_id(id))
            .collect();
        let found = |f: &str| {
            explained_row(f, sheets.iter().enumerate())
                .map(|(n, s, g, i)| sheets[n].sections[s].groups[g].items[i].show.clone())
        };
        assert_eq!(found("V = IR").as_deref(), Some("V = I R"));
        assert!(found("A = Pe^{rt}").is_some());
        assert!(found("zzz = qqq").is_none());
    }
}
