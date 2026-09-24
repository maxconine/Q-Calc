// long enough that typing `4200` passes through 420 without firing
export const FOUR_TWENTY_SETTLE_MS = 200

// fires once per 420, and re-arms only after the settled answer is something else
export function fourTwentyGate(armed: boolean, n: number | undefined): { armed: boolean; fire: boolean } {
  if (n === 420) return { armed: false, fire: armed }
  return { armed: true, fire: false }
}
