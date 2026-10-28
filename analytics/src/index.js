// q calc's usage counts: POST /e from the apps, a daily github download snapshot, and a dashboard at /
// secrets: DASHBOARD_PASSWORD (wrangler secret put DASHBOARD_PASSWORD), optional GITHUB_TOKEN for rate limits

import { FEATURES, label, needsWork, perUser, rankLiked } from './stats.js'

const ID_RE = /^[0-9a-f]{32}$/
const DAY_RE = /^\d{4}-\d{2}-\d{2}$/
const EVENT_RE = /^[a-z]+(?:\.[a-z]+)?$/
const VERSION_RE = /^\d+\.\d+\.\d+(?:[-+][\w.]+)?$/
const MAX_BODY = 16 * 1024
const CORS = { 'access-control-allow-origin': '*', 'access-control-allow-methods': 'POST, OPTIONS', 'access-control-max-age': '86400' }

export default {
  async fetch(request, env) {
    const url = new URL(request.url)
    if (url.pathname === '/e') {
      if (request.method === 'OPTIONS') return new Response(null, { status: 204, headers: CORS })
      if (request.method !== 'POST') return new Response('method', { status: 405, headers: CORS })
      return ingest(request, env)
    }
    if (!authorized(request, env)) {
      return new Response('sign in', { status: 401, headers: { 'www-authenticate': 'Basic realm="Q Calc analytics"' } })
    }
    if (url.pathname === '/snapshot') {
      await snapshotDownloads(env)
      return Response.redirect(new URL('/', url).toString(), 303)
    }
    if (url.pathname === '/') return dashboard(env, url)
    return new Response('not found', { status: 404 })
  },

  async scheduled(_event, env, ctx) {
    ctx.waitUntil(snapshotDownloads(env))
  },
}

function utcDay(at = Date.now()) {
  return new Date(at).toISOString().slice(0, 10)
}

function addDays(day, n) {
  return utcDay(Date.parse(`${day}T00:00:00Z`) + n * 86400000)
}

function authorized(request, env) {
  if (!env.DASHBOARD_PASSWORD) return false
  const header = request.headers.get('authorization') || ''
  if (!header.startsWith('Basic ')) return false
  let decoded = ''
  try {
    decoded = atob(header.slice(6))
  } catch {
    return false
  }
  const password = decoded.slice(decoded.indexOf(':') + 1)
  // same length compare so the check doesn't leak how much matched
  const a = new TextEncoder().encode(password)
  const b = new TextEncoder().encode(env.DASHBOARD_PASSWORD)
  if (a.length !== b.length) return false
  let diff = 0
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i]
  return diff === 0
}

// ---- ingest ----

async function ingest(request, env) {
  const bad = (why) => new Response(why, { status: 400, headers: CORS })
  const text = await request.text()
  if (text.length > MAX_BODY) return bad('too big')
  let body
  try {
    body = JSON.parse(text)
  } catch {
    return bad('json')
  }
  const { id, platform, version, os, days } = body ?? {}
  if (typeof id !== 'string' || !ID_RE.test(id)) return bad('id')
  if (platform !== 'mac' && platform !== 'windows') return bad('platform')
  if (typeof version !== 'string' || !VERSION_RE.test(version)) return bad('version')
  if (!days || typeof days !== 'object') return bad('days')
  const cleanOs = typeof os === 'string' ? os.slice(0, 40) : null

  // a client's local day can run a day ahead of utc; anything older than a month is dropped
  const newest = addDays(utcDay(), 1)
  const oldest = addDays(utcDay(), -35)
  const rows = []
  for (const [day, counts] of Object.entries(days)) {
    if (!DAY_RE.test(day) || day > newest || day < oldest || !counts || typeof counts !== 'object') continue
    for (const [event, n] of Object.entries(counts)) {
      if (event.length > 32 || !EVENT_RE.test(event) || !Number.isInteger(n) || n < 1 || n > 100000) continue
      rows.push([day, event, n])
    }
  }
  if (!rows.length || rows.length > 1500) return new Response(null, { status: 204, headers: CORS })

  const sent = rows.map(([day]) => day).sort()
  const first = sent[0]
  const last = sent[sent.length - 1]
  const statements = [
    env.DB.prepare(
      `INSERT INTO installs (id, platform, version, os, first_day, last_day) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT (id) DO UPDATE SET platform = ?2, version = ?3, os = ?4,
         first_day = min(first_day, ?5), last_day = max(last_day, ?6)`,
    ).bind(id, platform, version, cleanOs, first, last),
    ...rows.map(([day, event, n]) =>
      env.DB.prepare(
        `INSERT INTO usage (day, id, event, n) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (day, id, event) DO UPDATE SET n = max(n, ?4)`,
      ).bind(day, id, event, n),
    ),
  ]
  await env.DB.batch(statements)
  return new Response(null, { status: 204, headers: CORS })
}

// ---- downloads ----

function assetPlatform(name) {
  if (/\.(zip|dmg)$/i.test(name)) return 'mac'
  if (/\.(exe|msi)$/i.test(name)) return 'windows'
  return null
}

async function snapshotDownloads(env) {
  const headers = { 'user-agent': 'qcalc-analytics', accept: 'application/vnd.github+json' }
  if (env.GITHUB_TOKEN) headers.authorization = `Bearer ${env.GITHUB_TOKEN}`
  const day = utcDay()
  const statements = []
  for (let page = 1; page <= 10; page++) {
    const res = await fetch(`https://api.github.com/repos/${env.GITHUB_REPO}/releases?per_page=100&page=${page}`, { headers })
    if (!res.ok) throw new Error(`github ${res.status}`)
    const releases = await res.json()
    for (const release of releases) {
      for (const asset of release.assets ?? []) {
        const platform = assetPlatform(asset.name)
        if (!platform) continue
        statements.push(
          env.DB.prepare(
            `INSERT INTO downloads (day, asset, platform, total) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (day, asset) DO UPDATE SET total = ?4`,
          ).bind(day, asset.name, platform, asset.download_count),
        )
      }
    }
    if (releases.length < 100) break
  }
  if (statements.length) await env.DB.batch(statements)
}

// ---- dashboard ----

async function dashboard(env, url) {
  const range = [7, 30, 90, 365].includes(Number(url.searchParams.get('days'))) ? Number(url.searchParams.get('days')) : 30
  const today = utcDay()
  const since = addDays(today, -(range - 1))
  const q = (sql, ...args) => env.DB.prepare(sql).bind(...args).all().then((r) => r.results)

  const [active, fresh, calcs, perEvent, perInstall, windows, cohort, dl, dlBefore, versions, totals, firstSnap] = await Promise.all([
    q(
      `SELECT u.day, i.platform, COUNT(DISTINCT u.id) AS n FROM usage u JOIN installs i ON i.id = u.id
       WHERE u.day >= ?1 GROUP BY u.day, i.platform`,
      since,
    ),
    q(`SELECT first_day AS day, platform, COUNT(*) AS n FROM installs WHERE first_day >= ?1 GROUP BY first_day, platform`, since),
    q(`SELECT day, SUM(n) AS n FROM usage WHERE day >= ?1 AND event LIKE 'calc.%' GROUP BY day`, since),
    // one row per install and event: enough for every per-user median, the liked list and the drop-offs
    q(
      `SELECT id, event, SUM(n) AS n, COUNT(*) AS days, max(day) AS last FROM usage WHERE day >= ?1 GROUP BY id, event`,
      since,
    ),
    q(`SELECT id, COUNT(DISTINCT day) AS days, max(day) AS last FROM usage WHERE day >= ?1 GROUP BY id`, since),
    // active yesterday (the last whole day), in the last 7 and in the last 30 days, whatever the range
    q(
      `SELECT COUNT(DISTINCT CASE WHEN day = ?1 THEN id END) AS d1, COUNT(DISTINCT CASE WHEN day >= ?2 THEN id END) AS d7,
         COUNT(DISTINCT id) AS d30 FROM usage WHERE day >= ?3`,
      addDays(today, -1),
      addDays(today, -6),
      addDays(today, -29),
    ),
    // installs first seen 8 to 37 days ago, old enough for a week to have passed: did they come back?
    q(
      `SELECT COUNT(*) AS cohort, coalesce(SUM(last_day > first_day), 0) AS back,
         coalesce(SUM(last_day >= date(first_day, '+7 days')), 0) AS week FROM installs WHERE first_day BETWEEN ?1 AND ?2`,
      addDays(today, -37),
      addDays(today, -8),
    ),
    q(`SELECT day, asset, platform, total FROM downloads WHERE day >= ?1 ORDER BY day`, since),
    // the last snapshot before the range, so the first day in it has a delta
    q(
      `SELECT d.asset, d.total FROM downloads d
       JOIN (SELECT asset, max(day) AS day FROM downloads WHERE day < ?1 GROUP BY asset) p ON p.asset = d.asset AND p.day = d.day`,
      since,
    ),
    q(
      `SELECT platform, version, COUNT(*) AS n FROM installs WHERE last_day >= ?1 GROUP BY platform, version
       ORDER BY platform, n DESC`,
      since,
    ),
    q(
      `SELECT d.platform, SUM(d.total) AS n FROM downloads d
       JOIN (SELECT asset, max(day) AS day FROM downloads GROUP BY asset) l ON l.asset = d.asset AND l.day = d.day
       GROUP BY d.platform`,
    ),
    q(`SELECT min(day) AS day FROM downloads`),
  ])

  const days = []
  for (let d = since; d <= today; d = addDays(d, 1)) days.push(d)

  // github only keeps a running total, so a day's downloads are its snapshot minus the one before
  const prev = new Map(dlBefore.map((r) => [r.asset, r.total]))
  const downloads = []
  for (const r of dl) {
    const before = prev.get(r.asset)
    // the first snapshot ever is the baseline; an asset that shows up later is a new release, counted from zero
    const delta = before == null ? (r.day > firstSnap[0].day ? r.total : 0) : r.total - before
    if (delta > 0) downloads.push({ day: r.day, platform: r.platform, n: delta })
    prev.set(r.asset, r.total)
  }

  // install ids stay in the worker; the page only gets counts
  const users = perUser(perEvent, perInstall)
  const retention = cohort[0]
  const liked = rankLiked(users.features).slice(0, 10)
  const work = needsWork({ ...users, retention }).slice(0, 8)
  const allFeatures = FEATURES.map(
    (e) => users.features.find((f) => f.event === e) ?? { event: e, label: label(e), users: 0, share: 0, repeat: 0, repeatRate: 0, dropped: 0, dropRate: 0, medianUses: null, total: 0 },
  )
  allFeatures.push(...users.features.filter((f) => !FEATURES.includes(f.event)))
  allFeatures.sort((a, b) => b.users - a.users || b.total - a.total)

  const data = {
    range, days, windows: windows[0], retention, activeUsers: users.activeUsers, spreads: users.spreads, liked, work,
    features: allFeatures, active, fresh, calcs, downloads, versions, downloadTotals: totals,
  }
  const json = JSON.stringify(data).replace(/</g, '\\u003c')
  return new Response(PAGE.replace('__DATA__', () => json), {
    headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' },
  })
}

const PAGE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Q Calc Analytics</title>
<style>
:root {
  color-scheme: light;
  --surface-0: #f4f4f2; --surface-1: #fcfcfb; --border: #e3e2de; --grid: #ecebe7;
  --text-primary: #0b0b0b; --text-secondary: #52514e; --text-muted: #8a8984;
  --series-1: #2a78d6; --series-2: #eb6834;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    color-scheme: dark;
    --surface-0: #111110; --surface-1: #1a1a19; --border: #2e2e2b; --grid: #262624;
    --text-primary: #ffffff; --text-secondary: #c3c2b7; --text-muted: #8f8e86;
    --series-1: #3987e5; --series-2: #d95926;
  }
}
:root[data-theme="dark"] {
  color-scheme: dark;
  --surface-0: #111110; --surface-1: #1a1a19; --border: #2e2e2b; --grid: #262624;
  --text-primary: #ffffff; --text-secondary: #c3c2b7; --text-muted: #8f8e86;
  --series-1: #3987e5; --series-2: #d95926;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--surface-0); color: var(--text-primary);
  font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
main { max-width: 1080px; margin: 0 auto; padding: 24px 16px 48px; }
header { display: flex; flex-wrap: wrap; gap: 12px; align-items: baseline; justify-content: space-between; margin-bottom: 20px; }
h1 { font-size: 20px; margin: 0; }
h2 { font-size: 14px; margin: 0 0 2px; }
.sub { color: var(--text-secondary); font-size: 12px; margin: 0 0 12px; }
nav { display: flex; gap: 4px; flex-wrap: wrap; }
nav a { color: var(--text-secondary); text-decoration: none; padding: 4px 10px; border: 1px solid var(--border); border-radius: 999px; font-size: 12px; }
nav a[aria-current] { color: var(--text-primary); background: var(--surface-1); font-weight: 600; }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; margin-bottom: 12px; }
.tile, .card { background: var(--surface-1); border: 1px solid var(--border); border-radius: 12px; padding: 14px 16px; }
.tile .k { color: var(--text-secondary); font-size: 12px; }
.tile .v { font-size: 26px; font-weight: 600; font-variant-numeric: tabular-nums; }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 480px), 1fr)); gap: 12px; }
.card { min-width: 0; }
.legend { display: flex; gap: 14px; font-size: 12px; color: var(--text-secondary); margin-bottom: 6px; }
.legend i { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 5px; vertical-align: -1px; }
svg { display: block; width: 100%; height: auto; overflow: visible; }
svg text { fill: var(--text-muted); font-size: 11px; font-variant-numeric: tabular-nums; }
.bars text.lbl { fill: var(--text-secondary); font-size: 12px; }
.bars text.val { fill: var(--text-primary); font-size: 12px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; font-variant-numeric: tabular-nums; }
th, td { text-align: left; padding: 5px 4px; border-bottom: 1px solid var(--grid); }
th { color: var(--text-secondary); font-weight: 500; }
td.n, th.n { text-align: right; }
.empty { color: var(--text-muted); font-size: 13px; padding: 24px 0; text-align: center; }
#tip { position: fixed; pointer-events: none; background: var(--surface-1); border: 1px solid var(--border); border-radius: 8px;
  padding: 6px 9px; font-size: 12px; box-shadow: 0 4px 14px rgba(0,0,0,.12); display: none; z-index: 9; white-space: nowrap; }
#tip b { display: block; margin-bottom: 2px; }
#tip i { display: inline-block; width: 8px; height: 8px; border-radius: 2px; margin-right: 5px; }
.foot { color: var(--text-muted); font-size: 12px; margin-top: 16px; }
.foot a { color: inherit; }
h3 { font-size: 15px; margin: 28px 0 10px; color: var(--text-secondary); font-weight: 600; }
.tile .note { color: var(--text-muted); font-size: 12px; margin-top: 2px; }
ol.rank { margin: 0; padding: 0; list-style: none; counter-reset: r; }
ol.rank li { counter-increment: r; display: grid; grid-template-columns: 22px 1fr auto; gap: 2px 8px; padding: 7px 0; border-bottom: 1px solid var(--grid); }
ol.rank li:last-child { border-bottom: 0; }
ol.rank li::before { content: counter(r); color: var(--text-muted); font-variant-numeric: tabular-nums; }
ol.rank .what { font-weight: 600; }
ol.rank .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
ol.rank .why { grid-column: 2 / 4; color: var(--text-secondary); font-size: 12px; }
.meter { grid-column: 2 / 4; height: 6px; border-radius: 3px; background: var(--grid); overflow: hidden; }
.meter i { display: block; height: 100%; background: var(--series-1); border-radius: 3px; }
.muted { color: var(--text-muted); }
.wide { grid-column: 1 / -1; }
.scroll { overflow-x: auto; }
</style>
</head>
<body>
<main>
  <header>
    <h1>Q Calc usage</h1>
    <nav id="range"></nav>
  </header>
  <div class="tiles" id="tiles"></div>
  <div class="grid">
    <section class="card"><h2>What people like</h2><p class="sub" id="liked-sub"></p><div id="liked"></div></section>
    <section class="card"><h2>What needs work</h2><p class="sub">Ranked by how many users each one touches</p><div id="work"></div></section>
  </div>
  <h3>Detail</h3>
  <div class="grid">
    <section class="card wide"><h2>Per user</h2><p class="sub">Medians across active installs in range, so one heavy user doesn't skew them. The middle half of users fall between the 25th and 75th percentiles.</p><div id="peruser" class="scroll"></div></section>
    <section class="card wide"><h2>All features</h2><p class="sub">Installs that used each feature in range, the share who came back to it on another day, and how often each used it</p><div id="features" class="scroll"></div></section>
    <section class="card"><h2>Daily active installs</h2><p class="sub">Installs that opened Q Calc that day</p><div id="dau"></div></section>
    <section class="card"><h2>Calculations per day</h2><p class="sub">Answers committed with Enter, all installs</p><div id="calcs"></div></section>
    <section class="card"><h2>New installs per day</h2><p class="sub">First day an install reported</p><div id="fresh"></div></section>
    <section class="card"><h2>Downloads per day</h2><p class="sub" id="dl-sub">Installers and Mac update zips from GitHub Releases</p><div id="dl"></div></section>
    <section class="card"><h2>Versions in use</h2><p class="sub">Installs active in range, by current version</p><div id="versions"></div></section>
  </div>
  <p class="foot">Apps report every few hours on their own calendar day, so today fills in late. Downloads update daily; <a href="/snapshot">snapshot now</a>.</p>
</main>
<div id="tip"></div>
<script>
const D = __DATA__
const PLATFORMS = [['mac', 'Mac', 'var(--series-1)'], ['windows', 'Windows', 'var(--series-2)']]
const fmt = (n) => (n == null ? '–' : (+n).toLocaleString('en-US', { maximumFractionDigits: 1 }))
const ppl = (n) => fmt(n) + (n === 1 ? ' user' : ' users')
const pct = (x) => (x == null ? '–' : Math.round(x * 100) + '%')
const short = (day) => new Date(day + 'T00:00:00Z').toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' })
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c])
const sum = (rows) => rows.reduce((a, r) => a + r.n, 0)

document.getElementById('range').innerHTML = [7, 30, 90, 365]
  .map((d) => '<a href="?days=' + d + '"' + (d === D.range ? ' aria-current="page"' : '') + '>' + (d === 365 ? '1 year' : d + ' days') + '</a>')
  .join('')

// per platform per day
function byDay(rows) {
  const m = new Map(D.days.map((d) => [d, { mac: 0, windows: 0 }]))
  for (const r of rows) if (m.has(r.day)) m.get(r.day)[r.platform] += r.n
  return m
}

const tip = document.getElementById('tip')
function showTip(e, html) {
  tip.innerHTML = html
  tip.style.display = 'block'
  const w = tip.offsetWidth
  tip.style.left = Math.min(e.clientX + 12, innerWidth - w - 8) + 'px'
  tip.style.top = e.clientY + 14 + 'px'
}
const hideTip = () => (tip.style.display = 'none')

function niceMax(v) {
  if (v <= 4) return 4
  const p = Math.pow(10, Math.floor(Math.log10(v)))
  for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p
}

// stacked columns, one per day, with a 2px gap between segments
function columns(el, series, stacked) {
  const W = 480, H = 180, L = 34, B = 20, T = 6
  const values = D.days.map((d) => series.map(([key]) => stacked.get(d)[key]))
  const total = values.reduce((a, v) => a + v.reduce((x, y) => x + y, 0), 0)
  if (!total) return (el.innerHTML = '<div class="empty">Nothing yet</div>')
  const max = niceMax(Math.max(...values.map((v) => v.reduce((x, y) => x + y, 0))))
  const n = D.days.length, slot = (W - L) / n, bw = Math.max(1, Math.min(18, slot - 2))
  const y = (v) => T + (H - B - T) * (1 - v / max)
  let s = '<svg viewBox="0 0 ' + W + ' ' + H + '" role="img">'
  for (let i = 0; i <= 4; i++) {
    const v = (max / 4) * i
    s += '<line x1="' + L + '" x2="' + W + '" y1="' + y(v) + '" y2="' + y(v) + '" stroke="var(--grid)"/>'
    s += '<text x="' + (L - 6) + '" y="' + (y(v) + 4) + '" text-anchor="end">' + fmt(v) + '</text>'
  }
  const every = Math.ceil(n / 6)
  D.days.forEach((day, i) => {
    const x = L + slot * i + (slot - bw) / 2
    let base = 0
    series.forEach(([, , color], k) => {
      const v = values[i][k]
      if (!v) return
      const top = y(base + v), bottom = y(base) - (base ? 2 : 0)
      const h = Math.max(1, bottom - top), r = Math.min(4, bw / 2, h)
      // rounded only on the data end
      s += '<path fill="' + color + '" d="M' + x + ',' + bottom + 'V' + (top + r) + 'q0,-' + r + ' ' + r + ',-' + r +
        'H' + (x + bw - r) + 'q' + r + ',0 ' + r + ',' + r + 'V' + bottom + 'Z"/>'
      base += v
    })
    if (i % every === 0) s += '<text x="' + (x + bw / 2) + '" y="' + (H - 4) + '" text-anchor="middle">' + short(day) + '</text>'
    s += '<rect class="hit" data-i="' + i + '" x="' + (L + slot * i) + '" y="0" width="' + slot + '" height="' + (H - B) + '" fill="transparent"/>'
  })
  s += '</svg>'
  const legend = series.length > 1
    ? '<div class="legend">' + series.map(([, name, c]) => '<span><i style="background:' + c + '"></i>' + name + '</span>').join('') + '</div>'
    : ''
  el.innerHTML = legend + s
  el.querySelectorAll('.hit').forEach((r) => {
    const i = +r.dataset.i
    r.addEventListener('mousemove', (e) => showTip(e, '<b>' + short(D.days[i]) + '</b>' +
      series.map(([, name, c], k) => (series.length > 1 ? '<i style="background:' + c + '"></i>' + name + ' ' : '') + fmt(values[i][k])).join('<br>')))
    r.addEventListener('mouseleave', hideTip)
  })
}

// ---- headline numbers
const W = D.windows, R = D.retention, C = D.spreads.calculations
const rangeName = D.range === 365 ? 'the last year' : 'the last ' + D.range + ' days'
const tiles = [
  ['Active yesterday', fmt(W.d1), 'installs that used Q Calc'],
  ['Active, last 7 days', fmt(W.d7), 'installs'],
  ['Active, last 30 days', fmt(W.d30), 'installs'],
  ['Median calculations per user', fmt(C.median), C.users ? 'middle half ' + fmt(C.p25) + '–' + fmt(C.p75) + ' · ' + fmt(C.users) + ' users, ' + rangeName : 'no users yet'],
  ['Still using a week later', R.cohort ? pct(R.week / R.cohort) : '–', R.cohort ? fmt(R.week) + ' of ' + fmt(R.cohort) + ' installs first seen 8–37 days ago' : 'no installs old enough yet'],
  ['Came back another day', R.cohort ? pct(R.back / R.cohort) : '–', R.cohort ? 'of the same ' + fmt(R.cohort) + ' installs' : ''],
]
document.getElementById('tiles').innerHTML = tiles
  .map(([k, v, note]) => '<div class="tile"><div class="k">' + k + '</div><div class="v">' + v + '</div><div class="note">' + esc(note) + '</div></div>').join('')

// ---- what people like
document.getElementById('liked-sub').textContent = 'Features ranked by how many of the ' + fmt(D.activeUsers) +
  ' active installs used them in ' + rangeName + ', then by how many came back to them on another day'
;(() => {
  const el = document.getElementById('liked')
  if (!D.liked.length) return (el.innerHTML = '<div class="empty">Nothing yet</div>')
  el.innerHTML = '<ol class="rank">' + D.liked.map((f) =>
    '<li><span class="what">' + esc(f.label) + '</span><span class="num">' + ppl(f.users) + ' <span class="muted">· ' + pct(f.share) + '</span></span>' +
    '<span class="meter"><i style="width:' + Math.max(2, f.share * 100) + '%"></i></span>' +
    '<span class="why">' + pct(f.repeatRate) + ' came back to it on another day · median ' + fmt(f.medianUses) + ' uses each</span></li>').join('') + '</ol>'
})()

// ---- what needs work
;(() => {
  const el = document.getElementById('work')
  if (!D.work.length) return (el.innerHTML = '<div class="empty">Nothing stands out yet</div>')
  el.innerHTML = '<ol class="rank">' + D.work.map((w) =>
    '<li><span class="what">' + esc(w.label) + '</span><span class="num">' + (w.users ? ppl(w.users) : '') + '</span>' +
    '<span class="why">' + esc(w.detail) + '</span></li>').join('') + '</ol>'
})()

// ---- per user
;(() => {
  const rows = [
    ['Calculations', D.spreads.calculations],
    ['Times the overlay was opened', D.spreads.opens],
    ['Days active', D.spreads.activeDays],
    ['Different features used', D.spreads.features],
  ]
  document.getElementById('peruser').innerHTML = '<table><thead><tr><th>Per user, ' + rangeName + '</th><th class="n">Median</th><th class="n">25th pct</th><th class="n">75th pct</th><th class="n">Users</th></tr></thead><tbody>' +
    rows.map(([k, s]) => '<tr><td>' + k + '</td><td class="n"><b>' + fmt(s.median) + '</b></td><td class="n">' + fmt(s.p25) + '</td><td class="n">' + fmt(s.p75) +
      '</td><td class="n">' + fmt(s.users) + '</td></tr>').join('') + '</tbody></table>'
})()

// ---- all features
document.getElementById('features').innerHTML = '<table><thead><tr><th>Feature</th><th class="n">Users</th><th class="n">Came back</th><th class="n">Tried once, dropped</th><th class="n">Median uses per user</th><th class="n">All uses</th></tr></thead><tbody>' +
  D.features.map((f) => '<tr><td>' + esc(f.label) + '</td><td class="n">' + fmt(f.users) + '</td><td class="n">' + (f.users ? pct(f.repeatRate) : '–') +
    '</td><td class="n">' + (f.users ? fmt(f.dropped) : '–') + '</td><td class="n">' + fmt(f.medianUses) + '</td><td class="n">' + fmt(f.total) + '</td></tr>').join('') +
  '</tbody></table>'

// ---- charts
columns(document.getElementById('dau'), PLATFORMS, byDay(D.active))
columns(document.getElementById('dl'), PLATFORMS, byDay(D.downloads))
columns(document.getElementById('fresh'), PLATFORMS, byDay(D.fresh))
const calcDay = new Map(D.days.map((d) => [d, { all: 0 }]))
for (const r of D.calcs) if (calcDay.has(r.day)) calcDay.get(r.day).all += r.n
columns(document.getElementById('calcs'), [['all', 'Calculations', 'var(--series-1)']], calcDay)
document.getElementById('dl-sub').textContent = fmt(sum(D.downloads)) + ' in range, ' + fmt(sum(D.downloadTotals)) +
  ' all time. Installers and Mac update zips from GitHub Releases'

// ---- versions
;(() => {
  const el = document.getElementById('versions')
  if (!D.versions.length) return (el.innerHTML = '<div class="empty">Nothing yet</div>')
  el.innerHTML = '<table><thead><tr><th>Platform</th><th>Version</th><th class="n">Installs</th></tr></thead><tbody>' +
    D.versions.map((r) => '<tr><td>' + (r.platform === 'mac' ? 'Mac' : 'Windows') + '</td><td>' + esc(r.version) +
      '</td><td class="n">' + fmt(r.n) + '</td></tr>').join('') + '</tbody></table>'
})()
</script>
</body>
</html>`
