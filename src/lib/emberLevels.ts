import type { LevelDef } from './emberTypes'

// the shipped levels, easiest first. ids are saved in progress: never change or reuse one.
// coordinates in the solution notes are tiles: x from the left wall (0), y from the top (0).
// "feet row" is the row a player's feet stand in; they stand on the row below it
export const LEVELS: LevelDef[] = [
  // 1. first light: movement, pools, gems, doors.
  //  - both start bottom left. jump the step at x5-6
  //  - ember drops down and wades the lava at x8-12 (fire gem in the lava); frost jumps onto the shelf above it (frost gem)
  //  - both jump the goo at x15-16 (a gem each hangs above it)
  //  - over the second step: frost wades the water at x21-25 (frost gem), ember takes the shelf above (fire gem)
  //  - ember stands in the red door, frost in the blue one
  {
    id: 'first-light',
    name: 'first light',
    hint: 'WASD for ember · arrows for frost',
    par: 25,
    map: [
      '################################',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#.........b............r.......#',
      '#.......#####..rb....#####.....#',
      '#..............................#',
      '#....##...........##...........#',
      '#.12.##...r.......##...b....RB.#',
      '########LLLLL##GG####WWWWW######',
      '################################',
    ],
    things: [
      { k: 'sign', x: 10, y: 6, text: 'ember walks on lava' },
      { k: 'sign', x: 15, y: 9, text: 'goo burns both: jump' },
      { k: 'sign', x: 23, y: 6, text: 'frost wades water' },
    ],
  },

  // 2. two lanes: climbing, and lanes only one of you survives.
  //  - jump the goo by the start (a gem each above it), climb the stairs at x8-13
  //  - ember walks into the lower lane at y9 and wades the lava (fire gem); its 2-tile ceiling stops frost jumping it
  //  - frost hops from the second stair onto the ledge at x9-10, then up onto the lip at x12 and the upper lane at y6,
  //    wading the water (frost gem); ember can't hop it either
  //  - a stretch jump from the ledge reaches the two high gems at x10-11 y4
  //  - both lanes drop into the end room: red door at x27, blue at x29
  {
    id: 'two-lanes',
    name: 'two lanes',
    hint: 'each of you has a lane: stay in yours',
    par: 30,
    map: [
      '################################',
      '#..............##########......#',
      '#..............##########......#',
      '#..............##########......#',
      '#.........rb...##########......#',
      '#..............................#',
      '#..................b...........#',
      '#...........#####WWWWW###......#',
      '#..............................#',
      '#........##........r...........#',
      '#...........#####LLLLL###......#',
      '#...........#############......#',
      '#...rb....###############..R.B.#',
      '#.........######################',
      '#.......########################',
      '#12.....########################',
      '####GG##########################',
      '################################',
    ],
    things: [],
  },

  // 3. hold the door: pressure plates and gates.
  //  - the lower corridor (water, frost's door) is shut by a gate at x14 that the plate at x6-7 opens;
  //    the upper corridor (lava, ember's door) by a gate at x14 that the plate at x24-25 opens
  //  - ember stands on the left plate; frost walks the lower corridor through the gate and wades the water
  //  - frost stands on the plate at x24-25; ember climbs the ledge at x1-2 and the shelf to the upper corridor,
  //    through its gate and across the lava to the red door
  //  - frost steps off and walks right to the blue door
  //  - swapping roles only drowns ember in the water, which restarts the level
  {
    id: 'hold-the-door',
    name: 'hold the door',
    hint: 'one holds the plate, the other walks through',
    par: 35,
    map: [
      '################################',
      '#..........#####################',
      '#..........#####################',
      '#..........#####################',
      '#..........#####################',
      '#..........#####################',
      '#..........#####################',
      '#..........#####################',
      '#......r...#####################',
      '#..........#####################',
      '#.b............................#',
      '#...................r.....r.R..#',
      '#....#############LLLLL#########',
      '#..........#####################',
      '###............................#',
      '#..12...............b.....b.B..#',
      '##################WWWWW#########',
      '################################',
    ],
    things: [
      { k: 'plate', x: 6, y: 15, ch: 'low' },
      { k: 'gate', x: 14, y: 14, w: 1, h: 2, open: 'low' },
      { k: 'plate', x: 24, y: 15, ch: 'high' },
      { k: 'gate', x: 14, y: 10, w: 1, h: 2, open: 'high' },
      { k: 'sign', x: 5, y: 4, text: 'plates hold gates open' },
    ],
  },

  // 4. lockstep: levers. each walks a lane split by gates, and every gate's lever sits in the other's lane.
  //  - ember flips the lever at x6 (S): frost's gate at x8 opens
  //  - frost wades the water, flips the lever at x14 (down arrow): ember's gate at x8 opens
  //  - ember wades the lava, flips x14: frost's x16 opens. frost hops the bump, flips x22: ember's x16 opens
  //  - ember hops the bump, flips x22: frost's x24 opens. frost flips x26 in the last room: ember's x24 opens
  //  - both walk to their doors at x29
  {
    id: 'lockstep',
    name: 'lockstep',
    hint: 'S or ↓ flips a lever',
    par: 40,
    map: [
      '################################',
      '################################',
      '################################',
      '#..............................#',
      '#..............................#',
      '#...................r..........#',
      '#.1.........r......##......r.R.#',
      '###########LLL##################',
      '################################',
      '################################',
      '################################',
      '################################',
      '#..............................#',
      '#..............................#',
      '#...................b..........#',
      '#.2.........b......##......b.B.#',
      '###########WWW##################',
      '################################',
    ],
    things: [
      { k: 'lever', x: 6, y: 6, ch: 'f1' },
      { k: 'lever', x: 14, y: 6, ch: 'f2' },
      { k: 'lever', x: 22, y: 6, ch: 'f3' },
      { k: 'lever', x: 14, y: 15, ch: 'e1' },
      { k: 'lever', x: 22, y: 15, ch: 'e2' },
      { k: 'lever', x: 26, y: 15, ch: 'e3' },
      { k: 'gate', x: 8, y: 3, w: 1, h: 4, open: 'e1', dir: 'down' },
      { k: 'gate', x: 16, y: 3, w: 1, h: 4, open: 'e2', dir: 'down' },
      { k: 'gate', x: 24, y: 3, w: 1, h: 4, open: 'e3', dir: 'down' },
      { k: 'gate', x: 8, y: 12, w: 1, h: 4, open: 'f1' },
      { k: 'gate', x: 16, y: 12, w: 1, h: 4, open: 'f2' },
      { k: 'gate', x: 24, y: 12, w: 1, h: 4, open: 'f3' },
      { k: 'sign', x: 4, y: 4, text: 'S flips levers' },
      { k: 'sign', x: 4, y: 13, text: '↓ flips levers' },
    ],
  },

  // 5. the lifts: movers driven by a plate and a lever.
  //  - frost hops onto the left lift (x1-2); ember stands on the plate at x24-25 and the lift carries frost up
  //  - frost steps onto the deck, rides the shuttle over the goo (a gem each above it) and reaches the lever at x22
  //  - ember hops onto the right lift (x29-30); frost flips the lever and the lift carries ember up
  //  - ember jumps the 2-tile gap to the deck. ember to the red door at x20, frost to the blue door at x25
  //  - the roles also work the other way round; a lift drops back down whenever its plate or lever lets go
  {
    id: 'the-lifts',
    name: 'the lifts',
    hint: 'a plate holds a lift up; a lever leaves it up',
    par: 45,
    map: [
      '################################',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#.b..........................r.#',
      '#..............................#',
      '#..............rb...R....B.....#',
      '#..##########.....#########....#',
      '#............GGGGG.............#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#....r..12.................b...#',
      '################################',
      '################################',
    ],
    things: [
      { k: 'plate', x: 24, y: 15, ch: 'p' },
      { k: 'lever', x: 22, y: 6, ch: 'q' },
      { k: 'mover', x: 1, y: 15, w: 2, h: 1, to: { x: 1, y: 7 }, when: 'p' },
      { k: 'mover', x: 29, y: 15, w: 2, h: 1, to: { x: 29, y: 7 }, when: 'q' },
      { k: 'mover', x: 13, y: 7, w: 2, h: 1, to: { x: 16, y: 7 }, speed: 1.5 },
      { k: 'sign', x: 15, y: 11, text: 'stand on a lift, then press' },
    ],
  },

  // 6. crates: pushable boxes onto plates.
  //  - ember drops into the pit at x3-4 (a plate): the gate at x11 opens and frost walks through
  //  - frost hops the pit at x13-14, climbs the stairs at x17-26 and turns left along the high walkway
  //  - frost pushes the first crate (x16) left into the hole at x13-14: it drops into the pit plate there and the
  //    gate at x27 in front of the doors opens for good
  //  - frost hops the hole, pushes the second crate (x6) off the walkway's end: it lands in ember's pit
  //    beside ember (ember keeps to the pit's left half). ember hops out; the crate holds the gate open
  //  - both climb the stairs: ember to the red door at x28, frost to the blue door at x30
  {
    id: 'crates',
    name: 'crates',
    hint: 'walk into a crate to push it',
    par: 50,
    map: [
      '################################',
      '#..........................#####',
      '#..........................#####',
      '#.........b..................r.#',
      '#....########..########........#',
      '#..........##...............R.B#',
      '#..........##............#######',
      '#..........##............#######',
      '#..........##..........#########',
      '#..........##.........r#########',
      '#..........##........###########',
      '#..........##.......b###########',
      '#..........##......#############',
      '#..................#############',
      '#...............r###############',
      '#12.............b###############',
      '###..########..#################',
      '################################',
    ],
    things: [
      { k: 'plate', x: 3, y: 16, ch: 'pit' },
      { k: 'gate', x: 11, y: 13, w: 1, h: 3, open: 'pit' },
      { k: 'plate', x: 13, y: 16, ch: 'drop' },
      { k: 'gate', x: 27, y: 3, w: 1, h: 3, open: 'drop', dir: 'down' },
      { k: 'box', x: 16, y: 3 },
      { k: 'box', x: 6, y: 3 },
      { k: 'sign', x: 6, y: 9, text: 'crates press plates too' },
    ],
  },

  // 7. thaw: ice that only ember can melt.
  //  - the lower corridor (water, frost's way) is sealed by ice at x10. ember walks in and leans on it until it melts
  //  - frost wades the water and stands on the plate at x20-21: the gate at x10 in the upper corridor opens
  //  - ember climbs the ledge and shelf on the left, walks the upper corridor through the gate and over the lava,
  //    and drops down the hole at x23-24 into the lower corridor
  //  - ember melts the ice at x25 that seals both doors (a fire gem is frozen inside), then steps into the red door at x26
  //  - frost leaves the plate and walks through to the blue door at x29
  {
    id: 'thaw',
    name: 'thaw',
    hint: 'ember: lean on ice to melt it',
    par: 40,
    map: [
      '################################',
      '#......#########################',
      '#......#########################',
      '#......#########################',
      '#......#########################',
      '#......#########################',
      '#......#########################',
      '#......#########################',
      '#.....r#########################',
      '#......#########################',
      '#.b.....................########',
      '#..............r........########',
      '#....########LLLLL#####..#######',
      '#......################..#######',
      '###............................#',
      '#..12..........b...b.....rR..B.#',
      '#############WWWWW##############',
      '################################',
    ],
    things: [
      { k: 'ice', x: 10, y: 14, w: 1, h: 2 },
      { k: 'ice', x: 25, y: 14, w: 1, h: 2 },
      { k: 'plate', x: 20, y: 15, ch: 'p' },
      { k: 'gate', x: 10, y: 10, w: 1, h: 2, open: 'p' },
      { k: 'sign', x: 3, y: 5, text: 'only ember melts ice' },
    ],
  },

  // 8. white crossing: thin water that frost freezes for ember.
  //  - frost walks out over the thin water at x5-12 first; it freezes around frost, and ember follows close behind
  //    (it stays frozen a couple of seconds after frost moves on)
  //  - frost hops the block at x14-15 and takes the low corridor from x17; ember climbs the block and jumps up
  //    to the high corridor, whose floor at x18-25 is thin water lying right on top of frost's corridor
  //  - frost walks slowly right underneath, freezing the floor above, and ember walks across on top of frost
  //  - ember stands on the plate at x27-28: the gate at x27 in frost's corridor sinks open
  //  - frost walks to the blue door at x29; ember steps off into the red door at x29 above
  {
    id: 'white-crossing',
    name: 'white crossing',
    hint: 'frost freezes thin water nearby; ember, stay close',
    par: 35,
    map: [
      '################################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....###########################',
      '#....########..b...............#',
      '#....########.........r......R.#',
      '#.r..########....#wwwwwwww######',
      '#....########..................#',
      '#.............##......b......B.#',
      '#.12....r.b...##.###############',
      '#####wwwwwwww###################',
      '################################',
    ],
    things: [
      { k: 'plate', x: 27, y: 11, ch: 'p' },
      { k: 'gate', x: 27, y: 13, w: 1, h: 2, open: 'p', dir: 'down' },
      { k: 'sign', x: 2, y: 5, text: 'frost freezes thin water' },
    ],
  },

  // 9. updraft: fans, two of them worked by levers.
  //  - the little fan by the left wall lifts either of you to a shelf with a gem each
  //  - ember wades the lava pit at x10-15 and flips the lever at x18: the big fan over the lava starts
  //  - frost walks into the fan from the left, rides it up and drifts right onto the high ledge at x16-23
  //  - frost wades the water under the low ceiling (ember can't hop it) and flips the lever at x22:
  //    the fan at x26-27 starts
  //  - ember walks into that fan and rides it up onto the top-right shelf, to the red door at x29
  //  - frost steps into the blue door at x23
  {
    id: 'updraft',
    name: 'updraft',
    hint: 'fans lift whoever stands over them',
    par: 40,
    map: [
      '################################',
      '#..............................#',
      '#..............................#',
      '#............................Rr#',
      '#...............########....####',
      '#..............................#',
      '#..............................#',
      '#...................b..B.......#',
      '#.rb............##WWWW##.......#',
      '#.##.........b.................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..12.....r....................#',
      '##########LLLLLL################',
      '################################',
    ],
    things: [
      { k: 'fan', x: 1, y: 15, w: 1, h: 7 },
      { k: 'fan', x: 11, y: 15, w: 4, h: 8, when: 'f' },
      { k: 'fan', x: 26, y: 15, w: 2, h: 12, when: 'g' },
      { k: 'lever', x: 18, y: 15, ch: 'f' },
      { k: 'lever', x: 22, y: 7, ch: 'g' },
      { k: 'sign', x: 6, y: 11, text: 'ride the wind' },
    ],
  },

  // 10. relay: timed buttons (races) and portals.
  //  - frost stands on the plate at x6-7 (bottom strip): the gate at x14 in ember's strip opens
  //  - ember taps the button at x4 and races: over the goo at x9-10, through x14, and through the timed gate
  //    at x20 before it shuts (3s)
  //  - ember stands on the plate at x23-24: the gate at x14 in frost's strip opens
  //  - frost taps the button at x9 and races: through x14, over the bump, through the timed gate at x20 (2.5s)
  //  - both walk into the portals at x29: frost comes out top left, wades the water to the blue door at x14;
  //    ember comes out top right, wades the lava to the red door at x17
  {
    id: 'relay',
    name: 'relay',
    hint: 'a button keeps its gate open a few seconds: run!',
    par: 40,
    map: [
      '################################',
      '#..............................#',
      '#..............................#',
      '#......b......B..R......r......#',
      '#####WWWWW############LLLLL#####',
      '################################',
      '################################',
      '#..............................#',
      '#........r.....................#',
      '#..............................#',
      '#.1.......................r....#',
      '#########GG#####################',
      '################################',
      '#..............................#',
      '#...............b..............#',
      '#.2.............#........b.....#',
      '################################',
      '################################',
    ],
    things: [
      { k: 'button', x: 4, y: 10, ch: 'te', secs: 3 },
      { k: 'plate', x: 6, y: 15, ch: 'pe' },
      { k: 'gate', x: 14, y: 8, w: 1, h: 3, open: 'pe' },
      { k: 'gate', x: 20, y: 8, w: 1, h: 3, open: 'te' },
      { k: 'plate', x: 23, y: 10, ch: 'pf' },
      { k: 'button', x: 9, y: 15, ch: 'tf', secs: 2.5 },
      { k: 'gate', x: 14, y: 14, w: 1, h: 2, open: 'pf' },
      { k: 'gate', x: 20, y: 14, w: 1, h: 2, open: 'tf' },
      { k: 'portal', x: 29, y: 15, pair: 'a' },
      { k: 'portal', x: 2, y: 3, pair: 'a' },
      { k: 'portal', x: 29, y: 10, pair: 'b' },
      { k: 'portal', x: 30, y: 3, pair: 'b' },
    ],
  },

  // 11. break the beam: light, sensors, and a mirror. bodies block light.
  //  - a lamp at x1 shines along the floor into the sensor at x6. while it's lit, the gate at x13 to the lower
  //    (water) corridor stays shut; it opens only while the beam is broken
  //  - ember stands in the beam (x3-5); frost walks through the lower gate and wades the water
  //  - frost presses down on the mirror at x24: it turns the lamp at x30's beam up into the sensor at x24,
  //    and the gate at x13 to the upper (lava) corridor opens
  //  - frost steps back left into the blue door at x22 (standing right of the mirror would block the beam)
  //  - ember climbs the ledge and shelf, walks the upper corridor over the lava to the red door at x28
  {
    id: 'break-the-beam',
    name: 'break the beam',
    hint: 'stand in a beam to break it; ↓ or S turns a mirror',
    par: 40,
    map: [
      '################################',
      '#.........######################',
      '#.........######################',
      '#.........######################',
      '#.........######################',
      '#.........######################',
      '#.........######################',
      '#.........######################',
      '#.....r...######################',
      '#.........######################',
      '#.b............................#',
      '#.................r.......r.R..#',
      '#....###########LLLLL###########',
      '#.........######################',
      '###............................#',
      '#......12.........b..bB........#',
      '################WWWWW###########',
      '################################',
    ],
    things: [
      { k: 'emitter', x: 1, y: 15, dir: 'right' },
      { k: 'sensor', x: 6, y: 15, ch: 'eye' },
      { k: 'gate', x: 13, y: 14, w: 1, h: 2, open: '!eye' },
      { k: 'emitter', x: 30, y: 15, dir: 'left' },
      { k: 'mirror', x: 24, y: 15, slant: '/' },
      { k: 'sensor', x: 24, y: 14, ch: 'lamp' },
      { k: 'gate', x: 13, y: 10, w: 1, h: 2, open: 'lamp' },
    ],
  },

  // 12. two mirrors: one beam steered by both players.
  //  - the doors (x1, x3) sit in a pen behind the gate at x4, which opens while the sensor at x5 is lit.
  //    the lamp at x1 shines right along row 7 into the high mirror at x12
  //  - ember climbs the block at x26-27, the lava step at x22-24 (frost can't), the ledge at x18-20 and the
  //    walkway at x13-17, and turns the high mirror: the beam now drops down column 12
  //  - frost wades the water at x15-20, climbs the block at x13-14 and turns the low mirror at x12:
  //    the falling beam turns left along row 13, over the left pool, into the sensor. the pen gate opens
  //  - ember jumps left from the walkway over column 12 to the ledge at x6-10 and drops down by the pen;
  //    frost wades the left pool. both walk into the pen
  //  - while anyone stands in the beam the gate shuts again; it waits rather than crush
  {
    id: 'two-mirrors',
    name: 'two mirrors',
    hint: 'light bounces off mirrors; turn them with ↓ or S',
    par: 40,
    map: [
      '################################',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............r...............#',
      '#.....#####..#####.............#',
      '#..................r...........#',
      '#.................###..........#',
      '#......................r.......#',
      '#.....................LLL......#',
      '#####........b.................#',
      '#............##...........##...#',
      '#R.B....b....##..b........##12.#',
      '######WWWWWWW##WWWWWW###########',
      '################################',
    ],
    things: [
      { k: 'emitter', x: 1, y: 7, dir: 'right' },
      { k: 'mirror', x: 12, y: 7, slant: '/' },
      { k: 'mirror', x: 12, y: 13, slant: '\\' },
      { k: 'sensor', x: 5, y: 13, ch: 'lit' },
      { k: 'gate', x: 4, y: 14, w: 1, h: 2, open: 'lit' },
    ],
  },

  // 13. cold storage: ice, a lift, a crate and thin water.
  //  - frost starts shut in behind the ice at x4: ember walks left and melts it
  //  - ember steps onto the lift at x8-9; frost stands on the plate at x12-13 and the lift carries ember up to the deck
  //  - ember pushes the crate (x11) right into the hole at x15-16: it drops into the pit plate below,
  //    and the gate at x21 into the low corridor sinks open for good
  //  - frost climbs the steps at x17-20 into the low corridor, right underneath the deck's thin water at x22-27,
  //    and walks slowly right; ember crosses on top, keeping just behind frost
  //  - ember to the red door at x29 on the deck, frost to the blue door at x29 in the corridor
  //  - (if frost rides the lift instead, frost can push the crate, drop through the hole and hold the plate for ember)
  {
    id: 'cold-storage',
    name: 'cold storage',
    par: 50,
    map: [
      '################################',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#..............................#',
      '#........r.....................#',
      '#..............................#',
      '#..................r.....r...R.#',
      '#.........#####..#####wwwwww####',
      '#..............................#',
      '#........................b...B.#',
      '#....................###########',
      '#####..............#############',
      '#................b.#############',
      '#................###############',
      '#.2b..1..........###############',
      '###############..###############',
      '################################',
    ],
    things: [
      { k: 'ice', x: 4, y: 13, w: 1, h: 3 },
      { k: 'plate', x: 12, y: 15, ch: 'lift' },
      { k: 'mover', x: 8, y: 15, w: 2, h: 1, to: { x: 8, y: 8 }, when: 'lift' },
      { k: 'box', x: 11, y: 7 },
      { k: 'plate', x: 15, y: 16, ch: 'crate' },
      { k: 'gate', x: 21, y: 9, w: 1, h: 2, open: 'crate', dir: 'down' },
    ],
  },

  // 14. lantern works: fans, light through a window, a timed button, portals and a crate.
  //  - frost hops the goo and flips the lever at x26: the fan at x11-12 in ember's shaft starts
  //  - ember hops the goo, rides the fan up and, still hovering, turns the mirror at x12 (↓/S): the lamp's beam
  //    now runs right through the slot in the middle wall into the sensor at x18, which starts frost's fan
  //  - ember drifts left onto the floor out of the beam; frost rides the fan up to the middle floor
  //  - frost stands on the timed button at x24: ember's gate at x5 opens, ember takes the portal at x1 to the top
  //  - ember flips the lever at x4 up top: frost's gate at x28 opens, frost takes the portal at x30 to the top
  //  - ember pushes the crate at x6 right into the pit plate at x8-9: both gates of the middle room sink open
  //  - ember to the red door at x15, frost to the blue door at x16
  {
    id: 'lantern-works',
    name: 'lantern works',
    par: 50,
    map: [
      '################################',
      '#..............................#',
      '#.........r...............b....#',
      '#..............RB..............#',
      '########..######################',
      '################################',
      '#.............####.............#',
      '#........r....####....b........#',
      '#.............####.............#',
      '#..............................#',
      '###########..######..###########',
      '#.............####.............#',
      '#.............####.............#',
      '#......r......####.....b.......#',
      '#.............####.............#',
      '#..1..........####...........2.#',
      '#######GG##############GG#######',
      '################################',
    ],
    things: [
      { k: 'lever', x: 26, y: 15, ch: 'f1' },
      { k: 'fan', x: 11, y: 15, w: 2, h: 6, when: 'f1' },
      { k: 'emitter', x: 12, y: 6, dir: 'down' },
      { k: 'mirror', x: 12, y: 9, slant: '/' },
      { k: 'sensor', x: 18, y: 9, ch: 'eye' },
      { k: 'fan', x: 19, y: 15, w: 2, h: 6, when: 'eye' },
      { k: 'button', x: 24, y: 9, ch: 'tb', secs: 3 },
      { k: 'gate', x: 5, y: 7, w: 1, h: 3, open: 'tb' },
      { k: 'portal', x: 1, y: 9, pair: 'e' },
      { k: 'portal', x: 1, y: 3, pair: 'e' },
      { k: 'lever', x: 4, y: 3, ch: 'g2' },
      { k: 'gate', x: 28, y: 7, w: 1, h: 3, open: 'g2' },
      { k: 'portal', x: 30, y: 9, pair: 'f' },
      { k: 'portal', x: 30, y: 3, pair: 'f' },
      { k: 'box', x: 6, y: 3 },
      { k: 'plate', x: 8, y: 4, ch: 'top' },
      { k: 'gate', x: 13, y: 1, w: 1, h: 3, open: 'top', dir: 'down' },
      { k: 'gate', x: 18, y: 1, w: 1, h: 3, open: 'top', dir: 'down' },
    ],
  },

  // 15. last light: everything at once. ember starts up top, frost down below.
  //  - ember walks left and turns the mirror at x6: the lamp's beam drops through the lava seam into the
  //    sensor at x6 below, and frost's gate at x8 opens (ember steps right, out of the beam)
  //  - frost wades the water and stands on the plate at x16-17: ember's gate at x12 sinks open; ember walks through
  //  - ember walks on and leans against the ice plug at x20-21 in its floor until it melts, opening a shaft
  //  - frost walks into the fan in the shaft and is blown up to ember's floor; drift right onto x22 at the top
  //    of a bob (a fall into the shaft just blows you back up)
  //  - ember jumps the shaft
  //  - frost walks out over the thin water, ember close behind; frost to the blue door at x29, ember to the red at x30
  {
    id: 'last-light',
    name: 'last light',
    par: 45,
    map: [
      '################################',
      '################################',
      '#..............................#',
      '#...r..........................#',
      '#...............r..............#',
      '#........1...............rb..BR#',
      '######L#############..#wwwwww###',
      '######L#############..##########',
      '######L#############..##########',
      '######L#############..##########',
      '######L#############..##########',
      '######L#############..##########',
      '#..............................#',
      '#..............................#',
      '#.................b............#',
      '#.2.........b..................#',
      '##########WWWWW#################',
      '################################',
    ],
    things: [
      { k: 'emitter', x: 1, y: 5, dir: 'right' },
      { k: 'mirror', x: 6, y: 5, slant: '/' },
      { k: 'sensor', x: 6, y: 13, ch: 'beam' },
      { k: 'gate', x: 8, y: 12, w: 1, h: 4, open: 'beam' },
      { k: 'plate', x: 16, y: 15, ch: 'p2' },
      { k: 'gate', x: 12, y: 2, w: 1, h: 4, open: 'p2', dir: 'down' },
      { k: 'ice', x: 20, y: 6, w: 2, h: 1 },
      { k: 'fan', x: 20, y: 15, w: 2, h: 10 },
    ],
  },
]
