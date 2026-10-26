//! The input's own undo (`src/lib/undo.ts`): typing and deleting group into runs, like the Mac's text fields.

use serde::{Deserialize, Serialize};

/// The input's text and selection (UTF-16 indices).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoState {
    pub value: String,
    pub start: usize,
    pub end: usize,
}

/// `insert` and `delete` runs group like typing does on the mac; anything else is its own step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditKind {
    Insert,
    Delete,
}

/// The undo history.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Undo {
    pub past: Vec<UndoState>,
    pub future: Vec<UndoState>,
    pub current: UndoState,
    /// The kind of the run `current` belongs to; None once the run is broken (written as JSON null).
    pub run: Option<EditKind>,
    /// When the run last grew, in ms.
    pub at: f64,
}

pub const UNDO_RUN_MS: f64 = 1000.0;
const UNDO_LIMIT: usize = 200;

/// A fresh history with the caret at the end of `value`.
pub fn undo_start(value: &str) -> Undo {
    let end = crate::app::js::len16(value);
    Undo {
        past: Vec::new(),
        future: Vec::new(),
        current: UndoState {
            value: value.to_string(),
            start: end,
            end,
        },
        run: None,
        at: 0.0,
    }
}

/// The kind of an `InputEvent.inputType`.
pub fn edit_kind(input_type: Option<&str>) -> Option<EditKind> {
    match input_type? {
        "insertText" | "insertCompositionText" => Some(EditKind::Insert),
        "deleteContentBackward" | "deleteContentForward" => Some(EditKind::Delete),
        _ => None,
    }
}

/// An edit: joins the current run when it is the same kind within `UNDO_RUN_MS`, otherwise a new step.
pub fn record_edit(u: &Undo, next: UndoState, kind: Option<EditKind>, now: f64) -> Undo {
    if next.value == u.current.value {
        return u.clone();
    }
    let joins = kind.is_some() && u.run == kind && now - u.at < UNDO_RUN_MS;
    if joins {
        return Undo {
            future: Vec::new(),
            current: next,
            at: now,
            ..u.clone()
        };
    }
    let mut past = u.past.clone();
    past.push(u.current.clone());
    let cut = past.len().saturating_sub(UNDO_LIMIT);
    Undo {
        past: past.split_off(cut),
        future: Vec::new(),
        current: next,
        run: kind,
        at: now,
    }
}

/// A click or an arrow key ends the typing run, so the next keystroke starts a new step.
pub fn break_run(u: &Undo) -> Undo {
    Undo {
        run: None,
        ..u.clone()
    }
}

/// One step back, or None at the start.
pub fn undo(u: &Undo) -> Option<Undo> {
    let prev = u.past.last()?.clone();
    let mut future = vec![u.current.clone()];
    future.extend(u.future.iter().cloned());
    Some(Undo {
        past: u.past[..u.past.len() - 1].to_vec(),
        future,
        current: prev,
        run: None,
        at: 0.0,
    })
}

/// One step forward, or None at the end.
pub fn redo(u: &Undo) -> Option<Undo> {
    let next = u.future.first()?.clone();
    let mut past = u.past.clone();
    past.push(u.current.clone());
    Some(Undo {
        past,
        future: u.future[1..].to_vec(),
        current: next,
        run: None,
        at: 0.0,
    })
}
