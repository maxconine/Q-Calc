import { botCell, type UtSkill } from './ultimateBot'
import { UT_VERSION, canPlay, decodeUt, encodeUt, firstFor, isOver, newGame, other, play, type Side, type UtMsg, type UtState } from './ultimate'

// hi goes out this often until the other side's hi comes in, since its board may not have been listening yet
const HI_MS = 400
// a turn can take a while, so both sides say they're still there this often
export const KEEP_MS = 1000
// nothing from the other side for this long and the game is over
export const UT_SILENCE_MS = 5000
// solo: how long the computer seems to think before it moves
export const BOT_THINK_MS = 500

export type UtView = {
  state: UtState
  // the other side's hi came in, so moves can go
  started: boolean
  // games won so far, host then guest
  wins: [host: number, guest: number]
  again: { mine: boolean; theirs: boolean }
  // set once the game is over for good: the line the lobby shows
  ended: string | null
  // bumps on every change, so the board knows when to redraw
  rev: number
  // solo: the computer is about to move
  thinking: boolean
}

// one side of a game, without the page: the board component feeds it cells, messages and times
export class UltimateSession {
  private game = 0
  private state: UtState = newGame(firstFor(0))
  private wins: [number, number] = [0, 0]
  private greeted = false
  private heard: number
  private lastHi: number
  private lastSent: number
  private again = { mine: false, theirs: false }
  private ended: string | null = null
  private rev = 0

  readonly role: Side
  // a game against the computer: no link, the computer plays the guest
  readonly solo: UtSkill | null
  // solo: when the computer moves next, once it's its turn
  private botAt: number | null = null
  private readonly send: (text: string) => void
  private readonly rand: () => number

  constructor(role: Side, send: (text: string) => void, now: number, solo: UtSkill | null = null, rand: () => number = Math.random) {
    this.role = solo ? 'host' : role
    this.solo = solo
    this.send = send
    this.rand = rand
    this.heard = now
    this.lastHi = now
    this.lastSent = now
    if (solo) {
      this.greeted = true
      this.botTurn(now)
    } else this.hi()
  }

  // solo: if it's the computer's turn, it moves after a moment
  private botTurn(now: number): void {
    this.botAt = this.solo && !isOver(this.state) && this.state.turn !== this.role ? now + BOT_THINK_MS : null
  }

  private hi(): void {
    this.post({ t: 'ut', v: UT_VERSION, ...(this.greeted ? { ok: true } : {}) })
  }

  private post(m: UtMsg): void {
    this.send(encodeUt(m))
  }

  private changed(): void {
    this.rev++
  }

  private end(note: string): void {
    if (this.ended) return
    this.ended = note
    this.changed()
  }

  private apply(cell: number, who: Side): boolean {
    const next = play(this.state, cell, who)
    if (!next) return false
    this.state = next
    if (next.winner) this.wins[next.winner === 'host' ? 0 : 1]++
    this.changed()
    return true
  }

  private restart(now = 0): void {
    this.game++
    this.state = newGame(firstFor(this.game))
    this.again = { mine: false, theirs: false }
    this.botTurn(now)
    this.changed()
  }

  // my turn, an open cell, the other side listening: takes it and sends it
  play(cell: number, now: number): boolean {
    if (this.ended || !this.greeted || !canPlay(this.state, cell, this.role)) return false
    const n = this.state.moves
    this.apply(cell, this.role)
    if (this.solo) {
      this.botTurn(now)
      return true
    }
    this.post({ t: 'utm', g: this.game, n, c: cell })
    this.lastSent = now
    return true
  }

  // enter: asks for a rematch once the game is over; both sides restart when both have asked
  rematch(now: number): void {
    if (this.ended || !isOver(this.state) || this.again.mine) return
    // the computer always wants another
    if (this.solo) {
      this.restart(now)
      return
    }
    this.again = { ...this.again, mine: true }
    this.post({ t: 'uta', g: this.game })
    this.lastSent = now
    if (this.again.theirs) this.restart()
    else this.changed()
  }

  receive(text: string, now: number): void {
    if (this.ended) return
    const m = decodeUt(text)
    if (!m) return
    this.heard = now
    switch (m.t) {
      case 'ut':
        if (m.v !== UT_VERSION) {
          this.post({ t: 'bye' })
          this.end('update both Q Calcs to play each other')
          return
        }
        if (!this.greeted) {
          this.greeted = true
          this.changed()
        }
        if (!m.ok) this.hi()
        return
      case 'other':
        this.post({ t: 'bye' })
        this.end(`your friend opened ${m.game} · both type ultimate`)
        return
      case 'utm':
        // only the next move of this game, from the side whose turn it is; anything else is stale or made up
        if (!this.greeted || m.g !== this.game || m.n !== this.state.moves) return
        this.apply(m.c, other(this.role))
        return
      case 'uta':
        // only an ask for this game, made at its end, counts
        if (m.g !== this.game || !isOver(this.state) || this.again.theirs) return
        this.again = { ...this.again, theirs: true }
        if (this.again.mine) this.restart()
        else this.changed()
        return
      case 'utk':
        return
      case 'bye':
        this.end('your friend left')
    }
  }

  // resends hi until it's answered, keeps the link warm, and notices when the other side has gone quiet
  tick(now: number): UtView {
    if (this.ended) return this.view()
    if (this.solo) {
      if (this.botAt != null && now >= this.botAt) {
        this.botAt = null
        const cell = botCell(this.state, other(this.role), this.solo, this.rand)
        if (cell >= 0) this.apply(cell, other(this.role))
      }
      return this.view()
    }
    if (now - this.heard > UT_SILENCE_MS) {
      this.end('lost the connection')
      return this.view()
    }
    if (!this.greeted && now - this.lastHi >= HI_MS) {
      this.lastHi = now
      this.lastSent = now
      this.hi()
    } else if (now - this.lastSent >= KEEP_MS) {
      this.lastSent = now
      this.post({ t: 'utk' })
    }
    return this.view()
  }

  view(): UtView {
    return { state: this.state, started: this.greeted, thinking: this.botAt != null, wins: [...this.wins], again: this.again, ended: this.ended, rev: this.rev }
  }
}
