// long enough that typing `670` passes through 67 without firing
export const SIXTY_SEVEN_SETTLE_MS = 200

// fires once per 67, and re-arms only after the settled answer is something else
export function sixtySevenGate(armed: boolean, n: number | undefined): { armed: boolean; fire: boolean } {
  if (n === 67) return { armed: false, fire: armed }
  return { armed: true, fire: false }
}
