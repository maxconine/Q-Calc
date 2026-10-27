// The rest of the games capture: the shipped levels as data, trails bots playing each other move by move, and
// a few pure helpers' answers. Run by capture.sh beside the level packs' own test files.
import { writeFileSync } from 'node:fs'
import { it } from 'vitest'
import { fmtPar, fmtTime, levelDifficulty, levelLength } from '../../../src/lib/ember'
import { baseView, easeCamera } from '../../../src/lib/emberCamera'
import { LEVELS } from '../../../src/lib/emberLevels'
import { parseProgress, recordResult } from '../../../src/lib/emberProgress'
import { botTurn, newRound, seeded, steer, step, COLS, ROWS, type Dir, type Level, type Player } from '../../../src/lib/trails'
import { decodeTrails, encodeTrails, hashStart, hashStep } from '../../../src/lib/trailsNet'
import { OUT } from './setup'

it('captures the levels', () => {
  writeFileSync(`${OUT}ember-levels.json`, JSON.stringify(LEVELS))
})

it('captures trails duels', () => {
  const duels: unknown[] = []
  for (const [a, b, rounds, seed] of [
    ['hard', 'easy', 3, 3],
    ['normal', 'easy', 4, 4],
    ['hard', 'normal', 3, 5],
    ['easy', 'easy', 4, 6],
  ] as Array<[Level, Level, number, number]>) {
    const rand = seeded(seed)
    const games: unknown[] = []
    for (let i = 0; i < rounds; i++) {
      const levels: [Level, Level] = i % 2 ? [b, a] : [a, b]
      let r = newRound()
      let h = hashStart(i, [0, 0])
      const moves: number[] = []
      const opening = Math.floor(rand() * 12)
      while (!r.result && r.tick < COLS * ROWS) {
        for (const p of [0, 1] as Player[]) {
          if (r.tick >= opening) {
            const d = botTurn(r, p, levels[p], rand)
            moves.push(d)
            r = steer(r, p, d)
          } else if (rand() < 0.15) r = steer(r, p, Math.floor(rand() * 4) as Dir)
        }
        r = step(r)
        h = hashStep(h, r)
      }
      games.push({ moves: moves.join(''), ticks: r.tick, winner: r.result?.winner ?? null, hash: h, snap: encodeTrails({ t: 'trs', e: 1, r: i, d: 2, score: [0, 1], phase: 'point', wait: 12.6, round: r, frames: [[], []], hash: h }) })
    }
    duels.push({ a, b, seed, games })
  }
  const draws = (() => {
    const r = seeded(99)
    return Array.from({ length: 8 }, () => r())
  })()
  writeFileSync(`${OUT}trails-duels.json`, JSON.stringify({ duels, draws }))
  // the snapshots decode again
  for (const d of duels as Array<{ games: Array<{ snap: string }> }>) for (const g of d.games) if (!decodeTrails(g.snap)) throw new Error(g.snap)
})

it('captures the small helpers', () => {
  const times = [0, 0.04, 7.44, 59.96, 60, 130, 3599.99, -3, Number.NaN].map((s) => [s, fmtTime(s), fmtPar(s)])
  const difficulty = Array.from({ length: 12 }, (_, i) => levelDifficulty(i, 12))
  const lengths = LEVELS.map(levelLength)
  const bases = [16 / 9, 1, 0.5, 3, 0, Number.NaN].map((a) => baseView(a))
  const eased = easeCamera({ x: 0, y: 0, w: 640, h: 360, zoom: 1 }, { x: 100, y: 40, w: 960, h: 540, zoom: 0.66 }, 3)
  const progress = [
    '{"levels":{"first-light":{"completed":true,"stars":5,"secs":12.5,"gems":4},"old":{"completed":true,"stars":1}},"muted":true}',
    '{"v":2,"levels":{"a":{"completed":true,"stars":2,"secs":-1,"gems":3.7,"deaths":2.2},"b":{"completed":false,"stars":3}},"muted":1}',
    'nonsense',
    '{"v":9,"levels":[]}',
  ].map((t) => JSON.stringify(parseProgress(t)))
  const recorded = JSON.stringify(
    recordResult(parseProgress(progress[1]!), 'a', { secs: 30.5, gems: [2, 2], total: [3, 3], stars: 2 }, 1),
  )
  writeFileSync(`${OUT}ember-helpers.json`, JSON.stringify({ times, difficulty, lengths, bases, eased, progress, recorded }))
})
