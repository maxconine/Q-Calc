import type { LevelDef } from '../emberTypes'

// pack 3 of 3, "summit": gravity, cannons and the finale. owner: levels-c.
// coordinates in the solution notes are tiles: x from the left wall (0), y from the top (0). "feet row" is the
// row an upright player's feet stand in; a flipped player hangs from the ceiling with their head one row down.
// what every gravity level here leans on (see the contract): a flipped player falls up, pools can't hurt them,
// doors don't take them, and they fall UP through any hole in a ceiling

// 11. head over heels: your own gravity switch. four halls stacked in an S, each seven tiles high (the top one
// six), joined by two-wide holes in the slabs that only a flipped player can rise through.
//  hall 1, the bottom (floor: feet row 33, ceiling: row 27), left to right. the lesson
//   - press the switch at x7: you fall up. walk the ceiling over the wall at x10-11 and the goo at x13-19 (a
//     gem each at x15 / x17 on the ceiling), and press the ceiling switch at x22 to come down before the hanging
//     wall at x24-25. walk under it (a gem each two tiles up at x30 / x33). checkpoint at x38
//   - the switch at x44, the ceiling over the wall at x46-47 and on to the hole at x60-61: up into hall 2
//  hall 2 (floor 24, ceiling 18), right to left: both arrive on the ceiling. checkpoint under x54-62
//   - EMBER comes down at the switch at x55 and wades the lava at x44-51 under the hanging wall at x47-48 (fire
//     gem at x46) to the plate at x41-42: the gate in the ceiling gap of that wall (x47-48, rows 18-19) opens
//   - FROST walks the ceiling through it (frost gem at x43) onto the ceiling plate at x38-39: the floor gate at
//     x36 opens and ember walks through. frost goes on to the ceiling switch at x33 and comes down
//   - both under the hanging wall at x30-31. checkpoint at x28. now the other way round:
//   - EMBER goes up at the switch at x26; FROST wades the water at x15-22 (frost gem at x21) to the plate at
//     x12-13, which opens the ceiling gate at x18-19 for ember (fire gem at x14 on the ceiling)
//   - ember onto the ceiling plate at x10-11: the floor gate at x8 opens for frost. ember walks on to the hole
//     at x2-3; frost goes up at the switch at x6 and follows
//  hall 3 (floor 15, ceiling 9), left to right: both arrive on the ceiling. checkpoint under x1-7
//   - look down before you turn: the floor under x8-21 is goo. the ceiling switches at x11 and x19 drop you in
//     it; the one at x15 drops you on the island at x14-16 (a gem each, and a switch to go back up); the one at
//     x24 is the way on, down to the floor before the hanging wall at x27-28. checkpoint at x22-30
//   - both climb the steps at x30-31 and cross the block at x32-36. FROST wades back under it, into the water
//     tunnel at x32-36 (two high: nobody can jump to its middle) and presses the switch at x33: frost comes
//     out along the tunnel's roof and up to the ceiling at x37, onto the ceiling plate at
//     x43-44, and the lift at x40-42 carries EMBER over the goo at x40-49 (frost gem at x46 on the ceiling)
//   - ember wades the lava at x52-57 (fire gem at x55) to the switch at x59 and rises through the hole at
//     x60-61; frost has walked the ceiling there already
//  hall 4, the top (floor 6, ceiling 1), right to left: both arrive on the ceiling. checkpoint under x54-62
//   - each to their own: over the wall at x53, ember comes down at the ceiling switch at x47, into the lava at
//     x46-48 (fire gem), frost at x50 into the water at x50-51 (frost gem), and frost hops the lava. under the
//     hanging wall at x41-42. checkpoint at x35-40
//   - the dash. one goes up at the switch at x37. the floor button at x32 opens the CEILING gate at x29 and
//     the ceiling button at x35 opens the FLOOR gate under it, each for a moment after it's left: the one
//     below stands on theirs, the one above steps on theirs and runs through; the one below goes as they pass
//   - the one above comes down at the ceiling switch at x22 (the hanging wall at x18-19 stops the ceiling);
//     red door at x5, blue at x8
//  no softlocks: every switch that takes you up has one on the ceiling near it to come back down; whoever gets
//  through a hole early can fall back through it and finds a switch beside it; either half of the dash can be
//  undone by the other standing on their button again
const HEAD_OVER_HEELS: LevelDef = {
  id: 'head-over-heels',
  name: 'head over heels',
  hint: 'S or ↓ at a switch turns your own gravity over',
  par: 140,
  stages: [
    {
      map: [
        '################################################################',
        '#.................##.....r...............##....................#',
        '#.................##.....................##....................#',
        '#.................##.........#...........##..........#.........#',
        '#.................##.........#...........##..........#.........#',
        '#....................................................#.........#',
        '#....R..B...............b......................r..b..#.........#',
        '##############################################LLL#WW########..##',
        '############################################################..##',
        '#..........................##.................b................#',
        '#..........................##..................................#',
        '#..........................##...#####..........................#',
        '#..........................##..######..........................#',
        '#..........................##..######..........................#',
        '#.............................##...............................#',
        '#.............b.r.............##.......................r.......#',
        '##..####GGGGGG###GGGGG##########WWWWW###GGGGGGGGGG##LLLLLL######',
        '##..############################################################',
        '#.............r...............##...........b...................#',
        '#.............................##...............................#',
        '#.......#.........##..........##....#..........##..............#',
        '#.......#.........##..........##....#..........##..............#',
        '#.......#.........##..........##....#..........##..............#',
        '#..............................................................#',
        '#....................b........................r................#',
        '###############WWWWWWWW#####################LLLLLLLL########..##',
        '############################################################..##',
        '#..............r.b......##.....................................#',
        '#.......................##.....................................#',
        '#.........##............##....................##...............#',
        '#.........##............##....................##...............#',
        '#.........##............##....b..r............##...............#',
        '#.........##..................................##...............#',
        '#.1.2.....##..................................##...............#',
        '#############GGGGGGG############################################',
        '################################################################',
      ],
      things: [
        // hall 1
        { k: 'sign', x: 6, y: 30, text: 'S or ↓ here: you fall up' },
        { k: 'gswitch', x: 7, y: 33 },
        { k: 'gswitch', x: 22, y: 27 },
        { k: 'gswitch', x: 22, y: 33 },
        { k: 'checkpoint', x: 38, y: 33, zone: { x: 35, y: 27, w: 8, h: 7 } },
        { k: 'gswitch', x: 44, y: 33 },
        { k: 'gswitch', x: 57, y: 33 },
        // hall 2
        { k: 'checkpoint', x: 57, y: 24, zone: { x: 54, y: 18, w: 9, h: 7 } },
        { k: 'gswitch', x: 55, y: 18 },
        { k: 'gswitch', x: 55, y: 24 },
        { k: 'plate', x: 41, y: 24, ch: 'p1' },
        { k: 'gate', x: 47, y: 18, w: 2, h: 2, open: 'p1' },
        { k: 'plate', x: 38, y: 18, ch: 'q1', ceil: true },
        { k: 'gate', x: 36, y: 23, w: 1, h: 2, open: 'q1' },
        { k: 'gswitch', x: 33, y: 18 },
        { k: 'gswitch', x: 33, y: 24 },
        { k: 'checkpoint', x: 28, y: 24, zone: { x: 24, y: 18, w: 6, h: 7 } },
        { k: 'gswitch', x: 26, y: 18 },
        { k: 'gswitch', x: 26, y: 24 },
        { k: 'plate', x: 12, y: 24, ch: 'p2' },
        { k: 'gate', x: 18, y: 18, w: 2, h: 2, open: 'p2' },
        { k: 'plate', x: 10, y: 18, ch: 'q2', ceil: true },
        { k: 'gate', x: 8, y: 23, w: 1, h: 2, open: 'q2' },
        { k: 'gswitch', x: 6, y: 18 },
        { k: 'gswitch', x: 6, y: 24 },
        // hall 3
        { k: 'checkpoint', x: 6, y: 15, zone: { x: 1, y: 9, w: 7, h: 7 } },
        { k: 'gswitch', x: 5, y: 9 },
        { k: 'gswitch', x: 5, y: 15 },
        { k: 'sign', x: 11, y: 12, text: 'look down before you turn' },
        { k: 'gswitch', x: 11, y: 9 },
        { k: 'gswitch', x: 15, y: 9 },
        { k: 'gswitch', x: 15, y: 15 },
        { k: 'gswitch', x: 19, y: 9 },
        { k: 'gswitch', x: 24, y: 9 },
        { k: 'gswitch', x: 24, y: 15 },
        { k: 'checkpoint', x: 29, y: 15, zone: { x: 22, y: 13, w: 9, h: 3 } },
        { k: 'gswitch', x: 33, y: 15 },
        { k: 'plate', x: 43, y: 9, ch: 'm3', ceil: true },
        { k: 'mover', x: 40, y: 15, w: 3, h: 1, to: { x: 47, y: 15 }, when: 'm3' },
        { k: 'gswitch', x: 59, y: 15 },
        // hall 4
        { k: 'checkpoint', x: 58, y: 6, zone: { x: 54, y: 1, w: 9, h: 6 } },
        { k: 'gswitch', x: 57, y: 1 },
        { k: 'gswitch', x: 57, y: 6 },
        { k: 'sign', x: 48, y: 4, text: 'each to their own' },
        { k: 'gswitch', x: 47, y: 1 },
        { k: 'gswitch', x: 50, y: 1 },
        { k: 'checkpoint', x: 39, y: 6, zone: { x: 34, y: 1, w: 7, h: 6 } },
        { k: 'gswitch', x: 37, y: 1 },
        { k: 'gswitch', x: 37, y: 6 },
        { k: 'button', x: 35, y: 1, ch: 'cb', secs: 1.6, ceil: true },
        { k: 'button', x: 32, y: 6, ch: 'fb', secs: 1.6 },
        { k: 'gate', x: 29, y: 1, w: 1, h: 2, open: 'fb', dir: 'down' },
        { k: 'gate', x: 29, y: 5, w: 1, h: 2, open: 'cb' },
        { k: 'gswitch', x: 22, y: 1 },
        { k: 'gswitch', x: 22, y: 6 },
      ],
    },
  ],
}

export const SUMMIT: LevelDef[] = [HEAD_OVER_HEELS]
