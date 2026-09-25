// clock times and calendar dates in the local time zone; everything relative to now takes `now` in

import type { PhraseAnswer } from './phrases'

type Day = { y: number; m: number; d: number }
type Span = { minutes: number; days: number; months: number }

const WEEKDAYS = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday']
const SHORT_WEEKDAYS: Record<string, number> = { sun: 0, mon: 1, tue: 2, tues: 2, wed: 3, thu: 4, thur: 4, thurs: 4, fri: 5, sat: 6 }
const MONTHS = ['january', 'february', 'march', 'april', 'may', 'june', 'july', 'august', 'september', 'october', 'november', 'december']
const SHORT_MONTHS: Record<string, number> = { jan: 0, feb: 1, mar: 2, apr: 3, jun: 5, jul: 6, aug: 7, sep: 8, sept: 8, oct: 9, nov: 10, dec: 11 }
const WD = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
const MON = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

function monthIndex(w: string): number | null {
  const i = MONTHS.indexOf(w)
  if (i >= 0) return i
  return SHORT_MONTHS[w] ?? null
}

function daysInMonth(y: number, m: number): number {
  return new Date(y, m + 1, 0).getDate()
}

function dayOf(date: Date): Day {
  return { y: date.getFullYear(), m: date.getMonth(), d: date.getDate() }
}

function toDate(day: Day): Date {
  return new Date(day.y, day.m, day.d)
}

function addDays(day: Day, k: number): Day {
  return dayOf(new Date(day.y, day.m, day.d + k))
}

// jan 31 + 1 month is the last day of february
function addMonths(day: Day, k: number): Day {
  const total = day.y * 12 + day.m + k
  const y = Math.floor(total / 12)
  const m = total - y * 12
  return { y, m, d: Math.min(day.d, daysInMonth(y, m)) }
}

// whole calendar days, immune to daylight saving
function dayDiff(a: Day, b: Day): number {
  return Math.round((Date.UTC(b.y, b.m, b.d) - Date.UTC(a.y, a.m, a.d)) / 86_400_000)
}

function compareDays(a: Day, b: Day): number {
  return dayDiff(b, a)
}

export function formatDate(day: Day): string {
  const wd = toDate(day).getDay()
  return `${WD[wd]} ${day.d} ${MON[day.m]} ${day.y}`
}

export function formatClock(minutes: number): string {
  const t = ((Math.round(minutes) % 1440) + 1440) % 1440
  const h = Math.floor(t / 60)
  const m = t % 60
  const h12 = h % 12 === 0 ? 12 : h % 12
  return `${h12}:${String(m).padStart(2, '0')} ${h < 12 ? 'am' : 'pm'}`
}

export function formatDuration(minutes: number): string {
  const h = Math.floor(minutes / 60)
  const m = minutes - h * 60
  if (h && m) return `${h} hr ${m} min`
  return h ? `${h} hr` : `${m} min`
}

function plural(n: number, word: string): string {
  return `${n} ${word}${Math.abs(n) === 1 ? '' : 's'}`
}

// `3:45pm`, `9 am`, `noon`, or an unmistakable 24 hour time like `15:30`
export function parseClock(s: string): number | null {
  const t = s.trim()
  if (t === 'noon' || t === 'midday') return 720
  if (t === 'midnight') return 0
  const ampm = t.match(/^(\d{1,2})(?::(\d{2}))?\s*([ap])\.?m\.?$/)
  if (ampm) {
    const h = Number(ampm[1])
    const m = ampm[2] ? Number(ampm[2]) : 0
    if (h < 1 || h > 12 || m > 59) return null
    return ((h % 12) + (ampm[3] === 'p' ? 12 : 0)) * 60 + m
  }
  // `3:45` could be a duration or either half of the day, so only 13:00 to 23:59 reads as a time
  const h24 = t.match(/^(\d{2}):(\d{2})$/)
  if (h24) {
    const h = Number(h24[1])
    const m = Number(h24[2])
    return h >= 13 && h <= 23 && m <= 59 ? h * 60 + m : null
  }
  return null
}

const SPAN_UNITS: [RegExp, keyof Span, number][] = [
  [/^(?:h|hr|hrs|hour|hours)$/, 'minutes', 60],
  [/^(?:min|mins|minute|minutes)$/, 'minutes', 1],
  [/^(?:day|days)$/, 'days', 1],
  [/^(?:wk|wks|week|weeks)$/, 'days', 7],
  [/^(?:month|months)$/, 'months', 1],
  [/^(?:yr|yrs|year|years)$/, 'months', 12],
]

// `4 hr 10 min`, `3 weeks`, `1 year and 2 months`; clock and calendar units never mix
export function parseSpan(s: string): Span | null {
  const parts = s.trim().split(/\s*(?:,|\band\b)\s*|\s+(?=\d)/).filter(Boolean)
  if (!parts.length) return null
  const span: Span = { minutes: 0, days: 0, months: 0 }
  const seen = new Set<string>()
  for (const part of parts) {
    const m = part.match(/^(\d+(?:\.\d+)?|a|an|one)\s*([a-z]+)$/)
    if (!m) return null
    const qty = /^\d/.test(m[1]) ? Number(m[1]) : 1
    const unit = SPAN_UNITS.find(([re]) => re.test(m[2]))
    if (!unit || seen.has(m[2])) return null
    seen.add(m[2])
    const add = qty * unit[2]
    if (!Number.isInteger(add)) return null
    span[unit[1]] += add
  }
  if (span.minutes && (span.days || span.months)) return null
  return span
}

type Roll = 'year' | 'future' | 'past'

// `today`, `next friday`, `monday`, `dec 25`, `25 december 2026`
export function parseDay(s: string, today: Day, roll: Roll = 'year'): Day | null {
  const t = s.trim()
  if (t === 'today') return today
  if (t === 'tomorrow') return addDays(today, 1)
  if (t === 'yesterday') return addDays(today, -1)
  const wd = t.match(/^(?:(next|last)\s+)?([a-z]+)$/)
  if (wd) {
    const full = WEEKDAYS.indexOf(wd[2])
    const idx = full >= 0 ? full : wd[1] ? SHORT_WEEKDAYS[wd[2]] : undefined
    if (idx != null) {
      const cur = toDate(today).getDay()
      if (wd[1] === 'last') return addDays(today, -(((cur - idx + 6) % 7) + 1))
      if (wd[1] === 'next') return addDays(today, ((idx - cur + 6) % 7) + 1)
      return addDays(today, (idx - cur + 7) % 7)
    }
  }
  const md = t.match(/^([a-z]+)\.?\s+(\d{1,2})(?:st|nd|rd|th)?(?:,?\s+(\d{4}))?$/)
  const dm = t.match(/^(\d{1,2})(?:st|nd|rd|th)?\s+(?:of\s+)?([a-z]+)\.?(?:,?\s+(\d{4}))?$/)
  const parts = md ? { mon: md[1], d: md[2], y: md[3] } : dm ? { mon: dm[2], d: dm[1], y: dm[3] } : null
  if (!parts) return null
  const m = monthIndex(parts.mon)
  if (m == null) return null
  const d = Number(parts.d)
  if (d < 1 || d > 31) return null
  const explicitYear = parts.y ? Number(parts.y) : null
  let y = explicitYear ?? today.y
  if (d < 1 || d > daysInMonth(y, m)) {
    // feb 29 without a year looks for the next leap year only when rolling
    if (explicitYear != null || roll === 'year') return null
  }
  if (explicitYear == null && roll !== 'year') {
    const step = roll === 'future' ? 1 : -1
    for (let i = 0; i < 8; i++) {
      const cand = { y, m, d }
      const ok = d <= daysInMonth(y, m)
      if (ok && (roll === 'future' ? compareDays(cand, today) >= 0 : compareDays(cand, today) <= 0)) return cand
      y += step
    }
    return null
  }
  return { y, m, d }
}

function shiftDay(day: Day, span: Span, sign: 1 | -1): Day | null {
  if (span.minutes) return null
  return addDays(addMonths(day, sign * span.months), sign * span.days)
}

function shiftNow(now: Date, span: Span, sign: 1 | -1): string | null {
  const today = dayOf(now)
  if (!span.minutes) {
    const day = shiftDay(today, span, sign)
    return day ? formatDate(day) : null
  }
  const at = new Date(now.getFullYear(), now.getMonth(), now.getDate(), now.getHours(), now.getMinutes() + sign * span.minutes)
  const clock = formatClock(at.getHours() * 60 + at.getMinutes())
  const day = dayOf(at)
  return compareDays(day, today) === 0 ? clock : `${formatDate(day)}, ${clock}`
}

const PLUS = String.raw`\s*([+-])\s*`

export function answerTimePhrase(s: string, now: Date): PhraseAnswer | null {
  const today = dayOf(now)

  // clock time on its own, or plus/minus a clock span
  const clock = parseClock(s)
  if (clock != null) return { display: formatClock(clock) }
  const arith = s.match(new RegExp(`^(.+?)${PLUS}(.+)$`))
  if (arith) {
    const t = parseClock(arith[1])
    const span = parseSpan(arith[3])
    if (t != null && span && span.minutes && !span.days && !span.months) {
      return { display: formatClock(t + (arith[2] === '+' ? span.minutes : -span.minutes)) }
    }
  }

  // the length of time between two clock times; overnight wraps to the next day
  const range = s.match(/^(?:from\s+)?(.+?)\s+(?:to|until|till)\s+(.+)$/) ?? s.match(/^between\s+(.+?)\s+and\s+(.+)$/)
  if (range) {
    const a = parseClock(range[1])
    const b = parseClock(range[2])
    if (a != null && b != null && a !== b) {
      return { display: formatDuration((b - a + 1440) % 1440) }
    }
  }

  const until = s.match(/^(?:(?:how long|time)\s+)?(?:until|till)\s+(.+)$/)
  if (until) {
    const t = parseClock(until[1])
    if (t != null) {
      const cur = now.getHours() * 60 + now.getMinutes() + now.getSeconds() / 60
      const left = Math.round((t - cur + 1440) % 1440)
      return left > 0 && left < 1440 ? { display: formatDuration(left) } : null
    }
  }

  // counting days
  const count = s.match(/^(?:how many\s+)?days?\s+(until|till|to|since)\s+(.+)$/)
  if (count) {
    const future = count[1] !== 'since'
    const day = parseDay(count[2], today, future ? 'future' : 'past')
    if (!day) return null
    const n = future ? dayDiff(today, day) : dayDiff(day, today)
    return n >= 0 ? { display: plural(n, 'day'), n } : null
  }
  const between =
    s.match(/^(?:how many\s+)?days?\s+between\s+(.+?)\s+and\s+(.+)$/) ?? s.match(/^(?:how many\s+)?days?\s+from\s+(.+?)\s+(?:to|until)\s+(.+)$/)
  if (between) {
    const a = parseDay(between[1], today)
    const b = parseDay(between[2], today)
    if (!a || !b) return null
    const n = dayDiff(a, b)
    const signed = /^(?:how many\s+)?days?\s+from/.test(s)
    if (signed && n < 0) return null
    return { display: plural(Math.abs(n), 'day'), n: Math.abs(n) }
  }

  // relative to now
  const ago = s.match(/^(.+?)\s+ago$/)
  if (ago) {
    const span = parseSpan(ago[1])
    return span ? wrap(shiftNow(now, span, -1)) : null
  }
  const fromNow = s.match(/^(.+?)\s+from\s+now$/) ?? s.match(/^in\s+(.+)$/)
  if (fromNow) {
    const span = parseSpan(fromNow[1])
    return span ? wrap(shiftNow(now, span, 1)) : null
  }
  const around = s.match(/^(.+?)\s+(from|after|before)\s+(.+)$/)
  if (around) {
    const span = parseSpan(around[1])
    const day = parseDay(around[3], today)
    if (span && day) {
      const out = shiftDay(day, span, around[2] === 'before' ? -1 : 1)
      return out ? { display: formatDate(out) } : null
    }
  }

  // a date on its own, or plus/minus a calendar span
  const bare = parseDay(s, today)
  if (bare) return { display: formatDate(bare) }
  if (arith) {
    const day = parseDay(arith[1], today)
    const span = parseSpan(arith[3])
    if (day && span) {
      const out = shiftDay(day, span, arith[2] === '+' ? 1 : -1)
      return out ? { display: formatDate(out) } : null
    }
  }
  return null
}

function wrap(display: string | null): PhraseAnswer | null {
  return display ? { display } : null
}
