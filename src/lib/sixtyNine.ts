// long enough that typing `690` passes through 69 without firing
export const SIXTY_NINE_SETTLE_MS = 200

// fires once per 69, and re-arms only after the settled answer is something else
export function sixtyNineGate(armed: boolean, n: number | undefined): { armed: boolean; fire: boolean } {
  if (n === 69) return { armed: false, fire: armed }
  return { armed: true, fire: false }
}
