// ember & frost: a two-player co-op puzzle platformer. this file is the shared contract between the
// engine, the levels, the camera, the renderer, the session and the panel. it holds only types and constants,
// no logic. pixel units throughout; y grows downward.
//
// version 2: a level is a list of STAGES played back to back with one clock. every stage is its own tile map
// of any size from one screen up to four across and four down, with its own things. the camera (emberCamera.ts)
// follows both players over it. deaths go back to the last checkpoint, not to the start.
// who builds what is in EMBER2_PLAN.md

// goes in the online hello: both sides must run the same rules and the same levels
export const EMBER_VERSION = 2
// the LevelDef format below. bump when a map glyph or thing changes meaning
export const LEVEL_FORMAT = 2

export const TILE = 20

// the VIEW: what one screen shows at zoom 1. nothing may assume the canvas is this size or this shape:
// the canvas is whatever the panel gives it (a small pane, a tall split, fullscreen) and the camera fits a
// rect of the canvas's aspect around the players
export const VIEW_COLS = 32
export const VIEW_ROWS = 18
export const VIEW_W = VIEW_COLS * TILE // 640
export const VIEW_H = VIEW_ROWS * TILE // 360

// a stage's map: at least one view, at most four across and four down. at the camera's widest zoom the
// whole of the biggest stage fits, so both players are always on screen
export const MIN_COLS = VIEW_COLS
export const MIN_ROWS = VIEW_ROWS
export const MAX_COLS = 4 * VIEW_COLS // 128
export const MAX_ROWS = 4 * VIEW_ROWS // 72
export const MAX_STAGES = 6

// fixed simulation step. step() always advances exactly one tick. nothing in the sim may use randomness,
// wall-clock time or iteration order that could differ between two computers
export const STEP_HZ = 60
export const STEP_S = 1 / STEP_HZ

// physics. a jump rises ~63px (just over 3 tiles) and lasts ~0.6s, carrying ~4 tiles sideways at full run.
// level design rule of thumb: climbs of up to 2 tiles are easy, 3 is a stretch; flat gaps of up to 3 tiles
export const PLAYER_W = 14
export const PLAYER_H = 22
export const RUN_SPEED = 150 // px/s
export const RUN_ACCEL = 1400 // px/s² on the ground
export const AIR_ACCEL = 1000 // px/s² in the air
export const GRAVITY = 1400 // px/s², along the body's own gravity (Player.grav)
export const JUMP_V = 420 // px/s, against gravity
export const MAX_FALL = 620 // px/s, along gravity
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
export const GATE_SPEED = 2.5 // gate open fraction per second
export const MOVER_SPEED = 2 // default tiles/s for movers

// ---- stages, deaths and checkpoints
export const DEATH_TICKS = 72 // after a death both players come back at the last checkpoint this many ticks later
export const STAGE_TICKS = 70 // both in their doors on a stage that isn't the last: phase 'clear' for this long, then the next stage
// a checkpoint's default zone, in tiles: this wide, centred on the flag's column, and this tall, ending on the flag's row
export const CHECKPOINT_ZONE_W = 3
export const CHECKPOINT_ZONE_H = 3

// ---- lights
// in the dark each player lights a circle of this radius around their centre; ember burns brighter.
// the renderer also lets lava, beams, gems, lit checkpoints and lamps glow at GLOW_SMALL. purely visual:
// the only thing lights change in the sim is which of the 'O' and 'D' blocks are solid. a body inside a block
// that turns solid is pushed out against its gravity, up to LIGHT_EJECT_TILES; with no room there it is crushed
export const GLOW_EMBER = 5.5 * TILE
export const GLOW_FROST = 3 * TILE
export const GLOW_SMALL = 1.5 * TILE
export const LIGHT_EJECT_TILES = 2
// the reserved channel that is on while the lights are on, so a gate, mover, fan, emitter or cannon can follow
// them ('lights' or '!lights' in a Cond). no plate, lever, button or sensor may drive it
export const CH_LIT = 'lights'

// ---- trampolines: a body landing on one leaves at min(TRAMP_MAX_V, max(TRAMP_MIN_V, impact * TRAMP_GAIN + TRAMP_ADD))
// where impact is its speed into the pad. stepping onto one still bounces at TRAMP_MIN_V (~3.8 tiles);
// a fall of 5 tiles gives ~6.3 tiles, terminal velocity ~8.2. nobody can stand still on one. letting go of
// jump while rising cuts a bounce short just as it cuts a jump; boxes don't bounce
export const TRAMP_MIN_V = 460
export const TRAMP_GAIN = 0.9
export const TRAMP_ADD = 120
export const TRAMP_MAX_V = 700

// ---- double-jump arrows: a floating pickup. a player touching one in the air (Player.air 0) takes it and
// holds one extra jump, spent by the next jump press in mid-air or lost on landing. the arrow comes back later
export const AIR_JUMP_V = 380 // px/s, a little under a ground jump (~2.6 tiles)
export const ARROW_RESPAWN_TICKS = 150
export const ARROW_BOX = 14 // the pickup's hit square, centred in its tile

// ---- one-way platforms: a slab over the top ONEWAY_H px of the tile. solid only to a body moving along its own
// gravity whose feet were on the far side at the start of the move: an upright body lands on its top and
// jumps up through it; a flipped body lands on its underside and jumps down through it. never solid sideways
export const ONEWAY_H = 6

// ---- crumbling blocks: solid; CRUMBLE_TICKS after a player first stands on (or, flipped, under) one it
// breaks, whether they stayed or not, and CRUMBLE_GONE_TICKS later it grows back once nothing is inside it
export const CRUMBLE_TICKS = 36
export const CRUMBLE_GONE_TICKS = 180

// ---- cannons: a player whose centre enters an empty cannon's tile is loaded: held at its centre, no control,
// safe from everything. it fires when its `fire` condition holds, or by itself CANNON_FUSE_TICKS after loading
// if it has none. the shot leaves at `power` along `aim`; the player then flies with gravity but no steering
// until they land, hit something or CANNON_FLY_TICKS pass
export const CANNON_POWER = 26 // default tiles/s (520 px/s: straight up ~4.8 tiles, at 45° ~9.5 tiles across)
export const CANNON_FUSE_TICKS = 40
export const CANNON_FLY_TICKS = 90
export const CANNON_COOLDOWN_TICKS = 30 // a player just fired can't be loaded again for this long

// ---- camera (emberCamera.ts)
export const CAM_MARGIN_X = 6 * TILE // room kept around the players sideways
export const CAM_MARGIN_Y = 4 * TILE // and above and below
export const CAM_MIN_ZOOM = 0.25 // the widest view: four screens across
export const CAM_EASE = 0.12 // per tick, how fast the drawn camera closes on its target

// input bits for one player
export const IN_LEFT = 1
export const IN_RIGHT = 2
export const IN_UP = 4 // jump (away from the surface the player stands on, whichever way gravity points)
export const IN_DOWN = 8 // interact: flips levers, lamps and gravity switches, turns mirrors
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
export const T_TRAMP = 6 // trampoline: solid, and bounces whatever lands on its top (or, flipped, its underside)
export const T_ONEWAY = 7 // one-way platform
export const T_CRUMBLE = 8 // crumbling block
export const T_LIGHT = 9 // solid only while the lights are on
export const T_DARK = 10 // solid only while the lights are off
export type Tile = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10

// ---- the map: one character per tile. every row of a stage has the same length (MIN_COLS..MAX_COLS) and
// there are MIN_ROWS..MAX_ROWS rows. shipped levels wall the outermost ring with '#' (the level tests check;
// the engine itself treats everything outside the map as wall). this table is every glyph there is:
//
//   glyph  what                     notes
//   '#'    wall
//   '.' ' ' empty
//   'L'    lava pool                ember wades it, frost dies
//   'W'    water pool               frost wades it, ember dies
//   'G'    goo pool                 kills both
//   'w'    thin water               water that freezes solid while frost is within FREEZE_RADIUS
//   'T'    trampoline               solid; see TRAMP_*
//   '='    one-way platform         see ONEWAY_H
//   '~'    crumbling block          see CRUMBLE_*
//   'O'    light-only block         solid while the lights are On, air in the dark
//   'D'    dark-only block          solid in the Dark, air while the lights are on
//   '1'    fire spawn               exactly one per stage. the tile the feet stand in; they stand on the tile below
//   '2'    frost spawn              exactly one per stage
//   'R'    fire door                exactly one per stage: the door's BOTTOM tile; 1 wide, 2 tall, standing on the tile below
//   'B'    frost door               exactly one per stage
//   'r'    fire gem                 centred in the tile
//   'b'    frost gem
//   '^'    double-jump arrow        see AIR_JUMP_V
//   'k'    key                      either player takes it; the team carries it to a 'lock' thing
//   'C'    checkpoint flag          standing in this tile, with the default zone (CHECKPOINT_ZONE_*)
//
// pools are solid like walls but their surface sits POOL_SINK px lower, so a player standing on one sinks in.
// pools only harm (or carry) a body standing on their top: from the side and from below they are plain walls,
// so a gravity-flipped player can walk the underside of a lava ceiling.
// both players in their doors ends the stage; on the last stage that wins the level.
// every other mechanic goes in StageDef.things

// a condition: one channel name, or several that must all be on. a leading '!' means "this channel is off".
// channels are driven by plates, levers, buttons and sensors; CH_LIT is driven by the lights
export type Cond = string | string[]
export type Dir = 'up' | 'down' | 'left' | 'right'
export type Aim = Dir | 'upleft' | 'upright' | 'downleft' | 'downright'
export type Slant = '/' | '\\'
// whose gravity a switch flips: one element, both, whoever pressed it, or the one who didn't
export type Who = Element | 'both' | 'self' | 'other'

// things, in TILE coordinates (x, y = the top-left tile; w, h in tiles)
export type ThingDef =
  // pressure plate on the floor of tile row y (draws on the bottom few px of that row). on while any player or box rests on it.
  // ceil: on the ceiling of row y instead (the top few px), for a gravity-flipped player to stand on
  | { k: 'plate'; x: number; y: number; w?: number; ch: string; ceil?: boolean }
  // lever standing in tile (x, y). the down key while touching it flips it. starts `on` if given
  | { k: 'lever'; x: number; y: number; ch: string; on?: boolean }
  // timed button on the floor of tile row y: stepping on it turns ch on, it stays on `secs` after being left. ceil as for plates
  | { k: 'button'; x: number; y: number; w?: number; ch: string; secs: number; ceil?: boolean }
  // a solid gate that slides `dir` (default up) by its own length while `open` holds, and back when it doesn't
  | { k: 'gate'; x: number; y: number; w: number; h: number; open: Cond; dir?: Dir }
  // a solid moving platform: rests at (x, y) while `when` is false and travels to `to` while it's true;
  // with no `when` it shuttles back and forth forever. carries whatever stands on it. speed in tiles/s
  | { k: 'mover'; x: number; y: number; w: number; h: number; to: { x: number; y: number }; when?: Cond; speed?: number }
  // a pushable 1x1 crate with gravity (always downward). players can stand on it; it presses plates and blocks beams
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
  // a checkpoint flag standing in tile (x, y), like the 'C' glyph but with its own zone (tiles). it is taken the
  // first tick BOTH players are alive, on the ground, not in a cannon, and overlapping the zone
  | { k: 'checkpoint'; x: number; y: number; zone?: { x: number; y: number; w: number; h: number } }
  // the light switch, in tile (x, y): the down key while touching it turns the stage's lights on or off.
  // with secs, the lights fall back to how the stage started that long after the press
  | { k: 'lamp'; x: number; y: number; secs?: number }
  // a gravity switch in tile (x, y): the down key while touching it flips gravity for `who` (default 'self')
  | { k: 'gswitch'; x: number; y: number; who?: Who }
  // a solid locked block. a player touching it while the team carries a key spends the key and opens it for good
  | { k: 'lock'; x: number; y: number; w: number; h: number }
  // a cannon in tile (x, y). power in tiles/s (default CANNON_POWER). fire: see CANNON_*
  | { k: 'cannon'; x: number; y: number; aim: Aim; power?: number; fire?: Cond }
  // a patch of darkness (tiles). with none of these a dark stage is dark all over; with some, only inside them.
  // visual only: 'O' and 'D' blocks follow the lights wherever they are
  | { k: 'dark'; x: number; y: number; w: number; h: number }

export type StageDef = {
  // shown small when the stage begins (multi-stage levels)
  name?: string
  // one short line shown when the stage starts
  hint?: string
  // the lights start off (default on)
  dark?: boolean
  map: string[]
  things: ThingDef[]
}

export type LevelDef = {
  id: string // stable, used for saved progress; never renumber or reuse
  name: string
  // one short line shown when the level starts
  hint?: string
  // seconds for the time star, over all stages (the clock runs through deaths and stage changes)
  par: number
  // 1..MAX_STAGES, played in order
  stages: StageDef[]
}

export type Rect = { x: number; y: number; w: number; h: number }

// a compiled stage, in px. arrays keep map order (row-major) then StageDef.things order, and GameState arrays index into them
export type Stage = {
  index: number // which stage of its level
  def: StageDef
  cols: number
  rows: number
  w: number // cols * TILE
  h: number // rows * TILE
  tiles: Tile[] // rows*cols, row-major: tiles[ty * cols + tx]
  dark: boolean // lights start off
  spawns: [{ x: number; y: number }, { x: number; y: number }] // player top-left px
  doors: [Rect, Rect] // [fire door, frost door]
  gems: Array<{ x: number; y: number; el: Element }> // gem centres
  plates: Array<Rect & { ch: string; ceil: boolean }>
  levers: Array<Rect & { ch: string; on: boolean }>
  buttons: Array<Rect & { ch: string; ticks: number; ceil: boolean }>
  gates: Array<Rect & { open: Cond; dir: Dir }>
  movers: Array<Rect & { to: { x: number; y: number }; when?: Cond; speed: number }> // speed in px/s
  boxes: Array<{ x: number; y: number }> // start positions
  fans: Array<Rect & { when?: Cond }> // the whole column the fan blows through, fan tile included
  ice: Rect[]
  emitters: Array<{ x: number; y: number; dir: Dir; when?: Cond }> // tile-centre px
  mirrors: Array<{ x: number; y: number; slant: Slant }> // tile-centre px, starting slant
  sensors: Array<Rect & { ch: string }>
  portals: Array<Rect & { pair: string; other: number }> // other = index of the paired portal
  signs: Array<{ x: number; y: number; text: string }> // tile-centre px
  thin: number[] // tile indices of 'w' tiles, in row-major order
  crumble: number[] // tile indices of '~' tiles, in row-major order
  checkpoints: Array<{ x: number; y: number; zone: Rect }> // x, y: the middle of the flag's foot (bottom centre of its tile)
  lamps: Array<Rect & { ticks: number }> // ticks: 0 for a plain toggle, else how long a press lasts
  gswitches: Array<Rect & { who: Who }>
  arrows: Array<{ x: number; y: number }> // centres
  keys: Array<{ x: number; y: number }> // centres
  locks: Rect[]
  cannons: Array<{ x: number; y: number; aim: Aim; vx: number; vy: number; fire?: Cond }> // tile-centre px; vx, vy: the shot, px/s
  darks: Rect[] // 'dark' patches; empty means the whole stage
}

// a compiled level
export type Level = {
  def: LevelDef
  stages: Stage[]
  gemTotals: [fire: number, frost: number] // over all stages
}

export type Player = {
  x: number // top-left px
  y: number
  vx: number
  vy: number
  ground: boolean // standing on something, on whichever side gravity points
  face: 1 | -1
  alive: boolean
  inDoor: boolean
  coyote: number // ticks of coyote time left
  buffer: number // ticks of buffered jump left
  prevIn: number // last tick's input bits, for edge detection
  portalCd: number
  gems: number // collected in this stage since it began (as of the last checkpoint, after a death)
  grav: 1 | -1 // 1: falls down. -1: falls up and stands on ceilings (drawn upside down)
  air: number // extra mid-air jumps held from arrows: 0 or 1
  cannon: number // index of the cannon this player is loaded in, or -1
  fly: number // ticks of cannon flight left (no steering); 0 otherwise
  cannonCd: number // ticks until a cannon will take this player again
}

// crush: caught inside a light-only or dark-only block when the lights changed, with nowhere to be pushed out to
export type DeathCause = 'lava' | 'water' | 'goo' | 'crush'

export type GameEvent =
  // bounce: off a trampoline (v = the speed it left at, for pitch). airjump: an arrow's jump spent
  | { k: 'jump' | 'land' | 'die' | 'door' | 'portal' | 'push' | 'airjump'; p: PlayerIndex; x: number; y: number; cause?: DeathCause }
  | { k: 'bounce'; p: PlayerIndex; x: number; y: number; v: number }
  | { k: 'gem'; p: PlayerIndex; x: number; y: number; el: Element }
  // arrow: p took arrow i. key: p took key i. load / fire: p went into / was shot out of cannon i
  | { k: 'arrow' | 'key' | 'load' | 'fire'; p: PlayerIndex; i: number; x: number; y: number }
  // flip: p's gravity is now grav
  | { k: 'flip'; p: PlayerIndex; x: number; y: number; grav: 1 | -1 }
  // gswitch: gravity switch i was pressed. arrowback: arrow i is back. unlock: lock i opened. checkpoint: flag i was taken
  | { k: 'lever' | 'mirror' | 'gswitch' | 'arrowback' | 'unlock' | 'checkpoint'; i: number; x: number; y: number }
  | { k: 'plate' | 'button' | 'sensor'; i: number; on: boolean; x: number; y: number }
  // light: the lights went on or off; x, y is the lamp pressed, or the stage's middle when a timed lamp ran out
  | { k: 'light'; on: boolean; x: number; y: number }
  | { k: 'melt'; i: number; x: number; y: number }
  // shake: a crumbling block was stepped on. break: it fell away. mend: it grew back
  | { k: 'freeze' | 'thaw' | 'shake' | 'break' | 'mend'; tile: number; x: number; y: number }
  // clear: stage `stage` is done (phase 'clear' begins). stage: stage `stage` has just begun.
  // respawn: both players are back after a death; cp is the checkpoint they came back to, -1 for the stage's start
  | { k: 'clear' | 'stage'; stage: number }
  | { k: 'respawn'; cp: number }
  | { k: 'win' }

// play: the game runs. dead: someone died; counts DEATH_TICKS, then respawns. clear: a stage is done; counts
// STAGE_TICKS, then the next begins. won: the last stage is done
export type Phase = 'play' | 'dead' | 'clear' | 'won'

// everything that changes. plain JSON (no Sets/Maps/typed arrays) so it can be cloned and sent as is.
// every per-thing array indexes the CURRENT stage's arrays (level.stages[stage]) and is rebuilt when the stage changes
export type GameState = {
  tick: number // the clock: ticks spent in phase 'play' since the level began, over all stages and deaths
  phase: Phase
  phaseT: number // ticks spent in the current phase
  deaths: number // since the level began
  stage: number // index into level.stages
  banked: [fire: number, frost: number] // gems taken in the stages already finished
  players: [Player, Player]
  taken: boolean[] // per stage.gems
  levers: boolean[]
  buttons: number[] // ticks of on-time left per button
  gates: number[] // open fraction 0..1 per gate
  movers: Array<{ x: number; y: number; t: number; dir: 1 | -1 }> // top-left px; t is progress 0..1 along the path
  boxes: Array<{ x: number; y: number; vx: number; vy: number }>
  ice: number[] // melt progress per ice block, 0..1; 1 means gone
  thin: number[] // ticks of frozen left per stage.thin tile; > 0 means frozen
  mirrors: Slant[]
  lit: boolean // the lights are on
  litT: number // ticks until a timed lamp press runs out and `lit` goes back to !stage.dark; 0 when none is running
  arrows: number[] // per stage.arrows: 0 = there to take, else ticks until it comes back
  // per stage.crumble: 0 whole; 1..CRUMBLE_TICKS shaking (counting up); above that gone, counting on up to
  // CRUMBLE_TICKS + CRUMBLE_GONE_TICKS, where it waits until nothing is inside it and goes back to 0
  crumble: number[]
  keys: number // keys the team carries
  keyTaken: boolean[] // per stage.keys
  locks: boolean[] // per stage.locks: opened
  cannons: number[] // per stage.cannons: ticks its player has been loaded, 0 when empty
  cp: number // index into stage.checkpoints of the last one taken in this stage, -1 for none
  // the world as it was when checkpoint `cp` was taken: what a death goes back to. null or absent: the stage's
  // start. never mutated, shared by reference from state to state, and NOT sent over the network: the host
  // alone respawns, and a guest only ever sees the result
  saved?: Snapshot | null
  on: string[] // channels on after the last step, sorted. never lists CH_LIT: `lit` says that
  events: GameEvent[] // what happened during the last step only
  // per stage.plates / stage.sensors: pressed / lit as of the last step (the engine fills these; they let it report changes)
  plates?: boolean[]
  sensors?: boolean[]
}

// a checkpoint's copy of the world. a respawn is this state again with the clock (tick) of the moment of death,
// deaths + 1, phase 'play', and every prevIn set to all-held so a key still down doesn't count as a fresh press
export type Snapshot = Omit<GameState, 'saved' | 'events'>

export type Beam = { x1: number; y1: number; x2: number; y2: number }

// gems and total count every stage of the level. stars: 1 for finishing, +1 for every gem, +1 for secs <= par
export type Score = { secs: number; gems: [fire: number, frost: number]; total: [fire: number, frost: number]; stars: number }

// ---- camera

// the part of the stage on screen, in world px. w / h is the canvas's aspect. zoom is 1 when the rect is one
// VIEW (or the smallest rect of that aspect holding one) and falls towards CAM_MIN_ZOOM as the players spread.
// x or y is negative, and the rect overhangs the stage evenly, when the stage is smaller than the rect that way:
// the renderer fills the overhang with wall
export type Camera = { x: number; y: number; w: number; h: number; zoom: number }

// where the camera wants to be for a state: pure, no smoothing, no memory. aspect = canvas width / height.
// it always holds both players' bodies. emberCamera.ts has the one implementation (cameraFor) and the easing
// the renderer uses between frames (easeCamera)
export type CameraFn = (stage: Stage, s: GameState, aspect: number) => Camera

// ---- the session's face to the panel (emberSession.ts implements these)

export type EmberScreen = 'wait' | 'select' | 'play'

export type EmberView = {
  mode: 'local' | 'online'
  screen: EmberScreen
  level: number // index into LEVELS: the one being played, or the one highlighted on select. every level can be picked
  compiled: Level | null // compiled LEVELS[level] while playing; the stage on screen is compiled.stages[state.stage]
  state: GameState | null // latest state to draw (online guest: predicted, corrections eased in)
  // the state one step before `state`, for interpolation. when the stage changed or the players respawned
  // in between, prev === state, so nothing is ever blended across a jump
  prev: GameState | null
  alpha: number // 0..1, how far between prev and state this frame sits
  events: GameEvent[] // everything that happened since the last tick() call
  me: PlayerIndex | null // online: the character this side plays; local: null (both)
  // online: the line the lobby shows once the link is over
  ended: string | null
  // online: something the other side did or asked for, shown small (e.g. 'frost restarted')
  note: string | null
}

export interface EmberGame {
  tick(now: number): EmberView
  // held keys. local: per player; online: the player argument is ignored and this side's character is used
  keys(player: PlayerIndex, bits: number): void
  // drop all held keys (window blur)
  release(): void
  // select screen: move the highlight / start the highlighted level
  pick(index: number): void
  play(): void
  // in play: back to the last checkpoint, as a death would (counts as one). after a win: the level again from the top
  retry(): void
  // in play: the whole level again from its first stage, clock and deaths at zero
  restart(): void
  // after a win: the next level; back to select
  next(): void
  menu(): void
  // online only: a message from the link
  receive(text: string, now: number): void
  // tell the other side we're leaving (online), stop
  leave(): void
}
