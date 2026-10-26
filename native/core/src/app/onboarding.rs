//! First-run examples, one-time hints and the `?` cheat sheet (`src/lib/onboarding.ts`).

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::answer::has_dual_answer;
use crate::app::js as jstr;
use crate::app::keybinds::KeyAction;
use crate::app::tour::TOUR_ALL;
use crate::jsre::{self, Lazy, js};
use crate::types::{AngleMode, LineResult, SolveOutcome};

/// What onboarding has seen, persisted by the shells (`qcalc.onboarding`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Onboarding {
    pub opens: u32,
    pub commits: u32,
    /// Bitmask of hint (and tour) bits already shown.
    pub hints: u32,
    /// Examples retired for good.
    pub done: bool,
}

pub const EXAMPLE_OPENS: u32 = 3;
pub const EXAMPLE_COMMITS: u32 = 5;
pub const EXAMPLE_MS: u64 = 2500;
pub const HINT_PAUSE_MS: u64 = 1500;

pub fn empty_onboarding() -> Onboarding {
    Onboarding::default()
}

fn count(raw: Option<&Json>) -> u32 {
    let n = match raw {
        Some(Json::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Json::String(s)) => jstr::number(s).unwrap_or(f64::NAN),
        Some(Json::Bool(b)) => f64::from(u8::from(*b)),
        Some(Json::Null) => 0.0,
        Some(Json::Array(a)) if a.is_empty() => 0.0,
        Some(Json::Array(a)) if a.len() == 1 => return count(a.first()),
        _ => f64::NAN,
    };
    if n.is_finite() && n > 0.0 {
        n.floor().min(f64::from(u32::MAX)) as u32
    } else {
        0
    }
}

/// Stored onboarding read back, whatever it holds.
pub fn sanitize_onboarding(raw: &Json) -> Onboarding {
    if !raw.is_object() {
        return empty_onboarding();
    }
    let done = raw.get("done");
    Onboarding {
        opens: count(raw.get("opens")),
        commits: count(raw.get("commits")),
        hints: count(raw.get("hints")) & hint_all(),
        done: done == Some(&Json::Bool(true)) || done.and_then(Json::as_f64) == Some(1.0),
    }
}

/// The web and native copies only move forward, so take the furthest of each.
pub fn merge_onboarding(a: &Onboarding, b: &Onboarding) -> Onboarding {
    Onboarding {
        opens: a.opens.max(b.opens),
        commits: a.commits.max(b.commits),
        hints: a.hints | b.hints,
        done: a.done || b.done,
    }
}

pub fn examples_active(s: &Onboarding) -> bool {
    !s.done && s.opens <= EXAMPLE_OPENS && s.commits < EXAMPLE_COMMITS
}

fn settle(s: Onboarding) -> Onboarding {
    if !s.done && !examples_active(&s) {
        Onboarding { done: true, ..s }
    } else {
        s
    }
}

pub fn record_open(s: &Onboarding) -> Onboarding {
    settle(Onboarding {
        opens: s.opens + 1,
        ..s.clone()
    })
}

pub fn record_commit(s: &Onboarding) -> Onboarding {
    settle(Onboarding {
        commits: s.commits + 1,
        ..s.clone()
    })
}

/// An example line; `plain` has no answer beside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Example {
    pub expr: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
}

fn ex(expr: &str, plain: bool) -> Example {
    Example {
        expr: expr.into(),
        plain,
    }
}

/// Lead with what no other calculator does: the first three are seen within 8 seconds.
pub fn examples() -> Vec<Example> {
    vec![
        ex("5 ft 10 in to cm", false),
        ex("80 + 15%", false),
        ex("(5.0 +/- 0.2) * 3", false),
        ex("graph sin(x)", true),
        ex("sqrt(8)", false),
        ex("100 W * 2 hr to kWh", false),
        ex("x^2 = 2", false),
    ]
}

/// The example answer reads like the live one: exact ≈ approx when there are two, after `x =` for a solve.
pub fn example_answer(line: Option<&LineResult>) -> String {
    let display = line.map_or("", |l| l.display.as_str());
    let exact = line.and_then(|l| l.exact.as_deref());
    let answer = match exact {
        Some(e) if !e.is_empty() && has_dual_answer(display, Some(e)) => format!("{e} ≈ {display}"),
        _ => display.to_string(),
    };
    match line.and_then(|l| l.solve.as_ref()) {
        Some(s) if s.outcome == SolveOutcome::Roots => format!("{} = {answer}", s.variable),
        _ => answer,
    }
}

/// The examples, after a "Press ⌘⌥Space anytime" line on first run.
pub fn example_list(first_run_hotkey: Option<&str>) -> Vec<Example> {
    let mut out = Vec::new();
    if let Some(h) = first_run_hotkey.filter(|h| !h.is_empty()) {
        out.push(ex(&format!("Press {h} anytime"), true));
    }
    out.extend(examples());
    out
}

/// Examples run until the first keystroke of a showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rotation {
    pub on: bool,
    pub tick: u32,
}

pub const ROTATION_OFF: Rotation = Rotation { on: false, tick: 0 };

pub fn start_rotation(active: bool) -> Rotation {
    Rotation {
        on: active,
        tick: 0,
    }
}

pub fn advance_rotation(r: Rotation) -> Rotation {
    if r.on {
        Rotation {
            on: true,
            tick: r.tick + 1,
        }
    } else {
        r
    }
}

pub fn dismiss_rotation(r: Rotation) -> Rotation {
    if r.on { ROTATION_OFF } else { r }
}

pub fn rotation_item<T: Clone>(r: Rotation, items: &[T]) -> Option<T> {
    if !r.on || items.is_empty() {
        return None;
    }
    items.get(r.tick as usize % items.len()).cloned()
}

/// What a hint rule can see.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HintFacts {
    pub expr: String,
    /// The live answer as shown.
    pub answer: Option<String>,
    pub variable: Option<String>,
    pub unit: Option<bool>,
    /// The input carries on from the last answer.
    pub chained: Option<bool>,
    pub angle_mode: Option<AngleMode>,
    pub fraction_mode: Option<bool>,
    pub native: Option<bool>,
    pub opens: Option<u32>,
}

/// When a hint may show: the answer sat still (`pause`), ↵ was pressed (`commit`) or the bar opened (`open`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HintOn {
    Pause,
    #[default]
    Commit,
    Open,
}

/// A hint to show once, and the bit that records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hint {
    pub bit: u32,
    pub text: String,
}

// one line each, shown once ever, first match wins. bits are persisted: never reuse or renumber one.
// retired bits: 1 (the old "⌘C copies · Enter saved it"), 4 and 8 (units and trig after enter)
const HINT_BITS: [u32; 7] = [16, 32, 64, 128, 256, 2, 512];

fn hint_all() -> u32 {
    HINT_BITS.iter().fold(0, |a, b| a | b) | TOUR_ALL
}

fn rule(bit: u32, f: &HintFacts) -> Option<(HintOn, String)> {
    static TRIG: Lazy = Lazy::new(|| js(r"(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan)\s*\(", "i"));
    static DIVISION: Lazy = Lazy::new(|| js(r"\d\s*/\s*\d", ""));
    static DECIMAL: Lazy = Lazy::new(|| js(r"^-?\d*\.\d+$", ""));
    let flag = |b: Option<bool>| b == Some(true);
    Some(match bit {
        16 => (HintOn::Pause, "⌘C copies the answer".into()),
        32 => {
            if !jsre::test(&TRIG, &f.expr) {
                return None;
            }
            let text = if f.angle_mode == Some(AngleMode::Rad) {
                "radians · ⌃D for degrees"
            } else {
                "degrees · ⌃D for radians"
            };
            (HintOn::Pause, text.into())
        }
        64 if flag(f.unit) => (HintOn::Pause, "⌥↑ ⌥↓ step the unit prefix".into()),
        128 if !flag(f.fraction_mode)
            && jsre::test(&DIVISION, &f.expr)
            && jsre::test(&DECIMAL, f.answer.as_deref().unwrap_or("")) =>
        {
            (HintOn::Pause, "⌃F shows fractions".into())
        }
        256 => (HintOn::Commit, "↑ brings it back".into()),
        2 => {
            let v = f.variable.as_deref().filter(|v| !v.is_empty())?;
            (HintOn::Commit, format!("use {v} in later calculations"))
        }
        // not on first run, which already has the hotkey to teach
        512 if flag(f.native) && f.opens.unwrap_or(0) >= 2 => (HintOn::Open, "⌘, opens settings".into()),
        _ => return None,
    })
}

fn rule_on(bit: u32) -> HintOn {
    match bit {
        256 | 2 => HintOn::Commit,
        512 => HintOn::Open,
        _ => HintOn::Pause,
    }
}

/// The first hint for this moment not seen yet, whose condition holds.
pub fn pick_hint(seen: u32, facts: &HintFacts, on: HintOn) -> Option<Hint> {
    HINT_BITS.iter().find_map(|&bit| {
        if rule_on(bit) != on || seen & bit != 0 {
            return None;
        }
        rule(bit, facts).map(|(_, text)| Hint { bit, text })
    })
}

pub const HOTKEY_FAILED_HINT: &str =
    "no shortcut works — open Q Calc from the menu bar icon, or change it in the menu";

pub fn is_help_command(text: &str) -> bool {
    let t = jstr::trim(text).to_lowercase();
    t == "?" || t == "help"
}

/// Typing past `?` or `help` keeps just the new text.
pub fn after_help_input(prev: &str, next: &str) -> String {
    if !is_help_command(prev) || jstr::len16(next) <= jstr::len16(prev) || !next.starts_with(prev) {
        return next.to_string();
    }
    next[prev.len()..].to_string()
}

/// One cheat-sheet row: the keys (or what to type) and what it does.
pub type CheatRow = (String, String);

const CHEAT_KEYS: [(&str, &str); 9] = [
    ("↵", "save"),
    ("↑", "history"),
    ("⌘C ⌘⇧C", "copy answer / line"),
    ("⌥↑ ⌥↓", "unit prefix"),
    ("⌃D", "degrees / radians"),
    ("⌃F", "fractions"),
    ("⌃S", "sig figs from input"),
    ("⌃C", "clear history"),
    ("esc", "hide"),
];

const CHEAT_TYPE: [(&str, &str); 11] = [
    ("ft to cm", "units"),
    ("80 + 15%", "percent"),
    ("+/-  -/+", "± ∓ uncertainty"),
    ("x = 5", "variables"),
    ("x^2 = 2", "solve"),
    ("sys 2", "systems"),
    ("graph x^2", "plot"),
    ("GCF  LCM", "factors"),
    ("sum n, n=1..9", "Σ sums"),
    ("tutorial", "the walkthrough"),
    ("?", "this list"),
];

fn row_actions(label: &str) -> Option<&'static [KeyAction]> {
    Some(match label {
        "copy answer / line" => &[KeyAction::CopyAnswer, KeyAction::CopyLine],
        "degrees / radians" => &[KeyAction::Angle],
        "fractions" => &[KeyAction::Fraction],
        "sig figs from input" => &[KeyAction::SigFigs],
        "clear history" => &[KeyAction::Clear],
        "settings" => &[KeyAction::Settings],
        _ => return None,
    })
}

/// The `?` sheet: [keys, things to type]. `labels` are the user's chosen keys, already written for the host; a row
/// whose keys are all unset goes. Only the Mac app has the settings window (`native`).
pub fn cheat_sheet(
    hotkey: Option<&str>,
    native: bool,
    labels: Option<&std::collections::BTreeMap<KeyAction, String>>,
) -> [Vec<CheatRow>; 2] {
    let own = |(a, b): &(&str, &str)| (a.to_string(), b.to_string());
    let mut keys: Vec<CheatRow> = Vec::new();
    if let Some(h) = hotkey.filter(|h| !h.is_empty()) {
        keys.push((h.to_string(), "show / hide".into()));
    }
    keys.extend(CHEAT_KEYS.iter().map(own));
    if native {
        let at = keys.len() - 1;
        keys.insert(at, ("⌘,".into(), "settings".into()));
    }
    if let Some(labels) = labels {
        keys = keys
            .into_iter()
            .filter_map(|(key, label)| match row_actions(&label) {
                None => Some((key, label)),
                Some(actions) => {
                    let shown: Vec<&str> = actions
                        .iter()
                        .filter_map(|a| labels.get(a).map(String::as_str))
                        .filter(|s| !s.is_empty())
                        .collect();
                    (!shown.is_empty()).then(|| (shown.join(" "), label))
                }
            })
            .collect();
    }
    [keys, CHEAT_TYPE.iter().map(own).collect()]
}
