// how long a dimmed answer outlives the last keystroke before the slot goes honestly blank
export const STEADY_MS = 1800
// matches the .live-steady opacity transition
export const STEADY_FADE_MS = 260

/** True when `next` reads as `prev` mid-edit (typed on, backspaced, changed in the middle), not new input. */
export function isEditOf(next: string, prev: string): boolean {
  if (!next.trim() || !prev.trim() || next === prev) return false
  let head = 0
  while (head < next.length && head < prev.length && next[head] === prev[head]) head++
  let tail = 0
  while (
    tail < next.length - head &&
    tail < prev.length - head &&
    next[next.length - 1 - tail] === prev[prev.length - 1 - tail]
  ) {
    tail++
  }
  return head + tail >= Math.ceil(prev.length / 2)
}
