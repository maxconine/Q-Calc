import type { LevelDef } from '../emberTypes'

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

// 3. thin ice: ice only ember melts, thin water only frost freezes, and blocks that crumble underfoot.
// two screens wide and two tall: right along the bottom, up the shaft on the right, back left along the top.
// the best of version 1's 'thaw', 'white crossing' and 'cold storage'.
//  the bottom (feet row 33, heading right):
//  - frost starts shut in behind the ice at x4: ember leans on it until it melts (frost gem in the cell)
//  - the floor at x9-16 is thin water: frost walks out first and it freezes around him, ember right behind
//  - the lift at x19-20 goes up to two decks (y26) while someone stands on the plate at x23-24. the only crate
//    is at the far end of the LEFT deck (x9), across thin water (x11-15): so it's frost who rides up, ember on the plate
//  - frost crosses the thin water (frost gem at x7), hops the crate and pushes it right: over the water, which
//    is ice around him, over the lift that ember still holds up as a bridge, and along the right deck into the
//    hole at x27-28. it falls into the pit below and holds the plate there: the gate at x35 into the low
//    corridor sinks open for good
//  - frost drops down the hole after it and stands on the lift's plate; ember rides up, walks right, jumps the
//    hole and waits at x36
//  - frost climbs the steps at x29-32 into the corridor, right under the deck's thin water at x37-44, and walks
//    right; ember crosses on top, a step behind him (ahead of him the water isn't frozen yet)
//  - frost wades the water at x45-49 (which is why it's frost down here), ember drops off the deck's end.
//    checkpoint at x53
//  the shaft (x54-61), six ledges, two up each:
//  - x55-56 (y27) crumbles: jump on, jump on again. x58-59 (y25) and x55-56 (y23) are thin water: frost goes
//    first and stands on each, ember follows onto it (it holds 2.5 s after he leaves)
//  - x58-59 (y21) crumbles, and it's the catch: if frost goes first he can't wait on it, and ember's ice thaws
//    before it grows back. EMBER goes first, off the ice they share, over the crumbling ledge to the rock at
//    x55-56 (y19); frost waits for it to mend and follows. both jump left to the top floor. checkpoint at x52
//  the top (feet row 16, heading left):
//  - the bridge over the lava at x42-48 is ice, with a crate on it. frost crosses first (over the crate).
//    then ember walks on: it melts under her, and she and the crate drop into the lava. the crate holds the
//    plate down there: both gates in the wall at x38 open (ember can't hold it herself: her own gate needs it)
//  - ember climbs out, up the one-way rungs at x40-41 and through the upper gate onto the deck (y11);
//    frost walks through the lower gate (the rungs don't stop him) and jumps up through the one-way at x34-35
//    into the corridor (y14)
//  - the last crossing, x14-29: ember's deck is thin water and frost's floor right under it crumbles, over a
//    pool. so frost can't stop, and ember has to run with him, a step behind, hopping the block at x22.
//    if frost falls he wades out to the right and they try again (a frost gem is down there at x20)
//  - red door at x3 on the deck, blue door at x3 in the corridor
//  - misplays: ember on the ice bridge before frost is over (frost jumps into the lava to go back to the
//    flag); ember ahead of frost on thin water
const THIN_ICE: LevelDef = {
  id: 'thin-ice',
  name: 'thin ice',
  hint: 'ember melts ice · frost freezes thin water: ember, stay a step behind him',
  par: 105,
  stages: [
    {
      map: [
    // 0000000000111111111122222222223333333333444444444455555555556666
    // 0123456789012345678901234567890123456789012345678901234567890123
    '################################################################', // 0
    '################################################################', // 1
    '################################################################', // 2
    '################################################################', // 3
    '################################################################', // 4
    '################################################################', // 5
    '################################################################', // 6
    '#.....................................#........................#', // 7
    '#.....................r...............#........................#', // 8
    '#..............................................................#', // 9
    '#..R.....r............#........................................#', // 10
    '##############wwwwwwwwwwwwwwww##########==.....................#', // 11
    '#.....................................##.......................#', // 12
    '#..B.....b............................##==.....................#', // 13
    '##############~~~~~~~~~~~~~~~~####==..##.......................#', // 14
    '##############..........................==.....................#', // 15
    '##############......b...............................C..........#', // 16
    '##############WWWWWWWWWWWWWWWW############.......#######.......#', // 17
    '##########################################.......#####.........#', // 18
    '##########################################LLLLLLL#####...##....#', // 19
    '######################################################.......r.#', // 20
    '#..................................................#.........~.#', // 21
    '#..................................................#...........#', // 22
    '#..................................................#.......ww..#', // 23
    '#........................................................b.....#', // 24
    '#......b........................r........r..............ww.....#', // 25
    '#......####wwwww###..######..########wwwwwwww######............#', // 26
    '#....................................................~~........#', // 27
    '#........................................b..........C..........#', // 28
    '#................................############WWWWW##############', // 29
    '#####..........................#################################', // 30
    '#..............................#################################', // 31
    '#............................###################################', // 32
    '#b2...1.....r................###################################', // 33
    '#########wwwwwwww##########..###################################', // 34
    '################################################################', // 35
      ],
      things: [
        // the bottom
        { k: 'ice', x: 4, y: 31, w: 1, h: 3 },
        { k: 'mover', x: 19, y: 33, w: 2, h: 1, to: { x: 19, y: 26 }, when: 'lift' },
        { k: 'plate', x: 23, y: 33, ch: 'lift' },
        { k: 'box', x: 9, y: 25 },
        { k: 'plate', x: 27, y: 34, ch: 'crate' },
        { k: 'gate', x: 35, y: 27, w: 1, h: 2, open: 'crate', dir: 'down' },
        // the top
        { k: 'ice', x: 42, y: 17, w: 7, h: 1 },
        { k: 'box', x: 45, y: 16 },
        { k: 'plate', x: 42, y: 18, w: 7, ch: 'shelf' },
        { k: 'gate', x: 38, y: 15, w: 1, h: 2, open: 'shelf' },
        { k: 'gate', x: 38, y: 9, w: 1, h: 2, open: 'shelf' },
        { k: 'sign', x: 5, y: 28, text: 'only ember melts ice' },
        { k: 'sign', x: 12, y: 29, text: 'frost freezes thin water' },
        { k: 'sign', x: 57, y: 30, text: 'these crumble: keep moving' },
        { k: 'sign', x: 45, y: 13, text: 'ice melts under ember' },
        { k: 'sign', x: 22, y: 5, text: 'run together' },
      ],
    },
  ],
}

// 4. updraft: fans, lifts, trampolines and double-jump arrows. one screen wide and three tall: a tower climbed
// from the bottom. the best of version 1's 'updraft' and 'the lifts'.
//  the bottom (floor y52):
//  - the lava at x8-19 has two fans over it: the left one (x8-14) runs while the plate at x5-6 is held, the
//    right one (x13-19) while it ISN'T. ember stands on the plate; frost walks off the edge into the left fan
//    and floats to the middle (x13-14), where one fan or the other always blows. there he waits, ember steps
//    off, the right fan takes over, and he floats on across. (off the plate too soon and he drops in the lava)
//  - ember wades the lava. the lift at x29-30 rises while someone stands on the plate at x23-25, which lies in
//    lava: so it's ember who holds it and frost who rides. frost jumps the lava and onto the lift FIRST, then
//    ember steps in; up on the ledge (y44) frost waits by the lever at x27
//  - ember steps out, the lift comes back down, she gets on, and frost flips the lever: it carries her up and stays
//  - both jump from the ledge's end (x21) onto the platform that shuttles between x16-18 and x10-12 over the
//    fans, ride it left (a gem each on the way) and jump off onto the ledge at x1-8
//  - the fan in the corner (x1-2) blows up through the ceiling while its plate is held: one stands on the
//    plate at x5-6, the other rides up and stands on the plate at x4-5 on the next floor, and the first follows.
//    checkpoint at x8
//  the middle (floor y35):
//  - the trampoline at x10-11: every bounce is higher than the last, up to eight tiles. a gem each right above
//    it; then steer right onto the ledge at x13-16 (y29)
//  - the platform up at y22 rests by the ledge at x29-30 and slides out to x21-22 while a plate is held: the
//    one in the water at x14-16 on the floor (frost's), or the one at x29 on the far ledge
//  - frost stands in the water. ember bounces on the second trampoline (x17-18, on the ledge) until she can
//    steer right onto the platform, and then frost lets go: the platform carries her back to the far ledge
//  - ember stands on the plate at x29: the platform comes out again. frost takes the trampolines up onto it,
//    ember steps off the plate into the fan at x30 and floats up through the ceiling, drifting left onto the
//    floor at x28; the platform brings frost over and he follows. checkpoint at x27
//  - (gone up without holding the plate for the other? the hole in the ceiling is two wide: drop back down
//    its left half, out of the fan, onto the plate)
//  the top (floor y17):
//  - the goo at x17-23 is too wide to jump: jump, touch the arrow over it (x21, y13) and jump again in mid-air.
//    the arrow comes back 2.5 s after someone takes it
//  - the fan at x7-8 runs while a plate is held: the one in the water at x11-13 (frost's) or the one in the lava
//    on the high ledge at x2-4, y10 (ember's). so ember goes first: frost stands in the water, ember floats up,
//    touches the arrow at the top of the fan (x7, y10) and jumps again, left onto the ledge. she stands in the
//    lava; frost walks into the fan, floats up, takes the arrow once it's back and joins her
//  - from the ledge's end (x6) another arrow (x9, y6) carries each across to the top ledge (x13-17):
//    red door at x14, blue door at x16
//  - nothing here can be lost for good: a fall costs the climb, and whoever is still up holds the plate again
const UPDRAFT: LevelDef = {
  id: 'updraft-tower',
  name: 'updraft',
  hint: 'fans lift whoever floats over them',
  par: 135,
  stages: [
    {
      map: [
      // 00000000001111111111222222222233
      // 01234567890123456789012345678901
      '################################', // 0
      '#..............................#', // 1
      '#..............................#', // 2
      '#..............................#', // 3
      '#..............................#', // 4
      '#..............................#', // 5
      '#........^.....................#', // 6
      '#..............................#', // 7
      '#..............................#', // 8
      '#..r.........bR.Br.............#', // 9
      '##LLL##^.....#####.............#', // 10
      '#..............................#', // 11
      '#..............................#', // 12
      '#......b.............^.........#', // 13
      '#..............................#', // 14
      '#..............................#', // 15
      '#..........................C...#', // 16
      '###########WWW###GGGGGGG#####..#', // 17
      '#############################..#', // 18
      '#..............................#', // 19
      '#..............................#', // 20
      '#.......................br.....#', // 21
      '#............................###', // 22
      '#..............................#', // 23
      '#..............................#', // 24
      '#..............................#', // 25
      '#..............................#', // 26
      '#.........rb...................#', // 27
      '#..............................#', // 28
      '#............####TT#...........#', // 29
      '#..............................#', // 30
      '#..............................#', // 31
      '#..............................#', // 32
      '#..............................#', // 33
      '#.......C......................#', // 34
      '#..#######TT##WWW###############', // 35
      '#..#############################', // 36
      '#..............................#', // 37
      '#..............................#', // 38
      '#..............................#', // 39
      '#..............................#', // 40
      '#..............................#', // 41
      '#..............................#', // 42
      '#............b.r...............#', // 43
      '#########............########..#', // 44
      '#..............................#', // 45
      '#............b.................#', // 46
      '#..............................#', // 47
      '#..............................#', // 48
      '#..............................#', // 49
      '#..............................#', // 50
      '#.12......r....................#', // 51
      '########LLLLLLLLLLLL###LLL######', // 52
      '################################', // 53
      ],
      things: [
        // the bottom
        { k: 'plate', x: 5, y: 51, ch: 'a' },
        { k: 'fan', x: 8, y: 51, w: 7, h: 5, when: 'a' },
        { k: 'fan', x: 13, y: 51, w: 7, h: 5, when: '!a' },
        { k: 'plate', x: 23, y: 51, w: 3, ch: 'lift' },
        { k: 'mover', x: 29, y: 51, w: 2, h: 1, to: { x: 29, y: 44 }, when: 'lift' },
        { k: 'lever', x: 27, y: 43, ch: 'lift' },
        { k: 'mover', x: 16, y: 44, w: 3, h: 1, to: { x: 10, y: 44 }, speed: 1.5 },
        { k: 'fan', x: 1, y: 43, w: 2, h: 10, when: 'g' },
        { k: 'plate', x: 5, y: 43, ch: 'g' },
        { k: 'plate', x: 4, y: 34, ch: 'g' },
        // the middle
        { k: 'plate', x: 14, y: 34, w: 3, ch: 'plat' },
        { k: 'mover', x: 27, y: 22, w: 2, h: 1, to: { x: 21, y: 22 }, when: 'plat' },
        { k: 'plate', x: 29, y: 21, w: 1, ch: 'plat' },
        { k: 'fan', x: 30, y: 21, w: 1, h: 6 },
        // the top
        { k: 'plate', x: 11, y: 16, w: 3, ch: 'h' },
        { k: 'fan', x: 7, y: 16, w: 2, h: 5, when: 'h' },
        { k: 'plate', x: 2, y: 9, w: 3, ch: 'h' },
        { k: 'sign', x: 11, y: 45, text: 'one fan stops, the other starts' },
        { k: 'sign', x: 10, y: 31, text: 'every bounce is higher' },
        { k: 'sign', x: 24, y: 24, text: 'it comes back when the plate lets go' },
        { k: 'sign', x: 21, y: 11, text: 'touch the arrow, jump again in the air' },
      ],
    },
  ],
}

// 5. lock and light: beams, mirrors and sensors, with keys and locks. two stages, each two screens wide and
// three rooms long. the best of version 1's 'break the beam', 'two mirrors' and 'last light'.
// light is stopped by walls, bodies, crates, ice, shut gates and shut locks; it passes through pools.
// keys belong to the team: whoever touches a lock opens it.
//  stage 1, room A (x1-20): the gate at x21 is shut while the beam at y12 (lamp x20, sensor x17) is lit
//  - a second beam runs low over the lava at x13-15, and the gate into the water lane above (x12, y9-10) is
//    open only while it's broken: ember wades into the lava (the key is there, x14) and waits in it
//  - frost climbs the one-way rungs at x10-11 and jumps through that gate into the water on the ledge
//    (x12-15, y11), wades across and pushes the crate (x17) off the ledge's end: it lands on the shelf at
//    x18, in the upper beam, and the gate at x21 opens for good
//  room B (x22-42): the gate at x43 opens while the sensor at x37, y6 is lit. the beam runs right along y10
//  into a lock (x26), and beyond it into a mirror (x37) that throws it down into the ledge
//  - both climb the one-way rungs at x31-32. frost wades left through the water (x27-30) and walks into
//    the lock: the key ember carries opens it, and the beam goes through
//  - ember jumps right into the lava (x35-38) and turns the mirror: the beam goes up into the sensor. she
//    stands clear of it (right of the mirror, or down off the ledge), and frost climbs down out of it too
//  room C (x44-62) and the lane in the roof: the beam over the lava at x46-48 holds frost's gate (x53) shut
//  - ember stands in the lava, in the beam: frost jumps the lava and walks in, and climbs the one-way rungs
//    at x60-61 all the way up into the lane (y3), where a crate waits. the lane is too low to jump the crate:
//    it only goes left, back over all three rooms, and each of the lane's gates is open only while the beam
//    in the room below it is BROKEN. ember is the only one who can stand in any of them: they all run over lava
//  - the first gate (x52) is the beam ember already stands in: she stays until frost has pushed the crate through
//  - the second (x40) is room B's: ember walks back (the gate at x43 is still lit open), climbs the rungs and
//    jumps into the lava on the ledge, left of the mirror. frost pushes on
//  - the third (x24) is the low beam in room A: ember walks back through the gate at x21 (the crate on the
//    shelf holds it open) and into the lava at x13-15. frost pushes on
//  - the crate drops into the pit at x12-13 and holds the plate in it: the last gate (x9) opens. blue door
//    at x7 in the lane; ember steps out of the lava to the red door at x19
//  stage 2, room D (x1-20): the beam along y9 has to be turned down by the mirror at x11, and at the floor
//  turned right by the mirror under it, into the sensor at x12: the gate at x21 opens
//  - ember climbs the one-way rungs at x6-7 into the lava on the ledge (x8-11, y10) and turns the top mirror; the
//    beam drops through the lava. she walks off the ledge's right end, out of the beam
//  - frost walks under the rungs into the water (x9-11) and turns the bottom mirror, then
//    wades on, out of the beam
//  room E (x22-41): the lift at x40-41 is up only while the beam is BROKEN. the beam runs along the deck
//  (y7), is turned down the hole at x37 and ends in the sensor on the floor there
//  - frost stands on the sensor at x37, in the beam: the lift carries ember up to the deck
//  - frost walks to the lift, which comes down as soon as he steps out; ember stands in the beam up on the
//    deck (anywhere left of x37) and it carries him up. she wades the lava on the deck for the key (x27)
//  - both jump the lift's gap (x40-41) to the ledge at x42-46. checkpoint. (frost only has to come up for
//    his gem: the doorway under the ledge at x42 leads to room F too, and back, should the key be forgotten)
//  room F (x43-62): the last beam runs along the walkway (y11): through ice (x49), a lock (x53) and into
//  a mirror over water (x56) that has to throw it up into the sensor at y7. that opens the gate at x57
//  - both drop onto the walkway. ember leans on the ice until it melts; either walks into the lock with the
//    key; frost wades the water and turns the mirror
//  - everyone out of the light: back along the walkway and down off its left end. the gate opens, and the
//    little lift inside (x62), which is up only while this beam is BROKEN, comes down
//  - the blue door is up on the shelf inside (x59, y8), the red one on the floor (x58). frost walks in and
//    stands on the lift; ember climbs the one-way rungs at x46-47 back onto the walkway, into the beam: the
//    gate shuts between them and the lift carries frost up. ember drops back down, the gate opens again,
//    and she walks in
//  - nothing spendable can go wrong: there is one key for each lock and each lock is the only thing its key can open
const LOCK_AND_LIGHT: LevelDef = {
  id: 'lock-and-light',
  name: 'lock and light',
  hint: 'light stops at anything solid: you, a crate, ice, a lock',
  par: 160,
  stages: [
    {
      name: 'lamplight',
      hint: '↓ or S turns a mirror · keys are shared',
      map: [
      // 0000000000111111111122222222223333333333444444444455555555556666
      // 0123456789012345678901234567890123456789012345678901234567890123
      '################################################################', // 0
      '################################################################', // 1
      '#####.........................................................##', // 2
      '#####..B......................b.................b.............##', // 3
      '############..##############################################==##', // 4
      '#...........####.....#.....................#...................#', // 5
      '#...........####.....#.....................#................==.#', // 6
      '#...........####.....#.....................#...................#', // 7
      '#...........####.....#.....................#................==.#', // 8
      '#....................#.....................#...................#', // 9
      '#............b.......#.......b......r......#................==.#', // 10
      '#...........WWWW##...#.####WWWW==..LLLL....#...................#', // 11
      '#.........==.........#.........==..........#.........#######==##', // 12
      '#.................#............................................#', // 13
      '#.........==...................==...........................==.#', // 14
      '#.....12......kr...R...C....................C..r........b......#', // 15
      '#############LLL##############################LLL###############', // 16
      '################################################################', // 17
      ],
      things: [
        // room A
        { k: 'box', x: 17, y: 10 },
        { k: 'emitter', x: 20, y: 12, dir: 'left' },
        { k: 'sensor', x: 17, y: 12, ch: 'ea' },
        { k: 'gate', x: 21, y: 13, w: 1, h: 3, open: '!ea' },
        // room B
        { k: 'emitter', x: 23, y: 10, dir: 'right' },
        { k: 'lock', x: 26, y: 9, w: 1, h: 2 },
        { k: 'mirror', x: 37, y: 10, slant: '\\' },
        { k: 'sensor', x: 37, y: 6, ch: 'eb' },
        { k: 'gate', x: 43, y: 13, w: 1, h: 3, open: 'eb' },
        // room C
        { k: 'emitter', x: 45, y: 15, dir: 'right' },
        { k: 'sensor', x: 50, y: 15, ch: 'ec' },
        { k: 'gate', x: 53, y: 13, w: 1, h: 3, open: '!ec' },
        // the lane in the roof: its gates want the beams below broken
        { k: 'box', x: 58, y: 3 },
        { k: 'gate', x: 52, y: 2, w: 1, h: 2, open: '!ec' },
        { k: 'gate', x: 40, y: 2, w: 1, h: 2, open: '!eb' },
        { k: 'plate', x: 12, y: 4, ch: 'pit' },
        { k: 'gate', x: 9, y: 2, w: 1, h: 2, open: 'pit' },
        // room A again: the gate into the water lane wants the beam over the lava broken
        { k: 'emitter', x: 12, y: 15, dir: 'right' },
        { k: 'sensor', x: 16, y: 15, ch: 'e0' },
        { k: 'gate', x: 12, y: 9, w: 1, h: 2, open: '!e0' },
        { k: 'gate', x: 24, y: 2, w: 1, h: 2, open: '!e0' },
        { k: 'sign', x: 5, y: 8, text: 'something has to break that beam for good' },
        { k: 'sign', x: 30, y: 7, text: 'a lock stops light too' },
        { k: 'sign', x: 48, y: 9, text: 'some gates want the dark' },
      ],
    },
    {
      name: 'last light',
      hint: 'stand in a beam to break it; step out to mend it',
      map: [
      // 0000000000111111111122222222223333333333444444444455555555556666
      // 0123456789012345678901234567890123456789012345678901234567890123
      '################################################################', // 0
      '################################################################', // 1
      '################################################################', // 2
      '################################################################', // 3
      '################################################################', // 4
      '#....................#....................#..............#.....#', // 5
      '#....................#...................................#.....#', // 6
      '#....................#.....kr....b..........C............#.....#', // 7
      '#....................#..##LLLL#######.##..#####..........#.B...#', // 8
      '#........r...........#....................#..............#####.#', // 9
      '#.......LLLL.........#....................#..............#.....#', // 10
      '#....................#....................#........r...b.#.....#', // 11
      '#.....==.............#....................#..#==######WWW#.....#', // 12
      '#..............................................................#', // 13
      '#.....==......................................==...............#', // 14
      '#12.......b............C..................................R....#', // 15
      '#########WWW####################################################', // 16
      '################################################################', // 17
      ],
      things: [
        // room D
        { k: 'emitter', x: 1, y: 9, dir: 'right' },
        { k: 'mirror', x: 11, y: 9, slant: '/' },
        { k: 'mirror', x: 11, y: 15, slant: '/' },
        { k: 'sensor', x: 12, y: 15, ch: 'ed' },
        { k: 'gate', x: 21, y: 13, w: 1, h: 3, open: 'ed' },
        // room E
        { k: 'emitter', x: 24, y: 7, dir: 'right' },
        { k: 'mirror', x: 37, y: 7, slant: '\\' },
        { k: 'sensor', x: 37, y: 15, ch: 'ee' },
        { k: 'mover', x: 40, y: 15, w: 2, h: 1, to: { x: 40, y: 8 }, when: '!ee' },
        // room F
        { k: 'emitter', x: 45, y: 11, dir: 'right' },
        { k: 'ice', x: 49, y: 10, w: 1, h: 2 },
        { k: 'lock', x: 53, y: 10, w: 1, h: 2 },
        { k: 'mirror', x: 56, y: 11, slant: '\\' },
        { k: 'sensor', x: 56, y: 7, ch: 'ef' },
        { k: 'gate', x: 57, y: 13, w: 1, h: 3, open: 'ef' },
        { k: 'mover', x: 62, y: 15, w: 1, h: 1, to: { x: 62, y: 9 }, when: '!ef' },
        { k: 'sign', x: 31, y: 12, text: 'this lift rises in the dark' },
      ],
    },
  ],
}

export const BASICS: LevelDef[] = [FIRST_LIGHT, DEAD_WEIGHT, THIN_ICE, UPDRAFT, LOCK_AND_LIGHT]
