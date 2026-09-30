import { isCalculusInput } from '../engine/calculus'
import { looksLikeMatrix } from '../engine/matrix'
import { nativeHandler, nativeWindow, type NativeWindow } from './bridge'
import { isWindowsHost } from './platform'

// anonymous usage counts: what kind of thing was done, never what was typed. counts pile up per day on
// the host's disk and go out at most every few hours in one small post; the worker lives in analytics/
// the deployed worker's /e endpoint; empty keeps it all off
export const ANALYTICS_URL = 'https://qcalc-analytics.maxconine.workers.dev/e'

declare const __APP_VERSION__: string
export const APP_VERSION = typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : '0.0.0'

// the only names that ever leave the app; anything else is dropped
export const USAGE_EVENTS = [
  'launch',
  'open',
  'update',
  'calc.arith',
  'calc.units',
  'calc.solve',
  'calc.system',
  'calc.graph',
  'calc.function',
  'calc.assign',
  'calc.calculus',
  'calc.matrix',
  'calc.words',
  'calc.chain',
  'copy.answer',
  'copy.line',
  'history.insert',
  'history.undo',
  'form.tab',
  'prefix.step',
  'periodic',
  'help',
  'settings.open',
  'error.blank',
  'error.unit',
] as const
export type UsageEvent = (typeof USAGE_EVENTS)[number]
const KNOWN = new Set<string>(USAGE_EVENTS)

export type DayCounts = Record<string, number>
export type Stash = { id: string; version: string; sentAt: number; days: Record<string, DayCounts> }

const SEND_EVERY_MS = 6 * 60 * 60 * 1000
const KEEP_DAYS = 30
const DAY_RE = /^\d{4}-\d{2}-\d{2}$/
const ID_RE = /^[0-9a-f]{32}$/

export function localDay(at = Date.now()): string {
  const d = new Date(at)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

export function newInstallId(): string {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

// what the host kept is trusted only field by field
export function sanitizeStash(raw: unknown): Stash | null {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Partial<Stash>
  if (typeof r.id !== 'string' || !ID_RE.test(r.id)) return null
  const days: Record<string, DayCounts> = {}
  for (const [day, counts] of Object.entries(r.days && typeof r.days === 'object' ? r.days : {})) {
    if (!DAY_RE.test(day) || !counts || typeof counts !== 'object') continue
    const clean: DayCounts = {}
    for (const [name, n] of Object.entries(counts)) {
      if (KNOWN.has(name) && Number.isInteger(n) && n > 0) clean[name] = n
    }
    if (Object.keys(clean).length) days[day] = clean
  }
  return {
    id: r.id,
    version: typeof r.version === 'string' ? r.version : '',
    sentAt: typeof r.sentAt === 'number' && Number.isFinite(r.sentAt) ? r.sentAt : 0,
    days: trimDays(days),
  }
}

function trimDays(days: Record<string, DayCounts>): Record<string, DayCounts> {
  const keep = Object.keys(days).sort().slice(-KEEP_DAYS)
  return Object.fromEntries(keep.map((d) => [d, days[d]!]))
}

export function bump(stash: Stash, event: UsageEvent, day: string): Stash {
  const counts = stash.days[day] ?? {}
  return { ...stash, days: trimDays({ ...stash.days, [day]: { ...counts, [event]: (counts[event] ?? 0) + 1 } }) }
}

export function dueToSend(stash: Stash, now: number): boolean {
  return Object.keys(stash.days).length > 0 && now - stash.sentAt >= SEND_EVERY_MS
}

// the server replaces a day's counts, so today stays until it is over and every past day was sent final
export function afterSend(stash: Stash, sent: Record<string, DayCounts>, today: string, now: number): Stash {
  const days = { ...stash.days }
  for (const day of Object.keys(sent)) if (day < today) delete days[day]
  return { ...stash, days, sentAt: now }
}

export type UsagePayload = {
  id: string
  platform: 'mac' | 'windows'
  version: string
  os?: string
  days: Record<string, DayCounts>
}

export function usagePayload(stash: Stash, platform: UsagePayload['platform'], os?: string): UsagePayload {
  return { id: stash.id, platform, version: APP_VERSION, ...(os ? { os } : {}), days: stash.days }
}

export type CalcFacts = {
  expr: string
  system?: boolean
  graph?: boolean
  fn?: boolean
  solve?: boolean
  assign?: boolean
  unit?: boolean
  chained?: boolean
  native?: boolean
}

export function calcKind(f: CalcFacts): UsageEvent {
  if (f.system) return 'calc.system'
  if (f.graph) return 'calc.graph'
  if (f.fn) return 'calc.function'
  if (f.solve) return 'calc.solve'
  if (f.assign) return 'calc.assign'
  const expr = f.expr.trim()
  if (isCalculusInput(expr)) return 'calc.calculus'
  if (looksLikeMatrix(expr)) return 'calc.matrix'
  if (f.unit) return 'calc.units'
  if (f.native) return 'calc.words'
  if (f.chained) return 'calc.chain'
  return 'calc.arith'
}

// ---- the running app ----

type UsageWindow = NativeWindow & { __QCALC_ANALYTICS?: unknown; __QCALC_OS?: string }

let stash: Stash | null = null
let enabled = false
let sending = false
let saveTimer = 0

// only the mac and windows apps report; the host sets __QCALC_ANALYTICS (null before the first save)
function hosted(): boolean {
  const w = nativeWindow() as UsageWindow | undefined
  return Boolean(ANALYTICS_URL && w && nativeHandler() && '__QCALC_ANALYTICS' in w)
}

function persist(): void {
  window.clearTimeout(saveTimer)
  saveTimer = window.setTimeout(() => nativeHandler()?.postMessage({ type: 'analytics', stash }), 2000)
}

async function maybeSend(): Promise<void> {
  if (!enabled || !stash || sending || !dueToSend(stash, Date.now())) return
  sending = true
  const sent = stash.days
  const body = usagePayload(stash, isWindowsHost() ? 'windows' : 'mac', (nativeWindow() as UsageWindow).__QCALC_OS)
  try {
    // text/plain keeps it a simple request, so there's no preflight
    const res = await fetch(ANALYTICS_URL, { method: 'POST', body: JSON.stringify(body), headers: { 'content-type': 'text/plain' } })
    if (res.ok && stash && enabled) {
      stash = afterSend(stash, sent, localDay(), Date.now())
      persist()
    }
  } catch {
    // offline: the counts wait for the next try
  } finally {
    sending = false
  }
}

export function setUsageSharing(on: boolean): void {
  if (!hosted()) return
  if (on === enabled && stash) return
  enabled = on
  if (!on) {
    // turning it off forgets everything, the install id too
    stash = null
    ;(nativeWindow() as UsageWindow).__QCALC_ANALYTICS = null
    window.clearTimeout(saveTimer)
    nativeHandler()?.postMessage({ type: 'analytics', stash: null })
    return
  }
  const kept = sanitizeStash((nativeWindow() as UsageWindow).__QCALC_ANALYTICS)
  stash = kept ?? { id: newInstallId(), version: APP_VERSION, sentAt: 0, days: {} }
  if (kept && kept.version && kept.version !== APP_VERSION) stash = bump(stash, 'update', localDay())
  stash = bump({ ...stash, version: APP_VERSION }, 'launch', localDay())
  persist()
  void maybeSend()
}

export function track(event: UsageEvent): void {
  if (!enabled || !stash) return
  stash = bump(stash, event, localDay())
  persist()
  void maybeSend()
}
