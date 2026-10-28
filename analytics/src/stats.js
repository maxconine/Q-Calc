// the dashboard's maths: per-user medians and spreads, the features people like, and what needs work.
// pure functions over rows the worker already pulled from d1, so they can be tested without a database

// plain names for every event the apps send (USAGE_EVENTS in src/lib/analytics.ts and native/core/src/app/analytics.rs)
export const LABELS = {
  launch: 'App launched',
  open: 'Overlay opened',
  update: 'Updated to a new version',
  'calc.arith': 'Arithmetic',
  'calc.units': 'Unit conversions',
  'calc.solve': 'Solve',
  'calc.system': 'Systems of equations',
  'calc.graph': 'Graphs',
  'calc.function': 'Defining functions',
  'calc.assign': 'Variables',
  'calc.calculus': 'Calculus',
  'calc.matrix': 'Matrices',
  'calc.words': 'Natural language',
  'calc.chain': 'Chaining from the last answer',
  'copy.answer': 'Copy answer',
  'copy.line': 'Copy line',
  'history.insert': 'Insert from history',
  'history.undo': 'Undo in history',
  'form.tab': 'Tab through answer forms',
  'prefix.step': 'Step SI prefix',
  periodic: 'Periodic table',
  help: 'Help sheet',
  'settings.open': 'Settings shortcut',
  'error.blank': 'Enter with no answer',
  'error.unit': 'Impossible unit conversion',
}

// events that say the app ran, updated or failed rather than that someone used a feature
export const META = new Set(['launch', 'open', 'update', 'error.blank', 'error.unit'])
export const FEATURES = Object.keys(LABELS).filter((e) => !META.has(e))

export const label = (event) => LABELS[event] ?? `Unnamed event (${event})`
export const isCalc = (event) => event.startsWith('calc.')

// the p-th quantile (0..1) of numbers, interpolating between the two nearest ranks (as numpy and excel's
// PERCENTILE.INC do), so the median of an even count is the mean of the middle two. null when empty
export function percentile(values, p) {
  if (!values.length) return null
  const s = [...values].sort((a, b) => a - b)
  return quantileSorted(s, p)
}

function quantileSorted(s, p) {
  const i = (s.length - 1) * Math.min(1, Math.max(0, p))
  const lo = Math.floor(i)
  const hi = Math.ceil(i)
  return s[lo] + (s[hi] - s[lo]) * (i - lo)
}

export const median = (values) => percentile(values, 0.5)

// median, the middle half (25th to 75th percentile) and how many users the figure covers
export function spread(values) {
  if (!values.length) return { users: 0, p25: null, median: null, p75: null }
  const s = [...values].sort((a, b) => a - b)
  return { users: s.length, p25: quantileSorted(s, 0.25), median: quantileSorted(s, 0.5), p75: quantileSorted(s, 0.75) }
}

// usage: one row per install and event over the range, { id, event, n (total uses), days (days used), last (last day used) }
// active: one row per install over the range, { id, days (days active), last (last day active) }
// per-user figures cover every active install, so someone who never graphed counts as 0 calculations, not as missing
export function perUser(usage, active) {
  const lastActive = new Map(active.map((r) => [r.id, r.last]))
  const calcs = new Map(active.map((r) => [r.id, 0]))
  const opens = new Map(active.map((r) => [r.id, 0]))
  const featureCount = new Map(active.map((r) => [r.id, 0]))
  const byEvent = new Map()
  const totals = {}
  const reach = {}

  for (const r of usage) {
    totals[r.event] = (totals[r.event] || 0) + r.n
    reach[r.event] = (reach[r.event] || 0) + 1
    if (isCalc(r.event) && calcs.has(r.id)) calcs.set(r.id, calcs.get(r.id) + r.n)
    if (r.event === 'open' && opens.has(r.id)) opens.set(r.id, r.n)
    if (META.has(r.event)) continue
    if (featureCount.has(r.id)) featureCount.set(r.id, featureCount.get(r.id) + 1)
    let f = byEvent.get(r.event)
    if (!f) byEvent.set(r.event, (f = { event: r.event, uses: [], repeat: 0, dropped: 0, total: 0 }))
    f.uses.push(r.n)
    f.total += r.n
    if (r.days >= 2) f.repeat++
    // used it on one day only, then kept opening q calc on later days without it
    else if (r.days === 1 && lastActive.has(r.id) && lastActive.get(r.id) > r.last) f.dropped++
  }

  const activeUsers = active.length
  const features = [...byEvent.values()].map((f) => ({
    event: f.event,
    label: label(f.event),
    users: f.uses.length,
    share: activeUsers ? f.uses.length / activeUsers : 0,
    repeat: f.repeat,
    repeatRate: f.uses.length ? f.repeat / f.uses.length : 0,
    dropped: f.dropped,
    dropRate: f.uses.length ? f.dropped / f.uses.length : 0,
    medianUses: median(f.uses),
    total: f.total,
  }))

  return {
    activeUsers,
    spreads: {
      calculations: spread([...calcs.values()]),
      opens: spread([...opens.values()]),
      activeDays: spread(active.map((r) => r.days)),
      features: spread([...featureCount.values()]),
    },
    features,
    totals,
    reach,
  }
}

// what people like: most distinct users first, then the share who came back to it on another day
export function rankLiked(features) {
  return features
    .filter((f) => f.users > 0)
    .sort((a, b) => b.users - a.users || b.repeatRate - a.repeatRate || b.total - a.total || (a.event < b.event ? -1 : 1))
}

const pct = (x) => `${Math.round(x * 100)}%`
const people = (n) => `${n} ${n === 1 ? 'user' : 'users'}`

// what needs work, as plain-language items ranked by how many users each touches, then how bad the rate is.
// minUsers keeps one person's quirk off the list
export function needsWork({ features, totals, reach = {}, activeUsers, retention, minUsers = 2 }) {
  const items = []

  for (const f of features) {
    if (f.dropped >= minUsers && f.dropRate >= 0.3) {
      items.push({
        key: `dropped:${f.event}`,
        label: `${f.label}: tried once, then dropped`,
        detail: `${f.dropped} of ${people(f.users)} used it on one day only, then kept using Q Calc without it`,
        users: f.dropped,
        rate: f.dropRate,
      })
    }
  }

  const t = (e) => totals[e] || 0
  const calcs = Object.keys(totals).filter(isCalc).reduce((a, e) => a + totals[e], 0)
  const blankUsers = reach['error.blank'] || 0
  const unitUsers = reach['error.unit'] || 0

  const attempts = calcs + t('error.blank') + t('error.unit')
  if (attempts && blankUsers >= minUsers) {
    const rate = t('error.blank') / attempts
    if (rate >= 0.02) {
      items.push({
        key: 'error.blank',
        label: 'Enter gave no answer',
        detail: `${pct(rate)} of lines entered showed no answer; ${people(blankUsers)} hit this`,
        users: blankUsers,
        rate,
      })
    }
  }

  const conversions = t('calc.units') + t('error.unit')
  if (conversions && unitUsers >= minUsers) {
    const rate = t('error.unit') / conversions
    if (rate >= 0.02) {
      items.push({
        key: 'error.unit',
        label: "Unit conversions that can't be done",
        detail: `${pct(rate)} of unit conversions asked for an impossible pair of units; ${people(unitUsers)} hit this`,
        users: unitUsers,
        rate,
      })
    }
  }

  if (retention && retention.cohort >= minUsers) {
    const gone = retention.cohort - retention.back
    const rate = gone / retention.cohort
    if (gone >= minUsers && rate >= 0.3) {
      items.push({
        key: 'one-day',
        label: 'New installs that never came back',
        detail: `${gone} of ${retention.cohort} installs first seen 8 to 37 days ago were used on their first day only`,
        users: gone,
        rate,
      })
    }
  }

  const used = new Set(features.filter((f) => f.users > 0).map((f) => f.event))
  const unused = FEATURES.filter((e) => !used.has(e))
  if (activeUsers >= 5 && unused.length) {
    items.push({
      key: 'unused',
      label: 'Features nobody used in this range',
      detail: unused.map(label).join(', '),
      users: 0,
      rate: 1,
    })
  }

  return items.sort((a, b) => b.users - a.users || b.rate - a.rate || (a.key < b.key ? -1 : 1))
}
