//! `src/engine/units.ts`: the unit reader. A line with units is a quantity (`5 cm`), a conversion (`5 cm to in`,
//! `72 F in C`) or unit arithmetic (`10 kN / 20 mm^2`), worked out over SI values with dimension vectors and
//! shown in a unit that follows what was typed. Also the default display units, `quantityText` (an answer as
//! text the reader reads back), Tab alternatives, SI prefix stepping and the helpers the unit solver uses.
//!
//! The table lives in [`table`], the Settings choices in [`settings`], and the units mathjs itself reads inside
//! function calls (`sin(90 deg)`) in [`mathjs`].
//!
//! Strings are worked on as `char` slices where the TS indexes them; every unit name is in the BMP, so a char is
//! one UTF-16 unit there.

pub mod golden;
pub mod mathjs;
mod mathjs_table;
pub mod settings;
pub mod table;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::jsnum;
use crate::jsre::{self, Lazy};
use crate::types::{AngleMode, DefaultUnits, Meas, Value, ValueKind};

pub use settings::{
    UNIT_SETTING_GROUPS, UnitChoice, UnitSettingGroup, UnitSettingItem, default_units_equal,
    sanitize_default_units, sanitize_default_units_json,
};
use table::{Dim, DimVec, Prefix, Unit, UnitRef, by_id, id, same_scale, table as tbl};

pub const IMPROPER_UNIT_CONVERSION: &str = "improper unit conversion";

pub fn is_improper_unit_conversion(text: &str) -> bool {
    text.trim().to_lowercase() == IMPROPER_UNIT_CONVERSION
}

fn unit_error() -> Value {
    Value::text(IMPROPER_UNIT_CONVERSION)
}

fn js(p: &str, f: &str) -> jsre::Regex {
    jsre::js(p, f)
}

fn ptr_eq(a: &UnitRef, b: &UnitRef) -> bool {
    std::sync::Arc::ptr_eq(a, b)
}

fn valued(n: f64, unit: impl Into<String>, unit_id: Option<&str>) -> Value {
    Value {
        unit: Some(unit.into()),
        unit_id: unit_id.map(str::to_string),
        ..Value::number(n)
    }
}

// ---- text helpers over chars ----

type Cs = [char];

fn cs(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn st(c: &Cs) -> String {
    c.iter().collect()
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn trim_start(s: &Cs) -> &Cs {
    let k = s.iter().take_while(|c| is_space(**c)).count();
    &s[k..]
}

fn trim_end(s: &Cs) -> &Cs {
    let k = s.iter().rev().take_while(|c| is_space(**c)).count();
    &s[..s.len() - k]
}

fn trim(s: &Cs) -> &Cs {
    trim_start(trim_end(s))
}

fn lower(s: &Cs) -> String {
    st(s).to_lowercase()
}

fn is_letter(c: Option<&char>) -> bool {
    c.is_some_and(char::is_ascii_alphabetic)
}

fn is_alnum(c: Option<&char>) -> bool {
    c.is_some_and(char::is_ascii_alphanumeric)
}

fn last_lower(c: char) -> Option<char> {
    c.to_lowercase().last()
}

fn first_lower(c: char) -> Option<char> {
    c.to_lowercase().next()
}

// ---- matching typed units ----

/// A unit read from text, and the text left beside it.
struct Hit<'a> {
    unit: UnitRef,
    rest: &'a Cs,
}

/// `beforeTail`: the text before `token` at the end of `t`, or None when it isn't a separate token there.
fn before_tail<'a>(t: &'a Cs, token: &Cs) -> Option<&'a Cs> {
    if t.len() <= token.len() {
        return None;
    }
    let rest = &t[..t.len() - token.len()];
    if lower(&t[rest.len()..]) != st(token) {
        return None;
    }
    if is_letter(rest.last()) && is_letter(token.first()) {
        return None;
    }
    (!trim(rest).is_empty()).then_some(rest)
}

fn is_symbol_of_other(t: &str, unit: &UnitRef) -> bool {
    tbl()
        .by_symbol
        .get(t)
        .is_some_and(|us| us.iter().any(|u| !ptr_eq(u, unit)))
}

/// `caseClash`: `mPa` is a millipascal and `Mg` a megagram, though the table's `MPa` and `mg` match them
/// ignoring case.
fn case_clash(typed: &Cs, unit: &UnitRef) -> bool {
    let sym: Vec<char> = unit.symbol.chars().collect();
    let tail = st(&typed[1..]);
    let (t0, s0) = (typed[0], sym.first().copied());
    let typed_s = st(typed);
    // only the prefix letter's case differs; `MG` or `MPH` in caps keep their table reading, and `pa` is still Pa
    let prefix_case = s0.is_some_and(|s0| {
        t0 != s0
            && t0.to_lowercase().eq(s0.to_lowercase())
            && tail == st(&sym[1..])
            && is_symbol_of_other(&tail, unit)
    });
    // or it's a prefix on another unit's exact symbol, and not a name this unit lists: `pA` is a picoamp
    let other_symbol =
        typed_s != unit.symbol && !unit.names.contains(&typed_s) && is_symbol_of_other(&tail, unit);
    if !prefix_case && !other_symbol {
        return false;
    }
    let Some((_, prefix)) = table::case_prefixes().iter().find(|(c, _)| *c == t0) else {
        return false;
    };
    let Some(base) = match_bare_unit_at_start(&typed[1..]) else {
        return false;
    };
    if !base.rest.is_empty() {
        return false;
    }
    // `uF` is the table's µF either way
    scale_unit(&base.unit, prefix)
        .is_some_and(|s| s.dim != unit.dim || !same_scale(s.to_base, unit.to_base))
}

fn case_fits(typed: &Cs, exact: Option<&str>, whole: bool) -> bool {
    let Some(exact) = exact else { return true };
    if whole {
        return st(typed) == exact;
    }
    for (i, e) in exact.chars().enumerate() {
        if (e == 'b' || e == 'B') && typed.get(i) != Some(&e) {
            return false;
        }
    }
    true
}

fn match_unit_at_end(s: &Cs) -> Option<Hit<'_>> {
    let t = trim_end(s);
    let last = last_lower(*t.last()?)?;
    let t_ = tbl();
    for &i in t_.by_last.get(&last)? {
        let a = &t_.alias_index[i];
        let Some(rest) = before_tail(t, &a.chars) else {
            continue;
        };
        let typed = &t[rest.len()..];
        if !case_fits(typed, a.exact.as_deref(), a.whole) || case_clash(typed, &a.unit) {
            continue;
        }
        // `1 H` is a henry, hours are `h`/`hr`; `72 F` and `100 C` stay temperatures here
        let unit = if st(typed) == "H" {
            id("henry")
        } else {
            a.unit.clone()
        };
        return Some(Hit {
            unit,
            rest: trim_end(rest),
        });
    }
    None
}

/// `scaleUnit`: a unit with a prefix put on, the table's own when it has one of that size.
fn scale_unit(base: &Unit, prefix: &Prefix) -> Option<UnitRef> {
    if matches!(
        base.dim,
        Dim::Temperature | Dim::Digital | Dim::Dimensionless
    ) {
        return None;
    }
    // of the time units only seconds take prefixes (`ms`, `µs`)
    if base.dim == Dim::Time && base.id != "s" {
        return None;
    }
    let to_base = base.to_base * prefix.factor;
    if !to_base.is_finite() || to_base == 0.0 {
        return None;
    }
    Some(
        tbl()
            .list
            .iter()
            .find(|u| u.dim == base.dim && same_scale(u.to_base, to_base))
            .cloned()
            .unwrap_or_else(|| std::sync::Arc::new(table::prefixed_unit(prefix, base, None))),
    )
}

fn match_case_prefix_at_end<'a>(rest: &'a Cs, unit: &Unit) -> Option<Hit<'a>> {
    for (sym, prefix) in table::case_prefixes() {
        if rest.len() <= 1 || rest.last() != Some(sym) {
            continue;
        }
        let before = &rest[..rest.len() - 1];
        if is_letter(before.last()) {
            continue;
        }
        if trim(before).is_empty() {
            continue;
        }
        let Some(scaled) = scale_unit(unit, prefix) else {
            continue;
        };
        return Some(Hit {
            unit: scaled,
            rest: trim_end(before),
        });
    }
    None
}

fn match_prefixed_unit(s: &Cs) -> Option<Hit<'_>> {
    let end = match_unit_at_end(s)?;
    let t = trim_end(end.rest);
    if let Some(cased) = match_case_prefix_at_end(t, &end.unit) {
        return Some(cased);
    }
    for (name, prefix) in &tbl().prefix_index {
        if name.chars().count() == 1 {
            continue;
        }
        let Some(rest) = before_tail(t, &cs(name)) else {
            continue;
        };
        let Some(scaled) = scale_unit(&end.unit, prefix) else {
            continue;
        };
        return Some(Hit {
            unit: scaled,
            rest: trim_end(rest),
        });
    }
    Some(end)
}

fn parse_amount(expr: &Cs) -> Option<f64> {
    static RE: Lazy = Lazy::new(|| js(r"^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$", "i"));
    let t = st(trim(expr));
    if t.is_empty() || !jsre::test(&RE, &t) {
        return None;
    }
    jsnum::parse(&t).filter(|n| n.is_finite())
}

fn is_unicode_letter_or_digit(c: char) -> bool {
    static RE: Lazy = Lazy::new(|| jsre::compile(r"^[\p{L}0-9]$"));
    jsre::test(&RE, c.encode_utf8(&mut [0; 4]))
}

fn starts_with_token(s: &Cs, token: &Cs) -> bool {
    if s.len() < token.len() || lower(&s[..token.len()]) != st(token) {
        return false;
    }
    let next = s.get(token.len());
    let last = token.last();
    // `kΩ` is one token, not kelvin then ohm
    if next.is_some_and(|n| is_unicode_letter_or_digit(*n)) && is_alnum(last) {
        return false;
    }
    // `ft/s^2` is ft / s^2, not (ft/s)^2
    !(token.contains(&'/') && next == Some(&'^'))
}

/// `SI_CAPITALS`: in arithmetic `F`, `C`, `H` are farad, coulomb, henry (`1 F * 1 V` = 1 C).
fn si_capital(c: char) -> Option<&'static str> {
    match c {
        'F' => Some("farad"),
        'C' => Some("coulomb"),
        'H' => Some("henry"),
        _ => None,
    }
}

fn match_bare_unit_at_start(s: &Cs) -> Option<Hit<'_>> {
    let t = trim_start(s);
    let first = *t.first()?;
    if let Some(si) = si_capital(first)
        && !is_alnum(t.get(1))
    {
        return Some(Hit {
            unit: id(si),
            rest: &t[1..],
        });
    }
    let t_ = tbl();
    for &i in t_.by_first.get(&first_lower(first)?)? {
        let a = &t_.alias_index[i];
        if !starts_with_token(t, &a.chars) {
            continue;
        }
        let typed = &t[..a.chars.len()];
        if !case_fits(typed, a.exact.as_deref(), a.whole) || case_clash(typed, &a.unit) {
            continue;
        }
        return Some(Hit {
            unit: a.unit.clone(),
            rest: &t[a.chars.len()..],
        });
    }
    None
}

fn prefixed_unit_at_start<'a>(prefix: &Prefix, s: &'a Cs) -> Option<Hit<'a>> {
    let u = match_bare_unit_at_start(s)?;
    let scaled = scale_unit(&u.unit, prefix)?;
    Some(Hit {
        unit: scaled,
        rest: u.rest,
    })
}

fn match_spelled_prefix_at_start(t: &Cs) -> Option<Hit<'_>> {
    for (name, prefix) in &tbl().prefix_index {
        let n = name.chars().count();
        if n == 1 {
            continue;
        }
        if t.len() < n || lower(&t[..n]) != *name {
            continue;
        }
        if let Some(hit) = prefixed_unit_at_start(prefix, &t[n..]) {
            return Some(hit);
        }
    }
    None
}

fn match_unit_at_start(s: &Cs) -> Option<Hit<'_>> {
    let t = trim_start(s);
    let prefixed = match_spelled_prefix_at_start(t);
    let bare = match_bare_unit_at_start(t);
    // the longer match wins; a tie goes to the bare unit
    if let Some(p) = &prefixed
        && bare.as_ref().is_none_or(|b| p.rest.len() < b.rest.len())
    {
        return prefixed;
    }
    if bare.is_some() {
        return bare;
    }
    let first = *t.first()?;
    for (sym, prefix) in table::case_prefixes() {
        if first != *sym {
            continue;
        }
        if let Some(hit) = prefixed_unit_at_start(prefix, &t[1..]) {
            return Some(hit);
        }
    }
    None
}

/// Whether a lowercased word is one of the table's names or symbols.
pub fn is_unit_name(word: &str) -> bool {
    tbl().alias_names.contains(&word.to_lowercase())
}

/// A unit name, or a prefixed symbol written exactly as its unit shows (`ms`, `kV`, not `ka`).
pub fn reads_as_unit(word: &str) -> bool {
    if is_unit_name(word) {
        return true;
    }
    let w = cs(word);
    match_unit_at_start(&w).is_some_and(|h| h.rest.is_empty() && h.unit.symbol == word)
}

/// What `unitInfo` says of a word: its symbol and spelled-out name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitInfo {
    pub symbol: String,
    pub name: String,
}

/// A whole typed word read as a unit (`Nm` a newton meter, `nm` a nanometer), with its spelled-out name.
pub fn unit_info(word: &str) -> Option<UnitInfo> {
    if word.is_empty() || word.chars().any(is_space) {
        return None;
    }
    // µ (micro sign) and μ (mu) look alike, and u stands in for either
    let mut tries: Vec<String> = Vec::new();
    for w in [
        word.to_string(),
        word.replace('µ', "μ"),
        word.replace(['µ', 'μ'], "u"),
    ] {
        if !tries.contains(&w) {
            tries.push(w);
        }
    }
    for w in tries {
        let c = cs(&w);
        if let Some(hit) = match_unit_at_start(&c)
            && hit.rest.is_empty()
        {
            return Some(UnitInfo {
                symbol: hit.unit.symbol.clone(),
                name: spelled_name(&hit.unit),
            });
        }
    }
    None
}

// the plainest singular name: `meter` for m, `second` not `secs`, `gram` not a generated prefix pile-up,
// and spaced when there's a choice (`foot pound`, `newton meter`)
fn spelled_name(unit: &Unit) -> String {
    static NAME: Lazy = Lazy::new(|| js(r"^[a-z][a-z -]*[a-z]$", "i"));
    static SHORT: Lazy = Lazy::new(|| js(r"^(?:sec|secs|hrs?|mins?|degs?|deg[cfkr])$|sec$", "i"));
    let symbol = unit.symbol.to_lowercase().replace(['·', '*'], "");
    let all: std::collections::HashSet<String> =
        unit.names.iter().map(|n| n.to_lowercase()).collect();
    let plural = |w: &str| {
        (w.ends_with('s') && all.contains(&w[..w.len() - 1]))
            || (w.ends_with("es") && all.contains(&w[..w.len() - 2]))
    };
    // `kw h`, `deg c` and `millisec` are shorthands, not names
    let abbreviated = |n: &str| {
        n.split([' ', '-'])
            .any(|w| w.encode_utf16().count() < 3 || jsre::test(&SHORT, w))
    };
    let spelled: Vec<&String> = unit
        .names
        .iter()
        .filter(|n| {
            jsre::test(&NAME, n)
                && n.to_lowercase() != symbol
                && !plural(&n.to_lowercase())
                && !abbreviated(n)
        })
        .collect();
    let score = |n: &str| n.len() as f64 - if n.contains([' ', '-']) { 1.5 } else { 0.0 };
    let pick = |min: usize| {
        let mut best: Option<&String> = None;
        for n in spelled.iter().filter(|n| n.len() >= min) {
            if best.is_none_or(|b| score(n) < score(b)) {
                best = Some(n);
            }
        }
        best.cloned()
    };
    pick(4)
        .or_else(|| pick(3))
        .unwrap_or_else(|| unit.symbol.clone())
}

// ---- quantities ----

const ANGLE_AXIS: usize = 5;
const ZERO: DimVec = [0.0; 7];

/// Sig figs (∞ = exact) and relative ± uncertainty; NaN once an addition mixes measured parts. `digits` marks a
/// ± still exactly as typed.
#[derive(Clone, Debug)]
struct Prec {
    sig: f64,
    rel: f64,
    digits: Option<String>,
}

fn lost() -> Prec {
    Prec {
        sig: f64::NAN,
        rel: f64::NAN,
        digits: None,
    }
}

/// The units as typed: each with its power, and how many US and metric units were typed.
#[derive(Clone, Debug, Default)]
struct Via {
    u: Vec<(UnitRef, f64)>,
    us: f64,
    si: f64,
}

/// `Qty`: an SI value with its dimensions. `bare` is a unit on its own, with no number.
#[derive(Clone, Debug)]
struct Qty {
    si: f64,
    dim: DimVec,
    prefer: Option<UnitRef>,
    prec: Option<Prec>,
    bare: bool,
    via: Option<Via>,
}

impl Qty {
    fn plain(si: f64) -> Qty {
        Qty {
            si,
            dim: ZERO,
            prefer: None,
            prec: None,
            bare: false,
            via: None,
        }
    }
}

/// US customary units; every other unit with a system (not time, angle, data, ...) counts as metric.
const US_UNIT_IDS: &[&str] = &[
    "in", "ft", "yd", "mi", "mil", "fathom", "rod", "chain", "furlong", "league", "oz", "lb", "st",
    "ton", "longton", "grain", "slug", "slinch", "f", "r", "tsp", "tbsp", "floz", "cup", "pt",
    "qt", "usgal", "impgal", "peck", "bushel", "barrel", "cord", "in3", "ft3", "yd3", "in2",
    "sqft", "yd2", "acre", "sqmi", "mph", "fps", "ftlbf", "btu", "therm", "hp", "psi", "ksi",
    "inhg", "ozf", "lbf", "kip",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum System {
    Us,
    Si,
}

fn unit_system(unit: &Unit) -> Option<System> {
    if US_UNIT_IDS.contains(&unit.id.as_str()) {
        return Some(System::Us);
    }
    match unit.dim {
        Dim::Time
        | Dim::Angle
        | Dim::Digital
        | Dim::Datarate
        | Dim::Dimensionless
        | Dim::Frequency => None,
        _ => Some(System::Si),
    }
}

/// `siOf`: volume's table base is the litre, not m³.
fn si_of(unit: &Unit) -> f64 {
    unit.to_base * if unit.dim == Dim::Volume { 0.001 } else { 1.0 }
}

fn pow_units(u: &[(UnitRef, f64)], e: f64) -> Vec<(UnitRef, f64)> {
    u.iter().map(|(unit, p)| (unit.clone(), p * e)).collect()
}

fn merge_units(a: &[(UnitRef, f64)], b: &[(UnitRef, f64)]) -> Vec<(UnitRef, f64)> {
    let mut out: Vec<(UnitRef, f64)> = a.to_vec();
    for (unit, p) in b {
        if let Some(hit) = out.iter_mut().find(|(u, _)| ptr_eq(u, unit)) {
            hit.1 += p;
        } else {
            out.push((unit.clone(), *p));
        }
    }
    out.retain(|(_, p)| *p != 0.0);
    out
}

fn via_of(q: &Qty) -> Via {
    q.via.clone().unwrap_or_default()
}

fn mul_via(a: &Qty, b: &Qty, sign: f64) -> Option<Via> {
    if a.via.is_none() && b.via.is_none() {
        return None;
    }
    let (x, y) = (via_of(a), via_of(b));
    Some(Via {
        u: merge_units(&x.u, &pow_units(&y.u, sign)),
        us: x.us + y.us,
        si: x.si + y.si,
    })
}

fn units_size(u: &[(UnitRef, f64)]) -> f64 {
    u.iter().fold(1.0, |f, (unit, p)| f * si_of(unit).powf(*p))
}

/// Products, quotients and powers: fewest sig figs, relative uncertainties add.
fn prec_mul(a: &Qty, b: &Qty) -> Option<Prec> {
    if a.prec.is_none() && b.prec.is_none() {
        return None;
    }
    let exact = Prec {
        sig: f64::INFINITY,
        rel: 0.0,
        digits: None,
    };
    let x = a.prec.clone().unwrap_or_else(|| exact.clone());
    let y = b.prec.clone().unwrap_or(exact);
    let digits = if b.bare {
        x.digits.clone()
    } else if a.bare {
        y.digits.clone()
    } else {
        None
    };
    Some(Prec {
        // Math.min: NaN wins
        sig: if x.sig.is_nan() || y.sig.is_nan() {
            f64::NAN
        } else {
            x.sig.min(y.sig)
        },
        rel: x.rel + y.rel,
        digits,
    })
}

fn add_vec(a: &DimVec, b: &DimVec) -> DimVec {
    std::array::from_fn(|i| a[i] + b[i])
}

fn sub_vec(a: &DimVec, b: &DimVec) -> DimVec {
    std::array::from_fn(|i| a[i] - b[i])
}

fn scale_vec(a: &DimVec, k: f64) -> DimVec {
    std::array::from_fn(|i| a[i] * k)
}

fn is_zero_vec(a: &DimVec) -> bool {
    a.iter().all(|x| *x == 0.0)
}

fn is_freq(d: &DimVec) -> bool {
    *d == Dim::Frequency.vec()
}

fn is_time(d: &DimVec) -> bool {
    *d == Dim::Time.vec()
}

fn is_ang_vel(d: &DimVec) -> bool {
    *d == sub_vec(&Dim::Angle.vec(), &Dim::Time.vec())
}

fn is_energy(d: &DimVec) -> bool {
    *d == Dim::Energy.vec()
}

fn is_power(d: &DimVec) -> bool {
    *d == Dim::Power.vec()
}

/// ft³ is ft to the 3rd and ft·lbf is ft times lbf, so `lb/ft^3 * ft` cancels to lb/ft².
fn typed_parts(unit: &UnitRef) -> Vec<(UnitRef, f64)> {
    if unit.id == "ftlbf" {
        return vec![(id("lbf"), 1.0), (id("ft"), 1.0)];
    }
    let torque: Option<[(&str, f64); 2]> = match unit.id.as_str() {
        "nm_torque" => Some([("n", 1.0), ("m", 1.0)]),
        "knm_torque" => Some([("kilonewton", 1.0), ("m", 1.0)]),
        "mnm_torque" => Some([("mega_n", 1.0), ("m", 1.0)]),
        "nmm_torque" => Some([("n", 1.0), ("mm", 1.0)]),
        _ => None,
    };
    if let Some(parts) = torque {
        return parts.iter().map(|(i, p)| (id(i), *p)).collect();
    }
    let sym: Vec<char> = unit.symbol.chars().collect();
    // /^(\S+?)([²³])$/
    if sym.len() >= 2
        && let Some(&last) = sym.last()
        && (last == '²' || last == '³')
        && !sym[..sym.len() - 1].iter().any(|c| is_space(*c))
    {
        let power = if last == '²' { 2.0 } else { 3.0 };
        let head = st(&sym[..sym.len() - 1]);
        let want = if power == 2.0 { Dim::Area } else { Dim::Volume };
        if let Some(len) = tbl()
            .list
            .iter()
            .find(|u| u.dim == Dim::Length && u.symbol == head)
            && unit.dim == want
            && same_scale(si_of(len).powf(power), si_of(unit))
        {
            return vec![(len.clone(), power)];
        }
    }
    vec![(unit.clone(), 1.0)]
}

const METRIC_MOMENTS: [(&str, f64); 4] =
    [("N·mm", 1e-3), ("N·m", 1.0), ("kN·m", 1e3), ("MN·m", 1e6)];

/// A metric moment reads 1 to 999 on the N·mm, N·m, kN·m, MN·m ladder (`1000 N * 500 mm` is 500 N·m).
fn metric_moment(n: f64, label: &str, size: f64) -> Value {
    let a = n.abs();
    if a == 0.0 || (1.0..1000.0).contains(&a) {
        return valued(n, label, None);
    }
    let si = n * size;
    let fit = METRIC_MOMENTS
        .iter()
        .find(|(_, f)| (si / f).abs() >= 1.0 && (si / f).abs() < 1000.0)
        .copied()
        .unwrap_or(if si.abs() < 1e-3 {
            METRIC_MOMENTS[0]
        } else {
            METRIC_MOMENTS[3]
        });
    valued(si / fit.1, fit.0, None)
}

/// A force times a length (`kN*m`, `lbf*in`) is a torque or moment, kept as typed rather than read as energy.
fn is_moment(u: &[(UnitRef, f64)]) -> bool {
    u.len() == 2
        && u.iter().all(|(_, p)| *p == 1.0)
        && u.iter().any(|(x, _)| x.dim == Dim::Force)
        && u.iter().any(|(x, _)| x.dim == Dim::Length)
}

fn unit_qty(unit: &UnitRef) -> Qty {
    let sys = unit_system(unit);
    Qty {
        si: si_of(unit),
        dim: unit.dim.vec(),
        prefer: Some(unit.clone()),
        prec: None,
        bare: true,
        via: Some(Via {
            u: typed_parts(unit),
            us: if sys == Some(System::Us) { 1.0 } else { 0.0 },
            si: if sys == Some(System::Si) { 1.0 } else { 0.0 },
        }),
    }
}

/// An rpm rate counts revolutions (2π rad each); a bare `50/hr` or Hz is just a rate.
fn is_revs(q: &Qty) -> bool {
    is_freq(&q.dim) && q.prefer.as_ref().is_some_and(|p| p.id == "rpm")
}

fn mul_qty(a: &Qty, b: &Qty) -> Qty {
    let mut si = a.si * b.si;
    let dim = add_vec(&a.dim, &b.dim);
    let prec = prec_mul(a, b);
    if (is_energy(&a.dim) && is_revs(b)) || (is_revs(a) && is_energy(&b.dim)) {
        si *= 2.0 * PI;
    }
    if (is_revs(a) && is_time(&b.dim)) || (is_time(&a.dim) && is_revs(b)) {
        return Qty {
            si: si * 2.0 * PI,
            dim: Dim::Angle.vec(),
            prefer: by_id("rad"),
            prec,
            bare: false,
            via: None,
        };
    }
    let prefer = if is_zero_vec(&a.dim) {
        b.prefer.clone()
    } else if is_zero_vec(&b.dim) {
        a.prefer.clone()
    } else {
        None
    };
    Qty {
        si,
        dim,
        prefer,
        prec,
        bare: false,
        via: mul_via(a, b, 1.0),
    }
}

fn div_qty(a: &Qty, b: &Qty) -> Option<Qty> {
    if b.si == 0.0 {
        return None;
    }
    let mut si = a.si / b.si;
    let dim = sub_vec(&a.dim, &b.dim);
    if is_power(&a.dim) && is_revs(b) && is_energy(&dim) {
        si /= 2.0 * PI;
    }
    let prefer = if is_zero_vec(&b.dim) {
        a.prefer.clone()
    } else if is_zero_vec(&a.dim) {
        b.prefer.clone()
    } else {
        None
    };
    Some(Qty {
        si,
        dim,
        prefer,
        prec: prec_mul(a, b),
        bare: false,
        via: mul_via(a, b, -1.0),
    })
}

/// A lone angle in a named unit: 30 deg, 0.5 rad.
fn plain_angle(q: &Qty) -> Option<&UnitRef> {
    let p = q.prefer.as_ref()?;
    (p.dim == Dim::Angle
        && q.dim
            .iter()
            .enumerate()
            .all(|(i, x)| *x == if i == ANGLE_AXIS { 1.0 } else { 0.0 }))
    .then_some(p)
}

fn add_qty(a: &Qty, b: &Qty, sign: f64) -> Option<Qty> {
    // an angle is a number at heart, so a plain number beside one is in its unit: `asin(0.5)` then `ans + 30`
    if let Some(p) = plain_angle(a)
        && is_zero_vec(&b.dim)
    {
        let b2 = Qty {
            si: b.si * si_of(p),
            dim: a.dim,
            ..b.clone()
        };
        return add_qty(a, &b2, sign);
    }
    if let Some(p) = plain_angle(b)
        && is_zero_vec(&a.dim)
    {
        let a2 = Qty {
            si: a.si * si_of(p),
            dim: b.dim,
            ..a.clone()
        };
        return add_qty(&a2, b, sign);
    }
    if a.dim != b.dim {
        return None;
    }
    let si = a.si + sign * b.si;
    let via = (a.via.is_some() || b.via.is_some()).then(|| Via {
        u: via_of(if a.via.is_some() { a } else { b }).u,
        us: via_of(a).us + via_of(b).us,
        si: via_of(a).si + via_of(b).si,
    });
    Some(Qty {
        si,
        dim: a.dim,
        prefer: a.prefer.clone().or_else(|| b.prefer.clone()),
        prec: prec_add(a, b, si),
        bare: false,
        via,
    })
}

/// Sums: absolute uncertainties add, a number without ± counting as exact; sig figs alone aren't kept.
fn prec_add(a: &Qty, b: &Qty, si: f64) -> Option<Prec> {
    if a.prec.is_none() && b.prec.is_none() {
        return None;
    }
    let unc = a.si.abs() * a.prec.as_ref().map_or(0.0, |p| p.rel)
        + b.si.abs() * b.prec.as_ref().map_or(0.0, |p| p.rel);
    if unc.is_nan() || unc <= 0.0 || si == 0.0 {
        return Some(lost());
    }
    Some(Prec {
        sig: f64::INFINITY,
        rel: unc / si.abs(),
        digits: None,
    })
}

/// Exponents are exact; an uncertain one is not modelled.
fn pow_qty(a: &Qty, exp: &Qty) -> Option<Qty> {
    let e = exp.si;
    if !e.is_finite() || !is_zero_vec(&exp.dim) {
        return None;
    }
    if a.si < 0.0 && !jsnum::is_integer(e) {
        return None;
    }
    let prec = if exp
        .prec
        .as_ref()
        .is_some_and(|p| p.rel != 0.0 && !p.rel.is_nan())
    {
        Some(lost())
    } else {
        a.prec.as_ref().map(|p| Prec {
            sig: p.sig,
            rel: p.rel * e.abs(),
            digits: None,
        })
    };
    let bare = a.bare && exp.prec.is_none();
    let via = a.via.as_ref().map(|v| Via {
        u: pow_units(&v.u, e),
        ..v.clone()
    });
    Some(Qty {
        si: crate::eval::ops::js_pow(a.si, e),
        dim: scale_vec(&a.dim, e),
        prefer: if e.abs() == 1.0 {
            a.prefer.clone()
        } else {
            None
        },
        prec,
        bare,
        via,
    })
}

/// Sig figs and ± of a unit answer `n`, in the answer's own unit.
fn prec_meas(n: f64, p: Option<&Prec>) -> Option<Meas> {
    let p = p?;
    if p.sig.is_nan() || !n.is_finite() {
        return None;
    }
    let mut out = Meas::default();
    if p.sig.is_finite() {
        out.sig = Some(p.sig as u32);
        let dp = if n == 0.0 {
            p.sig - 1.0
        } else {
            p.sig - 1.0 - n.abs().log10().floor()
        };
        out.dp = Some(dp as i32);
    }
    if p.rel > 0.0 {
        out.unc = Some(n.abs() * p.rel);
        if let Some(d) = &p.digits {
            out.unc_digits = Some(d.clone());
        }
    }
    (out.sig.is_some() || out.unc.is_some()).then_some(out)
}

fn convert_qty(q: &Qty, target: &Qty) -> Option<f64> {
    if q.dim == target.dim {
        return (target.si != 0.0).then(|| q.si / target.si);
    }
    if is_freq(&q.dim) && is_ang_vel(&target.dim) {
        return Some(q.si * (2.0 * PI) / target.si);
    }
    if is_ang_vel(&q.dim) && is_freq(&target.dim) {
        return Some(q.si / (2.0 * PI) / target.si);
    }
    let no_angle = |d: &DimVec| -> DimVec {
        std::array::from_fn(|i| if i == ANGLE_AXIS { 0.0 } else { d[i] })
    };
    if no_angle(&q.dim) == no_angle(&target.dim) {
        return (target.si != 0.0).then(|| q.si / target.si);
    }
    None
}

const SI_UNIT_IDS: &[&str] = &[
    "kg", "m", "m2", "m3", "s", "amp", "rad", "n", "j", "w", "pa", "coulomb", "volt", "ohm",
    "farad", "henry", "hz", "mps", "mps2",
];

fn named_unit_for(dim: &DimVec) -> Option<UnitRef> {
    let matches: Vec<&UnitRef> = tbl().list.iter().filter(|u| u.dim.vec() == *dim).collect();
    matches
        .iter()
        .find(|u| SI_UNIT_IDS.contains(&u.id.as_str()) && same_scale(si_of(u), 1.0))
        .or_else(|| matches.iter().find(|u| same_scale(si_of(u), 1.0)))
        .or_else(|| matches.iter().find(|u| same_scale(u.to_base, 1.0)))
        .or_else(|| matches.first())
        .map(|u| (*u).clone())
}

/// Units a typed compound is built from: `lb/in³`, `kN/m`, `mm/s`, `kg/L`, `BTU/hr`, never `bar·m²` or `hp/rpm`.
fn compound_dim(d: Dim) -> bool {
    matches!(
        d,
        Dim::Length | Dim::Mass | Dim::Time | Dim::Force | Dim::Volume | Dim::Energy
    )
}

/// Physical constants are never the unit of an answer: `1 c` would read back as a coulomb.
const CONSTANT_UNIT_IDS: &[&str] = &["light", "emass", "pmass", "nmass", "amu"];

/// The answer's unit follows the units typed, in the system most of them are in (an even split keeps SI).
fn system_value(q: &Qty) -> Option<Value> {
    let via = q.via.as_ref()?;
    if via.us == via.si || is_zero_vec(&q.dim) || !q.si.is_finite() {
        return None;
    }
    let sys = if via.us > via.si {
        System::Us
    } else {
        System::Si
    };
    let typed = fold_units(&via.u, sys);
    let size = units_size(&typed);
    let label = compound_label(&typed);
    if let Some(label) = &label
        && is_moment(&via.u)
    {
        return Some(if sys == System::Si {
            metric_moment(q.si / size, label, size)
        } else {
            valued(q.si / size, label.clone(), None)
        });
    }
    let matches: Vec<&UnitRef> = tbl()
        .list
        .iter()
        .filter(|u| {
            u.dim.vec() == q.dim
                && unit_system(u) == Some(sys)
                && si_of(u) > 0.0
                && !u.id.contains('_')
                && !CONSTANT_UNIT_IDS.contains(&u.id.as_str())
        })
        .collect();
    // named units only, not generated prefixes (daL); lengths multiplied out read as powers of one (cm³, not mL)
    let powered = |u: &Unit| i32::from(!u.symbol.contains(['²', '³']));
    let mut exact: Vec<&UnitRef> = matches
        .iter()
        .copied()
        .filter(|u| same_scale(si_of(u), size))
        .collect();
    exact.sort_by_key(|u| powered(u));
    if let Some(exact) = exact.first() {
        return if sys == System::Si {
            engineering(q, exact)
        } else {
            in_unit(q, exact)
        };
    }
    if sys == System::Si
        && let Some(on_ladder) = ladder_at(&q.dim, size)
    {
        return engineering(q, &on_ladder);
    }
    if let Some(label) = &label
        && typed.len() <= 2
        && typed.iter().all(|(u, _)| compound_dim(u.dim))
    {
        return Some(valued(q.si / size, label.clone(), None));
    }
    if sys == System::Si {
        let named = named_unit_for(&q.dim)?;
        return if same_scale(si_of(&named), 1.0) {
            engineering(q, &named)
        } else {
            None
        };
    }
    // power × time is heat (BTU); any other energy is work (ft·lbf)
    let heat = typed.iter().any(|(u, p)| u.dim == Dim::Time && *p > 0.0);
    let counterpart_id = if is_energy(&q.dim) {
        if heat {
            "btu".to_string()
        } else {
            "ftlbf".to_string()
        }
    } else {
        named_unit_for(&q.dim)
            .and_then(|u| u.default_to.clone())
            .unwrap_or_default()
    };
    let counterpart = by_id(&counterpart_id)?;
    if counterpart.dim.vec() == q.dim && unit_system(&counterpart) == Some(System::Us) {
        return in_unit(q, &counterpart);
    }
    None
}

fn in_unit(q: &Qty, unit: &Unit) -> Option<Value> {
    let n = q.si / si_of(unit);
    n.is_finite()
        .then(|| valued(n, unit.symbol.clone(), Some(&unit.id)))
}

/// Typed units of one kind become one unit, from the answer's system (`in·ft` → in²); mixed metric prefixes fold
/// to the SI unit (`km·mm` → m²).
fn fold_units(u: &[(UnitRef, f64)], sys: System) -> Vec<(UnitRef, f64)> {
    let mut out: Vec<(UnitRef, f64)> = Vec::new();
    for (unit, _) in u {
        if out.iter().any(|(o, _)| o.dim == unit.dim) {
            continue;
        }
        let kin: Vec<&(UnitRef, f64)> = u.iter().filter(|(o, _)| o.dim == unit.dim).collect();
        let metric = kin
            .iter()
            .filter(|(o, _)| unit_system(o) == Some(System::Si))
            .count();
        let si = (sys == System::Si && metric > 1)
            .then(|| named_unit_for(&unit.dim.vec()))
            .flatten();
        let pick = match si {
            Some(si) if same_scale(si_of(&si), 1.0) => si,
            _ => {
                let rank = |x: &(UnitRef, f64)| {
                    (if unit_system(&x.0) == Some(sys) {
                        100.0
                    } else {
                        0.0
                    }) + x.1.abs()
                };
                let mut best = kin[0];
                for cur in &kin[1..] {
                    if rank(cur) > rank(best) {
                        best = cur;
                    }
                }
                best.0.clone()
            }
        };
        let power: f64 = kin.iter().map(|(_, e)| *e).sum();
        // a ratio of one kind (in / mm) cancels out
        if power != 0.0 {
            out.push((pick, power));
        }
    }
    out
}

/// A prefixed SI unit of exactly this size (GPa for kN/mm²).
fn ladder_at(dim: &DimVec, size: f64) -> Option<UnitRef> {
    for rid in PREFIX_ROOTS {
        let root = id(rid);
        if root.dim.vec() != *dim {
            continue;
        }
        let exp = (size / si_of(&root)).log10();
        let k = jsnum::round(exp);
        if (exp - k).abs() < 1e-9 && k % 3.0 == 0.0 && in_engineering_range(&root, k) {
            return ladder_unit(&root, k);
        }
    }
    None
}

/// A metric answer steps along its prefix ladder to read 1 to 999; lengths stop at mm.
fn engineering(q: &Qty, unit: &UnitRef) -> Option<Value> {
    let v = in_unit(q, unit)?;
    let a = v.n.abs();
    let Some(place) = ladder_place(unit) else {
        return Some(v);
    };
    if a == 0.0 {
        return Some(v);
    }
    let log = a.log10() + place.1;
    let to_mm = place.0.id == "m" && place.1 < -3.0 && log >= -5.0;
    // 999.9999999 ms is 1 s
    if (1.0..1000.0 * (1.0 - 1e-12)).contains(&a) && !to_mm {
        return Some(v);
    }
    let mut exp = (log / 3.0).floor() * 3.0;
    if place.0.id == "m" && exp < -3.0 && log >= -5.0 {
        exp = -3.0;
    }
    if !in_engineering_range(&place.0, exp) {
        return Some(v);
    }
    Some(move_on_ladder(&v, &place, exp).unwrap_or(v))
}

/// Everyday prefixes only (n to G); time stays in seconds, not ks; mass tops out at t.
fn in_engineering_range(root: &Unit, exp: f64) -> bool {
    if exp.abs() > 9.0 {
        return false;
    }
    if root.id == "s" && exp > 0.0 {
        return false;
    }
    !(root.id == "g" && exp > 6.0)
}

fn engineering_dim(d: Dim) -> bool {
    matches!(d, Dim::Pressure | Dim::Force | Dim::Energy | Dim::Power)
}

/// `lbf/ft`, `kg/m³`, `lbf·in`; None when nothing is left on top.
fn compound_label(u: &[(UnitRef, f64)]) -> Option<String> {
    static SYM: Lazy = Lazy::new(|| js(r"^[A-Za-zμ]+[²³]?$", ""));
    let part = |(unit, p): &(UnitRef, f64)| -> Option<String> {
        let e = p.abs();
        let sym = if unit.id == "lb" {
            "lb"
        } else {
            unit.symbol.as_str()
        };
        if !jsre::test(&SYM, sym) || (e != 1.0 && sym.contains(['²', '³'])) {
            return None;
        }
        Some(if e == 1.0 {
            sym.to_string()
        } else if e == 2.0 {
            format!("{sym}²")
        } else if e == 3.0 {
            format!("{sym}³")
        } else if jsnum::is_integer(e) {
            format!("{sym}^{}", jsnum::to_string(e))
        } else {
            return None;
        })
    };
    let mut top: Vec<&(UnitRef, f64)> = u.iter().filter(|(_, p)| *p > 0.0).collect();
    top.sort_by_key(|(x, _)| i32::from(x.dim != Dim::Force));
    let top: Vec<Option<String>> = top.into_iter().map(part).collect();
    let bottom: Vec<Option<String>> = u.iter().filter(|(_, p)| *p < 0.0).map(part).collect();
    if top.is_empty() || top.iter().chain(&bottom).any(Option::is_none) {
        return None;
    }
    let top: Vec<String> = top.into_iter().flatten().collect();
    let bottom: Vec<String> = bottom.into_iter().flatten().collect();
    let den = if bottom.len() > 1 {
        Some(format!("({})", bottom.join("·")))
    } else {
        bottom.first().cloned()
    };
    Some(match den {
        Some(d) if !d.is_empty() => format!("{}/{d}", top.join("·")),
        _ => top.join("·"),
    })
}

thread_local! {
    /// The angle an answer per angle (N m / deg) reads in: the calculator's deg/rad mode, set for each conversion.
    static ANGLE_SHOWN: Cell<AngleMode> = const { Cell::new(AngleMode::Rad) };
}

fn angle_shown() -> AngleMode {
    ANGLE_SHOWN.with(Cell::get)
}

fn with_angle<T>(mode: Option<AngleMode>, f: impl FnOnce() -> T) -> T {
    struct Restore(AngleMode);
    impl Drop for Restore {
        fn drop(&mut self) {
            ANGLE_SHOWN.with(|a| a.set(self.0));
        }
    }
    let _restore = Restore(angle_shown());
    if let Some(m) = mode {
        ANGLE_SHOWN.with(|a| a.set(m));
    }
    f()
}

fn angle_name(m: AngleMode) -> &'static str {
    match m {
        AngleMode::Deg => "deg",
        AngleMode::Rad => "rad",
    }
}

/// A unit no name fits, in base units, with newtons pulled out: `kg m^2 / s^2 K` reads `N m / K`.
fn format_compound(dim: &DimVec) -> String {
    const NAMES: [&str; 7] = ["kg", "m", "s", "A", "K", "rad", "B"];
    let mut dim = *dim;
    let mut num: Vec<String> = Vec::new();
    let mut den: Vec<String> = Vec::new();
    if dim[0] == 1.0 && dim[2] <= -2.0 {
        num.push("N".into());
        dim = add_vec(&dim, &[-1.0, -1.0, 2.0, 0.0, 0.0, 0.0, 0.0]);
    }
    for (i, e) in dim.iter().enumerate() {
        if *e == 0.0 || e.is_nan() {
            continue;
        }
        let s = if e.abs() == 1.0 {
            NAMES[i].to_string()
        } else {
            format!("{}^{}", NAMES[i], jsnum::to_string(e.abs()))
        };
        if *e > 0.0 {
            num.push(s);
        } else {
            den.push(s);
        }
    }
    if num.is_empty() && den.is_empty() {
        return String::new();
    }
    if den.is_empty() {
        return num.join(" ");
    }
    let left = if num.is_empty() {
        "1".to_string()
    } else {
        num.join(" ")
    };
    format!("{left} / {}", den.join(" "))
}

/// `1/Ω` rather than a slash-containing compound like `1/m/s`.
fn reciprocal_label(symbol: &str) -> Option<String> {
    let t = symbol.trim();
    if t.is_empty()
        || t.contains(|c: char| is_space(c) || c == '/' || c == '^')
        || t.starts_with('1')
    {
        return None;
    }
    Some(format!("1/{t}"))
}

fn as_reciprocal(q: &Qty, unit: &Unit) -> Option<Value> {
    if scale_vec(&unit.dim.vec(), -1.0) != q.dim {
        return None;
    }
    let label = reciprocal_label(&unit.symbol)?;
    let n = q.si * si_of(unit);
    n.is_finite().then(|| valued(n, label, None))
}

/// Something per angle that isn't a turning rate: `549.16 Nm / 0.293 deg` is 1874.27 N m / deg in deg mode.
fn angle_rate_value(q: &Qty) -> Option<Value> {
    let e = q.dim[ANGLE_AXIS];
    if e >= 0.0 {
        return None;
    }
    let rest: DimVec = std::array::from_fn(|i| if i == ANGLE_AXIS { 0.0 } else { q.dim[i] });
    if rest.iter().enumerate().all(|(i, x)| i == 2 || *x == 0.0) {
        return None;
    }
    let shown = angle_shown();
    let rest_label = if is_energy(&rest) {
        "N m".to_string()
    } else {
        match named_unit_for(&rest) {
            Some(named) if same_scale(si_of(&named), 1.0) => named.symbol.clone(),
            _ => format_compound(&rest),
        }
    };
    let angle = if e.abs() == 1.0 {
        angle_name(shown).to_string()
    } else {
        format!("{}^{}", angle_name(shown), jsnum::to_string(e.abs()))
    };
    let unit = if rest_label.contains('/') {
        format!("{rest_label} {angle}")
    } else {
        format!("{rest_label} / {angle}")
    };
    let n = if shown == AngleMode::Deg {
        q.si * crate::eval::ops::js_pow(180.0 / PI, e)
    } else {
        q.si
    };
    n.is_finite().then(|| valued(n, unit, None))
}

fn qty_to_value(q: &Qty, target: Option<&Qty>, target_label: Option<&str>) -> Option<Value> {
    let mut v = qty_value(q, target, target_label)?;
    if v.kind == ValueKind::Number
        && let Some(meas) = prec_meas(v.n, q.prec.as_ref())
    {
        v.meas = Some(meas);
    }
    Some(v)
}

fn qty_value(q: &Qty, target: Option<&Qty>, target_label: Option<&str>) -> Option<Value> {
    if let Some(target) = target {
        if !q.si.is_finite() {
            return Some(unit_error());
        }
        let n = match convert_qty(q, target) {
            Some(n) if n.is_finite() => n,
            _ => return Some(unit_error()),
        };
        let unit = target_label
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .or_else(|| target.prefer.as_ref().map(|p| p.symbol.clone()))
            .unwrap_or_else(|| format_compound(&target.dim));
        let unit_id = target
            .prefer
            .as_ref()
            .filter(|p| p.dim.vec() == target.dim && same_scale(target.si, si_of(p)))
            .map(|p| p.id.clone());
        return Some(if unit.is_empty() {
            Value::number(n)
        } else {
            Value {
                unit: Some(unit),
                unit_id,
                ..Value::number(n)
            }
        });
    }
    if !q.si.is_finite() {
        return None;
    }
    if is_zero_vec(&q.dim) {
        return Some(Value::number(q.si));
    }
    if let Some(per_angle) = angle_rate_value(q) {
        return Some(per_angle);
    }
    if let Some(p) = &q.prefer {
        let fits = p.dim.vec() == q.dim;
        // `200 GPa * 0.001` is 200 MPa: scaled stresses, forces, energies and powers keep an engineering prefix
        if !q.bare && fits && engineering_dim(p.dim) && unit_system(p) == Some(System::Si) {
            return engineering(q, p);
        }
        if fits {
            let n = q.si / si_of(p);
            if !n.is_finite() {
                return None;
            }
            return Some(valued(n, p.symbol.clone(), Some(&p.id)));
        }
    }
    if let Some(typed) = system_value(q) {
        return Some(typed);
    }
    if let Some(named) = named_unit_for(&q.dim) {
        let n = q.si / si_of(&named);
        if !n.is_finite() {
            return None;
        }
        return Some(valued(n, named.symbol.clone(), Some(&named.id)));
    }
    if let Some(p) = &q.prefer
        && let Some(rec) = as_reciprocal(q, p)
    {
        return Some(rec);
    }
    if let Some(inv) = named_unit_for(&scale_vec(&q.dim, -1.0))
        && let Some(rec) = as_reciprocal(q, &inv)
    {
        return Some(rec);
    }
    let unit = format_compound(&q.dim);
    Some(if unit.is_empty() {
        Value::number(q.si)
    } else {
        valued(q.si, unit, None)
    })
}

// ---- measurement bits of `measure.ts` the reader needs (WP-MEASURE owns the rest) ----

/// `literalMeas(text).sig`: `2.50` → 3, `6.02e23` → 3, `300.` → 3, a bare integer exact (∞), zero 0.
fn literal_sig(text: &str) -> f64 {
    let v = jsnum::parse(text).unwrap_or(f64::NAN);
    let low = text.to_lowercase();
    let mut parts = low.split('e');
    let mant = parts.next().unwrap_or("");
    let exp = parts.next();
    if !mant.contains('.') && exp.is_none() {
        return f64::INFINITY;
    }
    let digits = mant.replacen('.', "", 1);
    let digits = digits.trim_start_matches('0');
    if v == 0.0 {
        0.0
    } else {
        (digits.encode_utf16().count().max(1)) as f64
    }
}

/// `typedDigits`: `0.10` → `10`, `1e-1` → `1`; a whole number's trailing zeros don't count.
fn typed_digits(text: &str) -> String {
    let low = text.to_lowercase();
    let mant = low.split('e').next().unwrap_or("");
    let digits = mant.replacen('.', "", 1);
    let digits = digits.trim_start_matches('0');
    if mant.contains('.') {
        digits.to_string()
    } else {
        digits.trim_end_matches('0').to_string()
    }
}

// ---- the parser ----

struct Parser<'v> {
    s: Vec<char>,
    i: usize,
    used_unit: bool,
    incompatible: bool,
    // a unit symbol that is also a stored variable (`n = 5`, then `2 n`) is ambiguous, so the whole reading goes
    shadowed: bool,
    vars: &'v BTreeMap<String, f64>,
}

static NO_VARS: BTreeMap<String, f64> = BTreeMap::new();

impl<'v> Parser<'v> {
    fn new(s: &str, vars: &'v BTreeMap<String, f64>) -> Self {
        Parser {
            s: cs(s),
            i: 0,
            used_unit: false,
            incompatible: false,
            shadowed: false,
            vars,
        }
    }

    fn at(&self, k: usize) -> Option<char> {
        self.s.get(k).copied()
    }

    /// A plain variable (`price`, not a unit name) standing in for its stored number.
    fn parse_variable_ref(&mut self) -> Option<Qty> {
        self.skip();
        let rest = &self.s[self.i..];
        let first = rest.first()?;
        if !(first.is_ascii_alphabetic() || *first == '_') {
            return None;
        }
        let n = rest
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
            .count();
        let v = *self.vars.get(&st(&rest[..n]))?;
        if !v.is_finite() {
            return None;
        }
        self.i += n;
        Some(Qty::plain(v))
    }

    fn skip(&mut self) {
        while self.at(self.i) == Some(' ') {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.at(self.i)
    }

    fn eat(&mut self, ch: char) -> bool {
        self.skip();
        if self.at(self.i) != Some(ch) {
            return false;
        }
        self.i += 1;
        true
    }

    fn parse(&mut self) -> Option<Qty> {
        let q = self.parse_add();
        self.skip();
        if self.i != self.s.len() || !self.used_unit || self.shadowed {
            return None;
        }
        q
    }

    fn parse_add(&mut self) -> Option<Qty> {
        let mut left = self.parse_mul()?;
        loop {
            self.skip();
            let sign = match self.at(self.i) {
                Some('+') => 1.0,
                Some('-') => -1.0,
                _ => break,
            };
            self.i += 1;
            let right = self.parse_mul()?;
            match add_qty(&left, &right, sign) {
                Some(next) => left = next,
                None => {
                    if !is_zero_vec(&left.dim) && !is_zero_vec(&right.dim) {
                        self.incompatible = true;
                    }
                    return None;
                }
            }
        }
        Some(left)
    }

    fn parse_mul(&mut self) -> Option<Qty> {
        let mut left = self.parse_pow()?;
        loop {
            self.skip();
            let ch = self.at(self.i);
            if ch == Some('*') || ch == Some('/') {
                self.i += 1;
                let right = self.parse_pow()?;
                if ch == Some('*') {
                    left = mul_qty(&left, &right);
                    continue;
                }
                left = div_qty(&left, &right)?;
                continue;
            }
            // `5 (2 m)` and `(2 m) (3 s)` multiply, but after a unit `(` is a function call (`sec(0)`)
            let after_paren = || trim_end(&self.s[..self.i]).last() == Some(&')');
            if is_letter(ch.as_ref())
                || (ch == Some('(') && (is_zero_vec(&left.dim) || after_paren()))
            {
                let right = self.parse_pow()?;
                left = mul_qty(&left, &right);
                continue;
            }
            // `5 ft 10 in`: a juxtaposed quantity of the same dimension adds
            if ch.is_some_and(|c| c.is_ascii_digit() || c == '.') && !is_zero_vec(&left.dim) {
                let right = self.parse_pow()?;
                if left.dim != right.dim {
                    return None;
                }
                left = add_qty(&left, &right, 1.0)?;
                continue;
            }
            break;
        }
        Some(left)
    }

    fn parse_pow(&mut self) -> Option<Qty> {
        let base = self.parse_primary()?;
        self.skip();
        if self.at(self.i) != Some('^') {
            return Some(base);
        }
        self.i += 1;
        let exp = self.parse_primary()?;
        pow_qty(&base, &exp)
    }

    fn word_at(&self, k: usize, word: &str) -> bool {
        let w: Vec<char> = word.chars().collect();
        self.s.len() >= k + w.len()
            && lower(&self.s[k..k + w.len()]) == word
            && !is_alnum(self.s.get(k + w.len()))
    }

    fn parse_primary(&mut self) -> Option<Qty> {
        self.skip();
        if self.eat('(') {
            let inner = self.parse_add()?;
            if !self.eat(')') {
                return None;
            }
            return Some(inner);
        }
        if self.word_at(self.i, "pi") {
            self.i += 2;
            let pi = Qty::plain(PI);
            return Some(match self.parse_unit_ref() {
                Some(unit) => mul_qty(&pi, &unit),
                None => pi,
            });
        }
        // the golden ratio, unless phi is a variable
        if self.word_at(self.i, "phi") && !self.vars.contains_key("phi") {
            self.i += 3;
            let golden = Qty::plain((1.0 + 5f64.sqrt()) / 2.0);
            return Some(match self.parse_unit_ref() {
                Some(unit) => mul_qty(&golden, &unit),
                None => golden,
            });
        }
        let amount = match self.parse_amount() {
            Some(a) => Some(a),
            None => self.parse_variable_ref(),
        };
        if let Some(amount) = amount {
            if let Some(unit) = self.parse_unit_ref() {
                let raised = self.parse_unit_power(unit)?;
                return self.parse_per_units(mul_qty(&amount, &raised));
            }
            return Some(amount);
        }
        self.parse_unit_ref()
    }

    fn parse_unit_power(&mut self, unit: Qty) -> Option<Qty> {
        self.skip();
        if self.at(self.i) != Some('^') {
            return Some(unit);
        }
        self.i += 1;
        let exp = self.parse_primary()?;
        pow_qty(&unit, &exp)
    }

    /// `4 MB/s` is one rate, so `32 GB / 4 MB/s` divides by the rate, not by 4 MB and then by s.
    fn parse_per_units(&mut self, q: Qty) -> Option<Qty> {
        let mut out = q;
        while self.peek() == Some('/') {
            let at = self.i;
            self.i += 1;
            let Some(unit) = self.parse_unit_ref() else {
                self.i = at;
                break;
            };
            let den = self.parse_unit_power(unit)?;
            out = div_qty(&out, &den)?;
        }
        Some(out)
    }

    /// A number as written: `2.50` carries 3 sig figs, `5.0 ± 0.1` / `10 ± 5%` an uncertainty, and a whole-number
    /// fraction right before a unit is one amount (`3/8 in`).
    fn parse_amount(&mut self) -> Option<Qty> {
        let value = self.parse_number()?;
        let mut q = value.clone();
        if self.eat('±') {
            let start = self.i;
            let u = self.parse_number();
            let u = u.filter(|_| value.si != 0.0)?;
            let pct = self.eat('%');
            let unc = (if pct { (value.si * u.si) / 100.0 } else { u.si }).abs();
            let digits = if pct {
                None
            } else {
                let typed: String = self.s[start..self.i]
                    .iter()
                    .filter(|c| !(is_space(**c) || **c == '+' || **c == '-'))
                    .collect();
                Some(typed_digits(&typed)).filter(|d| !d.is_empty())
            };
            q = Qty {
                prec: Some(Prec {
                    sig: value.prec.as_ref().map_or(f64::INFINITY, |p| p.sig),
                    rel: unc / value.si.abs(),
                    digits,
                }),
                ..value
            };
        }
        let at = self.i;
        if self.eat('/') {
            let den = self.parse_number();
            self.skip();
            let vulgar = jsnum::is_integer(q.si)
                && q.prec.is_none()
                && den
                    .as_ref()
                    .is_some_and(|d| jsnum::is_integer(d.si) && d.si > 1.0 && d.prec.is_none());
            if vulgar && match_unit_at_start(&self.s[self.i..]).is_some() {
                return div_qty(&q, den.as_ref()?);
            }
            self.i = at;
        }
        Some(q)
    }

    fn parse_number(&mut self) -> Option<Qty> {
        static RE: Lazy = Lazy::new(|| js(r"^([+-]?)((?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)", "i"));
        self.skip();
        let rest = st(&self.s[self.i..]);
        let c = RE.captures(rest.as_str()).ok()??;
        let whole = jsre::group(&c, 0);
        let si = jsnum::parse(whole)?;
        if !si.is_finite() {
            return None;
        }
        self.i += whole.chars().count();
        let sig = literal_sig(jsre::group(&c, 2));
        Some(Qty {
            prec: sig.is_finite().then_some(Prec {
                sig,
                rel: 0.0,
                digits: None,
            }),
            ..Qty::plain(si)
        })
    }

    fn parse_unit_ref(&mut self) -> Option<Qty> {
        self.skip();
        let (unit, rest_len) = {
            let hit = match_unit_at_start(&self.s[self.i..])?;
            (hit.unit, hit.rest.len())
        };
        // °C and °F are offset scales: `72 F in C` converts, but they can't be multiplied or added
        if unit.id == "c" || unit.id == "f" {
            return None;
        }
        let end = self.s.len() - rest_len;
        let text = st(&self.s[self.i..end]);
        static WORD: Lazy = Lazy::new(|| js(r"[A-Za-z_][A-Za-z0-9_]*", ""));
        if WORD
            .find_iter(&text)
            .filter_map(Result::ok)
            .any(|w| self.vars.contains_key(w.as_str()))
        {
            self.shadowed = true;
        }
        self.i = end;
        self.used_unit = true;
        Some(unit_qty(&unit))
    }
}

// ---- conversions ----

fn preprocess(s: &str) -> String {
    static FEET_INCHES: Lazy = Lazy::new(|| js(r#"(\d+)'(\d+(?:\.\d+)?)""#, ""));
    static ANGSTROM: Lazy = Lazy::new(|| js(r"[ÅÅ](?!\p{L})", ""));
    static DEG_C: Lazy = Lazy::new(|| js(r"°\s*C\b", "i"));
    static DEG_F: Lazy = Lazy::new(|| js(r"°\s*F\b", "i"));
    static DEG_R: Lazy = Lazy::new(|| js(r"°\s*R\b", "i"));
    static MICRO: Lazy = Lazy::new(|| js(r"[µμ](?=[A-Za-zΩ])", ""));
    static SLASH: Lazy = Lazy::new(|| js(r"\s*\/\s*", ""));
    static SPACES: Lazy = Lazy::new(|| js(r"\s+", ""));
    static CONVERT: Lazy = Lazy::new(|| js(r"^convert\s+", "i"));
    let t = jsre::replace_all(&FEET_INCHES, s, |c| {
        format!("{} ft {} in", jsre::group(c, 1), jsre::group(c, 2))
    })
    .into_owned();
    let t = t
        .replace('π', "pi")
        .replace(['·', '⋅'], "*")
        .replace('τ', "tau")
        .replace('−', "-")
        .replace('℃', "°C")
        .replace('℉', "°F");
    let t = jsre::replace_all(&ANGSTROM, &t, |_| " angstrom".into()).into_owned();
    let t = jsre::replace_all(&DEG_C, &t, |_| " degc".into()).into_owned();
    let t = jsre::replace_all(&DEG_F, &t, |_| " degf".into()).into_owned();
    let t = jsre::replace_all(&DEG_R, &t, |_| " degr".into()).into_owned();
    let t = t.replace('°', " deg");
    let t = jsre::replace_all(&MICRO, &t, |_| "u".into()).into_owned();
    let t = t.replace('²', "^2").replace('³', "^3");
    let t = jsre::replace_all(&SLASH, &t, |_| "/".into()).into_owned();
    let t = jsre::replace_all(&SPACES, &t, |_| " ".into()).into_owned();
    let t = st(trim(&cs(&t)));
    jsre::replace_all(&CONVERT, &t, |_| String::new()).into_owned()
}

fn split_convert(src: &str) -> (String, Option<String>) {
    static RE: Lazy = Lazy::new(|| js(r"^(.*)\s+(to|into)\s+(.+)$", "i"));
    match RE.captures(src) {
        Ok(Some(c)) => (
            st(trim(&cs(jsre::group(&c, 1)))),
            Some(st(trim(&cs(jsre::group(&c, 3))))),
        ),
        _ => (src.to_string(), None),
    }
}

fn split_in_convert(src: &str) -> Option<(String, String)> {
    static RE: Lazy = Lazy::new(|| js(r"^(.*)\s+in\s+(.+)$", "i"));
    let c = RE.captures(src).ok()??;
    let left = st(trim(&cs(jsre::group(&c, 1))));
    let right = st(trim(&cs(jsre::group(&c, 2))));
    (!left.is_empty() && !right.is_empty()).then_some((left, right))
}

fn target_for(from: &Unit, defaults: &DefaultUnits) -> Option<UnitRef> {
    if let Some(chosen) = defaults.get(from.dim.as_str()).filter(|c| !c.is_empty())
        && let Some(to) = by_id(chosen)
        && to.dim == from.dim
    {
        return Some(to);
    }
    from.default_to.as_deref().and_then(by_id)
}

fn apply_default_unit(q: &Qty, defaults: &DefaultUnits) -> Option<Value> {
    if let Some(named) = named_unit_for(&q.dim)
        && let Some(to) = defaults
            .get(named.dim.as_str())
            .filter(|i| !i.is_empty())
            .and_then(|i| by_id(i))
        && to.dim.vec() == q.dim
    {
        return qty_to_value(q, Some(&unit_qty(&to)), Some(&to.symbol));
    }
    qty_to_value(q, None, None)
}

fn collapse_spaces(s: &str) -> String {
    static RE: Lazy = Lazy::new(|| js(r"\s+", ""));
    jsre::replace_all(&RE, s, |_| " ".into()).into_owned()
}

fn eval_unit_sides(
    left: &str,
    right: Option<&str>,
    defaults: &DefaultUnits,
    vars: &BTreeMap<String, f64>,
) -> Option<Value> {
    let mut lp = Parser::new(left, vars);
    let left_q = lp.parse();
    if lp.shadowed {
        return None;
    }
    let Some(right) = right else {
        let Some(left_q) = left_q else {
            return lp.incompatible.then(unit_error);
        };
        return apply_default_unit(&left_q, defaults);
    };
    let mut rp = Parser::new(right, vars);
    let right_q = rp.parse();
    if rp.shadowed {
        return None;
    }
    let Some(left_q) = left_q else {
        if lp.incompatible || (right_q.is_some() && lp.used_unit) {
            return Some(unit_error());
        }
        return None;
    };
    let right_q = right_q?;
    qty_to_value(&left_q, Some(&right_q), Some(&collapse_spaces(right)))
}

fn try_unit_expression(
    src: &str,
    defaults: &DefaultUnits,
    vars: &BTreeMap<String, f64>,
) -> Option<Value> {
    let (left, right) = split_convert(src);
    if let Some(right) = right {
        return eval_unit_sides(&left, Some(&right), defaults, vars);
    }
    // `in` is also inches, so it only converts when both sides parse and the dimensions fit
    if let Some((l, r)) = split_in_convert(src) {
        let left_q = Parser::new(&l, vars).parse();
        let right_q = Parser::new(&r, vars).parse();
        if let (Some(lq), Some(rq)) = (&left_q, &right_q) {
            if convert_qty(lq, rq).is_some() {
                return qty_to_value(lq, Some(rq), Some(&collapse_spaces(&r)));
            }
            return Some(unit_error());
        }
        // `5 cm in x` is a conversion to something that isn't a unit, not cm times inches times x
        if left_q.is_some() {
            return None;
        }
    }
    eval_unit_sides(src, None, defaults, vars)
}

fn try_simple_convert(src: &str, defaults: &DefaultUnits, keep_units: bool) -> Option<Value> {
    static CONV: Lazy = Lazy::new(|| js(r"^(.*)\s+(to|into|in)$", "i"));
    static SIGN: Lazy = Lazy::new(|| js(r"^[+-]", ""));
    let s = cs(src);
    let end = match_prefixed_unit(&s)?;
    let end_rest = st(end.rest);
    let (from, to, expr) = if let Ok(Some(c)) = CONV.captures(end_rest.as_str()) {
        let left = cs(jsre::group(&c, 1));
        let source = match_prefixed_unit(trim(&left))?;
        // a mismatch (`2 F in uF`) may still convert once the unit parser reads `F` as a farad
        if source.unit.dim != end.unit.dim {
            return None;
        }
        (source.unit.clone(), end.unit.clone(), st(source.rest))
    } else {
        let from = end.unit.clone();
        let target = if keep_units {
            Some(from.clone())
        } else {
            target_for(&from, defaults)
        };
        let Some(target) = target else {
            let amount = parse_amount(end.rest);
            return match amount {
                Some(a) if from.dim == Dim::Dimensionless => Some(Value::number(a * from.to_base)),
                _ => None,
            };
        };
        (from, target, end_rest.clone())
    };
    let amount = parse_amount(&cs(&expr))?;
    let out = convert_amount(amount, &from, &to);
    // conversion factors are exact, so `12.0 kg in lb` keeps 3 sig figs; offset scales don't
    let trimmed = st(trim(&cs(&expr)));
    let sig = literal_sig(&jsre::replace_all(&SIGN, &trimmed, |_| String::new()));
    let mut out = out;
    if from.dim != Dim::Temperature
        && out.kind == ValueKind::Number
        && let Some(meas) = prec_meas(
            out.n,
            sig.is_finite()
                .then_some(Prec {
                    sig,
                    rel: 0.0,
                    digits: None,
                })
                .as_ref(),
        )
    {
        out.meas = Some(meas);
    }
    Some(out)
}

fn to_kelvin(n: f64, id: &str) -> f64 {
    match id {
        "c" => n + 273.15,
        "f" => ((n - 32.0) * 5.0) / 9.0 + 273.15,
        "r" => (n * 5.0) / 9.0,
        _ => n,
    }
}

fn from_kelvin(k: f64, id: &str) -> f64 {
    match id {
        "c" => k - 273.15,
        "f" => ((k - 273.15) * 9.0) / 5.0 + 32.0,
        "r" => (k * 9.0) / 5.0,
        _ => k,
    }
}

fn convert_amount(amount: f64, from: &Unit, to: &Unit) -> Value {
    if from.dim != to.dim {
        return unit_error();
    }
    let n = if from.dim == Dim::Temperature {
        from_kelvin(to_kelvin(amount, &from.id), &to.id)
    } else {
        (amount * from.to_base) / to.to_base
    };
    if !n.is_finite() {
        return unit_error();
    }
    valued(n, to.symbol.clone(), Some(&to.id))
}

/// The unit reader: a quantity, a conversion or unit arithmetic, or None when the text has no units.
/// `keep_units` answers in the units typed; `angle_mode` is the angle an answer per angle reads in.
pub fn try_convert(
    text: &str,
    defaults: &DefaultUnits,
    variables: &BTreeMap<String, f64>,
    keep_units: bool,
    angle_mode: AngleMode,
) -> Option<Value> {
    try_convert_opt(text, defaults, variables, keep_units, Some(angle_mode))
}

/// `tryConvert` with the TS's optional angle mode (None keeps the one in force, radians at first).
pub fn try_convert_opt(
    text: &str,
    defaults: &DefaultUnits,
    variables: &BTreeMap<String, f64>,
    keep_units: bool,
    angle_mode: Option<AngleMode>,
) -> Option<Value> {
    with_angle(angle_mode, || {
        convert_text(&preprocess(text), defaults, variables, keep_units, 0)
    })
}

/// `5 kg in` is a conversion still being typed (`5 sq in` is square inches).
fn dangling_in(src: &str) -> bool {
    let c = cs(src);
    let n = c.len();
    if n < 3 || lower(&c[n - 2..]) != "in" {
        return false;
    }
    // (?<!\b(?:to|into|in|sq|cu|square|cubic))(?<=[A-Za-z])\s+in$ : the space run before the last `in`
    let mut p = n - 2;
    if !is_space(c[p - 1]) {
        return false;
    }
    while p > 0 && is_space(c[p - 1]) {
        p -= 1;
    }
    if p == 0 || !c[p - 1].is_ascii_alphabetic() {
        return false;
    }
    let word_char = |k: usize| c[k].is_ascii_alphanumeric() || c[k] == '_';
    for w in ["to", "into", "in", "sq", "cu", "square", "cubic"] {
        let wl = w.len();
        if p >= wl && lower(&c[p - wl..p]) == w && (p == wl || !word_char(p - wl - 1)) {
            return false;
        }
    }
    true
}

fn convert_text(
    src: &str,
    defaults: &DefaultUnits,
    variables: &BTreeMap<String, f64>,
    keep_units: bool,
    depth: u32,
) -> Option<Value> {
    static QUOTES: Lazy = Lazy::new(|| js(r#"^['"]+$"#, ""));
    static INTO: Lazy = Lazy::new(|| js(r"^(.*\S)\s+in\s+(?:to|into)\s+(.+)$", "i"));
    if src.is_empty() || jsre::test(&QUOTES, src) || dangling_in(src) {
        return None;
    }
    let again = |text: &str| -> Option<Value> {
        if depth < 6 {
            convert_text(
                &preprocess(text),
                defaults,
                variables,
                keep_units,
                depth + 1,
            )
        } else {
            None
        }
    };

    // `(1.1009 in to mm) / 10`: a conversion in brackets is done first, and its answer used like ans
    let folded = fold_conversions(src, &again);
    if folded != src {
        return again(&folded);
    }
    // `1.1009 in to mm / 10`: arithmetic after the target unit works on the converted answer
    if let Some((convert, rest)) = split_target_tail(src)
        && let Some(converted) = again(&convert)
        && let Some(text) = quantity_text_now(&converted)
    {
        return again(&format!("({text}) {rest}"));
    }

    let units = if keep_units {
        DefaultUnits::new()
    } else {
        sanitize_default_units(defaults)
    };
    // `m = 3`, then `5 m`: the variable wins, so the bare `number unit` reading is off
    let simple = if mentions_unit_like_variable(src, variables) {
        None
    } else {
        try_simple_convert(src, &units, keep_units)
    };
    let value = simple.or_else(|| try_unit_expression(src, &units, variables));
    // `ans in to mm`: once the left already has a unit, `in to` is into, not inches
    if let Ok(Some(c)) = INTO.captures(src) {
        let improper = value.as_ref().is_none_or(|v| {
            v.kind == ValueKind::Text
                && is_improper_unit_conversion(v.text.as_deref().unwrap_or(""))
        });
        if improper
            && let Some(read) = again(&format!("{} to {}", jsre::group(&c, 1), jsre::group(&c, 2)))
            && read.kind == ValueKind::Number
        {
            return Some(read);
        }
    }
    value
}

/// The first bracket whose own level holds a conversion, replaced by its answer as a quantity.
fn fold_conversions(src: &str, convert: &dyn Fn(&str) -> Option<Value>) -> String {
    static NESTED: Lazy = Lazy::new(|| js(r"\([^()]*\)", ""));
    static CONV: Lazy = Lazy::new(|| js(r"\s(?:to|into)\s", "i"));
    let c = cs(src);
    let mut opens: Vec<usize> = Vec::new();
    for i in 0..c.len() {
        if c[i] == '(' {
            opens.push(i);
        } else if c[i] == ')'
            && let Some(start) = opens.pop()
        {
            let inner = st(&c[start + 1..i]);
            // only this bracket's own level: nested brackets blanked out
            let own = jsre::replace_all(&NESTED, &inner, |m| {
                " ".repeat(jsre::group(m, 0).encode_utf16().count())
            });
            if !jsre::test(&CONV, &own) || own.contains(['(', ')']) {
                continue;
            }
            let Some(v) = convert(&st(trim(&cs(&inner)))) else {
                continue;
            };
            if let Some(text) = quantity_text_now(&v) {
                return format!("{}({text}){}", st(&c[..start]), st(&c[i + 1..]));
            }
        }
    }
    src.to_string()
}

/// `… to mm / 10` → convert `… to mm`, then `/ 10`; the target is units only.
fn split_target_tail(src: &str) -> Option<(String, String)> {
    static RE: Lazy = Lazy::new(|| {
        js(
            r"^(.*\s(?:to|into)\s+)(.*?[A-Za-zμ°²³\d])\s*([*/+-]\s*[\d.(].*)$",
            "i",
        )
    });
    static POWER: Lazy = Lazy::new(|| js(r"\^\s*-?\d+", ""));
    let c = RE.captures(src).ok()??;
    let target = st(trim(&cs(jsre::group(&c, 2))));
    let stripped = jsre::replace_all(&POWER, &target, |_| String::new());
    if stripped.chars().any(|ch| ch.is_ascii_digit())
        || Parser::new(&format!("1 {target}"), &NO_VARS)
            .parse()
            .is_none()
    {
        return None;
    }
    Some((
        format!("{}{target}", jsre::group(&c, 1)),
        jsre::group(&c, 3).to_string(),
    ))
}

fn mentions_unit_like_variable(src: &str, variables: &BTreeMap<String, f64>) -> bool {
    static WORD: Lazy = Lazy::new(|| js(r"[A-Za-z_][A-Za-z0-9_]*", ""));
    WORD.find_iter(src)
        .filter_map(Result::ok)
        .any(|w| variables.contains_key(w.as_str()) && is_unit_name(w.as_str()))
}

// ---- prefix ladders ----

/// Units that step through SI prefixes with ⌥↑/⌥↓ (kg and t sit on the gram ladder).
const PREFIX_ROOTS: [&str; 16] = [
    "m", "g", "s", "l", "j", "w", "pa", "hz", "n", "volt", "amp", "ohm", "farad", "henry",
    "coulomb", "ev",
];

fn prefix_exponent(p: &Prefix) -> f64 {
    jsnum::round(p.factor.log10())
}

/// Table units by id, plus the `kilo_n`-style ids of prefixed units synthesized on the fly.
fn unit_by_id(uid: Option<&str>) -> Option<UnitRef> {
    let uid = uid.filter(|u| !u.is_empty())?;
    if let Some(known) = by_id(uid) {
        return Some(known);
    }
    let cut = uid.find('_')?;
    if cut == 0 {
        return None;
    }
    let prefix = table::prefix_by_name(&uid[..cut])?;
    let base = by_id(&uid[cut + 1..])?;
    Some(std::sync::Arc::new(table::prefixed_unit(
        prefix,
        &base,
        Some(uid),
    )))
}

/// Where a unit sits on a prefix ladder: `kN` → N at 10³, `cm` → m at 10⁻².
fn ladder_place(unit: &Unit) -> Option<(UnitRef, f64)> {
    for rid in PREFIX_ROOTS {
        let root = id(rid);
        if root.dim != unit.dim {
            continue;
        }
        let exp = (unit.to_base / root.to_base).log10();
        let k = jsnum::round(exp);
        if (exp - k).abs() < 1e-9 {
            return Some((root, k));
        }
    }
    None
}

/// The unit at 10^exp on a ladder, preferring the table's own (kN, MPa, t).
fn ladder_unit(root: &UnitRef, exp: f64) -> Option<UnitRef> {
    if exp == 0.0 {
        return Some(root.clone());
    }
    let prefix = table::PREFIXES.iter().find(|p| prefix_exponent(p) == exp)?;
    if root.id == "g" && exp == 6.0 {
        return Some(id("tonne"));
    }
    let unit = table::prefixed_unit(prefix, root, None);
    Some(
        tbl()
            .list
            .iter()
            .find(|u| {
                u.dim == unit.dim && u.symbol == unit.symbol && same_scale(u.to_base, unit.to_base)
            })
            .cloned()
            .unwrap_or_else(|| std::sync::Arc::new(unit)),
    )
}

fn move_on_ladder(value: &Value, from: &(UnitRef, f64), exp: f64) -> Option<Value> {
    let to = ladder_unit(&from.0, exp)?;
    let shift = from.1 - exp;
    let scale = 10f64.powf(shift.abs());
    let n = if shift >= 0.0 {
        value.n * scale
    } else {
        value.n / scale
    };
    Some(valued(n, to.symbol.clone(), Some(&to.id)))
}

/// One engineering step (×10³) up or down the SI prefixes: `98 N` → `0.098 kN`.
pub fn step_prefix(value: &Value, dir: i32) -> Option<Value> {
    if value.kind != ValueKind::Number || !value.n.is_finite() {
        return None;
    }
    let unit = unit_by_id(value.unit_id.as_deref())?;
    let place = ladder_place(&unit)?;
    let exp = if dir > 0 {
        ((place.1 + 1.0) / 3.0).ceil() * 3.0
    } else {
        ((place.1 - 1.0) / 3.0).floor() * 3.0
    };
    if exp.abs() > 24.0 {
        return None;
    }
    move_on_ladder(value, &place, exp)
}

/// Re-express a value in a unit from the same prefix ladder (keeps a ⌥↑ choice while typing).
pub fn in_ladder_unit(value: &Value, unit_id: &str) -> Option<Value> {
    if value.kind != ValueKind::Number || !value.n.is_finite() {
        return None;
    }
    let from = ladder_place(&*unit_by_id(value.unit_id.as_deref())?)?;
    let to = ladder_place(&*unit_by_id(Some(unit_id))?)?;
    if !ptr_eq(&from.0, &to.0) {
        return None;
    }
    move_on_ladder(value, &from, to.1)
}

// ---- answers as text, alternatives, SI ----

/// `5 cm` (or `5.0 ± 0.1 cm`) as text that parses back to the same quantity, or None when the label doesn't.
pub fn quantity_text(value: &Value, angle_mode: AngleMode) -> Option<String> {
    quantity_text_opt(value, Some(angle_mode))
}

/// `quantityText` with the TS's optional angle mode.
pub fn quantity_text_opt(value: &Value, angle_mode: Option<AngleMode>) -> Option<String> {
    if value.kind != ValueKind::Number || value.unit.is_none() || !value.n.is_finite() {
        return None;
    }
    with_angle(angle_mode, || quantity_text_now(value))
}

fn quantity_text_now(v: &Value) -> Option<String> {
    let unit_label = v.unit.as_deref().filter(|u| !u.is_empty())?;
    if v.kind != ValueKind::Number || !v.n.is_finite() {
        return None;
    }
    // full precision, so a stored quantity chains like a plain number: y = 2 kg, y^3 is 8 kg^3
    let n = v.n;
    let unc = v
        .meas
        .as_ref()
        .and_then(|m| m.unc)
        .filter(|u| *u != 0.0 && !u.is_nan());
    let amount = match unc {
        Some(u) => format!("{} ± {}", jsnum::to_string(n), jsnum::to_string(u)),
        None => jsnum::to_string(n),
    };
    let shown = Some(angle_shown());
    let empty = DefaultUnits::new();
    let Some(uid) = v.unit_id.as_deref().filter(|u| !u.is_empty()) else {
        // a compound in SI base units (`200 kg / s^2`) must read back as itself
        let text = format!("{amount} {unit_label}");
        let back = try_convert_opt(&text, &empty, &NO_VARS, false, shown)?;
        return (back.kind == ValueKind::Number
            && back.unit.as_deref() == Some(unit_label)
            && back.unit_id.as_deref().is_none_or(str::is_empty)
            && same_scale(back.n, n))
        .then_some(text);
    };
    // a symbol that reads back as another unit (g for gee, not gram) falls back to one of its names
    let mut labels = vec![unit_label.to_string()];
    if let Some(u) = unit_by_id(Some(uid)) {
        labels.extend(u.names.iter().cloned());
    }
    for unit in labels {
        let text = format!("{amount} {unit}");
        let back = try_convert_opt(&format!("{text} to {unit}"), &empty, &NO_VARS, false, shown);
        if let Some(back) = back
            && back.kind == ValueKind::Number
            && back.unit_id.as_deref() == Some(uid)
            && same_scale(back.n, n)
        {
            return Some(text);
        }
    }
    None
}

/// A few other units worth tabbing to, picked from the Settings lists by how readable the number is.
pub fn unit_alternatives(value: &Value, limit: usize) -> Vec<Value> {
    if value.kind != ValueKind::Number || !value.n.is_finite() {
        return Vec::new();
    }
    let Some(from) = unit_by_id(value.unit_id.as_deref()) else {
        return Vec::new();
    };
    if value.n == 0.0 && from.dim != Dim::Temperature {
        return Vec::new();
    }
    let choices = settings::setting_items()
        .find(|item| item.dim == from.dim.as_str())
        .map_or(&[][..], |i| i.units);
    let mut out: Vec<Value> = Vec::new();
    for choice in choices {
        let Some(to) = by_id(choice.id) else { continue };
        if to.id == from.id || (to.symbol == from.symbol && same_scale(to.to_base, from.to_base)) {
            continue;
        }
        let v = convert_amount(value.n, &from, &to);
        if v.kind == ValueKind::Number && v.n.is_finite() {
            out.push(v);
        }
    }
    // 1 to 999 reads best; tiny numbers read worse than big ones
    let readability = |n: f64| {
        let mag = n.abs().log10();
        if mag < 0.0 {
            -mag * 2.0
        } else {
            (mag - 3.0).max(0.0)
        }
    };
    let mut order: Vec<usize> = (0..out.len()).collect();
    order.sort_by(|&a, &b| {
        readability(out[a].n)
            .partial_cmp(&readability(out[b].n))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let picked: Vec<usize> = order.into_iter().take(limit).collect();
    out.into_iter()
        .enumerate()
        .filter(|(i, _)| picked.contains(i))
        .map(|(_, v)| v)
        .collect()
}

/// A typed quantity (`10 km`, `9.8 m/s^2`) in SI, for solvers that do their own algebra.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiQuantity {
    pub si: f64,
    pub dim: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<String>,
}

/// None for anything but a number with a unit; temperatures other than kelvin carry an offset, so they're out.
pub fn si_quantity(text: &str) -> Option<SiQuantity> {
    let q = Parser::new(&preprocess(text), &NO_VARS).parse()?;
    if !q.si.is_finite() || is_zero_vec(&q.dim) {
        return None;
    }
    if q.dim[4] != 0.0 && q.prefer.as_ref().is_none_or(|p| p.id != "k") {
        return None;
    }
    let unit_id = q
        .prefer
        .as_ref()
        .filter(|p| p.dim.vec() == q.dim)
        .map(|p| p.id.clone());
    Some(SiQuantity {
        si: q.si,
        dim: q.dim.to_vec(),
        unit_id,
    })
}

/// An SI amount shown in `unit_id` when it fits the dimension, else the default or named unit.
pub fn si_value(
    si: f64,
    dim: &[f64],
    unit_id: Option<&str>,
    defaults: &DefaultUnits,
) -> Option<Value> {
    let d: DimVec = std::array::from_fn(|i| dim.get(i).copied().unwrap_or(0.0));
    let prefer = unit_by_id(unit_id).filter(|p| p.dim.vec() == d);
    let has = prefer.is_some();
    let q = Qty {
        si,
        dim: d,
        prefer,
        prec: None,
        bare: false,
        via: None,
    };
    if has {
        qty_to_value(&q, None, None)
    } else {
        apply_default_unit(&q, &sanitize_default_units(defaults))
    }
}

#[cfg(test)]
mod debug_probe {
    #[test]
    fn probe_fns() {
        let Ok(path) = std::env::var("PROBE_FNS") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        for line in text.lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(m), Some(name), Some(args)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let args: Vec<serde_json::Value> = serde_json::from_str(args).unwrap_or_default();
            let out = super::golden::dispatch(m, name, &args).unwrap_or(serde_json::Value::Null);
            println!(
                "{}",
                serde_json::json!({ "mod": m, "name": name, "args": args, "out": out })
            );
        }
    }

    #[test]
    fn probe() {
        let Ok(path) = std::env::var("PROBE_FILE") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let show = |v: Option<crate::types::Value>| match v {
            None => "None".to_string(),
            Some(v) => {
                let m = v.meas.unwrap_or_default();
                let f = |x: Option<f64>| x.map_or("-".into(), crate::jsnum::to_string);
                format!(
                    "{} {} {} {} {} {} {}",
                    crate::jsnum::to_string(v.n),
                    v.unit.unwrap_or_default(),
                    v.unit_id.unwrap_or_default(),
                    v.text.unwrap_or_default(),
                    f(m.sig.map(f64::from)),
                    f(m.dp.map(f64::from)),
                    f(m.unc)
                )
            }
        };
        for line in text.lines() {
            let (line, opts) = line.split_once('\t').unwrap_or((line, "{}"));
            let o: crate::types::EvaluateOptions = serde_json::from_str(opts).unwrap_or_default();
            let mut r = crate::evaluate_line(line, &o);
            if line.contains(" || ") {
                let lines: Vec<String> = line.split(" || ").map(str::to_string).collect();
                let all = crate::evaluate_sheet(&lines, &o);
                r.display = all
                    .iter()
                    .map(|x| format!("{}[{}]", x.display, x.quantity.clone().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join(" ; ");
                r.quantity = None;
            }
            let v = super::try_convert_opt(line, &o.default_units, &o.variables, false, None);
            println!(
                "{} => {} q={} | {}",
                serde_json::to_string(line).unwrap_or_default(),
                serde_json::to_string(&r.display).unwrap_or_default(),
                r.quantity.unwrap_or_default(),
                show(v)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(s: &str) -> String {
        try_convert_opt(s, &DefaultUnits::new(), &BTreeMap::new(), false, None)
            .map(|v| crate::format::format_value(&v, 12.0))
            .unwrap_or_default()
    }

    #[test]
    fn halfway_shortest_digits_take_the_even_one_as_js_does() {
        // 1024 pt^0.5 * 100 ha^3 lands exactly between two 17-digit spellings
        assert_eq!(jsnum::to_string(2227468278904209.25), "2227468278904209.2");
        assert_eq!(jsnum::to_string(0.1), "0.1");
    }

    #[test]
    fn improper_conversions_are_recognised() {
        assert!(is_improper_unit_conversion(" Improper unit conversion "));
    }

    #[test]
    fn reads_quantities_and_conversions() {
        assert_eq!(conv("5 cm to in"), "1.96850393701 in");
        assert_eq!(conv("72 F in C"), "22.2222222222 °C");
        assert_eq!(conv("10 kN / 20 mm^2"), "500 MPa");
        assert_eq!(conv("2 m + 3 kg"), IMPROPER_UNIT_CONVERSION);
        assert_eq!(conv("2 + 3"), "");
    }
}
