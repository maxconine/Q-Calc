//! The bar's state and actions: the port of native/mac/Sources/Bar/BarModel.swift. No Win32 here: views read
//! the model and paint it, input arrives as `key_down` / `char16` / `paste` / clicks, and work for the engine is
//! submitted to `EngineThread` lanes whose results come back as `set_live` / commit closures.
//!
//! The rule the app relies on: **every mutation sets `changed`**. After each message app.rs checks
//! `take_changed()` and, if set, re-lays out the bar (its height follows the tape and the hint) and repaints it.
//! Nothing repaints on a timer.
//!
//! Done here: the live answer (with the history's context and chaining, as `evaluateLive`), Enter through
//! `Engine::commit` (the row it saves, the new input, the hint when nothing saved), the tape (↑ ↓ walk it, ← →
//! pick the expression or the answer, ↵ inserts it), the in-bar shortcuts (keys.rs), history persistence (the
//! engine's `save_history`, off the UI thread).
//!
//! Still to port from BarModel.swift (each owned by the module named): commands (`BarCommand`: help, games,
//! settings, periodic, identities, message, graph) → model.rs + the panel's module; answer forms on Tab
//! (answer.rs); closed-form upgrades (answer.rs); the steady dimmed answer while an edit is half typed
//! (answer.rs); decorations and squiggles (input.rs); the typeset row (typst_row.rs); `sys` (systems.rs); graph
//! (graph.rs); onboarding hints and the tour; the draft (store.rs `Draft`, restored in `will_show`); effects
//! (effects.rs); messaging cards (messaging.rs).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use qcalc_ffi::{
    AnswerForm, EvalOptions, HistoryRow, HistoryShow, HistoryView, LiveAnswer, LiveRequest,
};

use crate::engine::{EngineThread, Lane, Rows, done};
use crate::input::Field;
use crate::keys::{KeyAction, Keybinds, Mods};
use crate::store::Settings;

/// The recent view: rows younger than 5 minutes, at most 5 (History.swift).
pub const RECENT_MS: i64 = 5 * 60 * 1000;
pub const RECENT_ROWS: usize = 5;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// What a key did, for the window to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// not the bar's key: DefWindowProc
    Ignored,
    Handled,
    Hide,
    OpenSettings,
    /// the clear-history shortcut: app.rs empties the tape and saves it
    ClearHistory,
}

/// The bar's state.
pub struct BarModel {
    pub field: Field,
    /// the live answer and the text it answers (it may lag the field by a keystroke)
    pub live: Option<LiveAnswer>,
    pub live_for: String,
    pub chained: bool,
    pub rows: Rows,
    /// the highlighted tape row
    pub selected: Option<usize>,
    /// on a highlighted row, ← → pick the expression (false) or the answer (true)
    pub pick_answer: bool,
    pub tape_open: bool,
    /// the line under the composer (why nothing saved, a notice)
    pub hint: Option<String>,
    pub settings: Settings,
    pub keybinds: Keybinds,
    /// the settings changed here (a shortcut toggled angle mode): app.rs saves the store
    pub settings_dirty: bool,
    changed: bool,
    /// the clipboard text an action wants written (app.rs writes it)
    pub to_clipboard: Option<String>,
}

impl BarModel {
    pub fn new(settings: Settings) -> BarModel {
        BarModel {
            field: Field::default(),
            live: None,
            live_for: String::new(),
            chained: false,
            rows: Rows::default(),
            selected: None,
            pick_answer: true,
            tape_open: false,
            hint: None,
            keybinds: Keybinds::new(&settings.keybinds),
            settings,
            settings_dirty: false,
            changed: true,
            to_clipboard: None,
        }
    }

    pub fn touch(&mut self) {
        self.changed = true;
    }

    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.keybinds = Keybinds::new(&settings.keybinds);
        self.settings = settings;
        self.changed = true;
    }

    /// `EvalOptions.from(settings)` (EngineService.swift).
    pub fn options(&self) -> EvalOptions {
        let s = &self.settings;
        EvalOptions {
            angle_radians: s.angle_mode == "rad",
            fraction_mode: s.fraction_mode,
            sig_figs: s.sig_figs.clamp(2, 16) as u32,
            sig_fig_mode: s.sig_fig_mode,
            rationalize: s.rationalize,
            space_thousands: s.space_thousands,
            default_units: s
                .default_units
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>(),
            ..EvalOptions::default()
        }
    }

    /// `HistoryView.from(settings)`.
    pub fn view(&self, now: i64) -> HistoryView {
        HistoryView {
            show: match self.settings.history_show.as_str() {
                "always" => HistoryShow::Always,
                "arrow" => HistoryShow::Arrow,
                _ => HistoryShow::Recent,
            },
            answer_form: if self.settings.answer_form == "approx" {
                AnswerForm::Approx
            } else {
                AnswerForm::Exact
            },
            sig_figs: self.settings.sig_figs.clamp(2, 16) as u32,
            now_ms: now,
        }
    }

    // ---- the live answer ----

    /// The line changed (typed, pasted, undone, replaced): drop what belonged to the old text and evaluate.
    pub fn text_changed(&mut self, engine: &EngineThread) {
        self.hint = crate::games::hint(self.field.text());
        if self.selected.is_some() && !self.field.text().is_empty() {
            self.selected = None;
            self.tape_open = false;
        }
        self.changed = true;
        self.evaluate_live(engine);
    }

    pub fn evaluate_live(&mut self, engine: &EngineThread) {
        let typed = self.field.text().to_string();
        if typed.trim().is_empty() {
            engine.cancel(Lane::Live);
            self.chained = false;
            self.live = None;
            self.live_for = typed;
            self.changed = true;
            return;
        }
        let rows = self.rows.clone();
        let now = now_ms();
        let view = self.view(now);
        let options = self.options();
        engine.submit(Lane::Live, move |cx| {
            let context = cx.context(&rows, view);
            let chained = cx.engine.chains(typed.clone(), context.clone());
            let answer = cx.engine.live(LiveRequest {
                text: typed.clone(),
                options,
                context,
                chained,
                now_ms: now,
                phrases: true,
            });
            done(move |app| app.model.set_live(&typed, answer, chained))
        });
    }

    /// A live answer landed (on the UI thread, still the newest): kept only if it answers the line as it is.
    pub fn set_live(&mut self, typed: &str, answer: LiveAnswer, chained: bool) {
        if self.field.text() != typed {
            return;
        }
        self.chained = chained;
        self.live = Some(answer);
        self.live_for = typed.to_string();
        self.changed = true;
    }

    /// The answer as the slot shows it ("" for none).
    pub fn answer_text(&self) -> String {
        let Some(a) = &self.live else {
            return String::new();
        };
        if let Some(solve) = &a.solve
            && !a.pretty.is_empty()
            && !a.pretty.contains('=')
        {
            return format!("{} = {}", solve.variable, a.pretty);
        }
        if a.pretty.is_empty() {
            a.display.clone()
        } else {
            a.pretty.clone()
        }
    }

    // ---- Enter ----

    pub fn enter(&mut self, engine: &EngineThread) {
        if let Some(i) = self.selected {
            self.insert_row(i, self.pick_answer);
            self.text_changed(engine);
            return;
        }
        self.commit(engine);
    }

    /// Saves the line to the tape through `Engine::commit`; a blank line says why rather than saving nothing.
    pub fn commit(&mut self, engine: &EngineThread) {
        let typed = self.field.text().to_string();
        if typed.trim().is_empty() {
            return;
        }
        let rows = self.rows.clone();
        let now = now_ms();
        let view = self.view(now);
        let options = self.options();
        engine.submit(Lane::Commit, move |cx| {
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
            let result = cx.engine.commit(request, rows.rows.to_vec());
            done(move |app| {
                let saved = result.saved;
                if saved {
                    app.set_history(result.history);
                }
                let m = &mut app.model;
                if m.field.text() != typed {
                    return;
                }
                if saved {
                    m.field.set_text(&result.input);
                    m.field.break_run();
                    m.hint = None;
                    app.text_changed();
                } else {
                    m.hint = Some(result.hint.unwrap_or_else(|| {
                        "nothing to save yet · finish the line, or esc to close".into()
                    }));
                    m.changed = true;
                }
            })
        });
    }

    // ---- the tape ----

    /// recentStart: the first row of the recent view.
    pub fn recent_start(&self, now: i64) -> usize {
        let rows = &self.rows.rows;
        let mut i = rows.len();
        while i > 0
            && rows.len() - i < RECENT_ROWS
            && rows[i - 1]
                .at
                .is_some_and(|at| at <= now && now - at <= RECENT_MS)
        {
            i -= 1;
        }
        i
    }

    /// The rows the tape shows: all while it's open (or always), the recent ones otherwise.
    pub fn visible(&self, now: i64) -> std::ops::Range<usize> {
        let n = self.rows.rows.len();
        if self.tape_open || self.settings.history_show == "always" {
            return 0..n;
        }
        if self.settings.history_show == "arrow" {
            return n..n;
        }
        self.recent_start(now)..n
    }

    pub fn move_selection(&mut self, up: bool) {
        let n = self.rows.rows.len();
        if n == 0 {
            return;
        }
        self.changed = true;
        if up {
            let fresh = !self.tape_open || self.selected.is_none();
            self.tape_open = true;
            self.selected = Some(if fresh {
                n - 1
            } else {
                self.selected.unwrap_or(n).saturating_sub(1)
            });
            if fresh {
                self.pick_answer = true;
            }
        } else {
            if !self.tape_open {
                return;
            }
            match self.selected {
                Some(i) if i + 1 < n => self.selected = Some(i + 1),
                _ => {
                    self.selected = None;
                    self.tape_open = false;
                }
            }
        }
    }

    /// The text a row inserts: its expression, or its answer in the setting's form.
    pub fn row_piece(&self, row: &HistoryRow, answer: bool) -> String {
        if answer && row.kind != Some(qcalc_ffi::HistoryKind::Definition) {
            if self.settings.answer_form == "exact" {
                row.exact.clone().unwrap_or_else(|| row.display.clone())
            } else {
                row.display.clone()
            }
        } else {
            row.expr.clone()
        }
    }

    /// ↵ on a row, or a click on one of its halves: the piece goes in at the caret.
    pub fn insert_row(&mut self, i: usize, answer: bool) {
        let Some(row) = self.rows.rows.get(i).cloned() else {
            return;
        };
        let piece = self.row_piece(&row, answer);
        self.selected = None;
        self.tape_open = false;
        self.field.insert(&piece);
        self.field.break_run();
        self.changed = true;
    }

    // ---- keys ----

    /// A key-down while the bar has the keyboard. Text arrives separately as WM_CHAR (`char16`).
    pub fn key_down(
        &mut self,
        vk: u32,
        mods: Mods,
        engine: &EngineThread,
        clipboard: impl FnOnce() -> Option<String>,
    ) -> Outcome {
        const BACK: u32 = 0x08;
        const TAB: u32 = 0x09;
        const ENTER: u32 = 0x0D;
        const ESC: u32 = 0x1B;
        const END: u32 = 0x23;
        const HOME: u32 = 0x24;
        const LEFT: u32 = 0x25;
        const UP: u32 = 0x26;
        const RIGHT: u32 = 0x27;
        const DOWN: u32 = 0x28;
        const DELETE: u32 = 0x2E;
        let before = self.field.text().to_string();
        let ctrl = mods.ctrl && !mods.alt;
        // copying a selection beats the copy-answer shortcut on the same chord
        if ctrl && !mods.shift && vk == 0x43 && self.field.has_selection() {
            self.to_clipboard = Some(self.field.selected_text().to_string());
            return Outcome::Handled;
        }
        if let Some(action) = self.keybinds.action(mods, vk) {
            return self.perform(action, engine);
        }
        let shift = mods.shift;
        match vk {
            ESC => {
                if self.tape_open || self.selected.is_some() {
                    self.tape_open = false;
                    self.selected = None;
                    self.changed = true;
                    return Outcome::Handled;
                }
                return Outcome::Hide;
            }
            ENTER => {
                // a game's command (`pong`, `connect 4`…) opens its window and puts the bar away
                if self.selected.is_none()
                    && let Some(kind) = crate::games::command(self.field.text())
                {
                    crate::games::open(kind);
                    self.field.reset("");
                    self.text_changed(engine);
                    return Outcome::Hide;
                }
                self.enter(engine);
                return Outcome::Handled;
            }
            UP => self.move_selection(true),
            DOWN => self.move_selection(false),
            LEFT if self.selected.is_some() => {
                self.pick_answer = false;
                self.changed = true;
            }
            RIGHT if self.selected.is_some() => {
                self.pick_answer = true;
                self.changed = true;
            }
            LEFT => self.field.left(ctrl, shift),
            RIGHT => self.field.right(ctrl, shift),
            HOME => self.field.home(shift),
            END => self.field.end(shift),
            BACK => {
                self.field.backspace(ctrl);
            }
            DELETE => {
                self.field.delete(ctrl);
            }
            TAB => {}
            // Ctrl+A / X / V / Z / Y
            0x41 if ctrl => self.field.select_all(),
            0x58 if ctrl => {
                if let Some(cut) = self.field.cut() {
                    self.to_clipboard = Some(cut);
                }
            }
            0x56 if ctrl => {
                if let Some(text) = clipboard() {
                    self.field.paste(&text);
                }
            }
            0x5A if ctrl && shift => {
                self.field.redo();
            }
            0x5A if ctrl => {
                self.field.undo();
            }
            0x59 if ctrl => {
                self.field.redo();
            }
            _ => return Outcome::Ignored,
        }
        self.changed = true;
        if self.field.text() != before {
            self.text_changed(engine);
        }
        Outcome::Handled
    }

    /// WM_CHAR.
    pub fn char16(&mut self, unit: u16, engine: &EngineThread) {
        if self.field.char16(unit) {
            self.text_changed(engine);
        }
    }

    /// Text from the IME or a paste.
    pub fn insert_text(&mut self, text: &str, engine: &EngineThread) {
        self.field.insert(text);
        self.field.break_run();
        self.text_changed(engine);
    }

    pub fn perform(&mut self, action: KeyAction, engine: &EngineThread) -> Outcome {
        self.changed = true;
        match action {
            KeyAction::Settings => return Outcome::OpenSettings,
            KeyAction::CopyAnswer => {
                if let Some(a) = &self.live
                    && !a.display.is_empty()
                {
                    self.to_clipboard = Some(a.display.clone());
                }
            }
            KeyAction::CopyLine => {
                let answer = self
                    .live
                    .as_ref()
                    .map(|a| a.display.clone())
                    .unwrap_or_default();
                if !answer.is_empty() {
                    self.to_clipboard = Some(format!("{} = {answer}", self.field.text().trim()));
                }
            }
            KeyAction::Angle => {
                self.settings.angle_mode = if self.settings.angle_mode == "rad" {
                    "deg".into()
                } else {
                    "rad".into()
                };
                self.settings_dirty = true;
                self.evaluate_live(engine);
            }
            KeyAction::Fraction => {
                self.settings.fraction_mode = !self.settings.fraction_mode;
                self.settings_dirty = true;
                self.evaluate_live(engine);
            }
            KeyAction::SigFigs => {
                self.settings.sig_fig_mode = !self.settings.sig_fig_mode;
                self.settings_dirty = true;
                self.evaluate_live(engine);
            }
            KeyAction::Clear => {
                self.selected = None;
                self.tape_open = false;
                return Outcome::ClearHistory;
            }
        }
        Outcome::Handled
    }
}
