// the clock games run on: performance.now and Date.now with the paused stretches taken out. pausing a game
// stops this clock, so whatever reads it (frame loops, countdowns, the computer's thinking time) stands still
// without each game knowing about pauses

export class GameClock {
  private stoppedAt: number | null = null
  private lost = 0
  private readonly mono: () => number
  private readonly wall: () => number

  constructor(mono: () => number = () => performance.now(), wall: () => number = () => Date.now()) {
    this.mono = mono
    this.wall = wall
  }

  get paused(): boolean {
    return this.stoppedAt != null
  }

  // performance.now on this clock
  now(): number {
    return (this.stoppedAt ?? this.mono()) - this.lost
  }

  // Date.now on this clock, for games that count down in wall time
  date(): number {
    return this.wall() - (this.paused ? this.mono() - this.stoppedAt! : 0) - this.lost
  }

  pause(): void {
    if (this.stoppedAt == null) this.stoppedAt = this.mono()
  }

  resume(): void {
    if (this.stoppedAt == null) return
    this.lost += this.mono() - this.stoppedAt
    this.stoppedAt = null
  }
}

const shared = new GameClock()

export const gameNow = (): number => shared.now()
export const gameDateNow = (): number => shared.date()
export const pauseGameClock = (): void => shared.pause()
export const resumeGameClock = (): void => shared.resume()
export const gameClockPaused = (): boolean => shared.paused
