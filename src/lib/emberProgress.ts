import type { Score } from './emberTypes'

// ember & frost's saved progress: the best of every finished level, by level id, and the mute switch.
// pure parse/merge here; load/save wrap localStorage since the mac app's store can reject writes.
//
// version 2 (ember & frost 2): levels are long, multi-stage and have checkpoints, so a finish also keeps the
// fewest deaths. every reworked level has a new id; only the ids in CARRIED are the same map as before, so
// those are the only bests a version 1 file brings along: a time earned on an old, easier map never shows
// against a new one. a file from a newer version than this is read as far as it makes sense

export const PROGRESS_VERSION = 2

// levels whose map (and so whose bests) version 1 and version 2 share
const CARRIED: ReadonlySet<string> = new Set(['first-light'])

export type LevelBest = {
  completed: boolean
  stars: number // best stars, 0..3
  secs: number | null // best time over all stages; null until completed
  gems: number // most gems (both colours, all stages) in one finish
  deaths: number | null // fewest deaths in one finish; null when not known (a finish saved by version 1)
}

export type EmberProgress = {
  v: number
  levels: Record<string, LevelBest>
  muted: boolean
}

const KEY = 'qcalc-ember'
const MAX_STARS = 3
// a stored file is somebody else's text: don't let a huge one in
const MAX_LEVELS = 200
const MAX_ID = 64

export function emptyProgress(): EmberProgress {
  return { v: PROGRESS_VERSION, levels: {}, muted: false }
}

const count = (v: unknown, max = Infinity): number =>
  typeof v === 'number' && Number.isFinite(v) && v > 0 ? Math.min(max, Math.floor(v)) : 0

const tally = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? Math.floor(v) : null)

// whatever was stored, made safe and brought up to this version: unknown fields dropped, bad numbers zeroed,
// a level that was never finished but claims stars or a time cut back to nothing, and version 1's bests for
// maps that no longer exist left behind
export function sanitizeProgress(raw: unknown): EmberProgress {
  const out = emptyProgress()
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return out
  const o = raw as Record<string, unknown>
  out.muted = o.muted === true
  // version 1 had no version field
  const from = typeof o.v === 'number' && Number.isFinite(o.v) && o.v >= 1 ? Math.floor(o.v) : 1
  const levels = o.levels
  if (!levels || typeof levels !== 'object' || Array.isArray(levels)) return out
  let n = 0
  for (const [id, v] of Object.entries(levels as Record<string, unknown>)) {
    // '__proto__' as a key would become the table's prototype, not a level
    if (!id || id.length > MAX_ID || id === '__proto__' || !v || typeof v !== 'object') continue
    if (from < 2 && !CARRIED.has(id)) continue
    const l = v as Record<string, unknown>
    if (l.completed !== true) continue
    if (++n > MAX_LEVELS) break
    const secs = typeof l.secs === 'number' && Number.isFinite(l.secs) && l.secs > 0 ? l.secs : null
    out.levels[id] = { completed: true, stars: count(l.stars, MAX_STARS), secs, gems: count(l.gems), deaths: from < 2 ? null : tally(l.deaths) }
  }
  return out
}

export function parseProgress(text: string | null): EmberProgress {
  if (!text) return emptyProgress()
  try {
    return sanitizeProgress(JSON.parse(text))
  } catch {
    return emptyProgress()
  }
}

// a finished level: keep the best of each field separately (the fastest run needn't be the gem run, nor the
// cleanest). deaths: how many times the pair went back to a checkpoint on the way
export function recordResult(progress: EmberProgress, levelId: string, score: Score, deaths?: number): EmberProgress {
  const old = progress.levels[levelId]
  const secs = Number.isFinite(score.secs) && score.secs > 0 ? score.secs : null
  const gems = count(score.gems[0]) + count(score.gems[1])
  const died = tally(deaths)
  const best: LevelBest = {
    completed: true,
    stars: Math.max(old?.stars ?? 0, count(score.stars, MAX_STARS)),
    secs: old?.secs != null && (secs == null || old.secs <= secs) ? old.secs : secs,
    gems: Math.max(old?.gems ?? 0, gems),
    deaths: old?.deaths != null && (died == null || old.deaths <= died) ? old.deaths : died,
  }
  return { ...progress, v: PROGRESS_VERSION, levels: { ...progress.levels, [levelId]: best } }
}

// what this finish beat, for the win card. nothing is "new" on a first finish: there was nothing to beat
export function improved(old: LevelBest | undefined, score: Score, deaths?: number): { secs: boolean; stars: boolean; gems: boolean; deaths: boolean } {
  if (!old?.completed) return { secs: false, stars: false, gems: false, deaths: false }
  const died = tally(deaths)
  return {
    secs: old.secs != null && score.secs > 0 && score.secs < old.secs,
    stars: count(score.stars, MAX_STARS) > old.stars,
    gems: count(score.gems[0]) + count(score.gems[1]) > old.gems,
    deaths: old.deaths != null && died != null && died < old.deaths,
  }
}

// where the select screen's highlight starts: the first level not finished yet, or the first of all once
// every one is. nothing is ever locked
export function firstUnfinished(progress: EmberProgress, levels: ReadonlyArray<{ id: string }>): number {
  const i = levels.findIndex((l) => !progress.levels[l.id]?.completed)
  return i < 0 ? 0 : i
}

// how many of the levels that exist now are finished: a saved best for a level that has since gone doesn't count
export function finishedCount(progress: EmberProgress, levels: ReadonlyArray<{ id: string }>): number {
  let n = 0
  for (const l of levels) if (progress.levels[l.id]?.completed) n++
  return n
}

export function loadProgress(): EmberProgress {
  try {
    return parseProgress(localStorage.getItem(KEY))
  } catch {
    return emptyProgress()
  }
}

export function saveProgress(progress: EmberProgress): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(progress))
  } catch {
    // the mac app's non-persistent web store can reject writes; this run still remembers
  }
}
