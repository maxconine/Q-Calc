//! The full name of the unit, function or constant under the pointer in the line, in a small label over the word
//! (`kPa` kilopascal, `gcf` greatest common factor, `π` pi): InputField.swift's `hoverName` and WordTip.swift, the
//! web's `.quick-unit-tip`, with the core's `word_names`. `ans`, typed or the faint one a chained line continues
//! from, names the last answer it stands for ("last answer = 3,000 m", BarModel.swift `ansHover`).
//!
//! The first label waits for a 350 ms rest; once one has shown the next word's shows at once; it goes when the
//! pointer leaves the line. The label is hover.rs's system tooltip, put over the word.
//!
//! **Cost.** Nothing on a keystroke: the names are asked of the engine thread (`Lane::Misc`, which never drops a job
//! or makes one stale, so no other lane's work is disturbed) only while the pointer is over the line, once per text, and
//! hit-tested against the field's cached layout (input.rs `span_x`) as the pointer moves.

use std::cell::RefCell;

use crate::app::App;
use crate::engine::{Lane, done};

const REST_MS: u32 = 350;

/// The faint `ans`'s range stands apart from the text's.
const FAINT: (u32, u32) = (u32::MAX, u32::MAX);

/// The names in one text, and the last answer `ans` stands for.
struct Names {
    text: String,
    words: Vec<(u32, u32, String)>,
    ans: Option<String>,
}

#[derive(Default)]
struct State {
    names: Option<Names>,
    /// the text the engine is naming now
    asked: Option<String>,
    /// a label has shown since the pointer came onto the line: the next shows at once
    engaged: bool,
    /// the label up: its name and the range it names
    shown: Option<(String, (u32, u32))>,
    /// the pointer over the line, window DIPs
    pointer: Option<(f32, f32)>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State::default());
}

/// `ans`'s label: the number it stands for when there is one.
pub fn ans_label(value: Option<&str>) -> String {
    match value {
        Some(v) if !v.is_empty() => format!("last answer = {v}"),
        _ => "last answer".to_string(),
    }
}

/// The core's names with `ans`'s made the answer it stands for (UTF-16 ranges).
pub fn named(
    text: &str,
    variables: &[String],
    functions: &[String],
    ans: Option<&str>,
    has_ans: bool,
) -> Vec<(u32, u32, String)> {
    let units: Vec<u16> = text.encode_utf16().collect();
    qcalc_core::app::word_names::word_names(text, variables, functions, has_ans)
        .into_iter()
        .filter(|w| w.end > w.start && w.end <= units.len() && !w.name.is_empty())
        .map(|w| {
            let word = String::from_utf16_lossy(&units[w.start..w.end]).to_lowercase();
            let name = if word == "ans" {
                ans_label(ans)
            } else {
                w.name
            };
            (w.start as u32, w.end as u32, name)
        })
        .collect()
}

/// What the pointer at window x is on: the faint `ans`, or a named word (name, range, its left edge).
fn hit(app: &App, names: &Names, x: f32, y: f32) -> Option<(String, (u32, u32), f32)> {
    let view = &app.bar.field_view;
    if let Some(r) = view.ans_rect
        && r.contains(x, y)
        && app.model.chained
    {
        return Some((ans_label(names.ans.as_deref()), FAINT, r.x));
    }
    let field = view.rect;
    if !field.contains(x, y) {
        return None;
    }
    names.words.iter().find_map(|(a, b, name)| {
        let (l, r) = view.span_x(*a, *b)?;
        // only what's in sight of the field
        (x >= l && x <= r && r > field.x && l < field.right())
            .then(|| (name.clone(), (*a, *b), l.max(field.x)))
    })
}

/// The pointer moved over the bar (None: it left). From hover.rs's `pointer`.
pub fn moved(app: &mut App, at: Option<(f32, f32)>) {
    let over = at.filter(|&(x, y)| {
        app.bar.visible
            && !app.bar.selecting
            && (app.bar.field_view.rect.contains(x, y)
                || app
                    .bar
                    .field_view
                    .ans_rect
                    .is_some_and(|r| app.model.chained && r.contains(x, y)))
    });
    let Some((x, y)) = over else {
        let had = ST.with(|s| {
            let mut s = s.borrow_mut();
            s.pointer = None;
            s.engaged = false;
            s.shown.take().is_some()
        });
        crate::later::cancel("word-tip");
        if had {
            crate::hover::tip::hide();
        }
        return;
    };
    ST.with(|s| s.borrow_mut().pointer = Some((x, y)));
    let text = app.model.field.text().to_string();
    if text.trim().is_empty() {
        hide();
        return;
    }
    let fresh = ST.with(|s| s.borrow().names.as_ref().is_some_and(|n| n.text == text));
    if !fresh {
        // the old text's ranges would name the wrong words
        hide();
        ask(app, text);
        return;
    }
    let found = ST.with(|s| {
        let s = s.borrow();
        s.names.as_ref().and_then(|n| hit(app, n, x, y))
    });
    let Some((name, range, left)) = found else {
        hide();
        return;
    };
    let (same, now) = ST.with(|s| {
        let s = s.borrow();
        (
            s.shown
                .as_ref()
                .is_some_and(|(n, r)| *n == name && *r == range),
            s.engaged || s.shown.is_some(),
        )
    });
    if same && crate::hover::tip::shown() {
        return;
    }
    crate::later::cancel("word-tip");
    if now {
        show(app, name, range, left);
    } else {
        crate::later::after("word-tip", REST_MS, move |app| {
            // still resting on the same word
            let still = ST.with(|s| {
                let s = s.borrow();
                let (Some((x, y)), Some(n)) = (s.pointer, s.names.as_ref()) else {
                    return None;
                };
                hit(app, n, x, y).filter(|h| h.1 == range)
            });
            if let Some((name, range, left)) = still {
                show(app, name, range, left);
            }
        });
    }
}

fn hide() {
    crate::later::cancel("word-tip");
    if ST.with(|s| s.borrow_mut().shown.take().is_some()) {
        crate::hover::tip::hide();
    }
}

/// The label over the word: its bottom 2 DIPs into the top of the line's box, its left at the word's.
fn show(app: &App, name: String, range: (u32, u32), left: f32) {
    if !app.bar.visible {
        return;
    }
    ST.with(|s| {
        let mut s = s.borrow_mut();
        s.engaged = true;
        s.shown = Some((name.clone(), range));
    });
    let f = app.bar.frame();
    let s = app.bar.scale();
    let field = app.bar.field_view.rect;
    let x = f.left + (left * s).round() as i32;
    let y = f.top + ((field.y + 2.0) * s).round() as i32;
    let dark = app.theme.dark;
    let owner = app.bar.hwnd.0 as usize;
    crate::later::free("word-tip-show", 0, move || {
        crate::hover::tip::show_above(owner, &name, x, y, dark);
    });
}

/// Asks the engine thread for `text`'s names (with the history's variables, functions and `ans`).
fn ask(app: &mut App, text: String) {
    let asked = ST.with(|s| {
        let mut s = s.borrow_mut();
        if s.asked.as_deref() == Some(text.as_str()) {
            return false;
        }
        s.asked = Some(text.clone());
        true
    });
    if !asked {
        return;
    }
    let rows = app.model.rows.clone();
    let view = app.model.view(crate::model::now_ms());
    let figs = f64::from(app.model.options().sig_figs);
    app.engine.submit(Lane::Misc, move |cx| {
        let context = cx.context(&rows, view);
        let variables: Vec<String> = context.variables.keys().cloned().collect();
        let functions: Vec<String> = context.functions.keys().cloned().collect();
        let ans = context
            .chain_answer
            .as_ref()
            .map(|c| c.plain.clone())
            .filter(|p| !p.is_empty())
            .or_else(|| {
                context
                    .ans
                    .map(|n| qcalc_core::format::format_number(n, figs))
            });
        let words = named(
            &text,
            &variables,
            &functions,
            ans.as_deref(),
            context.ans.is_some(),
        );
        done(move |app| {
            let pointer = ST.with(|s| {
                let mut s = s.borrow_mut();
                if s.asked.as_deref() == Some(text.as_str()) {
                    s.asked = None;
                }
                s.names = Some(Names { text, words, ans });
                s.pointer
            });
            if pointer.is_some() {
                moved(app, pointer);
            }
        })
    });
}

/// The bar hid: no label, nothing remembered of the pointer.
pub fn will_hide() {
    crate::later::cancel("word-tip");
    ST.with(|s| {
        let mut s = s.borrow_mut();
        s.pointer = None;
        s.engaged = false;
        s.shown = None;
    });
}

/// For the e2e test (bench `wordTip`): the label up, if any, and where each named word and the faint `ans` are
/// on screen (physical pixels: [left, right, middle y]).
pub fn debug(app: &App) -> serde_json::Value {
    let f = app.bar.frame();
    let s = app.bar.scale();
    let view = &app.bar.field_view;
    let mid = f.top + ((view.rect.y + view.rect.h / 2.0) * s) as i32;
    let px = |x: f32| f.left + (x * s).round() as i32;
    ST.with(|st| {
        let st = st.borrow();
        let words: Vec<serde_json::Value> = st
            .names
            .as_ref()
            .filter(|n| n.text == app.model.field.text())
            .map(|n| {
                n.words
                    .iter()
                    .filter_map(|(a, b, name)| {
                        let (l, r) = view.span_x(*a, *b)?;
                        Some(serde_json::json!({ "name": name, "at": [px(l), px(r), mid] }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        serde_json::json!({
            "shown": st.shown.as_ref().map(|(n, _)| n.clone()),
            "tipUp": crate::hover::tip::shown(),
            "words": words,
            "ans": view.ans_rect.filter(|_| app.model.chained).map(|r| [px(r.x), px(r.right()), mid]),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_named() {
        let names = |t: &str| named(t, &[], &[], Some("3,000 m"), true);
        let n = names("5 kPa + gcf(4, 6) + π");
        let by: Vec<&str> = n.iter().map(|w| w.2.as_str()).collect();
        assert!(by.contains(&"kilopascal"), "{by:?}");
        assert!(by.contains(&"greatest common factor"), "{by:?}");
        assert!(by.contains(&"pi"), "{by:?}");
        // `ans` names the answer it stands for
        let a = names("ans*2");
        assert_eq!(a[0], (0, 3, "last answer = 3,000 m".to_string()));
        assert_eq!(ans_label(None), "last answer");
        assert!(
            names("lcd(4, 6)")
                .iter()
                .any(|w| w.2 == "least common denominator")
        );
    }
}
