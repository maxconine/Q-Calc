import { describe, expect, it } from 'vitest'
import worker from './index.js'
import { FEATURES, label, median, needsWork, percentile, perUser, rankLiked, spread } from './stats.js'

describe('median and percentiles', () => {
  it('is null for no values', () => {
    expect(median([])).toBeNull()
    expect(percentile([], 0.25)).toBeNull()
    expect(spread([])).toEqual({ users: 0, p25: null, median: null, p75: null })
  })

  it('takes the middle value, or the mean of the middle two', () => {
    expect(median([7])).toBe(7)
    expect(median([3, 1, 2])).toBe(2)
    expect(median([4, 1, 3, 2])).toBe(2.5)
  })

  it('is not pulled up by one heavy user the way a mean is', () => {
    const uses = [2, 3, 3, 4, 5000]
    expect(median(uses)).toBe(3)
    expect(uses.reduce((a, b) => a + b) / uses.length).toBeGreaterThan(1000)
  })

  it('interpolates between ranks like numpy and PERCENTILE.INC', () => {
    const v = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    expect(percentile(v, 0.25)).toBeCloseTo(3.25)
    expect(percentile(v, 0.75)).toBeCloseTo(7.75)
    expect(percentile(v, 0)).toBe(1)
    expect(percentile(v, 1)).toBe(10)
    expect(percentile([10, 20], 0.25)).toBe(12.5)
  })

  it('does not reorder the caller’s array', () => {
    const v = [3, 1, 2]
    percentile(v, 0.5)
    expect(v).toEqual([3, 1, 2])
  })

  it('reports the spread with how many users it covers', () => {
    expect(spread([5, 1, 9, 3])).toEqual({ users: 4, p25: 2.5, median: 4, p75: 6 })
  })
})

// a: heavy user; b: light, tried graphs once and dropped them; c: one day only
const ACTIVE = [
  { id: 'a', days: 10, last: '2026-10-20' },
  { id: 'b', days: 3, last: '2026-10-18' },
  { id: 'c', days: 1, last: '2026-10-10' },
]
const USAGE = [
  { id: 'a', event: 'open', n: 50, days: 10, last: '2026-10-20' },
  { id: 'a', event: 'calc.arith', n: 900, days: 10, last: '2026-10-20' },
  { id: 'a', event: 'calc.units', n: 40, days: 5, last: '2026-10-20' },
  { id: 'a', event: 'copy.answer', n: 30, days: 4, last: '2026-10-19' },
  { id: 'a', event: 'error.blank', n: 60, days: 6, last: '2026-10-20' },
  { id: 'b', event: 'open', n: 4, days: 3, last: '2026-10-18' },
  { id: 'b', event: 'calc.arith', n: 6, days: 3, last: '2026-10-18' },
  { id: 'b', event: 'calc.graph', n: 1, days: 1, last: '2026-10-11' },
  { id: 'b', event: 'copy.answer', n: 2, days: 1, last: '2026-10-18' },
  { id: 'b', event: 'error.blank', n: 3, days: 2, last: '2026-10-18' },
  { id: 'c', event: 'open', n: 1, days: 1, last: '2026-10-10' },
  { id: 'c', event: 'calc.graph', n: 2, days: 1, last: '2026-10-10' },
  { id: 'c', event: 'calc.arith', n: 1, days: 1, last: '2026-10-10' },
]

describe('per user', () => {
  const u = perUser(USAGE, ACTIVE)

  it('takes medians over every active install, counting a user who never did something as 0', () => {
    expect(u.activeUsers).toBe(3)
    // a 940, b 7, c 3
    expect(u.spreads.calculations).toEqual({ users: 3, p25: 5, median: 7, p75: 473.5 })
    expect(u.spreads.opens.median).toBe(4)
    expect(u.spreads.activeDays.median).toBe(3)
    // a: arith, units, copy · b: arith, graph, copy · c: graph, arith (opens and errors aren't features)
    expect(u.spreads.features).toEqual({ users: 3, p25: 2.5, median: 3, p75: 3 })
  })

  it('counts distinct users, repeat users and drop-offs per feature', () => {
    const graph = u.features.find((f) => f.event === 'calc.graph')
    // b tried graphs on the 11th and kept using q calc to the 18th; c's only day was the 10th, so c didn't drop it
    expect(graph).toMatchObject({ users: 2, repeat: 0, dropped: 1, dropRate: 0.5, medianUses: 1.5, total: 3 })
    const arith = u.features.find((f) => f.event === 'calc.arith')
    expect(arith).toMatchObject({ users: 3, repeat: 2, share: 1 })
    expect(u.features.some((f) => f.event === 'open' || f.event === 'error.blank')).toBe(false)
    expect(u.reach['error.blank']).toBe(2)
  })
})

describe('what people like', () => {
  const f = (event, users, repeatRate, total = 1) => ({ event, users, repeatRate, total })

  it('ranks by distinct users, then by how many came back, not by raw counts', () => {
    const ranked = rankLiked([f('copy.answer', 3, 0.2, 5), f('calc.graph', 1, 1, 10000), f('calc.units', 3, 0.9, 4), f('help', 0, 0)])
    expect(ranked.map((r) => r.event)).toEqual(['calc.units', 'copy.answer', 'calc.graph'])
  })

  it('breaks full ties by total uses, then by name, so the order is stable', () => {
    const ranked = rankLiked([f('periodic', 2, 0.5, 3), f('help', 2, 0.5, 3), f('copy.line', 2, 0.5, 9)])
    expect(ranked.map((r) => r.event)).toEqual(['copy.line', 'help', 'periodic'])
  })
})

describe('what needs work', () => {
  const u = perUser(USAGE, ACTIVE)

  it('flags a high rate of Enter with no answer, in plain words', () => {
    const items = needsWork({ ...u, retention: { cohort: 0, back: 0, week: 0 } })
    const blank = items.find((i) => i.key === 'error.blank')
    // 63 blanks out of 950 answers + 63 blanks
    expect(blank.rate).toBeCloseTo(63 / 1013)
    expect(blank.label).toBe('Enter gave no answer')
    expect(blank.detail).toBe('6% of lines entered showed no answer; 2 users hit this')
  })

  it('needs at least minUsers before something lands on the list', () => {
    const items = needsWork({ ...u, retention: null })
    expect(items.some((i) => i.key === 'dropped:calc.graph')).toBe(false)
    expect(needsWork({ ...u, retention: null, minUsers: 1 }).some((i) => i.key === 'dropped:calc.graph')).toBe(true)
  })

  it('ranks by users touched, then by rate', () => {
    const features = [
      { event: 'calc.graph', label: label('calc.graph'), users: 10, dropped: 6, dropRate: 0.6 },
      { event: 'periodic', label: label('periodic'), users: 4, dropped: 3, dropRate: 0.75 },
      { event: 'help', label: label('help'), users: 20, dropped: 2, dropRate: 0.1 },
    ]
    const items = needsWork({
      features,
      totals: { 'calc.arith': 90, 'calc.units': 10, 'error.unit': 5 },
      reach: { 'error.unit': 3 },
      activeUsers: 20,
      retention: { cohort: 10, back: 2, week: 1 },
    })
    expect(items.map((i) => i.key)).toEqual(['one-day', 'dropped:calc.graph', 'dropped:periodic', 'error.unit', 'unused'])
    expect(items[0].label).toBe('New installs that never came back')
    expect(items[1].label).toBe('Graphs: tried once, then dropped')
    // help's 10% drop-off is under the bar
    expect(items.some((i) => i.key === 'dropped:help')).toBe(false)
    expect(items.at(-1).detail).toContain('Arithmetic')
  })

  it('names every feature event', () => {
    for (const e of FEATURES) expect(label(e)).not.toMatch(/Unnamed/)
    expect(label('new.thing')).toBe('Unnamed event (new.thing)')
  })
})

describe('dashboard page', () => {
  // a d1 stand-in that answers each query by what it selects
  const db = {
    prepare(sql) {
      const all = async () => {
        if (sql.includes('GROUP BY id, event')) return { results: USAGE }
        if (sql.includes('COUNT(DISTINCT day)')) return { results: ACTIVE }
        if (sql.includes('AS d1')) return { results: [{ d1: 1, d7: 2, d30: 3 }] }
        if (sql.includes('AS cohort')) return { results: [{ cohort: 3, back: 2, week: 1 }] }
        if (sql.includes('min(day) AS day')) return { results: [{ day: null }] }
        return { results: [] }
      }
      return { bind: () => ({ all }) }
    },
  }
  const env = { DB: db, DASHBOARD_PASSWORD: 'pw' }
  const get = (path) => worker.fetch(new Request('https://x' + path, { headers: { authorization: 'Basic ' + btoa('u:pw') } }), env)

  it('serves counts only, with no install ids, and a script that parses', async () => {
    const html = await (await get('/?days=30')).text()
    expect(html).toContain('What people like')
    expect(html).toContain('What needs work')
    expect(html).not.toMatch(/"id":/)
    const script = html.slice(html.indexOf('<script>') + 8, html.indexOf('</script>'))
    expect(() => new Function(script)).not.toThrow()
    const data = JSON.parse(script.match(/const D = (.*)\n/)[1])
    expect(data.windows).toEqual({ d1: 1, d7: 2, d30: 3 })
    expect(data.spreads.calculations.median).toBe(7)
    expect(data.liked[0].event).toBe('calc.arith')
    expect(data.features).toHaveLength(FEATURES.length)
  })
})
