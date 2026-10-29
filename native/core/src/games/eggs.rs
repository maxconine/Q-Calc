//! The 420 smoke, 67 arms and 69 fold that play over the bar when the answer settles on that number
//! (`src/lib/fourTwenty.ts`, `sixtySeven.ts`, `sixtyNine.ts`, `smokeRoom.ts`, and the hooks' timings), and the
//! native-only line eggs (divide by zero, Euler's identity, Pi Day, the barrel roll) and the Konami code.

/// Which effect an answer plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Egg {
    /// 420: smoke rolls off the bar
    Smoke,
    /// 67: two hands weigh it up
    Arms,
    /// 69: the digits fold
    Fold,
}

impl Egg {
    pub const ALL: [Egg; 3] = [Egg::Smoke, Egg::Arms, Egg::Fold];

    /// the number that sets it off
    pub fn number(self) -> f64 {
        match self {
            Egg::Smoke => 420.0,
            Egg::Arms => 67.0,
            Egg::Fold => 69.0,
        }
    }

    /// the name the web bundle used for it (games.html#smoke, #arms, #fold)
    pub fn kind(self) -> &'static str {
        match self {
            Egg::Smoke => "smoke",
            Egg::Arms => "arms",
            Egg::Fold => "fold",
        }
    }

    /// how long the effect stays up, ms: each hook's timer, a little longer than its css animation
    pub fn play_ms(self) -> f64 {
        match self {
            Egg::Smoke => SMOKE_MS,
            Egg::Arms => SHAKE_MS,
            Egg::Fold => FOLD_MS,
        }
    }
}

/// long enough that typing `4200` passes through 420 without firing (FOUR_TWENTY_SETTLE_MS; the same for 67 and 69)
pub const SETTLE_MS: f64 = 200.0;
/// useFourTwentySmoke: a little longer than the 2.5s css animation
pub const SMOKE_MS: f64 = 2700.0;
/// useSixtySevenArms
pub const SHAKE_MS: f64 = 2700.0;
/// useSixtyNineFold
pub const FOLD_MS: f64 = 2400.0;

/// fourTwentyGate / sixtySevenGate / sixtyNineGate: fires once per `target`, and re-arms only after the settled
/// answer is something else. Returns (armed, fire).
pub fn gate(target: f64, armed: bool, n: Option<f64>) -> (bool, bool) {
    if n == Some(target) {
        return (false, armed);
    }
    (true, false)
}

/// The three gates side by side, as the bar runs them: each settled answer goes through all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EggGates {
    armed: [bool; 3],
}

impl Default for EggGates {
    fn default() -> Self {
        Self { armed: [true; 3] }
    }
}

impl EggGates {
    /// the settled answer (None when it isn't a plain number); the effect to play, if any
    pub fn settle(&mut self, n: Option<f64>) -> Option<Egg> {
        let mut out = None;
        for (i, egg) in Egg::ALL.iter().enumerate() {
            let (armed, fire) = gate(egg.number(), self.armed[i], n);
            self.armed[i] = armed;
            if fire {
                out = Some(*egg);
            }
        }
        out
    }
}

/// Room the host adds around the bar while the 420 smoke plays, in css px; the bar keeps its size and spot
/// (smokeRoom.ts `SMOKE_ROOM`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeRoom {
    pub top: f64,
    pub side: f64,
    pub bottom: f64,
}

pub const SMOKE_ROOM: SmokeRoom = SmokeRoom {
    top: 160.0,
    side: 100.0,
    bottom: 64.0,
};

/// wideSmokeRoom: only a host that says it can grow the window sideways gets the wide room
pub fn wide_smoke_room(native: bool, smoke_room: bool) -> bool {
    native && smoke_room
}

/// Where along the bar a puff sits: a fraction of its top, or its left or right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PuffAt {
    Left,
    Right,
    Frac(f64),
}

/// One puff of the 420 smoke (FourTwentySmoke.tsx `PUFFS`): x, y offset it (y below the bar's top), w, h its
/// size; delay in seconds and drift, rise, sway in px, as the css variables carry them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub at: PuffAt,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub delay: f64,
    pub drift: f64,
    pub rise: f64,
    pub sway: f64,
}

#[allow(clippy::too_many_arguments)]
const fn puff(
    at: PuffAt,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    delay: f64,
    drift: f64,
    rise: f64,
    sway: f64,
) -> Puff {
    Puff {
        at,
        x,
        y,
        w,
        h,
        delay,
        drift,
        rise,
        sway,
    }
}

use PuffAt::{Frac, Left, Right};

#[rustfmt::skip]
pub const PUFFS: [Puff; 22] = [
    // off the top, overlapping into one cloud that fans out as it rises
    puff(Frac(0.08), 0.0, 10.0, 150.0, 100.0, 0.04, -58.0, -70.0, -30.0),
    puff(Frac(0.2), 0.0, -4.0, 190.0, 110.0, 0.0, -44.0, -92.0, -16.0),
    puff(Frac(0.33), 0.0, 8.0, 170.0, 104.0, 0.1, -30.0, -108.0, 8.0),
    puff(Frac(0.47), 0.0, -10.0, 220.0, 120.0, 0.02, -8.0, -96.0, -6.0),
    puff(Frac(0.6), 0.0, 6.0, 180.0, 110.0, 0.07, 18.0, -112.0, 10.0),
    puff(Frac(0.72), 0.0, -6.0, 200.0, 112.0, 0.12, 36.0, -94.0, -8.0),
    puff(Frac(0.86), 0.0, 10.0, 160.0, 100.0, 0.03, 52.0, -80.0, 22.0),
    puff(Frac(0.15), 0.0, -20.0, 110.0, 110.0, 0.44, -48.0, -74.0, -20.0),
    puff(Frac(0.4), 0.0, -30.0, 130.0, 130.0, 0.3, -20.0, -84.0, -10.0),
    puff(Frac(0.52), 0.0, -48.0, 240.0, 110.0, 0.2, 4.0, -60.0, 6.0),
    puff(Frac(0.62), 0.0, -26.0, 120.0, 124.0, 0.36, 24.0, -88.0, 8.0),
    puff(Frac(0.83), 0.0, -22.0, 110.0, 110.0, 0.48, 46.0, -76.0, 20.0),
    // the corners, out and up
    puff(Left, 16.0, 4.0, 140.0, 100.0, 0.05, -74.0, -76.0, -44.0),
    puff(Right, -16.0, 4.0, 140.0, 100.0, 0.08, 74.0, -76.0, 44.0),
    // beside the bar, rolling out and curling up
    puff(Left, -4.0, 28.0, 110.0, 96.0, 0.0, -70.0, -56.0, -46.0),
    puff(Left, 6.0, 54.0, 96.0, 104.0, 0.12, -62.0, -80.0, -40.0),
    puff(Left, -10.0, 40.0, 120.0, 100.0, 0.3, -78.0, -44.0, -54.0),
    puff(Left, 10.0, 70.0, 80.0, 80.0, 0.42, -52.0, -64.0, -36.0),
    puff(Right, 4.0, 30.0, 112.0, 94.0, 0.04, 72.0, -58.0, 48.0),
    puff(Right, -6.0, 56.0, 94.0, 106.0, 0.16, 60.0, -82.0, 40.0),
    puff(Right, 10.0, 42.0, 118.0, 98.0, 0.26, 80.0, -46.0, 56.0),
    puff(Right, -10.0, 68.0, 82.0, 82.0, 0.46, 54.0, -62.0, 34.0),
];

impl Puff {
    /// puffLeft: the puff's left edge in a layer `width` wide whose bar starts `bar_x` in from each side
    pub fn left(&self, width: f64, bar_x: f64) -> f64 {
        match self.at {
            PuffAt::Left => bar_x + self.x,
            PuffAt::Right => width - bar_x + self.x,
            PuffAt::Frac(f) => bar_x + (width - 2.0 * bar_x) * f + self.x,
        }
    }
}

// ---- native-only eggs (no web counterpart): read from the line and its answer, not the number alone

/// An effect the typed line sets off, as the bar settles on it (the same 200 ms wait as 420/67/69).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEgg {
    /// `x/0`: a small black hole swallows the answer, which then comes back
    Void,
    /// `e^(iπ)+1`: a confetti burst and a faint note
    Euler,
    /// `pi` on March 14: more digits roll out under the answer
    PiDigits,
    /// `do a barrel roll`: the bar spins once
    BarrelRoll,
}

impl LineEgg {
    pub const ALL: [LineEgg; 4] = [
        LineEgg::Void,
        LineEgg::Euler,
        LineEgg::PiDigits,
        LineEgg::BarrelRoll,
    ];

    /// how long the effect stays up, ms (each at most 2.5 s)
    pub fn play_ms(self) -> f64 {
        match self {
            LineEgg::Void => 1900.0,
            LineEgg::Euler => 2400.0,
            LineEgg::PiDigits => 2500.0,
            LineEgg::BarrelRoll => 1100.0,
        }
    }

    /// does this line (and the answer the bar shows for it) set the effect off
    pub fn matches(self, text: &str, display: &str, number: Option<f64>, pi_day: bool) -> bool {
        match self {
            LineEgg::Void => divides_by_zero(text, display),
            LineEgg::Euler => is_euler(text, display, number),
            LineEgg::PiDigits => pi_day && is_just_pi(text) && number == Some(std::f64::consts::PI),
            LineEgg::BarrelRoll => is_barrel_roll(text),
        }
    }
}

/// The line with spaces, `*`, `·` and `×` gone, `π` as `pi`, and braces as parentheses.
fn squeeze(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '*' | '·' | '×' | '⋅'))
        .map(|c| match c {
            '{' | '[' => '(',
            '}' | ']' => ')',
            '−' => '-',
            c => c,
        })
        .collect::<String>()
        .replace('π', "pi")
}

/// An answer that is undefined or infinite, from a division whose divisor is a literal zero (`1/0`, `5 ÷ (0)`,
/// `2/0.0`), not one that only works out to zero (`1/(2-2)`).
pub fn divides_by_zero(text: &str, display: &str) -> bool {
    let shown = display.trim();
    let blown = shown == "undefined" || shown.contains('∞') || shown.contains("Infinity");
    if !blown {
        return false;
    }
    let s: Vec<char> = squeeze(text).chars().collect();
    for (i, &c) in s.iter().enumerate() {
        if c != '/' && c != '÷' {
            continue;
        }
        let mut j = i + 1;
        let mut opened = 0;
        while j < s.len() && s[j] == '(' {
            opened += 1;
            j += 1;
        }
        if j < s.len() && (s[j] == '-' || s[j] == '+') {
            j += 1;
        }
        let start = j;
        while j < s.len() && s[j] == '0' {
            j += 1;
        }
        if j == start {
            continue;
        }
        if j < s.len() && s[j] == '.' {
            j += 1;
            while j < s.len() && s[j] == '0' {
                j += 1;
            }
        }
        let mut closed = 0;
        while closed < opened && j < s.len() && s[j] == ')' {
            closed += 1;
            j += 1;
        }
        let next = s.get(j).copied();
        let ends = match next {
            None => true,
            Some(n) => !(n.is_ascii_digit() || n == '.' || n.is_alphabetic() || n == '('),
        };
        if closed == opened && ends {
            return true;
        }
    }
    false
}

/// Euler's identity: `e^(iπ)+1`, `1+e^(iπ)`, `e^(iπ)+1=0`, `e^(iπ)=-1` and `e^(iπ)` itself, with `πi` for `iπ`.
/// The bare forms count when the answer is what the identity says, or when there is none (the engine leaves
/// `e^(i π)` blank today), never on another number or an error; the equation forms need no answer.
pub fn is_euler(text: &str, display: &str, number: Option<f64>) -> bool {
    let s = squeeze(text);
    let s = s
        .strip_prefix("exp")
        .map_or(s.clone(), |r| format!("e^{r}"));
    const POWERS: [&str; 4] = ["e^(ipi)", "e^(pii)", "e^(i(pi))", "e^((pi)i)"];
    let close = |want: f64| match number {
        Some(n) => (n - want).abs() < 1e-9,
        None => display.trim().is_empty(),
    };
    POWERS.iter().any(|p| {
        let sum = [format!("{p}+1"), format!("1+{p}")];
        if sum.contains(&s) {
            return close(0.0);
        }
        if s == *p {
            return close(-1.0);
        }
        let equations = [
            format!("{p}+1=0"),
            format!("1+{p}=0"),
            format!("0={p}+1"),
            format!("0=1+{p}"),
            format!("{p}=-1"),
            format!("-1={p}"),
        ];
        equations.contains(&s)
    })
}

/// the whole line is `pi` or `π`
pub fn is_just_pi(text: &str) -> bool {
    let t = text.trim();
    t.eq_ignore_ascii_case("pi") || t == "π"
}

/// the whole line is `do a barrel roll` (any case, extra spaces, a closing `!` or `.`)
pub fn is_barrel_roll(text: &str) -> bool {
    let t = text.trim().trim_end_matches(['!', '.']).to_lowercase();
    t.split_whitespace().collect::<Vec<_>>() == ["do", "a", "barrel", "roll"]
}

/// The whole line is the Konami code typed out (`↑↑↓↓←→←→ba`, spaces and case aside) or `konami`: ↵ toggles the
/// CRT look as the keys do
pub fn is_konami_line(text: &str) -> bool {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    t == "↑↑↓↓←→←→ba" || t == "konami"
}

/// What the answer slot says while the line is the Konami code (native only), like a command's `↵ …`: nothing
/// showed before, so `konami` from the `?` list looked like it did nothing. `on` is the CRT look now.
pub fn konami_hint(on: bool) -> &'static str {
    if on {
        "↵ back to the usual look"
    } else {
        "↵ an old look"
    }
}

/// March 14, by the local calendar
pub fn is_pi_day(month: u32, day: u32) -> bool {
    month == 3 && day == 14
}

/// π to 50 places, for the digits that roll out under the answer
pub const PI_DIGITS: &str = "3.14159265358979323846264338327950288419716939937510";

/// The line eggs' gates, one per effect, each like the 420 gate: fires once when the settled line sets it off,
/// and re-arms only after a settled line that doesn't.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEggGates {
    armed: [bool; 4],
}

impl Default for LineEggGates {
    fn default() -> Self {
        Self { armed: [true; 4] }
    }
}

impl LineEggGates {
    /// the settled line and its answer; the effect to play, if any (the first that fires)
    pub fn settle(
        &mut self,
        text: &str,
        display: &str,
        number: Option<f64>,
        pi_day: bool,
    ) -> Option<LineEgg> {
        let mut out = None;
        for (i, egg) in LineEgg::ALL.iter().enumerate() {
            let hit = egg.matches(text, display, number, pi_day);
            if hit && self.armed[i] && out.is_none() {
                out = Some(*egg);
            }
            self.armed[i] = !hit;
        }
        out
    }
}

/// A key as the Konami code reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KonamiKey {
    Up,
    Down,
    Left,
    Right,
    B,
    A,
    Other,
}

/// ↑↑↓↓←→←→BA
pub const KONAMI: [KonamiKey; 10] = {
    use KonamiKey::*;
    [Up, Up, Down, Down, Left, Right, Left, Right, B, A]
};

/// Watches the keys for the Konami code; every completed code toggles (the caller keeps the state).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KonamiTracker {
    /// how much of the code the last keys spell
    at: usize,
}

impl KonamiTracker {
    /// how many keys of the code the last keys spell
    pub fn progress(&self) -> usize {
        self.at
    }

    /// one key down; true when it completes the code
    pub fn push(&mut self, key: KonamiKey) -> bool {
        // the longest prefix of the code that the keys so far end with (↑↑↑ still holds ↑↑)
        let mut at = self.at;
        loop {
            if KONAMI[at] == key {
                at += 1;
                break;
            }
            if at == 0 {
                break;
            }
            at = Self::fallback(at);
        }
        if at == KONAMI.len() {
            self.at = 0;
            return true;
        }
        self.at = at;
        false
    }

    /// the longest proper prefix of KONAMI[..at] that is also its suffix
    fn fallback(at: usize) -> usize {
        (1..at)
            .rev()
            .find(|&k| KONAMI[..k] == KONAMI[at - k..at])
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn konami_says_what_enter_does() {
        assert_eq!(konami_hint(false), "↵ an old look");
        assert_eq!(konami_hint(true), "↵ back to the usual look");
    }

    #[test]
    fn divide_by_zero_needs_a_literal_zero_and_a_blown_answer() {
        for t in [
            "1/0", "-1/0", "5 / 0", "0/0", "2/0.0", "3/(0)", "1/0*0", "7 ÷ 0", "1/(−0)", "1/[0]",
        ] {
            assert!(divides_by_zero(t, "undefined"), "{t}");
        }
        assert!(divides_by_zero("1/0", "∞"));
        assert!(divides_by_zero("1/0", "-Infinity"));
        for t in ["1/(2-2)", "1/0.5", "1/05", "1/0x", "1/0pi", "10", "0/1"] {
            assert!(!divides_by_zero(t, "undefined"), "{t}");
        }
        assert!(!divides_by_zero("1/0", "0"));
        assert!(!divides_by_zero("1/0", ""));
    }

    #[test]
    fn euler_reads_the_identity_in_its_forms() {
        for t in [
            "e^(i pi)+1",
            "e^(iπ) + 1",
            "e^(i*pi)+1",
            "1 + e^(πi)",
            "e^{i pi}+1",
            "exp(i pi)+1",
        ] {
            assert!(is_euler(t, "0", Some(0.0)), "{t}");
        }
        assert!(is_euler("e^(iπ)", "-1", Some(-1.0)));
        assert!(is_euler("e^(i π)+1", "", None));
        assert!(is_euler("e^(i pi)", "-1", Some(-1.0 + 1e-16)));
        for t in [
            "e^(iπ)+1=0",
            "e^(i pi) = -1",
            "-1 = e^(πi)",
            "0 = 1 + e^(i*pi)",
        ] {
            assert!(is_euler(t, "", None), "{t}");
        }
        // a bare form whose answer isn't the identity's (a misread line) never fires
        assert!(!is_euler("exp(iπ)+1", "undefined", None));
        assert!(!is_euler("e^(iπ)", "0", Some(0.0)));
        for t in ["e^(i pi)+2", "e^(pi)+1", "e^(2 i pi)+1", "e^(iπ)+1=1"] {
            assert!(!is_euler(t, "0", Some(0.0)), "{t}");
        }
    }

    #[test]
    fn pi_and_the_barrel_roll_are_the_whole_line() {
        assert!(is_just_pi("pi") && is_just_pi(" π ") && is_just_pi("PI"));
        assert!(!is_just_pi("2pi") && !is_just_pi("pi/2"));
        for t in [
            "do a barrel roll",
            "Do A Barrel Roll!",
            "  do  a barrel roll. ",
        ] {
            assert!(is_barrel_roll(t), "{t}");
        }
        for t in ["do a barrel roll now", "barrel roll", "do a barrel"] {
            assert!(!is_barrel_roll(t), "{t}");
        }
        assert!(is_pi_day(3, 14));
        assert!(!is_pi_day(3, 15) && !is_pi_day(4, 14));
        assert!(PI_DIGITS.starts_with("3.14159265358979"));
        assert_eq!(PI_DIGITS.len(), 52);
    }

    #[test]
    fn line_gates_fire_once_per_answer() {
        let pi = Some(std::f64::consts::PI);
        let mut g = LineEggGates::default();
        assert_eq!(
            g.settle("1/0", "undefined", None, false),
            Some(LineEgg::Void)
        );
        assert_eq!(g.settle("1/0", "undefined", None, false), None);
        // still dividing by zero: no second black hole while the line stays that way
        assert_eq!(g.settle("1/00", "undefined", None, false), None);
        assert_eq!(g.settle("1/", "", None, false), None);
        assert_eq!(
            g.settle("1/0", "undefined", None, false),
            Some(LineEgg::Void)
        );
        assert_eq!(g.settle("pi", "3.14159", pi, false), None);
        assert_eq!(g.settle("pi", "3.14159", pi, true), Some(LineEgg::PiDigits));
        assert_eq!(g.settle("pi", "3.14159", pi, true), None);
        assert_eq!(
            g.settle("do a barrel roll", "", None, false),
            Some(LineEgg::BarrelRoll)
        );
        assert_eq!(g.settle("do a barrel roll", "", None, false), None);
        assert_eq!(
            g.settle("e^(iπ)+1", "0", Some(0.0), false),
            Some(LineEgg::Euler)
        );
        assert_eq!(g.settle("", "", None, false), None);
        assert_eq!(
            g.settle("do a barrel roll!", "", None, false),
            Some(LineEgg::BarrelRoll)
        );
        for e in LineEgg::ALL {
            assert!(e.play_ms() <= 2500.0);
        }
    }

    #[test]
    fn konami_toggles_on_each_whole_code() {
        use KonamiKey::*;
        let mut k = KonamiTracker::default();
        let run =
            |k: &mut KonamiTracker, keys: &[KonamiKey]| keys.iter().filter(|&&x| k.push(x)).count();
        assert_eq!(run(&mut k, &KONAMI), 1);
        assert_eq!(run(&mut k, &KONAMI), 1);
        // an extra ↑ first still counts (↑↑↑↓↓…)
        assert_eq!(run(&mut k, &[Up]), 0);
        assert_eq!(run(&mut k, &KONAMI), 1);
        // a wrong key part way resets it
        assert_eq!(
            run(
                &mut k,
                &[Up, Up, Down, Other, Down, Left, Right, Left, Right, B, A]
            ),
            0
        );
        assert_eq!(
            run(
                &mut k,
                &[Up, Up, Down, Down, Left, Right, Left, Right, A, B]
            ),
            0
        );
        // and the code after a near miss still works
        assert_eq!(
            run(
                &mut k,
                &[
                    Up, Up, Down, Down, Up, Up, Down, Down, Left, Right, Left, Right, B, A
                ]
            ),
            1
        );
    }

    // fourTwenty.test.ts, sixtyNine.test.ts, sixtySeven.test.ts: the same cases for each number
    fn run(target: f64, answers: &[Option<f64>]) -> usize {
        let mut armed = true;
        let mut fired = 0;
        for &n in answers {
            let (a, f) = gate(target, armed, n);
            armed = a;
            if f {
                fired += 1;
            }
        }
        fired
    }

    #[test]
    fn gates_fire_once_and_rearm_after_a_change() {
        for (t, near, far) in [
            (420.0, 42.0, 4200.0),
            (69.0, 6.9, 690.0),
            (67.0, 6.7, 670.0),
        ] {
            assert_eq!(run(t, &[Some(near), Some(t)]), 1);
            assert_eq!(run(t, &[Some(t), Some(t), Some(t)]), 1);
            assert_eq!(run(t, &[Some(t), None, Some(t)]), 2);
            assert_eq!(run(t, &[Some(t), Some(far), Some(t)]), 2);
            assert_eq!(run(t, &[Some(near), Some(far), Some(near), None]), 0);
        }
    }

    #[test]
    fn the_three_gates_run_side_by_side() {
        let mut g = EggGates::default();
        assert_eq!(g.settle(Some(420.0)), Some(Egg::Smoke));
        assert_eq!(g.settle(Some(420.0)), None);
        assert_eq!(g.settle(Some(69.0)), Some(Egg::Fold));
        assert_eq!(g.settle(Some(67.0)), Some(Egg::Arms));
        assert_eq!(g.settle(Some(420.0)), Some(Egg::Smoke));
        assert_eq!(g.settle(None), None);
    }

    // smokeRoom.test.ts
    #[test]
    fn wide_room_needs_a_native_host_that_says_so() {
        assert!(wide_smoke_room(true, true));
        assert!(!wide_smoke_room(true, false));
        assert!(!wide_smoke_room(false, true));
        assert!(!wide_smoke_room(false, false));
        assert_eq!(
            SMOKE_ROOM,
            SmokeRoom {
                top: 160.0,
                side: 100.0,
                bottom: 64.0
            }
        );
    }

    #[test]
    fn puffs_sit_along_the_bar() {
        assert_eq!(PUFFS[12].left(600.0, 100.0), 116.0);
        assert_eq!(PUFFS[13].left(600.0, 100.0), 484.0);
        assert!((PUFFS[0].left(600.0, 100.0) - 132.0).abs() < 1e-9);
    }

    #[test]
    fn konami_line() {
        assert!(is_konami_line("↑↑↓↓←→←→BA"));
        assert!(is_konami_line(" ↑ ↑ ↓ ↓ ← → ← → b a "));
        assert!(is_konami_line("Konami"));
        assert!(!is_konami_line("↑↑↓↓←→←→ab"));
        assert!(!is_konami_line("konami code"));
    }

    #[test]
    fn konami_progress() {
        let mut t = KonamiTracker::default();
        for k in [KonamiKey::Up, KonamiKey::Up, KonamiKey::Down] {
            t.push(k);
        }
        assert_eq!(t.progress(), 3);
    }
}
