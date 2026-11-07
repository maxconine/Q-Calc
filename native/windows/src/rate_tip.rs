//! The rate behind a currency conversion, on the answer's tip (BarModel.swift `rateNote`, BarView.swift): with
//! `100 usd to eur` on the line the tip reads `1 USD = 0.8909 EUR · ECB rate, Nov 5 · click to copy`.
//!
//! The core's `lib/currency` `rateNote` says it, from the rates file the engine reads (`rates.json`). It is asked
//! on the engine's misc lane when the pointer comes onto the answer, never on the keystroke path, and kept for
//! the line it was asked for; the tip shows 450 ms later and reads what came back.

use std::cell::RefCell;

use crate::app::App;
use crate::engine::{Lane, done};

thread_local! {
    /// (the line, its note): the last line asked about
    static MEMO: RefCell<Option<(String, Option<String>)>> = const { RefCell::new(None) };
}

/// The pointer came onto the answer: ask for the line's note unless it's known.
pub fn ask(app: &App) {
    let text = app.model.field.text().to_string();
    if app.model.live.as_ref().is_none_or(|a| a.display.is_empty())
        || !could_convert(&text)
        || MEMO.with(|m| m.borrow().as_ref().is_some_and(|(t, _)| *t == text))
    {
        return;
    }
    let Some(path) = crate::store::config_dir().map(|d| d.join("rates.json")) else {
        return;
    };
    app.engine.submit(Lane::Misc, move |cx| {
        let rates = std::fs::read_to_string(&path).ok()?;
        let args = serde_json::json!([text, rates, crate::model::now_ms()]).to_string();
        let note = cx
            .engine
            .call_json("lib/currency".into(), "rateNote".into(), args)
            .ok()
            .and_then(|s| serde_json::from_str::<Option<String>>(&s).ok().flatten())
            .filter(|s| !s.is_empty());
        done(move |_| MEMO.with(|m| *m.borrow_mut() = Some((text, note))))
    });
}

/// The answer's tip for this line: the rate when it converts currencies (a lone answer's tip only, not a dual's
/// or a root's).
pub fn tip(app: &App, plain: &str) -> String {
    let text = app.model.field.text();
    MEMO.with(|m| match &*m.borrow() {
        Some((t, Some(note))) if t == text && plain.starts_with("Copy to clipboard") => {
            format!("{note} · click to copy")
        }
        _ => plain.to_string(),
    })
}

/// Only a line with a number and a conversion word can name a rate (`rateNote` is null for anything else).
fn could_convert(text: &str) -> bool {
    let t = text.to_lowercase();
    t.chars().any(|c| c.is_ascii_digit())
        && [" to ", " in ", " as ", " into "]
            .iter()
            .any(|w| t.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_conversions_are_asked_about() {
        assert!(could_convert("100 usd to eur"));
        assert!(could_convert("$20/hr * 40 hr in EUR"));
        assert!(!could_convert("usd to eur"));
        assert!(!could_convert("2+2"));
    }
}
