//! Onboarding and the walkthrough, as Tour.swift runs them over the core's `onboarding` and `tour` rules: the
//! first-run flow (the tour enrolled on the first opens, its step's example faint in the empty bar with its answer
//! beside it), the open's hint, a hint after an answer has sat still 1.5 s, a hint after a save, the tutorial's line
//! under the bar (`n/m`, skip), `tutorial` / `skip`, and the "you can also" extras on later opens. The state is
//! settings.json's `onboarding` (store.rs), shared with the Tauri app's page, so progress carries over.
//!
//! The rotating examples of the first opens, and counting the opens, are the input line's (line.rs on the
//! `win-input` branch): `will_show` runs after it has counted this one.
//!
//! The rules' words name the Mac's keys (⌘C, ⌃D, ⌘,); `winify` puts the Windows chords in, following the user's
//! keybinds.

use qcalc_core::app::onboarding::{self as ob, HintFacts, HintOn, Onboarding};
use qcalc_core::app::tour::{self as core_tour, TourFacts, TourId};

use crate::keys::{KeyAction, Keybinds};

/// The tutorial's line under the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub at: usize,
    pub of: usize,
}

/// What the empty bar shows: an example and (once the engine has said) its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Example {
    pub id: i64,
    pub expr: String,
    pub answer: String,
}

/// The walkthrough's state on the bar (BarModel's `example`, `tourLine`, `replay`, `rotation`...).
#[derive(Clone, Debug, Default)]
pub struct Tour {
    /// `tutorial` replays the steps from scratch: the bits done in this replay
    pub replay: Option<u32>,
    pub line: Option<Line>,
    pub example: Option<Example>,
    ghost_example: Option<String>,
    ghost_plain: Option<String>,
    /// an extra's (hint, expr, plain), shown in the empty bar while its hint is up
    hint_ghost: Option<(String, String, bool)>,
    /// the stored onboarding as last saved, and the shortcut's title, so a keystroke's refresh needs only the bar
    o: Onboarding,
    hotkey: String,
}

/// The Mac's key glyphs in the core's words, as the Windows chords (the user's own, from keybinds).
pub fn winify(text: &str, keybinds: &Keybinds) -> String {
    if !text.contains(['⌘', '⌃', '⌥']) {
        return text.to_string();
    }
    let chord = |a: KeyAction, fallback: &str| {
        keybinds
            .chord_for(a)
            .map_or_else(|| fallback.to_string(), crate::hotkey::title)
    };
    text.replace("⌘⇧C", &chord(KeyAction::CopyLine, "Ctrl+Shift+C"))
        .replace("⌘C", &chord(KeyAction::CopyAnswer, "Ctrl+C"))
        .replace("⌃D", &chord(KeyAction::Angle, "Ctrl+D"))
        .replace("⌃F", &chord(KeyAction::Fraction, "Ctrl+F"))
        .replace("⌘,", &chord(KeyAction::Settings, "Ctrl+,"))
        .replace(['⌘', '⌃'], "Ctrl+")
        .replace('⌥', "Alt+")
}

pub fn onboarding_of(s: &crate::store::Onboarding) -> Onboarding {
    ob::sanitize_onboarding(&serde_json::json!({
        "opens": s.opens, "commits": s.commits, "hints": s.hints, "done": s.done,
    }))
}

fn stored(o: &Onboarding) -> crate::store::Onboarding {
    crate::store::Onboarding {
        opens: u64::from(o.opens),
        commits: u64::from(o.commits),
        hints: u64::from(o.hints),
        done: o.done,
    }
}

/// The step the walkthrough is on (`tourStepNow`), given what's done and whether it runs.
pub fn step_now(o: &Onboarding, replay: Option<u32>, hotkey: &str) -> Option<core_tour::TourStep> {
    let running = replay.is_some() || core_tour::tour_running(o);
    if !running {
        return None;
    }
    core_tour::tour_step(replay.unwrap_or(o.hints), hotkey)
}

#[cfg(windows)]
pub use ui::*;

#[cfg(windows)]
mod ui {
    use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_RANGE};

    use super::*;
    use crate::app::App;
    use crate::engine::{EngineThread, Lane, done};
    use crate::later;
    use crate::model::BarModel;
    use crate::render::{Canvas, Gfx, Rect, measure, size, with_alpha};

    fn hotkey_title(app: &App) -> String {
        let chord = app
            .hotkey
            .active
            .clone()
            .unwrap_or_else(|| app.hotkey.wanted.clone());
        crate::hotkey::title(&chord)
    }

    fn state(app: &App) -> Onboarding {
        onboarding_of(&app.store.onboarding)
    }

    fn save(app: &mut App, o: &Onboarding) {
        let next = stored(o);
        if next != app.store.onboarding {
            app.store
                .onboarding
                .merge(&serde_json::to_value(next).unwrap_or_default());
            app.save_store();
        }
        app.model.tour.o = state(app);
    }

    fn hints(app: &App) -> u32 {
        state(app).hints
    }

    fn touring(app: &App) -> bool {
        app.model.tour.replay.is_some() || core_tour::tour_running(&state(app))
    }

    fn done_bits(app: &App) -> u32 {
        app.model.tour.replay.unwrap_or_else(|| hints(app))
    }

    fn step(app: &App) -> Option<core_tour::TourStep> {
        step_now(&state(app), app.model.tour.replay, &hotkey_title(app))
    }

    /// The walkthrough owns the line under the bar until its basics are done.
    pub fn basics_left(app: &App) -> bool {
        step(app).is_some()
    }

    pub fn mark(app: &mut App, bits: u32) {
        if bits == 0 {
            return;
        }
        let mut o = state(app);
        if o.hints & bits != bits {
            o.hints |= bits;
            save(app, &o);
        }
        if let Some(r) = app.model.tour.replay {
            app.model.tour.replay = Some(r | bits);
        }
        refresh(&mut app.model, &app.engine);
    }

    fn mark_hint(app: &mut App, bit: u32) {
        let mut o = state(app);
        o.hints |= bit;
        save(app, &o);
    }

    /// `tutorial`.
    pub fn start(app: &mut App) {
        app.model.tour.replay = Some(0);
        app.model.hint = None;
        refresh(&mut app.model, &app.engine);
    }

    /// `skip`, or the line's skip.
    pub fn skip(app: &mut App) {
        if app.model.tour.replay.is_some() {
            app.model.tour.replay = None;
        } else {
            mark(app, core_tour::TOUR_OFF);
        }
        app.model.hint = Some("skipped · type tutorial any time to see it again".into());
        refresh(&mut app.model, &app.engine);
    }

    /// Each showing (`beginShowing`), after the open was counted: the tour enrolled, the open's hint or an extra.
    pub fn will_show(app: &mut App) {
        app.model.tour.hotkey = hotkey_title(app);
        app.model.tour.o = state(app);
        let o = core_tour::enrol_tour(&state(app), true);
        save(app, &o);
        let o = state(app);
        // the hotkey's notice (app.notice) already has the line when the shortcut couldn't be had
        if app.notice.is_none() {
            let facts = HintFacts {
                expr: String::new(),
                native: Some(true),
                opens: Some(o.opens),
                ..HintFacts::default()
            };
            if let Some(h) = ob::pick_hint(o.hints, &facts, HintOn::Open) {
                mark_hint(app, h.bit);
                app.model.hint = Some(winify(&h.text, &app.model.keybinds));
            }
        }
        // an open after the first is the shortcut learned; once the basics are done each open brings one extra
        if o.opens >= 2 || app.model.tour.replay.is_some() {
            mark(app, core_tour::tour_bit(TourId::Hotkey));
        }
        if touring(app)
            && app.notice.is_none()
            && let Some(extra) = core_tour::tour_extra(done_bits(app), &hotkey_title(app))
        {
            mark(app, extra.bit);
            let text = winify(&extra.text, &app.model.keybinds);
            app.model.hint = Some(text.clone());
            app.model.tour.hint_ghost = Some((text, extra.expr, extra.plain));
        }
        refresh(&mut app.model, &app.engine);
    }

    pub fn will_hide(m: &mut BarModel) {
        m.tour.example = None;
        later::cancel("pause-hint");
    }

    /// The line changed (model.rs `text_changed`): the tour's line and example follow the text. Cheap: no store,
    /// and the engine only when a step's example needs its answer.
    pub fn text_changed(m: &mut BarModel, engine: &EngineThread) {
        later::cancel("pause-hint");
        refresh(m, engine);
    }

    /// The line under the bar and the example the step shows, after anything that moves the tour.
    pub fn refresh(m: &mut BarModel, engine: &EngineThread) {
        let typed = m.field.text().to_string();
        let answered = m.live.as_ref().is_some_and(|a| !a.display.is_empty());
        let keybinds = m.keybinds.clone();
        let hint = m.hint.clone();
        let t = &mut m.tour;
        let before = (
            t.line.clone(),
            t.ghost_example.clone(),
            t.ghost_plain.clone(),
        );
        if let Some(step) = step_now(&t.o, t.replay, &t.hotkey) {
            let progress = core_tour::tour_progress(t.replay.unwrap_or(t.o.hints), &t.hotkey);
            let fits = core_tour::tour_line_fits(&step, &typed, answered);
            t.line = fits.then(|| Line {
                text: winify(&step.text, &keybinds),
                at: progress.at,
                of: progress.of,
            });
            (t.ghost_example, t.ghost_plain) = if step.plain {
                (None, Some(step.expr))
            } else {
                (Some(step.expr), None)
            };
        } else {
            t.line = None;
            if t.hint_ghost.as_ref().map(|g| &g.0) != hint.as_ref() {
                t.hint_ghost = None;
            }
            (t.ghost_example, t.ghost_plain) = match &t.hint_ghost {
                Some((_, e, true)) => (None, Some(e.clone())),
                Some((_, e, false)) => (Some(e.clone()), None),
                None => (None, None),
            };
        }
        if before
            != (
                t.line.clone(),
                t.ghost_example.clone(),
                t.ghost_plain.clone(),
            )
        {
            m.touch();
        }
        refresh_example(m, engine);
    }

    fn refresh_example(m: &mut BarModel, engine: &EngineThread) {
        if !m.field.text().is_empty() {
            if m.tour.example.take().is_some() {
                m.touch();
            }
            return;
        }
        let at = m.tour.line.as_ref().map_or(0, |l| l.at as i64);
        let (id, expr, plain) = if let Some(g) = &m.tour.ghost_example {
            (-1 - at, g.clone(), false)
        } else if let Some(p) = &m.tour.ghost_plain {
            (-1 - at, p.clone(), true)
        } else {
            if m.tour.example.take().is_some() {
                m.touch();
            }
            return;
        };
        let expr = winify(&expr, &m.keybinds);
        if m.tour
            .example
            .as_ref()
            .is_some_and(|e| e.id == id && e.expr == expr)
        {
            return;
        }
        m.tour.example = Some(Example {
            id,
            expr: expr.clone(),
            answer: String::new(),
        });
        m.touch();
        if plain {
            return;
        }
        // exampleAnswer: evaluated as the settings say, exact ≈ approx when there are two, `x =` for a solve
        let options = m.options();
        engine.submit(Lane::Misc, move |cx| {
            let line = cx
                .engine
                .evaluate(vec![expr.clone()], options)
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
            if let Some(v) = &line.variable
                && line.kind == qcalc_ffi::AnswerKind::Solve
            {
                answer = format!("{v} = {answer}");
            }
            done(move |app| {
                if let Some(ex) = &mut app.model.tour.example
                    && ex.id == id
                    && ex.expr == expr
                {
                    ex.answer = answer;
                    app.model.touch();
                }
            })
        });
    }

    fn facts_now(m: &BarModel, expr: &str) -> HintFacts {
        let live = m.live.as_ref();
        HintFacts {
            expr: expr.to_string(),
            answer: live.map(|a| a.display.clone()),
            variable: live.and_then(|a| a.variable.clone()),
            unit: Some(live.is_some_and(|a| a.unit.is_some())),
            chained: Some(m.chained),
            angle_mode: Some(if m.settings.angle_mode == "rad" {
                qcalc_core::types::AngleMode::Rad
            } else {
                qcalc_core::types::AngleMode::Deg
            }),
            fraction_mode: Some(m.settings.fraction_mode),
            native: Some(true),
            ..HintFacts::default()
        }
    }

    /// What the line was, for `after_commit` (taken before the commit replaces it).
    pub struct Committed {
        facts: HintFacts,
        solve: bool,
    }

    pub fn committed(m: &BarModel, expr: &str) -> Committed {
        Committed {
            facts: facts_now(m, expr),
            solve: m.live.as_ref().is_some_and(|a| a.solve.is_some()),
        }
    }

    /// After a save: the commit's hint (unless the tour has the line), the count, and the tour steps it used.
    pub fn after_commit(app: &mut App, expr: &str, c: Committed) {
        let next = if basics_left(app) {
            None
        } else {
            ob::pick_hint(hints(app), &c.facts, HintOn::Commit)
        };
        let mut o = ob::record_commit(&state(app));
        o.hints |= next.as_ref().map_or(0, |h| h.bit);
        save(app, &o);
        let used = TourFacts {
            expr: expr.to_string(),
            unit: c.facts.unit == Some(true),
            chained: c.facts.chained == Some(true),
            solve: c.solve,
            graph: expr.trim().to_lowercase().starts_with("graph"),
        };
        mark(app, core_tour::tour_used(&used));
        if app.model.tour.replay.is_some() && step(app).is_none() {
            app.model.tour.replay = None;
            app.model.hint = Some(core_tour::TOUR_DONE_HINT.into());
        } else if let Some(h) = next {
            app.model.hint = Some(winify(&h.text, &app.model.keybinds));
        }
        refresh(&mut app.model, &app.engine);
    }

    /// The live answer landed (model.rs `set_live`): a hint once it has sat still a while.
    pub fn live_landed(m: &BarModel) {
        later::cancel("pause-hint");
        let typed = m.field.text().to_string();
        let answered = m.live.as_ref().is_some_and(|a| !a.display.is_empty());
        if typed.trim().is_empty() || !answered || m.command.is_some() {
            return;
        }
        later::after("pause-hint", ob::HINT_PAUSE_MS as u32, move |app| {
            if app.model.field.text() != typed || basics_left(app) {
                return;
            }
            let facts = facts_now(&app.model, &typed);
            if let Some(h) = ob::pick_hint(hints(app), &facts, HintOn::Pause) {
                mark_hint(app, h.bit);
                app.model.hint = Some(winify(&h.text, &app.model.keybinds));
                app.model.touch();
            }
        });
    }

    /// The copy shortcut copied an answer: the walkthrough's copy step is done.
    pub fn copied(app: &mut App) {
        mark(app, core_tour::tour_bit(TourId::Copy));
    }

    // ---- the empty bar's example and the line under the bar ----

    /// The muted text in the empty field: the example, else None (the field's own placeholder).
    pub fn placeholder(m: &BarModel) -> Option<&str> {
        m.tour.example.as_ref().map(|e| e.expr.as_str())
    }

    /// The example's answer for the answer slot (muted at 70%), while the field is empty.
    pub fn example_answer(m: &BarModel) -> Option<&str> {
        if !m.field.text().is_empty() {
            return None;
        }
        m.tour
            .example
            .as_ref()
            .map(|e| e.answer.as_str())
            .filter(|a| !a.is_empty())
    }

    /// The tour line shows (in the hint's place) when there's no hint.
    pub fn line_shown(m: &BarModel) -> bool {
        m.tour.line.is_some() && m.hint.as_ref().is_none_or(|h| h.is_empty())
    }

    fn skip_rect(gfx: &Gfx, r: Rect) -> Option<Rect> {
        let l = gfx.layout("skip", &gfx.fonts.small, 0.0, r.h)?;
        let w = measure(&l).0;
        Some(Rect::new(r.right() - 44.0 - w, r.y, w, r.h))
    }

    /// `.tour-line`: the step, `n/m` at 70%, and an underlined skip clear of the clear button's corner.
    pub fn paint_line(c: &Canvas, m: &BarModel, r: Rect) {
        let Some(line) = &m.tour.line else { return };
        let t = c.theme;
        let skip = skip_rect(c.gfx, r);
        let right = skip.map_or(r.right() - 44.0, |s| s.x - 8.0);
        let count = format!("{}/{}", line.at, line.of);
        let cw = c
            .gfx
            .layout(&count, &c.gfx.fonts.small, 0.0, r.h)
            .map_or(0.0, |l| measure(&l).0);
        let tw = (right - r.x - cw - 8.0).max(0.0);
        let mut x = r.x;
        if let Some(l) = c.gfx.layout_in(
            &line.text,
            &c.gfx.fonts.small,
            tw,
            r.h,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        ) {
            c.text(&l, x, r.y, t.muted);
            x += measure(&l).0.min(tw) + 8.0;
        }
        if let Some(l) = c.gfx.layout(&count, &c.gfx.fonts.small, 0.0, r.h) {
            c.text(&l, x, r.y, with_alpha(t.muted, 0.7));
        }
        if let (Some(s), Some(l)) = (skip, c.gfx.layout("skip", &c.gfx.fonts.small, 0.0, r.h)) {
            // SAFETY: a setter on the live layout
            let _ = unsafe {
                l.SetUnderline(
                    true,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: 4,
                    },
                )
            };
            c.text(&l, s.x, s.y, t.muted);
        }
    }

    /// A click on the line's skip.
    pub fn click(app: &mut App, x: f32, y: f32) -> bool {
        let Some(r) = app.bar.layout.tour else {
            return false;
        };
        let hit = skip_rect(&app.gfx, r).is_some_and(|s| s.inset(-4.0, -2.0).contains(x, y));
        if hit {
            skip(app);
        }
        hit
    }

    /// The line's height in the bar (the hint's: 18, tucked 12 into the composer).
    pub const LINE_H: f32 = 18.0;
    pub const _GUTTER: f32 = size::GUTTER;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn mac_keys_read_as_windows_chords() {
        let k = Keybinds::new(&BTreeMap::new());
        assert_eq!(
            winify("⌘C copies the answer", &k),
            "Ctrl+C copies the answer"
        );
        assert_eq!(
            winify("degrees · ⌃D for radians", &k),
            "degrees · Ctrl+D for radians"
        );
        assert_eq!(winify("⌘, opens settings", &k), "Ctrl+, opens settings");
        assert_eq!(
            winify("⌥↑ ⌥↓ step the unit prefix", &k),
            "Alt+↑ Alt+↓ step the unit prefix"
        );
        assert_eq!(winify("12 * 7", &k), "12 * 7");
        let mine = Keybinds::new(&BTreeMap::from([(
            "copyAnswer".to_string(),
            "ctrl+alt+c".to_string(),
        )]));
        assert_eq!(
            winify("⌘C copies the answer: type 2^10, then ⌘C", &mine),
            "Ctrl+Alt+C copies the answer: type 2^10, then Ctrl+Alt+C"
        );
    }

    #[test]
    fn the_walkthrough_runs_its_steps_in_order() {
        let fresh = core_tour::enrol_tour(&ob::record_open(&Onboarding::default()), true);
        let s = step_now(&fresh, None, "Alt+Space").unwrap();
        assert_eq!(s.id, TourId::Hotkey);
        let all_basics = core_tour::tour_steps("x").iter().fold(0, |a, s| a | s.bit);
        let done = Onboarding {
            hints: fresh.hints | all_basics,
            ..fresh.clone()
        };
        assert!(step_now(&done, None, "Alt+Space").is_none());
        // a replay starts over whatever was done
        assert_eq!(
            step_now(&done, Some(0), "Alt+Space").map(|s| s.id),
            Some(TourId::Hotkey)
        );
        let store = crate::store::Onboarding {
            opens: 2,
            commits: 1,
            hints: 16,
            done: false,
        };
        let o = onboarding_of(&store);
        assert_eq!((o.opens, o.commits, o.hints), (2, 1, 16));
        assert_eq!(stored(&o), store);
    }
}
