import type { LevelDef } from '../emberTypes'

// pack 2 of 3, "depths": the dark levels. owner: levels-b.
// coordinates in the solution notes are tiles: x from the left wall (0), y from the top (0).
// "feet row" is the row a player's feet stand in; they stand on the row below it.
// 'O' blocks are there while the lights are on, 'D' blocks while they are off

// 6. lights out: lamps, and bridges that are only there in the dark or only in the light. three floors:
// along the bottom to the right, up at the right end, and back to the left in two lanes, frost's under ember's.
//  the bottom floor (feet row 14), lit at the start. the doors at x1 and x3 are walled off: the way in is from above
//  - the lamp at x7 puts the lights out: the goo pit at x9-15 has a bridge of dark in its mouth. both cross
//  - the lamp at x18 lights the bridge of light at x20-24, but the gate at x28 opens only in the dark. frost
//    crosses and waits at x25-27, ember puts the lights out, frost goes through to the lamp at x30 and lights
//    the bridge for ember, then puts the lights out again to let her through the gate
//  - in the dark: a lava trench at x33-37 whose bridge is light. ember wades it and lights up at x39; frost
//    crosses. a water trench at x41-45 whose bridge is dark: frost wades it and puts the lights out at x47;
//    ember crosses. checkpoint at x51
//  the lift (x56-57) rises to the top floor in the dark and comes down in the light; its lamp is at x54.
//  the stair room beyond it (x58-62) has two steps of light, x61-62 and x58-59
//  - ember lights up, frost steps on the lift, ember puts the lights out: frost rides up to his gem (x57)
//    and she lights up again to bring him down
//  - in the light ember crosses the lift and climbs the steps to her gem (x59), comes back and stands on the lift
//  - frost at the lamp: lights out, and ember rides to the top floor (feet row 4) and steps off to the left;
//    lights on, and he climbs the steps and jumps the shaft to the middle floor (feet row 9)
//  the lanes, to the left, starting lit. each has a moat only its owner wades, then five bridges between
//  islands. frost's are light, dark, light, light, dark; ember's are dark, light, light, dark, dark.
//  whoever throws a switch waits for the other to say they stand on rock
//  - frost crosses to his first island and puts the lights out at its lamp (x39). ember crosses to hers,
//    frost to his second (x30-32)
//  - ember lights up at her island's lamp (x39): frost crosses two bridges to x14-16, ember two to x22-24
//  - frost puts the lights out at x15, and both walk their last bridges to the left end
//  the trapdoors at x1-2: ember's is light, frost's (under it) is dark
//  - in the dark ember drops through hers and lands on his. frost lights up at x5: his trapdoor is gone and she
//    falls into the door room; he walks in after her. red door x1, blue door x3
//  a wrong switch can strand someone on an island; the goo beside every island ends that, back to the checkpoint
const LIGHTS_OUT: LevelDef = {
  id: 'lights-out',
  name: 'lights out',
  hint: 'S or ↓ at a lamp turns the lights off, and on',
  par: 100,
  stages: [
    {
      map: [
        //         1         2         3         4         5         6
        //1234567890123456789012345678901234567890123456789012345678901234
        '################################################################', // 0
        '################################################################', // 1
        '#.........................................................######', // 2
        '#.........................................................######', // 3
        '#..........r...............r.............................b######', // 4
        '#OO######DDDDD###DDDDD###OOOOO###OOOOO###DDDDD###LLLLLL#..######', // 5
        '#..######GGGGG###GGGGG###GGGGG###GGGGG###GGGGG##########..######', // 6
        '#..............................................................#', // 7
        '#..............................................................#', // 8
        '#..................b.......................b...............r...#', // 9
        '#DD######DDDDD###OOOOO###OOOOO###DDDDD###OOOOO###WWWWWW#.......#', // 10
        '#..######GGGGG###GGGGG###GGGGG###GGGGG###GGGGG##########..OO...#', // 11
        '#...#..........................................................#', // 12
        '#...#.......b................................................OO#', // 13
        '#R.B#12............................................C...........#', // 14
        '#########DDDDDDD####OOOOO########OOOOO###DDDDD##########..######', // 15
        '#########GGGGGGG####GGGGG########LLLLL###WWWWW##################', // 16
        '################################################################', // 17
      ],
      things: [
        { k: 'lamp', x: 7, y: 14 },
        { k: 'lamp', x: 18, y: 14 },
        { k: 'gate', x: 28, y: 12, w: 1, h: 3, open: '!lights' },
        { k: 'lamp', x: 30, y: 14 },
        { k: 'lamp', x: 39, y: 14 },
        { k: 'lamp', x: 47, y: 14 },
        { k: 'lamp', x: 54, y: 14 },
        { k: 'mover', x: 56, y: 15, w: 2, h: 1, to: { x: 56, y: 5 }, when: '!lights', speed: 2.5 },
        { k: 'lamp', x: 39, y: 9 },
        { k: 'lamp', x: 39, y: 4 },
        { k: 'lamp', x: 15, y: 9 },
        { k: 'lamp', x: 5, y: 9 },
        { k: 'sign', x: 7, y: 12, text: 'the lamp: S or ↓' },
        { k: 'sign', x: 22, y: 12, text: 'a bridge of light, a gate for the dark' },
        { k: 'sign', x: 53, y: 12, text: 'the lift rises in the dark' },
        { k: 'sign', x: 47, y: 7, text: 'say when you stand on rock' },
      ],
    },
  ],
}

export const DEPTHS: LevelDef[] = [LIGHTS_OUT]
