//! The tape (history) above the line: its view, its keys and clicks, and its persistence.
//!
//! Persistence goes through the engine, off the UI thread (Lane::Misc): `load` at launch (`Engine::load_history`
//! reads `<config dir>\history.json`, migrating legacy rows) and `save` after every change
//! (`Engine::save_history` writes what `persistable_history` keeps). The model holds the rows as `engine::Rows`.
//!
//! The view (`HistoryTape.tsx`, `HistoryTape` in BarView.swift): rows of 36 DIPs, at most 5 in sight, the tape's
//! own colour and a hairline under it, scrolled by the wheel or the touchpad. Each row is the expression (13,
//! muted) left and its answers (17) right in up to 46% of the width, as cells (`cells`): a solve's letter (copies
//! every root) and each root (inserts itself), `exact ≈ approx` (each inserts its side), or the one answer
//! (matrices, complex numbers and lists are the engine's text). A system row's expression reopens its panel
//! (through `BarModel::insert_row`, where systems.rs hooks in).
//!
//! Keys (from model.rs): ↑ ↓ walk the rows and keep to the side ← → last chose (`land`), ← → step through the
//! row's picks (`historyPicks`: the expression, then each answer), ↵ inserts the pick, Alt+↵ the other half
//! (`enter_row`), Ctrl+Backspace deletes the row and Ctrl+Z puts it back. Text layouts are cached per row, so a
//! keystroke redraws the tape without making any.

use std::cell::RefCell;
use std::ops::Range;

use qcalc_core::app::answer::{self as core_answer, AnswerForm};
use qcalc_ffi::{HistoryKind, HistoryRow, SolveOutcome};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
use windows::Win32::Graphics::DirectWrite::{IDWriteTextFormat, IDWriteTextLayout};
use windows::Win32::Graphics::Gdi::ScreenToClient;

use crate::app::App;
use crate::engine::{EngineThread, Lane, Rows, done};
use crate::model::{BarModel, now_ms};
use crate::qlog;
use crate::render::{Canvas, Gfx, Rect, measure, size, with_alpha};

/// Loads the tape at launch; the rows land in the model through `App::set_history_loaded`.
pub fn load(engine: &EngineThread) {
    engine.submit(Lane::Misc, |cx| {
        let t = std::time::Instant::now();
        let rows = cx.engine.load_history();
        qlog!(
            "history: {} rows loaded in {:.1} ms",
            rows.len(),
            t.elapsed().as_secs_f64() * 1000.0
        );
        done(move |app| app.set_history_loaded(rows))
    });
}

/// Writes the tape (the engine slims it and caps it).
pub fn save(engine: &EngineThread, rows: Vec<HistoryRow>) {
    engine.submit(Lane::Misc, move |cx| {
        if let Err(e) = cx.engine.save_history(rows) {
            qlog!("history: save failed: {e}");
        }
        None
    });
}

/// Tape padding (6 8) and its hairline.
pub const PAD_Y: f32 = 6.0;
pub const PAD_X: f32 = 8.0;

// ---- state ----

/// The tape's own state on the model (`BarModel.tape`).
#[derive(Clone, Debug, Default)]
pub struct TapeState {
    /// on the highlighted row, what ← → picked: 0 the expression, 1… its answers (historyPicks)
    pub pick: usize,
    /// ↑ ↓ keep to the side ← → last chose (historyPickOn)
    side_expr: bool,
    /// the row Ctrl+Backspace took, for Ctrl+Z, while the tape is still the one it came out of
    deleted: Option<(HistoryRow, usize, u64)>,
    /// DIPs the tape is scrolled up from its newest row
    pub scroll: f32,
    /// a wheel notch's glide: from, to, and when it started (`wheel`, `glide_step`)
    glide: Option<(f32, f32, std::time::Instant)>,
    /// `?` / `help`: the cheat sheet shows where the tape is (cheats.rs)
    pub help: bool,
    /// the global shortcut, for the cheat sheet's first row
    pub hotkey: String,
    last_text: String,
    /// the row and pick a Shift-click or Shift+↵ just put in, still picked: Backspace then edits the line
    /// (`tape_rules::delete_edits_line`, the Mac's `keptInsert`)
    kept: Option<(usize, usize)>,
    /// the row whose copy button shows its check (1.2 s after the last click on it), by row id
    pub check: Option<String>,
}

/// The pick ← → stand on, the row's answers each way they insert and copy (the core's `row_answers`), cached per
/// row and the settings it reads, so a paint or a key makes no new ones.
pub fn row_answers(m: &BarModel, row: &HistoryRow) -> core_answer::RowAnswers {
    type Key = (String, String, Option<String>, bool, u32, bool);
    thread_local! {
        static CACHE: RefCell<Vec<(Key, core_answer::RowAnswers)>> = const { RefCell::new(Vec::new()) };
    }
    let key: Key = (
        row.id.clone(),
        row.display.clone(),
        row.exact.clone(),
        m.settings.answer_form == "approx",
        figs(m) as u32,
        m.settings.copy_unitless,
    );
    if let Some(hit) = CACHE.with(|c| {
        c.borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
    }) {
        return hit;
    }
    let got = core_answer::row_answers(&json(row), form(m), figs(m), m.settings.copy_unitless);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= 64 {
            c.remove(0);
        }
        c.push((key, got.clone()));
    });
    got
}

/// The answer a row reads as (`answers[start-1]`), None when it has none (a note, system or function).
pub fn read_answer(m: &BarModel, row: &HistoryRow) -> Option<core_answer::RowAnswer> {
    let a = row_answers(m, row);
    (a.start > 0 && a.start <= a.answers.len()).then(|| a.answers[a.start - 1].clone())
}

/// What a row's copy button and Copy Answer copy: the answer the row reads as, as shown (the unitless setting
/// applied, a reaction in ASCII); a note its text.
pub fn row_copy(m: &BarModel, row: &HistoryRow) -> String {
    if row.kind == Some(HistoryKind::Note) {
        return row.expr.clone();
    }
    read_answer(m, row).map_or_else(
        || qcalc_core::app::touches::answer_copy(&row.display, m.settings.copy_unitless),
        |a| a.copy,
    )
}

/// The "Insert … at the cursor" tips on tape rows stop for good once the user has inserted from the tape (the
/// Mac's `TapeTips`, `qcalc.tape.insertLearned`): a marker file beside settings.json, read once.
pub mod tips {
    use std::cell::Cell;

    thread_local! {
        static LEARNED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::store::config_dir().map(|d| d.join("tape-insert-learned"))
    }

    pub fn learned() -> bool {
        LEARNED.with(|l| {
            if l.get().is_none() {
                l.set(Some(path().is_some_and(|p| p.exists())));
            }
            l.get() == Some(true)
        })
    }

    /// The first insert from the tape: the tips go (written off the UI thread).
    pub fn learn() {
        if learned() {
            return;
        }
        LEARNED.with(|l| l.set(Some(true)));
        if let Some(p) = path() {
            std::thread::spawn(move || {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(p, b"1");
            });
        }
    }
}

fn form(m: &BarModel) -> AnswerForm {
    if m.settings.answer_form == "approx" {
        AnswerForm::Approx
    } else {
        AnswerForm::Exact
    }
}

fn figs(m: &BarModel) -> f64 {
    m.settings.sig_figs.clamp(2, 16) as f64
}

fn json(row: &HistoryRow) -> serde_json::Value {
    qcalc_ffi::history::row_json(row)
}

/// The line changed: typing past `?` keeps only the new text (afterHelpInput); the cheat sheet follows the line.
pub fn text_changed(m: &mut BarModel) {
    let now = m.field.text().to_string();
    let next = qcalc_core::app::onboarding::after_help_input(&m.tape.last_text, &now);
    if next != now {
        m.field.set_text(&next);
    }
    m.tape.help = qcalc_core::app::onboarding::is_help_command(&next);
    m.tape.last_text = next;
}

/// The ← → picks of a row after its expression, as each inserts, and the pick ↑ lands on (`row_answers`, the Mac's
/// `picks`).
pub fn picks(m: &BarModel, row: &HistoryRow) -> core_answer::HistoryPicks {
    let a = row_answers(m, row);
    core_answer::HistoryPicks {
        answers: a.answers.into_iter().map(|a| a.insert).collect(),
        start: a.start,
    }
}

/// What a screen reader hears for a row (↑ ↓ landing on it, the UI Automation list item).
pub fn spoken(row: &HistoryRow) -> String {
    crate::tape_rules::spoken(&row.expr, &row.display, row.kind == Some(HistoryKind::Note))
}

/// ↑ ↓ landed on row `selected`: the pick on the side last chosen, and the row in sight.
pub fn land(m: &mut BarModel, fresh: bool) {
    if fresh {
        m.tape.side_expr = false;
    }
    m.tape.help = false;
    m.tape.kept = None;
    let Some(i) = m.selected else { return };
    let Some(row) = m.rows.rows.get(i).cloned() else {
        return;
    };
    m.tape.pick = if m.tape.side_expr {
        0
    } else {
        picks(m, &row).start
    };
    // the row in sight: scroll the least that shows it
    let visible = m.visible(now_ms());
    if visible.contains(&i) {
        m.tape.glide = None;
        let k = visible.len().min(size::TAPE_VISIBLE) as f32;
        let from_bottom = (visible.end - 1 - i) as f32 * size::TAPE_ROW;
        let s = &mut m.tape.scroll;
        if from_bottom < *s {
            *s = from_bottom;
        }
        if from_bottom + size::TAPE_ROW > *s + k * size::TAPE_ROW {
            *s = from_bottom + size::TAPE_ROW - k * size::TAPE_ROW;
        }
    }
}

/// ← → on a highlighted row (onSide).
pub fn pick_side(m: &mut BarModel, dir: i32) {
    let Some(row) = m.selected.and_then(|i| m.rows.rows.get(i)).cloned() else {
        return;
    };
    m.tape.kept = None;
    let n = picks(m, &row).answers.len() as i32;
    let next = (m.tape.pick as i32 + dir).clamp(0, n) as usize;
    m.tape.side_expr = next == 0;
    m.tape.pick = next;
    m.touch();
}

/// A piece into the line at the caret; the tape closes.
pub fn insert_piece(m: &mut BarModel, piece: &str) {
    m.selected = None;
    m.tape_open = false;
    m.field.insert(piece);
    m.field.break_run();
    m.touch();
    crate::matrices::inserted(piece);
}

/// An answer from the tape among other text on the line keeps its value (the page's insertPlain → answerAmong):
/// `5` then a root `-2` is `5(-2)`, not `5-2`; `±2` is `(±2)`. Alone on the line, or a word's value, as it is.
fn among(m: &BarModel, piece: &str, row: &HistoryRow) -> String {
    if row.kind == Some(HistoryKind::Definition)
        || crate::tape_rules::answer_alone(m.field.text(), m.field.selected_text())
    {
        return piece.to_string();
    }
    core_answer::answer_among(piece, false)
}

/// A piece from row `selected` goes in. Shift (`keep`) keeps the tape open on the row and its pick, so more can
/// follow, and a plain Backspace then edits the line (`kept`).
fn finish_insert(m: &mut BarModel, engine: &EngineThread, piece: &str, keep: bool) {
    if piece.is_empty() {
        return;
    }
    tips::learn();
    let (sel, pick, side) = (m.selected, m.tape.pick, m.tape.side_expr);
    insert_piece(m, piece);
    m.text_changed(engine);
    m.tape.kept = None;
    if keep && let Some(sel) = sel.filter(|&s| s < m.rows.rows.len()) {
        m.tape_open = true;
        m.selected = Some(sel);
        m.tape.pick = pick;
        m.tape.side_expr = side;
        m.tape.kept = Some((sel, pick));
        m.touch();
    }
}

/// A click on a row's half (clicks insert the side they hit): its expression (`answer` false), or answer `at` (1…,
/// the row's `row_answers`; None: the one the row reads as) at full precision with its unit; Alt (`bare`) leaves the
/// unit off, Shift (`keep`) keeps the tape open. `shown` is what goes in when the core has no answer for it (a
/// complex solve's roots).
#[allow(clippy::too_many_arguments)]
pub fn insert_from_row(
    m: &mut BarModel,
    engine: &EngineThread,
    i: usize,
    answer: bool,
    at: Option<usize>,
    shown: Option<&str>,
    keep: bool,
    bare: bool,
) {
    let Some(row) = m.rows.rows.get(i).cloned() else {
        return;
    };
    if row.kind == Some(HistoryKind::System) {
        m.insert_row(i, false);
        m.text_changed(engine);
        return;
    }
    if keep && m.selected != Some(i) {
        // a Shift-click keeps the tape open on the row it came from
        m.selected = Some(i);
        m.tape_open = true;
        m.tape.pick = 0;
    }
    if !answer {
        if keep {
            m.tape.pick = 0;
            m.tape.side_expr = true;
        }
        return finish_insert(m, engine, &row.expr, keep);
    }
    let got = row_answers(m, &row);
    let n = at.unwrap_or(got.start);
    if n > 0 && n <= got.answers.len() {
        let a = &got.answers[n - 1];
        let piece = among(m, if bare { &a.bare } else { &a.insert }, &row);
        if keep {
            m.tape.pick = n;
            m.tape.side_expr = false;
        }
        return finish_insert(m, engine, &piece, keep);
    }
    // no answer from the core (a complex solve…): as shown
    let piece = match shown {
        Some(s) => s.to_string(),
        None if row.kind == Some(HistoryKind::Definition) => row.expr.clone(),
        None if m.settings.answer_form == "exact" => {
            row.exact.clone().unwrap_or_else(|| row.display.clone())
        }
        None => row.display.clone(),
    };
    if piece.is_empty() {
        return;
    }
    let piece = among(m, &piece, &row);
    finish_insert(m, engine, &piece, keep);
}

/// ↵ on the highlighted row inserts what ← → picked (insertHistoryPick); Alt+↵ the other half
/// (insertHistoryOther). A system row reopens its equations.
pub fn enter_row(m: &mut BarModel, engine: &EngineThread, alt: bool) {
    enter_row_with(m, engine, alt, false, false);
}

/// `enter_row`, and Shift (`keep`) keeps the tape open on the row, Ctrl (`bare`) inserts the picked answer
/// without its unit.
fn enter_row_with(m: &mut BarModel, engine: &EngineThread, alt: bool, keep: bool, bare: bool) {
    let Some(i) = m.selected else { return };
    let Some(row) = m.rows.rows.get(i).cloned() else {
        return;
    };
    // a system row goes through the model's insert_row, whose systems hook reopens its panel
    if row.kind == Some(HistoryKind::System) {
        m.insert_row(i, false);
        m.text_changed(engine);
        return;
    }
    if alt {
        let expr = matches!(
            row.kind,
            Some(HistoryKind::Definition | HistoryKind::Function)
        ) || m.settings.history_insert == "answer";
        return insert_from_row(m, engine, i, !expr, None, None, keep, bare);
    }
    let answers = row_answers(m, &row).answers;
    if m.tape.pick > 0 && !answers.is_empty() {
        let a = &answers[m.tape.pick.min(answers.len()) - 1];
        let piece = among(m, if bare { &a.bare } else { &a.insert }, &row);
        finish_insert(m, engine, &piece, keep);
    } else {
        insert_from_row(m, engine, i, false, None, None, keep, bare);
    }
}

/// A row's Delete (Ctrl+Backspace, Backspace or Delete on a picked row, its context menu); Ctrl+Z brings it back.
/// The pick moves as `tape_rules` says, with ← → landed again for the row it moves to; the tape stays where it is.
pub fn delete_row(m: &mut BarModel, engine: &EngineThread, i: usize) -> bool {
    let mut next = m.rows.rows.to_vec();
    if i >= next.len() {
        return false;
    }
    let row = next.remove(i);
    m.rows = Rows::new(next.clone());
    m.tape.deleted = Some((row, i, m.rows.generation));
    m.tape.kept = None;
    m.selected = crate::tape_rules::selection_after_delete(i, m.selected, next.len());
    if m.selected.is_some() {
        land(m, false);
    } else if next.is_empty() {
        m.tape_open = false;
    }
    m.touch();
    save(engine, next);
    m.evaluate_live(engine);
    true
}

/// Ctrl+Backspace on a highlighted row: it goes (saved), and Ctrl+Z can put it back.
pub fn delete_selected(m: &mut BarModel, engine: &EngineThread) -> bool {
    let Some(i) = m.selected else { return false };
    delete_row(m, engine, i)
}

/// Ctrl+Z after a delete (and nothing saved since): the row comes back where it was, picked only while the tape is
/// being walked (a menu Delete's undo leaves the line's keys alone).
pub fn undo_delete(m: &mut BarModel, engine: &EngineThread) -> bool {
    let Some((row, i, generation)) = m.tape.deleted.take() else {
        return false;
    };
    if generation != m.rows.generation {
        return false;
    }
    let pick_it = crate::tape_rules::selects_restored(m.tape_open, m.selected);
    let mut next = m.rows.rows.to_vec();
    let at = i.min(next.len());
    next.insert(at, row);
    m.rows = Rows::new(next.clone());
    if pick_it {
        m.tape_open = true;
        m.selected = Some(at);
        land(m, false);
    }
    m.touch();
    save(engine, next);
    m.evaluate_live(engine);
    true
}

/// The tape's keys on a picked row, before the line's (the Mac's `key`): ↵ with Shift / Ctrl / Alt, Ctrl+Alt+C,
/// Ctrl+↓, and Backspace / Delete. None when the key isn't one of them, or no row is picked.
pub fn key_down(
    m: &mut BarModel,
    vk: u32,
    mods: crate::keys::Mods,
    engine: &EngineThread,
) -> Option<crate::model::Outcome> {
    use crate::model::Outcome;
    const BACK: u32 = 0x08;
    const ENTER: u32 = 0x0D;
    const DOWN: u32 = 0x28;
    const DELETE: u32 = 0x2E;
    let i = m.selected.filter(|&i| i < m.rows.rows.len())?;
    let plain = !mods.ctrl && !mods.alt && !mods.shift && !mods.win;
    let only_ctrl = mods.ctrl && !mods.alt && !mods.shift && !mods.win;
    match vk {
        // ↵ with Shift (keep the tape open), Ctrl (without the unit) and Alt (the other half), in any mix
        ENTER if !mods.win => {
            enter_row_with(m, engine, mods.alt, mods.shift, mods.ctrl);
            Some(Outcome::Handled)
        }
        // Ctrl+Alt+C: the pick at full precision
        0x43 if mods.ctrl && mods.alt && !mods.shift => {
            copy_selected(m, true);
            Some(Outcome::Handled)
        }
        // Ctrl+↓: straight back to the line
        DOWN if only_ctrl => {
            m.selected = None;
            m.tape.kept = None;
            m.touch();
            Some(Outcome::Handled)
        }
        BACK | DELETE if plain || (only_ctrl && vk == BACK) => {
            // right after a Shift-insert from this row, a plain one edits the line
            if plain && crate::tape_rules::delete_edits_line(m.tape.kept, m.selected, m.tape.pick) {
                m.tape.kept = None;
                m.selected = None;
                m.touch();
                return None;
            }
            delete_row(m, engine, i);
            Some(Outcome::Handled)
        }
        _ => None,
    }
}

/// Ctrl+C on a row picked with ↑: what ← → picked there, the expression or an answer's copy (a note's text, a
/// system's solution); Ctrl+Alt+C (`full`) the answer at full precision. False when no row is picked.
pub fn copy_selected(m: &mut BarModel, full: bool) -> bool {
    let Some(row) = m.selected.and_then(|i| m.rows.rows.get(i)).cloned() else {
        return false;
    };
    let answers = row_answers(m, &row).answers;
    let text = if m.tape.pick > 0 && !answers.is_empty() {
        let a = &answers[m.tape.pick.min(answers.len()) - 1];
        if full {
            a.copy_full.clone()
        } else {
            a.copy.clone()
        }
    } else if row.kind == Some(HistoryKind::System) {
        row.display.clone()
    } else {
        row.expr.clone()
    };
    copy_text(m, text);
    true
}

/// Ctrl+Shift+C with a row picked: its own line (the core's `row_line_copy`). None with no row picked.
pub fn line_copy(m: &BarModel) -> Option<String> {
    let row = m.selected.and_then(|i| m.rows.rows.get(i))?;
    Some(qcalc_core::app::touches::row_line_copy(&json(row), form(m)))
}

/// Text to the clipboard as is, with the flash.
pub fn copy_text(m: &mut BarModel, text: String) {
    if text.is_empty() {
        return;
    }
    m.to_clipboard = Some(text);
    crate::answer::flash(m);
}

/// A row's copy button: the answer it reads as (a note's text); its icon turns to a check for 1.2 s after the
/// last click. The line keeps the keys.
pub fn copy_button(m: &mut BarModel, i: usize) {
    let Some(row) = m.rows.rows.get(i).cloned() else {
        return;
    };
    let text = row_copy(m, &row);
    copy_text(m, text);
    m.tape.check = Some(row.id.clone());
    crate::later::after("tape-copy-check", crate::answer::COPIED_MS, |app| {
        app.model.tape.check = None;
        app.model.touch();
    });
}

/// ↵ on a line with no answer: it is kept as typed, a note row (never `ans`, a variable or a function: the core
/// skips `note` rows), and the hint says so. Not for a `graph…` line, whose hint says how to finish it.
pub fn save_note(m: &mut BarModel, engine: &EngineThread, typed: &str) -> bool {
    if typed.trim().is_empty() || typed.trim_start().to_lowercase().starts_with("graph") {
        return false;
    }
    let now = now_ms();
    let row = HistoryRow {
        id: format!("{}-note", radix36(now.max(0) as u64)),
        expr: typed.to_string(),
        display: String::new(),
        exact: None,
        n: None,
        meas: None,
        quantity: None,
        kind: Some(HistoryKind::Note),
        equations: None,
        fn_name: None,
        fn_params: None,
        fn_body: None,
        solve: None,
        at: Some(now),
    };
    let mut next = m.rows.rows.to_vec();
    next.push(row);
    m.rows = Rows::new(next.clone());
    m.tape.deleted = None;
    save(engine, next);
    m.field.reset("");
    m.text_changed(engine);
    m.hint = Some("saved as a note".into());
    m.touch();
    true
}

fn radix36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The clear-history shortcut: the hint says so, and goes after 2 s unless something else took the line since.
pub fn cleared(m: &mut BarModel) {
    m.tape.deleted = None;
    m.tape.kept = None;
    m.hint = Some("history cleared".into());
    crate::later::after("history-cleared", 2000, |app| {
        if app.model.hint.as_deref() == Some("history cleared") {
            app.model.hint = None;
            app.model.touch();
        }
    });
}

/// Quitting (or the session ending): a history save still queued is written before the app goes, not dropped.
/// Waits (at most 3 s) for the engine thread to reach a marker queued after it.
pub fn flush(engine: &EngineThread) {
    let (tx, rx) = std::sync::mpsc::channel();
    engine.submit(Lane::Misc, move |_| {
        let _ = tx.send(());
        None
    });
    let _ = rx.recv_timeout(std::time::Duration::from_secs(3));
}

/// A scroll up over the bar opens the whole history, no row picked (↑ picks the newest); a scroll down past the
/// newest row closes a history a scroll opened. True when the scroll did that.
fn scroll_open_close(m: &mut BarModel, up: bool) -> bool {
    if m.settings.history_show == "always" || crate::commands::games_shown(m) {
        return false;
    }
    if up && !m.tape_open && !m.rows.rows.is_empty() {
        m.tape.help = false;
        m.tape_open = true;
        m.selected = None;
        m.tape.scroll = 0.0;
        m.tape.glide = None;
        m.touch();
        return true;
    }
    let at_newest = m.tape.scroll <= 0.01 && m.tape.glide.is_none_or(|g| g.1 <= 0.01);
    if !up && m.tape_open && m.selected.is_none() && at_newest {
        m.tape_open = false;
        m.touch();
        return true;
    }
    false
}

// ---- cells ----

/// What a cell does when clicked.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    None,
    /// the expression (a note's text) goes in at the caret
    Expr,
    /// answer `at` (1…) of the row's `row_answers` goes in at full precision; the text is what goes in when the core
    /// has none for it (a complex solve's roots, a solve's message: nothing)
    Answer(usize, String),
    Copy(String),
    OpenSystem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// 13 muted, ink when picked
    Expr,
    /// 17, the answer colour
    Answer,
    /// 13 muted: a solve's letter, the ≈, a message
    Quiet,
}

/// One part of a row, left to right: the expression first, then its answers.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub look: Look,
    pub act: Act,
    /// which ← → pick it is
    pub pick: Option<usize>,
}

fn cell(text: impl Into<String>, look: Look, act: Act, pick: Option<usize>) -> Cell {
    Cell {
        text: text.into(),
        look,
        act,
        pick,
    }
}

/// The row as the web's HistoryTape draws it. A note is its text alone, across the row.
pub fn cells(row: &HistoryRow, form: AnswerForm, figs: f64) -> Vec<Cell> {
    let j = json(row);
    let system = row.kind == Some(HistoryKind::System);
    let mut out = vec![cell(
        row.expr.replace("sqrt(", "√("),
        Look::Expr,
        if system { Act::OpenSystem } else { Act::Expr },
        Some(0),
    )];
    if row.kind == Some(HistoryKind::Note) {
        return out;
    }
    let solved_for = row
        .solve
        .as_ref()
        .filter(|s| matches!(s.outcome, SolveOutcome::Roots | SolveOutcome::Complex))
        .map(|s| format!("{} =", s.variable));
    let roots = core_answer::solve_roots(&j, figs);
    let root_cells = |out: &mut Vec<Cell>, lead: bool| {
        let roots = roots.clone().unwrap_or_default();
        if lead && let Some(s) = &row.solve {
            out.push(cell(
                format!("{} =", s.variable),
                Look::Quiet,
                Act::Copy(row.display.clone()),
                None,
            ));
        }
        // each root its own button (the page's RootPicks): as the row lists them, else at the row's figures
        let labels = crate::tape_rules::root_labels(&row.display, &roots);
        for (k, root) in roots.iter().enumerate() {
            if k > 0 {
                out.push(cell(", ", Look::Answer, Act::None, None));
            }
            out.push(cell(
                roots_text(&labels[k]),
                Look::Answer,
                Act::Answer(k + 1, root.clone()),
                Some(k + 1),
            ));
        }
        if row.solve.as_ref().is_some_and(|s| s.more) {
            out.push(cell(", …", Look::Answer, Act::None, None));
        }
    };
    if core_answer::row_has_dual_answer(&j)
        && let Some(exact) = &row.exact
    {
        if let Some(s) = &solved_for {
            out.push(cell(s, Look::Quiet, Act::None, None));
        }
        let insert_exact = core_answer::solve_insert(&j, figs)
            .unwrap_or_else(|| core_answer::insertable_answer(exact, None, 12.0));
        let exact_text = if row.solve.is_some() {
            roots_text(exact)
        } else {
            crate::answer::pretty_text(exact)
        };
        // with roots after the ≈, the exact side inserts the first root, as on the page
        out.push(cell(
            exact_text,
            Look::Answer,
            Act::Answer(1, insert_exact),
            roots.is_none().then_some(1),
        ));
        out.push(cell("≈", Look::Quiet, Act::None, None));
        if roots.is_some() {
            root_cells(&mut out, false);
        } else {
            let approx = if row.solve.is_some() {
                roots_text(&row.display)
            } else {
                crate::answer::pretty_text(&row.display)
            };
            out.push(cell(
                approx,
                Look::Answer,
                Act::Answer(
                    2,
                    core_answer::insertable_history_answer(&j, AnswerForm::Approx, figs),
                ),
                Some(2),
            ));
        }
        return out;
    }
    if roots.is_some() {
        root_cells(&mut out, true);
        return out;
    }
    // one answer: a definition's word, a solve's list or message as written, else the pretty answer
    let shown = core_answer::visible_answer(&row.display, row.exact.as_deref(), form);
    let message = row
        .solve
        .as_ref()
        .is_some_and(|s| !matches!(s.outcome, SolveOutcome::Roots | SolveOutcome::Complex));
    let text = if row.kind == Some(HistoryKind::Definition) {
        shown.to_string()
    } else if row.solve.is_some() {
        roots_text(shown)
    } else {
        crate::answer::pretty_text(shown)
    };
    if let Some(s) = &solved_for {
        out.push(cell(s, Look::Quiet, Act::None, None));
    }
    let fallback = if row.kind == Some(HistoryKind::Definition) {
        row.expr.clone()
    } else {
        core_answer::insertable_history_answer(&j, form, figs)
    };
    if !text.is_empty() && !system {
        out.push(cell(
            text,
            if message { Look::Quiet } else { Look::Answer },
            Act::Answer(1, fallback),
            Some(1),
        ));
    }
    out
}

fn roots_text(s: &str) -> String {
    crate::answer::radicals_if_math(&core_answer::pretty_roots(s))
}

/// The copy button's place among a row's cells (`cell_at`, hover.rs's spot).
pub const COPY_CELL: usize = usize::MAX;

// ---- layout ----

/// Where the tape is: the rows in the view, the ones drawn, and the scroll.
#[derive(Clone, Debug, Default)]
pub struct TapeLayout {
    /// every row the view shows (the recent ones, or all)
    pub visible: Range<usize>,
    /// the rows at least partly in sight
    pub drawn: Range<usize>,
    pub scroll: f32,
    pub height: f32,
}

/// The tape for the rows the view shows: at most 5 in sight, scrolled.
pub fn layout(model: &BarModel, visible: Range<usize>) -> TapeLayout {
    let n = visible.len();
    let k = n.min(size::TAPE_VISIBLE);
    if k == 0 {
        return TapeLayout {
            drawn: visible.end..visible.end,
            visible,
            ..TapeLayout::default()
        };
    }
    let max = (n - k) as f32 * size::TAPE_ROW;
    let scroll = model.tape.scroll.clamp(0.0, max);
    // rows from the bottom: the window is [scroll, scroll + k rows)
    let first_from_bottom = (scroll / size::TAPE_ROW).floor() as usize;
    let last_from_bottom = ((scroll + k as f32 * size::TAPE_ROW) / size::TAPE_ROW).ceil() as usize;
    let end = visible.end - first_from_bottom.min(n);
    let start = visible
        .end
        .saturating_sub(last_from_bottom)
        .max(visible.start);
    TapeLayout {
        drawn: start..end,
        scroll,
        height: height(k),
        visible,
    }
}

/// The rows to draw (at most 5, the selected one in sight) of the visible range, unscrolled.
pub fn shown(model: &BarModel, visible: Range<usize>) -> Range<usize> {
    layout(model, visible).drawn
}

/// The tape's height for `rows` shown rows (0 for none).
pub fn height(rows: usize) -> f32 {
    if rows == 0 {
        0.0
    } else {
        rows as f32 * size::TAPE_ROW + 2.0 * PAD_Y + 1.0
    }
}

/// Where row `i` is, scrolled (it may be partly out of the window).
pub fn row_rect(width: f32, t: &TapeLayout, i: usize) -> Rect {
    let k = t.visible.len().min(size::TAPE_VISIBLE) as f32;
    let bottom = PAD_Y + k * size::TAPE_ROW;
    let y = bottom - (t.visible.end - i) as f32 * size::TAPE_ROW + t.scroll;
    Rect::new(PAD_X, y, width - 2.0 * PAD_X, size::TAPE_ROW)
}

fn window(t: &TapeLayout) -> Rect {
    let k = t.visible.len().min(size::TAPE_VISIBLE) as f32;
    Rect::new(0.0, PAD_Y, f32::MAX, k * size::TAPE_ROW)
}

/// A row ready to draw: its cells' layouts, placed in a row of the cached width.
#[derive(Clone)]
struct RowView {
    key: (String, String, String, Option<String>, bool, u32, u32),
    /// (cell, its layout, x from the row's left, width)
    cells: Vec<(Cell, IDWriteTextLayout, f32, f32)>,
    /// the expression drawn stacked, while "stacked fractions and powers" is on (stacked_text.rs)
    stacked: Option<std::rc::Rc<crate::stacked_text::Picture>>,
}

thread_local! {
    static ROWS: RefCell<Vec<RowView>> = const { RefCell::new(Vec::new()) };
}

const ROW_CACHE: usize = 48;

/// Drops the cached row layouts (the hidden trim).
pub fn trim() {
    ROWS.with(|r| r.borrow_mut().clear());
}

fn format_of(gfx: &Gfx, look: Look) -> &IDWriteTextFormat {
    match look {
        Look::Answer => &gfx.fonts.row,
        Look::Expr | Look::Quiet => &gfx.fonts.body,
    }
}

/// The copy button at a row's right end: 22 square, 4 in from the end, 12 after the answers (the Mac's
/// `RowCopyButton`); it always takes its room so the row doesn't shift.
const COPY_W: f32 = 22.0;
const COPY_RESERVE: f32 = 4.0 + COPY_W + 12.0;

/// The copy button's box in a row of width `w` (from the row's left).
fn copy_rect(w: f32) -> Rect {
    Rect::new(
        w - 4.0 - COPY_W,
        (size::TAPE_ROW - COPY_W) / 2.0,
        COPY_W,
        COPY_W,
    )
}

/// .tape-row grid: the expression in `1fr`, the answers in minmax(120px, 46%), 12 apart, 8 in, then the copy
/// button. A note's text takes the row.
fn build(gfx: &Gfx, m: &BarModel, row: &HistoryRow, w: f32) -> Option<RowView> {
    let cs = cells(row, form(m), figs(m));
    let inner = w - 8.0 - COPY_RESERVE;
    let col_max = ((w - 16.0) * 0.46).max(120.0);
    let gap = 6.0;
    // the answers, right to left, natural widths; the first (leftmost) one cut when they don't fit
    let mut answers: Vec<(Cell, IDWriteTextLayout, f32)> = Vec::new();
    for c in cs.iter().skip(1) {
        let l = gfx.layout(&c.text, format_of(gfx, c.look), 0.0, size::TAPE_ROW)?;
        let lw = measure(&l).0;
        answers.push((c.clone(), l, lw));
    }
    // a root list's commas sit tight against the roots
    let gaps: Vec<f32> = (0..answers.len())
        .map(|k| {
            let tight = |i: usize| answers[i].0.text.starts_with(',');
            if k == 0 || tight(k) {
                // nothing before a comma: it sits on its root as written ("−2, 2")
                0.0
            } else if tight(k - 1) {
                // the comma's own space is the gap after it
                1.0
            } else {
                gap
            }
        })
        .collect();
    let natural: f32 = answers.iter().map(|a| a.2).sum::<f32>() + gaps.iter().sum::<f32>();
    if natural > col_max && answers.len() == 1 {
        let c = &answers[0].0;
        let l = gfx.layout_in(
            &c.text,
            format_of(gfx, c.look),
            col_max,
            size::TAPE_ROW,
            windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING,
        )?;
        answers[0].1 = l;
        answers[0].2 = col_max;
    }
    let used: f32 = answers.iter().map(|a| a.2).sum::<f32>() + gaps.iter().sum::<f32>();
    let col = used.clamp(if answers.is_empty() { 0.0 } else { 120.0 }, col_max);
    let mut out = Vec::new();
    let gap_after_expr = if answers.is_empty() { 0.0 } else { 12.0 };
    let expr_w = (inner - col - gap_after_expr).max(0.0);
    let e = &cs[0];
    let el = gfx.layout_in(
        &e.text,
        format_of(gfx, e.look),
        expr_w,
        size::TAPE_ROW,
        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
    )?;
    // stacked as the bar draws it while the setting is on
    let stacked = (m.settings.stacked_math && crate::stacked_text::stacks(&row.expr))
        .then(|| std::rc::Rc::new(crate::stacked_text::picture(gfx, &e.text, size::BODY)));
    let ew = stacked
        .as_ref()
        .map_or_else(|| measure(&el).0, |p| p.w)
        .min(expr_w);
    out.push((e.clone(), el, 8.0, ew));
    let mut x = 8.0 + inner;
    let mut placed: Vec<(Cell, IDWriteTextLayout, f32, f32)> = Vec::new();
    for ((c, l, lw), g) in answers.into_iter().zip(gaps).rev() {
        x -= lw;
        placed.push((c, l, x, lw));
        x -= g;
    }
    placed.reverse();
    out.extend(placed);
    Some(RowView {
        key: key_of(m, row, w),
        cells: out,
        stacked,
    })
}

fn key_of(
    m: &BarModel,
    row: &HistoryRow,
    w: f32,
) -> (String, String, String, Option<String>, bool, u32, u32) {
    (
        row.id.clone(),
        row.expr.clone(),
        row.display.clone(),
        row.exact.clone(),
        m.settings.answer_form == "approx",
        (figs(m) as u32) << 2
            | u32::from(m.settings.history_insert == "answer")
            | u32::from(m.settings.stacked_math) << 1,
        w.to_bits(),
    )
}

fn view(gfx: &Gfx, m: &BarModel, row: &HistoryRow, w: f32) -> Option<RowView> {
    let key = key_of(m, row, w);
    if let Some(v) = ROWS.with(|r| r.borrow().iter().find(|v| v.key == key).cloned()) {
        return Some(v);
    }
    let v = build(gfx, m, row, w)?;
    ROWS.with(|r| {
        let mut r = r.borrow_mut();
        if r.len() >= ROW_CACHE {
            r.remove(0);
        }
        r.push(v.clone());
    });
    Some(v)
}

/// The cell under a point: its row, the cell's index in the row (None: the row's background; `COPY_CELL`: its copy
/// button) and what it does.
pub fn cell_at(
    gfx: &Gfx,
    m: &BarModel,
    width: f32,
    t: &TapeLayout,
    x: f32,
    y: f32,
) -> Option<(usize, Option<usize>, Act)> {
    if !window(t).contains(x, y) {
        return None;
    }
    t.drawn.clone().find_map(|i| {
        let r = row_rect(width, t, i);
        if !r.contains(x, y) {
            return None;
        }
        let row = m.rows.rows.get(i)?;
        let local = x - r.x;
        if local >= copy_rect(r.w).x - 2.0 {
            return Some((i, Some(COPY_CELL), Act::None));
        }
        let v = view(gfx, m, row, r.w)?;
        // the expression takes its column; an answer cell its own box (4 around it)
        let exprs = &v.cells[0];
        let first_answer = v.cells.get(1).map_or(r.w, |c| c.2);
        if local < first_answer - 6.0 {
            return Some((i, Some(0), exprs.0.act.clone()));
        }
        v.cells
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, c)| local >= c.2 - 4.0 && local < c.2 + c.3 + 4.0)
            .map(|(k, c)| (i, Some(k), c.0.act.clone()))
            .or(Some((i, None, Act::None)))
    })
}

/// The cell under a point: its row and what it does.
pub fn hit(
    gfx: &Gfx,
    m: &BarModel,
    width: f32,
    t: &TapeLayout,
    x: f32,
    y: f32,
) -> Option<(usize, Act)> {
    cell_at(gfx, m, width, t, x, y).map(|(i, _, act)| (i, act))
}

/// A tape cell's tooltip (the web's HistoryTape titles), None for a cell that does nothing.
pub fn tip(row: &HistoryRow, cells: &[Cell], k: usize, form: AnswerForm) -> Option<&'static str> {
    if k == COPY_CELL {
        return Some("Copy");
    }
    let c = cells.get(k)?;
    if k == 0 {
        return Some(match row.kind {
            Some(HistoryKind::System) => "Edit equations",
            Some(HistoryKind::Definition) => "Insert word at the cursor",
            Some(HistoryKind::Note) => "Insert note at the cursor",
            _ => "Insert expression at the cursor",
        });
    }
    match &c.act {
        Act::None | Act::Expr => None,
        Act::Copy(_) => Some("Copy all the solutions"),
        Act::OpenSystem => Some("Edit equations"),
        Act::Answer(..) => {
            let roots = cells.iter().filter(|c| c.text.starts_with(',')).count() > 0
                || cells.iter().any(|c| matches!(c.act, Act::Copy(_)));
            let dual = cells.iter().any(|c| c.text == "≈");
            Some(if dual {
                // the ≈ splits the exact side (before it) from the approximation (after)
                let approx = cells
                    .iter()
                    .position(|c| c.text == "≈")
                    .is_some_and(|q| k > q);
                if approx && roots {
                    "Insert this solution at the cursor"
                } else if approx {
                    "Insert approximation at the cursor"
                } else {
                    "Insert exact value at the cursor"
                }
            } else if roots {
                "Insert this solution at the cursor"
            } else if row.kind == Some(HistoryKind::Definition) {
                "Insert word at the cursor"
            } else if form == AnswerForm::Exact && row.exact.is_some() {
                "Insert exact value at the cursor"
            } else {
                "Insert approximation at the cursor"
            })
        }
    }
}

/// `tip` with what Shift and Alt do on a click, and on the expression a long line in full after it, since one cut
/// off at the row's end can't be read otherwise (the Mac's `exprTip`: past 40 characters). Once the user has
/// inserted from the tape (`learned`) the insert labels go; a long expression still shows in full.
pub fn tip_text(
    row: &HistoryRow,
    cells: &[Cell],
    k: usize,
    form: AnswerForm,
    learned: bool,
) -> Option<String> {
    let what = tip(row, cells, k, form)?;
    let inserts = what.starts_with("Insert");
    let long = k == 0 && row.expr.chars().count() > 40;
    if learned && inserts {
        return long.then(|| row.expr.clone());
    }
    let mut out = what.to_string();
    if inserts {
        out += " · Shift-click keeps the history open";
        let unit = cells
            .get(k)
            .is_some_and(|c| k > 0 && crate::answer::number_and_unit(&c.text).is_some());
        if unit && row.kind != Some(HistoryKind::Definition) {
            out += ", Alt-click leaves off the unit";
        }
    }
    if long {
        out += &format!(" · {}", row.expr);
    }
    Some(out)
}

/// The tooltip and the cell's box (window DIPs) for the cell under a point.
pub fn tip_at(app: &App, row: usize, k: usize) -> Option<(String, Rect)> {
    let t = &app.bar.layout.tape;
    let r = row_rect(app.bar.layout.width, t, row);
    let hr = app.model.rows.rows.get(row)?;
    if k == COPY_CELL {
        let b = copy_rect(r.w);
        return Some(("Copy".into(), Rect::new(r.x + b.x, r.y + b.y, b.w, b.h)));
    }
    let v = view(&app.gfx, &app.model, hr, r.w)?;
    let cells: Vec<Cell> = v.cells.iter().map(|c| c.0.clone()).collect();
    let text = tip_text(hr, &cells, k, form(&app.model), tips::learned())?;
    let (_, _, x, w) = &v.cells[k];
    Some((text, Rect::new(r.x + x, r.y, *w, r.h)))
}

/// Shift and Alt as a click came (the bench's forced keys count too).
fn click_mods() -> crate::keys::Mods {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_MENU, VK_SHIFT};
    crate::bench::forced_mods().unwrap_or_else(|| {
        // SAFETY: plain queries of the keyboard state for this message
        let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
        crate::keys::Mods {
            shift: down(VK_SHIFT.0),
            alt: down(VK_MENU.0),
            ..Default::default()
        }
    })
}

/// A click on the tape: the cell's piece goes in at the caret at full precision (Shift keeps the tape open on the
/// row, Alt leaves the unit off), or is copied (the copy button, a solve's letter), or a system reopens.
pub fn click(app: &mut App, x: f32, y: f32) -> bool {
    let t = app.bar.layout.tape.clone();
    let Some((i, k, act)) = cell_at(&app.gfx, &app.model, app.bar.layout.width, &t, x, y) else {
        return false;
    };
    if k == Some(COPY_CELL) {
        copy_button(&mut app.model, i);
        return true;
    }
    let mods = click_mods();
    let (keep, bare) = (mods.shift, mods.alt);
    let engine = &app.engine;
    let m = &mut app.model;
    match act {
        Act::None => {}
        Act::Expr => insert_from_row(m, engine, i, false, None, None, keep, bare),
        Act::Answer(at, shown) => {
            insert_from_row(m, engine, i, true, Some(at), Some(&shown), keep, bare)
        }
        Act::Copy(text) => crate::answer::copy(m, &text),
        Act::OpenSystem => {
            m.insert_row(i, false);
            m.text_changed(engine);
        }
    }
    true
}

/// A right click on a tape row: its menu (the Mac's row context menu): Copy Expression, Copy Answer, Copy As (each
/// way the answer copies), Delete; a note's Copy Note and Delete. Shown outside the app's borrow.
pub fn menu(app: &mut App, x: f32, y: f32) -> bool {
    let t = app.bar.layout.tape.clone();
    let Some((i, _)) = hit(&app.gfx, &app.model, app.bar.layout.width, &t, x, y) else {
        return false;
    };
    let Some(row) = app.model.rows.rows.get(i).cloned() else {
        return false;
    };
    let note = row.kind == Some(HistoryKind::Note);
    let forms: Vec<(String, String)> = if note {
        Vec::new()
    } else {
        read_answer(&app.model, &row)
            .map(|a| a.copy_as.into_iter().map(|f| (f.label, f.text)).collect())
            .unwrap_or_default()
    };
    let (hwnd, dark, id) = (app.bar.hwnd.0 as usize, app.theme.dark, row.id.clone());
    crate::later::free("tape-menu", 0, move || {
        let Some(picked) = show_menu(hwnd, dark, note, &forms) else {
            return;
        };
        crate::app::poke(move |app| {
            // the row by its id: the tape may have changed while the menu was up
            let Some(i) = app.model.rows.rows.iter().position(|r| r.id == id) else {
                return;
            };
            let row = app.model.rows.rows[i].clone();
            match picked {
                MenuPick::Expr => copy_text(&mut app.model, row.expr.clone()),
                MenuPick::Answer => {
                    let text = row_copy(&app.model, &row);
                    copy_text(&mut app.model, text);
                }
                MenuPick::Form(text) => copy_text(&mut app.model, text),
                MenuPick::Delete => {
                    delete_row(&mut app.model, &app.engine, i);
                }
            }
        });
    });
    true
}

enum MenuPick {
    Expr,
    Answer,
    Form(String),
    Delete,
}

/// The row's popup menu at the pointer; what was picked (None: dismissed).
fn show_menu(hwnd: usize, dark: bool, note: bool, forms: &[(String, String)]) -> Option<MenuPick> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_POPUP, MF_SEPARATOR, MF_STRING,
        PostMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
    };
    use windows::core::HSTRING;
    const EXPR: usize = 1;
    const ANSWER: usize = 2;
    const DELETE: usize = 3;
    const FORM: usize = 100;
    let hwnd = HWND(hwnd as *mut _);
    crate::settings_window::dark_menus(dark);
    // a label's & would underline the next letter
    let label = |s: &str| HSTRING::from(s.replace('&', "&&"));
    let mut pt = POINT::default();
    // SAFETY: menus of our own, shown and destroyed here; the bar comes forward so the menu goes when the user
    // clicks away (TrackPopupMenu's remarks)
    let picked = unsafe {
        let _ = GetCursorPos(&mut pt);
        let menu = CreatePopupMenu().ok()?;
        if note {
            let _ = AppendMenuW(menu, MF_STRING, EXPR, &label("Copy Note"));
        } else {
            let _ = AppendMenuW(menu, MF_STRING, EXPR, &label("Copy Expression"));
            let _ = AppendMenuW(menu, MF_STRING, ANSWER, &label("Copy Answer"));
            if !forms.is_empty()
                && let Ok(sub) = CreatePopupMenu()
            {
                for (k, (name, text)) in forms.iter().enumerate() {
                    let _ =
                        AppendMenuW(sub, MF_STRING, FORM + k, &label(&format!("{name}: {text}")));
                }
                // the submenu is destroyed with its menu
                let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, &label("Copy As"));
            }
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, DELETE, &label("Delete"));
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0 as usize
    };
    match picked {
        EXPR => Some(MenuPick::Expr),
        ANSWER => Some(MenuPick::Answer),
        DELETE => Some(MenuPick::Delete),
        k if k >= FORM => forms.get(k - FORM).map(|f| MenuPick::Form(f.1.clone())),
        _ => None,
    }
}

/// The wheel or the touchpad over the bar: a scroll up opens the whole history, a scroll down past its newest row
/// closes it again; over the tape, the tape scrolls (120 a notch is a row).
pub fn wheel(app: &mut App, wp: WPARAM, lp: LPARAM) {
    let delta = f32::from((wp.0 >> 16) as u16 as i16);
    if delta != 0.0 && scroll_open_close(&mut app.model, delta > 0.0) {
        return;
    }
    let mut pt = POINT {
        x: i32::from((lp.0 & 0xffff) as u16 as i16),
        y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    };
    // SAFETY: our window, a live point
    let _ = unsafe { ScreenToClient(app.bar.hwnd, &mut pt) };
    let y = pt.y as f32 / app.bar.scale();
    let t = &app.bar.layout.tape;
    if t.visible.len() <= size::TAPE_VISIBLE || y > t.height {
        return;
    }
    let max = (t.visible.len() - size::TAPE_VISIBLE) as f32 * size::TAPE_ROW;
    let tape = &mut app.model.tape;
    // a notch adds to where a glide under way is going, not to where it is
    let from = tape.glide.map_or(tape.scroll, |g| g.1);
    let next = (from + delta / 120.0 * size::TAPE_ROW).clamp(0.0, max);
    if (next - from).abs() <= 0.01 {
        return;
    }
    // a touchpad's small steps are smooth already; a mouse wheel's notch glides
    if delta.abs() < 120.0 || reduced_motion() {
        tape.glide = None;
        tape.scroll = next;
        app.model.touch();
        return;
    }
    tape.glide = Some((tape.scroll, next, std::time::Instant::now()));
    glide_step(app);
}

/// The bench pipe's `tape`: the tape's state and, for each row drawn, its cells' and copy button's boxes in screen
/// pixels, so a test can click them for real.
pub fn debug(app: &App) -> serde_json::Value {
    let (f, s) = (app.bar.frame(), app.bar.scale());
    let screen = |r: Rect| {
        [
            f.left + ((r.x + r.w / 2.0) * s) as i32,
            f.top + ((r.y + r.h / 2.0) * s) as i32,
        ]
    };
    let t = &app.bar.layout.tape;
    let m = &app.model;
    let rows: Vec<serde_json::Value> = t
        .drawn
        .clone()
        .filter_map(|i| {
            let row = m.rows.rows.get(i)?;
            let r = row_rect(app.bar.layout.width, t, i);
            let v = view(&app.gfx, m, row, r.w)?;
            let cells: Vec<serde_json::Value> = v
                .cells
                .iter()
                .map(|(c, _, x, w)| {
                    serde_json::json!({ "text": c.text, "pick": c.pick,
                        "at": screen(Rect::new(r.x + x, r.y, *w, r.h)) })
                })
                .collect();
            let b = copy_rect(r.w);
            Some(serde_json::json!({
                "i": i, "expr": row.expr, "display": row.display, "kind": json(row)["kind"],
                "cells": cells, "copy": screen(Rect::new(r.x + b.x, r.y + b.y, b.w, b.h)),
                "spoken": spoken(row),
            }))
        })
        .collect();
    serde_json::json!({
        "open": m.tape_open, "selected": m.selected, "pick": m.tape.pick, "scroll": m.tape.scroll,
        "kept": m.tape.kept, "check": m.tape.check, "hover": crate::hover::tape_cell(),
        "hint": m.hint, "copied": m.answer.copied, "learned": tips::learned(), "rows": rows,
        "count": m.rows.rows.len(),
    })
}

/// How long a notch's glide takes.
const GLIDE_MS: f32 = 140.0;

/// Where a glide is after `t` of its time (0..1): ease-out cubic.
pub fn glide_at(from: f32, to: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    from + (to - from) * (1.0 - (1.0 - t).powi(3))
}

/// One frame of the glide; the next is a one-shot timer, and the last ends it (no timer while idle).
fn glide_step(app: &mut App) {
    let Some((from, to, at)) = app.model.tape.glide else {
        return;
    };
    let t = at.elapsed().as_secs_f32() * 1000.0 / GLIDE_MS;
    app.model.tape.scroll = glide_at(from, to, t);
    app.model.touch();
    if t >= 1.0 || !app.bar.visible {
        app.model.tape.scroll = to;
        app.model.tape.glide = None;
        return;
    }
    crate::later::after("tape-glide", 8, glide_step);
}

/// Windows' "Animation effects" off (Settings > Accessibility > Visual effects): no glides or fades.
pub fn reduced_motion() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut on = windows::core::BOOL(1);
    // SAFETY: a live out BOOL
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut on as *mut windows::core::BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && !on.as_bool()
}

// ---- paint ----

fn outline(c: &Canvas, r: Rect, radius: f32, color: crate::render::Color, width: f32) {
    let rr = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: r.x + width / 2.0,
            top: r.y + width / 2.0,
            right: r.right() - width / 2.0,
            bottom: r.bottom() - width / 2.0,
        },
        radiusX: radius,
        radiusY: radius,
    };
    let brush = c.brush(color);
    // SAFETY: inside the frame's BeginDraw/EndDraw
    unsafe { c.rt().DrawRoundedRectangle(&rr, brush, width, None) };
}

/// .tape-row.selected .picked: a 1 pt ring and a tint, both the answer colour; the pointer on an answer or the
/// expression draws the same
fn picked_mark(c: &Canvas, r: Rect) {
    c.fill_round(r, 4.0, with_alpha(c.theme.answer, 0.14));
    outline(c, r, 4.0, with_alpha(c.theme.answer, 0.55), 1.0);
}

/// The row's copy button (`doc.on.doc`, 11 pt, muted, ink with a faint fill on hover), a green check once it
/// copied.
fn copy_icon(c: &Canvas, b: Rect, hot: bool, done: bool) {
    let th = c.theme;
    if hot {
        c.fill_round(b, size::RADIUS_S, with_alpha(th.ink, 0.08));
    }
    let ink = if done {
        th.accent_hover
    } else if hot {
        th.ink
    } else {
        th.muted
    };
    let (x, y) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    if done {
        c.line(x - 4.5, y + 0.5, x - 1.5, y + 3.5, 1.5, ink);
        c.line(x - 1.5, y + 3.5, x + 4.5, y - 3.5, 1.5, ink);
        return;
    }
    // two sheets: the front one whole, the back one's top and left edge showing behind it
    outline(c, Rect::new(x - 2.5, y - 2.5, 8.0, 9.0), 1.5, ink, 1.1);
    c.line(x - 5.5, y + 3.5, x - 5.5, y - 4.5, 1.1, ink);
    c.line(x - 5.5, y - 5.0, x + 2.5, y - 5.0, 1.1, ink);
}

pub fn paint(c: &Canvas, model: &BarModel, t: &TapeLayout) {
    if t.drawn.is_empty() {
        return;
    }
    let th = c.theme;
    c.fill(Rect::new(0.0, 0.0, c.width, t.height), th.bg_tape);
    c.fill(Rect::new(0.0, t.height - 1.0, c.width, 1.0), th.line);
    let win = window(t);
    c.push_clip(Rect::new(0.0, win.y, c.width, win.h));
    // the pointer's row and cell (hover.rs): .tape-row:hover, .tape-q:hover, .tape-a:hover, .tape-root:hover. One
    // row is filled at a time: the row under the pointer while there is one, else the row ↑ ↓ picked (whose ← →
    // box hides too while the pointer is on another row)
    let hovered = crate::hover::tape_cell();
    let lit_row = hovered.map(|h| h.0).or(model.selected);
    for i in t.drawn.clone() {
        let Some(row) = model.rows.rows.get(i) else {
            continue;
        };
        let r = row_rect(c.width, t, i);
        let selected = model.selected == Some(i);
        let lit = lit_row == Some(i);
        let over = hovered.filter(|h| h.0 == i);
        if lit {
            let fill = if selected {
                th.fill_selected
            } else {
                th.fill_hover
            };
            c.fill_round(r, size::RADIUS_S, fill);
        }
        let Some(v) = view(c.gfx, model, row, r.w) else {
            continue;
        };
        for (k, (cell, layout, x, w)) in v.cells.iter().enumerate() {
            let picked = selected && lit && cell.pick == Some(model.tape.pick);
            let hot = over.is_some_and(|h| h.1 == Some(k)) && cell.act != Act::None;
            let mark = Rect::new(r.x + x - 4.0, r.y + 4.0, w + 8.0, r.h - 8.0);
            if picked || (hot && !matches!(cell.act, Act::Copy(_))) {
                picked_mark(c, mark);
            } else if hot {
                // a solve's letter copies every root
                c.fill_round(mark, 4.0, with_alpha(th.answer, 0.12));
            }
            let ink = match cell.look {
                Look::Expr if picked || hot => th.ink,
                Look::Quiet if hot => th.answer_hover,
                Look::Expr | Look::Quiet => th.muted,
                Look::Answer if hot => th.answer_hover,
                Look::Answer => th.answer,
            };
            match (&v.stacked, k) {
                // a stacked expression on the row's baseline, moved only as far as all of it needs to show
                (Some(pic), 0) => {
                    let plain = crate::stacked_layout::baseline(layout);
                    c.push_clip(Rect::new(r.x + x, r.y, *w + 1.0, r.h));
                    pic.draw(c, r.x + x, r.y + pic.baseline_in(plain, r.h), ink);
                    c.pop_clip();
                }
                _ => c.text(layout, r.x + x, r.y, ink),
            }
        }
        // the copy button: on the row under the pointer, and its check after a copy
        let done = model.tape.check.as_deref() == Some(row.id.as_str());
        if over.is_some() || done {
            let b = copy_rect(r.w);
            let hot = over.is_some_and(|h| h.1 == Some(COPY_CELL));
            copy_icon(c, Rect::new(r.x + b.x, r.y + b.y, b.w, b.h), hot, done);
        }
    }
    c.pop_clip();
    crate::answer::timer::recent_expiry(model);
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcalc_ffi::SolveInfo;

    fn row(expr: &str, display: &str, exact: Option<&str>) -> HistoryRow {
        HistoryRow {
            id: format!("{expr}|{display}"),
            expr: expr.into(),
            display: display.into(),
            exact: exact.map(String::from),
            n: display.parse().ok(),
            meas: None,
            quantity: None,
            kind: None,
            equations: None,
            fn_name: None,
            fn_params: None,
            fn_body: None,
            solve: None,
            at: None,
        }
    }

    fn solved(expr: &str, display: &str, roots: Vec<f64>) -> HistoryRow {
        let mut r = row(expr, display, None);
        r.n = None;
        r.solve = Some(SolveInfo {
            variable: "x".into(),
            roots,
            outcome: SolveOutcome::Roots,
            more: false,
        });
        r
    }

    fn acts(c: &[Cell]) -> Vec<(String, Act, Option<usize>)> {
        c.iter()
            .map(|c| (c.text.clone(), c.act.clone(), c.pick))
            .collect()
    }

    fn engine() -> EngineThread {
        EngineThread::start(
            qcalc_ffi::EngineConfig {
                is_windows: true,
                locale: "en_US".into(),
                data_dir: None,
                time_zone: None,
            },
            || {},
        )
    }

    #[test]
    fn one_answer() {
        let c = cells(&row("2+2", "4", None), AnswerForm::Exact, 6.0);
        assert_eq!(
            acts(&c),
            [
                ("2+2".into(), Act::Expr, Some(0)),
                ("4".into(), Act::Answer(1, "4".into()), Some(1)),
            ]
        );
    }

    #[test]
    fn dual_answer() {
        let c = cells(
            &row("sqrt(2)", "1.41421", Some("√2")),
            AnswerForm::Exact,
            6.0,
        );
        assert_eq!(c[0].text, "√(2)");
        assert_eq!(c[1].text, "√2");
        assert_eq!(c[1].pick, Some(1));
        assert_eq!(c[2].text, "≈");
        assert_eq!(c[3].pick, Some(2));
        assert_eq!(c[3].act, Act::Answer(2, "1.41421".into()));
    }

    #[test]
    fn solve_rows() {
        let r = solved("x^2=4", "-2, 2", vec![-2.0, 2.0]);
        let c = cells(&r, AnswerForm::Exact, 6.0);
        assert_eq!(c[1].text, "x =");
        assert_eq!(c[1].act, Act::Copy("-2, 2".into()));
        assert_eq!(c[2].act, Act::Answer(1, "-2".into()));
        assert_eq!(c[2].pick, Some(1));
        assert_eq!(c[3].text, ", ");
        assert_eq!(c[3].act, Act::None);
        assert_eq!(c[4].text, "2");
        assert_eq!(c[4].pick, Some(2));
        // `±2`: each root its own button
        let pm = solved("x^2=4", "±2", vec![-2.0, 2.0]);
        let c = cells(&pm, AnswerForm::Exact, 6.0);
        assert_eq!(
            c.iter()
                .filter(|c| matches!(c.act, Act::Answer(..)))
                .count(),
            2
        );
        let mut none = row("x^2=-4", "no real solution", None);
        none.solve = Some(SolveInfo {
            variable: "x".into(),
            roots: vec![],
            outcome: SolveOutcome::None,
            more: false,
        });
        let c = cells(&none, AnswerForm::Exact, 6.0);
        assert_eq!(c[1].look, Look::Quiet);
        assert_eq!(c[1].act, Act::Answer(1, String::new()));
    }

    #[test]
    fn a_note_is_its_text_alone() {
        let mut n = row("call Sam at 3pm", "", None);
        n.kind = Some(HistoryKind::Note);
        let c = cells(&n, AnswerForm::Exact, 6.0);
        assert_eq!(acts(&c), [("call Sam at 3pm".into(), Act::Expr, Some(0))]);
        assert_eq!(
            tip(&n, &c, 0, AnswerForm::Exact),
            Some("Insert note at the cursor")
        );
        assert_eq!(spoken(&n), "Note call Sam at 3pm");
        let m = BarModel::new(crate::store::Settings::default());
        // nothing to pick, and its copy is its text
        assert_eq!(picks(&m, &n).answers.len(), 0);
        assert_eq!(row_copy(&m, &n), "call Sam at 3pm");
    }

    /// The tape's keys through the model as the bar routes them (no window; the engine thread runs but its
    /// answers never come back, so only the synchronous parts are checked).
    #[test]
    fn tape_keys() {
        use crate::keys::Mods;
        use crate::model::Outcome;
        const UP: u32 = 0x26;
        const LEFT: u32 = 0x25;
        const ENTER: u32 = 0x0D;
        const BACK: u32 = 0x08;
        let engine = engine();
        let mut m = BarModel::new(crate::store::Settings::default());
        m.rows = Rows::new(vec![
            row("2+2", "4", None),
            row("sqrt(8)", "2.82843", Some("2sqrt(2)")),
            row("255 in hex", "0xFF", None),
        ]);
        let none = Mods::default();
        let alt = Mods { alt: true, ..none };
        let ctrl = Mods { ctrl: true, ..none };
        let key = |m: &mut BarModel, vk: u32, mods: Mods| m.key_down(vk, mods, &engine, || None);
        // ↑ lands on the newest row's answer; ↵ inserts it
        key(&mut m, UP, none);
        assert_eq!((m.selected, m.tape.pick), (Some(2), 1));
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "0xFF");
        assert_eq!(m.selected, None);
        // Alt+↵ inserts the other half than the setting (expression by default, so the answer)
        m.field.reset("");
        key(&mut m, UP, none);
        key(&mut m, UP, none);
        assert_eq!(m.selected, Some(1));
        // a dual row lands on the exact side when the setting is exact
        assert_eq!(m.tape.pick, 1);
        key(&mut m, ENTER, alt);
        assert_eq!(m.field.text(), "2sqrt(2)");
        // ← picks the expression, and ↑ keeps to that side
        m.field.reset("");
        key(&mut m, UP, none);
        key(&mut m, LEFT, none);
        assert_eq!(m.tape.pick, 0);
        key(&mut m, UP, none);
        assert_eq!((m.selected, m.tape.pick), (Some(1), 0));
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "sqrt(8)");
        // Ctrl+Backspace deletes the highlighted row, Ctrl+Z puts it back
        m.field.reset("");
        key(&mut m, UP, none);
        key(&mut m, UP, none);
        assert_eq!(m.selected, Some(1));
        key(&mut m, BACK, ctrl);
        assert_eq!(m.rows.rows.len(), 2);
        assert_eq!(m.rows.rows[1].expr, "255 in hex");
        // the next row is picked, its pick landed again
        assert_eq!((m.selected, m.tape.pick), (Some(1), 1));
        key(&mut m, 0x5A, ctrl);
        assert_eq!(m.rows.rows.len(), 3);
        assert_eq!(m.rows.rows[1].expr, "sqrt(8)");
        assert_eq!(m.selected, Some(1));
        // a plain Backspace on a picked row deletes it too
        key(&mut m, BACK, none);
        assert_eq!(m.rows.rows.len(), 2);
        key(&mut m, 0x5A, ctrl);
        assert_eq!(m.rows.rows.len(), 3);
        // Ctrl+↓ goes straight back to the line
        key(&mut m, 0x28, ctrl);
        assert_eq!(m.selected, None);
        // Ctrl+W asks the app to close a sheet or hide
        assert_eq!(key(&mut m, 0x57, ctrl), Outcome::CloseSheet);
        // `?` shows the cheat sheet; Esc closes it
        m.field.reset("");
        m.char16(u16::from(b'?'), &engine);
        assert!(m.tape.help);
        key(&mut m, 0x1B, none);
        assert!(!m.tape.help);
        engine.shutdown();
    }

    /// Earlier answers go in at full precision with their unit (Ctrl+↵ without it), an angle as its number, among
    /// other text in brackets; Shift+↵ keeps the tape open, and Backspace then edits the line.
    #[test]
    fn inserts_at_full_precision_and_among_text() {
        use crate::keys::Mods;
        const UP: u32 = 0x26;
        const ENTER: u32 = 0x0D;
        const BACK: u32 = 0x08;
        let engine = engine();
        let mut m = BarModel::new(crate::store::Settings::default());
        let mut third = row("1/3", "0.333333", None);
        third.n = Some(1.0 / 3.0);
        let mut mm = row("0.2 in in mm", "5.08 mm", None);
        mm.n = Some(5.08);
        mm.quantity = Some("5.08 mm".into());
        let mut deg = row("asin(0.5)", "30 deg", None);
        deg.n = Some(30.0);
        deg.quantity = Some("30 deg".into());
        let roots = solved("x^2=4", "±2", vec![-2.0, 2.0]);
        m.rows = Rows::new(vec![third, mm, deg, roots]);
        let none = Mods::default();
        let shift = Mods {
            shift: true,
            ..none
        };
        let ctrl = Mods { ctrl: true, ..none };
        let key = |m: &mut BarModel, vk: u32, mods: Mods| m.key_down(vk, mods, &engine, || None);
        let up = |m: &mut BarModel, n: usize| {
            for _ in 0..n {
                key(m, UP, none);
            }
        };
        up(&mut m, 4);
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "0.333333333333333");
        m.field.reset("");
        up(&mut m, 3);
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "5.08 mm");
        m.field.reset("");
        up(&mut m, 3);
        key(&mut m, ENTER, ctrl);
        assert_eq!(m.field.text(), "5.08");
        m.field.reset("");
        up(&mut m, 2);
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "30");
        // a root among other text keeps its value: `5(-2)`, not `5-2`
        m.field.reset("5");
        up(&mut m, 1);
        assert_eq!(m.tape.pick, 1);
        key(&mut m, ENTER, none);
        assert_eq!(m.field.text(), "5(-2)");
        // Shift+↵ keeps the tape open on the row and its pick; Backspace then edits the line
        m.field.reset("");
        up(&mut m, 1);
        key(&mut m, ENTER, shift);
        assert_eq!(m.field.text(), "-2");
        assert_eq!((m.selected, m.tape_open, m.tape.pick), (Some(3), true, 1));
        key(&mut m, BACK, none);
        assert_eq!(m.rows.rows.len(), 4);
        assert_eq!(m.selected, None);
        assert_eq!(m.field.text(), "-");
        // Ctrl+C on a picked row copies its pick as shown, Ctrl+Alt+C at full precision
        m.field.reset("");
        up(&mut m, 4);
        key(&mut m, 0x43, ctrl);
        assert_eq!(m.to_clipboard.take().as_deref(), Some("0.333333"));
        m.key_down(
            0x43,
            Mods {
                ctrl: true,
                alt: true,
                ..none
            },
            &engine,
            || None,
        );
        assert_eq!(m.to_clipboard.take().as_deref(), Some("0.333333333333333"));
        // Ctrl+Shift+C copies the picked row's own line
        crate::answer::copy_line(&mut m);
        assert_eq!(m.to_clipboard.take().as_deref(), Some("1/3 = 0.333333"));
        engine.shutdown();
    }

    #[test]
    fn a_menu_delete_leaves_the_line_its_keys() {
        let engine = engine();
        let mut m = BarModel::new(crate::store::Settings::default());
        m.rows = Rows::new(vec![
            row("1", "1", None),
            row("2", "2", None),
            row("3", "3", None),
        ]);
        m.field.reset("12");
        delete_row(&mut m, &engine, 1);
        assert_eq!((m.rows.rows.len(), m.selected), (2, None));
        // Backspace is the line's again
        m.key_down(0x08, crate::keys::Mods::default(), &engine, || None);
        assert_eq!(m.field.text(), "1");
        assert_eq!(m.rows.rows.len(), 2);
        // Ctrl+Z (here, straight to the tape's undo) brings the row back without picking it
        m.field.reset("");
        assert!(undo_delete(&mut m, &engine));
        assert_eq!((m.rows.rows.len(), m.selected), (3, None));
        engine.shutdown();
    }

    #[test]
    fn a_line_with_no_answer_is_kept_as_a_note() {
        let engine = engine();
        let mut m = BarModel::new(crate::store::Settings::default());
        m.field.reset("call Sam at 3pm");
        assert!(save_note(&mut m, &engine, "call Sam at 3pm"));
        let r = &m.rows.rows[0];
        assert_eq!(
            (r.expr.as_str(), r.display.as_str(), r.kind),
            ("call Sam at 3pm", "", Some(HistoryKind::Note))
        );
        assert_eq!(m.field.text(), "");
        assert_eq!(m.hint.as_deref(), Some("saved as a note"));
        // not a graph line
        assert!(!save_note(&mut m, &engine, "graph"));
        assert_eq!(m.rows.rows.len(), 1);
        engine.shutdown();
    }

    #[test]
    fn a_scroll_opens_and_closes_the_history() {
        let mut m = BarModel::new(crate::store::Settings::default());
        m.rows = Rows::new(vec![row("1", "1", None)]);
        assert!(scroll_open_close(&mut m, true));
        assert!((m.tape_open, m.selected) == (true, None));
        // up again scrolls it, down at its newest row closes it
        assert!(!scroll_open_close(&mut m, true));
        assert!(scroll_open_close(&mut m, false));
        assert!(!m.tape_open);
        // not with a row picked, nor when the setting always shows it
        m.tape_open = true;
        m.selected = Some(0);
        assert!(!scroll_open_close(&mut m, false));
        m.settings.history_show = "always".into();
        m.tape_open = false;
        m.selected = None;
        assert!(!scroll_open_close(&mut m, true));
    }

    #[test]
    fn a_glide_eases_out_and_lands() {
        assert_eq!(glide_at(0.0, 36.0, 0.0), 0.0);
        assert_eq!(glide_at(0.0, 36.0, 1.0), 36.0);
        assert_eq!(glide_at(0.0, 36.0, 2.0), 36.0);
        // most of the way by half time, and never past
        let half = glide_at(0.0, 36.0, 0.5);
        assert!(half > 30.0 && half < 36.0, "{half}");
        assert!(glide_at(36.0, 0.0, 0.5) < 6.0);
    }

    #[test]
    fn a_long_expression_shows_whole_in_its_tip() {
        let f = AnswerForm::Exact;
        let long = "1+2+3+4+5+6+7+8+9+10+11+12+13+14+15+16+17";
        let r = row(long, "153", None);
        let c = cells(&r, f, 6.0);
        assert_eq!(
            tip_text(&r, &c, 0, f, false).as_deref(),
            Some(
                format!(
                    "Insert expression at the cursor · Shift-click keeps the history open · {long}"
                )
                .as_str()
            )
        );
        assert_eq!(
            tip_text(&r, &c, 1, f, false).as_deref(),
            Some("Insert approximation at the cursor · Shift-click keeps the history open")
        );
        // once learned, only a long line's text is left
        assert_eq!(tip_text(&r, &c, 0, f, true).as_deref(), Some(long));
        assert_eq!(tip_text(&r, &c, 1, f, true), None);
        let short = row("2+2", "4", None);
        let c = cells(&short, f, 6.0);
        assert_eq!(tip_text(&short, &c, 0, f, true), None);
        // a unit's answer says Alt leaves it off
        let unit = row("2 m + 3 m", "5 m", None);
        let c = cells(&unit, f, 6.0);
        assert_eq!(
            tip_text(&unit, &c, 1, f, false).as_deref(),
            Some(
                "Insert approximation at the cursor · Shift-click keeps the history open, Alt-click leaves off the unit"
            )
        );
        // the copy button
        assert_eq!(
            tip_text(&unit, &c, COPY_CELL, f, true).as_deref(),
            Some("Copy")
        );
    }

    #[test]
    fn tips_say_what_a_click_does() {
        let f = AnswerForm::Exact;
        let one = row("2+2", "4", None);
        let c = cells(&one, f, 6.0);
        assert_eq!(tip(&one, &c, 0, f), Some("Insert expression at the cursor"));
        assert_eq!(
            tip(&one, &c, 1, f),
            Some("Insert approximation at the cursor")
        );
        let dual = row("sqrt(2)", "1.41421", Some("√2"));
        let c = cells(&dual, f, 6.0);
        assert_eq!(
            tip(&dual, &c, 1, f),
            Some("Insert exact value at the cursor")
        );
        assert_eq!(tip(&dual, &c, 2, f), None);
        assert_eq!(
            tip(&dual, &c, 3, f),
            Some("Insert approximation at the cursor")
        );
        let solve = solved("x^2=4", "-2, 2", vec![-2.0, 2.0]);
        let c = cells(&solve, f, 6.0);
        assert_eq!(tip(&solve, &c, 1, f), Some("Copy all the solutions"));
        assert_eq!(
            tip(&solve, &c, 2, f),
            Some("Insert this solution at the cursor")
        );
        assert_eq!(tip(&solve, &c, 3, f), None);
        let mut sys = row("x+y=3; x-y=1", "x = 2, y = 1", None);
        sys.kind = Some(HistoryKind::System);
        let c = cells(&sys, f, 6.0);
        assert_eq!(tip(&sys, &c, 0, f), Some("Edit equations"));
    }

    #[test]
    fn system_rows_reopen() {
        let mut r = row("x+y=3; x-y=1", "x = 2, y = 1", None);
        r.kind = Some(HistoryKind::System);
        let c = cells(&r, AnswerForm::Exact, 6.0);
        assert_eq!(c[0].act, Act::OpenSystem);
    }
}
