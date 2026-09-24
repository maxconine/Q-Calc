// the input's own undo: react and the symbol swaps set the value by hand, which wipes the browser's undo,
// and the mac overlay has no edit menu to send ⌘Z anyway

export type UndoState = { value: string; start: number; end: number }

// `insert` and `delete` runs group like typing does on the mac; anything else is its own step
export type EditKind = 'insert' | 'delete' | null

export type Undo = {
  past: UndoState[]
  future: UndoState[]
  current: UndoState
  // the kind of the run `current` belongs to, null once the run is broken
  run: EditKind
  at: number
}

export const UNDO_RUN_MS = 1000
const UNDO_LIMIT = 200

export function undoStart(value: string): Undo {
  return { past: [], future: [], current: { value, start: value.length, end: value.length }, run: null, at: 0 }
}

export function editKind(inputType: string | undefined): EditKind {
  if (inputType === 'insertText' || inputType === 'insertCompositionText') return 'insert'
  if (inputType === 'deleteContentBackward' || inputType === 'deleteContentForward') return 'delete'
  return null
}

export function recordEdit(u: Undo, next: UndoState, kind: EditKind, now: number): Undo {
  if (next.value === u.current.value) return u
  const joins = kind != null && u.run === kind && now - u.at < UNDO_RUN_MS
  if (joins) return { ...u, future: [], current: next, at: now }
  return { past: [...u.past, u.current].slice(-UNDO_LIMIT), future: [], current: next, run: kind, at: now }
}

// a click or an arrow key ends the typing run, so the next keystroke starts a new step
export function breakRun(u: Undo): Undo {
  return u.run == null ? u : { ...u, run: null }
}

export function undo(u: Undo): Undo | null {
  const prev = u.past[u.past.length - 1]
  if (!prev) return null
  return { past: u.past.slice(0, -1), future: [u.current, ...u.future], current: prev, run: null, at: 0 }
}

export function redo(u: Undo): Undo | null {
  const next = u.future[0]
  if (!next) return null
  return { past: [...u.past, u.current], future: u.future.slice(1), current: next, run: null, at: 0 }
}
