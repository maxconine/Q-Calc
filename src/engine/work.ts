// Budgets count work, not milliseconds: the same input always gets the same answer, however busy the machine is.

let done = 0

/** One unit is about one evaluated character of an expression. */
export function spend(units: number): void {
  done += units
}

/** A work budget of `units` from now; `over()` once it's spent. */
export function workBudget(units: number): { over: () => boolean } {
  const until = done + units
  return { over: () => done > until }
}
