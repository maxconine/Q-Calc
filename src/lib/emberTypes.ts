// ember & frost: a two-player co-op puzzle platformer. this file is the shared contract between the
// engine, the levels, the renderer, the session and the panel. it holds only types and constants, no logic.
// pixel units throughout; the world is a fixed COLS x ROWS grid of TILE px squares, y grows downward

export const EMBER_VERSION = 1

export const TILE = 20
export const COLS = 32
export const ROWS = 18
export const WORLD_W = COLS * TILE // 640
export const WORLD_H = ROWS * TILE // 360

// fixed simulation step. step() always advances exactly one tick
export const STEP_HZ = 60
export const STEP_S = 1 / STEP_HZ

// physics. a jump rises ~63px (just over 3 tiles) and lasts ~0.6s, carrying ~4 tiles sideways at full run.
// level design rule of thumb: climbs of up to 2 tiles are easy, 3 is a stretch; flat gaps of up to 3 tiles
export const PLAYER_W = 14
export const PLAYER_H = 22
export const RUN_SPEED = 150 // px/s
export const RUN_ACCEL = 1400 // px/s² on the ground
export const AIR_ACCEL = 1000 // px/s² in the air
export const GRAVITY = 1400 // px/s²
export const JUMP_V = 420 // px/s, upward
export const MAX_FALL = 620 // px/s
export const COYOTE_TICKS = 6 // a jump still works this many ticks after walking off a ledge
export const JUMP_BUFFER_TICKS = 6 // a jump pressed this many ticks before landing still happens
export const PUSH_SPEED = 70 // px/s, walking while pushing a box
export const BOX_SIZE = TILE
export const POOL_SINK = 6 // a pool's liquid surface sits this many px below its tile's top
export const FAN_ACCEL = 2600 // px/s² upward inside a running fan's column (beats gravity)
export const FAN_MAX_UP = 260 // px/s, the fastest a fan lifts
export const MELT_TICKS = 45 // ticks the fire player must touch an ice block for it to melt away
export const FREEZE_RADIUS = 2.2 * TILE // frost within this (centre to tile centre) freezes 'w' water
export const FREEZE_TICKS = 150 // a frozen 'w' tile stays frozen this long after frost moves away
export const PORTAL_COOLDOWN_TICKS = 24
export const DEATH_TICKS = 72 // after a death the level restarts by itself this many ticks later
export const GATE_SPEED = 2.5 // gate open fraction per second
export const MOVER_SPEED = 2 // default tiles/s for movers

// input bits for one player
export const IN_LEFT = 1
export const IN_RIGHT = 2
export const IN_UP = 4 // jump
export const IN_DOWN = 8 // interact: flips levers, turns mirrors
export type Inputs = [fire: number, frost: number]

export type Element = 'fire' | 'frost'
// player index: 0 is always fire (ember), 1 is always frost
export type PlayerIndex = 0 | 1
export const ELEMENTS: readonly [Element, Element] = ['fire', 'frost']

// tiles
export const T_EMPTY = 0
export const T_WALL = 1
export const T_LAVA = 2 // fire safe, kills frost
export const T_WATER = 3 // frost safe, kills fire
export const T_GOO = 4 // kills both
export const T_THIN = 5 // freezable water: like water, but frozen solid & safe for all while frost is near
export type Tile = 0 | 1 | 2 | 3 | 4 | 5

// map characters, one per tile, rows of exactly COLS chars, exactly ROWS rows
//   '#' wall        '.' or ' ' empty
//   'L' lava pool   'W' water pool   'G' goo pool   'w' thin (freezable) water
//   '1' fire spawn  '2' frost spawn  (the tile the player's feet stand in; they stand on the tile below)
//   'r' fire gem    'b' frost gem    (centred in the tile)
//   'R' fire door   'B' frost door   (the door's BOTTOM tile; doors are 1 tile wide, 2 tall, standing on the tile below)
// pools are solid like walls but their surface sits POOL_SINK px lower, so a player standing on one sinks in.
// every other mechanic goes in LevelDef.things

// a condition: one channel name, or several that must all be on. a leading '!' means "this channel is off"
export type Cond = string | string[]
export type Dir = 'up' | 'down' | 'left' | 'right'
export type Slant = '/' | '\\'

// things, in TILE coordinates (x, y = the top-left tile; w, h in tiles)
export type ThingDef =
  // pressure plate on the floor of tile row y (draws on the bottom few px of that row). on while any player or box rests on it
  | { k: 'plate'; x: number; y: number; w?: number; ch: string }
  // lever standing in tile (x, y). the down key while touching it flips it. starts `on` if given
  | { k: 'lever'; x: number; y: number; ch: string; on?: boolean }
  // timed button on the floor of tile row y: stepping on it turns ch on, it stays on `secs` after being left
  | { k: 'button'; x: number; y: number; w?: number; ch: string; secs: number }
  // a solid gate that slides `dir` (default up) by its own length while `open` holds, and back when it doesn't
  | { k: 'gate'; x: number; y: number; w: number; h: number; open: Cond; dir?: Dir }
  // a solid moving platform: rests at (x, y) while `when` is false and travels to `to` while it's true;
  // with no `when` it shuttles back and forth forever. carries whatever stands on it. speed in tiles/s
  | { k: 'mover'; x: number; y: number; w: number; h: number; to: { x: number; y: number }; when?: Cond; speed?: number }
  // a pushable 1x1 crate with gravity. players can stand on it; it presses plates and blocks beams
  | { k: 'box'; x: number; y: number }
  // a fan in the floor at tile (x..x+w-1, y) blowing up through the h tiles above it (y-h+1 .. y). runs while `when` holds (always if absent)
  | { k: 'fan'; x: number; y: number; w: number; h: number; when?: Cond }
  // solid ice that melts away for good after the fire player touches it for MELT_TICKS
  | { k: 'ice'; x: number; y: number; w: number; h: number }
  // a light emitter in tile (x, y) shining `dir` while `when` holds (always if absent)
  | { k: 'emitter'; x: number; y: number; dir: Dir; when?: Cond }
  // a mirror in tile (x, y). '/' turns a beam heading right to up, '\\' turns right to down. the down key turns it
  | { k: 'mirror'; x: number; y: number; slant: Slant }
  // a light sensor in tile (x, y): ch is on while a beam reaches it
  | { k: 'sensor'; x: number; y: number; ch: string }
  // a portal, 1 tile wide and 2 tall, whose BOTTOM tile is (x, y). walking in sends a player out of the other portal with the same pair
  | { k: 'portal'; x: number; y: number; pair: string }
  // a floating tutorial sign drawn at tile (x, y)
  | { k: 'sign'; x: number; y: number; text: string }

export type LevelDef = {
  id: string // stable, used for saved progress; never renumber
  name: string
  // one short line shown when the level starts
  hint?: string
  // seconds for the time star
  par: number
  map: string[]
  things: ThingDef[]
}

export type Rect = { x: number; y: number; w: number; h: number }

// a compiled level, in px. arrays keep LevelDef order, and GameState arrays index into them
export type Level = {
  def: LevelDef
  tiles: Tile[] // ROWS*COLS, row-major: tiles[ty * COLS + tx]
  spawns: [{ x: number; y: number }, { x: number; y: number }] // player top-left px
  doors: [Rect, Rect] // [fire door, frost door]
  gems: Array<{ x: number; y: number; el: Element }> // gem centres
  plates: Array<Rect & { ch: string }>
  levers: Array<Rect & { ch: string; on: boolean }>
  buttons: Array<Rect & { ch: string; ticks: number }>
  gates: Array<Rect & { open: Cond; dir: Dir }>
  movers: Array<Rect & { to: { x: number; y: number }; when?: Cond; speed: number }> // speed in px/s
  boxes: Array<{ x: number; y: number }> // start positions
  fans: Array<Rect & { when?: Cond }> // the whole column the fan blows through, fan tile included
  ice: Rect[]
  emitters: Array<{ x: number; y: number; dir: Dir; when?: Cond }> // tile-centre px
  mirrors: Array<{ x: number; y: number; slant: Slant }> // tile-centre px, starting slant
  sensors: Array<Rect & { ch: string }>
  portals: Array<Rect & { pair: string; other: number }> // other = index of the paired portal
  signs: Array<{ x: number; y: number; text: string }>
  thin: number[] // tile indices of 'w' tiles, in row-major order
}

export type Player = {
  x: number // top-left px
  y: number
  vx: number
  vy: number
  ground: boolean
  face: 1 | -1
  alive: boolean
  inDoor: boolean
  coyote: number // ticks of coyote time left
  buffer: number // ticks of buffered jump left
  prevIn: number // last tick's input bits, for edge detection
  portalCd: number
  gems: number // collected this attempt
}

export type DeathCause = 'lava' | 'water' | 'goo'

export type GameEvent =
  | { k: 'jump' | 'land' | 'die' | 'door' | 'portal' | 'push'; p: PlayerIndex; x: number; y: number; cause?: DeathCause }
  | { k: 'gem'; p: PlayerIndex; x: number; y: number; el: Element }
  | { k: 'lever' | 'mirror'; i: number; x: number; y: number }
  | { k: 'plate' | 'button' | 'sensor'; i: number; on: boolean; x: number; y: number }
  | { k: 'melt'; i: number; x: number; y: number }
  | { k: 'freeze' | 'thaw'; tile: number; x: number; y: number }
  | { k: 'win' }

export type Phase = 'play' | 'dead' | 'won'

// everything that changes. plain JSON (no Sets/Maps/typed arrays) so it can be cloned and sent as is
export type GameState = {
  tick: number // ticks played this attempt (the clock)
  phase: Phase
  phaseT: number // ticks spent in the current phase
  deaths: number // across restarts of this level
  players: [Player, Player]
  taken: boolean[] // per level.gems
  levers: boolean[]
  buttons: number[] // ticks of on-time left per button
  gates: number[] // open fraction 0..1 per gate
  movers: Array<{ x: number; y: number; t: number; dir: 1 | -1 }> // top-left px; t is progress 0..1 along the path
  boxes: Array<{ x: number; y: number; vx: number; vy: number }>
  ice: number[] // melt progress per ice block, 0..1; 1 means gone
  thin: number[] // ticks of frozen left per level.thin tile; > 0 means frozen
  mirrors: Slant[]
  on: string[] // channels on after the last step, sorted
  events: GameEvent[] // what happened during the last step only
}

export type Beam = { x1: number; y1: number; x2: number; y2: number }

export type Score = { secs: number; gems: [fire: number, frost: number]; total: [fire: number, frost: number]; stars: number }
