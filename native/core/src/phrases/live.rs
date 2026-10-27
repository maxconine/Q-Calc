//! `src/lib/phraseLive.ts`: the phrase layer in the bar. Same priority a SoulverCore reply had: it beats the
//! engine on natural language, and otherwise only fills a blank. (`hasSoulver` and `hostRates` read the web
//! view's window and aren't ported; the host hands rates in through [`super::PhraseOptions`].)

use super::native_eval::looks_like_natural_language;
use super::{PhraseOptions, answer_phrase, js_trim};
use crate::stubs::is_calculus_input;

/// `{display, n?}`.
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    pub display: String,
    pub n: Option<f64>,
}

/// `withPhraseAnswer`: `merged` (the bar's answer so far) or the phrase answer for `expr`.
pub fn with_phrase_answer(
    expr: &str,
    js_display: &str,
    merged: Live,
    enabled: bool,
    options: &PhraseOptions<'_>,
) -> Live {
    if !enabled || js_trim(expr).is_empty() || is_calculus_input(expr) {
        return merged;
    }
    // a real native reply already won
    if !merged.display.is_empty() && merged.display != js_display {
        return merged;
    }
    if !js_display.is_empty() && !looks_like_natural_language(expr) {
        return merged;
    }
    match answer_phrase(expr, options) {
        Some(p) => Live {
            display: p.display,
            n: p.n,
        },
        None => merged,
    }
}
