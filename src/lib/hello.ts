// every linked game opens with a hello tagged with its game, so a friend who opened another game is told which.
// older builds sent pong's and ember's hellos as plain `hi` (ember's with u, or w from ember 2), connect 4's as `c4` and
// ultimate's as `ut` and trails' as `tr`, so those still read as theirs

export type LinkedGame = 'pong' | 'connect4' | 'ultimate' | 'ember' | 'trails'

const NAMES: Record<LinkedGame, string> = { pong: 'pong', connect4: 'connect 4', ultimate: 'ultimate', ember: 'ember', trails: 'trails' }

const LINKED = new Set<string>(Object.keys(NAMES))

// the game a decoded message's hello belongs to, or null when it isn't a hello
export function helloGame(o: Record<string, unknown>): LinkedGame | null {
  if (typeof o.g === 'string' && LINKED.has(o.g) && (o.t === 'hi' || o.t === 'c4' || o.t === 'ut' || o.t === 'tr')) return o.g as LinkedGame
  if (o.t === 'c4') return 'connect4'
  if (o.t === 'tr') return 'trails'
  if (o.t === 'ut') return 'ultimate'
  if (o.t === 'hi') return 'u' in o || 'w' in o ? 'ember' : 'pong'
  return null
}

// another game's hello, as the name to show; null for this game's own or for anything else
export function otherHello(o: Record<string, unknown>, mine: LinkedGame): string | null {
  const g = helloGame(o)
  return g && g !== mine ? NAMES[g] : null
}

export function otherGameNote(other: string, mine: LinkedGame): string {
  return `your friend opened ${other} · both type ${NAMES[mine]}`
}
