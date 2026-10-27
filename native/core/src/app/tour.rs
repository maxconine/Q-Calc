//! The first-run walkthrough (`src/lib/tour.ts`). Its progress lives in the onboarding `hints` bitmask, which the
//! shells already persist and merge by union. Bits stay below 1 << 30; never reuse or renumber one.

use serde::{Deserialize, Serialize};

use crate::app::onboarding::Onboarding;
use crate::jsre::{self, Lazy, js};

pub const TOUR_ON: u32 = 1 << 12;
/// Skipped, or the install was past onboarding when the tour arrived.
pub const TOUR_OFF: u32 = 1 << 13;
/// The tour retires after this many opens however far it got.
pub const TOUR_OPENS: u32 = 3;

/// A step or an extra.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TourId {
    Hotkey,
    Arithmetic,
    Units,
    Chain,
    Copy,
    Solve,
    Graph,
    Periodic,
    Help,
}

/// One line of the walkthrough: shown faintly in the empty bar (`plain` has no answer beside it), with `text` under it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TourStep {
    pub id: TourId,
    pub bit: u32,
    pub expr: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
    pub text: String,
}

fn step(id: TourId, bit: u32, expr: &str, plain: bool, text: &str) -> TourStep {
    TourStep {
        id,
        bit,
        expr: expr.into(),
        plain,
        text: text.into(),
    }
}

/// The steps in order: each line waits for the ones before it.
pub fn tour_steps(hotkey: &str) -> Vec<TourStep> {
    vec![
        step(
            TourId::Hotkey,
            1 << 14,
            &format!("{hotkey} opens Q Calc from anywhere"),
            true,
            &format!("try it: {hotkey} hides me, {hotkey} again brings me back"),
        ),
        step(
            TourId::Arithmetic,
            1 << 15,
            "12 * 7",
            false,
            "type 12 * 7 and press ↵",
        ),
        step(
            TourId::Units,
            1 << 16,
            "5 ft to cm",
            false,
            "units convert too: type 5 ft to cm, then ↵",
        ),
        step(
            TourId::Chain,
            1 << 17,
            "* 2",
            true,
            "carry on from the last answer: type * 2, then ↵",
        ),
        step(
            TourId::Copy,
            1 << 18,
            "2^10",
            false,
            "⌘C copies the answer: type 2^10, then ⌘C",
        ),
    ]
}

/// One per open once the basics are done, and only for what hasn't been used already.
pub fn tour_extras() -> Vec<TourStep> {
    vec![
        step(
            TourId::Solve,
            1 << 19,
            "x^2 = 2",
            false,
            "you can also solve: type x^2 = 2",
        ),
        step(
            TourId::Graph,
            1 << 20,
            "graph sin(x)",
            true,
            "or plot one: graph sin(x), then ↵",
        ),
        step(
            TourId::Periodic,
            1 << 21,
            "periodic",
            true,
            "periodic ↵ opens the periodic table",
        ),
        step(
            TourId::Help,
            1 << 22,
            "?",
            true,
            "? lists everything else, any time",
        ),
    ]
}

/// Every bit the walkthrough uses.
pub const TOUR_ALL: u32 = TOUR_ON | TOUR_OFF | (0b11111 << 14) | (0b1111 << 19);

pub const TOUR_DONE_HINT: &str = "that’s the basics · ? lists everything";
pub const TUTORIAL_HINT: &str = "start the tutorial";

/// The bit of a step or extra.
pub fn tour_bit(id: TourId) -> u32 {
    tour_steps("")
        .into_iter()
        .chain(tour_extras())
        .find(|s| s.id == id)
        .map_or(0, |s| s.bit)
}

/// Decided once per install, on the first open that sees it: a new install takes the tour, one already past
/// onboarding never does. Whatever it has clearly done already counts as done.
pub fn enrol_tour(s: &Onboarding, native: bool) -> Onboarding {
    // past its opens it's off for good, written down like a skip; `tutorial` still replays it
    if s.hints & TOUR_OFF != 0 {
        return s.clone();
    }
    let with = |hints: u32| Onboarding { hints, ..s.clone() };
    if s.opens > TOUR_OPENS {
        return with(s.hints | TOUR_OFF);
    }
    if s.hints & TOUR_ON != 0 {
        return s.clone();
    }
    if s.done || !native {
        return with(s.hints | TOUR_OFF);
    }
    let known = (if s.opens >= 2 {
        tour_bit(TourId::Hotkey)
    } else {
        0
    }) | (if s.commits > 0 {
        tour_bit(TourId::Arithmetic)
    } else {
        0
    });
    with(s.hints | TOUR_ON | known)
}

pub fn tour_running(s: &Onboarding) -> bool {
    s.hints & TOUR_ON != 0 && s.hints & TOUR_OFF == 0 && s.opens <= TOUR_OPENS
}

fn shown_steps(hotkey: &str) -> Vec<TourStep> {
    tour_steps(hotkey)
        .into_iter()
        .filter(|s| !hotkey.is_empty() || s.id != TourId::Hotkey)
        .collect()
}

/// The first step not done yet. The hotkey step is left out when there's no shortcut to teach.
pub fn tour_step(done: u32, hotkey: &str) -> Option<TourStep> {
    shown_steps(hotkey).into_iter().find(|s| done & s.bit == 0)
}

/// How far along: step `at` of `of`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TourProgress {
    pub at: usize,
    pub of: usize,
}

pub fn tour_progress(done: u32, hotkey: &str) -> TourProgress {
    let steps = shown_steps(hotkey);
    let at = steps
        .iter()
        .position(|s| done & s.bit == 0)
        .map_or(steps.len(), |i| i + 1);
    TourProgress {
        at,
        of: steps.len(),
    }
}

/// The next "you can also", once the basics are done.
pub fn tour_extra(done: u32, hotkey: &str) -> Option<TourStep> {
    if tour_step(done, hotkey).is_some() {
        return None;
    }
    tour_extras().into_iter().find(|e| done & e.bit == 0)
}

/// What a saved calculation was.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TourFacts {
    pub expr: String,
    pub unit: bool,
    pub chained: bool,
    pub solve: bool,
    pub graph: bool,
}

/// What a saved calculation counts toward, steps and extras alike.
pub fn tour_used(f: &TourFacts) -> u32 {
    let mut bits = if f.graph {
        tour_bit(TourId::Graph)
    } else {
        tour_bit(TourId::Arithmetic)
    };
    if f.unit {
        bits |= tour_bit(TourId::Units);
    }
    if f.chained {
        bits |= tour_bit(TourId::Chain);
    }
    if f.solve {
        bits |= tour_bit(TourId::Solve);
    }
    bits
}

/// Whether the line under the bar still fits what's typed: an empty bar, or the start of the step's own text.
pub fn tour_line_fits(step: &TourStep, typed: &str, answer: bool) -> bool {
    if crate::app::js::trim(typed).is_empty() {
        return true;
    }
    if step.id == TourId::Hotkey {
        return false;
    }
    // ⌘C works on any answer, so the copy line stays while there is one
    if step.id == TourId::Copy && answer {
        return true;
    }
    let squash = |t: &str| {
        t.chars()
            .filter(|c| !crate::app::js::is_js_space(*c))
            .collect::<String>()
            .to_lowercase()
    };
    squash(&step.expr).starts_with(&squash(typed))
}

pub fn is_tutorial_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^(?:tutorial|tour)$", "i"));
    jsre::test(&RE, crate::app::js::trim(text))
}

pub fn is_skip_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^skip(?:\s+(?:tutorial|tour))?$", "i"));
    jsre::test(&RE, crate::app::js::trim(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits() {
        let all = tour_steps("")
            .iter()
            .chain(&tour_extras())
            .fold(TOUR_ON | TOUR_OFF, |a, s| a | s.bit);
        assert_eq!(all, TOUR_ALL);
        assert_eq!(tour_progress(0, ""), TourProgress { at: 1, of: 4 });
    }
}
