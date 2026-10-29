// whether a name typed after `message` is anyone's, asked of the relay (lookupName) a moment after typing stops
// and kept a while, so a name typed again or a backspace back to it doesn't ask again. only the relay's own
// answers are kept: an ask that couldn't be made says nothing and is tried again next time

// a held name stays held for weeks; a free one can be claimed any minute
export const FOUND_MS = 10 * 60_000
export const MISSING_MS = 30_000
// how long typing rests before a name is asked about
export const CHECK_DELAY_MS = 350
const CACHE_MAX = 200

type Ask = (name: string) => Promise<boolean | null>

export class NameCheck {
  private readonly kept = new Map<string, { found: boolean; at: number }>()
  private readonly asking = new Map<string, Promise<boolean | null>>()
  private readonly ask: Ask
  private readonly now: () => number

  constructor(ask: Ask, now: () => number = Date.now) {
    this.ask = ask
    this.now = now
  }

  // the kept answer, or null when there's none still fresh
  cached(name: string): boolean | null {
    const hit = this.kept.get(name)
    if (!hit) return null
    if (this.now() - hit.at > (hit.found ? FOUND_MS : MISSING_MS)) {
      this.kept.delete(name)
      return null
    }
    return hit.found
  }

  // what a send found out on the way: delivered or offline means someone holds it, unknown that nobody does
  note(name: string, found: boolean): void {
    this.kept.delete(name)
    this.kept.set(name, { found, at: this.now() })
    while (this.kept.size > CACHE_MAX) this.kept.delete(this.kept.keys().next().value!)
  }

  // one ask per name at a time
  check(name: string): Promise<boolean | null> {
    const hit = this.cached(name)
    if (hit != null) return Promise.resolve(hit)
    const pending = this.asking.get(name)
    if (pending) return pending
    const next = this.ask(name)
      .catch(() => null)
      .then((found) => {
        this.asking.delete(name)
        if (found != null) this.note(name, found)
        return found
      })
    this.asking.set(name, next)
    return next
  }
}
