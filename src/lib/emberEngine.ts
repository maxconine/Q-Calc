import type { Beam, GameState, Inputs, Level, LevelDef, Score, Tile } from './emberTypes'

// ember & frost's simulation: pure and deterministic. no DOM, no Math.random, no Date.
// STUB: the engine agent replaces the bodies; the signatures are the contract

// turns a LevelDef into px geometry. throws on a malformed map (wrong size, missing spawn or door)
export function compileLevel(def: LevelDef): Level {
  void def
  throw new Error('todo')
}

// everything wrong with a level definition, as readable lines; [] when it compiles cleanly
export function validateLevel(def: LevelDef): string[] {
  void def
  return []
}

// a fresh attempt at a level. deaths carries the death count across restarts
export function initState(level: Level, deaths = 0): GameState {
  void level
  void deaths
  throw new Error('todo')
}

// advances exactly one STEP_S tick. pure: never mutates `s`, returns a new state whose `events` are this tick's only.
// in phase 'dead' it counts DEATH_TICKS then returns initState(level, deaths + 1); in 'won' it only counts phaseT
export function step(level: Level, s: GameState, input: Inputs): GameState {
  void level
  void input
  return s
}

// the tile at (tx, ty) as it currently behaves: outside the map is T_WALL; a frozen thin tile reads as T_WALL
export function tileAt(level: Level, s: GameState, tx: number, ty: number): Tile {
  void level
  void s
  void tx
  void ty
  return 0
}

// light beam segments for the current state, emitter to wherever each stops (for drawing)
export function traceBeams(level: Level, s: GameState): Beam[] {
  void level
  void s
  return []
}

// current gate rect (after sliding) for drawing and collision
export function gateRect(level: Level, s: GameState, i: number): { x: number; y: number; w: number; h: number } {
  void s
  const g = level.gates[i]!
  return { x: g.x, y: g.y, w: g.w, h: g.h }
}

// stars: 1 for finishing, +1 for every gem, +1 for secs <= par
export function scoreOf(level: Level, s: GameState): Score {
  void level
  return { secs: s.tick / 60, gems: [0, 0], total: [0, 0], stars: 0 }
}
