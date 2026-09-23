/** How long an answer must hold at 67 before the arms fire (typing `670` passes through 67). */
export const SIXTY_SEVEN_SETTLE_MS = 300

/**
 * Fire-once gate for the 67 arms. `armed` is true until the arms fire, and re-arms only once the
 * settled answer is something other than 67.
 */
export function sixtySevenGate(armed: boolean, n: number | undefined): { armed: boolean; fire: boolean } {
  if (n === 67) return { armed: false, fire: armed }
  return { armed: true, fire: false }
}
