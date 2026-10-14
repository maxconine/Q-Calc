// what esc and the keys do around an open game, as plain functions; GameShell keeps the state.
// esc leaves full screen first. in the lobby it steps back, and closes the game from the list. a game on this
// computer pauses on the first esc and leaves to the lobby on the second; a game over the link can't pause the
// other side, so the first esc only warns and a second one soon after leaves

export type ShellMode = 'lobby' | 'local' | 'online'

// how long "esc again to leave" waits for the second esc
export const ARM_MS = 2000

export type EscState = {
  mode: ShellMode
  fullscreen: boolean
  paused: boolean
  // when the first esc of an online game came, or null
  armedAt: number | null
  // nothing is running (a level picker inside the game), so there's nothing to pause
  idle: boolean
  now: number
}

export type EscAction = 'unfullscreen' | 'lobby' | 'pause' | 'arm' | 'leave'

export function escAction(s: EscState): EscAction {
  if (s.fullscreen) return 'unfullscreen'
  if (s.mode === 'lobby') return 'lobby'
  if (s.mode === 'local') return s.paused || s.idle ? 'leave' : 'pause'
  return s.armedAt != null && s.now - s.armedAt <= ARM_MS ? 'leave' : 'arm'
}

type KeyLike = Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey'>

const MODIFIERS = new Set(['Shift', 'Meta', 'Control', 'Alt', 'CapsLock', 'Fn'])

// a key while paused: f flips full screen, esc goes on to the esc handling, shortcuts with ⌘ or ⌃ still work
// (⌃F among them, which useGameFullscreen handles), and any other key resumes (and goes no further, so it
// doesn't also move a paddle)
export function pausedKey(e: KeyLike): 'pass' | 'fullscreen' | 'resume' {
  if (e.key === 'Escape' || e.key === 'Esc' || MODIFIERS.has(e.key)) return 'pass'
  if (e.metaKey || e.ctrlKey || e.altKey) return 'pass'
  if (e.key.toLowerCase() === 'f') return 'fullscreen'
  return 'resume'
}
