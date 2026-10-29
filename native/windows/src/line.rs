//! The input line's side of the bar (BarModel.swift's input half, Tour.swift's examples): what the line draws
//! over its text and where that comes from, the keys that finish a line, the draft, the empty bar's examples, the
//! d/dt chips, the SI prefix step, pastes, and the bar's timers (the caret's blink, the examples' rotation, the
//! squiggle's pause).
//!
//! **Keystroke cost.** Everything a keystroke adds comes back with the live answer in the same lane job
//! (`decorate`, on the engine thread): the inferred parens, unit and known-word spans and the completion. The
//! typing rules that change the text itself (symbols, brackets, limits) are a few precompiled regexes over one
//! short line and run as the key is typed (input.rs). The squiggle waits for typing to pause 700 ms, then asks the
//! engine on `Lane::Input`; so do the prefix step and the examples' answers. Nothing here evaluates on the UI
//! thread.
//!
//! **Timers.** All on the bar window, one-shot (each tick re-arms only if still wanted), none while the bar is
//! hidden: the blink (only while shown and focused; a keystroke restarts it solid), the examples' rotation (2.5 s,
//! only while the line is empty and examples are on), the squiggle (700 ms after a blank answer lands).

use std::cell::RefCell;
use std::collections::HashMap;

use qcalc_core::app::onboarding::{self, Rotation};
use qcalc_ffi::{EvalContext, LiveAnswer, LiveRequest, TimeLetters};

use crate::engine::{EngineThread, Lane, done};
use crate::input::Decorations;
use crate::keys::Mods;
use crate::model::{BarModel, Outcome, now_ms};
use crate::render::Rect;
use crate::store::Draft;

pub const TIMER_BLINK: usize = 0x4C01;
pub const TIMER_EXAMPLE: usize = 0x4C02;
pub const TIMER_SQUIGGLE: usize = 0x4C03;
/// SQUIGGLE_IDLE_MS
const SQUIGGLE_MS: u32 = 700;

/// The input's state beside the field (a field of `BarModel`).
#[derive(Default)]
pub struct LineState {
    /// what the line draws over its text, and the text it was worked out for
    pub deco: Option<Decorations>,
    pub deco_for: String,
    /// the rest of the word being typed, drawn after the caret; Tab or → takes it
    pub completion: Option<String>,
    /// d/dt letters moved by hand (TimeLetters.tsx); sent as EvalOptions.timeVarying
    pub time_varying: HashMap<String, bool>,
    /// the answer one SI prefix up or down (Alt+↑ / Alt+↓), until the line changes
    pub prefix_shown: Option<String>,
    prefix_value: Option<String>,
    /// the line a unit was offered for, and what ↵ again makes it
    unit_offer: Option<(String, String)>,
    /// the empty bar's examples
    pub rotation: Rotation,
    /// (id, expr, answer, plain)
    pub example: Option<(u32, String, String)>,
    /// equations pasted to open as a system (`sys N`): systems.rs takes them (`take`) and opens its fields
    pub pasted_system: Option<Vec<String>>,
    /// the caret's blink phase (false: hidden this half-period)
    pub caret_on: bool,
    /// the field's state at the last `after`, to restart the blink on a change: (selection, text hash)
    seen: (usize, usize, u64),
    blinking: bool,
    /// since when the caret has blinked unchanged: after Windows' caret timeout it stays on and the timer stops
    blink_from: Option<std::time::Instant>,
    example_armed: bool,
    squiggle_armed: Option<String>,
    /// a blank line waiting for its squiggle
    squiggle_for: Option<String>,
    /// the draft kept across hides and restarts (draft.json), read once
    draft: Option<Option<Draft>>,
    /// the d/dt chips' rects at the last paint: (rect, letter, varying, locked)
    pub chips: RefCell<Vec<(Rect, String, bool, bool)>>,
    /// a selected part of the line's own answer, and the (line, selection) it was asked for (selection.rs)
    pub selection_answer: Option<String>,
    pub sel_seen: (String, usize, usize),
}

// ---- on the engine thread, with the live answer ----

/// QuickInput.tsx's decorations for `typed`: inferParens, unitSpans, knownWordSpans; and the completion (the
/// engine's when it answered, else completionFor with the history's names).
pub fn decorate(
    typed: &str,
    context: &EvalContext,
    answer: &LiveAnswer,
) -> (Decorations, Option<String>) {
    let fill = qcalc_core::parens::infer_parens(typed);
    let variables: Vec<String> = context.variables.keys().cloned().collect();
    let functions: Vec<String> = context.functions.keys().cloned().collect();
    let message = typed.trim_start().to_lowercase().starts_with("message");
    let (units, words) = if message {
        (Vec::new(), Vec::new())
    } else {
        (
            qcalc_core::app::unit_spans::unit_spans(typed, &variables, &functions)
                .into_iter()
                .map(|s| (s.start as u32, s.end as u32))
                .collect(),
            qcalc_core::app::known_words::known_word_spans(
                typed,
                &functions,
                context.ans.is_some(),
            )
            .into_iter()
            .map(|s| (s.start as u32, s.end as u32))
            .collect(),
        )
    };
    let deco = Decorations {
        leading: if message { 0 } else { fill.leading },
        trailing: if message { 0 } else { fill.trailing },
        units,
        words,
        squiggle: None,
        offer: None,
    };
    let mut completion = answer.completion.clone().filter(|c| !c.is_empty());
    if completion.is_none() {
        let names = qcalc_core::app::completion::CompletionNames {
            variables,
            functions,
            ans: context.ans.is_some(),
            contacts: Vec::new(),
        };
        let c = qcalc_core::app::completion::completion_for(typed, &names);
        completion = (!c.is_empty()).then_some(c);
    }
    (deco, completion)
}

/// A live answer landed for `typed` (still the line): its decorations, and the squiggle's wait if it is blank.
pub fn landed(m: &mut BarModel, typed: &str, deco: Decorations, completion: Option<String>) {
    // a squiggle or an offer already up for this very text stays
    let keep = m.line.deco_for == typed;
    let (squiggle, offer) = match (&m.line.deco, keep) {
        (Some(d), true) => (d.squiggle, d.offer.clone()),
        _ => (None, None),
    };
    m.line.deco = Some(Decorations {
        squiggle,
        offer,
        ..deco
    });
    m.line.deco_for = typed.to_string();
    m.line.completion = completion;
    let blank = m.live.as_ref().is_none_or(|a| a.display.is_empty());
    m.line.squiggle_for = (blank && !typed.trim().is_empty()).then(|| typed.to_string());
    m.touch();
}

/// The line changed: what belonged to the old text goes.
pub fn text_changed(m: &mut BarModel) {
    m.line.prefix_shown = None;
    m.line.prefix_value = None;
    m.line.unit_offer = None;
    m.line.completion = None;
    m.line.squiggle_for = None;
    if let Some(d) = &mut m.line.deco {
        d.squiggle = None;
        d.offer = None;
    }
    if !m.field.text().is_empty() {
        m.line.rotation = onboarding::dismiss_rotation(m.line.rotation);
        m.line.example = None;
    }
    if m.field.text().trim().is_empty() {
        m.line.deco = None;
    }
}

/// After a commit saved nothing: the unit a number was owed (missingUnit, when the units were improper) and where
/// the line stops reading (blankReason), on the engine thread.
pub fn after_failed_commit(
    engine: &qcalc_ffi::Engine,
    request: &LiveRequest,
    improper: bool,
) -> (Option<qcalc_ffi::UnitOffer>, Option<(u32, u32)>) {
    if improper && let Some(fix) = engine.missing_unit(request.clone()) {
        return (Some(fix), None);
    }
    let graph = request.text.trim().to_lowercase().starts_with("graph");
    let span = if graph {
        None
    } else {
        engine
            .blank_span(request.clone())
            .filter(|s| s.end > s.start)
            .map(|s| (s.start, s.end))
    };
    (None, span)
}

/// The commit's offer and squiggle, on the UI thread.
pub fn show_failed_commit(
    m: &mut BarModel,
    typed: &str,
    offer: Option<qcalc_ffi::UnitOffer>,
    span: Option<(u32, u32)>,
) {
    let d = m.line.deco.get_or_insert_with(Decorations::default);
    if let Some(o) = offer {
        d.offer = Some((o.at, o.text.clone()));
        m.line.unit_offer = Some((typed.to_string(), o.filled));
        m.hint = Some(o.hint);
    }
    if span.is_some() {
        d.squiggle = span;
    }
    m.line.deco_for = typed.to_string();
    m.touch();
}

/// ↵ before committing: an offered unit is taken (the line becomes what it filled); a word still waiting at the
/// end becomes its symbol (finishTokens). True when ↵ is used up.
pub fn before_enter(m: &mut BarModel, engine: &EngineThread) -> bool {
    if let Some((typed, filled)) = m.line.unit_offer.take()
        && typed == m.field.text()
    {
        m.hint = None;
        m.field.set_text(&filled);
        m.text_changed(engine);
        return true;
    }
    if !m.field.keep_words
        && !m
            .field
            .text()
            .trim_start()
            .to_lowercase()
            .starts_with("message")
    {
        let (t, _) = crate::text_rules::pretty(m.field.text(), 0, false);
        if t != m.field.text() {
            m.field.set_text(&t);
            m.text_changed(engine);
        }
    }
    false
}

// ---- keys ----

const TAB: u32 = 0x09;
const ENTER: u32 = 0x0D;
const UP: u32 = 0x26;
const RIGHT: u32 = 0x27;
const DOWN: u32 = 0x28;

/// The input's keys, before the bar's own (model.rs `key_down` asks first): Alt+↑ / Alt+↓ step the answer's SI
/// prefix; →, tab, ↑, ↓, ↵ move between a sign's limits; → at the end and tab take the completion, then a chain's
/// faint `ans`, then the inferred parens (autofillParens). None leaves the key to the bar.
pub fn key_down(m: &mut BarModel, vk: u32, mods: Mods, engine: &EngineThread) -> Option<Outcome> {
    if mods.alt && !mods.ctrl && !mods.shift && (vk == UP || vk == DOWN) {
        step_prefix(m, if vk == UP { 1 } else { -1 }, engine);
        return Some(Outcome::Handled);
    }
    if mods.ctrl || mods.alt || mods.win {
        return None;
    }
    if m.selected.is_some() && vk != TAB {
        return None;
    }
    let before = m.field.text().to_string();
    let name = match vk {
        RIGHT if !mods.shift => "ArrowRight",
        UP if !mods.shift => "ArrowUp",
        DOWN if !mods.shift => "ArrowDown",
        TAB if !mods.shift => "Tab",
        ENTER if !mods.shift => "Enter",
        _ => return None,
    };
    if m.field.bound_key(name) {
        m.touch();
        if m.field.text() != before {
            m.text_changed(engine);
        }
        return Some(Outcome::Handled);
    }
    // Tab during the walkthrough types the step's line (fillTourStep), before anything else
    if vk == TAB && !mods.shift && m.selected.is_none() && crate::tour::fill_step(m) {
        m.field.break_run();
        m.text_changed(engine);
        return Some(Outcome::Handled);
    }
    let at_end = m.field.caret_at_end();
    if ((vk == RIGHT && at_end) || (vk == TAB && !mods.shift)) && m.selected.is_none() {
        if at_end && let Some(rest) = m.line.completion.clone() {
            m.field.insert(&rest);
            m.field.break_run();
            m.text_changed(engine);
            return Some(Outcome::Handled);
        }
        // the faint `ans` and the grey parens become typed ones where they're drawn, together
        if fill_parens(m) {
            m.text_changed(engine);
            return Some(Outcome::Handled);
        }
    }
    // Tab on an empty line moves to the newest card's reply (tabToCard)
    if vk == TAB && !mods.shift && m.field.text().is_empty() && crate::messaging::tab_to_card(m) {
        return Some(Outcome::Handled);
    }
    None
}

/// The faint `ans` a chained line continues from, as drawn before the text and as Tab types it: what the engine
/// reads the line as (`chainedExpr`) less the line itself, so `*2` shows `ans` and `to mm` shows `ans `.
pub fn ans_ghost(text: &str) -> String {
    let full = qcalc_core::app::chain::chained_expr(text);
    match full.strip_suffix(text) {
        Some(lead) if !lead.is_empty() => lead.to_string(),
        _ => "ans".to_string(),
    }
}

/// The faint `ans` the line shows now: only on a chained line whose answer is in ("" for none).
fn chain_ghost(m: &BarModel) -> String {
    let text = m.field.text();
    if m.chained && m.live_for == text && !text.trim_start().to_lowercase().starts_with("message") {
        ans_ghost(text)
    } else {
        String::new()
    }
}

/// The line with its grey parens typed in where they're drawn (acceptParens), and the caret kept by the same
/// character: past the new `(`s, or at the very end when it was at the end. None when there's nothing to add.
pub fn parens_filled(text: &str, caret: usize) -> Option<(String, usize)> {
    lead_filled(text, caret, "")
}

/// As `parens_filled`, with a chained line's faint `ans` (`ghost`) typed in too, after the `(`s as it's drawn:
/// `*2` → `ans*2`, nothing moving (the Mac's acceptParens).
pub fn lead_filled(text: &str, caret: usize, ghost: &str) -> Option<(String, usize)> {
    if text.trim().is_empty() || text.trim_start().to_lowercase().starts_with("message") {
        return None;
    }
    let fill = qcalc_core::parens::infer_parens(text);
    if fill.leading + fill.trailing == 0 && ghost.is_empty() {
        return None;
    }
    let filled = format!(
        "{}{ghost}{text}{}",
        "(".repeat(fill.leading),
        ")".repeat(fill.trailing)
    );
    let at = if caret >= text.len() {
        filled.len()
    } else {
        caret + fill.leading + ghost.len()
    };
    Some((filled, at))
}

/// Tab (or → at the end): the faint `ans` and the grey parens become typed ones, the grey ones going in the same
/// step so nothing on the line moves.
fn fill_parens(m: &mut BarModel) -> bool {
    let caret = m.field.selection().0;
    let ghost = chain_ghost(m);
    let Some((filled, at)) = lead_filled(m.field.text(), caret, &ghost) else {
        return false;
    };
    if !ghost.is_empty() {
        // typed in now: the faint one goes in the same frame
        m.chained = false;
    }
    if let Some(d) = &mut m.line.deco {
        d.leading = 0;
        d.trailing = 0;
    }
    m.field.set_text_at(&filled, at);
    m.field.break_run();
    true
}

/// Ctrl+V: equations on several lines (or with `;`) open as a system; anything else is pasted as one line.
pub fn paste(m: &mut BarModel, raw: &str) {
    if let Some(lines) = crate::text_rules::pasted_system(raw) {
        let n = lines.len();
        m.line.pasted_system = Some(lines);
        m.field.set_text(&format!("sys {n}"));
        return;
    }
    m.field.paste(raw);
}

/// onPrefixStep: the answer one SI prefix up or down (stepPrefix), shown until the line changes.
fn step_prefix(m: &mut BarModel, dir: i32, engine: &EngineThread) {
    let base = m
        .line
        .prefix_value
        .clone()
        .or_else(|| m.live.as_ref().and_then(|a| a.value_json.clone()));
    let Some(base) = base else { return };
    let typed = m.field.text().to_string();
    let sig = m.settings.sig_figs.clamp(2, 16);
    engine.submit(Lane::Input, move |cx| {
        let next = cx
            .engine
            .call_json(
                "engine/units".into(),
                "stepPrefix".into(),
                format!("[{base},{dir}]"),
            )
            .ok()?;
        let v: serde_json::Value = serde_json::from_str(&next).ok()?;
        v.get("unitId")?;
        let shown = cx
            .engine
            .call_json(
                "engine/format".into(),
                "formatValue".into(),
                format!("[{next},{sig}]"),
            )
            .ok()?;
        let shown: String = serde_json::from_str(&shown).ok()?;
        done(move |app| {
            let m = &mut app.model;
            if m.field.text() == typed {
                m.line.prefix_value = Some(next);
                m.line.prefix_shown = Some(shown);
                m.touch();
            }
        })
    });
}

// ---- the d/dt chips ----

/// The letters under a d/dt, when there are any.
pub fn letters(m: &BarModel) -> Option<&TimeLetters> {
    let a = m.live.as_ref()?;
    if m.live_for != m.field.text() && m.field.text().trim().is_empty() {
        return None;
    }
    a.time_letters
        .as_ref()
        .filter(|l| !l.varying.is_empty() || !l.constant.is_empty())
}

/// A click at window DIPs (x, y): a chip moves its letter to the other side and the line answers again.
pub fn click_chip(m: &mut BarModel, x: f32, y: f32, engine: &EngineThread) -> bool {
    let hit = m
        .line
        .chips
        .borrow()
        .iter()
        .find(|(r, _, _, _)| r.contains(x, y))
        .cloned();
    let Some((_, letter, varying, locked)) = hit else {
        return false;
    };
    if !locked {
        m.line.time_varying.insert(letter, !varying);
        m.evaluate_live(engine);
        m.touch();
    }
    true
}

// ---- showing and hiding: the draft and the examples ----

fn draft(m: &mut BarModel) -> Option<Draft> {
    m.line.draft.get_or_insert_with(Draft::load).clone()
}

/// willHide: a line with something in it is kept for `draftSeconds` (draft.json, so a restart keeps it too); with
/// "keep unfinished" off (0) a line with an answer is saved to the tape and the line cleared.
pub fn will_hide(m: &mut BarModel, engine: &EngineThread) {
    let text = m.field.text().to_string();
    m.line.example = None;
    m.line.squiggle_for = None;
    if text.trim().is_empty() {
        m.line.draft = Some(None);
        engine.submit(Lane::Misc, |_| {
            Draft::clear();
            None
        });
    } else if m.settings.draft_seconds > 0 {
        let d = Draft {
            expr: text,
            saved_at: now_ms(),
        };
        m.line.draft = Some(Some(d.clone()));
        engine.submit(Lane::Misc, move |_| {
            if let Err(e) = d.save() {
                crate::qlog!("draft: couldn't save: {e}");
            }
            None
        });
    } else {
        if m.live.as_ref().is_some_and(|a| !a.display.is_empty()) {
            m.commit(engine);
        }
        m.field.reset("");
        m.text_changed(engine);
    }
    m.touch();
}

/// willShow: the draft comes back if the bar was away less than `draftSeconds` (shouldRestoreDraft); the open is
/// counted and the examples start (examplesActive: the first three opens, until five saves).
pub fn will_show(
    m: &mut BarModel,
    onboarding: &mut crate::store::Onboarding,
    engine: &EngineThread,
) {
    m.field.keep_words = m.settings.keep_words;
    let seconds = m.settings.draft_seconds;
    match draft(m) {
        Some(d) if d.fresh(now_ms(), seconds) => {
            if m.field.text() != d.expr {
                m.field.reset(&d.expr);
                m.text_changed(engine);
            }
        }
        other => {
            if other.is_some() || seconds <= 0 || !m.field.text().is_empty() {
                if !m.field.text().is_empty() {
                    m.field.reset("");
                    m.text_changed(engine);
                }
                if other.is_some() {
                    m.line.draft = Some(None);
                    engine.submit(Lane::Misc, |_| {
                        Draft::clear();
                        None
                    });
                }
            }
        }
    }
    let o = core_onboarding(onboarding);
    let o = onboarding::record_open(&o);
    onboarding.merge(&serde_json::json!({
        "opens": o.opens, "commits": o.commits, "hints": o.hints, "done": o.done,
    }));
    m.line.rotation = onboarding::start_rotation(onboarding::examples_active(&o));
    refresh_example(m, engine);
}

fn core_onboarding(o: &crate::store::Onboarding) -> onboarding::Onboarding {
    onboarding::Onboarding {
        opens: o.opens as u32,
        commits: o.commits as u32,
        hints: o.hints as u32,
        done: o.done,
    }
}

/// The example the empty bar shows now, and its answer (evaluated off the UI thread).
fn refresh_example(m: &mut BarModel, engine: &EngineThread) {
    if !m.field.text().is_empty() {
        m.line.example = None;
        return;
    }
    let list = onboarding::example_list(None);
    let Some(item) = onboarding::rotation_item(m.line.rotation, &list) else {
        m.line.example = None;
        return;
    };
    let id = m.line.rotation.tick;
    m.line.example = Some((id, item.expr.clone(), String::new()));
    m.touch();
    if item.plain {
        return;
    }
    let typed = crate::text_rules::pretty(&item.expr, item.expr.len(), m.settings.keep_words).0;
    let options = m.options();
    engine.submit(Lane::Input, move |cx| {
        let line = cx
            .engine
            .evaluate(vec![typed], options)
            .into_iter()
            .next()?;
        if line.display.is_empty() {
            return None;
        }
        let mut answer = line.display.clone();
        if let Some(e) = &line.exact
            && e.trim() != line.display.trim()
        {
            answer = format!("{e} ≈ {}", line.display);
        }
        if line.kind == qcalc_ffi::AnswerKind::Solve
            && let Some(v) = &line.variable
        {
            answer = format!("{v} = {answer}");
        }
        done(move |app| {
            if let Some((i, e, a)) = &mut app.model.line.example
                && *i == id
                && *e == item.expr
            {
                *a = answer;
                app.model.touch();
            }
        })
    });
}

/// The placeholder: the rotating example, or "Calculate".
pub fn placeholder(m: &BarModel) -> String {
    // the walkthrough's step example (tour.rs) stands in for the rotation while it shows
    if let Some(p) = crate::tour::placeholder(m) {
        return p.to_string();
    }
    match &m.line.example {
        Some((_, expr, _)) => crate::text_rules::pretty(expr, expr.len(), false).0,
        None => "Calculate".into(),
    }
}

// ---- timers ----

/// A bar timer fired; false when it isn't the input's.
pub fn on_timer(m: &mut BarModel, id: usize, engine: &EngineThread, shown: bool) -> bool {
    match id {
        TIMER_BLINK => {
            m.line.caret_on = !m.line.caret_on;
            m.line.blinking = false;
            m.touch();
        }
        TIMER_EXAMPLE => {
            if shown && m.line.rotation.on && m.field.text().is_empty() {
                m.line.rotation = onboarding::advance_rotation(m.line.rotation);
                refresh_example(m, engine);
            }
        }
        TIMER_SQUIGGLE => {
            let Some(typed) = m.line.squiggle_for.take() else {
                return true;
            };
            if typed != m.field.text() || m.live.as_ref().is_some_and(|a| !a.display.is_empty()) {
                return true;
            }
            let rows = m.rows.clone();
            let now = now_ms();
            let view = m.view(now);
            let options = m.options();
            engine.submit(Lane::Input, move |cx| {
                if qcalc_core::app::needs::looks_like_natural_language(&typed) {
                    return None;
                }
                let context = cx.context(&rows, view);
                let chained = cx.engine.chains(typed.clone(), context.clone());
                let request = LiveRequest {
                    text: typed.clone(),
                    options,
                    context,
                    chained,
                    now_ms: now,
                    phrases: true,
                };
                let span = cx.engine.blank_span(request).filter(|s| s.end > s.start)?;
                done(move |app| {
                    let m = &mut app.model;
                    if m.field.text() == typed
                        && m.live.as_ref().is_none_or(|a| a.display.is_empty())
                        && let Some(d) = &mut m.line.deco
                    {
                        d.squiggle = Some((span.start, span.end));
                        m.touch();
                    }
                })
            });
        }
        _ => return false,
    }
    true
}

/// After each message (app.rs `after`): the input's timers armed or stopped for the bar as it is now. Hidden or
/// unfocused, none run. The blink restarts solid whenever the line or the caret changed.
pub fn after(app: &mut crate::app::App) {
    use windows::Win32::UI::WindowsAndMessaging::{GetCaretBlinkTime, KillTimer, SetTimer};
    crate::selection::after(app);
    crate::a11y::after(app);
    let hwnd = Some(app.bar.hwnd);
    let shown = app.bar.visible;
    let focused = app.bar.focused;
    let m = &mut app.model;
    m.field.keep_words = m.settings.keep_words;
    let t = m.field.text();
    let hash = t.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    let (a, b) = m.field.selection();
    let seen = (a, b, hash);
    let l = &mut m.line;
    let changed = seen != l.seen;
    l.seen = seen;
    // SAFETY: timers on our own bar window
    unsafe {
        if shown && focused {
            if changed && !l.caret_on {
                l.caret_on = true;
                m.touch();
            }
            let l = &mut m.line;
            if changed || l.blink_from.is_none() {
                l.blink_from = Some(std::time::Instant::now());
            }
            // as Windows' own edit boxes: the blink stops, caret shown, after the caret timeout (5 s by default)
            let tired = l.caret_on
                && l.blink_from.is_some_and(|t| {
                    caret_tired(t.elapsed().as_millis() as u64, caret_timeout_ms())
                });
            if changed || (!l.blinking && !tired) {
                l.blinking = true;
                // GetCaretBlinkTime is INFINITE (u32::MAX) when blinking is off in Settings
                let ms = GetCaretBlinkTime();
                if ms > 0 && ms < 10_000 {
                    SetTimer(hwnd, TIMER_BLINK, ms, None);
                }
            }
        } else if l.blinking || !l.caret_on || l.blink_from.is_some() {
            l.blink_from = None;
            l.blinking = false;
            l.caret_on = true;
            let _ = KillTimer(hwnd, TIMER_BLINK);
        }
        let l = &mut m.line;
        let example = shown && l.rotation.on && m.field.text().is_empty();
        if example != l.example_armed {
            l.example_armed = example;
            if example {
                SetTimer(hwnd, TIMER_EXAMPLE, EXAMPLE_DELAY_MS, None);
            } else {
                let _ = KillTimer(hwnd, TIMER_EXAMPLE);
            }
        }
        let squiggle = if shown { l.squiggle_for.clone() } else { None };
        if squiggle != l.squiggle_armed {
            if squiggle.is_some() {
                SetTimer(hwnd, TIMER_SQUIGGLE, SQUIGGLE_DELAY_MS, None);
            } else {
                let _ = KillTimer(hwnd, TIMER_SQUIGGLE);
            }
            l.squiggle_armed = squiggle;
        }
    }
}

/// Whether a caret that has blinked `ms` unchanged should rest (shown, no timer), for a timeout of `timeout` ms
/// (0: never rests).
pub fn caret_tired(ms: u64, timeout: u32) -> bool {
    timeout > 0 && ms >= u64::from(timeout)
}

/// Windows' caret timeout (SPI_GETCARETTIMEOUT; 5000 ms unless changed), 0 when it can't be read.
fn caret_timeout_ms() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{
        SYSTEM_PARAMETERS_INFO_ACTION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut ms = 0u32;
    // SAFETY: a live out u32 (SPI_GETCARETTIMEOUT, 0x2022)
    let ok = unsafe {
        SystemParametersInfoW(
            SYSTEM_PARAMETERS_INFO_ACTION(0x2022),
            0,
            Some((&mut ms as *mut u32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_ok() { ms } else { 5000 }
}

/// WM_TIMER on the bar: each of the input's timers is one-shot (`after` arms the next if still wanted).
pub fn timer(app: &mut crate::app::App, id: usize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::KillTimer;
    if ![TIMER_BLINK, TIMER_EXAMPLE, TIMER_SQUIGGLE].contains(&id) {
        return false;
    }
    // SAFETY: our own bar's timer
    let _ = unsafe { KillTimer(Some(app.bar.hwnd), id) };
    match id {
        TIMER_EXAMPLE => app.model.line.example_armed = false,
        TIMER_SQUIGGLE => app.model.line.squiggle_armed = None,
        _ => {}
    }
    let shown = app.bar.visible;
    on_timer(&mut app.model, id, &app.engine, shown)
}

pub const SQUIGGLE_DELAY_MS: u32 = SQUIGGLE_MS;
pub const EXAMPLE_DELAY_MS: u32 = onboarding::EXAMPLE_MS as u32;

#[cfg(windows)]
pub use paint::*;

#[cfg(windows)]
mod paint {
    use super::*;
    use crate::input::{Extras, FieldView};
    use crate::render::{Canvas, measure, size, with_alpha};
    use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING;

    /// The line in `field` (window.rs calls this for the composer): lead, text, ghosts, caret; with the empty
    /// bar's example answer at the composer's right.
    pub fn paint_field(
        c: &Canvas,
        view: &mut FieldView,
        m: &BarModel,
        field: Rect,
        composer: Rect,
        focused: bool,
    ) {
        let deco = m.line.deco.as_ref();
        let placeholder = placeholder(m);
        let x = Extras {
            deco,
            chained: m.chained && !m.field.text().is_empty(),
            completion: m.line.completion.as_deref(),
            placeholder: &placeholder,
            caret_on: m.line.caret_on || !focused,
        };
        view.paint(c, &m.field, field, focused, &x);
        // .live-example: the example's answer, muted at 70%
        if m.field.text().is_empty()
            && crate::tour::placeholder(m).is_none()
            && let Some((_, _, answer)) = &m.line.example
            && !answer.is_empty()
        {
            let w = composer.w - 2.0 * size::GUTTER;
            if let Some(l) = c.gfx.layout_in(
                answer,
                &c.gfx.fonts.display,
                w * 0.45,
                field.h,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            ) {
                c.text(
                    &l,
                    composer.right() - size::GUTTER - w * 0.45,
                    field.y,
                    with_alpha(c.theme.muted, 0.7),
                );
            }
        }
    }

    /// The d/dt chips' row height (0 without letters).
    pub fn letters_height(m: &BarModel) -> f32 {
        if letters(m).is_some() { 22.0 } else { 0.0 }
    }

    /// .time-letters: "changing with time" chips, "·", "constant" chips; small, muted labels.
    pub fn paint_letters(c: &Canvas, m: &BarModel, r: Rect) {
        let mut chips = m.line.chips.borrow_mut();
        chips.clear();
        let Some(l) = letters(m) else { return };
        let t = c.theme;
        let mut x = r.x;
        let label = |x: &mut f32, s: &str| {
            if let Some(lay) = c.gfx.layout(s, &c.gfx.fonts.small, 0.0, r.h) {
                c.text_plain(&lay, *x, r.y, t.muted);
                *x += measure(&lay).0 + 6.0;
            }
        };
        let mut chip = |x: &mut f32, letter: &str, varying: bool| {
            let locked = l.locked.iter().any(|s| s == letter);
            let Some(lay) = c.gfx.layout(letter, &c.gfx.fonts.small, 0.0, 20.0) else {
                return;
            };
            let w = (measure(&lay).0 + 14.0).max(22.0);
            let rect = Rect::new(*x, r.y + (r.h - 20.0) / 2.0, w, 20.0);
            let a = if locked { 0.75 } else { 1.0 };
            c.fill_round(
                rect,
                size::RADIUS_S,
                if varying {
                    with_alpha(t.accent, 0.14 * a)
                } else {
                    with_alpha(t.fill_hover, a)
                },
            );
            let fg = if varying { t.accent } else { t.muted };
            c.text_plain(
                &lay,
                rect.x + (w - measure(&lay).0) / 2.0,
                rect.y,
                with_alpha(fg, a),
            );
            chips.push((rect, letter.to_string(), varying, locked));
            *x += w + 4.0;
        };
        if !l.varying.is_empty() {
            label(&mut x, "changing with time");
            for v in &l.varying {
                chip(&mut x, v, true);
            }
        }
        if !l.varying.is_empty() && !l.constant.is_empty() {
            label(&mut x, "·");
        }
        if !l.constant.is_empty() {
            label(&mut x, "constant");
            for v in &l.constant {
                chip(&mut x, v, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caret_rests_after_the_timeout() {
        assert!(!caret_tired(4999, 5000));
        assert!(caret_tired(5000, 5000));
        assert!(!caret_tired(60_000, 0));
    }

    #[test]
    fn decorations_come_from_the_core() {
        let a = LiveAnswer {
            completion: None,
            ..blank()
        };
        let (d, completion) = decorate("(5 km + sqr", &EvalContext::default(), &a);
        assert_eq!(d.trailing, 1);
        assert_eq!(d.units, vec![(3, 5)]);
        assert_eq!(completion.as_deref(), Some("t"));
        let (d, _) = decorate("5+3)*2", &EvalContext::default(), &a);
        assert_eq!(d.leading, 1);
        let (d, _) = decorate("message bob (", &EvalContext::default(), &a);
        assert_eq!(d.trailing, 0);
    }

    #[test]
    fn tab_types_the_grey_parens_where_they_stand() {
        // `5+3)*2` reads as `(5+3)*2`: the caret after `3` stays after `3`
        assert_eq!(parens_filled("5+3)*2", 3), Some(("(5+3)*2".into(), 4)));
        // at the end it stays at the end, past the new `)`
        assert_eq!(parens_filled("(2+3", 4), Some(("(2+3)".into(), 5)));
        assert_eq!(parens_filled("sqrt(2", 1), Some(("sqrt(2)".into(), 1)));
        assert_eq!(parens_filled("2+3", 3), None);
        assert_eq!(parens_filled("", 0), None);
        assert_eq!(parens_filled("message bob (", 13), None);
    }

    #[test]
    fn tab_types_the_faint_ans_where_it_stands() {
        // `*2` continues from the last answer: `ans` goes in front, the caret kept by its character
        assert_eq!(ans_ghost("*2"), "ans");
        assert_eq!(lead_filled("*2", 2, "ans"), Some(("ans*2".into(), 5)));
        assert_eq!(lead_filled("*2", 1, "ans"), Some(("ans*2".into(), 4)));
        // with grey parens too: `(` first, as they're drawn
        assert_eq!(
            lead_filled("*2)+1", 5, "ans"),
            Some(("(ans*2)+1".into(), 9))
        );
        // a conversion reads `ans to mm`
        assert_eq!(ans_ghost("to mm"), "ans ");
        assert_eq!(ans_ghost(" to mm"), "ans");
        assert_eq!(lead_filled("2+3", 3, ""), None);
    }

    fn blank() -> LiveAnswer {
        // a blank answer the way the engine makes one, via its JSON-free constructor path
        let e = qcalc_ffi::Engine::new(qcalc_ffi::EngineConfig {
            is_windows: true,
            locale: "en_US".into(),
            data_dir: None,
            time_zone: None,
        });
        e.live(LiveRequest {
            text: String::new(),
            options: qcalc_ffi::default_eval_options(),
            context: EvalContext::default(),
            chained: false,
            now_ms: 0,
            phrases: false,
        })
    }
}
