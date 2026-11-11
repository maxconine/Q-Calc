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
use std::sync::LazyLock;

struct Constant {
    /// Each name as its words, lowercase with apostrophes taken out (`earths axial tilt`).
    names: Vec<Vec<String>>,
    /// The quantity, as the unit reader reads it (`23.45 deg`).
    expr: &'static str,
    /// The unit a line of just this constant answers in.
    unit: &'static str,
}

fn words(name: &str) -> Vec<String> {
    name.split([' ', '-'])
        .filter(|w| !w.is_empty() && *w != "the")
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
    // "values of little g": each body's surface gravity and radius
    const BODIES: [(&str, &str, &str); 11] = [
        ("sun", "274.0 m/s^2", "695500 km"),
        ("jupiter", "20.9 m/s^2", "71492 km"),
        ("neptune", "10.7 m/s^2", "24764 km"),
        ("saturn", "10.4 m/s^2", "60268 km"),
        ("earth", "9.8 m/s^2", "6378 km"),
        ("venus", "8.9 m/s^2", "6051 km"),
        ("uranus", "8.4 m/s^2", "25559 km"),
        ("mars", "3.7 m/s^2", "3397 km"),
        ("mercury", "3.7 m/s^2", "2439 km"),
        ("moon", "1.6 m/s^2", "1737 km"),
        ("pluto", "0.8 m/s^2", "1151 km"),
    ];
    for (body, gravity, radius) in BODIES {
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
        out.push(constant(names, gravity, "m/s^2"));
        out.push(constant(
            of(&[body], &["radius", "mean radius"]),
            radius,
            "km",
        ));
    }
    out
});

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

/// The longest constant whose name starts at word `i`: the constant and the index just past its last word.
fn match_at(text: &str, words: &[Word], i: usize) -> Option<(&'static Constant, usize)> {
    let mut best: Option<(&'static Constant, usize)> = None;
    for c in CONSTANTS.iter() {
        for name in &c.names {
            let (mut j, mut k) = (i, 0);
            while k < name.len() && j < words.len() {
                if j > i && !joined(text, &words[j - 1], &words[j]) {
                    break;
                }
                if words[j].norm == name[k] {
                    k += 1;
                } else if words[j].norm != "the" {
                    break;
                }
                j += 1;
            }
            if k == name.len() && best.is_none_or(|(_, end)| j > end) {
                best = Some((c, j));
            }
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
        && tail
            .chars()
            .all(|c| c.is_whitespace() || c == '?' || c == '=')
        && let Some((c, end)) = match_at(text, &words, first)
        && end == words.len()
        && !(end - first == 1 && is_user_name(&text[words[first].start..words[first].end]))
    {
        return Cow::Owned(format!("{} to {}", c.expr, c.unit));
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
