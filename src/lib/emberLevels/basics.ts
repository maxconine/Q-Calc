import type { LevelDef } from '../emberTypes'
import { LEGACY } from './legacy'

// pack 1 of 3, "basics": the tutorial, then the classics of version 1 reworked into bigger, harder levels.
// owner: levels-a. coordinates in the solution notes are tiles: x from the left wall (0), y from the top (0).
// "feet row" is the row a player's feet stand in; they stand on the row below it

// 1. first light: movement, pools, gems, doors. version 1's tutorial, kept exactly as it was.
//  - both start bottom left. jump the step at x5-6
//  - ember drops down and wades the lava at x8-12 (fire gem in the lava); frost jumps onto the shelf above it (frost gem)
//  - both jump the goo at x15-16 (a gem each hangs above it)
//  - over the second step: frost wades the water at x21-25 (frost gem), ember takes the shelf above (fire gem)
//  - ember stands in the red door, frost in the blue one
const FIRST_LIGHT: LevelDef = {
  id: 'first-light',
  name: 'first light',
  hint: 'WASD for ember · arrows for frost',
  par: 25,
  stages: [
    {
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
  ],
}

// 2. dead weight: plates, gates, crates and levers, with one-way decks you can jump up through but never drop
// through. two screens wide, in three acts: along the floor, up the decks on the right, and back left along
// two lanes in the roof. the best of version 1's 'hold the door', 'crates' and 'lockstep'.
//  act 1, the floor (x1-36):
//  - one of you stands on the plate at x4-5: the gate at x7 opens. the other walks through and pushes the crate
//    (x10) right onto the plate at x12-13, which holds the same gate, so the first can follow
//  - the deck at x17-24 (y12) is four tiles up: too high to jump. push the crate under it; frost jumps from the
//    crate up through the deck (frost gem above it at x19)
//  - ember stands on the plate under the deck (x21-22): the upper gate at x25 opens, frost walks the upper lane
//    and wades the water at x28-32 (ember can't: the ceiling is too low to jump it)
//  - frost stands on the plate at x33-34: the lower gate at x25 opens, ember walks the lower lane through the
//    lava at x28-32. frost drops down the hole at x35. checkpoint at x38 (both stand by the flag)
//  act 2, the decks (x37-62):
//  - both hop the goo at x40-41 and the pit at x43-44 (a plate in it, and a frost gem: standing there opens the
//    gate at x52 on the top deck, which shows what the pit is for), climb the block at x49-50 and jump up
//    through the deck (y13)
//  - the top deck (x46-51, y9) is four up again, and the only crate (x47) is on this deck. frost climbs the crate
//    and jumps up through to the top; ember can do the same first for the fire gem at x47, y8, and walk back
//    off the deck's left end
//  - ember pushes the crate left off the deck's end (x45): it falls into the pit and holds the plate for good.
//    the top gate (x52, y7-8) sinks open: frost walks in, wades the water at x54-58 and stands on the plate at x59
//  - ember's gate (x52, y11-12) needs the pit's plate AND frost's: it opens now. ember wades the lava at x54-58
//    and climbs the one-way rungs in the shaft at x62 to the lower roof lane (y5); frost steps off the plate and
//    climbs the rungs at x60, through ember's floor, to the upper roof lane (y2). checkpoint (both at x60-61)
//  - misplays: the crate pushed off before frost is up, or the wrong one sent up top, can't be undone:
//    the goo at x40-41 (or R) takes you back to the flag
//  act 3, the roof lanes, heading left: frost above (water at x53-57), ember below (lava at x53-57). two
//  airlocks, each a pair of gates worked by one lever in the OTHER lane: on opens the near gate and shuts the far one
//  - ember flips the lever at x41: frost's gate at x38 opens (and x33 shuts). frost walks into the airlock
//    and flips the lever at x35: ember's gate at x38 opens (and x33 shuts)
//  - ember flips x41 back (frost's far gate opens again) and walks into her airlock; frost flips x35 back
//    (ember's far gate opens) and both walk out. flip in the wrong order and someone is shut in until it's undone
//  - the second airlock swaps the roles: frost flips the lever at x26, ember walks in and flips the one at x20,
//    frost flips x26 back and walks in, ember flips x20 back
//  - blue door at x9 above, red door at x9 below
const DEAD_WEIGHT: LevelDef = {
  id: 'dead-weight',
  name: 'dead weight',
  hint: 'a crate holds a plate down for good: spend it wisely',
  par: 80,
  stages: [
    {
      map: [
    // 0000000000111111111122222222223333333333444444444455555555556666
    // 0123456789012345678901234567890123456789012345678901234567890123
    '################################################################', // 0
    '########.....................................................###', // 1
    '########.B...b..............b................................###', // 2
    '#####################################################WWWWW##=###', // 3
    '########....................................................=..#', // 4
    '########.R...r..............r..................................#', // 5
    '#####################################################LLLLL##=#.#', // 6
    '#####################################.#.....................=#=#', // 7
    '#......#...........b.....############..........r........b....#.#', // 8
    '#......#.................############.........======##WWWWW###=#', // 9
    '#......#............................#...............##########.#', // 10
    '#......#......................b.....#.........................=#', // 11
    '#......#.........========###WWWWW##.#...................r......#', // 12
    '#........................##########.#........=======##LLLLL#####', // 13
    '#......................r............................############', // 14
    '#12...........................r.......C..........##.############', // 15
    '############################LLLLL#######GG#b.###################', // 16
    '################################################################', // 17
      ],
      things: [
        // act 1
        { k: 'plate', x: 4, y: 15, ch: 'a' },
        { k: 'gate', x: 7, y: 13, w: 1, h: 3, open: 'a' },
        { k: 'box', x: 10, y: 15 },
        { k: 'plate', x: 12, y: 15, ch: 'a' },
        { k: 'plate', x: 21, y: 15, ch: 'up' },
        { k: 'gate', x: 25, y: 10, w: 1, h: 2, open: 'up' },
        { k: 'plate', x: 33, y: 11, ch: 'low' },
        { k: 'gate', x: 25, y: 14, w: 1, h: 2, open: 'low' },
        // act 2
        { k: 'box', x: 47, y: 12 },
        { k: 'plate', x: 43, y: 16, ch: 'pit' },
        { k: 'gate', x: 52, y: 7, w: 1, h: 2, open: 'pit', dir: 'down' },
        { k: 'plate', x: 59, y: 8, w: 1, ch: 'top' },
        { k: 'gate', x: 52, y: 11, w: 1, h: 2, open: ['pit', 'top'] },
        { k: 'checkpoint', x: 61, y: 5, zone: { x: 58, y: 1, w: 4, h: 5 } },
        // act 3: frost's gates (the upper lane) slide up, ember's (the lower) down
        { k: 'lever', x: 41, y: 5, ch: 'x' },
        { k: 'gate', x: 38, y: 1, w: 1, h: 2, open: 'x' },
        { k: 'gate', x: 33, y: 1, w: 1, h: 2, open: '!x' },
        { k: 'lever', x: 35, y: 2, ch: 'y' },
        { k: 'gate', x: 38, y: 4, w: 1, h: 2, open: 'y', dir: 'down' },
        { k: 'gate', x: 33, y: 4, w: 1, h: 2, open: '!y', dir: 'down' },
        { k: 'lever', x: 26, y: 2, ch: 'z' },
        { k: 'gate', x: 23, y: 4, w: 1, h: 2, open: 'z', dir: 'down' },
        { k: 'gate', x: 18, y: 4, w: 1, h: 2, open: '!z', dir: 'down' },
        { k: 'lever', x: 20, y: 5, ch: 'w' },
        { k: 'gate', x: 23, y: 1, w: 1, h: 2, open: 'w' },
        { k: 'gate', x: 18, y: 1, w: 1, h: 2, open: '!w' },
        { k: 'sign', x: 3, y: 10, text: 'plates hold gates open' },
        { k: 'sign', x: 13, y: 10, text: 'crates press plates too' },
        { k: 'sign', x: 21, y: 9, text: 'jump up through a deck' },
        { k: 'sign', x: 46, y: 4, text: 'a lever opens one gate and shuts the next' },
      ],
    },
  ],
}

// TODO(ember-2:levels-a): version 1's levels that haven't been reworked yet stay playable after the new ones
const REPLACED = ['first-light', 'two-lanes', 'hold-the-door', 'lockstep', 'crates']
export const BASICS: LevelDef[] = [FIRST_LIGHT, DEAD_WEIGHT, ...LEGACY.filter((l) => !REPLACED.includes(l.id))]
