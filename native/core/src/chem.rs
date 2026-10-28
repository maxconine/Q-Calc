//! `src/engine/chem.ts`: chemical formulas and reactions, balancing by an integer null space, molar masses from
//! the periodic table's weights (`app::elements`), and the gas constant in the units a word picks.
//!
//! The TS scans by UTF-16 unit; this scans by `char`. Every test it makes is on a BMP character (ASCII, the
//! sub/superscript digits and signs), and an astral character fails each test as both of its halves do, so the
//! two read every input the same.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::app::elements::{self, subscript_digits};
use crate::jsre::{self, Lazy};
use crate::jsstr;

/// Atoms in the order first written, as the TS `Map` keeps them.
pub type Counts = Vec<(String, BigInt)>;

fn add_count(into: &mut Counts, el: &str, n: BigInt) {
    match into.iter_mut().find(|(e, _)| e == el) {
        Some((_, k)) => *k += n,
        None => into.push((el.to_string(), n)),
    }
}

fn add(into: &mut Counts, from: &Counts, times: &BigInt) {
    for (el, n) in from {
        add_count(into, el, n * times);
    }
}

fn count_of(c: &Counts, el: &str) -> BigInt {
    c.iter()
        .find(|(e, _)| e == el)
        .map_or_else(BigInt::zero, |(_, n)| n.clone())
}

#[derive(Clone, Debug)]
pub struct Species {
    /// Formula and charge as typed, for spotting duplicates and variable names.
    pub key: String,
    pub base: String,
    pub display: String,
    pub atoms: Counts,
    pub charge: BigInt,
    /// Digits, parens, hydrate dots, charges or states: things plain math never writes.
    pub marked: bool,
    /// The coefficient as typed, if any.
    pub typed: Option<BigInt>,
}

#[derive(Clone, Debug)]
pub struct Reaction {
    pub arrow: String,
    pub left: Vec<Species>,
    pub right: Vec<Species>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Balance {
    Balanced {
        coefficients: Vec<BigInt>,
        display: String,
    },
    Underdetermined,
    Impossible,
}

const SUB_DIGITS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
const SUP_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
const HYDRATE_DOTS: [char; 5] = ['·', '•', '∙', '⋅', '.'];
const STATES: [&str; 4] = ["(aq)", "(s)", "(l)", "(g)"];

static ARROW_RE: Lazy = Lazy::new(|| jsre::js(r"->|→|⟶|=>|=", ""));

fn sup(digits: &str) -> String {
    digits
        .chars()
        .map(|c| match c {
            '0'..='9' => SUP_DIGITS[c as usize - '0' as usize],
            _ => c,
        })
        .collect()
}

/// Subscript digits (`₀`–`₉`) as ASCII digits.
fn unsub(s: &str) -> String {
    s.chars()
        .map(|c| match SUB_DIGITS.iter().position(|&d| d == c) {
            Some(i) => char::from(b'0' + i as u8),
            None => c,
        })
        .collect()
}

fn unsup(s: &str) -> String {
    s.chars()
        .map(|c| match SUP_DIGITS.iter().position(|&d| d == c) {
            Some(i) => char::from(b'0' + i as u8),
            None => c,
        })
        .collect()
}

/// How deep a formula's brackets read. The TS reader (on node's stack) reads `H2O` 3,000 deep and overflows
/// by 8,000.
const MAX_BRACKETS: usize = 5_000;

struct Scanner {
    s: Vec<char>,
    i: usize,
    /// Brackets open around `i`: past [`MAX_BRACKETS`] a formula doesn't read, so `((((…` 100,000 deep can't
    /// overflow the stack.
    depth: usize,
}

impl Scanner {
    fn new(s: &str) -> Self {
        Scanner {
            s: s.chars().collect(),
            i: 0,
            depth: 0,
        }
    }
    /// The character `k` ahead, or None past the end (the TS's `''`).
    fn peek(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }
    fn digits(&mut self) -> String {
        let start = self.i;
        while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        self.s[start..self.i].iter().collect()
    }
    fn ws(&mut self) {
        while self.peek(0).is_some_and(jsstr::is_space) {
            self.i += 1;
        }
    }
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn starts_with(&self, word: &str) -> bool {
        (self.i..)
            .zip(word.chars())
            .all(|(at, c)| self.s.get(at) == Some(&c))
    }
    fn rest(&self) -> String {
        self.s[self.i.min(self.s.len())..].iter().collect()
    }
}

/// Subscript count after an element or group; 0 is not a count.
fn count(sc: &mut Scanner) -> Option<(BigInt, String)> {
    let text = sc.digits();
    if text.is_empty() {
        return Some((BigInt::one(), text));
    }
    let n: BigInt = text.parse().ok()?;
    (n > BigInt::zero()).then_some((n, text))
}

fn close_of(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        _ => None,
    }
}

struct Groups {
    atoms: Counts,
    display: String,
    plain: String,
}

fn groups(sc: &mut Scanner) -> Option<Groups> {
    let mut atoms = Counts::new();
    let mut display = String::new();
    let mut plain = String::new();
    while let Some(c) = sc.peek(0) {
        if c.is_ascii_uppercase() {
            let mut el = c.to_string();
            if let Some(l) = sc.peek(1).filter(char::is_ascii_lowercase) {
                el.push(l);
            }
            sc.i += el.chars().count();
            let (n, text) = count(sc)?;
            add_count(&mut atoms, &el, n);
            display += &el;
            display += &subscript_digits(&text);
            plain += &el;
            plain += &text;
        } else if let Some(close) =
            close_of(c).filter(|_| !STATES.iter().any(|st| sc.starts_with(st)))
        {
            if sc.depth >= MAX_BRACKETS {
                return None;
            }
            sc.i += 1;
            sc.depth += 1;
            let inner = groups(sc);
            sc.depth -= 1;
            let inner = inner?;
            if sc.peek(0) != Some(close) {
                return None;
            }
            sc.i += 1;
            let (n, text) = count(sc)?;
            add(&mut atoms, &inner.atoms, &n);
            display.push(c);
            display += &inner.display;
            display.push(close);
            display += &subscript_digits(&text);
            plain.push(c);
            plain += &inner.plain;
            plain.push(close);
            plain += &text;
        } else {
            break;
        }
    }
    if atoms.is_empty() {
        return None;
    }
    Some(Groups {
        atoms,
        display,
        plain,
    })
}

struct Formula {
    atoms: Counts,
    display: String,
    plain: String,
    marked: bool,
}

fn formula(sc: &mut Scanner) -> Option<Formula> {
    let first = groups(sc)?;
    let atoms = first.atoms;
    let mut f = Formula {
        marked: first
            .plain
            .chars()
            .any(|c| c.is_ascii_digit() || c == '(' || c == '['),
        atoms,
        display: first.display,
        plain: first.plain,
    };
    while sc.peek(0).is_some_and(|c| HYDRATE_DOTS.contains(&c))
        && sc
            .peek(1)
            .is_some_and(|c| c.is_ascii_digit() || c.is_ascii_uppercase() || c == '(' || c == '[')
    {
        sc.i += 1;
        let mult = sc.digits();
        let n: BigInt = if mult.is_empty() {
            BigInt::one()
        } else {
            mult.parse().ok()?
        };
        let part = groups(sc)?;
        if n.is_zero() {
            return None;
        }
        add(&mut f.atoms, &part.atoms, &n);
        f.display += &format!("·{mult}{}", part.display);
        f.plain += &format!("·{mult}{}", part.plain);
        f.marked = true;
    }
    Some(f)
}

fn signed_charge(mag: &str, sign: &str) -> BigInt {
    let n: BigInt = if mag.is_empty() {
        BigInt::one()
    } else {
        mag.parse().unwrap_or_else(|_| BigInt::one())
    };
    if sign == "-" { -n } else { n }
}

/// `^2+`, `^+2`, `^{2+}`, `²⁺`; a bare `+`/`-` only straight after a letter or bracket. None is "refused".
fn charge(sc: &mut Scanner, last_char: Option<char>) -> Option<BigInt> {
    static CARET: Lazy = Lazy::new(|| {
        jsre::js(
            r"^\^(?:\{(\d*)([+-])\}|\((\d*)([+-])\)|([+-])(\d+)|(\d*)([+-]))",
            "",
        )
    });
    static SUPER: Lazy =
        Lazy::new(|| jsre::js(r"^(?:([⁰¹²³⁴⁵⁶⁷⁸⁹]*)([⁺⁻])|([⁺⁻])([⁰¹²³⁴⁵⁶⁷⁸⁹]+))", ""));
    let rest = sc.rest();
    let first = |caps: &jsre::Caps<'_>, groups: &[usize]| {
        groups
            .iter()
            .find_map(|&g| caps.get(g).map(|m| m.as_str().to_string()))
    };
    if let Ok(Some(m)) = CARET.captures(rest.as_str()) {
        sc.i += jsre::group(&m, 0).chars().count();
        let mag = first(&m, &[1, 3, 6, 7]).unwrap_or_default();
        let sign = first(&m, &[2, 4, 5, 8]).unwrap_or_default();
        return Some(signed_charge(&mag, &sign));
    }
    if let Ok(Some(m)) = SUPER.captures(rest.as_str()) {
        sc.i += jsre::group(&m, 0).chars().count();
        let mag = unsup(&first(&m, &[1, 4]).unwrap_or_default());
        let sign = if first(&m, &[2, 3]).as_deref() == Some("⁻") {
            "-"
        } else {
            "+"
        };
        return Some(signed_charge(&mag, sign));
    }
    let c = sc.peek(0);
    if c != Some('+') && c != Some('-') {
        return Some(BigInt::zero());
    }
    if c == Some('-') && sc.peek(1) == Some('>') {
        return Some(BigInt::zero());
    }
    // `+` followed by another species is the separator, not a charge
    let after: String = sc.s[sc.i + 1..].iter().collect();
    let after = jsstr::trim_start(&after);
    if c == Some('+') && !after.is_empty() && !after.starts_with('+') {
        return Some(BigInt::zero());
    }
    // `Fe2+` could be Fe₂⁺ or Fe²⁺, so a bare sign after a digit is refused
    if last_char.is_some_and(|l| l.is_ascii_digit()) {
        return None;
    }
    sc.i += 1;
    Some(if c == Some('-') {
        -BigInt::one()
    } else {
        BigInt::one()
    })
}

/// A leading coefficient is read and ignored: the line gets balanced from scratch.
fn species(sc: &mut Scanner) -> Option<Species> {
    let coef = sc.digits();
    if !coef.is_empty() {
        sc.ws();
    }
    let f = formula(sc)?;
    let last = sc.i.checked_sub(1).and_then(|i| sc.s.get(i).copied());
    let q = charge(sc, last)?;
    let state = STATES
        .iter()
        .find(|st| sc.starts_with(st))
        .copied()
        .unwrap_or("");
    sc.i += state.chars().count();
    let mag = q.abs();
    let negative = q.is_negative();
    let mag_text = if mag.is_one() {
        String::new()
    } else {
        mag.to_string()
    };
    let (charge_display, charge_plain) = if q.is_zero() {
        (String::new(), String::new())
    } else {
        (
            sup(&mag_text) + if negative { "⁻" } else { "⁺" },
            format!("^{mag_text}{}", if negative { '-' } else { '+' }),
        )
    };
    Some(Species {
        key: f.plain.clone() + &charge_plain,
        base: f.plain,
        display: f.display + &charge_display + state,
        atoms: f.atoms,
        marked: f.marked || !q.is_zero() || !state.is_empty(),
        charge: q,
        typed: if coef.is_empty() {
            None
        } else {
            coef.parse().ok()
        },
    })
}

fn side(text: &str) -> Option<Vec<Species>> {
    let mut sc = Scanner::new(text);
    let mut out = Vec::new();
    sc.ws();
    loop {
        out.push(species(&mut sc)?);
        sc.ws();
        if sc.done() {
            return Some(out);
        }
        if sc.peek(0) != Some('+') {
            return None;
        }
        sc.i += 1;
        sc.ws();
    }
}

/// Unicode subscripts are read as digits so a shown answer can be typed back in.
pub fn parse_reaction(text: &str) -> Option<Reaction> {
    let s = unsub(text);
    let s = jsstr::trim(&s);
    let mut arrows = ARROW_RE.find_iter(s);
    let a = arrows.next()?.ok()?;
    if arrows.next().is_some() {
        return None;
    }
    let left = side(&s[..a.start()])?;
    let right = side(&s[a.end()..])?;
    Some(Reaction {
        arrow: a.as_str().to_string(),
        left,
        right,
    })
}

fn gcd(a: &BigInt, b: &BigInt) -> BigInt {
    a.gcd(b)
}

fn reduce(v: Vec<BigInt>) -> Vec<BigInt> {
    let g = v.iter().fold(BigInt::zero(), |g, x| gcd(&g, x));
    if g > BigInt::one() {
        v.into_iter().map(|x| x / &g).collect()
    } else {
        v
    }
}

/// Integer null space basis by fraction-free elimination to reduced row echelon form.
pub fn null_space(matrix: &[Vec<BigInt>], cols: usize) -> Vec<Vec<BigInt>> {
    let mut m: Vec<Vec<BigInt>> = matrix.to_vec();
    let at = |row: &[BigInt], j: usize| row.get(j).cloned().unwrap_or_else(BigInt::zero);
    let mut pivots: Vec<usize> = Vec::new();
    let mut r = 0;
    let mut c = 0;
    while c < cols && r < m.len() {
        let Some(p) = (r..m.len()).find(|&i| !at(&m[i], c).is_zero()) else {
            c += 1;
            continue;
        };
        m.swap(r, p);
        let pr = m[r].clone();
        for (i, row) in m.iter_mut().enumerate() {
            if i == r || at(row, c).is_zero() {
                continue;
            }
            let a = at(&pr, c);
            let b = at(row, c);
            let next: Vec<BigInt> = row
                .iter()
                .enumerate()
                .map(|(j, x)| x * &a - at(&pr, j) * &b)
                .collect();
            *row = reduce(next);
        }
        pivots.push(c);
        r += 1;
        c += 1;
    }
    let lcm = pivots.iter().enumerate().fold(BigInt::one(), |l, (i, &c)| {
        let ap = at(&m[i], c).abs();
        (&l / gcd(&l, &ap)) * ap
    });
    let mut basis = Vec::new();
    for f in 0..cols {
        if pivots.contains(&f) {
            continue;
        }
        let mut v = vec![BigInt::zero(); cols];
        v[f] = lcm.clone();
        for (i, &c) in pivots.iter().enumerate() {
            // BigInt division truncates toward zero, as JS bigint `/` does
            v[c] = (-at(&m[i], f) * &lcm) / at(&m[i], c);
        }
        basis.push(reduce(v));
    }
    basis
}

/// Whether some combination of the basis is positive in every species, by Fourier–Motzkin on the strict system
/// `y·b_j > 0`. Too many constraints gives up, and blank beats a guess.
fn has_positive_combination(basis: &[Vec<BigInt>]) -> bool {
    let k = basis.len();
    let mut rows: Vec<Vec<BigInt>> = (0..basis[0].len())
        .map(|j| basis.iter().map(|b| b[j].clone()).collect())
        .collect();
    for v in (0..k).rev() {
        let pos: Vec<&Vec<BigInt>> = rows.iter().filter(|r| r[v].is_positive()).collect();
        let neg: Vec<&Vec<BigInt>> = rows.iter().filter(|r| r[v].is_negative()).collect();
        let mut next: Vec<Vec<BigInt>> = rows.iter().filter(|r| r[v].is_zero()).cloned().collect();
        for p in &pos {
            for n in &neg {
                let row: Vec<BigInt> = p
                    .iter()
                    .enumerate()
                    .map(|(i, x)| x * -&n[v] + &n[i] * &p[v])
                    .collect();
                next.push(reduce(row));
            }
        }
        // `new Map(next.map((r) => [r.join(','), r])).values()`: one of each, in first-seen order
        let mut seen = std::collections::HashSet::new();
        rows = next
            .into_iter()
            .filter(|r| seen.insert(r.clone()))
            .collect();
        if rows.len() > 5000 {
            return false;
        }
    }
    rows.is_empty()
}

pub fn balance(reaction: &Reaction) -> Balance {
    let left = &reaction.left;
    let right = &reaction.right;
    let all: Vec<&Species> = left.iter().chain(right).collect();
    // a repeated species (on one side or both) adds a free direction that isn't a reaction
    let mut keys = std::collections::HashSet::new();
    if !all.iter().all(|s| keys.insert(s.key.as_str())) {
        return Balance::Impossible;
    }
    let mut elements: Vec<&str> = Vec::new();
    for s in &all {
        for (el, _) in &s.atoms {
            if !elements.contains(&el.as_str()) {
                elements.push(el);
            }
        }
    }
    let sign_of = |i: usize| {
        if i < left.len() {
            BigInt::one()
        } else {
            -BigInt::one()
        }
    };
    let mut rows: Vec<Vec<BigInt>> = elements
        .iter()
        .map(|el| {
            all.iter()
                .enumerate()
                .map(|(i, s)| count_of(&s.atoms, el) * sign_of(i))
                .collect()
        })
        .collect();
    if all.iter().any(|s| !s.charge.is_zero()) {
        rows.push(
            all.iter()
                .enumerate()
                .map(|(i, s)| &s.charge * sign_of(i))
                .collect(),
        );
    }
    let basis = null_space(&rows, all.len());
    if basis.is_empty() {
        return Balance::Impossible;
    }
    if basis.len() > 1 {
        return if has_positive_combination(&basis) {
            Balance::Underdetermined
        } else {
            Balance::Impossible
        };
    }
    let mut v = basis[0].clone();
    if v[0].is_negative() {
        v = v.into_iter().map(|x| -x).collect();
    }
    if !v.iter().all(Signed::is_positive) {
        return Balance::Impossible;
    }
    let zero = |row: &Vec<BigInt>| {
        row.iter()
            .zip(&v)
            .fold(BigInt::zero(), |s, (x, y)| s + x * y)
            .is_zero()
    };
    if !rows.iter().all(zero) {
        return Balance::Impossible;
    }
    let half = |list: &[Species], off: usize| {
        list.iter()
            .enumerate()
            .map(|(i, s)| {
                if v[i + off].is_one() {
                    s.display.clone()
                } else {
                    format!("{} {}", v[i + off], s.display)
                }
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let display = format!("{} → {}", half(left, 0), half(right, left.len()));
    Balance::Balanced {
        coefficients: v,
        display,
    }
}

fn marked(r: &Reaction) -> bool {
    r.left.len() > 1 || r.right.len() > 1 || r.left.iter().chain(&r.right).any(|s| s.marked)
}

/// A reaction written with an arrow that looks chemical; `100 C -> F` doesn't. These answer blank rather than
/// falling back to other readings.
pub fn is_reaction_input(text: &str) -> bool {
    parse_reaction(text).is_some_and(|r| r.arrow != "=" && marked(&r))
}

/// The shown answer for a reaction line, or None to leave the line to the math engine; Some("") is a reaction
/// left blank on purpose. With `=` the line is only chemistry when it balances and uses no known variable names,
/// so `x + y = 3`, `H = CO` or `CO2 = C + O2` with those defined stay math.
pub fn chem_answer(text: &str, is_name: &dyn Fn(&str) -> bool) -> Option<String> {
    static ANY_ARROW: Lazy = Lazy::new(|| jsre::js(r"->|→|⟶|=", ""));
    if !jsre::test(&ANY_ARROW, text) {
        return None;
    }
    let r = parse_reaction(text)?;
    let strict = r.arrow == "=";
    let pair: Vec<&Species> = r.left.iter().chain(&r.right).collect();
    if strict && pair.iter().any(|s| is_name(&s.base)) {
        return None;
    }
    if !strict && !marked(&r) {
        return None;
    }
    let b = balance(&r);
    // `V1 = 9 V` is an assignment: one species a side whose typed coefficients the balance throws out
    if let Balance::Balanced { coefficients, .. } = &b
        && strict
        && pair.len() == 2
        && pair.iter().any(|sp| sp.typed.is_some())
    {
        let a = pair[0].typed.clone().unwrap_or_else(BigInt::one);
        let c = pair[1].typed.clone().unwrap_or_else(BigInt::one);
        if a * &coefficients[1] != c * &coefficients[0] {
            return None;
        }
    }
    match b {
        Balance::Balanced { display, .. } => Some(display),
        _ if strict => None,
        Balance::Underdetermined => Some("underdetermined".into()),
        Balance::Impossible => Some(String::new()),
    }
}

/// One parsed formula: atoms in the order written (counts as JS numbers) and whether it's marked.
#[derive(Clone, Debug)]
pub struct ParsedFormula {
    pub atoms: Vec<(String, f64)>,
    pub marked: bool,
}

/// One formula, the whole of `text`: `H2O`, `Ca(OH)2`, `CuSO4·5H2O`, `H₂O`.
pub fn parse_formula(text: &str) -> Option<ParsedFormula> {
    let mut sc = Scanner::new(&unsub(jsstr::trim(text)));
    let f = formula(&mut sc)?;
    if !sc.done() {
        return None;
    }
    Some(ParsedFormula {
        // `Number(n)`: the nearest double, which `to_f64` gives for any size
        atoms: f
            .atoms
            .into_iter()
            .map(|(el, n)| (el, n.to_f64().unwrap_or(f64::INFINITY)))
            .collect(),
        marked: f.marked,
    })
}

static MOLAR_ASK: Lazy = Lazy::new(|| {
    jsre::js(
        &jsstr::spaces(
            r"^(?:molar\s+mass|molecular\s+(?:weight|mass)|formula\s+(?:weight|mass)|chem)(?:\s+of)?\s+(\S+)$",
        ),
        "i",
    )
});
static MOLAR_AFTER: Lazy = Lazy::new(|| jsre::js(&jsstr::spaces(r"^(\S+)\s+molar\s+mass$"), "i"));

/// Whether a line asks for a molar mass in words: `molar mass H2O`, `H2O molar mass`.
pub fn is_molar_mass_input(text: &str) -> bool {
    let t = jsstr::trim(text);
    jsre::test(&MOLAR_ASK, t) || jsre::test(&MOLAR_AFTER, t)
}

#[derive(Clone, Debug)]
pub struct MolarMass {
    pub mass: f64,
    pub display: String,
}

/// The molar mass a line asks for, as `18.015 g/mol`, or None to leave the line alone. In words any formula goes
/// (`molar mass NaCl`); on its own a formula needs digits, brackets or a hydrate dot and two elements (`H2O`,
/// `Ca(OH)2`), and none of its letters may be a name in use, so `V2` and a stored `H` stay math.
pub fn molar_mass_answer(text: &str, is_name: &dyn Fn(&str) -> bool) -> Option<MolarMass> {
    let t = jsstr::trim(text);
    let asked = [&MOLAR_ASK, &MOLAR_AFTER].iter().find_map(|re| {
        re.captures(t)
            .ok()
            .flatten()
            .map(|c| jsre::group(&c, 1).to_string())
    });
    let f = parse_formula(asked.as_deref().unwrap_or(t))?;
    if asked.is_none()
        && (!f.marked
            || f.atoms.len() < 2
            || is_name(t)
            || f.atoms.iter().any(|(el, _)| is_name(el)))
    {
        return None;
    }
    let found = elements::formula_of(f.atoms.iter().map(|(el, n)| (el.as_str(), *n)))?;
    Some(MolarMass {
        mass: crate::jsnum::parse(&found.mass).unwrap_or(f64::NAN),
        display: format!("{} g/mol", found.mass),
    })
}

/// Copy text for a shown balanced reaction: ascii digits, caret charges and `->`. Anything else is unchanged.
pub fn chem_copy_text(shown: &str) -> String {
    static CHARGE: Lazy = Lazy::new(|| jsre::js(r"([⁰¹²³⁴⁵⁶⁷⁸⁹]*)([⁺⁻])", ""));
    if !shown.contains(" → ") {
        return shown.to_string();
    }
    let plain = unsub(shown);
    let plain = jsre::replace_all(&CHARGE, &plain, |c| {
        let m = unsup(jsre::group(c, 1));
        format!("^{m}{}", if jsre::group(c, 2) == "⁻" { '-' } else { '+' })
    })
    .replacen(" → ", " -> ", 1);
    if parse_reaction(&plain).is_some() {
        plain
    } else {
        shown.to_string()
    }
}

// R = N_A·k, both exact since 2019
const R: f64 = 6.02214076e23 * 1.380649e-23;

/// The words `chem R` takes after it, offered by completion.
pub const GAS_CONSTANT_UNITS: [&str; 6] = ["atm", "bar", "torr", "mmhg", "cal", "si"];

#[derive(Clone, Debug)]
pub struct GasConstant {
    pub n: f64,
    pub unit: String,
}

/// `chem R`, `chem R atm`, `chem R torr`: the gas constant in those units, or None. `chem R` alone is SI; a word
/// after it picks the pressure or energy the answer is in.
pub fn gas_constant(text: &str) -> Option<GasConstant> {
    // JS `.` stops at every line terminator
    static ASK: Lazy = Lazy::new(|| {
        jsre::js(
            &jsstr::spaces(r"^chem\s+r(?:\s+([^\n\r\u{2028}\u{2029}]+))?$"),
            "i",
        )
    });
    static TABLE: crate::jsre::Lazy<Vec<(jsre::Regex, f64, &'static str)>> =
        crate::jsre::Lazy::new(|| {
            vec![
                (jsre::js(r"^(?:si|j|joules?)$", ""), R, "J·mol⁻¹·K⁻¹"),
                (
                    jsre::js(&jsstr::spaces(r"^(?:l\s*)?(?:atm|atmospheres?)$"), ""),
                    R / 101.325,
                    "L·atm·mol⁻¹·K⁻¹",
                ),
                (
                    jsre::js(&jsstr::spaces(r"^(?:l\s*)?bars?$"), ""),
                    R / 100.0,
                    "L·bar·mol⁻¹·K⁻¹",
                ),
                (
                    jsre::js(&jsstr::spaces(r"^(?:l\s*)?torr$"), ""),
                    (R * 760.0) / 101.325,
                    "L·torr·mol⁻¹·K⁻¹",
                ),
                (
                    jsre::js(&jsstr::spaces(r"^(?:l\s*)?mm\s*hg$"), ""),
                    (R * 760.0) / 101.325,
                    "L·mmHg·mol⁻¹·K⁻¹",
                ),
                (
                    jsre::js(r"^(?:cal|calories?)$", ""),
                    R / 4.184,
                    "cal·mol⁻¹·K⁻¹",
                ),
            ]
        });
    let m = ASK.captures(jsstr::trim(text)).ok().flatten()?;
    let word = m.get(1).map_or("si", |w| w.as_str());
    let word = jsstr::trim(word).to_lowercase();
    TABLE
        .iter()
        .find(|(re, _, _)| jsre::test(re, &word))
        .map(|(_, n, unit)| GasConstant {
            n: *n,
            unit: (*unit).to_string(),
        })
}

fn bigint_json(n: &BigInt) -> serde_json::Value {
    serde_json::json!({ "$bigint": n.to_string() })
}

fn bigint_arg(v: &serde_json::Value) -> Option<BigInt> {
    v.get("$bigint")?.as_str()?.parse().ok()
}

fn element_arg(args: &[serde_json::Value]) -> Option<elements::Element> {
    serde_json::from_value(crate::golden::decode(args.first()?)).ok()
}

/// Replays a recorded call of `engine/chem` or `lib/elements` (bigints as `{"$bigint": "…"}`); None for a
/// function not ported here.
pub fn golden_dispatch(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    use serde_json::{Value as Json, json};
    let s = |i: usize| crate::golden::str_arg(args, i).unwrap_or_default();
    let no = |_: &str| false;
    Some(match name {
        "chemAnswer" => json!(chem_answer(&s(0), &no)),
        "chemCopyText" => json!(chem_copy_text(&s(0))),
        "isMolarMassInput" => json!(is_molar_mass_input(&s(0))),
        "isReactionInput" => json!(is_reaction_input(&s(0))),
        "molarMassAnswer" => molar_mass_answer(&s(0), &no).map_or(
            Json::Null,
            |m| json!({ "mass": m.mass, "display": m.display }),
        ),
        "gasConstant" => {
            gas_constant(&s(0)).map_or(Json::Null, |g| json!({ "n": g.n, "unit": g.unit }))
        }
        "parseFormula" => parse_formula(&s(0)).map_or(
            Json::Null,
            |f| json!({ "atoms": f.atoms, "marked": f.marked }),
        ),
        "nullSpace" => {
            let matrix: Vec<Vec<BigInt>> = args
                .first()?
                .as_array()?
                .iter()
                .map(|r| {
                    r.as_array()
                        .map(|r| r.iter().filter_map(bigint_arg).collect())
                })
                .collect::<Option<_>>()?;
            let cols = crate::golden::f64_arg(args, 1)? as usize;
            Json::Array(
                null_space(&matrix, cols)
                    .iter()
                    .map(|v| Json::Array(v.iter().map(bigint_json).collect()))
                    .collect(),
            )
        }
        "ELEMENTS" => serde_json::to_value(elements::elements()).ok()?,
        "formulaOf" => {
            let atoms: Vec<(String, f64)> = args
                .first()?
                .as_array()?
                .iter()
                .map(|p| {
                    let p = p.as_array()?;
                    let sym = p.first()?.as_str()?.to_string();
                    let n = crate::golden::f64_arg(p, 1).unwrap_or(f64::NAN);
                    Some((sym, n))
                })
                .collect::<Option<_>>()?;
            serde_json::to_value(elements::formula_of(
                atoms.iter().map(|(s, n)| (s.as_str(), *n)),
            ))
            .ok()?
        }
        "formulaOfPicks" => {
            let picks: Vec<String> = args
                .first()?
                .as_array()?
                .iter()
                .map(|p| p.as_str().unwrap_or("").to_string())
                .collect();
            serde_json::to_value(elements::formula_of_picks(&picks)).ok()?
        }
        "gridCell" => serde_json::to_value(elements::grid_cell(&element_arg(args)?)).ok()?,
        "hasStandardWeight" => json!(elements::has_standard_weight(&element_arg(args)?)),
        "insertableMass" => json!(elements::insertable_mass(&element_arg(args)?)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no(_: &str) -> bool {
        false
    }

    #[test]
    fn balances() {
        assert_eq!(
            chem_answer("H2 + O2 -> H2O", &no).as_deref(),
            Some("2 H₂ + O₂ → 2 H₂O")
        );
        assert_eq!(
            chem_answer("Ag^+ + Cu -> Ag + Cu^2+", &no).as_deref(),
            Some("2 Ag⁺ + Cu → 2 Ag + Cu²⁺")
        );
        assert_eq!(chem_answer("100 C -> F", &no), None);
        assert_eq!(chem_answer("x + y = 3", &no), None);
    }

    #[test]
    fn null_space_matches_the_ts() {
        let b = |v: &[i64]| v.iter().map(|&x| BigInt::from(x)).collect::<Vec<_>>();
        assert_eq!(
            null_space(&[b(&[2, 4, -6])], 3),
            vec![b(&[-2, 1, 0]), b(&[3, 0, 1])]
        );
    }

    #[test]
    fn molar_masses_and_gas_constant() {
        assert_eq!(
            molar_mass_answer("molar mass of H2O", &no).unwrap().display,
            "18.015 g/mol"
        );
        assert!(molar_mass_answer("NaCl", &no).is_none());
        assert_eq!(gas_constant("chem R atm").unwrap().unit, "L·atm·mol⁻¹·K⁻¹");
        assert_eq!(
            chem_copy_text("C₃H₈ + 5 O₂ → 3 CO₂ + 4 H₂O"),
            "C3H8 + 5 O2 -> 3 CO2 + 4 H2O"
        );
    }
}
