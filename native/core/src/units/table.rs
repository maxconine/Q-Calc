//! The unit table of `src/engine/units.ts`: units, SI prefixes, the generated prefixed units, the alias index the
//! readers search, and the lookups over them (`BY_ID`, `ALIAS_INDEX`, `PREFIX_INDEX`, `CASE_PREFIXES`).
//!
//! Built once, in the TS order, because the order decides matches: the alias index is searched first to last and
//! `UNIT_LIST.find` takes the first unit that fits. Units are shared as `Arc`s; identity (`Arc::ptr_eq`) stands in
//! for the TS object identity, so a prefixed unit made on the fly (`scaleUnit`'s fallback) is never "the same" as
//! another one made the same way, exactly as in the TS.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::sync::{Arc, LazyLock};

use crate::jsre::{self, Lazy};

/// A unit's kind of quantity (`Dim` in the TS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dim {
    Length,
    Mass,
    Temperature,
    Volume,
    Area,
    Speed,
    Time,
    Digital,
    Datarate,
    Energy,
    Power,
    Pressure,
    Force,
    Angle,
    Frequency,
    Acceleration,
    Charge,
    Current,
    Voltage,
    Resistance,
    Capacitance,
    Inductance,
    Dimensionless,
}

impl Dim {
    pub const ALL: [Dim; 23] = [
        Dim::Length,
        Dim::Mass,
        Dim::Temperature,
        Dim::Volume,
        Dim::Area,
        Dim::Speed,
        Dim::Time,
        Dim::Digital,
        Dim::Datarate,
        Dim::Energy,
        Dim::Power,
        Dim::Pressure,
        Dim::Force,
        Dim::Angle,
        Dim::Frequency,
        Dim::Acceleration,
        Dim::Charge,
        Dim::Current,
        Dim::Voltage,
        Dim::Resistance,
        Dim::Capacitance,
        Dim::Inductance,
        Dim::Dimensionless,
    ];

    /// The TS name (`'length'`), which is also the settings key.
    pub fn as_str(self) -> &'static str {
        match self {
            Dim::Length => "length",
            Dim::Mass => "mass",
            Dim::Temperature => "temperature",
            Dim::Volume => "volume",
            Dim::Area => "area",
            Dim::Speed => "speed",
            Dim::Time => "time",
            Dim::Digital => "digital",
            Dim::Datarate => "datarate",
            Dim::Energy => "energy",
            Dim::Power => "power",
            Dim::Pressure => "pressure",
            Dim::Force => "force",
            Dim::Angle => "angle",
            Dim::Frequency => "frequency",
            Dim::Acceleration => "acceleration",
            Dim::Charge => "charge",
            Dim::Current => "current",
            Dim::Voltage => "voltage",
            Dim::Resistance => "resistance",
            Dim::Capacitance => "capacitance",
            Dim::Inductance => "inductance",
            Dim::Dimensionless => "dimensionless",
        }
    }

    pub fn from_name(name: &str) -> Option<Dim> {
        Dim::ALL.into_iter().find(|d| d.as_str() == name)
    }

    /// `DIM_VEC`: mass, length, time, current, temperature, angle, data.
    pub fn vec(self) -> DimVec {
        match self {
            Dim::Mass => [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Length => [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Time => [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Current => [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            Dim::Temperature => [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            Dim::Angle => [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            Dim::Speed => [0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Acceleration => [0.0, 1.0, -2.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Force => [1.0, 1.0, -2.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Energy => [1.0, 2.0, -2.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Power => [1.0, 2.0, -3.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Pressure => [1.0, -1.0, -2.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Area => [0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Volume => [0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Frequency => [0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0],
            Dim::Charge => [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
            Dim::Voltage => [1.0, 2.0, -3.0, -1.0, 0.0, 0.0, 0.0],
            Dim::Resistance => [1.0, 2.0, -3.0, -2.0, 0.0, 0.0, 0.0],
            Dim::Capacitance => [-1.0, -2.0, 4.0, 2.0, 0.0, 0.0, 0.0],
            Dim::Inductance => [1.0, 2.0, -2.0, -2.0, 0.0, 0.0, 0.0],
            Dim::Dimensionless => [0.0; 7],
            Dim::Digital => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            Dim::Datarate => [0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// Exponents of mass, length, time, current, temperature, angle and data.
pub type DimVec = [f64; 7];

/// One unit (`Unit` in the TS).
#[derive(Clone, Debug)]
pub struct Unit {
    pub id: String,
    pub dim: Dim,
    pub symbol: String,
    pub to_base: f64,
    pub names: Vec<String>,
    /// The counterpart shown when there is no "to ..." target.
    pub default_to: Option<String>,
    /// Accepts SI prefixes (kilometer, ms, mega joules).
    pub prefixable: bool,
    /// Symbols whose b or B must match as typed: `Mb` is a megabit, `MB` a megabyte.
    pub cased: Vec<String>,
    /// Names that only match exactly as written: `Cal` is the food calorie, `cal` the small one.
    pub exact_names: Vec<String>,
}

pub type UnitRef = Arc<Unit>;

/// An SI prefix.
#[derive(Clone, Debug)]
pub struct Prefix {
    pub name: &'static str,
    pub names: &'static [&'static str],
    pub symbol: &'static str,
    pub factor: f64,
}

pub const PREFIXES: &[Prefix] = &[
    Prefix {
        name: "yocto",
        names: &["yocto"],
        symbol: "y",
        factor: 1e-24,
    },
    Prefix {
        name: "zepto",
        names: &["zepto"],
        symbol: "z",
        factor: 1e-21,
    },
    Prefix {
        name: "atto",
        names: &["atto"],
        symbol: "a",
        factor: 1e-18,
    },
    Prefix {
        name: "femto",
        names: &["femto"],
        symbol: "f",
        factor: 1e-15,
    },
    Prefix {
        name: "pico",
        names: &["pico"],
        symbol: "p",
        factor: 1e-12,
    },
    Prefix {
        name: "nano",
        names: &["nano"],
        symbol: "n",
        factor: 1e-9,
    },
    Prefix {
        name: "micro",
        names: &["micro"],
        symbol: "μ",
        factor: 1e-6,
    },
    Prefix {
        name: "milli",
        names: &["milli", "m"],
        symbol: "m",
        factor: 1e-3,
    },
    Prefix {
        name: "centi",
        names: &["centi", "c"],
        symbol: "c",
        factor: 1e-2,
    },
    Prefix {
        name: "deci",
        names: &["deci"],
        symbol: "d",
        factor: 1e-1,
    },
    Prefix {
        name: "deka",
        names: &["deka", "deca"],
        symbol: "da",
        factor: 1e1,
    },
    Prefix {
        name: "hecto",
        names: &["hecto"],
        symbol: "h",
        factor: 1e2,
    },
    Prefix {
        name: "kilo",
        names: &["kilo", "k"],
        symbol: "k",
        factor: 1e3,
    },
    Prefix {
        name: "mega",
        names: &["mega"],
        symbol: "M",
        factor: 1e6,
    },
    Prefix {
        name: "giga",
        names: &["giga"],
        symbol: "G",
        factor: 1e9,
    },
    Prefix {
        name: "tera",
        names: &["tera"],
        symbol: "T",
        factor: 1e12,
    },
    Prefix {
        name: "peta",
        names: &["peta"],
        symbol: "P",
        factor: 1e15,
    },
    Prefix {
        name: "exa",
        names: &["exa"],
        symbol: "E",
        factor: 1e18,
    },
    Prefix {
        name: "zetta",
        names: &["zetta"],
        symbol: "Z",
        factor: 1e21,
    },
    Prefix {
        name: "yotta",
        names: &["yotta"],
        symbol: "Y",
        factor: 1e24,
    },
];

pub fn prefix_by_name(name: &str) -> Option<&'static Prefix> {
    PREFIXES.iter().find(|p| p.name == name)
}

/// One entry of the alias index: a lowercased alias, its unit, and the case it must be typed in, if any.
#[derive(Clone, Debug)]
pub struct Alias {
    pub alias: String,
    pub chars: Vec<char>,
    pub unit: UnitRef,
    pub exact: Option<String>,
    pub whole: bool,
}

/// The whole table, built once.
#[derive(Debug)]
pub struct Table {
    pub list: Vec<UnitRef>,
    pub by_id: HashMap<String, UnitRef>,
    pub alias_index: Vec<Alias>,
    /// Alias positions by their first char, in index order (`startsWithToken` needs that char to match).
    pub by_first: HashMap<char, Vec<usize>>,
    /// Alias positions by their last char, in index order (`beforeTail`).
    pub by_last: HashMap<char, Vec<usize>>,
    pub alias_names: HashSet<String>,
    /// Units by symbol, for `caseClash`'s "another unit's exact symbol".
    pub by_symbol: HashMap<String, Vec<UnitRef>>,
    /// `PREFIX_INDEX`: every prefix name, longest first.
    pub prefix_index: Vec<(String, &'static Prefix)>,
}

pub static TABLE: LazyLock<Table> = LazyLock::new(build);

pub fn table() -> &'static Table {
    &TABLE
}

pub fn by_id(id: &str) -> Option<UnitRef> {
    TABLE.by_id.get(id).cloned()
}

/// A table unit by id that the TS reads with `!` (always present).
pub fn id(id: &str) -> UnitRef {
    match TABLE.by_id.get(id) {
        Some(u) => u.clone(),
        None => panic!("unit table has no {id}"),
    }
}

/// `sameScale`: within 1e-12 relative.
pub fn same_scale(a: f64, b: f64) -> bool {
    let scale = a.abs().max(b.abs()).max(1e-300);
    (a - b).abs() / scale < 1e-12
}

/// The JS length of a string (UTF-16 units).
pub fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn is_word_name(n: &str) -> bool {
    n.chars().count() >= 3 && n.chars().all(|c| c.is_ascii_alphabetic())
}

/// `wordNames`: names of three or more ASCII letters.
pub fn word_names(unit: &Unit) -> Vec<&str> {
    unit.names
        .iter()
        .map(String::as_str)
        .filter(|n| is_word_name(n))
        .collect()
}

/// `ms`, `kV`, `MN`. A squared or cubed base keeps the spelled label, since `km²` would mean 10⁶ m².
pub fn prefixed_label(prefix: &Prefix, base: &Unit) -> String {
    if !base.symbol.contains(['²', '³']) {
        return format!("{}{}", prefix.symbol, base.symbol);
    }
    let word = word_names(base).first().map(|w| w.to_string());
    format!(
        "{}{}",
        prefix.name,
        word.unwrap_or_else(|| base.symbol.clone())
    )
}

/// `prefixedUnit`: a prefixed copy of `base`, with no names of its own.
pub fn prefixed_unit(prefix: &Prefix, base: &Unit, id: Option<&str>) -> Unit {
    Unit {
        id: id.map_or_else(|| format!("{}_{}", prefix.name, base.id), str::to_string),
        dim: base.dim,
        symbol: prefixed_label(prefix, base),
        to_base: base.to_base * prefix.factor,
        default_to: Some(base.id.clone()),
        names: Vec::new(),
        prefixable: false,
        cased: Vec::new(),
        exact_names: Vec::new(),
    }
}

/// `normalizeName`: the spelling the alias index also files a name under.
pub fn normalize_name(s: &str) -> String {
    static DEG_C: Lazy = Lazy::new(|| jsre::js(r"°\s*c\b", ""));
    static DEG_F: Lazy = Lazy::new(|| jsre::js(r"°\s*f\b", ""));
    static DEG_R: Lazy = Lazy::new(|| jsre::js(r"°\s*r\b", ""));
    static SLASH: Lazy = Lazy::new(|| jsre::js(r"\s*\/\s*", ""));
    static PUNCT: Lazy = Lazy::new(|| jsre::js(r"[_.,]", ""));
    static SPACES: Lazy = Lazy::new(|| jsre::js(r"\s+", ""));
    let t = s.trim().to_lowercase();
    let t = jsre::replace_all(&DEG_C, &t, |_| "c".into()).into_owned();
    let t = jsre::replace_all(&DEG_F, &t, |_| "f".into()).into_owned();
    let t = jsre::replace_all(&DEG_R, &t, |_| "r".into()).into_owned();
    let t = t
        .replace('°', "deg")
        .replace(['µ', 'μ'], "u")
        .replace('²', "2")
        .replace('³', "3")
        .replace('^', "");
    let t = jsre::replace_all(&SLASH, &t, |_| "/".into()).into_owned();
    let t = jsre::replace_all(&PUNCT, &t, |_| " ".into()).into_owned();
    let t = jsre::replace_all(&SPACES, &t, |_| " ".into()).into_owned();
    t.trim().to_string()
}

const LB: f64 = 0.45359237;
const FT: f64 = 0.3048;
const MI: f64 = 1609.344;
const US_GAL: f64 = 3.785411784;
const US_FLOZ: f64 = 0.0295735295625;
const ACRE: f64 = 4046.8564224;
const IN3: f64 = 0.016387064;
const PSI: f64 = 6894.757293168361;
const LBF: f64 = 4.4482216152605;
pub const G0: f64 = 9.80665;
const C: f64 = 299792458.0;
const E_CHARGE: f64 = 1.602176634e-19;
const AMU: f64 = 1.6605390666e-27;
const AU: f64 = 149597870700.0;
const YEAR: f64 = 365.25 * 86400.0;

struct Spec {
    id: &'static str,
    dim: Dim,
    symbol: &'static str,
    to_base: f64,
    default_to: Option<&'static str>,
    prefixable: bool,
    names: &'static [&'static str],
    exact: &'static [&'static str],
}

fn spec(
    id: &'static str,
    dim: Dim,
    symbol: &'static str,
    to_base: f64,
    default_to: &'static str,
    names: &'static [&'static str],
) -> Spec {
    Spec {
        id,
        dim,
        symbol,
        to_base,
        default_to: (!default_to.is_empty()).then_some(default_to),
        prefixable: false,
        names,
        exact: &[],
    }
}

fn pre(mut s: Spec) -> Spec {
    s.prefixable = true;
    s
}

fn exact(mut s: Spec, exact: &'static [&'static str]) -> Spec {
    s.exact = exact;
    s
}

fn unit_of(s: Spec) -> Unit {
    Unit {
        id: s.id.into(),
        dim: s.dim,
        symbol: s.symbol.into(),
        to_base: s.to_base,
        names: s.names.iter().map(|n| n.to_string()).collect(),
        default_to: s.default_to.map(str::to_string),
        prefixable: s.prefixable,
        cased: Vec::new(),
        exact_names: s.exact.iter().map(|n| n.to_string()).collect(),
    }
}

const DATA_PREFIXES: [(&str, &str, f64); 9] = [
    ("", "", 1.0),
    ("k", "kilo", 1e3),
    ("M", "mega", 1e6),
    ("G", "giga", 1e9),
    ("T", "tera", 1e12),
    ("P", "peta", 1e15),
    ("E", "exa", 1e18),
    ("Z", "zetta", 1e21),
    ("Y", "yotta", 1e24),
];

const BINARY_PREFIXES: [(&str, &str); 6] = [
    ("Ki", "kibi"),
    ("Mi", "mebi"),
    ("Gi", "gibi"),
    ("Ti", "tebi"),
    ("Pi", "pebi"),
    ("Ei", "exbi"),
];

fn data(
    id: String,
    dim: Dim,
    symbol: String,
    to_base: f64,
    names: Vec<String>,
    cased: Vec<String>,
) -> Unit {
    Unit {
        id,
        dim,
        symbol,
        to_base,
        names,
        default_to: None,
        prefixable: false,
        cased,
        exact_names: Vec::new(),
    }
}

// bytes are the base; the byte ids (kb, mb, gb, tb) predate the bit units and stay as they were
fn data_units() -> Vec<Unit> {
    let mut out = Vec::new();
    for (p, word, f) in DATA_PREFIXES {
        let id = p.to_lowercase();
        let byte_id = if id.is_empty() {
            "byte".to_string()
        } else {
            format!("{id}b")
        };
        out.push(data(
            byte_id,
            Dim::Digital,
            format!("{p}B"),
            f,
            vec![format!("{word}byte"), format!("{word}bytes")],
            vec![format!("{p}B")],
        ));
        // a lone b stays a letter, since `a + b = 10` is an equation, not ares plus bits
        out.push(data(
            format!("{id}bit"),
            Dim::Digital,
            if p.is_empty() {
                "bit".into()
            } else {
                format!("{p}b")
            },
            f / 8.0,
            vec![format!("{word}bit"), format!("{word}bits")],
            if p.is_empty() {
                vec![]
            } else {
                vec![format!("{p}b")]
            },
        ));
        out.push(data(
            format!("{id}bps"),
            Dim::Datarate,
            format!("{p}bps"),
            f / 8.0,
            vec![
                format!("{p}bps"),
                format!("{p}bit/s"),
                format!("{word}bits per second"),
            ],
            vec![format!("{p}b/s")],
        ));
        out.push(data(
            format!("{id}byteps"),
            Dim::Datarate,
            format!("{p}B/s"),
            f,
            vec![format!("{word}bytes per second")],
            vec![format!("{p}B/s")],
        ));
    }
    for (i, (p, word)) in BINARY_PREFIXES.iter().enumerate() {
        let id = p.to_lowercase();
        out.push(data(
            format!("{id}b"),
            Dim::Digital,
            format!("{p}B"),
            crate::scientific::functions::jsmath::pow(1024.0, (i + 1) as f64),
            vec![
                format!("{id}b"),
                format!("{word}byte"),
                format!("{word}bytes"),
            ],
            vec![],
        ));
    }
    out
}

#[rustfmt::skip]
fn unit_list() -> Vec<Unit> {
    use Dim::*;
    let slug = (G0 * LB) / FT;
    let slinch = slug * 12.0;
    let light_year = C * YEAR;
    let parsec = (AU * 648000.0) / PI;
    let therm = 1e5 * 1055.05585262;
    let us_bushel = 35.23907016688;
    let oil_bbl = 42.0 * US_GAL;
    let cord = 128.0 * 28.316846592;
    let darcy = 9.869232667160128e-13;
    let head: Vec<Spec> = vec![
        pre(spec("in", Length, "in", 0.0254, "mm", &["in", "inch", "inches", "\""])),
        spec("ft", Length, "ft", FT, "m", &["ft", "foot", "feet", "'"]),
        spec("yd", Length, "yd", 0.9144, "m", &["yd", "yds", "yard", "yards"]),
        spec("mi", Length, "mi", MI, "km", &["mi", "mile", "miles"]),
        spec("nmi", Length, "nmi", 1852.0, "km", &["nmi", "nmile", "nauticalmile", "nauticalmiles", "nautical mile", "nautical miles"]),
        spec("mil", Length, "mil", 2.54e-5, "um", &["mil", "mils", "thou"]),
        spec("fathom", Length, "fathom", 6.0 * FT, "m", &["fathom", "fathoms"]),
        spec("rod", Length, "rod", 16.5 * FT, "m", &["rod", "rods", "perch", "pole"]),
        spec("chain", Length, "chain", 66.0 * FT, "m", &["chain", "chains"]),
        spec("furlong", Length, "furlong", 660.0 * FT, "m", &["furlong", "furlongs"]),
        spec("league", Length, "league", 3.0 * MI, "km", &["league", "leagues"]),
        spec("fermi", Length, "fm", 1e-15, "m", &["fermi", "fermis", "femtometer", "femtometers", "femtometre", "femtometres"]),
        spec("angstrom", Length, "Å", 1e-10, "nm", &["angstrom", "angstroms", "ångström", "ångstrom", "Å"]),
        spec("au", Length, "au", AU, "km", &["au", "astronomicalunit", "astronomicalunits", "astronomical unit", "astronomical units"]),
        spec("ly", Length, "ly", light_year, "km", &["ly", "lightyear", "lightyears", "light year", "light years"]),
        pre(spec("pc", Length, "pc", parsec, "ly", &["pc", "parsec", "parsecs"])),
        spec("mm", Length, "mm", 0.001, "in", &["mm", "millimeter", "millimeters", "millimetre", "millimetres"]),
        spec("cm", Length, "cm", 0.01, "in", &["cm", "centimeter", "centimeters", "centimetre", "centimetres"]),
        spec("dm", Length, "dm", 0.1, "in", &["dm", "decimeter", "decimeters", "decimetre", "decimetres"]),
        pre(spec("m", Length, "m", 1.0, "ft", &["m", "meter", "meters", "metre", "metres"])),
        spec("km", Length, "km", 1000.0, "mi", &["km", "kilometer", "kilometers", "kilometre", "kilometres"]),
        spec("um", Length, "μm", 1e-6, "mil", &["um", "μm", "micron", "microns", "micrometer", "micrometers", "micrometre", "micrometres"]),
        spec("nm", Length, "nm", 1e-9, "in", &["nm", "nanometer", "nanometers", "nanometre", "nanometres"]),

        spec("oz", Mass, "oz", 28.349523125 / 1000.0, "g", &["oz", "ozm", "ounce", "ounces", "ouncemass", "ounce mass"]),
        spec("lb", Mass, "lbs", LB, "kg", &["lb", "lbs", "lbm", "pound", "pounds", "poundmass", "pound mass"]),
        spec("st", Mass, "st", 6.35029318, "kg", &["st", "stone", "stones"]),
        spec("ton", Mass, "ton", 2000.0 * LB, "tonne", &["ton", "tons", "shortton", "shorttons", "short ton", "short tons", "us short ton", "us short tons"]),
        spec("longton", Mass, "long ton", 2240.0 * LB, "tonne", &["longton", "longtons", "long ton", "long tons", "imperial ton", "imperial tons", "uk ton", "uk tons"]),
        spec("grain", Mass, "gr", 64.79891e-6, "mg", &["gr", "grain", "grains"]),
        spec("slug", Mass, "slug", slug, "kg", &["slug", "slugs"]),
        spec("slinch", Mass, "slinch", slinch, "kg", &["slinch", "snail", "snails"]),
        spec("amu", Mass, "u", AMU, "kg", &["u", "amu", "atomicmassunit", "atomicmassunits", "atomic mass unit", "atomic mass units"]),
        spec("emass", Mass, "mₑ", 9.1093837015e-31, "kg", &["electronrestmass", "electron rest mass", "mₑ"]),
        spec("pmass", Mass, "mₚ", 1.007276466621 * AMU, "kg", &["protonrestmass", "proton rest mass", "mₚ"]),
        spec("nmass", Mass, "mₙ", 1.00866491588 * AMU, "kg", &["neutronrestmass", "neutron rest mass", "mₙ"]),
        spec("mg", Mass, "mg", 1e-6, "grain", &["mg", "milligram", "milligrams"]),
        pre(spec("g", Mass, "g", 0.001, "oz", &["g", "gram", "grams", "gramme", "grammes"])),
        spec("kg", Mass, "kg", 1.0, "lb", &["kg", "kilogram", "kilograms"]),
        pre(spec("tonne", Mass, "t", 1000.0, "ton", &["t", "tonne", "tonnes", "metricton", "metrictons", "metric ton", "metric tons"])),

        // °C and °F are offset scales: conversions only, never arithmetic
        spec("c", Temperature, "°C", 0.0, "f", &["c", "celsius", "centigrade", "degc", "deg c", "degree celsius", "degrees celsius", "°c"]),
        spec("f", Temperature, "°F", 0.0, "c", &["f", "fahrenheit", "degf", "deg f", "degree fahrenheit", "degrees fahrenheit", "°f"]),
        spec("k", Temperature, "K", 1.0, "r", &["k", "kelvin", "kelvins"]),
        spec("r", Temperature, "°R", 5.0 / 9.0, "k", &["r", "rankine", "degr", "deg r", "degree rankine", "degrees rankine", "°r"]),

        spec("drop", Volume, "drop", 5e-5, "ml", &["drop", "drops"]),
        spec("tsp", Volume, "tsp", US_FLOZ / 6.0, "ml", &["tsp", "teaspoon", "teaspoons"]),
        spec("tbsp", Volume, "tbsp", US_FLOZ / 2.0, "ml", &["tbs", "tbsp", "tablespoon", "tablespoons"]),
        spec("floz", Volume, "fl oz", US_FLOZ, "ml", &["floz", "flozs", "fl oz", "fluidounce", "fluidounces", "fluid ounce", "fluid ounces", "us fluid ounce", "us fluid ounces"]),
        spec("cup", Volume, "cup", 0.2365882365, "ml", &["cup", "cups", "us cup", "us cups"]),
        spec("pt", Volume, "pt", 0.473176473, "l", &["pt", "pint", "pints", "us pint", "us pints"]),
        spec("qt", Volume, "qt", 0.946352946, "l", &["qt", "quart", "quarts", "us quart", "us quarts"]),
        spec("usgal", Volume, "gal", US_GAL, "l", &["gal", "gallon", "gallons", "us gallon", "us gallons"]),
        spec("impgal", Volume, "imp gal", 4.54609, "l", &["imp gal", "imperial gallon", "imperial gallons", "uk gallon", "uk gallons"]),
        spec("peck", Volume, "peck", us_bushel / 4.0, "l", &["peck", "pecks"]),
        spec("bushel", Volume, "bu", us_bushel, "l", &["bu", "bushel", "bushels"]),
        spec("barrel", Volume, "bbl", oil_bbl, "l", &["bbl", "barrel", "barrels", "oil barrel", "oil barrels"]),
        spec("cord", Volume, "cord", cord, "m3", &["cord", "cords"]),
        spec("in3", Volume, "in³", IN3, "cm3", &["in3", "in^3", "cu in", "cuin", "cubic inch", "cubic inches"]),
        spec("ft3", Volume, "ft³", 28.316846592, "m3", &["ft3", "ft^3", "cu ft", "cuft", "cubic foot", "cubic feet"]),
        spec("yd3", Volume, "yd³", 764.554857984, "m3", &["yd3", "yd^3", "cu yd", "cubic yard", "cubic yards"]),
        spec("ml", Volume, "mL", 0.001, "floz", &["ml", "milliliter", "milliliters", "millilitre", "millilitres", "cc", "ccs"]),
        spec("cm3", Volume, "cm³", 0.001, "in3", &["cm3", "cm^3", "cubic centimeter", "cubic centimeters", "cubic centimetre", "cubic centimetres"]),
        pre(spec("l", Volume, "L", 1.0, "usgal", &["l", "liter", "liters", "litre", "litres"])),
        spec("stere", Volume, "stere", 1000.0, "ft3", &["stere", "steres"]),
        spec("m3", Volume, "m³", 1000.0, "ft3", &["m3", "m^3", "cubic meter", "cubic meters", "cubic metre", "cubic metres"]),

        pre(spec("barn", Area, "barn", 1e-28, "m2", &["barn", "barns"])),
        spec("darcy", Area, "darcy", darcy, "m2", &["darcy", "darcys", "darcies"]),
        spec("in2", Area, "in²", 0.00064516, "cm2", &["in2", "in^2", "sqin", "sq in", "square inch", "square inches"]),
        spec("sqft", Area, "ft²", 0.09290304, "m2", &["sqft", "sq ft", "ft2", "ft^2", "square foot", "square feet"]),
        spec("yd2", Area, "yd²", 0.83612736, "m2", &["yd2", "yd^2", "sq yd", "square yard", "square yards"]),
        spec("acre", Area, "acres", ACRE, "ha", &["acre", "acres"]),
        spec("sqmi", Area, "mi²", MI * MI, "km2", &["sqmi", "sq mi", "mi2", "mi^2", "square mile", "square miles"]),
        spec("cm2", Area, "cm²", 0.0001, "in2", &["cm2", "cm^2", "sq cm", "square centimeter", "square centimeters", "square centimetre", "square centimetres"]),
        pre(spec("m2", Area, "m²", 1.0, "sqft", &["m2", "m^2", "sqm", "sq m", "square meter", "square meters", "square metre", "square metres"])),
        spec("km2", Area, "km²", 1e6, "sqmi", &["km2", "km^2", "sq km", "square kilometer", "square kilometers", "square kilometre", "square kilometres"]),
        spec("ha", Area, "ha", 10000.0, "acre", &["ha", "hectare", "hectares"]),

        spec("mph", Speed, "mph", MI / 3600.0, "kmh", &["mph", "mi/h", "mile/h", "mile per hour", "miles per hour"]),
        spec("fps", Speed, "ft/s", FT, "mps", &["fps", "ft/s", "foot per second", "feet per second"]),
        spec("knot", Speed, "kt", 1852.0 / 3600.0, "kmh", &["kt", "knot", "knots"]),
        spec("kmh", Speed, "km/h", 1000.0 / 3600.0, "mph", &["km/h", "km/hr", "kph", "kmh", "kilometer per hour", "kilometers per hour", "kilometre per hour", "kilometres per hour"]),
        spec("mps", Speed, "m/s", 1.0, "fps", &["m/s", "meter per second", "meters per second", "metre per second", "metres per second"]),
        spec("light", Speed, "c", C, "mps", &["lightspeed", "speedoflight", "speed of light"]),

        // time and digital have no defaultTo: they convert only with an explicit "to"
        spec("hr", Time, "hr", 3600.0, "", &["h", "hr", "hrs", "hour", "hours"]),
        pre(spec("s", Time, "s", 1.0, "", &["s", "sec", "secs", "second", "seconds"])),
        spec("day", Time, "d", 86400.0, "", &["d", "day", "days"]),
        spec("min", Time, "min", 60.0, "", &["min", "mins", "minute", "minutes"]),
        spec("wk", Time, "wk", 604800.0, "", &["wk", "wks", "week", "weeks"]),
        spec("yr", Time, "yr", YEAR, "", &["yr", "yrs", "year", "years"]),
        spec("month", Time, "mo", YEAR / 12.0, "", &["mo", "mos", "month", "months"]),
        spec("decade", Time, "decades", 10.0 * YEAR, "", &["decade", "decades"]),
        spec("century", Time, "centuries", 100.0 * YEAR, "", &["century", "centuries"]),
        spec("millennium", Time, "kyr", 1000.0 * YEAR, "", &["millenium", "millennium", "millenniums", "millennia", "kyr"]),
    ];
    let tail: Vec<Spec> = vec![
        pre(spec("ev", Energy, "eV", E_CHARGE, "j", &["ev", "evs", "electronvolt", "electronvolts", "electron volt", "electron volts"])),
        spec("ftlbf", Energy, "ft·lbf", FT * LBF, "j", &["ftlbf", "footpound", "footpounds", "foot pound", "foot pounds"]),
        // newton-metres as engineers write them; cased, so `nm` stays a nanometre. an id with _ is never picked for an answer
        exact(spec("nm_torque", Energy, "N·m", 1.0, "ftlbf", &["newton meter", "newton meters", "newton metre", "newton metres", "newton-meter", "newton-meters", "newton-metre", "newton-metres"]), &["Nm"]),
        exact(spec("knm_torque", Energy, "kN·m", 1e3, "ftlbf", &["kilonewton meter", "kilonewton meters", "kilonewton metre", "kilonewton metres"]), &["kNm"]),
        exact(spec("mnm_torque", Energy, "MN·m", 1e6, "ftlbf", &["meganewton meter", "meganewton meters", "meganewton metre", "meganewton metres"]), &["MNm"]),
        exact(spec("nmm_torque", Energy, "N·mm", 1e-3, "ftlbf", &["newton millimeter", "newton millimeters", "newton millimetre", "newton millimetres"]), &["Nmm"]),
        spec("erg", Energy, "erg", 1e-7, "j", &["erg", "ergs"]),
        spec("cal", Energy, "cal", 4.184, "j", &["cal", "calorie", "calories", "thermodynamic calorie", "thermodynamic calories"]),
        exact(spec("kcal", Energy, "kcal", 4184.0, "kj", &["kcal", "kilocalorie", "kilocalories", "food calorie", "food calories"]), &["Cal"]),
        spec("btu", Energy, "BTU", 1055.05585262, "kj", &["btu", "btus", "british thermal unit", "british thermal units"]),
        spec("therm", Energy, "thm", therm, "btu", &["thm", "therm", "therms"]),
        pre(spec("j", Energy, "J", 1.0, "cal", &["j", "joule", "joules"])),
        spec("kj", Energy, "kJ", 1000.0, "btu", &["kj", "kilojoule", "kilojoules"]),
        spec("wh", Energy, "Wh", 3600.0, "kj", &["wh", "watt hour", "watt hours"]),
        spec("kwh", Energy, "kWh", 3.6e6, "btu", &["kwh", "kw h", "kilowatt hour", "kilowatt hours"]),

        spec("hp", Power, "hp", 745.699872, "kw", &["hp", "horsepower", "mechanical horsepower"]),
        pre(spec("w", Power, "W", 1.0, "hp", &["w", "watt", "watts"])),
        spec("kw", Power, "kW", 1000.0, "hp", &["kw", "kilowatt", "kilowatts"]),

        spec("psi", Pressure, "psi", PSI, "kpa", &["psi", "pound per square inch", "pounds per square inch"]),
        spec("ksi", Pressure, "ksi", PSI * 1000.0, "mpa", &["ksi"]),
        spec("inhg", Pressure, "inHg", 3386.389, "kpa", &["inhg", "in hg", "inch of mercury", "inches of mercury"]),
        pre(spec("pa", Pressure, "Pa", 1.0, "psi", &["pa", "pascal", "pascals"])),
        spec("kpa", Pressure, "kPa", 1000.0, "psi", &["kpa", "kilopascal", "kilopascals"]),
        spec("mpa", Pressure, "MPa", 1e6, "ksi", &["mpa", "megapascal", "megapascals"]),
        spec("bar", Pressure, "bar", 1e5, "psi", &["bar", "bars"]),
        spec("atm", Pressure, "atm", 101325.0, "psi", &["atm", "atmosphere", "atmospheres"]),
        spec("torr", Pressure, "torr", 101325.0 / 760.0, "psi", &["torr", "torrs", "mmhg", "mm hg", "millimeter of mercury", "millimeters of mercury"]),

        spec("dyn", Force, "dyn", 1e-5, "n", &["dyn", "dyne", "dynes"]),
        spec("ozf", Force, "ozf", LBF / 16.0, "n", &["ozf", "ounceforce", "ounce-force", "ounce force"]),
        spec("lbf", Force, "lbf", LBF, "n", &["lbf", "poundforce", "pound-force", "pounds-force", "pound force"]),
        spec("kip", Force, "kip", 1000.0 * LBF, "n", &["kip", "kips"]),
        pre(spec("n", Force, "N", 1.0, "lbf", &["n", "newton", "newtons"])),
        spec("kilonewton", Force, "kN", 1000.0, "lbf", &["kn", "kilonewton", "kilonewtons"]),
        spec("kgf", Force, "kgf", G0, "n", &["kgf", "kgforce", "kilogramforce", "kilogram-force", "kilogram force"]),

        spec("arcsec", Angle, "″", PI / (180.0 * 3600.0), "deg", &["arcsec", "arcsecs", "arcsecond", "arcseconds", "arc sec", "arc secs", "arc second", "arc seconds"]),
        spec("arcmin", Angle, "′", PI / (180.0 * 60.0), "deg", &["arcmin", "arcmins", "arcminute", "arcminutes", "arc min", "arc mins", "arc minute", "arc minutes"]),
        spec("deg", Angle, "deg", PI / 180.0, "rad", &["deg", "degs", "degree", "degrees"]),
        pre(spec("rad", Angle, "rad", 1.0, "deg", &["rad", "rads", "radian", "radians"])),
        spec("rev", Angle, "rev", 2.0 * PI, "deg", &["rev", "revs", "revolution", "revolutions"]),

        pre(spec("hz", Frequency, "Hz", 1.0, "rpm", &["hz", "hertz"])),
        spec("rpm", Frequency, "rpm", 1.0 / 60.0, "hz", &["rpm", "rpms", "revolution per minute", "revolutions per minute"]),

        spec("mps2", Acceleration, "m/s²", 1.0, "gee", &["m/s2", "m/s^2", "m/s²", "meter per second squared", "meters per second squared", "metre per second squared", "metres per second squared"]),
        spec("gee", Acceleration, "g", G0, "mps2", &["gs", "gee", "gravity"]),

        spec("electron", Charge, "e", E_CHARGE, "coulomb", &["electron", "electrons"]),
        spec("photon", Dimensionless, "photon", 1.0, "", &["photon", "photons"]),
        pre(spec("coulomb", Charge, "C", 1.0, "electron", &["coulomb", "coulombs"])),

        pre(spec("amp", Current, "A", 1.0, "", &["a", "amp", "amps", "ampere", "amperes"])),
        pre(spec("volt", Voltage, "V", 1.0, "", &["v", "volt", "volts"])),
        pre(spec("ohm", Resistance, "Ω", 1.0, "", &["ohm", "ohms", "Ω"])),
        pre(spec("farad", Capacitance, "F", 1.0, "", &["farad", "farads"])),
        pre(spec("henry", Inductance, "H", 1.0, "", &["henry", "henrys", "henries"])),

        // "1" is left out on purpose; it would steal trailing digits
        spec("none", Dimensionless, "dimensionless", 1.0, "", &["dimensionless", "none", "nounit", "nounits"]),
        spec("unit", Dimensionless, "units", 1.0, "", &["unit", "units"]),
    ];
    let mut out: Vec<Unit> = head.into_iter().map(unit_of).collect();
    out.extend(data_units());
    out.extend(tail.into_iter().map(unit_of));
    for p in PREFIXES {
        let names: Vec<String> = p
            .names
            .iter()
            .filter(|n| n.encode_utf16().count() > 1)
            .map(|n| n.to_string())
            .collect();
        if names.is_empty() {
            continue;
        }
        out.push(Unit {
            id: format!("prefix_{}", p.name),
            dim: Dimensionless,
            symbol: p.name.into(),
            to_base: p.factor,
            names,
            default_to: None,
            prefixable: false,
            cased: Vec::new(),
            exact_names: Vec::new(),
        });
    }
    out
}

fn taken_aliases(units: &[Unit]) -> HashSet<String> {
    let mut taken = HashSet::new();
    for unit in units {
        for name in &unit.cased {
            taken.insert(name.to_lowercase());
        }
        for name in &unit.names {
            taken.insert(name.to_lowercase());
            let norm = normalize_name(name);
            if !norm.is_empty() {
                taken.insert(norm);
            }
        }
    }
    taken
}

fn apply_si_prefixes(units: &mut Vec<Unit>) {
    let mut taken = taken_aliases(units);
    let roots: Vec<usize> = (0..units.len()).filter(|&i| units[i].prefixable).collect();
    let mut families: HashMap<Dim, Vec<usize>> = HashMap::new();
    for (i, u) in units.iter().enumerate() {
        families.entry(u.dim).or_default().push(i);
    }
    for base_i in roots {
        let dim = units[base_i].dim;
        let words: Vec<String> = word_names(&units[base_i])
            .into_iter()
            .map(str::to_string)
            .collect();
        for prefix in PREFIXES {
            let to_base = prefix.factor * units[base_i].to_base;
            if !to_base.is_finite() || to_base == 0.0 {
                continue;
            }
            let mut fresh: Vec<String> = Vec::new();
            for word in &words {
                for pname in prefix.names {
                    if pname.encode_utf16().count() == 1 {
                        continue;
                    }
                    let a = format!("{pname}{word}").to_lowercase();
                    if !fresh.contains(&a) {
                        fresh.push(a);
                    }
                }
            }
            fresh.retain(|a| !a.is_empty() && !taken.contains(a));
            if fresh.is_empty() {
                continue;
            }
            let family = families.get(&dim).cloned().unwrap_or_default();
            if let Some(&existing) = family
                .iter()
                .find(|&&i| same_scale(units[i].to_base, to_base))
            {
                units[existing].names.extend(fresh.iter().cloned());
                taken.extend(fresh);
                continue;
            }
            let mut unit = prefixed_unit(prefix, &units[base_i], None);
            unit.names = fresh.clone();
            units.push(unit);
            families.entry(dim).or_default().push(units.len() - 1);
            taken.extend(fresh);
        }
    }
}

/// The finished `UNIT_LIST`, in the TS order.
fn finished_list() -> Vec<Unit> {
    let mut units = unit_list();
    apply_si_prefixes(&mut units);
    if let Some(megaohm) = units
        .iter_mut()
        .find(|u| u.dim == Dim::Resistance && same_scale(u.to_base, 1e6))
    {
        megaohm.names.extend(["megohm".into(), "megohms".into()]);
    }
    if let Some(microfarad) = units
        .iter_mut()
        .find(|u| u.dim == Dim::Capacitance && same_scale(u.to_base, 1e-6))
    {
        microfarad.names.push("uf".into());
    }
    // squared and cubed millimeters sit outside the prefix pass, so µL keeps its own name
    units.push(unit_of(spec(
        "mm2",
        Dim::Area,
        "mm²",
        1e-6,
        "in2",
        &[
            "mm2",
            "mm^2",
            "sq mm",
            "square millimeter",
            "square millimeters",
            "square millimetre",
            "square millimetres",
        ],
    )));
    units.push(unit_of(spec(
        "mm3",
        Dim::Volume,
        "mm³",
        1e-6,
        "in3",
        &[
            "mm3",
            "mm^3",
            "cu mm",
            "cubic millimeter",
            "cubic millimeters",
            "cubic millimetre",
            "cubic millimetres",
        ],
    )));
    units
}

fn build() -> Table {
    let list: Vec<UnitRef> = finished_list().into_iter().map(Arc::new).collect();
    let mut by_id = HashMap::new();
    for u in &list {
        by_id.insert(u.id.clone(), u.clone());
    }
    let mut alias_index: Vec<Alias> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let entry = |alias: String, unit: &UnitRef, exact: Option<String>, whole: bool| Alias {
        chars: alias.chars().collect(),
        alias,
        unit: unit.clone(),
        exact,
        whole,
    };
    for unit in &list {
        for e in &unit.cased {
            alias_index.push(entry(e.to_lowercase(), unit, Some(e.clone()), false));
        }
        for e in &unit.exact_names {
            alias_index.push(entry(e.to_lowercase(), unit, Some(e.clone()), true));
        }
        for name in &unit.names {
            for alias in [name.to_lowercase(), normalize_name(name)] {
                if alias.is_empty() {
                    continue;
                }
                if !seen.insert((alias.clone(), unit.id.clone())) {
                    continue;
                }
                alias_index.push(entry(alias, unit, None, false));
            }
        }
    }
    // longest first, and a case-sensitive name before a plain one of the same length (a stable sort, as V8's)
    alias_index.sort_by(|a, b| {
        js_len(&b.alias)
            .cmp(&js_len(&a.alias))
            .then(b.exact.is_some().cmp(&a.exact.is_some()))
    });
    let mut by_first: HashMap<char, Vec<usize>> = HashMap::new();
    let mut by_last: HashMap<char, Vec<usize>> = HashMap::new();
    for (i, a) in alias_index.iter().enumerate() {
        if let (Some(&f), Some(&l)) = (a.chars.first(), a.chars.last()) {
            by_first.entry(f).or_default().push(i);
            by_last.entry(l).or_default().push(i);
        }
    }
    let alias_names = alias_index.iter().map(|a| a.alias.clone()).collect();
    let mut by_symbol: HashMap<String, Vec<UnitRef>> = HashMap::new();
    for u in &list {
        by_symbol
            .entry(u.symbol.clone())
            .or_default()
            .push(u.clone());
    }
    let mut prefix_index: Vec<(String, &'static Prefix)> = PREFIXES
        .iter()
        .flat_map(|p| p.names.iter().map(move |n| (n.to_lowercase(), p)))
        .collect();
    prefix_index.sort_by_key(|p| std::cmp::Reverse(js_len(&p.0)));
    Table {
        list,
        by_id,
        alias_index,
        by_first,
        by_last,
        alias_names,
        by_symbol,
        prefix_index,
    }
}

/// `CASE_PREFIXES`: case-sensitive one-letter prefixes (`MN`, `mN`, `us` for µs after preprocess).
pub fn case_prefixes() -> &'static [(char, &'static Prefix)] {
    static LIST: LazyLock<Vec<(char, &'static Prefix)>> = LazyLock::new(|| {
        [
            ('Y', "yotta"),
            ('Z', "zetta"),
            ('E', "exa"),
            ('P', "peta"),
            ('T', "tera"),
            ('G', "giga"),
            ('M', "mega"),
            ('k', "kilo"),
            ('h', "hecto"),
            ('d', "deci"),
            ('c', "centi"),
            ('m', "milli"),
            ('u', "micro"),
            ('n', "nano"),
            ('p', "pico"),
            ('f', "femto"),
            ('a', "atto"),
            ('z', "zepto"),
            ('y', "yocto"),
        ]
        .into_iter()
        .filter_map(|(c, n)| prefix_by_name(n).map(|p| (c, p)))
        .collect()
    });
    &LIST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_builds_with_its_generated_units() {
        let t = table();
        assert!(t.by_id.contains_key("mega_n"));
        assert!(t.by_id.contains_key("kilo_in"));
        assert_eq!(id("kilo_in").symbol, "kin");
        assert!(id("km").names.iter().any(|n| n == "kilometer"));
        assert!(t.alias_names.contains("megohm"));
        assert_eq!(normalize_name("°C"), "c");
        assert_eq!(normalize_name("m / s²"), "m/s2");
    }
}
