//! Named constants typed in words: `Earth's axial tilt` is 23.45°, `mass of the moon` is 7.34 × 10²² kg,
//! `gravity on Mars` is 3.7 m/s². The values come from the appendix of constants in Mitiguy's *Advanced Dynamics
//! & Motion Simulation* (p. 306), which the user asked Q Calc to know.
//!
//! A line that is only a constant (`Earth's axial tilt`, `what is the mass of the earth?`) answers in the
//! constant's own unit. A constant inside a longer line reads as its quantity in parentheses, so
//! `mass of earth / mass of moon`, `2 * earth moon distance` and `earth's axial tilt to rad` all work. Matching
//! ignores case, apostrophes (`earths`) and any `the`, and a word the user defined as a variable stays theirs.
//!
//! Native only.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

struct Constant {
    /// Each name as its words, lowercase with apostrophes taken out (`earths axial tilt`).
    names: Vec<Vec<String>>,
    /// The quantity, as the unit reader reads it (`23.45 deg`).
    expr: &'static str,
    /// The unit a line of just this constant answers in.
    unit: &'static str,
}

/// Words a name may have or leave out: `mass of the earth`, `speed of a cheetah`.
const FILLERS: [&str; 3] = ["the", "a", "an"];

fn words(name: &str) -> Vec<String> {
    name.split([' ', '-'])
        .filter(|w| !w.is_empty() && !FILLERS.contains(w))
        .map(norm)
        .collect()
}

fn norm(word: &str) -> String {
    word.chars()
        .filter(|c| *c != '\'' && *c != '’')
        .flat_map(char::to_lowercase)
        .collect()
}

/// A property of a body in all the ways it's said: `earth's mass`, `earth mass`, `mass of earth`, `mass on earth`.
fn of(subjects: &[&str], props: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for s in subjects {
        for p in props {
            out.push(format!("{s}'s {p}"));
            out.push(format!("{s} {p}"));
            out.push(format!("{p} of {s}"));
            out.push(format!("{p} on {s}"));
        }
    }
    out
}

fn constant(names: Vec<String>, expr: &'static str, unit: &'static str) -> Constant {
    Constant {
        names: names.iter().map(|n| words(n)).collect(),
        expr,
        unit,
    }
}

fn list(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn with(mut a: Vec<String>, b: Vec<String>) -> Vec<String> {
    a.extend(b);
    a
}

static CONSTANTS: LazyLock<Vec<Constant>> = LazyLock::new(|| {
    const EARTH: &[&str] = &["earth"];
    let mut out = vec![
        constant(
            list(&["speed of light", "speed of light in vacuum"]),
            "299792458 m/s",
            "m/s",
        ),
        constant(
            list(&[
                "universal gravitational constant",
                "gravitational constant",
                "newtonian constant of gravitation",
                "newton's gravitational constant",
                "constant of gravitation",
            ]),
            "6.67e-11 N m^2/kg^2",
            "N m^2/kg^2",
        ),
        constant(
            with(of(&["moon"], &["mass"]), list(&["lunar mass"])),
            "7.34e22 kg",
            "kg",
        ),
        constant(of(EARTH, &["mass"]), "5.9742e24 kg", "kg"),
        constant(of(EARTH, &["surface area"]), "5.10e8 km^2", "km^2"),
        constant(of(EARTH, &["volume"]), "1.0832e12 km^3", "km^3"),
        constant(
            of(EARTH, &["average density", "mean density", "density"]),
            "5.515 g/cm^3",
            "g/cm^3",
        ),
        constant(
            list(&[
                "acceleration of free fall",
                "standard acceleration of free fall",
                "standard gravity",
                "standard acceleration of gravity",
                "standard acceleration due to gravity",
                "acceleration due to gravity",
                "acceleration of gravity",
            ]),
            "9.80665 m/s^2",
            "m/s^2",
        ),
        constant(
            list(&[
                "distance between earth's center and moon's center",
                "distance between earth and moon",
                "distance from earth to moon",
                "distance from earth to the moon",
                "distance to moon",
                "earth moon distance",
                "earth to moon distance",
                "moon's distance",
                "moon distance",
                "lunar distance",
            ]),
            "384400 km",
            "km",
        ),
        constant(
            with(list(&["perihelion"]), of(EARTH, &["perihelion"])),
            "147085800 km",
            "km",
        ),
        constant(
            with(list(&["aphelion"]), of(EARTH, &["aphelion"])),
            "152104980 km",
            "km",
        ),
        constant(
            list(&[
                "average distance between earth and sun",
                "distance between earth and sun",
                "distance from earth to sun",
                "distance to sun",
                "earth sun distance",
                "average earth sun distance",
                "earth to sun distance",
                "sun's distance",
                "sun distance",
            ]),
            "149597870 km",
            "km",
        ),
        constant(
            of(
                &["sun"],
                &[
                    "average apparent size",
                    "apparent size",
                    "angular size",
                    "angular diameter",
                    "apparent diameter",
                ],
            ),
            "0.53 deg",
            "deg",
        ),
        constant(
            of(
                EARTH,
                &[
                    "average orbital velocity",
                    "orbital velocity",
                    "average orbital speed",
                    "orbital speed",
                ],
            ),
            "107244 km/hr",
            "km/hr",
        ),
        constant(
            of(EARTH, &["escape velocity", "escape speed"]),
            "11.18 km/s",
            "km/s",
        ),
        constant(
            with(
                of(EARTH, &["axial tilt", "tilt", "obliquity", "axis tilt"]),
                list(&["obliquity of the ecliptic"]),
            ),
            "23.45 deg",
            "deg",
        ),
        constant(
            of(
                EARTH,
                &["magnetic field tilt", "magnetic tilt", "magnetic axis tilt"],
            ),
            "11 deg",
            "deg",
        ),
        constant(
            of(
                EARTH,
                &[
                    "orbit inclination to ecliptic",
                    "orbital inclination to ecliptic",
                    "orbit inclination",
                    "orbital inclination",
                ],
            ),
            "0.00005 deg",
            "deg",
        ),
        constant(
            with(list(&["sidereal day"]), of(EARTH, &["sidereal day"])),
            "(23 hr + 56.1 min)",
            "hr",
        ),
        constant(
            with(list(&["sidereal year"]), of(EARTH, &["sidereal year"])),
            "365.2422 days",
            "days",
        ),
        constant(of(EARTH, &["polar diameter"]), "12712.02 km", "km"),
        constant(of(EARTH, &["equatorial diameter"]), "12756.28 km", "km"),
        constant(
            list(&[
                "distance between earth's center and an equatorial geosynchronous satellite",
                "distance between earth's center and a geosynchronous satellite",
                "geosynchronous orbit radius",
                "geostationary orbit radius",
                "geosynchronous radius",
                "geostationary radius",
                "radius of geosynchronous orbit",
                "radius of geostationary orbit",
            ]),
            "4.2108e7 m",
            "m",
        ),
        constant(
            of(
                EARTH,
                &[
                    "minimum surface temperature",
                    "lowest surface temperature",
                    "min surface temperature",
                ],
            ),
            "-69 degC",
            "degC",
        ),
        constant(
            of(
                EARTH,
                &[
                    "average surface temperature",
                    "mean surface temperature",
                    "surface temperature",
                ],
            ),
            "15 degC",
            "degC",
        ),
        constant(
            of(
                EARTH,
                &[
                    "maximum surface temperature",
                    "highest surface temperature",
                    "max surface temperature",
                ],
            ),
            "58 degC",
            "degC",
        ),
        constant(
            list(&[
                "cesium frequency",
                "cesium hyperfine frequency",
                "cesium 133 frequency",
                "cesium 133 hyperfine frequency",
                "caesium frequency",
                "caesium hyperfine frequency",
            ]),
            "9192631770 Hz",
            "Hz",
        ),
    ];
    out.extend(physics());
    out.extend(solar_system());
    out.extend(engineering());
    out.extend(fun_facts());
    out
});

/// A constant whose expression is `number unit`, answering in that unit (or a bare number).
fn plain(names: Vec<String>, expr: &'static str) -> Constant {
    let unit = expr.split_once(' ').map_or("", |(_, u)| u);
    constant(names, expr, unit)
}

/// Physical constants by name (CODATA 2022). Q Calc has no mole, so Avogadro's number and the gas constant
/// are bare numbers (per mol, and J/(mol·K)).
fn physics() -> Vec<Constant> {
    vec![
        plain(
            list(&["planck constant", "planck's constant"]),
            "6.62607015e-34 J s",
        ),
        plain(
            list(&[
                "reduced planck constant",
                "reduced planck's constant",
                "h bar",
                "dirac constant",
            ]),
            "1.054571817e-34 J s",
        ),
        plain(
            list(&["boltzmann constant", "boltzmann's constant"]),
            "1.380649e-23 J/K",
        ),
        plain(
            list(&[
                "avogadro's number",
                "avogadro number",
                "avogadro constant",
                "avogadro's constant",
            ]),
            "6.02214076e23",
        ),
        plain(
            list(&[
                "gas constant",
                "ideal gas constant",
                "universal gas constant",
                "molar gas constant",
            ]),
            "8.314462618",
        ),
        constant(
            list(&[
                "elementary charge",
                "charge of an electron",
                "electron charge",
                "proton charge",
                "charge of a proton",
            ]),
            "1.602176634e-19 coulomb",
            "coulomb",
        ),
        plain(
            list(&[
                "electron mass",
                "mass of an electron",
                "electron's mass",
                "electron rest mass",
            ]),
            "9.1093837139e-31 kg",
        ),
        plain(
            list(&[
                "proton mass",
                "mass of a proton",
                "proton's mass",
                "proton rest mass",
            ]),
            "1.67262192595e-27 kg",
        ),
        plain(
            list(&[
                "neutron mass",
                "mass of a neutron",
                "neutron's mass",
                "neutron rest mass",
            ]),
            "1.67492750056e-27 kg",
        ),
        plain(
            list(&[
                "vacuum permittivity",
                "permittivity of free space",
                "permittivity of vacuum",
                "electric constant",
                "vacuum electric permittivity",
            ]),
            "8.8541878188e-12 F/m",
        ),
        plain(
            list(&[
                "vacuum permeability",
                "permeability of free space",
                "permeability of vacuum",
                "magnetic constant",
                "vacuum magnetic permeability",
            ]),
            "1.25663706127e-6 N/A^2",
        ),
        plain(
            list(&[
                "coulomb constant",
                "coulomb's constant",
                "electrostatic constant",
            ]),
            "8.9875517862e9 N m^2/C^2",
        ),
        plain(
            list(&["stefan boltzmann constant", "stefan-boltzmann constant"]),
            "5.670374419e-8 W/(m^2 K^4)",
        ),
        plain(list(&["rydberg constant"]), "10973731.568157 m^-1"),
        plain(list(&["bohr radius"]), "5.29177210544e-11 m"),
        plain(
            list(&["fine structure constant", "fine-structure constant"]),
            "0.0072973525643",
        ),
    ]
}

/// The planets, the Moon and the Sun (NASA's planetary fact sheets) and a few distances and ages.
fn solar_system() -> Vec<Constant> {
    // body, gravity and radius (the appendix's "values of little g"), then mass, orbital period, length of a
    // solar day, sidereal rotation period and escape velocity; empty where another constant has it
    const BODIES: [[&str; 8]; 11] = [
        [
            "sun",
            "274.0 m/s^2",
            "695500 km",
            "1.9885e30 kg",
            "",
            "",
            "609.12 hr",
            "617.7 km/s",
        ],
        [
            "jupiter",
            "20.9 m/s^2",
            "71492 km",
            "1.8982e27 kg",
            "4332.59 days",
            "9.9259 hr",
            "9.925 hr",
            "59.5 km/s",
        ],
        [
            "neptune",
            "10.7 m/s^2",
            "24764 km",
            "1.02413e26 kg",
            "60189 days",
            "16.11 hr",
            "16.11 hr",
            "23.5 km/s",
        ],
        [
            "saturn",
            "10.4 m/s^2",
            "60268 km",
            "5.6834e26 kg",
            "10759.22 days",
            "10.656 hr",
            "10.656 hr",
            "35.5 km/s",
        ],
        [
            "earth",
            "9.8 m/s^2",
            "6378 km",
            "",
            "365.256 days",
            "24 hr",
            "23.9345 hr",
            "",
        ],
        [
            "venus",
            "8.9 m/s^2",
            "6051 km",
            "4.8675e24 kg",
            "224.701 days",
            "2802 hr",
            "5832.5 hr",
            "10.36 km/s",
        ],
        [
            "uranus",
            "8.4 m/s^2",
            "25559 km",
            "8.681e25 kg",
            "30685.4 days",
            "17.24 hr",
            "17.24 hr",
            "21.3 km/s",
        ],
        [
            "mars",
            "3.7 m/s^2",
            "3397 km",
            "6.4171e23 kg",
            "686.98 days",
            "24.6597 hr",
            "24.6229 hr",
            "5.03 km/s",
        ],
        [
            "mercury",
            "3.7 m/s^2",
            "2439 km",
            "3.3011e23 kg",
            "87.969 days",
            "4222.6 hr",
            "1407.6 hr",
            "4.25 km/s",
        ],
        [
            "moon",
            "1.6 m/s^2",
            "1737 km",
            "",
            "27.3217 days",
            "708.7 hr",
            "655.72 hr",
            "2.38 km/s",
        ],
        [
            "pluto",
            "0.8 m/s^2",
            "1151 km",
            "1.303e22 kg",
            "90560 days",
            "153.2928 hr",
            "153.2928 hr",
            "1.21 km/s",
        ],
    ];
    let mut out = Vec::new();
    for [body, gravity, radius, mass, orbit, day, rotation, escape] in BODIES {
        let mut names = of(
            &[body],
            &[
                "gravity",
                "surface gravity",
                "gravitational acceleration",
                "little g",
            ],
        );
        names.extend([
            format!("acceleration due to gravity on {body}"),
            format!("acceleration of gravity on {body}"),
            format!("g on {body}"),
        ]);
        let props: [(&str, &[&str]); 7] = [
            (gravity, &[]),
            (radius, &["radius", "mean radius"]),
            (mass, &["mass"]),
            (
                orbit,
                &[
                    "orbital period",
                    "orbit period",
                    "year",
                    "length of year",
                    "year length",
                ],
            ),
            (
                day,
                &[
                    "day",
                    "solar day",
                    "length of day",
                    "day length",
                    "length of a day",
                ],
            ),
            (
                rotation,
                if body == "earth" {
                    &[
                        "rotation period",
                        "rotational period",
                        "sidereal rotation period",
                    ]
                } else {
                    &[
                        "rotation period",
                        "rotational period",
                        "sidereal rotation period",
                        "sidereal day",
                    ]
                },
            ),
            (escape, &["escape velocity", "escape speed"]),
        ];
        for (expr, prop) in props {
            if expr.is_empty() {
                continue;
            }
            let names = if prop.is_empty() {
                std::mem::take(&mut names)
            } else {
                of(&[body], prop)
            };
            out.push(plain(names, expr));
        }
    }
    let sun = |props: &[&str]| of(&["sun"], props);
    out.extend([
        plain(sun(&["luminosity"]), "3.828e26 W"),
        plain(sun(&["surface temperature", "temperature"]), "5772 K"),
        plain(sun(&["age"]), "4.6e9 yr"),
        plain(
            list(&[
                "sidereal month",
                "moon's orbital period",
                "lunar orbital period",
            ]),
            "27.3217 days",
        ),
        plain(
            list(&[
                "synodic month",
                "lunar month",
                "lunar cycle",
                "moon phase cycle",
                "lunation",
            ]),
            "29.5306 days",
        ),
        plain(
            list(&[
                "distance to proxima centauri",
                "proxima centauri distance",
                "distance from earth to proxima centauri",
                "nearest star distance",
                "distance to nearest star",
            ]),
            "4.2465 ly",
        ),
        plain(
            list(&["distance to alpha centauri", "alpha centauri distance"]),
            "4.37 ly",
        ),
        plain(
            list(&[
                "distance to galactic center",
                "distance to center of milky way",
                "distance to center of galaxy",
                "galactic center distance",
            ]),
            "26000 ly",
        ),
        plain(
            list(&[
                "distance to andromeda",
                "andromeda distance",
                "distance to andromeda galaxy",
            ]),
            "2.537e6 ly",
        ),
        plain(
            with(of(&["universe"], &["age"]), list(&["age of the universe"])),
            "13.787e9 yr",
        ),
        plain(of(&["earth"], &["age"]), "4.54e9 yr"),
    ]);
    out
}

/// Materials and water: densities, speeds of sound, stiffness and strength, and water's heats.
fn engineering() -> Vec<Constant> {
    let mut out = Vec::new();
    const DENSITY: [(&str, &str); 10] = [
        ("water", "1000 kg/m^3"),
        ("air", "1.225 kg/m^3"),
        ("steel", "7850 kg/m^3"),
        ("aluminum", "2700 kg/m^3"),
        ("aluminium", "2700 kg/m^3"),
        ("concrete", "2400 kg/m^3"),
        ("ice", "917 kg/m^3"),
        ("copper", "8960 kg/m^3"),
        ("titanium", "4500 kg/m^3"),
        ("gold", "19300 kg/m^3"),
    ];
    for (m, v) in DENSITY {
        out.push(plain(of(&[m], &["density"]), v));
    }
    let sound = |m: &str| {
        list(&[
            &format!("speed of sound in {m}"),
            &format!("sound speed in {m}"),
            &format!("{m} speed of sound"),
        ])
    };
    out.push(plain(
        with(sound("air"), list(&["speed of sound", "sound speed"])),
        "343 m/s",
    ));
    out.push(plain(sound("water"), "1481 m/s"));
    out.push(plain(sound("steel"), "5960 m/s"));
    const MODULUS: [(&str, &str); 6] = [
        ("steel", "200 GPa"),
        ("aluminum", "69 GPa"),
        ("aluminium", "69 GPa"),
        ("concrete", "30 GPa"),
        ("titanium", "116 GPa"),
        ("copper", "117 GPa"),
    ];
    for (m, v) in MODULUS {
        out.push(plain(
            of(
                &[m],
                &[
                    "young's modulus",
                    "youngs modulus",
                    "modulus of elasticity",
                    "elastic modulus",
                ],
            ),
            v,
        ));
    }
    // A36 steel, 6061-T6 aluminum, Ti-6Al-4V, annealed copper
    const YIELD: [(&str, &str); 5] = [
        ("steel", "250 MPa"),
        ("aluminum", "276 MPa"),
        ("aluminium", "276 MPa"),
        ("titanium", "880 MPa"),
        ("copper", "70 MPa"),
    ];
    for (m, v) in YIELD {
        out.push(plain(of(&[m], &["yield strength", "yield stress"]), v));
    }
    const SPECIFIC_HEAT: [(&str, &str); 6] = [
        ("water", "4184 J/(kg K)"),
        ("air", "1005 J/(kg K)"),
        ("ice", "2108 J/(kg K)"),
        ("steel", "490 J/(kg K)"),
        ("aluminum", "897 J/(kg K)"),
        ("aluminium", "897 J/(kg K)"),
    ];
    for (m, v) in SPECIFIC_HEAT {
        out.push(plain(
            of(&[m], &["specific heat", "specific heat capacity"]),
            v,
        ));
    }
    out.extend([
        plain(
            list(&[
                "atmospheric pressure",
                "standard atmospheric pressure",
                "standard pressure",
                "sea level pressure",
                "air pressure at sea level",
            ]),
            "101325 Pa",
        ),
        plain(list(&["absolute zero"]), "-273.15 degC"),
        plain(
            with(of(&["water"], &["boiling point"]), list(&["boiling point"])),
            "100 degC",
        ),
        plain(
            with(
                of(&["water"], &["freezing point", "melting point"]),
                list(&["freezing point", "melting point of ice"]),
            ),
            "0 degC",
        ),
        plain(
            with(
                of(&["water"], &["latent heat of fusion", "heat of fusion"]),
                list(&["latent heat of fusion", "heat of fusion"]),
            ),
            "334 kJ/kg",
        ),
        plain(
            with(
                of(
                    &["water"],
                    &["latent heat of vaporization", "heat of vaporization"],
                ),
                list(&["latent heat of vaporization", "heat of vaporization"]),
            ),
            "2257 kJ/kg",
        ),
    ]);
    out
}

/// Heights, depths, speeds and lengths people like to compare against.
fn fun_facts() -> Vec<Constant> {
    vec![
        plain(
            list(&[
                "height of mount everest",
                "mount everest height",
                "height of everest",
                "everest height",
                "mount everest",
                "everest",
                "tallest mountain",
                "highest mountain",
            ]),
            "8849 m",
        ),
        plain(
            list(&[
                "depth of mariana trench",
                "mariana trench depth",
                "mariana trench",
                "challenger deep",
                "deepest point in ocean",
            ]),
            "10935 m",
        ),
        plain(
            with(
                of(&["earth"], &["circumference", "equatorial circumference"]),
                list(&["length of equator", "equator length"]),
            ),
            "40075 km",
        ),
        plain(
            list(&[
                "human body temperature",
                "body temperature",
                "normal body temperature",
            ]),
            "37 degC",
        ),
        plain(
            list(&["average human height", "average height", "human height"]),
            "170 cm",
        ),
        plain(
            list(&[
                "resting heart rate",
                "average resting heart rate",
                "average heart rate",
            ]),
            "70 min^-1",
        ),
        plain(
            list(&[
                "speed of a cheetah",
                "cheetah speed",
                "cheetah top speed",
                "top speed of a cheetah",
            ]),
            "112 km/h",
        ),
        plain(
            list(&[
                "fastest car",
                "top speed of fastest car",
                "fastest car speed",
                "fastest production car",
            ]),
            "490 km/h",
        ),
        plain(
            list(&[
                "cruising speed of a commercial jet",
                "commercial jet cruising speed",
                "jet cruising speed",
                "airliner cruising speed",
                "cruising speed of a jet",
                "commercial jet speed",
            ]),
            "900 km/h",
        ),
        plain(
            list(&["marathon", "marathon distance", "length of a marathon"]),
            "42.195 km",
        ),
        plain(
            list(&[
                "football field",
                "length of a football field",
                "football field length",
            ]),
            "100 yd",
        ),
        plain(
            list(&[
                "eiffel tower",
                "height of eiffel tower",
                "eiffel tower height",
            ]),
            "330 m",
        ),
        plain(list(&["smoot", "smoots"]), "1.7018 m"),
    ]
}

/// A word in the line: its byte span and its normalized spelling.
struct Word {
    start: usize,
    end: usize,
    norm: String,
}

fn line_words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start = None;
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '’';
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (start, is_word(c) && i < text.len()) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                out.push(Word {
                    start: s,
                    end: i,
                    norm: norm(&text[s..i]),
                });
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// Whether two words next to each other belong to one phrase: only spaces, or one hyphen, between them.
fn joined(text: &str, a: &Word, b: &Word) -> bool {
    let gap = &text[a.end..b.start];
    gap == "-" || (!gap.is_empty() && gap.chars().all(char::is_whitespace))
}

/// Every name by its first word, so a line only tries the names that could start where it is.
/// One name of a constant, as its words.
type Name = (&'static Constant, &'static [String]);

static BY_FIRST: LazyLock<HashMap<&'static str, Vec<Name>>> = LazyLock::new(|| {
    let mut map: HashMap<&str, Vec<_>> = HashMap::new();
    for c in CONSTANTS.iter() {
        for name in &c.names {
            map.entry(name[0].as_str())
                .or_default()
                .push((c, name.as_slice()));
        }
    }
    map
});

/// The longest constant whose name starts at word `i` (a leading `the` included): the constant and the index
/// just past its last word.
fn match_at(text: &str, words: &[Word], i: usize) -> Option<(&'static Constant, usize)> {
    let mut first = i;
    while first < words.len() && words[first].norm == "the" {
        first += 1;
    }
    if first >= words.len() || (first > i && !joined(text, &words[first - 1], &words[first])) {
        return None;
    }
    let mut best: Option<(&'static Constant, usize)> = None;
    for &(c, name) in BY_FIRST.get(words[first].norm.as_str())? {
        let (mut j, mut k) = (first, 0);
        while k < name.len() && j < words.len() {
            if j > first && !joined(text, &words[j - 1], &words[j]) {
                break;
            }
            if words[j].norm == name[k] {
                k += 1;
            } else if j == first || !FILLERS.contains(&words[j].norm.as_str()) {
                break;
            }
            j += 1;
        }
        if k == name.len() && best.is_none_or(|(_, end)| j > end) {
            best = Some((c, j));
        }
    }
    best
}

/// Whether the text after a name assigns to it (`aphelion = 3`), so the name is the user's.
fn assigned(after: &str) -> bool {
    let after = after.trim_start();
    after.starts_with('=') && !after.starts_with("==")
}

/// The line with every named constant read as its quantity, or the line as typed when it names none.
/// `is_user_name` keeps a one-word name the user defined (`aphelion = 3`) theirs.
pub fn rewrite<'a>(text: &'a str, is_user_name: &dyn Fn(&str) -> bool) -> Cow<'a, str> {
    let words = line_words(text);
    if words.is_empty() {
        return Cow::Borrowed(text);
    }
    // a line of just one constant, maybe asked as a question, answers in its own unit
    let mut first = 0;
    if words.len() > 1 && words[0].norm == "what" && words[1].norm == "is" {
        first = 2;
    } else if words[0].norm == "whats" {
        first = 1;
    }
    let lead_ok = first > 0 || text[..words[0].start].trim().is_empty();
    let tail = &text[words[words.len() - 1].end..];
    if first < words.len()
        && lead_ok
        && let Some((c, end)) = match_at(text, &words, first)
        && !(end - first == 1 && is_user_name(&text[words[first].start..words[first].end]))
    {
        if end == words.len()
            && tail
                .chars()
                .all(|c| c.is_whitespace() || c == '?' || c == '=')
        {
            return Cow::Owned(if c.unit.is_empty() {
                c.expr.to_string()
            } else {
                format!("{} to {}", c.expr, c.unit)
            });
        }
        // `absolute zero to K`: a temperature in parentheses doesn't convert, so the conversion follows bare
        if end < words.len()
            && matches!(words[end].norm.as_str(), "to" | "in")
            && joined(text, &words[end - 1], &words[end])
        {
            return Cow::Owned(format!("{} {}", c.expr, &text[words[end].start..]));
        }
    }
    let mut out = String::new();
    let (mut done, mut i) = (0, 0);
    while i < words.len() {
        // a conversion's target (`… to speed of light`) stays a unit
        let target = i > 0 && matches!(words[i - 1].norm.as_str(), "to" | "in");
        match match_at(text, &words, i) {
            Some((c, end))
                if !target
                    && !assigned(&text[words[end - 1].end..])
                    && !(end - i == 1 && is_user_name(&text[words[i].start..words[i].end])) =>
            {
                out.push_str(&text[done..words[i].start]);
                out.push('(');
                out.push_str(c.expr);
                out.push(')');
                done = words[end - 1].end;
                i = end;
            }
            _ => i += 1,
        }
    }
    if done == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[done..]);
    Cow::Owned(out)
}
